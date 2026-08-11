# 実装計画 — issue #1034（計器の制約 2 件 + 故障注入の教訓を横断的な正本へ吸収する）

## 目的

#1004 の spec にしか無い「次に計器を作る人が必ず踏む」事実を、**それぞれ正本 1 か所へ置き、
他は参照にする**。新しい事実の発見はせず、置き場の裁定と写しの解消だけを行う。

## 受け入れ条件

1. 制約 1（trace の 1 行が約 10 ms）の正本が `PERFORMANCE.md`「計測と受け入れ基準」に在り、
   **前提条件（`SNOTRA_TRACE` 有効・stderr をファイルへリダイレクト）つきで**書かれている
2. 制約 2（smoke の索引は 1 件）の**帰結**が `docs/build-commands.md`「スモーク運用メモ」に在る。
   **事実そのものは書き足さない**（既に 2 か所に在るため）
3. 教訓 3（故障注入が発火しないときの読み方）の正本が
   `docs/development-principles.md`「構造的設計原則と強制の階梯」に在り、
   `.claude/rules/safety-nets.md` はそこへ向くルーター 1 文だけを持つ
4. `docs/superpowers/specs/2026-08-10-search-worker-design.md` §3.3 が数値を持たず、
   **決定（H6 を置かない）とその理由の骨は読める**
5. `PERFORMANCE.md` に残る同じ事実の写しが 0 になる（`:484-488` と `:550`）
6. `npm run governance:check` が緑（新設した参照が正準形で着地する）

## 裁定（issue の「どこへ置くか」への回答）

| # | 正本 | 却下した候補と理由 |
|---|---|---|
| 制約 1 | `PERFORMANCE.md`「計測と受け入れ基準」 | `docs/build-commands.md`——同節が既に「**このリストが計器の正本である**——`docs/build-commands.md` には置かない」と射程を宣言しており、計器の話をそちらへ割ると宣言が偽になる。`src-tauri/src/trace.rs` の `//!`——単独の正本にはしない（計測の作法であってコードの契約ではない）が、**次に計器を作る人が居る場所**なのでポインタ 1 行は置く |
| 制約 2 | `docs/build-commands.md`「スモーク運用メモ」 | 他候補なし（issue も 1 つだけ挙げる）。**書くのは帰結だけ**——事実「索引 1 件」は `scripts/smoke-egui.ps1:109` と同節に既に在り、3 枚目を作ると `AGENTS.md`「文書に事実の写しを増やす変更」の踏み方そのものになる |
| 教訓 3 | `docs/development-principles.md`「構造的設計原則と強制の階梯」（既存の故障注入 bullet の**逆向き**として） | `.claude/rules/safety-nets.md` を正本にする案——同ファイルは既に「**注入の強さの本文は `docs/development-principles.md`「構造的設計原則と強制の階梯」**」と委譲しており、逆向きだけ rules 側に置くと同じ話題が 2 か所に分かれる。**この分担は既存であって新設ではない** |

**「debounce が追い越しを構造的に防ぐ」を製品側の文書（`src-tauri/CLAUDE.md` 等）へも書くことは
しない**——issue はこれを故障注入の教訓として扱っており、一般則の根拠（実例）として同じ bullet に
書けば足りる。製品側の不変条件として独立に必要になったときに、そのとき書く（YAGNI）。

## 変更ファイルと対象シンボル

| ファイル | 対象（見出し・シンボル） | 変更 |
|---|---|---|
| `PERFORMANCE.md` | 「計測と受け入れ基準」 | 制約 1 の正本 bullet を新設 |
| `PERFORMANCE.md` | 「打鍵中のフレーム所要 — A 側（#1004・2026-08-11・release・実運用点 312,180 件）」 | 但し書きから数値・機序を落とし参照へ |
| `PERFORMANCE.md` | 「フレーム後半の帰属 — #1032（2026-08-11・release・実運用点 312,180 件・3 標本 × 2 巡）」 | 「（1 本約 10 ms）」を削る |
| `docs/build-commands.md` | 「スモーク運用メモ」 | 制約 2 の帰結 bullet を新設 |
| `docs/development-principles.md` | 「構造的設計原則と強制の階梯」の故障注入 bullet | 逆向きを追記 |
| `.claude/rules/safety-nets.md` | 「効いていることは、フォールトインジェクションで一度は実測する」 | ルーター 1 文を追記（**面積の残りが 385 字しかない**——下記） |
| `src-tauri/src/trace.rs` | `//!` | ポインタ 1 行を追記（英語・既存ブロックの言語に合わせる） |
| `scripts/lib/SnotraTraceInvariants.psm1` | H7 の arm のコメント（「故障注入で発火を実測済み・#1004 PR 2」の近傍） | **どう発火させたか**の再現手順を追記（人間の裁定で射程へ追加） |
| `docs/superpowers/specs/2026-08-10-search-worker-design.md` | §3.3 | 数値を参照へ置換（**末尾段落「この判断で捨てた前提を記録しておく」は残す**——下記） |

**触らないもの（根拠つき）**:

- `PERFORMANCE.md:1668`（`SNOTRA_EGUI_INPUT_TRACE` の項）と `:1707`、
  `snotra-egui-runtime/src/input.rs:26, :136` — **runner での別の計器の実測値**であり、
  開発機の `SNOTRA_TRACE` の値と 1 つの数へ併合してはならない。新しい正本からは
  「観測点が違うので併合しない」と断ったうえで相互参照する
- `docs/superpowers/plans/2026-08-10-search-worker.md`（4 箇所）— issue の「やること」が
  名指すのは spec だけであり、plans は当時の実行記録である。**この扱いは前提として明示する**
- `scripts/smoke-egui.ps1:109` / `docs/build-commands.md` の「索引 1 件」の記述 —
  制約 2 の事実の在り処であり、帰結だけを足す

**§3.3 には外部からの依存がある（自己照合の項目 7 で発見）**: `src-tauri/src/egui_shell/layout.rs:406`
の doc コメントが「**間隔は合否ではなく内訳である**」の**正本として §3.3 を名指している**。
依存しているのは §3.3 末尾の段落（「この判断で捨てた前提を記録しておく」——`interval_us` は
16 件中 15 件が予算超過で、間隔で判定していれば worker 化の後も永久に赤かった）である。
**この段落は残す。** なお当該引用は `§3.3` という序数形で書かれており正準形ではないため
`governance:check` の G-heading-refs は照合しない（**機構は守らない**——手で確かめる）。
序数参照そのものを正準形へ直すことは本 issue の射程外とする（別の変更が要る）。

## 実装順序

Phase 1 → 4 の順。**Phase 1 で正本を置いてから写しを参照へ落とす**（逆順にすると参照先が
無い時点が生じる）。

## 不変条件と異常系

- **全称表現は前提条件とセットで書く**: 「trace の 1 行は約 10 ms」は
  「`SNOTRA_TRACE` 有効・stderr をファイルへリダイレクト」の下でのみ真。前提を落とさない
- **数え上げを散文に書かない**: 「緩和は 2 通り」のような数は書かず、実例を名指しする
- **新しい参照は正準形** `` `<path>.md`「<見出し>」 `` で書く（`.claude/rules/governance-docs.md`）。
  同一ファイル内の自己参照も同じ形でよい（`scripts/governance-check.mjs` の
  `resolveRefTarget` は自ファイルを除外せず、`collectAnchors` が ATX 見出しを拾う・実読で確認）
- **異常系**: `governance:check` が「見出し参照が着地しない」を出したら、**参照先の見出しを
  現物で読み直してから**文字列を直す（前方一致・`**`・バッククォート・「」・空白は正規化される）
- **セーフティネットの変更に当たる**（`.claude/rules/safety-nets.md` と規範文書）。
  同ファイル「規範（ドキュメント・スキル・チェックリスト）を足すとき」に従い、
  **条項を足す前にその節のスコープ宣言を確かめる**——
  「構造的設計原則と強制の階梯」は冒頭で「横断原則としてここに明文化する」と射程を宣言しており、
  故障注入の作法はその内側にある（確認済み）

## 受容する残余（plan-review で明示化）

- **spec へ書く参照は機械照合されない。** `scripts/governance-check.mjs` の `headingRefDocs` は
  `docs/superpowers/` を母集団から外す（#589 で非規範化）ため、spec に正準形で書いた参照は
  G-heading-refs が見ず、参照先の改題で沈黙して腐る。**それでも正準形で書く**——読み手にとって
  最も曖昧さが無く、腐っても害は歴史資料の中に閉じる
- **`docs/superpowers/plans/2026-08-10-search-worker.md` の 4 件は残る。** spec だけを参照化するので、
  同じ歴史資料の中で扱いが割れる。issue の指示が spec のみを名指すことを優先する
- **`snotra-egui-runtime/src/input.rs:26, :136` の 17〜56 ms は残る**（別の計器・別条件であり、
  当該コードの設計判断の根拠として機能している）。片付けるなら別 issue

## テスト方針と検証コマンド

- `npm run governance:check`（カテゴリ F・必須。`*.md` は PostToolUse hook の対象外で沈黙は
  「何も走らなかった」）
- `trace.rs` を触るのでカテゴリ A。fmt / clippy / test は hook が自動発火するが、
  **`cargo doc --workspace --no-deps --document-private-items` は手で走らせる**
  （`.claude/rules/comments.md` のトリガー・hook は intra-doc link に沈黙する）
- **訂正した段落は diff ではなく現物を読み直す**（#755/#801。写しを参照へ落とすコミットは、
  同じ段落の別の主張が context 行に沈む形の典型）
- **写しの残数を機械で数える**: 変更後に**2 通りのパターンで**数え、件数を突き合わせる——
  `同期 write\|10〜18` と、より緩い `約 10 ms\|1 本約 10\|1 本あたり約 10`。生きた文書に残るのが
  **新しい正本 1 か所だけ**であることを確かめる（`docs/superpowers/` と `.superpowers/` と
  `target/` は母集団外）
- **正本の見出し名で逆引きする**: `grep -rn '「計測と受け入れ基準」'` で、参照が期待した箇所から
  張られていることを確かめる（綴りが割れて静かに落ちた写しを、参照の側から拾い直す）
- `npm run test:powershell`（`scripts/lib/**` を触るため。**`scripts/` の非 TS ファイルに
  PostToolUse hook の検査割り当ては無く沈黙は合格ではない**。コメントだけの変更でも、
  `SnotraTraceInvariants.Tests.ps1` がソーステキストを走査する検査を持つので緑を確かめる。
  未ビルドなら先に `cargo build -p snotra`）
- **`.claude/rules/safety-nets.md` を触った直後に `governance:check` を再実行し、rules の字数を読む**
  （baseline 11,615/12,000 字・残り 385 字。**赤くなったら追記を取り下げる**——本 issue の正本は
  `docs/development-principles.md` 側であり、ルーターの 1 文は無くても計画の受け入れ条件は満たす）

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。挙動を 1 行も変えない（文書だけの変更）
- `docs/architecture.md`: 不要（アーキテクチャ・横断パターンに影響しない）
- `RETROSPECTIVE.md`: 不要（サイクル末の `/retrospective` が所有する）

## 作業項目

### Phase 1 — 制約 1（trace の 1 行の費用）

- [ ] `PERFORMANCE.md`「計測と受け入れ基準」へ正本 bullet を新設する。**置く位置は
      「ランタイムの計測は `SNOTRA_TRACE=1` の構造化トレース…で行う」の直下**——その下の
      「egui/softbuffer の計器は**5 つの**env」というリストの中へ入れると、その数え上げが偽になる
      （`SNOTRA_TRACE` は別の意味論であり 5 件に含まれない）。含める要素:
      前提条件 / `eprintln!` が同期 write であること / 実測 10〜18 ms（開発機・release・
      `SearchDispatch::issue` と `SearchState::set_results` しか挟まない区間で 12 ms・
      同条件の `Engine::search` は 7〜162 µs）/ 予算 16,700 µs は trace 2 本で超えること /
      **絶対値で合否判定できず A/B の差分としてなら読めること** / **区間ごとに吐かないこと**と
      その緩和の実例（末尾 1 本へ集約〔#1004〕・区間から控除〔#1032〕）/
      runner の値は `SNOTRA_EGUI_INPUT_TRACE` の項が持ち併合しないこと
- [ ] `PERFORMANCE.md` A 側の但し書き（`:484-488`）を、数値と機序を落として
      `PERFORMANCE.md`「計測と受け入れ基準」への参照へ置き換える（表を読むのに要る
      「差分として読む」は残す）
- [ ] `PERFORMANCE.md` #1032 節（`:550`）から「（1 本約 10 ms）」を落とす
- [ ] `src-tauri/src/trace.rs` の `//!` へポインタ 1 行を足す（**英語**——既存ブロックが英語で、
      `docs/comment-guidelines.md`「言語（日英）」が同一ブロック内の混在を禁じる。**数値は書かない**）

### Phase 2 — 制約 2（smoke の索引は 1 件）

- [ ] `docs/build-commands.md`「スモーク運用メモ」へ帰結 bullet を新設する。含める要素:
      索引規模に依存する性能は smoke で測れないこと / 件数ゲートを持つ検知器は永久 SKIP に
      なること（#930 の「発火しえない検出器」）/ **規模に依存しない検知器なら置けること**
      （H7 が実例）/ 実運用点は手動計測が唯一の観測手段であること。
      **事実「索引 1 件」と scan の固定先は書かず、既存の記述と `scripts/smoke-egui.ps1` を指す**

### Phase 3 — 教訓 3（故障注入が発火しないときの読み方）

- [ ] `docs/development-principles.md`「構造的設計原則と強制の階梯」の故障注入 bullet へ
      逆向きを追記する。含める要素: 発火しないことは検査が縛れている証拠にならないこと /
      #1004 の 2 度の失敗（① `let _ = rest.count();` が溜まった要求を**消費して捨て**
      coalescing と同じ結果になった〔正しくは `let _ = rest;`〕、② `Debouncer`（50 ms・leading）が
      要求の間隔を保つのに対し実運用点の検索が 40〜70 ms で拮抗し、worker への人工的な遅延を
      入れて初めて追い越したこと）/ **「めったに起きないが起きたら深刻」の検知器は人工的にでも
      一度発火させないと発火しうるかが分からない**こと
- [ ] `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」
      の該当 bullet へ、逆向きも同じ節が本文を持つ旨を 1 文で足す（写しは置かない）
- [ ] `scripts/lib/SnotraTraceInvariants.psm1` の H7 の arm へ再現手順を追記する。含める要素:
      `let _ = rest.count();` は溜まった要求を**消費して捨てる**ので coalescing を外したのと同じ
      結果になり追い越しが起きないこと（正しくは `let _ = rest;`）/ `Debouncer` が要求の間隔を
      保つのに対し実運用点の検索が拮抗するため、**worker へ人工的な遅延を入れないと確実には
      追い越さない**こと。**一般則は書かず**、`docs/development-principles.md`
      「構造的設計原則と強制の階梯」を指す（写しにしない）

### Phase 4 — spec の参照化と検証

- [ ] `docs/superpowers/specs/2026-08-10-search-worker-design.md` §3.3 の (1)(2) から数値を落とし、
      **決定（H6 を置かない）と理由の骨**を残したうえで
      `PERFORMANCE.md`「計測と受け入れ基準」と `docs/build-commands.md`「スモーク運用メモ」を指す。
      **末尾段落（`interval_us` の 16 件中 15 件）は `layout.rs` が正本として引いているので残す**
- [ ] `src-tauri/src/egui_shell/layout.rs:406` の引用が指す内容が §3.3 に残っていることを、
      **現物を読んで**確かめる（序数参照ゆえ `governance:check` は照合しない）
- [ ] `npm run governance:check` を実行して緑を確認する
- [ ] `cargo doc --workspace --no-deps --document-private-items` を実行して緑を確認する
- [ ] 写しの残数を grep で数え、生きた文書に残るのが正本 1 か所だけであることを確かめる
- [ ] 触った段落を**現物で**読み直し、隣接する主張が偽になっていないか確認する

## 未確定（実装前に潰す）

- [x] `/plan-review` のリスク判定 — **高リスク**（「hook、CI、rules、skills、ガバナンス文書を
      変更する」に逐語で該当）。`--deep` は「ユーザーが明示した」場合のフラグで、既に高リスクと
      判定される本計画では追加の効果を持たない（`.claude/skills/plan-review/SKILL.md` を実読）。
      高リスクは Step 2 か Step 2b の**どちらか一方**を 1 回。**Step 2b（独立導出）を選ぶ**——
      本計画の要は「置き場の裁定」と「写しの母集団」であり、計画の分解を前提としない導出の方が
      盲点に当たる（`.claude/rules/governance-docs.md` も移動・圧縮の完全性に独立再導出を求める）
- [x] 同一ファイル内の自己参照が G-heading-refs で着地するか — **する**。
      `resolveRefTarget` は自ファイルを除外せず、`collectAnchors` が ATX 見出しを拾い、
      照合は正規化後の前方一致（`scripts/governance-check.mjs` を実読）
- [x] `trace.rs` の `//!` へポインタを置くのは「必要なことだけ」を満たすか — **満たす**。
      issue 自身が候補に挙げており、**計器を足す人が実際に居る場所**である。数値を持たない
      1 行のポインタなので写しにならない（`.claude/rules/governance-docs.md`「書く約束」の
      (1) かぶりなく に適合）
- [x] 写しの母集団が閉じているか — **閉じている**。想定した書き方（`同期 write` /
      `1 本あたり` / `10〜18 ms` / `17〜56`）と緩いパターン（`trace` と `ms` の共起・
      `eprintln|stderr`）の 2 通りで数え、件数が一致した（`workspace/research.md` §2.2）

## plan-review 結果

- リスク: 高（`AGENTS.md` の「hook、CI、rules、skills、ガバナンス文書を変更する」に逐語で該当）
- レビュー方式: 独立導出1体（Step 2b）
- エージェント数: 1
- 成果物: `workspace/plan-review-1034-instrument-constraints.txt`
- **独立性の限定**: レビュアーが grep の除外指定を落とし、`workspace/` の断片が 2 度視界に入ったと
  自己開示している。**主要 2 裁定（制約 1 の置き場・17〜56 ms との突き合わせ）の根拠は汚染より前に
  取得された**とトランスクリプトの順序で示されているため、その 2 点は独立の一致として読む。
  写しの数え方と検証コマンドの導出は汚染後ゆえ独立性を主張しない

### 要対処（4 件 — 再照合の結果）

1. **制約 1 は条件つきで書き、17〜56 ms と併合しない** — 既に計画済み（受け入れ条件 1・不変条件）。
   再照合: 生きた層に `17〜56` が 4 件（`PERFORMANCE.md:1668, :1707` / `input.rs:26, :136`）あることを
   grep で確認済み
2. **`:550` は「（1 本約 10 ms）」だけ落とし「50〜96%」は残す** — 既に計画済み。
   再照合: 50〜96% は #1032 の実測から導いた同節固有の値であり、制約 1 の写しではない
3. **`:484-488` は表を読むのに要る 1 文を残す** — 既に計画済み
4. **`.claude/rules/safety-nets.md` は面積の残りが 385 字** — **新情報。計画へ反映した**
   （`npm run governance:check` を自分で実行し 11,615/12,000 字を実測。追記後の再測と、
   赤なら取り下げる手順を検証欄へ追加）

### 軽微（4 件 — 採否）

- **`scripts/lib/SnotraTraceInvariants.psm1` の H7 コメントへ再現手順を足す** — **採る**
  （人間が裁定・2026-08-11）。:393 は「故障注入で発火を実測済み」とだけ書き、
  **どうやって発火させたかが無い**
- **`docs/architecture.md`「検索フロー」へ debounce の頻度を 1 文足す** — **採らない**。
  一般則の根拠として `docs/development-principles.md` の bullet 内に実例として書けば足り、
  製品側の独立した不変条件としてはまだ要らない（YAGNI）。`:228` の現文は真のまま
- `trace.rs` はポインタ 1 行まで（正本にしない） — 計画どおり
- 制約 2 は新しい家を作らず既存の行へ帰結を足す — 計画どおり

### ⚠️（7 件 — 処理）

- A（spec への参照が機械照合されない）→ 受容残余へ明記済み
- B（`docs/development-principles.md` が「合意が要る規範」か）→ **人間へ諮る**（下記の問い 2）。
  ルート `CLAUDE.md` が名指すのは `CLAUDE.md` / `AGENTS.md` で、dev-principles は名指されていない
- C（plans の 4 件）→ 受容残余へ明記済み
- D（`input.rs` の 2 件）→ 射程外・受容残余へ明記済み
- E（「区間ごとに吐かない」の置き場）→ `PERFORMANCE.md` に同型の指示（「率を測る回と機序を測る回は
  別の回にすること」）が既に在るので同じ節に置く
- F（「5 つの env」の数え上げを壊さない）→ 作業項目へ置き位置を明記済み
- G（数え上げの完全性）→ 検証欄へ「見出し名での逆引き」を追加済み

### 判断

- 実装着手: **可**（人間の裁定と承認を得た）

### 裁定後の差分に対する追加レビューの要否

`scripts/lib/SnotraTraceInvariants.psm1` が対象ファイル集合へ加わったため、形式上は
「対象ファイル/シンボルが変わった高リスク計画」に当たる。**追加の `/plan-review` は実行しない**——
この追加は**独立導出が自ら導出して挙げた項目**（成果物の導出表と軽微 5）であり、変化の向きが
レビューの結論**へ**近づく方向だからである。同じ独立導出を再実行しても同じ表を返す。
`.claude/rules/safety-nets.md` の追記は元の計画に含まれており、集合は変わっていない。

## 人間レビュー

- [x] 承認済み — 2026-08-11 / 問い: "この計画で実装へ渡してよいですか？" / 回答: "承認する"
  - 問い: "`scripts/lib/SnotraTraceInvariants.psm1` の H7 コメントへ再現手順 3 行を足しますか？"
    / 回答: "足す（推奨）"
  - 問い: "`.claude/rules/safety-nets.md` へ「発火しないときも同じである」の 1 文を足してよいですか？"
    / 回答: "足してよい（推奨）"
