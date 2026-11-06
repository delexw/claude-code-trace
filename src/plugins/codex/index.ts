import type { SourcePlugin } from "../types";
import { CodexSettingsPanel } from "./CodexSettingsPanel";
import { useCodexSource } from "./useCodexSource";

export const codexPlugin: SourcePlugin = {
  id: "codex",
  label: "Codex",
  canDisable: true,
  useSource: useCodexSource,
  SettingsPanel: CodexSettingsPanel,
};
