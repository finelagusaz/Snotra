# 調査 — issue #1494 委譲先に、触るパスの rules を Read で読ませる

## issue の要約

issue は「委譲先は bash 中心で動くので `.claude/rules/` が配送されない」とし、`/implement` 3b と `/start-issue` 3b の委譲の指示に「触るパスの rule を Read で読む」を足すとしていた。前提を測り直した結果、範囲を絞る（ユーザーの裁定 2026-11-21「絞って進める」）。

1. `/implement`「3b. 委譲へ渡すもの」に、自分の worktree の `.claude/rules/` から触るパスに当たる rule を Read で読む指示を足す
2. `AGENTS.md` 冒頭の配送の文に、worktree に隔離したサブエージェントには配送されないことを足す
3. `/start-issue` 3b には足さない
4. 測定結果を #1494 にコメントする

## 測定（2026-11-21・このセッションのサブエージェントの記録 `~/.claude/projects/<project>/<session>/subagents/*.jsonl`）

rules の配送は、記録の `"type":"nested_memory"` の attachment として現れる（主セッションの記録でも同じ形で、`path` に `.claude/rules/*.md` が入る）。

| 委譲先 | `isolation: "worktree"` | AGENTS.md（`Contents of …AGENTS.md`） | `nested_memory` の rules |
|---|---|---|---|
| #1493 の敵対的調査 | なし | あり | safety-nets・governance-docs |
| #1493 の独立導出 | なし | あり | governance-docs（safety-nets は自分で Read 済み） |
| #1493 の実装レビュー | あり | あり | 0（rules を先に自分で Read していたため判定に使えない） |
| 今回の測定用（haiku） | あり | あり | 0（rules を読まず、自分の worktree の `.claude/skills/merge-pr/SKILL.md` だけを Read） |

- 隔離しない委譲先は、主セッションのパス（`…/local-memory-insights-abf21e/.claude/…`）を Read したときに配送を受けた
- 隔離した委譲先は、自分の worktree のパス（`…/.claude/worktrees/agent-<id>/.claude/skills/…`）を Read しても配送を受けなかった
- 機序は確かめていない。推測: `paths` の照合がセッションの起点（主の worktree のルート）からの相対パスで行われ、`.claude/worktrees/agent-<id>/…` は `.claude/skills/**` などに当たらない

## 敵対的調査（3b）の反映

出典: `workspace/adversarial-1494.txt`

- 採る: 表の 3 行目（隔離した実装レビュー）は判定に使える。自分の worktree の `.claude/agents/code-reviewer.md` を Read した直後（rules を自分で Read する前）の窓に配送が無く、記録全体でも `nested_memory` は 0 件だった。隔離して配送されなかった例は、別のモデルで 2 件になる
- 採る: 機序の推測は不十分。隔離側の `.md` は `governance-docs` の `**/*.md` にも当たるのに届いていない。候補は「起点のルート外のパスは対象外」「`.gitignore` の `.claude/worktrees/` に当たるパスは対象外」「隔離したサブエージェントでは機構自体が無効」で、どれも測っていない。→ `AGENTS.md` には機序を書かない
- 採る（⚠️3）: 測ったのは隔離 2 件と Read の対象 2 種だけ。`AGENTS.md` には全称ではなく、測定の報告の形（「2026-11-21 の測定では、隔離した委譲先に届かなかった」）で書く
- 壊せなかった（維持）: 配送は `nested_memory` の形だけで現れる／隔離しない委譲先は Read の直後に配送を受けた（Bash は契機にならない）／委譲先にも `AGENTS.md` が届く／`isolation` の指定は `/implement` 3b だけ
- 別件として記録: 隔離した委譲先が受ける `AGENTS.md`・ルート `CLAUDE.md` は、自分の worktree ではなく主の worktree の版だった（アンカーとずれうる）。本 issue の範囲外
- 採らない: ⚠️1（haiku の非隔離の対照が無い）— 別モデルの 2 件目で交絡は弱まり、結論（隔離では届かない例がある）の形は変わらない

## 関連するファイル・シンボル

- `AGENTS.md` 冒頭の段落: 「読み込みは Read・Edit などのファイルツールで触ったときに起き、bash の `cat`・`sed`・`grep` で読んでも起きない（#1143 で測り、2026-11-21 にも同じ挙動を観測した。harness の版で変わりうる）。bash で作業するときは、触るパスに当たる rule を Read ツールで読む。」
- `.claude/skills/implement/SKILL.md`「3b. 委譲へ渡すもの」の `Agent` を `isolation: "worktree"` で起動する段の箇条（「意図的に分けた構造を…」「⚠️（確信の持てない所見）も…」の並び）。rule への言及は、timeout の後始末で `.claude/rules/safety-nets.md` の節を指す箇条だけ
- `.claude/skills/start-issue/SKILL.md`「3b. 敵対的調査」: `isolation` を指定しない（隔離しない）
- `.claude/skills/plan-review/SKILL.md`「Step 2」「Step 2b」: `isolation` を指定しない。Step 2 は関連する `.claude/rules/` のパスを渡している
- ルート `CLAUDE.md`「サブエージェント委譲と worktree」: 委譲の一般則。配送の話は無い

## 再利用できる既存のパターン

- 委譲の指示で正本の文書を「全文読ませる」形（`/implement` 3b の `.claude/agents/code-reviewer.md`。名前だけ渡すと委譲先は一般的な観点で代用する）
- 配送の挙動を「#NNN で測り、日付にも同じ挙動を観測した。harness の版で変わりうる」と書く既存の書き方（`AGENTS.md` 冒頭）

## 技術的な制約

- `AGENTS.md` は規範文書、`.claude/skills/**` はセーフティネット。マージは合意してから
- `AGENTS.md` は常時ロード面。手順の本文を置かない（事実と参照だけ）
- 委譲先が読むべき rule は、自分の worktree の版である（主の作業ツリーの版はアンカーと食い違いうる。`/implement` 3b「作業ツリーと食い違ったら `git show <sha>:<path>` が正本」）
- どの rule が当たるかの判定は、各 rule のフロントマターの `paths`。照合の意味論は harness が持つ（bare 名はルート直下のみ、階層横断は `**/` が要る）

## 未解決の疑問

- Q1 `/implement` 3b の指示で、どの rule を読ませるかを主が列挙するか、委譲先に `paths` から判定させるか
- Q2 `AGENTS.md` の文に、隔離したサブエージェントの例外を 1 文で足すだけでよいか（「bash で作業するときは Read で読む」の行動形が、隔離した委譲先には足りない）
