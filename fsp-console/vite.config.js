import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: false,
    proxy: {
      "/mfa-api": {
        target: "http://127.0.0.1:1025",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/mfa-api/, ""),
      },
    },
  },
  preview: {
    port: 5173,
    proxy: {
      "/mfa-api": {
        target: "http://127.0.0.1:1025",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/mfa-api/, ""),
      },
    },
  },
  build: {
    outDir: "dist",
  },
});
