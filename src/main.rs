use axum::{routing::post, Json, Router};
use rquickjs::{Context, Result as QuickJsResult, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Engine, Linker, Memory, Module, Store};

// Import modules
mod error;
mod js_ffis;
mod wasm_ffis;

// Import types from error module
use error::{AppError, ExecuteResponse, ExecutionMetadata};

// Data structures for Wasm fetch communication (JSON)
// These are also defined in wasm_ffis.rs. Consider moving to a shared location.
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchOptions {
    url: String,
    method: String, // e.g., "GET", "POST"
    headers: HashMap<String, String>,
    body: Option<String>, // For simplicity, string body. Could be base64 for binary data.
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
    body: String, // Body as string (e.g., text or base64 encoded binary)
    error: Option<WasmFetchError>, // Optional error information
}

// AppError definition has been moved to error.rs
// Also moved ErrorInfo, ExecutionMetadata, and ExecuteResponse to error.rs

// Context for Wasm store to hold shared resources like the HTTP client
// Make WasmCtx public so it can be accessed by wasm_ffis.rs
pub struct WasmCtx {
    reqwest_client: reqwest::Client,
    memory: Option<Memory>, // Add Option<Memory> to store the Wasm module's memory
}

enum CodeType {
    JavaScript,
    WebAssembly,
}

#[derive(Deserialize)]
struct ExecuteRequest {
    url: String,
}

async fn execute_handler(
    Json(payload): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, AppError> {
    println!("Received URL: {}", payload.url);

    // FR1.3: Determine code type from URL
    let code_type = if payload.url.ends_with(".js") {
        CodeType::JavaScript
    } else if payload.url.ends_with(".wasm") {
        CodeType::WebAssembly
    } else {
        return Err(AppError::Internal(
            "Unsupported file extension. Only .js and .wasm are supported.".to_string(),
        ));
    };

    // FR1.4: Download code from URL
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

    // TODO: Implement FR2 (JavaScript execution)
    // TODO: Implement FR3 (WebAssembly execution)

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

            let runtime = Runtime::new()?;
            let context = Context::full(&runtime)?;

            // Execute JavaScript with simpler approach
            let result = context.with(|ctx| -> QuickJsResult<String> {
                // Register JavaScript functions from the js_ffis module
                crate::js_ffis::register_to_globals(&ctx)?;

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

            // Return the execution result with metadata
            Ok(Json(ExecuteResponse {
                status: "success".to_string(),
                output: Some(result),
                error: None,
                metadata: ExecutionMetadata {
                    execution_time: execution_time,
                    code_type: "javascript".to_string(),
                    timestamp,
                    resource_size: resource_size,
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
                reqwest_client: reqwest::Client::new(), // This client is now used by wasm_ffis
                memory: None,
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

                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some("WASM module executed (_start)".to_string()),
                    error: None,
                    metadata: updated_metadata,
                }))
            } else {
                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some(
                        "WASM module instantiated (no _start called or found)".to_string(),
                    ),
                    error: None,
                    metadata,
                }))
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/execute", post(execute_handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service(),
    ) // Changed axum::Server::bind to axum::serve
    .await
    .unwrap();
}
