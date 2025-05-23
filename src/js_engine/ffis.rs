use rquickjs::{Ctx, Function, Object, Result as QuickJsResult, Value};
use std::sync::{Arc, Mutex};

/// Output buffers for capturing stdout and stderr
pub struct OutputBuffers {
    pub stdout: Arc<Mutex<String>>,
    pub stderr: Arc<Mutex<String>>,
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
    let console_log_str = format!(
        r#"
        function(...args) {{
            const message = args.map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' ');
            __internal_capture_stdout(message);
        }}
        "#
    );
    let console_log_fn: Value = ctx.eval(console_log_str)?;

    // Capture stderr buffer for console.error
    let stderr = output_buffers.stderr.clone();
    let console_error_str = format!(
        r#"
        function(...args) {{
            const message = args.map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' ');
            __internal_capture_stderr(message);
        }}
        "#
    );
    let console_error_fn: Value = ctx.eval(console_error_str)?;

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
    let app_log_str = r#"
    function(level, message) {
        console.log("[JS LOG - " + (level || 'INFO').toUpperCase() + "]: " + (message || ''));
    }
    "#;
    let app_log_fn: Value = ctx.eval(app_log_str)?;
    globals.set("app_log", app_log_fn)?;

    // Create get_unixtime function
    let get_unixtime_str = r#"
    function() {
        return Date.now() / 1000;
    }
    "#;
    let get_unixtime_fn: Value = ctx.eval(get_unixtime_str)?;
    globals.set("get_unixtime", get_unixtime_fn)?;

    // Create fetch function
    let fetch_str = r#"
    function(options) {
        throw {
            code: "FETCH_NOT_IMPLEMENTED",
            message: "fetch is not fully implemented in this runtime",
            details: { requestedUrl: options && options.url }
        };
    }
    "#;
    let fetch_fn: Value = ctx.eval(fetch_str)?;
    globals.set("fetch", fetch_fn)?;

    Ok(())
}
