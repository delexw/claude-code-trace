"""Tests for api._get/_post attaching the API token and surfacing 401s."""

from __future__ import annotations

import httpx
import pytest

import api
import auth
from sse import SSEClient


class FakeResponse:
    def __init__(self, status: int = 200, payload: object = None) -> None:
        self.status_code = status
        self._payload = payload
        self.request = httpx.Request("GET", "http://test/api/x")
        self.text = "" if payload is None else "x"

    def raise_for_status(self) -> None:
        if self.status_code >= 400:
            raise httpx.HTTPStatusError(
                "error", request=self.request, response=httpx.Response(self.status_code)
            )

    def json(self) -> object:
        return self._payload


class FakeClient:
    calls: list[tuple] = []
    status = 200
    payload: object = ["/proj"]

    def __init__(self, **_kwargs) -> None:
        pass

    async def __aenter__(self) -> FakeClient:
        return self

    async def __aexit__(self, *_exc) -> bool:
        return False

    async def get(self, url: str, headers: dict | None = None) -> FakeResponse:
        FakeClient.calls.append(("GET", url, headers))
        return FakeResponse(FakeClient.status, FakeClient.payload)

    async def post(
        self, url: str, json: object = None, headers: dict | None = None
    ) -> FakeResponse:
        FakeClient.calls.append(("POST", url, headers, json))
        return FakeResponse(FakeClient.status, FakeClient.payload)


@pytest.fixture(autouse=True)
def fake_client(monkeypatch):
    FakeClient.calls = []
    FakeClient.status = 200
    FakeClient.payload = ["/proj"]
    monkeypatch.setattr(api.httpx, "AsyncClient", FakeClient)
    monkeypatch.setattr(auth, "resolve_api_token", lambda: "tok")
    return FakeClient


async def test_get_sends_token_header():
    assert await api.get_project_dirs() == ["/proj"]
    method, url, headers = FakeClient.calls[0]
    assert method == "GET"
    assert url.endswith("/api/project-dirs")
    assert headers == {"X-CCTrace-Token": "tok"}


async def test_post_sends_token_header_and_body():
    FakeClient.payload = None
    await api.watch_session("/s.jsonl")
    method, url, headers, body = FakeClient.calls[0]
    assert method == "POST"
    assert url.endswith("/api/session/watch")
    assert headers == {"X-CCTrace-Token": "tok"}
    assert body == {"path": "/s.jsonl"}


async def test_no_header_when_token_unavailable(monkeypatch):
    monkeypatch.setattr(auth, "resolve_api_token", lambda: None)
    await api.get_project_dirs()
    assert FakeClient.calls[0][2] == {}


async def test_401_raises_api_auth_error_naming_the_token_source():
    FakeClient.status = 401
    with pytest.raises(api.ApiAuthError) as info:
        await api.get_project_dirs()
    assert "api-token" in str(info.value)
    assert "CCTRACE_API_TOKEN" in str(info.value)
    # Still an httpx.HTTPStatusError, so existing broad handlers keep working.
    assert isinstance(info.value, httpx.HTTPStatusError)


async def test_other_errors_are_plain_http_status_errors():
    FakeClient.status = 500
    with pytest.raises(httpx.HTTPStatusError) as info:
        await api.get_project_dirs()
    assert not isinstance(info.value, api.ApiAuthError)


# --- SSE ----------------------------------------------------------------------


class FakeStream:
    def __init__(self, lines: list[str]) -> None:
        self._lines = lines

    async def __aenter__(self) -> FakeStream:
        return self

    async def __aexit__(self, *_exc) -> bool:
        return False

    def raise_for_status(self) -> None:
        pass

    async def aiter_lines(self):
        for line in self._lines:
            yield line


class FakeStreamClient:
    seen: list[tuple] = []
    lines: list[str] = ["event: picker-refresh", 'data: {"n": 1}', ""]

    def __init__(self, **_kwargs) -> None:
        pass

    async def __aenter__(self) -> FakeStreamClient:
        return self

    async def __aexit__(self, *_exc) -> bool:
        return False

    def stream(self, method: str, url: str, headers: dict | None = None) -> FakeStream:
        FakeStreamClient.seen.append((method, url, headers))
        return FakeStream(FakeStreamClient.lines)


async def test_sse_client_forwards_static_headers(monkeypatch):
    import sse

    FakeStreamClient.seen = []
    monkeypatch.setattr(sse.httpx, "AsyncClient", FakeStreamClient)
    received: list[object] = []
    client = SSEClient("http://x/api/events", headers={"X-CCTrace-Token": "t"})
    client.on("picker-refresh", received.append)

    await client._stream()

    assert FakeStreamClient.seen == [("GET", "http://x/api/events", {"X-CCTrace-Token": "t"})]
    assert received == [{"n": 1}]


async def test_sse_client_evaluates_header_callable_on_each_connect(monkeypatch):
    import sse

    FakeStreamClient.seen = []
    monkeypatch.setattr(sse.httpx, "AsyncClient", FakeStreamClient)
    tokens = iter(["first", "second"])
    client = SSEClient("http://x/api/events", headers=lambda: {"X-CCTrace-Token": next(tokens)})

    await client._stream()
    await client._stream()

    assert [h for _, _, h in FakeStreamClient.seen] == [
        {"X-CCTrace-Token": "first"},
        {"X-CCTrace-Token": "second"},
    ]


async def test_sse_client_without_headers_sends_empty_mapping(monkeypatch):
    import sse

    FakeStreamClient.seen = []
    monkeypatch.setattr(sse.httpx, "AsyncClient", FakeStreamClient)
    await SSEClient("http://x/api/events")._stream()
    assert FakeStreamClient.seen[0][2] == {}
