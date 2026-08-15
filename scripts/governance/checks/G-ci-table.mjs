//! G-ci-table — 「CI/CD メモ」対応表 ↔ .github/workflows/*.yml の照合。
import { finding } from "../lib.mjs";

export const id = "G-ci-table";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkCiTable(snapshot);
}

// ---------------------------------------------------------------------------
// G-ci-table — 「CI/CD メモ」対応表 ↔ .github/workflows/*.yml（旧 Check 10 の機械部分）。
// wrapper 等価: `npm run X` が verbatim に無くても、scripts[X] の値のパス様トークンが
// workflow に現れれば「実行あり」（CI は引数を上書きしてスクリプトを直接呼ぶことがある）。
// ---------------------------------------------------------------------------
export function checkCiTable(snapshot) {
  const findings = [];
  const p = "docs/build-commands.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/build-commands.md が読めない")];
  let scripts = {};
  try {
    scripts = JSON.parse(snapshot.read("package.json") ?? "{}").scripts ?? {};
  } catch {
    /* G-build-commands が報告する */
  }
  const lines = text.split("\n");
  const start = lines.findIndex((l) => /^\| 検証コマンド \| workflow \|/.test(l));
  if (start === -1) return [finding(p, 1, "「CI/CD メモ」対応表（| 検証コマンド | workflow |）が見つからない")];
  // 終了条件は「最初の空行」である。`startsWith("|")` で打ち切ると、表の途中に非表の行が 1 行
  // 紛れただけで**以降の行が照合されないまま緑になる**（#863 で発見した 2 つ目の沈黙経路。
  // 崩れた行の黙殺が 1 つ目）。走査の終わりは表の終わりであって、最初の異常ではない。
  for (let i = start + 2; i < lines.length && lines[i].trim() !== ""; i++) {
    if (!lines[i].startsWith("|")) {
      findings.push(finding(p, i + 1, `対応表の途中に表でない行がある（以降が照合されない経路）: ${lines[i]}`));
      continue;
    }
    // 必要なのは先頭 2 列 = split で 4 要素以上（実表は トリガー 列を持つ 3 列 = 5 要素）
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) {
      findings.push(finding(p, i + 1, `対応表の行に検証コマンド列と workflow 列がそろっていない: ${lines[i]}`));
      continue;
    }
    const wf = cols[2].match(/`?([A-Za-z0-9_-]+\.yml)`?/)?.[1];
    if (!wf) {
      findings.push(finding(p, i + 1, `workflow 列から .yml ファイル名を特定できない: ${cols[2]}`));
      continue;
    }
    const wfText = snapshot.read(`.github/workflows/${wf}`);
    if (wfText == null) {
      findings.push(finding(p, i + 1, `workflow ファイルが実在しない: ${wf}`));
      continue;
    }
    for (const m of cols[1].matchAll(/`((?:npm|cargo)[^`]*)`/g)) {
      const cmd = m[1];
      if (wfText.includes(cmd)) continue;
      const scriptName = cmd.match(/^npm run ([A-Za-z0-9:_-]+)/)?.[1];
      const wrapperPaths = (scripts[scriptName] ?? "")
        .split(/\s+/)
        .filter((t) => t.includes("/") || /\.(ps1|mjs|ts)$/.test(t));
      if (wrapperPaths.some((t) => wfText.includes(t))) continue;
      // npm ライフサイクル経由（1 段）: workflow が `npm run Y` を実行し、Y または preY が
      // `npm run X` を呼ぶなら X も実行される（例: build の prebuild が typecheck を呼ぶ）
      const viaLifecycle =
        scriptName &&
        Object.keys(scripts).some(
          (y) =>
            wfText.includes(`npm run ${y}`) &&
            (scripts[y]?.includes(`npm run ${scriptName}`) || scripts[`pre${y}`]?.includes(`npm run ${scriptName}`)),
        );
      if (viaLifecycle) continue;
      findings.push(finding(p, i + 1, `表のコマンド \`${cmd}\` が ${wf} のどの run にも現れない（wrapper パスも不一致）`));
    }
  }
  return findings;
}
