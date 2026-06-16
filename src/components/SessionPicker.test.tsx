import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { SessionPicker } from "./SessionPicker";
import type { SessionInfo } from "../types";

type IOCallback = (entries: IntersectionObserverEntry[], observer: IntersectionObserver) => void;

class FakeIO {
  static last: FakeIO | null = null;
  cb: IOCallback;
  observed: Element[] = [];
  observe = (el: Element) => {
    this.observed.push(el);
  };
  unobserve = vi.fn();
  disconnect = vi.fn();
  takeRecords = vi.fn(() => [] as IntersectionObserverEntry[]);
  root = null;
  rootMargin = "";
  thresholds: number[] = [];
  constructor(cb: IOCallback) {
    this.cb = cb;
    FakeIO.last = this;
  }
  trigger(els: Element[]) {
    const entries = els.map(
      (el) =>
        ({
          target: el,
          isIntersecting: true,
          intersectionRatio: 1,
          boundingClientRect: new DOMRect(),
          intersectionRect: new DOMRect(),
          rootBounds: new DOMRect(),
          time: 0,
        }) as unknown as IntersectionObserverEntry,
    );
    this.cb(entries, this as unknown as IntersectionObserver);
  }
}

function makeSession(overrides: Partial<SessionInfo> = {}): SessionInfo {
  return {
    path: "/home/user/.claude/projects/proj/session1.jsonl",
    session_id: "session1",
    mod_time: new Date().toISOString(),
    first_message: "Hello world",
    turn_count: 5,
    is_ongoing: false,
    total_tokens: 2000,
    input_tokens: 1000,
    output_tokens: 1000,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    cost_usd: 0.05,
    duration_ms: 30000,
    model: "claude-sonnet-4-20250514",
    cwd: "/home/user/proj",
    git_branch: "main",
    permission_mode: "default",
    ...overrides,
  };
}

describe("SessionPicker", () => {
  it("shows loading spinner when loading", () => {
    render(
      <SessionPicker
        sessions={[]}
        loading={true}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/Discovering sessions/)).toBeInTheDocument();
  });

  it("shows 'No sessions found' when empty and no search", () => {
    render(
      <SessionPicker
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("No sessions found")).toBeInTheDocument();
  });

  it("shows 'No matching sessions' when empty and searching", () => {
    render(
      <SessionPicker
        sessions={[]}
        loading={false}
        searchQuery="xyz"
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("No matching sessions")).toBeInTheDocument();
  });

  it("renders sessions grouped by date", () => {
    const sessions = [makeSession()];
    render(
      <SessionPicker
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    // Should show "Today" group header since mod_time is now
    expect(screen.getByText("Today")).toBeInTheDocument();
    expect(screen.getByText(/Hello world/)).toBeInTheDocument();
  });

  it("shows active badge for ongoing sessions", () => {
    const sessions = [makeSession({ is_ongoing: true })];
    render(
      <SessionPicker
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("ACTIVE")).toBeInTheDocument();
  });

  it("shows model, tokens, cost, duration, and time", () => {
    const sessions = [
      makeSession({
        model: "claude-sonnet-4-20250514",
        total_tokens: 5000,
        cost_usd: 1.23,
        duration_ms: 60000,
      }),
    ];
    render(
      <SessionPicker
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("sonnet4")).toBeInTheDocument();
    // Tokens appear in both header and session row
    expect(screen.getAllByText(/5\.0k/).length).toBeGreaterThanOrEqual(1);
    // Cost appears in both header and session row
    expect(screen.getAllByText("1.23")).toHaveLength(2);
    expect(screen.getByText("1m 0s")).toBeInTheDocument();
  });

  it("search input updates on change", () => {
    const onSearchChange = vi.fn();
    render(
      <SessionPicker
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={onSearchChange}
      />,
    );
    const input = screen.getByPlaceholderText("Search sessions...");
    fireEvent.change(input, { target: { value: "test" } });
    expect(onSearchChange).toHaveBeenCalledWith("test");
  });

  it("selected session is highlighted", () => {
    const sessions = [makeSession()];
    render(
      <SessionPicker
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    const sessionEl = screen.getByText(/Hello world/).closest(".picker__session")!;
    expect(sessionEl).toHaveClass("picker__session--selected");
  });

  it("clicking session calls onSelect", () => {
    const onSelect = vi.fn();
    const sessions = [makeSession()];
    render(
      <SessionPicker
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={onSelect}
        onSearchChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText(/Hello world/).closest(".picker__session")!);
    expect(onSelect).toHaveBeenCalledWith(sessions[0]);
  });

  it("does not show loading spinner when not loading", () => {
    render(
      <SessionPicker
        sessions={[makeSession()]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.queryByText(/Discovering sessions/)).not.toBeInTheDocument();
  });

  describe("viewport-aware visibility tracking", () => {
    beforeEach(() => {
      FakeIO.last = null;
      (globalThis as { IntersectionObserver: unknown }).IntersectionObserver = FakeIO;
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it("observes each session card and reports visible paths via onVisiblePathsChange", () => {
      const onVisible = vi.fn();
      const sessions = [
        makeSession({ path: "/a.jsonl", session_id: "a", first_message: "alpha" }),
        makeSession({ path: "/b.jsonl", session_id: "b", first_message: "beta" }),
      ];
      render(
        <SessionPicker
          sessions={sessions}
          loading={false}
          searchQuery=""
          selectedIndex={0}
          onSelect={vi.fn()}
          onSearchChange={vi.fn()}
          onVisiblePathsChange={onVisible}
        />,
      );
      expect(FakeIO.last).not.toBeNull();
      expect(FakeIO.last!.observed).toHaveLength(2);

      act(() => {
        FakeIO.last!.trigger(FakeIO.last!.observed);
        vi.advanceTimersByTime(150);
      });
      expect(onVisible).toHaveBeenCalledExactlyOnceWith(
        expect.arrayContaining(["/a.jsonl", "/b.jsonl"]),
      );
    });

    it("works without onVisiblePathsChange (no-op observer)", () => {
      const sessions = [makeSession()];
      expect(() =>
        render(
          <SessionPicker
            sessions={sessions}
            loading={false}
            searchQuery=""
            selectedIndex={0}
            onSelect={vi.fn()}
            onSearchChange={vi.fn()}
          />,
        ),
      ).not.toThrow();
    });
  });
});
