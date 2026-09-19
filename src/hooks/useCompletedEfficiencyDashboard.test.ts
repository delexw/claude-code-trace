import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EfficiencyAnalysisJob } from "../types";
import { useCompletedEfficiencyDashboard } from "./useCompletedEfficiencyDashboard";

function makeJob(status: EfficiencyAnalysisJob["status"]): EfficiencyAnalysisJob {
  return {
    analysisId: "analysis-1",
    sessionId: "session-1",
    sessionPath: "/sessions/one.jsonl",
    sessionName: "Session one",
    status,
    progress: status === "completed" ? 100 : 60,
    message: status,
    updatedAt: "2026-09-19T04:00:00.000Z",
  };
}

describe("useCompletedEfficiencyDashboard", () => {
  it("opens the dashboard when the tracked analysis completes", () => {
    const onCompleted = vi.fn();
    const queuedJob = makeJob("analysing");
    const { result, rerender } = renderHook(
      ({ jobs }) => useCompletedEfficiencyDashboard(jobs, onCompleted),
      { initialProps: { jobs: [queuedJob] } },
    );

    result.current(queuedJob);
    rerender({ jobs: [makeJob("completed")] });

    expect(onCompleted).toHaveBeenCalledOnce();
    expect(onCompleted).toHaveBeenCalledWith("/sessions/one.jsonl");
  });

  it("does not open the dashboard when the tracked analysis fails", () => {
    const onCompleted = vi.fn();
    const queuedJob = makeJob("analysing");
    const { result, rerender } = renderHook(
      ({ jobs }) => useCompletedEfficiencyDashboard(jobs, onCompleted),
      { initialProps: { jobs: [queuedJob] } },
    );

    result.current(queuedJob);
    rerender({ jobs: [makeJob("failed")] });
    rerender({ jobs: [makeJob("completed")] });

    expect(onCompleted).not.toHaveBeenCalled();
  });
});
