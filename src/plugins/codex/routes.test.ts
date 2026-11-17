import { describe, it, expect } from "vitest";
import { codexRoutes } from "./routes";

describe("Codex routes", () => {
  it("preserves bounded-window arguments and clears an omitted root", () => {
    expect(codexRoutes.load_codex_session.path).toBe("/api/codex/session");
    expect(codexRoutes.load_codex_session.method).toBe("POST");
    expect(
      codexRoutes.load_codex_session.body!({ path: "/sample/chat.jsonl", start: 12, limit: 0 }),
    ).toEqual({ path: "/sample/chat.jsonl", start: 12, limit: 0 });
    expect(codexRoutes.set_codex_root.body!({})).toEqual({ path: null });
  });

  it("prepares Codex efficiency data through its own source adapter", () => {
    expect(codexRoutes.prepare_codex_efficiency_payload.path).toBe("/api/codex/efficiency/prepare");
    expect(
      codexRoutes.prepare_codex_efficiency_payload.body!({
        path: "/invented/chat.jsonl",
        payloadMode: "minimized",
      }),
    ).toEqual({ path: "/invented/chat.jsonl", payloadMode: "minimized" });
  });
});
