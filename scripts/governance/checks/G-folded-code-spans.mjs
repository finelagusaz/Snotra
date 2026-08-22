//! G-folded-code-spans — コードスパンが物理改行を跨いだ形（#992）。
import { finding, refScanLines, linesOfComments } from "../lib.mjs";

export const id = "G-folded-code-spans";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（allRefDocs・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("codeSpans", scanFoldedCodeSpans(snapshot, ctx.allRefDocs));
}

// ---------------------------------------------------------------------------
// G-folded-code-spans — コードスパンが物理改行を跨いだ形（#992）。
//
// **害は `grep` にしか出ない。** 折れたスパンは**正しい CommonMark** であり、rustdoc は
// soft line break を跨いで正しく描画する——だから読んでも気づかず、壊れるのは検索だけである
// （`docs/comment-guidelines.md`「日本語の折返し」が 2026-08-08 の実測で記録した形。
// うち 1 件は `current_thread().id() == context.main_thread_id` を丸ごと分断していた）。
//
// **既製の道具は持てない。固定版 1.98 で実測した**（`ADR-folded-code-span-detector`）:
// rustfmt の `wrap_comments` は stable で警告のみ出して無視され、nightly でも向きが逆
// （`comment_width` へ**折り込む**側の機能である）。clippy は doc lint を全部有効にしても 0 件。
// 整形器も lint も「人間が正規表現で検索する」ことをモデルに持たないためで、道具の欠落ではない。
//
// **述語は折返しの意味を見ない。** 見るのは「その行末でスパンが開いたままか」だけである。
// 単独のバッククォートを左から順に対応づけ、**開いた行と閉じた行が違えば finding** にする。
// 2 連以上の連なり（コードフェンスの記号と `` ` `` のエスケープ形）は先に落とす——落とさないと
// 実ツリーで 10 件の誤検出になった（2026-08-22 実測。22 行 → 12 行）。
//
// **報告は開始行 1 件だけである**（3 行以上に割れても増えない）。行ごとの偶奇で見ると
// 同じ折返しを開始行と終了行の 2 回報告する（実測: 12 行 = 6 箇所）。
//
// **射程はファイル種別に依らない**——`grep` で辿るのはどの言語のコメントでも同じだからである。
// 母集団は `G-heading-refs` と同一（`allHeadingRefDocs`）で、新しい母集団定義を作らない。
// **`.ps1` / `.psm1` にはコメント作法の生きた規範が一つも無い**（`G-folded-heading-refs` の
// ヘッダが宣言している）。規範が一言も無い面で機構だけが赤を出すので、**この宣言が赤を
// 受け取った者の唯一の拠り所になる**。実際 #992 の時点で実在した 6 箇所のうち 2 件がそこに在った。
//
// **宣言する死角——ここが見ないもの（沈黙側）:**
//   - **行末のインラインコメント**（`let x = 5; // 説明`）。`linesOfComments` が行頭で判定するため、
//     **折れの両端が行末コメントに在ると 0 件で素通りする**（2026-08-22 実測）
//   - **母集団から外れる `.md`**。`docs/adr/` ・`docs/superpowers/` ・`workspace/` は
//     `allHeadingRefDocs` が落とす。実測でツリーの `.md` 200 枚のうち母集団は 48 枚である
//     （`.rs` とコメント記法スクリプトは全数入る）。**枚数をここへ書かない**——腕が増えれば腐る
//   - **`.md` のフェンス内**（`linesOutsideFences` が落とす）。フェンスは折返しの対象外である
//
// **宣言する死角——赤側へ倒れるもの（沈黙しないので人が解く）:**
//   - **`.rs` の rustdoc コードフェンスの内側**。`linesOfComments` はフェンスをマスクしない
//     （`lib.mjs` の `refScanLines` 周辺が同型の残余を既に宣言している）。今日 0 件
//   - **`.rs` の raw string の中で `//` から始まる行**をコメントと誤判定する。`linesOfComments` の
//     js 族が自ら宣言している死角をそのまま継承する
//   - **PowerShell のエスケープ文字**（`` `n `` / `` `$ `` 等）。スパンを開く意図が無いのに 1 個数える。今日 0 件
//   - **`.md` の 4 スペースのインデントコードブロック**。`linesOutsideFences` は ``` と ~~~ しか見ない
//   - **綴りの意味**。非実在の例示コードも実在の識別子と同じ重みで赤にする（`launcher_controller.rs`
//     の畳みが実例。スパンを 2 つへ割る形で解いた）
//
// **述語は変えずに死角を宣言する側へ倒した**——母集団の外にある `.md` 152 枚（この検査が一度も
// 直していないコーパス）へ当てて 18155 スパン中 findings 0 件だったので、実データでの誤検出率は
// 測れる限り 0 である。赤側の死角はいずれも逃げ道を要さない（直せば消える）。
// 決定と却下した代替案は `ADR-folded-code-span-detector` が持つ。
//
// **例示に実在の折れを置かない**——`checks/` はこの検査自身の走査母集団である。
// ---------------------------------------------------------------------------

// **危ないのはこのファイルのコメントであって、コードではない。** この検査の母集団に `checks/` が
// 入るが、走査されるのは `linesOfComments` が返す**コメント行だけ**なので、下の正規表現リテラルの
// ような**コード行は何個書いても当たらない**（実測: この行を含む状態で自分自身の finding は 0 件）。
// 当たるのは上のヘッダのような散文の側であり、そこでは 2 連（`` ` ``）で綴るか偶数個にする。
// **`matchAll` からだけ使う。** `g` 付きの定数を `test` / `exec` と共有すると `lastIndex` が
// 持ち越されて行ごとの判定がずれる（`lib.mjs` の `HEADING_REF` が同じ理由で同じ注意を持つ）。
const TICK = /`/g;

/** 2 連以上の連なり（コードフェンスの記号・`` ` `` のエスケープ形）を落とす。
 *  **囲まれた中身ごと落とす**——エスケープ形の内側の 1 個を残すと、その行が奇数になる（実測 2 件）。 */
const stripRuns = (line) => line.replace(/`{2,}[\s\S]*?`{2,}/g, "").replace(/`{2,}/g, "");

/**
 * 走査行。**`.rs` だけ `refScanLines` を通さない**——あちらは `.rs` を散文側（全行）へ落とすので、
 * コード中の char リテラルのバッククォートを拾う（実例は `snotra-settings/src/tabs/backup.rs` の
 * `find` の引数）。`COMMENT_FAMILY` へ `.rs` を足す形は採れない——`refScanLines` の意味論が変わり、
 * 見出し参照の 3 検査の母集団が同じ変更で動く（#925 以来の意図的な母集団・#489）。
 * ゆえに族を明示して `linesOfComments` を借りる。
 */
const spanScanLines = (text, file, findings) =>
  file.endsWith(".rs") ? linesOfComments(text, file, "js") : refScanLines(text, file, findings);

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497）。
 *  `checked` は**スパンの開始点**を数える——閉じたものも含むので、0 は「1 つも見ていない」を指す。 */
export function scanFoldedCodeSpans(snapshot, docs) {
  const findings = [];
  let checked = 0;
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-folded-code-spans 母集団の欠落）"));
      continue;
    }
    // 「開いたまま」は行をまたいで持ち越す。**走査行が途切れたら捨てる**——別のコメントブロックや
    // フェンスの向こう側の偶奇が混ざると、沈黙側にも誤報側にも倒れうる。
    let inside = false;
    let openLine = 0;
    let openTail = "";
    let prevNo = 0;
    for (const [lineNo, line] of spanScanLines(text, doc, findings)) {
      if (lineNo !== prevNo + 1) inside = false;
      prevNo = lineNo;
      const body = stripRuns(line);
      for (const m of body.matchAll(TICK)) {
        if (!inside) {
          checked += 1;
          openLine = lineNo;
          openTail = body.slice(m.index + 1);
        }
        inside = !inside;
      }
      // 報告は開始行だけ。継続行（`openLine` が古い）では出さないので、3 行以上でも 1 件である
      if (inside && openLine === lineNo) findings.push(fold(doc, lineNo, openTail));
    }
  }
  return { findings, checked };
}

/** 報告に載せる綴りの上限。全文を載せると折返しの長い行がそのまま流れる */
const TAIL = 40;

const fold = (doc, lineNo, tail) => {
  const shown = tail.trim().slice(0, TAIL);
  return finding(
    doc,
    lineNo,
    `コードスパンが物理改行を跨いでいる（開いたまま行が終わる: 「${shown}」）— ` +
      "grep で辿れなくなる。1 物理行に収めること",
  );
};

export function checkFoldedCodeSpans(snapshot, docs) {
  return scanFoldedCodeSpans(snapshot, docs).findings;
}
