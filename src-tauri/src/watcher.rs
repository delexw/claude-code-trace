use std::sync::Arc;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::classify::ClassifiedMsg;
use crate::parser::ongoing::OngoingChecker;
use crate::parser::session::{read_session_with_debug_hooks, IncrementalTokenScanner};
use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};
use crate::parser::team::reconstruct_teams;
use crate::state::AppState;

const WATCHER_DEBOUNCE: Duration = Duration::from_millis(1000);

/// Run a debounced file-change loop: receive notify events, apply `filter`,
/// and send a signal after `WATCHER_DEBOUNCE` of quiet time.
/// Exits when `thread_stop_rx` receives a value or is disconnected.
fn run_debounce_loop(
    rx: std::sync::mpsc::Receiver<Result<notify::Event, notify::Error>>,
    filter: impl Fn(&notify::Event) -> bool,
    signal_tx: mpsc::Sender<()>,
    thread_stop_rx: std::sync::mpsc::Receiver<()>,
) {
    let mut debounce_timer: Option<std::time::Instant> = None;

    loop {
        // Check for an explicit stop signal before blocking on notify events.
        match thread_stop_rx.try_recv() {
            Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

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
    /// Stops the async rebuild task.
    stop_tx: mpsc::Sender<()>,
    /// Stops the underlying std::thread running notify + debounce, releasing OS watcher fds.
    thread_stop_tx: std::sync::mpsc::SyncSender<()>,
}

impl WatcherHandle {
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
        let _ = self.thread_stop_tx.try_send(());
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
    state: Arc<AppState>,
    app: Option<AppHandle>,
) -> WatcherHandle {
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let (signal_tx, mut signal_rx) = mpsc::channel::<()>(4);
    let (thread_stop_tx, thread_stop_rx) = std::sync::mpsc::sync_channel::<()>(1);

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

        // Watch the project directory recursively — catches the session file,
        // team session files, and subagent files in any subdirectory (including
        // subagent directories created after the watcher starts).
        let project_dir = Path::new(&path).parent().unwrap_or(Path::new(""));
        let _ = watcher.watch(project_dir, RecursiveMode::Recursive);

        // Only react to changes in this session's files — not other sessions.
        let session_file = path.clone();
        let session_base = Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let project_dir_str = project_dir.to_string_lossy().to_string();

        run_debounce_loop(
            rx,
            move |event| {
                event.paths.iter().any(|p| {
                    let ps = p.to_string_lossy();
                    // Exact match on the session file.
                    if ps == session_file {
                        return true;
                    }
                    // Files inside this session's subdirectory (subagents, etc.).
                    if let Some(parent) = p.parent() {
                        let parent_str = parent.to_string_lossy();
                        if parent_str.contains(&session_base) {
                            return p.extension().map(|e| e == "jsonl").unwrap_or(false);
                        }
                    }
                    // New team session files directly in the project directory.
                    if let Some(parent) = p.parent() {
                        if parent.to_string_lossy() == project_dir_str {
                            return p.extension().map(|e| e == "jsonl").unwrap_or(false);
                        }
                    }
                    false
                })
            },
            signal_tx,
            thread_stop_rx,
        );
        // watcher dropped here → OS watcher fd released
    });

    // Spawn the async rebuild loop.
    let path_for_rebuild = path.clone();
    tokio::spawn(async move {
        let mut token_scanner = IncrementalTokenScanner::new();
        let mut prev_msg_count: usize = 0;
        let mut prev_item_count: usize = 0;
        let mut prev_ongoing = false;
        let mut prev_file_size: u64 = 0;

        // Seed the token scanner with the initial file content.
        token_scanner.scan_new_bytes(&path_for_rebuild);

        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    break;
                }
                Some(()) = signal_rx.recv() => {
                    // Detect file truncation (e.g. after /clear): reset state
                    // so the next emit fires with the fresh content.
                    let file_size = std::fs::metadata(&path_for_rebuild)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    if file_size < prev_file_size {
                        prev_msg_count = 0;
                        prev_item_count = 0;
                        token_scanner = IncrementalTokenScanner::new();
                    }
                    prev_file_size = file_size;

                    // Re-read the full session from scratch on every event.
                    // Using a local variable (dropped at end of each iteration)
                    // avoids holding all classified messages in memory for the
                    // entire session lifetime — which caused multi-GB growth
                    // for long sessions with large tool inputs/outputs.
                    let all_classified = match read_session_with_debug_hooks(&path_for_rebuild) {
                        Ok((msgs, _, _)) => msgs,
                        Err(_) => continue,
                    };

                    let mut chunks = build_chunks(&all_classified);

                    let (mut all_procs, color_map) = discover_and_link_all(&path_for_rebuild, &chunks);
                    inject_orphan_subagents(&mut chunks, &mut all_procs);

                    let ongoing = OngoingChecker::new(&chunks, &all_procs, &path_for_rebuild).is_ongoing();

                    // Share ongoing status with AppState so the picker can use it.
                    state.set_watched_ongoing(path_for_rebuild.clone(), ongoing);

                    let teams = reconstruct_teams(&chunks, &all_procs);
                    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);

                    // Skip emit if nothing meaningful changed.
                    // Track both message count and total item count so that hook
                    // events (which are added inside existing AI chunks without
                    // creating a new top-level message) still trigger an emit.
                    let msg_count = messages.len();
                    let item_count: usize = messages.iter().map(|m| m.items.len()).sum();
                    if msg_count == prev_msg_count
                        && item_count == prev_item_count
                        && !ongoing
                        && !prev_ongoing
                    {
                        // Token totals may still have changed — update scanner
                        // but skip the expensive emit + serialize.
                        token_scanner.scan_new_bytes(&path_for_rebuild);
                        continue;
                    }
                    prev_msg_count = msg_count;
                    prev_item_count = item_count;
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

                    // Broadcast to SSE clients (HTTP API).
                    if let Ok(json) = serde_json::to_string(&payload) {
                        state.broadcast("session-update", &json);
                    }

                    if let Some(ref app_handle) = app {
                        let _ = app_handle.emit("session-update", payload);
                    }
                }
            }
        }
    });

    WatcherHandle {
        stop_tx,
        thread_stop_tx,
    }
}

/// Start watching project directories for new/changed sessions.
/// When changes are detected the watcher broadcasts a lightweight `picker-refresh`
/// signal with no payload. Clients are responsible for fetching the updated
/// session list via the `discover_sessions` / `/api/sessions` endpoint, which uses
/// a short-lived cache to coalesce concurrent re-fetches.
pub fn start_picker_watcher(
    project_dirs: Vec<String>,
    state: Arc<AppState>,
    app: Option<AppHandle>,
) -> WatcherHandle {
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let (signal_tx, mut signal_rx) = mpsc::channel::<()>(4);
    let (thread_stop_tx, thread_stop_rx) = std::sync::mpsc::sync_channel::<()>(1);

    // Derive unique parent directories (e.g. ~/.claude/projects) from the
    // individual project dirs. Watching the parent instead of individual
    // subdirs ensures newly created project directories are detected
    // automatically — no re-registration needed when new projects appear.
    let mut seen_parents = std::collections::HashSet::new();
    let base_dirs: Vec<std::path::PathBuf> = project_dirs
        .iter()
        .filter_map(|d| Path::new(d).parent().map(|p| p.to_path_buf()))
        .filter(|p| seen_parents.insert(p.clone()))
        .collect();
    // Fallback to individual dirs if parent derivation yielded nothing.
    let watch_dirs: Vec<std::path::PathBuf> = if base_dirs.is_empty() {
        project_dirs.iter().map(std::path::PathBuf::from).collect()
    } else {
        base_dirs.clone()
    };

    let signal_tx_clone = signal_tx.clone();

    // Spawn the file watcher thread.
    std::thread::spawn(move || {
        let signal_tx = signal_tx_clone;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(_) => return,
        };

        for dir in &watch_dirs {
            if dir.exists() {
                let _ = watcher.watch(dir, RecursiveMode::Recursive);
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
            thread_stop_rx,
        );
        // watcher dropped here → OS watcher fd released
    });

    // Spawn the async refresh loop.
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    break;
                }
                Some(()) = signal_rx.recv() => {
                    // Send a lightweight signal — no session data embedded.
                    // Clients call discover_sessions to fetch fresh data; the
                    // server-side cache coalesces concurrent requests.
                    state.broadcast("picker-refresh", "{}");

                    if let Some(ref app_handle) = app {
                        let _ = app_handle.emit("picker-refresh", serde_json::json!({}));
                    }
                }
            }
        }
    });

    WatcherHandle {
        stop_tx,
        thread_stop_tx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    /// run_debounce_loop must exit when the stop signal is sent.
    #[test]
    fn debounce_loop_exits_on_stop_signal() {
        let (signal_tx, _signal_rx) = mpsc::channel::<()>(4);
        let (thread_stop_tx, thread_stop_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let (_notify_tx, notify_rx) = std::sync::mpsc::channel();

        thread_stop_tx.send(()).unwrap();

        let handle = std::thread::spawn(move || {
            run_debounce_loop(notify_rx, |_| false, signal_tx, thread_stop_rx);
        });

        handle
            .join()
            .expect("debounce thread should exit after stop signal");
    }

    /// run_debounce_loop must exit when the stop sender is dropped (Disconnected).
    #[test]
    fn debounce_loop_exits_when_stop_sender_dropped() {
        let (signal_tx, _signal_rx) = mpsc::channel::<()>(4);
        let (thread_stop_tx, thread_stop_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let (_notify_tx, notify_rx) = std::sync::mpsc::channel();

        drop(thread_stop_tx);

        let handle = std::thread::spawn(move || {
            run_debounce_loop(notify_rx, |_| false, signal_tx, thread_stop_rx);
        });

        handle
            .join()
            .expect("debounce thread should exit when stop sender is dropped");
    }

    /// WatcherHandle::stop() must not panic when called multiple times on a closed channel.
    #[test]
    fn watcher_handle_stop_is_idempotent() {
        let (stop_tx, _stop_rx) = mpsc::channel::<()>(1);
        let (thread_stop_tx, _thread_stop_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let handle = WatcherHandle {
            stop_tx,
            thread_stop_tx,
        };
        handle.stop();
        handle.stop();
    }
}
