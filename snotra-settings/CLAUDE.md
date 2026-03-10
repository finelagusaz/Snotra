# snotra-settings

egui ベースの設定・about バイナリ crate。本体（`src-tauri`）とは別プロセスで動作する。

## アーキテクチャ

- 本体との連携は `config.toml` ファイル1点のみ。IPC は使わない
- 6タブの設定エディタ（最後のタブ「Snotra について」に about 情報を統合）
- 設定の読み書きは `snotra-core::Config` を直接使用。本体は `notify` クレートで変更を検知する

## モジュール構成

- `main.rs`: エントリポイント、eframe 起動
- `app.rs`: `eframe::App` 実装、タブ管理（About タブ含む）、保存/破棄/リセットロジック
- `font.rs`: 日本語フォント読み込み + システムフォント列挙
- `hotkey_input.rs`: ホットキーキャプチャウィジェット
- `i18n.rs`: 翻訳構造体 `Tr(Language)`。各メソッド（`tr.tab_general()` 等）が `match self.0` で `&'static str` を返す。タブ UI 関数は `tr: &Tr` を引数に取り、保存時に `self.tr = Tr(new_language)` で即時反映
- `tabs/`: 5タブの UI 実装（About タブは `app.rs` に直接実装）
  - `mod.rs`: サブモジュール宣言のみ
  - `general.rs`: 全般設定（起動時表示、トレイ、IME、ホットキー）
  - `search.rs`: 検索設定（検索モード、履歴、隠しファイル）
  - `index.rs`: インデックス設定（スキャンパス管理）
  - `visual.rs`: ビジュアル設定（テーマプリセット、カラーピッカー、フォント）
  - `opener.rs`: オープナー設定（ツール/ルール管理、プリセット検出・追加）
  - `instant.rs`: インスタントコマンド設定（プレフィックス・コマンド追加/編集/削除）

## egui 実装の注意点

### API の型に注意

- `egui::Key::ALL` は `&[Key]`（`&&[Key]` ではない）。`for &key in egui::Key::ALL` が正しい
- `color_edit_button_srgba` は `&mut Color32` を取る。一時変数に変換して渡すと変更が反映されない。`let mut color = Color32::from_hex(hex)` のように変数を作り、変更後に hex 文字列に書き戻す
- `egui::Stroke::new(width, color)` は 2 引数で動作する（eframe 0.33 現在）。`StrokeKind` が必要になるのは将来のバージョンの可能性があるため、egui をバージョンアップする際は確認する
- `ThemePreset` は `Copy`。`.clone()` ではなく値コピーで渡す（clippy `clone_on_copy`）

### Win キーの制限

egui の `Modifiers` は `ctrl` / `alt` / `shift` / `mac_cmd` / `command` のみ。Win キーは検出できない。ホットキーキャプチャでは Ctrl/Alt/Shift のみサポートする。デフォルトホットキー `Alt+Q` は問題なく動作する。

### フレームごとの重い処理を避ける

egui は毎フレーム `update()` を呼ぶ（60fps）。`list_system_fonts()` のような Win32 API 呼び出しをフレームごとに実行するとパフォーマンスが劣化する。初期化時に一度だけ取得して `SettingsApp` のフィールドにキャッシュする。

## draft / saved 二重状態モデル

`SettingsApp` は `draft: Config`（UI 編集中）と `saved: Config`（最後に保存した状態）の2つを保持する。

- **draft**: 各タブの UI 入力で即座に変更される。タブを切り替えても保持される（タブ単位の中間保存はない）
- **saved**: `save()` 成功時のみ `draft` のクローンで更新。バリデーション失敗や I/O エラー時は更新されない
- **has_changes()**: `draft != saved` で判定。Save/Discard ボタンの有効化に使用
- **Discard**: `draft = saved.clone()` で全タブの編集を一括破棄
- **Reset to default**: `draft = Config::default()` で既定値に戻す（saved は変更しない → has_changes() が true になり Save が必要）

### 保存フロー

```
Save クリック → draft.clone() → normalize_scan_paths() → normalize_openers()
→ validate() → エラーあり: ステータスに最初のエラーを5秒表示、return
→ エラーなし: config.save() → I/O エラー: ステータスに5秒表示、return
→ 成功: saved = config.clone(); draft = config; Tr 更新; ステータスに2秒表示
```

### タブ別ステート

モーダルを持つ3タブはタブ固有の状態を `*TabState` に保持する。モーダル内の編集値は `draft` Config に反映されるまで独立しており、モーダルの Save で初めて `draft` に書き込まれる。

| タブ | ステート | 特記 |
|------|---------|------|
| index | `IndexTabState` | `PickerState`（フォルダ選択）+ `ModalState`（Create/Edit） |
| opener | `OpenerTabState` | `ExePickerState`（exe 選択）+ `ModalState`（ネストした rule/tool インデックス） |
| instant | `InstantTabState` | `ModalState` のみ（シンプルなフラットリスト） |

## モーダル Create/Edit パターン

index / opener / instant の3タブが共通して使うモーダルの設計パターン:

- `ModalMode::Create`: フィールド初期化 → モーダル表示 → Save で Vec に `push`
- `ModalMode::Edit`: 既存値をフィールドにコピー → モーダル表示 → Save でインデックス指定で上書き。Delete ボタンも表示
- **インデックス陳腐化ガード**: Edit モードで保持する `editing_index` はモーダルを開いた時点のスナップショット。モーダル表示中に外部で行が削除されるケース（このアプリでは発生しないが）に備え、Save/Delete 前に必ず `if idx < vec.len()` で境界チェックする

## 非同期ファイルピッカーパターン

`rfd::FileDialog` はブロッキング API のため、egui の UI スレッドで直接呼ぶとフリーズする。代わりにスレッド spawn + `Arc<Mutex<Option<Option<PathBuf>>>>` パターンを使う。

```
PickerState {
    result: Arc<Mutex<Option<Option<PathBuf>>>>,  // None=実行中, Some(None)=キャンセル, Some(Some(path))=選択
    active: bool,                                 // true の間ボタン無効化
}
```

- スレッド内で `ctx.request_repaint()` を呼んで UI 更新をトリガーする
- 毎フレーム `try_lock()`（非ブロッキング）で結果をポーリングし、取得後に `.take()` + `active = false`
- **`active = false` の書き忘れはボタンが永久に無効化されるバグになる**

## opener のターゲットエンコーディング

OpenerRule のターゲットは文字列プレフィックスで種別を表現する:

- `"folder"` → フォルダ用ルール
- `"ext:.txt"` → 拡張子マッチルール（`strip_prefix("ext:")` でパース）

保存時は `TargetKind::Extension` → `format!("ext:{}", ext.trim())`、`TargetKind::Folder` → `"folder".to_string()` に逆変換する。

## 開発ルール

- ロジック（Config の読み書き、バリデーション）は `snotra-core` に寄せる。このクレートは UI 層のみ
- 境界チェック: 配列アクセス前に必ずインデックスの有効性を確認する（`if idx < vec.len()`）
- opener のターゲット変更: ツールを旧ルールから削除し、新ルールに追加する。OpenerRule.target を上書きしない（他のツールが巻き添えになる）
- ユニットテストは書かない方針（egui UI コードはモック困難）。ロジックのテストは `snotra-core` 側で行う

## 本体との連携パターン

### 設定保存フロー

1. snotra-settings: `Config::save()` で `config.toml` に書き込み
2. 本体: `config_watcher` が `notify` でファイル変更を検知（100ms debounce）
3. 本体: `apply_config_change()` で差分検出 → ホットキー/トレイ/インデックス/テーマを反映

### 初回起動フロー

1. 本体: `Config::is_first_run()` → `launch_settings_process` で直接起動（`open_settings` の indexing ガードをバイパス）
2. snotra-settings: ユーザーが設定を編集・保存
3. 本体: 監視スレッドがプロセス終了を検知 → `start_index_build` を開始

**注意**: 本体の `open_settings` には `if indexing { return }` ガードがある。初回起動時は `indexing=true` なので、`open_settings` 経由ではなく `launch_settings_process` を直接呼ぶ必要がある。

### 初回起動時のタブ選択

コマンドライン引数 `--tab index` で起動すると Index タブがアクティブになる。引数なし + `--first-run` の場合もデフォルトで Index タブが開く（初回はスキャンパス設定が最優先のため）。通常起動時のデフォルトは General タブ。

## Escape キーの振る舞い

- ホットキーキャプチャ中: キャプチャをキャンセル（`hotkey_state.is_capturing()` で判定）
- 未保存の変更あり: 「未保存の変更があります」ステータスを3秒表示（ウィンドウは閉じない）
- 未保存の変更なし: ウィンドウを閉じる

## ステータスメッセージのタイマー

ステータスメッセージは `status_timer` で自動クリアされる。毎フレーム `stable_dt` で減算し、0 以下になったらメッセージをクリアする。

| メッセージ種別 | 表示時間 |
|-------------|---------|
| バリデーションエラー | 5秒 |
| 保存 I/O エラー | 5秒 |
| 未保存警告（Escape 時） | 3秒 |
| 保存成功 | 2秒 |

## ウィンドウ位置の永続化

毎フレーム `ctx.input().viewport().outer_rect` からウィンドウ位置を `last_position` に記録し、`on_exit()` で `window_data::save_settings_placement()` に保存する。次回起動時に `load_settings_placement()` で復元する。
