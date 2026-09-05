import { describe, it, expect, beforeEach } from "vitest";
import { authHeaders, getApiToken, setApiToken, withTokenQuery } from "./apiToken";

describe("apiToken", () => {
  beforeEach(() => {
    setApiToken(null);
  });

  it("starts empty under vitest (no VITE_API_TOKEN injected)", () => {
    expect(getApiToken()).toBeNull();
    expect(authHeaders()).toEqual({});
    expect(withTokenQuery("http://x/api/events")).toBe("http://x/api/events");
  });

  it("setApiToken makes the header and query carriers active", () => {
    setApiToken("abc123");
    expect(getApiToken()).toBe("abc123");
    expect(authHeaders()).toEqual({ "X-CCTrace-Token": "abc123" });
    expect(withTokenQuery("http://x/api/events")).toBe("http://x/api/events?token=abc123");
  });

  it("withTokenQuery appends with & when the URL already has a query", () => {
    setApiToken("t");
    expect(withTokenQuery("http://x/api/session/meta?path=%2Fa")).toBe(
      "http://x/api/session/meta?path=%2Fa&token=t",
    );
  });

  it("withTokenQuery URL-encodes the token", () => {
    setApiToken("a b&c");
    expect(withTokenQuery("/api/events")).toBe("/api/events?token=a%20b%26c");
  });

  it("setApiToken treats empty string and undefined as clearing", () => {
    setApiToken("x");
    setApiToken("");
    expect(getApiToken()).toBeNull();
    setApiToken("y");
    setApiToken(undefined);
    expect(getApiToken()).toBeNull();
  });
});
