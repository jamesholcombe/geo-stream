use engine::{Engine, GeoEngine as _, Geofence, GeofenceDwell, PointUpdate, RadiusZone};
use napi_derive::napi;
use serde::Serialize;
use spatial::polygon_from_json_value;

// ---------------------------------------------------------------------------
// Input types -- #[napi(object)] generates TypeScript interfaces
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct PointUpdateJs {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Unix epoch milliseconds. i64 maps to TS `number` (u64 would map to BigInt).
    pub t_ms: i64,
}

#[napi(object)]
pub struct DwellOptionsJs {
    pub min_inside_ms: Option<i64>,
    pub min_outside_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Event DTO -- serialized to serde_json::Value for the JS layer.
// Uses `kind` tag (more idiomatic for JS than `event`).
// `region` in AssignmentChanged is NOT skipped when None so JS consumers
// receive `null` (unassigned) rather than an absent field.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EventDto {
    Enter {
        id: String,
        geofence: String,
        t_ms: u64,
    },
    Exit {
        id: String,
        geofence: String,
        t_ms: u64,
    },
    EnterCorridor {
        id: String,
        corridor: String,
        t_ms: u64,
    },
    ExitCorridor {
        id: String,
        corridor: String,
        t_ms: u64,
    },
    Approach {
        id: String,
        zone: String,
        t_ms: u64,
    },
    Recede {
        id: String,
        zone: String,
        t_ms: u64,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t_ms: u64,
    },
}

impl From<engine::Event> for EventDto {
    fn from(ev: engine::Event) -> Self {
        match ev {
            engine::Event::Enter { id, geofence, t_ms } => EventDto::Enter { id, geofence, t_ms },
            engine::Event::Exit { id, geofence, t_ms } => EventDto::Exit { id, geofence, t_ms },
            engine::Event::EnterCorridor { id, corridor, t_ms } => {
                EventDto::EnterCorridor { id, corridor, t_ms }
            }
            engine::Event::ExitCorridor { id, corridor, t_ms } => {
                EventDto::ExitCorridor { id, corridor, t_ms }
            }
            engine::Event::Approach { id, zone, t_ms } => EventDto::Approach { id, zone, t_ms },
            engine::Event::Recede { id, zone, t_ms } => EventDto::Recede { id, zone, t_ms },
            engine::Event::AssignmentChanged { id, region, t_ms } => {
                EventDto::AssignmentChanged { id, region, t_ms }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Engine wrapper
// ---------------------------------------------------------------------------

#[napi]
pub struct GeoEngineNode {
    inner: Engine,
}

impl Default for GeoEngineNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl GeoEngineNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Engine::new(),
        }
    }

    /// Register a named geofence from a GeoJSON Polygon object.
    /// Optionally provide dwell thresholds to debounce enter/exit events.
    #[napi]
    pub fn register_geofence(
        &mut self,
        id: String,
        polygon: serde_json::Value,
        dwell: Option<DwellOptionsJs>,
    ) -> napi::Result<()> {
        let poly = polygon_from_json_value(&polygon)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let geofence = Geofence { id, polygon: poly };
        let dwell_config = dwell.map(|d| GeofenceDwell {
            min_inside_ms: d.min_inside_ms.map(|v| v as u64),
            min_outside_ms: d.min_outside_ms.map(|v| v as u64),
        });
        match dwell_config {
            Some(dwell_cfg) => self
                .inner
                .register_geofence_with_dwell(geofence, dwell_cfg)
                .map_err(engine_err),
            None => self.inner.register_geofence(geofence).map_err(engine_err),
        }
    }

    /// Register a named corridor from a GeoJSON Polygon object.
    #[napi]
    pub fn register_corridor(
        &mut self,
        id: String,
        polygon: serde_json::Value,
    ) -> napi::Result<()> {
        let poly = polygon_from_json_value(&polygon)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let corridor = Geofence { id, polygon: poly };
        self.inner.register_corridor(corridor).map_err(engine_err)
    }

    /// Register a named catalog region from a GeoJSON Polygon object.
    #[napi]
    pub fn register_catalog_region(
        &mut self,
        id: String,
        polygon: serde_json::Value,
    ) -> napi::Result<()> {
        let poly = polygon_from_json_value(&polygon)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let region = Geofence { id, polygon: poly };
        self.inner
            .register_catalog_region(region)
            .map_err(engine_err)
    }

    /// Register a named radius zone by center point and radius (same units as coordinates).
    #[napi]
    pub fn register_radius_zone(
        &mut self,
        id: String,
        cx: f64,
        cy: f64,
        r: f64,
    ) -> napi::Result<()> {
        let zone = RadiusZone { id, cx, cy, r };
        self.inner.register_radius_zone(zone).map_err(engine_err)
    }

    /// Process a batch of point updates and return the resulting events.
    /// Updates are sorted by entity ID then timestamp before processing.
    #[napi]
    pub fn ingest(&mut self, updates: Vec<PointUpdateJs>) -> napi::Result<Vec<serde_json::Value>> {
        let engine_updates: Vec<PointUpdate> = updates
            .into_iter()
            .map(|u| PointUpdate {
                id: u.id,
                x: u.x,
                y: u.y,
                t_ms: u.t_ms as u64,
            })
            .collect();

        let (events, _errors) = self.inner.process_batch(engine_updates);

        events
            .into_iter()
            .map(|ev| {
                serde_json::to_value(EventDto::from(ev))
                    .map_err(|e| napi::Error::from_reason(e.to_string()))
            })
            .collect()
    }
}

fn engine_err(e: engine::EngineError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}
