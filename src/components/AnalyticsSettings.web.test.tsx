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
  recommendationProvider: {
    type: "openai-compatible",
    baseUrl: "http://localhost:1234/v1",
    model: "example-model",
    apiKeyConfigured: false,
  },
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
    expect(screen.getByRole("note")).toHaveTextContent("JEV_API_KEY");
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("get_analytics_settings");
  });
});
