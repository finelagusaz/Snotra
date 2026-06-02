# plan — issue #356: PR CI と smoke/E2E 検証要件の整合

## ゴール / 受け入れ条件

- smoke/E2E を必須とする変更種別で、**PR 上の実行責任が明確**になっている。
- `.claude/skills/deps-update/SKILL.md` の記述が実際の CI トリガーと**矛盾しない**。
- エージェントが「**通常 CI 緑だけで smoke/E2E 済み**」と誤認しない。

**採用方針（ユーザー決定）**: 「CI を自動化する」。`e2e` ラベル付き PR で E2E workflow を自動実行。さらに同 workflow で **smoke:startup も実行**し、「`e2e` ラベル付き PR の CI 緑 = カテゴリ C（smoke + E2E）済み」を実態として真にする。health-check に対応チェックを追加。

## 変更ファイル一覧

### 1. `.github/workflows/e2e.yml`（トリガー + smoke step 追加）
- `on:` に `pull_request: { branches: [main] }` を追加（`workflow_dispatch` は維持）。
- job に label ゲートを追加:
  ```yaml
  if: >-
    github.event_name == 'workflow_dispatch' ||
    contains(github.event.pull_request.labels.*.name, 'e2e')
  ```
  （配列 `contains` で要素完全一致。`skip-ci` の join+部分一致より誤マッチに強い）
- `Build app and setup E2E`（`e2e:tauri:setup`）の後、`Run E2E tests` の**前**に smoke step を追加:
  ```yaml
  - name: Run startup smoke
    run: pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe
  ```
  （setup が生成した release バイナリを共有。debug 二重ビルドを避ける）
- workflow `name:` を `E2E` → `E2E & Smoke` に変更（実態を反映）。

### 2. `.github/labels.yml`（`e2e` + `skip-ci` ラベル定義追加）
- discoverability のため `e2e` ラベルを追加（label-sync が自動作成）:
  ```yaml
  - name: e2e
    color: "5319e7"
    description: この PR で E2E/smoke workflow を自動実行する（カテゴリ C 変更時に付与）
  ```
- 併せて、`ci.yml` が使用済みだが未定義の `skip-ci` も定義する（plan-review 指摘の整合修正）:
  ```yaml
  - name: skip-ci
    color: "ededed"
    description: 通常 PR CI（frontend-check / rust-check）をスキップする
  ```

### 3. `docs/build-commands.md`（カテゴリ C の責任明確化 + 対応表）
- カテゴリ C に「PR では `e2e` ラベルを付与すると `E2E & Smoke` workflow が smoke:startup + e2e:tauri を自動実行する。ローカル実行でも可」を明記。
- 「CI/CD メモ」に **build-commands ↔ workflow 対応表**を追加:
  - `npm test` → `ci.yml`（PR 自動）
  - `npm run smoke:startup` / `npm run e2e:tauri` → `e2e.yml`（`e2e` ラベル付き PR / workflow_dispatch）

### 4. `.claude/skills/deps-update/SKILL.md`（CI トリガーとの矛盾解消）
- Step 3 の「E2E・スモークテストはローカルで実行せず CI に委ねる」を、
  「E2E・スモークはローカル実行せず、PR に **`e2e` ラベルを付与**して `E2E & Smoke` workflow に委ねる（通常 PR CI では走らない）」に修正。
- Step 4（PR 作成）に「カテゴリ C 相当の変更を含む場合 `gh pr edit --add-label e2e` でラベル付与」を追記。
- Step 5（CI ポーリング）に「`e2e` ラベル付与時は `E2E & Smoke` の完了も待つ」を追記。

### 5. `.claude/skills/health-check/SKILL.md`（Check 10 追加）
- **Check 10 — build-commands.md ↔ .github/workflows/\* の対応**:
  - `docs/build-commands.md` の「必須」コマンド（特にカテゴリ C の `smoke:startup` / `e2e:tauri`）が、いずれかの workflow で実行されるか grep 検証。
  - 実行 workflow が無い必須コマンドを **Warning** で報告（CI で担保されない検証要件のドリフト検知）。
- frontmatter `description` の検証項目列挙に「workflow 対応」を追記。

### 6. スキル表 description の微更新（ルート `CLAUDE.md`）
- `/health-check` 行: 検証項目に「workflow 対応」を追記。
- `/deps-update` 行: 「カテゴリ C 相当時はラベル付与・E2E/smoke CI 完了確認」を追記。
- `.claude/skills/deps-update/SKILL.md` frontmatter `description` も同様に整合。

## 実装順序（依存考慮）

1. `.github/labels.yml`（`e2e` + `skip-ci` ラベル定義）
2. `.github/workflows/e2e.yml`（トリガー + smoke step + rename）
3. `docs/build-commands.md`（責任明確化 + 対応表）
4. `.claude/skills/deps-update/SKILL.md`（文言整合 + frontmatter description）
5. `.claude/skills/health-check/SKILL.md`（Check 10 + frontmatter description）
6. ルート `CLAUDE.md` スキル表（health-check / deps-update の description 微更新）

## 不変条件

- **workflow_dispatch は壊さない**: OR 条件の先頭に `github.event_name == 'workflow_dispatch'` を置き、dispatch 時は無条件 true（`pull_request` が null でも `contains(空配列, 'e2e')` = false に短絡されない）。
- **ラベル無し PR は E2E を起動しない**: コスト制御。`e2e` ラベルが付かない限り job はスキップ。
- **smoke の ExePath は release を指す**: setup が build する成果物。debug を別途ビルドしない（CI 時間の無駄回避）。smoke step は `e2e:tauri:setup` 成功後にのみ実行（バイナリ存在前提）。
- **失敗時の挙動**: smoke step が落ちれば後続 E2E は実行されず job が fail（GitHub Actions の既定 step 順序依存）。これは望ましい（起動エラーがあれば E2E まで進む意味がない）。
- **SSOT 非迂回**: build-commands.md にコマンド本体を集約。SKILL 側はコマンド名・ラベル操作のみ言及（health-check Check 5 の SSOT 迂回検知に抵触しない）。`gh pr edit --add-label e2e` は npm/cargo の検証コマンドではないため SSOT 対象外。

## テスト方針

- **YAML 妥当性**: `e2e.yml` / `labels.yml` を読み返し、インデント・`if` 式の構文を目視確認。可能なら `python -c "import yaml; yaml.safe_load(open(...))"` でパース確認。
- **ドキュメント整合**: health-check Check 5（SSOT 迂回）に新規違反を作らないこと、Check 9（スキル表整合）に影響しないことを確認。
- **CI 実行確認**: 本 PR 自体に `e2e` ラベルを付与し、`E2E & Smoke` workflow が起動 → smoke + e2e が緑になることを確認（受け入れ条件の実証）。ラベル無し時はスキップされることも確認。
- 変更は `.yml` / `.md` のみ。`.rs` / `.ts` 変更なしのため build-commands カテゴリ A/B のコンパイル検証は対象外。

## SPEC.md 更新要否

**不要**。CI トリガー・検証プロセス・ドキュメントの変更であり、`SPEC.md` が管理するアプリのユーザー可視挙動・状態機械・IPC 契約を一切変えない。

## セルフレビュー

**plan-review（Explore 2 並列）結果**: 計画は技術的に妥当・実装可能と判定。反映済みの指摘:
- `skip-ci` が `ci.yml` で使用済みだが `labels.yml` 未定義 → labels.yml に併せて定義（変更 2 に反映）。
- ExePath は CI working dir = repo root で `target/release/snotra.exe` が相対解決。`smoke-startup.ps1` が `Test-Path` で存在ガード済みのため追加 step 不要。
- `npx tauri build --no-bundle` の出力先は workspace root `target/release/`（`.cargo/config.toml` 無し、`release.yml` の出力パスと一致）で確定。
- `/health-check`・`/deps-update` の description 微更新を推奨 → 変更 6 に反映。

**セルフレビューチェックリスト**:
1. **対称コードパス**: CI トリガーの `workflow_dispatch`/`pull_request`、ラベルゲートの `skip-ci`(無効化)/`e2e`(有効化) は逆向きゲートだがコード対称ペアではない。symmetric-check は非該当（コードパス変更なし）。
2. **影響範囲の網羅性**: `.github/workflows/` 全 yml を grep。`E2E` workflow 名・job 名への外部参照なし（required status checks はコード非依存）。
3. **境界条件**: `workflow_dispatch` 時に `pull_request` が null → `contains(空配列,'e2e')`=false だが OR 先頭が true で短絡。ラベル無し PR はスキップ。検証済み。
4. **リソース管理**: 新規プロセス・フラグ・listener の導入なし。smoke step は既存 ps1 を呼ぶのみ（プロセスは ps1 内で起動/Stop-Process 済み）。
5. **既存パターンとの整合**: ラベルゲートは `ci.yml` の `skip-ci` 判定を踏襲（配列 contains でより堅牢に）。
6. **YAGNI 違反**: `skip-ci` ラベル定義は labels.yml を触る同一文脈での整合修正で許容範囲。それ以外に要求超過の追加なし。
7. **シンプル化**: 単一 workflow 内への step 追加のみで新規ステート・プロセス・抽象を導入しない。smoke は release バイナリ共有で二重ビルド回避。
8. **破壊不変条件**: 「壊れたら即アウト」= `workflow_dispatch` トリガーの維持（手動 E2E 実行の生命線）。OR 条件先頭で保護。**注意（手動）**: branch protection の required status checks に `E2E` 名が登録されている場合、rename で参照が外れる。e2e は従来 PR で走らず required になり得ないため低リスクだが、PR 後に GitHub UI で required checks を確認する。

**実装着手可否**: 可。

