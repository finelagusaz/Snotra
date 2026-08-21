# 独立導出レビュー — `productionOnly` の削除（#1095）

計画・research・issue 本文を読まずに、コードと規範だけから導出しました。根拠はすべて現ツリー（`chore/production-only-decision`・作業ツリー clean）の `file:line` と実測です。

## 導出した「変更すべきファイル・シンボル」

| # | ファイル | 対象 | 種別 |
|---|---|---|---|
| 1 | `scripts/governance/checks/G-stale-identifiers.mjs:17-21` | `productionOnly` の JSDoc 2 行と関数本体 | 削除 |
| 2 | `scripts/governance/checks/G-stale-identifiers.mjs:90-94` | 「受容する残余」の Rust テストコードの項が名指す `productionOnly` | 書き換え（概念へ） |
| 3 | `scripts/governance/lib.mjs:571-572` | `headingRefSourceDocs` の doc が名指す「`productionOnly` 相当」 | 書き換え（**偽の前提の除去を含む**） |
| 4 | `scripts/governance/lib.test.mjs:113-114` | 種 3 のコメントが名指す「`productionOnly` 相当」 | 書き換え（同上） |
| 5 | PR 本文 | `productionOnly` を生きたものとして書かない | 記述の制約 |

**触らない**: `docs/adr/ADR-stale-identifier-detector-scope.md:108`、`docs/superpowers/plans/2026-08-15-governance-check-per-check-split.md:161,170,173`。理由は「要対処 D」。

---

## 前提として確定させた事実（実測）

**`productionOnly` は G-stale-identifiers のために書かれた関数ではありません。** `git log -S` で追うと、導入は 43475aa（#793・config 値の到達性検査 G12）で、当時の呼び出し点は 2 つ（母集団側と読み手側）でした。b2ff79c（#897）が G12 を撤去した時点で**呼び出し点が 0 になり**、4f5f4d3（#1093・1 検査 1 ファイルへの分割）が `stripRustComments` との**物理的な隣接だけを理由に** `G-stale-identifiers.mjs` へ運びました。

```
43475aa: 916 定義 / 923 呼び / 956 呼び      ← G12 が使っていた
b2ff79c: 1242 定義のみ                      ← G12 撤去で死んだ（2026-08-07 前後）
4f5f4d3: checks/G-stale-identifiers.mjs:19  ← 隣接で運ばれた
```

ゆえに `:17-18` の JSDoc「**母集団と読み手の両方に適用する**——読み手側で落とさないと…（`visible_rows` で実測）」は、**現在形で書かれた撤去済みの層の記述**です。約 5 か月間、どの機構もこれを赤にしていません。削除は正しい判断であり、`AGENTS.md`「機構・層・ファイル群を撤去する」が言う「撤去した層の語彙」の残余（#1155・#1157 と同クラス）に当たります。

---

## 要対処

### A. `lib.mjs:571-572` は「名前が宙に浮く」のではなく、**#897 以降ずっと偽である**

```
scripts/governance/lib.mjs:571-572
  … `productionOnly` 相当を
  「G-stale-identifiers との対称性の完成」として後から入れてはならない（その非対称は意図である）。
```

この文は「G-stale-identifiers は `#[cfg(test)]` を落とすが、`.rs` の腕は落とさない。その非対称は意図である」と読めます。**その非対称は今日存在しません**——`productionOnly` が死んでいる以上、どちらの母集団も `#[cfg(test)]` の内側を見ています。

したがって修正は「宙に浮いた識別子の改名」ではなく、**偽の前提を落としつつ規範を保存する**作業です。規範そのもの（「`headingRefSourceDocs` へ `#[cfg(test)]` 以降を落とす変換を足してはならない」）は実在し、`lib.test.mjs:111-119` の種 3 が固定しています。**関数名ではなく変換を名指す形**（例:「`#[cfg(test)]` 以降を落とす変換を…入れてはならない」）へ書き換えてください。関数名を消して文をそのまま残すと、「対称性の完成」という言い回しだけが残り、読者は存在しない相手方を探すことになります。

`lib.test.mjs:113-114` も同じ文の写しであり、同じ書き換えが要ります（`.claude/rules/governance-docs.md`「書く約束」の (3) 古い情報を残さない）。

### B. `:90-94` の書き換えでは、**測定の結論を落とさない**

```
scripts/governance/checks/G-stale-identifiers.mjs:90-94
  - **Rust のテストコードは今も語彙を寄付しうる。** `VOCAB_TEST_FILE` が当たるのは…3 つの形…
    `productionOnly` を通しても落ちるのは 1 つ目だけである。現時点でこの穴に落ちた finding は
    1 件も無く（測定の全セルで 0 件）…
```

ここが担っているのは、ヘッダが `:61` と `:141` と finding メッセージ `:188` で 3 度使う「**production の**ソース」という語の**唯一の訂正**です。実際に "production" を支えている機構は `VOCAB_TEST_FILE`（`.test.<ext>` というファイル名の形）だけで、`.rs` のテストコードは語彙に入っています。`productionOnly` を消すと、ファイル内に `#[cfg(test)]` を志す痕跡が一切無くなるため、この項が消えると**ヘッダの言い過ぎを訂正する者がいなくなります**。

書き換え後も次の 2 つが残っているか確かめてください。

1. Rust 側の 3 つの形（`#[cfg(test)] mod` の中身 / `<crate>/tests/*.rs` / `src/**/tests/*.rs`）が語彙を寄付しうること
2. **`#[cfg(test)]` 以降を落とす変換を入れても閉じるのは 1 つ目だけ**という測定結果と、「全セルで finding が 1 件も動かなかったので採らなかった」という却下理由

2 は誰かが払った測定です。`ADR-stale-identifier-detector-scope:108` にも在りますが、**ADR は凍結された歴史であり、検査の保守者が読む場所ではありません**。`AGENTS.md`「調査・測定のための一時的な足場を…撤去する」が言う「撤去は出所が永久に失われる瞬間」がここに当たります。

### C. 書き換えのとき、`:112` / `:152` の「受容する残余」を正準形へ格上げしない

#1157（da32769）以降、`isRefTargetSpelling` は `.mjs` を対象綴りとして受理します（`scripts/governance/lib.mjs:194-195`）。一方 `ANCHOR_SPECS`（`:428-433`）の第 3 スペックは `^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*` であり、**JSDoc の継続行（` *  **X**`）は当たりますが、行コメントの箇条書き（`// - **X**`）は行頭が `/` なので当たりません**。

`collectAnchors` を実ファイルへ当てて実測しました。

```
G-stale-identifiers.mjs のアンカー 5 件:
  '追跡され・人が書き・CI が実際に実行する' / '3 述語は先頭文字と字種で相互排他である' /
  '修飾形（`::`）は末尾セグメントだけを見る' / '`.` の除外はトークン全体ではなく…' / '捕獲群を読まない'
  → いずれも JSDoc 側。「受容する残余」は**アンカーになっていない**
```

ゆえに `:112`（「Rust 側の穴は上の「受容する残余」」）と `:152`（「上の「受容する残余」が記録する失敗形」）は**バッククォート無しの素の「」のまま**にしてください。親切心で `` `G-stale-identifiers.mjs`「受容する残余」 `` と正準形にすると、パスは解決するのにアンカーが無く、G-heading-refs が恒久的に赤くなります。

### D. 触ってはならないもの

| 対象 | 理由（規範） |
|---|---|
| `docs/adr/ADR-stale-identifier-detector-scope.md:108` | ADR 本文は決定日時点の記述として凍結する（`ADR-adr-frozen-history`。`.claude/rules/governance-docs.md`「ADR 本文内の参照は照合されない——凍結された歴史であり腐るに任せる」）。同行の「`stripRustComments` の隣にある `productionOnly`」は位置の記述だが、凍結層ゆえ直さない |
| `docs/superpowers/plans/2026-08-15-…md:161,170,173` | #589 で非規範化された当時の設計。`lib.mjs:650-655` が `staleIdentifierGuideDocs` で明示的に除外している |
| **ADR への追記** | 不要。配線を採らなかった裁定は既に `:108` に記録済みで、追記は凍結の趣旨に反する。両歴史層が母集団外であることは実測済み（`population 34 / adr? false / sp? false`） |
| `stripRustComments`（`:13-15`） | `:154` から現に呼ばれている。削除対象ではない |
| `G-stale-identifiers.test.mjs` | 46 個の `describe`/`it` を確認したが `productionOnly` に触れるものは 0 件。テスト変更は不要 |

### E. PR 本文を数え上げの母集団に含める

`AGENTS.md`「文書に事実の写しを増やす変更」および「機構・層・ファイル群を撤去する」が指摘するとおり、PR 本文は squash で main の commit message になるのに**リポジトリの grep には入りません**（#1056）。PR 本文で `productionOnly` を「G-stale-identifiers が使っている関数」と書かないでください。正確には「#793 が G12 のために置き、#897 の G12 撤去で死に、#1093 が隣接で運んだ関数」です。

---

## 軽微

- **`visible_rows` の実測記録が失われます。** `:18` の「（`visible_rows` で実測）」は G12 が「テストだけが読むフィールドが読まれている側へ落ちる」を実測した記録です。G12 ごと撤去済みなので保存義務は薄く、`docs/adr/ADR-source-text-probes-not-lifted-to-types.md` などに `visible_rows` の文脈は別途残ります。削除でよいと判断しますが、落とすことは意識的に決めてください。
- **`//!`（`:1`）とヘッダブロック（`:24`）は同文が 2 回書かれています。** 今回の変更とは独立の既存事情なので、ついでに畳まないでください（`CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」）。
- **削除範囲は空行の扱いに注意。** `:16` と `:22` の空行のどちらを残すかで、`:13-15` の `stripRustComments` とヘッダブロックの間隔が変わります。実測では `17,22d`（JSDoc 2 行 + 本体 3 行 + 後続空行）で構文・全検査とも問題ありませんでした。

---

## 検証手段（「沈黙は合格」が成り立たない経路つき）

### 走らせるもの

| 手段 | コマンド | 実測（削除を当てた状態） |
|---|---|---|
| ガバナンス全検査（`docs/build-commands.md` カテゴリ F） | `npm run governance:check` | **緑**（検査 21 件 / 散文の識別子 404 件を 34 文書から照合 / 見出し参照 324 件） |
| governance のユニットテスト | `npx vitest run scripts/` | **緑**（32 files / 523 tests） |
| 語彙からの離脱と赤経路 | 合成 snapshot で `currentVocabulary` / `checkStaleIdentifiers` を直接呼ぶ | 削除後 `productionOnly` は語彙 false。生きた層の文書に `` `productionOnly` `` と書くと**赤になることを実測** |

3 番目は「削除したのに何も鳴らない」を「鳴る対象が無いから鳴らない」と区別するための対照です。生きた層（`.claude/**` / `docs/**` の非履歴 / `SPEC.md`・`CLAUDE.md`・`AGENTS.md`・`snotra-settings/SETTINGS-DESIGN.md`）に 1 件でも `` `productionOnly` `` を残せば CI が赤くなります——**今日そこには 1 件もありません**（repo 全体の `git grep` で `.md` の出現は ADR 1 件と superpowers 3 件のみ、いずれも母集団外）。

### 「沈黙は合格」が成り立たない経路（名指し）

1. **PostToolUse hook は `scripts/**/*.mjs` に検査を 1 本も割り当てません。** `.claude/hooks/post-edit.mjs:132-178` の `selectChecks` を読むと、分岐は `.rs` / `Cargo.toml` / `tauri.conf.json`・`config.toml` / `CHECK_DEFINITION`・`.claude/hooks/` / `.githooks/` / `.claude/lsp/`・`rust-analyzer.toml` だけです。`scripts/governance/checks/G-stale-identifiers.mjs` はどれにも当たらず**戻り値は空配列**。編集後の沈黙は「合格」ではなく「**何も走らなかった**」です（ルート `CLAUDE.md`「フック」の条項どおり）。`governance:check` は手で回してください。
2. **編集に帰属する reminder（#1139）も、この編集では腐りを見ません。** `scripts/governance/edit-findings.mjs:52-58` の `SCAN_SCOPED` は stale-identifiers の走査元を `staleIdentifierTargets` に取り、その母集団は `.md` 34 件のみ（実測）。`.mjs` は入りません。見出し参照 3 種だけは `allHeadingRefDocs`（`headingRefCommentDocs` 経由で `scripts/` を含む）ゆえ編集時に鳴りえます——**つまり C 項の正準形の事故は編集時に捕まりますが、A/B 項の腐りは編集時にも CI にも捕まりません**。
3. **JS の未使用検出器は存在しません。** リポジトリに eslint / oxlint / biome の設定はなく、`package.json` の scripts にも lint はありません。`productionOnly` が 5 か月間 dead のまま緑で推移したのはこれが理由です。

---

## この削除が持つリスクのうち、機構が検知しないもの

1. **残余を 1 つも直さなくても、全部が緑のままです。** 削除だけを当てた状態で `governance:check` 21 件が緑、`vitest` 523 件が緑でした。`:90-94` の腐った名指しも、`lib.mjs` の偽の非対称も、赤を一切生みません。**この PR の品質を測る機構は存在しない**という前提で書いてください。
2. **コメントは assert されません。** `lib.test.mjs` の種 3 は `#[cfg(test)]` の内側のコメントが見られることだけを測り、`:113-114` のコメントを**どう書き換えても通ります**。理由の説明が雑に消えても誰も気づきません。
3. **次に来る人が、規範そのものを消しうる経路。** A 項を「識別子を消すだけ」で処理すると、`lib.mjs` に「`#[cfg(test)]` 相当を入れてはならない（非対称は意図）」という**相手方が探せない警告**が残ります。将来の保守者が `productionOnly` を grep して 0 件を見て「この警告は腐っている」と判断し、**警告ごと削除する**——それは規範の消失であり、種 3 は「コメントを消しただけ」では落ちません。A 項を「偽の前提を落として変換で名指す」まで踏み込むべき理由がこれです。
4. **G-stale-identifiers の母集団が `.md` に閉じている非対称そのもの。** #1157 が同じ理由で `.mjs` を見出し参照の対象綴りへ足しましたが、**腐り検出の母集団は今も `.md` だけ**です。この残余が 5 か月生き延びたのはその穴の中に居たからで、削除しても穴は閉じません（閉じるべきとは主張しません——#975 が `.rs` への拡大を測って却下した理由が `.mjs` にも当たりうるため、判断には別途測定が要ります）。

---

## 未検証

- **CI（`governance-check` job）での実測**: PR が無いと `ci.yml` が起動しないため未確認（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）。ローカルの `npm run governance:check` は CI と同じスクリプトを呼ぶので差は小さいと見ますが、**同一であることは測っていません**。
- **`npm test` 全体（Rust 側を含む）**: 走らせたのは `npx vitest run scripts/` のみです。今回の変更は `.mjs` 1 ファイルに閉じ、Rust ビルドへの入力にならないため不要と判断しましたが、**判断であって測定ではありません**。
- **書き換え後の文面が G-near-heading-refs / G-folded-heading-refs を通るか**: 実際の文面が無いので測れません。書いたら `npm run governance:check` を回してください（C 項の事故はここで出ます）。
- **`productionOnly` が体現する概念のうち、日本語散文だけで書かれた腐り**: `#[cfg(test)]`・「テストコードを外す」・「非対称は意図」で全リポジトリを grep し、生きた層の該当は本文の 5 件に収束しました。ただし G-stale-identifiers 自身が認めるとおり**日本語散文の腐りは述語の外**にあり、「言い換えだけで書かれた記述」が残っていないことは grep では示せません。この一覧が完全であるとは主張しません。
- **`workspace/` 配下の既存計画との突き合わせ**: 指示により未読です。この文書は計画の追認でも否認でもなく、独立に導出した結果です。
