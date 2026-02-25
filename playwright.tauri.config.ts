import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.e2e.ts",
  timeout: 90_000,
  expect: {
    timeout: 10_000,
  },
  workers: 1,
  fullyParallel: false,
  retries: 0,
  reporter: [["list"]],
});
