use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::classify::ClassifiedMsg;
use crate::parser::ongoing::{is_ongoing, ONGOING_STALENESS_THRESHOLD};
use crate::parser::session::read_session_incremental;
use crate::parser::subagent::{discover_subagents, discover_team_sessions, link_subagents, SubagentProcess};
use crate::parser::team::reconstruct_teams;

const WATCHER_DEBOUNCE: Duration = Duration::from_millis(500);

/// Handle for stopping the session watcher.
pub struct SessionWatcherHandle {
    stop_tx: mpsc::Sender<()>,
}

impl SessionWatcherHandle {
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }
}

/// Serializable session update event.
#[derive(Clone, serde::Serialize)]
struct SessionUpdatePayload {
    messages: Vec<DisplayMessage>,
    teams: Vec<crate::parser::team::TeamSnapshot>,
    ongoing: bool,
    permission_mode: String,
}

/// Start watching a session file. Emits "session-update" events on changes.
pub fn start_session_watcher(
    path: String,
    initial_classified: Vec<ClassifiedMsg>,
    initial_offset: u64,
    app: AppHandle,
) -> SessionWatcherHandle {
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let (signal_tx, mut signal_rx) = mpsc::channel::<()>(4);

    let path_clone = path.clone();
    let signal_tx_clone = signal_tx.clone();

    // Spawn the file watcher thread (notify requires std thread).
    std::thread::spawn(move || {
        let signal_tx = signal_tx_clone;
        let path = path_clone;

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(_) => return,
        };

        // Watch the session file.
        let _ = watcher.watch(Path::new(&path), RecursiveMode::NonRecursive);

        // Watch the project directory for new team session files.
        let project_dir = Path::new(&path).parent().unwrap_or(Path::new(""));
        let _ = watcher.watch(project_dir, RecursiveMode::NonRecursive);

        let mut debounce_timer: Option<std::time::Instant> = None;

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    let dominated = event.paths.iter().any(|p| {
                        p.to_string_lossy() == path
                            || p.extension().map(|e| e == "jsonl").unwrap_or(false)
                    });
                    if dominated {
                        debounce_timer = Some(std::time::Instant::now());
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if let Some(timer) = debounce_timer {
                if timer.elapsed() >= WATCHER_DEBOUNCE {
                    debounce_timer = None;
                    let _ = signal_tx.try_send(());
                }
            }
        }
    });

    // Spawn the async rebuild loop.
    let path_for_rebuild = path.clone();
    tauri::async_runtime::spawn(async move {
        let mut all_classified = initial_classified;
        let mut offset = initial_offset;

        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    break;
                }
                Some(()) = signal_rx.recv() => {
                    // Read any new data.
                    match read_session_incremental(&path_for_rebuild, offset) {
                        Ok((new_msgs, new_offset, _)) => {
                            if !new_msgs.is_empty() || new_offset != offset {
                                offset = new_offset;
                                all_classified.extend(new_msgs);
                            }
                        }
                        Err(_) => continue,
                    }

                    let chunks = build_chunks(&all_classified);

                    let subagents = discover_subagents(&path_for_rebuild).unwrap_or_default();
                    let team_procs = discover_team_sessions(&path_for_rebuild, &chunks).unwrap_or_default();
                    let mut all_procs: Vec<SubagentProcess> = subagents;
                    all_procs.extend(team_procs);
                    let color_map = link_subagents(&mut all_procs, &chunks, &path_for_rebuild);

                    let mut ongoing = is_ongoing(&chunks);
                    if !ongoing {
                        for proc in &all_procs {
                            if is_ongoing(&proc.chunks) {
                                let elapsed = chrono::Utc::now()
                                    .signed_duration_since(proc.file_mod_time)
                                    .to_std()
                                    .unwrap_or(Duration::ZERO);
                                if elapsed <= ONGOING_STALENESS_THRESHOLD {
                                    ongoing = true;
                                    break;
                                }
                            }
                        }
                    }

                    let teams = reconstruct_teams(&chunks, &all_procs);
                    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);

                    // Extract last permission_mode from UserMsg entries.
                    let mut permission_mode = String::from("default");
                    for msg in all_classified.iter().rev() {
                        if let ClassifiedMsg::User(u) = msg {
                            if !u.permission_mode.is_empty() {
                                permission_mode = u.permission_mode.clone();
                                break;
                            }
                        }
                    }

                    let payload = SessionUpdatePayload {
                        messages,
                        teams,
                        ongoing,
                        permission_mode,
                    };

                    let _ = app.emit("session-update", payload);
                }
            }
        }
    });

    SessionWatcherHandle { stop_tx }
}

/// Handle for stopping the picker watcher.
pub struct PickerWatcherHandle {
    stop_tx: mpsc::Sender<()>,
}

impl PickerWatcherHandle {
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }
}

/// Serializable picker refresh event.
#[derive(Clone, serde::Serialize)]
struct PickerRefreshPayload {
    sessions: Vec<crate::parser::session::SessionInfo>,
}

/// Start watching project directories for new/changed sessions.
pub fn start_picker_watcher(
    project_dirs: Vec<String>,
    app: AppHandle,
) -> PickerWatcherHandle {
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let (signal_tx, mut signal_rx) = mpsc::channel::<()>(4);

    let dirs_clone = project_dirs.clone();
    let signal_tx_clone = signal_tx.clone();

    // Spawn the file watcher thread.
    std::thread::spawn(move || {
        let signal_tx = signal_tx_clone;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(_) => return,
        };

        for dir in &dirs_clone {
            if Path::new(dir).exists() {
                let _ = watcher.watch(Path::new(dir), RecursiveMode::NonRecursive);
            }
        }

        let mut debounce_timer: Option<std::time::Instant> = None;

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    let dominated = event.paths.iter().any(|p| {
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        name.ends_with(".jsonl") && !name.starts_with("agent_")
                    });
                    if dominated {
                        debounce_timer = Some(std::time::Instant::now());
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if let Some(timer) = debounce_timer {
                if timer.elapsed() >= WATCHER_DEBOUNCE {
                    debounce_timer = None;
                    let _ = signal_tx.try_send(());
                }
            }
        }
    });

    // Spawn the async refresh loop.
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    break;
                }
                Some(()) = signal_rx.recv() => {
                    let sessions = crate::parser::session::discover_all_project_sessions(&project_dirs)
                        .unwrap_or_default();

                    let payload = PickerRefreshPayload { sessions };
                    let _ = app.emit("picker-refresh", payload);
                }
            }
        }
    });

    PickerWatcherHandle { stop_tx }
}
