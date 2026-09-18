import { describe, it, expect, vi } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { usePicker } from "./usePicker";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Set<Listener>>();
vi.mock("../lib/listen", () => ({
  listen: (event: string, cb: Listener) => {
    let set = listeners.get(event);
    if (!set) {
      set = new Set();
      listeners.set(event, set);
    }
    set.add(cb);
    return Promise.resolve(() => {
      set?.delete(cb);
    });
  },
}));

function emit(event: string, payload: unknown) {
  const set = listeners.get(event);
  if (!set) return;
  for (const cb of set) cb({ payload });
}

const session = (path: string, ongoing: boolean) => ({
  path,
  session_id: path,
  mod_time: "2026-01-01T00:00:00Z",
  first_message: "test",
  turn_count: 1,
  is_ongoing: ongoing,
  total_tokens: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  cost_usd: 0,
  duration_ms: 0,
  model: "opus",
  cwd: "/test",
  git_branch: "main",
  permission_mode: "default",
});

describe("usePicker", () => {
  it("updateSessionOngoing sets is_ongoing for matching session", async () => {
    const sessions = [session("/a.jsonl", true), session("/b.jsonl", false)];
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") return Promise.resolve(sessions);
      return Promise.resolve();
    });

    const { result } = renderHook(() => usePicker());

    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });

    expect(result.current.allSessions[0].is_ongoing).toBe(true);

    act(() => {
      result.current.updateSessionOngoing("/a.jsonl", false);
    });

    expect(result.current.allSessions[0].is_ongoing).toBe(false);
  });

  it("updateSessionOngoing is a no-op for unknown path", async () => {
    const sessions = [session("/a.jsonl", true)];
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") return Promise.resolve(sessions);
      return Promise.resolve();
    });

    const { result } = renderHook(() => usePicker());

    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });

    act(() => {
      result.current.updateSessionOngoing("/nonexistent.jsonl", false);
    });

    // Original session unchanged
    expect(result.current.allSessions[0].is_ongoing).toBe(true);
  });

  it("picker-refresh signal re-fetches sessions via discover_sessions", async () => {
    const initial = [session("/a.jsonl", true)];
    const refreshed = [session("/a.jsonl", true), session("/b.jsonl", false)];
    let call = 0;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") {
        call += 1;
        return Promise.resolve(call === 1 ? initial : refreshed);
      }
      return Promise.resolve();
    });

    const { result } = renderHook(() => usePicker());

    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });
    expect(result.current.allSessions).toHaveLength(1);

    // Backend now broadcasts an empty signal — the hook must re-fetch
    // rather than treating the payload as the session list.
    await act(async () => {
      emit("picker-refresh", {});
    });

    await waitFor(() => {
      expect(result.current.allSessions).toHaveLength(2);
    });
  });

  it("selecting a project includes a forked session grouped under it", async () => {
    // Issue #238: a forked session's own cwd is a brand-new, unrelated worktree, so
    // selecting the parent's project must still surface it via forked_from_session_id.
    const parent = {
      ...session("/home/user/.claude/projects/-Users-me-repos-my-app/parent.jsonl", false),
      session_id: "parent",
    };
    const forked = {
      ...session("/home/user/.claude/projects/-Users-me-worktrees-abc123/forked.jsonl", false),
      session_id: "forked",
      forked_from_session_id: "parent",
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") return Promise.resolve([parent, forked]);
      return Promise.resolve();
    });

    const { result } = renderHook(() => usePicker("-Users-me-repos-my-app"));

    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });

    expect(result.current.sessions.map((s) => s.session_id).toSorted()).toEqual([
      "forked",
      "parent",
    ]);
  });

  it("updateSessionOngoing skips update if value unchanged", async () => {
    const sessions = [session("/a.jsonl", true)];
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") return Promise.resolve(sessions);
      return Promise.resolve();
    });

    const { result } = renderHook(() => usePicker());

    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });

    const before = result.current.allSessions;

    act(() => {
      result.current.updateSessionOngoing("/a.jsonl", true); // same value
    });

    // Should return same reference (no state update)
    expect(result.current.allSessions).toBe(before);
  });
});

const progress = (over: Record<string, unknown> = {}) => ({
  files_read: 120,
  total_files: 3375,
  bytes_read: 500,
  total_bytes: 9000,
  done: false,
  ...over,
});

describe("indexing progress", () => {
  async function pickerWith(sessions: unknown[]) {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "discover_sessions") return Promise.resolve(sessions);
      if (cmd === "get_project_dirs") return Promise.resolve(["/projects"]);
      return Promise.resolve();
    });
    const { result } = renderHook(() => usePicker());
    await act(async () => {
      await result.current.discoverSessions(["/projects"]);
    });
    return result;
  }

  it("starts with the bar hidden, before any walk has reported", async () => {
    const result = await pickerWith([]);
    expect(result.current.index.done).toBe(true);
  });

  it("follows the walk without asking for the list again", async () => {
    const result = await pickerWith([session("/a.jsonl", false)]);
    const before = mockInvoke.mock.calls.length;

    act(() => emit("index-progress", progress()));

    expect(result.current.index).toEqual(progress());
    // The walk sends its own picker-refresh once it has read more sessions; a fetch per
    // progress tick would put every visible session on the wire four times a second.
    expect(mockInvoke.mock.calls).toHaveLength(before);
  });

  it("follows a big file being read even while the file count stands still", async () => {
    const result = await pickerWith([]);

    act(() => emit("index-progress", progress({ bytes_read: 500 })));
    act(() => emit("index-progress", progress({ bytes_read: 4500 })));

    // One 22GB session reports its bytes as it is read. Ignoring those would freeze the
    // bar for as long as that file takes.
    expect(result.current.index.bytes_read).toBe(4500);
  });

  it("holds the same state object when a tick repeats itself", async () => {
    const result = await pickerWith([]);

    act(() => emit("index-progress", progress()));
    const first = result.current.index;
    act(() => emit("index-progress", progress()));

    expect(result.current.index).toBe(first);
  });

  it("hides the bar once the walk says it is done", async () => {
    const result = await pickerWith([]);

    act(() => emit("index-progress", progress()));
    act(() => emit("index-progress", progress({ done: true })));

    expect(result.current.index.done).toBe(true);
  });
});
