import { useEffect, useState } from "react";
import type { EfficiencyAnalysisJob } from "../types";

interface EfficiencyAnalysisProgressProps {
  jobs: EfficiencyAnalysisJob[];
  onOpenSession: (path: string) => void;
  onCancel: (analysisId: string) => void;
}

const activeStatuses = new Set([
  "queued",
  "preparing",
  "redacting",
  "sending",
  "analysing",
  "processing_result",
]);

export function EfficiencyAnalysisProgress({
  jobs,
  onOpenSession,
  onCancel,
}: EfficiencyAnalysisProgressProps) {
  const [now, setNow] = useState(() => Date.now());
  const latestCompletionExpiry = jobs.reduce(
    (latest, job) =>
      job.status === "completed" ? Math.max(latest, Date.parse(job.updatedAt) + 5_000) : latest,
    0,
  );
  useEffect(() => {
    if (latestCompletionExpiry <= now) return;
    const timer = window.setTimeout(() => setNow(Date.now()), latestCompletionExpiry - now);
    return () => window.clearTimeout(timer);
  }, [latestCompletionExpiry, now]);

  const visible = jobs.filter(
    (job) =>
      activeStatuses.has(job.status) ||
      job.status === "failed" ||
      (job.status === "completed" && now - Date.parse(job.updatedAt) < 5_000),
  );
  if (visible.length === 0) return null;
  return (
    <section className="efficiency-progress" aria-label="Jev analysis progress">
      <div className="efficiency-progress__title">Jev Analysis</div>
      {visible.map((job) => (
        <div
          key={job.analysisId}
          className={`efficiency-progress__job efficiency-progress__job--${job.status}`}
        >
          <button
            type="button"
            className="efficiency-progress__session"
            onClick={() => onOpenSession(job.sessionPath)}
          >
            {job.sessionName || job.sessionId}
          </button>
          <div
            className={`efficiency-progress__track${job.progress == null ? " efficiency-progress__track--indeterminate" : ""}`}
            aria-label={job.progress == null ? job.message : `${job.progress}% ${job.message}`}
          >
            {job.progress != null && <span style={{ width: `${job.progress}%` }} />}
          </div>
          <span className="efficiency-progress__message">
            {job.status === "completed" && job.score != null
              ? `✓ Analysis complete — Efficiency ${job.score}`
              : job.error || job.message}
          </span>
          {activeStatuses.has(job.status) && (
            <button
              type="button"
              className="efficiency-progress__cancel"
              onClick={() => onCancel(job.analysisId)}
            >
              Cancel
            </button>
          )}
        </div>
      ))}
    </section>
  );
}
