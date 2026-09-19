import type { EfficiencyFinding, SessionEfficiencyAnalysis } from "../types";
import { BetaBadge } from "./BetaBadge";
import { EfficiencyChart } from "./EfficiencyChart";

interface EfficiencyPanelProps {
  analysis: SessionEfficiencyAnalysis;
  currentTurns: number;
  onReanalyse: () => void;
  onJumpToFinding: (finding: EfficiencyFinding) => void;
}

const labels: Record<EfficiencyFinding["type"], string> = {
  "repeated-work": "Repeated work",
  thrashing: "Possible thrashing",
  "excessive-exploration": "Repeated exploration",
  "failed-retries": "Failed retries",
  recovery: "Effective recovery",
  "useful-subagent": "Useful subagent",
};

const positive = new Set<EfficiencyFinding["type"]>(["recovery", "useful-subagent"]);

export function EfficiencyPanel({
  analysis,
  currentTurns,
  onReanalyse,
  onJumpToFinding,
}: EfficiencyPanelProps) {
  const changed = analysis.stale || currentTurns !== analysis.analyzedTurns;
  return (
    <section className="efficiency-panel">
      <div className="efficiency-panel__header">
        <span>SESSION EFFICIENCY</span> <BetaBadge />
        <strong className="efficiency-panel__score">{analysis.score}</strong>
      </div>
      <EfficiencyChart dimensions={analysis.dimensions} />
      {changed && (
        <div className="efficiency-panel__stale">
          Analysed at turn {analysis.analyzedTurns}. Session has changed since this analysis.
        </div>
      )}
      <div className="efficiency-panel__findings">
        <strong>{analysis.findings.length} areas worth reviewing</strong>
        {analysis.findings.map((finding) => (
          <button
            key={`${finding.type}-${finding.startMessageIndex}-${finding.endMessageIndex}`}
            type="button"
            className="efficiency-panel__finding"
            onClick={() => onJumpToFinding(finding)}
          >
            <span>{positive.has(finding.type) ? "★" : "⚠"}</span>
            <span>{labels[finding.type]}</span>
            <span>{Math.round(finding.probability * 100)}%</span>
            <span>
              messages {finding.startMessageIndex + 1}–{finding.endMessageIndex + 1}
            </span>
          </button>
        ))}
      </div>
      <div className="efficiency-panel__footer">
        <span>Analysis by Jev · external analysis</span>
        <button type="button" className="settings-modal__btn" onClick={onReanalyse}>
          Re-analyse
        </button>
      </div>
    </section>
  );
}
