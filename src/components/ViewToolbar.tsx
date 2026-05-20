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
  onOpenTeams: () => void;
  onOpenDebug: () => void;
  onBackToList: () => void;
  onOpenSettings: () => void;
}

function scrollContent(to: "top" | "bottom") {
  const el = document.querySelector(".main-content");
  if (el) el.scrollTo({ top: to === "top" ? 0 : el.scrollHeight, behavior: "smooth" });
}

function CommonButtons({
  onExpandAll,
  onCollapseAll,
}: {
  onExpandAll: () => void;
  onCollapseAll: () => void;
}) {
  return (
    <>
      <button className="view-toolbar__btn" onClick={onExpandAll}>
        Expand All
      </button>
      <button className="view-toolbar__btn" onClick={onCollapseAll}>
        Collapse All
      </button>
      <span className="view-toolbar__separator" />
      <button className="view-toolbar__btn" onClick={() => scrollContent("top")}>
        Top
      </button>
      <button className="view-toolbar__btn" onClick={() => scrollContent("bottom")}>
        Bottom
      </button>
    </>
  );
}

function RightButtons({ onOpenSettings }: { onOpenSettings: () => void }) {
  return (
    <>
      <span className="view-toolbar__spacer" />
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
        <CommonButtons onExpandAll={onExpandAll} onCollapseAll={onCollapseAll} />
        <span className="view-toolbar__separator" />
        {hasTeams && (
          <button className="view-toolbar__btn" onClick={onOpenTeams}>
            Teams
          </button>
        )}
        <button className="view-toolbar__btn" onClick={onOpenDebug}>
          Debug
        </button>
        <RightButtons onOpenSettings={onOpenSettings} />
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
        <CommonButtons onExpandAll={onExpandAll} onCollapseAll={onCollapseAll} />
        <RightButtons onOpenSettings={onOpenSettings} />
      </div>
    );
  }

  // detail, team, debug
  return (
    <div className="view-toolbar">
      <button className="view-toolbar__btn" onClick={onBackToList}>
        <BackIcon /> Back to Messages
      </button>
      <CommonButtons onExpandAll={onExpandAll} onCollapseAll={onCollapseAll} />
      <RightButtons onOpenSettings={onOpenSettings} />
    </div>
  );
}
