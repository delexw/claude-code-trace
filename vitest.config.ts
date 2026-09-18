import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

/** `src/lib/apiToken.ts` imports a virtual module that only the dev-server
 * plugin in vite.config.ts provides. Under vitest it is always empty, exactly
 * like a production build. */
const apiTokenStub = {
  name: "cctrace-api-token-stub",
  resolveId: (id: string) =>
    id === "virtual:cctrace-api-token" ? "\0virtual:cctrace-api-token" : undefined,
  load: (id: string) => (id === "\0virtual:cctrace-api-token" ? 'export default "";' : undefined),
};

export default defineConfig({
  plugins: [react(), apiTokenStub],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: [
      "src/**/*.test.{ts,tsx}",
      "shared/**/*.test.{ts,tsx}",
      "bin/**/*.test.mjs",
      "script/**/*.test.mjs",
    ],
  },
});
