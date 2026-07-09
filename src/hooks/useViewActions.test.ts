import { describe, it, expect, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import {
  useViewActionsRef,
  useViewActionCallbacks,
  useRegisterViewActions,
} from "./useViewActions";

describe("useViewActions", () => {
  it("callbacks delegate to registered handlers", () => {
    const expand = vi.fn();
    const collapse = vi.fn();

    const { result: refResult } = renderHook(() => useViewActionsRef());
    const { result: cbResult } = renderHook(() => useViewActionCallbacks(refResult.current));

    // Register handlers
    renderHook(() =>
      useRegisterViewActions(refResult.current, { expandAll: expand, collapseAll: collapse }),
    );

    act(() => cbResult.current.expandAll());
    act(() => cbResult.current.collapseAll());

    expect(expand).toHaveBeenCalledTimes(1);
    expect(collapse).toHaveBeenCalledTimes(1);
  });

  it("callbacks are no-ops when no handlers registered", () => {
    const { result: refResult } = renderHook(() => useViewActionsRef());
    const { result: cbResult } = renderHook(() => useViewActionCallbacks(refResult.current));

    // Should not throw
    act(() => cbResult.current.expandAll());
    act(() => cbResult.current.collapseAll());
  });

  it("unmount clears registration", () => {
    const expand = vi.fn();
    const { result: refResult } = renderHook(() => useViewActionsRef());

    const { unmount } = renderHook(() =>
      useRegisterViewActions(refResult.current, { expandAll: expand }),
    );

    unmount();
    expect(refResult.current.current.expandAll).toBeUndefined();
  });

  it("scroll callbacks delegate to registered handlers", () => {
    const top = vi.fn();
    const bottom = vi.fn();

    const { result: refResult } = renderHook(() => useViewActionsRef());
    const { result: cbResult } = renderHook(() => useViewActionCallbacks(refResult.current));

    renderHook(() =>
      useRegisterViewActions(refResult.current, { scrollToTop: top, scrollToBottom: bottom }),
    );

    act(() => cbResult.current.scrollToTop());
    act(() => cbResult.current.scrollToBottom());

    expect(top).toHaveBeenCalledTimes(1);
    expect(bottom).toHaveBeenCalledTimes(1);
  });
});
