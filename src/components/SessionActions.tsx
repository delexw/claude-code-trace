import { useState } from "react";
import { VscCopy, VscCheck, VscWindow } from "react-icons/vsc";
import type { SessionInfo } from "../types";
import { resumeCommand } from "../lib/resumeCommand";
import { invoke } from "../lib/invoke";

export function SessionActions({
  session,
  canFocus,
}: {
  session: SessionInfo;
  /** Whether this backend can focus a session's terminal window (see
   *  `commands::terminal::can_focus` on the Rust side) — gates the Focus
   *  action instead of `isTauri`, since any local + macOS backend (Tauri app
   *  or browser talking to the HTTP API) can focus a terminal window. */
  canFocus: boolean;
}) {
  const [copied, setCopied] = useState<"resume" | "fork" | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [focusError, setFocusError] = useState<string | null>(null);
  const copy = async (fork: boolean) => {
    setCopyError(null);
    const cmd = resumeCommand(session.cwd, session.session_id, { fork });
    try {
      await navigator.clipboard.writeText(cmd);
      setCopied(fork ? "fork" : "resume");
      window.setTimeout(() => setCopied(null), 1500);
    } catch (err) {
      setCopyError(err instanceof Error ? err.message : String(err));
    }
  };
  const focus = async () => {
    setFocusError(null);
    try {
      await invoke("focus_session_window", { sessionId: session.session_id });
    } catch (err) {
      setFocusError(err instanceof Error ? err.message : String(err));
    }
  };
  // Shown in the tooltip so the button is honest about exactly what it copies.
  const resumeCmd = resumeCommand(session.cwd, session.session_id, { fork: false });
  const forkCmd = resumeCommand(session.cwd, session.session_id, { fork: true });
  return (
    <div className="session-actions">
      {/* Copy actions: the clipboard icon signals "this copies a command to paste in a terminal". */}
      <button
        className={`session-actions__button${copied === "resume" ? " session-actions__button--copied" : ""}`}
        onClick={() => copy(false)}
        aria-label="Copy resume command"
        title={resumeCmd}
      >
        {copied === "resume" ? <VscCheck aria-hidden /> : <VscCopy aria-hidden />}
        {copied === "resume" ? "Copied!" : "Resume"}
      </button>
      <button
        className={`session-actions__button${copied === "fork" ? " session-actions__button--copied" : ""}`}
        onClick={() => copy(true)}
        aria-label="Copy fork command"
        title={forkCmd}
      >
        {copied === "fork" ? <VscCheck aria-hidden /> : <VscCopy aria-hidden />}
        {copied === "fork" ? "Copied!" : "Fork"}
      </button>
      {/* Execute action (backend can focus + session live): no clipboard icon — it acts immediately, not a copy. */}
      {canFocus && session.liveness && (
        <>
          <span className="session-actions__separator" aria-hidden />
          <button
            className="session-actions__button"
            onClick={focus}
            title="Bring this session's terminal window to the front"
          >
            <VscWindow aria-hidden />
            Focus window
          </button>
        </>
      )}
      {copyError && (
        <span className="session-actions__error" role="alert">
          Couldn't copy: {copyError}
        </span>
      )}
      {focusError && (
        <span className="session-actions__error" role="alert">
          {focusError}
        </span>
      )}
    </div>
  );
}
