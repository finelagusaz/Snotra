<p align="right">
  <a href="README.en.md">English</a> | 日本語
</p>

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Snotra icon">
</p>

<h1 align="center">Snotra</h1>

<p align="center">
  <i>Windows 専用の高速キーボードランチャー</i>
</p>

<p align="center">
  <a href="https://github.com/finelagusaz/Snotra/actions/workflows/release.yml"><img src="https://github.com/finelagusaz/Snotra/actions/workflows/release.yml/badge.svg" alt="Build"></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078D4?logo=windows" alt="Platform">
  <img src="https://img.shields.io/badge/Rust-2024_edition-DEA584?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri&logoColor=white" alt="Tauri">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
</p>

---

## 特徴

- グローバルホットキー（Alt+Q）で即座に起動
- 先頭一致・部分一致・ファジーマッチの3段階検索
- 履歴ベースのスマートランキング
- 矢印キーによるフォルダ展開・ナビゲーション（右で展開、左で遡り）
- スラッシュコマンド（`/o` 設定・`/r` 履歴・`/s` 再構築・`/q` 終了など）
- アイコン表示（オンデマンド抽出、設定で切替可能）
- CSS カスタムプロパティベースのテーマシステム
- IME 自動制御
- システムトレイ常駐

## はじめに

### 必要環境

- **Windows 10/11**
- **Rust**（stable ツールチェイン）
- **Node.js** >= 22

### 開発

```bash
npm install
npm run tauri dev
```

型チェックを手動で行う場合は `npm run typecheck` を利用できます。CI では `npm run build` 実行時に `prebuild` 経由で型チェックが必ず実行されます。

### リリースビルド

```bash
npm run tauri build
```

### テスト

```bash
cargo test -p snotra-core
npm test
npm run smoke:startup
# Playwright runner + Tauri Driver
npm run e2e:tauri:setup
npm run e2e:tauri
```

`npm run e2e:tauri:setup` は `tauri-driver` のインストールに加えて `npx tauri build --no-bundle` で E2E 用の Tauri バイナリを更新します。Tauri Driver E2E はこのバイナリを使うため、`cargo build --release` 単体では `localhost` 向きバイナリになり `ERR_CONNECTION_REFUSED` で失敗します。

## アーキテクチャ

```
Snotra/
  Cargo.toml            # ワークスペース（snotra-core, src-tauri）
  snotra-core/          # 純ロジックライブラリ crate
  src-tauri/            # Tauri v2 バイナリ crate（Win32 連携）
  ui/                   # SolidJS フロントエンド
    src/
      components/       # SearchWindow, ResultRow, Settings
      stores/           # リアクティブ状態管理
      lib/              # 型定義, IPC ラッパー, テーマユーティリティ
  .github/workflows/    # CI/CD（リリースパイプライン）
```

- 詳細仕様と状態遷移図: [SPEC.md](SPEC.md)

## Codex 自動化

Issue 駆動で Codex 実装〜Draft PR 作成まで自動化する運用を用意しています。  
設定方法と運用ルールは [.github/codex-automation.md](.github/codex-automation.md) を参照してください。

## 技術スタック

<p>
  <img src="https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Tauri_v2-24C8D8?logo=tauri&logoColor=white" alt="Tauri">
  <img src="https://img.shields.io/badge/SolidJS-2C4F7C?logo=solid&logoColor=white" alt="SolidJS">
  <img src="https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white" alt="TypeScript">
  <img src="https://img.shields.io/badge/Vite-646CFF?logo=vite&logoColor=white" alt="Vite">
</p>

## ライセンス

このプロジェクトは [MIT License](LICENSE) の下で公開されています。

## 環境構築手順（Windows）

- **前提ソフトウェア**: Visual Studio 2022（または Build Tools）で「Desktop development with C++」ワークロードとWindows SDKを有効にしてください。`git`, `rustup`, `node`/`npm` が PATH にあることを確認してください。
- **Rust**: rustup を使って stable toolchain をインストールし、MSVC ターゲットを追加します。
  - コマンド例:
    - `rustup-init.exe` を実行してインストール
    - `rustup default stable`
    - `rustup target add x86_64-pc-windows-msvc`
- **Node.js / npm**: Node.js LTS（README の要件では >=22）をインストールしてください。`node -v` / `npm -v` で確認します。
- **依存インストール**: プロジェクトルートで依存をインストールします。
  - `npm ci`（または `npm install`）
  - フロントエンドで個別に行う場合: `cd ui && npm ci`
- **Tauri CLI**: 必要に応じてインストールします（グローバルでも可）。
  - `npm install -g @tauri-apps/cli` または `cargo install tauri-cli`
- **開発起動（典型）**:
  - フロントエンド（手動）: `cd ui && npm run dev`
  - ルートで Tauri 開発実行: `npm run tauri dev`

### トラブルシューティング（よくある問題と対策）

- **`EPERM: operation not permitted, unlink ... esbuild.exe`**
  - 原因: `esbuild.exe` が別プロセス（開発サーバ、エディタ拡張、アンチウイルス等）によりロックされています。
  - 対処:
    - すべての開発サーバ / ターミナル / エディタのターミナルを閉じる。
    - `tasklist | findstr /I "esbuild node"` でプロセスを確認し、`taskkill /F /IM esbuild.exe` や `Get-Process node | Stop-Process -Force` で停止する。
    - それでも残る場合は Sysinternals の Process Explorer（Ctrl+F 検索）や `handle.exe` でハンドルを特定し閉じる。
    - アンチウイルスが原因ならプロジェクトフォルダを除外する。

- **`failed to remove file target\\debug\\snotra.exe` (os error 5 / アクセスが拒否されました)**
  - 原因: 前回ビルド実行中の `snotra.exe` が終了しておらずファイル削除が失敗。
  - 対処:
    - 実行中プロセスを確認: `Get-Process -Name snotra -ErrorAction SilentlyContinue` / `tasklist | findstr /I snotra`
    - プロセスを終了: `taskkill /F /IM snotra.exe` または `Get-Process -Name snotra | Stop-Process -Force`。
    - ハンドルが残る場合は Process Explorer/handle.exe でハンドルを閉じる。
    - ファイル削除/キャッシュ掃除: `Remove-Item .\\target\\debug\\snotra.exe -Force` / `cargo clean`。
    - 必要なら管理者としてターミナルを再起動して実行する。

- **`linker not found` / MSVC 関連のビルドエラー**
  - 対処: Visual Studio の「Desktop development with C++」ワークロードと Windows SDK を確実にインストールし、ターミナルを再起動してから再ビルドしてください。

- **`tauri` CLI が見つからない/コマンドが失敗する**
  - 対処: `npm install -g @tauri-apps/cli` または `cargo install tauri-cli` を行う。プロジェクトではローカル devDependency として管理されている場合もあるので `npm run tauri dev` を利用してください。

- **Tauri Driver E2E で `ERR_CONNECTION_REFUSED` / `search-input not found`**
  - 原因: E2E が参照する `target\\release\\snotra.exe` が古い、または `cargo build --release` だけで作られた `devUrl` 向きバイナリになっている。
  - 対処: `npm run e2e:tauri:setup` または `npx tauri build --no-bundle` を実行して、E2E 用バイナリを作り直す。

### 短いチェックリスト

- **環境確認**: `node -v`, `npm -v`, `rustc --version`, `cargo --version`, `git --version`
- **依存インストール**: `npm ci`（必要に応じて `cd ui && npm ci`）
- **起動**: `npm run tauri dev`
