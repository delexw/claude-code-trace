"""Source-independent picker status for cached Jev scores and live analysis jobs."""

from collections.abc import Callable

from rich.text import Text

import theme
from efficiency import EfficiencyJob, EfficiencySummary, is_running, score_color, status_label
from format_utils import progress_bar

AnalysisLookup = Callable[[str], tuple[EfficiencyJob | None, EfficiencySummary | None]]
ICON_SCORE = "%"
ANALYSIS_BAR_WIDTH = 10


def analysis_line(job: EfficiencyJob | None, summary: EfficiencySummary | None) -> Text | None:
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
