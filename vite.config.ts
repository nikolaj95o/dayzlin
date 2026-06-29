import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// @tauri-apps/cli sets TAURI_DEV_HOST when running on a physical device.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/ — tuned for Tauri (see https://v2.tauri.app/start/frontend/vite/)
export default defineConfig({
  plugins: [svelte()],
  // Tauri expects a fixed port and surfaces Rust errors, so don't clear the screen.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    // Don't let Vite watch the Rust side.
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
