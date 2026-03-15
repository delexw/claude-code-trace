import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { KeybindBar } from "./KeybindBar";

describe("KeybindBar", () => {
  it("renders correct keybinds for picker view", () => {
    render(<KeybindBar view="picker" hasTeams={false} />);
    expect(screen.getByText("nav")).toBeInTheDocument();
    expect(screen.getByText("open")).toBeInTheDocument();
    expect(screen.getByText("search")).toBeInTheDocument();
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for list view", () => {
    render(<KeybindBar view="list" hasTeams={false} />);
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
    render(<KeybindBar view="list" hasTeams={true} />);
    expect(screen.getByText("tasks")).toBeInTheDocument();
  });

  it("renders correct keybinds for detail view", () => {
    render(<KeybindBar view="detail" hasTeams={false} />);
    expect(screen.getByText("items")).toBeInTheDocument();
    expect(screen.getByText("toggle")).toBeInTheDocument();
    expect(screen.getByText("open")).toBeInTheDocument();
    expect(screen.getByText("panels")).toBeInTheDocument();
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for debug view", () => {
    render(<KeybindBar view="debug" hasTeams={false} />);
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("renders correct keybinds for team view", () => {
    render(<KeybindBar view="team" hasTeams={false} />);
    expect(screen.getByText("back")).toBeInTheDocument();
  });

  it("hides hints when showHints=false", () => {
    render(<KeybindBar view="list" hasTeams={false} showHints={false} />);
    expect(screen.queryByText("nav")).not.toBeInTheDocument();
    expect(screen.queryByText("scroll")).not.toBeInTheDocument();
  });

  it("shows toggle button when onToggle provided", () => {
    const onToggle = vi.fn();
    render(<KeybindBar view="list" hasTeams={false} onToggle={onToggle} />);
    expect(screen.getByTitle("Hide keybinds")).toBeInTheDocument();
  });

  it("does not show toggle button when onToggle not provided", () => {
    render(<KeybindBar view="list" hasTeams={false} />);
    expect(screen.queryByTitle("Hide keybinds")).not.toBeInTheDocument();
  });

  it("clicking toggle calls onToggle", () => {
    const onToggle = vi.fn();
    render(<KeybindBar view="list" hasTeams={false} onToggle={onToggle} />);
    fireEvent.click(screen.getByTitle("Hide keybinds"));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("clickable items call their action", () => {
    const action = vi.fn();
    render(<KeybindBar view="list" hasTeams={false} actions={{ debug: action }} />);
    fireEvent.click(screen.getByText("debug").closest(".keybind-bar__item")!);
    expect(action).toHaveBeenCalledTimes(1);
  });

  it("clickable items have clickable class", () => {
    const action = vi.fn();
    render(<KeybindBar view="list" hasTeams={false} actions={{ debug: action }} />);
    const item = screen.getByText("debug").closest(".keybind-bar__item")!;
    expect(item).toHaveClass("keybind-bar__item--clickable");
  });

  it("non-clickable items do not have clickable class", () => {
    render(<KeybindBar view="list" hasTeams={false} />);
    const item = screen.getByText("nav").closest(".keybind-bar__item")!;
    expect(item).not.toHaveClass("keybind-bar__item--clickable");
  });
});
