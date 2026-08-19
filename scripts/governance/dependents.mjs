//! 節の中身が変わったとき、その節に依存している参照を報告する（#1140）。
//!
//! **合否を持たない。** `checks/` の外に在ることが「検査ではない」の担保である
//! （`instrument.mjs` と同じ理由——registry は `checks/` だけを走査するので、ここに在る限り
//! 「検査 N 件」には数えられない）。判定は人が持つ（`ADR-retire-area-budget` が面積で採った形）。
//!
//! **`G-heading-refs` が届かない穴を埋める。** あちらが見るのは「ラベルが見出しに着地するか」だけで、
//! **見出しの名前が変わらないまま本文が入れ替わっても緑のままである**。実測（2026-08-19）: 生きた層で
//! 着地している参照 271 件のうち 81 件が「参照が書かれたあとに指し先の節が変更された」状態にあった。
//!
//! **CLI として hook から subprocess で呼ばれる。** import ではないのは、静的 import が
//! `post-edit.mjs` の `try { main() } catch` の**外**で走り、解決に失敗すると JSON エンベロープを
//! 出さずにプロセスごと落ちるからである（`.rs` の fmt / clippy / test まで含めて全編集で hook が沈黙する）。
//! 相対 import が `${CLAUDE_PROJECT_DIR}` 基準で解決し `resolveRoot` のツリーとずれる問題
//! （`docs/hooks.md`「PostToolUse（post-edit.mjs）の機構と保守」）も、subprocess なら起きない。

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { makeSnapshot, normAnchor, resolveRefTarget, refScanLines, HEADING_REF, isRefTargetSpelling, allHeadingRefDocs, ANCHOR_SPECS } from "./lib.mjs";

/** 索引の鍵の区切り。**パスにもラベルにも現れない文字を明示のエスケープで書く**——
 *  生のバイトをソースへ置くと `grep` がファイルを binary と判定して以後の走査から落ちる（2026-08-19 実測） */
const SEP = "␟";
const keyOf = (target, label) => `${target}${SEP}${label}`;

/**
 * 逆引き索引 `(対象パス, 正規化ラベル) → 参照している箇所[]` を組む。
 *
 * **母集団は `G-heading-refs` と同じ 3 本の腕である**（md・`.rs`・スクリプトのコメント）。
 * 別に持つと、腕が増えたときに報告だけが取り残される。
 */
export function buildDependentIndex(snapshot) {
  const index = new Map();
  const docs = allHeadingRefDocs(snapshot);
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue;
    // findings は捨てる——釣り合わないフェンスを報告するのは検査の仕事であって計器の仕事ではない
    for (const [lineNo, line] of refScanLines(text, doc, [])) {
      for (const m of line.matchAll(HEADING_REF)) {
        const [, target, label] = m;
        if (!isRefTargetSpelling(target)) continue;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) continue;
        const k = keyOf(p, normAnchor(label));
        if (!index.has(k)) index.set(k, []);
        index.get(k).push(`${doc}:${lineNo}`);
      }
    }
  }
  return index;
}

/**
 * 文書を節へ切る。`{ labels, depth, from, to }`（行番号は 1 起点・`to` を含む）。
 *
 * **アンカーの一覧は `ANCHOR_SPECS`（`lib.mjs`）が正本である**——着地判定の `collectAnchors` と
 * 同じ一覧を読むので、種類を足したときに片方だけが知っている状態が作れない。
 *
 * **深さが要る。** 与えないと「次のアンカーまで」で節を切ることになり、`## 節` が最初の箇条書きの
 * 手前で終わる。実測 2026-08-19: `CLAUDE.md`「フック」を指す参照が在るのに、その節の中の
 * 箇条書きを編集しても報告が出なかった（**実物で走らせて初めて出た欠陥**——合成フィクスチャは
 * 見出し直下の行しか触っていなかった）。
 *
 * **節は入れ子になる。** ATX 見出しは「同じか浅い深さの次のアンカー」まで伸び、箇条書きのアンカーは
 * 次のアンカーの手前で終わる。ゆえに 1 行は複数の節に属し、`## 節` を指す参照とその中の
 * 箇条書きを指す参照の**両方**が報告される。
 *
 * **`sectionOf` は使わない**——あちらの契約は「見出しの正規表現を 1 本渡してその 1 節の body を返す」形で、
 * 複数一致は finding になる（全節の列挙という用途に合わない）。
 *
 * **フェンスをマスクしない。** `collectAnchors`（`G-heading-refs` の着地判定が使う）がマスクしないので、
 * 参照はフェンス内の見出しへも着地しうる。**境界だけマスクすると索引と境界が食い違う**ので、
 * 両方マスクしない側へ揃えた。**受容する残余**: テンプレート塊の見出しが節を作るので、その塊が
 * 上位の見出しより浅く見えることは無いが、塊自身の見出しを指す参照は塊の変更で鳴る。実測 2026-08-19:
 * 参照の対象になっている 28 文書のアンカー行 1371 本のうちフェンス内は 27 本（2.0%）。
 */
export function sectionsOf(snapshot, docPath) {
  const text = snapshot.read(docPath);
  if (text == null) return [];
  const lines = text.split("\n");
  const anchors = [];
  lines.forEach((line, i) => {
    const labels = [];
    let depth = 7;
    for (const spec of ANCHOR_SPECS) {
      const m = spec.re.exec(line);
      if (!m) continue;
      labels.push(normAnchor(spec.label(m)));
      depth = Math.min(depth, spec.depth(m));
    }
    if (labels.length) anchors.push({ labels: [...new Set(labels)], depth, at: i + 1 });
  });
  return anchors.map((a, i) => {
    let to = lines.length;
    for (let j = i + 1; j < anchors.length; j++) {
      if (anchors[j].depth <= a.depth) {
        to = anchors[j].at - 1;
        break;
      }
    }
    return { labels: a.labels, depth: a.depth, from: a.at, to };
  });
}

/**
 * `git diff -U0` のヘッダから `{ start, len, oldLen }` を取る（新側の範囲と、旧側の行数）。
 * `oldLen === 0` は**純粋な追記**である。
 */
export function parseHunks(diffText) {
  const out = [];
  for (const line of diffText.split("\n")) {
    const m = /^@@ -\d+(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (!m) continue;
    const oldLen = m[1] === undefined ? 1 : Number(m[1]);
    const len = m[3] === undefined ? 1 : Number(m[3]);
    if (len === 0) continue; // 削除のみ——新側に行が無いので節を特定できない
    out.push({ start: Number(m[2]), len, oldLen });
  }
  return out;
}

/**
 * 変更された行が属する節のうち、依存を持つものを返す（`[{ labels, refs }]`）。
 *
 * **純粋な追記（`oldLen === 0`）は外す。** 行が足されただけでは既存の参照を偽にしないためである。
 *
 * **発火の実測は「この実装で」測る。** 2026-08-19、`.md` を触った直近 80 コミットに対し
 * **44 件（55%）が発火・重複を除いた参照は中央 6 件・最大 37 件**。計画段階の見積もり（24〜25%）は、
 * 節を入れ子にしていなかった当時の数字であり、**`## 節` が最初の箇条書きの手前で切れる欠陥による
 * 過小評価だった**（`sectionsOf` の doc 参照）。**コミット単位の値なので、編集 1 回あたりの発火は
 * これより低い**——1 コミットは多くの編集を畳んでいる。
 */
export function changedSectionDependents(snapshot, docPath, hunks, index = buildDependentIndex(snapshot)) {
  const substantive = hunks.filter((h) => h.oldLen > 0);
  if (substantive.length === 0) return [];
  const out = [];
  for (const sec of sectionsOf(snapshot, docPath)) {
    if (!substantive.some((h) => h.start + h.len - 1 >= sec.from && h.start <= sec.to)) continue;
    const refs = new Set();
    for (const [k, where] of index) {
      const sep = k.indexOf(SEP);
      if (k.slice(0, sep) !== docPath) continue;
      const refLabel = k.slice(sep + 1);
      // 前方一致の向きは `G-heading-refs` と同じ——アンカーが参照ラベルで始まる
      if (sec.labels.some((l) => l.startsWith(refLabel))) for (const w of where) refs.add(w);
    }
    if (refs.size) out.push({ labels: sec.labels, refs: [...refs] });
  }
  return out;
}

/** 報告に並べる件数の上限。実測の中央 9〜10 件・最大 21〜31 件に対し、全部並べると読まれない */
const LISTED = 3;

/** hook が読む 1 行。**依存が無ければ空文字**（呼び出し側は空なら何も出さない） */
export function reportFor(snapshot, docPath, hunks) {
  const found = changedSectionDependents(snapshot, docPath, hunks);
  if (found.length === 0) return "";
  const refs = [...new Set(found.flatMap((f) => f.refs))].sort();
  const names = found.map((f) => `「${f.labels[0]}」`).join("");
  const head = refs.slice(0, LISTED).join(" / ");
  const rest = refs.length > LISTED ? ` ほか ${refs.length - LISTED} 件` : "";
  return (
    `WARN: ${docPath} の${names}の本文を変更しました。この節に依存する参照が ${refs.length} 件あります` +
    `（${head}${rest}）。指し先が変わったなら参照側も読み直してください` +
    `（全件: \`node scripts/governance/dependents.mjs ${docPath}\`）。`
  );
}

// ---------------------------------------------------------------------------
// CLI: `node scripts/governance/dependents.mjs <rel>` — cwd をツリー根として、
// そのファイルの未コミット差分に対する報告を stdout へ出す。**exit code は常に 0 である**
// （合否を持たない計器なので、赤にする資格がない）。
// ---------------------------------------------------------------------------

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const rel = process.argv[2];
  if (!rel) {
    console.error("usage: node scripts/governance/dependents.mjs <path>");
    process.exitCode = 0;
  } else {
    let diff = "";
    try {
      diff = execFileSync("git", ["diff", "HEAD", "-U0", "--", rel], { cwd: process.cwd(), encoding: "utf8" });
    } catch {
      // repo でない・HEAD が無い（初回コミット前）——比べる相手が無いので何も言わない
      diff = "";
    }
    const hunks = parseHunks(diff);
    const snapshot = makeSnapshot(process.cwd());
    const line = reportFor(snapshot, rel.replaceAll("\\", "/"), hunks);
    if (line) console.log(line);
    process.exitCode = 0;
  }
}
