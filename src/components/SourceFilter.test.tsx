import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SourceFilter } from "./SourceFilter";

const SOURCES = [
  { id: "claude", label: "Claude Code" },
  { id: "antigravity", label: "Antigravity" },
];

describe("SourceFilter", () => {
  it("is hidden when only one source is on", () => {
    const { container } = render(
      <SourceFilter sources={SOURCES.slice(0, 1)} value="all" onChange={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("offers All plus each source, marks the current one, and reports a change", () => {
    const onChange = vi.fn();
    render(<SourceFilter sources={SOURCES} value="claude" onChange={onChange} />);

    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual([
      "All",
      "Claude Code",
      "Antigravity",
    ]);
    expect(screen.getByRole("button", { name: "Claude Code" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "Antigravity" }));
    expect(onChange).toHaveBeenCalledWith("antigravity");
  });
});
