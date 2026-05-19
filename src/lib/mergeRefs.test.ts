import { describe, it, expect, vi } from "vitest";
import { createRef } from "react";
import { mergeRefs } from "./mergeRefs";

describe("mergeRefs", () => {
  it("writes to a ref object and calls a callback ref with the same element", () => {
    const objectRef = createRef<HTMLDivElement>();
    const callbackRef = vi.fn();
    const el = document.createElement("div");

    mergeRefs(objectRef, callbackRef)(el);

    expect(objectRef.current).toBe(el);
    expect(callbackRef).toHaveBeenCalledWith(el);
  });

  it("passes null through to both refs on unmount", () => {
    const objectRef = createRef<HTMLDivElement>();
    const callbackRef = vi.fn();

    const merged = mergeRefs(objectRef, callbackRef);
    merged(document.createElement("div"));
    merged(null);

    expect(objectRef.current).toBeNull();
    expect(callbackRef).toHaveBeenLastCalledWith(null);
  });

  it("ignores nullish refs in the list", () => {
    const callbackRef = vi.fn();
    const el = document.createElement("div");

    mergeRefs(null, undefined, callbackRef)(el);

    expect(callbackRef).toHaveBeenCalledWith(el);
  });
});
