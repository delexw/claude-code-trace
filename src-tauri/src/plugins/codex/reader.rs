use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use super::roots::CodexRoot;
use crate::convert::{DisplayMessage, FrontendDisplayItem};

#[derive(Debug, Clone, Serialize)]
pub struct CodexSession {
    pub id: String,
    pub path: String,
    pub title: String,
    pub cwd: String,
    pub model: String,
    pub modified_at: String,
    pub archived: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexWindow {
    pub messages: Vec<DisplayMessage>,
    pub count: usize,
    pub start: usize,
    pub roles: Vec<String>,
    pub ongoing: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_tokens: i64,
    pub revision: String,
}

#[derive(Default)]
struct Parsed {
    messages: Vec<DisplayMessage>,
    active: bool,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
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

fn injected(content: &str) -> bool {
    let content = content.trim_start();
    [
        "# AGENTS.md",
        "<environment_context>",
        "<permissions instructions>",
        "<turn_aborted>",
    ]
    .iter()
    .any(|prefix| content.starts_with(prefix))
}

fn title(content: &str) -> Option<String> {
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

fn metadata(
    path: &Path,
    source: &str,
    archived: bool,
    stat: &fs::Metadata,
) -> Option<CodexSession> {
    let mut session = CodexSession {
        id: path.file_stem()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        title: String::new(),
        cwd: String::new(),
        model: String::new(),
        modified_at: stat
            .modified()
            .map(DateTime::<Utc>::from)
            .map(|d| d.to_rfc3339())
            .unwrap_or_default(),
        source: source.to_owned(),
        archived,
    };
    // The rollout filename remains authoritative when resumed/forked histories
    // contain metadata from more than one session.
    let filename_id = session
        .id
        .get(session.id.len().saturating_sub(36)..)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .map(str::to_owned);
    let mut saw_meta = false;
    for line in BufReader::new(fs::File::open(path).ok()?).lines().take(512) {
        let Ok(line) = line else { continue };
        let Ok(row) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) else {
            continue;
        };
        let payload = &row["payload"];
        match row["type"].as_str().unwrap_or_default() {
            "session_meta" => {
                if !saw_meta {
                    let id = string(payload, "id");
                    if !id.is_empty() {
                        session.id = id;
                    }
                    session.cwd = string(payload, "cwd");
                }
                saw_meta = true;
            }
            "turn_context" => {
                let model = string(payload, "model");
                if !model.is_empty() {
                    session.model = model;
                }
                let cwd = string(payload, "cwd");
                if !cwd.is_empty() {
                    session.cwd = cwd;
                }
            }
            "event_msg" if payload["type"] == "user_message" => {
                if let Some(hint) = title(&string(payload, "message")) {
                    session.title = hint;
                }
            }
            "response_item"
                if payload["type"] == "message"
                    && payload["role"] == "user"
                    && session.title.is_empty() =>
            {
                if let Some(hint) = title(&text(&payload["content"])) {
                    session.title = hint;
                }
            }
            _ => {}
        }
        if saw_meta && !session.title.is_empty() && !session.model.is_empty() {
            break;
        }
    }
    if !saw_meta {
        return None;
    }
    if let Some(id) = filename_id {
        session.id = id;
    }
    if session.title.is_empty() {
        session.title = session.id.clone();
    }
    Some(session)
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
        if !id.is_empty() && !name.trim().is_empty() {
            names.insert(id, name);
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

fn parse(path: &Path) -> Result<Parsed, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut parsed = Parsed::default();
    // The bool marks an event fallback. Only neighboring visible twins are
    // paired: matching text elsewhere can belong to a different user turn.
    let mut candidates: Vec<(DisplayMessage, bool)> = Vec::new();
    let mut dedup_boundaries = std::collections::HashSet::new();
    let mut calls: HashMap<String, usize> = HashMap::new();
    let mut model = String::new();
    for (line_index, line) in BufReader::new(file).split(b'\n').enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(line) = std::str::from_utf8(&line) else {
            continue;
        };
        let Ok(row) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) else {
            continue;
        };
        let payload = &row["payload"];
        let ts = string(&row, "timestamp");
        let kind = row["type"].as_str().unwrap_or_default();
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
            let usage = &payload["thread_token_usage"];
            if usage.is_object() {
                parsed.input_tokens = usage["input_tokens"].as_i64().unwrap_or(0);
                parsed.output_tokens = usage["output_tokens"].as_i64().unwrap_or(0);
                parsed.cache_read_tokens = usage["cached_input_tokens"].as_i64().unwrap_or(0);
                parsed.total_tokens = usage["total_tokens"]
                    .as_i64()
                    .unwrap_or(parsed.input_tokens + parsed.output_tokens);
            }
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
                    parsed.active = true;
                    dedup_boundaries.insert(candidates.len());
                }
                "task_complete" | "task_completed" | "turn_complete" | "turn_completed"
                | "turn_aborted" => {
                    parsed.active = false;
                    dedup_boundaries.insert(candidates.len());
                }
                "token_count" => {
                    let usage = &payload["info"]["total_token_usage"];
                    if usage.is_object() {
                        parsed.input_tokens = usage["input_tokens"].as_i64().unwrap_or(0);
                        parsed.output_tokens = usage["output_tokens"].as_i64().unwrap_or(0);
                        parsed.cache_read_tokens =
                            usage["cached_input_tokens"].as_i64().unwrap_or(0);
                        parsed.total_tokens = usage["total_tokens"]
                            .as_i64()
                            .unwrap_or(parsed.input_tokens + parsed.output_tokens);
                    }
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
                if payload["phase"] == "final_answer" || payload["channel"] == "final" {
                    parsed.active = false;
                }
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
    for (index, (msg, fallback)) in candidates.iter().enumerate() {
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
                keep[if *fallback { index } else { previous_index }] = false;
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
    Ok(parsed)
}

type ActiveCache = Option<(PathBuf, String, Arc<Parsed>)>;
type IndexCache = Option<(PathBuf, String, CodexWindow)>;
pub fn load_window(path: &Path, start: usize, limit: Option<usize>) -> Result<CodexWindow, String> {
    let stat = fs::metadata(path).map_err(|e| e.to_string())?;
    if !stat.is_file() || path.extension().map_or(true, |e| e != "jsonl") {
        return Err("path must be a Codex JSONL rollout file".into());
    }
    let stamp = revision(&stat);
    let recent = stat
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|d| d.as_secs() < 120);
    static INDEX: OnceLock<Mutex<IndexCache>> = OnceLock::new();
    let index_cache = INDEX.get_or_init(|| Mutex::new(None));
    if limit == Some(0) {
        let guard = index_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((old_path, old_stamp, index)) = &*guard {
            if old_path == path && old_stamp == &stamp {
                let mut index = index.clone();
                index.start = start.min(index.count);
                index.ongoing &= recent;
                return Ok(index);
            }
        }
    }
    static CACHE: OnceLock<Mutex<ActiveCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let cached = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(old_path, old_stamp, _)| old_path == path && old_stamp == &stamp)
        .map(|(_, _, parsed)| Arc::clone(parsed));
    let parsed = if let Some(parsed) = cached {
        parsed
    } else {
        let parsed = Arc::new(parse(path)?);
        // Keep polling cheap without retaining unbounded tool outputs for a
        // very large rollout. At most one modest-sized conversation is cached.
        let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        *guard = (stat.len() <= 16 * 1024 * 1024)
            .then(|| (path.to_path_buf(), stamp.clone(), Arc::clone(&parsed)));
        parsed
    };
    let count = parsed.messages.len();
    let start = start.min(count);
    let end = start.saturating_add(limit.unwrap_or(usize::MAX)).min(count);
    let mut index = CodexWindow {
        messages: Vec::new(),
        count,
        start,
        roles: parsed.messages.iter().map(|m| m.role.clone()).collect(),
        ongoing: parsed.active && recent,
        input_tokens: parsed.input_tokens,
        output_tokens: parsed.output_tokens,
        cache_read_tokens: parsed.cache_read_tokens,
        total_tokens: parsed.total_tokens,
        revision: stamp,
    };
    *index_cache.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((path.to_path_buf(), index.revision.clone(), index.clone()));
    index.messages = parsed.messages[start..end].to_vec();
    Ok(index)
}

#[cfg(test)]
mod tests;
