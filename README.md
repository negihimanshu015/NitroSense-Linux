# NitroSense Linux

> A native Linux replacement for Acer's NitroSense software — fan control, keyboard RGB, and live system telemetry for Acer Nitro laptops.

---

## What Is This?

**NitroSense Linux** is an open-source desktop application that brings the functionality of Acer's Windows-only NitroSense utility to Linux. It is built with [Tauri 2](https://tauri.app/) (Rust backend + HTML/CSS/JS frontend) and communicates directly with the laptop's embedded controller via the `acpi_call` kernel module and the Acer WMID WMI interface — the same hardware pathway used by the official Windows software.

---

## Purpose

This project exposes the ACPI/WMI hardware interface through a native Linux GUI. The goal is to give Acer Nitro Linux users the same level of hardware control they would have on Windows, without needing to reboot, use a VM, or run shell scripts manually.

---

## Tested Hardware & Kernel

| Field | Value |
|---|---|
| **Laptop Model** | Acer Nitro AN515-57 |
| **Manufacturer** | Acer |
| **Linux Kernel** | `7.0.12-201.fc44.x86_64` |
| **Distribution** | Fedora 44 Workstation |

> [!NOTE]
> Only tested on the hardware listed above. Other Acer Nitro/Aspire models that use the same `\_SB.PC00.WMID.WMBH` ACPI interface may work, but have **not** been tested.

---

## Compatibility

### Likely Compatible
- Other **Acer Nitro 5 / Nitro 7** variants that share the `WMBH` WMI interface (e.g. AN515-55, AN515-58, AN517 series)

### May Not Work
- Acer models that use a different WMI path (the app checks for this on startup and will warn you)
- Laptops where the GPU reports 0°C via ACPI while in Optimus D3cold

### Not Supported
- Non-Acer hardware
- Any Windows environment

---

## Features

| Feature | Description |
|---|---|
| **Fan Control** | Switch between Auto, Max, and Custom modes |
| **Custom Fan Speed** | Set CPU and GPU fan speed independently (0–100%) |
| **Temperatures & RPM** | Live CPU/GPU temperatures and fan RPM |
| **System Monitoring** | Real-time CPU, GPU, and RAM usage |
| **Temperature History** | Rolling 60-second temperature chart |
| **Keyboard RGB** | 4-zone RGB color control with effects (Static, Breathing, Wave, Neon) |

---

## Warnings

> [!CAUTION]
> Fan control writes directly to the embedded controller. The app clamps values to 0–100% and uses official Acer opcodes, but use at your own risk.

> [!WARNING]
> `/proc/acpi/call` is a privileged interface. The `install-permissions.sh` script sets up group-level access so the app never needs to run as root.

---

## Dependencies

| Dependency | Purpose |
|---|---|
| `acpi_call` kernel module | Sends ACPI calls for fan and RGB control |
| `acer_wmi` kernel module | Exposes the Acer WMID WMI device path |
| `nvidia-smi` *(optional)* | GPU utilization and temperature for NVIDIA drivers |
| `WebKit2GTK` | Tauri's rendering backend |

---

## Installation

### Step 1 — Install `acpi_call`

**Fedora** (via RPM Fusion):
```bash
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install akmod-acpi_call
```

---

### Step 2 — Clone the Repository

```bash
git clone https://github.com/negihimanshu015/NitroSense-Linux.git
cd NitroSense-Linux
```

---

### Step 3 — Set Up Permissions

This script creates a `nitrosense` group, adds your user to it, and configures `/proc/acpi/call` to be writable by that group — so the app never needs `sudo`.

```bash
sudo ./install-permissions.sh
```

Then **log out and log back in** (or run `newgrp nitrosense`) for the group change to take effect.

---

### Step 4 — Install Tauri Prerequisites

If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install Node.js:
```bash
sudo dnf install nodejs
```

Install Tauri system dependencies:
```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel
```

---

### Step 5 — Build the Application

```bash
npm install
npm run tauri build -- --no-bundle
```

The compiled binary will be at:
```
src-tauri/target/release/nitrosense-linux
```

---

### Step 6 — Install as a Desktop App (Optional)

```bash
bash install-app.sh
```

This installs to `~/.local/` and **does not require `sudo`**. Make sure `~/.local/bin` is in your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

You can then launch from your app menu by searching **NitroSense**, or from a terminal:

```bash
nitrosense-linux
```

---

### Running Without Installing

```bash
./src-tauri/target/release/nitrosense-linux
```

---

## Development

To run in development mode with hot-reload:

```bash
npm install
npm run tauri dev
```

---

## How to Contribute

### Reporting Issues

1. Open an issue on [GitHub Issues](https://github.com/negihimanshu015/NitroSense-Linux/issues)
2. Include your **laptop model** (`cat /sys/devices/virtual/dmi/id/product_name`)
3. Include your **kernel version** (`uname -r`)
4. Attach any error output from the terminal

### Submitting Code

1. **Fork** the repository
2. **Create a branch** for your feature or fix:
   ```bash
   git checkout -b feat/my-feature
   ```
3. **Make your changes** — keep commits focused and well-described
4. **Test on real hardware** if your change touches `wmi.rs` or any ACPI code
5. **Open a Pull Request** against the `main` branch with a clear description of what changed and why

### Areas That Need Help

- Testing on other Acer Nitro / Aspire model variants
- Support for additional RGB lighting effects
- Power profile switching (Silent / Balanced / Performance)
- Packaging: `.deb`, `.rpm`, and AppImage bundles

### Code Style

- Rust: follow standard `rustfmt` formatting (`cargo fmt`)
- JavaScript: vanilla ES module style — no frameworks, no build step for the frontend
- Comments: Keep comments minimal and focused on explaining non-obvious hardware/protocol constraints rather than describing the code.

---

## License

This project is licensed under the **MIT License** — see the [LICENSE](https://github.com/negihimanshu015/NitroSense-Linux/blob/main/LICENSE) file for details.

Copyright (c) 2026 Negi Himanshu

---

## Acknowledgements

- The [`acpi_call`](https://github.com/mkottman/acpi_call) kernel module for making userspace ACPI calls possible
- The Tauri project for a lightweight, secure Rust + web stack
- Acer Nitro Linux users who documented WMI opcodes and ACPI interfaces across various forums and GitHub issues