import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { CodexHeaderStats } from "./CodexHeaderStats";
import { makeSession } from "./testFixtures";

describe("CodexHeaderStats", () => {
  it("shows token and estimated dollar totals for the supplied sessions", () => {
    render(
      <CodexHeaderStats
        sessions={[
          makeSession({ cost_usd: 1.25, total_tokens: 1000 }),
          makeSession({ id: "another", cost_usd: 2.5, total_tokens: 2000 }),
        ]}
      />,
    );
    expect(screen.getByText("3.0k tok")).toBeInTheDocument();
    expect(screen.getByLabelText("Estimated Codex API token cost")).toHaveTextContent("$3.75");
    expect(screen.getByLabelText("Estimated Codex API token cost")).not.toHaveTextContent("+");
  });

  it("marks a subtotal when some sessions have unavailable costs", () => {
    render(
      <CodexHeaderStats
        sessions={[makeSession({ cost_usd: 1.25 }), makeSession({ cost_usd: null })]}
      />,
    );
    const cost = screen.getByLabelText("Estimated Codex API token cost");
    expect(cost).toHaveTextContent("$1.25+");
    expect(cost).toHaveAttribute("title", expect.stringContaining("1 of 2 chats"));
  });

  it("shows unavailable rather than a zero total for unpriced sessions", () => {
    render(<CodexHeaderStats sessions={[makeSession({ cost_usd: null })]} />);
    expect(screen.getByLabelText("Codex API token cost unavailable")).toHaveTextContent(
      "Cost unavailable",
    );
    expect(screen.queryByText(/\$0\.00/)).toBeNull();
  });

  it("marks known subtotals when every chat is priced but worker costs are incomplete", () => {
    render(
      <CodexHeaderStats
        sessions={[
          makeSession({ cost_usd: 1.25, cost_partial: true }),
          makeSession({ cost_usd: 2.5 }),
        ]}
      />,
    );
    expect(screen.getByLabelText("Estimated Codex API token cost")).toHaveTextContent("$3.75+");
  });

  it("includes known zeros and does not add empty-session notices", () => {
    const rendered = render(<CodexHeaderStats sessions={[makeSession({ cost_usd: 0 })]} />);
    expect(screen.getByLabelText("Estimated Codex API token cost")).toHaveTextContent("$0.00");
    rendered.rerender(<CodexHeaderStats sessions={[]} />);
    expect(rendered.container).toBeEmptyDOMElement();
  });
});
