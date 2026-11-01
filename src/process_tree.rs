//! Process Hierarchy and 3-Class Taxonomy Engine for Antigravity KeepAwake.
//!
//! Categorizes Linux processes into:
//! - Class 0: Golden Whitelist (Core Desktop, Audio, Display, D-Bus, systemd) - STRICT IMMUNITY
//! - Class 1: Antigravity Family (IDE, Language Server, and recursive children) - FULL PRIORITY
//! - Class 2: External Target Applications (Chrome, Spotify, Discord, etc.) - TARGET FOR FREEZING
//! - Other: Normal user and system tasks not targeted for freezing - UNTOUCHED

use std::collections::HashSet;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessClass {
    Class0, // Golden Whitelist (Core Desktop / System)
    Class1, // Antigravity Family (IDE, tools, compilers)
    Class2, // External Target Applications (Chrome, etc.)
    Other,  // Other non-targeted processes
}

pub const CLASS_0_WHITELIST: &[&str] = &[
    // Desktop Shell & Compositor
    "gnome-shell",
    "mutter",
    "xwayland",
    "gdm-wayland-session",
    "gdm-x-session",
    // Audio Subsystem
    "pipewire",
    "pipewire-pulse",
    "pipewire-media-session",
    "wireplumber",
    "pulseaudio",
    // IPC & System Daemons
    "dbus-daemon",
    "systemd",
    "systemd-logind",
    "systemd-journald",
    "systemd-udevd",
    "systemd-resolved",
    "polkitd",
    "upowerd",
    "rtkit-daemon",
    "accounts-daemon",
    "networkmanager",
];

pub const DEFAULT_CLASS_2_TARGETS: &[&str] = &[
    "google-chrome",
    "chrome",
    "chromium",
    "chromium-browser",
    "brave",
    "brave-browser",
    "firefox",
    "firefox-bin",
    "slack",
    "discord",
    "spotify",
    "telegram-desktop",
];

/// Collect all descendant PIDs under given root PIDs recursively.
fn collect_children_recursive(sys: &System, roots: &HashSet<u32>, out: &mut HashSet<u32>) {
    let mut current_level = roots.clone();
    while !current_level.is_empty() {
        let mut next_level = HashSet::new();
        for (pid, proc) in sys.processes() {
            let pid_u32 = pid.as_u32();
            if let Some(parent) = proc.parent() {
                let parent_u32 = parent.as_u32();
                if current_level.contains(&parent_u32) && !out.contains(&pid_u32) {
                    out.insert(pid_u32);
                    next_level.insert(pid_u32);
                }
            }
        }
        current_level = next_level;
    }
}

/// Dynamically traverse and return all PIDs belonging to the Antigravity family.
pub fn get_antigravity_family_pids(sys: &System) -> HashSet<u32> {
    let mut family_pids = HashSet::new();
    let mut roots = HashSet::new();

    // 1. Scan for root Antigravity IDE and Language Server processes
    for (pid, proc) in sys.processes() {
        let pid_u32 = pid.as_u32();
        let name_str = proc.name().to_string_lossy().to_lowercase();
        let cmdline_vec: Vec<String> = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_lowercase())
            .collect();
        let cmdline_str = cmdline_vec.join(" ");

        if name_str.contains("antigravity-ide")
            || cmdline_str.contains("/opt/antigravity")
            || (name_str.contains("language_server_linux") && cmdline_str.contains("antigravity"))
        {
            roots.insert(pid_u32);
            family_pids.insert(pid_u32);
        }
    }

    // 2. Add current process and its parent chain if any parent is an Antigravity process
    let current_pid = Pid::from_u32(std::process::id());
    let mut curr = Some(current_pid);
    let mut ancestors = Vec::new();
    let mut is_under_antigravity = false;

    while let Some(p) = curr {
        ancestors.push(p.as_u32());
        if let Some(proc) = sys.process(p) {
            let cmdline_str = proc
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            let name_str = proc.name().to_string_lossy().to_lowercase();

            if name_str.contains("antigravity-ide") || cmdline_str.contains("/opt/antigravity") {
                is_under_antigravity = true;
                break;
            }
            curr = proc.parent();
        } else {
            break;
        }
    }

    if is_under_antigravity {
        for a in ancestors {
            roots.insert(a);
            family_pids.insert(a);
        }
    }

    // 3. Collect all recursive children of roots
    collect_children_recursive(sys, &roots, &mut family_pids);

    // 4. Also unconditionally include the current process and its children
    let self_pid = std::process::id();
    family_pids.insert(self_pid);
    let mut self_set = HashSet::new();
    self_set.insert(self_pid);
    collect_children_recursive(sys, &self_set, &mut family_pids);

    family_pids
}

/// Classify a process according to the 3-Class Taxonomy.
pub fn classify_process(
    pid: u32,
    name: &str,
    cmdline: &[String],
    antigravity_pids: &HashSet<u32>,
    class2_targets: &HashSet<String>,
) -> ProcessClass {
    let clean_name = name.to_lowercase();
    let cmdline_str = cmdline.join(" ").to_lowercase();

    // Precedence 1: Explicitly tracked Antigravity family PID
    if antigravity_pids.contains(&pid) {
        return ProcessClass::Class1;
    }

    // Precedence 2: Internal Antigravity helper binaries / paths
    if cmdline_str.contains("antigravity") || cmdline_str.contains("/opt/antigravity") {
        return ProcessClass::Class1;
    }

    // Precedence 3: Class 0 Golden Whitelist (Core Desktop / Audio / System)
    for &whitelist in CLASS_0_WHITELIST {
        if clean_name == whitelist {
            return ProcessClass::Class0;
        }
    }

    // Precedence 4: Class 2 External User Target Applications
    for target in class2_targets {
        let t = target.to_lowercase();
        if clean_name == t || clean_name.contains(&t) || cmdline_str.contains(&format!("/{t}")) {
            return ProcessClass::Class2;
        }
    }

    // Precedence 5: Other normal system or user processes
    ProcessClass::Other
}

/// Scan and return set of PIDs eligible for SIGSTOP freezing.
pub fn get_freezable_pids(
    sys: &mut System,
    class2_targets: Option<&HashSet<String>>,
    current_user_only: bool,
) -> HashSet<u32> {
    sys.refresh_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()));

    let default_targets: HashSet<String> = DEFAULT_CLASS_2_TARGETS
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let targets = class2_targets.unwrap_or(&default_targets);

    let antigravity_pids = get_antigravity_family_pids(sys);
    let current_uid = nix::unistd::getuid();
    let mut freezable = HashSet::new();

    for (pid, proc) in sys.processes() {
        let pid_u32 = pid.as_u32();

        if current_user_only {
            if let Some(user_id) = proc.user_id() {
                if **user_id != current_uid.as_raw() {
                    continue;
                }
            } else {
                continue;
            }
        }

        let name = proc.name().to_string_lossy();
        let cmdline: Vec<String> = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        let pclass = classify_process(pid_u32, &name, &cmdline, &antigravity_pids, targets);
        if pclass == ProcessClass::Class2 {
            freezable.insert(pid_u32);
        }
    }

    freezable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class0_golden_whitelist_coverage() {
        let expected = [
            "gnome-shell",
            "mutter",
            "xwayland",
            "pipewire",
            "wireplumber",
            "pulseaudio",
            "dbus-daemon",
            "systemd",
            "systemd-logind",
        ];
        for name in expected {
            assert!(
                CLASS_0_WHITELIST.contains(&name),
                "Missing expected Class 0 whitelist item: {name}"
            );
        }
    }

    #[test]
    fn test_classify_class0_processes() {
        let targets: HashSet<String> = ["google-chrome", "spotify"]
            .iter()
            .map(|&s| s.to_string())
            .collect();
        let antigravity_pids = HashSet::new();

        for name in [
            "gnome-shell",
            "pipewire",
            "wireplumber",
            "dbus-daemon",
            "xwayland",
        ] {
            let pclass = classify_process(
                99999,
                name,
                &[name.to_string()],
                &antigravity_pids,
                &targets,
            );
            assert_eq!(pclass, ProcessClass::Class0, "Failed for {name}");
        }
    }

    #[test]
    fn test_classify_antigravity_family() {
        let mut antigravity_pids = HashSet::new();
        antigravity_pids.insert(1001);
        antigravity_pids.insert(1002);
        antigravity_pids.insert(1003);

        let targets: HashSet<String> = ["google-chrome"].iter().map(|&s| s.to_string()).collect();

        // Root Antigravity IDE
        let pclass_root = classify_process(
            1001,
            "antigravity-ide",
            &["/opt/antigravity/antigravity-ide".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass_root, ProcessClass::Class1);

        // Child process (e.g., cargo or git or pytest)
        let pclass_child = classify_process(
            1003,
            "cargo",
            &["cargo".to_string(), "test".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass_child, ProcessClass::Class1);
    }

    #[test]
    fn test_antigravity_chromium_helper_never_frozen() {
        let mut antigravity_pids = HashSet::new();
        antigravity_pids.insert(2001);
        antigravity_pids.insert(2002);

        let targets: HashSet<String> = ["google-chrome", "chrome", "chromium"]
            .iter()
            .map(|&s| s.to_string())
            .collect();

        let pclass = classify_process(
            2002,
            "chrome_crashpad_handler",
            &[
                "/opt/antigravity/chrome_crashpad_handler".to_string(),
                "--database=...".to_string(),
            ],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass, ProcessClass::Class1);
    }

    #[test]
    fn test_classify_class2_apps() {
        let targets: HashSet<String> = ["google-chrome", "spotify", "discord", "slack"]
            .iter()
            .map(|&s| s.to_string())
            .collect();
        let antigravity_pids = HashSet::new();

        let pclass_chrome = classify_process(
            5555,
            "google-chrome",
            &["/opt/google/chrome/chrome".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass_chrome, ProcessClass::Class2);

        let pclass_spotify = classify_process(
            5556,
            "spotify",
            &["/usr/share/spotify/spotify".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass_spotify, ProcessClass::Class2);
    }

    #[test]
    fn test_classify_other_process() {
        let targets: HashSet<String> = ["google-chrome", "spotify"]
            .iter()
            .map(|&s| s.to_string())
            .collect();
        let antigravity_pids = HashSet::new();

        let pclass_gedit = classify_process(
            7777,
            "gedit",
            &["gedit".to_string(), "file.txt".to_string()],
            &antigravity_pids,
            &targets,
        );
        assert_eq!(pclass_gedit, ProcessClass::Other);
    }

    #[test]
    fn test_get_antigravity_family_pids_on_live_system() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let pids = get_antigravity_family_pids(&sys);
        assert!(!pids.is_empty(), "Expected to detect Antigravity processes");
        let self_pid = std::process::id();
        assert!(
            pids.contains(&self_pid),
            "Current process PID {self_pid} must be in the family tree"
        );
    }

    #[test]
    fn test_get_freezable_pids() {
        let mut sys = System::new_all();
        let mut dummy_targets = HashSet::new();
        dummy_targets.insert("nonexistent_dummy_app_xyz_123".to_string());

        let freezable = get_freezable_pids(&mut sys, Some(&dummy_targets), true);
        assert!(
            freezable.is_empty(),
            "Nonexistent app should have 0 freezable PIDs"
        );
    }
}
