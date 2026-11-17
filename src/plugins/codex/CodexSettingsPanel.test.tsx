import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import type { SettingsPanelHandle } from "../types";

const mockInvoke = vi.fn();
vi.mock("../../lib/invoke", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
import { CodexSettingsPanel } from "./CodexSettingsPanel";

const roots = [
  { path: "/sample/.codex", source: "local", exists: true },
  {
    path: "\\\\wsl.localhost\\SampleLinux\\home\\sample\\.codex",
    source: "WSL: SampleLinux",
    exists: true,
  },
];

function panel() {
  let handle: SettingsPanelHandle | null = null;
  const onEdited = vi.fn();
  const onSubmit = vi.fn();
  render(
    <CodexSettingsPanel
      settings={null}
      onEdited={onEdited}
      onSubmit={onSubmit}
      register={(next) => {
        handle = next;
      }}
    />,
  );
  return { handle: () => handle!, onEdited, onSubmit };
}

describe("CodexSettingsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string, args?: { path: string | null }) =>
      Promise.resolve({ root: command === "set_codex_root" ? args?.path : null, roots }),
    );
  });

  it("shows discovered local and WSL homes", async () => {
    panel();
    expect(await screen.findByText(`Reading from: ${roots[0].path} (local)`)).toBeInTheDocument();
    expect(
      screen.getByText(`Reading from: ${roots[1].path} (WSL: SampleLinux)`),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Codex Home Directory")).toHaveValue("");
  });

  it("saves a custom home and resets to auto-detection", async () => {
    const controls = panel();
    await screen.findByText(/Reading from: \/sample/);
    fireEvent.change(screen.getByLabelText("Codex Home Directory"), {
      target: { value: "  /custom/sample-codex  " },
    });
    expect(controls.onEdited).toHaveBeenCalled();
    await act(async () => controls.handle().save());
    expect(mockInvoke).toHaveBeenLastCalledWith("set_codex_root", { path: "/custom/sample-codex" });
    await act(async () => controls.handle().reset!());
    expect(mockInvoke).toHaveBeenLastCalledWith("set_codex_root", { path: null });
    expect(screen.getByLabelText("Codex Home Directory")).toHaveValue("");
  });

  it("reports loading errors and submits the modal on Enter", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("sample settings unavailable"));
    const controls = panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("sample settings unavailable");
    fireEvent.keyDown(screen.getByLabelText("Codex Home Directory"), { key: "Enter" });
    expect(controls.onSubmit).toHaveBeenCalledOnce();
  });
});
