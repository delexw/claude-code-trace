import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { KeybindBar } from "./KeybindBar";

/** A finished walk: the progress box is empty, as it is for every test not about it. */
const indexed = {
  files_read: 1,
  total_files: 1,
  bytes_read: 1,
  total_bytes: 1,
  done: true,
};

describe("KeybindBar", () => {
  it("renders correct keybinds for picker view", () => {
    render(<KeybindBar index={indexed} view="picker" hasTeams={false} />);
    expect(screen.getByText("nav")).toBeInTheDocument();
    expect(screen.getByText("open")).toBeInTheDocument();
    expect(screen.getByText("search")).toBeInTheDocument();
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for list view", () => {
    render(<KeybindBar index={indexed} view="list" hasTeams={false} />);
    expect(screen.getByText("nav")).toBeInTheDocument();
    expect(screen.getByText("scroll")).toBeInTheDocument();
    expect(screen.getByText("jump")).toBeInTheDocument();
    expect(screen.getByText("toggle")).toBeInTheDocument();
    expect(screen.getByText("detail")).toBeInTheDocument();
    expect(screen.getByText("debug")).toBeInTheDocument();
    expect(screen.getByText("expand/collapse")).toBeInTheDocument();
    expect(screen.getByText("sessions")).toBeInTheDocument();
    expect(screen.queryByText("tasks")).not.toBeInTheDocument();
  });

  it("list view shows tasks keybind when hasTeams=true", () => {
    render(<KeybindBar index={indexed} view="list" hasTeams={true} />);
    expect(screen.getByText("tasks")).toBeInTheDocument();
  });

  it("renders correct keybinds for detail view", () => {
    render(<KeybindBar index={indexed} view="detail" hasTeams={false} />);
    expect(screen.getByText("items")).toBeInTheDocument();
    expect(screen.getByText("toggle")).toBeInTheDocument();
    expect(screen.getByText("open")).toBeInTheDocument();
    expect(screen.getByText("panels")).toBeInTheDocument();
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for debug view", () => {
    render(<KeybindBar index={indexed} view="debug" hasTeams={false} />);
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for team view", () => {
    render(<KeybindBar index={indexed} view="team" hasTeams={false} />);
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("hides hints when showHints=false", () => {
    render(<KeybindBar index={indexed} view="list" hasTeams={false} showHints={false} />);
    expect(screen.queryByText("nav")).not.toBeInTheDocument();
    expect(screen.queryByText("scroll")).not.toBeInTheDocument();
  });

  it("shows toggle button when onToggle provided", () => {
    const onToggle = vi.fn();
    render(<KeybindBar index={indexed} view="list" hasTeams={false} onToggle={onToggle} />);
    expect(screen.getByTitle("Hide keybinds")).toBeInTheDocument();
  });

  it("does not show toggle button when onToggle not provided", () => {
    render(<KeybindBar index={indexed} view="list" hasTeams={false} />);
    expect(screen.queryByTitle("Hide keybinds")).not.toBeInTheDocument();
  });

  it("clicking toggle calls onToggle", () => {
    const onToggle = vi.fn();
    render(<KeybindBar index={indexed} view="list" hasTeams={false} onToggle={onToggle} />);
    fireEvent.click(screen.getByTitle("Hide keybinds"));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("clickable items call their action", () => {
    const action = vi.fn();
    render(<KeybindBar index={indexed} view="list" hasTeams={false} actions={{ debug: action }} />);
    fireEvent.click(screen.getByText("debug").closest(".keybind-bar__item")!);
    expect(action).toHaveBeenCalledTimes(1);
  });

  it("clickable items have clickable class", () => {
    const action = vi.fn();
    render(<KeybindBar index={indexed} view="list" hasTeams={false} actions={{ debug: action }} />);
    const item = screen.getByText("debug").closest(".keybind-bar__item")!;
    expect(item).toHaveClass("keybind-bar__item--clickable");
  });

  it("non-clickable items do not have clickable class", () => {
    render(<KeybindBar index={indexed} view="list" hasTeams={false} />);
    const item = screen.getByText("nav").closest(".keybind-bar__item")!;
    expect(item).not.toHaveClass("keybind-bar__item--clickable");
  });
});

describe("KeybindBar indexing progress", () => {
  const walking = {
    files_read: 120,
    total_files: 3375,
    bytes_read: 3_700_000,
    total_bytes: 10_000_000,
    done: false,
  };

  it("shows how far the walk has got, in the strip along the bottom", () => {
    render(<KeybindBar index={walking} view="picker" hasTeams={false} />);

    const strip = document.querySelector(".keybind-bar") as HTMLElement;
    expect(strip.querySelector(".index-progress")).toBeInTheDocument();
    expect(screen.getByText("120 sessions · 3.7 MB / 10.0 MB")).toBeInTheDocument();
  });

  it("keeps the box once the walk has finished, so the strip does not change height", () => {
    const { container, rerender } = render(
      <KeybindBar index={walking} view="picker" hasTeams={false} />,
    );
    const boxesWhileWalking = container.querySelectorAll(".index-progress").length;

    rerender(<KeybindBar index={{ ...walking, done: true }} view="picker" hasTeams={false} />);

    // A box that came and went would shift every row above it each time a walk started.
    expect(container.querySelectorAll(".index-progress")).toHaveLength(boxesWhileWalking);
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("still shows the keybinds while a walk is running", () => {
    render(<KeybindBar index={walking} view="picker" hasTeams={false} />);

    expect(screen.getByText("nav")).toBeInTheDocument();
  });
});
