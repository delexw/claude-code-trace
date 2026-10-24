import type { ComponentType, CSSProperties, ReactNode } from "react";
import type { ViewActionsRef } from "../hooks/useViewActions";
import type { SettingsResponse } from "../types";

export type KeyMap = Record<string, () => void>;

export interface KeyHint {
  key: string;
  label: string;
}

/** One screen a plugin can show in the main content column. */
export interface SourceView {
  render(): ReactNode;
  infoBar?: ReactNode;
  /** Buttons left of the toolbar's shared right cluster. */
  toolbarLeft: ReactNode;
  /** Whether the shared Expand All / Collapse All buttons apply. */
  collapsible: boolean;
  keyMap: KeyMap;
  hints: KeyHint[];
  /** Keybind-bar hints that are clickable, keyed by hint label. */
  hintActions?: KeyMap;
}

export interface PickerSectionRenderArgs {
  /** Index within this section, or null when the selection is in another section. */
  selectedIndex: number | null;
  onHover(localIndex: number): void;
}

/** A plugin's block of items in the shared session picker. */
export interface PickerSection {
  /** Keyboard-selectable items this section renders. */
  count: number;
  open(localIndex: number): void;
  render(args: PickerSectionRenderArgs): ReactNode;
  headerStats?: ReactNode;
}

export interface SourceSidebar {
  render(style: CSSProperties): ReactNode;
  focused: boolean;
  focus(): void;
  blur(): void;
  keyMap: KeyMap;
}

export interface SourceRuntime {
  picker: PickerSection;
  sidebar?: SourceSidebar;
  /** Shown at the left of the keybind bar. */
  status?: ReactNode;
  hasOpenSession: boolean;
  backToSession(): void;
  views: Record<string, SourceView>;
  /** Modals the plugin owns. */
  overlays?: ReactNode;
  /** The shell loaded settings at startup (or after an auth recovery); null when that failed. */
  onSettingsLoaded?(settings: SettingsResponse | null): void | Promise<void>;
  onSettingsSaved?(): void;
}

export type SettingsTab = "general" | "appearance" | "api" | "analytics";

/** What the shell gives each plugin's `useSource`. */
export interface SourceContext {
  enabled: boolean;
  /** The view this plugin currently shows, or null when the picker or another plugin is active. */
  activeView: string | null;
  openView(view: string): void;
  goToPicker(): void;
  /** Move the shared picker selection (an index across every section). */
  setPickerIndex(index: number): void;
  searchQuery: string;
  viewActionsRef: ViewActionsRef;
  fontScale: number;
  recapPreview: boolean;
  /** The bootstrap `get_settings` answer; null until it loads or when it failed. */
  settings: SettingsResponse | null;
  openSettings(tab?: SettingsTab): void;
}

export interface SettingsPanelHandle {
  save(): Promise<SettingsResponse | undefined>;
  reset?(): Promise<SettingsResponse | undefined>;
}

export interface SettingsPanelProps {
  settings: SettingsResponse | null;
  /** Call when the user edits a field, so a stale error is cleared. */
  onEdited(): void;
  /** Call on Enter, to save the whole modal. */
  onSubmit(): void;
  register(handle: SettingsPanelHandle | null): void;
}

export interface SourcePlugin {
  id: string;
  label: string;
  /** false for a plugin the app cannot run without. */
  canDisable: boolean;
  /** Runs on every render for every registered plugin; a disabled plugin returns an empty runtime. */
  useSource(ctx: SourceContext): SourceRuntime;
  /** Rendered in Settings → General while the plugin is enabled. */
  SettingsPanel?: ComponentType<SettingsPanelProps>;
}
