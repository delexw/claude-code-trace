import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ViewToolbar } from "./ViewToolbar";

function defaultProps(overrides: Partial<Parameters<typeof ViewToolbar>[0]> = {}) {
  return {
    view: "list" as const,
    hasTeams: false,
    hasSession: false,
    messageCount: 5,
    onGoToSessions: vi.fn(),
    onExpandAll: vi.fn(),
    onCollapseAll: vi.fn(),
    onJumpTop: vi.fn(),
    onJumpBottom: vi.fn(),
    onOpenTeams: vi.fn(),
    onOpenDebug: vi.fn(),
    onBackToList: vi.fn(),
    onOpenSettings: vi.fn(),
    ...overrides,
  };
}

describe("ViewToolbar", () => {
  describe("list view", () => {
    it("shows all expected buttons", () => {
      render(<ViewToolbar {...defaultProps()} />);
      expect(screen.getByText(/Sessions/)).toBeInTheDocument();
      expect(screen.getByText("Expand All")).toBeInTheDocument();
      expect(screen.getByText("Collapse All")).toBeInTheDocument();
      expect(screen.getByText("Top")).toBeInTheDocument();
      expect(screen.getByText("Bottom")).toBeInTheDocument();
      expect(screen.getByText("Debug")).toBeInTheDocument();
    });

    it("shows Teams button when hasTeams=true", () => {
      render(<ViewToolbar {...defaultProps({ hasTeams: true })} />);
      expect(screen.getByText("Teams")).toBeInTheDocument();
    });

    it("hides Teams button when hasTeams=false", () => {
      render(<ViewToolbar {...defaultProps({ hasTeams: false })} />);
      expect(screen.queryByText("Teams")).not.toBeInTheDocument();
    });

    it("disables Top/Bottom when messageCount=0", () => {
      render(<ViewToolbar {...defaultProps({ messageCount: 0 })} />);
      expect(screen.getByText("Top")).toBeDisabled();
      expect(screen.getByText("Bottom")).toBeDisabled();
    });

    it("enables Top/Bottom when messageCount > 0", () => {
      render(<ViewToolbar {...defaultProps({ messageCount: 3 })} />);
      expect(screen.getByText("Top")).not.toBeDisabled();
      expect(screen.getByText("Bottom")).not.toBeDisabled();
    });

    it("calls correct callbacks when buttons clicked", () => {
      const props = defaultProps({ hasTeams: true });
      render(<ViewToolbar {...props} />);

      fireEvent.click(screen.getByText(/Sessions/));
      expect(props.onGoToSessions).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Expand All"));
      expect(props.onExpandAll).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Collapse All"));
      expect(props.onCollapseAll).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Top"));
      expect(props.onJumpTop).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Bottom"));
      expect(props.onJumpBottom).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Teams"));
      expect(props.onOpenTeams).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Debug"));
      expect(props.onOpenDebug).toHaveBeenCalled();
    });
  });

  describe("picker view", () => {
    it("shows Back to Messages when hasSession=true", () => {
      render(<ViewToolbar {...defaultProps({ view: "picker", hasSession: true })} />);
      expect(screen.getByText(/Back to Messages/)).toBeInTheDocument();
    });

    it("calls onBackToList when Back to Messages clicked", () => {
      const props = defaultProps({ view: "picker", hasSession: true });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByText(/Back to Messages/));
      expect(props.onBackToList).toHaveBeenCalled();
    });

    it("shows settings button even when hasSession=false", () => {
      render(<ViewToolbar {...defaultProps({ view: "picker", hasSession: false })} />);
      expect(screen.getByTitle("Settings")).toBeInTheDocument();
    });

    it("calls onOpenSettings when settings button clicked", () => {
      const props = defaultProps({ view: "picker", hasSession: false });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByTitle("Settings"));
      expect(props.onOpenSettings).toHaveBeenCalled();
    });
  });

  describe("detail/team/debug views show back + settings", () => {
    for (const view of ["detail", "team", "debug"] as const) {
      it(`${view} view shows Back to Messages and Settings`, () => {
        render(<ViewToolbar {...defaultProps({ view })} />);
        expect(screen.getByText(/Back to Messages/)).toBeInTheDocument();
        expect(screen.getByTitle("Settings")).toBeInTheDocument();
      });
    }

    it("calls onBackToList when back clicked in detail view", () => {
      const props = defaultProps({ view: "detail" });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByText(/Back to Messages/));
      expect(props.onBackToList).toHaveBeenCalled();
    });
  });
});
