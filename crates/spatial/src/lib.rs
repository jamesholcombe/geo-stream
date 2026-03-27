//! Point-in-polygon checks, disk containment, and R-tree–accelerated polygon indices.

use geo::algorithm::bounding_rect::BoundingRect;
use geo::algorithm::contains::Contains;
use geo::{LineString, Point, Polygon};
use geojson::Geometry;
use rstar::{RTree, RTreeObject, AABB};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use thiserror::Error;

// ---------------------------------------------------------------------------
// GeoJSON polygon parsing (previously the standalone `polygon-json` crate)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PolygonJsonError {
    #[error("invalid GeoJSON geometry: {0}")]
    InvalidGeometry(String),
    #[error("unsupported geometry for geofence")]
    UnsupportedGeometry,
    #[error("geofence polygon must be a GeoJSON Polygon")]
    NotPolygon,
}

/// Parse `value` as a GeoJSON Polygon geometry and return a [`geo::Polygon`] with all rings
/// (exterior plus any interior holes).
pub fn polygon_from_json_value(value: &Value) -> Result<Polygon<f64>, PolygonJsonError> {
    let geom: Geometry = serde_json::from_value(value.clone())
        .map_err(|e| PolygonJsonError::InvalidGeometry(e.to_string()))?;
    let g: geo::Geometry<f64> = geom
        .try_into()
        .map_err(|_| PolygonJsonError::UnsupportedGeometry)?;
    match g {
        geo::Geometry::Polygon(p) => Ok(p),
        _ => Err(PolygonJsonError::NotPolygon),
    }
}

/// R-tree wrapper for a radius zone: stores the zone's index in the Vec and its AABB.
#[derive(Clone, Copy)]
struct IndexedRadius {
    index: usize,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for IndexedRadius {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

fn radius_aabb(cx: f64, cy: f64, r: f64) -> AABB<[f64; 2]> {
    AABB::from_corners([cx - r, cy - r], [cx + r, cy + r])
}

/// A named geofence as a polygon (exterior ring plus optional interior holes).
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
    fn geofence_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>);
    fn corridor_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>);
    fn radius_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>);
    fn primary_catalog_at(&self, point: (f64, f64)) -> Option<String>;
}

/// R-tree index on polygon bounding boxes with exact `contains` refinement; radius zones also R-tree indexed.
pub struct NaiveSpatialIndex {
    fences: Vec<Geofence>,
    fence_tree: RTree<IndexedPolygon>,
    corridors: Vec<Geofence>,
    corridor_tree: RTree<IndexedPolygon>,
    catalog: Vec<Geofence>,
    catalog_tree: RTree<IndexedPolygon>,
    radius_zones: Vec<RadiusZone>,
    radius_tree: RTree<IndexedRadius>,
}

impl Default for NaiveSpatialIndex {
    fn default() -> Self {
        Self {
            fences: Vec::new(),
            fence_tree: RTree::new(),
            corridors: Vec::new(),
            corridor_tree: RTree::new(),
            catalog: Vec::new(),
            catalog_tree: RTree::new(),
            radius_zones: Vec::new(),
            radius_tree: RTree::new(),
        }
    }
}

impl fmt::Debug for NaiveSpatialIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NaiveSpatialIndex")
            .field("fences", &self.fences.len())
            .field("corridors", &self.corridors.len())
            .field("catalog", &self.catalog.len())
            .field("radius_zones", &self.radius_zones.len())
            .finish()
    }
}

#[derive(Clone, Copy)]
struct IndexedPolygon {
    index: usize,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for IndexedPolygon {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

fn polygon_aabb(polygon: &Polygon<f64>) -> Result<AABB<[f64; 2]>, SpatialError> {
    let rect = polygon
        .bounding_rect()
        .ok_or(SpatialError::InvalidPolygon)?;
    let min = rect.min();
    let max = rect.max();
    Ok(AABB::from_corners([min.x, min.y], [max.x, max.y]))
}

fn point_probe_envelope(point: (f64, f64)) -> AABB<[f64; 2]> {
    AABB::from_point([point.0, point.1])
}

fn containing_polygons<'a>(
    zones: &'a [Geofence],
    tree: &RTree<IndexedPolygon>,
    point: (f64, f64),
) -> Vec<&'a Geofence> {
    let pt = Point::new(point.0, point.1);
    let probe = point_probe_envelope(point);
    let mut out = Vec::new();
    for obj in tree.locate_in_envelope_intersecting(&probe) {
        let z = &zones[obj.index];
        if z.polygon.contains(&pt) {
            out.push(z);
        }
    }
    out
}

fn fill_polygon_zone_ids(
    zones: &[Geofence],
    tree: &RTree<IndexedPolygon>,
    point: (f64, f64),
    out: &mut BTreeSet<String>,
) {
    out.clear();
    let pt = Point::new(point.0, point.1);
    let probe = point_probe_envelope(point);
    for obj in tree.locate_in_envelope_intersecting(&probe) {
        let z = &zones[obj.index];
        if z.polygon.contains(&pt) {
            out.insert(z.id.clone());
        }
    }
}

fn primary_catalog_at_indexed(
    catalog: &[Geofence],
    tree: &RTree<IndexedPolygon>,
    point: (f64, f64),
) -> Option<String> {
    let pt = Point::new(point.0, point.1);
    let probe = point_probe_envelope(point);
    let mut min_id: Option<&str> = None;
    for obj in tree.locate_in_envelope_intersecting(&probe) {
        let f = &catalog[obj.index];
        if f.polygon.contains(&pt) {
            let id = f.id.as_str();
            if min_id.is_none_or(|m| id < m) {
                min_id = Some(id);
            }
        }
    }
    min_id.map(String::from)
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
        let env = polygon_aabb(&fence.polygon)?;
        self.fences.push(fence);
        let index = self.fences.len() - 1;
        self.fence_tree.insert(IndexedPolygon {
            index,
            envelope: env,
        });
        Ok(())
    }

    /// Register a corridor as a pre-buffered polygon (`enter_corridor` / `exit_corridor` events).
    pub fn try_push_corridor(&mut self, corridor: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&corridor.polygon)?;
        if self.id_exists(&corridor.id) {
            return Err(SpatialError::DuplicateZoneId(corridor.id.clone()));
        }
        let env = polygon_aabb(&corridor.polygon)?;
        self.corridors.push(corridor);
        let index = self.corridors.len() - 1;
        self.corridor_tree.insert(IndexedPolygon {
            index,
            envelope: env,
        });
        Ok(())
    }

    /// Register a catalog region (`assignment_changed` events; tie-break: lexicographically smallest id).
    pub fn try_push_catalog_region(&mut self, region: Geofence) -> Result<(), SpatialError> {
        validate_polygon(&region.polygon)?;
        if self.id_exists(&region.id) {
            return Err(SpatialError::DuplicateZoneId(region.id.clone()));
        }
        let env = polygon_aabb(&region.polygon)?;
        self.catalog.push(region);
        let index = self.catalog.len() - 1;
        self.catalog_tree.insert(IndexedPolygon {
            index,
            envelope: env,
        });
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
        let envelope = radius_aabb(zone.cx, zone.cy, zone.r);
        self.radius_zones.push(zone);
        let index = self.radius_zones.len() - 1;
        self.radius_tree.insert(IndexedRadius { index, envelope });
        Ok(())
    }

    pub fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.fences, &self.fence_tree, point)
    }

    pub fn containing_corridors(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.corridors, &self.corridor_tree, point)
    }

    pub fn containing_catalog_regions(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.catalog, &self.catalog_tree, point)
    }

    pub fn containing_radius_zones(&self, point: (f64, f64)) -> Vec<&RadiusZone> {
        let probe = point_probe_envelope(point);
        self.radius_tree
            .locate_in_envelope_intersecting(&probe)
            .filter(|obj| self.radius_zones[obj.index].contains_point(point.0, point.1))
            .map(|obj| &self.radius_zones[obj.index])
            .collect()
    }
}

impl SpatialIndex for NaiveSpatialIndex {
    fn containing_geofences(&self, point: (f64, f64)) -> Vec<&Geofence> {
        containing_polygons(&self.fences, &self.fence_tree, point)
    }

    fn geofence_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        fill_polygon_zone_ids(&self.fences, &self.fence_tree, point, out);
    }

    fn corridor_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        fill_polygon_zone_ids(&self.corridors, &self.corridor_tree, point, out);
    }

    fn radius_membership_at(&self, point: (f64, f64), out: &mut BTreeSet<String>) {
        out.clear();
        let probe = point_probe_envelope(point);
        for obj in self.radius_tree.locate_in_envelope_intersecting(&probe) {
            let z = &self.radius_zones[obj.index];
            if z.contains_point(point.0, point.1) {
                out.insert(z.id.clone());
            }
        }
    }

    fn primary_catalog_at(&self, point: (f64, f64)) -> Option<String> {
        primary_catalog_at_indexed(&self.catalog, &self.catalog_tree, point)
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

fn validate_ring(ring: &LineString<f64>) -> Result<(), SpatialError> {
    if ring.coords().count() < 4 {
        return Err(SpatialError::InvalidPolygon);
    }
    if !ring.is_closed() {
        return Err(SpatialError::InvalidPolygon);
    }
    Ok(())
}

fn validate_polygon(polygon: &Polygon<f64>) -> Result<(), SpatialError> {
    validate_ring(polygon.exterior())?;
    for interior in polygon.interiors() {
        validate_ring(interior)?;
    }
    Ok(())
}

#[cfg(test)]
impl NaiveSpatialIndex {
    fn linear_geofence_ids_at(&self, p: (f64, f64)) -> BTreeSet<String> {
        let pt = Point::new(p.0, p.1);
        self.fences
            .iter()
            .filter(|f| f.polygon.contains(&pt))
            .map(|f| f.id.clone())
            .collect()
    }

    fn linear_corridor_ids_at(&self, p: (f64, f64)) -> BTreeSet<String> {
        let pt = Point::new(p.0, p.1);
        self.corridors
            .iter()
            .filter(|f| f.polygon.contains(&pt))
            .map(|f| f.id.clone())
            .collect()
    }

    fn linear_primary_catalog_at(&self, p: (f64, f64)) -> Option<String> {
        let pt = Point::new(p.0, p.1);
        self.catalog
            .iter()
            .filter(|f| f.polygon.contains(&pt))
            .map(|f| f.id.as_str())
            .min()
            .map(String::from)
    }

    fn linear_radius_ids_at(&self, p: (f64, f64)) -> BTreeSet<String> {
        self.radius_zones
            .iter()
            .filter(|z| z.contains_point(p.0, p.1))
            .map(|z| z.id.clone())
            .collect()
    }
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

    fn unit_square_at(ox: f64, oy: f64) -> Polygon<f64> {
        Polygon::new(
            LineString::from(vec![
                (ox, oy),
                (ox + 1.0, oy),
                (ox + 1.0, oy + 1.0),
                (ox, oy + 1.0),
                (ox, oy),
            ]),
            vec![],
        )
    }

    /// A 10×10 square (0,0)–(10,10) with a 4×4 hole centred at (5,5), i.e. (3,3)–(7,7).
    fn square_with_hole() -> Polygon<f64> {
        Polygon::new(
            LineString::from(vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ]),
            vec![LineString::from(vec![
                (3.0, 3.0),
                (7.0, 3.0),
                (7.0, 7.0),
                (3.0, 7.0),
                (3.0, 3.0),
            ])],
        )
    }

    #[test]
    fn inside_and_outside() {
        let p = square();
        assert!(point_in_polygon((5.0, 5.0), &p));
        assert!(!point_in_polygon((50.0, 5.0), &p));
    }

    #[test]
    fn point_in_hole_is_outside_polygon() {
        // (5, 5) is inside the hole — should NOT be contained.
        let p = square_with_hole();
        assert!(!point_in_polygon((5.0, 5.0), &p));
    }

    #[test]
    fn point_in_exterior_outside_hole_is_inside_polygon() {
        // (1, 1) is in the exterior ring but outside the hole — should be contained.
        let p = square_with_hole();
        assert!(point_in_polygon((1.0, 1.0), &p));
    }

    #[test]
    fn polygon_without_holes_still_works() {
        let p = square();
        assert!(point_in_polygon((5.0, 5.0), &p));
        assert!(!point_in_polygon((15.0, 5.0), &p));
    }

    #[test]
    fn geofence_with_hole_excludes_point_in_hole() {
        let mut idx = NaiveSpatialIndex::new();
        idx.try_push(Geofence {
            id: "holey".into(),
            polygon: square_with_hole(),
        })
        .unwrap();
        // Inside hole → not contained.
        assert!(idx.containing_geofences((5.0, 5.0)).is_empty());
        // Outside hole but inside exterior → contained.
        let hits = idx.containing_geofences((1.0, 1.0));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "holey");
    }

    #[test]
    fn invalid_hole_ring_rejected() {
        // A hole ring with only 3 points (not closed, not ≥4) must fail validation.
        let bad = Polygon::new(
            LineString::from(vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ]),
            vec![LineString::from(vec![
                (1.0, 1.0),
                (2.0, 1.0),
                (1.0, 1.0), // only 3 points; not a valid ring (< 4)
            ])],
        );
        let mut idx = NaiveSpatialIndex::new();
        let err = idx
            .try_push(Geofence {
                id: "bad_hole".into(),
                polygon: bad,
            })
            .unwrap_err();
        assert!(matches!(err, SpatialError::InvalidPolygon));
    }

    #[test]
    fn naive_index_finds_fence() {
        let mut idx = NaiveSpatialIndex::new();
        idx.try_push(Geofence {
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
        idx.try_push_catalog_region(Geofence {
            id: "b".into(),
            polygon: square(),
        })
        .unwrap();
        idx.try_push_catalog_region(Geofence {
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
        idx.try_push(Geofence {
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

    #[test]
    fn rtree_geofence_membership_matches_linear_scan() {
        let mut idx = NaiveSpatialIndex::new();
        for i in 0..24 {
            let ox = (i as f64) * 3.0;
            let oy = (i as f64 % 5.0) * 2.0;
            idx.try_push(Geofence {
                id: format!("z{i}"),
                polygon: unit_square_at(ox, oy),
            })
            .unwrap();
        }
        let probes = [
            (0.5, 0.5),
            (1.5, 0.5),
            (50.0, 50.0),
            (0.0, 0.0),
            (1.0, 1.0),
            (3.5, 0.5),
        ];
        for p in probes {
            let mut rt = BTreeSet::new();
            idx.geofence_membership_at(p, &mut rt);
            assert_eq!(rt, idx.linear_geofence_ids_at(p), "probe {p:?}");
        }
    }

    #[test]
    fn rtree_corridor_membership_matches_linear_scan() {
        let mut idx = NaiveSpatialIndex::new();
        for i in 0..16 {
            idx.try_push_corridor(Geofence {
                id: format!("c{i}"),
                polygon: unit_square_at(i as f64 * 2.0, 0.0),
            })
            .unwrap();
        }
        for p in [(0.5, 0.5), (2.5, 0.5), (100.0, 0.0)] {
            let mut rt = BTreeSet::new();
            idx.corridor_membership_at(p, &mut rt);
            assert_eq!(rt, idx.linear_corridor_ids_at(p));
        }
    }

    #[test]
    fn rtree_primary_catalog_matches_linear_scan() {
        let mut idx = NaiveSpatialIndex::new();
        for i in 0..12 {
            idx.try_push_catalog_region(Geofence {
                id: format!("r{i:02}"),
                polygon: unit_square_at(0.0, i as f64 * 0.5),
            })
            .unwrap();
        }
        idx.try_push_catalog_region(Geofence {
            id: "r_overlap".into(),
            polygon: unit_square_at(0.0, 0.0),
        })
        .unwrap();
        for p in [(0.5, 0.5), (0.5, 2.0), (0.5, 100.0)] {
            assert_eq!(
                idx.primary_catalog_at(p),
                idx.linear_primary_catalog_at(p),
                "probe {p:?}"
            );
        }
    }

    #[test]
    fn rtree_radius_membership_matches_linear_scan() {
        let mut idx = NaiveSpatialIndex::new();
        // Spread disks at varied positions and radii so some overlap and some don't.
        for i in 0..20u32 {
            let cx = (i as f64) * 5.0;
            let cy = (i as f64 % 4.0) * 5.0;
            let r = 1.0 + (i as f64 % 3.0);
            idx.try_push_radius_zone(RadiusZone {
                id: format!("rz{i}"),
                cx,
                cy,
                r,
            })
            .unwrap();
        }
        let probes = [
            (0.0, 0.0),
            (5.0, 0.0),
            (10.0, 5.0),
            (50.0, 50.0),
            (1000.0, 1000.0),
            (0.5, 0.5),
        ];
        for p in probes {
            let mut rt = BTreeSet::new();
            idx.radius_membership_at(p, &mut rt);
            assert_eq!(rt, idx.linear_radius_ids_at(p), "probe {p:?}");
        }
    }

    // --- polygon_from_json_value tests (moved from the polygon-json crate) ---

    #[test]
    fn polygon_json_simple_no_holes() {
        use serde_json::json;
        let v = json!({
            "type": "Polygon",
            "coordinates": [
                [[0.0,0.0],[10.0,0.0],[10.0,10.0],[0.0,10.0],[0.0,0.0]]
            ]
        });
        let p = polygon_from_json_value(&v).unwrap();
        assert_eq!(p.interiors().len(), 0);
    }

    #[test]
    fn polygon_json_with_hole_parsed_correctly() {
        use serde_json::json;
        let v = json!({
            "type": "Polygon",
            "coordinates": [
                [[0.0,0.0],[10.0,0.0],[10.0,10.0],[0.0,10.0],[0.0,0.0]],
                [[3.0,3.0],[7.0,3.0],[7.0,7.0],[3.0,7.0],[3.0,3.0]]
            ]
        });
        let p = polygon_from_json_value(&v).unwrap();
        assert_eq!(p.interiors().len(), 1, "expected one interior ring (hole)");
        use geo::algorithm::contains::Contains;
        use geo::Point;
        assert!(!p.contains(&Point::new(5.0_f64, 5.0_f64)));
        assert!(p.contains(&Point::new(1.0_f64, 1.0_f64)));
    }

    #[test]
    fn polygon_json_non_polygon_rejected() {
        use serde_json::json;
        let v = json!({ "type": "Point", "coordinates": [0.0, 0.0] });
        assert!(matches!(
            polygon_from_json_value(&v),
            Err(PolygonJsonError::NotPolygon)
        ));
    }

    #[test]
    fn polygon_json_invalid_json_rejected() {
        use serde_json::json;
        let v = json!("not a geometry");
        assert!(matches!(
            polygon_from_json_value(&v),
            Err(PolygonJsonError::InvalidGeometry(_))
        ));
    }
}
