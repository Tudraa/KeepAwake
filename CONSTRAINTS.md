# Constraints: Antigravity KeepAwake

Last reviewed: 2026-11-01 by AI Engineering & Systems Pair Programming
Canonical source: file:///home/martian/antigravity-keepawake/CONSTRAINTS.md

## Floor (Always Enforced, No Setup Required)

- No new suppression comments: `#[allow(...)]`, `/* eslint-disable */`, `@ts-ignore`.
- No unimplemented stubs: `todo!()`, `unimplemented!()`, empty `catch {}`.
- No skipped or deleted tests without justification in commit message and review.
- No secrets or credentials in source code.
- 100% Unprivileged Execution: All scripts and daemons must run under `systemd --user` (UID 1000). Zero `sudo` or root requirements.
- This file does not get weakened to make a change pass.

---

## Enforced Technical & Architectural Constraints

| Dimension | Rule & Threshold | Checked By | Runs At |
| :--- | :--- | :--- | :--- |
| **CPU Idle Footprint** | **0.0% CPU** consumption when laptop lid is open; strictly event-driven. | `cargo test -- test_zero_cpu_at_idle` | Task end, CI |
| **Zero-Polling Loop** | **Zero `sleep()` polling loops**. Banned pattern: `std::thread::sleep` in loop. All triggers must use D-Bus signals / inotify events. | `cargo test -- test_no_sleep_polling_loops` | Every edit |
| **Memory Footprint** | Daemon Resident Set Size (RSS) **< 8 MB** (Rust native ELF binary). | `cargo test -- test_memory_budget` | Task end |
| **Desktop Safety Immunity (Class 0)** | **100% Whitelist Protection**: Sinyal `SIGSTOP` dilarang keras dikirim ke `gnome-shell`, `xwayland`, `mutter`, `pipewire`, `wireplumber`, `dbus-daemon`, `systemd`. | `cargo test -- test_class0_golden_whitelist_immune` | Every edit |
| **Antigravity Family Immunity (Class 1)** | **100% Tree Immunity**: Induk Antigravity (`/opt/antigravity/antigravity-ide`), language server, dan seluruh sub-proses turunannya (`git`, `rtk`, `cargo`, `node`, `bash`) dilarang dikirimi `SIGSTOP`. | `cargo test -- test_class1_antigravity_tree_immune` | Every edit |
| **Signal Latency: Freeze** | Pengiriman `SIGSTOP` ke proses Kelas 2 **< 50 ms** pasca sinyal `LidClosed=true`. | `cargo test -- test_freeze_latency` | Task end |
| **Signal Latency: Thaw** | **Pre-Wakeup Thaw**: Pengiriman `SIGCONT` **< 20 ms** pasca sinyal `LidClosed=false` sebelum backlight layar menyala. | `cargo test -- test_thaw_latency` | Task end |
| **Crash Recovery Guarantee** | **100% Recovery**: Jika daemon terminasi atau panic (`SIGTERM`, `SIGINT`), seluruh proses yang berstatus `SIGSTOP` wajib dikirimi `SIGCONT` seketika. | `cargo test -- test_crash_recovery_watchdog` | Every edit |
| **Battery Safety Floor** | Baterai **$\le$ 12%** wajib melepaskan inhibitor lock dan memicu suspend darurat secara otonom. | `cargo test -- test_battery_floor_cutoff` | Every edit |
| **Thermal Escalation Floor** | Suhu **$\ge$ 75°C** memicu deprioritisasi (`nice` +10), dan **$\ge$ 80°C** wajib memicu Emergency Deep Sleep. | `cargo test -- test_thermal_escalation_cutoff` | Every edit |
| **Code Hygiene & Lint** | Zero compiler warnings & zero clippy errors. | `cargo clippy -- -D warnings` | Every edit |
| **Formatting** | Rust standard code format. | `cargo fmt --check` | Every edit |

---

## Measured, Not Yet Enforced

| Metric | Target | Direction |
| :--- | :--- | :--- |
| Extension GJS Init Latency | < 150 ms | Must not grow |
| D-Bus IPC Round-Trip Latency | < 1 ms | Must not grow |

---

## Exceptions

*No exceptions granted. Any proposed change requires explicit review.*
