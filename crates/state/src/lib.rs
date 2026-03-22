//! Per-entity geofence membership and deterministic spatial events.

use std::collections::BTreeSet;

/// Last known position, observation time, and spatial membership used by the engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityState {
    pub position: Option<(f64, f64)>,
    /// Milliseconds since Unix epoch for the last processed update (`None` if never updated).
    pub last_t_ms: Option<u64>,
    pub inside: BTreeSet<String>,
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
        Event::Enter {
            id,
            geofence,
            t_ms,
        } => (
            id.as_str(),
            *t_ms,
            EventTier::Geofence,
            geofence.as_str(),
            0,
        ),
        Event::Exit {
            id,
            geofence,
            t_ms,
        } => (
            id.as_str(),
            *t_ms,
            EventTier::Geofence,
            geofence.as_str(),
            1,
        ),
        Event::EnterCorridor {
            id,
            corridor,
            t_ms,
        } => (
            id.as_str(),
            *t_ms,
            EventTier::Corridor,
            corridor.as_str(),
            0,
        ),
        Event::ExitCorridor {
            id,
            corridor,
            t_ms,
        } => (
            id.as_str(),
            *t_ms,
            EventTier::Corridor,
            corridor.as_str(),
            1,
        ),
        Event::Approach {
            id,
            zone,
            t_ms,
        } => (id.as_str(), *t_ms, EventTier::Radius, zone.as_str(), 0),
        Event::Recede {
            id,
            zone,
            t_ms,
        } => (id.as_str(), *t_ms, EventTier::Radius, zone.as_str(), 1),
        Event::AssignmentChanged {
            id,
            region,
            t_ms,
        } => {
            let r = region.as_deref().unwrap_or("");
            (id.as_str(), *t_ms, EventTier::Assignment, r, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
