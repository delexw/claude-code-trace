import { useCallback, useEffect, useRef, useState } from "react";
import { BetaBadge } from "../../components/BetaBadge";
import { EfficiencyAnalysisProgress } from "../../components/EfficiencyAnalysisProgress";
import { EfficiencyDashboardModal } from "../../components/EfficiencyDashboardModal";
import { EfficiencyPanel } from "../../components/EfficiencyPanel";
import { EfficiencyPrivacyModal } from "../../components/EfficiencyPrivacyModal";
import { JevKeyRequiredModal } from "../../components/JevKeyRequiredModal";
import { OperationResultModal } from "../../components/OperationResultModal";
import { useCompletedEfficiencyDashboard } from "../../hooks/useCompletedEfficiencyDashboard";
import { useEfficiencyJobs } from "../../hooks/useEfficiencyJobs";
import { invoke } from "../../lib/invoke";
import type {
  AnalyticsSettings,
  EfficiencyAnalysisJob,
  EfficiencyFinding,
  PreparedEfficiencyPayload,
  SessionEfficiencyAnalysis,
} from "../../types";
import type { CodexSession } from "./types";

interface Props {
  session: CodexSession;
  currentTurns: number;
  onJumpToFinding(finding: EfficiencyFinding): void;
  openSettings(): void;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A fresh instance per chat prevents consent or in-flight UI work from carrying
 * over when the user switches conversations. */
export function CodexEfficiency(props: Props) {
  return <SessionEfficiency key={props.session.path} {...props} />;
}

function SessionEfficiency({ session, currentTurns, onJumpToFinding, openSettings }: Props) {
  const [analysis, setAnalysis] = useState<SessionEfficiencyAnalysis | null>(null);
  const [prepared, setPrepared] = useState<PreparedEfficiencyPayload | null>(null);
  const [preparing, setPreparing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [keyRequired, setKeyRequired] = useState(false);
  const [dashboard, setDashboard] = useState(false);
  const [error, setError] = useState("");
  const mounted = useRef(true);
  const prepareRequest = useRef(0);
  const startPending = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reportFailure = useCallback(
    (job: EfficiencyAnalysisJob) => {
      if (job.sessionPath === session.path && mounted.current)
        setError(`${session.title || session.id}: ${job.error || job.message}`);
    },
    [session.path, session.title, session.id],
  );
  const jobs = useEfficiencyJobs(reportFailure);
  const job = jobs.jobsBySession.get(session.path);
  const summaryRevision = jobs.summariesBySession.get(session.path)?.analyzedAt;
  const completedRevision = job?.status === "completed" ? job.updatedAt : undefined;
  const running = job && !["completed", "failed", "cancelled"].includes(job.status);
  const openDashboard = useCallback(() => setDashboard(true), []);
  const watchCompletion = useCompletedEfficiencyDashboard(jobs.jobs, openDashboard);

  useEffect(() => {
    let disposed = false;
    void invoke<SessionEfficiencyAnalysis | null>("get_session_efficiency", { path: session.path })
      .then((result) => {
        if (!disposed) setAnalysis(result);
      })
      .catch((failure) => {
        if (!disposed) setError(errorText(failure));
      });
    return () => {
      disposed = true;
    };
    // oxlint-disable-next-line react/exhaustive-effect-dependencies -- a changed backend cache/job revision requires a fresh cache read
  }, [session.path, summaryRevision, completedRevision]);

  const requestAnalysis = async () => {
    if (preparing || starting || running) return;
    const request = ++prepareRequest.current;
    setPreparing(true);
    setError("");
    try {
      const settings = await invoke<AnalyticsSettings>("get_analytics_settings");
      if (!mounted.current || request !== prepareRequest.current) return;
      if (!settings.jev.configured) {
        setKeyRequired(true);
        return;
      }
      const payload = await invoke<PreparedEfficiencyPayload>("prepare_codex_efficiency_payload", {
        path: session.path,
        payloadMode: settings.defaultPayloadMode,
      });
      if (!mounted.current || request !== prepareRequest.current) return;
      if (payload.sessionPath !== session.path)
        throw new Error("Prepared analysis belongs to another chat.");
      setPrepared(payload);
    } catch (failure) {
      if (mounted.current && request === prepareRequest.current) setError(errorText(failure));
    } finally {
      if (mounted.current && request === prepareRequest.current) setPreparing(false);
    }
  };

  const confirm = async () => {
    if (!prepared || prepared.sessionPath !== session.path || startPending.current) return;
    startPending.current = true;
    setStarting(true);
    setError("");
    try {
      const started = await invoke<EfficiencyAnalysisJob>("start_session_efficiency_analysis", {
        path: session.path,
        payload: prepared,
      });
      if (!mounted.current) return;
      watchCompletion(started);
      setPrepared(null);
      await jobs.refresh();
    } catch (failure) {
      if (mounted.current) setError(errorText(failure));
    } finally {
      startPending.current = false;
      if (mounted.current) setStarting(false);
    }
  };

  const jumpToFinding = (finding: EfficiencyFinding) => {
    setDashboard(false);
    onJumpToFinding(finding);
  };

  return (
    <div className="session-efficiency-action">
      {analysis && !running ? (
        <EfficiencyPanel
          analysis={analysis}
          currentTurns={currentTurns}
          onReanalyse={() => void requestAnalysis()}
          onJumpToFinding={jumpToFinding}
        />
      ) : (
        <button
          type="button"
          className="settings-modal__btn"
          disabled={preparing || starting || Boolean(running)}
          onClick={() => void requestAnalysis()}
        >
          {preparing ? "Preparing analysis..." : "Analyse efficiency"} <BetaBadge />
        </button>
      )}
      <EfficiencyAnalysisProgress
        jobs={job ? [job] : []}
        onOpenSession={() => {}}
        onOpenDashboard={openDashboard}
        onCancel={(analysisId) =>
          void jobs.cancel(analysisId).catch((failure) => {
            if (mounted.current) setError(errorText(failure));
          })
        }
      />
      {prepared && (
        <EfficiencyPrivacyModal
          payload={prepared}
          sourceLabel="Codex"
          busy={starting}
          onCancel={() => {
            if (!startPending.current) setPrepared(null);
          }}
          onConfirm={() => void confirm()}
        />
      )}
      {dashboard && (
        <EfficiencyDashboardModal
          sessionName={session.title || session.id}
          currentTurns={currentTurns}
          analysis={analysis}
          error=""
          onClose={() => setDashboard(false)}
          onReanalyse={() => {
            setDashboard(false);
            void requestAnalysis();
          }}
          onJumpToFinding={jumpToFinding}
        />
      )}
      {keyRequired && (
        <JevKeyRequiredModal
          onCancel={() => setKeyRequired(false)}
          onOpenSettings={() => {
            setKeyRequired(false);
            openSettings();
          }}
        />
      )}
      {error && <OperationResultModal kind="error" message={error} onClose={() => setError("")} />}
    </div>
  );
}
