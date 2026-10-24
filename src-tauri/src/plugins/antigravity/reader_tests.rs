//! Reader tests ported from #353 (Bandar Alsehli): roots, scanning, titles,
//! merging, the transcript index, windows and durations.

use std::fs;
use std::path::{Path, PathBuf};

use super::roots::*;
use super::test_support::*;
use super::transcript::*;

fn candidate_roots(home: &Path) -> Vec<BrainRoot> {
    brain_root_candidates(None, None, Some(&home.to_string_lossy()))
}

#[test]
fn root_prefers_explicit_override() {
    let r = antigravity_brain_root(Some("/custom/brain"), None, Some("/home/u"));
    assert_eq!(r, Some(PathBuf::from("/custom/brain")));
}

#[test]
fn root_uses_gemini_home_when_set() {
    let r = antigravity_brain_root(None, Some("/data/g"), Some("/home/u"));
    assert_eq!(
        r,
        Some(PathBuf::from("/data/g/.gemini/antigravity-ide/brain"))
    );
}

#[test]
fn root_falls_back_to_os_home() {
    let r = antigravity_brain_root(None, None, Some("/home/u"));
    assert_eq!(
        r,
        Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
    );
}

#[test]
fn empty_override_falls_through_to_home() {
    let r = antigravity_brain_root(Some("   "), None, Some("/home/u"));
    assert_eq!(
        r,
        Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
    );
}

#[test]
fn root_is_none_without_any_home() {
    assert_eq!(antigravity_brain_root(None, None, None), None);
    assert_eq!(antigravity_brain_root(Some("  "), None, None), None);
}

// ---- flavor candidates ------------------------------------------------

#[test]
fn candidates_cover_both_flavors_newest_first() {
    let c = candidate_roots(Path::new("/home/u"));
    assert_eq!(c.len(), 2, "got {c:#?}");
    assert_eq!(c[0].flavor, "antigravity-ide");
    assert_eq!(c[1].flavor, "antigravity");
    assert_eq!(c[0].path, "/home/u/.gemini/antigravity-ide/brain");
    assert_eq!(c[1].path, "/home/u/.gemini/antigravity/brain");
}

#[test]
fn candidates_exclude_the_backup_snapshot() {
    // `antigravity-backup` is a snapshot of the old flavor, not a separate
    // install. Probing it would list every old conversation twice, so it
    // must never appear as a candidate.
    let roots = candidate_roots(Path::new("/home/u"));
    let flavors: Vec<&str> = roots.iter().map(|r| r.flavor.as_str()).collect();
    assert!(!flavors.iter().any(|f| f.contains("backup")), "{flavors:?}");
    assert_eq!(ANTIGRAVITY_FLAVORS, &["antigravity-ide", "antigravity"]);
}

#[test]
fn gemini_home_pointing_at_dot_gemini_is_not_doubled() {
    // GEMINI_HOME conventionally names the `.gemini` directory itself, so
    // joining `.gemini` again would produce `…/.gemini/.gemini/…`.
    let r = antigravity_brain_root(None, Some("/data/g/.gemini"), Some("/home/u"));
    assert_eq!(
        r,
        Some(PathBuf::from("/data/g/.gemini/antigravity-ide/brain"))
    );
}

#[test]
fn os_home_never_doubles_the_gemini_segment() {
    let r = antigravity_brain_root(None, None, Some("/home/u/.gemini"));
    assert_eq!(
        r,
        Some(PathBuf::from("/home/u/.gemini/antigravity-ide/brain"))
    );
}

#[test]
fn override_short_circuits_flavor_probing() {
    let c = brain_root_candidates(Some("/custom/brain"), Some("/data/g"), Some("/home/u"));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].flavor, "custom");
    assert_eq!(c[0].path, "/custom/brain");
}

#[test]
fn candidates_report_existence() {
    let tmp = tmpdir("cctrace-antig-cand-exists");
    fs::create_dir_all(tmp.join(".gemini/antigravity/brain")).unwrap();
    let c = candidate_roots(&tmp);
    assert!(!c[0].exists, "antigravity-ide not created → false");
    assert!(c[1].exists, "antigravity created → true");
}

#[test]
fn scan_returns_empty_when_root_missing() {
    let tmp = std::env::temp_dir().join("cctrace-antig-missing");
    let _ = fs::remove_dir_all(&tmp);
    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert!(v.is_empty());
}

#[test]
fn scan_finds_folders_sorted_newest_first_with_task_title() {
    let tmp = std::env::temp_dir().join("cctrace-antig-scan");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let older = tmp.join("00000000-0000-0000-0000-000000000001");
    let newer = tmp.join("00000000-0000-0000-0000-000000000002");
    fs::create_dir_all(&older).unwrap();
    fs::create_dir_all(&newer).unwrap();
    fs::write(older.join("task.md"), "First older task\nmore lines").unwrap();
    fs::write(newer.join("task.md"), "Second newer task").unwrap();
    // Sleep so the newer folder has a strictly later mtime.
    std::thread::sleep(std::time::Duration::from_millis(10));

    let results = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].uuid, "00000000-0000-0000-0000-000000000002");
    assert_eq!(results[1].uuid, "00000000-0000-0000-0000-000000000001");
    assert_eq!(results[0].title.as_deref(), Some("Second newer task"));
    assert_eq!(results[1].title.as_deref(), Some("First older task"));
    assert!(results[0].mod_time_ms > 0);
    assert!(results[0].mod_time_ms >= results[1].mod_time_ms);
}

#[test]
fn scan_prefers_metadata_summary_over_task_md() {
    // Real Antigravity folders use `conversation_*.md.metadata.json`
    // rather than `task.md`. The metadata summary must win when both
    // are present (the summary describes the conversation; task.md
    // is whatever the user wrote there).
    let tmp = std::env::temp_dir().join("cctrace-antig-meta");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("33333333-3333-3333-3333-333333333333");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("task.md"), "ignored\n").unwrap();
    fs::write(
        folder.join("conversation_48eb.md.metadata.json"),
        r#"{"summary":"ملف مرجعي يعرض محتوى سجل المحادثة 48eb5442","updatedAt":"2026-09-30"}"#,
    )
    .unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    assert_eq!(
        v[0].title.as_deref(),
        Some("ملف مرجعي يعرض محتوى سجل المحادثة 48eb5442"),
    );
}

#[test]
fn scan_skips_empty_metadata_summary_and_falls_back_to_task() {
    // If metadata summary is empty/whitespace, fall through to task.md.
    let tmp = std::env::temp_dir().join("cctrace-antig-meta-empty");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("44444444-4444-4444-4444-444444444444");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("task.md"), "Fallback task title\n").unwrap();
    fs::write(
        folder.join("conversation_x.md.metadata.json"),
        r#"{"summary":"   ","userFacing":true}"#,
    )
    .unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].title.as_deref(), Some("Fallback task title"));
}

#[test]
fn scan_skips_files_and_unreadable_task_md() {
    let tmp = std::env::temp_dir().join("cctrace-antig-skip");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    // A stray file should be ignored (not a directory).
    fs::write(tmp.join("stray.txt"), "not a folder").unwrap();
    let only = tmp.join("11111111-1111-1111-1111-111111111111");
    fs::create_dir_all(&only).unwrap();
    // No task.md → title is None.

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].uuid, "11111111-1111-1111-1111-111111111111");
    assert!(v[0].title.is_none());
}

#[test]
fn task_title_truncates_at_80_chars() {
    let tmp = std::env::temp_dir().join("cctrace-antig-trunc");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let big = "a".repeat(200);
    let only = tmp.join("22222222-2222-2222-2222-222222222222");
    fs::create_dir_all(&only).unwrap();
    fs::write(only.join("task.md"), &big).unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].title.as_deref().map(str::len), Some(80));
}

#[test]
fn scan_extracts_title_from_first_user_input() {
    // Real-world layout: `.system_generated/logs/transcript.jsonl`
    // with a USER_INPUT line wrapping the request in `<USER_REQUEST>`.
    // The first non-`<USER_REQUEST>` line is an AGENT_OUTPUT that
    // should NOT be picked.
    let tmp = std::env::temp_dir().join("cctrace-antig-userinput");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("55555555-5555-5555-5555-555555555555");
    let logs = folder.join(".system_generated/logs");
    fs::create_dir_all(&logs).unwrap();

    let transcript = r#"{"step_index":0,"type":"AGENT_OUTPUT","content":"earlier output"}
{"step_index":1,"type":"USER_INPUT","content":"<USER_REQUEST>\n@[\\wsl.localhost\\Ubuntu\\root\\ArenaFlows_V5_Enhanced\\static\\picker.html:L52-L68]  ضيف سكشن ........ لاختيار .............................. ملف من المسار \\data\\parquet_db .. بديلا عن البيانات المخمرة\n</USER_REQUEST>\n<ADDITIONAL_METADATA>...</ADDITIONAL_METADATA>"}
"#;
    fs::write(logs.join("transcript.jsonl"), transcript).unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    let title = v[0].title.as_deref().expect("title present");
    // The @[…:L…-L…] anchor is stripped; the user's actual request
    // is what survives.
    assert!(title.contains("ضيف سكشن"), "got {title:?}");
    assert!(title.contains("parquet_db"), "got {title:?}");
    assert!(!title.contains("@["), "anchor leaked: {title:?}");
    // Truncated to 80 chars max.
    assert!(title.chars().count() <= 80);
}

#[test]
fn scan_falls_through_to_uuid_when_transcript_has_no_user_input() {
    let tmp = std::env::temp_dir().join("cctrace-antig-notranscript");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("66666666-6666-6666-6666-666666666666");
    let logs = folder.join(".system_generated/logs");
    fs::create_dir_all(&logs).unwrap();
    // Only an agent output, no user input.
    fs::write(
        logs.join("transcript.jsonl"),
        r#"{"step_index":0,"type":"AGENT_OUTPUT","content":"..."}"#,
    )
    .unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1);
    assert!(
        v[0].title.is_none(),
        "no USER_INPUT → title is None (UUID fallback in UI)"
    );
}

#[test]
fn load_transcript_returns_empty_when_transcript_missing() {
    let tmp = std::env::temp_dir().join("cctrace-antig-load-empty");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("77777777-7777-7777-7777-777777777777");
    fs::create_dir_all(&folder).unwrap();
    // No .system_generated/logs directory at all.
    assert!(load_antigravity_transcript(&folder).is_empty());
}

#[test]
fn load_transcript_maps_types_to_roles_and_skips_system_rows() {
    let tmp = std::env::temp_dir().join("cctrace-antig-load-roles");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let folder = tmp.join("88888888-8888-8888-8888-888888888888");
    let logs = folder.join(".system_generated/logs");
    fs::create_dir_all(&logs).unwrap();

    let transcript = "\
{\"step_index\":0,\"type\":\"USER_INPUT\",\"created_at\":\"2026-09-29T20:08:27Z\",\"content\":\"<USER_REQUEST>hi</USER_REQUEST>\"}\n\
{\"step_index\":1,\"type\":\"CONVERSATION_HISTORY\",\"created_at\":\"2026-09-29T20:08:28Z\",\"content\":\"# Conversation History\\n<conversation_summaries>\\n## Conversation abc: title\\n</conversation_summaries>\"}\n\
{\"step_index\":2,\"type\":\"PLANNER_RESPONSE\",\"created_at\":\"2026-09-29T20:08:30Z\",\"content\":\"hello\"}\n\
{\"step_index\":3,\"type\":\"MCP_TOOL\",\"created_at\":\"2026-09-29T20:08:31Z\",\"content\":\"Created At: ...\\nargs\"}\n\
{\"step_index\":4,\"type\":\"VIEW_FILE\",\"created_at\":\"2026-09-29T20:08:32Z\",\"content\":\"x = 1\"}\n\
{\"step_index\":5,\"type\":\"GREP_SEARCH\",\"created_at\":\"2026-09-29T20:08:33Z\",\"content\":\"no results\"}\n\
{\"step_index\":6,\"type\":\"ERROR_MESSAGE\",\"created_at\":\"2026-09-29T20:08:34Z\",\"content\":\"boom\"}\n\
{\"step_index\":7,\"type\":\"CHECKPOINT\",\"created_at\":\"2026-09-29T20:08:35Z\",\"content\":\"# Resuming...\"}\n\
{\"step_index\":8,\"type\":\"KNOWLEDGE_ARTIFACTS\",\"created_at\":\"2026-09-29T20:08:36Z\",\"content\":\"hidden\"}\n\
";
    fs::write(logs.join("transcript.jsonl"), transcript).unwrap();

    let msgs = load_antigravity_transcript(&folder);
    // Only KNOWLEDGE_ARTIFACTS (8) is hidden. CONVERSATION_HISTORY (1)
    // is surfaced as the "context" role because it carries titles + objectives.
    assert_eq!(msgs.len(), 8, "expected 8 visible rows, got {msgs:#?}");

    let roles: Vec<&str> = msgs.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(
        roles,
        vec![
            "user",
            "context", // ← was hidden before; now surfaced
            "assistant",
            "tool",
            "file",
            "search",
            "error",
            "checkpoint",
        ],
    );

    // Step indexes preserved in order.
    let steps: Vec<u64> = msgs.iter().map(|m| m.step_index).collect();
    assert_eq!(steps, vec![0, 1, 2, 3, 4, 5, 6, 7]);

    // USER_INPUT content is preserved verbatim (no embellishment).
    assert!(msgs[0].content.contains("<USER_REQUEST>"));
    assert_eq!(msgs[0].created_at.as_deref(), Some("2026-09-29T20:08:27Z"));

    // CONVERSATION_HISTORY content is preserved (titles + summaries intact).
    let context = &msgs[1];
    assert_eq!(context.role, "context");
    assert!(context.content.contains("conversation_summaries"));
    assert!(context.content.contains("## Conversation abc: title"));
}

#[test]
fn load_transcript_maps_the_types_real_transcripts_carry() {
    // Types measured from live Antigravity transcripts that the explicit
    // match arm does not name. They must surface as `system` rather than
    // being dropped.
    let tmp = tmpdir("cctrace-antig-load-unmapped");
    let uuid = "99999999-9999-9999-9999-999999999999";
    make_transcript(
        &tmp,
        uuid,
        concat!(
            r#"{"step_index":0,"type":"LIST_DIRECTORY","content":"a"}"#,
            "\n",
            r#"{"step_index":1,"type":"CODE_ACTION","content":"b"}"#,
            "\n",
            r#"{"step_index":2,"type":"RUN_COMMAND","content":"c"}"#,
            "\n",
            r#"{"step_index":3,"type":"INVOKE_SUBAGENT","content":"d"}"#,
            "\n",
            r#"{"step_index":4,"type":"SYSTEM_MESSAGE","content":"e"}"#,
            "\n",
            r#"{"step_index":5,"type":"GENERIC","content":"f"}"#,
            "\n",
        ),
    );
    let msgs = load_antigravity_transcript(&tmp.join(uuid));
    assert_eq!(msgs.len(), 6, "got {msgs:#?}");
    assert!(msgs.iter().all(|m| m.role == "system"));
}

#[test]
fn omitting_the_limit_returns_the_whole_conversation() {
    // The path the reader actually uses: one call, no paging.
    let tmp = tmpdir("cctrace-antig-whole");
    let uuid = "ffff6666-aaaa-7777-bbbb-888888888888";
    let mut lines: Vec<String> = Vec::new();
    for i in 0..240 {
        if i % 5 == 0 {
            lines.push(format!(
                r#"{{"step_index":{i},"type":"EPHEMERAL_MESSAGE","created_at":"2026-09-29T20:10:{:02}Z"}}"#,
                i % 60
            ));
        } else {
            lines.push(format!(
                r#"{{"step_index":{i},"type":"PLANNER_RESPONSE","created_at":"2026-09-29T20:10:{:02}Z","content":"body {i}"}}"#,
                i % 60
            ));
        }
    }
    make_transcript(&tmp, uuid, &lines.join("\n"));

    let index = index_antigravity_transcript(&tmp.join(uuid));
    let window = load_antigravity_window(&tmp.join(uuid), 0, usize::MAX);
    assert_eq!(window.messages.len(), index.count);
    assert_eq!(window.roles.len(), index.count);
    assert_eq!(window.start, 0);
    // First and last, so a truncated read cannot pass.
    assert_eq!(window.messages[0].step_index, 1);
    assert_eq!(window.messages[window.messages.len() - 1].step_index, 239);
}

#[test]
fn paging_returns_exactly_what_a_whole_file_read_returns() {
    // The bug this guards: the index leaves payload-less rows out, and the
    // window reader did not — so a page filled with rows the index never
    // pointed at, and 60 of 73 real conversations came back shifted. Its
    // durations also timed to the raw next row instead of the next shown
    // one, so the same step showed two different numbers.
    let tmp = tmpdir("cctrace-antig-agrees");
    let uuid = "eeee5555-ffff-6666-aaaa-777777777777";
    let mut lines: Vec<String> = Vec::new();
    for i in 0..300 {
        if i % 4 == 0 {
            // Records nothing: still a step, still takes a line.
            lines.push(format!(
                r#"{{"step_index":{i},"type":"EPHEMERAL_MESSAGE","created_at":"2026-09-29T20:09:{:02}Z"}}"#,
                i % 60
            ));
        } else if i % 17 == 0 {
            // Dropped outright by the parser, and still takes a line.
            lines.push(format!(
                r#"{{"step_index":{i},"type":"KNOWLEDGE_ARTIFACTS"}}"#
            ));
        } else {
            lines.push(format!(
                r#"{{"step_index":{i},"type":"PLANNER_RESPONSE","created_at":"2026-09-29T20:09:{:02}Z","content":"body {i}"}}"#,
                i % 60
            ));
        }
    }
    make_transcript(&tmp, uuid, &lines.join("\n"));

    let index = index_antigravity_transcript(&tmp.join(uuid));
    let expect: Vec<AntigravityMessage> = load_antigravity_transcript(&tmp.join(uuid))
        .into_iter()
        .filter(message_has_payload)
        .collect();
    assert_eq!(
        index.count,
        expect.len(),
        "index must match the visible rows (index={} total={} expect={})",
        index.count,
        index.total_rows,
        expect.len()
    );

    let mut got: Vec<AntigravityMessage> = Vec::new();
    let mut start = 0;
    while start < index.count {
        let (page, _) = load_antigravity_transcript_window(&tmp.join(uuid), start, 100);
        if page.is_empty() {
            break;
        }
        start += page.len();
        got.extend(page);
    }
    assert_eq!(
        got.len(),
        expect.len(),
        "pages must tile the conversation: index={} total_rows={} expect={} got={} full_payload={}",
        index.count,
        index.total_rows,
        expect.len(),
        got.len(),
        load_antigravity_transcript(&tmp.join(uuid))
            .iter()
            .filter(|m| message_has_payload(m))
            .count()
    );
    for (i, (a, b)) in got.iter().zip(expect.iter()).enumerate() {
        assert_eq!(a.step_index, b.step_index, "row {i} identity");
        assert_eq!(a.role, b.role, "row {i} role");
        assert_eq!(a.content, b.content, "row {i} content");
        assert_eq!(a.duration_ms, b.duration_ms, "row {i} duration");
    }
}

#[test]
fn the_index_is_stored_beside_the_app_and_invalidated_on_change() {
    // Opening a conversation is only fast because the second open reads a
    // stored index instead of re-parsing the transcript.
    let tmp = tmpdir("cctrace-antig-sidecar");
    let uuid = "dddd4444-eeee-5555-ffff-666666666666";
    let transcript = tmp.join(uuid).join(TRANSCRIPT_SUBPATH);
    make_transcript(
        &tmp,
        uuid,
        concat!(
            r#"{"step_index":0,"type":"USER_INPUT","content":"hi"}"#,
            "\n",
            r#"{"step_index":1,"type":"PLANNER_RESPONSE","content":"hello"}"#,
            "\n",
        ),
    );
    let first = index_antigravity_transcript(&tmp.join(uuid));
    assert_eq!(first.count, 2);

    // A sidecar now exists, and it lives in the app's config dir rather
    // than beside the transcript — the brain root belongs to the IDE.
    let sidecar = sidecar_path(&transcript).expect("sidecar path");
    assert!(sidecar.exists(), "expected a stored index at {sidecar:?}");
    assert!(sidecar
        .to_string_lossy()
        .contains("cctrace-antigravity-index"));

    // Growing the transcript must not serve the stale index.
    let mut text = std::fs::read_to_string(&transcript).unwrap();
    text.push_str(r#"{"step_index":2,"type":"PLANNER_RESPONSE","content":"more"}"#);
    text.push('\n');
    std::fs::write(&transcript, &text).unwrap();
    let second = index_antigravity_transcript(&tmp.join(uuid));
    assert_eq!(second.count, 3, "a changed transcript must be re-indexed");
}

#[test]
fn index_and_windows_agree_on_what_exists() {
    // The index and the window loader must see the same rows, or the
    // frontend is handed indices it can never fill.
    let tmp = tmpdir("cctrace-antig-index");
    let uuid = "bbbb2222-cccc-3333-dddd-444444444444";
    let mut lines: Vec<String> = Vec::new();
    for i in 0..250 {
        // Every third step recorded nothing — 84 of 250.
        if i % 3 == 0 {
            lines.push(format!(
                r#"{{"step_index":{i},"type":"EPHEMERAL_MESSAGE","created_at":"2026-09-29T20:09:00Z"}}"#
            ));
        } else {
            lines.push(format!(
                r#"{{"step_index":{i},"type":"PLANNER_RESPONSE","created_at":"2026-09-29T20:09:00Z","content":"body {i}"}}"#
            ));
        }
    }
    // Dropped outright: pure engine state.
    lines.push(r#"{"step_index":999,"type":"KNOWLEDGE_ARTIFACTS"}"#.to_owned());
    make_transcript(&tmp, uuid, &lines.join("\n"));

    let index = index_antigravity_transcript(&tmp.join(uuid));
    assert_eq!(index.total_rows, 250, "empty steps still count as steps");
    assert_eq!(index.count, 250 - 84, "payload-less steps are left out");
    assert!(index.rows.iter().all(|r| r.role == "assistant"));

    // Pages tile the visible conversation exactly once — no gap, no overlap.
    let first = load_antigravity_window(&tmp.join(uuid), 0, 100);
    assert_eq!(first.messages.len(), 100);
    assert_eq!(first.roles.len(), index.count);
    assert_eq!(first.total_rows, 250);
    assert_eq!(first.messages[0].content, "body 1");

    let second = load_antigravity_window(&tmp.join(uuid), 100, 100);
    assert_eq!(second.start, 100);
    assert_eq!(second.messages.len(), index.count - 100);
    assert_eq!(first.messages.len() + second.messages.len(), index.count);

    // Past the end: empty, and still reports the totals.
    let past = load_antigravity_window(&tmp.join(uuid), 5_000, 100);
    assert!(past.messages.is_empty());
    assert_eq!(past.count, index.count);
}

#[test]
fn the_index_alone_builds_no_message_bodies() {
    let tmp = tmpdir("cctrace-antig-index-only");
    let uuid = "cccc3333-dddd-4444-eeee-555555555555";
    make_transcript(
        &tmp,
        uuid,
        concat!(
            r#"{"step_index":0,"type":"PLANNER_RESPONSE","content":"a","tool_calls":[{"name":"t","args":{}}]}"#,
            "\n",
            r#"{"step_index":1,"type":"USER_INPUT","content":"b"}"#,
            "\n",
        ),
    );
    // `limit: 0` is the opening request — it must cost roles, not bodies.
    let window = load_antigravity_window(&tmp.join(uuid), 0, 0);
    assert!(window.messages.is_empty());
    assert_eq!(window.count, 2);
    assert_eq!(window.roles, vec!["assistant", "user"]);
}

#[test]
fn load_transcript_reads_tool_calls_thinking_and_durations() {
    // The three fields the shared stats bar needs. `tool_calls` and
    // `thinking` are measured off a real transcript — the earlier comment
    // claiming MCP_TOOL never occurs, and that tool names were unavailable,
    // was wrong.
    let tmp = tmpdir("cctrace-antig-load-stats");
    let uuid = "aaaa1111-bbbb-2222-cccc-333333333333";
    make_transcript(
        &tmp,
        uuid,
        concat!(
            r#"{"step_index":0,"type":"PLANNER_RESPONSE","created_at":"2026-09-29T20:08:27Z","content":"reading","thinking":"weighing options","tool_calls":[{"name":"view_file","args":{"toolSummary":"\"Viewing file\"","AbsolutePath":"\"x\""}}]}"#,
            "\n",
            r#"{"step_index":1,"type":"MCP_TOOL","created_at":"2026-09-29T20:08:30Z","content":"done","tool_calls":[{"name":"run_command","args":{"toolSummary":"npm test"}}]}"#,
            "\n",
            r#"{"step_index":2,"type":"USER_INPUT","created_at":"2026-09-29T20:08:31Z","content":"hi"}"#,
            "\n",
            r##"{"step_index":3,"type":"CHECKPOINT","created_at":"2026-09-29T20:08:36Z","content":"# Resuming"}"##,
            "\n",
            // No `content` at all — the shape 10,328 of 25,064 real rows
            // have. The reader must still have something to show.
            r#"{"step_index":4,"type":"PLANNER_RESPONSE","created_at":"2026-09-29T20:08:40Z","thinking":"only thought","tool_calls":[{"name":"run_command","args":{"toolSummary":"cargo test"}}]}"#,
            "\n",
        ),
    );
    let msgs = load_antigravity_transcript(&tmp.join(uuid));
    assert_eq!(msgs.len(), 5, "got {msgs:#?}");

    // Tool calls, with the extra quoting Antigravity wraps args in removed.
    assert_eq!(msgs[0].tool_calls.len(), 1);
    assert_eq!(msgs[0].tool_calls[0].name, "view_file");
    assert_eq!(msgs[0].tool_calls[0].summary, "Viewing file");
    assert_eq!(msgs[1].tool_calls.len(), 1);
    assert_eq!(msgs[1].tool_calls[0].name, "run_command");
    assert_eq!(msgs[1].tool_calls[0].summary, "npm test");
    // The args are kept verbatim as the call's input — for a content-less
    // row this is the only place the step's payload exists.
    assert!(msgs[0].tool_calls[0].input.contains("Viewing file"));
    assert_eq!(
        msgs[1].tool_calls[0].input,
        "{\n  \"toolSummary\": \"npm test\"\n}"
    );
    // A row with no `tool_calls` key is empty, not absent-but-fatal.
    assert!(msgs[2].tool_calls.is_empty());

    // Thinking and errors: present on row 0 only.
    assert_eq!(msgs[0].thinking_text.as_deref(), Some("weighing options"));
    assert_eq!(msgs[1].thinking_text, None);
    assert_eq!(msgs[2].thinking_text, None);
    assert!(msgs.iter().all(|m| m.error_text.is_none()));

    // Duration = gap to the next row; the last row has none and stays 0.
    assert_eq!(msgs[0].duration_ms, 3_000);
    assert_eq!(msgs[1].duration_ms, 1_000);
    assert_eq!(msgs[2].duration_ms, 5_000);
    assert_eq!(msgs[3].duration_ms, 4_000);
    assert_eq!(msgs[4].duration_ms, 0);

    // A content-less row still carries what it actually did.
    assert!(msgs[4].content.is_empty());
    assert_eq!(msgs[4].thinking_text.as_deref(), Some("only thought"));
    assert_eq!(msgs[4].tool_calls[0].summary, "cargo test");
}

// ---- scanning --------------------------------------------------------

#[test]
fn scan_labels_each_folder_with_its_flavor() {
    let tmp = tmpdir("cctrace-antig-flavor");
    make_folder(&tmp, "00000000-0000-0000-0000-0000000000aa", "x");
    let v = scan_antigravity_brain(&tmp, "antigravity");
    assert_eq!(v[0].flavor, "antigravity");
}

#[test]
fn scan_skips_non_conversation_directories() {
    // Real brain roots carry a `tempmediaStorage` screenshot cache beside
    // the conversation folders. Listing it would show a phantom
    // conversation with a garbled title.
    let tmp = tmpdir("cctrace-antig-nonuuid");
    make_folder(
        &tmp,
        "00000000-0000-0000-0000-0000000000bb",
        "Real conversation",
    );
    fs::create_dir_all(tmp.join("tempmediaStorage")).unwrap();
    fs::create_dir_all(tmp.join("scrap")).unwrap();

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(v.len(), 1, "got {v:#?}");
    assert_eq!(v[0].uuid, "00000000-0000-0000-0000-0000000000bb");
}

#[test]
fn scan_marks_which_folders_actually_hold_a_transcript() {
    let tmp = tmpdir("cctrace-antig-has-transcript");
    make_transcript(
        &tmp,
        "00000000-0000-0000-0000-0000000000cc",
        "{\"step_index\":0,\"type\":\"USER_INPUT\",\"content\":\"hi\"}\n",
    );
    // Artifact-only folder: screenshots and reports, no conversation.
    let artifacts = make_folder(&tmp, "00000000-0000-0000-0000-0000000000dd", "artifacts");
    fs::write(artifacts.join("shot.webp"), b"x").unwrap();

    let mut v = scan_antigravity_brain(&tmp, "antigravity-ide");
    v.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    assert!(v[0].has_transcript, "{}", v[0].uuid);
    assert!(!v[1].has_transcript, "{}", v[1].uuid);
}

#[test]
fn malformed_first_user_input_does_not_hide_later_titles() {
    // A transcript whose first USER_INPUT row carries no `content`, or
    // wraps its text in something other than <USER_REQUEST>, must not cost
    // us the title a later row provides.
    let tmp = tmpdir("cctrace-antig-malformed-input");
    make_transcript(
        &tmp,
        "66666666-6666-6666-6666-666666666666",
        concat!(
            r#"{"step_index":0,"type":"USER_INPUT"}"#,
            "\n",
            r#"{"step_index":1,"type":"USER_INPUT","content":"no markers here"}"#,
            "\n",
            r#"{"step_index":2,"type":"USER_INPUT","content":"<USER_REQUEST>the real request</USER_REQUEST>"}"#,
            "\n",
        ),
    );

    let v = scan_antigravity_brain(&tmp, "antigravity-ide");
    assert_eq!(
        v[0].title.as_deref(),
        Some("the real request"),
        "got {:?}",
        v[0].title
    );
}

// ---- merging across flavors -------------------------------------------

#[test]
fn merge_skips_roots_that_do_not_exist() {
    let tmp = tmpdir("cctrace-antig-merge-missing");
    let roots = vec![
        BrainRoot {
            flavor: "antigravity-ide".into(),
            path: tmp.join("nope-ide").to_string_lossy().into_owned(),
            exists: false,
        },
        BrainRoot {
            flavor: "antigravity".into(),
            path: tmp.join("nope-old").to_string_lossy().into_owned(),
            exists: false,
        },
    ];
    assert!(merge_brain_roots(&roots).is_empty());
}

#[test]
fn merge_lists_each_conversation_once_across_flavors() {
    // A migration copies old conversations into the new root, so the same
    // UUID sits under both flavors. Merging naively would double them.
    let tmp = tmpdir("cctrace-antig-merge-dup");
    let gemini = tmp.join(".gemini");
    let ide = gemini.join("antigravity-ide/brain");
    let old = gemini.join("antigravity/brain");
    fs::create_dir_all(&ide).unwrap();
    fs::create_dir_all(&old).unwrap();

    let shared = "aaaaaaaa-0000-0000-0000-000000000001";
    make_folder(&ide, shared, "Migrated conversation");
    make_folder(&old, shared, "Migrated conversation");
    make_folder(
        &old,
        "aaaaaaaa-0000-0000-0000-000000000002",
        "Old-only conversation",
    );
    make_folder(
        &ide,
        "aaaaaaaa-0000-0000-0000-000000000003",
        "New-only conversation",
    );

    let roots = vec![
        BrainRoot {
            flavor: "antigravity-ide".into(),
            path: ide.to_string_lossy().into_owned(),
            exists: true,
        },
        BrainRoot {
            flavor: "antigravity".into(),
            path: old.to_string_lossy().into_owned(),
            exists: true,
        },
    ];
    let v = merge_brain_roots(&roots);
    assert_eq!(v.len(), 3, "got {v:#?}");

    let shared_entry = v
        .iter()
        .find(|f| f.uuid == shared)
        .expect("shared conversation listed exactly once");
    // Newest flavor wins the tie.
    assert_eq!(shared_entry.flavor, "antigravity-ide");
    assert_eq!(shared_entry.title.as_deref(), Some("Migrated conversation"));
}

#[test]
fn merge_prefers_the_copy_that_has_a_transcript() {
    // A migration can carry a folder's artifacts across without its
    // transcript. The copy that can actually be opened is the one to show.
    let tmp = tmpdir("cctrace-antig-merge-transcript");
    let gemini = tmp.join(".gemini");
    let ide = gemini.join("antigravity-ide/brain");
    let old = gemini.join("antigravity/brain");
    let uuid = "bbbbbbbb-0000-0000-0000-000000000001";
    make_transcript(
        &old,
        uuid,
        concat!(
            r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>kept</USER_REQUEST>"}"#,
            "\n",
        ),
    );
    make_folder(&ide, uuid, "Artifacts only, transcript lost in migration");

    let roots = vec![
        BrainRoot {
            flavor: "antigravity-ide".into(),
            path: ide.to_string_lossy().into_owned(),
            exists: true,
        },
        BrainRoot {
            flavor: "antigravity".into(),
            path: old.to_string_lossy().into_owned(),
            exists: true,
        },
    ];
    let v = merge_brain_roots(&roots);
    assert_eq!(v.len(), 1, "got {v:#?}");
    assert!(v[0].has_transcript);
    assert_eq!(v[0].flavor, "antigravity");
    assert!(v[0].path.contains("/antigravity/brain/"), "{}", v[0].path);
    assert_eq!(v[0].title.as_deref(), Some("kept"));
}

#[test]
fn merge_sorts_newest_first() {
    let tmp = tmpdir("cctrace-antig-merge-order");
    let gemini = tmp.join(".gemini");
    let ide = gemini.join("antigravity-ide/brain");
    let old = gemini.join("antigravity/brain");
    fs::create_dir_all(&ide).unwrap();
    fs::create_dir_all(&old).unwrap();
    make_folder(&old, "cccccccc-0000-0000-0000-000000000001", "older");
    std::thread::sleep(std::time::Duration::from_millis(10));
    make_folder(&ide, "cccccccc-0000-0000-0000-000000000002", "newer");

    let roots = vec![
        BrainRoot {
            flavor: "antigravity-ide".into(),
            path: ide.to_string_lossy().into_owned(),
            exists: true,
        },
        BrainRoot {
            flavor: "antigravity".into(),
            path: old.to_string_lossy().into_owned(),
            exists: true,
        },
    ];
    let v = merge_brain_roots(&roots);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].title.as_deref(), Some("newer"));
    assert_eq!(v[1].title.as_deref(), Some("older"));
}
