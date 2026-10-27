import { resolve } from "node:path";
import preact from "@preact/preset-vite";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

// Four entry points share the same components: the app window (index.html), the search panel (search.html), the
// password window (unlock.html) and the local browser management page served by the core (gate.html). The menu
// bar menu is drawn by the system, not by a page.
export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        gate: resolve(import.meta.dirname, "gate.html"),
        search: resolve(import.meta.dirname, "search.html"),
        unlock: resolve(import.meta.dirname, "unlock.html"),
      },
    },
  },
  test: {
    environment: "happy-dom",
    setupFiles: ["src/test-setup.ts"],
    include: ["src/**/*.test.tsx", "src/**/*.test.ts"],
  },
});
