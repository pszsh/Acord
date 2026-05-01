use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
pub struct Config {
    path: PathBuf,
    data: HashMap<String, String>,
}

#[allow(dead_code)]
impl Config {
    pub fn load() -> Self {
        let dir = config_dir();
        std::fs::create_dir_all(&dir).ok();
        let notes = dir.join("notes");
        std::fs::create_dir_all(&notes).ok();

        let path = dir.join("config.json");
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    pub fn get(&self, key: &str, default: &str) -> String {
        self.data.get(key).cloned().unwrap_or_else(|| default.to_string())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.save();
    }

    pub fn theme_mode(&self) -> &str {
        self.data.get("themeMode").map(|s| s.as_str()).unwrap_or("auto")
    }

    pub fn line_indicator(&self) -> &str {
        self.data.get("lineIndicatorMode").map(|s| s.as_str()).unwrap_or("on")
    }

    pub fn gutter_rainbow(&self) -> bool {
        self.data.get("gutterRainbow").map(|s| s != "false").unwrap_or(true)
    }

    pub fn notes_dir(&self) -> PathBuf {
        let default = config_dir().join("notes");
        match self.data.get("autoSaveDirectory") {
            Some(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    default
                } else {
                    let p = PathBuf::from(trimmed);
                    if p.is_dir() { p } else { default }
                }
            }
            None => default,
        }
    }

    pub fn auto_pair_flags(&self) -> u32 {
        self.data.get("autoPairFlags")
            .and_then(|s| s.parse().ok())
            .unwrap_or(63)
    }
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".acord")
}
