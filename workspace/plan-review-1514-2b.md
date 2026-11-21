# plan-review 1514 Step 2b（独立導出）

## 対象 issue

#1514「差分・範囲・規範の基準とする版を 1 か所で宣言する」（関連 #1507・#1510・#1504）。
導出は worktree `chore/1514-base-version`（HEAD 8a905f17 時点）の追跡ファイルから行った。`workspace/` は読んでいない。

## 計画を目にしたか

目にしていない（`workspace/` 配下は開いていない。grep はすべて `':!workspace'` か具体パスで実行）。

## 1. 正本の置き場所の候補

| 候補 | 常時ロード | 書式の役割 | 評価 |
|---|---|---|---|
| `AGENTS.md`「検証の作法（全タスク共通）」(AGENTS.md:73-82) | する | トリガー → 行動／参照先 | 既に近い注意がある: AGENTS.md:81「`git diff main...HEAD` は作業ツリーを見ない。`..` と空白区切りの 2 点形も同じ」。起点の版も「全タスク共通」の作法で、スキル・agents・docs から 1 行で指せる。費用は常時ロード面の増分 |
| `docs/development-principles.md` に新節 | しない | 局面 → 立てる問い（development-principles.md:120） | 「局面ごとに立てる問い」表（:11-26）に 1 行足して節を指す形。ただしこの規範は「問い」より「起点は origin/main、fetch してから、3 点形」という行動で、書式が合いにくい。#1510（アンカー版）・#1507（merged 判定）まで寄せると「判断の型」になりやすく、こちらが向く |
| `docs/build-commands.md` | しない | 索引 + 行動形 | 既に `race:boundaries --base` の注意（:244）がある。だがコマンドリファレンスであり、スキル・agent の差分取得の正本としては射程が狭い |
| `docs/glossary.md` | しない | 語・原語・意味の表 | 「基準の版」「アンカー」を語として定義する手はあるが、行動を求める規範の置き場ではない |

導出: 起点（origin/main・fetch・3 点形）だけなら `AGENTS.md`「検証の作法」が最短（隣の :81 と同根で、常時ロードなので委譲先も起動時に受け取る。ただし委譲先は主の作業ツリー版を受け取る＝#1510 の問題自体がここに効く ⚠️）。#1507・#1510 の決着まで同じ正本に寄せるなら `docs/development-principles.md` の新節 + `AGENTS.md`:81 付近から 1 行参照、が書式の役割表と整合する。

## 2. 差分・範囲の起点を書いている全箇所と分類

母集団の出し方: `git grep -n -E "ローカルの .?main|origin/main|git fetch|main\.\.\.|main\.\.HEAD|3 点形|2 点形|merge-base|--base" -- . ':!workspace' ':!docs/superpowers' ':!docs/adr' ':!docs/design'` と、`git (diff|log|show|branch -d|switch)`・`古いこと|古い .?main|分岐元|起点|upstream|@\{u\}` の概念語 grep（`.claude`・`docs`・`scripts`・`.github`・`.githooks`・`AGENTS.md`・`CLAUDE.md`）。`.github` は該当 0 件（`fetch-depth`・`base_ref`・`origin/` なし）。

「ローカルの `main` は古い」の写し（issue の `git grep "ローカルの .main"` で 5 件、いずれも確認）:

| 箇所 | 中身 | 分類 |
|---|---|---|
| `.claude/agents/code-reviewer.md:11` | `git diff origin/main...<sha>` + 「起点をローカルの `main` にしないのは、古いことがあり…」 | 理由文 → 参照に置き換える。コマンドは手順として残す。**`git fetch` が無い**（下記） |
| `.claude/skills/implement/SKILL.md:143` | 「差分は `git fetch` のあと `origin/main...HEAD` で取る（ローカルの `main` は古いことがあり…）」 | 括弧内 → 参照に置き換える。fetch とコマンドは手順として残す |
| `.claude/skills/retrospective/SKILL.md:23-28` | `git fetch origin` / `git log origin/main..HEAD` / `git diff origin/main...HEAD --stat` + 括弧の理由（ローカル main が古い／3 点形にする／2 点形はマージ後に自分の変更が消え他の PR の変更が逆向きに出る） | 括弧 → 参照に置き換える。コマンドは残す。**2 点形の害（マージ後の消失・逆向き）は起点以外の主張**なので正本へ移すか残す（§4） |
| `.claude/skills/start-issue/SKILL.md:39` 末尾 | 「ローカルの `main` はこの手順では更新されないので、差分の起点には `origin/main` を使う」 | 参照に置き換える（ただし「この手順でローカル main が更新されない」は start-issue 固有の事実で、残す価値あり ⚠️） |
| `docs/build-commands.md:244` | 「分岐元が `main` なら…`origin/main` を渡す（ローカルの `main` は古いことがあり…）」 | 括弧 → 参照に置き換える。「別の feature ブランチから切ったら分岐元を渡す」は手順として残す |

言い回しの違う同じ主張・コマンド・例:

| 箇所 | 中身 | 分類 |
|---|---|---|
| `.claude/skills/implement/SKILL.md:116` | `git diff --name-only origin/main...<sha>`（rule 照合の母集団） | 手順として残す。**`git fetch` が無い** |
| `.claude/skills/start-issue/SKILL.md:30-39` | `git fetch origin` → `git switch --no-track -c … origin/main`、fetch 失敗で止まる、`--no-track` の理由 | 手順として残す（切る基点の規範の適用例。`--no-track` と pre-bash の upstream 判定は起点とは別の主張で動かさない） |
| `.claude/skills/deps-update/SKILL.md:24-27` | 「worktree からは `main` を最新にできず、古い `main` から切ることになる」「`git fetch` して `main` を最新にする」「`main` から…ブランチを作る」 | ⚠️ 規範と食い違う候補。(a) `git fetch` はローカル `main` を進めない（`pull --ff-only` か `origin/main` から切る必要）(b)「worktree からは最新にできない」は start-issue Step 2 が `origin/main` から切るようになった今は前提が古い。規範に合わせて `origin/main` から切る手順へ変えるか、本体で走らせる別の理由（依存更新の影響範囲）だけを残すかを決める。「変えない」なら理由が要る |
| `.claude/skills/race-check/SKILL.md:20` | `--base` の SSOT は build-commands と明記 | 変えない（既に参照） |
| `.claude/skills/implement/SKILL.md:139` | 「`/race-check` の母集団は `--base` 起点の累積差分」 | 変えない（版の選択ではなく母集団の不変性の主張） |
| `AGENTS.md:81` | 例 `git diff main...HEAD`（作業ツリーを見ない罠） | 変えない候補（主張は作業ツリーで、起点ではない）。ただし例がローカル `main` なので、正本と同じ節に置くなら例を `origin/main...HEAD` へ揃える ⚠️ |
| `docs/check-skill-skeleton-design.md:31` | 「差分アンカーならリビジョンを明示する（`git diff main...HEAD`）」 | ⚠️ 例が規範に反する（ローカル `main`）。凍結扱いの `docs/design/` ではなく生きた文書（AGENTS.md「ドキュメント参照」の check 系スキルの骨格）なので、例を `origin/main...HEAD` に直すか正本参照を添える候補 |
| `.claude/hooks/pre-bash.mjs:293` | pull 拒否メッセージ「feature ブランチでは `git fetch` してから `git merge`」 | 変えない（取り込み手順で、差分の起点ではない） |
| `CLAUDE.md:15,30` | main を取り込むときは merge | 変えない（同上） |
| `.claude/rules/governance-docs.md:17` | `git show HEAD:<file>` と作業ファイルの比較 | 変えない（main 起点ではない） |
| `scripts/race-boundaries.mjs:12-15,214,244,251` | `--base` 必須、`git merge-base <base> HEAD`、作業ツリー対 merge-base（`main...HEAD` を使わない理由） | 変えない（利用者が base を渡す道具。規範は渡す値を決める側＝build-commands:244） |
| `scripts/merge-pr.mjs:20,217` | `git fetch -q origin refs/pull/<N>/head` | 変えない（PR head を取る fetch で、起点ではない） |

`git fetch` が欠けている箇所:
- `.claude/agents/code-reviewer.md:11`（`origin/main...<sha>` を使うが fetch 指示なし。呼び出し側の implement 3b にも fetch は :143 以外に無い）
- `.claude/skills/implement/SKILL.md:116`（同上）
- `docs/build-commands.md:244`（`origin/main` を渡せと言うが fetch に触れない）
- `.claude/skills/deps-update/SKILL.md:26` は fetch はあるが、それでローカル `main` が最新になるという誤った含意（上記）
- ⚠️ 影響の見立て: branch は start-issue Step 2 で fetch 直後の `origin/main` から切られ、refs は worktree 間で共有なので、3 点形なら古い `origin/main` でも merge-base はほぼ正しい。害が出るのは「ブランチへ新しい main を取り込んだが `origin/main` は古い」組合せで、`git fetch` してから merge する限り起きにくい。規範で「取る前に fetch」とするなら、上の 3 箇所は規範に反する状態になる

## 3. 射程の注意（射程に入れると既存の道具が規範に反する条項）

- **「比較は 3 点形」を全称で書くと反する**:
  - `.claude/hooks/pre-bash.mjs:531-533` は意図して 2 端点（`git diff --name-only summary.sha target`）。コメントに「3 点形は、HEAD が sha の祖先へ戻ると空を返す」。
  - `scripts/race-boundaries.mjs:251` は作業ツリー対 merge-base（`diff-index`）で `...` を使わない（未コミット分を含めるため。AGENTS.md:81 と同根）。
  - `.claude/rules/governance-docs.md:17` の `git show HEAD:<file>` 対作業ファイル。
  → 射程は「`main` を起点にした範囲（ブランチの変更の母集団）」に限り、sha 対 sha・作業ツリー対 base は外すと書く必要がある。
- **「merged か」は `origin/main` の祖先かで判定する**（#1507 の決着）を規範に入れると:
  - `scripts/verify-tree.mjs:398-409` の `git branch -d`（判定はブランチの upstream か、`root`＝本体チェックアウトの HEAD に対して）が反する。
  - `scripts/clean-worktrees.mjs:9-10,77-92` も `git branch -d` を「成果保全の最終防衛」として git に委ねる設計で反する。さらにコメント :77-80 は「squash マージのみの本リポジトリでは worktree 削除後にブランチだけ残るのが標準経路」と書く。squash でマージされたコミットは `origin/main` の祖先にならないので、「祖先なら merged」は squash 済みブランチを merged と判定しない。「merged」の語を規範で定義すると、squash 済み（内容は取り込み済み・祖先ではない）とのずれが生じる ⚠️。
  - 規範に入れるなら、この 2 本を同じ変更で直す（#1507 を吸収）か、「従わない理由」を書く必要がある（受け入れ条件「各場所がそれに従うか、従わない理由が書かれている」）。
- **「委譲先が読む規範はアンカーの sha の版」**（#1510）を規範に入れると: 隔離した委譲先は起動時に主の作業ツリー版の `AGENTS.md`・`CLAUDE.md` を受け取る（AGENTS.md:3、#1510）。これは harness の挙動で道具側では変えられないので、規範は「Read で読み直す」という委譲指示（`.claude/skills/implement/SKILL.md` 3b「委譲へ渡すもの」:110-125 付近）を要求する形になる。正本を `AGENTS.md` に置くと、その正本自体が主の版で届く再帰がある ⚠️。
- **`/retrospective` の範囲**（#1504 論点 3）: `origin/main..HEAD` はマージ後・複数 PR で空になる（retrospective/SKILL.md:24）。「起点は origin/main」を規範にしても範囲の問題は解けない（起点が正しくても範囲が空）。規範の適用で片づけたことにすると受け入れ条件を偽に満たす ⚠️。
- `/start-issue` の `--no-track`（:39）と pre-bash の `@{u}..HEAD`（pre-bash.mjs:390-402）: upstream は同名のリモートブランチであって `origin/main` ではない。規範に「upstream を `origin/main` にする」が紛れると PR 前ゲートが誤作動する。射程に入れない。
- `deps-update` は意図してローカル `main`（本体チェックアウト）で動く。規範に入れるなら手順を変えるか例外を書く。

## 4. 置き換えで落ちうる主張

- retrospective/SKILL.md:28「2 点形は merge-base を取らずに 2 つのツリーを比べるので、マージの後に走らせると自分の変更が消え、他の PR の変更が逆向きに出る」— 3 点形にする**理由**。正本が「3 点形」だけを書くと理由が落ちる。
- start-issue/SKILL.md:39「ローカルの `main` はこの手順では更新されない」— start-issue 固有の帰結（だからローカル main が古くなる、の原因）。#1507 の「起きやすくなった」原因でもある。
- code-reviewer.md:11「worktree はコミットから切られていて作業ツリーが空なので、`git diff HEAD` は空を返す」— 起点ではなく比較相手の主張。理由文を丸ごと参照に置き換えるときに一緒に消さない。
- build-commands.md:244「別の feature ブランチから切ったブランチでは、その分岐元を渡す」— `origin/main` の例外。正本が「起点は `origin/main`」とだけ書くと、スタックしたブランチの扱いが落ちる。
- implement/SKILL.md:143「対象は、チェックアウト中の作業ブランチ名で名指す（引数を省くと…upstream の状態で変わる）」— `/code-review` の対象指定の主張で、起点の括弧と隣接している。
- 各所の「他の PR の差分が所見／母集団／サイクルに混ざる」— 症状の名前が場所ごとに違い（所見・母集団・サイクル）、その場の読者への害を示している。参照に置き換えると、その場で何が壊れるかが見えなくなる（許容するかは判断）。
- start-issue/SKILL.md:39「`git fetch` が失敗したら止まる」— fetch 失敗時の振る舞い。正本に「取る前に fetch」だけ書くと、失敗時にどうするかが落ちる。

## 5. 凍結された歴史を対象外とする理由

- `docs/adr/`: AGENTS.md「ドキュメント参照」で「書いた後は更新しない記録」ではないが、ADR は試し直すと高くつく否定の知識で連番なし・凍結扱い（メモリの #593 合意）。grep では起点の写しは該当なし（除外して数えたので要再確認 ⚠️。除外前の `git grep "ローカルの .main" -- docs/adr` を計画側で一度打つとよい）。
- `docs/design/`: AGENTS.md「設計メモ…書いた後は更新しない記録なので」と明記 → 対象外が成り立つ。
- `docs/superpowers/`: 同種の過去の計画・設計（development-principles.md:126 が経緯の出典として参照）。対象外は成り立つ。
- ただし `docs/check-skill-skeleton-design.md` は名前に design を含むが `docs/design/` 配下ではなく、AGENTS.md「ドキュメント参照」に生きた文書として載っている → 凍結扱いにできない（§2 で候補に挙げた）。

## 導出した変更ファイルとシンボル（まとめ）

- 正本: `AGENTS.md`「検証の作法（全タスク共通）」または `docs/development-principles.md` 新節（+ 局面表 :11-26 に 1 行）
- 参照へ置き換え: `.claude/agents/code-reviewer.md:11`、`.claude/skills/implement/SKILL.md:143`、`.claude/skills/retrospective/SKILL.md:28`、`.claude/skills/start-issue/SKILL.md:39`、`docs/build-commands.md:244`
- fetch の追加候補: `.claude/agents/code-reviewer.md:11`、`.claude/skills/implement/SKILL.md:116`、`docs/build-commands.md:244`
- 食い違いの判断が要る: `.claude/skills/deps-update/SKILL.md:24-27`、`docs/check-skill-skeleton-design.md:31`、`AGENTS.md:81`（例）
- #1507 を吸収するなら: `scripts/verify-tree.mjs`（`remove`・:398-409）、`scripts/clean-worktrees.mjs`（:77-92）とそれぞれの `.test.mjs`
- #1510 を吸収するなら: `.claude/skills/implement/SKILL.md` 3b「委譲へ渡すもの」
- #1504 論点 3: `.claude/skills/retrospective/SKILL.md` Step 1（規範の適用だけでは解けない）
- セーフティネット該当: `.claude/skills/**`・`.claude/agents/**`・`scripts/**`（`.claude/rules/safety-nets.md` paths）と規範文書 `AGENTS.md` → CLAUDE.md ルール 2 によりマージは合意後
- `npm run governance:check`（見出しを新設したら G-heading-refs 系の参照整合）

## ⚠️ まとめ

1. 正本を `AGENTS.md` に置くと、隔離した委譲先は主の作業ツリー版を受け取る（#1510 の再帰）
2. 「merged ＝ `origin/main` の祖先」は squash 済みブランチを merged と判定しない。clean-worktrees のコメントが前提にする標準経路と語がずれる
3. deps-update の「`git fetch` して `main` を最新にする」は、fetch でローカル `main` が進まないので不正確の疑い
4. fetch 欠落の実害は 3 点形なら小さい見立て（未実測）
5. `docs/adr` の該当 0 件は除外して数えたので未確認
6. #1504 論点 3 を「規範の適用」で閉じると、範囲が空になる問題が残ったまま受け入れ条件を満たして見える
