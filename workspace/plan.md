# 実装計画: Claude Code 用 rust-analyzer を project-local LSP plugin に分離する（#1083）

調査は `workspace/research.md`。以下の設計判断はすべてそこの一次証拠（Claude Code 2.1.232 バイナリの zod スキーマ・公式 docs・スクラッチでの変異注入）に基づく。

## 目的

Claude Code が起動する rust-analyzer（RA）の設定を **リポジトリが所有する project-scope LSP plugin** で与え、VS Code 側の RA を巻き込まずに `checkOnSave` を切る。検証の確定判定は `post-edit.mjs` の fmt / clippy / crate test に残したまま変えない。あわせて `rust-analyzer.toml` の偽の双条件コメントを訂正する。

**調査中に前提が 1 つひっくり返った（証拠 F・実測）**: **`rust-analyzer.toml` は Claude Code の RA インスタンスに届いていない。** 独立な 2 つの workspace 水準キーがどちらも不発である——(1) flycheck は実際に走っており、その cargo check は `target/debug/` を使う（`cargo.targetDir = true` が効いていない）、(2) `workspaceSymbol("config")` が Struct / Enum だけを返す（`kind = "all_symbols"` が効いていない・対照クエリ `update_config` で「Claude Code 側が型で絞っている」説は排除済み）。機序は未確定。

**これは計画をブロックしない——むしろ plugin の価値を上げる。** `initializationOptions` が Claude Code の RA へ届く**唯一の経路**である以上、plugin こそが修理経路であり、前置タスクは要らない。

### issue からの逸脱（2 件）

**逸脱 1（ユーザー承認済み・2026-08-14）**: issue の受け入れ条件のうち `diagnostics.enable=false` は**今サイクルでは入れない**。理由は 2 つ。(1) バイナリの実測で、目的により直接効く別機構（Claude Code 側の `diagnostics: false`＝注入だけ止める）が見つかり、どちらを採るかが未決である。(2) 2026-08-14 の自前実測（`ra-diagnostics-noise-is-baseline-not-edits`）は「量の本丸は編集由来ではない」「`unlinked-file` は cargo から見えないので残す側」と結論しており、抑制そのものの費用対効果に疑問が立っている。**先に器と `checkOnSave` を入れ、`RA_LOG` で実効設定を実測してから、診断の扱いを別 issue で決める。**

**逸脱 2（ユーザー承認済み・2026-08-14）**: `.lsp.json` に **`workspace.symbol.search`（`kind = "all_symbols"` / `limit = 512`）も載せる**。**承認時の判断基準**: 「重要なのはエージェントが LSP から**過不足なく**情報を受け取ること」——ゆえに不足の側（symbol search が既定で切り詰められている）を直すのは目的そのものであり、issue の枠外だから外す、という理由では削らない。**この基準は診断の扱いにも同じ向きで効く**（次サイクルの判断材料）。issue は `checkOnSave` と `diagnostics` しか挙げていないが、証拠 F-2 で**この設定が現に効いていない**ことが判った。#1075 が「網羅性が要件の作業」のために広げた設定であり、**受益者はまさにエージェント**である。器を作る同じ変更で運べるので、別 issue へ切り出すほうがかえって遠回りになる。

**`cargo.targetDir` は今回入れない。** `checkOnSave` を切れば flycheck 分の `target/` 競合は消え、残るのは build script / proc-macro build 分だけになる。その規模と価値は実測後に決める（成果物の二重化は 2〜2.5 GB という既測値があるが、それは check 成果物込みの数字である）。

## 受け入れ条件

1. Snotra 所有の project-scope rust-analyzer LSP plugin がリポジトリに在る
2. Claude Code の RA インスタンスに `checkOnSave=false` が渡る（挙動プローブで実測・PR 本文）
2b. 同インスタンスで `workspace.symbol.search` が効く（＝現在の不発が直る・挙動プローブで実測・PR 本文）
3. `findReferences` / definition / implementation / hover / workspace symbols が引き続き使える
4. VS Code 側の RA diagnostics は従来どおり（`rust-analyzer.toml` も `.vscode/settings.json` も挙動を変えない）
5. 公式 `rust-analyzer-lsp` と custom plugin が `.rs` に二重に付かない
6. Edit/Write 後の確定検証は引き続き `post-edit.mjs` の fmt / clippy / crate test が担当する（`post-edit.mjs` の判定材料に LSP の状態を混ぜない）
7. `.lsp.json` の抑制キーが消えた／二重 LSP に戻った／JSON が壊れた場合に**機械が赤くなる**
8. `rust-analyzer.toml` の `targetDir` 検算コメントが事実に合っている
9. `npm run governance:check` が通る

**次サイクルへ送る**（issue の受け入れ条件のうち今回入れないもの）: `diagnostics.enable=false` が渡ること。→ 実測の結果とともに別 issue を立てる。

## 変更ファイル一覧と対象シンボル

| # | ファイル | 種別 | 対象 |
|---|---|---|---|
| 1 | `.claude/lsp/.claude-plugin/marketplace.json` | 新規 | marketplace 定義（`name: "snotra"`） |
| 2 | `.claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json` | 新規 | plugin マニフェスト |
| 3 | `.claude/lsp/snotra-rust-lsp/.lsp.json` | 新規 | RA の起動設定（`initializationOptions.checkOnSave=false`） |
| 4 | `.claude/settings.json` | 変更 | `extraKnownMarketplaces` 追加、`enabledPlugins` に 2 行 |
| 5 | `.claude/hooks/post-edit.mjs` | 変更 | `selectChecks` に `.claude/lsp/` 分岐 |
| 6 | `.claude/hooks/post-edit.test.mjs` | 変更 | `selectChecks` の期待値 |
| 7 | `.claude/hooks/lsp-config.mjs` | 新規 | 判定の純関数 `checkLspConfig(rootDir)`。**非 test の `.mjs` である必要がある**（下記 R-3） |
| 7b | `.claude/hooks/lsp-config.test.mjs` | 新規 | カナリア（実リポジトリ 1 本 ＋ 複製への変異注入群） |
| 8 | `.claude/rules/safety-nets.md` | 変更 | frontmatter `paths` に `.claude/lsp/**` |
| 9 | `docs/hooks.md` | 変更 | 発火一覧に 1 行 ＋「RA と hook の分担」節（この分担の正本） |
| 10 | `rust-analyzer.toml` | 変更 | `targetDir` 検算コメントの訂正 ＋「入れない側」への追記 |

**触らない**: `post-edit.mjs` の Rust 検査本体・`.vscode/settings.json`・`SPEC.md`（アプリの挙動を変えない）・`.githooks/`・CI。

## 実装順序

### Phase 1 — plugin の器と配線

- [x] `.claude/lsp/.claude-plugin/marketplace.json` を作る（スクラッチで validate 済みの形）

  ```json
  { "name": "snotra",
    "description": "Snotra リポジトリが所有する plugin の marketplace",
    "owner": { "name": "Snotra" },
    "plugins": [ { "name": "snotra-rust-lsp", "source": "./snotra-rust-lsp",
                   "description": "...", "version": "1.0.0", "author": { "name": "Snotra" } } ] }
  ```

- [x] `.claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json` を作る（`name` / `description` / `version` / `author`）
- [x] `.claude/lsp/snotra-rust-lsp/.lsp.json` を作る

  ```json
  { "rust-analyzer": {
      "command": "rust-analyzer",
      "extensionToLanguage": { ".rs": "rust" },
      "initializationOptions": {
        "checkOnSave": false,
        "workspace": { "symbol": { "search": { "kind": "all_symbols", "limit": 512 } } } } } }
  ```

  **入れ子で書く**（dotted key ではない）。RA の内部識別子は `workspace_symbol_search_kind` / `cargo_targetDir` で、dotted 形の文字列はバイナリに literal として存在しない（`config_data!` が実行時に `_` → `.` で組み立てる）ため、**文字列の有無では入れ子か dotted かを決められなかった**。ratoml の TOML 表現・issue のスニペット（`diagnostics.enable` を `{"diagnostics":{"enable":false}}` と書いている）と揃えて入れ子を採り、**効いたかどうかは PR 本文の挙動プローブで測る**（F と同型の「入れたのに効かない」を沈黙で再演させない）。

- [x] `.claude/settings.json` に配線を足す。**`claude plugin marketplace add` は使わない**——絶対パスを書き込むため（証拠 C-2）。相対パスで手書きする

  ```json
  "extraKnownMarketplaces": {
    "snotra": { "source": { "source": "directory", "path": "./.claude/lsp" } } },
  "enabledPlugins": {
    "snotra-rust-lsp@snotra": true,
    "rust-analyzer-lsp@claude-plugins-official": false, ... }
  ```

- [x] `claude plugin validate .claude/lsp --strict` と `claude plugin validate .claude/lsp/snotra-rust-lsp --strict` が exit 0 になることを確認する（マニフェスト 2 枚の妥当性はここまで。`.lsp.json` はこの検証器の視界の外・下記 Phase 2）

### Phase 2 — 機械検査（カナリア）

**`.lsp.json` は native の検証器に守られない**（実測: 壊しても `✔ Validation passed`）。この 1 枚が沈黙で腐る唯一の経路なので、自前のカナリアで縛る。

- [x] `post-edit.mjs` の `selectChecks` に分岐を足す（`CHECK_DEFINITION` には入れない——あれは「検査の定義を変えるファイル」の集合であり、意味が違う）

  ```js
  // Claude Code の RA インスタンスの設定。抑制キーが消えても、二重 LSP に戻っても、
  // 動くものは動いてしまう（RA は既定値で上がる）。沈黙する経路なのでカナリアで縛る。
  if (rel.startsWith(".claude/lsp/")) checks.push("hook-selftest");
  ```

- [x] **判定を `.claude/hooks/lsp-config.mjs`（非 test）へ、テストを `.claude/hooks/lsp-config.test.mjs` へ分ける。** 分割には**独立した 2 つの理由**がある
  - (i) 稼働中のガードへ変異を当てずに済む（`safety-nets.md`「複製に変異を当てる」）
  - (ii) **`G-stale-identifiers` の語彙源になる**（実測: `governance-check.mjs:1518` の `VOCAB_SOURCE_EXT` に `.json` は入らず、`:1522` の `VOCAB_TEST_FILE` が `*.test.mjs` を語彙源から外す）。`checkOnSave` / `initializationOptions` / `extensionToLanguage` / `lspServers` / `extraKnownMarketplaces` / `enabledPlugins` はどれも**現行語彙に無い**ので、判定を test ファイルにだけ置くと `docs/hooks.md` にキー名を書いた瞬間に `governance:check` が赤くなる。**キー名は文字列リテラルとしてコードに現れること**——`currentVocabulary` は `.mjs` のコメントを落とす（`:1599`）
- [x] `checkLspConfig(rootDir)` は**リポジトリ root を引数で受ける**——これで (a) 実リポジトリを指す薄いテスト 1 本と (b) temp へ複製した木へ変異を当てるテスト群、の 2 層に分けられる。稼働中の `.claude/settings.json` へ変異を当てずに済み（`safety-nets.md`「稼働中のガードを弱めない——複製に変異を当てる」）、worktree での cwd 依存も同時に消える。`vitest.config.ts` の include は `.claude/hooks/**/*.test.mjs`（実測）なので `hook-selftest` と CI の `npm test` の両方が自動で拾う

  検査する不変条件は次の 5 つ（**実ファイルを読む**——期待値の写しを持たない）

  1. `.lsp.json` が parse でき、宣言するサーバはちょうど 1 つ
  2. `extensionToLanguage` が `.rs` → `rust` を持つ
  3. `initializationOptions.checkOnSave === false` かつ `initializationOptions.workspace.symbol.search` が `kind: "all_symbols"` / `limit: 512` を持つ（**どちらも消えても動くものは動く**——RA は既定値で上がるので沈黙する）
  4. `enabledPlugins` が `snotra-rust-lsp@snotra: true` かつ `rust-analyzer-lsp@claude-plugins-official: false`（**`.rs` を宣言する LSP は 1 つだけ**）
  5. `extraKnownMarketplaces.snotra.source` が `{source:"directory", path:"./.claude/lsp"}` で、その先に `.claude-plugin/marketplace.json` が実在し、その plugin entry の `source` が `.lsp.json` を持つディレクトリを指す（**配送経路そのもの**——`.lsp.json` を直読みするだけの検査はここを素通りする）
  6. **`.lsp.json` 以外の 2 か所に `lspServers` が書かれていない**（`plugin.json` と marketplace entry）。**`.lsp.json` は 3 つの宣言箇所のうち優先度が最も低い**——`cJt` は `.lsp.json` を読んだ後に `Object.assign(n, manifest.lspServers)` で**上書きする**（自分でバイナリを読んで確認済み）。正本を 1 か所に定めるなら、残り 2 か所が黙って勝つ経路を塞ぐ必要がある
  7. **名前が一致している**——`marketplace.json` の `name` ／ `extraKnownMarketplaces` のキー ／ `enabledPlugins` キーの `@` 以降、および `plugin.json` の `name` ／ entry の `name` ／ `enabledPlugins` キーの `@` 以前。バイナリ逐語: *"Marketplace name. Must match the extraKnownMarketplaces key (enforced)"*。**ずれると LSP が上がらないだけで、エラーは debug log にしか出ない**
  8. `rust-analyzer.toml` が `checkOnSave` も `diagnostics` も書いていない（**クライアント非対称性の土台**——ratoml は workspace / local 水準でクライアント設定を上書きするので、ここへ書かれた瞬間に plugin の設定が黙って無効化され、しかも VS Code 側にも波及する。証拠 D）
- [x] `post-edit.test.mjs` に `selectChecks(".claude/lsp/snotra-rust-lsp/.lsp.json") === ["hook-selftest"]` を足す
- [x] `.claude/rules/safety-nets.md` の frontmatter `paths` に `".claude/lsp/**"` を足す。**Phase 1 より後でなければならない**——`G-rules-globs` は各 glob が**実在ファイルに 1 件以上マッチする**ことを要求する（`scripts/governance-check.mjs:805`）。ファイルを作る前に glob を足すと赤くなる
- [x] `docs/hooks.md` の発火一覧へ 1 行足す（**実在する具体パス 1 件**が要件・`G-hook-fires` が検査する）

  | `.claude/lsp/snotra-rust-lsp/.lsp.json` | `hook-selftest` | `.claude/lsp/**` 全体 |

- [x] **不変条件の足ごとに変異を当て、カナリアが赤くなることを実測する**（#858: 足が複数あるとき、どの足が欠けても赤くなるとは限らない）。**変異はすべて temp へ複製した木に当てる**——稼働中の `.claude/settings.json` と `.lsp.json` には触れない
  1. `.lsp.json` から `initializationOptions.checkOnSave` を消す（足 3・前半）
  1b. `.lsp.json` から `initializationOptions.workspace.symbol.search` を消す（足 3・後半。**同じ足でも枝が 2 本あるので別々に測る**）
  2. `.lsp.json` を JSON として壊す（足 1）
  3. `extensionToLanguage` から `.rs` を消す（足 2）
  4. `enabledPlugins` の `rust-analyzer-lsp@claude-plugins-official` を `true` に戻す（足 4・二重 LSP）
  5. `extraKnownMarketplaces.snotra.source.path` を実在しないディレクトリへ向ける（足 5・**`.lsp.json` は正しいのに配送が死んでいる**形）
  6. `plugin.json` に `lspServers` を足して `.lsp.json` と矛盾させる（足 6・**正本が黙って負ける**形）
  7. `marketplace.json` の `name` を `extraKnownMarketplaces` のキーとずらす（足 7・**沈黙で load されない**形）
  8. `rust-analyzer.toml` に `checkOnSave = false` を足す（足 8・**設定は正しく見えるのに ratoml が上書きしている**形）
- [x] 9 変異それぞれで赤くなり、無変異の複製では緑になることを確認する（**発火しない向きも測る**——変異が本来の回帰より強くて赤くなっているのではないことの検算）

- [x] `.lsp.json` に `settings`（`workspace/didChangeConfiguration` 経由）を**書かない**と明示する。`initializationOptions` と役割が重なるうえ、`settings` を書くと `workspace/configuration` capability が true になり、RA が設定を pull できる経路が開いて決定論性が落ちる（証拠 A）

**検査の層は 1 枚に留める（`docs/development-principles.md`「検証の層と、層と層の隙間」を通した結果）**

| 層 | カバーする | カバーしない |
|---|---|---|
| hook のカナリア（`hook-selftest`）＋ CI の `npm test` | 編集した瞬間に赤／**ファイル削除も** CI 側の実リポジトリ読み取りで赤（カナリアは実ファイルを読むため、消えれば落ちる）／ubuntu・windows 両方 | **`skip-ci` ラベル付き PR** |

独立導出レビューは `governance-check.mjs` へ `G-lsp-config` を足す 2 枚構成を推奨したが、**採らない**。(i) 削除は CI のカナリアが捕まえる（hook が見られないだけで、検知器そのものは実ファイルを読む）、(ii) 語彙の供給は `lsp-config.mjs` が既に果たす、(iii) 残る差は `skip-ci` PR だけである。**`skip-ci` の残余は受容する**——設定 JSON だけを触る変更に `skip-ci` を付ける動機が薄く、層を 1 枚増やす費用に見合わない。

### Phase 3 — `rust-analyzer.toml` のコメント訂正

- [x] `[cargo] targetDir` の検算コメントから双条件を外し、**測定事実だけ**を書く（機序へ踏み込まない）
  - `target/rust-analyzer/` は RA の `cargo check` だけでなく build script / proc-macro build にも使われるので、**育っても** cargo work の存在までしか言えない
  - **2026-08-14 の実測**: flycheck は実際に走っている（`target/flycheck0/stdout` が `cargo check --message-format=json` の生出力）が、その artifact は `target/debug/` にあり `target/rust-analyzer/` は不在。**この設定は Claude Code の RA には効いていない**
  - **同じ実測が `[workspace.symbol.search]` にも当たる**（`workspaceSymbol("config")` が Struct / Enum だけを返す）。**独立な 2 キーの不発ゆえ、このファイル全体が Claude Code の RA へ届いていない**と読める。**機序は未確定**
  - 判定の手段は「ディレクトリが出来たか」ではなく**挙動プローブ**である（`workspaceSymbol` に Function が混ざるか・`target/flycheck0/stdout` の mtime が編集後に動くか）
- [x] `checkOnSave` を切っても `targetDir` の価値（build script / proc-macro build と通常 `target/` のロック分離）が残ることを書く
- [x] **このファイルは消さない・値も動かさない。** 機序が未確定なまま撤去すると、届くようになったときに workspace 水準の解決順で plugin を上書きする世界が黙って戻る。現状の値は plugin へ入れる値と同じ向きなので衝突せず、足 6 のカナリアがその世界も守る
- [x] 「以下は入れない側」の節へ 1 行足す: **`checkOnSave` / `diagnostics.*` をこのファイルへ書くと、workspace / local 水準ゆえ plugin の `initializationOptions` を上書きし、しかも VS Code 側にも掛かる**（証拠 D）

### Phase 4 — 文書整合

- [x] `docs/hooks.md` に「Claude Code の RA インスタンスと hook の分担」節を置く（**この分担の正本はここ 1 か所**。`.lsp.json` は JSON でコメントを持てないため、カナリアのコメントからこの見出しを参照する）
- [x] `npm run governance:check` を通す（新規ファイルを含むので必須・`pr-governance-check-before-pr`）
- [x] `npx vitest run .claude/hooks` が緑であることを確認する

### 実装中に判明した計画外の作業（すべて完了）

- [x] **`checkLspConfig` の母集団を「ツリー内の全 `rust-analyzer.toml`」へ広げる**（`findRatomlFiles` を新設）。code-reviewer の H-1 は `selectChecks` を basename でアンカーせよという指摘だったが、それだけ入れると `snotra-core/rust-analyzer.toml` で**hook が走ったのに判定は root しか見ず緑**になる。割り当てられたファイルの緑は「合格」を意味するので沈黙より悪い——発火と判定の母集団を揃えた
- [x] **ratoml 検査を配送経路の検査より前へ移す**（M-1）。前段の早期 return に道連れにされ、多重故障のとき報告から消えていた。独立性の回帰テストも足した
- [x] **引用キー `"checkOnSave" = false` の偽陰性を閉じる**（M-2）。TOML として正当な構文が素通りしていた。行末コメントの偽陽性も `governance-check.mjs` の toml 先例に合わせて解消
- [x] **故障注入を 14 → 22 本へ**。`/symmetric-check` が 2 本（真偽の対・名前の対の片枝）、code-reviewer の `if (false)` 注入が 3 本（`extraKnownMarketplaces` 検査・entry 実在検査・サーバ数検査が誰にも縛られていなかった）、残りは L-2 と ratoml の 2 形
- [x] **「壊れ方がどれも沈黙する」という偽の全称を限定する**（M-3）。反例は計画自身の異常系に在った（load 失敗は navigation の消失として現れる）。`docs/hooks.md` を 2 分類の表に開き、写し 4 か所は主張を限定して正本の見出しへの参照に落とした
- [x] `.claude/settings.local.json` が project より優先されることを残余として記録（⚠️-2）

## 不変条件と異常系

- **`post-edit.mjs` の判定材料に LSP の状態を混ぜない。** 今回触るのは `selectChecks` の**発火条件**だけで、検査の実行・判定機構には手を入れない。
- **`.rs` を宣言する LSP サーバは常にちょうど 1 つ。** 増えれば Claude Code が警告を出すが、警告は会話に残らないのでカナリアが正本の検知器になる。
- **VS Code 側の RA の挙動は変わらない。** 変えるのは Claude Code の client 設定だけで、`rust-analyzer.toml` にも `.vscode/settings.json` にも `checkOnSave` を書かない。
- **異常系: plugin が load されない**（trust 未受諾・マニフェスト不正・パス解決失敗）。このとき `.rs` の LSP は**上がらない**（公式 plugin を `false` にしたため）。fail-closed ではあるが「navigation が消える」形で現れるので、PR 本文の実測項目で必ず確かめる。
- **異常系: `rust-analyzer` が PATH に無い。** 現状も同じ前提（公式 plugin も `command: "rust-analyzer"`）なので条件は変わらない。
- **cwd 依存**: `path` の相対解決は cwd 基準（証拠 C-2）。リポジトリルート以外を cwd にして起動すると marketplace が見つからない。**これは受容する残余**——`.claude/settings.json` 自体が同じ前提（`${CLAUDE_PROJECT_DIR:-.}`）で動いている。
- **worktree での沈黙する乖離（R-14・呼び出し側で機序を裁定済み）**。`known_marketplaces.json` は `~/.claude/plugins/` の**グローバルな平坦マップで、キーは marketplace 名**（実測: 4 件登録・各エントリが `source` と `installLocation` を 1 つずつ持つ）。ゆえに名前 `snotra` はマシンに 1 個しか無いのに、`"./.claude/lsp"` はメインツリーと `.claude/worktrees/agent-xxxx` で**別の絶対パスへ resolve される**。さらに reconciler に次の分岐が在る（`claude.exe` から逐語抽出・`pZ` は `source==="file"||source==="directory"` で自分で確認済み）:

  ```js
  if (d.action === "update" && pZ(d.source) && !await Oy(d.source.path)) {
    w(`[reconcile] '${d.name}' declared path does not exist; keeping materialized entry`); s.push(d.name); continue }
  ```

  → **宣言パスが無いとき、debug log へ書いて「以前マテリアライズした登録を維持する」。** `.claude/lsp` を持たないコミットから作った worktree（＝この変更より前の枝・移行期）では、**別のツリーの plugin を黙って使い続ける**。カナリアは**そのツリーのファイル**を読むので、**検査は緑・実際に効いている plugin は別物**という乖離が成立する。**受け入れ条件 7 はこの経路に届かない**——それが今回の残余である。

  **今サイクルでは検知機構を置かない。** 検知するには `~/.claude/plugins/known_marketplaces.json`（ユーザーマシンの状態・CI に無い）を読んで「登録された `snotra` の path がこのツリーか」を突き合わせることになるが、(i) CI では必ず skip する検査になり、(ii) ファイルを作った直後から次のセッション再起動までの**正当な過渡状態で赤くなる**。**再マテリアライズの実挙動を測る前に、鳴りうる検知器を静的読解だけで設計しない**（`measure-whether-detector-can-fire`）。→ PR 本文で実測し、次サイクルで決める。
  ⚠️ `Xdg` が比較の前に相対→絶対を解決すること（＝ツリーを移るたび `sourceChanged` が立つこと）はレビュアの静的読解であり、呼び出しグラフを自分で辿ってはいない。
- **実装中の副作用（想定しておく）**: `.claude/settings.json` の編集は file watcher が即座に拾う（`docs/hooks.md`「機構と保守」で実測済み）。ゆえに Phase 1 の配線を入れた瞬間に、実装中のセッションで公式 plugin が無効化され `.rs` の LSP が落ちる／trust の確認が出る、という挙動がありうる。**それ自体は異常ではない**が、実装中に LSP ツールが使えなくなったら「壊した」ではなく「切り替わった」を先に疑う。plugin の適用そのものは再起動が要る（証拠 A）。 受け入れ条件 6（`post-edit.mjs` の判定材料に LSP の状態を混ぜない）は**規範であって機構ではない**。既存の `hook-selftest` は `post-edit.mjs` の挙動を守るが、「LSP の状態を混ぜていないこと」を直接は検査しない。今回それを機構化しない理由は、混ぜるには非同期の待ちを新設する必要があり、その変更は必ず `.claude/hooks/**` の編集として `hook-selftest` と `.claude/rules/safety-nets.md` の配送に掛かるためである（**沈黙で入る経路が無い**）。

## テスト方針と検証コマンド

| 対象 | 手段 | 実行時点 |
|---|---|---|
| マニフェスト 2 枚の妥当性 | `claude plugin validate <path> --strict` | Phase 1（手動・CI には `claude` が無い） |
| `.lsp.json` と配線の意味的整合 | `.claude/hooks/lsp-config.test.mjs` | 編集ごと自動（`hook-selftest`）＋ PR CI。**測定済み**: `vitest.config.ts` の include は `.claude/hooks/**/*.test.mjs` で、`ci.yml` は ubuntu / windows の両方で `npm test`（＝`vitest run`）を実行する |
| カナリアの検出力 | 複製した木への 5 変異の注入 | Phase 2 |
| 発火表の写しの整合 | `npm run governance:check`（`G-hook-fires`） | Phase 4 ＋ PR CI |
| **RA の実効設定** | 挙動プローブ 2 本（下記）。`RA_LOG` は補助 | **PR 本文**（セッション再起動が要る） |

**着手前のベースライン（2026-08-14 実測・実装後にこれと比べる）**

- `npm run governance:check` → green（検査 19 件 / 対象文書 35 件 / rules 8 件 / skills 12 件）
- `npx vitest run .claude/hooks` → 2 files / **285 tests** passed（1.94s）
- `hook-selftest` の実体は `vitest run .claude/hooks`（`.claude/hooks/post-edit.mjs:326` の `buildCommand`）
- `workspaceSymbol("config")` → 14 件・**Function ゼロ**（証拠 F-2）
- `target/flycheck0/stdout` の mtime → 動いている（証拠 F-1）

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。アプリの挙動を一切変えない（開発環境の設定のみ）。
- `docs/hooks.md`: 必要（発火一覧 ＋ 分担の節）。
- ルート `CLAUDE.md` / `AGENTS.md`: **不要**。「フック」表の記述（発火は `settings.json`、判定は各スクリプトが SSOT）も、条件別チェック表のセーフティネット行も、今回の変更で偽にならない。**新しい規範を足さない**——足すなら既存の全事例に当てて検算する必要があり、今回はその必要が無い。
- **偽になる散文の洗い出し（概念ラベルで grep 済み）**: `rust-analyzer` / `LSP` / `enabledPlugins` / `extraKnownMarketplaces` で `**/*.md` を走査した結果、`workspace/` 以外の該当は `AGENTS.md`（「呼び出し元は LSP ツールの findReferences で列挙する」——navigation は保つので**偽にならない**）と `docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md:1106`（当時の `settings.json` の写しに `rust-analyzer-lsp@claude-plugins-official: true` が在る）の 2 件。**後者は書き換えない**——`docs/superpowers/` は #589 で非規範化された歴史資料であり、`scripts/governance-check.mjs` が `G-references` / `G-spec-sections` / `G-stale-identifiers` の対象から明示的に除外している（`f.startsWith("docs/superpowers/")` の除外が 4 箇所）。当時の記録として凍結するのが既存の契約。
- `docs/adr/`: **不要**。否定の知識（inline `settings` marketplace・user ratoml・shim・検査 id 新設の却下）は `workspace/research.md` と本計画に根拠つきで残り、いずれも「将来また検討されうる分岐」ではなく実測で閉じた枝である。

## 未確定（実装前に潰す）

- [x] **`extraKnownMarketplaces` の `directory` source の `path` は何を基準に解決されるか** — バイナリ実測: `path.resolve(e.path)` ゆえ cwd 基準。相対パスを手書きすれば可搬。CLI（`marketplace add`）は入力を `path.resolve` して**絶対パスで書き込む**ので使わない。
- [x] **`.lsp.json` の形（トップレベルがサーバ名か）** — バイナリ実測: plugin root の `.lsp.json` を `record<serverName, LspServerConfig>` として parse（関数 `cJt`）。issue のスニペットの形は正しい。
- [x] **`initializationOptions` が実際に渡るか** — バイナリ実測: `initialize` の params に `initializationOptions: t.initializationOptions ?? {}`。かつ `settings` を書かない限り `workspace/configuration` capability が false になり、RA は他経路で設定を pull できない。
- [x] **`checkOnSave` は RA 1.97.1 に実在するキーか** — `rust-analyzer.exe`（40 MB・toolchain 実体）の文字列に 3 件。ratoml 側は当該キーを書いていないのでクライアント設定が勝つ（証拠 D）。
- [x] **公式 plugin を project scope で `false` にして user scope の `true` を上書きできるか** — スキーマの description が名言（逐語）: *"Settings precedence is user < project < local < flag < policy"*。
- [x] **マニフェスト 2 枚の必須フィールド** — スクラッチで実測。`marketplace.json` は `name` / `description` / `owner` / `plugins[]`、plugin entry は `name` / `source` / `description` / `version` / `author` で `--strict` が exit 0。`plugin.json` は `name` / `description` / `version` / `author` で exit 0。
- [x] **`claude plugin validate --strict` は `.lsp.json` を守るか** — **守らない**。JSON を壊しても抑制キーを消しても exit 0（スクラッチで変異注入）。→ 自前カナリアが必須、検査 id の新設は不要（マニフェスト検証は native、意味的整合はカナリア、と層を分ける）。
- [x] **診断（`diagnostics`）をどう扱うか** — ユーザー判断で「まず測ってから決める」。今サイクルは器と `checkOnSave` のみ。
- [x] **`rust-analyzer.toml` は Claude Code の RA に届いているか** — **届いていない**（証拠 F・独立な 2 キーで実測）。機序は未確定だが、設計判断（plugin が唯一の到達経路）はこの観測だけで決まる。
- [x] **`initializationOptions` の入れ子キーは受理されるか** — **文字列の有無では決められなかった**（RA の内部識別子は `_` 区切りで、dotted 形は実行時に組み立てられる）。入れ子形を採り、**PR 本文の挙動プローブ 2 で測る**と決めた。外していれば `workspaceSymbol("config")` に Function が混ざらないので、沈黙せずに判る。

**PR 本文のチェックリストへ送るもの**（セッション再起動が要り、計画に置くと循環する。`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」と同型）:

- 再起動後に `findReferences` / definition / hover / workspace symbols が使えること
- 二重 LSP の警告が出ないこと
- **挙動プローブ 1（`checkOnSave` が渡ったか）**: `.rs` を編集したあと `target/flycheck0/stdout` の mtime が**動かない**こと（今は動く・証拠 F-1）
- **挙動プローブ 2（入れ子キーが渡ったか）**: `workspaceSymbol("config")` に **Function が混ざる**こと（今は Struct / Enum だけ・証拠 F-2）。**これは `workspace.symbol.search` の検証であると同時に、入れ子形の `initializationOptions` が受理されるかの検証でもある**
- `RA_LOG=rust_analyzer=info` を `env` に渡して実効 config を読む（**機序の解明用・補助**。`RA_LOG` が実効 config を吐くかは未検証なので、上の 2 プローブを主とする。測り終えたら `env` は外す）
- サブディレクトリ起動・agent worktree で marketplace の相対 path が解決されるか（証拠 C-2 の 2 サイトのどちらが効くか）
- **worktree のセッションで LSP が上がるか、そして「どのツリーの `.lsp.json` が効いているか」**（R-14）。ツリーを移った直後の再マテリアライズが冪等で安価か。`.claude/lsp` を持たない枝の worktree で `[reconcile] ... keeping materialized entry` が実際に出るか（debug log で確認）

## plan-review 結果

- **リスク**: 高（セーフティネットの新設/変更 ＋ ガバナンス文書の変更）
- **レビュー方式**: 独立導出 1 体（Step 2b）。争点そのものは敵対枠（3b）と advisor が既に 2 度独立に当たっており、まだ誰も見ていない「**変更ファイル集合の漏れ**」へ枠を使った
- **エージェント数**: 2（3b の敵対枠 1 体 ＋ 独立導出 1 体）
- **状態**: 完了（`workspace/plan-review-lsp-plugin.md`・要対処 13 / 軽微 9 / 未検証 11）。レビュア自身が冒頭で**汚染（催促メッセージで一部の実測値が漏れたこと）を開示**し、漏れた項目を所見から外して ⚠️ 未検証へ出典つきで移している

### 主エージェントの自己照合（Step 1）

1. **issue の全要件に作業項目が対応する** — 対応。diagnostics の 2 条件だけがユーザー承認のうえで次サイクルへ、`RA_LOG` 実測は PR 本文へ（循環回避）
2. **変更ファイル・シンボルが実在する** — `selectChecks` / `CHECK_DEFINITION` / `buildCommand`（`.claude/hooks/post-edit.mjs:125,70,285`）、`docs/hooks.md` の発火一覧、`safety-nets.md` の frontmatter をすべて読んで確認
3. **不変条件・異常系・テスト期待値が具体化されている** — カナリア 6 不変条件 / 7 変異 / ベースライン実測値つき
4. **`SPEC.md` と関連文書の更新要否** — `SPEC.md` 不要（アプリ挙動を変えない）。偽になる散文は概念ラベルで grep 済み（該当 2 件・うち 1 件は歴史資料ゆえ凍結）
5. **未確定欄に未チェック項目が無い** — 無い（8 件すべて実測または決定で閉じた）
6. **タスク分割が既存トリガーを跨いでいない** — Phase 1（器）→ Phase 2（検査）の順は `G-rules-globs`（glob は実在ファイルに 1 件以上マッチする必要がある）と `G-hook-fires`（代表パスは実在が要件）が**強制する**もので、恣意的な分割ではない
7. **偽になる散文の洗い出し** — 済（上記「`SPEC.md`・関連文書の更新要否」）

### 要対処（自己照合 ＋ 独立導出）

自己照合で出た 2 件（Phase の順序制約・`.claude/settings.json` 即時反映の副作用）は反映済み。独立導出の 13 件のうち、**計画を変えたのは 4 件**。いずれも根拠を主エージェントが再照合してから採った。

| # | 所見 | 対処 | 再照合した根拠 |
|---|---|---|---|
| R-3 | 文書に `checkOnSave` 等の camelCase を書くと `G-stale-identifiers` が赤になる（`.json` は語彙源でない・`*.test.mjs` は語彙源から外れる） | **採用**。判定を非 test の `.claude/hooks/lsp-config.mjs` へ分離 | `governance-check.mjs:1518`（`VOCAB_SOURCE_EXT`）/ `:1522`（`VOCAB_TEST_FILE`）/ `:1590-1600`（母集団にディレクトリ制限なし・`.mjs` はコメントを落とす）を自分で読んで確認 |
| R-9 | `.lsp.json` は 3 つの宣言箇所のうち**優先度が最も低い**（manifest が `Object.assign` で上書き） | **採用**。カナリアの足 6（他 2 か所に `lspServers` が無いこと）を追加 | `claude.exe` の `cJt` を自分で抽出済み——`.lsp.json` を読んだ後に `if(e.manifest.lspServers){...Object.assign(n,s)}` |
| R-6 | marketplace 名が 3 か所で一致しないと**沈黙で load されない** | **採用**。カナリアの足 7 を追加 | バイナリ逐語 *"Must match the extraKnownMarketplaces key (enforced)"*（自分の抽出にも同じ文字列が在る） |
| M-9 | `settings` は `initializationOptions` と役割が重なる | **採用**。「使わない」を明示 | 証拠 A——`settings` を書くと `workspace/configuration` capability が true になり決定論性が落ちる |
| **R-14**（レビュアの追加報告・**独立ではなく差分レビュー**として受領） | worktree では marketplace 名がグローバルに 1 個なのに相対 path が別の絶対パスへ解決され、宣言パスが無いツリーでは**別ツリーの plugin を黙って使い続ける** | **採用**。異常系へ残余として明記 ＋ PR 本文へ実測項目。**検知機構は今サイクルでは置かない**（理由は当該節） | 足を 2 本とも自分で測った——(1) `known_marketplaces.json` は名前キーの平坦マップ（実ファイルを読んだ・4 件）、(2) `keeping materialized entry` の分岐を `claude.exe` から逐語抽出（`pZ` は自分で確認済み） |
| R-12 | 検査の層を hook + governance の 2 枚にする | **不採用（降格）**。残る差は `skip-ci` PR だけで、削除は CI のカナリアが捕まえ、語彙供給は `lsp-config.mjs` が果たす。**受容する残余として明記** | `ci.yml:45,155` の `npm test` はカナリアを実行し、カナリアは実ファイルを読むので削除でも落ちる |
| R-1 / R-2 / R-4 / R-5 / R-7 / R-8 / R-10 / R-11 / R-13 | カナリアが対で要る・発火表を同じ変更で直す・vitest include・二重 LSP は settings の 1 行が防いでいる・rules の paths・セーフティネット母集団・双条件の訂正・PR 本文への振り分け・検証の層の表を通す | **既に計画に在った** | 各 Phase に対応項目あり |

**独立導出 ∖ plan の差分（漏れ候補）は上記 4 件で尽きた。plan ∖ 独立導出（スコープ過剰）は 0 件。**

**⚠️ ただし「独立に収束した」とは書けない。** レビュア自身が汚染を開示している——`*.md` 全域への概念ラベル grep で `workspace/` の除外を掛け忘れ、`plan.md` / `research.md` の一部を読んでいる。本人が「grep より前に自力導出した」と線を引いたのは**ファイル集合・シンボル集合・検査の発火・偽になる散文**（R 系の大半がここに属する）で、**leak 由来として母集団から外すよう明示された**のは証拠 F・スコープ裁定・受け入れ条件リスト・ファイル表の一部・「CLI が絶対パスを書く」件である。ゆえに上表の「既に計画に在った」は**一致の観測**であって独立性の証拠ではない。**採否の根拠は一致ではなく、右列の再照合（自分で読んだ `file:line`）に置いてある。**

**R-3 の対処先はレビュアの提案と異なる。** レビュアは `scripts/governance-check.mjs` へ置くことを薦めたが、`currentVocabulary`（`governance-check.mjs:1590-1600`）は `snapshot.files` を**ディレクトリで制限せず**、`VOCAB_SOURCE_EXT` と `VOCAB_TEST_FILE` だけで振り分ける。ゆえに `.claude/hooks/lsp-config.mjs`（非 test の `.mjs`）でも語彙は供給される。**検査を governance 層へ動かす理由にはならない**ので、責務（判定は hook 層・純関数）を優先した。

### 未検証（PR 本文のチェックリストへ送る）

- ⚠️ `initializationOptions` の入れ子キーが RA に受理されるか（挙動プローブ 2）
- ⚠️ marketplace の相対 path 解決の主経路（cwd 基準と読んだが、呼び出しグラフの直接トレースは未実施）
- ⚠️ fresh clone / worktree での再現（U-1）。とくに **directory source のキャッシュ名が `path.basename` 由来**で、worktree と本体で衝突しうる（U-3）——`.claude/lsp` を指す形なら basename は `lsp` で一定だが、衝突時の挙動は未検証
- ⚠️ `.lsp.json` の編集がセッション中に反映されるか（`.claude/settings.json` は即時反映と実測済みだが、plugin 側は未確認・U-5）
- ⚠️ clone 直後の初回セッションで、trust を受けてから LSP が上がるまでの経路（U-8）
- ⚠️ 走査していない範囲: `.rs` のコメント・`docs/adr/`・`docs/superpowers/`（U-11。**「探した範囲での不在」であって全称否定ではない**）

### 判断

- **実装着手: 可**（逸脱 1・2 ともユーザー承認済み。要対処 4 件を計画へ反映済み。未検証は PR 本文へ振り分け済み）

## 人間レビュー

- [x] 承認済み — 2026-08-14 / 問い: "**逸脱 2**: `.lsp.json` に `workspace.symbol.search`（`all_symbols` / `limit 512`）も載せるか（issue には無い項目。証拠 F-2 で現に効いていないと判ったため、器を作る同じ変更で運ぶ案）。" / 回答: "逸脱 2 はやりましょう。重要なのはエージェント（この場合はあなた）がLSPから過不足なく情報を受け取ることにあるので"
