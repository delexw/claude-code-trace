import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { OperationResultModal } from "./OperationResultModal";

describe("OperationResultModal", () => {
  it.each([
    ["success", "Provider connected", "Success"],
    ["error", "Codex could not start", "Action failed"],
  ] as const)("shows a visible %s result", (kind, message, title) => {
    render(<OperationResultModal kind={kind} message={message} onClose={() => {}} />);

    expect(screen.getByRole("alertdialog", { name: title })).toHaveTextContent(message);
    expect(screen.getByRole("button", { name: "OK" })).toHaveFocus();
  });

  it("closes from the button or surrounding overlay without propagating", () => {
    const onClose = vi.fn();
    const outerClick = vi.fn();
    const { container } = render(
      <div onClick={outerClick}>
        <OperationResultModal kind="success" message="Saved" onClose={onClose} />
      </div>,
    );

    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(outerClick).not.toHaveBeenCalled();

    fireEvent.click(container.querySelector(".operation-result-overlay")!);
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(outerClick).not.toHaveBeenCalled();
  });
});
