import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  test: {
    include: ["ui/src/**/*.test.{ts,tsx}"],
    environment: "node",
  },
});
