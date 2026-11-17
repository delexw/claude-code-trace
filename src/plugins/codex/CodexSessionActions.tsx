import { useEffect, useRef, useState } from "react";
import { VscCheck, VscCopy } from "react-icons/vsc";
import type { CodexSession } from "./types";
import { codexSessionCommand } from "./sessionCommand";

export function CodexSessionActions({ session }: { session: CodexSession }) {
  const [copied, setCopied] = useState<"resume" | "fork" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = async (fork: boolean) => {
    const generated = codexSessionCommand(session, { fork });
    if (!generated) return;
    setError(null);
    try {
      await navigator.clipboard.writeText(generated.command);
      setCopied(fork ? "fork" : "resume");
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(null), 1500);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  };
  return (
    <div className="session-actions">
      {(["resume", "fork"] as const).map((action) => {
        const generated = codexSessionCommand(session, { fork: action === "fork" });
        const label = action === "resume" ? "Resume" : "Fork";
        return (
          <button
            key={action}
            className={`session-actions__button${copied === action ? " session-actions__button--copied" : ""}`}
            disabled={!generated}
            aria-label={`Copy Codex ${action} command`}
            title={
              generated
                ? `Paste in ${generated.shell} in this chat's environment: ${generated.command}`
                : "The recorded chat location cannot be used to build a command."
            }
            onClick={() => void copy(action === "fork")}
          >
            {copied === action ? <VscCheck aria-hidden /> : <VscCopy aria-hidden />}
            {copied === action ? "Copied!" : label}
          </button>
        );
      })}
      {error && (
        <span className="session-actions__error" role="alert">
          Couldn't copy: {error}
        </span>
      )}
    </div>
  );
}
