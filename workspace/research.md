# 調査 — #1312 `/merge-pr` の確認と事後検証を道具へ移す（#1304 を吸収）

## issue の要約

`/merge-pr` は「機構がここに無いので、下の手順が唯一の防御である」と宣言している手順スキルである。手順が防ぐ落とし穴（`--subject` で `(#N)` が消える・非 ISO 時刻の検索が沈黙で 0 件・0 件の検算・一覧の再取得判定）を、1 本の道具の構造へ移す。

ユーザー裁定（2026-10-31・`AskUserQuestion`）:

- **#1304 を吸収する** — マージ直前に、道具（`verify:tree`）の要約の sha と PR の head のあいだに `workspace/` の外の変更が無いことを照合する。判定は hook と共有し、写しを作らない
- **閉じたくない issue が一覧に在れば、道具は止めて人へ返す** — 本文の編集は道具が行わない

## 関連ファイル・シンボル（実在確認済み）

| 所在 | 内容 |
|---|---|
| `.claude/skills/merge-pr/SKILL.md` | 手順 1〜4・「誤って閉じていたら」・「`--delete-branch` を付ける前に」 |
| `.claude/hooks/pre-bash.mjs` `judgeSummary(summary, branch)` | 要約の中身（branch 一致・green・40 桁 sha）の判定。export 済み |
| `.claude/hooks/pre-bash.mjs` `readVerifyState(cwd, summaryPathFor, importError)` | 要約を読み、`git diff --name-only --no-renames -z <sha> HEAD` で `workspace/` の外の変更を見る。**比較先が `HEAD` に固定**、ブランチは `git branch --show-current` 固定 |
| `.claude/hooks/pre-bash.mjs` 末尾 `invokedDirectly` | import しただけでは `main()` が走らない（テストからの import 前提の構造）。道具から import できる |
| `scripts/verify-tree.mjs` `summaryPathFor(root, branch)` | 要約の置き場 `.claude/worktrees/agent-verify-<branch>.summary.json`（本体ルート相対）。export 済み |
| `docs/adr/ADR-squash-merge-issue-autoclose.md` | 却下案「hook で merge / close を守る」の 3 理由。(1) は「誤りは人の意図にあり機構が SSOT を問えない」 |
| `docs/adr/ADR-pr-gate-verified-sha.md` | 受容した残余「PR を作ったあとに足したコミットは射程外」（#1304 の起点） |
| ルート `CLAUDE.md`「Git/GitHub 運用」「フック」「利用できるスキル」 | `/merge-pr` への言及 3 か所。「フックが守る外部 API の不可逆呼び出しは `gh pr create` だけ」 |
| `vitest.config.ts` | `scripts/**/*.test.mjs` が `npm test` の母集団に入る |
| `package.json` | `verify:tree` / `prose:sweep` / `race:boundaries` と同じく `node scripts/<x>.mjs` で登録する形 |

リポジトリ設定（`gh api repos/finelagusaz/Snotra` 実測 2026-10-31）: `allow_squash_merge: true` のみ、`squash_merge_commit_title: PR_TITLE`、`squash_merge_commit_message: PR_BODY`、**`delete_branch_on_merge: true`**。

直近 200 件のマージ済み PR に Dependabot 作成のものは 0 件（要約の無いブランチをこの道具でマージする経路は、現状の運用には無い）。

## 再利用できる既存パターン

- `scripts/prose-sweep.mjs` / `race-boundaries.mjs` / `verify-tree.mjs` の骨組み: 依存ゼロ・shebang なし（CRLF で vitest が落ちる・PR #592）・**exit 2 = 走らなかった／使用法の誤り、exit 1 = 赤**・純関数を export して `*.test.mjs` がフィクスチャで検証
- `readVerifyState` の 2 端点 diff（3 点形は祖先へ戻ると空を返す）・`--no-renames`・`-z`
- 動的 import の失敗を `{ ok: false }` で受ける配線（`ADR-pr-gate-verified-sha` の却下案 6 番目）

## 技術的制約

- 道具の外部呼び出しは `gh` と `git` だけ。テストで差し替えられるよう、実行器を注入する
- `gh pr view --json mergedAt` は `--jq .mergedAt` で生文字列を取る（PowerShell の `ConvertFrom-Json` 経由で DateTime に崩れる・2026-08-10 実測）。道具は Node なのでこの崩れ自体は起きないが、**検索に渡す前に ISO 8601 の形を検める**
- `--subject` を渡すと件名が逐語で使われ `(#N)` が付かない。道具は件名を渡す口を持たない
- 要約の判定は `judgeSummary` を共有する。`workspace/` の外の差分を見る部分は `readVerifyState` に埋まっており、比較先が `HEAD`・ブランチが現在のブランチに固定されているので、そのままでは PR の head と headRefName に当てられない
- 要約は本体の作業ツリーの `.claude/worktrees/` にある（gitignore 済み）。別の機体ではマージ時に要約が無い
- PR の head の commit が手元に無いことがある（他の機体から push された場合）

## 未解決の疑問

1. 照合の差分ロジックを hook と共有する形 — `readVerifyState` に比較先とブランチの引数を足すか、差分部分を別関数へ切り出して両方から呼ぶか
2. PR の head が手元に無いとき — `git fetch origin <headRefName>` してから比べるか、止めるか
3. 要約が無いとき（別の機体・消した） — fail-closed で止め、迂回口は作らないか
4. 事後検証 (2)「残すと決めた issue が今も OPEN か」— 道具は「残す」を知らない。閉じる予定の集合を人が宣言し（例 `--closes 1312,1304` / `--closes none`）、(3) の「マージ以降に閉じた issue」に宣言外が現れたら報告する形で (2) を (3) へ吸収できるか。並行セッションが同時刻に issue を閉じると誤報になる
5. 0 件の検算 — 現行手順は「既知の閉じた issue を含む境界で打ち直す」。マージした PR 自身を `is:pr merged:>=<mergedAt>` で検索し、それが現れることで時刻の形が効いていることを示せるか（`closed:` と `merged:` が同じ時刻構文を受けるかは要確認）
6. `delete_branch_on_merge: true` の下で、そのブランチを base にする open PR があるときの扱い（#830 の CLOSED 化は `--delete-branch` で実測。自動削除で GitHub が retarget するかは未測）— 道具はマージ前に検出して止めれば、どちらでも安全側
7. 道具を経ない `gh pr merge`（Web UI・手打ち）は残余のまま。ルート `CLAUDE.md`「フック」の記述はそのままで真か

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1312.txt`。

**壊せなかった項目**:

- 命題 1: import しても副作用が無い（stdin 読みと `main` は `invokedDirectly` の内側にある）
- 命題 2: `readVerifyState` の比較先は `HEAD`、ブランチは現在のブランチに固定されている。ただし `judgeSummary` は既に引数化されており、共有に要るのは差分部分の切り出しだけである
- 命題 3: 要約は `.gitignore:22` で ignore され、`remove` 後も残る
- 命題 4: `closed:` と `merged:` はどちらも ISO 8601 と日付のみを受ける。境界は秒単位で inclusive。`MM/DD/YYYY` と `garbage` はエラーなしで 0 件になる
- 命題 6 の結論
- Q7

| 所見 | 採否 | 理由・反映 |
|---|---|---|
| hook の `findUp(cwd, ".git")` と verify-tree の `mainContext` では、linked worktree での root の導き方が食い違う | **採る** | 道具は `mainContext` と同じ導き方を共有する（git-dir と common-dir が一致しなければ exit 2）。`findUp` が `.git` の存在だけを見ることは手で確認した |
| 要約のファイル名では `/` が `-` に潰れ、別のブランチが同じファイルになる | **採る** | `judgeSummary(summary, headRefName)` が要約の中の branch と照合するので、共有すれば閉じる。テストで固定する |
| 宣言した集合と事後検索だけでは (1)(2)(3) を覆えない。予防にならない・索引の遅延・並行セッションによる誤報・他リポジトリの close・`gh pr merge` の終了コードとマージ成否のずれがある | **採る** | 予防: マージ前に `closingIssuesReferences` を宣言集合と**完全一致**で照合し、他リポジトリの参照も一致の対象にする。(1): 番号ごとに `gh issue view` で状態を引く。マージの成否は `gh pr view --json state,mergedAt` で確定してから検証へ進む。宣言外の close は報告にとどめ、並行セッションの可能性を出力に書く |
| Dependabot のマージ済み PR は 9 件ある。「直近 200 件に 0 件」は観測範囲の取り方による偽 | **採る**（`-L 1000` で再測: #312〜#875、最新は 2026-08-08） | 要約が無ければ止め、迂回口は作らない。代わりに復帰手順を出力に書く: `gh pr checkout <N>` → `npm run verify:tree -- run --sha HEAD ...` → 道具を再実行 |
| `delete_branch_on_merge: true` なので、フラグに関係なく head が消える（#830 の timeline では merged の 3 秒後に `head_ref_deleted`） | **採る** | base になっている open PR の検査は、フラグに依らず無条件で行う。SKILL の節題がフラグ依存の書き方なので直す |
| ⚠ `closed:>=` の `>` は shell を経由すると壊れる | **採る** | gh は `shell: false` で呼ぶ（`verify-tree.mjs` の `shell: true` は真似ない） |
| ⚠ `gh issue list` の既定は `-L 30` | **採る** | `-L` を明示し、件数が上限に達したら exit 2 |
| ⚠ 索引の遅延 | **採る**（形を変える） | 宣言した issue が検索に現れなければ短く再試行し、それでも現れなければ「検索が未反映」として exit 2 |
| ⚠ ADR の「受容した残余」と SKILL の「唯一の防御」が写しとして偽になる | **採る** | 更新対象に入れる |
| ⚠ fork PR が `--base` の一覧に出るか／remote が増えたときの `gh -R` | 採らない（記録のみ） | fork PR の運用は無く、remote は origin 1 本だけ。道具は `-R` を渡さず gh の既定リポジトリに従う |
| 自 PR が `merged:>=` に出ても、issue 側の `closed:` が効いている証拠にはならない | **一部採る** | 時刻の形の検算としては有効（同じ構文・同じ境界）。issue 側の索引遅延は、上の再試行と番号ごとの状態確認で覆う |
