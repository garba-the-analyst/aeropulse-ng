import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  // Config is location-independent: Tauri invokes it via the root
  // package.json scripts while assets live beside the sources.
  root: __dirname,
  plugins: [react()],
  // vitest config — cast to any to avoid vite/vitest type duel (vite 8 vs vitest 3)
  ...( {
    test: {
      environment: "jsdom",
      globals: true,
      include: ["**/*.{test,spec}.{ts,tsx}"],
    },
  } as any),
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  // Tauri dev-server hardening: bind strictly, no browser auto-open.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**", "**/python-sidecar/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : true,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    outDir: path.resolve(__dirname, "../dist"),
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        hud: path.resolve(__dirname, "hud.html"),
      },
    },
  },
});
