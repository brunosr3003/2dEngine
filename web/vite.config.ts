import { defineConfig } from "vite";
import { resolve } from "path";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:3020",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main:  resolve(__dirname, "index.html"),
        pixel: resolve(__dirname, "pixel.html"),
        econ:  resolve(__dirname, "econ.html"),
      },
    },
  },
});
