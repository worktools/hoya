mod ffis;

use crate::error::{AppError, ExecuteResponse, ExecutionMetadata};
use axum::Json;
use ffis as js_ffis; // Adjusted import path
use rquickjs::{Context, Result as QuickJsResult, Runtime, Value};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Execute JavaScript code and return the execution result
///
/// # Arguments
///
/// * `js_code` - JavaScript code to execute as a byte array
///
/// # Returns
///
/// * `Result<Json<ExecuteResponse>, AppError>` - Execution result or error
pub fn execute_js(downloaded_code: bytes::Bytes) -> Result<Json<ExecuteResponse>, AppError> {
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

    // Create buffers for stdout and stderr
    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));

    // It seems register_context_properties was intended to set up global functions and capture.
    // We will use register_to_globals_with_capture for this.
    // The actual registration will happen inside context.with() where Ctx is available.

    // Execute JavaScript with output capturing
    let result = context.with(|ctx| -> QuickJsResult<String> {
        // Register JavaScript functions with stdout/stderr capture
        let output_buffers = js_ffis::OutputBuffers {
            stdout: stdout_buffer.clone(),
            stderr: stderr_buffer.clone(),
        };
        // Corrected: Use the alias js_ffis
        js_ffis::register_to_globals_with_capture(&ctx, output_buffers)?;

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
