import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsModal } from "./SettingsModal";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const DEFAULT_DIR = "/Users/x/.claude/projects";

const makeSettings = (projects_dir: string | null, effective_dir_exists = true) => ({
  projects_dir,
  default_dir: DEFAULT_DIR,
  effective_dir: projects_dir ?? DEFAULT_DIR,
  effective_dir_exists,
});

describe("SettingsModal", () => {
  const onClose = vi.fn();
  const onSaved = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.resolve(makeSettings(null));
      return Promise.resolve();
    });
  });

  it("shows empty input and default hint when no config exists", async () => {
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
    await waitFor(() => {
      expect(screen.getByText(`Default: ${DEFAULT_DIR}`)).toBeInTheDocument();
    });
    const input = screen.getByLabelText("Projects Directory");
    expect((input as HTMLInputElement).value).toBe("");
    expect((input as HTMLInputElement).placeholder).toContain(DEFAULT_DIR);
  });

  it("shows active path when effective dir exists", async () => {
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
    await waitFor(() => {
      expect(screen.getByText(new RegExp(`✓ Active:`))).toBeInTheDocument();
    });
  });

  it("shows missing warning when effective dir does not exist", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null, false));
      return Promise.resolve();
    });
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
    await waitFor(() => {
      expect(screen.getByText(new RegExp(`✗ Not found:`))).toBeInTheDocument();
    });
  });

  it("shows current configured path when one exists", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings("/custom/path"));
      return Promise.resolve();
    });
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
    await waitFor(() => {
      expect(screen.getByDisplayValue("/custom/path")).toBeInTheDocument();
    });
  });

  it("calls set_projects_dir on save", async () => {
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
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
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
    await waitFor(() => expect(screen.getByDisplayValue("/custom/path")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Reset to Default"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_projects_dir", { path: null });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("shows error when save fails", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve(makeSettings(null));
      if (cmd === "set_projects_dir") return Promise.reject("path does not exist: /bad");
      return Promise.resolve();
    });
    render(<SettingsModal onClose={onClose} onSaved={onSaved} />);
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
