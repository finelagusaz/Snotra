# Retrospective — issue #536 検索/instant の生タイマーを所有 `OwnedTimer` primitive へ統合

純粋 refactor（primitive 抽出プログラム第 3 弾。`latestRun`(supersede)・#540 / `exclusive`(mutex)・#541 の姉妹）。散在した生 `setTimeout`/`clearTimeout`（search debounce の `debounceTimer`+`leadingFired`、instant の `instantCmdDebounceTimer`）を、timer resource だけを所有する `createOwnedTimer(ms)`（`arm`/`cancel`/`isPending`）へ統合。挙動不変・公開 API 不変（396 テスト + adapter テスト + codex 敵対レビューで実証）。設計は当初 `createDebouncer`（leading policy 内包）から、多レンズ探索を経て `OwnedTimer`（resource/policy 分離）へ転換した。

## よかったこと

### 多レンズ設計探索 + codex 敵対レビューが issue の前提を覆した
「一段離れて別の should-be は?」という問いに、canonical / minimalist / FSM の 3 レンズを持つサブエージェント + codex 2 回（計画反証・設計攻め）を走らせた。**枠組みの独立した 4 視点が、独立に 2 点へ収束**——①`leadingFired` は `timer !== undefined` と等価で冗長、②再入は同期発火をなくせば構造的に不能（文書契約が要らない）。これが設計を `createDebouncer`（leading policy を primitive に内包）から `OwnedTimer`（timer resource だけ所有・policy は呼び出し側）へ転換させ、偶発的複雑さ（フラグ・再入契約・未使用 `dispose`）を**設計で消した**。単一 framing の plan-review では出ない盲点を、framing の多様性が暴いた。「実行の独立」より「枠組みの独立」が盲点に効く、を実証。

### 「緑は等価の十分条件でない」を adapter テストで能動的に埋めた
既存 `search.test.ts` は `runAllTimersAsync` で一括 flush し leading/trailing を区別しない。codex がこの片方向性（既存緑 ≠ 挙動固定）を指摘。50ms/30ms 境界をまたぐ adapter テスト 4 件（leading 即時 IPC・burst の trailing 1 回/最後の query・instant の即時クリア・最新 filterName の 1 回取得）を足し、載せ替えの等価性を **store 越しに直接固定**した。回帰の十分条件（1 つ赤なら挙動変更）と、必要挙動の能動的固定を分けて設計した。

### code-reviewer が「計画に書き込まれたバグ」を捕捉（実コードは正しい）
Phase 2 で plan.md が `leadingFired` の等価性を**符号反転**（`=== undefined` と記述）していたのを code-reviewer が検出。実コードは正しく `!== undefined` を使っていたが、放置すると「コードを計画に合わせる」将来修正が退行を生む。**実コードとドキュメントの不一致は、ドキュメント側が誤っていても危険**——訂正した。

## 伸びしろ

### 状態フラグの冗長性を、当初計画の自己レビューで見抜けなかった
当初計画（`createDebouncer`）は `leadingFired` を「残す」前提で書かれ、self-review「シンプル化の挑戦」も冗長性を素通りした。`leadingFired ≡ (timer !== undefined)` の等価性は、外部の敵対的レンズ（FSM + codex）が初めて暴いた。教訓: **状態フラグを導入するとき「既に所有する別状態（timer の有無等）から導出できないか」を、フラグを計画に落とす前に検算する。** 既存の「シンプル化の挑戦」の射程内だが、"新しいフラグ" にも明示的に当てる意識が要る。新規ルールは追加しない（既存原則で包含・ドキュメントを重くしない判断）。

### 計画に書く等価性/不変条件の主張を、代表状態で向きを検算していなかった
plan.md の符号反転は、AGENTS.md「検証の作法」の既存原則——「計画に書いた判定ロジック（述語）は実装前に代表入力で実行して測る」——を**等価性の主張に適用し損ねた**もの。「明らか」に見える等価も、各状態で値を辿って向きを確かめる。既存原則の射程内ゆえ新規追加なし。次サイクルでは「plan に `A ≡ B` を書いたら、全状態で A と B の真偽表を並べて確かめる」を意識する。

### codex 敵対レビューの既定ターゲットは working tree（全コミット済みでは空を見る）
初回 `/codex:adversarial-review` が working tree を対象にし、全コミット済みの #536 実装ではなく無関係な未追跡 JSON を見た（verdict は approve だが空レビュー）。出力の "Target: working tree diff" で気づき `--base main` で再実行。**ブランチ全体を敵対レビューするには `--base <ref>` が要る**（working tree がクリーンなとき）。メモリ [[codex-exec-adversarial-review]] へ追記。
