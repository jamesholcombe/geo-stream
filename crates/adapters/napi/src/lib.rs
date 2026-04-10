use engine::{
    Circle, CircleDwell, ConfigurableRule, Engine, EngineOptions, EventKind, GeoEngine as _,
    PointUpdate, RuleFilter, RuleTrigger, SequenceRule, Zone, ZoneDwell,
};
use napi_derive::napi;
use serde::Serialize;
use spatial::polygon_from_json_value;

// ---------------------------------------------------------------------------
// Input types
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

#[napi(object)]
pub struct EngineOptionsJs {
    /// Maximum historical position samples per entity. Default: 10.
    pub history_size: Option<i32>,
}

/// A single trigger condition for a configurable rule.
#[napi(object)]
pub struct RuleTriggerJs {
    /// One of: "enter", "exit", "approach", "recede".
    pub event_kind: String,
    /// The zone or circle id to watch.
    pub target_id: String,
}

/// A single filter condition for a configurable rule.
#[napi(object)]
pub struct RuleFilterJs {
    /// One of: "speed_above", "speed_below", "heading_between".
    pub filter_type: String,
    /// Speed threshold in units/s (used for speed_above / speed_below).
    pub value: Option<f64>,
    /// Start of heading range in degrees (used for heading_between).
    pub from: Option<f64>,
    /// End of heading range in degrees (used for heading_between).
    pub to: Option<f64>,
}

/// Configuration for a user-defined rule.
#[napi(object)]
pub struct RuleConfigJs {
    pub name: String,
    pub triggers: Vec<RuleTriggerJs>,
    pub filters: Vec<RuleFilterJs>,
    /// Name of the emitted `rule` event.
    pub emit: String,
    /// Arbitrary JSON payload attached to every emitted event.
    pub data: Option<serde_json::Value>,
}

/// Configuration for a sequence rule.
#[napi(object)]
pub struct SequenceConfigJs {
    pub name: String,
    /// Zone/circle ids that must be triggered (enter/approach) in order.
    pub steps: Vec<String>,
    /// Optional window in ms; sequence resets if not completed within this time.
    pub within_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Snapshot of current entity state returned from getEntityState / getEntities.
#[napi(object)]
pub struct EntityStateJs {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub t_ms: i64,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
}

/// Entity state plus Euclidean distance from a query point, returned from spatial queries.
#[napi(object)]
pub struct EntityWithDistanceJs {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub t_ms: i64,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    /// Euclidean distance from the query point in the same units as coordinates.
    pub distance: f64,
}

// ---------------------------------------------------------------------------
// Event DTO
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EventDto {
    Enter {
        id: String,
        zone: String,
        t_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<f64>,
    },
    Exit {
        id: String,
        zone: String,
        t_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<f64>,
    },
    Approach {
        id: String,
        circle: String,
        t_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<f64>,
    },
    Recede {
        id: String,
        circle: String,
        t_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<f64>,
    },
    AssignmentChanged {
        id: String,
        region: Option<String>,
        t_ms: u64,
    },
    Rule {
        id: String,
        name: String,
        t_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<f64>,
        #[serde(skip_serializing_if = "serde_json::Value::is_null")]
        data: serde_json::Value,
    },
    SequenceComplete {
        id: String,
        sequence: String,
        t_ms: u64,
    },
}

impl From<engine::Event> for EventDto {
    fn from(ev: engine::Event) -> Self {
        match ev {
            engine::Event::Enter {
                id,
                zone,
                t_ms,
                speed,
                heading,
            } => EventDto::Enter {
                id,
                zone,
                t_ms,
                speed,
                heading,
            },
            engine::Event::Exit {
                id,
                zone,
                t_ms,
                speed,
                heading,
            } => EventDto::Exit {
                id,
                zone,
                t_ms,
                speed,
                heading,
            },
            engine::Event::Approach {
                id,
                circle,
                t_ms,
                speed,
                heading,
            } => EventDto::Approach {
                id,
                circle,
                t_ms,
                speed,
                heading,
            },
            engine::Event::Recede {
                id,
                circle,
                t_ms,
                speed,
                heading,
            } => EventDto::Recede {
                id,
                circle,
                t_ms,
                speed,
                heading,
            },
            engine::Event::AssignmentChanged { id, region, t_ms } => {
                EventDto::AssignmentChanged { id, region, t_ms }
            }
            engine::Event::Custom {
                id,
                name,
                t_ms,
                speed,
                heading,
                data,
            } => EventDto::Rule {
                id,
                name,
                t_ms,
                speed,
                heading,
                data,
            },
            engine::Event::SequenceComplete { id, sequence, t_ms } => {
                EventDto::SequenceComplete { id, sequence, t_ms }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn parse_rule_config(js: RuleConfigJs) -> napi::Result<ConfigurableRule> {
    let triggers = js
        .triggers
        .into_iter()
        .map(|t| {
            let kind = match t.event_kind.as_str() {
                "enter" => Ok(EventKind::Enter),
                "exit" => Ok(EventKind::Exit),
                "approach" => Ok(EventKind::Approach),
                "recede" => Ok(EventKind::Recede),
                other => Err(napi::Error::from_reason(format!(
                    "unknown event_kind: {other}"
                ))),
            }?;
            Ok(RuleTrigger {
                event_kind: kind,
                target_id: t.target_id,
            })
        })
        .collect::<napi::Result<Vec<_>>>()?;

    let filters = js
        .filters
        .into_iter()
        .map(|f| match f.filter_type.as_str() {
            "speed_above" => Ok(RuleFilter::SpeedAbove(f.value.unwrap_or(0.0))),
            "speed_below" => Ok(RuleFilter::SpeedBelow(f.value.unwrap_or(0.0))),
            "heading_between" => Ok(RuleFilter::HeadingBetween {
                from: f.from.unwrap_or(0.0),
                to: f.to.unwrap_or(360.0),
            }),
            other => Err(napi::Error::from_reason(format!(
                "unknown filter_type: {other}"
            ))),
        })
        .collect::<napi::Result<Vec<_>>>()?;

    Ok(ConfigurableRule {
        name: js.name,
        triggers,
        filters,
        emit: js.emit,
        data: js.data.unwrap_or(serde_json::Value::Null),
    })
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
        Self::new(None)
    }
}

#[napi]
impl GeoEngineNode {
    #[napi(constructor)]
    pub fn new(options: Option<EngineOptionsJs>) -> Self {
        let opts = options
            .map(|o| EngineOptions {
                history_size: o.history_size.unwrap_or(10) as usize,
            })
            .unwrap_or_default();
        Self {
            inner: Engine::with_options(opts),
        }
    }

    /// Register a named zone from a GeoJSON Polygon object.
    /// Optionally provide dwell thresholds to debounce enter/exit events.
    #[napi]
    pub fn register_zone(
        &mut self,
        id: String,
        polygon: serde_json::Value,
        dwell: Option<DwellOptionsJs>,
    ) -> napi::Result<()> {
        let poly = polygon_from_json_value(&polygon)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let zone = Zone { id, polygon: poly };
        let dwell_config = dwell.map(|d| ZoneDwell {
            min_inside_ms: d.min_inside_ms.map(|v| v as u64),
            min_outside_ms: d.min_outside_ms.map(|v| v as u64),
        });
        match dwell_config {
            Some(dwell_cfg) => self
                .inner
                .register_zone_with_dwell(zone, dwell_cfg)
                .map_err(engine_err),
            None => self.inner.register_zone(zone).map_err(engine_err),
        }
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
        let region = Zone { id, polygon: poly };
        self.inner
            .register_catalog_region(region)
            .map_err(engine_err)
    }

    /// Register a named circle by center point and radius (same units as coordinates).
    /// Optionally provide dwell thresholds to debounce approach/recede events.
    #[napi]
    pub fn register_circle(
        &mut self,
        id: String,
        cx: f64,
        cy: f64,
        r: f64,
        dwell: Option<DwellOptionsJs>,
    ) -> napi::Result<()> {
        let circle = Circle { id, cx, cy, r };
        match dwell {
            Some(d) => {
                let dwell_cfg = CircleDwell {
                    min_inside_ms: d.min_inside_ms.map(|v| v as u64),
                    min_outside_ms: d.min_outside_ms.map(|v| v as u64),
                };
                self.inner
                    .register_circle_with_dwell(circle, dwell_cfg)
                    .map_err(engine_err)
            }
            None => self.inner.register_circle(circle).map_err(engine_err),
        }
    }

    /// Define a configurable rule. Fires a `rule` event when triggers and filters match.
    #[napi]
    pub fn define_rule(&mut self, config: RuleConfigJs) -> napi::Result<()> {
        let rule = parse_rule_config(config)?;
        self.inner.add_rule(rule);
        Ok(())
    }

    /// Define a sequence rule. Fires `sequence_complete` when all steps match in order.
    #[napi]
    pub fn define_sequence(&mut self, config: SequenceConfigJs) -> napi::Result<()> {
        let rule = SequenceRule::new(
            config.name,
            config.steps,
            config.within_ms.map(|v| v as u64),
        );
        self.inner.add_sequence(rule);
        Ok(())
    }

    /// Return the current state snapshot for an entity, or undefined if not yet seen.
    #[napi]
    pub fn get_entity_state(&self, id: String) -> Option<EntityStateJs> {
        self.inner
            .get_entity_state(&id)
            .and_then(|st| entity_to_js(&id, st))
    }

    /// Return state snapshots for all known entities.
    #[napi]
    pub fn get_entities(&self) -> Vec<EntityStateJs> {
        self.inner
            .get_entities()
            .filter_map(|(id, st)| entity_to_js(id, st))
            .collect()
    }

    /// Return all entities whose logical zone membership includes `zone_id`.
    #[napi]
    pub fn entities_in_zone(&self, zone_id: String) -> Vec<EntityStateJs> {
        self.inner
            .entities_in_zone(&zone_id)
            .into_iter()
            .filter_map(|(id, st)| entity_to_js(id, st))
            .collect()
    }

    /// Return all entities whose logical circle membership includes `circle_id`.
    #[napi]
    pub fn entities_in_circle(&self, circle_id: String) -> Vec<EntityStateJs> {
        self.inner
            .entities_in_circle(&circle_id)
            .into_iter()
            .filter_map(|(id, st)| entity_to_js(id, st))
            .collect()
    }

    /// Return all entities whose current catalog region matches `region_id`.
    #[napi]
    pub fn entities_in_region(&self, region_id: String) -> Vec<EntityStateJs> {
        self.inner
            .entities_in_region(&region_id)
            .into_iter()
            .filter_map(|(id, st)| entity_to_js(id, st))
            .collect()
    }

    /// Return all entities within `radius` of `(x, y)`, sorted by distance ascending.
    #[napi]
    pub fn entities_near_point(&self, x: f64, y: f64, radius: f64) -> Vec<EntityWithDistanceJs> {
        self.inner
            .entities_near_point(x, y, radius)
            .into_iter()
            .filter_map(|(id, st, dist)| entity_to_js_with_distance(id, st, dist))
            .collect()
    }

    /// Return the `k` nearest entities to `(x, y)`, sorted by distance ascending.
    #[napi]
    pub fn nearest_to_point(&self, x: f64, y: f64, k: i32) -> Vec<EntityWithDistanceJs> {
        self.inner
            .nearest_to_point(x, y, k.max(0) as usize)
            .into_iter()
            .filter_map(|(id, st, dist)| entity_to_js_with_distance(id, st, dist))
            .collect()
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

fn entity_to_js(id: &str, st: &engine::EntityState) -> Option<EntityStateJs> {
    let (x, y) = st.position?;
    let t_ms = st.last_t_ms? as i64;
    Some(EntityStateJs {
        id: id.to_string(),
        x,
        y,
        t_ms,
        speed: st.speed,
        heading: st.heading,
    })
}

fn entity_to_js_with_distance(
    id: &str,
    st: &engine::EntityState,
    distance: f64,
) -> Option<EntityWithDistanceJs> {
    let (x, y) = st.position?;
    let t_ms = st.last_t_ms? as i64;
    Some(EntityWithDistanceJs {
        id: id.to_string(),
        x,
        y,
        t_ms,
        speed: st.speed,
        heading: st.heading,
        distance,
    })
}
