use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

use super::{
    EfficiencyFinding, EfficiencyFindingType, EfficiencyInput, EfficiencyMetricEvaluation,
    EfficiencyMetricScale, JevEfficiencyDecision,
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

/// How Jev is asked to answer one metric.
///
/// `Noul` is a yes/no probability. `Rubric` is a Score question: Jev places the
/// session along ordered levels and returns a weighted position, so a two-sided
/// judgement keeps its direction instead of collapsing into one probability.
enum MetricAnswer {
    Noul,
    Rubric(&'static [&'static str]),
}

struct BaseMetricDefinition {
    key: &'static str,
    label: &'static str,
    question: &'static str,
    higher_probability_is_better: bool,
    answer: MetricAnswer,
}

pub const THINKING_LEVELS: [&str; 5] = [
    "Far too little thinking for the difficulty of the work",
    "A little less thinking than the work needed",
    "Thinking matched the difficulty of the work",
    "A little more thinking than the work needed",
    "Far more thinking than the work needed",
];

/// The rubric position that means the amount of thinking matched the work.
pub fn balanced_thinking_position() -> f64 {
    (THINKING_LEVELS.len() - 1) as f64 / 2.0
}

/// Turn a rubric position into a 0..1 "how balanced" value, penalising a session
/// the same amount whether it thought too little or too much.
pub fn thinking_balance(position: f64) -> f64 {
    let middle = balanced_thinking_position();
    (1.0 - (position.clamp(0.0, middle * 2.0) - middle).abs() / middle).clamp(0.0, 1.0)
}

const BASE_METRICS: [BaseMetricDefinition; 10] = [
    BaseMetricDefinition {
        key: "progressingEfficiently",
        label: "Progress",
        question: "Did the agent make steady, meaningful progress toward the user's task?",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "toolCallsUseful",
        label: "Useful tool calls",
        question: "Were the tool calls useful and proportionate to completing the task?",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "redundantWorkPresent",
        label: "Avoided redundant work",
        question: "Was materially redundant work present? An action is redundant only when it repeats earlier work whose result could not have changed, because nothing relevant was modified in between — for example re-reading an unchanged file, or re-running the same search after no edits. Do NOT count: re-running a command after a change that could alter its result, such as re-running tests or a build after an edit; the same tool applied to a different target or different input; or a retry after an error or interruption. Each entry in state.actions carries repeatedSimilarCallCount, the number of other actions with the identical tool and input; treat that as evidence, not proof, since an identical call can still be legitimate once the state it reads has changed.",
        higher_probability_is_better: false,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "excessiveExploration",
        label: "Proportionate exploration",
        question: "Was exploration excessive relative to the task?",
        higher_probability_is_better: false,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "likelyThrashing",
        label: "Avoided thrashing",
        question: "Did the agent cycle among similar actions without meaningful progress?",
        higher_probability_is_better: false,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "effectiveRecovery",
        label: "Effective recovery",
        question: "When mistakes or failures occurred, did the agent recover effectively?",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "tokenUsageEfficient",
        label: "Efficient token use",
        question: "Was token usage efficient for the work completed? Consider total tokens, context growth, turn count, repeated work, and tool activity. Judge resource efficiency only; do not estimate or consider monetary cost.",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "thinkingBalance",
        label: "Balanced thinking",
        question: "Rate the amount of extended thinking the agent did against what the work actually required. state.signals reports thinkingBlocks (how many extended-thinking blocks the agent produced) and thinkingChars (their combined length); weigh those against the work actually done, which state.task and the rest of state.signals describe as turns, duration, total tokens, and tool calls. The thinking text itself is never shared, so judge the volume against the difficulty of the task and against what the agent did next. Too little thinking shows up as avoidable mistakes, rework, wrong paths, or failed tool calls on steps that needed planning. Too much shows up as long or frequent deliberation on work that was simple, mechanical, or already decided — for example thinking at length before a single trivial read, or re-deliberating a choice the user had already made. A genuinely hard, ambiguous, or high-risk task warrants heavy thinking, and a quick answer to simple, well-specified work is correct. Judge the amount against the work, not against a fixed budget.",
        higher_probability_is_better: true,
        answer: MetricAnswer::Rubric(&THINKING_LEVELS),
    },
    BaseMetricDefinition {
        key: "subagentsUseful",
        label: "Useful subagents",
        question: "If subagents were used, did they add useful independent work? Answer yes when no subagents were needed or used.",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
    BaseMetricDefinition {
        key: "likelyTaskCompleted",
        label: "Task completion",
        question: "Does the trace indicate that the user's requested task was completed successfully?",
        higher_probability_is_better: true,
        answer: MetricAnswer::Noul,
    },
];

#[derive(Serialize)]
struct JevQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<&'static [&'static str]>,
}

impl JevQuestion {
    fn noul(instructions: String) -> Self {
        Self {
            question_type: "noul",
            instructions,
            criteria: None,
        }
    }

    fn rubric(instructions: String, criteria: &'static [&'static str]) -> Self {
        Self {
            question_type: "score",
            instructions,
            criteria: Some(criteria),
        }
    }
}

#[derive(Serialize)]
struct JevRequest<'a> {
    model: &'static str,
    state: &'a EfficiencyInput,
    questions: HashMap<String, JevQuestion>,
}

#[derive(Debug, Default, Deserialize)]
struct JevAnswer {
    #[serde(default)]
    noul: Option<f64>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    probabilities: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct JevResponse {
    answers: HashMap<String, JevAnswer>,
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

fn questions(input: &EfficiencyInput) -> HashMap<String, JevQuestion> {
    let mut questions = BASE_METRICS
        .iter()
        .map(|metric| {
            let question = match metric.answer {
                MetricAnswer::Noul => JevQuestion::noul(metric.question.to_string()),
                MetricAnswer::Rubric(criteria) => {
                    JevQuestion::rubric(metric.question.to_string(), criteria)
                }
            };
            (metric.key.to_string(), question)
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
                JevQuestion::noul(format!(
                    "Evaluate only state.actions at zero-based positions {first_action_position} through {last_action_position}. Ignore every action outside that range. {prompt}"
                )),
            );
        }
    }
    questions
}

fn probability(answers: &HashMap<String, JevAnswer>, key: &str) -> Result<f64, String> {
    let value = answers
        .get(key)
        .and_then(|answer| answer.noul)
        .ok_or_else(|| format!("Jev response omitted decision {key}"))?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("Jev returned an invalid probability for {key}"));
    }
    Ok(value)
}

/// Read a Score answer as a position along its ordered levels.
fn rubric_position(
    answers: &HashMap<String, JevAnswer>,
    key: &str,
    levels: usize,
) -> Result<f64, String> {
    let value = answers
        .get(key)
        .and_then(|answer| answer.score)
        .ok_or_else(|| format!("Jev response omitted decision {key}"))?;
    let highest = (levels - 1) as f64;
    if !value.is_finite() || !(0.0..=highest).contains(&value) {
        return Err(format!("Jev returned an invalid score for {key}"));
    }
    Ok(value)
}

fn metric_evaluations(
    answers: &HashMap<String, JevAnswer>,
) -> Result<Vec<EfficiencyMetricEvaluation>, String> {
    BASE_METRICS
        .iter()
        .map(|metric| match metric.answer {
            MetricAnswer::Noul => {
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
                    scale: None,
                })
            }
            MetricAnswer::Rubric(levels) => {
                let position = rubric_position(answers, metric.key, levels.len())?;
                let landed = position.round() as usize;
                Ok(EfficiencyMetricEvaluation {
                    key: metric.key.to_string(),
                    label: metric.label.to_string(),
                    question: metric.question.to_string(),
                    higher_probability_is_better: metric.higher_probability_is_better,
                    probability: answers
                        .get(metric.key)
                        .and_then(|answer| answer.probabilities.get(&landed.to_string()))
                        .copied()
                        .unwrap_or_default(),
                    score: (thinking_balance(position) * 100.0).round() as u8,
                    scale: Some(EfficiencyMetricScale {
                        levels: levels.iter().map(|level| (*level).to_string()).collect(),
                        position,
                        level: levels.get(landed).copied().unwrap_or_default().to_string(),
                    }),
                })
            }
        })
        .collect()
}

fn findings(
    input: &EfficiencyInput,
    answers: &HashMap<String, JevAnswer>,
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
            let Some(noul) = answers
                .get(&format!("window_{window}_{suffix}"))
                .and_then(|answer| answer.noul)
            else {
                continue;
            };
            if noul >= 0.65 {
                findings.push(EfficiencyFinding {
                    finding_type,
                    probability: noul,
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
        thinking_balance: rubric_position(&answers, "thinkingBalance", THINKING_LEVELS.len())?,
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

    fn noul(value: f64) -> JevAnswer {
        JevAnswer {
            noul: Some(value),
            ..Default::default()
        }
    }

    fn rubric(position: f64) -> JevAnswer {
        JevAnswer {
            score: Some(position),
            probabilities: HashMap::from([(position.round().to_string(), 0.8)]),
            ..Default::default()
        }
    }

    fn every_answer(value: f64, position: f64) -> HashMap<String, JevAnswer> {
        BASE_METRICS
            .iter()
            .map(|metric| {
                let answer = match metric.answer {
                    MetricAnswer::Noul => noul(value),
                    MetricAnswer::Rubric(_) => rubric(position),
                };
                (metric.key.to_string(), answer)
            })
            .collect()
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
                thinking_blocks: 0,
                thinking_chars: 0,
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
    fn thinking_is_asked_as_an_ordered_rubric_not_a_yes_no() {
        // "Was thinking proportionate?" as one probability cannot say which way a
        // session went wrong. A Score question keeps the direction: below the
        // middle level is too little, above it is too much.
        let metric = BASE_METRICS
            .iter()
            .find(|m| m.key == "thinkingBalance")
            .expect("metric exists");
        let MetricAnswer::Rubric(levels) = metric.answer else {
            panic!("thinking must be a rubric, not a noul");
        };

        assert_eq!(metric.label, "Balanced thinking");
        assert_eq!(levels.len(), 5);
        assert!(
            levels[2].contains("matched"),
            "the middle level is balanced"
        );
        assert!(levels[0].contains("too little"));
        assert!(levels[4].contains("more"));
        for required in [
            "thinkingBlocks",
            "thinkingChars",
            "difficulty",
            "tool calls",
        ] {
            assert!(
                metric.question.contains(required),
                "the rubric question must reference {required:?}"
            );
        }

        let question = &questions(&empty_input())["thinkingBalance"];
        assert_eq!(question.question_type, "score");
        assert_eq!(question.criteria, Some(&THINKING_LEVELS[..]));
    }

    #[test]
    fn only_the_thinking_question_is_sent_as_a_score() {
        let questions = questions(&empty_input());
        for metric in &BASE_METRICS {
            let question = &questions[metric.key];
            match metric.answer {
                MetricAnswer::Noul => {
                    assert_eq!(question.question_type, "noul", "{}", metric.key);
                    assert!(question.criteria.is_none(), "{}", metric.key);
                }
                MetricAnswer::Rubric(_) => {
                    assert_eq!(metric.key, "thinkingBalance");
                    assert_eq!(question.question_type, "score");
                }
            }
        }
    }

    #[test]
    fn a_rubric_answer_keeps_the_level_it_landed_on() {
        // The bar alone says "unbalanced" without saying which way, so the level
        // Jev picked travels with it.
        let answers = every_answer(0.5, 3.4);
        let thinking = metric_evaluations(&answers)
            .unwrap()
            .into_iter()
            .find(|evaluation| evaluation.key == "thinkingBalance")
            .expect("evaluated");
        let scale = thinking.scale.expect("a rubric metric carries its scale");

        assert_eq!(scale.position, 3.4);
        assert_eq!(scale.level, THINKING_LEVELS[3]);
        assert_eq!(scale.levels.len(), THINKING_LEVELS.len());
        assert_eq!(thinking.score, 30);
        assert_eq!(thinking.probability, 0.8);
    }

    #[test]
    fn yes_no_metrics_carry_no_scale() {
        let answers = every_answer(0.5, 2.0);
        for evaluation in metric_evaluations(&answers).unwrap() {
            if evaluation.key != "thinkingBalance" {
                assert!(evaluation.scale.is_none(), "{}", evaluation.key);
            }
        }
    }

    #[test]
    fn the_request_carries_thinking_volume_but_never_thinking_text() {
        let mut input = empty_input();
        input.signals.thinking_blocks = 55;
        input.signals.thinking_chars = 42_854;
        let request = serde_json::to_value(JevRequest {
            model: JEV_MODEL,
            state: &input,
            questions: questions(&input),
        })
        .unwrap();

        assert_eq!(
            request.pointer("/state/signals/thinkingBlocks"),
            Some(&json!(55))
        );
        assert_eq!(
            request.pointer("/state/signals/thinkingChars"),
            Some(&json!(42_854))
        );
        assert_eq!(
            request.pointer("/questions/thinkingBalance/type"),
            Some(&json!("score"))
        );
        assert_eq!(
            request
                .pointer("/questions/thinkingBalance/criteria")
                .and_then(|criteria| criteria.as_array())
                .map(Vec::len),
            Some(5)
        );
    }

    #[test]
    fn rejects_missing_or_out_of_range_probabilities() {
        assert!(probability(&HashMap::new(), "missing").is_err());
        let answers = HashMap::from([("bad".into(), noul(1.1))]);
        assert!(probability(&answers, "bad").is_err());
        assert!(probability(&HashMap::from([("r".into(), rubric(2.0))]), "r").is_err());
    }

    #[test]
    fn base_request_contains_all_narrow_decisions() {
        let questions = questions(&empty_input());
        for metric in &BASE_METRICS {
            assert_eq!(questions[metric.key].instructions, metric.question);
        }
    }

    #[test]
    fn rejects_a_rubric_position_outside_its_levels() {
        let levels = THINKING_LEVELS.len();
        assert!(rubric_position(&HashMap::new(), "missing", levels).is_err());
        let off_the_end = HashMap::from([("t".to_string(), rubric(4.5))]);
        assert!(rubric_position(&off_the_end, "t", levels).is_err());
        let noul_only = HashMap::from([("t".to_string(), noul(0.5))]);
        assert!(rubric_position(&noul_only, "t", levels).is_err());
        let inside = HashMap::from([("t".to_string(), rubric(3.2))]);
        assert_eq!(rubric_position(&inside, "t", levels).unwrap(), 3.2);
    }

    #[test]
    fn dashboard_metrics_are_generated_from_the_same_definitions_as_jev_questions() {
        let answers = every_answer(0.25, 2.0);
        let evaluations = metric_evaluations(&answers).unwrap();

        assert_eq!(evaluations.len(), BASE_METRICS.len());
        for (evaluation, definition) in evaluations.iter().zip(BASE_METRICS.iter()) {
            assert_eq!(evaluation.key, definition.key);
            assert_eq!(evaluation.label, definition.label);
            assert_eq!(evaluation.question, definition.question);
            let expected = match definition.answer {
                MetricAnswer::Rubric(_) => 100,
                MetricAnswer::Noul if definition.higher_probability_is_better => 25,
                MetricAnswer::Noul => 75,
            };
            assert_eq!(evaluation.score, expected);
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
            ("window_0_recovery".into(), noul(0.75)),
            ("window_1_recovery".into(), noul(0.76)),
            ("window_2_recovery".into(), noul(0.78)),
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
