//! Reading one conversation's `transcript.jsonl`: a byte-offset index of the
//! steps a reader can see, and the message bodies for any window of them.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The transcript path inside one conversation folder. Shared by the scanner
/// (to set `has_transcript`) and the loader.
pub(super) const TRANSCRIPT_SUBPATH: &str = ".system_generated/logs/transcript.jsonl";

/// One entry of a row's `tool_calls` array. Antigravity writes the tool name
/// plus an `args` object; only `toolSummary` is human-readable (the other args
/// are JSON-escaped absolute paths), so that is what the reader shows.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AntigravityToolCall {
    pub name: String,
    pub summary: String,
    /// The call's `args`, pretty-printed. This is where a brain step's payload
    /// actually lives when `content` is empty — measured over 9,058 tool calls,
    /// the args carry `CommandLine`, `AbsolutePath`, `Query`, `Arguments` and
    /// more. Empty when the row has no `args` object.
    pub input: String,
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
///
/// **`content` is often absent by construction.** Measured over 25,064 rows of
/// real brain transcripts, 10,328 carry no `content` at all — and 8,829 of the
/// 11,059 `PLANNER_RESPONSE` rows are among them, because a planner turn that
/// only thought, or only called a tool, writes its payload into `thinking` or
/// `tool_calls` instead. Those are surfaced as their own fields below; the
/// reader composes a body from whichever of the three it has, rather than
/// showing an empty card.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::doc_overindented_list_items)]
pub struct AntigravityMessage {
    pub step_index: u64,
    pub role: String,
    pub content: String,
    pub created_at: Option<String>,
    /// Tool calls this row issued, from its `tool_calls` array. Real
    /// transcripts carry them on `PLANNER_RESPONSE` rows and on `MCP_TOOL`
    /// rows alike — 8,351 of the empty rows above hold nothing else.
    pub tool_calls: Vec<AntigravityToolCall>,
    /// The row's `thinking` block, when it has one (2,931 rows). Empty for the
    /// majority of rows, which is why it stays an `Option`.
    pub thinking_text: Option<String>,
    /// The row's `error` text (463 rows). Only `ERROR_MESSAGE` rows carry it.
    pub error_text: Option<String>,
    /// Wall-clock gap to the next timestamped row, in ms. `0` when either
    /// timestamp is missing or unparseable, and for the final row — the stats
    /// bar hides a zero duration rather than drawing one.
    pub duration_ms: u64,
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
    stamp_durations(&mut out);
    out
}

/// The role a transcript row's `type` maps to, or `None` for a row the reader
/// drops. Shared by the index pass and the body parser so the two always agree
/// on which rows exist — an index that kept a row the parser drops would hand
/// the frontend an index it can never fill.
fn role_for_type(r#type: &str) -> Option<&'static str> {
    Some(match r#type {
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
    })
}

/// A row's position in the transcript, without its content.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptRowRef {
    pub role: String,
    /// Byte offset of the row's line within `transcript.jsonl`.
    pub offset: u64,
    /// Length of that line in bytes, newline excluded.
    pub len: u64,
}

/// Everything the reader needs before any message body is built: how many
/// steps there are, and which role each one carries.
///
/// One pass over the file, no `AntigravityMessage` and no `DisplayMessage`
/// built — that is the point. A 2.7 MB transcript becomes ~2,300 small structs
/// instead of 2,300 parsed-and-nested objects shipped whole to the browser.
///
/// Steps that recorded nothing are left out here rather than in the frontend.
/// With paging the frontend no longer holds a body to test, and this pass is
/// already reading every row — so the decision belongs here, where it also
/// keeps the index and the window loader agreeing on what exists.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptIndex {
    pub rows: Vec<TranscriptRowRef>,
    /// Reader-visible steps, i.e. `rows.len()`.
    pub count: usize,
    /// Every step the reader would otherwise show, including the ones that
    /// recorded nothing. Distinguishes "no transcript" from "a transcript of
    /// empty steps", which need different words.
    pub total_rows: usize,
}

/// An index plus the transcript stamp it was built from.
type CachedIndex = (u64, u64, TranscriptIndex);

/// Transcripts are immutable once written, so an index stays valid until the
/// file's length or mtime changes. Without this, every page request re-ran the
/// full index pass and paging measured *slower* than reading the whole file.
fn index_cache() -> &'static Mutex<HashMap<PathBuf, CachedIndex>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedIndex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How many transcripts to remember. A picker shows one conversation at a time;
/// the bound only exists so walking a whole brain root cannot grow it forever.
const INDEX_CACHE_ENTRIES: usize = 8;

/// A stored index plus the transcript stamp it was built from. The stamp is the
/// whole validity check: a conversation that grew or was rewritten produces a
/// different one, so a stale index is discarded rather than trusted.
#[derive(Serialize, Deserialize)]
struct SidecarIndex {
    len: u64,
    modified: u64,
    index: TranscriptIndex,
}

/// Where sidecar indexes live. Under the app's config dir, never beside the
/// transcript: a brain root belongs to the Gemini IDE and this tool only reads
/// it.
fn sidecar_dir() -> Option<PathBuf> {
    // Tests build throwaway transcripts; their sidecars belong beside them, not
    // in the developer's real config dir where they would evict real indexes.
    let base = if cfg!(test) {
        std::env::temp_dir()
    } else {
        crate::settings::config_root()?
    };
    Some(base.join("cctrace-antigravity-index"))
}

/// One file per transcript, named by a hash of its path — transcripts live under
/// user and Windows paths that no filename rule can hold.
pub(super) fn sidecar_path(transcript: &Path) -> Option<PathBuf> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    transcript.hash(&mut hasher);
    Some(sidecar_dir()?.join(format!("{:016x}.json", hasher.finish())))
}

/// Keeps the directory from growing forever as conversations come and go.
/// Anything past the bound goes, oldest first.
const SIDECAR_ENTRIES: usize = 256;

fn prune_sidecars(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "json") {
                let modified = e.metadata().ok()?.modified().ok()?;
                Some((modified, path))
            } else {
                None
            }
        })
        .collect();
    if files.len() <= SIDECAR_ENTRIES {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in files.iter().take(files.len() - SIDECAR_ENTRIES) {
        let _ = std::fs::remove_file(path);
    }
}

fn read_sidecar(transcript: &Path, len: u64, modified: u64) -> Option<TranscriptIndex> {
    let path = sidecar_path(transcript)?;
    let text = std::fs::read_to_string(path).ok()?;
    let stored: SidecarIndex = serde_json::from_str(&text).ok()?;
    (stored.len == len && stored.modified == modified).then_some(stored.index)
}

/// Written through a temporary file and renamed, so a crash mid-write cannot
/// leave a half-written index that later parses as valid.
fn write_sidecar(transcript: &Path, len: u64, modified: u64, index: &TranscriptIndex) {
    let Some(dir) = sidecar_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Some(path) = sidecar_path(transcript) else {
        return;
    };
    let payload = SidecarIndex {
        len,
        modified,
        index: index.clone(),
    };
    let Ok(text) = serde_json::to_string(&payload) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, &path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    prune_sidecars(&dir);
}

/// Index a transcript without building any message bodies.
///
/// Three tiers, cheapest first: this process's memory, then a sidecar file in
/// the app's config dir, then the full pass. The sidecar is what makes opening a
/// conversation fast on a real install — the full pass over a 2.6 MB transcript
/// costs more than reading the file outright, because it parses every row to
/// find offsets and roles.
pub fn index_antigravity_transcript(path: &Path) -> TranscriptIndex {
    let transcript = path.join(TRANSCRIPT_SUBPATH);
    let stamp = std::fs::metadata(&transcript).ok().map(|m| {
        let modified = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        (m.len(), modified)
    });
    let Some((len, modified)) = stamp else {
        return TranscriptIndex {
            rows: Vec::new(),
            count: 0,
            total_rows: 0,
        };
    };
    if let Ok(cache) = index_cache().lock() {
        if let Some((l, m, index)) = cache.get(&transcript) {
            if *l == len && *m == modified {
                return index.clone();
            }
        }
    }
    let index = match read_sidecar(&transcript, len, modified) {
        Some(stored) => stored,
        None => {
            let fresh = index_transcript_uncached(&transcript);
            write_sidecar(&transcript, len, modified, &fresh);
            fresh
        }
    };
    if let Ok(mut cache) = index_cache().lock() {
        if cache.len() >= INDEX_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(transcript, (len, modified, index.clone()));
    }
    index
}

/// The one full pass over a transcript: byte offsets and roles, no bodies.
fn index_transcript_uncached(transcript: &Path) -> TranscriptIndex {
    let file = match std::fs::File::open(transcript) {
        Ok(f) => f,
        Err(_) => {
            return TranscriptIndex {
                rows: Vec::new(),
                count: 0,
                total_rows: 0,
            }
        }
    };
    let mut reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut buf = Vec::new();
    let mut offset: u64 = 0;
    let mut rows = Vec::new();
    let mut total_rows = 0usize;
    loop {
        buf.clear();
        // `read_until` leaves the trailing newline in the buffer; the length
        // recorded excludes it so a later `read_exact` reads the row alone.
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let total = buf.len() as u64;
        let line_len = if buf.last() == Some(&b'\n') {
            total - 1
        } else {
            total
        };
        let trimmed: &[u8] = &buf[..line_len as usize];
        if !trimmed.is_empty() {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(trimmed) {
                let has_step = value.get("step_index").and_then(|s| s.as_u64()).is_some();
                let r#type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if has_step {
                    if let Some(role) = role_for_type(r#type) {
                        total_rows += 1;
                        if row_has_payload(&value) {
                            rows.push(TranscriptRowRef {
                                role: role.to_owned(),
                                offset,
                                len: line_len,
                            });
                        }
                    }
                }
            }
        }
        offset += total;
    }
    TranscriptIndex {
        count: rows.len(),
        total_rows,
        rows,
    }
}

/// Whether a row carries anything a reader can see. Measured over 25,064 real
/// rows, 1,847 carry none of the four — an `EPHEMERAL_MESSAGE` with no content,
/// a `PLANNER_RESPONSE` that only advanced a cursor. They are real steps, but as
/// cards they are a header, a timestamp and silence.
fn row_has_payload(value: &serde_json::Value) -> bool {
    value
        .get("content")
        .and_then(|c| c.as_str())
        .is_some_and(|c| !c.trim().is_empty())
        || non_empty(value.get("thinking")).is_some()
        || non_empty(value.get("error")).is_some()
        || value
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .is_some_and(|calls| !calls.is_empty())
}

/// Read only rows `[start, start + limit)` of a transcript, as the reader's
/// message bodies.
///
/// One row past the window is read too — never returned — because a row's
/// duration is the gap to its successor and the last row of the transcript has
/// no successor inside the window.
pub fn load_antigravity_transcript_window(
    path: &Path,
    start: usize,
    limit: usize,
) -> (Vec<AntigravityMessage>, usize) {
    let index = index_antigravity_transcript(path);
    let total = index.count;
    if start >= total || limit == 0 {
        return (Vec::new(), total);
    }
    let end = (start + limit).min(total);
    let transcript = path.join(TRANSCRIPT_SUBPATH);
    let mut file = match std::fs::File::open(&transcript) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), total),
    };

    // Rows `[start, end)` are contiguous in the file, so the whole window is
    // one byte range: seek once and read it straight through. Seeking per row
    // cost ~140 small reads on the 9P mount and made paging slower than reading
    // the entire transcript.
    let from = index.rows[start].offset;
    let last = &index.rows[end - 1];
    let to = last.offset + last.len + 1;
    // One row past the window, read but never returned: a row's duration is the
    // gap to its successor, and the last row of the transcript has none inside
    // the window.
    let read_to = index
        .rows
        .get(end)
        .map(|next| next.offset + next.len + 1)
        .unwrap_or(to);

    use std::io::{Read, Seek, SeekFrom};
    if file.seek(SeekFrom::Start(from)).is_err() {
        return (Vec::new(), total);
    }
    // The row length excludes its newline, so `read_to` is one byte past the
    // last row *including* that newline. A transcript whose final line has no
    // trailing newline ends there, and demanding the byte would fail the read
    // and blank the last page — so clamp to the real end and accept a short
    // read rather than insisting on it.
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(read_to);
    let read_to = read_to.min(file_len);
    if read_to <= from {
        return (Vec::new(), total);
    }
    let mut buf = vec![0u8; (read_to - from) as usize];
    if file.read(&mut buf).is_err() {
        return (Vec::new(), total);
    }

    // A byte range covers every *physical* line between the window's first and
    // last row, and dropped rows (`KNOWLEDGE_ARTIFACTS`) still occupy lines.
    // So count reader rows, not lines — the same filter the index applied.
    let lines: Vec<&[u8]> = buf.split(|b| *b == b'\n').collect();
    let wanted = end - start;
    let mut parsed: Vec<AntigravityMessage> = Vec::with_capacity(wanted + 1);
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        // The index left payload-less rows out; this reader must too, or the
        // window fills with rows the index never pointed at and every
        // conversation comes back shifted.
        if !row_has_payload(&value) {
            continue;
        }
        let Some(message) = parse_transcript_row(&value) else {
            continue;
        };
        parsed.push(message);
        if parsed.len() > wanted {
            break; // the successor, read only to time the row before it
        }
    }
    for i in 0..parsed.len().min(wanted) {
        if let Some(next) = parsed.get(i + 1) {
            parsed[i].duration_ms =
                gap_ms(parsed[i].created_at.as_deref(), next.created_at.as_deref());
        }
    }
    parsed.truncate(wanted);
    (parsed, total)
}

/// The gap between two timestamps, in ms; 0 when either is missing or the
/// timestamps run backwards.
fn gap_ms(from: Option<&str>, to: Option<&str>) -> u64 {
    let (Some(a), Some(b)) = (
        from.and_then(parse_rfc3339_ms),
        to.and_then(parse_rfc3339_ms),
    ) else {
        return 0;
    };
    b.saturating_sub(a).max(0) as u64
}

/// Map a single transcript.jsonl row to our normalized message shape.
/// Returns `None` only for `KNOWLEDGE_ARTIFACTS`, which is pure engine state.
/// Every other type is surfaced: `CONVERSATION_HISTORY` as `"context"`, and
/// anything unrecognized as `"system"`.
fn parse_transcript_row(value: &serde_json::Value) -> Option<AntigravityMessage> {
    let step_index = value.get("step_index")?.as_u64()?;
    let r#type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let role = role_for_type(r#type)?;
    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_owned();
    let created_at = value
        .get("created_at")
        .and_then(|c| c.as_str())
        .map(str::to_owned);
    let tool_calls = parse_tool_calls(value.get("tool_calls"));
    let thinking_text = non_empty(value.get("thinking"));
    let error_text = non_empty(value.get("error"));
    Some(AntigravityMessage {
        step_index,
        role: role.to_owned(),
        content,
        created_at,
        tool_calls,
        thinking_text,
        error_text,
        // Filled in by `stamp_durations` for a whole-transcript read, or by
        // the window reader from the next row's timestamp.
        duration_ms: 0,
    })
}

/// A present, non-blank string field, or `None`. A transcript row carries these
/// keys only when it has something to say, so absence is the normal case.
fn non_empty(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Normalize a row's `tool_calls` array into `{name, summary}` pairs.
///
/// Antigravity writes `{"name": "view_file", "args": {"toolSummary":
/// "\"Viewing file\"", ...}}`: the args values are JSON-escaped strings, so the
/// summary arrives wrapped in a second layer of quotes that must come off
/// before it is shown.
fn parse_tool_calls(value: Option<&serde_json::Value>) -> Vec<AntigravityToolCall> {
    let Some(array) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|call| {
            let name = call.get("name")?.as_str()?.to_owned();
            let args = call.get("args");
            let summary = args
                .and_then(|args| args.get("toolSummary"))
                .and_then(|s| s.as_str())
                .map(unquote)
                .unwrap_or_default();
            let input = args
                .filter(|a| a.is_object() && !a.as_object().is_some_and(|o| o.is_empty()))
                .and_then(|a| serde_json::to_string_pretty(a).ok())
                .unwrap_or_default();
            Some(AntigravityToolCall {
                name,
                summary,
                input,
            })
        })
        .collect()
}

/// Strip the extra quoting Antigravity wraps around its `args` strings, so
/// `"\"Viewing file\""` reads as `Viewing file`.
fn unquote(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed)
        .replace("\\\"", "\"")
}

/// Give every row the wall-clock gap to its successor. A row without a
/// parseable timestamp — or the last row — keeps 0, which the stats bar hides.
/// Time each step to the next one the reader actually shows.
///
/// The successor is the next **visible** step, not the next physical row: a
/// step's duration should read as the gap the user sees before the next card,
/// and 1,847 rows per install are left out for recording nothing. Timed against
/// the raw next row instead, the whole-file path and the paged path disagreed,
/// so the same step showed two different durations depending on how it was
/// loaded. Walking backwards keeps this one pass.
fn stamp_durations(messages: &mut [AntigravityMessage]) {
    let mut successor: Option<i64> = None;
    for i in (0..messages.len()).rev() {
        let ts = messages[i].created_at.as_deref().and_then(parse_rfc3339_ms);
        if !message_has_payload(&messages[i]) {
            continue;
        }
        messages[i].duration_ms = match (ts, successor) {
            (Some(a), Some(b)) => b.saturating_sub(a).max(0) as u64,
            _ => 0,
        };
        if ts.is_some() {
            successor = ts;
        }
    }
}

/// The payload test, applied to an already-built message. Mirrors
/// [`row_has_payload`], which works on the raw JSON during indexing.
pub(super) fn message_has_payload(message: &AntigravityMessage) -> bool {
    !message.content.trim().is_empty()
        || message.thinking_text.is_some()
        || message.error_text.is_some()
        || !message.tool_calls.is_empty()
}

fn parse_rfc3339_ms(raw: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.timestamp_millis())
}

/// One page of a conversation, plus the index that describes all of it —
/// the same shape the Claude reader gets from `load_session`.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AntigravityWindow {
    /// The bodies for `[start, start + messages.len())`. Empty when `limit` is 0,
    /// which is how the reader asks for the index on its own.
    pub messages: Vec<AntigravityMessage>,
    /// Total reader-visible steps in this conversation.
    pub count: usize,
    /// Index of the first returned message within the conversation.
    pub start: usize,
    /// Role of every step, so the list can draw placeholders before their
    /// bodies arrive.
    pub roles: Vec<String>,
    /// Every step the transcript holds, including the ones that recorded
    /// nothing and are therefore absent from `roles`.
    pub total_rows: usize,
}

/// `limit == 0` returns the index only — every role and the total count, with no
/// bodies built. That is what the reader asks for when it opens a conversation,
/// so a 2.7 MB transcript costs one pass and nothing else.
pub fn load_antigravity_window(path: &Path, start: usize, limit: usize) -> AntigravityWindow {
    let index = index_antigravity_transcript(path);
    let roles: Vec<String> = index.rows.iter().map(|r| r.role.clone()).collect();
    let count = index.count;
    if limit == 0 || start >= count {
        return AntigravityWindow {
            messages: Vec::new(),
            count,
            start,
            roles,
            total_rows: index.total_rows,
        };
    }
    let (messages, _) = load_antigravity_transcript_window(path, start, limit);
    AntigravityWindow {
        messages,
        count,
        start,
        roles,
        total_rows: index.total_rows,
    }
}
