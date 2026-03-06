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
- ワークフローは `snotra.exe`（`npx tauri build --no-bundle`）と `snotra-settings.exe`（`cargo build --release -p snotra-settings`）を個別にビルドし、両バイナリを ZIP にまとめて GitHub Release を作成する
- リリースノートは GitHub の自動生成機能を使用する

## CI

`main` ブランチへの push および PR で以下の2ジョブが自動実行される。どちらかが失敗する PR はマージしない。同一ブランチで再 push すると、実行中の古い run は自動キャンセルされる。

- **frontend-check** (`ubuntu-latest`): `npm test`（Vitest）+ `npm run build`（型チェック + Vite ビルド）
- **rust-check** (`windows-latest`): `cargo check -p snotra-core -p snotra` + `cargo test -p snotra-core` + `cargo clippy -p snotra-core -p snotra -- -D warnings`

## 開発環境のセットアップ

### 必要環境

- **Windows 10/11**
- **Rust**（stable ツールチェイン）
  - `rustup-init.exe` でインストール後、`rustup default stable` / `rustup target add x86_64-pc-windows-msvc`
  - Visual Studio 2022（または Build Tools）で「Desktop development with C++」ワークロードと Windows SDK を有効にする
- **Node.js** >= 22

### 起動

```bash
npm ci
npm run tauri dev
```

### snotra-settings（設定 GUI）の単独確認

```bash
cargo run -p snotra-settings
```

### テスト

```bash
cargo test -p snotra-core        # Rust ユニットテスト
npm test                          # フロントエンドユニットテスト（Vitest）
npm run smoke:startup             # 起動スモーク
npm run e2e:tauri:setup           # Tauri Driver + E2E 用バイナリ更新
npm run e2e:tauri                 # Playwright + Tauri Driver E2E
```

`npm run e2e:tauri:setup` は `npx tauri build --no-bundle` を含みます。Tauri Driver E2E はこのバイナリを使うため、`cargo build --release` 単体では不十分です。

## アーキテクチャ

```
Snotra/
  Cargo.toml            # ワークスペース（snotra-core, src-tauri, snotra-settings）
  snotra-core/          # 純ロジックライブラリ crate（Win32 非依存・ユニットテスト可能）
  src-tauri/            # Tauri v2 バイナリ crate（Win32 連携・IPC）
  snotra-settings/      # egui 設定 GUI バイナリ（別プロセス）
  ui/                   # SolidJS フロントエンド
    src/
      components/       # SearchWindow, ResultsWindow, ResultRow ほか
      stores/           # リアクティブ状態管理
      lib/              # 型定義, IPC ラッパー, テーマユーティリティ
  .github/workflows/    # CI/CD（ci.yml, release.yml, label-sync.yml）
```

詳細仕様と状態遷移図: [SPEC.md](SPEC.md)
実装ルール・パターン集: [CLAUDE.md](CLAUDE.md)

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

### Tauri Driver E2E で `ERR_CONNECTION_REFUSED` / `search-input not found`

`cargo build --release` 単体では `devUrl` 向きバイナリになります。

```bash
npm run e2e:tauri:setup
```

で E2E 用バイナリを作り直してください。
