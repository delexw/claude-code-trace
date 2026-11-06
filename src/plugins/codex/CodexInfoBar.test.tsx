import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { CodexInfoBar } from "./CodexInfoBar";
import { makeSession, makeWindow } from "./testFixtures";

describe("CodexInfoBar cost", () => {
  it("uses the open conversation's latest estimated cost", () => {
    render(
      <CodexInfoBar
        session={makeSession({ cost_usd: 1 })}
        window={makeWindow({ cost_usd: 2.5 })}
      />,
    );
    const cost = screen.getByLabelText("Estimated API token cost");
    expect(cost).toHaveTextContent("$2.50");
    expect(cost).toHaveClass("info-bar__cost");
    expect(cost).toHaveAttribute("title", expect.stringContaining("Subscription"));
  });

  it("displays a known zero without treating it as unavailable", () => {
    render(<CodexInfoBar session={makeSession()} window={makeWindow({ cost_usd: 0 })} />);
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$0.00");
    expect(screen.queryByText("Cost unavailable")).toBeNull();
  });

  it("keeps unknown costs distinct from zero dollars", () => {
    render(<CodexInfoBar session={makeSession()} window={makeWindow({ cost_usd: null })} />);
    expect(screen.getByLabelText("API token cost unavailable")).toHaveTextContent(
      "Cost unavailable",
    );
    expect(screen.queryByText(/\$0\.00/)).toBeNull();
  });
});
