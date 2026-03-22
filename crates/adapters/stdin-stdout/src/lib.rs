//! Newline-delimited JSON over stdin/stdout; parses protocol v1 lines and drives [`engine::Engine`].

use engine::{Engine, EngineError, GeoEngine, Geofence, PointUpdate, RadiusZone};
use polygon_json::polygon_from_json_value;
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
    /// Number of point updates per `process_batch` call. `0` means buffer all updates until EOF, then one batch.
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
        /// Unix epoch milliseconds for this observation. Omitted or null → `0`.
        #[serde(default, rename = "t")]
        t_ms: u64,
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
    RegisterCorridor {
        id: String,
        polygon: Value,
        #[serde(default, rename = "v")]
        _protocol_version: Option<u8>,
    },
    RegisterCatalogRegion {
        id: String,
        polygon: Value,
        #[serde(default, rename = "v")]
        _protocol_version: Option<u8>,
    },
    RegisterRadius {
        id: String,
        center: [f64; 2],
        radius: f64,
        #[serde(default, rename = "v")]
        _protocol_version: Option<u8>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum NdjsonEvent {
    Enter {
        id: String,
        geofence: String,
        t: u64,
    },
    Exit {
        id: String,
        geofence: String,
        t: u64,
    },
    EnterCorridor {
        id: String,
        corridor: String,
        t: u64,
    },
    ExitCorridor {
        id: String,
        corridor: String,
        t: u64,
    },
    Approach {
        id: String,
        zone: String,
        t: u64,
    },
    Recede {
        id: String,
        zone: String,
        t: u64,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t: u64,
    },
}

impl From<engine::Event> for NdjsonEvent {
    fn from(ev: engine::Event) -> Self {
        match ev {
            engine::Event::Enter {
                id,
                geofence,
                t_ms,
            } => NdjsonEvent::Enter {
                id,
                geofence,
                t: t_ms,
            },
            engine::Event::Exit {
                id,
                geofence,
                t_ms,
            } => NdjsonEvent::Exit {
                id,
                geofence,
                t: t_ms,
            },
            engine::Event::EnterCorridor {
                id,
                corridor,
                t_ms,
            } => NdjsonEvent::EnterCorridor {
                id,
                corridor,
                t: t_ms,
            },
            engine::Event::ExitCorridor {
                id,
                corridor,
                t_ms,
            } => NdjsonEvent::ExitCorridor {
                id,
                corridor,
                t: t_ms,
            },
            engine::Event::Approach { id, zone, t_ms } => NdjsonEvent::Approach {
                id,
                zone,
                t: t_ms,
            },
            engine::Event::Recede { id, zone, t_ms } => NdjsonEvent::Recede {
                id,
                zone,
                t: t_ms,
            },
            engine::Event::AssignmentChanged { id, region, t_ms } => {
                NdjsonEvent::AssignmentChanged {
                    id,
                    region,
                    t: t_ms,
                }
            }
        }
    }
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
                let poly = match polygon_from_json_value(&polygon) {
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
            InputLine::RegisterCorridor { id, polygon, .. } => {
                let poly = match polygon_from_json_value(&polygon) {
                    Ok(p) => p,
                    Err(e) => {
                        writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                        continue;
                    }
                };
                if let Err(e) = engine.register_corridor(Geofence { id, polygon: poly }) {
                    writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                }
            }
            InputLine::RegisterCatalogRegion { id, polygon, .. } => {
                let poly = match polygon_from_json_value(&polygon) {
                    Ok(p) => p,
                    Err(e) => {
                        writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                        continue;
                    }
                };
                if let Err(e) = engine.register_catalog_region(Geofence { id, polygon: poly }) {
                    writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                }
            }
            InputLine::RegisterRadius {
                id, center, radius, ..
            } => {
                if let Err(e) = engine.register_radius_zone(RadiusZone {
                    id,
                    cx: center[0],
                    cy: center[1],
                    r: radius,
                }) {
                    writeln_err(&mut err, &format!("line {line_no}: {e}"))?;
                }
            }
            InputLine::Update {
                id,
                location,
                t_ms,
                ..
            } => {
                pending.push(PointUpdate {
                    id,
                    x: location[0],
                    y: location[1],
                    t_ms,
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
    let events = engine.process_batch(batch);
    for ev in events {
        let line: NdjsonEvent = ev.into();
        writeln!(out, "{}", serde_json::to_string(&line)?)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use engine::Engine;
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
