import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  EfficiencyAnalysisJob,
  PreparedEfficiencyPayload,
  SessionEfficiencyAnalysis,
} from "../../types";
import type { CodexSession } from "./types";
import { CodexEfficiency } from "./CodexEfficiency";

const mockInvoke = vi.fn();
vi.mock("../../lib/invoke", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
let emit: ((job: EfficiencyAnalysisJob) => void) | undefined;
vi.mock("../../lib/listen", () => ({
  listen: (_event: string, callback: (event: { payload: EfficiencyAnalysisJob }) => void) => {
    emit = (payload) => callback({ payload });
    return Promise.resolve(() => {});
  },
}));

const session = {
  id: "00000000-0000-0000-0000-000000000123",
  title: "Build a fabricated toy calculator",
  cwd: "/work/calculator",
  path: "/fabricated/codex/sessions/rollout.jsonl",
} as CodexSession;
const prepared: PreparedEfficiencyPayload = {
  sessionId: session.id,
  sessionPath: session.path,
  transcriptFingerprint: "fabricated-fingerprint",
  payloadMode: "minimized",
  destination: {
    provider: "Jev",
    endpoint: "https://example.invalid/analysis",
    model: "toy-model",
  },
  input: {
    task: {
      firstUserMessage: "Build a toy calculator",
      turns: 3,
      durationMs: 100,
      totalTokens: 20,
    },
    actions: [],
    signals: {
      toolCalls: 0,
      failedToolCalls: 0,
      repeatedToolCalls: 0,
      subagentCount: 0,
      contextGrowth: 0,
      thinkingBlocks: 0,
      thinkingChars: 0,
    },
    selectedExcerpts: ["user: Build a toy calculator"],
  },
};
const finding = {
  type: "repeated-work" as const,
  probability: 0.9,
  startMessageIndex: 1,
  endMessageIndex: 2,
};
const analysis: SessionEfficiencyAnalysis = {
  sessionId: session.id,
  sessionPath: session.path,
  score: 82,
  dimensions: {
    progress: 80,
    toolUse: 70,
    focus: 60,
    exploration: 50,
    recovery: 40,
    tokenUse: 30,
    thinking: 30,
  },
  metricEvaluations: [],
  findings: [finding],
  decisions: {
    progressingEfficiently: 0.8,
    toolCallsUseful: 0.7,
    redundantWorkPresent: 0.4,
    excessiveExploration: 0.5,
    likelyThrashing: 0.4,
    effectiveRecovery: 0.4,
    tokenUsageEfficient: 0.3,
    thinkingBalance: 2,
    subagentsUseful: 0.5,
    likelyTaskCompleted: 0.8,
  },
  analyzedAt: "2026-01-01T00:00:00Z",
  analyzedTurns: 3,
  transcriptFingerprint: "fabricated-fingerprint",
  analysisVersion: 1,
  decisionSetVersion: 1,
  scoreFormulaVersion: 1,
  stale: false,
};
const job: EfficiencyAnalysisJob = {
  analysisId: "fabricated-job",
  sessionId: session.id,
  sessionPath: session.path,
  sessionName: session.title,
  status: "analysing",
  message: "Analysing fabricated payload",
  updatedAt: "2026-01-01T00:00:00Z",
};
let configured: boolean;
let cached: SessionEfficiencyAnalysis | null;
let backendJobs: EfficiencyAnalysisJob[];

beforeEach(() => {
  mockInvoke.mockReset();
  emit = undefined;
  configured = true;
  cached = null;
  backendJobs = [];
  mockInvoke.mockImplementation(async (command: string) => {
    switch (command) {
      case "get_analytics_settings":
        return { jev: { configured }, defaultPayloadMode: "minimized" };
      case "prepare_codex_efficiency_payload":
        return prepared;
      case "get_session_efficiency":
        return cached;
      case "list_efficiency_analysis_jobs":
        return backendJobs;
      case "list_efficiency_summaries":
        return [];
      case "start_session_efficiency_analysis":
        backendJobs = [job];
        return job;
      case "cancel_efficiency_analysis":
        return undefined;
      default:
        throw new Error(`Unexpected command: ${command}`);
    }
  });
});

function setup(value = session) {
  const onJumpToFinding = vi.fn();
  const openSettings = vi.fn();
  const result = render(
    <CodexEfficiency
      session={value}
      currentTurns={3}
      onJumpToFinding={onJumpToFinding}
      openSettings={openSettings}
    />,
  );
  return { ...result, onJumpToFinding, openSettings };
}

async function openPrivacy() {
  fireEvent.click(await screen.findByRole("button", { name: /Analyse efficiency/i }));
  await screen.findByRole("heading", { name: "Privacy Notice" });
}

function confirm() {
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "Continue & Analyse" }));
}

describe("CodexEfficiency", () => {
  it("prepares locally, previews the exact Codex payload, and requires consent before sending", async () => {
    setup();
    await openPrivacy();
    expect(mockInvoke).toHaveBeenCalledWith("prepare_codex_efficiency_payload", {
      path: session.path,
      payloadMode: "minimized",
    });
    expect(screen.getByText(/selected data from this Codex session/i)).toBeInTheDocument();
    expect(screen.getByText("https://example.invalid/analysis")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue & Analyse" })).toBeDisabled();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "start_session_efficiency_analysis"),
    ).toBe(false);
    confirm();
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("start_session_efficiency_analysis", {
        path: session.path,
        payload: prepared,
      }),
    );
    expect(await screen.findByText("Analysing fabricated payload")).toBeInTheDocument();
  });

  it("cancels the preview without sending and requires fresh consent on the next request", async () => {
    setup();
    await openPrivacy();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("heading", { name: "Privacy Notice" })).toBeNull();
    await openPrivacy();
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "start_session_efficiency_analysis"),
    ).toBe(false);
  });

  it("offers Analytics settings when no key is configured", async () => {
    configured = false;
    const { openSettings } = setup();
    fireEvent.click(await screen.findByRole("button", { name: /Analyse efficiency/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Open Analytics Settings" }));
    expect(openSettings).toHaveBeenCalledOnce();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "prepare_codex_efficiency_payload"),
    ).toBe(false);
  });

  it("displays cached findings and marks them stale when the session grows", async () => {
    cached = analysis;
    const result = setup();
    fireEvent.click(await screen.findByRole("button", { name: /Repeated work/i }));
    expect(result.onJumpToFinding).toHaveBeenCalledWith(finding);
    result.rerender(
      <CodexEfficiency
        session={session}
        currentTurns={4}
        onJumpToFinding={result.onJumpToFinding}
        openSettings={result.openSettings}
      />,
    );
    expect(screen.getByText(/Session has changed since this analysis/i)).toBeInTheDocument();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "start_session_efficiency_analysis"),
    ).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Re-analyse" }));
    await screen.findByRole("heading", { name: "Privacy Notice" });
    expect(screen.getByRole("checkbox")).not.toBeChecked();
  });

  it("shows backend preparation errors", async () => {
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === "get_analytics_settings") throw new Error("Fabricated settings failure");
      return command === "get_session_efficiency" ? null : [];
    });
    setup();
    fireEvent.click(await screen.findByRole("button", { name: /Analyse efficiency/i }));
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("Fabricated settings failure");
  });

  it("keeps the consent preview available if starting analysis fails", async () => {
    const implementation = mockInvoke.getMockImplementation()!;
    mockInvoke.mockImplementation((command: string) =>
      command === "start_session_efficiency_analysis"
        ? Promise.reject(new Error("Fabricated start failure"))
        : implementation(command),
    );
    setup();
    await openPrivacy();
    confirm();
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("Fabricated start failure");
    expect(screen.getByRole("heading", { name: "Privacy Notice" })).toBeInTheDocument();
  });

  it("can cancel an active analysis", async () => {
    backendJobs = [job];
    setup();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_efficiency_analysis", {
        analysisId: job.analysisId,
      }),
    );
    expect(await screen.findByRole("button", { name: /Analyse efficiency/i })).toBeEnabled();
  });

  it("opens the completed dashboard and jumps to a finding", async () => {
    const { onJumpToFinding } = setup();
    await openPrivacy();
    confirm();
    await screen.findByText("Analysing fabricated payload");
    cached = analysis;
    await act(async () =>
      emit?.({ ...job, status: "completed", score: 82, updatedAt: "2026-01-01T00:00:01Z" }),
    );
    const dashboard = await screen.findByRole("dialog", { name: "Efficiency Dashboard" });
    fireEvent.click(await within(dashboard).findByRole("button", { name: /Repeated work/i }));
    expect(onJumpToFinding).toHaveBeenCalledWith(finding);
    expect(screen.queryByRole("dialog", { name: "Efficiency Dashboard" })).toBeNull();
  });

  it("shows live failures only for this chat", async () => {
    setup();
    await waitFor(() => expect(emit).toBeDefined());
    await act(async () =>
      emit?.({
        ...job,
        sessionPath: "/another/fabricated.jsonl",
        status: "failed",
        error: "Other chat failed",
      }),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await act(async () =>
      emit?.({ ...job, status: "failed", error: "Fabricated provider failure" }),
    );
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("Fabricated provider failure");
  });

  it("discards an in-flight prepared payload when switching chats", async () => {
    let resolvePrepare!: (payload: PreparedEfficiencyPayload) => void;
    const pending = new Promise<PreparedEfficiencyPayload>((resolve) => {
      resolvePrepare = resolve;
    });
    const implementation = mockInvoke.getMockImplementation()!;
    mockInvoke.mockImplementation((command: string) =>
      command === "prepare_codex_efficiency_payload" ? pending : implementation(command),
    );
    const result = setup();
    fireEvent.click(await screen.findByRole("button", { name: /Analyse efficiency/i }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        "prepare_codex_efficiency_payload",
        expect.anything(),
      ),
    );
    result.rerender(
      <CodexEfficiency
        session={{ ...session, id: "other", path: "/fabricated/codex/sessions/other.jsonl" }}
        currentTurns={3}
        onJumpToFinding={result.onJumpToFinding}
        openSettings={result.openSettings}
      />,
    );
    await act(async () => resolvePrepare(prepared));
    expect(screen.queryByRole("heading", { name: "Privacy Notice" })).toBeNull();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "start_session_efficiency_analysis"),
    ).toBe(false);
  });

  it("discards an old consent preview immediately when switching chats", async () => {
    const result = setup();
    await openPrivacy();
    fireEvent.click(screen.getByRole("checkbox"));
    await act(async () => {
      result.rerender(
        <CodexEfficiency
          session={{ ...session, path: "/fabricated/codex/sessions/other.jsonl" }}
          currentTurns={3}
          onJumpToFinding={result.onJumpToFinding}
          openSettings={result.openSettings}
        />,
      );
    });
    expect(screen.queryByRole("heading", { name: "Privacy Notice" })).toBeNull();
    expect(
      mockInvoke.mock.calls.some(([command]) => command === "start_session_efficiency_analysis"),
    ).toBe(false);
  });
});
