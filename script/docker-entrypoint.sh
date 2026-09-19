#!/bin/sh
set -e

# Compose mounts the optional Jev token from a Docker-managed volume. Load it
# inside the container so it never crosses the browser or appears in the
# Compose service environment shown by `docker inspect`.
if [ -z "${JEV_API_KEY:-}" ] && [ -n "${JEV_API_KEY_FILE:-}" ] && [ -r "$JEV_API_KEY_FILE" ]; then
    JEV_API_KEY=$(cat "$JEV_API_KEY_FILE")
    export JEV_API_KEY
fi

# In headless mode Tauri/WebKit is bypassed entirely, so no display is needed.
for arg in "$@"; do
    [ "$arg" = "--headless" ] && exec "$@"
done

: "${DISPLAY:=:99}"
export DISPLAY
exec xvfb-run --auto-servernum --server-args="-screen 0 1024x768x24" "$@"
