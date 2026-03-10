# Plan — 設定バックアップ機能 (#226)

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/config.rs` | `config_dir()` を `pub` 公開（バックアップタブからフォルダパス取得に必要） |
| `snotra-settings/src/tabs/mod.rs` | `pub mod backup;` 追加 |
| `snotra-settings/src/tabs/backup.rs` | **新規**: バックアップタブ UI（エクスポート・インポート・フォルダを開く） |
| `snotra-settings/src/app.rs` | `TabId::Backup` 追加、`BackupTabState` フィールド追加、タブ描画分岐追加 |
| `snotra-settings/src/i18n.rs` | バックアップタブ用の翻訳メソッド追加 |
| `snotra-settings/CLAUDE.md` | モジュール構成にバックアップタブ追記 |
| `SPEC.md` | §13 にバックアップ機能の仕様追記 |

## 実装順序

### Phase 1: Core（config_dir 公開）

`Config::config_dir()` は現在 `pub` だが、返すのが `Option<PathBuf>` であることを確認。
→ 調査の結果、既に `pub fn config_dir()` なので追加変更不要の可能性が高い。

### Phase 2: i18n

バックアップタブに必要な翻訳を `i18n.rs` に追加:
- `tab_backup()`: "バックアップ" / "Backup"
- `heading_export()`: "エクスポート" / "Export"
- `heading_import()`: "インポート" / "Import"
- `heading_data_folder()`: "データフォルダ" / "Data folder"
- `btn_export()`: "設定をエクスポート…" / "Export settings…"
- `btn_import()`: "設定をインポート…" / "Import settings…"
- `btn_open_folder()`: "設定フォルダを開く" / "Open settings folder"
- `status_export_success()`: "エクスポートしました" / "Exported"
- `status_export_failed()`: "エクスポート失敗: " / "Export failed: "
- `status_import_success()`: "インポートしました。設定を再読み込みします" / "Imported. Reloading settings"
- `status_import_failed()`: "インポート失敗: " / "Import failed: "
- `status_import_validation_error()`: "インポートファイルにエラーがあります: " / "Import file has errors: "
- `label_data_folder_path()`: パス表示用
- `label_data_folder_description()`: "%APPDATA%\Snotra フォルダを..." / "Open the %APPDATA%\Snotra folder..."
- `label_export_description()`: 説明テキスト
- `label_import_description()`: 説明テキスト
- `dialog_export_config()`: "設定をエクスポート" / "Export settings"
- `dialog_import_config()`: "設定をインポート" / "Import settings"
- `filter_toml()`: "TOML ファイル" / "TOML files"

### Phase 3: バックアップタブ UI（backup.rs 新規作成）

```
BackupTabState {
    export_picker: PickerState,  // エクスポート先選択（保存ダイアログ）
    import_picker: PickerState,  // インポート元選択（開くダイアログ）
}
```

**エクスポート処理**:
1. ユーザーが「設定をエクスポート」ボタンをクリック
2. `rfd::FileDialog` で保存先を選択（デフォルトファイル名: `config_yyyymmddhh24mm.toml`）
3. 現在の `config.toml`（`Config::config_path()` のファイル）をそのままコピー
4. draft ではなく保存済みファイルをコピーする（未保存の変更は含めない）
5. 成功: ステータス「エクスポートしました」(2秒)
6. 失敗: ステータス「エクスポート失敗: <理由>」(5秒)

**インポート処理**:
1. ユーザーが「設定をインポート」ボタンをクリック
2. `rfd::FileDialog` で TOML ファイルを選択
3. 選択ファイルを `toml::from_str::<Config>()` でパース（欠損キー補完 = `#[serde(default)]`、未知キー無視 = `#[serde(deny_unknown_fields)]` なし）
4. `Config::validate()` でバリデーション
5. バリデーション失敗: ステータスにエラー表示 (5秒)、インポートしない
6. バリデーション成功: `Config::config_path()` に上書き保存（`Config::save()` 使用）
7. `draft` と `saved` を新しい Config で更新（UI に即反映）
8. 成功: ステータス「インポートしました」(2秒)

**設定フォルダを開く**:
1. `open::that(Config::config_dir())` でエクスプローラーを開く
2. 失敗時のみステータス表示

### Phase 4: app.rs にタブ統合

1. `TabId::Backup` を enum に追加（`About` の前に配置）
2. `TabId::ALL` に追加
3. `TabId::label()` に `Backup` 分岐追加
4. `TabId::from_str()` に `"backup"` 追加
5. `SettingsApp` に `backup_state: tabs::backup::BackupTabState` 追加
6. `update()` の match に `TabId::Backup` 分岐追加
7. フッター: `Backup` タブでは Save/Discard/Reset ボタンを非表示にする（About と同じ扱い）

### Phase 5: SPEC.md + CLAUDE.md 更新

- `SPEC.md` §13 にバックアップ機能の記述追加
- `snotra-settings/CLAUDE.md` のモジュール構成に `backup.rs` 追記

## 不変条件

1. **draft/saved に副作用を与えない**: エクスポートは保存済みファイルのコピー。インポートは draft/saved 両方を新 Config で更新する
2. **ファイルピッカーの `active = false` リセット**: エクスポート・インポートの両ピッカーで、成功・キャンセル・エラーの全パスで `active = false` にする
3. **インポート時のバリデーション**: `Config::validate()` を通す。パース成功でもバリデーション失敗なら拒否
4. **原子的保存**: インポート時の上書きは `Config::save()` を使う（tmp → rename パターン）
5. **本体との整合**: インポートで `config.toml` を上書きすると、本体の `config_watcher` が自動検知して反映する。追加の IPC は不要

## テスト方針

- `snotra-core` 側: Config のラウンドトリップテスト（save → load が一致することを確認）は既存テストでカバー済み
- `snotra-settings` 側: egui UI コードはユニットテスト対象外（CLAUDE.md の方針）
- **手動テスト**:
  - エクスポート → 別名で保存 → ファイル内容が元の config.toml と一致すること
  - インポート → 設定が即座に UI に反映されること
  - 不正な TOML ファイルのインポート → エラーメッセージが表示されること
  - 設定フォルダを開く → エクスプローラーが開くこと
- **検証コマンド**: `cargo check -p snotra-core -p snotra -p snotra-settings`

## SPEC.md 更新内容

§13「データ保存」に以下を追記:

```
### 13.3 設定バックアップ

- 設定画面の「バックアップ」タブから config.toml のエクスポート/インポートが可能
- エクスポート: 保存済み config.toml を指定先にコピー。ファイル名デフォルトは `config_yyyymmddhh24mm.toml`
- インポート: TOML ファイルを選択 → パース → バリデーション → config.toml に上書き保存
  - 欠損キーはデフォルト補完、未知キーは無視（通常の config.toml 読み込みと同じ）
  - バリデーション失敗時はインポートを中止しエラー表示
- 「設定フォルダを開く」: %APPDATA%\Snotra をエクスプローラーで開く
```

## セルフレビュー

1. **対称コードパス**: エクスポート/インポートは対称ペア。両方のピッカーで `active = false` リセットを確認する → 計画に明記済み
2. **影響範囲の網羅性**: 変更は snotra-settings 内に閉じる。snotra-core は `config_dir()` の可視性確認のみ。本体 (src-tauri) への変更は不要（config_watcher が自動検知）
3. **境界条件**:
   - 空ファイルのインポート → `toml::from_str` がパースエラーを返す → ステータスに表示
   - 巨大ファイルのインポート → TOML パーサーが処理（実用上問題なし）
   - config_dir が存在しない → `open::that` がエラーを返す → ステータスに表示
   - エクスポート先がリードオンリー → `fs::copy` がエラーを返す → ステータスに表示
4. **リソース管理**: PickerState のスレッドは結果を `Arc<Mutex>` に書いて終了。生存期間は短い。明示的な破棄は不要
5. **既存パターンとの整合**: PickerState パターン（index.rs）、open::that パターン（About タブ）を再利用
6. **YAGNI 違反**: なし。zip/バイナリ形式ではなく素の TOML コピーで最もシンプル
7. **シンプル化の挑戦**: OK。新たな状態は `BackupTabState`（2つの PickerState）のみ。バックアップ専用のバイナリフォーマットや暗号化は導入しない
8. **破壊不変条件**: インポート時に `Config::save()` で原子的書き込みするため、中間状態で config.toml が壊れるリスクはない。バリデーション失敗時は書き込まない
