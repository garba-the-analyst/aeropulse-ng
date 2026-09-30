import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Vite 8 warns `__dirname` is unsupported under future native loader, but the
// classic loader (current default) still shims it to the real src/ dir while
// import.meta.url points at a .vite-temp copy. Keep __dirname for correct
// resolution; suppress the future-loader warning via env in package.json.
declare const __dirname: string;
process.env.VITE_CONFIG_NATIVE_IGNORE_WARNING ??= "true";

const host = process.env.TAURI_DEV_HOST;

const dirname: string = __dirname;

export default defineConfig({
  // Config is location-independent: Tauri invokes it via the root
  // package.json scripts while assets live beside the sources.
  root: dirname,
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
    alias: { "@": path.resolve(dirname, "./src") },
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
    outDir: path.resolve(dirname, "../dist"),
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: path.resolve(dirname, "index.html"),
        hud: path.resolve(dirname, "hud.html"),
      },
    },
  },
});
