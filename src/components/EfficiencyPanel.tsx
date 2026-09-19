import { mergeOverlappingEfficiencyFindings } from "../lib/efficiencyFindings";
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

interface FindingGroupProps {
  title: string;
  findings: EfficiencyFinding[];
  onJumpToFinding: (finding: EfficiencyFinding) => void;
}

function FindingGroup({ title, findings, onJumpToFinding }: FindingGroupProps) {
  return (
    <div className="efficiency-panel__finding-group">
      <strong>{title}</strong>
      {findings.map((finding) => (
        <button
          key={`${finding.type}-${finding.startMessageIndex}-${finding.endMessageIndex}`}
          type="button"
          className="efficiency-panel__finding"
          onClick={() => onJumpToFinding(finding)}
        >
          <span aria-hidden="true">{positive.has(finding.type) ? "★" : "⚠"}</span>
          <span className="efficiency-panel__finding-label">{labels[finding.type]}</span>
          <span className="efficiency-panel__finding-likelihood">
            {Math.round(finding.probability * 100)}% likelihood
          </span>
          <span className="efficiency-panel__finding-activity">
            {finding.activitySummary || "Open related tool activity"}
          </span>
        </button>
      ))}
    </div>
  );
}

function countLabel(count: number, singular: string, plural: string) {
  return `${count} ${count === 1 ? singular : plural}`;
}

export function EfficiencyPanel({
  analysis,
  currentTurns,
  onReanalyse,
  onJumpToFinding,
}: EfficiencyPanelProps) {
  const changed = analysis.stale || currentTurns !== analysis.analyzedTurns;
  const findings = mergeOverlappingEfficiencyFindings(analysis.findings);
  const strengths = findings.filter((finding) => positive.has(finding.type));
  const improvements = findings.filter((finding) => !positive.has(finding.type));
  return (
    <section className="efficiency-panel">
      <div className="efficiency-panel__header">
        <span>SESSION EFFICIENCY</span> <BetaBadge />
        <strong className="efficiency-panel__score">{analysis.score}</strong>
      </div>
      <EfficiencyChart metrics={analysis.metricEvaluations} />
      {changed && (
        <div className="efficiency-panel__stale">
          Analysed at turn {analysis.analyzedTurns}. Session has changed since this analysis.
        </div>
      )}
      <div className="efficiency-panel__findings">
        {improvements.length > 0 && (
          <FindingGroup
            title={countLabel(improvements.length, "area to improve", "areas to improve")}
            findings={improvements}
            onJumpToFinding={onJumpToFinding}
          />
        )}
        {strengths.length > 0 && (
          <FindingGroup
            title={countLabel(strengths.length, "strength", "strengths")}
            findings={strengths}
            onJumpToFinding={onJumpToFinding}
          />
        )}
        {findings.length === 0 && <strong>No notable patterns detected</strong>}
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
