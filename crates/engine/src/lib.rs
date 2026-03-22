//! Pure, transport-agnostic geospatial stream engine: zone registration, single-update processing.

mod rules;

use spatial::NaiveSpatialIndex;
use state::sort_events_deterministic;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use thiserror::Error;

pub use rules::{
    default_rules, CatalogRule, CorridorRule, GeofenceRule, RadiusRule, RuleContext, SpatialRule,
};

pub use spatial::{Geofence, RadiusZone, SpatialError};
pub use state::{EntityState, Event};

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
    fn register_geofence(&mut self, geofence: Geofence) -> Result<(), EngineError>;
    fn register_corridor(&mut self, corridor: Geofence) -> Result<(), EngineError>;
    fn register_catalog_region(&mut self, region: Geofence) -> Result<(), EngineError>;
    fn register_radius_zone(&mut self, zone: RadiusZone) -> Result<(), EngineError>;

    /// Process one location update. For multiple updates with cross-update event ordering, use [`Engine::process_batch`].
    fn process_event(&mut self, update: PointUpdate) -> Vec<Event>;
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Spatial(#[from] spatial::SpatialError),
}

/// In-memory engine: R-tree–accelerated polygon queries + per-entity membership state.
pub struct Engine {
    spatial: NaiveSpatialIndex,
    entities: HashMap<String, EntityState>,
    /// Reused between membership tiers to avoid cloning [`EntityState`] sets each update.
    membership_scratch: BTreeSet<String>,
    rules: Vec<Box<dyn SpatialRule>>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("spatial", &self.spatial)
            .field("entities", &self.entities)
            .field("rules", &self.rules.len())
            .finish()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            spatial: NaiveSpatialIndex::default(),
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
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules,
        }
    }

    /// Sort updates by entity id, run `GeoEngine::process_event` for each, then `state::sort_events_deterministic` on the combined output.
    pub fn process_batch(&mut self, mut batch: Vec<PointUpdate>) -> Vec<Event> {
        batch.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.t_ms.cmp(&b.t_ms)));
        let mut events = Vec::new();
        for u in batch {
            events.extend(self.process_event(u));
        }
        sort_events_deterministic(&mut events);
        events
    }
}

impl GeoEngine for Engine {
    fn register_geofence(&mut self, geofence: Geofence) -> Result<(), EngineError> {
        self.spatial.try_push(geofence)?;
        Ok(())
    }

    fn register_corridor(&mut self, corridor: Geofence) -> Result<(), EngineError> {
        self.spatial.try_push_corridor(corridor)?;
        Ok(())
    }

    fn register_catalog_region(&mut self, region: Geofence) -> Result<(), EngineError> {
        self.spatial.try_push_catalog_region(region)?;
        Ok(())
    }

    fn register_radius_zone(&mut self, zone: RadiusZone) -> Result<(), EngineError> {
        self.spatial.try_push_radius_zone(zone)?;
        Ok(())
    }

    fn process_event(&mut self, update: PointUpdate) -> Vec<Event> {
        let mut events = Vec::new();
        let p = (update.x, update.y);
        let st = self.entities.entry(update.id.clone()).or_default();
        let entity_id = update.id.as_str();
        let spatial = &self.spatial;
        let scratch = &mut self.membership_scratch;
        let t_ms = update.t_ms;
        let ctx = rules::RuleContext {
            entity_id,
            position: p,
            at_ms: t_ms,
        };
        for rule in &self.rules {
            rule.apply(spatial, &ctx, st, scratch, &mut events);
        }
        st.position = Some(p);
        st.last_t_ms = Some(t_ms);
        events
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
    fn process_event_enter_then_exit_geofence() {
        let mut e = Engine::new();
        e.register_geofence(Geofence {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e.process_event(PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 100,
        });
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Enter { id, geofence, t_ms: 100 } if id == "c1" && geofence == "zone-1"
        ));

        let ev2 = e.process_event(PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 200,
        });
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Exit { id, geofence, t_ms: 200 } if id == "c1" && geofence == "zone-1"
        ));
    }

    #[test]
    fn enter_then_exit_square() {
        let mut e = Engine::new();
        e.register_geofence(Geofence {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Enter { id, geofence, .. } if id == "c1" && geofence == "zone-1"
        ));

        let ev2 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Exit { id, geofence, .. } if id == "c1" && geofence == "zone-1"
        ));
    }

    #[test]
    fn deterministic_batch_ordering() {
        let mut e = Engine::new();
        e.register_geofence(Geofence {
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
        let ev = e.process_batch(batch);
        assert_eq!(ev.len(), 2);
        assert!(matches!(&ev[0], Event::Enter { id, .. } if id == "a"));
        assert!(matches!(&ev[1], Event::Enter { id, .. } if id == "b"));
    }

    #[test]
    fn catalog_assignment_tie_break_smallest_id() {
        let mut e = Engine::new();
        e.register_catalog_region(Geofence {
            id: "region-b".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.register_catalog_region(Geofence {
            id: "region-a".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::AssignmentChanged { id, region: Some(r), .. } if id == "c1" && r == "region-a"
        ));

        let ev2 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::AssignmentChanged { id, region: None, .. } if id == "c1"
        ));
    }

    #[test]
    fn approach_recede_radius() {
        let mut e = Engine::new();
        e.register_radius_zone(RadiusZone {
            id: "rad-1".into(),
            cx: 0.0,
            cy: 0.0,
            r: 2.0,
        })
        .unwrap();

        let ev1 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 0,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Approach { id, zone, .. } if id == "c1" && zone == "rad-1"
        ));

        let ev2 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 10.0,
            y: 0.0,
            t_ms: 0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Recede { id, zone, .. } if id == "c1" && zone == "rad-1"
        ));
    }

    #[test]
    fn corridor_enter_exit() {
        let mut e = Engine::new();
        e.register_corridor(Geofence {
            id: "cor-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::EnterCorridor { id, corridor, .. } if id == "c1" && corridor == "cor-1"
        ));

        let ev2 = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::ExitCorridor { id, corridor, .. } if id == "c1" && corridor == "cor-1"
        ));
    }
}
