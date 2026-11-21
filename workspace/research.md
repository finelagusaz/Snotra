# 調査 — issue #1400

## issue の要約

実 `index.bin` を読む検知器（corpus テスト）は、`index.bin` が読めないとき、理由を区別せずに
「実インデックスが無いためスキップします。」と出して緑になる。開発機の実 `index.bin` は v7 のまま
（2026-11-22 にヘッダーを `od` で確認: `INDX` + `07 00 00 00`、mtime 2026-11-20）。現行は
`INDEX_CACHE_VERSION = 10`（`snotra-core/src/indexer/cache.rs:49`）。v7〜v9 は設計上読まない
（`load_cache_in` の v6 分岐の直前のコメント）。そのため #1394 では、既定スイートの検知器が
何も走らずに緑だった。

ユーザーの裁定（2026-11-22・`AskUserQuestion`）:

- 理由を区別して出す（どの選択でも行う前提で提示）
- **コードより古い版で読めなかったときは失敗（panic）させる。コードより新しい版は理由つきでスキップする**
- 版上げのチェックリスト（`snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」）には足さない

## 関連するファイル・シンボル

- `snotra-core/src/indexer/cache.rs`
  - `load_cached_entries(scan, show_hidden_system) -> Option<Vec<AppEntry>>`（:367）。`#[doc(hidden)] pub`。
    `load_cache(hash, LegacyUpgrade::Skip)` を通す。呼び手は corpus テストの 2 helper だけ（`git grep` で確認）
  - `load_cache`（:896）: `Config::config_dir()?` → `load_cache_in`
  - `load_cache_in(dir, config_hash, upgrade) -> Option<LoadCacheResult>`（:1023）: `bf.load_bytes()?` →
    現行版 → v6 → v5 → v4 → v3 → v2 のチェーン。どの分岐でも `config_hash` 不一致は `None`。
    現行版は `IndexTree::from_parts` / `IndexMaterial::from_untrusted` の検証失敗でも `None`。
    **`None` の理由は返さない**
  - `load_cached_material`（:762）: 製品の走査しない再構築の入口。同じく `load_cache_in(.., Skip)` を通す。
    **この変更で挙動を変えてはならない**
  - `cache_bin_file_in(dir)`（:332）、`INDEX_MAGIC`（:41）
- `snotra-core/src/binfmt.rs`
  - `peek_version(bytes) -> Option<u32>`（:162）: ヘッダーの版だけを返す。doc が「版を発見する用途はこれ」と定める
  - `BinFile::load_bytes`（:53）: 無い・読めないときは `None`
- `snotra-core/src/search/tests/common.rs`
  - `read_real_config_read_only()`（:45）、`real_index_entries()`（:64）: 実 config を読むだけで読み、
    scan が空なら `None`、それ以外は `load_cached_entries` の結果
  - `real_scanned_entries()`（:88）の doc: **「呼び出し側でメッセージを書き分けないこと——自動スキップの理由が
    1 か所に在ることが…唯一の手がかり」**。こちらは helper が `println!` する
- `real_index_entries` の呼び手（7 本・すべて呼び手側で同じ `println!("実インデックスが無いためスキップします。")`）
  - 既定スイート（5 本）: `basic.rs` `utf32_haystack_matches_utf32_string_from_over_real_index`、
    `build.rs` `entry_view_shared_strings_match_derivation_over_real_index`、
    `path.rs` `index_tree_file_key_matches_normalize_file_name_key_over_real_index` /
    `path_store_cursor_matches_normalize_entry_key_over_real_index` /
    `key_tail_into_matches_cursor_key_tail_over_real_index`
  - `#[ignore]`（2 本）: `path.rs` `sorted_prefix_fast_path_changes_nothing_over_real_index` /
    `skipping_name_scoring_changes_nothing_over_real_index`
- `snotra-core/tests/path_query_cost.rs`
  - `load_real_index()`（:119）: `real_index_entries` と同じ処理の写し（統合テストは `pub(super)` を呼べない）
  - `derives_same_bytes_as_normalize_entry_key`（:75）: `#[ignore]` なし。呼び手側で同じ文言の `println!`
  - 同ファイルの `#[ignore]` 計測（`measure_path_query_frame_cost` ほか）は `Config::load()` +
    `load_or_scan_with_stats`（`LegacyUpgrade::Write`）を通し、`load_real_index` を通らない

## 再利用できる既存のパターン

- 版の発見は `binfmt::peek_version` の 1 つ（写しを作らない。同関数の doc）
- `memory_footprint.rs` の `report_cache_bytes`（:535 付近）: 読めないときに「この計測コードが読める版では
  読めなかった」と区別して出し、版を出す。版の一覧を書き写さない、現行版はリテラルでなく
  `INDEX_CACHE_VERSION` を読む、という作法
- dir 注入の入口（`load_cache_in` / `cache_byte_breakdown_in` の形）。テストは `temp_dir` +
  `GOLDEN_V7`（`cache/tests.rs:339`）などの凍結バイト列で書く
- 現行版より新しい版の fixture は、`try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION + 1, ..)` で作れる

## 技術的な制約

- `snotra-core/CLAUDE.md`「開発ルール」: core は UI 表示文字列を持たない。理由は型（enum）で返し、
  文言はテスト側で組み立てる
- 同「開発ルール」: テストから実 `%APPDATA%\Snotra` に触れない。新しい判定は dir 注入の入口でテストする
- `load_cache_in` のフォールバックチェーンを写して分類しない（版を足すたびに写しが古くなる）。
  分類は「`load_cache_in` が `None` を返した後に、ヘッダーの版を現行版と大小比較する」だけで足りる:
  - 版 < 現行: 古い版で読めなかった（v7〜v9 の設計上読まない版、または v2〜v6 で `config_hash` 不一致）。
    どちらも製品を起動すれば現行版で置き換わる → 失敗させる
  - 版 > 現行: コードより新しい製品が書いた → スキップ
  - 版 = 現行: `config_hash` 不一致か、検証失敗（壊れている）→ スキップ（裁定は「旧版だけ失敗」）
  - ヘッダーが読めない（短い・magic 違い）: スキップ
- 製品の経路（`load_cached_material`・`load_or_scan_with_stats`・`load_cache_with_stats`）は変えない
- CI（`.github/workflows/ci.yml:132` `cargo test -p snotra-core`）の runner には実 config が無いので、
  helper は config の段で `None` に落ち、失敗の分岐へは届かない

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1400.txt`。

- 壊1（採る・自分で裏取り）: 「古い版で落ちたら製品を起動すれば置き換わる」は偽。開発機で使っている製品
  `C:\toolbox\Snotra\snotra.exe` の mtime は 2026-10-08。v7 は 2026-08-08（4c96fef7）に入り、v8 は 2026-10-31
  （#1326）に入った。つまりインストール済みの製品は v7 を書くビルドで、起動しても v7 のままになる。
  v10 を含む最新リリースは v0.19.5（2026-11-07。`git show v0.19.5:` で `INDEX_CACHE_VERSION = 10` を確認）。
  直すには製品の更新が要る。さらに、版を上げる PR がマージされてからリリースされ、開発機の製品が更新されるまでの間、
  開発機の既定スイートは赤のままになる。**この期間が、裁定の時点で想定していた「起動するまで」より長い**ので、
  裁定をユーザーへ戻す
  - 起動経路そのものも無条件には書かない（起動は走査せず、背景の `rebuild_and_save` が完走して保存できたときだけ
    置き換わる）。帰結は同じ
- 壊2（採る）: `peek_version` は magic を見ない。分類の前に `INDEX_MAGIC` を照合する（計画に既に入れてある。
  テストの「別 magic」の行が守る）。v7〜v9 のヘッダーで本体が壊れたファイルも `Older` になるが、
  処置（現行版で置き換える）は同じなので区別しない
- 壊3（採る）: 「#1394 では計測が走らなかった可能性が高い」は推論にすぎない。範囲外のまま、断定を外す
- 壊せなかった項目: 実 `index.bin` は v7・config の scan は 5 件・`SNOTRA_CONFIG_DIR` は未設定で、
  落ちる理由は版の違い／呼び手の数（2 helper・7 本 + 1 本）／`load_cached_material` は経由しない／
  CI は config の段でスキップする／v2〜v6 の hash 不一致も `Older` に入る（副作用: scan を編集してまだ製品を
  起動していない開発機でも、旧版なら落ちる）
- ⚠️ v7 の書き手の断定（→ 上の `git log` で v7〜v8 の期間に exe の日付が入ることを確かめた）・別シェルの env
  （未確認のまま。結論への影響は env が設定されていれば読む場所が変わるだけ）

## 未解決の疑問

- issue は「`path_query_cost.rs` の運用点の計測が走らなかった」とする。ところが `#[ignore]` 計測は
  `load_or_scan_with_stats`（`Write`）を通るので、v7 なら cache-miss → 全走査 → v10 で書き戻し になるはずで、
  スキップにはならない。実 `index.bin` が今も v7（mtime 11-20）なのは、v7 を書くインストール済みの製品が
  後から上書きしたとも読めるので、走ったかどうかは判別できない。この変更の範囲（`load_cached_entries` を通る検知器）には影響しないので、
  範囲の外として扱う
- `#[ignore]` の 2 本（`path.rs`）も helper を通るので、旧版なら失敗する。手元で意図して走らせるものなので、
  失敗で困ることはない（実データが無いと何も測れない）
