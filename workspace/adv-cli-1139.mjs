// 敵対的検証用の使い捨て CLI。実際の hook subprocess 呼び出しを模す
// （node 起動 + import 解決 + makeSnapshot + 絞った検査）。稼働中のファイルは変更しない。
import { makeSnapshot, governanceDocs, gitIgnoredPaths } from "../scripts/governance/lib.mjs";
import { checkModuleIndex } from "../scripts/governance/checks/G-module-index.mjs";
import { checkReferences } from "../scripts/governance/checks/G-references.mjs";

const rel = process.argv[2] ?? "AGENTS.md";
const root = process.cwd();
const snap = makeSnapshot(root);
const docs = governanceDocs(snap);
let findings = [];
if (rel.endsWith(".md") && docs.includes(rel)) {
  findings = findings.concat(checkReferences(snap, [rel], gitIgnoredPaths));
}
if (rel.endsWith(".rs")) {
  const crate = Object.keys({
    "snotra-core": 1, "snotra-egui-runtime": 1, "src-tauri": 1, "snotra-settings": 1,
  }).find((c) => rel.startsWith(`${c}/`));
  if (crate) findings = findings.concat(checkModuleIndex(snap, [crate]));
}
console.log(`findings=${findings.length}`);
