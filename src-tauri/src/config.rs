use crate::ai::AiSettings;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub vault: Option<String>,
    #[serde(default)]
    pub ai: AiSettings,
}

pub fn read_config(app: &AppHandle) -> AppConfig {
    let Ok(dir) = app.path().app_config_dir() else {
        return AppConfig::default();
    };
    fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Не удалось найти папку конфигурации: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("config.json"),
        serde_json::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn save_vault(app: &AppHandle, root: &Path) -> Result<(), String> {
    let mut config = read_config(app);
    config.vault = Some(root.to_string_lossy().to_string());
    save_config(app, &config)
}
