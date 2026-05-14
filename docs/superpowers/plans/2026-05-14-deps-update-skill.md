# /deps-update スキル Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** cargo/npm の依存更新を「更新 → ローカル検証 → PR 作成 → CI グリーン確認」まで畳み込む `/deps-update` スキルを追加する（マージは人間が判断）。

**Architecture:** 既存スキル（`/implement`・`/health-check`）と同じ単一 `SKILL.md` 形式・一直線のステップ構成。`docs/build-commands.md` を検証コマンドの SSOT として参照し、コマンド本体は SKILL.md に直書きしない。スキルは `CLAUDE.md` の「利用できるスキル」表に登録して `/health-check` Check 9 の整合性を満たす。

**Tech Stack:** Markdown（スキル定義）、YAML frontmatter、PowerShell（検証コマンド）

**Spec:** `docs/superpowers/specs/2026-05-14-deps-update-skill-design.md`

---

## File Structure

| ファイル | 区分 | 責務 |
|---|---|---|
| `.claude/skills/deps-update/SKILL.md` | 新規作成 | スキル本体。frontmatter（メタデータ・`allowed-tools`）＋ステップ1〜6の手順 |
| `CLAUDE.md` | 変更 | 「利用できるスキル」表に `/deps-update` の行を1行追加 |

検証コマンドの SSOT である `docs/build-commands.md` には触れない（参照のみ）。

### SSOT に関する設計判断

`SKILL.md` の Step 2 には `cargo update` / `npm update`、Step 4〜5 には `gh pr create` / `gh pr checks` という**コマンド名**が現れる。これらは:

- `docs/build-commands.md` が管理する**検証**コマンドではない（更新・PR・CI のアクションそのもの）
- 具体的な引数を持たない**コマンド名への言及**であり、`/health-check` Check 5 が許容する範囲（「コマンド名への言及や参照リンク自体は許容」）

一方、**検証**コマンド（`cargo check -p ...`・`npm run typecheck` 等の具体的な引数を含むもの）は SKILL.md に直書きせず、`docs/build-commands.md` のカテゴリ参照に留める。`/implement` の Step 4 と同じ方針。

---

## Task 1: `SKILL.md` の作成

**Files:**
- Create: `.claude/skills/deps-update/SKILL.md`

- [ ] **Step 1: 検証内容を確認する**

Task 1 完了時に満たすべき条件:
1. `.claude/skills/deps-update/SKILL.md` が存在する
2. frontmatter に `name: deps-update` と `disable-model-invocation: true` がある
3. 検証コマンドの直書き（`cargo check -p`・`npm run typecheck`・`npm run build`・`npm run verify`・`cargo clippy -p` 等の具体的引数つきコマンド）が**ない**

- [ ] **Step 2: ファイルが未作成であることを確認する（Red）**

Run:
```powershell
Test-Path .claude/skills/deps-update/SKILL.md
```
Expected: `False`

- [ ] **Step 3: `SKILL.md` を作成する**

Create `.claude/skills/deps-update/SKILL.md` with exactly this content:

````markdown
---
name: deps-update
description: "依存関係の定期更新: cargo/npm 更新 → ローカル検証 → chore(deps) PR 作成 → CI グリーン確認。マージは手動。Cargo/npm の依存をまとめて更新したいときに使用。"
disable-model-invocation: true
argument-hint: "[cargo | npm | 空=両方]"
allowed-tools:
  - Bash(cargo *)
  - Bash(npm *)
  - Bash(git *)
  - Bash(gh *)
  - Read
  - Edit
  - Grep
  - Glob
---

cargo / npm の依存関係を一括更新し、ローカル検証 → PR 作成 → CI グリーン確認まで行う。マージはしない（人間が判断する）。

対象: $ARGUMENTS（`cargo` / `npm` / 空=両方）

## Step 1 -- 前提確認

- `git status` で作業ツリーが clean か確認する。未コミットの変更がある場合、停止してユーザーに整理を促す
- `git fetch` して `main` を最新化する
- `main` から `chore/deps-YYYYMMDD` ブランチを作成する（`YYYYMMDD` は当日の日付。`CLAUDE.md` の「`main` 直コミット禁止」を遵守）

## Step 2 -- 更新

- `$ARGUMENTS` の対象に応じて依存を更新する:
  - `cargo` または空: `cargo update`
  - `npm` または空: `npm update`
- 更新前後の `Cargo.lock` / `package-lock.json` の差分から、更新されたクレート・パッケージを列挙する
- 各更新を **minor/patch と major に分類**して記録する（major = メジャーバージョン番号が変わったもの）

## Step 3 -- ローカル検証（最大5サイクル）

`docs/build-commands.md` を SSOT として参照し、以下のカテゴリを実行する:

- カテゴリ A: 必須＋追加検証（Rust 全 crate チェック・clippy・core テスト）
- カテゴリ B: 必須（TypeScript 型チェック・フロントエンドビルド）
- カテゴリ C: フロントユニットテスト（Vitest）のみ

E2E・スモークテストはローカルで実行せず CI に委ねる。具体的なコマンド文字列は `docs/build-commands.md` を参照する（二重メンテを避けるためこの SKILL に書かない）。

検証が失敗した場合:

- エラー出力を読み、原因を特定する
- 修正し、失敗したステップから再検証する
- clippy 警告・型エラー・軽い API 追従はこのスキルで直す
- **実ロジックの大幅な書き換えを要する破壊的変更**に当たった場合は、無理に直さず Step 6 の診断サマリーで停止する

5サイクル後もエラーが残る場合、中止して診断サマリーを書く（試みた内容・残存エラー・推定根本原因）。**PR は作らない。調査用にブランチは残す。**

## Step 4 -- コミット & PR

- このスキルで生じた変更（`Cargo.lock`・`package-lock.json`・修復した実コード等）のみをステージする
- `chore(deps): ...` の conventional commit を作成する。何を更新したかの簡潔な要約を含める
- push して `gh pr create` で PR を作成する
- PR 本文に更新一覧を載せる。**major bump は ⚠ で明示する**

## Step 5 -- CI ポーリング

- `gh pr checks` で CI の完了まで待機する

## Step 6 -- 報告

以下を報告する:

1. 更新されたクレート・パッケージ一覧（minor/patch と major を区別）
2. ローカル検証の結果（修復サイクルがあればその内容）
3. PR の URL
4. CI の結果 -- 緑なら「マージ可能」、赤なら失敗したチェックとログ

**いずれの場合もマージはせず停止する。** マージはユーザーが判断する。

検証が5サイクルで通らなかった場合は、PR の代わりに診断サマリー（試みた内容・残存エラー・推定根本原因）を出力する。
````

- [ ] **Step 4: 検証する（Green）**

Run:
```powershell
Test-Path .claude/skills/deps-update/SKILL.md
```
Expected: `True`

Run:
```powershell
Select-String -Path .claude/skills/deps-update/SKILL.md -Pattern '^name: deps-update$' -Quiet
Select-String -Path .claude/skills/deps-update/SKILL.md -Pattern '^disable-model-invocation: true$' -Quiet
```
Expected: 両方とも `True`

Run（SSOT 直書きチェック — 検証コマンドの具体的引数つき記述が無いこと）:
```powershell
Select-String -Path .claude/skills/deps-update/SKILL.md -Pattern 'cargo check -p','cargo clippy -p','cargo test -p','npm run typecheck','npm run build','npm run verify'
```
Expected: 出力なし（マッチ 0 件）

- [ ] **Step 5: コミットする**

```powershell
git add .claude/skills/deps-update/SKILL.md
git commit -m 'chore: /deps-update スキルを追加' -m 'cargo/npm の依存更新を畳み込むスキル。前提確認 → 更新 → ローカル検証（最大5サイクル）→ PR 作成 → CI グリーン確認まで。マージは人間が判断する。' -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

> **Note:** PowerShell の here-string（`@'...'@`）は `&&` 連結や `-m` 直後で解釈が崩れることがある。複数段落のコミットメッセージは段落ごとに `-m` を重ねる（`-m` 間は自動で空行連結される）。

---

## Task 2: `CLAUDE.md` のスキル表に登録

**Files:**
- Modify: `CLAUDE.md`（「## 利用できるスキル」の表）

- [ ] **Step 1: 検証内容を確認する**

Task 2 完了時に満たすべき条件:
1. `CLAUDE.md` の「利用できるスキル」表に `/deps-update` の行がある
2. その行のスキル名が `SKILL.md` の frontmatter `name: deps-update` と一致する（`/health-check` Check 9 の整合性）

- [ ] **Step 2: 表に未登録であることを確認する（Red）**

Run:
```powershell
Select-String -Path CLAUDE.md -Pattern '/deps-update' -Quiet
```
Expected: `False`

- [ ] **Step 3: 表に行を追加する**

`CLAUDE.md` の「## 利用できるスキル」表の最終行（`/implement` の行）の直後に、以下の行を追加する:

```markdown
| `/deps-update`       | cargo/npm の依存を一括更新し PR 作成・CI グリーン確認まで（マージは手動） | `/deps-update` または `/deps-update npm`                                       |
```

変更対象の箇所（`/implement` の行）:
```markdown
| `/implement`         | フルサイクル開発: 調査 → 計画 → 実装 → 検証 → レビュー → コミット      | `/implement キーボードショートカットの追加`                                     |
```
↓ この行の直後に `/deps-update` の行を挿入する。

- [ ] **Step 4: 検証する（Green）**

Run:
```powershell
Select-String -Path CLAUDE.md -Pattern '\| `/deps-update`' -Quiet
```
Expected: `True`

Run（Check 9 整合性 — 表の登録名とスキルファイルの `name` が一致すること）:
```powershell
Select-String -Path .claude/skills/deps-update/SKILL.md -Pattern '^name: deps-update$' -Quiet
```
Expected: `True`

- [ ] **Step 5: コミットする**

```powershell
git add CLAUDE.md
git commit -m 'chore: /deps-update を CLAUDE.md のスキル表に登録' -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Final Verification

実装完了を宣言する前に、以下を確認する（`superpowers:verification-before-completion` に従い、出力を確認してから完了を主張すること）:

- [ ] **スキルファイル ↔ スキル表の整合（`/health-check` Check 9 相当）**

```powershell
Test-Path .claude/skills/deps-update/SKILL.md          # True
Select-String -Path CLAUDE.md -Pattern '\| `/deps-update`' -Quiet   # True
```
両方 `True` であること。スキルファイルと表の双方に存在し、`name: deps-update` で一致している。

- [ ] **SSOT 非迂回（`/health-check` Check 5 相当）**

```powershell
Select-String -Path .claude/skills/deps-update/SKILL.md -Pattern 'cargo check -p','cargo clippy -p','cargo test -p','npm run typecheck','npm run build','npm run verify'
```
出力なし（検証コマンドの具体的引数つき直書きが無い）であること。

- [ ] **ビルド検証は不要**

変更したのは `.md` ファイルのみ（`.rs` / `.ts` / `.tsx` の変更なし）。`docs/build-commands.md` の検証チェックリスト カテゴリ A〜D はいずれも該当しない。

- [ ] **コミット履歴の確認**

```powershell
git log --oneline main..HEAD
```
`chore: /deps-update スキルを追加` と `chore: /deps-update を CLAUDE.md のスキル表に登録` の2コミット（および設計ドキュメントのコミット）が `chore/deps-update-skill` ブランチ上にあること。
