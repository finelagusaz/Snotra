# 計画 — issue #1504 retrospective の運用とタイミングを見直す

## 目的

`/retrospective` を「毎サイクルの締め」から「人が必要を感じたときに、まとまった範囲を振り返る道具」へ定義し直す。それに合わせて、作業の入口で `RETROSPECTIVE.md` を「前回のサイクル」として読む手順をやめる。

## 受け入れ条件

1. 振り返りの単位（範囲）と、走らせる契機が `.claude/skills/retrospective/SKILL.md` の 1 か所に書かれている。ルート `CLAUDE.md` のスキル表はそれと矛盾しない
2. Step 1 が、複数 PR にまたがる範囲でも、マージ後に走らせても、範囲の PR とコミットを列挙できる。既定の起点は、前回の振り返りが `RETROSPECTIVE.md` に記録した終点である
3. `AGENTS.md`「開発ワークフロー」・`/start-issue`「3a. 調査を書く」・`/implement`「1b. 分岐」から、`RETROSPECTIVE.md` を入口で読む手順が消えている
4. `/retrospective` の Step の番号と見出しは変わらない（health-check 7d が `/retrospective`「Step 6」を名指す）
5. `npm run governance:check` が通る

## 変更するファイル

| ファイル | 対象 | 変更 |
|---|---|---|
| `.claude/skills/retrospective/SKILL.md` | フロントマターの `description`・`argument-hint` | description を「まとまった範囲を振り返る必要を人が感じたときに使用」へ。argument-hint を範囲の与え方（PR 番号の一覧・起点の sha・日付。省略時は前回の終点から `origin/main` まで）へ |
| 同 | 冒頭（「対象: $ARGUMENTS」の前後） | 単位と契機の定義を置く: 単位は「前回の振り返りの終点から今までに main へ入った変更（引数で絞れる）と、未マージのこのブランチ」。契機は人の判断で、毎 PR では回さない。経緯（当初は `/implement` と対で毎回回し、手順の安定で頻度が下がった。積み残し・却下案・範囲外作業は今は `/implement` と PR 本文の「残したもの」が受け持つ）を 1〜2 文で添える。あわせて、本文に残る「サイクル」（Step 4 の「サイクルを越えて残るタスク」、Step 5 の「次のサイクルで上書き」など）が、この範囲 1 回分を指すと定義の中で一言述べる（語は置き換えない） |
| 同 | 「Step 1 — サイクルの全体像を把握」 | 範囲の決め方とコマンドを書き直す（下の「Step 1 の新しい形」）。見出しの文言は変えない |
| 同 | 「Step 5 — RETROSPECTIVE.md の上書き（教訓のみ）」 | フォーマットに範囲の行（`範囲: <起点>..<終点>（YYYY-MM-DD）`）を足す。終点は Step 1 で取った `origin/main` の sha。`RETROSPECTIVE.md` は振り返りの記録であって、作業の入口では読まれないことを 1 文で書く |
| `RETROSPECTIVE.md` | 見出し直下 | 範囲の行を 1 行足す（`範囲: 〜..d2a0b9a4（2026-11-03）`）。次回の既定の起点を与えるため。本文は触らない |
| `AGENTS.md` | 「開発ワークフロー」冒頭の 1 文 | 削る |
| `.claude/skills/start-issue/SKILL.md` | 「3a. 調査を書く」の読む文書の列挙 | `RETROSPECTIVE.md` を外す |
| `.claude/skills/implement/SKILL.md` | 「1b. 分岐」の「計画なしの経路でだけ行う調査」 | `RETROSPECTIVE.md` を読む項目を削る |
| ルート `CLAUDE.md` | 「チーム憲章」2 項目め | 恒久的な情報の置き場所の列挙から `RETROSPECTIVE.md` を外す（上書きされるファイルであり、「RETROSPECTIVE.md の運用」と食い違っていた） |
| ルート `CLAUDE.md` | 「利用できるスキル」表の `/retrospective` 行 | 「人が判断したときの、範囲を指定した振り返り・教訓の配置」 |

### Step 1 の新しい形

1. 範囲を決める（`git fetch origin` の後）
   - 引数が PR 番号の一覧なら、その PR だけを対象にする（`gh pr view <N> --json title,body,mergeCommit,comments,reviews`）
   - 引数が sha なら、それを起点にする。日付なら `git rev-list -1 --before=<日付> origin/main` で sha に直して起点にする
   - 引数が無ければ、`RETROSPECTIVE.md` の範囲の行の終点を起点にする
2. 起点を確かめる: `git cat-file -e <起点>^{commit}` で存在を、`git merge-base --is-ancestor <起点> origin/main` で `origin/main` の祖先であることを確かめる（存在しない sha では後者が 128 で落ちるので、この順）。範囲の行が無い・起点が存在しない・祖先でない、のどれかなら、起点をユーザーに尋ねる（このスキルで質問してよい 2 つめの例外として冒頭の文に足す）
3. 列挙する
   ```bash
   git log --oneline <起点>..origin/main
   git diff --stat <起点>...origin/main
   git log --oneline origin/main..HEAD
   ```
   `git rev-list --count <起点>..origin/main` と `git rev-list --count origin/main..HEAD` がどちらも 0 なら、振り返る対象が無いと報告して止まる。squash マージなので、`origin/main` の各コミットの件名末尾の `(#N)` が PR 番号である（`(#issue) (#PR)` と 2 つ並ぶときは後ろが PR）。PR ごとに本文（とくに「残したもの」）とレビューを `gh pr view` で読む
4. 計画との比較: マージ済みの PR では、`workspace/plan.md` は main に残らない（squash で撤去コミットも main に入らない。`16c84e92`（#1508）が `origin/main` の祖先でないことを実測）。そのため PR 本文を一次資料にする。`workspace/plan.md` は、未マージのブランチにあるときだけ読む

### 範囲の行の更新（Step 5）

- 書く形: `範囲: <起点の完全 sha>..<終点の完全 sha>（YYYY-MM-DD）`。終点は Step 1 で取った `git rev-parse origin/main`
- 終点を進めてよいのは、範囲が前回の終点から `origin/main` まで途切れずに覆ったとき（引数なし、または起点が前回の終点と同じかその祖先）だけである。PR 番号の一覧や、前回の終点より後の起点で回したときは、前回の範囲の行をそのまま写す（対象外の PR を済み扱いにして、次回の既定の範囲から落とさないため）
- 未マージのブランチの変更（`origin/main..HEAD`）と、この振り返り自身の PR は、記録した終点より後に main へ入るので、次回の範囲に現れる。読み飛ばしてよいと一言添える

## 実装の順序

1. `.claude/skills/retrospective/SKILL.md`（定義・Step 1・Step 5）
2. `RETROSPECTIVE.md` に範囲の行
3. 入口の 3 か所（`AGENTS.md`・`/start-issue`・`/implement`）
4. ルート `CLAUDE.md` の 2 か所
5. 検証

## 不変条件と異常系

- `/retrospective` の Step 番号と見出しを変えない（health-check 7d の参照）
- 範囲の行が無い・起点が存在しない・祖先でない、空の範囲、部分範囲での終点の扱い: 「Step 1 の新しい形」と「範囲の行の更新（Step 5）」に書いたとおり SKILL.md へ書く（Phase 1 の作業項目に含める）
- `AGENTS.md`「RETROSPECTIVE.md の運用」は触らない（持ち越しタスクを置かない規則は真のまま）

## テスト方針と検証コマンド

規範文書とスキルの変更で、実行コードは無い。

- `npm run governance:check`（G-references など）
- `git grep -n "RETROSPECTIVE"` を凍結記録（`docs/superpowers/`・`docs/design/`・`docs/adr/`）を除いて打ち、入口で読ませる記述が 0 件、残る参照がすべて「記録としての `RETROSPECTIVE.md`」か歴史の記述であることを確かめる
- Step 1 の新しいコマンドを実際に打つ: `RETROSPECTIVE.md` に足した終点 `d2a0b9a4` を起点に `git log --oneline d2a0b9a4..origin/main` が #1360 以降のコミットを列挙し、`git merge-base --is-ancestor d2a0b9a4 origin/main` が真であること。わざと祖先でない sha（このブランチの未マージのコミットなど）で偽になること
- `git grep -n '/retrospective「Step'` で参照先の見出しが残っていることを確かめる

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（製品の挙動を変えない）
- `docs/`: 不要（開発プロセスの正本はスキルとルート文書）

## 作業項目

### Phase 1 — `/retrospective` の定義と Step 1・Step 5

- [ ] description と argument-hint を書き直す
- [ ] 冒頭に単位と契機の定義を置き、質問してよい例外に「範囲の起点が決まらないとき」を足す
- [ ] Step 1 を「Step 1 の新しい形」の 1〜4（起点の確かめ・空の範囲で止まる・日付の sha への変換を含む）のとおり書き直す（見出しは変えない）
- [ ] Step 5 のフォーマットに範囲の行を足し、「範囲の行の更新（Step 5）」の 3 項（書く形・終点を進めてよい条件・次回に現れるもの）と、入口では読まれない記録であることを書く
- [ ] `RETROSPECTIVE.md` の見出し直下に範囲の行を足す（`範囲: 〜..<d2a0b9a4 の完全 sha>（2026-11-03）`）

### Phase 2 — 入口と規範文書

- [ ] `AGENTS.md`「開発ワークフロー」冒頭の 1 文を削る
- [ ] `/start-issue`「3a. 調査を書く」の列挙から `RETROSPECTIVE.md` を外す
- [ ] `/implement`「1b. 分岐」の調査項目から `RETROSPECTIVE.md` を削る
- [ ] ルート `CLAUDE.md`「チーム憲章」の列挙から `RETROSPECTIVE.md` を外す
- [ ] ルート `CLAUDE.md` スキル表の `/retrospective` 行を書き直す

### Phase 3 — 検証

- [ ] `npm run governance:check` が緑
- [ ] 凍結記録を除いた `git grep -n "RETROSPECTIVE"` の全件を分類し、入口で読ませる記述が 0 件
- [ ] Step 1 のコマンドを `d2a0b9a4` 起点で打ち、祖先判定が真であること、このブランチの未マージのコミット（workspace のコミット）で偽になること、存在しない sha で `cat-file -e` が非 0 になることを確かめる。日付 `2026-11-04` が `rev-list -1 --before` で sha に変わることも確かめる

## PR について

- `.claude/skills/**` と規範文書（`AGENTS.md`・ルート `CLAUDE.md`）を触るセーフティネットの変更なので、PR 本文の冒頭に「マージは合意してから」と書く（ルート `CLAUDE.md` 最重要ルール 2）

## 範囲外（PR 本文の「残したもの」へ）

- `snotra-core/src/search/build.rs` の `new_from_tree` の doc が「`RETROSPECTIVE.md` が前サイクルの取りこぼしとして記録している」と書く。上書きで既に偽だが、本変更が偽にしたものではない

## 未確定（実装前に潰す）

- [x] （裁定）作業の入口で `RETROSPECTIVE.md` をどう扱うか — 問い: 「入口で読むのをやめる (a)／読むのは続け古さを見出しに書く (b)／ファイル自体を廃止する (c)」。回答: 「(a) で進めて」
- [x] （裁定）振り返りの契機と起動の主体 — 私の当初の推し「`/merge-pr` 成功時に促す」に対し、ユーザーが経緯を説明（「このスキルはもともと implement とセットで運用していた…開発方法が安定してきて…だんだん頻度が下がってきた」）。頻度低下は安定化によるものと受け、人の判断のまま・user 起動専用を保つ
- [x] 前回の振り返りの終点をどう特定するか — `RETROSPECTIVE.md` に範囲の行を書く（案 A）。見出し行の変化を追う `git log -G` は、見出しの語の直しで壊れるので採らない。今のファイルには行を 1 行足して初回の起点を与える（初回だけ別経路を持たない）
- [x] マージ前のブランチを範囲に含めるか — 含める（`origin/main..HEAD`）。空なら何も出ないだけで害が無い
- [x] Step 6・7 を別の周期へ切り出すか — 切り出さない。振り返り自体が「必要を感じたときにまとめて回す」ものになるので、メモリの鮮度と health-check を同じ契機で回して不自然ではない。health-check は単独でも user が起動できる

## plan-review 結果

- リスク: 高（skills・ガバナンス文書を変更する）
- レビュー方式: 計画準拠レビュー1体（issue に無い人間の裁定が前提にあるため Step 2。結果は `workspace/plan-review-retro-cadence.md`）
- エージェント数: 1

### 要対処
- マージ後は `workspace/plan.md` が main に無い — 計画の修正（PR 本文を一次資料にする）— `git merge-base --is-ancestor 16c84e92 origin/main` が 1、`git ls-tree origin/main workspace` が 0 件
- 部分範囲で終点を進めると取りこぼす — 計画の修正（「範囲の行の更新（Step 5）」に終点を進めてよい条件）— 指摘の論理を確認
- 異常系が Step 1 と作業項目に入っていない・存在しない sha で is-ancestor が 128 — 計画の修正（Step 1 の 2・3、Phase 1）— 実走: 存在しない sha で `cat-file -e` が 128
- 日付引数の sha への変換が未定義 — 計画の修正（`git rev-list -1 --before`）— 実走: `2026-11-04` → `48d0f981`

### 軽微
- 完全 sha・`(#issue) (#PR)` の後ろが PR・次回の範囲に自身の PR が現れる — 計画へ反映
- `CLAUDE.md` の `/health-check` 行と health-check の description（⚠️）— 採らない。単独起動もでき、本変更で偽にならない（research.md の採否と同じ）
- 96 PR を読む重さ — 採らない（必須でない）
- 「マージは合意してから」— 計画の「PR について」へ反映

### 未検証
- `refs/pull/N/head` が残るか — PR 本文を一次資料にしたので不要

### 判断
- 実装着手: 人間の承認待ち。修正は要対処の対処案をそのまま採ったもので、新しいコマンドは主エージェントが実走して確かめた（上の根拠）。独立レビューの再実行はしていない

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 4 件。すべて計画へ反映（Step 1 の新しい形・範囲の行の更新・Phase 1/3 の項目）
- 未検証: なし
- check スキル: なし（永続形式・状態・対称ペア・関数の新設に当たらない。`governance:check` は Phase 3）
- 散文の全件列挙: 語 4 / 総件数 29 行 / ★未掲載 2（計画へ足した 0: `.claude/rules/governance-docs.md` の #1222 の項は歴史の記述、`snotra-core/tests/path_query_cost.rs` は「参照先にしない」で真のまま） / ☆行未掲載 0（計画へ足した 0）

## 承認

- [x] 承認済み — 2026-11-22 / 問い: "workspace/plan.md の計画を承認なさいますか？" / 回答: "承認して実装"
