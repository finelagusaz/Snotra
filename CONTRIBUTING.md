# Contributing to Snotra

## ブランチ戦略（GitHub Flow）

- `main` ブランチが常にデプロイ可能な状態を保つ唯一のブランチ
- 全ての変更は `main` からブランチを切り、Pull Request 経由でマージする
- ブランチ名は `feat/xxx`、`fix/xxx`、`docs/xxx` などの形式を推奨
- マージ後のブランチは削除する

## コミットとPR

- コミットメッセージは日本語または英語で、変更の意図が伝わる内容にする
- PR タイトルは "feat: ...", "fix: ...", "docs: ..." などのプレフィックスを付ける
- PR には変更内容・影響範囲・テスト結果を記載する
- レビュー承認後にスカッシュマージまたは通常マージする

## リリースフロー

リリースは `main` ブランチのコミットに対して Git タグを打つことでトリガーされる。

```bash
git tag v0.9.1
git push origin v0.9.1
```

- タグ名は `v<major>.<minor>.<patch>` 形式（例: `v0.9.0`）
- タグを push すると GitHub Actions の Release ワークフローが自動実行される
- ワークフローは Windows 向けバイナリをビルドし、ZIP にまとめて GitHub Release を作成する
- リリースノートは GitHub の自動生成機能を使用する

## CI

- `main` ブランチへの push および PR で CI が自動実行される
- CI はフロントエンドの型チェックとビルドを検証する
- CI が通らない PR はマージしない

## 開発環境のセットアップ

```bash
# 依存関係のインストール
npm ci

# 開発実行（ホットリロード付き）
npm run tauri dev

# テスト
cargo test -p snotra-core   # Rust ユニットテスト
npm test                     # フロントエンドユニットテスト
```

詳細な開発ガイドラインは [CLAUDE.md](./CLAUDE.md) を参照。
