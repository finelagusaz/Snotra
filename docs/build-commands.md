# ビルド・実行コマンド

**環境を確認の上、実行してください。**

このドキュメントは Snotra のビルド／検証コマンドの単一の真実源（SSOT）です。`AGENTS.md` の開発ワークフローや `.claude/skills/*/SKILL.md` の検証ステップは、コマンド本体をここに集約して参照します。コマンドを追加・変更するときはこのファイルのみを更新してください。

## 変更後の検証チェックリスト（必須・スキップ不可）

変更したファイルの種類に応じて、以下のカテゴリの必須コマンドを実行する。複数カテゴリに該当する場合はすべて実行する。

### A. Rust ファイル（`*.rs`）を変更した場合

```bash
cargo check --workspace                                                                # 必須: Rust 全 crate 型チェック
cargo clippy --workspace --all-targets -- -D warnings                                 # 必須: lint（全 .rs 変更、テストターゲット含む）
cargo test -p snotra-core                                                              # 必須（snotra-core を変更した場合）: 純ロジック層 TDD
cargo test -p snotra                                                                   # 必須（src-tauri を変更した場合）: Tauri 統合層のユニットテスト
cargo test -p snotra-settings                                                          # 必須（snotra-settings を変更した場合）: 設定 GUI の純ロジックテスト
```

- **`cargo test` の必須/任意**: 変更した crate のテストはローカル**必須**（PostToolUse フックが自動実行）。変更していない crate のテストはローカル任意（CI の rust-check が PR で全 3 crate のテストを常に実行し担保）
- CI（`ci.yml` rust-check）は `cargo clippy --workspace --all-targets` と全 3 crate のテストを PR で自動実行する。`cargo check` はローカル必須の早期型検査として残し、CI では検査対象を包含する clippy に統合する（「CI/CD メモ」の対応表参照）。PostToolUse フック（`.claude/hooks/post-edit.mjs`）も `*.rs` 編集で clippy、`snotra-core/**` / `src-tauri/**` / `snotra-settings/**` 編集でその crate のテストを自動発火する。`src-tauri/tauri.conf.json` の編集では CSP 契約テスト（`ui/src/lib/cspValidation.test.ts`）を、`Cargo.toml` の編集では `cargo check` を自動発火する（ルートの `Cargo.toml` ではさらに hook-selftest = members カナリア）
- **`check` / `clippy` は `--workspace` を使う**（#500）。crate 名を `-p` で列挙すると `Cargo.toml` の `members` の写しになり、4 つ目の crate を追加したとき hook・CI・本ファイルが同じ誤りを共有して静かに漏れる。`--workspace` は cargo に真実源を読ませる。一方 `cargo test -p <crate>` は「編集した crate → そのテスト」の写像なので `-p` のまま残す（`--workspace` にすると編集していない crate のテストまで走る）
- **フックの検査コマンドと本ファイルの整合規約**: フックの cargo コマンドは、**カテゴリ A のコードブロック**の記載と**合否・検査対象を変えるフラグにおいて一致**させる（`--lib` の付与・`-p` の欠落等を乖離とする）。**出力整形のみのフラグ**（`--message-format short` 等、exit code を変えないもの）は hook 側の証拠予算のための追加として許容する。npm 系検査は SSOT コマンド（`npm test` / `npm run typecheck`）の部分集合ラッパー（単一テストファイルの vitest 実行・tsc 直接起動）を許容する。乖離は `/health-check` Check 5 が検知する
- **検査が割り当てられているファイルでは、フックの沈黙は合格を意味する**（#471・前提条件は #497）。検出は exit code で行い、成功した検査は何も出力しない。失敗時のみ再現コマンド付きで会話に届くため、そのコマンドを実行すれば全診断を見られる。**割り当ての無いファイル**（`*.md`・`scripts/`・`.github/workflows/` 等）の沈黙は「何も走らなかった」であり合格ではない。割り当ての SSOT は `post-edit.mjs` の `selectChecks` である
- `snotra-settings` を含めるのは egui ネイティブウィンドウ側の型壊れも検知するため

### B. TypeScript／フロントエンドファイル（`ui/src/**`・`e2e/**`・ルートの config `.ts`）を変更した場合

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
- **PR 上の実行責任**: `npm test` は通常 PR CI（`ci.yml`）で自動実行されるが、`smoke:startup` / `e2e:tauri` は**通常 PR CI では走らない**。`src-tauri`・`ui`・`e2e`・依存 manifest/lockfile 等を含む変更は `E2E & Smoke` workflow（`e2e.yml`）が **paths により自動起動**し smoke + e2e を実行する（#145 Phase 3）。paths 外の変更で E2E を回したいときは `workflow_dispatch`（手動実行）。「通常 CI が緑」だけでは smoke/E2E 済みを意味しない

### D. UI のスタイル・レイアウト・テキスト表示に影響する変更（A／B／C に追加）

`npm run tauri dev` で起動し、目視で overflow／clipping／フォントレンダリングを確認する。PR 作成前に必須。

### E. git hook（`.githooks/**`）を変更した場合

```bash
npm test    # 必須: 使い捨て repo で hook を実測する（.githooks/githooks.test.mjs）
```

- PostToolUse フックが `.githooks/**` の編集で `vitest run .githooks` を自動発火する（#484）。`.claude/hooks/**` と同じ理由 — 安全網そのものを編集したら、安全網が生きているか確かめる
- `.githooks/` は **main 保護のローカル層**。commit / merge / rebase / push の各操作で git が直接呼ぶため、ツール・シェル・worktree・`git -C` のいずれにも依存しない
- **bootstrap**: `npm install` / `npm ci` が `prepare` スクリプトで `git config core.hooksPath .githooks` を実行する。worktree は `.git/config` を共有するため一度で全 worktree に効く
- この層は best-effort。`core.hooksPath` が外れても **GitHub ruleset（`default`）が main への直接 push を拒否する**ため、外れたことを検知する仕組みは意図的に設けていない

## Windows/macOS/Linux で実行可能

```bash
npm test                          # フロントユニットテスト（Vitest）
npm run build                    # フロントエンドビルド（typecheck → vite build、プロジェクトルートから実行）
npm run clean:worktrees          # Agent 委譲で残った worktree/ブランチを掃除（dirty はスキップ、-- --force で強制）
```

## Windows のみ実行可能（`windows` クレートや Win32 API・実行バイナリに依存）

```bash
npm ci                           # 依存インストール（初回セットアップ・CI）
cargo test -p snotra-core        # ユニットテスト（純ロジック層）
cargo test -p snotra             # ユニットテスト（Tauri 統合層: state/indexing/config_watcher 等）
cargo test -p snotra-settings    # ユニットテスト（設定 GUI の純ロジック: font face 検証・TOML エラーローカライズ）
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測（詳細: PERFORMANCE.md）
cargo check --workspace          # Rust 全 crate 型チェック
cargo clippy --workspace --all-targets -- -D warnings  # lint チェック（カテゴリ A と同じ）
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
- **ビルド済みバイナリの手動 smoke では「変更が含まれているか」を確認する**: `cargo` は `target/debug/deps/<crate>-<hash>.exe` を `target/debug/<crate>.exe` に hardlink し直すため、ソース変更後でも（fingerprint 上「最新」と判断され再リンクされず）`<crate>.exe` のタイムスタンプが更新されないことがある。タイムスタンプを信用せず、変更固有の文字列でバイナリを grep して目的の変更が入っているか確認する（例: `[Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe)).Contains('<変更固有の文字列>')`）。#343 の balloon smoke で実際に踏んだ
- **再レンダリングが保留中の文脈で属性をアサートするなら `driver.wait` 内で re-find し、さらに `StaleElementReferenceError` を catch して `return false` する**: `.result-row` を `findElements` で掴んでから `getAttribute` を読むと、検索デバウンス（leading + trailing 50ms）の trailing リフレッシュが結果リストを再レンダリングした瞬間に掴んだハンドルが `StaleElementReferenceError` で flake する（#382）。注意点は **`driver.wait` は throw をリトライしない**こと（`webdriver.js` の poll は `evaluateCondition().then(onFulfilled, reject)` で、コールバックが reject すると即失敗。再ポーリングは falsy `return` のみ）。よって `driver.wait` 内で re-find する*だけ*では `find→getAttribute` 間の stale-throw を救えず、コールバック内で `catch (e) { if (e instanceof error.StaleElementReferenceError) return false; throw e; }` を併用して初めて再レンダリングを待てる。クエリ変更直後のように trailing リフレッシュが保留中の確認が対象。キー入力で行 DOM が作り直されない文脈（↓/↑ 選択移動は SolidJS が class のみ更新）や安定要素（`.search-input`）の掴み置きでは catch 不要

## CI/CD メモ

### 検証コマンド ↔ GitHub Actions workflow の対応

「変更後の検証チェックリスト」の必須コマンドが、どの workflow でどのトリガーで自動実行されるかの対応表。エージェントが「PR CI 緑」だけで全検証済みと誤認しないための SSOT。

| 検証コマンド | workflow | トリガー |
|---|---|---|
| `npm test`（Vitest） | `ci.yml`（frontend-check=ubuntu / rust-check=windows） | PR 自動（`skip-ci` ラベルで無効化可） |
| `npm run build` / `npm run typecheck` | `ci.yml`（frontend-check） | PR 自動 |
| `cargo check` | CI では直接実行せず、検査対象を包含する `cargo clippy --workspace --all-targets` に統合 | ローカル必須 / `Cargo.toml` 編集時は PostToolUse hook |
| `cargo test -p snotra-core` / `cargo test -p snotra` / `cargo test -p snotra-settings` / `cargo clippy` | `ci.yml`（rust-check） | PR 自動 |
| `npm run smoke:startup`（注） | `e2e.yml`（E2E & Smoke） | 対象 paths を含む PR（自動）/ 手動 dispatch |
| `npm run e2e:tauri` | `e2e.yml`（E2E & Smoke） | 対象 paths を含む PR（自動）/ 手動 dispatch |

（注）CI では `e2e:tauri:setup` が生成した release バイナリを共有するため、`npm run smoke:startup`（既定 ExePath = debug）ではなく `scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe` を直接実行する。検証する起動経路は同じ（release バイナリの起動 trace に `*:error` が無いこと）。これは E2E 用ビルドの起動健全性検証であり、配布バンドル（`tauri build`）の検証ではない。

- `npm test` は ubuntu（frontend-check）と windows（rust-check）の両方で走る（#509）。`.githooks` / `.claude/hooks` の selftest は実運用が Windows でのみ起きる安全網であり、hook 実行機構（Git-for-Windows の shebang 経由 sh 起動・パス/クォート境界）が本番と一致する OS で回帰検査する。ubuntu 側は実行ビット・POSIX sh 厳密性を相補的に担保する。CRLF 由来の fail-open は `.gitattributes` の `.githooks/** text eol=lf` で両 OS 回避済みで、かつ dash 側の故障モードなので windows 固有ではない。
- カテゴリ C（ウィンドウ生成・ホットキー・スラッシュコマンド）相当の変更や依存更新を含む PR は、対象 paths（`src-tauri/**`・`ui/**`・`e2e/**`・`**/Cargo.toml`・`Cargo.lock`・`package.json`・`package-lock.json` 等）に該当するため `E2E & Smoke` workflow が自動起動する。paths 外の変更で手動実行するには `workflow_dispatch`。
- この対応関係のドリフト（必須コマンドに対応 workflow が無い等）は `/health-check` の Check 10 で検出する。

### その他

- **`GITHUB_TOKEN` では他のワークフローをトリガーできない**: tag push や `workflow_dispatch` を `GITHUB_TOKEN` で発火させても、別ワークフローは起動しない（GitHub の仕様）。ワークフロー間の連鎖には `workflow_call`（呼び出し元から直接呼ぶ）を使う
