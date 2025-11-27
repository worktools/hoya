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
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{info, Level};

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

/// 404 handler - returns JSON error for unmatched routes
async fn not_found_handler() -> impl IntoResponse {
    let error_response = serde_json::json!({
        "error": {
            "code": "NOT_FOUND",
            "message": "The requested resource was not found",
            "details": {
                "type": "route_not_found",
                "description": "This endpoint does not exist. Available endpoints: /, /create, /app/:id, /execute/:id, /execute, /health, /ready"
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

    // Initialize storage
    let storage = Arc::new(AppStorage::new());

    // Create a router with all endpoints
    let app = Router::new()
        // API endpoints
        .route("/execute", post(execute_handler))
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

    // Bind to all interfaces on port 3000 for Kubernetes compatibility
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Listening on {}", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service(),
    )
    .await
    .unwrap();
}
