# 調査 — issue #1514 差分・範囲・規範の基準とする版を 1 か所で宣言する

## issue の要約

「どの版を基準にするか」を道具や手順が暗黙に選び、選んだ版が古いと黙ってずれる。2026-11-21 に 7 か所で現れた（4 か所はその場で直し、3 か所は未解決: #1507・#1510・#1504 の論点 3）。直すたびに理由の説明が写された。基準の版の規範を 1 か所に置き、写しを正本への参照へ置き換える。

## 関連するファイル・シンボル（`git grep` で実在を確かめた）

### 理由の説明の写し（「ローカルの `main` は古いことがある」系）

`git grep -n "ローカルの .main" -- . ':!docs/superpowers' ':!docs/adr' ':!docs/design'` で 5 か所:

1. `.claude/agents/code-reviewer.md:11` — 「起点をローカルの `main` にしないのは、古いことがあり、他の PR の変更が所見に混ざるため」
2. `.claude/skills/implement/SKILL.md:143`（3c の軸の違うレビュー）— 「差分は `git fetch` のあと `origin/main...HEAD` で取る（ローカルの `main` は古いことがあり…）」
3. `.claude/skills/retrospective/SKILL.md:28` — 「ローカルの `main` は古いことがあり…`diff` は 3 点形にする。2 点形は merge-base を取らずに…」
4. `.claude/skills/start-issue/SKILL.md:39`（Step 2）— 「ローカルの `main` はこの手順では更新されないので、差分の起点には `origin/main` を使う」
5. `docs/build-commands.md:244`（`race:boundaries`）— 「分岐元が `main` なら、ローカルの `main` ではなく `origin/main` を渡す（ローカルの `main` は古いことがあり…）」

ほかに `origin/main` を基準として書く箇所: `.claude/skills/implement/SKILL.md:116`（3b の rules の照合で `git diff --name-only origin/main...<sha>`）、`.claude/skills/retrospective/SKILL.md:24-25`、`.claude/skills/start-issue/SKILL.md:30・36`。

### 2 点形・3 点形の注意の既存の正本

`AGENTS.md`「検証の作法（全タスク共通）」の判定ロジックの項（81 行）に「`git diff main...HEAD` は作業ツリーを見ない。`..` と空白区切りの 2 点形も同じ」がある。これは「作業ツリーを見ない」の注意で、「起点の版が古い」の注意とは別。ここも `main` と書いている。

### 未解決の現れ

- `scripts/verify-tree.mjs` の `remove`（405 行付近）: `git(root, ["branch", "-d", branch])`。`root` は主の作業ツリー。`git branch -d` は、upstream があればそれ、なければ HEAD に merged かで判定する。委譲先のブランチ（`worktree-agent-*`）は upstream を持たず、主の作業ツリーの HEAD が古いローカルの `main`（本体の場合）なら「not fully merged」で失敗する（#1507。2 回観測）
- `scripts/clean-worktrees.mjs:89`: `git(["branch", force ? "-D" : "-d", b])`。同じ判定
- `/implement` 3b: 隔離した委譲先は、起動時の `AGENTS.md`・`CLAUDE.md` を主の作業ツリーから受け取る（#1510）
- `/retrospective` Step 1: HEAD のブランチしか見ない（#1504 の論点 3）

## 再利用できる既存のパターン

- 「照合は SSOT に対して行う」「数え上げは正本を指す」（`AGENTS.md`「検証の作法」）
- `.claude/rules/governance-docs.md` の書く約束 (1)「かぶりなく書く。正本の 1 か所に寄せて、他からは参照する」
- 他の文書を指す書式: `` `<対象>`「<見出し>」 ``

## 技術的な制約

- `AGENTS.md` は常時ロードされる規範文書。変えるとマージの前に合意が要る（ルート `CLAUDE.md` 最重要ルール 2）
- `.claude/skills/**`・`.claude/agents/**`・`scripts/**` はセーフティネット（`.claude/rules/safety-nets.md` の `paths`）
- ガバナンス文書を変えるので `npm run governance:check` が要る。G-references はパスの実在を、見出しの参照は照合しない
- 写しを参照に置き換えても、手順のコマンド（`origin/main...HEAD` など）は各スキルに残る（コマンドは写しではなく手順そのもの）。置き換えるのは理由の説明の部分

## 未解決の疑問

1. 正本をどこに置くか — `AGENTS.md`「検証の作法」（常時ロード・既に git diff の注意がある）か、`docs/development-principles.md`（局面ごとの問い）か
2. 範囲 — 規範と写しの置き換えだけか、#1507（コード + テスト）・#1510（3b に 1 行）・#1504 の論点 3 までこの PR で片づけるか
3. 「merged かの判定は `origin/main` の祖先で」を規範に書くと、#1507 を直すまで `verify:tree remove`・`clean:worktrees` が規範に反する状態になる
4. 81 行の「`git diff main...HEAD`」の例も `origin/main` に揃えるか（作業ツリーの注意としては `main` でも真）

## 敵対的調査（3b）の反映（`workspace/adversarial-1514.txt`）

- 壊せなかった: 写しは 5 か所（概念語でも増えない）／`git branch -d` の判定は一時リポジトリで再現（upstream なし・HEAD が古い main で失敗、HEAD を origin/main にするか upstream を付けると成功）／「起点が古い」と「作業ツリーを見ない」は別の主張
- 採った（壊せた）:
  - `origin/main` 自体も `git fetch` しないと古い。fetch を書いているのは retrospective・start-issue・implement:143 だけで、code-reviewer:11 と build-commands:244 には無い → 規範に「起点は fetch したあとの `origin/main`」を入れ、build-commands からは規範を指す。code-reviewer は委譲先が読むので、fetch は呼び出し側（`/implement` 3b）が委譲の前に済ませる形にするかを計画で決める
  - 規範の射程を「差分・範囲の起点」に限れば、`git branch -d`（#1507）は射程外で、規範に反する状態を作らない → 「merged の判定」は条項に入れず、#1507 は個別に直す
  - 置き場所は断定しない。AGENTS.md「検証の作法」の検証コマンドの項（81 行・見る範囲の注意）へ足すのが最小（既存の項の延長で、新しい節を作らない）。development-principles は「局面 → 問い」の書式で、手順の基準の宣言には合いにくい
- ⚠️ の扱い:
  - start-issue:39 は主張が違う（「この手順はローカルの main を更新しない」）→ 写しではなく手順の帰結。規範への参照を添えるだけにする
  - deps-update:26（`git fetch` して `main` を最新にする）は本体のチェックアウトで main を pull する手順で、ローカルの main を最新にしてから使うので規範と矛盾しない → 変えない
  - `docs/check-skill-skeleton-design.md:31` の例 `git diff main...HEAD` → 例を `origin/main...HEAD` に揃える
  - squash マージでは委譲先が新しいコミットを作ると origin/main の祖先にならない（clean-worktrees.mjs:76）→ #1507 側の論点。委譲先は `align` で detach しレビューだけでコミットしないので、通常は祖先になる。#1507 に書き添える
  - harness が委譲先ブランチに upstream を付けるかは未測定 → #1507 側の論点
