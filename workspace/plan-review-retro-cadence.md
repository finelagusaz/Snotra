# plan-review: issue #1504（計画準拠の独立レビュー・観点 1 と 2 のみ）

対象: workspace/plan.md。コードは編集していない。

## 要対処

1. 【観点1・Step 1 の穴】計画比較の手順が、マージ後には動かない。plan.md「Step 1 の新しい形」3 は「撤去の前のコミットの `workspace/plan.md` を `git show` で読む（`/implement` Step 4 の撤去コミットの親）」とする。しかし squash マージなので、撤去コミットは main に入らない。実測: `git merge-base --is-ancestor` で 16c84e92（#1508 の workspace 撤去）・f04e99e0（#1400）・7863b2d6（#1499）はどれも `origin/main` の祖先でなく、`git ls-tree origin/main` に `workspace` は無い。受け入れ条件 2「マージ後に走らせても」と衝突する。マージ後の主な資料は PR 本文（とくに「残したもの」）とレビュー。plan.md の取得は、PR の head（`refs/pull/<N>/head` を fetch するなど）の探索になり、ブランチ削除後も残るかは未確認。plan.md に代案が無く、実装者が判断を足すことになる。対処案: 3 を削るか、「PR 本文を一次資料とし、plan.md は未マージのブランチ（`origin/main..HEAD`）にあるときだけ読む」へ変える。

2. 【観点1・Step 1 の穴】範囲の行（Step 5）と、引数の形が噛み合わない。引数が PR 番号の一覧（連続でない部分集合）や、起点の sha・日付のとき、Step 5 が「終点は Step 1 で取った `origin/main` の sha」を `範囲:` の行へ書くと、対象にしなかった PR まで「振り返り済み」として次回の既定の起点が進み、取りこぼす。受け入れ条件 2「取りこぼさない」に反する。対処案: `範囲:` の行を更新してよいのは、引数が無い（前回の終点から `origin/main` まで）か、起点だけを後ろへ動かした場合に限る。部分集合では行を触らない（または旧値を保つ）と、Step 5 に 1 文で書く。

3. 【観点1・計画の抜け】異常系の処理が、作業項目に落ちていない。plan.md の「不変条件と異常系」にある祖先判定・空範囲の停止は、Phase 1 の項目「Step 1 を『Step 1 の新しい形』のとおり書き直す」が参照する「Step 1 の新しい形」の節（plan.md 30〜44 行）に入っていない（コマンドブロックにも無い）。そのまま実装すると SKILL.md へ書かれない。「範囲の行が無い」は書かれているが、「祖先でない」「空」は無い。Phase 1 に明示の項目を足すか、「新しい形」へ移す。あわせて次を決める。
   - `git merge-base --is-ancestor` は、sha が存在しないとき偽（1）でなくエラー（128）で終わる。起点がユーザー指定の sha のとき、`git cat-file -e <起点>^{commit}` のような存在確認が先に要る
   - 空範囲の判定コマンドが無い。`git rev-list --count <起点>..origin/main` と `origin/main..HEAD` の両方が 0、と書く
   - 起点を尋ねる「例外」を冒頭の文へ足す件は書いてあるが、「祖先でない」「sha が無い」でも尋ねるのか、が不明

4. 【観点1・Step 1 の穴】日付の引数の列挙が定義されていない。計画は「日付は `git log --since`」とだけ書く。Step 1 の 4 コマンドは `<起点>` に sha を要求する（`git diff --stat <起点>...origin/main`）ので、日付は sha へ変換する必要がある。例: `git rev-list -1 --before=<日付> origin/main`。実装者の追加判断になる。祖先判定の対象にもなる（変換結果は常に祖先なので自明だが、書いておく）。

5. 【観点2・偽になる散文】ルート `CLAUDE.md` の `/health-check` 行（CLAUDE.md:52「サイクル完了後の governance:check + 意味的整合」）と、同スキルの description（.claude/skills/health-check/SKILL.md:3「大きなサイクル完了後や定期メンテナンス時」）が、変更ファイルの一覧に無い。`/retrospective` の単位を「毎サイクル」から「人が選んだ範囲」へ改めても、`/health-check` は Step 7 から「サイクル末」に呼ばれる前提のまま。ただし `/health-check` 自身は単独起動も可能と書かれていて、現在形の主張が偽になる、とまでは言えない。⚠️ 要否は判断が分かれる。少なくとも CLAUDE.md の 52 行が 53 行の新しい文言と並んで読めるか、実装者に確かめさせる。

## 軽微

- 範囲の行の終点の表記が未定義。短縮 sha（`d2a0b9a4`）か完全か。短縮はリポジトリが大きくなると曖昧になりうる。`git rev-parse origin/main` の完全 sha を書く、と Step 5 に決めておく
- ⚠️ 振り返りを未マージのブランチで回すと、`origin/main..HEAD` のコミット（振り返り自身の PR や直前の変更）は、記録した終点（`origin/main` の sha）の外にある。マージ後に次回の範囲へ入る。重複して振り返る分は害が小さいが、振り返りを回した PR 自体が次回の範囲に混ざる。「範囲の行の PR は範囲に入っていない」と添えるか、無視してよいと書く
- `(#N)` の読み方: 計画は「件名末尾の `(#N)` が PR 番号」。実測 `git log d2a0b9a4..origin/main` の 96 コミットのうち、`(#N)` で終わらない件名は 0 件、`(#issue) (#PR)` の 2 つ並びは 54 件。末尾だけが PR という読みは成り立つ。ただし「2 つ並ぶときは後ろが PR」と明記すると実装者の判断が減る
- 96 コミット分の PR を `gh pr view` で 1 件ずつ読むのは重い。人の裁定のうえで、範囲の PR が多いときは「残したもの」の節だけを読む、などの縮め方を書けるが、必須ではない
- Phase 3 の検証「わざと祖先でない sha（このブランチの未マージのコミットなど）で偽」は、このブランチに未マージのコミットがまだ無いと成立しない。検証時にコミットがあることを確かめる

## 未検証

- 観点1: 上記 1 の「PR の head が残るか」。`git ls-remote origin 'refs/pull/*/head'` で #1508 などを引けるかは打っていない（`gh` の認証・ネットワーク前提）
- 観点1: `git log --oneline d2a0b9a4..origin/main` が #1360 以降を列挙することは確認した（`d2a0b9a4` は commit で、`origin/main` の祖先、96 コミット）。`git diff --stat <起点>...origin/main` は未実行。3 点形は起点が祖先なら merge-base が起点と一致するので 2 点形と同じ結果になるはずだが、実行して確かめていない。AGENTS.md の起点・2 点/3 点形の規則（`log` は 2 点、`diff` は 3 点、起点は fetch 後の `origin/main`）には、計画のコマンド 4 本とも合っている（`origin/main..HEAD` の log は 2 点形で正しい）
- 観点2: 凍結記録（`docs/superpowers/`・`docs/design/`・`docs/adr/`）と `workspace/` を除いた `git grep -n RETROSPECTIVE` の件数は 14 行（`.claude/rules/governance-docs.md:32`・implement:54・retrospective の 3・86・88・106 行ほか・start-issue:47・AGENTS.md:31, 85・CLAUDE.md:26, 53・build.rs:413・path_query_cost.rs:275）。計画の変更ファイルの一覧は、入口 3 か所（AGENTS.md:31・start-issue:47・implement:54）、CLAUDE.md:26, 53、retrospective 内を覆う。残りは、governance-docs.md:32（#1222 の歴史）、AGENTS.md:85, 87（持ち越しタスクを置かない規則。計画は触らないと明記）、build.rs:413（計画が範囲外へ明記）、path_query_cost.rs:275（「参照先にしない」という真のまま）で、偽にならない。この分類は私が読んだ範囲であり、全件を完全に読んだわけではない
- 観点2: health-check の 7d が `/retrospective`「Step 6」を名指す箇所（health-check/SKILL.md:54）は、計画が Step の番号・見出しを変えないので壊れない。retrospective 本文の「サイクル」の用例（Step 4・Step 5・Step 7・出力の 1）は、計画が定義で一言述べて語を残す方針。偽になるかは、定義の文言次第で未確定
- 追加: 計画に、ルート `CLAUDE.md`「最重要ルール」2（セーフティネット変更のマージは合意してから、PR 本文の冒頭に「マージは合意してから」）が書かれていない。`.claude/skills/**`・`AGENTS.md`・`CLAUDE.md` を触るので該当する（safety-nets.md の `paths` に `.claude/skills/**` がある）。実装の PR 本文の作法であり、計画の要件ではないが、書いておくと忘れない
