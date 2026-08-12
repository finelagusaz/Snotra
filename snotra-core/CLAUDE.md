# snotra-core

純ロジック lib crate。Win32 非依存でユニットテスト可能。

- 各ルールは「**太字 = 守る指示**、後続 = 理由・経緯」の形式。迷ったら太字部分に従えば安全
- 本ファイルの構成: モジュール構成（責務 + モジュール別の不変条件）→ 開発ルール・実装前チェック → クロスモジュール不変条件 → データ永続化 → モジュール別の詳細規約

## モジュール構成

各モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする。本節はファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**を記す。`//!` はコード側で改名に追従し、`cargo doc` の intra-doc link 検査が相互参照の腐敗を捕まえる（#562）。

- `lib.rs`: モジュール宣言のみ（責務を持たない）
- `engine.rs` — 検索・履歴・設定を単一ロックに統合する facade（責務は `//!`）。以下は engine ロックに閉じる横断コヒーレンシ:
  - **`IndexInputs`**: index 構築入力（scan / show_hidden_system / include_path_env / migemo_enabled）の単一定義。**「変わったら索引を建て直さねばならない入力」だけを載せる**——`show_icons` は「アイコンキャッシュを落とす契機」を運ぶために長く相乗りしていたが、この判定は**向きを持たない**ので `false → true` でも全再構築を払って効果 0 になり、#996 follow-up で外した（破棄は `snotra` の `config_watcher::icons_turned_off` がエッジで撃つ）
  - **`index_stale` ledger**: `mark_index_stale` / `begin_index_drain` → snapshot / `complete_index_drain` → swap + re-diff で stale をクリア / `is_index_stale`。コヒーレンシ判断を engine Mutex（軸1）に閉じ、config 変更→index 再構築の lost-update を塞ぐ（#347/#348-A）
  - **`complete_index_drain` は「ビルド開始時 snapshot == 現在 IndexInputs」のときだけ stale をクリアする**（ビルド中変更を取りこぼさない）
- `config.rs` — `config.toml` の読込/保存・既定値補完、`Language` enum の定義（責務は `//!`）。以下は設定移行・デシリアライズ経路の不変条件:
  - **保存先の導出は `Config::config_dir()` の 1 点だけである**（#803）: `config.toml` / `history.bin` / `index.bin` / `icons.bin` / `window.bin` のすべてがここから派生し、`dirs::config_dir()` を直接呼ぶ箇所は他に無い。**新しい永続ファイルを足すときもここを通す**——別経路で組むと `SNOTRA_CONFIG_DIR` が効かず、「導出が 2 経路」の欠陥（`docs/development-principles.md`「config の値は到達性の検出器を持たない」）になる
  - **env 上書きと既定は非対称である**（契約の全文は `config_dir` / `config_dir_from` の rustdoc）: 上書きは**そのまま**使い `Snotra` を付け足さない・空文字は未設定・展開も絶対化もしない。**判定核 `config_dir_from(override, base)` は env を読まない**ので並列テストから安全に測れるが、その代償として **`config_dir()` が `dirs::config_dir()` を呼んでいること自体は純粋関数のテストからは見えない**（`base` を注入するため）。この結線は `config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix` が env を**読むだけ**で pin する。**Windows では `dirs::config_dir()` と `dirs::data_dir()` は同一**（どちらも RoamingAppData。dirs クレート 6.0.0 の Windows 実装で実測）なので、実際に危険な取り違えは `config_local_dir()` / `data_local_dir()`（LocalAppData）である
  - **新しいセクション・設定キーには serde の既定を付ける**（#824）: `SPEC.md`「13.1 設定データ」の「欠損キーはデフォルト補完」が正本で、欠けた 1 キーが `config.toml` 全体を `.bak` 退避へ落とさないための不変条件である。検知器は 2 つ——キー欠落は `empty_section_deserializes_to_default_*` 群、セクション欠落は `config_parses_with_all_sections_omitted`（どちらも必須フィールドが混入すると空文字列 parse が落ちる）。**例外は配列要素の必須フィールド**（`[[paths.scan]]` / `[[openers]]` / `[[instant_commands]]` の各要素・理由は `SPEC.md` 同節）
  - **件数パラメータ**（#388 で役割に合わせて改名済み）: `appearance.visible_rows` = 可視行数 / `search.result_limit` = **検索・フォルダの結果リスト最大長**（`Engine::search`/`capture_folder_list_context` の fetch_limit）/ `search.recent_limit` = 空クエリ recent 件数（`recent_history`）
  - **旧キーの後方互換移行**: 旧キー（`max_results`/`top_n_history`/`max_history_display`）は `apply_migrations()` が `skip_serializing` の legacy フィールド経由で移行する（2層レガシー: `result_limit` ← `[search].top_n_history` ← `[appearance].top_n_history`）
  - `Config::icon_cache_cap()` はこれらから派生（独立した config キーを持たない）。実上限は名前でなく `engine.rs` の dispatch で確認する
  - **`apply_migrations()` は migration 系統ごとの private fn へ段階分解済み**（issue #435）: `migrate_legacy_additional_paths` / `migrate_legacy_count_params` / `resolve_count_param_defaults` / `sanitize_fuzzy_history_cap_ratio` / `migrate_instant_legacy_commands` / `fallback_invalid_hotkey`
  - **migration の呼び出し順は元と同一に固定する**: `migrate_legacy_additional_paths`（`paths.additional`→`scan` 追加）→ `paths.normalize_scan_paths()`（dedup）の順序だけが真の依存（先に追加されたエントリを後続の正規化がまとめて dedup する）。他のステップは独立だが diff 最小化のため元の並びを保つ
- `opener.rs` — 外部ツール起動ルールの解析・正規化・マッチングと Win プリセット検出（責務は `//!`、公開 API 契約は各 `///` を正とする。`config.rs` から分離・#435）
  - **依存方向は `config.rs` → `opener.rs`**: `OpenerRule`/`OpenerTool` は `Config.openers` として config.toml に紐づく serde 型のため型定義はこちらに置き、`config.rs` が `pub use crate::opener::{...}` で re-export して `snotra_core::config::...` の既存呼び出し元パスを維持する
  - 逆方向の依存として `normalize_opener_target` が `config.rs::normalize_scan_path_key` / `normalize_extensions`（`pub(crate)`、`paths.scan` の正規化とも共有する汎用ヘルパー）を使う
- `hotkey.rs` — 永続ホットキー文字列の意味解析とシステムショートカット競合判定（責務は `//!`）。`HotkeyConfig` は serde 互換のため `config.rs` から re-export し、設定検証・UI・Win32 platform は同じ `ParsedHotkey` を消費する。文字列 parser を下流へ複製しない
- `search.rs` — 検索順位計算・履歴ブースト・incremental search キャッシュ・空クエリ時履歴候補（責務・スコア階層は `//!` と `SearchEngine` の struct doc）。以下は並列 Vec レイアウトの不変条件:
  - **並列レイアウト**: `SearchEngine` は `entries` / `lower_names` / `lower_file_names` / `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks` を添字で対応づけた並列の列として持つ（cache locality）。**すべてが `Vec` ではない**——`lower_names` / `lower_file_names` は `str_arena` のアリーナ、表示名は `PathStore` の `NameArena` である。**エントリ数に比例する確保を持つ列は 1 つも残っていない**（`kana_*` は migemo 有効時のみ per-entry・額は `PERFORMANCE.md`「採用: 派生文字列 2 列も文字列アリーナで持つ」）
  - **正規化キー（履歴照合・パスマッチ）は索引に持たない**: `target_path` から `normalize_entry_key_into` で導出し、スレッドローカルのバッファへ詰め直す（唯一の経路は `search/scoring.rs` の `with_normalized_key`）。**畳み込み比較を別実装で書き起こしてはならない**——記録側と照合側が同じ関数を通ることがバイト一致の根拠であり、1 バイトずれると履歴照合が沈黙で外れる（クラッシュせず検索結果も返り、ブーストだけが消える）
  - **`kana_lower_names` / `kana_char_masks` は `migemo_enabled` が true のときのみ構築し、無効時は空 Vec**（migemo 無効ユーザーの死蔵メモリ ~2.1–2.7MB/50k を削る・構築も約 2 倍速、issue #337）。2 つの kana 系 Vec は必ず同時に空/同長（`assemble` の debug_assert が検証）。空 Vec のとき検索ループは `kana_available` 空ガードで `kana_lower_names[i]` アクセスを回避し、Fuzzy pre-filter は `kana_char_masks.is_empty()` チェックで kana 経路を棄却する（構築時 migemo OFF→検索時 ON の窓での panic 防止）
  - **migemo トグルの反映は index 再構築経由**: `update_config` は engine を再構築しないため、`config_watcher` が engine の `IndexInputs` 差分で `start_index_build` を kick する再構築に依存する（#347 Phase 2 で `needs_reindex` は `IndexInputs` に統合）
  - **パスマッチング**: クエリにパス区切り文字（`\` `/`）を含む場合、導出した正規化キー（= `normalize_entry_key(target_path)` と同値）に対して Substring マッチを試みる。スコアは `3000 - min(byte_pos, 500)`。name/file_name/kana 全て不成立時のフォールバック。`has_path_sep` 時は Fuzzy ビットマスク pre-filter をスキップする
  - **スコアリング・順位計算は `search/scoring.rs` に分離**（#600。責務は `//!`）: `mod score_tier`（+ `const _` 全順序アサーション）・thread-local `MATCHER`・`EntryView` / `entry_view`・`score_one_entry`・`ScoredEntry` と `Ord` 一式・`heap_into_results`・`adjusted_history_boost`・`kana_substring_score`・`match_score_single_cached`・`TopK`（top-k 更新規則の一元化。fold/reduce が同じ `push`/`merge` を共有・#602）。候補選択・rayon fold/reduce の骨格・incremental cache 更新は `search.rs` に残す。子から親の並列 Vec private を直接読み、共有型（`EntryView` / `ScoredEntry` / `score_one_entry` / `adjusted_history_boost` / `TopK` 一式）は `pub(super)`（`heap_into_results` は #602 で `TopK::into_results` 内部に隠蔽され private）
  - **クエリ計画は `search/query_plan.rs` に分離**（#599。責務は `//!`）: `QueryPlan` と `prepare_query_plan`（正規化クエリ・dot/path 判定・Fuzzy bitmask・migemo かなクエリ・UTF-32 needle・パス照合クエリ・履歴キーの純粋導出）。incremental 判定と前回状態の read/write は `search.rs` の `IncrementalCache`（`can_reuse` / `update`・#601）に残す。`QueryPlan` とフィールドは `pub(super)` で親のみに公開
  - **`target_path` は索引に持たず、フォルダ木の接頭辞共有から組み立てる**（`search/path_store.rs`。責務は `//!`）: `PathStore` が `CompactEntry`（`parent` / `aux` / `is_folder`・12 B）と表示名の `NameArena` と intern 表を持ち、`raw_into`（原文）と `normalized_into`（正規化キー）の 2 系統で組み立てる。**この 2 つを流用してはならない**——tie-break（`ScoredEntry::cmp` → `cmp_paths`）と `SearchResult.path` は原文のバイトを、履歴照合とパスマッチは正規化キーを要求する。**セグメント単位の比較は禁止**（区切り `\`(0x5C) は `-`(0x2D) より大きく、バイト順と一致しない。検知器は `search/tests/ranking.rs` の `search_result_order_is_stable_across_target_path_representations`）。**`index.bin`（v7）も木そのものを持つ**ので `assemble` は木を建て直さず、`PathStore::adopt` で索引側の並べ方へ移すだけである（反復 10。木を建てる規則は `index_tree.rs` の `IndexTree::build` 1 点で、ディスクへ書く側とここが同じものを通ることが両者の一致の根拠）
  - **構築処理は `search/build.rs` に分離**（#598。責務は `//!`）: Wave 1/2・kana マスクの並列構築、IndexCache 復元（v4 ヒット時 Wave 1 スキップ / v3 fallback）、全コンストラクタ（`from_material`（**索引を建てる唯一の入口**・派生データの有無で分岐するのはここだけ） / `new` / `new_with_migemo` / `new_from_tree` / `new_with_cached_masks` / `assemble`）。検索ホットパスは `search.rs` に残す。`kana_char_mask`（query 側と共有しうる純粋関数）は `search.rs` 側に残置
  - **重複する派生文字列は索引に持たず、鎖で共有する**（反復 4・5）: `lower_file_names[i]` → `lower_names[i]` → 表示名（`PathStore::name_at`）の順に、`assemble` が構築時に**測って**上流と一致するものを `None` へ潰す（実測 9.71 + 9.80 MiB / 526,316 ブロック）。`lower_names` の `None` は「表示名と同一」、`lower_file_names` の共有は `CompactEntry.file_name_is_lower_name`（空きパディングゆえ 12 B のまま）で表す——**`lower_file_names` の `None` には「file name 成分が無い」という先客がいる**ため、そちらだけ旗を要した。読み替えは `SearchEngine::entry_view` の 1 点で、同じ順に解決する
    - **判定を全部済ませてから落とす**（鎖ゆえ、先に上流を潰すと下流の比較相手が消える）。**潰す位置はビットマスク確定より後**でなければならない（先に潰すと `file_char_mask(None) == 0` で pre-filter が false negative を出す）。どちらの誤りも**結果は正しいまま削減だけが減る**ので挙動テストでは捕まらない——検知器は `search/tests/build.rs` が索引の `is_none` まで見る
    - **`is_folder` から推論してはならない**——実データの folder 100% で成り立つのは indexer の名前導出規則の帰結であって、`SearchEngine::new` が受け取る `AppEntry` の性質ではない（検知器は `shared_file_name_flag_is_measured_not_inferred_from_is_folder`）
    - **`index.bin`（v6）は同じ潰し方を持つ**（反復 8）。ディスクだけが全件を実体で持っていた頃は、読んで確保して `assemble` が即座に捨てていた（実測 21.63 → 1.90 MiB・確保 -527,000）。**表現は `Option<String>` と 3 状態の `LowerFileName`（`Absent` / `SameAsLowerName` / `Text`）に分かれる**——`lower_file_names` の `None` には「file name 成分が無い」という先客がおり、メモリ側は `CompactEntry` の空きパディングで解いたが**ディスクに空きパディングは無い**
    - **潰し済みかどうかは型で区別する**（`CachedLower::{Collapsed, Raw}` → `DerivedStrings::{Collapsed, Measured}`）。**同じ型で渡してはならない**——`assemble` の共有判定は「全要素が `Some`」を前提にしており、潰し済みの列を流すと `lower_file_names[i]` と `lower_names[i]` がどちらも `None` のときに「一致」と読まれ、**file name 成分を持たないエントリに旗が立つ**（実データのフォルダ 256,262 件すべてが該当し、`entry_view` はそれらの file name として `lower_name` を返す。結果は「それらしく」出るので挙動テストでは捕まらない）
    - **判定は `query::measure_derived_sharing` の 1 か所である**——記録側（`save_cache_sorted_in`）・追記側（`extend_cached_masks`）・適用側（`assemble`）が同じ関数を通ることだけが、ディスクとメモリで潰れ方が一致する根拠になる（`normalize_entry_key_into` と同じ理屈）。別実装を書くと、その経路の分だけが索引の読み替えとずれ、**スコアという形で静かに現れる**
    - **記録側が潰したものを、cache-miss の枝がそのまま索引の表現に使う**（反復 11）。導出は `derive_columns`（I/O を持たない）が持ち、`save_cache_sorted_in` はそれを書いてから `CachedMasks` へ畳んで返す——かつては**計算して書いた直後に捨て、`new_from_tree` が全件を実体化してから同じものを作り直していた**（額は `PERFORMANCE.md`「採用: 保存が返した派生データを cache-miss がそのまま使う」が正本）。**どのコンストラクタが選ばれるかを決めるのは材料が派生データを持つかと `CachedLower` の variant だけであり、`cache_changed` ではない**（分岐は `SearchEngine::from_material` の 1 か所に閉じ、条件の正本は `save_cache_sorted` と `load_cache_in` の分岐）。両経路が同じ表現へ着地することを CI で守るのは `search/tests/build.rs` の合成 fixture（`save_side_collapse_and_assemble_measurement_agree_at_entry_view`）である——実データ全件を突き合わせる相方は `#[ignore]` かつ scan パスが空なら自己スキップするので、**規模と実際のパスの形（深い鎖・根と非根の混在・非 ASCII・大文字の拡張子）は手元で明示的に走らせたときしか検証されない**。実データの原文は `common::real_scanned_entries`（ファイルシステム走査）から取る——`index.bin` から取ると組み直し対組み直しの不動点になる。**木と派生データは `indexer::IndexMaterial` が組のまま運ぶ**（フィールドは private・`CachedMasks` が 4 本を束ねるのと同じ理屈で、こちらが消すのは「木を伸ばしたのにマスクへ追記し忘れる」誤りである）。PATH エントリのマージは `IndexMaterial::extend_with_path_entries` の 1 メソッドで、起動経路と背景の再構築が同じそこを通る。守るのは `search/tests/build.rs` の `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree`（追記の欠落・マスクの取り落とし）と `path_merge_extends_the_tree_even_without_derived_data`（派生データが無いときの木の伸び）で、**3 種の変異を注入してどれかが落ちることを実測してある**。**crate 外からは迂回できない**（`IndexTree::extend_with_roots` も `pub(crate)`）が、**crate 内では書ける**——`snotra-core` の中に PATH マージの呼び出し点は無いので、現状は残余ではなく余地である
  - **常駐ヒープの内訳は `search/footprint.rs` が数える**（計測専用。責務は `//!`）: `SearchEngine::footprint_rows`（`#[doc(hidden)] pub`）と `PathStore::footprint_rows` が、構築**後**の自分自身を走査して項目ごとの確保バイト・ブロック数を返す。出力と実測の突き合わせは `tests/memory_footprint.rs`。**構築前の `Vec<AppEntry>` を走査して代用しない**——`target_path` は `PathStore` へ組み替えられて解放されるため、その走査は存在しない物体を測る（反復 3 で実際にそうなった）。**走査は `self` の網羅的な分解で書く（`..` を書かない）**ので、並列 Vec を足したときの漏れはチェックリストではなくコンパイラが捕まえる
  - **ユニットテストは `search/tests/` に機能別分割**（#597。責務は各ファイルの `//!`・製品コードは `search.rs` のまま）: 索引 `search/tests/mod.rs`、共通 fixture `search/tests/common.rs`、`search/tests/basic.rs`（基本検索・拡張子・正規化）/ `search/tests/ranking.rs`（top-k・タイブレーク・ビットマスク）/ `search/tests/incremental.rs`（incremental キャッシュ）/ `search/tests/migemo.rs`（かな検索・条件付き構築）/ `search/tests/path.rs`（パスマッチ）/ `search/tests/build.rs`（構築の不変条件。余剰容量は検索結果を変えないため挙動テストでは捕まらない）/ `search/tests/performance.rs`（`#[ignore]` ベンチ・メモリ計測）
- `history.rs`: 起動履歴・クエリ別履歴・フォルダ展開履歴の管理、バイナリ永続化
  - **剪定容量 `top_n` は焼き込まず `prepare_save_if_dirty`/`prepare_flush`/`prune` の引数で受け取る（live-read）**: `Engine` が呼び出し時に現在の config（`effective_result_limit()`）を渡すため、`result_limit` 設定変更が再起動なしで反映される（#348）
  - **`HistoryStore` に `top_n` フィールドを再導入しないこと** — 焼き込むと設定変更が反映されないドリフトが復活する
- `folder.rs` — フォルダ内エントリの列挙とフィルタ/ソート（責務は `//!`）
- `indexer.rs` — スキャン対象の列挙・重複排除とインデックスキャッシュ（責務は `//!`）
- `query.rs` — クエリ/履歴キーの正規化と文字ビットマスク（責務は `//!`）
- `str_arena.rs` — 疎な派生文字列の列（`lower_names` / `lower_file_names`）を、要素ごとの確保を持たない表現で保つ（責務は `//!`）。**線上表現は `Vec<Option<String>>` / `Vec<LowerFileName>` のままである**——その一致を守るのは同ファイルの `*_wire_format_is_identical_to_*` 2 本であって golden bytes ではない（射程は `//!`）
- `index_tree.rs` — `target_path` をフォルダ木の接頭辞共有で表す、オンディスクと索引が共有する表現（責務は `//!`）。**辿る規則（`walk_to_root` / `raw_path_into`）はここが唯一持ち、記憶域の並べ方だけを `TreeNodes` が抽象化する**——`indexer` の並列 Vec と `search` の構造体の列が同じ実装を通ることが、ディスクと索引で木がずれないことの根拠である
- `binfmt.rs` — `magic` + `version` 付きバイナリ入出力の共通処理（責務は `//!`）
- `error.rs` — crate 共通の error 型（責務は `//!`）
- `window_data.rs` — ウィンドウ位置（`window.bin`）の保存/復元（責務は `//!`）
- `instant.rs` — インスタントコマンド（プレフィックス起動の URL/コマンド）の展開。公開関数の署名・契約と変数展開の中核（修飾子パイプ・encoding-as-sink・`{{X}}` エスケープ・date/uuid 純粋性・`format_date` の panic 安全 #394）は `//!` と各 `///` を正とする
- `ui_types.rs`
- `tests/search_frame_cost.rs`（crate ルート統合テスト）: #634 G-SYNC の `Engine::search` facade フレームコスト実測ハーネス（`#[ignore]`・手元 release 実行専用。`search/tests/performance.rs` との層の区別は `//!`）
- `tests/memory_footprint.rs`（crate ルート統合テスト）: 索引の常駐ヒープをアロケータ実測で取るハーネス（`#[ignore]`・手元 release 実行専用。責務は `//!`、計測値は `PERFORMANCE.md`）
- `tests/path_query_cost.rs`（crate ルート統合テスト）: パスクエリ（`has_path_sep`）全走査のコスト実測ハーネス（`#[ignore]`・手元 release 実行専用。責務は `//!`、計測値は `PERFORMANCE.md`）。**`normalized_keys` を保持するか導出するかの差を測る唯一の計器**であり、既存の bench 群はパス区切りを含むクエリを 1 つも持たない

## 開発ルール

- 新規ロジックは可能な限りこの crate に追加してテスト可能性を維持する
- `#[cfg(test)]` でユニットテストを必ず書く
- **ユニットテストの fixture に `HistoryStore::load()` を使わない**（#963）。実 `%APPDATA%\Snotra\history.bin` を読むため開発者のマシン状態で結果が変わり、しかも CI のランナーにはそのファイルが無いので**食い違いは CI では緑のまま開発機でだけ現れる**。空は `HistoryStore::empty()`、特定の内容は `HistoryStore::load_in` へ注入する。実運用の姿を測る計測ハーネス（`tests/` の `#[ignore]` ベンチ）だけは `load()` のままでよい
- 検索スコア計算は `search.rs`、フォルダ列挙は `folder.rs` に集約（DRY）
- **UI 表示文字列を持たない**: この crate は Win32 非依存の純ロジック層。エラーは `is_error: true` フラグで呼び出し側へ伝え、ユーザー向け文言の組み立て・表示は UI 層（`src-tauri/src/egui_shell/strings.rs`・`snotra-settings/src/i18n.rs`）の責務。ここに表示メッセージを埋め込まない
- **`#[cfg(windows)]` で Win32 依存コードを追加する場合**: テストも `#[cfg(windows)]` で囲むか、OS リソースが存在しない環境でも安全にスキップできるよう `if let Some(...) =` パターンを使う。`assert!(value.is_some())` のような環境前提アサーションは環境依存テストになる

## 実装前チェック（必須）

- 共通原則は `AGENTS.md`「事前調査（レビュー未然防止）」に従う
- `search.rs` で `Ord` / `Reverse` / `BinaryHeap` を扱う変更では、`BinaryHeap` の先頭が最良/最悪のどちらかを実装前に明記する
- `search.rs` の top-k 更新ロジックを変更する場合は、入力順を変えても結果が不変であるテストを追加または更新する
- `SearchEngine` にフィールドを追加する前に: 既存の並列 Vec で代替できないか、あるいは**そもそも持たずに導出できないか**を先に検討する。再利用・導出できれば 5 箇所同時更新・IndexCache バージョンバンプが不要になる。**導出を選ぶ判断は「その読みが早期 return の前にあるか後ろにあるか」で決まる**——同じフィールドでも、フィルタとして使われる読みと通過後の装飾として使われる読みではコストの桁が違う（`normalized_keys` は後者が主で導出へ移せた・`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）
- `SearchEngine` に新しい並列 Vec フィールドを追加するとき: `EntryView` 構造体・`entry_view()` メソッド・`assemble()` 内の `debug_assert!` **と `shrink_to_fit()`**（余剰容量は索引が伸長しないぶん最後まで常駐する。理由は `assemble` の doc、検知は `search/tests/build.rs`）を同時に更新し、全 Vec 長の同期を保つ。Wave 1 の文字列正規化は `compute_wave1` に、Wave 2 のビットマスク計算は `compute_wave2` に追加する（`new()` / `new_with_migemo()` / `new_from_tree()` / `new_with_cached_masks()` が共有。木を受け取る経路は `wave1_from_tree` 経由で `compute_wave1` を通る）
- **`kana_lower_names` / `kana_char_masks` は条件付き構築（migemo 有効時のみ）で長さ `{0, entries.len()}` の例外**:
  - `assemble` の `debug_assert!` は他 5 Vec を `== entries.len()` で検証するが、kana 系 2 Vec は「両方空 or 両方 `== entries.len()`」を許す
  - `kana_lower_names[i]` / `kana_char_masks[i]` へアクセスする全箇所は `is_empty()` ガードを通す（`kana_char_masks` は `kana_lower_names` から `compute_kana_char_masks` で導出し、3 コンストラクタ全経路で `assemble` 直前に構築する）
  - 条件分岐は `compute_wave1(.., migemo_enabled)` と `new_with_cached_masks` の v4/v3 両パスに**同時に**入れる（片方だけだと migemo ON でも空になる）
  - migemo は index 構築入力なので、engine の `IndexInputs`（`config_watcher` の kick 判定と `complete_index_drain` の re-diff が共有する**単一定義**）に含める（#347 Phase 2 で `needs_reindex` / in-flight `needs_rebuild` を `IndexInputs` に統合・削除済み）
- incremental search キャッシュに述語や状態を追加するとき: 状態は `IncrementalCache` 型に集約済み（#601。`prev_query` / `prev_candidates` / `prev_mode` / `prev_kana_query`）。read（`can_reuse`）と write（`update`）を**対で**変更し、`/cache-check` で単調性を検証する。read が参照する全フィールドを `update` が書くこと（型に閉じたので対称更新漏れは起きにくいが、フィールド追加時は両メソッドを同時に触る）
- `query.rs` の正規化を変更する場合は、タブ・全角スペース・NBSP を `' '` に統一するテストと冪等性テストを追加または更新する
- `folder.rs` のソート順変更時: 順序（先頭要素が最良）と 2 段階の構造（O(N) 平均の top-k 選択 ＋ 安定ソート）は `//!` が正本——崩さない。入力順に依存しないことを確認するテスト（`score_entries_top_k_order_independent_of_input_order`）を通す

## クロスモジュール不変条件

### `normalize_entry_key` の冪等性契約

`indexer::normalize_entry_key` は「小文字化 + `/` → `\\`」の正規化関数。**2回適用しても1回と同じ結果になる（冪等）** ことが設計契約であり、`migrate_normalize_keys_is_idempotent` テストで保証されている。以下の3モジュールが依存する:

- `indexer.rs`: スキャン時の重複排除キー
- `history.rs`: 全記録・参照・マイグレーションのキー正規化
- `search.rs` / `search/scoring.rs`: 履歴照合とパスマッチのキー（索引には持たず `with_normalized_key` が導出する）

**規則の定義は `normalize_entry_key_into` 1 つである**（`normalize_entry_key` はその薄い包み）。記録側と照合側が同じ関数を通ることがバイト一致の根拠なので、**この関数を迂回する畳み込み比較を書かないこと**。ASCII 高速路は分岐しても結果が変わらない（ASCII 範囲では Unicode 小文字化と ASCII 小文字化が一致する）ことに依存しており、実インデックスの全パスでの一致を `tests/path_query_cost.rs` の `derives_same_bytes_as_normalize_entry_key` が固定する。

**同じ関数に「末尾セグメントだけ」を通す派生が 1 つある**（`indexer.rs` の `normalize_file_name_key_into`・反復 9）。PATH スキャンが既存エントリを素通しするための**篩**であり、判定ではない。照合する両辺が同じ手順を通ることだけが「偽陰性を出さない」の根拠なので、**ここでも別実装を書き起こしてはならない**（規則の全文と健全性の論証はその関数の doc）。

この関数の正規化ルールを変更する場合は、3モジュール全てへの影響と冪等性テストを確認する。

### `char_bitmask` は `query.rs` に一元化済み

`query::char_bitmask()` が唯一の定義（bits 0-25 = a-z, bits 26-35 = 0-9）。`search.rs` と `indexer.rs` の両方がこの関数を import して使用する。ロジックを変更する場合は `query.rs` の1箇所のみ修正すればよい。

**エントリ単位のマスク導出・file_name 導出も `query.rs` に集約済み**（issue #437）: 「ascii なら `char_bitmask`、非 ascii なら `u64::MAX`」は `query::name_char_mask()`、file_name 側の `None→0` を含む版は `query::file_char_mask()`、`target_path` から小文字 file_name を導出する処理は `query::lower_file_name()` に一元化。`search.rs`（Wave 2 / Wave 1）と `indexer.rs`（`save_cache_sorted_in` / `extend_cached_masks`）が共有する。検索ホットパスのため3関数とも `#[inline]`。

### hidden/system 判定は `indexer::is_hidden_or_system` に一元化済み

`indexer.rs` の `is_hidden_or_system(meta) -> bool`（true = hidden または system）が唯一の定義。`folder.rs::read_dir_entries` はこれを import して使う（旧 `folder.rs::is_hidden_or_system` は逆極性の別定義だったため削除・統合済み、issue #437）。

### `query.rs` の正規化と `Cow<str>` 遅延アロケーション

`normalize_query()` は `Cow<str>` を返す。ASCII 小文字のみのクエリでは借用（ゼロアロケーション）、大文字・アクセント・連続空白がある場合のみ所有に切り替わる。正規化ロジックを変更するとき、この2パスの一貫性（チェック条件とビルド条件が同じ入力に対して同じ結果を生む）を壊さない。

### incremental cache とパスクエリの非互換

`has_path_sep` 時は incremental search を無条件で無効化する。理由: `norm_query`（アクセント折畳み・スペース圧縮）と `path_query`（生クエリベース・アクセント保持・スペース保持）で正規化が異なり、`norm_query` の `starts_with` では `path_query` の単調性を保証できない。パス区切りを含むクエリは稀なため性能影響は無視できる。将来 incremental を有効化するには `prev_path_query` を別途保持して単調性を検証する必要がある。

## データ永続化の注意

- シリアライザを切り替える場合は**必ずバージョン番号をバンプ**し、旧形式のフォールバックデシリアライザを追加する。切り替え前後でバイト列の互換性はほぼ存在しない（例: bincode の u32 は 4バイト LE、postcard は LEB128 varint）
- **データの意味（セマンティクス）を変更する場合もバージョン番号をバンプする**。バイト列のフォーマットが同一でも、値の解釈が変わればデータ破損と同じ（例: 絶対座標→モニター相対座標。旧データをそのまま新セマンティクスで読むと位置がずれる）
- `deserialize_failed → save()` パターン（デコード失敗時に空データを即時上書き保存）は HistoryStore など学習データを持つモジュールでデータ喪失を招く。フォールバック読み込みを先に試み、次回の通常 save() で新形式に昇格させること
- **読み込み失敗は種類で扱いを分ける（`Config::load`）**:
  - 不在（`NotFound`）= 既定値を生成・保存
  - 内容破損（TOML parse 失敗・非 UTF-8 `InvalidData`）= `config.toml.bak` へ退避し既定値・**保存しない**
  - 一時的失敗（権限・ロック等）= 退避も上書きもせず既定値・**保存しない**
  - `Err(_)` 一括 first-run 扱いは一時的失敗で実データを既定値に潰す。**1分岐だけ直しても同じ `match` の兄弟分岐に同じ破壊的フォールバックが残る**ため、読み込み失敗を直すときは全分岐の保全方針を揃える（#338/#343: アドバーサリアルレビューが兄弟分岐＝read 失敗 arm の漏れと「後続 save で破損元が失われる」非対称を検出した）
- **TOML フィールドを別の struct に移動するとき**: 旧フィールドを削除するのではなく `#[serde(default, skip_serializing)] pub field: Option<T>` として残し、`apply_migrations()` で `self.old.field.take()` → 新フィールドへ代入する。`Config::default()` の明示的 struct 初期化に `field: None` を追加するのを忘れない。また、`apply_migrations()` には複数のマイグレーションが存在するため、一部だけをテストする場合でも他の副作用（`additional → scan` 移行等）を踏まえたアサーション順序・内容を設計する
- **serde 表現（enum variant・`#[serde(untagged/flatten/tag)]`）を変更するときは、旧オンディスク形式が deserialize できるテストを「新形式の往復」とは別に必ず追加する**:
  - 旧形式が新構造体に deserialize 失敗すると `toml::from_str::<Config>` が失敗 → `config.toml.bak` 退避 → **全設定リセット**（`apply_migrations()` は deserialize の後に走るため移行では救えない）＝データ損失
  - 旧形式は untagged の `Legacy { .. }` variant 等で必ず受理し、移行を `apply_migrations()` で行う
  - 新形式の往復テストだけでは parse 失敗を検出できず false-green になる（#394: 多観点レビューが `toml` で実証）
- **オンディスクのシリアライズ struct をリファクタするとき（「バイト形式不変」を主張する場合も含む）は、後方互換を *旧形式の凍結バイト列* を入力にした load テストで証明する**:
  - 新コードの出力を golden 化しても保証されるのは forward-stability だけで、「新出力＝旧形式」を独立には証明しない（形式が壊れていても新 golden がそれを凍結して素通りする）
  - 正しい向きは「旧形式の凍結バイト列 → 新コードで deserialize できる」の検証
  - 形式を変える計画は着手前に最小 spike で往復バイト一致を実証してから plan を建てる（前提が崩れれば approach 自体が不成立）
  - #461: owned/borrowed struct の `Cow` 統合で当初 golden を新コード出力から採取し forward-stability のみになっていたのを code-reviewer が検出、凍結バイト列からの deserialize に修正した

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

1. **`IndexCache<'a>` 構造体**: 新フィールドを `Cow<'a, [T]>` で追加（owned/borrowed は #461 で `Cow` 統合済み。save は `Cow::Borrowed` で全件 clone 回避、load は `IndexCache<'static>` へ Owned deserialize）
2. **バージョン番号**: `INDEX_CACHE_VERSION` をバンプ
3. **旧バージョン用フォールバック構造体**: 旧スキーマを `IndexCacheVN` として残す
4. **`load_cache()`**: 新バージョン → 旧バージョンのフォールバックチェーンを追加（Cow フィールドは `.into_owned()` で `CachedMasks`/`entries` へ）。**各枝で `LoadCacheResult.version` に読めた版を入れる**——鎖のどの枝で読めたのかを外から見る手段はこれしか無く、取り違えても検索結果は正しいまま枝選択の退行が残る（フィールドの doc が正本）。昇格そのものは「旧版枝に入ったこと」が条件ゆえ**新しい版を足すたびに手当てする必要はない**（→「indexer.rs の索引更新の契機」）
5. **`save_cache_sorted()`**: 新フィールドの計算ロジックを追加（`Cow::Borrowed` で渡す）
6. **`CachedMasks` 構造体**: 新フィールドを `Option<T>` で追加（旧キャッシュでは None）
7. **`SearchEngine::new_with_cached_masks()`**: 新パラメータを受け取り、None 時は自前で計算

1つでも欠けるとキャッシュヒット時/ミス時で異なる結果を返す。

**版を 1 つ上げると、直前の版が「全ユーザーの `index.bin` が今まさに置かれている版」へ変わる**（反復 10 で 4 件の取りこぼしが同時に出た）。ゆえに 8 番目として、**直前の版を受け取る側を全部数え直す**: (a) `cache_byte_breakdown_in` の鎖（現行版だけを読む形にすると**一番測りたい相手にだけ黙る**）、(b) `load_cache_in_reports_the_version_it_actually_read` の枝（この値だけが昇格判定の入力で、取り違えても**検索結果は正しいまま**）、(c) 直前の版の凍結バイト列を `load_cache_in` 経由で読むテスト（`try_deserialize_with_header` の直呼びでは枝選択・`config_hash` 判定・`CachedLower` の variant・`version` の帰属を 1 つも通らない）。**枝の数を散文に書かないこと**——版を足したときにその数だけが腐り、しかも「揃っている」と読める。

**派生文字列 2 列も同じ形である**（#1003 の次の反復）。`lower_names` は `Cow<'a, LowerNameColumn>`（線上は `seq of Option<str>`）、`lower_file_names` は `Cow<'a, LowerFileColumn>`（線上は 3 variant の `seq of LowerFileName`）で、どちらも要素ごとの `String` を作らない。**`IndexCacheV6` も同じ列の型で読む**——線上表現が v7 と同一だからであり、`Vec<Option<String>>` へ戻すと旧版枝でだけ per-entry の確保が復活する。守るのは `str_arena` の `lower_name_column_wire_format_is_identical_to_vec_of_option_string` / `lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name` で、**2 列とも変異注入で「golden は素通りし、この 2 本だけが落ちる」ことを実測してある**（射程は `str_arena` の `//!`——タグの並べ替えは golden も捕まえる）。

**`names` の型は `Vec<String>` ではないが、線上のバイト列は seq of str である**（#1003）。`IndexCache.names: Cow<'a, NameArena>` は要素ごとの `String` を作らずに 1 本のバッファへ流し込むための表現であり、**線上表現を保っていることが「版を上げずに型を替えられた」根拠のすべてである**。`NameArena` の `Serialize` / `Deserialize` に触るときは、それが**全ユーザーの `index.bin` を版はそのままに読めなくする**変更になりうると承知して触ること（症状は破損ではなく cache-miss ＝ 22〜30 秒の全走査）。守るのは `index_tree.rs` の `arena_wire_format_is_identical_to_vec_of_string` であって golden bytes ではない——**golden の fixture は名前の形を混ぜて持たないので素通りする**（`serialize` に `trim_end` を挟む変異で実測）。

**on-disk 形式の安定ガード**: 旧 `IndexCacheRef`（borrowed 双子）は #461 で `Cow` 統合され消滅した（owned/borrowed のフィールド順ズレ→`index.bin` 無言破損の footgun が型として解消）。統合後は save/load が単一 struct を共有するためフィールド reorder が roundtrip テストを素通りする。**バイト形式の絶対安定は `index_cache_on_disk_format_is_stable`（golden bytes）がガードする**。フィールド追加・順序変更で `INDEX_CACHE_VERSION` をバンプしたら、この golden bytes も更新すること。

## engine.rs のロック最小化パターン

`Engine` は Tauri 側で `Mutex<Engine>` に包まれる。**`Config` だけはその錠の外から読める**——`Arc<RwLock<Config>>` で持ち、`config_handle()` が同じ `Arc` を渡す（#1032・契約の正本は同メソッドと `config` フィールドの doc）。**読みだけが外に出ており、書き（`update_config`）は `&mut self` ＝ `Mutex<Engine>` の内側に残る。この非対称を崩してはならない**——`complete_index_drain` の「index を swap してから現在の `IndexInputs` と照合する」原子性は、書き手が外側の錠を要求することだけで成り立っている（#347/#348-A の lost-update 対策）。ロック保持時間を最小化するためのパターンは他に 3 つある:

- **`FolderListContext`**: ロック内で `capture_folder_list_context()` してスナップショットを取得 → ロック外で I/O（`read_dir_entries`）→ ロック内で `finalize_folder_list()` でスコアリング。設定変更との微小な不整合は許容する設計判断
- **`PrebuiltIndex`**: ロック外で構築 → ロック内で `apply_prebuilt_index()` でスワップ。SearchEngine の構築コスト（Wave 1/2 の並列計算）をロック外に追い出す。**入口は `PrebuiltIndex::from_material` の 1 つである**——派生データの有無で建て方が分かれるのは `SearchEngine::from_material` の 1 か所に閉じており、呼び出し点は分岐を持たない。`PrebuiltIndex::new` は `#[cfg(test)]` ゆえ製品から呼べない
- **`PreparedHistorySave`**: ロック内で剪定・シリアライズ済み snapshot を取得 → ロック外で `save()`。process-wide の書き込み mutex と history path ごとの完了 sequence により、並行した古い snapshot が新しい `history.bin` を上書きしない。終了時の `prepare_history_flush` は、通常保存が prepare 済み・未書込の窓を回収するため `dirty_count` に関係なく最終 snapshot を生成する

新しい Engine メソッドを追加するとき、I/O やインデックス構築をロック内で行わないよう注意する。

## index.bin 書き込みの排他（INDEX_WRITE_LOCK）

`index.bin` を scan+save する経路は**すべて `INDEX_WRITE_LOCK`（`indexer.rs` の module-level `static Mutex<()>`）を経由する**。`BinFile::save` の tmp→rename は固定 tmp 名（`index.bin.tmp`）での原子的置換であり、単一書き手が前提。複数経路が同時に書くと tmp ファイルを食い合い破損する。

- 走査して書く経路（`rebuild_and_save` / `load_or_scan_with_stats` の cache-miss 枝）: `with_index_write_lock`（blocking）で取得
- 走査せずに書く経路（ロードの旧版枝からの形式昇格・`upgrade_legacy_cache_in`）: 同じく `with_index_write_lock`。読めた旧版の走査結果をそのまま現行版で書き直すだけで、索引の中身は変えない

**世代機構（読んだ時点の世代を控え、保存の直前に照合する）は #1023 で撤去した。** 守っていたのは「**内容を決めてから保存するまでの間に、別の書き手が新しい索引を書き終える**」窓であり、それを持つ書き手は背景再スキャンだけだった——22〜30 秒の走査をロックの**外**で終えてから、保存のためにロックを取りに行っていた。今の書き手 3 つのうち 2 つ（cache-miss 枝・`rebuild_and_save`）は走査から保存までを 1 回のロック取得で覆うので、この窓は構造的に開かない。

**例外は形式昇格である**——`index.bin` を読むのはロックの外で、保存だけがロックの内側にある。**その窓が今日開かないのは機構ではなく順序による**: 製品でこの経路を通る呼び出し元は `main` の起動段の 1 つだけで、もう一方の書き手（索引ビルドのスレッド）は `AppHandle` を要求するためその時点でまだ存在しない。**ロード後に走りうる書き手をこの経路へ足す日には、窓が戻る。** なお**プロセスをまたぐ同時起動は世代機構でも守れていなかった**——世代は `INDEX_WRITE_LOCK` と同じくプロセス大域の `static` であり、射程が同じだからである。
- `save_cache_sorted` 自身はロックを取らない（呼び出し側が保持する契約）。ロック取得済みのクロージャ内から呼ぶ。`save_cache_sorted` がロックを取ると自己デッドロックする
- **`index.bin` を書く新しい経路を追加するときは、必ず `with_index_write_lock` を経由させる**

## indexer.rs の索引更新の契機

**索引の中身が更新される契機は明示操作の 3 つだけである**（初回構築・`/s` による手動再構築・設定変更による再構築。`SPEC.md` §3.3）。キャッシュヒットの起動は `index.bin` を読むだけで、背景での走査は行わない。自動の背景再スキャンを間引くのではなく撤去した根拠と、そこで捨てた設計（`built_at` を使う間引き）の中身は `docs/adr/ADR-rescan-explicit-only.md` が正本。

**ただし「更新の契機」と「`index.bin` を書く契機」は同じではない。** 書く側には 4 つ目があり、それが下の形式昇格である——キャッシュヒットの起動でも、置かれているのが旧版なら書き戻す。**中身は変わらないので更新ではない**が、「読んで終わり」とも読まないこと。

- **`cache_load_ms` と `total_ms` の間に処理を足すときは、`LoadOrScanStats` に並ぶ項目を必ず作る**（正本はその struct の doc）。反復 6 で実際に踏んだ——ロード直後に居た全エントリ複製がどのフェーズ計測にも現れず、起動段の live ブロックの 1/3 を占めたまま見えなかった。残余は `tests/memory_footprint.rs` が毎回出す
- **旧版の `index.bin` を現行版へ書き戻すのは `load_cache_in` の旧版枝（`upgrade_legacy_cache_in`）の責務である。** かつては背景再スキャンが担っていたが、索引の中身が変わらない間は save の契機が来ず、そのユーザーの `index.bin` は旧版のまま何日でも残り、新形式の削減を**永久に受け取らない**問題を持っていた（2026-08-07 実測: v5 導入後の実運用点が v4 のまま残り、毎起動で `normalized_keys` 35.98 MiB を読んでは捨てていた。**症状は「遅い」だけで検索結果は正しいまま**ゆえ挙動テストでは捕まらない）。ロードのたびに必ず通る旧版枝へ移すことでこの経路への依存を断った。**「昇格をロード側に置いてはならない」の射程は v7 の枝だけである**——v7 は木を直読みするので `entries: Vec<AppEntry>` が存在せず、木から作り直すと反復 6 で消した 62.5 MiB の複製が復活する。v2〜v6 の枝は走査結果 `cache.entries` を手に持って `IndexTree::build` へ渡す 1 行の置換で済むため、複製は発生しない。検知器は `load_cache_upgrades_a_legacy_format_in_place` と `load_cache_does_not_rewrite_when_the_format_is_current` の対
- **`save_cache_sorted(_in)` を呼ぶ側は直前に `sort_entries_canonical` を通す契約である**（正本は同関数の doc）。親の解決が整列済みを前提に二分探索するため、崩すと木が平たくなりフルパスが `table` へ実体で戻る。**守るのは正しさではなくサイズである**——未整列でも `IndexTree::build` は別の親を返さず、取りこぼしが根になるだけなので検索結果は変わらない。検知器は `legacy_upgrade_sorts_before_saving_so_the_tree_stays_shared` 1 本で、**射程は昇格枝に限る**——3 つの書き手のうち入力の並びが自分の制御下に無いのはそこだけだからである（他の 2 つは自分で走査した結果を数行上で整列させる）
- **形式昇格は `built_at` を打ち直さない**（`BuiltAt::{Scanned, Carried}`）。走査していない書き手が現在時刻を打つと、設定アプリの最終構築日時が**最も索引が古い層**——旧版のまま放置していたユーザー——にだけ「たった今構築した」と嘘をつく。理由の正本は `BuiltAt` の doc、検知器は `upgrade_carries_the_built_at_it_read`（両方向）

## `scan_all` の重複排除

**`scan_all` の重複排除は根ごとに `check`/`record` を割り当てる**（`root_roles`）。積むのは「後続の根と重なる」根だけで、先行とだけ重なる根は照合のみでキーを積まない——木の走査は同じディレクトリを二度読まないので、1 回の走査の中で同じ正規化キーが二度現れる経路は**根が入れ子のとき以外に無い**（`dedup_scan_paths` は完全一致マージのみゆえ入れ子の根は表現可能であり、**素で消すことはできない**）。実運用点は最大の根 `C:\` が最後に来るため、その根ぶんの `String` 確保が消える（額は `PERFORMANCE.md`「採用: `scan_all` の `seen` を根ごとの役割（`check`/`record`）で条件づける」が正本）。**検知器は両方向を固定する**（`scan_all_dedups_when_roots_are_nested` / `scan_all_dedups_when_the_child_root_comes_first`）——役割は木の深さではなく**走査順**の関数なので、親が先・子が先で役割が入れ替わり、それでも重複排除が成立することを走査結果で見る。**「片方では捕まらない変異が在る」とは書かない**——2 根の治具でそれを示せる変異は見つかっておらず（役割の入れ替えも結線の破壊も両順序で対称に落ちる）、示せない必然を理由として書けば規範に反する。**退行の射程は 2 段に分かれる**: `root_roles` の役割割り当ての退行（例: 最大の根が積む側に回る）は `root_roles_over_the_real_shape_leave_the_largest_root_inert` が捕まえる——ここは CI で守られる。一方、`Dedup::accept` の照合枝で確保が復活する退行や `scan_all` の結線そのものの退行は、走査結果が同じままなので挙動テストでも `root_roles` の単体テストでも捕まらない。捕まえるのは `tests/memory_footprint.rs` の確保回数だけで、これは `#[ignore]` の手動計測ゆえ CI は守らない

## エントリ名の導出ルール

`indexer.rs` のスキャンでは:
- **ファイル**: `file_stem()` を `name` に使用（拡張子なし）。例: `firefox.lnk` → `name: "firefox"`
- **フォルダ**: `file_name()` を `name` に使用（そのまま）。例: `Projects/` → `name: "Projects"`

`folder.rs` のフォルダ内列挙では `file_name()` をそのまま使う（拡張子付き）。この違いは意図的で、フォルダ展開時にはファイル拡張子がフィルタリングの手がかりになるため。

**エントリ名導出は共通関数へ括り出さない**（issue #997）: #995 で生じた `index_tree.rs` の fixture は `0bb7b11` で利用対象と一緒に撤去済み。現行の導出は `indexer.rs` 内で拡張子照合・空名除外・再帰継続・重複排除の各処理に隣接し、共通関数はそれらを束ねないためインラインを維持する。別モジュールに実行可能な消費者が再び生じたら再検討する。

## Config のデシリアライズ経路

`Config::load()` はデシリアライズ後に `apply_migrations()` で後処理（レガシーフィールド移行・正規化・システムショートカットフォールバック）を実行する。**Config をデシリアライズする新しい経路**（インポート、テスト用ファクトリ等）を追加するときは、`apply_migrations()` の適用要否を明示的に判断する。迂回すると旧版データの移行漏れ（例: `paths.additional` の消失）が起きる。

### `Option<T>` フィールドを migration の「明示設定か否か」の sentinel に使う場合

`None` = TOML 未記載、`Some(v)` = 明示設定 として使う場合、`SearchConfig::default()` は **`None` を返すこと**。`Some(default_value)` を返すと、`[search]` セクション全体が TOML に存在しない場合でも serde が `SearchConfig::default()` を使うため `Some(v)` になり、`apply_migrations()` の `is_none()` チェックが常に false になって legacy 値の移行が起きなくなる。

- 正しいパターン: `Default` → `None`、使用時に `effective_*()` アクセサで `unwrap_or_else(default_fn)` する
- migration 後の「None を解消する」処理 (`get_or_insert_with`) は `apply_migrations()` の最後にまとめて実行する
- `reset_to_default()` でも `Config::default()` 後に `apply_migrations()` を呼び、None を解消してから保存する

## Gotcha（計測の罠）

**ここに貯めるのは「対象ではなく**計器の方が**壊れていた」事例だけである。** 同型を踏んだら 1 件足す。挙動の不変条件は上の各節が正本であり、ここへ写さない。

### 走査の進行・終端を I/O 読み取り回数で判定してはならない（使うのは CPU 時間とスレッド数）

`scan_all` のような長い走査を**プロセスの外から**測るときの話である（2026-08-09・#1001 で実測）。

- **PowerShell 7 の `Get-Process` は `ReadOperationCount` を持たない**（返るのは .NET の `System.Diagnostics.Process`。あの値は PS 5.1 の WMI 由来）。**空値が 0 として計算に混ざり**、「増分が閾値未満なら頭打ち」の判定が走査の途中で誤発火した——走っているのに「終わった」と読んだ。I/O カウンタが要るなら `Get-CimInstance Win32_Process`
- **取り直しても値はほぼ動かない。** `read_dir` はメタデータ操作で `OtherOperationCount` 側に立つため、31 万ディレクトリを歩いても読み取り回数は 16 のままだった
- 実際に効いたのは `TotalProcessorTime`（走査中は 1 コアを使い切るので増分 ≒ 経過秒）と `Threads.Count`（作業スレッドの消滅）で、**独立な 2 つの信号が同じ時刻を指したことが「その区間は再スキャンだった」の帰属の根拠**になった
- 一般形: **「計器が黙っている」と「対象が止まっている」は別である。** 全部 0・全部空の観測量は、まず計器の欠落を疑う

### 製品の入口を通るテストは、その経路へ計器を足した瞬間に実 `%APPDATA%\Snotra` を書き始める

**しかもテストは全部通ったままである**（判定が 1 つも変わらないため）。#1013 で当時の背景再スキャンに記録を足したところ、`Config::config_dir()` を内部で解決する入口を通る既存テスト 2 本が実 `rescan-log.jsonl` へ書き始めた（実測: 当該ファイルの全 36 行がテスト由来・実起動由来 0 行）。上限つきの記録なら、テスト実行が実運用の窓を食いつぶす。

- 直し方は **dir 注入の入口**（`load_cache_in` / `save_cache_sorted_in` のように `dir: &Path` を取る形）へ寄せること
- **`SNOTRA_CONFIG_DIR` で迂回してはならない**——プロセス大域の env であり、並列実行中の他テストの保存先まで動かす
- **検算は「フルテスト実行の前後でファイルの行数が変わらないこと」を実測する。** 緑であることは根拠にならない
- 同型の先例が「開発ルール」の `HistoryStore::load()` 禁止（#963）である——**あちらは実データを読む側、こちらは書く側**。読む側は結果が環境で揺れ、書く側は結果が揺れないまま実データを汚す
