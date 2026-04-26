#!/bin/bash
# Build the frontend, rebuild the Docker image, and restart via docker-compose.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building frontend..."
npm run build

echo "==> Stopping existing containers..."
docker compose down --remove-orphans 2>/dev/null || true
# Also stop any manually-started container occupying the target port
PORT="${CCTRACE_HOST_PORT:-1421}"
EXISTING=$(docker ps -q --filter "publish=${PORT}" 2>/dev/null)
if [ -n "$EXISTING" ]; then
  echo "    Stopping container(s) on port ${PORT}: $EXISTING"
  docker stop $EXISTING >/dev/null
  docker rm $EXISTING >/dev/null 2>&1 || true
fi

echo "==> Building Docker image (no cache on --fresh)..."
if [[ "${1:-}" == "--fresh" ]]; then
  docker compose build --no-cache
else
  docker compose build
fi

echo "==> Starting container..."
docker compose up -d

echo "==> Waiting for service to be ready..."
for i in $(seq 1 20); do
  if curl -sf "http://localhost:${PORT}/" >/dev/null 2>&1; then
    echo "==> Service is up."
    break
  fi
  if [ "$i" -eq 20 ]; then
    echo "WARNING: service not reachable after 20s — check logs with: docker compose logs"
  fi
  sleep 1
done

echo "==> Done. Running at http://localhost:${CCTRACE_HOST_PORT:-1421}"
