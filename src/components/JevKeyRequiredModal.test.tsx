import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { JevKeyRequiredModal } from "./JevKeyRequiredModal";

describe("JevKeyRequiredModal", () => {
  it("lets the user cancel or open Analytics settings", () => {
    const onCancel = vi.fn();
    const onOpenSettings = vi.fn();
    render(<JevKeyRequiredModal onCancel={onCancel} onOpenSettings={onOpenSettings} />);

    expect(screen.getByText("Jev API key required")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Open Analytics Settings" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
  });
});
