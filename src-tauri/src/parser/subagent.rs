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
#[derive(Clone, Default)]
pub struct TokenSnapshot {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_create: i64,
    pub model: String,
    /// Whether the response completed (has a stop_reason).
    /// Entries without stop_reason are partial streaming snapshots whose
    /// output_tokens may be understated.
    pub has_stop_reason: bool,
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
        ModelPricing {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        }
    } else if m.contains("haiku") {
        ModelPricing {
            input: 1.0,
            output: 5.0,
            cache_read: 0.1,
            cache_write: 1.25,
        }
    } else {
        // Default to sonnet
        ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        }
    }
}

/// Insert a token snapshot, preferring entries with stop_reason (complete responses)
/// over partial streaming snapshots with understated output_tokens.
pub fn insert_best_snapshot(
    map: &mut HashMap<String, TokenSnapshot>,
    key: String,
    snap: TokenSnapshot,
) {
    match map.get(&key) {
        Some(existing) => {
            // Prefer complete entries (has_stop_reason) over partial ones.
            // Among same completeness, prefer higher output (later streaming = more tokens).
            if (!existing.has_stop_reason && snap.has_stop_reason)
                || (existing.has_stop_reason == snap.has_stop_reason
                    && snap.output >= existing.output)
            {
                map.insert(key, snap);
            }
        }
        None => {
            map.insert(key, snap);
        }
    }
}

/// Estimate output tokens from assistant message content character count.
/// Returns 0 if estimation is not possible.
pub fn estimate_output_from_content(raw: &serde_json::Value) -> i64 {
    let content = match raw.get("message").and_then(|m| m.get("content")) {
        Some(c) => c,
        None => return 0,
    };
    let arr = match content.as_array() {
        Some(a) => a,
        None => return 0,
    };

    let mut total_chars: i64 = 0;
    for block in arr {
        let btype = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match btype {
            "text" => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    total_chars += t.len() as i64;
                }
            }
            "thinking" => {
                if let Some(t) = block.get("thinking").and_then(|v| v.as_str()) {
                    total_chars += t.len() as i64;
                }
            }
            "tool_use" => {
                // tool_use name + JSON input serialization
                total_chars += 20; // name overhead
                if let Some(input) = block.get("input") {
                    total_chars += input.to_string().len() as i64;
                }
            }
            _ => {}
        }
    }
    // Rough estimate: ~4 characters per token for English text/code.
    total_chars / 4
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

/// A lookup graph over a flat slice of SubagentProcesses.
///
/// Centralises the repeated pattern of:
/// - mapping `parent_task_id → &SubagentProcess` (used by convert)
/// - finding the child processes spawned by a given proc (used by ongoing)
///
/// Both consumers receive a `&ProcGraph` so the map is built exactly once
/// and cycle-detection logic is not duplicated.
pub struct ProcGraph<'a> {
    procs: &'a [SubagentProcess],
    /// parent_task_id → index into `procs`
    by_task_id: HashMap<&'a str, usize>,
}

impl<'a> ProcGraph<'a> {
    pub fn new(procs: &'a [SubagentProcess]) -> Self {
        let by_task_id = procs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.parent_task_id.is_empty())
            .map(|(i, p)| (p.parent_task_id.as_str(), i))
            .collect();
        Self { procs, by_task_id }
    }

    /// Look up the SubagentProcess whose `parent_task_id` matches `tool_id`.
    pub fn get(&self, tool_id: &str) -> Option<&'a SubagentProcess> {
        self.by_task_id.get(tool_id).map(|&i| &self.procs[i])
    }

    /// Return all SubagentProcesses directly spawned by `proc`
    /// (Subagent display items or Task/Agent tool calls).
    pub fn children_of(&self, proc: &SubagentProcess) -> Vec<&'a SubagentProcess> {
        proc.chunks
            .iter()
            .flat_map(|c| c.items.iter())
            .filter(|it| {
                it.item_type == DisplayItemType::Subagent
                    || ((it.tool_name == "Task" || it.tool_name == "Agent")
                        && it.item_type == DisplayItemType::ToolCall)
            })
            .filter_map(|it| self.get(it.tool_id.as_str()))
            .collect()
    }
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
    pub prompt: String,
    /// True when the JSONL contains a `<synthetic>` end marker, meaning the
    /// subagent has definitively finished regardless of what the chunks look like.
    pub has_end_marker: bool,
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
            prompt: String::new(),
            has_end_marker: false,
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

        let session_data = read_subagent_session(&file_path)?;
        if session_data.chunks.is_empty() {
            continue;
        }

        let meta_path = subagents_dir
            .join(name.replace(".jsonl", ".meta.json"))
            .to_string_lossy()
            .to_string();
        let subagent_type = read_agent_type(&meta_path);

        let mut proc = build_subagent_process(
            agent_id,
            file_path,
            &metadata,
            session_data.chunks,
            subagent_type,
            session_data.team_summary,
            session_data.team_color,
        );
        proc.has_end_marker = session_data.has_end_marker;
        procs.push(proc);
    }

    procs.sort_by_key(|a| a.start_time);
    Ok(procs)
}

/// Extract the first user chunk's text from a list of chunks.
fn first_user_text(chunks: &[Chunk]) -> String {
    for c in chunks {
        if c.chunk_type == ChunkType::User && !c.user_text.is_empty() {
            return c.user_text.clone();
        }
    }
    String::new()
}

/// Build a SubagentProcess from parsed chunks and file metadata.
fn build_subagent_process(
    id: String,
    file_path: String,
    metadata: &std::fs::Metadata,
    chunks: Vec<Chunk>,
    subagent_type: String,
    team_summary: String,
    teammate_color: String,
) -> SubagentProcess {
    let (start_time, end_time, duration_ms) = chunk_timing(&chunks);
    let usage = aggregate_usage(&chunks);
    let file_mod_time = metadata
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now);
    let prompt = first_user_text(&chunks);

    SubagentProcess {
        id,
        file_path,
        file_mod_time,
        chunks,
        start_time,
        end_time,
        duration_ms,
        usage,
        subagent_type,
        team_summary,
        teammate_color,
        prompt,
        ..Default::default()
    }
}

/// Read agentType from a .meta.json file next to a subagent .jsonl.
fn read_agent_type(meta_path: &str) -> String {
    fs::read_to_string(meta_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("agentType")
                .and_then(|a| a.as_str())
                .map(String::from)
        })
        .unwrap_or_default()
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

/// Parsed subagent session data.
struct SubagentSessionData {
    chunks: Vec<Chunk>,
    team_summary: String,
    team_color: String,
    /// True when the JSONL contains a `<synthetic>` assistant end marker.
    has_end_marker: bool,
}

fn read_subagent_session(path: &str) -> Result<SubagentSessionData, String> {
    use super::patterns::{TEAMMATE_COLOR_RE, TEAMMATE_SUMMARY_RE};

    let f = fs::File::open(path).map_err(|e| format!("opening {path}: {e}"))?;
    let reader = BufReader::new(f);

    let mut msgs = Vec::new();
    let mut team_summary = String::new();
    let mut team_color = String::new();
    let mut extracted_team_meta = false;
    let mut has_end_marker = false;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };

        let mut entry = match parse_entry(line.as_bytes()) {
            Some(e) => e,
            None => continue,
        };

        // Detect <synthetic> assistant end marker — signals the subagent
        // has definitively finished. classify() filters these out, so we
        // detect them here before classification.
        if entry.entry_type == "assistant" && entry.message.model == "<synthetic>" {
            has_end_marker = true;
        }

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

    Ok(SubagentSessionData {
        chunks: build_chunks(&msgs),
        team_summary,
        team_color,
        has_end_marker,
    })
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
    processes: &mut [SubagentProcess],
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
    let linked_tool_ids: HashSet<&str> = links
        .agent_to_tool_id
        .values()
        .map(|s| s.as_str())
        .collect();

    // Collect all Subagent DisplayItems + ToolCall items that have agent links.
    let mut task_items: Vec<&DisplayItem> = Vec::new();
    for c in parent_chunks {
        if c.chunk_type != ChunkType::AI {
            continue;
        }
        for item in &c.items {
            if item.item_type == DisplayItemType::Subagent
                || (item.item_type == DisplayItemType::ToolCall
                    && linked_tool_ids.contains(item.tool_id.as_str()))
            {
                task_items.push(item);
            }
        }
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
            if best_idx.is_none()
                || processes[i].start_time < processes[best_idx.unwrap()].start_time
            {
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

    // Phase 4: Nested enrichment — search all processes' chunks for task items
    // to fill in description/type for processes that Phase 1 linked by tool_id
    // but couldn't find in the parent session's chunks. This handles cases where
    // a subagent spawns further subagents via Skill/Agent.
    let needs_enrichment = processes
        .iter()
        .any(|p| !p.parent_task_id.is_empty() && p.description.is_empty());
    if needs_enrichment {
        // Collect task items from all processes' chunks.
        let mut nested_task_items: Vec<DisplayItem> = Vec::new();
        for proc in processes.iter() {
            for c in &proc.chunks {
                if c.chunk_type != ChunkType::AI {
                    continue;
                }
                for item in &c.items {
                    if item.item_type == DisplayItemType::Subagent
                        || (item.item_type == DisplayItemType::ToolCall
                            && linked_tool_ids.contains(item.tool_id.as_str()))
                    {
                        nested_task_items.push(item.clone());
                    }
                }
            }
        }
        let nested_task_map: HashMap<String, &DisplayItem> = nested_task_items
            .iter()
            .map(|it| (it.tool_id.clone(), it))
            .collect();
        for proc in processes.iter_mut() {
            if proc.description.is_empty() {
                if let Some(item) = nested_task_map.get(&proc.parent_task_id) {
                    // Use subagent_desc if available, otherwise fall back to tool_summary.
                    proc.description = if !item.subagent_desc.is_empty() {
                        item.subagent_desc.clone()
                    } else {
                        item.tool_summary.clone()
                    };
                    if proc.subagent_type.is_empty() {
                        proc.subagent_type = item.subagent_type.clone();
                    }
                }
            }
        }
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
                        proc.id = format!("{agent_name}@{team_name}");
                    }
                }
            }
        }
    }

    links.tool_id_to_color
}

/// Inject synthetic Subagent DisplayItems for processes that remain unlinked
/// (no parent_task_id) after all linking phases. Appends them to the last AI
/// chunk, or creates a new AI chunk if none exists.
pub fn inject_orphan_subagents(chunks: &mut Vec<Chunk>, processes: &mut [SubagentProcess]) {
    let mut orphan_indices: Vec<usize> = processes
        .iter()
        .enumerate()
        .filter(|(_, p)| p.parent_task_id.is_empty() && !p.chunks.is_empty())
        .map(|(i, _)| i)
        .collect();

    if orphan_indices.is_empty() {
        return;
    }

    // Sort orphans oldest-first (ascending start_time).
    orphan_indices.sort_by(|&a, &b| processes[a].start_time.cmp(&processes[b].start_time));

    // Find or create last AI chunk.
    let ai_idx = chunks.iter().rposition(|c| c.chunk_type == ChunkType::AI);

    let idx = match ai_idx {
        Some(i) => i,
        None => {
            // No AI chunk — create a synthetic one.
            chunks.push(Chunk {
                chunk_type: ChunkType::AI,
                timestamp: processes[orphan_indices[0]].start_time,
                ..Default::default()
            });
            chunks.len() - 1
        }
    };

    for &oi in &orphan_indices {
        let synthetic_tool_id = format!("orphan-{}", processes[oi].id);
        // Set parent_task_id so convert_display_items can link via proc_by_task_id.
        processes[oi].parent_task_id = synthetic_tool_id.clone();

        // Derive a description for the orphan from its prompt when the linking
        // phases didn't supply one (e.g. after /clear truncated the main JSONL).
        let desc = if processes[oi].description.is_empty() {
            orphan_description_from_prompt(&processes[oi].prompt)
        } else {
            processes[oi].description.clone()
        };

        chunks[idx].items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Agent".to_string(),
            tool_id: synthetic_tool_id,
            subagent_type: processes[oi].subagent_type.clone(),
            subagent_desc: desc,
            is_orphan: true,
            duration_ms: processes[oi].duration_ms,
            ..Default::default()
        });
    }
}

/// Derive a human-readable description for an orphan subagent from its prompt.
/// Skill-based subagents typically start with "Base directory for this skill: /…/skill-name\n".
/// Falls back to the first 80 characters of the prompt.
pub fn orphan_description_from_prompt(prompt: &str) -> String {
    if prompt.is_empty() {
        return String::new();
    }
    // Skill subagents: "Base directory for this skill: /path/to/skill-name"
    if let Some(rest) = prompt.strip_prefix("Base directory for this skill: ") {
        let first_line = rest.lines().next().unwrap_or("");
        // Extract the last path component as the skill name.
        if let Some(name) = first_line.rsplit('/').next() {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    // Fallback: first line, capped at 80 chars.
    let first_line = prompt.lines().next().unwrap_or(prompt);
    if first_line.len() > 80 {
        format!("{}…", &first_line[..80])
    } else {
        first_line.to_string()
    }
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

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };

        if !line.contains("skill_progress") {
            continue;
        }

        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            let data_type = v
                .pointer("/data/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if data_type != "skill_progress" {
                continue;
            }
            let agent_id = v
                .pointer("/data/agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parent_tool_id = v
                .get("parentToolUseID")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !agent_id.is_empty() && !parent_tool_id.is_empty() && !result.contains_key(agent_id)
            {
                result.insert(agent_id.to_string(), parent_tool_id.to_string());
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

        let session_data = read_subagent_session(&file_path)?;
        if session_data.chunks.is_empty() {
            continue;
        }

        let mut proc = build_subagent_process(
            format!("{agent_name}@{team_name}"),
            file_path,
            &metadata,
            session_data.chunks,
            String::new(),
            String::new(),
            session_data.team_color,
        );
        proc.has_end_marker = session_data.has_end_marker;
        procs.push(proc);
    }

    procs.sort_by_key(|a| a.start_time);
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
        let team_name = raw
            .get("teamName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let agent_name = raw
            .get("agentName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
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
    let dir = Path::new(session_path).parent().unwrap_or(Path::new(""));
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
            let inp = usage
                .get("input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let out = usage
                .get("output_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let cr = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let cc = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if inp + out + cr + cc == 0 {
                continue;
            }
            let has_stop = !raw
                .get("message")
                .and_then(|m| m.get("stop_reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .is_empty();
            let snap = TokenSnapshot {
                input: inp,
                output: out,
                cache_read: cr,
                cache_create: cc,
                model: model.to_string(),
                has_stop_reason: has_stop,
            };
            let req_id = raw.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
            if !req_id.is_empty() {
                insert_best_snapshot(request_tokens, req_id.to_string(), snap);
            } else {
                fallback.input += inp;
                fallback.output += out;
                fallback.cache_read += cr;
                fallback.cache_create += cc;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_chunk(text: &str) -> Chunk {
        Chunk {
            chunk_type: ChunkType::User,
            user_text: text.to_string(),
            ..Default::default()
        }
    }

    fn make_ai_chunk() -> Chunk {
        Chunk {
            chunk_type: ChunkType::AI,
            ..Default::default()
        }
    }

    #[test]
    fn first_user_text_returns_first_user_chunk() {
        let chunks = vec![
            make_ai_chunk(),
            make_user_chunk("hello world"),
            make_user_chunk("second user"),
        ];
        assert_eq!(first_user_text(&chunks), "hello world");
    }

    #[test]
    fn first_user_text_skips_empty_user_chunks() {
        let chunks = vec![make_user_chunk(""), make_user_chunk("non-empty")];
        assert_eq!(first_user_text(&chunks), "non-empty");
    }

    #[test]
    fn first_user_text_returns_empty_when_no_user_chunks() {
        let chunks = vec![make_ai_chunk()];
        assert_eq!(first_user_text(&chunks), "");
    }

    #[test]
    fn first_user_text_returns_empty_for_empty_vec() {
        assert_eq!(first_user_text(&[]), "");
    }

    /// Create temp session files simulating nested Skill-spawned agents.
    /// Layout: main session → orchestrator (orphan) → child agents (via Skill).
    fn setup_nested_skill_session() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let session_id = "test-session";
        let main_path = dir.path().join(format!("{session_id}.jsonl"));

        // Main session: just a user message + assistant text (no Agent/Task calls).
        std::fs::write(
            &main_path,
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"run pir"},"uuid":"u1","timestamp":"2026-03-12T21:20:00Z"}"#,
                "\n",
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Running PIR..."}],"stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50}},"requestId":"req1","uuid":"u2","timestamp":"2026-03-12T21:20:01Z"}"#,
                "\n",
            ),
        )
        .unwrap();

        // Subagent directory
        let sub_dir = dir.path().join(format!("{session_id}/subagents"));
        std::fs::create_dir_all(&sub_dir).unwrap();

        // Orchestrator subagent: has Skill tool_use + skill_progress
        let orch_id = "agent-orch001";
        let child_id = "agent-child01";
        let skill_tool_id = "toolu_skill_pg";
        std::fs::write(
            sub_dir.join(format!("{orch_id}.jsonl")),
            format!(
                concat!(
                    r#"{{"type":"user","agentId":"orch001","message":{{"role":"user","content":"Base directory for this skill: /skills/pir\n\n# Post Incident Record"}},"uuid":"o1","timestamp":"2026-03-12T21:21:00Z"}}"#,
                    "\n",
                    r#"{{"type":"assistant","agentId":"orch001","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{skill_tool_id}","name":"Skill","input":{{"skill":"pagerduty-oncall","args":"last 24h"}}}}],"stop_reason":"tool_use","usage":{{"input_tokens":200,"output_tokens":30}}}},"requestId":"req2","uuid":"o2","timestamp":"2026-03-12T21:21:01Z"}}"#,
                    "\n",
                    r#"{{"type":"progress","data":{{"type":"skill_progress","agentId":"{child_id_short}"}},"parentToolUseID":"{skill_tool_id}","uuid":"o3","timestamp":"2026-03-12T21:21:02Z"}}"#,
                    "\n",
                    r#"{{"type":"user","agentId":"orch001","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{skill_tool_id}","content":"Skill completed."}}]}},"uuid":"o4","timestamp":"2026-03-12T21:22:00Z"}}"#,
                    "\n",
                ),
                skill_tool_id = skill_tool_id,
                child_id_short = "child01",
            ),
        )
        .unwrap();
        std::fs::write(
            sub_dir.join(format!("{orch_id}.meta.json")),
            r#"{"agentType":"general-purpose"}"#,
        )
        .unwrap();

        // Child subagent (spawned by orchestrator's Skill call)
        std::fs::write(
            sub_dir.join(format!("{child_id}.jsonl")),
            concat!(
                r#"{"type":"user","agentId":"child01","message":{"role":"user","content":"Investigate PagerDuty incidents for the past 24 hours"},"uuid":"c1","timestamp":"2026-03-12T21:21:05Z"}"#,
                "\n",
                r#"{"type":"assistant","agentId":"child01","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_bash1","name":"Bash","input":{"command":"pd incident list"}}],"stop_reason":"tool_use","usage":{"input_tokens":100,"output_tokens":20}},"requestId":"req3","uuid":"c2","timestamp":"2026-03-12T21:21:06Z"}"#,
                "\n",
                r#"{"type":"user","agentId":"child01","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_bash1","content":"No incidents found."}]},"uuid":"c3","timestamp":"2026-03-12T21:21:07Z"}"#,
                "\n",
                r#"{"type":"assistant","agentId":"child01","message":{"role":"assistant","content":[{"type":"text","text":"No PagerDuty incidents."}],"stop_reason":"end_turn","usage":{"input_tokens":150,"output_tokens":10}},"requestId":"req4","uuid":"c4","timestamp":"2026-03-12T21:21:08Z"}"#,
                "\n",
            ),
        )
        .unwrap();
        std::fs::write(
            sub_dir.join(format!("{child_id}.meta.json")),
            r#"{"agentType":"general-purpose"}"#,
        )
        .unwrap();

        (dir, main_path.to_string_lossy().to_string())
    }

    #[test]
    fn nested_skill_agents_are_linked_not_orphan() {
        let (_dir, main_path) = setup_nested_skill_session();

        let (classified, _, _) =
            crate::parser::session::read_session_incremental(&main_path, 0).unwrap();
        let mut chunks = crate::parser::chunk::build_chunks(&classified);
        let (mut procs, _color_map) = discover_and_link_all(&main_path, &chunks);

        // Before orphan injection: orchestrator should be orphan, child should be linked.
        let orch = procs.iter().find(|p| p.id == "orch001").unwrap();
        assert!(
            orch.parent_task_id.is_empty(),
            "orchestrator should be orphan (no parent in main session)"
        );

        let child = procs.iter().find(|p| p.id == "child01").unwrap();
        assert_eq!(
            child.parent_task_id, "toolu_skill_pg",
            "child should link to the orchestrator's Skill tool_use_id"
        );
        // Phase 4 should have enriched the description from the Skill tool_summary.
        assert!(
            !child.description.is_empty(),
            "child description should be enriched from the Skill tool call"
        );

        // After orphan injection: only orchestrator becomes orphan item.
        inject_orphan_subagents(&mut chunks, &mut procs);
        let orphan_items: Vec<_> = chunks
            .iter()
            .flat_map(|c| &c.items)
            .filter(|it| it.is_orphan)
            .collect();
        assert_eq!(
            orphan_items.len(),
            1,
            "only the orchestrator should be an orphan item"
        );
        assert_eq!(orphan_items[0].subagent_type, "general-purpose");
    }

    #[test]
    fn nested_skill_agents_appear_in_orchestrator_messages() {
        let (_dir, main_path) = setup_nested_skill_session();

        let (classified, _, _) =
            crate::parser::session::read_session_incremental(&main_path, 0).unwrap();
        let mut chunks = crate::parser::chunk::build_chunks(&classified);
        let (mut procs, color_map) = discover_and_link_all(&main_path, &chunks);
        inject_orphan_subagents(&mut chunks, &mut procs);

        let messages = crate::convert::chunks_to_messages(&chunks, &procs, &color_map);

        // Find the orchestrator orphan item in the messages.
        let orch_item = messages
            .iter()
            .flat_map(|m| &m.items)
            .find(|it| it.agent_id == "orch001")
            .expect("orchestrator item should exist");

        assert!(
            !orch_item.subagent_messages.is_empty(),
            "orchestrator should have subagent_messages"
        );

        // Inside the orchestrator's messages, the Skill ToolCall should link to the child.
        let skill_item = orch_item
            .subagent_messages
            .iter()
            .flat_map(|m| &m.items)
            .find(|it| it.tool_name == "Skill");
        assert!(
            skill_item.is_some(),
            "Skill tool call should exist in orchestrator messages"
        );

        let skill_item = skill_item.unwrap();
        assert_eq!(
            skill_item.agent_id, "child01",
            "Skill item should be linked to child agent"
        );
        assert!(
            !skill_item.subagent_messages.is_empty(),
            "Skill item should have child's subagent_messages"
        );
    }

    #[test]
    fn orphan_subagents_appended_after_existing_items() {
        use chrono::TimeZone;

        let existing_item = DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_id: "toolu_existing".to_string(),
            ..Default::default()
        };

        let mut chunks = vec![Chunk {
            chunk_type: ChunkType::AI,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 12, 21, 20, 0).unwrap(),
            items: vec![existing_item],
            ..Default::default()
        }];

        let mut procs = vec![SubagentProcess {
            id: "orphan-agent".to_string(),
            parent_task_id: String::new(), // orphan: no parent
            subagent_type: "general-purpose".to_string(),
            description: "orphan desc".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 12, 21, 21, 0).unwrap(),
            chunks: vec![Chunk {
                chunk_type: ChunkType::AI,
                ..Default::default()
            }],
            ..Default::default()
        }];

        inject_orphan_subagents(&mut chunks, &mut procs);

        let items = &chunks[0].items;
        assert_eq!(items.len(), 2, "should have existing item + orphan");
        assert_eq!(
            items[0].tool_name, "Bash",
            "existing item should remain first"
        );
        assert!(items[1].is_orphan, "orphan should be appended at the end");
        assert_eq!(items[1].subagent_type, "general-purpose");
    }

    #[test]
    fn first_user_text_skips_non_user_chunks() {
        let chunks = vec![
            Chunk {
                chunk_type: ChunkType::System,
                output: "system text".to_string(),
                ..Default::default()
            },
            make_user_chunk("user prompt"),
        ];
        assert_eq!(first_user_text(&chunks), "user prompt");
    }

    #[test]
    fn orphan_description_extracts_skill_name() {
        let prompt = "Base directory for this skill: /Users/yang/.claude/skills/qa-web-test\n\n# QA Web Testing";
        assert_eq!(orphan_description_from_prompt(prompt), "qa-web-test");
    }

    #[test]
    fn orphan_description_extracts_nested_skill_name() {
        let prompt = "Base directory for this skill: /Users/yang/.claude/skills/forge\n\n# Forge";
        assert_eq!(orphan_description_from_prompt(prompt), "forge");
    }

    #[test]
    fn orphan_description_falls_back_to_first_line() {
        let prompt = "Investigate PagerDuty incidents for the past 24 hours";
        assert_eq!(orphan_description_from_prompt(prompt), prompt);
    }

    #[test]
    fn orphan_description_truncates_long_prompt() {
        let prompt = "A".repeat(120);
        let desc = orphan_description_from_prompt(&prompt);
        assert!(desc.ends_with('…'));
        assert_eq!(desc.chars().count(), 81); // 80 chars + '…'
    }

    #[test]
    fn orphan_description_empty_prompt() {
        assert_eq!(orphan_description_from_prompt(""), "");
    }

    #[test]
    fn orphan_gets_description_from_prompt_when_unlinked() {
        use chrono::TimeZone;

        let mut chunks = vec![Chunk {
            chunk_type: ChunkType::AI,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 12, 21, 20, 0).unwrap(),
            ..Default::default()
        }];

        let mut procs = vec![SubagentProcess {
            id: "skill-agent".to_string(),
            parent_task_id: String::new(),
            subagent_type: "general-purpose".to_string(),
            description: String::new(), // no description from linking
            prompt: "Base directory for this skill: /Users/yang/.claude/skills/qa-web-test\n\nQA content".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 12, 21, 21, 0).unwrap(),
            chunks: vec![Chunk {
                chunk_type: ChunkType::AI,
                ..Default::default()
            }],
            ..Default::default()
        }];

        inject_orphan_subagents(&mut chunks, &mut procs);

        // Orphan appended to existing AI chunk.
        let orphan = &chunks[0].items[0];
        assert!(orphan.is_orphan);
        assert_eq!(
            orphan.subagent_desc, "qa-web-test",
            "orphan should extract skill name from prompt"
        );
    }
}
