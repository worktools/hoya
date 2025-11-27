use crate::models::{AppStatus, SandboxApp};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 简单的内存存储，用于保存沙箱应用
#[derive(Clone)]
pub struct AppStorage {
    apps: Arc<Mutex<HashMap<String, SandboxApp>>>,
}

impl AppStorage {
    pub fn new() -> Self {
        Self {
            apps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建新应用
    pub fn create_app(&self, app: SandboxApp) -> Result<(), String> {
        let mut apps = self.apps.lock().map_err(|e| format!("Lock error: {}", e))?;
        apps.insert(app.id.clone(), app);
        Ok(())
    }

    /// 获取应用列表
    pub fn list_apps(&self) -> Result<Vec<SandboxApp>, String> {
        let apps = self.apps.lock().map_err(|e| format!("Lock error: {}", e))?;
        Ok(apps.values().cloned().collect())
    }

    /// 根据ID获取应用
    pub fn get_app(&self, id: &str) -> Result<Option<SandboxApp>, String> {
        let apps = self.apps.lock().map_err(|e| format!("Lock error: {}", e))?;
        Ok(apps.get(id).cloned())
    }

    /// 更新应用状态
    pub fn update_app_status(&self, id: &str, status: AppStatus) -> Result<bool, String> {
        let mut apps = self.apps.lock().map_err(|e| format!("Lock error: {}", e))?;
        if let Some(app) = apps.get_mut(id) {
            app.status = status;
            app.updated_at = chrono::Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 删除应用
    pub fn delete_app(&self, id: &str) -> Result<bool, String> {
        let mut apps = self.apps.lock().map_err(|e| format!("Lock error: {}", e))?;
        Ok(apps.remove(id).is_some())
    }
}

impl Default for AppStorage {
    fn default() -> Self {
        Self::new()
    }
}
