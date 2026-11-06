//! API-equivalent model-token estimates, not a Codex subscription invoice.
//!
//! Rates verified 2026-11-07 against official OpenAI pricing and model pages:
//! https://developers.openai.com/api/docs/pricing
//! https://developers.openai.com/api/docs/models/gpt-6.1-sol
//! https://developers.openai.com/api/docs/models/gpt-6-astra
//! Cached reads and cache writes are included in input tokens. Reasoning is
//! already included in output tokens. Unrecorded cache retention uses the
//! standard 30-minute write rate. Legacy aggregate-only usage uses short-context
//! rates because a session-wide cumulative input count is not a request size.

use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Copy, Default, PartialEq)]
struct Usage {
    input: i64,
    cached: i64,
    writes: i64,
    output: i64,
    total: i64,
}

impl Usage {
    fn read(value: &Value) -> Option<Self> {
        let input = value.get("input_tokens")?.as_i64()?;
        let output = value.get("output_tokens")?.as_i64()?;
        let cached = value
            .get("cached_input_tokens")
            .map_or(Some(0), Value::as_i64)?;
        let writes = value
            .get("cache_write_input_tokens")
            .map_or(Some(0), Value::as_i64)?;
        let total = value
            .get("total_tokens")
            .map_or_else(|| Some(input.saturating_add(output)), Value::as_i64)?;
        (input >= 0
            && output >= 0
            && cached >= 0
            && writes >= 0
            && total >= 0
            && cached.saturating_add(writes) <= input)
            .then_some(Self {
                input,
                cached,
                writes,
                output,
                total,
            })
    }

    fn material(self) -> bool {
        self.input > 0 || self.output > 0
    }

    fn subtract(self, previous: Self) -> Option<Self> {
        (self.input >= previous.input
            && self.cached >= previous.cached
            && self.writes >= previous.writes
            && self.output >= previous.output
            && self.total >= previous.total)
            .then(|| Self {
                input: self.input - previous.input,
                cached: self.cached - previous.cached,
                writes: self.writes - previous.writes,
                output: self.output - previous.output,
                total: self.total - previous.total,
            })
    }

    fn add(&mut self, usage: Self) {
        self.input = self.input.saturating_add(usage.input);
        self.cached = self.cached.saturating_add(usage.cached);
        self.writes = self.writes.saturating_add(usage.writes);
        self.output = self.output.saturating_add(usage.output);
        self.total = self.total.saturating_add(usage.total);
    }

    fn same_tokens(self, other: Self) -> bool {
        self.input == other.input && self.cached == other.cached && self.output == other.output
    }

    fn remaining(self, covered: Self) -> Self {
        Self {
            input: self.input.saturating_sub(covered.input).max(0),
            cached: self.cached.saturating_sub(covered.cached).max(0),
            writes: self.writes.saturating_sub(covered.writes).max(0),
            output: self.output.saturating_sub(covered.output).max(0),
            total: self.total.saturating_sub(covered.total).max(0),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct Context {
    model: String,
    tier: String,
    long_cache: bool,
}

#[derive(Clone)]
struct Charge {
    usage: Usage,
    context: Context,
    request_input: Option<i64>,
    suppressed: bool,
}

struct Pending {
    usage: Usage,
    context: Context,
    legacy_index: usize,
}

#[derive(Default)]
pub(super) struct UsageTracker {
    context: Context,
    modern: HashMap<String, Charge>,
    legacy: Vec<Charge>,
    previous_legacy: Option<Usage>,
    modern_since_legacy: Usage,
    accounted_usage: Usage,
    pending: Option<Pending>,
    reported: Option<Usage>,
    invalid: bool,
    seen: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct UsageSummary {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: Option<f64>,
}

impl UsageTracker {
    pub fn observe(&mut self, row: &Value) {
        let payload = &row["payload"];
        match row["type"].as_str().unwrap_or_default() {
            "turn_context" => {
                if let Some(model) = payload["model"].as_str() {
                    self.context.model = model.into();
                }
                self.context.tier = payload["service_tier"].as_str().unwrap_or_default().into();
                self.context.long_cache = payload["prompt_cache_retention"] == "12h";
            }
            "token_usage_record" => {
                if let Some(usage) = Usage::read(&payload["thread_token_usage"]) {
                    self.reported = Some(usage);
                }
                if !payload["usage"].is_object() {
                    return;
                }
                self.seen = true;
                let Some(usage) = Usage::read(&payload["usage"]) else {
                    self.invalid = true;
                    return;
                };
                let Some(id) = payload["response_id"].as_str().filter(|id| !id.is_empty()) else {
                    self.invalid = true;
                    return;
                };
                // Some resumed logs begin with a thread snapshot that includes
                // earlier requests. Only add a historical gap not already
                // represented by the legacy prefix or previous responses.
                if !self.modern.contains_key(id) {
                    if let Some(prefix) = Usage::read(&payload["thread_token_usage"])
                        .and_then(|thread| thread.subtract(usage))
                    {
                        if let Some(gap) = prefix.subtract(self.accounted()) {
                            if gap.material() {
                                self.push_legacy(gap, None);
                            }
                        }
                    }
                }
                let (context, increment) = if let Some(previous) = self.modern.get_mut(id) {
                    let increment = usage.subtract(previous.usage);
                    if increment.is_none() {
                        self.invalid = true;
                    }
                    self.accounted_usage = self.accounted_usage.remaining(previous.usage);
                    self.accounted_usage.add(usage);
                    previous.usage = usage;
                    previous.request_input = Some(usage.input);
                    (previous.context.clone(), increment.unwrap_or_default())
                } else {
                    let context = self.context.clone();
                    self.modern.insert(
                        id.into(),
                        Charge {
                            usage,
                            context: context.clone(),
                            request_input: Some(usage.input),
                            suppressed: false,
                        },
                    );
                    self.accounted_usage.add(usage);
                    (context, usage)
                };
                if !increment.material() {
                    self.pending = None;
                    return;
                }
                // In the reverse order, the legacy anchor already included
                // this response. Replace its provisional charge, rather than
                // treating the modern twin as usage after that anchor.
                if let Some(previous) = self.pending.take() {
                    if previous.context == context && previous.usage.same_tokens(usage) {
                        self.legacy[previous.legacy_index].suppressed = true;
                        self.accounted_usage = self
                            .accounted_usage
                            .remaining(self.legacy[previous.legacy_index].usage);
                        return;
                    }
                }
                self.modern_since_legacy.add(increment);
            }
            "event_msg" if payload["type"] == "token_count" => {
                let info = &payload["info"];
                if !info["total_token_usage"].is_object() {
                    return;
                }
                self.seen = true;
                let Some(total) = Usage::read(&info["total_token_usage"]) else {
                    self.invalid = true;
                    return;
                };
                self.reported = Some(total);
                let delta = self
                    .previous_legacy
                    .and_then(|previous| total.subtract(previous))
                    .unwrap_or(total);
                let covered = if self.previous_legacy.is_some() {
                    self.modern_since_legacy
                } else {
                    self.accounted()
                };
                let delta = delta.remaining(covered);
                self.previous_legacy = Some(total);
                self.modern_since_legacy = Usage::default();
                self.pending = None;
                if !delta.material() {
                    return;
                }
                if delta.cached.saturating_add(delta.writes) > delta.input {
                    self.invalid = true;
                }
                let last = Usage::read(&info["last_token_usage"]);
                let (prefix, current, request_input) =
                    match last.and_then(|last| delta.subtract(last).map(|prefix| (prefix, last))) {
                        Some((prefix, last)) => (prefix, last, Some(last.input)),
                        None => (Usage::default(), delta, last.map(|last| last.input)),
                    };
                if prefix.material() {
                    self.push_legacy(prefix, None);
                }
                let index = self.push_legacy(current, request_input);
                self.pending = Some(Pending {
                    usage: last.unwrap_or(current),
                    context: self.context.clone(),
                    legacy_index: index,
                });
            }
            "event_msg"
                if matches!(
                    payload["type"].as_str(),
                    Some(
                        "task_started"
                            | "turn_started"
                            | "task_complete"
                            | "task_completed"
                            | "turn_complete"
                            | "turn_completed"
                            | "turn_aborted"
                    )
                ) =>
            {
                self.pending = None
            }
            _ => {}
        }
    }

    fn accounted(&self) -> Usage {
        self.accounted_usage
    }

    fn push_legacy(&mut self, usage: Usage, request_input: Option<i64>) -> usize {
        let index = self.legacy.len();
        self.legacy.push(Charge {
            usage,
            context: self.context.clone(),
            request_input,
            suppressed: false,
        });
        self.accounted_usage.add(usage);
        index
    }

    pub fn summary(&self) -> UsageSummary {
        let mut total = self.accounted_usage;
        let mut cost = (self.seen && !self.invalid).then_some(0.0);
        for charge in self
            .modern
            .values()
            .chain(self.legacy.iter())
            .filter(|charge| !charge.suppressed)
        {
            cost = cost.and_then(|total| estimate(charge).map(|cost| total + cost));
        }
        // Older records can contain only a thread-wide snapshot, without a
        // response identity. Keep its recorded totals, but never invent a cost
        // for usage not covered by the priced response/legacy observations.
        if let Some(reported) = self.reported {
            if reported.input > total.input || reported.output > total.output {
                total = reported;
                cost = None;
            }
        }
        UsageSummary {
            input_tokens: total.input,
            output_tokens: total.output,
            cache_read_tokens: total.cached,
            total_tokens: total.total,
            cost_usd: cost,
        }
    }
}

fn estimate(charge: &Charge) -> Option<f64> {
    if !charge.usage.material() {
        return Some(0.0);
    }
    let (input, cached, output, long_context) = match charge.context.model.as_str() {
        "gpt-6.1-sol" => (2.0, 0.1, 10.0, true),
        "gpt-6.1-terra" => (1.0, 0.05, 5.0, true),
        "gpt-6-astra" => (10.0, 1.0, 50.0, true),
        "gpt-6-luna" => (0.1, 0.01, 0.5, true),
        "gpt-5.3-codex" if charge.usage.writes == 0 => (1.75, 0.175, 14.0, false),
        _ => return None,
    };
    let speed = match charge.context.tier.as_str() {
        "" | "default" | "standard" | "auto" => 1.0,
        "priority" | "fast" => 2.0,
        "ultrafast" if matches!(charge.context.model.as_str(), "gpt-6.1-sol" | "gpt-6-astra") => {
            6.0
        }
        "flex" | "batch" if long_context => 0.5,
        _ => return None,
    };
    let long = long_context && charge.request_input.is_some_and(|input| input > 272_000);
    let input_scale = if long { 2.0 } else { 1.0 };
    let output_scale = if long { 1.5 } else { 1.0 };
    let usage = charge.usage;
    let writes = input * if charge.context.long_cache { 2.5 } else { 1.25 };
    Some(
        ((usage.input - usage.cached - usage.writes) as f64 * input
            + usage.cached as f64 * cached
            + usage.writes as f64 * writes)
            * input_scale
            * speed
            / 1_000_000.0
            + usage.output as f64 * output * output_scale * speed / 1_000_000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context(model: &str) -> Value {
        json!({"type":"turn_context","payload":{"model":model}})
    }
    fn tokens(input: i64, cached: i64, output: i64) -> Value {
        json!({"input_tokens":input,"cached_input_tokens":cached,"output_tokens":output,"total_tokens":input+output})
    }
    fn modern(id: &str, usage: Value) -> Value {
        json!({"type":"token_usage_record","payload":{"response_id":id,"usage":usage}})
    }
    fn legacy(total: Value, last: Value) -> Value {
        json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":total,"last_token_usage":last}}})
    }
    fn summary(rows: &[Value]) -> UsageSummary {
        let mut tracker = UsageTracker::default();
        for row in rows {
            tracker.observe(row);
        }
        tracker.summary()
    }
    fn assert_cost(summary: UsageSummary, expected: f64) {
        assert!((summary.cost_usd.unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn legacy_cumulative_usage_is_not_a_long_context_request_size() {
        let rows = [
            context("gpt-6.1-sol"),
            legacy(tokens(1_000_000, 200_000, 100_000), Value::Null),
        ];
        assert_cost(summary(&rows), 2.62);
        let mut rows = rows.to_vec();
        rows.push(legacy(tokens(1_200_000, 200_000, 150_000), Value::Null));
        rows.push(legacy(tokens(1_200_000, 200_000, 150_000), Value::Null));
        assert_cost(summary(&rows), 3.52);
    }

    #[test]
    fn legacy_prefix_and_modern_requests_are_both_counted_once_in_either_order() {
        let prefix = tokens(1_000, 0, 100);
        let response = tokens(2_000, 0, 200);
        let total = tokens(3_000, 0, 300);
        for reverse in [false, true] {
            let mut rows = vec![
                context("gpt-6.1-sol"),
                legacy(prefix.clone(), prefix.clone()),
            ];
            let pair = [
                modern("invented-1", response.clone()),
                legacy(total.clone(), response.clone()),
            ];
            rows.extend(if reverse {
                pair.into_iter().rev().collect::<Vec<_>>()
            } else {
                pair.to_vec()
            });
            let result = summary(&rows);
            assert_eq!(result.total_tokens, 3_300);
            assert_cost(result, 0.009);
        }
    }

    #[test]
    fn coalesced_legacy_snapshot_does_not_rebill_modern_requests() {
        let result = summary(&[
            context("gpt-6.1-sol"),
            modern("invented-1", tokens(100, 0, 10)),
            modern("invented-2", tokens(200, 0, 20)),
            legacy(tokens(300, 0, 30), tokens(200, 0, 20)),
        ]);
        assert_eq!(result.total_tokens, 330);
        assert_cost(result, 0.0009);
    }

    #[test]
    fn corrected_response_and_legacy_mirror_do_not_double_charge_the_increment() {
        for reverse in [false, true] {
            let mut rows = vec![context("gpt-6.1-sol")];
            for (input, output) in [(100, 10), (120, 12)] {
                let usage = tokens(input, 0, output);
                let pair = [
                    modern("invented-same-response", usage.clone()),
                    legacy(usage.clone(), usage),
                ];
                rows.extend(if reverse {
                    pair.into_iter().rev().collect::<Vec<_>>()
                } else {
                    pair.to_vec()
                });
            }
            let result = summary(&rows);
            assert_eq!(result.total_tokens, 132);
            assert_cost(result, 0.00036);
        }
    }

    #[test]
    fn modern_thread_prefix_only_adds_missing_historical_usage() {
        let response = json!({"type":"token_usage_record","payload":{"response_id":"invented-1","usage":tokens(100,0,10),"thread_token_usage":tokens(1100,0,110)}});
        for known_prefix in [false, true] {
            let mut rows = vec![context("gpt-6.1-sol")];
            if known_prefix {
                rows.push(legacy(tokens(1000, 0, 100), tokens(1000, 0, 100)));
            }
            rows.push(response.clone());
            rows.push(legacy(tokens(1100, 0, 110), tokens(100, 0, 10)));
            let result = summary(&rows);
            assert_eq!(result.total_tokens, 1210);
            assert_cost(result, 0.0033);
        }
    }

    #[test]
    fn later_legacy_only_requests_and_counter_resets_are_preserved() {
        let result = summary(&[
            context("gpt-6.1-sol"),
            legacy(tokens(1000, 200, 100), Value::Null),
            modern("invented-1", tokens(1000, 200, 100)),
            legacy(tokens(2000, 400, 200), tokens(1000, 200, 100)),
            legacy(tokens(2500, 500, 250), tokens(500, 100, 50)),
            legacy(tokens(100, 20, 10), tokens(100, 20, 10)),
        ]);
        assert_eq!(result.total_tokens, 2860);
        assert_cost(result, 0.006812);
    }

    #[test]
    fn model_switches_and_request_speed_use_each_responses_own_rates() {
        let mut fast = context("gpt-6-astra");
        fast["payload"]["service_tier"] = json!("ultrafast");
        let result = summary(&[
            context("gpt-6.1-sol"),
            modern("invented-sol", tokens(1000, 0, 100)),
            fast,
            modern("invented-astra", tokens(1000, 0, 100)),
        ]);
        assert_cost(result, 0.093);
        for (model, expected) in [
            ("gpt-6.1-terra", 0.0015),
            ("gpt-6-luna", 0.00015),
            ("gpt-5.3-codex", 0.00315),
        ] {
            assert_cost(
                summary(&[context(model), modern("invented", tokens(1000, 0, 100))]),
                expected,
            );
        }
    }

    #[test]
    fn long_context_threshold_is_strict_and_applies_to_the_whole_response() {
        assert_cost(
            summary(&[
                context("gpt-6.1-sol"),
                modern("invented", tokens(272000, 0, 1000)),
            ]),
            0.554,
        );
        assert_cost(
            summary(&[
                context("gpt-6.1-sol"),
                modern("invented", tokens(272001, 0, 1000)),
            ]),
            1.103004,
        );
        assert_cost(
            summary(&[
                context("gpt-6.1-sol"),
                modern("invented", tokens(300000, 100000, 20000)),
            ]),
            1.12,
        );
    }

    #[test]
    fn missing_unknown_and_malformed_usage_never_looks_free() {
        assert!(summary(&[context("gpt-6.1-sol")]).cost_usd.is_none());
        assert!(summary(&[
            context("invented-unpriced"),
            modern("invented", tokens(100, 0, 10))
        ])
        .cost_usd
        .is_none());
        assert!(summary(&[
            context("gpt-6.1-sol"),
            modern("invented", tokens(-100, 0, 10))
        ])
        .cost_usd
        .is_none());
        assert!(
            summary(&[context("gpt-6.1-sol"), modern("", tokens(100, 0, 10))])
                .cost_usd
                .is_none()
        );
        assert_cost(
            summary(&[context("gpt-6.1-sol"), modern("invented", tokens(0, 0, 0))]),
            0.0,
        );
    }
}
