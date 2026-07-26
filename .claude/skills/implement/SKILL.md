---
name: implement
description: "コード変更を伴うタスク（機能追加・バグ修正・リファクタリング）の実装時に使用。調査からコミット作成まで自律的に行う。"
disable-model-invocation: true
argument-hint: "[機能の説明]"
allowed-tools:
  - Bash(cargo *)
  - Bash(npm *)
  - Bash(git *)
  - Read
  - Edit
  - Write
  - Grep
  - Glob
  - Agent
  - Skill
---

自律的にフルサイクル開発を行う。実装判断はコードとドキュメントから自律的に行い、要求判断が必要な場合のみ確認する。

タスク: $ARGUMENTS

## Step 1 — 調査

- `workspace/plan.md` が存在する場合、先に読む — `/start-issue` で作成された調査・計画が含まれている。Step 3 に進む。
- それ以外: `SPEC.md` と関連する `CLAUDE.md` を読み、意図とアーキテクチャを理解する
- `$ARGUMENTS` からエントリポイントと関連モジュールを特定する
- 要求された機能と重複する既存コードを検索する
- 3層モデルの制約（意図は SPEC.md、実装はコード）に留意する

## Step 2 — 計画

- `workspace/plan.md` が存在する場合、それを計画として使用する。別の計画を作成しない。
- それ以外:
  1. 変更計画を短いリストにまとめる（どのファイルを作成/変更するか、その理由）
  2. **要求の曖昧さを判定する** — 「何を作るか」がコードとドキュメントから一意に決まらない場合（UI の見た目、対象範囲、既存動作を変えるか追加するか等）、最も影響の大きい1〜2点に絞って質問し、回答を得てから実装に進む
- 計画を会話に出力する
- プロジェクト原則に従う: ロジックは `snotra-core`、薄いラッパーは `commands.rs`（KISS/DRY/YAGNI）

## Step 3 — 実装

- **純粋核（crate を問わず egui/Win32 非依存でテスト可能なロジック）の追加・変更: 先に失敗する `#[cfg(test)]` テストを書き（Red）、次に実装して通す（Green）**。`snotra-core` に限らず `src-tauri` 等の純粋モジュール（`lifecycle.rs`・`search_state.rs` 等）も対象（`view`/Win32 依存はテスト前提にしない）
- 計画に沿って変更を行う
- 新しい純ロジックには、それが属する crate に `#[cfg(test)]` ユニットテストを追加する
- **ソースファイル（`.rs`/`.ts`/`.tsx`）を新規追加・削除したら、同じ変更で該当 `CLAUDE.md` のモジュール構成節の索引（ファイル名）を更新する**（責務散文は各ファイルの `//!`/TSDoc が正本・#562。索引漏れは `governance:check` が捕捉するが PR まで漏らさない）
- `SPEC.md` に記載された挙動に影響する変更の場合、`SPEC.md` も更新する（`AGENTS.md`「3層分担」に従う）

## Step 4 — 検証（最大5サイクル）

`docs/build-commands.md` の「変更後の検証チェックリスト」を SSOT として、変更したファイルの種類に該当するカテゴリ A〜E をすべて実行する。失敗した場合、修正して失敗したステップから再実行する。

- カテゴリ A（Rust 変更）の clippy・`cargo test -p snotra-core` も SSOT 上「必須」（最初の失敗で停止するチェーン実行を推奨）
- **ソースファイルを追加/削除した場合、カテゴリ F（`npm run governance:check`）も実行する**（モジュール索引・参照・スキル表の整合。#629/#630 で索引更新漏れが CI まで再発した）
- 状態でゲートされた挙動（分岐表示・エラー経路・cold path）は build/test が通っても検証済みとは限らない——その状態を実際に発生させて確認する（`docs/development-principles.md` デバッグ節）
- 具体的なコマンド文字列は `docs/build-commands.md` を参照（二重メンテを避けるためこの SKILL に書かない）

5サイクル後もエラーが残る場合、中止して診断サマリーを書く:
- 試みた内容
- 残存するエラー
- 根本原因の推定

## Step 5 — レビュー

### 5a. check スキルの実行

変更の種類に応じた check スキル（`/symmetric-check`・`/dry-check`・`/race-check`・`/cache-check`・`/persistence-check`・`/state-check`）を、**`AGENTS.md`「条件別チェック（トリガー → 参照先）」表に従って**実行する（トリガー→検査の写像の SSOT はその表。二重管理を避けるためここに再掲しない）。`/symmetric-check` はコードパス変更・バグ修正でほぼ常に該当。発見事項があれば修正してから 5b に進む。

### 5b. code-reviewer エージェント

`code-reviewer` エージェントを変更に対して実行する。Critical または High の発見事項は修正してから次に進む。

## Step 6 — コミット

- **`git branch --show-current` で現在のブランチを確認し、`main` 上にいる場合は feature ブランチ（`feat/` / `fix/` / `chore/` 等）を作成してからコミットする**（`main` 直コミットは禁止・`.githooks/pre-commit` に弾かれる。`/start-issue` を経ていない単体起動時に該当しやすい）
- このタスクで変更したファイルのみをステージする
- **`workspace/` を削除する前に、否定の知識を ADR へ回収する**（#593）: `plan.md`・本サイクルの検討に「代替案 B を検討して却下した」判断があれば、削除で失う前に `docs/adr/NNNN-<title>.md` を起こす（形式は `docs/adr/0001-*.md` に倣う）。**トリガーは否定の知識が生じたときだけ** — 自明な実装・一本道の選択では作らない。無ければ何もしない
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
