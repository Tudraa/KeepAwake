# 🚀 Antigravity KeepAwake (Rust Edition)

A high-performance, ultra-lightweight Ubuntu Top Bar (GNOME Shell 42) extension and Rust daemon that keeps your laptop awake while Antigravity runs tasks with the lid closed.

---

## 🎯 Key Features
- **Lid-Closed Execution**: Prevents `systemd-logind` from suspending when the laptop lid is closed while Antigravity is active using D-Bus `Inhibit` locks.
- **Top Bar Integration**: Toggle switch and real-time status telemetry (⚡ Running, 💤 Idle, 🛑 Suspended) right beside the Ubuntu clock.
- **Auto-Sleep on Completion**: Automatically releases the inhibitor lock after a configurable 2-minute grace period and transitions cleanly to sleep (`systemctl suspend`).
- **Smart Resource Throttling (3-Class Taxonomy)**:
  - **Class 0 (Golden Whitelist)**: Absolute immunity for GNOME Shell, Wayland, PipeWire, D-Bus, and systemd.
  - **Class 1 (Antigravity Tree)**: Recursive tree-walk protection for the IDE and all child compilers, test runners, and tooling.
  - **Class 2 (External Heavy Apps)**: Freezes non-essential apps (e.g., Chrome, Spotify, Discord) via `SIGSTOP` while lid is closed, and thaws them via `SIGCONT` on lid open.
- **Hardware Safeguards**:
  - Battery floor cutoff at $\le$ 12% to prevent battery drain.
  - Thermal safeguards: Tier 1 (75°C `nice` +10), Tier 2 (80°C emergency suspend).
- **Zero Polling & Resource Constraints**: Built with pure async event loops (`notify` inotify + `zbus` D-Bus signals), guaranteeing 0.0% CPU when idle and < 8 MB RSS memory footprint.

---

## 📁 Project Structure
```text
.
├── Cargo.toml                 # Rust dependencies & binary targets
├── src/
│   ├── lib.rs                 # Library exports & module definitions
│   ├── main.rs                # Standalone daemon entrypoint & D-Bus service loop
│   ├── process_tree.rs        # 3-Class process taxonomy & recursive tree-walk
│   ├── freezer.rs             # SIGSTOP/SIGCONT process freeze & crash watchdog
│   ├── sleep_inhibitor.rs     # systemd-logind Inhibit lock via zbus
│   ├── activity_sentinel.rs   # inotify transcript watcher & grace period timer
│   ├── thermal_guard.rs       # sysfs thermal & UPower/sysfs battery monitor
│   └── service.rs             # Unified coordinator & org.antigravity.KeepAwake D-Bus service
├── extension/                 # GNOME Shell 42 Extension (GJS)
│   ├── extension.js           # Top bar panel indicator & popover menu
│   ├── metadata.json          # GNOME Shell extension metadata
│   └── stylesheet.css         # Styling for top bar indicators
├── schemas/                   # GSettings configuration schema
├── scripts/
│   ├── install.sh             # User-mode installation script (no sudo required)
│   ├── uninstall.sh           # Clean uninstallation script
│   └── antigravity-keepawake.service # systemd user service unit
├── tests/
│   └── integration_tests.rs   # End-to-end integration tests & constraint verifications
├── CONSTRAINTS.md             # Quality & safety contracts
├── PRD.md                     # Product requirements document
└── tasks/                     # Plan and tracking tasks
```

---

## 🛠️ Build & Installation

### Requirements
- Ubuntu 22.04 LTS (GNOME Shell 42)
- Rust toolchain (`cargo`, `rustc` 1.80+)
- `glib2.0` (`glib-compile-schemas`)

### 1. Build and Run Tests
```bash
cargo test
cargo clippy -- -D warnings
```

### 2. Install (No Sudo Required)
```bash
./scripts/install.sh
```

### 3. Check Service Status
```bash
systemctl --user status antigravity-keepawake.service
journalctl --user -u antigravity-keepawake -f
```

### 4. Uninstall
```bash
./scripts/uninstall.sh
```
