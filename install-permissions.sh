#!/bin/bash
# Setup script to grant the user group-level write access to /proc/acpi/call without sudo.

set -euo pipefail

if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with sudo:"
  echo "sudo ./install-permissions.sh"
  exit 1
fi

# Get real user (avoid adding root to nitrosense group).
REAL_USER="${SUDO_USER:-$USER}"

if [ "$REAL_USER" = "root" ]; then
  echo "ERROR: Running directly as root without SUDO_USER."
  echo "       Please run from a normal user shell with: sudo ./install-permissions.sh"
  exit 1
fi

# Pre-flight check: Verify acpi_call is installed.
if ! modinfo acpi_call &>/dev/null 2>&1; then
  echo ""
  echo "ERROR: The 'acpi_call' kernel module is not installed."
  echo "       NitroSense-Linux cannot control fans without it."
  echo ""
  echo "Install it using your distro's package manager:"
  echo ""
  if command -v dnf &>/dev/null; then
    echo "  Fedora (COPR required):"
    echo "    sudo dnf copr enable rhea/acpi_call"
    echo "    sudo dnf install acpi_call-dkms"
  elif command -v apt &>/dev/null; then
    echo "  Ubuntu / Debian:"
    echo "    sudo apt install acpi-call-dkms"
  elif command -v pacman &>/dev/null; then
    echo "  Arch Linux:"
    echo "    sudo pacman -S acpi_call        # community/extra repo"
    echo "    # or via AUR: yay -S acpi_call-dkms"
  elif command -v zypper &>/dev/null; then
    echo "  openSUSE:"
    echo "    sudo zypper addrepo https://download.opensuse.org/repositories/hardware/openSUSE_Tumbleweed/ hardware"
    echo "    sudo zypper install acpi_call-kmp-default"
  else
    echo "  Your distro was not auto-detected. Please install 'acpi_call' manually:"
    echo "  https://github.com/nflx/acpi_call"
  fi
  echo ""
  echo "After installing, re-run:  sudo ./install-permissions.sh"
  exit 1
fi

echo "Creating 'nitrosense' group (if it doesn't already exist)..."
if ! getent group nitrosense > /dev/null 2>&1; then
  groupadd nitrosense
fi

echo "Adding '$REAL_USER' to the 'nitrosense' group..."
usermod -aG nitrosense "$REAL_USER"

# Load kernel modules first so /proc/acpi/call is created by the kernel.
echo "Loading kernel modules..."
modprobe acpi_call || true
modprobe acer_wmi || true

if [ ! -f /proc/acpi/call ]; then
  echo ""
  echo "=========================================================================="
  echo "ERROR: /proc/acpi/call was NOT created after loading 'acpi_call'."
  echo "=========================================================================="

  IS_SB=false
  if command -v mokutil &>/dev/null && mokutil --sb-state 2>/dev/null | grep -qi "enabled"; then
    IS_SB=true
  elif [ -f /sys/kernel/security/lockdown ] && grep -vq "\[none\]" /sys/kernel/security/lockdown 2>/dev/null; then
    IS_SB=true
  fi

  if [ "$IS_SB" = true ]; then
    echo "REASON: Secure Boot is ENABLED on your system."
    echo "        The Linux kernel blocked loading the unsigned 'acpi_call' module."
    echo ""
    echo "To fix this, choose ONE of the following options:"
    echo ""
    echo "  OPTION 1 (Recommended & Easiest): Disable Secure Boot in BIOS"
    echo "    1. Reboot your laptop and press F2 (or Del) to enter BIOS Setup."
    echo "    2. Under the 'Security' tab, set a Supervisor Password (if required)."
    echo "    3. Change 'Secure Boot' to 'Disabled'."
    echo "    4. Save & Exit, then boot into Linux."
    echo ""
    echo "  OPTION 2: Enroll DKMS MOK Key (Keep Secure Boot ON)"
    echo "    1. Run:  sudo mokutil --import /var/lib/dkms/mok.pub"
    echo "             (Set a temporary password when prompted, e.g. 12345678)"
    echo "    2. Reboot your laptop."
    echo "    3. On boot, the blue Shim MOK Manager screen will appear."
    echo "    4. Select 'Enroll MOK' -> 'Continue' -> 'Yes' -> Enter your password -> Reboot."
    echo ""
    echo "After completing Option 1 or Option 2, re-run:  sudo ./install-permissions.sh"
    echo "=========================================================================="
  else
    echo "Please check 'dmesg | tail -n 20' to see why the acpi_call module failed to initialize."
    echo "=========================================================================="
  fi
  exit 1
fi

echo "Installing tmpfiles configuration for /proc/acpi/call..."

# Persist /proc/acpi/call permissions using systemd-tmpfiles (udev doesn't manage /proc).
# Note: 'z' is used so systemd-tmpfiles sets permissions on /proc/acpi/call when present without attempting to create it.
ACPI_CONF="/etc/tmpfiles.d/acpi_call.conf"
if [ -f "$ACPI_CONF" ]; then
  if grep -q "^f /proc/acpi/call" "$ACPI_CONF"; then
    echo "Updating existing $ACPI_CONF to use 'z' directive..."
    cat << 'EOF' > "$ACPI_CONF"
z /proc/acpi/call 0660 root nitrosense - -
EOF
  else
    echo "WARNING: $ACPI_CONF already exists. Skipping to avoid overwriting custom configuration."
    echo "         To reinstall, delete it manually: sudo rm $ACPI_CONF"
  fi
else
  cat << 'EOF' > "$ACPI_CONF"
z /proc/acpi/call 0660 root nitrosense - -
EOF
fi

echo "Configuring kernel modules to load on boot..."
MODULES_CONF="/etc/modules-load.d/nitrosense.conf"
if [ -f "$MODULES_CONF" ]; then
  echo "WARNING: $MODULES_CONF already exists. Skipping to avoid overwriting your configuration."
  echo "         To reinstall, delete it manually: sudo rm $MODULES_CONF"
else
  cat << 'EOF' > "$MODULES_CONF"
acpi_call
acer_wmi
EOF
fi

# Apply tmpfiles rule immediately or fallback if on a non-systemd distro.
if command -v systemd-tmpfiles &>/dev/null; then
  systemd-tmpfiles --create /etc/tmpfiles.d/acpi_call.conf
else
  echo "NOTE: 'systemd-tmpfiles' not found (non-systemd distro)."
  echo "      /proc/acpi/call permissions will be applied on next boot."
fi

# Apply session permissions manually
if [ -f /proc/acpi/call ]; then
  chown root:nitrosense /proc/acpi/call
  chmod 660 /proc/acpi/call
fi

echo ""
echo "Success! The acpi_call permissions have been configured."
echo ""
echo "NOTE: Current terminal will not see group membership changes until a new login session."
echo ""
echo "To start using NitroSense-Linux without sudo, do ONE of the following:"
echo "  • Log out and log back in  (recommended)"
echo "  • Run 'newgrp nitrosense'  (applies to current terminal only)"
echo ""
