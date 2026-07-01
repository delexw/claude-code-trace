import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsModal } from "./SettingsModal";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const DEFAULT_DIR = "/Users/x/.claude/projects";

const makeSettings = (
  projects_dir: string | null,
  effective_dir_exists = true,
  wsl_distros: string[] = [],
) => ({
  projects_dir,
  default_dir: DEFAULT_DIR,
  effective_dir: projects_dir ?? DEFAULT_DIR,
  effective_dir_exists,
  wsl_distros,
});

describe("SettingsModal", () => {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  const onFontScaleChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null));
      if (cmd === "set_wsl_distros") return Promise.resolve(makeSettings(null));
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

  it("highlights the active font scale and applies a new one on click", async () => {
    render(
      <SettingsModal
        onClose={onClose}
        onSaved={onSaved}
        fontScale={1}
        onFontScaleChange={onFontScaleChange}
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
});
