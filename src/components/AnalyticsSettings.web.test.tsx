import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AnalyticsSettings } from "./AnalyticsSettings";

const mockInvoke = vi.fn();
vi.mock("../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
vi.mock("../lib/isTauri", () => ({ isTauri: false }));

const webSettings = {
  jev: { configured: false, source: null, status: "not_configured" },
  defaultPayloadMode: "minimized",
  recommendationProvider: { type: "codex-subscription", model: null },
  subscriptionProvidersAvailable: false,
};

describe("AnalyticsSettings in a browser", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(webSettings);
  });

  it("never renders fields that could send API tokens over HTTP", async () => {
    render(<AnalyticsSettings />);

    await waitFor(() =>
      expect(screen.getByText("API tokens cannot be entered in the browser")).toBeInTheDocument(),
    );

    expect(screen.queryByLabelText("Jev API key")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("API key (optional)")).not.toBeInTheDocument();
    expect(screen.getAllByRole("note")[0]).toHaveTextContent("JEV_API_KEY");
    expect(screen.getAllByRole("note")[0]).toHaveTextContent(
      "local, unauthenticated OpenAI-compatible endpoint",
    );
    expect(screen.getByLabelText("Provider")).toHaveValue("openai-compatible");
    expect(screen.queryByRole("option", { name: "Codex Subscription" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "Claude Code Subscription" }),
    ).not.toBeInTheDocument();
    expect(screen.getAllByRole("note")[1]).toHaveTextContent(
      "Subscription providers are unavailable in Docker",
    );
    expect(screen.getAllByRole("note")[2]).toHaveTextContent(
      "API keys, URL credentials, and token query parameters are not accepted",
    );
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("get_analytics_settings");
  });
});
