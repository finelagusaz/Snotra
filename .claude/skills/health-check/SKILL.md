---
name: health-check
description: "コードベースとドキュメントの衛生チェック: CLAUDE.md モジュール構成・architecture.md モジュール表の非再導入・AGENTS.md 参照・SPEC.md 番号・build-commands.md コマンド・build-commands↔workflow 対応・MEMORY.md 参照・rules パスパターンの整合性を検証する。大きなサイクル完了後や定期的に使用。"
disable-model-invocation: true
argument-hint: ""
allowed-tools:
  - Bash(git *)
  - Read
  - Grep
  - Glob
  - Agent
---

コードベースとドキュメントの衛生状態を検証する。発見事項を報告し、修正は行わない。

## Check 1 -- CLAUDE.md モジュール構成の乖離

各サブディレクトリの `CLAUDE.md` に記載されたモジュール構成が、実際のファイル一覧と一致しているか検証する。

対象:
- `snotra-core/CLAUDE.md` ↔ `snotra-core/src/*.rs`
- `src-tauri/CLAUDE.md` ↔ `src-tauri/src/**/*.rs`
- `ui/CLAUDE.md` ↔ `ui/src/**/*.{ts,tsx}`（エントリポイント・components・stores・lib セクション）
- `snotra-settings/CLAUDE.md` ↔ `snotra-settings/src/**/*.rs`

手順:
1. 各 `CLAUDE.md` を読み、記載されたファイル名を抽出する
2. Glob で実際のファイルを列挙する
3. 差分を報告:
   - **記載あり・実ファイルなし**: 削除されたのに CLAUDE.md が未更新
   - **実ファイルあり・記載なし**: 追加されたのに CLAUDE.md が未更新

## Check 2 -- docs/architecture.md にファイル単位モジュール表が再導入されていないか

`docs/architecture.md` はファイル単位のモジュール一覧を**持たない**設計。モジュール構成（各ファイルとその責務）の SSOT は各サブディレクトリの `CLAUDE.md` であり、architecture.md は位置づけ・主要な型・横断パターン・データフローのみを記す。二重管理がドリフトの温床になるため、ファイル単位のモジュール表は撤去された経緯がある。

このチェックは、その不変条件の回帰——architecture.md にファイル単位のモジュール表が再導入されていないか——を検証する。

手順:
1. `docs/architecture.md` を読む
2. 先頭セルがバッククォート付きファイル名（`*.rs` / `*.ts` / `*.tsx` 等）の Markdown 表行が存在するか検査する（例: `| \`engine.rs\` | ... |`）
3. 該当行が見つかった場合は **Warning** で報告:
   - architecture.md にファイル単位のモジュール表が再導入されている。モジュール構成は各サブディレクトリの `CLAUDE.md` に一本化する設計のため、表の内容は該当 `CLAUDE.md` 側へ集約し、architecture.md からは撤去する

## Check 3 -- AGENTS.md ドキュメント参照の実在性

`AGENTS.md` の「ドキュメント参照」セクションに記載されたファイルパスが実在するか検証する。
存在しないファイルへの参照を報告する。

> **Note:** 検証コマンドの整合性は SSOT である `docs/build-commands.md` を対象とする Check 5 で一括検証する（AGENTS.md は Step 8 で `docs/build-commands.md` を参照する形式に統合済み）。

## Check 4 -- SPEC.md セクション番号の連続性

`SPEC.md` を読み、`## N.` と `### N.x` の番号が連続しているか確認する。
飛び・重複・親子の不整合があれば報告する。

## Check 5 -- docs/build-commands.md コマンドの実在性（SSOT）

`docs/build-commands.md` はビルド／検証コマンドの単一の真実源（SSOT）。記載された `npm run XXX` / `npm XXX` コマンドが `package.json` の `scripts` に定義されているか検証する。
`cargo` コマンドは `Cargo.toml` のワークスペースメンバーと照合する（`-p <crate>` のクレート名が存在するか）。
定義されていないコマンドを報告する。

加えて、SSOT ドリフトの検知として以下も確認する:
- `AGENTS.md` Step 8 や `.claude/skills/*/SKILL.md` に **コマンド本体**（`cargo XXX` / `npm XXX` / `npx XXX` の具体的な引数を含む実行コマンド）が直書きされていないか grep する。`docs/build-commands.md` の SSOT を迂回している箇所を報告する（コマンド名への言及や参照リンク自体は許容）。

## Check 6 -- docs/development-principles.md 参照の実在性

`docs/development-principles.md` に記載されたファイルパス参照（バッククォート内の `*.md`, `*.rs` 等）が実在するか検証する。
存在しないファイルへの参照を報告する。

## Check 7 -- MEMORY.md 参照の実在性

`MEMORY.md` を読み、リンク先のメモリファイルが実在するか確認する。
存在しないファイルへの参照を報告する。

各メモリファイルの `description` が内容と合っているか簡易チェックする（ファイルを読んで description と内容を比較）。

## Check 8 -- .claude/rules/ パスパターンの有効性

`.claude/rules/` 内の各ルールファイルを読み、`paths:` フロントマターに記載されたパスパターンが実在するファイルにマッチするか Glob で検証する。
マッチ 0 件のパターンを報告する。

## Check 9 -- スキル定義の整合性

`.claude/skills/*/SKILL.md` の一覧と、`CLAUDE.md` の「利用できるスキル」テーブルを比較する。
- スキルファイルはあるがテーブルに記載なし
- テーブルに記載があるがスキルファイルなし

## Check 10 -- docs/build-commands.md ↔ .github/workflows/\* の対応

`docs/build-commands.md`「変更後の検証チェックリスト」の**必須コマンド**が、いずれかの GitHub Actions workflow で実際に実行されるか検証する。CI で担保されない検証要件のドリフトを検知する。

手順:
1. `docs/build-commands.md` のカテゴリ A〜C から「必須」とマークされた `npm` / `cargo` コマンド名を抽出する（特にカテゴリ C の `smoke:startup` / `e2e:tauri`）。
2. `.github/workflows/*.yml` を grep し、各必須コマンドを実行する `run:` ステップが存在するか確認する。npm script が薄いラッパーの場合（例: `smoke:startup` = `pwsh -File scripts/smoke-startup.ps1`）、その**ラッパーが呼ぶスクリプトパス**（`scripts/smoke-startup.ps1`）が `run:` に現れていれば「実行あり」とみなす（CI では引数を上書きして直接呼ぶことがあるため）。
3. 「CI/CD メモ」の対応表（検証コマンド ↔ workflow）が実際の workflow 定義と一致するか照合する:
   - 表に載っているが実行する workflow が無いコマンド
   - workflow で実行されているが表に無いコマンド
   - 表の workflow 名・トリガー記述が実ファイル（`name:` / `on:`）とずれている
4. 対応 workflow が無い必須コマンド、または対応表とのずれを **Warning** で報告する（例: 「`smoke:startup` は必須だが、どの workflow にも対応する run ステップが無い」）。

> **Note:** 必須コマンドが PR で自動実行されるか、`e2e` ラベル等の条件付きかは問わない（条件付き実行は許容）。検知対象は「実行する workflow が存在しない」「対応表が実態とずれている」ことのみ。

## 出力

```markdown
# Health Check Report

## 発見事項

### [Critical / Warning / Info] <カテゴリ>
- <具体的な乖離の説明>

## サマリー
- チェック項目数: N
- 発見事項: N件（Critical: N / Warning: N / Info: N）
```

発見事項がゼロの場合は「All checks passed.」と報告する。
