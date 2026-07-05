import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { InfoBar } from "./InfoBar";
import type { SessionMeta, SessionTotals, GitInfo } from "../types";

function makeMeta(overrides: Partial<SessionMeta> = {}): SessionMeta {
  return {
    cwd: "/home/user/my-project",
    git_branch: "main",
    permission_mode: "default",
    ...overrides,
  };
}

function makeTotals(overrides: Partial<SessionTotals> = {}): SessionTotals {
  return {
    total_tokens: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    cost_usd: 0,
    model: "claude-sonnet-4-20250514",
    ...overrides,
  };
}

describe("InfoBar", () => {
  it("shows project name from shortPath(meta.cwd)", () => {
    render(
      <InfoBar
        meta={makeMeta({ cwd: "/home/user/my-project" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("my-project")).toBeInTheDocument();
  });

  it("shows session ID from sessionPath", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath="/some/path/abc123.jsonl"
        ongoing={false}
      />,
    );
    expect(screen.getByText("abc123")).toBeInTheDocument();
  });

  it("shows branch name", () => {
    const gitInfo: GitInfo = { branch: "feature-x", dirty: false, worktree_dirs: [] };
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={gitInfo}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("feature-x")).toBeInTheDocument();
  });

  it("adds dirty class when gitInfo.dirty is true", () => {
    const gitInfo: GitInfo = { branch: "main", dirty: true, worktree_dirs: [] };
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={gitInfo}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    const branchEl = screen.getByText("main");
    expect(branchEl).toHaveClass("info-bar__branch--dirty");
  });

  it("does not add dirty class when gitInfo.dirty is false", () => {
    const gitInfo: GitInfo = { branch: "main", dirty: false, worktree_dirs: [] };
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={gitInfo}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    const branchEl = screen.getByText("main");
    expect(branchEl).not.toHaveClass("info-bar__branch--dirty");
  });

  it("shows permission mode pill for bypassPermissions", () => {
    render(
      <InfoBar
        meta={makeMeta({ permission_mode: "bypassPermissions" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("yolo")).toBeInTheDocument();
  });

  it("shows permission mode pill for acceptEdits", () => {
    render(
      <InfoBar
        meta={makeMeta({ permission_mode: "acceptEdits" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("auto-edit")).toBeInTheDocument();
  });

  it("shows permission mode pill for plan", () => {
    render(
      <InfoBar
        meta={makeMeta({ permission_mode: "plan" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("plan")).toBeInTheDocument();
  });

  it("does not show permission mode pill for default", () => {
    render(
      <InfoBar
        meta={makeMeta({ permission_mode: "default" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.queryByText("default")).not.toBeInTheDocument();
  });

  it("does not show permission mode pill for manual (v2.1.200+ default mode)", () => {
    render(
      <InfoBar
        meta={makeMeta({ permission_mode: "manual" })}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.queryByText("manual")).not.toBeInTheDocument();
  });

  it("shows context bar when contextTokens > 0", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={500_000}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("ctx 50%")).toBeInTheDocument();
  });

  it("does not show context bar when no context data", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.queryByText(/ctx/)).not.toBeInTheDocument();
  });

  it("shows tokens when total_tokens > 0", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals({ total_tokens: 5000 })}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText(/5\.0k tok/)).toBeInTheDocument();
  });

  it("shows cost when cost > 0", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals({ cost_usd: 1.5 })}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.getByText("1.50")).toBeInTheDocument();
  });

  it("shows active spinner when ongoing=true", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={true}
      />,
    );
    expect(screen.getByText(/active/)).toBeInTheDocument();
  });

  it("does not show active spinner when ongoing=false", () => {
    render(
      <InfoBar
        meta={makeMeta()}
        gitInfo={null}
        contextTokens={0}
        sessionTotals={makeTotals()}
        sessionPath=""
        ongoing={false}
      />,
    );
    expect(screen.queryByText(/active/)).not.toBeInTheDocument();
  });
});
