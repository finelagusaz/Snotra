import { defineConfig } from "vitest/config";

// フロント（ui/）は #532 SU7 で撤去済み。残る検査対象はセーフティネット
// （.claude/hooks・.githooks・scripts）のみ——この include が CI の npm test のスコープを定義する。
export default defineConfig({
  test: {
    include: [
      ".claude/hooks/**/*.test.mjs",
      ".githooks/**/*.test.mjs",
      "scripts/**/*.test.mjs",
    ],
    environment: "node",
  },
});
