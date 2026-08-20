# plan — #1154 観測結果の残し方を決め、正本化の逆行に検知器を置く

ブランチ: `chore/measurement-canon-and-fold-detector` / 調査は `workspace/research.md`

## ユーザー裁定（2026-08-20・実装の前提）

| 論点 | 裁定 |
|---|---|
| A-1 出所欄 | **新規のみ・遡及しない**（既存 40 件は触らない。3 月の測定は機体を知る手段が無く、埋めれば捏造になる） |
| A-2 `PERFORMANCE.md` の分割 | **分けない**（既に時系列の採否ログ。「今も支えている値」は #1128 の形＝コードの doc を正本にする、で表す。`governanceDocs` へも足さない） |
| A-3 コメント規範 + B 検知器 | **折れを赤にする検知器を新設**（`HEADING_REF` の行跨ぎ対応＝吸収案は却下） |
| A-4 着地の規範 | **する。引き金は「計装の撤去」** |
| 規範の置き場 | **面ごとに分ける** — A-1 は `PERFORMANCE.md` 冒頭、A-4 は `AGENTS.md`「条件別チェック（トリガー → 参照先）」の既存行へ 1 行 |

## 目的

1. **観測結果の残し方を規範として定める**（A）——出所欄・着地の引き金・コメントの折らない条項。
2. **正準形の参照が物理改行で折れて母集団から落ちる死角を塞ぐ**（B）——検査を 1 本新設し、**今日すでに折れている 33 件**（形 A 20 + 形 B 13）を解消する。

## 受け入れ条件

- [ ] `npm run governance:check` の evidence 行で **見出し参照が 285 → 318 件**になり、**新検査の `checked` も同じ行に現れる**
- [ ] 新検査 `G-folded-heading-refs` がリポジトリ実走査で **0 件**（初回走査時の 33 件は本 PR で解消済み）
- [ ] **形 A と形 B の両方**に fixture の正負テストが在る
- [ ] `npm test` が緑（新検査の unit test を含む）
- [ ] **フォールトインジェクション**: 複製 worktree で 1 件を再び折ると `governance:check` が **exit 1**。稼働中のガードは 1 バイトも触らない
- [ ] `PERFORMANCE.md` に「この文書へ記録するときの規約」節が在り、出所欄を必須にし、**「新規に書く記録に限る・既存記述へ遡及しない」の適用境界を持ち**、理由は `docs/comment-guidelines.md`「歴史メモの様式」を正準形で指している
- [ ] `AGENTS.md`「条件別チェック（トリガー → 参照先）」の足場の行が、**トリガー列で撤去と計装を引き**、参照先列に着地の 1 行を持つ
- [ ] `docs/comment-guidelines.md`「日本語の折返し」に、正準形の指しを折らない条項が入っている
- [ ] `.claude/rules/governance-docs.md` の射程宣言が「1 物理行に収まったものだけ」を含み、**新しい全称を作っていない**
- [ ] 却下した案が ADR に残っている — B 側: 1' 吸収 / 2 ratchet / 3 同一数値検知 / `ANCHOR_SPECS` 拡張 / `path_query_cost.rs:265` の直し方 2 案。A 側: `PERFORMANCE.md` 分割 / 遡及補完
- [ ] **`ADR-folded-canonical-reference-detector` が、検知器を置く理由に答えている** — (i)「壊れたとき緑が緑のまま推移するか」への実測（#1128 の 285 → 284・exit 0 のまま）、(ii) `ADR-governance-meta-demotion`「格下げの線 — 「メタかどうか」ではなく「止めたとき 21 本の合否が信用できなくなるか」」に照らして**母集団の欠落検知の側**に落ちること、(iii) `ADR-comment-guideline-delivery-by-pointer`「却下 3: 行またぎコードスパンの検知器を `governance:check` へ新設する」の**却下理由が陳腐化した経緯**
- [ ] **`ADR-measurement-record-provenance` が受容する残余を持つ** — `PERFORMANCE.md` を `governanceDocs` へ足さないこと（issue が名指した穴 3 は開いたまま）、「今も支えている値」はコードの doc を正本にする形（#1128）で表すこと、面積計器の対象外であること
- [ ] **死角一覧の正本が検査ヘッダに在る**（`G-rules-script-coverage.mjs` の「宣言する死角」が先例）

## 変更ファイル一覧と対象シンボル

### 新規

| パス | 内容 |
|---|---|
| `scripts/governance/checks/G-folded-heading-refs.mjs` | `id` / `run(snapshot, ctx)` / `scanFoldedHeadingRefs(snapshot, docs)` → `{findings, checked}` |
| `scripts/governance/checks/G-folded-heading-refs.test.mjs` | fixture ベースの正負テスト |
| `docs/adr/ADR-folded-canonical-reference-detector.md` | B の裁定と却下 3 案 |
| `docs/adr/ADR-measurement-record-provenance.md` | A の裁定と却下案（`PERFORMANCE.md` 分割・遡及補完） |

### 変更（規範）

| パス | 内容 |
|---|---|
| `PERFORMANCE.md` | 「この文書へ記録するときの規約」節。`:1050` の既存の先例（「開発機は 2 台あるので機体を書く」）を規範へ昇格。**適用境界「新規に書く記録に限る。既存記述へ遡及しない」を節本文へ明記する**（無いと書いた瞬間に既存 36 件を違反にする偽の全称になる・監査 2 枠が独立に指摘）。**理由文は書かない**——`docs/comment-guidelines.md`「歴史メモの様式」の「実測値には条件を添える…条件のない数値は再検証できない」が先着かつ一般であり、そこを正本として**正準形で指す**（`PERFORMANCE.md` は `headingRefDocs` に入るので、この参照は機構が検算する唯一の綱である） |
| `AGENTS.md` | 「条件別チェック」の足場の行。**トリガー列（左）も広げる**——現行は「一時的な足場（script・workflow・env フック）を**新設**」で、A-4 の引き金である**撤去**では引かれない。計装（製品コード内の区間計器）も括弧書きの母集団に入っていない。参照先列（右）へ「計装を撤去する前に、引用されうる値を出所つきで着地させる」を足すだけでは、**規範は在るのに読まれない状態が確定する** |
| `docs/comment-guidelines.md` | 「日本語の折返し」へ正準形の指しの条項（+ 新検査が捕まえることを明記） |
| `.claude/rules/governance-docs.md` | 「G-heading-refs が見るのは正準形だけである」へ「かつ 1 物理行に収まったものだけである」。**「折れは G-folded-heading-refs が捕まえる」とは書かない**——死角 3 件（バッククォート跨ぎ・助詞近傍形・`refScanLines` 未供給）が偽にする新しい全称になる（監査 N2）。射程の正本は検査ヘッダに置き、ここからは指す |

### 変更（evidence 行 — 新検査の `checked` を印字するため）

**evidence 行は自動導出ではなく手書きのテンプレートである**（`scripts/governance/evidence.mjs:106`
で実装を確認）。足さなければ新検査の `checked` はどこにも現れず、`{findings, checked}` を返す
目的（#497「検出は exit code、出力は証拠」——「照合していない」と「差分ゼロ」を分ける）が果たせない。

| パス | 内容 |
|---|---|
| `scripts/governance/evidence.mjs` | `assembleEvidence` のテンプレートへ「折れた見出し参照の候補 N 件」を足す |
| `scripts/governance-check.mjs` | `evidenceView` へ渡す袋へ新しいキーを供給する（`runAll` の 192〜208 行付近） |

**片側だけ変更したときに止めるのは `governance:check` ではなく `npm test` である**（2026-08-20・逆向き監査 R1 を採用し実測で確認）。初稿は「`evidenceView` が finding を出すので赤で止まる」と書いていたが、**その finding の push 先は `metaFindings` であり**（`governance-check.mjs:205`「供給断の検知は `metaFindings` へ受ける」）、#1150 の格下げ以降、既定モードでは `findings` へ合流しない（`:210` が `metaAuditEnabled()` を条件にする）。CI は `SNOTRA_GOV_META_AUDIT` を立てていない。実際に捕まえるのは `scripts/governance-check.test.mjs:121` の `expect(evidence).not.toContain("?")` である。**ゆえに Phase 1-7 の `npm test` は省けない。**
| `scripts/governance/evidence.test.mjs` ほかカナリア | 既存テストが件数や文字列を固定していれば追随する（Phase 1-2 で `npm test` が示す） |

### 変更（折れの解消・33 件）

`workspace/research.md`「結果」の一覧が母集団（形 A 20 件 + 形 B 13 件 = 33 件）。32 件は**改行位置の移動のみ**、1 件は参照の書き換え。

**直し方は「参照を含む文を結合する」である**（2026-08-20 ユーザー裁定）。折れている 2 行を繋ぐ。
行は形 A が最長 157 字・中央値 98 字、形 B が最長 139 字・中央値 96 字になる（scratchpad `joined-len.mjs` の実測）。

> **訂正（2026-08-20・逆向き監査 R3 を採用）**: 初稿は「改行位置を参照の手前へ移す」を既定とし、
> 根拠に「`.rs` の既存コメント 12,302 本の p99 は 94 字だから 157 字は長すぎる」を挙げていた。**両方とも誤りである。**
> (a) 改行位置の移動は**改行を残したまま位置を動かす**操作であり、`docs/comment-guidelines.md`
> 「日本語の折返し」の主条項「文途中で物理改行を入れない（1 段落 1 行）」に正面から反する。
> 「触るのは参照 1 本であって段落ではない」という初稿の限定は**適用範囲の限定であって、新しく置く改行を正当化しない**。
> (b) p99 = 94 字という分布は**条項がやめろと言っている手折りの産物**であり、違反状態を基準として固定していた。
> 条項が求める姿は 1 段落 1 行＝もっと長い行であって、157 字は外れ値ですらない。
> **結合は改行を「減らす」操作なので条項に違反しない。**

**受容する残余**: 結合するのは参照を含む文だけで、**段落の他の行の手折りは残る**。条項の適用範囲
（「新規に書くコメントと、その変更で触った段落」）を厳密に読めば段落ごと 1 行へ畳むのが完全準拠だが、
issue の射程（参照）を超えるので採らない。**`.rs` コメントの手折り慣習そのものは本 PR では直さない。**

**形 A（20 件）**

- `.rs` 11: `snotra-core/src/index_tree.rs:62` / `snotra-core/src/search/build.rs:192,462` / `snotra-core/src/search/path_store.rs:11` / `snotra-core/tests/path_query_cost.rs:3,10,265` / `src-tauri/src/egui_shell/mod.rs:92` / `src-tauri/src/egui_shell/results_view.rs:76` / `src-tauri/src/egui_shell/window_coordinator.rs:190` / `src-tauri/src/working_set.rs:5`
- `.mjs` 4: `.claude/hooks/lsp-config.mjs:8` / `scripts/governance/checks/G-near-heading-refs.mjs:39` / `scripts/governance-check.mjs:122` / `scripts/plan-review-ledger.test.mjs:2`
- `.ps1` / `.psm1` 3: `scripts/lib/SnotraTraceInvariants.psm1:8` / `scripts/lib/SnotraWindowColors.psm1:17` / `scripts/manual-smoke.ps1:63`
- `.md` 2: `PERFORMANCE.md:1530` / `docs/design/2026-05-31-coherence-staleset.md:17`

**このうち 2 件は機構自身が折れている** — `scripts/governance/checks/G-near-heading-refs.mjs:39` と
`scripts/governance-check.mjs:122`。**新設する検査のヘッダで同じ折れをしないこと**（1-2 の
プレースホルダ規律と合わせて、この検査は自分に 2 通りで当たりうる）。

**形 B（13 件・全件 `.rs`）**

`snotra-core/src/indexer.rs:60,479,1030` / `snotra-core/src/index_tree.rs:148,181` /
`snotra-core/src/search/build.rs:77` / `snotra-core/src/search/footprint.rs:206` /
`snotra-core/src/str_arena.rs:5` / `src-tauri/src/egui_shell/launcher_controller.rs:1828` /
`src-tauri/src/egui_shell/layout.rs:403` / `src-tauri/src/egui_shell/results_window.rs:111` /
`src-tauri/src/main.rs:614` / `src-tauri/src/state.rs:37`

**1 件だけ扱いが違う** — `snotra-core/tests/path_query_cost.rs:265` の
`docs/development-principles.md`「判定を持たない道具を層に数えてよい」は、指し先が
**段落先頭の太字**で `ANCHOR_SPECS` のアンカーではない。**参照側を直す**（2026-08-20 ユーザー裁定）——
包含する ATX 見出し `docs/development-principles.md`「検証の層と、層と層の隙間」を指し、
具体的な言明は `「」` の外へ散文で置く。これは `ADR-canonical-heading-references`
「検討した代替案と却下理由」が既に採った解き方である（本文の言明を着地させるために
SPEC へ太字リードを足す案・記法を 2 系統に分ける案をどちらも却下し、参照側の記法を直した）。
**却下した 2 案**: 指し先を箱条書き化する（規範文書の文章構造を検査器の都合で変える）/
参照を散文化する（機構の射程から外れ、折れで隠れていた状態と実質同じになる）。

**`ANCHOR_SPECS` へ「段落先頭の太字」を足す案は採らない** — 実測で `.md` 196 本のアンカーが
5,492 → 7,375（**+34.3%**）になり、増えるのはほぼ全部が節のリードではない強調文である。
アンカーが薄まると、前方一致で**無関係な太字文へ着地して緑になる**側（沈黙）へ倒れる。

## 検知器の設計

```
id: G-folded-heading-refs
母集団: ctx.allRefDocs（G-heading-refs と同一。新しい母集団定義を作らない）

形 A — 対象綴りで行が終わり、ラベルが次行から始まる
  ① 行末が `<対象>`（+ 任意の `§ N`）で終わり、isRefTargetSpelling(<対象>) が真
  ② 行番号 i+1 が走査集合に在り、行頭の記号（コメント・引用・箇条書き）を落とした後 `「` で始まる

形 B — ラベル本文が次行へ流れる（次行を見ない）
  ③ 同一行に `<対象>`（+ 任意の `§ N`）「 が在り、isRefTargetSpelling(<対象>) が真
  ④ その `「` に対応する `」` が同じ行に無い

→ finding「正準形の参照が物理改行で折れている（G-heading-refs の母集団から落ちる）」
checked: ①または③ を満たした位置の数
```

**形 B が次行を見ないのは意図である** — 対象綴りの直後に開いた `「` がその行で閉じていない時点で
折れであり、次行が何であっても `HEADING_REF` / `NEAR_REF` は一致を生成しない。この形なら
**ラベルが 3 行以上に割れた場合も同じ述語で捕まる**（今日は 0 件）。

**検査ヘッダに置く宣言（射程の正本）**: 「**射程はファイル種別に依らない**——正準形の指しは
どの言語のコメントでも機構の入力だからである」。33 件のうち **9 件が `.rs` 以外**（`.mjs` 4 /
`.ps1`・`.psm1` 3 / `.md` 2）である一方、`docs/comment-guidelines.md` の配送は 4 crate の
`**/*.rs` のみで、**`.ps1` / `.psm1` にはコメント作法の生きた規範が一つも無い**（規範枠が
`AGENTS.md` / ルート `CLAUDE.md` / `docs/comment-guidelines.md` / `.claude/rules/*.md` を
交差検索して 0 行と実測）。**規範が一言も無い面で機構だけが赤を出すので、この 1 文が赤を
受け取った者の唯一の拠り所になる。**

**「必要な分だけ縛る」ため、次は見ない（死角として宣言する）。**

- **バッククォート自体が行を跨ぐ形**（`` `docs/develop- `` / `` ment.md`「…」 ``）— 今日 0 件。奇数バッククォートの行 22 本を目視したが、いずれも `` ` `` の単独出現（`find('`')` 等）か 4 連フェンス
- **助詞を挟んで折れた形**（`G-near-heading-refs` の対象が折れたもの）
- **`refScanLines` が i+1 を供給しない形**（フェンス内・非コメント行に落ちた次行）——形 A のみ・fail-open。形 B はこの死角を持たない

## PR 本文へ送る項目（計画の作業項目ではない）

**CI の `governance manifest delta` job は PR 本文に `+G-folded-heading-refs` の逐語を要求する。**
`.github/workflows/ci.yml` が `PR_BODY` を環境変数で渡し、`scripts/governance-manifest.mjs:59`
`undeclared` が「本文に逐語で現れるか」だけを見る（書式は強制しない・`:84` が要求文言を印字する）。
**リポジトリを grep しても出てこない要求であり、独立導出が拾わなければ落としていた。**

**この節はチェックボックスを持たない。** `.claude/hooks/pre-bash.mjs` の `countUnchecked` は
`/^\s*[-*]\s+\[ \]/gm` を `plan.md` **全文**に当て、見出しを一切見ない（監査で実装を確認）。
「PR が無いと完了できない項目」をチェックボックスで置くと、**`gh pr create` を自分で塞ぐ**。

1. PR 本文へ `## governance manifest delta` と `+G-folded-heading-refs` を書く。
   新規 ADR 2 本は `docs/adr/` ゆえ `governanceDocs` の対象外で `docs` 列の delta にならない（`lib.mjs:461` の除外を確認済み）
2. **PR 本文のチェックリストへ「rust-check（windows）の `npm test` が緑」を置く。**
   `.gitattributes` が `eol=lf` を強制するのは `.githooks/**` だけで、**windows runner は
   `core.autocrlf=true` で CRLF チェックアウトする**（ローカルは `core.autocrlf=input` で LF）。
   `linesOutsideFences` / `linesOfComments` は `split("\n")` ゆえ `\r` を行末に残すので、
   新検査の述語が `\r` に敏感だと**ローカル緑・windows の `npm test` だけ赤**になり、手元で再現できない。
   `scripts/governance-check.test.mjs` の実リポジトリ スモークは findings が `[]` であることを要求し、
   これは rust-check（windows）でも走る
3. 手順: 本文を先に Write でファイルへ書き、`git push -u origin HEAD && gh pr create --body-file <path>` の
   `&&` 鎖で 1 コマンドにする。**鎖に `cd` を含めない**（対象リポジトリを判定できず拒否される）

## 実装順序

### Phase 1 — 検知器と、今日の 20 件の解消

- [x] 1-0. **ADR 2 本を先に書く**（`ADR-folded-canonical-reference-detector` / `ADR-measurement-record-provenance`）。**後ろに置けない**——`G-adr-citations` の母集団 `nonDocSources`（`.rs`/`.mjs`・`docs/` の外・`.test.mjs` を除く）に新検査ファイルが入り、`.claude/rules/governance-docs.md` は `governanceDocs` 経由で入る。**実在しない ADR を引いた時点で 1-6 / 2-5 が赤になる**（非 md の腕はフェンスのマスクを掛けず行全体を走査する）。フォールトインジェクションの実測値は 3-5 で追記する
- [x] 1-1. `G-folded-heading-refs.test.mjs` を fixture で書き、**落ちることを確認する**（Red）。fixture が固定するもの: 形 A の正例・形 B の正例・1 行に収まった参照（負例）・行末が対象綴りだが次行が箇条書き（負例）・行頭記号の落としすぎ／落とし足りない両方向・**blockquote `>` の剥がし**（33 件のうち `docs/design/2026-05-31-coherence-staleset.md:17` だけがこの形）・**CRLF**（`split("\n")` が `\r` を残すため）
- [x] 1-2. `G-folded-heading-refs.mjs` を実装し unit を通す（Green）。**この検査自身のコメントとテストの fixture は、`isRefTargetSpelling` に当たらないプレースホルダで書く**——`scripts/governance/checks/` は `allRefDocs` のコメントの腕に入っており、実在の形の対象を例示すると**検査が自分自身を赤にする**（先例: `G-near-heading-refs.mjs` の「例示に実在の対象を置かない」、`ADR-canonical-heading-references`「検討した代替案と却下理由」の `.mjs` 拡張の却下）
- [x] 1-2b. `evidence.mjs` のテンプレートと `governance-check.mjs` の袋へ新しいキーを足す。**供給する値は平坦な数にする——配列を渡して `.length` をテンプレート側で取ってはならない**（逆向き監査 M4b）。配列にすると**供給が断たれても「1 件」と印字して exit 0** になり、`?` canary も素通りする（`evidenceView` のガードは 1 段目の読みしか見ない。同型の穴が `evidence.mjs:41-44` / `governance-check.mjs:186-189` の `.length` に今も残っている）
- [x] 1-3. **リポジトリ実走査を行い、findings 全件を本ファイルへ貼る**（Red の実物。33 件のはず。**数が合わなければ実走査を正とし、`research.md` の一覧を差し替える**）
- [x] 1-4. 32 件を**参照を含む文の結合**で解消する（改行位置の移動ではない——上の訂正を参照）
- [x] 1-5. `path_query_cost.rs:265` を参照の書き換えで解消する
- [x] 1-6. `npm run governance:check` で **見出し参照 318 件 / 新検査 0 件**を確認する。**evidence 行は 1-4 の前後で全文を 2 本貼る**——`headingRefs` だけを見ると、33 件は「新しく生きた組み合わせ」ゆえ他の列が同時に動いていても気づけない（逆向き監査 M5）
- [x] 1-7. `npm test` と `cargo doc`（intra-doc link）を実行する。**`npm test` は省けない**——evidence の片側変更を捕まえる唯一の層である（上の訂正）。`.rs` は 24 箇所 / **16 ファイル**（`index_tree.rs` や `indexer.rs` のように 1 ファイルに複数箇所あるため、箇所数とファイル数は違う）
- [x] 1-8. コミット

### Phase 2 — 規範 4 面

- [x] 2-1. `PERFORMANCE.md` へ記録規約節（A-1・A-2 の「分けない」理由も 1 文で）。**置き場は「冒頭の無見出し散文の直後・`## ビルドプロファイル最適化の知見` の直前」**——H1 の直後へ挿すと既存の導入散文（着手の順序の 4 段）が新節へ流れ込む。既存散文へ見出しを与える構造変更は裁定に無いので行わない
- [x] 2-2. `AGENTS.md` の足場の行へ A-4 を 1 行
- [x] 2-3. `docs/comment-guidelines.md`「日本語の折返し」へ正準形の条項
- [x] 2-4. `.claude/rules/governance-docs.md` の射程宣言を新検査に合わせる
- [x] 2-5. `npm run governance:check` 緑を確認しコミット

### Phase 3 — フォールトインジェクションと ADR

- [x] 3-1. 複製を作る。**置き場は `.claude/worktrees/` 配下かリポジトリ外**——`WALK_EXCLUDE_PATHS` はルート錨止めの完全一致なので、リポジトリ直下へ別名で置くと**稼働中ツリーの `governance:check` がその中まで歩き**、見出し参照が二重に数えられて 1-6 / 2-5 / 3-6 の件数確認が壊れる。`npm test` を走らせる枠だけ `node_modules` を junction で張る（`governance:check` 単体には `node_modules` は要らない）
- [x] 3-2. **注入前に複製で exit 0 を確認**してから、解消した 1 件を**再び折る**変異を当て exit 1 を実測する（形 A と形 B を 1 件ずつ）。**2 点で書く**——「exit 1 になった」だけでは複製の作成手順が壊れていないことと区別できない
- [x] 3-2b. **継続行の剥がし写像を外す変異**を当て、検知件数が落ちることを実測する（独立導出は剥がしを外すと 33 → 15 件に落ちると測った）——**剥がしは判定の中核であって装飾ではない**ことを、この変異が示す
- [ ] 3-3. 逆向き（既存 20 本のゲートを弱めていないか）を 1 件測る。**`npm test` を使う場合は junction が要る**——張らないと vitest は**起動前に**モジュール解決で exit 1 を返し、**変異を検知した場合と終了コードが同じ**になる。モジュール解決の失敗を見たら**どのモジュールが解決できなかったかを必ず読む**（vitest 本体なら測定環境の欠陥、変異先なら検知）
- [x] 3-4. 複製を撤去し、HEAD 一致と、作業ツリーの変更が本 PR の対象に限ることを確認する
- [ ] 3-5. ADR 2 本（1-0 で作成済み）へ**フォールトインジェクションの実測値**を追記する
- [ ] 3-6. `npm run governance:check` 緑を確認しコミット

## 不変条件と異常系

- **`HEADING_REF` の意味論を変えない。** 3 消費者（`G-heading-refs` / `G-near-heading-refs` の `ADJACENT_REF` / `dependents.mjs`）はすべて行単位のまま。新検査は入力の欠落を**赤で告げる**だけで、母集団を広げない
- **新しい母集団定義を作らない。** `ctx.allRefDocs` をそのまま使う（`ADR-governance-corpus-reduction-rejected` の実測——検査の改修費用 21 件中 17 件が母集団定義の変更由来）
- **稼働中のガードを弱めない。** 変異は複製 worktree に当てる（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）
- **`docs/adr/` の既存 ADR は編集しない**（凍結・`ADR-adr-frozen-history`）。新規 2 本の追加のみ
- **既存の測定値を測り直さない**（issue の射程外。`40〜95 ms` の再計測は `ADR-measurement-canon-in-code-doc` が別に残している）
- **`docs/development-principles.md`「検証の層と、層と層の隙間」の表に行を足さない。** `AGENTS.md`「条件別チェック（トリガー → 参照先）」の「検査・検証手段を新設する」トリガーが名指す表だが、**新検査は層ではなく既存の層（`governance:check`）の穴を塞ぐもの**であり、既存行「文書の整合 / `governance:check` / 参照・索引・命名の着地 / 意味の側」がそのまま当たる。むしろこの折れは、同節が言う**「穴は層の内側ではなく境界に空く」の実例**である——`G-heading-refs` は自分の担当（着地の照合）を保証しているが、**その入力が母集団へ届いているか**は誰も見ていなかった。この読みは ADR へ記録する（Phase 3-5）
- **異常系**: 行が長くなる。`rustfmt.toml` は不在で `wrap_comments` は既定 false ゆえ fmt は折り返さない。**改行位置の移動を既定にしたので行長は増えない**——結合を選んだ行だけ、結合後の文字数を Phase 1-4 で記録する（比較の基準: `.rs` コメント 12,302 本の p50 56 / p90 76 / p99 94 / max 551 字、120 字超が 71 本）

## テスト方針と検証コマンド

| 対象 | 手段 |
|---|---|
| 新検査の正負 | `G-folded-heading-refs.test.mjs`（fixture。正: 折れた参照 / 負: 1 行に収まった参照・行末が対象綴りだが次行が箇条書き） |
| リポジトリ実走査 | `npm run governance:check`（evidence 行の件数で確認） |
| 既存の退行 | `npm test` |
| intra-doc link | `cargo doc`（`.rs` の doc コメントを触るため。PostToolUse は発火しない・`docs/build-commands.md`） |
| 検知器が効くこと | 複製 worktree での変異注入（Phase 3） |

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** 製品の挙動・状態遷移を 1 つも変えない（`.rs` の変更はコメントの行の結合のみ）
- **`docs/architecture.md`: 不要。** 横断パターンを変えない
- **`docs/build-commands.md`: 不要。** 検査の一覧は `checks/` の走査が SSOT で、手書きの範囲が存在しない（#1088）
- **`RETROSPECTIVE.md`: サイクル末の `/retrospective` に委ねる**

## 未確定（実装前に潰す）

- [x] **ADR を 2 本に分けるか 1 本に束ねるか** — **2 本**（2026-08-20 ユーザー裁定）。`ADR-measurement-record-provenance` と `ADR-folded-canonical-reference-detector`
- [x] **`G-folded-heading-refs` という id でよいか** — よい。命名規則は `G-<name>` で連番禁止のみが制約（`.claude/rules/governance-docs.md`「名前はテーマ・目的が決まった時点で、何を指すか分かる形で付ける。」）。既存の `G-heading-refs` / `G-near-heading-refs` と並ぶ
- [x] **行頭記号を落とす正規表現の形** — **形 A のみが必要**（形 B は次行を見ないので不要）。調査で使った `/^\s*(?:\/\/[!/]?|#|\*|>|-)*\s*/` を出発点にし、落としすぎ（`-` が本文の冒頭にある行）と落とし足りない（`///` `//!` `#`）の両方向を fixture で固定する。**誤りの向きは赤側**（落とし足りなければ折れを見逃す＝沈黙）なので、fixture で塞ぐ
- [x] **20 件という数が敵対レビューで動くか** — **動いた。33 件（形 A 20 + 形 B 13）へ訂正済み。** 形 B は敵対レビューの指摘を受けて自分で独立に測り直した（`fold-shape-b.mjs`。全件着地・全件 `.rs`）
- [x] **結合後の最長行の文字数** — 実測（2026-08-20）: 素朴な結合で最長 157 字・中央値 98 字。`.rs` の既存コメントは p99 94 字ゆえ超過する。**解決: 直し方を「改行位置の移動」へ変えた**ので行長は増えない

## plan-review 結果

- **リスク: 高**（セーフティネットの新設 + ガバナンス文書の変更 + 網羅性が要件）
- **レビュー方式: 独立導出 1 体**（Step 2b。`--deep`。`workspace/` / `.superpowers/` / `.claude/worktrees/` を読ませず、走査範囲の除外まで指定した）
- **エージェント数: 2**（3b の敵対レビュー 1 + 独立導出 1）
- 報告書: `workspace/plan-review-fold-detector.md`

### 収束の実測 — 独立導出は 33 件で全件一致した

導出側は**別の述語**（連続する走査行の極大 run をブロック連結し、繋いだ後にだけ現れる `HEADING_REF` の
一致のうち行境界を跨ぐものを折れとする）で数え、**33 件・全件のファイル:行が一致した**。
走査文書 258 / 連続行ブロック 961 / 隣接行対 63,800 / 偽陽性 0 / 跨いだ行数はすべて 2 行。
形の分類も一致する（導出側の B = 本計画の形 A で 20 件、導出側の C = 本計画の形 B で 13 件）。
導出側は**バッククォートの内側での折り**（本計画が「今日 0 件」と目視で宣言した形）を
**述語で 0 件と測り**、0 である理由まで示した——`docs/comment-guidelines.md`「日本語の折返し」の
「コードスパンを行またぎさせない」が先に効いている。**本計画の死角宣言のうち 1 つは、目視から実測へ格上げされた。**

### 要対処（計画へ反映済み）

- **PR 本文の manifest delta 宣言** — 計画から漏れていた。`ci.yml` が `PR_BODY` を渡し `governance-manifest.mjs:59` `undeclared` が逐語照合する。**リポジトリの grep には現れない要求**。「PR 本文へ送る項目」節を新設して反映（作業項目には置かない——コミット以降の境界）
- **継続行の剥がしを外す変異** — Phase 3-2b を追加。導出側は剥がしを外すと 33 → 15 件へ落ちると測っており、**剥がしが判定の中核である**ことを変異で示せる
- **機構自身が 2 件折れている** — 変更対象一覧へ明示し、新検査のヘッダで同じ折れをしない規律を 1-2 へ足した
- **`PERFORMANCE.md` の A-1 節の置き場が構造変更を含みうる** — H1 直後へ挿すと既存の導入散文が新節へ流れ込む。**冒頭散文の直後・最初の `##` の直前**へ置くと決めた（既存散文へ見出しを与える構造変更は裁定に無いので行わない）
- **blockquote と CRLF の fixture** — 1-1 へ追加。`docs/design/2026-05-31-coherence-staleset.md:17` だけが `>` の形で、`split("\n")` は `\r` を残す
- **`path_query_cost.rs:265` の直し方を独断で選ぶな** — 導出側の指摘。**ユーザーへ問う**（下記）
- **`.claude/rules/governance-docs.md:18` の死角一覧の更新** — 既に 2-4 に在る

### 軽微

- **`docs/build-commands.md:163` の括弧内列挙へは足さない**（導出側の推奨に同意）。同行は既に「参照実在・モジュール索引・…・見出し参照の着地」と列挙しており、検査を足すたびに書き足す面である。**検査の一覧は `checks/` の走査が SSOT** と同じ段落の下の bullet が宣言しているので、括弧内は概念の列挙にとどめる
- **導出側は「新しい ADR を切らず、却下 3 案は検査ヘッダで足りる」と推した**（先例: `G-near-heading-refs.mjs` のヘッダ）。**採らない——ユーザーが ADR 2 本を明示的に裁定している。** ただし導出側はこの裁定を知らされていなかったので、指摘としては筋が通っている。A 側の却下（`PERFORMANCE.md` の分割・遡及補完）は検査ヘッダに置き場が無く、ADR が要ることは変わらない
- **近傍形（助詞つき）の折れは射程外** — 既に死角として宣言済み

### 未検証

- **偽陽性の将来率** — 今日 0 件しか測っていない。連続する箇条書き・表セル・引用の折返しが候補（導出側の指摘）。**「必要な分だけ縛る」の判断は今日の 0 件で行い、将来の率は測れないものとして受容する**
- **`npm test` / `governance:check` は新検査を含めては未実行**（実装前ゆえ当然）。ただし現行の 285 件は主エージェントが `npm run governance:check` を実行して生出力で確認済み

### 射程外だが発見した別件の腐り（issue 化の候補）

`scripts/governance/lib.mjs:489` と `scripts/governance/checks/G-module-index.mjs:25` が
`domains.test.mjs` を指しているが、**そのファイルは #1152（`74ae45fc`・2 コミット前）で削除済み**である
（`ls` で不在を確認）。`.mjs` は `isRefTargetSpelling` に当たらないので `G-heading-refs` の視界外にあり、
**折れ検知器でも拾われない**。本 issue の射程外ゆえ触らないが、**機構自身の doc が 2 コミット前から偽である。**

### 判断

- **実装着手: 人間の裁定待ち**（下記 1 点 + 承認）

## 多観点レビュー（4 枠・2026-08-20）

ユーザーの依頼で、**枠組みと道具を分けた 4 体**を並列に走らせた。報告書は `workspace/plan-audit-{trace,reverse,exec,norm}.txt`。

| 枠 | 観点 | 固有の道具 | 要対処 |
|---|---|---|---|
| `audit-trace` | issue の要求 → 計画への写像（前向き） | `AGENTS.md` 条件別チェック表の総当たり | 6 |
| `audit-reverse` | **逆向き**——実行したとき何が黙って壊れるか | **`git log -S` / `blame` / `git show`**（この枠だけ） | 3 |
| `audit-exec` | 手順どおり手を動かしたらどこで詰まるか | 検証コマンドの実行・複製での再現 | 6 |
| `audit-norm` | 足す規範が既存規範と衝突・重複しないか | 面積計器の実測・rules の frontmatter 全件 | 5 |

### 計画の前提を壊した所見（採用・本文へ反映済み）

- **直し方が、引用している規範に違反していた**（`audit-reverse` R3）。「改行位置を移す」は**改行を残したまま位置を動かす**操作で、`docs/comment-guidelines.md`「日本語の折返し」の主条項に反する。さらに行長の根拠にした p99 = 94 字は**条項がやめろと言っている手折りの産物**だった。**ユーザー裁定で「参照を含む文の結合」へ変更**（結合は改行を減らす操作なので違反しない）
- **evidence の片側変更を止めるのは `governance:check` ではない**（`audit-reverse` R1・`audit-exec` 要対処 1 が独立に到達）。`evidenceView` の finding は `metaFindings` へ入り、#1150 の格下げ以降は既定モードで `findings` へ合流しない。実際に止めるのは `governance-check.test.mjs:121` の `npm test` である
- **`AGENTS.md:63` のトリガー列は「新設」であり、A-4 の引き金（撤去）で引かれない**（`audit-trace` 要対処 4・`audit-norm` N1 が独立に到達）。参照先列だけ足すと**規範は在るのに読まれない**
- **ADR を後ろに置けない**（`audit-exec` 要対処 3）。新検査ファイルは `nonDocSources` に入るので、実在しない ADR を引いた時点で赤になる。**1-0 へ前倒しした**
- **「PR 本文へ送る項目」のチェックボックスが `gh pr create` を塞ぐ**（`audit-exec` 要対処 4）。`countUnchecked` は見出しを見ない。**チェックボックスを外した**
- **windows runner は CRLF チェックアウトする**（`audit-exec` 要対処 5）。ローカルで再現できない赤の経路であり、PR 本文のチェックリストへ送った
- **A-1 の規約文が射程なしだと偽の全称になる**（`audit-trace` 要対処 2・`audit-norm` N5 が独立に到達）。日付つき 40 件のうち機体名を持つのは 4 件で、**書いた瞬間に既存 36 件を違反にする**
- **A-1 の理由文だけが写しである**（`audit-norm` N4）。正本を `docs/comment-guidelines.md`「歴史メモの様式」に置き `PERFORMANCE.md` から正準形で指す——`PERFORMANCE.md` は `headingRefDocs` に入るので**この参照だけは機構が検算する**
- **2-4 の括弧書きが新しい全称になる**（`audit-norm` N2）。死角 3 件が偽にする
- **フォールトインジェクション手順に 4 つの欠け**（`audit-exec` 要対処 6）。複製の置き場・junction の要否・同一 exit code の罠・注入前の基準

### 計画の前提を強めた所見

- **折れ 33 件に「意図的に折った」形跡は 1 件も無い**（`audit-reverse` M1・blame と `log -S` で 9 件サンプリング）。全件、その行を生んだコミット 1 本で最初から折れて生まれている。**原因は行長である**——`4c5892a20` は**同一コミットの中で同じ参照を `.md` では 1 行、`.rs` では 2 行に割っている**
- **条項だけでは止まらなかった**。33 件のうち少なくとも 4 件は条項導入（2026-08-08）**より後**に生まれている
- **318 は独立に実測で裏が取れた**（`audit-exec` E2）。複製に対し 33 件を機械的に結合して測っている
- **「1 物理行に収まったものだけである」は真**（`audit-norm` Q3・2 経路で実測）

### 射程外として受け取った所見

- `.ps1` には「既存ファイルの作法（折返しあり）へ合わせる」という先例が在る（`docs/superpowers/plans/2026-08-10-search-worker.md:17`）。ただし `docs/superpowers/` は #589 で非規範化されており**生きた規範ではない**。**結合という直し方なら衝突しない**（段落の手折りは残すため）
- `governance-check.test.mjs:124` の 3 本目の assertion は今日恒真である（`audit-reverse` R2）。#1150 の格下げの残滓で、#1155 と同型の 3 件目。`evidence.mjs:46` の `@param` も同じ理由で偽。**本 PR では触らない**——#1155 へ寄せる
- **新検査の `checked` の供給断だけは R1 の非対称の内側に残る**（`audit-reverse` U3）。新検査そのものは通常のゲート検査だが、その evidence 供給が断たれた場合の検知は `metaFindings` 側に落ちる。**`ADR-folded-canonical-reference-detector` の「受容する残余」で名指す**
- `.ps1` の先例（`docs/superpowers/plans/2026-08-10-search-worker.md:17`）についての規範枠の当初の性格づけ「逆向きの明示的先例」は**本人が取り下げた**。`docs/superpowers/` は #589 で非規範化され `lib.mjs:446` ほか 4 述語が接頭辞で落とす歴史資料であり、生きた規範ではない。**正しい形は「衝突」ではなく「規範の空白」である**

## セルフレビュー

- **リスク: 高**（セーフティネットの新設 + 規範文書の変更。`AGENTS.md`「条件別チェック（トリガー → 参照先）」の当該 2 行に該当）
- **plan-review**: `--deep`（Step 2b・独立導出 1 体）を実施。**当初これを省こうとしたのは誤りだった**——3b が見たのは `research.md` であって計画そのものではなく、計画の検知器設計・フェーズ順序・fixture・規範の置き場は誰の独立検算も受けていなかった。実際に PR 本文の manifest delta 宣言という**リポジトリの grep に現れない要求**を落としていた
- **エージェント数: 2**（3b の敵対レビュー 1 + Step 2b の独立導出 1）
- **要対処: 6 件**（敵対レビュー所見 1〜6）+ **6 件**（独立導出）。全件を裁定し `research.md` と本計画へ反映済み。最重要は敵対所見 1（20 → 33 件・検知器を形 A + 形 B へ）と独立導出の PR 本文要求
- **未検証**: 「バッククォート自体が行を跨ぐ形が今日 0 件」は目視（奇数バッククォート 22 行）であって機械判定ではない。**死角として宣言して止める**（`detector-scope-only-as-tight-as-needed`）

### 主エージェント自身の照合（5 点）

1. **issue の全要件に作業項目が対応する** — A の 4 チェックボックス（出所欄 / 分割 / コメント規範 / 着地）は Phase 2 の 2-1〜2-3 と ADR、B の 3 チェックボックス（緑が緑のまま推移するかの問い / 候補比較 / 必要な分だけ縛る）は Phase 1・3 と「検知器の設計」の死角宣言に対応する
2. **境界条件と検証** — 形 A の次行が走査集合に無い場合・形 B のラベルが 3 行以上に割れる場合・行頭記号の落としすぎ／落とし足りない、の 4 条件すべてに fixture を置く（1-1）
3. **新しい状態・リソース・プロセスの正常/失敗/破棄経路** — 新設するのは純関数の検査 1 本のみ。リソースを持たない。Phase 3 の worktree だけが破棄経路を持ち、3-4 が撤去を明示している
4. **より単純な既存パターンで置き換えられないか** — 候補 1'（`HEADING_REF` の行跨ぎ対応）が「より単純」に見えるが、3 消費者の意味論を同時に変え、既存規範と逆を向く。ユーザー裁定で却下済み
5. **壊してはならない不変条件に検知手段がある** — 「`HEADING_REF` の意味論を変えない」は既存の `npm test`（3 消費者のテスト）が守る。「新検査が本当に効く」は Phase 3 のフォールトインジェクションが測る

## 人間レビュー

- [x] 承認済み — 2026-08-20 / 問い: "`workspace/plan.md` は未確定欄が空、人間レビュー欄が「承認待ち」のままなので、**あなたの承認をいただいた時点で** `workspace/` をコミットして `/implement` へ渡します。" / 回答: "承認"

**承認前に得た裁定（すべてユーザー本人の発言）**: A-1「新規のみ・遡及しない（推奨）」/ A-2「分けない（推奨）」/ A-3+B「折れを赤にする検知器を新設（推奨）」/ A-4「する・計装撤去を引き金にする（推奨）」/ 規範の置き場「面ごとに分ける（推奨）」/ ADR「2 本に分ける（推奨）」/ 腐り 1 件「参照側を見出しへ指し替える（推奨）」/ 直し方「参照を含む文を結合する（推奨）」。

**未提示のまま裁定を求めた事項が 1 つあり、承認前に開示済みである**——`ADR-comment-guideline-delivery-by-pointer`「却下 3: 行またぎコードスパンの検知器を `governance:check` へ新設する」で**同じ族の検知器が過去に却下されている**こと。却下理由（「5 件は `.rs` のコメント 8030 行に対する事故率であり、規範 1 行と現存違反の修正で足りる」）が陳腐化した経緯は `ADR-folded-canonical-reference-detector` の受け入れ条件に入っている。
