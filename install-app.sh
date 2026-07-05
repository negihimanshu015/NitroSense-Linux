#!/bin/bash
# install-app.sh
# Installs NitroSense-Linux as a proper desktop application.
# Run from the project root: bash install-app.sh
# Does NOT require sudo — installs to the current user's local app directories.

set -e

BINARY_PATH="src-tauri/target/release/nitrosense-linux"
ICON_PATH="src-tauri/icons/icon.png"
INSTALL_BIN="$HOME/.local/bin/nitrosense-linux"
INSTALL_ICON="$HOME/.local/share/icons/hicolor/512x512/apps/nitrosense-linux.png"
INSTALL_DESKTOP="$HOME/.local/share/applications/nitrosense-linux.desktop"

# ── Sanity check ────────────────────────────────────────────────────────────
if [ ! -f "$BINARY_PATH" ]; then
  echo "ERROR: Release binary not found at $BINARY_PATH"
  echo "Build it first with:  npm run tauri build -- --no-bundle"
  exit 1
fi

# ── Create directories ───────────────────────────────────────────────────────
mkdir -p "$HOME/.local/bin"
mkdir -p "$(dirname "$INSTALL_ICON")"
mkdir -p "$(dirname "$INSTALL_DESKTOP")"

# ── Copy binary ──────────────────────────────────────────────────────────────
echo "Installing binary → $INSTALL_BIN"
cp "$BINARY_PATH" "$INSTALL_BIN"
chmod +x "$INSTALL_BIN"

# ── Copy icon ────────────────────────────────────────────────────────────────
if [ ! -f "$ICON_PATH" ]; then
  echo "WARNING: Icon not found at $ICON_PATH — skipping icon install."
  echo "         The app will launch without a taskbar/launcher icon."
  echo "         To fix: ensure src-tauri/icons/icon.png exists before running this script."
else
  echo "Installing icon   → $INSTALL_ICON"
  cp "$ICON_PATH" "$INSTALL_ICON"
fi

# ── Write .desktop file ──────────────────────────────────────────────────────
echo "Installing desktop entry → $INSTALL_DESKTOP"
cat > "$INSTALL_DESKTOP" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=NitroSense Linux
Comment=Fan, RGB and system monitoring for Acer Nitro laptops
Exec=$INSTALL_BIN
Icon=$INSTALL_ICON
Terminal=false
Categories=System;HardwareSettings;Utility;
Keywords=fan;cooling;rgb;nitro;acer;laptop;
StartupWMClass=nitrosense-linux
EOF

# ── Refresh desktop database ─────────────────────────────────────────────────
if command -v update-desktop-database &>/dev/null; then
  update-desktop-database "$HOME/.local/share/applications/"
fi

# Refresh icon cache if xdg-icon-resource is available
if command -v xdg-icon-resource &>/dev/null; then
  xdg-icon-resource forceupdate --theme hicolor 2>/dev/null || true
fi

echo ""
echo "✓ NitroSense-Linux installed successfully!"
echo ""
echo "You can now:"
echo "  • Launch it from your app launcher (search 'NitroSense')"
echo "  • Pin it to your taskbar/dock"
echo "  • Run it from a terminal:  nitrosense-linux"
echo ""
echo "NOTE: Make sure ~/.local/bin is in your PATH."
echo "      If it isn't, add this to ~/.bashrc or ~/.zshrc:"
echo '      export PATH="$HOME/.local/bin:$PATH"'
