import {
  Fragment,
  useState,
  useEffect,
  useLayoutEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { invoke, ApiAuthError } from "./lib/invoke";
import { onApiTokenChange } from "./lib/apiToken";
import type { SettingsResponse } from "./types";
import type { KeyMap, SettingsTab, SourcePlugin, SourceRuntime } from "./plugins/types";
import { PLUGINS, isPluginEnabled, useSourceRuntimes } from "./plugins/registry";
import { useKeyboard } from "./hooks/useKeyboard";
import { useViewActionsRef, useViewActionCallbacks } from "./hooks/useViewActions";
import { useFontScale } from "./hooks/useFontScale";
import { useRecapPreview } from "./hooks/useRecapPreview";
import { useSourceFilter } from "./hooks/useSourceFilter";
import { ALL_SOURCES } from "./lib/sourceFilter";
import { SessionPicker } from "./components/SessionPicker";
import { KeybindBar, PICKER_HINTS } from "./components/KeybindBar";
import { ViewToolbar } from "./components/ViewToolbar";
import { ResizeHandle } from "./components/ResizeHandle";
import { SettingsModal } from "./components/SettingsModal";
import { BackToMessagesButton } from "./components/BackToMessagesButton";
import { SourceFilter } from "./components/SourceFilter";

/** Where the main column is: the shared picker, or one of a plugin's views. */
type Location = { kind: "picker" } | { kind: "view"; plugin: string; view: string };

const PICKER: Location = { kind: "picker" };

interface ActiveSource {
  plugin: SourcePlugin;
  runtime: SourceRuntime;
}

export function App() {
  const [location, setLocation] = useState<Location>(PICKER);
  const [pickerSelectedIndex, setPickerSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [showKeybinds, setShowKeybinds] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(180);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>("general");
  const [fontScale, setFontScale] = useFontScale();
  const [recapPreview, setRecapPreview] = useRecapPreview();
  const [sourceFilter, setSourceFilter] = useSourceFilter();
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  // The plugin whose session "Back to Messages" returns to.
  const [sessionOwner, setSessionOwner] = useState<string | null>(null);

  // Set when the backend rejects this browser as an unaccepted client (HTTP
  // 401, see lib/apiToken.ts). Every API call would fail the same way — the
  // Settings modal included — so a banner replaces the usual bootstrap path.
  const [authError, setAuthError] = useState<string | null>(null);

  const viewActionsRef = useViewActionsRef();
  const { expandAll, collapseAll, scrollToTop, scrollToBottom } =
    useViewActionCallbacks(viewActionsRef);

  const openSettings = useCallback((tab: SettingsTab = "general") => {
    setSettingsInitialTab(tab);
    setShowSettings(true);
  }, []);

  const goToPicker = useCallback(() => setLocation(PICKER), []);

  const viewOpeners = useMemo(
    () =>
      Object.fromEntries(
        PLUGINS.map((plugin) => [
          plugin.id,
          (view: string) => {
            setLocation({ kind: "view", plugin: plugin.id, view });
            setSessionOwner(plugin.id);
          },
        ]),
      ),
    [],
  );

  const runtimes = useSourceRuntimes(PLUGINS, (plugin) => ({
    enabled: isPluginEnabled(plugin, settings?.plugins),
    activeView: location.kind === "view" && location.plugin === plugin.id ? location.view : null,
    openView: viewOpeners[plugin.id],
    goToPicker,
    setPickerIndex: setPickerSelectedIndex,
    searchQuery,
    viewActionsRef,
    fontScale,
    recapPreview,
    settings,
    openSettings,
  }));

  const sources: ActiveSource[] = PLUGINS.flatMap((plugin, i) =>
    isPluginEnabled(plugin, settings?.plugins) ? [{ plugin, runtime: runtimes[i] }] : [],
  );

  // Async callbacks below read the plugins' latest runtimes, not the ones from
  // the render that created the callback.
  const runtimesRef = useRef(runtimes);
  useLayoutEffect(() => {
    runtimesRef.current = runtimes;
  });

  const settingsRef = useRef(settings);
  useLayoutEffect(() => {
    settingsRef.current = settings;
  });

  /** Hand `loaded` to every enabled plugin that `shouldStart` picks. */
  const startPlugins = useCallback(
    (loaded: SettingsResponse | null, shouldStart: (plugin: SourcePlugin) => boolean) =>
      Promise.all(
        PLUGINS.flatMap((plugin, i) =>
          isPluginEnabled(plugin, loaded?.plugins) && shouldStart(plugin)
            ? [runtimesRef.current[i].onSettingsLoaded?.(loaded)]
            : [],
        ),
      ),
    [],
  );

  // Bootstrap: load settings, then let each enabled plugin start up from them.
  // A 401 means this browser is not an accepted client; every other call would
  // fail the same way, so stop at the banner.
  const discover = useCallback(async () => {
    let loaded: SettingsResponse | null = null;
    try {
      loaded = await invoke<SettingsResponse>("get_settings");
    } catch (err) {
      if (err instanceof ApiAuthError) {
        setAuthError(err.message);
        return;
      }
      // no settings file yet
    }
    setSettings(loaded);
    await startPlugins(loaded, () => true);
  }, [startPlugins]);

  // A plugin was turned on or off in Settings: show the new set, and start the
  // plugins that just came on.
  const handlePluginsChanged = useCallback(
    async (next: SettingsResponse) => {
      const before = settingsRef.current?.plugins;
      setSettings(next);
      await startPlugins(next, (plugin) => !isPluginEnabled(plugin, before));
    },
    [startPlugins],
  );

  // Auto-discover sessions on mount (once, even under StrictMode's double effect).
  const discoveredRef = useRef(false);
  useEffect(() => {
    if (discoveredRef.current) return;
    discoveredRef.current = true;
    void discover();
  }, [discover]);

  // Recover from the banner when this tab's credential arrives or changes:
  // in dev/web mode the backend may start after the dev server (first run),
  // or `web-ui` may have been reissued from another client. The Vite plugin
  // pushes the new value over HMR, so re-run the bootstrap instead of leaving
  // a banner whose remedy (a restart) is no longer needed.
  //
  // Subscribed for the whole mount, not only while the banner is up. The
  // banner's state lands from a rejected promise, so React may flush the
  // passive effect that would subscribe a tick after the banner paints; a
  // credential arriving in that gap found no listener and the banner stuck.
  // The ref is written in a layout effect so the listener sees the banner
  // state from the same commit that painted it, not a passive tick later.
  const authErrorRef = useRef<string | null>(null);
  useLayoutEffect(() => {
    authErrorRef.current = authError;
  }, [authError]);
  useEffect(
    () =>
      onApiTokenChange((credential) => {
        if (!credential || !authErrorRef.current) return;
        setAuthError(null);
        void discover();
      }),
    [discover],
  );

  const handleSettingsSaved = () => {
    for (const source of sources) source.runtime.onSettingsSaved?.();
  };

  const toggleKeybinds = useCallback(() => {
    setShowKeybinds((v) => !v);
  }, []);

  const activeSource =
    location.kind === "view" ? sources.find((s) => s.plugin.id === location.plugin) : undefined;
  const activeView =
    location.kind === "view" ? activeSource?.runtime.views[location.view] : undefined;
  const owner = sources.find((s) => s.plugin.id === sessionOwner)?.runtime;
  const backToSession = () => owner?.backToSession();

  const sidebars = sources.flatMap((s) => (s.runtime.sidebar ? [s] : []));
  const focusedSidebar = sidebars.find((s) => s.runtime.sidebar?.focused)?.runtime.sidebar;
  const blurSidebars = () => {
    for (const s of sidebars) s.runtime.sidebar?.blur();
  };
  const focusSidebar = () => sidebars[0]?.runtime.sidebar?.focus();

  // The picker shows every enabled plugin, or just the one the filter picks.
  const filterMatches = sources.some((s) => s.plugin.id === sourceFilter);
  const activeFilter = filterMatches ? sourceFilter : ALL_SOURCES;
  const pickerSources = filterMatches
    ? sources.filter((s) => s.plugin.id === sourceFilter)
    : sources;
  const changeSourceFilter = (filter: string) => {
    setSourceFilter(filter);
    setPickerSelectedIndex(0);
  };

  // The picker is one list across every shown plugin's section; map the
  // shared selection onto (section, index within it).
  const sections = pickerSources.map((s) => s.runtime.picker);
  const pickerTotal = sections.reduce((sum, section) => sum + section.count, 0);
  const openPickerItem = (index: number) => {
    let offset = 0;
    for (const section of sections) {
      if (index >= offset && index < offset + section.count) {
        section.open(index - offset);
        return;
      }
      offset += section.count;
    }
  };

  // Keyboard navigation — the focused sidebar, else the active view, else the picker.
  let keyMap: KeyMap;
  if (focusedSidebar) {
    keyMap = { ...focusedSidebar.keyMap };
  } else if (activeView) {
    keyMap = { ...activeView.keyMap };
  } else {
    const pickerDown = () => setPickerSelectedIndex((i) => Math.min(i + 1, pickerTotal - 1));
    const pickerUp = () => setPickerSelectedIndex((i) => Math.max(i - 1, 0));
    keyMap = {
      j: pickerDown,
      ArrowDown: pickerDown,
      k: pickerUp,
      ArrowUp: pickerUp,
      Enter: () => openPickerItem(pickerSelectedIndex),
      q: backToSession,
      Escape: backToSession,
      h: focusSidebar,
      ArrowLeft: focusSidebar,
    };
  }
  keyMap["?"] = toggleKeybinds;
  useKeyboard(keyMap);

  const renderPicker = () => {
    let offset = 0;
    return (
      <div className="picker-dashboard">
        <SessionPicker
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          headerStats={pickerSources.map((s) => (
            <Fragment key={s.plugin.id}>{s.runtime.picker.headerStats}</Fragment>
          ))}
          viewActionsRef={viewActionsRef}
        >
          {pickerSources.map((s) => {
            const sectionOffset = offset;
            const { count } = s.runtime.picker;
            offset += count;
            const local = pickerSelectedIndex - sectionOffset;
            return (
              <Fragment key={s.plugin.id}>
                {s.runtime.picker.render({
                  selectedIndex: local >= 0 && local < count ? local : null,
                  onHover: (localIndex) => setPickerSelectedIndex(sectionOffset + localIndex),
                })}
              </Fragment>
            );
          })}
        </SessionPicker>
      </div>
    );
  };

  return (
    <div className="app">
      {authError && (
        <div className="app-auth-banner" role="alert">
          <strong>Not an accepted client.</strong> This browser did not present a valid{" "}
          <code>web-ui</code> client credential, so the backend refused the connection. In dev/web
          mode restart <code>cctrace --web</code>; in Docker open the UI via localhost or an allowed
          origin. If <code>web-ui</code> was revoked, reissue it from the desktop app or another
          client.
          <span className="app-auth-banner__detail">{authError}</span>
        </div>
      )}
      {activeView?.infoBar}

      <ViewToolbar
        left={
          activeView ? (
            activeView.toolbarLeft
          ) : (
            <>
              {owner?.hasOpenSession && <BackToMessagesButton onClick={backToSession} />}
              <SourceFilter
                sources={sources.map((s) => ({ id: s.plugin.id, label: s.plugin.label }))}
                value={activeFilter}
                onChange={changeSourceFilter}
              />
            </>
          )
        }
        collapsible={activeView?.collapsible ?? false}
        onExpandAll={expandAll}
        onCollapseAll={collapseAll}
        onScrollToTop={scrollToTop}
        onScrollToBottom={scrollToBottom}
        onOpenSettings={() => openSettings("general")}
      />

      <div className="app-body">
        {sidebars.map((s) => (
          <Fragment key={s.plugin.id}>
            {s.runtime.sidebar?.render({ width: sidebarWidth, minWidth: 100, maxWidth: 400 })}
          </Fragment>
        ))}
        {sidebars.length > 0 && <ResizeHandle onResize={setSidebarWidth} />}
        <div className="main-content" onClick={blurSidebars}>
          {activeView ? activeView.render() : renderPicker()}
        </div>
      </div>

      <KeybindBar
        status={sources.map((s) => (
          <Fragment key={s.plugin.id}>{s.runtime.status}</Fragment>
        ))}
        hints={activeView ? activeView.hints : PICKER_HINTS}
        showHints={showKeybinds}
        onToggle={toggleKeybinds}
        actions={activeView ? activeView.hintActions : { back: backToSession }}
      />

      {showSettings && (
        <SettingsModal
          onClose={() => setShowSettings(false)}
          onSaved={handleSettingsSaved}
          onPluginsChanged={(next) => void handlePluginsChanged(next)}
          fontScale={fontScale}
          onFontScaleChange={setFontScale}
          recapPreview={recapPreview}
          onRecapPreviewChange={setRecapPreview}
          initialTab={settingsInitialTab}
        />
      )}
      {sources.map((s) => (
        <Fragment key={s.plugin.id}>{s.runtime.overlays}</Fragment>
      ))}
    </div>
  );
}
