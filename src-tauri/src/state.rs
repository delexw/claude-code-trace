use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

use std::collections::HashMap;

use crate::convert::{DisplayMessage, LoadResult};
use crate::parser::cache::SessionCache;
use crate::parser::session::{Liveness, LivenessCache, SessionInfo, SessionNamesCache};
use crate::session_load::{
    build_light_session, build_session, slice_light, LightBuild, LoadOptions, TimeFilter,
};
use crate::settings::Settings;
use crate::watcher::WatcherHandle;

/// A Server-Sent Event destined for browser clients.
#[derive(Clone, Debug)]
pub struct SseEvent {
    /// Event name, e.g. "session-update" or "picker-refresh".
    pub event: String,
    /// JSON-serialized payload.
    pub data: String,
}

/// Short-lived cache for the picker's session list. Multiple concurrent
/// callers within the TTL window share one disk scan.
struct SessionsCache {
    dirs: Vec<String>,
    cached_at: Instant,
    sessions: Vec<SessionInfo>,
}

const SESSIONS_CACHE_TTL: Duration = Duration::from_secs(2);

/// TTL for the live session-name registry scan. Short enough that `/rename`
/// names (and their removal) appear within roughly one picker-refresh cycle,
/// long enough that a burst of refreshes shares a single scan.
const SESSION_NAMES_CACHE_TTL: Duration = Duration::from_secs(1);

/// TTL for the computed-liveness cache (registry scan + `is_pid_alive` checks).
/// Mirrors [`SESSION_NAMES_CACHE_TTL`] so both registry joins share the same
/// freshness window and the same picker-refresh cache-sharing story.
const LIVENESS_CACHE_TTL: Duration = Duration::from_secs(1);

/// A lightened session build cached for the active session, keyed by
/// `(path, size)`. Lets range fetches during list scrolling slice an
/// already-built session instead of re-parsing the file on every scroll step.
/// Holds only the heavy-body-stripped messages — the full tool output is never
/// persisted here (see [`AppState::full_message_at`], which re-parses fresh and
/// discards the heavy build immediately after each detail lookup, trading
/// per-click latency for not holding tool output in memory between clicks).
struct CachedLight {
    path: String,
    size: u64,
    light: LightBuild,
}

/// AppState holds shared state managed by Tauri.
pub struct AppState {
    pub session_watcher: Mutex<Option<WatcherHandle>>,
    pub picker_watcher: Mutex<Option<WatcherHandle>>,
    pub session_cache: Mutex<SessionCache>,
    pub settings: Mutex<Settings>,
    /// Ongoing status reported by the session watcher for the currently viewed session.
    /// (session_path, is_ongoing) — kept in sync by the session watcher loop.
    pub watched_session_ongoing: Mutex<Option<(String, bool)>>,
    /// Broadcast channel for SSE — watchers send events here, HTTP clients subscribe.
    pub event_tx: broadcast::Sender<SseEvent>,
    /// 2-second TTL cache for the picker session list.
    sessions_cache: Mutex<Option<SessionsCache>>,
    /// Short-TTL cache for the live `/rename` session-name registry, shared by
    /// all concurrent `discover_sessions_cached` callers.
    session_names_cache: Mutex<SessionNamesCache>,
    /// Short-TTL cache for computed session liveness (registry scan + the
    /// per-entry `is_pid_alive`/`kill -0` checks), shared by all concurrent
    /// `discover_sessions_cached` callers so the picker-refresh broadcast
    /// fan-out shares one round of liveness checks instead of spawning a
    /// `kill` subprocess per live session per client.
    liveness_cache: Mutex<LivenessCache>,
    /// Lightened-build cache for the active session so windowed list fetches
    /// slice an existing build instead of re-parsing the file on every scroll.
    /// Never holds heavy tool bodies — see [`CachedLight`].
    session_light_cache: Mutex<Option<CachedLight>>,
}

impl AppState {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            session_watcher: Mutex::new(None),
            picker_watcher: Mutex::new(None),
            session_cache: Mutex::new(SessionCache::new()),
            settings: Mutex::new(crate::settings::load_settings()),
            watched_session_ongoing: Mutex::new(None),
            event_tx,
            sessions_cache: Mutex::new(None),
            session_names_cache: Mutex::new(SessionNamesCache::new()),
            liveness_cache: Mutex::new(LivenessCache::new()),
            session_light_cache: Mutex::new(None),
        }
    }

    /// Load a windowed slice of a session, caching the lightened build for the
    /// active session so repeated list range fetches (scrolling) don't re-parse
    /// the file. Never holds heavy tool bodies — see [`full_message_at`] for
    /// those.
    ///
    /// The cache holds one session, keyed by `(path, size)`: a different path or
    /// a grown/truncated file rebuilds it, so memory stays bounded to the active
    /// session. Time-filtered loads bypass the cache (they're rare and would
    /// pollute the single slot).
    ///
    /// [`full_message_at`]: Self::full_message_at
    pub fn load_session_windowed(
        &self,
        path: &str,
        opts: LoadOptions,
    ) -> Result<LoadResult, String> {
        // Time-filtered loads bypass the cache and return full messages (rare,
        // used by the by-id range endpoint).
        if opts.time.since.is_some() || opts.time.before.is_some() {
            return crate::session_load::load_session(path, opts);
        }

        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mut cache = self.session_light_cache.lock().map_err(|e| e.to_string())?;
        let fresh = cache
            .as_ref()
            .is_some_and(|c| c.path == path && c.size == size);
        if !fresh {
            let light = build_light_session(path, TimeFilter::default())?;
            *cache = Some(CachedLight {
                path: path.to_string(),
                size,
                light,
            });
        }
        Ok(slice_light(
            &cache.as_ref().unwrap().light,
            path,
            opts.range,
        ))
    }

    /// Return the full (heavy-body) message at `index` for the detail view.
    ///
    /// Deliberately re-parses the whole session fresh on every call rather than
    /// caching the heavy build: a tool-output-heavy session can be hundreds of
    /// MB, and caching it for as long as the session is open would hold that in
    /// the Rust process the whole time. Re-parsing trades per-click latency
    /// (roughly the same cost as the session's first list load) for never
    /// persisting the heavy bodies between clicks — the full build is dropped
    /// as soon as this function returns.
    pub fn full_message_at(
        &self,
        path: &str,
        index: usize,
    ) -> Result<Option<DisplayMessage>, String> {
        let built = build_session(path, TimeFilter::default())?;
        Ok(built.messages.get(index).cloned())
    }

    /// Drop the lightened-build cache (e.g. when leaving a session for the picker).
    pub fn clear_session_build_cache(&self) {
        if let Ok(mut cache) = self.session_light_cache.lock() {
            *cache = None;
        }
    }

    /// Read the live `/rename` session-name registry through a short-TTL cache so
    /// concurrent/rapid callers share a single disk scan. Falls back to an
    /// uncached scan if the cache lock is poisoned or the home dir is unknown.
    fn live_session_names_cached(&self) -> HashMap<String, String> {
        let dir = match crate::parser::session::live_session_names_dir() {
            Some(d) => d,
            None => return HashMap::new(),
        };
        match self.session_names_cache.lock() {
            Ok(mut cache) => cache.get_or_load(&dir, SESSION_NAMES_CACHE_TTL),
            Err(_) => crate::parser::session::live_session_names(),
        }
    }

    /// Read computed liveness (registry scan + `is_pid_alive`/`kill -0` checks)
    /// through a short-TTL cache so concurrent/rapid callers — and the
    /// picker-refresh broadcast fan-out — share one scan and one round of
    /// liveness checks instead of spawning a `kill` subprocess per live
    /// session on every call. Falls back to an uncached scan+compute if the
    /// cache lock is poisoned.
    fn live_liveness_cached(&self, dir: &std::path::Path) -> HashMap<String, Liveness> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        match self.liveness_cache.lock() {
            Ok(mut cache) => cache.get_or_load(dir, LIVENESS_CACHE_TTL, now_ms),
            Err(_) => {
                let reg = crate::parser::session::read_session_registry(dir);
                crate::parser::session::liveness_map_from_registry(&reg, now_ms)
            }
        }
    }

    /// Stop and clear the session watcher if one is running.
    pub fn stop_session_watcher(&self) -> Result<(), String> {
        let mut guard = self.session_watcher.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            handle.stop();
        }
        Ok(())
    }

    /// Replace the session watcher with a new handle.
    pub fn set_session_watcher(&self, handle: WatcherHandle) -> Result<(), String> {
        let mut guard = self.session_watcher.lock().map_err(|e| e.to_string())?;
        *guard = Some(handle);
        Ok(())
    }

    /// Stop and clear the picker watcher if one is running.
    pub fn stop_picker_watcher(&self) -> Result<(), String> {
        let mut guard = self.picker_watcher.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            handle.stop();
        }
        Ok(())
    }

    /// Replace the picker watcher with a new handle.
    pub fn set_picker_watcher(&self, handle: WatcherHandle) -> Result<(), String> {
        let mut guard = self.picker_watcher.lock().map_err(|e| e.to_string())?;
        *guard = Some(handle);
        Ok(())
    }

    /// Update the ongoing status for the currently watched session.
    pub fn set_watched_ongoing(&self, path: String, ongoing: bool) {
        if let Ok(mut guard) = self.watched_session_ongoing.lock() {
            *guard = Some((path, ongoing));
        }
    }

    /// Clear the watched session ongoing status (e.g. when unwatching).
    pub fn clear_watched_ongoing(&self) {
        if let Ok(mut guard) = self.watched_session_ongoing.lock() {
            *guard = None;
        }
    }

    /// Apply the session watcher's ongoing status to a list of sessions.
    /// The session watcher has the most accurate ongoing detection (full chunk
    /// analysis + subagent tracking), so its verdict overrides the picker's
    /// lightweight metadata scan.
    pub fn apply_watched_ongoing(&self, sessions: &mut [crate::parser::session::SessionInfo]) {
        let guard = match self.watched_session_ongoing.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some((ref path, ongoing)) = *guard {
            if let Some(s) = sessions.iter_mut().find(|s| s.path == *path) {
                s.is_ongoing = ongoing;
            }
        }
    }

    /// Discover sessions across `project_dirs`, returning a cached result if
    /// fresh enough. Multiple concurrent callers within the TTL window share
    /// one disk scan.
    pub fn discover_sessions_cached(
        &self,
        project_dirs: &[String],
    ) -> Result<Vec<SessionInfo>, String> {
        let mut cache = self.sessions_cache.lock().map_err(|e| e.to_string())?;
        let fresh = cache
            .as_ref()
            .is_some_and(|c| c.dirs == project_dirs && c.cached_at.elapsed() < SESSIONS_CACHE_TTL);
        let mut sessions = if fresh {
            cache.as_ref().unwrap().sessions.clone()
        } else {
            let session_cache = self.session_cache.lock().map_err(|e| e.to_string())?;
            let sessions = session_cache.discover_all_project_sessions(project_dirs)?;
            *cache = Some(SessionsCache {
                dirs: project_dirs.to_vec(),
                cached_at: Instant::now(),
                sessions: sessions.clone(),
            });
            sessions
        };
        // Release the cache lock before the filesystem work below, so concurrent
        // callers don't serialize behind it.
        drop(cache);
        // Join the live `/rename` names on every call. Names live in the pid-keyed
        // `~/.claude/sessions/*.json` registry, which the transcript-file cache does
        // not track (a rename never touches the JSONL), so the join runs after the
        // cache rather than being baked into the cached `SessionInfo`. The registry
        // read itself is behind its own short-TTL cache so a burst of refreshes and
        // the broadcast fan-out share one disk scan.
        crate::parser::session::apply_session_names(
            &mut sessions,
            &self.live_session_names_cached(),
        );
        // Liveness reads the same registry directory but needs the richer
        // per-entry data (status/pid), not just names, so it joins through its
        // own short-TTL cache (`liveness_cache`) rather than reusing the name
        // cache above. That cache covers both the registry scan and the
        // per-entry `is_pid_alive` ("kill -0") checks, so a burst of refreshes
        // and the broadcast fan-out share one round of liveness checks.
        if let Some(dir) = crate::parser::session::live_session_names_dir() {
            let liveness = self.live_liveness_cached(&dir);
            crate::parser::session::apply_liveness_map(&mut sessions, &liveness);
        }
        Ok(sessions)
    }

    /// Broadcast an SSE event to all connected browser clients.
    pub fn broadcast(&self, event: &str, data: &str) {
        let _ = self.event_tx.send(SseEvent {
            event: event.to_string(),
            data: data.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_load::MessageRange;
    use std::io::Write;

    fn write_session(dir: &std::path::Path) -> String {
        let path = dir.join("s.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","uuid":"u1","timestamp":"2025-01-01T12:00:00Z","message":{{"role":"user","content":"hello"}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2025-01-01T12:00:01Z","message":{{"role":"assistant","model":"claude-sonnet-4-20250514","content":[{{"type":"text","text":"hi there"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2025-01-01T12:00:02Z","message":{{"role":"user","content":"bye"}}}}"#
        )
        .unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn load_session_windowed_returns_window_with_total_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path());
        let state = AppState::new();

        let full = state
            .load_session_windowed(&path, LoadOptions::full())
            .unwrap();
        assert_eq!(full.count, full.messages.len());
        assert!(full.count >= 3);

        let win = state
            .load_session_windowed(
                &path,
                LoadOptions::window(MessageRange {
                    start: 1,
                    limit: Some(1),
                }),
            )
            .unwrap();
        assert_eq!(win.count, full.count, "count is the total, not the window");
        assert_eq!(win.start, 1);
        assert_eq!(win.messages.len(), 1);
        assert_eq!(win.messages[0].content, full.messages[1].content);
    }

    #[test]
    fn light_cache_serves_repeated_windows_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path());
        let state = AppState::new();

        // First call builds and caches; second (same path+size) hits the cache
        // and must return identical content.
        let a = state
            .load_session_windowed(&path, LoadOptions::full())
            .unwrap();
        assert!(state.session_light_cache.lock().unwrap().is_some());
        let b = state
            .load_session_windowed(&path, LoadOptions::full())
            .unwrap();
        assert_eq!(a.count, b.count);
        let contents_a: Vec<_> = a.messages.iter().map(|m| &m.content).collect();
        let contents_b: Vec<_> = b.messages.iter().map(|m| &m.content).collect();
        assert_eq!(contents_a, contents_b);

        state.clear_session_build_cache();
        assert!(state.session_light_cache.lock().unwrap().is_none());
        // Still works after clearing (rebuilds).
        let c = state
            .load_session_windowed(&path, LoadOptions::full())
            .unwrap();
        assert_eq!(c.count, a.count);
    }

    #[test]
    fn full_message_at_works_without_a_warm_light_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path());
        let state = AppState::new();

        // No list load has happened yet — full_message_at must not depend on
        // the light cache being populated first.
        assert!(state.session_light_cache.lock().unwrap().is_none());
        let msg = state.full_message_at(&path, 1).unwrap();
        assert!(msg.is_some());
        assert_eq!(msg.unwrap().content, "hi there");
    }

    #[test]
    fn full_message_at_never_populates_the_light_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path());
        let state = AppState::new();

        state.full_message_at(&path, 0).unwrap();
        state.full_message_at(&path, 1).unwrap();
        // Detail lookups re-parse fresh each time and never persist the heavy
        // build — the light cache (used only by list windows) stays empty.
        assert!(state.session_light_cache.lock().unwrap().is_none());
    }

    #[test]
    fn full_message_at_out_of_range_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path());
        let state = AppState::new();

        assert!(state.full_message_at(&path, 9999).unwrap().is_none());
    }
}
