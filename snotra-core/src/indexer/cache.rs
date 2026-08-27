//! `index.bin` の入出力——オンディスク形式（現行版と旧版へのフォールバック鎖）、キャッシュと
//! 走査の振り分け（[`load_or_scan_with_stats`]）、保存、旧版を現行版へ書き直す形式昇格。
//!
//! **書き込みは `INDEX_WRITE_LOCK` で単一書き手に直列化する**——`BinFile::save` の tmp→rename は
//! 固定 tmp 名での原子的置換ゆえ、複数経路が同時に書くと tmp を食い合って破損する。
//! **走査の契機は明示操作だけである**——初回構築・`/s` による手動再構築・設定変更による再構築の
//! 3 つで、キャッシュヒットの起動は読むだけで終わる（判断の記録は
//! `docs/adr/ADR-rescan-explicit-only.md`）。**ただし「更新の契機」と「`index.bin` を書く契機」は
//! 同じではない**——旧版を読んだ起動は中身を変えないまま現行版で書き直す。
//!
//! 版を足すときに揃える項目は `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」。
//! オンディスク内訳の計測は子モジュール [`breakdown`] が担う。

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::binfmt::{BinFile, try_deserialize_with_header};
use crate::config::{Config, ScanPath};
use crate::index_tree::{IndexTree, NameArena};
use crate::str_arena::{LowerFileColumn, LowerNameColumn};

use super::columns::{CachedLower, CachedMasks};
use super::scan::scan_all;
use super::{AppEntry, IndexMaterial, derive_columns, sort_entries_canonical};

// 計測専用: `index.bin` のオンディスク内訳（親の private なオンディスク構造体を直接読む）。
mod breakdown;
pub use breakdown::{CacheByteBreakdown, CacheByteRow, cache_byte_breakdown_in};

#[cfg(test)]
mod tests;

const INDEX_MAGIC: [u8; 4] = *b"INDX";
/// `index.bin` の現行フォーマット版。
///
/// 計測ハーネス（`tests/memory_footprint.rs`）が「読めた版が現行版か」を判定するために読む。
/// **版のリテラルを他所へ焼き込まないこと**——反復 8 で v6 へ上げたとき、ハーネスの注記だけ
/// が `5` のまま取り残され、「現行は v5。実運用点は v6 のまま」という**それ自体が矛盾した**
/// 文を出し続けた（現行が v5 なら v6 は存在しえない）。
#[doc(hidden)]
pub const INDEX_CACHE_VERSION: u32 = 7;

/// `load_or_scan_with_stats` の各フェーズ所要時間。
///
/// **`cache_load` と `total` の間に処理を足すときは、必ずここに並ぶ項目を作ること。**
/// 項目が無い処理は `total` にしか効かず、差を読む者がいなければ計測上は存在しないままに
/// なる（**規則の正本はここ**。反復 6 で実際に踏んだ経緯は
/// `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」が持つ）。
///
/// **全項目を丸めずに [`Duration`] で運ぶ**（#1027 で `total` だけ、#1178 で残りを移した）。
/// ミリ秒への丸めは表示境界（`src-tauri` の `startup.rs` の `to_ms`・`main.rs` の
/// `[index-load]` 行・`tests/memory_footprint.rs` のフェーズ内訳）でだけ行う。
#[derive(Debug, Clone, Copy)]
pub struct LoadOrScanStats {
    pub cache_hit: bool,
    pub hash: Duration,
    pub cache_load: Duration,
    /// `index.bin` をバイト列として読む時間。**`cache_load` の内数である**
    /// （他の項目と違い、フェーズの和には足さない）。
    ///
    /// `cache_load` は「読む」と「deserialize する」の 2 つを 1 つの数にしており、
    /// **両者はオンディスク形式の変更に対して逆向きに振る舞う**——読むバイトを減らせば前者は
    /// 減るが、形式を圧縮すれば後者は増えうる。分けずに測ると、どちらが効いたのか原理的に
    /// 区別できない。cache-miss の枝では [`Duration::ZERO`]（読む対象が無い）。
    pub cache_read: Duration,
    pub scan: Duration,
    pub sort: Duration,
    /// キャッシュ保存にかかった時間。**枝によって和への足し方が違う。**
    ///
    /// cache-miss 枝（scan して save する）では `scan` / `sort` に続く独立フェーズであり、
    /// フェーズの和に足す。**cache-hit 枝で旧版昇格（`upgrade_legacy_cache_in`）が走った
    /// 場合はここに昇格の save 時間が入るが、`cache_load` の内数である**
    /// （`cache_read` と同じ扱い——足すと二重計上になり、`total` から差し引く残余計算が
    /// 負に振れる）。save は `load_cache_in` 呼び出しの
    /// 内側（`LegacyUpgrade::Write` で旧版を読んだとき）で起きるため、`cache_load` の外に
    /// 出しようがない。cache-hit かつ現行版を読んだときは [`Duration::ZERO`]。
    /// **この [`Duration::ZERO`] を「昇格が走らなかった」の判定に使ってはならない**——区別が
    /// 要る読み手は `LoadCacheResult::upgrade_save` の variant を見る（#1054 / #1063）。
    /// 判定を値に負わせないというこの規約は、分解能が ms から ns へ上がっても変わらない
    /// （#1178 で型が変わり、`Instant::elapsed` が 0 を返す確率は実質消えたが、
    /// **「昇格が走ったか」を表しているのは依然として variant のほうである**）。
    ///
    /// **`load_or_scan_with_stats`（この struct の生成元）は常に `LegacyUpgrade::Write` で
    /// 呼ぶ**——`LegacyUpgrade::Skip` は corpus テストの入口（`load_cached_entries`）専用で、
    /// `LoadOrScanStats` を生成しない。ゆえにここでは `Skip` は考慮しなくてよい
    /// （`LoadCacheResult::upgrade_save` の doc は `Skip` 経由の `None` も併記しているが、
    /// それは `LoadCacheResult` 自体が両方の呼び出し元を持つため）。
    pub cache_save: Duration,
    /// `load_or_scan_with_stats` 全体の所要時間。**丸めずに `Duration` で運ぶ。**
    ///
    /// 起動計器（`src-tauri` の `startup.rs`）はこの値と外側の区間の差を
    /// `index_load_unattributed_ms` として出す。その引き算は両辺を同じ関数（`to_ms`）へ
    /// 通すので、**ここが丸めた値を入れると、外側と丸め方が食い違って差が負へ振れうる**（#1027）。
    ///
    /// **この要求を守る検査は見つかっていない。** 生成を四捨五入へ変える変異を入れて
    /// `docs/build-commands.md` のカテゴリ A〜F を一通り（fmt・clippy `--all-targets`・両 crate の
    /// `cargo test`・`cargo doc`・`npm test`・`governance:check`・Pester・`smoke:startup`・
    /// `smoke:egui`）と `bench-startup.ps1` を回したが、**どれも緑だった**（2026-08-25 実測）。
    ///
    /// 届かない理由は層ごとに違う。**テストは構造的に見ない**——`index_load_unattributed_ms` を
    /// 固定するテストは `Timeline` を直に組むので、この struct の生成側を一度も通らない
    /// （ゆえに「あちらへ検知器をもう 1 本足す」では塞がらない）。**`bench-startup.ps1` の
    /// `>= 0` が見るのは症状（負値）**であり、丸めが足す高々 1 ms が余裕（**機体依存**。
    /// 実測した構成では 3〜4 ms）に収まると掛からない。**切り捨てで丸める形に至っては出力が
    /// 変わらない**（`floor ∘ floor = floor`。`to_ms` が切り捨てであることは
    /// `to_ms_truncates_toward_zero` が固定している）ので、余裕がいくら小さくても発火しない。
    /// **ゆえにこの一行は規約でしか守られていない。**
    ///
    /// **これは「この値がミリ秒へ落ちる場所は 1 つだ」という主張ではない。** 表示のために
    /// `as_millis()` する消費者は別に在る（現在の一覧は `git grep "\.total\b"` が出す）。
    /// それらは各自の表示境界であって、上の引き算とは無関係である。
    pub total: Duration,
}

/// `load_or_scan_with_stats` の戻り値。
pub struct LoadOrScanResult {
    /// ロード or スキャンされた索引の材料（木と派生データの組）。
    ///
    /// **`Vec<AppEntry>` ではない**——v7 は `target_path` をディスクに持たないので、
    /// 実体へ戻すと削った 312,691 回の確保がその場で復活する。
    ///
    /// **組のまま渡す。** ほどいた 2 値を持ち回せると、木を伸ばしてマスクを追記し忘れる形が書けるようになる（正本は [`IndexMaterial`] の doc）。
    pub material: IndexMaterial,
    /// キャッシュが無く（または stale で）フルスキャンが走った場合 true。
    pub cache_changed: bool,
    /// 各フェーズの所要時間。
    pub stats: LoadOrScanStats,
}

/// v5 フォーマット: ビットマスクに加えて lower_names / lower_file_names を保存。
/// 起動時に SearchEngine の Wave 1（to_lower_folded）を完全スキップできる。
///
/// **v4 との差は `normalized_keys` を持たないことだけである**（実測 35.56 MiB / 312,377 件）。
/// `target_path` から `normalize_entry_key_into` で導出できる純粋な派生であり、検索時に
/// 必要な候補についてだけ詰め直す形へ移した（`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）。
///
/// **owned/borrowed を単一 struct に統合する（`Cow<'a, [T]>`）**。save は `Cow::Borrowed` で
/// `entries` の全件 clone を避けてシリアライズし、load は `Cow::Owned` で deserialize する
/// （`IndexCache<'static>`）。単一 struct ゆえ「owned 版と borrowed 版でフィールド順がズレて
/// `index.bin` を無言破損する」footgun は型として起こり得ない。`Cow<[T]>` は Borrowed/Owned とも
/// 内側スライスの `serialize_seq` に委譲し `Vec<T>`/`&[T]` とバイト列が一致するため、
/// Cow 化そのものはバイト形式を変えない。形式の絶対安定は
/// `index_cache_on_disk_format_is_stable`（golden bytes）でガードする。
#[derive(Serialize, Deserialize)]
struct IndexCache<'a> {
    built_at: u64,
    /// 表示名（`AppEntry.name`）。
    ///
    /// **型は `Vec<String>` ではなく [`NameArena`] だが、線上のバイト列は変わっていない**
    /// ——アリーナは `seq of str` として読み書きし、要素ごとの `String` を作らずに 1 本の
    /// バッファへ流し込む（正本は [`NameArena`] の doc）。ゆえにこの変更は
    /// `INDEX_CACHE_VERSION` のバンプを伴わず、旧版フォールバックも増えない。
    /// **検知器は `index_tree` 側の `arena_wire_format_is_identical_to_vec_of_string` である**
    /// ——下の golden は名前の形を混ぜて持たないので、この一致だけを守る役には立たない
    /// （射程は [`NameArena`] の doc）。
    names: Cow<'a, NameArena>,
    is_folder: Cow<'a, [bool]>,
    /// 木の親（`crate::index_tree::TreeNodes::parent_of`）。
    parent: Cow<'a, [u32]>,
    /// 木の `table` 添字（`crate::index_tree::TreeNodes::aux_of`）。
    aux: Cow<'a, [u32]>,
    /// 拡張子と、親を持たないエントリのフルパスの intern 表。
    table: Cow<'a, [String]>,
    /// 保存時に測った整列の旗。**5 列と違い、読む側は検証せずそのまま信じる**——
    /// 理由と被害範囲は [`crate::index_tree::IndexTree::from_parts`] の doc に書いてある
    /// （壊れた値の帰結は同スコアの tie-break の順序に閉じる）。
    sorted_by_path: bool,
    config_hash: u64,
    char_masks: Cow<'a, [u64]>,
    file_name_char_masks: Cow<'a, [u64]>,
    /// `None` = `names[i]` とバイト一致（実データで 86.6%）。
    ///
    /// **型は `[Option<String>]` ではなく [`LowerNameColumn`] だが、線上のバイト列は
    /// 変わっていない**——`names` が [`NameArena`] へ移ったのと同じ理屈で、要素ごとの
    /// `String` を作らずに 1 本のバッファへ流し込む（正本は `crate::str_arena` の doc）。
    /// **検知器は `str_arena` 側の `lower_name_column_wire_format_is_identical_to_vec_of_option_string`
    /// である**——下の golden は名前の形を混ぜて持たないので、この一致を守る役には立たない。
    lower_names: Cow<'a, LowerNameColumn>,
    /// 3 状態（`LowerFileName`）。「無い」と「`lower_name` と同一」を別の値で表す。
    ///
    /// 線上表現は `Vec<LowerFileName>` のままである（`lower_names` と同じ理屈。検知器は
    /// `lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name`）。
    lower_file_names: Cow<'a, LowerFileColumn>,
}

/// v6 フォールバック用スキーマ（`target_path` を全件そのまま持つ旧形式）。
///
/// **v7 との差は `target_path` の表現だけである。** v6 は 312,691 件のフルパスを実体で持ち
/// （実測 36.01 MiB・ディスクの 70%）、v7 は木の親と拡張子 id に置き換えて持たない。
/// 読み込みはどちらも成功し、**違うのは確保の回数**——v6 を読むと `String` を 312,691 個
/// 余分に作り、`PathStore` へ組み替えた `assemble` が即座に捨てる。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IndexCacheV6 {
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    /// **v7 と線上表現が同一なので、同じ列の型で読む。** ここを `Vec<Option<String>>` の
    /// ままにすると、旧版枝でだけ per-entry の `String` が 41,994 個復活し、しかも
    /// `CachedLower::Collapsed` へ渡すために詰め替えが要る（正本は `crate::str_arena` の doc）。
    lower_names: LowerNameColumn,
    lower_file_names: LowerFileColumn,
}

/// v5 フォールバック用スキーマ（派生文字列を全件そのまま持つ旧形式）。
///
/// **v6 との差は `lower_names` / `lower_file_names` の表現だけである。** v5 は全 312,690 件を
/// 実体で持ち（実測 21.63 MiB）、v6 は共有を測って潰した形で持つ（1.90 MiB）。読み込みは
/// どちらも成功し、**違うのは確保の回数**——v5 を読むと 625,380 個の `String` を作り、
/// うち 527,000 個は `assemble` が即座に捨てる。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IndexCacheV5 {
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
}

/// v4 フォールバック用スキーマ（末尾に `normalized_keys` を持つ旧形式）。
///
/// **読むだけで捨てる。** 導出可能な派生を持たない形へ移したため、v4 バイト列から
/// 復元するのは v5 と同じ 4 本（マスク 2 本 + lower 2 本）である。**それらは v4 にも
/// 揃っているので Wave 1 はスキップされたまま**で、v4 ユーザーの初回起動は遅くならない。
/// `Serialize` は**テストのときだけ**derive する。製品が v4 を書く経路はもう無い
/// （読むだけ・書くのは常に現行版）が、「中身は同じだが版だけ古い」状況を作る治具には要る。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IndexCacheV4 {
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
    #[allow(dead_code)]
    normalized_keys: Vec<String>,
}

/// v3 フォールバック用スキーマ（ビットマスクのみ、lower names なし）。
#[derive(Serialize, Deserialize)]
struct IndexCacheV3 {
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
}

/// v2 フォールバック用スキーマ（ビットマスクフィールドなし）。
/// v2 キャッシュをヒットした場合はマスクなし（None）で返し、
/// SearchEngine::new() が通常通りマスクを計算する。
#[derive(Serialize, Deserialize)]
struct IndexCacheV2 {
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
}

/// `index.bin` の有効性を判定する鍵。**走査対象そのものの同一性だけを表す**（入力の内訳と、
/// `include_path_env` / `migemo_enabled` を含めない理由は [`scan_identity_hash`] の doc）。
///
/// **この値は `index.bin` に焼き込まれ、次の起動が同じ計算で照合する**（[`load_cache_in`] の
/// 各枝の `config_hash != config_hash` 判定）。ゆえに**入力・順序・ハッシュ関数のどれを変えても
/// 全ユーザーのキャッシュが一斉に無効になる**——不一致は破損ではなく「設定が変わった」と
/// 読まれるので、次の起動は cache-miss 枝へ落ちて 22〜30 秒の全走査を払う（索引の更新は
/// 明示操作だけという設計上、これが自動で走る唯一の場所である）。
///
/// **[`DefaultHasher`] の出力は Rust のリリース間で安定しない**（std の契約——値ではなく
/// アルゴリズムが未規定である）。ツールチェインを上げた版を配ると、設定を何も変えていない
/// ユーザーが一度だけ全走査を払いうる。**症状は「その日の起動だけ遅い」で、検索結果は
/// 正しいまま**ゆえ挙動テストでは捕まらない。安定を要求するなら std ではないハッシュへ
/// 移すことになり、それ自体が同じ一斉無効化を 1 回払う。
fn compute_config_hash(scan: &[ScanPath], show_hidden_system: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    for sp in scan {
        sp.path.hash(&mut hasher);
        sp.extensions.hash(&mut hasher);
        sp.include_folders.hash(&mut hasher);
    }
    show_hidden_system.hash(&mut hasher);
    hasher.finish()
}

/// `compute_config_hash`（private）を crate 外から呼ぶための薄い入口。**走査対象そのものの
/// 同一性だけを表す**——入力は `scan` と `show_hidden_system` の 2 つだけで、
/// `include_path_env` / `migemo_enabled` は含まない（それらは走査ではなく索引の**構築**を
/// 左右する入力であり、`engine::IndexInputs` が別に持つ）。
///
/// **呼び出し元（`src-tauri` の `apply_rescanned_index`）は #1001 で撤去済みで、
/// この関数は現在どこからも呼ばれない**——背景再スキャンが返した材料について、
/// それを走査した時点の対象と差し替え時点の現在 config が同じかをこの入口経由で照合して
/// いた。**内部を丸ごと公開する代わりに、比較に要る計算だけをここへ閉じ込めてある**という
/// 設計は、消費者が戻る日のために残す。
pub fn scan_identity_hash(scan: &[ScanPath], show_hidden_system: bool) -> u64 {
    compute_config_hash(scan, show_hidden_system)
}

fn cache_bin_file_in(dir: &Path) -> BinFile {
    BinFile::new_in(dir, INDEX_MAGIC, INDEX_CACHE_VERSION, "index.bin")
}

/// `index.bin` が名乗る最終構築時刻（UNIX 秒）を読む。読めなければ `None`。
///
/// **索引本体を読まない。** 設定アプリがこれを呼ぶので、17 MiB の確保を持ち込まない。
///
/// **`built_at` が全版で先頭フィールドであることは、観測された性質であって契約ではない**
/// （v2〜v7 の 6 版で確認した）。新しい版を足すときも先頭へ置くこと——この依存は
/// `index_cache_on_disk_format_is_stable` の assertion 1 本が固定している。
pub fn index_built_at_in(dir: &Path) -> Option<u64> {
    // u64 の postcard varint は最大 10 バイト。
    cache_bin_file_in(dir).peek_first_field::<u64>(10)
}

/// `index.bin` に載っているときだけエントリを返す（**走査は絶対にしない**）。実データを
/// corpus として使うテスト専用。
///
/// **`load_or_scan_with_stats` で代用してはならない。** あちらの cache-miss の枝は全走査を
/// 走らせたうえで `index.bin` を書く。`INDEX_WRITE_LOCK` はプロセス内の `static` ゆえ、
/// **テストバイナリが複数プロセスに分かれると排他が効かず**、固定 tmp 名（`index.bin.tmp`）の
/// 食い合いでキャッシュが壊れうる（`snotra-core/CLAUDE.md`「index.bin 書き込みの排他」）。
/// 開発者の `cargo test` が黙って C ドライブを全走査する副作用も同時に消える。
///
/// **既定のスイートで実データを読むテストはすべてここを通すこと**（`search/tests/path.rs` の
/// corpus 2 件と `tests/path_query_cost.rs` の `derives_same_bytes_as_normalize_entry_key`）。
/// `#[ignore]` の計測ハーネスのうち `cached_masks` を要するものだけは
/// [`load_or_scan_with_stats`] のままでよい——手元で 1 つずつ意図して走らせるものであり、
/// 実運用の起動経路を測ることが目的だからである。
///
/// **返すのは木を実体へ戻したものであり、ディスクに在った文字列ではない**（v7 は
/// `target_path` を持たない）。ゆえに「組み直しが原文と一致するか」をこの返り値と
/// 突き合わせても、**組み直しの結果どうしを比べることになる**。その照合の接地は
/// `index_tree_raw_matches_frozen_v6_specimen`（旧形式の凍結バイト列が唯一の原文）が持つ。
#[doc(hidden)]
pub fn load_cached_entries(scan: &[ScanPath], show_hidden_system: bool) -> Option<Vec<AppEntry>> {
    let hash = compute_config_hash(scan, show_hidden_system);
    // **実体化の規則は `IndexTree::materialize` の 1 つである。** ここに写しを置いていた
    // ときは、木 →`Vec<AppEntry>` の規則が 2 部出荷されていた——片方に触れば corpus テストは
    // 製品が決して見ないデータを検証することになる（`index_tree.rs` の `//!` が「辿る規則は
    // 1 つ」と定めているのと同じ理屈で、戻す規則も 1 つでなければならない）。
    //
    // **`LegacyUpgrade::Skip` を渡す。** ここは開発者の実 `%APPDATA%\Snotra\index.bin` を
    // 読む corpus テストの入口であり（`search/tests/common.rs`）、`Write` にすると
    // テストを走らせるだけで実データを書き換える（`LegacyUpgrade` の doc）。
    Some(
        load_cache(hash, LegacyUpgrade::Skip)?
            .material
            .tree()
            .materialize(),
    )
}

/// 走査して正準の並びへ整列するまで（保存はしない）と、その 2 段の所要時間。
struct Scanned {
    entries: Vec<AppEntry>,
    scan: Duration,
    sort: Duration,
}

/// 全走査して [`sort_entries_canonical`] を通し、2 段を測って返す。
///
/// **保存する枝と保存しない枝（`Config::config_dir` が引けないとき）が同じここを通る。**
/// 書き起こすと計器が 2 部出荷になり、片方だけが段を足したときに `scan` / `sort` の
/// 意味が枝ごとにずれる——**どちらの枝を測ったのかは `LoadOrScanStats` の値からは
/// 区別できない**ので、ずれても数字はもっともらしいまま残る。
///
/// **`INDEX_WRITE_LOCK` は取らない。** 走査は共有資源に触れないが、保存する枝は
/// 「走査から保存までを 1 回のロック取得で覆う」ことに依存している（→
/// [`upgrade_legacy_cache_in`] だけがその例外である理由は `LoadCacheResult::upgrade_save`
/// の doc）。ゆえにロックの範囲は呼び出し側が決める。
fn scan_and_sort_timed(scan: &[ScanPath], show_hidden_system: bool) -> Scanned {
    let scan_started = Instant::now();
    let mut entries = scan_all(scan, show_hidden_system);
    let scan_took = scan_started.elapsed();

    let sort_started = Instant::now();
    sort_entries_canonical(&mut entries);
    let sort_took = sort_started.elapsed();

    Scanned {
        entries,
        // **shorthand を使わない。** `scan` は同スコープの `scan: &[ScanPath]` 引数の名前で
        // あり、フィールド名と衝突する（#1178）。
        scan: scan_took,
        sort: sort_took,
    }
}

/// `load_or_scan_with_stats` と同じ手順を `dir` 注入で行う（統合テスト用）。
///
/// **製品の入口（`Config::config_dir()` を解決する側）でテストを書かないこと。**
/// 実 `%APPDATA%\Snotra` を読み書きし、テスト実行が実運用のデータを動かす（#1013 の Gotcha）。
///
/// **「`load_or_scan` と同じで、ただし〜」と書いてはならない**（かつてそう書いていた）。その関数は #984 で削除され、この doc だけが実在しない名前を基準に自分を説明する状態になっていた。
fn load_or_scan_with_stats_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
) -> LoadOrScanResult {
    let total_started = Instant::now();

    let hash_started = Instant::now();
    let current_hash = compute_config_hash(scan, show_hidden_system);
    let hash_took = hash_started.elapsed();

    let cache_load_started = Instant::now();
    // **キャッシュが読めたらそこで終わりである。** 走査は明示操作の契機でしか走らない
    // （`docs/adr/ADR-rescan-explicit-only.md`）。
    if let Some(result) = load_cache_in(dir, current_hash, LegacyUpgrade::Write) {
        let cache_load_took = cache_load_started.elapsed();
        let stats = LoadOrScanStats {
            cache_hit: true,
            hash: hash_took,
            cache_load: cache_load_took,
            cache_read: result.read,
            scan: Duration::ZERO,
            sort: Duration::ZERO,
            // 昇格が走らなかった枝は `None` ゆえ `ZERO`（`LoadCacheResult::upgrade_save` の doc）。
            // **この値は計器であって、昇格の有無の判定には使えない**——判定を負っているのは
            // 向こうの variant であり、こちらは時間を運ぶだけである（#1054 / #1063）。
            // `cache_load` の内数——フェーズの和には足さない
            // （`LoadOrScanStats::cache_save` の doc）。
            cache_save: result.upgrade_save.unwrap_or(Duration::ZERO),
            total: total_started.elapsed(),
        };
        return LoadOrScanResult {
            material: result.material,
            cache_changed: false,
            stats,
        };
    }
    let cache_load_took = cache_load_started.elapsed();

    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 別ビルドとの index.bin 同時書き込みを防ぐ。
    // フェーズ計測はクロージャの戻り値として持ち出す。
    let (material, scan_took, sort_took, cache_save_took) = with_index_write_lock(|| {
        // **shorthand を使わない**（`scan` は引数名と衝突する・#1178）。
        let Scanned {
            entries,
            scan: scan_took,
            sort: sort_took,
        } = scan_and_sort_timed(scan, show_hidden_system);

        let cache_save_started = Instant::now();
        // **保存が返す木と派生データをそのまま使う。** 走査結果を保存のために建て直させると、
        // 同じ木を 2 回建てることになる（親解決は実測 23 ms）。派生データも同じ理屈で、
        // 保存側が計算して書いたものをここで受け取らないと、下流が全件を実体化してから
        // 建て直すことになる。
        let (tree, masks) = save_cache_sorted_in(dir, entries, current_hash, BuiltAt::Scanned);
        let material = IndexMaterial::derived(tree, masks);
        let cache_save_took = cache_save_started.elapsed();

        (material, scan_took, sort_took, cache_save_took)
    });

    let stats = LoadOrScanStats {
        cache_hit: false,
        hash: hash_took,
        cache_load: cache_load_took,
        // cache-miss の枝は `index.bin` を読み切れていない（不在・stale・破損のいずれか）。
        cache_read: Duration::ZERO,
        scan: scan_took,
        sort: sort_took,
        cache_save: cache_save_took,
        total: total_started.elapsed(),
    };

    LoadOrScanResult {
        // **cache-miss でも派生データを持って返る。** 保存側が `index.bin` へ書いたのと同じ 4 本であり、キャッシュヒット時に `load_cache_in` が返すものと**表現まで同じ**である（どちらも `collapse_lower_pair` = `measure_derived_sharing` を通した `Collapsed`）。ゆえに `cache_changed` は「どのコンストラクタが選ばれるか」を決めない。
        material,
        cache_changed: true,
        stats,
    }
}

/// キャッシュを読む、無ければ全走査して保存する。返すのは [`LoadOrScanResult`]——索引の材料（[`IndexMaterial`]）とフェーズ計測である。
///
/// 保存先（`Config::config_dir()`）を解決してから [`load_or_scan_with_stats_in`] へ委譲する薄い
/// 包みである。解決できない環境では保存できないが索引は建てられる——`save_cache_sorted` の
/// `None` 枝と同じ方針で、走査はして `index.bin` へは書かない。
///
/// **cache-hit でも書きうる。** 常に `LegacyUpgrade::Write` で読むので、置かれているのが
/// 旧版なら**その場で現行版へ書き戻す**（`upgrade_legacy_cache_in`）。ゆえに `#[ignore]` の
/// 計測ハーネスがこれを呼ぶと、**1 回目の実行が開発者の実 `index.bin` を現行版へ変える**
/// ——旧版のロードを測りたければ**先に退避すること**。2 回目以降は昇格後の姿しか測れず、
/// しかも出力は成功時とまったく同じに見える。実データを**書き換えずに**読む口は
/// [`load_cached_entries`]（`LegacyUpgrade::Skip`）で、既定のスイートはそちらを通る。
pub fn load_or_scan_with_stats(scan: &[ScanPath], show_hidden_system: bool) -> LoadOrScanResult {
    match Config::config_dir() {
        Some(dir) => load_or_scan_with_stats_in(&dir, scan, show_hidden_system),
        None => {
            // **ここだけ `with_index_write_lock` を経由しない。** 保存先が無いので
            // `index.bin` を書かず、保護すべき共有資源（tmp→rename の書き込み対象）が
            // そもそも存在しない——「index.bin を書く経路は排他を経由する」契約
            // （→「index.bin 書き込みの排他」）が対象を持たないだけで、免除ではない。
            let total_started = Instant::now();

            // **shorthand を使わない**（`scan` は引数名と衝突する・#1178）。
            let Scanned {
                entries,
                scan: scan_took,
                sort: sort_took,
            } = scan_and_sort_timed(scan, show_hidden_system);

            LoadOrScanResult {
                material: IndexMaterial::from_tree(IndexTree::build(entries)),
                cache_changed: true,
                stats: LoadOrScanStats {
                    cache_hit: false,
                    // **照合する相手が居ないので計算しない。** `config_hash` は `index.bin` へ
                    // 焼き込んで次の起動と突き合わせるための値であり、書かない枝では
                    // 消費者が居ない（かつては捨てる前提で計算し、この項目を埋めていた）。
                    hash: Duration::ZERO,
                    cache_load: Duration::ZERO,
                    cache_read: Duration::ZERO,
                    scan: scan_took,
                    sort: sort_took,
                    cache_save: Duration::ZERO,
                    total: total_started.elapsed(),
                },
            }
        }
    }
}

/// `index.bin` へ書く `built_at` の出どころ。
///
/// **「走査した時刻」を名乗る値なので、走査していない書き手が現在時刻を打ってはならない。**
/// 形式昇格（[`upgrade_legacy_cache_in`]）は旧版のバイト列を現行版へ詰め替えるだけで
/// 走査を伴わないため、ここで現在時刻を打つと**索引の中身は何日も前のまま、名乗る時刻だけが
/// 今**になる。表示（設定アプリの最終構築日時・[`index_built_at_in`]）はそれを唯一の手がかりに
/// 「再構築が要ることに気づく」ためにあるので、嘘をつく相手は**最も索引が古い層**——旧版のまま
/// 放置していたユーザー——に限られる。
///
/// **引数にしてあるのは、書き手に選ばせるためではなく選ばざるを得なくするためである。**
/// 既定を「現在時刻」にすると、走査しない書き手を足した日に何も書かなくても通ってしまう。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltAt {
    /// いま走査した結果を書く。
    Scanned,
    /// 走査していない（形式昇格）。読めた値をそのまま持ち越す。
    Carried(u64),
}

impl BuiltAt {
    fn resolve(self) -> u64 {
        match self {
            BuiltAt::Scanned => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            BuiltAt::Carried(secs) => secs,
        }
    }
}

/// エントリを木へ組み替えて保存し、**その木と、書いたばかりの派生データを返す**。
///
/// **`&[AppEntry]` を借りる形にしてはならない。** v7 が書くのは木であり、木を建てる段は
/// `target_path` の `String` を move で吸い上げる。借りる形にすると保存のたびに全件を
/// clone することになり、削ったはずの 36.01 MiB が保存経路で復活する。返り値にしてあるのは、
/// 呼び出し側がそのまま索引の材料に使えるようにするためである（`rebuild_and_save` と
/// cache-miss の枝が実際にそうする）。
///
/// **派生データを持たない材料を返す枝がある。** `config_dir` が引けないときは `index.bin` を書かないので派生データも計算しない——常に計算する形にすると、誰も読まない列を全件ぶん組み立てることになる。**この分岐がこの性質の正本である**（数え上げは他へ書かない）。
fn save_cache_sorted(entries: Vec<AppEntry>, config_hash: u64) -> IndexMaterial {
    let Some(dir) = Config::config_dir() else {
        return IndexMaterial::from_tree(IndexTree::build(entries));
    };
    let (tree, masks) = save_cache_sorted_in(&dir, entries, config_hash, BuiltAt::Scanned);
    IndexMaterial::derived(tree, masks)
}

/// `save_cache_sorted` と同じ保存処理を `dir` 注入で行う（統合テスト用、issue #429）。
///
/// **`INDEX_WRITE_LOCK` は取らない**（`save_cache_sorted` と同じ契約で、呼び出し側が保持する）。
/// この契約は型に無いので、**呼び出し元をこのモジュールの中に閉じておくこと**が唯一の担保で
/// ある（→「index.bin 書き込みの排他」）。導出だけを要する検知器は [`derive_columns`] を呼ぶ。
///
/// **書いた 4 本をそのまま返す。** かつては書いた直後に捨てており、cache-miss の枝は
/// `new_from_tree` が木を実体化して Wave 1/2 を建て直していた——**計算したものを捨ててから、
/// 同じものを作り直していた**（実測は `PERFORMANCE.md`「採用: 保存が返した派生データを cache-miss がそのまま使う」）。
///
/// **書き込みの失敗は返り値に影響しない。** 返すのは今メモリに在る木と、その木に対して導出
/// した派生データであり、両者の整合はディスクへ届いたかとは独立である。書けなければ次回が
/// cache-miss になるだけで、この起動の索引は正しい。
fn save_cache_sorted_in(
    dir: &Path,
    entries: Vec<AppEntry>,
    config_hash: u64,
    built_at: BuiltAt,
) -> (IndexTree, CachedMasks) {
    let bf = cache_bin_file_in(dir);
    let derived = derive_columns(entries);

    // Cow::Borrowed で木の列と派生 Vec の全件 clone を避ける。
    // 出力バイト列は Owned 版と同一（golden テストで保証）。
    let derived_cols = derived.tree.columns();
    let cache = IndexCache {
        built_at: built_at.resolve(),
        names: Cow::Borrowed(derived_cols.names),
        is_folder: Cow::Borrowed(derived_cols.is_folder),
        parent: Cow::Borrowed(derived_cols.parent),
        aux: Cow::Borrowed(derived_cols.aux),
        table: Cow::Borrowed(derived_cols.table),
        sorted_by_path: derived_cols.sorted_prefix_len == derived_cols.names.len(),
        config_hash,
        char_masks: Cow::Borrowed(&derived.char_masks),
        file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
        lower_names: Cow::Borrowed(&derived.lower_names),
        lower_file_names: Cow::Borrowed(&derived.lower_file_names),
    };
    if !bf.save(&cache) {
        eprintln!("[indexer] failed to save {}", bf.path().display());
    }
    // **畳むのは書き終えた後である**（借用の順序・[`DerivedColumns::into_cached_masks`] の doc）。
    // `clone` は挟まない——上の `Cow::Borrowed` の借用は `bf.save` を最後に終わる（NLL）。
    derived.into_cached_masks()
}

/// Force rebuild: scan and save cache, regardless of existing cache.
/// Called from settings dialog (Phase 5).
///
/// **書いた派生データをそのまま返す。** かつては捨てており、呼び出し側は木しか受け取れず Wave 1/2 を建て直していた——**計算したものを捨ててから、同じものを作り直していた**（額は `PERFORMANCE.md`「採用: `PrebuiltIndex` を `CachedMasks` 込みで建てる」）。
///
/// **派生データを持つかは [`IndexMaterial`] の内側の話である**（保存先が引けない枝では計算しない）。**持たない場合を数え上げてはならない**——正本は `save_cache_sorted` の分岐である。
///
/// PATH エントリをマージするなら [`IndexMaterial::extend_with_path_entries`] を通すこと。
pub fn rebuild_and_save(scan: &[ScanPath], show_hidden_system: bool) -> IndexMaterial {
    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 別の書き手との index.bin 同時書き込みを防ぐ。
    with_index_write_lock(|| {
        let mut entries = scan_all(scan, show_hidden_system);
        sort_entries_canonical(&mut entries);
        let config_hash = compute_config_hash(scan, show_hidden_system);
        save_cache_sorted(entries, config_hash)
    })
}

/// キャッシュ読み込み結果。
struct LoadCacheResult {
    /// 索引の材料。**v7 は木をディスクから直接読み、旧版は `target_path` から建て直す。**
    ///
    /// **通る構築子は `upgrade`（[`LegacyUpgrade`]）で分かれる。** `Write` はどの版でも
    /// `upgrade_legacy_cache_in` 経由で [`IndexMaterial::derived`]（自分で導出した組・
    /// 長さを検証しない）を通る。`Skip` はディスクの生データをそのまま使うため、
    /// マスクを持つ版（v3〜v7）は [`IndexMaterial::from_untrusted`]（列長を検証する）、
    /// マスクを持たない v2 だけ `from_tree` を通る。
    material: IndexMaterial,
    /// `index.bin` をバイト列として読み終えるまでの時間（`LoadOrScanStats::cache_read` へ運ぶ）。
    read: Duration,
    /// 旧版昇格（`upgrade_legacy_cache_in`）が走った場合の save 所要時間
    /// （`LoadOrScanStats::cache_save` へ運ぶ）。昇格が走らなかった枝（現行版 v7・
    /// `LegacyUpgrade::Skip`）では `None`。
    ///
    /// **`Some` を作れるのは [`upgrade_legacy_cache_in`] の内側だけである。** ゆえに
    /// variant が「昇格 save を通ったか」そのものであり、**時間の値は判定に使わない**
    /// ——`Some(Duration::ZERO)` は「通ったが測れる時間を要さなかった」を表す正当な値である。
    /// 壁時計のミリ秒を「通った」の代理に使っていた頃は、1 件の治具で区間が時計の量子化に載り、
    /// 検知器が確率的に落ちた（#1054 / #1063 実測）。判定を variant へ移したので、
    /// 代理は残っていない。**#1178 で `u128` のミリ秒から [`Duration`] へ替えたが、
    /// この規約は分解能の話ではない**——値が判定を負わないことが要点であり、ns へ上げても
    /// 「`Some` かどうか」を見る読み方は変わらない。
    ///
    /// **`INDEX_WRITE_LOCK` の取得待ちを含む。** 昇格は読み終えてからロックを取りに行くので、
    /// 計測の始点がロックの外にある——**cache-miss 枝の `cache_save` とは非対称で**、
    /// あちらは scan ごとロックの内側なので待ちが save の数に乗らない。**今のところ待ちは
    /// 立たない**: 製品の呼び出し元は `main` の起動段の 1 つだけで、もう一方の書き手
    /// （索引ビルドのスレッド）は `AppHandle` を要求するためその時点でまだ存在しない。
    /// 待ちが立ちうる書き手を足す日には、この値が「save が遅い」と読める形で嘘をつく。
    upgrade_save: Option<Duration>,
    /// 実際に読めた形式のバージョン。**現行版とは限らない**——フォールバック経路で読めた
    /// ときは旧版であり、`Write` のときは旧版枝（`upgrade_legacy_cache_in`）がその場で
    /// 現行版へ書き戻す（[`LegacyUpgrade`] の doc）。
    ///
    /// **読み手はテストだけである**（`allow(dead_code)` はそのためである）。かつては背景
    /// 再スキャンの昇格判定の入力だったが、昇格がロードの旧版枝へ移り、判定は枝そのものに
    /// なった。**残すのは、フォールバックの鎖のどの枝で読めたのかを外から見る手段がこれ
    /// しか無いからである**——材料だけを見ても v5 の枝で読めたのか v6 の枝で読めたのかは
    /// 区別できず、鎖の枝選択の退行は静かに通る。
    #[allow(dead_code)]
    version: u32,
}

/// 旧版を読んだとき、その場で現行版へ書き戻すか。
///
/// **`Skip` が要るのは、製品の起動経路以外にも `load_cache_in` を通る入口があるからである。**
/// `load_cached_entries` は corpus テストの入口（`search/tests/common.rs`）であり、
/// 開発者の実 `%APPDATA%\Snotra\index.bin` を読む。ここで書き戻すと、テストを走らせる
/// だけで実データを書き換える（#1013 と同型）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyUpgrade {
    Write,
    Skip,
}

fn load_cache(config_hash: u64, upgrade: LegacyUpgrade) -> Option<LoadCacheResult> {
    let dir = Config::config_dir()?;
    load_cache_in(&dir, config_hash, upgrade)
}

/// 旧版を読んだ枝の共通処理: **走査結果を既に手に持っているので、その場で現行版へ書き戻す。**
///
/// **返す材料は書き戻しの副産物である**（`save_cache_sorted_in` が返す木とマスクをそのまま
/// 使う）。旧版が持っていたマスクは捨てて derive し直すが、これは**昇格する起動でだけ払う
/// 一回性の代価**であり、以後は現行版の枝に入る。
///
/// **保存に失敗してもロードは成功させる。** 昇格は最適化であって、失敗が索引の可用性を
/// 落としてはならない——落とすと、書けない環境（読み取り専用・ディスク満杯）で
/// 旧版ユーザーだけが索引を失う。
///
/// **`IndexMaterial::derived` を使う（`from_untrusted` ではない）。** `save_cache_sorted_in` →
/// `derive_columns` がこの場で導出した組であり、ディスクから読んだ検証すべき組ではない
/// （`IndexMaterial` の型 doc）。列長は `derive_columns` が 1 周で 4 本を埋める構成上の
/// 保証であり、`Option` で失敗を返す理由が無い。
///
/// **`built_at` は旧版が名乗っていた値を持ち越す**（[`BuiltAt::Carried`]）。ここは走査しない
/// 書き手なので、現在時刻を打つと「最後に走査した時刻」の意味が壊れる（理由の正本は
/// [`BuiltAt`] の doc）。検知器は `upgrade_carries_the_built_at_it_read`。
fn upgrade_legacy_cache_in(
    dir: &Path,
    mut entries: Vec<AppEntry>,
    config_hash: u64,
    read: Duration,
    version: u32,
    built_at: u64,
) -> LoadCacheResult {
    // **他の全呼び出し元と同じく、保存の直前に整列する。** `save_cache_sorted_in` は
    // 名前どおり整列済みの入力を前提とする——ここだけ怠ると、旧版ファイルが現在の
    // canon と違う順序で書かれていた場合に親の解決が当てにならなくなり（規約と帰結は
    // `sort_entries_canonical` の doc）、平たい木と `sorted_by_path=false` が現行版へ
    // そのまま焼き込まれる。
    sort_entries_canonical(&mut entries);
    // **`index.bin` を書く経路はすべて書き込みロックを経由する契約である。**
    //
    // **ここで測る save_ms はロック取得の待ちを含む**（`Instant::now()` が
    // `with_index_write_lock` より前にある）。**cache-miss 枝の `cache_save` とは
    // 非対称である**——あちらは scan + sort + save をまとめて包むロックの内側で計測を
    // 始めるので、待ちは `scan` より前に落ちて save の数には乗らない。直前の
    // `sort_entries_canonical` を含めない点だけが両者で揃っている。
    //
    // この区間全体は呼び出し元の `cache_load_started` の計測区間の内側で起きるため、
    // `LoadOrScanStats::cache_save` へ運んだ値は `cache_load` の**内数**になる
    // （フェーズの和には足さない——doc を参照）。
    let save_started = Instant::now();
    let (tree, masks) = with_index_write_lock(|| {
        save_cache_sorted_in(dir, entries, config_hash, BuiltAt::Carried(built_at))
    });
    // **`Some` を作るのはこの 1 行だけである**（`LoadCacheResult::upgrade_save` の doc）。
    // 昇格 save を通ったことは、時間の値ではなくこの variant が表す。
    let upgrade_save = Some(save_started.elapsed());
    LoadCacheResult {
        material: IndexMaterial::derived(tree, masks),
        read,
        upgrade_save,
        // **`version` は「読めた」版のままにする。** 呼び出し側はこれで「旧版だった」を
        // 知る。書き戻した後の版を入れると、その事実が消える。
        version,
    }
}

/// 旧版枝が読み終えた生の材料。**判定（`LegacyUpgrade`）はまだ載っていない。**
///
/// `masks` は v2 だけ `None`（マスクを持たない版）。それ以外の旧版は必ず `Some`。
struct LegacyRead {
    entries: Vec<AppEntry>,
    masks: Option<CachedMasks>,
    version: u32,
    /// 旧版が名乗っていた最終構築時刻。**昇格はこれを持ち越す**（理由は [`BuiltAt`] の doc）。
    built_at: u64,
}

/// 旧版枝の出口を一本化する。**`match upgrade` を書くのはここ 1 か所だけである。**
///
/// 旧版枝を 1 本足すときは、その版のバイト列を [`LegacyRead`] へ組み立ててここへ渡すだけでよい
/// ——`Write`/`Skip` の 2 腕を枝ごとに書き起こす義務は生まれない（判定の写しを作らない）。
fn finish_legacy_read(
    dir: &Path,
    config_hash: u64,
    // **`read_took` である**——同じ引数列に `read: LegacyRead`（読めた材料）が居るので、
    // フィールド名 `read` をそのまま使えない（#1178）。
    read_took: Duration,
    upgrade: LegacyUpgrade,
    read: LegacyRead,
) -> Option<LoadCacheResult> {
    match upgrade {
        LegacyUpgrade::Write => Some(upgrade_legacy_cache_in(
            dir,
            read.entries,
            config_hash,
            read_took,
            read.version,
            read.built_at,
        )),
        LegacyUpgrade::Skip => {
            let tree = IndexTree::build(read.entries);
            let material = match read.masks {
                Some(masks) => IndexMaterial::from_untrusted(tree, masks)?,
                // マスクを持たない版（v2）ゆえ検証する列が無い（木の整合は `IndexTree::build` が持つ）。
                None => IndexMaterial::from_tree(tree),
            };
            Some(LoadCacheResult {
                material,
                read: read_took,
                // **`Skip` は書き戻さないので save は起きない。**
                upgrade_save: None,
                version: read.version,
            })
        }
    }
}

/// `load_cache` と同じ読み込みを `dir` 注入で行う（統合テスト用、issue #429）。
///
/// **`INDEX_WRITE_LOCK` を保持したまま呼んではならない。** `LegacyUpgrade::Write` で旧版を
/// 読むと、この関数は [`upgrade_legacy_cache_in`] 経由で同じロックを取りに行く——
/// `std::sync::Mutex` は再入できないので、その場で自己デッドロックする（**索引ロードが
/// 永久に返らない**形なので、テストが落ちるのではなくハングする）。
///
/// **ロードは「読むだけ」に見えるが、旧版枝は書き手である**——`load_cache_in` を「読み取り
/// なのでロックの内側でも安全」と読んだ瞬間にこの罠へ落ちる。実際の呼び出し順は
/// [`load_or_scan_with_stats_in`] が正本で、そこはロックの**外**でこれを呼び、cache-miss と
/// 判ってから初めてロックを取る。
fn load_cache_in(dir: &Path, config_hash: u64, upgrade: LegacyUpgrade) -> Option<LoadCacheResult> {
    let bf = cache_bin_file_in(dir);
    let read_started = Instant::now();
    let bytes = bf.load_bytes()?;
    let read_took = read_started.elapsed();

    // v7 (現行): 木の列 + ビットマスク + **共有を潰した** lower names。
    // deserialize は Cow::Owned を返すため .into_owned() は clone なしの move。
    if let Ok(cache) =
        try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
    {
        if cache.config_hash != config_hash {
            return None;
        }
        // **検証を通してから木にする。** 5 本の列は独立に読まれるので、壊れたファイルは
        // 長さの揃わない列や範囲外の添字を与えうる（帰結は `IndexTree::from_parts` の doc）。
        // 通らなければ `None`＝全走査へ落とす——**版が読めたことは中身が健全であることを
        // 意味しない。**
        let tree = IndexTree::from_parts(
            cache.names.into_owned(),
            cache.is_folder.into_owned(),
            cache.parent.into_owned(),
            cache.aux.into_owned(),
            cache.table.into_owned(),
            cache.sorted_by_path,
        )?;
        let masks = CachedMasks {
            char_masks: cache.char_masks.into_owned(),
            file_name_char_masks: cache.file_name_char_masks.into_owned(),
            // **`Collapsed` で渡す。** ここを `Raw` にすると `assemble` が測り直し、
            // `None` どうしの一致で file name 成分の無いエントリに旗が立つ。
            lower: Some(CachedLower::Collapsed {
                lower_names: cache.lower_names.into_owned(),
                lower_file_names: cache.lower_file_names.into_owned(),
            }),
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(tree, masks)?,
            read: read_took,
            // **現行版は昇格しないので save は起きない。**
            upgrade_save: None,
            version: INDEX_CACHE_VERSION,
        });
    }

    // v6 フォールバック: `target_path` を全件そのまま持つ形式。**読めるが確保が 312,691 回
    // 余分にかかる**——フルパスの `String` を作り、木へ組み替えた段で即座に捨てる。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV6>(&bytes, INDEX_MAGIC, 6) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Collapsed {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return finish_legacy_read(
            dir,
            config_hash,
            read_took,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 6,
                built_at: cache.built_at,
            },
        );
    }

    // v5 フォールバック: 派生文字列を全件そのまま持つ形式。**読めるが確保が倍以上かかる**
    // ——625,380 個の `String` を作り、うち約 527,000 個は `assemble` が測って即座に捨てる。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV5>(&bytes, INDEX_MAGIC, 5) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Raw {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return finish_legacy_read(
            dir,
            config_hash,
            read_took,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 5,
                built_at: cache.built_at,
            },
        );
    }

    // v4 フォールバック: 末尾の normalized_keys を**読んで捨てる**。`Skip`（corpus テストの
    // 入口のみが通る）のときは v5 と同じ 4 本を復元し、どれも v4 に揃っているため
    // **Wave 1 はスキップされたまま**（v4 ユーザーの初回起動は遅くならない）。
    //
    // **`Write` のときは `finish_legacy_read` がその場で現行版へ書き戻す。** かつては
    // 「次回の save」に賭けていたが、当時 save が来る契機は索引の中身が変わったときだけで
    // あり、変わらない限りその「次回」は来なかった——実運用点で v4 が残り続け、毎起動
    // 35.98 MiB を読んで捨てていた（2026-08-07 実測）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV4>(&bytes, INDEX_MAGIC, 4) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Raw {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return finish_legacy_read(
            dir,
            config_hash,
            read_took,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 4,
                built_at: cache.built_at,
            },
        );
    }

    // v3 フォールバック: ビットマスクのみ。`Skip` のときは lower names を持たないため
    // Wave 1 が実行される。**`Write` のときは `finish_legacy_read` 経由で derive し直すため、
    // v3 ユーザーも Wave 1 がスキップされるようになる**（昇格する起動 1 回ぶんの代価は
    // `upgrade_legacy_cache_in` の doc を参照）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV3>(&bytes, INDEX_MAGIC, 3) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: None,
        };
        return finish_legacy_read(
            dir,
            config_hash,
            read_took,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 3,
                built_at: cache.built_at,
            },
        );
    }

    // v2 フォールバック (マスクなし)。`masks: None` を渡すと `finish_legacy_read` の `Skip` 腕が
    // `from_tree` へ落とす（検証する列が無いため）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV2>(&bytes, INDEX_MAGIC, 2) {
        if cache.config_hash != config_hash {
            return None;
        }
        return finish_legacy_read(
            dir,
            config_hash,
            read_took,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: None,
                version: 2,
                built_at: cache.built_at,
            },
        );
    }

    None
}

/// `index.bin` の scan + save 区間を直列化する書き込みロック。
/// 書き手（`rebuild_and_save` / cache-miss save / ロードの旧版枝の昇格）が共有する。
/// `save_cache_sorted` 自体はロックを取らない（呼び出し側が保持する契約）。
static INDEX_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 書き込みロックをブロッキングで取得し、クロージャを実行して結果を返す。
fn with_index_write_lock<R>(f: impl FnOnce() -> R) -> R {
    // Mutex<()> は保持する状態を持たないため、poison しても into_inner で回復して継続する。
    // （`.unwrap()` だと一度の panic 以降、全 index 書き込みが永久に panic する）
    let _guard = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}
