use crate::models::{AppType, ExecuteSandboxRequest, ExecuteSandboxResponse, MockData, SandboxApp};
use crate::storage::AppStorage;
use crate::templates::{
    generate_app_id, AppDetailTemplate, CreateTemplate, ExecuteTemplate, IndexTemplate,
};
use askama::Template;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use base64::Engine;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// 将模板渲染为HTML响应
fn render_template<T: Template>(template: T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template render error: {}", err),
        )
            .into_response(),
    }
}

/// 首页 - 显示应用列表
pub async fn index_handler(State(storage): State<Arc<AppStorage>>) -> impl IntoResponse {
    info!("Accessing homepage");
    let apps = match storage.list_apps() {
        Ok(apps) => {
            info!("Successfully retrieved {} apps", apps.len());
            apps
        }
        Err(err) => {
            error!("Failed to list apps: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list apps: {}", err),
            )
                .into_response();
        }
    };

    let template = IndexTemplate {
        apps,
        title: "Hoya - JavaScript/WASM 沙箱执行平台".to_string(),
        content: String::new(),
    };
    render_template(template)
}

/// 创建应用页面
pub async fn create_page_handler() -> impl IntoResponse {
    let template = CreateTemplate {
        title: "创建应用 - Hoya".to_string(),
        content: String::new(),
    };
    render_template(template)
}

/// 创建应用提交处理
pub async fn create_submit_handler(
    State(storage): State<Arc<AppStorage>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    info!("Processing app creation submission");
    let mut name = String::new();
    let mut description = String::new();
    let mut app_type = None;
    let mut js_code = String::new();
    let mut js_params = String::new();
    let mut wasm_params = String::new();
    let mut wasm_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name_str = field.name().unwrap().to_string();

        match name_str.as_str() {
            "name" => name = field.text().await.unwrap(),
            "description" => description = field.text().await.unwrap(),
            "app_type" => {
                let type_str = field.text().await.unwrap();
                app_type = Some(match type_str.as_str() {
                    "JavaScript" => AppType::JavaScript,
                    "WebAssembly" => AppType::WebAssembly,
                    _ => {
                        warn!("Invalid app type received: {}", type_str);
                        return (StatusCode::BAD_REQUEST, "Invalid app type").into_response();
                    }
                });
            }
            "js_code" => js_code = field.text().await.unwrap(),
            "js_params" => js_params = field.text().await.unwrap(),
            "wasm_params" => wasm_params = field.text().await.unwrap(),
            "wasm_file" => {
                if let Ok(bytes) = field.bytes().await {
                    wasm_bytes = Some(bytes.to_vec());
                }
            }
            _ => {
                debug!("Unknown form field received: {}", name_str);
            }
        }
    }

    let app_type = match app_type {
        Some(t) => t,
        None => {
            warn!("App type is missing in app creation");
            return (StatusCode::BAD_REQUEST, "App type is required").into_response();
        }
    };

    let app_id = generate_app_id();

    // 根据应用类型创建代码URL和参数
    let (code_url, mock_data) = match app_type {
        AppType::JavaScript => {
            if js_code.is_empty() {
                warn!("JavaScript code is missing for JS app creation");
                return (StatusCode::BAD_REQUEST, "JavaScript code is required").into_response();
            }

            // 解析mock数据
            let mock_data = if js_params.is_empty() {
                None
            } else {
                match serde_json::from_str::<MockData>(&js_params) {
                    Ok(data) => Some(data),
                    Err(e) => {
                        warn!("Invalid JS params format: {}", e);
                        return (
                            StatusCode::BAD_REQUEST,
                            format!("Invalid JS params format: {}", e),
                        )
                            .into_response();
                    }
                }
            };

            (
                format!(
                    "data:application/javascript;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&js_code)
                ),
                mock_data,
            )
        }
        AppType::WebAssembly => {
            if wasm_bytes.is_none() {
                warn!("WebAssembly file is missing for WASM app creation");
                return (StatusCode::BAD_REQUEST, "WASM file is required").into_response();
            }

            // 解析mock数据
            let mock_data = if wasm_params.is_empty() {
                None
            } else {
                match serde_json::from_str::<MockData>(&wasm_params) {
                    Ok(data) => Some(data),
                    Err(e) => {
                        warn!("Invalid WASM params format: {}", e);
                        return (
                            StatusCode::BAD_REQUEST,
                            format!("Invalid WASM params format: {}", e),
                        )
                            .into_response();
                    }
                }
            };

            (
                format!(
                    "data:application/wasm;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(wasm_bytes.unwrap())
                ),
                mock_data,
            )
        }
    };

    let app = SandboxApp::new(
        app_id.clone(),
        name,
        description,
        app_type,
        code_url,
        mock_data,
    );
    let app_name = app.name.clone();
    info!("Creating new app: {} (type: {:?})", app_name, app.app_type);

    match storage.create_app(app) {
        Ok(_) => {
            info!("App created successfully: {} (id: {})", app_name, app_id);
            axum::response::Redirect::to(&format!("/app/{}", app_id)).into_response()
        }
        Err(err) => {
            error!("Failed to save app {}: {}", app_name, err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create app: {}", err),
            )
                .into_response()
        }
    }
}

/// 应用详情页面
pub async fn app_detail_handler(
    State(storage): State<Arc<AppStorage>>,
    Path(app_id): Path<String>,
) -> impl IntoResponse {
    info!("Accessing app detail page for id: {}", app_id);
    let app = match storage.get_app(&app_id) {
        Ok(Some(app)) => {
            info!("Found app: {} (type: {:?})", app.name, app.app_type);
            app
        }
        Ok(None) => {
            warn!("App not found: {}", app_id);
            return (StatusCode::NOT_FOUND, "App not found").into_response();
        }
        Err(err) => {
            error!("Failed to get app {}: {}", app_id, err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get app: {}", err),
            )
                .into_response();
        }
    };

    let template = AppDetailTemplate {
        app: app.clone(),
        title: format!("{} - Hoya", app.name),
        content: String::new(),
    };
    render_template(template)
}

/// 执行页面
pub async fn execute_page_handler(
    State(storage): State<Arc<AppStorage>>,
    Path(app_id): Path<String>,
) -> impl IntoResponse {
    info!("Accessing execute page for app id: {}", app_id);
    let app = match storage.get_app(&app_id) {
        Ok(Some(app)) => {
            info!(
                "Found app for execution: {} (type: {:?})",
                app.name, app.app_type
            );
            app
        }
        Ok(None) => {
            warn!("App not found for execution: {}", app_id);
            return (StatusCode::NOT_FOUND, "App not found").into_response();
        }
        Err(err) => {
            error!("Failed to get app {} for execution: {}", app_id, err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get app: {}", err),
            )
                .into_response();
        }
    };

    let template = ExecuteTemplate {
        app: app.clone(),
        title: format!("执行应用 - {} - Hoya", app.name),
        content: String::new(),
    };
    render_template(template)
}

/// 执行沙箱应用
pub async fn execute_sandbox_handler(
    State(storage): State<Arc<AppStorage>>,
    Path(app_id): Path<String>,
    Json(request): Json<ExecuteSandboxRequest>,
) -> impl IntoResponse {
    info!("Executing sandbox app: {}", app_id);
    debug!(
        "Input parameters: {}",
        serde_json::to_string_pretty(&request).unwrap_or_default()
    );

    let app = match storage.get_app(&app_id) {
        Ok(Some(app)) => {
            info!(
                "Found app for sandbox execution: {} (type: {:?})",
                app.name, app.app_type
            );
            app
        }
        Ok(None) => {
            warn!("App not found for sandbox execution: {}", app_id);
            return (StatusCode::NOT_FOUND, "App not found").into_response();
        }
        Err(err) => {
            error!(
                "Failed to get app {} for sandbox execution: {}",
                app_id, err
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get app: {}", err),
            )
                .into_response();
        }
    };

    // 验证输入参数格式
    if let Err(validation_error) = validate_inputs(&request.inputs, &app) {
        return Json(ExecuteSandboxResponse {
            success: false,
            output: None,
            error: Some(validation_error),
            execution_time_ms: 0,
        })
        .into_response();
    }

    // 执行应用
    let start_time = std::time::Instant::now();
    let result = match execute_app(&app, &request.inputs).await {
        Ok(output) => ExecuteSandboxResponse {
            success: true,
            output: Some(output),
            error: None,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
        },
        Err(err) => ExecuteSandboxResponse {
            success: false,
            output: None,
            error: Some(err),
            execution_time_ms: start_time.elapsed().as_millis() as u64,
        },
    };

    Json(result).into_response()
}

/// 验证输入参数格式
fn validate_inputs(
    inputs: &HashMap<String, serde_json::Value>,
    app: &SandboxApp,
) -> Result<(), String> {
    // 如果有 mock_data，验证输入是否符合预期格式
    if let Some(ref mock_data) = app.mock_data {
        for (key, expected_value) in &mock_data.inputs {
            if let Some(input_value) = inputs.get(key) {
                // 基本类型验证
                if !validate_value_type(input_value, expected_value) {
                    return Err(format!(
                        "参数 '{}' 类型不匹配: 期望 {:?}, 实际 {:?}",
                        key, expected_value, input_value
                    ));
                }
            } else if !is_optional_field(key, &app.app_type) {
                return Err(format!("缺少必需参数: '{}'", key));
            }
        }
    }

    // 验证参数值范围和格式
    for (key, value) in inputs {
        if let Err(e) = validate_input_value(key, value, &app.app_type) {
            return Err(e);
        }
    }

    Ok(())
}

/// 验证值类型是否匹配
fn validate_value_type(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::Number(_), serde_json::Value::Number(_)) => true,
        (serde_json::Value::String(_), serde_json::Value::String(_)) => true,
        (serde_json::Value::Bool(_), serde_json::Value::Bool(_)) => true,
        (serde_json::Value::Array(_), serde_json::Value::Array(_)) => true,
        (serde_json::Value::Object(_), serde_json::Value::Object(_)) => true,
        (serde_json::Value::Null, serde_json::Value::Null) => true,
        _ => false,
    }
}

/// 检查字段是否为可选字段
fn is_optional_field(field_name: &str, app_type: &AppType) -> bool {
    // 根据应用类型定义可选字段
    match app_type {
        AppType::JavaScript => {
            matches!(field_name, "debug" | "verbose" | "timeout")
        }
        AppType::WebAssembly => {
            matches!(field_name, "memory_pages" | "debug" | "timeout")
        }
    }
}

/// 验证输入值的有效性
fn validate_input_value(
    key: &str,
    value: &serde_json::Value,
    app_type: &AppType,
) -> Result<(), String> {
    match app_type {
        AppType::JavaScript => validate_js_input(key, value),
        AppType::WebAssembly => validate_wasm_input(key, value),
    }
}

/// 验证 JavaScript 输入参数
fn validate_js_input(key: &str, value: &serde_json::Value) -> Result<(), String> {
    match key {
        "timeout" => {
            if let Some(num) = value.as_u64() {
                if num > 30000 {
                    // 最大30秒
                    return Err("超时时间不能超过30秒".to_string());
                }
            } else {
                return Err("超时时间必须是正整数".to_string());
            }
        }
        "debug" => {
            if !value.is_boolean() {
                return Err("debug 参数必须是布尔值".to_string());
            }
        }
        "verbose" => {
            if !value.is_boolean() {
                return Err("verbose 参数必须是布尔值".to_string());
            }
        }
        _ => {
            // 自定义参数验证
            if key.starts_with("_") {
                return Err(format!("参数名不能以 '_' 开头: '{}'", key));
            }
            if key.len() > 50 {
                return Err(format!("参数名过长 (最大50字符): '{}'", key));
            }
        }
    }
    Ok(())
}

/// 验证 WebAssembly 输入参数
fn validate_wasm_input(key: &str, value: &serde_json::Value) -> Result<(), String> {
    match key {
        "memory_pages" => {
            if let Some(num) = value.as_u64() {
                if num == 0 || num > 65536 {
                    // 1页=64KB，最大4GB
                    return Err("内存页数必须在1-65536之间".to_string());
                }
            } else {
                return Err("内存页数必须是正整数".to_string());
            }
        }
        "timeout" => {
            if let Some(num) = value.as_u64() {
                if num > 30000 {
                    // 最大30秒
                    return Err("超时时间不能超过30秒".to_string());
                }
            } else {
                return Err("超时时间必须是正整数".to_string());
            }
        }
        "debug" => {
            if !value.is_boolean() {
                return Err("debug 参数必须是布尔值".to_string());
            }
        }
        _ => {
            // 自定义参数验证
            if key.starts_with("_") {
                return Err(format!("参数名不能以 '_' 开头: '{}'", key));
            }
            if key.len() > 50 {
                return Err(format!("参数名过长 (最大50字符): '{}'", key));
            }
        }
    }
    Ok(())
}

/// 执行应用
async fn execute_app(
    app: &SandboxApp,
    inputs: &HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    info!(
        "Starting execution of app: {} (type: {:?})",
        app.name, app.app_type
    );
    debug!(
        "Input parameters: {}",
        serde_json::to_string_pretty(inputs).unwrap_or_default()
    );

    // 设置默认参数
    let mut execution_inputs = inputs.clone();
    set_default_params(&mut execution_inputs, &app.app_type);
    debug!(
        "Parameters after setting defaults: {}",
        serde_json::to_string_pretty(&execution_inputs).unwrap_or_default()
    );

    let start_time = std::time::Instant::now();
    info!(
        "Starting {} execution for app: {}",
        match app.app_type {
            AppType::JavaScript => "JavaScript",
            AppType::WebAssembly => "WebAssembly",
        },
        app.name
    );

    // Both UI and API paths delegate to the same engines. In particular, do
    // not maintain a second, less-restricted Wasmtime configuration here.
    let result = match app.app_type {
        AppType::JavaScript => execute_javascript(app, &execution_inputs).await,
        AppType::WebAssembly => execute_wasm(app, &execution_inputs).await,
    };

    let execution_time = start_time.elapsed().as_millis();

    match &result {
        Ok(_) => {
            info!(
                "Successfully executed app: {} in {}ms",
                app.name, execution_time
            );
        }
        Err(error) => {
            error!(
                "Failed to execute app: {} after {}ms - Error: {}",
                app.name, execution_time, error
            );
        }
    }

    result
}

/// 设置默认参数
fn set_default_params(inputs: &mut HashMap<String, serde_json::Value>, app_type: &AppType) {
    match app_type {
        AppType::JavaScript => {
            inputs.entry("timeout".to_string()).or_insert(json!(10000)); // 默认10秒
            inputs.entry("debug".to_string()).or_insert(json!(false));
            inputs.entry("verbose".to_string()).or_insert(json!(false));
        }
        AppType::WebAssembly => {
            inputs
                .entry("memory_pages".to_string())
                .or_insert(json!(256)); // 默认16MB
            inputs.entry("timeout".to_string()).or_insert(json!(10000)); // 默认10秒
            inputs.entry("debug".to_string()).or_insert(json!(false));
        }
    }
}

/// 执行 JavaScript 代码
async fn execute_javascript(
    app: &SandboxApp,
    inputs: &HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    info!("Starting JavaScript execution for app: {}", app.name);
    debug!("Input parameters: {:?}", inputs);

    // 提取执行参数
    let timeout = inputs
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(10000) as u32;

    let debug = inputs
        .get("debug")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 准备用户参数
    let mut user_inputs = inputs.clone();
    user_inputs.remove("timeout");
    user_inputs.remove("debug");
    user_inputs.remove("verbose");

    // 从data URL中提取JavaScript代码
    let js_code = if app
        .code_url
        .starts_with("data:application/javascript;base64,")
    {
        let base64_data = &app.code_url["data:application/javascript;base64,".len()..];
        match base64::engine::general_purpose::STANDARD.decode(base64_data) {
            Ok(bytes) => {
                let code = String::from_utf8(bytes)
                    .map_err(|e| format!("JavaScript代码解码失败: {}", e))?;
                debug!("Decoded JavaScript code ({} bytes)", code.len());
                code
            }
            Err(e) => {
                error!("Base64解码失败: {}", e);
                return Err(format!("Base64解码失败: {}", e));
            }
        }
    } else {
        return Err(format!(
            "Invalid JavaScript code URL format for app: {}",
            app.name
        ));
    };

    // Guest execution is synchronous; keep it off Tokio's I/O workers just
    // like the API routes and the common WASM path.
    let js_code_bytes = bytes::Bytes::from(js_code);
    match crate::execution::run_blocking(move || crate::js_engine::execute_js(js_code_bytes)).await
    {
        Ok(Json(response)) => {
            let execution_result = json!({
                "status": "success",
                "message": "JavaScript executed successfully",
                "inputs": user_inputs,
                "outputs": {
                    "result": response.output,
                    "logs": response.stdout,
                    "execution_time": response.metadata.execution_time,
                },
                "metadata": {
                    "engine": "QuickJS",
                    "timeout": timeout,
                    "debug": debug,
                    "timestamp": response.metadata.timestamp,
                }
            });
            Ok(execution_result)
        }
        Err(err) => {
            error!("JavaScript execution failed: {:?}", err);
            Err(format!("JavaScript执行错误: {:?}", err))
        }
    }
}

/// Execute WebAssembly through the same bounded Wasmtime path used by
/// `/execute/wasm`. The old UI-only Wasmtime setup had no fuel, usable wall
/// clock deadline, or StoreLimiter; keeping one execution implementation
/// prevents those controls from silently drifting apart again.
async fn execute_wasm(
    app: &SandboxApp,
    inputs: &HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    info!(
        "Starting resource-bounded WebAssembly execution for app: {}",
        app.name
    );
    let mut user_inputs = inputs.clone();
    user_inputs.remove("memory_pages");
    user_inputs.remove("timeout");
    user_inputs.remove("debug");

    // Decode the UI's stored data URL, then use the common Hosta ABI executor.
    let wasm_bytes = if app.code_url.starts_with("data:application/wasm;base64,") {
        let base64_data = &app.code_url["data:application/wasm;base64,".len()..];
        base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| {
                error!("Base64解码失败: {}", e);
                format!("Base64解码失败: {}", e)
            })?
    } else {
        return Err(format!(
            "Invalid WebAssembly code URL format for app: {}",
            app.name
        ));
    };
    let input_json = serde_json::to_string(&user_inputs)
        .map_err(|error| format!("failed to encode WASM input: {error}"))?;
    let Json(response) = crate::execution::run_blocking(move || {
        crate::wasm_engine::execute_wasm_with_input(
            bytes::Bytes::from(wasm_bytes),
            Some(input_json),
            None,
        )
    })
    .await
    .map_err(|error| format!("WASM execution error: {error}"))?;

    if response.status != "success" {
        return Err(response
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| "WASM execution failed".to_string()));
    }

    Ok(json!({
        "status": "success",
        "message": "WebAssembly executed successfully",
        "inputs": user_inputs,
        "outputs": {
            "result": response.output,
            "stdout": response.stdout,
            "stderr": response.stderr,
            "execution_time": response.metadata.execution_time,
        },
        "metadata": {
            "engine": "Wasmtime",
            "timestamp": response.metadata.timestamp,
        }
    }))
}
