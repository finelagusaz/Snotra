# Codex Issue Automation

## 概要

`codex:implement` ラベル付き Issue を起点に、次を自動実行します。

1. Spec QA（不明点レビュー）
2. 最新 `origin/v*` ブランチ選定
3. `codex/issue-<番号>` ブランチ作成
4. Codex 実装
5. Codex レビュー + 修正（最大2ループ）
6. 対象 `v*` ブランチ向け Draft PR 作成

対象 workflow: `.github/workflows/codex-issue-implement.yml`

## 必要な Repository 設定

### Labels

`.github/labels.yml` のラベルを作成してください。

- `codex:implement`
- `codex:needs-clarification`
- `codex:in-progress`
- `codex:reviewing`
- `codex:ready-pr`
- `type:*`
- `size:*`

### Secrets

- `OPENAI_API_KEY` または `CODEX_API_KEY`

### Variables

- `CODEX_RUNNER_COMMAND`

`CODEX_RUNNER_COMMAND` はワークフローが実行する Codex コマンドです。  
以下プレースホルダーを使用できます。

- `{mode}`: `implement` / `review` / `fix`
- `{prompt_file}`: プロンプトファイル絶対パス
- `{output_file}`: 出力ファイル絶対パス（主に review）

例（ラッパースクリプトを使う場合）:

```bash
./scripts/run-codex.sh --mode {mode} --prompt {prompt_file} --output {output_file}
```

注意:

- リポジトリごとに Codex CLI の実行方法は異なるため、実際のコマンドに合わせて設定してください。
- review モードでは `{output_file}` へレビュー結果を保存する契約にしてください。

## Spec QA の停止条件

次のいずれかで `BLOCK` になり、実装を停止します。

- 背景/目的 が未記入
- 受入条件 が未記入、または箇条書きなし
- 受入条件 に曖昧語のみの項目がある
- 非対象 が未記入
- 受入条件 と 非対象 が矛盾

`BLOCK` 時の動作:

- Issue に質問コメント投稿
- `codex:needs-clarification` を付与
- `codex:implement` を除去

回答後、`codex:implement` を再付与すると再実行されます。
