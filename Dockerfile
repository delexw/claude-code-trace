# syntax=docker/dockerfile:1.7

# =============================================================================
# Claude Code Trace — Docker image
#
# Runs the Rust/axum backend in headless mode behind a virtual X display
# (Tauri v2's webview runtime on Linux links against webkit2gtk and needs a
# display even when no window is shown). The React frontend is built to a
# static bundle and served from the same axum process as an API fallback, so
# the whole app is reachable on a single port.
#
# Build:
#   docker build -t claude-code-trace .
#
# Run (mount your Claude Code session data read-only):
#   docker run --rm -p 1421:1421 \
#     -v "$HOME/.claude:/home/app/.claude:ro" \
#     claude-code-trace
#
# Then open http://localhost:1421 in a browser.
#
# Configurable env vars:
#   CCTRACE_HTTP_HOST   bind host    (default: 0.0.0.0 in this image)
#   CCTRACE_HTTP_PORT   bind port    (default: 1421 in this image)
#   CCTRACE_STATIC_DIR  static dist  (default: /app/dist in this image)
# =============================================================================

ARG RUST_IMAGE=rust:latest
ARG NODE_VERSION=24
ARG DEBIAN_CODENAME=bookworm

# -----------------------------------------------------------------------------
# Stage 1 — build the React frontend
# -----------------------------------------------------------------------------
FROM node:${NODE_VERSION}-${DEBIAN_CODENAME}-slim AS frontend-builder

WORKDIR /build

COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY tsconfig.json tsconfig.node.json vite.config.ts index.html ./
COPY src ./src
COPY shared ./shared

# Empty VITE_API_BASE → frontend uses relative URLs, matching the single-port
# axum server that also serves these static assets.
ENV VITE_API_BASE=""
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2 — build the Rust backend (Tauri v2 linking needs webkit2gtk headers)
# -----------------------------------------------------------------------------
FROM ${RUST_IMAGE} AS backend-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libwebkit2gtk-4.1-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        libxdo-dev \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Tauri's generate_context!() embeds frontendDist at compile time, so the
# frontend bundle must exist before `cargo build`.
COPY src-tauri ./src-tauri
COPY --from=frontend-builder /build/dist ./dist

WORKDIR /build/src-tauri
RUN cargo build --release --locked --bin claude-code-trace

# -----------------------------------------------------------------------------
# Stage 3 — runtime image
# -----------------------------------------------------------------------------
FROM debian:trixie-slim AS runtime

# Runtime deps:
#   * webkit2gtk + friends — required by the Tauri v2 runtime on Linux even
#     in headless mode (the window is created but not shown).
#   * xvfb               — provides a virtual X display for webkit2gtk.
#   * dumb-init          — PID 1 that forwards signals (SIGTERM/SIGINT) so
#                          `docker stop` shuts cleanly down.
#   * ca-certificates    — outbound TLS (e.g. for future features).
RUN apt-get update && apt-get install -y --no-install-recommends \
        libwebkit2gtk-4.1-0 \
        libayatana-appindicator3-1 \
        librsvg2-2 \
        libxdo3 \
        xvfb \
        xauth \
        dumb-init \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --home-dir /home/app --shell /bin/bash --uid 1000 app

WORKDIR /app

COPY --from=backend-builder /build/src-tauri/target/release/claude-code-trace /usr/local/bin/claude-code-trace
COPY --from=frontend-builder /build/dist /app/dist
COPY script/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENV CCTRACE_HTTP_HOST=0.0.0.0 \
    CCTRACE_HTTP_PORT=1421 \
    CCTRACE_STATIC_DIR=/app/dist \
    XDG_CONFIG_HOME=/home/app/.config \
    XDG_DATA_HOME=/home/app/.local/share

USER app

# Mountpoint for the host's ~/.claude directory — session JSONL files live here.
VOLUME ["/home/app/.claude"]

EXPOSE 1421

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD /bin/sh -c 'exec 3<>/dev/tcp/127.0.0.1/${CCTRACE_HTTP_PORT:-1421}' || exit 1

ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/docker-entrypoint.sh"]
CMD ["claude-code-trace", "--headless"]
