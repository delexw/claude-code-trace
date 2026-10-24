import { BackIcon } from "../../components/Icons";
import { BackToMessagesButton } from "../../components/BackToMessagesButton";
import type { ClaudeView } from "./keyHints";

interface ClaudeToolbarProps {
  view: ClaudeView;
  hasTeams: boolean;
  onGoToSessions: () => void;
  onOpenTeams: () => void;
  onOpenDebug: () => void;
  onBackToList: () => void;
}

/** Navigation buttons for Claude's views, left of the toolbar's shared right cluster. */
export function ClaudeToolbar({
  view,
  hasTeams,
  onGoToSessions,
  onOpenTeams,
  onOpenDebug,
  onBackToList,
}: ClaudeToolbarProps) {
  if (view !== "list") return <BackToMessagesButton onClick={onBackToList} />;
  return (
    <>
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
    </>
  );
}
