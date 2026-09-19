import type { EfficiencyFinding, SessionEfficiencyAnalysis } from "../types";
import { EfficiencyPanel } from "./EfficiencyPanel";
import { PopoutModal } from "./PopoutModal";

interface EfficiencyDashboardModalProps {
  sessionName: string;
  currentTurns: number;
  analysis: SessionEfficiencyAnalysis | null;
  error: string;
  onClose: () => void;
  onReanalyse: () => void;
  onJumpToFinding: (finding: EfficiencyFinding) => void;
}

export function EfficiencyDashboardModal({
  sessionName,
  currentTurns,
  analysis,
  error,
  onClose,
  onReanalyse,
  onJumpToFinding,
}: EfficiencyDashboardModalProps) {
  return (
    <PopoutModal
      onClose={onClose}
      header={
        <div className="efficiency-dashboard-modal__header">
          <h2 id="efficiency-dashboard-title">Efficiency Dashboard</h2>
          <span title={sessionName}>{sessionName}</span>
        </div>
      }
      initialWidth={900}
      initialHeight={620}
    >
      <div
        className="efficiency-dashboard-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="efficiency-dashboard-title"
      >
        {error ? (
          <div className="efficiency-dashboard-modal__status" role="alert">
            {error}
          </div>
        ) : analysis ? (
          <EfficiencyPanel
            analysis={analysis}
            currentTurns={currentTurns}
            onReanalyse={onReanalyse}
            onJumpToFinding={onJumpToFinding}
          />
        ) : (
          <div className="efficiency-dashboard-modal__status">
            <span className="braille-spinner" /> Loading dashboard…
          </div>
        )}
      </div>
    </PopoutModal>
  );
}
