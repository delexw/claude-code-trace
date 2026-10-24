import type { SourcePlugin } from "../types";
import { AntigravitySettingsPanel } from "./AntigravitySettingsPanel";
import { useAntigravitySource } from "./useAntigravitySource";

export const antigravityPlugin: SourcePlugin = {
  id: "antigravity",
  label: "Antigravity",
  canDisable: true,
  useSource: useAntigravitySource,
  SettingsPanel: AntigravitySettingsPanel,
};
