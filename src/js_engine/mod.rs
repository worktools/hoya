mod ffis;

use crate::error::{AppError, ExecuteResponse, ExecutionMetadata};
use axum::Json;
use ffis as js_ffis; // Adjusted import path
use rquickjs::{Ctx, Context, Result as QuickJsResult, Runtime, Value};
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
    execute_js_with_input(downloaded_code, None, None)
}

/// Execute JavaScript code with optional input JSON and datasource.
///
/// This implements the Hosta `main(input, ctx)` calling convention:
/// 1. Evaluates the JS code to define `main` function
/// 2. Calls `main(input, ctx)` with the provided input and context
/// 3. Captures stdout/stderr and returns the result
///
/// The context object `ctx` provides:
/// - `ctx.log(level, message, fields?)` — structured logging
/// - `ctx.now()` — current timestamp (ms)
/// - `ctx.datasource` — datasource data (if provided)
pub fn execute_js_with_input(
    downloaded_code: bytes::Bytes,
    input_json: Option<serde_json::Value>,
    datasource_json: Option<serde_json::Value>,
) -> Result<Json<ExecuteResponse>, AppError> {
    println!(
        "Code type: JavaScript, size: {} bytes, input: {}",
        downloaded_code.len(),
        if input_json.is_some() { "provided" } else { "none" }
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

    // Execute JavaScript with output capturing
    let result = context.with(|ctx| -> QuickJsResult<String> {
        // Register JavaScript functions with stdout/stderr capture
        let output_buffers = js_ffis::OutputBuffers {
            stdout: stdout_buffer.clone(),
            stderr: stderr_buffer.clone(),
        };
        js_ffis::register_to_globals_with_capture(&ctx, output_buffers)?;

        // Evaluate the JS code to define functions (including main)
        ctx.eval::<Value, _>(js_code.as_str())?;

        // Check if main function exists
        let has_main: bool = ctx.eval("typeof main === 'function'")?;
        if !has_main {
            // If no main function, return the eval result (simple script mode)
            let result = ctx.eval::<Value, _>("undefined")?;
            let output = value_to_string(&result, &ctx)?;
            return Ok(output);
        }

        // Stringify the input JSON for injection
        let input_str = input_json
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());

        let datasource_str = datasource_json
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());

        // Build the context object for `main(input, ctx)`
        // We inject ctx as a JavaScript object with log, now, and datasource
        let ctx_setup = format!(
            r#"
(function() {{
    const logs = [];
    const ctx = {{
        datasource: {datasource},
        log: function(level, message, fields) {{
            if (logs.length < 100) {{
                const entry = {{
                    level: ['debug','info','warn','error'].includes(level) ? level : 'info',
                    message: String(message).slice(0, 2000),
                    fields: fields || null,
                    at: new Date().toISOString()
                }};
                logs.push(entry);
                __internal_capture_stdout('[' + entry.level.toUpperCase() + '] ' + entry.message);
            }}
        }},
        now: function() {{ return Date.now(); }}
    }};
    // Store logs for retrieval
    globalThis.__hosta_logs = logs;
    globalThis.__hosta_ctx = ctx;
    // Parse input
    var input = JSON.parse(`{input_str}`);
    // Call main
    var result = main(input, ctx);
    // If result is a Promise, we need to handle it
    if (result && typeof result.then === 'function') {{
        // rquickjs doesn't support async/await directly in eval
        // We'll use a synchronous approach: try to get the resolved value
        // For simplicity, we'll stringify the promise
        return JSON.stringify({{ __async: true, message: 'Async result not supported in synchronous mode' }});
    }}
    return JSON.stringify({{ __result: result }});
}})()
"#,
            datasource = datasource_str,
            input_str = input_str
        );

        // Execute the wrapped call
        let result_value = ctx.eval::<Value, _>(ctx_setup.as_str())?;
        let result_str = value_to_string(&result_value, &ctx)?;

        Ok(result_str)
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

/// Helper to convert a rquickjs Value to a String
fn value_to_string<'js>(result: &Value<'js>, _ctx: &Ctx<'js>) -> QuickJsResult<String> {
    let output = match result.type_of() {
        rquickjs::Type::String => result.as_string().unwrap().to_string()?,
        rquickjs::Type::Int => result.as_int().unwrap().to_string(),
        rquickjs::Type::Bool => result.as_bool().unwrap().to_string(),
        rquickjs::Type::Float => result.as_float().unwrap().to_string(),
        rquickjs::Type::Null => "null".to_string(),
        rquickjs::Type::Undefined => "undefined".to_string(),
        rquickjs::Type::Object => {
            // For objects, we return the JSON string representation
            // via JSON.stringify. Since we can't easily call JS functions with
            // a Value arg from Rust, we return the type name.
            "[object Object]".to_string()
        }
        _ => format!(
            "Execution resulted in a non-primitive type: {:?}",
            result.type_of()
        ),
    };
    Ok(output)
}
