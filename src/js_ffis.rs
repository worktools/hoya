use rquickjs::{Ctx, Result as QuickJsResult, Value};

/// Register JavaScript functions in the QuickJS runtime
pub fn register_functions<'js>(ctx: &Ctx<'js>) -> QuickJsResult<()> {
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

    Ok(())
}

/// Register JavaScript functions directly to the global object
/// 
/// This alternative approach attaches functions directly to the global object
/// rather than defining them in the global scope.
pub fn register_to_globals<'js>(ctx: &Ctx<'js>) -> QuickJsResult<()> {
    // Get the global object
    let globals = ctx.globals();
    
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
        throw new Error("fetch is not fully implemented in this runtime");
    }
    "#;
    let fetch_fn: Value = ctx.eval(fetch_str)?;
    globals.set("fetch", fetch_fn)?;

    Ok(())
}
