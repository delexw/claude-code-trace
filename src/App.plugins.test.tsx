import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";

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
  mockInvoke.mockImplementation((cmd: string) =>
    cmd === "get_settings"
      ? Promise.resolve(settingsWith(betaEnabled))
      : Promise.resolve(undefined),
  );
  return render(<App />);
}

const selected = () =>
  screen.getAllByTestId(/-item$/).filter((el) => el.dataset.selected === "true");

describe("App with session-source plugins", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
});
