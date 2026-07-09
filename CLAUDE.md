# CLAUDE.md

このリポジトリで Claude Code が作業するときの運用ガイド。

- 共通開発プロセス（ワークフロー・事前チェック・環境制約）は `AGENTS.md`（次行で自動読込）
- モジュール固有の不変条件は各サブディレクトリの `CLAUDE.md`（`snotra-core/` / `src-tauri/` / `ui/` / `snotra-settings/`）
- 本ファイルの各ルールは「**太字 = 守る指示**、後続 = 理由・過去の事故」の形式。迷ったら太字部分に従えば安全

@AGENTS.md

## 最重要ルール（常に適用）

作業種別を問わず適用される4つ。詳細は各セクションを参照。

1. **`main` へ直接コミット・プッシュしない** — 必ず feature ブランチ（`feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>`）を作成してからコミットする
2. **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook はコマンド実行の**前**に upstream を評価するため、`git push -u origin HEAD && gh pr create` は必ずブロックされる（→「Git/GitHub 運用」）
3. **bash の HEREDOC（`<<EOF`）を使わない** — 複数行テキストは一時ファイルか PowerShell here-string（→「シェル環境」）
4. **エージェント設定（スキル・フック・rules）の変更は合意してから** — Claude が単独で判断しない（→「チーム憲章」）

## MCP ツール

- **Tauri v2 / SolidJS / Rust クレートの最新 API 調査には context7 MCP を使う**（設定済み）

## シェル環境（Windows / PowerShell）

このリポジトリは Windows + PowerShell 環境で運用されている。Bash 系の慣習をそのまま持ち込むと、過去のセッションで複数回踏んだ摩擦を再発させる。

| やらないこと | 代わりにやること | 理由（過去の事故） |
|---|---|---|
| bash の HEREDOC（`<<EOF` / `<<'EOF'`） | 一時ファイルに書き出して `git commit -F <tmpfile>`、または PowerShell here-string `@'...'@`（閉じ `'@` は必ず行頭） | here-string の引用境界が壊れ、終端マーカーがコミットメッセージ本文に漏れる事故が起きている |
| 文字列中のパスに `\` 区切り | `/` で統一する | PowerShell でも Git/Node/Cargo は `/` を受け付ける。`\` はエスケープが必要になり壊れやすい |
| `/tmp` への書き込み | `$env:TEMP` 配下に置くか Write ツールで作る | Windows の Bash ツールに `/tmp` は無く、`cat > /tmp/...` は `FileNotFoundError` で失敗する |
| Python で非 ASCII をそのまま標準出力 | `PYTHONIOENCODING=utf-8` を付ける | cp932 コンソールで `—`・日本語などを print すると `UnicodeEncodeError` で落ちる（JSON/ログ整形で多用） |

## Git/GitHub 運用

- **main 保護の実体は `.githooks/` と GitHub ruleset である** — `.githooks/{pre-commit,pre-merge-commit,pre-rebase,pre-push}` が `git commit` / 非 FF の `git merge` / `git rebase` / `git push` を拒否する（`.githooks/githooks.test.mjs` の回帰テストで実測。`git -C <別ツリー>`・linked worktree・`ui/` 等のサブディレクトリからの起動を含む）。GitHub ruleset `default` は main への直接 push を拒否する（実測）。force-push と削除は `non_fast_forward` / `deletion` 規則が `active`（設定の read-back のみ。実地の試行は未実施）。git は hook を「操作されるツリーのトップ」を cwd として呼び、相対 `core.hooksPath` もそこを基準に解決する（実測）。bootstrap は `npm install`（`prepare` が `core.hooksPath` を設定する）
- **`.githooks/` を含まないツリーでは Layer 1 は存在しない** — hook は追跡ファイルなので、`.githooks/` が無いコミットを checkout すると git は「hook 無し」として操作を通す（fail-open）。古いタグや導入前のコミットが該当する。**ローカルの取りこぼしは push の時点で GitHub ruleset が捕捉する**（直接 push の拒否は実測済み）。`.githooks/` は「手前で親切に止める」best-effort な層であり、その不在を検知する仕組みは意図的に置いていない
- **Layer 1 が見ていない操作がある** — git は `cherry-pick` / `revert` / `am` / `branch -f` / `update-ref` で `pre-commit` を呼ばない。main 上でこれらを実行すると **hook は何も出力せず main が進む**（実測）。`commit --amend` と `merge --squash` 後の `commit` は拒否される。取りこぼしは push の時点で GitHub ruleset が捕捉する
- **`--no-verify` は人間専用** — `.githooks/` を迂回する。Claude は使用してはならない。迂回しても main への直接 push は GitHub ruleset が拒む（実測）
- **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook は `tool_input` 全体を grep したうえで、コマンド実行の**前**に `@{u}` を評価する。`git push -u origin HEAD && gh pr create` は upstream 未設定と判定されて必ずブロックされる（この誤爆の根治は #482）
- **main の同期は `git pull --ff-only` を使う** — 非 FF の `git pull` は main にマージコミットを作るため `.githooks/pre-merge-commit` が拒否する。FF ならマージコミットが生じず hook は呼ばれない
- **複数 issue にまたがる PR を squash マージするとき auto-close を明示制御する** — ブランチ各コミット本文の `Fixes/Closes #N` は squash 時に GitHub が拾い、意図しない issue を閉じうる。一部だけ閉じたい場合（例: 中核 issue は Phase 残しで open、対症療法 issue のみ close）の手順:
  1. `gh pr merge --squash --subject "...(#issue) (#PR)" --body-file <tmp>` で最終メッセージを明示し、`Closes`/`Refs` を制御する
  2. マージ後に `gh issue view <N> --json state` で意図どおり閉じた/残ったかを検証する

## フック（.claude/settings.json）

エージェントの操作には以下のフックが介入する。PreToolUse の発火条件は `.claude/settings.json` を、PostToolUse の発火条件と検査対応表は **`.claude/hooks/post-edit.mjs` の `selectChecks`** を SSOT とする。**main 保護の実体はここではない** — リポジトリの状態は hook の視界の外にあるため、`.githooks/` と GitHub ruleset が担う（→「Git/GitHub 運用」）。

| フック | 発火条件 | 正しい対応 |
|---|---|---|
| PR 作成前 push チェック（PreToolUse） | 未 push コミットまたは upstream 未設定での `gh pr create`（空 PR / `Closes` 誤 close 防止） | `git push -u origin HEAD` してから PR を作る |
| 編集後の自動検証（PostToolUse） | `tool_input.file_path` が属するツリーからの相対パスで判定。`*.rs` → clippy（`snotra-core` / `snotra-settings` 配下ではその crate のテストも）、`ui/src/**/*.{ts,tsx,mts,cts}`（`*.test.ts(x)` を除く）→ typecheck、`tauri.conf.json` / `config.toml` → WARN、`.claude/settings.json` と `.claude/hooks/**` → hook-selftest | **沈黙は合格を意味する**。失敗時のみ `exit code` と再現コマンドと診断が会話に届く。手動での再実行は不要 |

- **検出は exit code、出力は証拠**（#471）。検査が成功した hook は何も出力しない。失敗したときだけ `--- <検査>: 失敗 (exit N) ---` と再現コマンドが会話に現れる。診断が予算（`head`/`tail` 数行〜数十行）を超えても、再現コマンドで全件を見られるので取りこぼしは無い
- **沈黙を「合格」と読めるのは、沈黙しうる経路をすべて塞いだから**。タイムアウト（検査ごと 300s で自ら打ち切る）・出力溢れ・起動失敗・スクリプト内部エラーは、いずれも必ず報告される。この契約を壊す変更を `.claude/hooks/` に入れてはならない
- **`.ts`/`.tsx` を編集したのに何も出ない場合は 2 通りある**: 型検査が通った、または `tsconfig.json` の `include` 対象外（`e2e/`・`*.config.ts`・`*.test.ts(x)`）。後者では `[post-edit] ... は tsconfig の include 対象外です` という一行が出る
- **hook は worktree でも「そのファイルが属するツリー」を検査する**。root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`CLAUDE_PROJECT_DIR` の意味論に依存しない。ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決し、相対 `file_path` を受け取った場合は cwd 基準で `resolve` する
- **`.claude/settings.json` の編集は file watcher が即座に拾う**（セッション再起動は不要・実測）。壊れたスクリプトを配線するとその瞬間から全検査が沈黙する。そのため `.claude/settings.json` と `.claude/hooks/**` の編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する
- `config.toml` はリポジトリに実在しない（ランタイムのユーザー領域ファイル）ため、WARN の真陽性は事実上 `tauri.conf.json` のみ

## チーム憲章

Claude とユーザーが一緒に作業するときの関係性の原則。

- **意図は明確に、指示は短くていい** — 「なぜそうしたいか」が共有されていれば、具体的な手順は Claude が判断する。意図のない指示より、意図のある短い一言の方がよい結果になる
- **複雑さや意図不明さに気づいたら声を上げる** — 設計が複雑すぎる、コードの意図が読めない、と感じたらどちらが先でも「あ」の一言でいい。指摘のタイミングに遠慮はいらない
- **「やりすぎでは」を歓迎する** — 提案・実装・ドキュメントのどれに対しても、削る・簡略化するという方向の指摘を双方が行う
- **記録への信頼で動く** — 記憶ではなく AGENTS.md・CLAUDE.md・スキル・RETROSPECTIVE.md への記録がチームの連続性を作る。気づきはその場で記録する
- **エージェント設定の更新は相談してから** — スキル・フック・rules など、エージェントの行動やワークフローを制約する設定の変更は Claude が単独で判断せず、合意してから行う

## コミュニケーション原則

### 着手の判断

- **タスクが真に曖昧でない限り、分析・計画より実行にバイアスをかける**
- **ユーザーが具体的な計画や修正指示を既に提示している場合、プランモードへの遷移・事前の全体探索を禁止する** — 読むファイルは直接関係する最小限（1〜2ファイル）に絞り、最初の Edit/Write から着手する
- **コミット・PR 作成を指示された場合、確認やプランモードなしに即実行する** — コミットは必ず feature ブランチで行う（→「最重要ルール」1）
- **不明点がある場合は、1つの焦点を絞った質問をしてから実装に移る**

### 実装・レビュー時

- **計画書・設計書を提示された実装は、内容を忠実に実装する** — 計画書の要素を省略・統合・削除するのは明示的に指示された場合のみ行う
- **意図的なリファクタリングの結果を元に戻さない** — `/simplify` などのコードレビュー系スキルを実行するとき、意図的な分割・名前変更・責務分離は維持する。「重複に見えるが意図的に分けた」構造は、ユーザーに確認してから変更する

### 調査・助言の依頼

- **分析・調査・助言を求められたら、調査結果のみを報告する** — 明示的に指示されない限り、実装計画やコード変更に踏み込まない

## 利用できるスキル

| スキル               | 使うとき                                                                | 呼び出し例                                                                     |
|----------------------|-------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| `/research-review`   | 調査（research.md）完了後: サブエージェントで影響範囲の漏れ・誤認識を並列検証 | `/research-review`                                                        |
| `/plan-review`       | 計画（plan.md）完了後: サブエージェントで影響範囲・不変条件・スコープを並列検証（横断変更では独立導出+差分も） | `/plan-review`                                                            |
| `/symmetric-check`   | コードパス変更・バグ発見時に対称ペアの適用漏れを確認                    | `/symmetric-check result-clicked: added emitSelectionUpdate`                   |
| `/dry-check`         | 関数を新規定義・変更したとき、手書き重複が残っていないか確認            | `/dry-check show_main_and_emit: show() + set_focus() + emit(window-shown)`     |
| `/race-check`        | async 関数を新規追加・変更したとき、各 await 地点の状態競合リスクを検証 | `/race-check executeInstantCommandSelected: await api.executeInstantCommand()` |
| `/cache-check`       | キャッシュロジックの追加・変更時に述語の単調性と状態遷移の安全性を検証  | `/cache-check search_with_options: use_incremental 判定`                       |
| `/persistence-check` | シリアライズ・on-disk 形式（index.bin/config.toml/history/window.bin）の変更時に version バンプ要否・旧形式の後方互換テスト・デコード失敗時のデータ保全を検証 | `/persistence-check IndexCache: Cow 統合`                       |
| `/state-check`       | UI モード・ガード条件の追加・変更時に直交性・リセット経路・SPEC §8.6 整合を検証 | `/state-check InstantCommandMode 追加`                                    |
| `/health-check`      | 定期・サイクル完了後にドキュメントと実装の整合性を10項目で検証（報告のみ・修正しない） | `/health-check`                                                           |
| `/retrospective`     | サイクル終了後に教訓の抽出・残タスクの振り分け・RETROSPECTIVE.md 上書きを実施 | `/retrospective`                                                               |
| `/start-issue`       | GitHub issue から作業を開始（実装前段階のブランチ作成・調査・計画まで）  | `/start-issue 123`                                                        |
| `/implement`         | コード変更を伴うタスクの実装（調査からコミットまでのフルサイクル）      | `/implement キーボードショートカットの追加`                                     |
| `/deps-update`       | cargo/npm の依存を一括更新し PR 作成・CI 確認まで（マージは手動）       | `/deps-update` または `/deps-update npm`                                       |

サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement` Step 5b が自動で起動する。
