//! Optional HTTP adapter (protocol v2 sketch): JSON batch ingest, JSON events response.

#[cfg(feature = "server")]
mod server_impl {
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};
    use engine::{Engine, Event, Geofence, GeoEngine, PointUpdate};
    use geo::Polygon;
    use geojson::Geometry;
    use serde::Deserialize;
    use serde_json::Value;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tower_http::trace::TraceLayer;

    #[derive(Clone)]
    struct AppState {
        engine: Arc<Mutex<Engine>>,
    }

    #[derive(Debug, Deserialize)]
    struct IngestBody {
        updates: Vec<PointUpdate>,
    }

    #[derive(Debug, Deserialize)]
    struct RegisterBody {
        id: String,
        polygon: Value,
    }

    fn polygon_from_value(v: Value) -> Result<Polygon<f64>, (StatusCode, String)> {
        let geom: Geometry = serde_json::from_value(v).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid GeoJSON geometry: {e}"),
            )
        })?;
        let g: geo::Geometry<f64> = geom.try_into().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "unsupported geometry (need Polygon)".into(),
            )
        })?;
        match g {
            geo::Geometry::Polygon(p) => Ok(p),
            _ => Err((
                StatusCode::BAD_REQUEST,
                "geofence must be a Polygon".into(),
            )),
        }
    }

    async fn register_handler(
        State(state): State<AppState>,
        Json(body): Json<RegisterBody>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let polygon = polygon_from_value(body.polygon)?;
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
    ) -> Result<Json<Vec<Event>>, (StatusCode, String)> {
        let mut eng = state
            .engine
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let events = eng.ingest(body.updates);
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
