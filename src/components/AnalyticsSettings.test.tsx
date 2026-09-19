import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AnalyticsSettings } from "./AnalyticsSettings";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
vi.mock("../lib/isTauri", () => ({ isTauri: true }));

const settings = {
  jev: { configured: false, source: null, status: "not_configured" },
  defaultPayloadMode: "minimized",
  recommendationProvider: { type: "codex-subscription", model: null },
  subscriptionProvidersAvailable: true,
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

  it("sends a newly entered key only through native IPC", async () => {
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
    expect(screen.getByRole("alertdialog", { name: "Success" })).toHaveTextContent(
      "Jev API key stored in the platform credential store.",
    );
  });

  it("shows provider test errors and successes in a result pop-up", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_analytics_settings") return Promise.resolve(settings);
      if (command === "test_recommendation_provider") {
        return Promise.reject(new Error("Codex could not start"));
      }
      return Promise.resolve();
    });
    render(<AnalyticsSettings />);

    fireEvent.click(await screen.findByRole("button", { name: "Test recommendation provider" }));
    expect(await screen.findByRole("alertdialog", { name: "Action failed" })).toHaveTextContent(
      "Codex could not start",
    );
    expect(screen.queryByText("Error: Codex could not start")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    mockInvoke.mockResolvedValue(undefined);
    fireEvent.click(screen.getByRole("button", { name: "Test recommendation provider" }));
    expect(await screen.findByRole("alertdialog", { name: "Success" })).toHaveTextContent(
      "Recommendation provider connected.",
    );
  });
});
