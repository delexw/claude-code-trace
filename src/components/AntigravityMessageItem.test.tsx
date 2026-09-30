import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AntigravityMessageItem } from "./AntigravityMessageItem";
import type { AntigravityMessage } from "../types";

const USER_MSG: AntigravityMessage = {
  step_index: 0,
  role: "user",
  content: "<USER_REQUEST>\nهل تشوف الكود؟\n</USER_REQUEST>",
  created_at: "2026-09-29T20:08:27Z",
  tool_name: null,
};

const ASSISTANT_MSG: AntigravityMessage = {
  step_index: 1,
  role: "assistant",
  content: "نعم، أراه.",
  created_at: "2026-09-29T20:08:30Z",
  tool_name: null,
};

const TOOL_MSG: AntigravityMessage = {
  step_index: 2,
  role: "tool",
  content: "Created At: 2026-09-29T20:08:31Z\nResult: ok",
  created_at: "2026-09-29T20:08:31Z",
  tool_name: null,
};

const FILE_MSG: AntigravityMessage = {
  step_index: 3,
  role: "file",
  content: "x = 1",
  created_at: "2026-09-29T20:08:32Z",
  tool_name: null,
};

const ERROR_MSG: AntigravityMessage = {
  step_index: 4,
  role: "error",
  content: "Error: API overloaded",
  created_at: "2026-09-29T20:08:34Z",
  tool_name: null,
};

const renderOne = (msg: AntigravityMessage) =>
  render(
    <AntigravityMessageItem
      message={msg}
      index={0}
      isSelected={false}
      isExpanded={false}
      onClick={vi.fn()}
    />,
  );

describe("AntigravityMessageItem", () => {
  it("renders user role with the raw <USER_REQUEST> block preserved", () => {
    renderOne(USER_MSG);
    // The exact wire-format string survives — no embellishment, no XML
    // escaping. (after whitespace collapse across the <pre>.)
    expect(screen.getByText(/<USER_REQUEST>/)).toBeInTheDocument();
    expect(screen.getByText(/هل تشوف/)).toBeInTheDocument();
    expect(screen.getByText(/<\/USER_REQUEST>/)).toBeInTheDocument();
    expect(screen.getByText("#0")).toBeInTheDocument();
  });

  it("renders assistant content with role label", () => {
    renderOne(ASSISTANT_MSG);
    expect(screen.getByText("نعم، أراه.")).toBeInTheDocument();
    expect(screen.getByText(/Assistant/i)).toBeInTheDocument();
    expect(screen.getByText("#1")).toBeInTheDocument();
  });

  it("renders tool content under .message--tool", () => {
    renderOne(TOOL_MSG);
    expect(screen.getByText(/MCP Tool/i)).toBeInTheDocument();
    expect(document.querySelector(".message--tool")).not.toBeNull();
  });

  it("renders file content under .message--file", () => {
    renderOne(FILE_MSG);
    expect(screen.getByText(/View File/i)).toBeInTheDocument();
    expect(document.querySelector(".message--file")).not.toBeNull();
  });

  it("renders error content under .message--system-error", () => {
    renderOne(ERROR_MSG);
    // "Error" appears twice (role label + content prefix); scope the match
    // to the role label to avoid the ambiguity.
    expect(screen.getAllByText(/Error/i).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Error: API overloaded")).toBeInTheDocument();
    expect(document.querySelector(".message--system-error")).not.toBeNull();
  });

  it("collapses long content to first 12 lines when isExpanded=false", () => {
    const manyLines = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join("\n");
    const longMsg: AntigravityMessage = {
      step_index: 99,
      role: "assistant",
      content: manyLines,
      created_at: null,
      tool_name: null,
    };
    renderOne(longMsg);
    expect(document.querySelector(".message__content--collapsed")).not.toBeNull();
  });

  it("expands the full content when isExpanded=true", () => {
    const manyLines = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join("\n");
    const longMsg: AntigravityMessage = {
      step_index: 99,
      role: "assistant",
      content: manyLines,
      created_at: null,
      tool_name: null,
    };
    render(
      <AntigravityMessageItem
        message={longMsg}
        index={0}
        isSelected={false}
        isExpanded={true}
        onClick={vi.fn()}
      />,
    );
    expect(document.querySelector(".message__content--collapsed")).toBeNull();
  });

  it("calls onClick(index) when the row is clicked", () => {
    const onClick = vi.fn();
    render(
      <AntigravityMessageItem
        message={ASSISTANT_MSG}
        index={7}
        isSelected={false}
        isExpanded={false}
        onClick={onClick}
      />,
    );
    fireEvent.click(screen.getByText("نعم، أراه."));
    expect(onClick).toHaveBeenCalledWith(7);
  });

  it("renders the step badge as a discrete chip inside the header", () => {
    // Locks the wire-format chip so it stays visually separated from the
    // role label even at narrow widths. Without .message__step the chip
    // collapses into the role text and the header looks like
    // "Error#42026-09-30 18:40:11" with no breathing room.
    renderOne(ERROR_MSG);
    const header = document.querySelector(".message__header");
    expect(header).not.toBeNull();
    const step = header?.querySelector(".message__step");
    expect(step).not.toBeNull();
    expect(step?.textContent).toBe(`#${ERROR_MSG.step_index}`);
    expect(header?.querySelector(".message__role")).not.toBeNull();
    expect(header?.querySelector(".message__timestamp")).not.toBeNull();
  });
});
