---
name: start-issue
description: "GitHub issue から作業を開始する: issue 読み込み → main 最新化 → ブランチ作成 → 調査(workspace/research.md) → 実装計画(workspace/plan.md) → セルフレビューして計画更新。実装の前段階をすべて行う。"
disable-model-invocation: true
argument-hint: "<issue-number>"
allowed-tools:
  - Bash(git *)
  - Bash(gh *)
  - Read
  - Write
  - Grep
  - Glob
  - Skill
---

Issue #$ARGUMENTS の作業を開始する。質問せず自律的に進めること。

## Step 1 -- ISSUE 読み込み

```bash
gh issue view $ARGUMENTS
```

issue の内容（タイトル・本文・ラベル・コメント）を把握する。

## Step 2 -- main 最新化 & ブランチ作成

```bash
git checkout main && git pull --ff-only
```

issue の内容から適切なブランチ名を決める:
- 機能追加: `feat/<短い説明>`
- バグ修正: `fix/<短い説明>`
- その他: `chore/<短い説明>`

```bash
git checkout -b <branch-name>
```

## Step 3 -- 調査と分析（workspace/research.md）

`workspace/research.md` と `workspace/plan.md` が既に存在する場合は上書きする（前回の作業成果物）。

`SPEC.md`、関連する `CLAUDE.md`、ソースコードを読み、issue の要求を分析する。

`workspace/research.md` に以下を出力:

- **issue の要約**: 何を解決/実現するか
- **関連コード**: 影響を受けるファイル・モジュール・関数の列挙
- **既存パターン**: 類似の実装が既にあるか、再利用できるか
- **技術的制約**: Win32 依存、IPC 境界、リアクティブ制約など。Win32 API を使う場合、`SendInput`/`SetForegroundWindow`/`ShowWindow` 等の入力・ウィンドウ系 API は部分的に非同期な場合がある。計画時に MSDN で同期性を確認し、技術的制約に記録する
- **未解決の疑問**: 調査で判明しなかった点（あれば）

## Step 4 -- 実装計画（workspace/plan.md）

`workspace/research.md` の分析結果をもとに、`workspace/plan.md` に実装計画を作成する。

計画には以下を含める:

- **変更ファイル一覧**: 各ファイルで何を変更するか
- **実装順序**: 依存関係を考慮したフェーズ分け
- **不変条件**: 各変更が守るべき不変条件。新たな状態フラグ・プロセス・ウィンドウ・リソースを導入する場合は「失敗・異常終了・予期しない順序で呼ばれたときにどうなるか」も含めて記述する
- **テスト方針**: 追加・更新するテストと検証コマンド
- **SPEC.md 更新要否**: 挙動変更を伴う場合は更新内容を記載

CLAUDE.md の開発原則（KISS/DRY/YAGNI）と開発ワークフロー（ステップ 0〜3）に従うこと。

## Step 5 -- セルフレビュー & 計画更新

### 5a. check スキルによる計画検証

計画の内容に応じて該当する check スキルを実行し、計画の見落としを検出する。発見事項があれば計画を更新する。

| スキル | トリガー条件 |
|---|---|
| `/symmetric-check` | 計画が対称ペアを持つコードパスに触れる |
| `/cache-check` | 計画がキャッシュ・インクリメンタル再利用ロジックに触れる |

### 5b. セルフレビューチェックリスト

作成した `workspace/plan.md` を以下の観点でレビューし、問題があれば修正する:

1. **対称コードパス**: 変更対象に対称ペアがあるか確認したか（5a で検証済みなら結果を記録）
2. **影響範囲の網羅性**: 関連する全コードパスを grep で確認したか
3. **境界条件**: エッジケースの検証が計画に含まれているか
4. **リソース管理**: 生成/破棄ペアが計画されているか
5. **既存パターンとの整合**: 新規パターンを導入していないか、既存パターンで対応できないか
6. **YAGNI 違反**: 要求範囲を超える機能追加がないか
7. **シンプル化の挑戦**: 「この設計、本当にこの複雑さが必要か」— 新たな状態（`AtomicBool`・`Mutex`・子プロセス等）・汎用インターフェース・暗黙の前提を導入する箇所について、より単純な代替がないか問い直す。「この操作が失敗したらどうなるか」を設計段階で書けているか
8. **破壊不変条件の明示**: この変更で「壊れたら即アウト」なシステム不変条件を列挙したか。特に Win32 フック・ホットキー・プロセス間通信など「戻ってこない」系のリスクがある場合、検知手段（テスト・スモーク・手動確認手順）とセットで plan.md に記述する

レビュー結果を `workspace/plan.md` 末尾の「セルフレビュー」セクションに記録する。

## Step 6 -- workspace をコミット & プッシュ

セッション断絶・別マシン継続に備え、`workspace/` を必ずコミットしてプッシュする:

```bash
git add workspace/
git commit -m "chore: workspace 調査・計画 (issue #$ARGUMENTS)"
git push -u origin HEAD
```

## Output

最後に以下を報告:

1. issue の要約（1〜2行）
2. 作成したブランチ名
3. 計画の概要（変更ファイル数・フェーズ数）
4. セルフレビューで修正した点（あれば）
5. 次のアクション: `/implement` で実装に進めること
