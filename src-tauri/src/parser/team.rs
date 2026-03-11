use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use super::chunk::*;
use super::ongoing::is_ongoing;
use super::subagent::SubagentProcess;

/// TeamTask represents a single task in a team's task board.
#[derive(Debug, Clone, Serialize)]
pub struct TeamTask {
    pub id: String,
    pub subject: String,
    pub status: String,
    pub owner: String,
}

/// TeamSnapshot represents the reconstructed state of a team.
#[derive(Debug, Clone, Serialize)]
pub struct TeamSnapshot {
    pub name: String,
    pub description: String,
    pub tasks: Vec<TeamTask>,
    pub members: Vec<String>,
    pub member_colors: HashMap<String, String>,
    pub member_ongoing: HashMap<String, bool>,
    pub deleted: bool,
}

/// Reconstruct team task boards from chunks and worker processes.
pub fn reconstruct_teams(chunks: &[Chunk], workers: &[SubagentProcess]) -> Vec<TeamSnapshot> {
    let mut teams: Vec<TeamSnapshot> = Vec::new();
    let mut active_idx: Option<usize> = None;
    let mut task_counter = 0;

    // Phase 1: Lead chunk events.
    for c in chunks {
        if c.chunk_type != ChunkType::AI {
            continue;
        }
        for item in &c.items {
            match (&item.item_type, item.tool_name.as_str()) {
                (DisplayItemType::ToolCall, "TeamCreate") => {
                    teams.push(team_snapshot_from_create(&item.tool_input));
                    active_idx = Some(teams.len() - 1);
                    task_counter = 0;
                }
                (DisplayItemType::ToolCall, "TaskCreate") if active_idx.is_some() => {
                    task_counter += 1;
                    let task = team_task_from_create(&item.tool_input, task_counter);
                    teams[active_idx.unwrap()].tasks.push(task);
                }
                (DisplayItemType::ToolCall, "TaskUpdate") if active_idx.is_some() => {
                    apply_team_task_update(&item.tool_input, &mut teams[active_idx.unwrap()]);
                }
                (DisplayItemType::ToolCall, "TeamDelete") if active_idx.is_some() => {
                    teams[active_idx.unwrap()].deleted = true;
                    active_idx = None;
                }
                (DisplayItemType::Subagent, _) if is_team_task(item) => {
                    add_team_spawn_member(&item.tool_input, &mut teams);
                }
                _ => {}
            }
        }
    }

    // Phase 2: Worker TaskUpdate events.
    for worker in workers {
        let (agent_name, team_name) = split_worker_id(&worker.id);
        if team_name.is_empty() {
            continue;
        }
        if let Some(team) = find_team_by_name_mut(&mut teams, &team_name) {
            apply_worker_task_updates(&worker.chunks, team, &agent_name);
        }
    }

    // Phase 3: Populate member colors.
    for team in &mut teams {
        team.member_colors = HashMap::new();
        team.member_ongoing = HashMap::new();
    }
    for worker in workers {
        let (agent_name, team_name) = split_worker_id(&worker.id);
        if team_name.is_empty() || worker.teammate_color.is_empty() {
            continue;
        }
        for team in &mut teams {
            if team.name == team_name {
                team.member_colors
                    .insert(agent_name.clone(), worker.teammate_color.clone());
            }
        }
    }

    // Phase 4: Populate member ongoing state.
    for worker in workers {
        let (agent_name, team_name) = split_worker_id(&worker.id);
        if team_name.is_empty() {
            continue;
        }
        if is_ongoing(&worker.chunks) {
            for team in &mut teams {
                if team.name == team_name {
                    team.member_ongoing.insert(agent_name.clone(), true);
                }
            }
        }
    }

    teams
}

fn team_snapshot_from_create(input: &Option<Value>) -> TeamSnapshot {
    let (name, desc) = get_string_fields(input, "team_name", "description");
    TeamSnapshot {
        name,
        description: desc,
        tasks: Vec::new(),
        members: Vec::new(),
        member_colors: HashMap::new(),
        member_ongoing: HashMap::new(),
        deleted: false,
    }
}

fn team_task_from_create(input: &Option<Value>, seq_id: i32) -> TeamTask {
    let subject = get_single_field(input, "subject");
    TeamTask {
        id: seq_id.to_string(),
        subject,
        status: "pending".to_string(),
        owner: String::new(),
    }
}

fn apply_team_task_update(input: &Option<Value>, team: &mut TeamSnapshot) {
    let task_id = get_single_field(input, "taskId");
    if task_id.is_empty() {
        return;
    }
    for task in &mut team.tasks {
        if task.id != task_id {
            continue;
        }
        let status = get_single_field(input, "status");
        if !status.is_empty() {
            task.status = status;
        }
        let owner = get_single_field(input, "owner");
        if !owner.is_empty() {
            task.owner = owner;
        }
        let subject = get_single_field(input, "subject");
        if !subject.is_empty() {
            task.subject = subject;
        }
        return;
    }
}

fn add_team_spawn_member(input: &Option<Value>, teams: &mut [TeamSnapshot]) {
    let (team_name, member_name) = get_string_fields(input, "team_name", "name");
    if team_name.is_empty() || member_name.is_empty() {
        return;
    }
    for team in teams.iter_mut() {
        if team.name != team_name {
            continue;
        }
        if !team.members.contains(&member_name) {
            team.members.push(member_name);
        }
        return;
    }
}

fn apply_worker_task_updates(chunks: &[Chunk], team: &mut TeamSnapshot, worker_name: &str) {
    for c in chunks {
        if c.chunk_type != ChunkType::AI {
            continue;
        }
        for item in &c.items {
            if item.item_type != DisplayItemType::ToolCall || item.tool_name != "TaskUpdate" {
                continue;
            }
            let task_id = get_single_field(&item.tool_input, "taskId");
            if task_id.is_empty() {
                continue;
            }
            for task in &mut team.tasks {
                if task.id != task_id {
                    continue;
                }
                let status = get_single_field(&item.tool_input, "status");
                if !status.is_empty() {
                    task.status = status;
                }
                let owner = get_single_field(&item.tool_input, "owner");
                if !owner.is_empty() {
                    task.owner = owner;
                } else if task.owner.is_empty() {
                    task.owner = worker_name.to_string();
                }
                let subject = get_single_field(&item.tool_input, "subject");
                if !subject.is_empty() {
                    task.subject = subject;
                }
            }
        }
    }
}

fn split_worker_id(id: &str) -> (String, String) {
    if let Some((agent, team)) = id.split_once('@') {
        (agent.to_string(), team.to_string())
    } else {
        (String::new(), String::new())
    }
}

fn find_team_by_name_mut<'a>(teams: &'a mut [TeamSnapshot], name: &str) -> Option<&'a mut TeamSnapshot> {
    teams.iter_mut().find(|t| t.name == name)
}

fn get_single_field(input: &Option<Value>, key: &str) -> String {
    match input {
        Some(Value::Object(map)) => map
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn get_string_fields(input: &Option<Value>, key1: &str, key2: &str) -> (String, String) {
    match input {
        Some(Value::Object(map)) => {
            let v1 = map.get(key1).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let v2 = map.get(key2).and_then(|v| v.as_str()).unwrap_or("").to_string();
            (v1, v2)
        }
        _ => (String::new(), String::new()),
    }
}
