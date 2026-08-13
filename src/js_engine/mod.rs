mod ffis;

use crate::error::{AppError, ExecuteResponse, ExecutionMetadata};
use axum::Json;
use ffis as js_ffis; // Adjusted import path
use rquickjs::{Context, Ctx, Result as QuickJsResult, Runtime, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Wall-clock execution budget for a single JS invocation.
const JS_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);
/// Max heap the QuickJS runtime may allocate (64 MiB).
const JS_MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
/// Safety cap on microtask (Promise) drain iterations, in case a job keeps
/// re-queuing itself.
const MAX_PENDING_JOB_ITERATIONS: usize = 10_000;

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
        if input_json.is_some() {
            "provided"
        } else {
            "none"
        }
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

    // Guard against infinite loops: QuickJS polls this handler periodically
    // and aborts execution once the deadline passes.
    let deadline = Instant::now() + JS_EXECUTION_TIMEOUT;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    // Guard against unbounded allocation (e.g. `while(1) arr.push(...)`).
    runtime.set_memory_limit(JS_MEMORY_LIMIT_BYTES);
    // Run GC more eagerly under memory pressure so the limit above is less
    // likely to be hit by garbage that's already collectible.
    runtime.set_gc_threshold(JS_MEMORY_LIMIT_BYTES / 4);

    let context = Context::full(&runtime)?;

    // Create buffers for stdout and stderr
    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));

    // Execute JavaScript with output capturing
    let eval_result = context.with(|ctx| -> QuickJsResult<String> {
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
    globalThis.__hosta_settled = false;
    globalThis.__hosta_is_error = false;
    globalThis.__hosta_value = undefined;
    // Call main. A synchronous throw here propagates naturally as an uncaught
    // exception (host sees it as an execution error), matching the
    // pre-existing behavior for non-async code.
    var result = main(input, ctx);
    if (result && typeof result.then === 'function') {{
        // `main` is async — only Promises that settle synchronously (no real
        // async I/O, since fetch/timers aren't implemented) are supported.
        result.then(
            function(v) {{
                globalThis.__hosta_settled = true;
                globalThis.__hosta_value = v;
            }},
            function(e) {{
                globalThis.__hosta_settled = true;
                globalThis.__hosta_is_error = true;
                globalThis.__hosta_value = (e && e.message !== undefined) ? e.message : String(e);
            }}
        );
        return "__HOSTA_PENDING__";
    }}
    globalThis.__hosta_settled = true;
    globalThis.__hosta_value = result;
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

    // If `main` returned a Promise, drain the microtask queue so any
    // synchronously-resolvable Promise (i.e. one that doesn't depend on real
    // async I/O, which this sandbox doesn't provide) settles.
    let result = if eval_result == "__HOSTA_PENDING__" {
        let mut iterations = 0usize;
        while runtime.is_job_pending() {
            if iterations >= MAX_PENDING_JOB_ITERATIONS {
                return Err(AppError::Internal(
                    "JavaScript execution aborted: too many pending microtasks (possible Promise loop)"
                        .to_string(),
                ));
            }
            if let Err(e) = runtime.execute_pending_job() {
                return Err(AppError::Internal(format!(
                    "Unhandled exception while resolving Promise: {}",
                    e
                )));
            }
            iterations += 1;
        }

        let (settled, is_error, output) =
            context.with(|ctx| -> QuickJsResult<(bool, bool, String)> {
                let settled: bool = ctx.eval("globalThis.__hosta_settled === true")?;
                let is_error: bool = ctx.eval("globalThis.__hosta_is_error === true")?;
                let output: String = if is_error {
                    ctx.eval("String(globalThis.__hosta_value)")?
                } else {
                    ctx.eval(
                        r#"(function() {
                        try { return JSON.stringify({ __result: globalThis.__hosta_value }); }
                        catch (e) { return JSON.stringify({ __result: null }); }
                    })()"#,
                    )?
                };
                Ok((settled, is_error, output))
            })?;

        if !settled {
            return Err(AppError::Internal(
                "Promise did not resolve: this sandbox has no real async I/O (fetch/timers), so only synchronously-resolvable Promises are supported".to_string(),
            ));
        }
        if is_error {
            return Err(AppError::Internal(format!(
                "Uncaught (in promise): {}",
                output
            )));
        }
        output
    } else {
        eval_result
    };

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
