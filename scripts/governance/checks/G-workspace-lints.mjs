//! G-workspace-lints — ルート `[workspace.lints.rustdoc]` の deny が全 member で実効しているか（#713）。
import { finding, workspaceMembers, tomlLine, lintLevel } from "../lib.mjs";

export const id = "G-workspace-lints";
export const domains = "unmigrated";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkWorkspaceLints(snapshot);
}

// ---------------------------------------------------------------------------
// G-workspace-lints — ルート `[workspace.lints.rustdoc]` の deny が全 member で実効しているか（#713）。
//
// **守る命題**（前提つき: `Cargo.toml` を正規表現で近似パースする範囲で）: この検査が緑 ⇒
// `cargo doc` の intra-doc link 検出が全 workspace member で deny として効く。
// #706 では `snotra-egui-runtime` が opt-in を欠いたまま #627 から #700 の検証中まで CI を素通りした。
//
// 塞ぐのは cargo が **exit 0 で沈黙した** 次の 6 経路だけである（cargo 1.94.0 で実測）:
//   member 側 — [lints] が無い / [lints.rustdoc] だけ持つ（workspace テーブルを継承しない） /
//               [package] 配下の `lints.workspace = true`（`unused manifest key: package.lints` と
//               警告は出るが exit 0 のまま通る）
//   ルート側 — deny → warn への降格 / rustdoc サブテーブルが無い or 空 / 必須 lint の行だけ消える
// 射程外（cargo が manifest エラーにする＝沈黙しない）: ルートに `[workspace.lints]` が無い形・
// member の `workspace = false`・`[lints]` への他 lint 併記。**沈黙しない経路に見張りは置かない**。
//
// 受容する残余:
// - 見るのは `rustdoc` カテゴリだけである。`[workspace.lints.clippy]` の降格でこの検査は鳴らない——
//   そのうち `disallowed_methods` の deny だけは **G-clippy-disallowed** が見張るが（#950。src-tauri の
//   禁止集合が実効する条件の 1 つとして）、**それ以外の clippy lint は依然としてどの検査も見ていない**。
//   **「lints 全般が守られている」と読める書き方をしてはならない**。
// - 次の 2 つの dotted 表記は cargo 上は有効だが、この述語は非実効と判定する＝**赤に倒れる**（実測）。
//   向きが赤（沈黙しない）なので受容するが、**次の人の最も安い直し方が「検査を緩める」にならない**よう、
//   直し方を書いておく: (a) member 側の `["lints"]`（クォートした見出し）→ `[lints]` と書く、
//   (b) ルート側の `[workspace.lints]` 配下の `rustdoc.broken_intra_doc_links = "deny"`
//   → `[workspace.lints.rustdoc]` テーブルで書く。
// ---------------------------------------------------------------------------

/** ルートに在ることを要求する rustdoc lint。**名指しは意図的である**——「非空かつ全エントリ deny」だけでは
 *  片方の行が消えた形（残った 1 件は deny のまま）が緑を通る（実測）。消えたら困る識別子をカナリアが
 *  持つのは正しい形で、先例は `.claude/hooks/post-edit.test.mjs` の member 名ハードコードである。
 *  **固定するのは名指した lint の在否だけで、一覧そのものは固定しない**——3 つ目の lint を
 *  `[workspace.lints.rustdoc]` へ deny で足してもここへ足さなければ、**その行が後日まるごと消えても
 *  誰も気づかない**（受容する残余・2026-08-09 実測 #1008）。**足した lint が非実効になるのではない**:
 *  cargo はその lint を適用するし、在るあいだは下の「全エントリが deny/forbid」の側でも見られている。
 *  固定されないのは「在り続けること」である（`DISALLOWED_METHODS_GROUPS` と同型）。 */
export const REQUIRED_RUSTDOC_LINTS = ["broken_intra_doc_links", "invalid_html_tags"];

/** member 側の opt-in。**字面ではなく構文的位置で判定する**——`version.workspace = true` と
 *  `<dep>.workspace = true` が同じ字面で全 member に現れるため、字面一致の述語は常に緑になる
 *  （`docs/development-principles.md`「6. 検出は構造化された信号で行い」）。 */
export function hasWorkspaceLintsOptIn(text) {
  let section = ""; // "" = 最初の `[` 見出しより前（ルート直下）
  for (const raw of text.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      section = line;
      continue;
    }
    if (section === "[lints]" && /^workspace\s*=\s*true$/.test(line)) return true;
    // ルート直下の dotted key は `[lints]` テーブルと等価（実測）。`[package]` 配下の同じ行は
    // `package.lints` になるだけで cargo は黙って無視するため、section が "" のときだけ数える。
    if (section === "" && /^lints\.workspace\s*=\s*true$/.test(line)) return true;
  }
  return false;
}

/** ルートの `[workspace.lints.rustdoc]` が実効か（非空 + 必須 lint が在る + 全エントリが deny/forbid）。
 *  level は文字列形（`= "deny"`）とテーブル形（`= { level = "deny", priority = 1 }`）の 2 形を受ける。 */
export function rustdocLintsAreDenied(rootText) {
  const entries = new Map();
  let inSection = false;
  for (const raw of rootText.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      inSection = line === "[workspace.lints.rustdoc]";
      continue;
    }
    if (!inSection || line === "") continue;
    const m = line.match(/^([A-Za-z0-9_-]+)\s*=\s*(.+)$/);
    if (m == null) continue;
    entries.set(m[1], lintLevel(m[2].trim()));
  }
  if (entries.size === 0) return false;
  if (!REQUIRED_RUSTDOC_LINTS.every((k) => entries.has(k))) return false;
  return [...entries.values()].every((v) => v === "deny" || v === "forbid");
}

export function checkWorkspaceLints(snapshot) {
  // ルートは 1 回だけ読む——workspaceMembers も同じファイルを読むため、素直に書くと
  // 「読めない」1 つの事実が 2 件の finding になる（G-build-commands で避けたのと同じ重複）
  const root = snapshot.read("Cargo.toml");
  if (root == null) return [finding("Cargo.toml", 1, "ルート Cargo.toml が読めない（G-workspace-lints 母集団の欠落）")];
  const findings = [];
  if (!rustdocLintsAreDenied(root)) {
    findings.push(
      finding(
        "Cargo.toml",
        1,
        `[workspace.lints.rustdoc] に ${REQUIRED_RUSTDOC_LINTS.join(" / ")} が deny/forbid で揃っていない（全 member が opt-in していても intra-doc link の検出が黙って無効になる・#713）`,
      ),
    );
  }
  const { members, error } = workspaceMembers(snapshot);
  if (error) {
    findings.push(finding("Cargo.toml", 1, `${error}（G-workspace-lints 母集団の欠落）`));
    return findings;
  }
  for (const dir of members) {
    const p = `${dir}/Cargo.toml`;
    const text = snapshot.read(p);
    if (text == null) {
      findings.push(finding(p, 1, "member の Cargo.toml が読めない（G-workspace-lints 母集団の欠落）"));
      continue;
    }
    if (!hasWorkspaceLintsOptIn(text)) {
      findings.push(finding(p, 1, "[lints] workspace = true が無い（ルート [workspace.lints] の deny がこの crate だけ黙って無効になる・#713）"));
    }
  }
  return findings;
}
