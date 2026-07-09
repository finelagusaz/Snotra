import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid({ hot: false })],
  test: {
    include: [
      "ui/src/**/*.test.{ts,tsx}",
      ".claude/hooks/**/*.test.mjs",
      ".githooks/**/*.test.mjs",
    ],
    environment: "node",
  },
});
