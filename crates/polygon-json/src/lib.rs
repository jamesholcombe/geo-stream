//! Parse a GeoJSON Polygon object (as JSON) into [`geo::Polygon`].

use geo::Polygon;
use geojson::Geometry;
use serde_json::Value;
use thiserror::Error;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_polygon_no_holes() {
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
    fn polygon_with_hole_parsed_correctly() {
        // 10×10 square with a 4×4 hole at (3,3)–(7,7).
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
        // Point inside the hole is NOT inside the polygon.
        assert!(!p.contains(&Point::new(5.0_f64, 5.0_f64)));
        // Point in exterior but outside the hole IS inside the polygon.
        assert!(p.contains(&Point::new(1.0_f64, 1.0_f64)));
    }

    #[test]
    fn non_polygon_geometry_rejected() {
        let v = json!({
            "type": "Point",
            "coordinates": [0.0, 0.0]
        });
        assert!(matches!(
            polygon_from_json_value(&v),
            Err(PolygonJsonError::NotPolygon)
        ));
    }

    #[test]
    fn invalid_json_rejected() {
        let v = json!("not a geometry");
        assert!(matches!(
            polygon_from_json_value(&v),
            Err(PolygonJsonError::InvalidGeometry(_))
        ));
    }
}
