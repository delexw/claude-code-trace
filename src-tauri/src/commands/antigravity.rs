//! Read-only listing of Antigravity (Gemini IDE) brain folders.
//!
//! Each `~/.gemini/<flavor>/brain/<uuid>/` directory is one Antigravity
//! conversation. Antigravity ships under more than one flavor directory
//! (`antigravity-ide` on current installs, `antigravity` on older ones), and
//! a migration copies old conversations into the new root — so the same
//! conversation UUID can be present under two flavors. This module resolves
//! every known flavor, merges them and de-duplicates by UUID.
//!
//! Gating: enabled via `Settings.enable_antigravity`. Default is off, so the
//! picker never scans an Antigravity root unless the user opts in.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use crate::state::AppState;

/// The Antigravity "brain" subpath lives under `.gemini/<flavor>/`. Antigravity
/// has shipped under two flavor directory names: `antigravity-ide` on current
/// installs, `antigravity` on older ones. Ordered newest-first — the first
/// entry that exists on disk is the primary root the picker reports as
/// "Active", but every entry that exists is scanned and merged.
///
/// `antigravity-backup` is deliberately absent: it is a snapshot of the old
/// flavor, not an independent install, and scanning it would re-list every
/// old conversation a second time.
pub const ANTIGRAVITY_FLAVORS: &[&str] = &["antigravity-ide", "antigravity"];

/// The transcript path inside one conversation folder. Shared by the scanner
/// (to set `has_transcript`) and the loader.
const TRANSCRIPT_SUBPATH: &str = ".system_generated/logs/transcript.jsonl";

/// Env var that relocates the whole `.gemini` directory. Mirrors how
/// `dirs::home_dir()` is used elsewhere in the app.
pub const ENV_GEMINI_HOME: &str = "GEMINI_HOME";

/// One conversation folder under the brain root. Shown in the picker as a
/// pseudo-session.
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
    /// Best available one-line hint: the conversation metadata `summary`,
    /// else `task.md`'s first line, else the first `USER_INPUT` body.
    /// Truncated to 80 chars. `None` when no source is readable.
    pub title: Option<String>,
    /// Which flavor directory this folder was found under (`antigravity-ide`
    /// or `antigravity`). Lets the picker badge the install a conversation
    /// came from.
    pub flavor: String,
    /// Whether this folder actually holds a transcript. Brain roots also hold
    /// artifact/attachment folders (screenshots, scratch dirs) that have no
    /// conversation in them — the picker marks those instead of showing a
    /// "no transcript" error when they are opened.
    pub has_transcript: bool,
    /// Always `"antigravity"`. Lets the frontend badge future providers
    /// without a schema change.
    pub provider_id: &'static str,
}

/// One resolved brain root: a flavor directory name and the path it maps to.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BrainRoot {
    pub flavor: String,
    pub path: String,
    pub exists: bool,
}

/// One transcript step, normalized to a Claude-like shape so the frontend
/// can reuse the existing `MessageItem` rendering. Roles we emit:
/// - `"user"`        — USER_INPUT
/// - `"assistant"`   — PLANNER_RESPONSE
/// - `"tool"`        — MCP_TOOL
/// - `"file"`        — VIEW_FILE
/// - `"search"`      — GREP_SEARCH
/// - `"error"`       — ERROR_MESSAGE
/// - `"checkpoint"`  — CHECKPOINT (compaction)
/// - `"context"`     — CONVERSATION_HISTORY (Antigravity's roll-up of recent
///                     conversations — carries `<conversation_summaries>` with
///                     ids, titles and objectives)
/// - `"system"`      — anything else (LIST_DIRECTORY, CODE_ACTION,
///                     RUN_COMMAND, SYSTEM_MESSAGE, GENERIC, …)
///
/// `KNOWLEDGE_ARTIFACTS` is the one type dropped outright: it is pure engine
/// state with no reader value.
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
    /// Always `None` today. Tool calls arrive in a row's `tool_calls` array
    /// (`{name, args}`), not in a dedicated MCP_TOOL row — no transcript
    /// produced by Antigravity emits that type. Populated once the reader
    /// renders `tool_calls`.
    pub tool_name: Option<String>,
}

/// Join the `.gemini` base for a flavor. If `base` already *is* a `.gemini`
/// directory (which is what `GEMINI_HOME` conventionally points at — the same
/// way `dirs::home_dir()` points at the parent), do not append it twice.
fn gemini_base(base: &Path) -> PathBuf {
    if base.file_name().is_some_and(|n| n == ".gemini") {
        base.to_path_buf()
    } else {
        base.join(".gemini")
    }
}

/// Every brain root worth scanning, in priority order (newest flavor first).
/// Unlike [`antigravity_brain_root`] this does not require the candidate to
/// exist — the caller decides what to do with a missing directory (the picker
/// wants the list for its "also found" line; the scanner wants existing ones
/// only).
///
/// 1. `override_dir` (Settings.antigravity_root) when non-empty — returned
///    alone and used verbatim, so a user pointing at an arbitrary directory
///    gets exactly that directory and no flavor probing.
/// 2. `<gemini_home>/.gemini/<flavor>/brain` when `gemini_home` is set.
/// 3. `<os_home>/.gemini/<flavor>/brain` when `os_home` is set.
/// 4. Empty when no usable home exists.
pub fn brain_root_candidates(
    override_dir: Option<&str>,
    gemini_home: Option<&str>,
    os_home: Option<&str>,
) -> Vec<BrainRoot> {
    if let Some(p) = override_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
    {
        let exists = p.exists();
        return vec![BrainRoot {
            flavor: "custom".to_owned(),
            path: p.to_string_lossy().into_owned(),
            exists,
        }];
    }
    let base = gemini_home
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| os_home.map(PathBuf::from));
    let Some(base) = base else {
        return Vec::new();
    };
    let gemini = gemini_base(&base);
    ANTIGRAVITY_FLAVORS
        .iter()
        .map(|flavor| {
            let p = gemini.join(flavor).join("brain");
            let exists = p.exists();
            BrainRoot {
                flavor: (*flavor).to_owned(),
                path: p.to_string_lossy().into_owned(),
                exists,
            }
        })
        .collect()
}

/// The primary brain root — the first candidate. Kept as its own function
/// because the picker, the demo example and the settings response all want
/// exactly one path, and because it makes the resolution order testable
/// without touching the filesystem.
pub fn antigravity_brain_root(
    override_dir: Option<&str>,
    gemini_home: Option<&str>,
    os_home: Option<&str>,
) -> Option<PathBuf> {
    brain_root_candidates(override_dir, gemini_home, os_home)
        .into_iter()
        .map(|r| PathBuf::from(r.path))
        .next()
}

/// Whether a directory name looks like a conversation folder. Antigravity
/// names them as 36-character UUIDs; a brain root also holds non-conversation
/// folders (`tempmediaStorage` is a screenshot cache), which must not be listed
/// as conversations. Permissive enough to survive a future non-UUID naming
/// scheme — anything 36 characters long made of letters, digits and dashes
/// passes — while rejecting the caches.
fn looks_like_conversation_id(name: &str) -> bool {
    name.len() == 36
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
/// 80 chars. None when the transcript is missing, no usable USER_INPUT row
/// exists in the first 20 lines, or the request body is empty.
///
/// Rows that don't parse, carry no `content`, or wrap their text in something
/// other than `<USER_REQUEST>` are *skipped*, not fatal — a transcript whose
/// first user turn is malformed must not cost us the titles of every later
/// one.
fn read_first_user_input(folder: &Path) -> Option<String> {
    let path = folder.join(TRANSCRIPT_SUBPATH);
    let text = std::fs::read_to_string(&path).ok()?;
    for line in text.lines().take(20) {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("USER_INPUT") {
            continue;
        }
        let Some(content) = value.get("content").and_then(|c| c.as_str()) else {
            continue;
        };
        let Some(request) = extract_user_request(content) else {
            continue;
        };
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

/// Scan one brain root for conversation folders. Returns `[]` when `root`
/// does not exist (an unconfigured Antigravity install is not an error).
/// Non-conversation directories (screenshot caches and the like) are
/// skipped. Sort: newest modification first.
pub fn scan_antigravity_brain(root: &Path, flavor: &str) -> Vec<AntigravityBrainFolder> {
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
            if !looks_like_conversation_id(&uuid) {
                return None;
            }
            let mod_time_ms = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let has_transcript = p.join(TRANSCRIPT_SUBPATH).is_file();
            let title = read_folder_title(&p);
            Some(AntigravityBrainFolder {
                uuid,
                path: p.to_string_lossy().into_owned(),
                mod_time_ms,
                title,
                flavor: flavor.to_owned(),
                has_transcript,
                provider_id: "antigravity",
            })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.mod_time_ms));
    out
}

/// Which of two copies of the same conversation wins. Prefer the one that
/// actually has a transcript — a migration can copy a folder's artifacts
/// across without its transcript. On a tie keep the incumbent, which is the
/// copy from the earlier (newer) flavor, because roots are scanned in flavor
/// order.
fn prefer_folder(current: &AntigravityBrainFolder, incoming: &AntigravityBrainFolder) -> bool {
    incoming.has_transcript && !current.has_transcript
}

/// Merge every existing brain root into one de-duplicated list, newest first.
///
/// A conversation migrated from `antigravity` to `antigravity-ide` is present
/// under both UUIDs — identical `uuid`, two paths. Returning both would list
/// it twice, so the UUID is the key and [`prefer_folder`] picks the survivor.
pub fn merge_brain_roots(roots: &[BrainRoot]) -> Vec<AntigravityBrainFolder> {
    let mut by_uuid: HashMap<String, AntigravityBrainFolder> = HashMap::new();
    for root in roots.iter().filter(|r| r.exists) {
        for folder in scan_antigravity_brain(Path::new(&root.path), &root.flavor) {
            match by_uuid.get(&folder.uuid) {
                Some(existing) if prefer_folder(existing, &folder) => {
                    by_uuid.insert(folder.uuid.clone(), folder);
                }
                Some(_) => {}
                None => {
                    by_uuid.insert(folder.uuid.clone(), folder);
                }
            }
        }
    }
    let mut out: Vec<AntigravityBrainFolder> = by_uuid.into_values().collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.mod_time_ms));
    out
}

/// Resolves every brain root candidate using live settings + environment.
/// Public so the HTTP API and the settings response share one answer.
pub fn resolve_roots(settings: &crate::settings::Settings) -> Vec<BrainRoot> {
    let os_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    brain_root_candidates(
        settings.antigravity_root.as_deref(),
        std::env::var(ENV_GEMINI_HOME).ok().as_deref(),
        os_home.as_deref(),
    )
}

/// The primary brain root plus whether it exists. `path` is empty when no
/// home directory is known.
pub fn resolve_root(settings: &crate::settings::Settings) -> (PathBuf, bool) {
    let (root, exists) = resolve_root_entry(settings);
    (PathBuf::from(root), exists)
}

/// [`resolve_root`]'s scalar form, for callers that want a string.
pub fn resolve_root_entry(settings: &crate::settings::Settings) -> (String, bool) {
    match resolve_roots(settings).into_iter().next() {
        Some(r) => (r.path, r.exists),
        None => (String::new(), false),
    }
}

/// Pure helper: returns the brain folders currently visible to the user.
/// Empty when the user has not opted in (`Settings.enable_antigravity ==
/// false`) or no candidate root exists on disk.
///
/// Shared by the Tauri command and the HTTP API so both paths get the same
/// answer regardless of which entry point is used.
pub fn list_visible_antigravity_folders(
    settings: &crate::settings::Settings,
) -> Vec<AntigravityBrainFolder> {
    if !settings.enable_antigravity {
        return Vec::new();
    }
    merge_brain_roots(&resolve_roots(settings))
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
    let transcript = path.join(TRANSCRIPT_SUBPATH);
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
        // Real transcripts also carry LIST_DIRECTORY, CODE_ACTION,
        // RUN_COMMAND, INVOKE_SUBAGENT, SYSTEM_MESSAGE and GENERIC.
        _ => "system",
    };
    // Tool names live in the row's `tool_calls` array (`{name, args}`), which
    // no row type carries as a name of its own — MCP_TOOL, the only type this
    // field was written for, does not occur in real transcripts. Populated
    // when the reader starts rendering `tool_calls`.
    let tool_name: Option<String> = None;
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

    fn tmpdir(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    /// Create `<root>/<uuid>` with a one-line task title.
    fn make_folder(root: &Path, uuid: &str, title: &str) -> PathBuf {
        let dir = root.join(uuid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("task.md"), title).unwrap();
        dir
    }

    /// Create `<root>/<uuid>/.system_generated/logs/transcript.jsonl`.
    fn make_transcript(root: &Path, uuid: &str, rows: &str) -> PathBuf {
        let dir = root.join(uuid).join(".system_generated/logs");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("transcript.jsonl"), rows).unwrap();
        root.join(uuid)
    }

    fn candidate_roots(home: &Path) -> Vec<BrainRoot> {
        brain_root_candidates(None, None, Some(&home.to_string_lossy()))
    }

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

    // ---- flavor candidates ------------------------------------------------

    #[test]
    fn candidates_cover_both_flavors_newest_first() {
        let c = candidate_roots(Path::new("/home/u"));
        assert_eq!(c.len(), 2, "got {c:#?}");
        assert_eq!(c[0].flavor, "antigravity-ide");
        assert_eq!(c[1].flavor, "antigravity");
        assert_eq!(c[0].path, "/home/u/.gemini/antigravity-ide/brain");
        assert_eq!(c[1].path, "/home/u/.gemini/antigravity/brain");
    }

    #[test]
    fn candidates_exclude_the_backup_snapshot() {
        // `antigravity-backup` is a snapshot of the old flavor, not a separate
        // install. Probing it would list every old conversation twice, so it
        // must never appear as a candidate.
        let roots = candidate_roots(Path::new("/home/u"));
        let flavors: Vec<&str> = roots.iter().map(|r| r.flavor.as_str()).collect();
        assert!(!flavors.iter().any(|f| f.contains("backup")), "{flavors:?}");
        assert_eq!(ANTIGRAVITY_FLAVORS, &["antigravity-ide", "antigravity"]);
    }

    #[test]
    fn gemini_home_pointing_at_dot_gemini_is_not_doubled() {
        // GEMINI_HOME conventionally names the `.gemini` directory itself, so
        // joining `.gemini` again would produce `…/.gemini/.gemini/…`.
        let r = antigravity_brain_root(None, Some("/data/g/.gemini"), Some("/home/u"));
        assert_eq!(
            r,
            Some(PathBuf::from("/data/g/.gemini/antigravity-ide/brain"))
        );
    }

    #[test]
    fn os_home_never_doubles_the_gemini_segment() {
        let r = antigravity_brain_root(None, None, Some("/home/u/.gemini"));
        assert_eq!(
            r,
            Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
        );
    }

    #[test]
    fn override_short_circuits_flavor_probing() {
        let c = brain_root_candidates(Some("/custom/brain"), Some("/data/g"), Some("/home/u"));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].flavor, "custom");
        assert_eq!(c[0].path, "/custom/brain");
    }

    #[test]
    fn candidates_report_existence() {
        let tmp = tmpdir("cctrace-antig-cand-exists");
        fs::create_dir_all(tmp.join(".gemini/antigravity/brain")).unwrap();
        let c = candidate_roots(&tmp);
        assert!(!c[0].exists, "antigravity-ide not created → false");
        assert!(c[1].exists, "antigravity created → true");
    }

    #[test]
    fn scan_returns_empty_when_root_missing() {
        let tmp = std::env::temp_dir().join("cctrace-antig-missing");
        let _ = fs::remove_dir_all(&tmp);
        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let results = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
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

    #[test]
    fn load_transcript_maps_the_types_real_transcripts_carry() {
        // Types measured from live Antigravity transcripts that the explicit
        // match arm does not name. They must surface as `system` rather than
        // being dropped.
        let tmp = tmpdir("cctrace-antig-load-unmapped");
        let uuid = "99999999-9999-9999-9999-999999999999";
        make_transcript(
            &tmp,
            uuid,
            concat!(
                r#"{"step_index":0,"type":"LIST_DIRECTORY","content":"a"}"#,
                "\n",
                r#"{"step_index":1,"type":"CODE_ACTION","content":"b"}"#,
                "\n",
                r#"{"step_index":2,"type":"RUN_COMMAND","content":"c"}"#,
                "\n",
                r#"{"step_index":3,"type":"INVOKE_SUBAGENT","content":"d"}"#,
                "\n",
                r#"{"step_index":4,"type":"SYSTEM_MESSAGE","content":"e"}"#,
                "\n",
                r#"{"step_index":5,"type":"GENERIC","content":"f"}"#,
                "\n",
            ),
        );
        let msgs = load_antigravity_transcript(&tmp.join(uuid));
        assert_eq!(msgs.len(), 6, "got {msgs:#?}");
        assert!(msgs.iter().all(|m| m.role == "system"));
    }

    // ---- scanning --------------------------------------------------------

    #[test]
    fn scan_labels_each_folder_with_its_flavor() {
        let tmp = tmpdir("cctrace-antig-flavor");
        make_folder(&tmp, "00000000-0000-0000-0000-0000000000aa", "x");
        let v = scan_antigravity_brain(&tmp, "antigravity");
        assert_eq!(v[0].flavor, "antigravity");
    }

    #[test]
    fn scan_skips_non_conversation_directories() {
        // Real brain roots carry a `tempmediaStorage` screenshot cache beside
        // the conversation folders. Listing it would show a phantom
        // conversation with a garbled title.
        let tmp = tmpdir("cctrace-antig-nonuuid");
        make_folder(
            &tmp,
            "00000000-0000-0000-0000-0000000000bb",
            "Real conversation",
        );
        fs::create_dir_all(tmp.join("tempmediaStorage")).unwrap();
        fs::create_dir_all(tmp.join("scrap")).unwrap();

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
        assert_eq!(v.len(), 1, "got {v:#?}");
        assert_eq!(v[0].uuid, "00000000-0000-0000-0000-0000000000bb");
    }

    #[test]
    fn scan_marks_which_folders_actually_hold_a_transcript() {
        let tmp = tmpdir("cctrace-antig-has-transcript");
        make_transcript(
            &tmp,
            "00000000-0000-0000-0000-0000000000cc",
            "{\"step_index\":0,\"type\":\"USER_INPUT\",\"content\":\"hi\"}\n",
        );
        // Artifact-only folder: screenshots and reports, no conversation.
        let artifacts = make_folder(&tmp, "00000000-0000-0000-0000-0000000000dd", "artifacts");
        fs::write(artifacts.join("shot.webp"), b"x").unwrap();

        let mut v = scan_antigravity_brain(&tmp, "antigravity-ide");
        v.sort_by(|a, b| a.uuid.cmp(&b.uuid));
        assert!(v[0].has_transcript, "{}", v[0].uuid);
        assert!(!v[1].has_transcript, "{}", v[1].uuid);
    }

    #[test]
    fn malformed_first_user_input_does_not_hide_later_titles() {
        // A transcript whose first USER_INPUT row carries no `content`, or
        // wraps its text in something other than <USER_REQUEST>, must not cost
        // us the title a later row provides.
        let tmp = tmpdir("cctrace-antig-malformed-input");
        make_transcript(
            &tmp,
            "66666666-6666-6666-6666-666666666666",
            concat!(
                r#"{"step_index":0,"type":"USER_INPUT"}"#,
                "\n",
                r#"{"step_index":1,"type":"USER_INPUT","content":"no markers here"}"#,
                "\n",
                r#"{"step_index":2,"type":"USER_INPUT","content":"<USER_REQUEST>the real request</USER_REQUEST>"}"#,
                "\n",
            ),
        );

        let v = scan_antigravity_brain(&tmp, "antigravity-ide");
        assert_eq!(
            v[0].title.as_deref(),
            Some("the real request"),
            "got {:?}",
            v[0].title
        );
    }

    // ---- merging across flavors -------------------------------------------

    #[test]
    fn merge_skips_roots_that_do_not_exist() {
        let tmp = tmpdir("cctrace-antig-merge-missing");
        let roots = vec![
            BrainRoot {
                flavor: "antigravity-ide".into(),
                path: tmp.join("nope-ide").to_string_lossy().into_owned(),
                exists: false,
            },
            BrainRoot {
                flavor: "antigravity".into(),
                path: tmp.join("nope-old").to_string_lossy().into_owned(),
                exists: false,
            },
        ];
        assert!(merge_brain_roots(&roots).is_empty());
    }

    #[test]
    fn merge_lists_each_conversation_once_across_flavors() {
        // A migration copies old conversations into the new root, so the same
        // UUID sits under both flavors. Merging naively would double them.
        let tmp = tmpdir("cctrace-antig-merge-dup");
        let gemini = tmp.join(".gemini");
        let ide = gemini.join("antigravity-ide/brain");
        let old = gemini.join("antigravity/brain");
        fs::create_dir_all(&ide).unwrap();
        fs::create_dir_all(&old).unwrap();

        let shared = "aaaaaaaa-0000-0000-0000-000000000001";
        make_folder(&ide, shared, "Migrated conversation");
        make_folder(&old, shared, "Migrated conversation");
        make_folder(
            &old,
            "aaaaaaaa-0000-0000-0000-000000000002",
            "Old-only conversation",
        );
        make_folder(
            &ide,
            "aaaaaaaa-0000-0000-0000-000000000003",
            "New-only conversation",
        );

        let roots = vec![
            BrainRoot {
                flavor: "antigravity-ide".into(),
                path: ide.to_string_lossy().into_owned(),
                exists: true,
            },
            BrainRoot {
                flavor: "antigravity".into(),
                path: old.to_string_lossy().into_owned(),
                exists: true,
            },
        ];
        let v = merge_brain_roots(&roots);
        assert_eq!(v.len(), 3, "got {v:#?}");

        let shared_entry = v
            .iter()
            .find(|f| f.uuid == shared)
            .expect("shared conversation listed exactly once");
        // Newest flavor wins the tie.
        assert_eq!(shared_entry.flavor, "antigravity-ide");
        assert_eq!(shared_entry.title.as_deref(), Some("Migrated conversation"));
    }

    #[test]
    fn merge_prefers_the_copy_that_has_a_transcript() {
        // A migration can carry a folder's artifacts across without its
        // transcript. The copy that can actually be opened is the one to show.
        let tmp = tmpdir("cctrace-antig-merge-transcript");
        let gemini = tmp.join(".gemini");
        let ide = gemini.join("antigravity-ide/brain");
        let old = gemini.join("antigravity/brain");
        let uuid = "bbbbbbbb-0000-0000-0000-000000000001";
        make_transcript(
            &old,
            uuid,
            concat!(
                r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>kept</USER_REQUEST>"}"#,
                "\n",
            ),
        );
        make_folder(&ide, uuid, "Artifacts only, transcript lost in migration");

        let roots = vec![
            BrainRoot {
                flavor: "antigravity-ide".into(),
                path: ide.to_string_lossy().into_owned(),
                exists: true,
            },
            BrainRoot {
                flavor: "antigravity".into(),
                path: old.to_string_lossy().into_owned(),
                exists: true,
            },
        ];
        let v = merge_brain_roots(&roots);
        assert_eq!(v.len(), 1, "got {v:#?}");
        assert!(v[0].has_transcript);
        assert_eq!(v[0].flavor, "antigravity");
        assert!(v[0].path.contains("/antigravity/brain/"), "{}", v[0].path);
        assert_eq!(v[0].title.as_deref(), Some("kept"));
    }

    #[test]
    fn merge_sorts_newest_first() {
        let tmp = tmpdir("cctrace-antig-merge-order");
        let gemini = tmp.join(".gemini");
        let ide = gemini.join("antigravity-ide/brain");
        let old = gemini.join("antigravity/brain");
        fs::create_dir_all(&ide).unwrap();
        fs::create_dir_all(&old).unwrap();
        make_folder(&old, "cccccccc-0000-0000-0000-000000000001", "older");
        std::thread::sleep(std::time::Duration::from_millis(10));
        make_folder(&ide, "cccccccc-0000-0000-0000-000000000002", "newer");

        let roots = vec![
            BrainRoot {
                flavor: "antigravity-ide".into(),
                path: ide.to_string_lossy().into_owned(),
                exists: true,
            },
            BrainRoot {
                flavor: "antigravity".into(),
                path: old.to_string_lossy().into_owned(),
                exists: true,
            },
        ];
        let v = merge_brain_roots(&roots);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title.as_deref(), Some("newer"));
        assert_eq!(v[1].title.as_deref(), Some("older"));
    }
}
