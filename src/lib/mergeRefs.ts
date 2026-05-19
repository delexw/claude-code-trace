import type { Ref, MutableRefObject } from "react";

/** Combine multiple refs into a single callback ref. */
export function mergeRefs<T>(...refs: (Ref<T> | null | undefined)[]): (el: T | null) => void {
  return (el) => {
    for (const ref of refs) {
      if (!ref) continue;
      if (typeof ref === "function") {
        ref(el);
      } else {
        (ref as MutableRefObject<T | null>).current = el;
      }
    }
  };
}
