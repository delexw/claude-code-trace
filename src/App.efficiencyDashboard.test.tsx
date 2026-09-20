import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";

const mockInvoke = vi.fn();
vi.mock("./lib/invoke", async () => {
  const actual = await vi.importActual<typeof import("./lib/invoke")>("./lib/invoke");
  return { ...actual, invoke: (...args: unknown[]) => mockInvoke(...args) };
});
let emit: ((payload: unknown) => void) | null = null;
vi.mock("./lib/listen", () => ({
  listen: vi.fn((event: string, cb: (e: { payload: unknown }) => void) => {
    if (event === "efficiency-analysis-update") emit = (p) => cb({ payload: p });
    return Promise.resolve(() => {});
  }),
  reconnectSse: vi.fn(),
}));

import { App } from "./App";

const SETTINGS = {
  projects_dir: null,
  default_dir: "/d",
  effective_dir: "/d",
  effective_dir_exists: true,
  wsl_distros: [],
  allowed_origins: [],
  can_focus: false,
  api_auth_enabled: false,
  api_auth_source: "file",
  clients: [],
};
const SESSION = {
  path: "/s/one.jsonl",
  session_id: "one",
  name: "One",
  first_message: "hi",
  model: "opus",
  turn_count: 3,
  modified: "2026-09-20T00:00:00Z",
  ongoing: false,
  dirs: ["/p"],
  size: 10,
  tokens: 1,
  git_branch: "",
  subagent_count: 0,
  teams: [],
  cwd: "/p",
};
const ANALYSIS = {
  sessionId: "one",
  sessionPath: "/s/one.jsonl",
  score: 80,
  dimensions: {
    progress: 8,
    toolUse: 8,
    focus: 8,
    exploration: 8,
    recovery: 8,
    tokenUse: 8,
    thinking: 8,
  },
  metricEvaluations: [],
  findings: [],
  decisions: {},
  analyzedAt: "2026-09-20T00:00:00Z",
  analyzedTurns: 3,
  transcriptFingerprint: "f",
  analysisVersion: 7,
  decisionSetVersion: 4,
  scoreFormulaVersion: 2,
  stale: false,
};
const OLD_JOB = {
  analysisId: "a1",
  sessionId: "one",
  sessionPath: "/s/one.jsonl",
  sessionName: "One",
  status: "completed",
  progress: 100,
  score: 80,
  message: "Analysis complete",
  updatedAt: "2026-09-20T00:00:00Z",
};
const NEW_JOB = {
  analysisId: "a2",
  sessionId: "one",
  sessionPath: "/s/one.jsonl",
  sessionName: "One",
  status: "queued",
  progress: 0,
  message: "Queued",
  updatedAt: "2026-09-20T00:00:01Z",
};

function setup(opts: { priorJob: boolean; priorSummary: boolean }) {
  const jobList: unknown[] = opts.priorJob ? [OLD_JOB] : [];
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_settings":
        return Promise.resolve(SETTINGS);
      case "get_project_dirs":
        return Promise.resolve(["/p"]);
      case "discover_sessions":
        return Promise.resolve([SESSION]);
      case "list_efficiency_analysis_jobs":
        return Promise.resolve([...jobList]);
      case "list_efficiency_summaries":
        return Promise.resolve(
          opts.priorSummary
            ? [
                {
                  sessionPath: "/s/one.jsonl",
                  score: 80,
                  analyzedAt: ANALYSIS.analyzedAt,
                  stale: false,
                },
              ]
            : [],
        );
      case "get_analytics_settings":
        return Promise.resolve({
          jev: { configured: true, source: "secure-storage", status: "configured" },
          defaultPayloadMode: "minimized",
          recommendationProvider: { type: "codex-subscription" },
          subscriptionProvidersAvailable: true,
        });
      case "prepare_session_efficiency_payload":
        return Promise.resolve({
          sessionId: "one",
          sessionPath: "/s/one.jsonl",
          transcriptFingerprint: "f",
          payloadMode: "minimized",
          destination: {
            provider: "TypeSafe AI (Jev)",
            endpoint: "https://x",
            model: "jev-latest",
          },
          input: {
            task: { firstUserMessage: "hi", turns: 3, durationMs: 1, totalTokens: 1 },
            actions: [],
            signals: {},
            selectedExcerpts: [],
          },
        });
      case "start_session_efficiency_analysis":
        jobList.length = 0;
        jobList.push(NEW_JOB);
        return Promise.resolve(NEW_JOB);
      case "get_session_efficiency":
        return Promise.resolve(ANALYSIS);
      case "load_session":
        return Promise.resolve({
          messages: [],
          count: 0,
          teams: [],
          ongoing: false,
          meta: { session_id: "one", cwd: "/p", model: "opus", git_branch: "" },
          session_totals: { total_tokens: 1 },
        });
      case "get_session_meta":
        return Promise.resolve({ session_id: "one", cwd: "/p", model: "opus", git_branch: "" });
      case "message_roles":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
}

async function runFlow(label: RegExp) {
  render(<App />);
  const btn = await screen.findByRole("button", { name: label }, { timeout: 4000 });
  await act(async () => {
    fireEvent.click(btn);
  });
  const cb = await screen.findByRole("checkbox");
  await act(async () => {
    fireEvent.click(cb);
  });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
  });
  await waitFor(() =>
    expect(mockInvoke).toHaveBeenCalledWith("start_session_efficiency_analysis", expect.anything()),
  );
  await act(async () => {
    emit?.({
      ...NEW_JOB,
      status: "completed",
      progress: 100,
      score: 81,
      updatedAt: "2026-09-20T00:00:09Z",
    });
  });
}

describe("picker: first analyse vs re-analyse", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    emit = null;
  });

  it("FIRST analyse opens the dashboard", async () => {
    setup({ priorJob: false, priorSummary: false });
    await runFlow(/^Analyse/i);
    await waitFor(
      () =>
        expect(mockInvoke).toHaveBeenCalledWith("get_session_efficiency", { path: "/s/one.jsonl" }),
      { timeout: 3000 },
    );
  });

  it("RE-analyse opens the dashboard", async () => {
    setup({ priorJob: true, priorSummary: true });
    await runFlow(/Re-analyse/i);
    await waitFor(
      () =>
        expect(mockInvoke).toHaveBeenCalledWith("get_session_efficiency", { path: "/s/one.jsonl" }),
      { timeout: 3000 },
    );
  });

  it("RE-analyse from inside the open dashboard re-opens it when done", async () => {
    setup({ priorJob: true, priorSummary: true });
    render(<App />);

    // Open the dashboard from the picker score button.
    const dash = await screen.findByRole("button", { name: /Dashboard/i }, { timeout: 4000 });
    await act(async () => {
      fireEvent.click(dash);
    });
    await screen.findByText(/SESSION EFFICIENCY/i, {}, { timeout: 4000 });

    // Re-analyse from inside the modal: it closes, then should come back.
    // Two exist: the picker row's and the modal's. Take the modal's (rendered last).
    const all = screen.getAllByRole("button", { name: /Re-analyse/i });
    await act(async () => {
      fireEvent.click(all[all.length - 1]);
    });

    const cb = await screen.findByRole("checkbox", {}, { timeout: 4000 });
    await act(async () => {
      fireEvent.click(cb);
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
    });
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        "start_session_efficiency_analysis",
        expect.anything(),
      ),
    );

    mockInvoke.mockClear();
    await act(async () => {
      emit?.({
        ...NEW_JOB,
        status: "completed",
        progress: 100,
        score: 81,
        updatedAt: "2026-09-20T00:00:09Z",
      });
    });

    await waitFor(
      () =>
        expect(mockInvoke).toHaveBeenCalledWith("get_session_efficiency", { path: "/s/one.jsonl" }),
      { timeout: 3000 },
    );
    await screen.findByText(/SESSION EFFICIENCY/i, {}, { timeout: 4000 });
  });

  it("re-analyse from the session view also opens the dashboard", async () => {
    // Regression: these two buttons omitted the open-dashboard flag, so analysing
    // from the session view never popped the dashboard while the picker's did.
    setup({ priorJob: true, priorSummary: true });
    render(<App />);

    // Enter the session from the picker.
    const row = await screen.findByText("One", {}, { timeout: 4000 });
    await act(async () => {
      fireEvent.click(row);
    });

    const reanalyse = await screen.findByRole("button", { name: /Re-analyse/i }, { timeout: 4000 });
    await act(async () => {
      fireEvent.click(reanalyse);
    });
    const cb = await screen.findByRole("checkbox", {}, { timeout: 4000 });
    await act(async () => {
      fireEvent.click(cb);
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Continue/i }));
    });
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        "start_session_efficiency_analysis",
        expect.anything(),
      ),
    );

    mockInvoke.mockClear();
    await act(async () => {
      emit?.({
        ...NEW_JOB,
        status: "completed",
        progress: 100,
        score: 81,
        updatedAt: "2026-09-20T00:00:09Z",
      });
    });
    await waitFor(
      () =>
        expect(mockInvoke).toHaveBeenCalledWith("get_session_efficiency", { path: "/s/one.jsonl" }),
      { timeout: 3000 },
    );
  });
});
