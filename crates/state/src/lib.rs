//! Per-entity geofence membership and deterministic enter/exit events.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Last known position and which geofence ids currently contain the entity.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityState {
    pub position: Option<(f64, f64)>,
    pub inside: BTreeSet<String>,
}

/// Emitted when an entity crosses a geofence boundary between batches/updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    Enter { id: String, geofence: String },
    Exit { id: String, geofence: String },
}

/// Compute enter/exit events from membership set diff (unordered).
pub fn membership_transitions(
    entity_id: &str,
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
) -> Vec<Event> {
    let mut out = Vec::new();
    for gid in current.difference(previous) {
        out.push(Event::Enter {
            id: entity_id.to_string(),
            geofence: gid.clone(),
        });
    }
    for gid in previous.difference(current) {
        out.push(Event::Exit {
            id: entity_id.to_string(),
            geofence: gid.clone(),
        });
    }
    out
}

/// Stable ordering: entity id, geofence id, enter before exit.
pub fn sort_events_deterministic(events: &mut [Event]) {
    events.sort_by(|a, b| event_key(a).cmp(&event_key(b)));
}

fn event_key(e: &Event) -> (&str, &str, u8) {
    match e {
        Event::Enter { id, geofence } => (id.as_str(), geofence.as_str(), 0),
        Event::Exit { id, geofence } => (id.as_str(), geofence.as_str(), 1),
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
        let ev = membership_transitions("e1", &prev, &cur);
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            &ev[0],
            Event::Enter { id, geofence } if id == "e1" && geofence == "z1"
        ));
    }

    #[test]
    fn exit_when_leaving() {
        let mut prev = BTreeSet::new();
        prev.insert("z1".into());
        let cur = BTreeSet::new();
        let ev = membership_transitions("e1", &prev, &cur);
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], Event::Exit { .. }));
    }
}
