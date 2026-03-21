//! Pure, transport-agnostic geospatial stream engine: batch ingest, geofence registration.

use serde::{Deserialize, Serialize};
use spatial::{NaiveSpatialIndex, SpatialIndex};
use state::{membership_transitions, sort_events_deterministic};
use std::collections::{BTreeSet, HashMap};
use thiserror::Error;

pub use spatial::{Geofence, SpatialError};
pub use state::{EntityState, Event};

/// Single location observation for an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointUpdate {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Engine API: register fences and ingest batches of points.
pub trait GeoEngine {
    fn register_geofence(&mut self, geofence: Geofence) -> Result<(), EngineError>;
    fn ingest(&mut self, batch: Vec<PointUpdate>) -> Vec<Event>;
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Spatial(#[from] spatial::SpatialError),
}

/// In-memory engine: naive spatial index + per-entity membership state.
#[derive(Debug, Default)]
pub struct Engine {
    spatial: NaiveSpatialIndex,
    entities: HashMap<String, EntityState>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }
}

impl GeoEngine for Engine {
    fn register_geofence(&mut self, geofence: Geofence) -> Result<(), EngineError> {
        self.spatial.try_push(geofence)?;
        Ok(())
    }

    fn ingest(&mut self, mut batch: Vec<PointUpdate>) -> Vec<Event> {
        batch.sort_by(|a, b| a.id.cmp(&b.id));
        let mut events = Vec::new();
        for update in batch {
            let prev_inside = self
                .entities
                .get(&update.id)
                .map(|s| s.inside.clone())
                .unwrap_or_default();
            let containing = self.spatial.containing_geofences((update.x, update.y));
            let new_inside: BTreeSet<String> =
                containing.into_iter().map(|g| g.id.clone()).collect();
            events.extend(membership_transitions(
                &update.id,
                &prev_inside,
                &new_inside,
            ));
            let st = self.entities.entry(update.id.clone()).or_default();
            st.position = Some((update.x, update.y));
            st.inside = new_inside;
        }
        sort_events_deterministic(&mut events);
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
    fn enter_then_exit_square() {
        let mut e = Engine::new();
        e.register_geofence(Geofence {
            id: "zone-1".into(),
            polygon: unit_square(),
        })
        .unwrap();

        let ev1 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Enter { id, geofence } if id == "c1" && geofence == "zone-1"
        ));

        let ev2 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Exit { id, geofence } if id == "c1" && geofence == "zone-1"
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
            },
            PointUpdate {
                id: "a".into(),
                x: 0.5,
                y: 0.5,
            },
        ];
        let ev = e.ingest(batch);
        assert_eq!(ev.len(), 2);
        assert!(matches!(&ev[0], Event::Enter { id, .. } if id == "a"));
        assert!(matches!(&ev[1], Event::Enter { id, .. } if id == "b"));
    }
}
