# snotra-core

純ロジック lib crate（12モジュール + `lib.rs`）。Win32 非依存でユニットテスト可能。

## モジュール構成

- `engine.rs`: `Engine` struct（`SearchEngine` + `HistoryStore` + `Config` の facade）。`FolderListContext`（ロック外スナップショット）と `PrebuiltIndex`（インデックス高速スワップ用）を公開
- `config.rs`: `%APPDATA%\Snotra\config.toml` の読込/保存、既定値補完。`Language` enum（`Ja`/`En`）と `default_language()`（`sys-locale` による OS 言語自動判定、非日本語は英語フォールバック）を定義
- `search.rs`: 検索順位計算（Prefix/Substring/Kana/Fuzzy/Path）、履歴ブースト、incremental search キャッシュ、空クエリ時履歴候補。`SearchEngine` は並列 Vec レイアウト（`entries` / `lower_names` / `lower_file_names` / `normalized_keys` / `char_masks` / `file_name_char_masks` / `kana_lower_names`）で cache locality を確保。構築は `compute_wave1`（文字列正規化）→ `compute_wave2`（ビットマスク計算）のヘルパー関数で共通化し、`new()`（= `new_with_migemo(.., true)`）/ `new_with_migemo(entries, migemo_enabled)` / `new_with_cached_masks(.., migemo_enabled)` が共有する。**`kana_lower_names` は `migemo_enabled` が true のときのみ構築し、無効時は空 Vec**（migemo 無効ユーザーの死蔵メモリ ~2.1–2.7MB/50k を削る・構築も約 2 倍速、issue #337）。空 Vec のとき検索ループは `kana_available` 空ガードで `kana_lower_names[i]` アクセスを回避する（構築時 migemo OFF→検索時 ON の窓での panic 防止）。migemo トグルの反映は `update_config` が engine を再構築しないため、`config_watcher::needs_reindex` 経由の再構築に依存する。パスマッチング: クエリにパス区切り文字（`\` `/`）を含む場合、`normalized_key`（= `normalize_entry_key(target_path)`）に対して Substring マッチを試みる。スコアは `3000 - min(byte_pos, 500)`。name/file_name/kana 全て不成立時のフォールバック。`has_path_sep` 時は Fuzzy ビットマスク pre-filter をスキップする
- `history.rs`: 起動履歴・クエリ別履歴・フォルダ展開履歴の管理、バイナリ永続化
- `folder.rs`: フォルダ内列挙とフィルタ/ソート
- `indexer.rs`: スキャン対象列挙と重複排除、インデックスキャッシュ
- `query.rs`: クエリ正規化（`normalize_query`）、履歴クエリキー正規化（`normalize_history_query_key` — `normalize_query` + パス区切り統一を一元化）、`char_bitmask`（文字存在ビットマスク計算 — `search.rs` と `indexer.rs` の両方が使用）
- `binfmt.rs`: `magic + version` 付きバイナリ入出力共通処理
- `error.rs`: `BinError`（バイナリシリアライズ/デシリアライズ失敗）と `ConfigError`（設定バリデーション失敗）の error 型定義
- `window_data.rs`: ウィンドウ位置（`window.bin`）の保存/復元
- `instant.rs`: インスタントコマンドの変数展開（`expand_instant_command`）と前方一致フィルタ（`filter_instant_commands`）
- `ui_types.rs`: フロントエンドとの IPC 用データ型

## 開発ルール

- 新規ロジックは可能な限りこの crate に追加してテスト可能性を維持する
- `#[cfg(test)]` でユニットテストを必ず書く
- 検索スコア計算は `search.rs`、フォルダ列挙は `folder.rs` に集約（DRY）
- **`#[cfg(windows)]` で Win32 依存コードを追加する場合**: テストも `#[cfg(windows)]` で囲むか、OS リソースが存在しない環境でも安全にスキップできるよう `if let Some(...) =` パターンを使う。`assert!(value.is_some())` のような環境前提アサーションは環境依存テストになる

## 実装前チェック（必須）

- 共通原則は `AGENTS.md` の「事前調査（レビュー未然防止）」に従う
- `search.rs` で `Ord` / `Reverse` / `BinaryHeap` を扱う変更では、`BinaryHeap` の先頭が最良/最悪のどちらかを実装前に明記する
- `search.rs` の top-k 更新ロジックを変更する場合は、入力順を変えても結果が不変であるテストを追加または更新する
- `SearchEngine` にフィールドを追加する前に: 既存の並列 Vec（特に `normalized_keys`）で代替できないか先に検討する。再利用できれば 5 箇所同時更新・IndexCache バージョンバンプが不要になる
- `SearchEngine` に新しい並列 Vec フィールドを追加するとき: `EntryView` 構造体・`entry_view()` メソッド・`assemble()` 内の `debug_assert!` を同時に更新し、全 Vec 長の同期を保つ。Wave 1 の文字列正規化は `compute_wave1` に、Wave 2 のビットマスク計算は `compute_wave2` に追加する（`new()` / `new_with_migemo()` / `new_with_cached_masks()` が共有）
- **`kana_lower_names` は条件付き構築（migemo 有効時のみ）で長さ `{0, entries.len()}` の例外**: `assemble` の `debug_assert!` は他 5 Vec を `== entries.len()` で検証するが kana は `is_empty() || == entries.len()` を許す。`kana_lower_names[i]` へアクセスする全箇所は `!kana_lower_names.is_empty()` ガードを通す。条件分岐は `compute_wave1(.., migemo_enabled)` と `new_with_cached_masks` の v4/v3 両パスに**同時に**入れる（片方だけだと migemo ON でも空になる）。migemo を構築入力に追加した設定なので `config_watcher::needs_reindex` と `indexing.rs` の開始時キャプチャ↔完了後 `needs_rebuild` 比較の**両方**に migemo を含める
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

### `char_bitmask` は `query.rs` に一元化済み

`query::char_bitmask()` が唯一の定義（bits 0-25 = a-z, bits 26-35 = 0-9）。`search.rs` と `indexer.rs` の両方がこの関数を import して使用する。ロジックを変更する場合は `query.rs` の1箇所のみ修正すればよい。

### `query.rs` の正規化と `Cow<str>` 遅延アロケーション

`normalize_query()` は `Cow<str>` を返す。ASCII 小文字のみのクエリでは借用（ゼロアロケーション）、大文字・アクセント・連続空白がある場合のみ所有に切り替わる。正規化ロジックを変更するとき、この2パスの一貫性（チェック条件とビルド条件が同じ入力に対して同じ結果を生む）を壊さない。

### incremental cache とパスクエリの非互換

`has_path_sep` 時は incremental search を無条件で無効化する。理由: `norm_query`（アクセント折畳み・スペース圧縮）と `path_query`（生クエリベース・アクセント保持・スペース保持）で正規化が異なり、`norm_query` の `starts_with` では `path_query` の単調性を保証できない。パス区切りを含むクエリは稀なため性能影響は無視できる。将来 incremental を有効化するには `prev_path_query` を別途保持して単調性を検証する必要がある。

## データ永続化の注意

- シリアライザを切り替える場合は**必ずバージョン番号をバンプ**し、旧形式のフォールバックデシリアライザを追加する。切り替え前後でバイト列の互換性はほぼ存在しない（例: bincode の u32 は 4バイト LE、postcard は LEB128 varint）
- **データの意味（セマンティクス）を変更する場合もバージョン番号をバンプする**。バイト列のフォーマットが同一でも、値の解釈が変わればデータ破損と同じ（例: 絶対座標→モニター相対座標。旧データをそのまま新セマンティクスで読むと位置がずれる）
- `deserialize_failed → save()` パターン（デコード失敗時に空データを即時上書き保存）は HistoryStore など学習データを持つモジュールでデータ喪失を招く。フォールバック読み込みを先に試み、次回の通常 save() で新形式に昇格させること
- **読み込み失敗は種類で扱いを分ける（`Config::load`）**: 不在（`NotFound`）= 既定値を生成・保存 / 内容破損（TOML parse 失敗・非 UTF-8 `InvalidData`）= `config.toml.bak` へ退避し既定値・**保存しない** / 一時的失敗（権限・ロック等）= 退避も上書きもせず既定値・**保存しない**。`Err(_)` 一括 first-run 扱いは一時的失敗で実データを既定値に潰す。**1分岐だけ直しても同じ `match` の兄弟分岐に同じ破壊的フォールバックが残る**ため、読み込み失敗を直すときは全分岐の保全方針を揃える（#338/#343: アドバーサリアルレビューが兄弟分岐＝read 失敗 arm の漏れと「後続 save で破損元が失われる」非対称を検出した）
- **TOML フィールドを別の struct に移動するとき**: 旧フィールドを削除するのではなく `#[serde(default, skip_serializing)] pub field: Option<T>` として残し、`apply_migrations()` で `self.old.field.take()` → 新フィールドへ代入する。`Config::default()` の明示的 struct 初期化に `field: None` を追加するのを忘れない。また、`apply_migrations()` には複数のマイグレーションが存在するため、一部だけをテストする場合でも他の副作用（`additional → scan` 移行等）を踏まえたアサーション順序・内容を設計する

## history.rs のキー正規化に関するチェックリスト

`history.rs` のパスキー形式（`normalize_entry_key` の適用有無など）を変更したとき、以下の3者が揃っているか確認する。**クエリキーの正規化は `query.rs::normalize_history_query_key` に一元化済み**（`normalize_query` + パス区切り `/` `¥` → `\` 統一）。新しいコードパスで手書き重複を追加せず、このヘルパーを使うこと:

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
8. **`IndexCacheRef<'a>` 構造体**: `save_cache_sorted` が使う Serialize 専用の借用版。新フィールドを **`IndexCache` と完全に同じ順序・型**（`Vec<T>` → `&[T]`）で追加する。postcard は構造順依存のため、ずれると無言でフォーマットが変わり旧 `index.bin` を破損する。バイト一致は `index_cache_ref_serializes_identically_to_owned` テストでガード

1つでも欠けるとキャッシュヒット時/ミス時で異なる結果を返す。

## engine.rs のロック最小化パターン

`Engine` は Tauri 側で `Mutex<Engine>` に包まれる。ロック保持時間を最小化するための2つのパターンがある:

- **`FolderListContext`**: ロック内で `capture_folder_list_context()` してスナップショットを取得 → ロック外で I/O（`read_dir_entries`）→ ロック内で `finalize_folder_list()` でスコアリング。設定変更との微小な不整合は許容する設計判断
- **`PrebuiltIndex`**: ロック外で `PrebuiltIndex::new(entries)` を構築 → ロック内で `apply_prebuilt_index()` でスワップ。SearchEngine の構築コスト（Wave 1/2 の並列計算）をロック外に追い出す

新しい Engine メソッドを追加するとき、I/O やインデックス構築をロック内で行わないよう注意する。

## index.bin 書き込みの排他（INDEX_WRITE_LOCK）

`index.bin` を scan+save する経路は**すべて `INDEX_WRITE_LOCK`（`indexer.rs` の module-level `static Mutex<()>`）を経由する**。`BinFile::save` の tmp→rename は固定 tmp 名（`index.bin.tmp`）での原子的置換であり、単一書き手が前提。複数経路が同時に書くと tmp ファイルを食い合い破損する。

- 権威的書き手（`rebuild_and_save` / `load_or_scan_with_stats` の cache-miss 枝）: `with_index_write_lock`（blocking）で取得
- 日和見的書き手（`try_background_rescan`）: `try_with_index_write_lock`（`try_lock`、競合時スキップ）。本式ビルドが走っていれば再スキャンは不要
- `save_cache_sorted` 自身はロックを取らない（呼び出し側が保持する契約）。ロック取得済みのクロージャ内から呼ぶ。`save_cache_sorted` がロックを取ると自己デッドロックする
- **`index.bin` を書く新しい経路を追加するときは、必ず `with_index_write_lock` / `try_with_index_write_lock` を経由させる**

## indexer.rs の背景再スキャン

`load_or_scan_with_stats` はキャッシュヒット時、再スキャンを*その場で spawn せず* `LoadOrScanResult.rescan_task`（`Some(BackgroundRescanTask)`）として返す。`src-tauri` が `AppHandle` を持った状態で低優先度スレッドで `task.run()` し、`RescanOutcome::Changed` ならアイコンキャッシュを無効化する。

- ロジック（lock 取得・`scan_all` / `sort_entries_canonical` / `entries_equal` 比較・`save_cache_sorted`）は `snotra-core`、spawn とアイコン無効化は `src-tauri`。`index.bin` は snotra-core の資源、`icons.bin` は src-tauri の資源——所有者に責務を寄せている
- `try_background_rescan` はアイコンキャッシュに触れない。`RescanOutcome::{Skipped, Unchanged, Changed}` で結果を伝え、呼び出し側が `Changed` を見て無効化する
- スキャンロジックを変更するとき、このバックグラウンドパスにも影響することを意識する

## エントリ名の導出ルール

`indexer.rs` のスキャンでは:
- **ファイル**: `file_stem()` を `name` に使用（拡張子なし）。例: `firefox.lnk` → `name: "firefox"`
- **フォルダ**: `file_name()` を `name` に使用（そのまま）。例: `Projects/` → `name: "Projects"`

`folder.rs` のフォルダ内列挙では `file_name()` をそのまま使う（拡張子付き）。この違いは意図的で、フォルダ展開時にはファイル拡張子がフィルタリングの手がかりになるため。

## Config のデシリアライズ経路

`Config::load()` はデシリアライズ後に `apply_migrations()` で後処理（レガシーフィールド移行・正規化・システムショートカットフォールバック）を実行する。**Config をデシリアライズする新しい経路**（インポート、テスト用ファクトリ等）を追加するときは、`apply_migrations()` の適用要否を明示的に判断する。迂回すると旧版データの移行漏れ（例: `paths.additional` の消失）が起きる。

### `Option<T>` フィールドを migration の「明示設定か否か」の sentinel に使う場合

`None` = TOML 未記載、`Some(v)` = 明示設定 として使う場合、`SearchConfig::default()` は **`None` を返すこと**。`Some(default_value)` を返すと、`[search]` セクション全体が TOML に存在しない場合でも serde が `SearchConfig::default()` を使うため `Some(v)` になり、`apply_migrations()` の `is_none()` チェックが常に false になって legacy 値の移行が起きなくなる。

- 正しいパターン: `Default` → `None`、使用時に `effective_*()` アクセサで `unwrap_or_else(default_fn)` する
- migration 後の「None を解消する」処理 (`get_or_insert_with`) は `apply_migrations()` の最後にまとめて実行する
- `reset_to_default()` でも `Config::default()` 後に `apply_migrations()` を呼び、None を解消してから保存する
