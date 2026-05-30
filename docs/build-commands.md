# ビルド・実行コマンド

**環境を確認の上、実行してください。**

このドキュメントは Snotra のビルド／検証コマンドの単一の真実源（SSOT）です。`AGENTS.md` の開発ワークフローや `.claude/skills/*/SKILL.md` の検証ステップは、コマンド本体をここに集約して参照します。コマンドを追加・変更するときはこのファイルのみを更新してください。

## 変更後の検証チェックリスト（必須・スキップ不可）

変更したファイルの種類に応じて、以下のカテゴリの必須コマンドを実行する。複数カテゴリに該当する場合はすべて実行する。

### A. Rust ファイル（`*.rs`）を変更した場合

```bash
cargo check -p snotra-core -p snotra -p snotra-settings     # 必須: Rust 全 crate 型チェック
```

- 追加検証: `cargo clippy -p snotra-core -p snotra -p snotra-settings -- -D warnings`、`cargo test -p snotra-core`
- `snotra-settings` を含めるのは egui ネイティブウィンドウ側の型壊れも検知するため

### B. TypeScript／フロントエンドファイル（`ui/src/**/*.{ts,tsx}` 等）を変更した場合

```bash
npm run typecheck    # 必須: TypeScript 型チェック
npm run build        # 必須: typecheck → vite build（プロジェクトルートから実行）
```

- `npm run build` は内部で `typecheck` を呼びますが、型エラーを早期に切り分けるため別途実行を推奨

### C. ウィンドウ生成／表示順・ホットキー・スラッシュコマンドに触れた場合（A／B に追加）

```bash
npm test                 # 必須: フロントユニットテスト（Vitest）
npm run smoke:startup    # 必須: 起動時ウィンドウ生成スモーク（trace 検証）
npm run e2e:tauri        # 必須: Playwright + Tauri Driver E2E
```

- 初回のみ `npm run e2e:tauri:setup` でセットアップが必要

### D. UI のスタイル・レイアウト・テキスト表示に影響する変更（A／B／C に追加）

`npm run tauri dev` で起動し、目視で overflow／clipping／フォントレンダリングを確認する。PR 作成前に必須。

## Windows/macOS/Linux で実行可能

```bash
npm test                          # フロントユニットテスト（Vitest）
npm run build                    # フロントエンドビルド（typecheck → vite build、プロジェクトルートから実行）
```

## Windows のみ実行可能（`windows` クレートや Win32 API・実行バイナリに依存）

```bash
npm ci                           # 依存インストール（初回セットアップ・CI）
cargo test -p snotra-core        # ユニットテスト
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測（詳細: PERFORMANCE.md）
cargo check -p snotra-core -p snotra -p snotra-settings  # Rust 全 crate 型チェック
cargo clippy -p snotra-core -p snotra -p snotra-settings  # lint チェック
cargo run -p snotra-settings     # snotra-settings（egui ネイティブ設定 GUI）の単独起動
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
- **E2E ハーネスは msedgedriver を WebView2 Runtime のバージョンに合わせて自動解決する**（`resolveWebView2DriverVersion`）。アプリが automation するのは Edge ブラウザではなく WebView2 Runtime であり、両者はパッチレベルでドリフトする。不一致は全セッションが `session not created: Chrome instance exited` で失敗する。`EDGEDRIVER_VERSION` で明示上書き可能
- **E2E が生成する `config.toml` は妥当な TOML でなければならない**: parse 失敗時アプリは `Config::default()`（Start Menu / Desktop スキャン）にフォールバックするため、fixture が索引されず検索系テストが全滅する。`buildE2EConfigToml` を編集したら生成 TOML の妥当性を確認する。TOML 文字列に `"` を含む値は JS テンプレートリテラルの `\"` が `"` に潰れて不正になりやすいため、TOML リテラル文字列（シングルクォート）を使う。#338 で parse 失敗時に stderr ログ + `config.toml.bak` 退避を実装済み（黙殺は解消）。ただし default フォールバック自体は不変で、E2E が stderr を拾わなければ症状は同じため、E2E config は依然 valid TOML が必須

## CI/CD メモ

- **`GITHUB_TOKEN` では他のワークフローをトリガーできない**: tag push や `workflow_dispatch` を `GITHUB_TOKEN` で発火させても、別ワークフローは起動しない（GitHub の仕様）。ワークフロー間の連鎖には `workflow_call`（呼び出し元から直接呼ぶ）を使う
