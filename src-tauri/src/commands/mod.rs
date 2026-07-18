// Command modules only wired into the Tauri `invoke_handler` (desktop build).
// The HTTP API does not call into these, so they compile out entirely in
// headless-only builds.
#[cfg(feature = "desktop")]
pub mod debug;
#[cfg(feature = "desktop")]
pub mod picker;
#[cfg(feature = "desktop")]
pub mod session;

// Modules with impl functions the HTTP API also uses. These stay compiled in
// all builds; only their `#[tauri::command]` wrappers are desktop-gated.
pub mod cors;
pub mod git;
pub mod settings;
pub mod terminal;
pub mod wsl;
