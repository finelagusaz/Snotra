---
name: health-check
description: "コードベースとドキュメントの衛生チェック: CLAUDE.md モジュール構成・architecture.md モジュール一覧・AGENTS.md 参照・SPEC.md 番号・build-commands.md コマンド・MEMORY.md 参照・rules パスパターンの整合性を検証する。大きなサイクル完了後や定期的に使用。"
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

## Check 2 -- docs/architecture.md モジュール一覧の乖離

`docs/architecture.md` の各クレート・パッケージのモジュール一覧表に記載されたファイル名が、実際のファイルと一致しているか検証する。

対象テーブル:
- 「snotra-core（純ロジック層）」の表 ↔ `snotra-core/src/*.rs`
- 「src-tauri」トップレベル + commands/ + platform/ の各表 ↔ `src-tauri/src/**/*.rs`
- 「ui」の components/ + stores/ + lib/ の各表 ↔ `ui/src/**/*.{ts,tsx}`（テストファイル除外）
- 「snotra-settings」トップレベル + tabs/ の各表 ↔ `snotra-settings/src/**/*.rs`

手順:
1. `docs/architecture.md` を読み、各表からモジュール名（ファイル名）を抽出する
2. Glob で実際のファイルを列挙する
3. 差分を報告:
   - **記載あり・実ファイルなし**: 削除されたのに architecture.md が未更新
   - **実ファイルあり・記載なし**: 追加されたのに architecture.md が未更新

## Check 3 -- AGENTS.md ドキュメント参照・コマンドの整合性

### 3a. ドキュメント参照の実在性
`AGENTS.md` の「ドキュメント参照」セクションに記載されたファイルパスが実在するか検証する。
存在しないファイルへの参照を報告する。

### 3b. 検証コマンドの整合性
`AGENTS.md` の「変更後の検証」（ステップ 8）に記載された npm コマンド（`npm run XXX`）が `package.json` の `scripts` に定義されているか検証する。
定義されていないコマンドを報告する。

## Check 4 -- SPEC.md セクション番号の連続性

`SPEC.md` を読み、`## N.` と `### N.x` の番号が連続しているか確認する。
飛び・重複・親子の不整合があれば報告する。

## Check 5 -- docs/build-commands.md コマンドの実在性

`docs/build-commands.md` に記載された `npm run XXX` / `npm XXX` コマンドが `package.json` の `scripts` に定義されているか検証する。
`cargo` コマンドは `Cargo.toml` のワークスペースメンバーと照合する（`-p <crate>` のクレート名が存在するか）。
定義されていないコマンドを報告する。

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
