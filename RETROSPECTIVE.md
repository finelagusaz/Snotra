# Retrospective — issue #535 起動レーンの `activationInFlight` を `exclusive`(mutex) primitive へ集約

純粋 refactor。`search.ts` の 4 関数が手書き反復していた二重起動防止 mutex（module boolean `activationInFlight`）を、`createExclusive()` single-flight primitive へ集約した。検索 lane の supersede primitive（`latestRun`・#540）の姉妹。挙動不変・公開 API 不変（384 テスト + 変異テストで実証）。

## よかったこと

### 独立再導出（plan-review Step 2b）が「作者の盲点」を 1 件拾った
成果物監査（Explore）と独立導出（Plan・plan.md を読ませない）の 2 体が、いずれも独立に「`withLaunchLifecycle` の JSDoc が削除予定の `activationInFlight` を名指しで残す」漏れを検出した。計画初版はこのコメントを列挙し損ねていた。#495 の「Step 2b は常に実施」方針が、局所的な単一ファイル refactor でも実効を持った実例。一致（同期起動タイミング・boolean 契約・単一 mutex・SPEC 更新不要）が積み上がったことも、盲点なしの能動的証拠として機能した。

### 偽陽性テストの罠を実装中に発見し、変異テストで判別能力を接地した
「起動 in-flight 中の 2 回目が弾かれる」テストを通常モードで書くと、`withLaunchLifecycle` の `clearResults()` により 2 本目が空 results で無条件 false になり、mutex が壊れていても launch 呼び出しが増えず判別不能（false green）だと気づいた。`launchWithSelectedTool` が `results()` ではなく `frame.tools` を読む点を利用し、ツール選択モードへ切替えて判別可能にした。さらに primitive の guard を一時無効化する変異テストで、テストが確実に落ちることを確認した——「テストが本当にバグを検知するか」を接地する実践。

### 構造的一致により差分が最小化された
`try {`（2-space）+ body（4-space）という現行構造が `return (await activationLane(async () => {`（2-space）へそのまま置き換わり、body の再インデントが一切不要だった。手書き mutex 反復を primitive 1 箇所へ畳んだ結果、`activationInFlight` の 12 参照が消えた。姉妹 `latestRun` の品位（純粋ファクトリ・JSDoc・同期起動）に揃えたことでレビューの見通しも良かった。

---

## 伸びしろ

### 識別子を削除するとき、コメント/JSDoc 内の名指し参照を記憶から列挙して 1 件漏らした
計画初版で `tryModalActivate` コメントの更新は挙げたが、同じ `activationInFlight` を参照する `withLaunchLifecycle` の JSDoc を漏らした。research 段階の grep は `files_with_matches` で「search.ts が持つ」までしか見ず、具体的な出現は読解の記憶から列挙したため取りこぼした。既存の「検証の作法」（真実源を grep で数え上げる）を content モードで徹底していれば初版で拾えた——原則は既にあり plan-review が捕捉したため新規ルールは追加しない（ドキュメントを重くしない判断）。次サイクルでは「識別子の改名・削除時は content grep で全出現を列挙してから計画に落とす」を意識する。

### テストのモック実装リーク（`clearAllMocks` は `mockImplementation` を復元しない）
test 1 で `launchWithTool` を deferred 化した実装が test 2 へ漏れ、never-resolve な launch を待ってタイムアウトした。`beforeEach` の `vi.clearAllMocks()` は `.mock.calls` を消すが実装差し替えは復元しない（復元するのは `resetAllMocks`）。今回は timeout で loud に落ちたが、`mockResolvedValue` の漏れなら誤った値で緑になる（false green）。repo 固有の落とし穴（`beforeEach` が `clearAllMocks` を使い、明示再設定するのは `api.search` のみ）として `ui/CLAUDE.md` テスト基盤に 1 行追記した。
