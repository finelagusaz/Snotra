# Contributing to Snotra

## ブランチ戦略（GitHub Flow）

- `main` ブランチが常にデプロイ可能な状態を保つ唯一のブランチ
- 全ての変更は `main` からブランチを切り、Pull Request 経由でマージする
- ブランチ名は `feat/xxx`、`fix/xxx`、`docs/xxx` などの形式を推奨
- マージ後のブランチは削除する

## コミットとPR

- コミットメッセージは日本語または英語で、変更の意図が伝わる内容にする
- コードコメントの書き方（rustdoc / TSDoc の様式・粒度）は [docs/comment-guidelines.md](docs/comment-guidelines.md) を参照
- PR タイトルは "feat: ...", "fix: ...", "docs: ..." などのプレフィックスを付ける
- PR には変更内容・影響範囲・テスト結果を記載する
- レビュー承認後にスカッシュマージする（リポジトリ設定で squash のみ有効。運用の詳細はルート `CLAUDE.md`「Git/GitHub 運用」を参照）

## リリースフロー

リリースは `main` ブランチのコミットに対して Git タグを打つことでトリガーされる。

```bash
git tag v0.9.1
git push origin v0.9.1
```

- タグ名は `v<major>.<minor>.<patch>` 形式（例: `v0.9.0`）
- タグを push すると GitHub Actions の Release ワークフローが自動実行される
- ワークフローは `snotra.exe`（`npx tauri build --no-bundle`）と `snotra-settings.exe`（`cargo build --release -p snotra-settings`）を個別にビルドし、両バイナリを ZIP にまとめて GitHub Release を作成する
- リリースノートは GitHub の自動生成機能を使用する

## CI

`main` ブランチへの push および PR で自動実行される。詳細は [docs/build-commands.md](docs/build-commands.md) を参照。

## 開発環境のセットアップ

### 必要環境

- **Windows 10/11**
- **Rust**（stable ツールチェイン）
  - `rustup-init.exe` でインストール後、`rustup default stable` / `rustup target add x86_64-pc-windows-msvc`
  - Visual Studio 2022（または Build Tools）で「Desktop development with C++」ワークロードと Windows SDK を有効にする
- **Node.js** >= 22

### 起動・開発コマンド

開発実行・依存インストール・設定 GUI の単独起動・テスト・E2E など、すべてのビルド／実行コマンドは [docs/build-commands.md](docs/build-commands.md) を SSOT として参照する。

最初のセットアップから開発実行までの最短経路は以下の通り（コマンドの詳細・補足は SSOT を参照）:

1. `npm ci` で依存をインストール（githooks bootstrap・セーフティネット vitest 用）
2. `cargo run -p snotra` で開発実行（egui 既定・#532 SU7 flip 済み）
3. 設定 GUI のみ確認したい場合は `cargo run -p snotra-settings`

## アーキテクチャ

ディレクトリ構成・横断パターン・検索フローは [docs/architecture.md](docs/architecture.md) を参照。

詳細仕様と状態遷移図: [SPEC.md](SPEC.md)

## トラブルシューティング

### `EPERM: operation not permitted, unlink ... esbuild.exe`

`esbuild.exe` が別プロセス（開発サーバ、エディタ拡張、アンチウイルス等）によりロックされています。

```powershell
taskkill /F /IM esbuild.exe
# または
Get-Process node | Stop-Process -Force
```

アンチウイルスが原因の場合はプロジェクトフォルダを除外してください。

### `failed to remove file target\debug\snotra.exe`（アクセス拒否）

前回ビルドの `snotra.exe` が終了していません。

```powershell
taskkill /F /IM snotra.exe
# または
Get-Process -Name snotra | Stop-Process -Force
```

### `linker not found` / MSVC 関連ビルドエラー

Visual Studio の「Desktop development with C++」ワークロードと Windows SDK を確認し、ターミナルを再起動してください。

### 自動回帰 smoke が失敗する

`npm run smoke:egui`（egui show/hide の trace 観測）と `npm run smoke:startup` が自動回帰の最低線です（WebView2 e2e は #532 SU7 で撤去済み）。hotkey は config 依存のため、実 config を持つ環境では `-HotkeyVks` で実際のキーを渡してください（詳細は `docs/build-commands.md` スモーク運用メモ）。
