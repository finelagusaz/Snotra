//! G-fullwidth-doc-link-bracket — intra-doc link の角括弧が半角と全角で混在した形（#1172）。
import { finding, linesOfComments, headingRefSourceDocs } from "../lib.mjs";

export const id = "G-fullwidth-doc-link-bracket";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("docLinkBrackets", scanFullwidthDocLinkBrackets(snapshot, headingRefSourceDocs(snapshot)));
}

// ---------------------------------------------------------------------------
// G-fullwidth-doc-link-bracket — intra-doc link の角括弧が半角と全角で混在した形（#1172）。
//
// **rustdoc はリンクと認識せず、リテラルとして出す。** `[`X`]` は intra-doc link だが、閉じが全角の
// `[`X`］` は `<code>X</code>］` とそのまま描画される。`broken_intra_doc_links = "deny"` が赤にするのは
// **壊れたリンク**であって、**リンクですらないもの**には何も言わない。PR #1171 でこの形が 3 本あった
// 状態で `cargo doc` / clippy / test / `governance:check` / PostToolUse がすべて緑のまま通り、見つけたのは
// `/code-review` の 2 巡目だった。rust-analyzer も該当の診断を持たない（issue が 4 面で実測）。
//
// **守るのは `docs/comment-guidelines.md`「名指しと正本の指名」の表の 1 行である**——「production の
// `///` `//!`」の行（着地を検査する ○）。表が「○」と名乗った面で黙るのが害であり、× の 2 行
// （`#[cfg(test)]` の中・`//` インライン）は規範上の保証が最初から無いので、この検査が赤にする理由も無い。
//
// **母集団は `.rs` の doc 行（`///` / `//!`）だけである。** `headingRefSourceDocs`（`.rs` 全件）を受け、
// `linesOfComments(text, file, "js")` でコメント行を取り、行頭の `///` / `//!` へ絞る。
// **`trimStart()` は必須である**——`linesOfComments` は判定にだけ trim 済みの行を使い、返すのは
// インデント込みの raw なので、無いと `impl` ブロック内の doc（実ツリーの大半）を全滅で見逃す
// （2026-09-02 実測。隣のテストのインデント済み fixture がこの欠落を赤にする）。
// `.md` / `.mjs` / `.ps1` は見ない——intra-doc link の概念が無く、「表の ○ が嘘になる」構造も無い。
//
// **`#[cfg(test)]` は絞らない。** 個別アイテムに付く `#[cfg(test)]` では doc が属性より**前**に来るため、
// テキスト走査で「属性以降を落とす」は絞ったつもりで絞れない（`mod tests` の入れ子も同様）。誤った境界は
// **沈黙側**へ倒れる。`#[cfg(test)]` 内の検出は「直して困らない過剰」として受容する——表の × は
// 「保証が無い」であって「書いてよい」ではない。
//
// **述語は 1 物理行の中だけを見る。** 開きと閉じの間に別の角括弧（半角・全角とも）を含まない最短の対で、
// 片方だけが全角なら finding。行を跨ぐ角括弧は rustdoc も解決しないので見ない。
//
// **宣言する死角——ここが見ないもの（沈黙側）:**
//   - **`/** … */` ブロック形の doc**。行頭判定が `///` / `//!` だけなので落ちる。実ツリー 0 件（2026-09-02）
//   - **`//` インラインの混在形**。表で × の面
//   - **全角同士 `［…］`**。rustdoc も人もリテラルとして読むので誤りではない
//   - **コードスパンの内側に角括弧を持つリンク**（`` [`Vec<[u8; 4]>`］ `` の形）。述語が「内側に角括弧を
//     含まない最短の対」を見るため、混在していても一致しない。実ツリー 0 件（委譲レビュー L2・2026-09-02）
//   - **`////`（4 本以上）**。Rust では doc ではないので `isDocLine` が外す。実ツリー 0 件
//   - **`〔〕` `【】` 等の他の括弧**。ASCII `[` `]` の全角互換文字ではなく、打鍵ミスで `[`…`〕` が生じる
//     経路が無い。issue の実例も `］` だけである
//   - **git 未追跡の `.rs`**。`makeSnapshot` が歩けば入るが、実ツリー 0 件確認は `git grep` ではなく
//     この検査自身の findings と `checked` で行うこと（`git grep` は追跡ファイルしか見ない）
//
// **宣言する死角——赤側へ倒れるもの（沈黙しないので人が解く）:**
//   - **rustdoc コードフェンスの内側の混在形**。`linesOfComments` はフェンスをマスクしない。今日 0 件
//   - **散文の括弧書きで意図的に混ぜた形**（`（…［…］…）` の内側に半角 `[` が同居する等）。今日 0 件。
//     出たら全角対にするか `` ` `` で包む
//
// **`checked` は角括弧の対の総数**（正しい対も混在も数える）。0 なら「1 つも見ていない」を指す（#497）。
// ---------------------------------------------------------------------------

// **`matchAll` からだけ使う。** `g` 付きの定数を `test` / `exec` と共有すると `lastIndex` が持ち越される
// （`lib.mjs` の `HEADING_REF` と同じ注意）。
/** 混在した対: 開き半角＋閉じ全角、または開き全角＋閉じ半角。内側に角括弧を含まない */
const MIXED = /\[[^\[\]［］\n]*］|［[^\[\]［］\n]*\]/g;
/** 証跡用: 何らかの角括弧の対（半角・全角の 4 通り） */
const ANY_PAIR = /[\[［][^\[\]［］\n]*[\]］]/g;

// `////`（4 本以上）は Rust では doc ではなく通常のコメントなので外す（委譲レビュー L1・2026-09-02）
const isDocLine = (line) => {
  const t = line.trimStart();
  return (t.startsWith("///") && !t.startsWith("////")) || t.startsWith("//!");
};

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497）。 */
export function scanFullwidthDocLinkBrackets(snapshot, docs) {
  const findings = [];
  let checked = 0;
  for (const doc of docs) {
    if (!doc.endsWith(".rs")) continue;
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-fullwidth-doc-link-bracket 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOfComments(text, doc, "js")) {
      if (!isDocLine(line)) continue;
      checked += [...line.matchAll(ANY_PAIR)].length;
      for (const m of line.matchAll(MIXED)) {
        findings.push(
          finding(
            doc,
            lineNo,
            `intra-doc link の角括弧が半角と全角で混在している（「${m[0]}」）— ` +
              "rustdoc はリンクと認識せずリテラル出力し、broken_intra_doc_links も黙る。両方を ASCII の [ ] にすること",
          ),
        );
      }
    }
  }
  return { findings, checked };
}

export function checkFullwidthDocLinkBrackets(snapshot, docs) {
  return scanFullwidthDocLinkBrackets(snapshot, docs).findings;
}
