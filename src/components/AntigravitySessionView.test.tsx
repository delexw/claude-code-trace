import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AntigravitySessionView } from "./AntigravitySessionView";
import type { AntigravitySession } from "../types";

const SESSION: AntigravitySession = {
  folder: {
    uuid: "11111111-2222-3333-4444-555555555555",
    path: "/home/u/.gemini/antigravity-ide/brain/11111111",
    mod_time_ms: 1790776853673,
    title: "ضيف سكشن لاختيار ملف من المسار",
    flavor: "antigravity-ide",
    has_transcript: true,
    provider_id: "antigravity",
  },
  messages: [
    {
      step_index: 0,
      role: "user",
      content: "<USER_REQUEST>أهلاً</USER_REQUEST>",
      created_at: "2026-09-29T20:08:27Z",
      tool_name: null,
    },
    {
      step_index: 1,
      role: "assistant",
      content: "أهلاً بك",
      created_at: "2026-09-29T20:08:30Z",
      tool_name: null,
    },
    {
      step_index: 2,
      role: "tool",
      content: "tool result here",
      created_at: "2026-09-29T20:08:31Z",
      tool_name: null,
    },
  ],
};

describe("AntigravitySessionView", () => {
  it("renders title (falls back to uuid) + step count + each message", () => {
    render(<AntigravitySessionView session={SESSION} onBack={vi.fn()} onOpenFolder={vi.fn()} />);
    // Title shown in the topbar.
    expect(screen.getByText(SESSION.folder.title!)).toBeInTheDocument();
    expect(screen.getByText("3 steps")).toBeInTheDocument();
    // Every message content is in the document.
    for (const msg of SESSION.messages) {
      expect(screen.getByText(msg.content)).toBeInTheDocument();
    }
  });

  it("falls back to the uuid when title is missing", () => {
    const session: AntigravitySession = {
      ...SESSION,
      folder: { ...SESSION.folder, title: null },
    };
    render(<AntigravitySessionView session={session} onBack={vi.fn()} onOpenFolder={vi.fn()} />);
    expect(screen.getByText(SESSION.folder.uuid)).toBeInTheDocument();
  });

  it("calls onBack when the back button is clicked", () => {
    const onBack = vi.fn();
    render(<AntigravitySessionView session={SESSION} onBack={onBack} onOpenFolder={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /back/i }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("calls onOpenFolder when the open-folder button is clicked", () => {
    const onOpenFolder = vi.fn();
    render(
      <AntigravitySessionView session={SESSION} onBack={vi.fn()} onOpenFolder={onOpenFolder} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /open folder/i }));
    expect(onOpenFolder).toHaveBeenCalledTimes(1);
  });

  it("Expand all / Collapse all toggle every row at once", () => {
    render(<AntigravitySessionView session={SESSION} onBack={vi.fn()} onOpenFolder={vi.fn()} />);
    // Default: only index 0 is expanded. Only step_index 0 has no
    // message__content--collapsed modifier.
    expect(document.querySelectorAll(".message__content--collapsed").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole("button", { name: /expand all/i }));
    // After expand all, every message body is uncollapsed.
    expect(document.querySelectorAll(".message__content--collapsed").length).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: /collapse all/i }));
    // After collapse all, every message body is collapsed again.
    expect(document.querySelectorAll(".message__content--collapsed").length).toBe(
      SESSION.messages.length,
    );
  });

  it("renders the empty state when the transcript has no messages", () => {
    const emptySession: AntigravitySession = {
      ...SESSION,
      messages: [],
    };
    render(
      <AntigravitySessionView session={emptySession} onBack={vi.fn()} onOpenFolder={vi.fn()} />,
    );
    expect(screen.getByText(/No transcript found at/i)).toBeInTheDocument();
  });
});
