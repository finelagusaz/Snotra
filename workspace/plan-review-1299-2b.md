# plan-review 2b（独立再導出）— issue #1299

## 対象

- issue #1299「PR 前ゲートで『最後に委譲検証した sha』と HEAD のコード差分を照合する」
- ブランチ `fix/pr-gate-verified-sha`、main = `19443bb7`（#1298 の verify-tree 導入直後）
- 要件（ユーザー裁定）: (1) 照合するのは `npm run verify:tree` の要約が名乗る sha だけ (2) 「小さい手直し」例外なし、`workspace/` 外の差分があれば止める (3) 要約が無ければ `gh pr create` を止め、復帰手順は道具を回すこと
- plan.md / research.md / adversarial-1299.txt は読んでいない

## 前提として測った事実

- 要約の既定置き場は `<root>/.claude/worktrees/agent-verify-<branch を [^A-Za-z0-9._-]→- で置換>.summary.json`（`scripts/verify-tree.mjs:98`・`:206`）。`.claude/worktrees/` は gitignore（`.gitignore:22`、`git check-ignore -v` で確認）
- 要約の `sha` は `rev-parse --verify <sha>^{commit}` の 40 桁（`verify-tree.mjs:125`・`:223`）。`run` は「本体の作業ツリー（linked worktree でない）」「`git status --porcelain` が空（未追跡も含む）」「sha == HEAD」を前提に要求する（`:92-94`・`:186-189`）
- **`remove` が要約を消す**（`verify-tree.mjs:289`。`verify-tree.test.mjs:365` が固定）。`/implement` Step 4 の最初の操作がこの `remove`（`.claude/skills/implement/SKILL.md:140`）
- `--out` で要約を既定外へ出せる（`verify-tree.mjs:7`・`:206`）
- `decide` の判定順: shape → `gh pr create` 検出 → cwd 変更 → plan.md → `hasSafeChain` で ALLOW の早期 return → `readGitState`（`pre-bash.mjs:308-346`）
- **rename の実測**（使い捨て repo・git 2.56）: `src/a.rs` を `workspace/a.rs` へ `git mv` したとき、`git diff --name-only A HEAD` は `workspace/a.rs` **だけ**を出す（コード側の削除が消える）。`--no-renames` なら `src/a.rs` と `workspace/a.rs` の両方。`diff.renames` は未設定（既定で rename 検出が効く）
- **3 点形の実測**: 検証 sha S の後に `reset --hard HEAD~1`（HEAD が S の祖先）すると、`git diff --name-only S...HEAD` は**空**、`git diff --no-renames --name-only S HEAD` は `src/b.rs` を出す
- **rebase / merge の実測**（使い捨て repo）: feature で S を検証 → main が `m.rs` を足す → `rebase main` 後の `git diff --no-renames --name-only S HEAD` は `m.rs`。S へ戻して `merge main`（main は `m2.rs` も足した）後は `m.rs m2.rs`。**2 端点の木比較は履歴を見ないので、rebase・merge で照合がすり抜ける形は無い。逆に過剰検出側へ倒れる**——main を取り込んだ直後は main 側の変更が workspace/ 外の差分として数えられ、**必ず block → `run` のやり直しが要る**。要件 (2) の正しい帰結（取り込んだ版は誰も検証していない）だが、SKILL と拒否文言に書かないと「ゲートが壊れた」と読まれる

## 導出した変更ファイルとシンボル

### A. `.claude/hooks/pre-bash.mjs`（本体）

1. **新しい reader**（例 `readVerifyState(cwd)`）— `{ ok:false, reason }` / `{ ok:true, exists:false }` / `{ ok:true, exists:true, sha, outside:[...] }` 形。`readGitState`（`:349`）・`readPlanState`（`:393`）と同じ注入形にする（`decide` の分岐表をファイルシステム無しでテストするため・`:300-301`）
   - root と branch は `git rev-parse --show-toplevel` / `git branch --show-current` で取る（`readPlanState` の `findUp(.git)` 方式は linked worktree では `.git` がファイルなので同じ root を返すが、要約の置き場は verify-tree が `--show-toplevel` 由来で決める・`verify-tree.mjs:90-98`）
   - 差分は **`git diff --no-renames --name-only -z <sha> HEAD`**（2 端点の木の比較）。`--no-renames` と 2 端点は上の実測 2 件が根拠。`-z` は非 ASCII パスの引用符化を避けるため（`verify-tree.mjs:102-103` と同じ理由）
   - 除外は `path.startsWith("workspace/")` の完全一致接頭辞（`workspace` 単独や `workspaces/`・大文字違いを除外しない）
   - `spawnSync` に `GIT_TIMEOUT_MS`（`:42`）を付ける。sha が解決できない（gc 済み・別リポジトリの要約）・JSON が壊れている・`sha` が無い → `ok:false` → block
2. **`decide` へ照合を挿入する位置**: plan.md 判定（`:329-336`）の**後**、`hasSafeChain` の早期 ALLOW（`:338`）の**前**。後ろに置くと `git push … && gh pr create`（日常の形・`CLAUDE.md:23`）が照合を素通りする。拒否文言は「`npm run verify:tree -- run --sha HEAD --categories <3b の該当判定>` を回してから」を持つ（`resolveSha` は ref を受ける・`verify-tree.mjs:122-129`）。**裸の `run --sha HEAD` を書かない**——`--categories` 無しでも E/F だけ回って `status: green` の要約ができ（`verify-tree.mjs:163`）、sha だけを照合するゲートは通る。差分ありの文言は workspace/ 外のパスを数件示し、main の取り込み（rebase/merge）直後もこれに当たると書く
3. **`decide` の引数**: `platform` が第 4 位置（`:308`）。5 本目の扱いは「判断が分かれる点」1
4. **`main()` の配線**（`:424`）に新 reader を渡す
5. **冒頭コメント**（`:1-4` の (1)(2)(3) の列挙、`:7-10`「それが走る瞬間にコミットが remote に存在するかを判定する。真になる経路は 2 つ」）、`decide` の JSDoc（`:300-307`）を新判定に合わせる

### B. `.claude/hooks/pre-bash.test.mjs`

- import に新 reader を足す（`:9-27`）。fixture `VERIFIED` / `NO_SUMMARY` / `STALE(paths)` / `VERIFY_UNKNOWN` を `CLEAN` 等の並び（`:33-49`）に足す
- **母集団**: `grep -n "decide(" pre-bash.test.mjs` の全 29 件を分類した（切り詰めなし）。`gh pr create` を含むのは `:229 :233 :237 :241 :247 :253 :257 :261 :262 :266 :289 :294 :297 :300 :308` の 15 件。うち**新判定（plan.md の後に置く場合）に到達するのは 10 件**: `:229 :233 :237 :241 :247 :253 :257 :266 :294 :297`。残り 5 件は手前で決まる（`:261 :262` = cwd 変更、`:289 :308` = 未チェック、`:300` = plan.md 読めず）。残り 14 件は `gh pr create` を含まない（管轄外・shape 判定）。**10 件はどれも platform を渡していない**（3 引数形）。platform を渡す 7 件（`:792-831`）はどれも `gh pr create` を含まない
- 到達する 10 件すべてに `VERIFIED` を渡す。**block を期待する行（`:229 :233 :241 :253`）にも渡す**——渡さずに新判定を前に置くと、reader 未注入の TypeError か新判定の block で「upstream 未設定なら block」が**別の理由で緑**になる（主張の空洞化）。`reason` の文言まで assert すると堅い。**新しい引数に既定値（no-op reader）を置かない**——置くと移行し忘れた呼び出し点が沈黙で緑になり、「reader 未注入は TypeError で赤」という移行漏れの検知器が消える
- 新しい分岐表テスト: 要約なし → block（安全な鎖でも）/ 要約 sha == HEAD → allow / workspace/ 外の差分 → block / workspace/ だけ → allow / reader `ok:false` → block / `gh pr create` 以外では reader を読まない（`:222`・`:302` の spy と同型）/ 形の判定と plan.md が先に決まる（`:823` と同型）
- 実 git を使う reader テスト（`describe("readGitState"` の `:344` 節と同型・temp repo）: rename into workspace/ が block、HEAD が sha の祖先（reset）が block、`workspace-x/` や `workspaces/` が除外されない、ブランチ名に `/` を含む（`fix/x` → `agent-verify-fix-x`）
- **プロセス級 e2e の 2 本が反転する**: 「安全な鎖は git を読まずに exit 0」（`:396`）と「全項目チェック済みの plan.md なら安全な鎖は exit 0」（`:421`）は非 git の temp dir で exit 0 を期待している。新判定の下では要約が読めず block になる。temp repo + 要約 fixture を作って exit 0 を見る形へ書き換え、併せて「要約なしで exit 2・stderr に `verify:tree`」の e2e を 1 本足す（`main()` の配線を実行で固定する——`:435` の platform e2e と同じ役割）
- ソースカナリア「main() は decide へ process.platform を渡す」（`:496`、正規表現 `decide\(\s*payload,[\s\S]*?process\.platform\s*\)`）は、**platform の後ろへ 5 本目を足すと赤になる**。引数の形次第でこの正規表現も変わる

### C. `scripts/verify-tree.mjs` と `scripts/verify-tree.test.mjs`（要約のライフサイクル）

- **`remove` が要約を消す（`:289`）と、`/implement` Step 4 の順序では PR 作成時に要約が無い**——要件 (3) でゲートが必ず止まる。直し方は「判断が分かれる点」2。どちらでも `verify-tree.test.mjs:365`（remove 後に summary が無いことを assert）か SKILL の順序が変わる
- 要約の置き場の導出を hook と共有するなら export する関数が要る（現状 `mainContext` は非 export・`:90-99`）。「判断が分かれる点」3
- `--out`（`:7`・`:206`）: ゲートは既定の置き場しか読まないので、`--out` で出した要約はゲートに効かない。残すなら usage 行・ヘッダコメントにその旨を書くか、ゲートが読むのは既定の置き場だけだと hook 側に書く

### D. `.claude/skills/implement/SKILL.md`

- `:140` Step 4「木を片づける」: 要約を消す `remove` が PR 作成より前にある（上の C）
- `:142` Step 4 の ADR 回収: **`docs/adr/ADR-*.md` を起こすコミットは workspace/ の外の差分**であり、最後の `run` の後に積むと要件 (2) でゲートが止まる。ADR を最後の検証より前に起こすか、ADR コミットの後に `run` を回し直す（F の `governance:check` も ADR を見る必要がある）と書く必要がある。**`git rm -r workspace/` のコミット（`:143-144`）は workspace/ だけなので通る**
- `:125`「報告を記録する: workspace/ へ写し次のコミットに含める」は workspace/ だけ → 通る（変更不要の根拠）
- `:127` 3c「この Step を出るのは、いま HEAD にある差分が…すべて通ったとき」: ゲートがこの条件の「道具」の半分を機械化したこと、委譲（レビュー・変異注入）の sha は照合しないこと（要件 1）を書くかの判断。少なくとも「再検証しない手直し」の余地が無いことは書く
- `:84`「…sha を機械可読で名乗るからである」の段落は偽にならない
- `:159` 出力表「要約が名乗る sha」は偽にならない

### E. `.claude/skills/deps-update/SKILL.md`

- Step 3（`:53-60`）はローカルでカテゴリを手で回し、Step 4（`:82`）で `gh pr create` を打つ。**道具を回す手順が無いので要件 (3) で必ず止まる**。Step 3 か Step 4 に `npm run verify:tree -- run --sha HEAD --categories A[,C]` を足す。ついでに `:58`「カテゴリ B（TypeScript）: 必須」は道具が B を拒む（`verify-tree.mjs:160`）ので `--categories` に B を入れられない——書き方に注意（B 自体の記述の正誤は本 issue の範囲外）

### F. ルート `CLAUDE.md`

- `:23`「`gh pr create` は未 push と `workspace/plan.md` の未チェック `- [ ]` で拒まれる」は**偽になる**（3 つ目の理由が増える）。常時ロード面なので、復帰手順（`npm run verify:tree -- run --sha HEAD`）は拒否文言側に持たせ、ここは理由の追記だけにするのが軽い
- `:65` code-reviewer の行は偽にならない

### G. `docs/hooks.md`

- `:15` の plan.md ゲート項の隣に「検証 sha ゲート」の項を足す（発火点・fail-closed の倒し方・除外の述語・`--no-renames` と 2 端点の理由・`--out` は見ない・要約が無いときは管轄外ではなく block）
- `:15` 末尾の「`decide(payload, readGitState, readPlanState)` の注入で…」と `:30`「platform は第 4 位置引数で値として注入する」は引数の形次第で偽になる
- `:31`「この 5 判定は全コマンドで走る（`gh pr create` 系は検出後のみ）」は偽にならない
- `:11` fail-closed の列挙「判定不能はすべて block へ倒す — payload 破損・command が非文字列・git 状態が読めない・鎖の途中で cd」は数え上げであり、新判定で**偽になる**（AGENTS.md「数え上げは偽になる時点が確定している」）。「要約が読めない・sha が解決できない」を足すか、列挙をやめて `decide` を指す形へ変えるかの二択（任意ではない）

## 触らない・変更不要と判断したもの

- `docs/adr/*`（凍結）: `ADR-implement-verification-tool-tree.md`「受容した残余」の「報告が sha を名乗ることで、事後に食い違いを見つける経路は残る」は、事後経路が機構化されても偽にならない（「残る」は下限の主張）。`ADR-command-shape-norms-in-hook.md:21` の「5 本目を足すときに再検討する」は**この issue が再検討の時点**なので、判断を新 ADR に残すかは「判断が分かれる点」1 次第（既存 ADR は編集しない）
- `AGENTS.md`: `gh pr create`・PR 前ゲート・verify:tree への言及なし（grep 0 件）
- `.claude/settings.json`: matcher・timeout(30s) は変えない。照合は git 2〜3 回で `GIT_TIMEOUT_MS` 10s × 回数が 30s に近づく点だけ注意（⚠️ 4）
- `.github/workflows/ci.yml:159`: hook テストを ubuntu/windows で回す既存配線で足りる。新しい e2e は両 OS で通るパス組み立て（`path.join` / `tmpdir`）にする（`pre-bash.test.mjs:29-30`）
- `scripts/clean-worktrees.mjs`: `agent-*` の**木**とブランチだけを見る（`:35`・`:82`）ので要約ファイルを消さない——要約を残す案でも干渉しない
- `.claude/skills/start-issue/SKILL.md:100`・`:102`: 「未チェックガード（#749）」は plan.md ゲートの名指しで、偽にならない
- `docs/build-commands.md:11`: 偽にならない
- `docs/development-principles.md`「検証の層と、層と層の隙間」（`:181`）: 節を開いて確認。冒頭が「hook のような『走らせるまでもなく効くもの』は階梯が扱う」と射程から外しており、hook の行を持たない（`:189` の vitest 行は「hook の判定ロジック」を対象に挙げるだけ）。偽にならない。`:153`・`:165` も一般則で偽にならない（`:165` は「判断が分かれる点」5 の根拠として引くだけ）

## 判断が分かれる点と推奨

1. **`decide` の 5 本目の引数** — (a) platform の後ろへ足す（既存テストの platform 位置は不変、カナリアの正規表現だけ変える）/ (b) platform の前へ挿入（`decide(…, "win32")` の呼び出しすべてがずれる）/ (c) `ADR-command-shape-norms-in-hook.md:21` が予告した options オブジェクト化。**推奨は (c)**。根拠: 新判定に到達する既存 10 件は**どれも platform を渡していない**（B の分類）ので、(a) では 10 件すべてが `decide(x, git, plan, undefined, VERIFIED)` になり、意味の無い `undefined` の穴を 10 個作る——ADR が「5 本目」を再検討の条件に挙げた劣化そのもの。ADR が (c) を却下した理由「既存テストを 1 行も触らずに済む」は、10 件を必ず書き換えるこの issue では成り立たない。(c) を採らないなら、ADR の予告への応答として理由を新 ADR に残す
2. **要約のライフサイクル（Step 4 の `remove` との衝突）** — (a) `remove` が要約を消さない（`verify-tree.mjs:289` を外す。古い要約が残っても sha 照合で無害——別ブランチの要約は置き場が違い、同ブランチの古い要約は差分で止まる）/ (b) SKILL の順序を「PR 作成 → remove」にする / (c) 要約を workspace/ へ写したものをゲートが読む（Step 4 で `git rm -r workspace/` が消すので不可）。**推奨は (a)**——(b) は PR 作成が SKILL の外（ユーザー指示・`/deps-update`）にあり順序を強制できない。(a) では掃除の責任が残るが、gitignore 済み・1 ブランチ 1 ファイルで上書きされる
3. **置き場の導出の共有** — hook が `scripts/verify-tree.mjs` を import する（DRY）か、hook 内で同じ式を書きテストで一致を固定するか。**推奨は後者＋一致テスト**。理由: hook は**全コマンド**で走り、静的 import の失敗（その checkout に `scripts/verify-tree.mjs` が無い古いコミット・`./governance/lib.mjs` の import 失敗）は `main()` 到達前の throw になる——ESM の静的 import 失敗は try/catch で捕まらず node が exit 1（使い捨てスクリプトで実測: `import "./missing.mjs"` の後に `process.exitCode = 2` を置いても exit=1）→ **fail-open（`pre-bash.mjs:12-15`）**。どうしても共有するなら `gh pr create` 検出後の動的 import に限る
4. **要約の `status` / `branch` を見るか** — 要件 (1) は sha だけを言う。だが `status: "red"` の要約で sha == HEAD なら通ってしまう。**推奨: `status === "green"` も要求する**（「赤のまま PR」は #1299 の動機と同型）。`branch` は置き場が branch 由来なので冗長だが、sanitize の衝突（`fix/a-b` と `fix-a-b` が同じ置き場）を考えると一致を見るのが安い。どちらも**ユーザー裁定の範囲を越える**ので、主から確認を取るべき
5. **非 git / 他リポジトリ / linked worktree での `gh pr create`** — 要件 (3) を字義どおりに取ると「要約が無い = block」。だが `run` は linked worktree からは打てない（`verify-tree.mjs:92-94`）ので、**linked worktree のセッションでは復帰手順が適用できない拒否**になる（`docs/development-principles.md:165`「復帰手順が適用できない誤爆」）。同様に `run` は未追跡ファイルがあると exit 2（`:186-187`）なので、`workspace/` を未コミットで持つ状態でも復帰手順は「先にコミットせよ」を含む必要がある。**推奨: 拒否文言にこの 2 つの前提を書く**。worktree セッションからの PR を正式経路として残すかは裁定が要る
6. **`--out`** — 残すならゲートに効かないことを usage に書く。消すのも可（`/implement` は使っていない・SKILL に `--out` の出現なし）

## 検知器が効くことを確かめる変異（実装後に当てる）

1. `decide` から照合を外す → 「要約なし → block」と e2e「要約なしで exit 2」が赤
2. 照合を `hasSafeChain` の早期 ALLOW の後ろへ動かす → 「安全な鎖でも要約なしは block」が赤
3. `--no-renames` を外す → 「rename into workspace/ は block」の実 git テストが赤
4. 2 端点を 3 点形 `sha...HEAD` にする → 「HEAD が sha の祖先」の実 git テストが赤
5. 接頭辞を `"workspace"`（スラッシュ無し）にする → `workspaces/x` か `workspace-x/y` のテストが赤
6. reader の `ok:false` を ALLOW に倒す → 「要約が壊れている / sha が解決できない → block」が赤
7. `main()` で新 reader を渡さない → e2e が赤（TypeError → catch → exit 2 なので**要約ありで exit 0 を期待する e2e**でないと捕まらない——要約なしの e2e は配線が抜けても exit 2 で緑のまま。**この非対称は重要**）
8. 置き場の導出から sanitize を外す → ブランチ名 `fix/x` のテストが赤
9. `remove` に要約削除を戻す（案 2a を採った場合）→ lifecycle テストが赤
10. 既存 block テストから `VERIFIED` を外す → 赤にならないなら空洞化（B の注意）。reason の assert で縛る

## ⚠️（確信の持てない所見）

1. **`gh pr create --head <別ブランチ>`** は HEAD 以外のブランチから PR を作る。ゲートは HEAD と HEAD のブランチの要約しか見ない。既存の push 判定も同じ前提なので受容範囲かもしれないが、docs/hooks.md の「見ない形」に足すべきか
2. **要約を手で書けば通る**。`--no-verify` と同格の意図的迂回として扱えるか（hook は fail-closed の事故防止であって改竄防止ではない）——文書に明記するか
3. `/implement` 以外で PR を作る経路は、grep（`gh pr create`・「PR を作」・「PR 作成」）で `/deps-update` しか見つからなかった。**ユーザーの直接指示で主が PR を作る経路**（docs だけの PR 等）は SKILL に現れないので、そこへ道具を回す手順はルート `CLAUDE.md:23` の 1 行と拒否文言だけが運ぶ
4. 照合の git 呼び出しが 3 本（toplevel・branch・rev-parse sha・diff）になると、`GIT_TIMEOUT_MS` 10s × 4 が hook の timeout 30s（`.claude/settings.json:10`）を越えうる。hook の timeout 到達は exit≠2 = **fail-open**。呼び出しを減らす（`rev-parse --show-toplevel --abbrev-ref HEAD` を 1 本にまとめる等）か、合計の予算で打ち切る
5. `start-issue/SKILL.md:102` の「`/implement` Step 5 以降」は、現行の `/implement` に Step 5 が無い（Step 4 まで）。本 issue と無関係の既存の食い違いだが、Step 4 を触るついでに見つかったので記録する
6. **カテゴリの妥当性はゲートの射程外**。要件 (1) の下でゲートが保証するのは「HEAD で道具が何かを走らせた」までで、`categories_run` と変更の突き合わせ（`implement/SKILL.md:123`）は散文のまま残る。E/F だけの要約でも通る。判断 4 の `status` と同じ列の論点で、ゲートに `categories_requested` を見させるかは裁定が要る（見させても「何を要求すべきか」は意味判断なので機械化できない）
7. 要件 (2) の「workspace/ の外」について、`.delegate/` は追跡されないので差分に現れない（`verify-tree.mjs:20-21`）。`.claude/worktrees/` も gitignore で現れない。ほかに「差分に出るが検証不要」なパスがあるかは見ていない（要件が例外を認めないので、見つけても除外しない）
