# 調査 — issue #1551

## issue の要約

`.claude/hooks/pre-bash.mjs` に、次の 3 つの判定を足す。

1. `git rm` が `workspace/` を消すとき、`workspace/plan.md` に未チェック項目が残っていれば止める
2. 未コミットの変更を捨てる形を止める: `git rm -f`／`--force`、`git checkout -- <path>`、`git checkout .`、`git checkout-index`、`--staged` だけでない `git restore`
3. コマンド位置の `gh pr merge` を止め、`/merge-pr` へ案内する

範囲外（ユーザーの裁定・2026-11-28 の会話）: `git reset --hard`・`git clean -f` は、足りないと分かったときに足す。`git rm` の判定が拾わない消し方（`Remove-Item` で消してから `git add -A` など）は、残余として受け入れる。

## 関連するファイル・シンボル

- `.claude/hooks/pre-bash.mjs`
  - `decide`: 判定の SSOT。`judgeCommandShape`（I/O なし）→ `ghPrCreateSites` で `gh pr create` を検出 → reader の必須確認 → `readPlanState` → `readVerifyState` → `hasSafeChain` → `readGitState` の順
  - `judgeCommandShape`: コマンドの形だけの判定。`usesNoVerify`・`pullWithoutFfOnly` は全環境、`usesHeredoc`・`usesBackslashPath`・`needsPyEncoding` は `platform === "win32"` のときだけ
  - `gitSegments` / `gitSegmentsInBothViews`: コマンド位置の `git` から次の区切りまでを、元の文字列と `unquotedView` の両方で切り出す
  - `GIT_COMMIT_FORMS` / `GIT_PULL_FORMS`: `^git\s+${FLAG}<sub>\b` を `FLAG` と `FLAG_QUOTED` の 2 形で持つ
  - `GH_PR_CREATE` / `GH_PR_CREATE_QUOTED` / `ghPrCreateSites`: `AT_CMD_POS` + `ENV_PREFIX`(`_QUOTED`) + `gh\s+FLAG pr\s+FLAG create\b`
  - `SHAPE_REMEDY`: 拒否文言の表（「なぜ止めたか」と「代わりに何をするか」を必ず持つ）
  - `readPlanState(cwd)`: リポジトリルートの `workspace/plan.md` を読み、`{ ok, exists, unchecked }` を返す。無ければ `exists: false`
  - `countUnchecked`
  - `main`: `decide` に reader 3 つと `platform` を渡す。`verify-tree.mjs` の動的 import は `gh pr create` を検出したときだけ
- `.claude/hooks/pre-bash.test.mjs`: describe ごとの構成。`usesNoVerify`・`pullWithoutFfOnly`・`judgeCommandShape — platform ゲート`・「5 述語は全域関数である（I2・no-throw）」・「止めた理由まで確かめる（#1523）」・「引用符で囲んだ値の病的入力でも時間内に終わる（#1523）」・「プロセス起動 — exit code の契約」・「settings.json ドリフト検出カナリア」
- `.claude/skills/implement/SKILL.md`: 「守ること」の表（マージしない・未確定ゼロ・`git rm` の行）、Step 4 の `workspace/` 削除の項
- `.claude/skills/merge-pr/SKILL.md`「ツールが守らないもの」:「hook は `gh pr merge` を見ない」
- `scripts/merge-pr.mjs`: `gh pr merge` を `spawnSync`（`shell: false`）で呼ぶ。Bash/PowerShell ツールを経ないので、hook の対象にならない

## 再利用できる既存のパターン

- git のサブコマンドの判定: `gitSegmentsInBothViews(command).some((s) => FORMS.some((re) => re.test(s)) && 条件)`（`usesNoVerify`・`pullWithoutFfOnly`）
- gh のサブコマンドの検出: `GH_PR_CREATE` の 2 形と、元の文字列・`unquotedView` の両方で探す `ghPrCreateSites`
- I/O を要る判定: `decide` に reader を注入し、テストで差し替える（`readPlanState` は既にある）
- 拒否文言: `SHAPE_REMEDY` に足す
- テスト: 理由ごとに入力を置き、拒否の理由（文言）まで assert する（「止めた理由まで確かめる」）。全域性（no-throw）と病的入力の時間の describe に、新しい述語を足す

## 技術的な制約

- `judgeCommandShape` は全コマンドで走るので、全域関数で、入力長に対して線形でなければならない（throw は exit 2 で全コマンドを止め、hang は timeout で fail-open になる）
- `judgeCommandShape` は I/O を持たない。`workspace/` 削除の判定は `plan.md` を読むので、`decide` の中で reader を使う形になる
- 現状、reader の必須確認は `gh pr create` を検出した後にしかない。`git rm workspace` の経路で `readPlanState` を使うなら、その経路でも配線を確かめる
- hook は本体の作業ツリーの版で走る（`pre-bash.mjs` 冒頭）。この worktree の変更は、マージまでこのセッションのゲートに効かない。検証はユニットテストと process 級の e2e で行う
- `.claude/rules/governance-docs.md`: 述語の種類・件数を散文へ写さない。`pre-bash.mjs` 冒頭の「規範 5 件」と、テストの describe 名「5 述語」は、件数を写している
- PR 前ゲートの (2)（`plan.md` の未チェック）は残す。`workspace/` を消し忘れたときに効く

## 敵対的調査（3b）の採否

出典: `workspace/adversarial-1551.txt`

- B1 採る — `scripts/merge-pr.mjs:33` が `pre-bash.mjs` を静的 import する（`verifiedSummaryState`）。`gh pr merge` の判定は hook を通らないので `merge:pr` を止めないが、`pre-bash.mjs` のロード時の失敗は `merge:pr` も落とす。変更は関数の追加に留め、モジュールの先頭で throw しうる処理を置かない。`npm test` は `merge-pr.test.mjs` も回すので、ロードの破綻はそこで赤になる
- B2 採る — 偽になる散文の列挙を足す: `implement/SKILL.md` の「守ること」の「マージしない」行と「散文のみ」の `git rm` の行、Step 4 の削除の項の末尾「PR 前ゲートが計画を見られなくなる」と `git rm` が止まったときの箇条、`pre-bash.mjs` 冒頭の「(1)(2)(4) は `gh pr create` 検出後のみ」、`judgeCommandShape` と `decide` の doc（「管轄外」の説明）。ルートの `CLAUDE.md`「`gh pr merge` を手で打たない」は、判定が入っても真のままなので直さない
- B3 採る（裁定は自分で行う） — `gh pr merge` の判定は、`gh pr create` と同じく `unquotedView` の見え方でも探すので、引用の内側の言及（`git commit -m "x; gh pr merge"`）で止まる。`#1523` の「見え方で判定する過剰検出は、意図として固定する」と同じ扱いにし、テストで意図として固定する
- U1 採る — `git rm workspace` の経路で reader が渡されなければ、TypeError ではなく理由つきで block する（既存の reader 確認と同じ形）
- U2 採らない — 本体の版で走ることは、`.claude/settings.json` の command と `pre-bash.mjs` 冒頭の記述（#1436）で足りる。検証はテストで行うので、この計画の結論に影響しない
- U3 採る — 判定 A の e2e は、`payload.cwd` に fixture のリポジトリを渡す（渡さないと本体の `workspace/plan.md` を読む）。platform に依らない判定なので、ubuntu と windows で同じ結果になることを前提にする
- U4 一部採る — 拾う形に `git rm -r -- workspace` を足す（`--` の後のパス語も見る）。`git rm -r .`・`*`（`workspace` を含みうる）と、`git -C <dir> rm workspace`（`readPlanState` が読むリポジトリと違いうる）は、拾わない残余として受け入れる（ユーザーの裁定「git rm の判定の変更も現状維持」）
- U5 採る — 判定 B・C は I/O なしなので `judgeCommandShape` に、判定 A は `decide` に置く。件数を写した記述（「規範 5 件」・describe 名「5 述語」）は、件数を書かない形へ直す

壊せなかった項目（`judgeCommandShape` が全コマンドで先に走り I/O を持たないこと、`merge-pr.mjs` が `shell: false` で gh を呼ぶこと、Step 4 → Step 5 の順でゲート (2) が `plan.md` を見ないこと、`settings.json` の配線）は、本文の記述のまま。

## 未解決の疑問

いずれも計画（`workspace/plan.md`「設計」「残余」）で決めた。

- `git rm --cached -f` — 止める（ステージした内容を失いうるので、fail-closed の向きに倒す）
- `git checkout <path>`（`--` も `.` も無い形） — ブランチ名と区別できないので、拾わない残余として受け入れる
- `git checkout -f`・`git switch --discard-changes` — ユーザーの裁定により範囲外
- `git rm -rf workspace/` — B の文言で止める（B を先に判定する）
- `gh pr merge` への言及 — `grep "gh pr merge"` は `gh` がコマンド位置に無いので止まらない。区切りを含む引用の内側の言及は止まる（B3）
