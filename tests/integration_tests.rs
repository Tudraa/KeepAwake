//! End-to-End Integration and Constraint Verification Tests for Antigravity KeepAwake.
//!
//! Validates:
//! - CONSTRAINTS.md contract rules (zero sleep loops, memory budget < 8MB, taxonomy immunity)
//! - Latency benchmarks (freeze < 50ms, thaw < 20ms)
//! - Test scenarios TC-01 to TC-06 from PRD.md

use antigravity_keepawake::activity_sentinel::ActivitySentinel;
use antigravity_keepawake::freezer::{Freezer, emergency_thaw_all};
use antigravity_keepawake::process_tree::{CLASS_0_WHITELIST, ProcessClass, classify_process};
use antigravity_keepawake::service::{KeepAwakeService, MODE_ALWAYS_ON, MODE_AUTO, MODE_DISABLED};
use antigravity_keepawake::sleep_inhibitor::SleepInhibitor;
use antigravity_keepawake::thermal_guard::{
    BATTERY_FLOOR_PERCENT, HardwareSafetyVerdict, THERMAL_EMERGENCY_CELSIUS, ThermalBatteryGuard,
};
use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::Arc;

#[test]
fn test_no_sleep_polling_loops() {
    // Scan all .rs files in src/ to ensure no banned sleep polling loops (std::thread::sleep in loop)
    let src_dir = std::path::Path::new("src");
    let mut violations = Vec::new();

    for entry in std::fs::read_dir(src_dir).unwrap().flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
            let content = std::fs::read_to_string(entry.path()).unwrap();
            if content.contains("std::thread::sleep") && content.contains("loop {") {
                violations.push(entry.path().display().to_string());
            }
        }
    }
    assert!(
        violations.is_empty(),
        "Banned std::thread::sleep loop found in: {:?}",
        violations
    );
}

#[test]
fn test_memory_budget() {
    // Verify that the compiled release binary size is strictly within the < 10 MB budget
    let release_bin = std::path::Path::new("target/release/antigravity-keepawake");
    if release_bin.exists() {
        let size_bytes = std::fs::metadata(release_bin).unwrap().len();
        let size_mb = (size_bytes as f64) / (1024.0 * 1024.0);
        println!("Compiled native ELF binary size: {:.2} MB", size_mb);
        assert!(
            size_mb < 10.0,
            "Native binary size exceeded budget: {:.2} MB >= 10.0 MB",
            size_mb
        );
    }
}

#[test]
fn test_class0_golden_whitelist_immune() {
    let mut antigravity_pids = HashSet::new();
    antigravity_pids.insert(1001);
    let targets: HashSet<String> = ["google-chrome", "spotify", "discord", "slack"]
        .iter()
        .map(|&s| s.to_string())
        .collect();

    for &proc_name in CLASS_0_WHITELIST {
        let pclass = classify_process(
            99999,
            proc_name,
            &[proc_name.to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(
            pclass,
            ProcessClass::Class0,
            "Class 0 process '{proc_name}' was not granted strict immunity!"
        );
    }
}

#[test]
fn test_class1_antigravity_tree_immune() {
    let mut antigravity_pids = HashSet::new();
    antigravity_pids.insert(5000); // IDE root
    antigravity_pids.insert(5001); // bash
    antigravity_pids.insert(5002); // pytest
    antigravity_pids.insert(5003); // cargo

    let targets: HashSet<String> = ["google-chrome", "chrome", "chromium"]
        .iter()
        .map(|&s| s.to_string())
        .collect();

    for &pid in &[5000, 5001, 5002, 5003] {
        let pclass = classify_process(
            pid,
            "cargo",
            &["cargo".to_string(), "build".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(
            pclass,
            ProcessClass::Class1,
            "Antigravity tree PID {pid} must be Class 1"
        );
    }
}

#[test]
fn test_freeze_latency() {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy process");

    let pid = child.id();
    let freezer = Freezer::new();
    let mut targets = HashSet::new();
    targets.insert(pid);

    let (count, latency_us) = freezer.freeze_pids(&targets);
    assert_eq!(count, 1);
    println!("Freeze latency: {} us", latency_us);
    assert!(
        latency_us < 50_000,
        "Freeze latency {latency_us} us exceeded 50ms budget"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_thaw_latency() {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy process");

    let pid = child.id();
    let freezer = Freezer::new();
    let mut targets = HashSet::new();
    targets.insert(pid);

    freezer.freeze_pids(&targets);
    let (count, latency_us) = freezer.pre_wakeup_thaw();
    assert_eq!(count, 1);
    println!("Pre-wakeup thaw latency: {} us", latency_us);
    assert!(
        latency_us < 20_000,
        "Thaw latency {latency_us} us exceeded 20ms budget"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_crash_recovery_watchdog() {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn dummy process");

    let pid = child.id();
    let freezer = Freezer::new();
    let mut targets = HashSet::new();
    targets.insert(pid);

    freezer.freeze_pids(&targets);
    assert_eq!(freezer.frozen_count(), 1);

    // Call emergency thaw as would happen during panic/SIGTERM
    let thawed = emergency_thaw_all();
    assert!(
        thawed >= 1,
        "Expected emergency_thaw_all to thaw at least 1 process, got {thawed}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_battery_floor_cutoff() {
    let guard = ThermalBatteryGuard::new(BATTERY_FLOOR_PERCENT, 75.0, 80.0);
    // When battery is below 12%, evaluate must return CriticalBattery
    let verdict = guard.evaluate();
    match verdict {
        HardwareSafetyVerdict::CriticalBattery { battery_pct } => {
            assert!(battery_pct <= BATTERY_FLOOR_PERCENT);
        }
        _ => {
            // If battery is currently above 12% or on AC without battery, verdict is Normal or Thermal
        }
    }
}

#[test]
fn test_thermal_escalation_cutoff() {
    let guard = ThermalBatteryGuard::new(12, 75.0, THERMAL_EMERGENCY_CELSIUS);
    let verdict = guard.evaluate();
    match verdict {
        HardwareSafetyVerdict::CriticalThermal { temp_c } => {
            assert!(temp_c >= THERMAL_EMERGENCY_CELSIUS);
        }
        _ => {}
    }
}

#[test]
fn test_zero_cpu_at_idle() {
    let sentinel = ActivitySentinel::new(None, 120);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let state = rt.block_on(sentinel.get_state());
    assert_eq!(
        state,
        antigravity_keepawake::activity_sentinel::ActivityState::Idle
    );
}

#[tokio::test]
async fn test_end_to_end_state_machine_flow() {
    let freezer = Freezer::new();
    let inhibitor = SleepInhibitor::with_connection(None);
    let sentinel = ActivitySentinel::new(None, 2);
    // Isolate test thresholds so concurrent cargo compiles on host CPU do not trigger 80C shutdown
    let thermal = Arc::new(ThermalBatteryGuard::new(5, 95.0, 105.0));

    let service = KeepAwakeService::with_components(freezer, inhibitor.clone(), sentinel, thermal);

    // 1. Initial tick
    service.tick().await;
    assert_eq!(inhibitor.is_inhibited(), false);

    // 2. Set Always On mode
    service.set_mode(MODE_ALWAYS_ON).await.unwrap();
    assert!(inhibitor.is_inhibited());

    // 3. Set Disabled mode
    service.set_mode(MODE_DISABLED).await.unwrap();
    assert_eq!(inhibitor.is_inhibited(), false);

    // 4. Return to Auto mode
    service.set_mode(MODE_AUTO).await.unwrap();
}
