import { describe, it, expect } from "vitest";
import { PLUGINS, isPluginEnabled } from "./registry";
import type { SourcePlugin } from "./types";

const optional = { id: "optional", canDisable: true } as SourcePlugin;
const required = { id: "required", canDisable: false } as SourcePlugin;

describe("plugin registry", () => {
  it("registers each plugin once", () => {
    const ids = PLUGINS.map((plugin) => plugin.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("registers Claude, which cannot be turned off", () => {
    const claude = PLUGINS.find((plugin) => plugin.id === "claude");
    expect(claude?.canDisable).toBe(false);
  });
});

describe("isPluginEnabled", () => {
  it("keeps a plugin that cannot be disabled on, whatever the settings say", () => {
    expect(isPluginEnabled(required, undefined)).toBe(true);
    expect(
      isPluginEnabled(required, [
        { id: "required", label: "", enabled: false, can_disable: false },
      ]),
    ).toBe(true);
  });

  it("keeps an optional plugin off until the settings turn it on", () => {
    expect(isPluginEnabled(optional, undefined)).toBe(false);
    expect(isPluginEnabled(optional, [])).toBe(false);
    expect(
      isPluginEnabled(optional, [{ id: "optional", label: "", enabled: true, can_disable: true }]),
    ).toBe(true);
  });
});
