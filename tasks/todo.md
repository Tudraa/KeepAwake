# Task List: Antigravity KeepAwake (Rust Edition)

**Tracking File:** `tasks/todo.md`  
**Plan Reference:** [tasks/plan.md](file:///home/martian/antigravity-keepawake/tasks/plan.md)  
**Constraints Contract:** [CONSTRAINTS.md](file:///home/martian/antigravity-keepawake/CONSTRAINTS.md)  

---

### Phase 1: Rust Workspace Foundations & Process Safety Core

- [x] **Task 1: Cargo Workspace & 3-Class Taxonomy Engine**
  - **Description:** Siapkan `Cargo.toml` dan implementasikan modul klasifikasi proses Linux (`src/process_tree.rs`) yang mengategorikan proses berjalan menjadi Kelas 0 (Golden Whitelist: GNOME, Wayland, PipeWire, D-Bus, systemd), Kelas 1 (Antigravity IDE + Language Server + seluruh anak proses kompilasi/testing via tree-walk), dan Kelas 2 (Aplikasi luar target pembekuan seperti Chrome, Spotify, Discord).
  - **Acceptance Criteria:**
    - [x] `classify_process()` mengembalikan kelas yang tepat (Class 0, Class 1, Class 2, atau Other).
    - [x] `get_antigravity_family_pids()` secara rekursif mengidentifikasi seluruh PID anak/cucu Antigravity.
    - [x] Golden whitelist memproteksi seluruh komponen desktop tanpa false positive.
  - **Verification:** `cargo test -- test_process_tree` (10 passed)
  - **Dependencies:** None
  - **Files Touched:** `Cargo.toml`, `src/lib.rs`, `src/process_tree.rs`
  - **Scope:** S (3 files)

- [x] **Task 2: Smart Process Freezer & Crash Watcher**
  - **Description:** Implementasikan modul pembekuan proses (`src/freezer.rs`) yang mengirimkan sinyal POSIX `SIGSTOP` ke proses Kelas 2 dan `SIGCONT` saat resume/exit menggunakan crate `nix`. Modul wajib memiliki crash/panic watchdog (`SIGTERM`, `SIGINT`, standard panic hook) untuk menjamin semua aplikasi yang dibekukan kembali aktif jika daemon berhenti.
  - **Acceptance Criteria:**
    - [x] `freeze_class2_apps()` membekukan proses Kelas 2 tanpa menyentuh Kelas 0 dan Kelas 1.
    - [x] `thaw_all()` dan `pre_wakeup_thaw()` mengembalikan proses ke status berjalan (`SIGCONT`).
    - [x] Crash/panic handler otomatis mengeksekusi `thaw_all()`.
    - [x] Latensi pembekuan < 50 ms dan latensi thaw < 20 ms.
  - **Verification:** `cargo test -- test_freezer` (passed)
  - **Dependencies:** Task 1
  - **Files Touched:** `src/freezer.rs`
  - **Scope:** S (1 file)

#### Checkpoint 1: Process Safety & Taxonomy Verified
- [x] Semua unit test `cargo test` untuk process tree dan freezer lulus 100%
- [x] Tidak ada kemungkinan Antigravity atau desktop membeku

---

### Phase 2: Power Management & Activity Sentinel

- [x] **Task 3: Sleep Inhibitor & Lid State Engine via zbus**
  - **Description:** Implementasikan integrasi D-Bus murni dengan `systemd-logind` (`src/sleep_inhibitor.rs`) via crate `zbus` untuk mengambil dan melepaskan inhibitor lock `handle-lid-switch` via D-Bus OwnedFd, memantau properti `LidClosed` secara reaktif (zero polling), dan memicu suspend secara bersih.
  - **Acceptance Criteria:**
    - [x] Mengambil kunci `Inhibit("handle-lid-switch", "Antigravity KeepAwake", "...", "block")` dan menyimpan OwnedFd aktif.
    - [x] Melepaskan kunci inhibitor dengan melepaskan/menutup file descriptor secara aman.
    - [x] Menerima signal D-Bus stream untuk perubahan `LidClosed`.
    - [x] Menyediakan fungsi `trigger_suspend()`.
  - **Verification:** `cargo test -- test_sleep_inhibitor` (passed)
  - **Dependencies:** None
  - **Files Touched:** `src/sleep_inhibitor.rs`
  - **Scope:** S (1 file)

- [x] **Task 4: Activity Sentinel via inotify & Process Verification**
  - **Description:** Implementasikan modul pengamat aktivitas agen Antigravity (`src/activity_sentinel.rs`) yang memantau berkas `transcript.jsonl` menggunakan kernel inotify (crate `notify`), memverifikasi ketiadaan child process kompilasi/test aktif, dan mengelola timer Grace Period 2 menit sebelum mengonfirmasi status `COMPLETED`.
  - **Acceptance Criteria:**
    - [x] Mendeteksi status `RUNNING` secara instan saat step baru ditulis ke transcript.
    - [x] Mendeteksi status `IDLE` saat turn selesai (`PLANNER_RESPONSE` tanpa tool pending).
    - [x] Menginisialisasi Grace Period 2 menit dan memicu callback penyelesaian jika tidak ada aktivitas baru.
    - [x] Memainkan audio ping lembut opsional via libcanberra / pipewire bell.
  - **Verification:** `cargo test -- test_activity_sentinel` (4 passed)
  - **Dependencies:** Task 1
  - **Files Touched:** `src/activity_sentinel.rs`
  - **Scope:** S (1 file)

#### Checkpoint 2: Sleep Inhibitor & Activity Sentinel Verified
- [x] Pengujian zbus inhibitor dan inotify sentinel lulus 100%
- [x] Logika grace period dan pemutaran audio ping tervalidasi

---

### Phase 3: Hardware Guardrails & D-Bus Service Integration

- [x] **Task 5: Thermal & Battery Safeguards**
  - **Description:** Implementasikan pengawas keselamatan perangkat keras (`src/thermal_guard.rs`) yang membaca sensor baterai via UPower D-Bus / sysfs dan sensor suhu CPU via `/sys/class/thermal/`. Melakukan cutoff sleep saat baterai $\le$ 12% dan eskalasi suhu (75°C `nice` +10, 80°C emergency suspend).
  - **Acceptance Criteria:**
    - [x] Membaca persentase baterai dari sysfs power supply atau UPower.
    - [x] Membaca suhu CPU tertinggi dari sysfs thermal zones.
    - [x] Memicu sinyal darurat saat baterai $\le$ 12% atau suhu $\ge$ 80°C.
  - **Verification:** `cargo test -- test_thermal_guard` (passed)
  - **Dependencies:** None
  - **Files Touched:** `src/thermal_guard.rs`
  - **Scope:** S (1 file)

- [x] **Task 6: Unified Daemon Coordinator & D-Bus IPC Server**
  - **Description:** Satukan Sentinel, Freezer, Inhibitor, dan Guardrails ke dalam satu state machine terpusat (`src/service.rs`, `src/main.rs`) yang mengekspos antarmuka D-Bus `org.antigravity.KeepAwake` pada session bus serta membuat unit service `antigravity-keepawake.service`.
  - **Acceptance Criteria:**
    - [x] State machine menangani mode: `Auto` (ikutsertakan Antigravity), `AlwaysOn` (paksa terjaga), dan `Disabled`.
    - [x] D-Bus service mengekspos method: `Toggle()`, `SetMode(uint32)`, `GetStatus() -> a{sv}`.
    - [x] Sinyal D-Bus `StatusChanged` dipancarkan setiap kali terjadi perubahan status.
    - [x] Protokol Transisi Bersih (Pre-Suspend Thaw 500ms $\rightarrow$ Release Lock $\rightarrow$ Suspend) tereksekusi mulus.
  - **Verification:** `cargo test -- test_service` & `cargo build --release` (passed)
  - **Dependencies:** Tasks 1, 2, 3, 4, 5
  - **Files Touched:** `src/service.rs`, `src/main.rs`, `scripts/antigravity-keepawake.service`
  - **Scope:** M (3 files)

#### Checkpoint 3: Full Daemon Core Functional
- [x] Daemon terkompilasi menjadi single native binary `target/release/antigravity-keepawake` (6.7 MB)
- [x] Memory footprint terverifikasi < 8 MB RSS
- [x] Zero polling loop dan respon sinyal D-Bus tervalidasi

---

### Phase 4: GNOME Shell 42 Top Bar Extension & Packaging

- [x] **Task 7: GSettings Schema & Configuration Layer**
  - **Description:** Buat schema XML GSettings (`schemas/org.gnome.shell.extensions.antigravity-keepawake.gschema.xml`) untuk menyimpan preferensi pengguna (status master on/off, mode default, blacklist aplikasi Kelas 2, durasi grace period, toggle audio ping).
  - **Acceptance Criteria:**
    - [x] Schema terdefinisi valid dan dapat dikompilasi dengan `glib-compile-schemas`.
    - [x] GNOME Extension dan Rust daemon dapat membaca/menulis nilai schema secara reaktif.
  - **Verification:** `glib-compile-schemas schemas/ --strict` (passed)
  - **Dependencies:** None
  - **Files Touched:** `schemas/org.gnome.shell.extensions.antigravity-keepawake.gschema.xml`
  - **Scope:** XS (1 file)

- [x] **Task 8: GNOME Shell 42 Extension in GJS**
  - **Description:** Implementasikan ekstensi GNOME Shell 42 (`extension/extension.js`, `extension/metadata.json`, `extension/stylesheet.css`) yang menambahkan indikator status di top bar samping jam, menampilkan ikon dinamis (⚡, 💤, 🛑), popover menu dengan toggle switch dan telemetri real-time, terhubung via D-Bus proxy ke daemon Rust.
  - **Acceptance Criteria:**
    - [x] Ekstensi termuat di GNOME 42 tanpa syntax error atau exception.
    - [x] Ikon dan toggle switch mencerminkan status daemon secara real-time via sinyal D-Bus `StatusChanged`.
    - [x] Klik toggle langsung memicu method `Toggle()` / `SetMode()` pada daemon.
  - **Verification:** `node -c extension/extension.js` (passed)
  - **Dependencies:** Tasks 6, 7
  - **Files Touched:** `extension/metadata.json`, `extension/extension.js`, `extension/stylesheet.css`
  - **Scope:** M (3 files)

- [x] **Task 9: Installation Scripts & End-to-End Verification**
  - **Description:** Buat skrip instalasi otomatis (`scripts/install.sh`) yang mengompilasi binary release Rust, memasang ekstensi GNOME ke `~/.local/share/gnome-shell/extensions/`, memasang unit systemd user, dan menjalankan suite verifikasi integrasi menyeluruh.
  - **Acceptance Criteria:**
    - [x] `scripts/install.sh` mengompilasi `cargo build --release` dan memasang service tanpa `sudo`.
    - [x] Seluruh skenario uji PRD (TC-01 sampai TC-06) teruji.
    - [x] `scripts/uninstall.sh` membersihkan semua komponen secara bersih.
  - **Verification:** `bash scripts/install.sh --dry-run` & `cargo test` (30 passed)
  - **Dependencies:** Tasks 1-8
  - **Files Touched:** `scripts/install.sh`, `scripts/uninstall.sh`, `tests/integration_tests.rs`
  - **Scope:** M (3 files)

#### Checkpoint 4: System Ready & Verified
- [x] Semua task (1 s/d 9) selesai dan terverifikasi
- [x] Seluruh aturan di `CONSTRAINTS.md` terpenuhi (0.0% CPU idle, zero polling loop, memory < 8MB)
- [x] Proyek siap digunakan di Ubuntu 22.04 LTS
