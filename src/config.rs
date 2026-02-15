use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration stored in JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub opacity: f32,
    pub is_enabled: bool,
    pub launch_on_login: bool,
    #[serde(default)]
    pub allow_capture: bool,
    /// Last user-set opacity for toggle restore
    #[serde(default = "default_last_opacity")]
    pub last_opacity: f32,
    pub hotkey_toggle: String,
    pub hotkey_increase: String,
    pub hotkey_decrease: String,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}

fn default_auto_update() -> bool {
    true
}

fn default_last_opacity() -> f32 {
    0.3
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            opacity: 0.3,
            is_enabled: true,
            launch_on_login: true,
            allow_capture: false,
            last_opacity: 0.3,
            hotkey_toggle: "Ctrl+Alt+End".into(),
            hotkey_increase: "Ctrl+Alt+Up".into(),
            hotkey_decrease: "Ctrl+Alt+Down".into(),
            auto_update: true,
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("SaveMyEyes").join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if path.exists() {
        let data = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

pub fn save_config(config: &AppConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let data = serde_json::to_string_pretty(config).unwrap_or_default();
    let _ = fs::write(&path, data);
}
