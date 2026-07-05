use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::sync::Mutex;

pub enum FanGroup { CPU, GPU }
pub enum FanBehavior { Auto, Max, Custom }

static ACPI_MUTEX: Mutex<()> = Mutex::new(());

// ─── WMI Buffer Protocol (WMBH opcode 0x0E / 0x10 / 0x05 / 0x06 / 0x14) ────
//
// Every WMBH call uses a 17-character hex buffer: "b" + 16 hex digits.
// Byte 0 is the first hex pair after "b".
//
// Fan Behavior (opcode 0x0E):
//   Byte[0]=0x09, Byte[1]=<sub-mode>, Byte[2]=<flags>
//   Auto   → sub-mode 0x04, flags 0x10  → "b0900410000000000"
//   Max    → sub-mode 0x08, flags 0x20  → "b0900820000000000"
//   Custom → sub-mode 0x0C, flags 0x30  → "b0900C30000000000"
//
// Fan Speed (opcode 0x10):
//   Byte[0]=<group_id>, Byte[1]=<percent 0-100>
//   CPU group_id=0x01, GPU group_id=0x04
//
// Sensor Read (opcode 0x05):
//   Byte[0]=0x01, Byte[1]=<sensor_id_hex>
//   Response: "{0x00, 0x<byte1>, 0x<byte2>, ...}"
//   Temperatures: 8-bit at index 1 (°C)
//   RPMs: 16-bit little-endian at indices 1+2
//   Sensor IDs: CPU temp=0x01, CPU RPM=0x02, GPU temp=0x0A, GPU RPM=0x06
//
// RGB Zone color (opcode 0x06):
//   Byte[0]=<zone_mask>, Byte[1]=R, Byte[2]=G, Byte[3]=B
//   Zone masks: 1→0x01, 2→0x02, 3→0x04, 4→0x08
//
// RGB Settings (opcode 0x14):
//   Byte[0]=<mode>, Byte[1]=<speed>, Byte[2]=<brightness>
//   Static mode=0, Breathing=1, Neon=2, Wave=3

pub fn set_fan_behavior(behavior: FanBehavior) -> Result<(), String> {
    let buffer = match behavior {
        // Byte[0]=0x09 (cmd), Byte[1]=sub-mode, Byte[2]=flags — see protocol notes above
        FanBehavior::Auto   => "b0900410000000000",
        FanBehavior::Max    => "b0900820000000000",
        FanBehavior::Custom => "b0900C30000000000",
    };
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x0E {}", buffer)).map(|_| ())
}

/// Probe the Acer WMID WMI device path with a harmless CPU-temp sensor read.
/// Returns Ok if the ACPI interface responds correctly, Err if the device path
/// is absent or the acpi_call module returns an error.
/// Used by check_dependencies() to distinguish "file accessible" from "WMI functional".
pub fn probe_wmi_path() -> bool {
    // Read CPU temperature sensor (sensor_id=0x01). This is the lightest
    // possible WMID call and has no side effects.
    read_sensor("01").is_ok()
}

pub fn set_fan_speed(fan: FanGroup, percent: u8) -> Result<(), String> {
    let group_id: u8 = match fan {
        FanGroup::CPU => 0x01,
        FanGroup::GPU => 0x04,
    };
    // Clamp to 0-100; the EC rejects out-of-range values with an ACPI Error
    let clamped = percent.min(100);
    let buffer = format!("b{:02x}{:02x}000000000000", group_id, clamped);
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x10 {}", buffer)).map(|_| ())
}

pub fn get_telemetry() -> Result<(u32, u32, u32, u32), String> {
    let cpu_temp = read_sensor("01")?;
    let mut gpu_temp = read_sensor("0A")?;
    let cpu_rpm = read_sensor("02")?;
    let gpu_rpm = read_sensor("06")?;

    // Fallback for GPU Temperature:
    // When NVIDIA Optimus puts the GPU in D3cold power state, the Acer ACPI Embedded Controller
    // often returns 0. We fallback to the native Linux hwmon driver to fetch the real temp if possible.
    if gpu_temp == 0 {
        gpu_temp = get_hwmon_gpu_temp();
    }

    Ok((cpu_temp, gpu_temp, cpu_rpm, gpu_rpm))
}

fn get_hwmon_gpu_temp() -> u32 {
    let mut highest_temp = 0u32;

    // 1. Try native Linux hwmon (works for nouveau, amdgpu, radeon)
    if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let name_path = entry.path().join("name");
            if let Ok(name) = std::fs::read_to_string(&name_path) {
                let name = name.trim().to_lowercase();
                if name.contains("amdgpu") || name.contains("nouveau") || name.contains("nvidia") || name.contains("radeon") {
                    let temp_path = entry.path().join("temp1_input");
                    if let Ok(val_str) = std::fs::read_to_string(&temp_path) {
                        if let Ok(val) = val_str.trim().parse::<u32>() {
                            highest_temp = highest_temp.max(val / 1000);
                        }
                    }
                }
            }
        }
    }

    // 2. Try proprietary NVIDIA driver (nvidia-smi) as a last resort if hwmon found nothing.
    //    This branch is intentionally separate from the nvidia-smi call in get_system_status()
    //    (which queries GPU *utilization*). Merging them would require more shared state; keeping
    //    them separate makes the fallback chain easy to reason about.
    if highest_temp == 0 {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=temperature.gpu")
            .arg("--format=csv,noheader")
            .output() {
            if output.status.success() {
                if let Ok(val_str) = String::from_utf8(output.stdout) {
                    if let Ok(val) = val_str.trim().parse::<u32>() {
                        highest_temp = val;
                    }
                }
            }
        }
    }

    highest_temp
}

fn read_sensor(sensor_id_hex: &str) -> Result<u32, String> {
    let buffer = format!("b01{}000000000000", sensor_id_hex);
    let result = execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x05 {}", buffer))?;
    // Result is like: {0x00, 0x37, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}
    // Temperatures: 8-bit value at index 1 (°C).
    // RPMs: 16-bit little-endian across indices 1 and 2.
    let clean = result.replace('{', "").replace('}', "").replace(' ', "");
    let parts: Vec<&str> = clean.split(',').collect();
    if parts.len() < 2 {
        return Err(format!("Sensor {}: invalid response format: '{}'", sensor_id_hex, result));
    }

    // Propagate parse failures rather than silently returning 0, so callers can
    // distinguish "sensor read 0°C" from "sensor response is corrupted / format changed".
    let byte1 = u32::from_str_radix(
        parts[1].trim_start_matches("0x"),
        16,
    ).map_err(|e| format!("Sensor {}: failed to parse byte1 '{}': {}", sensor_id_hex, parts[1], e))?;

    let byte2 = if parts.len() > 2 {
        u32::from_str_radix(
            parts[2].trim_start_matches("0x"),
            16,
        ).map_err(|e| format!("Sensor {}: failed to parse byte2 '{}': {}", sensor_id_hex, parts[2], e))?
    } else {
        0
    };

    // Some readings (like RPM) are 16-bit little endian. If it's just temp, byte2 is 0.
    Ok(byte1 | (byte2 << 8))
}

fn execute_acpi_call(command: &str) -> Result<String, String> {
    // Lock the mutex to prevent concurrent writes/reads causing empty buffers.
    // Use lock().unwrap_or_else to recover from Mutex poisoning: if a previous
    // thread panicked while holding the lock, we clear the poison and continue,
    // since the ACPI file handle is re-opened on every call and no shared state
    // is left in an inconsistent condition.
    let _lock = ACPI_MUTEX.lock().unwrap_or_else(|poisoned| {
        eprintln!("[nitrosense] ACPI_MUTEX was poisoned — recovering");
        poisoned.into_inner()
    });

    {
        let mut file = OpenOptions::new().write(true).open("/proc/acpi/call")
            .map_err(|e| format!("Failed to open /proc/acpi/call for writing: {}", e))?;
        file.write_all(format!("{}\n", command).as_bytes())
            .map_err(|e| format!("Failed to write to /proc/acpi/call: {}", e))?;
    }

    let mut file = OpenOptions::new().read(true).open("/proc/acpi/call")
        .map_err(|e| format!("Failed to open /proc/acpi/call for reading: {}", e))?;

    // acpi_call kernel module has a bug where multiple small reads corrupt the buffer state.
    // fs::read_to_string() uses a 32-byte initial buffer which triggers this bug and returns "".
    // To fix this, we read in one syscall using a dynamic Vec, growing until the read is complete.
    // A typical ACPI response is ~40 bytes; 4096 is generous headroom against truncation.
    let mut buf = Vec::with_capacity(4096);
    buf.resize(4096, 0u8);
    let bytes_read = file.read(&mut buf)
        .map_err(|e| format!("Failed to read result: {}", e))?;

    if bytes_read == buf.len() {
        // If we exactly filled the buffer the response may have been truncated — return an error
        // rather than silently returning partial data.
        return Err(format!(
            "ACPI response may be truncated: read {} bytes which filled the entire buffer",
            bytes_read
        ));
    }

    let result = String::from_utf8_lossy(&buf[..bytes_read]).to_string();

    // Use a case-insensitive prefix check for ACPI errors to avoid misidentifying
    // a valid hex response that happens to contain the substring "Error" elsewhere.
    let trimmed = result.trim().trim_end_matches('\0');
    if trimmed.to_ascii_lowercase().starts_with("error") {
        return Err(format!("ACPI Error: {}", trimmed));
    }

    Ok(trimmed.to_string())
}

pub fn init_rgb() -> Result<(), String> {
    // Opcode 0x05 with sensor_id=0x00 initializes the RGB WMI subsystem on Acer Nitro hardware.
    // Despite sharing opcode 0x05 with sensor reads, a sensor_id of 0x00 is not a valid sensor
    // and acts as an initialization trigger. The response is ignored intentionally.
    execute_acpi_call("\\_SB.PC00.WMID.WMBH 0x0 0x05 b0000000000000000").map(|_| ())
}

pub fn set_rgb_zone(zone: u8, r: u8, g: u8, b: u8) -> Result<(), String> {
    let zone_mask = match zone {
        1 => 0x01,
        2 => 0x02,
        3 => 0x04,
        4 => 0x08,
        _ => return Err("Invalid zone".into()),
    };
    let buffer = format!("b{:02x}{:02x}{:02x}{:02x}00000000", zone_mask, r, g, b);
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x06 {}", buffer)).map(|_| ())
}

pub fn apply_rgb_settings(mode: u8, speed: u8, brightness: u8) -> Result<(), String> {
    let buffer = format!("b{:02x}{:02x}{:02x}000000000001000000000000", mode, speed, brightness);
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x14 {}", buffer)).map(|_| ())
}
