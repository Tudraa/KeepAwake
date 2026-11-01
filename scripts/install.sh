#!/usr/bin/env bash
# Antigravity KeepAwake - Automated Local Installation Script
# 100% Unprivileged (User space only, no sudo required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
UUID="antigravity-keepawake@antigravity.org"
DRY_RUN=false

for arg in "$@"; do
    case "$arg" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
    esac
done

echo "[Antigravity KeepAwake] Installing local components..."
echo "Root Directory: $ROOT_DIR"

# 1. Ensure Rust toolchain is available
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

command -v cargo >/dev/null 2>&1 || {
    echo "Error: cargo not found. Please install Rust via rustup." >&2
    exit 1
}

# 2. Compile release binary
echo "Compiling Rust daemon (release mode)..."
if [ "$DRY_RUN" = false ]; then
    (cd "$ROOT_DIR" && cargo build --release)
else
    echo "   [dry-run] cargo build --release"
fi

# 3. Target Installation Directories
BIN_DIR="$HOME/.local/bin"
EXTENSION_DIR="$HOME/.local/share/gnome-shell/extensions/$UUID"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"

mkdir -p "$BIN_DIR" "$EXTENSION_DIR/schemas" "$EXTENSION_DIR/icons" "$SYSTEMD_USER_DIR"

# 4. Install native binary
echo "Installing daemon binary to $BIN_DIR/antigravity-keepawake..."
if [ "$DRY_RUN" = false ]; then
    cp -f "$ROOT_DIR/target/release/antigravity-keepawake" "$BIN_DIR/antigravity-keepawake"
    chmod +x "$BIN_DIR/antigravity-keepawake"
else
    echo "   [dry-run] cp target/release/antigravity-keepawake -> $BIN_DIR"
fi

# 5. Install GNOME Shell Extension
echo "Installing GNOME Shell Extension to $EXTENSION_DIR..."
if [ "$DRY_RUN" = false ]; then
    cp -f "$ROOT_DIR/extension/metadata.json" "$EXTENSION_DIR/"
    cp -f "$ROOT_DIR/extension/extension.js" "$EXTENSION_DIR/"
    cp -f "$ROOT_DIR/extension/stylesheet.css" "$EXTENSION_DIR/"
    cp -rf "$ROOT_DIR/extension/icons/"* "$EXTENSION_DIR/icons/"

    # Compile GSettings schema inside extension directory
    cp -f "$ROOT_DIR/schemas/org.gnome.shell.extensions.antigravity-keepawake.gschema.xml" "$EXTENSION_DIR/schemas/"
    glib-compile-schemas "$EXTENSION_DIR/schemas/"
else
    echo "   [dry-run] cp extension files -> $EXTENSION_DIR"
fi

# 6. Install and enable systemd user service
echo "Installing systemd --user service..."
if [ "$DRY_RUN" = false ]; then
    cp -f "$ROOT_DIR/scripts/antigravity-keepawake.service" "$SYSTEMD_USER_DIR/"
    systemctl --user daemon-reload
    systemctl --user enable antigravity-keepawake.service
    systemctl --user restart antigravity-keepawake.service || echo "Warning: systemctl restart returned non-zero (daemon may need active session)"
else
    echo "   [dry-run] cp antigravity-keepawake.service -> $SYSTEMD_USER_DIR"
fi

# 7. Enable GNOME Extension if gnome-extensions CLI exists
if command -v gnome-extensions >/dev/null 2>&1; then
    echo "Enabling GNOME extension $UUID..."
    if [ "$DRY_RUN" = false ]; then
        gnome-extensions enable "$UUID" 2>/dev/null || echo "Info: Extension enabled or queued for next Shell reload."
    fi
fi

echo "[Antigravity KeepAwake] Installation complete."
echo "   Daemon status: systemctl --user status antigravity-keepawake.service"
echo "   Logs: journalctl --user -u antigravity-keepawake -f"
