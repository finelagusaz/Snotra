# 調査: Claude Code 用 rust-analyzer を project-local LSP plugin に分離する（#1083）

## issue の要約

Claude Code が起動する rust-analyzer（以下 RA）の責務を semantic navigation に絞り、検証の確定判定は既存の PostToolUse hook（`post-edit.mjs` の fmt / clippy / crate test）に残す。そのために **Snotra 所有の project-scope LSP plugin** を用意して `checkOnSave=false` / `diagnostics.enable=false` を Claude Code の RA インスタンスにだけ渡す。VS Code 側の RA は従来どおり診断を出す。あわせて `rust-analyzer.toml` の `targetDir` 検算コメント（強すぎる双条件）を訂正する。

ユーザーからの追加要求: **「今の repo の設定確認と、公式ドキュメント・ベストプラクティスを参照してよりよい方法を模索する」**——issue の方針（`.lsp.json` の形・機構の選択）を所与とせず、現行仕様を一次証拠で確かめたうえで再設計してよい。

## 測定環境（この調査の前提そのもの）

| 対象 | 実測値 | 取得方法 |
|---|---|---|
| Claude Code | `2.1.232` / BUILD `2026-08-13T16:45:27Z` / GIT_SHA `a640e968` | `claude.exe` 内の埋め込み文字列 |
| `claude.exe` | `C:/Users/Eoh/.local/bin/claude.exe`（319 MB・単一バイナリ） | `Get-Command claude` |
| rust-analyzer | `1.97.1 (8bab26f4 2026-07-14)` | `rust-analyzer --version` |
| user ratoml | **不在**（`%APPDATA%/rust-analyzer/` ごと無い） | `ls` |
| `target/rust-analyzer/` | **不在** | `ls` |

**正本の順序**: 「installed binary > 公式 docs > issue の記述」。issue の参考 URL は `utm_source=chatgpt.com` 付きで、後述のとおり形式の一部が実体とずれていた。

## 一次証拠 A — Claude Code の LSP 設定スキーマ（バイナリの zod 定義）

`claude.exe` から抽出した LSP サーバ設定のスキーマ（逐語）:

```
command / args / extensionToLanguage / transport("stdio"|"socket") / env
initializationOptions : "Initialization options passed to the server during initialization"
settings              : "Settings passed to the server via workspace/didChangeConfiguration"
workspaceFolder / startupTimeout / shutdownTimeout / restartOnCrash / maxRestarts
diagnostics           : "Whether to push publishDiagnostics into the agent context after edits.
                         Set to false to keep LSP navigation (goToDefinition, hover, etc.) but
                         suppress automatic diagnostic injection. Defaults to true."
```

**この `diagnostics` フィールドが今回の最大の発見である。** issue は「RA 側で診断計算を止める」（`initializationOptions.diagnostics.enable=false`）しか想定していなかったが、Claude Code には**注入だけを止める専用スイッチ**がある。公式 docs の plugins-reference にも同じ表があり、**意味は同一**（逐語は一致しない——docs 側は "publishDiagnostics" を "diagnostics"、"LSP navigation (goToDefinition, hover, etc.)" を "code navigation" と言い換えている）。

このフィールドは**宣言だけでなく実行時にも配線されている**（敵対枠が確認・採用）: 診断をエージェント context へ流すループの内側に `if(c?.config?.diagnostics===!1){ w(\`Diagnostics disabled for ${l}, skipping\`), s++; continue }` が在る。⚠️ ただし「`diagnostics===false` のとき navigation の request が実際に成功する」ところまでは静的読解の傍証止まりで、実行時未検証。

初期化の実体も確認した:

```js
initializationOptions: t.initializationOptions ?? {},
capabilities: { workspace: { configuration: t.settings != null, workspaceFolders: false }, ... }
```

→ **`settings` を書かない限り `workspace/configuration` capability が false** になる。RA はクライアントから設定を pull できず、`initializationOptions` だけが唯一のクライアント設定経路になる（決定論的で好都合）。

`command` / `args` / `env` / `workspaceFolder` では `${CLAUDE_PLUGIN_ROOT}` / `${CLAUDE_PROJECT_DIR}` / `${CLAUDE_PLUGIN_DATA}` / `${ENV_VAR}` が展開される（関数 `GIe` / `i1b`）。**`initializationOptions` は展開対象外**。

## 一次証拠 B — 配送形式（issue の `.lsp.json` 記述の検算）

- **`.lsp.json` は実在する。** plugin root の `.lsp.json` を読み `record<serverName, LspServerConfig>` として parse する（関数 `cJt`）。issue のスニペットの入れ子（トップレベルがサーバ名）は**正しい**。
- **plugin manifest でも宣言できる。** `plugin.json` の `lspServers` は「`.lsp.json` への相対パス | 設定 record | その配列」の union。`.lsp.json` を先に読み、manifest 側を `Object.assign` で重ねる。
- **plugin と認識される目印**: `.claude-plugin/` または `commands/` `skills/` `agents/` `hooks/` `themes/` `output-styles/` `monitors/` `workflows/` `SKILL.md` `.mcp.json` `.lsp.json` のいずれかが top level に在ること。**`.lsp.json` 単体でも plugin になる。**
- **公式 plugin の実体は `.lsp.json` を持たない。** `~/.claude/plugins/cache/claude-plugins-official/rust-analyzer-lsp/1.0.0/` には `LICENSE` と `README.md` しか無く、LSP 宣言は **marketplace.json のエントリ側の `lspServers`** に在る:

  ```json
  { "name": "rust-analyzer-lsp", "source": "./plugins/rust-analyzer-lsp", "strict": false,
    "lspServers": { "rust-analyzer": { "command": "rust-analyzer",
                                       "extensionToLanguage": { ".rs": "rust" } } } }
  ```

  → **`initializationOptions` は一切渡していない**ので、現状の Claude Code の RA は全キー既定値で動いている。

## 一次証拠 C — repo からの配送経路と二重起動

- `extraKnownMarketplaces` の説明（逐語）: *"Additional marketplaces to make available for this repository. Typically used in repository .claude/settings.json to ensure team members have required plugin sources."* → **repo 所有 plugin の正規経路がこれである。**
- marketplace の source 種別に **`{"source":"directory","path":"<.claude-plugin/marketplace.json を含むローカルディレクトリ>"}`** と `{"source":"file","path":...}` がある。repo 内 marketplace は仕様上サポートされている。
- 設定の優先順位（逐語）: `userSettings → projectSettings → localSettings → flagSettings → policySettings`、*"Ordered low-to-high priority — later entries override earlier ones."* → **project の `.claude/settings.json` は user の `enabledPlugins` を上書きできる**。現状 user 側で `rust-analyzer-lsp@claude-plugins-official: true`、project 側でも `true`。
- **二重宣言は検知される。** 同じ拡張子に 2 つの LSP が付くと警告が出る（逐語）: *"Disable plugin \"X\" to use this plugin's LSP server for .rs files, or disable \"X\" to silence this warning"*。→ 受け入れ条件「二重起動しない」は **project 設定で公式 plugin を `false` にする**ことで満たせる。
- **project scope の plugin は workspace trust を受けてから load される**（公式 docs・逐語）: *"LSP servers start only after you trust the workspace"*。
- **`enabledPlugins` の優先順位はスキーマ自身が名言している**（逐語）: *"Settings precedence is user < project < local < flag < policy, so to disable a plugin that project settings enable, set it to false in `.claude/settings.local.json` — setting false in `~/.claude/settings.json` is overridden by the project."*

### C-2. `directory` source の `path` 解決（未確定 1 の決着・実測）

読み手側のコード（逐語）:

```js
case "directory": { let g = path.resolve(e.path);
                    s = path.join(g, ".claude-plugin", "marketplace.json"); i = g; a = false; break }
```

- **`path.resolve()` ゆえ、相対パスはプロセスの cwd を基準に解決される。** Claude Code の cwd は通常プロジェクトルートなので、`"./..."` と手書きすれば**マシン間で可搬**になる。
- **正規化サイトは 2 つあるが、主経路は上の生の `path.resolve()` である。** もう 1 つ（`Xdg`）は `(source==="directory"||source==="file") && !isAbsolute(path)` のとき **git root 基準**で解決するが、敵対枠が**自ら訂正したとおり**、これは marketplace.json を読み込む主経路ではなく、**登録済み宣言が変わったかを判定して再インストールを起こす reconciliation 経路**の関数である。

  → **cwd 依存は緩和されない**。リポジトリルート以外を cwd にして起動すると相対 path は解決されない。⚠️ 呼び出しグラフの直接トレースは両者とも未実施（静的な位置関係と処理内容からの推論）——確定は PR 本文の挙動プローブへ送る。

  **これは「所見は正しくても機序の説明は独立に誤りうる」の実例である**（ルート `CLAUDE.md`）。所見（絶対パス必須ではない）は最初から正しく、誤っていたのは添えられた機序（git root 正準化）のほうだった。
- **`${CLAUDE_PROJECT_DIR}` の展開は掛からない**（展開は LSP 設定の `command`/`args`/`env`/`workspaceFolder` だけ・証拠 A）。
- **CLI（`claude plugin marketplace add <path> --scope project`）は使わない。** 入力を `path.resolve()` して**絶対パスで書き込む**（逐語: `let s = path.resolve(t.startsWith("~") ? ... : t)` → `{source:"directory", path: s}`）。絶対パスは他マシン・agent worktree で壊れる。→ **`.claude/settings.json` へ相対パスで手書きする。**

### C-3. 却下した第 3 の経路 — inline `settings` marketplace

バイナリには marketplace source `{"source":"settings", "name":..., "plugins":[...]}` が在り、*"Inline marketplace manifest defined directly in settings.json"* と説明されている。パスを一切持たないので魅力的だが、**採らない**——合成 marketplace の root は**キャッシュ配下**（`path.join(<cache>, name)`）に書かれるため、plugin entry の相対 `source` はリポジトリではなくキャッシュを指す。plugin entry の source union に `directory` / 絶対パス形は無い（`npm` / `archive` / `github` / `git` / `command` と marketplace root 相対のパスのみ）。ゆえにリポジトリ内の plugin を指せない。

## 一次証拠 D — RA 側の設定水準（`rust-analyzer.toml` との衝突可否）

`ratoml-workspace-level-only`（#1082 で訂正済み）より、rustc 1.97.1 同梱 RA の解決順:

- **workspace 水準**（`checkOnSave` / `check.*` / `cargo.*` / `workspace.symbol.search.*`）: リポジトリ直下 ratoml → **クライアント設定** → ユーザ ratoml → 既定
- **local 水準**（`diagnostics.*`）: source root を遡る ratoml 群 → **クライアント設定** → ユーザ ratoml → 既定

現行 `rust-analyzer.toml` が書いているのは `workspace.symbol.search.*` と `cargo.targetDir` の 2 つだけで、**`checkOnSave` も `diagnostics.*` も書いていない**。よって `initializationOptions` 経由の両キーは ratoml に潰されず効く（ratoml が書いていないキーはクライアント設定が勝つ）。

## 一次証拠 E — `target/rust-analyzer/` は現時点で不在

`rust-analyzer.toml` の現行コメントは次の双条件を書いている。

> `target/rust-analyzer/` が出来て育てば回しており競合も実在した、出来なければ回しておらず**この設定も `checkOnSave = false` も買う意味が無い**、と判る。

**実測すると当該ディレクトリは存在しない。** つまりコメント自身の論理に従えば「買う意味が無い」という結論になるが、この双条件は不成立である。

- 「出来た」から言えるのは *RA 由来の Cargo work が在った* までで、`cargo check` / build script / proc-macro build を識別できない（issue の指摘のとおり）。
- 「出来ない」からも `cargo check` 不在は導けない——RA が workspace を完全ロードしていない／設定が当該インスタンスに届いていない／VS Code を開いていない、が区別できない。**不在の観測 1 つで全称否定を確定させてはならない**（AGENTS.md「検証の作法」）。
- ゆえに**実効設定は directory の有無ではなく RA のログで測る**（`env` に `RA_LOG` を渡す経路がスキーマに在る・証拠 A）。

## 一次証拠 F — **`rust-analyzer.toml` は Claude Code の RA インスタンスに届いていない**（実測）

敵対枠（3b）が `target/flycheck0/` を見つけたのを起点に、呼び出し側で機序まで裁定した。**採ったのは所見であって、添えられた機序ではない。**

### F-1. flycheck は走っており、通常の `target/` を使っている

`target/flycheck0/stdout`（435 KB・mtime **2026-08-14 10:47**）は `cargo check --message-format=json` の生出力で、artifact のパスは:

```
"filenames":["C:\\workspace\\Snotra\\target\\debug\\build\\proc-macro2-.../build-script-build.exe", ...]
```

→ **flycheck（`cargo check` on save）は実際に走っている**（`checkOnSave` は既定の true）。かつ**書き先は `target/debug/` であって `target/rust-analyzer/` ではない**。つまり `[cargo] targetDir = true` はこの RA インスタンスに効いていない。RA のプロセスはこのセッションの Claude Code（PID 1520）の直下で 9:50 台に起動しており（敵対枠が `Win32_Process` の親子で実測）、ratoml のコミット（2026-08-13）より後である。

### F-2. `workspace.symbol.search` も効いていない（独立な 2 本目の足）

`rust-analyzer.toml` は `kind = "all_symbols"` を書いている。効いているかの検算は既定 `only_types` との差で測れる。

| クエリ | 結果 | 読み方 |
|---|---|---|
| `config` | 14 件・**全部 Struct / Enum**（Function ゼロ） | 型が当たったので関数が落ちている＝`only_types`（**既定**） |
| `update_config` | 2 件・**Function** | 型が 1 件も当たらないときは全シンボルへ落ちる＝`only_types` のフォールバック。**Claude Code 側が型で絞っているのではない**ことの対照 |

`snotra-core/src/engine.rs` には `from_config` / `update_config` / `config_handle` 等が実在する（grep で確認）のに `config` クエリでは 1 件も返らない。→ **`kind = "all_symbols"` は効いていない。**

### F-3. 結論と、書いてよい強さ

**独立な 2 つの workspace 水準キーが、どちらも効いていない。** ゆえに「`rust-analyzer.toml` は Claude Code の RA インスタンスに届いていない」と観測から言える。

**機序は未確定である**（届かない理由が RA 側の ratoml 探索条件なのか、Claude Code のクライアント capability なのか、切り分けていない）。**実効設定は `RA_LOG` で直接測る**——それが F を機序ごと決着させる唯一の手段である。

### F-4. この発見が設計に与える影響

1. **issue の前提の半分がひっくり返る。** 「ratoml は両クライアントに掛かるので、Claude Code だけに効かせる別経路が要る」——非対称性は**すでに存在**しており、しかも向きが逆だった。Claude Code の RA は**素の既定値で動いている**。
2. **`initializationOptions` は「もう 1 つの経路」ではなく、Claude Code の RA に対する唯一の経路である。** ratoml が届かない以上、証拠 D の「ratoml がクライアント設定を上書きする」という衝突の心配も、現状では起きていない（将来届くようになったときのために足 6 のカナリアは要る）。
3. **#1075 が買った `workspace.symbol.search`（`all_symbols` / `limit=512`）は、その受益者であるエージェントに届いていない。** 網羅性が要件の作業のために広げた設定が、広げた当人に効いていない。**plugin の `initializationOptions` へ移せば直る**——今回の器はこの回復も同時に運べる。

## 設計候補と評価

| 案 | 機構 | 評価 |
|---|---|---|
| **A. repo 所有 plugin + repo 内 marketplace（directory source）** | `.claude/settings.json` の `extraKnownMarketplaces` + `enabledPlugins`、plugin root に `.lsp.json` | **本命。** 公式が「repository の team plugin」用途と名指しした経路。repo に全部入り、governance 検査を掛けられる |
| B. `--plugin-dir` 常用 | CLI フラグ | issue が却下。起動手段に依存し再現性が無い |
| C. user ratoml（`%APPDATA%/rust-analyzer/rust-analyzer.toml`） | RA 側 | **不採用。** 全プロジェクトに掛かる／repo 外所有で機械検査不能／`checkOnSave`・`diagnostics.*` は workspace・local 水準ゆえ**クライアント設定より下**で、Claude Code が当該キーを送らない今は効くが、案 A を入れた瞬間に無効化される二重機構になる |
| D. `command` を stdio proxy に向ける shim | 自作 | **不採用。** スキーマが `initializationOptions` を正規に持つ以上、写しを自作する理由が無い（`recommend-native-over-handrolled`） |

### 抑制スイッチの選択（ここが issue からの逸脱点）

issue は 2 つを同時に要求している。**別々に評価する。**

1. **`checkOnSave = false`（`initializationOptions`）— 採用に足る根拠がある**
   - clippy は `post-edit.mjs` が正本（`cargo clippy --workspace --all-targets -- -D warnings`）。RA の flycheck は同じ仕事の重複。
   - `target/` ロックの食い合いを避ける動機は `cargo.targetDir` と同源。
   - ただし**現に flycheck が走っているかは未測定**（証拠 E）。効果の有無は RA ログで測る。

2. **診断の抑制 — 機構が 2 つあり、選択は同じでない**
   - (2a) **Claude Code の `diagnostics: false`**: RA は計算・publish するが、Claude Code が**エージェント context へ注入しない**。navigation は明示的に保持されると docstring が名言。
   - (2b) **RA の `initializationOptions.diagnostics.enable = false`**: RA が診断を**計算しない**。
   - **(2a) を推す。** 理由は 3 つ。(i) issue の動機（編集途中の stale 診断が会話へ入り正しい編集を戻させる）は**注入**の問題であって計算の問題ではない。(ii) (2b) は RA 内部の設定水準・解決順に依存するが、(2a) は Claude Code 側で完結し、将来 ratoml に `diagnostics.*` を書いた誰かに黙って壊されない。(iii) (2a) なら `Diagnostics` を明示的に引く LSP ツール経路が残る（要確認・未確定 3 へ）。

   **採否は「まず測ってから決める」で決着した（ユーザー判断・2026-08-14）。** 根拠は `ra-diagnostics-noise-is-baseline-not-edits`（2026-08-14 実測）——`<new-diagnostics>` の量は編集由来 3〜9 件に対し底値 96 件で、**設定で消せるのは実セッションに出ていない底値のほう**である。同メモリは **`unlinked-file`（`.rs` を作って `mod` 忘れ）は cargo から見えないので必ず残す側**と名指ししており、これは (2a)(2b) のどちらでも失われる。→ **今サイクルは器と `checkOnSave` だけを入れ、`RA_LOG` で実効設定を実測してから診断の扱いを別 issue で決める。** issue の受け入れ条件のうち diagnostics の 2 条件は次サイクルへ送る。

## 敵対的調査（3b）の所見と採否

出力は `workspace/adversarial-1083.txt`。**壊せた項目と壊せなかった項目の両方**が母集団の各項目に札つきで返ってきており、出力契約は満たされている。

### 壊せなかった（＝独立に再現された）

測定環境（Claude Code 2.1.232・RA 1.97.1・user ratoml 不在）、一次証拠 A（`diagnostics` フィールドの実在と逐語）、B（`.lsp.json` の record parse・manifest との重ね順・plugin 認定マーカー・公式 plugin の実体）、C（`extraKnownMarketplaces` の説明・設定優先順位・二重宣言の検知メッセージ・trust gate）、D（`checkOnSave`=workspace / `diagnostics.*`=local / `cargo.targetDir`=workspace）、案 C・案 D の却下理由。**いずれも採用（変更なし）。**

### 壊された・更新を要した項目

| 所見 | 採否 | 理由 |
|---|---|---|
| **`target/flycheck0/` が在り、flycheck は実際に走っている。しかも書き先は `target/debug/` で `targetDir` が効いていない** | **採用**（証拠 F へ昇格） | 一次証拠を自分で読み直して裁定した（stdout の artifact パス）。さらに `workspace.symbol.search` でも同じ不発を独立に確認し、**「ratoml が届いていない」**という一段強い結論へ進めた。**採ったのは所見であって機序の説明ではない** |
| `Xdg` による git root 基準の path 解決（呼び出し側が読んだ loader とは別サイト） | **所見は採用・機序は不採用** | 敵対枠自身が追記で訂正した——`Xdg` は reconciliation 経路であって marketplace.json の読み込み主経路ではない。**cwd 依存は緩和されない。** 「所見は正しくても機序の説明は独立に誤りうる」の実例として C-2 に残す |
| 追記での再検証（validate が `.lsp.json` を見ないことを 3 パターンの実行で独立再現ほか） | **採用** | 呼び出し側の実測をバイナリ読解ではなく実行結果で裏づけた。**`idle` は書き終わりであって最終見解ではない**——催促後の追記に、本人の先行報告への訂正が 1 件含まれていた |
| 「docs とバイナリが一致」は逐語では偽（言い換えが複数ある） | **採用** | 「意味は同一・逐語は不一致」へ弱めた。自分たちの「全称表現は前提条件とセットで書く」に照らして正しい指摘 |
| `diagnostics` フラグが実行時に消費されている（`xlp` の gate） | **採用** | 「死んだスキーマではない」ことの補強。ただし navigation が生きることの実行時検証は未実施（⚠️ ごと記録） |
| 環境変数 `AI_AGENT` が 1 世代前（2.1.229）を指す ⚠️ | **不採用（記録のみ）** | 実行バイナリは `CLAUDE_CODE_EXECPATH` と `--version` の両方が 2.1.232 で一致しており、「調べたバイナリが違う」という懸念は裏づかなかった |
| 「`targetDir` の切り分けを #1083 より手前の前置タスクにすべき」 | **不採用** | 証拠 F が反転させた——`initializationOptions` が Claude Code の RA への唯一の到達経路である以上、**plugin こそが修理経路**であり、前に置くべき別タスクは無い |
| rust-analyzer 本家 `config.rs` は master を参照しており 1.97.1 のタグではない ⚠️ | **採用（限定つき）** | 水準分類の一次証拠は #1082 が rustc 1.97.1 同梱版で取ったものが既に在り、master 参照はその追認として扱う |

### 未検証と宣言された項目（対象外の宣言も契約どおり）

`selectChecks` の扱い・`post-edit.test.mjs` の網羅性・`governance:check` の挙動・`safety-nets.md` の該当箇所——いずれも計画段階の判断として呼び出し側が担当する（本計画の Phase 2 が対応する）。案 B（`--plugin-dir`）の評価は issue の既存判断の追認であり検証していない、と宣言された。

## 関連ファイル・シンボル（実在を確認済み）

| パス | 役割 | 今回の関わり |
|---|---|---|
| `.claude/settings.json` | hooks + `enabledPlugins` | `extraKnownMarketplaces` 追加・公式 plugin の `false` 化 |
| `.claude/hooks/post-edit.mjs` | `selectChecks` が SSOT | 新設ファイルへの検査割り当て（沈黙 = 未検査を作らない） |
| `.claude/hooks/post-edit.test.mjs` | `selectChecks` の網羅テスト | 期待値追加 |
| `rust-analyzer.toml` | RA の workspace/local 設定 | `targetDir` 検算コメントの訂正 |
| `.vscode/settings.json`（gitignore 対象・ローカルに実在） | VS Code の RA クライアント設定 | 変更しない。`cargo.allTargets=false` / `check.allTargets=false` / `cachePriming.enable=false` / `lru.capacity=192` を持つ |
| `scripts/` + `npm run governance:check` | ガバナンス検査 | 新設 JSON の検査層をどこに置くか |
| `.claude/rules/safety-nets.md` | セーフティネット変更時の作法 | 検査層の責務分離の判断根拠 |

## 技術的制約

- **LSP は起動時に初期化される。** `initializationOptions` の変更を測るには Claude Code セッションの再起動が要る。ratoml も「反映は再起動時のみ」（2026-08-13 実測）。→ 検証項目はセッション内で閉じない。
- **project scope の plugin は trust 後に load される。** clone 直後の初回セッションでは trust dialog を通るまで LSP が上がらない。
- **`initializationOptions` に変数展開は掛からない**ので、パスを含む設定は書けない（今回は不要）。
- **CI では検証できない。** Claude Code の LSP 起動は CI に無い。機械検査で守れるのは「JSON が parse でき、意図したキーが意図した値である」ところまで。実効値は手元の RA ログでしか測れない。

## 未解決の疑問（→ `plan.md` の未確定欄へ）

1. ~~`extraKnownMarketplaces` の `directory` source の `path` は何に対して解決されるか。~~ **決着済み（証拠 C-2）**: `path.resolve()` で cwd 基準。相対パスを手書きすれば可搬。CLI は絶対パスを書き込むので使わない。
2. **`checkOnSave` の実効値と、flycheck が実際に走っているか。** `env` に `RA_LOG=rust_analyzer=info` を渡して測る。**セッション再起動が要るので、計画の未確定欄ではなく PR 本文のチェックリストへ送る**（`.claude/rules/safety-nets.md` の「CI の実測は PR が在って初めて行える」と同型の循環）。
3. **`diagnostics: false` にしたとき、LSP ツール経由で診断を明示的に取得する経路が残るか。** 残らないなら `unlinked-file` の検出手段が消える。
4. **`unlinked-file` 相当を別の層で守れるか。** 現状は「`.rs` 追加時に `CLAUDE.md` のモジュール索引を更新する」という規範のみで、機械検査は無い（`AGENTS.md` 条件別チェック表）。`cargo build` は `mod` 未宣言のファイルを見ない。
5. **新設 JSON の検査をどの層へ置くか。** `post-edit.mjs` の `selectChecks`（編集時・沈黙 = 合格）か、`governance:check`（PR CI）か、両方か。抑制キーが消えたことを捕まえるカナリアの置き場所も同じ判断に含まれる。

## ユーザー確認の結果（「何を作るか」に関わる・決着済み）

**問い**: 診断の抑制にどこまで踏み込むか（`checkOnSave` だけ / ＋ Claude Code 側の `diagnostics:false` / issue のとおり RA 側で止める / まず測る）。
**回答**: **「まず測ってから決める」**。→ 今サイクルの成果物は plugin の器と `checkOnSave=false` まで。`RA_LOG` による実効設定の実測は PR 本文のチェックリストへ送り、診断の扱いは実測後に別 issue で決める。

## 追加の実測（計画確定前に潰した項目）

| 測ったこと | 結果 |
|---|---|
| `claude plugin validate <path> --strict` の存在と振る舞い | 実在。マニフェストを検証し、warning も exit 1 にする（公式 marketplace で 30 warnings → exit 1） |
| スクラッチの器が validate を通るか | 通る。`marketplace.json` は `name`/`description`/`owner`/`plugins[]`、entry は `name`/`source`/`description`/`version`/`author`。`plugin.json` は `name`/`description`/`version`/`author` |
| **validate は `.lsp.json` を守るか** | **守らない。** JSON を壊しても抑制キーを消しても `✔ Validation passed` / exit 0（2 変異とも）。→ 自前カナリアが必須 |
| `checkOnSave` が RA 1.97.1 に実在するか | `rust-analyzer.exe`（toolchain 実体・40 MB）の文字列に 3 件 |
| カナリアが CI にも届くか | `vitest.config.ts` の include は `.claude/hooks/**/*.test.mjs`、`ci.yml` は ubuntu / windows の両方で `npm test` を実行する |
