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
// G1 — 各サブディレクトリ CLAUDE.md「モジュール構成」↔ 実ファイルの双方向照合。
// basename 包含方式: ディレクトリ集約行（`commands/` のベア名列挙）・`tabs/` プレフィックス省略・
// 1 行複数バッククォートをパースせずに済ませる意図的な弱化（wrong-directory 検出は放棄）。
// ---------------------------------------------------------------------------
// ui は #532 SU7 のフロント撤去で消滅（ui/CLAUDE.md ごと削除）
// snotra-egui-runtime は #701 で追加。「#532 の検証層」として作られたまま母集団から漏れており、
// SU7 で製品の描画層になった後も更新されていなかった（G3 の governanceDocs も同時に是正）
export const G1_CRATES = {
  "snotra-core": { src: "snotra-core/src/", exts: /\.rs$/ },
  "snotra-egui-runtime": { src: "snotra-egui-runtime/src/", exts: /\.rs$/ },
  "src-tauri": { src: "src-tauri/src/", exts: /\.rs$/ },
  "snotra-settings": { src: "snotra-settings/src/", exts: /\.rs$/ },
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

const SKILL_FILE_RE = /^\.claude\/skills\/[^/]+\/SKILL\.md$/;

/** `.claude/skills/<name>/SKILL.md` の一覧（G8・G10 の共通母集団） */
function skillFiles(snapshot) {
  return snapshot.files.filter((f) => SKILL_FILE_RE.test(f));
}

/**
 * `disable-model-invocation: true` の skill 名の集合 = **harness の roster に注入されない skill**。
 * G8（表が索引すべき対象）と G10（常時ロード面に載る description）の両方がこの集合で決まるため、
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
// G8 — ルート CLAUDE.md「利用できるスキル」表 ↔ roster に載らない skill（旧 Check 9）
// harness は毎セッション skill roster を description 付きで注入するため、注入される skill を
// 表へ書き写すことは同じ面での二重課税である（ADR-0005 が description を常時ロード面に算入した
// のと同じ理由）。ゆえに表が索引すべき対象は `disable-model-invocation: true` の skill だけであり、
// G8 はその集合と表の**双方向**一致を見る。「表の射程」を規範ではなくこの判定で固定する。
// ---------------------------------------------------------------------------
export function checkSkillTable(snapshot) {
  const findings = [];
  const text = snapshot.read("CLAUDE.md");
  if (text == null) return [finding("CLAUDE.md", 1, "ルート CLAUDE.md が読めない")];
  const section = text.split(/^## 利用できるスキル$/m)[1]?.split(/^## /m)[0] ?? "";
  const inTable = new Set([...section.matchAll(/^\|\s*`\/([a-z0-9-]+)`/gm)].map((m) => m[1]));
  const inDirs = new Set(skillFiles(snapshot).map((f) => f.split("/")[2]));
  const hidden = modelHiddenSkills(snapshot);
  if (inDirs.size === 0) findings.push(finding(".claude/skills", 1, "SKILL.md が 0 件（G8 母集団の欠落）"));
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
// G10 — 恒久規範の面積 ratchet（二面独立）。#593 §2 ・ADR-0001 ・ADR-0005。
// 恒久規範（読むかを選べずロードされる = コンテキスト予算への課税）の面積を単調非増加に保つ。
// 「常時ロード」と「rules」を独立の上限で見るのは、常時→rules の面替えだけで数字を下げる回避を
// 塞ぐため（合計 ratchet なら総額不変で通ってしまう）。基準の引き上げは AREA_BUDGET を理由コメント
// 付きで更新すること（= 明示的な合意の摩擦）。
// 指標は**文字数（コードポイント・CR 除く）**である。行数だと「改行を消す」が最も安い削減手段になり、
// 読む量を 1 文字も減らさずに数字だけ下げられた（ADR-0005 に実測）。CR を除くのは CRLF checkout が
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
 * 行数時代の推移: #616 で 233→228、#488 マージ節を ADR-0002 へ退去し 228→222、フック節の実装契約を
 * `docs/hooks.md` へ退去し 222→216、2026-07-26 に (A2) の 3 理由退去・コミュニケーション原則の圧縮・
 * 重複規則の除去で 216→191 行。ADR-0005 で指標を文字数へ切り替えた。
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
 * 定数の書き換えを要求して摩擦が日常化し、赤の意味が失われる（ADR-0005）。
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
 *       description を算入していた。ADR-0005 の算入根拠（毎セッション注入される）を満たさない。
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
 *   G11 のアンカーで、`.claude/rules/src-tauri.md` や `/health-check` が中身を当てにして指しており、
 *   見出しを残して中身だけ移すと G11 は緑のまま参照先が空洞になる（`.claude/rules/governance-docs.md`）。
 * **2026-07-27 rules のみ引き下げ 8328→8056**（実測 rules 8228→7956・常時ロードは 13958→14036 で**上昇**）:
 *   `.claude/rules/safety-nets.md` の敵対的読者節を `/norm-review` skill へ移した。これは削減ではなく
 *   **面替え**である——手順の本体は非課税の skill 本文へ、skill の `description` 78 字は常時ロード面へ載る。
 *   ADR-0001 が二面独立にしたのはまさにこの移動を可視にするためなので、**常時ロードの基準は上げない**。
 *   結果として常時ロード面の余裕は 22 字しか残っていない——次に規範を 1 本足すときは明示的な引き上げが要る。
 * **2026-07-27 引き下げ 14058→13374（#768）**（実測 常時ロード 14036→13274・**-762 字**）:
 *   コマンドの形で判定できる規範 5 件（bash HEREDOC・`\` パス区切り・Python の `PYTHONIOENCODING`・
 *   `--no-verify` 人間専用・`git pull --ff-only`）を `.claude/hooks/pre-bash.mjs` の判定へ吸収した。
 *   これは面替えではなく**機構への吸収**である（#593 の階梯）——「シェル環境」節と git の 2 bullet が消え、
 *   事故の理由と代替手段は拒否文言（`SHAPE_REMEDY`）が実行時に運ぶ。Windows 機で 5 件のライブ実測を行い、
 *   5/5 が復帰手順付きで `exit 2` になることを確認した。判定の詳細は `docs/hooks.md`、
 *   注入面の否定の知識は `docs/adr/0009-command-shape-norms-in-hook.md`。
 *   rules 面は本 PR で `.claude/rules/**` を触らないため据え置き。
 */
export const AREA_BUDGET = { alwaysLoaded: 13374, rules: 8056 };

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
 * **`disable-model-invocation: true` の skill は除く** — ADR-0005 が description を常時ロード面へ
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
      findings.push(finding(f, 1, `${f} が読めない（G10 母集団の欠落）`));
      continue;
    }
    const m = text.match(/^description:[ \t]*(.*)$/m);
    const v = m ? m[1].trim() : "";
    if (!m || v === "" || v.startsWith("|") || v.startsWith(">")) {
      findings.push(finding(f, 1, "description が 1 行スカラーでない（G10 が面積を数えられない）"));
      continue;
    }
    total += [...v.replace(/^["']/, "").replace(/["']$/, "")].length;
  }
  return { total, findings, count: all.length };
}

export function checkNormativeAreaBudget(snapshot) {
  const findings = [];

  const docs = sumChars(snapshot, ALWAYS_LOADED_FILES, "G10");
  const desc = skillDescriptionArea(snapshot);
  findings.push(...docs.findings, ...desc.findings);
  if (desc.count === 0) findings.push(finding(".claude/skills", 1, "skills が 0 件（G10 母集団の欠落）"));
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
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G10 母集団の欠落）"));
  } else {
    const rules = sumChars(snapshot, ruleFiles, "G10");
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
    (sumChars(snapshot, ALWAYS_LOADED_FILES, "G10").total ?? 0) + skillDescriptionArea(snapshot).total;
  const rules = sumChars(
    snapshot,
    snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)),
    "G10",
  ).total;
  return { always, rules };
}

// ---------------------------------------------------------------------------
// G11 — 見出し参照の実在（正準形 `<対象>`「<見出し>」）。
// 参照に構文を与えて機械照合可能にする。これが `.claude/rules/governance-docs.md` の
// 「改変前に参照側を名前と序数で数え上げる」手作業を置き換える機構である。
// アンカーは ATX 見出し・番号付きリスト項目・太字リードの 3 種（この repo の参照実態に合わせた。
// 節ではなく箇条書きのリード文を指す参照が実在する: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。
// 照合は正規化（`**`・バッククォート・「」・空白の除去）後の**前方一致**——見出しが後置の
// 括弧注記（「…の不変条件（#532 SU5）」「条件別チェック（トリガー → 参照先）」）を持つため。
// 受容する偽陰性: 「ルート `CLAUDE.md` のフック節」のような散文形の参照は見ない。
// 検査されるのは正準形に書かれたものだけであり、**この検査は規範の完全な代替ではない**。
// ---------------------------------------------------------------------------

/** 見出し参照の正準形。対象は `<path>.md` か `/skill-name` */
const HEADING_REF = /`([^`\n]+)`\s*(?:§\s*)?「([^「」\n]+)」/g;

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
function resolveRefTarget(snapshot, doc, target) {
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
      findings.push(finding(doc, 1, "対象文書が読めない（G11 母集団の欠落）"));
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
// 実行
// ---------------------------------------------------------------------------

/** G3/G4 の対象文書（ガバナンス文書群）。docs/superpowers/ は歴史資料（#589 で非規範化）ゆえ除外 */
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
 * G11 の対象。見出し参照はガバナンス文書の外（`PERFORMANCE.md`・`.claude/agents/`）にも書かれ、
 * 実際にそこで腐っていた（`src-tauri/CLAUDE.md`「TrySuspend / Resume パターン」）ため母集団を広く取る。
 * 除外は履歴資料（`docs/superpowers/`）と作業バッファ（`workspace/`・`/implement` が削除する）のみ。
 */
export function headingRefDocs(snapshot) {
  return snapshot.files.filter(
    (f) => f.endsWith(".md") && !f.startsWith("docs/superpowers/") && !f.startsWith("workspace/"),
  );
}

export function runAll(snapshot) {
  const docs = governanceDocs(snapshot);
  const refDocs = headingRefDocs(snapshot);
  const findings = [];
  if (docs.length === 0) findings.push(finding(".", 1, "ガバナンス文書が 0 件（母集団の欠落）"));
  if (refDocs.length === 0) findings.push(finding(".", 1, "G11 の対象 md が 0 件（母集団の欠落）"));
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
    ...checkNormativeAreaBudget(snapshot),
  );
  const headingRefs = scanHeadingRefs(snapshot, refDocs);
  findings.push(...headingRefs.findings);
  const area = normativeArea(snapshot);
  const evidence = `対象文書 ${docs.length} 件 / rules ${snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)).length} 件 / skills ${snapshot.files.filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f)).length} 件 / 恒久規範 常時ロード ${area.always}/${AREA_BUDGET.alwaysLoaded} 字・rules ${area.rules}/${AREA_BUDGET.rules} 字 / 見出し参照 ${headingRefs.checked} 件を ${refDocs.length} 文書から照合`;
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
    console.log(`governance:check — G1..G11 passed（${evidence}）`);
  }
}
