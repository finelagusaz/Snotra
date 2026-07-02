# CLAUDE.md

このリポジトリで Claude Code が作業するときの運用ガイド。

@AGENTS.md

## 補足（Claude Code 固有）

- context7 MCP が設定済み。Tauri v2 / SolidJS / Rust クレートの最新 API を調べる際は context7 を使う

## シェル環境（Windows / PowerShell）

このリポジトリは Windows + PowerShell 環境で運用されている。Bash 系の慣習をそのまま持ち込むと、過去のセッションで複数回踏んだ摩擦を再発させる。

- **bash の HEREDOC（`<<EOF` / `<<'EOF'`）を使わない** — PowerShell では here-string の引用境界が壊れ、終端マーカーがコミットメッセージ本文に漏れる事故が起きている。複数行のコミットメッセージは一時ファイルに書き出して `git commit -F <tmpfile>` を使うか、PowerShell の here-string `@'...'@`（閉じ `'@` は必ず行頭）を使う
- **パス区切りは `/` を優先** — PowerShell でも Git/Node/Cargo は `/` を受け付ける。`\` を含めるとエスケープが必要になるため、文字列中のパスは `/` で統一する
- **Bash ツールに `/tmp` は無い（Windows）** — 一時ファイルは `$env:TEMP` 配下に置くか Write ツールで作る。`cat > /tmp/...` は `FileNotFoundError` で失敗する
- **Python で非 ASCII を標準出力するときは `PYTHONIOENCODING=utf-8` を付ける** — cp932 コンソールで `—`・日本語などを print すると `UnicodeEncodeError` で落ちる（JSON/ログ整形で多用）

## Git/GitHub 運用

- **`git` コマンドをチェーンしない** — `git checkout <branch> && git rebase main` のような連鎖は `block-main-commit` フックを誤発火させた実績がある。`checkout` と `rebase`、`add` と `commit` のように影響範囲の異なる操作はそれぞれ独立した呼び出しに分ける
- **main の fast-forward 同期は `git pull --ff-only` を使う** — `git merge --ff-only origin/main` はコミットを作らない FF でも `block-main-commit` フックに弾かれる（コマンド文字列一致で判定するため）
- **複数 issue にまたがる PR を squash マージするとき auto-close を明示制御する** — ブランチ各コミット本文の `Fixes/Closes #N` は squash 時に GitHub が拾い、意図しない issue を閉じうる。一部だけ閉じたい場合（例: 中核 issue は Phase 残しで open、対症療法 issue のみ close）は `gh pr merge --squash --subject "...(#issue) (#PR)" --body-file <tmp>` で最終メッセージを明示し、`Closes`/`Refs` を制御する。マージ後は `gh issue view <N> --json state` で意図どおりか検証する

## フック（.claude/settings.json）

エージェントの操作には以下のフックが介入する。発火条件の正確な定義は `.claude/settings.json` を SSOT とする。

- **`block-main-commit`（PreToolUse）**: main ブランチ上の `git commit` / `merge` / `rebase` を拒否する。feature ブランチを作成してから操作する
- **PR 作成前 push チェック（PreToolUse）**: 未 push コミットまたは upstream 未設定の状態での `gh pr create` を拒否する（空 PR / `Closes` 誤 close 防止）。`git push -u origin HEAD` してから PR を作る
- **編集後の自動検証（PostToolUse）**: `.rs` 編集で clippy（`snotra-core` 編集では core テストも）、`.ts`/`.tsx` 編集で typecheck が自動実行される。Edit/Write 後に会話へ流れる clippy / typecheck 出力はこのフック由来であり、手動での再実行は不要

## チーム憲章

Claude とユーザーが一緒に作業するときの関係性の原則。

- **意図は明確に、指示は短くていい** — 「なぜそうしたいか」が共有されていれば、具体的な手順は Claude が判断する。意図のない指示より、意図のある短い一言の方がよい結果になる
- **複雑さや意図不明さに気づいたら声を上げる** — 設計が複雑すぎる、コードの意図が読めない、と感じたらどちらが先でも「あ」の一言でいい。指摘のタイミングに遠慮はいらない
- **「やりすぎでは」を歓迎する** — 提案・実装・ドキュメントのどれに対しても、削る・簡略化するという方向の指摘を双方が行う
- **記録への信頼で動く** — 記憶ではなく AGENTS.md・CLAUDE.md・スキル・RETROSPECTIVE.md への記録がチームの連続性を作る。気づきはその場で記録する
- **エージェント設定の更新は相談してから** — スキル・フック・rules など、エージェントの行動やワークフローを制約する設定の変更は Claude が単独で判断せず、合意してから行う

## コミュニケーション原則

- タスクが真に曖昧でない限り、分析・計画より実行にバイアスをかける
- ユーザーが具体的な計画や修正指示を既に提示している場合、プランモードへの遷移・事前の全体探索を禁止する。読むファイルは直接関係する最小限（1〜2ファイル）に絞り、最初の Edit/Write から着手する
- コミット・PR 作成を指示された場合、確認やプランモードなしに即実行する
- コミットを作成するときは**必ず feature ブランチを作成してから**行う。`main` への直接コミット・プッシュは禁止。ブランチ名は `feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>` とする
- ユーザーが計画書・設計書を提示して実装を依頼した場合、内容を忠実に実装する。計画書の要素を省略・統合・削除するのは明示的に指示された場合のみ行う
- `/simplify` などのコードレビュー系スキルを実行するとき、意図的なリファクタリング（分割・名前変更・責務分離）の結果を元に戻さない。「重複に見えるが意図的に分けた」構造は、ユーザーに確認してから変更する
- 不明点がある場合は、1つの焦点を絞った質問をしてから実装に移る
- ユーザーが分析・調査・助言を求めた場合は、調査結果のみを報告する。明示的に指示されない限り、実装計画やコード変更に踏み込まない

## 利用できるスキル

| スキル               | 使うとき                                                                | 呼び出し例                                                                     |
|----------------------|-------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| `/research-review`   | 調査（research.md）完了後: サブエージェントで影響範囲の漏れ・誤認識を並列検証 | `/research-review`                                                        |
| `/plan-review`       | 計画（plan.md）完了後: サブエージェントで影響範囲・不変条件・スコープを並列検証（横断変更では独立導出+差分も） | `/plan-review`                                                            |
| `/symmetric-check`   | コードパス変更・バグ発見時に対称ペアの適用漏れを確認                    | `/symmetric-check result-clicked: added emitSelectionUpdate`                   |
| `/dry-check`         | 関数を新規定義・変更したとき、手書き重複が残っていないか確認            | `/dry-check show_main_and_emit: show() + set_focus() + emit(window-shown)`     |
| `/race-check`        | async 関数を新規追加・変更したとき、各 await 地点の状態競合リスクを検証 | `/race-check executeInstantCommandSelected: await api.executeInstantCommand()` |
| `/cache-check`       | キャッシュロジックの追加・変更時に述語の単調性と状態遷移の安全性を検証  | `/cache-check search_with_options: use_incremental 判定`                       |
| `/state-check`       | UI モード・ガード条件の追加・変更時に直交性・リセット経路・SPEC §8.6 整合を検証 | `/state-check InstantCommandMode 追加`                                    |
| `/health-check`      | 定期・サイクル完了後にドキュメントと実装の整合性を10項目で検証（報告のみ・修正しない） | `/health-check`                                                           |
| `/retrospective`     | サイクル終了後に教訓の抽出・残タスクの振り分け・RETROSPECTIVE.md 上書きを実施 | `/retrospective`                                                               |
| `/start-issue`       | GitHub issue から作業を開始（実装前段階のブランチ作成・調査・計画まで）  | `/start-issue 123`                                                        |
| `/implement`         | コード変更を伴うタスクの実装（調査からコミットまでのフルサイクル）      | `/implement キーボードショートカットの追加`                                     |
| `/deps-update`       | cargo/npm の依存を一括更新し PR 作成・CI 確認まで（マージは手動）       | `/deps-update` または `/deps-update npm`                                       |

サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement` Step 5b が自動で起動する。
