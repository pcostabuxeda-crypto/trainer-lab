import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error sense @types/node no hi ha tipus per a node:process
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// Opcions pensades per a `tauri dev`: port fix i sense vigilar src-tauri
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/research/**"] },
  },
}));
