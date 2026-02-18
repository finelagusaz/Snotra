# CLAUDE.md

このファイルは、このリポジトリで Claude Code が作業するときの運用ガイドです。

## このファイルの目的

- 実装時の判断基準と作業ルールを短時間で確認できるようにする
- コード変更時に守るべき原則を明確化する
- `*.md` で意図（何を実現したいか）を管理し、コードで実装事実（どう実装したか）を管理する

## 3層分担（責務分離）

- 第1層（意図管理）: `SPEC.md` と `CLAUDE.md`
  - `SPEC.md`: あるべき仕様、要件、振る舞いの定義
  - `CLAUDE.md`: 実装時の運用ルールと判断基準
- 第2層（実装事実）: `src/*.rs`
  - 現在の実際の動作・制約・実装詳細
- 第3層（整合運用）: 変更時の同期ルール
  - 挙動変更を伴う変更では、意図（`SPEC.md`）と実装（`src/*.rs`）を同時に整合させる

### 不一致時の扱い

- まず「バグ」か「仕様変更」かを判定する
- バグの場合:
  - `SPEC.md` の意図に合わせてコードを修正する
  - 必要に応じて `CLAUDE.md` の運用記述を更新する
- 仕様変更の場合:
  - 先に `SPEC.md` を更新して意図を確定する
  - 次にコードを更新する
  - 最後に `CLAUDE.md` を更新して運用ルールと参照先を整合させる

## プロジェクト概要

Snotra は Windows 専用のキーボードランチャーです。GUI は Rust + egui（`eframe`）で実装し、システムトレイ/グローバルホットキー/IME などの Windows 固有機能は `windows` クレートで実装します。グローバルホットキー（既定: `Alt+Q`）で検索ウィンドウを表示し、検索と起動を行います。

## ビルド・実行コマンド

```bash
cargo build            # デバッグビルド
cargo build --release  # リリースビルド
cargo run              # デバッグ実行
cargo test             # ユニットテスト実行
cargo check            # 静的チェック
cargo clippy           # lint チェック
```

## アーキテクチャ概要

純 Rust 構成で、GUI は egui（`eframe`）を利用し、OS 統合部分のみ Win32 API を直接呼び出します。

### モジュール構成（簡潔版）

- `main.rs`:
  - エントリポイント
  - `eframe` 起動と初期化配線
- `app.rs`:
  - egui 検索UI/設定UIの描画と状態管理
- `platform_win32.rs`:
  - Win32 メッセージループ（ホットキー/トレイ/IME/終了制御）
- `config.rs`:
  - `%APPDATA%\Snotra\config.toml` の読込/保存
  - 既定値補完
  - `Alt+Space` を `Alt+Q` に補正
- `hotkey.rs`:
  - グローバルホットキー登録/解除
- `search.rs`:
  - 検索順位計算（先頭部分一致/中間部分一致/スキップマッチング）
  - 履歴ブースト適用
  - 空クエリ時の履歴候補生成
- `history.rs`:
  - 起動履歴・クエリ別履歴・フォルダ展開履歴の管理
  - バイナリ永続化
- `folder.rs`:
  - フォルダ内列挙とフィルタ/ソート
  - ルート判定・遷移補助
- `indexer.rs`:
  - スキャン対象列挙と重複排除
  - インデックスキャッシュ（設定ハッシュ付き）の読込/再構築
- `icon.rs`:
  - アイコン抽出とキャッシュ永続化
- `launcher.rs`:
  - 選択項目の起動（`ShellExecuteW`）
- `query.rs`:
  - クエリ正規化
- `binfmt.rs`:
  - `magic + version` 付きバイナリ入出力共通処理
- `window_data.rs`:
  - 検索ウィンドウ位置（`window.bin`）の保存/復元

### 実装上の重要パターン

- 検索/設定UI状態は `app.rs` の `SnotraApp` で保持
- 検索ウィンドウは起動時に作成し、ホットキーで表示/非表示を切替
- ホットキーは `RegisterHotKey` を `platform_win32.rs` のメッセージループスレッドで処理
- 履歴ストアは UI 状態として保持し、設定反映時に再ロード
- フォルダ展開は「開始時スナップショットを保持し、`Escape` で一括復帰」モデル
- 履歴/インデックス/アイコン保存は `.tmp` を使った原子的書き込み

### eframe パッチ運用

- `eframe` は crates.io 直参照ではなく `[patch.crates-io]` で `vendor/eframe` を参照する（`Cargo.toml`）。
- 起動時可視化のパッチ箇所は `vendor/eframe/src/native/epi_integration.rs` の `EpiIntegration::post_rendering`。
  - `show_on_startup=false` 起動時の黒ウィンドウ一瞬表示を避けつつ、ホットキー復帰性を保つため、初回描画後の可視化動作を調整している。
- `eframe` / `egui` などフレームワーク更新時は、以下を必ず再確認する。
  - `post_rendering` 実装差分が取り込めるか
  - `show_on_startup=false` で起動時に検索窓が見えないか
  - ホットキーで検索窓が確実に表示されるか

## 実装状況（実装フェーズ）

- [x] Phase 1: 履歴・優先度システム（起動回数、クエリ別重み付け、空クエリ時履歴表示、bincode 永続化）
- [x] Phase 2: フォルダ展開機能（左右キー遷移、フォルダ内フィルタ、`Escape` 復帰、展開回数反映）
- [x] Phase 3: インデックス拡張（`ScanPath` 拡張子指定、フォルダ登録、アイコン抽出/キャッシュ、設定ハッシュ付きキャッシュ）
- [x] Phase 4: 検索方式拡張（先頭部分一致/中間部分一致/スキップマッチング、`config.toml` で方式指定）
- [x] Phase 5: 設定画面（egui 別ウィンドウ、タブ UI、`/o` コマンド）
- [x] Phase 6: ビジュアル・その他（プリセットテーマ、IME 制御、ホットキートグル、タイトルバー切替、ウィンドウ位置記憶、タスクトレイ表示切替）

## 開発原則

### TDD

- 純ロジックモジュール（`search.rs` / `history.rs` / `config.rs` / `folder.rs` / `indexer.rs` / `icon.rs` / `query.rs` / `binfmt.rs`）は `#[cfg(test)]` でユニットテストを追加する
- Win32 依存モジュール（`window.rs` / `hotkey.rs` / `tray.rs` / `launcher.rs` / `main.rs`）はユニットテスト前提にしない
- ロジック追加時は、可能な限りロジック層へ分離してテスト可能性を維持する

### KISS

- `main.rs` に業務ロジックを増やさない
- コールバックは薄く保ち、実処理は専用モジュールへ寄せる
- 責務を跨ぐ実装をしない（例: `window.rs` に検索スコア計算を置かない）

### DRY

- 検索スコア計算は `search.rs` に集約する
- フォルダ列挙・フィルタ・並び替えは `folder.rs` に集約する
- 同一ロジックを `main.rs` や UI 側へ重複実装しない

### YAGNI

- 使う予定だけの抽象化（不要な trait/generics/レイヤー）を導入しない
- 現在の要求範囲を超える機能追加を行わない
- 拡張性より、現要件での単純さと可読性を優先する

## 参照先（3層分担）

- 意図（仕様）: `SPEC.md`
- 運用ルール: `CLAUDE.md`
- 実装事実: `src/*.rs`
- 設定値・デフォルト: `src/config.rs`
- 検索順位ロジック: `src/search.rs`
- 履歴保存仕様: `src/history.rs` と `src/binfmt.rs`
