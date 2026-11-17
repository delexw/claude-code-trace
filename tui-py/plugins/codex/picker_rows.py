"""Codex conversations in the shared terminal picker."""

from datetime import UTC, datetime, timedelta

from rich.console import Group
from rich.rule import Rule
from rich.text import Text

import theme
from format_utils import format_cost, format_duration, format_tokens, time_ago
from plugins.base import PickerRow, PickerSection
from plugins.codex.models import CodexSession
from widgets.efficiency_status import AnalysisLookup, analysis_line


def _render(session: CodexSession, frame: int = 0, analysis: AnalysisLookup | None = None) -> Group:
    title = Text(session.title or session.id, style=theme.TEXT_PRIMARY)
    if session.is_ongoing:
        title.append(f"  {theme.SPIN[frame % len(theme.SPIN)]}", style=theme.ONGOING)
    metadata = Text(session.source, style=theme.ACCENT)
    if session.archived:
        metadata.append("  Archived", style=theme.TEXT_DIM)
    if session.cwd:
        metadata.append(f"  {session.cwd}", style=theme.TEXT_DIM)
    for value in (
        session.model,
        session.git_branch,
        session.permission_mode,
        session.sandbox_policy,
    ):
        if value:
            metadata.append(f"  {value}", style=theme.TEXT_DIM)
    if session.turn_count > 0:
        metadata.append(f"  {session.turn_count} turns", style=theme.TEXT_DIM)
    if session.duration_ms is not None:
        metadata.append(f"  {format_duration(session.duration_ms)}", style=theme.TEXT_DIM)
    if session.total_tokens > 0:
        metadata.append(f"  {format_tokens(session.total_tokens)} tok", style=theme.TEXT_DIM)
    metadata.append(
        f"  ~{format_cost(session.cost_usd)}{'+' if session.cost_partial else ''}"
        if session.cost_usd is not None
        else "  Cost unavailable",
        style=theme.TOKEN_HIGH if session.cost_usd is not None else theme.TEXT_DIM,
    )
    if ago := time_ago(session.modified_at):
        metadata.append(f"  {ago}", style=theme.TEXT_DIM)
    content = Text.assemble(title, "\n", metadata)
    if analysis is not None:
        job, summary = analysis(session.path)
        if status := analysis_line(job, summary):
            content.append("\n")
            content.append(status)
    return Group(content, Rule(style=theme.TEXT_MUTED))


def _group_by_date(sessions: list[CodexSession]) -> list[tuple[str, list[CodexSession]]]:
    today = datetime.now(UTC).replace(hour=0, minute=0, second=0, microsecond=0)
    groups: dict[str, list[CodexSession]] = {}
    for session in sorted(sessions, key=lambda session: session.modified_at, reverse=True):
        try:
            modified = datetime.fromisoformat(session.modified_at.replace("Z", "+00:00"))
            if modified.tzinfo is None:
                modified = modified.replace(tzinfo=UTC)
        except ValueError:
            modified = datetime.min.replace(tzinfo=UTC)
        category = "Older"
        for label, threshold in (
            ("Today", today),
            ("Yesterday", today - timedelta(days=1)),
            ("This Week", today - timedelta(days=7)),
            ("This Month", today - timedelta(days=30)),
        ):
            if modified >= threshold:
                category = label
                break
        groups.setdefault(category, []).append(session)
    return [
        (category, groups[category])
        for category in ("Today", "Yesterday", "This Week", "This Month", "Older")
        if category in groups
    ]


def build_section(
    sessions: list[CodexSession], loading: bool, error: str, analysis: AnalysisLookup | None = None
) -> PickerSection:
    if loading:
        return PickerSection(loading=True)
    if error:
        return PickerSection(rows=[PickerRow(render=lambda _f: Text(error, style=theme.ERROR))])
    if not sessions:
        return PickerSection(
            rows=[
                PickerRow(
                    render=lambda _f: Text("No Codex conversations found", style=theme.TEXT_DIM)
                )
            ]
        )
    known_costs = [session.cost_usd for session in sessions if session.cost_usd is not None]
    heading = f"Codex ({len(sessions)})"
    if known_costs:
        partial = (
            "+"
            if len(known_costs) < len(sessions) or any(session.cost_partial for session in sessions)
            else ""
        )
        heading += f"  ~{format_cost(sum(known_costs))}{partial} estimated API cost"
    rows = [PickerRow(render=lambda _f: Text(heading, style="bold"), header=True)]
    for category, grouped in _group_by_date(sessions):
        rows.append(
            PickerRow(
                render=lambda _f, category=category: Text(category, style=theme.TEXT_MUTED),
                header=True,
            )
        )
        rows.extend(
            PickerRow(
                render=lambda frame, session=session: _render(session, frame, analysis),
                item=session,
                animated=session.is_ongoing,
            )
            for session in grouped
        )
    return PickerSection(rows=rows)
