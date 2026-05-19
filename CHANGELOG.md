# Changelog

All notable changes to claude-code-trace are documented here. Versions follow
[semantic versioning](https://semver.org/).

## [0.5.1] — 2026-05-20

A release focused entirely on the session-watcher / picker-refresh path. Brings the picker
closer to a live view: the cards you can see on screen update without waiting for filesystem
events, and a long-standing TUI bug that silently disabled picker auto-refresh is fixed.

### Added

- **Viewport-driven picker refresh in the web frontend**. The picker now wraps every session
  card in a shared `IntersectionObserver` (`useVisibleSessions` hook) and re-fetches the
  session list whenever the visible set changes — debounced 150 ms — plus a 2 s heartbeat
  while any cards are visible. This means the cards the user is actually looking at stay
  fresh without paying the cost of polling everything all the time, and ongoing-status /
  token-count badges update much closer to real time. Verified end-to-end in a real browser
  (Chromium): 549/549 cards observed, scroll triggers a `POST /api/sessions` at ~T+1.3 s,
  then the heartbeat keeps the list fresh while the picker is mounted.

### Fixed

- **TUI picker auto-refresh**
  ([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5)). The terminal UI
  subscribed to a non-existent `picker-update` event and destructured `payload.sessions` from
  it, while the backend emits `picker-refresh` with an empty payload
  (`src-tauri/src/watcher.rs:340`). The TUI picker never auto-updated when sessions changed
  on disk — it only refreshed when the user re-entered the picker view. The TUI now
  subscribes to `picker-refresh` and re-fetches via `api.discoverSessions(dirs)`, mirroring
  the web frontend pattern in `src/hooks/usePicker.ts`.

- **Web picker re-fetch on `picker-refresh` signal**
  ([`01f8212`](https://github.com/delexw/claude-code-trace/commit/01f8212)). Memoise the most
  recent `projectDirs` in a ref so the SSE handler can re-issue `discover_sessions` without
  the caller needing to thread state through.

[0.5.1]: https://github.com/delexw/claude-code-trace/releases/tag/v0.5.1
