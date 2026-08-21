//! governance:check の共有基盤。運用規則はただ一つ——**helper を置く前に、まずその検査のファイル
//! （`scripts/governance/checks/`）へ移せないかを問う。移しても何も壊れないなら、ここへ置いてはならない。**
//! ここに残ってよいのは、単一の検査ファイルへ移すと壊れるもの——複数の検査ファイルが import する・
//! facade が直接 import する・lib 内の他の宣言が参照する、など——だけである。
//! **「ここに何が在るか」の理由は列挙しない**——列挙は次に来る成員のたびに書き足しを要求し、
//! 書き漏らせば偽になる（このヘッダー自身が `gitIgnoredPaths`・`STALE_EXTRA_DOCS` で 2 回そうなった）。
//! 依存は Node 標準モジュールのみ（`governance-check.mjs` の契約を継承する）。
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

/** 走査から除外するディレクトリ。
 *  **PATHS の照合は `rel` の完全一致＝ルート錨止めである**——一致したディレクトリへ降りないので
 *  配下ごと落ちる。`docs/.superpowers` も `.superpowers-extra` も `rel` が一致しないので
 *  巻き込まない（#728）。`.superpowers/` は SDD（subagent-driven-development）の作業バッファで、
 *  gitignore 済みゆえ CI のチェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で
 *  別の母集団を見る（#722）。
 *
 *  **生成物（`node_modules` / `target` / `dist`）も PATHS 側に置く**（#1089）。かつては名前一致・
 *  全階層で落としており、`demo/src/target/orphan.rs` のような `.rs` が**どの深さでも**母集団から
 *  消えていた——`G-module-index` の逆方向も `G-module-linkage` も見ないまま緑になる形で、
 *  **向きは沈黙である**。ネストした同名ディレクトリは今日 0 件（2026-08-17 実測。4 crate 配下に
 *  `target/` は不在）ゆえ露出は 0 で、塞いだのは将来その形が現れたときの沈黙である。
 *  **失うもの**: 2 つ目の npm パッケージを置くと `ui/node_modules` が走査に入る。**そのときの向きを
 *  決めるのは走査器ではなく呼び出し点の述語である**（#1089 が `sectionOf` について確立したのと同じ原理が、
 *  走査の側にも当たる）。**「ノイズ＝安全側」だけではない**——`G-module-index` の順方向は
 *  「索引に書かれた basename が `snapshot.files` のどれかに在るか」＝所属で判定するので、走査が広がるほど
 *  照合先が育ち、**索引に書かれた実在しないファイル名が偶然一致して緑になる＝沈黙側**である
 *  （順方向が受け付ける拡張子には `ts` と `html` が含まれ、`node_modules` はどちらも大量に持つ）。
 *  そのときは PATHS へ 1 行足す。述語の側で塞ぐ案（`allBasenames` を crate の src 配下へ絞る）は
 *  この時点では入れていない。
 *
 *  **`.claude/hooks/lsp-config.mjs` の同型（`ADR-claude-code-ra-lsp-plugin-delivery.md`「受容する残余」）
 *  とは足並みを揃えない。** 理由は 3 つ: (1) 今日すでに非対称である——あちらは `worktrees` を名前一致で
 *  落とし、こちらは `.claude/worktrees` をルート錨止めで落とす（#728）。「両方直す」は対称性の回復では
 *  なく**新規の創出**になる (2) 母集団の意味が違う——こちらは検査の入力、あちらは ratoml の探索である
 *  (3) ADR は凍結された歴史ゆえ書き換えない（`ADR-adr-frozen-history`）。 */
const WALK_EXCLUDE_NAMES = new Set([".git"]);
const WALK_EXCLUDE_PATHS = ["workspace", ".claude/worktrees", ".superpowers", "node_modules", "target", "dist"];

/** リポジトリを歩いて snapshot（files: "/" 区切り相対パス一覧, read(rel)）を作る。
 *  列挙は fs 自身に問う（`git ls-files` の pathspec `**` 意味論の罠を避ける・health-check Check 1 注記） */
export function makeSnapshot(root) {
  const files = [];
  const walk = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const rel = path.relative(root, path.join(dir, e.name)).replaceAll("\\", "/");
      if (e.isDirectory()) {
        if (!WALK_EXCLUDE_NAMES.has(e.name) && !WALK_EXCLUDE_PATHS.includes(rel)) walk(path.join(dir, e.name));
      } else {
        files.push(rel);
      }
    }
  };
  walk(root);
  return {
    files,
    read: (rel) => {
      try {
        return fs.readFileSync(path.join(root, rel), "utf8");
      } catch {
        return null;
      }
    },
  };
}

/**
 * コードフェンスの内側を落として `[lineNo, text]` を返す（誤検出源: SPEC.md の TOML コメント等）。
 *
 * **開いたフェンスを閉じられるのは、同じ文字の・開いた長さ以上の・情報文字列を持たない行だけである。**
 * かつてここは `` /^\s*```/ `` に当たる行の**パリティ**を数えていた。パリティは「対になる行の文字も
 * 長さも同じ」という前提の上でしか成り立たず、その前提は少なくとも 2 つの記法で崩れる:
 *   - **4 連バッククォート**（3 連を含む例を書くための記法。このリポジトリで既に使われている）
 *     ——内側の 3 連 2 本がパリティを反転させ、findings 0 件のままマスクが壊れる
 *   - **`~~~` フェンス**——同上
 *
 * どちらも `sectionOf` 越しでは**フェンス内の `## にせ終端` が終端に採られて節が黙って縮み**、
 * **フェンス内の見出しがアンカーとして採用された**（2026-08-17 実測）。**今日の露出は 0 である**
 * ——`linesOutsideFences` / `sectionOf` の消費者（`governance-check.mjs` 経由の各検査）が実際に
 * 走査する 201 文書に、4 連も `~~~` も 1 行も無い。4 連は追跡 `.md` 177 件中 6 行あるが、
 * いずれも `docs/superpowers/plans/` にあって走査に現れない（2026-08-17 に走査を全件記録して実測）。
 *
 * **ここが CommonMark から採ったのは上の 1 文である。「準拠」ではない**——準拠は全称の主張であり、
 * 測れない。実装していない規則が少なくとも次のとおり在る: 開きフェンスのインデントを 0〜3 桁に限る規則（ここは `^\s*` ＝
 * 任意桁を許す。**今日の挙動を保つための選択**で、リスト内の深くインデントされたフェンスも従来どおり
 * フェンスとして扱う）・バッククォートの情報文字列にバッククォートを含められない規則・リスト項目や
 * 引用の内側でのフェンスの相対的な扱い・インデントコードブロック。
 *
 * **釣り合っていないフェンス（開いたまま閉じない）を finding にする。** 閉じないフェンスは
 * 以降の全行を「内側」にするので、この関数を通す検査はその範囲を丸ごと見なくなる。**向きは
 * 呼び出し点で決まり、赤くなる側だけではない**——`sectionOf` の `ending: "eof"` は、本来の終端見出しが
 * マスクされて findings 0 件のまま body が EOF まで伸びる（2026-08-17 実測）。
 *
 * **検知をこの関数の中に置くのは、母集団を一致させるためである。** 別の検査として外に置くと、
 * その検査が持つ文書一覧は消費側の母集団の写しになり、新しい消費者が現れたときに黙って射程から外れる。
 * 中に在れば「マスクされる入力」と「検算される入力」は構造上つねに同じものになる。
 *
 * **向きの使い分け**（`sectionOf` と同じ区別）:
 *   - `file` / `findings` の欠落は **throw**——呼び出し側の契約違反であって文書の欠陥ではない
 *   - 釣り合わないフェンスは **finding**——文書の欠陥である。throw にすると registry 経由の検査が
 *     スタックトレースで落ち、どの文書が原因かも `file:line` の形で出ない
 *
 * **受容する残余**は少なくとも 2 つ在る:
 *   - 同じ文書を複数の検査が走査するので、1 つの欠陥から重複した finding が出る
 *     （既に赤い状態でのみ起こる）
 *   - **情報文字列を持つ行が閉じフェンスにならなくなった**——パリティの下では
 *     `` ```bash `` で開いて `` ```bash `` で閉じる書き方が通っていた。今はそれが「開いたまま」に
 *     なり、文書は釣り合わない finding で赤くなる。走査中の 201 文書ではマスクの出力が
 *     1 行も変わらないことを実測済みだが（下の対照）、**将来その書き方が現れれば赤くなる側である**
 *
 * **対照**（2026-08-17・使い捨ての複製に対して測定）:
 *   - 4 連の中に 3 連と `## にせ終端` を置いた入力: 修正前は findings 0 件・body が
 *     `本文1` とフェンス 2 行だけへ縮み、`## にせ終端` がアンカーに採用された。修正後は body が
 *     `本文2` まで伸び、`## にせ終端` はアンカー 0 件（①赤）になる
 *   - `~~~` 版も同じ
 *   - 走査中の 201 文書に対するマスク（`[lineNo]` の列）は修正前後で全件一致
 *   - `sectionOf` の 8 呼び出し点の body は SHA-256 で 8/8 一致
 *
 * @param {string} text 文書全文
 * @param {string} file finding の帰属先（対象文書のパス）。**必須**
 * @param {object[]} findings 釣り合わないフェンスを積む先。**必須**
 */
export function linesOutsideFences(text, file, findings) {
  if (typeof file !== "string" || file === "") {
    throw new Error(`linesOutsideFences: file（finding の帰属先）は必須（受け取った値: ${file}）`);
  }
  if (!Array.isArray(findings)) {
    throw new Error(`linesOutsideFences: findings（finding を積む配列）は必須（受け取った値: ${findings}）`);
  }
  const out = [];
  /** 開いているフェンス。`null` ならフェンスの外。**パリティ（真偽 1 つ）では表せない**
   *  ——閉じられるかどうかは開いた側の文字と長さで決まるので、その 2 つを持ち歩く必要がある */
  let open = null;
  text.split("\n").forEach((line, i) => {
    if (open === null) {
      const m = /^\s*(`{3,}|~{3,})/.exec(line);
      if (m) {
        open = { char: m[1][0], len: m[1].length, at: i + 1 };
        return;
      }
      out.push([i + 1, line]);
      return;
    }
    // 閉じフェンス: 同じ文字・開いた長さ以上・情報文字列を持たない（`$` まで空白だけ）。
    // この 3 条件が「4 連の中に 3 連を書ける」理由であり、`` ` `` と `~` が互いを閉じない理由である
    // （`` ` `` も `~` も正規表現のメタ文字ではないのでエスケープは要らない）
    if (new RegExp(`^\\s*${open.char}{${open.len},}\\s*$`).test(line)) open = null;
    // フェンスの内側（閉じた行そのものを含む）は落とす
  });
  if (open !== null) {
    findings.push(
      finding(file, open.at, "コードフェンスが開いたまま閉じていない——以降の全行がフェンスの内側と判定され、この文書を走査する検査の母集団から落ちる"),
    );
  }
  return out;
}

export const finding = (file, line, message) => ({ file, line, message });

/** 正準形の**頭**——対象のバッククォートと、任意の `§ <番号>` まで。第 1 群が対象を捕る。
 *  **ラベル（`「…」`）の側は消費者ごとに違う**ので、共有するのはここまでである:
 *    - `HEADING_REF`（下）はラベルを閉じた完全形を見る
 *    - `G-near-heading-refs` の `ADJACENT_REF` は `「` が続くことだけを見る
 *    - `G-folded-heading-refs` は「ラベルが来ない」「ラベルが閉じない」を見る
 *  **文字列として持つのは、3 者が別のフラグ・別の末尾を要るからである**（`g` の有無・`$` の有無）。
 *  頭をここへ寄せる理由は `G-heading-refs` のヘッダが書いている——**再定義すると片方だけ直す形が作れる**。 */
export const REF_HEAD = "`([^`\\n]+)`\\s*(?:§\\s*[\\d.]*\\s*)?";

/** 見出し参照の正準形。**対象として認める綴りは `isRefTargetSpelling` が正本である**（ここへ写さない）。
 *  `§` には節番号を伴ってよい（`SPEC.md` §11「見た目の規範」）——番号を許さないと、
 *  節番号つきの参照は正準形へ直しても照合されず、G-near-heading-refs が「直せない指摘」を出し続ける（#727 で実測）。
 *
 *  **`g` フラグを持つので `matchAll` からだけ使う**（`matchAll` は内部で複製するため `lastIndex` を持ち越さない）。
 *  `test` / `exec` で共有すると、消費者どうしが互いの `lastIndex` を踏む。
 *  **消費者は 1 つではない**——`G-heading-refs` の照合と `dependents.mjs` の逆引きが同じ形を読む（#1140）。 */
export const HEADING_REF = new RegExp(`${REF_HEAD}「([^「」\\n]+)」`, "g");

/** 正準形の対象として認める綴り。**ここが対象綴りの正本である**——`HEADING_REF` の第 1 群に当てる。
 *  **消費者は 1 つではない**（検査群と `dependents.mjs` の逆引きが同じ述語を読む）ので、
 *  ここを広げると読む側すべての射程が同時に動く。
 *
 *  **`.mjs` を含めたのは #1155 である**（`ADR-canonical-heading-references` の 2026-08-20 追記）。
 *  スクリプトのコメントは #1138 で**走査元**に入っていたが、**対象の綴り**としては認められておらず、
 *  スクリプトを対象に置いた正準形は照合そのものが生成されなかった——撤去されたファイルを指したまま
 *  緑で推移する形がそこにあった。着地先は `ANCHOR_SPECS` のテスト名の腕が供給する。
 *
 *  **例示に対象の形を書かないこと**——`.mjs` で終わるプレースホルダはこの述語に当たり、
 *  検出器の説明が検出器を赤にする（#1155 の導入時に実測。#925 が却下 (1) で挙げた形が
 *  対象綴りの側でも起きる）。
 *
 *  **`.ps1` / `.psm1` / `.rs` は入らない**（宣言する死角）——対象にした正準形が今日 0 件であり、
 *  足しても照合が 1 件も生まれず面積だけ増える。書かれ始めたらここへ足す。 */
export const isRefTargetSpelling = (target) =>
  target.endsWith(".md") || target.endsWith(".mjs") || /^\/[a-z0-9-]+$/.test(target);

/** コメント記法の族。**拡張子ではなく記法で束ねる**——PowerShell と YAML は別の言語だが同じ `#` 族である。
 *  `.json` は入らない（コメント記法を持たない）。ここに無い拡張子は「散文の文書」として扱われる。 */
const COMMENT_FAMILY = new Map([
  [".mjs", "js"], [".js", "js"], [".cjs", "js"], [".ts", "js"],
  [".ps1", "ps"], [".psm1", "ps"], [".psd1", "ps"],
  [".sh", "hash"], [".yml", "hash"], [".yaml", "hash"], [".toml", "hash"],
]);

/** ファイル名からコメント記法の族を引く。散文の文書（`.md` / `.rs` ほか）は `null` */
export const commentFamilyOf = (file) => COMMENT_FAMILY.get((/\.[a-z0-9]+$/i.exec(file) ?? [""])[0].toLowerCase()) ?? null;

/**
 * スクリプトの**コメント行だけ**を `[lineNo, text]` で返す。
 *
 * **これが「テストファイルを外す」の代わりに置く意味の写像である**（#1138）。負の fixture が
 * 検査を赤くするのは「テストファイルだから」ではなく「**文字列リテラルに書かれたデータ**だから」で、
 * 参照はコメント＝散文に書かれる。拡張子で外すと `*.Tests.ps1` のような別の綴りが素通りし、
 * 逆に fixture を持たないテストのコメント内の参照まで落ちる。実測（この変更の直前の作業ツリー）:
 * コメント行の参照 35 件はすべて実参照、非コメント行の 19 件はすべて `*.test.mjs` の fixture だった。
 *
 * **字句解析ではない。** 行頭のコメント記号とブロックの開閉だけを見る。ゆえに
 * **テンプレートリテラルの中で `//` から始まる行はコメントと誤判定される**——これは受容する
 * 死角ではなく**赤に倒れる**側の誤りなので、沈黙せず人が解く（負の fixture をそう書かない、が回避策）。
 * 逆向きの取りこぼし（コメントと判定されなかった実コメント）は「今までどおり見ない」に落ちるだけである。
 *
 * @param {string} text ファイル全文
 * @param {string} file 拡張子から記法族を引く。`commentFamilyOf` が `null` を返す名前は**契約違反**
 */
export function linesOfComments(text, file) {
  const family = commentFamilyOf(file);
  if (family === null) {
    throw new Error(`linesOfComments: コメント記法を持たない対象（受け取った値: ${file}）`);
  }
  const out = [];
  let inBlock = false;
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    let isComment = false;
    if (family === "js") {
      if (inBlock) {
        isComment = true;
        if (line.includes("*/")) inBlock = false;
      } else if (line.startsWith("//")) {
        isComment = true;
      } else if (line.startsWith("/*")) {
        isComment = true;
        if (!line.includes("*/")) inBlock = true;
      }
    } else if (family === "ps") {
      if (inBlock) {
        isComment = true;
        if (line.includes("#>")) inBlock = false;
      } else if (line.startsWith("<#")) {
        isComment = true;
        if (!line.includes("#>")) inBlock = true;
      } else if (line.startsWith("#")) {
        isComment = true;
      }
    } else {
      isComment = line.startsWith("#");
    }
    if (isComment) out.push([i + 1, raw]);
  });
  return out;
}

/**
 * 見出し参照の検査が走査する行。**散文の文書は全行（フェンスの外側）・スクリプトはコメント行だけ**。
 * `.rs` がここで散文側に落ちるのは意図である——`.rs` は #925 から全行を走査しており、
 * その母集団を同じ変更で動かさない（検査対象を変更しながら検査を検証しない・#489）。
 */
export function refScanLines(text, file, findings) {
  return commentFamilyOf(file) === null ? linesOutsideFences(text, file, findings) : linesOfComments(text, file);
}

/**
 * 見出しで節を切り出す共有の口。**「見出しで」節を切る検査はここを通る**——節を母集団にする検査は
 * ほかにもあり、そちらは通らない（`G-clippy-disallowed` は TOML の `[dependencies]` 節を、
 * `G-ci-table` は表の区間を、それぞれ独自に切る）。
 * かつては 3 つの実装形（同レベル以上で終端 / `## ` だけで終端 / `### ` だけで終端）が
 * 並んでいた。下位の実装形は、上位の見出しが 1 本消えるだけで節が次の節へ流れ込む。
 *
 * **切り出しを 1 本にしても、向きの分析は呼び出し点ごとに残る。**
 * 母集団の広がりが沈黙になるか誤報になるかを決めるのは切り出し器ではなく、
 * **切り出した結果を食う述語**である——許可集合への所属（`docsLines.includes(cmd)`）なら
 * 広がりは沈黙、集合の一致・実在の主張なら広がりは誤報になる。ここが縛るのは
 * 「宣言（`ending`）と実際の文書構造の食い違い」だけであり、それ以上ではない。
 *
 * `ending` は節の位置の宣言で、**双方向に検算する**——`"heading"` は終端が無ければ赤、
 * `"eof"` は終端が在れば赤。片側だけにすると、宣言そのものが誰も検算しない写しとして腐り、
 * 次に読む人はそれを信じる。
 *
 * 赤にするのは 4 条件（いずれも `body: null` を返す）:
 *   ① アンカーが 0 件——見出しの改題・消滅で母集団が空になる
 *   ② アンカーが 2 件以上——先に現れた方を掴み、本物の節が照合されないまま緑になる
 *      （`G-hook-fires` が表のヘッダ多重度に対して置いた検知と同型）
 *   ③ `ending: "heading"` なのに終端が無い——節が EOF まで伸びる
 *   ④ `ending: "eof"` なのに終端が在る——宣言が腐った
 *
 * 行分割は `\r?\n` である。文書全文へ `^…$` を当てる形（旧 2 形）は CRLF チェックアウトで
 * `$` が `\r` の手前に当たらず節を見失う——CI は ubuntu ＝ LF、このリポジトリの `.gitattributes` が
 * 固定しているのは `.githooks/**` だけなので、`core.autocrlf=true` の機体で
 * `npm run governance:check` を打った場合の話である（今日の露出は 0）。
 *
 * **アンカーと終端はコードフェンスの外だけで探す**（`linesOutsideFences` を行番号のマスクとして使う）。
 * 字面だけを見ると、フェンス内の列 0 の `#` 行が終端になったり 2 本目のアンカーに数えられたりする——
 * `docs/build-commands.md` の §A のフェンスへ `# 整形` を 1 行足すと、findings 0 件のまま body が
 * 8 文字へ縮み、G-hook-commands の母集団が 8 行から 0 行になった（2026-08-17 実測）。**向きは
 * 呼び出し点ごとに違う**——許可集合への所属で判定する側は縮んでも赤くならない。
 * **body の切り出しは生の行で行う**（フェンスの中身を落とさない）——落とすと、まさに上の
 * cargo 行のようなフェンス内が母集団である検査を、この関数自身が空にしてしまう。
 *
 * **閉じていないフェンスは `linesOutsideFences` が finding にする**（そちらの doc が正本）。
 * マスクを導入した当初はここに残余が在った——フェンスがアンカーより前で開けばアンカーが消えて①が赤いが、
 * アンカーの後で開いた場合、`ending: "heading"` は③で赤くなる一方 **`ending: "eof"` は沈黙した**
 * （本来の終端見出しがマスクされて `end` が -1 のままになり、findings 0 件で body が EOF まで伸びる。
 * 2026-08-17 実測）。マスク以前はこの形が④で赤くなっていたので、沈黙はマスクが作った側だった。
 * ゆえに切り出しへ入る前にフェンスの釣り合いを検算し、崩れていれば `body: null` を返す。
 *
 * @param {string} text 文書全文
 * @param {RegExp} headingRe アンカー行にだけ当たる正規表現。**行単位で当てる**ので `^`/`$` は行頭・行末を指す。
 *   `g` / `y` は `lastIndex` の持ち越しで行ごとの判定がずれるため throw する（呼び出し側の契約違反であり、
 *   文書の欠陥ではない——`registry.mjs` が id 不一致を throw で拒むのと同じ扱い）
 * @param {{file: string, ending: "heading"|"eof", by: string}} opts file は finding の帰属先（＝対象文書）、
 *   by は `ending` を宣言している検査の id。**by は必須である**——finding の `file` が指すのは文書であって
 *   宣言の在り処ではないので、名乗らないと文書を直した人が「直す先はこの検査の `ending`」へ辿り着けない
 * @returns {{ body: string|null, findings: object[] }} body が null なら findings が非空
 */
export function sectionOf(text, headingRe, { file, ending, by }) {
  if (headingRe.global || headingRe.sticky) {
    throw new Error(`sectionOf: headingRe に g / y フラグは渡せない（lastIndex の持ち越しで行ごとの判定がずれる）: ${headingRe}`);
  }
  if (ending !== "heading" && ending !== "eof") {
    throw new Error(`sectionOf: ending は "heading" か "eof" のいずれか（受け取った値: ${ending}）`);
  }
  if (typeof by !== "string" || by === "") {
    throw new Error(`sectionOf: by（宣言している検査の id）は必須（受け取った値: ${by}）`);
  }
  const at = ` — 宣言元: ${by}`;
  const lines = text.split(/\r?\n/);
  // フェンスの外の行番号（1 起点）。**マスクとしてだけ使い、body はこの集合から組まない**
  const fenceFindings = [];
  const outside = new Set(linesOutsideFences(text, file, fenceFindings).map(([n]) => n));
  // 釣り合わないフェンスの下でマスクは信用できない——④が沈黙する形がここに在る。
  // `body: null` を返すことで「body が null なら findings が非空」の契約も保たれる
  if (fenceFindings.length > 0) return { body: null, findings: fenceFindings };
  const anchors = lines.map((l, i) => (headingRe.test(l) && outside.has(i + 1) ? i : -1)).filter((i) => i >= 0);
  if (anchors.length === 0) {
    return { body: null, findings: [finding(file, 1, `節の見出しが見つからない（アンカー: ${headingRe}）——見出しの改題・消滅で母集団が空になる${at}`)] };
  }
  if (anchors.length > 1) {
    return {
      body: null,
      findings: [finding(file, anchors[1] + 1, `節の見出し ${headingRe} が ${anchors.length} 本ある（どれが本物か決まらない・母集団の曖昧化）${at}`)],
    };
  }
  const start = anchors[0];
  const level = lines[start].match(/^(#{1,6})\s/)?.[1].length;
  if (level == null) {
    return { body: null, findings: [finding(file, start + 1, `アンカー ${headingRe} が当たった行が ATX 見出しでない（節のレベルが決まらない）: ${lines[start]}${at}`)] };
  }
  const rest = lines.slice(start + 1);
  // rest[i] は lines[start + 1 + i] ＝ 1 起点で start + i + 2 行目
  const end = rest.findIndex((l, i) => /^#{1,6}\s/.test(l) && l.match(/^#+/)[0].length <= level && outside.has(start + i + 2));
  if (ending === "heading" && end < 0) {
    return {
      body: null,
      findings: [finding(file, start + 1, `\`ending: "heading"\` の宣言に対し、同レベル以上の見出しが後方に無い（節が EOF まで伸び、母集団が広がる）: ${lines[start]}${at}`)],
    };
  }
  if (ending === "eof" && end >= 0) {
    return {
      body: null,
      findings: [finding(file, start + end + 2, `\`ending: "eof"\` の宣言に対し、同レベル以上の見出しが後方に在る（宣言が腐った。節はこの行で終端している）: ${rest[end]}${at}`)],
    };
  }
  return { body: rest.slice(0, end < 0 ? rest.length : end).join("\n"), findings: [] };
}

/** `git check-ignore` は**ファイルの存在に依らずパス名だけで判定する**（2026-08-14 実測: 不在の
 *  `test-results/never-created.json` が当たり、`docs/nonexistent-typo.md` は当たらない）——これが
 *  「CI に存在しない生成物の名前を散文へバッククォートで書けない」という表記の歪みを解く（#1088）。
 *  **読むのはチェックアウト内の `.gitignore` だけではない**——`.git/info/exclude` と、ユーザ全体の
 *  除外ファイル（`core.excludesFile`。未設定なら `$XDG_CONFIG_HOME/git/ignore`、既定
 *  `~/.config/git/ignore`）も読む。これらはどちらもチェックアウトの外（機体ごとのローカル状態）に
 *  あり CI のチェックアウトには存在しないので、**免除の面は「手元 ⊇ CI」になりうる**——手元だけで
 *  免除されるパスがあれば、その回は「手元で緑・CI で赤」が起こる（逆は起きない。CI が手元より
 *  広く免除することは無い）。実例: `.claude/agent-registry.json` は追跡された `.gitignore` に無いが
 *  `.git/info/exclude` にあり、この機体では免除される（2026-08-14 実測）。
 *  **exit 1 は「該当なし」であって失敗ではない**（失敗は 128）。git が無い・repo でない場合、
 *  および**候補のいずれか 1 件でもリポジトリ外パス（絶対パス・`..` でツリー外へ出る相対パス）で
 *  status が 128 になった場合**は空集合を返す——後者は batch 単位の判定ゆえ、1 件の汚染が
 *  同じ回の他の候補の免除も道連れに落とす（向きは赤側＝安全。何も免除しない側へ倒す）。
 *  **決定的性**: 同一チェックアウト・同一機体では再現する（ネットワーク・時刻・環境変数に依らない）。
 *  機体をまたぐ決定性は無い（上記のとおり `.git/info/exclude` 等が機体ごとに違う）。 */
export function gitIgnoredPaths(paths, root = process.cwd()) {
  if (paths.length === 0) return new Set();
  const r = spawnSync("git", ["check-ignore", "--stdin", "-z"], {
    cwd: root,
    input: paths.join("\0"),
    encoding: "utf8",
  });
  if (r.error || r.status === 128) return new Set();
  return new Set(r.stdout.split("\0").filter(Boolean));
}

/**
 * アンカーの種類（ATX 見出し・番号付きリスト項目・太字リード・テスト名）。**1 行に当てる形で持つ**——
 * `g` フラグを付けないのは、行ごとに `exec` する消費者が `lastIndex` を持ち越さないためである。
 *
 * **`depth` は節の入れ子を決める。** ATX は `#` の数、残りは最も深い 7。
 * 着地判定（`collectAnchors`）と節境界（`dependents.mjs` の `sectionsOf`）が**同じ一覧を読む**ので、
 * 種類を足したときに片方だけが知っている状態を作れない（#1140 で 2 か所へ写していたのを畳んだ）。
 *
 * **テスト名の腕（`describe` / `it` の第 1 引数）は #1155 で足した。** `.mjs` を対象の綴りへ入れた以上、
 * 着地先が要る——`.mjs` には ATX 見出しが無く、`//! - **…**` は行頭が `//!` なので太字リードにも当たらない
 * （実測）。**この腕を持たずに対象綴りだけ広げてはならない**: 実在するファイルを指す参照が
 * 「着地しない」で恒久的に赤くなる（独立導出が拡張前の写しで実測・2 件）。
 * **行頭に錨を張る**ので、`split("\n")` のように行の途中に現れる `it(` には当たらない。
 *
 * **受容する残余——この腕は `.mjs` の外でも当たりうる。** `docs/superpowers/plans/` の擬似コードには
 * この形の行が実在する（`.rs` には無い）。**今日それが着地候補になることは無い**——`collectAnchors` は
 * 参照の**対象にされたファイル**に対してだけ呼ばれ、当該ディレクトリを対象にした正準形が生きた層に
 * 無いためである（#1155 で実測）。書かれた日には偽のアンカーが供給される側へ倒れる。
 * **走査元の除外はこの残余を塞がない**——`headingRefDocs` の doc が言うとおり、除外が絞るのは走査元
 * だけで、参照先の解決は `snapshot.files` 全体に対して行われる。
 *
 * **`describe` と `it` へ同じ深さを与えている**ので、`sectionsOf` から見て両者は入れ子にならず、
 * `describe` の節は次のアンカーで閉じる。着地判定（前方一致）はこれで足りるが、`dependents.mjs` の
 * 節境界は `describe` 全体を指さない。合否を持たない計器の側の精度なので受容する。
 */
export const ANCHOR_SPECS = [
  { re: /^(#{1,6})\s+(.+?)\s*$/, depth: (m) => m[1].length, label: (m) => m[2] },
  { re: /^\s*\d+[.)]\s+(.+?)\s*$/, depth: () => 7, label: (m) => m[1] },
  { re: /^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*/, depth: () => 7, label: (m) => m[1] },
  { re: /^\s*(?:describe|it)\(\s*"([^"]*)"/, depth: () => 7, label: (m) => m[1] },
];

export function collectAnchors(text) {
  const out = [];
  for (const line of text.split("\n")) {
    for (const spec of ANCHOR_SPECS) {
      const m = spec.re.exec(line);
      if (m) out.push(spec.label(m));
    }
  }
  return out;
}

export const normAnchor = (s) => s.replace(/[`*「」\s]/g, "");

/** 参照文字列 → リポジトリ内パス。解決できなければ null */
export function resolveRefTarget(snapshot, doc, target) {
  if (/^\/[a-z0-9-]+$/.test(target)) {
    const p = `.claude/skills/${target.slice(1)}/SKILL.md`;
    return snapshot.files.includes(p) ? p : null;
  }
  if (!isRefTargetSpelling(target)) return null;
  const norm = (p) => path.posix.normalize(p);
  const rel = norm(path.posix.join(path.posix.dirname(doc), target)); // 文書ディレクトリ基準を優先
  if (snapshot.files.includes(rel)) return rel;
  if (snapshot.files.includes(norm(target))) return norm(target);
  const suffix = `/${norm(target)}`;
  if (suffix.includes("..")) return null;
  const hit = snapshot.files.filter((f) => f.endsWith(suffix));
  return hit.length === 1 ? hit[0] : null;
}

/** ルート `Cargo.toml` の `[workspace] members`（ディレクトリ相対パス）を導出する唯一の口。
 *  返り値 `{ members, error }` の `error` は**母集団の欠落**（fail-closed）——読めない・`[workspace]` 節が無い・
 *  `members` 行が無い・0 件・glob 要素。glob（`crates/*`）は展開器を持たないので「読めなかった」側へ倒す。
 *  `[workspace]` セクションへスコープするのは、`default-members = [...]` を足したときに
 *  全文正規表現が**先に現れた方**を拾うため（`.claude/hooks/post-edit.test.mjs` のカナリアと同じ形）。 */
export function workspaceMembers(snapshot) {
  const src = snapshot.read("Cargo.toml");
  if (src == null) return { members: [], error: "ルート Cargo.toml が読めない" };
  const section = src.match(/\[workspace\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/);
  if (section == null) return { members: [], error: "ルート Cargo.toml に [workspace] セクションが無い" };
  const m = section[1].match(/^members\s*=\s*\[([^\]]*)\]/m);
  if (m == null) return { members: [], error: "[workspace] に members 行が無い（書式が変わった）" };
  const members = m[1]
    .split(",")
    .map((s) => s.trim().replace(/^"|"$/g, ""))
    .filter((s) => s.length > 0);
  if (members.length === 0) return { members: [], error: "[workspace] members が 0 件" };
  const glob = members.find((s) => s.includes("*"));
  if (glob) return { members: [], error: `members に glob 要素が在る（展開器を持たない）: ${glob}` };
  return { members, error: null };
}

/** G-references/G-spec-sections の対象文書（ガバナンス文書群）。docs/superpowers/ は歴史資料（#589 で非規範化）ゆえ除外 */
/** G-references / G-spec-sections の走査元。`docs/adr/` を除くのは**凍結された歴史**の契約
 *  （`ADR-adr-frozen-history`）——ADR 本文は決定日時点の世界の記述であり、そこから外への参照
 *  （パス・SPEC 節）は生きた層の改名・移動に追随させない。守るのは実在の辺だけ
 *  （生きた層 → ADR と ADR → ADR の短縮引用 = `adrCitationDocs` が明示的に持つ）
 *
 *  **保証は狭い**: 3 検査が照合するのは、ここが返した文書の中に書かれた参照だけである
 *  （G-adr-citations は `adrCitationDocs` で入力を足す）。ここに入らない層——**ルート直下へ新設した
 *  文書**など——に書いた実在しない参照・`SPEC §N`・ADR 引用は素通りする（2026-08-09 実測・#1008）。
 *  **リポジトリ全体を見る検査ではない。**
 *  なお crate 名の正規表現は `MODULE_INDEX_CRATES` と同じ一覧を独立に持つ 2 本目であり、
 *  真の母集団はどちらもルート `Cargo.toml` の `[workspace] members` である。**crate を増やしたときの
 *  この正規表現の更新漏れは `governance-check.test.mjs` の母集団カナリア（#701）が `npm test` で
 *  捕まえる**（詳細は `MODULE_INDEX_CRATES` の doc）——**カナリアが見ないのはルート文書の配列の側**
 *  であり、そこへの足し忘れは今も沈黙する。 */
export function governanceDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      ["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "SPEC.md"].includes(f) ||
      (f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("docs/adr/")) ||
      /^(snotra-core|snotra-egui-runtime|src-tauri|snotra-settings)\/CLAUDE\.md$/.test(f) ||
      RULE_FILE_RE.test(f) ||
      /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f),
  );
}

/** `.claude/rules/` 直下の md の形。**`governanceDocs` の腕と `G-rules-globs` の母集団が同じ集合を要る**ため
 *  綴りを 1 か所に閉じる（`globToRegex` / `rulePathPatterns` を #1143 でここへ寄せたのと同じ理由）。
 *
 *  **`governance-manifest.mjs` の `rules` 列はここを読まない。** あちらが `governanceDocs` の定義とは
 *  独立にファイル走査だけから導出しているのは意図であり、その二重導出こそが母集団の裏取りになる
 *  （正本は同ファイル `diffManifest` の doc）。写しに見えるが畳んではならない側である。 */
export const RULE_FILE_RE = /^\.claude\/rules\/[^/]+\.md$/;

/** `.claude/rules/` 直下の md。 */
export function ruleDocs(snapshot) {
  return snapshot.files.filter((f) => RULE_FILE_RE.test(f));
}

/** workspace member の `src/` 配下の `.rs`。
 *  crate の一覧はルート `Cargo.toml`（`workspaceMembers`）が SSOT である。
 *  **`G-module-index` はこれを使わない**——あちらの母集団は `MODULE_INDEX_CRATES` から出る
 *  2 本目の導出である。今日は同じ集合を返すが、**同じ SSOT から導いてはならない**側である。
 *
 *  **2 本の導出が食い違わないことを、今日は誰も見ていない。** かつては
 *  「`moduleIndexSources` は本関数の結果の部分集合」を `npm test` で縛るテストが在ったが、
 *  それは錨の層と一緒に #1152 で撤去された（`ADR-governance-anchor-layer-discarded`「受容する残余」）。
 *  **畳んではならないという制約だけが残り、畳まれていないことの検知は無い**（#1155 で確認）。 */
export function crateSourceFiles(snapshot) {
  const { members } = workspaceMembers(snapshot);
  return snapshot.files.filter((f) => f.endsWith(".rs") && members.some((m) => f.startsWith(`${m}/src/`)));
}

/**
 * G-heading-refs / G-near-heading-refs の走査元のうち **md の腕**。見出し参照はガバナンス文書の外
 * （`PERFORMANCE.md`・`.claude/agents/`）にも書かれ、実際にそこで腐っていた
 * （`PERFORMANCE.md` が src-tauri の WebView2 期の節を指したまま残っていた。**消えた節の名前は
 * 正準形で書かない**ので散文にしてある——書けばこの検査が自分のコメントを赤にする）ため母集団を広く取る。
 * 除外は履歴資料（`docs/superpowers/`）・作業バッファ（`workspace/`・`/implement` が削除する）・
 * 凍結された歴史（`docs/adr/`・`ADR-adr-frozen-history`。実測で照合 203 件中 86 件が
 * ADR 内＝歴史の研磨だった）。**除外が絞るのは走査元だけである**——参照先のアンカー解決は
 * `snapshot.files` 全体に対して行われるため、生きた層から ADR の見出しを指す参照は除外後も照合される。
 * **ソースの腕（`.rs`）は `headingRefSourceDocs` が別に持つ。** 束ねて 1 本にしないのは、
 * `runAll` の 0 件検知が**母集団ごとに 1 本ずつ**要るからである（`staleDocs` / `staleGuides` と
 * 同型——和にすると md 側の長さが `.rs` の消滅を埋めて永久に沈黙する）。
 */
export function headingRefDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      f.endsWith(".md") &&
      !f.startsWith("docs/superpowers/") &&
      !f.startsWith("workspace/") &&
      !f.startsWith("docs/adr/"),
  );
}

/**
 * 同じ走査元の **ソースの腕**（`.rs`）。#921 で `SPEC.md` の節の中身を移したとき、`.rs` 側の参照は
 * 手で直す必要があり検査は緑のままだった。`.rs` のコメントには正準形の参照が 27 件あり（#925 実測）、
 * そのすべてが参照先の改題・移動・削除に対して沈黙していた。
 *
 * **Rust のテストコードを外さない。** `adrCitationDocs` が `*.test.mjs` を外すのは「フィクスチャが
 * 赤経路を測るため意図的に実在しない名前を持つ」からであって、テストだからではない。Rust の
 * テストコメントに書かれた規範への参照は本物であり、腐れば同じ害になる——#925 が見つけた腐り 1 件は
 * 現に `#[cfg(test)]` の内側にあった（`snotra-settings/src/tabs/visual.rs`）。ゆえに、
 * この走査元へ **`#[cfg(test)]` 以降を落とす変換を入れてはならない**——テストコードを外さないのは
 * 意図であり、`lib.test.mjs` の種 3（`#[cfg(test)]` の内側のコメントも見る）が**この変換を**固定する
 * ——入れるとあの it が落ちる。**`governance:check` の全検査と `vitest run scripts/` を通して、
 * 落ちるのはそれだけである**（2026-08-21 実測）——`governance:check` 自身は緑のまま、
 * `#[cfg(test)]` の内側の生きた正準形が黙って照合から外れる。
 *
 * **`.mjs` / `.ps1` はここではなく `headingRefCommentDocs` が持つ**（#1138）。#925 はこれらを
 * 却下したが、その裁定の実測は `.mjs` だけを見ており、`.ps1` / `.psm1` には本物の腐りが残っていた。
 * 却下の理由（フィクスチャと検出器自身のコメントが赤になる）は、走査をコメント行へ限る
 * 意味の写像で解ける——詳細は `headingRefCommentDocs` と `linesOfComments`。
 *
 * **md の腕が持つ除外接頭辞を共有しない。** `docs/adr/` の除外は「ADR **本文**は決定日時点の世界の
 * 記述として凍結する」という散文についての契約であり（`ADR-adr-frozen-history`）、`docs/superpowers/`
 * も #589 で非規範化された文書である。どちらもコードについては何も決めていない——決まっていない契約を
 * 述語で主張しない（該当する `.rs` は 0 件ゆえ挙動の差も無い）。
 *
 * **受容する残余**: rustdoc のコードフェンス（`///` に続く ``` 行）は `linesOutsideFences` の
 * `/^\s*```/` に当たらないため、rustdoc の例の中に書かれた参照も照合される（今日の影響は 0 件）。
 */
export function headingRefSourceDocs(snapshot) {
  return snapshot.files.filter((f) => f.endsWith(".rs"));
}

/**
 * 同じ走査元の **スクリプトの腕**（コメント記法を持つ全ファイル。走査は `linesOfComments` が
 * コメント行へ限る）。#1137 で `/implement` の見出しを改名したとき、`.md` の壊れた参照は
 * 名指しされたが `scripts/race-boundaries.mjs` の 1 件は名指しされなかった——見つけたのは
 * 委譲したレビュアであって機構ではない（#1138）。
 *
 * **拡張子を並べた列ではなく `commentFamilyOf` を母集団の述語にする。** 「どのファイルを見るか」と
 * 「その中のどの行を見るか」が同じ 1 つの写像から出るので、片方だけ足して他方を忘れる形が作れない。
 *
 * **規範はすでにここへ配送されている**——`.claude/rules/governance-docs.md` の `paths` が `scripts/` 配下を
 * 覆っており（正本はその frontmatter。ここに glob を写さない）、正準形で書けと言いながら検めていない
 * 状態だった（`.rs` の非対称はこの逆で、検めるが規範を配送しない）。
 *
 * **腕を 3 本目として分ける理由は 2 本目と同じである**——`runAll` の 0 件検知が母集団ごとに
 * 1 本ずつ要る（和にすると `.md` の長さが他の腕の消滅を埋めて永久に沈黙する）。
 *
 * **受容する残余**: `.json` はコメント記法を持たないので入らない。文字列リテラルの中に書かれた
 * 参照（hook の案内文が文書の見出しを引く形など）も見ない。PR 本文と凍結層も従来どおり視界の外である。
 */
export function headingRefCommentDocs(snapshot) {
  return snapshot.files.filter((f) => commentFamilyOf(f) !== null);
}

/** 3 本の腕の**和**。腕ごとの 0 件検知は `runAll` が別に持つので、束ねてよいのは走査元として渡すときだけである。
 *  **和をここに 1 つ置く**——消費者（`governance-check.mjs` の検査と `dependents.mjs` の逆引き）が
 *  それぞれ連結を書くと、腕を足したとき片方だけが知っている状態が作れる（#1140） */
export const allHeadingRefDocs = (snapshot) => [
  ...headingRefDocs(snapshot),
  ...headingRefSourceDocs(snapshot),
  ...headingRefCommentDocs(snapshot),
];

/** 語彙源ではなく検査対象になる、`.claude/**` の外の**固定パス**文書
 *  （意図の SSOT・常時ロードの規範・設定 UI のデザイン規約）。
 *  **静的リテラルであること自体が fail-closed である**——読めなければ `scanStaleIdentifiers` が
 *  「母集団の欠落」を出すので、グロブ由来の母集団（`staleIdentifierGuideDocs`）と違って
 *  `runAll` 側の 0 件検知を別に置く必要がない。
 *  **保証は狭い**——「意図の SSOT」級の文書を新設してここへ足さなければ、その文書の腐り識別子は
 *  一度も照合されない（2026-08-09 実測: ルート直下に新設した文書へ実在しない識別子を 3 形置いても
 *  照合件数が動かなかった・#1008）。 */
// `export` を持たない——#1094 で facade からの import が消え、外部の消費者が 0 になった。
// この PR が確立した「公開面は実際の消費者と一致させる」をこの行にも当てる（`staleIdentifierTargets` の内部利用のみ）。
const STALE_EXTRA_DOCS = ["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"];

/** 規範の散文。skills / rules / agents の md。
 *  **検査対象の全体ではない**——`staleIdentifierTargets` と分けてあるのは、`runAll` の
 *  「対象 md が 0 件（母集団の欠落）」が `.claude/**` の消滅で鳴り続けるためである
 *  （`STALE_EXTRA_DOCS` を混ぜると長さが常に 1 以上になり、その検知が永久に沈黙する） */
export function staleIdentifierDocs(snapshot) {
  return snapshot.files.filter((f) => /^\.claude\/(skills\/.*|rules\/[^/]+|agents\/[^/]+)\.md$/.test(f));
}

/** G-stale-identifiers の母集団のうち、**グロブ由来**の開発ガイド（`docs/**`）。
 *  除くのは `docs/superpowers/`（#589 で非規範化された当時の設計）と `docs/adr/`（却下案＝
 *  **もう存在しない案**を書く場所）。**基準は「日付を持つか」ではなく「もう成り立たないことを書く場所か」である**
 *  ——`docs/design/` は `status: Agreed` で `docs/architecture.md` が現在形で指す先ゆえ含める。
 *  `docs/adr/` の除外は #893 当時この検査だけの非対称だったが、`ADR-adr-frozen-history` で
 *  全検査（G-references / G-heading-refs 等の走査元）へ揃った。
 *  **静的リテラルと違い空になっても自分では鳴れない**ので `runAll` が 0 件検知を持つ */
export function staleIdentifierGuideDocs(snapshot) {
  return snapshot.files.filter(
    (f) => f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("docs/adr/"),
  );
}

/** G-stale-identifiers の検査対象。規範の散文 + 開発ガイド + 固定パスの文書。
 *  `STALE_EXTRA_DOCS` は実在を問わず加える——読めなければ `scanStaleIdentifiers` が母集団の欠落として鳴る */
export function staleIdentifierTargets(snapshot) {
  return [...staleIdentifierDocs(snapshot), ...staleIdentifierGuideDocs(snapshot), ...STALE_EXTRA_DOCS];
}

/** TOML の 1 行から**引用符の外の** `#` 以降を落とす。**引用符を見ない実装にしてはならない**——
 *  `src-tauri/clippy.toml` の reason は `（#751）` を含み、素朴な `replace(/#.*$/, "")` は行を途中で切る
 *  （#950 で実測。切れた先に `path` が在れば禁止集合が丸ごと消えたように見える）。 */
export function stripTomlComment(raw) {
  let out = "";
  let inString = false;
  for (let i = 0; i < raw.length; i++) {
    const c = raw[i];
    if (c === '"' && raw[i - 1] !== "\\") inString = !inString;
    if (c === "#" && !inString) break;
    out += c;
  }
  return out;
}

/** TOML の 1 行から行末コメントを落として trim する。`[lints]  # opt-in` も有効な TOML ゆえ、
 *  厳密文字列比較のままだと表記の揺れで false negative になる（#713） */
export const tomlLine = (raw) => stripTomlComment(raw).trim();

/** Cargo の lints テーブルの値から level を取る。文字列形（`= "deny"`）とテーブル形
 *  （`= { level = "deny", priority = 1 }`）の 2 形を受ける。**rustdoc と clippy の 2 検査が共有する**——
 *  cargo が 3 つ目の表記を足したとき、直す場所が 1 か所であるために切り出してある（#950）。 */
export const lintLevel = (value) => (value.startsWith("{") ? (value.match(/level\s*=\s*"([^"]+)"/)?.[1] ?? null) : (value.match(/^"([^"]+)"$/)?.[1] ?? null));

// ---------------------------------------------------------------------------
// `.claude/rules/` の frontmatter `paths` を読む道具。**2 つの検査が import する**ため
// （`G-rules-globs` = glob → 実ファイルが 0 件 / `G-rules-script-coverage` = 実ファイル → glob が 0 件）、
// 冒頭が定める掲載条件に当たる。写しにすると glob の意味論が検査ごとに独立に腐る。
// ---------------------------------------------------------------------------

/** documented 意味論（bare 名 = ルート直下のみ・`**` = 階層横断・`{a,b}` ブレース）の自前変換。
 *  **harness の配送判定の再現ではなく近似である**——言えるのは「この意味論で覆われているか」までで、
 *  「harness が実際に配送するか」ではない（`**` が 3 段跨ぐことだけは 2026-08-19 に実測・#1143）。 */
export function globToRegex(pattern) {
  let re = "";
  let i = 0;
  while (i < pattern.length) {
    const c = pattern[i];
    if (c === "{" && pattern.indexOf("}", i) === -1) {
      re += "\\{"; // 未閉ブレースは literal 扱い（無限ループ防止・0 件マッチの明示的な赤に倒れる）
      i += 1;
    } else if (c === "*") {
      if (pattern.startsWith("**/", i)) {
        re += "(?:.*/)?";
        i += 3;
        continue;
      }
      if (pattern.startsWith("**", i)) {
        re += ".*";
        i += 2;
        continue;
      }
      re += "[^/]*";
      i += 1;
    } else if (c === "{") {
      const end = pattern.indexOf("}", i);
      re += `(?:${pattern
        .slice(i + 1, end)
        .split(",")
        .map((s) => s.replace(/[.+^$()|[\]]/g, "\\$&"))
        .join("|")})`;
      i = end + 1;
    } else {
      re += /[.+^$()|[\]?\\]/.test(c) ? `\\${c}` : c;
      i += 1;
    }
  }
  return new RegExp(`^${re}$`);
}

/** rule 本文から `paths` の glob 文字列を取り出す（frontmatter ブロックの中だけを見る。CRLF checkout 耐性）。
 *  **`G-skill-table` の frontmatter 読みとは束ねない**——あちらが取り出すのは別のキーであり、
 *  片方だけが変わる将来を挙げられる＝別概念である。 */
export function rulePathPatterns(text) {
  const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] ?? "";
  return [...fm.matchAll(/^\s*-\s*"([^"]+)"/gm)].map((m) => m[1]);
}
