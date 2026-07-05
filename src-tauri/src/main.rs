// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod wmi;
use wmi::{FanBehavior, FanGroup};
use sysinfo::{System, RefreshKind};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::State;

/// Cached nvidia-smi GPU utilization value.
/// We re-run the subprocess at most once every NVIDIA_CACHE_TTL to avoid
/// the overhead of spawning a process on every 2-second telemetry tick.
struct NvidiaCache {
    value: f32,
    last_updated: Option<Instant>,
}

/// How long a cached nvidia-smi reading is considered fresh.
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
    // Apply CPU first; only proceed to GPU if CPU succeeded.
    // This prevents partial application (GPU set, CPU not) which would leave
    // the system in an inconsistent fan state.
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
    // Recover from Mutex poisoning: if a previous thread panicked while holding
    // this lock, the System state is inconsistent but re-initialising it is safe.
    let mut sys = state.sys.lock().unwrap_or_else(|poisoned| {
        eprintln!("[nitrosense] AppState Mutex was poisoned — recovering");
        poisoned.into_inner()
    });

    sys.refresh_cpu();
    sys.refresh_memory();
    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let ram_usage = (sys.used_memory() as f32 / sys.total_memory() as f32) * 100.0;

    // GPU utilization via nvidia-smi.
    // We cache the result for NVIDIA_CACHE_TTL (4 s) so we don't spawn a new
    // process on every 2-second telemetry tick — halving the subprocess overhead.
    let mut cache = state.nvidia_cache.lock().unwrap_or_else(|poisoned| {
        eprintln!("[nitrosense] nvidia_cache Mutex was poisoned — recovering");
        poisoned.into_inner()
    });

    let needs_refresh = cache.last_updated
        .map(|t| t.elapsed() >= NVIDIA_CACHE_TTL)
        .unwrap_or(true); // Always fetch on the very first call

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

/// Returns (acpi_ok, wmi_ok):
///   acpi_ok  — /proc/acpi/call is writable (acpi_call module loaded + permissions set)
///   wmi_ok   — the Acer WMID WMI path responds to a probe read (acer_wmi interface present)
///
/// A false-positive from checking only /proc/acpi/call write access is possible when the
/// file exists but the WMID device path is absent. Probing with a harmless sensor read
/// confirms the WMI interface is actually functional, not just reachable at the file level.
#[tauri::command]
fn check_dependencies() -> (bool, bool) {
    let acpi_ok = std::fs::OpenOptions::new()
        .write(true)
        .open("/proc/acpi/call")
        .is_ok();

    // Only probe the WMI path if /proc/acpi/call is accessible — otherwise we'd
    // just get an error from the file open, not from the WMI path being absent.
    let wmi_ok = if acpi_ok {
        // Probe with a harmless sensor-read command (CPU temp, sensor 0x01).
        // If the WMI device path is missing the EC returns an ACPI error, which
        // wmi::execute_acpi_call propagates as Err — so is_ok() will be false.
        wmi::probe_wmi_path()
    } else {
        false
    };

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
    let sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(sysinfo::CpuRefreshKind::everything())
            .with_memory(sysinfo::MemoryRefreshKind::everything())
    );

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
            init_rgb, set_rgb_zone, apply_rgb_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
