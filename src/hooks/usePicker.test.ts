import { describe, it, expect, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { usePicker } from "./usePicker";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("../lib/listen", () => ({
  listen: () => Promise.resolve(() => {}),
}));

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
