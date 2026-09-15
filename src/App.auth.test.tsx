import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";

const mockInvoke = vi.fn();
vi.mock("./lib/invoke", async () => {
  const actual = await vi.importActual<typeof import("./lib/invoke")>("./lib/invoke");
  return { ...actual, invoke: (...args: unknown[]) => mockInvoke(...args) };
});
vi.mock("./lib/listen", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  reconnectSse: vi.fn(),
}));

import { App } from "./App";
import { ApiAuthError } from "./lib/invoke";
import { setApiToken } from "./lib/apiToken";

const SETTINGS = {
  projects_dir: null,
  default_dir: "/d",
  effective_dir: "/d",
  effective_dir_exists: true,
  wsl_distros: [],
  allowed_origins: [],
  can_focus: false,
  api_auth_enabled: true,
  api_auth_source: "file",
  clients: [],
};

describe("App client-verification banner", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setApiToken(null);
  });

  it("shows the banner on a 401 and recovers when a credential arrives (HMR)", async () => {
    let accepted = false;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return accepted
          ? Promise.resolve(SETTINGS)
          : Promise.reject(new ApiAuthError("invalid or revoked client credential"));
      }
      if (cmd === "get_project_dirs") return Promise.resolve([]);
      if (cmd === "discover_sessions") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(<App />);

    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Not an accepted client");
    expect(banner).toHaveTextContent("invalid or revoked client credential");
    const settingsCalls = () => mockInvoke.mock.calls.filter(([c]) => c === "get_settings").length;
    expect(settingsCalls()).toBe(1);

    // The backend (re)issues web-ui; the Vite plugin pushes the credential to
    // this tab over HMR, which lands in setApiToken. The bootstrap re-runs and
    // the banner clears without a reload.
    accepted = true;
    act(() => setApiToken("eyJ.web-ui.sig"));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    await waitFor(() => expect(settingsCalls()).toBe(2));
    expect(mockInvoke).toHaveBeenCalledWith("get_project_dirs");
  });

  it("recovers from a credential that arrives the instant the banner paints", async () => {
    // The listener must be live from mount: the banner state lands from a
    // rejected promise, so its own passive effect may flush a tick after the
    // DOM shows the alert. Fire the credential as soon as the alert exists,
    // without letting any further work flush first.
    let accepted = false;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return accepted
          ? Promise.resolve(SETTINGS)
          : Promise.reject(new ApiAuthError("invalid or revoked client credential"));
      }
      if (cmd === "get_project_dirs") return Promise.resolve([]);
      if (cmd === "discover_sessions") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const { container } = render(<App />);

    await new Promise<void>((resolve) => {
      const observer = new MutationObserver(() => {
        if (!container.querySelector(".app-auth-banner")) return;
        observer.disconnect();
        accepted = true;
        setApiToken("eyJ.web-ui.sig");
        resolve();
      });
      observer.observe(container, { childList: true, subtree: true });
    });

    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([c]) => c === "get_settings")).toHaveLength(2),
    );
  });

  it("ignores a credential that arrives while no banner is up", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(SETTINGS);
      if (cmd === "get_project_dirs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(<App />);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_project_dirs"));
    expect(screen.queryByRole("alert")).toBeNull();

    act(() => setApiToken("eyJ.web-ui.sig"));
    await act(async () => {});
    expect(mockInvoke.mock.calls.filter(([c]) => c === "get_settings")).toHaveLength(1);
  });

  it("ignores a credential being cleared while the banner is up", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_settings"
        ? Promise.reject(new ApiAuthError("nope"))
        : Promise.resolve(undefined),
    );
    setApiToken("stale");
    render(<App />);
    await screen.findByRole("alert");
    act(() => setApiToken(null));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(mockInvoke.mock.calls.filter(([c]) => c === "get_settings")).toHaveLength(1);
  });
});
