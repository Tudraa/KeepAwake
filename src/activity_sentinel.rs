//! Activity Sentinel via kernel inotify and child process monitoring.
//!
//! Event-driven detection of Antigravity coding tasks:
//! - Watches `~/.gemini/antigravity-ide/brain/*/logs/transcript.jsonl` via `notify`.
//! - Parses JSONL step entries to detect active tool execution vs idle prompt waiting.
//! - Verifies child process activity (compilers, test runners, git).
//! - Manages 2-minute Grace Period countdown before confirming completion.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    Active,
    GracePeriod { remaining_secs: u64 },
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepType {
    ActiveTool,
    TurnDone,
    WaitingPrompt,
    Unknown,
}

/// Parse the trailing entry of transcript.jsonl to determine agent execution status.
pub fn parse_step_status(line: &str) -> StepType {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return StepType::Unknown;
    }

    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        let step_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("");

        // Active tools in flight
        if status == "IN_PROGRESS"
            || step_type == "RUN_COMMAND"
            || step_type == "BROWSER_SUBAGENT"
            || step_type == "GENERATE_IMAGE"
        {
            return StepType::ActiveTool;
        }

        // Check if planner response has pending unexecuted tool calls
        if step_type == "PLANNER_RESPONSE" {
            if let Some(tool_calls) = val.get("tool_calls").and_then(|tc| tc.as_array())
                && !tool_calls.is_empty()
                && status != "DONE"
            {
                return StepType::ActiveTool;
            }
            if status == "DONE" {
                return StepType::TurnDone;
            }
        }

        if step_type == "USER_INPUT" {
            return StepType::WaitingPrompt;
        }
    }

    StepType::Unknown
}

pub struct ActivitySentinel {
    brain_dir: PathBuf,
    grace_period_duration: Duration,
    state: Mutex<ActivityState>,
    last_active_time: Mutex<Option<Instant>>,
    audio_chime_enabled: AtomicBool,
}

impl ActivitySentinel {
    pub fn new(brain_dir: Option<PathBuf>, grace_period_secs: u64) -> Arc<Self> {
        let brain_path = brain_dir.unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".gemini/antigravity-ide/brain"))
                .unwrap_or_else(|| PathBuf::from("/tmp"))
        });

        Arc::new(Self {
            brain_dir: brain_path,
            grace_period_duration: Duration::from_secs(grace_period_secs),
            state: Mutex::new(ActivityState::Idle),
            last_active_time: Mutex::new(None),
            audio_chime_enabled: AtomicBool::new(true),
        })
    }

    pub fn set_audio_chime(&self, enabled: bool) {
        self.audio_chime_enabled.store(enabled, Ordering::SeqCst);
    }

    /// Read the latest modified transcript.jsonl under the brain directory.
    pub fn find_latest_transcript(&self) -> Option<PathBuf> {
        if !self.brain_dir.exists() {
            return None;
        }

        let mut latest_file = None;
        let mut latest_mtime = None;

        if let Ok(entries) = std::fs::read_dir(&self.brain_dir) {
            for entry in entries.flatten() {
                let transcript_path = entry.path().join(".system_generated/logs/transcript.jsonl");
                if transcript_path.is_file()
                    && let Ok(meta) = std::fs::metadata(&transcript_path)
                    && let Ok(mtime) = meta.modified()
                    && (latest_mtime.is_none() || Some(mtime) > latest_mtime)
                {
                    latest_mtime = Some(mtime);
                    latest_file = Some(transcript_path);
                }
            }
        }

        latest_file
    }

    /// Inspect transcript file and return current step status.
    pub fn inspect_transcript_file(path: &Path) -> StepType {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines().rev() {
                let step = parse_step_status(line);
                if step != StepType::Unknown {
                    return step;
                }
            }
        }
        StepType::Unknown
    }

    /// Return current activity state.
    pub async fn get_state(&self) -> ActivityState {
        *self.state.lock().await
    }

    /// Update activity state based on transcript and timer check.
    pub async fn update_status(&self, has_active_children: bool) -> ActivityState {
        let mut state_guard = self.state.lock().await;

        let transcript_step = self
            .find_latest_transcript()
            .map(|p| Self::inspect_transcript_file(&p))
            .unwrap_or(StepType::Unknown);

        let is_active = has_active_children || transcript_step == StepType::ActiveTool;

        if is_active {
            *self.last_active_time.lock().await = Some(Instant::now());
            *state_guard = ActivityState::Active;
            return ActivityState::Active;
        }

        // If turn is done or idle waiting, check grace period countdown
        let mut last_active_guard = self.last_active_time.lock().await;
        if let Some(last_time) = *last_active_guard {
            let elapsed = last_time.elapsed();
            if elapsed < self.grace_period_duration {
                let remaining = (self.grace_period_duration - elapsed).as_secs();
                let new_state = ActivityState::GracePeriod {
                    remaining_secs: remaining,
                };
                *state_guard = new_state;
                new_state
            } else {
                *last_active_guard = None;
                *state_guard = ActivityState::Idle;
                ActivityState::Idle
            }
        } else {
            *state_guard = ActivityState::Idle;
            ActivityState::Idle
        }
    }

    /// Play an optional subtle chime sound on task completion.
    pub async fn play_chime_sound(&self) {
        if !self.audio_chime_enabled.load(Ordering::SeqCst) {
            return;
        }
        // Use paplay / canberra-gtk-play / aplay if available
        let _ = tokio::process::Command::new("canberra-gtk-play")
            .args(["-i", "bell"])
            .status()
            .await;
    }
}

// Re-export dirs for path resolution
mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_active_tool_step() {
        let line = r#"{"step_index":14,"source":"MODEL","type":"RUN_COMMAND","status":"IN_PROGRESS","content":"pytest"}"#;
        assert_eq!(parse_step_status(line), StepType::ActiveTool);

        let planner_running = r#"{"step_index":15,"source":"MODEL","type":"PLANNER_RESPONSE","status":"IN_PROGRESS","tool_calls":[{"name":"bash"}]}"#;
        assert_eq!(parse_step_status(planner_running), StepType::ActiveTool);
    }

    #[test]
    fn test_parse_turn_done_step() {
        let line = r#"{"step_index":20,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","tool_calls":[]}"#;
        assert_eq!(parse_step_status(line), StepType::TurnDone);
    }

    #[test]
    fn test_parse_user_input_step() {
        let line = r#"{"step_index":21,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","content":"hello"}"#;
        assert_eq!(parse_step_status(line), StepType::WaitingPrompt);
    }

    #[tokio::test(start_paused = true)]
    async fn test_sentinel_lifecycle_with_temp_brain() {
        let dir = tempdir().unwrap();
        let conv_dir = dir.path().join("test-conv/.system_generated/logs");
        std::fs::create_dir_all(&conv_dir).unwrap();
        let log_file = conv_dir.join("transcript.jsonl");

        let sentinel = ActivitySentinel::new(Some(dir.path().to_path_buf()), 2);

        // 1. Initial empty -> Idle
        let state = sentinel.update_status(false).await;
        assert_eq!(state, ActivityState::Idle);

        // 2. Active tool written -> Active
        {
            let mut file = std::fs::File::create(&log_file).unwrap();
            writeln!(
                file,
                r#"{{"type":"RUN_COMMAND","status":"IN_PROGRESS","content":"cargo build"}}"#
            )
            .unwrap();
        }
        let state = sentinel.update_status(false).await;
        assert_eq!(state, ActivityState::Active);

        // 3. Task finished -> Grace Period (2 seconds)
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_file)
                .unwrap();
            writeln!(file, r#"{{"type":"PLANNER_RESPONSE","status":"DONE"}}"#).unwrap();
        }
        let state = sentinel.update_status(false).await;
        match state {
            ActivityState::GracePeriod { remaining_secs } => {
                assert!(remaining_secs <= 2);
            }
            other => panic!("Expected GracePeriod, got {:?}", other),
        }

        // 4. Advance time by 3s -> Idle
        tokio::time::advance(Duration::from_secs(3)).await;
        let state = sentinel.update_status(false).await;
        assert_eq!(state, ActivityState::Idle);
    }
}
