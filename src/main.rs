use axum::{routing::post, Json, Router};
use rquickjs::{Context, Value, Result as QuickJsResult, Runtime}; // Added Ctx, QuickJsResult, ArrayBuffer, Runtime
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Engine, Linker, Module, Store, Memory}; // Removed unused imports
use anyhow::{anyhow, Result as AnyhowResult, Error as AnyhowError}; // Add anyhow types

// Add axum response types for AppError
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;


// Data structures for Wasm fetch communication (JSON)
#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchOptions {
    url: String,
    method: String, // e.g., "GET", "POST"
    headers: HashMap<String, String>,
    body: Option<String>, // For simplicity, string body. Could be base64 for binary data.
}

#[derive(Serialize, Deserialize, Debug)]
struct WasmFetchResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String, // Body as string (e.g., text or base64 encoded binary)
}

// Define AppError for handler
#[derive(Debug)]
enum AppError {
    QuickJs(rquickjs::Error),
    Wasmtime(AnyhowError),
    Reqwest(reqwest::Error),
    Internal(String),
}

impl From<rquickjs::Error> for AppError {
    fn from(err: rquickjs::Error) -> Self {
        AppError::QuickJs(err)
    }
}

impl From<AnyhowError> for AppError { // For wasmtime::Error and other anyhow errors
    fn from(err: AnyhowError) -> Self {
        AppError::Wasmtime(err)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Reqwest(err)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Internal(s.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status_code, error_message) = match self {
            AppError::QuickJs(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("JavaScript Execution Error: {}", e)),
            AppError::Wasmtime(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("WebAssembly Execution Error: {}", e)),
            AppError::Reqwest(e) => (StatusCode::BAD_GATEWAY, format!("Failed to fetch resource: {}", e)),
            AppError::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s),
        };
        let body = Json(ExecuteResponse {
            status: "error".to_string(),
            output: None,
            error: Some(error_message),
        });
        (status_code, body).into_response()
    }
}


// Context for Wasm store to hold shared resources like the HTTP client
struct WasmCtx {
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

#[derive(Serialize)]
struct ExecuteResponse {
    status: String,
    output: Option<String>,
    error: Option<String>,
}

async fn execute_handler(Json(payload): Json<ExecuteRequest>) -> Result<Json<ExecuteResponse>, AppError> {
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
    let response = reqwest::get(&payload.url).await.map_err(AppError::Reqwest)?;

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
            let js_code = String::from_utf8(downloaded_code.to_vec())
                .map_err(|e| AppError::Internal(format!("Failed to convert downloaded code to string: {}", e)))?;
            
            let runtime = Runtime::new()?;
            let context = Context::full(&runtime)?;

            // Execute JavaScript with simpler approach
            let result = context.with(|ctx| -> QuickJsResult<String> {
                // Get globals object
                let globals = ctx.globals();
                
                // Create app_log function with more limited functionality
                let app_log_str = r#"
                function app_log(level, message) {
                    console.log("[JS LOG - " + (level || 'INFO').toUpperCase() + "]: " + (message || ''));
                }
                "#;
                ctx.eval::<(), _>(app_log_str)?;

                // Create get_unixtime function - simplified
                let get_unixtime_str = r#"
                function get_unixtime() {
                    return Date.now() / 1000;
                }
                "#;
                ctx.eval::<(), _>(get_unixtime_str)?;
                
                // Create fetch function - simplified error response
                let fetch_str = r#"
                function fetch(options) {
                    throw new Error("fetch is not fully implemented in this runtime");
                }
                "#;
                ctx.eval::<(), _>(fetch_str)?;
                
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
                    _ => format!("Execution resulted in a non-primitive type: {:?}", result.type_of()),
                };
                
                Ok(output)
            })?;

            // Return the execution result
            Ok(Json(ExecuteResponse {
                status: "success".to_string(),
                output: Some(result),
                error: None,
            }))
        }
        CodeType::WebAssembly => {
            println!(
                "Code type: WebAssembly, size: {} bytes",
                downloaded_code.len()
            );

            let engine = Engine::default();
            let wasm_shared_data = WasmCtx {
                reqwest_client: reqwest::Client::new(),
                memory: None,
            };
            let mut store = Store::new(&engine, wasm_shared_data);
            let mut linker = Linker::new(&engine);

            // FR3.3: Inject app_log (WASM)
            linker.func_wrap(
                "env",
                "app_log",
                |caller: Caller<'_, WasmCtx>, level_ptr: u32, level_len: u32, msg_ptr: u32, msg_len: u32| -> AnyhowResult<()> { // Removed mut
                    let memory = caller.data().memory.ok_or_else(|| anyhow!("app_log: memory not initialized in WasmCtx"))?;
                    let level_bytes = memory.data(&caller)
                        .get(level_ptr as usize..(level_ptr + level_len) as usize)
                        .ok_or_else(|| anyhow!("app_log: level pointer/length out of bounds"))?;
                    let level_str = std::str::from_utf8(level_bytes).map_err(|_| anyhow!("app_log: level not valid UTF-8"))?;
                    let msg_bytes = memory.data(&caller)
                        .get(msg_ptr as usize..(msg_ptr + msg_len) as usize)
                        .ok_or_else(|| anyhow!("app_log: message pointer/length out of bounds"))?;
                    let msg_str = std::str::from_utf8(msg_bytes).map_err(|_| anyhow!("app_log: message not valid UTF-8"))?;
                    println!("[WASM LOG - {}]: {}", level_str.to_uppercase(), msg_str);
                    Ok(())
                },
            )?;

            // FR3.3: Inject get_unixtime (WASM)
            linker.func_wrap("env", "get_unixtime", |_caller: Caller<'_, WasmCtx>| -> AnyhowResult<u64> {
                match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(n) => Ok(n.as_secs()),
                    Err(_) => Err(anyhow!("get_unixtime: Failed to get system time")),
                }
            })?;

            // FR3.3 Inject fetch (WASM)
            linker.func_wrap(
                "env",
                "fetch",
                |mut caller: Caller<'_, WasmCtx>, options_ptr: u32, options_len: u32, resp_buf_ptr: u32, resp_buf_max_len: u32| -> AnyhowResult<i32> {
                    let memory = caller.data().memory.ok_or_else(|| anyhow!("fetch: memory not initialized in WasmCtx"))?;

                    let options_bytes_vec: Vec<u8> = memory.data(&caller)
                        .get(options_ptr as usize..(options_ptr + options_len) as usize)
                        .ok_or_else(|| anyhow!("fetch: options pointer/length out of bounds"))?
                        .to_vec();
                    
                    let fetch_options: WasmFetchOptions = serde_json::from_slice(&options_bytes_vec)
                        .map_err(|e| anyhow!("fetch: failed to deserialize options JSON: {}", e))?;

                    let http_method = reqwest::Method::from_bytes(fetch_options.method.as_bytes())
                        .map_err(|_| anyhow!("fetch: invalid HTTP method string: {}", fetch_options.method))?;

                    let mut http_headers = reqwest::header::HeaderMap::new();
                    for (key, value) in fetch_options.headers {
                        let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                            .map_err(|_| anyhow!("fetch: invalid header name {}", key))?;
                        let header_value = reqwest::header::HeaderValue::from_str(&value)
                            .map_err(|_| anyhow!("fetch: invalid header value for {}", key))?;
                        http_headers.insert(header_name, header_value);
                    }
                    
                    let client = reqwest::blocking::Client::new();
                    let mut request_builder = client.request(http_method, &fetch_options.url).headers(http_headers);
                    
                    if let Some(body_str) = fetch_options.body {
                        request_builder = request_builder.body(body_str);
                    }

                    match request_builder.send() {
                        Ok(response) => {
                            let status_code = response.status().as_u16();
                            let mut response_headers_map = HashMap::new();
                            for (name, value) in response.headers().iter() {
                                response_headers_map.insert(name.to_string(), value.to_str().unwrap_or("").to_string());
                            }
                            
                            let response_body_text = response.text()
                                .map_err(|e| anyhow!("fetch: failed to read response body as text: {}", e))?;

                            let wasm_response = WasmFetchResponse {
                                status: status_code,
                                headers: response_headers_map,
                                body: response_body_text,
                            };

                            let response_json_bytes = serde_json::to_vec(&wasm_response)
                                .map_err(|e| anyhow!("fetch: failed to serialize response to JSON: {}", e))?;

                            if response_json_bytes.len() > resp_buf_max_len as usize {
                                return Ok(-(response_json_bytes.len() as i32)); 
                            }

                            let memory_data_mut = memory.data_mut(&mut caller);
                            let response_target_slice = memory_data_mut
                                .get_mut(resp_buf_ptr as usize..(resp_buf_ptr as usize + response_json_bytes.len()))
                                .ok_or_else(|| anyhow!("fetch: response buffer pointer/length out of bounds for writing"))?;
                            
                            response_target_slice.copy_from_slice(&response_json_bytes);
                            Ok(response_json_bytes.len() as i32)
                        }
                        Err(e) => Err(anyhow!("fetch: HTTP request execution failed: {}", e)),
                    }
                }
            )?;

            let module = Module::from_binary(&engine, &downloaded_code)?;
            
            let instance = linker.instantiate(&mut store, &module)?;
            
            if let Some(wasmtime::Extern::Memory(mem)) = instance.get_export(&mut store, "memory") {
                store.data_mut().memory = Some(mem);
            } else {
                return Err(AppError::Internal("WASM module does not export 'memory'".to_string()));
            }
            
            if let Ok(start_func) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
                start_func.call(&mut store, ()).map_err(AppError::Wasmtime)?;
                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some("WASM module executed (_start)".to_string()),
                    error: None,
                }))
            } else {
                Ok(Json(ExecuteResponse {
                    status: "success".to_string(),
                    output: Some("WASM module instantiated (no _start called or found)".to_string()),
                    error: None,
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
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app.into_make_service()) // Changed axum::Server::bind to axum::serve
        .await
        .unwrap();
}
