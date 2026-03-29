---
name: health-check
description: "コードベースとドキュメントの衛生チェック: CLAUDE.md のモジュール構成・SPEC.md セクション番号・MEMORY.md 参照・rules パスパターンの整合性を検証する。大きなサイクル完了後や定期的に使用。"
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

## Check 2 -- SPEC.md セクション番号の連続性

`SPEC.md` を読み、`## N.` と `### N.x` の番号が連続しているか確認する。
飛び・重複・親子の不整合があれば報告する。

## Check 3 -- MEMORY.md 参照の実在性

`MEMORY.md` を読み、リンク先のメモリファイルが実在するか確認する。
存在しないファイルへの参照を報告する。

各メモリファイルの `description` が内容と合っているか簡易チェックする（ファイルを読んで description と内容を比較）。

## Check 4 -- .claude/rules/ パスパターンの有効性

`.claude/rules/` 内の各ルールファイルを読み、`paths:` フロントマターに記載されたパスパターンが実在するファイルにマッチするか Glob で検証する。
マッチ 0 件のパターンを報告する。

## Check 5 -- スキル定義の整合性

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
