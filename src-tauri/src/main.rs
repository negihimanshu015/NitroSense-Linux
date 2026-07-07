// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod wmi;
use wmi::{FanBehavior, FanGroup};
use sysinfo::{System, RefreshKind};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::thread;
use tauri::State;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AppConfig {
    fan_mode: String,
    cpu_percent: u8,
    gpu_percent: u8,
    rgb_mode: String,
    rgb_brightness: u8,
    rgb_speed_index: u8,
    rgb_zone_color_1: String,
    rgb_zone_color_2: String,
    rgb_zone_color_3: String,
    rgb_zone_color_4: String,
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

fn get_config_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("Failed to read HOME env var: {}", e))?;
    let path = std::path::PathBuf::from(home).join(".config").join("nitrosense-linux");
    Ok(path)
}

fn load_config_file() -> Result<AppConfig, String> {
    let mut path = get_config_path()?;
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create config dir: {}", e))?;
    path.push("config.json");

    if !path.exists() {
        let default_config = AppConfig::default();
        let serialized = serde_json::to_string_pretty(&default_config)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        std::fs::write(&path, serialized)
            .map_err(|e| format!("Failed to write default config: {}", e))?;
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
            std::fs::write(&path, serialized)
                .map_err(|e| format!("Failed to write default config: {}", e))?;
            Ok(default_config)
        }
    }
}

fn save_config_file(config: &AppConfig) -> Result<(), String> {
    let mut path = get_config_path()?;
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create config dir: {}", e))?;
    path.push("config.json");

    let serialized = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(&path, serialized)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

#[tauri::command]
fn load_config() -> Result<AppConfig, String> {
    load_config_file()
}

#[tauri::command]
fn save_config(config: AppConfig) -> Result<(), String> {
    save_config_file(&config)
}

fn parse_rgb(color_str: &str) -> Option<(u8, u8, u8)> {
    let clean = color_str.replace(" ", "");
    if clean.starts_with("rgb(") && clean.ends_with(')') {
        let inner = &clean[4..clean.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some((r, g, b));
        }
    }
    None
}

fn apply_saved_settings() -> Result<(), String> {
    let config = load_config_file()?;

    // Apply Fan settings
    let behavior = match config.fan_mode.as_str() {
        "auto"   => FanBehavior::Auto,
        "max"    => FanBehavior::Max,
        "custom" => FanBehavior::Custom,
        _        => FanBehavior::Auto,
    };
    wmi::set_fan_behavior(behavior)?;

    if config.fan_mode == "custom" {
        wmi::set_fan_speed(FanGroup::CPU, config.cpu_percent)?;
        wmi::set_fan_speed(FanGroup::GPU, config.gpu_percent)?;
    }

    // Apply RGB settings
    wmi::init_rgb()?;

    let zones = [
        (1, &config.rgb_zone_color_1),
        (2, &config.rgb_zone_color_2),
        (3, &config.rgb_zone_color_3),
        (4, &config.rgb_zone_color_4),
    ];

    for (zone_id, color_str) in zones {
        if let Some((r, g, b)) = parse_rgb(color_str) {
            wmi::set_rgb_zone(zone_id, r, g, b)?;
        }
    }

    let mode = match config.rgb_mode.as_str() {
        "static" => 0,
        "breathing" => 1,
        "neon" => 2,
        "wave" => 3,
        _ => 0,
    };

    let speed = if mode == 0 {
        0
    } else {
        match config.rgb_speed_index {
            1 => 1,
            3 => 9,
            _ => 5,
        }
    };

    wmi::apply_rgb_settings(mode, speed, config.rgb_brightness)?;

    Ok(())
}

/// Cached nvidia-smi GPU utilization to avoid execution overhead on every tick.
struct NvidiaCache {
    value: f32,
    last_updated: Option<Instant>,
}

const NVIDIA_CACHE_TTL: Duration = Duration::from_secs(4);

struct AppState {
    sys: Mutex<System>,
    nvidia_cache: Mutex<NvidiaCache>,
}

#[tauri::command]
fn set_fan_mode(mode: &str) -> Result<(), String> {
    let behavior = match mode {
        "auto"   => FanBehavior::Auto,
        "max"    => FanBehavior::Max,
        "custom" => FanBehavior::Custom,
        _        => return Err("Invalid mode".to_string()),
    };
    wmi::set_fan_behavior(behavior)
}

#[tauri::command]
fn set_fan_speed(cpu_percent: u8, gpu_percent: u8) -> Result<(), String> {
    // Set CPU first, then GPU (no auto-rollback if GPU fails).
    wmi::set_fan_speed(FanGroup::CPU, cpu_percent)?;
    wmi::set_fan_speed(FanGroup::GPU, gpu_percent)?;
    Ok(())
}

#[tauri::command]
fn get_telemetry() -> Result<(u32, u32, u32, u32), String> {
    wmi::get_telemetry()
}

#[tauri::command]
fn get_system_status(state: State<AppState>) -> Result<(f32, f32, f32), String> {
    // Recover from Mutex poisoning if a previous thread panicked.
    let mut sys = state.sys.lock().unwrap_or_else(|poisoned| {
        eprintln!("[nitrosense] AppState Mutex was poisoned — recovering");
        poisoned.into_inner()
    });

    sys.refresh_cpu();
    sys.refresh_memory();
    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let ram_usage = (sys.used_memory() as f32 / sys.total_memory() as f32) * 100.0;

    // Cache nvidia-smi results to minimize subprocess execution overhead.
    let mut cache = state.nvidia_cache.lock().unwrap_or_else(|poisoned| {
        eprintln!("[nitrosense] nvidia_cache Mutex was poisoned — recovering");
        poisoned.into_inner()
    });

    let needs_refresh = cache.last_updated
        .map(|t| t.elapsed() >= NVIDIA_CACHE_TTL)
        .unwrap_or(true);

    if needs_refresh {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=utilization.gpu")
            .arg("--format=csv,noheader,nounits")
            .output()
        {
            if output.status.success() {
                if let Ok(val_str) = String::from_utf8(output.stdout) {
                    if let Ok(val) = val_str.trim().parse::<f32>() {
                        cache.value = val;
                        cache.last_updated = Some(Instant::now());
                    }
                }
            }
        }
    }

    Ok((cpu_usage, ram_usage, cache.value))
}

/// Check if the acpi_call module is loaded and the Acer WMI path is responding.
#[tauri::command]
fn check_dependencies() -> (bool, bool) {
    let acpi_ok = std::fs::OpenOptions::new()
        .write(true)
        .open("/proc/acpi/call")
        .is_ok();

    // Probe the WMI path if /proc/acpi/call is accessible to verify the path is functional.
    let wmi_ok = acpi_ok && wmi::probe_wmi_path();

    (acpi_ok, wmi_ok)
}

#[tauri::command]
fn init_rgb() -> Result<(), String> {
    wmi::init_rgb()
}

#[tauri::command]
fn set_rgb_zone(zone: u8, r: u8, g: u8, b: u8) -> Result<(), String> {
    wmi::set_rgb_zone(zone, r, g, b)
}

#[tauri::command]
fn apply_rgb_settings(mode: u8, speed: u8, brightness: u8) -> Result<(), String> {
    wmi::apply_rgb_settings(mode, speed, brightness)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--apply".to_string()) || args.contains(&"-a".to_string()) {
        println!("[nitrosense-linux] Applying saved configurations...");
        match apply_saved_settings() {
            Ok(_) => {
                println!("[nitrosense-linux] Settings applied successfully.");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[nitrosense-linux] Error applying settings: {}", e);
                std::process::exit(1);
            }
        }
    }

    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(sysinfo::CpuRefreshKind::everything())
            .with_memory(sysinfo::MemoryRefreshKind::everything())
    );

    // Prime the CPU usage baseline (sysinfo requires two snapshots to calculate usage delta).
    sys.refresh_cpu();
    thread::sleep(Duration::from_millis(200));
    sys.refresh_cpu();

    tauri::Builder::default()
        .manage(AppState {
            sys: Mutex::new(sys),
            nvidia_cache: Mutex::new(NvidiaCache {
                value: 0.0,
                last_updated: None,
            }),
        })
        .invoke_handler(tauri::generate_handler![
            set_fan_mode, set_fan_speed, get_telemetry, get_system_status, check_dependencies,
            init_rgb, set_rgb_zone, apply_rgb_settings, load_config, save_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
