/// REST API server for Noricum migration tools.
///
/// Provides HTTP endpoints that map to the core migration pipeline:
/// health check, analysis, compilation check, scoring, diff testing, and migration.
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use noricum_core::MigrationConfig;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

/// Shared application state.
pub struct AppState {
    pub config: MigrationConfig,
}

/// Build the API router with all endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/migrate", post(migrate))
        .route("/api/analyze", post(analyze))
        .route("/api/check", post(check))
        .route("/api/score", post(score))
        .route("/api/diff-test", post(diff_test))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// --- Request / Response types ---

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[derive(Deserialize)]
struct AnalyzeRequest {
    source: String,
}

#[derive(Serialize)]
struct AnalyzeResponse {
    difficulty: String,
    lines: usize,
}

#[derive(Deserialize)]
struct CheckRequest {
    source: String,
}

#[derive(Serialize)]
struct CheckResponse {
    success: bool,
    errors: Vec<String>,
}

#[derive(Deserialize)]
struct ScoreRequest {
    source: String,
    #[serde(default)]
    c_source: Option<String>,
}

#[derive(Serialize)]
struct ScoreResponse {
    score: u32,
    unsafe_count: u32,
}

#[derive(Deserialize)]
struct DiffTestRequest {
    c_source: String,
    rust_source: String,
}

#[derive(Serialize)]
struct DiffTestResponse {
    passed: bool,
    c_output: String,
    rust_output: String,
    c_compiled: bool,
    rust_compiled: bool,
    c_exit_code: i32,
    rust_exit_code: i32,
}

#[derive(Deserialize)]
struct MigrateRequest {
    source: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize)]
struct MigrateResponse {
    name: String,
    state: String,
    rust_output: Option<String>,
    idiomatic_score: Option<u32>,
    unsafe_count: Option<u32>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_response(status: StatusCode, msg: impl Into<String>) -> impl IntoResponse {
    (status, Json(ErrorResponse { error: msg.into() }))
}

// --- Handlers ---

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn analyze(Json(req): Json<AnalyzeRequest>) -> impl IntoResponse {
    let difficulty = noricum_core::router::classify_difficulty(&req.source);
    let lines = req.source.lines().count();
    Json(AnalyzeResponse {
        difficulty: format!("{difficulty:?}"),
        lines,
    })
}

async fn check(Json(req): Json<CheckRequest>) -> impl IntoResponse {
    match noricum_tools::compiler::check_rust_compiles(&req.source) {
        Ok(result) => {
            let errors: Vec<String> = if !result.success {
                result
                    .stderr
                    .lines()
                    .filter(|l| l.contains("error"))
                    .map(String::from)
                    .collect()
            } else {
                Vec::new()
            };
            (
                StatusCode::OK,
                Json(CheckResponse {
                    success: result.success,
                    errors,
                }),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn score(Json(req): Json<ScoreRequest>) -> impl IntoResponse {
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&req.source);
    let clippy = noricum_tools::compiler::run_clippy_on_source(&req.source).unwrap_or_default();
    let c_source = req.c_source.as_deref().unwrap_or("");
    let score = noricum_validation::compute_idiomatic_score_from_source(
        unsafe_count,
        clippy.len() as u32,
        &req.source,
        c_source,
    );
    Json(ScoreResponse {
        score,
        unsafe_count,
    })
}

async fn diff_test(Json(req): Json<DiffTestRequest>) -> impl IntoResponse {
    match noricum_tools::diff_test::run_diff_test(&req.c_source, &req.rust_source) {
        Ok(result) => (
            StatusCode::OK,
            Json(DiffTestResponse {
                passed: result.passed,
                c_output: result.c_output,
                rust_output: result.rust_output,
                c_compiled: result.c_compiled,
                rust_compiled: result.rust_compiled,
                c_exit_code: result.c_exit_code,
                rust_exit_code: result.rust_exit_code,
            }),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn migrate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MigrateRequest>,
) -> impl IntoResponse {
    let name = req.name.unwrap_or_else(|| "input".to_string());

    // Write source to temp file and run the sync pipeline
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                .into_response();
        }
    };
    let c_file = tmp.path().join(format!("{name}.c"));
    if let Err(e) = std::fs::write(&c_file, &req.source) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    // Try async (LLM) pipeline if API key is available, otherwise sync
    let unit = if state.config.anthropic_api_key.is_some() {
        match noricum_core::orchestrator::migrate_file(&c_file, &state.config).await {
            Ok(u) => u,
            Err(e) => {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                    .into_response();
            }
        }
    } else {
        match noricum_core::orchestrator::migrate_file_sync(&c_file) {
            Ok(u) => u,
            Err(e) => {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                    .into_response();
            }
        }
    };

    (
        StatusCode::OK,
        Json(MigrateResponse {
            name: unit.name,
            state: format!("{:?}", unit.state),
            rust_output: unit.rust_output,
            idiomatic_score: unit.idiomatic_score,
            unsafe_count: unit.unsafe_count,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            config: MigrationConfig::default(),
        })
    }

    async fn response_json(app: Router, req: Request<Body>) -> serde_json::Value {
        let response = app.oneshot(req).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());
        let req = Request::get("/api/health").body(Body::empty()).unwrap();
        let json = response_json(app, req).await;
        assert_eq!(json["status"], "ok");
        assert!(json["version"].is_string());
    }

    #[tokio::test]
    async fn test_analyze_simple_c() {
        let app = build_router(test_state());
        let req = Request::post("/api/analyze")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({
                    "source": "int add(int a, int b) { return a + b; }"
                }))
                .unwrap(),
            ))
            .unwrap();
        let json = response_json(app, req).await;
        assert_eq!(json["difficulty"], "Easy");
    }

    #[tokio::test]
    async fn test_check_valid_rust() {
        let app = build_router(test_state());
        let req = Request::post("/api/check")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({
                    "source": "pub fn add(a: i32, b: i32) -> i32 { a + b }"
                }))
                .unwrap(),
            ))
            .unwrap();
        let json = response_json(app, req).await;
        assert_eq!(json["success"], true);
    }

    #[tokio::test]
    async fn test_check_invalid_rust() {
        let app = build_router(test_state());
        let req = Request::post("/api/check")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({
                    "source": "fn bad( { invalid }"
                }))
                .unwrap(),
            ))
            .unwrap();
        let json = response_json(app, req).await;
        assert_eq!(json["success"], false);
    }

    #[tokio::test]
    async fn test_score_endpoint() {
        let app = build_router(test_state());
        let req = Request::post("/api/score")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({
                    "source": "pub fn add(a: i32, b: i32) -> i32 { a + b }",
                    "c_source": "int add(int a, int b) { return a + b; }"
                }))
                .unwrap(),
            ))
            .unwrap();
        let json = response_json(app, req).await;
        assert!(json["score"].as_u64().unwrap() > 0);
        assert_eq!(json["unsafe_count"], 0);
    }

    #[tokio::test]
    async fn test_diff_test_endpoint() {
        let app = build_router(test_state());
        let req = Request::post("/api/diff-test")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({
                    "c_source": "#include <stdio.h>\nint main(void) { printf(\"5\\n\"); return 0; }",
                    "rust_source": "fn main() { println!(\"5\"); }"
                }))
                .unwrap(),
            ))
            .unwrap();
        let json = response_json(app, req).await;
        assert_eq!(json["passed"], true);
        assert_eq!(json["c_compiled"], true);
        assert_eq!(json["rust_compiled"], true);
    }
}
