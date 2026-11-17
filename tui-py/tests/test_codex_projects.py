"""Fabricated folder identities exercise Codex grouping on both host platforms."""

from dataclasses import replace

from plugins.codex.models import CodexSession
from plugins.codex.projects import build_project_items, codex_project_key, filter_project_sessions


def test_real_folders_group_counts_and_keep_equal_basename_projects_separate():
    first = CodexSession(id="first", cwd="/example/one/app")
    same = replace(first, id="same", cwd="/example/one/app/")
    other = CodexSession(id="other", cwd="/example/two/app")
    items = build_project_items([first, same, other])
    assert items[0].count == 3
    assert [item.count for item in items[1:]] == [2, 1]
    assert codex_project_key(first) != codex_project_key(other)
    assert filter_project_sessions([first, same, other], codex_project_key(first)) == [first, same]


def test_windows_normalizes_case_and_separators_but_posix_and_distros_stay_distinct():
    assert codex_project_key(CodexSession(cwd="D:\\Example\\App\\")) == codex_project_key(
        CodexSession(cwd="d:/example/app")
    )
    assert codex_project_key(CodexSession(cwd="/example/App")) != codex_project_key(
        CodexSession(cwd="/example/app")
    )
    ubuntu = CodexSession(cwd="/home/demo/app", source="WSL: ExampleUbuntu")
    debian = replace(ubuntu, source="WSL: ExampleDebian")
    assert codex_project_key(ubuntu) != codex_project_key(debian)
    assert codex_project_key(ubuntu) == codex_project_key(
        CodexSession(cwd="\\\\wsl.localhost\\ExampleUbuntu\\home\\demo\\app")
    )


def test_nested_folders_use_segment_boundaries_and_exact_project_filter():
    parent = CodexSession(id="parent", cwd="/example/app")
    child = CodexSession(id="child", cwd="/example/app/tools")
    unrelated = CodexSession(id="unrelated", cwd="/example/app-other")
    sessions = [parent, child, unrelated]
    by_key = {item.key: item for item in build_project_items(sessions)}
    assert by_key[codex_project_key(parent)].has_children
    assert by_key[codex_project_key(child)].depth == 1
    assert by_key[codex_project_key(unrelated)].depth == 0
    assert filter_project_sessions(sessions, codex_project_key(parent)) == [parent]


def test_forks_group_under_parent_origin_and_missing_and_cyclic_parents_are_safe():
    parent = CodexSession(id="parent", cwd="/example/elsewhere", dirs=["/example/origin"])
    fork = CodexSession(
        id="fork", cwd="/example/fork", forked_from_session_id="parent", is_ongoing=True
    )
    items = build_project_items([parent, fork])
    assert len(items) == 2
    assert items[1].name == "origin"
    assert items[1].count == 2
    assert items[1].ongoing
    assert filter_project_sessions([parent, fork], codex_project_key(parent)) == [parent, fork]
    assert build_project_items([fork])[1].name == "fork"
    assert build_project_items([replace(parent, forked_from_session_id="fork"), fork])


def test_unknown_folders_remain_selectable():
    session = CodexSession(id="unknown")
    items = build_project_items([session])
    assert items[1].name == "Unknown folder"
    assert filter_project_sessions([session], items[1].key) == [session]
