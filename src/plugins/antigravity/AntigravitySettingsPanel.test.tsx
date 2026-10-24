import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import type { SettingsPanelHandle } from "../types";

const mockInvoke = vi.fn();
vi.mock("../../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import { AntigravitySettingsPanel } from "./AntigravitySettingsPanel";

const IDE = { flavor: "antigravity-ide", path: "/h/.gemini/antigravity-ide/brain", exists: true };
const OLD = { flavor: "antigravity", path: "/h/.gemini/antigravity/brain", exists: true };

function renderPanel(settings: { root: string | null; roots: (typeof IDE)[] }) {
  mockInvoke.mockImplementation((cmd: string, args?: { path: string | null }) =>
    Promise.resolve(
      cmd === "set_antigravity_root"
        ? { root: args?.path ?? null, roots: settings.roots }
        : settings,
    ),
  );
  let handle: SettingsPanelHandle | null = null;
  render(
    <AntigravitySettingsPanel
      settings={null}
      onEdited={vi.fn()}
      onSubmit={vi.fn()}
      register={(h) => {
        handle = h;
      }}
    />,
  );
  return () => handle!;
}

describe("AntigravitySettingsPanel", () => {
  beforeEach(() => vi.clearAllMocks());

  it("starts empty while the roots are auto-detected, and names the one it reads", async () => {
    renderPanel({ root: null, roots: [IDE] });
    expect(await screen.findByText(`✓ Reading from: ${IDE.path}`)).toBeInTheDocument();
    expect(screen.getByLabelText("Antigravity Brain Directory")).toHaveValue("");
  });

  it("shows the user's own directory when one is set", async () => {
    renderPanel({
      root: "/custom/brain",
      roots: [{ ...IDE, flavor: "custom", path: "/custom/brain" }],
    });
    await waitFor(() =>
      expect(screen.getByLabelText("Antigravity Brain Directory")).toHaveValue("/custom/brain"),
    );
  });

  it("names the other installs it found", async () => {
    renderPanel({ root: null, roots: [IDE, OLD] });
    expect(await screen.findByText(/Also found:/)).toHaveTextContent(
      `antigravity-ide → ${IDE.path} · antigravity → ${OLD.path}`,
    );
  });

  it("saves the typed directory, and clears it when the field is emptied", async () => {
    const handle = renderPanel({ root: null, roots: [IDE] });
    await screen.findByText(/Reading from/);
    const input = screen.getByLabelText("Antigravity Brain Directory");

    fireEvent.change(input, { target: { value: "  /custom/brain  " } });
    await waitFor(() => expect(input).toHaveValue("  /custom/brain  "));
    await handle().save();
    expect(mockInvoke).toHaveBeenCalledWith("set_antigravity_root", { path: "/custom/brain" });

    fireEvent.change(input, { target: { value: "" } });
    await waitFor(() => expect(input).toHaveValue(""));
    await handle().save();
    expect(mockInvoke).toHaveBeenLastCalledWith("set_antigravity_root", { path: null });
  });

  it("goes back to auto-detecting on reset", async () => {
    const handle = renderPanel({ root: "/custom/brain", roots: [IDE] });
    await screen.findByText(/Reading from/);
    await handle().reset!();
    expect(mockInvoke).toHaveBeenLastCalledWith("set_antigravity_root", { path: null });
  });
});
