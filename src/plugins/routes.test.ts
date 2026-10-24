import { describe, it, expect } from "vitest";
import { PLUGINS } from "./registry";
import { PLUGIN_ROUTES } from "./routes";

describe("plugin routes", () => {
  it("belong only to registered plugins", () => {
    const registered = new Set(PLUGINS.map((plugin) => plugin.id));
    for (const id of Object.keys(PLUGIN_ROUTES)) expect(registered).toContain(id);
  });

  it("never give two plugins the same command", () => {
    const commands = Object.values(PLUGIN_ROUTES).flatMap((table) => Object.keys(table));
    expect(new Set(commands).size).toBe(commands.length);
  });
});
