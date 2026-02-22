# Codex Issue Automation

## 概要

外部ユーザーが Issue を起票する前提で、Codex 自動実装はラベル起動ではなくコメントコマンド起動です。

1. allowlist 登録者が `/codex spec` コメントで仕様を確定
2. allowlist 登録者が `/codex run` コメントで実行
3. Spec QA（不明点レビュー）
4. 最新 `origin/v*` ブランチ選定
5. `codex/issue-<番号>` ブランチ作成
6. Codex 実装
7. Codex レビュー + 修正（最大2ループ）
8. 対象 `v*` ブランチ向け Draft PR 作成

対象 workflow: `.github/workflows/codex-issue-implement.yml`

## 必要な Repository 設定

### Labels

`.github/labels.yml` のラベルを作成してください。

- `codex:needs-clarification`
- `codex:spec-approved`
- `codex:in-progress`
- `codex:reviewing`
- `codex:ready-pr`
- `type:*`
- `size:*`

### Secrets

- `OPENAI_API_KEY` または `CODEX_API_KEY`

### Variables

- `CODEX_RUNNER_COMMAND`
- `CODEX_ALLOWED_ACTORS`

`CODEX_RUNNER_COMMAND` はワークフローが実行する Codex コマンドです。  
以下プレースホルダーを使用できます。

- `{mode}`: `implement` / `review` / `fix`
- `{prompt_file}`: プロンプトファイル絶対パス
- `{output_file}`: 出力ファイル絶対パス（主に review）

例（ラッパースクリプトを使う場合）:

```bash
bash ./scripts/run-codex.sh --mode {mode} --prompt {prompt_file} --output {output_file}
```

`CODEX_ALLOWED_ACTORS` は `/codex run` 実行を許可する GitHub ユーザー名の CSV です。  
例:

```text
owner-name,maintainer-a,maintainer-b
```

注意:

- `CODEX_ALLOWED_ACTORS` が空の場合は実行を拒否します。
- allowlist 外ユーザーの `/codex run` は拒否されます。
- review モードでは `{output_file}` へレビュー結果を保存する契約にしてください。
- `CODEX_RUNNER_COMMAND` で `run-codex.sh` を使う場合は、デフォルトブランチに `scripts/run-codex.sh` が必要です。
- GitHub Hosted Runner (`ubuntu-latest`) では `codex` CLI をジョブ内でインストールし、`codex --version` で事前検証してください。

## コメントコマンド

### `/codex spec`

allowlist 登録者が次の形式で仕様を投稿します。

```md
/codex spec
## 背景/目的
...

## 受入条件
- ...

## 非対象
- ...
```

投稿時に Spec QA を実行し、結果をコメントで返します。  
PASS した場合は `codex:spec-approved` が付与されます。

### `/codex run`

allowlist 登録者が Issue コメントで `/codex run` を投稿すると自動実装を開始します。  
`codex:spec-approved` が付与されている場合のみ実行されます。  
`issue_comment.created` のみ受け付けます（コメント編集は対象外）。

## Spec QA の停止条件

次のいずれかで `BLOCK` になり、実装を停止します。

- 背景/目的 が未記入
- 受入条件 が未記入、または箇条書きなし
- 受入条件 に曖昧語のみの項目がある
- 非対象 が未記入
- 受入条件 と 非対象 が矛盾

`BLOCK` 時の動作:

- Issue に不足項目コメント投稿
- `codex:needs-clarification` を付与

仕様更新後、allowlist 登録者が `/codex run` を再投稿すると再実行されます。

## 失敗時通知

`/codex run` 実行中に失敗した場合は、Issue に次を自動コメントします。

- 失敗通知
- Workflow Run URL
- 失敗ジョブ名とジョブログURL
- 失敗ステップ名（取得できる範囲）
- 主な確認ポイント（`CODEX_RUNNER_COMMAND` / `scripts/run-codex.sh`）

あわせて `codex:needs-clarification` を付与し、進行中ラベル（`codex:in-progress` / `codex:reviewing`）を外します。
