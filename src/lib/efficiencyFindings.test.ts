import { describe, expect, it } from "vitest";
import type { EfficiencyFinding } from "../types";
import { mergeOverlappingEfficiencyFindings } from "./efficiencyFindings";

describe("mergeOverlappingEfficiencyFindings", () => {
  it("merges overlapping same-type findings using their full range and highest confidence", () => {
    const findings: EfficiencyFinding[] = [
      {
        type: "recovery",
        probability: 0.75,
        startMessageIndex: 2,
        endMessageIndex: 2,
        activitySummary: "6 tool calls · Read",
      },
      {
        type: "recovery",
        probability: 0.78,
        startMessageIndex: 2,
        endMessageIndex: 4,
        activitySummary: "15 tool calls · Read, Edit, Bash",
      },
      { type: "recovery", probability: 0.76, startMessageIndex: 4, endMessageIndex: 4 },
    ];

    expect(mergeOverlappingEfficiencyFindings(findings)).toEqual([
      {
        type: "recovery",
        probability: 0.78,
        startMessageIndex: 2,
        endMessageIndex: 4,
        activitySummary: "15 tool calls · Read, Edit, Bash",
      },
    ]);
  });

  it("keeps different types and non-overlapping evidence separate", () => {
    const findings: EfficiencyFinding[] = [
      { type: "recovery", probability: 0.75, startMessageIndex: 2, endMessageIndex: 2 },
      { type: "thrashing", probability: 0.8, startMessageIndex: 2, endMessageIndex: 2 },
      { type: "recovery", probability: 0.9, startMessageIndex: 5, endMessageIndex: 5 },
    ];

    expect(mergeOverlappingEfficiencyFindings(findings)).toHaveLength(3);
  });
});
