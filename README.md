# NitroSense Linux

> A native Linux replacement for Acer's NitroSense software — fan control, keyboard RGB, and live system telemetry for Acer Nitro laptops.

---

## What Is This?

**NitroSense Linux** is an open-source desktop application that brings the functionality of Acer's Windows-only NitroSense utility to Linux. It is built with [Tauri 2](https://tauri.app/) (Rust backend + HTML/CSS/JS frontend) and communicates directly with the laptop's embedded controller via the `acpi_call` kernel module and the Acer WMID WMI interface — the same hardware pathway used by the official Windows software.

---

## Features

| Category | What You Can Do |
|---|---|
| **Fan Control** | Switch between Auto, Max, and Custom fan modes |
| **Custom Fan Speed** | Set independent CPU and GPU fan duty cycles (0–100%) |
| **Thermal Telemetry** | Live CPU and GPU temperatures + fan RPM readings |
| **System Monitoring** | Real-time CPU usage, GPU utilization (via `nvidia-smi`), and RAM usage |
| **Thermal History** | Rolling 60-second CPU/GPU temperature chart |
| **Keyboard RGB** | Per-zone (4-zone) RGB color control |
| **RGB Lighting Effects** | Static, Breathing, Color Wave, and Neon Loop modes |
| **RGB Modifiers** | Adjustable brightness and animation speed |
| **Dependency Check** | On launch, verifies `acpi_call` and the WMID WMI path are working before enabling controls |

---

## Purpose

This project reverse-engineers the ACPI/WMI call protocol used by the hardware and exposes it through a polished, native Linux GUI. The goal is to give Acer Nitro Linux users the same level of hardware control they would have on Windows, without needing to reboot, use a VM, or run shell scripts manually.

---

## Tested Hardware & Kernel

| Field | Value |
|---|---|
| **Laptop Model** | Acer Nitro AN515-57 |
| **Manufacturer** | Acer |
| **Linux Kernel** | `7.0.12-201.fc44.x86_64` |
| **Distribution** | Fedora 44 Workstation |

> [!NOTE]
> The application is primarily developed and tested on the hardware listed above. Other Acer Nitro/Aspire models that use the same `\_SB.PC00.WMID.WMBH` ACPI interface may work, but have **not** been tested. See the [Compatibility](#compatibility) section.

---

## Compatibility

### Likely Compatible
- Other **Acer Nitro 5 / Nitro 7** variants that share the `WMBH` WMI interface (e.g. AN515-55, AN515-58, AN517 series)
- Distributions with `acpi_call` and `acer_wmi` kernel modules available (Fedora, Ubuntu, Arch, openSUSE, Debian)

### May Not Work
- Acer models that use a different WMI path (the app checks for this on startup and will warn you)
- Laptops where the GPU reports 0°C via ACPI while in Optimus D3cold — the app has a fallback to `hwmon` and `nvidia-smi`, but results may vary

### Not Supported
- Non-Acer hardware
- Any Windows environment

---

## Warnings

> [!CAUTION]
> **Fan control writes directly to the embedded controller via ACPI.** Incorrect values can cause thermal issues. The application clamps all fan speed values to 0–100% and uses the same opcodes as the official Acer firmware, but use at your own risk.

> [!WARNING]
> **Setting fans to 0% in Custom mode is possible.** This tells the EC to target 0% duty cycle. Do not do this under sustained load. The EC's safety cutoffs are firmware-dependent and may not protect against all scenarios.

> [!WARNING]
> **`/proc/acpi/call` is a privileged interface.** The `install-permissions.sh` script grants group-level access via a dedicated `nitrosense` group so the app does not need to run as root. Running hardware control software as root is strongly discouraged.


---

## Dependencies

The following must be in place before the app's hardware controls will function:

| Dependency | Purpose |
|---|---|
| `acpi_call` kernel module | Allows userspace to send ACPI method calls (fan + RGB control) |
| `acer_wmi` kernel module | Exposes the Acer WMID WMI device path |
| `nvidia-smi` *(optional)* | GPU utilization % and temperature fallback for proprietary NVIDIA drivers |
| `WebKit2GTK` | Tauri's rendering backend (usually installed as a system dependency) |

---

## Installation

### Step 1 — Install `acpi_call`

The `acpi_call` kernel module is required. Install it for your distribution:

**Fedora** (via RPM Fusion):
```bash
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install akmod-acpi_call
```

**Ubuntu / Debian:**
```bash
sudo apt install acpi-call-dkms
```

**Arch Linux** (AUR only):
```bash
yay -S acpi_call-dkms
# or with paru:
paru -S acpi_call-dkms
```

**openSUSE:**
```bash
sudo zypper addrepo https://download.opensuse.org/repositories/hardware/openSUSE_Tumbleweed/ hardware
sudo zypper install acpi_call-kmp-default
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

Tauri requires Rust and its system dependencies. If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install Node.js (for the Tauri CLI):
```bash
# Fedora
sudo dnf install nodejs

# Ubuntu / Debian
sudo apt install nodejs npm

# Arch
sudo pacman -S nodejs npm
```

Install Tauri system dependencies (WebKit2GTK etc.):

**Fedora:**
```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel
```

**Ubuntu / Debian:**
```bash
sudo apt install libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

**Arch:**
```bash
sudo pacman -S webkit2gtk-4.1 openssl libayatana-appindicator librsvg
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

To add a launcher entry, icon, and make the app launchable from your app grid:

```bash
bash install-app.sh
```

This installs to `~/.local/` and **does not require `sudo`**. Make sure `~/.local/bin` is in your `PATH`:

```bash
# Add to ~/.bashrc or ~/.zshrc if needed:
export PATH="$HOME/.local/bin:$PATH"
```

You can then launch the app from your application menu by searching **NitroSense**, or run it from a terminal:

```bash
nitrosense-linux
```

---

### Running Without Installing

You can run the binary directly after building:

```bash
./src-tauri/target/release/nitrosense-linux
```

---

## Development

To run the app in development mode with hot-reload:

```bash
npm install
npm run tauri dev
```

### Project Structure

```
nitrosense-linux/
├── src/                        # Frontend (HTML, CSS, JavaScript)
│   ├── index.html              # App UI
│   ├── main.js                 # Frontend logic & Tauri IPC calls
│   ├── styles.css              # Glassmorphic UI styles
│   └── fonts.css               # Font definitions
├── src-tauri/                  # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs             # Tauri commands & app state
│   │   └── wmi.rs              # ACPI/WMI hardware interface
│   ├── Cargo.toml              # Rust dependencies
│   └── tauri.conf.json         # Tauri app configuration
├── install-permissions.sh      # One-time sudo setup for /proc/acpi/call
└── install-app.sh              # Desktop launcher installer (no sudo)
```

---

## How to Contribute

Contributions are welcome! Here's how to get started:

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
- Non-systemd distro compatibility (Void Linux, Gentoo/OpenRC)

### Code Style

- Rust code should follow standard `rustfmt` formatting (`cargo fmt`)
- JavaScript follows vanilla ES module style — no frameworks, no build step for the frontend
- Comments are important, especially in `wmi.rs` where protocol details are non-obvious — please keep them accurate

---

## License

This project is licensed under the **MIT License** — see the [LICENSE](https://github.com/negihimanshu015/NitroSense-Linux/blob/main/LICENSE) file for details.

Copyright (c) 2026 Negi Himanshu

---

## Acknowledgements

- The [`acpi_call`](https://github.com/mkottman/acpi_call) kernel module for making userspace ACPI calls possible
- The Tauri project for a lightweight, secure Rust + web stack
- Acer Nitro Linux users who documented WMI opcodes and ACPI interfaces across various forums and GitHub issues
