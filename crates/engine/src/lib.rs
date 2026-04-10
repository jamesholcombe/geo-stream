//! Pure, transport-agnostic geospatial stream engine: zone registration, single-update processing.

mod rules;

use spatial::NaiveSpatialIndex;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

pub use rules::{default_rules, CatalogRule, RadiusRule, RuleContext, SpatialRule, ZoneRule};
pub use spatial::{Circle, SpatialError, SpatialIndex, Zone};
pub use state::{CircleDwell, EntityState, HistoryPoint, MemoryStateStore, StateStore, ZoneDwell};

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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EventKind {
    Enter,
    Exit,
    Approach,
    Recede,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleTrigger {
    pub event_kind: EventKind,
    pub target_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Serializable form of [`SequenceRule`] for use in engine snapshots.
/// Per-entity progress state is intentionally excluded — in-flight sequences are abandoned
/// on process restart (acceptable for this persistence model).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SequenceRuleSnapshot {
    pub name: String,
    pub steps: Vec<String>,
    pub within_ms: Option<u64>,
}

impl From<&SequenceRule> for SequenceRuleSnapshot {
    fn from(r: &SequenceRule) -> Self {
        Self {
            name: r.name.clone(),
            steps: r.steps.clone(),
            within_ms: r.within_ms,
        }
    }
}

impl From<SequenceRuleSnapshot> for SequenceRule {
    fn from(s: SequenceRuleSnapshot) -> Self {
        SequenceRule::new(s.name, s.steps, s.within_ms)
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
    fn register_zone_with_dwell(&mut self, zone: Zone, dwell: ZoneDwell)
        -> Result<(), EngineError>;
    fn register_catalog_region(&mut self, region: Zone) -> Result<(), EngineError>;
    fn register_circle(&mut self, circle: Circle) -> Result<(), EngineError>;
    fn register_circle_with_dwell(
        &mut self,
        circle: Circle,
        dwell: CircleDwell,
    ) -> Result<(), EngineError>;

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

// ---------------------------------------------------------------------------
// Snapshot / persistence
// ---------------------------------------------------------------------------

/// A complete, serializable point-in-time capture of [`Engine`] state. Can be saved to a file
/// or any other backing store and passed to [`Engine::restore_from_snapshot`] to resume.
///
/// Zone/circle registrations are included so the spatial index is fully reconstructed.
/// The default spatial rule pipeline is deterministic and is not stored; custom rules ARE stored.
/// In-flight sequence rule progress is not preserved (sequences restart after a restore).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EngineSnapshot {
    pub entities: HashMap<String, EntityState>,
    pub fences: Vec<Zone>,
    pub catalog: Vec<Zone>,
    pub circles: Vec<Circle>,
    pub zone_dwell: HashMap<String, ZoneDwell>,
    pub circle_dwell: HashMap<String, CircleDwell>,
    pub configurable_rules: Vec<ConfigurableRule>,
    pub sequence_rules: Vec<SequenceRuleSnapshot>,
    pub history_size: usize,
}

/// Pluggable persistence backend for engine snapshots. Implement this to save/load
/// snapshots from a file, Redis, DynamoDB, or any other store.
pub trait SnapshotStore {
    type Error: std::error::Error + Send + Sync + 'static;
    /// Persist `snapshot` to the backing store.
    fn save(&self, snapshot: &EngineSnapshot) -> Result<(), Self::Error>;
    /// Load the most recent snapshot, or `None` if the store is empty.
    fn load(&self) -> Result<Option<EngineSnapshot>, Self::Error>;
}

/// [`SnapshotStore`] implementation that reads and writes a single JSON file.
pub struct FileSnapshotStore {
    pub path: PathBuf,
}

impl SnapshotStore for FileSnapshotStore {
    type Error = std::io::Error;

    fn save(&self, snapshot: &EngineSnapshot) -> Result<(), Self::Error> {
        let json = serde_json::to_string(snapshot)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&self.path, json)
    }

    fn load(&self) -> Result<Option<EngineSnapshot>, Self::Error> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&self.path)?;
        let snap = serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(snap))
    }
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
    /// Per circle id: minimum inside/outside dwell before approach/recede events.
    circle_dwell: HashMap<String, CircleDwell>,
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
            .field("circle_dwell", &self.circle_dwell.len())
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
            circle_dwell: HashMap::new(),
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
            circle_dwell: HashMap::new(),
            entities: HashMap::new(),
            membership_scratch: BTreeSet::new(),
            rules,
            configurable_rules: Vec::new(),
            sequence_rules: Vec::new(),
            history_size: EngineOptions::default().history_size,
        }
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

    /// Return all entities whose logical zone membership includes `zone_id`.
    pub fn entities_in_zone(&self, zone_id: &str) -> Vec<(&str, &EntityState)> {
        self.entities
            .iter()
            .filter(|(_, st)| st.inside.contains(zone_id))
            .map(|(id, st)| (id.as_str(), st))
            .collect()
    }

    /// Return all entities whose logical circle membership includes `circle_id`.
    pub fn entities_in_circle(&self, circle_id: &str) -> Vec<(&str, &EntityState)> {
        self.entities
            .iter()
            .filter(|(_, st)| st.inside_circle.contains(circle_id))
            .map(|(id, st)| (id.as_str(), st))
            .collect()
    }

    /// Return all entities whose current catalog region matches `region_id`.
    pub fn entities_in_region(&self, region_id: &str) -> Vec<(&str, &EntityState)> {
        self.entities
            .iter()
            .filter(|(_, st)| st.catalog_region.as_deref() == Some(region_id))
            .map(|(id, st)| (id.as_str(), st))
            .collect()
    }

    /// Return all entities within `radius` of `(x, y)`, sorted by distance ascending.
    /// Entities with no known position are excluded.
    pub fn entities_near_point(
        &self,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Vec<(&str, &EntityState, f64)> {
        let r2 = radius * radius;
        let mut out: Vec<_> = self
            .entities
            .iter()
            .filter_map(|(id, st)| {
                let (ex, ey) = st.position?;
                let dx = ex - x;
                let dy = ey - y;
                let dist2 = dx * dx + dy * dy;
                (dist2 <= r2).then(|| (id.as_str(), st, dist2.sqrt()))
            })
            .collect();
        out.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Return the `k` nearest entities to `(x, y)`, sorted by distance ascending.
    /// Entities with no known position are excluded.
    pub fn nearest_to_point(&self, x: f64, y: f64, k: usize) -> Vec<(&str, &EntityState, f64)> {
        let mut out: Vec<_> = self
            .entities
            .iter()
            .filter_map(|(id, st)| {
                let (ex, ey) = st.position?;
                let dx = ex - x;
                let dy = ey - y;
                Some((id.as_str(), st, (dx * dx + dy * dy).sqrt()))
            })
            .collect();
        out.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(k);
        out
    }

    /// Capture a serializable snapshot of the full engine state.
    ///
    /// The snapshot includes entity state, all registered zones/circles/catalog regions, dwell
    /// configs, and configurable/sequence rule definitions. Pass the result to
    /// [`Engine::restore_from_snapshot`] (or a [`SnapshotStore`] impl) to persist it.
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            entities: self.entities.clone(),
            fences: self.spatial.zones().to_vec(),
            catalog: self.spatial.catalog_regions().to_vec(),
            circles: self.spatial.circles().to_vec(),
            zone_dwell: self.zone_dwell.clone(),
            circle_dwell: self.circle_dwell.clone(),
            configurable_rules: self.configurable_rules.clone(),
            sequence_rules: self
                .sequence_rules
                .iter()
                .map(SequenceRuleSnapshot::from)
                .collect(),
            history_size: self.history_size,
        }
    }

    /// Reconstruct an engine from a previously captured [`EngineSnapshot`].
    ///
    /// Zone/circle registrations and all entity membership state are restored. The default
    /// spatial rule pipeline is rebuilt deterministically. In-flight sequence progress is not
    /// restored (sequences restart from step 0).
    pub fn restore_from_snapshot(snap: EngineSnapshot) -> Result<Self, EngineError> {
        let spatial = NaiveSpatialIndex::from_vecs(snap.fences, snap.catalog, snap.circles)?;
        let mut engine = Self {
            spatial,
            zone_dwell: snap.zone_dwell,
            circle_dwell: snap.circle_dwell,
            entities: snap.entities,
            membership_scratch: BTreeSet::new(),
            rules: rules::default_rules(),
            configurable_rules: snap.configurable_rules,
            sequence_rules: snap
                .sequence_rules
                .into_iter()
                .map(SequenceRule::from)
                .collect(),
            history_size: snap.history_size,
        };
        // Ensure every zone that has a registered dwell config also has a default entry.
        // (Zones registered via register_zone always insert a default; snapshot round-trips do too.)
        for zone in engine.spatial.zones() {
            engine.zone_dwell.entry(zone.id.clone()).or_default();
        }
        for circle in engine.spatial.circles() {
            engine.circle_dwell.entry(circle.id.clone()).or_default();
        }
        Ok(engine)
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
        self.register_zone_with_dwell(zone, ZoneDwell::default())
    }

    fn register_zone_with_dwell(
        &mut self,
        zone: Zone,
        dwell: ZoneDwell,
    ) -> Result<(), EngineError> {
        let id = zone.id.clone();
        self.spatial.try_push_zone(zone)?;
        self.zone_dwell.insert(id, dwell);
        Ok(())
    }

    fn register_catalog_region(&mut self, region: Zone) -> Result<(), EngineError> {
        self.spatial.try_push_catalog_region(region)?;
        Ok(())
    }

    fn register_circle(&mut self, circle: Circle) -> Result<(), EngineError> {
        self.register_circle_with_dwell(circle, CircleDwell::default())
    }

    fn register_circle_with_dwell(
        &mut self,
        circle: Circle,
        dwell: CircleDwell,
    ) -> Result<(), EngineError> {
        let id = circle.id.clone();
        self.spatial.try_push_circle(circle)?;
        self.circle_dwell.insert(id, dwell);
        Ok(())
    }

    fn process_event(&mut self, update: PointUpdate) -> Result<Vec<Event>, EngineError> {
        let p = (update.x, update.y);
        let t_ms = update.t_ms;
        let entity_id = update.id.clone();

        let Engine {
            spatial,
            zone_dwell,
            circle_dwell,
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
            circle_dwell,
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
    fn circle_min_inside_ms_delays_approach() {
        let mut e = Engine::new();
        e.register_circle_with_dwell(
            Circle {
                id: "rad-1".into(),
                cx: 0.0,
                cy: 0.0,
                r: 2.0,
            },
            CircleDwell {
                min_inside_ms: Some(50),
                min_outside_ms: None,
            },
        )
        .unwrap();

        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 1.0,
                y: 0.0,
                t_ms: 0
            })
            .unwrap()
            .is_empty());

        let ev = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 1.0,
                y: 0.0,
                t_ms: 50,
            })
            .unwrap();
        assert_eq!(ev.len(), 1);
        assert!(
            matches!(&ev[0], Event::Approach { id, circle, t_ms: 50, .. } if id == "c1" && circle == "rad-1")
        );
    }

    #[test]
    fn circle_min_outside_ms_debounces_recede() {
        let mut e = Engine::new();
        e.register_circle_with_dwell(
            Circle {
                id: "rad-1".into(),
                cx: 0.0,
                cy: 0.0,
                r: 2.0,
            },
            CircleDwell {
                min_inside_ms: None,
                min_outside_ms: Some(30),
            },
        )
        .unwrap();

        // Enter immediately (no min_inside_ms).
        e.process_event(PointUpdate {
            id: "c1".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 0,
        })
        .unwrap();

        // Leave — exit pending starts.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 0
            })
            .unwrap()
            .is_empty());

        let ev = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 30,
            })
            .unwrap();
        assert_eq!(ev.len(), 1);
        assert!(
            matches!(&ev[0], Event::Recede { id, circle, t_ms: 30, .. } if id == "c1" && circle == "rad-1")
        );
    }

    #[test]
    fn circle_dwell_cancels_approach_on_bounce() {
        let mut e = Engine::new();
        e.register_circle_with_dwell(
            Circle {
                id: "rad-1".into(),
                cx: 0.0,
                cy: 0.0,
                r: 2.0,
            },
            CircleDwell {
                min_inside_ms: Some(100),
                min_outside_ms: None,
            },
        )
        .unwrap();

        // Enter circle — pending starts.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 1.0,
                y: 0.0,
                t_ms: 0
            })
            .unwrap()
            .is_empty());

        // Bounce out before threshold — pending cancels.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 50
            })
            .unwrap()
            .is_empty());

        // Re-enter — no Approach yet (fresh pending timer).
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 1.0,
                y: 0.0,
                t_ms: 60
            })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn circle_dwell_cancels_recede_on_re_entry() {
        let mut e = Engine::new();
        e.register_circle_with_dwell(
            Circle {
                id: "rad-1".into(),
                cx: 0.0,
                cy: 0.0,
                r: 2.0,
            },
            CircleDwell {
                min_inside_ms: None,
                min_outside_ms: Some(100),
            },
        )
        .unwrap();

        // Enter immediately (no min_inside_ms).
        e.process_event(PointUpdate {
            id: "c1".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 0,
        })
        .unwrap();

        // Leave — exit_pending starts.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 50,
            })
            .unwrap()
            .is_empty());

        // Re-enter before min_outside_ms elapses — pending cancels, no Recede.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 1.0,
                y: 0.0,
                t_ms: 60,
            })
            .unwrap()
            .is_empty());

        // Leave again — exit_pending restarts.
        assert!(e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 70,
            })
            .unwrap()
            .is_empty());

        // Threshold elapses — Recede fires.
        let ev = e
            .process_event(PointUpdate {
                id: "c1".into(),
                x: 10.0,
                y: 0.0,
                t_ms: 170,
            })
            .unwrap();
        assert_eq!(ev.len(), 1);
        assert!(
            matches!(&ev[0], Event::Recede { id, circle, t_ms: 170, .. } if id == "c1" && circle == "rad-1")
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

    // ---------------------------------------------------------------------------
    // Query API tests
    // ---------------------------------------------------------------------------

    #[test]
    fn entities_in_zone_returns_matching() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "depot".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "truck-1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 1,
        })
        .unwrap();

        let result = e.entities_in_zone("depot");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "truck-1");
    }

    #[test]
    fn entities_in_zone_excludes_outside() {
        let mut e = Engine::new();
        e.register_zone(Zone {
            id: "depot".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "truck-1".into(),
            x: 5.0,
            y: 5.0,
            t_ms: 1,
        })
        .unwrap();

        assert!(e.entities_in_zone("depot").is_empty());
    }

    #[test]
    fn entities_in_zone_unknown_zone_returns_empty() {
        let e = Engine::new();
        assert!(e.entities_in_zone("nonexistent").is_empty());
    }

    #[test]
    fn entities_in_circle_returns_matching() {
        let mut e = Engine::new();
        e.register_circle(Circle {
            id: "bay".into(),
            cx: 0.0,
            cy: 0.0,
            r: 2.0,
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "van-1".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();

        let result = e.entities_in_circle("bay");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "van-1");
    }

    #[test]
    fn entities_in_region_returns_matching() {
        let mut e = Engine::new();
        e.register_catalog_region(Zone {
            id: "north".into(),
            polygon: unit_square(),
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "driver-5".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 1,
        })
        .unwrap();

        let result = e.entities_in_region("north");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "driver-5");
    }

    #[test]
    fn entities_near_point_sorted_by_distance() {
        let mut e = Engine::new();
        // truck-far at distance 5, truck-near at distance 1
        e.process_event(PointUpdate {
            id: "truck-far".into(),
            x: 5.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "truck-near".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();

        let result = e.entities_near_point(0.0, 0.0, 10.0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "truck-near");
        assert!((result[0].2 - 1.0).abs() < 1e-9);
        assert_eq!(result[1].0, "truck-far");
        assert!((result[1].2 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn entities_near_point_excludes_beyond_radius() {
        let mut e = Engine::new();
        e.process_event(PointUpdate {
            id: "near".into(),
            x: 1.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();
        e.process_event(PointUpdate {
            id: "far".into(),
            x: 100.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();

        let result = e.entities_near_point(0.0, 0.0, 5.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "near");
    }

    #[test]
    fn nearest_to_point_respects_k() {
        let mut e = Engine::new();
        for i in 1..=5u32 {
            e.process_event(PointUpdate {
                id: format!("e{i}"),
                x: i as f64,
                y: 0.0,
                t_ms: i as u64,
            })
            .unwrap();
        }

        let result = e.nearest_to_point(0.0, 0.0, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "e1");
        assert_eq!(result[1].0, "e2");
    }

    #[test]
    fn nearest_to_point_excludes_entities_without_position() {
        // An entity with no position can't be produced by process_event (position is always set).
        // This test confirms that entities_near_point and nearest_to_point only return entities
        // with a known position — enforced by the filter_map(|..| st.position?).
        let mut e = Engine::new();
        e.process_event(PointUpdate {
            id: "known".into(),
            x: 2.0,
            y: 0.0,
            t_ms: 1,
        })
        .unwrap();

        // k larger than entity count: should return only those with positions.
        let result = e.nearest_to_point(0.0, 0.0, 100);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "known");
    }
}
