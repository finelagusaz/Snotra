//! G-adr-file-names — `docs/adr/` のファイル名が `ADR-<slug>.md` 形で、本文の見出しと一致するか（#816）。
import { finding } from "../lib.mjs";

export const id = "G-adr-file-names";
export const domains = ["adrFiles"];

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkAdrFileNames(snapshot);
}

// ---------------------------------------------------------------------------
// G-adr-file-names — `docs/adr/` のファイル名が `ADR-<slug>.md` 形で、本文の見出しと一致するか（#816）。
//
// `G-adr-citations` は**引用側**しか見ない——「`ADR-<slug>` と書かれた引用に対応するファイルが
// 在るか」を照合する。**ファイルの側が規約に従っているか**は見ておらず、`docs/adr/foo.md` を
// 作っても誰も引用しなければ静かに通る（#789 の見直しで残余として特定した）。
//
// 見出しと stem の一致まで見るのは、#812 の裁定が「**stem = 引用文字列**」にすることで機械照合を
// 可能にしたからである。2 つがずれると、文書の自己申告と実体が食い違う——引用は解決するのに
// 開いた先が別の名前を名乗る形になり、どちらが正しいかを機械で決められなくなる。
//
// **連番へ戻る変更もここで落ちる**（`0019-foo.md` は形に合わない）。#812 が廃した連番は、
// 規範だけでは戻りうる——「番号の方が並び順が分かる」という理由は毎回もっともらしく見える。
// ---------------------------------------------------------------------------

/** ADR のファイル名の形。stem がそのまま短縮引用になる（#812） */
const ADR_FILE_NAME = /^ADR-([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\.md$/;

export function adrFiles(snapshot) {
  return snapshot.files.filter((f) => /^docs\/adr\/[^/]+\.md$/.test(f));
}

export function checkAdrFileNames(snapshot) {
  const findings = [];
  const files = adrFiles(snapshot);
  // 空母集団は明示 fail——走査が空でも「逸脱なし」に見える沈黙経路を塞ぐ（#497）
  if (files.length === 0) return [finding("docs/adr", 1, "ADR が 0 件（G-adr-file-names 母集団の欠落）")];
  for (const f of files) {
    const base = f.slice("docs/adr/".length);
    const m = base.match(ADR_FILE_NAME);
    if (!m) {
      findings.push(finding(f, 1, `ADR のファイル名が \`ADR-<slug>.md\` 形でない: ${base}（連番を振らない・#812）`));
      continue;
    }
    const text = snapshot.read(f);
    if (text == null) {
      findings.push(finding(f, 1, "ADR が読めない（G-adr-file-names 母集団の欠落）"));
      continue;
    }
    const heading = text.split("\n")[0].match(/^#\s+(ADR-[a-z0-9-]+)\s*[:：]/);
    if (heading == null) {
      findings.push(finding(f, 1, `冒頭が \`# ADR-<slug>: <題>\` の形でない（stem = 引用文字列の対応が取れない）`));
    } else if (heading[1] !== `ADR-${m[1]}`) {
      findings.push(
        finding(f, 1, `見出しがファイル名と食い違う: 見出し \`${heading[1]}\` / ファイル名 \`ADR-${m[1]}\``),
      );
    }
  }
  return findings;
}
