import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SessionEfficiencyAnalysis } from "../types";
import { EfficiencyDashboardModal } from "./EfficiencyDashboardModal";

const analysis: SessionEfficiencyAnalysis = {
  sessionId: "session-1",
  sessionPath: "/private/session.jsonl",
  score: 82,
  dimensions: {
    progress: 80,
    toolUse: 70,
    focus: 60,
    exploration: 50,
    recovery: 40,
    tokenUse: 30,
  },
  metricEvaluations: [
    {
      key: "progressingEfficiently",
      label: "Progress",
      question: "Did the agent make progress?",
      higherProbabilityIsBetter: true,
      probability: 0.8,
      score: 80,
    },
  ],
  findings: [],
  decisions: {
    progressingEfficiently: 0.8,
    toolCallsUseful: 0.7,
    redundantWorkPresent: 0.4,
    excessiveExploration: 0.5,
    likelyThrashing: 0.4,
    effectiveRecovery: 0.4,
    tokenUsageEfficient: 0.3,
    subagentsUseful: 0.5,
    likelyTaskCompleted: 0.8,
  },
  analyzedAt: "2026-09-19T04:00:00.000Z",
  analyzedTurns: 5,
  transcriptFingerprint: "fingerprint",
  analysisVersion: 1,
  decisionSetVersion: 1,
  scoreFormulaVersion: 1,
  stale: false,
};

describe("EfficiencyDashboardModal", () => {
  it("shows a completed dashboard without navigating to the session", () => {
    render(
      <EfficiencyDashboardModal
        sessionName="Fix login"
        currentTurns={5}
        analysis={analysis}
        error=""
        onClose={vi.fn()}
        onReanalyse={vi.fn()}
        onJumpToFinding={vi.fn()}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Efficiency Dashboard" })).toBeInTheDocument();
    expect(screen.getByText("Fix login")).toBeInTheDocument();
    expect(screen.getByText("82")).toBeInTheDocument();
  });

  it("shows loading and error states and can be closed", () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <EfficiencyDashboardModal
        sessionName="Fix login"
        currentTurns={5}
        analysis={null}
        error=""
        onClose={onClose}
        onReanalyse={vi.fn()}
        onJumpToFinding={vi.fn()}
      />,
    );
    expect(screen.getByText("Loading dashboard…")).toBeInTheDocument();

    rerender(
      <EfficiencyDashboardModal
        sessionName="Fix login"
        currentTurns={5}
        analysis={null}
        error="Dashboard unavailable"
        onClose={onClose}
        onReanalyse={vi.fn()}
        onJumpToFinding={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Dashboard unavailable");
    fireEvent.click(document.querySelector(".popout-modal__close")!);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
