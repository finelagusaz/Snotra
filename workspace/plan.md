# plan.md — issue #145 Phase 3（E2E を paths で自動実行）

## ゴール

カテゴリ C 相当の変更（`src-tauri`・`ui`・`e2e`・依存 manifest/lockfile 等）を含む PR で、`E2E & Smoke` workflow を **paths フィルタにより自動起動**する。付け忘れの人的規範を機構化し、`e2e` ラベルへの依存を廃する。

## 方式（A-1 = シンプル + Option 1 = lockfile も paths。本サイクルで 2 度の user 合意）

- **A-1**: `on.pull_request.paths` を唯一の自動ゲートにする。detect ジョブ・外部アクションは足さない（GitHub ネイティブの paths 判定を真実源として直接使う）。
- **Option 1**（plan-review R-1 の解決・user 合意）: manifests/lockfiles も paths に含め、依存更新 PR も E2E 自動起動する。これにより `e2e` ラベル機構は不要になり**廃止**。手動実行は `workflow_dispatch` に集約。
- 帰結: **paths が唯一のゲート**。`if:` 削除、`types` から `labeled` 削除、`e2e` ラベル廃止、`deps-update` スキルのラベル起動ステップ削除。

## 変更ファイルと内容（全 7 ファイル）

### 1. `.github/workflows/e2e.yml`（機能中核）

**on:**
```yaml
on:
  workflow_dispatch:
  pull_request:
    branches:
      - main
    paths:
      - 'src-tauri/**'          # main.rs / platform/* / commands/*.rs / capabilities/main.json / tauri.conf.json / Cargo.toml。カテゴリ C 実装を全包含（capabilities は E2E の core:window:allow-show 依存を拾う）
      - 'ui/**'                 # ui/main.html（vite エントリ）+ ui/src/**（commands.ts・検索 UI・キーボード操作）。E2E が広く exercise
      - 'e2e/**'                # E2E テスト本体
      - 'scripts/smoke-startup.ps1'
      - '.github/workflows/e2e.yml'   # workflow 自身（変更で自己検証＝ドッグフーディング）
      - '**/Cargo.toml'         # 依存 manifest（全 crate）
      - 'Cargo.lock'            # 依存 lockfile（ルート集約・src-tauri/Cargo.lock は無い）
      - 'package.json'
      - 'package-lock.json'
    # types 明示指定を削除 → 既定（opened, synchronize, reopened）。labeled は不要（ラベル廃止）
```
- **`types:` 行を削除**（既定へ）。理由: `e2e` ラベル廃止で `labeled` 購読が不要。M-1（任意ラベル追加での 30 分無駄再実行）も同時に解消。

**job `if`:**
- 変更前: `github.event_name == 'workflow_dispatch' || contains(...labels..., 'e2e')`
- 変更後: **削除**。`on` は `workflow_dispatch` と（paths + branches で絞った）`pull_request` のみ。起動した時点で実行してよいことが確定するため冗長。
  - ケース検証: `workflow_dispatch` → 常に実行 ✓ / paths 該当 PR（opened/synchronize/reopened）→ 実行 ✓ / paths 非該当 PR → `on` レベルで非起動 ✓

**コメント:** 現状 L8-10（labeled 購読理由）・L18-23（if 説明）を全面書き換え。明記: 「paths が唯一の自動ゲート。手動は workflow_dispatch。`e2e` ラベルは廃止済み。`if` 削除の前提＝`on` が 2 トリガーのみ、将来 `on` にトリガーを足すなら `if` 再導入を検討」。

**over-trigger の受容（明記）**: `src-tauri/**`・`ui/**` は配下の `*.md`（`src-tauri/CLAUDE.md` 等）や `Cargo.toml` 変更でも E2E を起動する。拾いすぎ＝安全側ゆえ negation（`!**/*.md`）は足さない（KISS）。

### 2. `.github/labels.yml`（`e2e` ラベル廃止）

- `e2e` ラベル定義（L19-21）を**削除**。`EndBug/label-sync`（`delete-other-labels: true`・SSOT）が次回同期で GitHub 上のラベルも削除する。OPEN PR = 0（実測）ゆえ実 PR への波及なし（過去クローズ PR のラベル表示が消えるのみ＝無害）。
- 他ラベル（type/size/skip-ci/dependencies/javascript/rust）は不変。

### 3. `docs/build-commands.md`（SSOT・health-check Check 10 監視対象）

- **カテゴリ C（L46）**: 「`e2e` ラベルを付与すること」→「`src-tauri`・`ui`・`e2e`・依存 manifest/lockfile 等を含む変更は `E2E & Smoke` workflow が **paths により自動起動**する。paths 外の変更で E2E を回したいときは `workflow_dispatch`」。`e2e` ラベル言及を除去。
- **対応表（L115-116）トリガー列**: 「`e2e` ラベル付き PR / 手動 dispatch」→「対象 paths を含む PR（自動）/ 手動 dispatch」。
- **CI/CD メモ（L121）**: 「`e2e` ラベルを付与し」→ paths 自動起動へ書き換え。
- 書式（列構成）は維持しトリガー列の中身のみ変更（Check 10 の照合を壊さない）。注記 L118 はトリガー非依存ゆえ現状維持。

### 4. `.claude/skills/deps-update/SKILL.md`（★エージェント設定・user 合意済み）

- **L43**: 「E2E…は `e2e` ラベルを付与して…に委ねる。ラベル付与を忘れると検証されない」→「依存更新は `Cargo.lock`/`package-lock.json`（+ manifest）を変えるため、`e2e.yml` が paths で **E2E/smoke を自動起動**する（忘れの余地なし）」。
- **L64**: `gh pr edit --add-label e2e` の**ステップを削除**（自動化されるため不要）。
- **L70**: 「`e2e` ラベルを付与した場合は…完了も待つ」を削除/簡素化。E2E は通常の PR チェックとして `gh pr checks`（Step 5 L68）に自動で現れるため特別扱い不要。

### 5. `.claude/skills/retrospective/SKILL.md`（★エージェント設定・軽微）

- **L66**: PR ライフサイクル例の「push・`e2e` 等のラベル付与」から `e2e` ラベルを除去（ラベル廃止に伴う陳腐化解消）。

### 6. `.claude/skills/health-check/SKILL.md`（★エージェント設定・軽微・精度更新）

- Check 10 の合否ロジック（L123-126）は**不変**（L129 が「自動/条件付きを問わない」と明言）。例示文言（L121 の `smoke:startup`/`e2e:tauri`、L129 の「`e2e` ラベル等の条件付き」）が実態（paths 自動・ラベル廃止）とずれるため、例示を現行機構へ更新。**ロジック変更なし＝ゲートではない精度更新**。

### 7. `.github/workflows/release.yml`（行番号参照の修正）

- **L80** コメント「e2e.yml (L53-54) が…」の**行番号参照**を、e2e.yml 冒頭に paths を足すと腐るため、ステップ名（"Run startup smoke" ステップ）での参照に修正（AGENTS.md「順序に依存する参照は削除・挿入で静かに腐る」「番号ではなく名前で参照し直す」）。

### 変更不要と確定（grep 接地）
- **`.github/pull_request_template.md`**: 6 行の最小構成、E2E 参照なし → 変更不要。
- **ルート `CLAUDE.md` / `AGENTS.md` / `.claude/rules/**` / `src-tauri/CLAUDE.md` 等**: `e2e` の実質ヒットは tsconfig include・ディレクトリ列挙のみ、トリガー機構への言及なし → 対象外。
- **`SPEC.md`**: CI 運用は SPEC 対象外 → 更新不要。
- **`CONTRIBUTING.md` / README**: build コマンドは build-commands.md へ委譲、トリガー規則を再掲せず → 変更不要。

## 実装順序

1. `.github/workflows/e2e.yml`（paths 追加 + `types`/`if` 削除 + コメント全面書き換え）
2. `.github/labels.yml`（`e2e` ラベル定義削除）
3. `docs/build-commands.md`（L46 / L115-116 / L121）
4. `.claude/skills/deps-update/SKILL.md`（L43 / L64 / L70）
5. `.claude/skills/retrospective/SKILL.md`（L66）
6. `.claude/skills/health-check/SKILL.md`（例示更新）
7. `.github/workflows/release.yml`（L80 行番号→ステップ名参照）
8. YAML 妥当性 + トリガーロジックの机上検証
9. push → **この PR 自身が paths 該当（`e2e.yml` 変更）→ E2E 自動起動＝ドッグフーディング検証**

## 不変条件

- paths 該当 PR（コード・依存 manifest/lockfile）で E2E が起動する（Phase 3 中核）
- paths 非該当 PR（docs のみ・`.claude/` 設定のみ等）で E2E が起動しない（コスト浪費回避）
  - 注: `src-tauri/**`・`ui/**` 配下の `.md` は over-trigger（受容・安全側）
- `workflow_dispatch` は常に起動（唯一の手動フォールバック）
- E2E/smoke の steps は不変（トリガーのみ変更）
- concurrency `cancel-in-progress: true` 維持
- docs の「検証コマンド ↔ workflow 対応表」と実 workflow が整合（health-check Check 10）
- **依存更新 PR が E2E 網に載る**（R-1 解決の核）: lockfile 変更が paths に該当し自動起動。`deps-update` の false green（起動しない workflow を待つ）が解消。
- **push イベント非購読（意図的）**: main 直 push は ruleset 禁止（全変更 PR 経由）。paths 該当 PR は PR 時点で E2E 通過済み。`ci.yml` との非対称は明示判断。

### 破壊不変条件（壊れたら即アウト）
- **workflow YAML 破損 → `e2e` workflow が全停止**。検知: この PR 自身が paths 該当ゆえ push で自動起動、起動しなければ Actions タブで即露見（ドッグフーディングが検知を兼ねる）。
- **paths 拾い漏れ → リグレッション見逃し**。緩和: `src-tauri/**`・`ui/**` の広い包含 + lockfile。拾いすぎは過剰実行（安全側）。受容残余（`snotra-core/**` 除外）は research.md に明記。
- **`e2e` ラベル削除の波及**: label-sync が GitHub 上のラベルを削除。OPEN PR=0 で実害なし。`contains(labels,'e2e')` 参照は e2e.yml 自身のみ（同時削除）。deps-update の唯一の能動的ラベル付与も同時削除。

## テスト方針

- 変更は YAML 2 + md 1 + skill md 3 + workflow md 1。カテゴリ A-E（`.rs`/`.ts`）非該当、PostToolUse フック非発火（`selectChecks` 対象外＝「何も走らない」沈黙。合格を意味しない）。
- **YAML 構文**: `node`/`python` の YAML パーサで e2e.yml・labels.yml・release.yml の妥当性を確認。
- **トリガーロジック**: 3 ケース（dispatch / paths 該当 / paths 非該当）を机上検証。paths glob（`src-tauri/**` が深さ 1/2+、`**/Cargo.toml` が全 crate、lockfile 完全一致）を GitHub 仕様に照合。
- **実機（ドッグフーディング）**: PR push で新 `e2e.yml` が paths トリガーされ E2E 自動起動を Actions で確認＝Phase 3 の受け入れ条件そのもの。
- E2E テスト内容は不変ゆえローカル `npm run e2e:tauri` 再実行は必須でない（この PR の CI で担保）。

## SPEC.md 更新
不要（CI 運用は SPEC 対象外、`docs/build-commands.md` が SSOT）。

## plan-review 記録

### 第1ラウンド（前セッション・3 体）
- paths 集合の中核・`tauri.conf.json` 包含・SPEC 更新不要・paths glob 構文: 計画と独立導出が一致。
- push 非購読の非対称を要対処 → 不変条件に明記。

### 第2ラウンド（本セッション・Explore×2 + Plan×1）— 前案の完全性主張を反証
- **R-1（機能破壊）**: `deps-update/SKILL.md` の `e2e` ラベル起動が A-1 で空振り＝false green（3 体独立検出・現物裏取り）→ **Option 1 で解決**（user 合意）。
- **R-2（取りこぼし）**: `ui/main.html`・`capabilities/main.json` が narrow paths の外（現物確認）→ **`src-tauri/**`・`ui/**` の広い包含で解消**。
- **R-3（腐る参照）**: `release.yml:80` の行番号参照 → ステップ名参照へ修正。
- **M-1**: `labeled` 維持での無駄再実行 → `types` から `labeled` 削除で解消。
- **M-2**: `retrospective/SKILL.md:66` の陳腐化 → 例示更新。
- 独立導出との一致（完全性の能動的証拠）: paths glob 挙動・Option B 却下（`GITHUB_TOKEN` 制約）・SPEC 更新不要・`if` 削除の健全性 = 全て再一致。

## セルフレビュー（Step 5b チェックリスト）

1. **対称コードパス**: トリガーの対称は「起動する/しない」。`ci.yml`（push+PR・paths なし）との非対称は push 非購読として明示記録。ラベル廃止に伴う対称ペア（labels.yml 定義 ↔ e2e.yml `contains` ↔ deps-update 付与）を 3 点同時に除去（片方残しの腐りを回避）。
2. **影響範囲の網羅性**: `e2e`/ラベル/カテゴリ C を全走査 + 3 体独立検証。同期対象を docs 3 + skill 3 + workflow 1 に確定（第1ラウンドの「docs 3 のみ」は反証済み）。pull_request_template / CLAUDE.md / AGENTS.md / SPEC.md は現物照合で対象外確定。
3. **境界条件**: dispatch / paths 該当 / paths 非該当 / 依存 PR の 4 ケース机上検証。
4. **リソース管理**: 新規リソース（プロセス・リスナー・状態フラグ）生成なし。
5. **既存パターン整合**: `skip-ci` ラベル（ci.yml）と同系の CI 制御。`dorny/paths-filter` 等の外部アクション追加は回避（真実源はツール自身）。ラベル廃止で「写し」を 3 箇所同時に消す（真実源への接地）。
6. **YAGNI**: paths 一本化 + ラベル廃止で機構を減らす。detect ジョブ（A-2）・labeler（案 B）・paths negation はいずれも不採用。
7. **シンプル化**: `if` + `types` + `e2e` ラベルの 3 機構を削除。新規状態なし。
8. **破壊不変条件**: 上記「破壊不変条件」節に YAML 破損（ドッグフーディングが検知）・paths 拾い漏れ（広い包含で緩和）・ラベル削除波及（実害なし裏取り）を明記。

## 総評
- completeness: **高**（2 ラウンドの plan-review、R-1〜R-3 を検出し全解決、独立導出と主要判断が再一致、エージェント設定変更は user 合意済み）。
- 実装着手: **可**（`/implement` へ）。
