// governance:check — ガバナンス文書の決定的検査（#587）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された
// shebang 行は vitest の transform を SyntaxError で落とす（PR #592 で実測。
// 他の *.mjs も同じ理由で shebang なし。起動は常に `node scripts/...` 経由）。
//
// PostToolUse hook は `.md`・rules・skills に検査を割り当てない（#497 で受容した残余）。
// 本スクリプトはその残余のうち決定的に照合できる項目を PR CI（governance-check job）と
// `npm run governance:check` で引き取る。意味判断（責務の妥当性・npm 系ラッパーの等価判断・
// メモリ整合）は `/health-check` に残る（cargo フラグ照合は G-hook-commands が機械化済み・#589）。
//
// 契約:
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）
// - findings ゼロ → exit 0 + 照合母集団の件数を印字（根拠の接地）
// - findings あり → exit 1 + `file:line` 付き全件列挙。免除注記の機構は設けない
// - 空母集団（対象文書 0 件・rules 0 件・skills 0 件）は明示 fail（沈黙経路の閉塞）
// - 各検査はスナップショット注入の純関数（scripts/governance-check.test.mjs がフィクスチャで
//   フォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する）
//   - **例外は G-hook-fires ただ 1 つ**: 判定の再実装を避けるため `.claude/hooks/post-edit.mjs` の
//     `selectChecks` を import し、既定引数として注入する（理由は同検査のコメント）。ゆえに
//     **snapshot の root（cwd）と import 元（スクリプト相対）が同じツリーであること**を前提とする——
//     `npm run governance:check` 経由では常に成り立つが、別ツリーのスクリプトを叩けば崩れる
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
// G-hook-fires は判定を再実装せず hook の純関数そのものを呼ぶ（理由は同検査のコメント）。
// post-edit.mjs は import しただけでは main() を走らせない（I13 のガード）。
import { selectChecks } from "../.claude/hooks/post-edit.mjs";

/** 実在検査の対象と見なすソース系拡張子（G-references）。ランタイム生成物（.bin/.bak 等）は含めない。
 *  **保証は狭い**——バッククォート内のパス様参照は、拡張子がここに無ければ（`/` を含んでいても）
 *  静かにスキップされる（2026-08-09 実測: `.psm1` の実在しないパスが素通り・#1008）。 */
const REF_EXTENSIONS = /\.(md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css)$/;
/** 走査から除外するディレクトリ。名前ベース（任意の深さの生成物）とルート相対パス
 *  （untracked バッファ）を分ける——`ui/src/workspace/` のような将来の同名ソースを気づかれないまま
 *  落とさないため、PATHS 側はルート錨止めにする
 *  **PATHS の照合は `rel` の完全一致である**——一致したディレクトリへ降りないので配下ごと落ちる。
 *  `docs/.superpowers` も `.superpowers-extra` も `rel` が一致しないので巻き込まない（#728）
 *  `.superpowers/` は SDD（subagent-driven-development）の作業バッファで、gitignore 済みゆえ CI の
 *  チェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で別の母集団を見る（#722）。 */
const WALK_EXCLUDE_NAMES = new Set([".git", "node_modules", "target", "dist"]);
const WALK_EXCLUDE_PATHS = ["workspace", ".claude/worktrees", ".superpowers"];

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
function linesOutsideFences(text) {
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

const finding = (file, line, message) => ({ file, line, message });

// ---------------------------------------------------------------------------
// G-module-index — 各サブディレクトリ CLAUDE.md「モジュール構成」↔ 実ファイルの双方向照合。
// basename 包含方式: ディレクトリ集約行（`commands/` のベア名列挙）・`tabs/` プレフィックス省略・
// 1 行複数バッククォートをパースせずに済ませる意図的な弱化（wrong-directory 検出は放棄）。
// ---------------------------------------------------------------------------
// ui は #532 SU7 のフロント撤去で消滅（ui/CLAUDE.md ごと削除）
// snotra-egui-runtime は #701 で追加。「#532 の検証層」として作られたまま母集団から漏れており、
// SU7 で製品の描画層になった後も更新されていなかった（G-references の governanceDocs も同時に是正）
/** G-module-index が照合する crate。**本検査の保証は狭い**——crate を新設してここへ足さなければ、
 *  その `CLAUDE.md` のモジュール構成は順方向も逆方向も一度も照合されず `governance:check` は緑を
 *  返す（2026-08-09 実測: member を 1 つ増やし、その索引へ実在しない `.rs` を書いても緑・#1008）。
 *  真の母集団はルート `Cargo.toml` の `[workspace] members` であり、この表はその写しである。
 *  **ただし写しのずれ自体は本ファイルの外で固定されている**——`governance-check.test.mjs` の
 *  母集団カナリア（#701）が実 `Cargo.toml` を読み、`CLAUDE.md` を持つ member が本表と
 *  `governanceDocs()` の**両方**に載ることを `npm test` で強制する。**残る穴は `CLAUDE.md` を
 *  持たない crate だけで、そのとき照合すべき索引もまだ無い**（`skip-ci` ラベルの付いた PR では
 *  そのカナリアも走らない）。 */
export const MODULE_INDEX_CRATES = {
  "snotra-core": { src: "snotra-core/src/", exts: /\.rs$/ },
  "snotra-egui-runtime": { src: "snotra-egui-runtime/src/", exts: /\.rs$/ },
  "src-tauri": { src: "src-tauri/src/", exts: /\.rs$/ },
  "snotra-settings": { src: "snotra-settings/src/", exts: /\.rs$/ },
};

export function checkModuleIndex(snapshot, crates = Object.keys(MODULE_INDEX_CRATES)) {
  const findings = [];
  const allBasenames = new Set(snapshot.files.map((f) => f.split("/").pop()));
  for (const crate of crates) {
    const cfg = MODULE_INDEX_CRATES[crate];
    const mdPath = `${crate}/CLAUDE.md`;
    const text = snapshot.read(mdPath);
    if (text == null) {
      findings.push(finding(mdPath, 1, "CLAUDE.md が読めない（G-module-index 母集団の欠落）"));
      continue;
    }
    const section = text.split(/^## モジュール構成$/m)[1]?.split(/^## /m)[0];
    if (!section) {
      findings.push(finding(mdPath, 1, "「モジュール構成」節が見つからない"));
      continue;
    }
    // 順方向: 節内のバッククォート付きソースファイル名 → basename がリポジトリに実在。
    // **見るのは直下の正規表現が挙げる拡張子だけである**——`` `foo.mjs` `` のような他種の
    // バッククォート参照は実在照合されない（2026-08-09 実測・#1008）。どれを対象にするかは
    // 本プロジェクトの編集方針であって、外部仕様の写しではない。
    for (const m of section.matchAll(/`([^`\n]+\.(?:rs|ts|tsx|html))`/g)) {
      const token = m[1];
      if (/[*?{]/.test(token)) continue; // glob・パターン例は対象外
      const base = token.split("/").pop();
      if (!allBasenames.has(base)) {
        findings.push(finding(mdPath, 1, `索引に記載の \`${token}\` に対応する実ファイル（basename: ${base}）が無い`));
      }
    }
    // 逆方向: production ファイルの basename が CLAUDE.md 本文に出現
    const production = snapshot.files.filter(
      (f) => f.startsWith(cfg.src) && cfg.exts.test(f) && !(cfg.excludeTest && cfg.excludeTest.test(f)),
    );
    for (const f of production) {
      const base = f.split("/").pop();
      if (!text.includes(`\`${base}\``) && !text.includes(`/${base}\``)) {
        findings.push(finding(mdPath, 1, `実ファイル ${f} が索引（本文のバッククォート）に見当たらない`));
      }
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-module-linkage — `<crate>/src/**/*.rs` が crate ルートから `mod` 宣言で到達できるか（#1085）。
//
// **G-module-index が塞がない足を塞ぐ。** 両者は同じ「`.rs` を足したときの取りこぼし」を守るが、
// 見ているものが違う——G-module-index は実ファイル ↔ `CLAUDE.md` の索引、本検査は `mod` の到達性。
// 足ごとに壊して測った結果（2026-08-14・#1085）:
//   索引にも `mod` にも書かない → G-module-index が赤
//   **索引には書き、`mod` 宣言だけ忘れる → どの検査も緑だった**（本検査が塞ぐのはこの足）
//
// **`mod` 忘れは cargo も LSP も報せない。** 未リンクの `.rs` は `cargo fmt/clippy/test` の視界に無く
// （PostToolUse hook は沈黙する）、rust-analyzer は当該ファイルを読むが `unlinked-file` を publish
// しない（#1085 で stdio クライアントから生の publishDiagnostics を読んで実測）。最悪の帰結は
// `#[cfg(test)] mod tests` を持つファイルが 1 度もコンパイルされず**テストが黙って走らない**ことである。
//
// 母集団はルート `Cargo.toml` の `[workspace] members`（`workspaceMembers` が唯一の口・関数巻き上げで
// 参照する）。**`MODULE_INDEX_CRATES` を使わない**——あれは同じ members の写しで、その母集団カナリアが
// 縛るのは `CLAUDE.md` を持つ member だけであり、`CLAUDE.md` の無い crate を黙って飛ばす。
// リンク性は `CLAUDE.md` の有無と無関係である。
//
// 射程外（`mod` 宣言を要さないまま正当なもの）: cargo が target として自動発見するもの——crate 直下の
// `tests/*.rs`・`benches/`・`examples/`・`build.rs` は母集団を `<crate>/src/` に閉じることで外れ、
// **`src/bin/` は `src/` の内側なので明示的に除外する**（除外しないと `mod` 宣言の書き忘れという
// 誤った直し方を指示する赤が出る。現在 0 件）。
//
// **判定は「行頭ちょうどの `mod name;`」だけを拾う。** 空白を許すと、インライン `mod x { ... }` の中の
// `mod y;` を拾ったうえで基準ディレクトリを外し、**同名の兄弟ファイルへ誤って一致して孤児を緑にする**
// （2026-08-14 のレビューで実測）。列を固定すればインライン内は一切拾わず、当該ファイルは未到達＝
// 赤に倒れる。同じ理由で、判定前に**コードでない部分**（コメント・文字列・char リテラル）を
// 1 パスの字句スキャナで潰す。**この判定を誤れば両方向へ倒れる**——潰しすぎれば本物の宣言を
// 見失って赤、潰し足りなければ文字列やコメントの中の綴りを拾って孤児を緑にする。ゆえに
// `governance-check.test.mjs` のカナリアが**両方向**を固定し、さらに実リポジトリの全 `.rs` へ
// 宣言を注入して拾えることをドッグフードで測る（2 パスだった初版は、実ファイル 3 枚が
// 誤検出の縁に居た——`"http://x"` の `//` を行コメントと誤認する形・レビューが実測）。
//
// 受容する残余:
// - **`#[cfg]` は無視して和を取る。** ゆえに保証は「どれかの cfg で宣言されている」であって
//   「ビルド構成でコンパイルされる」ではない。**決して有効化されない cfg / feature の下だけで
//   宣言されたファイルは緑になる**——上で名指しした最悪の帰結（テストが黙って走らない）を、
//   この検査は取りこぼす。cfg を評価しないのは誤検出を避けるための選択である。
// - `include!` によるファイル取り込みを追わない（現在 0 件）。当該ファイルが赤に倒れる向き。
// - **属性の綴りが `#[path = "..."]`（二重引用符・同一行）から外れると赤に倒れる**——
//   `#[path = r"..."]`（raw string）・`#[path = "dir"]`（ディレクトリ指定）・
//   `#[cfg_attr(..., path = "...")]`・`#[cfg(windows)] mod win;` のような同一行の属性つき宣言。
//   現在 0 件で、いずれも沈黙ではなく過検出の向きである。
// - `[lib] path = ...` で `src/` の外へソースを置く crate は母集団から外れる（現在 0 件）。

/** ある `.rs` が宣言する子モジュールの探索基準ディレクトリ。
 *  `lib.rs` / `main.rs` / `mod.rs` は自分のディレクトリ、それ以外は自分の stem のディレクトリを持つ。 */
function moduleChildDir(file) {
  const slash = file.lastIndexOf("/");
  const dir = slash < 0 ? "" : file.slice(0, slash);
  const base = file.slice(slash + 1);
  if (base === "lib.rs" || base === "main.rs" || base === "mod.rs") return dir;
  return `${dir}/${base.slice(0, -3)}`;
}

/**
 * Rust ソースの**コードでない部分**（コメント・文字列・char リテラル）を空白へ潰す。
 * 長さと改行位置を保つので、潰した文字列に対する行頭アンカーと、元テキスト上のオフセットが両立する。
 *
 * **1 パスの字句スキャナである。** コメントの除去と文字列の判定を別々のパスでやると、
 * 一方の数え違いがもう一方の判定を**反転**させる（2026-08-14 のレビューが実測: `"http://x"` の
 * `//` を行コメントと誤認して閉じ引用符を食べ、以降の文字列の内側が「外側」に見えた）。
 * 状態を 1 つ持てば、文字列の中の `//` もコメントの中の `"` も原理的に取り違えない。
 *
 * 扱う字句: 入れ子ブロックコメント / `//` 行コメント / 文字列（`\` エスケープ・行継続を含む）/
 * raw string（`r"…"`・`r#"…"#`・`#` は任意個）/ byte 版（`b"…"`・`br#"…"#`・`b'…'`）/
 * char リテラル。**char とライフタイムは綴りで区別する**——`'x'` / `'\n'` / `'"'` は char、
 * `'a` は閉じないのでライフタイムとして素通しする。
 */
function blankRustNonCode(text) {
  const n = text.length;
  let out = "";
  let i = 0;
  const blank = (to) => {
    for (; i < to; i++) out += text[i] === "\n" ? "\n" : " ";
  };
  while (i < n) {
    const two = text.slice(i, i + 2);
    if (two === "/*") {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        const t = text.slice(j, j + 2);
        if (t === "/*") depth++, (j += 2);
        else if (t === "*/") depth--, (j += 2);
        else j++;
      }
      blank(j);
      continue;
    }
    if (two === "//") {
      let j = i;
      while (j < n && text[j] !== "\n") j++;
      blank(j);
      continue;
    }
    const raw = /^b?r(#*)"/.exec(text.slice(i, i + 24));
    if (raw) {
      const close = `"${"#".repeat(raw[1].length)}`;
      const end = text.indexOf(close, i + raw[0].length);
      blank(end < 0 ? n : end + close.length);
      continue;
    }
    const quote = text[i] === '"' ? i : text[i] === "b" && text[i + 1] === '"' ? i + 1 : -1;
    if (quote >= 0) {
      let j = quote + 1;
      while (j < n) {
        if (text[j] === "\\") j += 2;
        else if (text[j] === '"') {
          j++;
          break;
        } else j++;
      }
      blank(j);
      continue;
    }
    const tick = text[i] === "'" ? i : text[i] === "b" && text[i + 1] === "'" ? i + 1 : -1;
    if (tick >= 0) {
      const ch = /^'(?:\\.|[^'\\\n])'/.exec(text.slice(tick, tick + 12));
      if (ch) {
        blank(tick + ch[0].length);
        continue;
      }
    }
    out += text[i];
    i++;
  }
  return out;
}

/** `file` が宣言する子モジュールの候補ファイルパスを返す（カナリアが実ファイルで検算するため export）。 */
export function declaredModuleFiles(file, raw) {
  // BOM は落とす（残すと 1 行目の宣言が列 0 から外れて拾えない）。以降は同じ文字列を基準にする。
  const source = raw.charCodeAt(0) === 0xfeff ? raw.slice(1) : raw;
  const text = blankRustNonCode(source);
  const slash = file.lastIndexOf("/");
  const ownDir = slash < 0 ? "" : file.slice(0, slash);
  const childDir = moduleChildDir(file);
  const out = [];
  const viaPath = new Set();
  // `#[path = "..."] mod n;` — **`#[path]` は「そのソースファイルが在るディレクトリ」からの相対**である
  // （実例: snotra-egui-runtime/src/ime.rs の `#[path = "windows_ime.rs"]` が src/windows_ime.rs を指す）。
  // 間に他の属性（`#[cfg(...)]` 等）が挟まる形も拾う。
  // **元テキストで照合し、その位置がコードであることを潰した側で確かめる。** 属性の中の文字列は
  // 解決に要るので潰せないが、raw string の中に綴られた `#[path]` は拾ってはならない（実測で沈黙した）。
  for (const m of source.matchAll(
    /#\[path\s*=\s*"([^"]+)"\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z_0-9]*)\s*;/g,
  )) {
    if (text[m.index] !== "#") continue; // コメント・文字列の内側
    // `..` は畳んで解決する（Rust の意味論と一致）。根を越えた形だけが残り、母集団に一致せず赤に倒れる。
    out.push(path.posix.normalize(`${ownDir}/${m[1]}`));
    viaPath.add(m[2]);
  }
  // 通常の `mod name;` — **行頭ちょうど**に限る（空白を許すとインライン `mod` の中身を拾い、
  // 基準ディレクトリを外したまま同名の兄弟ファイルへ一致して孤児を緑にする）。
  for (const m of text.matchAll(/^(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z_0-9]*)\s*;/gm)) {
    if (viaPath.has(m[1])) continue; // `#[path]` 付きは上で解決済み
    out.push(`${childDir}/${m[1]}.rs`, `${childDir}/${m[1]}/mod.rs`);
  }
  return out;
}

export function checkModuleLinkage(snapshot) {
  const { members, error } = workspaceMembers(snapshot);
  if (error) return [finding("Cargo.toml", 1, `${error}（G-module-linkage 母集団の欠落）`)];

  const findings = [];
  for (const crate of members) {
    const prefix = `${crate}/src/`;
    // `src/bin/` は cargo が target として自動発見するので `mod` 宣言を要さない（射程外）。
    const population = snapshot.files.filter(
      (f) => f.startsWith(prefix) && f.endsWith(".rs") && !f.startsWith(`${prefix}bin/`),
    );
    // 空母集団を合格に見せない（沈黙経路の閉塞・本ファイル冒頭の契約）
    if (population.length === 0) {
      findings.push(finding(`${crate}/Cargo.toml`, 1, `${prefix} 配下に .rs が無い（G-module-linkage 母集団の欠落）`));
      continue;
    }
    const present = new Set(population);
    const roots = [`${prefix}lib.rs`, `${prefix}main.rs`].filter((f) => present.has(f));
    // ルートが無ければ探索が始まらず、全ファイルが未到達になる。**その形を「全部赤」ではなく
    // 母集団の欠落として 1 件で報告する**——原因（ルート不在）を名指ししないと直し方が伝わらない。
    if (roots.length === 0) {
      findings.push(finding(`${prefix}lib.rs`, 1, `crate ルート（lib.rs / main.rs）が無い（G-module-linkage 母集団の欠落）`));
      continue;
    }

    const seen = new Set();
    const queue = [...roots];
    while (queue.length > 0) {
      const f = queue.shift();
      if (seen.has(f)) continue;
      seen.add(f);
      const text = snapshot.read(f);
      if (text == null) continue;
      for (const cand of declaredModuleFiles(f, text)) {
        if (present.has(cand) && !seen.has(cand)) queue.push(cand);
      }
    }

    for (const f of population) {
      if (!seen.has(f)) {
        findings.push(
          finding(f, 1, `crate ルートから mod 宣言で到達できない（mod 宣言の書き忘れ。cargo も rust-analyzer も報せない）`),
        );
      }
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-architecture-table — docs/architecture.md にファイル単位モジュール表が再導入されていないか（旧 Check 2）
// ---------------------------------------------------------------------------
export function checkArchitectureTable(snapshot) {
  const findings = [];
  const p = "docs/architecture.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/architecture.md が読めない")];
  for (const [lineNo, line] of linesOutsideFences(text)) {
    if (/^\|\s*`[^`]+\.(rs|ts|tsx|mts|mjs)`\s*\|/.test(line)) {
      findings.push(finding(p, lineNo, `ファイル単位のモジュール表行が再導入されている: ${line.trim().slice(0, 60)}（責務の正本は //! / TSDoc・#562）`));
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-references — ガバナンス文書群の参照実在（Markdown リンク + バッククォート内パス様参照）。
// バッククォート参照の検査述語（受容する偽陰性はスクリプトコメントとテストで固定）:
//   `/` を含む・glob（* ? {）なし・<> なし・% なし・URL なし・拡張子が REF_EXTENSIONS・
//   workspace/ 配下でない・`\` を含まない。
//   → ベア名（`SPEC.md` 等）とランタイム生成物（`config.toml`・`*.bin`・`*.bak`）は構造的に対象外。
// ---------------------------------------------------------------------------
export function checkReferences(snapshot, docs) {
  const findings = [];
  const fileSet = new Set(snapshot.files);
  const exists = (doc, ref, { allowSuffix = false } = {}) => {
    const norm = (p) => path.posix.normalize(p);
    if (fileSet.has(norm(ref))) return true; // リポジトリルート基準
    const rel = norm(path.posix.join(path.posix.dirname(doc), ref)); // 文書ディレクトリ基準
    if (fileSet.has(rel)) return true;
    // crate 内相対参照（`lib/types.ts` = ui/src/lib/types.ts、`commands/launch.rs` =
    // src-tauri/src/commands/launch.rs 等）はサフィックス一致で解決する（意図的な近似）。
    // バッククォート参照（`/` 必須の述語 = 2 セグメント以上）に限る——Markdown リンクへ
    // 適用すると、壊れた相対リンクが同 basename の別ファイルで偽陰性になる
    if (!allowSuffix) return false;
    const suffix = `/${norm(ref)}`;
    return !suffix.includes("..") && snapshot.files.some((f) => f.endsWith(suffix));
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-references 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text)) {
      // (i) Markdown リンク
      for (const m of line.matchAll(/\[[^\]]*\]\(([^()\s]+)\)/g)) {
        let target = m[1];
        if (/^[a-z]+:/.test(target)) continue; // https: / mailto: 等
        target = target.split("#")[0];
        if (!target) continue; // 純アンカー
        if (!exists(doc, target)) {
          findings.push(finding(doc, lineNo, `Markdown リンク先が実在しない: ${m[1]}`));
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
          findings.push(finding(doc, lineNo, `バッククォート参照のパスが実在しない: ${t}`));
        }
      }
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-spec-sections — SPEC.md 番号連続性 + SPEC 前置の §N(.x) 参照の実在（旧 Check 4 + #587 新規）。
// 裸の `§N` は各文書自身の節参照でありうるため対象外（不混入はテストで固定）。
// ---------------------------------------------------------------------------
export function checkSpecSections(snapshot, docs) {
  const findings = [];
  const spec = snapshot.read("SPEC.md");
  if (spec == null) return [finding("SPEC.md", 1, "SPEC.md が読めない")];
  const sections = new Set();
  let prevTop = null;
  let prevSub = null;
  for (const [lineNo, line] of linesOutsideFences(spec)) {
    const top = line.match(/^## (\d+)\. /);
    if (top) {
      const n = Number(top[1]);
      if (prevTop != null && n !== prevTop + 1) {
        findings.push(finding("SPEC.md", lineNo, `セクション番号が連続しない: ## ${prevTop}. の次が ## ${n}.`));
      }
      prevTop = n;
      prevSub = 0;
      sections.add(`${n}`);
      continue;
    }
    const sub = line.match(/^### (\d+)\.(\d+) /);
    if (sub) {
      const [n, x] = [Number(sub[1]), Number(sub[2])];
      if (n !== prevTop) {
        findings.push(finding("SPEC.md", lineNo, `子セクション ### ${n}.${x} が親 ## ${prevTop}. と不一致`));
      } else if (x !== prevSub + 1) {
        findings.push(finding("SPEC.md", lineNo, `子セクション番号が連続しない: ${n}.${prevSub} の次が ${n}.${x}`));
      }
      prevSub = x;
      sections.add(`${n}.${x}`);
    }
  }
  if (sections.size === 0) findings.push(finding("SPEC.md", 1, "セクション見出し（## N.）が 1 件も無い（G-spec-sections 母集団の欠落）"));
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 母集団欠落は G-references が報告する
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(/SPEC(?:\.md)?`?(?: の)? ?§(\d+(?:\.\d+)?)/g)) {
        if (!sections.has(m[1])) {
          findings.push(finding(doc, lineNo, `SPEC §${m[1]} が SPEC.md に実在しない`));
        }
      }
    }
  }
  return findings;
}

/** TOML の 1 行から**引用符の外の** `#` 以降を落とす。**引用符を見ない実装にしてはならない**——
 *  `src-tauri/clippy.toml` の reason は `（#751）` を含み、素朴な `replace(/#.*$/, "")` は行を途中で切る
 *  （#950 で実測。切れた先に `path` が在れば禁止集合が丸ごと消えたように見える）。 */
function stripTomlComment(raw) {
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
const tomlLine = (raw) => stripTomlComment(raw).trim();

/** Cargo の lints テーブルの値から level を取る。文字列形（`= "deny"`）とテーブル形
 *  （`= { level = "deny", priority = 1 }`）の 2 形を受ける。**rustdoc と clippy の 2 検査が共有する**——
 *  cargo が 3 つ目の表記を足したとき、直す場所が 1 か所であるために切り出してある（#950）。 */
const lintLevel = (value) => (value.startsWith("{") ? (value.match(/level\s*=\s*"([^"]+)"/)?.[1] ?? null) : (value.match(/^"([^"]+)"$/)?.[1] ?? null));

/** TOML の整数リテラル。**数値区切りの `_` を落とす**——落とさないと `1_0`（TOML では 10）から 1 だけを
 *  読み、群の allow が実際より小さい priority に見えて緑へ倒れる（#950 のレビューで実測）。 */
const tomlInt = (text) => Number((String(text).match(/-?[0-9_]+/)?.[0] ?? "0").replaceAll("_", ""));

/** 同じく priority。文字列形は既定の 0。**priority が大きいほど後に当たる**ので、群の allow が個別 lint の
 *  deny と同じか大きい priority を持つと禁止が消える（#950 で実測）。 */
const lintPriority = (value) => (value.startsWith("{") ? tomlInt(value.match(/priority\s*=\s*([^,}]+)/)?.[1] ?? "0") : 0);

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
 *  （`docs/development-principles.md`「6. 検出は構造化された信号で行い…」）。 */
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

// ---------------------------------------------------------------------------
// G-clippy-disallowed — src-tauri/clippy.toml の禁止集合が実効しているか（#950）。
//
// **守る命題**: この検査が緑 ⇒ `Context` 経由の global style 書き込み（#751 / #900）が src-tauri の clippy で
// error として落ちる。**前提は 4 つあり、どれも緑が含意しない**——(1) clippy.toml と Cargo.toml を正規表現で
// 近似パースする範囲で、(2) member 側の opt-in（`[lints] workspace = true`）は G-workspace-lints が見る、
// (3) 名指しした各パスが解決し続ける（解決しなくなっても文字列は変わらないので沈黙する。
//     群 1 は上流 egui のピン更新、群 2 は snotra-core 側の改名が契機になる）、
// (4) DISALLOWED_METHODS_GROUPS が上流の群構成に追随している。**単独の緑を「禁止は生きている」と読んではならない。**
//
// 塞ぐのは **clippy 自身が exit 0 で沈黙する** 次の経路である（clippy 1.94.0 で実測）:
//   内容側 — ファイルの削除 / disallowed-methods の消滅・空配列化 / エントリが 1 行だけ消える /
//            メソッド名・型名の書き損じ（`does not refer to a reachable function` の warning は出るが
//            `-D warnings` でも exit 0）/ crate 名の書き損じ・egui 依存の消滅（診断そのものが出ない）/
//            エントリが `#` でコメントアウトされる
//   レベル側 — ルート [workspace.lints.clippy] の disallowed_methods の消滅・warn への降格・**同じ節の
//            群 allow による打ち消し**（`all = "allow"` を 1 行足すと deny の行を残したまま禁止が消える。
//            clippy 1.94.0 で実測: exit 0・診断 0 件）。この lint は **warn 既定**ゆえ、どの形でも黙る
// **PostToolUse hook は exit code でしか検出しないため、上記の warning はエージェントにも届かない。**
// 沈黙は二重である——それがこの検査を冗長でなくしている性質である（cargo のキャッシュを一切介さない
// Node の静的読み取りなので、6 経路すべてが入力テキストの差分として現れる）。
//
// 射程外（意図的）: reason 文言の変更・`#[allow]` による迂回（lint に内在する性質）・
// disallowed_methods 以外の clippy lint のレベル・clippy.toml が cargo の fingerprint に入らないこと
// （`.rs` を触らず同じコマンドを打つとキャッシュ replay で exit 0。正本は clippy.toml 冒頭のコメント）。
//
// 受容する残余:
// - member 側の opt-in（src-tauri の `[lints] workspace = true`）は **G-workspace-lints が全 member について
//   見る**ため重ねない。**deny が実効するのはその opt-in が在る間だけ**であり、両検査は組で 1 つの命題を守る。
// - ルート Cargo.toml が読めない事実は G-workspace-lints でも鳴る（1 事実 2 件）。**沈黙させない側へ倒す**
//   ——黙って skip すれば、それ自体が新しい沈黙経路になる。
// - disallowed_methods を**ハイフン**（`disallowed-methods`）で書いた形は非実効と判定する＝**赤に倒れる**。
//   向きが赤（沈黙しない）なので受容するが、**次の人の最も安い直し方が「検査を緩める」にならない**よう
//   直し方を書いておく: Cargo の lints テーブルは lint 名をそのまま書くのでアンダースコアにする
//   （ハイフンなのは clippy.toml 側のキーだけである）。
// ---------------------------------------------------------------------------

/** src-tauri/clippy.toml に在ることを要求する禁止メソッド。**名指しは意図的である**——「配列が非空」だけでは
 *  1 行だけ消えた形も、メソッド名を書き損じた形も緑を通る（どちらも clippy 側は exit 0・実測）。
 *  **含めなかったメソッドと、その除外理由の正本は src-tauri/clippy.toml 冒頭のコメントである**——
 *  ここは「消えたら困る識別子」の写しだけを持つ（先例は REQUIRED_RUSTDOC_LINTS）。 */
export const REQUIRED_DISALLOWED_METHODS = [
  "egui::Context::set_visuals",
  "egui::Context::set_visuals_of",
  "egui::Context::style_mut_of",
  "egui::Context::set_style_of",
  "egui::Context::global_style_mut",
  "egui::Context::set_global_style",
  "egui::Context::all_styles_mut",
  // 群 2（#1067）: 計測ハーネス専用の観測口。製品が読んで分岐してはならない。
  "snotra_core::engine::Engine::sorted_by_path",
];

const CLIPPY_TOML = "src-tauri/clippy.toml";
const SRC_TAURI_MANIFEST = "src-tauri/Cargo.toml";

/** disallowed-methods 配列の path 値を**全件**返す。配列そのものが無ければ `null`（「空」と区別する）。
 *  **全域 match である**——per-line の単発 match は 1 行形（インラインテーブルを並べた配列）で先頭 1 件しか
 *  拾わない。**コメント除去を先に通す**——通さないと `#` でコメントアウトされたエントリを「在る」と数え、
 *  `disallowed-methods = []` との組み合わせが緑を通る（実測。あのファイルはコメントで長く説明する様式ゆえ、
 *  一時的な無効化はこの形で起きるのが最も自然である）。
 *  配列の終端は最初の `]` とする。reason に `]` を書くと途中で切れてカナリアが欠け**赤に倒れる**。 */
export function disallowedMethodPaths(text) {
  const body = text.split("\n").map(stripTomlComment).join("\n");
  const array = body.match(/disallowed-methods\s*=\s*\[([\s\S]*?)\]/);
  if (array == null) return null;
  return [...array[1].matchAll(/path\s*=\s*"([^"]+)"/g)].map((m) => m[1]);
}

/** src-tauri が egui を**通常の依存として**宣言しているか。**字面ではなく構文的位置で判定する**——
 *  `snotra-egui-runtime = { path = "../snotra-egui-runtime" }` が部分文字列で誤爆するためで、
 *  hasWorkspaceLintsOptIn と同じ理由である。実際の宣言形は dotted 形（`egui.workspace = true`）。
 *
 *  **節は `[dependencies]` と `[target.<cfg>.dependencies]` に限る。** `dependencies]` で終わる節を広く
 *  受けると 3 つが紛れ込み、どれも実害を持つ: `[dev-dependencies]` だけに egui が在る形は **bin/lib で
 *  パスが解決しない**のに緑になり（clippy は診断そのものを出さない）、`[build-dependencies]` も同じ。
 *  ルートの `[workspace.dependencies]` は egui を宣言しているので、**checkClippyDisallowed が 3 つの
 *  同型な読み取り（clippy.toml / src-tauri の Cargo.toml / ルート Cargo.toml）を取り違えても緑を通す**。
 *  節を絞ることで、その取り違えは赤として現れる（#950 の対称性検査で発見）。
 *
 *  ルート直下の dotted 形（`dependencies.egui = …`）は cargo 上有効だが非実効と判定する＝**赤に倒れる**。
 *  向きが赤なので受容する。直し方: `[dependencies]` テーブルで書く。
 *  **`[target.<cfg>.dependencies]` を受けるのは非対称な残余である**——cfg がビルド対象で偽なら egui は依存に
 *  入らず、禁止パスは解決せず clippy は無診断で沈黙する。現構成では到達不能（実データの egui は素の
 *  `[dependencies]`・CI は Windows ジョブ）ゆえ受容する。 */
export function declaresEguiDependency(text) {
  let section = "";
  for (const raw of text.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      section = line;
      continue;
    }
    if (!/^\[(?:target\.[^\]]+\.)?dependencies\]$/.test(section)) continue;
    if (/^egui\s*=/.test(line) || /^egui\.[A-Za-z0-9_-]+\s*=/.test(line)) return true;
  }
  return false;
}

/** disallowed_methods を含む lint group。**名指しは意図的である**——群を allow にする兄弟が 1 行在るだけで、
 *  `disallowed_methods = "deny"` はそのままに禁止が黙って消える（clippy 1.94.0 で実測: exit 0・診断 0 件）。
 *  この 2 つは `clippy-driver -W help` の群一覧から disallowed-methods を含むものを数え上げた結果である
 *  ——**上流が 3 つ目の群へ入れたら、この配列が更新されるまで沈黙する**（受容する残余）。 */
const DISALLOWED_METHODS_GROUPS = ["all", "style"];

/** ルート [workspace.lints.clippy] の disallowed_methods が deny/forbid で、**かつ後から allow で
 *  打ち消されていない**か。level と priority の 2 形は lintLevel / lintPriority が受ける
 *  （前者は rustdocLintsAreDenied と共有）。
 *
 *  **「deny の行が在る」だけでは足りない**——同じ節の `all = "allow"` 1 行で禁止は完全に消える（実測）。
 *  **エントリは 3 つの綴りで書ける**（インライン形・dotted 形・サブテーブル形）。TOML 上は等価で
 *  clippy の挙動も同じなので、**3 つとも読む**——1 つでも落とすとその綴りだけが緑を通る（実測）。
 *  priority で向きが決まり、群が**同じか大きい** priority を持つときだけ打ち消す（`priority = -1` で
 *  群を先に当てる形は禁止が生き残ることを実測したので、緑に倒す）。**`>=` は `all` で測った境界に
 *  合わせた保守的な規則である**——同 priority の `style` は実測では打ち消さないが、ここでは赤に倒れる
 *  （fail-closed。直し方は `priority = -1` で群を先に当てること）。
 *  隣の rustdocLintsAreDenied が「節内の全エントリが deny」という ∀ で同型の穴を塞いでいるのに対し、
 *  こちらは節に allow を書く正当な用途を残すため、**打ち消しうる群だけを名指しして**塞ぐ。
 *  **節が無い形が最も起きやすい欠落である**（2 行消すだけで起きる）。 */
export function clippyMethodsDenied(rootText) {
  const entries = new Map();
  const upsert = (key, patch) => entries.set(key, { level: null, priority: 0, ...entries.get(key), ...patch });
  let section = "";
  for (const raw of rootText.split("\n")) {
    const line = tomlLine(raw);
    if (/^\[.*\]$/.test(line)) {
      section = line;
      continue;
    }
    if (line === "") continue;
    if (section === "[workspace.lints.clippy]") {
      // インライン形（`all = "allow"` / `all = { level = "allow", priority = 1 }`）
      const flat = line.match(/^([A-Za-z0-9_-]+)\s*=\s*(.+)$/);
      if (flat != null) {
        const value = flat[2].trim();
        upsert(flat[1], { level: lintLevel(value), priority: lintPriority(value) });
        continue;
      }
      // dotted 形（`all.level = "allow"`）。**この形を落とすと fail-open になる**——群が Map に現れず
      // 「打ち消し無し」と読んで緑を返す一方、clippy 側は禁止が消えて exit 0 になる（実測）
      const dotted = line.match(/^([A-Za-z0-9_-]+)\.(level|priority)\s*=\s*(.+)$/);
      if (dotted != null) {
        upsert(dotted[1], dotted[2] === "level" ? { level: lintLevel(dotted[3].trim()) } : { priority: tomlInt(dotted[3]) });
      }
      continue;
    }
    // サブテーブル形（`[workspace.lints.clippy.all]` の下に level / priority）。dotted 形と同じ理由で要る
    const sub = section.match(/^\[workspace\.lints\.clippy\.([A-Za-z0-9_-]+)\]$/);
    if (sub == null) continue;
    const kv = line.match(/^(level|priority)\s*=\s*(.+)$/);
    if (kv != null) {
      upsert(sub[1], kv[1] === "level" ? { level: lintLevel(kv[2].trim()) } : { priority: tomlInt(kv[2]) });
    }
  }
  const target = entries.get("disallowed_methods");
  if (target == null || (target.level !== "deny" && target.level !== "forbid")) return false;
  for (const group of DISALLOWED_METHODS_GROUPS) {
    const e = entries.get(group);
    if (e != null && e.level === "allow" && e.priority >= target.priority) return false;
  }
  return true;
}

/** evidence 用の件数。**読めない・配列が無い形は 0 とする**——素直に書くと
 *  「clippy 禁止 undefined 件」になり、この検査が存在する当の失敗ケースで evidence が壊れる。 */
export function clippyDisallowedCount(snapshot) {
  return disallowedMethodPaths(snapshot.read(CLIPPY_TOML) ?? "")?.length ?? 0;
}

export function checkClippyDisallowed(snapshot) {
  const findings = [];
  const toml = snapshot.read(CLIPPY_TOML);
  if (toml == null) {
    findings.push(finding(CLIPPY_TOML, 1, "禁止設定が読めない（G-clippy-disallowed 母集団の欠落）——消しても clippy は沈黙して exit 0 を返す（#950）"));
  } else {
    const paths = disallowedMethodPaths(toml);
    if (paths == null) {
      findings.push(finding(CLIPPY_TOML, 1, "disallowed-methods の配列が無い（#751 の禁止が丸ごと消えている・#950）"));
    } else {
      const missing = REQUIRED_DISALLOWED_METHODS.filter((p) => !paths.includes(p));
      if (missing.length > 0) {
        findings.push(
          finding(CLIPPY_TOML, 1, `disallowed-methods に ${missing.join(" / ")} が無い（行の消失・書き損じ・コメントアウトのいずれでも clippy は exit 0 で沈黙する・#950）`),
        );
      }
    }
  }
  const manifest = snapshot.read(SRC_TAURI_MANIFEST);
  if (manifest == null) {
    findings.push(finding(SRC_TAURI_MANIFEST, 1, "src-tauri の Cargo.toml が読めない（G-clippy-disallowed 母集団の欠落）"));
  } else if (!declaresEguiDependency(manifest)) {
    findings.push(finding(SRC_TAURI_MANIFEST, 1, "egui を依存に宣言していない（禁止パスが解決する前提が消え、clippy は診断そのものを出さなくなる・#950）"));
  }
  const root = snapshot.read("Cargo.toml");
  if (root == null) {
    findings.push(finding("Cargo.toml", 1, "ルート Cargo.toml が読めない（G-clippy-disallowed 母集団の欠落）"));
  } else if (!clippyMethodsDenied(root)) {
    findings.push(
      finding("Cargo.toml", 1, "[workspace.lints.clippy] の disallowed_methods が deny/forbid で無い（warn 既定へ戻り、禁止が -D warnings 依存の助言へ黙って降格する・#950）"),
    );
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-build-commands — docs/build-commands.md の npm script / cargo test -p crate の実在（旧 Check 5 の決定的部分）。
// crate 名はディレクトリ名でなく各 member Cargo.toml の [package] name（`-p snotra` = src-tauri/）。
// check/clippy は --workspace で cargo 自身が SSOT を読むため照合対象外（#500）。
// ---------------------------------------------------------------------------
export function checkBuildCommands(snapshot) {
  const findings = [];
  const p = "docs/build-commands.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/build-commands.md が読めない")];
  let scripts = {};
  try {
    scripts = JSON.parse(snapshot.read("package.json") ?? "{}").scripts ?? {};
  } catch {
    findings.push(finding("package.json", 1, "package.json がパースできない"));
  }
  const crateNames = new Set();
  // 母集団は workspaceMembers に一本化する（#713）。error はここでは報告しない——同じ欠落を
  // 2 検査で 2 件にしないため。members が空なら crateNames も空になり、`cargo test -p` の行が
  // すべて赤くなるので、この検査は従来どおり fail-closed のままである。
  for (const dir of workspaceMembers(snapshot).members) {
    const name = (snapshot.read(`${dir}/Cargo.toml`) ?? "").match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    if (name) crateNames.add(name);
  }
  text.split("\n").forEach((line, i) => {
    for (const m of line.matchAll(/npm run ([A-Za-z0-9:_-]+)/g)) {
      if (!(m[1] in scripts)) findings.push(finding(p, i + 1, `npm script が package.json に無い: ${m[1]}`));
    }
    if (/(?:^|[^a-z])npm test(?:$|[^a-z])/.test(line) && !("test" in scripts)) {
      findings.push(finding(p, i + 1, "npm test に対応する scripts.test が無い"));
    }
    for (const m of line.matchAll(/cargo test -p ([A-Za-z0-9_-]+)/g)) {
      if (!crateNames.has(m[1])) findings.push(finding(p, i + 1, `cargo test -p の crate が workspace に無い: ${m[1]}`));
    }
  });
  return findings;
}

// ---------------------------------------------------------------------------
// G-ci-table — 「CI/CD メモ」対応表 ↔ .github/workflows/*.yml（旧 Check 10 の機械部分）。
// wrapper 等価: `npm run X` が verbatim に無くても、scripts[X] の値のパス様トークンが
// workflow に現れれば「実行あり」（CI は引数を上書きしてスクリプトを直接呼ぶことがある）。
// ---------------------------------------------------------------------------
export function checkCiTable(snapshot) {
  const findings = [];
  const p = "docs/build-commands.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/build-commands.md が読めない")];
  let scripts = {};
  try {
    scripts = JSON.parse(snapshot.read("package.json") ?? "{}").scripts ?? {};
  } catch {
    /* G-build-commands が報告する */
  }
  const lines = text.split("\n");
  const start = lines.findIndex((l) => /^\| 検証コマンド \| workflow \|/.test(l));
  if (start === -1) return [finding(p, 1, "「CI/CD メモ」対応表（| 検証コマンド | workflow |）が見つからない")];
  // 終了条件は「最初の空行」である。`startsWith("|")` で打ち切ると、表の途中に非表の行が 1 行
  // 紛れただけで**以降の行が照合されないまま緑になる**（#863 で発見した 2 つ目の沈黙経路。
  // 崩れた行の黙殺が 1 つ目）。走査の終わりは表の終わりであって、最初の異常ではない。
  for (let i = start + 2; i < lines.length && lines[i].trim() !== ""; i++) {
    if (!lines[i].startsWith("|")) {
      findings.push(finding(p, i + 1, `対応表の途中に表でない行がある（以降が照合されない経路）: ${lines[i]}`));
      continue;
    }
    // 必要なのは先頭 2 列 = split で 4 要素以上（実表は トリガー 列を持つ 3 列 = 5 要素）
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) {
      findings.push(finding(p, i + 1, `対応表の行に検証コマンド列と workflow 列がそろっていない: ${lines[i]}`));
      continue;
    }
    const wf = cols[2].match(/`?([A-Za-z0-9_-]+\.yml)`?/)?.[1];
    if (!wf) {
      findings.push(finding(p, i + 1, `workflow 列から .yml ファイル名を特定できない: ${cols[2]}`));
      continue;
    }
    const wfText = snapshot.read(`.github/workflows/${wf}`);
    if (wfText == null) {
      findings.push(finding(p, i + 1, `workflow ファイルが実在しない: ${wf}`));
      continue;
    }
    for (const m of cols[1].matchAll(/`((?:npm|cargo)[^`]*)`/g)) {
      const cmd = m[1];
      if (wfText.includes(cmd)) continue;
      const scriptName = cmd.match(/^npm run ([A-Za-z0-9:_-]+)/)?.[1];
      const wrapperPaths = (scripts[scriptName] ?? "")
        .split(/\s+/)
        .filter((t) => t.includes("/") || /\.(ps1|mjs|ts)$/.test(t));
      if (wrapperPaths.some((t) => wfText.includes(t))) continue;
      // npm ライフサイクル経由（1 段）: workflow が `npm run Y` を実行し、Y または preY が
      // `npm run X` を呼ぶなら X も実行される（例: build の prebuild が typecheck を呼ぶ）
      const viaLifecycle =
        scriptName &&
        Object.keys(scripts).some(
          (y) =>
            wfText.includes(`npm run ${y}`) &&
            (scripts[y]?.includes(`npm run ${scriptName}`) || scripts[`pre${y}`]?.includes(`npm run ${scriptName}`)),
        );
      if (viaLifecycle) continue;
      findings.push(finding(p, i + 1, `表のコマンド \`${cmd}\` が ${wf} のどの run にも現れない（wrapper パスも不一致）`));
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-rules-globs — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチ（旧 Check 8）。
// documented 意味論（bare 名 = ルート直下のみ・`**` = 階層横断・{a,b} ブレース）の自前変換。
// harness の配送判定の再現ではなく「マッチ 0 件の検知」に限定した近似。
// ---------------------------------------------------------------------------
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

export function checkRulesGlobs(snapshot) {
  const findings = [];
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (rules.length === 0) return [finding(".claude/rules", 1, "rules ファイルが 0 件（G-rules-globs 母集団の欠落）")];
  for (const rule of rules) {
    const text = snapshot.read(rule) ?? "";
    const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] ?? ""; // CRLF checkout 耐性
    const patterns = [...fm.matchAll(/^\s*-\s*"([^"]+)"/gm)].map((m) => m[1]);
    if (patterns.length === 0) {
      findings.push(finding(rule, 1, "frontmatter に paths パターンが 1 件も無い"));
      continue;
    }
    for (const pat of patterns) {
      const re = globToRegex(pat);
      if (!snapshot.files.some((f) => re.test(f))) {
        findings.push(finding(rule, 1, `paths glob が実在ファイルに 1 件もマッチしない: ${pat}`));
      }
    }
  }
  return findings;
}

const SKILL_FILE_RE = /^\.claude\/skills\/[^/]+\/SKILL\.md$/;

/** `.claude/skills/<name>/SKILL.md` の一覧（G-skill-table・G-area-instrument の共通母集団） */
function skillFiles(snapshot) {
  return snapshot.files.filter((f) => SKILL_FILE_RE.test(f));
}

/**
 * `disable-model-invocation: true` の skill 名の集合 = **harness の roster に注入されない skill**。
 * G-skill-table（表が索引すべき対象）と G-area-instrument（常時ロード面に載る description）の両方がこの集合で決まるため、
 * 導出は 1 箇所に閉じる。判定は frontmatter ブロックの中だけを見る——本文が同じキー名に言及する
 * 実例がある（`/retrospective` が `/health-check` の起動方法を説明している）。
 *
 * **判定不能はすべて「隠しでない」へ倒す。** 同じ状態へ入る経路は 4 本ある——値が `true` でない /
 * キーが無い / frontmatter が壊れている・無い / ファイルが読めない。この向きなら、判定できなかった
 * skill は「roster が覆う側」に残り、表に行が無くても緑になる（静かな方の失敗）。逆へ倒すと、
 * 存在しない表の行を要求して赤になる。
 */
export function modelHiddenSkills(snapshot) {
  const hidden = new Set();
  for (const f of skillFiles(snapshot)) {
    const text = snapshot.read(f);
    if (text == null) continue;
    const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/);
    if (!fm) continue;
    if (/^disable-model-invocation:[ \t]*true[ \t]*$/m.test(fm[1])) hidden.add(f.split("/")[2]);
  }
  return hidden;
}

// ---------------------------------------------------------------------------
// G-skill-table — ルート CLAUDE.md「利用できるスキル」表 ↔ roster に載らない skill（旧 Check 9）
// harness は毎セッション skill roster を description 付きで注入するため、注入される skill を
// 表へ書き写すことは同じ面での二重課税である（ADR-area-metric-characters が description を常時ロード面に算入した
// のと同じ理由）。ゆえに表が索引すべき対象は `disable-model-invocation: true` の skill だけであり、
// G-skill-table はその集合と表の**双方向**一致を見る。「表の射程」を規範ではなくこの判定で固定する。
// ---------------------------------------------------------------------------
export function checkSkillTable(snapshot) {
  const findings = [];
  const text = snapshot.read("CLAUDE.md");
  if (text == null) return [finding("CLAUDE.md", 1, "ルート CLAUDE.md が読めない")];
  const section = text.split(/^## 利用できるスキル$/m)[1]?.split(/^## /m)[0] ?? "";
  const inTable = new Set([...section.matchAll(/^\|\s*`\/([a-z0-9-]+)`/gm)].map((m) => m[1]));
  const inDirs = new Set(skillFiles(snapshot).map((f) => f.split("/")[2]));
  const hidden = modelHiddenSkills(snapshot);
  if (inDirs.size === 0) findings.push(finding(".claude/skills", 1, "SKILL.md が 0 件（G-skill-table 母集団の欠落）"));
  for (const s of inTable) {
    if (!inDirs.has(s)) findings.push(finding("CLAUDE.md", 1, `スキル表の /${s} に SKILL.md が無い（.claude/skills/${s}/）`));
    else if (!hidden.has(s)) {
      findings.push(
        finding("CLAUDE.md", 1, `スキル表の /${s} は harness の roster に載る（表の対象は disable-model-invocation: true の skill だけ）`),
      );
    }
  }
  for (const s of hidden) {
    if (!inTable.has(s)) findings.push(finding("CLAUDE.md", 1, `.claude/skills/${s}/ は roster に載らないのにスキル表に無い`));
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-hook-commands — PostToolUse hook の cargo コマンド ↔ docs/build-commands.md カテゴリ A の照合（#589）。
// hook は触らない（非 export・import は main 実行の副作用があるため、ソーステキストから
// `cargoSpec([...])` を抽出する。抽出アンカーが hook のリファクタで腐ったら抽出 0 件 fail で
// 明示的に失敗する）。出力整形のみのフラグ（exit code を変えないもの）は arity 付き除去リストで
// 落としてから照合する（build-commands.md の既存整合規約の機械化）。
// nodeSpec / vitest 系（npm SSOT の部分集合ラッパー）は意味判断を要するため対象外＝
// /health-check の Check 5 残置部分が受け持つ（受容する範囲）。
// ---------------------------------------------------------------------------
const OUTPUT_ONLY_FLAGS = { "--message-format": 1 }; // フラグ名 → 後続引数の個数

export function checkHookCommands(snapshot) {
  const findings = [];
  const hookPath = ".claude/hooks/post-edit.mjs";
  const hookSrc = snapshot.read(hookPath);
  if (hookSrc == null) return [finding(hookPath, 1, "post-edit.mjs が読めない（G-hook-commands 母集団の欠落）")];
  // cargoSpec([...]) の引数配列を抽出（clippy は複数行折返しのため dotall 必須）
  const hookCommands = [...hookSrc.matchAll(/cargoSpec\(\[([\s\S]*?)\]\)/g)].map((m) =>
    [...m[1].matchAll(/"([^"]*)"/g)].map((t) => t[1]),
  );
  if (hookCommands.length === 0) {
    return [finding(hookPath, 1, "cargoSpec([...]) が 1 件も抽出できない（G-hook-commands 母集団の欠落。抽出アンカーの腐敗か buildCommand のリファクタ）")];
  }
  const docsPath = "docs/build-commands.md";
  const docsText = snapshot.read(docsPath);
  if (docsText == null) return [finding(docsPath, 1, "docs/build-commands.md が読めない（G-hook-commands）")];
  // カテゴリ A 節の bash フェンス内 cargo 行を母集団にする（行末 # コメントを除去）
  const sectionA = docsText.split(/^### A\. /m)[1]?.split(/^### /m)[0] ?? "";
  // 行分割は \r?\n — CRLF checkout（Windows CI・autocrlf=true）では `.` が \r に
  // マッチしないため、\r を残すと行末コメント除去 `#.*$` が発火しない（PR #595 で実測）
  const docsLines = sectionA
    .split(/\r?\n/)
    .filter((l) => l.trim().startsWith("cargo "))
    .map((l) => l.replace(/\s+#.*$/, "").trim().split(/\s+/).join(" "));
  if (docsLines.length === 0) {
    return [finding(docsPath, 1, "カテゴリ A の cargo コマンド行が 0 件（G-hook-commands 母集団の欠落）")];
  }
  for (const args of hookCommands) {
    // 出力整形フラグを arity 込みで除去し、"cargo" を前置してトークン列を正規化
    const normalized = ["cargo"];
    for (let i = 0; i < args.length; i++) {
      if (args[i] in OUTPUT_ONLY_FLAGS) {
        i += OUTPUT_ONLY_FLAGS[args[i]];
        continue;
      }
      normalized.push(args[i]);
    }
    const cmd = normalized.join(" ");
    if (!docsLines.includes(cmd)) {
      findings.push(
        finding(hookPath, 1, `hook の cargo コマンドが docs/build-commands.md カテゴリ A に無い（フラグ乖離の疑い）: ${cmd}`),
      );
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-hook-fires — PostToolUse hook の発火割り当て ↔ docs/hooks.md「PostToolUse（post-edit.mjs）の
// 発火一覧」の照合（#863）。
//
// 表は selectChecks の写しであり、同型のドリフトは一度起きている——ルート CLAUDE.md の同じ表が腐り、
// その退去先が docs/hooks.md である（#474〜#497）。#858 で fmt を足したとき、この行は実際に計画の
// 変更ファイル一覧から漏れた。
//
// **G-hook-commands と形が違うのは、可視性が違うからである。** あちらが hook のソーステキストから
// `cargoSpec([...])` を抽出するのは cargoSpec が**非 export** だからであって、import が危険だから
// ではない（post-edit.mjs は I13 のガードで import 安全であり、pre-bash.test.mjs が既に import している）。
// selectChecks は export 済みなので、**判定を再実装せず関数そのものを呼ぶ** — 抽出で近似すると、
// 閉じたい写しを一段下で作り直すことになる。テストのため既定値付き引数で注入する
// （checkModuleIndex / checkConfigFieldReachability と同形）。
//
// 照合は 2 方向:
//   (1) 行ごと — select(代表パス) と「走る検査 id」列が**順序込みで**一致する。順序は表自身の主張
//       （fmt を先頭に置く理由が本文に書いてある）ゆえ、集合ではなく配列で比較する。
//   (2) 母集団 — ソースの `checks.push("<id>")` リテラル全件が表のどこかに現れる。逆向き（表にあって
//       発行されない id）は (1) から導かれるので、こちらは順方向だけでよい。
//
// 母集団を BUDGETS から取らない: BUDGETS ⊇ 発行されうる id であり、どのパスも発火させない id が
// BUDGETS に入ったとき、表に「決して走らない検査」を書かせることになる。
//
// **代表パスの実在は自前で見る。** G-references に委ねる案は却下した——あちらの述語は「バッククォート内で
// `/` を含み REF_EXTENSIONS に当たる」であり、実測すると現行 10 行のうち `Cargo.toml`（`/` 無し）と
// `.githooks/pre-commit`（拡張子無し）の 2 行が述語の外にある。代償として実在しない入力（4 crate 外の
// `.rs`・`config.toml`）は行にできない＝受容する残余は ADR-hook-fires-table-check が名指しする。
// ---------------------------------------------------------------------------
const HOOK_FIRES_HEADER = /^\|\s*編集したファイル（代表パス）\s*\|\s*走る検査 id\s*\|/;

export function checkHookFires(snapshot, select = selectChecks) {
  const findings = [];
  const docsPath = "docs/hooks.md";
  const hookPath = ".claude/hooks/post-edit.mjs";
  const text = snapshot.read(docsPath);
  if (text == null) return [finding(docsPath, 1, "docs/hooks.md が読めない（G-hook-fires 母集団の欠落）")];
  const hookSrc = snapshot.read(hookPath);
  if (hookSrc == null) return [finding(hookPath, 1, "post-edit.mjs が読めない（G-hook-fires 母集団の欠落）")];
  // 行分割は \r?\n — CRLF checkout では列末に \r が残り、id の一致がすべて外れる（#587/#589 で二度踏んでいる）
  const lines = text.split(/\r?\n/);
  const headers = lines.map((l, i) => (HOOK_FIRES_HEADER.test(l) ? i : -1)).filter((i) => i >= 0);
  if (headers.length === 0) {
    return [
      finding(docsPath, 1, "発火一覧のヘッダ行（| 編集したファイル（代表パス） | 走る検査 id | …）が見つからない（G-hook-fires 母集団の欠落）"),
    ];
  }
  // 例示（コードフェンス内の書き方の見本など）でヘッダが 2 本になると、findIndex は先に現れた方を
  // 掴み、本物の表が照合されないまま緑になる。母集団の曖昧化は明示的な赤にする
  if (headers.length > 1) {
    return [finding(docsPath, headers[1] + 1, `発火一覧のヘッダ行が ${headers.length} 本ある（どれが本物か決まらない・G-hook-fires 母集団の曖昧化）`)];
  }
  const start = headers[0];
  const fileSet = new Set(snapshot.files);
  const mentioned = new Set();
  let rows = 0;
  let sawEmptyRow = false;
  // 終了条件は「最初の空行」である（checkCiTable と同じ理由）——`startsWith("|")` で打ち切ると、
  // 表の途中に非表の行が 1 行紛れただけで**以降の行が照合されないまま緑になる**（#863 の code-review が実測）
  for (let i = start + 2; i < lines.length && lines[i].trim() !== ""; i++) {
    if (!lines[i].startsWith("|")) {
      findings.push(finding(docsPath, i + 1, `発火一覧の途中に表でない行がある（以降が照合されない経路）: ${lines[i]}`));
      continue;
    }
    // 必要なのは先頭 2 列 = split で 4 要素以上（実表は補足列を持つ 3 列 = 5 要素）
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) {
      findings.push(finding(docsPath, i + 1, `発火一覧の行に代表パス列と検査 id 列がそろっていない: ${lines[i]}`));
      continue;
    }
    // 代表パス列は「バッククォート括りの単一パス」だけを許す。glob や複数併記を通すと
    // select() へ何を食わせたかが曖昧になり、緑の意味が薄まる
    const rel = cols[1].match(/^`([^`]+)`$/)?.[1];
    if (!rel) {
      findings.push(finding(docsPath, i + 1, `代表パス列がバッククォート括りの単一パスでない: ${cols[1]}`));
      continue;
    }
    rows += 1;
    // 実在は自前で見る。G-references に委ねると、その述語（`/` を含み REF_EXTENSIONS に当たる）から
    // 外れる代表パス（`Cargo.toml`・`.githooks/pre-commit`）が素通りする——実測で 10 行中 2 行。
    // 死んだパスは `selectChecks` の接頭辞判定を通り続けるので、表だけが静かに嘘になる
    if (!fileSet.has(rel)) {
      findings.push(finding(docsPath, i + 1, `代表パスが実在しない（死んだ行は判定を通り続ける）: ${rel}`));
    }
    // 検査 id 列のバッククォートは検査 id だけ（散文は補足列へ置く、が書式規約）
    const declared = [...cols[2].matchAll(/`([^`]+)`/g)].map((m) => m[1]);
    if (declared.length === 0) {
      // 空集合は「（なし）」と綴らせる。散文・空白・空バッククォートを空集合と読むと、
      // 書き崩しが「検査は走らない」という主張に化ける
      if (cols[2] === "（なし）") sawEmptyRow = true;
      else findings.push(finding(docsPath, i + 1, `検査 id 列にバッククォートが無い。空集合は「（なし）」と書く: ${cols[2]}`));
    }
    for (const id of declared) mentioned.add(id);
    const actual = select(rel);
    if (declared.join(" ") !== actual.join(" ")) {
      findings.push(
        finding(
          docsPath,
          i + 1,
          `発火一覧が selectChecks と一致しない: \`${rel}\` は [${actual.join(", ")}] を発火するが表は [${declared.join(", ")}]`,
        ),
      );
    }
  }
  if (rows === 0) {
    findings.push(finding(docsPath, start + 1, "発火一覧の行が 0 件（G-hook-fires 母集団の欠落）"));
    return findings;
  }
  // 行の削除は、id を持つ行なら下の母集団照合が拾う。**唯一拾えないのが空集合の行**であり、
  // それは「割り当ての無いファイルの沈黙は合格ではない」という最上位の契約を運ぶ行である（#471/#497）。
  // 消しても緑になる経路をここで閉じる
  if (!sawEmptyRow) {
    findings.push(finding(docsPath, start + 1, "検査が 1 つも走らないパスの行が無い——「沈黙は合格ではない」という主張が表から消えている"));
  }
  const emitted = new Set([...hookSrc.matchAll(/checks\.push\("([^"]+)"\)/g)].map((m) => m[1]));
  if (emitted.size === 0) {
    findings.push(
      finding(hookPath, 1, 'checks.push("<id>") が 1 件も抽出できない（G-hook-fires 母集団の欠落。抽出アンカーの腐敗か selectChecks のリファクタ）'),
    );
    return findings;
  }
  for (const id of emitted) {
    if (!mentioned.has(id)) {
      findings.push(finding(docsPath, start + 1, `selectChecks が発行しうる検査 id が発火一覧のどの行にも現れない: ${id}`));
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-area-instrument — 恒久規範の面積の計器（合否を持たない）。ADR-retire-area-budget。
// **上限判定は廃止した。** 一次規範は「書く約束」（`.claude/rules/governance-docs.md`: かぶりなく・
// 必要なことだけ・古い情報を残さない）であり、数字はその代役になれない。ratchet 期の 3 発火は
// すべて正当な追記に対するもので（#894 の実績調査）、続く火災報知器期は 8 日で両面が +30% 育つ間
// 一度も鳴らなかった——通算 4 観測点で欠陥検出はゼロである。残るのは実測値の報告だけで、
// 推移は `governance:check` の成功行と git 履歴が運ぶ。
//
// **合否を持たない道具でも、母集団の欠落だけは判定する。** 読めない入力の上で出した数字は
// 静かに誤り、判定が無いぶん誰も気づかない（`check:colors` がロック画面を撮る形と同型）。
// ゆえにこの検査が残すのは「計器が入力を読めているか」だけである。
//
// 指標は**文字数（コードポイント・CR 除く）**——行数は「改行を消す」で読む量を減らさず数字だけ
// 下がる（ADR-area-metric-characters に実測）。CR を除くのは CRLF checkout 対策（#587/#589）。
// 常時ロード面には skill の description を含める（毎セッション注入される面）。skills 本文・
// モジュール CLAUDE.md・docs・ADR は対象外——「その作業に入った者だけが読む面」への退去は
// #593 が推奨する経路であり、課税すれば登ってほしい階梯を登る側が罰せられる。
// 二面（常時ロード / rules）を分けて報告するのは、面替えによる片面の肥大が合計では見えないため。
// ---------------------------------------------------------------------------

/** 常時ロードされる恒久規範ファイル（ルート直下の 2 文書。ほかに skill description が同じ面に載る）。
 *  **保証は狭い**——常時ロード面にファイルが増えてもここへ足さなければ、その面積は報告に
 *  一度も算入されない（2026-08-09 実測: 5000 字の文書を新設して `CLAUDE.md` から `@` で読み込ませても、
 *  計上が動いたのは `CLAUDE.md` 側の 1 行分だけ・#1008）。足し忘れを知るのはファイルシステムであって
 *  この検査ではない。 */
export const ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"];

/** コードポイント数（CR は除く）。読めなければ null（母集団欠落を上位で検知） */
function countChars(text) {
  return text == null ? null : [...text.replace(/\r/g, "")].length;
}

/** 指定ファイル群の総文字数。読めないファイルは finding に積み、面積へは算入しない */
function sumChars(snapshot, files, gLabel) {
  let total = 0;
  const findings = [];
  for (const f of files) {
    const c = countChars(snapshot.read(f));
    if (c == null) findings.push(finding(f, 1, `${f} が読めない（${gLabel} 母集団の欠落）`));
    else total += c;
  }
  return { total, findings };
}

/**
 * 毎セッション注入される skill の `description` の総文字数。
 * 複数行スカラー（`|` / `>`）と欠落は数えられないので finding に倒す（沈黙経路の閉塞）。
 * **`disable-model-invocation: true` の skill は除く** — ADR-area-metric-characters が description を常時ロード面へ
 * 算入した根拠は「毎セッション注入されるのに ratchet から見えていない」であり、roster に載らない
 * skill はその前提を満たさない（載らないものを数えれば、実際には注入されていない字に課税する）。
 * `count` は母集団の存在確認用なので、除外前の全 skill 数を返す。
 */
export function skillDescriptionArea(snapshot) {
  const all = skillFiles(snapshot);
  const hidden = modelHiddenSkills(snapshot);
  const files = all.filter((f) => !hidden.has(f.split("/")[2]));
  let total = 0;
  const findings = [];
  for (const f of files) {
    const text = snapshot.read(f);
    if (text == null) {
      findings.push(finding(f, 1, `${f} が読めない（G-area-instrument 母集団の欠落）`));
      continue;
    }
    const m = text.match(/^description:[ \t]*(.*)$/m);
    const v = m ? m[1].trim() : "";
    if (!m || v === "" || v.startsWith("|") || v.startsWith(">")) {
      findings.push(finding(f, 1, "description が 1 行スカラーでない（G-area-instrument が面積を数えられない）"));
      continue;
    }
    total += [...v.replace(/^["']/, "").replace(/["']$/, "")].length;
  }
  return { total, findings, count: all.length };
}

/**
 * 計器の母集団だけを見る（面積の大小は判定しない・ADR-retire-area-budget）。
 * 返す finding はすべて「入力が読めない／空」であり、面積が大きいことは finding にならない。
 */
export function checkNormativeAreaInstrument(snapshot) {
  const findings = [];

  const docs = sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument");
  const desc = skillDescriptionArea(snapshot);
  findings.push(...docs.findings, ...desc.findings);
  if (desc.count === 0) findings.push(finding(".claude/skills", 1, "skills が 0 件（G-area-instrument 母集団の欠落）"));

  const ruleFiles = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (ruleFiles.length === 0) {
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G-area-instrument 母集団の欠落）"));
  } else {
    findings.push(...sumChars(snapshot, ruleFiles, "G-area-instrument").findings);
  }
  return findings;
}

/** evidence 用の実測（検査と同じ母集団・同じ数え方であることを型で担保するための共有関数） */
export function normativeArea(snapshot) {
  const always =
    (sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument").total ?? 0) + skillDescriptionArea(snapshot).total;
  const rules = sumChars(
    snapshot,
    snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)),
    "G-area-instrument",
  ).total;
  return { always, rules };
}

// ---------------------------------------------------------------------------
// G-heading-refs — 見出し参照の実在（正準形 `<対象>`「<見出し>」）。
// 参照に構文を与えて機械照合可能にする。これが `.claude/rules/governance-docs.md` の
// 「改変前に参照側を名前と序数で数え上げる」手作業を置き換える機構である。
// アンカーは ATX 見出し・番号付きリスト項目・太字リードの 3 種（この repo の参照実態に合わせた。
// 節ではなく箇条書きのリード文を指す参照が実在する: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。
// 照合は正規化（`**`・バッククォート・「」・空白の除去）後の**前方一致**——見出しが後置の
// 括弧注記（「…の不変条件（#532 SU5）」「条件別チェック（トリガー → 参照先）」）を持つため。
// 受容する偽陰性: 「ルート `CLAUDE.md` のフック節」のような散文形の参照は見ない。
// 検査されるのは正準形に書かれたものだけであり、**この検査は規範の完全な代替ではない**。
// ---------------------------------------------------------------------------

/** 見出し参照の正準形。対象は `<path>.md` か `/skill-name`。
 *  `§` には節番号を伴ってよい（`SPEC.md` §11「見た目の規範」）——番号を許さないと、
 *  節番号つきの参照は正準形へ直しても照合されず、G-near-heading-refs が「直せない指摘」を出し続ける（#727 で実測）。 */
const HEADING_REF = /`([^`\n]+)`\s*(?:§\s*[\d.]*\s*)?「([^「」\n]+)」/g;

/** 参照先になりうる位置（ATX 見出し / 番号付きリスト項目 / 太字リード） */
export function collectAnchors(text) {
  const out = [];
  for (const m of text.matchAll(/^#{1,6}\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*\d+[.)]\s+(.+?)\s*$/gm)) out.push(m[1]);
  for (const m of text.matchAll(/^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*/gm)) out.push(m[1]);
  return out;
}

const normAnchor = (s) => s.replace(/[`*「」\s]/g, "");

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

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497） */
export function scanHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const anchorCache = new Map();
  const anchorsOf = (p) => {
    if (!anchorCache.has(p)) {
      const t = snapshot.read(p);
      anchorCache.set(p, t == null ? null : collectAnchors(t).map(normAnchor));
    }
    return anchorCache.get(p);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-heading-refs 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(HEADING_REF)) {
        const [, target, label] = m;
        if (!target.endsWith(".md") && !/^\/[a-z0-9-]+$/.test(target)) continue;
        checked += 1;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が解決できない: \`${target}\`「${label}」`));
          continue;
        }
        const anchors = anchorsOf(p);
        if (anchors == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が読めない: ${p}`));
          continue;
        }
        if (!anchors.some((a) => a.startsWith(normAnchor(label)))) {
          findings.push(
            finding(doc, lineNo, `見出し参照が着地しない: \`${target}\`「${label}」（${p} に該当する見出し・リード文が無い）`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkHeadingRefs(snapshot, docs) {
  return scanHeadingRefs(snapshot, docs).findings;
}

// ---------------------------------------------------------------------------
// G-near-heading-refs — 正準形に見えて隣接していない見出し参照（#727）。
//
// G-heading-refs が見るのはバッククォートを閉じた直後（`§` と空白のみを挟む）に `「` が続く形だけで、
// **助詞が 1 つ挟まると検査対象から外れる**。人の目には同じ参照に見える:
//   `/start-issue`「Step 6 — …」      ← G-heading-refs が見る
//   `/start-issue` は「Step 6 — …」   ← 見ない
// #725 では Claude 自身が書いた 3 件がこの形で、しかも `/implement` の入口判定の中核推論を
// 支えていた（`/start-issue` が改番されれば黙って壊れる）。
//
// **判定の要は窓幅ではなく「引用が実際に着地するか」である。** 近傍に `「…」` があるだけでは
// 参照と散文の引用（「`SPEC.md`（…）は「何を実現すべきか」を記す」）を分けられない。実測:
//
// | 窓幅 | 着地する（＝正準形へ直せる真の参照） | 着地しない（＝散文の引用） |
// |---|---|---|
// | 2 | 5 | 8 |
// | 4 | 7 | 12 |
// | 8 | **8** | 28 |
// | 12 | 8 | 34 |
//
// 着地条件を課すと、窓を広げても真の参照は 8 件で頭打ちになり、増えるのは無視する側だけである。
// ゆえに**窓幅 8・着地必須**とした。この形なら誤爆の代償は「散文の引用がたまたま見出しと同名」
// に限られ、そのときは正準形へ直すのが正しい（G-heading-refs の保護下へ入る）。
//
// **受容する残余**: 着地しない非隣接参照は見ない。腐った参照（消滅した節を指す散文形）は
// この検査では捕まらない——歴史記述と区別できないためである（`.claude/rules/governance-docs.md`
// 「既に消滅した節の名前を正準形で書かない」が規範として担う）。
// ---------------------------------------------------------------------------

/** 閉じバッククォートから `「` までに挟まってよい最大文字数（実測で頭打ちになる値） */
const NEAR_REF_GAP = 8;
/** 非隣接の近傍参照。gap は最短一致で取る */
const NEAR_REF = new RegExp("`([^`\\n]+)`([^`\\n]{1," + NEAR_REF_GAP + "}?)「([^「」\\n]+)」", "g");
/** G-heading-refs が既に見ている隣接形（`§` + 節番号と空白のみを挟む）。HEADING_REF と同じ前提を持つ */
const ADJACENT_REF = /`[^`\n]+`\s*(?:§\s*[\d.]*\s*)?「/;

export function scanNearHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const anchorCache = new Map();
  const anchorsOf = (p) => {
    if (!anchorCache.has(p)) {
      const t = snapshot.read(p);
      anchorCache.set(p, t == null ? null : collectAnchors(t).map(normAnchor));
    }
    return anchorCache.get(p);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 読めない文書は G-heading-refs が母集団の欠落として報告済み
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(NEAR_REF)) {
        const [, target, gap, label] = m;
        if (!target.endsWith(".md") && !/^\/[a-z0-9-]+$/.test(target)) continue;
        if (ADJACENT_REF.test(m[0])) continue;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) continue;
        const anchors = anchorsOf(p);
        if (anchors == null) continue;
        checked += 1;
        if (anchors.some((a) => a.startsWith(normAnchor(label)))) {
          // 節番号は正準形が許すので、直し方の提示から落とさない
          const section = (gap.match(/§\s*[\d.]+/) ?? [""])[0];
          const canonical = `\`${target}\`${section ? ` ${section}` : ""}「${label}」`;
          findings.push(
            finding(doc, lineNo, `見出し参照が正準形でない（G-heading-refs の視界外）: \`${target}\`【${gap}】「${label}」— ${canonical}と書く`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkNearHeadingRefs(snapshot, docs) {
  return scanNearHeadingRefs(snapshot, docs).findings;
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

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

/** Rust のコメントを落とす。落とさないと `preset` のような普通の英単語が doc コメントに埋もれる（実測）。
 *  文字列リテラル内の `//` 以降も落ちるが、向きは赤側（読みが消える）ゆえ沈黙しない */
function stripRustComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/\/\/.*$/gm, " ");
}

/** `#[cfg(test)]` 以降を落とす。**母集団と読み手の両方に適用する**——読み手側で落とさないと
 *  「テストだけが読む」フィールドが読まれている側へ落ちる（`visible_rows` で実測） */
function productionOnly(src) {
  return src.split(/^#\[cfg\(test\)\]/m)[0];
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
//   `productionOnly` を通しても落ちるのは 1 つ目だけである。現時点でこの穴に落ちた finding は
//   1 件も無く（測定の全セルで 0 件）、測って動かなかったものを入れないだけの理由で開けてある
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
/** 語彙源ではなく検査対象になる、`.claude/**` の外の**固定パス**文書
 *  （意図の SSOT・常時ロードの規範・設定 UI のデザイン規約）。
 *  **静的リテラルであること自体が fail-closed である**——読めなければ `scanStaleIdentifiers` が
 *  「母集団の欠落」を出すので、グロブ由来の母集団（`staleIdentifierGuideDocs`）と違って
 *  `runAll` 側の 0 件検知を別に置く必要がない。
 *  **保証は狭い**——「意図の SSOT」級の文書を新設してここへ足さなければ、その文書の腐り識別子は
 *  一度も照合されない（2026-08-09 実測: ルート直下に新設した文書へ実在しない識別子を 3 形置いても
 *  照合件数が動かなかった・#1008）。 */
export const STALE_EXTRA_DOCS = ["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"];
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
    for (const [lineNo, line] of linesOutsideFences(text)) {
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

// ---------------------------------------------------------------------------
// G-check-skill-enumeration — `/implement`「4a. check スキルの実行」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
//
// `/implement`「出力」のレビュー表は報告の母集団を 4a の列挙で閉じており、それは表の**写し**である。
// 乖離すると、表に増えた check スキルが報告母集団から**沈黙して落ちる**。
//
// **問題は義務が行為者の視界の外にあることだった。** 同期義務は 4a の括弧書きに書いてあるが、
// それを実行するのは `AGENTS.md` の表を編集する人であり、その人が `/implement`「出力」を読む
// 必然性は無い。#765 が塞いだ「実施の有無が報告から消える」より一段手前で、報告を読んでも気づけない。
//
// **述語は着手前に現行コーパスへ当てて実測した**（#778 が明示的に要求している。表は rules 参照・
// grep 指示を含む混成表で、`/plan-review` のような非 check スキルも現れるため）:
// - 表の `/…-check` = {cache, dry, persistence, race, state, symmetric}（6 件）
// - 4a の `/…-check` = 同じ 6 件
// `/plan-review` は `-check` で終わらないため**構造的に外れる**。`/health-check` は
// 表に現れない（ルート `CLAUDE.md` のスキル表に在り、そちらは G-skill-table が見る）。
//
// これで #778 の (a)（表側へ同期義務を 1 行置く）が不要になった——`AGENTS.md` は G-area-instrument の常時ロード面で
// 余裕が小さいため、機構で吸収できるならそちらが安い。
// ---------------------------------------------------------------------------

const CHECK_SKILL_REF = /\/[a-z][a-z0-9-]*-check\b/g;
/** 節を見出しで切り出す（次の同レベル以上の見出しまで）。見つからなければ null */
function sectionOf(text, headingRe) {
  const lines = text.split("\n");
  const start = lines.findIndex((l) => headingRe.test(l));
  if (start < 0) return null;
  const level = (lines[start].match(/^#+/) ?? ["#"])[0].length;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => /^#+\s/.test(l) && (l.match(/^#+/) ?? [""])[0].length <= level);
  return rest.slice(0, end < 0 ? rest.length : end).join("\n");
}

export function checkCheckSkillEnumeration(snapshot) {
  const findings = [];
  const agents = snapshot.read("AGENTS.md");
  const impl = snapshot.read(".claude/skills/implement/SKILL.md");
  if (agents == null || impl == null) {
    return [finding("AGENTS.md", 1, "G-check-skill-enumeration の母集団が読めない（AGENTS.md か /implement の SKILL.md）")];
  }
  const table = sectionOf(agents, /^##\s+条件別チェック/);
  const step4a = sectionOf(impl, /^###\s+4a\./);
  if (table == null) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 「条件別チェック」節が見つからない（見出しが変わった）"));
  if (step4a == null) findings.push(finding(".claude/skills/implement/SKILL.md", 1, "G-check-skill-enumeration: 「4a.」節が見つからない（見出しが変わった）"));
  if (findings.length > 0) return findings;

  const setOf = (t) => new Set((t.match(CHECK_SKILL_REF) ?? []).map((s) => s.trim()));
  const inTable = setOf(table);
  const in4a = setOf(step4a);
  // 空母集団は明示 fail（沈黙経路の閉塞）
  if (inTable.size === 0) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 表に check スキルが 0 件（母集団の欠落）"));
  if (in4a.size === 0) findings.push(finding(".claude/skills/implement/SKILL.md", 1, "G-check-skill-enumeration: 4a に check スキルが 0 件（母集団の欠落）"));

  for (const s of inTable) {
    if (!in4a.has(s)) {
      findings.push(
        finding(".claude/skills/implement/SKILL.md", 1, `G-check-skill-enumeration: \`${s}\` が AGENTS.md の表に在るが 4a の列挙に無い（報告母集団から沈黙して落ちる）`),
      );
    }
  }
  for (const s of in4a) {
    if (!inTable.has(s)) {
      findings.push(finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` が 4a の列挙に在るが AGENTS.md の表に無い（起動条件を持たない検査）`));
    }
  }
  // 列挙されたスキルが実在するか（誤記の検出）
  for (const s of new Set([...inTable, ...in4a])) {
    const p = `.claude/skills/${s.slice(1)}/SKILL.md`;
    if (!snapshot.files.includes(p)) findings.push(finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` に対応する ${p} が実在しない`));
  }
  return findings;
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

// ---------------------------------------------------------------------------
// G-adr-citations — `ADR-<slug>` の短縮引用が実在の ADR を指すか（#812 の A）。
//
// **連番だった頃、この検査は書けなかった。** `ADR-0007` はファイル名の一部でしかなく、
// 引用文字列とファイル名 stem が別物だったためである（`0007-results-presentation-two-stage.md`）。
// `ADR-<slug>.md` へ移して stem = 引用文字列にしたことで、初めて機械照合できるようになった
// ——`docs/adr/ADR-canonical-heading-references.md` が見出し参照に正準形を与えて照合可能にしたのと同じ手。
//
// **母集団はコードコメントを含む。** 製品コードの 5 箇所（`view.rs` ほか）が ADR を短縮名で呼んでおり、
// そこは今日まで検出器を 1 つも持っていなかった。
// **テストファイル（`*.test.mjs`）は母集団外である**——フィクスチャは赤経路を測るために
// 意図的に実在しない名前を持つ（実測: 本検査の初回実行で 5 件すべてが自分のフィクスチャだった）。
// md のコードフェンスを見ないのと同じ理由で、構造的に外す。
// **受容する残余**: `docs/superpowers/` は歴史資料（#589 で非規範化）ゆえ母集団外である。
// 旧番号のパスが残るが、その時点の事実の記録であり、書き換えると当時を偽ることになる。
// ---------------------------------------------------------------------------

/** 短縮引用の形。`ADR-` + kebab slug */
const ADR_CITATION = /\bADR-([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\b/g;

/** G-adr-citations の母集団: ガバナンス文書 + skills + 製品ソース（コメントに引用が在る） */
export function adrCitationDocs(snapshot, docs) {
  return [
    ...docs,
    // 凍結された歴史も**実在の辺だけ**は守る——ADR → ADR の短縮引用は、指す側が凍結でも
    // 指される側の削除で壊れる。`docs`（governanceDocs）は docs/adr/ を含まないため明示的に足す。
    // この 1 行が落ちると ADR→ADR の実在検査が沈黙で消える（母集団カナリアがテストで膜を張る）
    ...snapshot.files.filter((f) => /^docs\/adr\/[^/]+\.md$/.test(f)),
    ...snapshot.files.filter((f) => /^\.claude\/skills\/.*\.md$/.test(f)),
    // 非 docs のソース。**見るのは直下の正規表現が挙げる拡張子だけである**——`.ts` / `.tsx` /
    // `.ps1` / `.psm1` に書いた ADR の短縮引用は実在照合を素通りする（2026-08-09 実測・#1008）。
    ...snapshot.files.filter((f) => /\.(rs|mjs)$/.test(f) && !f.startsWith("docs/") && !f.endsWith(".test.mjs")),
  ];
}

export function scanAdrCitations(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const exists = (slug) => snapshot.files.includes(`docs/adr/ADR-${slug}.md`);
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue;
    const isMd = doc.endsWith(".md");
    const lines = isMd ? linesOutsideFences(text) : text.split("\n").map((l, i) => [i + 1, l]);
    for (const [lineNo, line] of lines) {
      for (const m of line.matchAll(ADR_CITATION)) {
        checked += 1;
        if (!exists(m[1])) {
          findings.push(finding(doc, lineNo, `ADR の短縮引用が実在しない: \`${m[0]}\`（docs/adr/ADR-${m[1]}.md が無い）`));
        }
      }
    }
  }
  return { findings, checked };
}

export function checkAdrCitations(snapshot, docs) {
  return scanAdrCitations(snapshot, docs).findings;
}

/** 検査の登録表。**ここが検査 ID の SSOT である**——サマリ行の件数もこの配列から計算するので、
 *  「G1..G15 passed」のような範囲を手で書く面が存在しない（範囲は黙って腐る。実例が
 *  `docs/build-commands.md` に「G1〜G12」と残っていた・#812）。
 *  ID は `G-<name>` 形で連番を持たない——連番は「いま空いている最大値 + 1」をマージの瞬間に
 *  確定させるため、並行する 2 本の PR が同じ値を見る（`.claude/rules/governance-docs.md`
 *  「序数で他を指してはならない」）。 */
export function buildChecks(snapshot, sink = {}) {
  const docs = governanceDocs(snapshot);
  const refDocs = headingRefDocs(snapshot);
  const refSourceDocs = headingRefSourceDocs(snapshot);
  // 2 つの腕は検査へ渡すときだけ束ねる。母集団としては別々に持つ——`runAll` の 0 件検知が
  // 腕ごとに 1 本ずつ要るためである（束ねた長さは片方の消滅を隠す）
  const allRefDocs = [...refDocs, ...refSourceDocs];
  const staleDocs = staleIdentifierDocs(snapshot);
  const staleGuides = staleIdentifierGuideDocs(snapshot);
  const staleTargets = staleIdentifierTargets(snapshot);
  sink.docs = docs;
  sink.refDocs = refDocs;
  sink.refSourceDocs = refSourceDocs;
  sink.staleDocs = staleDocs;
  sink.staleGuides = staleGuides;
  sink.staleTargets = staleTargets;
  const record = (key, r) => {
    sink[key] = r.checked;
    return r.findings;
  };
  return [
    { id: "G-module-index", run: () => checkModuleIndex(snapshot) },
    { id: "G-module-linkage", run: () => checkModuleLinkage(snapshot) },
    { id: "G-architecture-table", run: () => checkArchitectureTable(snapshot) },
    { id: "G-references", run: () => checkReferences(snapshot, docs) },
    { id: "G-spec-sections", run: () => checkSpecSections(snapshot, docs) },
    { id: "G-build-commands", run: () => checkBuildCommands(snapshot) },
    { id: "G-workspace-lints", run: () => checkWorkspaceLints(snapshot) },
    { id: "G-clippy-disallowed", run: () => checkClippyDisallowed(snapshot) },
    { id: "G-ci-table", run: () => checkCiTable(snapshot) },
    { id: "G-rules-globs", run: () => checkRulesGlobs(snapshot) },
    { id: "G-skill-table", run: () => checkSkillTable(snapshot) },
    { id: "G-hook-commands", run: () => checkHookCommands(snapshot) },
    { id: "G-hook-fires", run: () => checkHookFires(snapshot) },
    { id: "G-area-instrument", run: () => checkNormativeAreaInstrument(snapshot) },
    { id: "G-check-skill-enumeration", run: () => checkCheckSkillEnumeration(snapshot) },
    { id: "G-adr-file-names", run: () => checkAdrFileNames(snapshot) },
    { id: "G-adr-citations", run: () => record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, docs))) },
    { id: "G-heading-refs", run: () => record("headingRefs", scanHeadingRefs(snapshot, allRefDocs)) },
    { id: "G-stale-identifiers", run: () => record("stale", scanStaleIdentifiers(snapshot, staleTargets)) },
    { id: "G-near-heading-refs", run: () => record("nearRefs", scanNearHeadingRefs(snapshot, allRefDocs)) },
  ];
}

export function runAll(snapshot) {
  const ctx = {};
  const checks = buildChecks(snapshot, ctx);
  const findings = [];
  if (ctx.docs.length === 0) findings.push(finding(".", 1, "ガバナンス文書が 0 件（母集団の欠落）"));
  if (ctx.refDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象 md が 0 件（母集団の欠落）"));
  // 腕ごとに 1 本ずつ要る（`staleDocs` / `staleGuides` と同型）——束ねると md 側の長さが
  // `.rs` の消滅を埋め、Rust コメントの見出し参照が誰にも見られないまま緑になる
  if (ctx.refSourceDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象ソース（.rs）が 0 件（母集団の欠落）"));
  // `staleTargets` ではなく `staleDocs` を見る——`STALE_EXTRA_DOCS` が常に長さを埋めるため、
  // targets 側で判定すると `.claude/**` が 1 枚残らず消えてもこの検知が沈黙する。
  // **グロブ由来の母集団ごとに 1 本ずつ要る**——束ねると片方が埋めた長さで他方の消滅が隠れる。
  // 固定パスの `STALE_EXTRA_DOCS` はここに要らない（読めなければ scanStaleIdentifiers が鳴る）
  if (ctx.staleDocs.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の対象 md が 0 件（母集団の欠落）"));
  if (ctx.staleGuides.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の開発ガイド（docs/**）が 0 件（母集団の欠落）"));
  for (const c of checks) findings.push(...c.run());
  const area = normativeArea(snapshot);
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)).length;
  const skills = snapshot.files.filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f)).length;
  const evidence = `検査 ${checks.length} 件 / 対象文書 ${ctx.docs.length} 件 / rules ${rules} 件 / skills ${skills} 件 / 恒久規範 常時ロード ${area.always} 字・rules ${area.rules} 字 / 見出し参照 ${ctx.headingRefs} 件を md ${ctx.refDocs.length} 件 + .rs ${ctx.refSourceDocs.length} 件から照合 / workspace member ${workspaceMembers(snapshot).members.length} 件の lints opt-in / clippy 禁止 ${clippyDisallowedCount(snapshot)} 件 / 散文の識別子 ${ctx.stale} 件を ${ctx.staleTargets.length} 文書から照合 / 近傍の見出し参照 ${ctx.nearRefs} 件 / ADR ${adrFiles(snapshot).length} 本の名前 / ADR の短縮引用 ${ctx.adrCitations} 件`;
  return { findings, evidence };
}

// fileURLToPath を使う — URL.pathname は空白等を percent-encode するため resolve と一致せず、
// 「検査ゼロ件のまま exit 0」という沈黙経路になる（レビュー H1 で実測）
const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const { findings, evidence } = runAll(makeSnapshot(process.cwd()));
  if (findings.length > 0) {
    console.error(`governance:check — ${findings.length} 件の不整合:`);
    for (const f of findings) console.error(`  ${f.file}:${f.line}  ${f.message}`);
    process.exitCode = 1;
  } else {
    console.log(`governance:check — 全検査 passed（${evidence}）`);
  }
}
