//! G-stale-identifiers — 規範の散文に残る、現行語彙に無い識別子（腐り）の検出（#736 の同クラス）。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-stale-identifiers";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（staleTargets・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("stale", scanStaleIdentifiers(snapshot, ctx.staleTargets));
}

/** Rust のコメントを落とす。落とさないと `preset` のような普通の英単語が doc コメントに埋もれる（実測）。
 *  文字列リテラル内の `//` 以降も落ちるが、向きは赤側（読みが消える）ゆえ沈黙しない */
function stripRustComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/\/\/.*$/gm, " ");
}

// ---------------------------------------------------------------------------
// G-stale-identifiers — 規範の散文に残る、現行語彙に無い識別子（腐り）の検出（#736 の同クラス）。
//
// #698 が述べた「述語だけが書かれた間接参照」は概念での再導出でしか拾えなかったが、
// **識別子として書かれた腐りは機械で拾える**。G-references が見るのはパスの実在までで、
// 識別子の実在は誰も見ていなかった。
//
// **自称スコープ**（#891 で広げた。射程の内訳は `ADR-stale-identifier-detector-scope` の追記節）。
// 見るのは次の 3 群の中の**バッククォート内 camelCase / SCREAMING_SNAKE / lowercase snake_case
// 識別子**だけである（型で修飾した形は末尾セグメントを見る・#993。判定の正本は `staleTarget`）:
// - `.claude/**` の規範の散文（`staleIdentifierDocs`）
// - **開発ガイド `docs/**`**（`staleIdentifierGuideDocs`。設計原則・ビルド手順・フック契約・
//   アーキ説明という性質の違うものが混在する）から**歴史記録 2 種を除いたもの**
// - 固定パスの `STALE_EXTRA_DOCS`（意図の SSOT・常時ロードの規範・設定 UI のデザイン規約）
//
// **母集団から外す基準は「日付を持つか」ではなく「もう成り立たないことを書く場所か」である。**
// `docs/adr/` は却下案（＝もう存在しない案）、`docs/superpowers/` は #589 で非規範化された当時の設計。
// 一方 `docs/design/` は日付スラグを持つが `status: Agreed` で `docs/architecture.md` が現在形で
// 指す先ゆえ**含める**——外すと G-references が守るポインタの**指し先だけが黙って腐る**。
// `docs/adr/` の除外はかつてこの検査だけの非対称だったが、`ADR-adr-frozen-history` で
// 全検査へ揃った——**凍結された歴史は語彙も供給せず、精度の照合もされない**（残るのは実在の辺のみ）。
// 実測でも `docs/adr/` を検査対象に入れると finding の 8 割が ADR 自身の却下記録で、
// **この検出器の ADR がこの検出器を赤にする**。
// **モジュール `CLAUDE.md` は入れない**——ラップ対象の外部 API（Win32 / tao / TTC）を語る場所ゆえ
// 外部語彙の**密度**が高い（実測 真の腐り 1 : 外部語彙 3。`WM_SETCURSOR` 等は語彙源をどう広げても免罪できない）。
//
// **述語の外に在るもの**は依然として多い。frontmatter の文字列・素の表テキスト・
// 日本語散文（「リアクティブ制約」等）は構造的に対象外で、#736 が挙げた 10 件のうちこの述語が
// 届くのは 0 件である（実測）。PascalCase・ドット区切り・式で書かれた腐りも述語の外にある
// ——**修飾形で見るのは末尾セグメントだけ**なので、型が改名されメンバ名が残った形も鳴らない。
// **「文書の腐りが機構で捕まる」とは言えない**——言えるのは
// **「`.md` の散文に camelCase / SCREAMING_SNAKE / lowercase snake_case で書かれた再発は捕まる
// （型で修飾されていてもよい）」**までである。**`.rs` の doc コメントは母集団外**ゆえ、そこに書かれた腐りは捕まらない
// （#975 で `.rs` を足す案を測って却下した。理由は外部 API の密度・`ADR-stale-identifier-detector-scope`
// 「その後（#975・述語へ lowercase snake_case を足し、`.rs` への母集団拡大は却下した）」）。
// **この検査は #736 の代替ではない**——同 issue は手作業で閉じ、G-stale-identifiers が引き受けるのは再発防止だけである。
//
// 判定: 識別子が「現行語彙」に 1 度も現れないなら finding。**現行語彙の正本は
// 「production のソースの非コメント本文」ただ 1 つである**（`stripRustComments` + `*.test.*` の除外）。
// この母集団を狭める 2 つは、どちらも同じ 1 つの原則から出ている——
// **語彙を寄付してよいのは「現に動いている実装」だけである**:
// - **コメントを外す**。含めると `resetForShow` のような由来注記（「〜相当」「parity」）が
//   語彙に化け、腐りが原理的に検出できない（実測 11 件）
// - **テストコードを外す**。含めると検出器自身のフィクスチャが偽陰性を作る——
//   `createObjectURL`（本検査が守りたい対象として `governance-check.test.mjs` に名指しで書いた語）が、
//   同ファイルに書かれているという理由だけで実リポジトリでは永久に検出されなかった（実測）
//
// **`SPEC.md` は語彙源ではなく検査対象である**（`ADR-stale-identifier-detector-scope`
// 「却下 4: 現行語彙をソースだけから作る（`SPEC.md` を入れない）」の失効注記が経緯を持つ）。
// 語彙に置いていた頃は、SPEC 内の候補が**自分自身に一致して自分を免罪していた**。
// 語彙から外すと同時に検査対象へ入れることで、SSOT と写しが**同時に鳴る**——
// 「どちらを先に直すか」という向きの問いが構造的に消える。
//
// **外部ツールの語彙は空白の規則が外す**: コマンドを書いた span（`gh pr view <PR> --json
// closingIssuesReferences` 等）は空白を含むので、そもそも判定対象にならない。かつては行に
// コマンドが在れば**その行ごと**捨てていたが、#993 で撤去した——日本語の長い段落にコマンドを
// 1 つ書いただけで段落の識別子が全滅する**沈黙経路**であり、しかも空白の規則と役目が重複していた
// （実測: コマンド span 153 件のすべてが空白を含み、行の述語を置く／置かないで結果が完全一致）。
// #984 の腐りを隠していたのはこの経路である。
//
// **受容する残余**:
// - 単語 1 つの識別子（`Glob` `expand` `plain`）は対象外である。こぶを 1 つ以上要求しないと、
//   harness のツール名と散文の語彙が大量に混じる（実測 53 件中 40 件弱）。SCREAMING_SNAKE 側が
//   `_` を 1 つ以上要求するのも同じ構造である（`CI` `TODO` `README` は対象外）
// - **`.yml` は GitHub 提供の語彙を寄付する**（`GITHUB_ENV` / `GITHUB_OUTPUT` / `GITHUB_TOKEN` /
//   `TAG_NAME` / `TAURI_SIGNING_PRIVATE_KEY`）ほか、`'` で分断された日付書式の断片
//   （`ddTHH` `ssZ` `yyyyMMddHHmm`）も語彙に化ける。同名の識別子が散文に書かれれば誤って免罪する（今日 0 件）
// - **Rust のテストコードは今も語彙を寄付しうる。** `VOCAB_TEST_FILE` が当たるのはファイル名の
//   `.test.<ext>` という形だけで、Rust 側の 3 つの形——`#[cfg(test)] mod` の中身・
//   `<crate>/tests/*.rs` の統合テスト・`src/**/tests/*.rs` へ分けたテストファイル——はどれも外れる。
//   `#[cfg(test)]` 以降を落とす変換を通しても落ちるのは 1 つ目だけで、残る 2 つには述語が要る。
//   **開けたままにするのは、塞ぐと偽陽性が出るからである**——語彙源へその変換を当てると、
//   `#[cfg(test)]` の中にしか綴りを持たないテストヘルパやテスト名を正しく引いている文書が
//   finding になる（2026-08-21 実測。`#[serde(rename_all)]` が属性から導出する綴りも同じ側へ落ちる）。
//   導入当時（#891）は全セルで 0 件だったので理由は「測って動かなかったから」だったが、**今日は動く**
//   ——穴のせいで見逃された真の腐りは今日も 0 件のまま、塞ぐ側の代償だけが育った
// - **`.json` は語彙源ではない**（`VOCAB_SOURCE_EXT`）。設定キーが JSON にしか無い語は偽陽性になりうる
//   ——`docs/hooks.md` の `${CLAUDE_PROJECT_DIR}` はこの残余を避けて**文書側の記述を正確化**して外した。
//   `.json` を入れれば免罪できるが、生成物（`src-tauri/gen/schemas/`）・依存メタデータ
//   （`package-lock.json` の integrity 断片）・gitignore 済みで CI に存在しないファイルを同時に招き、
//   **除外リスト無しには分離できない**（ファイル冒頭の「免除注記の機構を設けない」契約に当たる）
// - **テストコードにしか無い識別子も偽陽性になりうる**——上の「テストコードを外す」の裏返しで、
//   語彙源を狭めた側が新しく作った残余である（今日 0 件）
// ---------------------------------------------------------------------------

/** 現行語彙の正本になるソース拡張子。`.yml` が入るのは `.github/workflows/**` が
 *  **追跡され・人が書き・CI が実際に実行する**＝「現に動いている実装」だからである
 *  （`.yaml` はリポジトリに 1 本も無い）。**`.json` は入れない**——生成物・依存メタデータ・
 *  gitignore 済みファイルを同時に招き入れ、除外リスト無しには分離できない
 *  （`ADR-stale-identifier-detector-scope` の追記節） */
const VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml|yml)$/;
/** 語彙源から外すテストコード。見るのは `.test.<ext>` という**ファイル名の形**だけで、
 *  拡張子は `VOCAB_SOURCE_EXT` の **JS/TS 系だけ**を採る（`rs|ps1|toml` は含まない——
 *  Rust 側の穴は上の「受容する残余」） */
const VOCAB_TEST_FILE = /\.test\.(mjs|ts|tsx)$/;
/** バッククォート内で腐りを問う形: camelCase（こぶ 1 つ以上）・末尾 `()` は任意 */
const STALE_IDENT = /^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/;
/** 同じく SCREAMING_SNAKE（`_` 1 つ以上）。camelCase 側が「こぶを 1 つ以上要求する」のと同じ構造で、
 *  単語 1 つの識別子を受容する残余から外さない。**2 述語は先頭文字で相互排他**ゆえ、
 *  どちらが当たっても `scanStaleIdentifiers` の照合件数は 1 しか進まない */
const STALE_SNAKE_IDENT = /^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(\(\))?$/;
/** 同じく lowercase snake_case（`_` 1 つ以上・#975）。**このリポジトリの主要言語の語彙はここに居る**
 *  ——Rust の関数名・テスト名・フィールド名はすべて lowercase snake_case であり、上の 2 述語は
 *  そのどれにも当たらなかった（`index_tree.rs` の doc が存在しないテスト名を引いたまま素通りした）。
 *  **3 述語は先頭文字と字種で相互排他である**（camelCase は `_` を含まず、SCREAMING は先頭が大文字）
 *  ゆえ、どれが当たっても `scanStaleIdentifiers` の照合件数は 1 しか進まない */
const STALE_LOWER_SNAKE_IDENT = /^([a-z][a-z0-9]*(?:_[a-z0-9]+)+)(\(\))?$/;
/** 判定対象の識別子を取り出す。無ければ `null`。
 *  **修飾形（`::`）は末尾セグメントだけを見る**——型セグメント（PascalCase）を見ない理由は
 *  「単語 1 つの識別子は対象外」と同じで、外部の型名は語彙源をどう広げても免罪できない
 *  （`ADR-stale-identifier-detector-scope` の #993 の追記節に測定表がある）。
 *  **`.` の除外はトークン全体ではなくセグメントへ当てる**——先に当てると
 *  `icon.rs::encode_batch_binary` の形が素通りする（実測で唯一の真の腐りがこの形だった）。
 *  **捕獲群を読まない**のは 3 述語と同じ理由である（`scanStaleIdentifiers` のコメント）。 */
function staleTarget(raw) {
  const bare = raw.replace(/\(\)$/, "");
  const seg = bare.includes("::") ? bare.slice(bare.lastIndexOf("::") + 2) : bare;
  if (seg.includes(".")) return null;
  if (!STALE_IDENT.test(seg) && !STALE_SNAKE_IDENT.test(seg) && !STALE_LOWER_SNAKE_IDENT.test(seg)) return null;
  return seg.replace(/\(\)$/, "");
}

/** 現行語彙。production のソースだけを集め、コメントを落とす
 *  （`#` コメントの言語は行頭・行中とも落とす） */
export function currentVocabulary(snapshot) {
  const parts = [];
  for (const f of snapshot.files) {
    if (!VOCAB_SOURCE_EXT.test(f) || VOCAB_TEST_FILE.test(f)) continue;
    const src = snapshot.read(f);
    if (src == null) continue;
    // コメント除去の振り分け。**`VOCAB_SOURCE_EXT` へ `#` コメントの言語を足したら、この正規表現へも
    // 同時に足すこと**——足し忘れるとその言語のコメントが生のまま語彙へ入り、由来注記に書かれた
    // 識別子が「現行語彙」に化ける。その識別子が文書で腐っていても免罪されて検出されない
    // （2026-08-09 実測: `.psm1` のコメント語が実際に赤を緑へ変えた・#1008。上の「受容する残余」が
    // 記録する失敗形の再演）。この対応を強制する機構は無い。
    parts.push(/\.(ps1|toml|yml)$/.test(f) ? src.replace(/#.*$/gm, " ") : stripRustComments(src));
  }
  return parts.join("\n");
}

/** findings に加えて照合件数を返す（「腐りゼロ」と「照合していない」を区別する証跡・#497） */
export function scanStaleIdentifiers(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const vocab = currentVocabulary(snapshot);
  const seen = new Map();
  const inVocab = (id) => {
    if (!seen.has(id)) seen.set(id, new RegExp(`\\b${id}\\b`).test(vocab));
    return seen.get(id);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-stale-identifiers 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text, doc, findings)) {
      for (const m of line.matchAll(/`([^`\n]+)`/g)) {
        const raw = m[1];
        if (raw.includes("/") || raw.includes(" ")) continue;
        // **捕獲群を読まない**——`staleTarget` は `test` で当てて `()` は自分で落とす。マッチ結果の
        // `[1]` を読む形だと、2 述語を `|` で 1 本へ畳んだ瞬間に群がずれて `inVocab(undefined)` になり、
        // しかも実語彙は `undefined` を含むので**赤が出ないまま沈黙する**（複製への変異で実測）。
        // 読まなければ畳もうが分けようが結果が変わらず、「畳むな」という文書契約自体が要らなくなる
        const target = staleTarget(raw);
        if (target == null) continue;
        checked += 1;
        if (!inVocab(target)) {
          findings.push(
            finding(doc, lineNo, `散文に、現行語彙に無い識別子が残っている: \`${raw}\`（production のソースの非コメント本文に無い）`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkStaleIdentifiers(snapshot, docs) {
  return scanStaleIdentifiers(snapshot, docs).findings;
}
