import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SessionEfficiencyAnalysis } from "../types";
import { EfficiencyPanel } from "./EfficiencyPanel";

const analysis: SessionEfficiencyAnalysis = {
  sessionId: "session-1",
  sessionPath: "/private/session.jsonl",
  score: 24,
  dimensions: {
    progress: 12,
    toolUse: 20,
    focus: 50,
    exploration: 59,
    recovery: 19,
    tokenUse: 9,
  },
  metricEvaluations: [
    {
      key: "progressingEfficiently",
      label: "Progress",
      question: "Did the agent make progress?",
      higherProbabilityIsBetter: true,
      probability: 0.12,
      score: 12,
    },
  ],
  findings: [
    {
      type: "thrashing",
      probability: 0.82,
      startMessageIndex: 1,
      endMessageIndex: 1,
      activitySummary: "4 tool calls · Read, Bash",
    },
    {
      type: "recovery",
      probability: 0.75,
      startMessageIndex: 2,
      endMessageIndex: 2,
    },
    {
      type: "recovery",
      probability: 0.78,
      startMessageIndex: 2,
      endMessageIndex: 4,
      activitySummary: "15 tool calls · Read, Edit, Bash",
    },
    {
      type: "recovery",
      probability: 0.76,
      startMessageIndex: 4,
      endMessageIndex: 4,
    },
  ],
  decisions: {
    progressingEfficiently: 0.12,
    toolCallsUseful: 0.2,
    redundantWorkPresent: 0.4,
    excessiveExploration: 0.41,
    likelyThrashing: 0.6,
    effectiveRecovery: 0.19,
    tokenUsageEfficient: 0.09,
    subagentsUseful: 0.5,
    likelyTaskCompleted: 0.2,
  },
  analyzedAt: "2026-09-19T04:00:00.000Z",
  analyzedTurns: 2,
  transcriptFingerprint: "fingerprint",
  analysisVersion: 1,
  decisionSetVersion: 1,
  scoreFormulaVersion: 1,
  stale: false,
};

describe("EfficiencyPanel", () => {
  it("separates improvements from strengths and describes the evidence action", () => {
    const onJumpToFinding = vi.fn();
    render(
      <EfficiencyPanel
        analysis={analysis}
        currentTurns={2}
        onReanalyse={vi.fn()}
        onJumpToFinding={onJumpToFinding}
      />,
    );

    const finding = screen.getByRole("button", { name: /Possible thrashing/ });

    expect(screen.getByText("1 area to improve")).toBeInTheDocument();
    expect(screen.getByText("1 strength")).toBeInTheDocument();
    expect(within(finding).getByText("Possible thrashing")).toHaveClass(
      "efficiency-panel__finding-label",
    );
    expect(within(finding).getByText("82% likelihood")).toHaveClass(
      "efficiency-panel__finding-likelihood",
    );
    expect(within(finding).getByText("4 tool calls · Read, Bash")).toBeInTheDocument();
    expect(screen.queryByText(/messages \d/)).not.toBeInTheDocument();

    const recovery = screen.getByText("15 tool calls · Read, Edit, Bash").closest("button")!;
    expect(within(recovery).getByText("78% likelihood")).toBeInTheDocument();
    expect(within(recovery).getByText("15 tool calls · Read, Edit, Bash")).toBeInTheDocument();
    fireEvent.click(recovery);
    expect(onJumpToFinding).toHaveBeenCalledWith({
      type: "recovery",
      probability: 0.78,
      startMessageIndex: 2,
      endMessageIndex: 4,
      activitySummary: "15 tool calls · Read, Edit, Bash",
    });
  });
});
