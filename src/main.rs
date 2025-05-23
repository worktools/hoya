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

use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

mod error;
mod js_engine;
mod wasm_engine;

use error::{AppError, ExecuteResponse};

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
    println!("Received URL: {}", payload.url);

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

    // TODO: JavaScript execution and WebAssembly execution

    match code_type {
        CodeType::JavaScript => js_engine::execute_js(downloaded_code),
        CodeType::WebAssembly => wasm_engine::execute_wasm(downloaded_code),
    }
}

#[tokio::main]
async fn main() {
    // Create a router with a single POST route for the execute endpoint
    let app = Router::new().route("/execute", post(execute_handler));

    // Bind to localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service(),
    )
    .await
    .unwrap();
}
