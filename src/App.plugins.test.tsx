import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within, waitFor } from "@testing-library/react";

const mockInvoke = vi.fn();
vi.mock("./lib/invoke", async () => {
  const actual = await vi.importActual<typeof import("./lib/invoke")>("./lib/invoke");
  return { ...actual, invoke: (...args: unknown[]) => mockInvoke(...args) };
});
vi.mock("./lib/listen", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  reconnectSse: vi.fn(),
}));
vi.mock("./plugins/registry", async () => {
  const actual = await vi.importActual<typeof import("./plugins/registry")>("./plugins/registry");
  const { alpha, beta } = await import("./test/fakePlugins");
  return { ...actual, PLUGINS: [alpha.plugin, beta.plugin] };
});

import { App } from "./App";
import { alpha, beta } from "./test/fakePlugins";
import { SOURCE_FILTER_KEY } from "./lib/sourceFilter";

function settingsWith(betaEnabled: boolean) {
  return {
    projects_dir: null,
    default_dir: "/d",
    effective_dir: "/d",
    effective_dir_exists: true,
    wsl_distros: [],
    allowed_origins: [],
    plugins: [
      { id: "alpha", label: "alpha", enabled: true, can_disable: false },
      { id: "beta", label: "beta", enabled: betaEnabled, can_disable: true },
    ],
  };
}

function renderApp(betaEnabled: boolean) {
  mockInvoke.mockImplementation((cmd: string, args?: { enabled: boolean }) => {
    if (cmd === "get_settings") return Promise.resolve(settingsWith(betaEnabled));
    if (cmd === "set_plugin_enabled") return Promise.resolve(settingsWith(args!.enabled));
    return Promise.resolve(undefined);
  });
  return render(<App />);
}

const pickerItems = () => screen.queryAllByTestId(/-item$/).map((el) => el.textContent);

const selected = () =>
  screen.getAllByTestId(/-item$/).filter((el) => el.dataset.selected === "true");

describe("App with session-source plugins", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("shows one picker section per enabled plugin, in registry order", async () => {
    renderApp(true);

    await screen.findByText("beta one");
    const items = screen.getAllByTestId(/-item$/).map((el) => el.textContent);
    expect(items).toEqual(["alpha one", "alpha two", "beta one"]);
    expect(screen.getByText("alpha stats")).toBeInTheDocument();
    expect(screen.getByText("beta stats")).toBeInTheDocument();
  });

  it("leaves out everything from a disabled plugin", async () => {
    renderApp(false);

    await screen.findByText("alpha one");
    expect(screen.queryByText("beta one")).toBeNull();
    expect(screen.queryByText("beta stats")).toBeNull();
  });

  it("moves the keyboard selection from one plugin's section into the next", async () => {
    renderApp(true);
    await screen.findByText("beta one");

    fireEvent.keyDown(window, { key: "j" });
    fireEvent.keyDown(window, { key: "j" });
    expect(selected().map((el) => el.textContent)).toEqual(["beta one"]);

    fireEvent.keyDown(window, { key: "j" });
    expect(selected().map((el) => el.textContent)).toEqual(["beta one"]);

    fireEvent.keyDown(window, { key: "Enter" });
    expect(beta.opened).toHaveBeenCalledWith(0);
    expect(alpha.opened).not.toHaveBeenCalled();
  });

  it("maps a hover in a later section onto the shared selection", async () => {
    renderApp(true);
    await screen.findByText("beta one");

    fireEvent.mouseEnter(screen.getByText("beta one"));
    fireEvent.keyDown(window, { key: "k" });

    expect(selected().map((el) => el.textContent)).toEqual(["alpha two"]);
  });

  it("shows the opened plugin's view, toolbar, hints and info bar, and returns to the picker", async () => {
    renderApp(true);
    await screen.findByText("beta one");

    fireEvent.mouseEnter(screen.getByText("beta one"));
    fireEvent.keyDown(window, { key: "Enter" });

    expect(screen.getByText("beta view")).toBeInTheDocument();
    expect(screen.getByText("beta info")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "beta nav" })).toBeInTheDocument();
    expect(screen.getByText("beta hint")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "q" });
    expect(screen.getByText("beta one")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Back to Messages/ })).toBeInTheDocument();
  });

  it("shows only enabled plugins' panels in Settings", async () => {
    renderApp(false);
    await screen.findByText("alpha one");

    fireEvent.click(screen.getByTitle("Settings"));

    const modal = await screen.findByText("alpha settings");
    expect(modal).toBeInTheDocument();
    expect(within(document.body).queryByText("beta settings")).toBeNull();
  });

  it("narrows the picker to one source and remembers the choice", async () => {
    const { unmount } = renderApp(true);
    await screen.findByText("beta one");

    fireEvent.click(screen.getByRole("button", { name: "beta" }));
    expect(pickerItems()).toEqual(["beta one"]);
    expect(screen.queryByText("alpha stats")).toBeNull();

    unmount();
    renderApp(true);
    await screen.findByText("beta one");
    expect(pickerItems()).toEqual(["beta one"]);

    fireEvent.click(screen.getByRole("button", { name: "All" }));
    expect(pickerItems()).toEqual(["alpha one", "alpha two", "beta one"]);
  });

  it("hides the source filter when only one source is on", async () => {
    renderApp(false);
    await screen.findByText("alpha one");
    expect(screen.queryByRole("group", { name: "Show sessions from" })).toBeNull();
  });

  it("shows every source again when the remembered one is turned off", async () => {
    localStorage.setItem(SOURCE_FILTER_KEY, "beta");
    renderApp(false);
    await screen.findByText("alpha one");
    expect(pickerItems()).toEqual(["alpha one", "alpha two"]);
  });

  it("adds and removes a source's sessions when it is switched in Settings", async () => {
    renderApp(false);
    await screen.findByText("alpha one");
    fireEvent.click(screen.getByTitle("Settings"));

    const toggle = await screen.findByRole("switch", { name: "Show beta sessions" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);

    expect(await screen.findByText("beta one")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("set_plugin_enabled", { id: "beta", enabled: true });
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));

    fireEvent.click(toggle);
    await waitFor(() => expect(screen.queryByText("beta one")).toBeNull());
  });

  it("offers no switch for a source that is always on", async () => {
    renderApp(true);
    await screen.findByText("alpha one");
    fireEvent.click(screen.getByTitle("Settings"));

    await screen.findByRole("switch", { name: "Show beta sessions" });
    expect(screen.queryByRole("switch", { name: "Show alpha sessions" })).toBeNull();
  });
});
