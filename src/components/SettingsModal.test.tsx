import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { SettingsModal } from "./SettingsModal";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
import { getApiToken, setApiToken } from "../lib/apiToken";

const DEFAULT_DIR = "/Users/x/.claude/projects";

/** The Accepted-clients table row for `name`. */
const row = (name: string) =>
  screen.getByText(name, { selector: ".settings-modal__client-name" }).closest("tr")!;

const makeSettings = (
  projects_dir: string | null,
  effective_dir_exists = true,
  wsl_distros: string[] = [],
  allowed_origins: string[] = [],
) => ({
  projects_dir,
  default_dir: DEFAULT_DIR,
  effective_dir: projects_dir ?? DEFAULT_DIR,
  effective_dir_exists,
  wsl_distros,
  allowed_origins,
});

describe("SettingsModal", () => {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  const onFontScaleChange = vi.fn();
  const onRecapPreviewChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    setApiToken(null);
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null));
      if (cmd === "set_wsl_distros") return Promise.resolve(makeSettings(null));
      if (cmd === "set_allowed_origins") return Promise.resolve(makeSettings(null));
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      return Promise.resolve();
    });
  });

  it("shows empty input and default hint when no config exists", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument();
    });
    const input = screen.getByLabelText("Projects Directory");
    expect((input as HTMLInputElement).value).toBe("");
    expect((input as HTMLInputElement).placeholder).toContain(DEFAULT_DIR);
  });

  it("shows active path when effective dir exists", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(new RegExp(`✓ Active:`))).toBeInTheDocument();
    });
  });

  it("shows missing warning when effective dir does not exist", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null, false));
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(new RegExp(`✗ Not found:`))).toBeInTheDocument();
    });
  });

  it("shows current configured path when one exists", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings("/custom/path"));
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByDisplayValue("/custom/path")).toBeInTheDocument();
    });
  });

  it("calls set_projects_dir on save", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument());

    const input = screen.getByLabelText("Projects Directory");
    fireEvent.change(input, { target: { value: "/new/path" } });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_projects_dir", { path: "/new/path" });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls set_projects_dir with null on reset", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings("/custom/path"));
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null));
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByDisplayValue("/custom/path")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Reset to Default"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_projects_dir", { path: null });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("shows no-distros hint when WSL reports none", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(/No WSL distributions detected/)).toBeInTheDocument();
    });
  });

  it("renders detected WSL distros with configured ones checked", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null, true, ["Ubuntu"]));
      if (cmd === "list_wsl_distros") return Promise.resolve(["Ubuntu", "Debian"]);
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );

    await waitFor(() => expect(screen.getByLabelText("Ubuntu")).toBeInTheDocument());
    expect((screen.getByLabelText("Ubuntu") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Debian") as HTMLInputElement).checked).toBe(false);
  });

  it("persists toggled WSL distros on save", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null, true, ["Ubuntu"]));
      if (cmd === "list_wsl_distros") return Promise.resolve(["Ubuntu", "Debian"]);
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null, true, ["Ubuntu"]));
      if (cmd === "set_wsl_distros")
        return Promise.resolve(makeSettings(null, true, ["Ubuntu", "Debian"]));
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );

    await waitFor(() => expect(screen.getByLabelText("Debian")).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText("Debian"));
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_wsl_distros", {
        distros: ["Ubuntu", "Debian"],
      });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("shows configured allowed origins in the textarea", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve(
          makeSettings(null, true, [], ["https://a.example", "https://b.example"]),
        );
      }
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );

    const textarea = await screen.findByLabelText("Allowed Origins (CORS)");
    expect((textarea as HTMLTextAreaElement).value).toBe("https://a.example\nhttps://b.example");
  });

  it("calls set_allowed_origins with parsed origins on save", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument());

    const textarea = screen.getByLabelText("Allowed Origins (CORS)");
    fireEvent.change(textarea, {
      target: { value: "https://a.example\n https://b.example ,https://c.example" },
    });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_allowed_origins", {
        origins: ["https://a.example", "https://b.example", "https://c.example"],
      });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("shows error when allowed-origins save fails", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null));
      if (cmd === "set_wsl_distros") return Promise.resolve(makeSettings(null));
      if (cmd === "set_allowed_origins")
        return Promise.reject("invalid origin: bad (expected e.g. http://example.com:8080)");
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument());

    const textarea = screen.getByLabelText("Allowed Origins (CORS)");
    fireEvent.change(textarea, { target: { value: "bad" } });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(
        screen.getByText("invalid origin: bad (expected e.g. http://example.com:8080)"),
      ).toBeInTheDocument();
    });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("highlights the active font scale and applies a new one on click", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument());

    expect(screen.getByRole("button", { name: "100%", pressed: true })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "150%" }));
    expect(onFontScaleChange).toHaveBeenCalledWith(1.5);
  });

  it("shows error when save fails", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.reject("path does not exist: /bad");
      return Promise.resolve();
    });
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    await waitFor(() => expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument());

    const input = screen.getByLabelText("Projects Directory");
    fireEvent.change(input, { target: { value: "/bad" } });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(screen.getByText("path does not exist: /bad")).toBeInTheDocument();
    });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("toggles recap preview via the SESSION PREVIEW control", async () => {
    const onChange = vi.fn();
    render(
      <SettingsModal
        onClose={() => {}}
        onSaved={() => {}}
        fontScale={1}
        onFontScaleChange={() => {}}
        recapPreview={true}
        onRecapPreviewChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole("switch", { name: /recap preview/i }));
    expect(onChange).toHaveBeenCalledWith(false);
  });

  // --- Accepted clients (per-client credentials) ---------------------------

  const WEB_UI = {
    id: "11111111-1111-4111-8111-111111111111",
    name: "web-ui",
    builtin: true,
    created_at: 1_700_000_000,
    issued_at: 1_700_000_000,
  };
  const TUI = { ...WEB_UI, id: "22222222-2222-4222-8222-222222222222", name: "tui" };
  const SCRIPT = {
    id: "33333333-3333-4333-8333-333333333333",
    name: "ci-script",
    builtin: false,
    created_at: 1_700_000_100,
    issued_at: 1_700_000_100,
    revoked_at: 1_700_000_200,
  };

  const withClients = (
    source: "file" | "ephemeral" | "disabled",
    clients: object[] = [WEB_UI, TUI, SCRIPT],
  ) => ({
    ...makeSettings(null),
    api_auth_enabled: source !== "disabled",
    api_auth_source: source,
    clients,
  });

  const renderModal = () =>
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={true}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );

  const useSettings = (settings: object, extra: Record<string, unknown> = {}) => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      if (cmd in extra) {
        const v = extra[cmd];
        return v instanceof Error ? Promise.reject(v) : Promise.resolve(v);
      }
      return Promise.resolve(settings);
    });
  };

  it("lists the accepted clients with built-in badges and status", async () => {
    useSettings(withClients("file"));
    renderModal();
    const table = await screen.findByRole("table", { name: "Accepted clients" });
    expect(table).toBeInTheDocument();
    expect(row("web-ui")).toHaveTextContent("built-in");
    expect(row("web-ui")).toHaveTextContent("Active");
    expect(row("tui")).toHaveTextContent("built-in");
    expect(row("ci-script")).not.toHaveTextContent("built-in");
    expect(row("ci-script")).toHaveTextContent("Revoked");
    // A revoked client cannot be revoked again, but can be reissued.
    expect((screen.getByLabelText("Revoke ci-script") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Reissue ci-script") as HTMLButtonElement).disabled).toBe(false);
    // Credentials are never part of the listing.
    expect(screen.queryByLabelText("New client credential")).toBeNull();
  });

  it("names both token carriers and dates each client unambiguously", async () => {
    useSettings(withClients("file"));
    renderModal();
    const table = await screen.findByRole("table", { name: "Accepted clients" });
    // `Authorization: Bearer` is accepted just as much as the custom header
    // (see `presented_credentials` in auth.rs); naming only one sends people
    // down a single path and hides the other.
    expect(screen.getByText("Authorization: Bearer")).toBeInTheDocument();
    expect(screen.getByText("X-CCTrace-Token")).toBeInTheDocument();
    expect(within(table).getByRole("columnheader", { name: "Issued" })).toBeInTheDocument();
    // A named month: "08/09/2026" reads as two different dates depending on
    // the reader's locale.
    const date = within(row("web-ui")).getAllByRole("cell")[1].textContent ?? "";
    expect(date).toMatch(/[A-Za-z]{3}/);
    expect(date).toMatch(/\d{4}/);
  });

  it("registers a client and shows its credential exactly once", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const registered = {
      id: "44444444-4444-4444-8444-444444444444",
      name: "deploy-bot",
      builtin: false,
      created_at: 1_700_000_300,
      issued_at: 1_700_000_300,
    };
    useSettings(withClients("file"), {
      register_client: { client: registered, credential: "eyJ.deploy.sig" },
    });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });

    const addButton = screen.getByText("Add client") as HTMLButtonElement;
    expect(addButton.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("New client name"), {
      target: { value: "  deploy-bot " },
    });
    expect(addButton.disabled).toBe(false);
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("register_client", { name: "deploy-bot" }),
    );
    const credential = (await screen.findByLabelText("New client credential")) as HTMLInputElement;
    expect(credential.value).toBe("eyJ.deploy.sig");
    expect(credential.readOnly).toBe(true);
    expect(screen.getByText(/shown once and not kept/)).toBeInTheDocument();
    expect(row("deploy-bot")).toHaveTextContent("Active");
    expect((screen.getByLabelText("New client name") as HTMLInputElement).value).toBe("");

    fireEvent.click(screen.getByText("Copy"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("eyJ.deploy.sig"));
    expect(screen.getByText("Credential copied to clipboard.")).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("Dismiss credential"));
    expect(screen.queryByLabelText("New client credential")).toBeNull();
    // Registering never touches this tab's own credential.
    expect(getApiToken()).toBeNull();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("reissues only after a confirmation click and swaps this tab's credential for web-ui", async () => {
    useSettings(withClients("file"), {
      reissue_client: {
        client: { ...WEB_UI, issued_at: 1_700_000_500 },
        credential: "eyJ.new-web.sig",
      },
    });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });

    fireEvent.click(screen.getByLabelText("Reissue web-ui"));
    expect(screen.getByText("Confirm reissue?")).toBeInTheDocument();
    expect(screen.getByText(/Click again to confirm/)).toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalledWith("reissue_client", expect.anything());

    fireEvent.click(screen.getByText("Confirm reissue?"));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("reissue_client", { id: WEB_UI.id }),
    );
    await waitFor(() => expect(getApiToken()).toBe("eyJ.new-web.sig"));
    expect(screen.getByText(/"web-ui" reissued/)).toBeInTheDocument();
    expect((screen.getByLabelText("New client credential") as HTMLInputElement).value).toBe(
      "eyJ.new-web.sig",
    );
    expect(screen.getByLabelText("Reissue web-ui")).toHaveTextContent("Reissue");
  });

  it("reissuing a non-web-ui client leaves this tab's credential alone", async () => {
    setApiToken("mine");
    useSettings(withClients("file"), {
      reissue_client: { client: { ...TUI, issued_at: 1_700_000_500 }, credential: "eyJ.tui.sig" },
    });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });
    fireEvent.click(screen.getByLabelText("Reissue tui"));
    fireEvent.click(screen.getByText("Confirm reissue?"));
    await waitFor(() => expect(screen.getByText(/"tui" reissued/)).toBeInTheDocument());
    expect(getApiToken()).toBe("mine");
  });

  it("revokes only after a confirmation click and warns before locking out web-ui", async () => {
    useSettings(withClients("file"), {
      revoke_client: { ...TUI, revoked_at: 1_700_000_600 },
    });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });

    // Arming web-ui explains the consequence; arming tui replaces it.
    fireEvent.click(screen.getByLabelText("Revoke web-ui"));
    expect(screen.getByText(/locks every browser tab/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Revoke tui"));
    expect(screen.getByLabelText("Revoke web-ui")).toHaveTextContent("Revoke");
    expect(screen.getByLabelText("Revoke tui")).toHaveTextContent("Confirm revoke?");
    expect(mockInvoke).not.toHaveBeenCalledWith("revoke_client", expect.anything());

    fireEvent.click(screen.getByLabelText("Revoke tui"));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("revoke_client", { id: TUI.id }));
    await waitFor(() => expect(row("tui")).toHaveTextContent("Revoked"));
    expect((screen.getByLabelText("Revoke tui") as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/"tui" revoked/)).toBeInTheDocument();
  });

  it("runs one client action at a time — a second Enter does not register twice", async () => {
    let resolveRegister!: (v: unknown) => void;
    const inFlight = new Promise((resolve) => {
      resolveRegister = resolve;
    });
    useSettings(withClients("file"), { register_client: inFlight });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });

    const input = screen.getByLabelText("New client name");
    fireEvent.change(input, { target: { value: "twice" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.click(screen.getByLabelText("Reissue tui"));
    fireEvent.click(screen.getByLabelText("Reissue tui")); // confirm while busy → ignored
    expect(mockInvoke.mock.calls.filter(([c]) => c === "register_client")).toHaveLength(1);
    expect(mockInvoke).not.toHaveBeenCalledWith("reissue_client", expect.anything());

    resolveRegister({
      client: { id: "5", name: "twice", builtin: false, created_at: 1, issued_at: 1 },
      credential: "eyJ.twice.sig",
    });
    await screen.findByLabelText("New client credential");
    expect(mockInvoke.mock.calls.filter(([c]) => c === "register_client")).toHaveLength(1);
  });

  it("shows the backend error when a client action fails", async () => {
    useSettings(withClients("file"), {
      register_client: new Error('a client named "tui" already exists'),
    });
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });
    fireEvent.change(screen.getByLabelText("New client name"), { target: { value: "tui" } });
    fireEvent.click(screen.getByText("Add client"));
    await waitFor(() => expect(screen.getByText(/already exists/)).toBeInTheDocument());
    expect(screen.queryByLabelText("New client credential")).toBeNull();
  });

  it("explains when the signing key could not be persisted", async () => {
    useSettings(withClients("ephemeral"));
    renderModal();
    await screen.findByRole("table", { name: "Accepted clients" });
    expect(screen.getByText(/signing key could not be written at startup/)).toBeInTheDocument();
  });

  it("shows only a hint when client verification is disabled", async () => {
    useSettings(withClients("disabled", []));
    renderModal();
    await waitFor(() => expect(screen.getByText(/CCTRACE_API_AUTH=off/)).toBeInTheDocument());
    expect(screen.queryByRole("table", { name: "Accepted clients" })).toBeNull();
    expect(screen.queryByText("Add client")).toBeNull();
  });
});

describe("SettingsModal Antigravity toggle", () => {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  const onFontScaleChange = vi.fn();
  const onRecapPreviewChange = vi.fn();

  const ANTIGRAVITY_BRAIN = "/Users/x/.gemini/antigravity-ide/brain";

  beforeEach(() => {
    vi.clearAllMocks();
    setApiToken(null);
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({
          projects_dir: null,
          default_dir: DEFAULT_DIR,
          effective_dir: DEFAULT_DIR,
          effective_dir_exists: true,
          wsl_distros: [],
          allowed_origins: [],
          enable_antigravity: false,
          antigravity_brain_root: ANTIGRAVITY_BRAIN,
          antigravity_brain_root_exists: true,
        });
      }
      if (cmd === "set_antigravity_enabled") {
        return Promise.resolve({
          projects_dir: null,
          default_dir: DEFAULT_DIR,
          effective_dir: DEFAULT_DIR,
          effective_dir_exists: true,
          wsl_distros: [],
          allowed_origins: [],
          enable_antigravity: true,
          antigravity_brain_root: ANTIGRAVITY_BRAIN,
          antigravity_brain_root_exists: true,
        });
      }
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      return Promise.resolve();
    });
  });

  it("renders the Antigravity toggle in the off state", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const toggle = await screen.findByRole("switch", {
      name: "Show Antigravity conversations",
    });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/Reading from:/)).toBeInTheDocument();
    expect(
      screen.getByText(new RegExp(ANTIGRAVITY_BRAIN.replace(/[/]/g, "\\/"))),
    ).toBeInTheDocument();
  });

  it("invokes set_antigravity_enabled and notifies onSaved when toggled on", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const toggle = await screen.findByRole("switch", {
      name: "Show Antigravity conversations",
    });
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_antigravity_enabled", {
        enabled: true,
      });
    });
    // The picker refresh hook is fired on toggle so the new folders show up.
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  it("shows an editable Brain Directory field seeded empty when auto-detecting", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const input = (await screen.findByLabelText(/Brain Directory/i)) as HTMLInputElement;
    // Empty means "auto-detect"; the resolved path is shown as a hint, not
    // baked into the field.
    expect(input.value).toBe("");
    expect(input.placeholder).toContain(ANTIGRAVITY_BRAIN);
    expect(input.placeholder).toMatch(/auto-detected/i);
  });

  it("seeds the field with the user's existing override", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({
          projects_dir: null,
          default_dir: DEFAULT_DIR,
          effective_dir: DEFAULT_DIR,
          effective_dir_exists: true,
          wsl_distros: [],
          allowed_origins: [],
          enable_antigravity: true,
          antigravity_root: "/custom/brain",
          antigravity_brain_root: "/custom/brain",
          antigravity_brain_root_exists: true,
          antigravity_brain_roots: [{ flavor: "custom", path: "/custom/brain", exists: true }],
        });
      }
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      return Promise.resolve();
    });

    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const input = (await screen.findByLabelText(/Brain Directory/i)) as HTMLInputElement;
    expect(input.value).toBe("/custom/brain");
  });

  it("saves the typed brain root, and clears it when emptied", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const input = await screen.findByLabelText(/Brain Directory/i);
    fireEvent.change(input, { target: { value: "/mnt/c/Users/me/.gemini/antigravity/brain" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_antigravity_root", {
        path: "/mnt/c/Users/me/.gemini/antigravity/brain",
      });
    });

    // Emptying the field means "go back to auto-detection", not "read /".
    mockInvoke.mockClear();
    fireEvent.change(input, { target: { value: "  " } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_antigravity_root", { path: null });
    });
  });

  it("names the other installs it found when more than one exists", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({
          projects_dir: null,
          default_dir: DEFAULT_DIR,
          effective_dir: DEFAULT_DIR,
          effective_dir_exists: true,
          wsl_distros: [],
          allowed_origins: [],
          enable_antigravity: true,
          antigravity_brain_root: ANTIGRAVITY_BRAIN,
          antigravity_brain_root_exists: true,
          antigravity_brain_roots: [
            { flavor: "antigravity-ide", path: ANTIGRAVITY_BRAIN, exists: true },
            { flavor: "antigravity", path: "/Users/x/.gemini/antigravity/brain", exists: true },
            { flavor: "antigravity-backup", path: "/nope", exists: false },
          ],
        });
      }
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      return Promise.resolve();
    });

    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
        recapPreview={false}
        onRecapPreviewChange={onRecapPreviewChange}
      />,
    );
    const alsoFound = await screen.findByText(/Also found:/);
    expect(alsoFound).toBeInTheDocument();
    // Only roots that exist are named — a missing one is not an install.
    expect(alsoFound.textContent).toContain("antigravity →");
    expect(alsoFound.textContent).not.toContain("backup");
  });
});
