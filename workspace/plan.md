# Plan — Issue #221: オープナーのプリセット（VSCode, Terminal, Explorer）

## 概要

オープナータブのルール一覧の上部に「よく使うツール」セクションを追加。検出されたツールごとに「追加」ボタンを表示し、ワンクリックで `config.openers` の `folder` ルールに追加する。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/config.rs` | `OpenerPreset` 構造体 + `detect_opener_presets()` 関数 |
| `snotra-settings/src/tabs/opener.rs` | プリセットセクション UI + `OpenerTabState` にキャッシュ追加 |
| `snotra-settings/src/i18n.rs` | 翻訳キー追加（4件） |
| `SPEC.md` | §17.6 にプリセット機能の記述追加 |
| `snotra-settings/CLAUDE.md` | モジュール説明の更新（プリセット機能追記） |

## フェーズ構成

### Phase 1: snotra-core にプリセット検出ロジック追加

**ファイル**: `snotra-core/src/config.rs`

```rust
/// オープナープリセットの定義
pub struct OpenerPreset {
    pub name: &'static str,     // 表示名
    pub exe: String,            // 検出された exe パス
    pub args: &'static str,     // 固定引数
    pub target: &'static str,   // "folder"
}

/// システム上で利用可能なオープナープリセットを検出する。
/// PATH 検索と既知インストールパスを確認し、見つかったもののみ返す。
pub fn detect_opener_presets() -> Vec<OpenerPreset> { ... }
```

内部ヘルパー:
- `find_in_path(filename: &str) -> Option<String>`: PATH 上で exe を検索
- 検出順: VSCode → Windows Terminal → Explorer（Explorer は常に含める）

VSCode 検出:
1. PATH 上の `code.cmd` を検索
2. なければ `%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe` を確認
3. 見つかった方のパスを exe に設定

Windows Terminal 検出:
1. PATH 上の `wt.exe` を検索

Explorer:
- 常に `explorer.exe` として追加

### Phase 2: snotra-settings の UI 追加

**ファイル**: `snotra-settings/src/tabs/opener.rs`

`OpenerTabState` にプリセットキャッシュを追加:
```rust
pub struct OpenerTabState {
    pub exe_picker: ExePickerState,
    modal: ModalState,
    presets: Vec<snotra_core::config::OpenerPreset>,  // 初期化時に一度だけ検出
}
```

`OpenerTabState` の初期化を `Default` から明示的な `new()` に変更:
```rust
impl OpenerTabState {
    pub fn new() -> Self {
        Self {
            exe_picker: ExePickerState::default(),
            modal: ModalState::default(),
            presets: snotra_core::config::detect_opener_presets(),
        }
    }
}
```

`app.rs` の `SettingsApp::new()` で `OpenerTabState::new()` を使用。

UI レイアウト（`opener::ui()` 内、ルール一覧の前に挿入）:

```
[よく使うツール]  ← heading
(説明テキスト)

  Visual Studio Code    [追加]  ← 検出済みかつ未追加の場合
  Windows Terminal      [追加]
  Explorer              [追加済み]  ← 既に config に存在する場合は disabled

(8px 空白)

[オープナールール]  ← 既存の heading
...
```

「追加」ボタンクリック時の処理:
- `config.openers` から `target == "folder"` のルールを探す
- 見つかれば `tools` に `OpenerTool { name, exe, args }` を追加
- なければ新しい `OpenerRule { target: "folder", tools: vec![tool] }` を作成
- これは既存の `save_opener()` の Create モード相当のロジック

「追加済み」判定:
- `config.openers` の全ツールの `exe` を case-insensitive で比較
- マッチすれば追加済み（ボタン disabled）

**ファイル**: `snotra-settings/src/i18n.rs`

追加する翻訳キー:
- `heading_presets()`: "よく使うツール" / "Common tools"
- `preset_description()`: "検出されたツールをワンクリックで追加できます。" / "Add detected tools with one click."
- `btn_add_preset()`: "追加" / "Add"
- `label_already_added()`: "追加済み" / "Added"

### Phase 3: SPEC.md 更新

§17.6「設定画面」の末尾に追加:

```
- プリセット機能: オープナータブ上部に「よく使うツール」セクションを表示。
  システム上で検出されたツール（VSCode, Windows Terminal, Explorer）を
  ワンクリックで folder ルールに追加できる。既に同じ exe が登録済みの場合は
  「追加済み」として表示しボタンを無効化する。
```

### Phase 4: テスト

`snotra-core/src/config.rs` のテスト:
- `detect_opener_presets_returns_at_least_explorer`: Explorer は常に含まれる
- `detect_opener_presets_has_correct_fields`: 各プリセットの name/args/target が正しい
- `find_in_path_returns_none_for_nonexistent`: 存在しない exe は None

検証コマンド:
- `cargo test -p snotra-core`
- `cargo check -p snotra-core -p snotra -p snotra-settings`
- `npm run build`（TypeScript 側は変更なしだが念のため）

## 不変条件

1. プリセット検出は `OpenerTabState` 初期化時に一度だけ実行（フレームごとに実行しない）
2. プリセットで追加されたルールは通常の `[[openers]]` として config.toml に保存される（特別なフラグなし）
3. 「追加済み」判定は exe の case-insensitive 比較で行う
4. Explorer は常にプリセット候補に含まれる
5. config.openers への追加は既存の `save_opener()` と同じロジック（同一 target のルールに追加 or 新規ルール作成）

## SPEC.md 更新要否

✅ §17.6 に追記必要

## セルフレビュー

| 観点 | 対応状況 |
|------|---------|
| 対称コードパス | プリセット追加のみ（削除のペアは不要 — 通常の編集/削除で対応可能） ✅ |
| 影響範囲の網羅性 | opener タブ UI + snotra-core のみ。IPC/フロントエンド変更不要 ✅ |
| 境界条件 | PATH 未設定時、VSCode 未インストール時、全プリセット追加済み時を考慮 ✅ |
| リソース管理 | 新規リソース導入なし ✅ |
| 既存パターンとの整合 | `config.openers` への追加は既存パターン再利用 ✅ |
| YAGNI 違反 | プリセットの編集/カスタマイズ機能は追加しない。issue で挙げられた3ツールのみ ✅ |
| シンプル化 | 新たな状態は `Vec<OpenerPreset>` キャッシュのみ。モーダル不要、状態フラグ不要 ✅ |
| 破壊不変条件 | config.toml への書き込みは既存の Save フローを通る。プリセット追加自体は draft 変更のみ ✅ |
| docs/ 更新要否 | アーキテクチャ変更なし。`snotra-settings/CLAUDE.md` のモジュール説明に追記 ✅ |
