use crate::parser::debuglog::*;

/// Get debug log entries for a session, with optional filtering.
#[tauri::command]
pub async fn get_debug_log(
    session_path: String,
    min_level: Option<String>,
    filter_text: Option<String>,
) -> Result<Vec<DebugEntry>, String> {
    let debug_path = debug_log_path(&session_path);
    if debug_path.is_empty() {
        return Ok(Vec::new());
    }

    let (entries, _offset) = read_debug_log(&debug_path)?;

    let level = match min_level.as_deref() {
        Some("WARN") | Some("warn") => DebugLevel::Warn,
        Some("ERROR") | Some("error") => DebugLevel::Error,
        _ => DebugLevel::Debug,
    };

    let filtered = filter_by_level(&entries, &level);
    let filtered = filter_by_text(&filtered, filter_text.as_deref().unwrap_or(""));
    let collapsed = collapse_duplicates(filtered);

    Ok(collapsed)
}
