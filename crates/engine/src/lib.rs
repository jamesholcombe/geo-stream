//! Pure, transport-agnostic geospatial stream engine: zone registration, single-update processing.

mod rules;

use spatial::NaiveSpatialIndex;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use thiserror::Error;

pub use rules::{default_rules, CatalogRule, RadiusRule, RuleContext, SpatialRule, ZoneRule};
pub use spatial::{Circle, SpatialError, SpatialIndex, Zone};
pub use state::{EntityState, HistoryPoint, ZoneDwell};

// ---------------------------------------------------------------------------
// Public event type
// ---------------------------------------------------------------------------

/// All events produced by the engine. Spatial events carry optional speed/heading metadata
/// when two or more successive updates have been processed for the entity.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Enter {
        id: String,
        zone: String,
        t_ms: u64,
        speed: Option<f64>,
        heading: Option<f64>,
    },
    Exit {
        id: String,
        zone: String,
        t_ms: u64,
        speed: Option<f64>,
        heading: Option<f64>,
    },
    Approach {
        id: String,
        circle: String,
        t_ms: u64,
        speed: Option<f64>,
        heading: Option<f64>,
    },
    Recede {
        id: String,
        circle: String,
        t_ms: u64,
        speed: Option<f64>,
        heading: Option<f64>,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t_ms: u64,
    },
    /// Emitted when a user-defined `ConfigurableRule` matches.
    Custom {
        id: String,
        name: String,
        t_ms: u64,
        speed: Option<f64>,
        heading: Option<f64>,
        /// Arbitrary JSON payload supplied when the rule was defined.
        data: serde_json::Value,
    },
    /// Emitted when all steps of a `SequenceRule` are matched in order.
    SequenceComplete {
        id: String,
        sequence: String,
        t_ms: u64,
    },
}

fn enrich(ev: state::Event, speed: Option<f64>, heading: Option<f64>) -> Event {
    match ev {
        state::Event::Enter { id, zone, t_ms } => Event::Enter {
            id,
            zone,
            t_ms,
            speed,
            heading,
        },
        state::Event::Exit { id, zone, t_ms } => Event::Exit {
            id,
            zone,
            t_ms,
            speed,
            heading,
        },
        state::Event::Approach { id, circle, t_ms } => Event::Approach {
            id,
            circle,
            t_ms,
            speed,
            heading,
        },
        state::Event::Recede { id, circle, t_ms } => Event::Recede {
            id,
            circle,
            t_ms,
            speed,
            heading,
        },
        state::Event::AssignmentChanged { id, region, t_ms } => {
            Event::AssignmentChanged { id, region, t_ms }
        }
    }
}

/// Stable sort for engine events:
/// entity_id → t_ms → tier (Zone < Circle < Assignment < Custom < Sequence) → id → enter/approach before exit/recede
pub fn sort_events_deterministic(events: &mut [Event]) {
    events.sort_by(|a, b| event_sort_key(a).cmp(&event_sort_key(b)));
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventTier {
    Zone = 0,
    Circle = 1,
    Assignment = 2,
    Custom = 3,
    Sequence = 4,
}

fn event_sort_key(e: &Event) -> (&str, u64, EventTier, &str, u8) {
    match e {
        Event::Enter { id, zone, t_ms, .. } => {
            (id.as_str(), *t_ms, EventTier::Zone, zone.as_str(), 0)
        }
        Event::Exit { id, zone, t_ms, .. } => {
            (id.as_str(), *t_ms, EventTier::Zone, zone.as_str(), 1)
        }
        Event::Approach {
            id, circle, t_ms, ..
        } => (id.as_str(), *t_ms, EventTier::Circle, circle.as_str(), 0),
        Event::Recede {
            id, circle, t_ms, ..
        } => (id.as_str(), *t_ms, EventTier::Circle, circle.as_str(), 1),
        Event::AssignmentChanged { id, region, t_ms } => {
            let r = region.as_deref().unwrap_or("");
            (id.as_str(), *t_ms, EventTier::Assignment, r, 0)
        }
        Event::Custom { id, name, t_ms, .. } => {
            (id.as_str(), *t_ms, EventTier::Custom, name.as_str(), 0)
        }
        Event::SequenceComplete { id, sequence, t_ms } => (
            id.as_str(),
            *t_ms,
            EventTier::Sequence,
            sequence.as_str(),
            0,
        ),
    }
}

// ---------------------------------------------------------------------------
// Configurable rule types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Enter,
    Exit,
    Approach,
    Recede,
}

#[derive(Debug, Clone)]
pub struct RuleTrigger {
    pub event_kind: EventKind,
    pub target_id: String,
}

#[derive(Debug, Clone)]
pub enum RuleFilter {
    SpeedAbove(f64),
    SpeedBelow(f64),
    /// Heading range in degrees (0–360). Handles wrap-around (e.g. from=350, to=10).
    HeadingBetween {
        from: f64,
        to: f64,
    },
}

/// A named rule that fires a `Custom` event when spatial events and entity-state filters all match.
#[derive(Debug, Clone)]
pub struct ConfigurableRule {
    pub name: String,
    pub triggers: Vec<RuleTrigger>,
    pub filters: Vec<RuleFilter>,
    /// Name of the emitted `Custom` event.
    pub emit: String,
    /// Arbitrary payload attached to every emitted `Custom` event.
    pub data: serde_json::Value,
}

impl ConfigurableRule {
    fn fire(
        &self,
        events: &[Event],
        entity_state: &EntityState,
        entity_id: &str,
        t_ms: u64,
        out: &mut Vec<Event>,
    ) {
        let triggered = events.iter().any(|e| {
            self.triggers
                .iter()
                .any(|t| trigger_matches(e, t, entity_id))
        });
        if !triggered {
            return;
        }
        let passes = self.filters.iter().all(|f| match f {
            RuleFilter::SpeedAbove(thr) => entity_state.speed.is_some_and(|s| s > *thr),
            RuleFilter::SpeedBelow(thr) => entity_state.speed.is_some_and(|s| s < *thr),
            RuleFilter::HeadingBetween { from, to } => entity_state.heading.is_some_and(|h| {
                if from <= to {
                    h >= *from && h <= *to
                } else {
                    h >= *from || h <= *to
                }
            }),
        });
        if passes {
            out.push(Event::Custom {
                id: entity_id.to_string(),
                name: self.emit.clone(),
                t_ms,
                speed: entity_state.speed,
                heading: entity_state.heading,
                data: self.data.clone(),
            });
        }
    }
}

fn trigger_matches(event: &Event, trigger: &RuleTrigger, entity_id: &str) -> bool {
    match (&trigger.event_kind, event) {
        (EventKind::Enter, Event::Enter { id, zone, .. }) => {
            id == entity_id && zone == &trigger.target_id
        }
        (EventKind::Exit, Event::Exit { id, zone, .. }) => {
            id == entity_id && zone == &trigger.target_id
        }
        (EventKind::Approach, Event::Approach { id, circle, .. }) => {
            id == entity_id && circle == &trigger.target_id
        }
        (EventKind::Recede, Event::Recede { id, circle, .. }) => {
            id == entity_id && circle == &trigger.target_id
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Sequence rule
// ---------------------------------------------------------------------------

/// A rule that fires `SequenceComplete` when an entity triggers each step in order.
/// Steps are matched on `Enter` (zone) or `Approach` (circle) events.
pub struct SequenceRule {
    pub name: String,
    pub steps: Vec<String>,
    pub within_ms: Option<u64>,
    /// Per-entity state machine state.
    entity_progress: HashMap<String, SequenceProgress>,
}

struct SequenceProgress {
    current_step: usize,
    started_at: u64,
}

impl SequenceRule {
    pub fn new(name: String, steps: Vec<String>, within_ms: Option<u64>) -> Self {
        Self {
            name,
            steps,
            within_ms,
            entity_progress: HashMap::new(),
        }
    }

    fn fire(&mut self, events: &[Event], entity_id: &str, t_ms: u64, out: &mut Vec<Event>) {
        if self.steps.is_empty() {
            return;
        }
        let progress =
            self.entity_progress
                .entry(entity_id.to_string())
                .or_insert(SequenceProgress {
                    current_step: 0,
                    started_at: 0,
                });

        // Expire if window exceeded.
        if progress.current_step > 0 {
            if let Some(within) = self.within_ms {
                if t_ms.saturating_sub(progress.started_at) > within {
                    progress.current_step = 0;
                    progress.started_at = 0;
                }
            }
        }

        let expected = &self.steps[progress.current_step];
        let matched = events.iter().any(|e| match e {
            Event::Enter { id, zone, .. } => id == entity_id && zone == expected,
            Event::Approach { id, circle, .. } => id == entity_id && circle == expected,
            _ => false,
        });

        if matched {
            if progress.current_step == 0 {
                progress.started_at = t_ms;
            }
            progress.current_step += 1;

            if progress.current_step >= self.steps.len() {
                out.push(Event::SequenceComplete {
                    id: entity_id.to_string(),
                    sequence: self.name.clone(),
                    t_ms,
                });
                progress.current_step = 0;
                progress.started_at = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

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

/// Options for constructing an [`Engine`].
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Maximum number of historical position samples retained per entity. Default: 10.
    pub history_size: usize,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self { history_size: 10 }
    }
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
    configurable_rules: Vec<ConfigurableRule>,
    sequence_rules: Vec<SequenceRule>,
    history_size: usize,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("spatial", &self.spatial)
            .field("zone_dwell", &self.zone_dwell.len())
            .field("entities", &self.entities)
            .field("rules", &self.rules.len())
            .field("configurable_rules", &self.configurable_rules.len())
            .field("sequence_rules", &self.sequence_rules.len())
            .field("history_size", &self.history_size)
            .finish()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::with_options(EngineOptions::default())
    }

    pub fn with_options(opts: EngineOptions) -> Self {
        Self {
            spatial: NaiveSpatialIndex::default(),
            zone_dwell: HashMap::new(),
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules: rules::default_rules(),
            configurable_rules: Vec::new(),
            sequence_rules: Vec::new(),
            history_size: opts.history_size,
        }
    }

    pub fn with_rules(rules: Vec<Box<dyn SpatialRule>>) -> Self {
        Self {
            spatial: NaiveSpatialIndex::default(),
            zone_dwell: HashMap::new(),
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules,
            configurable_rules: Vec::new(),
            sequence_rules: Vec::new(),
            history_size: EngineOptions::default().history_size,
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

    /// Add a configurable rule that fires `Custom` events when its triggers and filters match.
    pub fn add_rule(&mut self, rule: ConfigurableRule) {
        self.configurable_rules.push(rule);
    }

    /// Add a sequence rule that fires `SequenceComplete` when all steps are matched in order.
    pub fn add_sequence(&mut self, rule: SequenceRule) {
        self.sequence_rules.push(rule);
    }

    /// Return a snapshot of the current state for the given entity, or `None` if unseen.
    pub fn get_entity_state(&self, id: &str) -> Option<&EntityState> {
        self.entities.get(id)
    }

    /// Return snapshots for all known entities.
    pub fn get_entities(&self) -> impl Iterator<Item = (&str, &EntityState)> {
        self.entities.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Sort updates by entity id, run `GeoEngine::process_event` for each, then
    /// `sort_events_deterministic` on the combined output.
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
        let p = (update.x, update.y);
        let t_ms = update.t_ms;
        let entity_id = update.id.clone();

        let Engine {
            spatial,
            zone_dwell,
            entities,
            membership_scratch,
            rules,
            configurable_rules,
            sequence_rules,
            history_size,
        } = self;

        let st = entities.entry(entity_id.clone()).or_default();

        // Enforce monotonicity: reject strictly backwards timestamps.
        if let Some(prev) = st.last_t_ms {
            if t_ms < prev {
                return Err(EngineError::MonotonicityViolation {
                    entity_id,
                    last_t_ms: prev,
                    incoming_t_ms: t_ms,
                });
            }
        }

        // Compute speed and heading before updating state.
        let (speed, heading) = if let (Some(last_pos), Some(last_t)) = (st.position, st.last_t_ms) {
            let dt_s = t_ms.saturating_sub(last_t) as f64 / 1000.0;
            if dt_s > 0.0 {
                let dx = p.0 - last_pos.0;
                let dy = p.1 - last_pos.1;
                let dist = (dx * dx + dy * dy).sqrt();
                // heading: 0° = north (+y), clockwise. atan2(dx, dy) gives bearing from north.
                let hdg = dx.atan2(dy).to_degrees().rem_euclid(360.0);
                (Some(dist / dt_s), Some(hdg))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        st.speed = speed;
        st.heading = heading;

        // Append to history ring buffer.
        if *history_size > 0 {
            if st.history.len() >= *history_size {
                st.history.pop_front();
            }
            st.history.push_back(HistoryPoint {
                x: p.0,
                y: p.1,
                t_ms,
            });
        }

        // Run spatial rules → raw state events.
        let ctx = rules::RuleContext {
            entity_id: entity_id.as_str(),
            position: p,
            at_ms: t_ms,
            zone_dwell,
        };
        let mut raw: Vec<state::Event> = Vec::new();
        for rule in rules.iter() {
            rule.apply(
                spatial as &dyn SpatialIndex,
                &ctx,
                st,
                membership_scratch,
                &mut raw,
            );
        }

        // Enrich spatial events with speed/heading.
        let mut events: Vec<Event> = raw.into_iter().map(|e| enrich(e, speed, heading)).collect();

        // Configurable rules (read-only over events so far).
        let mut custom: Vec<Event> = Vec::new();
        for rule in configurable_rules.iter() {
            rule.fire(&events, st, entity_id.as_str(), t_ms, &mut custom);
        }
        events.extend(custom);

        // Sequence rules (mutate per-rule state).
        let mut seq: Vec<Event> = Vec::new();
        for rule in sequence_rules.iter_mut() {
            rule.fire(&events, entity_id.as_str(), t_ms, &mut seq);
        }
        events.extend(seq);

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
        assert!(
            matches!(&ev1[0], Event::Enter { id, zone, t_ms: 100, .. } if id == "c1" && zone == "zone-1")
        );

        let ev2 = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 5.0,
                y: 5.0,
                t_ms: 200,
            })
            .unwrap();
        assert_eq!(ev2.len(), 1);
        assert!(
            matches!(&ev2[0], Event::Exit { id, zone, t_ms: 200, .. } if id == "c1" && zone == "zone-1")
        );
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
        assert!(matches!(&ev1[0], Event::Enter { id, zone, .. } if id == "c1" && zone == "zone-1"));

        let (ev2, errs2) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 0,
        }]);
        assert!(errs2.is_empty());
        assert_eq!(ev2.len(), 1);
        assert!(matches!(&ev2[0], Event::Exit { id, zone, .. } if id == "c1" && zone == "zone-1"));
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
        assert!(matches!(&ev2[0], Event::AssignmentChanged { id, region: None, .. } if id == "c1"));
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
        assert!(
            matches!(&ev1[0], Event::Approach { id, circle, .. } if id == "c1" && circle == "rad-1")
        );

        let (ev2, errs2) = e.process_batch(vec![PointUpdate {
            id: "c1".into(),
            x: 10.0,
            y: 0.0,
            t_ms: 0,
        }]);
        assert!(errs2.is_empty());
        assert_eq!(ev2.len(), 1);
        assert!(
            matches!(&ev2[0], Event::Recede { id, circle, .. } if id == "c1" && circle == "rad-1")
        );
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
                t_ms: 0
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
        assert!(
            matches!(&ev[0], Event::Enter { id, zone, t_ms: 50, .. } if id == "c1" && zone == "zone-1")
        );
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
                t_ms: 0
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
        assert!(
            matches!(&ev[0], Event::Exit { id, zone, t_ms: 30, .. } if id == "c1" && zone == "zone-1")
        );
    }

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
            EngineError::MonotonicityViolation { ref entity_id, last_t_ms: 100, incoming_t_ms: 50 }
            if entity_id == "e1"
        ));
    }

    #[test]
    fn duplicate_zone_id_raises() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "a".into(),
            polygon: unit_square(),
        })
        .unwrap();
        let result = e.register_zone(Zone {
            id: "a".into(),
            polygon: unit_square(),
        });
        assert!(result.is_err());
        assert!(matches!(result, Err(EngineError::Spatial(_))));
    }

    #[test]
    fn duplicate_circle_id_raises() {
        let mut e = Engine::new();
        e.register_circle(Circle {
            id: "1".into(),
            cx: 1.0,
            cy: 1.0,
            r: 1.0,
        })
        .unwrap();
        let result = e.register_circle(Circle {
            id: "1".into(),
            cx: 1.0,
            cy: 1.0,
            r: 1.0,
        });
        assert!(matches!(result, Err(EngineError::Spatial(_))))
    }

    #[test]
    fn cross_type_duplicate_id_ok() {
        let mut e = Engine::new();
        e.register_circle(Circle {
            id: "1".into(),
            cx: 1.0,
            cy: 1.0,
            r: 1.0,
        })
        .unwrap();
        e.register_zone(Zone {
            id: "1".into(),
            polygon: unit_square(),
        })
        .unwrap();
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

        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 100,
        })
        .unwrap();

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

        assert_eq!(events.len(), 1, "expected exactly one Exit event");
        assert!(
            matches!(&events[0], Event::Exit { id, zone, .. } if id == "e1" && zone == "zone-1")
        );

        assert_eq!(errors.len(), 1, "expected exactly one monotonicity error");
        assert!(matches!(
            &errors[0],
            EngineError::MonotonicityViolation { entity_id, incoming_t_ms: 50, .. }
            if entity_id == "e1"
        ));
    }

    // --- New feature tests ---

    #[test]
    fn speed_and_heading_computed_after_two_updates() {
        let mut e = Engine::new();
        // First update — no prior position, speed/heading should be None.
        let ev1 = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 0.0,
                y: 0.0,
                t_ms: 0,
            })
            .unwrap();
        assert!(ev1.is_empty());
        let st = e.get_entity_state("e1").unwrap();
        assert!(st.speed.is_none());
        assert!(st.heading.is_none());

        // Register a zone to get events that carry speed/heading.
        let mut e2 = Engine::new();
        e2.register_zone(Zone {
            id: "z".into(),
            polygon: unit_square(),
        })
        .unwrap();
        // First update inside zone — no prior position.
        e2.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.0,
            y: 0.0,
            t_ms: 0,
        })
        .unwrap();
        // Second update: move 1 unit north in 1000ms → speed = 1.0 u/s, heading = 0°
        let ev2 = e2
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 1000,
            })
            .unwrap();
        let st2 = e2.get_entity_state("e1").unwrap();
        assert!(st2.speed.is_some());
        assert!(st2.heading.is_some());
        // Enter event should carry speed/heading.
        assert!(matches!(
            &ev2[0],
            Event::Enter {
                speed: Some(_),
                heading: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn history_ring_buffer_bounded() {
        let mut e = Engine::with_options(EngineOptions { history_size: 3 });
        for i in 0..5u64 {
            e.process_event(PointUpdate {
                id: "e1".into(),
                x: i as f64,
                y: 0.0,
                t_ms: i * 1000,
            })
            .unwrap();
        }
        let st = e.get_entity_state("e1").unwrap();
        assert_eq!(st.history.len(), 3);
        // Oldest kept should be i=2 (x=2.0).
        assert_eq!(st.history[0].x, 2.0);
    }

    #[test]
    fn configurable_rule_fires_custom_event() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "zone-a".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.add_rule(ConfigurableRule {
            name: "test-rule".into(),
            triggers: vec![RuleTrigger {
                event_kind: EventKind::Enter,
                target_id: "zone-a".into(),
            }],
            filters: vec![],
            emit: "entry-alert".into(),
            data: serde_json::json!({ "severity": "high" }),
        });

        let events = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 100,
            })
            .unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Event::Enter { .. }));
        assert!(matches!(&events[1], Event::Custom { name, .. } if name == "entry-alert"));
    }

    #[test]
    fn configurable_rule_speed_filter_suppresses() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "zone-a".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.add_rule(ConfigurableRule {
            name: "fast-entry".into(),
            triggers: vec![RuleTrigger {
                event_kind: EventKind::Enter,
                target_id: "zone-a".into(),
            }],
            filters: vec![RuleFilter::SpeedAbove(5.0)],
            emit: "fast-entry".into(),
            data: serde_json::Value::Null,
        });

        // First update: enters zone but no speed yet (first position).
        let events = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            })
            .unwrap();
        // Rule should NOT fire because speed is None (filter fails).
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::Enter { .. }));
    }

    #[test]
    fn sequence_rule_fires_on_completion() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "a".into(),
            polygon: unit_square(),
        })
        .unwrap();
        let far_square = geo::Polygon::new(
            geo::LineString::from(vec![
                (10.0, 10.0),
                (11.0, 10.0),
                (11.0, 11.0),
                (10.0, 11.0),
                (10.0, 10.0),
            ]),
            vec![],
        );
        e.register_zone(Zone {
            id: "b".into(),
            polygon: far_square,
        })
        .unwrap();
        e.add_sequence(SequenceRule::new(
            "a-then-b".into(),
            vec!["a".into(), "b".into()],
            None,
        ));

        // Step 1: enter zone-a.
        let ev1 = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            })
            .unwrap();
        assert!(ev1
            .iter()
            .all(|ev| !matches!(ev, Event::SequenceComplete { .. })));

        // Step 2: enter zone-b.
        let ev2 = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 10.5,
                y: 10.5,
                t_ms: 1000,
            })
            .unwrap();
        assert!(ev2.iter().any(
            |ev| matches!(ev, Event::SequenceComplete { sequence, .. } if sequence == "a-then-b")
        ));
    }

    #[test]
    fn sequence_rule_expires_on_timeout() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "a".into(),
            polygon: unit_square(),
        })
        .unwrap();
        let far_square = geo::Polygon::new(
            geo::LineString::from(vec![
                (10.0, 10.0),
                (11.0, 10.0),
                (11.0, 11.0),
                (10.0, 11.0),
                (10.0, 10.0),
            ]),
            vec![],
        );
        e.register_zone(Zone {
            id: "b".into(),
            polygon: far_square,
        })
        .unwrap();
        e.add_sequence(SequenceRule::new(
            "a-then-b".into(),
            vec!["a".into(), "b".into()],
            Some(500),
        ));

        // Step 1: enter zone-a at t=0.
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        })
        .unwrap();
        // Wait past the window (600ms > 500ms).
        e.process_event(PointUpdate {
            id: "e1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 600,
        })
        .unwrap();
        // Step 2 attempt after expiry — sequence should NOT complete.
        let ev = e
            .process_event(PointUpdate {
                id: "e1".into(),
                x: 10.5,
                y: 10.5,
                t_ms: 700,
            })
            .unwrap();
        assert!(!ev
            .iter()
            .any(|e| matches!(e, Event::SequenceComplete { .. })));
    }
}
