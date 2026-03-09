# AGENTS.md

このリポジトリで AI エージェントが作業するときのプロダクト知識。エージェント固有のルール・ワークフローは各エージェントの設定ファイル（`CLAUDE.md` 等）を参照。

## プロジェクト概要

Snotra は Windows 専用のキーボードランチャー。バックエンドは Rust（Tauri v2）、フロントエンドは SolidJS + TypeScript で構築。システムトレイ/グローバルホットキー/IME などの Windows 固有機能は `windows` クレートで直接実装。グローバルホットキー（既定: `Alt+Q`）で検索ウィンドウを表示し、検索と起動を行う。

## ディレクトリ構成

```
Snotra/
  Cargo.toml              # workspace (snotra-core, src-tauri, snotra-settings)
  snotra-core/            # 純ロジック lib crate
  src-tauri/              # Tauri v2 バイナリ crate
  snotra-settings/        # egui 設定バイナリ（About タブ統合）
  ui/                     # SolidJS フロントエンド
  package.json, vite.config.ts, tsconfig.json
```

Cargo ワークスペース構成で、純ロジックライブラリ（`snotra-core`）、Tauri バイナリ（`src-tauri`）、設定 GUI（`snotra-settings`）を分離。検索 UI は SolidJS + CSS 変数ベースのテーマシステムで Tauri IPC 経由で Rust バックエンドと通信。設定は egui ベースの別プロセスで（About 情報はタブとして統合）、`config.toml` ファイルを介して本体と連携する。

## 横断的な実装パターン

- 検索ウィンドウは起動時に作成し `visible: false`、ホットキーで表示/非表示を切替
- フォルダ展開は「開始時スナップショットを保持し、`Escape` で一括復帰」モデル
- 履歴/インデックス/アイコン保存は `.tmp` を使った原子的書き込み
- アイコンは検索時にオンデマンドで抽出し base64 PNG としてフロントエンドに送信、キャッシュは終了時に永続化
- テーマは CSS カスタムプロパティで動的に切替
- 多言語対応は3層で実装: フロントエンド（`ui/src/lib/i18n.ts` — SolidJS シグナル + 翻訳テーブル）、バックエンド（`config_watcher.rs` — `language-changed` イベント発火 + `PlatformCommand::SetLanguage` でトレイ切替）、設定 GUI（`snotra-settings/src/i18n.rs` — `Tr` 構造体の match ベース翻訳）。初期言語は OS 設定から自動判定（Rust: `sys-locale`、JS: `navigator.language`、同一ロジック）
- 検索バーと検索結果は単一ウィンドウ内のコンポーネント（`SearchWindow` + `ResultsSection`）。結果の表示/非表示は `shouldShowResults` メモシグナル（`results().length > 0 && (!indexing() || instantCommandMode())`）で制御し、ウィンドウ高さは `createEffect` + Tauri `set_size()` で動的に変更する。Rust 側の `show_main_and_emit` で毎回 52px にリセットしてからフロントエンドが結果に応じて拡張する
- インスタントコマンドはプレフィックス（デフォルト `@`）で始まる入力でユーザー定義コマンドを即座に実行する機能。4層で実装: 純ロジック（`snotra-core/src/instant.rs` — 変数展開 `{query}`/`{clip}` + 前方一致フィルタ）、IPC（`src-tauri/src/commands/instant.rs` — クリップボード読み取り + ShellExecuteW）、UI（`ui/src/stores/search.ts` — `instantCommandMode` シグナル + query effect でモード切替）、設定 GUI（`snotra-settings/src/tabs/instant.rs`）。プレフィックス変更は `config_watcher.rs` が `instant-prefix-changed` イベントで UI に通知する
- `launch_item` は `LaunchResult(status/code/message)` を返す契約で扱い、失敗通知の自動クリアは単一タイマーを再利用して競合を防ぐ
- 起動時にスレッドを並列 spawn する場合、そのスレッドが発火するイベントに依存する機能（ホットキー・トレイ等）はスレッド init フェーズで有効化せず、main 側でリスナー/ウィンドウ準備が整った後にコマンド（`RegisterInitialHotkey` / `SetTrayVisible`）で有効化する（「有効化 ≥ リスナー登録」不変条件）
- 設定は `snotra-settings.exe` を子プロセスとして起動する（About 情報はタブに統合）。相互依存は `config.toml` ファイル1点のみ（IPC 不要）。本体は `notify` クレートで config.toml 変更を検知し即時反映する
- 子プロセス管理: `Mutex<Option<Child>>` で保持し、起動時に重複チェック、監視スレッドで終了検知 + alwaysOnTop 復元、exit ハンドラで kill。**子プロセスを spawn する場合は exit ハンドラでの kill を必ずペアで追加する**
- 子プロセスとして起動する外部バイナリ（`snotra-settings.exe` 等）は、Cargo ワークスペースの依存関係外にある場合リリースワークフローでビルドされない。**隣接バイナリを追加・変更した場合は `release.yml` のビルドステップと artifact 検証ステップを必ず確認する**

## 参照先

- 意図（仕様）: `SPEC.md`
- 設定値・デフォルト: `snotra-core/src/config.rs`
- パフォーマンス最適化: `PERFORMANCE.md`

## ビルド・実行コマンド

**Windows 不要**（macOS/Linux でも実行可能）:

```bash
npm test                          # フロントユニットテスト（Vitest）
npm run build                    # フロントエンドビルド（typecheck → vite build、プロジェクトルートから実行）
```

**Windows 必須**（`windows` クレートや Win32 API・実行バイナリに依存）:

```bash
cargo test -p snotra-core        # ユニットテスト
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測
cargo check -p snotra-core -p snotra -p snotra-settings  # Rust 全 crate 型チェック
cargo clippy -p snotra-core -p snotra -p snotra-settings  # lint チェック
npm run verify                   # Rust + フロントエンド一括検証（cargo check + npm run build）
npm run smoke:startup             # 起動時ウィンドウ生成スモーク（trace検証）
npm run e2e:tauri:setup           # Tauri Driver E2E 用セットアップ
npm run e2e:tauri                 # Playwright + Tauri Driver E2E
npm run tauri dev                # 開発実行（ホットリロード付き）
npm run tauri build              # リリースビルド
```

### E2E/スモーク運用メモ

- `scripts/smoke-startup.ps1` は `SNOTRA_TRACE=1` で起動し、`*:error` トレースイベントが不在であることを検証する
- `e2e/tauri.slash.e2e.ts` は Playwright runner 上で `tauri-driver + selenium-webdriver + edgedriver` を使い、起動入力・`/o` の動作を検証する
- E2E セットアップは `npx tauri build --no-bundle` を使う（`cargo build --release` は `localhost` 向きバイナリになり `ERR_CONNECTION_REFUSED` で失敗する）
- スラッシュコマンドの実行順（`hide -> /r|/o|/s|/q`）は `ui/src/lib/commands.test.ts` で固定し、順序変更時は必ず更新する
- Tauri Driver E2E の可視判定は `document.visibilityState` を真実源にしない。`plugin:window|is_visible` を優先して判定する
- **`snotra-settings` は egui ネイティブウィンドウのため WebDriver から完全に不可視**: `waitForVisibleLabel(driver, "settings", ...)` は常にタイムアウトする。`/o` コマンドの副作用（`main.alwaysOnTop → false`）など、Tauri WebView 側で観測可能な状態変化で間接的に検証すること
- **`waitForVisibleLabel` / `waitForHiddenLabel` 後は必ず `switchToLabel` を呼ぶ**: これらの関数は内部でウィンドウを切り替えるため、返却後のドライバーコンテキストが期待のウィンドウにない場合がある。直後に `findElement` すると `NoSuchElementError` になる
- **fixture インデックスは `[[paths.scan]]` + `extensions` で指定する**: E2E config の `paths.additional` はレガシーで `.lnk` 専用に migrate される。`.txt` 等の fixture ファイルをインデックスに載せるには `[[paths.scan]]` に `extensions = [".txt"]` を明示すること

## 開発原則

### KISS

- `main.rs` に業務ロジックを増やさない
- 責務を跨ぐ実装をしない
- 新規コードは既存のファイル構成・命名規則・スタイルパターンに合わせる。独自パターンを導入する前に既存パターンの利用を検討する

### DRY

- 責務の集約先は各サブディレクトリのドキュメント（`snotra-core/CLAUDE.md`, `src-tauri/CLAUDE.md`, `ui/CLAUDE.md`, `snotra-settings/CLAUDE.md`）に記載
- 同一ロジックの繰り返しは2回まで許容し、3回目で抽出を検討する（無理な抽象化よりも多少の重複を許容）

### YAGNI

- 使う予定だけの抽象化（不要な trait/generics/レイヤー）を導入しない
- 現在の要求範囲を超える機能追加を行わない
- 拡張性より、現要件での単純さと可読性を優先する

## デバッグ・バグ修正の原則

- バグ修正時は、コードを書く前に根本原因を一文で明示する
- 根本原因の説明には「壊れた不変条件（何が常に成り立つべきだったか）」を必ず1つ含める
- 最初の修正案が失敗した場合、同じ深さで別の推測を試みるのではなく、より深い調査に切り替える
- バグ修正時は、修正対象のパターンをコードベース全体で検索し、同一パターンが他の箇所にも存在しないか確認してから完了とする
- `snotra-core`（純ロジック層）に UI 表示文字列を持たない。エラー状態の意味は `is_error: true` フラグで伝え、エラーメッセージのような表示文字列は UI 層（`ResultRow.tsx` 等）が決める責務を持つ
- Win32 / Tauri 固有の注意事項は `src-tauri/CLAUDE.md`、データ永続化の注意は `snotra-core/CLAUDE.md` に詳細あり。実装前チェックは `.claude/rules/` で自動配送される
- `tauri.conf.json` や platform 固有ファイルに設定を追加する際は、その設定が Windows でサポートされているか事前に確認する（例: `backgroundThrottlingPolicy` は Windows 非対応でビルドエラーになる）
- 修正案が API 境界をまたぐとき、「呼び出し側パッチ」と「API 側で責務を完結させる修正」の両案を比較し、後者を優先する
- 競合しやすい一時状態（通知・ローディング・遅延処理）を導入する場合は、タイマー/購読のライフサイクルを単一管理し、再実行時に必ず前回ハンドルを破棄する
