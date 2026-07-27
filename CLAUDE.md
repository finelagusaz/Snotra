# CLAUDE.md

このリポジトリで Claude Code が作業するときの運用ガイド。共通開発プロセス（ワークフロー・事前チェック）は `AGENTS.md`（次行で自動読込）、モジュール固有の不変条件は各サブディレクトリの `CLAUDE.md`（→ `AGENTS.md`「ドキュメント参照」）。

@AGENTS.md

## 最重要ルール（常に適用）

1. **`main` へ直接コミット・プッシュしない** — 必ず feature ブランチ（`feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>`）を作成してからコミットする
2. **エージェント設定（スキル・フック・rules）の変更は合意してから** — エージェントの行動やワークフローを制約する設定はチームの共有物であり、Claude が単独で判断しない

## シェル環境（Windows / PowerShell）

| やらないこと | 代わりにやること | 理由（過去の事故） |
|---|---|---|
| bash の HEREDOC（`<<EOF` / `<<'EOF'`） | 一時ファイルに書き出して `git commit -F <tmpfile>`、または PowerShell here-string `@'...'@`（閉じ `'@` は必ず行頭） | here-string の引用境界が壊れ、終端マーカーがコミットメッセージ本文に漏れる事故が起きている |
| 文字列中のパスに `\` 区切り | `/` で統一する | PowerShell でも Git/Node/Cargo は `/` を受け付ける。`\` はエスケープが必要になり壊れやすい |
| Python で非 ASCII をそのまま標準出力 | `PYTHONIOENCODING=utf-8` を付ける | cp932 コンソールで `—`・日本語などを print すると `UnicodeEncodeError` で落ちる（JSON/ログ整形で多用） |

## Git/GitHub 運用

- **main 保護の実体は `.githooks/` と GitHub ruleset である** — `.githooks/` の 4 hook が commit / 非 FF merge / rebase / push を拒み（`githooks.test.mjs` で実測）、GitHub ruleset `default` が main への直接 push を拒む（実測）
- **Layer 1（`.githooks/`）は best-effort である** — hook は追跡ファイルゆえ、含まないコミットの checkout は fail-open。`cherry-pick` / `revert` / `am` / `branch -f` / `update-ref` も `pre-commit` を呼ばず**沈黙で main が進む**（実測）。**取りこぼしは push 時に GitHub ruleset が捕捉する**（不在検知は意図的に置かない）
- **`--no-verify` は人間専用** — `.githooks/` を迂回する。Claude は使用してはならない。迂回しても main への直接 push は GitHub ruleset が拒む（実測）
- **main の同期は `git pull --ff-only` を使う** — 非 FF の `git pull` は main にマージコミットを作るため `.githooks/pre-merge-commit` が拒否する。FF ならマージコミットが生じず hook は呼ばれない
- **マージで閉じる issue を決めるのは PR 本文であり、`gh pr merge` の `--subject` / `--body-file` では抑止できない**（#488 実測）。auto-close は本文の**どこにあっても** `close`/`fix`/`resolve` 系 9 形（大文字小文字問わず・表やチェックリスト内も）でマージ時に走り、PR テンプレートが `Closes` を埋めるため**書いた覚えが無くても残る**。hook も見ていない（→「フック」の (A2)）。**だから下の手順が唯一の防御である**。なぜこの機構になるか（2 経路の可視性の非対称・マージ方式では逃げられない・squash 設定と復元レシピ）は `docs/adr/0002-squash-merge-issue-autoclose.md`

  手順（squash マージでは常にこの順。`<PR>` は PR 番号、`<issue>` は issue 番号）:
  1. **マージ直前に** `gh pr view <PR> --json closingIssuesReferences` を**必ず**見る。これが GitHub の計算した「いま閉じる issue」である
  2. 一覧に閉じたくない issue があれば **PR 本文を編集して手順 1 を実行し直す**（`gh pr edit <PR> --body-file <tmp>`）。**一覧から消えるまで繰り返す。** どの行のどの語が効いたかを推測しない — 認識されるのは `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形で大文字小文字を問わず、表やチェックリストの中の行も、1 行に同居する複数の参照も効く。**編集を終えてよいと決めるのは一覧であって、自分のキーワード走査ではない**。マージ時の `--subject` / `--body-file` では止められない
  3. `--subject` / `--body-file` は squash commit のメッセージを整えるためだけに使う。**closing keyword を書いてはならない**（散文の "partially fixes #N" も効く）— 書くと手順 1 の一覧に現れないまま閉じる。省けば squash 本文は **PR 説明文そのもの**になる（表・チェックリスト込みで冗長）
  4. マージ後に**必ず**、次の 3 つを確認する。**`closingIssuesReferences` を数えるだけでは足りない** — それは PR 本文からその瞬間に再計算される値であって、閉じた事実そのものではない:
     - 取り直した `gh pr view <PR> --json closingIssuesReferences` の全件が意図どおり閉じたか
     - **残すと決めた issue が今も `OPEN` か**（`gh issue view <issue> --json state`）。正しく動いていればそれらは上の一覧に現れない。**ゆえに一覧を数えるだけでは、守りたい当の issue を一度も見ないことになる**
     - `gh issue list --state closed --search "closed:>=<mergedAt>"`（`mergedAt` は `gh pr view <PR> --json mergedAt`）。どちらの一覧にも属さない「知らないうちに閉じた issue」を拾う、唯一の接地した観測点
     誤って閉じていたら `gh issue reopen <issue>`（close イベントは履歴に残り、close を契機に動く下流は巻き戻らない。**reopen は回復であって、事前確認を省く免罪符ではない**）

  **手順 1 の一覧が「閉じる issue のすべて」になるのは、手順 3 を守り、かつ確認からマージまで PR 本文が変わらなかったときだけである。** 本文を凍結する機構は無く、`gh pr merge --auto` は確認とマージを引き離すため**使わない**。

## フック（.claude/settings.json）

エージェントの操作には以下のフックが介入する。**どちらのフックも、発火（`matcher`）は `.claude/settings.json` が、判定は各スクリプトが SSOT である** — PreToolUse は `.claude/hooks/pre-bash.mjs` の `decide`、PostToolUse は `.claude/hooks/post-edit.mjs` の `selectChecks`。**main 保護の実体はここではない** — リポジトリの状態は hook の視界の外にあるため、`.githooks/` と GitHub ruleset が担う（→「Git/GitHub 運用」）。

| フック | 発火条件（一覧は `docs/hooks.md`） | 正しい対応 |
|---|---|---|
| PR 作成前 push チェック（PreToolUse） | `gh pr create` が**コマンド位置**にあり、安全と確認できないとき（未 push＝空 PR / `Closes` 誤 close 防止）。`&&` で `git push` が先行するなら通る。`workspace/plan.md` に未チェックの `- [ ]` が残るときも拒む（#749・鎖の安全とは独立に判定） | `git push -u origin HEAD` してから PR を作る（または `&&` で繋ぐ）。**鎖に `cd` を含めない**——作業ディレクトリを変えると対象リポジトリを判定できず拒否される（実測）。未チェック項目は完了させて `[x]` にするか、やらないと決めた項目は計画から外して理由を記録する |
| 編集後の自動検証（PostToolUse） | 編集した `file_path` の種類で決まる（写像の SSOT は `post-edit.mjs` の `selectChecks`） | **検査が割り当てられているファイルでは、沈黙は合格を意味する**（割り当ての SSOT は `selectChecks`）。失敗時のみ `exit code` と再現コマンドと診断が会話に届く。手動での再実行は不要 |

- **(A2)「外部 API の不可逆呼び出し」のうち hook が守るのは `gh pr create` だけである**（#488 実測・**意図的な非対称**）。`merge` / `close` を hook で守らない 3 理由・Layer 0（`squash_merge_commit_message=PR_BODY`）での遮断・設定 read-back の検知器を置かない判断は `docs/adr/0002-squash-merge-issue-autoclose.md` が SSOT。残余は上の手順 3 に委ねられる
- **検出は exit code、出力は証拠**（#471）。成功した検査は何も出力せず、失敗したときだけ `--- <検査>: 失敗 (exit N) ---` と再現コマンドが会話に現れる。**沈黙しうる経路はすべて塞いであり、その閉塞を壊す変更を `.claude/hooks/` に入れてはならない**（経路の内訳は `docs/hooks.md`）
- **沈黙が「合格」なのは `selectChecks` に検査が割り当てられたファイルだけである**（#497・機構ではなく規範ゆえ前提を忘れれば false green が再発する）。`*.md` 全般・`SPEC.md`・`scripts/` 配下の非 TS ファイル・`.github/workflows/`・`Cargo.lock` の沈黙は「何も走らなかった」である（`scripts/*.ts` は「include 対象外」の一行が出るため沈黙しない）。決定的な項目（参照実在・索引・スキル表・SPEC 番号・rules glob・コマンド写像）は PR CI の `governance-check` job（`skip-ci` 非対象・#587）が事後に捕捉し、その検査対象外（責務の妥当性等の意味的整合）は**受容する残余**である
- **フックの改修者向けの実装契約・機構・保守**は `docs/hooks.md`（原理は `docs/development-principles.md` §6・§7）。フック改修時は `.claude/rules/safety-nets.md` からも配送される

## チーム憲章

- **意図は明確に、指示は短くていい** — 「なぜそうしたいか」が共有されていれば、具体的な手順は Claude が判断する。意図のない指示より、意図のある短い一言の方がよい結果になる
- **複雑さや意図不明さに気づいたら声を上げる** — 設計が複雑すぎる、コードの意図が読めない、と感じたらどちらが先でも「あ」の一言でいい。指摘のタイミングに遠慮はいらない
- **「やりすぎでは」を歓迎する** — 提案・実装・ドキュメントのどれに対しても、削る・簡略化するという方向の指摘を双方が行う
- **記録への信頼で動く** — 記憶ではなく AGENTS.md・CLAUDE.md・スキル・RETROSPECTIVE.md への記録がチームの連続性を作る。気づきはその場で記録する

## サブエージェント委譲と worktree

- **並列エージェント委譲はファイル境界で衝突を予測してから行う** — 同一ファイルに触りうるタスクは直列化するかマージ順を決める。境界は「実装中に踏み込みうる隣接ファイル」まで含めて見積もる。リベース解決はコンテキストを保持した実装エージェント本人に依頼するのが最短（#439, #435）
- **委譲はコンテキストを継承しない** — メインエージェントの system prompt にしか無い事実（メモリ領域の絶対パス等）はサブエージェントのプロンプトへ明示的に渡す——渡し忘れると見えないものを「無いもの」として報告する。**`allowed-tools` はインライン実行のスキルを拘束しない**（実測）——frontmatter に `Agent` が無いことを根拠に「このスキルは委譲しない」と推論してはならない。また**委譲した検査が対象を読む時刻は制御できない＝検査対象を変更しながら検査を走らせない**（#489）——**起動したことを相手は知らないので、「以降この範囲を触るな」と伝えるのは委譲側の責務である**。**委譲した検査の成果物は、呼び出し側が指定したパスへ書かせる**（返り値に依存させない）：実装の成果は git に残るので報告が落ちても検知できるが、レビュー・判定は会話にしか無く、届かなければ実施の有無すら区別できない（#725 で 6 回中 5 回・束C で 2 回中 2 回。落ちる機序は `docs/development-principles.md`「デバッグ・バグ修正」）
- **長時間の委譲タスクは中断を前提に設計する** — セッションリミット・API エラーで途中終了しうる。大きなタスクは Phase 分割し「各 Phase の検証 green 後にコミット」を指示に含める（#431）

## コミュニケーション原則

- **実行にバイアスをかける** — 具体的な計画や修正指示が既にあるなら、事前の全体探索もプランモードも挟まず最初の Edit/Write から着手する。コミット・PR 作成の指示は確認なしに即実行する（→「最重要ルール」1）。不明点があれば焦点を絞った 1 つの質問をしてから実装に移る
- **計画書・設計書を提示された実装は忠実に実装する** — 要素の省略・統合・削除は明示的に指示されたときだけ行う
- **意図的なリファクタリングの結果を元に戻さない** — `/simplify` 等のレビュー系スキルでも、意図的な分割・改名・責務分離は維持する。「重複に見えるが意図的に分けた」構造の変更はユーザーに確認する
- **分析・調査・助言を求められたら、調査結果のみを報告する** — 明示的に指示されない限り実装計画やコード変更に踏み込まない

## 利用できるスキル

トリガー（どの変更でどの検査に振るか）の SSOT は `AGENTS.md`「条件別チェック」表、引数の形は各 `SKILL.md` の `argument-hint`。**この表が索引するのは `disable-model-invocation: true` の user 起動専用スキルだけである** — 残りは harness が skill roster を `description` ごと毎セッション注入するため、書き写すと同じ面に二重で課税される（射程は G8 が双方向で固定する）。

| スキル（user 起動専用） | 使うとき |
|---|---|
| `/health-check`      | 定期・サイクル完了後の governance:check + 意味的整合（報告のみ・修正しない） |
| `/retrospective`     | サイクル終了後の教訓抽出・残タスク振り分け・RETROSPECTIVE.md 上書き |
| `/deps-update`       | cargo/npm 依存の一括更新と PR 作成・CI 確認（マージは手動） |

サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement`「4b. code-reviewer エージェント」が自動で起動する。
