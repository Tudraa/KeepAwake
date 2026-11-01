//! systemd-logind Sleep Inhibitor and Lid State Engine via zbus.
//!
//! Controls sleep prevention specifically when the laptop lid is closed:
//! - Acquires and releases Inhibit lock (`handle-lid-switch`) via D-Bus File Descriptor.
//! - Monitors `LidClosed` property on `org.freedesktop.login1.Manager`.
//! - Triggers clean deep sleep via `Suspend(true)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tracing::{error, info};
use zbus::Connection;
use zbus::zvariant::OwnedFd;

pub const INHIBIT_WHAT: &str = "handle-lid-switch";
pub const INHIBIT_WHO: &str = "Antigravity KeepAwake";
pub const INHIBIT_WHY: &str = "Antigravity coding task in progress";
pub const INHIBIT_MODE: &str = "block";

pub struct SleepInhibitor {
    system_conn: Option<Connection>,
    active_fd: Mutex<Option<OwnedFd>>,
    is_locked: AtomicBool,
}

impl SleepInhibitor {
    pub async fn new() -> Arc<Self> {
        let system_conn = match Connection::system().await {
            Ok(c) => Some(c),
            Err(e) => {
                error!(
                    "Failed to connect to D-Bus System Bus: {e}. Running in degraded mock mode."
                );
                None
            }
        };

        Arc::new(Self {
            system_conn,
            active_fd: Mutex::new(None),
            is_locked: AtomicBool::new(false),
        })
    }

    /// Construct with an explicit connection (useful for testing or fallback).
    pub fn with_connection(conn: Option<Connection>) -> Arc<Self> {
        Arc::new(Self {
            system_conn: conn,
            active_fd: Mutex::new(None),
            is_locked: AtomicBool::new(false),
        })
    }

    /// Check if the inhibitor lock is currently acquired.
    pub fn is_inhibited(&self) -> bool {
        self.is_locked.load(Ordering::SeqCst)
    }

    /// Acquire the handle-lid-switch block inhibitor lock.
    pub async fn acquire(&self) -> Result<bool, String> {
        if self.is_inhibited() {
            return Ok(false); // Already acquired
        }

        let mut fd_guard = self.active_fd.lock().await;
        if fd_guard.is_some() {
            self.is_locked.store(true, Ordering::SeqCst);
            return Ok(false);
        }

        if let Some(conn) = &self.system_conn {
            let reply: OwnedFd = conn
                .call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    "Inhibit",
                    &(INHIBIT_WHAT, INHIBIT_WHO, INHIBIT_WHY, INHIBIT_MODE),
                )
                .await
                .map_err(|e| format!("Failed to acquire login1 Inhibit lock: {e}"))?
                .body()
                .deserialize()
                .map_err(|e| format!("Failed to deserialize Inhibit fd reply: {e}"))?;

            info!("Successfully acquired systemd-logind lid-switch inhibitor lock.");
            *fd_guard = Some(reply);
        }

        self.is_locked.store(true, Ordering::SeqCst);
        Ok(true)
    }

    /// Release the inhibitor lock by dropping the file descriptor.
    pub async fn release(&self) -> bool {
        if !self.is_inhibited() {
            return false;
        }

        let mut fd_guard = self.active_fd.lock().await;
        let was_held = fd_guard.take().is_some() || self.is_locked.load(Ordering::SeqCst);
        self.is_locked.store(false, Ordering::SeqCst);

        if was_held {
            info!("Released systemd-logind lid-switch inhibitor lock.");
        }
        was_held
    }

    /// Query whether the laptop lid is currently closed.
    pub async fn is_lid_closed(&self) -> Result<bool, String> {
        if let Some(conn) = &self.system_conn {
            let proxy = zbus::Proxy::new(
                conn,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )
            .await
            .map_err(|e| format!("Failed to create login1 Manager proxy: {e}"))?;

            let lid_closed: bool = proxy
                .get_property("LidClosed")
                .await
                .map_err(|e| format!("Failed to read LidClosed property: {e}"))?;

            Ok(lid_closed)
        } else {
            // Fallback reading from /proc/acpi/button/lid/
            Self::read_lid_sysfs()
        }
    }

    /// Trigger deep sleep (Suspend) on systemd-logind.
    pub async fn trigger_suspend(&self) -> Result<(), String> {
        info!("Triggering clean system suspend via systemd-logind...");
        if let Some(conn) = &self.system_conn {
            conn.call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "Suspend",
                &(true,), // interactive = true
            )
            .await
            .map_err(|e| format!("D-Bus Suspend call failed: {e}"))?;
            Ok(())
        } else {
            let status = tokio::process::Command::new("systemctl")
                .arg("suspend")
                .status()
                .await
                .map_err(|e| format!("systemctl suspend execution failed: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("systemctl suspend exited with {status}"))
            }
        }
    }

    /// Sysfs fallback reader for /proc/acpi/button/lid/*/state
    fn read_lid_sysfs() -> Result<bool, String> {
        let lid_dir = std::path::Path::new("/proc/acpi/button/lid");
        if !lid_dir.exists() {
            return Ok(false);
        }

        if let Ok(entries) = std::fs::read_dir(lid_dir) {
            for entry in entries.flatten() {
                let state_file = entry.path().join("state");
                if let Ok(content) = std::fs::read_to_string(state_file)
                    && content.to_lowercase().contains("closed")
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inhibitor_acquire_and_release_lifecycle() {
        let inhibitor = SleepInhibitor::with_connection(None);
        assert!(!inhibitor.is_inhibited());

        // 1. Acquire
        let res = inhibitor.acquire().await;
        assert!(res.is_ok());
        assert!(inhibitor.is_inhibited());

        // Duplicate acquire should be idempotent (return Ok(false))
        let res_dup = inhibitor.acquire().await;
        assert_eq!(res_dup, Ok(false));

        // 2. Release
        let released = inhibitor.release().await;
        assert!(released);
        assert!(!inhibitor.is_inhibited());

        // Duplicate release should return false
        let released_dup = inhibitor.release().await;
        assert!(!released_dup);
    }

    #[tokio::test]
    async fn test_is_lid_closed_query() {
        let inhibitor = SleepInhibitor::new().await;
        let lid_closed = inhibitor.is_lid_closed().await;
        assert!(
            lid_closed.is_ok(),
            "Lid query should succeed on Ubuntu system: {:?}",
            lid_closed
        );
        // On standard running laptop or runner, lid is currently open (false)
        assert_eq!(lid_closed.unwrap(), false);
    }
}
