//! Where Antigravity keeps its conversations, and listing them.
//!
//! Each `<root>/<uuid>/` directory under a brain root is one Antigravity
//! conversation. Antigravity ships under more than one flavor directory
//! (`antigravity-ide` on current installs, `antigravity` on older ones), and
//! a migration copies old conversations into the new root — so the same
//! conversation UUID can be present under two flavors. This module resolves
//! every known flavor, merges them and de-duplicates by UUID.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::transcript::TRANSCRIPT_SUBPATH;

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
}

/// One resolved brain root: a flavor directory name and the path it maps to.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BrainRoot {
    pub flavor: String,
    pub path: String,
    pub exists: bool,
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
/// 1. `override_dir` (the plugin's `root` setting) when non-empty — returned
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

/// The primary brain root — the first candidate. Makes the resolution order
/// testable without touching the filesystem.
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
