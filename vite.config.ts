import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { apiTokenPath, resolveApiToken } from "./bin/api-token.mjs";

const host = process.env.TAURI_DEV_HOST;

/**
 * Hand the browser UI the shared HTTP API client token in dev/web mode.
 *
 * `cctrace --web` serves the UI from Vite (port 1420) while the Rust API
 * listens on 11423, so the browser can't get the token as a same-origin
 * cookie the way the Docker bundle does. This plugin reads (or, on a first
 * run, creates) the same token file the backend uses — see
 * `bin/api-token.mjs` — and exposes it as `import.meta.env.VITE_API_TOKEN`
 * for `src/lib/apiToken.ts`.
 *
 * `apply: "serve"` is load-bearing: a production/Docker `vite build` must
 * never bake a token into the bundle. When the file changes on disk
 * (Settings → Regenerate rewrites it), the dev server restarts so a reload
 * picks up the new value.
 */
function apiTokenPlugin(): Plugin {
  return {
    name: "cctrace-api-token",
    apply: "serve",
    config: () => ({
      define: {
        "import.meta.env.VITE_API_TOKEN": JSON.stringify(resolveApiToken() ?? ""),
      },
    }),
    configureServer(server) {
      const path = apiTokenPath();
      server.watcher.add(path);
      server.watcher.on("change", (changed) => {
        if (changed === path) void server.restart();
      });
    },
  };
}

export default defineConfig(async () => ({
  plugins: [react(), apiTokenPlugin()],
  clearScreen: false,
  build: {
    chunkSizeWarningLimit: 1500,
  },
  server: {
    // VITE_PORT allows headless/TUI mode to use a different port to avoid
    // conflicting with an already-running web/desktop Vite instance.
    port: process.env.VITE_PORT ? parseInt(process.env.VITE_PORT) : 1420,
    strictPort: !process.env.VITE_PORT,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
