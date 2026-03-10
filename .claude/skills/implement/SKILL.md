---
name: implement
description: "自律的なフルサイクル開発: 調査 → 計画 → 実装 → 検証 → レビュー → コミット。機能追加・バグ修正・リファクタリングなど、コード変更を伴うタスクの実装時に使用。"
disable-model-invocation: true
argument-hint: "[機能の説明]"
allowed-tools:
  - Bash(cargo *)
  - Bash(npx vite build)
  - Bash(git *)
  - Read
  - Edit
  - Write
  - Grep
  - Glob
  - Agent
---

自律的にフルサイクル開発を行う。質問せず、コードとドキュメントから判断すること。

タスク: $ARGUMENTS

## Step 1 -- 調査

- `workspace/plan.md` が存在する場合、先に読む — `/start-issue` で作成された調査・計画が含まれている。Step 3 に進む。
- それ以外: `SPEC.md` と関連する `CLAUDE.md` を読み、意図とアーキテクチャを理解する
- `$ARGUMENTS` からエントリポイントと関連モジュールを特定する
- 要求された機能と重複する既存コードを検索する
- 3層モデルの制約（意図は SPEC.md、実装はコード）に留意する

## Step 2 -- 計画

- `workspace/plan.md` が存在する場合、それを計画として使用する。別の計画を作成しない。
- それ以外: 変更計画を短いリストにまとめる（どのファイルを作成/変更するか、その理由）
- 計画を会話に出力する
- プロジェクト原則に従う: ロジックは `snotra-core`、薄いラッパーは `commands.rs`（KISS/DRY/YAGNI）

## Step 3 -- 実装

- `snotra-core` の純ロジック変更: 先に失敗する `#[cfg(test)]` テストを書き（Red）、次に実装して通す（Green）
- 計画に沿って変更を行う
- 新しい純ロジックには `snotra-core` に `#[cfg(test)]` ユニットテストを追加する
- `SPEC.md` に記載された挙動に影響する変更の場合、`SPEC.md` も更新する

## Step 4 -- 検証（最大5サイクル）

以下のチェックを順に実行する。失敗した場合、修正して失敗したステップから再実行:

1. Rust 検証（チェーン実行 — 最初の失敗で停止）:
   ```bash
   cargo check -p snotra-core -p snotra && cargo clippy -p snotra-core -p snotra -- -D warnings && cargo test -p snotra-core
   ```
2. フロントエンド検証（TypeScript/フロントエンドファイルを変更した場合のみ）:
   ```bash
   npx vite build
   ```

5サイクル後もエラーが残る場合、中止して診断サマリーを書く:
- 試みた内容
- 残存するエラー
- 根本原因の推定

## Step 5 -- レビュー

`code-reviewer` エージェントを変更に対して実行する。Critical または High の発見事項は修正してから次に進む。

## Step 6 -- コミット

- このタスクで変更したファイルのみをステージする
- `workspace/` ディレクトリが存在する場合、削除してステージに含める（`/start-issue` の引き継ぎバッファは実装完了で役目を終える。git 履歴から復元可能）
- conventional commit を作成する（例: `feat:`, `fix:`, `refactor:`）
- 何を実装し、なぜ実装したかの簡潔な説明を含める

## 出力

以下を報告:
1. 調査結果（Step 1）
2. 変更計画（Step 2）
3. 最終検証結果 — check, clippy, test の出力（Step 4）
4. コミットハッシュとメッセージ（Step 6）
5. 全変更の diff
