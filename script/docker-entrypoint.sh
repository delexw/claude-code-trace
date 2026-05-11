#!/bin/sh
set -e

# In headless mode Tauri/WebKit is bypassed entirely, so no display is needed.
for arg in "$@"; do
    [ "$arg" = "--headless" ] && exec "$@"
done

: "${DISPLAY:=:99}"
export DISPLAY
exec xvfb-run --auto-servernum --server-args="-screen 0 1024x768x24" "$@"
