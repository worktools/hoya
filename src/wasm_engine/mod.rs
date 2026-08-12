mod ffis;

use crate::error::{AppError, ExecuteResponse, ExecutionMetadata};
use crate::wasm_engine::ffis as wasm_ffis; // Adjusted import path
use axum::Json;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Config, Engine, Linker, Memory, Module, Store};

/// Context for Wasm store to hold shared resources like the HTTP client
///
/// This struct provides access to shared resources for WebAssembly modules.
/// It includes a reqwest HTTP client and optional memory reference.
pub struct WasmCtx {
    /// HTTP client for making network requests
    pub reqwest_client: reqwest::Client,
    /// Optional reference to the WebAssembly module's memory
    pub memory: Option<Memory>,
    /// Captured stdout content
    pub stdout: Arc<Mutex<String>>,
    /// Captured stderr content
    pub stderr: Arc<Mutex<String>>,
    /// Optional input JSON string (written by host, read by get_input FFI)
    pub input_json: Option<String>,
    /// Optional datasource JSON string (written by host, read by get_datasource FFI)
    pub datasource_json: Option<String>,
}

/// Default fuel limit for WASM execution (100,000 operations)
const DEFAULT_FUEL_LIMIT: u64 = 100_000;
/// Build a wasmtime `Engine` with resource limits configured.
fn build_engine_with_limits() -> Result<Engine, AppError> {
    let mut config = Config::new();
    // Enable fuel consumption for instruction-level metering
    config.consume_fuel(true);
    // Limit WASM stack and CPU via fuel metering
    config.max_wasm_stack(500_000);
    // Note: wasmtime 33.0.0 doesn't have a direct wasm_memory_limits API.
    // Memory limits are enforced at instantiation time via the module's memory type.
    Engine::new(&config).map_err(|e| AppError::Internal(format!("Failed to create engine: {}", e)))
}

/// Execute WebAssembly code and return the execution result
///
/// # Arguments
///
/// * `wasm_code` - WebAssembly code to execute as a byte array
///
/// # Returns
///
/// * `Result<Json<ExecuteResponse>, AppError>` - Execution result or error
pub fn execute_wasm(downloaded_code: bytes::Bytes) -> Result<Json<ExecuteResponse>, AppError> {
    execute_wasm_with_input(downloaded_code, None, None)
}

/// Execute WebAssembly code with optional input/datasource JSON and resource limits.
///
/// Resource limits include:
/// - Fuel: instruction-level metering (stops execution after ~100k ops)
/// - Memory: enforced via module's memory type at instantiation time
/// - Stack: 500KB max WASM stack
pub fn execute_wasm_with_input(
    downloaded_code: bytes::Bytes,
    input_json: Option<String>,
    datasource_json: Option<String>,
) -> Result<Json<ExecuteResponse>, AppError> {
    println!(
        "Code type: WebAssembly, size: {} bytes, input: {}",
        downloaded_code.len(),
        if input_json.is_some() { "provided" } else { "none" }
    );

    let start_time = std::time::Instant::now();
    let resource_size = downloaded_code.len();

    let engine = build_engine_with_limits()?;
    let wasm_shared_data = WasmCtx {
        reqwest_client: reqwest::Client::new(),
        memory: None,
        stdout: Arc::new(Mutex::new(String::new())),
        stderr: Arc::new(Mutex::new(String::new())),
        input_json: input_json.clone(),
        datasource_json: datasource_json.clone(),
    };
    let mut store = Store::new(&engine, wasm_shared_data);

    // Inject fuel into the store — execution stops when fuel runs out
    store
        .set_fuel(DEFAULT_FUEL_LIMIT)
        .map_err(|e| AppError::Internal(format!("Failed to set fuel: {}", e)))?;

    let mut linker = Linker::new(&engine);

    // Call the function from wasm_ffis to register linker functions
    wasm_ffis::register_linker_functions(&mut linker)
        .map_err(|e| AppError::Internal(format!("Failed to register linker functions: {}", e)))?;

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
        resource_size,
    };

    // Execute _start function, handling fuel exhaustion and other errors
    let exec_result = (|| -> Result<Json<ExecuteResponse>, AppError> {
        let metadata = metadata.clone();
        if let Ok(start_func) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
            start_func.call(&mut store, ())
                .map_err(|e| AppError::Wasmtime(anyhow::Error::from(e)))?;

            let total_execution_time = start_time.elapsed().as_millis() as u64;
            let updated_metadata = ExecutionMetadata {
                execution_time: total_execution_time,
                ..metadata
            };

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
                output: Some("WASM module instantiated (no _start called or found)".to_string()),
                stdout: Some(stdout),
                stderr: Some(stderr),
                error: None,
                metadata,
            }))
        }
    })();

    match exec_result {
        Ok(response) => Ok(response),
        Err(err) => {
            let err_msg = format!("{}", err);
            let total_execution_time = start_time.elapsed().as_millis() as u64;
            let is_fuel = err_msg.contains("fuel") || err_msg.contains("all fuel consumed");

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
                status: "failed".to_string(),
                output: Some(if is_fuel {
                    "WASM execution exceeded fuel limit".to_string()
                } else {
                    format!("WASM execution error: {}", err_msg)
                }),
                stdout: Some(stdout),
                stderr: Some(stderr),
                error: Some(crate::error::ErrorInfo {
                    code: if is_fuel { "WASM_FUEL_EXHAUSTED".to_string() } else { "WASM_EXECUTION_ERROR".to_string() },
                    message: err_msg,
                    details: None,
                }),
                metadata: ExecutionMetadata {
                    execution_time: total_execution_time,
                    ..metadata
                },
            }))
        }
    }
}
