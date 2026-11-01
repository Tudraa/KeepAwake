# Antigravity KeepAwake (Rust Edition)

[![Platform](https://img.shields.io/badge/Platform-Ubuntu%2022.04%20LTS-E95420?logo=ubuntu&logoColor=white)](https://ubuntu.com/)
[![Desktop](https://img.shields.io/badge/Desktop-GNOME%20Shell%2042-4a154b?logo=gnome&logoColor=white)](https://www.gnome.org/)
[![Rust](https://img.shields.io/badge/Rust-2021%20Edition-black?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Privileges](https://img.shields.io/badge/Privileges-Zero%20Sudo%20Required-success)](#zero-sudo-security-model)

An intelligent closed-lid keep-awake daemon and GNOME Shell top bar indicator designed for developers running background AI coding workflows on Linux laptops.

---

## Table of Contents
- [The Problem & The Solution](#the-problem--the-solution)
- [System Specifications & Compatibility](#system-specifications--compatibility)
- [Key Features](#key-features)
  - [Operating Modes](#operating-modes)
  - [Smart 3-Class Process Freezing](#smart-3-class-process-freezing)
  - [Hardware Safeguards & Thermals](#hardware-safeguards--thermals)
- [Architecture Overview](#architecture-overview)
- [Prerequisites & Dependencies](#prerequisites--dependencies)
- [Installation Guide](#installation-guide)
  - [Quick Install (Automated)](#1-quick-install-automated)
  - [Activating the GNOME Shell Extension](#2-activating-the-gnome-shell-extension)
- [Usage & Interface](#usage--interface)
  - [Top Bar Indicator](#top-bar-indicator)
  - [Popover Menu](#popover-menu)
  - [CLI & D-Bus Control](#cli--d-bus-control)
- [Troubleshooting & Verification](#troubleshooting--verification)
- [Uninstallation](#uninstallation)
- [Repository Structure](#repository-structure)

---

## The Problem & The Solution

When running autonomous AI agent tasks (such as Antigravity coding loops, large compilations, or multi-step test suites), jobs routinely take 15 to 60+ minutes. 

On Linux laptops:
1. **Closing the laptop lid** immediately triggers `systemd-logind` to suspend the computer, cutting off internet, D-Bus, and running compiler processes.
2. **Disabling sleep completely** (via generic Caffeine tools) keeps the laptop running indefinitely, draining your battery and risking thermal runaways if the laptop is slipped into a bag.

**Antigravity KeepAwake** provides a surgical, automated middle ground:
- **Active Task Protection**: Acquires a `systemd-logind` inhibitor lock (`handle-lid-switch`) only while Antigravity is actively executing tasks with the lid closed.
- **Resource Freezing**: Temporarily suspends background non-essential applications (`SIGSTOP`) while the lid is closed to keep thermals and power consumption minimal.
- **Auto-Sleep on Completion**: Automatically releases the inhibitor and initiates deep system suspend (`systemctl suspend`) **2 minutes** after Antigravity finishes, preventing battery drain.
- **Instant Thawing**: When you open the laptop lid, all frozen applications are restored (`SIGCONT`) in **under 20 milliseconds**.

---

## System Specifications & Compatibility

| Specification | Target / Supported Environment |
| :--- | :--- |
| **Operating System** | **Ubuntu 22.04 LTS (Jammy Jellyfish)** *(Primary target)*<br>Compatible with Ubuntu 23.10 and Ubuntu 24.04 LTS |
| **Desktop Environment** | **GNOME Shell 42.x** (GNOME 42.0 – 42.9)<br>Compatible with GNOME 43–45 with minor metadata adjustments |
| **Display Server** | **Wayland** & **X11** (Fully supported) |
| **CPU Architecture** | `x86_64` / `amd64` (AMD Ryzen, Intel Core) |
| **Privileges** | **100% Unprivileged (User Space)** — **No `sudo` required** |
| **Rust Version** | Rust 1.75+ (2021 Edition) |

---

## Key Features

### Operating Modes

The extension provides two distinct modes selectable directly from the GNOME Shell top bar menu:

1. **`Only Antigravity Always Awake` (Mode 0 - Recommended)**
   - Monitors the Antigravity activity transcript in real-time using inotify.
   - Activates keep-awake and Class 2 process freezing only while tasks are executing with the lid closed.
   - Initiates an automated 2-minute countdown timer once Antigravity finishes. If the lid remains closed when the timer elapses, the system safely triggers `systemctl suspend`.
   - If the lid is opened at any point, auto-sleep is canceled immediately.

2. **`All Apps Awake` (Mode 1)**
   - Functions like traditional Keep-Awake tools.
   - Prevents system sleep regardless of Antigravity activity.
   - Does not freeze background applications and disables the 2-minute auto-sleep timer.

---

### Smart 3-Class Process Freezing

To keep your laptop cool and quiet with the lid closed, the daemon categorizes running processes into three distinct classes:

- **Class 0 (Golden System Whitelist)**:
  - Systemd, Wayland compositors, GNOME Shell, PipeWire, PulseAudio, D-Bus, and display managers are immune.
- **Class 1 (Antigravity Tree)**:
  - The Antigravity IDE and all its descendants (Rust compiler `rustc`, `cargo`, Node.js, Python, Git, test runners) are recursively discovered and fully immune.
- **Class 2 (External Heavy Apps)**:
  - Non-essential background apps (e.g., Google Chrome, Discord, Spotify, Slack) are cleanly frozen with `SIGSTOP` while the lid is closed, and thawed with `SIGCONT` in `<20ms` upon opening the lid.

---

### Hardware Safeguards & Thermals

- **Strict Open-Lid Safety**: The daemon will **never** trigger system suspend while the laptop lid is open.
- **Thermal Guardrails**:
  - **88°C**: Throttling warning threshold.
  - **95°C**: Emergency closed-lid cutoff (safely suspends system to prevent heat buildup inside laptop chassis).
- **Battery Protection**:
  - Automatically aborts keep-awake and suspends if battery capacity drops below **12%**.
- **Crash Recovery Watchdog**:
  - A signal handler ensures that if the daemon ever receives `SIGTERM`, `SIGINT`, or encounters an unexpected crash, all frozen Class 2 applications are immediately thawed before termination.

---

## Architecture Overview

Antigravity KeepAwake consists of two decoupled components communicating via Session D-Bus:

```
┌────────────────────────────────────────────────────────┐
│               GNOME Shell Top Bar                      │
│   (GJS Extension: antigravity-keepawake@antigravity)   │
│   • Vector Coffee Cup Status Icon (Active/Idle/Off)    │
│   • Mode Selector & Real-Time Popover Telemetry        │
└───────────────────────────▲────────────────────────────┘
                            │ Session D-Bus (org.antigravity.KeepAwake)
┌───────────────────────────▼────────────────────────────┐
│         Rust Daemon (antigravity-keepawake)            │
│   • systemd-logind Inhibit Lock ("handle-lid-switch")  │
│   • Activity Sentinel (inotify transcript watcher)     │
│   • 3-Class Process Freezer (SIGSTOP / SIGCONT)        │
│   • Thermal & Battery Monitor (/sys/class/thermal)     │
│   • Systemd User Service (systemctl --user)            │
└────────────────────────────────────────────────────────┘
```

---

## Prerequisites & Dependencies

Before installation, ensure you have the required build packages and Rust toolchain installed:

```bash
# 1. Update package lists and install build dependencies
sudo apt update
sudo apt install -y build-essential libglib2.0-bin curl git

# 2. Install Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

---

## Installation Guide

### 1. Quick Install (Automated)

Clone the repository and run the automated installer:

```bash
git clone https://github.com/martian/antigravity-keepawake.git
cd antigravity-keepawake

# Run installer (Zero sudo required!)
./scripts/install.sh
```

The installer script automatically:
1. Compiles the optimized Rust release binary (`cargo build --release`).
2. Installs the binary to `~/.local/bin/antigravity-keepawake`.
3. Installs the GNOME Shell extension and compiles its GSettings schema.
4. Registers and starts the user systemd service (`antigravity-keepawake.service`).

---

### 2. Activating the GNOME Shell Extension

After running `./scripts/install.sh`, enable the extension in GNOME:

#### On Ubuntu Wayland (Default in 22.04):
GNOME Shell caches JavaScript extensions in memory. You must **Log Out and Log Back In** to initialize the extension:
1. Save your open work.
2. Log out of your Ubuntu session and log back in.
3. Enable the extension via terminal:
   ```bash
   gnome-extensions enable antigravity-keepawake@antigravity.org
   ```
   *(Or toggle it on using the **Extension Manager** application).*

#### On Ubuntu X11:
Restart GNOME Shell directly without logging out:
1. Press `Alt + F2`.
2. Type `r` and press `Enter`.
3. Run:
   ```bash
   gnome-extensions enable antigravity-keepawake@antigravity.org
   ```

---

## Usage & Interface

### Top Bar Indicator

- ☕ **Gray Cup with Yellow Steam** (`coffee-antigravity.svg`): Displayed in `Only Antigravity Always Awake` mode (steam plumes glow yellow for agent activity, while the cup body remains calm neutral gray).
- ☕ **Full Golden Yellow Cup** (`coffee-active.svg`): Displayed in `All Apps Awake` mode (entire cup, handle, saucer, and steam glow warm yellow).
- ☕ **Slashed Dark Cup** (`coffee-disabled.svg`): Master switch disabled or daemon offline.

### Popover Menu

Clicking the coffee cup icon opens the popover menu:
- **Master Switch**: Toggle keep-awake system on or off.
- **Operating Modes**:
  - `Only Antigravity Always Awake` (with native GNOME radio bullet)
  - `All Apps Awake` (with native GNOME radio bullet)
- **Live System Telemetry**:
  - Lid state (`Closed` / `Open`)
  - Frozen Class 2 applications / threads counter
  - CPU Temperature (°C)
  - Battery capacity (%)

---

### CLI & D-Bus Control

You can inspect or control the daemon using standard Linux tools:

```bash
# Check D-Bus responsiveness
busctl --user call org.antigravity.KeepAwake /org/antigravity/KeepAwake org.antigravity.KeepAwake Ping

# Query live status JSON
busctl --user call org.antigravity.KeepAwake /org/antigravity/KeepAwake org.antigravity.KeepAwake GetStatus

# Change operating mode (0 = Only Antigravity, 1 = All Apps)
busctl --user call org.antigravity.KeepAwake /org/antigravity/KeepAwake org.antigravity.KeepAwake SetMode u 0
```

---

## Troubleshooting & Verification

### Check Daemon Status
```bash
systemctl --user status antigravity-keepawake.service
```

### View Live Logs
```bash
journalctl --user -u antigravity-keepawake.service -f
```

### Verify Lid Inhibitor Lock
To verify that `systemd-logind` recognizes the active lid inhibitor:
```bash
systemd-inhibit --list
```
Look for an entry with:
- **Who**: `Antigravity KeepAwake`
- **What**: `handle-lid-switch`
- **Mode**: `block`

### Run Test Suite
To run the automated Rust test suite (31 unit and integration tests):
```bash
cargo test
```

---

## Uninstallation

To cleanly remove the daemon, extension, and systemd service:

```bash
./scripts/uninstall.sh
```

This will:
- Stop and disable the `systemd --user` service.
- Delete the installed binary from `~/.local/bin/`.
- Remove the GNOME extension from `~/.local/share/gnome-shell/extensions/`.
- Clean up systemd user unit definitions.

---

## Repository Structure

```text
.
├── Cargo.toml                 # Rust manifest and dependencies
├── Cargo.lock                 # Pinned dependencies
├── README.md                  # Project documentation
├── .gitignore                 # Git ignore configuration
├── src/                       # Native Rust daemon
│   ├── lib.rs                 # Library exports & types
│   ├── main.rs                # Daemon entrypoint & D-Bus service loop
│   ├── service.rs             # State coordinator & D-Bus server
│   ├── sleep_inhibitor.rs     # systemd-logind D-Bus inhibitor
│   ├── activity_sentinel.rs   # inotify transcript file watcher & countdown
│   ├── process_tree.rs        # 3-Class process classifier & tree-walk
│   ├── freezer.rs             # SIGSTOP/SIGCONT process freeze & crash watchdog
│   └── thermal_guard.rs       # sysfs thermal & UPower battery monitor
├── extension/                 # GNOME Shell 42 Extension (GJS)
│   ├── extension.js           # Top bar UI, D-Bus client, popover menu
│   ├── metadata.json          # Extension manifest & GNOME version specs
│   ├── stylesheet.css         # UI styling
│   └── icons/                 # Custom SVG status icons
│       ├── coffee-active.svg      # All Apps Awake (all yellow)
│       ├── coffee-antigravity.svg # Only Antigravity (yellow steam, gray cup)
│       ├── coffee-idle.svg        # Idle cup (no steam)
│       └── coffee-disabled.svg    # Disabled (red slash)
├── schemas/                   # GSettings schema configuration
│   └── org.gnome.shell.extensions.antigravity-keepawake.gschema.xml
├── scripts/                   # User-space scripts
│   ├── install.sh             # Unprivileged build & installation script
│   ├── uninstall.sh           # Clean teardown script
│   └── antigravity-keepawake.service # Systemd user unit
└── tests/
    └── integration_tests.rs   # Integration tests & constraint checks
```

---

## License

Distributed under the MIT License. See `LICENSE` for details.
