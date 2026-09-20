use chrono::Utc;

use super::{
    EfficiencyDimensions, EfficiencyFinding, EfficiencyMetricEvaluation, JevEfficiencyDecision,
    SessionEfficiencyAnalysis, ANALYSIS_VERSION, DECISION_SET_VERSION, SCORE_FORMULA_VERSION,
};

fn percentage(probability: f64) -> u8 {
    (probability.clamp(0.0, 1.0) * 100.0).round() as u8
}

pub fn build_analysis(
    session_id: String,
    session_path: String,
    transcript_fingerprint: String,
    analyzed_turns: usize,
    decisions: JevEfficiencyDecision,
    metric_evaluations: Vec<EfficiencyMetricEvaluation>,
    findings: Vec<EfficiencyFinding>,
) -> SessionEfficiencyAnalysis {
    let dimensions = EfficiencyDimensions {
        progress: percentage(decisions.progressing_efficiently),
        tool_use: percentage(decisions.tool_calls_useful),
        focus: percentage(
            1.0 - (decisions.redundant_work_present + decisions.likely_thrashing) / 2.0,
        ),
        exploration: percentage(1.0 - decisions.excessive_exploration),
        recovery: percentage(decisions.effective_recovery),
        token_use: percentage(decisions.token_usage_efficient),
        thinking: percentage(super::jev::thinking_balance(decisions.thinking_balance)),
    };
    let weighted = f64::from(dimensions.progress) * 0.25
        + f64::from(dimensions.tool_use) * 0.20
        + f64::from(dimensions.focus) * 0.15
        + f64::from(dimensions.exploration) * 0.10
        + f64::from(dimensions.recovery) * 0.10
        + f64::from(dimensions.token_use) * 0.10
        + f64::from(dimensions.thinking) * 0.05
        + decisions.likely_task_completed.clamp(0.0, 1.0) * 100.0 * 0.05;
    SessionEfficiencyAnalysis {
        session_id,
        session_path,
        score: weighted.round().clamp(0.0, 100.0) as u8,
        dimensions,
        metric_evaluations,
        findings,
        decisions,
        analyzed_at: Utc::now().to_rfc3339(),
        analyzed_turns,
        transcript_fingerprint,
        analysis_version: ANALYSIS_VERSION,
        decision_set_version: DECISION_SET_VERSION,
        score_formula_version: SCORE_FORMULA_VERSION,
        stale: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverts_negative_focus_and_exploration_decisions() {
        let analysis = build_analysis(
            "s".into(),
            "p".into(),
            "f".into(),
            1,
            JevEfficiencyDecision {
                progressing_efficiently: 0.9,
                tool_calls_useful: 0.8,
                redundant_work_present: 0.2,
                excessive_exploration: 0.3,
                likely_thrashing: 0.4,
                effective_recovery: 0.7,
                token_usage_efficient: 0.8,
                thinking_balance: 2.7,
                subagents_useful: 0.5,
                likely_task_completed: 1.0,
            },
            vec![],
            vec![],
        );
        assert_eq!(analysis.dimensions.focus, 70);
        assert_eq!(analysis.dimensions.exploration, 70);
        assert_eq!(analysis.dimensions.token_use, 80);
        assert_eq!(analysis.score, 79);
    }

    fn decisions() -> JevEfficiencyDecision {
        JevEfficiencyDecision {
            progressing_efficiently: 1.0,
            tool_calls_useful: 1.0,
            redundant_work_present: 0.0,
            excessive_exploration: 0.0,
            likely_thrashing: 0.0,
            effective_recovery: 1.0,
            token_usage_efficient: 1.0,
            thinking_balance: super::super::jev::balanced_thinking_position(),
            subagents_useful: 1.0,
            likely_task_completed: 1.0,
        }
    }

    fn analysis_for(decisions: JevEfficiencyDecision) -> SessionEfficiencyAnalysis {
        build_analysis(
            "s".into(),
            "p".into(),
            "f".into(),
            1,
            decisions,
            vec![],
            vec![],
        )
    }

    #[test]
    fn the_middle_rubric_level_scores_full_marks() {
        assert_eq!(analysis_for(decisions()).dimensions.thinking, 100);
    }

    #[test]
    fn too_little_and_too_much_thinking_are_penalised_the_same() {
        // The rubric is two-sided around its middle level, so an equal distance
        // either way must cost the same.
        let mut under = decisions();
        under.thinking_balance = 0.5;
        let mut over = decisions();
        over.thinking_balance = 3.5;

        assert_eq!(analysis_for(under).dimensions.thinking, 25);
        assert_eq!(analysis_for(over).dimensions.thinking, 25);
    }

    #[test]
    fn the_worst_rubric_ends_score_zero() {
        let mut silent = decisions();
        silent.thinking_balance = 0.0;
        let mut endless = decisions();
        endless.thinking_balance = 4.0;

        assert_eq!(analysis_for(silent).dimensions.thinking, 0);
        assert_eq!(analysis_for(endless).dimensions.thinking, 0);
    }

    #[test]
    fn the_weights_still_total_one_hundred() {
        assert_eq!(analysis_for(decisions()).score, 100);
    }
}
