import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  root: "ui",
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "esnext",
    outDir: "../dist",
  },
});
