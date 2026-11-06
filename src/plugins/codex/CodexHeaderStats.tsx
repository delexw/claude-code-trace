import { CostIcon, TokensIcon } from "../../components/Icons";
import { formatCost, formatTokens } from "../../lib/format";
import { COST_ESTIMATE_HINT, knownCost } from "./CodexCost";
import type { CodexSession } from "./types";

export function CodexHeaderStats({ sessions }: { sessions: CodexSession[] }) {
  if (sessions.length === 0) return null;
  let totalTokens = 0;
  let totalCost = 0;
  let priced = 0;
  for (const session of sessions) {
    totalTokens += session.total_tokens;
    if (knownCost(session.cost_usd)) {
      totalCost += session.cost_usd;
      priced++;
    }
  }
  const complete = priced === sessions.length;
  return (
    <>
      {totalTokens > 0 && (
        <span className="picker__total-tokens">
          <TokensIcon /> {formatTokens(totalTokens)} tok
        </span>
      )}
      {priced > 0 ? (
        <span
          className="picker__total-cost"
          aria-label="Estimated Codex API token cost"
          title={
            complete
              ? COST_ESTIMATE_HINT
              : `Estimated API token cost for ${priced} of ${sessions.length} chats; remaining costs unavailable. ${COST_ESTIMATE_HINT}`
          }
        >
          <CostIcon /> ≈ ${formatCost(totalCost)}
          {!complete && "+"}
        </span>
      ) : (
        <span
          className="picker__total-tokens"
          aria-label="Codex API token cost unavailable"
          title="API token costs are unavailable because usage or pricing is incomplete."
        >
          Cost unavailable
        </span>
      )}
    </>
  );
}
