//! Focus the terminal window/tab a live session is running in.
//!
//! v1 scope: Terminal.app only (macOS). Other terminal apps resolve to
//! `FocusError::Unsupported` — a small, explicit registry (`TerminalAdapter`)
//! keeps adding a new terminal a matter of implementing the trait, not
//! branching deeper into this command. Shells out via `std::process::Command`
//! (`ps`, `osascript`), matching the codebase's existing OS-integration idiom
//! (git.rs, wsl.rs, process.rs) — no new dependency, no `unsafe`.

use crate::process::is_pid_alive;

/// Why focusing a session's terminal window failed.
#[derive(Debug)]
pub enum FocusError {
    /// No adapter recognizes the detected terminal app.
    Unsupported(String),
    /// The session's pid is no longer running.
    NotLive,
    /// Could not resolve a controlling terminal app for the pid.
    NoTerminal,
    /// The adapter's shell-out (e.g. `osascript`) failed or exited non-zero.
    Osascript(String),
}

impl FocusError {
    /// The user-facing message for this failure. Kept as a single pure
    /// mapping so every failure path produces a distinct, accurate message —
    /// in particular so a runtime `osascript` failure on a *supported*
    /// terminal is never confused with an actually unsupported terminal app.
    pub fn user_message(&self) -> String {
        match self {
            FocusError::Unsupported(app) => format!("Focus for {app} not supported yet"),
            FocusError::Osascript(_) => {
                "Couldn't focus the terminal window — it may have closed, or Terminal needs \
                 Automation permission"
                    .to_string()
            }
            FocusError::NotLive => "Session is not currently running".to_string(),
            FocusError::NoTerminal => "Couldn't find the session's terminal".to_string(),
        }
    }
}

impl From<FocusError> for String {
    fn from(e: FocusError) -> Self {
        e.user_message()
    }
}

/// A terminal application this command knows how to bring to the front.
pub trait TerminalAdapter {
    /// True if `app` (the detected terminal app name, e.g. `"Terminal"`) is
    /// handled by this adapter.
    fn matches(&self, app: &str) -> bool;
    /// Bring the window/tab associated with `pid`/`tty` to the front.
    fn focus(&self, pid: i64, tty: &str) -> Result<(), FocusError>;
}

struct TerminalAppAdapter;

impl TerminalAdapter for TerminalAppAdapter {
    fn matches(&self, app: &str) -> bool {
        app == "Terminal"
    }

    fn focus(&self, _pid: i64, tty: &str) -> Result<(), FocusError> {
        let script = osascript_focus_for_tty(tty);
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| {
                let detail = e.to_string();
                eprintln!("Focus: failed to spawn osascript: {detail}");
                FocusError::Osascript(detail)
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!("exit code {}", output.status)
        } else {
            format!("exit code {}: {}", output.status, stderr.trim())
        };
        eprintln!("Focus: osascript failed: {detail}");
        Err(FocusError::Osascript(detail))
    }
}

/// The AppleScript that brings the Terminal.app tab whose tty matches `tty` to
/// the front. Pure string-building — kept separate from the adapter's
/// `focus()` so it can be unit-tested without shelling out to `osascript`
/// (which would steal window focus during a test run).
pub fn osascript_focus_for_tty(tty: &str) -> String {
    format!(
        // Guard each window with `try`: Terminal.app has windows without tabs
        // (e.g. the Settings window), and `tabs of w` on those throws -1728,
        // which would abort the whole script. Skip such windows instead.
        r#"tell application "Terminal"
  repeat with w in windows
    try
      repeat with t in tabs of w
        if (tty of t) is "{tty}" then
          set selected of t to true
          set frontmost of w to true
          activate
        end if
      end repeat
    end try
  end repeat
end tell"#
    )
}

/// The adapter registry. v1 = Terminal.app only; add new adapters here as
/// support grows.
fn adapters() -> Vec<Box<dyn TerminalAdapter>> {
    vec![Box::new(TerminalAppAdapter)]
}

/// Select the adapter that handles `app` (the detected terminal app name), or
/// `None` if no adapter supports it yet.
pub fn pick_adapter(app: &str) -> Option<Box<dyn TerminalAdapter>> {
    adapters().into_iter().find(|a| a.matches(app))
}

/// Walk up the process tree from `pid` via `ps -o ppid=,comm=` looking for the
/// nearest ancestor whose command path ends in a `.app` bundle (e.g.
/// `/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal`),
/// returning the app's bundle name (e.g. `"Terminal"`). Returns `"unknown"`
/// if no `.app` ancestor is found or the walk fails.
pub fn detect_terminal_app(pid: i64) -> String {
    let mut current = pid;
    // Bounded walk: a runaway ppid chain (or a cycle from a stale/reused pid)
    // must not loop forever.
    for _ in 0..32 {
        if current <= 1 {
            break;
        }
        let output = match std::process::Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &current.to_string()])
            .output()
        {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                // Distinct from a normal "walked off the top of the tree":
                // `ps` ran but rejected the pid/args. Log it so a broken `ps`
                // doesn't silently masquerade as "no .app ancestor found".
                eprintln!(
                    "detect_terminal_app: `ps -p {current}` exited {}: {}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr).trim()
                );
                break;
            }
            Err(e) => {
                eprintln!("detect_terminal_app: failed to spawn `ps -p {current}`: {e}");
                break;
            }
        };
        let line = String::from_utf8_lossy(&output.stdout);
        let line = line.trim();
        let Some((ppid_str, comm)) = line.split_once(' ') else {
            break;
        };
        let comm = comm.trim();
        if let Some(name) = app_name_from_comm(comm) {
            return name;
        }
        current = match ppid_str.trim().parse::<i64>() {
            Ok(p) => p,
            Err(_) => break,
        };
    }
    "unknown".to_string()
}

/// Extract the `.app` bundle name from a `comm` path, e.g.
/// `/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal` -> `Terminal`.
fn app_name_from_comm(comm: &str) -> Option<String> {
    comm.split('/')
        .find_map(|segment| segment.strip_suffix(".app").map(|name| name.to_string()))
}

/// Resolve the controlling tty for `pid` via `ps -o tty= -p <pid>`, e.g.
/// `"ttys002"` -> `"/dev/ttys002"`. `None` if `ps` fails or reports no tty
/// (`"??"`, detached processes).
pub fn resolve_tty(pid: i64) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tty.is_empty() || tty == "??" {
        return None;
    }
    Some(format!("/dev/{tty}"))
}

/// Bring the terminal window/tab that owns a live session's process to the
/// front. Exposed both as a Tauri command (desktop) and, via
/// `focus_session_window_impl`, over the HTTP API (`POST /api/focus`) — any
/// frontend whose backend is local + macOS can focus a terminal window, not
/// just the Tauri app.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn focus_session_window(session_id: String) -> Result<(), String> {
    focus_session_window_impl(&session_id).map_err(|e| e.user_message())
}

/// Whether this backend can focus a session's terminal window at all, i.e.
/// whether a `TerminalAdapter` exists for the current platform. v1 ships only
/// `TerminalAppAdapter` (macOS Terminal.app), so this is currently equivalent
/// to "running on macOS". It is NOT a guarantee that a given `focus_session_window`
/// call will succeed — that can still fail for other reasons (session not
/// live, terminal app not recognized, Automation permission denied, etc.) —
/// only that the capability exists in principle on this platform.
pub fn can_focus() -> bool {
    cfg!(target_os = "macos")
}

pub fn focus_session_window_impl(session_id: &str) -> Result<(), FocusError> {
    let pid = crate::parser::session::live_pid_for(session_id).ok_or(FocusError::NotLive)?;
    if !is_pid_alive(pid) {
        return Err(FocusError::NotLive);
    }
    let app = detect_terminal_app(pid);
    let tty = resolve_tty(pid).ok_or(FocusError::NoTerminal)?;
    let adapter = pick_adapter(&app).ok_or(FocusError::Unsupported(app))?;
    adapter.focus(pid, &tty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osascript_targets_the_tty_tab() {
        let s = osascript_focus_for_tty("/dev/ttys002");
        assert!(s.contains("/dev/ttys002"));
        assert!(s.contains("set frontmost"));
        assert!(s.contains("activate"));
        // must guard windows without tabs (Settings window) so one -1728 doesn't abort
        assert!(
            s.contains("try"),
            "per-window tab enumeration must be wrapped in try"
        );
    }

    #[test]
    fn unknown_app_selects_unsupported() {
        assert!(pick_adapter("Ghostty").is_none()); // v1: only Terminal.app supported
        assert!(pick_adapter("Terminal").is_some());
    }

    #[test]
    fn focus_error_messages_are_distinct_and_accurate() {
        let unsupported = FocusError::Unsupported("Ghostty".to_string()).user_message();
        assert_eq!(unsupported, "Focus for Ghostty not supported yet");

        let osascript = FocusError::Osascript("nonzero".to_string()).user_message();
        assert!(!osascript.to_lowercase().contains("not supported yet"));
        assert!(osascript.to_lowercase().contains("automation permission"));

        let not_live = FocusError::NotLive.user_message();
        assert_eq!(not_live, "Session is not currently running");

        let no_terminal = FocusError::NoTerminal.user_message();
        assert_eq!(no_terminal, "Couldn't find the session's terminal");

        // All four messages must be pairwise distinct.
        let all = [unsupported, osascript, not_live, no_terminal];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn can_focus_is_true_on_macos() {
        assert!(can_focus());
    }

    #[test]
    fn app_name_extracts_bundle_from_comm_path() {
        assert_eq!(
            app_name_from_comm("/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"),
            Some("Terminal".to_string())
        );
        assert_eq!(app_name_from_comm("/usr/bin/zsh"), None);
    }
}
