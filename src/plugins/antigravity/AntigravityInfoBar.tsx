import type { AntigravityBrainFolder } from "./types";

/** The open conversation's title, id and step count, in the shared info-bar style. */
export function AntigravityInfoBar({
  folder,
  steps,
}: {
  folder: AntigravityBrainFolder;
  steps: number;
}) {
  return (
    <div className="info-bar">
      <span className="info-bar__project">{folder.title ?? folder.uuid}</span>
      <span className="info-bar__session-id">{folder.uuid}</span>
      <span className="info-bar__tokens">{steps} steps</span>
    </div>
  );
}
