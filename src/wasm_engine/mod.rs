mod ffis;

use crate::error::{AppError, ExecuteResponse, ExecutionMetadata};
use crate::wasm_engine::ffis as wasm_ffis; // Adjusted import path
use axum::Json;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Engine, Linker, Memory, Module, Store};

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
            output: Some("WASM module instantiated (no _start called or found)".to_string()),
            stdout: Some(stdout),
            stderr: Some(stderr),
            error: None,
            metadata,
        }))
    }
}
