# plan.md — issue #383 GitHub Actions Node.js 20 非推奨対応

## 0. バグか仕様変更か

純粋な CI 保守（インフラ設定変更）。アプリ挙動・SPEC.md 記載のフロー/IPC 契約/状態遷移には一切影響しない。
→ **SPEC.md 更新は不要**。`docs/` も該当なし（ビルドコマンド・アーキテクチャ・開発原則のいずれにも非該当）。

## 1. 受け入れ条件（issue より）

- 5 workflow の `actions/checkout` / `actions/setup-node` が更新され、Node 20 非推奨警告が消える。
- `ci.yml`（PR 自動）がグリーン、`e2e` ラベルで `e2e.yml` がグリーン。
  `release.yml` / `create-release.yml` は dispatch 時確認 or 目視レビュー。

## 2. 変更ファイル一覧

更新方針: checkout `@v4`→`@v7`、setup-node `@v4`→`@v6`、FORCE env 全削除（ユーザー確定）。

| ファイル | 変更内容 |
|---|---|
| `ci.yml` | checkout `@v4`→`@v7`（L27, L55）, setup-node `@v4`→`@v6`（L30）, `env:` ブロック削除（L15-16, FORCE のみ） |
| `e2e.yml` | checkout `@v4`→`@v7`（L32）, setup-node `@v4`→`@v6`（L35）, `env:` ブロック削除（L16-17, FORCE のみ） |
| `release.yml` | checkout `@v4`→`@v7`（L24）, setup-node `@v4`→`@v6`（L29）, `env:` の FORCE 行のみ削除（L16）。**`TAG_NAME` 行（L15）は残す** |
| `create-release.yml` | checkout `@v4`→`@v7`（L22）, `env:` ブロック削除（L14-15, FORCE のみ） |
| `label-sync.yml` | checkout `@v4`→`@v7`（L22）, `env:` ブロック削除（L14-15, FORCE のみ） |

**注意**: env ブロックの構造が release.yml だけ異なる。
- ci.yml / e2e.yml / create-release.yml / label-sync.yml: `env:` 直下が FORCE のみ → `env:` ブロックごと削除。
- release.yml: `env:` 直下が `TAG_NAME` と FORCE の 2 行 → FORCE 行のみ削除し `env: / TAG_NAME:` を残す。

setup-node の `with:`（`node-version: 22` / `cache: npm`）は**変更しない**（v6 でも有効・明示指定で挙動安定）。

## 3. 実装順序

依存関係なし（各ファイル独立）。1 フェーズで 5 ファイルを編集。
1. checkout を全 5 箇所で `@v7` へ
2. setup-node を全 3 箇所で `@v6` へ
3. FORCE env を全 5 ファイルから削除（release.yml のみ TAG_NAME を残す部分削除）

## 4. 不変条件

- **`release.yml` の `TAG_NAME` env を消さない**: FORCE と同じ env ブロック内にあるため、誤って
  ブロックごと削除するとリリースビルドのタグ参照（`${{ env.TAG_NAME }}` を L26, L51-101 等で多用）が
  全て壊れる。FORCE 行のみのピンポイント削除を厳守する。
- **setup-node の `with` を維持**: `node-version: 22` を消すとランナー既定 Node（24）でのビルドになり、
  検証環境が変わる。`cache: npm` を消すと npm キャッシュが無効化されCI が遅くなる。両方維持。
- **トリガー種別を変えない**: checkout v7 は `pull_request_target`/`workflow_run` の fork PR を
  ブロックするが、当リポジトリは不使用。トリガー定義（`on:`）は一切触らない。
- **異常系**: アクションのメジャー更新は宣言的変更のみ。状態フラグ・プロセス・リソースの新規導入なし。
  失敗時はCI が赤くなるだけで回復不能状態は発生しない。ロールバックは git revert で完結。

## 5. テスト方針

- ユニットテスト追加・更新は**なし**（CI 設定変更でテスト対象コードなし。TDD のRed/Green は非該当）。
- **検証コマンド**: ローカルで実行可能な検証は YAML 構文確認程度。実効検証は CI 上で行う:
  - `ci.yml`: PR 作成で自動実行 → frontend-check / rust-check グリーン確認。
  - `e2e.yml`: PR に `e2e` ラベル付与 → e2e ジョブグリーン確認。
  - `release.yml` / `create-release.yml`: PR CI では走らない → **目視レビュー**（差分が version 文字列のみ・
    TAG_NAME 保持を確認）。dispatch 検証はリリース時に実施。
  - `label-sync.yml`: push(main, paths: labels.yml) / dispatch 起動 → PR CI では走らない → 目視レビュー。
- **警告消失の確認**: PR の ci.yml run ログで Node 20 deprecation 警告が出ないことを確認
  （checkout/setup-node 起因の警告が消える）。

## 6. SPEC.md 更新要否

**不要**。挙動変更なし・CI インフラのみ。docs/ 更新も不要。

## 7. AGENTS.md ワークフロー ステップ 2 チェック

- **対称コードパス**: checkout は ci.yml に 2 箇所（frontend-check / rust-check）。両方更新する（L27, L55）。
  setup-node は ci.yml L30 / e2e.yml L35 / release.yml L29 の 3 箇所すべて更新。
- **関数使用箇所検索 / 同一パターン全コードパス検索**: `actions/checkout@v4` / `actions/setup-node@v4` /
  `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` を `.github/workflows/` 全体で grep し、列挙した箇所が
  漏れなく対象に入っていることを実装時に再確認する。
- **キー/識別子形式の変更**: アクションの参照タグ更新のみ。新規記録/移行/外部参照の3者問題は非該当。
- **モジュール構成ドキュメント同期**: 新規/削除ファイルなし → CLAUDE.md モジュール構成更新不要。
- **E2E テスト影響**: e2e.yml の checkout/setup-node 更新は基本利用のため E2E シナリオの前提を壊さない。
  `e2e/` ディレクトリのテストコードは非変更。
- **件数・上限パラメータ / 設定後方互換**: 非該当（CI アクションのバージョンのみ）。

## 8. セルフレビュー

1. **対称コードパス**: ci.yml の checkout 2 箇所を両方更新（§7 で確認）。setup-node 3 箇所すべて対象。✅
2. **影響範囲の網羅性**: GitHub API で全 6 アクションの Node ターゲットを一次確認。更新対象は
   checkout/setup-node のみ。第三者アクション（rust-cache=node24済、rust-toolchain=composite）は据置。
   node20 残（action-gh-release@v2・label-sync@v2）は follow-up issue へ振り分け。✅
3. **境界条件**: release.yml の env ブロック構造差異（TAG_NAME 同居）を不変条件として明記。✅
4. **リソース管理**: 新規リソース・状態フラグ・プロセス導入なし。生成/破棄ペア非該当。✅
5. **既存パターンとの整合**: 「最新メジャーへピン」は既存（rust-cache@v2 等のメジャーピン）と整合。
   新規パターン導入なし。✅
6. **YAGNI 違反**: action-gh-release v3 化・label-sync 対応は本 issue に含めず follow-up へ。
   スコープを checkout/setup-node + FORCE 除去に限定。✅
7. **シンプル化の挑戦**: FORCE env は期限切れの暫定回避策で no-op 化済み。削除はむしろ簡素化（削る方向）。
   新たな状態・抽象の導入はゼロ。✅
8. **破壊不変条件の明示**: 「壊れたら即アウト」= release.yml の TAG_NAME 誤削除（リリースビルド破綻）。
   検知手段 = 目視レビュー + git diff で env ブロックの TAG_NAME 残存確認。CI 系（fork PR ブロック・
   トリガー）は不使用のため非該当。Win32 フック/ホットキー/IPC は本変更と無関係。✅

### plan-review / check スキル適用判断
- `/plan-review`: 実行済み（下記結果）。
- `/symmetric-check`: checkout の ci.yml 2 箇所（frontend-check / rust-check）= 対称ペア。plan-review で両方列挙済みを確認。2 箇所とも同一の version bump で完結するため独立 skill 起動は省略（KISS）。
- `/state-check` / `/race-check` / `/cache-check`: 非該当（UI モード・async・キャッシュ述語のいずれも触れない）。

## 9. plan-review 結果（Explore 2 並列）

### 問題なし
- checkout 6・setup-node 3・FORCE env 5 の全箇所が計画に列挙済み・行番号一致・計画外参照なし。
- env ブロック構造: release.yml のみ TAG_NAME 同居 → FORCE 行のみ削除で正確。他 4 ファイルは FORCE 単独でブロック削除安全。
- TAG_NAME は release.yml 内 9 箇所参照（L26 `ref:` 含む）→ ブロックごと削除は破綻、FORCE 行のみ削除が正しい。
- 全 workflow が `pull_request_target`/`workflow_run` 不使用 → checkout v7 fork PR ブロック無影響。
- setup-node 全 3 箇所で `node-version`/`cache` 明示 → auto-cache 仕様変更無関係。
- `.github/` 外にアクション version ハードコードなし → doc 更新不要。composite action なし。

### 軽微な懸念
- `softprops/action-gh-release@v2`・`EndBug/label-sync@v2` は node20 のまま。GitHub force-migrate で当面動作するが恒久保証ではない。本 issue スコープ外で妥当だが**別 issue 起票推奨**（priority low）→ §6 follow-up と整合。

### 要対処
- なし。

### 総評
completeness: **高** / 実装着手可否: **可**
