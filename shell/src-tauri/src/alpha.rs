use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Manager}; 

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaTelemetryConfig {
    #[serde(default)]
    pub session_summary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaConstraints {
    #[serde(default)]
    pub no_screen_recording: bool,
    #[serde(default)]
    pub no_public_demos: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaAccess {
    pub enabled: bool,
    pub cohort_id: Option<String>,
    pub issued_at: u64,
    pub telemetry: Option<AlphaTelemetryConfig>,
    pub constraints: Option<AlphaConstraints>,
}

impl AlphaAccess {
    pub fn load(app_handle: &AppHandle) -> Option<Self> {
        let config_dir = app_handle.path().app_config_dir().ok()?;
        let alpha_path = config_dir.join("alpha.json");

        if !alpha_path.exists() {
            return None;
        }

        match fs::read_to_string(alpha_path) {
            Ok(content) => {
                match serde_json::from_str::<AlphaAccess>(&content) {
                    Ok(access) => {
                        if access.enabled {
                            Some(access)
                        } else {
                            None
                        }
                    },
                    Err(_) => None, // Silent failure
                }
            },
            Err(_) => None, // Silent failure
        }
    }
}
