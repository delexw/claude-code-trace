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
