use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use super::ongoing::apply_staleness;
use super::session::{discover_project_sessions_streaming, ScanCounts, SessionInfo};

/// Per-file cache entry keyed by (modTime, size).
struct CachedFile {
    mod_time: SystemTime,
    size: u64,
    info: SessionInfo,
}

/// SessionCache avoids rescanning unchanged session files on every picker
/// refresh. The cache key is (path, modTime, size) — when a file's modification
/// time or size changes, we rescan it. Files that haven't changed return
/// cached metadata immediately.
pub struct SessionCache {
    file_cache: Mutex<HashMap<String, CachedFile>>,
}

impl SessionCache {
    pub fn new() -> Self {
        Self {
            file_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Get cached SessionInfo or rescan if file changed.
    pub fn get_or_scan(&self, path: &str, mod_time: SystemTime, size: u64) -> Option<SessionInfo> {
        self.get_or_scan_reporting(path, mod_time, size, &mut |_| {})
    }

    /// [`get_or_scan`](Self::get_or_scan), telling the caller how many bytes a rescan is
    /// reading as it goes. A cache hit reports nothing: it opens no file.
    pub fn get_or_scan_reporting(
        &self,
        path: &str,
        mod_time: SystemTime,
        size: u64,
        on_bytes: &mut dyn FnMut(u64),
    ) -> Option<SessionInfo> {
        let mut cache = self.file_cache.lock().unwrap();
        if let Some(cached) = cache.get(path) {
            if cached.mod_time == mod_time && cached.size == size {
                let mut info = cached.info.clone();
                // Re-check staleness for ongoing sessions at read time
                info.is_ongoing = apply_staleness(info.is_ongoing, mod_time);
                // Also check subagent files (orphan agents may be running
                // while the parent session file hasn't changed).
                if !info.is_ongoing {
                    info.is_ongoing = crate::parser::subagent::has_recently_active_subagents(path);
                }
                return Some(info);
            }
        }

        // Cache miss or stale — rescan.
        let meta = super::session::scan_session_metadata_reporting(path, on_bytes);
        let info = super::session::session_info_from_metadata(path, mod_time, meta);
        cache.insert(
            path.to_string(),
            CachedFile {
                mod_time,
                size,
                info: info.clone(),
            },
        );
        Some(info)
    }

    /// Discover sessions across multiple project directories with per-file caching.
    pub fn discover_all_project_sessions(
        &self,
        project_dirs: &[String],
    ) -> Result<Vec<SessionInfo>, String> {
        let mut counts = ScanCounts::default();
        Ok(self
            .discover_all_project_sessions_streaming(project_dirs, &mut |_, _| true, &mut counts)
            .0)
    }

    /// [`discover_all_project_sessions`](Self::discover_all_project_sessions), reporting
    /// progress as it goes and stopping when `on_progress` returns false.
    ///
    /// Returns what was read and whether the walk reached the end. Directories are
    /// visited in order but each one's files newest first, so the recently used sessions
    /// land on screen before the rest of the history.
    pub fn discover_all_project_sessions_streaming(
        &self,
        project_dirs: &[String],
        on_progress: &mut impl FnMut(&[SessionInfo], &ScanCounts) -> bool,
        counts: &mut ScanCounts,
    ) -> (Vec<SessionInfo>, bool) {
        let mut all: Vec<SessionInfo> = Vec::new();
        for dir in project_dirs {
            let carried = std::mem::take(&mut all);
            let mut seen_so_far = carried;
            let walked = discover_project_sessions_streaming(
                dir,
                |path, mod_time, size, on_bytes| {
                    self.get_or_scan_reporting(path, mod_time, size, on_bytes)
                },
                &mut |found, counts| {
                    // What the caller sees is every directory finished so far plus this
                    // one's progress, not just the directory being walked.
                    let mut combined = seen_so_far.clone();
                    combined.extend_from_slice(found);
                    on_progress(&combined, counts)
                },
                counts,
            );
            match walked {
                Ok((sessions, completed)) => {
                    seen_so_far.extend(sessions);
                    all = seen_so_far;
                    if !completed {
                        all.sort_by_key(|b| std::cmp::Reverse(b.mod_time));
                        return (all, false);
                    }
                }
                Err(_) => all = seen_so_far,
            }
        }
        all.sort_by_key(|b| std::cmp::Reverse(b.mod_time));
        (all, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session file holding one user message, back-dated so its order is fixed.
    fn write_session(dir: &std::path::Path, name: &str, age_secs: u64) -> String {
        let path = dir.join(format!("{name}.jsonl"));
        std::fs::write(
            &path,
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        )
        .unwrap();
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(when)).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn a_rescan_reports_the_bytes_it_reads_and_a_cache_hit_reports_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), "a", 10);
        let meta = std::fs::metadata(&path).unwrap();
        let cache = SessionCache::new();

        let mut first = 0u64;
        cache.get_or_scan_reporting(&path, meta.modified().unwrap(), meta.len(), &mut |n| {
            first += n
        });
        let mut second = 0u64;
        cache.get_or_scan_reporting(&path, meta.modified().unwrap(), meta.len(), &mut |n| {
            second += n
        });

        assert!(
            first > 0,
            "the first scan read the file but reported nothing"
        );
        // A cache hit opens nothing, so it must not claim to have read anything — the
        // walk credits the file's whole size on its own either way.
        assert_eq!(second, 0);
    }

    #[test]
    fn progress_keeps_climbing_across_project_directories() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        write_session(one.path(), "a", 30);
        write_session(one.path(), "b", 20);
        write_session(two.path(), "c", 10);
        let dirs = vec![
            one.path().to_string_lossy().to_string(),
            two.path().to_string_lossy().to_string(),
        ];

        let cache = SessionCache::new();
        let mut seen = Vec::new();
        let mut counts = ScanCounts::default();
        let (sessions, completed) = cache.discover_all_project_sessions_streaming(
            &dirs,
            &mut |found, counts| {
                seen.push((found.len(), counts.files, counts.bytes));
                true
            },
            &mut counts,
        );

        assert!(completed);
        assert_eq!(sessions.len(), 3);
        assert_eq!(counts.files, 3);
        // The second directory must carry on from the first rather than restarting, or
        // the bar would fall back every time the walk moved on.
        for pair in seen.windows(2) {
            assert!(pair[1].0 >= pair[0].0, "sessions went backwards: {seen:?}");
            assert!(pair[1].1 >= pair[0].1, "files went backwards: {seen:?}");
            assert!(pair[1].2 >= pair[0].2, "bytes went backwards: {seen:?}");
        }
    }

    #[test]
    fn a_walk_reports_every_directory_finished_so_far_not_just_the_one_it_is_in() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        write_session(one.path(), "a", 30);
        write_session(two.path(), "b", 10);
        let dirs = vec![
            one.path().to_string_lossy().to_string(),
            two.path().to_string_lossy().to_string(),
        ];

        let cache = SessionCache::new();
        let mut counts = ScanCounts::default();
        let mut last_batch = 0;
        cache.discover_all_project_sessions_streaming(
            &dirs,
            &mut |found, _| {
                last_batch = found.len();
                true
            },
            &mut counts,
        );

        // The picker draws whatever the last report carried, so it has to be the whole
        // list: handing it one directory at a time would blank out the others.
        assert_eq!(last_batch, 2);
    }

    #[test]
    fn a_missing_project_directory_does_not_stop_the_rest() {
        let real = tempfile::tempdir().unwrap();
        write_session(real.path(), "a", 10);
        let dirs = vec![
            "/no/such/projects/dir".to_string(),
            real.path().to_string_lossy().to_string(),
        ];

        let cache = SessionCache::new();
        let mut counts = ScanCounts::default();
        let (sessions, completed) =
            cache.discover_all_project_sessions_streaming(&dirs, &mut |_, _| true, &mut counts);

        assert!(completed);
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn a_superseded_walk_drops_out_rather_than_reading_the_rest() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        for i in 0..4 {
            write_session(one.path(), &format!("s{i}"), 100 - i);
        }
        write_session(two.path(), "later", 10);
        let dirs = vec![
            one.path().to_string_lossy().to_string(),
            two.path().to_string_lossy().to_string(),
        ];

        let cache = SessionCache::new();
        let mut counts = ScanCounts::default();
        let (_, completed) =
            cache.discover_all_project_sessions_streaming(&dirs, &mut |_, _| false, &mut counts);

        assert!(!completed);
        assert_eq!(counts.files, 1);
    }

    #[test]
    fn sessions_come_back_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        write_session(dir.path(), "oldest", 3000);
        write_session(dir.path(), "newest", 10);
        let dirs = vec![dir.path().to_string_lossy().to_string()];

        let cache = SessionCache::new();
        let mut counts = ScanCounts::default();
        let (sessions, _) =
            cache.discover_all_project_sessions_streaming(&dirs, &mut |_, _| true, &mut counts);

        assert!(sessions[0].path.contains("newest"));
        assert!(sessions[1].path.contains("oldest"));
    }
}
