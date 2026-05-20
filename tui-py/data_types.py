"""Python dataclasses matching the TypeScript types from shared/types.ts."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

# ---------------------------------------------------------------------------
# ToolCallSummary
# ---------------------------------------------------------------------------


@dataclass
class ToolCallSummary:
    name: str = ""
    summary: str = ""


# ---------------------------------------------------------------------------
# LastOutput
# ---------------------------------------------------------------------------


@dataclass
class LastOutput:
    output_type: Literal["Text", "ToolResult", "ToolCalls"] = "Text"
    text: str = ""
    tool_name: str = ""
    tool_result: str = ""
    is_error: bool = False
    tool_calls: list[ToolCallSummary] = field(default_factory=list)


# ---------------------------------------------------------------------------
# DisplayItem
# ---------------------------------------------------------------------------

DisplayItemType = Literal[
    "Thinking",
    "Output",
    "ToolCall",
    "Subagent",
    "TeammateMessage",
    "HookEvent",
]


@dataclass
class DisplayItem:
    id: str = ""
    item_type: str = "Output"
    text: str = ""
    tool_name: str = ""
    tool_summary: str = ""
    tool_category: str = ""
    tool_input: str = ""
    tool_result: str = ""
    tool_error: bool = False
    duration_ms: int = 0
    token_count: int = 0
    subagent_type: str = ""
    subagent_desc: str = ""
    subagent_prompt: str = ""
    team_member_name: str = ""
    teammate_id: str = ""
    team_color: str = ""
    subagent_ongoing: bool = False
    agent_id: str = ""
    subagent_messages: list[DisplayMessage] = field(default_factory=list)
    hook_event: str = ""
    hook_name: str = ""
    hook_command: str = ""
    hook_metadata: str = ""
    tool_result_json: str = ""
    is_orphan: bool = False


# ---------------------------------------------------------------------------
# DisplayMessage
# ---------------------------------------------------------------------------


@dataclass
class DisplayMessage:
    role: str = "user"
    model: str = ""
    content: str = ""
    timestamp: str = ""
    thinking_count: int = 0
    tool_call_count: int = 0
    output_count: int = 0
    tokens_raw: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_tokens: int = 0
    cache_creation_tokens: int = 0
    context_tokens: int = 0
    duration_ms: int = 0
    items: list[DisplayItem] = field(default_factory=list)
    last_output: LastOutput | None = None
    is_error: bool = False
    teammate_spawns: int = 0
    teammate_messages: int = 0
    subagent_label: str = ""


# ---------------------------------------------------------------------------
# SessionInfo
# ---------------------------------------------------------------------------


@dataclass
class SessionInfo:
    path: str = ""
    session_id: str = ""
    mod_time: str = ""
    first_message: str = ""
    turn_count: int = 0
    is_ongoing: bool = False
    total_tokens: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_tokens: int = 0
    cache_creation_tokens: int = 0
    cost_usd: float = 0.0
    duration_ms: int = 0
    model: str = ""
    cwd: str = ""
    git_branch: str = ""
    permission_mode: str = ""


# ---------------------------------------------------------------------------
# SessionMeta
# ---------------------------------------------------------------------------


@dataclass
class SessionMeta:
    cwd: str = ""
    git_branch: str = ""
    permission_mode: str = ""


# ---------------------------------------------------------------------------
# SessionTotals
# ---------------------------------------------------------------------------


@dataclass
class SessionTotals:
    total_tokens: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_tokens: int = 0
    cache_creation_tokens: int = 0
    cost_usd: float = 0.0
    model: str = ""


# ---------------------------------------------------------------------------
# TeamTask / TeamSnapshot
# ---------------------------------------------------------------------------


@dataclass
class TeamTask:
    id: str = ""
    subject: str = ""
    status: str = ""
    owner: str = ""


@dataclass
class TeamSnapshot:
    name: str = ""
    description: str = ""
    tasks: list[TeamTask] = field(default_factory=list)
    members: list[str] = field(default_factory=list)
    member_colors: dict[str, str] = field(default_factory=dict)
    member_ongoing: dict[str, bool] = field(default_factory=dict)
    deleted: bool = False


# ---------------------------------------------------------------------------
# LoadResult
# ---------------------------------------------------------------------------


@dataclass
class LoadResult:
    messages: list[DisplayMessage] = field(default_factory=list)
    teams: list[TeamSnapshot] = field(default_factory=list)
    ongoing: bool = False
    meta: SessionMeta = field(default_factory=SessionMeta)
    session_totals: SessionTotals = field(default_factory=SessionTotals)


# ---------------------------------------------------------------------------
# DebugEntry
# ---------------------------------------------------------------------------


@dataclass
class DebugEntry:
    timestamp: str = ""
    level: str = ""
    category: str = ""
    message: str = ""
    extra: str = ""
    line_num: int = 0
    count: int = 1


# ---------------------------------------------------------------------------
# FlatItem (project tree)
# ---------------------------------------------------------------------------


@dataclass
class FlatItem:
    key: str | None = None
    name: str = ""
    count: int = 0
    ongoing: bool = False
    depth: int = 0
    is_group: bool = False
    has_children: bool = False
    is_expanded: bool = True


# ---------------------------------------------------------------------------
# JSON deserialisation helpers
# ---------------------------------------------------------------------------


def _item_from_dict(d: dict) -> DisplayItem:
    msgs_raw = d.get("subagent_messages") or []
    return DisplayItem(
        id=d.get("id", ""),
        item_type=d.get("item_type", "Output"),
        text=d.get("text", ""),
        tool_name=d.get("tool_name", ""),
        tool_summary=d.get("tool_summary", ""),
        tool_category=d.get("tool_category", ""),
        tool_input=d.get("tool_input", ""),
        tool_result=d.get("tool_result", ""),
        tool_error=bool(d.get("tool_error", False)),
        duration_ms=int(d.get("duration_ms", 0)),
        token_count=int(d.get("token_count", 0)),
        subagent_type=d.get("subagent_type", ""),
        subagent_desc=d.get("subagent_desc", ""),
        subagent_prompt=d.get("subagent_prompt", ""),
        team_member_name=d.get("team_member_name", ""),
        teammate_id=d.get("teammate_id", ""),
        team_color=d.get("team_color", ""),
        subagent_ongoing=bool(d.get("subagent_ongoing", False)),
        agent_id=d.get("agent_id", ""),
        subagent_messages=[_msg_from_dict(m) for m in msgs_raw],
        hook_event=d.get("hook_event", ""),
        hook_name=d.get("hook_name", ""),
        hook_command=d.get("hook_command", ""),
        hook_metadata=d.get("hook_metadata", ""),
        tool_result_json=d.get("tool_result_json", ""),
        is_orphan=bool(d.get("is_orphan", False)),
    )


def _msg_from_dict(d: dict) -> DisplayMessage:
    lo_raw = d.get("last_output")
    last_output: LastOutput | None = None
    if lo_raw:
        last_output = LastOutput(
            output_type=lo_raw.get("output_type", "Text"),
            text=lo_raw.get("text", ""),
            tool_name=lo_raw.get("tool_name", ""),
            tool_result=lo_raw.get("tool_result", ""),
            is_error=bool(lo_raw.get("is_error", False)),
            tool_calls=[
                ToolCallSummary(name=tc.get("name", ""), summary=tc.get("summary", ""))
                for tc in (lo_raw.get("tool_calls") or [])
            ],
        )
    return DisplayMessage(
        role=d.get("role", "user"),
        model=d.get("model", ""),
        content=d.get("content", ""),
        timestamp=d.get("timestamp", ""),
        thinking_count=int(d.get("thinking_count", 0)),
        tool_call_count=int(d.get("tool_call_count", 0)),
        output_count=int(d.get("output_count", 0)),
        tokens_raw=int(d.get("tokens_raw", 0)),
        input_tokens=int(d.get("input_tokens", 0)),
        output_tokens=int(d.get("output_tokens", 0)),
        cache_read_tokens=int(d.get("cache_read_tokens", 0)),
        cache_creation_tokens=int(d.get("cache_creation_tokens", 0)),
        context_tokens=int(d.get("context_tokens", 0)),
        duration_ms=int(d.get("duration_ms", 0)),
        items=[_item_from_dict(it) for it in (d.get("items") or [])],
        last_output=last_output,
        is_error=bool(d.get("is_error", False)),
        teammate_spawns=int(d.get("teammate_spawns", 0)),
        teammate_messages=int(d.get("teammate_messages", 0)),
        subagent_label=d.get("subagent_label", ""),
    )


def session_info_from_dict(d: dict) -> SessionInfo:
    return SessionInfo(
        path=d.get("path", ""),
        session_id=d.get("session_id", ""),
        mod_time=d.get("mod_time", ""),
        first_message=d.get("first_message", ""),
        turn_count=int(d.get("turn_count", 0)),
        is_ongoing=bool(d.get("is_ongoing", False)),
        total_tokens=int(d.get("total_tokens", 0)),
        input_tokens=int(d.get("input_tokens", 0)),
        output_tokens=int(d.get("output_tokens", 0)),
        cache_read_tokens=int(d.get("cache_read_tokens", 0)),
        cache_creation_tokens=int(d.get("cache_creation_tokens", 0)),
        cost_usd=float(d.get("cost_usd", 0.0)),
        duration_ms=int(d.get("duration_ms", 0)),
        model=d.get("model", ""),
        cwd=d.get("cwd", ""),
        git_branch=d.get("git_branch", ""),
        permission_mode=d.get("permission_mode", ""),
    )


def load_result_from_dict(d: dict) -> LoadResult:
    meta_d = d.get("meta") or {}
    tot_d = d.get("session_totals") or {}
    return LoadResult(
        messages=[_msg_from_dict(m) for m in (d.get("messages") or [])],
        teams=[_team_from_dict(t) for t in (d.get("teams") or [])],
        ongoing=bool(d.get("ongoing", False)),
        meta=SessionMeta(
            cwd=meta_d.get("cwd", ""),
            git_branch=meta_d.get("git_branch", ""),
            permission_mode=meta_d.get("permission_mode", ""),
        ),
        session_totals=SessionTotals(
            total_tokens=int(tot_d.get("total_tokens", 0)),
            input_tokens=int(tot_d.get("input_tokens", 0)),
            output_tokens=int(tot_d.get("output_tokens", 0)),
            cache_read_tokens=int(tot_d.get("cache_read_tokens", 0)),
            cache_creation_tokens=int(tot_d.get("cache_creation_tokens", 0)),
            cost_usd=float(tot_d.get("cost_usd", 0.0)),
            model=tot_d.get("model", ""),
        ),
    )


def _team_from_dict(d: dict) -> TeamSnapshot:
    return TeamSnapshot(
        name=d.get("name", ""),
        description=d.get("description", ""),
        tasks=[
            TeamTask(
                id=t.get("id", ""),
                subject=t.get("subject", ""),
                status=t.get("status", ""),
                owner=t.get("owner", ""),
            )
            for t in (d.get("tasks") or [])
        ],
        members=list(d.get("members") or []),
        member_colors=dict(d.get("member_colors") or {}),
        member_ongoing=dict(d.get("member_ongoing") or {}),
        deleted=bool(d.get("deleted", False)),
    )


def debug_entry_from_dict(d: dict) -> DebugEntry:
    return DebugEntry(
        timestamp=d.get("timestamp", ""),
        level=d.get("level", ""),
        category=d.get("category", ""),
        message=d.get("message", ""),
        extra=d.get("extra", ""),
        line_num=int(d.get("line_num", 0)),
        count=int(d.get("count", 1)),
    )


def session_update_from_dict(
    d: dict,
) -> tuple[list[DisplayMessage], bool, str, list[TeamSnapshot], SessionTotals]:
    """Parse a session-update SSE payload."""
    messages = [_msg_from_dict(m) for m in (d.get("messages") or [])]
    ongoing = bool(d.get("ongoing", False))
    permission_mode = d.get("permission_mode", "")
    teams = [_team_from_dict(t) for t in (d.get("teams") or [])]
    tot_d = d.get("session_totals") or {}
    totals = SessionTotals(
        total_tokens=int(tot_d.get("total_tokens", 0)),
        input_tokens=int(tot_d.get("input_tokens", 0)),
        output_tokens=int(tot_d.get("output_tokens", 0)),
        cache_read_tokens=int(tot_d.get("cache_read_tokens", 0)),
        cache_creation_tokens=int(tot_d.get("cache_creation_tokens", 0)),
        cost_usd=float(tot_d.get("cost_usd", 0.0)),
        model=tot_d.get("model", ""),
    )
    return messages, ongoing, permission_mode, teams, totals
