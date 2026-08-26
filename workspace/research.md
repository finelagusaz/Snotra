# research — issue #1188: 入れ子の鉤括弧を含む見出しへの正準形参照が照合されない

## issue の要約

`scripts/governance/lib.mjs` の `HEADING_REF` はラベルの文字クラスが `[^「」\n]` であるため、
**見出し名に `「…」` が入れ子で含まれるとき、その見出しを全形で指す正準形参照は一致そのものが
生成されない**。findings にも `checked` にも現れないので、「照合していない」と「差分ゼロ」が
区別できない（#497 が閉じたはずの沈黙経路が、ラベルの形の側に 1 本残っていた）。

採る案は issue が既に確定させている——**`HEADING_REF` を 1 段の入れ子まで受け入れる**（`lib.mjs` の 1 行）。
検知器を新設して書き方を縛るのではなく、死角そのものを消す。

## 一次証拠（自分で測った・作業ツリー不変）

測定は読み取り専用で行った。**稼働中のガードは 1 バイトも触っていない**——`lib.mjs` から
`REF_HEAD` / `isRefTargetSpelling` / `refScanLines` / `collectAnchors` / `normAnchor` /
`resolveRefTarget` / `allHeadingRefDocs` を import し、OLD/NEW 2 つの正規表現で
`(doc, line, target, label, landed)` の 5 つ組を作って集合として比べる別スクリプトを使った
（`workspace/measure-heading-ref-nesting.mjs`。歴史側は `git ls-tree` / `git show` から snapshot を組む）。

NEW の形:

```js
new RegExp(`${REF_HEAD}「((?:[^「」\\n]|「[^「」\\n]*」)+)」`, "g")
```

### 1. 歴史データで「18 日間の沈黙」を再現した（issue の表と完全一致）

| rev | docs | OLD checked | NEW checked | OLD findings | NEW findings | NEW にだけ在る |
|---|---:|---:|---:|---:|---:|---:|
| `6ccacd01` | 187 | 194 | 195 | 2 | 2 | **1** |
| `f1827b08` | 188 | 204 | 206 | 2 | 2 | **2** |
| `ac39cb26` | 266 | 355 | 357 | 0 | 0 | **2** |
| `b93b0fb8^` | 266 | 357 | 359 | 0 | 0 | **2** |
| `b93b0fb8`（#1185 の掃除）| 266 | 361 | 361 | 0 | 0 | 0 |
| `HEAD`（9916cff2）| 266 | 365 | 365 | 0 | 0 | 0 |

- **`OLD にだけ在る` は全 rev で 0 件**——純粋に加算的で、退行が無い
- **findings は OLD/NEW で全 rev 同値**——不可視だった 2 件は `landed=true` で「照合済み」へ上がり、
  **赤を 1 件も出さない**。`6ccacd01` / `f1827b08` の findings 2 は変更と無関係な既存の腐り
- 不可視だった 2 件（`b93b0fb8^`）:
  - `RETROSPECTIVE.md:23` → `docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」
  - `snotra-core/src/indexer.rs:53` → 同上（**製品コードの rustdoc**）
- `lib.mjs` は `b93b0fb8^` から `HEAD` まで**差分ゼロ**（`git diff` で確認）。歴史側の測定に
  現在の `lib.mjs` を当てても、判定は当時の実装と同一である

### 2. 生きたツリーでは A/B が完全一致

`HEAD` で `checked` 365 / findings 0 が OLD/NEW で不動、5 つ組の集合差は両向き 0 件。
**新たに赤くなる参照は 0 件**（移行コストゼロ）。

**「加算的」は今日の実測であって、将来にわたる普遍的性質ではない**（3b の ⚠️1 を採用・自分で再測）。
**ラベルの内側に別の正準形の頭が入った形では、NEW は外側 1 件へ統合し、OLD が拾っていた内側の
一致は消える**:

| 入力 | OLD | NEW |
|---|---|---|
| `` `a.md`「見出し `b.md`「入れ子」だ」 `` | `b.md`\|`入れ子` | `a.md`\|`見出し `b.md`「入れ子」だ` |
| `` `a.md`「A「B」C」と `b.md`「D」 `` | `b.md`\|`D` | `a.md`\|`A「B」C` と `b.md`\|`D` |

前者は **OLD にだけ在る一致が 1 件生まれる**形である。生きた母集団に今日 0 件（3b が独立に確認・
`docs/superpowers/` 配下の 2 件は母集団外）だが、**4 つ目の宣言する死角として書く**——
「他の参照のラベルの内側に置かれた正準形参照は、独立には照合されない」。

### 3. 宣言する死角は今日すべて 0 件（全追跡ファイル 496 本で実測）

`REF_HEAD「` が当たり、かつ NEW でも同一行内に閉じた一致を作れない位置を数え上げた:

- **深さ 2 以上の入れ子: 0 件**（全追跡ファイル。`docs/adr/` と `docs/superpowers/` を含む全域）
- **行内で閉じない: 13 件**——すべて `docs/adr/`・`docs/superpowers/`（生きた母集団の外）と
  `G-folded-heading-refs.test.mjs` の fixture 1 件。**このうち入れ子を含むものは 0 件**
  （＝「入れ子かつ折れ」は 0 件）。折れは `G-folded-heading-refs` の担当であり本件の射程外
- **閉じ不足（行末でも次行でもない形）: 0 件**
- **他の参照のラベルの内側に置かれた正準形参照: 0 件**（上記 ⚠️1 の形）

### 測定スクリプトの射程（3b の ⚠️2 を採用）

`measure-heading-ref-nesting.mjs` は `refScanLines(text, doc, [])` と**空配列を findings のシンクに
渡している**ため、本物の `scanHeadingRefs` と違い**フェンス不整合由来の finding を一切拾わない**。
ゆえに同スクリプトの `findings` は「着地しない参照の件数」であって `checkHeadingRefs` の findings と
同義ではない。**A/B 比較の妥当性は損なわれない**——OLD/NEW は同じ `refScanLines` を通るので、
拾わない分は両側で等しく欠ける。母集団の作り方も違う（本物は `makeSnapshot` が fs を歩き、
測定側は `git ls-tree` の追跡ファイル）。**この違いが無害であることは 3b が実 `npm run governance:check` の
出力（見出し参照 365 件 / 近傍 19 件 / 折れうる位置 20 件）と突き合わせて確認した**——
HEAD の `checked` が 365 で一致する。

### 4. 破滅的バックトラックは起きない

2 つの選択肢は先頭 1 文字で排他（`[^「」]` と `「`）なので曖昧さが無く線形。実測:

| 入力 | OLD | NEW |
|---|---:|---:|
| `` `a.md`「 `` + 「あ」×20000（閉じない） | 0.32 ms | 0.47 ms |
| `` `a.md`「 `` + 「「x」×5000 | 0.06 ms | 0.03 ms |
| `` `a.md`「 `` + 「あ「い」」×5000 | 0.07 ms | 0.27 ms |
| 入れ子 1 件を含む通常行 × 20 万回 | 93.6 ms | 66.5 ms |

**この 29% の改善は再現しなかった**（実装後の独立再測定・3b とは別の枠。warmup を揃え順序を入れ替えると
OLD 65.6〜74.3 ms / NEW 64.2〜71.9 ms で、6 組中 5 組が NEW ≤ OLD）。**93.6 ms は OLD 側が
warmup されていなかった値と見られる**（未検証の仮説）。**結論として書くのは「遅くならない」までである。**
破滅的バックトラックが起きないことは 2 枠が独立に確認した。

## 関連ファイル・シンボル（grep で実在確認済み）

| パス | 役割 | 本件で触るか |
|---|---|---|
| `scripts/governance/lib.mjs:177` | `HEADING_REF` の定義（正本） | **触る**（正規表現 1 行 + doc） |
| `scripts/governance/lib.mjs:168` | `REF_HEAD`（正準形の頭） | **触らない** |
| `scripts/governance/checks/G-heading-refs.mjs:47` | `HEADING_REF` の消費者 1 | 触らない |
| `scripts/governance/dependents.mjs:41` | `HEADING_REF` の消費者 2（逆引き） | 触らない |
| `scripts/governance/checks/G-near-heading-refs.mjs:49` | `ADJACENT_REF` = `REF_HEAD` + `「` | 触らない |
| `scripts/governance/checks/G-folded-heading-refs.mjs:51,54` | `TAIL_TARGET` / `OPEN_UNCLOSED` = `REF_HEAD` 由来 | 触らない |
| `scripts/governance/checks/G-heading-refs.test.mjs` | 検査の fixture | **触る**（入れ子の 3 fixture を足す） |
| `docs/adr/ADR-folded-canonical-reference-detector.md` | 帰結「`HEADING_REF` の意味論は変わらない」 | **触る**（追記 1 節） |
| `docs/adr/ADR-<新規>.md` | 本件の決定と却下 3 案 | **新規** |

### `HEADING_REF` の消費者は 2 つである（実測）

`grep -rn "HEADING_REF" --include=*.mjs`（`.claude/worktrees/` を除く）の結果、`import` しているのは
`G-heading-refs.mjs` と `dependents.mjs` の 2 本だけ。**`G-near-heading-refs` と
`G-folded-heading-refs` はどちらも `HEADING_REF` を import していない**ので、ラベルの文字クラスを
広げてもこの 2 検査の入力は変わらない（実測: `checked` 19 / 20 が不動）。`lib.mjs:161-167` の doc 自身が
「ラベルの側は消費者ごとに違う」と逐語で書いている。

**ただし「`REF_HEAD` から組まれているから安全」という機序の説明は誤りである**（Step 2 レビューの
要対処を採用・自分で再照合）。`G-near-heading-refs` は `REF_HEAD` 由来の `ADJACENT_REF`（49 行）に加えて
**`NEAR_REF`（45 行）を完全に手書きで持っており**、そのラベル部
`` 「([^「」\n]+)」 `` は `HEADING_REF` と**同じ文字クラスを独立に綴っている**。
安全なのは「頭を共有しているから」ではなく「**`HEADING_REF` を import していないから**」である。

**帰結として、本件のあと `HEADING_REF` と `NEAR_REF` のラベルの射程は分岐する**——
近傍形（助詞などを挟んだ形）で入れ子の見出しを指す参照は、引き続き `G-near-heading-refs` から
不可視のままになる。**今日 0 件**（`NEAR_REF` を入れ子受け入れ版へ差し替えても一致 27 件が不動・
両向きの差分 0 件で実測）。**本件では直さない**——`G-near` の射程を広げる根拠が今日 1 件も無く、
`.claude/rules/safety-nets.md` と「検知器は必要な分だけ縛る」に照らして面積だけが増える。
**この分岐は新 ADR の「受容する残余」として宣言する。**

なお `NEAR_REF` は入れ子ラベルに対して内側の断片を label として抽出する（Step 2 レビューが実測）。
これは**本件の変更前から在る挙動**であり、本件では変わらない。

### 編集時 reminder も同じ判定を通る（Step 2 レビューの軽微を採用・再照合）

`scripts/governance/edit-findings.mjs:33,62` が `checkHeadingRefs` をそのまま再利用しており
（`SCAN_SCOPED`）、`.claude/hooks/post-edit.mjs` の編集時 reminder（#1139）は本件の拡張を
**判定ロジックの複製なしに**引き継ぐ。**値を永続化する消費者は無い**——Step 2 レビューが
`evidence.mjs` / `governance-check.mjs` の sink / `governance-manifest.mjs` / `edit-findings.mjs` /
`post-edit.mjs` を実読し、全経路が毎呼び出しで作り直す一時オブジェクトか stdout 出力であることを確認した
（#755/#801 の型は当たらない）。

## 再利用できる既存パターン

- **凍結 ADR への読み替え**: `ADR-governance-meta-demotion.md` の
  「追記（2026-08-20・#1155）— `META_CHECK_IDS` は同じ撤去で消えた」が先例。
  本文は書き換えず、**日付と issue 番号を付けた追記節**で読み替えを与える形
- **宣言する死角**: `lib.mjs` の `isRefTargetSpelling` が
  「**`.ps1` / `.psm1` / `.rs` は入らない**（宣言する死角）——対象にした正準形が今日 0 件」と
  書いている形をそのまま踏襲する
- **フォールトインジェクションは複製で**: `.claude/rules/safety-nets.md`。本件の測定は
  複製ですらなく読み取り専用の別スクリプトで済んでいる（`lib.mjs` を import するだけ）
- **検査の fixture**: `G-heading-refs.test.mjs` が「同一フィクスチャの複製に変異を当てて赤を実測」の
  形を持っており、入れ子版もこの形に合わせる

## 技術的制約

- `governance:check` の契約は「依存ゼロ・決定的」（`scripts/governance-check.mjs` 冒頭）。
  正規表現 1 行の変更はこれを損なわない
- `HEADING_REF` は `g` フラグを持つので `matchAll` からだけ使う（`lib.mjs:174`）。
  形を変えてもこの制約は変わらない
- `normAnchor` は `` [`*「」\s] `` を除去する（`lib.mjs:470`）。ゆえにラベル側の入れ子鉤括弧は
  正規化で消え、着地判定は入れ子の有無に依らない——**照合が生成されさえすれば着地する**
  （実測で不可視だった 2 件はどちらも `landed=true`）
- `docs/adr/` は G-heading-refs の走査元から外れている（`ADR-adr-frozen-history`）。
  新 ADR 本文に書いた正準形は照合されない
- ADR → ADR の短縮引用（`ADR-<slug>`）は `G-adr-citations` の母集団に入る
  （`adrCitationDocs` が `docs/adr/` を明示的に足し戻す）——**追記から新 ADR を短縮名で指せば、
  その辺は機械照合される**

## 裁定した論点

### 論点 A: 読み替えを「新しい ADR」に置くか、凍結 ADR への追記に置くか

issue の実装項目は「**新しい ADR で** ... 読み替えを与える（`ADR-governance-meta-demotion` が
#1155 でやった先例）」と書くが、**その先例は追記を当の凍結 ADR 自身の中に置いている**
（`ADR-governance-meta-demotion.md:57`「追記（2026-08-20・#1155）」／同 59 行「凍結規約により
本文は書き換えず、**ここで**読み替えを与える」）。issue の指示と、issue が引いた先例が食い違う。

**採る形（両方を満たす）**:

1. **新 ADR** が決定と却下 3 案（(a)(b)(d)）を持つ——`AGENTS.md`「ドキュメント参照」が
   「否定の知識が生じた決定のみ」を ADR の条件にしており、本件は 3 案を却下しているので該当する
2. **凍結 ADR には日付つき追記 1 節**を足し、**短縮名で新 ADR を指すだけ**にする（写しを作らない）

判定規準は「**この変更のあと、凍結 ADR を読んだ者は訂正された帰結に出会うか**」である。
新 ADR だけに置くと出会わない——先例がその場所に置かれたのはまさにこの理由による。
かつ短縮引用にすることで `G-adr-citations` が辺の実在を見張る。

### 論点 B: `.claude/rules/governance-docs.md` は変えるか → **変えない**

同ファイルの記述は「G-heading-refs が見るのは正準形だけであり、かつ 1 物理行に収まったものだけ」
であり、ラベルの中身の形について何も言っていない。本件で偽にならない。

### 論点 C: `SPEC.md` は変えるか → **変えない**

ガバナンス機構の内部であり、製品の挙動でも意図でもない。

## 敵対的調査（Step 3b）の所見と採否

`general-purpose` / `sonnet` を 1 体。全文は `workspace/adversarial-1188.txt`。

### 壊せなかった項目（10 件）

1. 消費者 2 つ・`REF_HEAD` 不変（grep + `dependents.mjs` 直読。節境界は `ANCHOR_SPECS` 由来で
   `HEADING_REF` と無関係）
2. NEW が OLD の一致を非破壊で足す（15 ケースの直接実行。**ただし ⚠️1 で 1 形の例外**）
3. 深さ 2 以上の入れ子 = 0 件（**別の探し方**で独立再検算——参照の有無を問わず全アンカー行へ
   直接 grep する母集団の取り方。やはり 0 件）
4. 入れ子かつ折れ = 0 件（probe を再実行し 13 件を目視）
5. 破滅的バックトラックなし（3 種の病的入力すべて 1 ms 未満）
6. 歴史 5 rev の表（全件独立再実行し docs/checked/findings/差分件数が 1 桁も違わず一致）
7. **実 `npm run governance:check` の出力と一致**（見出し参照 365 / 近傍 19 / 折れうる位置 20）
   ——測定スクリプトが「何も測っていなかった」可能性を否定
8. `G-adr-citations` は ADR → ADR の辺を実際に見張る（メモリ上の snapshot へ変異注入し findings 1 件を実測）
9. `ADR-governance-meta-demotion.md:57,59` の引用が逐語一致——凍結 ADR への日付つき追記の先例は
   同 ADR 内に複数回あり、論点 A の裁定を支持
10. 論点 B/C で偽になる散文は無い（`docs/hooks.md` 等も含め概念ラベルで grep・0 件）

### 採用した所見（2 件）

- **⚠️1 — 「加算的」を普遍的性質として書くのは過大主張。** **機序まで自分で再測して裁定した**
  （上の §2 の表）。ラベルの内側に別の正準形の頭が入ると NEW は外側 1 件へ統合し、
  OLD の内側の一致が消える。今日 0 件。**4 つ目の宣言する死角として書く**
- **⚠️2 — 測定スクリプトの `findings` は本物と非同値**（フェンス不整合を拾わない）。
  上の「測定スクリプトの射程」節に限定を明記した。**A/B 比較の妥当性は損なわれない**

### 採らなかった所見（1 件）

- **⚠️3 — issue 本文の「591 件」と 3b の独立集計「570 件」が不一致（差 21）。**
  `research.md` はこの数字を主張しておらず、採用案の判定にも載らない
  （母集団の大きさは「入れ子アンカーが実在する」以上の役割を持たない）。**未検証として残す**——
  数え方の違い（アンカー種別の取り方・除外パス）が差の候補だが、測っていない

## 未解決の疑問

- issue 本文の入れ子アンカー数「591 件」の出所（⚠️3）。**本件の判定に載らないので潰さない**
