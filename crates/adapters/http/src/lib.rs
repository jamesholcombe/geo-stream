//! Optional HTTP adapter (protocol v2 sketch): JSON batch ingest, JSON events response.

#[cfg(feature = "server")]
mod server_impl {
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};
    use engine::{Engine, Geofence, GeoEngine, PointUpdate};
    use polygon_json::polygon_from_json_value;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tower_http::trace::TraceLayer;

    #[derive(Clone)]
    struct AppState {
        engine: Arc<Mutex<Engine>>,
    }

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
    struct RegisterBody {
        id: String,
        polygon: Value,
    }

    #[derive(Debug, Serialize)]
    #[serde(tag = "event", rename_all = "lowercase")]
    enum EventJson {
        Enter { id: String, geofence: String },
        Exit { id: String, geofence: String },
    }

    impl From<engine::Event> for EventJson {
        fn from(ev: engine::Event) -> Self {
            match ev {
                engine::Event::Enter { id, geofence } => EventJson::Enter { id, geofence },
                engine::Event::Exit { id, geofence } => EventJson::Exit { id, geofence },
            }
        }
    }

    async fn register_handler(
        State(state): State<AppState>,
        Json(body): Json<RegisterBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let polygon = polygon_from_json_value(&body.polygon).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                e.to_string(),
            )
        })?;
        let mut eng = state
            .engine
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

    async fn ingest_handler(
        State(state): State<AppState>,
        Json(body): Json<IngestBody>,
    ) -> Result<Json<Vec<EventJson>>, (StatusCode, String)> {
        let mut eng = state
            .engine
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

    /// Run a minimal Axum server: `POST /v2/ingest` with body `{"updates":[...]}`.
    pub async fn run_server(addr: SocketAddr) -> Result<(), std::io::Error> {
        let state = AppState {
            engine: Arc::new(Mutex::new(Engine::new())),
        };
        let app = Router::new()
            .route("/v2/register_geofence", post(register_handler))
            .route("/v2/ingest", post(ingest_handler))
            .layer(TraceLayer::new_for_http())
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await
    }
}

#[cfg(feature = "server")]
pub use server_impl::run_server;

/// Placeholder when the `server` feature is disabled.
#[cfg(not(feature = "server"))]
pub async fn run_server(_addr: std::net::SocketAddr) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "http-adapter built without `server` feature",
    ))
}
