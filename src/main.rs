//! # Hoya
//!
//! Hoya is a service that executes JavaScript and WebAssembly code from remote URLs.
//! It provides a simple HTTP API for executing code and returning results.
//!
//! ## Features
//!
//! - Execute JavaScript code using QuickJS engine
//! - Execute WebAssembly modules with Wasmtime
//! - Fetch and execute code from remote URLs
//! - Inject utility functions into JavaScript and WASM environments
//!
//! ## API
//!
//! The service exposes a POST endpoint at `/execute` that accepts a JSON payload
//! with a URL pointing to JavaScript (.js) or WebAssembly (.wasm) code.

use axum::{
    extract::{DefaultBodyLimit, Request},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{info, warn, Level};

/// Max request body size accepted by the code-execution endpoints (16 MiB —
/// enough for a base64-encoded WASM module plus JSON overhead).
const MAX_EXECUTE_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Requires a matching `Authorization: Bearer <token>` header when
/// `HOYA_AUTH_TOKEN` is set. Hoya is meant to run as a sidecar reachable only
/// by its owning process (e.g. Hosta), so without the env var set this is a
/// no-op — intended for local development only.
async fn require_auth(req: Request, next: Next) -> Response {
    match std::env::var("HOYA_AUTH_TOKEN") {
        Ok(token) if !token.is_empty() => {
            let provided = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "));
            if provided != Some(token.as_str()) {
                let body = serde_json::json!({
                    "status": "error",
                    "error": {
                        "code": "UNAUTHORIZED",
                        "message": "missing or invalid Authorization header"
                    }
                });
                return (StatusCode::UNAUTHORIZED, Json(body)).into_response();
            }
        }
        _ => {}
    }
    next.run(req).await
}

mod error;
mod handlers;
mod js_engine;
mod models;
mod storage;
mod templates;
mod wasm_engine;

use error::{AppError, ExecuteResponse};
use storage::AppStorage;

/// Data structures for Wasm fetch communication (JSON)
/// These are also defined in wasm_ffis.rs. Consider moving to a shared location.
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchOptions {
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchError {
    code: String,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
    error: Option<WasmFetchError>,
}

/// Type of code to be executed
enum CodeType {
    /// JavaScript code (.js files)
    JavaScript,
    /// WebAssembly code (.wasm files)
    WebAssembly,
}

/// Request payload for the execute endpoint
#[derive(Deserialize)]
struct ExecuteRequest {
    /// URL pointing to JavaScript or WebAssembly code to execute
    url: String,
}

/// Request payload for inline JS execution (Hosta integration)
#[derive(Deserialize)]
struct ExecuteJsInlineRequest {
    /// JavaScript source code to execute
    code: String,
    /// Optional JSON input passed to `main(input, ctx)`
    #[serde(default)]
    input: Option<serde_json::Value>,
    /// Optional datasource JSON available via `ctx.datasource`
    #[serde(default)]
    datasource: Option<serde_json::Value>,
}

/// Request payload for inline WASM execution (Hosta integration)
#[derive(Deserialize)]
struct ExecuteWasmInlineRequest {
    /// Base64-encoded WebAssembly binary
    code: String,
    /// Optional JSON input string (written into WASM memory)
    #[serde(default)]
    input_json: Option<String>,
    /// Optional JSON datasource string (available via get_datasource)
    #[serde(default)]
    datasource_json: Option<String>,
}

/// Handler for the /execute endpoint
///
/// This function handles POST requests to the /execute endpoint. It downloads
/// and executes code from the provided URL, and returns the execution result.
///
/// # Arguments
///
/// * `payload` - JSON payload containing a URL to code to execute
///
/// # Returns
///
/// * `Result<Json<ExecuteResponse>, AppError>` - Execution result or error
async fn execute_handler(
    Json(payload): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, AppError> {
    info!("Execute request received for URL: {}", payload.url);

    // Validate URL format
    if payload.url.trim().is_empty() {
        return Err(AppError::Internal("URL cannot be empty".to_string()));
    }

    // Determine code type from URL
    let code_type = if payload.url.ends_with(".js") {
        CodeType::JavaScript
    } else if payload.url.ends_with(".wasm") {
        CodeType::WebAssembly
    } else {
        return Err(AppError::Internal(
            "Unsupported file extension. Only .js and .wasm are supported.".to_string(),
        ));
    };

    // Download code from URL
    let response = reqwest::get(&payload.url)
        .await
        .map_err(AppError::Reqwest)?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Failed to download code: HTTP status {}",
            response.status()
        )));
    }
    let downloaded_code = response.bytes().await.map_err(AppError::Reqwest)?;

    info!(
        "Code downloaded successfully, size: {} bytes",
        downloaded_code.len()
    );

    // Execute the code
    match code_type {
        CodeType::JavaScript => js_engine::execute_js(downloaded_code),
        CodeType::WebAssembly => wasm_engine::execute_wasm(downloaded_code),
    }
}

/// Handler for inline JS execution (Hosta integration)
///
/// Accepts JS source code directly instead of downloading from a URL.
/// Supports the `main(input, ctx)` calling convention with optional input and datasource.
async fn execute_js_inline_handler(
    Json(payload): Json<ExecuteJsInlineRequest>,
) -> Result<Json<ExecuteResponse>, AppError> {
    info!(
        "Inline JS execution request received, code size: {} bytes",
        payload.code.len()
    );

    if payload.code.trim().is_empty() {
        return Err(AppError::Internal("Code cannot be empty".to_string()));
    }

    let code_bytes = bytes::Bytes::from(payload.code.into_bytes());
    js_engine::execute_js_with_input(code_bytes, payload.input, payload.datasource)
}

/// Handler for inline WASM execution (Hosta integration)
///
/// Accepts base64-encoded WASM binary directly instead of downloading from a URL.
async fn execute_wasm_inline_handler(
    Json(payload): Json<ExecuteWasmInlineRequest>,
) -> Result<Json<ExecuteResponse>, AppError> {
    info!(
        "Inline WASM execution request received, code size: {} bytes",
        payload.code.len()
    );

    if payload.code.trim().is_empty() {
        return Err(AppError::Internal("Code cannot be empty".to_string()));
    }

    // Decode base64 to binary
    use base64::Engine as _;
    let wasm_bytes = base64::engine::general_purpose::STANDARD
        .decode(&payload.code)
        .map_err(|e| AppError::Internal(format!("Failed to decode base64 WASM: {}", e)))?;

    let wasm_bytes = bytes::Bytes::from(wasm_bytes);
    wasm_engine::execute_wasm_with_input(wasm_bytes, payload.input_json, payload.datasource_json)
}

/// Health check endpoint - returns service status
async fn health_handler() -> Json<serde_json::Value> {
    info!("Health check requested");
    Json(serde_json::json!({
        "status": "healthy",
        "service": "hoya",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Readiness check endpoint - returns service readiness
async fn ready_handler() -> Json<serde_json::Value> {
    info!("Readiness check requested");
    // TODO: Add actual readiness checks (database connections, external services, etc.)
    // For now, just return ready since we don't have external dependencies
    Json(serde_json::json!({
        "status": "ready",
        "service": "hoya",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// API documentation endpoint — returns a self-describing JSON listing all
/// available endpoints, their methods, and descriptions.
async fn api_docs_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "hoya",
        "description": "Sandbox execution service for JavaScript and WebAssembly code",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": [
            {
                "method": "GET",
                "path": "/",
                "description": "Web UI — application management interface",
                "auth_required": false
            },
            {
                "method": "GET",
                "path": "/create",
                "description": "Web UI — create a new app",
                "auth_required": false
            },
            {
                "method": "POST",
                "path": "/create",
                "description": "Web UI — submit a new app creation form",
                "auth_required": false
            },
            {
                "method": "GET",
                "path": "/app/:id",
                "description": "Web UI — view app details",
                "auth_required": false
            },
            {
                "method": "GET",
                "path": "/execute/:id",
                "description": "Web UI — app execution page",
                "auth_required": false
            },
            {
                "method": "POST",
                "path": "/execute/:id",
                "description": "Web UI — execute an app in sandbox",
                "auth_required": false
            },
            {
                "method": "POST",
                "path": "/execute",
                "description": "Download and execute code from a URL (.js or .wasm)",
                "auth_required": true,
                "body": {
                    "url": "string (required) — URL to .js or .wasm file"
                }
            },
            {
                "method": "POST",
                "path": "/execute/js",
                "description": "Execute JavaScript source code inline (Hosta integration)",
                "auth_required": true,
                "body": {
                    "code": "string (required) — JavaScript source code",
                    "input": "object (optional) — JSON input passed to main(input, ctx)",
                    "datasource": "object (optional) — datasource JSON available via ctx.datasource"
                }
            },
            {
                "method": "POST",
                "path": "/execute/wasm",
                "description": "Execute base64-encoded WebAssembly module inline (Hosta integration)",
                "auth_required": true,
                "body": {
                    "code": "string (required) — base64-encoded WASM binary",
                    "input_json": "string (optional) — JSON input written into WASM memory",
                    "datasource_json": "string (optional) — JSON datasource available via get_datasource()"
                }
            },
            {
                "method": "GET",
                "path": "/health",
                "description": "Health check — returns service status",
                "auth_required": false
            },
            {
                "method": "GET",
                "path": "/ready",
                "description": "Readiness check — returns service readiness",
                "auth_required": false
            },
            {
                "method": "GET",
                "path": "/api",
                "description": "API documentation — this endpoint",
                "auth_required": false
            }
        ],
        "limits": {
            "max_body_size": "16 MB",
            "js_timeout": "5 seconds",
            "js_memory_limit": "64 MB",
            "wasm_fuel": "100,000",
            "wasm_memory": "32 MB",
            "fetch_timeout": "5 seconds",
            "fetch_max_response": "512 KB"
        },
        "auth": {
            "scheme": "Bearer",
            "env_var": "HOYA_AUTH_TOKEN",
            "note": "When HOYA_AUTH_TOKEN is not set, authentication is disabled (local development mode)"
        }
    }))
}

/// 404 handler — returns JSON error for unmatched routes
async fn not_found_handler() -> impl IntoResponse {
    let error_response = serde_json::json!({
        "error": {
            "code": "NOT_FOUND",
            "message": "The requested resource was not found",
            "details": {
                "type": "route_not_found",
                "description": "This endpoint does not exist. Available endpoints: /, /create, /app/:id, /execute/:id, /execute, /execute/js, /execute/wasm, /api, /health, /ready"
            }
        },
        "status": "error",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    (StatusCode::NOT_FOUND, Json(error_response))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    info!("Starting Hoya service...");
    if std::env::var("HOYA_AUTH_TOKEN").unwrap_or_default().is_empty() {
        warn!("HOYA_AUTH_TOKEN is not set — /execute* endpoints are unauthenticated. Set it in production.");
    }

    // Initialize storage
    let storage = Arc::new(AppStorage::new());

    // Code-execution endpoints: bearer-token auth + explicit body size cap.
    let execute_routes = Router::new()
        .route("/execute", post(execute_handler))
        .route("/execute/js", post(execute_js_inline_handler))
        .route("/execute/wasm", post(execute_wasm_inline_handler))
        .layer(middleware::from_fn(require_auth))
        .layer(DefaultBodyLimit::max(MAX_EXECUTE_BODY_BYTES));

    // Create a router with all endpoints
    let app = Router::new()
        .merge(execute_routes)
        .route("/api", get(api_docs_handler))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        // Web UI endpoints
        .route("/", get(handlers::index_handler))
        .route("/create", get(handlers::create_page_handler))
        .route("/create", post(handlers::create_submit_handler))
        .route("/app/:id", get(handlers::app_detail_handler))
        .route("/execute/:id", get(handlers::execute_page_handler))
        .route("/execute/:id", post(handlers::execute_sandbox_handler))
        // Add state and fallback
        .with_state(storage)
        .fallback(not_found_handler)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    // Bind to all interfaces; port is configurable via PORT (defaults to
    // 3000) so Hosta can run hoya as a sidecar on a non-default port.
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service(),
    )
    .await
    .unwrap();
}
