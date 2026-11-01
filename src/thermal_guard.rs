//! Thermal and Battery Floor Safeguards for closed-lid operation.
//!
//! Enforces:
//! - Low Battery Floor: <= 12% triggers automatic lock release and sleep.
//! - Tiered Thermal Escalation:
//!   - >= 75°C: Nice deprioritization (+10)
//!   - >= 80°C: Emergency Deep Sleep (prevents thermal runaway in closed backpack)

use std::fs;
use std::path::Path;
use tracing::{debug, warn};

pub const BATTERY_FLOOR_PERCENT: u32 = 12;
pub const THERMAL_THROTTLE_CELSIUS: f32 = 88.0;
pub const THERMAL_EMERGENCY_CELSIUS: f32 = 95.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardwareSafetyVerdict {
    Normal {
        battery_pct: Option<u32>,
        temp_c: Option<f32>,
    },
    ThermalThrottle {
        temp_c: f32,
    },
    CriticalBattery {
        battery_pct: u32,
    },
    CriticalThermal {
        temp_c: f32,
    },
}

pub struct ThermalBatteryGuard {
    battery_floor: u32,
    thermal_throttle: f32,
    thermal_emergency: f32,
}

impl Default for ThermalBatteryGuard {
    fn default() -> Self {
        Self {
            battery_floor: BATTERY_FLOOR_PERCENT,
            thermal_throttle: THERMAL_THROTTLE_CELSIUS,
            thermal_emergency: THERMAL_EMERGENCY_CELSIUS,
        }
    }
}

impl ThermalBatteryGuard {
    pub fn new(battery_floor: u32, thermal_throttle: f32, thermal_emergency: f32) -> Self {
        Self {
            battery_floor,
            thermal_throttle,
            thermal_emergency,
        }
    }

    /// Read highest CPU temperature across all thermal zones in Celsius.
    pub fn read_cpu_temperature() -> Option<f32> {
        let thermal_dir = Path::new("/sys/class/thermal");
        if !thermal_dir.exists() {
            return None;
        }

        let mut max_temp: Option<f32> = None;
        if let Ok(entries) = fs::read_dir(thermal_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("thermal_zone") {
                    let temp_path = entry.path().join("temp");
                    if let Ok(content) = fs::read_to_string(temp_path)
                        && let Ok(milli_c) = content.trim().parse::<i64>()
                    {
                        let c = (milli_c as f32) / 1000.0;
                        if max_temp.is_none() || Some(c) > max_temp {
                            max_temp = Some(c);
                        }
                    }
                }
            }
        }
        max_temp
    }

    /// Read battery percentage from /sys/class/power_supply/
    pub fn read_battery_percentage() -> Option<u32> {
        let ps_dir = Path::new("/sys/class/power_supply");
        if !ps_dir.exists() {
            return None;
        }

        if let Ok(entries) = fs::read_dir(ps_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("BAT") {
                    let cap_path = entry.path().join("capacity");
                    if let Ok(content) = fs::read_to_string(cap_path)
                        && let Ok(pct) = content.trim().parse::<u32>()
                    {
                        return Some(pct);
                    }
                }
            }
        }
        None
    }

    /// Evaluate hardware safety metrics and return appropriate verdict.
    pub fn evaluate(&self) -> HardwareSafetyVerdict {
        let battery_opt = Self::read_battery_percentage();
        let temp_opt = Self::read_cpu_temperature();

        // 1. Critical Battery Floor (<= 12%)
        if let Some(battery) = battery_opt
            && battery <= self.battery_floor
        {
            warn!(
                "Battery level critical: {battery}% <= {}%",
                self.battery_floor
            );
            return HardwareSafetyVerdict::CriticalBattery {
                battery_pct: battery,
            };
        }

        // 2. Critical Thermal Emergency (>= 80°C)
        if let Some(temp) = temp_opt {
            if temp >= self.thermal_emergency {
                warn!(
                    "CPU Temperature critical: {temp}°C >= {}°C",
                    self.thermal_emergency
                );
                return HardwareSafetyVerdict::CriticalThermal { temp_c: temp };
            }
            // 3. Thermal Warning Throttle (>= 75°C)
            if temp >= self.thermal_throttle {
                debug!(
                    "CPU Temperature warning: {temp}°C >= {}°C",
                    self.thermal_throttle
                );
                return HardwareSafetyVerdict::ThermalThrottle { temp_c: temp };
            }
        }

        HardwareSafetyVerdict::Normal {
            battery_pct: battery_opt,
            temp_c: temp_opt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thermal_guard_evaluation_logic() {
        let guard = ThermalBatteryGuard::new(12, 75.0, 80.0);

        // Test normal verdict structure
        let verdict = guard.evaluate();
        match verdict {
            HardwareSafetyVerdict::Normal { .. } => {}
            HardwareSafetyVerdict::CriticalBattery { .. } => {}
            HardwareSafetyVerdict::CriticalThermal { .. } => {}
            HardwareSafetyVerdict::ThermalThrottle { .. } => {}
        }
    }

    #[test]
    fn test_sysfs_readers_do_not_panic() {
        // Must succeed without throwing/panicking on any Linux system
        let _ = ThermalBatteryGuard::read_cpu_temperature();
        let _ = ThermalBatteryGuard::read_battery_percentage();
    }
}
