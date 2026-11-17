import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { CodexInfoBar } from "./CodexInfoBar";
import { makeSession, makeWindow } from "./testFixtures";

describe("CodexInfoBar cost", () => {
  it("uses the open conversation's latest estimated cost", () => {
    render(
      <CodexInfoBar
        session={makeSession({ cost_usd: 1 })}
        window={makeWindow({ cost_usd: 2.5 })}
      />,
    );
    const cost = screen.getByLabelText("Estimated API token cost");
    expect(cost).toHaveTextContent("$2.50");
    expect(cost).toHaveClass("info-bar__cost");
    expect(cost).toHaveAttribute("title", expect.stringContaining("Subscription"));
  });

  it("displays a known zero without treating it as unavailable", () => {
    render(<CodexInfoBar session={makeSession()} window={makeWindow({ cost_usd: 0 })} />);
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$0.00");
    expect(screen.queryByText("Cost unavailable")).toBeNull();
  });

  it("keeps known spend visible when worker costs are incomplete", () => {
    render(
      <CodexInfoBar
        session={makeSession()}
        window={makeWindow({ cost_usd: 2.5, cost_partial: true })}
      />,
    );
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$2.50+");
    expect(screen.getByLabelText("Estimated API token cost")).toHaveAttribute(
      "title",
      expect.stringContaining("Known subtotal"),
    );
  });

  it("keeps unknown costs distinct from zero dollars", () => {
    render(<CodexInfoBar session={makeSession()} window={makeWindow({ cost_usd: null })} />);
    expect(screen.getByLabelText("API token cost unavailable")).toHaveTextContent(
      "Cost unavailable",
    );
    expect(screen.queryByText(/\$0\.00/)).toBeNull();
  });

  it("shows recorded metadata and uses latest context rather than cumulative tokens", () => {
    render(
      <CodexInfoBar
        session={makeSession({ git_branch: "sample-branch" })}
        window={makeWindow({
          model: "fictional-codex",
          permission_mode: "on-request",
          sandbox_policy: "workspace-write",
          ongoing: true,
          context_tokens: 25000,
          context_window: 100000,
          total_tokens: 1000000,
          duration_ms: 5000,
        })}
      />,
    );
    expect(screen.getByText("sample-branch")).toBeInTheDocument();
    expect(screen.getByText("fictional-codex")).toBeInTheDocument();
    expect(screen.getByText("ctx 25%")).toBeInTheDocument();
    expect(screen.getByText("active")).toBeInTheDocument();
    expect(screen.getByTitle("Codex approval policy")).toHaveTextContent("on-request");
    expect(screen.getByTitle("Recorded active turn time")).toHaveTextContent("5.0s");
    expect(screen.getByRole("button", { name: /resume/i })).toBeInTheDocument();
  });

  it("hides unknown context and activity without inventing a context capacity", () => {
    const rendered = render(
      <CodexInfoBar
        session={makeSession()}
        window={makeWindow({ context_tokens: null, context_window: null, duration_ms: null })}
      />,
    );
    expect(screen.queryByText(/ctx /)).toBeNull();
    expect(screen.queryByText("active")).toBeNull();
    expect(screen.queryByTitle("Recorded active turn time")).toBeNull();
    rendered.rerender(
      <CodexInfoBar
        session={makeSession()}
        window={makeWindow({ context_tokens: 500, context_window: null })}
      />,
    );
    expect(screen.getByText("ctx 500")).toBeInTheDocument();
  });

  it("shows current Git branch and dirty state when the working folder is accessible", () => {
    render(
      <CodexInfoBar
        session={makeSession({ git_branch: "old-invented-branch" })}
        window={makeWindow()}
        gitInfo={{ branch: "current-invented-branch", dirty: true, worktree_dirs: [] }}
      />,
    );
    expect(screen.getByTitle("Current Git branch")).toHaveTextContent("current-invented-branch");
    expect(screen.getByTitle("Current Git branch")).toHaveClass("info-bar__branch--dirty");
    expect(screen.queryByText("old-invented-branch")).toBeNull();
  });
});
