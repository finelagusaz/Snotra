// 実 spawnSync で CLI 起動コストを測る（node -e "" との対照 / dependents.mjs 実測との対照）
import { spawnSync } from "node:child_process";
import { performance } from "node:perf_hooks";

function timeit(label, fn, n = 5) {
  const ts = [];
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    const res = fn();
    ts.push(performance.now() - t0);
    if (res.status !== 0) {
      console.log(`  !! exit ${res.status} stderr=${res.stderr}`);
    }
  }
  console.log(`${label}: ${ts.map((x) => x.toFixed(1)).join(" / ")} ms`);
}

const root = "C:/workspace/Snotra";

timeit('node -e ""', () =>
  spawnSync(process.execPath, ["-e", ""], { cwd: root, encoding: "utf8" }));

timeit("adv-cli-1139.mjs AGENTS.md (絞った G-references, 1 doc)", () =>
  spawnSync(process.execPath, ["workspace/adv-cli-1139.mjs", "AGENTS.md"], { cwd: root, encoding: "utf8" }));

timeit("adv-cli-1139.mjs snotra-core/src/lib.rs (絞った G-module-index, 1 crate)", () =>
  spawnSync(process.execPath, ["workspace/adv-cli-1139.mjs", "snotra-core/src/lib.rs"], { cwd: root, encoding: "utf8" }));

timeit("dependents.mjs AGENTS.md (既存 hook subprocess・対照)", () =>
  spawnSync(process.execPath, ["scripts/governance/dependents.mjs", "AGENTS.md"], { cwd: root, encoding: "utf8" }));

timeit("npm run governance:check 全体（対照）", () =>
  spawnSync("npm.cmd", ["run", "governance:check"], { cwd: root, encoding: "utf8", shell: true }), 3);
