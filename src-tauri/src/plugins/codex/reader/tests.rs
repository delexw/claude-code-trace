//! Every transcript in this module is invented; no laptop chat is a fixture.
use super::*;
use serde_json::json;

fn write_rows(path: &Path, rows: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
}

fn row(kind: &str, payload: Value) -> Value {
    json!({"timestamp":"2026-01-02T03:04:05Z","type":kind,"payload":payload})
}

fn response(role: &str, body: &str) -> Value {
    row(
        "response_item",
        json!({"type":"message","role":role,"content":[{"type":"output_text","text":body}]}),
    )
}

#[test]
fn response_items_win_over_event_twins_without_dropping_repeated_turns() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    write_rows(
        &path,
        &[
            row(
                "event_msg",
                json!({"type":"user_message","message":"Repeat this"}),
            ),
            response("user", "Repeat this"),
            response("assistant", "A fabricated answer"),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"A fabricated answer"}),
            ),
            row(
                "event_msg",
                json!({"type":"user_message","message":"Repeat this"}),
            ),
            response("user", "Repeat this"),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"Event-only legacy answer"}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.count, 4);
    assert_eq!(window.roles, ["user", "claude", "user", "claude"]);
    assert_eq!(window.messages[3].content, "Event-only legacy answer");
    assert_eq!(window.messages[1].items[0].item_type, "Output");
}

#[test]
fn repeated_event_only_turns_do_not_pair_with_later_response_twins() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-mixed-fictional.jsonl");
    write_rows(
        &path,
        &[
            row(
                "event_msg",
                json!({"type":"user_message","message":"Continue"}),
            ),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"First fictional reply"}),
            ),
            response("user", "Continue"),
            row(
                "event_msg",
                json!({"type":"user_message","message":"Continue"}),
            ),
            response("assistant", "Second fictional reply"),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"Second fictional reply"}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.roles, ["user", "claude", "user", "claude"]);
    assert_eq!(
        window
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        [
            "Continue",
            "First fictional reply",
            "Continue",
            "Second fictional reply"
        ]
    );
}

#[test]
fn identical_thought_and_output_text_are_distinct_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-thought-fictional.jsonl");
    write_rows(
        &path,
        &[
            row(
                "event_msg",
                json!({"type":"agent_reasoning","text":"A fictional explanation"}),
            ),
            response("assistant", "A fictional explanation"),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"A fictional explanation"}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.count, 2);
    assert_eq!(window.messages[0].items[0].item_type, "Thinking");
    assert_eq!(window.messages[1].items[0].item_type, "Output");
}

#[test]
fn lifecycle_boundaries_keep_consecutive_repeated_user_turns() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-consecutive-fictional.jsonl");
    write_rows(
        &path,
        &[
            response("user", "Continue"),
            row("event_msg", json!({"type":"task_complete"})),
            row("event_msg", json!({"type":"task_started"})),
            row(
                "event_msg",
                json!({"type":"user_message","message":"Continue"}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.roles, ["user", "user"]);
}

#[test]
fn metadata_and_usage_between_twins_do_not_create_extra_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-interleaved-fictional.jsonl");
    write_rows(
        &path,
        &[
            response("user", "A fictional request"),
            row("turn_context", json!({"model":"gpt-example"})),
            row(
                "event_msg",
                json!({"type":"user_message","message":"A fictional request"}),
            ),
            row(
                "event_msg",
                json!({"type":"agent_message","message":"A fictional answer"}),
            ),
            row("event_msg", json!({"type":"token_count","info":null})),
            response("assistant", "A fictional answer"),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.roles, ["user", "claude"]);
}

#[test]
fn function_and_custom_tools_keep_inputs_results_and_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    write_rows(
        &path,
        &[
            row("turn_context", json!({"model":"gpt-demo"})),
            row(
                "response_item",
                json!({"type":"function_call","name":"exec_command","call_id":"call-demo-1","arguments":"{\"cmd\":\"echo fictional\"}"}),
            ),
            row(
                "response_item",
                json!({"type":"custom_tool_call","name":"apply_patch","call_id":"call-demo-2","input":"*** Begin Patch\n*** End Patch"}),
            ),
            row(
                "response_item",
                json!({"type":"function_call_output","call_id":"call-demo-1","output":"fictional\n"}),
            ),
            row(
                "response_item",
                json!({"type":"custom_tool_call_output","call_id":"call-demo-2","is_error":true,"output":{"error":"invented failure"}}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.count, 2);
    assert_eq!(window.messages[0].model, "gpt-demo");
    assert_eq!(window.messages[0].items[0].tool_result, "fictional\n");
    assert!(window.messages[0].items[0]
        .tool_input
        .contains("echo fictional"));
    assert_eq!(window.messages[1].items[0].tool_name, "apply_patch");
    assert!(window.messages[1].items[0].tool_error);
    assert!(window.messages[1].items[0]
        .tool_result_json
        .contains("invented failure"));
}

#[test]
fn reasoning_is_visible_when_recorded_and_ciphertext_is_never_displayed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    write_rows(
        &path,
        &[
            row(
                "response_item",
                json!({"type":"reasoning","summary":[],"encrypted_content":"fictional-ciphertext"}),
            ),
            row(
                "event_msg",
                json!({"type":"item_completed","item":{"type":"Reasoning","summary_text":["A fabricated thought"],"raw_content":[]}}),
            ),
            response("assistant", "A fabricated conclusion"),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.count, 2);
    assert_eq!(window.messages[0].items[0].item_type, "Thinking");
    assert_eq!(window.messages[0].thinking_count, 1);
    assert!(!serde_json::to_string(&window)
        .unwrap()
        .contains("fictional-ciphertext"));
}

#[test]
fn structured_errors_do_not_mistake_quoted_source_for_failure() {
    assert!(structured_error(
        &json!({"exit_code":2,"output":"invented command failed"})
    ));
    assert!(structured_error(
        &json!({"i":0,"status":"fulfilled","value":{"exit_code":1}})
    ));
    assert!(structured_error(&json!({"isError":true})));
    assert!(structured_error(&json!({"metadata":{"exit_code":127}})));
    assert!(!structured_error(
        &json!({"exit_code":0,"output":"example source: {\"exit_code\":1}"})
    ));
    assert!(output_error(&json!([
        {"type":"input_text","text":"An invented result frame"},
        {"type":"input_text","text":"{\"exit_code\":2,\"output\":\"invented failure\"}"}
    ])));
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-errors.jsonl");
    write_rows(
        &path,
        &[
            row(
                "response_item",
                json!({"type":"function_call","name":"exec_command","namespace":"functions","call_id":"fictional-failure","arguments":"{}"}),
            ),
            row(
                "response_item",
                json!({"type":"function_call_output","call_id":"fictional-failure","output":[
                    {"type":"input_text","text":"An invented result frame"},
                    {"type":"input_text","text":"{\"exit_code\":2,\"output\":\"invented failure\"}"}
                ]}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert!(window.messages[0].items[0].tool_error);
    assert_eq!(
        window.messages[0].items[0].tool_name,
        "functions.exec_command"
    );
}

#[test]
fn agent_messages_and_compaction_markers_keep_their_order_without_ciphertext() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-agents.jsonl");
    write_rows(
        &path,
        &[
            row(
                "response_item",
                json!({"type":"agent_message","author":"reviewer","recipient":"main","content":[{"type":"input_text","text":"The imaginary review passed"},{"type":"encrypted_content","ciphertext":"fabricated-encrypted-value"}]}),
            ),
            row("compacted", json!({"message":"","replacement_history":[]})),
            response("assistant", "Continue the fictional task"),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.roles, ["system", "compact", "claude"]);
    assert_eq!(window.messages[0].content, "The imaginary review passed");
    assert!(window.messages[0].subagent_label.contains("reviewer"));
    assert_eq!(window.messages[1].content, "Context compacted");
    assert!(!serde_json::to_string(&window)
        .unwrap()
        .contains("fabricated-encrypted-value"));
}

#[test]
fn window_index_and_totals_are_complete_and_repeated_cumulative_usage_is_not_added() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    let usage =
        json!({"input_tokens":100,"output_tokens":30,"cached_input_tokens":60,"total_tokens":130});
    write_rows(
        &path,
        &[
            response("user", "Invent a story"),
            response("assistant", "A paper bird flew."),
            row(
                "event_msg",
                json!({"type":"token_count","info":{"total_token_usage":usage}}),
            ),
            row(
                "event_msg",
                json!({"type":"token_count","info":{"total_token_usage":usage}}),
            ),
            row(
                "token_usage_record",
                json!({"thread_token_usage":{"input_tokens":120,"output_tokens":40,"cached_input_tokens":70,"total_tokens":160}}),
            ),
        ],
    );
    let index = load_window(&path, 0, Some(0)).unwrap();
    assert!(index.messages.is_empty());
    assert_eq!(index.count, 2);
    assert_eq!(index.roles, ["user", "claude"]);
    assert_eq!(
        (
            index.input_tokens,
            index.output_tokens,
            index.cache_read_tokens,
            index.total_tokens
        ),
        (120, 40, 70, 160)
    );
    let window = load_window(&path, 1, Some(1)).unwrap();
    assert_eq!(window.start, 1);
    assert_eq!(window.messages[0].content, "A paper bird flew.");
    assert_eq!(window.revision, index.revision);
    assert_eq!(
        load_window(&path, usize::MAX, Some(usize::MAX))
            .unwrap()
            .start,
        2
    );
}

#[test]
fn polling_sees_appends_same_count_result_changes_and_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    let call = row(
        "response_item",
        json!({"type":"function_call","name":"demo","call_id":"fictional-call","arguments":"{}"}),
    );
    write_rows(
        &path,
        &[
            call.clone(),
            row("event_msg", json!({"type":"task_started"})),
        ],
    );
    let before = load_window(&path, 0, None).unwrap();
    assert!(before.ongoing);
    write_rows(
        &path,
        &[
            call,
            row(
                "response_item",
                json!({"type":"function_call_output","call_id":"fictional-call","output":"a newly finished result"}),
            ),
            row("event_msg", json!({"type":"task_complete"})),
        ],
    );
    let after = load_window(&path, 0, None).unwrap();
    assert_eq!(before.count, after.count);
    assert_ne!(before.revision, after.revision);
    assert_eq!(
        after.messages[0].items[0].tool_result,
        "a newly finished result"
    );
    assert!(!after.ongoing);
    write_rows(&path, &[response("user", "A restarted fictional chat")]);
    assert_eq!(
        load_window(&path, 0, None).unwrap().messages[0].content,
        "A restarted fictional chat"
    );
}

#[test]
fn malformed_and_partial_lines_do_not_hide_valid_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-demo.jsonl");
    fs::write(
        &path,
        format!(
            "not JSON\n{}\n{{\"type\":",
            response("user", "A valid fictional prompt")
        ),
    )
    .unwrap();
    assert_eq!(load_window(&path, 0, None).unwrap().count, 1);
    let mut partial_utf8 = format!(
        "{}\n{{\"content\":\"",
        response("user", "Still a valid fictional prompt")
    )
    .into_bytes();
    partial_utf8.extend_from_slice(&[0xe2, 0x82]);
    fs::write(&path, partial_utf8).unwrap();
    assert_eq!(load_window(&path, 0, None).unwrap().count, 1);
    assert!(load_window(tmp.path(), 0, None).is_err());
    assert!(load_window(&tmp.path().join("missing.jsonl"), 0, None).is_err());
}

#[test]
fn discovery_merges_active_and_archived_uses_names_and_ignores_non_rollouts() {
    let tmp = tempfile::tempdir().unwrap();
    let id = "00000000-0000-0000-0000-000000000001";
    let file = format!("rollout-2026-01-02T03-04-05-{id}.jsonl");
    let rows = [
        row(
            "session_meta",
            json!({"id":id,"session_id":"transport-id","cwd":"/fictional/project"}),
        ),
        response("user", "# AGENTS.md instructions for an imaginary project"),
        response("user", "Build a fictional tiny game"),
        row(
            "turn_context",
            json!({"model":"gpt-demo","cwd":"/fictional/project"}),
        ),
    ];
    let active = tmp.path().join("sessions/2026/01/02").join(&file);
    write_rows(&active, &rows);
    write_rows(&tmp.path().join("archived_sessions").join(&file), &rows);
    write_rows(&tmp.path().join("sessions/history.jsonl"), &rows);
    fs::write(tmp.path().join("session_index.jsonl"), json!({"id":id,"thread_name":"A fabricated saved name","updated_at":"2026-01-03T00:00:00Z"}).to_string()).unwrap();
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, id);
    assert_eq!(sessions[0].title, "A fabricated saved name");
    assert!(!sessions[0].archived);
    assert_eq!(sessions[0].cwd, "/fictional/project");
    assert_eq!(sessions[0].model, "gpt-demo");
    fs::remove_file(tmp.path().join("session_index.jsonl")).unwrap();
    assert_eq!(
        discover_sessions(&roots)[0].title,
        "Build a fictional tiny game"
    );
    fs::remove_file(&active).unwrap();
    assert!(discover_sessions(&roots)[0].archived);
}

#[test]
fn plugin_toggle_and_custom_root_keep_other_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let mut settings = crate::settings::Settings::default();
    assert!(!crate::plugins::is_enabled(&settings, super::super::ID));
    super::super::set_root(
        &mut settings,
        Some(tmp.path().to_string_lossy().into_owned()),
    )
    .unwrap();
    crate::plugins::set_enabled(&mut settings, super::super::ID, true).unwrap();
    assert!(super::super::set_root(
        &mut settings,
        Some(tmp.path().join("missing").to_string_lossy().into_owned())
    )
    .is_err());
    assert!(super::super::settings_view(&settings).root.is_some());
    super::super::set_root(&mut settings, Some(" ".into())).unwrap();
    assert!(crate::plugins::is_enabled(&settings, super::super::ID));
    assert!(!settings.plugins["codex"].extra.contains_key("root"));
}

#[test]
fn discovery_uses_real_prompt_after_control_messages_and_ignores_generated_control_names() {
    let tmp = tempfile::tempdir().unwrap();
    let id = "00000000-0000-0000-0000-000000000201";
    let path = tmp.path().join(format!("sessions/rollout-{id}.jsonl"));
    let control =
        "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>";
    write_rows(
        &path,
        &[
            row(
                "session_meta",
                json!({"id":id,"source":"vscode","cwd":"/fictional/project"}),
            ),
            response("user", "# AGENTS.md instructions for an invented project"),
            response("user", control),
            response("user", "Build a fabricated puzzle game"),
            row(
                "event_msg",
                json!({"type":"user_message","message":"Build a fabricated puzzle game"}),
            ),
            row(
                "event_msg",
                json!({"type":"user_message","message":"A later fictional follow-up"}),
            ),
            row("turn_context", json!({"model":"gpt-6.1-sol"})),
        ],
    );
    fs::write(
        tmp.path().join("session_index.jsonl"),
        json!({"id":id,"thread_name":control}).to_string(),
    )
    .unwrap();
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title, "Build a fabricated puzzle game");
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(
        window
            .messages
            .iter()
            .find(|message| message.content == control)
            .unwrap()
            .role,
        "system"
    );
}

#[test]
fn discovery_filters_background_agents_without_merging_independent_same_title_chats() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = "00000000-0000-0000-0000-000000000211";
    let child = "00000000-0000-0000-0000-000000000212";
    let fork = "00000000-0000-0000-0000-000000000213";
    let legacy_child = "00000000-0000-0000-0000-000000000214";
    let future_source = "00000000-0000-0000-0000-000000000215";
    for (id, source, forked_from) in [
        (parent, json!("vscode"), Value::Null),
        (
            child,
            json!({"subagent":{"thread_spawn":{"parent_thread_id":parent,"depth":1}}}),
            json!(parent),
        ),
        (fork, json!("vscode"), json!(parent)),
        (legacy_child, json!("subagent"), json!(parent)),
        (future_source, json!({"future_source":{}}), Value::Null),
    ] {
        write_rows(
            &tmp.path().join(format!("sessions/rollout-{id}.jsonl")),
            &[
                row(
                    "session_meta",
                    json!({"id":id,"source":source,"forked_from_id":forked_from,"cwd":"/fictional/project"}),
                ),
                // Later metadata can be inherited from a different rollout. It
                // must not change whether the file's own session is a user chat.
                row(
                    "session_meta",
                    json!({"id":child,"source":{"subagent":"review"}}),
                ),
                response("user", "An identical fabricated prompt"),
                row("turn_context", json!({"model":"gpt-6.1-sol"})),
            ],
        );
    }
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    let ids = sessions
        .iter()
        .map(|session| session.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids, [parent, fork, future_source].into_iter().collect());
    assert!(sessions
        .iter()
        .all(|session| session.title == "An identical fabricated prompt"));
    assert!(
        load_window(
            &tmp.path().join(format!("sessions/rollout-{child}.jsonl")),
            0,
            None
        )
        .is_ok(),
        "filtering the picker does not remove access to a rollout"
    );
}

#[test]
fn discovery_preserves_real_text_after_known_control_blocks_and_other_prompt_markup() {
    let tmp = tempfile::tempdir().unwrap();
    let cases = [
        ("201", "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>\n<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>\nInvestigate a fictional display bug", "Investigate a fictional display bug"),
        ("202", "<instruction>Keep this fictional user markup</instruction>", "<instruction>Keep this fictional user markup</instruction>"),
        ("203", "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>", "203"),
    ];
    for (id, prompt, _) in cases {
        write_rows(
            &tmp.path()
                .join(format!("sessions/rollout-fictional-{id}.jsonl")),
            &[
                row("session_meta", json!({"id":id,"source":"vscode"})),
                row("turn_context", json!({"model":"gpt-6.1-sol"})),
                response("user", prompt),
            ],
        );
    }
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    assert_eq!(sessions.len(), cases.len());
    for (id, _, expected) in cases {
        assert_eq!(
            sessions
                .iter()
                .find(|session| session.id == id)
                .unwrap()
                .title,
            expected
        );
    }
}

#[test]
fn costs_are_shared_by_discovery_and_windows_and_update_without_new_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-cost.jsonl");
    let mut rows = vec![
        row(
            "session_meta",
            json!({"id":"fictional-cost","cwd":"/example/cost"}),
        ),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
        response("user", "A fabricated cost example"),
        row(
            "token_usage_record",
            json!({"response_id":"fictional-response-1","usage":{
                "input_tokens":100000,"cached_input_tokens":20000,"cache_write_input_tokens":10000,
                "output_tokens":5000,"reasoning_output_tokens":1000,"total_tokens":105000
            }}),
        ),
    ];
    write_rows(&path, &rows);
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let index = load_window(&path, 0, Some(0)).unwrap();
    // 70k ordinary input * $2/M + 20k cached * $.10/M + 10k writes * $2.50/M
    // + 5k output * $10/M. Reasoning is already included in output.
    let cost = serde_json::to_value(&index).unwrap()["cost_usd"]
        .as_f64()
        .unwrap();
    assert!((cost - 0.217).abs() < 1e-10);
    let sessions = discover_sessions(&roots);
    let summary = serde_json::to_value(&sessions[0]).unwrap();
    assert!((summary["cost_usd"].as_f64().unwrap() - cost).abs() < 1e-10);
    assert_eq!(summary["total_tokens"], 105000);

    rows.push(row(
        "token_usage_record",
        json!({"response_id":"fictional-response-2","usage":{
            "input_tokens":10000,"cached_input_tokens":0,"cache_write_input_tokens":0,
            "output_tokens":1000,"total_tokens":11000
        }}),
    ));
    write_rows(&path, &rows);
    let updated = load_window(&path, 0, Some(0)).unwrap();
    assert_eq!(updated.count, index.count);
    assert_ne!(updated.revision, index.revision);
    let updated = serde_json::to_value(&updated).unwrap();
    assert!((updated["cost_usd"].as_f64().unwrap() - 0.247).abs() < 1e-10);
    let summary = serde_json::to_value(&discover_sessions(&roots)[0]).unwrap();
    assert_eq!(summary["cost_usd"], updated["cost_usd"]);
}

#[test]
fn unsupported_model_usage_is_unavailable_instead_of_a_free_or_partial_cost() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-unknown-cost.jsonl");
    write_rows(
        &path,
        &[
            row("turn_context", json!({"model":"gpt-6.1-sol"})),
            row(
                "token_usage_record",
                json!({"response_id":"fictional-known","usage":{"input_tokens":100,"output_tokens":10}}),
            ),
            row("turn_context", json!({"model":"fictional-unpriced-model"})),
            row(
                "token_usage_record",
                json!({"response_id":"fictional-unknown","usage":{"input_tokens":100,"output_tokens":10}}),
            ),
        ],
    );
    let window = serde_json::to_value(load_window(&path, 0, None).unwrap()).unwrap();
    assert!(window.as_object().unwrap().contains_key("cost_usd"));
    assert!(window["cost_usd"].is_null());
}

fn at(time: &str, mut row: Value) -> Value {
    row["timestamp"] = json!(time);
    row
}

#[test]
fn metadata_preserves_origin_and_fork_identity_but_tracks_later_working_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-metadata.jsonl");
    write_rows(
        &path,
        &[
            at(
                "2026-02-02T00:00:00Z",
                row(
                    "session_meta",
                    json!({
                        "id":"fictional-metadata", "cwd":"C:\\invented\\origin", "forked_from_id":"fictional-parent",
                        "git":{"branch":"fictional-feature"}, "timestamp":"2026-02-01T23:59:00Z"
                    }),
                ),
            ),
            // Resumed/forked histories can contain another session's metadata.
            row(
                "session_meta",
                json!({"id":"inherited-parent", "cwd":"/inherited/other", "git":{"branch":"wrong-branch"}, "forked_from_id":"wrong-parent"}),
            ),
            response("user", "A fabricated metadata request"),
            row(
                "event_msg",
                json!({"type":"user_message", "message":"A fabricated metadata request"}),
            ),
            row(
                "turn_context",
                json!({"cwd":"C:\\invented\\second", "model":"gpt-6.1-sol", "approval_policy":"on-request", "sandbox_policy":{"type":"workspace-write"}}),
            ),
            response("assistant", "An invented answer"),
            at(
                "2026-02-02T00:01:00Z",
                row(
                    "turn_context",
                    json!({"cwd":"C:\\invented\\origin", "model":"gpt-6.1-terra", "approval_policy":"never", "sandbox_policy":{"type":"read-only"}}),
                ),
            ),
        ],
    );
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    let session = &sessions[0];
    assert_eq!(session.cwd, "C:\\invented\\origin");
    assert_eq!(
        session.dirs,
        ["C:\\invented\\origin", "C:\\invented\\second"]
    );
    assert_eq!(
        session.forked_from_session_id.as_deref(),
        Some("fictional-parent")
    );
    assert_eq!(session.git_branch, "fictional-feature");
    assert_eq!(session.model, "gpt-6.1-terra");
    assert_eq!(session.permission_mode, "never");
    assert_eq!(session.sandbox_policy, "read-only");
    assert_eq!(session.started_at, "2026-02-01T23:59:00+00:00");
    assert_eq!(session.last_activity_at, "2026-02-02T00:01:00+00:00");
    assert_eq!(session.turn_count, 2);
    assert_eq!(session.duration_ms, None);
    assert_eq!(session.context_tokens, None);
    let window = load_window(&path, 0, Some(0)).unwrap();
    assert_eq!(window.model, session.model);
    assert_eq!(window.git_branch, session.git_branch);
    assert_eq!(window.permission_mode, session.permission_mode);
    assert_eq!(window.started_at, session.started_at);
    assert_eq!(window.turn_count, session.turn_count);
}

#[test]
fn lifecycle_duration_excludes_idle_gaps_and_counts_modern_and_legacy_turns() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-timing.jsonl");
    write_rows(
        &path,
        &[
            at(
                "2026-02-02T00:00:00Z",
                row(
                    "session_meta",
                    json!({"id":"fictional-timing", "cwd":"/invented/project"}),
                ),
            ),
            at(
                "2026-02-02T00:00:01Z",
                row("event_msg", json!({"type":"task_started"})),
            ),
            at(
                "2026-02-02T00:00:01Z",
                row("event_msg", json!({"type":"turn_started"})),
            ),
            at(
                "2026-02-02T00:00:01Z",
                response("user", "First invented turn"),
            ),
            at(
                "2026-02-02T00:00:03Z",
                response("assistant", "First invented answer"),
            ),
            at(
                "2026-02-02T00:00:04Z",
                row("event_msg", json!({"type":"task_complete"})),
            ),
            at(
                "2026-02-02T02:00:00Z",
                row("event_msg", json!({"type":"turn_started"})),
            ),
            at(
                "2026-02-02T02:00:00Z",
                row(
                    "event_msg",
                    json!({"type":"user_message", "message":"Second invented turn"}),
                ),
            ),
            at(
                "2026-02-02T02:00:02Z",
                row(
                    "event_msg",
                    json!({"type":"agent_message", "message":"Second invented answer"}),
                ),
            ),
            at(
                "2026-02-02T02:00:04Z",
                row("event_msg", json!({"type":"turn_aborted"})),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.duration_ms, Some(7_000));
    assert_eq!(window.turn_count, 4);
    assert!(!window.ongoing);
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let session = &discover_sessions(&roots)[0];
    assert_eq!(session.duration_ms, Some(7_000));
    assert_eq!(session.turn_count, 4);
    assert!(!session.is_ongoing);
}

#[test]
fn unfinished_turn_uses_recorded_activity_and_final_response_stops_it() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-active.jsonl");
    let mut rows = vec![
        at(
            "2026-02-02T00:00:00Z",
            row("session_meta", json!({"id":"fictional-active"})),
        ),
        at(
            "2026-02-02T00:00:00Z",
            row("event_msg", json!({"type":"task_started"})),
        ),
        at(
            "2026-02-02T00:00:02Z",
            response("assistant", "A fabricated interim update"),
        ),
    ];
    write_rows(&path, &rows);
    let before = load_window(&path, 0, Some(0)).unwrap();
    assert!(before.ongoing);
    assert_eq!(before.duration_ms, Some(2_000));
    let mut final_row = response("assistant", "An invented final answer");
    final_row["payload"]["phase"] = json!("final_answer");
    rows.push(at("2026-02-02T00:00:05Z", final_row));
    write_rows(&path, &rows);
    let after = load_window(&path, 0, Some(0)).unwrap();
    assert!(!after.ongoing);
    assert_eq!(after.duration_ms, Some(5_000));
}

#[test]
fn context_is_latest_request_total_and_cumulative_thread_usage_is_never_a_gauge() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-context.jsonl");
    write_rows(
        &path,
        &[
            row("session_meta", json!({"id":"fictional-context"})),
            row("turn_context", json!({"model":"gpt-6.1-sol"})),
            response("assistant", "A fabricated response"),
            row(
                "token_usage_record",
                json!({"response_id":"fictional-response", "usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100}, "thread_token_usage":{"input_tokens":8000,"cached_input_tokens":400,"output_tokens":500,"total_tokens":8500}}),
            ),
            row(
                "event_msg",
                json!({"type":"token_count", "info":{"total_token_usage":{"input_tokens":8000,"cached_input_tokens":400,"output_tokens":500,"total_tokens":8500},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100},"model_context_window":64000}}),
            ),
            // A later aggregate-only snapshot must not replace the per-request gauge.
            row(
                "event_msg",
                json!({"type":"token_count", "info":{"total_token_usage":{"input_tokens":9000,"cached_input_tokens":400,"output_tokens":600,"total_tokens":9600}}}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.context_tokens, Some(1100));
    assert_eq!(window.context_window, Some(64000));
    assert_eq!(window.total_tokens, 9600);
    assert_eq!(window.messages[0].input_tokens, 1000);
    assert_eq!(window.messages[0].cache_read_tokens, 400);
    assert_eq!(window.messages[0].tokens_raw, 1100);
    assert_eq!(window.messages[0].context_tokens, 1100);
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let session = &discover_sessions(&roots)[0];
    assert_eq!(session.context_tokens, Some(1100));
    assert_eq!(session.context_window, Some(64000));
}

#[test]
fn usage_between_display_twins_survives_dedup_and_modern_corrections_keep_their_message() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-fictional-request-stats.jsonl");
    write_rows(
        &path,
        &[
            row("turn_context", json!({"model":"gpt-6.1-sol"})),
            row(
                "event_msg",
                json!({"type":"agent_message", "message":"An invented first answer"}),
            ),
            row(
                "token_usage_record",
                json!({"response_id":"fictional-first", "usage":{"input_tokens":100,"output_tokens":10}}),
            ),
            response("assistant", "An invented first answer"),
            response("user", "An invented next request"),
            response("assistant", "An invented second answer"),
            row(
                "token_usage_record",
                json!({"response_id":"fictional-first", "usage":{"input_tokens":120,"output_tokens":12}}),
            ),
        ],
    );
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.count, 3);
    assert_eq!(window.messages[0].tokens_raw, 132);
    assert_eq!(window.messages[0].context_tokens, 132);
    assert_eq!(
        window.messages[2].tokens_raw, 0,
        "A correction belongs to its original response"
    );
}

#[test]
fn picker_scan_tracks_metadata_after_the_old_prefix_limit_and_unknown_context_stays_null() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp
        .path()
        .join("sessions/rollout-fictional-long-index.jsonl");
    let mut rows = vec![
        row(
            "session_meta",
            json!({"id":"fictional-long-index", "cwd":"/invented/origin"}),
        ),
        response("user", "A fabricated long transcript"),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
    ];
    rows.extend((0..520).map(|_| row("event_msg", json!({"type":"ignored_fictional_event"}))));
    rows.push(row(
        "turn_context",
        json!({"model":"gpt-6.1-terra", "cwd":"/invented/other"}),
    ));
    rows.push(row("event_msg", json!({"type":"token_count", "info":{"total_token_usage":{"input_tokens":5000,"output_tokens":500,"total_tokens":5500}, "model_context_window":0}})));
    write_rows(&path, &rows);
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let session = &discover_sessions(&roots)[0];
    assert_eq!(session.model, "gpt-6.1-terra");
    assert_eq!(session.cwd, "/invented/origin");
    assert_eq!(session.dirs, ["/invented/origin", "/invented/other"]);
    assert_eq!(session.context_tokens, None);
    assert_eq!(session.context_window, None);
    assert_eq!(session.duration_ms, None);
}

fn spawn(call: &str, child: &str, prompt: &str) -> [Value; 2] {
    [
        row(
            "response_item",
            json!({"type":"function_call", "namespace":"functions", "name":"spawn_agent", "call_id":call, "arguments":json!({"message":prompt,"agent_type":"reviewer"}).to_string()}),
        ),
        row(
            "response_item",
            json!({"type":"function_call_output", "call_id":call, "output":json!({"agent_id":child}).to_string()}),
        ),
    ]
}

fn usage(id: &str, input: i64, output: i64) -> Value {
    row(
        "token_usage_record",
        json!({"response_id":id,"usage":{"input_tokens":input,"output_tokens":output,"total_tokens":input+output}}),
    )
}

fn child_rows(id: &str, parent: &str, prompt: &str) -> Vec<Value> {
    let mut inherited = response("user", prompt);
    inherited["metadata"] = json!({"inherited_user_message":true});
    vec![
        row(
            "session_meta",
            json!({"id":id,"cwd":"/invented/project","source":{"subagent":{"thread_spawn":{"parent_thread_id":parent,"agent_nickname":"Fictional Reviewer","depth":1}}}}),
        ),
        // Legacy snapshots and even matching prompts copied from a parent must
        // contribute neither child work nor child spend.
        inherited,
        row(
            "event_msg",
            json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"output_tokens":100000,"total_tokens":1100000}}}),
        ),
        response("assistant", "An inherited fictional parent answer"),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
        row("event_msg", json!({"type":"task_started"})),
        response("user", prompt),
        response("assistant", "A fictional child answer"),
        usage("fictional-child-response", 500, 50),
        row("event_msg", json!({"type":"task_complete"})),
    ]
}

#[test]
fn explicit_children_expand_and_contribute_spend_once_without_inherited_history() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = "00000000-0000-0000-0000-000000000301";
    let child = "00000000-0000-0000-0000-000000000302";
    let prompt = "Review this fabricated widget";
    let path = tmp.path().join(format!("sessions/rollout-{parent}.jsonl"));
    let child_path = tmp
        .path()
        .join(format!("archived_sessions/rollout-{child}.jsonl"));
    let mut rows = vec![
        row(
            "session_meta",
            json!({"id":parent,"cwd":"/invented/project"}),
        ),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
        response("user", "An invented parent task"),
    ];
    rows.extend(spawn("fictional-spawn-1", child, prompt));
    // Repeated references to an identical child must not charge it twice.
    rows.extend(spawn("fictional-spawn-2", child, prompt));
    rows.push(response("assistant", "A fictional parent answer"));
    rows.push(usage("fictional-parent-response", 1000, 100));
    write_rows(&path, &rows);
    write_rows(&child_path, &child_rows(child, parent, prompt));
    let window = load_window(&path, 0, None).unwrap();
    assert_eq!(window.total_tokens, 1650);
    assert!((window.cost_usd.unwrap() - 0.0045).abs() < 1e-12);
    assert!(!window.cost_partial);
    let item = &window.messages[1].items[0];
    assert_eq!(item.item_type, "Subagent");
    assert_eq!(item.tool_category, "Task");
    assert_eq!(item.agent_id, child);
    assert_eq!(item.subagent_desc, "Fictional Reviewer");
    assert_eq!(item.subagent_prompt, prompt);
    assert_eq!(item.token_count, 550);
    assert_eq!(item.subagent_messages.len(), 2);
    assert_eq!(
        item.subagent_messages[1].content,
        "A fictional child answer"
    );
    assert!(!serde_json::to_string(&item.subagent_messages)
        .unwrap()
        .contains("inherited fictional"));
    assert!(
        !item.tool_result.is_empty(),
        "The original spawn result remains inspectable"
    );
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    assert_eq!(sessions.len(), 1, "Workers stay out of the picker");
    assert_eq!(sessions[0].total_tokens, window.total_tokens);
    assert_eq!(sessions[0].cost_usd, window.cost_usd);
    assert_eq!(sessions[0].cost_partial, window.cost_partial);
}

#[test]
fn child_appends_invalidate_parent_revision_and_picker_totals_without_parent_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = "00000000-0000-0000-0000-000000000311";
    let child = "00000000-0000-0000-0000-000000000312";
    let prompt = "Inspect an invented component";
    let path = tmp.path().join(format!("sessions/rollout-{parent}.jsonl"));
    let child_path = tmp.path().join(format!("sessions/rollout-{child}.jsonl"));
    let mut rows = vec![
        row("session_meta", json!({"id":parent})),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
    ];
    rows.extend(spawn("fictional-live-spawn", child, prompt));
    rows.push(usage("fictional-parent", 1000, 100));
    write_rows(&path, &rows);
    let missing = load_window(&path, 0, Some(0)).unwrap();
    assert!((missing.cost_usd.unwrap() - 0.003).abs() < 1e-12);
    assert!(
        missing.cost_partial,
        "Missing worker usage marks the known parent subtotal incomplete"
    );
    let mut children = child_rows(child, parent, prompt);
    write_rows(&child_path, &children);
    let before = load_window(&path, 0, Some(0)).unwrap();
    assert_ne!(missing.revision, before.revision);
    assert_eq!(before.total_tokens, 1650);
    assert!(!before.cost_partial);
    children.push(row("event_msg", json!({"type":"task_started"})));
    children.push(response("user", "Continue this fictional review"));
    children.push(response("assistant", "A fictional additional finding"));
    children.push(usage("fictional-child-next", 200, 20));
    write_rows(&child_path, &children);
    let after = load_window(&path, 0, Some(0)).unwrap();
    assert_eq!(after.count, before.count);
    assert_ne!(after.revision, before.revision);
    assert!(
        after.ongoing,
        "A running child keeps its parent activity visible"
    );
    assert_eq!(after.total_tokens, 1870);
    assert!((after.cost_usd.unwrap() - 0.0051).abs() < 1e-12);
    let detail = load_window(&path, 0, None).unwrap();
    assert!(detail.messages[0].items[0].subagent_ongoing);
    assert_eq!(detail.messages[0].items[0].subagent_messages.len(), 4);
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    assert_eq!(discover_sessions(&roots)[0].cost_usd, after.cost_usd);
}

#[test]
fn child_linking_requires_parent_identity_local_task_and_same_codex_home() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = "00000000-0000-0000-0000-000000000321";
    let child = "00000000-0000-0000-0000-000000000322";
    let path = tmp
        .path()
        .join(format!("first/sessions/rollout-{parent}.jsonl"));
    let child_path = tmp
        .path()
        .join(format!("first/sessions/rollout-{child}.jsonl"));
    let prompt = "Review a fictional task";
    let mut rows = vec![
        row("session_meta", json!({"id":parent})),
        row("turn_context", json!({"model":"gpt-6.1-sol"})),
    ];
    rows.extend(spawn("fictional-strict-spawn", child, prompt));
    rows.push(usage("fictional-strict-parent", 100, 10));
    write_rows(&path, &rows);
    write_rows(
        &tmp.path()
            .join(format!("second/sessions/rollout-{child}.jsonl")),
        &child_rows(child, parent, prompt),
    );
    let outside = load_window(&path, 0, None).unwrap();
    assert_eq!(outside.messages[0].items[0].item_type, "ToolCall");
    assert!((outside.cost_usd.unwrap() - 0.0003).abs() < 1e-12);
    assert!(outside.cost_partial);
    for children in [
        child_rows(child, "another-fictional-parent", prompt),
        child_rows(child, parent, "An unrelated fictional task"),
        {
            let mut rows = child_rows(child, parent, prompt);
            rows[0]["payload"]["source"] = json!("vscode");
            rows
        },
        {
            let mut rows = child_rows(child, parent, prompt);
            rows[6]["metadata"] = json!({"inherited_user_message":true});
            rows
        },
    ] {
        write_rows(&child_path, &children);
        let window = load_window(&path, 0, None).unwrap();
        assert_eq!(window.messages[0].items[0].item_type, "ToolCall");
        assert!(window.messages[0].items[0].subagent_messages.is_empty());
        assert_eq!(window.total_tokens, 110);
        assert!((window.cost_usd.unwrap() - 0.0003).abs() < 1e-12);
        assert!(window.cost_partial);
    }
}

#[test]
fn nested_children_are_bounded_and_cycles_never_recurse_or_double_charge() {
    let tmp = tempfile::tempdir().unwrap();
    let ids = (330..336)
        .map(|id| format!("00000000-0000-0000-0000-{id:012}"))
        .collect::<Vec<_>>();
    let prompt = "An invented nested task";
    for (index, id) in ids.iter().enumerate() {
        let parent = &ids[if index == 0 { ids.len() - 1 } else { index - 1 }];
        let mut rows = child_rows(id, parent, prompt);
        rows.extend(spawn(
            &format!("fictional-nested-{index}"),
            &ids[(index + 1) % ids.len()],
            prompt,
        ));
        write_rows(
            &tmp.path().join(format!("sessions/rollout-{id}.jsonl")),
            &rows,
        );
    }
    let path = tmp
        .path()
        .join(format!("sessions/rollout-{}.jsonl", ids[0]));
    let window = load_window(&path, 0, None).unwrap();
    let mut messages = &window.messages;
    let mut depth = 0;
    while let Some(item) = messages
        .iter()
        .flat_map(|message| &message.items)
        .find(|item| item.item_type == "Subagent")
    {
        depth += 1;
        messages = &item.subagent_messages;
    }
    assert_eq!(depth, 4);
    assert!((window.cost_usd.unwrap() - 0.006).abs() < 1e-12);
    assert!(
        window.cost_partial,
        "Unknown work beyond the depth bound marks the subtotal incomplete"
    );
    // Close a short explicit cycle, independent of the depth guard.
    let mut rows = child_rows(&ids[1], &ids[0], prompt);
    rows.extend(spawn("fictional-cycle", &ids[0], prompt));
    write_rows(
        &tmp.path()
            .join(format!("sessions/rollout-{}.jsonl", ids[1])),
        &rows,
    );
    let window = load_window(&path, 0, None).unwrap();
    assert!((window.cost_usd.unwrap() - 0.0015).abs() < 1e-12);
    assert!(window.cost_partial);
    assert_eq!(
        window.messages.last().unwrap().items[0]
            .subagent_messages
            .last()
            .unwrap()
            .items[0]
            .item_type,
        "ToolCall"
    );
}

#[test]
fn unchanged_oversized_rollout_index_polls_do_not_reparse_its_transcript() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rollout-fictional-oversized.jsonl");
    write_rows(
        &path,
        &[response("user", "A fictional large-file cache check")],
    );
    // Exceed the bounded full-body parse cache using an ignored, fabricated row.
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(17 * 1024 * 1024)
        .unwrap();
    let cache = Mutex::new(None);
    let before = PARSE_CALLS.with(std::cell::Cell::get);
    let first = load_window_cached(&path, 0, Some(0), &cache).unwrap();
    assert_eq!(first.count, 1);
    assert_eq!(PARSE_CALLS.with(std::cell::Cell::get), before + 1);
    let again = load_window_cached(&path, 1, Some(0), &cache).unwrap();
    assert_eq!(again.count, first.count);
    assert_eq!(again.start, 1);
    assert_eq!(PARSE_CALLS.with(std::cell::Cell::get),before+1,"A metadata-only poll must use the lightweight index even when the full transcript is too large to cache");
}

#[test]
fn invalid_utf8_row_does_not_remove_a_valid_chat_from_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions/rollout-fictional-bad-row.jsonl");
    write_rows(
        &path,
        &[
            row(
                "session_meta",
                json!({"id":"fictional-bad-row","cwd":"/invented/project"}),
            ),
            response("user", "An invented resilient reader request"),
        ],
    );
    let mut content = fs::read(&path).unwrap();
    content.extend_from_slice(b"\xff\xfe\n");
    content.extend_from_slice(
        row("turn_context", json!({"model":"gpt-6.1-sol"}))
            .to_string()
            .as_bytes(),
    );
    fs::write(&path, content).unwrap();
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    let sessions = discover_sessions(&roots);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title, "An invented resilient reader request");
    assert_eq!(sessions[0].model, "gpt-6.1-sol");
}

#[test]
fn partial_cost_preserves_known_parent_or_child_spend_and_fully_unknown_stays_null() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = "00000000-0000-0000-0000-000000000351";
    let child = "00000000-0000-0000-0000-000000000352";
    let prompt = "A fabricated partial-cost task";
    let path = tmp.path().join(format!("sessions/rollout-{parent}.jsonl"));
    let child_path = tmp.path().join(format!("sessions/rollout-{child}.jsonl"));
    let roots = [CodexRoot {
        path: tmp.path().to_string_lossy().into_owned(),
        source: "custom".into(),
        exists: true,
    }];
    for (parent_model, child_model, expected) in [
        ("fictional-unpriced-model", "gpt-6.1-sol", Some(0.0015)),
        ("gpt-6.1-sol", "fictional-unpriced-model", Some(0.0003)),
        ("fictional-unpriced-model", "fictional-unpriced-model", None),
    ] {
        let mut rows = vec![
            row("session_meta", json!({"id":parent})),
            row("turn_context", json!({"model":parent_model})),
        ];
        rows.extend(spawn("fictional-partial-spawn", child, prompt));
        rows.push(usage("fictional-partial-parent", 100, 10));
        write_rows(&path, &rows);
        let mut children = child_rows(child, parent, prompt);
        children[4]["payload"]["model"] = json!(child_model);
        write_rows(&child_path, &children);
        let window = load_window(&path, 0, Some(0)).unwrap();
        assert!(window.cost_partial);
        match expected {
            Some(expected) => assert!((window.cost_usd.unwrap() - expected).abs() < 1e-12),
            None => assert_eq!(window.cost_usd, None),
        }
        let cached = load_window(&path, 0, Some(0)).unwrap();
        assert_eq!(cached.cost_usd, window.cost_usd);
        assert_eq!(cached.cost_partial, window.cost_partial);
        let sessions = discover_sessions(&roots);
        assert_eq!(sessions[0].cost_usd, window.cost_usd);
        assert_eq!(sessions[0].cost_partial, window.cost_partial);
    }
}
