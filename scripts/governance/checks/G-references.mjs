//! G-references — ガバナンス文書群の参照実在（Markdown リンク + バッククォート内パス様参照）。
import path from "node:path";
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-references";
export const domains = ["governanceDocs"];

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（docs・gitIgnoredPaths を使う） */
export function run(snapshot, ctx) {
  return checkReferences(snapshot, ctx.docs, ctx.gitIgnoredPaths);
}

/** 実在検査の対象と見なすソース系拡張子（G-references）。ランタイム生成物（.bin/.bak 等）は含めない。
 *  **保証は狭い**——バッククォート内のパス様参照は、拡張子がここに無ければ（`/` を含んでいても）
 *  静かにスキップされる（2026-08-09 実測: `.psm1` の実在しないパスが素通り・#1008）。 */
const REF_EXTENSIONS = /\.(md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css)$/;

// ---------------------------------------------------------------------------
// G-references — ガバナンス文書群の参照実在（Markdown リンク + バッククォート内パス様参照）。
// バッククォート参照の検査述語（受容する偽陰性はスクリプトコメントとテストで固定）:
//   `/` を含む・glob（* ? {）なし・<> なし・% なし・URL なし・拡張子が REF_EXTENSIONS・
//   workspace/ 配下でない・`\` を含まない。
//   → ベア名（`SPEC.md` 等）とランタイム生成物（`config.toml`・`*.bin`・`*.bak`）は構造的に対象外。
// ---------------------------------------------------------------------------

export function checkReferences(snapshot, docs, filterIgnored = () => new Set()) {
  const findings = [];
  const fileSet = new Set(snapshot.files);
  // 実在判定（exists）と ignore 照合（下の candidates）が同じ「文書ディレクトリ基準の正規化」を
  // 使う——exists はここを呼ぶ（独立した式を 2 つ持つと、片方だけ変えたときに実在判定と
  // 免除照合がずれ、偽の赤か偽の緑になる）。
  const docRelative = (doc, ref) => path.posix.normalize(path.posix.join(path.posix.dirname(doc), ref));
  const exists = (doc, ref, { allowSuffix = false } = {}) => {
    const norm = (p) => path.posix.normalize(p);
    if (fileSet.has(norm(ref))) return true; // リポジトリルート基準
    if (fileSet.has(docRelative(doc, ref))) return true; // 文書ディレクトリ基準
    // crate 内相対参照（`lib/types.ts` = ui/src/lib/types.ts、`commands/launch.rs` =
    // src-tauri/src/commands/launch.rs 等）はサフィックス一致で解決する（意図的な近似）。
    // バッククォート参照（`/` 必須の述語 = 2 セグメント以上）に限る——Markdown リンクへ
    // 適用すると、壊れた相対リンクが同 basename の別ファイルで偽陰性になる
    if (!allowSuffix) return false;
    const suffix = `/${norm(ref)}`;
    return !suffix.includes("..") && snapshot.files.some((f) => f.endsWith(suffix));
  };
  // 実在しなかった参照は**いったん保留する**——ignore 判定を 1 回の spawn に束ねるため（#1088）。
  // findings の順序は pending の順序がそのまま保つ。
  const pending = [];
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-references 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text, doc, findings)) {
      // (i) Markdown リンク
      for (const m of line.matchAll(/\[[^\]]*\]\(([^()\s]+)\)/g)) {
        let target = m[1];
        if (/^[a-z]+:/.test(target)) continue; // https: / mailto: 等
        target = target.split("#")[0];
        if (!target) continue; // 純アンカー
        if (!exists(doc, target)) {
          pending.push({ doc, lineNo, ref: target, message: `Markdown リンク先が実在しない: ${m[1]}` });
        }
      }
      // (ii) バッククォート内パス様参照
      for (const m of line.matchAll(/`([^`\n]+)`/g)) {
        const t = m[1];
        if (!t.includes("/")) continue;
        if (/[*?{<>%\\]/.test(t)) continue;
        if (t.includes("://") || t.includes(" ")) continue;
        if (!REF_EXTENSIONS.test(t)) continue;
        if (t.startsWith("workspace/") || t.startsWith("~")) continue;
        if (!exists(doc, t, { allowSuffix: true })) {
          pending.push({ doc, lineNo, ref: t, message: `バッククォート参照のパスが実在しない: ${t}` });
        }
      }
    }
  }
  // ルート基準と文書ディレクトリ基準の**両方**を候補に出す（散文がどちらの形で書くかは選べない）
  const candidates = pending.flatMap((p) => [p.ref, docRelative(p.doc, p.ref)]);
  const ignored = filterIgnored([...new Set(candidates)]);
  for (const p of pending) {
    if (ignored.has(p.ref) || ignored.has(docRelative(p.doc, p.ref))) continue;
    findings.push(finding(p.doc, p.lineNo, p.message));
  }
  return findings;
}
