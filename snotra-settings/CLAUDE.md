# snotra-settings

egui ベースの設定・about バイナリ crate。本体（`src-tauri`）とは別プロセスで動作する。

## アーキテクチャ

- 本体との連携は `config.toml` ファイル1点のみ。IPC は使わない
- 7タブの設定エディタ（General / Search / Index / Visual / Opener / Instant Command / Backup）。バージョン情報はサイドバーに表示
- 設定の読み書きは `snotra-core::Config` を直接使用。本体は `notify` クレートで変更を検知する

## モジュール構成

- `main.rs`: エントリポイント、eframe 起動
- `app.rs`: `eframe::App` 実装、タブ管理（About タブ含む）、保存/破棄/リセットロジック。色は `style` のトークンを参照
- `font.rs`: 日本語フォント読み込み（Regular の `jp_font` + 見出し用 Semibold ファミリ `SEMIBOLD_FAMILY` を登録。Semibold 不在環境では未登録のまま graceful degrade）+ システムフォント列挙
- `hotkey_input.rs`: ホットキーキャプチャウィジェット
- `i18n.rs`: 翻訳構造体 `Tr(Language)`。各メソッド（`tr.tab_general()` 等）が `match self.0` で `&'static str` を返す。タブ UI 関数は `tr: &Tr` を引数に取り、保存時に `self.tr = Tr(new_language)` で即時反映
- `style.rs`: デザイントークン（色 / 余白 / フォントサイズ / 幅）と共有スタイルヘルパー（`tab_scroll_area` / `section_heading` / `hint` / `settings_grid` / `list_item` / `reorder_controls` / `modal_header` / `modal_buttons` / `danger_button` / `apply_type_ramp`）。全タブと app.rs がこれ経由で描画する。詳細は `SETTINGS-DESIGN.md`
- `tabs/`: 7タブの UI 実装
  - `mod.rs`: サブモジュール宣言のみ
  - `common.rs`: index / opener / instant 3タブ共通のモーダル状態（`ModalState<F, I>` / `ModalMode` / `save_entry` / `delete_entry`）と非同期ファイルピッカー（`PickerState::poll` / `launch`）。純ロジックのみ（egui 描画は各タブに残す）。ユニットテストあり
  - `general.rs`: 全般設定（起動時表示、トレイ、IME、ホットキー）
  - `search.rs`: 検索設定（検索モード、履歴、隠しファイル）
  - `index.rs`: インデックス設定（スキャンパス管理）
  - `visual.rs`: ビジュアル設定（テーマプリセット、カラーピッカー、フォント）
  - `opener.rs`: オープナー設定（ツール/ルール管理、プリセット検出・追加）
  - `instant.rs`: インスタントコマンド設定（プレフィックス・コマンド追加/編集/削除）
  - `backup.rs`: バックアップ設定（エクスポート・インポート・設定フォルダを開く）。Save/Discard ボタンはこのタブでは非表示

## スタイルシステム

設定 UI のデザイン規約とトークンの SSOT は **`SETTINGS-DESIGN.md`**（クレート直下）。`src/style.rs` が実装（トークン + ヘルパー）。

- **不変条件**: 色 / 余白 / ScrollArea / フォントサイズを各タブに**直書きしない**。`style` のトークン・ヘルパーを使う（`Color32::from_rgb` 直書きは visual の色編集機能を除き禁止）。
- タイポグラフィは `style::apply_type_ramp(ctx, heading_semibold)` が Fluent タイプランプ（見出し18 / 本文14 / 副文12）と見出しの Semibold を一括登録する。見出し（`section_heading` / `modal_header`）は Semibold、本文・副文は Regular。`heading_semibold` は `configure_fonts` の戻り値で、Semibold フォント不在時は Regular にフォールバック。`run()` で `apply_win11_theme` の後に呼ぶ。
- 新タブ・新パーツの追加時は `SETTINGS-DESIGN.md` の「新タブ追加チェックリスト」に従い、逸脱が要る場合は先に同書を更新する。

## egui 実装の注意点

### API の型に注意

- `egui::Key::ALL` は `&[Key]`（`&&[Key]` ではない）。`for &key in egui::Key::ALL` が正しい
- `color_edit_button_srgba` は `&mut Color32` を取る。一時変数に変換して渡すと変更が反映されない。`let mut color = Color32::from_hex(hex)` のように変数を作り、変更後に hex 文字列に書き戻す
- `egui::Stroke::new(width, color)` は 2 引数で動作する（egui 0.35 現在）。`StrokeKind` enum は 0.35 で追加されたが、`Stroke::new` と `Visuals` の stroke フィールドはこれを要求しない。egui をバージョンアップする際は `Stroke::new` が `StrokeKind` を取るようになっていないか確認する
- `ThemePreset` は `Copy`。`.clone()` ではなく値コピーで渡す（clippy `clone_on_copy`）

### Win キーの制限

egui の `Modifiers` は `ctrl` / `alt` / `shift` / `mac_cmd` / `command` のみ。Win キーは検出できない。ホットキーキャプチャでは Ctrl/Alt/Shift のみサポートする。デフォルトホットキー `Alt+Q` は問題なく動作する。

### フレームごとの重い処理を避ける

eframe は毎フレーム `App::ui()`（旧 `update()`。eframe 0.35 で `logic()` / `ui()` に分割）を呼ぶ（60fps）。`list_system_fonts()` のような Win32 API 呼び出しをフレームごとに実行するとパフォーマンスが劣化する。初期化時に一度だけ取得して `SettingsApp` のフィールドにキャッシュする。

### フォント登録（`set_fonts`）の注意点

- **複数フォントを1つの `FontFamily` に混ぜると混在テキストでベースラインがずれる**: Latin と CJK を別フォント（例 Segoe UI Semibold + Yu Gothic UI Semibold）で1ファミリに積むと、異なる vertical metrics + `FontTweak` により同一行（混在見出し「PATH 実行ファイル」等）で縦位置がずれる。混在スクリプトを描くパーツは、両スクリプトをカバーする**単一フォント**で統一する方が整列が安定する（#399。この欠陥は型チェック・clippy・ユニットテストを素通りし**視覚スモークでのみ顕在化**する）
- **`set_fonts` は登録フォントを起動時に eager parse し、不正データで panic する**: ttc の範囲外 face index 等は `set_fonts` 内のパースで panic（release は `panic = "abort"` なので即 abort・`catch_unwind` 不可）。`std::fs::read` の成否だけでは「ファイルは在るが face が無い」を弾けないため、渡す前に検証する（`font.rs` の `face_index_valid` = ttc ヘッダの `numFonts` を確認）。外部リソースの「不在時フォールバック」は不在の種類（**ファイル不在 / 存在するが不正 / パース不能**）を分解して各検知点を用意する

## draft / saved 二重状態モデル

`SettingsApp` は `draft: Config`（UI 編集中）と `saved: Config`（最後に保存した状態）の2つを保持する。

- **draft**: 各タブの UI 入力で即座に変更される。タブを切り替えても保持される（タブ単位の中間保存はない）
- **saved**: `save()` 成功時のみ `draft` のクローンで更新。バリデーション失敗や I/O エラー時は更新されない
- **has_changes()**: `draft != saved` で判定。Save/Discard ボタンの有効化に使用
- **Discard**: `draft = saved.clone()` で全タブの編集を一括破棄
- **Reset to default**: `draft = Config::normalized_default()` で既定値に戻す（saved は変更しない → has_changes() が true になり Save が必要）。`normalized_default()` は `apply_migrations()` 適用済みの「正規化済み（Option フィールドが全 Some）」既定値を返すため、`saved` との `PartialEq` がタブ遷移順序（DragValue の `get_or_insert`）に依存しない
- **タブ別ダーティ点（`•`）**: `app.rs` の `SECTION_TABLE`（Config セクション → TabId 対応表、SSOT）から導出。Config に新セクションを追加したら表を1箇所更新する（更新漏れは `section_table_covers_all_config_fields` テストの網羅 destructure がコンパイルエラー/テスト失敗で検出する）

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
| index | `IndexTabState` | `common::PickerState`（フォルダ選択）+ `common::ModalState<ScanPathFields>` |
| opener | `OpenerTabState` | `common::PickerState`（exe 選択）+ `common::ModalState<OpenerFields, (usize, usize)>`（ネストした rule/tool インデックス） |
| instant | `InstantTabState` | `common::ModalState<InstantFields>` + exe 用 `common::PickerState` |

## モーダル Create/Edit パターン

index / opener / instant の3タブが共通して使うモーダルの状態機械は `tabs/common.rs` の `ModalState<F, I = usize>`（`F` = タブ固有編集フィールド struct、`I` = 編集スナップショットの位置型）に集約されている:

- `ModalMode::Create`: `open_create()`（フィールドを `F::default()` に初期化）→ モーダル表示 → Save で `common::save_entry` が Vec に `push`。複製は `open_create_with(fields)`
- `ModalMode::Edit`: `open_edit(index, fields)` で既存値をコピー → モーダル表示 → Save で `common::save_entry` がインデックス指定で上書き。Delete ボタンも表示（`common::delete_entry`）
- **インデックス陳腐化ガード**: Edit モードで保持する `editing` はモーダルを開いた時点のスナップショット。モーダル表示中に外部で行が削除されるケース（このアプリでは発生しないが）に備え、`save_entry` / `delete_entry` が境界チェックを内蔵する（範囲外は no-op）。opener はネスト `(rule, tool)` のため Save/Delete の境界チェックのみ固有ロジック（`save_opener` / Delete ハンドラ）に残る
- モーダルの egui 描画（`show_modal` の `ui.xxx()` 列）はタブごとに固有のまま各タブに残す。共通化するのは状態遷移・境界チェックだけ

## 非同期ファイルピッカーパターン

`rfd::FileDialog` はブロッキング API のため、egui の UI スレッドで直接呼ぶとフリーズする。スレッド spawn + `Arc<Mutex<Option<Option<PathBuf>>>>` パターンを `tabs/common.rs` の `PickerState` に集約している。

```
PickerState {
    result: Arc<Mutex<Option<Option<PathBuf>>>>,  // None=実行中, Some(None)=キャンセル, Some(Some(path))=選択
    active: bool,                                 // true の間ボタン無効化
}
```

- `launch(ctx, dialog)`: `active = true` にしてスレッド spawn。完了時に `ctx.request_repaint()` で UI 更新をトリガーする
- `poll()`: 毎フレームの非ブロッキングポーリング（`try_lock` + `.take()`）。結果取得時（キャンセル含む）に `active = false` へ戻す
- **`active = false` の戻し忘れはボタンが永久に無効化されるバグになる** — この責務は `poll()` 1箇所に集約されており、各タブで手書きしない

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
  - 例外1: 純粋な非 egui ヘルパー（例 `font.rs` の `face_index_valid`）の境界テストはインラインで置いてよい。egui モック困難の理由が当たらず、かつ degrade パスが視覚スモークで再現できない（dev 機には対象フェイスが在る）ため、テストが唯一の検証手段になる
  - 例外2: **UI 操作 + 状態観測**は `egui_kittest`（AccessKit）でヘッドレステストできる（下記「ヘッドレス UI テスト」）。「egui モック困難」は描画のモックを指し、AccessKit ツリー経由の操作には当たらない

## ヘッドレス UI テスト（egui_kittest）

`egui_kittest`（dev-dependency）で AccessKit ツリー経由の操作テストを書ける。対象は
**フッターボタン（Save/Discard/Reset）の wiring + draft/saved フロー**——実 UI を操作して初めて
検証できる死角（#440）。テストは `app.rs` の `#[cfg(test)] mod tests` に**インラインで置く**
（`SettingsApp` / `new` / `has_changes` が private のため `tests/` の integration test からは不可視）。

- **パターン**: `Harness::new_ui_state(|ui, app| app.ui_impl(ui), app)` で `SettingsApp` を state
  として載せ、`harness.get_by_label(...).click()` で操作、`harness.state()` / `state_mut()` で
  内部状態を観測。`ui_impl` は `App::ui` から `eframe::Frame` 依存を除いた本体（`ui()` は 1 行委譲）。
- **`run()` でなく固定ステップ（`settle`）を使う**: この UI は checkbox 等のアニメーションで毎フレーム
  repaint を要求し、収束前提の `Harness::run()` は `max_steps` 超過で panic する。観測対象は描画の
  収束ではなく draft の内部状態なので、`step()` を数回回してクリック（press→release）を処理させる。
- **言語は `Language::En` 固定**: `default_language()` は OS 依存でラベルが非決定的になる。
- **重複させない**: dirty-dot 導出（`section_table_*`）とモーダル状態機械（`tabs::common::tests`）は
  純ロジックテスト済み。kittest は「実 UI 操作でしか検証できない wiring」に絞る。

### 境界（kittest で検証できないもの）

`egui_kittest` は AccessKit ツリー（操作・状態）の検証に限る。**レイアウト・レンダリング欠陥**
（#399 型のベースラインずれ・フォント混在・overflow 等）は**引き続き人手の視覚スモークが唯一の検知手段**。
wgpu スナップショット比較は評価の結果**採用しない**（フォント/GPU/driver 依存で CI flaky、かつ環境差を
吸収する threshold が #399 型欠陥そのものをマスクするため ROI が低い。#440 の判断）。CI に GPU 依存を
持ち込まないため dev-dependency は `default-features = false`（wgpu/snapshot/x11 を引き込まない）。

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

## ウィンドウの閉じ操作と変更保護

### Escape キー
- ホットキーキャプチャ中: キャプチャをキャンセル（`hotkey_state.is_capturing()` で判定）
- 未保存の変更あり: 「未保存の変更があります」ステータスを3秒表示（ウィンドウは閉じない）
- 未保存の変更なし: ウィンドウを閉じる

### × ボタン / Alt+F4（CloseRequested）
- 未保存の変更あり: `CancelClose` で閉じ操作をキャンセルし、Escape と同じステータスを表示
- 未保存の変更なし: 通常通り閉じる

### ウィンドウタイトルのダーティインジケーター
- `has_changes()` が true のとき、タイトル末尾に `*` を付加（例: `Snotra 設定*`）
- 毎フレーム再計算されるため、保存/破棄で即座に消える

## ステータスメッセージ

### タイマー制御メッセージ
`status_timer` で自動クリアされる。毎フレーム `stable_dt` で減算し、0 以下になったらメッセージをクリアする。

| メッセージ種別 | 表示時間 |
|-------------|---------|
| バリデーションエラー | 5秒 |
| 保存 I/O エラー | 5秒 |
| 未保存警告（Escape / × ボタン時） | 3秒 |
| 保存成功 | 2秒 |

### 常時表示メッセージ
タイマーメッセージが無い状態で `has_changes()` が true の場合、「未保存の変更があります」をフッターに常時表示する。タイマーメッセージが優先される（保存成功 "Saved" 等が表示中は常時テキストは隠れる）。

### フッター vs インラインの使い分け

フッターの `status` / `status_timer` は draft/saved ワークフロー（保存成功・バリデーションエラー等）に適する。draft/saved に参加しないタブ（Backup、About 等）でフィードバックが必要な場合は、タブ固有の state にメッセージを持たせてタブ内にインライン表示する。フッターを流用すると、永続性の要件（エラーは消えてほしくない）や複数行エラーとの衝突が起きる。

## ウィンドウ位置の永続化

毎フレーム `ctx.input().viewport().outer_rect` からウィンドウ位置を `last_position` に記録し、`on_exit()` で `window_data::save_settings_placement()` に保存する。次回起動時に `load_settings_placement()` で復元する。
