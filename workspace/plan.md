# plan — issue #1188: `HEADING_REF` を 1 段の入れ子まで受け入れる

ブランチ: `fix/nested-quote-heading-refs`　／　調査: `workspace/research.md`

## 目的

見出し名に鉤括弧が入れ子で含まれるとき、その見出しを全形で指す正準形参照が**照合そのものを
生成しない**死角を消す。検知器を新設して書き方を縛るのではなく、`lib.mjs` の `HEADING_REF` の
ラベル文字クラスを 1 段の入れ子まで広げる。

## 受け入れ条件

1. `HEADING_REF` が `` `docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」 ``
   の形の参照に一致する
2. `npm run governance:check` が exit 0 で、見出し参照 `checked` = 365 / findings 0
   （近傍 19 / 折れうる位置 20 も不動）
3. 歴史コミット `b93b0fb8^` で、変更後の正規表現の `checked` が変更前より **+2**、
   **findings は変わらず 0**（18 日間沈黙していた 2 件が赤を出さずに「照合済み」へ上がる）
4. `buildDependentIndex` の索引 149 キーが全エントリ逐語一致（変更前後で差分ゼロ）
5. `npm test` が変更前と同じ結果（変更に帰属する新規失敗 0 件）
6. `HEADING_REF` の doc に**宣言する死角 4 つ**が書かれている
7. 凍結 ADR `ADR-folded-canonical-reference-detector` の帰結
   「`HEADING_REF` の意味論は変わらない」に読み替えが与えられている
8. **この変更で偽になる散文が生きた層に残っていない**——`build.rs` の回避策の理由 2 行が消えており、
   同型の数え上げ grep が生きた層で 0 件を返す
9. `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test -p snotra-core` が通る
   （`build.rs` を触るため。PostToolUse hook が自動実行する）

## 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `scripts/governance/lib.mjs` | `HEADING_REF`（177 行）とその直上の JSDoc | 正規表現 1 行 + doc |
| `scripts/governance/checks/G-heading-refs.test.mjs` | `describe("G-heading-refs checkHeadingRefs（見出し参照の実在）")` | fixture 3 件を追加 |
| `docs/adr/ADR-nested-quote-heading-ref-labels.md` | — | **新規** |
| `docs/adr/ADR-folded-canonical-reference-detector.md` | — | 日付つき追記 1 節（本文は書き換えない） |
| `snotra-core/src/search/build.rs` | `enum DerivedStrings` の rustdoc（35-36 行） | **腐る散文 1 文を消す**（下記） |

### `build.rs:35-36` — この変更で偽になる唯一の生きた散文

```rust
/// **参照を切り詰め形で書いてあるのは、
/// 見出し名が鉤括弧を入れ子に含むためである**——全形で書くと照合そのものが生成されない。
```

指し先は `PERFORMANCE.md:2057`
`#### 潰し済みかどうかを型で区別する（理由は「壊れるから」ではなく「無駄だから」）`——
**まさに入れ子を含む見出し**である。この 2 行は #1185 が手で当てた回避策の**理由**を書いており、
本件が機構を入れた瞬間に**偽になる**。

**対処: 当該 2 行を消すだけにする**（切り詰め形の参照そのものは前方一致で着地し続けるので触らない）。
`docs/comment-guidelines.md`「第一原則: コメントは理由を書く」が「振る舞いを保つリファクタを
生き延びない一文は書かない」を定めており、消滅した回避策の理由はこれに当たる。

**数え上げ**: `切り詰め形｜切り詰めた形｜全形で書くと｜照合そのものが生成されない｜
一致そのものが生成されない` を全域 grep（`.claude/worktrees/`・`docs/superpowers/`・`.superpowers/`・
`workspace/` を除く）した結果、**生きた層の該当は build.rs のこの 1 件だけ**である。
`ADR-canonical-heading-references:40` は凍結された歴史かつ別の事象（対象綴り）、
`G-folded-heading-refs.mjs:15` は折れについての記述で本件では偽にならない。

### 射程外と決めたもの

**`docs/comment-guidelines.md:9` の見出しを `## 第一原則: コメントは「なぜ」を書く` へ戻さない。**
#1185 が入れ子を外す方向へ改名したのは意図的な編集であり、issue #1188 は改名の差し戻しを求めていない。
戻せば `RETROSPECTIVE.md:23` と `snotra-core/src/indexer.rs:53` の参照も同時に直す必要が生じ、
機構上の利得は 0 である（どちらの綴りでも照合される）。ルート `CLAUDE.md`「コミュニケーション原則」の
「意図的なリファクタリングの結果を元に戻さない」に従う。

**触らないもの**（実測で入力が変わらないことを確認済み・`research.md`）: `REF_HEAD`・
`G-near-heading-refs.mjs`・`G-folded-heading-refs.mjs`・`dependents.mjs`・`G-heading-refs.mjs`・
`.claude/rules/governance-docs.md`・`SPEC.md`。

**安全である理由は「頭を共有しているから」ではない**——`G-near-heading-refs` は `REF_HEAD` 由来の
`ADJACENT_REF` とは別に `NEAR_REF`（45 行）を**完全に手書き**で持ち、そのラベル部は
`HEADING_REF` と同じ文字クラスを独立に綴っている。安全なのは**この 2 検査が `HEADING_REF` を
import していないから**である（`G-near-heading-refs.mjs:2` の import 行で確認）。
**帰結: 本件のあと `HEADING_REF` と `NEAR_REF` のラベルの射程は分岐する。今日 0 件（実測）。
本件では直さず、新 ADR の「受容する残余」として宣言する。**

## 実装順序

### Phase 1 — 機構（`lib.mjs` + fixture）

- [ ] `lib.mjs:177` を次へ置き換える（**`REF_HEAD` は 1 文字も触らない**）

  ```js
  export const HEADING_REF = new RegExp(`${REF_HEAD}「((?:[^「」\\n]|「[^「」\\n]*」)+)」`, "g");
  ```

- [ ] `HEADING_REF` の JSDoc へ次を足す（既存の 3 段落は残す）
  - **1 段の入れ子まで受け入れる**こと、その理由（見出し名の入れ子は死角ではなく実在の形であり、
    #1188 の時点で 18 日間・独立 2 エピソードの沈黙が実在した）
  - **⚠ doc に書く例が検出器を赤にしないこと**（#1155 の再演を避ける）。`lib.mjs` は
    `headingRefCommentDocs` の母集団に入る（`.mjs` はコメント記法を持つ）ので、
    **自分の JSDoc が照合対象である**。とくに
    `` `docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」 `` は
    **#1185 が見出しを改名済みなので今は着地せず、書いた瞬間に赤になる**。
    例は正準形の隣接を崩すか、実在するアンカーへ着地する形で書く。
    `.mjs` で終わるプレースホルダも `isRefTargetSpelling` に当たる（同 doc の既存の警告どおり）
  - **宣言する死角 4 つ**（今日いずれも 0 件・全追跡ファイル 496 本で実測）:
    1. 深さ 2 以上の入れ子
    2. 入れ子かつ物理改行で折れた形（折れ一般は `G-folded-heading-refs` の担当）
    3. ラベルが閉じない形
    4. **他の参照のラベルの内側に置かれた正準形参照**——外側 1 件へ統合され、独立には照合されない
  - **境界が見えにくくなる副作用**（#489）: 線が「入れ子は照合されない」から
    「**1 段までは照合される**」へ動く。深さ 2 を書いた者はより強く動くと期待する。
    ゆえに死角は**宣言する**形で書く（`isRefTargetSpelling` が `.ps1` 等に採ったのと同じ形）
  - 決定と却下 3 案の所在として `ADR-nested-quote-heading-ref-labels` を**短縮名で**引く
- [ ] `G-heading-refs.test.mjs` へ fixture 3 件を足す
  - **緑**: 入れ子を含む見出しを全形で指す参照が着地する
    （アンカー `## 第一原則: コメントは「なぜ」を書く` ／ 参照
    `` `docs/c.md`「第一原則: コメントは「なぜ」を書く」 ``）
  - **赤（フォールトインジェクション）**: 同じ見出しを改名すると
    「見出し参照が着地しない」の finding が 1 件出る
    ——**これが「一致は生成されるようになった」ことの接地である**
    （旧実装では一致自体が無いので finding も 0 件になり、緑の fixture だけでは
    「照合していない」と区別が付かない）
  - **死角の固定**: 深さ 2 の入れ子（`「A「B「C」D」E」`）は一致を生成しない（findings 0・checked 0）

- [ ] **`workspace/measure-heading-ref-nesting.mjs:32` の `NEW` を、`lib.mjs` から import した
      `HEADING_REF` そのものへ差し替える**（`` () => HEADING_REF ``。`matchAll` は内部で複製するので
      `g` フラグの共有は安全——`lib.mjs:174` の doc が保証している）。
      **これをしないと V3/V4 は自明に緑である**——現行の script は OLD も NEW も**両方ハードコード**しており
      （31-32 行で確認）、`lib.mjs` から取るのは `REF_HEAD` だけなので、**`lib.mjs` を 1 文字も
      編集しなくても +2 が出る**（#922 型の「観測形が対象を含まない」）。
      `OLD` はハードコードのまま残す（歴史側のベースラインであり、実装から取れない）
- [ ] `snotra-core/src/search/build.rs` の 35-36 行（回避策の理由を書いた 2 行）を消す。
      **切り詰め形の参照そのものは触らない**。`.rs` の編集なので PostToolUse hook が
      fmt / clippy / test を走らせる（沈黙 = 合格。ただしその沈黙は見出し参照の着地を含まない）

### Phase 2 — 記録（ADR 2 枚）

- [ ] `docs/adr/ADR-nested-quote-heading-ref-labels.md` を新設する。1 行目は
      `# ADR-nested-quote-heading-ref-labels: <題>`（`G-adr-file-names` が stem と見出しの一致を見る）
  - **文脈**: 18 日間・独立 2 エピソード・2 つの別の見出し・2 つの別の引用元（うち 1 件は
    `snotra-core/src/indexer.rs` の rustdoc）。`b93b0fb8` の掃除は 1 見出しへ手で適用しただけで機構は無い
  - **決定**: ラベルを 1 段の入れ子まで受け入れる
  - **却下した代替案 3 つ**（issue の (a)(b)(d)）:
    - (a) 見出し側を縛る——`ADR-canonical-heading-references` が既に「参照側の記法を直す」を
      採っており向きが反転する。ATX に絞れば死角が残り、全アンカーへ広げれば 591 件の改名になる
    - (b) 参照側を縛る新検査（`G-nested-quote-heading-refs`）——誤検出 0 で成立はするが
      新規 2 ファイル + 配線 + ADR + 実行時間を要し、正規表現案の上位互換ではない
    - (d) 兄弟 `G-folded-heading-refs` へ同居——最安だが id が内容を偽る
  - **`ADR-folded-canonical-reference-detector` の却下 1 が本件に当たらない理由 3 つ**
    （消費者・行番号の帰属と節境界・折返しの規範との向き。いずれも実測で裁定済み）
  - **帰結**: 宣言する死角 4 つ（正本は `lib.mjs` の doc・**ここへ写さない**）／
    移行コスト 0（新たに赤くなる参照は全域で 0 件）／性能不変
  - **受容する残余**: `G-near-heading-refs` の `NEAR_REF` はラベル文字クラスを**独立に手書き**しており
    本件で広がらない。ゆえに**近傍形で入れ子の見出しを指す参照は引き続き不可視**である
    （今日 0 件・実測）。**安全性の根拠は「頭を共有しているから」ではなく
    「`HEADING_REF` を import していないから」である**——この区別を書かないと、
    次に `REF_HEAD` を触る者が誤った安心を得る
- [ ] `docs/adr/ADR-folded-canonical-reference-detector.md` へ
      **`## 追記（2026-08-26・#1188）— 帰結「`HEADING_REF` の意味論は変わらない」は覆った`** を足す
  - **本文は 1 文字も書き換えない**（`ADR-adr-frozen-history` の凍結規約）
  - 追記の中身は「ラベルの文字クラスが 1 段の入れ子まで広がった。**却下 1 が挙げた 3 つの理由は
    本件には当たらない**（理由の所在は `ADR-nested-quote-heading-ref-labels`）」の**短縮引用 1 本**に
    留める。**却下 1 の議論をここへ写さない**
  - 先例: `ADR-governance-meta-demotion.md:57`「追記（2026-08-20・#1155）」（同じ形）

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| `REF_HEAD` は変わらない | `git diff` で `lib.mjs:168` に差分が無いこと（目視 1 行） |
| `G-near` / `G-folded` の `checked` が不動（19 / 20） | `npm run governance:check` の evidence |
| `dependents` の索引が不動（149 キー） | 変更前後で `buildDependentIndex` を JSON 化して `diff` |
| 生きた母集団で新たに赤くなる参照が 0 件 | `npm run governance:check` の findings 0 |
| **`OLD にだけ在る` = 0 件**（今日の実測。普遍的性質ではない） | `measure-heading-ref-nesting.mjs` の出力 |
| `HEADING_REF` は `matchAll` からだけ使う（`g` フラグ） | 既存の doc が持つ。消費者 2 本とも `matchAll` |

**異常系**: ラベルが閉じない・深さ 2 以上・フェンス内——いずれも一致を生成しない
（＝ 変更前と同じ振る舞い）。`G-folded-heading-refs` が折れを別途赤にする射程は変わらない。

## テスト方針と検証コマンド

**測定は複製ですらなく読み取り専用の別スクリプトで行う**——`lib.mjs` を import するだけなので、
稼働中のガードを 1 バイトも弱めない（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、
稼働中のガードを弱めない——複製に変異を当てる」の要求より強い形）。

| # | コマンド | 期待 |
|---|---|---|
| V1 | `npm run governance:check` | exit 0・見出し参照 365 / 近傍 19 / 折れうる位置 20・findings 0 |
| V2 | `npm test` | 変更前と同じ結果（変更に帰属する新規失敗 0 件。**変更前の結果を先に採る**） |
| V3 | `node workspace/measure-heading-ref-nesting.mjs . "b93b0fb8^" "file:///<abs>/scripts/governance/lib.mjs"` | NEW checked が OLD +2・findings 0/0・`OLD にだけ在る` 0 件 |
| V4 | 同上を `HEAD` へ | OLD/NEW とも checked 365・差分両向き 0 件 |
| V5 | `node workspace/probe-heading-ref-blind-spots.mjs . HEAD <lib url>` | 深さ 2 以上 0 件・閉じ不足 0 件 |
| V6 | `buildDependentIndex` の JSON を変更前後で `diff` | 149 キー・全エントリ一致 |
| V7 | `切り詰め形｜切り詰めた形｜全形で書くと｜照合そのものが生成されない｜一致そのものが生成されない` を全域 grep（`.claude/worktrees/`・`docs/superpowers/`・`.superpowers/`・`workspace/` を除く） | 生きた層 0 件（凍結 ADR 1 件と `G-folded-heading-refs.mjs` 1 件は残る） |
| V8 | `cargo fmt --check` / `cargo clippy` / `cargo test -p snotra-core`（hook が自動実行） | 沈黙 = 合格 |

**V9 は置かない（自明緑ゆえ）。** 編集時 reminder 経路に実行可能な検証は無い——
`dependents.mjs` の CLI は `git diff HEAD -- <path>` の**未コミット差分**から hunks を作るので、
本件が触らない文書に対しては差分ゼロ → 無出力 exit 0 が**変更の有無と無関係に**出る。
保証は実行ではなく**コード経路の同一性**が持つ: `edit-findings.mjs:33,62` が `checkHeadingRefs` を
そのまま再利用しており、判定ロジックの複製が存在しない。

**V3 が本件のフォールトインジェクションになるのは、上の Phase 1 の差し替えを終えてからである。**
それまでの唯一の実装接地は**赤の fixture** である——実装が no-op なら入れ子の一致が生成されず、
「見出し参照が着地しない」の finding が 0 件になって当該テストが落ちる。

**V3 が本件のフォールトインジェクションである**——「変異が効いていない」ではないことの接地を、
歴史データの実在 2 件が担う。V3 が +2 を出さないなら、正規表現は何も変えていない。

**`.claude/worktrees/` は全 grep から除外する**——前サイクルの使い捨て worktree に古い写しが残っており、
数え上げを汚染する（3b の走査制約と同じ）。

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**（ガバナンス機構の内部であり製品の意図でも挙動でもない）
- `.claude/rules/governance-docs.md`: **不要**（同ファイルはラベルの中身の形について何も言っていない。
  「1 物理行に収まったものだけ」は本件で偽にならない）
- `AGENTS.md` / ルート `CLAUDE.md` / `docs/hooks.md` / `docs/development-principles.md`: **不要**
  （3b が概念ラベル「入れ子」「鉤括弧」「ラベル」「正準形」「照合」でも grep し 0 件）
- `RETROSPECTIVE.md`: 本件では触らない（サイクル終了時に `/retrospective` が扱う）

## 未確定（実装前に潰す）

（なし）

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に該当）
- plan-review: 独立レビュー 1 体（Step 2 — 計画準拠。網羅性が要件ではなく変更集合が有界なので 2b は採らない）
- エージェント数: 2（3b の敵対的調査 1 体 + plan-review Step 2 の 1 体）
- 要対処: **1 件・反映済み**——`G-near-heading-refs` は `NEAR_REF`（45 行）を完全に手書きで持ち、
  そのラベル文字クラスは `HEADING_REF` と同じ形を独立に綴っている。ゆえに
  「2 定数は `REF_HEAD` から組まれているから安全」という**機序の説明が誤り**だった
  （安全性の根拠は「`HEADING_REF` を import していない」ことである）。**結論は変わらない**
  ——`checked` 19 / 20 の不動は実測済み。**再照合の根拠**: `G-near-heading-refs.mjs:2`（import 行に
  `HEADING_REF` が無い）・同 45 行（`NEAR_REF` の定義）。あわせて**分岐する射程を
  「受容する残余」として新 ADR へ宣言する**作業を Phase 2 へ追加した
- 軽微: 1 件・反映済み——編集時 reminder（`edit-findings.mjs:33,62` が `checkHeadingRefs` を再利用）が
  検証一覧に無かった。V9 として追加。判定の複製が無いので正しさへの影響は無い
- 未検証: issue 本文の入れ子アンカー数「591 件」（3b の独立集計は 570 件）。
  **本件の判定に載らない**ため潰さない（`research.md`「採らなかった所見（1 件）」）

## plan-review 結果

- リスク: **高**
- レビュー方式: 計画準拠レビュー 1 体（Step 2）
- エージェント数: 1（3b の敵対的調査 1 体を除く）

### 要対処

- 消費者の機序記述の誤り — 計画と調査の両方を修正 + 新 ADR の「受容する残余」へ 1 項追加 —
  再照合の根拠は `G-near-heading-refs.mjs:2`（import 行）と同 45 行（`NEAR_REF`）。
  乖離が今日実在しないことは独立に実測（`NEAR_REF` を入れ子受け入れ版へ差し替えても一致 27 件が不動・
  両向きの差分 0 件）

### 軽微

- 編集時 reminder 経路が検証一覧に無かった — V9 として追加

### 未検証

- issue 本文の「591 件」（本件の判定に載らない）

### 判断

- 実装着手: **可**（人間の承認後）

### plan-review 後の訂正（再レビュー不要）

3 件を追加で直した。**要件・対象ファイル/シンボル・インターフェース・不変条件・テスト期待値
（+2 / findings 0）はいずれも動いていない**ので、`/plan-review` の再実行条件に該当しない。

1. **V3/V4 が自明緑だった**（#922 型）——`measure-heading-ref-nesting.mjs:31-32` は OLD も NEW も
   ハードコードしており、`lib.mjs` を 1 文字も編集しなくても +2 が出る。
   script の `NEW` を `HEADING_REF` の import へ差し替える作業を Phase 1 へ追加した
2. **V9 が無条件で緑だった**——`dependents.mjs` の CLI は未コミット差分を読むので、
   本件が触らない文書では差分ゼロ → 無出力 exit 0 が変更の有無と無関係に出る。V9 を削り、
   保証をコード経路の同一性へ帰属させる散文へ置き換えた
3. **JSDoc の例が検出器を赤にしうる**（#1155 の再演）——`lib.mjs` は自分の JSDoc が
   照合母集団に入り、かつ #1185 が改名した見出しを例に使うと着地しない。注意を Phase 1 へ追加した

## 人間レビュー

- [x] 承認済み — 2026-08-26 / 問い: "`workspace/plan.md` へ注釈を追加していただくか、**明示的に承認**いただければ、Step 6（workspace のコミット・push）を経て `/implement` で実装へ渡せます。" / 回答: "承認します、コミットして /implement へ進めてください"
