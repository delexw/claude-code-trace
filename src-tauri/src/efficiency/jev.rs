use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

use super::{
    EfficiencyFinding, EfficiencyFindingType, EfficiencyInput, EfficiencyMetricEvaluation,
    JevEfficiencyDecision,
};

const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const JEV_MODEL: &str = "jev-latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const WINDOW_ACTIONS: usize = 6;
const MAX_WINDOWS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JevDestination {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
}

pub fn destination() -> JevDestination {
    JevDestination {
        provider: "TypeSafe AI (Jev)".to_string(),
        endpoint: JEV_ENDPOINT.to_string(),
        model: JEV_MODEL.to_string(),
    }
}

struct BaseMetricDefinition {
    key: &'static str,
    label: &'static str,
    question: &'static str,
    higher_probability_is_better: bool,
}

const BASE_METRICS: [BaseMetricDefinition; 9] = [
    BaseMetricDefinition {
        key: "progressingEfficiently",
        label: "Progress",
        question: "Did the agent make steady, meaningful progress toward the user's task?",
        higher_probability_is_better: true,
    },
    BaseMetricDefinition {
        key: "toolCallsUseful",
        label: "Useful tool calls",
        question: "Were the tool calls useful and proportionate to completing the task?",
        higher_probability_is_better: true,
    },
    BaseMetricDefinition {
        key: "redundantWorkPresent",
        label: "Avoided redundant work",
        question: "Was materially redundant work present? An action is redundant only when it repeats earlier work whose result could not have changed, because nothing relevant was modified in between — for example re-reading an unchanged file, or re-running the same search after no edits. Do NOT count: re-running a command after a change that could alter its result, such as re-running tests or a build after an edit; the same tool applied to a different target or different input; or a retry after an error or interruption. Each entry in state.actions carries repeatedSimilarCallCount, the number of other actions with the identical tool and input; treat that as evidence, not proof, since an identical call can still be legitimate once the state it reads has changed.",
        higher_probability_is_better: false,
    },
    BaseMetricDefinition {
        key: "excessiveExploration",
        label: "Proportionate exploration",
        question: "Was exploration excessive relative to the task?",
        higher_probability_is_better: false,
    },
    BaseMetricDefinition {
        key: "likelyThrashing",
        label: "Avoided thrashing",
        question: "Did the agent cycle among similar actions without meaningful progress?",
        higher_probability_is_better: false,
    },
    BaseMetricDefinition {
        key: "effectiveRecovery",
        label: "Effective recovery",
        question: "When mistakes or failures occurred, did the agent recover effectively?",
        higher_probability_is_better: true,
    },
    BaseMetricDefinition {
        key: "tokenUsageEfficient",
        label: "Efficient token use",
        question: "Was token usage efficient for the work completed? Consider total tokens, context growth, turn count, repeated work, and tool activity. Judge resource efficiency only; do not estimate or consider monetary cost.",
        higher_probability_is_better: true,
    },
    BaseMetricDefinition {
        key: "subagentsUseful",
        label: "Useful subagents",
        question: "If subagents were used, did they add useful independent work? Answer yes when no subagents were needed or used.",
        higher_probability_is_better: true,
    },
    BaseMetricDefinition {
        key: "likelyTaskCompleted",
        label: "Task completion",
        question: "Does the trace indicate that the user's requested task was completed successfully?",
        higher_probability_is_better: true,
    },
];

#[derive(Serialize)]
struct NoulQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: String,
}

#[derive(Serialize)]
struct JevRequest<'a> {
    model: &'static str,
    state: &'a EfficiencyInput,
    questions: HashMap<String, NoulQuestion>,
}

#[derive(Debug, Deserialize)]
struct NoulAnswer {
    noul: f64,
}

#[derive(Debug, Deserialize)]
struct JevResponse {
    answers: HashMap<String, NoulAnswer>,
}

pub struct JevAnalysisResult {
    pub decisions: JevEfficiencyDecision,
    pub metric_evaluations: Vec<EfficiencyMetricEvaluation>,
    pub findings: Vec<EfficiencyFinding>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())
}

fn questions(input: &EfficiencyInput) -> HashMap<String, NoulQuestion> {
    let mut questions = BASE_METRICS
        .iter()
        .map(|metric| {
            (
                metric.key.to_string(),
                NoulQuestion {
                    question_type: "noul",
                    instructions: metric.question.to_string(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for (window, actions) in input
        .actions
        .chunks(WINDOW_ACTIONS)
        .take(MAX_WINDOWS)
        .enumerate()
    {
        let first_action_position = window * WINDOW_ACTIONS;
        let last_action_position = first_action_position + actions.len() - 1;
        for (kind, prompt) in [
            (
                "repeated",
                "Does this action window contain materially redundant work — an action repeating earlier work whose result could not have changed, because nothing relevant was modified in between? Do not count a re-run after a change that could alter the result, the same tool on a different target or input, or a retry after an error.",
            ),
            (
                "thrashing",
                "Does this action window show thrashing without progress?",
            ),
            (
                "exploration",
                "Does this action window show excessive exploration?",
            ),
            (
                "recovery",
                "Does this action window show effective recovery after an error or wrong path?",
            ),
            (
                "subagent",
                "Does this action window show a subagent contributing useful independent work?",
            ),
        ] {
            questions.insert(
                format!("window_{window}_{kind}"),
                NoulQuestion {
                    question_type: "noul",
                    instructions: format!(
                        "Evaluate only state.actions at zero-based positions {first_action_position} through {last_action_position}. Ignore every action outside that range. {prompt}"
                    ),
                },
            );
        }
    }
    questions
}

fn probability(answers: &HashMap<String, NoulAnswer>, key: &str) -> Result<f64, String> {
    let value = answers
        .get(key)
        .ok_or_else(|| format!("Jev response omitted decision {key}"))?
        .noul;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("Jev returned an invalid probability for {key}"));
    }
    Ok(value)
}

fn metric_evaluations(
    answers: &HashMap<String, NoulAnswer>,
) -> Result<Vec<EfficiencyMetricEvaluation>, String> {
    BASE_METRICS
        .iter()
        .map(|metric| {
            let probability = probability(answers, metric.key)?;
            let displayed_probability = if metric.higher_probability_is_better {
                probability
            } else {
                1.0 - probability
            };
            Ok(EfficiencyMetricEvaluation {
                key: metric.key.to_string(),
                label: metric.label.to_string(),
                question: metric.question.to_string(),
                higher_probability_is_better: metric.higher_probability_is_better,
                probability,
                score: (displayed_probability.clamp(0.0, 1.0) * 100.0).round() as u8,
            })
        })
        .collect()
}

fn findings(
    input: &EfficiencyInput,
    answers: &HashMap<String, NoulAnswer>,
) -> Vec<EfficiencyFinding> {
    let mut findings = Vec::new();
    for (window, actions) in input
        .actions
        .chunks(WINDOW_ACTIONS)
        .take(MAX_WINDOWS)
        .enumerate()
    {
        let Some(first) = actions.first() else {
            continue;
        };
        let Some(last) = actions.last() else { continue };
        for (suffix, finding_type) in [
            ("repeated", EfficiencyFindingType::RepeatedWork),
            ("thrashing", EfficiencyFindingType::Thrashing),
            ("exploration", EfficiencyFindingType::ExcessiveExploration),
            ("recovery", EfficiencyFindingType::Recovery),
            ("subagent", EfficiencyFindingType::UsefulSubagent),
        ] {
            let Some(answer) = answers.get(&format!("window_{window}_{suffix}")) else {
                continue;
            };
            if answer.noul >= 0.65 {
                findings.push(EfficiencyFinding {
                    finding_type,
                    probability: answer.noul,
                    start_message_index: first.index,
                    end_message_index: last.index,
                    activity_summary: String::new(),
                });
            }
        }
        let failed = actions.iter().filter(|action| action.error).count();
        if failed >= 2 {
            findings.push(EfficiencyFinding {
                finding_type: EfficiencyFindingType::FailedRetries,
                probability: (0.6 + failed as f64 * 0.1).min(0.95),
                start_message_index: first.index,
                end_message_index: last.index,
                activity_summary: String::new(),
            });
        }
    }
    let mut findings = merge_overlapping_findings(findings);
    for finding in &mut findings {
        let related_actions = input
            .actions
            .iter()
            .filter(|action| {
                action.index >= finding.start_message_index
                    && action.index <= finding.end_message_index
            })
            .collect::<Vec<_>>();
        finding.activity_summary = describe_activity(&related_actions);
    }
    findings
}

fn describe_activity(actions: &[&super::EfficiencyAction]) -> String {
    let mut tools = Vec::new();
    for action in actions {
        let tool = action.tool.trim();
        if !tool.is_empty() && !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    let displayed_tools = tools.iter().take(3).copied().collect::<Vec<_>>();
    let hidden_tool_count = tools.len().saturating_sub(displayed_tools.len());
    let call_label = if actions.len() == 1 {
        "1 tool call".to_string()
    } else {
        format!("{} tool calls", actions.len())
    };
    if displayed_tools.is_empty() {
        return call_label;
    }
    let mut summary = format!("{call_label} · {}", displayed_tools.join(", "));
    if hidden_tool_count > 0 {
        summary.push_str(&format!(" +{hidden_tool_count} more"));
    }
    let failed_call_count = actions.iter().filter(|action| action.error).count();
    if failed_call_count > 0 {
        summary.push_str(&format!(" · {failed_call_count} failed"));
    }
    summary
}

fn finding_type_rank(finding_type: &EfficiencyFindingType) -> u8 {
    match finding_type {
        EfficiencyFindingType::RepeatedWork => 0,
        EfficiencyFindingType::Thrashing => 1,
        EfficiencyFindingType::ExcessiveExploration => 2,
        EfficiencyFindingType::FailedRetries => 3,
        EfficiencyFindingType::Recovery => 4,
        EfficiencyFindingType::UsefulSubagent => 5,
    }
}

fn merge_overlapping_findings(mut findings: Vec<EfficiencyFinding>) -> Vec<EfficiencyFinding> {
    findings.sort_by_key(|finding| {
        (
            finding_type_rank(&finding.finding_type),
            finding.start_message_index,
            finding.end_message_index,
        )
    });
    let mut merged: Vec<EfficiencyFinding> = Vec::with_capacity(findings.len());
    for finding in findings {
        if let Some(previous) = merged.last_mut() {
            if previous.finding_type == finding.finding_type
                && finding.start_message_index <= previous.end_message_index
            {
                previous.end_message_index =
                    previous.end_message_index.max(finding.end_message_index);
                previous.probability = previous.probability.max(finding.probability);
                continue;
            }
        }
        merged.push(finding);
    }
    merged.sort_by_key(|finding| finding.start_message_index);
    merged
}

fn parse(input: &EfficiencyInput, response: JevResponse) -> Result<JevAnalysisResult, String> {
    let answers = response.answers;
    let metric_evaluations = metric_evaluations(&answers)?;
    let decisions = JevEfficiencyDecision {
        progressing_efficiently: probability(&answers, "progressingEfficiently")?,
        tool_calls_useful: probability(&answers, "toolCallsUseful")?,
        redundant_work_present: probability(&answers, "redundantWorkPresent")?,
        excessive_exploration: probability(&answers, "excessiveExploration")?,
        likely_thrashing: probability(&answers, "likelyThrashing")?,
        effective_recovery: probability(&answers, "effectiveRecovery")?,
        token_usage_efficient: probability(&answers, "tokenUsageEfficient")?,
        subagents_useful: probability(&answers, "subagentsUseful")?,
        likely_task_completed: probability(&answers, "likelyTaskCompleted")?,
    };
    Ok(JevAnalysisResult {
        decisions,
        metric_evaluations,
        findings: findings(input, &answers),
    })
}

fn response_error(status: StatusCode) -> String {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        "Authentication failed".to_string()
    } else {
        format!("Jev request failed ({status})")
    }
}

pub async fn analyse(api_key: &str, input: &EfficiencyInput) -> Result<JevAnalysisResult, String> {
    let response = client()?
        .post(JEV_ENDPOINT)
        .bearer_auth(api_key)
        .json(&JevRequest {
            model: JEV_MODEL,
            state: input,
            questions: questions(input),
        })
        .send()
        .await
        .map_err(|_| "Connection failed".to_string())?;
    if !response.status().is_success() {
        return Err(response_error(response.status()));
    }
    let response = response
        .json::<JevResponse>()
        .await
        .map_err(|_| "Jev returned an invalid response".to_string())?;
    parse(input, response)
}

pub async fn test_connection(api_key: &str) -> Result<(), String> {
    let response = client()?
        .post(JEV_ENDPOINT)
        .bearer_auth(api_key)
        .json(&json!({
            "model": JEV_MODEL,
            "state": "Connection test. No session data is included.",
            "questions": {
                "reachable": { "type": "noul", "instructions": "Is this a connection test?" }
            }
        }))
        .send()
        .await
        .map_err(|_| "Connection failed".to_string())?;
    if !response.status().is_success() {
        return Err(response_error(response.status()));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| "Jev returned an invalid response".to_string())?;
    if value
        .pointer("/answers/reachable/noul")
        .and_then(Value::as_f64)
        .is_none()
    {
        return Err("Jev returned an invalid response".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::efficiency::{EfficiencyAction, EfficiencySignals, EfficiencyTask};

    #[test]
    fn disclosed_destination_matches_the_actual_jev_request() {
        assert_eq!(
            destination(),
            JevDestination {
                provider: "TypeSafe AI (Jev)".to_string(),
                endpoint: JEV_ENDPOINT.to_string(),
                model: JEV_MODEL.to_string(),
            }
        );
    }

    fn empty_input() -> EfficiencyInput {
        EfficiencyInput {
            task: EfficiencyTask {
                first_user_message: "task".into(),
                turns: 1,
                duration_ms: 0,
                total_tokens: 0,
            },
            actions: vec![],
            signals: EfficiencySignals {
                tool_calls: 0,
                failed_tool_calls: 0,
                repeated_tool_calls: 0,
                subagent_count: 0,
                context_growth: 0,
            },
            selected_excerpts: vec![],
        }
    }

    #[test]
    fn the_redundant_work_question_defines_what_counts_and_what_does_not() {
        // A bare "was repeated work present?" left the model to invent the rule and
        // left the dashboard label unexplained. Both the base metric and the window
        // question must state the test and name the excluded cases.
        let metric = BASE_METRICS
            .iter()
            .find(|m| m.key == "redundantWorkPresent")
            .expect("metric exists");
        for required in [
            "could not have changed",
            "Do NOT count",
            "retry",
            "different target",
        ] {
            assert!(
                metric.question.contains(required),
                "base question must state {required:?}"
            );
        }
        assert_eq!(metric.label, "Avoided redundant work");

        let mut input = empty_input();
        input.actions = vec![EfficiencyAction {
            index: 0,
            tool: "Bash".into(),
            category: "Bash".into(),
            summary: "cargo test".into(),
            duration_ms: 1,
            error: false,
            repeated_similar_call_count: 0,
        }];
        let window = &questions(&input)["window_0_repeated"].instructions;
        assert!(window.contains("could not have changed"));
        assert!(window.contains("retry after an error"));
    }

    #[test]
    fn rejects_missing_or_out_of_range_probabilities() {
        assert!(probability(&HashMap::new(), "missing").is_err());
        let answers = HashMap::from([("bad".into(), NoulAnswer { noul: 1.1 })]);
        assert!(probability(&answers, "bad").is_err());
    }

    #[test]
    fn base_request_contains_all_narrow_decisions() {
        let questions = questions(&empty_input());
        for metric in &BASE_METRICS {
            assert_eq!(questions[metric.key].instructions, metric.question);
        }
    }

    #[test]
    fn dashboard_metrics_are_generated_from_the_same_definitions_as_jev_questions() {
        let answers = BASE_METRICS
            .iter()
            .map(|metric| (metric.key.to_string(), NoulAnswer { noul: 0.25 }))
            .collect::<HashMap<_, _>>();
        let evaluations = metric_evaluations(&answers).unwrap();

        assert_eq!(evaluations.len(), BASE_METRICS.len());
        for (evaluation, definition) in evaluations.iter().zip(BASE_METRICS.iter()) {
            assert_eq!(evaluation.key, definition.key);
            assert_eq!(evaluation.label, definition.label);
            assert_eq!(evaluation.question, definition.question);
            assert_eq!(
                evaluation.score,
                if definition.higher_probability_is_better {
                    25
                } else {
                    75
                }
            );
        }
    }

    #[test]
    fn request_includes_token_usage_but_no_monetary_cost() {
        let mut input = empty_input();
        input.task.total_tokens = 12_345;
        input.signals.context_growth = 4_321;
        let request = serde_json::to_value(JevRequest {
            model: JEV_MODEL,
            state: &input,
            questions: questions(&input),
        })
        .unwrap();

        assert_eq!(
            request.pointer("/state/task/totalTokens"),
            Some(&json!(12_345))
        );
        assert_eq!(
            request.pointer("/state/signals/contextGrowth"),
            Some(&json!(4_321))
        );
        assert!(request.pointer("/questions/tokenUsageEfficient").is_some());
        assert!(!request["state"].to_string().to_lowercase().contains("cost"));
    }

    #[test]
    fn window_questions_identify_the_exact_shared_state_actions_to_evaluate() {
        let mut input = empty_input();
        input.actions = (0..8)
            .map(|index| EfficiencyAction {
                index,
                tool: "Read".into(),
                category: "Read".into(),
                summary: format!("file-{index}"),
                duration_ms: 1,
                error: false,
                repeated_similar_call_count: 0,
            })
            .collect();

        let questions = questions(&input);
        assert!(questions["window_0_recovery"]
            .instructions
            .contains("state.actions at zero-based positions 0 through 5"));
        assert!(questions["window_1_recovery"]
            .instructions
            .contains("state.actions at zero-based positions 6 through 7"));
    }

    #[test]
    fn overlapping_window_answers_become_one_finding() {
        let mut input = empty_input();
        input.actions = (0..15)
            .map(|action_position| EfficiencyAction {
                index: if action_position < 13 { 2 } else { 4 },
                tool: "Read".into(),
                category: "Read".into(),
                summary: format!("file-{action_position}"),
                duration_ms: 1,
                error: false,
                repeated_similar_call_count: 0,
            })
            .collect();
        let answers = HashMap::from([
            ("window_0_recovery".into(), NoulAnswer { noul: 0.75 }),
            ("window_1_recovery".into(), NoulAnswer { noul: 0.76 }),
            ("window_2_recovery".into(), NoulAnswer { noul: 0.78 }),
        ]);

        assert_eq!(
            findings(&input, &answers),
            vec![EfficiencyFinding {
                finding_type: EfficiencyFindingType::Recovery,
                probability: 0.78,
                start_message_index: 2,
                end_message_index: 4,
                activity_summary: "15 tool calls · Read".into(),
            }]
        );
    }
}
