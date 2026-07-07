import { useRef, useEffect, useCallback, type MutableRefObject } from "react";

/** Actions that any view can register for the toolbar to call. */
export interface ViewActions {
  expandAll?: () => void;
  collapseAll?: () => void;
  scrollToTop?: () => void;
  scrollToBottom?: () => void;
}

export type ViewActionsRef = MutableRefObject<ViewActions>;

/** Create a shared ref for view action registration. */
export function useViewActionsRef(): ViewActionsRef {
  return useRef<ViewActions>({});
}

/**
 * Register expand/collapse handlers from within a view component.
 * Clears registration on unmount so stale handlers are never called.
 */
export function useRegisterViewActions(ref: ViewActionsRef, actions: ViewActions) {
  useEffect(() => {
    ref.current = actions;
    return () => {
      ref.current = {};
    };
  });
}

/** Build stable callbacks that delegate to whatever the ref currently holds. */
export function useViewActionCallbacks(ref: ViewActionsRef) {
  const expandAll = useCallback(() => ref.current.expandAll?.(), [ref]);
  const collapseAll = useCallback(() => ref.current.collapseAll?.(), [ref]);
  const scrollToTop = useCallback(() => ref.current.scrollToTop?.(), [ref]);
  const scrollToBottom = useCallback(() => ref.current.scrollToBottom?.(), [ref]);
  return { expandAll, collapseAll, scrollToTop, scrollToBottom };
}
