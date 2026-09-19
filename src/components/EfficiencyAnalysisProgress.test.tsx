import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EfficiencyAnalysisJob } from "../types";
import { EfficiencyAnalysisProgress } from "./EfficiencyAnalysisProgress";

const completedJob: EfficiencyAnalysisJob = {
  analysisId: "analysis-1",
  sessionId: "session-1",
  sessionPath: "/private/session.jsonl",
  sessionName: "Fix login",
  status: "completed",
  progress: 100,
  message: "Complete",
  score: 82,
  updatedAt: "2026-09-19T04:00:00.000Z",
};

describe("EfficiencyAnalysisProgress", () => {
  afterEach(() => vi.useRealTimers());

  it("briefly shows a completed job, then removes it from the progress strip", () => {
    vi.useFakeTimers();
    vi.setSystemTime("2026-09-19T04:00:00.000Z");
    render(
      <EfficiencyAnalysisProgress
        jobs={[completedJob]}
        onOpenSession={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("✓ Analysis complete — Efficiency 82")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(5_000));
    expect(screen.queryByLabelText("Jev analysis progress")).not.toBeInTheDocument();
  });
});
