//! G-build-commands — docs/build-commands.md の npm script / cargo test -p crate の実在。
import { finding, workspaceMembers } from "../lib.mjs";

export const id = "G-build-commands";
export const domains = "unmigrated";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkBuildCommands(snapshot);
}

// ---------------------------------------------------------------------------
// G-build-commands — docs/build-commands.md の npm script / cargo test -p crate の実在（旧 Check 5 の決定的部分）。
// crate 名はディレクトリ名でなく各 member Cargo.toml の [package] name（`-p snotra` = src-tauri/）。
// check/clippy は --workspace で cargo 自身が SSOT を読むため照合対象外（#500）。
// ---------------------------------------------------------------------------
export function checkBuildCommands(snapshot) {
  const findings = [];
  const p = "docs/build-commands.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/build-commands.md が読めない")];
  let scripts = {};
  try {
    scripts = JSON.parse(snapshot.read("package.json") ?? "{}").scripts ?? {};
  } catch {
    findings.push(finding("package.json", 1, "package.json がパースできない"));
  }
  const crateNames = new Set();
  // 母集団は workspaceMembers に一本化する（#713）。error はここでは報告しない——同じ欠落を
  // 2 検査で 2 件にしないため。members が空なら crateNames も空になり、`cargo test -p` の行が
  // すべて赤くなるので、この検査は従来どおり fail-closed のままである。
  for (const dir of workspaceMembers(snapshot).members) {
    const name = (snapshot.read(`${dir}/Cargo.toml`) ?? "").match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    if (name) crateNames.add(name);
  }
  text.split("\n").forEach((line, i) => {
    for (const m of line.matchAll(/npm run ([A-Za-z0-9:_-]+)/g)) {
      if (!(m[1] in scripts)) findings.push(finding(p, i + 1, `npm script が package.json に無い: ${m[1]}`));
    }
    if (/(?:^|[^a-z])npm test(?:$|[^a-z])/.test(line) && !("test" in scripts)) {
      findings.push(finding(p, i + 1, "npm test に対応する scripts.test が無い"));
    }
    for (const m of line.matchAll(/cargo test -p ([A-Za-z0-9_-]+)/g)) {
      if (!crateNames.has(m[1])) findings.push(finding(p, i + 1, `cargo test -p の crate が workspace に無い: ${m[1]}`));
    }
  });
  return findings;
}
