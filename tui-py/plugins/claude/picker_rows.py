"""Claude's rows in the shared picker: sessions grouped by date."""

from __future__ import annotations

from collections.abc import Callable
from datetime import UTC, datetime, timedelta

from rich.console import Group
from rich.rule import Rule
from rich.text import Text

import theme
from data_types import Liveness, SessionInfo
from efficiency import (
    EfficiencyJob,
    EfficiencySummary,
    is_running,
    score_color,
    status_label,
)
from format_utils import (
    format_cost,
    format_tokens,
    progress_bar,
    short_model,
    short_path,
    time_ago,
)
from plugins.base import PickerRow, PickerSection
from theme import get_model_color

# Session path -> (latest Jev job, latest Jev summary).
AnalysisLookup = Callable[[str], tuple[EfficiencyJob | None, EfficiencySummary | None]]

# Icons
ICON_SELECTED = "▸"
ICON_BRANCH = "*"
ICON_CHAT = "#"
ICON_CLOCK = "~"
ICON_SESSION = "@"
ICON_SCORE = "%"

# Narrower than the scan bar: it shares a row with the score and status text.
ANALYSIS_BAR_WIDTH = 10


def _liveness_badge(liveness: Liveness | None) -> str:
    """Render a liveness badge. Forward-compatible: an unknown `status` value
    (a future Claude-Code release) falls back to a neutral "○ <status>" rather
    than disappearing or raising."""
    if liveness is None:
        return ""
    if liveness.status == "busy":
        return "● busy"
    if liveness.status == "idle":
        minutes = liveness.idle_seconds // 60
        return f"○ idle {minutes}m"
    return f"○ {liveness.status}"


def _analysis_line(job: EfficiencyJob | None, summary: EfficiencySummary | None) -> Text | None:
    """The session's Jev line: its score, a running analysis, or a failure.

    Returns None for a session that has never been analysed, so untouched
    sessions keep their two-line layout.
    """
    if job is None and summary is None:
        return None

    line = Text()
    if summary is not None:
        line.append(f"{ICON_SCORE} Jev {summary.score}", style=f"bold {score_color(summary.score)}")
        if summary.stale:
            line.append(" · stale", style=theme.TEXT_DIM)

    if job is not None and is_running(job):
        percent = job.progress if job.progress is not None else 0
        if line.plain:
            line.append("  ")
        line.append(f"{status_label(job.status)} ", style=theme.ACCENT)
        line.append(progress_bar(percent, ANALYSIS_BAR_WIDTH), style=theme.ACCENT)
        line.append(f" {percent}%", style=f"bold {theme.ACCENT}")
        if job.message:
            line.append(f" · {job.message}", style=theme.TEXT_DIM)
    elif job is not None and job.status == "failed" and summary is None:
        line.append("Analysis failed", style=theme.ERROR)
        if job.error:
            line.append(f" · {job.error}", style=theme.TEXT_DIM)

    return line if line.plain else None


def _group_by_date(sessions: list[SessionInfo]) -> list[tuple[str, list[SessionInfo]]]:
    now = datetime.now(UTC)
    today_start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    yesterday_start = today_start - timedelta(days=1)
    week_start = today_start - timedelta(days=7)
    month_start = today_start - timedelta(days=30)

    groups: dict[str, list[SessionInfo]] = {}
    order = ["Today", "Yesterday", "This Week", "This Month", "Older"]

    for s in sessions:
        try:
            d = datetime.fromisoformat(s.mod_time.replace("Z", "+00:00"))
        except Exception:
            d = now
        if d >= today_start:
            cat = "Today"
        elif d >= yesterday_start:
            cat = "Yesterday"
        elif d >= week_start:
            cat = "This Week"
        elif d >= month_start:
            cat = "This Month"
        else:
            cat = "Older"
        groups.setdefault(cat, []).append(s)

    for cat in order:
        if cat in groups:
            groups[cat].sort(key=lambda s: s.mod_time, reverse=True)

    return [(cat, groups[cat]) for cat in order if cat in groups and groups[cat]]


def _render_session(
    s: SessionInfo,
    anim_frame: int = 0,
    job: EfficiencyJob | None = None,
    summary: EfficiencySummary | None = None,
) -> object:
    """Render a session as a Rich Group with full-width separator."""
    model_str = short_model(s.model) if s.model else ""
    model_clr = get_model_color(s.model) if s.model else theme.TEXT_DIM

    # Line 1: title. Prefer the user-assigned /rename name (accent-highlighted);
    # otherwise use the first message. When a name is shown, the first message is
    # kept as the subtitle.
    title = s.name or s.first_message or s.session_id
    line1 = Text()
    line1.append(title, style=theme.ACCENT if s.name else theme.TEXT_PRIMARY)
    if s.is_ongoing:
        line1.append(f"  {theme.SPIN[anim_frame]}", style=theme.ONGOING)

    # Line 2: metadata
    line2 = Text()
    if model_str:
        line2.append(model_str, style=model_clr)
    if s.git_branch:
        line2.append(f" {ICON_BRANCH} {s.git_branch}", style=theme.GIT_BRANCH)
    line2.append(f" {ICON_CHAT} {s.turn_count}", style=theme.TEXT_DIM)
    if s.total_tokens > 0:
        tok_clr = theme.TOKEN_HIGH if s.total_tokens > 150000 else theme.TEXT_DIM
        line2.append(f" {format_tokens(s.total_tokens)} tok", style=tok_clr)
    if s.cost_usd > 0:
        line2.append(f" {format_cost(s.cost_usd)}", style=theme.TOKEN_HIGH)
    line2.append(f" {ICON_SESSION} {s.session_id[:8]}", style=theme.TEXT_DIM)
    line2.append(f" {ICON_CLOCK} {time_ago(s.mod_time)}", style=theme.TEXT_DIM)
    badge = _liveness_badge(s.liveness)
    if badge:
        badge_style = (
            theme.ONGOING if s.liveness and s.liveness.status == "busy" else theme.TEXT_DIM
        )
        line2.append(f"  {badge}", style=badge_style)

    # "CWDs" line: only when the session roamed across >1 working directory
    # (e.g. /cd between repos). Lists every distinct dir so none is hidden.
    dirs_line: Text | None = None
    if len(s.dirs) > 1:
        dirs_line = Text()
        dirs_line.append("CWDs: ", style=f"bold {theme.ACCENT}")
        # Accent the roamed directories so they stand out from the dim meta row.
        dirs_line.append(", ".join(short_path(d) for d in s.dirs), style=theme.ACCENT)

    parts: list[Text | str] = [line1]
    if s.name and s.first_message:
        parts += ["\n", Text(s.first_message, style=theme.TEXT_DIM)]
    if dirs_line is not None:
        parts += ["\n", dirs_line]
    parts += ["\n", line2]
    analysis_line = _analysis_line(job, summary)
    if analysis_line is not None:
        parts += ["\n", analysis_line]
    content = Text.assemble(*parts)
    sep = Rule(style=theme.TEXT_MUTED, characters="─")
    return Group(content, sep)


def _notice(markup: str) -> PickerRow:
    return PickerRow(render=lambda _frame: markup)


def _header(markup: str) -> PickerRow:
    return PickerRow(render=lambda _frame: markup, header=True)


def build_section(
    sessions: list[SessionInfo],
    loading: bool,
    error: str,
    analysis: AnalysisLookup,
) -> PickerSection:
    """The picker rows for ``sessions``: a totals header, then one header per
    date group followed by its sessions, newest first.

    Rows look their analysis state up through ``analysis`` each time they are
    drawn, so re-rendering a row shows the latest job and score.
    """
    if loading:
        return PickerSection(loading=True)
    if error:
        return PickerSection(rows=[_notice(f"[{theme.ERROR}]{error}[/]")])
    if not sessions:
        return PickerSection(rows=[_notice(f"[{theme.TEXT_DIM}]No sessions found[/]")])

    total_tokens = sum(s.total_tokens for s in sessions)
    total_cost = sum(s.cost_usd for s in sessions)
    header_parts = [f"[bold]Sessions ({len(sessions)})[/]"]
    if total_tokens > 0:
        header_parts.append(f"  [{theme.TEXT_DIM}]{format_tokens(total_tokens)} tok[/]")
    if total_cost > 0:
        header_parts.append(f"  [{theme.TOKEN_HIGH}]{format_cost(total_cost)}[/]")
    rows = [_header("".join(header_parts))]

    for cat, group_sessions in _group_by_date(sessions):
        rows.append(_header(f"[bold {theme.TEXT_DIM}]{cat}[/]"))
        for session in group_sessions:
            rows.append(
                PickerRow(
                    render=_session_renderer(session, analysis),
                    item=session,
                    animated=session.is_ongoing,
                )
            )
    return PickerSection(rows=rows)


def _session_renderer(session: SessionInfo, analysis: AnalysisLookup):
    def render(anim_frame: int) -> object:
        job, summary = analysis(session.path)
        return _render_session(session, anim_frame, job=job, summary=summary)

    return render
