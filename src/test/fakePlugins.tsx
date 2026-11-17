import { vi } from "vitest";
import { useState } from "react";
import type { SourcePlugin } from "../plugins/types";

/** A minimal source plugin for shell tests: a few picker items and one view. */
export function fakePlugin(id: string, items: string[], canDisable: boolean) {
  const opened = vi.fn<(localIndex: number) => void>();
  const focused = vi.fn();
  const plugin: SourcePlugin = {
    id,
    label: id,
    canDisable,
    useSource: (ctx) => {
      const [sidebarFocused, setSidebarFocused] = useState(false);
      return {
        sidebar: {
          focused: sidebarFocused,
          focus: () => {
            focused();
            setSidebarFocused(true);
          },
          blur: () => setSidebarFocused(false),
          keyMap: { l: () => setSidebarFocused(false) },
          render: (style) => (
            <div
              style={style}
              onClick={() => {
                ctx.setPickerIndex(0);
                ctx.goToPicker();
              }}
            >
              {id} projects
            </div>
          ),
        },
        picker: {
          count: items.length,
          open: (localIndex) => {
            opened(localIndex);
            ctx.openView("main");
          },
          headerStats: <span>{id} stats</span>,
          render: ({ selectedIndex, onHover }) => (
            <>
              {items.map((item, i) => (
                <div
                  key={item}
                  data-testid={`${id}-item`}
                  data-selected={i === selectedIndex}
                  onMouseEnter={() => onHover(i)}
                >
                  {item}
                </div>
              ))}
            </>
          ),
        },
        hasOpenSession: true,
        backToSession: () => ctx.openView("main"),
        views: {
          main: {
            render: () => <div>{id} view</div>,
            infoBar: <div>{id} info</div>,
            toolbarLeft: <button className="view-toolbar__btn">{id} nav</button>,
            collapsible: false,
            keyMap: { q: ctx.goToPicker },
            hints: [{ key: "x", label: `${id} hint` }],
          },
        },
      };
    },
    SettingsPanel: () => <p>{id} settings</p>,
  };
  return { plugin, opened, focused };
}

export const alpha = fakePlugin("alpha", ["alpha one", "alpha two"], false);
export const beta = fakePlugin("beta", ["beta one"], true);
