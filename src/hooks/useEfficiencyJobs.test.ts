import { describe, expect, it } from "vitest";
import type { EfficiencyAnalysisJob } from "../types";
import { latestJobsBySession } from "./useEfficiencyJobs";

function job(analysisId: string, updatedAt: string): EfficiencyAnalysisJob {
  return {
    analysisId,
    sessionId: "session-1",
    sessionPath: "/private/session.jsonl",
    sessionName: "Fix login",
    status: "completed",
    progress: 100,
    message: "Complete",
    updatedAt,
  };
}

describe("latestJobsBySession", () => {
  it("chooses the newest job regardless of backend order", () => {
    const newer = job("newer", "2026-09-19T04:01:00.000Z");
    const older = job("older", "2026-09-19T04:00:00.000Z");

    expect(latestJobsBySession([newer, older]).get(older.sessionPath)?.analysisId).toBe("newer");
  });
});
