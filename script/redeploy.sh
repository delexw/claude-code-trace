#!/bin/bash
# Build the frontend, rebuild the Docker image, and restart via Docker Compose.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JEV_SECRET_VOLUME="${CCTRACE_JEV_SECRET_VOLUME:-claude-code-trace-secrets}"

ask_to_configure_jev_api_key() {
  local answer
  read -r -p "Configure or update the Jev API key for Docker? [y/N] " answer || return 1
  [[ "$answer" =~ ^[Yy]([Ee][Ss])?$ ]]
}

read_jev_api_key() {
  local api_key
  read -r -s -p "Jev API key: " api_key
  printf '\n' >&2
  if [[ -z "${api_key//[[:space:]]/}" ]]; then
    echo "ERROR: Jev API key cannot be empty." >&2
    return 1
  fi
  printf '%s' "$api_key"
}

store_docker_jev_api_key() {
  local api_key="$1"
  docker volume create "$JEV_SECRET_VOLUME" >/dev/null
  printf '%s' "$api_key" | docker run --rm -i --user 0 \
    -v "${JEV_SECRET_VOLUME}:/run/secrets" \
    --entrypoint /bin/sh \
    claude-code-trace:latest \
    -c 'umask 077; cat > /run/secrets/jev_api_key; chown 1000:1000 /run/secrets/jev_api_key; chmod 600 /run/secrets/jev_api_key'
  echo "==> Jev API key stored in Docker volume ${JEV_SECRET_VOLUME}."
}

main() {
  cd "$ROOT_DIR"

  echo "==> Building frontend..."
  npm run build

  echo "==> Stopping existing containers..."
  docker compose down --remove-orphans 2>/dev/null || true
  # Also stop any manually-started container occupying the target port
  local port="${CCTRACE_HOST_PORT:-1421}"
  local existing
  existing=$(docker ps -q --filter "publish=${port}" 2>/dev/null)
  if [ -n "$existing" ]; then
    echo "    Stopping container(s) on port ${port}: $existing"
    docker stop $existing >/dev/null
    docker rm $existing >/dev/null 2>&1 || true
  fi

  echo "==> Building Docker image (no cache on --fresh)..."
  if [[ "${1:-}" == "--fresh" ]]; then
    docker compose build --no-cache
  else
    docker compose build
  fi

  if [[ -t 0 ]] && ask_to_configure_jev_api_key; then
    local jev_api_key
    jev_api_key=$(read_jev_api_key)
    store_docker_jev_api_key "$jev_api_key"
    unset jev_api_key
  fi

  echo "==> Starting container..."
  docker compose up -d

  echo "==> Waiting for service to be ready..."
  local attempt
  for attempt in $(seq 1 20); do
    if curl -sf "http://localhost:${port}/" >/dev/null 2>&1; then
      echo "==> Service is up."
      break
    fi
    if [ "$attempt" -eq 20 ]; then
      echo "WARNING: service not reachable after 20s — check logs with: docker compose logs"
    fi
    sleep 1
  done

  echo "==> Done. Running at http://localhost:${port}"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
