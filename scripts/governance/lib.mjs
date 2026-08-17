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

/** コードフェンス（``` 行）の内側を落として [lineNo, text] を返す（誤検出源: SPEC.md の TOML コメント等） */
export function linesOutsideFences(text) {
  const out = [];
  let inFence = false;
  text.split("\n").forEach((line, i) => {
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      return;
    }
    if (!inFence) out.push([i + 1, line]);
  });
  return out;
}

export const finding = (file, line, message) => ({ file, line, message });

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
 * **アンカーと終端はコードフェンスの外だけで探す**（`linesOutsideFences` を行番号の遮蔽として使う）。
 * 字面だけを見ると、フェンス内の列 0 の `#` 行が終端になったり 2 本目のアンカーに数えられたりする——
 * `docs/build-commands.md` の §A のフェンスへ `# 整形` を 1 行足すと、findings 0 件のまま body が
 * 8 文字へ縮み、G-hook-commands の母集団が 8 行から 0 行になった（2026-08-17 実測）。**向きは
 * 呼び出し点ごとに違う**——許可集合への所属で判定する側は縮んでも赤くならない。
 * **body の切り出しは生の行で行う**（フェンスの中身を落とさない）——落とすと、まさに上の
 * cargo 行のようなフェンス内が母集団である検査を、この関数自身が空にしてしまう。
 * **受容する残余**: 閉じていないフェンスがあると以降の全行が「内側」になり、アンカーが消えて①が赤くなる
 * （露出は今日 0 件）。
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
  // フェンスの外の行番号（1 起点）。**遮蔽としてだけ使い、body はこの集合から組まない**
  const outside = new Set(linesOutsideFences(text).map(([n]) => n));
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

/** 参照先になりうる位置（ATX 見出し / 番号付きリスト項目 / 太字リード） */
export function collectAnchors(text) {
  const out = [];
  for (const m of text.matchAll(/^#{1,6}\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*\d+[.)]\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*/gm)) out.push(m[1]);
  return out;
}

export const normAnchor = (s) => s.replace(/[`*「」\s]/g, "");

/** 参照文字列 → リポジトリ内パス。解決できなければ null */
export function resolveRefTarget(snapshot, doc, target) {
  if (/^\/[a-z0-9-]+$/.test(target)) {
    const p = `.claude/skills/${target.slice(1)}/SKILL.md`;
    return snapshot.files.includes(p) ? p : null;
  }
  if (!target.endsWith(".md")) return null;
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
      /^\.claude\/rules\/[^/]+\.md$/.test(f) ||
      /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f),
  );
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
 * 現に `#[cfg(test)]` の内側にあった（`snotra-settings/src/tabs/visual.rs`）。`productionOnly` 相当を
 * 「G-stale-identifiers との対称性の完成」として後から入れてはならない（その非対称は意図である）。
 *
 * **`.mjs` / `.ps1` は入れない**（#925 の裁定）。実測した finding 9 件の内訳は、6 件が
 * `governance-check.test.mjs` のフィクスチャ（赤経路を測るため意図的に実在しない名前を持つ）、
 * 残り 3 件が**本ファイル自身のコメント**（正準形の例示 1・`…` で切り詰めた表記 1・本物の腐り 1）。
 * 入れれば検出器の説明が検出器を赤にする（`docs/adr/` を全検査の走査元から外したのと同クラスの理由）。
 * **腐り 1 件は #925 で直した**——母集団に入れなくても、直せるものは直す。
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
