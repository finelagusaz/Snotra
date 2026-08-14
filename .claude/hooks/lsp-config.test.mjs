/**
 * `checkLspConfig` のカナリア。
 *
 * 2 層に分けてある。(1) 実リポジトリを指す薄いテスト 1 本——設定が現に生きていることを言う。
 * (2) temp へ複製した木への故障注入——不変条件の**足ごと**に壊して、実際に赤くなることを測る
 * （`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）。
 *
 * **変異は必ず複製に当てる。** 稼働中の `.claude/settings.json` は hook の配線そのものを運ぶ live な
 * ガードで、file watcher が即座に拾う（同ファイル「稼働中のガードを弱めない——複製に変異を当てる」）。
 */

import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { checkLspConfig } from "./lsp-config.mjs";

const REPO_ROOT = path.resolve(import.meta.dirname, "..", "..");

/** 判定が読むファイルだけを temp へ複製する。実リポジトリと同じ相対配置を保つ。 */
const COPIED = [
  ".claude/settings.json",
  ".claude/lsp/.claude-plugin/marketplace.json",
  ".claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json",
  ".claude/lsp/snotra-rust-lsp/.lsp.json",
  "rust-analyzer.toml",
];

function materialize() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "snotra-lsp-"));
  for (const rel of COPIED) {
    const dest = path.join(dir, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(path.join(REPO_ROOT, rel), dest);
  }
  return dir;
}

const readJson = (dir, rel) => JSON.parse(fs.readFileSync(path.join(dir, rel), "utf-8"));
const writeJson = (dir, rel, value) =>
  fs.writeFileSync(path.join(dir, rel), `${JSON.stringify(value, null, 2)}\n`);

const LSP_JSON = ".claude/lsp/snotra-rust-lsp/.lsp.json";
const PLUGIN_JSON = ".claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json";
const MARKETPLACE_JSON = ".claude/lsp/.claude-plugin/marketplace.json";
const SETTINGS_JSON = ".claude/settings.json";

/** 複製に変異を当てて違反の一覧を返す。`mutate` は複製の絶対パスを受け取る。 */
function violationsAfter(mutate) {
  const dir = materialize();
  try {
    mutate(dir);
    return checkLspConfig(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

describe("checkLspConfig — 実リポジトリ", () => {
  it("現在のツリーの LSP 設定は不変条件をすべて満たす", () => {
    expect(checkLspConfig(REPO_ROOT)).toEqual([]);
  });
});

describe("checkLspConfig — 故障注入（複製に当てる）", () => {
  // 変異を当てない複製が緑であることを先に測る。これが無いと、以下の赤が「変異のせい」なのか
  // 「複製の作り方のせい」なのか区別できない（#872: 変異が本来の回帰より強いと、検査が縛れて
  // いなくても赤くなる）。
  it("無変異の複製は緑（発火しない向きの検算）", () => {
    expect(violationsAfter(() => {})).toEqual([]);
  });

  it("足 3a — initializationOptions.checkOnSave が消えると赤", () => {
    const v = violationsAfter((dir) => {
      const lsp = readJson(dir, LSP_JSON);
      delete lsp["rust-analyzer"].initializationOptions.checkOnSave;
      writeJson(dir, LSP_JSON, lsp);
    });
    expect(v.join("\n")).toMatch(/checkOnSave/);
  });

  it("足 3b — initializationOptions.workspace.symbol.search が消えると赤", () => {
    const v = violationsAfter((dir) => {
      const lsp = readJson(dir, LSP_JSON);
      delete lsp["rust-analyzer"].initializationOptions.workspace;
      writeJson(dir, LSP_JSON, lsp);
    });
    expect(v.join("\n")).toMatch(/workspace\.symbol\.search/);
  });

  it("足 1 — .lsp.json が JSON として壊れると赤", () => {
    const v = violationsAfter((dir) => {
      fs.writeFileSync(path.join(dir, LSP_JSON), '{ "rust-analyzer": { broken\n');
    });
    expect(v.join("\n")).toMatch(/\.lsp\.json を読めない/);
  });

  it("足 2 — extensionToLanguage から .rs が消えると赤", () => {
    const v = violationsAfter((dir) => {
      const lsp = readJson(dir, LSP_JSON);
      delete lsp["rust-analyzer"].extensionToLanguage[".rs"];
      writeJson(dir, LSP_JSON, lsp);
    });
    expect(v.join("\n")).toMatch(/extensionToLanguage/);
  });

  it("足 4 — 公式 plugin が true に戻ると赤（.rs に LSP が 2 つ）", () => {
    const v = violationsAfter((dir) => {
      const settings = readJson(dir, SETTINGS_JSON);
      settings.enabledPlugins["rust-analyzer-lsp@claude-plugins-official"] = true;
      writeJson(dir, SETTINGS_JSON, settings);
    });
    expect(v.join("\n")).toMatch(/rust-analyzer-lsp@claude-plugins-official/);
  });

  // 足 4 は枝が 2 本ある。片方（公式を戻す）だけ測ると、判定が真偽を取り違えていても——
  // つまり「自分の plugin が false・公式が true」を要求していても——検査は緑になりうる。
  it("足 4b — 自分の plugin が false になると赤", () => {
    const v = violationsAfter((dir) => {
      const settings = readJson(dir, SETTINGS_JSON);
      settings.enabledPlugins["snotra-rust-lsp@snotra"] = false;
      writeJson(dir, SETTINGS_JSON, settings);
    });
    expect(v.join("\n")).toMatch(/enabledPlugins\["snotra-rust-lsp@snotra"\] が true でない/);
  });

  it("足 5 — 配送だけが死んでも赤（.lsp.json は無傷）", () => {
    const v = violationsAfter((dir) => {
      const settings = readJson(dir, SETTINGS_JSON);
      settings.extraKnownMarketplaces.snotra.source.path = "./.claude/does-not-exist";
      writeJson(dir, SETTINGS_JSON, settings);
    });
    expect(v.join("\n")).toMatch(/marketplace\.json を読めない/);
  });

  // 足 5 は path だけでなく source 種別も見ている。path の変異は「marketplace.json を読めない」で
  // 赤くなるため、**extraKnownMarketplaces 検査そのものは誰も縛っていなかった**（code-reviewer が
  // 各検査を `if (false)` で無力化して実測・14 中 3 件が素通り）。
  it("足 5b — source 種別だけがずれても赤（path は無傷）", () => {
    const v = violationsAfter((dir) => {
      const settings = readJson(dir, SETTINGS_JSON);
      settings.extraKnownMarketplaces.snotra.source.source = "file";
      writeJson(dir, SETTINGS_JSON, settings);
    });
    expect(v.join("\n")).toMatch(/extraKnownMarketplaces\.snotra\.source が/);
  });

  it("足 5c — plugin.json が JSON として壊れると赤", () => {
    const v = violationsAfter((dir) => {
      fs.writeFileSync(path.join(dir, PLUGIN_JSON), '{ "name": broken\n');
    });
    expect(v.join("\n")).toMatch(/plugin\.json を読めない/);
  });

  it("足 7c — marketplace entry の name がずれると赤（entry 実在検査を縛る）", () => {
    const v = violationsAfter((dir) => {
      const marketplace = readJson(dir, MARKETPLACE_JSON);
      marketplace.plugins[0].name = "snotra-rust-lsp-renamed";
      writeJson(dir, MARKETPLACE_JSON, marketplace);
    });
    expect(v.join("\n")).toMatch(/plugin entry "snotra-rust-lsp" が無い/);
  });

  // 「.rs を宣言する LSP サーバは常にちょうど 1 つ」を担う検査。二重宣言は Claude Code 側では
  // 先勝ち + warn ＝ 沈黙に近い失敗なので、ここで縛らないと誰も見ない。
  it("足 9 — .lsp.json のサーバが 2 つになると赤", () => {
    const v = violationsAfter((dir) => {
      const lsp = readJson(dir, LSP_JSON);
      lsp["other-ls"] = { command: "other-ls", extensionToLanguage: { ".rs": "rust" } };
      writeJson(dir, LSP_JSON, lsp);
    });
    expect(v.join("\n")).toMatch(/サーバがちょうど 1 つでない/);
  });

  it("足 6 — plugin.json の lspServers が .lsp.json を上書きしうる形になると赤", () => {
    const v = violationsAfter((dir) => {
      const manifest = readJson(dir, PLUGIN_JSON);
      manifest.lspServers = { "rust-analyzer": { command: "rust-analyzer" } };
      writeJson(dir, PLUGIN_JSON, manifest);
    });
    expect(v.join("\n")).toMatch(/plugin\.json に lspServers/);
  });

  it("足 6 — marketplace entry の lspServers も赤", () => {
    const v = violationsAfter((dir) => {
      const marketplace = readJson(dir, MARKETPLACE_JSON);
      marketplace.plugins[0].lspServers = { "rust-analyzer": { command: "rust-analyzer" } };
      writeJson(dir, MARKETPLACE_JSON, marketplace);
    });
    expect(v.join("\n")).toMatch(/marketplace entry に lspServers/);
  });

  it("足 7 — marketplace 名がキーとずれると赤（沈黙で load されない形）", () => {
    const v = violationsAfter((dir) => {
      const marketplace = readJson(dir, MARKETPLACE_JSON);
      marketplace.name = "snotra-renamed";
      writeJson(dir, MARKETPLACE_JSON, marketplace);
    });
    expect(v.join("\n")).toMatch(/name が extraKnownMarketplaces のキーと一致しない/);
  });

  // 足 7 も枝が 2 本ある（marketplace 名と plugin 名）。plugin 名のずれは entry からも
  // `enabledPlugins` のキー前半からも外れるので、同じく沈黙して load されない。
  it("足 7b — plugin.json の name が entry とずれると赤", () => {
    const v = violationsAfter((dir) => {
      const manifest = readJson(dir, PLUGIN_JSON);
      manifest.name = "snotra-rust-lsp-renamed";
      writeJson(dir, PLUGIN_JSON, manifest);
    });
    expect(v.join("\n")).toMatch(/plugin\.json の name が entry と一致しない/);
  });

  it("足 8 — rust-analyzer.toml が checkOnSave を書くと赤（ratoml が client を上書きする）", () => {
    const v = violationsAfter((dir) => {
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), "\ncheckOnSave = false\n");
    });
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に checkOnSave/);
  });

  it("足 8 — rust-analyzer.toml が diagnostics を書いても赤", () => {
    const v = violationsAfter((dir) => {
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), "\n[diagnostics]\nenable = false\n");
    });
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に diagnostics/);
  });

  // 引用キーは TOML として正当なので、素の識別子だけを見る判定では素通りする（偽陰性）。
  it("足 8b — ratoml が引用キーで書いても赤", () => {
    const v = violationsAfter((dir) => {
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), '\n"checkOnSave" = false\n');
    });
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に checkOnSave/);
  });

  // crate 直下の ratoml も local 水準で効く。発火（basename アンカー）と判定の母集団を
  // 揃えていないと、hook が走ったのに別のファイルを見て緑になる。
  it("足 8c — crate 直下の ratoml も見る", () => {
    const v = violationsAfter((dir) => {
      const crate = path.join(dir, "snotra-core");
      fs.mkdirSync(crate, { recursive: true });
      fs.writeFileSync(path.join(crate, "rust-analyzer.toml"), "[diagnostics]\nenable = false\n");
    });
    expect(v.join("\n")).toMatch(/snotra-core\/rust-analyzer\.toml に diagnostics/);
  });

  // ratoml 検査は配送経路のどれにも依存しない。連鎖側が早期 return しても道連れにならないこと
  // （多重故障のとき報告から消えないこと）を測る。
  it("配送が死んでいても ratoml 違反は報告される（独立性）", () => {
    const v = violationsAfter((dir) => {
      fs.writeFileSync(path.join(dir, LSP_JSON), '{ "rust-analyzer": { broken\n');
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), "\ncheckOnSave = false\n");
    });
    expect(v.join("\n")).toMatch(/\.lsp\.json を読めない/);
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に checkOnSave/);
  });

  // 独立性の経路は 2 本ある。`.lsp.json` 破損（上）と settings.json 破損（下）で、後者は
  // ratoml 検査を前へ出したことで**初めて生きた**経路である（当該行は 1 文字も変えていない）。
  it("settings.json が壊れていても ratoml 違反は報告される（独立性・第 2 経路）", () => {
    const v = violationsAfter((dir) => {
      fs.writeFileSync(path.join(dir, SETTINGS_JSON), "{ broken\n");
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), "\ncheckOnSave = false\n");
    });
    expect(v.join("\n")).toMatch(/settings\.json を読めない/);
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に checkOnSave/);
  });

  // TOML はリテラル文字列もキーに使える。`"?` だけでは枝の片方しか閉じない。
  it("足 8b2 — ratoml が単一引用のキーで書いても赤", () => {
    const v = violationsAfter((dir) => {
      fs.appendFileSync(path.join(dir, "rust-analyzer.toml"), "\n'checkOnSave' = false\n");
    });
    expect(v.join("\n")).toMatch(/rust-analyzer\.toml に checkOnSave/);
  });

  // 行末コメントの除去は**偽陽性を防ぐ側**の挙動なので、赤の変異では縛れない。
  // 「赤くならないこと」を測る 1 本を置かないと、次に誰かが戻しても何も落ちない。
  it("足 8d — キーを論じる行末コメントでは赤くならない（発火しない向きの検算）", () => {
    const v = violationsAfter((dir) => {
      fs.appendFileSync(
        path.join(dir, "rust-analyzer.toml"),
        "\nfoo = 1 # rust-analyzer.checkOnSave は書かない側\n",
      );
    });
    expect(v).toEqual([]);
  });

  // ratoml が 1 枚も無ければ禁止キー検査は空虚に真になる。検知器が守る対象を失ったことを、
  // 検知器自身が言えなければならない。
  it("足 8e — rust-analyzer.toml が消えると赤（空虚な真を防ぐ）", () => {
    const v = violationsAfter((dir) => {
      fs.rmSync(path.join(dir, "rust-analyzer.toml"));
    });
    expect(v.join("\n")).toMatch(/リポジトリ直下の rust-analyzer\.toml が無い/);
  });

  // 「ツリーに 1 枚でもあればよい」にすると、設定を crate 側へ移す変更で root が消えても緑になる。
  // 空虚な真は 1 段内側にも作れる——問うのは所在であって存在ではない。
  it("足 8e2 — root が消えて crate 直下だけ残っても赤", () => {
    const v = violationsAfter((dir) => {
      fs.rmSync(path.join(dir, "rust-analyzer.toml"));
      const crate = path.join(dir, "snotra-core");
      fs.mkdirSync(crate, { recursive: true });
      fs.writeFileSync(path.join(crate, "rust-analyzer.toml"), "[cargo]\ntargetDir = true\n");
    });
    expect(v.join("\n")).toMatch(/リポジトリ直下の rust-analyzer\.toml が無い/);
  });

  it("settings を書くと赤（initializationOptions 以外の設定経路が開く）", () => {
    const v = violationsAfter((dir) => {
      const lsp = readJson(dir, LSP_JSON);
      lsp["rust-analyzer"].settings = { "rust-analyzer": { checkOnSave: true } };
      writeJson(dir, LSP_JSON, lsp);
    });
    expect(v.join("\n")).toMatch(/settings を書いてはならない/);
  });
});
