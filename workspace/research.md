# research.md — issue #383 GitHub Actions Node.js 20 非推奨対応

## issue の要約

GitHub Actions ランナーが Node 20 を非推奨化（2026-06-02 強制移行）。`.github/workflows/` の
`actions/checkout@v4` / `actions/setup-node@v4`（いずれも node20 ターゲット）を Node 24 ネイティブの
最新メジャーへ更新し、Node 20 非推奨警告を解消する。純粋な CI 保守でアプリ挙動には影響しない。

## 関連コード（影響を受けるファイル）

`.github/workflows/` の 5 ファイル:

| ファイル | checkout | setup-node | その他 JS アクション |
|---|---|---|---|
| `ci.yml` | L27, L55 | L30 | `Swatinem/rust-cache@v2`, `dtolnay/rust-toolchain@stable` |
| `e2e.yml` | L32 | L35 | `Swatinem/rust-cache@v2`, `dtolnay/rust-toolchain@stable` |
| `release.yml` | L24 | L29 | `Swatinem/rust-cache@v2`, `dtolnay/rust-toolchain@stable`, `softprops/action-gh-release@v2` |
| `create-release.yml` | L22 | — | `softprops/action-gh-release@v2` |
| `label-sync.yml` | L22 | — | `EndBug/label-sync@v2` |

全 5 ファイルに `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: true` の env が存在する（後述）。

## 既存パターン / 経緯

- FORCE env は #247（commit `cc1de0d`, 2026-03-12）で **2026-06-02 強制移行への暫定 opt-in** として
  全 workflow に追加された。v4 アクションを node24 で動かすための一時回避策。
- 今回の #383 は「v4 → native node24 メジャーへ更新」という #247 の暫定対応を本対応へ置き換えるもの。

## 各アクションの最新メジャーと Node ターゲット（GitHub API で一次確認）

`runs.using` を各アクションの `action.yml` から直接確認した結果:

| アクション | 現状 | `using` | 最新メジャー | 判定 |
|---|---|---|---|---|
| `actions/checkout` | **@v4** (node20) | v7 → **node24** | **v7** | 更新対象 |
| `actions/setup-node` | **@v4** (node20) | v6 → **node24** | **v6** | 更新対象 |
| `Swatinem/rust-cache@v2` | v2 | **node24** | v2.9.1 | 対応済み・据置 |
| `dtolnay/rust-toolchain@stable` | stable | **composite**（Node無関係） | — | 据置 |
| `softprops/action-gh-release@v2` | v2 | **node20** | v3 → node24 | スコープ外（follow-up） |
| `EndBug/label-sync@v2` | v2 | **node20** | v2.3.3（node24版なし） | スコープ外（upstream 待ち） |

**観察の裏付け**: #381 E2E run の警告対象が `checkout@v4`・`setup-node@v4` のみだったのは、
同じ e2e.yml が使う `rust-cache@v2`（node24）・`rust-toolchain@stable`（composite）が
既に非 node20 だから。整合する。

## 破壊的変更（CHANGELOG 確認）

### actions/checkout v4 → v7（3 メジャー跨ぎ）
- **v5.0.0**: Node 24 化。最小ランナー要件 **v2.327.1+**（GitHub ホスト ubuntu/windows-latest は充足済み）。
- **v6.0.0**: 認証情報を別ファイルへ永続化（セキュリティ改善・基本利用には透過的）。
- **v7.0.0**: ESM 化（アクション内部実装。consumer 側に影響なし）。`pull_request_target` /
  `workflow_run` での fork PR チェックアウトをブロック（セキュリティ強化）。
- **当リポジトリへの影響**: 全 workflow のトリガーは `pull_request` / `workflow_dispatch` /
  `workflow_call` / `push` のみ。`pull_request_target`・`workflow_run` は**不使用**のため
  v7 の fork PR ブロックは無影響。checkout は全て基本利用（`ref:` 指定のみ）で破壊的変更なし。

### actions/setup-node v4 → v6（2 メジャー跨ぎ）
- **v5.0.0**: Node 24 化。`package.json` に `packageManager` フィールドがある場合に auto-cache。
- **v6.0.0**: auto-cache を npm のみに限定。
- **当リポジトリへの影響**: 全箇所で `node-version: 22` + `cache: npm` を**明示指定**。
  `package.json` に `packageManager` フィールドは**存在しない**（grep 確認済み）。
  auto-cache 仕様変更は明示指定では無関係。破壊的影響なし。

## 技術的制約

- Win32 / IPC / リアクティブ制約は無関係（CI 設定のみ・アプリコード非変更）。
- ランナーは全て GitHub ホスト（`ubuntu-latest` / `windows-latest`）で常に最新 → 最小ランナー要件は自動充足。
- `release.yml` / `create-release.yml` は dispatch/workflow_call 起動のため PR CI では検証されない（目視レビュー）。

## FORCE_JAVASCRIPT_ACTIONS_TO_NODE24 env の現状分析

- **由来**: #247 の暫定 opt-in（期限 2026-06-02）。
- **現状（2026-06-20）**: 期限を過ぎ、GitHub が node20→node24 を**既定で強制移行**するため、
  この env は実質 **no-op**。
- **checkout/setup-node を native へ更新後**: ci.yml・e2e.yml には node20 アクションが残らず
  env は完全に冗長。release.yml・create-release.yml・label-sync.yml には node20 第三者アクション
  （action-gh-release@v2・label-sync@v2）が残るが、それらも GitHub 既定の force-migrate で node24 実行
  となり env の有無で挙動は変わらない。
- **ユーザー判断（確定）**: 全 workflow から FORCE env を削除する（推奨案）。期限切れ暫定対応の除去で
  移行を完結させる。

## 未解決の疑問

なし。全アクションの Node ターゲット・破壊的変更・env 挙動を一次情報で確認済み。

## follow-up（本 issue スコープ外）

- `softprops/action-gh-release@v2`（node20）→ v3（node24）への更新。v3 は破壊的変更の確認が必要。
- `EndBug/label-sync@v2`（node20）は node24 リリースが未提供。upstream の対応待ち。
- 上記 2 件は GitHub 既定 force-migrate で当面動作するため緊急性なし。別 issue で追跡。
