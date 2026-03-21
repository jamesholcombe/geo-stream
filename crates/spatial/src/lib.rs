//! Point-in-polygon checks, disk containment, and naive linear-scan indices.

use geo::algorithm::contains::Contains;
use geo::{LineString, Point, Polygon};
use std::collections::BTreeSet;
use thiserror::Error;

/// A named geofence as a single polygon ring (holes not used in POC).
#[derive(Debug, Clone)]
pub struct Geofence {
    pub id: String,
    pub polygon: Polygon<f64>,
}

/// Fixed center + radius disk in the same planar CRS as polygons (Euclidean distance).
#[derive(Debug, Clone)]
pub struct RadiusZone {
    pub id: String,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

impl RadiusZone {
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        let dx = x - self.cx;
        let dy = y - self.cy;
        dx * dx + dy * dy <= self.r * self.r
    }
}

#[derive(Debug, Error)]
pub enum SpatialError {
    #[error("zone id already registered: {0}")]
    DuplicateZoneId(String),
    #[error("polygon exterior must be a closed ring with at least 4 coordinates")]
    InvalidPolygon,
    #[error("radius must be positive")]
    InvalidRadius,
}

/// Spatial containment queries over registered geofences.
pub trait SpatialIndex {
    fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence>;
}

/// Linear scan over all zones — correct and simple; not for huge catalogs.
#[derive(Debug, Default)]
pub struct NaiveSpatialIndex {
    fences: Vec<Geofence>,
    corridors: Vec<Geofence>,
    catalog: Vec<Geofence>,
    radius_zones: Vec<RadiusZone>,
}

impl NaiveSpatialIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn id_exists(&self, id: &str) -> bool {
        self.fences
            .iter()
            .chain(self.corridors.iter())
            .chain(self.catalog.iter())
            .any(|g| g.id == id)
            || self.radius_zones.iter().any(|z| z.id == id)
    }

    /// Register a geofence (enter/exit events).
    pub fn try_push(&mut self, fence: Geofence) -> Result<(), SpatialError> {
        self.try_push_geofence(fence)
    }

    pub fn try_push_geofence(&mut self, fence: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&fence.polygon)?;
        if self.id_exists(&fence.id) {
            return Err(SpatialError::DuplicateZoneId(fence.id.clone()));
        }
        self.fences.push(fence);
        Ok(())
    }

    /// Register a corridor as a pre-buffered polygon (`enter_corridor` / `exit_corridor` events).
    pub fn try_push_corridor(&mut self, corridor: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&corridor.polygon)?;
        if self.id_exists(&corridor.id) {
            return Err(SpatialError::DuplicateZoneId(corridor.id.clone()));
        }
        self.corridors.push(corridor);
        Ok(())
    }

    /// Register a catalog region (`assignment_changed` events; tie-break: lexicographically smallest id).
    pub fn try_push_catalog_region(&mut self, region: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&region.polygon)?;
        if self.id_exists(&region.id) {
            return Err(SpatialError::DuplicateZoneId(region.id.clone()));
        }
        self.catalog.push(region);
        Ok(())
    }

    pub fn try_push_radius_zone(&mut self, zone: RadiusZone) -> Result<(), SpatialError> {
        if zone.r <= 0.0 || !zone.r.is_finite() {
            return Err(SpatialError::InvalidRadius);
        }
        if !zone.cx.is_finite() || !zone.cy.is_finite() {
            return Err(SpatialError::InvalidRadius);
        }
        if self.id_exists(&zone.id) {
            return Err(SpatialError::DuplicateZoneId(zone.id.clone()));
        }
        self.radius_zones.push(zone);
        Ok(())
    }

    pub fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.fences, point)
    }

    pub fn containing_corridors(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.corridors, point)
    }

    pub fn containing_catalog_regions(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.catalog, point)
    }

    pub fn containing_radius_zones(&self, point: (f64, f64)) -> Vec<&RadiusZone> {
        self.radius_zones
            .iter()
            .filter(|z| z.contains_point(point.0, point.1))
            .collect()
    }

    /// Clears `out` and inserts ids of geofences whose polygon contains `point`.
    pub fn geofence_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        out.clear();
        fill_polygon_zone_ids(&self.fences, point, out);
    }

    /// Clears `out` and inserts ids of corridors whose polygon contains `point`.
    pub fn corridor_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        out.clear();
        fill_polygon_zone_ids(&self.corridors, point, out);
    }

    /// Clears `out` and inserts ids of radius zones containing `point`.
    pub fn radius_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        out.clear();
        for z in &self.radius_zones {
            if z.contains_point(point.0, point.1) {
                out.insert(z.id.clone());
            }
        }
    }

    /// Lexicographically smallest catalog region id among polygons containing `point`, if any.
    pub fn primary_catalog_at(&self, point: (f64, f64)) -> Option<String> {
        let pt = Point::new(point.0, point.1);
        let mut min_id: Option<&str> = None;
        for f in &self.catalog {
            if f.polygon.contains(&pt) {
                let id = f.id.as_str();
                if min_id.map_or(true, |m| id < m) {
                    min_id = Some(id);
                }
            }
        }
        min_id.map(String::from)
    }
}

impl SpatialIndex for NaiveSpatialIndex {
    fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.fences, point)
    }
}

fn containing_polygons(fences: &[Geofence], point: (f64, f64)) -> Vec<&Geofence> {
    let pt = Point::new(point.0, point.1);
    fences
        .iter()
        .filter(|f| f.polygon.contains(&pt))
        .collect()
}

fn fill_polygon_zone_ids(zones: &[Geofence], point: (f64, f64), out: &mut BTreeSet<String>) {
    let pt = Point::new(point.0, point.1);
    for f in zones {
        if f.polygon.contains(&pt) {
            out.insert(f.id.clone());
        }
    }
}

/// When multiple catalog polygons contain the point, choose the lexicographically smallest id.
pub fn primary_catalog_region(containing: &[&Geofence]) -> Option<String> {
    containing
        .iter()
        .map(|g| g.id.as_str())
        .min()
        .map(String::from)
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

    #[test]
    fn radius_on_boundary_counts_inside() {
        let z = RadiusZone {
            id: "r1".into(),
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
        };
        assert!(z.contains_point(1.0, 0.0));
        assert!(!z.contains_point(1.01, 0.0));
    }

    #[test]
    fn primary_catalog_tie_break() {
        let a = Geofence {
            id: "b".into(),
            polygon: square(),
        };
        let b = Geofence {
            id: "a".into(),
            polygon: square(),
        };
        let refs = vec![&a, &b];
        assert_eq!(primary_catalog_region(&refs), Some("a".into()));
    }

    #[test]
    fn primary_catalog_at_matches_region_refs() {
        let mut idx = NaiveSpatialIndex::new();
        idx
            .try_push_catalog_region(Geofence {
                id: "b".into(),
                polygon: square(),
            })
            .unwrap();
        idx
            .try_push_catalog_region(Geofence {
                id: "a".into(),
                polygon: square(),
            })
            .unwrap();
        assert_eq!(idx.primary_catalog_at((5.0, 5.0)), Some("a".into()));
        assert_eq!(idx.primary_catalog_at((50.0, 5.0)), None);
    }

    #[test]
    fn duplicate_id_across_kinds_rejected() {
        let mut idx = NaiveSpatialIndex::new();
        idx
            .try_push(Geofence {
                id: "x".into(),
                polygon: square(),
            })
            .unwrap();
        let err = idx
            .try_push_radius_zone(RadiusZone {
                id: "x".into(),
                cx: 0.0,
                cy: 0.0,
                r: 1.0,
            })
            .unwrap_err();
        assert!(matches!(err, SpatialError::DuplicateZoneId(_)));
    }
}
