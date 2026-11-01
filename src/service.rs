//! Unified Daemon Coordinator and D-Bus IPC Server (`org.antigravity.KeepAwake`).
//!
//! Exposes methods and telemetry over Session D-Bus to the GNOME Shell Top Bar extension:
//! - Toggle() -> bool
//! - SetMode(u32)
//! - GetStatus() -> Dict
//! - StatusChanged signal

use crate::activity_sentinel::{ActivitySentinel, ActivityState};
use crate::freezer::Freezer;
use crate::process_tree::get_freezable_pids;
use crate::sleep_inhibitor::SleepInhibitor;
use crate::thermal_guard::{HardwareSafetyVerdict, ThermalBatteryGuard};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;
use sysinfo::System;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use zbus::zvariant::OwnedValue;

pub const MODE_AUTO: u32 = 0;
pub const MODE_ALWAYS_ON: u32 = 1;
pub const MODE_DISABLED: u32 = 2;

#[derive(Clone)]
pub struct KeepAwakeService {
    enabled: Arc<AtomicBool>,
    mode: Arc<AtomicU32>,
    freezer: Arc<Freezer>,
    inhibitor: Arc<SleepInhibitor>,
    sentinel: Arc<ActivitySentinel>,
    thermal_guard: Arc<ThermalBatteryGuard>,
    sys: Arc<Mutex<System>>,
    last_lid_state: Arc<AtomicBool>,
    last_suspend_time: Arc<Mutex<Option<Instant>>>,
}

impl KeepAwakeService {
    pub async fn new() -> Self {
        let freezer = Freezer::new();
        let inhibitor = SleepInhibitor::new().await;
        let sentinel = ActivitySentinel::new(None, 120); // 2 minute grace period
        let thermal_guard = Arc::new(ThermalBatteryGuard::default());
        let sys = Arc::new(Mutex::new(System::new_all()));

        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            mode: Arc::new(AtomicU32::new(MODE_AUTO)),
            freezer,
            inhibitor,
            sentinel,
            thermal_guard,
            sys,
            last_lid_state: Arc::new(AtomicBool::new(false)),
            last_suspend_time: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_components(
        freezer: Arc<Freezer>,
        inhibitor: Arc<SleepInhibitor>,
        sentinel: Arc<ActivitySentinel>,
        thermal_guard: Arc<ThermalBatteryGuard>,
    ) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            mode: Arc::new(AtomicU32::new(MODE_AUTO)),
            freezer,
            inhibitor,
            sentinel,
            thermal_guard,
            sys: Arc::new(Mutex::new(System::new_all())),
            last_lid_state: Arc::new(AtomicBool::new(false)),
            last_suspend_time: Arc::new(Mutex::new(None)),
        }
    }

    async fn can_trigger_suspend(&self) -> bool {
        let guard = self.last_suspend_time.lock().await;
        if let Some(last) = *guard
            && last.elapsed() < std::time::Duration::from_secs(60)
        {
            debug!("Suspend suppressed: currently in 60s post-suspend cooldown.");
            return false;
        }
        true
    }

    async fn mark_suspend_triggered(&self) {
        let mut guard = self.last_suspend_time.lock().await;
        *guard = Some(Instant::now());
    }

    /// Primary reactive evaluation tick called whenever an event occurs.
    pub async fn tick(&self) {
        if !self.enabled.load(Ordering::SeqCst) {
            // Disabled state: release inhibitor and thaw apps
            let _ = self.inhibitor.release().await;
            self.freezer.thaw_all();
            return;
        }

        let mode = self.mode.load(Ordering::SeqCst);
        let lid_closed = self.inhibitor.is_lid_closed().await.unwrap_or(false);
        let prev_lid = self.last_lid_state.swap(lid_closed, Ordering::SeqCst);

        // Pre-Wakeup Thaw: if lid was closed and is now open, thaw immediately (< 20ms)
        if prev_lid && !lid_closed {
            info!("Lid opened: Executing Pre-Wakeup Thaw for all Class 2 applications.");
            self.freezer.pre_wakeup_thaw();
        }

        // Check Hardware Safety Guardrails
        // NOTE: KeepAwake's responsibility is solely closed-lid power management.
        // If the lid is OPEN, the user is actively at their laptop; KeepAwake must NEVER
        // trigger an unprompted suspend while the lid is open!
        match self.thermal_guard.evaluate() {
            HardwareSafetyVerdict::CriticalBattery { battery_pct } => {
                if lid_closed {
                    if self.can_trigger_suspend().await {
                        warn!(
                            "Battery critical ({battery_pct}%) with lid closed: Triggering emergency suspend protocol."
                        );
                        self.freezer.pre_suspend_thaw().await;
                        let _ = self.inhibitor.release().await;
                        let _ = self.inhibitor.trigger_suspend().await;
                        self.mark_suspend_triggered().await;
                    }
                    return;
                } else {
                    debug!(
                        "Battery low ({battery_pct}%), but lid is open. System desktop alerts apply."
                    );
                }
            }
            HardwareSafetyVerdict::CriticalThermal { temp_c } => {
                if lid_closed {
                    if self.can_trigger_suspend().await {
                        warn!(
                            "Thermal critical ({temp_c}°C) with lid closed: Triggering emergency suspend protocol."
                        );
                        self.freezer.pre_suspend_thaw().await;
                        let _ = self.inhibitor.release().await;
                        let _ = self.inhibitor.trigger_suspend().await;
                        self.mark_suspend_triggered().await;
                    }
                    return;
                } else {
                    debug!(
                        "Thermal high ({temp_c}°C), but lid is open. Internal laptop fans managing cooling."
                    );
                }
            }
            HardwareSafetyVerdict::ThermalThrottle { .. } => {
                // Warning stage (nice deprioritization handled if needed)
            }
            HardwareSafetyVerdict::Normal { .. } => {}
        }

        // Evaluate Mode
        match mode {
            MODE_ALWAYS_ON => {
                let _ = self.inhibitor.acquire().await;
            }
            MODE_AUTO => {
                let activity = self.sentinel.update_status(false).await;
                match activity {
                    ActivityState::Active => {
                        let _ = self.inhibitor.acquire().await;

                        // If lid is closed and agent is active, freeze Class 2 apps
                        if lid_closed {
                            let mut sys_guard = self.sys.lock().await;
                            let freezable = get_freezable_pids(&mut sys_guard, None, true);
                            if !freezable.is_empty() {
                                self.freezer.freeze_pids(&freezable);
                            }
                        }
                    }
                    ActivityState::GracePeriod { remaining_secs } => {
                        // Keep inhibitor locked during grace period
                        let _ = self.inhibitor.acquire().await;
                        if remaining_secs == 120 {
                            // Play optional chime when task finishes
                            self.sentinel.play_chime_sound().await;
                        }
                    }
                    ActivityState::Idle => {
                        // Work completed and grace period expired
                        if self.inhibitor.is_inhibited() {
                            info!("Antigravity task completed and grace period expired.");
                            if lid_closed {
                                if self.can_trigger_suspend().await {
                                    info!(
                                        "Lid is closed: Executing Clean Transition Protocol to Deep Sleep."
                                    );
                                    self.freezer.pre_suspend_thaw().await;
                                    let _ = self.inhibitor.release().await;
                                    let _ = self.inhibitor.trigger_suspend().await;
                                    self.mark_suspend_triggered().await;
                                }
                            } else {
                                let _ = self.inhibitor.release().await;
                                self.freezer.thaw_all();
                            }
                        }
                    }
                }
            }
            _ => {
                // Disabled / unknown
                let _ = self.inhibitor.release().await;
                self.freezer.thaw_all();
            }
        }
    }
}

#[zbus::interface(name = "org.antigravity.KeepAwake")]
impl KeepAwakeService {
    /// Toggle master enable state ON / OFF.
    pub async fn toggle(&self) -> zbus::fdo::Result<bool> {
        let prev = self.enabled.fetch_xor(true, Ordering::SeqCst);
        let new_val = !prev;
        info!("Master switch toggled: enabled = {new_val}");
        self.tick().await;
        Ok(new_val)
    }

    /// Set operating mode (0: Auto, 1: AlwaysOn, 2: Disabled).
    pub async fn set_mode(&self, mode: u32) -> zbus::fdo::Result<()> {
        if mode > 2 {
            return Err(zbus::fdo::Error::InvalidArgs("Invalid mode index".into()));
        }
        self.mode.store(mode, Ordering::SeqCst);
        if mode == MODE_DISABLED {
            self.enabled.store(false, Ordering::SeqCst);
        } else {
            self.enabled.store(true, Ordering::SeqCst);
        }
        info!("Operating mode changed to {mode}");
        self.tick().await;
        Ok(())
    }

    /// Query comprehensive real-time telemetry dictionary.
    pub async fn get_status(&self) -> zbus::fdo::Result<HashMap<String, OwnedValue>> {
        let mut map = HashMap::new();

        let enabled = self.enabled.load(Ordering::SeqCst);
        let mode = self.mode.load(Ordering::SeqCst);
        let activity = self.sentinel.get_state().await;
        let lid_closed = self.inhibitor.is_lid_closed().await.unwrap_or(false);
        let inhibited = self.inhibitor.is_inhibited();
        let frozen_count = self.freezer.frozen_count() as u32;

        let (act_str, remaining_secs) = match activity {
            ActivityState::Active => ("Active", 0u64),
            ActivityState::GracePeriod { remaining_secs } => ("GracePeriod", remaining_secs),
            ActivityState::Idle => ("Idle", 0u64),
        };

        let temp_c = ThermalBatteryGuard::read_cpu_temperature().unwrap_or(0.0) as f64;
        let battery_pct = ThermalBatteryGuard::read_battery_percentage().unwrap_or(100);

        map.insert("enabled".into(), OwnedValue::from(enabled));
        map.insert("mode".into(), OwnedValue::from(mode));
        map.insert(
            "activity_state".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(act_str)).unwrap(),
        );
        map.insert(
            "grace_remaining_secs".into(),
            OwnedValue::from(remaining_secs),
        );
        map.insert("lid_closed".into(), OwnedValue::from(lid_closed));
        map.insert("inhibited".into(), OwnedValue::from(inhibited));
        map.insert("frozen_count".into(), OwnedValue::from(frozen_count));
        map.insert("cpu_temp_c".into(), OwnedValue::from(temp_c));
        map.insert("battery_pct".into(), OwnedValue::from(battery_pct));

        Ok(map)
    }

    /// Simple ping method for liveness check.
    pub fn ping(&self) -> String {
        "pong".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_toggle_and_mode_switching() {
        let freezer = Freezer::new();
        let inhibitor = SleepInhibitor::with_connection(None);
        let sentinel = ActivitySentinel::new(None, 2);
        let thermal = Arc::new(ThermalBatteryGuard::default());

        let service = KeepAwakeService::with_components(freezer, inhibitor, sentinel, thermal);

        // Initially enabled in Auto mode
        let status = service.get_status().await.unwrap();
        assert_eq!(
            status
                .get("enabled")
                .unwrap()
                .downcast_ref::<bool>()
                .unwrap(),
            true
        );
        assert_eq!(
            status.get("mode").unwrap().downcast_ref::<u32>().unwrap(),
            0
        );

        // Toggle OFF
        let toggled = service.toggle().await.unwrap();
        assert!(!toggled);
        assert!(!service.enabled.load(Ordering::SeqCst));

        // Set mode AlwaysOn
        service.set_mode(MODE_ALWAYS_ON).await.unwrap();
        assert!(service.enabled.load(Ordering::SeqCst));
        assert_eq!(service.mode.load(Ordering::SeqCst), MODE_ALWAYS_ON);
        assert_eq!(service.ping(), "pong");
    }

    #[tokio::test]
    async fn test_service_never_suspends_when_lid_open_even_under_critical_thermal() {
        let freezer = Freezer::new();
        let inhibitor = SleepInhibitor::with_connection(None);
        let sentinel = ActivitySentinel::new(None, 2);
        // Force critical thermal with 0.0 threshold
        let thermal = Arc::new(ThermalBatteryGuard::new(12, 0.0, 0.0));

        let service = KeepAwakeService::with_components(freezer, inhibitor, sentinel, thermal);

        // Tick with open lid
        service.tick().await;

        // Verify that suspend was NEVER triggered because lid is open
        assert!(service.last_suspend_time.lock().await.is_none());
    }
}
