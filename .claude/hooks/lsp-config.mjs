/**
 * Claude Code の rust-analyzer インスタンスへ渡す設定（project-scope LSP plugin）の不変条件を判定する。
 *
 * **なぜ自前の検知器が要るか。** `claude plugin validate --strict` は `.lsp.json` を視界に入れない——
 * JSON として壊しても抑制キーを消しても exit 0 を返す（2026-08-14 実測・#1083）。
 * そのうえ**設定が届かない・上書きされる壊れ方は沈黙する**——rust-analyzer は設定が無ければ既定値で
 * 普通に起動するので、navigation は動いたまま `checkOnSave` だけが復活する。壊れ方の分類と、
 * 沈黙しない枝（plugin の load 自体が失敗する形）は `docs/hooks.md`
 * 「Claude Code の RA インスタンスと hook の分担」が正本。
 *
 * **判定を非 test の `.mjs` に置くのは 2 つの理由による。** (1) 稼働中の `.claude/settings.json` へ
 * 変異を当てずに複製へ当てられる
 * （`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。
 * (2) `G-stale-identifiers` の語彙源になる——`VOCAB_SOURCE_EXT` に `.json` は入らず
 * `VOCAB_TEST_FILE` が `*.test.mjs` を外すため、設定キー名を test にだけ書くと、
 * 文書へ同じ名前を書いた瞬間に `governance:check` が赤くなる（`scripts/governance-check.mjs`）。
 * ゆえに**キー名は文字列リテラルとしてこのファイルに現れる必要がある**（コメントは語彙にならない）。
 */

import fs from "node:fs";
import path from "node:path";

import { toPosixPath } from "./post-edit.mjs";

/** marketplace 名。`extraKnownMarketplaces` のキーと `marketplace.json` の `name` が一致していないと
 *  Claude Code は plugin を load せず、しかもエラーは debug log にしか出ない（バイナリ逐語:
 *  "Marketplace name. Must match the extraKnownMarketplaces key (enforced)"）。 */
const MARKETPLACE = "snotra";
/** plugin 名。`enabledPlugins` のキーは `<plugin>@<marketplace>` の形を取る。 */
const PLUGIN = "snotra-rust-lsp";
/** marketplace root への相対パス。読み手は `path.resolve()`（cwd 基準）で解決するため、
 *  絶対パスを書くと他マシン・worktree で壊れる。`claude plugin marketplace add` は
 *  入力を `path.resolve` して絶対パスで書き込むので使わない。 */
const MARKETPLACE_PATH = "./.claude/lsp";
/** 二重に `.rs` を掴ませない相手。二重時の挙動は先勝ち + warn ＝ 沈黙に近い失敗である。 */
const OFFICIAL_PLUGIN = "rust-analyzer-lsp@claude-plugins-official";

/** `rust-analyzer.toml` へ書かれると plugin の設定を黙って上書きするキー。workspace / local 水準は
 *  「リポジトリ直下の ratoml → クライアント設定」の順で解決されるため ratoml が勝ち、
 *  しかも VS Code 側にも波及する（#1082 で水準を実測）。 */
const RATOML_FORBIDDEN = ["checkOnSave", "diagnostics"];

/** ratoml の走査から外すディレクトリ。生成物と他ツリーを持ち込まない。 */
const WALK_EXCLUDE = new Set([".git", "node_modules", "target", "dist", "worktrees"]);

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf-8"));

/**
 * ツリー内の `rust-analyzer.toml` を**すべて**返す（リポジトリ直下の 1 枚に限らない）。
 *
 * `diagnostics.*` は local 水準で、解決は「source root を遡る ratoml 群 → クライアント設定」の順
 * である（#1082 で水準を実測）。ゆえに crate 直下へ置かれた ratoml も plugin の設定を上書きしうる。
 * **発火側（`selectChecks` の basename アンカー）と母集団を揃えないと、hook が走ったのに
 * 判定は別のファイルを見ている＝緑になる**——割り当てられたファイルの緑は「合格」を意味するので、
 * 沈黙より悪い。
 *
 * 除外は**名前一致・全階層**であってパス固定ではない（`crates/dist/` のような名前の crate が
 * できれば取りこぼす）。`target` を全階層で外すのは cargo の入れ子 target のため。
 * `readdirSync` が権限エラーや壊れた junction で throw する経路は**受容する**——vitest が
 * エラーで落ちるので沈黙せず、fail-closed の向きである（続行させると読めない枝を黙って飛ばす）。
 */
export function findRatomlFiles(rootDir) {
  const found = [];
  const walk = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (e.isDirectory()) {
        if (!WALK_EXCLUDE.has(e.name)) walk(path.join(dir, e.name));
      } else if (e.name === "rust-analyzer.toml") {
        found.push(path.join(dir, e.name));
      }
    }
  };
  walk(rootDir);
  return found;
}

/**
 * `rootDir` のツリーが持つ LSP 設定の不変条件を検査し、違反の一覧を返す（空配列 = 合格）。
 *
 * root を引数で受けるのは、実リポジトリと「temp へ複製した木」の両方を同じ判定に掛けるためである。
 * 故障注入は複製にだけ当てる。
 */
export function checkLspConfig(rootDir) {
  const violations = [];
  const at = (...parts) => path.join(rootDir, ...parts);

  // --- ratoml がクライアント設定を上書きしていないか ---
  //
  // **配送経路の検査より前に置く。** この検査は下の JSON 連鎖のどれにも依存しない（読むのは
  // ratoml だけ）のに、後ろへ置くと連鎖側の早期 return に道連れにされ、多重故障のとき報告から
  // 消える。打ち切ってよいのは「前段が決まらないと後段を検査できない」依存がある場合だけである。
  const ratomlFiles = findRatomlFiles(rootDir);
  // **問うのは「リポジトリ直下の 1 枚が在るか」であって「ツリーに 1 枚でも在るか」ではない。**
  // 後者にすると、設定を crate 側へ移す変更で root が消えても緑になり、workspace 水準の設定
  // （`workspace.symbol.search` / `cargo.targetDir`）が VS Code から黙って失われる。
  // 0 件だと禁止キー検査が**空虚に真**になる、という同じ型の穴が 1 段内側に残る形である
  // （旧実装は root の 1 枚を必ず読んでいたので、削除は読み取り失敗として赤くなっていた）。
  if (!ratomlFiles.some((f) => path.dirname(f) === path.resolve(rootDir))) {
    violations.push(
      "リポジトリ直下の rust-analyzer.toml が無い — 禁止キー検査が空虚に真になり、" +
        "VS Code 側の workspace 水準の設定も失われる",
    );
  }
  for (const file of ratomlFiles) {
    const rel = toPosixPath(path.relative(rootDir, file));
    // 行頭コメントも行末コメントも落とす（`scripts/governance-check.mjs` の toml 処理と同じ扱い）。
    // 落とさないと、当該キーを**論じている**コメントで赤くなる——この差分自身がその形である。
    const body = fs.readFileSync(file, "utf-8").replace(/#.*$/gm, " ");
    for (const key of RATOML_FORBIDDEN) {
      // `["']?` は TOML の引用キーを捕まえるため。基本文字列（`"checkOnSave"`）も
      // リテラル文字列（`'checkOnSave'`）もキーに使える正当な構文である。
      if (new RegExp(`(^|\\.|\\[)\\s*["']?${key}\\b`, "m").test(body)) {
        violations.push(
          `${rel} に ${key} が書かれている — ratoml はクライアント設定より優先されるので` +
            " plugin の initializationOptions が黙って無効化され、しかも VS Code 側にも掛かる",
        );
      }
    }
  }

  // --- 配送経路: settings.json → marketplace → plugin ---
  let settings;
  try {
    settings = readJson(at(".claude", "settings.json"));
  } catch (e) {
    // `[...violations, …]` の形を崩さない——ratoml 検査は既に走っており、その結果を捨てると
    // 多重故障のとき報告から消える。**この 1 本は ratoml を前へ出したことで初めて生きた経路**で、
    // 1 文字も変えていないのに意味が変わった（新しく生きた組み合わせは既存の検査から漏れる）。
    return [...violations, `.claude/settings.json を読めない: ${e.message}`];
  }

  const declared = settings.extraKnownMarketplaces?.[MARKETPLACE]?.source;
  if (declared?.source !== "directory" || declared?.path !== MARKETPLACE_PATH) {
    violations.push(
      `extraKnownMarketplaces.${MARKETPLACE}.source が {source:"directory", path:"${MARKETPLACE_PATH}"} ではない: ${JSON.stringify(declared)}`,
    );
  }

  const enabled = settings.enabledPlugins ?? {};
  const pluginId = `${PLUGIN}@${MARKETPLACE}`;
  if (enabled[pluginId] !== true) {
    violations.push(`enabledPlugins["${pluginId}"] が true でない: ${JSON.stringify(enabled[pluginId])}`);
  }
  if (enabled[OFFICIAL_PLUGIN] !== false) {
    violations.push(
      `enabledPlugins["${OFFICIAL_PLUGIN}"] が false でない: ${JSON.stringify(enabled[OFFICIAL_PLUGIN])}` +
        " — .rs を宣言する LSP サーバが 2 つになる（先勝ち + warn ＝ 沈黙に近い失敗）",
    );
  }

  // 宣言された path の先に marketplace が実在するか。ここを見ないと、`.lsp.json` は正しいのに
  // 配送だけが死んでいる形を素通りする。
  const marketplaceRoot = path.resolve(rootDir, declared?.path ?? MARKETPLACE_PATH);
  let marketplace;
  try {
    marketplace = readJson(path.join(marketplaceRoot, ".claude-plugin", "marketplace.json"));
  } catch (e) {
    return [...violations, `marketplace.json を読めない (${marketplaceRoot}): ${e.message}`];
  }

  if (marketplace.name !== MARKETPLACE) {
    violations.push(
      `marketplace.json の name が extraKnownMarketplaces のキーと一致しない: ${JSON.stringify(marketplace.name)} ≠ "${MARKETPLACE}"`,
    );
  }

  const entry = (marketplace.plugins ?? []).find((p) => p.name === PLUGIN);
  if (!entry) {
    return [...violations, `marketplace.json に plugin entry "${PLUGIN}" が無い`];
  }
  if (entry.lspServers !== undefined) {
    violations.push(
      "marketplace entry に lspServers が書かれている — .lsp.json は 3 つの宣言箇所のうち最も弱く、黙って負ける",
    );
  }

  const pluginRoot = path.resolve(marketplaceRoot, entry.source);
  let manifest;
  try {
    manifest = readJson(path.join(pluginRoot, ".claude-plugin", "plugin.json"));
  } catch (e) {
    return [...violations, `plugin.json を読めない (${pluginRoot}): ${e.message}`];
  }
  if (manifest.name !== PLUGIN) {
    violations.push(`plugin.json の name が entry と一致しない: ${JSON.stringify(manifest.name)} ≠ "${PLUGIN}"`);
  }
  if (manifest.lspServers !== undefined) {
    violations.push(
      "plugin.json に lspServers が書かれている — .lsp.json を読んだ後に Object.assign で上書きされる",
    );
  }

  // --- 設定の中身 ---
  let lsp;
  try {
    lsp = readJson(path.join(pluginRoot, ".lsp.json"));
  } catch (e) {
    return [...violations, `.lsp.json を読めない (${pluginRoot}): ${e.message}`];
  }

  const names = Object.keys(lsp);
  if (names.length !== 1) {
    return [...violations, `.lsp.json が宣言するサーバがちょうど 1 つでない: ${JSON.stringify(names)}`];
  }
  const server = lsp[names[0]];

  if (server.extensionToLanguage?.[".rs"] !== "rust") {
    violations.push(
      `extensionToLanguage が .rs → rust を持たない: ${JSON.stringify(server.extensionToLanguage)}`,
    );
  }
  if (server.settings !== undefined) {
    violations.push(
      "settings を書いてはならない — workspace/configuration capability が有効になり、" +
        "initializationOptions 以外の設定経路が開いて決定論性が落ちる",
    );
  }

  const init = server.initializationOptions ?? {};
  if (init.checkOnSave !== false) {
    violations.push(
      `initializationOptions.checkOnSave が false でない: ${JSON.stringify(init.checkOnSave)}` +
        " — flycheck が復活し、確定判定を担う PostToolUse hook の cargo と target/ を食い合う",
    );
  }
  const search = init.workspace?.symbol?.search;
  if (search?.kind !== "all_symbols" || search?.limit !== 512) {
    violations.push(
      `initializationOptions.workspace.symbol.search が {kind:"all_symbols", limit:512} でない: ${JSON.stringify(search)}` +
        " — 既定の only_types / limit=128 は、網羅性が要件の作業で沈黙の打ち切りになる",
    );
  }

  return violations;
}
