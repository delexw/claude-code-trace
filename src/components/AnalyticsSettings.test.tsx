import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AnalyticsSettings } from "./AnalyticsSettings";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const settings = {
  jev: { configured: false, source: null, status: "not_configured" },
  defaultPayloadMode: "minimized",
  recommendationProvider: { type: "codex-subscription", model: null },
};

describe("AnalyticsSettings", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(settings);
  });

  it("never displays a stored Jev key and defaults recommendations to Codex", async () => {
    render(<AnalyticsSettings />);
    await waitFor(() => expect(screen.getByLabelText("Jev API key")).toBeInTheDocument());
    expect(screen.getByLabelText("Jev API key")).toHaveValue("");
    expect(screen.getByLabelText("Provider")).toHaveValue("codex-subscription");
    expect(screen.getByText(/Native Codex subscription/)).toBeInTheDocument();
  });

  it("sends a newly entered key only to the secure-storage command", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_analytics_settings") return Promise.resolve(settings);
      if (command === "set_jev_api_key") {
        return Promise.resolve({
          ...settings,
          jev: { configured: true, source: "secure-storage", status: "configured" },
        });
      }
      return Promise.resolve();
    });
    render(<AnalyticsSettings />);
    const input = await screen.findByLabelText("Jev API key");
    fireEvent.change(input, { target: { value: "jev-test-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("set_jev_api_key", { key: "jev-test-secret" }),
    );
    expect(input).toHaveValue("");
  });
});
