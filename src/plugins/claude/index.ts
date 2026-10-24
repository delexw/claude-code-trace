import type { SourcePlugin } from "../types";
import { ClaudeSettingsPanel } from "./ClaudeSettingsPanel";
import { useClaudeSource } from "./useClaudeSource";

export const claudePlugin: SourcePlugin = {
  id: "claude",
  label: "Claude Code",
  canDisable: false,
  useSource: useClaudeSource,
  SettingsPanel: ClaudeSettingsPanel,
};
