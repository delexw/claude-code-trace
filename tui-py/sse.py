"""Async SSE client for the Claude Code Trace backend.

Streams from GET /api/events and fires callbacks for named events.
Usage:
    client = SSEClient("http://127.0.0.1:11423/api/events")
    client.on("picker-refresh", my_handler)
    client.on("session-update", my_handler)
    await client.connect()   # runs until cancelled
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
from collections.abc import Awaitable, Callable
from typing import Any

import httpx

log = logging.getLogger(__name__)

Handler = Callable[[Any], Awaitable[None] | None]


class SSEClient:
    def __init__(self, url: str) -> None:
        self._url = url
        self._handlers: dict[str, list[Handler]] = {}
        self._task: asyncio.Task | None = None
        self._stop = asyncio.Event()

    def on(self, event: str, handler: Handler) -> None:
        self._handlers.setdefault(event, []).append(handler)

    def off(self, event: str, handler: Handler) -> None:
        if event in self._handlers:
            with contextlib.suppress(ValueError):
                self._handlers[event].remove(handler)

    def start(self) -> None:
        """Start the SSE connection in the background."""
        self._stop.clear()
        self._task = asyncio.create_task(self._run())

    def stop(self) -> None:
        """Stop the SSE connection."""
        self._stop.set()
        if self._task:
            self._task.cancel()

    async def _run(self) -> None:
        """Connect and stream SSE events, reconnecting on errors."""
        while not self._stop.is_set():
            try:
                await self._stream()
            except asyncio.CancelledError:
                return
            except Exception as e:
                log.debug("SSE disconnected: %s — reconnecting in 2s", e)
                if not self._stop.is_set():
                    await asyncio.sleep(2)

    async def _stream(self) -> None:
        async with (
            httpx.AsyncClient(timeout=httpx.Timeout(None)) as client,
            client.stream("GET", self._url) as resp,
        ):
            resp.raise_for_status()
            event_name = ""
            data_lines: list[str] = []

            async for raw_line in resp.aiter_lines():
                if self._stop.is_set():
                    return

                line = raw_line.strip()

                if line.startswith("event:"):
                    event_name = line[len("event:") :].strip()
                elif line.startswith("data:"):
                    data_lines.append(line[len("data:") :].strip())
                elif line == "":
                    # Dispatch accumulated event
                    if event_name and data_lines:
                        payload_str = "\n".join(data_lines)
                        try:
                            payload = json.loads(payload_str) if payload_str else None
                        except json.JSONDecodeError:
                            payload = payload_str

                        handlers = self._handlers.get(event_name, [])
                        for h in handlers:
                            try:
                                result = h(payload)
                                if asyncio.iscoroutine(result):
                                    await result
                            except Exception:
                                log.exception("SSE handler error for event %s", event_name)

                    event_name = ""
                    data_lines = []
