// allBasenames はスナップショット全体で不変（narrowing の有無に関わらない）ことを実測で確かめる。
// 実ファイルへは触れず、snapshot をラップして変異を注入する（.claude/rules/safety-nets.md 準拠）。
import { makeSnapshot } from "../scripts/governance/lib.mjs";
import { checkModuleIndex } from "../scripts/governance/checks/G-module-index.mjs";

const base = makeSnapshot("C:/workspace/Snotra");

// snotra-core/CLAUDE.md の「モジュール構成」節へ、snotra-core には実在しないが
// src-tauri 配下には実在する basename を index させる（跨 crate 衝突）。
const CM = "snotra-core/CLAUDE.md";
const CROSS_BASENAME = "events.rs"; // src-tauri/src/events.rs は実在・snotra-core/src には存在しない basename
const hasCrossFile = base.files.some((f) => f.startsWith("src-tauri/src/") && f.endsWith(`/${CROSS_BASENAME}`) || f === `src-tauri/src/${CROSS_BASENAME}`);
console.log(`前提: src-tauri/src 配下に basename=${CROSS_BASENAME} が実在するか = ${hasCrossFile}`);

const mutated = {
  files: base.files,
  read: (rel) => {
    const text = base.read(rel);
    if (rel !== CM) return text;
    return text.replace(/^## モジュール構成$/m, `## モジュール構成\n\n- \`${CROSS_BASENAME}\` — snotra-core には実在しないが src-tauri には実在する`);
  },
};

const narrow = checkModuleIndex(mutated, ["snotra-core"]);
const full = checkModuleIndex(mutated); // 全 4 crate

console.log(`絞った呼び出し（snotra-core のみ）findings=${narrow.length}`);
for (const f of narrow) console.log(`  ${f.file}:${f.line} ${f.message}`);
console.log(`全体呼び出し（4 crate）findings=${full.length}`);
for (const f of full) console.log(`  ${f.file}:${f.line} ${f.message}`);
