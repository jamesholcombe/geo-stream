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

/// Parse `value` as a GeoJSON geometry and return a single polygon ring (exterior only).
pub fn polygon_from_json_value(value: &Value) -> Result<Polygon<f64>, PolygonJsonError> {
    let geom: Geometry = serde_json::from_value(value.clone()).map_err(|e| {
        PolygonJsonError::InvalidGeometry(e.to_string())
    })?;
    let g: geo::Geometry<f64> = geom
        .try_into()
        .map_err(|_| PolygonJsonError::UnsupportedGeometry)?;
    match g {
        geo::Geometry::Polygon(p) => Ok(p),
        _ => Err(PolygonJsonError::NotPolygon),
    }
}
