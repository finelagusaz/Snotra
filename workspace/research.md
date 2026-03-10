# Research — Issue #221: オープナーのプリセット（VSCode, Terminal, Explorer）

## issue の要約

snotra-settings のオープナータブに「よく使われるツールを追加」機能を追加する。VSCode / Windows Terminal / Explorer をシステム上で検出し、ワンクリックで `config.openers` に追加できるようにする。設定コストの低減が目的。

## 関連コード

| ファイル | 役割 | 変更要否 |
|---------|------|--------|
| `snotra-core/src/config.rs` | `OpenerTool`, `OpenerRule` データモデル | ✅ プリセット定義と検出ロジック追加 |
| `snotra-settings/src/tabs/opener.rs` | オープナータブ UI | ✅ プリセットボタン追加 |
| `snotra-settings/src/app.rs` | タブ管理、`OpenerTabState` 保持 | ✅ `OpenerTabState` にプリセット検出結果キャッシュ追加 |
| `snotra-settings/src/i18n.rs` | 翻訳文字列 | ✅ プリセット関連文字列追加 |
| `SPEC.md` | 仕様書 §17.6 設定画面 | ✅ プリセット機能の追記 |
| `src-tauri/src/commands/launch.rs` | 起動ロジック | ❌ 変更不要（プリセットも通常の `[[openers]]` として保存） |
| `ui/src/` | フロントエンド | ❌ 変更不要 |

## 既存パターン

### opener タブの構造
- `OpenerTabState`: `ExePickerState` + `ModalState` を保持
- ルール一覧をフラット表示（rule_idx, tool_idx のペアで管理）
- 「追加…」ボタンでモーダル表示 → フィールド入力 → Save で `config.openers` に追加

### プリセットの追加方法
プリセットはモーダルを経由せず、ボタンクリックで直接 `config.openers` に追加する。既存の `save_opener()` ロジックと同じパターン（同じ target の既存ルールに tools を追加 or 新ルールを作成）を再利用する。

## プリセット定義

| ツール | name | exe | args | target | 検出方法 |
|--------|------|-----|------|--------|---------|
| VSCode | `Visual Studio Code` | PATH 上の `code.cmd` or `%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe` | (空) | `folder` | PATH 検索 + 既知パス |
| Windows Terminal | `Windows Terminal` | PATH 上の `wt.exe` | `-d {path}` | `folder` | PATH 検索 |
| Explorer | `Explorer` | `explorer.exe` | (空) | `folder` | 常に利用可能 |

## 技術的制約

### 検出ロジックの配置
- `snotra-core/config.rs` に配置（テスト可能性維持、CLAUDE.md 開発ルール準拠）
- 純粋な関数: `detect_opener_presets() -> Vec<OpenerPreset>` を返す

### I/O コスト
- PATH 検索 + ファイル存在確認は I/O を伴う
- egui のフレームごとルールに従い、`OpenerTabState::new()` 時に一度だけ実行してキャッシュ
- ただし `OpenerTabState` は `Default` trait で初期化されている → `OpenerTabState::default()` を拡張するか、`app.rs` 初期化時に検出を実行

### 重複防止
- 既に `config.openers` に同じ exe が存在するプリセットは「追加済み」として表示し、ボタンを無効化
- exe の比較は case-insensitive（Windows パス）

### PATH 上の exe 検索
- `std::env::var("PATH")` で PATH を取得
- `;` で分割（Windows）
- 各ディレクトリで `code.cmd` / `wt.exe` の存在を確認
- `code.cmd` は `.cmd` なので `std::path::Path::new(dir).join("code.cmd").exists()` で検出

## 未解決の疑問

なし。要件は明確。
