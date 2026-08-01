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

/** 実在検査の対象と見なすソース系拡張子（G-references）。ランタイム生成物（.bin/.bak 等）は含めない */
const REF_EXTENSIONS = /\.(md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css)$/;
/** 走査から除外するディレクトリ。名前ベース（任意の深さの生成物）とルート相対プレフィックス
 *  （untracked バッファ）を分ける——`ui/src/workspace/` のような将来の同名ソースを気づかれないまま
 *  落とさないため、workspace/worktrees はルート錨止めにする
 *  `.superpowers/` は SDD（subagent-driven-development）の作業バッファで、gitignore 済みゆえ CI の
 *  チェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で別の母集団を見る（#722）。 */
const WALK_EXCLUDE_NAMES = new Set([".git", "node_modules", "target", "dist"]);
const WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees", ".superpowers"];

/** リポジトリを歩いて snapshot（files: "/" 区切り相対パス一覧, read(rel)）を作る。
 *  列挙は fs 自身に問う（`git ls-files` の pathspec `**` 意味論の罠を避ける・health-check Check 1 注記） */
export function makeSnapshot(root) {
  const files = [];
  const walk = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const rel = path.relative(root, path.join(dir, e.name)).replaceAll("\\", "/");
      if (e.isDirectory()) {
        if (!WALK_EXCLUDE_NAMES.has(e.name) && !WALK_EXCLUDE_PREFIXES.includes(rel)) walk(path.join(dir, e.name));
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
    // 順方向: 節内のバッククォート付きソースファイル名 → basename がリポジトリに実在
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

/** TOML の 1 行から行末コメントを落として trim する。`[lints]  # opt-in` も有効な TOML ゆえ、
 *  厳密文字列比較のままだと表記の揺れで false negative になる（#713） */
const tomlLine = (raw) => raw.replace(/#.*$/, "").trim();

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
// - 見るのは `rustdoc` カテゴリだけである。`[workspace.lints.clippy]` 等が降格されてもこの検査は鳴らない
//   （clippy は `cargo clippy ... -- -D warnings` がコマンドライン側で昇格させており、workspace テーブルが
//   担っていない）。**「lints 全般が守られている」と読める書き方をしてはならない**。
// - 次の 2 つの dotted 表記は cargo 上は有効だが、この述語は非実効と判定する＝**赤に倒れる**（実測）。
//   向きが赤（沈黙しない）なので受容するが、**次の人の最も安い直し方が「検査を緩める」にならない**よう、
//   直し方を書いておく: (a) member 側の `["lints"]`（クォートした見出し）→ `[lints]` と書く、
//   (b) ルート側の `[workspace.lints]` 配下の `rustdoc.broken_intra_doc_links = "deny"`
//   → `[workspace.lints.rustdoc]` テーブルで書く。
// ---------------------------------------------------------------------------

/** ルートに在ることを要求する rustdoc lint。**名指しは意図的である**——「非空かつ全エントリ deny」だけでは
 *  片方の行が消えた形（残った 1 件は deny のまま）が緑を通る（実測）。消えたら困る識別子をカナリアが
 *  持つのは正しい形で、先例は `.claude/hooks/post-edit.test.mjs` の member 名ハードコードである。 */
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
    const value = m[2].trim();
    const level = value.startsWith("{") ? (value.match(/level\s*=\s*"([^"]+)"/)?.[1] ?? null) : (value.match(/^"([^"]+)"$/)?.[1] ?? null);
    entries.set(m[1], level);
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

/** `.claude/skills/<name>/SKILL.md` の一覧（G-skill-table・G-area-budget の共通母集団） */
function skillFiles(snapshot) {
  return snapshot.files.filter((f) => SKILL_FILE_RE.test(f));
}

/**
 * `disable-model-invocation: true` の skill 名の集合 = **harness の roster に注入されない skill**。
 * G-skill-table（表が索引すべき対象）と G-area-budget（常時ロード面に載る description）の両方がこの集合で決まるため、
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
// G-area-budget — 恒久規範の面積 ratchet（二面独立）。#593 §2 ・ADR-doc-minimization-cap-enforcement ・ADR-area-metric-characters。
// 恒久規範（読むかを選べずロードされる = コンテキスト予算への課税）の面積を単調非増加に保つ。
// 「常時ロード」と「rules」を独立の上限で見るのは、常時→rules の面替えだけで数字を下げる回避を
// 塞ぐため（合計 ratchet なら総額不変で通ってしまう）。基準の引き上げは AREA_BUDGET を理由コメント
// 付きで更新すること（= 明示的な合意の摩擦）。
// 指標は**文字数（コードポイント・CR 除く）**である。行数だと「改行を消す」が最も安い削減手段になり、
// 読む量を 1 文字も減らさずに数字だけ下げられた（ADR-area-metric-characters に実測）。CR を除くのは CRLF checkout が
// 面積を膨らませないため（\r の取り扱いは #587/#589 で二度踏んでいる）。
// 常時ロード面には skill の description を含める——毎セッション注入されるのに、どの ratchet からも
// 見えていなかった（表から description へ字を移すだけで数字が下がる抜け道になる）。
// 対象外は意図的である: skills 本文・モジュール CLAUDE.md・docs・ADR はいずれも「その作業に入った
// 者だけが読む面」であり、常時ロードからそこへ退去させることは #593 が推奨する経路そのものである。
// 課税すれば、登ってほしい階梯を登る側が罰せられる。
// ---------------------------------------------------------------------------

/** 常時ロードされる恒久規範ファイル（ルート直下の 2 文書。ほかに skill description が同じ面に載る） */
export const ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"];

/**
 * 二面独立の**文字数**上限。削減したら下げる（ratchet）。
 * 行数時代の推移: #616 で 233→228、#488 マージ節を ADR-squash-merge-issue-autoclose へ退去し 228→222、フック節の実装契約を
 * `docs/hooks.md` へ退去し 222→216、2026-07-26 に (A2) の 3 理由退去・コミュニケーション原則の圧縮・
 * 重複規則の除去で 216→191 行。ADR-area-metric-characters で指標を文字数へ切り替えた。
 * 文字数移行後の推移: 開発ワークフローの Red/Green 4 項目を 1 項目へ畳み、3層分担を 1 行にして
 * 実測 15685→15521（基準は許容差込みで 15785→15621）。
 * **2026-07-26 引き上げ 15621→15823**（実測 15554→15723・+169 字）: #725 のサイクルで「委譲した検査の
 * 成果物はファイルへ書かせる／起動したら停止条件を伝える」を「サブエージェント委譲と worktree」へ足した。
 * レビュー結果は会話にしか存在せず、届かなければ実施の有無すら区別できない——同サイクルで 6 回中 5 回
 * 踏み、そのたびに往復が発生した。**機構では吸収できない**（hook からサブエージェントの応答は見えない）
 * ため規範に置くほかなく、失われる往復のコストが 169 字の常時課税を上回ると判断した。
 * **初期値は「切り替え時点の実測 + 100 字」であり、削減して得た数字ではない**（行数からの換算元が
 * 無いため実測に据えるほかなく、rules は今回 1 字も減らしていない）。+100 は誤字修正や語順直しで
 * 赤にしないための許容差である——規範を 1 本足せば通常は超える。ゼロ余裕で据えると、あらゆる編集が
 * 定数の書き換えを要求して摩擦が日常化し、赤の意味が失われる（ADR-area-metric-characters）。
 * **2026-07-27 引き上げ 15823→16028**（実測 15798→15928・+130 字）: #749 の plan.md ゲート機構を
 * `docs/hooks.md` へ実装契約として、CLAUDE.md フック表へ発火条件・正しい対応を同期した（#749 の plan.md ゲート案内追記）。
 * ゲートの案内追記により常時ロード規範が旧予算 15823 を超過したため、実測 15928 + 100 字バッファで 16028 へ引き上げた。
 * **2026-07-27 引き下げ 16028→14712 / 8418→8328**（実測 常時ロード 15974→14612・rules 8408→8228）:
 * Claude 5 世代のコンテキスト監査。内訳は 3 種で、いずれも「読む量が実際に減った」ものである。
 *   (1) 重複の削除（CLAUDE.md）— スキル表の 9 行は harness の roster と同じ面で二重、`&&` 鎖の bullet は
 *       同ファイルのフック表と `pre-bash.mjs` の REMEDY と三重だった。
 *   (2) 導出可能の削除（CLAUDE.md）— context7 の一文・`/tmp` 行・worktree の所在は、いずれも harness の
 *       指示か `.gitignore` / `docs/build-commands.md` が同じことを言う。計 CLAUDE.md で -1119 字。
 *   (3) 計測の是正（-243 字）— `disable-model-invocation: true` の 3 件は roster に注入されないのに
 *       description を算入していた。ADR-area-metric-characters の算入根拠（毎セッション注入される）を満たさない。
 *   rules 面 -180 字は `safety-nets.md` の「カナリア」節を `docs/hooks.md` の正本へ寄せ、
 *   `snotra-core-search.md` の一般則を同時配送される `snotra-core.md` へ一本化したもの。
 * **2026-07-27 引き下げ 14712→14261**（実測 常時ロード 14612→14161）: 監査の続き。
 *   `AGENTS.md`「RETROSPECTIVE.md の運用」のタイミング・上書き・2 セクション構成は `/retrospective` の
 *   description と Step 5 に逐語で在り、常時ロード側は重複だった。`CLAUDE.md`「フック」表の PostToolUse
 *   発火条件は `selectChecks` の写しで、実際にドリフトした履歴がある（#474〜#497）——一覧を
 *   `docs/hooks.md` へ退去させ、常時ロードには「沈黙 = 合格」とその限定だけを残した。
 *   `.githooks/` の bootstrap 手順は `package.json` の `prepare` と `CONTRIBUTING.md` が持つ。
 * **2026-07-27 引き下げ 14261→14058**（実測 常時ロード 14161→13958）: `AGENTS.md` 開発ワークフローから、
 *   外部参照を背負っていない 3 件（サンプルコードの理由付記・テスト転用時の不変条件・報告の様式）を
 *   `/start-issue` と `/implement` へ移した。**移せたのは 3 件だけである**——「事前調査」「変更後の検証」は
 *   G-heading-refs のアンカーで、`.claude/rules/src-tauri.md` や `/health-check` が中身を当てにして指しており、
 *   見出しを残して中身だけ移すと G-heading-refs は緑のまま参照先が空洞になる（`.claude/rules/governance-docs.md`）。
 * **2026-07-27 rules のみ引き下げ 8328→8056**（実測 rules 8228→7956・常時ロードは 13958→14036 で**上昇**）:
 *   `.claude/rules/safety-nets.md` の敵対的読者節を `/norm-review` skill へ移した。これは削減ではなく
 *   **面替え**である——手順の本体は非課税の skill 本文へ、skill の `description` 78 字は常時ロード面へ載る。
 *   ADR-doc-minimization-cap-enforcement が二面独立にしたのはまさにこの移動を可視にするためなので、**常時ロードの基準は上げない**。
 *   結果として常時ロード面の余裕は 22 字しか残っていない——次に規範を 1 本足すときは明示的な引き上げが要る。
 * **2026-07-27 引き下げ 14058→13374（#768）**（実測 常時ロード 14036→13274・**-762 字**）:
 *   コマンドの形で判定できる規範 5 件（bash HEREDOC・`\` パス区切り・Python の `PYTHONIOENCODING`・
 *   `--no-verify` 人間専用・`git pull --ff-only`）を `.claude/hooks/pre-bash.mjs` の判定へ吸収した。
 *   これは面替えではなく**機構への吸収**である（#593 の階梯）——「シェル環境」節と git の 2 bullet が消え、
 *   事故の理由と代替手段は拒否文言（`SHAPE_REMEDY`）が実行時に運ぶ。Windows 機で 5 件のライブ実測を行い、
 *   5/5 が復帰手順付きで `exit 2` になることを確認した。判定の詳細は `docs/hooks.md`、
 *   注入面の否定の知識は `docs/adr/ADR-command-shape-norms-in-hook.md`。
 *   rules 面は本 PR で `.claude/rules/**` を触らないため据え置き。
 *
 * **引き上げは失敗ではない。** この定数は「意図が適切な分量で伝わること」を守るための道具であって、
 * 数字を小さく保つこと自体が目的ではない。ADR-doc-minimization-cap-enforcement が「ratchet は精密なメーターではなく**方向を守る道具**」
 * と述べ、ADR-area-metric-characters が「摩擦が日常化した ratchet は反射的に引き上げられ、赤が意味を失う」と警告するとおり、
 * **必要な規範を天井のせいで書かない**のは、この機構が防ごうとしている状態より悪い。上げるときに要るのは
 * 我慢ではなく理由であり、その理由をここへ書き足す摩擦が、合意の場を作るための設計である。
 * 2026-07-27: rules を 8056 → 8159 へ引き上げた（純増 +103 字）。check 系スキルの骨格
 * （4 スロット + 費用対称性・docs/check-skill-skeleton-design.md）
 * へのポインタ 1 行を safety-nets.md へ足したため、まず 8056 → 8188（+132 字＝旧ポインタ行
 * 131 字 + 改行 1 字）へ引き上げた。その後、骨格の正本を `docs/superpowers/specs/` から
 * `docs/` 直下へ移設しポインタ行のパス表記が 29 字縮んだため 8188 → 8159 へ引き下げた。
 * ADR-area-metric-characters が警告する反射的な引き上げに当たらない根拠は、引き上げ幅がポインタ 1 行ぶん
 * （102 字 + 改行 1 字 = 103 字）に一致すること——本文は 1 文字も増えていない。骨格そのものは
 * 面積対象外の文書に置き、rules へは到達経路だけを置いた（ADR-check-skill-skeleton 却下 4）。
 * **2026-07-28 引き下げ 13374→13338**（実測 常時ロード 13286→13250・-36 字）: `/norm-review` の
 * `description` から workflow の要約（「抜け道を探す 2 クラスの読者で検証し、停止条件と分量予算の
 * もとで塞ぐ」）を落とし、トリガーだけにした。description が workflow を要約すると、エージェントが
 * skill 本文を読まずに description の方に従う実測がある（`superpowers:writing-skills`）——
 * **削減は副産物であって、目的は誤ったルーティングの除去である**。余白 88 字は維持している。
 * rules 面は種蒔きへの書き換えで 8084→8123（+39 字）と増えたが、予算 8159 内につき据え置き
 * （ADR-norm-review-seeding）。
 * **2026-07-28 引き上げ 8159→8678**（実測 rules 8120→8578・+458 字）: `governance-docs.md` の
 * 「序数で指すな」の射程を見出しからファイル名・検査 ID・引用される識別子すべてへ広げ、
 * 「名前はテーマが決まった時点で意味のある形で付ける」を足した（#812 の C）。併せて `paths` へ
 * `docs/adr/**` と `scripts/governance-check.mjs` を加えた——**規範の射程だけ広げても、ADR を書く人と
 * 検査を足す人へ配送されなければ #778 と同じ「義務が行為者の視界の外」になる**。
 * ADR-area-metric-characters が警告する反射的な引き上げに当たらない根拠: 連番の衝突は `0014` `0016` `0017` で 3 回
 * 実測されており、3 回目は衝突を直す PR（#810）自身が 1 つ隣で作った。**規範を狭いまま置くことの
 * 費用が、面積の費用を上回ると判断した。**（#812）
 * **2026-07-30 引き上げ 8678→8790**（実測 rules 8578→8690・+112 字）: #843 で
 * `safety-nets.md` と `governance-docs.md` の配送対象へ `scripts/*.ps1` / `scripts/lib/**` を追加した。
 * PowerShell の共有 smoke 配管と Pester が CI・規範の実装になった一方、旧 glob は `.mjs` しか拾わず、
 * 変更者へ検証手順が届かないためである。本文規範は増やさず、必要な配送経路だけを加えた。
 * 上限は実測 8690 + 100 字の既定余白とした。
 * **2026-08-01 引き上げ 8790→9200**（実測 rules 8694→9097・+403 字）: #858 の教訓 2 件を
 * `safety-nets.md` へ置いた。**この引き上げは配送経路ではなく本文規範の追加であり、上の 2 回とは
 * 性質が違う**——ゆえに置き場の判断を先に検算した。両件とも「セーフティネットが本当に効いているか」
 * を確かめる手順であり、同ファイルの既存 2 項（FI で実測する / 入力集合を検算する）と同じ族の
 * 3 つ目（カバー範囲を検算する）である。`/plan-review` へ置く案は却下した: rules は対象を触れば
 * **自動配送**されるが skill は高リスク時に誰かが起動したときだけ走り、かつこの教訓は計画時に限らず
 * 「検査を書かない」と決めるあらゆる場面で効く。**余裕が枯れたために正しい置き場を諦める**のは、
 * ADR-area-metric-characters が却下した「ゼロ余裕」の裏返しの回避である（一度実際に起きた・#858）。
 * 上限は実測 9097 + 100 字の既定余白 → 切り上げて 9200 とした。
 */
export const AREA_BUDGET = { alwaysLoaded: 13338, rules: 9200 };

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
      findings.push(finding(f, 1, `${f} が読めない（G-area-budget 母集団の欠落）`));
      continue;
    }
    const m = text.match(/^description:[ \t]*(.*)$/m);
    const v = m ? m[1].trim() : "";
    if (!m || v === "" || v.startsWith("|") || v.startsWith(">")) {
      findings.push(finding(f, 1, "description が 1 行スカラーでない（G-area-budget が面積を数えられない）"));
      continue;
    }
    total += [...v.replace(/^["']/, "").replace(/["']$/, "")].length;
  }
  return { total, findings, count: all.length };
}

export function checkNormativeAreaBudget(snapshot) {
  const findings = [];

  const docs = sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-budget");
  const desc = skillDescriptionArea(snapshot);
  findings.push(...docs.findings, ...desc.findings);
  if (desc.count === 0) findings.push(finding(".claude/skills", 1, "skills が 0 件（G-area-budget 母集団の欠落）"));
  const alwaysTotal = docs.total + desc.total;
  if (alwaysTotal > AREA_BUDGET.alwaysLoaded) {
    findings.push(
      finding(
        "CLAUDE.md",
        1,
        `常時ロード規範 ${alwaysTotal} 字 > 基準 ${AREA_BUDGET.alwaysLoaded} 字（#593 二面 ratchet）。機構吸収か履歴退去で減らすか、正当なら AREA_BUDGET.alwaysLoaded を理由コメント付きで更新`,
      ),
    );
  }

  const ruleFiles = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (ruleFiles.length === 0) {
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G-area-budget 母集団の欠落）"));
  } else {
    const rules = sumChars(snapshot, ruleFiles, "G-area-budget");
    findings.push(...rules.findings);
    if (rules.total > AREA_BUDGET.rules) {
      findings.push(
        finding(
          ".claude/rules",
          1,
          `rules 合計 ${rules.total} 字 > 基準 ${AREA_BUDGET.rules} 字（#593 二面 ratchet）。ルーター化で減らすか、正当なら AREA_BUDGET.rules を理由コメント付きで更新`,
        ),
      );
    }
  }
  return findings;
}

/** evidence 用の実測（検査と同じ母集団・同じ数え方であることを型で担保するための共有関数） */
export function normativeArea(snapshot) {
  const always =
    (sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-budget").total ?? 0) + skillDescriptionArea(snapshot).total;
  const rules = sumChars(
    snapshot,
    snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)),
    "G-area-budget",
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
export function governanceDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      ["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "SPEC.md"].includes(f) ||
      (f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/")) ||
      /^(snotra-core|snotra-egui-runtime|src-tauri|snotra-settings)\/CLAUDE\.md$/.test(f) ||
      /^\.claude\/rules\/[^/]+\.md$/.test(f) ||
      /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f),
  );
}

/**
 * G-heading-refs の対象。見出し参照はガバナンス文書の外（`PERFORMANCE.md`・`.claude/agents/`）にも書かれ、
 * 実際にそこで腐っていた（`src-tauri/CLAUDE.md`「TrySuspend / Resume パターン」）ため母集団を広く取る。
 * 除外は履歴資料（`docs/superpowers/`）と作業バッファ（`workspace/`・`/implement` が削除する）のみ。
 */
export function headingRefDocs(snapshot) {
  return snapshot.files.filter(
    (f) => f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("workspace/"),
  );
}

// ---------------------------------------------------------------------------
// G-config-reachability — config フィールドの到達性（ランチャが読むか）の双方向照合。
//
// config は到達性の検出器を持たない面である——private / `pub(crate)` の関数は呼ばなければ
// `dead_code`、型は変えれば下流が compile-fail、モジュール索引は G-module-index、文書参照は G-references が捕まえる
// （lib crate の `pub` 項目は `dead_code` の対象外なので、この検出器は関数にも穴を持つ）。
// config のフィールドは `#[serde(default)]` を付ければ誰も読まなくてもコンパイルが通り、
// **届いたかを誰も検査しない**。原理は `docs/development-principles.md`「config の値は到達性の検出器を持たない」。
// 実例: `[visual].background_color` の描画経路は消費者ゼロのまま（実背景は renderer.rs の
// `CLEAR_COLOR` ハードコード）、`VisualConfig.preset` はランチャが型を import すらしていない。
//
// 判定は「ランチャ側ソースに識別子が現れないフィールドの集合」と下表の**双方向一致**である。
// 表は「読まれない理由」を持ち、検査は集合の一致だけを見る（理由は人間が読む）。
//
// **「0 件 = ランチャが読まない」に意味を与えるため、0 件になる経路を列挙して塞ぐ**
// （`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は…」）:
// - 本当に読まれない → 検出したい当のもの
// - `Option` の実効値をメソッドが畳む（`effective_result_limit()`）→ 表に載せる
// - legacy 受け皿（`apply_migrations()` だけが読む）→ 表に載せる
// - 設定エディタ専用（`snotra-settings/`）→ 表に載せる。同 crate は母集団に入れない
//   ——ディスクを往復してユーザーへ表示し返すだけで、ランチャの挙動には届かないため
// - **コメントに現れる** → `stripRustComments` で塞いだ。塞がないと `preset` のような普通の
//   英単語が doc コメント（`opener.rs` の "opener preset available"）に埋もれ、検出できない（実測）
// - **`#[cfg(test)]` の中だけが読む** → launcher blob も production に絞って塞いだ。塞がないと
//   `visible_rows` が `engine.rs` のテスト 4 箇所だけで「読まれている」側に落ちる（実測）
// - **文字列リテラル内の `//` 以降が消える** → `stripRustComments` の誤除去。向きは赤側
//   （読みが消えて findings が増える）ので沈黙しない。塞いでいない
//
// **受容する残余（false negative）**: 一致はフィールド名のドット始まり（`.field`）で見るため、
// **同名のフィールドアクセスがランチャ側に別の意味で在ると、読まれていなくても 0 件集合に入らない**。
// フィールド名が `path` / `name` / `key` / `search` のようにありふれた語である全フィールドが
// この分解能を持たない（`AppearanceConfig.max_results` はその一例）。
// **この残余は表を静かに過小申告させる**——実例が `OpenerRule.tools` で、実体は `target` と同じく
// `find_matching_tools()` 経由でしか読まれないのに、`ToolFrame.tools`（`search_state.rs`・別の型）への
// `.tools` 一致で「読まれている」側に落ちる。**表に載せると今度は「表の記載が古い」で赤になるため、
// 載せることもできない**。同じ struct の `target` だけが表に在るのはこの分解能の帰結であって、
// 分類の判断が割れているのではない。
//
// **母集団の範囲**: `Deserialize` を derive する struct のフィールドだけである（= config.toml から
// 読まれる型）。**enum variant のフィールド**（`InstantAction::Url` の `url` 等）と
// **generics 形の struct 定義**（`pub struct Foo<T>`）は入らない——増えたときに G-config-reachability は止めない。
// ---------------------------------------------------------------------------
/** 母集団のソース。`[[openers]]` と `[hotkey]` は `config.rs` が re-export するだけなので、
 * serde 型の実体を持つ各モジュールも列挙する。 */
export const CONFIG_SOURCE_PATHS = [
  "snotra-core/src/config.rs",
  "snotra-core/src/opener.rs",
  "snotra-core/src/hotkey.rs",
];
/** 読み手として数えるソース。`snotra-settings/` は入れない（上のコメント参照）。
 *  `snotra-egui-runtime/` も入れない——`snotra-core` に依存せず config を読めないため（実測）。
 *  将来依存が入って config を直接読んだ場合、G-config-reachability は赤へ振れる（安全側） */
export const LAUNCHER_PREFIXES = ["src-tauri/src/", "snotra-core/src/"];
/** 抽出アンカーの**部分**腐敗を検知する（0 件だけを見ると、途中で切れた母集団が沈黙する） */
export const CONFIG_EXPECTED_STRUCTS = [
  "Config", "HotkeyConfig", "GeneralConfig", "SearchConfig", "AppearanceConfig",
  "VisualConfig", "CustomTheme", "ScanPath", "PathsConfig", "InstantCommand",
  "OpenerTool", "OpenerRule",
];

/** ランチャが読まないフィールドと、その理由。**表のキーが `Struct.field` なのは理由を struct ごとに
 *  書き分けるためであって、判定の粒度ではない**——read 判定はフィールド名だけで行うので、
 *  同名フィールド（`SearchConfig.top_n_history` と `AppearanceConfig.top_n_history` 等）は必ず同じ判定になる */
export const NO_LAUNCHER_READ = {
  "SearchConfig.result_limit": "実効値は `effective_result_limit()` が畳む（未設定を既定へ）。ランチャはメソッドを呼ぶ",
  "SearchConfig.recent_limit": "実効値は `effective_recent_limit()` が畳む（同上）",
  "AppearanceConfig.visible_rows": "実効値は `effective_visible_rows()` が畳む（同上）",
  "OpenerRule.target": "マッチングは `opener.rs` の中で閉じる。ランチャは `find_matching_tools()` を呼ぶ",
  "SearchConfig.top_n_history": "legacy 受け皿。`apply_migrations()` が `result_limit` へ移す（`skip_serializing`）",
  "SearchConfig.max_history_display": "legacy 受け皿。`apply_migrations()` が `recent_limit` へ移す",
  "AppearanceConfig.top_n_history": "legacy 受け皿（`search` 側と対の旧キー）",
  "AppearanceConfig.max_history_display": "legacy 受け皿（同上）",
  "PathsConfig.additional": "legacy 受け皿。`apply_migrations()` が `scan` へ移す",
  "VisualConfig.custom_theme": "設定エディタがカスタム配色を 1 組保存する格納庫。ランチャが読まないのは設計意図",
  "VisualConfig.preset": "設定エディタの Custom カード強調だけが読む。ランチャは `ThemePreset` を import すらしない",
};

/** Rust のコメントを落とす。落とさないと普通の英単語が doc コメントに埋もれる（上のコメント参照）。
 *  文字列リテラル内の `//` 以降も落ちるが、向きは赤側（読みが消える）ゆえ沈黙しない */
function stripRustComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/\/\/.*$/gm, " ");
}

/** `#[cfg(test)]` 以降を落とす。**母集団と読み手の両方に適用する**——読み手側で落とさないと
 *  「テストだけが読む」フィールドが読まれている側へ落ちる（`visible_rows` で実測） */
function productionOnly(src) {
  return src.split(/^#\[cfg\(test\)\]/m)[0];
}

/** `Deserialize` を derive する `pub struct` のフィールドを `{struct, field}` で列挙する
 *  （= config.toml から読まれる型。derive を持たない型〔`OpenerPreset` 等〕は config のキーではない） */
export function configFields(text) {
  const production = productionOnly(text);
  const out = [];
  for (const m of production.matchAll(/pub struct (\w+)\s*\{([\s\S]*?)\n\}/g)) {
    // 直前の空行区切りブロック（derive + 属性 + doc コメント）に Deserialize があるか。
    // **空行の分割は CRLF 対応で行う**——`lastIndexOf("\n\n")` は CI の Windows checkout
    // （autocrlf=true）で見つからず、ブロックが全文へ広がって母集団が壊れる（PR #793 の CI で実測）
    const attrs = production.slice(0, m.index).split(/\r?\n[ \t]*\r?\n/).pop() ?? "";
    if (!/#\[derive\([^\]]*\bDeserialize\b/.test(attrs)) continue;
    for (const f of m[2].matchAll(/^\s*pub (\w+):/gm)) out.push({ struct: m[1], field: f[1] });
  }
  return out;
}

export function checkConfigFieldReachability(snapshot, table = NO_LAUNCHER_READ, expectedStructs = CONFIG_EXPECTED_STRUCTS) {
  const fields = [];
  for (const p of CONFIG_SOURCE_PATHS) {
    const text = snapshot.read(p);
    if (text == null) return [finding(p, 1, `${p} が読めない（G-config-reachability 母集団の欠落）`)];
    fields.push(...configFields(text));
  }
  if (fields.length === 0) {
    return [finding(CONFIG_SOURCE_PATHS[0], 1, "`pub struct` のフィールドが 1 件も抽出できない（G-config-reachability 母集団の欠落。抽出アンカーの腐敗）")];
  }
  // 部分腐敗の検知: 0 件だけを見ると、途中で切れた母集団が沈黙する
  const structs = new Set(fields.map((f) => f.struct));
  const missing = expectedStructs.filter((s) => !structs.has(s));
  if (missing.length > 0) {
    return [finding(CONFIG_SOURCE_PATHS[0], 1, `期待する struct が抽出できない: ${missing.join(", ")}（G-config-reachability 抽出アンカーの部分腐敗）`)];
  }
  const launcher = snapshot.files.filter(
    (f) => f.endsWith(".rs") && !CONFIG_SOURCE_PATHS.includes(f) && LAUNCHER_PREFIXES.some((p) => f.startsWith(p)),
  );
  if (launcher.length === 0) return [finding(CONFIG_SOURCE_PATHS[0], 1, "ランチャ側ソースが 0 件（G-config-reachability 母集団の欠落）")];
  const blob = launcher.map((f) => stripRustComments(productionOnly(snapshot.read(f) ?? ""))).join("\n");

  const all = new Set(fields.map((f) => `${f.struct}.${f.field}`));
  // 一致はドット始まり（`.field`）で見る——struct 初期化の `field:` は「書き」であって読みではない
  const unread = new Set(
    fields.filter((f) => !new RegExp(`\\.${f.field}\\b`).test(blob)).map((f) => `${f.struct}.${f.field}`),
  );
  const findings = [];
  for (const key of Object.keys(table)) {
    if (!all.has(key)) {
      findings.push(finding(CONFIG_SOURCE_PATHS[0], 1, `表の \`${key}\` に対応するフィールドが config serde 型の正本に無い（表の腐敗）`));
    } else if (!unread.has(key)) {
      findings.push(finding(CONFIG_SOURCE_PATHS[0], 1, `\`${key}\` はランチャ側から読まれている。表の記載が古い（NO_LAUNCHER_READ から外す）`));
    }
  }
  for (const key of unread) {
    if (!(key in table)) {
      findings.push(
        finding(CONFIG_SOURCE_PATHS[0], 1, `\`${key}\` をランチャ側が読んでいない。消費者を与えるか、読まない理由を NO_LAUNCHER_READ へ載せる`),
      );
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G-stale-identifiers — 規範の散文に残る、現行語彙に無い識別子（腐り）の検出（#736 の同クラス）。
//
// #698 が述べた「述語だけが書かれた間接参照」は概念での再導出でしか拾えなかったが、
// **識別子として書かれた腐りは機械で拾える**。G-references が見るのはパスの実在までで、
// 識別子の実在は誰も見ていなかった。
//
// **自称スコープは狭い。** 見るのは `.claude/**` の散文中の**バッククォート内 camelCase 識別子**
// だけである。frontmatter の文字列・素の表テキスト・日本語散文（「リアクティブ制約」等）は
// 構造的に対象外で、#736 が挙げた 10 件のうちこの述語が届くのは 0 件である（実測）。
// **この検査は #736 の代替ではない**——同 issue は手作業で閉じ、G-stale-identifiers が引き受けるのは再発防止だけである。
//
// 判定: 識別子が「現行語彙」に 1 度も現れないなら finding。現行語彙は 2 つの正本からなる:
// - **ソースの非コメント本文**（`stripRustComments`）。コメントを含めると `resetForShow` のような
//   由来注記（「〜相当」「parity」）が語彙に化け、腐りが検出できない（実測 11 件）
// - **`SPEC.md`**。SPEC は意図の正本であり、その語彙を写した規範は腐っていない
//   ——SPEC 自身の stale は `#735` の射程である（`folderState` 8 件・`toolSelectionState` 4 件・
//   `resetForShow` 2 件が実際に SPEC 側に在る）。**この 1 行が無いと、写しを SSOT より先に直す**
//
// **外部ツールの語彙を構造的に外す**: `gh` / `npm` / `cargo` 等のコマンドが同じ行に在るなら、
// その行の識別子はコマンドの引数（`--json closingIssuesReferences` 等）である。免除注記の機構を
// 設けない契約（本ファイル冒頭）を守るため、除外リストではなく行の形で外す。
// これで実測の偽陽性 1 件が構造的に消え、真の腐り 6 件は残った（両方向で確認）。
//
// **受容する残余**: 単語 1 つの識別子（`Glob` `expand` `plain`）は対象外である。こぶを 1 つ以上
// 要求しないと、harness のツール名と散文の語彙が大量に混じる（実測 53 件中 40 件弱）。
// ---------------------------------------------------------------------------

/** 現行語彙の正本になるソース拡張子 */
const VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml)$/;
/** 現行語彙の正本になる文書（意図の SSOT） */
export const VOCAB_DOCS = ["SPEC.md"];
/** バッククォート内で腐りを問う形: camelCase（こぶ 1 つ以上）・末尾 `()` は任意 */
const STALE_IDENT = /^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/;
/** 同じ行に在れば、その行の識別子は外部ツールの引数と見なす */
const EXTERNAL_CMD_LINE = /`(gh|npm|cargo|git|node|pwsh|npx) /;

/** 規範の散文（G-stale-identifiers の母集団）。skills / rules / agents の md */
export function staleIdentifierDocs(snapshot) {
  return snapshot.files.filter((f) => /^\.claude\/(skills\/.*|rules\/[^/]+|agents\/[^/]+)\.md$/.test(f));
}

/** 現行語彙。ソースはコメントを落とす（`#` コメントの言語は行頭・行中とも落とす） */
export function currentVocabulary(snapshot) {
  const parts = [];
  for (const f of snapshot.files) {
    if (!VOCAB_SOURCE_EXT.test(f)) continue;
    const src = snapshot.read(f);
    if (src == null) continue;
    parts.push(/\.(ps1|toml)$/.test(f) ? src.replace(/#.*$/gm, " ") : stripRustComments(src));
  }
  for (const d of VOCAB_DOCS) parts.push(snapshot.read(d) ?? "");
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
      if (EXTERNAL_CMD_LINE.test(line)) continue;
      for (const m of line.matchAll(/`([^`\n]+)`/g)) {
        const raw = m[1];
        if (raw.includes("/") || raw.includes(" ") || raw.includes(".")) continue;
        const im = raw.match(STALE_IDENT);
        if (!im) continue;
        checked += 1;
        if (!inVocab(im[1])) {
          findings.push(
            finding(doc, lineNo, `規範の散文に、現行語彙に無い識別子が残っている: \`${raw}\`（ソースの非コメント本文と SPEC.md のどちらにも無い）`),
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
// `/implement`「出力」項目 3 は報告の母集団を 4a の列挙で閉じており、それは表の**写し**である。
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
// `/plan-review` `/norm-review` は `-check` で終わらないため**構造的に外れる**。`/health-check` は
// 表に現れない（ルート `CLAUDE.md` のスキル表に在り、そちらは G-skill-table が見る）。
//
// これで #778 の (a)（表側へ同期義務を 1 行置く）が不要になった——`AGENTS.md` は G-area-budget の常時ロード面で
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
    ...snapshot.files.filter((f) => /^\.claude\/skills\/.*\.md$/.test(f)),
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
  const staleDocs = staleIdentifierDocs(snapshot);
  sink.docs = docs;
  sink.refDocs = refDocs;
  sink.staleDocs = staleDocs;
  const record = (key, r) => {
    sink[key] = r.checked;
    return r.findings;
  };
  return [
    { id: "G-module-index", run: () => checkModuleIndex(snapshot) },
    { id: "G-architecture-table", run: () => checkArchitectureTable(snapshot) },
    { id: "G-references", run: () => checkReferences(snapshot, docs) },
    { id: "G-spec-sections", run: () => checkSpecSections(snapshot, docs) },
    { id: "G-build-commands", run: () => checkBuildCommands(snapshot) },
    { id: "G-workspace-lints", run: () => checkWorkspaceLints(snapshot) },
    { id: "G-ci-table", run: () => checkCiTable(snapshot) },
    { id: "G-rules-globs", run: () => checkRulesGlobs(snapshot) },
    { id: "G-skill-table", run: () => checkSkillTable(snapshot) },
    { id: "G-hook-commands", run: () => checkHookCommands(snapshot) },
    { id: "G-hook-fires", run: () => checkHookFires(snapshot) },
    { id: "G-area-budget", run: () => checkNormativeAreaBudget(snapshot) },
    { id: "G-config-reachability", run: () => checkConfigFieldReachability(snapshot) },
    { id: "G-check-skill-enumeration", run: () => checkCheckSkillEnumeration(snapshot) },
    { id: "G-adr-file-names", run: () => checkAdrFileNames(snapshot) },
    { id: "G-adr-citations", run: () => record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, docs))) },
    { id: "G-heading-refs", run: () => record("headingRefs", scanHeadingRefs(snapshot, refDocs)) },
    { id: "G-stale-identifiers", run: () => record("stale", scanStaleIdentifiers(snapshot, staleDocs)) },
    { id: "G-near-heading-refs", run: () => record("nearRefs", scanNearHeadingRefs(snapshot, refDocs)) },
  ];
}

export function runAll(snapshot) {
  const ctx = {};
  const checks = buildChecks(snapshot, ctx);
  const findings = [];
  if (ctx.docs.length === 0) findings.push(finding(".", 1, "ガバナンス文書が 0 件（母集団の欠落）"));
  if (ctx.refDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象 md が 0 件（母集団の欠落）"));
  if (ctx.staleDocs.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の対象 md が 0 件（母集団の欠落）"));
  for (const c of checks) findings.push(...c.run());
  const area = normativeArea(snapshot);
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)).length;
  const skills = snapshot.files.filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f)).length;
  const configFieldCount = CONFIG_SOURCE_PATHS.flatMap((p) => configFields(snapshot.read(p) ?? "")).length;
  const evidence = `検査 ${checks.length} 件 / 対象文書 ${ctx.docs.length} 件 / rules ${rules} 件 / skills ${skills} 件 / 恒久規範 常時ロード ${area.always}/${AREA_BUDGET.alwaysLoaded} 字・rules ${area.rules}/${AREA_BUDGET.rules} 字 / 見出し参照 ${ctx.headingRefs} 件を ${ctx.refDocs.length} 文書から照合 / workspace member ${workspaceMembers(snapshot).members.length} 件の lints opt-in / config フィールド ${configFieldCount} 件の到達性 / 規範の識別子 ${ctx.stale} 件を ${ctx.staleDocs.length} 文書から照合 / 近傍の見出し参照 ${ctx.nearRefs} 件 / ADR ${adrFiles(snapshot).length} 本の名前 / ADR の短縮引用 ${ctx.adrCitations} 件`;
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
