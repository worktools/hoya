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
use rquickjs::{Context, Result as QuickJsResult, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Engine, Linker, Memory, Module, Store};

mod error;
mod js_ffis;
mod wasm_ffis;

use error::{AppError, ExecuteResponse, ExecutionMetadata};

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

/// Context for Wasm store to hold shared resources like the HTTP client
///
/// This struct provides access to shared resources for WebAssembly modules.
/// It includes a reqwest HTTP client and optional memory reference.
pub struct WasmCtx {
    /// HTTP client for making network requests
    reqwest_client: reqwest::Client,
    /// Optional reference to the WebAssembly module's memory
    memory: Option<Memory>,
    /// Captured stdout content
    stdout: Arc<Mutex<String>>,
    /// Captured stderr content
    stderr: Arc<Mutex<String>>,
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
        CodeType::JavaScript => {
            println!(
                "Code type: JavaScript, size: {} bytes",
                downloaded_code.len()
            );

            let start_time = std::time::Instant::now();
            let resource_size = downloaded_code.len();

            let js_code = String::from_utf8(downloaded_code.to_vec()).map_err(|e| {
                AppError::Internal(format!(
                    "Failed to convert downloaded code to string: {}",
                    e
                ))
            })?;

            // Create buffers for stdout and stderr
            let stdout_buffer = Arc::new(Mutex::new(String::new()));
            let stderr_buffer = Arc::new(Mutex::new(String::new()));

            let runtime = Runtime::new()?;
            let context = Context::full(&runtime)?;

            // Execute JavaScript with output capturing
            let result = context.with(|ctx| -> QuickJsResult<String> {
                // Register JavaScript functions with stdout/stderr capture
                let output_buffers = js_ffis::OutputBuffers {
                    stdout: stdout_buffer.clone(),
                    stderr: stderr_buffer.clone(),
                };
                crate::js_ffis::register_to_globals_with_capture(&ctx, output_buffers)?;

                // Execute the JS code
                let result = ctx.eval::<Value, _>(js_code.as_str())?;

                // Convert the result to a string
                let output = match result.type_of() {
                    rquickjs::Type::String => result.as_string().unwrap().to_string()?,
                    rquickjs::Type::Int => result.as_int().unwrap().to_string(),
                    rquickjs::Type::Bool => result.as_bool().unwrap().to_string(),
                    rquickjs::Type::Float => result.as_float().unwrap().to_string(),
                    rquickjs::Type::Null => "null".to_string(),
                    rquickjs::Type::Undefined => "undefined".to_string(),
                    _ => format!(
                        "Execution resulted in a non-primitive type: {:?}",
                        result.type_of()
                    ),
                };

                Ok(output)
            })?;

            // Calculate execution time
            let execution_time = start_time.elapsed().as_millis() as u64;

            // Generate ISO timestamp
            let now = SystemTime::now();
            let timestamp = match now.duration_since(UNIX_EPOCH) {
                Ok(duration) => {
                    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                        duration.as_secs() as i64,
                        duration.subsec_nanos(),
                    )
                    .unwrap_or_else(|| chrono::Utc::now());
                    datetime.to_rfc3339()
                }
                Err(_) => chrono::Utc::now().to_rfc3339(),
            };

            // Get the captured stdout and stderr
            let stdout = stdout_buffer.lock().map(|s| s.clone()).unwrap_or_default();
            let stderr = stderr_buffer.lock().map(|s| s.clone()).unwrap_or_default();

            // Return the execution result with metadata
            Ok(Json(ExecuteResponse {
                status: "success".to_string(),
                output: Some(result),
                stdout: Some(stdout),
                stderr: Some(stderr),
                error: None,
                metadata: ExecutionMetadata {
                    execution_time,
                    code_type: "javascript".to_string(),
                    timestamp,
                    resource_size,
                },
            }))
        }
        CodeType::WebAssembly => {
            println!(
                "Code type: WebAssembly, size: {} bytes",
                downloaded_code.len()
            );

            let start_time = std::time::Instant::now();
            let resource_size = downloaded_code.len();

            let engine = Engine::default();
            let wasm_shared_data = WasmCtx {
                reqwest_client: reqwest::Client::new(),
                memory: None,
                stdout: Arc::new(Mutex::new(String::new())),
                stderr: Arc::new(Mutex::new(String::new())),
            };
            let mut store = Store::new(&engine, wasm_shared_data);
            let mut linker = Linker::new(&engine);

            // Call the function from wasm_ffis to register linker functions
            wasm_ffis::register_linker_functions(&mut linker)?;

            let module = Module::from_binary(&engine, &downloaded_code)?;

            let instance = linker.instantiate(&mut store, &module)?;

            if let Some(wasmtime::Extern::Memory(mem)) = instance.get_export(&mut store, "memory") {
                store.data_mut().memory = Some(mem);
            } else {
                return Err(AppError::Internal(
                    "WASM module does not export 'memory'".to_string(),
                ));
            }

            // Calculate execution time before function call
            let instantiation_time = start_time.elapsed().as_millis() as u64;

            // Generate ISO timestamp
            let now = SystemTime::now();
            let timestamp = match now.duration_since(UNIX_EPOCH) {
                Ok(duration) => {
                    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                        duration.as_secs() as i64,
                        duration.subsec_nanos(),
                    )
                    .unwrap_or_else(|| chrono::Utc::now());
                    datetime.to_rfc3339()
                }
                Err(_) => chrono::Utc::now().to_rfc3339(),
            };

            let metadata = ExecutionMetadata {
                execution_time: instantiation_time,
                code_type: "webassembly".to_string(),
                timestamp,
                resource_size: resource_size,
            };

            if let Ok(start_func) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
                start_func
                    .call(&mut store, ())
                    .map_err(AppError::Wasmtime)?;

                // Update execution time including _start function
                let total_execution_time = start_time.elapsed().as_millis() as u64;
                let updated_metadata = ExecutionMetadata {
                    execution_time: total_execution_time,
                    ..metadata
                };

                // Get the captured stdout and stderr
                let stdout = store
                    .data()
                    .stdout
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();
                let stderr = store
                    .data()
                    .stderr
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();

                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some("WASM module executed (_start)".to_string()),
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    error: None,
                    metadata: updated_metadata,
                }))
            } else {
                // Get the captured stdout and stderr
                let stdout = store
                    .data()
                    .stdout
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();
                let stderr = store
                    .data()
                    .stderr
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();

                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some(
                        "WASM module instantiated (no _start called or found)".to_string(),
                    ),
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    error: None,
                    metadata,
                }))
            }
        }
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
