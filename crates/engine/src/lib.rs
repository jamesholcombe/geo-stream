//! Pure, transport-agnostic geospatial stream engine: zone registration, single-update processing.

mod rules;

use spatial::NaiveSpatialIndex;
use state::sort_events_deterministic;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use thiserror::Error;

pub use rules::{default_rules, CatalogRule, RadiusRule, RuleContext, SpatialRule, ZoneRule};

pub use spatial::{Circle, SpatialError, SpatialIndex, Zone};
pub use state::{EntityState, Event, ZoneDwell};

/// Single location observation for an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct PointUpdate {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Unix epoch time in milliseconds (observation time for this sample).
    pub t_ms: u64,
}

/// Engine API: zone registration and single-update processing.
pub trait GeoEngine {
    fn register_zone(&mut self, zone: Zone) -> Result<(), EngineError>;
    fn register_catalog_region(&mut self, region: Zone) -> Result<(), EngineError>;
    fn register_circle(&mut self, circle: Circle) -> Result<(), EngineError>;

    /// Process one location update. Returns an error if the update's timestamp is strictly less
    /// than the last seen timestamp for the entity (monotonicity violation).
    /// For multiple updates with cross-update event ordering, use [`Engine::process_batch`].
    fn process_event(&mut self, update: PointUpdate) -> Result<Vec<Event>, EngineError>;
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Spatial(#[from] spatial::SpatialError),
    #[error(
        "monotonicity violation for entity {entity_id}: incoming t_ms {incoming_t_ms} < last seen {last_t_ms}"
    )]
    MonotonicityViolation {
        entity_id: String,
        last_t_ms: u64,
        incoming_t_ms: u64,
    },
}

/// In-memory engine: R-tree-accelerated polygon queries + per-entity membership state.
pub struct Engine {
    spatial: NaiveSpatialIndex,
    /// Per zone id: minimum inside/outside dwell before enter/exit events.
    zone_dwell: HashMap<String, ZoneDwell>,
    entities: HashMap<String, EntityState>,
    /// Reused between membership tiers to avoid cloning [`EntityState`] sets each update.
    membership_scratch: BTreeSet<String>,
    rules: Vec<Box<dyn SpatialRule>>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("spatial", &self.spatial)
            .field("zone_dwell", &self.zone_dwell.len())
            .field("entities", &self.entities)
            .field("rules", &self.rules.len())
            .finish()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            spatial: NaiveSpatialIndex::default(),
            zone_dwell: HashMap::new(),
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules: rules::default_rules(),
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rules(rules: Vec<Box<dyn SpatialRule>>) -> Self {
        Self {
            spatial: NaiveSpatialIndex::default(),
            zone_dwell: HashMap::new(),
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules,
        }
    }

    /// Register a zone with dwell / exit-debounce parameters (see [`ZoneDwell`]).
    pub fn register_zone_with_dwell(
        &mut self,
        zone: Zone,
        dwell: ZoneDwell,
    ) -> Result<(), EngineError> {
        let id = zone.id.clone();
        self.spatial.try_push_zone(zone)?;
        self.zone_dwell.insert(id, dwell);
        Ok(())
    }

    /// Sort updates by entity id, run `GeoEngine::process_event` for each, then
    /// `state::sort_events_deterministic` on the combined output.
    ///
    /// Monotonicity violations are **skipped and collected**: processing continues for valid
    /// updates. Returns `(events, errors)` where `errors` contains one entry per violated update.
    pub fn process_batch(&mut self, mut batch: Vec<PointUpdate>) -> (Vec<Event>, Vec<EngineError>) {
        batch.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.t_ms.cmp(&b.t_ms)));
        let mut events = Vec::new();
        let mut errors = Vec::new();
        for u in batch {
            match self.process_event(u) {
                Ok(evs) => events.extend(evs),
                Err(e) => errors.push(e),
            }
        }
        sort_events_deterministic(&mut events);
        (events, errors)
    }
}

impl GeoEngine for Engine {
    fn register_zone(&mut self, zone: Zone) -> Result<(), EngineError> {
        let id = zone.id.clone();
        self.spatial.try_push_zone(zone)?;
        self.zone_dwell.insert(id, ZoneDwell::default());
        Ok(())
    }

    fn register_catalog_region(&mut self, region: Zone) -> Result<(), EngineError> {
        self.spatial.try_push_catalog_region(region)?;
        Ok(())
    }

    fn register_circle(&mut self, circle: Circle) -> Result<(), EngineError> {
        self.spatial.try_push_circle(circle)?;
        Ok(())
    }

    fn process_event(&mut self, update: PointUpdate) -> Result<Vec<Event>, EngineError> {
        let mut events = Vec::new();
        let p = (update.x, update.y);
        let t_ms = update.t_ms;
        let entity_id = update.id.as_str();

        let Engine {
            spatial,
            zone_dwell,
            entities,
            membership_scratch,
            rules,
        } = self;

        let st = entities.entry(update.id.clone()).or_default();

        // Enforce monotonicity: reject strictly backwards timestamps.
        if let Some(prev) = st.last_t_ms {
            if t_ms < prev {
                return Err(EngineError::MonotonicityViolation {
                    entity_id: update.id.clone(),
                    last_t_ms: prev,
                    incoming_t_ms: t_ms,
                });
            }
        }

        let ctx = rules::RuleContext {
            entity_id,
            position: p,
            at_ms: t_ms,
            zone_dwell,
        };
        for rule in rules.iter() {
            rule.apply(
                spatial as &dyn SpatialIndex,
                &ctx,
                st,
                membership_scratch,
                &mut events,
            );
        }
        st.position = Some(p);
        st.last_t_ms = Some(t_ms);
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::LineString;
    use geo::Polygon;

    fn unit_square() -> Polygon<f64> {
        Polygon::new(
            LineString::from(vec![
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 1.0),
                (0.0, 1.0),
                (0.0, 0.0),
            ]),
            vec![],
        )
    }

    #[test]
    fn process_event_enter_then_exit_zone() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 100,
            })
            .unwrap();
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Enter { id, zone, t_ms: 100 } if id == "c1" && zone == "zone-1"
        ));

        let ev2 = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 5.0,
                y: 5.0,
                t_ms: 200,
            })
            .unwrap();
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Exit { id, zone, t_ms: 200 } if id == "c1" && zone == "zone-1"
        ));
    }

    #[test]
    fn enter_then_exit_square() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let (ev1, errs1) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }]);
        assert!(errs1.is_empty());
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Enter { id, zone, .. } if id == "c1" && zone == "zone-1"
        ));

        let (ev2, errs2) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert!(errs2.is_empty());
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Exit { id, zone, .. } if id == "c1" && zone == "zone-1"
        ));
    }

    #[test]
    fn deterministic_batch_ordering() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "z".into(),
            polygon: unit_square(),
        })
        .unwrap();
        let batch = vec![
            PointUpdate {
                id: "b".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            },
            PointUpdate {
                id: "a".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            },
        ];
        let (ev, errs) = e.process_batch(batch);
        assert!(errs.is_empty());
        assert_eq!(ev.len(), 2);
        assert!(matches!(&ev[0], Event::Enter { id, .. } if id == "a"));
        assert!(matches!(&ev[1], Event::Enter { id, .. } if id == "b"));
    }

    #[test]
    fn catalog_assignment_tie_break_smallest_id() {
        let mut e = Engine::new();
        e.register_catalog_region(Zone {
            id: "region-b".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.register_catalog_region(Zone {
            id: "region-a".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let (ev1, errs1) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }]);
        assert!(errs1.is_empty());
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::AssignmentChanged { id, region: Some(r), .. } if id == "c1" && r == "region-a"
        ));

        let (ev2, errs2) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert!(errs2.is_empty());
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::AssignmentChanged { id, region: None, .. } if id == "c1"
        ));
    }

    #[test]
    fn approach_recede_circle() {
        let mut e = Engine::new();
        e.register_circle(Circle {
            id: "rad-1".into(),
            cx: 0.0,
            cy: 0.0,
            r: 2.0,
        })
        .unwrap();

        let (ev1, errs1) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 0,
        }]);
        assert!(errs1.is_empty());
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Approach { id, circle, .. } if id == "c1" && circle == "rad-1"
        ));

        let (ev2, errs2) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 10.0,
            y: 0.0,
            t_ms: 0,
        }]);
        assert!(errs2.is_empty());
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Recede { id, circle, .. } if id == "c1" && circle == "rad-1"
        ));
    }

    #[test]
    fn zone_min_inside_ms_delays_enter_until_engine() {
        let mut e = Engine::new();
        e.register_zone_with_dwell(
            Zone {
                id: "zone-1".into(),
                polygon: unit_square(),
            },
            ZoneDwell {
                min_inside_ms: Some(50),
                min_outside_ms: None,
            },
        )
        .unwrap();

        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            })
            .unwrap()
            .is_empty());

        let ev = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 50,
            })
            .unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            &ev[0],
            Event::Enter { id, zone, t_ms: 50, .. } if id == "c1" && zone == "zone-1"
        ));
    }

    #[test]
    fn zone_min_outside_ms_debounces_exit() {
        let mut e = Engine::new();
        e.register_zone_with_dwell(
            Zone {
                id: "zone-1".into(),
                polygon: unit_square(),
            },
            ZoneDwell {
                min_inside_ms: None,
                min_outside_ms: Some(30),
            },
        )
        .unwrap();

        e.process_event(PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        })
        .unwrap();

        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 10.0,
                t_ms: 0,
            })
            .unwrap()
            .is_empty());

        let ev = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 10.0,
                t_ms: 30,
            })
            .unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            &ev[0],
            Event::Exit { id, zone, t_ms: 30, .. } if id == "c1" && zone == "zone-1"
        ));
    }

    // --- Monotonicity tests ---

    #[test]
    fn backwards_timestamp_returns_monotonicity_violation() {
        let mut e = Engine::new();
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.0,
            y: 0.0,
            t_ms: 100,
        })
        .unwrap();

        let err = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 1.0,
                y: 1.0,
                t_ms: 50,
            })
            .unwrap_err();

        assert!(matches!(
            err,
            EngineError::MonotonicityViolation {
                ref entity_id,
                last_t_ms: 100,
                incoming_t_ms: 50,
            } if entity_id == "e1"
        ));
    }

    #[test]
    fn equal_timestamp_is_accepted() {
        let mut e = Engine::new();
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.0,
            y: 0.0,
            t_ms: 100,
        })
        .unwrap();

        // Same timestamp must not be rejected.
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 1.0,
            y: 1.0,
            t_ms: 100,
        })
        .expect("equal timestamp should be accepted");
    }

    #[test]
    fn fresh_entity_never_violates_monotonicity() {
        let mut e = Engine::new();
        // No prior state -- any timestamp is valid.
        e.process_event(PointUpdate {
            id: "brand-new".into(),
            x: 0.0,
            y: 0.0,
            t_ms: 0,
        })
        .expect("first update for a new entity must not be a violation");
    }

    #[test]
    fn process_batch_skip_and_collect_violations() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        // Seed entity at t=100 (inside the zone -> Enter event).
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 100,
        })
        .unwrap();

        // Batch: one valid forward update (t=200, outside -> Exit) and one violation (t=50).
        let (events, errors) = e.process_batch(vec![
            PointUpdate {
                id: "e1".into(),
                x: 5.0,
                y: 5.0,
                t_ms: 200,
            },
            PointUpdate {
                id: "e1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 50,
            },
        ]);

        // The valid update (t=200) must produce an Exit event.
        assert_eq!(events.len(), 1, "expected exactly one Exit event");
        assert!(matches!(
            &events[0],
            Event::Exit { id, zone, .. } if id == "e1" && zone == "zone-1"
        ));

        // The backwards update (t=50) must appear as a collected error.
        assert_eq!(errors.len(), 1, "expected exactly one monotonicity error");
        assert!(matches!(
            &errors[0],
            EngineError::MonotonicityViolation { entity_id, incoming_t_ms: 50, .. }
            if entity_id == "e1"
        ));
    }
}
