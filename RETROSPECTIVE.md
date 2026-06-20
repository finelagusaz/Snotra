# Retrospective — 検索モードの二軸モデル化（散在ガード集約: 調査 → 設計（一段抽象化）→ plan-review/state-check → 段階的 TDD → dry-check → health-check）

## よかったこと

### ユーザーの「一段抽象化してみよう」が設計を本質へ引き上げた
当初は散在ガードを discriminated union（A: 派生メモ / B: ネスト union）へ畳む二択で進めかけたが、ユーザーの「より本質的なものは何か」という問いで、**「モード」は単一の物ではなく二つの直交軸（View＝結果リストを占めるもの / Interp＝入力の意味）の混合物**だと気づいた。A/B の問いは「立てるべきでない問い」として溶け、`instantCommandMode` が持続 boolean であること自体が混同の臭いだと判明。設計の対象が「mode の型」から「入力欄と結果リストは何の関数か」へ移ったことで、綻び（非対称ガード）が修繕でなく**消滅**する解にたどり着いた。設計の早い段階で一段抽象化を促す問いは、局所最適を脱する効果が大きい。

### plan-review が自分自身の設計の穴を検出した
初版 plan は軸メモを `{ kind: ... }` のオブジェクト union で書いていた。`/plan-review`（Explore×3）の reactivity 観点で、3 エージェントは「再計算オーバーヘッドはあるが effect は高さキャッシュで抑止」止まりだったが、**SolidJS のメモ既定等価が `===`** という基礎に立ち返ることで「新オブジェクト identity が毎計算で伝播し、`query()` 依存の `interpKind` が plain 打鍵ごとにアイコン effect を stale 化する実害」まで届いた。プリミティブ判別子メモへ修正。レビューエージェントの所見を鵜呑みにせず、フレームワークの基礎仕様に照らして一段深掘りする価値の好例。

### 段階的 TDD + 派生テストモックで挙動不変リファクタを安全・可逆にした
6 フェーズを各 test-green コミットで刻み、毎フェーズ Red→Green を実証（Phase 1: viewKind/interpKind 未定義で 8 件 RED → 実装で GREEN、Phase 2: tool×indexing が現行式で false=RED → switch で GREEN）。コンポーネントテストは `mockViewKind`/`mockInterpKind` を下位シグナルモックから**導出**させ、既存テストを一切書き換えずに緑を維持。挙動不変を 226 テストで連続的に担保しつつ進められた。

### 偽の Read 出力を検出し ground truth から取り直した
Read が `tool-selection.ts` の重複宣言（コンパイル不能）や `MainApp.tsx` のコード内メタ注釈（`// 設計確認のため後で詳細を見る`）など、もっともらしいが実在しない像を返した回があった。「typecheck の通るリポジトリに再宣言は存在し得ない」と矛盾に気づき、Grep / 再 Read で実ファイルを確認してから設計を続けた。phantom な実装事実の上に設計を積むのを回避できた。

---

## 伸びしろ

### SolidJS 派生メモの伝播セマンティクスを設計段階で検証しきれなかった
オブジェクト union メモの毎計算伝播は plan-review で拾えたが、**plan を書く時点で**「この派生メモは何に依存し、値不変でも伝播するか」を検証していれば設計の穴自体を作らなかった。createMemo / createEffect を新設する際は、依存シグナルの変更頻度と等価セマンティクス（プリミティブ vs オブジェクト）を設計段階でチェックする。→ 一般則を `ui/CLAUDE.md` 実装パターンに反映済み。

### 計画の検証コマンドを SSOT と照合せずに書いた
plan の検証コマンドに `npm run lint` を記載したが、`package.json`・`docs/build-commands.md`（SSOT）のいずれにも存在しなかった（typecheck が型検査を担う）。計画に検証コマンドを書くときは、コマンド名を SSOT である `docs/build-commands.md` と照合してから記す。

### 網羅 switch + assertNever は変数束ねが必須（既知だが踏んだ）
`switch (viewKind()) { ... default: assertNever(viewKind()) }` は default 枝で関数を再呼び出しするため `never` に絞られず typecheck エラー。`const vk = viewKind()` に束ねてから switch / assertNever に渡す必要がある。typecheck で即検出できたため軽微。
