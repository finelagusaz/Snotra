# Plan — インスタントコマンド機能

仕様: `SPEC.md` §18

## 概要

検索ボックスにプレフィックス（デフォルト `@`）を入力すると、ユーザー定義の任意コマンドを即座に実行できる機能。
fenrir のインスタントコマンド（`instant.ini`）に相当。

## フェーズ構成

### Phase 1: コア型定義・変数展開（snotra-core）

**ファイル**: `snotra-core/src/config.rs`

1. `InstantCommand` 構造体を追加
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub struct InstantCommand {
       pub name: String,
       pub command: String,
   }
   ```

2. `SearchConfig` に `instant_command_prefix` を追加（デフォルト `"@"`）

3. `Config` に `instant_commands: Vec<InstantCommand>` を追加（`#[serde(default)]`）

4. `Config::validate()` にプレフィックスバリデーション追加
   - 空文字を禁止
   - `/` を禁止（スラッシュコマンドと衝突防止）

**ファイル**: `snotra-core/src/instant.rs`（新規）

5. 変数展開ロジック
   ```rust
   pub fn expand_instant_command(
       command: &str,
       query: &str,
       clipboard: &str,
   ) -> String
   ```
   - URL 判定: `command.starts_with("http://") || command.starts_with("https://")`
   - URL → `{query}` `{clip}` を URL エンコードして展開
   - 非 URL → 生文字列で展開

6. 前方一致フィルタ
   ```rust
   pub fn filter_instant_commands<'a>(
       commands: &'a [InstantCommand],
       input: &str,
   ) -> Vec<&'a InstantCommand>
   ```

7. ユニットテスト
   - URL エンコード展開（日本語、空白、記号）
   - 非 URL 展開（生文字列）
   - `{query}` 空文字展開
   - `{clip}` 展開
   - 前方一致フィルタ（空文字=全件、前方一致、一致なし）
   - プレフィックスバリデーション（空文字・`/` が拒否されること）

**ファイル**: `snotra-core/Cargo.toml`

8. `percent-encoding` クレートを依存に追加（URL エンコード用）

**検証**: `cargo test -p snotra-core`

---

### Phase 2: バックエンド IPC（src-tauri）

**ファイル**: `src-tauri/src/commands/launch.rs`

1. `launch_item_core` を `pub(crate)` に変更（インスタントコマンドから再利用するため）

**ファイル**: `src-tauri/src/commands/instant.rs`（新規）

2. IPC コマンド `get_instant_commands`
   - 入力: `{ prefix_input: String }` — プレフィックス除去済みの入力文字列
   - 出力: `Vec<InstantCommand>` — 前方一致フィルタ済み
   - config から `instant_commands` を取得し `filter_instant_commands` を呼ぶ

3. IPC コマンド `execute_instant_command`
   - 入力: `{ name: String, query: String }`
   - 処理:
     a. config から該当コマンドを検索
     b. クリップボードからテキスト取得（`arboard` クレートを使用）
     c. `expand_instant_command()` で変数展開
     d. 既存の `launch_item_core` を再利用して `ShellExecuteW` 実行（新規に ShellExecuteW を書かない）
   - 出力: `LaunchResult`

4. `src-tauri/src/commands/mod.rs` にモジュール追加

**ファイル**: `src-tauri/src/main.rs`

5. `.invoke_handler()` に新コマンド追加

**ファイル**: `src-tauri/Cargo.toml`

6. `arboard` クレートを依存に追加（クリップボード読み取り用）

**検証**: `cargo check -p snotra-core -p snotra -p snotra-settings`

---

### Phase 3: フロントエンド（ui）

**ファイル**: `ui/src/lib/types.ts`

1. `InstantCommand` 型追加
   ```typescript
   export interface InstantCommand {
     name: string;
     command: string;
   }
   ```

2. `BootstrapPayload` に `instant_command_prefix: string` フィールドを追加

**ファイル**: `ui/src/lib/invoke.ts`

2. IPC ラッパー追加
   - `getInstantCommands(prefixInput: string): Promise<InstantCommand[]>`
   - `executeInstantCommand(name: string, query: string): Promise<LaunchResult>`

**ファイル**: `ui/src/stores/search.ts`

3. インスタントコマンドモードの実装
   - `query` effect 内で `startsWith(prefix)` を判定（スラッシュコマンド判定の前に配置）
   - プレフィックス検出 → `getInstantCommands()` でコマンド一覧取得（毎回 IPC、キャッシュなし）
   - 結果を `SearchResult[]` に変換して表示（`name` = コマンド名、`path` = command、`isFolder` = false、`isError` = false）
   - `folderState` / `toolSelectionState` 中はスキップ
   - `shouldShowResults` の条件修正: `results().length > 0 && (!indexing() || isInstantCommandMode())` にする（インデックス中でもインスタントコマンド結果を表示するため）

4. `activateSelected` / `activateSelectedByIndex` にインスタントコマンドモード分岐を追加
   - インスタントコマンドモード中 → `executeInstantCommand(name, query)` にディスパッチ
   - 実行後の状態クリーンアップ: query クリア + results クリア + ウィンドウ非表示（`launchAndReset` は呼ばない → 履歴記録をスキップ）

5. `resetForShow` でインスタントコマンドモードの状態もリセット

6. プレフィックスシグナルを作成（初期値 `"@"`）、bootstrap payload 受信時に更新

7. `instant-prefix-changed` イベントをリッスンし、プレフィックスシグナルを更新（`unlistenFns` に push して `onCleanup` で解放）

**ファイル**: `ui/src/components/SearchWindow.tsx`

8. `noResults` メモシグナルにインスタントコマンドモード中の除外を追加

9. `handleInput` の indexing ガードバイパス: value 取得 → プレフィックス判定 → indexing チェック（プレフィックスありならバイパス）の順に変更

10. `handleKeyDown` の ArrowRight / ArrowLeft 分岐にインスタントコマンドモードガードを追加（フォルダ展開に入らない）

11. `handleKeyDown` の Shift+Enter 分岐にインスタントコマンドモードガードを追加: `e.shiftKey && !toolSelectionState() && !isInstantCommandMode()` に条件変更

**ファイル**: `ui/src/components/ResultsSection.tsx`

12. インスタントコマンドモード中のアイコン取得スキップ（props 経由でモード状態を渡すか、`showIcons` とは別の制御フラグ）

**検証**: `npm run build`

---

### Phase 4: 設定 GUI（snotra-settings）

**ファイル**: `snotra-settings/src/tabs/instant.rs`（新規）

1. インスタントコマンドタブ UI
   - コマンド一覧（テーブル表示）
   - 追加/編集/削除モーダル（name + command）
   - プレフィックス設定テキスト入力（バリデーション: 空文字・`/` 禁止）

**ファイル**: `snotra-settings/src/tabs/mod.rs`
2. `pub mod instant;` 追加

**ファイル**: `snotra-settings/src/app.rs`
3. タブ追加

**ファイル**: `snotra-settings/src/i18n.rs`
4. 翻訳キー追加

**検証**: `cargo check -p snotra-settings`

---

### Phase 5: config_watcher ホットリロード対応

**ファイル**: `src-tauri/src/config_watcher.rs`

1. `apply_config_change()` にプレフィックス変更検知を追加
2. `instant-prefix-changed` イベント emit
3. `instant_commands` 配列の変更は IPC 毎回読み込みのためホットリロード不要

**検証**: `cargo check -p snotra`

---

### Phase 6: ドキュメント更新

- `snotra-core/CLAUDE.md` — `instant.rs` モジュール追加
- `src-tauri/CLAUDE.md` — IPC コマンド追加、`launch_item_core` の pub(crate) 化
- `ui/CLAUDE.md` — InstantCommandMode 記述追加
- `snotra-settings/CLAUDE.md` — タブ追加
- `AGENTS.md` — パターン追記

---

## 影響範囲

### 触るファイル

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/Cargo.toml` | `percent-encoding` 依存追加 |
| `snotra-core/src/config.rs` | `InstantCommand` 型、`SearchConfig` にプレフィックス、`Config` に配列、バリデーション |
| `snotra-core/src/instant.rs` | 新規: 変数展開・前方一致フィルタ |
| `snotra-core/src/lib.rs` | モジュール宣言追加 |
| `src-tauri/Cargo.toml` | `arboard` 依存追加 |
| `src-tauri/src/commands/launch.rs` | `launch_item_core` を `pub(crate)` に変更 |
| `src-tauri/src/commands/instant.rs` | 新規: IPC コマンド2つ |
| `src-tauri/src/commands/mod.rs` | モジュール宣言追加 |
| `src-tauri/src/main.rs` | invoke_handler 追加 |
| `src-tauri/src/config_watcher.rs` | プレフィックス変更検知 + イベント emit |
| `ui/src/lib/types.ts` | `InstantCommand` 型 |
| `ui/src/lib/invoke.ts` | IPC ラッパー2つ |
| `ui/src/stores/search.ts` | モード判定・実行ロジック・resetForShow・prefix listen |
| `ui/src/components/SearchWindow.tsx` | `noResults` 除外、`handleInput` indexing ガード順序変更、ArrowRight/Left ガード、Shift+Enter ガード |
| `ui/src/components/ResultsSection.tsx` | アイコン取得スキップ |
| `snotra-settings/src/tabs/instant.rs` | 新規: 設定タブ |
| `snotra-settings/src/tabs/mod.rs` | モジュール宣言 |
| `snotra-settings/src/app.rs` | タブ追加 |
| `snotra-settings/src/i18n.rs` | 翻訳追加 |
| `Cargo.lock` | 依存更新（自動） |

### 触らないファイル

- `snotra-core/src/search.rs` — 検索ロジックは無関係
- `snotra-core/src/history.rs` — インスタントコマンドは履歴に記録しない
- `ui/src/lib/commands.ts` — スラッシュコマンドは変更不要（プレフィックス `/` は禁止で衝突しない）

## 依存クレート

| クレート | 追加先 | 用途 |
|---------|--------|------|
| `percent-encoding` | `snotra-core/Cargo.toml` | URL エンコード（`{query}` `{clip}` の変数展開時） |
| `arboard` | `src-tauri/Cargo.toml` | クリップボード読み取り（`{clip}` 変数用） |

## 対称コードパスチェックリスト

- [x] `activateSelected` (Enter) ↔ `activateSelectedByIndex` (Click): 両方にインスタントコマンドモード分岐
- [x] `resetForShow` (ホットキー再表示): インスタントコマンドモードもリセット
- [x] `query` effect (入力) ↔ `refreshResults` (表示更新): 両方にインスタントコマンドモード判定
- [x] `handleInput` の indexing ガード: value 取得 → プレフィックス判定 → indexing チェックの順
- [x] `shouldShowResults`: インスタントコマンドモード中は `indexing()` を無視
- [x] ArrowRight / ArrowLeft: インスタントコマンドモード中は無効化（フォルダ展開に入らない）
- [x] Shift+Enter: インスタントコマンドモード中は通常 Enter と同じ（ツール選択に入らない）
- [x] `instant-prefix-changed` listen → `unlistenFns` に push → `onCleanup` で解放
- [x] プレフィックスシグナル初期値 `"@"` → bootstrap 到着時に更新 → イベントで更新
