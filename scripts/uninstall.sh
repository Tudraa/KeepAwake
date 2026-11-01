#!/usr/bin/env bash
# Antigravity KeepAwake - Automated Local Uninstallation Script

set -euo pipefail

UUID="antigravity-keepawake@antigravity.org"
BIN_DIR="$HOME/.local/bin"
EXTENSION_DIR="$HOME/.local/share/gnome-shell/extensions/$UUID"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"

echo "[Antigravity KeepAwake] Uninstalling..."

# 1. Stop and disable systemd service
if systemctl --user is-active --quiet antigravity-keepawake.service 2>/dev/null; then
    echo "Stopping systemd --user service..."
    systemctl --user stop antigravity-keepawake.service || true
fi

if systemctl --user is-enabled --quiet antigravity-keepawake.service 2>/dev/null; then
    echo "Disabling systemd --user service..."
    systemctl --user disable antigravity-keepawake.service || true
fi

rm -f "$SYSTEMD_USER_DIR/antigravity-keepawake.service"
systemctl --user daemon-reload || true

# 2. Disable and remove GNOME Extension
if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "$UUID" 2>/dev/null || true
fi
rm -rf "$EXTENSION_DIR"

# 3. Remove binary
rm -f "$BIN_DIR/antigravity-keepawake"

echo "[Antigravity KeepAwake] Uninstallation complete."
