import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PreparedEfficiencyPayload } from "../types";
import { EfficiencyPrivacyModal } from "./EfficiencyPrivacyModal";

const payload: PreparedEfficiencyPayload = {
  sessionId: "session-1",
  sessionPath: "/private/session.jsonl",
  transcriptFingerprint: "fingerprint",
  payloadMode: "minimized",
  destination: {
    provider: "TypeSafe AI (Jev)",
    endpoint: "https://api.typesafe.ai/v1/systemone",
    model: "jev-latest",
  },
  input: {
    task: { firstUserMessage: "Fix login", turns: 4, durationMs: 100, totalTokens: 200 },
    actions: [
      {
        index: 1,
        tool: "Read",
        category: "read",
        summary: "~/src/auth.rs API_KEY=[REDACTED]",
        durationMs: 5,
        error: false,
        repeatedSimilarCallCount: 0,
      },
    ],
    signals: {
      toolCalls: 1,
      failedToolCalls: 0,
      repeatedToolCalls: 0,
      subagentCount: 0,
      contextGrowth: 0,
    },
    selectedExcerpts: ["user: Fix login"],
  },
};

describe("EfficiencyPrivacyModal", () => {
  it("shows the exact final payload and requires consent before confirmation", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <EfficiencyPrivacyModal
        payload={payload}
        busy={false}
        onCancel={onCancel}
        onConfirm={onConfirm}
      />,
    );
    expect(screen.getByRole("heading", { name: "Privacy Notice" })).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Your session data will leave this device" }),
    ).toBeInTheDocument();
    expect(screen.getByText("https://api.typesafe.ai/v1/systemone")).toBeInTheDocument();
    expect(screen.getByText("jev-latest")).toBeInTheDocument();
    expect(screen.getByText(/hosted in the United States/i)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Important disclaimer" })).toBeInTheDocument();
    expect(screen.getByText(/without warranties of any kind/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Privacy Policy" })).toHaveAttribute(
      "href",
      "https://typesafe.ai/legal/privacy-policy",
    );
    fireEvent.click(screen.getByText("Review the exact data being sent"));
    expect(screen.getByText(/API_KEY=\[REDACTED\]/)).toBeInTheDocument();
    expect(screen.queryByText(/Don't show this again/i)).not.toBeInTheDocument();
    expect(screen.getByText(/required every time/i)).toBeInTheDocument();

    const continueButton = screen.getByRole("button", { name: "Continue & Analyse" });
    expect(continueButton).toBeDisabled();
    fireEvent.click(continueButton);
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(
      screen.getByRole("checkbox", {
        name: /I have reviewed the exact data and destination/i,
      }),
    );
    expect(continueButton).toBeEnabled();
    fireEvent.click(continueButton);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("starts every new modal without privacy consent", () => {
    const onConfirm = vi.fn();
    const renderModal = () =>
      render(
        <EfficiencyPrivacyModal
          payload={payload}
          busy={false}
          onCancel={vi.fn()}
          onConfirm={onConfirm}
        />,
      );

    const first = renderModal();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("button", { name: "Continue & Analyse" })).toBeEnabled();
    first.unmount();

    renderModal();
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Continue & Analyse" })).toBeDisabled();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("allows cancellation without accepting external analysis", () => {
    const onCancel = vi.fn();
    render(
      <EfficiencyPrivacyModal
        payload={payload}
        busy={false}
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
