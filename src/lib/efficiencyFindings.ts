import type { EfficiencyFinding } from "../types";

const findingTypeOrder: Record<EfficiencyFinding["type"], number> = {
  "repeated-work": 0,
  thrashing: 1,
  "excessive-exploration": 2,
  "failed-retries": 3,
  recovery: 4,
  "useful-subagent": 5,
};

/**
 * Merge same-type findings whose message evidence overlaps. This also cleans up
 * cached analyses created before the backend started merging action windows.
 */
export function mergeOverlappingEfficiencyFindings(
  findings: readonly EfficiencyFinding[],
): EfficiencyFinding[] {
  const sorted = findings
    .map((finding) => ({ ...finding }))
    .toSorted(
      (left, right) =>
        findingTypeOrder[left.type] - findingTypeOrder[right.type] ||
        left.startMessageIndex - right.startMessageIndex ||
        left.endMessageIndex - right.endMessageIndex,
    );
  const merged: EfficiencyFinding[] = [];

  for (const finding of sorted) {
    const previous = merged.at(-1);
    if (
      previous &&
      previous.type === finding.type &&
      finding.startMessageIndex <= previous.endMessageIndex
    ) {
      previous.endMessageIndex = Math.max(previous.endMessageIndex, finding.endMessageIndex);
      if (finding.probability > previous.probability) {
        previous.probability = finding.probability;
        previous.activitySummary = finding.activitySummary ?? previous.activitySummary;
      } else if (!previous.activitySummary) {
        previous.activitySummary = finding.activitySummary;
      }
    } else {
      merged.push(finding);
    }
  }

  return merged.toSorted(
    (left, right) =>
      left.startMessageIndex - right.startMessageIndex ||
      findingTypeOrder[left.type] - findingTypeOrder[right.type],
  );
}
