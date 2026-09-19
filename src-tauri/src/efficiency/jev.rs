use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

use super::{EfficiencyFinding, EfficiencyFindingType, EfficiencyInput, JevEfficiencyDecision};

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

const BASE_QUESTIONS: [(&str, &str); 9] = [
    ("progressingEfficiently", "Did the agent make steady, meaningful progress toward the user's task?"),
    ("toolCallsUseful", "Were the tool calls useful and proportionate to completing the task?"),
    ("redundantWorkPresent", "Was materially redundant or repeated work present?"),
    ("excessiveExploration", "Was exploration excessive relative to the task?"),
    ("likelyThrashing", "Did the agent cycle among similar actions without meaningful progress?"),
    ("effectiveRecovery", "When mistakes or failures occurred, did the agent recover effectively?"),
    ("tokenUsageEfficient", "Was token usage efficient for the work completed? Consider total tokens, context growth, turn count, repeated work, and tool activity. Judge resource efficiency only; do not estimate or consider monetary cost."),
    ("subagentsUseful", "If subagents were used, did they add useful independent work? Answer yes when no subagents were needed or used."),
    ("likelyTaskCompleted", "Does the trace indicate that the user's requested task was completed successfully?"),
];

#[derive(Serialize)]
struct NoulQuestion<'a> {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: &'a str,
}

#[derive(Serialize)]
struct JevRequest<'a> {
    model: &'static str,
    state: &'a EfficiencyInput,
    questions: HashMap<String, NoulQuestion<'a>>,
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
    pub findings: Vec<EfficiencyFinding>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())
}

fn questions(input: &EfficiencyInput) -> HashMap<String, NoulQuestion<'static>> {
    let mut questions = BASE_QUESTIONS
        .into_iter()
        .map(|(id, instructions)| {
            (
                id.to_string(),
                NoulQuestion {
                    question_type: "noul",
                    instructions,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for window in 0..input
        .actions
        .len()
        .div_ceil(WINDOW_ACTIONS)
        .min(MAX_WINDOWS)
    {
        for (kind, prompt) in [
            (
                "repeated",
                "Does this action window contain materially repeated work?",
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
                    instructions: prompt,
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
            });
        }
    }
    findings.sort_by_key(|finding| finding.start_message_index);
    findings
}

fn parse(input: &EfficiencyInput, response: JevResponse) -> Result<JevAnalysisResult, String> {
    let answers = response.answers;
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
    use crate::efficiency::{EfficiencySignals, EfficiencyTask};

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
    fn rejects_missing_or_out_of_range_probabilities() {
        assert!(probability(&HashMap::new(), "missing").is_err());
        let answers = HashMap::from([("bad".into(), NoulAnswer { noul: 1.1 })]);
        assert!(probability(&answers, "bad").is_err());
    }

    #[test]
    fn base_request_contains_all_narrow_decisions() {
        let questions = questions(&empty_input());
        for (id, _) in BASE_QUESTIONS {
            assert!(questions.contains_key(id));
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
}
