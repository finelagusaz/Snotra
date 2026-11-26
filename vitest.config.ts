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
    // 本物の git を呼ぶテストは、CI（windows-latest）で数十秒続く遅い時間帯に当たると、中央値 1 s 前後のものが
    // 5〜20 s へ伸びる。vitest の既定 5 s では、`{ timeout: T }` を付け忘れたテストが偶発的に落ちる（2026-11 に 98 ジョブ中 5 件）。
    // 個別の T（20 s・60 s）は付けたまま残し、付け忘れの受け皿としてここで既定を上げる
    testTimeout: 20_000,
  },
});
