//! Read-only listing of Antigravity (Gemini IDE) brain folders.
//!
//! Each `~/.gemini/<flavor>/brain/<uuid>/` directory is one Antigravity
//! conversation. This module surfaces them in the picker as envelopes — it
//! does not parse the JSONL transcript yet. Adding transcript support later
//! is a separate concern (see `parser/`).
//!
//! Gating: enabled via `Settings.enable_antigravity`. Default is off, so the
//! picker never scans an Antigravity root unless the user opts in.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use crate::state::AppState;

/// The Antigravity "brain" subpath under `.gemini/<flavor>/`. Kept as a
/// single constant so the picker UI can label it consistently.
pub const ANTIGRAVITY_FLAVOR: &str = "antigravity-ide";

/// Env var that relocates the whole `.gemini` directory. Mirrors how
/// `dirs::home_dir()` is used elsewhere in the app.
pub const ENV_GEMINI_HOME: &str = "GEMINI_HOME";

/// One conversation folder under the brain root. Shown in the picker as a
/// pseudo-session. The transcript itself is not opened here.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AntigravityBrainFolder {
    /// Folder name (a UUID assigned by Antigravity).
    pub uuid: String,
    /// Absolute path to the folder on the host filesystem.
    pub path: String,
    /// Folder's last-modified time as Unix milliseconds. Drives the picker
    /// sort (`newest first`).
    pub mod_time_ms: i64,
    /// First non-empty line of `task.md`, truncated to 80 chars, when the
    /// file is present and readable. Used as a hint label.
    pub title: Option<String>,
    /// Always `"antigravity"` for v1. Lets the frontend badge future
    /// providers without a schema change.
    pub provider_id: &'static str,
}

/// One transcript step, normalized to a Claude-like shape so the frontend
/// can reuse the existing `MessageItem` rendering. Roles we emit:
/// - `"user"`        — first USER_INPUT row
/// - `"assistant"`   — PLANNER_RESPONSE
/// - `"tool"`        — MCP_TOOL
/// - `"file"`        — VIEW_FILE
/// - `"search"`      — GREP_SEARCH
/// - `"error"`       — ERROR_MESSAGE
/// - `"checkpoint"`  — CHECKPOINT (compaction)
/// - `"ignored"`     — CONVERSATION_HISTORY / KNOWLEDGE_ARTIFACTS (system
///                     context that the picker hides)
///
/// Content is the raw `content` string from the JSONL row (no embellishment,
/// no fabricated code blocks). `<USER_REQUEST>` markers stay visible so the
/// reader sees the exact wire shape.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::doc_overindented_list_items)]
pub struct AntigravityMessage {
    pub step_index: u64,
    pub role: String,
    pub content: String,
    pub created_at: Option<String>,
    /// Tool name when the row is an MCP_TOOL call (e.g. `"execute_url"`).
    pub tool_name: Option<String>,
}

/// Pure core for tests. Resolves the brain root in this order:
/// 1. `override_dir` (Settings.antigravity_root) when non-empty,
/// 2. `$GEMINI_HOME/.gemini/<flavor>/brain` when `gemini_home` is set,
/// 3. `<os_home>/.gemini/<flavor>/brain` when `os_home` is set,
/// 4. `None` when no usable home exists.
pub fn antigravity_brain_root(
    override_dir: Option<&str>,
    gemini_home: Option<&str>,
    os_home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(p) = override_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
    {
        return Some(p);
    }
    let base = gemini_home
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| os_home.map(PathBuf::from))?;
    Some(base.join(".gemini").join(ANTIGRAVITY_FLAVOR).join("brain"))
}

/// Read a one-line title hint from the most authoritative source inside a
/// brain folder. Tries:
/// 1. `conversation_*.md.metadata.json` (a reference artifact — its
///    `summary` describes a conversation this folder points to),
/// 2. `task.md` first line (legacy / hand-written),
/// 3. First `USER_INPUT` row in `.system_generated/logs/transcript.jsonl`
///    (the real conversation transcript — always present for Antigravity
///    sessions, this is the bread-and-butter fallback that gives every
///    folder a meaningful title),
/// 4. None — caller falls back to the folder UUID.
///
/// Returns None when every source is missing or unreadable.
fn read_folder_title(folder: &Path) -> Option<String> {
    if let Some(s) = read_metadata_summary(folder) {
        return Some(truncate(&s, 80));
    }
    if let Some(s) = read_first_line(&folder.join("task.md")) {
        return Some(truncate(&s, 80));
    }
    if let Some(s) = read_first_user_input(folder) {
        return Some(s);
    }
    None
}

/// Locate the first `<USER_REQUEST>…</USER_REQUEST>` block in `text` and
/// return its body. Matches across multiple lines. None if the markers
/// aren't both present.
fn extract_user_request(text: &str) -> Option<&str> {
    let start = text.find("<USER_REQUEST>")? + "<USER_REQUEST>".len();
    let end = text[start..].find("</USER_REQUEST>")? + start;
    Some(&text[start..end])
}

/// Best-effort cleanup of a user-request body for the picker:
/// - strip `@[path:Lstart-Lend]` IDE anchors (every Antigravity message
///   opens with them and they eat the visible character budget),
/// - collapse runs of dots / whitespace into a single space,
/// - re-trim.
fn clean_user_request(text: &str) -> String {
    let mut s = text.to_owned();
    // Strip @[...:Lnn-Lnn] anchors. The brackets can be nested with
    // backslashes, so we greedily consume until the closing `]`.
    while let Some(start) = s.find("@[") {
        let after = start + 2;
        if let Some(end_rel) = s[after..].find(']') {
            s.replace_range(start..after + end_rel + 1, "");
        } else {
            break;
        }
    }
    // Whitespace runs (incl. `\r\n`, tabs, full stops used as separators).
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        let is_space = ch.is_whitespace() || ch == '.' && prev_space;
        if is_space {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_owned()
}

/// Read `.system_generated/logs/transcript.jsonl` and return the first
/// `USER_INPUT` row's `<USER_REQUEST>` body, cleaned up and truncated to
/// 80 chars. None when the transcript is missing, no USER_INPUT row
/// exists in the first 20 lines, or the request body is empty.
fn read_first_user_input(folder: &Path) -> Option<String> {
    let path = folder.join(".system_generated/logs/transcript.jsonl");
    let text = std::fs::read_to_string(&path).ok()?;
    for line in text.lines().take(20) {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("USER_INPUT") {
            continue;
        }
        let content = value.get("content").and_then(|c| c.as_str())?;
        let request = extract_user_request(content)?;
        let cleaned = clean_user_request(request);
        if cleaned.is_empty() {
            return None;
        }
        return Some(truncate(&cleaned, 80));
    }
    None
}

/// Locate `conversation_*.md.metadata.json` (there's at most one in
/// practice) and pull the `summary` field. None if the file is missing or
/// the JSON has no `summary` string.
fn read_metadata_summary(folder: &Path) -> Option<String> {
    let candidate = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("conversation_") && name.ends_with(".md.metadata.json") {
                Some(entry.path())
            } else {
                None
            }
        })?;
    let text = std::fs::read_to_string(&candidate).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("summary")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Read the first non-empty trimmed line of `path`, truncated to `max` chars.
/// Returns None when the file is missing or unreadable.
fn read_first_line(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| truncate(l, 80))
}

/// Truncate a string at a Unicode char boundary (so multi-byte chars like
/// Arabic don't panic on `.chars().take(n)`).
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Scan `root` for direct subdirectories and build folder metadata. Returns
/// `[]` when `root` does not exist (an unconfigured Antigravity install is
/// not an error). Sort: newest modification first.
pub fn scan_antigravity_brain(root: &Path) -> Vec<AntigravityBrainFolder> {
    let entries = match std::fs::read_dir(root) {
        Ok(it) => it,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<AntigravityBrainFolder> = entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let p = entry.path();
            if !p.is_dir() {
                return None;
            }
            let uuid = entry.file_name().to_string_lossy().into_owned();
            let mod_time_ms = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let title = read_folder_title(&p);
            Some(AntigravityBrainFolder {
                uuid,
                path: p.to_string_lossy().into_owned(),
                mod_time_ms,
                title,
                provider_id: "antigravity",
            })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.mod_time_ms));
    out
}

/// Resolves the brain root using live settings + environment. Returns
/// `(path, exists)`. `path` may be empty when no home directory is known.
/// Resolves the brain root using live settings + environment. Public so
/// the HTTP API can reuse it (see `api_list_antigravity_brain_folders`).
pub fn resolve_root(settings: &crate::settings::Settings) -> (PathBuf, bool) {
    let os_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    let root = antigravity_brain_root(
        settings.antigravity_root.as_deref(),
        std::env::var(ENV_GEMINI_HOME).ok().as_deref(),
        os_home.as_deref(),
    );
    match root {
        Some(p) => (p.clone(), p.exists()),
        None => (PathBuf::new(), false),
    }
}

/// Pure helper: returns the brain folders currently visible to the user.
/// Empty when:
/// - the user has not opted in (`Settings.enable_antigravity == false`), or
/// - the resolved root does not exist on disk.
///
/// Shared by the Tauri command and the HTTP API so both paths get the
/// same answer regardless of which entry point is used.
pub fn list_visible_antigravity_folders(
    settings: &crate::settings::Settings,
) -> Vec<AntigravityBrainFolder> {
    if !settings.enable_antigravity {
        return Vec::new();
    }
    let (root, exists) = resolve_root(settings);
    if !exists {
        return Vec::new();
    }
    scan_antigravity_brain(&root)
}

/// Returns the brain folders currently visible to the user.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_antigravity_brain_folders(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AntigravityBrainFolder>, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(list_visible_antigravity_folders(&guard))
}

/// Returns the resolved brain root + a flag indicating whether the folder
/// exists on disk. Frontend uses this in Settings to label the toggle.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_antigravity_brain_root(
    state: State<'_, Arc<AppState>>,
) -> Result<(String, bool), String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    let (root, exists) = resolve_root(&guard);
    Ok((root.to_string_lossy().into_owned(), exists))
}

/// Reveal an Antigravity brain folder in the host file manager. Read-only —
/// the picker never opens the transcript itself. Resolves to the OS-native
/// `open` so the user lands in the folder, not the app itself.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn open_antigravity_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    if !p.is_dir() {
        return Err(format!("path is not a directory: {path}"));
    }
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// Read the conversation transcript for a brain folder. Walks
/// `.system_generated/logs/transcript.jsonl` row by row and emits a
/// Claude-shape message per step (see [`AntigravityMessage`]). Returns
/// an empty vec when the transcript is missing — the call site then shows
/// "no messages" rather than an error, since some brain folders reference
/// other sessions and have no local transcript.
///
/// Pure helper, used by both the Tauri command and the HTTP API.
pub fn load_antigravity_transcript(path: &Path) -> Vec<AntigravityMessage> {
    let transcript = path.join(".system_generated/logs/transcript.jsonl");
    let text = match std::fs::read_to_string(&transcript) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(msg) = parse_transcript_row(&value) {
            out.push(msg);
        }
    }
    out
}

/// Map a single transcript.jsonl row to our normalized message shape.
/// Returns `None` for system-context rows the reader doesn't want to see
/// (`CONVERSATION_HISTORY`, `KNOWLEDGE_ARTIFACTS`).
fn parse_transcript_row(value: &serde_json::Value) -> Option<AntigravityMessage> {
    let step_index = value.get("step_index")?.as_u64()?;
    let r#type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_owned();
    let created_at = value
        .get("created_at")
        .and_then(|c| c.as_str())
        .map(str::to_owned);
    let role = match r#type {
        "USER_INPUT" => "user",
        "PLANNER_RESPONSE" => "assistant",
        "MCP_TOOL" => "tool",
        "VIEW_FILE" => "file",
        "GREP_SEARCH" => "search",
        "ERROR_MESSAGE" => "error",
        "CHECKPOINT" => "checkpoint",
        // Surface Antigravity's periodic roll-up of recent conversations —
        // it carries `<conversation_summaries>` with conversation IDs, titles
        // and objectives. Useful for the reader, harmless if ignored.
        "CONVERSATION_HISTORY" => "context",
        // Knowledge artefacts are pure engine state with no reader value.
        "KNOWLEDGE_ARTIFACTS" => return None,
        // Unknown type — still surface it as system so we don't silently drop.
        _ => "system",
    };
    let tool_name = if role == "tool" {
        // The MCP tool name isn't a separate field; the first content line
        // is something like `Created At: ...` and the rest is the tool
        // payload. We leave it `None` for now — v3 can parse it from the
        // protobuf-backed `messages/` folder if needed.
        None
    } else {
        None
    };
    Some(AntigravityMessage {
        step_index,
        role: role.to_owned(),
        content,
        created_at,
        tool_name,
    })
}

/// Tauri command — returns all messages for a brain folder path.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn load_antigravity_session(path: String) -> Result<Vec<AntigravityMessage>, String> {
    Ok(load_antigravity_transcript(&PathBuf::from(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn root_prefers_explicit_override() {
        let r = antigravity_brain_root(Some("/custom/brain"), None, Some("/home/u"));
        assert_eq!(r, Some(PathBuf::from("/custom/brain")));
    }

    #[test]
    fn root_uses_gemini_home_when_set() {
        let r = antigravity_brain_root(None, Some("/data/g"), Some("/home/u"));
        assert_eq!(
            r,
            Some(PathBuf::from("/data/g/.gemini/antigravity-ide/brain"))
        );
    }

    #[test]
    fn root_falls_back_to_os_home() {
        let r = antigravity_brain_root(None, None, Some("/home/u"));
        assert_eq!(
            r,
            Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
        );
    }

    #[test]
    fn empty_override_falls_through_to_home() {
        let r = antigravity_brain_root(Some("   "), None, Some("/home/u"));
        assert_eq!(
            r,
            Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
        );
    }

    #[test]
    fn root_is_none_without_any_home() {
        assert_eq!(antigravity_brain_root(None, None, None), None);
        assert_eq!(antigravity_brain_root(Some("  "), None, None), None);
    }

    #[test]
    fn scan_returns_empty_when_root_missing() {
        let tmp = std::env::temp_dir().join("cctrace-antig-missing");
        let _ = fs::remove_dir_all(&tmp);
        let v = scan_antigravity_brain(&tmp);
        assert!(v.is_empty());
    }

    #[test]
    fn scan_finds_folders_sorted_newest_first_with_task_title() {
        let tmp = std::env::temp_dir().join("cctrace-antig-scan");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let older = tmp.join("00000000-0000-0000-0000-000000000001");
        let newer = tmp.join("00000000-0000-0000-0000-000000000002");
        fs::create_dir_all(&older).unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::write(older.join("task.md"), "First older task\nmore lines").unwrap();
        fs::write(newer.join("task.md"), "Second newer task").unwrap();
        // Sleep so the newer folder has a strictly later mtime.
        std::thread::sleep(std::time::Duration::from_millis(10));

        let results = scan_antigravity_brain(&tmp);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].uuid, "00000000-0000-0000-0000-000000000002");
        assert_eq!(results[1].uuid, "00000000-0000-0000-0000-000000000001");
        assert_eq!(results[0].title.as_deref(), Some("Second newer task"));
        assert_eq!(results[1].title.as_deref(), Some("First older task"));
        assert!(results[0].mod_time_ms > 0);
        assert!(results[0].mod_time_ms >= results[1].mod_time_ms);
        assert_eq!(results[0].provider_id, "antigravity");
    }

    #[test]
    fn scan_prefers_metadata_summary_over_task_md() {
        // Real Antigravity folders use `conversation_*.md.metadata.json`
        // rather than `task.md`. The metadata summary must win when both
        // are present (the summary describes the conversation; task.md
        // is whatever the user wrote there).
        let tmp = std::env::temp_dir().join("cctrace-antig-meta");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("33333333-3333-3333-3333-333333333333");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("task.md"), "ignored\n").unwrap();
        fs::write(
            folder.join("conversation_48eb.md.metadata.json"),
            r#"{"summary":"ملف مرجعي يعرض محتوى سجل المحادثة 48eb5442","updatedAt":"2026-09-30"}"#,
        )
        .unwrap();

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        assert_eq!(
            v[0].title.as_deref(),
            Some("ملف مرجعي يعرض محتوى سجل المحادثة 48eb5442"),
        );
    }

    #[test]
    fn scan_skips_empty_metadata_summary_and_falls_back_to_task() {
        // If metadata summary is empty/whitespace, fall through to task.md.
        let tmp = std::env::temp_dir().join("cctrace-antig-meta-empty");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("44444444-4444-4444-4444-444444444444");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("task.md"), "Fallback task title\n").unwrap();
        fs::write(
            folder.join("conversation_x.md.metadata.json"),
            r#"{"summary":"   ","userFacing":true}"#,
        )
        .unwrap();

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title.as_deref(), Some("Fallback task title"));
    }

    #[test]
    fn scan_skips_files_and_unreadable_task_md() {
        let tmp = std::env::temp_dir().join("cctrace-antig-skip");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        // A stray file should be ignored (not a directory).
        fs::write(tmp.join("stray.txt"), "not a folder").unwrap();
        let only = tmp.join("11111111-1111-1111-1111-111111111111");
        fs::create_dir_all(&only).unwrap();
        // No task.md → title is None.

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].uuid, "11111111-1111-1111-1111-111111111111");
        assert!(v[0].title.is_none());
        assert_eq!(v[0].provider_id, "antigravity");
    }

    #[test]
    fn task_title_truncates_at_80_chars() {
        let tmp = std::env::temp_dir().join("cctrace-antig-trunc");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let big = "a".repeat(200);
        let only = tmp.join("22222222-2222-2222-2222-222222222222");
        fs::create_dir_all(&only).unwrap();
        fs::write(only.join("task.md"), &big).unwrap();

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title.as_deref().map(str::len), Some(80));
    }

    #[test]
    fn scan_extracts_title_from_first_user_input() {
        // Real-world layout: `.system_generated/logs/transcript.jsonl`
        // with a USER_INPUT line wrapping the request in `<USER_REQUEST>`.
        // The first non-`<USER_REQUEST>` line is an AGENT_OUTPUT that
        // should NOT be picked.
        let tmp = std::env::temp_dir().join("cctrace-antig-userinput");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("55555555-5555-5555-5555-555555555555");
        let logs = folder.join(".system_generated/logs");
        fs::create_dir_all(&logs).unwrap();

        let transcript = r#"{"step_index":0,"type":"AGENT_OUTPUT","content":"earlier output"}
{"step_index":1,"type":"USER_INPUT","content":"<USER_REQUEST>\n@[\\wsl.localhost\\Ubuntu\\root\\ArenaFlows_V5_Enhanced\\static\\picker.html:L52-L68]  ضيف سكشن ........ لاختيار .............................. ملف من المسار \\data\\parquet_db .. بديلا عن البيانات المخمرة\n</USER_REQUEST>\n<ADDITIONAL_METADATA>...</ADDITIONAL_METADATA>"}
"#;
        fs::write(logs.join("transcript.jsonl"), transcript).unwrap();

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        let title = v[0].title.as_deref().expect("title present");
        // The @[…:L…-L…] anchor is stripped; the user's actual request
        // is what survives.
        assert!(title.contains("ضيف سكشن"), "got {title:?}");
        assert!(title.contains("parquet_db"), "got {title:?}");
        assert!(!title.contains("@["), "anchor leaked: {title:?}");
        // Truncated to 80 chars max.
        assert!(title.chars().count() <= 80);
    }

    #[test]
    fn scan_falls_through_to_uuid_when_transcript_has_no_user_input() {
        let tmp = std::env::temp_dir().join("cctrace-antig-notranscript");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("66666666-6666-6666-6666-666666666666");
        let logs = folder.join(".system_generated/logs");
        fs::create_dir_all(&logs).unwrap();
        // Only an agent output, no user input.
        fs::write(
            logs.join("transcript.jsonl"),
            r#"{"step_index":0,"type":"AGENT_OUTPUT","content":"..."}"#,
        )
        .unwrap();

        let v = scan_antigravity_brain(&tmp);
        assert_eq!(v.len(), 1);
        assert!(
            v[0].title.is_none(),
            "no USER_INPUT → title is None (UUID fallback in UI)"
        );
    }

    #[test]
    fn load_transcript_returns_empty_when_transcript_missing() {
        let tmp = std::env::temp_dir().join("cctrace-antig-load-empty");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("77777777-7777-7777-7777-777777777777");
        fs::create_dir_all(&folder).unwrap();
        // No .system_generated/logs directory at all.
        assert!(load_antigravity_transcript(&folder).is_empty());
    }

    #[test]
    fn load_transcript_maps_types_to_roles_and_skips_system_rows() {
        let tmp = std::env::temp_dir().join("cctrace-antig-load-roles");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let folder = tmp.join("88888888-8888-8888-8888-888888888888");
        let logs = folder.join(".system_generated/logs");
        fs::create_dir_all(&logs).unwrap();

        let transcript = "\
{\"step_index\":0,\"type\":\"USER_INPUT\",\"created_at\":\"2026-09-29T20:08:27Z\",\"content\":\"<USER_REQUEST>hi</USER_REQUEST>\"}\n\
{\"step_index\":1,\"type\":\"CONVERSATION_HISTORY\",\"created_at\":\"2026-09-29T20:08:28Z\",\"content\":\"# Conversation History\\n<conversation_summaries>\\n## Conversation abc: title\\n</conversation_summaries>\"}\n\
{\"step_index\":2,\"type\":\"PLANNER_RESPONSE\",\"created_at\":\"2026-09-29T20:08:30Z\",\"content\":\"hello\"}\n\
{\"step_index\":3,\"type\":\"MCP_TOOL\",\"created_at\":\"2026-09-29T20:08:31Z\",\"content\":\"Created At: ...\\nargs\"}\n\
{\"step_index\":4,\"type\":\"VIEW_FILE\",\"created_at\":\"2026-09-29T20:08:32Z\",\"content\":\"x = 1\"}\n\
{\"step_index\":5,\"type\":\"GREP_SEARCH\",\"created_at\":\"2026-09-29T20:08:33Z\",\"content\":\"no results\"}\n\
{\"step_index\":6,\"type\":\"ERROR_MESSAGE\",\"created_at\":\"2026-09-29T20:08:34Z\",\"content\":\"boom\"}\n\
{\"step_index\":7,\"type\":\"CHECKPOINT\",\"created_at\":\"2026-09-29T20:08:35Z\",\"content\":\"# Resuming...\"}\n\
{\"step_index\":8,\"type\":\"KNOWLEDGE_ARTIFACTS\",\"created_at\":\"2026-09-29T20:08:36Z\",\"content\":\"hidden\"}\n\
";
        fs::write(logs.join("transcript.jsonl"), transcript).unwrap();

        let msgs = load_antigravity_transcript(&folder);
        // Only KNOWLEDGE_ARTIFACTS (8) is hidden. CONVERSATION_HISTORY (1)
        // is surfaced as the "context" role because it carries titles + objectives.
        assert_eq!(msgs.len(), 8, "expected 8 visible rows, got {msgs:#?}");

        let roles: Vec<&str> = msgs.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(
            roles,
            vec![
                "user",
                "context", // ← was hidden before; now surfaced
                "assistant",
                "tool",
                "file",
                "search",
                "error",
                "checkpoint",
            ],
        );

        // Step indexes preserved in order.
        let steps: Vec<u64> = msgs.iter().map(|m| m.step_index).collect();
        assert_eq!(steps, vec![0, 1, 2, 3, 4, 5, 6, 7]);

        // USER_INPUT content is preserved verbatim (no embellishment).
        assert!(msgs[0].content.contains("<USER_REQUEST>"));
        assert_eq!(msgs[0].created_at.as_deref(), Some("2026-09-29T20:08:27Z"));

        // CONVERSATION_HISTORY content is preserved (titles + summaries intact).
        let context = &msgs[1];
        assert_eq!(context.role, "context");
        assert!(context.content.contains("conversation_summaries"));
        assert!(context.content.contains("## Conversation abc: title"));
    }
}
