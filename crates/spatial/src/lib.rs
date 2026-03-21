//! Point-in-polygon checks and a naive linear-scan spatial index.

use geo::algorithm::contains::Contains;
use geo::{LineString, Point, Polygon};
use thiserror::Error;

/// A named geofence as a single polygon ring (holes not used in POC).
#[derive(Debug, Clone)]
pub struct Geofence {
    pub id: String,
    pub polygon: Polygon<f64>,
}

#[derive(Debug, Error)]
pub enum SpatialError {
    #[error("geofence id already registered: {0}")]
    DuplicateGeofenceId(String),
    #[error("polygon exterior must be a closed ring with at least 4 coordinates")]
    InvalidPolygon,
}

/// Spatial containment queries over registered geofences.
pub trait SpatialIndex {
    fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence>;
}

/// Linear scan over all fences — correct and simple; not for huge catalogs.
#[derive(Debug, Default)]
pub struct NaiveSpatialIndex {
    fences: Vec<Geofence>,
}

impl NaiveSpatialIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_push(&mut self, fence: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&fence.polygon)?;
        if self.fences.iter().any(|f| f.id == fence.id) {
            return Err(SpatialError::DuplicateGeofenceId(fence.id.clone()));
        }
        self.fences.push(fence);
        Ok(())
    }
}

impl SpatialIndex for NaiveSpatialIndex {
    fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence> {
        let pt = Point::new(point.0, point.1);
        self
            .fences
            .iter()
            .filter(|f| f.polygon.contains(&pt))
            .collect()
    }
}

pub fn point_in_polygon(point: (f64, f64), polygon: &Polygon<f64>) -> bool {
    let pt = Point::new(point.0, point.1);
    polygon.contains(&pt)
}

fn validate_polygon(polygon: &Polygon<f64>) -> Result<(), SpatialError> {
    let exterior: &LineString<f64> = polygon.exterior();
    let n = exterior.coords().count();
    if n < 4 {
        return Err(SpatialError::InvalidPolygon);
    }
    if !exterior.is_closed() {
        return Err(SpatialError::InvalidPolygon);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Polygon<f64> {
        Polygon::new(
            LineString::from(vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ]),
            vec![],
        )
    }

    #[test]
    fn inside_and_outside() {
        let p = square();
        assert!(point_in_polygon((5.0, 5.0), &p));
        assert!(!point_in_polygon((50.0, 5.0), &p));
    }

    #[test]
    fn naive_index_finds_fence() {
        let mut idx = NaiveSpatialIndex::new();
        idx
            .try_push(Geofence {
                id: "a".into(),
                polygon: square(),
            })
            .unwrap();
        let hits = idx.containing_geofences((5.0, 5.0));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "a");
    }
}
