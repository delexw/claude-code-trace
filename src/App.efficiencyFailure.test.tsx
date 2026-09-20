import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";

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

const FAILED_JOB = {
  analysisId: "a1",
  sessionId: "one",
  sessionPath: "/s/one.jsonl",
  sessionName: "Fix login",
  status: "failed",
  progress: null,
  message: "Efficiency analysis failed",
  error: "Jev did not respond within 60s",
  updatedAt: "2026-09-20T00:00:09Z",
};

function setup(priorJobs: unknown[] = []) {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_settings":
        return Promise.resolve(SETTINGS);
      case "get_project_dirs":
        return Promise.resolve(["/p"]);
      case "discover_sessions":
        return Promise.resolve([SESSION]);
      case "list_efficiency_analysis_jobs":
        return Promise.resolve(priorJobs);
      case "list_efficiency_summaries":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
}

describe("a failed Jev analysis", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    emit = null;
  });

  it("tells the user why, naming the session", async () => {
    setup();
    render(<App />);
    await waitFor(() => expect(emit).not.toBeNull());

    await act(async () => {
      emit?.(FAILED_JOB);
    });

    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Action failed");
    expect(dialog).toHaveTextContent("Fix login");
    expect(dialog).toHaveTextContent("Jev did not respond within 60s");
  });

  it("falls back to the job's message when there is no error detail", async () => {
    setup();
    render(<App />);
    await waitFor(() => expect(emit).not.toBeNull());

    await act(async () => {
      emit?.({ ...FAILED_JOB, error: null });
    });

    expect(await screen.findByRole("alertdialog")).toHaveTextContent("Efficiency analysis failed");
  });

  it("can be dismissed", async () => {
    setup();
    render(<App />);
    await waitFor(() => expect(emit).not.toBeNull());
    await act(async () => {
      emit?.(FAILED_JOB);
    });

    await screen.findByRole("alertdialog");
    const dismiss = screen.getByRole("button", { name: /^OK$/ });
    await act(async () => {
      dismiss.click();
    });

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("stays quiet about failures that were already on the backend at launch", async () => {
    // Otherwise every start would re-announce an old failure the user has seen.
    setup([FAILED_JOB]);
    render(<App />);
    await waitFor(() => expect(emit).not.toBeNull());
    await act(async () => {});

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});
