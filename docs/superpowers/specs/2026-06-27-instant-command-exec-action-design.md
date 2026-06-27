# インスタントコマンドに `exec`(exe+args)種別を追加

- 日付: 2026-06-27
- ステータス: 設計合意済み（多観点レビュー反映済み・実装計画 writing-plans へ）
- 改訂: 初版を user/implementer/QA の3観点サブエージェントレビューで検証し、致命的欠陥（serde レガシー移行のデータ損失）と高価値の訂正を反映。任意拡張3件（移行発見ヒント・コンソール窓抑止・環境変数展開）をスコープに追加。
- 関連: SPEC.md §19、`snotra-core/src/instant.rs`、`snotra-core/src/config.rs`、`src-tauri/src/commands/instant.rs`、`src-tauri/src/commands/launch.rs`、`snotra-settings/src/tabs/instant.rs`、`ui/src/stores/search.ts`

## 1. 背景と問題

インスタントコマンドに `C:\Users\Eoh\scoop\shims\everything.exe -s {query}` のような「実行ファイル+引数」を登録すると、呼び出しが失敗する。

### 根本原因

`execute_instant_command`（`src-tauri/src/commands/instant.rs`）は `{query}` 展開後の文字列を `launch_item_core` に渡し、`launch_item_core`（`launch.rs:332`）は `ShellExecuteW` を次のように呼ぶ:

```rust
ShellExecuteW(None, "open", path /* = 展開文字列全体 */, None /* lpParameters */, None, SW_SHOWNORMAL)
```

`ShellExecuteW` の `lpFile` は「開く対象の単一ファイルパス」であり、空白以降を引数として切り出さない。引数は本来 `lpParameters` へ分離して渡す契約である。ところがここでは exe と引数が `lpFile` に貼り付き、`lpParameters` は `None`。結果、OS は `…everything.exe -s test` という名前のファイルを literally 探し、存在しないため `SE_ERR_FILE_NOT_FOUND`（コード 2）で失敗する。

### 動く例と動かない例の境界（診断の裏取り）

| 登録例 | `lpFile` | 結果 |
|---|---|---|
| `https://…?q={query}` | `https://…?q=test` | ✅ URL は単一の開く対象として正当 |
| `C:\tools\editor.exe`（引数なし） | `C:\tools\editor.exe` | ✅ 実在する単一パス |
| `everything.exe -s {query}`（引数あり） | `…everything.exe -s test` | ❌ そんな名前のファイルは無い |

分かれ目は「実行ファイルの後ろに引数が付くか」のみ。scoop shim は正規の実行ファイルであり無罪。原因は引数が `lpFile` に混入していること一点。

### 対照（オープナーは壊れていない）

オープナーは `OpenerTool { name, exe, args }` と構造を分け、`launch_with_tool_core`（`launch.rs:143`）が `std::process::Command::new(exe).arg(...)` + `split_args`/`build_launch_args` で引数を正しく分離する。インスタントコマンドだけがこの恩恵を受けていない。

## 2. 決定

インスタントコマンドが実行系統を **2つ** 持つことを、データモデルで明示する（種別を型で表現する = illegal state を表現不可能にする）。

- **URL 系**（shell-open）: `ShellExecuteW` で既定アプリ/ブラウザに開かせる。`exe`+`args` の形を持たない（1本の文字列）
- **プログラム系**（exec）: `exe` と `args` を分離し `Command::new(exe).args(...)` で起動

### 非目標（YAGNI）

- **`InstantCommand` と `OpenerTool` のデータ型統合はしない**。両者はトリガー（`@`プレフィックス vs パス選択）・プレースホルダ（`{query}`/`{clip}` vs `{path}`）・マッチング（名前前方一致 vs 拡張子/フォルダルール）が本質的に異なる。共通化するのは「実行の仕組み（引数分割）」であってデータ型ではない（§6）。

## 3. データモデル（`snotra-core/src/config.rs`）

```rust
pub struct InstantCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub action: InstantAction,
}

#[serde(untagged)]
pub enum InstantAction {
    Url    { url: String },                                  // ShellExecuteW 系
    Exec   { exe: String, #[serde(default)] args: String },  // Command::new 系
    Legacy { command: String },                              // 旧形式互換（§4 で必ず Url 化）
}
```

TOML 表現:

```toml
[[instant_commands]]
name = "g"
url = "https://www.google.com/search?q={query}"

[[instant_commands]]
name = "ev"
exe = "C:\\Users\\Eoh\\scoop\\shims\\everything.exe"
args = "-s {query}"
```

### 3.1 レガシー戦略の確定（レビュー反映: これがデータ損失防止の核心）

初版は `command: Option<String>` の named フィールド + `Option` でない `flatten` という、**serde 上両立しない**表現だった。これを実装すると、既存 config の `command = "…"` 行が新構造体に**デシリアライズできず**、失敗が `Config` 全体の parse 失敗へ伝播し、`config.toml` が `.bak` に退避されて**全設定が既定値リセット**される（QA が `toml 1.1.2` で実証）。`apply_migrations` は parse の後に走るため移行は一度も実行されない。

確定方針:

- **`InstantAction` に `Legacy { command }` variant を追加**し、`apply_migrations` で `Url` へ置換する。`flatten` の buffer に集まる `{command: "…"}` を untagged が Legacy として拾う。`None` を生まないため §2 の「illegal state を表現不可能に」を保つ。Legacy は移行後メモリ上で必ず Url 化されるため**保存時には存在しない**（`skip_serializing` の named フィールドは不要）
- **却下案**: `#[serde(flatten)] action: Option<InstantAction>`（command フィールドなし）は、legacy 行を `None` にして command 値を**取りこぼす**（QA 実証のハザード。移行できず無言で消える）。採らない
- **退避B（QA 実証済みフォールバック）**: フラットな `Option<String>` 群（`url`/`exe`/`args`/`command`）+ 実行時バリデーション（`url` と `exe` の排他を `ConfigError` で検査）。**§3.2 のゲートで Legacy variant が toml で deserialize できないと判明した場合のみ採用**。serialize/deserialize は QA 実証で素直に通る

### 3.2 serde round-trip ゲート（release gate・実装の最初のタスク）

新フォーマットの serialize/deserialize は `toml 1.1.2` で健全と実証済み。問題は**レガシー deserialize**。以下を**ゲート条件**とし、最初に書いて通す。ここが false-green を防ぐ:

- **T1（serialize 往復）**: `Config` 全体を `toml::to_string_pretty` → `from_str` → 等価、かつ変種が保たれる（`matches!(action, InstantAction::Exec{..})`）。`save_to_dir` は `Config` 全体を serialize するため、失敗すると instant に限らず**全設定保存が壊れる**
- **T2（legacy deserialize・最重要）**: 旧 `command =` 行を含む `Config` の `from_str` が `Ok`。**失敗 = 全設定リセットのデータ損失**。新フォーマット往復だけでは検出できないので独立ゲートにする
- **T3（ハンド編集の曖昧性）**: `url` と `exe` を両方書いた行は untagged が先頭 `Url` を採用し `exe` を黙殺する。この「url 先勝ち」をテストで固定（§2 の型方針上 validate 排他は不要）

T2 が渋れば即・退避B（QA 実証済み）へ。

## 4. レガシー移行（`apply_migrations`）

既存 config の `command = "…"` を **無改変で `Url { url }` へ移送**する。これがゼロ回帰の鍵:

- 現状は非URLコマンドも含め全て `ShellExecuteW(command 全体)` を通る。`Url` 種別はその挙動と完全に等価
- ゆえに「旧 `command` → `url`」は、今日動いているコマンド（URL・引数なし exe・スペース入りパス）を1つも壊さない
- 唯一壊れていた「引数つきコマンド」は今日も壊れているので回帰ではない。ユーザーが新UIで `exe`+`args` 形へ作り直す（§8 の移行発見ヒントが導線）
- **自動分割はしない**（コマンドライン文字列の exe/args 自動分割は、スペース入りパスや document-shell-open を誤って壊しうるため）

実装:

- `apply_migrations()` で `if let InstantAction::Legacy { command } = &cmd.action { cmd.action = InstantAction::Url { url: command.clone() } }`
- **冪等**（移行後 `Legacy` は消えるので再実行で no-op。T17 で検証）
- `Config::default()` の2リテラル（`config.rs:758-769`）を `action: InstantAction::Url { url: … }` へ書き換える
- `Config::load()` 以外のデシリアライズ経路（インポート等）でも `apply_migrations()` が走ることを確認（`reset_to_default` 含む）

## 5. 実行フロー（`src-tauri/src/commands/instant.rs`）

`execute_instant_command` を種別でディスパッチ:

- **`Url`** → `expand_instant_command`（http(s) は URL エンコード）→ `launch_item_core`（ShellExecuteW）。**現状経路そのまま**
- **`Exec`** → 引数を構築 → `Command::new(exe).args(tokens)` で spawn

### 5.1 実行ハーネスの対称性（レビュー反映）

Exec も Url 経路・`launch_with_tool` と同じく **`spawn_blocking` + `timeout(LAUNCH_TIMEOUT_MS)`** に載せる（CreateProcessW の PATH 解決等で一瞬ブロックしうるため対称に）。COM STA は不要（`Command::new`/CreateProcessW は COM 非依存。`src-tauri/CLAUDE.md`「EXE は COM 不要」と一致）。

### 5.2 引数構築の順序（重要・テスト対象）

1. `split_args(args)` で**先に分割**（クォート対応・core 純関数）
2. 各トークンに対し **(a) 環境変数展開 → (b) `{query}`/`{clip}` 置換** の順
   - **外部入力（query/clip）は環境変数展開しない**: env 展開を置換の前に行うことで、ユーザーが書いた template の `%VAR%` のみ展開され、`{query}` が運んだ `%VAR%` は生のまま渡る（安全側）
   - env 値の空白はトークン内に留まる（split 後に展開するため再分割されない。例: `%PROGRAMFILES%` の空白が引数を割らない）
   - 分割→置換順により、空白入り `{query}` は1引数を保つ（`args="-s {query}"`, query=`hello world` → `["-s","hello world"]`）
3. `exe` 自体にも環境変数展開を適用（`%LOCALAPPDATA%\…\app.exe` 等）

### 5.3 コンソール窓抑止（スコープ追加）

Exec spawn に `.creation_flags(CREATE_NO_WINDOW=0x08000000)` + `Stdio::null()`（stdin/out/err）を付け、CLI ツール（yt-dlp/ffmpeg/git 等）登録時の黒窓ちらつきを防ぐ。**exec のみ**（オープナーは現状維持＝今回スコープ外）。GUI exe（everything.exe）には無影響。

### 5.4 失敗フィードバック（レビュー反映）

spawn 失敗時は `LaunchResult::failed(-1, "spawn_failed: {e}")` を返し、フロントの `executeInstantCommandSelected` → `notifyLaunchFailure`（`search.ts`）が可視化する（既存導線。Url 経路の `shell_execute_error_message` と対称）。握り潰さない。

## 6. 共通化とヘルパー（健全な DRY）

データ型は統合しないが、純粋ロジックを `snotra-core` に集約してテスト可能にする:

- **`split_args`**（クォート対応分割）を `launch.rs` から `snotra-core` へ `pub` 移設。オープナー（`build_launch_args`）とインスタントが共有。テストも移設（§10）。src-tauri → snotra-core の単方向依存なので循環なし
- **`expand_vars(template, query, clip) -> String`**（core 純関数・生展開）を新設。`expand_instant_command` は `expand_vars` + http(s) の URL エンコード分岐に分解する（Exec のトークンに `expand_instant_command` を流用すると無意味な http 判定が走るため分離）
- **`expand_exec_args(args, query, clip, env_expand: impl Fn(&str) -> String) -> Vec<String>`**（core 純関数）。`split_args` → 各トークンに `env_expand` → `expand_vars` を適用。`env_expand` を注入することで Win32 非依存にテスト可能（テストは恒等関数や擬似 env マップ、本番は §6 の Win32 ラッパ）。`build_launch_args` の `{path}` 自動補完は**流用しない**（exec は末尾 append しない）
- **spawn / Win32**（`ExpandEnvironmentStringsW`, `creation_flags`, `Stdio`）は `src-tauri` に残す（副作用境界）。設定UIのプレビューも `expand_exec_args`/`expand_vars` を**共用**し、実行時と表示の乖離を防ぐ

## 7. IPC とフロントエンド

種別の内部構造をフロントへ漏らさぬよう、オープナー（`OpenerToolDto`）に倣い表示用 DTO を噛ませる:

```rust
struct InstantCommandDto { name: String, description: String, display: String }
```

- `display` 生成規則: `Url` → `url`／`Exec`(args 有) → `"exe args"`／`Exec`(args 空) → `exe`（**末尾スペースなし**）。生テンプレート（`{query}` リテラルのまま・URL エンコードしない。現行 `search.ts` の副表示と一致）
- `get_instant_commands` は `Vec<InstantCommandDto>` を返す（種別分岐をバックエンドで吸収）
- フロント変更（**`InstantCommand` 型に限定**。`SlashCommand.command` は別型・触らない）:
  - `ui/src/lib/types.ts` の `InstantCommand.command` → `display`
  - `ui/src/stores/search.ts:301` を `cmd.description || cmd.display`
  - `ui/src/stores/search.test.ts:75-76` のフィクスチャ（`command:` → `display:`）
- `execute_instant_command(name, query)` は名前ディスパッチのまま不変

## 8. 設定UI（`snotra-settings/src/tabs/instant.rs`）

モーダルに種別トグル（ラジオ: URL / プログラム、**既定 URL**）を追加し、フィールドを出し分ける:

- URL 選択 → `url` 単一フィールド + プレビュー（`expand_instant_command`）
- プログラム選択 → `exe` + `args` + プレビュー（`expand_exec_args`/`expand_vars` を共用）
- **exe ファイルピッカーは `["exe"]` 限定**（オープナーの `["exe","bat","cmd"]` を流用しない）。`Command::new` は `.lnk` を起動できず、`.bat`/`.cmd` は cmd.exe 経由で `{query}` がメタ文字に晒される（§12）。スクリプトは「インタプリタを exe に指定」のヒントを添える
- `ModalState` の全フィールド波及: `edit_kind`/`edit_url`/`edit_exe`/`edit_args`。`open_create_from`/`open_edit`/`save_instant_command` を `action` 構築へ。リスト行表示（`instant.rs:141-145` の `&cmd.command`）も `action` から `display` 相当を導出
- i18n（`snotra-settings/src/i18n.rs`、Ja/En 両方）: `label_instant_kind`・ラジオ2種・`label_instant_exe`/`label_instant_args`・各ヒント。既存 `label_instant_command`/`hint_instant_command` は URL 用に転用

### 8.1 移行発見ヒント（スコープ追加・非破壊）

設定リストの各 `Url` 種別行で、`url` が **http(s):// で始まらず空白を含む**（=引数つき exe らしい）場合、注意バッジ + 「プログラム種別へ作り直しますか?」のヒントを表示する。**自動変換はしない**（§4 のゼロ回帰を保つ）。旧コマンドを黙って url 化するだけだと「直したはずなのに動かない」混乱が残るため、気づきの導線を1つ足す。settings UI のみの変更（バックエンド無改変）。i18n 文字列を追加。

## 9. SPEC.md §19 更新

挙動変更を伴うため、AGENTS.md ステップ0「仕様内部の矛盾解消」として SPEC を同期する:

- §19.2 設定構造: 新 TOML 2形態（`url` / `exe`+`args`）。既存の `command =` 例（行 684-696）を更新
- §19.4 変数展開: 種別別（URL は http(s) エンコード、exec は env 展開→生展開）。環境変数展開を追記
- §19.5 マッチングと結果表示: 副表示の「コマンドテンプレート」→ `display`（url または `exe args`）
- §19.6 実行フロー: 種別ディスパッチ（url→ShellExecuteW / exec→`Command::new` + spawn_blocking/timeout/CREATE_NO_WINDOW）
- §19.8 設定画面: `name`/`command`/`description` のフィールド列挙を kind ラジオ + url/exe/args へ
- §19 内の子セクション番号・後続セクション番号のずれを確認

## 10. テスト計画（TDD）

### 10.1 serde / 表現（§3.2 のゲート）
- **T1** serialize 往復（`Config` 全体）+ 変種保存
- **T2** legacy 行 deserialize → `Ok`（最重要・データ損失検出器）
- **T3** ハンド編集 `url`+`exe` 両方 → `Url` 先勝ち固定
- **T4** `Exec` で `args` 省略 → `args == ""`
- **T5** `description` 省略 + Exec/Url 混在 Vec の往復等価
- **T17** `apply_migrations` 冪等（legacy→Url 後に再実行で `changed==false`）

### 10.2 移行回帰
- **T15** 非 URL legacy（`C:\tools\editor.exe`, `readme.pdf`）が `Url` へ移行し ShellExecuteW 経路へ（`Exec` に**しない**＝自動分割しない不変条件をロック）
- 既存 `instant_command_round_trip_toml`（`config.rs:2836`）を、移行後の `action` 変種を assert する形へ書き換え（転用で不変条件を孤立させない）

### 10.3 exec 引数構築（`expand_exec_args` 純関数。`env_expand` は恒等/擬似マップ注入）
- **T7** 空 args → `[]`（末尾 append しない＝`build_launch_args` 流儀の流用回帰を防ぐ）
- **T8** 順序兼インジェクション検出: `args="-s {query}"`, query=`--flag a b` → `["-s","--flag a b"]`（1引数）
- **T9** `{query}` 内の `"`: query=`a"b` → `["a\"b"]`（split は展開前なので再分割しない）
- **T10** `{clip}` 内の改行: clip=`a\nb` → `["a\nb"]`（再分割しない）
- **T11** 空 `{query}`: `args="-s {query}"`, query=`""` → `["-s",""]`（空引数を渡す仕様をロック）
- **T12** インライン placeholder: `args="-s={query}"`, query=`hello world` → `["-s=hello world"]`
- **T16** 環境変数: `args="--dir %FOO%"`, env `FOO=C:\a b` → `["--dir","C:\a b"]`（env 値の空白がトークン内に留まる）/ 外部入力 query=`%FOO%` は展開されない
- **split_args** 既存テスト（クォート/空クォート/未閉クォート）を core へ移設して維持

### 10.4 DTO 表示
- **T14** `display`: Url→`url`／Exec(args 有)→`"exe args"`／Exec(args 空)→`exe`（末尾空白なし）

### 10.5 プレビュー乖離防止
- 設定UIプレビューが `expand_exec_args` を共用し、実行時の分割結果と一致（同一純関数）

## 11. 影響範囲

- **触る（Rust）**: `snotra-core/src/config.rs`（struct/enum/`apply_migrations`/`default()` リテラル/round-trip テスト/`InstantCommand` 移動）、`snotra-core/src/instant.rs`（`expand_vars`/`expand_exec_args` 新設・**テストフィクスチャ `command:` リテラル 行123-128,165** 書き換え）、`snotra-core`（`split_args` 移設先）、`src-tauri/src/commands/instant.rs`（ディスパッチ/env展開/creation_flags）、`src-tauri/src/commands/launch.rs`（`split_args` 移設元）
- **触る（UI/設定）**: `snotra-settings/src/tabs/instant.rs`（ModalState 全フィールド/リスト行/移行ヒント）、`snotra-settings/src/i18n.rs`、`ui/src/lib/types.ts`、`ui/src/stores/search.ts:301`、`ui/src/stores/search.test.ts:75-76`、`SPEC.md §19.2/§19.4/§19.5/§19.6/§19.8`
- **モジュール構成ドキュメント同期**: `snotra-core/CLAUDE.md`（instant.rs ヘルパー・split_args 移設）、必要なら `docs/architecture.md`（instant の実行系統が2つになる横断パターン）
- **触らない（根拠）**: E2E（`e2e/` に instant 参照なし=無風・grep 0件実証）、`execute_instant_command` の IPC シグネチャ（name/query のまま）、`SlashCommand.command`（別型・`search.ts:334/336`・素朴な一括 rename は厳禁）、オープナーの実行経路（`build_launch_args` の `{path}` 補完は openers 専用で不変・コンソール窓抑止も今回適用しない）
- **改名は概念単位で grep 済み**: `InstantCommand.command`（型参照に限定）/ config 構造体 / settings モーダル+リスト行 / フロント型1+表示1+テスト1 / serde キー。compile-fail を改名検出器に使う

## 12. リスクとセキュリティ

- **serde（flatten + untagged + toml）**: §3.2 のゲートで de-risk。T2 が渋れば退避B（QA 実証済み）
- **`.bat`/`.cmd` 経由のインジェクション**: `Command::new` が `.bat`/`.cmd` を解決すると cmd.exe 経由になり、信頼境界外の `{query}`/`{clip}` が cmd.exe のメタ文字・`%VAR%` に晒される。Rust ≥1.77.2 が CVE-2024-24576（BatBadBut）対策でバッチ引数をエスケープするが、(a) ツールチェーン ≥1.77.2 を確認、(b) **exe ピッカーを `["exe"]` 限定**して UI 経路で防止、(c) 手編集 `.bat`/`.ps1` は残存リスクとしてドキュメント明記。真の `.exe` では `%VAR%`/`&`/`|` は不活性（シェル非経由）で安全
- **bare/相対 exe のバイナリプランティング**: `Command::new` は PATH と**プロセス cwd** で解決するため、cwd 直下の同名 exe を拾う余地。exe はユーザー定義=信頼だが、UI ピッカーは絶対パスを入れる。「exe は絶対パス推奨」を一言
- **環境変数展開の順序**: 外部入力（query/clip）は env 展開しない（§5.2）。template の `%VAR%` のみ展開
- **設定UI モーダル状態の増加**: 既存 Create/Edit パターン・境界チェック（`if idx < vec.len()`）を踏襲
