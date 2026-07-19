// governance:check — ガバナンス文書の決定的検査（#587）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された
// shebang 行は vitest の transform を SyntaxError で落とす（PR #592 で実測。
// 他の *.mjs も同じ理由で shebang なし。起動は常に `node scripts/...` 経由）。
//
// PostToolUse hook は `.md`・rules・skills に検査を割り当てない（#497 で受容した残余）。
// 本スクリプトはその残余のうち決定的に照合できる項目を PR CI（governance-check job）と
// `npm run governance:check` で引き取る。意味判断（責務の妥当性・npm 系ラッパーの等価判断・
// メモリ整合）は `/health-check` に残る（cargo フラグ照合は G9 が機械化済み・#589）。
//
// 契約:
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）
// - findings ゼロ → exit 0 + 照合母集団の件数を印字（根拠の接地）
// - findings あり → exit 1 + `file:line` 付き全件列挙。免除注記の機構は設けない
// - 空母集団（対象文書 0 件・rules 0 件・skills 0 件）は明示 fail（沈黙経路の閉塞）
// - 各検査はスナップショット注入の純関数（scripts/governance-check.test.mjs がフィクスチャで
//   フォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する）
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** 実在検査の対象と見なすソース系拡張子（G3）。ランタイム生成物（.bin/.bak 等）は含めない */
const REF_EXTENSIONS = /\.(md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css)$/;
/** 走査から除外するディレクトリ。名前ベース（任意の深さの生成物）とルート相対プレフィックス
 *  （untracked バッファ）を分ける——`ui/src/workspace/` のような将来の同名ソースを気付かれないまま
 *  落とさないため、workspace/worktrees はルート錨止めにする */
const WALK_EXCLUDE_NAMES = new Set([".git", "node_modules", "target", "dist"]);
const WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees"];

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
// G1 — 各サブディレクトリ CLAUDE.md「モジュール構成」↔ 実ファイルの双方向照合。
// basename 包含方式: ディレクトリ集約行（`commands/` のベア名列挙）・`tabs/` プレフィックス省略・
// 1 行複数バッククォートをパースせずに済ませる意図的な弱化（wrong-directory 検出は放棄）。
// ---------------------------------------------------------------------------
const G1_CRATES = {
  "snotra-core": { src: "snotra-core/src/", exts: /\.rs$/ },
  "src-tauri": { src: "src-tauri/src/", exts: /\.rs$/ },
  "snotra-settings": { src: "snotra-settings/src/", exts: /\.rs$/ },
  // ui のテスト除外は vitest.config.ts の include（ui/src/**/*.test.{ts,tsx}）と一致させる
  ui: { src: "ui/src/", exts: /\.(ts|tsx)$/, excludeTest: /\.test\.(ts|tsx)$/ },
};

export function checkModuleIndex(snapshot, crates = Object.keys(G1_CRATES)) {
  const findings = [];
  const allBasenames = new Set(snapshot.files.map((f) => f.split("/").pop()));
  for (const crate of crates) {
    const cfg = G1_CRATES[crate];
    const mdPath = `${crate}/CLAUDE.md`;
    const text = snapshot.read(mdPath);
    if (text == null) {
      findings.push(finding(mdPath, 1, "CLAUDE.md が読めない（G1 母集団の欠落）"));
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
// G2 — docs/architecture.md にファイル単位モジュール表が再導入されていないか（旧 Check 2）
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
// G3 — ガバナンス文書群の参照実在（Markdown リンク + バッククォート内パス様参照）。
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
      findings.push(finding(doc, 1, "対象文書が読めない（G3 母集団の欠落）"));
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
// G4 — SPEC.md 番号連続性 + SPEC 前置の §N(.x) 参照の実在（旧 Check 4 + #587 新規）。
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
  if (sections.size === 0) findings.push(finding("SPEC.md", 1, "セクション見出し（## N.）が 1 件も無い（G4 母集団の欠落）"));
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 母集団欠落は G3 が報告する
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

// ---------------------------------------------------------------------------
// G5 — docs/build-commands.md の npm script / cargo test -p crate の実在（旧 Check 5 の決定的部分）。
// crate 名はディレクトリ名でなく各 member Cargo.toml の [package] name（`-p snotra` = src-tauri/）。
// check/clippy は --workspace で cargo 自身がSSOTを読むため照合対象外（#500）。
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
  const members = (snapshot.read("Cargo.toml") ?? "").match(/members\s*=\s*\[([^\]]*)\]/)?.[1] ?? "";
  for (const m of members.matchAll(/"([^"]+)"/g)) {
    const name = (snapshot.read(`${m[1]}/Cargo.toml`) ?? "").match(/^name\s*=\s*"([^"]+)"/m)?.[1];
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
// G6 — 「CI/CD メモ」対応表 ↔ .github/workflows/*.yml（旧 Check 10 の機械部分）。
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
    /* G5 が報告する */
  }
  const lines = text.split("\n");
  const start = lines.findIndex((l) => /^\| 検証コマンド \| workflow \|/.test(l));
  if (start === -1) return [finding(p, 1, "「CI/CD メモ」対応表（| 検証コマンド | workflow |）が見つからない")];
  for (let i = start + 2; i < lines.length && lines[i].startsWith("|"); i++) {
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) continue;
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
// G7 — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチ（旧 Check 8）。
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
  if (rules.length === 0) return [finding(".claude/rules", 1, "rules ファイルが 0 件（G7 母集団の欠落）")];
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

// ---------------------------------------------------------------------------
// G8 — ルート CLAUDE.md「利用できるスキル」表 ↔ .claude/skills/*/SKILL.md（旧 Check 9）
// ---------------------------------------------------------------------------
export function checkSkillTable(snapshot) {
  const findings = [];
  const text = snapshot.read("CLAUDE.md");
  if (text == null) return [finding("CLAUDE.md", 1, "ルート CLAUDE.md が読めない")];
  const section = text.split(/^## 利用できるスキル$/m)[1]?.split(/^## /m)[0] ?? "";
  const inTable = new Set([...section.matchAll(/^\|\s*`\/([a-z0-9-]+)`/gm)].map((m) => m[1]));
  const inDirs = new Set(
    snapshot.files
      .filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f))
      .map((f) => f.split("/")[2]),
  );
  if (inDirs.size === 0) findings.push(finding(".claude/skills", 1, "SKILL.md が 0 件（G8 母集団の欠落）"));
  for (const s of inTable) {
    if (!inDirs.has(s)) findings.push(finding("CLAUDE.md", 1, `スキル表の /${s} に SKILL.md が無い（.claude/skills/${s}/）`));
  }
  for (const s of inDirs) {
    if (!inTable.has(s)) findings.push(finding("CLAUDE.md", 1, `.claude/skills/${s}/ がスキル表に載っていない`));
  }
  return findings;
}

// ---------------------------------------------------------------------------
// G9 — PostToolUse hook の cargo コマンド ↔ docs/build-commands.md カテゴリ A の照合（#589）。
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
  if (hookSrc == null) return [finding(hookPath, 1, "post-edit.mjs が読めない（G9 母集団の欠落）")];
  // cargoSpec([...]) の引数配列を抽出（clippy は複数行折返しのため dotall 必須）
  const hookCommands = [...hookSrc.matchAll(/cargoSpec\(\[([\s\S]*?)\]\)/g)].map((m) =>
    [...m[1].matchAll(/"([^"]*)"/g)].map((t) => t[1]),
  );
  if (hookCommands.length === 0) {
    return [finding(hookPath, 1, "cargoSpec([...]) が 1 件も抽出できない（G9 母集団の欠落。抽出アンカーの腐敗か buildCommand のリファクタ）")];
  }
  const docsPath = "docs/build-commands.md";
  const docsText = snapshot.read(docsPath);
  if (docsText == null) return [finding(docsPath, 1, "docs/build-commands.md が読めない（G9）")];
  // カテゴリ A 節の bash フェンス内 cargo 行を母集団にする（行末 # コメントを除去）
  const sectionA = docsText.split(/^### A\. /m)[1]?.split(/^### /m)[0] ?? "";
  // 行分割は \r?\n — CRLF checkout（Windows CI・autocrlf=true）では `.` が \r に
  // マッチしないため、\r を残すと行末コメント除去 `#.*$` が発火しない（PR #595 で実測）
  const docsLines = sectionA
    .split(/\r?\n/)
    .filter((l) => l.trim().startsWith("cargo "))
    .map((l) => l.replace(/\s+#.*$/, "").trim().split(/\s+/).join(" "));
  if (docsLines.length === 0) {
    return [finding(docsPath, 1, "カテゴリ A の cargo コマンド行が 0 件（G9 母集団の欠落）")];
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
// G10 — 恒久規範の面積 ratchet（二面独立）。#593 §2。
// 恒久規範（毎セッション/引き金時にロード = コンテキスト予算への課税）の面積を単調非増加に保つ。
// 「常時ロード」と「rules」を独立の上限で見るのは、常時→rules の面替えだけで数字を下げる回避を
// 塞ぐため（合計 ratchet なら総額不変で通ってしまう）。基準の引き上げは LINE_BUDGET を理由コメント
// 付きで更新すること（= 明示的な合意の摩擦）。ADR・spec・issue は履歴側ゆえ対象外。
// 行数は \r?\n で数える（CRLF checkout で `.` が \r を落とす沈黙経路を #587/#589 で二度踏んだ）。
// ---------------------------------------------------------------------------

/** 常時ロードされる恒久規範ファイル（ルート直下の 2 文書） */
export const ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"];

/** 二面独立の行数上限。削減したら下げる（ratchet）。#616 で常時ロードを写し除去で 233→228 に削減 */
export const LINE_BUDGET = { alwaysLoaded: 228, rules: 173 };

/** \r?\n の出現数 = wc -l 相当。読めなければ null（母集団欠落を上位で検知） */
function countLines(text) {
  return text == null ? null : (text.match(/\r?\n/g) || []).length;
}

/** 指定ファイル群の総行数を数える。読めないファイルは finding に積み、行数へは算入しない */
function sumLines(snapshot, files, gLabel) {
  let total = 0;
  const findings = [];
  for (const f of files) {
    const c = countLines(snapshot.read(f));
    if (c == null) findings.push(finding(f, 1, `${f} が読めない（${gLabel} 母集団の欠落）`));
    else total += c;
  }
  return { total, findings };
}

export function checkNormativeLineBudget(snapshot) {
  const findings = [];

  const always = sumLines(snapshot, ALWAYS_LOADED_FILES, "G10");
  findings.push(...always.findings);
  if (always.total > LINE_BUDGET.alwaysLoaded) {
    findings.push(
      finding(
        "CLAUDE.md",
        1,
        `常時ロード規範 ${always.total} 行 > 基準 ${LINE_BUDGET.alwaysLoaded} 行（#593 二面 ratchet）。機構吸収か履歴退去で減らすか、正当なら LINE_BUDGET.alwaysLoaded を理由コメント付きで更新`,
      ),
    );
  }

  const ruleFiles = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (ruleFiles.length === 0) {
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G10 母集団の欠落）"));
  } else {
    const rules = sumLines(snapshot, ruleFiles, "G10");
    findings.push(...rules.findings);
    if (rules.total > LINE_BUDGET.rules) {
      findings.push(
        finding(
          ".claude/rules",
          1,
          `rules 合計 ${rules.total} 行 > 基準 ${LINE_BUDGET.rules} 行（#593 二面 ratchet）。ルーター化で減らすか、正当なら LINE_BUDGET.rules を理由コメント付きで更新`,
        ),
      );
    }
  }
  return findings;
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

/** G3/G4 の対象文書（ガバナンス文書群）。docs/superpowers/ は歴史資料（#589 で非規範化）ゆえ除外 */
export function governanceDocs(snapshot) {
  return snapshot.files.filter(
    (f) =>
      ["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "SPEC.md"].includes(f) ||
      (f.startsWith("docs/") && f.endsWith(".md") && !f.startsWith("docs/superpowers/")) ||
      /^(snotra-core|src-tauri|ui|snotra-settings)\/CLAUDE\.md$/.test(f) ||
      /^\.claude\/rules\/[^/]+\.md$/.test(f) ||
      /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f),
  );
}

export function runAll(snapshot) {
  const docs = governanceDocs(snapshot);
  const findings = [];
  if (docs.length === 0) findings.push(finding(".", 1, "ガバナンス文書が 0 件（母集団の欠落）"));
  findings.push(
    ...checkModuleIndex(snapshot),
    ...checkArchitectureTable(snapshot),
    ...checkReferences(snapshot, docs),
    ...checkSpecSections(snapshot, docs),
    ...checkBuildCommands(snapshot),
    ...checkCiTable(snapshot),
    ...checkRulesGlobs(snapshot),
    ...checkSkillTable(snapshot),
    ...checkHookCommands(snapshot),
    ...checkNormativeLineBudget(snapshot),
  );
  const always = ALWAYS_LOADED_FILES.reduce((n, f) => n + (countLines(snapshot.read(f)) ?? 0), 0);
  const ruleLines = snapshot.files
    .filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f))
    .reduce((n, f) => n + (countLines(snapshot.read(f)) ?? 0), 0);
  const evidence = `対象文書 ${docs.length} 件 / rules ${snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)).length} 件 / skills ${snapshot.files.filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f)).length} 件 / 恒久規範 常時ロード ${always}/${LINE_BUDGET.alwaysLoaded} 行・rules ${ruleLines}/${LINE_BUDGET.rules} 行`;
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
    console.log(`governance:check — G1..G10 passed（${evidence}）`);
  }
}
