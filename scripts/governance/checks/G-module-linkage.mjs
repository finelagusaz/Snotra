//! G-module-linkage — `<crate>/src/**/*.rs` が crate ルートから `mod` 宣言で到達できるか（#1085）。
import path from "node:path";
import { finding, workspaceMembers, crateSourceFiles } from "../lib.mjs";

export const id = "G-module-linkage";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkModuleLinkage(snapshot);
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
// - **`macro_rules!` の本体で列 0 に来た `mod` を拾う**（沈黙の向き）。インライン `mod x { ... }` と
//   同類型だが、そちらは列で外れるのに対しこちらは外れない。rustfmt はマクロ本体を整形しないことが
//   あるので、列 0 に来る余地は消えない。**噛むのはマクロ本体が列 0 へ `mod` を置いたときだけ**
//   であり、マクロ本体を持つこと自体では噛まない。
// - `include!` によるファイル取り込みを追わない（現在 0 件）。当該ファイルが赤に倒れる向き。
// - **cfg で分けた同名モジュールの片方だけが `#[path]` を持つと、もう片方が赤になる**
//   （`#[cfg(windows)] #[path="win.rs"] mod platform;` ＋ `#[cfg(unix)] mod platform;`）。
//   名前で重複を落とすため通常形の候補が消える。**通常形も併せて積む向きへは倒さない**——
//   `#[path]` だけで宣言された名前と同名のファイルが実在すると、それを到達済みに見せて沈黙するため。
//   現在 0 件（`ime.rs` は非 Windows 側をインライン `mod` で書いているので当たらない）。
// - **属性の綴りが `#[path = "..."]`（二重引用符・同一行）から外れると赤に倒れる**——
//   `#[path = r"..."]`（raw string）・`#[path = "dir"]`（ディレクトリ指定）・
//   `#[cfg_attr(..., path = "...")]`・`#[cfg(windows)] mod win;` のような同一行の属性つき宣言。
//   現在 0 件で、いずれも沈黙ではなく過検出の向きである。
// - `[lib] path = ...` で `src/` の外へソースを置く crate は母集団から外れる（現在 0 件）。

/** ある `.rs` が宣言する子モジュールの探索基準ディレクトリ。
 *  `lib.rs` / `main.rs` / `mod.rs` は自分のディレクトリ、それ以外は自分の stem のディレクトリを持つ。 */
function moduleChildDir(file) {
  const dir = path.posix.dirname(file);
  const base = path.posix.basename(file);
  if (base === "lib.rs" || base === "main.rs" || base === "mod.rs") return dir;
  return `${dir}/${base.slice(0, -3)}`;
}

/** raw string の開始（`r"` / `r#"` / `br##"` …）。**sticky** ゆえ走査位置ちょうどからしか当たらない。 */
const RAW_STRING_PREFIX = /b?r(#*)"/y;

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
    // **窓で切らない。** 固定長の窓（かつて 24 文字）だと、ハッシュがその長さを超える raw string を
    // 見落として中身をコードとして読み、そこに綴られた `mod` を拾って孤児を緑にする（沈黙の向き。
    // 2026-08-14 のレビューが 23 個で閾値を実測）。Rust はハッシュを 255 個まで許す。
    RAW_STRING_PREFIX.lastIndex = i;
    const raw = RAW_STRING_PREFIX.exec(text);
    if (raw) {
      const close = `"${"#".repeat(raw[1].length)}`;
      const end = text.indexOf(close, i + raw[0].length);
      blank(end < 0 ? n : end + close.length);
      continue;
    }
    // `b` 接頭辞（byte 文字列 / byte char）を許して、リテラル本体の開始位置を返す。
    const literalStart = (ch) => (text[i] === ch ? i : text[i] === "b" && text[i + 1] === ch ? i + 1 : -1);
    const quote = literalStart('"');
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
    const tick = literalStart("'");
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
  const ownDir = path.posix.dirname(file);
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
  const sources = crateSourceFiles(snapshot);
  for (const crate of members) {
    const prefix = `${crate}/src/`;
    // `src/bin/` は cargo が target として自動発見するので `mod` 宣言を要さない（射程外）。
    // **除外は `crateSources` ドメインの側ではなくここに置く**——射程の判断はこの検査のもので、
    // 母集団を共有する他の消費者（今は無い）まで巻き込まない。
    const population = sources.filter((f) => f.startsWith(prefix) && !f.startsWith(`${prefix}bin/`));
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
