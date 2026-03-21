//! Pure, transport-agnostic geospatial stream engine: batch ingest, zone registration.

use spatial::NaiveSpatialIndex;
use state::{
    assignment_transition, corridor_membership_transitions, membership_transitions,
    radius_membership_transitions, sort_events_deterministic,
};
use std::collections::{BTreeSet, HashMap};
use thiserror::Error;

pub use spatial::{Geofence, RadiusZone, SpatialError};
pub use state::{EntityState, Event};

/// Single location observation for an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct PointUpdate {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Engine API: register zones and ingest batches of points.
pub trait GeoEngine {
    fn register_geofence(&mut self, geofence: Geofence) -> Result<(), EngineError>;
    fn register_corridor(&mut self, corridor: Geofence) -> Result<(), EngineError>;
    fn register_catalog_region(&mut self, region: Geofence) -> Result<(), EngineError>;
    fn register_radius_zone(&mut self, zone: RadiusZone) -> Result<(), EngineError>;
    fn ingest(&mut self, batch: Vec<PointUpdate>) -> Vec<Event>;
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Spatial(#[from] spatial::SpatialError),
}

/// In-memory engine: R-tree–accelerated polygon queries + per-entity membership state.
#[derive(Debug, Default)]
pub struct Engine {
    spatial: NaiveSpatialIndex,
    entities: HashMap<String, EntityState>,
    /// Reused between membership tiers to avoid cloning [`EntityState`] sets each ingest.
    membership_scratch: BTreeSet<String>,
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

    fn ingest(&mut self, mut batch: Vec<PointUpdate>) -> Vec<Event> {
        batch.sort_by(|a, b| a.id.cmp(&b.id));
        let mut events = Vec::new();

        let spatial = &self.spatial;
        let entities = &mut self.entities;
        let membership_scratch = &mut self.membership_scratch;

        for update in batch {
            let p = (update.x, update.y);
            let st = entities.entry(update.id.clone()).or_default();
            let entity_id = update.id.as_str();

            membership_scratch.clear();
            spatial.geofence_membership_at(p, membership_scratch);
            events.extend(membership_transitions(
                entity_id,
                &st.inside,
                membership_scratch,
            ));
            std::mem::swap(&mut st.inside, membership_scratch);

            membership_scratch.clear();
            spatial.corridor_membership_at(p, membership_scratch);
            events.extend(corridor_membership_transitions(
                entity_id,
                &st.inside_corridor,
                membership_scratch,
            ));
            std::mem::swap(&mut st.inside_corridor, membership_scratch);

            membership_scratch.clear();
            spatial.radius_membership_at(p, membership_scratch);
            events.extend(radius_membership_transitions(
                entity_id,
                &st.inside_radius,
                membership_scratch,
            ));
            std::mem::swap(&mut st.inside_radius, membership_scratch);

            let new_catalog = spatial.primary_catalog_at(p);
            events.extend(assignment_transition(
                entity_id,
                &st.catalog_region,
                &new_catalog,
            ));
            st.catalog_region = new_catalog;
            st.position = Some(p);
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

        let ev1 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::AssignmentChanged { id, region: Some(r) } if id == "c1" && r == "region-a"
        ));

        let ev2 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::AssignmentChanged { id, region: None } if id == "c1"
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

        let ev1 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 1.0,
            y: 0.0,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::Approach { id, zone } if id == "c1" && zone == "rad-1"
        ));

        let ev2 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 10.0,
            y: 0.0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::Recede { id, zone } if id == "c1" && zone == "rad-1"
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

        let ev1 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 0.5,
            y: 0.5,
        }]);
        assert_eq!(ev1.len(), 1);
        assert!(matches!(
            &ev1[0],
            Event::EnterCorridor { id, corridor } if id == "c1" && corridor == "cor-1"
        ));

        let ev2 = e.ingest(vec![PointUpdate {
            id: "c1".into(),
            x: 5.0,
            y: 5.0,
        }]);
        assert_eq!(ev2.len(), 1);
        assert!(matches!(
            &ev2[0],
            Event::ExitCorridor { id, corridor } if id == "c1" && corridor == "cor-1"
        ));
    }
}
