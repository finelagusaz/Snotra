# 計画: #1299 PR 前ゲートで「最後に検証した sha」と HEAD のコード差分を照合する

根拠は `workspace/research.md`（ユーザーの裁定 3 点と、敵対的調査の採否）。

## 目的

`gh pr create` の時点で、道具（`npm run verify:tree`）が最後に green を返した sha から HEAD までに、`workspace/` の外の変更が無いことを pre-bash hook で確かめる。最終巡のあとに載った変更が、検証されないまま PR になる形（issue の実測 9 件と #1302）を止める。

## 受け入れ条件

1. `gh pr create` を検出したら、次のどれかに当たると block する。止めるのは plan.md ゲートの後、`hasSafeChain` の allow の前である
   - 今のブランチの要約（`<root>/.claude/worktrees/agent-verify-<slug>.summary.json`）が無い
   - 要約が読めない、または JSON として壊れている
   - 要約の `branch` が今のブランチと違う
   - 要約の `status` が `green` でない
   - `git diff --name-only --no-renames -z <要約の sha> HEAD` が失敗する（sha が解決できないなど）。`-z` は、空白や非 ASCII を含むパスが引用符で囲まれて `workspace/` の接頭辞に当たらなくなる形を防ぐ。2 端点で比べる（3 点形は、reset で HEAD が sha の祖先へ戻ると空を返す・独立導出の実測）
   - その出力に `workspace/` の外のパスが 1 つでもある（拒否文言にパスを最大 10 件まで載せる）
   - 本体が detached HEAD である
2. 拒否文言は復帰手順を持つ: 「変更をコミットしてから、本体の作業ツリーで `npm run verify:tree -- run --sha HEAD --categories <該当カテゴリ>` を回す」
3. `decide` の引数を options オブジェクトへ改める: `decide(payload, { readGitState, readPlanState, readVerifyState, platform })`。3 つの reader は必須で、関数でなければ block にする（配線忘れが allow へ倒れない）。`platform` は今と同じく省略可能（省略時は Windows 専用判定を発火させない）。`ADR-command-shape-norms-in-hook` が「5 本目を足すときに再検討する——そのときは足す判定と同じサイクルで改修する」と予告しており、位置引数のまま足すと、省略可能な `platform` の後ろに `undefined` の穴が開く
3b. 新しい reader の git 呼び出しは 2 本に抑え（ブランチ名と diff。ルートは `findUp` で導く）、それぞれの timeout を 4 秒にする。hook 全体の timeout は 30 秒（`.claude/settings.json`）で、越えると非ブロッキングのエラー＝素通りになる。既存の `readGitState`（2 本 × 10 秒）と合わせても 28 秒に収める
4. hook は、`gh pr create` を検出したあとでだけ `scripts/verify-tree.mjs` を動的 import する（構文エラーなどの爆発半径を、全コマンドから `gh pr create` だけへ閉じ込める）。import の失敗は block にする
5. 要約のパスは `summaryPathFor(root, branch)`（verify-tree から export する純関数）で導く。verify-tree の `mainContext` も同じ関数を使う
6. `verify-tree remove` は、道具の木とログを消し、**要約は残す**。要約には `removed_at` を書き足す
7. `/implement` Step 4 の順序を、ADR の回収 → （最後の要約から `workspace/` の外が動いたときだけ）道具を回し直す → 木の片づけ → `workspace/` の撤去、に改める
8. `/deps-update` Step 4 は、`gh pr create` の前に道具を回す（カテゴリは A と C）
9. 文書の索引を直す: ルート `CLAUDE.md:23`、`docs/hooks.md` の契約（新しいゲートの箇条と、「見ないコマンド形」への `gh.exe` 系・`gh api` の追記）

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/verify-tree.mjs` | `summaryPathFor(root, branch)` を export する（`mainContext` から slug の導出を移す）。`cmdRemove` は要約を残して `removed_at` を書き、ログだけ消す |
| `scripts/verify-tree.test.mjs` | `remove` の断言を「ログは消え、要約は残り `removed_at` を持つ」へ改める。`summaryPathFor` の単体テスト（`/` を含むブランチ名） |
| `.claude/hooks/pre-bash.mjs` | `readVerifyState(cwd)`（async ではなく同期。`spawnSync` で `git rev-parse --show-toplevel`・`git branch --show-current`・`git diff --name-only --no-renames`）。`decide` に第 5 引数として必須の reader を足し、plan.md ゲートの直後に照合を置く。`main()` を async にし、`gh pr create` を検出したときだけ verify-tree を動的 import して reader へ渡す（未確定欄の決定）。ファイル冒頭の「受容する未対応リスク」に `gh.exe` 系・`gh api` を足し、(1)(2) の説明に要約の照合を足す |
| `.claude/hooks/pre-bash.test.mjs` | `decide(` の呼び出し 29 か所を options オブジェクトの形へ書き換える（新しい reader は `VERIFIED` などの定数）。**書き換えの前後で、既存のテストの件数と結果が一致することを測ってから**新しいテストを足す（ADR の I7「既存呼び出しの意味を変えない」）。block を期待する既存テストにも `VERIFIED` を渡す（渡さないと、新しい照合という別の理由で block になり、元の理由を測らなくなる）。ソースのカナリア「main() は decide へ process.platform を渡す」（`:496`）を新しい形へ改め、3 つの reader の配線も同じカナリアで固定する。新しい分岐表（要約なし・壊れている・ブランチ違い・red・diff 失敗・workspace の外の差分・workspace だけの差分・rename）。reader 欠落の block。本物の reader を通す e2e（本物の git repo と要約ファイル）。既存の e2e で `gh pr create` が exit 0 になる 2 件は、要約を置く形へ改める |
| `.claude/skills/implement/SKILL.md` | Step 4 の順序（受け入れ条件 7）と、PR 前ゲートが要約を照合する旨を 1 文 |
| `.claude/skills/deps-update/SKILL.md` | Step 4 に道具を回す 1 行 |
| ルート `CLAUDE.md:23` | 拒否理由に「道具の要約の照合」を足す |
| `docs/hooks.md` | 「PreToolUse（pre-bash.mjs）の実装契約」に「要約ゲート」の箇条を足す。「見ないコマンド形」に `gh.exe pr create`・`& '<path>/gh.exe' pr create`・`gh api …/pulls` を足す。偽になる箇所を直す: fail-closed の列挙（`:11`）、`decide` の引数の記述（`:15`）、「platform は第 4 位置引数で値として注入する」（`:30`） |
| `.claude/hooks/pre-bash.mjs` の注釈 | 冒頭コメント（`:1-10`）と `decide` の JSDoc（`:300-307`）を、新しいゲートと options オブジェクトに合わせて直す（上の行の変更と同じファイル） |
| `docs/adr/ADR-pr-gate-verified-sha.md`（新規） | 却下した案と理由: 委譲のレビューの sha も照合する（裁定）、小さな手直しの例外（裁定）、要約が無いときに通す（裁定）、記録を plan.md に置く（Step 4 で消える）、`docs/adr/` を除外に足す（例外の追加になる）、検出の正規表現を広げる（射程が広い）。受容した残余: 要約は自己申告で `categories_run` の妥当さは見ない（手で書けば通る）、PR を作ったあとのコミットは射程外、`--out` で外へ出した要約は見ない、`gh pr create --head <別ブランチ>` は今のブランチの要約で照合する、要約を消す側は置かない、rebase や main の取り込みの後は必ず止まる（`run` のやり直しが要る）。却下案に「options オブジェクト化を見送る」を加える（ADR-command-shape-norms-in-hook の予告に従って改修した） |

## 実装順序

1. verify-tree: `summaryPathFor` の切り出しと `remove` の変更（テストを先に Red にする）
2. pre-bash: `readVerifyState` と `decide` の第 5 引数（分岐表のテストを先に Red にする）、`main()` の配線、e2e
3. 規範と文書: `/implement` Step 4、`/deps-update`、ルート `CLAUDE.md:23`、`docs/hooks.md`、ADR

## 不変条件と異常系

- **fail-closed**: 判定できないもの（要約が読めない・git が失敗する・reader の import が失敗する・reader が渡されない）は block。allow は「要約が green で、branch が一致し、`workspace/` の外の差分が 0 件」のときだけ
- **爆発半径**: 新しい処理は `gh pr create` を検出したあとだけで走る。`gh pr create` を含まないコマンドでは、要約もファイルも読まない（スパイで固定する）
- **rename**: `--no-renames` を必須にする（rename のテストで固定する）
- **要約は消さない**: `remove` のあとも要約は残り、PR を作る時点で読める

## テスト方針と検証コマンド

変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う。先に書く変異は次のとおり。
- V1: `--no-renames` を外す → rename のテストが赤
- V2: 照合を `hasSafeChain` の allow の後ろへ移す → 「安全な鎖でも要約が無ければ block」のテストが赤
- V3: `main()` の reader の配線を外す → 本物の reader を通す e2e が赤
- V4: `status` の照合を外す → red の要約のテストが赤
- V5: `branch` の照合を外す → ブランチ違いのテストが赤
- V6: `remove` を、要約も消す元の形に戻す → verify-tree のテストが赤
- V7: 新しい reader の必須確認を外す（渡されなければ照合を飛ばす形）→ 「reader が渡されなければ block」のテストが赤
- V8: diff の `-z` を外して改行で分ける → 空白を含むパスのテストが赤

検証（`docs/build-commands.md`）: E（`npm test`）と F（`npm run governance:check`）。A〜D は非該当（`.rs` が無く、ウィンドウ・ホットキー・表示に触れない）。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（開発プロセスの変更）
- 関連文書: 上の変更ファイル一覧のとおり

## 作業項目

### Phase 1 — verify-tree
- [ ] `summaryPathFor` と `remove` の新しい挙動のテストを書き、今の実装で Red になることを確かめる
- [ ] `summaryPathFor` を export し、`mainContext` をそれへ寄せる
- [ ] `remove` を、要約を残して `removed_at` を書き、ログだけを消す形にする

### Phase 2 — pre-bash
- [ ] 新しい分岐表のテストと e2e を書き、今の実装で Red になることを確かめる
- [ ] `readVerifyState` を実装する
- [ ] `decide` を options オブジェクトへ改め、既存のテスト 29 か所を書き換える。書き換えの前後で既存テストの件数と結果が一致することを記録する
- [ ] `decide` に要約の照合を置く
- [ ] `main()` を async にして配線し、ファイル冒頭のコメントを直す

### Phase 3 — 規範と文書
- [ ] `/implement` Step 4 の順序と、ゲートの 1 文
- [ ] `/deps-update` Step 4 の 1 行
- [ ] ルート `CLAUDE.md:23`
- [ ] `docs/hooks.md` の要約ゲートの箇条と、「見ないコマンド形」の追記
- [ ] `docs/adr/ADR-pr-gate-verified-sha.md`

計画外（PR 本文へ送る）: PR を作ったあとに足したコミットを、`/merge-pr` の側で照合するか（research Q4。#1302 の `eef7a7aa` の形）。別の issue にする。

## 未確定（実装前に潰す）

- [x] hook から verify-tree の `summaryPathFor` を使う方法 — **決定: `main()` を async にし、`gh pr create` を検出したときだけ `await import("../../scripts/verify-tree.mjs")` する。** `pre-bash.mjs` の末尾（`invokedDirectly` の節）を読んだ。骨格は「先に `process.exitCode = 2` を置く → `main()` → catch で `emitBlock`」で、`process.exit` は使わない。async にしても、既定の 2 を先に置いたまま `main().catch((e) => emitBlock(…))` で受ければ骨格は保てる。Node は待ちの残る Promise があれば終了しない。`.catch` を付けるので、未処理の rejection（exit 1 で素通り）にもならない。`decide` は同期のまま残し、import 済みの関数を reader に渡す。import が失敗したら、reader が `{ ok: false }` を返して block する。slug の導出を hook に写す案は採らない（検出器で縛る写しより、写さない方が単純）

## 人間レビュー

- [x] 承認済み — 2026-10-25 / 問い: "注釈を書き込んでいただくか、このまま承認してくださるか、お知らせくださいませ。"（あわせて「sha だけで十分とお考えでしたら、お知らせくださいませ。」）/ 回答: "OKです、その解釈で進めて"

## plan-review 結果

- リスク: 高（hook・skills・規範文書を変える）
- レビュー方式: 独立導出 1 体（Step 2b）。成果物は `workspace/plan-review-1299-2b.md`
- エージェント数: 1

### 要対処（反映済み）
- diff を `-z` で読む・2 端点で比べる — 計画の修正（受け入れ条件 1・V8）— 根拠: 引用符の形は #1298 の 2 巡目で `git status` について実測済み。2 端点の根拠はレビュアの実測
- 5 本目の引数は options オブジェクトにする — 計画の修正（受け入れ条件 3）— 根拠: `docs/adr/ADR-command-shape-norms-in-hook.md:21` の予告を読んで確かめた
- block を期待する既存テストにも `VERIFIED` を渡す・ソースのカナリア `:496` を改める — 計画の修正（変更ファイル一覧）— 根拠: `pre-bash.test.mjs:496` の正規表現 `decide\(\s*payload,[\s\S]*?process\.platform\s*\)` を読んだ。options オブジェクトの形では一致の仕方が変わる
- 偽になる散文 `docs/hooks.md:11`・`:15`・`:30` と、hook の冒頭コメント・JSDoc — 計画の修正（変更ファイル一覧）
- git 呼び出しの合計時間が hook の timeout 30 秒を越えうる — 計画の修正（受け入れ条件 3b）— 根拠: `.claude/settings.json` の `"timeout": 30` と `GIT_TIMEOUT_MS = 10_000`（`pre-bash.mjs:42`）を読んだ

### 軽微
- レビュアの「置き場の導出は hook 内に書き、一致テストで固定する」— 採らない。静的 import が失敗すると exit 1 で素通りするのは、レビュアの実測どおりである。ただ計画は動的 import を、`.catch` で受ける async の `main()` の中で行う（未確定欄の決定）。import の失敗は reader が `{ ok: false }` を返して block になるので、写しを持つ理由が無い
- 復帰手順に裸の `run --sha HEAD` を書かない — 計画の文言はもともと `--categories <該当カテゴリ>` を含む（受け入れ条件 2）
- `start-issue/SKILL.md:102` の「/implement Step 5」が存在しない — 既存の食い違いで、この issue の範囲外。PR 本文で別件として挙げる

### 判断の不一致
- **status の green と branch の一致を要求するか**: レビュアは「裁定が要る」とした。計画は両方を要求する。裁定は「道具の A〜F を検証と数える」であり、赤い実行は検証を通っていない。branch の照合は、slug が衝突したときに別のブランチの要約で通る形を防ぐ。**承認を求めるときに、この解釈を明示して確かめる**

### 未検証（受容する残余として ADR へ）
- `gh pr create --head <別ブランチ>`: 照合するのは今のブランチの要約である
- 要約を手で書けば通る: hook が防ぐのは事故であって改竄ではない
- linked worktree のセッションと、未追跡の `workspace/` があるとき: 復帰手順の `run` が拒まれる（拒否文言でコミットと本体からの実行を求める）

### 判断
- 実装着手: 可（人間の承認待ち）
- 再レビュー: しない。計画の変更は独立導出の所見を取り込んだもので、issue の要件は変わっていない

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー 1 体（Step 2b）
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 敵対的調査から 5 件（rename・reader の必須化・ADR の順序・要約の保持・status の照合）、独立導出から 5 件。すべて計画へ反映した
- 条件別チェック: `/symmetric-check` は、要約の生成（`run`）と保持（`remove` が残す）の対で見た。消す側は意図的に置かない（1 KB 前後の gitignore 済みの JSON。ADR の残余に書く）。`/state-check` は該当しない（UI のモードではない）
- 未検証: 上の「未検証」のとおり
