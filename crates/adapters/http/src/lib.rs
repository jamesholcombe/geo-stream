//! Optional HTTP adapter (protocol v2 sketch): JSON batch ingest, JSON events response.

#[cfg(feature = "server")]
mod server_impl {
    use axum::extract::Extension;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use engine::{Engine, Geofence, GeoEngine, PointUpdate, RadiusZone};
    use polygon_json::polygon_from_json_value;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    use tower_http::trace::TraceLayer;

    type SharedEngine = Arc<Mutex<Engine>>;

    #[derive(Debug, Deserialize)]
    struct PointUpdateJson {
        id: String,
        x: f64,
        y: f64,
    }

    #[derive(Debug, Deserialize)]
    struct IngestBody {
        updates: Vec<PointUpdateJson>,
    }

    #[derive(Debug, Deserialize)]
    struct RegisterPolygonBody {
        id: String,
        polygon: Value,
    }

    #[derive(Debug, Deserialize)]
    struct RegisterRadiusBody {
        id: String,
        cx: f64,
        cy: f64,
        r: f64,
    }

    #[derive(Debug, Serialize)]
    #[serde(tag = "event", rename_all = "snake_case")]
    enum EventJson {
        Enter { id: String, geofence: String },
        Exit { id: String, geofence: String },
        EnterCorridor { id: String, corridor: String },
        ExitCorridor { id: String, corridor: String },
        Approach { id: String, zone: String },
        Recede { id: String, zone: String },
        AssignmentChanged {
            id: String,
            region: Option<String>,
        },
    }

    impl From<engine::Event> for EventJson {
        fn from(ev: engine::Event) -> Self {
            match ev {
                engine::Event::Enter { id, geofence } => EventJson::Enter { id, geofence },
                engine::Event::Exit { id, geofence } => EventJson::Exit { id, geofence },
                engine::Event::EnterCorridor { id, corridor } => {
                    EventJson::EnterCorridor { id, corridor }
                }
                engine::Event::ExitCorridor { id, corridor } => {
                    EventJson::ExitCorridor { id, corridor }
                }
                engine::Event::Approach { id, zone } => EventJson::Approach { id, zone },
                engine::Event::Recede { id, zone } => EventJson::Recede { id, zone },
                engine::Event::AssignmentChanged { id, region } => {
                    EventJson::AssignmentChanged { id, region }
                }
            }
        }
    }

    fn parse_polygon(v: &Value) -> Result<geo::Polygon<f64>, (StatusCode, String)> {
        polygon_from_json_value(v).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
    }

    async fn health_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "status": "ok" }))
    }

    async fn register_geofence_handler(
        Extension(engine): Extension<SharedEngine>,
        Json(body): Json<RegisterPolygonBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        eng
            .register_geofence(Geofence {
                id: body.id,
                polygon,
            })
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn register_corridor_handler(
        Extension(engine): Extension<SharedEngine>,
        Json(body): Json<RegisterPolygonBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        eng
            .register_corridor(Geofence {
                id: body.id,
                polygon,
            })
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn register_catalog_handler(
        Extension(engine): Extension<SharedEngine>,
        Json(body): Json<RegisterPolygonBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        eng
            .register_catalog_region(Geofence {
                id: body.id,
                polygon,
            })
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn register_radius_handler(
        Extension(engine): Extension<SharedEngine>,
        Json(body): Json<RegisterRadiusBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let mut eng = engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        eng
            .register_radius_zone(RadiusZone {
                id: body.id,
                cx: body.cx,
                cy: body.cy,
                r: body.r,
            })
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn ingest_handler(
        Extension(engine): Extension<SharedEngine>,
        Json(body): Json<IngestBody>,
    ) -> Result<Json<Vec<EventJson>>, (StatusCode, String)> {
        let mut eng = engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let updates: Vec<PointUpdate> = body
            .updates
            .into_iter()
            .map(|u| PointUpdate {
                id: u.id,
                x: u.x,
                y: u.y,
            })
            .collect();
        let events: Vec<EventJson> = eng.ingest(updates).into_iter().map(Into::into).collect();
        Ok(Json(events))
    }

    pub fn default_engine() -> SharedEngine {
        Arc::new(Mutex::new(Engine::new()))
    }

    /// `Router<()>` with engine in [`Extension`] so `axum::serve` and [`Router::into_service`] work.
    pub fn build_router(engine: SharedEngine) -> Router {
        Router::new()
            .route("/health", get(health_handler))
            .route("/v2/register_geofence", post(register_geofence_handler))
            .route("/v2/register_corridor", post(register_corridor_handler))
            .route("/v2/register_catalog_region", post(register_catalog_handler))
            .route("/v2/register_radius", post(register_radius_handler))
            .route("/v2/ingest", post(ingest_handler))
            .layer(TraceLayer::new_for_http())
            .layer(Extension(engine))
    }

    pub fn app_router() -> Router {
        build_router(default_engine())
    }

    pub async fn run_server(addr: std::net::SocketAddr) -> Result<(), std::io::Error> {
        let app = app_router();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await
    }

    #[cfg(test)]
    mod tests {
        use super::app_router;
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        #[tokio::test]
        async fn health_returns_ok_json() {
            let app = app_router();
            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["status"], "ok");
        }
    }
}

#[cfg(feature = "server")]
pub use server_impl::{app_router, default_engine, run_server};

/// Placeholder when the `server` feature is disabled.
#[cfg(not(feature = "server"))]
pub async fn run_server(_addr: std::net::SocketAddr) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "http-adapter built without `server` feature",
    ))
}
