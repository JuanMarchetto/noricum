/// REST API server for Noricum migration tools.
///
/// Provides HTTP endpoints that map to the core migration pipeline:
/// health check, analysis, compilation check, scoring, diff testing, and migration.
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use noricum_core::MigrationConfig;
use serde::{Deserialize, Serialize};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Maximum request body size: 10 MB.
const MAX_SOURCE_SIZE: usize = 10 * 1024 * 1024;

/// Default rate limit: 60 requests per minute per IP.
pub const DEFAULT_RATE_LIMIT_RPM: usize = 60;

/// Simple per-IP rate limiter using a sliding window (1 minute).
pub struct IpRateLimiter {
    max_rpm: usize,
    requests: std::sync::Mutex<HashMap<IpAddr, Vec<std::time::Instant>>>,
}

impl IpRateLimiter {
    /// Create a new rate limiter with the given max requests per minute.
    pub fn new(max_rpm: usize) -> Self {
        Self {
            max_rpm,
            requests: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Check if a request from the given IP should be allowed.
    pub fn check(&self, ip: IpAddr) -> bool {
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(60);
        let mut map = self.requests.lock().expect("rate limiter lock poisoned");
        let timestamps = map.entry(ip).or_default();
        timestamps.retain(|t| now.duration_since(*t) < window);
        if timestamps.len() >= self.max_rpm {
            return false;
        }
        timestamps.push(now);
        true
    }
}

/// Shared application state.
pub struct AppState {
    pub config: MigrationConfig,
    /// Optional API key for request authentication.
    pub api_key: Option<String>,
    /// Whether the server is bound to a non-localhost address.
    pub is_public: bool,
    /// Per-IP rate limiter.
    pub rate_limiter: IpRateLimiter,
}

/// Build the API router with all endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    // Build CORS layer: use env-based allowlist or restrict to localhost
    let cors = match std::env::var("NORICUM_CORS_ORIGINS") {
        Ok(origins) => {
            let allowed: Vec<_> = origins
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            CorsLayer::new().allow_origin(AllowOrigin::list(allowed))
        }
        Err(_) => CorsLayer::new().allow_origin(AllowOrigin::list([
            "http://localhost:3000"
                .parse()
                .expect("valid localhost URL literal"),
            "http://127.0.0.1:3000"
                .parse()
                .expect("valid localhost URL literal"),
        ])),
    };

    // Limit concurrent requests to prevent resource exhaustion (configurable via env)
    let max_concurrent: usize = std::env::var("NORICUM_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
        .max(1);

    // Security headers middleware
    let security_headers = axum::middleware::from_fn(add_security_headers);

    // Per-IP rate limiting middleware
    let rate_limit = axum::middleware::from_fn_with_state(state.clone(), rate_limit_middleware);

    Router::new()
        .route("/api/health", get(health))
        .route("/api/migrate", post(migrate))
        .route("/api/analyze", post(analyze))
        .route("/api/check", post(check))
        .route("/api/score", post(score))
        .route("/api/diff-test", post(diff_test))
        .layer(rate_limit)
        .layer(security_headers)
        .layer(ConcurrencyLimitLayer::new(max_concurrent))
        .layer(cors)
        .with_state(state)
}

/// Middleware that adds security headers to all responses.
async fn add_security_headers(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        "nosniff".parse().expect("valid header value literal"),
    );
    headers.insert(
        "x-frame-options",
        "DENY".parse().expect("valid header value literal"),
    );
    headers.insert(
        "content-security-policy",
        "default-src 'none'; frame-ancestors 'none'"
            .parse()
            .expect("valid header value literal"),
    );
    headers.insert(
        "x-xss-protection",
        "1; mode=block".parse().expect("valid header value literal"),
    );
    headers.insert(
        "cache-control",
        "no-store, no-cache, must-revalidate"
            .parse()
            .expect("valid header value literal"),
    );
    response
}

/// Per-IP rate limiting middleware.
async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Extract client IP from X-Forwarded-For or fall back to 127.0.0.1
    let ip: IpAddr = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

    if !state.rate_limiter.check(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: "rate limit exceeded — try again later".to_string(),
            }),
        )
            .into_response();
    }
    next.run(req).await
}

/// Validate API key if configured. Returns an error response if auth fails.
///
/// Uses constant-time comparison to prevent timing side-channel attacks.
fn check_api_key(state: &AppState, headers: &HeaderMap) -> Result<(), impl IntoResponse> {
    use subtle::ConstantTimeEq;

    if let Some(ref expected) = state.api_key {
        let provided = headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
            });
        match provided {
            Some(key)
                if key.len() == expected.len()
                    && bool::from(key.as_bytes().ct_eq(expected.as_bytes())) =>
            {
                Ok(())
            }
            _ => Err(error_response(
                StatusCode::UNAUTHORIZED,
                "invalid or missing API key",
            )),
        }
    } else if state.is_public {
        // Require authentication when serving on non-localhost addresses
        Err(error_response(
            StatusCode::UNAUTHORIZED,
            "API key required when serving on non-localhost. Set NORICUM_API_KEY env var.",
        ))
    } else {
        Ok(())
    }
}

/// Reject source code that exceeds the maximum size limit.
fn check_source_size(source: &str) -> Result<(), impl IntoResponse> {
    if source.len() > MAX_SOURCE_SIZE {
        Err(error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("source exceeds maximum size of {} bytes", MAX_SOURCE_SIZE),
        ))
    } else {
        Ok(())
    }
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

async fn analyze(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_api_key(&state, &headers) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.source) {
        return resp.into_response();
    }
    let difficulty = noricum_core::router::classify_difficulty(&req.source);
    let lines = req.source.lines().count();
    Json(AnalyzeResponse {
        difficulty: format!("{difficulty:?}"),
        lines,
    })
    .into_response()
}

async fn check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CheckRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_api_key(&state, &headers) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.source) {
        return resp.into_response();
    }
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

async fn score(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ScoreRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_api_key(&state, &headers) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.source) {
        return resp.into_response();
    }
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&req.source);
    let clippy = match noricum_tools::compiler::run_clippy_on_source(&req.source) {
        Ok(warnings) => warnings,
        Err(e) => {
            tracing::warn!(error = %e, "clippy analysis failed, score may be incomplete");
            Vec::new()
        }
    };
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
    .into_response()
}

async fn diff_test(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DiffTestRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_api_key(&state, &headers) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.c_source) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.rust_source) {
        return resp.into_response();
    }
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
    headers: HeaderMap,
    Json(req): Json<MigrateRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_api_key(&state, &headers) {
        return resp.into_response();
    }
    if let Err(resp) = check_source_size(&req.source) {
        return resp.into_response();
    }
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
            api_key: None,
            is_public: false,
            rate_limiter: IpRateLimiter::new(DEFAULT_RATE_LIMIT_RPM),
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
