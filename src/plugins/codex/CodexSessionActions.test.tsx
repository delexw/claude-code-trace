import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { CodexSessionActions } from "./CodexSessionActions";
import type { CodexSession } from "./types";

const writeText = vi.fn();
const session = {
  id: "00000000-0000-0000-0000-000000000123",
  cwd: "/work/calculator",
  path: "/custom/codex/sessions/rollout-fabricated.jsonl",
} as CodexSession;

beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
});

describe("CodexSessionActions", () => {
  it.each(["resume", "fork"])(
    "copies a %s command with feedback and an honest shell tooltip",
    async (action) => {
      render(<CodexSessionActions session={session} />);
      const button = screen.getByRole("button", { name: `Copy Codex ${action} command` });
      expect(button.title).toContain("POSIX shell");
      fireEvent.click(button);
      expect(writeText).toHaveBeenCalledWith(
        `CODEX_HOME='/custom/codex' codex -C '/work/calculator' ${action} -- '${session.id}'`,
      );
      expect(await screen.findByText("Copied!")).toBeInTheDocument();
    },
  );

  it("shows clipboard failures and lets the user retry", async () => {
    writeText.mockRejectedValueOnce(new Error("clipboard blocked"));
    render(<CodexSessionActions session={session} />);
    const button = screen.getByRole("button", { name: "Copy Codex resume command" });
    fireEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't copy: clipboard blocked");
    fireEvent.click(button);
    expect(await screen.findByText("Copied!")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("disables copy when recorded paths cannot target the chat environment", () => {
    render(<CodexSessionActions session={{ ...session, cwd: "" }} />);
    expect(screen.getByRole("button", { name: "Copy Codex resume command" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Copy Codex fork command" })).toBeDisabled();
    expect(writeText).not.toHaveBeenCalled();
  });
});
