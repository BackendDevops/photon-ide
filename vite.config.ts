import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and no clearing of the screen on dev.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  // Produce a relative-path build so Tauri can load it from the bundle.
  base: "./",
  build: {
    target: "es2021",
    outDir: "dist",
    sourcemap: false,
  },
});
