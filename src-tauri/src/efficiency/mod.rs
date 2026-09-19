pub mod cache;
pub mod extract;
pub mod jev;
pub mod redact;
pub mod score;
pub mod settings;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ANALYSIS_VERSION: u32 = 5;
pub const DECISION_SET_VERSION: u32 = 3;
pub const SCORE_FORMULA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyTask {
    pub first_user_message: String,
    pub turns: usize,
    pub duration_ms: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyAction {
    /// Message index in the Trace timeline. Kept stable so findings can jump to it.
    pub index: usize,
    pub tool: String,
    pub category: String,
    pub summary: String,
    pub duration_ms: i64,
    pub error: bool,
    pub repeated_similar_call_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencySignals {
    pub tool_calls: usize,
    pub failed_tool_calls: usize,
    pub repeated_tool_calls: usize,
    pub subagent_count: usize,
    pub context_growth: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyInput {
    pub task: EfficiencyTask,
    pub actions: Vec<EfficiencyAction>,
    pub signals: EfficiencySignals,
    pub selected_excerpts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedEfficiencyPayload {
    pub session_id: String,
    pub session_path: String,
    pub transcript_fingerprint: String,
    pub payload_mode: settings::PayloadMode,
    pub destination: jev::JevDestination,
    pub input: EfficiencyInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JevEfficiencyDecision {
    pub progressing_efficiently: f64,
    pub tool_calls_useful: f64,
    pub redundant_work_present: f64,
    pub excessive_exploration: f64,
    pub likely_thrashing: f64,
    pub effective_recovery: f64,
    #[serde(default)]
    pub token_usage_efficient: f64,
    pub subagents_useful: f64,
    pub likely_task_completed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum EfficiencyFindingType {
    RepeatedWork,
    Thrashing,
    ExcessiveExploration,
    FailedRetries,
    Recovery,
    UsefulSubagent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyFinding {
    #[serde(rename = "type")]
    pub finding_type: EfficiencyFindingType,
    pub probability: f64,
    pub start_message_index: usize,
    pub end_message_index: usize,
    #[serde(default)]
    pub activity_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyMetricEvaluation {
    pub key: String,
    pub label: String,
    pub question: String,
    pub higher_probability_is_better: bool,
    pub probability: f64,
    pub score: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyDimensions {
    pub progress: u8,
    pub tool_use: u8,
    pub focus: u8,
    pub exploration: u8,
    pub recovery: u8,
    #[serde(default)]
    pub token_use: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionEfficiencyAnalysis {
    pub session_id: String,
    pub session_path: String,
    pub score: u8,
    pub dimensions: EfficiencyDimensions,
    #[serde(default)]
    pub metric_evaluations: Vec<EfficiencyMetricEvaluation>,
    pub findings: Vec<EfficiencyFinding>,
    pub decisions: JevEfficiencyDecision,
    pub analyzed_at: String,
    pub analyzed_turns: usize,
    pub transcript_fingerprint: String,
    pub analysis_version: u32,
    pub decision_set_version: u32,
    pub score_formula_version: u32,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EfficiencyJobStatus {
    Queued,
    Preparing,
    Redacting,
    Sending,
    Analysing,
    ProcessingResult,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyAnalysisJob {
    pub analysis_id: String,
    pub session_id: String,
    pub session_path: String,
    pub session_name: String,
    pub status: EfficiencyJobStatus,
    pub progress: Option<u8>,
    pub message: String,
    pub updated_at: String,
    pub error: Option<String>,
    pub score: Option<u8>,
    #[serde(skip)]
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencySummary {
    pub session_path: String,
    pub score: u8,
    pub analyzed_at: String,
    pub stale: bool,
}

#[derive(Default)]
pub struct EfficiencyState {
    pub jobs: std::sync::Mutex<HashMap<String, EfficiencyAnalysisJob>>,
}
