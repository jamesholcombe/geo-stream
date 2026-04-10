//! Per-entity zone membership and deterministic spatial events.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Minimum continuous time inside / outside before emitting enter / exit for a zone.
///
/// `None` for a field means **no minimum** (immediate enter or exit when geometry changes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneDwell {
    /// Emit [`Event::Enter`] only after the point has been inside the polygon for this many ms.
    pub min_inside_ms: Option<u64>,
    /// Emit [`Event::Exit`] only after the point has been outside for this many ms (debounced exit).
    pub min_outside_ms: Option<u64>,
}

/// Minimum continuous time inside / outside before emitting approach / recede for a circle.
///
/// `None` for a field means **no minimum** (immediate approach or recede when membership changes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CircleDwell {
    /// Emit [`Event::Approach`] only after the point has been inside the circle for this many ms.
    pub min_inside_ms: Option<u64>,
    /// Emit [`Event::Recede`] only after the point has been outside for this many ms.
    pub min_outside_ms: Option<u64>,
}

/// A single historical position sample stored in the entity's ring buffer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub x: f64,
    pub y: f64,
    pub t_ms: u64,
}

/// Last known position, observation time, and spatial membership used by the engine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub position: Option<(f64, f64)>,
    /// Milliseconds since Unix epoch for the last processed update (`None` if never updated).
    pub last_t_ms: Option<u64>,
    /// Speed in units/second computed from the last two positions. `None` until two updates exist.
    pub speed: Option<f64>,
    /// Heading in degrees (0–360, north-up, clockwise) computed from the last two positions.
    pub heading: Option<f64>,
    /// Ring buffer of recent positions (capacity controlled by the engine). Oldest first.
    pub history: VecDeque<HistoryPoint>,
    pub inside: BTreeSet<String>,
    /// Zone id → first `at_ms` seen inside while waiting for [`ZoneDwell::min_inside_ms`].
    pub zone_enter_pending: HashMap<String, u64>,
    /// Zone id → first `at_ms` seen outside while logically inside (waiting for [`ZoneDwell::min_outside_ms`]).
    pub zone_exit_pending: HashMap<String, u64>,
    pub inside_circle: BTreeSet<String>,
    /// Circle id → first `at_ms` seen inside while waiting for [`CircleDwell::min_inside_ms`].
    pub circle_enter_pending: HashMap<String, u64>,
    /// Circle id → first `at_ms` seen outside while logically inside (waiting for [`CircleDwell::min_outside_ms`]).
    pub circle_exit_pending: HashMap<String, u64>,
    pub catalog_region: Option<String>,
}

/// Emitted when spatial relationships change between updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Enter {
        id: String,
        zone: String,
        t_ms: u64,
    },
    Exit {
        id: String,
        zone: String,
        t_ms: u64,
    },
    Approach {
        id: String,
        circle: String,
        t_ms: u64,
    },
    Recede {
        id: String,
        circle: String,
        t_ms: u64,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t_ms: u64,
    },
}

/// Compute enter/exit events from zone membership set diff.
pub fn membership_transitions(
    entity_id: &str,
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
    t_ms: u64,
) -> Vec<Event> {
    let mut out = Vec::new();
    for gid in current.difference(previous) {
        out.push(Event::Enter {
            id: entity_id.to_string(),
            zone: gid.clone(),
            t_ms,
        });
    }
    for gid in previous.difference(current) {
        out.push(Event::Exit {
            id: entity_id.to_string(),
            zone: gid.clone(),
            t_ms,
        });
    }
    out
}

pub fn assignment_transition(
    entity_id: &str,
    previous: &Option<String>,
    current: &Option<String>,
    t_ms: u64,
) -> Vec<Event> {
    if previous == current {
        Vec::new()
    } else {
        vec![Event::AssignmentChanged {
            id: entity_id.to_string(),
            region: current.clone(),
            t_ms,
        }]
    }
}

/// Shared dwell logic for zone and circle membership.
///
/// `get_thresholds` returns `(min_inside_ms, min_outside_ms)` for a geometry id.
/// `make_enter` / `make_exit` construct the appropriate event variant
/// (`Enter`/`Exit` for zones, `Approach`/`Recede` for circles).
#[allow(clippy::too_many_arguments)]
fn membership_with_dwell_impl(
    entity_id: &str,
    at_ms: u64,
    physical_inside: &BTreeSet<String>,
    logical_inside: &mut BTreeSet<String>,
    enter_pending: &mut HashMap<String, u64>,
    exit_pending: &mut HashMap<String, u64>,
    get_thresholds: impl Fn(&str) -> (u64, u64),
    make_enter: impl Fn(String, String, u64) -> Event,
    make_exit: impl Fn(String, String, u64) -> Event,
    out: &mut Vec<Event>,
) {
    let mut ids: BTreeSet<String> = logical_inside.iter().cloned().collect();
    ids.extend(physical_inside.iter().cloned());
    ids.extend(enter_pending.keys().cloned());
    ids.extend(exit_pending.keys().cloned());

    for id in ids {
        let (min_in, min_out) = get_thresholds(&id);
        let phys = physical_inside.contains(&id);
        let log = logical_inside.contains(&id);

        if phys && log {
            enter_pending.remove(&id);
            exit_pending.remove(&id);
            continue;
        }

        if phys && !log {
            exit_pending.remove(&id);
            if min_in == 0 {
                logical_inside.insert(id.clone());
                enter_pending.remove(&id);
                out.push(make_enter(entity_id.to_string(), id.clone(), at_ms));
            } else {
                match enter_pending.get(&id).copied() {
                    None => {
                        enter_pending.insert(id.clone(), at_ms);
                    }
                    Some(t0) if at_ms.saturating_sub(t0) >= min_in => {
                        logical_inside.insert(id.clone());
                        enter_pending.remove(&id);
                        out.push(make_enter(entity_id.to_string(), id.clone(), at_ms));
                    }
                    Some(_) => {}
                }
            }
            continue;
        }

        if !phys && log {
            enter_pending.remove(&id);
            if min_out == 0 {
                logical_inside.remove(&id);
                exit_pending.remove(&id);
                out.push(make_exit(entity_id.to_string(), id.clone(), at_ms));
            } else {
                match exit_pending.get(&id).copied() {
                    None => {
                        exit_pending.insert(id.clone(), at_ms);
                    }
                    Some(t0) if at_ms.saturating_sub(t0) >= min_out => {
                        logical_inside.remove(&id);
                        exit_pending.remove(&id);
                        out.push(make_exit(entity_id.to_string(), id.clone(), at_ms));
                    }
                    Some(_) => {}
                }
            }
            continue;
        }

        enter_pending.remove(&id);
        exit_pending.remove(&id);
    }
}

/// Zone enter/exit with optional dwell / exit debounce.
///
/// `physical_inside` is the polygon query at the current point. `logical_inside` is the set the
/// engine treats as "inside" for events (updated by this function). Pending maps cancel when the
/// entity bounces before thresholds elapse.
#[allow(clippy::too_many_arguments)]
pub fn zone_membership_with_dwell(
    entity_id: &str,
    at_ms: u64,
    physical_inside: &BTreeSet<String>,
    logical_inside: &mut BTreeSet<String>,
    enter_pending: &mut HashMap<String, u64>,
    exit_pending: &mut HashMap<String, u64>,
    dwell_by_id: &HashMap<String, ZoneDwell>,
    out: &mut Vec<Event>,
) {
    membership_with_dwell_impl(
        entity_id,
        at_ms,
        physical_inside,
        logical_inside,
        enter_pending,
        exit_pending,
        |id| {
            let d = dwell_by_id.get(id).cloned().unwrap_or_default();
            (d.min_inside_ms.unwrap_or(0), d.min_outside_ms.unwrap_or(0))
        },
        |eid, zone, t_ms| Event::Enter {
            id: eid,
            zone,
            t_ms,
        },
        |eid, zone, t_ms| Event::Exit {
            id: eid,
            zone,
            t_ms,
        },
        out,
    );
}

/// Circle approach/recede with optional dwell / exit debounce.
#[allow(clippy::too_many_arguments)]
pub fn circle_membership_with_dwell(
    entity_id: &str,
    at_ms: u64,
    physical_inside: &BTreeSet<String>,
    logical_inside: &mut BTreeSet<String>,
    enter_pending: &mut HashMap<String, u64>,
    exit_pending: &mut HashMap<String, u64>,
    dwell_by_id: &HashMap<String, CircleDwell>,
    out: &mut Vec<Event>,
) {
    membership_with_dwell_impl(
        entity_id,
        at_ms,
        physical_inside,
        logical_inside,
        enter_pending,
        exit_pending,
        |id| {
            let d = dwell_by_id.get(id).cloned().unwrap_or_default();
            (d.min_inside_ms.unwrap_or(0), d.min_outside_ms.unwrap_or(0))
        },
        |eid, circle, t_ms| Event::Approach {
            id: eid,
            circle,
            t_ms,
        },
        |eid, circle, t_ms| Event::Recede {
            id: eid,
            circle,
            t_ms,
        },
        out,
    );
}

// ---------------------------------------------------------------------------
// Pluggable state store
// ---------------------------------------------------------------------------

/// Read/write access to per-entity state. Implement this to back the engine with an external
/// store (Redis, DynamoDB, etc.). The default implementation is [`MemoryStateStore`].
///
/// Methods return and accept owned [`EntityState`] values so implementations can serialize
/// state to/from an external system without borrow-checker friction.
pub trait StateStore: Send {
    /// Return a clone of the stored state for `id`, or `None` if the entity is unknown.
    fn get(&self, id: &str) -> Option<EntityState>;
    /// Insert or overwrite the state for `id`.
    fn set(&mut self, id: &str, state: EntityState);
    /// Remove the state entry for `id` if it exists.
    fn remove(&mut self, id: &str);
    /// Return a snapshot of all stored entities as owned pairs.
    fn all_entities(&self) -> Vec<(String, EntityState)>;
}

/// In-memory [`StateStore`] backed by a [`HashMap`]. Zero-overhead default.
#[derive(Debug, Default)]
pub struct MemoryStateStore(pub HashMap<String, EntityState>);

impl StateStore for MemoryStateStore {
    fn get(&self, id: &str) -> Option<EntityState> {
        self.0.get(id).cloned()
    }

    fn set(&mut self, id: &str, state: EntityState) {
        self.0.insert(id.to_string(), state);
    }

    fn remove(&mut self, id: &str) {
        self.0.remove(id);
    }

    fn all_entities(&self) -> Vec<(String, EntityState)> {
        self.0.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// Stable ordering: entity id, observation time, tier, zone id, enter/approach before exit/recede.
pub fn sort_events_deterministic(events: &mut [Event]) {
    events.sort_by(|a, b| event_ord_key(a).cmp(&event_ord_key(b)));
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventTier {
    Zone = 0,
    Circle = 1,
    Assignment = 2,
}

fn event_ord_key(e: &Event) -> (&str, u64, EventTier, &str, u8) {
    match e {
        Event::Enter { id, zone, t_ms } => (id.as_str(), *t_ms, EventTier::Zone, zone.as_str(), 0),
        Event::Exit { id, zone, t_ms } => (id.as_str(), *t_ms, EventTier::Zone, zone.as_str(), 1),
        Event::Approach { id, circle, t_ms } => {
            (id.as_str(), *t_ms, EventTier::Circle, circle.as_str(), 0)
        }
        Event::Recede { id, circle, t_ms } => {
            (id.as_str(), *t_ms, EventTier::Circle, circle.as_str(), 1)
        }
        Event::AssignmentChanged { id, region, t_ms } => {
            let r = region.as_deref().unwrap_or("");
            (id.as_str(), *t_ms, EventTier::Assignment, r, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn enter_only_on_new_membership() {
        let prev = BTreeSet::new();
        let mut cur = BTreeSet::new();
        cur.insert("z1".into());
        let ev = membership_transitions("e1", &prev, &cur, 42);
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            &ev[0],
            Event::Enter { id, zone, t_ms: 42 } if id == "e1" && zone == "z1"
        ));
    }

    #[test]
    fn exit_when_leaving() {
        let mut prev = BTreeSet::new();
        prev.insert("z1".into());
        let cur = BTreeSet::new();
        let ev = membership_transitions("e1", &prev, &cur, 0);
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], Event::Exit { .. }));
    }

    #[test]
    fn assignment_no_event_when_unchanged() {
        let prev = Some("a".into());
        let cur = Some("a".into());
        assert!(assignment_transition("e", &prev, &cur, 1).is_empty());
    }

    #[test]
    fn assignment_emits_when_changes() {
        let ev = assignment_transition("e", &None, &Some("r1".into()), 9);
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            &ev[0],
            Event::AssignmentChanged { id, region: Some(r), t_ms: 9 } if id == "e" && r == "r1"
        ));
    }

    #[test]
    fn sort_orders_tiers() {
        let mut ev = vec![
            Event::AssignmentChanged {
                id: "a".into(),
                region: Some("z".into()),
                t_ms: 1,
            },
            Event::Enter {
                id: "a".into(),
                zone: "f".into(),
                t_ms: 1,
            },
        ];
        sort_events_deterministic(&mut ev);
        assert!(matches!(&ev[0], Event::Enter { .. }));
    }

    #[test]
    fn dwell_min_inside_delays_enter() {
        let mut dwell = HashMap::new();
        dwell.insert(
            "z".into(),
            ZoneDwell {
                min_inside_ms: Some(100),
                min_outside_ms: None,
            },
        );
        let phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log = BTreeSet::new();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        zone_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());
        assert!(!log.contains("z"));

        zone_membership_with_dwell("e", 50, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());

        zone_membership_with_dwell(
            "e", 100, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(matches!(&out[0], Event::Enter { zone, t_ms: 100, .. } if zone == "z"));
        assert!(log.contains("z"));
    }

    #[test]
    fn dwell_min_outside_delays_exit() {
        let mut dwell = HashMap::new();
        dwell.insert(
            "z".into(),
            ZoneDwell {
                min_inside_ms: None,
                min_outside_ms: Some(100),
            },
        );
        let mut phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        zone_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());

        phys.clear();
        zone_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());
        assert!(log.contains("z"));

        zone_membership_with_dwell(
            "e", 100, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(matches!(&out[0], Event::Exit { zone, t_ms: 100, .. } if zone == "z"));
        assert!(!log.contains("z"));
    }

    #[test]
    fn dwell_cancel_enter_on_bounce() {
        let mut dwell = HashMap::new();
        dwell.insert(
            "z".into(),
            ZoneDwell {
                min_inside_ms: Some(100),
                min_outside_ms: None,
            },
        );
        let mut phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log = BTreeSet::new();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        zone_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        phys.clear();
        zone_membership_with_dwell("e", 50, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());
        assert!(ep.is_empty());
    }
}
