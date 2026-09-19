import { useCallback, useEffect, useRef } from "react";
import type { EfficiencyAnalysisJob } from "../types";

export function useCompletedEfficiencyDashboard(
  jobs: EfficiencyAnalysisJob[],
  onCompleted: (sessionPath: string) => void,
) {
  const pendingJobRef = useRef<{ analysisId: string; sessionPath: string } | null>(null);

  const openDashboardWhenCompleted = useCallback((job: EfficiencyAnalysisJob) => {
    pendingJobRef.current = {
      analysisId: job.analysisId,
      sessionPath: job.sessionPath,
    };
  }, []);

  useEffect(() => {
    const pendingJob = pendingJobRef.current;
    if (!pendingJob) return;

    const currentJob = jobs.find((job) => job.analysisId === pendingJob.analysisId);
    if (!currentJob) return;

    if (currentJob.status === "completed") {
      pendingJobRef.current = null;
      onCompleted(pendingJob.sessionPath);
    } else if (currentJob.status === "failed" || currentJob.status === "cancelled") {
      pendingJobRef.current = null;
    }
  }, [jobs, onCompleted]);

  return openDashboardWhenCompleted;
}
