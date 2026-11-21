# #1495 変更対象の独立導出

対象: GitHub issue #1495「chore(skills): implement の軸の違うレビューを出力の母集団に入れ、対象を固定する」
導出日: 2026-11-21 / 読んだ HEAD: 9c125eaa（= origin/main）/ ブランチ: chore/1495-orthogonal-review-output
計画（workspace/ の中身）は目にしていない。workspace/ 配下はこのファイルを書いた以外に触っていない。
走査範囲: `.claude/skills/` `.claude/agents/` `.claude/rules/` `docs/` `AGENTS.md` `CLAUDE.md`（`scripts/` は禁止範囲のため未走査 → ⚠️ 下記）。
読んだ rule: `.claude/rules/safety-nets.md`（`.claude/skills/**` に当たる）、`.claude/rules/governance-docs.md`（自動配送）。

## 1. issue の 2 点それぞれの変更対象

すべて `.claude/skills/implement/SKILL.md`。

### 1-A. 出力節の母集団と表に、軸の違うレビューを足す

| 場所 | 今言っていること | 変更後に偽になること / 要る変更 |
|---|---|---|
| 「## 出力」リード文（173 行）「レビューの母集団は、3a の check スキル全件と 3b の委譲である。」 | 母集団 = 3a + 3b | 3c の `/code-review high` が母集団外。3c の軸の違うレビューを足す |
| 出力テンプレートの見出し `### レビュー（3a の check スキル全件 + 3b の委譲）`（186 行） | 表の母集団ラベル | 同上。見出しに 3c を足す（見出し名を変えるので旧名で git grep → 本ファイル以外に出現なし・確認済み。`docs/superpowers/` の過去計画にも出現なし） |
| 同表（188-189 行）の列 `実施（ラウンド数・件数・修正の有無） / 該当なし / 未実施 \| 根拠` | 値の語彙は既に「該当なし」を持つ | 列はそのまま使える見込み。ただし「該当なし」の根拠に「条件 (1)(2)(3) のどれも立たなかった理由」を書く旨を箇条で足す（issue の要件） |
| 箇条「レビューは、1 検査 1 行とする。…ラウンド数を書く」（206 行） | 1 検査 1 行 | 偽にはならない（`/code-review high` も 1 行）。ラウンド数の記述は「1 回通す」と整合するか確認のみ。⚠️ 3c の所見を直してツールを回し直した後に `/code-review` を再度回すかは 140 行が述べていない（回すならラウンド数が意味を持つ） |
| 箇条「レビューの母集団は、3a の列挙と一致する。…表へ check スキルを足すときは、3a の列挙も同じ変更で直す」（207 行） | 母集団 = 3a の列挙 | 既に 173 行（3a + 3b）と食い違っており、3c を足すとさらに偽。「表の check スキル部分は 3a の列挙と一致する」へ射程を絞るなど書き直しが要る（後半の同期規則は維持） |
| 箇条「委譲の行には、アンカーにした sha と変異注入の結果を書く（沈黙は…区別できない）」（208 行） | 委譲行の根拠の必須項目 | 偽にはならない。軸の違うレビューの行にも同様に「対象（下記 1-B の起点..sha）と件数」を根拠欄の必須にするかを決める（issue の背景「実施しなかったことと区別できない」への直接の対処） |
| 箇条「未実施（理由: 省いた）」（205 行） | 時間・手間を理由にしない | 変更不要。3c の行にもそのまま効く |

### 1-B. `/code-review high` へ渡す対象を名指す

| 場所 | 今言っていること | 変更後に偽になること / 要る変更 |
|---|---|---|
| 「### 3c. 報告を受けてから」最後の箇条（140 行）「主が Skill ツールで `/code-review high` を呼ぶ（…）」 | 対象を渡していない。`/code-review` は対象なしだと「現在の diff」を見るが、Step 3 の時点で作業ツリーは clean（3a で `git status --porcelain` 空を確認済み） | 引数に対象を書く（例: `/code-review high <ブランチ名>` か、起点..sha の範囲）。「Step 3 は sha に対して行う」（90 行）と整合させ、対象がアンカーの HEAD sha であることを書く |
| 「## Step 3」冒頭（90 行）「カテゴリ A〜F の検証も、レビューも、…コミットを 1 つ作り、その sha に対して行う」 | 原則 | 変更不要の見込み。ただし `/code-review` は主の作業ツリー（主ツリー）で走る点が「主エージェントの作業ツリーでは行わない」と字面で衝突する ⚠️（対象が不変 sha で、作業ツリーを読まない形で渡せるなら衝突しない。渡し方次第） |

## 2. issue が名指していない波及先

- `.claude/skills/implement/SKILL.md` 207 行（上記）— issue は「母集団の記述と表」とだけ言うが、207 行の「3a の列挙と一致する」も母集団の記述であり、直さないと 3c 追加後に偽が残る。
- 同 135 行「ラウンドの結果を、`plan.md` の消し込みへ 1 行で書く。書くのは、アンカーの sha・ツールの判定・指摘の件数・生き残った変異の数」— `/code-review` の実施・件数を plan.md の記録に載せるか未定。載せないと、Step 4 で workspace/ を消した後に実施の痕跡が出力表の 1 行だけになる ⚠️（判断事項）。
- 同 151 行（Step 4 の否定の知識の回収）は「`plan.md`・委譲先の `.delegate/` の報告・本サイクルの検討」を回収元に挙げる。`/code-review` の所見を主が退けた場合の判断はこの 3 つのどれにも入らない可能性 ⚠️（「本サイクルの検討」に含まれると読めば変更不要）。
- 同 116 行（3b、rules 照合の差分）`git diff --name-only main...<sha>` と `.claude/agents/code-reviewer.md` 起動時手順 1（11 行）`git diff main...<sha>` — どちらもローカル `main` を起点にしている。下記 4 の起点の決定と一貫させるかの判断対象（issue の範囲外として残すなら、その旨を PR 本文へ）。
- 同 125 行「`Agent` を起動してよいのは、この1体だけ」と 140 行の「上の『1体だけ』の例外」— `/code-review high` が内部でサブエージェントを起動するなら例外の書き方の射程に関わる。⚠️ high が subagent を起動するかは未確認（skill の説明は max / ultra について「many reviewer subagents」と言うのみ）。
- `/implement` 以外で「軸の違うレビュー」や `/code-review`（スキル）に触れる現役の箇所: 走査範囲内で 0 件（`code-review[^e]|軸の違う` を `.claude/skills` `.claude/agents` `.claude/rules` `AGENTS.md` `CLAUDE.md` で grep。`docs/` の一致は `docs/superpowers/`（書いた後は更新しない設計メモ・過去計画）だけ）。`/implement` の出力表（`## 実装結果`・`### レビュー（3a…`）を他スキルが読む箇所も 0 件。

## 3. 変えなくてよいと判断したファイル

- `AGENTS.md` — 「条件別チェック」表は check スキルのトリガー表で、`/code-review` は check スキルではなく 3c 固有の条件で起動する。表へ足すと 3a の列挙との同期規則（207 行）に巻き込まれる。ドキュメント参照の `code-reviewer.md` 行は無関係。
- ルート `CLAUDE.md` — `/code-review`・軸の違うレビューへの言及なし。ただし safety-nets 対象（`.claude/skills/**`）なので、PR 本文冒頭に「マージは合意してから」が要る。
- `.claude/agents/code-reviewer.md` — issue の 2 点には不要（起点の一貫性を揃える判断をした場合だけ触る）。
- `.claude/skills/start-issue/SKILL.md` — `/implement` の 3b を参照するだけで、出力表・3c には触れない。
- `docs/check-skill-skeleton-design.md` 119 行 — 3a の列挙を写しとして挙げるが、3a の列挙は変わらない。
- `docs/adr/ADR-implement-verification-tool-tree.md` — 3b を参照。3c・出力に触れない（ADR は凍結された歴史でもある）。
- `docs/superpowers/**` — 過去の計画・設計メモ。更新しない記録。
- `docs/build-commands.md` — `/code-review` を扱わない。

## 4. 設計上の判断が要る点

### 4-1. `/code-review high` に渡す対象
- skill の説明が受け付ける形: 「the current diff, or a PR number/branch/path target」。Step 3 の時点では PR は無い（PR は Step 5）ので、PR 番号は使えない。候補はブランチ名か、範囲の指定。
- ⚠️ ブランチを渡したときに何と比べるか（ローカル `main` / `origin/main` / 既定ブランチの merge-base）は、skill 本体が読めないため未確認。issue のとおり実際に試して決める必要がある。試す際は、ローカル `main` が古い今の状態（下記）で差分のファイル一覧が #1495 の変更だけになるかで判定できる。
- 対象は「アンカーの sha」に固定するのが Step 3 の原則と一致する。ブランチ名は HEAD が動けば別の版を指すので、3c の他の箇所（131 行「HEAD を進めない」）と同じく、`/code-review` の実行中は HEAD を進めないことを条件に加えるか、sha を含む範囲で渡す。

### 4-2. 差分の起点（ローカル `main` か `origin/main` か）
- 実測: いまローカル `main` = 6903204e、`origin/main` = HEAD = 9c125eaa。ローカル `main` は 1 コミット古い。この状態で `git diff main...HEAD` を取ると、merge-base が 6903204e になり、#1494（PR #1501）の差分がこのブランチの変更として混ざる。並行セッションが main を進める運用なので、ローカル `main` の陳腐化は日常的に起こる。
- ⇒ 起点は `origin/main`（直前に `git fetch` するか、少なくとも fetch 済みの `origin/main`）が安全。ただし `origin/main` も fetch 以後は古くなりうるので、「fetch してから」を手順に入れるかを決める。
- 既存の 2 箇所（116 行・`code-reviewer.md` 11 行）はローカル `main...<sha>` を使っている。3c だけ `origin/main` にすると不揃いになる。揃えるなら範囲が #1495 を超える（`code-reviewer.md` も safety-nets 対象）。揃えないなら、その判断を PR 本文に残す。⚠️ 既存 2 箇所の陳腐化の害は、委譲先の worktree がアンカーから切られるので差分が膨らむ（偽陽性の追加読み）方向で、取りこぼしはしない。`/code-review` も同じ向きなら害は「余計な所見」に留まるが、ブランチ名を渡したときの比較先が skill 内部で決まるなら、そもそも主が起点を選べない可能性がある ⚠️。

### 4-3. その他
- 表の行の名前（例「軸の違うレビュー（3c・`/code-review high`）」）と、「該当なし」の根拠に何を書かせるか（条件 (1)(2)(3) を 1 つずつ否定する形が、実施しなかったこととの区別に効く）。
- 207 行の書き直しの射程（check スキル部分だけの同期規則に絞る）。
- ⚠️ `scripts/` は走査禁止のため未確認: 出力表の見出し（`### レビュー（3a の check スキル全件 + 3b の委譲）`）を機械で照合する検査・hook が `scripts/` や `.claude/hooks/` にあれば波及する。`npm run governance:check` で G-heading-refs 系が拾うかを実装時に確かめる。
- #1493 は main にマージ済み（6903204e）なので、issue 末尾の「マージの順」の懸念は解消している。
