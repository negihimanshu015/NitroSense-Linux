use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use crate::wmi::{self, FanBehavior, FanGroup};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub fan_mode: String,
    pub cpu_percent: u8,
    pub gpu_percent: u8,
    pub rgb_mode: String,
    pub rgb_brightness: u8,
    pub rgb_speed_index: u8,
    pub rgb_zone_color_1: String,
    pub rgb_zone_color_2: String,
    pub rgb_zone_color_3: String,
    pub rgb_zone_color_4: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            fan_mode: "auto".to_string(),
            cpu_percent: 50,
            gpu_percent: 50,
            rgb_mode: "static".to_string(),
            rgb_brightness: 80,
            rgb_speed_index: 2,
            rgb_zone_color_1: "rgb(255, 0, 0)".to_string(),
            rgb_zone_color_2: "rgb(0, 255, 0)".to_string(),
            rgb_zone_color_3: "rgb(0, 0, 255)".to_string(),
            rgb_zone_color_4: "rgb(255, 255, 0)".to_string(),
        }
    }
}

pub fn get_config_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("Failed to read HOME env var: {}", e))?;
    let path = PathBuf::from(home).join(".config").join("nitrosense-linux");
    Ok(path)
}

pub fn load_config_file() -> Result<AppConfig, String> {
    let mut path = get_config_dir()?;
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create config dir: {}", e))?;
    path.push("config.json");

    if !path.exists() {
        let default_config = AppConfig::default();
        let serialized = serde_json::to_string_pretty(&default_config)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serialized)
            .map_err(|e| format!("Failed to write default config temp file: {}", e))?;
        std::fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Failed to rename default config: {}", e))?;
            
        return Ok(default_config);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;
    
    match serde_json::from_str(&content) {
        Ok(config) => Ok(config),
        Err(e) => {
            eprintln!("[nitrosense-linux] Warning: Config file corrupt, resetting to defaults. Error: {}", e);
            let default_config = AppConfig::default();
            let serialized = serde_json::to_string_pretty(&default_config)
                .map_err(|e| format!("Failed to serialize default config: {}", e))?;
            
            let tmp_path = path.with_extension("json.tmp");
            std::fs::write(&tmp_path, serialized)
                .map_err(|e| format!("Failed to write default config temp file: {}", e))?;
            std::fs::rename(&tmp_path, &path)
                .map_err(|e| format!("Failed to rename default config: {}", e))?;
                
            Ok(default_config)
        }
    }
}

pub fn save_config_file(config: &AppConfig) -> Result<(), String> {
    let mut path = get_config_dir()?;
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create config dir: {}", e))?;
    path.push("config.json");

    let serialized = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serialized)
        .map_err(|e| format!("Failed to write config temp file: {}", e))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename config file: {}", e))?;
        
    Ok(())
}

#[tauri::command]
pub fn load_config() -> Result<AppConfig, String> {
    load_config_file()
}

#[tauri::command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    save_config_file(&config)
}

fn parse_rgb(color_str: &str) -> Option<(u8, u8, u8)> {
    let clean = color_str.replace(" ", "");
    if (clean.starts_with("rgb(") || clean.starts_with("rgba(")) && clean.ends_with(')') {
        let start = if clean.starts_with("rgba(") { 5 } else { 4 };
        let inner = &clean[start..clean.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some((r, g, b));
        }
    }
    None
}

pub fn apply_saved_settings() -> Result<(), String> {
    let config = load_config_file()?;
    let mut errors = Vec::new();

    // Apply Fan settings
    let behavior = match config.fan_mode.as_str() {
        "auto"   => FanBehavior::Auto,
        "max"    => FanBehavior::Max,
        "custom" => FanBehavior::Custom,
        _        => FanBehavior::Auto,
    };
    if let Err(e) = wmi::set_fan_behavior(behavior) {
        errors.push(format!("Failed to set fan behavior: {}", e));
    }

    if config.fan_mode == "custom" {
        if let Err(e) = wmi::set_fan_speed(FanGroup::CPU, config.cpu_percent) {
            errors.push(format!("Failed to set CPU fan speed: {}", e));
        }
        if let Err(e) = wmi::set_fan_speed(FanGroup::GPU, config.gpu_percent) {
            errors.push(format!("Failed to set GPU fan speed: {}", e));
        }
    }

    // Apply RGB settings
    if let Err(e) = wmi::init_rgb() {
        errors.push(format!("Failed to initialize RGB: {}", e));
    } else {
        let zones = [
            (1, &config.rgb_zone_color_1),
            (2, &config.rgb_zone_color_2),
            (3, &config.rgb_zone_color_3),
            (4, &config.rgb_zone_color_4),
        ];

        for (zone_id, color_str) in zones {
            if let Some((r, g, b)) = parse_rgb(color_str) {
                if let Err(e) = wmi::set_rgb_zone(zone_id, r, g, b) {
                    errors.push(format!("Failed to set RGB zone {}: {}", zone_id, e));
                }
            }
        }

        let mode = match config.rgb_mode.as_str() {
            "static" => 0,
            "breathing" => 1,
            "neon" => 2,
            "wave" => 3,
            _ => 0,
        };

        if let Err(e) = wmi::apply_rgb_settings(mode, config.rgb_speed_index, config.rgb_brightness) {
            errors.push(format!("Failed to apply RGB settings: {}", e));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
