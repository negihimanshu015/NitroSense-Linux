#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]
mod wmi;
mod config;
use wmi::{FanBehavior, FanGroup};
use sysinfo::{System, RefreshKind};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::thread;
use tauri::State;

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
    // Check if GPU cache needs refresh and read last cached value under a brief lock scope
    let (needs_refresh, cached_gpu) = {
        let cache = state.nvidia_cache.lock().unwrap_or_else(|poisoned| {
            eprintln!("[nitrosense] nvidia_cache Mutex was poisoned — recovering");
            poisoned.into_inner()
        });
        let expired = cache.last_updated
            .map(|t| t.elapsed() >= NVIDIA_CACHE_TTL)
            .unwrap_or(true);
        (expired, cache.value)
    };

    let mut gpu_util = cached_gpu;
    if needs_refresh {
        // Run the subprocess query without holding any state mutex locks
        if let Some(stdout) = wmi::run_nvidia_smi_with_timeout(
            &["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"],
            Duration::from_secs(2),
        ) {
            if let Ok(val) = stdout.trim().parse::<f32>() {
                gpu_util = val;
                let mut cache = state.nvidia_cache.lock().unwrap_or_else(|poisoned| {
                    poisoned.into_inner()
                });
                cache.value = val;
                cache.last_updated = Some(Instant::now());
            }
        }
    }

    // Run CPU and memory refresh under a separate brief lock scope
    let (cpu_usage, ram_usage) = {
        let mut sys = state.sys.lock().unwrap_or_else(|poisoned| {
            eprintln!("[nitrosense] AppState Mutex was poisoned — recovering");
            poisoned.into_inner()
        });
        sys.refresh_cpu();
        sys.refresh_memory();
        let cpu = sys.global_cpu_info().cpu_usage();
        let total_mem = sys.total_memory();
        let ram = if total_mem > 0 {
            (sys.used_memory() as f32 / total_mem as f32) * 100.0
        } else {
            0.0
        };
        (cpu, ram)
    };

    Ok((cpu_usage, ram_usage, gpu_util))
}

/// Check if the acpi_call module is loaded and the Acer WMI path is responding.
#[tauri::command]
fn check_dependencies() -> (bool, bool) {
    match wmi::probe_wmi_path() {
        Ok(()) => (true, true),
        Err(wmi::WmiError::AcpiCallOpenFailed(_)) => (false, false),
        Err(wmi::WmiError::AcpiCallFailed(_)) => (true, false),
        Err(wmi::WmiError::Other(_)) => (true, false),
    }
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
        match config::apply_saved_settings() {
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
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                println!("[nitrosense-linux] Close requested, resetting fan behavior to Auto for safety");
                if let Err(e) = wmi::set_fan_behavior(FanBehavior::Auto) {
                    eprintln!("[nitrosense-linux] WARNING: Failed to reset fans on close: {}", e);
                }
            }
        })
        .manage(AppState {
            sys: Mutex::new(sys),
            nvidia_cache: Mutex::new(NvidiaCache {
                value: 0.0,
                last_updated: None,
            }),
        })
        .invoke_handler(tauri::generate_handler![
            set_fan_mode, set_fan_speed, get_telemetry, get_system_status, check_dependencies,
            init_rgb, set_rgb_zone, apply_rgb_settings, config::load_config, config::save_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
