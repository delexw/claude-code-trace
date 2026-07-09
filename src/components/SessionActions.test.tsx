import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SessionActions } from "./SessionActions";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const writeText = vi.fn();
beforeEach(() => {
  writeText.mockReset();
  mockInvoke.mockReset();
  Object.assign(navigator, { clipboard: { writeText } });
});
const base = { session_id: "u1", cwd: "/w" /* …other SessionInfo fields… */ } as any;

describe("SessionActions", () => {
  it("copies the resume command", () => {
    render(<SessionActions session={{ ...base, liveness: null }} canFocus={false} />);
    fireEvent.click(screen.getByRole("button", { name: /copy resume/i }));
    expect(writeText).toHaveBeenCalledWith("cd '/w' && claude --resume u1");
  });

  it("copies a fork command", () => {
    render(<SessionActions session={{ ...base, liveness: null }} canFocus={false} />);
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));
    expect(writeText).toHaveBeenCalledWith("cd '/w' && claude --resume u1 --fork-session");
  });

  it("gives visual feedback (Copied!) after a successful copy", async () => {
    render(<SessionActions session={{ ...base, liveness: null }} canFocus={false} />);
    // accessible name stays the honest aria-label; the visible label flips to "Copied!"
    fireEvent.click(screen.getByRole("button", { name: /copy resume/i }));
    expect(await screen.findByText(/copied!/i)).toBeInTheDocument();
  });

  it("shows an inline error when the clipboard write fails", async () => {
    writeText.mockRejectedValueOnce(new Error("clipboard blocked"));
    render(<SessionActions session={{ ...base, liveness: null }} canFocus={false} />);
    fireEvent.click(screen.getByRole("button", { name: /copy resume/i }));
    expect(await screen.findByText(/couldn't copy: clipboard blocked/i)).toBeInTheDocument();
  });

  describe("Focus window button", () => {
    it("shows Focus when canFocus and the session is live", () => {
      render(
        <SessionActions
          session={{ ...base, liveness: { status: "idle", idle_seconds: 60, pid: 9 } }}
          canFocus={true}
        />,
      );
      expect(screen.getByRole("button", { name: /focus window/i })).toBeEnabled();
    });

    it("hides Focus when canFocus is false, even when the session is live", () => {
      render(
        <SessionActions
          session={{ ...base, liveness: { status: "idle", idle_seconds: 60, pid: 9 } }}
          canFocus={false}
        />,
      );
      expect(screen.queryByRole("button", { name: /focus window/i })).toBeNull();
    });

    it("hides Focus when the session isn't live, even when canFocus is true", () => {
      render(<SessionActions session={{ ...base, liveness: null }} canFocus={true} />);
      expect(screen.queryByRole("button", { name: /focus window/i })).toBeNull();
    });

    it("invokes focus_session_window with the session id when clicked", () => {
      mockInvoke.mockResolvedValue(undefined);
      render(
        <SessionActions
          session={{ ...base, liveness: { status: "idle", idle_seconds: 60, pid: 9 } }}
          canFocus={true}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: /focus window/i }));
      expect(mockInvoke).toHaveBeenCalledWith("focus_session_window", { sessionId: "u1" });
    });

    it("renders an inline error when focus_session_window rejects", async () => {
      mockInvoke.mockRejectedValue(new Error("boom"));
      render(
        <SessionActions
          session={{ ...base, liveness: { status: "idle", idle_seconds: 60, pid: 9 } }}
          canFocus={true}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: /focus window/i }));
      expect(await screen.findByText(/boom/i)).toBeInTheDocument();
    });
  });
});
