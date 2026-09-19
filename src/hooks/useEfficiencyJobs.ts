import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/invoke";
import { listen } from "../lib/listen";
import type { EfficiencyAnalysisJob, EfficiencySummary } from "../types";

export function latestJobsBySession(jobs: EfficiencyAnalysisJob[]) {
  const latest = new Map<string, EfficiencyAnalysisJob>();
  for (const job of jobs) {
    const current = latest.get(job.sessionPath);
    if (!current || Date.parse(job.updatedAt) > Date.parse(current.updatedAt)) {
      latest.set(job.sessionPath, job);
    }
  }
  return latest;
}

export function useEfficiencyJobs() {
  const [jobs, setJobs] = useState<EfficiencyAnalysisJob[]>([]);
  const [summaries, setSummaries] = useState<EfficiencySummary[]>([]);

  const refreshSummaries = useCallback(async () => {
    setSummaries(await invoke<EfficiencySummary[]>("list_efficiency_summaries"));
  }, []);

  const refresh = useCallback(async () => {
    const [nextJobs, nextSummaries] = await Promise.all([
      invoke<EfficiencyAnalysisJob[]>("list_efficiency_analysis_jobs"),
      invoke<EfficiencySummary[]>("list_efficiency_summaries"),
    ]);
    setJobs(nextJobs ?? []);
    setSummaries(nextSummaries ?? []);
  }, []);

  useEffect(() => {
    void Promise.all([
      invoke<EfficiencyAnalysisJob[]>("list_efficiency_analysis_jobs"),
      invoke<EfficiencySummary[]>("list_efficiency_summaries"),
    ])
      .then(([nextJobs, nextSummaries]) => {
        setJobs(nextJobs ?? []);
        setSummaries(nextSummaries ?? []);
      })
      .catch((error) => console.error("Failed to load efficiency state:", error));
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<EfficiencyAnalysisJob>("efficiency-analysis-update", ({ payload }) => {
      if (disposed) return;
      setJobs((current) => {
        const index = current.findIndex((job) => job.analysisId === payload.analysisId);
        if (index < 0) return [...current, payload];
        const next = [...current];
        next[index] = payload;
        return next;
      });
      if (payload.status === "completed") {
        void refreshSummaries().catch((error) =>
          console.error("Failed to refresh efficiency summaries:", error),
        );
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshSummaries]);

  const jobsBySession = useMemo(() => latestJobsBySession(jobs), [jobs]);
  const summariesBySession = useMemo(
    () => new Map(summaries.map((summary) => [summary.sessionPath, summary])),
    [summaries],
  );

  const cancel = useCallback(async (analysisId: string) => {
    await invoke("cancel_efficiency_analysis", { analysisId });
    setJobs((current) =>
      current.map((job) =>
        job.analysisId === analysisId
          ? { ...job, status: "cancelled", progress: null, message: "Analysis cancelled" }
          : job,
      ),
    );
  }, []);

  return { jobs, jobsBySession, summariesBySession, refresh, cancel };
}
