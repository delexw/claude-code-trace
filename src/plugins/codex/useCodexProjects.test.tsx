import { act, fireEvent, render, screen } from "@testing-library/react";
import { useLayoutEffect } from "react";
import { describe, expect, it, vi } from "vitest";
import { useCodexProjects } from "./useCodexProjects";
import { codexProjectKey } from "./projects";
import { makeSession } from "./testFixtures";
import type { CodexSession } from "./types";

describe("Codex project sidebar", () => {
  it("selects folders, preserves the filter during refresh, and resets with All Projects", () => {
    const parent = makeSession({ cwd: "/example/app" });
    const child = makeSession({ id: "child", cwd: "/example/app/tools" });
    const onSelect = vi.fn();
    const onRefresh = vi.fn();
    let latest!: ReturnType<typeof useCodexProjects>;
    function Harness({ sessions }: { sessions: CodexSession[] }) {
      const result = useCodexProjects({ sessions, loading: false, onSelect, onRefresh });
      useLayoutEffect(() => {
        latest = result;
      });
      return result.sidebar.render({ width: 180 });
    }
    const { rerender } = render(<Harness sessions={[parent, child]} />);
    expect(screen.getByText("Codex Projects")).toBeInTheDocument();
    fireEvent.click(screen.getByText("app"));
    expect(latest.selectedProject).toBe(codexProjectKey(parent));
    expect(latest.sidebar.focused).toBe(false);
    expect(onSelect).toHaveBeenCalledOnce();
    rerender(<Harness sessions={[parent, child, makeSession({ cwd: "/example/another" })]} />);
    expect(latest.selectedProject).toBe(codexProjectKey(parent));
    fireEvent.click(screen.getByRole("button", { name: "Refresh all projects" }));
    expect(onRefresh).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByText("All Projects"));
    expect(latest.selectedProject).toBeNull();
  });

  it("uses keyboard navigation and collapse without changing the selected filter", () => {
    const parent = makeSession({ cwd: "/example/app" });
    const child = makeSession({ id: "child", cwd: "/example/app/tools" });
    let latest!: ReturnType<typeof useCodexProjects>;
    function Harness() {
      const result = useCodexProjects({
        sessions: [parent, child],
        loading: false,
        onSelect: vi.fn(),
        onRefresh: vi.fn(),
      });
      useLayoutEffect(() => {
        latest = result;
      });
      return result.sidebar.render({});
    }
    render(<Harness />);
    act(() => latest.sidebar.focus());
    act(() => latest.sidebar.keyMap.j());
    act(() => latest.sidebar.keyMap.ArrowLeft());
    expect(screen.queryByText("tools")).not.toBeInTheDocument();
    expect(latest.selectedProject).toBeNull();
    act(() => latest.sidebar.keyMap.ArrowRight());
    expect(screen.getByText("tools")).toBeInTheDocument();
    act(() => latest.sidebar.keyMap.ArrowDown());
    act(() => latest.sidebar.keyMap.Enter());
    expect(latest.selectedProject).toBe(codexProjectKey(child));
    expect(latest.sidebar.focused).toBe(false);
  });
});
