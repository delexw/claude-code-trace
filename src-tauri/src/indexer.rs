//! Reading the project directories in the background.
//!
//! Discovery has to open every session file: turn counts, token totals and whether a
//! session is still running are only knowable by reading it. Across a few thousand
//! sessions that is seconds to minutes of solid work, and until it finished the picker
//! had nothing to draw — the window came up blank with a core pinned.
//!
//! So the walk runs here, on its own thread, and the app reads whatever it has so far:
//!
//! - **It reports as it goes**, in bytes as well as files, from inside the read of a
//!   single file. Session files differ by orders of magnitude in size, so a bar that
//!   only moved between files would sit still on the big ones.
//! - **It holds itself back** to a fifth of a core, resting in proportion to the reading
//!   just done, so indexing does not compete with the window it is filling.
//! - **It walks newest first**, so the sessions someone opened the app for arrive in the
//!   first moments rather than the last.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use tauri::Emitter;
use tokio::sync::broadcast;

use crate::parser::cache::SessionCache;
use crate::parser::session::{measure_session_files, ScanCounts, SessionInfo};
use crate::state::SseEvent;
use crate::AppHandle;

/// How long a finished index stays good before a walk is started to refresh it. Short:
/// a warm re-walk is stat-only and costs almost nothing.
const REFRESH_AFTER: Duration = Duration::from_secs(2);

/// How long the very first request for a set of directories waits on the walk before
/// answering with whatever it has. A small projects directory finishes well inside this,
/// so the picker is complete on its first draw: a walk that fast publishes its refresh
/// signal before the page is even listening for one, and an empty first answer would
/// then stay empty until something else happened to ask again.
const FIRST_WAIT: Duration = Duration::from_millis(400);

/// How often that wait looks to see whether the walk has finished.
const FIRST_WAIT_POLL: Duration = Duration::from_millis(5);

/// How often a running walk says how far it has got.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// How often a running walk publishes the sessions it has read, for the picker to draw.
/// Longer than the progress tick: each publish costs the frontend a re-fetch of the list.
const PUBLISH_INTERVAL: Duration = Duration::from_secs(2);

/// The share of one core a walk may use. Reading a whole projects directory is solid
/// work; taking a core for it is what makes an app feel like it has hung.
const DUTY_CYCLE: f64 = 0.2;

/// How much reading to do between pauses. Short enough that the pauses are invisible,
/// long enough not to spend them all on the cost of sleeping.
const WORK_SLICE: Duration = Duration::from_millis(40);

/// A pause is never longer than this, however slow the preceding read was, so a walk
/// cannot disappear for seconds at a time.
const MAX_PAUSE: Duration = Duration::from_millis(400);

/// How long to rest after `worked` spent reading, to hold a walk to [`DUTY_CYCLE`] of a
/// core.
fn throttle_pause(worked: Duration) -> Duration {
    worked
        .mul_f64((1.0 - DUTY_CYCLE) / DUTY_CYCLE)
        .min(MAX_PAUSE)
}

/// How far the background walk has got. The picker draws what has been read so far and
/// shows a progress bar from these numbers, rather than sitting blank until the end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexProgress {
    /// Session files read so far.
    pub files_read: usize,
    /// Session files the directories hold, counted before any were read.
    pub total_files: usize,
    /// Bytes covered so far. How full the bar is, because file counts lie when sizes
    /// vary this much.
    pub bytes_read: u64,
    /// Bytes those files come to.
    pub total_bytes: u64,
    /// False while a walk is still running. The picker hides the bar on true.
    pub done: bool,
}

/// What a walk has read for one set of project directories.
struct Indexed {
    dirs: Vec<String>,
    at: Instant,
    sessions: Vec<SessionInfo>,
    progress: IndexProgress,
}

/// The walk currently running, if any.
struct Run {
    dirs: Vec<String>,
    /// Bumped every time a walk starts. A walk whose generation is no longer the current
    /// one has been superseded — the project selection changed under it — and stops at
    /// its next report rather than filling a list nobody is looking at.
    generation: u64,
}

pub struct Indexer {
    indexed: Mutex<Option<Indexed>>,
    run: Mutex<Option<Run>>,
    events: broadcast::Sender<SseEvent>,
}

impl Indexer {
    pub fn new(events: broadcast::Sender<SseEvent>) -> Self {
        Self {
            indexed: Mutex::new(None),
            run: Mutex::new(None),
            events,
        }
    }

    /// The sessions known for `dirs` right now, and how far the walk has got.
    ///
    /// Never waits for a walk. The first call starts one and returns an empty list; each
    /// later call returns more of the directories until `progress.done`.
    pub fn snapshot(
        indexer: &Arc<Self>,
        cache: Arc<Mutex<SessionCache>>,
        dirs: &[String],
        app: Option<AppHandle>,
    ) -> (Vec<SessionInfo>, IndexProgress) {
        let held = match indexer.indexed.lock() {
            Ok(indexed) => indexed
                .as_ref()
                .filter(|i| i.dirs == dirs)
                .map(|i| (i.sessions.clone(), i.progress.clone(), i.at.elapsed())),
            Err(_) => None,
        };

        match held {
            // In hand and recent enough to serve as is.
            Some((sessions, progress, age)) if progress.done && age < REFRESH_AFTER => {
                (sessions, progress)
            }
            // Either a walk is still running, or what we hold has gone stale and one
            // should start. Serve what there is either way: a stale list beats a blank
            // one, and a warm re-walk lands in a moment.
            Some((sessions, progress, _)) => {
                Self::start(indexer, cache, dirs.to_vec(), app);
                (sessions, progress)
            }
            None => {
                Self::start(indexer, cache, dirs.to_vec(), app);
                Self::wait_briefly(indexer, dirs)
            }
        }
    }

    /// What the walk has for `dirs`, giving it up to [`FIRST_WAIT`] to finish first.
    /// Returns the moment it is done, so a small directory costs a few milliseconds
    /// rather than the whole wait.
    fn wait_briefly(indexer: &Arc<Self>, dirs: &[String]) -> (Vec<SessionInfo>, IndexProgress) {
        let deadline = Instant::now() + FIRST_WAIT;
        loop {
            let held = match indexer.indexed.lock() {
                Ok(indexed) => indexed
                    .as_ref()
                    .filter(|i| i.dirs == dirs)
                    .map(|i| (i.sessions.clone(), i.progress.clone())),
                Err(_) => None,
            };
            match held {
                Some((sessions, progress)) if progress.done => return (sessions, progress),
                _ if Instant::now() >= deadline => {
                    return held.unwrap_or_else(|| (Vec::new(), IndexProgress::default()))
                }
                _ => std::thread::sleep(FIRST_WAIT_POLL),
            }
        }
    }

    /// Start walking `dirs` in the background, unless that is already happening.
    pub fn start(
        indexer: &Arc<Self>,
        cache: Arc<Mutex<SessionCache>>,
        dirs: Vec<String>,
        app: Option<AppHandle>,
    ) {
        if dirs.is_empty() {
            return;
        }
        let generation = {
            let Ok(mut run) = indexer.run.lock() else {
                return;
            };
            if run.as_ref().is_some_and(|current| current.dirs == dirs) {
                return;
            }
            let generation = run.as_ref().map(|r| r.generation).unwrap_or(0) + 1;
            *run = Some(Run {
                dirs: dirs.clone(),
                generation,
            });
            generation
        };

        let indexer = Arc::clone(indexer);
        std::thread::spawn(move || indexer.walk(cache, dirs, generation, app));
    }

    /// Whether this walk is still the one the app wants.
    fn is_current(&self, generation: u64) -> bool {
        self.run
            .lock()
            .map(|run| run.as_ref().is_some_and(|r| r.generation == generation))
            .unwrap_or(false)
    }

    /// Read every project directory, publishing as it goes. Runs on its own thread.
    fn walk(
        &self,
        cache: Arc<Mutex<SessionCache>>,
        dirs: Vec<String>,
        generation: u64,
        app: Option<AppHandle>,
    ) {
        let started = Instant::now();
        let total = measure_session_files(&dirs);
        self.publish(
            &dirs,
            None,
            IndexProgress {
                total_files: total.files,
                total_bytes: total.bytes,
                ..IndexProgress::default()
            },
            &app,
        );

        let mut last_progress = Instant::now();
        let mut last_publish = Instant::now();
        let mut worked = Duration::ZERO;
        let mut slice_started = Instant::now();
        let mut counts = ScanCounts::default();

        let walked = {
            // Held for the walk: the cache is per-file and the walk is the only writer,
            // so a second walk would duplicate every read rather than share it.
            let Ok(cache) = cache.lock() else { return };
            cache.discover_all_project_sessions_streaming(
                &dirs,
                &mut |found, counts| {
                    if !self.is_current(generation) {
                        return false;
                    }
                    worked += slice_started.elapsed();

                    let progress = IndexProgress {
                        files_read: counts.files,
                        // A directory written while it is walked can hold more than the
                        // count found, and a bar past its own end looks broken.
                        total_files: total.files.max(counts.files),
                        bytes_read: counts.bytes,
                        total_bytes: total.bytes.max(counts.bytes),
                        done: false,
                    };
                    if last_publish.elapsed() >= PUBLISH_INTERVAL {
                        last_publish = Instant::now();
                        last_progress = Instant::now();
                        self.publish(&dirs, Some(found.to_vec()), progress, &app);
                    } else if last_progress.elapsed() >= PROGRESS_INTERVAL {
                        last_progress = Instant::now();
                        self.publish(&dirs, None, progress, &app);
                    }

                    // Rest in proportion to the reading just done. Files served from the
                    // cache cost nothing to open, so a warm re-walk barely pauses.
                    if worked >= WORK_SLICE {
                        std::thread::sleep(throttle_pause(worked));
                        worked = Duration::ZERO;
                    }
                    slice_started = Instant::now();
                    true
                },
                &mut counts,
            )
        };

        let (sessions, completed) = walked;
        if !completed || !self.is_current(generation) {
            return;
        }
        eprintln!(
            "Indexed {} sessions ({:.2} GB) in {:.1}s",
            sessions.len(),
            total.bytes as f64 / 1e9,
            started.elapsed().as_secs_f64()
        );
        self.publish(
            &dirs,
            Some(sessions),
            IndexProgress {
                files_read: total.files,
                total_files: total.files,
                bytes_read: total.bytes,
                total_bytes: total.bytes,
                done: true,
            },
            &app,
        );
        if let Ok(mut run) = self.run.lock() {
            if run.as_ref().is_some_and(|r| r.generation == generation) {
                *run = None;
            }
        }
    }

    /// Keep a walk's latest results and tell the frontend about them.
    ///
    /// `sessions` is `None` for a progress-only tick: the bar moves several times
    /// between publishes, and re-sending the list each time would put the whole picker
    /// on the wire four times a second.
    fn publish(
        &self,
        dirs: &[String],
        sessions: Option<Vec<SessionInfo>>,
        progress: IndexProgress,
        app: &Option<AppHandle>,
    ) {
        let published_sessions = sessions.is_some();
        if let Ok(mut indexed) = self.indexed.lock() {
            match indexed.as_mut() {
                Some(existing) if existing.dirs == dirs => {
                    if let Some(sessions) = sessions {
                        existing.sessions = sessions;
                        existing.at = Instant::now();
                    }
                    existing.progress = progress.clone();
                }
                _ => {
                    *indexed = Some(Indexed {
                        dirs: dirs.to_vec(),
                        at: Instant::now(),
                        sessions: sessions.unwrap_or_default(),
                        progress: progress.clone(),
                    })
                }
            }
        }

        let payload = serde_json::to_string(&progress).unwrap_or_else(|_| "{}".to_string());
        self.send("index-progress", &payload, app, &progress);

        // Only when there is more of the list to draw. The picker answers this by
        // re-fetching every session it is showing, too expensive to do on each tick.
        if published_sessions {
            self.send("picker-refresh", "{}", app, &serde_json::json!({}));
        }
    }

    /// Both ways the frontend can be listening: SSE for a browser, Tauri's own event
    /// bridge for the desktop window.
    fn send<T: Serialize + Clone>(
        &self,
        event: &str,
        sse_payload: &str,
        app: &Option<AppHandle>,
        payload: &T,
    ) {
        let _ = self.events.send(SseEvent {
            event: event.to_string(),
            data: sse_payload.to_string(),
        });
        emit_to_webview(app, event, payload);
    }
}

/// Emit an event to the desktop webview, if one is attached. Compiles to a no-op in
/// headless-only builds (no Tauri), where `app` is always `None`.
#[cfg(feature = "desktop")]
fn emit_to_webview<T: Serialize + Clone>(app: &Option<AppHandle>, event: &str, payload: &T) {
    if let Some(app) = app {
        let _ = app.emit(event, payload.clone());
    }
}

#[cfg(not(feature = "desktop"))]
fn emit_to_webview<T: Serialize + Clone>(_app: &Option<AppHandle>, _event: &str, _payload: &T) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexer() -> Arc<Indexer> {
        Arc::new(Indexer::new(broadcast::channel(16).0))
    }

    fn cache() -> Arc<Mutex<SessionCache>> {
        Arc::new(Mutex::new(SessionCache::new()))
    }

    /// A project directory holding `count` one-exchange sessions.
    fn project_dir(count: usize) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..count {
            std::fs::write(
                dir.path().join(format!("session-{i}.jsonl")),
                concat!(
                    "{\"type\":\"user\",\"uuid\":\"u1\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
                    "{\"type\":\"assistant\",\"uuid\":\"a1\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
                ),
            )
            .unwrap();
        }
        dir
    }

    /// Poll until the walk reports itself finished, or give up.
    fn wait_for_index(
        indexer: &Arc<Indexer>,
        cache: &Arc<Mutex<SessionCache>>,
        dirs: &[String],
    ) -> IndexProgress {
        for _ in 0..300 {
            let (_, progress) = Indexer::snapshot(indexer, Arc::clone(cache), dirs, None);
            if progress.done {
                return progress;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the background walk never finished");
    }

    #[test]
    fn a_small_directory_is_complete_on_the_first_ask() {
        // A walk this quick publishes its refresh signal before a page is listening for
        // one, so an empty first answer would stay empty until something else happened
        // to ask again.
        let indexer = indexer();
        let cache = cache();
        let dir = project_dir(5);
        let dirs = vec![dir.path().to_string_lossy().to_string()];

        let (sessions, progress) = Indexer::snapshot(&indexer, Arc::clone(&cache), &dirs, None);

        assert!(progress.done);
        assert_eq!(sessions.len(), 5);
        assert_eq!(progress.files_read, 5);
        assert_eq!(progress.total_files, 5);
        assert!(progress.bytes_read > 0);
        assert_eq!(progress.bytes_read, progress.total_bytes);
    }

    #[test]
    fn the_first_ask_gives_up_waiting_rather_than_holding_the_caller() {
        // The whole point of the background walk: directories too big to read in the
        // moment must not sit on the caller until every file has been read. Nothing ever
        // publishes for this one, standing in for a walk that runs for minutes.
        let indexer = indexer();

        let started = Instant::now();
        let (sessions, progress) =
            Indexer::wait_briefly(&indexer, &["/no/such/projects/dir".to_string()]);
        let waited = started.elapsed();

        assert!(sessions.is_empty());
        assert!(!progress.done);
        assert!(
            waited < FIRST_WAIT * 4,
            "the first ask waited {waited:?}, well past the {FIRST_WAIT:?} cap"
        );
    }

    #[test]
    fn a_later_ask_answers_from_what_the_walk_has_read_so_far() {
        let indexer = indexer();
        let cache = cache();
        let dir = project_dir(5);
        let dirs = vec![dir.path().to_string_lossy().to_string()];

        Indexer::snapshot(&indexer, Arc::clone(&cache), &dirs, None);
        wait_for_index(&indexer, &cache, &dirs);
        let started = Instant::now();
        let (sessions, progress) = Indexer::snapshot(&indexer, Arc::clone(&cache), &dirs, None);

        // Only the first ask ever waits: every refresh after it is served from what the
        // walk has already read.
        assert!(started.elapsed() < FIRST_WAIT);
        assert_eq!(sessions.len(), 5);
        assert!(progress.done);
    }

    #[test]
    fn an_empty_directory_finishes_rather_than_running_forever() {
        let indexer = indexer();
        let cache = cache();
        let dir = tempfile::tempdir().unwrap();
        let dirs = vec![dir.path().to_string_lossy().to_string()];

        Indexer::snapshot(&indexer, Arc::clone(&cache), &dirs, None);

        let progress = wait_for_index(&indexer, &cache, &dirs);
        assert_eq!(progress.files_read, 0);
        assert!(progress.done);
    }

    #[test]
    fn no_directories_starts_no_walk() {
        // The picker asks before a project is chosen; that must not spawn a thread that
        // immediately reports an empty index as finished.
        let indexer = indexer();
        let (sessions, progress) = Indexer::snapshot(&indexer, cache(), &[], None);

        assert!(sessions.is_empty());
        assert!(!progress.done);
        assert!(indexer.run.lock().unwrap().is_none());
    }

    #[test]
    fn a_second_ask_for_the_same_directories_joins_the_walk_already_running() {
        let indexer = indexer();
        let cache = cache();
        let dir = project_dir(3);
        let dirs = vec![dir.path().to_string_lossy().to_string()];
        // A walk already under way over these directories, which real timing makes
        // awkward to hold still: a directory small enough for a test finishes in
        // milliseconds.
        *indexer.run.lock().unwrap() = Some(Run {
            dirs: dirs.clone(),
            generation: 7,
        });

        Indexer::start(&indexer, cache, dirs.clone(), None);

        let generation = indexer.run.lock().unwrap().as_ref().map(|r| r.generation);
        assert_eq!(
            generation,
            Some(7),
            "a rival walk was started over the same directories"
        );
    }

    #[test]
    fn a_throttled_walk_rests_longer_than_it_works() {
        // At a fifth of a core, 40ms of reading buys 160ms of rest.
        assert_eq!(
            throttle_pause(Duration::from_millis(40)),
            Duration::from_millis(160)
        );
    }

    #[test]
    fn a_walk_that_did_no_reading_does_not_rest() {
        // Files served from the cache cost nothing to open, so a warm re-walk must not
        // sit there sleeping between them.
        assert_eq!(throttle_pause(Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn no_single_pause_stalls_the_walk() {
        // A slow read must not buy a pause measured in seconds.
        assert_eq!(throttle_pause(Duration::from_secs(60)), MAX_PAUSE);
    }
}
