use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// 沙箱应用类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppType {
    #[serde(rename = "js")]
    JavaScript,
    #[serde(rename = "wasm")]
    WebAssembly,
}

impl fmt::Display for AppType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppType::JavaScript => write!(f, "JavaScript"),
            AppType::WebAssembly => write!(f, "WebAssembly"),
        }
    }
}

impl AppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppType::JavaScript => "js",
            AppType::WebAssembly => "wasm",
        }
    }
}

/// 沙箱应用状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
}

impl AppStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppStatus::Active => "active",
            AppStatus::Inactive => "inactive",
        }
    }
}

/// 沙箱应用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxApp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub app_type: AppType,
    pub status: AppStatus,
    pub code_url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub mock_data: Option<MockData>,
}

/// Mock数据定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockData {
    pub inputs: HashMap<String, serde_json::Value>,
    pub expected_output: Option<serde_json::Value>,
}

/// 创建应用请求
#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    pub description: String,
    pub app_type: AppType,
    pub code_content: String,
    pub mock_data: Option<MockData>,
}

/// 执行请求
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteSandboxRequest {
    pub inputs: HashMap<String, serde_json::Value>,
}

/// JavaScript 执行参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaScriptParams {
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// 是否启用调试模式
    #[serde(default)]
    pub debug: bool,
    /// 是否启用详细日志
    #[serde(default)]
    pub verbose: bool,
    /// 自定义参数
    #[serde(flatten)]
    pub custom_params: HashMap<String, serde_json::Value>,
}

/// WebAssembly 执行参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmParams {
    /// 内存页数（每页64KB）
    #[serde(default = "default_memory_pages")]
    pub memory_pages: u32,
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// 是否启用调试模式
    #[serde(default)]
    pub debug: bool,
    /// 自定义参数
    #[serde(flatten)]
    pub custom_params: HashMap<String, serde_json::Value>,
}

/// 参数验证结果
#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// 执行上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub app_id: String,
    pub app_type: AppType,
    pub inputs: HashMap<String, serde_json::Value>,
    pub metadata: ExecutionMetadata,
}

/// 执行元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub timeout: u64,
    pub memory_limit: Option<u64>,
    pub debug: bool,
}

// 默认参数值
fn default_timeout() -> u64 {
    10000
} // 10秒
fn default_memory_pages() -> u32 {
    256
} // 16MB

/// 执行响应
#[derive(Debug, Serialize)]
pub struct ExecuteSandboxResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// 列表响应
#[derive(Debug, Serialize)]
pub struct AppListResponse {
    pub apps: Vec<SandboxApp>,
    pub total: usize,
}

impl SandboxApp {
    pub fn new(
        id: String,
        name: String,
        description: String,
        app_type: AppType,
        code_url: String,
        mock_data: Option<MockData>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            name,
            description,
            app_type,
            status: AppStatus::Active,
            code_url,
            created_at: now,
            updated_at: now,
            mock_data,
        }
    }
}
