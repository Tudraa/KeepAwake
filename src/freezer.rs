//! Smart Process Freezer and Crash Watcher using POSIX SIGSTOP/SIGCONT.
//!
//! Guarantees:
//! - Sends SIGSTOP to target Class 2 applications when lid closes.
//! - Delivers Pre-Wakeup Thaw (< 20 ms) when lid opens.
//! - Delivers Pre-Suspend Thaw (500 ms pause) before systemctl suspend.
//! - Emergency crash watchdog (SIGTERM, SIGINT, panic) immediately thaws all frozen processes.

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static GLOBAL_FROZEN_PIDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
static WATCHDOG_INSTALLED: AtomicBool = AtomicBool::new(false);

fn get_global_frozen() -> &'static Mutex<HashSet<u32>> {
    GLOBAL_FROZEN_PIDS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Emergency fallback: synchronous thaw of all known frozen processes.
pub fn emergency_thaw_all() -> usize {
    let mut count = 0;
    if let Ok(mut set) = get_global_frozen().lock() {
        for &pid in set.iter() {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGCONT);
            count += 1;
        }
        set.clear();
    }
    count
}

/// Install signal handlers and panic hook to guarantee no process remains in SIGSTOP.
pub fn install_crash_watchdog() {
    if WATCHDOG_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    // Standard Rust panic hook
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        emergency_thaw_all();
        prev_hook(info);
    }));

    // POSIX Signal handling for SIGINT and SIGTERM
    unsafe {
        use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, sigaction};
        extern "C" fn handle_signal(_: libc::c_int) {
            emergency_thaw_all();
            std::process::exit(128 + 15);
        }

        let sa = SigAction::new(
            SigHandler::Handler(handle_signal),
            SaFlags::SA_RESTART,
            SigSet::empty(),
        );
        let _ = sigaction(Signal::SIGTERM, &sa);
        let _ = sigaction(Signal::SIGINT, &sa);
        let _ = sigaction(Signal::SIGHUP, &sa);
    }
}

#[derive(Debug, Default)]
pub struct Freezer {
    frozen_pids: Mutex<HashSet<u32>>,
}

impl Freezer {
    pub fn new() -> Arc<Self> {
        install_crash_watchdog();
        Arc::new(Self {
            frozen_pids: Mutex::new(HashSet::new()),
        })
    }

    /// Freeze a set of target PIDs using SIGSTOP.
    /// Returns (number of newly frozen processes, latency in microseconds).
    pub fn freeze_pids(&self, pids: &HashSet<u32>) -> (usize, u128) {
        let start = Instant::now();
        let mut frozen_guard = self.frozen_pids.lock().unwrap();
        let mut global_guard = get_global_frozen().lock().unwrap();
        let mut count = 0;

        for &pid in pids {
            if frozen_guard.contains(&pid) {
                continue;
            }
            if kill(Pid::from_raw(pid as i32), Signal::SIGSTOP).is_ok() {
                frozen_guard.insert(pid);
                global_guard.insert(pid);
                count += 1;
            }
        }

        let elapsed = start.elapsed().as_micros();
        (count, elapsed)
    }

    /// Thaw all currently frozen PIDs using SIGCONT.
    /// Returns (number of thawed processes, latency in microseconds).
    pub fn thaw_all(&self) -> (usize, u128) {
        let start = Instant::now();
        let mut frozen_guard = self.frozen_pids.lock().unwrap();
        let mut global_guard = get_global_frozen().lock().unwrap();
        let mut count = 0;

        for &pid in frozen_guard.iter() {
            if kill(Pid::from_raw(pid as i32), Signal::SIGCONT).is_ok() {
                count += 1;
            }
            global_guard.remove(&pid);
        }

        frozen_guard.clear();

        let elapsed = start.elapsed().as_micros();
        (count, elapsed)
    }

    /// Pre-Wakeup Thaw: Instantly thaw processes when laptop lid opens.
    pub fn pre_wakeup_thaw(&self) -> (usize, u128) {
        self.thaw_all()
    }

    /// Pre-Suspend Thaw: Thaw processes and pause for 500ms before system suspend.
    pub async fn pre_suspend_thaw(&self) -> usize {
        let (count, _) = self.thaw_all();
        if count > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        count
    }

    /// Return the number of currently frozen processes.
    pub fn frozen_count(&self) -> usize {
        self.frozen_pids.lock().unwrap().len()
    }

    /// Return a clone of all currently frozen PIDs.
    pub fn get_frozen_pids(&self) -> HashSet<u32> {
        self.frozen_pids.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn test_freezer_lifecycle_with_dummy_process() {
        // Spawn a benign dummy child process (sleep 60)
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn dummy process");

        let child_pid = child.id();
        let freezer = Freezer::new();
        let mut targets = HashSet::new();
        targets.insert(child_pid);

        // 1. Freeze
        let (frozen_count, freeze_latency_us) = freezer.freeze_pids(&targets);
        assert_eq!(frozen_count, 1);
        assert_eq!(freezer.frozen_count(), 1);
        // Latency must be well under 50ms (50,000 microseconds)
        assert!(
            freeze_latency_us < 50_000,
            "Freeze latency {freeze_latency_us}us exceeded 50ms budget"
        );

        // 2. Thaw
        let (thaw_count, thaw_latency_us) = freezer.thaw_all();
        assert_eq!(thaw_count, 1);
        assert_eq!(freezer.frozen_count(), 0);
        // Pre-wakeup thaw latency must be well under 20ms (20,000 microseconds)
        assert!(
            thaw_latency_us < 20_000,
            "Thaw latency {thaw_latency_us}us exceeded 20ms budget"
        );

        // Cleanup
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_emergency_thaw_guarantee() {
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn dummy process");

        let child_pid = child.id();
        let freezer = Freezer::new();
        let mut targets = HashSet::new();
        targets.insert(child_pid);

        freezer.freeze_pids(&targets);
        assert_eq!(freezer.frozen_count(), 1);

        // Execute emergency thaw
        let thawed = emergency_thaw_all();
        assert!(thawed >= 1);

        // Cleanup
        let _ = child.kill();
        let _ = child.wait();
    }
}
