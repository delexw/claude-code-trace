"""Antigravity steps as the shared DisplayMessage, so the shared list and
detail views draw them unchanged."""

from __future__ import annotations

from data_types import DisplayItem, DisplayMessage
from plugins.antigravity.models import StepMessage, ToolCall

ROLE_LABELS = {
    "user": "User",
    "assistant": "Assistant",
    "tool": "MCP Tool",
    "file": "View File",
    "search": "Grep Search",
    "error": "Error",
    "checkpoint": "Checkpoint",
    "context": "Context",
    "system": "System",
}


def _display_role(role: str) -> str:
    if role == "user":
        return "user"
    if role == "assistant":
        return "claude"
    if role == "checkpoint":
        return "compact"
    return "system"


def body(step: StepMessage) -> str:
    """The step's text, or — when it wrote none — what it thought, called and
    hit, so a step that only thought or only called a tool is not blank."""
    if step.content.strip():
        return step.content
    parts: list[str] = []
    if step.thinking_text:
        parts.append(step.thinking_text)
    for call in step.tool_calls:
        parts.append(f"{call.name} — {call.summary}" if call.summary else call.name)
    if step.error_text:
        parts.append(step.error_text)
    return "\n\n".join(parts)


def _tool_item(step: StepMessage, call: ToolCall, index: int) -> DisplayItem:
    return DisplayItem(
        id=f"ag-{step.step_index}-{index}",
        item_type="ToolCall",
        text=call.summary,
        tool_name=call.name,
        tool_summary=call.summary,
        tool_input=call.input,
        tool_result=step.content,
    )


def to_display_message(step: StepMessage) -> DisplayMessage:
    items: list[DisplayItem] = []
    if step.thinking_text:
        items.append(
            DisplayItem(
                id=f"ag-{step.step_index}-thinking",
                item_type="Thinking",
                text=step.thinking_text,
            )
        )
    items += [_tool_item(step, call, i) for i, call in enumerate(step.tool_calls)]
    return DisplayMessage(
        role=_display_role(step.role),
        content=body(step),
        timestamp=step.created_at or "",
        thinking_count=1 if step.thinking_text else 0,
        tool_call_count=len(step.tool_calls),
        output_count=1 if step.role == "assistant" else 0,
        duration_ms=step.duration_ms,
        items=items,
        is_error=step.role == "error",
        role_label=ROLE_LABELS.get(step.role, "System"),
    )
