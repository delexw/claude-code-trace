import type { ViewState } from "../types";
import { BackIcon } from "./Icons";
import { IoMdSettings } from "react-icons/io";
import { isTauri } from "../lib/isTauri";
import { invoke } from "../lib/invoke";

interface ViewToolbarProps {
  view: ViewState;
  hasTeams: boolean;
  hasSession: boolean;
  onGoToSessions: () => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
  onOpenTeams: () => void;
  onOpenDebug: () => void;
  onBackToList: () => void;
  onOpenSettings: () => void;
}

/**
 * Content-scoped actions — they act on the main content column (the session list
 * or the message list), so they live in the RIGHT cluster, above that column,
 * not on the left above the PROJECTS sidebar. Expand/Collapse only apply where
 * there are collapsible entries (`collapsible`); Top/Bottom always apply.
 */
function ContentActions({
  collapsible,
  onExpandAll,
  onCollapseAll,
  onScrollToTop,
  onScrollToBottom,
}: {
  collapsible: boolean;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
}) {
  return (
    <>
      {collapsible && (
        <>
          <button className="view-toolbar__btn" onClick={onExpandAll}>
            Expand All
          </button>
          <button className="view-toolbar__btn" onClick={onCollapseAll}>
            Collapse All
          </button>
          <span className="view-toolbar__separator" />
        </>
      )}
      <button className="view-toolbar__btn" onClick={onScrollToTop}>
        Top
      </button>
      <button className="view-toolbar__btn" onClick={onScrollToBottom}>
        Bottom
      </button>
    </>
  );
}

/** Right cluster: spacer pushes it to the right edge, then content actions,
 * then the app-level Open-in-Browser / Settings. */
function RightCluster({
  collapsible,
  onExpandAll,
  onCollapseAll,
  onScrollToTop,
  onScrollToBottom,
  onOpenSettings,
}: {
  collapsible: boolean;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <>
      <span className="view-toolbar__spacer" />
      <ContentActions
        collapsible={collapsible}
        onExpandAll={onExpandAll}
        onCollapseAll={onCollapseAll}
        onScrollToTop={onScrollToTop}
        onScrollToBottom={onScrollToBottom}
      />
      <span className="view-toolbar__separator" />
      {isTauri && (
        <button
          className="view-toolbar__btn"
          onClick={async () => {
            try {
              await invoke("switch_to_browser");
            } catch {}
          }}
          title="Open in browser and hide this window"
        >
          Open in Browser
        </button>
      )}
      <button className="view-toolbar__btn" onClick={onOpenSettings} title="Settings">
        <IoMdSettings />
      </button>
    </>
  );
}

export function ViewToolbar({
  view,
  hasTeams,
  hasSession,
  onGoToSessions,
  onExpandAll,
  onCollapseAll,
  onScrollToTop,
  onScrollToBottom,
  onOpenTeams,
  onOpenDebug,
  onBackToList,
  onOpenSettings,
}: ViewToolbarProps) {
  const right = (collapsible: boolean) => (
    <RightCluster
      collapsible={collapsible}
      onExpandAll={onExpandAll}
      onCollapseAll={onCollapseAll}
      onScrollToTop={onScrollToTop}
      onScrollToBottom={onScrollToBottom}
      onOpenSettings={onOpenSettings}
    />
  );

  if (view === "list") {
    return (
      <div className="view-toolbar">
        <button className="view-toolbar__btn" onClick={onGoToSessions}>
          <BackIcon /> Sessions
        </button>
        {hasTeams && (
          <button className="view-toolbar__btn" onClick={onOpenTeams}>
            Teams
          </button>
        )}
        <button className="view-toolbar__btn" onClick={onOpenDebug}>
          Debug
        </button>
        {right(true)}
      </div>
    );
  }

  if (view === "picker") {
    return (
      <div className="view-toolbar">
        {hasSession && (
          <button className="view-toolbar__btn" onClick={onBackToList}>
            <BackIcon /> Back to Messages
          </button>
        )}
        {/* Picker has no collapsible entries — only scroll actions apply. */}
        {right(false)}
      </div>
    );
  }

  // detail, team, debug
  return (
    <div className="view-toolbar">
      <button className="view-toolbar__btn" onClick={onBackToList}>
        <BackIcon /> Back to Messages
      </button>
      {right(true)}
    </div>
  );
}
