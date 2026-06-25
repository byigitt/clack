//! Persisted settings (JSON in `~/Library/Application Support/Clack/settings.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub volume: f32,
    pub pitch: f32,
    pub ignore_rapid: bool,
    pub disable_modifiers: bool,
    /// Selected soundpack directory name (UUID), if any.
    pub pack: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            volume: 0.5,
            pitch: 0.0,
            ignore_rapid: false,
            disable_modifiers: false,
            pack: None,
        }
    }
}

fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Clack/settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(p) = path() else {
            return Settings::default();
        };
        std::fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_merges_defaults() {
        // Only volume present -> the rest fall back to Default.
        let s: Settings = serde_json::from_str(r#"{"volume":0.9}"#).unwrap();
        assert_eq!(s.volume, 0.9);
        assert!(s.enabled, "enabled defaults to true");
        assert_eq!(s.pitch, 0.0);
        assert!(s.pack.is_none());
    }
}
