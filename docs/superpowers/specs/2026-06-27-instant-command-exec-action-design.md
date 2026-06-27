# インスタントコマンドに `exec`(exe+args)種別を追加

- 日付: 2026-06-27
- ステータス: 設計合意済み（実装計画 writing-plans へ）
- 関連: SPEC.md §19、`snotra-core/src/instant.rs`、`snotra-core/src/config.rs`、`src-tauri/src/commands/instant.rs`、`src-tauri/src/commands/launch.rs`、`snotra-settings/src/tabs/instant.rs`、`ui/src/stores/search.ts`

## 1. 背景と問題

インスタントコマンドに `C:\Users\Eoh\scoop\shims\everything.exe -s {query}` のような「実行ファイル + 引数」を登録すると、呼び出しが失敗する。

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

- **`InstantCommand` と `OpenerTool` のデータ型統合はしない**。両者はトリガー（`@`プレフィックス vs パス選択）・プレースホルダ（`{query}`/`{clip}` vs `{path}`）・マッチング（名前前方一致 vs 拡張子/フォルダルール）が本質的に異なる。統合は別概念の混在を生む。共通化するのは「実行の仕組み（引数分割）」であってデータ型ではない（§5）。

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
    Url  { url: String },                                  // ShellExecuteW 系
    Exec { exe: String, #[serde(default)] args: String },  // Command::new 系
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

### serde 表現の懸念と退避策

`#[serde(flatten)]` + `#[serde(untagged)]` + `toml` クレートの三者は相性問題の報告がある。**実装の最初のタスクを「この serde 表現の round-trip テストを書いて通すこと」とし、ここが渋れば次へ退避する**:

- 退避A: 内部タグ方式 `#[serde(tag = "kind")]`（TOML に `kind = "url"|"exec"` が1行増えるが堅牢・自己説明的）
- 退避B: フラットな `Option<String>` 群（`url` / `exe` / `args`）+ 実行時バリデーション（`url` と `exe` の排他を `ConfigError` で検査）

いずれの退避でも §4 以降の方針（移行・実行・UI）は不変。

## 4. レガシー移行（`apply_migrations`）

既存 config の `command = "…"` を **無改変で `Url { url }` へ移送**する。これがゼロ回帰の鍵:

- 現状は非URLコマンドも含め全て `ShellExecuteW(command 全体)` を通る。`Url` 種別はその挙動と完全に等価
- ゆえに「旧 `command` → `url`」は、今日動いているコマンド（URL・引数なし exe・スペース入りパス）を1つも壊さない
- 唯一壊れていた「引数つきコマンド」は今日も壊れているので回帰ではない。ユーザーが新UIで `exe`+`args` 形へ作り直す（一度きり）
- **自動分割はしない**（コマンドライン文字列の exe/args 自動分割は、スペース入りパスや document-shell-open を誤って壊しうるため）

実装はプロジェクト既存パターン踏襲:

- `#[serde(default, skip_serializing)] command: Option<String>` を残す
- `apply_migrations()` で `self.<legacy>.command.take()` → `Url { url }` を構築
- `Config::default()` の明示初期化にも legacy `command: None` を追加
- `Config::load()` 以外のデシリアライズ経路（インポート等）でも `apply_migrations()` が走ることを確認

## 5. 実行フロー（`src-tauri/src/commands/instant.rs`）

`execute_instant_command` を種別でディスパッチ:

- **`Url`** → `expand_instant_command`（http(s) は URL エンコード）→ `launch_item_core`（ShellExecuteW）。**現状経路そのまま**
- **`Exec`** → `exe`/`args` の `{query}`/`{clip}` を展開 → `Command::new(exe).args(tokens)` で spawn

### 引数展開の順序（重要・テスト対象）

`args` は「**分割してから各トークンを展開**」する:

- `args = "-s {query}"`、query = `hello world` → `split_args` → `["-s", "{query}"]` → 展開 → `["-s", "hello world"]` → 「hello world」は1引数のまま
- 逆順（展開→分割）だと query 内の空白で引数が割れる。オープナーの `build_launch_args`（`{path}`）と同じ流儀

## 6. 共通化（健全な DRY の範囲）

データ型は統合しないが、引数分割の純粋ロジックを共有する:

- `split_args`（クォート対応分割）を `launch.rs` から `snotra-core` へ移設し、オープナーとインスタントの両者が呼ぶ。純関数なのでユニットテストを集約できる
- `Command::new(exe).args(...)` の spawn 自体は副作用なので `src-tauri` 側に残す
- オープナー固有の「`{path}` 補完」（`build_launch_args`）は openers 専用のまま。インスタントは補完せず `{query}`/`{clip}` のみ展開

## 7. IPC とフロントエンド

種別の内部構造をフロントへ漏らさぬよう、オープナー（`OpenerToolDto`）に倣い表示用 DTO を噛ませる:

```rust
struct InstantCommandDto { name: String, description: String, display: String }
// display = url、または "exe args" の表示文字列（バックエンドで生成）
```

- `get_instant_commands` は `Vec<InstantCommandDto>` を返す（種別分岐をバックエンドで吸収）
- フロント変更は実質2点:
  - `ui/src/lib/types.ts` の `InstantCommand.command` → `display` に改名
  - `ui/src/stores/search.ts:301` を `cmd.description || cmd.display` に
- `execute_instant_command(name, query)` は名前ディスパッチのまま不変

## 8. 設定UI（`snotra-settings/src/tabs/instant.rs`）

モーダルに種別トグル（ラジオ: URL / プログラム）を追加し、フィールドを出し分ける:

- URL 選択 → `url` 単一フィールド + プレビュー（`expand_instant_command`）
- プログラム選択 → `exe`（オープナー同様のファイルピッカー流用可）+ `args` + プレビュー（`exe` に展開後 `args`）
- `ModalState` に `edit_kind` / `edit_url` / `edit_exe` / `edit_args` を追加。`save_instant_command` を種別に応じて `InstantAction` 構築へ変更
- i18n（`snotra-settings/src/i18n.rs`、Ja/En 両方）: `label_instant_kind`・ラジオ2種・`label_instant_exe`/`label_instant_args`・各ヒントを追加

## 9. SPEC.md §19 更新

挙動変更を伴うため、AGENTS.md ステップ0「仕様内部の矛盾解消」として SPEC を同期する:

- §19.2 設定構造: 新 TOML 2形態（`url` / `exe`+`args`）
- §19.4 変数展開: 種別別（URL は http(s) エンコード、exec は生展開）
- §19.6 実行フロー: 種別ディスパッチ（url→ShellExecuteW / exec→`Command::new`）
- §19 内の子セクション番号・後続セクション番号がずれていないか確認

## 10. テスト計画（TDD）

優先順:

1. **serde round-trip**（§3 の最優先）: `Url` / `Exec` 両形態の TOML ↔ 構造体の往復一致。退避判断のゲート
2. **移行**: legacy `command`（URL / 非URL 両方）→ `Url { url }`。旧データで動いていた挙動が保たれること
3. **`split_args` 移設後テスト**: 既存テストを `snotra-core` 側へ移し、クォート/空クォート/未閉クォートを維持
4. **exec の分割→展開順序**: `args="-s {query}"` + 空白入り query が1引数を保つ
5. **バリデーション**（退避B採用時のみ）: `url` と `exe` の排他、両方空の拒否

## 11. 影響範囲

- **触る**: `snotra-core/src/config.rs`（struct/enum/migration/default/validate）、`snotra-core/src/instant.rs`（expand は維持、必要なら exec 用ヘルパー）、`snotra-core`（`split_args` 移設先）、`src-tauri/src/commands/instant.rs`（ディスパッチ）、`src-tauri/src/commands/launch.rs`（`split_args` 移設元）、`snotra-settings/src/tabs/instant.rs`、`snotra-settings/src/i18n.rs`、`ui/src/lib/types.ts`、`ui/src/stores/search.ts`、`SPEC.md §19`
- **モジュール構成ドキュメント同期**: `snotra-core/CLAUDE.md`（instant.rs / split_args 移設）、必要なら `docs/architecture.md`（instant の実行系統が2つになる横断パターン）
- **触らない（根拠）**: E2E（`e2e/` に instant 参照なし=無風）、`execute_instant_command` の IPC シグネチャ（name/query のまま）、オープナーの実行経路（`build_launch_args` の `{path}` 補完は openers 専用で不変）
- **件数・キー形式変更なし**: 識別子改名（`command` → `url`/`exe`/`args`）はシンボル単位で grep 済み（config.rs 構造体、settings モーダル、フロント型1+表示1、serde キー）。compile-fail を改名検出器として使う

## 12. リスク

- serde（flatten + untagged + toml）の相性 → §3 の退避策で吸収。実装初手で round-trip テストにより de-risk
- 設定UI のラジオ追加に伴うモーダル状態の増加 → 既存の Create/Edit パターン・境界チェックを踏襲
