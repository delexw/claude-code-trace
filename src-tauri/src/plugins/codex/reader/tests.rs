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
