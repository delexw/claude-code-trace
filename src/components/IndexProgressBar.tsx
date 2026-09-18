import type { IndexProgress } from "../types";
import { formatBytes } from "../../shared/format";

interface IndexProgressBarProps {
  progress: IndexProgress;
}

/**
 * How much of the projects directory the backend has read.
 *
 * Reading every session file takes minutes on a large directory. The list fills in as
 * the walk goes, newest first, and this says how much is still to come so a half-filled
 * list does not read as the whole of it.
 *
 * Lives in the strip along the bottom, and keeps its box there whether a walk is running
 * or not: a container that came and went would shift everything above it each time.
 */
export function IndexProgressBar({ progress }: IndexProgressBarProps) {
  if (progress.done) return <div className="index-progress" />;

  // Measured in bytes, not files: one 22GB session and three thousand small ones would
  // otherwise show a bar at 99% with nearly all the reading still to do. The label says
  // bytes too, so the number and the bar cannot contradict each other.
  const { files_read: files, bytes_read: bytes, total_bytes: totalBytes } = progress;
  const percent = totalBytes > 0 ? Math.min(100, Math.round((bytes / totalBytes) * 100)) : 0;

  return (
    <div
      className="index-progress"
      role="progressbar"
      aria-label="Indexing sessions"
      aria-valuemin={0}
      aria-valuemax={totalBytes}
      aria-valuenow={bytes}
    >
      <div className="index-progress__track">
        <div className="index-progress__fill" style={{ width: `${percent}%` }} />
      </div>
      <span className="index-progress__count">
        {totalBytes > 0
          ? `${files.toLocaleString()} sessions · ${formatBytes(bytes)} / ${formatBytes(totalBytes)}`
          : "Counting sessions..."}
      </span>
    </div>
  );
}
