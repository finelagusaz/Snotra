# Retrospective — Claude Code セットアップ改善・スキルとエージェントの整理

## よかったこと

### insights 駆動で優先度付きの改善ができた

`/insights` レポートの摩擦分析から「main 直プッシュ」「計画書の勝手な簡略化」「初回ビルド失敗」を特定し、CLAUDE.md ルールと PostToolUse フックで対策した。根拠のある変更なので過剰追記にならず済んだ。

### スキル・エージェントの整理を体系的に実施できた

重複の特定（debugger 削除、tdd→implement 統合）と未使用原因の分析（`disable-model-invocation` が忘却を引き起こす）を行い、ワークフロー全体を `/start-issue` → `/implement` → `/retrospective` の3スキル連携に整理できた。

### 和訳の一括統一でプロジェクト全体の一貫性が向上した

エージェント2件（code-reviewer, code-optimizer-reviewer）+ スキル4件（implement, symmetric-check, dry-check, start-issue）を日本語化し、description のトリガー条件も日本語で記述した。

---

## 伸びしろ

### MCP 設定のクロスプラットフォーム調査が後手に回った

プロジェクト `.mcp.json` → ローカル設定 → グローバル設定と3回やり直した。Windows の `cmd /c` ラッパーが必要な点と、`.mcp.json` にクロスプラットフォーム切替機能がない点を最初に確認していれば1回で済んだ。

### CLI バグ（`claude mcp add` の `/c` → `C:/` 変換）への対処

既知の issue (#20061) だが、事前に知らなかったため Python スクリプトによる手動 JSON 編集が必要になった。MCP 設定変更時は CLI を信用せず `~/.claude.json` を直接確認する運用が安全。

---

## ネクストアクション

- [ ] 次回の実装サイクルで `/start-issue` → `/implement` → `/retrospective` の連携を実際に通してワークフローを検証する
- [ ] symmetric-check / dry-check の自動トリガーが実際に発火するか、日本語 description での精度を観察する
- [ ] 別マシン（macOS）で context7 MCP のグローバル設定を行い、セットアップ手順を chezmoi 管理外のドキュメントに記録する
