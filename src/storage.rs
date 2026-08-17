use crate::models::SandboxApp;
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
}

impl Default for AppStorage {
    fn default() -> Self {
        Self::new()
    }
}
