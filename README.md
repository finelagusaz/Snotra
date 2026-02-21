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
- 矢印キーによるフォルダ内ナビゲーション
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
npm run typecheck
npm run tauri dev
```

CI では `npm run build` 実行時に `prebuild` 経由で型チェックが必ず実行されます。

### リリースビルド

```bash
npm run tauri build
```

### テスト

```bash
cargo test -p snotra-core
```

## アーキテクチャ

```
Snotra/
  snotra-core/          # 純ロジックライブラリ crate
  src-tauri/            # Tauri v2 バイナリ crate（Win32 連携）
  ui/                   # SolidJS フロントエンド
    src/
      components/       # SearchWindow, ResultRow, Settings
      stores/           # リアクティブ状態管理
      lib/              # 型定義, IPC ラッパー, テーマユーティリティ
  .github/workflows/    # CI/CD（リリースパイプライン）
```

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
