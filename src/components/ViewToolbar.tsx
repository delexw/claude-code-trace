import type { BrowseMode, ViewState } from "../types";
import { BackIcon } from "./Icons";
import { IoMdSettings } from "react-icons/io";
import { isTauri } from "../lib/isTauri";
import { invoke } from "../lib/invoke";
import { selectableModes } from "../lib/browseMode";

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
  /** The Claude / Antigravity / All selector. Omitted → not rendered. It only
   *  applies to the picker, so it is drawn on that view alone: in a transcript
   *  it would change something the reader cannot see. */
  browseMode?: BrowseMode;
  onBrowseModeChange?: (mode: BrowseMode) => void;
  /** Whether Antigravity is switched on at all — with it off, only "Claude" is
   *  offered rather than two modes that would show nothing. */
  antigravityEnabled?: boolean;
  /** How many steps the open conversation has. Drawn on the Antigravity view,
   *  whose viewer used to carry its own header with this and the title — the
   *  toolbar is where the Claude list puts the same controls. */
  stepsLabel?: string;
}

/** The browse-mode selector. Lives in the toolbar rather than inside the list
 *  header: it decides what the whole list *is*, so it belongs with the view,
 *  and it stays reachable without scrolling the list back to the top. */
function BrowseModeControl({
  antigravityEnabled,
  browseMode,
  onBrowseModeChange,
}: {
  antigravityEnabled: boolean;
  browseMode: BrowseMode;
  onBrowseModeChange?: (mode: BrowseMode) => void;
}) {
  const modes = selectableModes(antigravityEnabled);
  if (modes.length < 2) return null;
  return (
    <div className="picker__mode" role="group" aria-label="Browse mode">
      {modes.map((mode) => (
        <button
          key={mode.id}
          type="button"
          className={`picker__mode-btn${browseMode === mode.id ? " picker__mode-btn--active" : ""}`}
          aria-pressed={browseMode === mode.id}
          title={mode.title}
          onClick={() => onBrowseModeChange?.(mode.id)}
        >
          {mode.label}
        </button>
      ))}
    </div>
  );
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
  stat,
  onExpandAll,
  onCollapseAll,
  onScrollToTop,
  onScrollToBottom,
  onOpenSettings,
}: {
  collapsible: boolean;
  /** A count about the content on screen, drawn ahead of the actions. */
  stat?: string;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <>
      <span className="view-toolbar__spacer" />
      {stat && <span className="view-toolbar__stat">{stat}</span>}
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
  browseMode,
  onBrowseModeChange,
  antigravityEnabled = false,
  stepsLabel,
}: ViewToolbarProps) {
  const right = (collapsible: boolean, stat?: string) => (
    <RightCluster
      collapsible={collapsible}
      stat={stat}
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
        {browseMode && (
          <BrowseModeControl
            antigravityEnabled={antigravityEnabled}
            browseMode={browseMode}
            onBrowseModeChange={onBrowseModeChange}
          />
        )}
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

  // The Antigravity viewer is read-only and has no selection to expand — it
  // needs its way out, and `backToList` is a no-op without a Claude session
  // path, so send it through `onGoToSessions` like the list view does.
  if (view === "antigravity") {
    return (
      <div className="view-toolbar">
        <button className="view-toolbar__btn" onClick={onGoToSessions}>
          <BackIcon /> Sessions
        </button>
        {/* Navigation on the left, counts and actions on the right — the same
            split the Claude list view uses. */}
        {right(true, stepsLabel)}
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
