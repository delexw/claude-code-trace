import { CostIcon } from "../../components/Icons";
import { formatCost } from "../../lib/format";

export const COST_ESTIMATE_HINT =
  "Estimated API-equivalent model token cost. Subscription charges and tool fees may differ.";

export function knownCost(cost: number | null | undefined): cost is number {
  return typeof cost === "number" && Number.isFinite(cost) && cost >= 0;
}

export function CodexCost({
  cost,
  className,
  partial = false,
}: {
  cost: number | null;
  className: string;
  partial?: boolean;
}) {
  if (!knownCost(cost)) {
    return (
      <span
        className="codex-cost-unavailable"
        aria-label="API token cost unavailable"
        title="API token cost is unavailable because usage or pricing is incomplete."
      >
        Cost unavailable
      </span>
    );
  }
  return (
    <span
      className={className}
      aria-label="Estimated API token cost"
      title={
        partial
          ? `Known subtotal; additional worker costs are unavailable. ${COST_ESTIMATE_HINT}`
          : COST_ESTIMATE_HINT
      }
    >
      <CostIcon /> ≈ ${formatCost(cost)}
      {partial && "+"}
    </span>
  );
}
