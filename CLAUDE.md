# CLAUDE.md

このファイルは、このリポジトリで Claude Code が作業するときの運用ガイドです。

## このファイルの目的

- 実装時の判断基準と作業ルールを短時間で確認できるようにする
- コード変更時に守るべき原則を明確化する
- `*.md` で意図（何を実現したいか）を管理し、コードで実装事実（どう実装したか）を管理する

## 3層分担（責務分離）

- 第1層（意図管理）: `SPEC.md` と `CLAUDE.md`
  - `SPEC.md`: あるべき仕様、要件、振る舞いの定義
  - `CLAUDE.md`: 実装時の運用ルールと判断基準
- 第2層（実装事実）: `snotra-core/src/*.rs`, `src-tauri/src/*.rs`, `ui/src/**`
  - 現在の実際の動作・制約・実装詳細
- 第3層（整合運用）: 変更時の同期ルール
  - 挙動変更を伴う変更では、意図（`SPEC.md`）と実装を同時に整合させる

### 不一致時の扱い

- まず「バグ」か「仕様変更」かを判定する
- バグの場合:
  - `SPEC.md` の意図に合わせてコードを修正する
  - 必要に応じて `CLAUDE.md` の運用記述を更新する
- 仕様変更の場合:
  - 先に `SPEC.md` を更新して意図を確定する
  - 次にコードを更新する
  - 最後に `CLAUDE.md` を更新して運用ルールと参照先を整合させる

## プロジェクト概要

Snotra は Windows 専用のキーボードランチャーです。バックエンドは Rust（Tauri v2）、フロントエンドは SolidJS + TypeScript で構築しています。システムトレイ/グローバルホットキー/IME などの Windows 固有機能は `windows` クレートで直接実装しています。グローバルホットキー（既定: `Alt+Q`）で検索ウィンドウを表示し、検索と起動を行います。

## ビルド・実行コマンド

```bash
cargo test -p snotra-core        # ユニットテスト（64テスト）
cargo check -p snotra            # Rustバックエンド型チェック
cargo clippy -p snotra-core -p snotra  # lint チェック
npx vite build                   # フロントエンドビルド
npm run tauri dev                # 開発実行（ホットリロード付き）
npm run tauri build              # リリースビルド
```

## アーキテクチャ概要

Cargo ワークスペース構成で、純ロジックライブラリ（`snotra-core`）と Tauri バイナリ（`src-tauri`）を分離。GUI は SolidJS + CSS 変数ベースのテーマシステムで、Tauri IPC 経由で Rust バックエンドと通信します。

### ディレクトリ構成

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
  package.json, vite.config.ts, tsconfig.json
```

### モジュール構成

**snotra-core（純ロジック）:**

- `config.rs`: `%APPDATA%\Snotra\config.toml` の読込/保存、既定値補完
- `search.rs`: 検索順位計算（先頭/中間/ファジー）、履歴ブースト、空クエリ時履歴候補
- `history.rs`: 起動履歴・クエリ別履歴・フォルダ展開履歴の管理、バイナリ永続化
- `folder.rs`: フォルダ内列挙とフィルタ/ソート、ルート判定
- `indexer.rs`: スキャン対象列挙と重複排除、インデックスキャッシュ
- `query.rs`: クエリ正規化
- `binfmt.rs`: `magic + version` 付きバイナリ入出力共通処理
- `window_data.rs`: ウィンドウ位置（`window.bin`）の保存/復元
- `ui_types.rs`: フロントエンドとの IPC 用データ型

**src-tauri（Tauri バイナリ）:**

- `main.rs`: エントリポイント、Tauri セットアップ、イベントリスナー登録
- `commands.rs`: 15個の `#[tauri::command]`（検索/履歴/設定/アイコン/ウィンドウ位置）
- `state.rs`: `AppState` 定義（`Mutex<SearchEngine>`, `Mutex<HistoryStore>`, `Mutex<Config>`）
- `platform.rs`: Win32 メッセージループスレッド + トレイアイコン（Tauri イベント経由で通信）
- `hotkey.rs`: グローバルホットキー登録/解除
- `ime.rs`: IME 制御
- `icon.rs`: アイコン抽出（`SHGetFileInfoW` → BGRA → PNG → base64）、キャッシュ永続化

**ui/src（SolidJS フロントエンド）:**

- `App.tsx`: ウィンドウラベルで検索/設定を出し分け、テーマ適用、ウィンドウ位置復元
- `components/SearchWindow.tsx`: 検索入力 + キーボードナビゲーション + `/o` コマンド
- `components/ResultRow.tsx`: アイコン + 名前 + パス + フォルダバッジ
- `stores/search.ts`: 検索状態管理（クエリ/結果/選択/フォルダ展開/アイコンキャッシュ）
- `stores/settings.ts`: 設定ドラフト管理
- `lib/invoke.ts`: 型付き Tauri IPC ラッパー
- `lib/theme.ts`: CSS 変数によるテーマ適用

### 実装上の重要パターン

- 検索ウィンドウは起動時に作成し `visible: false`、ホットキーで表示/非表示を切替
- ホットキーは `RegisterHotKey` を `platform.rs` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定ウィンドウは `WebviewWindowBuilder` で同一プロセス内の第2ウィンドウとして生成
- フォルダ展開は「開始時スナップショットを保持し、`Escape` で一括復帰」モデル
- 履歴/インデックス/アイコン保存は `.tmp` を使った原子的書き込み
- アイコンは base64 エンコード PNG としてフロントエンドに送り、`<img>` タグで表示
- テーマは CSS カスタムプロパティで動的に切替
- **Win32 メッセージ配送の注意**: Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている

## 開発原則

### TDD

- 純ロジックモジュール（`snotra-core/src/` 内）は `#[cfg(test)]` でユニットテストを追加する
- Win32 依存モジュール（`src-tauri/src/` 内の `hotkey.rs`, `ime.rs`, `platform.rs`）はユニットテスト前提にしない
- ロジック追加時は、可能な限り `snotra-core` に分離してテスト可能性を維持する

### KISS

- `main.rs` に業務ロジックを増やさない
- `commands.rs` は薄いラッパーに保ち、実処理は `snotra-core` に寄せる
- 責務を跨ぐ実装をしない

### DRY

- 検索スコア計算は `snotra-core/src/search.rs` に集約する
- フォルダ列挙・フィルタ・並び替えは `snotra-core/src/folder.rs` に集約する
- TypeScript 型定義は `ui/src/lib/types.ts` に集約する

### YAGNI

- 使う予定だけの抽象化（不要な trait/generics/レイヤー）を導入しない
- 現在の要求範囲を超える機能追加を行わない
- 拡張性より、現要件での単純さと可読性を優先する

## 参照先（3層分担）

- 意図（仕様）: `SPEC.md`
- 運用ルール: `CLAUDE.md`
- 実装事実（Rust）: `snotra-core/src/*.rs`, `src-tauri/src/*.rs`
- 実装事実（フロントエンド）: `ui/src/**`
- 設定値・デフォルト: `snotra-core/src/config.rs`
- 検索順位ロジック: `snotra-core/src/search.rs`
- 履歴保存仕様: `snotra-core/src/history.rs` と `snotra-core/src/binfmt.rs`
- Tauri コマンド一覧: `src-tauri/src/commands.rs`
- フロントエンド型定義: `ui/src/lib/types.ts`
