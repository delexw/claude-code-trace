//! "Is this pid a running process?" for the liveness guard. Shells out to
//! `kill -0`, matching cctrace's existing OS-integration idiom (git.rs, wsl.rs
//! already use `std::process::Command`) — no new dependency, no `unsafe`. The
//! spawn cost is bounded by the caller's TTL cache (`LivenessCache` in
//! `parser::session`, wired via `AppState::live_liveness_cached`): the
//! registry scan and every `is_pid_alive` check in it run at most once per
//! cache window, not once per `discover_sessions_cached` call. Windows has no
//! `kill`: liveness degrades to "closed" there.

use std::process::{Command, Stdio};

/// True if `pid` is a live process. `kill -0 <pid>` sends no signal — it only
/// performs the permission/existence check and exits 0 iff the process exists.
pub fn is_pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // Liveness is read entirely from the exit code — `kill`'s own
        // "No such process" stderr message on a dead pid is expected and
        // meaningless here, so it's suppressed rather than leaking into the
        // app's logs on every TTL-cache refresh for a permanently-dead entry.
        match Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(s) => s.success(),
            Err(e) => {
                // Distinguish "couldn't run kill at all" (PATH/sandbox/resource
                // issue — every session would silently read as dead) from a
                // clean "process doesn't exist". Safe to log per call: this sits
                // behind the registry's TTL cache, not a per-render hot path.
                eprintln!("is_pid_alive: failed to spawn `kill -0 {pid}`: {e}");
                false
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn own_pid_is_alive() {
        let me = std::process::id() as i64;
        assert!(is_pid_alive(me));
    }
    #[test]
    fn absurd_pid_is_not_alive() {
        assert!(!is_pid_alive(2_000_000_000));
    }
}
