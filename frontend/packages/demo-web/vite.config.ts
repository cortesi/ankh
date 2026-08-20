import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The demo backend (ankh-demo) the dev server proxies API and admin traffic to.
const backendTarget = process.env.ANKH_DEMO_BACKEND_URL ?? "http://127.0.0.1:8080";

export default defineConfig({
  // Consume the workspace ankh packages from source so edits are picked up without a rebuild.
  optimizeDeps: {
    exclude: ["@ankh/auth-react", "@ankh/types", "@ankh/ui"],
  },
  plugins: [react()],
  build: {
    // Emit straight into the ankh-demo crate so the Rust server can serve the built SPA.
    outDir: "../../../crates/ankh-demo/dist",
    emptyOutDir: true,
  },
  server: {
    proxy: {
      "/api": backendTarget,
      "/admin": backendTarget,
    },
  },
});
