import path from "node:path";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// @tauri-apps/cli sets TAURI_DEV_HOST when running on a physical device.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/ — tuned for Tauri (see https://v2.tauri.app/start/frontend/vite/)
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: {
      $lib: path.resolve("./src/lib"),
    },
  },
  // Tauri expects a fixed port and surfaces Rust errors, so don't clear the screen.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    // Don't watch the Rust side or local build-output trees. `build-dir/` (the Flatpak builder
    // output) captures a sandbox filesystem with `var/run -> /run`, whose `udev/watch/*` circular
    // symlinks make chokidar throw ELOOP and kill the dev server — so it must be ignored.
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/build-dir/**",
        "**/.flatpak-builder/**",
      ],
    },
  },
});
