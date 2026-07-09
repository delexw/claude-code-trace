import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ViewToolbar } from "./ViewToolbar";

/** Rendered toolbar button labels, in DOM order (icons/settings have no text). */
function buttonOrder() {
  return [...document.querySelectorAll(".view-toolbar__btn")]
    .map((b) => b.textContent?.trim())
    .filter(Boolean);
}

function defaultProps(overrides: Partial<Parameters<typeof ViewToolbar>[0]> = {}) {
  return {
    view: "list" as const,
    hasTeams: false,
    hasSession: false,
    onGoToSessions: vi.fn(),
    onExpandAll: vi.fn(),
    onCollapseAll: vi.fn(),
    onScrollToTop: vi.fn(),
    onScrollToBottom: vi.fn(),
    onOpenTeams: vi.fn(),
    onOpenDebug: vi.fn(),
    onBackToList: vi.fn(),
    onOpenSettings: vi.fn(),
    ...overrides,
  };
}

describe("ViewToolbar", () => {
  describe("common buttons on views with collapsible content", () => {
    // The picker has no collapsible entries, so it is handled separately below.
    for (const view of ["list", "detail", "team", "debug"] as const) {
      it(`${view} view shows Expand All, Collapse All, Top, Bottom`, () => {
        render(<ViewToolbar {...defaultProps({ view, hasSession: true })} />);
        expect(screen.getByText("Expand All")).toBeInTheDocument();
        expect(screen.getByText("Collapse All")).toBeInTheDocument();
        expect(screen.getByText("Top")).toBeInTheDocument();
        expect(screen.getByText("Bottom")).toBeInTheDocument();
        expect(screen.getByTitle("Settings")).toBeInTheDocument();
      });
    }

    it("Top and Bottom call the scroll callbacks", () => {
      const props = defaultProps({ view: "list" });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByText("Top"));
      expect(props.onScrollToTop).toHaveBeenCalled();
      fireEvent.click(screen.getByText("Bottom"));
      expect(props.onScrollToBottom).toHaveBeenCalled();
    });
  });

  describe("list view", () => {
    it("shows Sessions, Teams, Debug buttons", () => {
      render(<ViewToolbar {...defaultProps({ hasTeams: true })} />);
      expect(screen.getByText(/Sessions/)).toBeInTheDocument();
      expect(screen.getByText("Teams")).toBeInTheDocument();
      expect(screen.getByText("Debug")).toBeInTheDocument();
    });

    it("hides Teams button when hasTeams=false", () => {
      render(<ViewToolbar {...defaultProps({ hasTeams: false })} />);
      expect(screen.queryByText("Teams")).not.toBeInTheDocument();
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

      fireEvent.click(screen.getByText("Teams"));
      expect(props.onOpenTeams).toHaveBeenCalled();

      fireEvent.click(screen.getByText("Debug"));
      expect(props.onOpenDebug).toHaveBeenCalled();
    });
  });

  describe("picker view", () => {
    it("shows Top and Bottom but not the inert Expand/Collapse", () => {
      render(<ViewToolbar {...defaultProps({ view: "picker", hasSession: true })} />);
      expect(screen.getByText("Top")).toBeInTheDocument();
      expect(screen.getByText("Bottom")).toBeInTheDocument();
      expect(screen.queryByText("Expand All")).not.toBeInTheDocument();
      expect(screen.queryByText("Collapse All")).not.toBeInTheDocument();
    });

    it("Top and Bottom scroll the picker via the callbacks", () => {
      const props = defaultProps({ view: "picker", hasSession: true });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByText("Top"));
      expect(props.onScrollToTop).toHaveBeenCalled();
      fireEvent.click(screen.getByText("Bottom"));
      expect(props.onScrollToBottom).toHaveBeenCalled();
    });

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
  });

  describe("detail/team/debug views", () => {
    for (const view of ["detail", "team", "debug"] as const) {
      it(`${view} view shows Back to Messages`, () => {
        render(<ViewToolbar {...defaultProps({ view })} />);
        expect(screen.getByText(/Back to Messages/)).toBeInTheDocument();
      });
    }

    it("calls onBackToList when back clicked in detail view", () => {
      const props = defaultProps({ view: "detail" });
      render(<ViewToolbar {...props} />);
      fireEvent.click(screen.getByText(/Back to Messages/));
      expect(props.onBackToList).toHaveBeenCalled();
    });
  });

  describe("content actions live in the right cluster", () => {
    it("list view: navigation first, then content actions (Expand/Collapse/Top/Bottom)", () => {
      render(<ViewToolbar {...defaultProps({ view: "list", hasTeams: false })} />);
      expect(buttonOrder()).toEqual([
        "Sessions",
        "Debug",
        "Expand All",
        "Collapse All",
        "Top",
        "Bottom",
      ]);
    });

    it("detail view: back nav first, then content actions", () => {
      render(<ViewToolbar {...defaultProps({ view: "detail" })} />);
      expect(buttonOrder()).toEqual([
        "Back to Messages",
        "Expand All",
        "Collapse All",
        "Top",
        "Bottom",
      ]);
    });
  });
});
