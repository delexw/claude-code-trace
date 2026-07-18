//! Shared session-load pipeline.
//!
//! Building the display messages for a session (parse → chunk → link subagents →
//! inject orphans → reconstruct teams → convert to messages) was duplicated
//! across the Tauri command, the HTTP API, and the watcher. This module owns the
//! single implementation and adds index-range windowing so virtualized clients
//! can fetch only the slice they're viewing.
//!
//! Windowing slices the *fully built* message list by index. The build itself is
//! still whole-file (message linking has cross-message dependencies), so this is
//! correct but O(file) per call; an incremental byte-offset index can later make
//! range fetches O(window) without changing this module's contract.

use chrono::{DateTime, Utc};

use crate::convert::{chunks_to_messages, DisplayMessage, LoadResult, SessionTotals};
use crate::parser::chunk::{build_chunks, Chunk};
use crate::parser::classify::ClassifiedMsg;
use crate::parser::ongoing::OngoingChecker;
use crate::parser::session::{extract_session_meta, read_session_with_debug_hooks, SessionMeta};
use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};
use crate::parser::team::{reconstruct_teams, TeamSnapshot};

/// The display-level build of a session: messages, teams, ongoing status and the
/// live permission mode. Deliberately excludes session totals and file metadata
/// so the watcher can reuse it on every change without paying for a full token
/// re-scan (it keeps its own incremental scanner).
pub struct ViewBuild {
    pub messages: Vec<DisplayMessage>,
    pub teams: Vec<TeamSnapshot>,
    pub ongoing: bool,
    /// Latest permission mode, derived from the classified messages (free — the
    /// entries were already read to build the view).
    pub permission_mode: String,
}

/// A fully built session: the display view plus file metadata and token totals.
/// The caller decides what to do with `ongoing` (e.g. record it in `AppState`).
pub struct BuiltSession {
    pub messages: Vec<DisplayMessage>,
    pub teams: Vec<TeamSnapshot>,
    pub ongoing: bool,
    pub meta: SessionMeta,
    pub session_totals: SessionTotals,
}

/// A session build with the heavy message bodies already stripped — the shape
/// safe to persist in a long-lived cache (e.g. across scroll-driven range
/// fetches) without holding tool output in memory for the session's lifetime.
/// Built by lightening a [`BuiltSession`] and dropping the heavy original.
pub struct LightBuild {
    pub light_messages: Vec<DisplayMessage>,
    pub teams: Vec<TeamSnapshot>,
    pub ongoing: bool,
    pub meta: SessionMeta,
    pub session_totals: SessionTotals,
}

/// A timestamp window applied to the session's chunks before messages are built.
/// `since` is inclusive, `before` is exclusive; either may be `None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeFilter {
    pub since: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
}

impl TimeFilter {
    fn is_active(&self) -> bool {
        self.since.is_some() || self.before.is_some()
    }

    /// Keep only chunks whose timestamp falls within the window.
    pub(crate) fn retain(&self, chunks: &mut Vec<Chunk>) {
        if !self.is_active() {
            return;
        }
        chunks.retain(|c| {
            self.since.map_or(true, |s| c.timestamp >= s)
                && self.before.map_or(true, |b| c.timestamp < b)
        });
    }
}

/// An index range into a session's message list. `start` is inclusive; `limit`
/// caps the number of messages returned (`None` = to the end).
#[derive(Debug, Clone, Copy, Default)]
pub struct MessageRange {
    pub start: usize,
    pub limit: Option<usize>,
}

impl MessageRange {
    /// The full session — every message from the beginning.
    pub fn full() -> Self {
        Self {
            start: 0,
            limit: None,
        }
    }

    /// Resolve `[start, end)` against a concrete message count, clamping both
    /// ends so out-of-range requests return an empty slice rather than panic.
    fn resolve(&self, count: usize) -> (usize, usize) {
        let start = self.start.min(count);
        let end = match self.limit {
            Some(limit) => start.saturating_add(limit).min(count),
            None => count,
        };
        (start, end)
    }
}

/// Options for a session load: an optional timestamp filter (applied to chunks)
/// and an optional index window (applied to the resulting messages). Both
/// default to "everything".
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    pub time: TimeFilter,
    pub range: MessageRange,
}

impl LoadOptions {
    /// Load the whole session, no filtering or windowing.
    pub fn full() -> Self {
        Self::default()
    }

    /// Load with an index window only.
    pub fn window(range: MessageRange) -> Self {
        Self {
            range,
            ..Self::default()
        }
    }

    /// Load with a timestamp filter only.
    pub fn filtered(time: TimeFilter) -> Self {
        Self {
            time,
            ..Self::default()
        }
    }
}

/// Latest non-empty permission mode across the classified messages (the tip of
/// the conversation wins). Empty when none is set.
fn last_permission_mode(classified: &[ClassifiedMsg]) -> String {
    for msg in classified.iter().rev() {
        if let ClassifiedMsg::User(u) = msg {
            if !u.permission_mode.is_empty() {
                return u.permission_mode.clone();
            }
        }
    }
    String::new()
}

/// Build the display view of a session (messages, teams, ongoing, permission
/// mode), applying an optional timestamp filter to the chunks before messages
/// are built.
///
/// This is the single owner of the parse→chunk→link→messages pipeline — every
/// caller (Tauri command, HTTP API, watcher) goes through here rather than
/// re-assembling the steps, so the build stays consistent everywhere.
pub fn build_view(path: &str, time: TimeFilter) -> Result<ViewBuild, String> {
    let (classified, _offset, _bytes) = read_session_with_debug_hooks(path)?;
    let mut chunks = build_chunks(&classified);

    // Discover and link subagent execution traces, then inject any orphans that
    // have no parent tool_use in the main session yet.
    let (mut all_procs, color_map) = discover_and_link_all(path, &chunks);
    inject_orphan_subagents(&mut chunks, &mut all_procs);

    // Apply the timestamp window after linking so ongoing/teams/messages all
    // agree on the same filtered chunk set.
    time.retain(&mut chunks);

    let ongoing = OngoingChecker::new(&chunks, &all_procs, path).is_ongoing();
    let teams = reconstruct_teams(&chunks, &all_procs);
    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);
    let permission_mode = last_permission_mode(&classified);

    Ok(ViewBuild {
        messages,
        teams,
        ongoing,
        permission_mode,
    })
}

/// Build the display view plus file metadata and token totals. Used by the
/// one-shot load paths (Tauri command, HTTP API); the watcher uses
/// [`build_view`] directly with its own incremental token scanner.
pub fn build_session(path: &str, time: TimeFilter) -> Result<BuiltSession, String> {
    let view = build_view(path, time)?;
    let meta = extract_session_meta(path);

    let scanned = crate::parser::session::scan_session_metadata(path);
    let session_totals = SessionTotals {
        total_tokens: scanned.total_tokens,
        input_tokens: scanned.input_tokens,
        output_tokens: scanned.output_tokens,
        cache_read_tokens: scanned.cache_read_tokens,
        cache_creation_tokens: scanned.cache_creation_tokens,
        cost_usd: scanned.cost_usd,
        model: scanned.model,
    };

    Ok(BuiltSession {
        messages: view.messages,
        teams: view.teams,
        ongoing: view.ongoing,
        meta,
        session_totals,
    })
}

/// Build a session, then immediately lighten and discard the heavy message
/// bodies — the caller ends up holding only [`LightBuild`], never the full
/// tool-output-heavy `messages`. Use this for anything that persists the
/// result in a long-lived cache; use [`build_session`] when the heavy bodies
/// are needed for that one call only (e.g. a single detail lookup) so they can
/// be dropped right after without ever being cached.
pub fn build_light_session(path: &str, time: TimeFilter) -> Result<LightBuild, String> {
    let built = build_session(path, time)?;
    let light_messages = lighten_messages(&built.messages);
    Ok(LightBuild {
        light_messages,
        teams: built.teams,
        ongoing: built.ongoing,
        meta: built.meta,
        session_totals: built.session_totals,
    })
    // `built.messages` (heavy) is dropped here, at the end of this function —
    // never persisted by the caller.
}

/// The role string of every message, in order — the lightweight index a
/// virtualized client needs (placeholders, expand-all) without the bodies.
pub fn message_roles(messages: &[DisplayMessage]) -> Vec<String> {
    messages.iter().map(|m| m.role.clone()).collect()
}

/// Context-window fill: the `context_tokens` of the most recent Claude message
/// that reports a non-zero value, or 0 if none. A virtualized client needs this
/// as a scalar because it can no longer scan the full message list itself.
pub fn latest_context_tokens(messages: &[DisplayMessage]) -> i64 {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "claude" && m.context_tokens > 0)
        .map_or(0, |m| m.context_tokens)
}

/// Build a session and return a [`LoadResult`] honoring `opts`: `messages` is
/// the requested (optionally filtered, optionally windowed) slice, while `count`
/// is always the total message count after filtering so the client can size a
/// virtualized list. `teams`, `meta` and `session_totals` describe the whole
/// (filtered) session regardless of the index window.
pub fn load_session(path: &str, opts: LoadOptions) -> Result<LoadResult, String> {
    let built = build_session(path, opts.time)?;
    Ok(slice_built(&built, &built.messages, path, opts.range))
}

/// Assemble a windowed [`LoadResult`] from an already-built session, windowing
/// over the given `messages` (which may be the full or the lightened list).
/// Split out so a cached build can be sliced repeatedly (different windows)
/// without rebuilding — the cache-hit path in `AppState`.
pub fn slice_built(
    built: &BuiltSession,
    messages: &[DisplayMessage],
    path: &str,
    range: MessageRange,
) -> LoadResult {
    assemble_load_result(
        messages,
        built.teams.clone(),
        built.ongoing,
        built.meta.clone(),
        built.session_totals.clone(),
        path,
        range,
    )
}

/// Same as [`slice_built`], but for a [`LightBuild`] — the cache-hit path for
/// list windows when only the lightened messages are kept in memory.
pub fn slice_light(light: &LightBuild, path: &str, range: MessageRange) -> LoadResult {
    assemble_load_result(
        &light.light_messages,
        light.teams.clone(),
        light.ongoing,
        light.meta.clone(),
        light.session_totals.clone(),
        path,
        range,
    )
}

/// Shared tail for [`slice_built`] and [`slice_light`]: window `messages` and
/// pair it with the (already-owned) session-level fields.
fn assemble_load_result(
    messages: &[DisplayMessage],
    teams: Vec<TeamSnapshot>,
    ongoing: bool,
    meta: SessionMeta,
    session_totals: SessionTotals,
    path: &str,
    range: MessageRange,
) -> LoadResult {
    let count = messages.len();
    // Roles and context fill cover the whole session; messages is only the window.
    let roles = message_roles(messages);
    let context_tokens = latest_context_tokens(messages);
    let (start, end) = range.resolve(count);
    let window = messages[start..end].to_vec();

    LoadResult {
        messages: window,
        teams,
        path: path.to_string(),
        ongoing,
        meta,
        session_totals,
        count,
        start,
        roles,
        context_tokens,
    }
}

/// Produce a memory-light copy of the messages for the list view: the heavy
/// per-item bodies the list never renders (`tool_input`, `tool_result`,
/// `tool_result_json`) and the last-output snapshot are dropped, recursively
/// including nested subagent transcripts. Structure, roles, counts and
/// `subagent_messages` boundaries are preserved so stats/expand-all stay correct.
/// The full bodies remain available via [`BuiltSession::messages`] for the
/// detail view. This is what keeps the renderer flat on tool-output-heavy
/// sessions (a 100-message session can otherwise be >100 MB of tool output).
pub fn lighten_messages(messages: &[DisplayMessage]) -> Vec<DisplayMessage> {
    messages.iter().map(lighten_message).collect()
}

/// Build a light copy of one message field-by-field. Constructing the heavy
/// fields directly as `String::new()` — rather than cloning the message and
/// then overwriting them — means the heavy buffer is never duplicated: a naive
/// `m.clone()` would allocate a full second copy of every `tool_result` /
/// `tool_result_json` string before immediately discarding it, doubling peak
/// memory for tool-output-heavy sessions.
fn lighten_message(m: &crate::convert::DisplayMessage) -> crate::convert::DisplayMessage {
    crate::convert::DisplayMessage {
        role: m.role.clone(),
        model: m.model.clone(),
        content: m.content.clone(),
        timestamp: m.timestamp.clone(),
        thinking_count: m.thinking_count,
        tool_call_count: m.tool_call_count,
        output_count: m.output_count,
        tokens_raw: m.tokens_raw,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        cache_read_tokens: m.cache_read_tokens,
        cache_creation_tokens: m.cache_creation_tokens,
        context_tokens: m.context_tokens,
        duration_ms: m.duration_ms,
        items: m.items.iter().map(lighten_item).collect(),
        last_output: None,
        is_error: m.is_error,
        teammate_spawns: m.teammate_spawns,
        teammate_messages: m.teammate_messages,
        subagent_label: m.subagent_label.clone(),
    }
}

fn lighten_item(it: &crate::convert::FrontendDisplayItem) -> crate::convert::FrontendDisplayItem {
    crate::convert::FrontendDisplayItem {
        id: it.id.clone(),
        item_type: it.item_type.clone(),
        text: it.text.clone(),
        tool_name: it.tool_name.clone(),
        tool_summary: it.tool_summary.clone(),
        tool_category: it.tool_category.clone(),
        tool_input: String::new(),
        tool_result: String::new(),
        tool_error: it.tool_error,
        duration_ms: it.duration_ms,
        token_count: it.token_count,
        subagent_type: it.subagent_type.clone(),
        subagent_desc: it.subagent_desc.clone(),
        team_member_name: it.team_member_name.clone(),
        teammate_id: it.teammate_id.clone(),
        team_color: it.team_color.clone(),
        subagent_ongoing: it.subagent_ongoing,
        agent_id: it.agent_id.clone(),
        subagent_messages: lighten_messages(&it.subagent_messages),
        hook_event: it.hook_event.clone(),
        hook_name: it.hook_name.clone(),
        hook_command: it.hook_command.clone(),
        hook_metadata: it.hook_metadata.clone(),
        tool_result_json: String::new(),
        is_orphan: it.is_orphan,
        subagent_prompt: it.subagent_prompt.clone(),
        is_deferred: it.is_deferred,
        hook_source_agent_name: it.hook_source_agent_name.clone(),
        hook_requesting_agent_uuid: it.hook_requesting_agent_uuid.clone(),
        advisor_model: it.advisor_model.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write a minimal two-message session (one user, one assistant) to a temp
    /// JSONL file and return its path.
    fn write_fixture(dir: &std::path::Path) -> String {
        let path = dir.join("session.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","uuid":"u1","timestamp":"2025-01-01T12:00:00Z","message":{{"role":"user","content":"hello"}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2025-01-01T12:00:01Z","message":{{"role":"assistant","model":"claude-sonnet-4-20250514","content":[{{"type":"text","text":"hi there"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2025-01-01T12:00:02Z","message":{{"role":"user","content":"bye"}}}}"#
        )
        .unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn window_full_matches_build_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_fixture(dir.path());

        let full = build_session(&path, TimeFilter::default()).unwrap();
        let windowed = load_session(&path, LoadOptions::full()).unwrap();

        // Parity: the full window must equal the direct build, one-for-one.
        assert_eq!(windowed.count, full.messages.len());
        assert_eq!(windowed.start, 0);
        assert_eq!(windowed.messages.len(), full.messages.len());
        for (a, b) in windowed.messages.iter().zip(full.messages.iter()) {
            assert_eq!(a.role, b.role);
            assert_eq!(a.content, b.content);
            assert_eq!(a.timestamp, b.timestamp);
        }
    }

    #[test]
    fn window_slice_returns_requested_range_with_total_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_fixture(dir.path());
        let full = build_session(&path, TimeFilter::default()).unwrap();
        assert!(
            full.messages.len() >= 3,
            "fixture should yield >=3 messages"
        );

        let win = load_session(
            &path,
            LoadOptions::window(MessageRange {
                start: 1,
                limit: Some(1),
            }),
        )
        .unwrap();
        assert_eq!(
            win.count,
            full.messages.len(),
            "count is the total, not the slice"
        );
        assert_eq!(win.start, 1);
        assert_eq!(win.messages.len(), 1);
        assert_eq!(win.messages[0].content, full.messages[1].content);
    }

    #[test]
    fn window_out_of_range_is_empty_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_fixture(dir.path());
        let full = build_session(&path, TimeFilter::default()).unwrap();

        let win = load_session(
            &path,
            LoadOptions::window(MessageRange {
                start: 9999,
                limit: Some(10),
            }),
        )
        .unwrap();
        assert_eq!(win.count, full.messages.len());
        assert_eq!(win.start, full.messages.len());
        assert!(win.messages.is_empty());
    }

    #[test]
    fn range_resolve_clamps_both_ends() {
        assert_eq!(
            MessageRange {
                start: 0,
                limit: None
            }
            .resolve(5),
            (0, 5)
        );
        assert_eq!(
            MessageRange {
                start: 2,
                limit: Some(2)
            }
            .resolve(5),
            (2, 4)
        );
        assert_eq!(
            MessageRange {
                start: 3,
                limit: Some(10)
            }
            .resolve(5),
            (3, 5)
        );
        assert_eq!(
            MessageRange {
                start: 8,
                limit: Some(2)
            }
            .resolve(5),
            (5, 5)
        );
    }

    #[test]
    fn time_filter_future_since_excludes_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_fixture(dir.path());
        let since = "2099-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();

        let win = load_session(
            &path,
            LoadOptions::filtered(TimeFilter {
                since: Some(since),
                before: None,
            }),
        )
        .unwrap();
        assert_eq!(win.count, 0);
        assert!(win.messages.is_empty());
    }

    #[test]
    fn time_filter_wide_window_keeps_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_fixture(dir.path());
        let full = build_session(&path, TimeFilter::default()).unwrap();
        let before = "2099-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();

        let win = load_session(
            &path,
            LoadOptions::filtered(TimeFilter {
                since: None,
                before: Some(before),
            }),
        )
        .unwrap();
        assert_eq!(win.count, full.messages.len());
    }

    // --- TimeFilter::retain predicate (synthetic chunks) ---

    use crate::parser::chunk::{Chunk, ChunkType};
    use chrono::TimeZone;

    fn chunk_at(year: i32, month: u32, day: u32) -> Chunk {
        Chunk {
            chunk_type: ChunkType::User,
            timestamp: Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap(),
            ..Chunk::default()
        }
    }

    fn retained(mut chunks: Vec<Chunk>, filter: TimeFilter) -> Vec<Chunk> {
        filter.retain(&mut chunks);
        chunks
    }

    #[test]
    fn retain_future_since_excludes_all() {
        let chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        let since = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
        assert!(retained(
            chunks,
            TimeFilter {
                since: Some(since),
                before: None
            }
        )
        .is_empty());
    }

    #[test]
    fn retain_ancient_before_excludes_all() {
        let chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        let before = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert!(retained(
            chunks,
            TimeFilter {
                since: None,
                before: Some(before)
            }
        )
        .is_empty());
    }

    #[test]
    fn retain_since_keeps_newer() {
        let chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2026, 1, 1),
        ];
        let since = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let kept = retained(
            chunks,
            TimeFilter {
                since: Some(since),
                before: None,
            },
        );
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|c| c.timestamp >= since));
    }

    #[test]
    fn retain_before_keeps_older() {
        let chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2026, 1, 1),
        ];
        let before = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let kept = retained(
            chunks,
            TimeFilter {
                since: None,
                before: Some(before),
            },
        );
        assert_eq!(kept.len(), 1);
        assert!(kept.iter().all(|c| c.timestamp < before));
    }

    #[test]
    fn retain_since_and_before_window() {
        let chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2025, 9, 1),
            chunk_at(2026, 1, 1),
        ];
        let since = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let kept = retained(
            chunks,
            TimeFilter {
                since: Some(since),
                before: Some(before),
            },
        );
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn retain_no_filter_keeps_all() {
        let chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        assert_eq!(retained(chunks, TimeFilter::default()).len(), 2);
    }

    // --- lighten_messages ---

    use crate::convert::{FrontendDisplayItem, FrontendLastOutput};

    fn heavy_item(id: &str, subs: Vec<DisplayMessage>) -> FrontendDisplayItem {
        FrontendDisplayItem {
            id: id.to_string(),
            item_type: "ToolCall".to_string(),
            text: String::new(),
            tool_name: "Read".to_string(),
            tool_summary: "read a file".to_string(),
            tool_category: String::new(),
            tool_input: "big input".to_string(),
            tool_result: "big result".to_string(),
            tool_error: false,
            duration_ms: 0,
            token_count: 0,
            subagent_type: String::new(),
            subagent_desc: String::new(),
            team_member_name: String::new(),
            teammate_id: String::new(),
            team_color: String::new(),
            subagent_ongoing: false,
            agent_id: String::new(),
            subagent_messages: subs,
            hook_event: String::new(),
            hook_name: String::new(),
            hook_command: String::new(),
            hook_metadata: String::new(),
            tool_result_json: "{\"big\":\"json\"}".to_string(),
            is_orphan: false,
            subagent_prompt: String::new(),
            is_deferred: false,
            hook_source_agent_name: String::new(),
            hook_requesting_agent_uuid: String::new(),
            advisor_model: String::new(),
        }
    }

    fn msg_with(role: &str, items: Vec<FrontendDisplayItem>) -> DisplayMessage {
        DisplayMessage {
            role: role.to_string(),
            model: String::new(),
            content: "hi".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            thinking_count: 0,
            tool_call_count: 0,
            output_count: 0,
            tokens_raw: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            context_tokens: 0,
            duration_ms: 0,
            items,
            last_output: Some(FrontendLastOutput {
                output_type: String::new(),
                text: "big output".to_string(),
                tool_name: String::new(),
                tool_result: "big".to_string(),
                is_error: false,
                tool_calls: vec![],
            }),
            is_error: false,
            teammate_spawns: 0,
            teammate_messages: 0,
            subagent_label: String::new(),
        }
    }

    #[test]
    fn lighten_strips_bodies_recursively_and_keeps_structure() {
        let inner = msg_with("claude", vec![heavy_item("s1", vec![])]);
        let outer = msg_with("claude", vec![heavy_item("i1", vec![inner])]);

        let light = lighten_messages(&[outer.clone()]);
        assert_eq!(light.len(), 1);

        let it = &light[0].items[0];
        // Heavy bodies dropped.
        assert!(it.tool_input.is_empty());
        assert!(it.tool_result.is_empty());
        assert!(it.tool_result_json.is_empty());
        // Metadata + structure preserved.
        assert_eq!(it.id, "i1");
        assert_eq!(it.tool_name, "Read");
        assert_eq!(it.tool_summary, "read a file");
        assert_eq!(it.subagent_messages.len(), 1, "subagent boundary preserved");
        // Recursion into nested subagent transcripts.
        let inner_it = &it.subagent_messages[0].items[0];
        assert_eq!(inner_it.id, "s1");
        assert!(inner_it.tool_result.is_empty());
        assert!(inner_it.tool_result_json.is_empty());
        // last_output snapshot dropped.
        assert!(light[0].last_output.is_none());

        // Roles/context are unchanged by lightening (parity for the index).
        assert_eq!(message_roles(&light), message_roles(&[outer]));
    }
}
