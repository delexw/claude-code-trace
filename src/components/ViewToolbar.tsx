import type { ViewState } from "../types";
import { BackIcon } from "./Icons";
import { IoMdSettings } from "react-icons/io";

interface ViewToolbarProps {
  view: ViewState;
  hasTeams: boolean;
  hasSession: boolean;
  messageCount: number;
  onGoToSessions: () => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onJumpTop: () => void;
  onJumpBottom: () => void;
  onOpenTeams: () => void;
  onOpenDebug: () => void;
  onBackToList: () => void;
  onOpenSettings: () => void;
}

function SettingsButton({ onClick }: { onClick: () => void }) {
  return (
    <>
      <span className="view-toolbar__spacer" />
      <button className="view-toolbar__btn" onClick={onClick} title="Settings">
        <IoMdSettings />
      </button>
    </>
  );
}

export function ViewToolbar({
  view,
  hasTeams,
  hasSession,
  messageCount,
  onGoToSessions,
  onExpandAll,
  onCollapseAll,
  onJumpTop,
  onJumpBottom,
  onOpenTeams,
  onOpenDebug,
  onBackToList,
  onOpenSettings,
}: ViewToolbarProps) {
  if (view === "list") {
    return (
      <div className="view-toolbar">
        <button className="view-toolbar__btn" onClick={onGoToSessions}>
          <BackIcon /> Sessions
        </button>
        <button className="view-toolbar__btn" onClick={onExpandAll}>
          Expand All
        </button>
        <button className="view-toolbar__btn" onClick={onCollapseAll}>
          Collapse All
        </button>
        <span className="view-toolbar__separator" />
        <button className="view-toolbar__btn" onClick={onJumpTop} disabled={messageCount === 0}>
          Top
        </button>
        <button className="view-toolbar__btn" onClick={onJumpBottom} disabled={messageCount === 0}>
          Bottom
        </button>
        <span className="view-toolbar__separator" />
        {hasTeams && (
          <button className="view-toolbar__btn" onClick={onOpenTeams}>
            Teams
          </button>
        )}
        <button className="view-toolbar__btn" onClick={onOpenDebug}>
          Debug
        </button>
        <SettingsButton onClick={onOpenSettings} />
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
        <SettingsButton onClick={onOpenSettings} />
      </div>
    );
  }

  // detail, team, debug views have their own back buttons
  return null;
}
