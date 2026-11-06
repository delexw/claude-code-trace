use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use super::costs::UsageTracker;
use super::roots::CodexRoot;
use crate::convert::{DisplayMessage, FrontendDisplayItem};

#[derive(Debug, Clone, Serialize)]
pub struct CodexSession {
    pub id: String,
    pub path: String,
    pub title: String,
    pub cwd: String,
    pub dirs: Vec<String>,
    pub forked_from_session_id: Option<String>,
    pub model: String,
    pub git_branch: String,
    pub permission_mode: String,
    pub sandbox_policy: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub duration_ms: Option<i64>,
    pub turn_count: usize,
    pub is_ongoing: bool,
    pub context_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub modified_at: String,
    pub archived: bool,
    pub source: String,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
    pub cost_partial: bool,
    #[serde(skip)]
    has_spawns: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexWindow {
    pub messages: Vec<DisplayMessage>,
    pub count: usize,
    pub start: usize,
    pub roles: Vec<String>,
    pub ongoing: bool,
    pub model: String,
    pub git_branch: String,
    pub permission_mode: String,
    pub sandbox_policy: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub duration_ms: Option<i64>,
    pub turn_count: usize,
    pub context_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
    pub cost_partial: bool,
    pub revision: String,
}

#[derive(Default)]
struct Parsed {
    messages: Vec<DisplayMessage>,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    cost_usd: Option<f64>,
    details: ScanMetadata,
    local_boundaries: usize,
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn text(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        return value.to_owned();
    }
    if let Some(values) = value.as_array() {
        return values
            .iter()
            .filter_map(|v| {
                if v.is_string() {
                    v.as_str().map(str::to_owned)
                } else if v
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.contains("image"))
                {
                    Some("[Image]".into())
                } else {
                    v.get("text").and_then(Value::as_str).map(str::to_owned)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if value.is_null() {
        String::new()
    } else {
        serde_json::to_string_pretty(value).unwrap_or_default()
    }
}

fn revision(metadata: &fs::Metadata) -> String {
    let nanos = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}:{nanos}", metadata.len())
}

/// Context supplied by the client is not a user-authored chat name. Strip only
/// recognized leading blocks so real prompts containing XML remain intact.
fn without_page_context(content: &str) -> &str {
    let mut content = content.trim_start();
    while let Some(rest) = content.strip_prefix("<external_codex_apps_open_page>") {
        let Some((_, rest)) = rest.split_once("</external_codex_apps_open_page>") else {
            return "";
        };
        content = rest.trim_start();
    }
    content
}

fn injected(content: &str) -> bool {
    let content = without_page_context(content);
    [
        "# AGENTS.md",
        "<environment_context>",
        "<permissions instructions>",
        "<turn_aborted>",
    ]
    .iter()
    .any(|prefix| content.starts_with(prefix))
        || content.is_empty()
}

fn title(content: &str) -> Option<String> {
    let content = without_page_context(content);
    (!content.trim().is_empty() && !injected(content)).then(|| {
        content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect()
    })
}

fn timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn policy(value: &Value) -> String {
    value
        .as_str()
        .or_else(|| value["type"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if value.is_object() {
                value.to_string()
            } else {
                String::new()
            }
        })
}

/// Metadata, activity and request context share the usage scan. No tool output
/// or full conversation is retained while building the picker index.
#[derive(Default)]
struct ScanMetadata {
    saw_meta: bool,
    worker: bool,
    parent_thread_id: Option<String>,
    agent_name: String,
    id: String,
    title: String,
    cwd: String,
    dirs: Vec<String>,
    forked_from_session_id: Option<String>,
    model: String,
    git_branch: String,
    permission_mode: String,
    sandbox_policy: String,
    started_at: Option<DateTime<Utc>>,
    last_activity_at: Option<DateTime<Utc>>,
    turn_started_at: Option<DateTime<Utc>>,
    duration_ms: Option<i64>,
    active: bool,
    turn_count: usize,
    awaiting_ai: bool,
    last_visible: Option<(u64, bool)>,
    context_tokens: Option<i64>,
    context_window: Option<i64>,
    has_spawns: bool,
}

impl ScanMetadata {
    fn add_dir(&mut self, cwd: String) {
        if !cwd.is_empty() {
            if self.cwd.is_empty() {
                self.cwd.clone_from(&cwd);
            }
            if !self.dirs.contains(&cwd) {
                self.dirs.push(cwd);
            }
        }
    }

    fn finish_turn(&mut self, time: Option<DateTime<Utc>>) {
        if let (Some(start), Some(end)) = (self.turn_started_at.take(), time) {
            let elapsed = (end - start).num_milliseconds();
            if elapsed >= 0 {
                self.duration_ms = Some(self.duration_ms.unwrap_or(0).saturating_add(elapsed));
            }
        }
        self.active = false;
    }

    fn duration(&self) -> Option<i64> {
        if let (Some(start), Some(end)) = (self.turn_started_at, self.last_activity_at) {
            let elapsed = (end - start).num_milliseconds();
            if elapsed >= 0 {
                return Some(self.duration_ms.unwrap_or(0).saturating_add(elapsed));
            }
        }
        self.duration_ms
    }

    fn visible(&mut self, role: &str, kind: &str, body: &str, fallback: bool) {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (role, kind, body).hash(&mut hash);
        let fingerprint = hash.finish();
        if self
            .last_visible
            .is_some_and(|(previous, event)| previous == fingerprint && event != fallback)
        {
            self.last_visible = None;
            return;
        }
        self.last_visible = Some((fingerprint, fallback));
        if role == "user" {
            self.turn_count += 1;
            self.awaiting_ai = true;
            if self.title.is_empty() {
                self.title = title(body).unwrap_or_default();
            }
        } else if role == "claude" && self.awaiting_ai {
            self.turn_count += 1;
            self.awaiting_ai = false;
        }
    }

    fn observe(&mut self, row: &Value) {
        let payload = &row["payload"];
        let kind = row["type"].as_str().unwrap_or_default();
        let time = row["timestamp"].as_str().and_then(timestamp);
        if let Some(time) = time {
            self.started_at.get_or_insert(time);
            if self.last_activity_at.map_or(true, |last| time > last) {
                self.last_activity_at = Some(time);
            }
        }
        match kind {
            "session_meta" if !self.saw_meta => {
                self.saw_meta = true;
                let source = &payload["source"];
                self.worker = source == "subagent" || source.get("subagent").is_some();
                let spawn = source["subagent"]
                    .get("thread_spawn")
                    .or_else(|| source["subagent"].get("spawn"));
                if let Some(spawn) = spawn {
                    self.parent_thread_id = spawn["parent_thread_id"].as_str().map(str::to_owned);
                    self.agent_name = string(spawn, "agent_nickname");
                }
                self.id = string(payload, "id");
                self.add_dir(string(payload, "cwd"));
                self.git_branch = string(&payload["git"], "branch");
                self.forked_from_session_id = payload["forked_from_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
                // A fork can carry earlier parent history after its own metadata.
                // Its origin and creation date still belong to the first record.
                self.started_at = payload["timestamp"]
                    .as_str()
                    .and_then(timestamp)
                    .or(time)
                    .or(self.started_at);
            }
            "turn_context" => {
                self.add_dir(string(payload, "cwd"));
                if let Some(model) = payload["model"].as_str().filter(|model| !model.is_empty()) {
                    self.model = model.to_owned();
                }
                if !payload["approval_policy"].is_null() {
                    self.permission_mode = policy(&payload["approval_policy"]);
                }
                if !payload["sandbox_policy"].is_null() {
                    self.sandbox_policy = policy(&payload["sandbox_policy"]);
                }
            }
            "token_usage_record" => {
                self.observe_context(&payload["usage"], &payload["model_context_window"]);
            }
            "event_msg" => {
                let subtype = payload["type"].as_str().unwrap_or_default();
                match subtype {
                    "token_count" => self.observe_context(
                        &payload["info"]["last_token_usage"],
                        &payload["info"]["model_context_window"],
                    ),
                    "task_started" | "turn_started" => {
                        // Some versions emit both aliases for one running turn.
                        if !self.active {
                            self.turn_started_at = time;
                        }
                        self.active = true;
                        self.last_visible = None;
                    }
                    "task_complete" | "task_completed" | "turn_complete" | "turn_completed"
                    | "turn_aborted" => {
                        self.finish_turn(time);
                        self.last_visible = None;
                    }
                    "user_message" | "agent_message" | "agent_reasoning" => {
                        let body = string(
                            payload,
                            if subtype == "agent_reasoning" {
                                "text"
                            } else {
                                "message"
                            },
                        );
                        if !body.is_empty() {
                            let role = if subtype == "user_message" {
                                if injected(&body) {
                                    "system"
                                } else {
                                    "user"
                                }
                            } else {
                                "claude"
                            };
                            self.visible(
                                role,
                                if subtype == "agent_reasoning" {
                                    "Thinking"
                                } else {
                                    "Output"
                                },
                                &body,
                                true,
                            );
                        }
                    }
                    "item_completed" if payload["item"]["type"] == "Reasoning" => {
                        let thought = &payload["item"];
                        let summary = text(&thought["summary_text"]);
                        let body = if summary.is_empty() {
                            text(&thought["raw_content"])
                        } else {
                            summary
                        };
                        if !body.is_empty() {
                            self.visible("claude", "Thinking", &body, true);
                        }
                    }
                    _ => {}
                }
            }
            "response_item" => match payload["type"].as_str().unwrap_or_default() {
                "message" => {
                    let body = text(&payload["content"]);
                    if !body.is_empty() {
                        let role = match payload["role"].as_str().unwrap_or_default() {
                            "user" if !injected(&body) => "user",
                            "assistant" => "claude",
                            _ => "system",
                        };
                        self.visible(role, "Output", &body, false);
                    }
                    if payload["phase"] == "final_answer" || payload["channel"] == "final" {
                        self.finish_turn(time);
                    }
                }
                "reasoning" => {
                    let summary = text(&payload["summary"]);
                    let body = if summary.is_empty() {
                        text(&payload["content"])
                    } else {
                        summary
                    };
                    if !body.is_empty() {
                        self.visible("claude", "Thinking", &body, false);
                    }
                }
                "function_call" | "custom_tool_call" | "web_search_call" | "local_shell_call" => {
                    self.has_spawns |=
                        string(payload, "name").rsplit('.').next() == Some("spawn_agent");
                    self.visible("claude", "ToolCall", "", false);
                }
                "agent_message" | "compaction" => self.last_visible = None,
                _ => {}
            },
            "compacted" => self.last_visible = None,
            _ => {}
        }
    }

    fn observe_context(&mut self, usage: &Value, window: &Value) {
        if let Some(window) = window.as_i64().filter(|window| *window > 0) {
            self.context_window = Some(window);
        }
        // Codex's tokens_in_context_window uses the latest request's total,
        // never the thread's cumulative usage. Cached reads are already included.
        if let Some(input) = usage["input_tokens"].as_i64().filter(|input| *input >= 0) {
            self.context_tokens = usage["total_tokens"]
                .as_i64()
                .filter(|total| *total >= 0)
                .or_else(|| {
                    usage["output_tokens"]
                        .as_i64()
                        .filter(|output| *output >= 0)
                        .map(|output| input.saturating_add(output))
                });
        }
    }
}

fn recent(stat: &fs::Metadata) -> bool {
    stat.modified()
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|elapsed| elapsed.as_secs() < 120)
}

fn metadata(
    path: &Path,
    source: &str,
    archived: bool,
    stat: &fs::Metadata,
) -> Option<CodexSession> {
    let stem = path.file_stem()?.to_string_lossy();
    // The rollout filename remains authoritative when resumed/forked histories
    // contain metadata from more than one session.
    let filename_id = stem
        .get(stem.len().saturating_sub(36)..)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .map(str::to_owned);
    let mut details = ScanMetadata::default();
    let mut usage = UsageTracker::default();
    for line in BufReader::new(fs::File::open(path).ok()?).split(b'\n') {
        let line = line.ok()?;
        let Ok(line) = std::str::from_utf8(&line) else {
            continue;
        };
        let Ok(row) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) else {
            continue;
        };
        details.observe(&row);
        if details.worker {
            return None;
        }
        usage.observe(&row);
    }
    if !details.saw_meta {
        return None;
    }
    let id = filename_id.unwrap_or_else(|| {
        if details.id.is_empty() {
            stem.into_owned()
        } else {
            details.id.clone()
        }
    });
    let usage = usage.summary();
    Some(CodexSession {
        id: id.clone(),
        path: path.to_string_lossy().into_owned(),
        title: if details.title.is_empty() {
            id
        } else {
            details.title.clone()
        },
        cwd: details.cwd.clone(),
        dirs: details.dirs.clone(),
        forked_from_session_id: details.forked_from_session_id.clone(),
        model: details.model.clone(),
        git_branch: details.git_branch.clone(),
        permission_mode: details.permission_mode.clone(),
        sandbox_policy: details.sandbox_policy.clone(),
        started_at: details
            .started_at
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
        last_activity_at: details
            .last_activity_at
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
        duration_ms: details.duration(),
        turn_count: details.turn_count,
        is_ongoing: details.active && recent(stat),
        context_tokens: details.context_tokens,
        context_window: details.context_window,
        modified_at: stat
            .modified()
            .map(DateTime::<Utc>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
        source: source.to_owned(),
        archived,
        total_tokens: usage.total_tokens,
        cost_usd: usage.cost_usd,
        cost_partial: false,
        has_spawns: details.has_spawns,
    })
}

fn rollout_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Do not follow symlink directories or scan unrelated Codex state.
        if kind.is_dir() {
            rollout_files(&entry.path(), files);
        } else if kind.is_file()
            && entry.path().extension().is_some_and(|e| e == "jsonl")
            && entry.file_name().to_string_lossy().starts_with("rollout-")
        {
            files.push(entry.path());
        }
    }
}

type SummaryCache = HashMap<PathBuf, (String, CodexSession)>;
pub fn discover_sessions(roots: &[CodexRoot]) -> Vec<CodexSession> {
    static CACHE: OnceLock<Mutex<SummaryCache>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut sessions = HashMap::<String, CodexSession>::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let names = session_names(Path::new(&root.path));
        for (subdir, archived) in [("sessions", false), ("archived_sessions", true)] {
            let mut files = Vec::new();
            rollout_files(&Path::new(&root.path).join(subdir), &mut files);
            for path in files {
                seen.insert(path.clone());
                let Ok(stat) = fs::metadata(&path) else {
                    continue;
                };
                let stamp = revision(&stat);
                let cached = cache
                    .get(&path)
                    .filter(|(key, _)| *key == stamp)
                    .map(|(_, value)| value.clone());
                let Some(mut session) =
                    cached.or_else(|| metadata(&path, &root.source, archived, &stat))
                else {
                    continue;
                };
                session.source.clone_from(&root.source);
                session.archived = archived;
                // The file can become stale without its revision changing.
                session.is_ongoing &= recent(&stat);
                cache.insert(path, (stamp, session.clone()));
                if let Some(name) = names.get(&session.id) {
                    session.title.clone_from(name);
                }
                // Separate installs can contain independent UUID namespaces; only
                // de-duplicate a copied/archived rollout within the same source.
                let key = format!("{}:{}", root.source, session.id);
                let replace = sessions.get(&key).map_or(true, |old| {
                    (old.archived && !archived)
                        || (old.archived == archived && old.modified_at < session.modified_at)
                });
                if replace {
                    sessions.insert(key, session);
                }
            }
        }
    }
    cache.retain(|path, _| seen.contains(path));
    let mut result = sessions.into_values().collect::<Vec<_>>();
    for session in &mut result {
        if session.has_spawns {
            if let Ok(window) = load_window(Path::new(&session.path), 0, Some(0)) {
                session.total_tokens = window.total_tokens;
                session.cost_usd = window.cost_usd;
                session.cost_partial = window.cost_partial;
                session.is_ongoing = window.ongoing;
            } else {
                session.cost_partial = true;
            }
        }
    }
    result.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

fn session_names(home: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Ok(file) = fs::File::open(home.join("session_index.jsonl")) else {
        return names;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = string(&row, "id");
        let name = string(&row, "thread_name");
        let name = without_page_context(&name);
        if !id.is_empty() && title(name).is_some() {
            names.insert(id, name.trim().to_owned());
        }
    }
    names
}

fn message(role: &str, content: String, timestamp: &str, model: &str) -> DisplayMessage {
    DisplayMessage {
        role: role.into(),
        content,
        timestamp: timestamp.into(),
        model: model.into(),
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
        items: Vec::new(),
        last_output: None,
        is_error: false,
        teammate_spawns: 0,
        teammate_messages: 0,
        subagent_label: String::new(),
    }
}

fn item(kind: &str, id: String, body: String) -> FrontendDisplayItem {
    FrontendDisplayItem {
        id,
        item_type: kind.into(),
        text: body,
        tool_name: String::new(),
        tool_summary: String::new(),
        tool_category: "Tool".into(),
        tool_input: String::new(),
        tool_result: String::new(),
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
        subagent_messages: Vec::new(),
        hook_event: String::new(),
        hook_name: String::new(),
        hook_command: String::new(),
        hook_metadata: String::new(),
        tool_result_json: String::new(),
        is_orphan: false,
        subagent_prompt: String::new(),
        is_deferred: false,
        hook_source_agent_name: String::new(),
        hook_requesting_agent_uuid: String::new(),
        advisor_model: String::new(),
        tool_denial_kind: String::new(),
    }
}

fn prose(role: &str, body: String, ts: &str, model: &str, id: String) -> DisplayMessage {
    let mut result = message(role, body.clone(), ts, model);
    if role == "claude" {
        result.output_count = 1;
        result.items.push(item("Output", id, body));
    }
    result
}

fn structured_error(value: &Value) -> bool {
    if let Some(values) = value.as_array() {
        return values.iter().any(structured_error);
    }
    value["isError"].as_bool().unwrap_or(false)
        || value["is_error"].as_bool().unwrap_or(false)
        || value["exit_code"].as_i64().is_some_and(|code| code != 0)
        || value["metadata"]["exit_code"]
            .as_i64()
            .is_some_and(|code| code != 0)
        || ["result", "value"]
            .iter()
            .any(|key| value.get(key).is_some_and(structured_error))
}

fn output_error(output: &Value) -> bool {
    if let Some(parts) = output.as_array() {
        return parts.iter().any(output_error);
    }
    if structured_error(output) {
        return true;
    }
    let raw = output
        .as_str()
        .or_else(|| output.get("text").and_then(Value::as_str));
    raw.and_then(|s| serde_json::from_str::<Value>(s).ok())
        .is_some_and(|json| structured_error(&json))
}

fn request_usage(row: &Value) -> Option<&Value> {
    let payload = &row["payload"];
    match row["type"].as_str()? {
        "token_usage_record" => payload.get("usage"),
        "event_msg" if payload["type"] == "token_count" => payload["info"].get("last_token_usage"),
        _ => None,
    }
}

fn apply_request_usage(message: &mut DisplayMessage, usage: &Value) {
    let Some(input) = usage["input_tokens"].as_i64().filter(|value| *value >= 0) else {
        return;
    };
    let Some(output) = usage["output_tokens"].as_i64().filter(|value| *value >= 0) else {
        return;
    };
    let cached = usage["cached_input_tokens"].as_i64().unwrap_or(0);
    let writes = usage["cache_write_input_tokens"].as_i64().unwrap_or(0);
    if cached < 0 || writes < 0 || cached.saturating_add(writes) > input {
        return;
    }
    message.input_tokens = input;
    message.output_tokens = output;
    message.cache_read_tokens = cached;
    message.cache_creation_tokens = writes;
    message.tokens_raw = usage["total_tokens"]
        .as_i64()
        .filter(|value| *value >= 0)
        .unwrap_or_else(|| input.saturating_add(output));
    message.context_tokens = message.tokens_raw;
}

#[cfg(test)]
thread_local! {
    static PARSE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn parse(path: &Path, local_prompt: Option<&str>) -> Result<Parsed, String> {
    #[cfg(test)]
    PARSE_CALLS.with(|count| count.set(count.get() + 1));
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut parsed = Parsed::default();
    let mut usage_tracker = UsageTracker::default();
    // The bool marks an event fallback. Only neighboring visible twins are
    // paired: matching text elsewhere can belong to a different user turn.
    let mut candidates: Vec<(DisplayMessage, bool)> = Vec::new();
    let mut dedup_boundaries = std::collections::HashSet::new();
    let mut calls: HashMap<String, usize> = HashMap::new();
    let mut request_messages: HashMap<String, usize> = HashMap::new();
    let mut model = String::new();
    let mut usage_context = None;
    for (line_index, line) in BufReader::new(file).split(b'\n').enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(line) = std::str::from_utf8(&line) else {
            continue;
        };
        let Ok(row) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) else {
            continue;
        };
        let payload = &row["payload"];
        let inherited = row["metadata"]["inherited_user_message"]
            .as_bool()
            .unwrap_or(false);
        let kind = row["type"].as_str().unwrap_or_default();
        if kind == "turn_context" {
            usage_context = Some(row.clone());
        }
        if let Some(prompt) = local_prompt {
            let boundary = kind == "response_item"
                && payload["type"] == "message"
                && payload["role"] == "user"
                && !inherited
                && text(&payload["content"]).trim() == prompt.trim();
            if boundary {
                parsed.local_boundaries += 1;
                if parsed.local_boundaries == 1 {
                    // Copied parent events can contain old cumulative counters.
                    // Start fresh only at the explicitly assigned child task.
                    usage_tracker = UsageTracker::default();
                    if let Some(context) = &usage_context {
                        usage_tracker.observe(context);
                    }
                    parsed.details.duration_ms = None;
                    parsed.details.turn_started_at = row["timestamp"].as_str().and_then(timestamp);
                    parsed.details.active = true;
                    parsed.details.turn_count = 0;
                    parsed.details.awaiting_ai = false;
                    parsed.details.last_visible = None;
                }
            }
            if parsed.local_boundaries == 0 || inherited {
                parsed.details.observe(&row);
                if kind == "turn_context" {
                    model = string(payload, "model");
                }
                continue;
            }
        }
        usage_tracker.observe(&row);
        parsed.details.observe(&row);
        if let Some(usage) = request_usage(&row) {
            let response_id = payload["response_id"].as_str().filter(|id| !id.is_empty());
            let last_assistant = candidates
                .len()
                .checked_sub(1)
                .filter(|&index| candidates[index].0.role == "claude");
            let index = response_id
                .and_then(|id| request_messages.get(id).copied())
                .or(last_assistant);
            if let Some(index) = index {
                apply_request_usage(&mut candidates[index].0, usage);
                if let Some(id) = response_id {
                    request_messages.insert(id.to_owned(), index);
                }
            }
        }
        let ts = string(&row, "timestamp");
        if kind == "compacted" {
            let body = string(payload, "message");
            candidates.push((
                message(
                    "compact",
                    if body.is_empty() {
                        "Context compacted".into()
                    } else {
                        body
                    },
                    &ts,
                    &model,
                ),
                false,
            ));
            continue;
        }
        if kind == "token_usage_record" {
            continue;
        }
        if kind == "turn_context" {
            let next = string(payload, "model");
            if !next.is_empty() {
                model = next;
            }
            continue;
        }
        let subtype = payload["type"].as_str().unwrap_or_default();
        if kind == "event_msg" {
            match subtype {
                "task_started" | "turn_started" => {
                    dedup_boundaries.insert(candidates.len());
                }
                "task_complete" | "task_completed" | "turn_complete" | "turn_completed"
                | "turn_aborted" => {
                    dedup_boundaries.insert(candidates.len());
                }
                "user_message" | "agent_message" | "agent_reasoning" => {
                    let body = string(
                        payload,
                        if subtype == "agent_reasoning" {
                            "text"
                        } else {
                            "message"
                        },
                    );
                    if !body.is_empty() {
                        let role = if subtype == "user_message" {
                            if injected(&body) {
                                "system"
                            } else {
                                "user"
                            }
                        } else {
                            "claude"
                        };
                        let mut msg = prose(
                            role,
                            body.clone(),
                            &ts,
                            &model,
                            format!("codex-{line_index}"),
                        );
                        if subtype == "agent_reasoning" {
                            msg.items = vec![item("Thinking", format!("codex-{line_index}"), body)];
                            msg.thinking_count = 1;
                            msg.output_count = 0;
                        }
                        candidates.push((msg, true));
                    }
                }
                "item_completed" if payload["item"]["type"] == "Reasoning" => {
                    let thought = &payload["item"];
                    let summary = text(&thought["summary_text"]);
                    let raw = text(&thought["raw_content"]);
                    let body = if !summary.is_empty() { summary } else { raw };
                    if !body.is_empty() {
                        let mut msg = message("claude", body.clone(), &ts, &model);
                        msg.thinking_count = 1;
                        msg.items
                            .push(item("Thinking", format!("codex-{line_index}"), body));
                        candidates.push((msg, true));
                    }
                }
                _ => {}
            }
            continue;
        }
        if kind != "response_item" {
            continue;
        }
        match subtype {
            "agent_message" => {
                let body = text(&payload["content"]);
                if !body.is_empty() {
                    let mut msg = message("system", body, &ts, &model);
                    let author = string(payload, "author");
                    let recipient = string(payload, "recipient");
                    msg.subagent_label = if recipient.is_empty() {
                        author
                    } else {
                        format!("{author} → {recipient}")
                    };
                    candidates.push((msg, false));
                }
            }
            "message" => {
                let body = text(&payload["content"]);
                if body.is_empty() {
                    continue;
                }
                let role = match payload["role"].as_str().unwrap_or_default() {
                    "assistant" => "claude",
                    "user" if !injected(&body) => "user",
                    _ => "system",
                };
                candidates.push((
                    prose(role, body, &ts, &model, format!("codex-{line_index}")),
                    false,
                ));
            }
            "reasoning" => {
                let mut body = text(&payload["summary"]);
                if body.is_empty() {
                    body = text(&payload["content"]);
                }
                // encrypted_content is intentionally never rendered as text.
                if body.is_empty() {
                    continue;
                }
                let mut msg = message("claude", body.clone(), &ts, &model);
                msg.thinking_count = 1;
                msg.items
                    .push(item("Thinking", format!("codex-{line_index}"), body));
                candidates.push((msg, false));
            }
            "function_call" | "custom_tool_call" | "web_search_call" | "local_shell_call" => {
                let mut name = if subtype == "web_search_call" {
                    "web_search".into()
                } else if subtype == "local_shell_call" {
                    "local_shell".into()
                } else {
                    string(payload, "name")
                };
                let namespace = string(payload, "namespace");
                if !namespace.is_empty() {
                    name = format!("{namespace}.{name}");
                }
                let input = text(
                    &payload[if subtype == "function_call" {
                        "arguments"
                    } else if subtype == "custom_tool_call" {
                        "input"
                    } else {
                        "action"
                    }],
                );
                let mut id = string(payload, "call_id");
                if id.is_empty() {
                    id = string(payload, "id");
                }
                if id.is_empty() {
                    id = format!("codex-{line_index}");
                }
                let mut call = item("ToolCall", id.clone(), name.clone());
                call.tool_name = name.clone();
                call.tool_category = match name.rsplit('.').next().unwrap_or_default() {
                    "exec_command" | "shell" | "shell_command" | "local_shell" => "Bash",
                    "apply_patch" => "Edit",
                    "read_file" => "Read",
                    "list_dir" | "glob" => "Glob",
                    "grep" | "grep_files" => "Grep",
                    "web_search" => "Web",
                    "spawn_agent" | "send_input" | "wait_agent" | "close_agent" => "Task",
                    _ if name.starts_with("mcp__") => "Mcp",
                    _ => "Tool",
                }
                .into();
                call.tool_summary = input
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect();
                call.tool_input = input;
                call.tool_error = payload["status"] == "failed";
                let mut msg = message("claude", name, &ts, &model);
                msg.tool_call_count = 1;
                msg.items.push(call);
                calls.insert(id, candidates.len());
                candidates.push((msg, false));
            }
            "function_call_output" | "custom_tool_call_output" | "local_shell_call_output" => {
                let output = text(&payload["output"]);
                let id = string(payload, "call_id");
                if let Some(&index) = calls.get(&id) {
                    let msg = &mut candidates[index].0;
                    let call = &mut msg.items[0];
                    call.tool_result = output;
                    call.tool_error |= payload["is_error"].as_bool().unwrap_or(false)
                        || output_error(&payload["output"]);
                    if let Ok(json) = serde_json::from_str::<Value>(&call.tool_result) {
                        call.tool_error |= structured_error(&json);
                        if json.is_object() || json.is_array() {
                            call.tool_result_json =
                                serde_json::to_string_pretty(&json).unwrap_or_default();
                        }
                    }
                    if let (Ok(start), Ok(end)) = (
                        DateTime::parse_from_rfc3339(&msg.timestamp),
                        DateTime::parse_from_rfc3339(&ts),
                    ) {
                        call.duration_ms = (end - start).num_milliseconds().max(0);
                        msg.duration_ms = call.duration_ms;
                    }
                } else if !output.is_empty() {
                    candidates.push((message("system", output, &ts, &model), false));
                }
            }
            "compaction" => {
                let body = string(payload, "summary");
                if !body.is_empty() {
                    candidates.push((message("compact", body, &ts, &model), false));
                }
            }
            _ => {}
        }
    }
    let mut keep = vec![true; candidates.len()];
    let mut previous: Option<usize> = None;
    for index in 0..candidates.len() {
        let (msg, fallback) = &candidates[index];
        if dedup_boundaries.contains(&index) {
            previous = None;
        }
        if let Some(previous_index) = previous {
            let (previous_msg, previous_fallback) = &candidates[previous_index];
            if fallback != previous_fallback
                && msg.role == previous_msg.role
                && msg.content == previous_msg.content
                && msg.items.first().map(|item| item.item_type.as_str())
                    == previous_msg
                        .items
                        .first()
                        .map(|item| item.item_type.as_str())
            {
                let removed = if *fallback { index } else { previous_index };
                let retained = if *fallback { previous_index } else { index };
                // Usage can arrive between event/response twins. Keep it with
                // whichever representation survives the display deduplication.
                if candidates[removed].0.tokens_raw > 0 && candidates[retained].0.tokens_raw == 0 {
                    let usage = &candidates[removed].0;
                    let values = (
                        usage.tokens_raw,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_read_tokens,
                        usage.cache_creation_tokens,
                        usage.context_tokens,
                    );
                    let target = &mut candidates[retained].0;
                    (
                        target.tokens_raw,
                        target.input_tokens,
                        target.output_tokens,
                        target.cache_read_tokens,
                        target.cache_creation_tokens,
                        target.context_tokens,
                    ) = values;
                }
                keep[removed] = false;
                previous = None;
                continue;
            }
        }
        previous = Some(index);
    }
    for ((msg, _), keep) in candidates.into_iter().zip(keep) {
        if keep {
            parsed.messages.push(msg);
        }
    }
    let usage = usage_tracker.summary();
    parsed.input_tokens = usage.input_tokens;
    parsed.output_tokens = usage.output_tokens;
    parsed.cache_read_tokens = usage.cache_read_tokens;
    parsed.total_tokens = usage.total_tokens;
    parsed.cost_usd = usage.cost_usd;
    Ok(parsed)
}

type ParseCache = HashMap<(PathBuf, Option<String>), (String, Arc<Parsed>, u64)>;
fn cached_parse(
    path: &Path,
    stat: &fs::Metadata,
    prompt: Option<&str>,
) -> Result<Arc<Parsed>, String> {
    static CACHE: OnceLock<Mutex<ParseCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (path.to_path_buf(), prompt.map(str::to_owned));
    let stamp = revision(stat);
    if let Some((_, parsed, _)) = cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&key)
        .filter(|(old_stamp, _, _)| old_stamp == &stamp)
    {
        return Ok(Arc::clone(parsed));
    }
    let parsed = Arc::new(parse(path, prompt)?);
    if stat.len() <= 16 * 1024 * 1024 {
        let mut guard = cache.lock().unwrap_or_else(|error| error.into_inner());
        // Keep one modest conversation and its workers cheap to poll without
        // retaining unbounded tool results across previously opened chats.
        if guard.len() >= 8
            || guard
                .values()
                .map(|(_, _, size)| size)
                .sum::<u64>()
                .saturating_add(stat.len())
                > 16 * 1024 * 1024
        {
            guard.clear();
        }
        guard.insert(key, (stamp, Arc::clone(&parsed), stat.len()));
    }
    Ok(parsed)
}

struct ChildLinks {
    files: Vec<PathBuf>,
    visited: std::collections::HashSet<String>,
    accounted: std::collections::HashSet<String>,
    revisions: Vec<String>,
    dependencies: Vec<(PathBuf, String, bool)>,
    unresolved: bool,
    child_active: bool,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    cost_usd: Option<f64>,
    cost_partial: bool,
}

impl ChildLinks {
    fn new(path: &Path, parsed: &Parsed) -> Self {
        let mut files = Vec::new();
        if parsed.details.has_spawns {
            if let Some(root) = path
                .ancestors()
                .find(|ancestor| {
                    ancestor
                        .file_name()
                        .is_some_and(|name| name == "sessions" || name == "archived_sessions")
                })
                .and_then(Path::parent)
            {
                rollout_files(&root.join("sessions"), &mut files);
                rollout_files(&root.join("archived_sessions"), &mut files);
                files.sort();
            }
        }
        Self {
            files,
            visited: [parsed.details.id.clone()].into_iter().collect(),
            accounted: [parsed.details.id.clone()].into_iter().collect(),
            revisions: Vec::new(),
            dependencies: Vec::new(),
            unresolved: false,
            child_active: false,
            input_tokens: parsed.input_tokens,
            output_tokens: parsed.output_tokens,
            cache_read_tokens: parsed.cache_read_tokens,
            total_tokens: parsed.total_tokens,
            cost_usd: parsed.cost_usd,
            cost_partial: false,
        }
    }

    fn child(&mut self, id: &str, parent: &str, prompt: &str) -> Option<(Arc<Parsed>, bool)> {
        // IDs are compared with file metadata, never interpolated into paths.
        for path in self.files.iter().filter(|path| {
            path.file_stem()
                .is_some_and(|stem| stem.to_string_lossy().ends_with(id))
        }) {
            let Ok(stat) = fs::metadata(path) else {
                continue;
            };
            self.revisions.push(format!(
                "{}:{}:{}",
                path.to_string_lossy(),
                revision(&stat),
                recent(&stat)
            ));
            self.dependencies
                .push((path.clone(), revision(&stat), recent(&stat)));
            let Ok(child) = cached_parse(path, &stat, Some(prompt)) else {
                continue;
            };
            if child.details.worker
                && child.details.id == id
                && child.details.parent_thread_id.as_deref() == Some(parent)
                && child.local_boundaries == 1
            {
                return Some((child, recent(&stat)));
            }
        }
        self.unresolved = true;
        None
    }

    fn walk(
        &mut self,
        parsed: &Parsed,
        depth: usize,
        range: Option<(usize, usize)>,
        materialize: bool,
        fresh: bool,
    ) -> (Vec<DisplayMessage>, bool) {
        let mut messages = Vec::new();
        let mut active = parsed.details.active && fresh;
        for (index, message) in parsed.messages.iter().enumerate() {
            let selected =
                materialize && range.map_or(true, |(start, end)| index >= start && index < end);
            let mut display = selected.then(|| message.clone());
            for (item_index, item) in message.items.iter().enumerate() {
                if item.tool_name.rsplit('.').next() != Some("spawn_agent") || item.tool_error {
                    continue;
                }
                let input = serde_json::from_str::<Value>(&item.tool_input).unwrap_or(Value::Null);
                let result =
                    serde_json::from_str::<Value>(&item.tool_result).unwrap_or(Value::Null);
                let prompt = input["message"]
                    .as_str()
                    .filter(|prompt| !prompt.trim().is_empty());
                let id = result["agent_id"].as_str().filter(|id| !id.is_empty());
                let Some((prompt, id)) = prompt.zip(id) else {
                    self.cost_partial = true;
                    continue;
                };
                if depth >= 4 || self.visited.contains(id) {
                    self.cost_partial = true;
                    continue;
                }
                let Some((child, fresh)) = self.child(id, &parsed.details.id, prompt) else {
                    self.cost_partial = true;
                    continue;
                };
                if self.accounted.insert(id.to_owned()) {
                    self.input_tokens = self.input_tokens.saturating_add(child.input_tokens);
                    self.output_tokens = self.output_tokens.saturating_add(child.output_tokens);
                    self.cache_read_tokens = self
                        .cache_read_tokens
                        .saturating_add(child.cache_read_tokens);
                    self.total_tokens = self.total_tokens.saturating_add(child.total_tokens);
                    self.cost_usd = match (self.cost_usd, child.cost_usd) {
                        (Some(parent), Some(child)) => Some(parent + child),
                        (known, unknown) => {
                            self.cost_partial = true;
                            known.or(unknown)
                        }
                    };
                }
                self.visited.insert(id.to_owned());
                let (child_messages, child_active) =
                    self.walk(&child, depth + 1, None, selected, fresh);
                self.visited.remove(id);
                active |= child_active;
                self.child_active |= child_active;
                if let Some(display) = &mut display {
                    let target = &mut display.items[item_index];
                    target.item_type = "Subagent".into();
                    target.agent_id = id.to_owned();
                    target.subagent_type =
                        input["agent_type"].as_str().unwrap_or("Codex").to_owned();
                    target.subagent_desc = if child.details.agent_name.is_empty() {
                        prompt
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .take(120)
                            .collect()
                    } else {
                        child.details.agent_name.clone()
                    };
                    target.subagent_prompt = prompt.to_owned();
                    target.subagent_messages = child_messages;
                    target.subagent_ongoing = child_active;
                    target.token_count = usize::try_from(child.total_tokens).unwrap_or(0);
                    target.duration_ms = child.details.duration().unwrap_or(0);
                }
            }
            if let Some(display) = display {
                messages.push(display);
            }
        }
        (messages, active)
    }
}

struct CachedIndex {
    path: PathBuf,
    parent_revision: String,
    window: CodexWindow,
    dependencies: Vec<(PathBuf, String, bool)>,
    unresolved: bool,
    main_active: bool,
    child_active: bool,
}
type IndexCache = Option<CachedIndex>;
pub fn load_window(path: &Path, start: usize, limit: Option<usize>) -> Result<CodexWindow, String> {
    static INDEX: OnceLock<Mutex<IndexCache>> = OnceLock::new();
    load_window_cached(path, start, limit, INDEX.get_or_init(|| Mutex::new(None)))
}

fn load_window_cached(
    path: &Path,
    start: usize,
    limit: Option<usize>,
    index_cache: &Mutex<IndexCache>,
) -> Result<CodexWindow, String> {
    let stat = fs::metadata(path).map_err(|e| e.to_string())?;
    if !stat.is_file() || path.extension().map_or(true, |e| e != "jsonl") {
        return Err("path must be a Codex JSONL rollout file".into());
    }
    let parent_revision = revision(&stat);
    if limit == Some(0) {
        let guard = index_cache
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cached) = guard.as_ref().filter(|cached| {
            cached.path == path && cached.parent_revision == parent_revision && !cached.unresolved
        }) {
            let unchanged = cached.dependencies.iter().all(|(path, stamp, fresh)| {
                fs::metadata(path)
                    .is_ok_and(|stat| revision(&stat) == *stamp && recent(&stat) == *fresh)
            });
            if unchanged {
                let mut index = cached.window.clone();
                index.start = start.min(index.count);
                index.ongoing = (cached.main_active && recent(&stat)) || cached.child_active;
                return Ok(index);
            }
        }
    }
    let parsed = cached_parse(path, &stat, None)?;
    let count = parsed.messages.len();
    let start = start.min(count);
    let end = start.saturating_add(limit.unwrap_or(usize::MAX)).min(count);
    let mut links = ChildLinks::new(path, &parsed);
    let (messages, ongoing) = links.walk(
        &parsed,
        0,
        Some((start, end)),
        limit != Some(0),
        recent(&stat),
    );
    let stamp = if links.revisions.is_empty() {
        revision(&stat)
    } else {
        format!("{}|{}", revision(&stat), links.revisions.join("|"))
    };
    let mut index = CodexWindow {
        messages: Vec::new(),
        count,
        start,
        roles: parsed.messages.iter().map(|m| m.role.clone()).collect(),
        ongoing,
        model: parsed.details.model.clone(),
        git_branch: parsed.details.git_branch.clone(),
        permission_mode: parsed.details.permission_mode.clone(),
        sandbox_policy: parsed.details.sandbox_policy.clone(),
        started_at: parsed
            .details
            .started_at
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
        last_activity_at: parsed
            .details
            .last_activity_at
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
        duration_ms: parsed.details.duration(),
        turn_count: parsed.details.turn_count,
        context_tokens: parsed.details.context_tokens,
        context_window: parsed.details.context_window,
        input_tokens: links.input_tokens,
        output_tokens: links.output_tokens,
        cache_read_tokens: links.cache_read_tokens,
        total_tokens: links.total_tokens,
        cost_usd: links.cost_usd,
        cost_partial: links.cost_partial,
        revision: stamp,
    };
    *index_cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(CachedIndex {
        path: path.to_path_buf(),
        parent_revision,
        window: index.clone(),
        dependencies: links.dependencies,
        unresolved: links.unresolved,
        main_active: parsed.details.active,
        child_active: links.child_active,
    });
    index.messages = messages;
    Ok(index)
}

#[cfg(test)]
mod tests;
