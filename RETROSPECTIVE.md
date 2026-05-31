# Retrospective — #347 config↔派生状態コヒーレンシの一元化（Phase 1: history live-read / Phase 2: index_stale ledger）+ #348 解消

## よかったこと

### 設計先行 + git archaeology が「削ってよい線」を引いた
#347 を設計先行で起こし、git 史実調査で**本質的制約**（lock 最小化 / レイヤー境界 / 2 AtomicBool / INDEX_WRITE_LOCK、commit 証拠付き）と**偶発的複雑さ**（所有権追放 / キー二重メンテ / top_n 漏れ）を切り分けた。これにより「カテゴリ B はキャッシュ、既定 live-read」と再定義し、history を live-read 化して B を既約核（SearchEngine 1 つ）へ縮小。当初の「全 B を reconcile する機構」より単純な「**B を減らす**」解に到達。整合機構を足す前に整合対象を消せないか問う、が効いた（`docs/development-principles.md` に反映）。

### 二段レビューが別種の欠陥を別タイミングで捕えた
実装前のマルチパースペクティブ DESIGN レビュー（並行性 / 呼び出しグラフ / 不変条件）が **panic wedge** を実装前に検出 → catch_unwind を書く前に織り込めた。実装後の Codex レビューが **panic="abort" での過大主張**を検出。前者は「設計の前提の脆さ」、後者は「事実と主張のズレ」と、別レイヤーの欠陥を別タイミングのレビューが捕えた。

### 「実証で判断」が過剰修正を回避
Codex「出荷不可（panic wedge）」を盲従せず、`Cargo.toml:13` の `panic="abort"` を実際に確認 → 事実は認めつつ「release は元々 abort で挙動不変＝regression なし、silent wedge も起きない」と severity を実証で切り下げ。過剰修正（panic 方針変更・Result 大改修）を回避し、コメント是正という最小の正しい対応に着地。#338 サイクルの「盲従も性急な反論もせず実証で判断」を再演。

### TDD が責務配置を示し lost-update の核を固定
Phase 1 で engine 層の disk-非分離（load/save が固定パス）が「履歴ロジックの責務は history.rs（disk-free テスト可）」を示唆。Phase 2 で `complete_index_drain` を「無条件 clear」の仮実装にして lost-update の核テスト（`complete_index_drain_keeps_stale_when_config_changed_during_build`）を RED→GREEN で固定し、「ビルド中変更を取りこぼさない」不変条件をコードで保証。

---

## 伸びしろ

### 設計スケッチが実呼び出しグラフを取りこぼしていた
設計メモ §4 スケッチは「`update_config` が bit を立てる」前提だったが、first-run / 手動 rebuild / finish 窓を取りこぼしていた（config 変更を伴わない経路で begin が空振り）。実装時に全呼び出し元を grep して「`start_index_build` が bit を立てる」に確定。API スケッチを実装に落とす前に**駆動経路（特殊フロー含む）を洗う**必要を再認識（既存の「初回フローとガードの相互作用を検証する」を設計確定時にも適用）。

### panic 戦略（abort/unwind）をレビュー観点に入れていなかった
catch_unwind による wedge 回復を設計したが、release の `panic="abort"` を実装前レビューも自分も見落とし、Codex が後段で検出。「panic 回復設計は build profile 依存」を `AGENTS.md` 事前調査に反映済み。

### データ損失経路を Phase 1 の自前レビューが踏めなかった
config ReadFailed → live-read 履歴剪定のデータ損失を、Phase 1 の TDD（disk-free）も設計レビューも踏まなかった（config_watcher 経路を通らない）。Codex アドバーサリアルレビューが捕捉。データ損失系は独立視点（特に実コードを読む手段）が効くを再確認。

### Codex がこの環境で不安定
Codex のサンドボックス git が常に失敗（exit -1）し、GitHub MCP フォールバックの発火が不安定（このサイクルで 5 回中 2 回のみ実結果）。リトライは宝くじ。実コードを確実に読むリポジトリ内サブエージェントレビューが確実な代替（メモリに記録）。

---

## ネクストアクション

- [ ] **#347 Phase 3**: `docs/architecture.md` に StaleSet 契約 + 設計メモ参照、`.claude/rules/*` 同期（`.claude/` 編集は相談のうえ）。完了で #347 クローズ
- [ ] `/health-check` でモジュール構成・SPEC.md 番号・メモリ・スキル・ルールの整合を確認（サイクル末）
