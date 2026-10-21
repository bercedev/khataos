import { defineConfig } from "vite";

// Tauri expects a fixed port in dev and plain ES2022 output.
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "es2022", outDir: "dist", emptyOutDir: true, sourcemap: false },
  // Screen class names survive minifying: crash reports list them (src/crash.ts).
  esbuild: { keepNames: true },
});
