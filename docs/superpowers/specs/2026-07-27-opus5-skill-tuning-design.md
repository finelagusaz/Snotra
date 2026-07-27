# Opus 5 プロンプティングガイドに沿ったスキル・エージェントの調整 — 設計

日付: 2026-07-27 / 対象: `.claude/skills/*/SKILL.md`（12 本）・`.claude/agents/code-reviewer.md`

## 背景

Anthropic の "Prompting Claude Opus 5" は、旧モデル向けプロンプトから次を削れと述べる——明示的な検証ステップ指示（"include a final verification step"・"use a subagent to verify"）、自己再確認の指示（"double-check your answer"）、サブエージェントへの過剰な委譲。加えて、Opus 5 は**ディスクへ書く成果物が長くなる**・**進捗をよく実況する**・**自発的に委譲しやすい**と述べる。

このリポジトリの 13 ファイル（1908 行）を全読した結果、**ガイドが削れと言う対象はほとんど存在しない**ことが分かった。検証文の大半は「モデルが自分を疑う」ものではなく、**信頼できない外部チャネルの観測**である。

## 分類の方法 — 引用の有無で二分する

判別子は「その行が issue 番号か『実測』を背負っているか」。このリポジトリでは記録の規律により、この判定が機械的に立つ。

| 列 | 性質 | ガイドの射程 |
|---|---|---|
| **B** | 実測された失敗の記録。`/plan-review` Step 2 の台帳と `mkdir` の非冪等化（#725・実測）、Step 2b の独立再導出（#495・列挙型トリガの的中率 0% の記録つき）、`/health-check` Check 7（#489/#492/#488）、`/persistence-check` の全パターン（#338/#343/#394/#461/#646）、`/symmetric-check` 2c（#671 PR D） | **射程外**。削れば documented incident が再開する |
| **A** | 引用を持たない汎用の自己検証・様式 | 検討対象。実測に支えられていない行でもあり、safety-nets の観点でも弱い |

A 列は 4 件しか存在せず、うち削るのは 2 件（変更 4・5）、1 件は変更 4 と連動して適合し、1 件は対象外と判定した（後述）。

**ガイドの「code review では conservative と言うな・全部報告して別パスで濾せ」は既に適合済み**である。`code-reviewer` は Critical/High/Medium/Low の 4 段階すべてを報告し、`/implement` 4b が Critical/High で絞り、`/plan-review` Step 3 が再照合による降格を行う。ガイドが推奨する形そのものであり、変更しない。

## 事実確認 — effort は今回の主レバーにならない

`claude.exe` の文字列から frontmatter のキー集合を抽出して確認した。

- **SKILL.md に effort は書けない。** キー集合は `allowed-tools` / `disallowed-tools` / `argument-hint` / `disable-model-invocation` / `user-invocable` / `argNames` / `declaredFields` の 7 つで、effort は不在（完全な並びを抽出したため不在が言える）
- `effort` が実在するのは Workflow の `agent()` opts（`schema / model / effort / isolation / agentType`）と CLI の `--effort`
- **`.claude/agents/*.md` はキー名が確定できない。** Agent ツールの説明は「reasoning effort はその定義から来る」と明言するが、agent 側キー列の抽出断片（`when_to_use` / `when-to-use` / `isolation` / `maxTurns` / `getSystemPrompt`）に effort は現れなかった。断片ゆえ不在の証明にはならない

確かめるには実 Agent を 1 体起動するほかなく、得られる効果は `code-reviewer` 1 体分の費用のみ。**未確認のまま計画に載せない**という規律に従い、任意の後続項目として切り出す（本サイクルの変更には含めない）。

## 変更 1 — 成果物の長さ較正（4 箇所・各々別文言）

Opus 5 の成果物肥大に対する較正を、**各 authoring site に 1 行ずつ**置く。共通原理を 1 か所に置いて参照させる形は採らない——各サイトの較正は**内容が異なる**ため写しにならず、共通原理を別途書けば「同じ教訓を 2 か所に書かない」に抵触する。

| 場所 | 加える較正 |
|---|---|
| `/start-issue` Step 3（`research.md`） | 調査で判明した事実のみを書く。読んだが計画に影響しなかったファイルの要約・一般論・前置きを書かない |
| `/start-issue` Step 4（`plan.md`） | 散文は各チェック項目の判断根拠に限る（チェックリスト規定は既存のまま） |
| `/plan-review` Step 2 の出力形式（項目 3） | 各項目 1〜3 行。根拠は `file:line` か grep 結果そのもの。前置き節・要約節を作らない |
| `/retrospective` Step 5 | 各見出し 3〜6 行 |

`/health-check` は「該当が無いセクションは省略してよい」を既に持つため追加しない。

## 変更 2 — アドホック委譲の抑制（`/implement` のみ）

`/implement` Step 4b に一文を加える:

> スキルが規定するこの 1 体以外に `Agent` を起動しない。調査は自分の `Grep` / `Read` で完結させる。

`allowed-tools` の `Agent` は 4b のために残す。**`/plan-review` の fan-out（Step 2 の複数体 + Step 2b の 1 体）には触れない**——B 列であり、#495 が「3 サイクル連続でトリガが外れ、独立再導出だけが毎回漏れを拾った」と記録している。

## 変更 3 — ナレーションの型（`/implement` のみ）

`/implement`「出力」節に一文:

> 作業中の実況は、発見があったときと方針を変えたときに限る。

2 か所に書くとドリフトするため、最長時間のスキル 1 本に限定する。`/plan-review` はサブエージェント待ちが主で実況の余地が小さい。

## 変更 4 — `code-reviewer` Phase 1 の圧縮

現行 8 項目のうち 6 項目を削る。

| 項目 | 措置 | 理由 |
|---|---|---|
| コードの重複がないか（DRY） | 削除 | Phase 2c が同じ検査を持つ |
| パフォーマンスへの配慮 | 削除 | Phase 3 が持つ |
| テストカバレッジ | 削除 | `AGENTS.md`「開発ワークフロー」6 が持つ |
| 簡潔さ・可読性 / 命名の適切さ / エラーハンドリング | 削除 | 指示なしで見る層。Opus 5 で指示すると二度目の実行になる |
| シークレット・API キーの露出 | **残す** | 見落とすと不可逆 |
| システム境界での入力バリデーション | **残す** | このプロダクト固有（IPC・Win32 境界） |

Phase 1 の柱「コードが意図した変更を正しく実装しているか」は残す。

## 変更 5 — `/implement`「出力」から項目 4「全変更の diff」を削除

git に残るものを会話へ再掲している。Opus 5 の冗長性と掛け算になる。項目 1〜3（入口判定・検証結果・コミット）は残す。

## 変更 4・5 で「4b の `code-reviewer` 起動」がガイドに適合する

ガイドの "do not use subagents to verify or double-check your own work" は、形の上では 4b に当たる。**しかし変更 4 と連動して適合する**——Phase 1 の汎用 6 項目こそが「二度目の実行」であり、それを削れば `code-reviewer` に残るのは SPEC.md 同期・`plan.md` 不変条件の照合・「変更不要」判断の再評価であって、いずれも**別文書との突き合わせ**であり自己検証ではない。よって 4b の起動は残す。

`/implement` Step 4a の check スキル群は**インライン実行ゆえ委譲ではなく**、ガイドの対象外と判定した。

## 敵対的読者レビュー（`.claude/rules/safety-nets.md` の要求）

スキルは規範型セーフティネットであり、本改修自体にこの手順が掛かる。

**通してはならないシナリオ**（合格条件）:

1. *手を抜く読者* が「長さ較正」を口実に、根拠 `file:line` や「未検証（理由）」欄を省く
2. *手を抜く読者* が「委譲するな」を口実に、`/plan-review` Step 2 / 2b の規定 fan-out を省く
3. *規則を全部守る読者* が Phase 1 の圧縮を「削った観点は見なくてよい」と読む
4. *規則を全部守る読者* が diff 削除を「変更内容を報告しなくてよい」と読む

**上限 2 巡。** 2 巡後に残る抜け道は受容する残余として本節へ追記する。

## 検証

`.claude/skills/**` に PostToolUse 検査は割り当てられていない——**編集後の沈黙は「何も走らなかった」である**。PR 前に `npm run governance:check`（カテゴリ F）を手で実行する。

## 受容する残余

- **agent frontmatter の effort は未確認のまま残る。** 実測には Agent 1 体の起動が要り、効果は `code-reviewer` 1 体分の費用に限られる
- **長さ較正は規範であり機構ではない。** 較正の遵守を測る検出器は置かない（`npm run governance:check` の母集団にも入らない）。カナリアで守るのは沈黙する経路だけでよく、成果物の冗長さは読めば分かる＝沈黙しない
