# Implementation Plan: Antigravity KeepAwake (Rust Edition)

**Architecture Document:** [PRD.md](file:///home/martian/antigravity-keepawake/PRD.md)  
**Constraints Contract:** [CONSTRAINTS.md](file:///home/martian/antigravity-keepawake/CONSTRAINTS.md)  
**Target Platform:** Ubuntu 22.04.5 LTS / GNOME Shell 42 / Rust (Edition 2021)  

---

## 1. Overview
Antigravity KeepAwake adalah aplikasi tingkat sistem untuk Ubuntu 22.04 LTS yang menjaga laptop tetap bekerja saat layar ditutup selama Antigravity mengeksekusi coding tasks. Sistem terdiri dari dua komponen inti:
1. **Rust Background Daemon (`systemd --user`)**: Binary ELF native ultra-ringan (memory < 8 MB RSS, CPU 0.0% idle) yang mengelola kunci `systemd-inhibit` via `zbus`, mengamati aktivitas agen secara event-driven via `notify`, membekukan proses non-esensial via `nix` (`SIGSTOP`/`SIGCONT`), dan mengawasi ambang termal/baterai.
2. **GNOME Shell 42 Extension (GJS)**: Menampilkan switch kontrol dan telemetri langsung di top bar di samping jam, berkomunikasi dengan daemon melalui Session D-Bus (`org.antigravity.KeepAwake`).

---

## 2. Dependency Graph & Architecture Layers (Rust)

```text
[schemas/ GSettings]
       │
       ├── [extension/ GJS Extension] (UI Client in Top Bar)
       │         │
       │         │ (Session D-Bus: org.antigravity.KeepAwake)
       │         ▼
       └── [src/service.rs] (Unified State Machine Coordinator)
                 │
                 ├── [src/process_tree.rs] (Class 0/1/2 Taxonomy & Tree-walk)
                 │         │
                 │         └── [src/freezer.rs] (SIGSTOP/SIGCONT Manager & Watchdog)
                 │
                 ├── [src/activity_sentinel.rs] (notify inotify Watcher + Grace Timer)
                 │
                 ├── [src/sleep_inhibitor.rs] (zbus systemd-logind Inhibit Lock)
                 │
                 └── [src/thermal_guard.rs] (sysfs / UPower Battery & Heat Floor)
```

---

## 3. Ordered Task Phases

### Phase 1: Rust Workspace Foundations & Process Safety Core
- **Task 1:** Cargo Workspace Setup & 3-Class Taxonomy Engine (`src/process_tree.rs`)
- **Task 2:** Smart Process Freezer & Crash Watcher (`src/freezer.rs`)
- *Checkpoint 1: Zero-Risk Process Management Validated via `cargo test`.*

### Phase 2: Power Management & Activity Sentinel
- **Task 3:** Sleep Inhibitor & Lid State Engine via zbus (`src/sleep_inhibitor.rs`)
- **Task 4:** Activity Sentinel via inotify & Process Verification (`src/activity_sentinel.rs`)
- *Checkpoint 2: Event-driven Inhibit and Transcript Detection Validated.*

### Phase 3: Hardware Guardrails & D-Bus Service Integration
- **Task 5:** Thermal & Battery Floor Safeguards (`src/thermal_guard.rs`)
- **Task 6:** Unified Daemon Coordinator & D-Bus IPC Server (`src/service.rs`, `src/main.rs`)
- *Checkpoint 3: Full Daemon Core Functional as native ELF binary & systemd user unit.*

### Phase 4: GNOME Shell 42 Top Bar Extension & Packaging
- **Task 7:** GSettings Schema & Configuration Layer (`schemas/`)
- **Task 8:** GNOME Shell 42 Extension in GJS (`extension/`)
- **Task 9:** Installation Scripts & End-to-End Verification (`scripts/`)
- *Checkpoint 4: Full System Verification Complete.*

---

## 4. Risks & Mitigations

| Risiko | Dampak | Mitigasi Arsitektur |
| :--- | :--- | :--- |
| Salah membekukan Antigravity sendiri (karena berbasis Electron/Chromium). | **Kritis** | `src/process_tree.rs` melakukan recursive parent PID inspection dari root PID Antigravity IDE dan melindunginya secara absolut (Kelas 1). |
| Desktop hang / black screen jika proses Wayland/Audio dibekukan. | **Kritis** | Golden Whitelist Kelas 0 melindungi `gnome-shell`, `mutter`, `xwayland`, `pipewire`, `wireplumber`, dan `dbus-daemon`. |
| Aplikasi Kelas 2 terkunci di status `SIGSTOP` saat daemon crash/panic. | **Tinggi** | Signal handler untuk `SIGINT`, `SIGTERM`, dan panic hook otomatis memicu `SIGCONT` seketika ke seluruh PID yang tercatat dibekukan. |
| Polling CPU menguras baterai saat laptop tertutup di tas. | **Tinggi** | Dilarang menggunakan loop sleep. Semua operasi dipicu oleh sinyal D-Bus (sensor ACPI engsel) dan kernel `inotify`. |
| Laptop overheat di dalam tas ransel. | **Kritis** | Tiered Thermal Escalation: 75°C `nice` +10, 80°C darurat pemicu deep sleep otomatis. |
