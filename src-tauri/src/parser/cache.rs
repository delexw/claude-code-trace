use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use super::ongoing::apply_staleness;
use super::session::{discover_project_sessions_with_scan, SessionInfo};

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
        let mut cache = self.file_cache.lock().unwrap();
        if let Some(cached) = cache.get(path) {
            if cached.mod_time == mod_time && cached.size == size {
                let mut info = cached.info.clone();
                // Re-check staleness for ongoing sessions at read time
                info.is_ongoing = apply_staleness(info.is_ongoing, mod_time);
                return Some(info);
            }
        }

        // Cache miss or stale — rescan.
        let meta = super::session::scan_session_metadata(path);
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
        let mut all = Vec::new();
        for dir in project_dirs {
            if let Ok(sessions) =
                discover_project_sessions_with_scan(dir, |path, mod_time, size| {
                    self.get_or_scan(path, mod_time, size)
                })
            {
                all.extend(sessions);
            }
        }
        all.sort_by(|a, b| b.mod_time.cmp(&a.mod_time));
        Ok(all)
    }
}
