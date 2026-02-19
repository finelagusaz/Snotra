# snotra-core

純ロジック lib crate（9モジュール）。Win32 非依存でユニットテスト可能。

## モジュール構成

- `config.rs`: `%APPDATA%\Snotra\config.toml` の読込/保存、既定値補完
- `search.rs`: 検索順位計算（先頭/中間/ファジー）、履歴ブースト、空クエリ時履歴候補
- `history.rs`: 起動履歴・クエリ別履歴・フォルダ展開履歴の管理、バイナリ永続化
- `folder.rs`: フォルダ内列挙とフィルタ/ソート、ルート判定
- `indexer.rs`: スキャン対象列挙と重複排除、インデックスキャッシュ
- `query.rs`: クエリ正規化
- `binfmt.rs`: `magic + version` 付きバイナリ入出力共通処理
- `window_data.rs`: ウィンドウ位置（`window.bin`）の保存/復元
- `ui_types.rs`: フロントエンドとの IPC 用データ型

## 開発ルール

- 新規ロジックは可能な限りこの crate に追加してテスト可能性を維持する
- `#[cfg(test)]` でユニットテストを必ず書く
- 検索スコア計算は `search.rs`、フォルダ列挙は `folder.rs` に集約（DRY）
