import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AntigravityPickerSection, visibleConversations } from "./AntigravityPickerSection";
import { makeFolder } from "./testFixtures";

describe("visibleConversations", () => {
  const conversation = makeFolder({ uuid: "aaa", title: "Fix the flaky build" });
  const artifacts = makeFolder({ uuid: "bbb", title: "screenshots", has_transcript: false });
  const other = makeFolder({ uuid: "ccc", title: null, flavor: "antigravity" });

  it("leaves out folders that hold no conversation", () => {
    expect(visibleConversations([conversation, artifacts, other], "")).toEqual([
      conversation,
      other,
    ]);
  });

  it("matches the search text against title, id and flavour", () => {
    expect(visibleConversations([conversation, other], "FLAKY")).toEqual([conversation]);
    expect(visibleConversations([conversation, other], "ccc")).toEqual([other]);
    expect(visibleConversations([conversation, other], "antigravity-ide")).toEqual([conversation]);
  });
});

function renderSection(props: Partial<Parameters<typeof AntigravityPickerSection>[0]> = {}) {
  return render(
    <AntigravityPickerSection
      conversations={[makeFolder(), makeFolder({ uuid: "zzz", path: "/b", title: null })]}
      loading={false}
      searchQuery=""
      selectedIndex={null}
      onHover={vi.fn()}
      onOpen={vi.fn()}
      {...props}
    />,
  );
}

describe("AntigravityPickerSection", () => {
  it("shows a header with the count, then one card per conversation", () => {
    renderSection();
    expect(screen.getByText("Antigravity")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("Fix the flaky build")).toBeInTheDocument();
    expect(screen.getByText("Untitled conversation")).toBeInTheDocument();
  });

  it("shows when the conversation was last touched", () => {
    renderSection();
    expect(screen.getAllByText(/^\d{4}-01-02 \d{2}:00:00$/)).toHaveLength(2);
  });

  it("opens a conversation from its card or its Detail button", () => {
    const onOpen = vi.fn();
    renderSection({ onOpen });
    fireEvent.click(screen.getByText("Fix the flaky build"));
    fireEvent.click(screen.getAllByRole("button", { name: /Detail/ })[1]);
    expect(onOpen.mock.calls.map(([folder]) => folder.uuid)).toEqual([makeFolder().uuid, "zzz"]);
  });

  it("reports hovers by position and marks the selected card", () => {
    const onHover = vi.fn();
    renderSection({ onHover, selectedIndex: 1 });
    fireEvent.mouseEnter(screen.getByText("Untitled conversation"));
    expect(onHover).toHaveBeenCalledWith(1);
    expect(screen.getByText("Untitled conversation").closest(".picker__session")).toHaveClass(
      "picker__session--selected",
    );
  });

  it("says why the list is empty", () => {
    renderSection({ conversations: [] });
    expect(screen.getByText("No conversations found")).toBeInTheDocument();
  });
});
