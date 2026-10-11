import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { SessionPicker } from "./SessionPicker";
import type { SessionInfo } from "../types";
import type { ViewActions } from "../hooks/useViewActions";

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
    recap: null,
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

/** A finished walk: the bar is hidden, as it is for every test that is not about it. */
const indexed = {
  files_read: 1,
  total_files: 1,
  bytes_read: 1,
  total_bytes: 1,
  done: true,
};

describe("SessionPicker", () => {
  it("starts analysis without opening the session card", () => {
    const session = makeSession();
    const onSelect = vi.fn();
    const onAnalyse = vi.fn();
    render(
      <SessionPicker
        index={indexed}
        sessions={[session]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={onSelect}
        onSearchChange={vi.fn()}
        onAnalyse={onAnalyse}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Analyse/i }));
    expect(onAnalyse).toHaveBeenCalledWith(session.path);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("shows only inline progress and a percentage while analysing", () => {
    const session = makeSession();
    render(
      <SessionPicker
        index={indexed}
        sessions={[session]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        onAnalyse={vi.fn()}
        efficiencyJobs={
          new Map([
            [
              session.path,
              {
                analysisId: "analysis-1",
                sessionId: session.session_id,
                sessionPath: session.path,
                sessionName: session.first_message,
                status: "analysing",
                progress: 60,
                message: "Analysing session behaviour",
                updatedAt: "2026-09-19T04:00:00.000Z",
              },
            ],
          ])
        }
      />,
    );

    expect(screen.getByRole("progressbar", { name: "Jev analysis progress" })).toHaveAttribute(
      "aria-valuenow",
      "60",
    );
    expect(screen.getByText("60%")).toBeInTheDocument();
    expect(screen.queryByText("Analysis running")).not.toBeInTheDocument();
    expect(screen.queryByText("Analysing session behaviour")).not.toBeInTheDocument();
  });

  it("opens cached efficiency in the dashboard without opening the session", () => {
    const session = makeSession();
    const onSelect = vi.fn();
    const onAnalyse = vi.fn();
    const onOpenEfficiencyDashboard = vi.fn();
    render(
      <SessionPicker
        index={indexed}
        sessions={[session]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={onSelect}
        onSearchChange={vi.fn()}
        onAnalyse={onAnalyse}
        onOpenEfficiencyDashboard={onOpenEfficiencyDashboard}
        efficiencySummaries={
          new Map([
            [
              session.path,
              { sessionPath: session.path, score: 82, analyzedAt: "now", stale: false },
            ],
          ])
        }
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Dashboard · 82" }));
    expect(onOpenEfficiencyDashboard).toHaveBeenCalledWith(session);
    expect(onSelect).not.toHaveBeenCalled();
    expect(onAnalyse).not.toHaveBeenCalled();
  });
  it("shows loading spinner when loading", () => {
    render(
      <SessionPicker
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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

  it("prefers the session name as title, with first_message as subtitle", () => {
    const sessions = [makeSession({ name: "valkey-admin-contribution" })];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    // Name is the primary title; first_message survives as the subtitle.
    const title = screen.getByText("valkey-admin-contribution");
    expect(title).toBeInTheDocument();
    // Named sessions get the accent-highlight modifier class.
    expect(title).toHaveClass("picker__session-preview--named");
    expect(screen.getByText(/Hello world/)).toBeInTheDocument();
  });

  it("falls back to first_message when no name is set", () => {
    const sessions = [makeSession({ name: null })];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/Hello world/)).toBeInTheDocument();
  });

  it("unnamed session: keeps first_message as the main line and adds the recap as the subtitle", () => {
    const sessions = [
      makeSession({ name: null, first_message: "just do it", recap: "Migrated X, decided Y" }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        recapPreview={true}
      />,
    );
    // Main line is left untouched (the first message); the recap is a second line below it.
    expect(screen.getByText(/just do it/)).toBeInTheDocument();
    expect(screen.getByText(/Migrated X, decided Y/)).toBeInTheDocument();
  });

  it("unnamed session: no recap subtitle when recapPreview is off", () => {
    const sessions = [
      makeSession({ name: null, first_message: "just do it", recap: "Migrated X" }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        recapPreview={false}
      />,
    );
    expect(screen.getByText(/just do it/)).toBeInTheDocument();
    expect(screen.queryByText(/Migrated X/)).not.toBeInTheDocument();
  });

  it("named session: keeps the name as title and shows the recap as the subtitle", () => {
    const sessions = [
      makeSession({
        name: "home-lab-4e",
        first_message: "bitte lies issue 13",
        recap: "VictoriaLogs migration planned",
      }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        recapPreview={true}
      />,
    );
    // Name stays the title (a `/rename` or derived name is never replaced)...
    expect(screen.getByText("home-lab-4e")).toHaveClass("picker__session-preview--named");
    // ...and the recap takes the subtitle line, replacing the first message.
    expect(screen.getByText(/VictoriaLogs migration planned/)).toBeInTheDocument();
    expect(screen.queryByText(/bitte lies issue 13/)).not.toBeInTheDocument();
  });

  it("named session: keeps the first_message subtitle when recapPreview is off", () => {
    const sessions = [
      makeSession({
        name: "home-lab-4e",
        first_message: "bitte lies issue 13",
        recap: "VictoriaLogs migration planned",
      }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        recapPreview={false}
      />,
    );
    expect(screen.getByText("home-lab-4e")).toBeInTheDocument();
    expect(screen.getByText(/bitte lies issue 13/)).toBeInTheDocument();
    expect(screen.queryByText(/VictoriaLogs/)).not.toBeInTheDocument();
  });

  it("shows a 'CWDs' line listing every distinct directory when a session roamed", () => {
    const sessions = [
      makeSession({
        dirs: [
          "/Users/me/repos/sso-server",
          "/Users/me/seo/elements-storefront",
          "/Users/me/seo/elements-backend",
        ],
        cwd: "/Users/me/seo/elements-backend",
      }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("CWDs:")).toBeInTheDocument();
    expect(
      screen.getByText(/sso-server, elements-storefront, elements-backend/),
    ).toBeInTheDocument();
    // The line carries its own styling hook so the dirs are accent-coloured.
    expect(screen.getByText("CWDs:").closest(".picker__session-dirs")).not.toBeNull();
  });

  it("hides the 'CWDs' line for a single-directory session", () => {
    const sessions = [makeSession({ dirs: ["/Users/me/repos/sso-server"] })];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.queryByText("CWDs:")).not.toBeInTheDocument();
  });

  it("shows a splice warning when the parser flags an integrity_warning", () => {
    const sessions = [
      makeSession({
        integrity_warning:
          "possible spliced transcript: entry a2 has an unknown parent (u1) right where the working directory/branch or timestamp order jumps",
      }),
    ];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.getByText("⚠ Possibly spliced:")).toBeInTheDocument();
    expect(
      screen.getByText("⚠ Possibly spliced:").closest(".picker__session-warning"),
    ).not.toBeNull();
  });

  it("hides the splice warning when integrity_warning is absent", () => {
    const sessions = [makeSession({ integrity_warning: null })];
    render(
      <SessionPicker
        index={indexed}
        sessions={sessions}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );
    expect(screen.queryByText("⚠ Possibly spliced:")).not.toBeInTheDocument();
  });

  it("shows active badge for ongoing sessions", () => {
    const sessions = [makeSession({ is_ongoing: true })];
    render(
      <SessionPicker
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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
        index={indexed}
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
          index={indexed}
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
            index={indexed}
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

  describe("toolbar scroll actions", () => {
    it("registers scrollToTop/scrollToBottom that scroll the session list", () => {
      const viewActionsRef = { current: {} as ViewActions };
      const sessions = [
        makeSession(),
        makeSession({ session_id: "s2", path: "/home/user/.claude/projects/proj/s2.jsonl" }),
      ];

      render(
        <SessionPicker
          index={indexed}
          sessions={sessions}
          loading={false}
          searchQuery=""
          selectedIndex={0}
          onSelect={vi.fn()}
          onSearchChange={vi.fn()}
          viewActionsRef={viewActionsRef}
        />,
      );

      const list = document.querySelector(".picker__list") as HTMLElement;
      // jsdom has no layout: fake a scrollable height so top != bottom.
      Object.defineProperty(list, "scrollHeight", { value: 5000, configurable: true });
      const scrollTo = vi.fn();
      list.scrollTo = scrollTo as unknown as typeof list.scrollTo;

      act(() => viewActionsRef.current.scrollToBottom?.());
      expect(scrollTo).toHaveBeenCalledWith({ top: 5000, behavior: "smooth" });

      act(() => viewActionsRef.current.scrollToTop?.());
      expect(scrollTo).toHaveBeenCalledWith({ top: 0, behavior: "smooth" });
    });
  });
});

describe("SessionPicker indexing progress", () => {
  it("does not say there are no sessions while the walk is still running", () => {
    render(
      <SessionPicker
        sessions={[]}
        index={{
          files_read: 120,
          total_files: 3375,
          bytes_read: 3_700_000,
          total_bytes: 10_000_000,
          done: false,
        }}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
      />,
    );

    // The bar itself lives in the strip along the bottom; all the picker does with the
    // progress is hold its tongue until the walk has actually finished.
    expect(screen.queryByText("No sessions found")).not.toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
});

describe("SessionPicker Antigravity section", () => {
  const folder = {
    uuid: "00000000-0000-0000-0000-000000000001",
    path: "/home/user/.gemini/antigravity-ide/brain/00000000-0000-0000-0000-000000000001",
    mod_time_ms: Date.parse("2026-09-30T12:00:00Z"),
    title: "Build a Rust trading bot",
    flavor: "antigravity-ide",
    has_transcript: true,
    provider_id: "antigravity" as const,
  };

  it("hides the Antigravity section when no folders are provided", () => {
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[]}
      />,
    );
    expect(screen.queryByText(/Antigravity/i)).not.toBeInTheDocument();
  });

  it("treats a null antigravityFolders prop as empty (no crash)", () => {
    // Defensive against older call sites that lazily hydrate the list.
    expect(() =>
      render(
        <SessionPicker
          index={indexed}
          sessions={[]}
          loading={false}
          searchQuery=""
          selectedIndex={0}
          onSelect={vi.fn()}
          onSearchChange={vi.fn()}
          antigravityFolders={null}
        />,
      ),
    ).not.toThrow();
  });

  it("renders the section header and folder card when folders are provided", () => {
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={vi.fn()}
        antigravityEnabled
      />,
    );
    expect(screen.getAllByText(/Antigravity/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText(folder.title!).length).toBeGreaterThan(0);
    expect(screen.getByText(/read-only/i)).toBeInTheDocument();
  });

  it("invokes onSelectAntigravity when the card is clicked", () => {
    const onSelectAntigravity = vi.fn();
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={onSelectAntigravity}
        antigravityEnabled
      />,
    );
    fireEvent.click(screen.getAllByRole("button", { name: new RegExp(folder.title!) })[0]);
    expect(onSelectAntigravity).toHaveBeenCalledTimes(1);
    expect(onSelectAntigravity).toHaveBeenCalledWith(folder);
  });

  it("does not invoke onSelect when an Antigravity card is clicked", () => {
    // The Antigravity section is read-only — clicking must not open the
    // standard session flow.
    const onSelect = vi.fn();
    const onSelectAntigravity = vi.fn();
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={onSelect}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={onSelectAntigravity}
        antigravityEnabled
      />,
    );
    fireEvent.click(screen.getAllByRole("button", { name: new RegExp(folder.title!) })[0]);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("shows the short flavor label on each card", () => {
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={vi.fn()}
        antigravityEnabled
      />,
    );
    expect(screen.getAllByText(/^ide$/).length).toBeGreaterThan(0);
  });

  it("marks artifact-only folders instead of showing them as openable conversations", () => {
    const artifactsOnly = {
      ...folder,
      uuid: "00000000-0000-0000-0000-000000000009",
      path: "/home/user/.gemini/antigravity-ide/brain/00000000-0000-0000-0000-000000000009",
      has_transcript: false,
      flavor: "antigravity",
    };
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[artifactsOnly]}
        onSelectAntigravity={vi.fn()}
        antigravityEnabled
      />,
    );
    expect(screen.getByText(/artifacts only/i)).toBeInTheDocument();
    expect(screen.getByText(/^antigravity$/)).toBeInTheDocument();
  });

  it("renders the folder's modification time", () => {
    // Regression guard: passing epoch milliseconds through formatExactTime
    // yields an Invalid Date and silently renders nothing.
    render(
      <SessionPicker
        index={indexed}
        sessions={[]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={vi.fn()}
        antigravityEnabled
      />,
    );
    const stamp = document.querySelector(".picker__session-time");
    expect(stamp?.textContent?.trim()).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  });
});

const session = (over: Partial<SessionInfo> = {}): SessionInfo =>
  ({
    path: "/home/u/.claude/projects/a/s1.jsonl",
    name: "Claude session one",
    first_message: "hello",
    session_id: "s1",
    model: "claude-opus-5",
    cwd: "/home/u",
    total_tokens: 100,
    cost_usd: 0,
    turn_count: 2,
    duration_ms: 1000,
    mod_time: "2026-09-30T10:00:00Z",
    ...over,
  }) as SessionInfo;

describe("SessionPicker browse mode", () => {
  const folder = {
    uuid: "00000000-0000-0000-0000-000000000001",
    path: "/home/user/.gemini/antigravity-ide/brain/00000000-0000-0000-0000-000000000001",
    mod_time_ms: Date.parse("2026-09-30T12:00:00Z"),
    title: "An Antigravity conversation",
    flavor: "antigravity-ide",
    has_transcript: true,
    provider_id: "antigravity" as const,
  };

  const renderPicker = (props: Partial<React.ComponentProps<typeof SessionPicker>> = {}) =>
    render(
      <SessionPicker
        index={indexed}
        sessions={[session()]}
        loading={false}
        searchQuery=""
        selectedIndex={0}
        onSelect={vi.fn()}
        onSearchChange={vi.fn()}
        antigravityFolders={[folder]}
        onSelectAntigravity={vi.fn()}
        antigravityEnabled
        browseMode="all"
        {...props}
      />,
    );

  it("hides the mode selector entirely while Antigravity is disabled", () => {
    renderPicker({ antigravityEnabled: false });
    expect(screen.queryByRole("group", { name: /browse mode/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Antigravity" })).not.toBeInTheDocument();
  });

  it("offers all three modes once Antigravity is enabled", () => {
    renderPicker();
    const group = screen.getByRole("group", { name: /browse mode/i });
    expect(group).toBeInTheDocument();
    for (const label of ["Claude", "Antigravity", "All"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("reports a mode change", () => {
    const onBrowseModeChange = vi.fn();
    renderPicker({ onBrowseModeChange });
    fireEvent.click(screen.getByRole("button", { name: "Antigravity" }));
    expect(onBrowseModeChange).toHaveBeenCalledWith("antigravity");
  });

  it("browseMode=antigravity hides the Claude list and keeps the conversations", () => {
    renderPicker({ browseMode: "antigravity" });
    expect(screen.queryByText("Claude session one")).not.toBeInTheDocument();
    expect(screen.getAllByText(/An Antigravity conversation/).length).toBeGreaterThan(0);
  });

  it("browseMode=claude hides the Antigravity section", () => {
    renderPicker({ browseMode: "claude" });
    expect(screen.getByText("Claude session one")).toBeInTheDocument();
    expect(screen.queryByText("An Antigravity conversation")).not.toBeInTheDocument();
    // The selector stays visible — the user needs a way back.
    expect(screen.getByRole("group", { name: /browse mode/i })).toBeInTheDocument();
  });

  it("clamps a persisted mode that is no longer selectable back to Claude", () => {
    // Antigravity was turned off in Settings while the mode was still
    // "antigravity" in localStorage — the picker must not render the
    // (unavailable) Antigravity view.
    renderPicker({ browseMode: "antigravity", antigravityEnabled: false });
    expect(screen.getByText("Claude session one")).toBeInTheDocument();
  });

  it("gives Antigravity cards indices that continue past the Claude list", () => {
    // Two Claude sessions, so the first Antigravity card is index 2 and the
    // second is index 3 — one keyboard selection spans both lists.
    const onSelectIndex = vi.fn();
    renderPicker({
      sessions: [session(), session({ path: "/p/s2.jsonl", session_id: "s2", name: "Claude two" })],
      onSelectIndex,
    });
    const cards = document.querySelectorAll(".picker__session--antigravity");
    expect(cards).toHaveLength(1);
    fireEvent.mouseEnter(cards[0]);
    expect(onSelectIndex).toHaveBeenCalledWith(2);
  });

  it("shows an explanation when no conversations are found", () => {
    renderPicker({ antigravityFolders: [] });
    expect(screen.getAllByText(/No conversations found/i).length).toBeGreaterThan(0);
  });
});
