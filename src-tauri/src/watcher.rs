use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::classify::ClassifiedMsg;
use crate::parser::ongoing::OngoingChecker;
use crate::parser::session::{read_session_incremental, IncrementalTokenScanner};
use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};
use crate::parser::team::reconstruct_teams;

const WATCHER_DEBOUNCE: Duration = Duration::from_millis(200);

/// Run a debounced file-change loop: receive notify events, apply `filter`,
/// and send a signal after `WATCHER_DEBOUNCE` of quiet time.
fn run_debounce_loop(
    rx: std::sync::mpsc::Receiver<Result<notify::Event, notify::Error>>,
    filter: impl Fn(&notify::Event) -> bool,
    signal_tx: mpsc::Sender<()>,
) {
    let mut debounce_timer: Option<std::time::Instant> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if filter(&event) {
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
}

/// Handle for stopping a file watcher (session or picker).
pub struct WatcherHandle {
    stop_tx: mpsc::Sender<()>,
}

impl WatcherHandle {
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
    session_totals: crate::convert::SessionTotals,
}

/// Start watching a session file. Emits "session-update" events on changes.
pub fn start_session_watcher(
    path: String,
    initial_classified: Vec<ClassifiedMsg>,
    initial_offset: u64,
    app: AppHandle,
) -> WatcherHandle {
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

        // Watch the subagents directory so ongoing dot updates quickly.
        let subagents_dir = crate::parser::subagent::subagents_dir(&path);
        if subagents_dir.exists() {
            let _ = watcher.watch(&subagents_dir, RecursiveMode::NonRecursive);
        }

        run_debounce_loop(
            rx,
            |event| {
                event.paths.iter().any(|p| {
                    p.to_string_lossy() == path
                        || p.extension().map(|e| e == "jsonl").unwrap_or(false)
                })
            },
            signal_tx,
        );
    });

    // Spawn the async rebuild loop.
    let path_for_rebuild = path.clone();
    tauri::async_runtime::spawn(async move {
        let mut all_classified = initial_classified;
        let mut offset = initial_offset;
        let mut token_scanner = IncrementalTokenScanner::new();
        let mut prev_msg_count: usize = 0;
        let mut prev_ongoing = false;

        // Seed the token scanner with the initial file content.
        token_scanner.scan_new_bytes(&path_for_rebuild);

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

                    let mut chunks = build_chunks(&all_classified);

                    let (mut all_procs, color_map) = discover_and_link_all(&path_for_rebuild, &chunks);
                    inject_orphan_subagents(&mut chunks, &mut all_procs);

                    let ongoing = OngoingChecker::new(&chunks, &all_procs, &path_for_rebuild).is_ongoing();

                    let teams = reconstruct_teams(&chunks, &all_procs);
                    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);

                    // Skip emit if nothing meaningful changed.
                    let msg_count = messages.len();
                    if msg_count == prev_msg_count && !ongoing && !prev_ongoing {
                        // Token totals may still have changed — update scanner
                        // but skip the expensive emit + serialize.
                        token_scanner.scan_new_bytes(&path_for_rebuild);
                        continue;
                    }
                    prev_msg_count = msg_count;
                    prev_ongoing = ongoing;

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

                    // Incrementally scan only new bytes for token totals.
                    let session_totals = token_scanner.scan_new_bytes(&path_for_rebuild);

                    let payload = SessionUpdatePayload {
                        messages,
                        teams,
                        ongoing,
                        permission_mode,
                        session_totals,
                    };

                    let _ = app.emit("session-update", payload);
                }
            }
        }
    });

    WatcherHandle { stop_tx }
}

/// Serializable picker refresh event.
#[derive(Clone, serde::Serialize)]
struct PickerRefreshPayload {
    sessions: Vec<crate::parser::session::SessionInfo>,
}

/// Start watching project directories for new/changed sessions.
pub fn start_picker_watcher(project_dirs: Vec<String>, app: AppHandle) -> WatcherHandle {
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
                let _ = watcher.watch(Path::new(dir), RecursiveMode::Recursive);
            }
        }

        run_debounce_loop(
            rx,
            |event| {
                event.paths.iter().any(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.ends_with(".jsonl")
                })
            },
            signal_tx,
        );
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

    WatcherHandle { stop_tx }
}
