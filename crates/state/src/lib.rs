//! Per-entity geofence membership and deterministic spatial events.

use std::collections::{BTreeSet, HashMap};

/// Minimum continuous time inside / outside before emitting enter / exit for a geofence.
///
/// `None` for a field means **no minimum** (immediate enter or exit when geometry changes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeofenceDwell {
    /// Emit [`Event::Enter`] only after the point has been inside the polygon for this many ms.
    pub min_inside_ms: Option<u64>,
    /// Emit [`Event::Exit`] only after the point has been outside for this many ms (debounced exit).
    pub min_outside_ms: Option<u64>,
}

/// Last known position, observation time, and spatial membership used by the engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityState {
    pub position: Option<(f64, f64)>,
    /// Milliseconds since Unix epoch for the last processed update (`None` if never updated).
    pub last_t_ms: Option<u64>,
    pub inside: BTreeSet<String>,
    /// Geofence id → first `at_ms` seen inside while waiting for [`GeofenceDwell::min_inside_ms`].
    pub geofence_enter_pending: HashMap<String, u64>,
    /// Geofence id → first `at_ms` seen outside while logically inside (waiting for [`GeofenceDwell::min_outside_ms`]).
    pub geofence_exit_pending: HashMap<String, u64>,
    pub inside_corridor: BTreeSet<String>,
    pub inside_radius: BTreeSet<String>,
    pub catalog_region: Option<String>,
}

/// Emitted when spatial relationships change between updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Enter {
        id: String,
        geofence: String,
        t_ms: u64,
    },
    Exit {
        id: String,
        geofence: String,
        t_ms: u64,
    },
    EnterCorridor {
        id: String,
        corridor: String,
        t_ms: u64,
    },
    ExitCorridor {
        id: String,
        corridor: String,
        t_ms: u64,
    },
    Approach {
        id: String,
        zone: String,
        t_ms: u64,
    },
    Recede {
        id: String,
        zone: String,
        t_ms: u64,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t_ms: u64,
    },
}

/// Compute enter/exit events from geofence membership set diff.
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
            geofence: gid.clone(),
            t_ms,
        });
    }
    for gid in previous.difference(current) {
        out.push(Event::Exit {
            id: entity_id.to_string(),
            geofence: gid.clone(),
            t_ms,
        });
    }
    out
}

pub fn corridor_membership_transitions(
    entity_id: &str,
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
    t_ms: u64,
) -> Vec<Event> {
    let mut out = Vec::new();
    for gid in current.difference(previous) {
        out.push(Event::EnterCorridor {
            id: entity_id.to_string(),
            corridor: gid.clone(),
            t_ms,
        });
    }
    for gid in previous.difference(current) {
        out.push(Event::ExitCorridor {
            id: entity_id.to_string(),
            corridor: gid.clone(),
            t_ms,
        });
    }
    out
}

pub fn radius_membership_transitions(
    entity_id: &str,
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
    t_ms: u64,
) -> Vec<Event> {
    let mut out = Vec::new();
    for gid in current.difference(previous) {
        out.push(Event::Approach {
            id: entity_id.to_string(),
            zone: gid.clone(),
            t_ms,
        });
    }
    for gid in previous.difference(current) {
        out.push(Event::Recede {
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

/// Geofence enter/exit with optional dwell / exit debounce.
///
/// `physical_inside` is the polygon query at the current point. `logical_inside` is the set the
/// engine treats as “inside” for events (updated by this function). Pending maps cancel when the
/// entity bounces before thresholds elapse.
#[allow(clippy::too_many_arguments)]
pub fn geofence_membership_with_dwell(
    entity_id: &str,
    at_ms: u64,
    physical_inside: &BTreeSet<String>,
    logical_inside: &mut BTreeSet<String>,
    enter_pending: &mut HashMap<String, u64>,
    exit_pending: &mut HashMap<String, u64>,
    dwell_by_id: &HashMap<String, GeofenceDwell>,
    out: &mut Vec<Event>,
) {
    let mut zone_ids: BTreeSet<String> = logical_inside.iter().cloned().collect();
    zone_ids.extend(physical_inside.iter().cloned());
    zone_ids.extend(enter_pending.keys().cloned());
    zone_ids.extend(exit_pending.keys().cloned());

    for z in zone_ids {
        let dwell = dwell_by_id.get(&z).cloned().unwrap_or_default();
        let min_in = dwell.min_inside_ms.unwrap_or(0);
        let min_out = dwell.min_outside_ms.unwrap_or(0);
        let phys = physical_inside.contains(&z);
        let log = logical_inside.contains(&z);

        if phys && log {
            enter_pending.remove(&z);
            exit_pending.remove(&z);
            continue;
        }

        if phys && !log {
            exit_pending.remove(&z);
            if min_in == 0 {
                logical_inside.insert(z.clone());
                enter_pending.remove(&z);
                out.push(Event::Enter {
                    id: entity_id.to_string(),
                    geofence: z.clone(),
                    t_ms: at_ms,
                });
            } else {
                match enter_pending.get(&z).copied() {
                    None => {
                        enter_pending.insert(z.clone(), at_ms);
                    }
                    Some(t0) if at_ms.saturating_sub(t0) >= min_in => {
                        logical_inside.insert(z.clone());
                        enter_pending.remove(&z);
                        out.push(Event::Enter {
                            id: entity_id.to_string(),
                            geofence: z.clone(),
                            t_ms: at_ms,
                        });
                    }
                    Some(_) => {}
                }
            }
            continue;
        }

        if !phys && log {
            enter_pending.remove(&z);
            if min_out == 0 {
                logical_inside.remove(&z);
                exit_pending.remove(&z);
                out.push(Event::Exit {
                    id: entity_id.to_string(),
                    geofence: z.clone(),
                    t_ms: at_ms,
                });
            } else {
                match exit_pending.get(&z).copied() {
                    None => {
                        exit_pending.insert(z.clone(), at_ms);
                    }
                    Some(t0) if at_ms.saturating_sub(t0) >= min_out => {
                        logical_inside.remove(&z);
                        exit_pending.remove(&z);
                        out.push(Event::Exit {
                            id: entity_id.to_string(),
                            geofence: z.clone(),
                            t_ms: at_ms,
                        });
                    }
                    Some(_) => {}
                }
            }
            continue;
        }

        enter_pending.remove(&z);
        exit_pending.remove(&z);
    }
}

/// Stable ordering: entity id, observation time, tier, zone id, enter/approach before exit/recede.
pub fn sort_events_deterministic(events: &mut [Event]) {
    events.sort_by(|a, b| event_ord_key(a).cmp(&event_ord_key(b)));
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventTier {
    Geofence = 0,
    Corridor = 1,
    Radius = 2,
    Assignment = 3,
}

fn event_ord_key(e: &Event) -> (&str, u64, EventTier, &str, u8) {
    match e {
        Event::Enter { id, geofence, t_ms } => (
            id.as_str(),
            *t_ms,
            EventTier::Geofence,
            geofence.as_str(),
            0,
        ),
        Event::Exit { id, geofence, t_ms } => (
            id.as_str(),
            *t_ms,
            EventTier::Geofence,
            geofence.as_str(),
            1,
        ),
        Event::EnterCorridor { id, corridor, t_ms } => (
            id.as_str(),
            *t_ms,
            EventTier::Corridor,
            corridor.as_str(),
            0,
        ),
        Event::ExitCorridor { id, corridor, t_ms } => (
            id.as_str(),
            *t_ms,
            EventTier::Corridor,
            corridor.as_str(),
            1,
        ),
        Event::Approach { id, zone, t_ms } => {
            (id.as_str(), *t_ms, EventTier::Radius, zone.as_str(), 0)
        }
        Event::Recede { id, zone, t_ms } => {
            (id.as_str(), *t_ms, EventTier::Radius, zone.as_str(), 1)
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
            Event::Enter { id, geofence, t_ms: 42 } if id == "e1" && geofence == "z1"
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
                geofence: "f".into(),
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
            GeofenceDwell {
                min_inside_ms: Some(100),
                min_outside_ms: None,
            },
        );
        let phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log = BTreeSet::new();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        geofence_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());
        assert!(!log.contains("z"));

        geofence_membership_with_dwell(
            "e", 50, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert!(out.is_empty());

        geofence_membership_with_dwell(
            "e", 100, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(matches!(&out[0], Event::Enter { geofence, t_ms: 100, .. } if geofence == "z"));
        assert!(log.contains("z"));
    }

    #[test]
    fn dwell_min_outside_delays_exit() {
        let mut dwell = HashMap::new();
        dwell.insert(
            "z".into(),
            GeofenceDwell {
                min_inside_ms: None,
                min_outside_ms: Some(100),
            },
        );
        let mut phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        geofence_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());

        phys.clear();
        geofence_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        assert!(out.is_empty());
        assert!(log.contains("z"));

        geofence_membership_with_dwell(
            "e", 100, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(matches!(&out[0], Event::Exit { geofence, t_ms: 100, .. } if geofence == "z"));
        assert!(!log.contains("z"));
    }

    #[test]
    fn dwell_cancel_enter_on_bounce() {
        let mut dwell = HashMap::new();
        dwell.insert(
            "z".into(),
            GeofenceDwell {
                min_inside_ms: Some(100),
                min_outside_ms: None,
            },
        );
        let mut phys: BTreeSet<String> = ["z".into()].into_iter().collect();
        let mut log = BTreeSet::new();
        let mut ep = HashMap::new();
        let mut xp = HashMap::new();
        let mut out = Vec::new();

        geofence_membership_with_dwell("e", 0, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out);
        phys.clear();
        geofence_membership_with_dwell(
            "e", 50, &phys, &mut log, &mut ep, &mut xp, &dwell, &mut out,
        );
        assert!(out.is_empty());
        assert!(ep.is_empty());
    }
}
