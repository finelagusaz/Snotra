# Retrospective — 単一ドライバ sequential 2-issue サイクル（#436 Phase 1 分割リファクタ merged / #461 IndexCache Cow 統合 PR #463）

## よかったこと

### issue の額面を疑い「一段抽象化 → concern 分解 → 射程確定」を両 issue で貫いた
#436（size:L・4 direction）と #461（cache 集約）のどちらも、issue の提案規模を鵜呑みにせず実コードを読んで concern 分解し、cost/risk で射程を絞った。#436 は 4 direction を「1 事実の N 箇所コピー」に抽象化し、2+3（読み手側 80/20）を実装・1 を #461 へ分離・4 を偽陽性（config↔engine の意図的層境界）と判定。#461 は「大集約」を owned/borrowed 双子・versioning・field-list マクロの 3 concern に割り、畳めるもの（Cow 統合）だけ実施し versioning（irreducible）と macro（過剰設計）を明示除外。issue を書いた本人（前サイクルの自分）の楽観的な「13→1 集約」像を、実コードで「本当の 80% は footgun 除去」に修正できた。射程は situation/abstraction を prose で提示した上で AskUserQuestion に委ねた。

### 前提をコードで裏取りしてから計画を建てた（feasibility spike）
#461 の approach 全体が「serde の `Cow<[T]>` がバイト一致で serialize・Owned で deserialize」という一点に依存していたため、plan 着手前に使い捨て spike を実 serde/postcard で回して version バンプ不要を確定してから計画を書いた。前提が崩れれば approach 自体が不成立ゆえ、投機的な計画を防いだ。spike は実施後に撤去し、知見を plan/PR に記録した。

### 多経路レビューが設計・実装の両段階で独立に別クラスの盲点を回収した
#436 は plan-review（Explore 2 体）+ Codex 独立レビュー（設計段階）が私と Explore の盲点 4 件（9000 閾値の score_tier 混同・SPEC §3.2 誤参照・bitmask pre-filter の順序不変条件・スコープ記述の矛盾）を回収。#461 は code-reviewer が golden テストの「生成方向」の穴（新コード採取ゆえ forward-stability のみ）を指摘し、凍結バイト列からの deserialize に強化。設計段階の Codex と実装段階の code-reviewer が別クラスの欠陥を捕らえ、独立フレーミングの価値が両段階で確認された。

---

## 伸びしろ

### 「バイト形式不変を主張するリファクタ」に既存の後方互換ルールを初手で適用しなかった
snotra-core/CLAUDE.md には #394 由来の「旧オンディスク形式が deserialize できるテストを別に追加する」ルールが既にあったが、#461 の Cow 統合を「バイト不変ゆえ該当せず」と見送り、golden を新コード出力から採取して forward-stability のみのテストにしてしまった（code-reviewer が検出）。既存ルールのトリガーが「serde 表現を変更するとき」に閉じて読め、「形式不変を主張する struct リファクタ」も証明対象だという一般化が抜けていた。→ snotra-core/CLAUDE.md のデータ永続化ルールに「凍結バイト列を入力にした load テストで後方互換を証明」「着手前 spike で往復バイト一致を実証」を追記済み。

### plan の「影響範囲網羅」が型構築箇所（テスト fixture）を取りこぼした
#461 の plan は struct フィールド型変更（Vec→Cow）が `index_cache_binary_roundtrip` テストの *構築* 箇所を compile-fail させることを列挙し損ね、「影響範囲網羅」と自己申告したが plan-review（Explore）が回収した。型シグネチャ変更は呼び出し元だけでなく「その型を構築する箇所（テスト fixture 含む）」も compile-fail 対象。実害は最小（Phase 1 の cargo test の build gate で即露見）だったが、plan の網羅性主張を誇張した。AGENTS.md の「compile-fail を検出器として使う」ルールが検出自体は保証しており（実際 build が捕捉）、これは検出ギャップでなく plan 記述精度の問題ゆえ新ルールは追加しない。
