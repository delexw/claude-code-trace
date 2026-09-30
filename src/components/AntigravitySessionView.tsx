import { useCallback, useState } from "react";
import type { AntigravityMessage, AntigravitySession } from "../types";
import { AntigravityMessageItem } from "./AntigravityMessageItem";
import { BackButton } from "./BackButton";

interface AntigravitySessionViewProps {
  session: AntigravitySession;
  onBack: () => void;
  onOpenFolder: () => void;
}

/**
 * Read-only viewer for an Antigravity conversation. Mirrors the Claude
 * MessageList shape (header / content per step, click to select, click
 * again to expand) but skips the virtualization — real Antigravity
 * folders cap at a few hundred transcript steps so a plain scroll
 * container is plenty and keeps the view simpler than the Claude one.
 */
export function AntigravitySessionView({
  session,
  onBack,
  onOpenFolder,
}: AntigravitySessionViewProps) {
  const { folder, messages } = session;
  const [selectedIndex, setSelectedIndex] = useState<number>(-1);
  const [expandedSet, setExpandedSet] = useState<Set<number>>(() => new Set([0]));

  const onSelect = useCallback((index: number) => setSelectedIndex(index), []);

  const onToggle = useCallback((index: number) => {
    setExpandedSet((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  const handleClick = useCallback(
    (index: number) => {
      if (selectedIndex === index) onToggle(index);
      else onSelect(index);
    },
    [selectedIndex, onToggle, onSelect],
  );

  const onExpandAll = useCallback(
    () => setExpandedSet(new Set(messages.map((_, i) => i))),
    [messages],
  );
  const onCollapseAll = useCallback(() => setExpandedSet(new Set()), []);

  if (messages.length === 0) {
    return (
      <div className="message-list message-list--antigravity-empty">
        <div className="message-list__topbar">
          <BackButton onClick={onBack} />
          <h2 className="message-list__title">{folder.title ?? folder.uuid}</h2>
          <button type="button" className="message-list__open-folder" onClick={onOpenFolder}>
            Open folder
          </button>
        </div>
        <div className="message-list__empty">
          No transcript found at <code>{folder.path}/.system_generated/logs/transcript.jsonl</code>
        </div>
      </div>
    );
  }

  return (
    <div className="message-list message-list--antigravity">
      <div className="message-list__topbar">
        <BackButton onClick={onBack} />
        <h2 className="message-list__title" title={folder.path}>
          {folder.title ?? folder.uuid}
        </h2>
        <span className="message-list__count">{messages.length} steps</span>
        <button type="button" onClick={onExpandAll} className="message-list__btn">
          Expand all
        </button>
        <button type="button" onClick={onCollapseAll} className="message-list__btn">
          Collapse all
        </button>
        <button type="button" className="message-list__open-folder" onClick={onOpenFolder}>
          Open folder
        </button>
      </div>
      <div className="message-list__scroll">
        {messages.map((msg: AntigravityMessage, index: number) => (
          <AntigravityMessageItem
            key={msg.step_index}
            message={msg}
            index={index}
            isSelected={index === selectedIndex}
            isExpanded={expandedSet.has(index)}
            onClick={handleClick}
          />
        ))}
      </div>
    </div>
  );
}
