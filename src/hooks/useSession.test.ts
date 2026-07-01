import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useSession } from "./useSession";
import type { DisplayMessage, LoadResult } from "../types";

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
    return Promise.resolve(() => set?.delete(cb));
  },
}));

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

function makeMessage(index: number): DisplayMessage {
  return {
    role: index % 2 === 0 ? "user" : "claude",
    model: "",
    content: `message ${index}`,
    timestamp: "2025-01-01T12:00:00Z",
    thinking_count: 0,
    tool_call_count: 0,
    output_count: 0,
    tokens_raw: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    context_tokens: 0,
    duration_ms: 0,
    items: [],
    last_output: null,
    is_error: false,
    teammate_spawns: 0,
    teammate_messages: 0,
    subagent_label: "",
  };
}

const TOTAL = 250;

/** Build a load_session response for [start, start+limit) against a TOTAL-message session. */
function windowResult(start: number, limit: number | undefined): LoadResult {
  const count = TOTAL;
  const end = limit === undefined ? count : Math.min(start + limit, count);
  const clampedStart = Math.min(start, count);
  const messages: DisplayMessage[] = [];
  for (let i = clampedStart; i < end; i++) messages.push(makeMessage(i));
  return {
    messages,
    teams: [],
    ongoing: false,
    meta: { cwd: "/proj", git_branch: "main", permission_mode: "default" },
    session_totals: {
      total_tokens: 0,
      input_tokens: 0,
      output_tokens: 0,
      cache_read_tokens: 0,
      cache_creation_tokens: 0,
      cost_usd: 0,
      model: "",
    },
    count,
    start: clampedStart,
    roles: Array.from({ length: count }, (_, i) => (i % 2 === 0 ? "user" : "claude")),
    context_tokens: 0,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  mockInvoke.mockImplementation(
    (cmd: string, args?: { start?: number; limit?: number; index?: number }) => {
      if (cmd === "load_session") {
        return Promise.resolve(windowResult(args?.start ?? 0, args?.limit));
      }
      if (cmd === "load_message") {
        return Promise.resolve(makeMessage(args?.index ?? 0));
      }
      return Promise.resolve(undefined);
    },
  );
});

describe("useSession windowing", () => {
  it("loads count/roles and primes only the tail window", async () => {
    const { result } = renderHook(() => useSession());

    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });

    expect(result.current.count).toBe(TOTAL);
    expect(result.current.roles).toHaveLength(TOTAL);
    // Tail is loaded, the head is not (memory stays windowed).
    await waitFor(() => expect(result.current.getMessage(TOTAL - 1)).toBeDefined());
    expect(result.current.getMessage(0)).toBeUndefined();
  });

  it("fetches missing pages on ensureRange and evicts far ones", async () => {
    const { result } = renderHook(() => useSession());
    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });
    // Tail (page 2, indices 200-249) is loaded after prime.
    await waitFor(() => expect(result.current.getMessage(240)).toBeDefined());

    // Scroll to the top pages.
    await act(async () => {
      await result.current.ensureRange(0, 100);
    });

    expect(result.current.getMessage(0)).toBeDefined();
    // The far tail page is evicted to keep memory bounded.
    await waitFor(() => expect(result.current.getMessage(240)).toBeUndefined());
  });

  it("evicts far pages even when the requested range needs no new fetch", async () => {
    const { result } = renderHook(() => useSession());
    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });
    await waitFor(() => expect(result.current.getMessage(240)).toBeDefined());

    // Scroll to the top: evicts the tail page, loads page 0.
    await act(async () => {
      await result.current.ensureRange(0, 50);
    });
    expect(result.current.getMessage(240)).toBeUndefined();

    // Scroll back down near the tail: re-loads the tail page. Page 0 survives
    // because it's within this range's wider keep band.
    await act(async () => {
      await result.current.ensureRange(150, 250);
    });
    expect(result.current.getMessage(240)).toBeDefined();
    expect(result.current.getMessage(20)).toBeDefined();

    // Move back to a narrow range at the top. Page 0 is already cached (no
    // fetch needed this time), but the tail page is now two pages outside the
    // keep band — it must still be evicted even though nothing was fetched.
    await act(async () => {
      await result.current.ensureRange(0, 50);
    });
    expect(result.current.getMessage(240)).toBeUndefined();
  });

  it("clearWindow drops all loaded pages so the next range re-fetches", async () => {
    const { result } = renderHook(() => useSession());
    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });
    await waitFor(() => expect(result.current.getMessage(240)).toBeDefined());

    act(() => {
      result.current.clearWindow();
    });
    expect(result.current.getMessage(240)).toBeUndefined();

    mockInvoke.mockClear();
    await act(async () => {
      await result.current.ensureRange(200, 250);
    });
    // Re-fetched even though this page was already loaded before clearing.
    expect(mockInvoke).toHaveBeenCalledWith("load_session", {
      path: "/proj/s.jsonl",
      start: 200,
      limit: 100,
    });
    expect(result.current.getMessage(240)).toBeDefined();
  });

  it("loadFullMessage fetches the full message body on demand", async () => {
    const { result } = renderHook(() => useSession());
    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });

    let full: DisplayMessage | undefined;
    await act(async () => {
      full = await result.current.loadFullMessage(42);
    });
    expect(full).toBeDefined();
    expect(full?.content).toBe("message 42");
    expect(mockInvoke).toHaveBeenCalledWith("load_message", { path: "/proj/s.jsonl", index: 42 });
  });

  it("applies the live update signal (count/context) and keeps roles in sync", async () => {
    const { result } = renderHook(() => useSession());
    await act(async () => {
      await result.current.loadSession("/proj/s.jsonl");
    });

    act(() => {
      emit("session-update", {
        count: 260,
        roles: Array.from({ length: 260 }, () => "user"),
        context_tokens: 12345,
        teams: [],
        ongoing: true,
        permission_mode: "acceptEdits",
        session_totals: windowResult(0, 0).session_totals,
      });
    });

    expect(result.current.count).toBe(260);
    expect(result.current.roles).toHaveLength(260);
    expect(result.current.contextTokens).toBe(12345);
    expect(result.current.ongoing).toBe(true);
    expect(result.current.meta.permission_mode).toBe("acceptEdits");
  });
});
