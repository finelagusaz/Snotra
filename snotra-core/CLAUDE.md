# snotra-core

純ロジック lib crate（12モジュール + `lib.rs`）。Win32 非依存でユニットテスト可能。

## モジュール構成

- `engine.rs`: `Engine` struct（`SearchEngine` + `HistoryStore` + `Config` の facade）。`FolderListContext`（ロック外スナップショット）と `PrebuiltIndex`（インデックス高速スワップ用）を公開
- `config.rs`: `%APPDATA%\Snotra\config.toml` の読込/保存、既定値補完。`Language` enum（`Ja`/`En`）と `default_language()`（`sys-locale` による OS 言語自動判定、非日本語は英語フォールバック）を定義
- `search.rs`: 検索順位計算（Prefix/Substring/Kana/Fuzzy）、履歴ブースト、incremental search キャッシュ、空クエリ時履歴候補。`SearchEngine` は並列 Vec レイアウト（`entries` / `lower_names` / `lower_file_names` / `normalized_keys` / `char_masks` / `file_name_char_masks` / `kana_lower_names`）で cache locality を確保。`new()` は Wave 1（文字列正規化）→ Wave 2（ビットマスク計算）の 2 段並列構築
- `history.rs`: 起動履歴・クエリ別履歴・フォルダ展開履歴の管理、バイナリ永続化
- `folder.rs`: フォルダ内列挙とフィルタ/ソート、ルート判定
- `indexer.rs`: スキャン対象列挙と重複排除、インデックスキャッシュ
- `query.rs`: クエリ正規化
- `binfmt.rs`: `magic + version` 付きバイナリ入出力共通処理
- `error.rs`: `BinError`（バイナリシリアライズ/デシリアライズ失敗）と `ConfigError`（設定バリデーション失敗）の error 型定義
- `window_data.rs`: ウィンドウ位置（`window.bin`）の保存/復元
- `instant.rs`: インスタントコマンドの変数展開（`expand_instant_command`）と前方一致フィルタ（`filter_instant_commands`）
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
- `search.rs` の incremental search キャッシュ（`prev_*` フィールド群）に新しい述語を追加するとき: `use_incremental` の条件式と `prev_*` の更新箇所を同時に変更し、`/cache-check` で単調性を検証する
- `query.rs` の正規化を変更する場合は、タブ・全角スペース・NBSP を `' '` に統一するテストと冪等性テストを追加または更新する
- `folder.rs` のソート順変更時: ソート順は「`is_folder` 降順 → `exp_count` 降順 → `lower_name` 昇順」で、先頭要素が最良（最優先）。`select_nth_unstable_by`（O(N) 平均の top-k 選択）＋ `sort_by`（安定ソートで確定順）の2段階を崩さない。入力順に依存しないことを確認するテスト（`score_entries_top_k_order_independent_of_input_order`）を通す

## クロスモジュール不変条件

### `normalize_entry_key` の冪等性契約

`indexer::normalize_entry_key` は「小文字化 + `/` → `\\`」の正規化関数。**2回適用しても1回と同じ結果になる（冪等）** ことが設計契約であり、`migrate_normalize_keys_is_idempotent` テストで保証されている。以下の3モジュールが依存する:

- `indexer.rs`: スキャン時の重複排除キー、IndexCache の `normalized_keys`
- `history.rs`: 全記録・参照・マイグレーションのキー正規化
- `search.rs`: `SearchEngine.normalized_keys` ベクタ（履歴照合用）

この関数の正規化ルールを変更する場合は、3モジュール全てへの影響と冪等性テストを確認する。

### `char_bitmask` の二重定義

`search.rs::char_bitmask()` と `indexer.rs::char_bitmask_for_cache()` は同一ロジック（bits 0-25 = a-z, bits 26-35 = 0-9）。IndexCache に保存したマスクを SearchEngine がそのまま使うため、**一方を変更したら他方も必ず同時に変更する**。不一致があるとキャッシュヒット時のみ検索結果が狂う（再現困難なバグ）。

### `query.rs` の正規化と `Cow<str>` 遅延アロケーション

`normalize_query()` は `Cow<str>` を返す。ASCII 小文字のみのクエリでは借用（ゼロアロケーション）、大文字・アクセント・連続空白がある場合のみ所有に切り替わる。正規化ロジックを変更するとき、この2パスの一貫性（チェック条件とビルド条件が同じ入力に対して同じ結果を生む）を壊さない。

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

## IndexCache バージョン変更チェックリスト

`indexer.rs` の `IndexCache` にフィールドを追加する場合、以下を全て更新する:

1. **IndexCache 構造体**: 新フィールドを追加
2. **バージョン番号**: `INDEX_CACHE_VERSION` をバンプ
3. **旧バージョン用フォールバック構造体**: 旧スキーマを `IndexCacheVN` として残す
4. **`load_cache()`**: 新バージョン → 旧バージョンのフォールバックチェーンを追加
5. **`save_cache_sorted()`**: 新フィールドの計算ロジックを追加
6. **`CachedMasks` 構造体**: 新フィールドを `Option<T>` で追加（旧キャッシュでは None）
7. **`SearchEngine::new_with_cached_masks()`**: 新パラメータを受け取り、None 時は自前で計算

1つでも欠けるとキャッシュヒット時/ミス時で異なる結果を返す。

## engine.rs のロック最小化パターン

`Engine` は Tauri 側で `Mutex<Engine>` に包まれる。ロック保持時間を最小化するための2つのパターンがある:

- **`FolderListContext`**: ロック内で `capture_folder_list_context()` してスナップショットを取得 → ロック外で I/O（`read_dir_entries`）→ ロック内で `finalize_folder_list()` でスコアリング。設定変更との微小な不整合は許容する設計判断
- **`PrebuiltIndex`**: ロック外で `PrebuiltIndex::new(entries)` を構築 → ロック内で `apply_prebuilt_index()` でスワップ。SearchEngine の構築コスト（Wave 1/2 の並列計算）をロック外に追い出す

新しい Engine メソッドを追加するとき、I/O やインデックス構築をロック内で行わないよう注意する。

## indexer.rs の背景再スキャン

`spawn_background_rescan` はキャッシュヒット時に低優先度スレッドでファイルシステムを再スキャンし、キャッシュの新鮮さを保つ。エントリが変わった場合は `save_cache_sorted` + `invalidate_icon_cache` を実行する。スキャンロジック（`scan_all` / `sort_entries_canonical` / `entries_equal`）を変更するとき、このバックグラウンドパスにも影響することを意識する。

## エントリ名の導出ルール

`indexer.rs` のスキャンでは:
- **ファイル**: `file_stem()` を `name` に使用（拡張子なし）。例: `firefox.lnk` → `name: "firefox"`
- **フォルダ**: `file_name()` を `name` に使用（そのまま）。例: `Projects/` → `name: "Projects"`

`folder.rs` のフォルダ内列挙では `file_name()` をそのまま使う（拡張子付き）。この違いは意図的で、フォルダ展開時にはファイル拡張子がフィルタリングの手がかりになるため。
