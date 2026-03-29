# ビルド・実行コマンド

**環境を確認の上、実行してください。**

## Windows/macOS/Linux で実行可能

```bash
npm test                          # フロントユニットテスト（Vitest）
npm run build                    # フロントエンドビルド（typecheck → vite build、プロジェクトルートから実行）
```

## Windows のみ実行可能（`windows` クレートや Win32 API・実行バイナリに依存）

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
npm run tauri build              # リリースビルド（フロント+Rust 一括。cargo build --release 単体は UI が壊れる）
```

## E2E/スモーク運用メモ

- `scripts/smoke-startup.ps1` は `SNOTRA_TRACE=1` で起動し、`*:error` トレースイベントが不在であることを検証する
- `e2e/tauri.slash.e2e.ts` は Playwright runner 上で `tauri-driver + selenium-webdriver + edgedriver` を使い、起動入力・`/o` の動作を検証する
- E2E セットアップは `npx tauri build --no-bundle` を使う（`cargo build --release` は `localhost` 向きバイナリになり `ERR_CONNECTION_REFUSED` で失敗する）
- スラッシュコマンドの実行順（`hide -> /r|/o|/s|/q`）は `ui/src/lib/commands.test.ts` で固定し、順序変更時は必ず更新する
- Tauri Driver E2E の可視判定は `document.visibilityState` を真実源にしない。`plugin:window|is_visible` を優先して判定する
- **`snotra-settings` は egui ネイティブウィンドウのため WebDriver から完全に不可視**: `waitForVisibleLabel(driver, "settings", ...)` は常にタイムアウトする。`/o` コマンドの副作用（`main.alwaysOnTop → false`）など、Tauri WebView 側で観測可能な状態変化で間接的に検証すること
- **`waitForVisibleLabel` / `waitForHiddenLabel` 後は必ず `switchToLabel` を呼ぶ**: これらの関数は内部でウィンドウを切り替えるため、返却後のドライバーコンテキストが期待のウィンドウにない場合がある。直後に `findElement` すると `NoSuchElementError` になる
- **fixture インデックスは `[[paths.scan]]` + `extensions` で指定する**: E2E config の `paths.additional` はレガシーで `.lnk` 専用に migrate される。`.txt` 等の fixture ファイルをインデックスに載せるには `[[paths.scan]]` に `extensions = [".txt"]` を明示すること

## CI/CD メモ

- **`GITHUB_TOKEN` では他のワークフローをトリガーできない**: tag push や `workflow_dispatch` を `GITHUB_TOKEN` で発火させても、別ワークフローは起動しない（GitHub の仕様）。ワークフロー間の連鎖には `workflow_call`（呼び出し元から直接呼ぶ）を使う
