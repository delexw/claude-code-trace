import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

/**
 * Stand-in for the browser's `EventSource` with the three things a real one
 * does that `listen.ts` has to cope with: open a stream (200), *fail the
 * connection* on a non-200 reply (`readyState` CLOSED, one `error`, no retry —
 * the 401 case), and retry a network failure by itself (`readyState`
 * CONNECTING, one `error` per attempt).
 */
class FakeEventSource {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;
  static instances: FakeEventSource[] = [];

  readyState = FakeEventSource.CONNECTING;
  closed = false;
  private listeners = new Map<string, Set<EventListener>>();

  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, fn: EventListener) {
    let set = this.listeners.get(type);
    if (!set) {
      set = new Set();
      this.listeners.set(type, set);
    }
    set.add(fn);
  }

  removeEventListener(type: string, fn: EventListener) {
    this.listeners.get(type)?.delete(fn);
  }

  close() {
    this.closed = true;
    this.readyState = FakeEventSource.CLOSED;
  }

  /** The server answered `200 text/event-stream`. */
  open() {
    this.readyState = FakeEventSource.OPEN;
    this.dispatch("open");
  }

  /** The server answered anything else (e.g. 401): the browser gives up for good. */
  fail() {
    this.readyState = FakeEventSource.CLOSED;
    this.dispatch("error");
  }

  /** The connection dropped: the browser reconnects on its own. */
  drop() {
    this.readyState = FakeEventSource.CONNECTING;
    this.dispatch("error");
  }

  message(event: string, data: string) {
    this.dispatch(event, new MessageEvent(event, { data }));
  }

  /** Named events with at least one handler attached — the lifecycle listeners
   * `listen.ts` adds for itself (`open`, `error`) are not part of the contract. */
  eventNames(): string[] {
    return [...this.listeners]
      .filter(([type, set]) => type !== "open" && type !== "error" && set.size > 0)
      .map(([type]) => type);
  }

  private dispatch(type: string, ev: Event = new Event(type)) {
    for (const fn of this.listeners.get(type) ?? []) fn.call(this, ev);
  }
}

const urls = () => FakeEventSource.instances.map((s) => s.url);
const latest = () => FakeEventSource.instances.at(-1)!;

describe("listen (web/SSE mode)", () => {
  beforeEach(async () => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
    // Reset module-level SSE state by clearing the module cache.
    vi.resetModules();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
    const { setApiToken } = await import("./apiToken");
    setApiToken(null);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("creates an EventSource and registers event listener", async () => {
    const { listen } = await import("./listen");
    const unlisten = await listen("session-update", vi.fn());

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(latest().eventNames()).toEqual(["session-update"]);
    expect(typeof unlisten).toBe("function");
  });

  it("hands each parsed payload to the handler and ignores malformed data", async () => {
    const { listen } = await import("./listen");
    const handler = vi.fn();
    await listen("session-update", handler);

    latest().message("session-update", '{"count":3}');
    latest().message("session-update", "not json");
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith({ payload: { count: 3 } });
  });

  it("unlisten removes the event listener and the last one closes the stream", async () => {
    const { listen } = await import("./listen");
    const unlistenA = await listen("test-event", () => {});
    const unlistenB = await listen("other-event", () => {});
    const source = latest();
    expect(source.eventNames()).toEqual(["test-event", "other-event"]);

    unlistenA();
    expect(source.eventNames()).toEqual(["other-event"]);
    expect(source.closed).toBe(false);

    unlistenB();
    expect(source.eventNames()).toEqual([]);
    expect(source.closed).toBe(true);
  });

  it("connects to /api/events without a token when none is set", async () => {
    const { listen } = await import("./listen");
    await listen("session-update", () => {});
    expect(urls()).toEqual(["http://127.0.0.1:11423/api/events"]);
  });

  it("carries the API token in the query string (EventSource cannot set headers)", async () => {
    const { setApiToken } = await import("./apiToken");
    setApiToken("tok123");
    const { listen } = await import("./listen");
    await listen("session-update", () => {});
    expect(urls()).toEqual(["http://127.0.0.1:11423/api/events?token=tok123"]);
  });

  it("reconnectSse closes the stream, reopens with the current token, and re-attaches listeners", async () => {
    const { setApiToken } = await import("./apiToken");
    const { listen, reconnectSse } = await import("./listen");
    setApiToken("tok");
    await listen("session-update", () => {});
    await listen("picker-refresh", () => {});
    const first = latest();
    expect(first.eventNames()).toEqual(["session-update", "picker-refresh"]);

    reconnectSse();

    expect(first.closed).toBe(true);
    expect(urls()).toEqual([
      "http://127.0.0.1:11423/api/events?token=tok",
      "http://127.0.0.1:11423/api/events?token=tok",
    ]);
    // Both listeners were re-registered on the replacement connection.
    expect(latest().eventNames()).toEqual(["session-update", "picker-refresh"]);
  });

  it("reconnectSse is a no-op when nothing is listening", async () => {
    const { reconnectSse } = await import("./listen");
    reconnectSse();
    expect(urls()).toEqual([]);
  });

  it("unlisten after reconnect detaches from the replacement connection and does not re-add it", async () => {
    const { listen, reconnectSse } = await import("./listen");
    const unlisten = await listen("session-update", () => {});
    reconnectSse();
    const replacement = latest();
    unlisten();
    expect(replacement.eventNames()).toEqual([]);
    // Refcount dropped to zero → the replacement is closed too.
    expect(replacement.closed).toBe(true);
    // A later reconnect has nothing to re-attach and nothing open to replace.
    reconnectSse();
    expect(FakeEventSource.instances).toHaveLength(2);
  });

  it("reopens the stream with the new token when the live token changes", async () => {
    const { setApiToken } = await import("./apiToken");
    const { listen } = await import("./listen");
    setApiToken("before");
    await listen("session-update", () => {});
    expect(urls()).toEqual(["http://127.0.0.1:11423/api/events?token=before"]);

    // A rotation — from Settings in this tab, or pushed over HMR because
    // another process rewrote the file — must not leave the stream on the
    // dead token.
    setApiToken("after");
    expect(FakeEventSource.instances[0].closed).toBe(true);
    expect(urls()).toEqual([
      "http://127.0.0.1:11423/api/events?token=before",
      "http://127.0.0.1:11423/api/events?token=after",
    ]);
    expect(latest().eventNames()).toEqual(["session-update"]);
  });

  it("ignores a token change when nothing is listening", async () => {
    const { setApiToken } = await import("./apiToken");
    await import("./listen");
    setApiToken("x");
    expect(urls()).toEqual([]);
  });

  describe("a stream the browser gave up on", () => {
    it("is reopened with the current credential and its listeners, after a short delay", async () => {
      const { setApiToken } = await import("./apiToken");
      const { listen, SSE_REOPEN_MIN_MS } = await import("./listen");
      setApiToken("tok");
      const handler = vi.fn();
      await listen("session-update", handler);
      const dead = latest();

      // e.g. the backend was restarted and answers the reconnect with a 401.
      dead.fail();
      expect(FakeEventSource.instances).toHaveLength(1);

      vi.advanceTimersByTime(SSE_REOPEN_MIN_MS - 1);
      expect(FakeEventSource.instances).toHaveLength(1);
      vi.advanceTimersByTime(1);
      expect(FakeEventSource.instances).toHaveLength(2);
      expect(latest().url).toBe("http://127.0.0.1:11423/api/events?token=tok");
      expect(latest().eventNames()).toEqual(["session-update"]);

      // Events on the replacement reach the same handler.
      latest().message("session-update", '{"count":1}');
      expect(handler).toHaveBeenCalledWith({ payload: { count: 1 } });
    });

    it("is left alone while the browser is still retrying a network failure itself", async () => {
      const { listen, SSE_REOPEN_MAX_MS } = await import("./listen");
      await listen("session-update", () => {});

      latest().drop();
      latest().drop();
      vi.advanceTimersByTime(SSE_REOPEN_MAX_MS * 4);
      expect(FakeEventSource.instances).toHaveLength(1);
    });

    it("backs off while the reply stays refused and starts over once a stream opens", async () => {
      const { listen, SSE_REOPEN_MIN_MS, SSE_REOPEN_MAX_MS } = await import("./listen");
      await listen("session-update", () => {});

      // 1s, 2s, 4s, 8s, 16s, 30s, 30s, ...
      let expected = SSE_REOPEN_MIN_MS;
      for (let attempt = 1; attempt <= 8; attempt++) {
        const before = FakeEventSource.instances.length;
        latest().fail();
        vi.advanceTimersByTime(expected - 1);
        expect(FakeEventSource.instances).toHaveLength(before);
        vi.advanceTimersByTime(1);
        expect(FakeEventSource.instances).toHaveLength(before + 1);
        expected = Math.min(expected * 2, SSE_REOPEN_MAX_MS);
      }

      // A stream that opens resets the schedule.
      latest().open();
      latest().fail();
      vi.advanceTimersByTime(SSE_REOPEN_MIN_MS);
      expect(FakeEventSource.instances).toHaveLength(10);
    });

    it("is not reopened once the last listener is gone", async () => {
      const { listen, SSE_REOPEN_MAX_MS } = await import("./listen");
      const unlisten = await listen("session-update", () => {});
      latest().fail();
      unlisten();
      vi.advanceTimersByTime(SSE_REOPEN_MAX_MS * 4);
      expect(FakeEventSource.instances).toHaveLength(1);
    });

    it("is reopened at once, not on the timer, when the credential changes meanwhile", async () => {
      const { setApiToken } = await import("./apiToken");
      const { listen, SSE_REOPEN_MAX_MS } = await import("./listen");
      setApiToken("stale");
      await listen("session-update", () => {});
      latest().fail();

      // The reissued credential arrives (Settings in this tab, or HMR) before
      // the timer fires: the stream follows it immediately…
      setApiToken("fresh");
      expect(urls()).toEqual([
        "http://127.0.0.1:11423/api/events?token=stale",
        "http://127.0.0.1:11423/api/events?token=fresh",
      ]);
      // …and the pending reopen does not open a third one on top.
      vi.advanceTimersByTime(SSE_REOPEN_MAX_MS * 4);
      expect(FakeEventSource.instances).toHaveLength(2);
    });

    it("ignores an error from a stream that has already been replaced", async () => {
      const { listen, reconnectSse, SSE_REOPEN_MAX_MS } = await import("./listen");
      await listen("session-update", () => {});
      const old = latest();
      reconnectSse();
      expect(FakeEventSource.instances).toHaveLength(2);

      old.fail();
      vi.advanceTimersByTime(SSE_REOPEN_MAX_MS * 4);
      expect(FakeEventSource.instances).toHaveLength(2);
    });

    it("a new listener arriving on a dead stream opens a fresh one and drops the pending reopen", async () => {
      const { listen, SSE_REOPEN_MAX_MS } = await import("./listen");
      await listen("session-update", () => {});
      latest().fail();

      await listen("picker-refresh", () => {});
      expect(FakeEventSource.instances).toHaveLength(2);
      expect(latest().eventNames()).toEqual(["session-update", "picker-refresh"]);

      vi.advanceTimersByTime(SSE_REOPEN_MAX_MS * 4);
      expect(FakeEventSource.instances).toHaveLength(2);
    });
  });
});
