//! G-hook-commands — PostToolUse hook の cargo コマンド ↔ docs/build-commands.md カテゴリ A の照合（#589）。
import { finding } from "../lib.mjs";

export const id = "G-hook-commands";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkHookCommands(snapshot);
}

// ---------------------------------------------------------------------------
// G-hook-commands — PostToolUse hook の cargo コマンド ↔ docs/build-commands.md カテゴリ A の照合（#589）。
// hook は触らない（非 export・import は main 実行の副作用があるため、ソーステキストから
// `cargoSpec([...])` を抽出する。抽出アンカーが hook のリファクタで腐ったら抽出 0 件 fail で
// 明示的に失敗する）。出力整形のみのフラグ（exit code を変えないもの）は arity 付き除去リストで
// 落としてから照合する（build-commands.md の既存整合規約の機械化）。
// nodeSpec / vitest 系（npm SSOT の部分集合ラッパー）は意味判断を要するため対象外＝
// /health-check の Check 5 残置部分が受け持つ（受容する範囲）。
// ---------------------------------------------------------------------------
const OUTPUT_ONLY_FLAGS = { "--message-format": 1 }; // フラグ名 → 後続引数の個数

export function checkHookCommands(snapshot) {
  const findings = [];
  const hookPath = ".claude/hooks/post-edit.mjs";
  const hookSrc = snapshot.read(hookPath);
  if (hookSrc == null) return [finding(hookPath, 1, "post-edit.mjs が読めない（G-hook-commands 母集団の欠落）")];
  // cargoSpec([...]) の引数配列を抽出（clippy は複数行折返しのため dotall 必須）
  const hookCommands = [...hookSrc.matchAll(/cargoSpec\(\[([\s\S]*?)\]\)/g)].map((m) =>
    [...m[1].matchAll(/"([^"]*)"/g)].map((t) => t[1]),
  );
  if (hookCommands.length === 0) {
    return [finding(hookPath, 1, "cargoSpec([...]) が 1 件も抽出できない（G-hook-commands 母集団の欠落。抽出アンカーの腐敗か buildCommand のリファクタ）")];
  }
  const docsPath = "docs/build-commands.md";
  const docsText = snapshot.read(docsPath);
  if (docsText == null) return [finding(docsPath, 1, "docs/build-commands.md が読めない（G-hook-commands）")];
  // カテゴリ A 節の bash フェンス内 cargo 行を母集団にする（行末 # コメントを除去）
  const sectionA = docsText.split(/^### A\. /m)[1]?.split(/^### /m)[0] ?? "";
  // 行分割は \r?\n — CRLF checkout（Windows CI・autocrlf=true）では `.` が \r に
  // マッチしないため、\r を残すと行末コメント除去 `#.*$` が発火しない（PR #595 で実測）
  const docsLines = sectionA
    .split(/\r?\n/)
    .filter((l) => l.trim().startsWith("cargo "))
    .map((l) => l.replace(/\s+#.*$/, "").trim().split(/\s+/).join(" "));
  if (docsLines.length === 0) {
    return [finding(docsPath, 1, "カテゴリ A の cargo コマンド行が 0 件（G-hook-commands 母集団の欠落）")];
  }
  for (const args of hookCommands) {
    // 出力整形フラグを arity 込みで除去し、"cargo" を前置してトークン列を正規化
    const normalized = ["cargo"];
    for (let i = 0; i < args.length; i++) {
      if (args[i] in OUTPUT_ONLY_FLAGS) {
        i += OUTPUT_ONLY_FLAGS[args[i]];
        continue;
      }
      normalized.push(args[i]);
    }
    const cmd = normalized.join(" ");
    if (!docsLines.includes(cmd)) {
      findings.push(
        finding(hookPath, 1, `hook の cargo コマンドが docs/build-commands.md カテゴリ A に無い（フラグ乖離の疑い）: ${cmd}`),
      );
    }
  }
  return findings;
}
