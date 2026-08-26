// 歴史コミットでの読み取り専用測定（git のオブジェクトから snapshot を組む・作業ツリー不変）
import { execFileSync } from "node:child_process";

const repo = process.argv[2];
const rev = process.argv[3];
const libUrl = process.argv[4];
const {
  REF_HEAD, isRefTargetSpelling, refScanLines,
  collectAnchors, normAnchor, resolveRefTarget, allHeadingRefDocs,
} = await import(libUrl);

const git = (args) => execFileSync("git", ["-C", repo, ...args], { encoding: "utf8", maxBuffer: 512 * 1024 * 1024 });

const EXCLUDE = ["workspace/", ".claude/worktrees/", ".superpowers/", "node_modules/", "target/", "dist/"];
const files = git(["ls-tree", "-r", "--name-only", rev]).split("\n").filter(Boolean)
  .filter((f) => !EXCLUDE.some((p) => f.startsWith(p)));

const cacheRead = new Map();
const snapshot = {
  files,
  read: (rel) => {
    if (!cacheRead.has(rel)) {
      let v = null;
      if (files.includes(rel)) { try { v = git(["show", `${rev}:${rel}`]); } catch { v = null; } }
      cacheRead.set(rel, v);
    }
    return cacheRead.get(rel);
  },
};

const OLD = () => new RegExp(`${REF_HEAD}「([^「」\\n]+)」`, "g");
const NEW = () => new RegExp(`${REF_HEAD}「((?:[^「」\\n]|「[^「」\\n]*」)+)」`, "g");

function scan(docs, mk) {
  const rows = [];
  const cache = new Map();
  const anchorsOf = (p) => {
    if (!cache.has(p)) { const t = snapshot.read(p); cache.set(p, t == null ? null : collectAnchors(t).map(normAnchor)); }
    return cache.get(p);
  };
  let findings = 0;
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) { findings++; continue; }
    for (const [lineNo, line] of refScanLines(text, doc, [])) {
      for (const m of line.matchAll(mk())) {
        const [, target, label] = m;
        if (!isRefTargetSpelling(target)) continue;
        const p = resolveRefTarget(snapshot, doc, target);
        let landed = false;
        if (p != null) {
          const a = anchorsOf(p);
          landed = a != null && a.some((x) => x.startsWith(normAnchor(label)));
        }
        if (!landed) findings++;
        rows.push([doc, lineNo, target, label, landed].join(" ¦ "));
      }
    }
  }
  return { rows, findings };
}

const docs = allHeadingRefDocs(snapshot);
const o = scan(docs, OLD), n = scan(docs, NEW);
const so = new Set(o.rows), sn = new Set(n.rows);
console.log(`rev=${rev} docs=${docs.length}  OLD checked=${o.rows.length} findings=${o.findings}  NEW checked=${n.rows.length} findings=${n.findings}`);
const onlyOld = [...so].filter((r) => !sn.has(r));
const onlyNew = [...sn].filter((r) => !so.has(r));
console.log(`OLD にだけ在る = ${onlyOld.length} 件`);
onlyOld.forEach((r) => console.log("  - " + r));
console.log(`NEW にだけ在る = ${onlyNew.length} 件`);
onlyNew.forEach((r) => console.log("  + " + r));
