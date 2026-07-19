# research.md — issue #384 第三者 GitHub Actions を Node 24 ネイティブへ更新

## issue の要約

`.github/workflows/` に残る第三者 JS アクションのうち、依然 Node 20 ターゲットの 2 つを Node 24 ネイティブへ移行する。#383 の follow-up。GitHub の force-migrate で当面動作するが、Node 20 ランタイム撤去時の停止リスクと非推奨警告を解消する。

**本 issue のスコープ（ユーザー確認済み・2026-07-20）**:
1. `softprops/action-gh-release@v2` → `@v3`（node24）
2. `label-sync.yml` を `EndBug/label-sync@v2`（node20・node24 版未提供）から `crazy-max/ghaction-github-labeler@v6`（node24）へ**移行**（コメントの検討指示を実行に格上げ）

## 関連コード（実在確認済み・grep で 3 ファイルのみ）

| ファイル | 箇所 | 現状 |
|---|---|---|
| `.github/workflows/release.yml:121` | Upload to GitHub Release ステップ | `softprops/action-gh-release@v2`。`draft: true` / `prerelease` / `files`（`.zip` / `*-setup.exe` / `latest.json`）を指定 |
| `.github/workflows/create-release.yml:50` | Create draft release ステップ | `softprops/action-gh-release@v2`。`draft: true` / `generate_release_notes: true` / `body` を指定 |
| `.github/workflows/label-sync.yml:22` | Sync labels ステップ | `EndBug/label-sync@v2`。`config-file: .github/labels.yml` / `delete-other-labels: true` / `env: GITHUB_TOKEN` |
| `.github/labels.yml` | ラベル SSOT | 10 ラベル定義。color は **`#` なし**（例 `"a2eeef"`）。dependabot 3 ラベル（dependencies/javascript/rust）を保護のため明示 |

grep 結果: `EndBug|action-gh-release|label-sync|ghaction-github-labeler` は上記 workflow 3 ファイルのみにヒット。docs・スキル・rules に言及なし → 移行に伴う文書追随は不要。

## 一次ソースで確認した事実

### softprops/action-gh-release v2 → v3（action.yml @v3 で実測）

- `runs.using: node24`（v2 は node20。Node 20 最終ラインは `v2.6.2`）。
- **入力パラメータは不変**。当リポジトリが使う `tag_name` / `name` / `draft` / `prerelease` / `files` / `generate_release_notes` / `body` はすべて `@v3` に存置。削除・改名なし（v3 で追加された入力はあるが使用しない）。
- v2→v3 の本質は**ランタイム bump のみ**でアップロード挙動の破壊的変更なし → **ドロップイン置換**（`@v2` → `@v3`）。
- リポジトリ慣習は major タグ運用（`checkout@v7` / `setup-node@v6` / 既存 `action-gh-release@v2`）→ `@v3` を採用（SHA ピンは既存方針に無いため踏襲しない）。

### crazy-max/ghaction-github-labeler v6.0.0（action.yml / README / 自 repo labels.yml で実測）

- `runs.using: node24`。最新 v6.0.0。
- 入力（EndBug との対応表）:

  | 目的 | EndBug/label-sync@v2 | crazy-max/ghaction-github-labeler@v6 |
  |---|---|---|
  | 設定ファイル | `config-file:` | `yaml-file:`（default `.github/labels.yml`） |
  | SSOT 外ラベルの削除 | `delete-other-labels: true`（opt-**in**） | **default 削除**。抑止は `skip-delete: true`（default `false`）。→ **現行の削除挙動を保つには何も指定しない** |
  | トークン | `env: GITHUB_TOKEN` | `github-token:`（default `${{ github.token }}`）→ 省略可 |
  | 除外 | （なし） | `exclude:`（newline 区切り・任意）。今回は不要 |
  | dry-run | （なし） | `dry-run:`（default `false`）。検証時のみ使用 |

- **color 形式は `#` なしで動作する**（crux）: README 例は `#` 付きだが、crazy-max **自身の** `.github/labels.yml` は `#` なし（`"69cde9"` 等）を使用し、PR #207 で「color 欄をサニタイズして hex code を許容」する処理が入っている＝`#` 付き・なしの双方を受容。→ **現行 `labels.yml` の 10 色はそのまま流用でき、書き換え不要**。
- 挙動対応: EndBug（delete-other-labels:true）と crazy-max（skip-delete 省略）はいずれも「SSOT に無いラベルを削除」＝**同一の削除挙動**。labels.yml の内容は不変ゆえ、同期対象のラベル集合も不変。

## 技術的制約

- **workflow YAML は PostToolUse hook の自動検査対象外**（ルート CLAUDE.md フック節: `.github/workflows/` は「沈黙 = 何も走らなかった」の分類）。編集後の即時検証が無いため、YAML パース・actionlint 相当を手動で行う。決定的整合は PR CI の `governance-check` job が捕捉。
- **critical path**: `release.yml` の Upload は署名済み成果物の draft アップロード。壊してはならない 3 点は (a) `draft: true` 維持、(b) `latest.json` 生成、(c) `files` glob（`.zip` / `*-setup.exe` / `latest.json`）で全成果物添付。**`.sig` 経路の正確な仕組み**: `.sig` ファイル自体は release asset として添付されない。L95 で `.sig` の**内容**を `Get-Content` で読み取り → latest.json の `signature` フィールドへ埋め込み（L103）→ latest.json を `files:` 経由でアップロード（L134）。この生成ステップ（L91-108）もファイル一覧（L131-134）も v2→v3 では変わらない（v3 は入力不変ゆえ構造的リスクは低いが、critical path のため明示検証する）。
- **label-sync の削除挙動**: 誤設定時に既存ラベルを消しうる。ただし labels.yml 内容不変・削除挙動同一ゆえ、移行による差分は「実行ランタイムとアクション実装」のみ。検証は dry-run または dispatch 観測で担保。
- Win32 / IPC / リアクティブ制約: 本 issue は CI 保守のみで無関係。

## 未解決の疑問

- なし（一次ソースで移行に必要な差分はすべて確定）。残る不確実性は「crazy-max v6 の実行が当リポジトリの labels.yml で意図どおり no-op 同期になるか」で、これは検証フェーズ（dry-run dispatch）で接地確認する。
