use crate::models::SandboxApp;
use askama::Template;

/// 基础页面模板
#[derive(Template)]
#[template(path = "base.html")]
pub struct BaseTemplate {
    pub title: String,
}

/// 首页模板
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub apps: Vec<SandboxApp>,
    pub title: String,
}

/// 创建应用页面模板
#[derive(Template)]
#[template(path = "create.html")]
pub struct CreateTemplate {
    pub title: String,
}

/// 应用详情页面模板
#[derive(Template)]
#[template(path = "app_detail.html")]
pub struct AppDetailTemplate {
    pub app: SandboxApp,
    pub title: String,
}

/// 执行页面模板
#[derive(Template)]
#[template(path = "execute.html")]
pub struct ExecuteTemplate {
    pub app: SandboxApp,
    pub title: String,
}

/// 工具函数：生成应用ID
pub fn generate_app_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("app_{}", timestamp)
}
