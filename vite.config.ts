import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [solid()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 5173,
    strictPort: true,
    // Required for SPA routing: serve index.html for all unknown paths.
    historyApiFallback: true,
    // Optional dev proxy. The app calls `/api` same-origin, so without this
    // `npm run dev` cannot reach a backend at all: session restore fails and you
    // are stuck on the login screen, unable to exercise any real page.
    //
    // Point `ECHO_DEV_API` at a reachable server to develop against it, e.g. an
    // ssh forward to the deploy VM:
    //   ssh -N -L 18080:localhost:8080 <user>@<vm>
    //   ECHO_DEV_API=http://localhost:18080 npm run dev
    //
    // ⚠️ That target is PRODUCTION. Read-only browsing is fine; do not exercise
    // destructive admin controls against it. Unset by default, so this is inert
    // unless someone opts in.
    proxy: process.env.ECHO_DEV_API
      ? {
          "/api": {
            target: process.env.ECHO_DEV_API,
            changeOrigin: true,
          },
        }
      : undefined,
    watch: {
      // 3. tell vite to ignore watching `src-tauri` and heavy non-source dirs
      ignored: [
        "**/src-tauri/**",
        "**/.cargo/**",
        "**/target/**",
        "**/.fastembed_cache/**",
        "**/models/**",
        "**/logs/**",
        "**/history/**",
        "**/.git/**",
        "**/node_modules/**",
        "**/echo.db*",
      ],
    },
  },
  // 3. to make use of `TAURI_ENV_DEBUG` and other env variables
  // https://tauri.app/v1/api/config/#buildconfig.beforedevcommand
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri uses Chromium on Windows and WebKit on macOS and Linux
    target:
      process.env.TAURI_ENV_PLATFORM === "windows"
        ? "chrome105"
        : "safari13",
    // don't minify for debug builds
    minify: !process.env.TAURI_ENV_DEBUG ? ("esbuild" as const) : false,
    // produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
}));
