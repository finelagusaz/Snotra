# Retrospective — #532 Phase 2 SU6.5（flip 前ハードニング: 計測ゲート + #643/#648/#652）

## よかったこと

### 計測が、コードレビューを 2 度通過したコードの gap を 2 つ暴いた
G1 メモリ実測は egui-hidden が trim されず可視時同値（43.4MiB）のままだった gap を、G2 目視は結果リストの早期スクロールを暴いた。どちらも task-review と最終 whole-branch review（opus）が「Ready to merge」にした後である。`hide_egui_main` は SU2〜SU6 でレビューを重ねたが、hidden の working set 挙動は一度も測られていなかった。#634/SU4 で確立した「設計を決める前に測る」の対を成す「マージする前に測る」——計測ゲートはレビューと冗長でなく、実行時リソース挙動という別クラスを捕える。この気づきを `docs/development-principles.md` デバッグ節へ昇格した。

### 視覚症状の精確化が、バグと設計課題を正しく切り分けた
「窓が少し高い」という第一印象を症状語として固定せず、font_size を 15→24 に振って観測したら、正体が「固定行高 30px × 小フォントの余白」だと分かった。これは parity 中立（WebView2 も 30px 固定）の設計課題であり、Task 8 の scroll 早期発火（真の parity バグ）とは別物だった。前者は #646（UI デザインパス）へ、後者は SU6.5 で即修正、と正しく振り分けられた。第一印象に anchor せず変数を振る規律（memory [[debug-visual-render-precise-symptom]]）が効いた。

### auto-close ガードを機構どおり守り、#532 を守り切った
PR 作成時・マージ後の両方で `closingIssuesReferences` の一覧で判定し（自分のキーワード走査でなく）、#532 が一覧に不在・OPEN 維持・`closed:>=mergedAt` 検索で知らない close ゼロを確認した。手順を「注意」でなく「実行する検査」として回したことで、13 サイクル近く続く #532 を今回も守れた。

### subagent-driven + human-in-loop 計測が滑らかに回った
controller が scriptable 部分（release ビルド・measure:memory・trace 集計・PrintWindow 試行）を駆動し、user が Alt+Q・目視・打鍵を担う分担で、8 タスク + 4 ゲート + 機能スモークを一続きで完走した。実測の生数値は #532 コメントへ、進捗は ledger へ durable に残した。

---

## 伸びしろ

### 偽の全称を 2 度書いた——いずれも自分の計画文由来
Task 2 の「WebView2 も generic 表示ゆえ parity 影響なし」と Task 7 の「単一プロセスゆえ子孫 BFS は snotra 自身のみ」は、どちらも brief（＝計画）に verbatim で書いた全称主張が実装コメントへ転記され、レビュアーが実物照合（`UpdateToast.tsx` / `snotra-settings` 子プロセス）で捕捉した。「全称表現は前提条件とセット・検算してから書く」規範（`AGENTS.md`）は存在するが、レビュー時には効いても plan/brief **執筆時**には salient でなかった。規範不在ではなく適用漏れゆえドキュメントは足さないが、「レビュアーでなく自分が最初の書き手であるコメント・計画文にも同じ検算を当てる」ことを次サイクルで意識する。

### trim gap は SU2 の時点で測れたはずだった
egui hide 経路を作った SU2 で hidden メモリを一度測っていれば、trim 欠落は SU6.5 まで持ち越さなかった。「メモリ削減」という #532 の価値の中核が実行時プロパティである以上、その経路を作った時点で最小の計測を挟むべきだった。今回は flip ゲートが backstop として機能したが、計測を「サイクル末のゲート」でなく「実行時プロパティに触れた時点」へ前倒す余地がある。

### 計測器が実装に残っていなかった
#628 が引用した `raster_ms` トレースは SU1 スパイク時の計測で、製品コードには残っていなかった。G3(b) を測る前に `SNOTRA_EGUI_PAINT_TRACE` を作り直す一手が要った。計測が再び必要になると分かっている軸（描画コスト・メモリ）は、env ゲートの計器を恒久化しておけば再測が速い。今回 measure-memory.ps1 を PrivWS 軸で恒久化し paint 計器も入れたので、SU7 以降の再測はこの資産を使える。
