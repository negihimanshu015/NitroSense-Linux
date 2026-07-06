use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::sync::Mutex;

pub enum FanGroup { CPU, GPU }
pub enum FanBehavior { Auto, Max, Custom }

static ACPI_MUTEX: Mutex<()> = Mutex::new(());

// WMI Buffer Protocol (WMBH 17-char hex buffer: "b" + 16 hex digits)
// Opcode 0x0E (Fan Behavior): Byte[0]=0x09, Byte[1]=sub-mode, Byte[2]=flags (Auto: 04,10; Max: 08,20; Custom: 0C,30)
// Opcode 0x10 (Fan Speed):    Byte[0]=group_id (CPU: 01, GPU: 04), Byte[1]=percent (0-100)
// Opcode 0x05 (Sensor Read):  Byte[0]=0x01, Byte[1]=sensor_id (CPU temp: 01, RPM: 02; GPU temp: 0A, RPM: 06)
//                             Response matches "{0x00, 0x<b1>, 0x<b2>, ...}". Temp is 8-bit (b1); RPM is 16-bit LE (b1|b2<<8)
// Opcode 0x06 (RGB Color):    Byte[0]=zone_mask (01, 02, 04, 08), Byte[1..3]=R,G,B
// Opcode 0x14 (RGB Settings): Byte[0]=mode (Static: 0, Breath: 1, Neon: 2, Wave: 3), Byte[1]=speed, Byte[2]=brightness

pub fn set_fan_behavior(behavior: FanBehavior) -> Result<(), String> {
    let buffer = match behavior {
        FanBehavior::Auto   => "b0900410000000000",
        FanBehavior::Max    => "b0900820000000000",
        FanBehavior::Custom => "b0900C30000000000",
    };
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x0E {}", buffer)).map(|_| ())
}

/// Probe the Acer WMI path with a harmless CPU-temp sensor read.
pub fn probe_wmi_path() -> bool {
    read_sensor("01").is_ok()
}

pub fn set_fan_speed(fan: FanGroup, percent: u8) -> Result<(), String> {
    let group_id: u8 = match fan {
        FanGroup::CPU => 0x01,
        FanGroup::GPU => 0x04,
    };
    let clamped = percent.min(100); // EC rejects values > 100
    let buffer = format!("b{:02x}{:02x}000000000000", group_id, clamped);
    execute_acpi_call(&format!("\\_SB.PC00.WMID.WMBH 0x0 0x10 {}", buffer)).map(|_| ())
}

pub fn get_telemetry() -> Result<(u32, u32, u32, u32), String> {
    let cpu_temp = read_sensor("01")?;
    let mut gpu_temp = read_sensor("0A")?;
    let cpu_rpm = read_sensor("02")?;
    let gpu_rpm = read_sensor("06")?;

    // Fallback to hwmon if GPU is in D3cold (EC returns 0 temp).
    if gpu_temp == 0 {
        gpu_temp = get_hwmon_gpu_temp();
    }

    Ok((cpu_temp, gpu_temp, cpu_rpm, gpu_rpm))
}

fn get_hwmon_gpu_temp() -> u32 {
    let mut highest_temp = 0u32;

    // 1. Try native Linux hwmon drivers.
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

    // 2. Fallback to nvidia-smi query.
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
    
    // Parse ACPI response (e.g. "{0x00, 0x37, 0x00, ...}").
    let clean = result.replace(['{', '}', ' '], "");
    let parts: Vec<&str> = clean.split(',').collect();
    if parts.len() < 2 {
        return Err(format!("Sensor {}: invalid response format: '{}'", sensor_id_hex, result));
    }

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

    // RPM readings are 16-bit little endian. Temperatures only use byte1.
    Ok(byte1 | (byte2 << 8))
}

fn execute_acpi_call(command: &str) -> Result<String, String> {
    // Prevent concurrent writes and recover from Mutex poisoning.
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

    // Read in one syscall to bypass acpi_call bug where multiple small reads corrupt state.
    let mut buf = vec![0u8; 4096];
    let bytes_read = file.read(&mut buf)
        .map_err(|e| format!("Failed to read result: {}", e))?;

    if bytes_read == buf.len() {
        return Err("ACPI response may be truncated".into());
    }

    let result = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
    let trimmed = result.trim().trim_end_matches('\0');
    if trimmed.to_ascii_lowercase().starts_with("error") {
        return Err(format!("ACPI Error: {}", trimmed));
    }

    Ok(trimmed.to_string())
}

pub fn init_rgb() -> Result<(), String> {
    // Opcode 0x05 with sensor_id 0x00 initializes the RGB WMI subsystem.
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
