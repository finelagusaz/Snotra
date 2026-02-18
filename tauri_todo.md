# Tauri 移行 TODO

## 完了済み

### Phase 1: プロジェクト骨格 ✅
- [x] `snotra-core/` クレート作成（純ロジック9モジュール抽出）
- [x] ワークスペース `Cargo.toml` 作成（members: `snotra-core`, `src-tauri`）
- [x] `src-tauri/` Tauriバイナリクレート作成
- [x] `tauri.conf.json`（メインウィンドウ: `visible: false`, `decorations: false`, `skipTaskbar: true`）
- [x] SolidJS + Vite + TypeScript セットアップ（`ui/` ディレクトリ）
- [x] `legacy/` に旧egui実装を退避
- [x] `cargo test -p snotra-core` — 全64テストパス

### Phase 2: Tauriコマンド接続 ✅
- [x] `state.rs` — `AppState`（Mutex\<SearchEngine\>, Mutex\<HistoryStore\>, Mutex\<Config\>）
- [x] `commands.rs` — 10コマンド実装
- [x] `ui/src/lib/types.ts` — Rust DTOに対応するTypeScript型定義
- [x] `ui/src/lib/invoke.ts` — 型付きinvokeラッパー
- [x] `main.rs` にコマンド登録とAppState管理

### Phase 3: 検索UI ✅
- [x] `SearchWindow.tsx` — テキスト入力 + キーボードハンドリング
- [x] `ResultRow.tsx` — 結果行コンポーネント（アイコン表示対応）
- [x] `stores/search.ts` — 検索状態管理（クエリ、結果、選択、フォルダ展開、アイコンキャッシュ）
- [x] キーボード操作: Up/Down/Enter/Escape/Left/Right
- [x] フォルダ展開・Escape復帰ロジック
- [x] `styles/global.css` — CSS変数ベースのスタイリング

### Phase 4: プラットフォーム統合 ✅
- [x] `platform.rs` — Win32メッセージループスレッド（mpsc → Tauriイベントに変更）
- [x] `hotkey.rs` — `RegisterHotKey`/`UnregisterHotKey`（既存ロジック移植）
- [x] `ime.rs` — `ImmSetOpenStatus`（既存ロジック移植）
- [x] ホットキー受信 → `app_handle.emit("hotkey-pressed")` → ウィンドウ表示/非表示
- [x] トレイアイコン（手動Shell_NotifyIconW実装を移植）
- [x] トレイメニュー: 設定/終了
- [x] シングルインスタンスガード（`tauri-plugin-single-instance`）
- [x] 起動時表示制御（`visible: false` → 条件付き `window.show()`）
- [x] フロントエンドで `window-shown` イベントを受信し検索状態リセット

### Phase 5: 設定UI ✅
- [x] `SettingsWindow.tsx` — タブ切替コンテナ
- [x] `SettingsGeneral.tsx` — ホットキー、起動、自動非表示、トレイ、IME、タイトルバー
- [x] `SettingsSearch.tsx` — 検索モード、隠しファイル
- [x] `SettingsIndex.tsx` — 表示件数、幅、履歴、アイコン、追加パス、スキャンパス
- [x] `SettingsVisual.tsx` — テーマプリセット、カラー、フォント
- [x] `stores/settings.ts` — 設定ドラフト管理、保存
- [x] `styles/settings.css`
- [x] `open_settings` コマンド — `WebviewWindowBuilder`で第2ウィンドウ生成
- [x] ウィンドウラベルによる検索/設定画面の出し分け

### Phase 6: アイコン抽出パイプライン ✅
- [x] `src-tauri/src/icon.rs` — SHGetFileInfoW → BGRA → PNG → base64
- [x] `png` + `base64` クレート追加
- [x] `get_icon_base64` / `get_icons_batch` コマンド
- [x] `ResultRow.tsx` にアイコン `<img>` 追加（フォールバック付き）
- [x] フロントエンドにアイコンキャッシュ `Map<string, string>`
- [x] アイコンキャッシュの永続化（`icons.bin` 互換）

### Phase 7: ポリッシュ ✅
- [x] テーマ適用: `lib/theme.ts` → CSS変数を動的セット
- [x] ウィンドウ位置記憶: 検索/設定ウィンドウの位置・サイズをデバウンス保存・復元
  - `get_search_placement` / `save_search_placement`
  - `get_settings_placement` / `save_settings_placement` / `save_settings_size`
- [x] フォーカス喪失時自動非表示: `onFocusChanged` イベント
- [x] `/o` コマンド: 検索入力で `/o` + Enter → 設定ウィンドウ開く
- [x] ウィンドウ幅適用: `appearance.window_width` を起動時にセット
- [x] 設定保存後にホットキー再登録（`PlatformCommand::SetHotkey`）
- [x] 設定保存後にトレイ表示切替（`PlatformCommand::SetTrayVisible`）

---

## 残作業

### Phase 8: クリーンアップ（要確認）
- [ ] `legacy/` ディレクトリ削除
- [ ] `vendor/eframe/` ディレクトリ削除
- [ ] 旧 `src/` ディレクトリ削除（`snotra-core/src/` が正本）
- [ ] `CLAUDE.md` 更新: Tauri/SolidJSアーキテクチャ、ビルドコマンド、eframeパッチ記述削除
- [ ] `SPEC.md` 更新: 起動速度要件を500msに変更、Tauri/SolidJS記述
- [ ] `.gitignore` 最終整理

---

## 現在のプロジェクト構成

```
Snotra/
  Cargo.toml              # workspace (snotra-core, src-tauri)
  snotra-core/            # 純ロジック lib crate (9モジュール, 64テスト)
  src-tauri/              # Tauri v2 バイナリ crate
    src/main.rs           # エントリ + setup (hotkey/tray/IME/icon)
    src/commands.rs       # 15 Tauriコマンド
    src/state.rs          # AppState (Mutex<Engine/History/Config>)
    src/platform.rs       # Win32メッセージループ + トレイ
    src/hotkey.rs         # RegisterHotKey/UnregisterHotKey
    src/ime.rs            # ImmSetOpenStatus
    src/icon.rs           # HICON → BGRA → PNG → base64
    tauri.conf.json
    capabilities/default.json
  ui/                     # SolidJS フロントエンド
    index.html
    src/App.tsx           # ルート (検索/設定の出し分け, テーマ適用, 位置復元)
    src/components/       # SearchWindow, ResultRow, Settings*
    src/stores/           # search.ts, settings.ts
    src/styles/           # global.css, settings.css
    src/lib/              # types.ts, invoke.ts, theme.ts
  legacy/                 # 旧egui実装 (参照用, 削除予定)
  package.json, vite.config.ts, tsconfig.json
```

## ビルドコマンド

```bash
cargo test -p snotra-core        # ユニットテスト (64テスト)
cargo check -p snotra            # Rustバックエンド型チェック
npx vite build                   # フロントエンドビルド
npm run tauri dev                # 開発実行
npm run tauri build              # リリースビルド
```
