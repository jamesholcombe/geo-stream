//! Optional HTTP adapter: JSON batch updates, JSON events response (`/v1/...` routes).

#[cfg(feature = "server")]
mod server_impl {
    use axum::extract::rejection::JsonRejection;
    use axum::extract::Extension;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use engine::{Engine, EngineError, GeoEngine, Geofence, PointUpdate, RadiusZone, SpatialError};
    use spatial::polygon_from_json_value;
    use spatial::PolygonJsonError;
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    use tower_http::trace::TraceLayer;
    use utoipa::{OpenApi, ToSchema};

    type SharedEngine = Arc<Mutex<Engine>>;

    // --- Structured errors ---

    #[derive(Debug, Serialize, ToSchema)]
    pub struct ApiErrorBody {
        pub code: String,
        pub message: String,
    }

    #[derive(Debug, Serialize, ToSchema)]
    pub struct ErrorEnvelope {
        pub error: ApiErrorBody,
    }

    #[derive(Debug)]
    pub struct HttpError {
        status: StatusCode,
        code: &'static str,
        message: String,
    }

    impl HttpError {
        fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
            Self {
                status,
                code,
                message: message.into(),
            }
        }

        fn invalid_json(message: impl Into<String>) -> Self {
            Self::new(StatusCode::BAD_REQUEST, "invalid_json", message)
        }

        fn invalid_input(message: impl Into<String>) -> Self {
            Self::new(StatusCode::BAD_REQUEST, "invalid_input", message)
        }

        fn conflict(message: impl Into<String>) -> Self {
            Self::new(StatusCode::CONFLICT, "conflict", message)
        }

        fn internal(message: impl Into<String>) -> Self {
            Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
        }
    }

    impl From<PolygonJsonError> for HttpError {
        fn from(e: PolygonJsonError) -> Self {
            HttpError::invalid_input(e.to_string())
        }
    }

    impl From<EngineError> for HttpError {
        fn from(e: EngineError) -> Self {
            match e {
                EngineError::Spatial(SpatialError::DuplicateZoneId(_)) => {
                    HttpError::conflict(e.to_string())
                }
                EngineError::Spatial(_) => HttpError::invalid_input(e.to_string()),
                EngineError::MonotonicityViolation { .. } => {
                    HttpError::invalid_input(e.to_string())
                }
            }
        }
    }

    impl IntoResponse for HttpError {
        fn into_response(self) -> Response {
            let envelope = ErrorEnvelope {
                error: ApiErrorBody {
                    code: self.code.to_string(),
                    message: self.message,
                },
            };
            (self.status, Json(envelope)).into_response()
        }
    }

    fn read_json<T: DeserializeOwned>(
        body: Result<Json<T>, JsonRejection>,
    ) -> Result<T, HttpError> {
        body.map(|Json(v)| v)
            .map_err(|e| HttpError::invalid_json(e.to_string()))
    }

    fn lock_engine(engine: &SharedEngine) -> Result<std::sync::MutexGuard<'_, Engine>, HttpError> {
        engine
            .lock()
            .map_err(|e| HttpError::internal(e.to_string()))
    }

    fn parse_polygon(v: &Value) -> Result<geo::Polygon<f64>, HttpError> {
        polygon_from_json_value(v).map_err(HttpError::from)
    }

    // --- DTOs ---

    #[derive(Debug, Deserialize, ToSchema)]
    struct PointUpdateJson {
        id: String,
        x: f64,
        y: f64,
        /// Unix epoch milliseconds. Omitted → `0`.
        #[serde(default, rename = "t")]
        t_ms: u64,
    }

    #[derive(Debug, Deserialize, ToSchema)]
    struct IngestBody {
        updates: Vec<PointUpdateJson>,
    }

    #[derive(Debug, Deserialize, ToSchema)]
    struct RegisterPolygonBody {
        id: String,
        #[schema(value_type = Object, example = json!({"type":"Polygon","coordinates":[[[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],[0.0,0.0]]]}))]
        polygon: Value,
    }

    #[derive(Debug, Deserialize, ToSchema)]
    struct RegisterRadiusBody {
        id: String,
        cx: f64,
        cy: f64,
        r: f64,
    }

    #[derive(Debug, Serialize, ToSchema)]
    struct HealthResponse {
        status: String,
    }

    #[derive(Debug, Serialize, ToSchema)]
    #[serde(tag = "event", rename_all = "snake_case")]
    enum EventJson {
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
            #[serde(skip_serializing_if = "Option::is_none")]
            region: Option<String>,
            t: u64,
        },
    }

    impl From<engine::Event> for EventJson {
        fn from(ev: engine::Event) -> Self {
            match ev {
                engine::Event::Enter { id, geofence, t_ms } => EventJson::Enter {
                    id,
                    geofence,
                    t: t_ms,
                },
                engine::Event::Exit { id, geofence, t_ms } => EventJson::Exit {
                    id,
                    geofence,
                    t: t_ms,
                },
                engine::Event::EnterCorridor { id, corridor, t_ms } => EventJson::EnterCorridor {
                    id,
                    corridor,
                    t: t_ms,
                },
                engine::Event::ExitCorridor { id, corridor, t_ms } => EventJson::ExitCorridor {
                    id,
                    corridor,
                    t: t_ms,
                },
                engine::Event::Approach { id, zone, t_ms } => {
                    EventJson::Approach { id, zone, t: t_ms }
                }
                engine::Event::Recede { id, zone, t_ms } => EventJson::Recede { id, zone, t: t_ms },
                engine::Event::AssignmentChanged { id, region, t_ms } => {
                    EventJson::AssignmentChanged {
                        id,
                        region,
                        t: t_ms,
                    }
                }
            }
        }
    }

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "geo-stream HTTP API",
            description = "Batch geospatial stream processing. Matches engine semantics described in protocol/ndjson.md.",
            version = "1.0.0"
        ),
        paths(
            openapi_json_handler,
            health_handler,
            register_geofence_handler,
            register_corridor_handler,
            register_catalog_handler,
            register_radius_handler,
            ingest_handler
        ),
        components(schemas(
            HealthResponse,
            ErrorEnvelope,
            PointUpdateJson,
            IngestBody,
            RegisterPolygonBody,
            RegisterRadiusBody,
            EventJson
        )),
        tags((name = "v1", description = "Version 1 endpoints"))
    )]
    struct V1ApiDoc;

    /// OpenAPI 3 document (JSON).
    #[utoipa::path(
        get,
        path = "/openapi.json",
        tag = "v1",
        responses(
            (status = 200, description = "OpenAPI 3.0 document", content_type = "application/json")
        )
    )]
    async fn openapi_json_handler() -> Json<utoipa::openapi::OpenApi> {
        Json(V1ApiDoc::openapi())
    }

    #[utoipa::path(
        get,
        path = "/health",
        tag = "v1",
        responses((status = 200, description = "Service is up", body = HealthResponse))
    )]
    async fn health_handler() -> Json<HealthResponse> {
        Json(HealthResponse {
            status: "ok".into(),
        })
    }

    #[utoipa::path(
        post,
        path = "/v1/register_geofence",
        tag = "v1",
        request_body = RegisterPolygonBody,
        responses(
            (status = 204, description = "Registered"),
            (status = 400, description = "Invalid JSON or polygon", body = ErrorEnvelope),
            (status = 409, description = "Zone id already exists", body = ErrorEnvelope),
            (status = 500, description = "Internal error", body = ErrorEnvelope)
        )
    )]
    async fn register_geofence_handler(
        Extension(engine): Extension<SharedEngine>,
        body: Result<Json<RegisterPolygonBody>, JsonRejection>,
    ) -> Result<StatusCode, HttpError> {
        let body = read_json(body)?;
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = lock_engine(&engine)?;
        eng.register_geofence(Geofence {
            id: body.id,
            polygon,
        })
        .map_err(HttpError::from)?;
        Ok(StatusCode::NO_CONTENT)
    }

    #[utoipa::path(
        post,
        path = "/v1/register_corridor",
        tag = "v1",
        request_body = RegisterPolygonBody,
        responses(
            (status = 204, description = "Registered"),
            (status = 400, description = "Invalid JSON or polygon", body = ErrorEnvelope),
            (status = 409, description = "Zone id already exists", body = ErrorEnvelope),
            (status = 500, description = "Internal error", body = ErrorEnvelope)
        )
    )]
    async fn register_corridor_handler(
        Extension(engine): Extension<SharedEngine>,
        body: Result<Json<RegisterPolygonBody>, JsonRejection>,
    ) -> Result<StatusCode, HttpError> {
        let body = read_json(body)?;
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = lock_engine(&engine)?;
        eng.register_corridor(Geofence {
            id: body.id,
            polygon,
        })
        .map_err(HttpError::from)?;
        Ok(StatusCode::NO_CONTENT)
    }

    #[utoipa::path(
        post,
        path = "/v1/register_catalog_region",
        tag = "v1",
        request_body = RegisterPolygonBody,
        responses(
            (status = 204, description = "Registered"),
            (status = 400, description = "Invalid JSON or polygon", body = ErrorEnvelope),
            (status = 409, description = "Zone id already exists", body = ErrorEnvelope),
            (status = 500, description = "Internal error", body = ErrorEnvelope)
        )
    )]
    async fn register_catalog_handler(
        Extension(engine): Extension<SharedEngine>,
        body: Result<Json<RegisterPolygonBody>, JsonRejection>,
    ) -> Result<StatusCode, HttpError> {
        let body = read_json(body)?;
        let polygon = parse_polygon(&body.polygon)?;
        let mut eng = lock_engine(&engine)?;
        eng.register_catalog_region(Geofence {
            id: body.id,
            polygon,
        })
        .map_err(HttpError::from)?;
        Ok(StatusCode::NO_CONTENT)
    }

    #[utoipa::path(
        post,
        path = "/v1/register_radius",
        tag = "v1",
        request_body = RegisterRadiusBody,
        responses(
            (status = 204, description = "Registered"),
            (status = 400, description = "Invalid JSON or radius", body = ErrorEnvelope),
            (status = 409, description = "Zone id already exists", body = ErrorEnvelope),
            (status = 500, description = "Internal error", body = ErrorEnvelope)
        )
    )]
    async fn register_radius_handler(
        Extension(engine): Extension<SharedEngine>,
        body: Result<Json<RegisterRadiusBody>, JsonRejection>,
    ) -> Result<StatusCode, HttpError> {
        let body = read_json(body)?;
        let mut eng = lock_engine(&engine)?;
        eng.register_radius_zone(RadiusZone {
            id: body.id,
            cx: body.cx,
            cy: body.cy,
            r: body.r,
        })
        .map_err(HttpError::from)?;
        Ok(StatusCode::NO_CONTENT)
    }

    #[utoipa::path(
        post,
        path = "/v1/ingest",
        tag = "v1",
        request_body = IngestBody,
        responses(
            (status = 200, description = "Emitted events for this batch", body = [EventJson]),
            (status = 400, description = "Invalid JSON", body = ErrorEnvelope),
            (status = 500, description = "Internal error", body = ErrorEnvelope)
        )
    )]
    async fn ingest_handler(
        Extension(engine): Extension<SharedEngine>,
        body: Result<Json<IngestBody>, JsonRejection>,
    ) -> Result<Json<Vec<EventJson>>, HttpError> {
        let body = read_json(body)?;
        let mut eng = lock_engine(&engine)?;
        let updates: Vec<PointUpdate> = body
            .updates
            .into_iter()
            .map(|u| PointUpdate {
                id: u.id,
                x: u.x,
                y: u.y,
                t_ms: u.t_ms,
            })
            .collect();
        let (raw_events, _errors) = eng.process_batch(updates);
        let events: Vec<EventJson> = raw_events.into_iter().map(Into::into).collect();
        Ok(Json(events))
    }

    pub fn default_engine() -> SharedEngine {
        Arc::new(Mutex::new(Engine::new()))
    }

    /// `Router<()>` with engine in [`Extension`] so `axum::serve` and [`Router::into_service`] work.
    pub fn build_router(engine: SharedEngine) -> Router {
        Router::new()
            .route("/openapi.json", get(openapi_json_handler))
            .route("/health", get(health_handler))
            .route("/v1/register_geofence", post(register_geofence_handler))
            .route("/v1/register_corridor", post(register_corridor_handler))
            .route(
                "/v1/register_catalog_region",
                post(register_catalog_handler),
            )
            .route("/v1/register_radius", post(register_radius_handler))
            .route("/v1/ingest", post(ingest_handler))
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
        use axum::http::{header::CONTENT_TYPE, Request, StatusCode};
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

        #[tokio::test]
        async fn openapi_json_lists_core_paths() {
            let app = app_router();
            let res = app
                .oneshot(
                    Request::builder()
                        .uri("/openapi.json")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let ct = res
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|h| h.to_str().ok());
            assert!(
                ct.is_some_and(|c| c.contains("json")),
                "content-type: {ct:?}"
            );
            let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let paths = doc["paths"].as_object().expect("paths");
            assert!(paths.contains_key("/health"));
            assert!(paths.contains_key("/v1/ingest"));
            assert!(paths.contains_key("/openapi.json"));
        }

        #[tokio::test]
        async fn register_radius_invalid_returns_structured_error() {
            let app = app_router();
            let bad = r#"{"id":"r1","cx":0,"cy":0,"r":-1}"#;
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/register_radius")
                        .header("content-type", "application/json")
                        .body(Body::from(bad))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["error"]["code"], "invalid_input");
            assert!(v["error"]["message"].as_str().is_some());
        }

        #[tokio::test]
        async fn duplicate_zone_id_returns_409() {
            let app = app_router();
            let poly = r#"{"id":"dup","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}"#;
            let res1 = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/register_geofence")
                        .header("content-type", "application/json")
                        .body(Body::from(poly))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res1.status(), StatusCode::NO_CONTENT);

            let res2 = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/register_radius")
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"id":"dup","cx":0,"cy":0,"r":1}"#.as_bytes()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res2.status(), StatusCode::CONFLICT);
            let body = to_bytes(res2.into_body(), usize::MAX).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["error"]["code"], "conflict");
        }

        #[tokio::test]
        async fn malformed_json_returns_invalid_json() {
            let app = app_router();
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/ingest")
                        .header("content-type", "application/json")
                        .body(Body::from("{not json"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["error"]["code"], "invalid_json");
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
