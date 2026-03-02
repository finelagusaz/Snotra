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

## 実装前チェック（必須）

- 共通原則はルート `CLAUDE.md` の「レビュー未然防止の事前調査（必須）」に従う
- `search.rs` で `Ord` / `Reverse` / `BinaryHeap` を扱う変更では、`BinaryHeap` の先頭が最良/最悪のどちらかを実装前に明記する
- `search.rs` の top-k 更新ロジックを変更する場合は、入力順を変えても結果が不変であるテストを追加または更新する
- `SearchEngine` に新しい並列 Vec フィールドを追加するとき: `EntryView` 構造体・`entry_view()` メソッド・`new()` 末尾の `debug_assert!` を同時に更新し、全 Vec 長の同期を保つ（書き込み側 `new()` と読み取り側 `entry_view()` は常にペアで更新する）
- `query.rs` の正規化を変更する場合は、タブ・全角スペース・NBSP を `' '` に統一するテストと冪等性テストを追加または更新する

## データ永続化の注意

- シリアライザを切り替える場合は**必ずバージョン番号をバンプ**し、旧形式のフォールバックデシリアライザを追加する。切り替え前後でバイト列の互換性はほぼ存在しない（例: bincode の u32 は 4バイト LE、postcard は LEB128 varint）
- `deserialize_failed → save()` パターン（デコード失敗時に空データを即時上書き保存）は HistoryStore など学習データを持つモジュールでデータ喪失を招く。フォールバック読み込みを先に試み、次回の通常 save() で新形式に昇格させること

## history.rs のキー正規化に関するチェックリスト

`history.rs` のパスキー形式（`normalize_entry_key` の適用有無など）を変更したとき、以下の3者が揃っているか確認する:

1. **新規記録** (`record_launch` / `record_folder_expansion`): 書き込み時に正規化しているか
2. **既存データ移行** (`load()` 内 `migrate_normalize_keys`): デシリアライズ直後に全キーを正規化しているか
3. **外部向け参照 API** (`get_global_stats` / `query_count_normalized` 等): 参照時に正規化を内部で完結させているか

「新規記録だけ直した」「参照だけ直した」は必ず互換性バグを残す。3者同時に揃える。

## 内部キー形式の知識を漏洩させない

raw なデータ構造（`FxHashMap<String, u32>` など）を返す pub API は、呼び出し側に「キーの形式（正規化済みか否か）」の知識を強制する。同等の encapsulated API が存在する場合は必ずそちらを使う。存在しない場合は作る（DRY）。

- 悪例: `get_query_stats(&norm_query)` → 呼び出し側が `m.get(&entry.target_path)` と元ケースで引く
- 良例: `query_count_normalized(&norm_query, &entry.target_path)` → 正規化を内部で完結
