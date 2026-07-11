# research.md — issue #145 Phase 3（E2E を paths で自動実行）

## issue の要約

E2E テスト（`e2e/tauri.slash.e2e.ts` = 起動 smoke + スラッシュコマンド/検索/キーボード操作/config ホットリロード/index 再構築/instant コマンド検証）の CI 組み込みの最終フェーズ。

- **Phase 1（`workflow_dispatch` 手動）・Phase 2（`e2e` ラベルトリガー）は実装済み**（`.github/workflows/e2e.yml`）。issue コメント（owner, 2026-07-11）で確認。本文の想定ラベル名 `run-e2e` ではなく実ラベルは **`e2e`**。
- 残ギャップは **Phase 3 = カテゴリ C 相当の変更を含む PR で E2E を自動実行**。付け忘れ検知が現状「手動規範（docs カテゴリ C の `e2e` ラベル付与）」のみで機構が無い。

## 方式決定の来歴（本サイクルで A-2 を評価し A-1 に確定）

本セッションで 2 方式を提示し比較した:

- **A-1（採用・シンプル）**: `on.pull_request.paths` を直接足すだけ。detect ジョブなし。GitHub ネイティブの paths 判定をそのまま使う。label は縮退存置、手動オーバーライドは `workflow_dispatch`。
- **A-2（不採用）**: detect ジョブ（ubuntu / git diff）で category C パスを検出し、`e2e` label を真の手動オーバーライドとして併存させる。narrow パス。

**A-1 を選んだ理由**:
1. **A-2 の detect ジョブは GitHub がネイティブに持つ `paths` 判定を bash で再実装する** —— AGENTS.md「列挙の真実源はツール自身に問う」「glob の意味論はツールごとに違う（#471）」「写しを消す」に反する。A-1 は GitHub の paths マッチャ（真実源そのもの）を直接使い写しを作らない。
2. リポジトリの KISS/YAGNI 文化に整合（`if` を削り分岐を減らす）。
3. 前セッションで同方式を `/plan-review`（3 体）通過済み。

**Option B（`actions/labeler` で `e2e` ラベル自動付与）は不採用**: `GITHUB_TOKEN` で付けたラベルは下流 workflow をトリガーできない（GitHub 仕様。`docs/build-commands.md:126` に既記録）。labeler が label を付けても `e2e.yml` の `labeled` トリガーは発火しない → 原理的に機能しない。

## 現状の裏取り（本セッションで再測定・トレース）

### 現状の e2e.yml トリガー
- `on: workflow_dispatch` + `pull_request(branches:[main], types:[opened,synchronize,reopened,labeled])`。
- job `if: workflow_dispatch OR contains(labels,'e2e')`。
- L8-10 のコメントが「`labeled` を明示購読しないと後付けラベルで起動しない」ことを守っている（A-1 では paths が唯一ゲートになるため、このコメントは虚偽化 → 書き換え要）。

### スラッシュコマンドの backend マッピング（実コードで裏取り）
- `/o` → `commands/window.rs`（`open_settings`, `ERR_INDEXING_IN_PROGRESS`）
- `/r` → `commands/search.rs`（履歴 `get_history_results`）
- `/s` → `commands/window.rs`（`rebuild_index`）+ `commands/system.rs`（`notify_main_hidden`）
- `/q` → `commands/system.rs`（`quit_app`）
- → いずれも `src-tauri/src/**` 配下。A-1 の broad パスは全て包含する（narrow パスなら `system.rs` 取りこぼしに注意が要ったが、broad では不要）。

### ラベル機構
- `.github/labels.yml` に `e2e` ラベル定義（「カテゴリ C 変更時に付与」）。
- `.github/workflows/label-sync.yml` は `EndBug/label-sync`（labels.yml → リポジトリのラベル一覧を SSOT 同期、`delete-other-labels: true`）。PR 自動付与の `actions/labeler` **ではない**。
- OPEN PR = **0 件**（`gh pr list` 実測）→ labels.yml の description 変更が既存 PR のラベルへ波及するリスク無し。

### SPEC.md
- CI/workflow の記述は無い。CI 運用は `docs/build-commands.md` の「検証コマンド ↔ workflow 対応表」が SSOT。**→ SPEC.md 更新は不要**。

### health-check Check 10（本セッションで L129 を確認）
- `.claude/skills/health-check/SKILL.md` L116-129。「必須コマンドが PR で自動実行されるか、`e2e` ラベル等の条件付きかは**問わない**」（L129）。検知対象は「対応 workflow が存在しない」「対応表が実態とずれている」のみ。
- → 対応表を paths 自動実行の実態へ合わせれば緑。**Check 10 自体は変更不要**。

### 前案の誤りを訂正（派生コピーの陳腐化を裏取りが検出）
- 前 `plan.md` は `.github/pull_request_template.md:15`「E2E が必要な変更か確認した」を編集対象に挙げていたが、**現テンプレートは 7 行の最小構成（対応Issue / 変更内容）で、E2E 参照は存在しない**。
- → **`pull_request_template.md` は編集対象から除外**。前案が真実源で照合していなかった痕跡（「照合は真実源に対して行う」#500 の教訓が的中）。

## 技術的制約（設計の核心）

- **`on.pull_request.paths` は `labeled` を含む全 activity types に AND 適用される**（GitHub 公式挙動）。
  - 帰結: paths を足すと、**paths 非該当 PR に `e2e` ラベルを後付けしても workflow は起動しない**。既存の「手動ラベル昇格」が paths 該当 PR 限定に縮退する。
  - この縮退は「シンプル優先」で許容。paths 非該当 PR の随時実行は `workflow_dispatch` で代替（ブランチ/ref 指定で手動実行）。
- **paths のフェイル方向**: 拾いすぎ = 過剰実行（安全・コスト増）、拾い漏れ = E2E skip でリグレッション見逃し（危険）。→ paths は**広め**に取る。
- **job `if` から変更パスは見えない**（`github.event.pull_request` に changed files は無い）。ゆえに job 内 paths 判定には検出ジョブ（`dorny/paths-filter` 等）が要る = A-2 の複雑さの源。A-1 はトリガーレベル paths で回避。
- **GitHub Actions の paths glob 意味論**: `**` は `/` を含む 0 個以上の文字にマッチ。`src-tauri/src/**` は深さ 1（`main.rs`）も深さ 2 以上（`commands/window.rs`）も拾う。`src-tauri/tauri.conf.json` は深さ 1 の完全一致で購読される。git pathspec の `**/`（1 段以上要求）とは異なる（AGENTS.md「glob の意味論はツールごとに違う」）。
  - **実測の接地点**: この PR 自身が `.github/workflows/e2e.yml` を変更する = paths 該当。push で E2E が自動起動しなければ即座に露見する（ドッグフーディングが glob 挙動の実測を兼ねる）。

## 影響範囲（plan-review 第2ラウンドで確定・全 7 ファイル）

| ファイル | 変更 |
|---|---|
| `.github/workflows/e2e.yml` | `on.pull_request.paths` 追加（broad + lockfile）、`types` 削除（既定へ・labeled 廃止）、job `if` 削除、コメント全面更新 |
| `.github/labels.yml` | `e2e` ラベル定義を**削除**（Option 1 でラベル廃止。label-sync が GitHub 側も削除。OPEN PR=0 で無害） |
| `docs/build-commands.md` | カテゴリ C（L46）・対応表トリガー列（L115-116）・CI/CD メモ（L121）を paths 自動実行へ。`e2e` ラベル言及除去 |
| `.claude/skills/deps-update/SKILL.md`（★設定・合意済） | L43/L64/L70 のラベル起動を削除（依存 PR は lockfile が paths 該当で E2E 自動起動） |
| `.claude/skills/retrospective/SKILL.md`（★設定・軽微） | L66 の PR ライフサイクル例から `e2e` ラベル付与を除去 |
| `.claude/skills/health-check/SKILL.md`（★設定・軽微） | Check 10 例示文言を実態へ（ロジック不変・ゲートではない精度更新） |
| `.github/workflows/release.yml` | L80 の「e2e.yml (L53-54)」行番号参照をステップ名参照へ修正（腐り防止） |
| `SPEC.md` / `.github/pull_request_template.md` / ルート `CLAUDE.md` / `AGENTS.md` / `.claude/rules/**` / README / CONTRIBUTING | **更新不要**（現物照合で E2E トリガー機構への言及なし） |
| `e2e/`・`src-tauri/`・`ui/` 実装 | **触らない**（トリガーのみの変更、テスト内容・実装は不変） |

## paths glob 設計（Option 1: カテゴリ C + 依存を安全側に包含）

- `src-tauri/**` — main.rs / platform/*（hotkey/tray/wndproc）/ commands/*.rs / `capabilities/main.json`（E2E の `core:window:allow-show` 依存）/ `tauri.conf.json`（ウィンドウ・CSP）/ Cargo.toml = カテゴリ C 中核を全包含（R-2 の `capabilities` 取りこぼしを解消）
- `ui/**` — `ui/main.html`（vite エントリ・`ui/src/**` の外）+ `ui/src/**`（commands.ts・検索 UI・キーボード操作）。E2E が広く exercise（R-2 の `main.html` 取りこぼしを解消）
- `e2e/**` — E2E テスト自体
- `scripts/smoke-startup.ps1` — smoke スクリプト
- `.github/workflows/e2e.yml` — workflow 自身（変更で自己検証 = ドッグフーディング）
- `**/Cargo.toml` / `Cargo.lock` / `package.json` / `package-lock.json` — 依存 manifest/lockfile。**R-1 解決の核**: 依存更新 PR（lockfile のみ変更）を E2E 網に載せる。`e2e` ラベル起動の代替

**over-trigger の受容**: `src-tauri/**`・`ui/**` 配下の `*.md`（`src-tauri/CLAUDE.md` 等）変更でも E2E が起動する。拾いすぎ＝安全側ゆえ negation（`!**/*.md`）は足さない（KISS）。

**受容する残余（明記）**: `snotra-core/**` は除外する。現 E2E は検索/index アサーションも持つため core の検索・index ロジック変更が E2E を壊しうるが、(1) core は `cargo test -p snotra-core` の広範なユニットテストで大半のリグレッションを捕捉、(2) core 全変更で E2E を回すとコスト過大、(3) 独立導出も除外結論。core 変更で E2E が要るときは `workflow_dispatch` で救済。ただし core クレートの**バージョン更新**（`snotra-core/Cargo.toml`）は `**/Cargo.toml` で拾う。

## 未解決の疑問
- なし（load-bearing な paths glob 挙動はドッグフーディング push で実測される）。
