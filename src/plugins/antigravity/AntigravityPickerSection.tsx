import { useScrollToSelected } from "../../hooks/useScrollToSelected";
import { ForwardIcon } from "../../components/Icons";
import { formatMsTimestamp, truncate } from "../../lib/format";
import { AntigravityIcon } from "./roleStyles";
import type { AntigravityBrainFolder } from "./types";

/**
 * The conversations the picker lists: folders that hold a transcript (the rest
 * are artifact folders), matched against the search text. The picker counts
 * and draws this same list, so a keyboard selection always lands on a card.
 */
export function visibleConversations(
  folders: AntigravityBrainFolder[],
  searchQuery: string,
): AntigravityBrainFolder[] {
  const query = searchQuery.trim().toLowerCase();
  return folders.filter(
    (folder) =>
      folder.has_transcript &&
      (!query ||
        (folder.title?.toLowerCase().includes(query) ?? false) ||
        folder.uuid.toLowerCase().includes(query) ||
        folder.flavor.toLowerCase().includes(query)),
  );
}

interface AntigravityPickerSectionProps {
  conversations: AntigravityBrainFolder[];
  loading: boolean;
  searchQuery: string;
  selectedIndex: number | null;
  onHover: (index: number) => void;
  onOpen: (folder: AntigravityBrainFolder) => void;
}

export function AntigravityPickerSection({
  conversations,
  loading,
  searchQuery,
  selectedIndex,
  onHover,
  onOpen,
}: AntigravityPickerSectionProps) {
  const selectedRef = useScrollToSelected(selectedIndex ?? -1);

  return (
    <div className="picker__section antigravity-section">
      <div className="picker__section-header">
        <span className="antigravity-section__badge">Antigravity</span>
        <span className="picker__section-count">{conversations.length}</span>
        <span className="picker__section-hint">read-only</span>
      </div>
      {loading && conversations.length === 0 && (
        <div className="picker__loading">
          <span className="braille-spinner" />
          Reading conversations...
        </div>
      )}
      {!loading && conversations.length === 0 && (
        <div className="picker__empty">
          {searchQuery ? "No matching conversations" : "No conversations found"}
        </div>
      )}
      {conversations.map((folder, index) => {
        const isSelected = index === selectedIndex;
        return (
          <div
            key={folder.path}
            ref={isSelected ? selectedRef : undefined}
            className={`picker__session antigravity-section__card${isSelected ? " picker__session--selected" : ""}`}
            title={folder.path}
            onMouseEnter={() => onHover(index)}
            onClick={() => onOpen(folder)}
          >
            <div className="picker__session-top">
              <span className="picker__session-icon">
                <AntigravityIcon />
              </span>
              <span className="picker__session-preview">
                {folder.title ? truncate(folder.title, 80) : "Untitled conversation"}
              </span>
              <button
                className="message__detail-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  onOpen(folder);
                }}
              >
                Detail <ForwardIcon />
              </button>
            </div>
            <div className="picker__session-meta antigravity-section__meta">
              {folder.mod_time_ms > 0 && (
                <span className="picker__session-time">
                  {formatMsTimestamp(folder.mod_time_ms)}
                </span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
