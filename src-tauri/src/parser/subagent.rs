use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::chunk::*;
use super::classify::*;
use super::entry::parse_entry;

/// Per-requestId token snapshot used for deduplication across JSONL files.
#[derive(Clone)]
pub struct TokenSnapshot {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_create: i64,
    pub model: String,
}

/// Pricing per million tokens for a model family.
struct ModelPricing {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
}

fn pricing_for_model(model: &str) -> ModelPricing {
    let m = model.to_lowercase();
    if m.contains("opus") {
        ModelPricing { input: 5.0, output: 25.0, cache_read: 0.5, cache_write: 6.25 }
    } else if m.contains("haiku") {
        ModelPricing { input: 1.0, output: 5.0, cache_read: 0.1, cache_write: 1.25 }
    } else {
        // Default to sonnet
        ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 }
    }
}

/// Compute cost in USD from a set of token snapshots, pricing each by its model.
pub fn estimate_cost_from_snapshots(
    request_tokens: &std::collections::HashMap<String, TokenSnapshot>,
    fallback: &TokenSnapshot,
) -> f64 {
    let mut cost = 0.0;

    // Fallback tokens (no requestId) — use fallback.model for pricing.
    if fallback.input + fallback.output + fallback.cache_read + fallback.cache_create > 0 {
        let p = pricing_for_model(&fallback.model);
        cost += (fallback.input as f64 * p.input
            + fallback.output as f64 * p.output
            + fallback.cache_read as f64 * p.cache_read
            + fallback.cache_create as f64 * p.cache_write)
            / 1_000_000.0;
    }

    for snap in request_tokens.values() {
        let p = pricing_for_model(&snap.model);
        cost += (snap.input as f64 * p.input
            + snap.output as f64 * p.output
            + snap.cache_read as f64 * p.cache_read
            + snap.cache_create as f64 * p.cache_write)
            / 1_000_000.0;
    }
    cost
}

/// SubagentProcess holds a parsed subagent and its computed metadata.
#[derive(Debug, Clone, Serialize)]
pub struct SubagentProcess {
    pub id: String,
    pub file_path: String,
    pub file_mod_time: DateTime<Utc>,
    pub chunks: Vec<Chunk>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: i64,
    pub usage: Usage,
    pub description: String,
    pub subagent_type: String,
    pub parent_task_id: String,
    pub team_summary: String,
    pub teammate_color: String,
}

impl Default for SubagentProcess {
    fn default() -> Self {
        Self {
            id: String::new(),
            file_path: String::new(),
            file_mod_time: Utc::now(),
            chunks: Vec::new(),
            start_time: Utc::now(),
            end_time: Utc::now(),
            duration_ms: 0,
            usage: Usage::default(),
            description: String::new(),
            subagent_type: String::new(),
            parent_task_id: String::new(),
            team_summary: String::new(),
            teammate_color: String::new(),
        }
    }
}

/// Discover all subagent and team processes for a session, link them, and return
/// the process list with its color map.
pub fn discover_and_link_all(
    session_path: &str,
    chunks: &[Chunk],
) -> (Vec<SubagentProcess>, HashMap<String, String>) {
    let subagents = discover_subagents(session_path).unwrap_or_default();
    let team_procs = discover_team_sessions(session_path, chunks).unwrap_or_default();
    let mut all_procs: Vec<SubagentProcess> = subagents;
    all_procs.extend(team_procs);
    let color_map = link_subagents(&mut all_procs, chunks, session_path);
    (all_procs, color_map)
}

/// Discover and parse subagent files for a session.
pub fn discover_subagents(session_path: &str) -> Result<Vec<SubagentProcess>, String> {
    let dir = Path::new(session_path).parent().unwrap_or(Path::new(""));
    let base = Path::new(session_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let subagents_dir = dir.join(base).join("subagents");

    let entries = match fs::read_dir(&subagents_dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut procs = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("agent-") || !name.ends_with(".jsonl") {
            continue;
        }

        let agent_id = name
            .strip_prefix("agent-")
            .unwrap_or("")
            .strip_suffix(".jsonl")
            .unwrap_or("")
            .to_string();

        let file_path = subagents_dir.join(&name).to_string_lossy().to_string();

        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.len() == 0 {
            continue;
        }

        if is_warmup_agent(&file_path) {
            continue;
        }

        let (chunks, team_summary, team_color) = read_subagent_session(&file_path)?;
        if chunks.is_empty() {
            continue;
        }

        let (start_time, end_time, duration_ms) = chunk_timing(&chunks);
        let usage = aggregate_usage(&chunks);
        let file_mod_time = metadata
            .modified()
            .ok()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(Utc::now);

        procs.push(SubagentProcess {
            id: agent_id,
            file_path,
            file_mod_time,
            chunks,
            start_time,
            end_time,
            duration_ms,
            usage,
            team_summary,
            teammate_color: team_color,
            ..Default::default()
        });
    }

    procs.sort_by(|a, b| a.start_time.cmp(&b.start_time));
    Ok(procs)
}

fn is_warmup_agent(path: &str) -> bool {
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let reader = BufReader::new(f);
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        let raw: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if entry_type != "user" {
            continue;
        }
        let content = raw
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");
        return content == "Warmup";
    }
    false
}

fn read_subagent_session(path: &str) -> Result<(Vec<Chunk>, String, String), String> {
    use super::patterns::{TEAMMATE_SUMMARY_RE, TEAMMATE_COLOR_RE};

    let f = fs::File::open(path).map_err(|e| format!("opening {}: {}", path, e))?;
    let reader = BufReader::new(f);

    let mut msgs = Vec::new();
    let mut team_summary = String::new();
    let mut team_color = String::new();
    let mut extracted_team_meta = false;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };

        let mut entry = match parse_entry(line.as_bytes()) {
            Some(e) => e,
            None => continue,
        };

        // Extract team metadata from first user entry.
        if !extracted_team_meta && entry.entry_type == "user" {
            if let Some(content) = &entry.message.content {
                if let Some(s) = content.as_str() {
                    if let Some(caps) = TEAMMATE_SUMMARY_RE.captures(s) {
                        team_summary = caps.get(1).map_or("", |m| m.as_str()).to_string();
                    }
                    if let Some(caps) = TEAMMATE_COLOR_RE.captures(s) {
                        team_color = caps.get(1).map_or("", |m| m.as_str()).to_string();
                    }
                    extracted_team_meta = true;
                }
            }
        }

        // Clear sidechain flag so classify doesn't filter.
        entry.is_sidechain = false;
        if let Some(msg) = classify(entry) {
            msgs.push(msg);
        }
    }

    Ok((build_chunks(&msgs), team_summary, team_color))
}

fn chunk_timing(chunks: &[Chunk]) -> (DateTime<Utc>, DateTime<Utc>, i64) {
    let mut start: Option<DateTime<Utc>> = None;
    let mut end: Option<DateTime<Utc>> = None;

    for c in chunks {
        if start.is_none() || c.timestamp < *start.as_ref().unwrap() {
            start = Some(c.timestamp);
        }
        if end.is_none() || c.timestamp > *end.as_ref().unwrap() {
            end = Some(c.timestamp);
        }
    }

    let s = start.unwrap_or_else(Utc::now);
    let e = end.unwrap_or(s);
    let dur = e.signed_duration_since(s).num_milliseconds();
    (s, e, dur)
}

fn aggregate_usage(chunks: &[Chunk]) -> Usage {
    for c in chunks.iter().rev() {
        if c.chunk_type == ChunkType::AI && c.usage.total_tokens() > 0 {
            return c.usage.clone();
        }
    }
    Usage::default()
}

/// Link subagents to parent Task tool calls. Returns toolID -> color map.
pub fn link_subagents(
    processes: &mut Vec<SubagentProcess>,
    parent_chunks: &[Chunk],
    parent_session_path: &str,
) -> HashMap<String, String> {
    let mut links = scan_agent_links(parent_session_path);

    // Also scan subagent files for skill_progress entries (Skill forked execution).
    for proc in processes.iter() {
        let skill_links = scan_skill_progress_links(&proc.file_path);
        for (agent_id, tool_id) in skill_links {
            links.agent_to_tool_id.entry(agent_id).or_insert(tool_id);
        }
    }

    if processes.is_empty() {
        return links.tool_id_to_color;
    }

    // Collect tool IDs that have known agent links (for ToolCall items like Skill).
    let linked_tool_ids: HashSet<&str> = links.agent_to_tool_id.values().map(|s| s.as_str()).collect();

    // Collect all Subagent DisplayItems + ToolCall items that have agent links.
    let mut task_items: Vec<&DisplayItem> = Vec::new();
    for c in parent_chunks {
        if c.chunk_type != ChunkType::AI {
            continue;
        }
        for item in &c.items {
            if item.item_type == DisplayItemType::Subagent {
                task_items.push(item);
            } else if item.item_type == DisplayItemType::ToolCall && linked_tool_ids.contains(item.tool_id.as_str()) {
                task_items.push(item);
            }
        }
    }

    if task_items.is_empty() {
        return links.tool_id_to_color;
    }

    let tool_id_to_task: HashMap<String, &DisplayItem> = task_items
        .iter()
        .map(|it| (it.tool_id.clone(), *it))
        .collect();

    let mut matched_procs: HashMap<String, bool> = HashMap::new();
    let mut matched_tools: HashMap<String, bool> = HashMap::new();

    // Phase 1: Result-based matching.
    for proc in processes.iter_mut() {
        if let Some(tool_id) = links.agent_to_tool_id.get(&proc.id) {
            if let Some(item) = tool_id_to_task.get(tool_id) {
                proc.parent_task_id = item.tool_id.clone();
                proc.description = item.subagent_desc.clone();
                proc.subagent_type = item.subagent_type.clone();
                matched_procs.insert(proc.id.clone(), true);
                matched_tools.insert(tool_id.clone(), true);
            } else {
                // Tool ID not in parent chunks (e.g. Skill inside a subagent).
                // Set parent_task_id anyway so convert_display_items can link
                // when processing the subagent's nested chunks.
                proc.parent_task_id = tool_id.clone();
                matched_procs.insert(proc.id.clone(), true);
            }
        }
    }

    // Phase 2: Team member matching by description.
    let team_task_items: Vec<&&DisplayItem> = task_items
        .iter()
        .filter(|it| !matched_tools.contains_key(&it.tool_id) && is_team_task(it))
        .collect();

    for item in &team_task_items {
        let mut best_idx: Option<usize> = None;
        for (i, proc) in processes.iter().enumerate() {
            if matched_procs.contains_key(&proc.id) {
                continue;
            }
            if proc.team_summary.is_empty() || proc.team_summary != item.subagent_desc {
                continue;
            }
            if best_idx.is_none() || processes[i].start_time < processes[best_idx.unwrap()].start_time {
                best_idx = Some(i);
            }
        }
        if let Some(idx) = best_idx {
            processes[idx].parent_task_id = item.tool_id.clone();
            processes[idx].description = item.subagent_desc.clone();
            processes[idx].subagent_type = item.subagent_type.clone();
            matched_procs.insert(processes[idx].id.clone(), true);
            matched_tools.insert(item.tool_id.clone(), true);
        }
    }

    // Phase 3: Positional fallback.
    let unmatched_procs: Vec<usize> = processes
        .iter()
        .enumerate()
        .filter(|(_, p)| !matched_procs.contains_key(&p.id))
        .map(|(i, _)| i)
        .collect();
    let unmatched_tasks: Vec<&&DisplayItem> = task_items
        .iter()
        .filter(|it| !matched_tools.contains_key(&it.tool_id) && !is_team_task(it))
        .collect();

    for (i, task) in unmatched_tasks.iter().enumerate() {
        if i >= unmatched_procs.len() {
            break;
        }
        let idx = unmatched_procs[i];
        processes[idx].parent_task_id = task.tool_id.clone();
        processes[idx].description = task.subagent_desc.clone();
        processes[idx].subagent_type = task.subagent_type.clone();
    }

    // Populate teammate color from toolUseResult data.
    for proc in processes.iter_mut() {
        if proc.teammate_color.is_empty() && !proc.parent_task_id.is_empty() {
            if let Some(color) = links.tool_id_to_color.get(&proc.parent_task_id) {
                proc.teammate_color = color.clone();
            }
        }
    }

    // Remap IDs for team workers.
    for proc in processes.iter_mut() {
        if proc.parent_task_id.is_empty() {
            continue;
        }
        if let Some(item) = tool_id_to_task.get(&proc.parent_task_id) {
            if is_team_task(item) {
                if let Some(Value::Object(map)) = &item.tool_input {
                    let team_name = map.get("team_name").and_then(|v| v.as_str()).unwrap_or("");
                    let agent_name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if !team_name.is_empty() && !agent_name.is_empty() {
                        proc.id = format!("{}@{}", agent_name, team_name);
                    }
                }
            }
        }
    }

    links.tool_id_to_color
}

struct AgentLinkData {
    agent_to_tool_id: HashMap<String, String>,
    tool_id_to_color: HashMap<String, String>,
}

fn scan_agent_links(session_path: &str) -> AgentLinkData {
    let mut data = AgentLinkData {
        agent_to_tool_id: HashMap::new(),
        tool_id_to_color: HashMap::new(),
    };

    let f = match fs::File::open(session_path) {
        Ok(f) => f,
        Err(_) => return data,
    };
    let reader = BufReader::new(f);

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        let entry = match parse_entry(line.as_bytes()) {
            Some(e) => e,
            None => continue,
        };
        let result_map = match entry.tool_use_result_map() {
            Some(m) => m,
            None => continue,
        };

        let agent_id = result_map
            .get("agentId")
            .or_else(|| result_map.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if agent_id.is_empty() {
            continue;
        }

        let mut tool_use_id = entry.source_tool_use_id.clone();
        if tool_use_id.is_empty() {
            tool_use_id = extract_first_tool_result_id(&entry);
        }
        if tool_use_id.is_empty() {
            continue;
        }

        data.agent_to_tool_id.insert(agent_id, tool_use_id.clone());

        if let Some(color) = result_map.get("color").and_then(|v| v.as_str()) {
            if !color.is_empty() {
                data.tool_id_to_color.insert(tool_use_id, color.to_string());
            }
        }
    }

    data
}

/// Scan a JSONL file for skill_progress entries to find Skill → agent links.
/// Returns agentId → tool_use_id mapping.
fn scan_skill_progress_links(file_path: &str) -> HashMap<String, String> {
    let mut result: HashMap<String, String> = HashMap::new();

    let f = match fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return result,
    };
    let reader = BufReader::new(f);

    // Track the last Skill tool_use_id seen
    let mut last_skill_tool_id = String::new();

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };

        // Quick check before full parse
        if line.contains("\"Skill\"") {
            // Look for tool_use with name=Skill
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let Some(content) = v.pointer("/message/content") {
                    if let Some(arr) = content.as_array() {
                        for block in arr {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                                && block.get("name").and_then(|v| v.as_str()) == Some("Skill")
                            {
                                if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                                    last_skill_tool_id = id.to_string();
                                }
                            }
                        }
                    }
                }
            }
        } else if line.contains("skill_progress") && !last_skill_tool_id.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let data_type = v.pointer("/data/type").and_then(|v| v.as_str()).unwrap_or("");
                if data_type == "skill_progress" {
                    if let Some(agent_id) = v.pointer("/data/agentId").and_then(|v| v.as_str()) {
                        if !agent_id.is_empty() && !result.contains_key(agent_id) {
                            result.insert(agent_id.to_string(), last_skill_tool_id.clone());
                        }
                    }
                }
            }
        }
    }

    result
}

fn extract_first_tool_result_id(entry: &super::entry::Entry) -> String {
    if let Some(Value::Array(blocks)) = &entry.message.content {
        for b in blocks {
            let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if bt == "tool_result" {
                if let Some(id) = b.get("tool_use_id").and_then(|v| v.as_str()) {
                    if !id.is_empty() {
                        return id.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

/// Discover team sessions in the project directory.
pub fn discover_team_sessions(
    session_path: &str,
    parent_chunks: &[Chunk],
) -> Result<Vec<SubagentProcess>, String> {
    let specs = extract_team_specs(parent_chunks);
    if specs.is_empty() {
        return Ok(Vec::new());
    }

    let wanted: HashMap<(String, String), bool> = specs
        .iter()
        .map(|s| ((s.0.clone(), s.1.clone()), true))
        .collect();

    let project_dir = Path::new(session_path)
        .parent()
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .to_string();
    let parent_base = Path::new(session_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let entries = match fs::read_dir(&project_dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut procs = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jsonl") || name == parent_base || name.starts_with("agent-") {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }

        let file_path = entry.path().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if metadata.len() == 0 {
            continue;
        }

        let (team_name, agent_name) = read_team_session_meta(&file_path);
        if team_name.is_empty() || agent_name.is_empty() {
            continue;
        }
        if !wanted.contains_key(&(team_name.clone(), agent_name.clone())) {
            continue;
        }

        let (chunks, _, team_color) = read_subagent_session(&file_path)?;
        if chunks.is_empty() {
            continue;
        }

        let (start_time, end_time, duration_ms) = chunk_timing(&chunks);
        let usage = aggregate_usage(&chunks);
        let file_mod_time = metadata
            .modified()
            .ok()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(Utc::now);

        procs.push(SubagentProcess {
            id: format!("{}@{}", agent_name, team_name),
            file_path,
            file_mod_time,
            chunks,
            start_time,
            end_time,
            duration_ms,
            usage,
            teammate_color: team_color,
            ..Default::default()
        });
    }

    procs.sort_by(|a, b| a.start_time.cmp(&b.start_time));
    Ok(procs)
}

fn read_team_session_meta(path: &str) -> (String, String) {
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (String::new(), String::new()),
    };
    let reader = BufReader::new(f);
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        let raw: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let team_name = raw.get("teamName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let agent_name = raw.get("agentName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        return (team_name, agent_name);
    }
    (String::new(), String::new())
}

/// Extract (team_name, agent_name) pairs from Task items in parent chunks.
fn extract_team_specs(chunks: &[Chunk]) -> Vec<(String, String)> {
    let mut specs = Vec::new();
    for c in chunks {
        if c.chunk_type != ChunkType::AI {
            continue;
        }
        for item in &c.items {
            if item.item_type != DisplayItemType::Subagent || !is_team_task(item) {
                continue;
            }
            if let Some(Value::Object(map)) = &item.tool_input {
                let tn = map.get("team_name").and_then(|v| v.as_str()).unwrap_or("");
                let an = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if !tn.is_empty() && !an.is_empty() {
                    specs.push((tn.to_string(), an.to_string()));
                }
            }
        }
    }
    specs
}

/// Resolve the subagents directory for a session file path.
pub fn subagents_dir(session_path: &str) -> std::path::PathBuf {
    let dir = Path::new(session_path)
        .parent()
        .unwrap_or(Path::new(""));
    let base = Path::new(session_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    dir.join(base).join("subagents")
}

/// Returns true if any subagent JSONL file was recently modified (within staleness threshold).
pub fn has_recently_active_subagents(session_path: &str) -> bool {
    let dir = subagents_dir(session_path);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("agent-") || !name.ends_with(".jsonl") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if super::ongoing::apply_staleness(true, modified) {
                    return true;
                }
            }
        }
    }
    false
}

/// Scan all subagent JSONL files, inserting token snapshots into a shared
/// `request_tokens` map for global requestId deduplication with the main session.
/// Entries without a requestId are accumulated directly into `fallback`.
pub fn scan_subagent_tokens_into(
    session_path: &str,
    request_tokens: &mut HashMap<String, TokenSnapshot>,
    fallback: &mut TokenSnapshot,
) {
    let dir = subagents_dir(session_path);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("agent-") || !name.ends_with(".jsonl") {
            continue;
        }
        // Include compact files — they contain requestIds from compacted
        // conversation data that may no longer be in the main session file.
        let file_path = dir.join(&name);
        let f = match fs::File::open(&file_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(f);
        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => break,
            };
            let raw: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if entry_type != "assistant" {
                continue;
            }
            let model = raw
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if model == "<synthetic>" {
                continue;
            }
            let usage = match raw.get("message").and_then(|m| m.get("usage")) {
                Some(u) => u,
                None => continue,
            };
            let inp = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let out = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let cr = usage.get("cache_read_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let cc = usage.get("cache_creation_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            if inp + out + cr + cc == 0 {
                continue;
            }
            let snap = TokenSnapshot {
                input: inp, output: out, cache_read: cr, cache_create: cc,
                model: model.to_string(),
            };
            let req_id = raw.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
            if !req_id.is_empty() {
                request_tokens.insert(req_id.to_string(), snap);
            } else {
                fallback.input += inp;
                fallback.output += out;
                fallback.cache_read += cr;
                fallback.cache_create += cc;
            }
        }
    }
}
