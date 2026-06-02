# research — issue #356: PR CI と smoke/E2E 検証要件の整合

## issue の要約

手元の検証要件（`docs/build-commands.md` カテゴリ C）と GitHub Actions の自動実行範囲にズレがある。

- `docs/build-commands.md` は、ウィンドウ生成・表示順・ホットキー・スラッシュコマンドに触れた場合、`npm test` / `npm run smoke:startup` / `npm run e2e:tauri` を**必須**としている。
- しかし `.github/workflows/e2e.yml` は `workflow_dispatch` のみで、通常 PR では smoke/E2E が自動実行されない。
- `.claude/skills/deps-update/SKILL.md` は「E2E・スモークは CI に委ねる」としているが、その CI は通常 PR から起動しない（**文書と実態の矛盾**）。

**リスク**: エージェントは「PR の CI が緑」を完了条件にしがち。Tauri 起動・ホットキー・ウィンドウライフサイクル・スラッシュコマンド系の回帰が PR で素通りする。

**ユーザー決定（要求の曖昧さ解消）**:
1. 整合の方向 = **CI を自動化する**（対象ラベル付き PR で E2E workflow を自動実行）
2. health-check に「build-commands.md ↔ .github/workflows/\*」対応チェックを**追加する**

## 関連コード／ファイル

| ファイル | 現状 | 役割 |
|---|---|---|
| `.github/workflows/e2e.yml` | `workflow_dispatch` のみ。`e2e:tauri:setup` → `e2e:tauri` を実行 | E2E 実行ワークフロー |
| `.github/workflows/ci.yml` | `push`/`pull_request`(main)。frontend test+build / cargo check+test+clippy。`skip-ci` ラベルで無効化可 | 通常 PR CI |
| `.github/labels.yml` | `type:*` / `size:*` のみ定義。`skip-ci` は未定義だが ci.yml で使用 | label-sync 対象 |
| `.github/workflows/label-sync.yml` | `labels.yml` push 時に同期。`delete-other-labels: false` | ラベル同期 |
| `docs/build-commands.md` | カテゴリ A〜D。C が smoke/E2E 必須。「CI/CD メモ」あり | 検証コマンド SSOT |
| `.claude/skills/deps-update/SKILL.md` | Step 3 で「E2E・スモークは CI に委ねる」 | 依存更新スキル |
| `.claude/skills/health-check/SKILL.md` | Check 1〜9。workflow 対応チェックは無い | 衛生チェック |
| `scripts/smoke-startup.ps1` | 既定 `-ExePath C:\workspace\Snotra\target\debug\snotra.exe`。`SNOTRA_TRACE=1` で起動し `*:error` トレース不在を検証 | 起動スモーク |

## npm scripts（package.json）

- `test` = `vitest run`（CI で実行済み）
- `smoke:startup` = `pwsh -NoProfile -File scripts/smoke-startup.ps1`（既定 ExePath = **debug**）
- `e2e:tauri:setup` = `cargo install tauri-driver --locked && npm run prepare:sidecar && npx tauri build --no-bundle`（**release** バイナリ生成）
- `e2e:tauri` = `playwright test -c playwright.tauri.config.ts`

## 既存パターン（再利用）

- **ラベルゲート**: `ci.yml` の `if: github.event_name == 'push' || !contains(join(github.event.pull_request.labels.*.name, ','), 'skip-ci')`。同形を E2E に適用（ただし配列 `contains` で誤マッチ回避）。
- **ターゲットディレクトリ**: ワークスペース target は**リポジトリ root**（smoke 既定 ExePath が `target/debug/snotra.exe` であることから確定）。release は `target/release/snotra.exe`。
- **CI 環境**: e2e.yml は `windows-latest` + Rust toolchain + Swatinem cache + `npm ci`。同 job 内に smoke step を追加すれば release バイナリを共有できる。

## 技術的制約

- **smoke と e2e のビルド成果物が別**: smoke 既定は debug、e2e setup は release（tauri build）。CI で両方走らせるなら release バイナリを共有し、smoke は `-ExePath target/release/snotra.exe` を明示する（debug を別途ビルドすると二重ビルドで無駄）。
- **release バイナリの妥当性**: `cargo build --release` 単体は localhost 向きで `ERR_CONNECTION_REFUSED`（build-commands メモ）。だが `npx tauri build --no-bundle`（e2e setup が使用）はフロントを埋め込むため正常起動する。smoke はこの release バイナリで動作する。
- **GitHub Actions の `contains`**: 配列に対する `contains(github.event.pull_request.labels.*.name, 'e2e')` は要素完全一致。`workflow_dispatch` 時は `pull_request` が null → 配列空 → false。`github.event_name == 'workflow_dispatch'` を OR の先頭に置けば短絡で true になる。
- **`GITHUB_TOKEN` でのワークフロー連鎖不可**（build-commands メモ）— 今回は単一 workflow 内に step 追加なので影響なし。
- **E2E は ~30 分（timeout-minutes: 30）**: ラベル付き PR でのみ走るのでコスト制御済み。

## 未解決の疑問

- smoke step を release バイナリで CI 実行したとき、ホットキー登録など CI 環境固有の `*:error` が出ないか。→ E2E が同 release バイナリを起動して通っている実績があるため、起動経路は clean と判断（実行で確認）。
