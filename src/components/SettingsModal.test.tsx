import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsModal } from "./SettingsModal";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
const mockReconnectSse = vi.fn();
vi.mock("../lib/listen", () => ({
  reconnectSse: () => mockReconnectSse(),
}));

const DEFAULT_DIR = "/Users/x/.claude/projects";

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

  // --- API access (shared client token) -------------------------------------

  const withToken = (source: "file" | "env" | "disabled", token: string | null) => ({
    ...makeSettings(null),
    api_auth_enabled: source !== "disabled",
    api_token_source: source,
    api_token: token,
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

  const useSettings = (settings: object) => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      if (cmd === "regenerate_api_token") return Promise.resolve(withToken("file", "new-token"));
      return Promise.resolve(settings);
    });
  };

  it("shows the API token masked and reveals it with Show", async () => {
    useSettings(withToken("file", "abc123"));
    renderModal();
    const input = (await screen.findByLabelText("API token")) as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe("abc123"));
    expect(input.type).toBe("password");
    expect(input.readOnly).toBe(true);

    fireEvent.click(screen.getByText("Show"));
    expect(input.type).toBe("text");
    expect(screen.getByText("Hide")).toBeInTheDocument();
  });

  it("copies the token to the clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    useSettings(withToken("file", "abc123"));
    renderModal();
    await waitFor(() =>
      expect((screen.getByLabelText("API token") as HTMLInputElement).value).toBe("abc123"),
    );

    fireEvent.click(screen.getByText("Copy"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("abc123"));
    expect(screen.getByText("Token copied to clipboard.")).toBeInTheDocument();
  });

  it("regenerates only after a confirmation click, then shows the new token and reconnects SSE", async () => {
    useSettings(withToken("file", "old-token"));
    renderModal();
    await waitFor(() =>
      expect((screen.getByLabelText("API token") as HTMLInputElement).value).toBe("old-token"),
    );

    fireEvent.click(screen.getByText("Regenerate"));
    expect(screen.getByText("Confirm regenerate?")).toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalledWith("regenerate_api_token");

    fireEvent.click(screen.getByText("Confirm regenerate?"));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("regenerate_api_token"));
    await waitFor(() =>
      expect((screen.getByLabelText("API token") as HTMLInputElement).value).toBe("new-token"),
    );
    expect(mockReconnectSse).toHaveBeenCalled();
    expect(screen.getByText(/Token regenerated/)).toBeInTheDocument();
    expect(screen.getByText("Regenerate")).toBeInTheDocument();
    // Regeneration is independent of Save — the modal stays open.
    expect(onClose).not.toHaveBeenCalled();
  });

  it("shows the backend error when regeneration fails", async () => {
    useSettings(withToken("file", "old-token"));
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_wsl_distros") return Promise.resolve([]);
      if (cmd === "regenerate_api_token") return Promise.reject(new Error("rotation failed"));
      return Promise.resolve(withToken("file", "old-token"));
    });
    renderModal();
    await screen.findByLabelText("API token");
    fireEvent.click(screen.getByText("Regenerate"));
    fireEvent.click(screen.getByText("Confirm regenerate?"));
    await waitFor(() => expect(screen.getByText(/rotation failed/)).toBeInTheDocument());
    expect(mockReconnectSse).not.toHaveBeenCalled();
  });

  it("disables Regenerate when the token comes from CCTRACE_API_TOKEN", async () => {
    useSettings(withToken("env", "env-token"));
    renderModal();
    await waitFor(() =>
      expect((screen.getByLabelText("API token") as HTMLInputElement).value).toBe("env-token"),
    );
    expect((screen.getByText("Regenerate") as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/cannot be regenerated here/)).toBeInTheDocument();
  });

  it("shows only a hint when client verification is disabled", async () => {
    useSettings(withToken("disabled", null));
    renderModal();
    await waitFor(() => expect(screen.getByText(/CCTRACE_API_AUTH=off/)).toBeInTheDocument());
    expect(screen.queryByLabelText("API token")).toBeNull();
    expect(screen.queryByText("Regenerate")).toBeNull();
  });
});
