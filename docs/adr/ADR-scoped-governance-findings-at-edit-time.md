# ADR-scoped-governance-findings-at-edit-time: 索引と参照実在の不整合を、編集したファイルに帰属させて編集直後に知らせる

## 文脈

`governance-check` の CI ログ 45 日分（`ci.yml` の run 1000 本・2026-07-05 〜 08-19）を集計したところ、
**job が赤くなったのは 9 回、findings は 15 件**で、**12/15 が `G-module-index` と `G-references` の 2 本**だった。

**CI ログは「検査の発火実績」ではなく「ローカルの輪から漏れた残り」しか映さない**（PR 前に
`npm run governance:check` を回す運用なので、そこで捕まえた分は痕跡を残さずに消える）。ゆえに
「CI で 0 件だから不要」は使えない基準である——`G-heading-refs` は CI で 0 件だが、同じ期間に main の
履歴では 48 コミットが参照ラベルを追随させている。逆に、**CI へ漏れてくる検査は「ローカルの輪から
漏れる種類」を名指ししている**。

## 決定

`.rs` と `.md` の編集直後に、PostToolUse hook が「**その編集に帰属する**索引漏れ・参照の不在」を
**reminder** として出す。判定は `scripts/governance/edit-findings.mjs` が持ち、hook は subprocess で呼ぶ。
**合否は持たない**（`ADR-dependents-reminder-at-edit-time` と同型）。**CI の `governance-check` job は
変更しない。**

`.rs` の Edit も対象にし、reminder は `systemMessage`（人間向け）と `additionalContext`（エージェント向け）の
両方へ流す。この 2 点は 2026-08-19 にユーザーが名指しで承認した。

## 検討した代替案と却下理由

- **gate にする（`selectChecks` へ検査 id を足し、exit code を動かす）**: 却下。**新規 `.rs` を書いた直後は
  索引が未更新であるのが正常な作業順**なので、gate にすると構造的に必ず赤くなる——
  「**永久に赤いゲートはゲートが無いのと機能的に同じ**」（`ADR-declared-colors-over-modal-color`）。
  加えて gate 枝は `docs/hooks.md` の発火一覧表・`BUDGETS`・`buildCommand` の repro 正規化・
  `G-hook-fires` の `sawEmptyRow`（`.md` が id を発火すると現行の `（なし）` 行が空でなくなる）まで
  玉突きで動かす。reminder 枝ではこの 4 点セットが無傷である。
- **`governance:check` 全体を毎編集で回す**: 却下。ただし**却下の理由は費用ではない**。実測すると
  `node scripts/governance-check.mjs` は 0.65 秒（`npm run` 経由で 1.1〜1.3 秒）で、すでに受け入れている
  `dependentsReminder` の hook 全体 330〜386 ms と同じ桁である。**却下の理由は帰属**——全体を回すと
  「今の編集と無関係な既存の赤」が毎編集鳴り、慣れを作る（`partial-automation-habituates` の型）。
  絞れば鳴るのは「今の編集が壊しえた分」だけになる。
- **絞らずにその crate の findings を全件出す**: 却下。`checkModuleIndex` は crate 内**全ファイル**を
  照合するので、未解消の索引債務が 1 件でも残っている間は**その crate への無関係な編集のたびに同じ
  reminder が繰り返し出る**（敵対的レビューが指摘）。ゆえに `.rs` の編集では finding のメッセージが
  編集パスを**パスとして完結した形で**名指すものだけへ絞る。代償は順方向（索引に書かれた実在しない
  ファイル名）が `.rs` の編集では出ないことで、そちらは `<crate>/CLAUDE.md` の編集時と CI が引き取る。
- **`dependents.mjs` へ相乗りさせて subprocess を 1 本にする**: 却下。責務が別である（あちらは
  「節の中身が変わったこと」を知らせる計器、こちらは検査の判定を借りる）。相乗りで浮くのは
  node 起動 1 回分（70〜130 ms）だが、`.md` の編集にしか効かない——`.rs` では相手が起動しないためである。
- **`G-module-linkage`（`mod` 宣言）も同時に前倒しする**: 却下。CI 45 日で 0 件＝この issue が採った
  選定基準（「CI へ漏れてくる検査は、ローカルの輪から漏れる種類を名指ししている」）に掛からない。
  **代償として、索引だけが編集時に見えて `mod` は見えない非対称が生まれる**——`AGENTS.md` と
  `docs/hooks.md` がこの非対称を名指しする。
- **`systemMessage`（人間向け）だけに流す**: 却下（ユーザー裁定 2026-08-19）。#629/#630 の索引更新漏れは
  **エージェント**の実行漏れであり、人間向けの面だけに出すと「機構を足したのに当の失敗主体に見えない」で
  終わる。**なお過去ログではこの分岐は決められなかった**——#629/#630 は reminder 機構の実装前の出来事で、
  「人間向けの面に出したが見落とされた」のか「そもそも機構が無かった」のか切り分けられない。
  設計原則（失敗主体に届ける・`TS_LIKE` の情報行という先例）で決めた。
- **crate を `rel.startsWith(crate + "/")` で導出する**: 採らなかったが、**この却下の理由付けは 2 度書き直した。
  その経緯自体がこの ADR の記録に値する。**
  - 当初は「`<crate>/tests/*.rs` を拾って索引に載る筋合いの無いファイルへ reminder を出す」と書いた。
    委譲レビューの変異注入が前方一致版を**緑のまま通し**、実測（実ツリーの `.rs` 101 件で出力の差分 0 件・
    変わるのは `checkModuleIndex` の呼び出し回数 95 → 101）で偽と分かった。後段の `attributesTo` が
    同じ `rel` を落とすためである。
  - そこで「費用の最適化であって正しさの門ではない」と書き直したところ、**それも実装より強かった**——
    順方向の finding は索引の token をそのままメッセージへ載せるので、**`rel` がツリーに実在しない
    呼び出し**では前方一致版だけが誤って帰属させる（実測: 索引が `<crate>/tests/ghost.rs` を持ち実ファイルも
    `rel` も無いとき、現行 0 件 / 前方一致版 1 件。実ツリーの索引に crate 名から始まる token は 0 件ゆえ
    今日の露出は無く、hook 経由の `rel` は編集直後の実在ファイルなのでこの経路にも来ない）。
  - **連鎖を止めたのは、条件を書き足すことではなく同値主張をやめることだった**（`#1091` の型）。
    今の doc が言うのは「測った母集団でこう出た」「この判定の形を固定する検知器は無い」の 2 つだけである。

## 帰結

- **`.rs` の編集経路に新しく費用が乗る。** `dependentsReminder` は `.md` 判定を先頭に置いて `.rs` に
  一切の費用を載せていない設計だったが、索引を見るのが目的である以上ここは**意図的に破る**。
  代償は最頻操作への +70〜130 ms（同じ編集で走る `cargo clippy` は秒オーダー）。
- **旧 `isSourceFileWrite` を置き換えた。** かつては「`.rs` を Write した」という低頻度シグナルだけで
  **無条件に**索引更新を促していた（判定を hook で再実装すると drift する、という理由で判定を持たなかった）。
  判定を subprocess へ置いたことでその反論は解け、Write に絞る理由も同時に消えた。
- **帰属の判定は文字列結合であり、沈黙側で壊れる。** `finding` は `{file, line, message}` で `file` は
  **文書側**（`<crate>/CLAUDE.md`）を指すため、編集ファイルとの結合は message にしか無い。
  `G-module-index` のメッセージ書式が変われば静かに 0 件へ倒れる。**検知器は
  `edit-findings.test.mjs` が実物の `checkModuleIndex` を呼んでいることそのものである**——
  モックへ替えるとこの結合は誰にも見られなくなる。
- **配線はプロセス級の統合テストでしか守れない**（#1140 の実測が示した型）。ユニットテストは
  `editFindingsReminder` を spy で試験できるが、その戻り値を `warnings` と `sections` へ積む行は
  見ていない。一時 git リポジトリへ最小の木を作って hook をプロセス起動する形で固定した。
  **`.md` では 2 つの reminder が鳴るため、assert は reminder ごとに 1 本ずつ置く**——束ねると
  片方の配線が消えてももう片方が埋めて沈黙する。
- **受容する残余**: 削除（`rm` は `Edit|Write` matcher に届かない）・`governanceDocs` の外の `.md`・
  他文書からその文書を指す壊れた参照・`mod` 宣言・`.rs` の編集時の順方向。**いずれも CI が引き取る。**
  編集粒度の発火率は**原理的に測れない**（git はコミットしか記録しない）。
- **旧 WARN が唯一の合図だった経路が 1 つ消えた**（委譲レビューの逆向きの監査が拾った）。旧
  `isSourceFileWrite` は `MODULE_INDEX_CRATES` の外の `.rs` でも盲目に鳴っていたので、**まだ
  `CLAUDE.md` を持たない新設 crate の最初の `.rs`** にも WARN が出ていた。その形は
  `G-module-index`（`MODULE_INDEX_CRATES` に載る crate だけを見る）にも `npm test` の母集団カナリア
  （`CLAUDE.md` を持つ member だけを縛る）にも掛からない。**索引についてはどの層も見ていない**——
  ただし**その crate が完全に無防備になるわけではない**: `mod` 到達性は `G-module-linkage` が見ており、
  あちらの母集団は `workspaceMembers`（ルート `Cargo.toml` の `[workspace] members`）なので
  `CLAUDE.md` の有無に依らない。

---

status: Accepted
関連: `docs/adr/ADR-dependents-reminder-at-edit-time.md` ・`docs/adr/ADR-hook-fires-table-check.md` ・`docs/hooks.md`
