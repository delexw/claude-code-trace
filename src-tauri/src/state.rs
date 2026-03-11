use std::sync::Mutex;

use crate::parser::cache::SessionCache;
use crate::watcher::{PickerWatcherHandle, SessionWatcherHandle};

/// AppState holds shared state managed by Tauri.
pub struct AppState {
    pub session_watcher: Mutex<Option<SessionWatcherHandle>>,
    pub picker_watcher: Mutex<Option<PickerWatcherHandle>>,
    pub session_cache: Mutex<SessionCache>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session_watcher: Mutex::new(None),
            picker_watcher: Mutex::new(None),
            session_cache: Mutex::new(SessionCache::new()),
        }
    }
}
