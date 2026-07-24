# アーキテクチャ

## プロジェクト概要

Snotra は Windows 専用のキーボードランチャー。全層 Rust（Tauri v2 ランタイム + egui/softbuffer 検索 UI・#532 で WebView2/SolidJS フロントを撤去）。システムトレイ/グローバルホットキー/IME などの Windows 固有機能は `windows` クレートで直接実装。グローバルホットキー（既定: `Alt+Q`）で検索ウィンドウを表示し、検索と起動を行う。

## ディレクトリ構成

```
Snotra/
  Cargo.toml              # workspace（製品3 crate + egui検証2 crate）
  snotra-core/            # 純ロジック lib crate
  snotra-egui-runtime/    # Tauri native Window向けegui/wgpu接着層
  snotra-egui-mvp/        # Issue #532の独立検証バイナリ（非配布）
  src-tauri/              # Tauri v2 バイナリ crate（egui_shell = 検索 UI）
  snotra-settings/        # egui 設定バイナリ（版数・About はサイドバー表示）
  package.json            # node（セーフティネット vitest + @tauri-apps/cli）
```

Cargo ワークスペース構成で、純ロジックライブラリ（`snotra-core`）、Tauri バイナリ（`src-tauri`）、設定 GUI（`snotra-settings`）を分離。検索 UI は `src-tauri/src/egui_shell/`（egui + `snotra-egui-runtime` の softbuffer CPU ラスタ）で、`Engine` を直接呼ぶ——IPC・TypeScript DTO は #532 SU7 で消滅した。設定は egui ベースの別プロセスで（版数・About 情報はサイドバーに表示）、`config.toml` ファイルを介して本体と連携する。`snotra-egui-mvp` は Issue #532 の採用判断に使った検証バイナリで、製品の既定起動経路・設定・配布物には接続しない。

## レイヤー構成

```
┌───────────────────────────────────────────────────────┐
│  src-tauri/ (Tauri v2 binary crate)                   │
│  egui_shell/* (検索 UI・softbuffer) ← snotra-egui-runtime │
│  commands/* (共有 core 関数)  ←  state.rs (Mutex<Engine>) │
│  platform/*  (Win32 message loop / hotkey / tray)     │
│  config_watcher / indexing / icon / monitor / ime     │
└────────────────────┬──────────────────────────────────┘
                     │ Rust 関数呼び出し
┌────────────────────▼──────────────────────────────────┐
│  snotra-core/ (pure-logic lib crate)                  │
│  Engine ← SearchEngine / HistoryStore / Config        │
│  indexer / folder / query / instant / binfmt          │
└───────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────┐
│  snotra-settings/ (egui standalone binary)            │
│  app.rs (draft/saved model) → tabs/*                  │
│  通信: config.toml ファイル経由のみ（IPC なし）       │
└───────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────┐
│  Issue #532 egui MVP（非配布）                        │
│  snotra-egui-mvp → snotra-egui-runtime → Tauri Window │
│                                      → egui-wgpu/wgpu  │
└───────────────────────────────────────────────────────┘
```

## 各クレート・パッケージの詳細

各モジュールの責務宣言は各ファイルの `//!`（Rust）/ TSDoc（TS）を正本とし、各サブディレクトリの `CLAUDE.md` はファイル名の索引と横断不変条件を持つ（#562。このセクションでは重複させない）。ここでは各クレート・パッケージの位置づけと主要な型のみを記す。

### snotra-core（純ロジック層）

Win32 依存なし（`#[cfg(windows)]` ゲート以外）、完全にユニットテスト可能。UI 表示文字列を持たない（エラー状態は `is_error: true` フラグで伝え、表示文字列は UI 層が決める）。多段ランク付き検索・履歴ブースト・インデックスキャッシュ・バイナリ永続化を担う純ロジック lib crate。

主要な型: `Engine`（全操作の入口）、`AppEntry`（インデックスエントリ）、`SearchResult`（検索結果 DTO）、`Config`（設定）、`PrebuiltIndex`（ロック外で構築→スワップ）、`FolderListContext`（スナップショットパターン）

→ モジュール構成は `snotra-core/CLAUDE.md`

### src-tauri（Tauri v2 バイナリ層）

Tauri v2 バイナリ crate（パッケージ名 `snotra`）。検索 UI（`egui_shell/`・egui + softbuffer）と Win32 API 統合（システムトレイ・グローバルホットキー・IME・マルチモニター）を担当。`commands/` は UI・トレイが共有する core 関数群（旧 IPC ハンドラは #532 SU7 で撤去・`LaunchResult` 契約等は残置）、`platform/` は Win32 メッセージループ・ホットキー・トレイのネイティブ統合。

→ モジュール構成は `src-tauri/CLAUDE.md`

### snotra-settings（egui 設定 GUI）

独立バイナリ。本体との通信は `config.toml` ファイル経由のみ（IPC なし）。`app.rs` の draft/saved 二重状態モデルを核に、`tabs/` のタブ式設定エディタを構成する egui アプリ。

→ モジュール構成は `snotra-settings/CLAUDE.md`

### snotra-egui-runtime / snotra-egui-mvp（Issue #532 検証層）

`snotra-egui-runtime`はTauri wry pluginでTaoイベントを受け、egui入力・Win32 IME composition・repaint・wgpu Surface／Device復旧をWindow単位で管理する接着層。`snotra-egui-mvp`は固定10,000件の実`Engine`検索、現行版相当の`Alt+Q`、3更新モードと署名検証付きダウンロードを組み合わせた独立バイナリであり、`app.windows`を空にしてネイティブ`Window`だけを生成する。製品版へ統合する前の技術検証に限定し、release workflowのartifactには含めない。

→ モジュール構成と制約は `snotra-egui-runtime/CLAUDE.md`、`snotra-egui-mvp/CLAUDE.md`

## 横断的な実装パターン

### ウィンドウ管理

- 検索ウィンドウは起動時に作成し `visible: false`、ホットキーで表示/非表示を切替
- 検索バーと検索結果は単一 egui ウィンドウ内の描画（`egui_shell/view.rs`）
- 結果の表示/非表示は `search_state.rs` の純粋核（view 種別 = tool>folder>results の優先度射影 + indexing 表示ゲート）で制御
- ウィンドウ高さは view が `compute_window_height` で算出し `set_size` する（view 単独 size writer）。show 時に 52px へリセットしてから結果に応じて拡張する
- マルチモニター: モニター作業領域原点からの相対座標（物理ピクセル）で位置を保存。ホットキー押下時にターゲットモニターを決定し絶対座標に変換

### 起動シーケンスと初期化順序

- `platform/mod.rs` の Win32 メッセージループスレッドはウィンドウ生成より前に spawn し、Win32 初期化とウィンドウ生成を並列実行（起動時間短縮）
- トレイアイコンの表示はウィンドウ生成完了後に行う
- ホットキー登録（`RegisterHotKey`）は `hotkey-pressed` イベントリスナーの登録完了後に行う（「有効化 ≥ リスナー登録」不変条件）
- egui view は config を毎フレーム live-read するため bootstrap 一括取得は存在しない（#532 SU7 で IPC ごと撤去）

### 設定管理

- 設定は `snotra-settings.exe` を子プロセスとして起動。相互依存は `config.toml` ファイル1点のみ（IPC 不要）
- 本体は `notify` クレートで `config.toml` 変更を検知し即時反映する
- 子プロセス管理: `Mutex<Option<Child>>` で保持。起動時に重複チェック、監視スレッドで終了検知 + `alwaysOnTop` 復元、exit ハンドラで kill
- `snotra-settings` の設定編集は draft/saved 二重状態モデル（Save 時にのみ `config.toml` に書き込み）
- **config↔派生状態のコヒーレンシ**: config 由来の派生状態は「live-read（毎操作で読み直し＝即時整合）」と「構築時焼き込み（要再構築）」の 2 種。後者の中核 `SearchEngine`（index）の整合は engine の `index_stale` ledger が所有し、`config_watcher` が `IndexInputs` 差分で `start_index_build` を kick → ロック外ビルド → `complete_index_drain` の re-diff で stale をクリアする（ビルド進行中の設定変更も取りこぼさない）。`HistoryStore` の剪定容量は live-read 化しキャッシュを持たない。詳細は `docs/design/2026-05-31-coherence-staleset.md`（StaleSet 契約）

### データ永続化

- 履歴/インデックス/アイコン/ウィンドウ位置は `.tmp` を使った原子的書き込み
- `%APPDATA%\Snotra\` にファイル分割保存: `index.bin`, `icons.bin`, `history.bin`, `window.bin`, `config.toml`
- バイナリファイルは先頭に `magic + u32 version` ヘッダ。読み込みはバージョンフォールバック
- アイコンは検索時にオンデマンドで抽出し PNG バイト列としてキャッシュ。`icons.bin` は初回アイコン取得時に遅延ロード

### アイコンパイプライン

- `SHGetFileInfoW` → HICON → BGRA → PNG で抽出（`icons.bin` に LRU キャッシュ・遅延ロード）
- egui へは `commands::load_icon_pngs`（worker スレッド）→ ColorImage decode → `load_texture`（`egui_shell/icon_textures.rs`・#532 SU4）
- path キーで stale 無害・in-flight 重複 spawn 防止・clear-on-hide でメモリ境界

### 多言語対応（3層）

- 検索 UI: `src-tauri/src/egui_shell/strings.rs` — 文言テーブル（言語は view の毎フレーム live-read）
- バックエンド: `config_watcher.rs` — `PlatformCommand::SetLanguage` でトレイ切替
- 設定 GUI: `snotra-settings/src/i18n.rs` — `Tr` 構造体 + `TrKey` enum のテーブル駆動翻訳（`t(key)`/`t_params(key, params)` + `{param}` プレースホルダ置換）
- 初期言語は OS 設定から自動判定（`sys-locale`・`ja` で始まれば日本語、それ以外は英語）

### インスタントコマンド（4層）

- 純ロジック: `snotra-core/src/instant.rs` — 変数展開 `{query}` / `{clip}` / `{date:書式}` / `{uuid}`（修飾子パイプ `{name | lower|upper|trim|default:x|raw}` 対応）+ `{{…}}` リテラルエスケープ + 前方一致フィルタ。date は strftime（不正書式は空文字でフォールバック＝panic 回避）、uuid は v4。`{{X}}` は literal `{X}`（変数名と衝突する literal の opt-out）。エンコードはシンク（種別）責務で URL 判定時に自動付与、`raw` で抑止。不明修飾子は `Config::validate` が保存時に拒否
- 実行分岐: `src-tauri/src/commands/instant.rs` の `execute_instant_action_core` — 種別分岐で実行（URL/Legacy は `expand_instant_command` → `launch_item_core`（ShellExecuteW）、Exec は `launch_exec_core`（exe + args 起動））。clipboard は呼び出し側がエンジンロック外で読む
- UI: `src-tauri/src/egui_shell/`（search_state.rs の `interpret` でモード判定・view.rs が直呼び実行・#532 SU7 で WebView2 UI 撤去）。indexing 中でも使用可能
- 設定 GUI: `snotra-settings/src/tabs/instant.rs` — プレフィックス設定 + コマンド CRUD + 展開プレビュー
- プレフィックス変更は egui が `config-applied` wake 後の live-read で拾う

### カスタムオープナー

- `config.toml` の `[[openers]]` でルール定義（`target` + `tools`）
- 全起動経路で統一適用（通常 Enter / Shift+Enter / クリック / トレイ履歴メニュー）
- 最具体ルール1つだけ採用（排他）。具体度 = パス条件の長さ
- Shift+Enter でツール選択メニュー表示（2件以上の場合）
- ツール引数: 固定引数 + パス末尾付加。`{path}` プレースホルダ対応

### 自動更新

- `tauri-plugin-updater` で GitHub Releases 経由
- 3モード: `full`（トースト + インストールボタン）/ `check_only`（通知のみ）/ `disabled`
- トースト UI は検索バーと結果リストの間に 52px で表示
- リリース形式: ポータブル ZIP + NSIS インストーラー

### その他のパターン

- フォルダ展開は「開始時スナップショットを保持し、`Escape` で一括復帰」モデル
- テーマは CSS カスタムプロパティで動的に切替（`document.documentElement.style.setProperty()`）
- `launch_item` は `LaunchResult(status/code/message)` を返す契約。失敗通知の自動クリアは単一タイマーを再利用して競合防止
- 隣接バイナリ（`snotra-settings.exe`）を追加・変更した場合は `release.yml` のビルドステップと artifact 検証ステップの確認が必要

## 検索フロー（入力 → 結果表示）

```mermaid
sequenceDiagram
    participant User
    participant SW as SearchWindow.tsx
    participant SS as search.ts (store)
    participant API as invoke.ts (IPC)
    participant Cmd as commands/search.rs
    participant Eng as Engine (snotra-core)
    participant SE as SearchEngine

    User->>SW: キー入力
    SW->>SS: setQuery(value)

    Note over SS: createEffect が query 変更を検知

    SS->>SS: debouncedRefresh()<br/>OwnedTimer(refreshTimer) で leading+trailing 50ms

    SS->>SS: refreshResults()<br/>searchLane.run() (world 世代 +1・stale 検出用)
    SS->>API: search(query)
    API->>Cmd: invoke("search", { query })
    Cmd->>Eng: engine.search(&query)
    Eng->>SE: search_with_options()

    Note over SE: rayon 並列スコアリング<br/>1. Bitmask プレフィルタ (Fuzzy)<br/>2. match_score (Prefix/Substring/Fuzzy)<br/>3. 履歴ブースト<br/>4. BinaryHeap top-k

    SE-->>Eng: Vec<SearchResult>
    Eng-->>Cmd: Vec<SearchResult>
    Cmd-->>API: JSON シリアライズ
    API-->>SS: SearchResult[]

    Note over SS: run ctx の isStale() で stale チェック

    SS->>SS: setResults(items), setSelected(0)

    SS->>API: getIconsBatch(paths)
    API->>Cmd: invoke("get_icons_batch")

    Note over Cmd: ipc::Response (バイナリ)<br/>custom protocol 経由で ArrayBuffer

    Cmd-->>SS: ArrayBuffer (PNG バッチ)
    SS->>SS: parseBinaryBatch()<br/>→ Blob URL 生成
```

**補足**:
- 検索/データ lane の world 世代は `latestRun` primitive（`searchLane`）が所有する。`run()` が実行ごとに世代を +1 して `isStale()` を渡し、応答が返ったとき最新世代と比較して古いレスポンスを破棄する。モード遷移・起動は `searchLane.invalidate()` で世代を進め in-flight 検索を supersede する（#534）
- 起動（launch/activate）lane は `exclusive` primitive（`activationLane`）が単一の in-flight フラグを所有し、実行中の 2 つ目の起動を `false` で拒否する（single-flight mutex）。検索 lane の supersede と対をなす 2 方針——検索は「新しい実行が古い実行を無効化」、起動は「実行中は 2 つ目を拒否」——を別名 primitive で明示する（#535）
- アイコンは `ipc::Response` でバイナリ返却するため、CSP の `connect-src` に `ipc: http://ipc.localhost` が必須（`tauri dev` では不要だがリリースビルドで必要）

## 状態遷移（概要）

```
LauncherStopped → Standby → SearchVisible
                              ├── NormalMode
                              ├── CommandMode (/コマンド入力)
                              ├── InstantCommandMode (@プレフィックス入力)
                              ├── FolderExpansionMode (ArrowRight/Left)
                              ├── ToolSelectionMode (Shift+Enter)
                              └── IndexingMode (構築中)
```

- 上図の括弧内は実装上 **2 軸 + オーバーレイ**に対応: NormalMode/CommandMode/InstantCommandMode は軸2 `interpKind`（plain/command/instant）、FolderExpansionMode/ToolSelectionMode は軸1 `viewKind`（folder/tool）。**IndexingMode は排他モードではなくオーバーレイ**（`indexing` はどのモードにも重なる）
- `Escape` は内側のモードから順に復帰（ToolSelection → FolderExpansion → NormalMode → Standby）
- `snotra-settings` は子プロセスとして起動され、本体の状態遷移には影響しない
- 詳細な遷移ルールは `SPEC.md` §8.6 を参照

## 参照先

- 意図（仕様）: `SPEC.md`
- 設定値・デフォルト: `snotra-core/src/config.rs`
- パフォーマンス最適化: `PERFORMANCE.md`
- モジュール詳細: 各サブディレクトリの `CLAUDE.md`
- config↔派生状態コヒーレンシ設計（StaleSet 契約）: `docs/design/2026-05-31-coherence-staleset.md`
