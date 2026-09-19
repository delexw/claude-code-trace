use std::path::Path;
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "desktop")]
use tauri::{Emitter, State};

use crate::efficiency::settings::{
    AnalyticsConfiguration, AnalyticsSettingsResponse, PayloadMode, RecommendationProvider,
};
use crate::efficiency::{EfficiencyAnalysisJob, EfficiencyJobStatus, PreparedEfficiencyPayload};
#[cfg(feature = "desktop")]
use crate::efficiency::{EfficiencySummary, SessionEfficiencyAnalysis};
use crate::state::AppState;
use crate::AppHandle;

const UPDATE_EVENT: &str = "efficiency-analysis-update";

fn session_id(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session")
        .to_string()
}

pub fn get_analytics_settings_impl() -> Result<AnalyticsSettingsResponse, String> {
    crate::efficiency::settings::response()
}

pub fn set_analytics_settings_impl(
    default_payload_mode: PayloadMode,
    recommendation_provider: RecommendationProvider,
) -> Result<AnalyticsSettingsResponse, String> {
    crate::efficiency::settings::ensure_provider_supported(&recommendation_provider)?;
    crate::efficiency::settings::save(&AnalyticsConfiguration {
        default_payload_mode,
        recommendation_provider,
    })?;
    get_analytics_settings_impl()
}

pub fn prepare_session_efficiency_payload_impl(
    path: &str,
    mode: Option<PayloadMode>,
) -> Result<PreparedEfficiencyPayload, String> {
    let mode = mode.unwrap_or_else(|| crate::efficiency::settings::load().default_payload_mode);
    let session =
        crate::session_load::build_session(path, crate::session_load::TimeFilter::default())?;
    let input = crate::efficiency::extract::extract_input(
        &session.messages,
        session.session_totals.total_tokens,
        mode,
    );
    let input = crate::efficiency::redact::redact_input(input);
    Ok(PreparedEfficiencyPayload {
        session_id: session_id(path),
        session_path: path.to_string(),
        transcript_fingerprint: crate::efficiency::cache::transcript_fingerprint(path)?,
        payload_mode: mode,
        destination: crate::efficiency::jev::destination(),
        input,
    })
}

fn emit_update(state: &AppState, app: &Option<AppHandle>, job: &EfficiencyAnalysisJob) {
    if let Ok(json) = serde_json::to_string(job) {
        state.broadcast(UPDATE_EVENT, &json);
    }
    emit_desktop(app, job);
}

#[cfg(feature = "desktop")]
fn emit_desktop(app: &Option<AppHandle>, job: &EfficiencyAnalysisJob) {
    if let Some(app) = app {
        let _ = app.emit(UPDATE_EVENT, job.clone());
    }
}

#[cfg(not(feature = "desktop"))]
fn emit_desktop(_app: &Option<AppHandle>, _job: &EfficiencyAnalysisJob) {}

fn update_job(
    state: &AppState,
    app: &Option<AppHandle>,
    analysis_id: &str,
    status: EfficiencyJobStatus,
    progress: Option<u8>,
    message: &str,
) -> bool {
    let updated = {
        let Ok(mut jobs) = state.efficiency.jobs.lock() else {
            return false;
        };
        let Some(job) = jobs.get_mut(analysis_id) else {
            return false;
        };
        if job.cancelled {
            job.status = EfficiencyJobStatus::Cancelled;
            job.progress = None;
            job.message = "Analysis cancelled".to_string();
        } else {
            job.status = status;
            job.progress = progress;
            job.message = message.to_string();
        }
        job.updated_at = chrono::Utc::now().to_rfc3339();
        job.clone()
    };
    emit_update(state, app, &updated);
    !updated.cancelled
}

fn insert_latest_session_job(
    jobs: &mut std::collections::HashMap<String, EfficiencyAnalysisJob>,
    job: EfficiencyAnalysisJob,
) {
    jobs.retain(|_, existing| existing.session_path != job.session_path);
    jobs.insert(job.analysis_id.clone(), job);
}

pub fn start_session_efficiency_analysis_impl(
    state: Arc<AppState>,
    app: Option<AppHandle>,
    path: String,
    payload: PreparedEfficiencyPayload,
) -> Result<EfficiencyAnalysisJob, String> {
    if payload.session_path != path {
        return Err("Prepared payload does not match the selected session".to_string());
    }
    // Resolve before creating the job: a missing key opens the dedicated settings flow,
    // never a privacy modal followed by an inevitably failed background job.
    let api_key =
        crate::credentials::api_tokens::resolve(crate::credentials::api_tokens::ApiToken::Jev)?
            .ok_or_else(|| "Jev API key required".to_string())?;
    let analysis_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let job = EfficiencyAnalysisJob {
        analysis_id: analysis_id.clone(),
        session_id: payload.session_id.clone(),
        session_path: path,
        session_name: payload.input.task.first_user_message.clone(),
        status: EfficiencyJobStatus::Queued,
        progress: Some(0),
        message: "Queued".to_string(),
        updated_at: now,
        error: None,
        score: None,
        cancelled: false,
    };
    {
        let mut jobs = state
            .efficiency
            .jobs
            .lock()
            .map_err(|error| error.to_string())?;
        insert_latest_session_job(&mut jobs, job.clone());
    }
    emit_update(&state, &app, &job);

    tokio::spawn(async move {
        if !update_job(
            &state,
            &app,
            &analysis_id,
            EfficiencyJobStatus::Preparing,
            Some(10),
            "Preparing payload",
        ) {
            return;
        }
        if !update_job(
            &state,
            &app,
            &analysis_id,
            EfficiencyJobStatus::Redacting,
            Some(25),
            "Payload redacted locally",
        ) {
            return;
        }
        if !update_job(
            &state,
            &app,
            &analysis_id,
            EfficiencyJobStatus::Sending,
            Some(35),
            "Sending to Jev",
        ) {
            return;
        }
        if !update_job(
            &state,
            &app,
            &analysis_id,
            EfficiencyJobStatus::Analysing,
            Some(60),
            "Analysing session behaviour",
        ) {
            return;
        }
        match crate::efficiency::jev::analyse(&api_key, &payload.input).await {
            Ok(result) => {
                if !update_job(
                    &state,
                    &app,
                    &analysis_id,
                    EfficiencyJobStatus::ProcessingResult,
                    Some(90),
                    "Processing result",
                ) {
                    return;
                }
                let analysis = crate::efficiency::score::build_analysis(
                    payload.session_id,
                    payload.session_path,
                    payload.transcript_fingerprint,
                    payload.input.task.turns,
                    result.decisions,
                    result.metric_evaluations,
                    result.findings,
                );
                let updated = {
                    let Ok(mut jobs) = state.efficiency.jobs.lock() else {
                        return;
                    };
                    let Some(job) = jobs.get_mut(&analysis_id) else {
                        return;
                    };
                    if job.cancelled {
                        job.status = EfficiencyJobStatus::Cancelled;
                        job.message = "Analysis cancelled".to_string();
                    } else {
                        // Keep the current job lock through the small cache write. A newer
                        // re-analysis must replace this job before an older result can write,
                        // or wait and then become the final writer itself.
                        match crate::efficiency::cache::write(&analysis) {
                            Ok(()) => {
                                job.status = EfficiencyJobStatus::Completed;
                                job.progress = Some(100);
                                job.message = "Analysis complete".to_string();
                                job.score = Some(analysis.score);
                            }
                            Err(error) => {
                                job.status = EfficiencyJobStatus::Failed;
                                job.message = "Could not save analysis".to_string();
                                job.error = Some(error);
                            }
                        }
                    }
                    job.updated_at = chrono::Utc::now().to_rfc3339();
                    job.clone()
                };
                emit_update(&state, &app, &updated);
            }
            Err(error) => {
                let updated = {
                    let Ok(mut jobs) = state.efficiency.jobs.lock() else {
                        return;
                    };
                    let Some(job) = jobs.get_mut(&analysis_id) else {
                        return;
                    };
                    job.status = if job.cancelled {
                        EfficiencyJobStatus::Cancelled
                    } else {
                        EfficiencyJobStatus::Failed
                    };
                    job.progress = None;
                    job.message = if job.cancelled {
                        "Analysis cancelled"
                    } else {
                        "Efficiency analysis failed"
                    }
                    .to_string();
                    if !job.cancelled {
                        job.error = Some(error);
                    }
                    job.updated_at = chrono::Utc::now().to_rfc3339();
                    job.clone()
                };
                emit_update(&state, &app, &updated);
            }
        }
    });
    Ok(job)
}

pub fn list_efficiency_analysis_jobs_impl(
    state: &AppState,
) -> Result<Vec<EfficiencyAnalysisJob>, String> {
    Ok(state
        .efficiency
        .jobs
        .lock()
        .map_err(|error| error.to_string())?
        .values()
        .cloned()
        .collect())
}

pub fn cancel_efficiency_analysis_impl(state: &AppState, analysis_id: &str) -> Result<(), String> {
    let mut jobs = state
        .efficiency
        .jobs
        .lock()
        .map_err(|error| error.to_string())?;
    let job = jobs
        .get_mut(analysis_id)
        .ok_or_else(|| "Analysis job not found".to_string())?;
    job.cancelled = true;
    job.status = EfficiencyJobStatus::Cancelled;
    job.progress = None;
    job.message = "Analysis cancelled".to_string();
    job.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(())
}

pub async fn test_recommendation_provider_impl(
    provider: RecommendationProvider,
) -> Result<(), String> {
    crate::efficiency::settings::ensure_provider_supported(&provider)?;
    match provider {
        RecommendationProvider::CodexSubscription { model } => {
            let mut command = tokio::process::Command::new("codex");
            command.args(["exec", "--skip-git-repo-check"]);
            if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
                command.args(["--model", &model]);
            }
            command.arg("Return exactly this JSON object and nothing else: {\"connected\":true}");
            run_cli_connection_test(command, "Codex").await
        }
        RecommendationProvider::ClaudeCodeSubscription { model } => {
            let mut command = tokio::process::Command::new("claude");
            command.args([
                "-p",
                "Return exactly this JSON object and nothing else: {\"connected\":true}",
                "--output-format",
                "json",
            ]);
            if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
                command.args(["--model", &model]);
            }
            run_cli_connection_test(command, "Claude Code").await
        }
        RecommendationProvider::OpenaiCompatible {
            base_url, model, ..
        } => {
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            let mut request = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()
                .map_err(|error| error.to_string())?
                .post(url)
                .json(&serde_json::json!({
                    "model": model,
                    "messages": [{ "role": "user", "content": "Return JSON: {\"connected\":true}" }],
                    "max_tokens": 20,
                    "response_format": { "type": "json_object" }
                }));
            if let Some(key) = crate::credentials::api_tokens::resolve(
                crate::credentials::api_tokens::ApiToken::RecommendationProvider,
            )? {
                request = request.bearer_auth(key);
            }
            let response = request
                .send()
                .await
                .map_err(|_| "Connection failed".to_string())?;
            response.status().is_success().then_some(()).ok_or_else(|| {
                if response.status().as_u16() == 401 {
                    "Authentication failed".to_string()
                } else {
                    format!("Provider request failed ({})", response.status())
                }
            })
        }
    }
}

async fn run_cli_connection_test(
    mut command: tokio::process::Command,
    cli_name: &str,
) -> Result<(), String> {
    command.stdin(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .map_err(|_| format!("{cli_name} connection test timed out"))?
        .map_err(|error| cli_start_failure_message(cli_name, &error))?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| cli_failure_message(cli_name, &output))
}

fn cli_failure_message(cli_name: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.lines().rev().find_map(|line| {
        let line = line.trim();
        let payload = line.strip_prefix("ERROR: ")?;
        serde_json::from_str::<serde_json::Value>(payload)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")?
                    .as_str()
                    .map(str::to_string)
            })
    });
    match detail {
        Some(detail) => {
            let message = format!("{cli_name} connection test failed: {detail}");
            append_upgrade_guidance(cli_name, message, &detail)
        }
        None => format!(
            "{cli_name} connection test failed. Run `{}` in a terminal for details.",
            if cli_name == "Codex" {
                "codex login status"
            } else {
                "claude auth status"
            }
        ),
    }
}

fn cli_start_failure_message(cli_name: &str, error: &std::io::Error) -> String {
    let message = format!("Could not start {cli_name}: {error}");
    if error.kind() == std::io::ErrorKind::NotFound {
        format!(
            "{message}. Install or upgrade {cli_name} to the latest version with `{}`, then restart Claude Code Trace.",
            cli_upgrade_command(cli_name)
        )
    } else {
        message
    }
}

fn append_upgrade_guidance(cli_name: &str, message: String, detail: &str) -> String {
    let normalized = detail.to_ascii_lowercase();
    let needs_upgrade = normalized.contains("upgrade")
        || normalized.contains("outdated")
        || normalized.contains("newer version")
        || (normalized.contains("update") && normalized.contains("version"));
    if !needs_upgrade {
        return message;
    }
    format!(
        "{message} To upgrade {cli_name}, run `{}`, then restart Claude Code Trace.",
        cli_upgrade_command(cli_name)
    )
}

fn cli_upgrade_command(cli_name: &str) -> &'static str {
    if cli_name == "Codex" {
        "npm install --global @openai/codex@latest"
    } else {
        "claude update"
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_analytics_settings() -> Result<AnalyticsSettingsResponse, String> {
    get_analytics_settings_impl()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_analytics_settings(
    default_payload_mode: PayloadMode,
    recommendation_provider: RecommendationProvider,
) -> Result<AnalyticsSettingsResponse, String> {
    set_analytics_settings_impl(default_payload_mode, recommendation_provider)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_jev_api_key(key: String) -> Result<AnalyticsSettingsResponse, String> {
    crate::credentials::api_tokens::store(crate::credentials::api_tokens::ApiToken::Jev, &key)?;
    get_analytics_settings_impl()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn clear_jev_api_key() -> Result<AnalyticsSettingsResponse, String> {
    crate::credentials::api_tokens::delete(crate::credentials::api_tokens::ApiToken::Jev)?;
    get_analytics_settings_impl()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn test_jev_connection() -> Result<(), String> {
    let key =
        crate::credentials::api_tokens::resolve(crate::credentials::api_tokens::ApiToken::Jev)?
            .ok_or_else(|| "Jev API key required".to_string())?;
    crate::efficiency::jev::test_connection(&key).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_recommendation_provider_api_key(key: String) -> Result<(), String> {
    crate::credentials::api_tokens::store(
        crate::credentials::api_tokens::ApiToken::RecommendationProvider,
        &key,
    )
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn clear_recommendation_provider_api_key() -> Result<(), String> {
    crate::credentials::api_tokens::delete(
        crate::credentials::api_tokens::ApiToken::RecommendationProvider,
    )
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn test_recommendation_provider(provider: RecommendationProvider) -> Result<(), String> {
    test_recommendation_provider_impl(provider).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn prepare_session_efficiency_payload(
    path: String,
    payload_mode: Option<PayloadMode>,
) -> Result<PreparedEfficiencyPayload, String> {
    prepare_session_efficiency_payload_impl(&path, payload_mode)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn start_session_efficiency_analysis(
    path: String,
    payload: PreparedEfficiencyPayload,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<EfficiencyAnalysisJob, String> {
    start_session_efficiency_analysis_impl(state.inner().clone(), Some(app), path, payload)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_efficiency_analysis_jobs(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<EfficiencyAnalysisJob>, String> {
    list_efficiency_analysis_jobs_impl(&state)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_session_efficiency(
    path: String,
) -> Result<Option<SessionEfficiencyAnalysis>, String> {
    crate::efficiency::cache::read(&path)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_efficiency_summaries() -> Result<Vec<EfficiencySummary>, String> {
    crate::efficiency::cache::list_summaries()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_session_efficiency(path: String) -> Result<(), String> {
    crate::efficiency::cache::delete(&path)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn cancel_efficiency_analysis(
    analysis_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    cancel_efficiency_analysis_impl(&state, &analysis_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn failed_output(stderr: &str) -> Output {
        std::process::Command::new("sh")
            .args(["-c", &format!("printf '%s' '{stderr}' >&2; exit 1")])
            .output()
            .unwrap()
    }

    fn job(analysis_id: &str, session_path: &str) -> EfficiencyAnalysisJob {
        EfficiencyAnalysisJob {
            analysis_id: analysis_id.to_string(),
            session_id: "session".to_string(),
            session_path: session_path.to_string(),
            session_name: "Session".to_string(),
            status: EfficiencyJobStatus::Queued,
            progress: Some(0),
            message: "Queued".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            error: None,
            score: None,
            cancelled: false,
        }
    }

    #[test]
    fn a_new_analysis_replaces_the_previous_job_for_the_same_session() {
        let mut jobs = std::collections::HashMap::new();
        insert_latest_session_job(&mut jobs, job("old", "/sessions/a.jsonl"));
        insert_latest_session_job(&mut jobs, job("other", "/sessions/b.jsonl"));
        insert_latest_session_job(&mut jobs, job("new", "/sessions/a.jsonl"));

        assert_eq!(jobs.len(), 2);
        assert!(!jobs.contains_key("old"));
        assert!(jobs.contains_key("new"));
        assert!(jobs.contains_key("other"));
    }

    #[cfg(unix)]
    #[test]
    fn cli_failure_surfaces_the_actual_provider_message() {
        let output = failed_output(
            r#"ERROR: {"type":"error","status":400,"error":{"message":"Please upgrade to the latest Codex CLI."}}"#,
        );

        assert_eq!(
            cli_failure_message("Codex", &output),
            "Codex connection test failed: Please upgrade to the latest Codex CLI. To upgrade Codex, run `npm install --global @openai/codex@latest`, then restart Claude Code Trace."
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_failure_includes_its_native_upgrade_command() {
        let output = failed_output(
            r#"ERROR: {"type":"error","status":400,"error":{"message":"This model requires a newer version of Claude Code."}}"#,
        );

        assert_eq!(
            cli_failure_message("Claude Code", &output),
            "Claude Code connection test failed: This model requires a newer version of Claude Code. To upgrade Claude Code, run `claude update`, then restart Claude Code Trace."
        );
    }

    #[test]
    fn a_missing_cli_includes_install_or_upgrade_guidance() {
        let error = std::io::Error::from(std::io::ErrorKind::NotFound);
        let message = cli_start_failure_message("Codex", &error);

        assert!(message.starts_with("Could not start Codex:"));
        assert!(message.contains(
            "Install or upgrade Codex to the latest version with `npm install --global @openai/codex@latest`, then restart Claude Code Trace."
        ));
    }

    #[cfg(unix)]
    #[test]
    fn cli_failure_without_structured_detail_gives_a_diagnostic_command() {
        let output = failed_output("unstructured failure");

        assert_eq!(
            cli_failure_message("Codex", &output),
            "Codex connection test failed. Run `codex login status` in a terminal for details."
        );
    }
}
