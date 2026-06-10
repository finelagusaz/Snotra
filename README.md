<p align="right">
  <a href="README.en.md">English</a> | 日本語
</p>

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="Snotra icon">
</p>

<h1 align="center">Snotra</h1>

<p align="center">
  <b>Type less, launch more.</b><br>
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

<img width="1056" height="660" alt="Image" src="https://github.com/user-attachments/assets/1840de5f-47c0-4fa7-8523-0b3d975cb01e" />

## 特徴

- **グローバルホットキー**（Alt+Q）でどこからでも即呼び出し
- **3段階検索**：先頭一致・部分一致・ファジーマッチ
- **履歴ベースのスマートランキング**：よく使うアプリが上位に
- **フォルダナビゲーション**：右キーで展開、左キーで遡り
- **スラッシュコマンド**：`/o` 設定・`/r` 履歴・`/s` 再構築・`/q` 終了
- **インスタントコマンド**：`@` プレフィックスでユーザー定義コマンドを即実行（変数展開対応）
- **カスタムオープナー**：ファイルを任意のツールで開くルールを設定
- アイコン表示、テーマカスタマイズ、IME 自動制御、システムトレイ常駐

## インストール

1. [Releases](https://github.com/finelagusaz/Snotra/releases/latest) から最新の `Snotra-vX.X.X.zip` をダウンロード
2. ZIP を任意のフォルダに展開
3. `snotra.exe` を実行

## 基本的な使い方

| 操作 | 動作 |
|------|------|
| `Alt+Q` | 検索ウィンドウを表示 |
| 文字を入力 | インデックスを検索 |
| `↑` / `↓` | 候補を選択 |
| `Enter` | 選択したアプリ・ファイルを起動 |
| `→`（フォルダ選択中） | フォルダを展開 |
| `←` | 親フォルダへ戻る |
| `Shift+Enter` | カスタムオープナーで開くツールを選択 |
| `Escape` | 検索ウィンドウを閉じる |
| `/o` | 設定を開く |
| `/r` | 直近の起動履歴を表示 |
| `/s` | インデックスを再構築 |
| `/q` | アプリを終了 |

### インスタントコマンド

`@` に続けてコマンド名を入力すると、設定済みのコマンドを即座に実行できます。

| 入力例 | 動作 |
|--------|------|
| `@google SolidJS` | `{query}` を展開して URL を開く |
| `@clip` | クリップボードの内容（`{clip}`）を使ってコマンドを実行 |

コマンドの追加・編集は `/o` の設定画面から行えます。

### パスの一部で検索

`tool\editor` や `workspace\myproject` のようにパスの一部を入力すると、パスが一致するアプリやファイルを検索できます。フォルダが見つかったら `→` キーで中身を展開できます。

## ライセンス

このプロジェクトは [MIT License](LICENSE) の下で公開されています。

---

開発・コントリビュート: [CONTRIBUTING.md](CONTRIBUTING.md)
