#!/bin/bash
# Setup script to grant the user group-level write access to /proc/acpi/call without sudo.

if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with sudo:"
  echo "sudo ./install-permissions.sh"
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
    echo "  Fedora (RPM Fusion required):"
    echo "    sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-\$(rpm -E %fedora).noarch.rpm"
    echo "    sudo dnf install akmod-acpi_call"
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

# Get real user (avoid adding root to nitrosense group).
REAL_USER="${SUDO_USER:-$USER}"

echo "Creating 'nitrosense' group (if it doesn't already exist)..."
if ! getent group nitrosense > /dev/null 2>&1; then
  groupadd nitrosense
fi

echo "Adding '$REAL_USER' to the 'nitrosense' group..."
usermod -aG nitrosense "$REAL_USER"

echo "Installing udev rule for /proc/acpi/call..."

# Persist /proc/acpi/call permissions using systemd-tmpfiles (udev doesn't manage /proc).
ACPI_CONF="/etc/tmpfiles.d/acpi_call.conf"
if [ -f "$ACPI_CONF" ]; then
  echo "WARNING: $ACPI_CONF already exists. Skipping to avoid overwriting your configuration."
  echo "         To reinstall, delete it manually: sudo rm $ACPI_CONF"
else
  cat << 'EOF' > "$ACPI_CONF"
f /proc/acpi/call 0660 root nitrosense - -
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
  echo "      Applying permissions manually for this session..."
fi

# Load kernel modules and apply session permissions.
modprobe acer_wmi 2>/dev/null
modprobe acpi_call 2>/dev/null
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
