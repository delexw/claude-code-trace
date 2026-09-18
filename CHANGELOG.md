# Changelog

All notable changes to claude-code-trace are documented here. Versions follow
[semantic versioning](https://semver.org/).

## [0.16.0] — 2026-09-18

Installing on macOS no longer involves a terminal workaround. The app is unsigned, so every
`.dmg` downloaded through a browser arrived tagged as quarantined, and macOS refused to open it
until you cleared that flag with `xattr -cr` by hand. There is now a one-line installer that
sidesteps the problem entirely, and the `.dmg` that caused it is no longer published.

### Added

- **One-line macOS install**
  ([`96d5aaf`](https://github.com/delexw/claude-code-trace/commit/96d5aaf)). Running
  `curl -fsSL https://raw.githubusercontent.com/delexw/claude-code-trace/main/script/install-macos.sh | bash`
  downloads the latest release and puts **Claude Code Trace.app** into `/Applications`, ready to
  open from Spotlight. No clone, no Rust or Node toolchain, and no quarantine step: the quarantine
  flag is applied by whatever downloads a file rather than baked into the artifact, and `curl`
  never sets it, so the app launches the way a normal app does. Set `CCTRACE_VERSION` to pin a
  release or `CCTRACE_INSTALL_DIR` to install somewhere else. Apple Silicon only, as before — the
  installer asks the kernel whether it is running under Rosetta so an Apple Silicon Mac in an
  Intel shell still resolves correctly, and points a genuine Intel Mac at a source build instead
  of failing on a download that was never published.

### Removed

- **The macOS `.dmg`** ([`96d5aaf`](https://github.com/delexw/claude-code-trace/commit/96d5aaf)).
  `Claude.Code.Trace_aarch64.app.tar.gz` is now the only macOS artifact. The `.dmg` looked like an
  ordinary installer, which made it both the most likely thing to download and the most likely
  thing to be blocked on arrival; dropping it leaves the one-line install as the path people
  actually meet. Linux and Windows artifacts are unchanged.

[0.16.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.16.0

## [0.15.1] — 2026-09-16

A reliability release. Three of these bugs could make a session look emptier than it really was —
one unreadable field could blank a whole conversation, a turn that fired fourteen tools at once
rendered one of them, and a live tail could quietly stop updating while the tab still looked
connected. The rest tighten up what v0.14.0 and v0.15.0 introduced: the Docker UI holds on to its
credential across a browser restart, the "not an accepted client" banner clears the moment a
credential arrives, and Settings now spells out every way to send one. Costs for Fable, Mythos and
Opus 5 sessions are also corrected.

### Fixed

- **One unreadable field no longer blanks an entire session**
  ([`05f28d3`](https://github.com/delexw/claude-code-trace/commit/05f28d3)). serde only falls back
  to a default when a key is missing, so a transcript line carrying `"field": null` on a
  non-optional field failed to parse and the whole entry was thrown away. That punched a hole in
  the `parentUuid` chain, the live-chain walk stopped at the hole, and the session came up empty in
  the UI. Claude Code v2.1.266 triggered exactly this by writing `null` for `iterations`, `speed`
  and `inference_geo` on synthetic assistant messages. Every non-optional field on `Entry`,
  `EntryMessage`, `EntryUsage`, `CacheCreationUsage` and `IterationUsage` now reads `null` as its
  default, and a test reads the struct definitions directly so a field added later without the
  annotation fails the build rather than silently reintroducing the bug.

- **A turn that runs tools in parallel now shows all of them**
  ([`c936d08`](https://github.com/delexw/claude-code-trace/commit/c936d08)). When one turn fires
  several tools at once, Claude Code writes each call as its own entry and parents each result to
  the call that issued it, so the turn hangs off the conversation as a fan rather than sitting
  along it. The backward walk followed a single path, kept one call and one result per batch, and
  dropped every sibling as a dead end — a fourteen-call turn rendered one call. The siblings are
  now re-added under two narrow rules, so rewound prompts and superseded retries stay hidden. The
  walk also no longer stops dead when a `parentUuid` points at an entry the file doesn't contain;
  it resumes from the nearest earlier entry, so a gap costs one entry instead of every message
  before it. Checked against the 40 newest sessions on disk: 20 render differently and all 20
  gained content, none lost any.

- **The live tail no longer freezes after the browser gives up on a refused reconnect**
  ([`35e7533`](https://github.com/delexw/claude-code-trace/commit/35e7533)). `/api/events` needs a
  client credential, and a reconnect the browser makes on its own reuses whatever credential the
  stream was opened with — so after a backend restart, a sleep/wake, or a suspended tab waking up
  once the credential has changed, the reply is a 401. `EventSource` treats any non-200 as a failed
  connection: it closes, fires one error and never retries. Nothing was listening for that error,
  so the tab looked connected while no update ever arrived and the live tail and ongoing indicator
  sat still. The stream is now reopened with the current credential and every registered listener,
  backing off from 1s to 30s and resetting once a stream opens.

- **Conversations stay scrollable at larger font sizes**
  ([`f296346`](https://github.com/delexw/claude-code-trace/commit/f296346), @sy2002). At bigger
  font scales you could not reach the end of a conversation. Two separate things caused it: the
  message list could not shrink, so WKWebView never gave it a real scroll range, and the app shell
  height ignored the root CSS zoom. Scaling is now applied before React mounts and the virtual list
  is remeasured after a live font-size change, so rows are measured against the geometry actually
  on screen. Fixes #297.

- **Fable, Mythos and Opus 5 sessions are costed at the right rates**
  ([`e119897`](https://github.com/delexw/claude-code-trace/commit/e119897), @guillermodrums-ui).
  Fable and Mythos sessions fell through to Sonnet 4's $3/$15, undercounting cost by roughly 3x;
  they now use the official $10/$50, with cache reads at $1 for the 5 models and $0.25 for 5.1.
  Opus 5 drops to the standard Opus $5/$25 — the previous $10/$50 was fast-mode pricing, which a
  transcript gives no way to tell apart. Sonnet 5's $2/$10 row was also missing from the frontend
  table and now matches the backend.

- **The Docker UI keeps its credential when you close and reopen the browser**
  ([`9a2d4f5`](https://github.com/delexw/claude-code-trace/commit/9a2d4f5)). The same-origin UI
  gets its credential as a session cookie attached to the HTML shell, but only a full 200 response
  carried it. Once a browser had `index.html` cached it revalidated instead, and the resulting 304
  — headers only, no content type — wasn't recognised as the shell, so no cookie rode along. The
  tab sat on "Not an accepted client" while the same URL worked fine in a private window. The shell
  is now recognised on a 304 by its request path and served `Cache-Control: no-cache` so the
  browser always checks in; assets are untouched and stay cacheable.

- **The "not an accepted client" banner clears as soon as a credential arrives**
  ([`fc6af7c`](https://github.com/delexw/claude-code-trace/commit/fc6af7c)). The recovery path only
  started listening for a new credential after the banner's error state had landed, which can be a
  tick after the banner has already painted. A credential pushed in that gap found nobody
  listening, so the banner stayed up and the app never retried its start-up. The listener is now
  live for the whole mount.

- **Settings names every way to send a credential, and dates you can read**
  ([`061bcad`](https://github.com/delexw/claude-code-trace/commit/061bcad)). The Accepted-clients
  note named `X-CCTrace-Token` as though it were the only accepted carrier, when
  `Authorization: Bearer` works just as well, and called the credential "signed" without
  mentioning it is a JWT. The Allowed-origins note didn't say that origins only apply to browsers
  and grant nothing on their own, so adding one and still getting a 401 looked like a bug. The
  **Since** column is now **Issued** and shows a named month, because the old format rendered
  `08/09/2026` — two different dates depending on where you live.

- **The Docker image builds again**
  ([`ad3ad86`](https://github.com/delexw/claude-code-trace/commit/ad3ad86)). `vite.config.ts` loads
  `bin/api-token.mjs` when the config is read, but the frontend build stage never copied `bin/`, so
  `vite build` failed with `UNRESOLVED_IMPORT` even though the plugin only runs during development.
  Nothing connected the config's imports to the stage's `COPY` lines, so this only ever broke
  inside Docker; a test now reads both files and asserts every local import is covered.

[0.15.1]: https://github.com/delexw/claude-code-trace/releases/tag/v0.15.1

## [0.15.0] — 2026-09-08

A short release that finishes what v0.14.0 started. Locking the local API behind a single shared
token told the backend that a caller was allowed in, but not who it was — so there was no way to
cut off one tool without cutting off all of them. Every caller is now a named client with its own
credential that can be revoked or reissued on its own. Regenerating a credential in the web UI
also stops throwing you out of the Settings dialog you did it from.

### Changed

- **Every API client now has its own identity: per-client signed credentials replace the shared
  token** ([`6b09244`](https://github.com/delexw/claude-code-trace/commit/6b09244)). v0.14.0
  proved a caller was _an_ accepted client but not _which_ one, and revoking one
  meant rotating everybody. Now every caller of `/api/*` is a registered **client** with a name and
  an HS256 credential (`{ sub: <client id>, name, iat }`, no expiry); the backend attaches the
  caller's identity to each request (`GET /api/whoami`). Clients can be **revoked** or **reissued**
  individually — reissue invalidates every older credential of that client and nobody else's. The
  web UI (`web-ui`) and the TUI (`tui`) are registered automatically on first start and keep
  working as before: the dev server and the TUI read their credentials from
  `<config dir>/clients/<name>.jwt`, and the Docker same-origin UI still gets its credential as an
  `HttpOnly` cookie. Settings → **API access** becomes **Accepted clients**: list, Add client
  (credential shown once, never stored), Reissue and Revoke with two-click confirms. New routes
  `GET|POST /api/clients`, `POST /api/clients/{id}/reissue|revoke`, `GET /api/whoami`; Tauri
  commands `list_clients`, `register_client`, `reissue_client`, `revoke_client`.

  **Behaviour change:** the shared `api-token` file and `CCTRACE_API_TOKEN` are gone, and
  `POST /api/settings/token/regenerate` / `regenerate_api_token` are removed. Scripts that sent the
  shared token must register a client (Settings → Accepted clients, or `POST /api/clients` using the
  `tui` client's `clients/tui.jwt` as a bootstrap) and send its credential instead. On the first
  start after upgrading, the backend creates `api-secret`, `clients.json` and the two built-in
  clients in the config dir; the old `api-token` file is ignored and can be deleted.
  `CCTRACE_API_AUTH=off` still disables verification. Existing browser tabs and TUIs pick the new
  credentials up on reload / next request.

### Fixed

- **Regenerating a credential no longer closes the Settings dialog you did it from**
  ([`4382147`](https://github.com/delexw/claude-code-trace/commit/4382147)). In `cctrace --web`
  the browser gets its credential from the dev server, which watched the credential file and
  restarted itself whenever it changed. Clicking Regenerate in Settings rewrote that file, so the
  restart reloaded the page and tore down the very dialog you were standing in — the new value
  appeared, but only after the UI had thrown you back to the start. The dev server now pushes the
  new credential into the running page instead of restarting, so the dialog stays open and the
  live SSE stream reconnects on its own. Rotations performed by another `cctrace` process are
  picked up the same way, and production bundles still never contain a credential.

[0.15.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.15.0

## [0.14.0] — 2026-09-06

This release closes the biggest hole in the local backend: until now anything running on your
machine could talk to the HTTP API, and every `/api/*` route now requires a shared token
instead. The rest is keeping pace with Claude Code — sessions whose transcripts were silently
spliced together by a pre-v2.1.251 bug are now flagged rather than shown as one conversation,
subagent failures and interrupted tool calls stop disappearing from the view, and the project
tree nests worktrees by their real path instead of guessing from a folder name that a host can
now rename.

### Added

- **The local HTTP API now only answers accepted clients**
  ([`f205651`](https://github.com/delexw/claude-code-trace/commit/f205651),
  [`875dc07`](https://github.com/delexw/claude-code-trace/commit/875dc07)). Every `/api/*` route requires a
  shared secret token. Until now the only gate was a CORS allowlist, which protects browsers and
  nothing else: any local process — or any LAN host, when the Docker image binds `0.0.0.0` —
  could read session transcripts, rewrite `settings.json`, add CORS origins, or trigger the
  `git`/`osascript` side effects of `/api/git-info` and `/api/focus`. The token is generated once
  into `<config dir>/claude-code-trace/api-token` (mode `0600`) and read by the bundled clients
  automatically: the web UI gets it from a Vite plugin in dev mode and from an `HttpOnly`
  same-origin cookie in Docker, the TUI reads the file. `CCTRACE_API_TOKEN` pins a token;
  `CCTRACE_API_AUTH=off` disables the check. A new **API access** section in Settings shows the
  token (masked) with Copy and a two-click Regenerate. The desktop webview is unaffected (IPC).

  **Behaviour change:** scripts calling the API must now send `X-CCTrace-Token` (or
  `Authorization: Bearer`) or set `CCTRACE_API_AUTH=off`. Docker users reaching the UI via a LAN
  IP or reverse-proxy hostname must add that origin to `CCTRACE_ALLOWED_ORIGINS` for the cookie to
  be issued — the cookie is only handed to allowlisted `Host`s so a DNS-rebinding page can't
  collect it.

### Fixed

- **Transcripts that Claude Code spliced together are now flagged**
  ([`7e9c911`](https://github.com/delexw/claude-code-trace/commit/7e9c911)). Before
  v2.1.251, changing directory mid-session could move a transcript onto a filename
  already used by an unrelated session with the same session id, quietly merging two
  conversations into one file. Those sessions used to render as a single continuous
  conversation with no hint that half of it belonged to something else. The scanner now
  recognises the signature — a main-chain entry whose parent is missing, landing exactly
  where the working directory, branch or timestamp order jumps — and shows an integrity
  warning in both the session picker and the open session's info bar. Checked against 93
  real session files with no false positives.

- **Worktrees nest by their real path in the project tree**
  ([`8619622`](https://github.com/delexw/claude-code-trace/commit/8619622)). Claude Code
  v2.1.234 lets a host name a project's transcript folder anything it likes via
  `CLAUDE_CODE_PROJECT_DIR_NAME`, and the sidebar was deciding what nests under what by
  string-matching that folder name. Under an assigned name, unrelated projects could end
  up nested together and related ones could fail to nest at all. The tree now uses the
  real working directory recorded in each session. This also fixes ordinary paths that
  merely look nested — `/home/user/backend` and `/home/user/backend-v2` are siblings, and
  are finally treated as such.

- **Interrupted tool calls no longer look like the conversation just stopped**
  ([`9962c5f`](https://github.com/delexw/claude-code-trace/commit/9962c5f)). From
  v2.1.236 Claude Code stops writing an interrupt marker or a synthetic denial result
  when a print/SDK session is killed mid tool call — the transcript simply ends on a
  `tool_use` with nothing after it. The backend already understood this, but nothing in
  the UI said so, so the session appeared to trail off for no reason. Those calls now
  carry a **pending** badge next to the existing orphan badge.

- **Subagent failures are visible again**
  ([`240aa5d`](https://github.com/delexw/claude-code-trace/commit/240aa5d)). In v2.1.247
  a subagent retries down the session's fallback model chain after a model 404, and when
  every fallback is exhausted the `Task` result becomes a structured error object. That
  object was being dropped during classification, and the Subagent detail panel never
  rendered tool results at all — so the failure was invisible on both the desktop/web UI
  and the TUI. The error now survives into the rendered result on every surface, with a
  warning icon on the item.

- **Two tool calls in one message no longer collapse into one**
  ([`56b2815`](https://github.com/delexw/claude-code-trace/commit/56b2815)). Third-party
  `ANTHROPIC_BASE_URL` proxies can stream a `tool_use` block with no `id`. Every such
  block used to share the same empty key, so all but the last one lost its matching
  result. Each now gets a stable placeholder id scoped to its entry and position, so the
  calls render separately and, in the worst case, degrade to an orphan result rather than
  silently swallowing one another.

- **Background-task notifications between turns are no longer dropped**
  ([`d1064f5`](https://github.com/delexw/claude-code-trace/commit/d1064f5)). Since
  v2.1.234 these arrive wrapped in `<system-reminder>` tags, and anything fully wrapped
  in those tags was treated as noise and discarded — taking the task's completed, failed
  or killed status with it. The wrapper is now unwrapped before the noise check, so the
  notification shows up as a System message with the right status. Plain reminders with
  nothing inside are still dropped.

- **React rendering correctness across the UI**
  ([`3c7e1c2`](https://github.com/delexw/claude-code-trace/commit/3c7e1c2),
  [`7f90c2a`](https://github.com/delexw/claude-code-trace/commit/7f90c2a)). A newer
  oxlint surfaced 23 real violations: refs written during render, and effects that
  clamped an out-of-range index only after a first bad render. The keyboard, event,
  visible-session and message-detail hooks now write their refs after commit, and the
  selected-message and debug-viewer indexes are derived during render so every read in a
  pass agrees. The message detail panel also stops restoring a stale scroll position when
  a panel is swapped at the same depth.

[0.14.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.14.0

## [0.13.0] — 2026-08-20

Claude Code moved fast between v2.1.208 and v2.1.233, and most of this release is about
keeping up with it. Forked sessions got their own worktrees, cross-session messaging
arrived, `Task`/`TodoWrite` went off-by-default on the newest models, and several tool
inputs documented as strings turned out not to be. Sessions that previously rendered
under the wrong project — or dropped messages entirely — now parse correctly. Opus 5
also gets its real pricing, and a scrolling bug that pushed the toolbars off-screen with
no way to get them back is fixed.

### Added

- **Correct pricing for `claude-opus-5`**
  ([`ca065c0`](https://github.com/delexw/claude-code-trace/commit/ca065c0)). Claude Code
  v2.1.219 made `claude-opus-5` the default Opus model. Because the name contains
  "opus", it fell through to the old Opus 4.x rate and every session was costed at
  $5/$25 per Mtok instead of $10/$50 — roughly half the real figure. Both the Rust and
  TypeScript pricing tables now carry an explicit Opus 5 entry. This also fixed a latent
  bug where the unknown-model fallback was a hardcoded array index, so adding any entry
  silently repriced unrecognised models.

### Fixed

- **Non-Latin prompts no longer crash the parser**
  ([`1e609e2`](https://github.com/delexw/claude-code-trace/commit/1e609e2), @SAY-5). An
  orphan subagent's description was truncated by slicing at byte 80. For Chinese,
  Japanese, Korean or emoji text that offset lands inside a character, and Rust panics
  rather than splitting it — so the app died outright on any such prompt longer than 80
  bytes. Truncation now counts characters.

- **Forked subagents no longer inherit their parent's history**
  ([`ca454a9`](https://github.com/delexw/claude-code-trace/commit/ca454a9)). In
  v2.1.232+ a forked subagent's own transcript replays inherited parent-conversation
  entries. Those were being read as the subagent's own, which corrupted its task
  description and inflated its reported duration. They're now excluded, and the
  `.forked-skill.json` sidecar is read so a forked skill still gets a sensible
  description when its parent conversation is gone (after `/clear`, for example).

- **Tool summaries survive non-string inputs**
  ([`3e2e10f`](https://github.com/delexw/claude-code-trace/commit/3e2e10f)). As of
  v2.1.229, fields documented as strings — Bash's `command`, Read/Edit/Write's
  `file_path`, Grep's `glob` — can arrive as an array or object. Those were silently
  dropped, leaving a bare tool name with no detail. Non-string values are now stringified
  so the summary still tells you what the tool did.

- **Empty Teams boards explain themselves**
  ([`e17791e`](https://github.com/delexw/claude-code-trace/commit/e17791e)). v2.1.233
  disables `Task`/`TodoWrite` by default on Opus 4.8, Sonnet 5, Fable 5 and Mythos 5+. The
  Tasks section used to disappear when a team had no tasks, which was indistinguishable
  from a parsing failure. It now always renders, with a note explaining why it's empty, on
  both web and TUI.

- **Cross-session messages render as messages**
  ([`626b5ce`](https://github.com/delexw/claude-code-trace/commit/626b5ce)). v2.1.224
  added SendMessage between sessions. A delivered inbound message arrives wrapped in a
  `<cross-session-message>` tag, which showed up raw in the transcript. The tag is now
  unwrapped and the text prefixed with the sender's name so you can still tell who it came
  from.

- **Messages that are only an attachment no longer vanish**
  ([`b9d39dd`](https://github.com/delexw/claude-code-trace/commit/b9d39dd)). v2.1.223
  added a diagnostics attachment block whose type discriminant isn't documented. A user
  message containing nothing else produced no visible text, so the whole entry was
  dropped from the transcript. Attachment-shaped blocks are now recognised generically
  and render an `[Attachment: <type>]` placeholder instead of disappearing.

- **Forked sessions nest under the repo they came from**
  ([`6b83f64`](https://github.com/delexw/claude-code-trace/commit/6b83f64)). v2.1.221
  gave `/fork` its own worktree, so a fork's working directory shares no path prefix with
  its parent. The project tree groups by path, so forks showed up as unrelated top-level
  projects. Forks now resolve back through the fork chain to their origin project, with a
  cycle guard for multi-hop chains. Mirrored in the Python TUI.

- **Roaming sessions are labelled by where they started**
  ([`fd7528e`](https://github.com/delexw/claude-code-trace/commit/fd7528e)). A session
  that changes directory across repos records a different `cwd` per entry, and only the
  last one was kept. A session started in one repo that later worked in another was filed
  under the second, even though its transcript lives with the first. Sessions now track
  every directory in first-seen order, label the node by the origin, and show a "worked
  in" line when a session spanned more than one.

- **The toolbars stop scrolling off the top of the window**
  ([`fd7528e`](https://github.com/delexw/claude-code-trace/commit/fd7528e)). Scrolling
  the selected item into view used `scrollIntoView`, which scrolls every scrollable
  ancestor. The app shell is `overflow: hidden` but still scrollable programmatically, so
  entering a session or returning to the list could push the InfoBar and ViewToolbar out
  of sight with no scrollbar to bring them back. Only the real list container is scrolled
  now.

- **Directories added mid-session are tracked**
  ([`31ec584`](https://github.com/delexw/claude-code-trace/commit/31ec584)). v2.1.219
  added the `DirectoryAdded` hook, fired by `/add-dir`. The new path lives in the hook's
  data block rather than the entry's top-level `cwd`, so it never made it into the
  session's directory list. It's now picked up and de-duplicated, leaving the primary
  working directory unchanged.

- **Assistant messages from v2.1.212+ parse again**
  ([`af7e19d`](https://github.com/delexw/claude-code-trace/commit/af7e19d)). v2.1.212
  started recording the reasoning effort level on every assistant message. The field was
  only expected at the top level of an entry, so messages carrying it failed to parse.

- **Long project paths fail loudly instead of guessing**
  ([`b0dccac`](https://github.com/delexw/claude-code-trace/commit/b0dccac)). v2.1.224
  changed how Claude Code disambiguates project directory names longer than 200
  characters, and the scheme isn't documented. The helper that recomputes those names now
  refuses rather than guessing, so it can't quietly resolve to another project's sessions
  if it's wired up later.

[0.13.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.13.0

## [0.12.0] — 2026-07-18

This release closes a real privacy gap: the backend's CORS policy allowed any origin, so
any site you visited could read your local Claude session data — prompts, code, tool
output — while the app was running. The fix ships alongside a new Settings UI for
managing extra allowed origins yourself. Also new: an advisor-tool model badge in the
detail view, and a `desktop` Cargo feature that lets the server build and run without
webkit2gtk on machines that don't have it. Plus fixes to the Docker image's healthcheck
and settings persistence, a parser live-chain edge case, and Sonnet 5's inlined
system-reminders.

### Added

- **Configurable CORS-allowed origins in Settings**
  ([`2ae2725`](https://github.com/delexw/claude-code-trace/commit/2ae2725)).
  You can now add extra CORS-allowed origins from the Settings modal instead of only via
  the `CCTRACE_ALLOWED_ORIGINS` environment variable at launch. Changes take effect
  immediately — no server restart required, matching every other setting.

- **Advisor tool model badge**
  ([`5f372a8`](https://github.com/delexw/claude-code-trace/commit/5f372a8)).
  Sessions that call the `advisor` tool now show which model produced the advice as a
  badge next to the tool name in the detail view. (Web only for now — the TUI doesn't
  render this badge yet.)

- **Headless-only builds without webkit2gtk**
  ([`bc188e0`](https://github.com/delexw/claude-code-trace/commit/bc188e0), @smoser).
  A new `desktop` Cargo feature (on by default) gates the Tauri/webview stack, so building
  with `--no-default-features` produces a headless HTTP-server-only binary that links no
  webkit2gtk — useful for machines and base images (e.g. Wolfi/Chainguard) without a
  display or that library available. Desktop builds are unaffected.

### Fixed

- **CORS no longer allows any origin**
  ([`0b0432a`](https://github.com/delexw/claude-code-trace/commit/0b0432a), @Guthman).
  The backend previously used a permissive CORS policy, returning
  `Access-Control-Allow-Origin: *` from its unauthenticated local server — any site you
  visited in a browser could read your local session data cross-origin. CORS is now
  scoped to an allowlist of the web UI's own origins (extendable via
  `CCTRACE_ALLOWED_ORIGINS` or the new Settings UI above), with methods limited to
  GET/POST.

- **Docker image healthcheck, settings persistence, and log spam**
  ([`de2f9a1`](https://github.com/delexw/claude-code-trace/commit/de2f9a1)).
  The container's healthcheck used a `/dev/tcp/...` redirect that the image's `dash`
  shell doesn't support, so it reported unhealthy forever; it now runs under `bash`.
  `settings.json` lived only in the container's writable layer and reset to `{}` on every
  recreate — it's now persisted to a named volume. A dead process's own "No such
  process" diagnostic output no longer leaks into the app's logs on every liveness
  recheck.

- **Live sessions no longer lose messages written after a stale checkpoint**
  ([`29e8682`](https://github.com/delexw/claude-code-trace/commit/29e8682)).
  A `last-prompt` checkpoint's `leafUuid` hint is stamped at prompt-submit time, but if
  the conversation kept going live in the same file afterward, trusting the hint as the
  tip cut off everything written after it. The parser now falls back to the true last
  leaf entry when the hint has children.

- **Sonnet 5 messages with inlined system-reminders**
  ([`7c39e04`](https://github.com/delexw/claude-code-trace/commit/7c39e04)).
  Claude Code v2.1.201 started inlining harness reminders as `<system-reminder>` tags at
  the start of user entries in Sonnet 5 sessions instead of using dedicated entries. The
  parser was treating any such entry as noise and silently dropping the user's actual
  message; mixed entries are now classified correctly with the reminder stripped.

- **Stale libc constraints removed from the lockfile**
  ([`c00dd36`](https://github.com/delexw/claude-code-trace/commit/c00dd36)).
  `package-lock.json` carried redundant `libc` platform tags on several `@oxfmt`/`@oxlint`
  native-binary optional dependencies, which could make `npm ci` reject a valid platform
  match on some Linux base images. The tags are removed.

[0.12.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.12.0

## [0.11.0] — 2026-07-10

This release turns the viewer into a launchpad: you can resume, fork, or jump to the
terminal window of the session you're reading, and the picker can preview each session
by its end-of-session recap. It also fixes toolbar actions that silently did nothing,
keeps the permission-mode badge correct for Claude Code v2.1.200+, and corrects cost
estimates for sessions running Sonnet 5.

### Added

- **Resume, fork, or focus the session you're viewing**
  ([`5fe375c`](https://github.com/delexw/claude-code-trace/commit/5fe375c), @michaelstingl).
  A new session-control panel in the session view: Resume and Fork copy a ready-to-paste
  `cd '<cwd>' && claude --resume <id>` command (Fork adds `--fork-session`), and Focus —
  on a local macOS backend — brings the session's terminal window to the front. Sessions
  also get a live busy/idle badge read from Claude Code's `~/.claude/sessions` registry,
  replacing the transcript-derived "active" indicator. The TUI mirrors the liveness badge
  and a copy-resume key binding.

- **End-of-session recap as the picker preview**
  ([`0904952`](https://github.com/delexw/claude-code-trace/commit/0904952), @michaelstingl).
  When a session ends with a recap, the picker now shows that recap as the session's
  preview instead of the first message, so you can tell what a session accomplished at a
  glance. A SESSION PREVIEW toggle in Settings turns it off if you prefer the old
  first-message preview (default on).

- **Toolbar actions scoped to the view they act on**
  ([`b4483a7`](https://github.com/delexw/claude-code-trace/commit/b4483a7), @michaelstingl).
  Top and Bottom now actually scroll in every view — they previously targeted a
  non-scrolling container and did nothing. Expand All / Collapse All no longer appear in
  the picker, where nothing is collapsible, and the content actions moved to the right
  cluster, above the column they operate on.

### Fixed

- **Sonnet 5 sessions no longer overestimate cost**
  ([`7396d03`](https://github.com/delexw/claude-code-trace/commit/7396d03)). Sonnet 5
  ships with promotional pricing ($2/Mtok input, $10/Mtok output through Aug 31, 2026),
  but the cost calculator was applying the standard $3/$15 Sonnet rate — inflating every
  `claude-sonnet-5` session's cost by roughly a third. A dedicated pricing tier now
  matches any `sonnet-5` model ID before the generic Sonnet fallback.

- **Spurious permission-mode badge on Claude Code v2.1.200+ sessions**
  ([`17344f0`](https://github.com/delexw/claude-code-trace/commit/17344f0)). Claude Code
  v2.1.200 renamed the default permission mode written to session files from "default"
  to "manual", which made every normal session show a permission-mode badge in the info
  bar with a raw-string label. Both values are now recognised as the badge-free default.

[0.11.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.11.0

## [0.10.0] — 2026-07-01

This release keeps the JSONL parser current with another run of Claude Code releases
(v2.1.186 through v2.1.195 — `/rewind`, background-subagent permission attribution, and
background-agent format changes), fixes a live-chain cycle that could drop a session's
entire history, and follows up 0.9.0's performance work with virtualized message lists,
lower memory usage on large sessions, and a matching detail-view fix in the TUI.

### Added

- **Session picker titles from `/rename`**
  ([`2c63774`](https://github.com/delexw/claude-code-trace/commit/2c63774), @michaelstingl).
  Sessions renamed with Claude Code's `/rename` command now show that name as the title in
  the picker, joined from the live `~/.claude/sessions` registry by session ID. Falls back
  to the first message when a session hasn't been renamed.

### Fixed

- **`/rewind` support (Claude Code v2.1.191)**
  ([`04b813d`](https://github.com/delexw/claude-code-trace/commit/04b813d)). Resuming a
  conversation from before a `/clear` via `/rewind` produces `rewind-pointer` entries and
  `rewindable`/`checkpointUuid` markers on compaction checkpoints; both are now parsed and
  the structural markers are filtered from the display so a rewound session renders
  cleanly instead of showing raw pointer entries.

- **Background-agent JSONL format changes (Claude Code v2.1.195)**
  ([`1d0a1f4`](https://github.com/delexw/claude-code-trace/commit/1d0a1f4)). SDK and
  background-agent sessions gained new top-level fields (schema version, entrypoint,
  agent ID, and a `last-prompt` checkpoint type). These are now parsed with safe
  defaults for older sessions, and `last-prompt` checkpoints are discarded instead of
  falling through the classifier without a role.

- **Background subagent permission-prompt attribution (Claude Code v2.1.186)**
  ([`acdbf75`](https://github.com/delexw/claude-code-trace/commit/acdbf75)). Background
  subagents now surface permission prompts directly in the main session instead of
  auto-denying, and each prompt carries which subagent requested it. The viewer parses
  the new attribution fields and shows a "Requesting Agent" section in the detail view
  when present.

- **Auto-mode denials no longer vanish from the transcript (Claude Code v2.1.193)**
  ([`552bb59`](https://github.com/delexw/claude-code-trace/commit/552bb59)). Tool calls
  denied automatically in auto/plan mode were silently dropped by the noise filter
  instead of being shown. Denial entries — both top-level and the kind embedded inside
  progress entries — are now rescued and rendered as a system notice naming the tool and
  the reason it was denied.

- **Virtualized message list and lower memory use on large sessions**
  ([`800fd01`](https://github.com/delexw/claude-code-trace/commit/800fd01)). Sessions
  with heavy tool output (100MB+ transcripts) could grow the app's memory well beyond
  the file size because the message list held every message and its full tool
  input/output at once, and the desktop webview leaked render-tree memory across
  repeated session switches. The message list is now virtualized and fetched in pages
  that evict as you scroll, the detail view re-parses on demand instead of caching a
  heavy build, and the desktop app periodically reloads its webview to reclaim memory
  WebKit doesn't release on its own. Also fixes a stale row staying hover-highlighted
  after scrolling stops, caches the session-name registry read, and adds a font-size
  setting.

- **TUI detail view, live SSE updates, and a live-chain history bug**
  ([`1e92f8c`](https://github.com/delexw/claude-code-trace/commit/1e92f8c)). Following
  the virtualization work above: the TUI's detail view now fetches the full message
  on demand instead of showing empty tool bodies from the lightened list payload, and
  it re-fetches on the live-update signal instead of trusting a `messages` field the
  backend no longer sends (which was silently emptying the view). Fixes a Detail-view
  CSS flash and switches JSON/tool output to syntax-highlighted, word-wrapped text
  instead of horizontally-scrolling code blocks. Separately, a real session was found
  where an auto-compaction boundary pointed back into its own post-compaction
  descendants, closing a cycle that made the parser truncate the entire session history
  at that point — the resolver now falls back to the nearest earlier entry when this
  cycle is detected. Also fixes `cctrace --tui` orphaning its backend process group on
  exit, which could break a later `cctrace --web`.

[0.10.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.10.0

## [0.9.1] — 2026-06-27

A maintenance release that repairs two regressions from the 0.9.0 WSL and packaging
work: the WSL distro settings now actually save on the desktop build, and `install.sh`
no longer fails on setups that don't carry the legacy `tui/` directory.

### Fixed

- **WSL distro settings now save on the desktop build**
  ([`2917f29`](https://github.com/delexw/claude-code-trace/commit/2917f29), @RobotHanzo).
  The WSL discovery commands shipped in 0.9.0 were wired into the Tauri handler but never
  granted in the ACL permission set, so on the desktop build saving the Settings modal
  failed with "Command set_wsl_distros not allowed by ACL" and the distro listing was
  silently blocked. The commands are now ACL-granted, and a regression test cross-checks
  every registered command against the permission set so a missing grant fails CI instead
  of at runtime.

- **`install.sh` no longer breaks when the `tui/` directory is absent**
  ([`a6ec94e`](https://github.com/delexw/claude-code-trace/commit/a6ec94e), @DoTTak). The
  installer unconditionally `cd`'d into `tui/` to build the TUI, which aborted the install
  on checkouts that no longer carry that directory (the TUI moved to `tui-py/`). The build
  step is now guarded — it runs when `tui/` exists and prints a skip notice otherwise.

## [0.9.0] — 2026-06-26

This release widens where the viewer looks for sessions and sharpens how it shows what
Claude changed. Windows users running Claude Code inside WSL can now surface those
Linux-side sessions, Edit tool calls render as a proper colour-coded diff instead of raw
JSON, and the JSONL parser keeps pace with another run of Claude Code releases
(v2.1.178 through v2.1.183).

### Added

- **WSL session discovery**
  ([`6912a6d`](https://github.com/delexw/claude-code-trace/commit/6912a6d), @RobotHanzo).
  On Windows, Claude Code running inside a WSL distribution stores its projects under the
  Linux home, out of reach of the host viewer. Trace can now opt in to discovering those
  sessions: detected distributions appear as checkboxes in Settings, and the ones you
  enable are folded into the project list alongside your host projects. All WSL access is
  Windows-gated, so other platforms are unaffected.

- **Structured diff view for Edit tool calls**
  ([`6a6c2d9`](https://github.com/delexw/claude-code-trace/commit/6a6c2d9)). Edit tool
  inputs used to render as raw `old_string` / `new_string` JSON. They now display as a
  colour-coded diff — unchanged context lines are preserved, removed and added lines are
  marked with `-`/`+`, and only the words that actually changed are highlighted within a
  line. Both the web UI and the TUI get the new rendering.

### Fixed

- **Thinking-only and re-prompt entries from Claude Code v2.1.183**
  ([`2c69c85`](https://github.com/delexw/claude-code-trace/commit/2c69c85)). v2.1.183
  emits assistant entries that carry only thinking content, plus `isMeta`-flagged user
  entries for re-prompts. These previously tripped up the parser; they are now recognised
  and the transcript renders without gaps.

- **Partial assistant entries with missing usage and token fields**
  ([`d0581ee`](https://github.com/delexw/claude-code-trace/commit/d0581ee)). Some
  assistant entries arrive mid-stream with null `usage` and token fields. The parser now
  tolerates these instead of failing on them, so a live transcript no longer breaks while
  a partial entry is still being written.

- **Implicit team discovery for Claude Code v2.1.178**
  ([`945deb4`](https://github.com/delexw/claude-code-trace/commit/945deb4)). v2.1.178
  stopped writing the team header that discovery relied on, so sub-agent and team sessions
  silently disappeared. Discovery now uses `agentName` as the primary signal and finds
  those sessions again.

[0.9.1]: https://github.com/delexw/claude-code-trace/releases/tag/v0.9.1
[0.9.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.9.0

## [0.8.0] — 2026-06-16

This release keeps the JSONL parser in step with another run of Claude Code releases
(v2.1.157 through v2.1.174): deeper sub-agent nesting, sessions that change directory or
switch worktrees partway through, Workflow sub-agents that didn't always announce
themselves, and newer model IDs like Claude Fable 5. The project tree also stops
stranding worktree-only sessions at the top level.

### Added

- **Five-level sub-agent nesting**
  ([`beb3a61`](https://github.com/delexw/claude-code-trace/commit/beb3a61)). Claude Code
  v2.1.172 lets sub-agents spawn sub-agents up to five levels deep. The `Entry` struct now
  captures the optional `agentDepth` and `parentAgentName` fields those sidechain entries
  carry, so depth and attribution survive into the transcript instead of being dropped, and
  the main-session chain is still isolated correctly at every level.

### Fixed

- **Session picker froze on the starting directory after a worktree switch**
  ([`6aa548d`](https://github.com/delexw/claude-code-trace/commit/6aa548d)). Session
  metadata took the working directory and git branch from the first entry only, so after an
  `EnterWorktree` switch mid-session (v2.1.157+) the picker kept showing where the session
  started rather than where it ended up. Both fields now follow the last-seen value.

- **Mid-session `/cd` changes and the `Cd` tool weren't reflected**
  ([`3295fc6`](https://github.com/delexw/claude-code-trace/commit/3295fc6)). Directory
  changes from `/cd` (v2.1.169+) now update the InfoBar and session picker, and `Cd` tool
  calls render with a tool-category icon and a short directory marker in the transcript
  timeline instead of going uncategorised.

- **Workflow sub-agent sessions were silently skipped**
  ([`e2faaff`](https://github.com/delexw/claude-code-trace/commit/e2faaff)). Before
  v2.1.174, `agent()` sub-agents from the Workflow tool didn't write attribution headers on
  their first line, so team-session discovery returned empty and dropped them. Discovery now
  scans every line for attribution, making those historical sub-agent sessions visible when
  any entry carries it.

- **Model IDs with bracket suffixes or dates rendered awkwardly**
  ([`ae7c0ec`](https://github.com/delexw/claude-code-trace/commit/ae7c0ec)). Model labels
  now strip bracket context suffixes (e.g. `[1m]`/`[1M]` from pre-v2.1.173 sessions) and
  drop `YYYYMMDD` date components before shortening, so `claude-fable-5-20261001[1m]` shows
  as `fable5` while existing names like `haiku4.5` are unchanged.

- **Worktree-only sessions stranded at the top of the project tree**
  ([`d3aba67`](https://github.com/delexw/claude-code-trace/commit/d3aba67)). Sessions that
  only ever ran inside per-item worktrees (never at the repo root) rendered as flat
  top-level nodes. The tree now synthesizes a repo-root node for them so they nest under
  their repository in a `CLAUDE-WORKTREES` group; runs that already have a real anchor are
  unchanged.

## [0.7.0] — 2026-06-08

This release is mostly about keeping the JSONL parser in lockstep with a fast run of
Claude Code releases (v2.1.145 through v2.1.166) so newer transcripts stop silently
losing content or rendering blank turns. Alongside the compat work, the macOS desktop
install now produces a real `.app` bundle instead of a window that launches blank, and
turn detail reads in chronological order instead of a flattened wall of text.

### Added

- **`background_tasks` and `session_crons` hook fields**
  ([`da22c00`](https://github.com/delexw/claude-code-trace/commit/da22c00)). Stop and
  SubagentStop hook payloads in Claude Code v2.1.145+ carry two new arrays — running
  background-task descriptors and session-scoped cron jobs. Both are now captured on the
  `Entry` struct instead of being dropped, so they survive into the transcript view.

### Fixed

- **macOS install launches a blank white window**
  ([`2752850`](https://github.com/delexw/claude-code-trace/commit/2752850)). `cargo install`
  produced a bare Mach-O binary with no `.app` wrapper or `Info.plist`, so the webview came
  up blank. The macOS installer now builds a proper `.app` bundle via `tauri build`,
  installs it to `/Applications`, and removes any stale cargo binary left on `PATH`.
  Linux and other platforms keep the existing cargo-install path.

- **Turn detail rendered out of order**
  ([`2752850`](https://github.com/delexw/claude-code-trace/commit/2752850)). The detail view
  flattened every assistant text block into one blob at the top, then repeated the same text
  as collapsed Output items below — so commentary and the final result read as a jumbled
  wall. Output prose now renders inline and always-visible, interleaved with tool calls in
  chronological order, with the duplicated top blob suppressed. Web and the Python TUI now
  match.

- **Blank AI turns from fallback-model retries**
  ([`dbb7f5b`](https://github.com/delexw/claude-code-trace/commit/dbb7f5b)). Claude Code
  v2.1.166's `fallbackModel` writes a partial assistant entry with null/empty content before
  the successful fallback response. These stubs surfaced as empty AI turns and confused
  chunk aggregation. The parser now drops assistant entries with null or empty content, while
  the real fallback response (and its actual model ID, e.g. `claude-haiku-4-5`) passes through
  unaffected.

- **Hook `additionalContext` was silently dropped**
  ([`e03684b`](https://github.com/delexw/claude-code-trace/commit/e03684b)). Stop /
  SubagentStop hook entries in v2.1.163 carry `hookSpecificOutput.additionalContext` at the
  top level, which serde discarded. It is now captured as hook metadata and shown in the
  transcript's hook panel.

- **Cache-write token counts showed 0 on v2.1.152+**
  ([`e7e3a88`](https://github.com/delexw/claude-code-trace/commit/e7e3a88)). The parser only
  read the flat `cache_creation_input_tokens` field; v2.1.152+ reports cache-write tokens
  nested under `cache_creation.input_tokens`. Both formats are now read (taking the larger of
  the two), so cache-write counts are correct for recent sessions and still backward-compatible.

- **v2.1.154 dynamic-workflow entries broke parsing**
  ([`2e37441`](https://github.com/delexw/claude-code-trace/commit/2e37441)). Dynamic workflows
  introduced new entry types (`workflow-start`/`-progress`/`-complete`/`-cancelled`/`-error`)
  and `workflow*` state fields. The five lifecycle types are now discarded as noise and the
  state fields captured, instead of hitting the role-based fallback or tripping
  `deny_unknown_fields`.

- **`.meta.json` parsing fragile to schema changes**
  ([`3fc65ba`](https://github.com/delexw/claude-code-trace/commit/3fc65ba)). Sidecar metadata
  is now parsed through a typed `SidecarMeta` struct where every field is optional or
  `serde(default)`, so sessions written before (or after) a field was added keep parsing
  instead of failing.

- **Tool results not pretty-printed**
  ([`b7ea662`](https://github.com/delexw/claude-code-trace/commit/b7ea662)). JSON tool results
  now render as formatted code blocks — using the backend-parsed `tool_result_json` when
  available, otherwise attempting a JSON parse and falling back to plain text — matching how
  tool inputs already display, in both web/desktop and the Python TUI.

## [0.6.0] — 2026-05-21

A release that rewrites the terminal UI from scratch in Python/Textual, fixes the JSONL
parser against two real-world Claude Code data shapes that used to silently drop content,
and finally lands a viewport-driven picker that updates the cards you're actually looking
at instead of polling the whole list. Docker headless mode also gets a long-overdue CPU
cut after the WebKit and SSE-flooding root causes were untangled.

### Added

- **Python/Textual TUI replaces the Ink/React TUI**
  ([`b21753a`](https://github.com/delexw/claude-code-trace/commit/b21753a)). The terminal UI
  now lives under `tui-py/` and runs via `python3 tui-py/main.py` (also wired into
  `cctrace --tui` and `npm run dev:tui`). A shared `HighlightListView` base owns selection
  styling and cursor-init for the picker, message list, and detail-view items list — fixing
  the inconsistencies the old Ink implementation accumulated. Rapid SSE updates and
  load-session resets are now serialised through an exclusive worker group so the message
  list no longer races with its own deferred `clear()` calls and duplicates rows. The
  picker stays visible with a `LoadingIndicator` overlay until the message list is fully
  populated, eliminating the few-second window where j/k against an empty list pane felt
  broken.

- **Viewport-driven picker refresh in the web frontend**
  ([`276f438`](https://github.com/delexw/claude-code-trace/commit/276f438)). The picker
  wraps every session card in a shared `IntersectionObserver` (`useVisibleSessions` hook)
  and re-fetches the session list whenever the visible set changes — debounced 150 ms —
  plus a 2 s heartbeat while any cards are on screen. Cards the user is actually looking at
  stay fresh without paying the cost of polling everything, and ongoing-status / token-count
  badges update much closer to real time.

- **`effort` and `terminalSequence` fields for Claude Code v2.1.133+ / v2.1.141+**
  ([`512f9e1`](https://github.com/delexw/claude-code-trace/commit/512f9e1)). Claude Code
  v2.1.133 injects an `effort: {level}` payload into hook input JSON and v2.1.141 adds
  `terminalSequence` to hook output entries. Both are now captured on the `Entry` struct
  instead of being silently dropped by serde's default unknown-field behaviour, so future
  inspectors can read them.

- **Desktop and macOS Tauri permission schemas**
  ([`0af3f9a`](https://github.com/delexw/claude-code-trace/commit/0af3f9a)). Adds
  `(allow|deny)-supports-multiple-windows`, `(allow|deny)-set-icon-with-as-template`, and
  per-window `activity-name` / `scene-identifier` permissions so capability files can opt
  into the newer Tauri 2.11 surface explicitly.

### Fixed

- **JSONL parser: deduplicate summary entries from pre-v2.1.128 sessions**
  ([`aa22a61`](https://github.com/delexw/claude-code-trace/commit/aa22a61)). Pre-v2.1.128
  Claude Code wrote duplicate `summary`-type JSONL entries when sub-agents were idle,
  causing the same summary state to be re-emitted on every tick. That surfaced as duplicate
  CompactMsg blocks in the UI, inflated token counts, and potential conversation-tree
  corruption. `read_session_incremental` now tracks a `HashSet` of
  `(agentName, teamName, summary_text)` triples and keeps only the first occurrence.

- **JSONL parser: sanitize lone UTF-16 surrogates**
  ([`833e266`](https://github.com/delexw/claude-code-trace/commit/833e266)). Sessions
  written by Claude Code before v2.1.132 occasionally split a multi-byte emoji across the
  tool-error truncation boundary, leaving a lone `\uD83D` without its matching low
  surrogate. `serde_json` rejects those per RFC 8259 and silently discarded the offending
  lines. A new `sanitize_lone_surrogates()` step replaces unpaired surrogates with `�`
  before the deserializer sees them, while valid surrogate pairs pass through untouched.
  Allocation is deferred to `Cow::Borrowed` for the common no-surrogate case.

- **Headless Docker CPU and SSE traffic**
  ([`fea185e`](https://github.com/delexw/claude-code-trace/commit/fea185e)). Two unrelated
  costs were stacking: `--headless` still booted `tauri::Builder` (spawning WebKit's web
  and network processes plus Xvfb under docker-entrypoint) and the picker watcher fanned
  the full session list to every SSE client on every inotify event. With 2000+ session
  files this produced continuous high CPU and megabytes-per-second of SSE traffic.
  `--headless` now returns early with just a tokio runtime + axum server, Xvfb is skipped,
  and the watcher emits a lightweight `{}` signal that clients turn into a single cached
  `list_sessions` re-fetch (debounce raised from 300 ms to 1 s).

- **Async handlers in `useTauriEvent` and `useSSE`**
  ([`f7d3c8d`](https://github.com/delexw/claude-code-trace/commit/f7d3c8d)). The two
  subscription hooks now accept `async` listeners, so callers can `await` inside an event
  handler without React swallowing the returned Promise. Paired with
  ([`bc173bd`](https://github.com/delexw/claude-code-trace/commit/bc173bd)) and
  ([`b76c8d5`](https://github.com/delexw/claude-code-trace/commit/b76c8d5)), every unhandled
  async inner call across the codebase is now either properly awaited, `void`-discarded,
  or `.catch()`-attached — eliminating the silent unhandled-promise-rejection class.

- **Picker auto-detect for new project folders**
  ([`270e717`](https://github.com/delexw/claude-code-trace/commit/270e717),
  [`2007752`](https://github.com/delexw/claude-code-trace/commit/2007752)). On a
  picker-refresh signal the frontend now also re-derives the project-directory set, so a
  newly-created `~/.claude/projects/<slug>/` directory shows up without restarting the app.

- **TUI picker auto-refresh**
  ([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5)). The terminal
  UI subscribed to a non-existent `picker-update` event while the backend emits
  `picker-refresh` with an empty payload. The TUI picker never auto-updated when sessions
  changed on disk — it only refreshed when the user re-entered the picker view. The TUI
  now subscribes to `picker-refresh` and re-fetches via `api.discoverSessions(dirs)`,
  mirroring the web frontend pattern in `src/hooks/usePicker.ts`.

- **Web picker re-fetch on `picker-refresh` signal**
  ([`01f8212`](https://github.com/delexw/claude-code-trace/commit/01f8212)). Memoise the
  most recent `projectDirs` in a ref so the SSE handler can re-issue `discover_sessions`
  without the caller threading state through.

[0.8.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.8.0
[0.7.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.7.0
[0.6.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.6.0
