//! Newline-delimited JSON over stdin/stdout; parses protocol v1 lines and drives [`engine::Engine`].

use engine::{Engine, EngineError, Geofence, GeoEngine, PointUpdate};
use geo::Polygon;
use geojson::Geometry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StdioAdapterError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("geojson/geometry: {0}")]
    Geometry(String),
    #[error("engine: {0}")]
    Engine(#[from] EngineError),
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Number of point updates per `ingest` call. `0` means buffer all updates until EOF, then one ingest.
    pub batch_size: usize,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self { batch_size: 1 }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputLine {
    Update {
        id: String,
        location: [f64; 2],
        /// Protocol version (optional); reserved for forward compatibility.
        #[serde(default, rename = "v")]
        _protocol_version: Option<u8>,
    },
    RegisterGeofence {
        id: String,
        polygon: Value,
        #[serde(default, rename = "v")]
        _protocol_version: Option<u8>,
    },
}

#[derive(Debug, Serialize)]
struct ErrorLine {
    error: String,
}

/// Read NDJSON from `reader`, write events to `out`, errors to `err`.
pub fn run<R, O, E>(
    engine: &mut Engine,
    reader: R,
    mut out: O,
    mut err: E,
    config: RunConfig,
) -> Result<(), StdioAdapterError>
where
    R: BufRead,
    O: Write,
    E: Write,
{
    let mut pending: Vec<PointUpdate> = Vec::new();
    let mut line_no: u64 = 0;

    for line in reader.lines() {
        line_no += 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: InputLine = match serde_json::from_str(&line) {
            Ok(p) => p,
            Err(e) => {
                writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                continue;
            }
        };

        match parsed {
            InputLine::RegisterGeofence { id, polygon, .. } => {
                let poly = match polygon_from_json_value(polygon) {
                    Ok(p) => p,
                    Err(e) => {
                        writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                        continue;
                    }
                };
                if let Err(e) = engine.register_geofence(Geofence { id, polygon: poly }) {
                    writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                }
            }
            InputLine::Update { id, location, .. } => {
                pending.push(PointUpdate {
                    id,
                    x: location[0],
                    y: location[1],
                });
                let should_flush = config.batch_size > 0 && pending.len() >= config.batch_size;
                if should_flush {
                    flush_batch(engine, &mut pending, &mut out)?;
                }
            }
        }
    }

    if !pending.is_empty() {
        flush_batch(engine, &mut pending, &mut out)?;
    }

    Ok(())
}

fn flush_batch<O: Write>(
    engine: &mut Engine,
    pending: &mut Vec<PointUpdate>,
    out: &mut O,
) -> Result<(), StdioAdapterError> {
    let batch = std::mem::take(pending);
    let events = engine.ingest(batch);
    for ev in events {
        writeln!(out, "{}", serde_json::to_string(&ev)?)?;
    }
    out.flush()?;
    Ok(())
}

fn writeln_err<E: Write>(err: &mut E, msg: &str) -> io::Result<()> {
    let line = serde_json::to_string(&ErrorLine {
        error: msg.to_string(),
    })
    .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", msg.replace('"', "'")));
    writeln!(err, "{line}")
}

fn polygon_from_json_value(v: Value) -> Result<Polygon<f64>, StdioAdapterError> {
    let geom: Geometry = serde_json::from_value(v).map_err(|e| StdioAdapterError::Geometry(e.to_string()))?;
    let g: geo::Geometry<f64> = geom
        .try_into()
        .map_err(|_| StdioAdapterError::Geometry("unsupported geometry for geofence".into()))?;
    match g {
        geo::Geometry::Polygon(p) => Ok(p),
        _ => Err(StdioAdapterError::Geometry(
            "geofence polygon must be a GeoJSON Polygon".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fence_line() -> String {
        r#"{"type":"register_geofence","id":"zone-1","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}"#.to_string()
    }

    #[test]
    fn enter_event_ndjson() {
        let mut eng = Engine::new();
        let input = format!(
            "{}\n{{\"type\":\"update\",\"id\":\"c1\",\"location\":[0.5,0.5]}}\n",
            fence_line()
        );
        let mut out = Vec::new();
        let mut err_out = Vec::new();
        run(
            &mut eng,
            Cursor::new(input),
            &mut out,
            &mut err_out,
            RunConfig { batch_size: 1 },
        )
        .unwrap();
        assert!(err_out.is_empty(), "{}", String::from_utf8_lossy(&err_out));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("enter"));
        assert!(s.contains("zone-1"));
    }
}
