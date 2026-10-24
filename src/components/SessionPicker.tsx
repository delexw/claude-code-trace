import { useRef, type ReactNode } from "react";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";

interface SessionPickerProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  /** Totals the enabled plugins show beside the title. */
  headerStats?: ReactNode;
  /**
   * Shared toolbar-action registry. The picker registers Top/Bottom so they
   * scroll its own list; without it the toolbar's scroll buttons are inert here.
   */
  viewActionsRef?: ViewActionsRef;
  /** One section per enabled plugin. */
  children: ReactNode;
}

export function SessionPicker({
  searchQuery,
  onSearchChange,
  headerStats,
  viewActionsRef,
  children,
}: SessionPickerProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // Register Top/Bottom against the picker's own scroll container. Falls back to
  // a local ref when no registry is supplied (e.g. in isolated tests).
  const fallbackActionsRef = useRef({});
  useRegisterViewActions(viewActionsRef ?? fallbackActionsRef, {
    scrollToTop: () => listRef.current?.scrollTo({ top: 0, behavior: "smooth" }),
    scrollToBottom: () =>
      listRef.current?.scrollTo({ top: listRef.current.scrollHeight, behavior: "smooth" }),
  });

  return (
    <div className="picker">
      <div className="picker__header">
        <div className="picker__title">
          Sessions
          {headerStats}
        </div>
        <input
          className="picker__search"
          type="text"
          placeholder="Search sessions..."
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>

      <div className="picker__list" ref={listRef}>
        {children}
      </div>
    </div>
  );
}
