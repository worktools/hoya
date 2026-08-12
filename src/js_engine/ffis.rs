use rquickjs::{Ctx, Function, Object, Result as QuickJsResult, Value};
use std::sync::{Arc, Mutex};

/// Output buffers for capturing stdout and stderr
pub struct OutputBuffers {
    pub stdout: Arc<Mutex<String>>,
    pub stderr: Arc<Mutex<String>>,
}

/// Hostnames that must never be reachable from sandboxed code (basic SSRF guard,
/// mirrors the same block-list used by Hosta's WASM `fetch` bridge).
fn is_blocked_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

/// Perform a GET request on behalf of sandboxed JS code and return a JSON
/// envelope string: `{"ok":true,"data":"..."}` or `{"ok":false,"error":{...}}`.
/// Mirrors the WASM engine's fetch contract so both engines behave the same way.
fn fetch_url(url: String) -> String {
    let parsed = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(_) => {
            return serde_json::json!({
                "ok": false,
                "error": { "code": "INVALID_URL", "message": "invalid URL" }
            })
            .to_string();
        }
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return serde_json::json!({
            "ok": false,
            "error": { "code": "INVALID_URL", "message": "only http/https URLs allowed" }
        })
        .to_string();
    }
    if is_blocked_host(parsed.host_str().unwrap_or_default()) {
        return serde_json::json!({
            "ok": false,
            "error": { "code": "BLOCKED_HOST", "message": "cannot access localhost" }
        })
        .to_string();
    }

    let result: Result<String, String> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let resp = reqwest::Client::new()
                .get(url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            resp.text().await.map_err(|e| e.to_string())
        })
    });

    match result {
        Ok(text) if text.len() > 524_288 => serde_json::json!({
            "ok": false,
            "error": { "code": "RESPONSE_TOO_LARGE", "message": "response exceeds 512KB limit" }
        })
        .to_string(),
        Ok(text) => serde_json::json!({ "ok": true, "data": text }).to_string(),
        Err(message) => serde_json::json!({
            "ok": false,
            "error": { "code": "FETCH_ERROR", "message": message }
        })
        .to_string(),
    }
}

/// Register JavaScript functions directly to the global object with output capturing
///
/// This approach attaches functions directly to the global object and
/// captures console.log and console.error output.
pub fn register_to_globals_with_capture<'js>(
    ctx: &Ctx<'js>,
    output_buffers: OutputBuffers,
) -> QuickJsResult<()> {
    // Get the global object
    let globals = ctx.globals();

    // Capture stdout buffer for console.log
    let stdout = output_buffers.stdout.clone();
    let console_log_fn: Value = ctx.eval(
        r#"(function captureStdout(...args) {
            const message = args.map(arg =>
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' ');
            __internal_capture_stdout(message);
        })"#
    )?;

    // Capture stderr buffer for console.error
    let stderr = output_buffers.stderr.clone();
    let console_error_fn: Value = ctx.eval(
        r#"(function captureStderr(...args) {
            const message = args.map(arg =>
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' ');
            __internal_capture_stderr(message);
        })"#
    )?;

    // Create console object if it doesn't exist
    let console_exists: bool = ctx.eval("typeof console !== 'undefined'")?;
    if !console_exists {
        ctx.eval::<(), _>("var console = {};")?;
    }

    // Set the console.log and console.error functions
    let console: Object = ctx.eval("console")?;
    console.set("log", console_log_fn)?;
    console.set("error", console_error_fn)?;

    // Register internal capture functions
    let stdout_clone = stdout.clone();
    globals.set(
        "__internal_capture_stdout",
        Function::new(ctx.clone(), move |message: String| -> QuickJsResult<()> {
            println!("{}", &message); // Also print to host stdout for debugging
            if let Ok(mut buffer) = stdout_clone.lock() {
                buffer.push_str(&message);
                buffer.push('\n');
            }
            Ok(())
        })?,
    )?;

    let stderr_clone = stderr.clone();
    globals.set(
        "__internal_capture_stderr",
        Function::new(ctx.clone(), move |message: String| -> QuickJsResult<()> {
            eprintln!("{}", &message); // Also print to host stderr for debugging
            if let Ok(mut buffer) = stderr_clone.lock() {
                buffer.push_str(&message);
                buffer.push('\n');
            }
            Ok(())
        })?,
    )?;

    // Create app_log function
    let app_log_fn: Value = ctx.eval(
        r#"(function appLog(level, message) {
            console.log("[JS LOG - " + (level || 'INFO').toUpperCase() + "]: " + (message || ''));
        })"#
    )?;
    globals.set("app_log", app_log_fn)?;

    // Create get_unixtime function
    let get_unixtime_fn: Value = ctx.eval(
        r#"(function getUnixTime() {
            return Date.now() / 1000;
        })"#
    )?;
    globals.set("get_unixtime", get_unixtime_fn)?;

    // Create fetch function: performs a real (GET-only) HTTP request and
    // returns a JSON envelope string `{ok,data}`/`{ok:false,error}`, matching
    // the WASM engine's fetch contract. Note this call blocks the calling
    // thread until the request completes or times out (5s) — same tradeoff
    // already accepted by the WASM `fetch` FFI in this codebase.
    let fetch_fn = Function::new(ctx.clone(), fetch_url)?;
    globals.set("fetch", fetch_fn)?;

    Ok(())
}
