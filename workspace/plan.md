# plan — issue #1172 全角の閉じ括弧で intra-doc link が沈黙する

## 目的と受け入れ条件

`governance:check` に検査 `G-fullwidth-doc-link-bracket` を足し、production の `///` / `//!` に書かれた開き ASCII `[`・閉じ全角 `］`（および逆向き `［`…`]`）の混在を赤にする。

受け入れ条件（issue の 4 項）:

1. production の `///` へ混在形を注入すると `governance:check` が exit 1 になる（フォールトインジェクション実測・巻き戻しは SHA256 照合）
2. 現行ツリーで finding 0 件、かつ `checked`（照合した角括弧対の数）が 0 でない
3. 検査モジュールのヘッダに、射程（母集団の導き方）と死角（沈黙側・赤側）、および `docs/comment-guidelines.md`「名指しと正本の指名」の表の**どの行を守るか**（production の `///` `//!` の行）を書く
4. `#[cfg(test)]` を**絞らない**選択と理由をヘッダに書く

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/governance/checks/G-fullwidth-doc-link-bracket.mjs` | 新規。`id` / `run` / `scanFullwidthDocLinkBrackets`（`{findings, checked}`）/ `checkFullwidthDocLinkBrackets`（findings のみ・edit-time 用） |
| `scripts/governance/checks/G-fullwidth-doc-link-bracket.test.mjs` | 新規。赤 / 緑 / 証跡の 3 節 |
| `scripts/governance/evidence.mjs` `assembleEvidence` | 要約行に `/ doc の角括弧対 ${ev.docLinkBrackets} 件` を追加 |
| `scripts/governance/evidence.test.mjs` `complete()` | fixture へ `docLinkBrackets: <整数>` を 1 行追加。**足さないと「緑: すべて記録済み」の it が未記録キーで落ちる**（独立導出が見つけた漏れ） |
| `scripts/governance/edit-findings.mjs` `SCAN_SCOPED` | `{ population: headingRefSourceDocs, check: checkFullwidthDocLinkBrackets }` を 1 行追加。`headingRefSourceDocs` は `.rs` 全件（`lib.mjs:648`・`tests/` や `build.rs` も入る無害な過剰）。`allHeadingRefDocs` ではなくこちらにするのは、母集団の宣言を検査の射程（`.rs` の doc）と一致させるため |
| `scripts/governance/edit-findings.test.mjs` | 「配線されていることの固定」を 1 本追加（`G-folded-code-spans` の赤ケースと同じ型: 1 形だけを見る。判定の正しさは検査隣接のテストが持つ） |
| `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」 | 表に 1 行追加（判定列 `checkFullwidthDocLinkBrackets`） |
| `docs/comment-guidelines.md`「名指しと正本の指名」 | 「表の「○」が嘘になる形が 1 つある」の段落末に、機構が赤にする旨と検査名を 1 文追記 |

触らないもの: `lib.mjs`（`linesOfComments` の既定引数・`COMMENT_FAMILY` は動かさない・#489）、`registry.mjs`（走査で自動登録）、`allHeadingRefDocs`（母集団定義を増やさない）。

## 述語（実装者が判断しない粒度）

```js
// 1 物理行内の混在対だけを見る。内側に角括弧（半角・全角）を含まない最短一致
const MIXED = /\[[^\[\]［］\n]*］|［[^\[\]［］\n]*\]/g;
// 証跡用: 何らかの角括弧対（開き半角/全角 → 閉じ半角/全角）を数える
const ANY_PAIR = /[\[［][^\[\]［］\n]*[\]］]/g;
```

走査行: `file.endsWith(".rs")` のときだけ `linesOfComments(text, file, "js")` を取り、`const t = line.trimStart(); t.startsWith("///") || t.startsWith("//!")` の行に絞る。**`trimStart()` は必須である**——`linesOfComments` は `raw`（インデント込み）を返すので、無いと実ツリーの大半のインデント済み doc 行が母集団から落ちる（敵対的調査が壊した点）。この誤りは単体テストの「インデント済み `///`」fixture が赤にする。`run(snapshot, ctx)` は `headingRefSourceDocs(snapshot)`（`.rs` 全件）を母集団に渡す。`.rs` 以外が渡っても `[]` を返す（`SCAN_SCOPED` と `run` で同じ関数を共有するため）。

`checked` は `ANY_PAIR` の一致数（混在も正しい対も数える）。`MIXED` の一致ごとに finding を 1 件（同じ行に 2 つあれば 2 件——issue の実例は 1 行に 2 つ）。

メッセージ: `intra-doc link の角括弧が半角と全角で混在している（「${shown}」）— rustdoc はリンクと認識せずリテラル出力し、broken_intra_doc_links も黙る。両方を ASCII の [ ] にすること`

## 検査ヘッダに書く射程と死角

- **守る行**: `docs/comment-guidelines.md`「名指しと正本の指名」の表の「production の `///` `//!`」行（着地を検査する ○）。× の 2 行（`#[cfg(test)]` 内・`//` インライン）は規範上の保証が無い面であり、この検査が赤にする理由も無い
- **`#[cfg(test)]` は絞らない**: `mod tests` の入れ子と個別アイテムの `#[cfg(test)]` をテキスト走査で判定するのは非自明で、誤った境界は沈黙側へ倒れる。`#[cfg(test)]` 内の検出は「直して困らない過剰」として受容する（表の × は「保証が無い」であって「書いてよい」ではない）
- **沈黙側の死角**: `/** … */` ブロック doc（実ツリー 0 件・2026-09-02）／`//` インラインの `[`x`］`（表で ×）／`.md` `.mjs` `.ps1`（intra-doc link の概念が無い）／全角同士 `［…］`（rustdoc も人もリテラルとして読むので誤りではない）／`〔〕` `【】` 等の他の括弧（ASCII `[` `]` の全角互換文字ではなく、打鍵ミスで `[`…`〕` が生じる経路が無い。issue の実例も `］` だけ）／行を跨ぐ角括弧（rustdoc も解決しない）／`git` 未追跡の `.rs`（`makeSnapshot` が歩けば入る。現行ツリーでは 0 件で、実装後の 0 件確認は `git grep` ではなく検査自身の findings と `checked` で行う）
- **赤側の死角**: rustdoc コードフェンス内の混在形（`linesOfComments` がフェンスをマスクしない。今日 0 件）／散文の括弧書きで `（…［…］…）` のように意図的に混ぜた形（実ツリー 0 件。出たら全角対にするか `` ` `` で包む）

## 実装順序

1. 検査モジュールとテストを書き、`npx vitest run scripts/governance/checks/G-fullwidth-doc-link-bracket.test.mjs` を緑にする（Red → Green: 先に issue の実例を fixture にした赤テストを書く）
2. `evidence.mjs` の要約行に 1 句足し、`npm run governance:check` で finding 0 件・`checked` が 0 でないことを確認
3. `SCAN_SCOPED` と `docs/hooks.md` の表を同じ変更で足し、`G-edit-findings-table` が緑のままであることを確認
4. `docs/comment-guidelines.md` に 1 文追記
5. フォールトインジェクション: `src-tauri/src/egui_shell/window_coordinator.rs` の既存 production `///` 行 1 つ（**`impl` 内のインデント済みの行を選ぶ**——行頭 `///` だけで測ると `trimStart()` の欠落を見逃す）の `]` を `］` に置換 → `npm run governance:check` が exit 1 で当該 file:line を名指すことを確認 → `git checkout -- <file>` で戻し `sha256sum` が HEAD の blob と一致することを確認。**同じ注入を `#[cfg(test)]` 内の `///` にも当て、絞らない設計どおり赤になることを確認**（issue の追加条件: 注入位置で結論が逆になる設計ではないことの実測）
6. `npm test`（vitest 全件）と `npm run governance:check` を最終実行

## 不変条件と異常系

- 対象文書が読めない（`snapshot.read` が null）→ `G-folded-code-spans` と同じく「母集団の欠落」finding を 1 件出す（沈黙させない）
- `linesOfComments` へ渡す族は `"js"` の逐語。族の取り違え（`"hash"`）はテストの `///` fixture が赤にする（`lib.mjs` の doc が要求する隣接テストの責務）
- 現行ツリーで 1 件でも finding が出たら述語の誤りとして扱う（`［` `］` は全域 0 件と実測済み）
- `checked` が 0 なら「見ていない」——`governance-check.mjs` の evidence 経由で数字として残る

## テスト方針と検証コマンド

- 単体: 赤 5 本（issue 実例の 2 件同居行・逆向き・素のリンク形・`//!` 行・**4 スペースでインデントされた `///` 行**〔`impl` ブロック内の doc を模す。`trimStart()` を落とす変異で唯一これが赤になる〕）／緑 5 本（正しい形・全角対・バッククォート内の角括弧・`//` インライン・`.rs` 以外のファイル）／証跡 2 本（`checked` が対の総数・読めない文書で finding）
- `npm run governance:check`（カテゴリ F）
- `npm test`（`G-edit-findings-table.test.mjs` と `edit-findings.test.mjs` が SCAN_SCOPED の変更を受ける）
- フォールトインジェクション（上の順序 5）。結果は PR 本文へ file:line と exit code を書く

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（製品挙動に触れない）
- `docs/hooks.md`: 表 1 行（機構が照合）
- `docs/comment-guidelines.md`: 1 文
- `docs/build-commands.md`: 不要（カテゴリ F のコマンドは変わらない）
- PR 本文: manifest の要求により `G-fullwidth-doc-link-bracket` を逐語で書く

## 作業項目

### Phase 1 — 検査本体

- [x] `G-fullwidth-doc-link-bracket.test.mjs` に issue の実例 fixture を置いた赤テストを書き、落ちることを確認する（スタブ実装に対し 6 failed / 7 passed を実測）
- [x] `G-fullwidth-doc-link-bracket.mjs` を実装し、単体テスト全件を緑にする（13 passed）
- [x] ヘッダに守る表の行・`#[cfg(test)]` を絞らない理由・沈黙側と赤側の死角を書く

### Phase 2 — 配線

- [x] `assembleEvidence` に `docLinkBrackets` の 1 句を足し、`evidence.test.mjs` の `complete()` fixture へ同じキーを足し、`governance:check` の要約に件数が出ることを確認する（「doc の角括弧対 755 件」）
- [x] `SCAN_SCOPED` へ 1 行、`docs/hooks.md` の表へ 1 行、`edit-findings.test.mjs` へ配線固定 1 本を同じ変更で足し、`G-edit-findings-table` が緑であることを確認する（「reminder 表 11 行」）
- [x] `docs/comment-guidelines.md`「名指しと正本の指名」に機構の名指しを 1 文足す
- [x] （実装中に判明）`G-edit-findings-table.test.mjs` の `ALL` 配列は判定名の手書きの写しで、`SCAN_SCOPED` へ足すと 9 本落ちる。新判定名を追加した（独立導出も敵対的調査もこのファイルは挙げていない——「写し」は検査本体ではなくテストの fixture 側に居た）

### Phase 3 — 実測

- [ ] production `///` へ注入して `governance:check` が exit 1 で当該行を名指すことを確認し、SHA256 照合で巻き戻す
- [ ] `#[cfg(test)]` 内の `///` へ注入しても赤になることを確認し、同様に巻き戻す
- [ ] `npm test` と `npm run governance:check` が緑で、現行ツリーの finding が 0 件・`checked` が 0 でないことを確認する

## 未確定（実装前に潰す）

- [x] `linesOfComments("js")` が返す行は trim 済みか — **trim 前の `raw`**（`lib.mjs` の `out.push([i + 1, raw])`。主エージェントと敵対的調査が独立に実測・2026-09-02）。述語に `trimStart()` を入れ、インデント済み fixture を赤テストに置いた
- [x] 検査 ID の列挙（写し）が `checks/` 以外に無いか — 無い。`git grep G-folded-code-spans` の `checks/` 外の一致は散文の言及と `edit-findings.mjs` の import だけで、列挙表は存在しない（敵対的調査も同結論）
- [x] evidence の新キーを要約行へ載せないと赤になる経路の有無 — 無い。`evidenceView` が finding にするのは「読んだが未記録」だけで、「記録したが未読」は沈黙する（`evidence.mjs` 冒頭の宣言・敵対的調査が裏付け）。ゆえに載せる作業は Phase 2 の任意の位置でよいが、#497 の趣旨で必ず載せる

## plan-review 結果

- リスク: 高（ガバナンス文書 `docs/hooks.md` / `docs/comment-guidelines.md` と edit-time 判定 `SCAN_SCOPED` を変更する）
- レビュー方式: 独立導出1体（Step 2b。`workspace/plan-review-1172-derive.md`）
- エージェント数: 1

### 要対処
- `scripts/governance/evidence.test.mjs` の `complete()` fixture が変更一覧から漏れていた — 計画を修正（変更一覧・Phase 2 に追加） — 再照合: `evidence.test.mjs:8-31` の `complete()` は全キーを持つ袋で「緑: すべて記録済み」を assert しており、`assembleEvidence` が新キーを読めば未記録の finding で落ちる

### 軽微
- `edit-findings.test.mjs` の配線固定 1 本（`G-folded-code-spans` の赤ケース `edit-findings.test.mjs:151` と同型）— 計画に追加
- `SCAN_SCOPED` の母集団を `allHeadingRefDocs` から `headingRefSourceDocs`（`.rs` 全件・`lib.mjs:648`）へ — 検査の射程と母集団の宣言を一致させるため採用
- 導出は `docs/comment-guidelines.md:100` の段落を「機構が黙る」から検査の着地先へ**書き換え**と読んだ。計画は 1 文追記。rustdoc と `broken_intra_doc_links` が黙る事実は変わらないので、既存文は残し「`G-fullwidth-doc-link-bracket` が赤にする」を続ける追記で足りる

### 未検証
- `git grep -P "[［］]"` が Bash ツール経由で空を返す原因（導出が ⚠️ で報告）。結論（0 件）は node のバイト走査と敵対的調査の独立測定で再現しているため計画には影響しない。実装後の 0 件確認は `git grep` に依らず検査自身の findings と `checked` で行う

### 判断
- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b 独立導出）
- エージェント数: 2（3b 敵対的調査 1・Step 2b 独立導出 1）
- 要対処: 1 件（`evidence.test.mjs` の fixture を変更一覧と Phase 2 へ追加）
- 未検証: `git grep -P` の挙動（結論に影響せず。上記）

## 人間レビュー

- [x] 承認済み — 2026-09-02 / 問い: "次のどちらかをお選びくださいまし。1. `workspace/plan.md` に注釈を書き込む 2. この計画を明示的に承認する" / 回答: "OK"
