//! `SearchEngine` の構築（インデックス構築時のみ実行される責務を search.rs から分離・#598）。
//!
//! Wave 1（文字列正規化）→ Wave 2（文字ビットマスク）→ kana マスクの並列構築と、
//! IndexCache 復元経路（v4 ヒット時 Wave 1 スキップ / v3 fallback）を担う。全コンストラクタは
//! `assemble` に集約し、並列 Vec 長・kana 系 2 本の {0, entries.len()} 不変条件を一元検証する。
//! 検索ホットパス（`search_with_options` 系）は親 `search.rs` に残す。

use rayon::prelude::*;

use crate::index_tree::{IndexTree, NameArena};
use crate::indexer::{AppEntry, CachedLower, CachedMasks, IndexMaterial};
use crate::query::{
    contains_path_sep, file_char_mask, lower_file_name, measure_derived_sharing, name_char_mask,
    to_kana, to_lower_folded,
};
use crate::str_arena::{LowerFileColumn, LowerFileSlot, LowerNameColumn};

use super::{IncrementalCache, PathStore, SearchEngine, kana_char_mask};

/// `assemble` へ渡す派生文字列。**測定が済んでいるかを型で区別する。**
///
/// **分ける理由は「測り直しが無駄だから」であって、「測り直すと壊れるから」ではない。**
/// 潰し済みの列を測定ループへ流しても結果は変わらない——`measure_derived_sharing` が
/// `lower_name: &str`（`Option` ではない）を取るため、潰れた要素はループの `let Some(..) else`
/// で素通りするからである。**「`None` どうしの一致で file name 成分の無いエントリに旗が立つ」
/// という危険は、判定を一元化する前の `Option<&str>` どうしの比較に固有のものだった**
/// （2026-08-08 に、測り直す実装へ戻す実験で確かめた——検知器は落ちなかった）。
///
/// 分ける実益は 2 つある: **312,690 回の比較を丸ごと省ける**こと（構築 68 → 58 ms のうち
/// 比較ぶん）と、**呼び出し点から「この版は測り直さない」が読めること**である。
enum DerivedStrings {
    /// Wave 1 の出力、または v5 以下のキャッシュ。全要素が実体を持つ。`assemble` が測って潰す。
    Measured {
        lower_names: Vec<Box<str>>,
        lower_file_names: Vec<Option<Box<str>>>,
    },
    /// v6 キャッシュ。記録時に `measure_derived_sharing` で測って潰してある。**測り直さない。**
    ///
    /// **列はアリーナである**（`crate::str_arena`）——ディスクから読んだ形がそのまま索引の
    /// 表現になるので、`assemble` は詰め替えない。
    Collapsed {
        lower_names: LowerNameColumn,
        lower_file_names: LowerFileColumn,
    },
}

impl DerivedStrings {
    /// エントリ数。**2 本の長さは一致していなければならない**ので、片方だけを返す形にはしない
    /// ——食い違いをここで潰すと、`assemble` の長さ検証がその食い違いを素通りする。
    fn len(&self) -> usize {
        let (names, files) = match self {
            Self::Measured {
                lower_names,
                lower_file_names,
            } => (lower_names.len(), lower_file_names.len()),
            Self::Collapsed {
                lower_names,
                lower_file_names,
            } => (lower_names.len(), lower_file_names.len()),
        };
        debug_assert_eq!(
            names, files,
            "DerivedStrings: lower_names と lower_file_names の長さが違う"
        );
        names
    }
}

/// Wave 1 の出力: `(lower_names, lower_file_names, kana_lower_names)`。
/// 前 2 者は構築後に伸長しないため `Box<str>` で保持する（容量ワード 8B/要素を節約）。
///
/// **kana だけ表現が違う。** `assemble` が潰し判定を通す前 2 者と違い、kana は測る相手を
/// 持たないので（`to_kana` はローマ字も変換するため `lower_name` と一致しない）、
/// ここで既に最終形——[`NameArena`]——になっている。
///
/// **`normalized_keys` はここに無い**——`target_path` からの導出に置き換えて索引から外した
/// （実測 35.56 MiB。経緯は `PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を
/// 保持するか導出するか」）。
type Wave1Strings = (Vec<Box<str>>, Vec<Option<Box<str>>>, NameArena);

/// Wave 1: entries から文字列正規化データを並列構築する。
/// lower_names / lower_file_names / kana_lower_names は entries への純粋な map であり
/// 相互依存がないため rayon::join で並列構築する。
/// `migemo_enabled` が false の場合、kana_lower_names は空のアリーナ（migemo 無効ユーザーの
/// 死蔵メモリを削るため、issue #337）。空のときの検索ループ側ガードは search_with_options 参照。
/// 木から Wave 1 を導出する（実体へ戻してから [`compute_wave1`] を通す）。
///
/// **木専用の導出を書き起こさない**——`lower_file_name` は `target_path` を取るので材料は
/// フルパスを要求する。規則が 2 つになると、片方の経路だけが静かにすれる。
///
/// 通るのは [`SearchEngine::new_from_tree`] と、[`SearchEngine::new_with_cached_masks`] の v3 フォールバック腕である。**1 本に寄せてあるのは、実体化するという判断を 2 部出荷しないためである**——コメントごと二重化していると、片方だけ直したときに文面の食い違いが「説明の違い」に見えてしまう。
fn wave1_from_tree(tree: &IndexTree, migemo_enabled: bool) -> Wave1Strings {
    let materialized = tree.materialize();
    compute_wave1(&materialized, migemo_enabled)
}

fn compute_wave1(entries: &[AppEntry], migemo_enabled: bool) -> Wave1Strings {
    let ((lower_names, lower_file_names), kana_lower_names) = rayon::join(
        || {
            rayon::join(
                || {
                    entries
                        .iter()
                        .map(|e| to_lower_folded(&e.name).into_boxed_str())
                        .collect::<Vec<_>>()
                },
                || {
                    entries
                        .iter()
                        .map(|e| lower_file_name(&e.target_path).map(String::into_boxed_str))
                        .collect::<Vec<_>>()
                },
            )
        },
        || {
            // migemo 無効時は kana を構築しない（空のアリーナ）。
            if migemo_enabled {
                // **ここは逐次のままでよい。** この枝が通るのは cache-miss（走査 22〜30 秒）の
                // 内側だけで、キャッシュヒット起動が通るのは `new_with_cached_masks` の
                // `kana_for_cached` である——**あちらは並列を保つ**（そちらの doc が理由を持つ）。
                //
                // 合計バイト数は見込まない（`with_capacity` の第 2 引数が 0）——先に知るには
                // 全件を `to_kana` して数えるしかなく、この枝ではその 1 パスのほうが高くつく。
                let mut arena = NameArena::with_capacity(entries.len(), 0);
                for e in entries {
                    arena.push(&to_kana(&to_lower_folded(&e.name)));
                }
                arena
            } else {
                NameArena::new()
            }
        },
    );
    (lower_names, lower_file_names, kana_lower_names)
}

/// Wave 2: lower_names / lower_file_names からビットマスクを並列構築する。
/// to_lower_folded が主要な Latin アクセントを ASCII へ折り畳み済み（é→e）のため、
/// ここで非 ASCII として残る name は典型的には CJK・アラビア文字など。
/// u64::MAX は任意の query_mask に対して (query_mask & u64::MAX) == query_mask を
/// 満たすため、これらのエントリはクエリに依らず bitmask pre-filter を常に通過する。
fn compute_wave2(
    lower_names: &[Box<str>],
    lower_file_names: &[Option<Box<str>>],
) -> (Vec<u64>, Vec<u64>) {
    rayon::join(
        || {
            lower_names
                .iter()
                .map(|n| name_char_mask(n))
                .collect::<Vec<_>>()
        },
        || {
            // None → 0: entries without a file_name cannot match via the file_name path,
            // so failing the bitmask check (and being skipped when the name also fails) is correct.
            lower_file_names
                .iter()
                .map(|n| file_char_mask(n.as_deref()))
                .collect::<Vec<_>>()
        },
    )
}

/// `kana_for_cached` が添字を割る塊の大きさ。
///
/// **テストから参照できる位置に置いてある。** 塊併合の結線を守る
/// `kana_column_survives_chunked_parallel_merge` は「これを跨ぐ件数」で fixture を組む必要が
/// あり、値を写すとこの定数を増やしたときに**テストが黙って射程を失う**（塊が 1 つに戻り、
/// 順序保存も底上げも検証されなくなる）。
pub(super) const KANA_CHUNK: usize = 4096;

/// migemo 有効時の kana pre-filter 用マスクを構築する。kana 未構築時は空を保つ。
fn compute_kana_char_masks(kana_lower_names: &NameArena) -> Vec<u64> {
    (0..kana_lower_names.len())
        .map(|i| kana_char_mask(kana_lower_names.get(i)))
        .collect()
}

impl SearchEngine {
    /// 全並列 Vec の長さが entries と一致することを検証し、Self を組み立てる。
    ///
    /// 組み立て前に全 Vec を [`Vec::shrink_to_fit`] する。**索引は構築後に伸長しないため
    /// 余剰容量は最後まで解放されない常駐**であり、`index.bin` 経由の Vec はそれを大量に持つ:
    /// serde の `size_hint` は DoS 防止のため事前確保を 1 MiB 相当で頭打ちにし、以降を Vec の
    /// 倍々成長に委ねるため、確保が実使用の約 2 倍で着地する（312,377 エントリの実測で
    /// `char_masks` は 2^19 = 524,288 要素分を確保して 312,377 しか使っていなかった）。
    /// 余剰は `SearchEngine` へそのまま持ち越される——`new_with_cached_masks` の
    /// `Vec<String>` → `Vec<Box<str>>` 変換は確保ブロックを再利用するため、要素サイズが
    /// 縮んでも確保バイト数が動かない。
    ///
    /// **合流点はここ 1 箇所である。** 3 つのコンストラクタがすべて通るため、
    /// 個々の経路へ `shrink_to_fit` を配ると漏れが生じる。実測は `PERFORMANCE.md`
    /// 「索引の常駐の内訳」。
    fn assemble(
        tree: IndexTree,
        derived: DerivedStrings,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
        kana: (NameArena, Vec<u64>),
    ) -> Self {
        let (mut kana_lower_names, mut kana_char_masks) = kana;
        let mut char_masks = char_masks;
        let mut file_name_char_masks = file_name_char_masks;
        // **長さの検証は展開より前に行う。** 下の `debug_assert` は `PathStore` に対して測るので、
        // 派生文字列の長さはここで見ておかないと、ずれた入力が展開の添字 panic として先に出る。
        debug_assert!(
            derived.len() == tree.len(),
            "SearchEngine: 派生文字列の長さが entries と一致しない"
        );
        // `target_path` の `String` はここで木の接頭辞共有へ組み替えられ、`Vec<AppEntry>` ごと
        // 解放される（`path_store` の `//!`）。長さの検証は組み替え後の `PathStore` に対して行う
        // ——組み替えは全件を 1 対 1 で写すが、そう**信じる**のではなく測るのがこの節の役目である。
        let n_before = tree.len();
        // **木は建て直さない。** `index.bin` から読んだ木は親も拡張子も解決済みで、
        // ここは索引側の並べ方へ移すだけである（`PathStore::adopt`）。
        let mut paths = PathStore::adopt(tree);
        paths.shrink_to_fit();
        debug_assert_eq!(
            paths.len(),
            n_before,
            "PathStore: 組み替えでエントリ数が変わってはならない"
        );
        let mut entries = paths;

        // ここから先の `lower_names` の `None` は「`entries[i].name` と同一」を意味する
        // （`SearchEngine::lower_names` の doc）。**`Measured` の時点では全要素が `Some` である**
        // ——下の共有判定はその前提に乗っており、だから `Collapsed` を同じ入口へ流せない
        // （`DerivedStrings` の doc）。`Option<Box<str>>` は `Box<str>` と同じ 16 B ゆえ、
        // 包んでも Vec 本体は動かない。
        //
        // **展開を `PathStore::build` より後に置くのは、旗をその場で立てるためである。**
        // `mark_file_name_is_lower_name` は `CompactEntry` の空きパディングへ書くので、木が
        // 建つ前には呼べない。位置を控える `Vec<usize>` を挟む形もあるが、**制約に足場で
        // 応えるより、制約が満たされる場所まで展開を遅らせるほうが構造が 1 段浅くなる**
        // （`PathStore::build` は派生文字列を一切見ないので、遅らせる障害は無い）。
        // **どちらの枝も、出口では同じ 2 本の列（`crate::str_arena`）になる。** 潰す判定を
        // 通すかどうかだけが違い、`Collapsed` は記録側が測った結果をそのまま表現として使う
        // ——ここで測り直すと、`None` どうしの一致で誤った旗が立つ。
        //
        // **`match` の中で分ける。** 外で `matches!` を取ると、variant を足したときに既定値へ
        // 黙って落ちる——「測らない」へ倒れれば削減だけが減り、「測る」へ倒れれば結果が
        // 壊れる。どちらも沈黙するので、コンパイラに列挙させる。
        let (mut lower_names, mut lower_file_names) = match derived {
            // **旗はここで立てる**（`mark_file_name_is_lower_name` は `CompactEntry` の空き
            // パディングへ書くので、木が建った後でしか呼べない）。ディスクは 3 状態を
            // `LowerFileName` で運び、メモリは「旗 + `Absent`」で表す——**その 2 状態を
            // 混ぜてはならない**（`crate::indexer::LowerFileName` の doc）。
            DerivedStrings::Collapsed {
                lower_names,
                lower_file_names,
            } => {
                for i in 0..lower_file_names.len() {
                    if matches!(lower_file_names.get(i), LowerFileSlot::SameAsLowerName) {
                        entries.mark_file_name_is_lower_name(i);
                    }
                }
                (lower_names, lower_file_names)
            }
            // 重複する派生文字列を落としながら列へ積む。**共有は鎖になっている**:
            //
            //   `lower_file_names[i]` → `lower_names[i]` → `entries[i].name`
            //
            // **判定を全部済ませてから落とす。** 先に `lower_name` を潰すと、前段の比較相手が
            // 消えて file name 側の共有を取りこぼす（結果は正しいまま削減だけが減るので、
            // **挙動テストでは捕まらない**種類の誤りである）。両方を同時に返す
            // `measure_derived_sharing` と、元の列を書き換えずに新しい列へ積むこの形が、
            // その順序を構造として保っている。
            //
            // **判定は `query::measure_derived_sharing` が正本である。** 記録側
            // （`indexer::save_cache_sorted_in`）と同じ関数を通ることだけが、ディスクとメモリで
            // 潰れ方が一致する根拠になる——片方だけ書き換えると、索引はディスクの潰し方を
            // 信じて読み替えるので**結果が静かにずれる**。
            //
            // **`is_folder` で分岐しない。** 一致は indexer の名前導出規則の帰結として実データの
            // folder 100% に成り立つが、`SearchEngine::new` は任意の `AppEntry` を受け取れる。
            // 測って落とすので、外れた入力は文字列を持ち続けるだけで**結果は変わらない**。
            DerivedStrings::Measured {
                lower_names,
                lower_file_names,
            } => {
                let n = lower_names.len();
                let mut names = LowerNameColumn::with_capacity(n);
                let mut files = LowerFileColumn::with_capacity(n);
                for i in 0..n {
                    let file = lower_file_names[i].as_deref();
                    let sharing =
                        measure_derived_sharing(entries.name_at(i), &lower_names[i], file);
                    if sharing.file_name_is_lower_name {
                        entries.mark_file_name_is_lower_name(i);
                        files.push(LowerFileSlot::SameAsLowerName);
                    } else {
                        files.push(match file {
                            Some(s) => LowerFileSlot::Text(s),
                            None => LowerFileSlot::Absent,
                        });
                    }
                    names.push((!sharing.lower_name_is_name).then_some(&*lower_names[i]));
                }
                (names, files)
            }
        };
        lower_names.shrink_to_fit();
        lower_file_names.shrink_to_fit();
        char_masks.shrink_to_fit();
        file_name_char_masks.shrink_to_fit();
        kana_lower_names.shrink_to_fit();
        kana_char_masks.shrink_to_fit();
        debug_assert!(
            lower_names.len() == entries.len()
                && lower_file_names.len() == entries.len()
                && char_masks.len() == entries.len()
                && file_name_char_masks.len() == entries.len(),
            "SearchEngine: all parallel Vecs must have the same length as entries"
        );
        // kana 系の 2 列は {0, entries.len()} を許す（migemo 無効時は空、issue #337）。
        debug_assert!(
            (kana_lower_names.is_empty() && kana_char_masks.is_empty())
                || (kana_lower_names.len() == entries.len()
                    && kana_char_masks.len() == entries.len()),
            "SearchEngine: kana parallel Vecs must both be empty or match entries length"
        );

        // **合流点は `assemble` の末尾そのものである。** 両経路の分岐は上の `match` だけに
        // 絞ってあり、組み立てを別関数へ切り出すと「長さの検証と `shrink_to_fit` を迂回して
        // 直接呼ぶ」経路が表現できてしまう（doc で禁じるより構造で消すほうが強い）。
        let mut engine = Self {
            entries,
            lower_names,
            lower_file_names,
            char_masks,
            file_name_char_masks,
            kana_lower_names,
            kana_char_masks,
            // **保守側で組んでから測る。** `true` は「飛ばさない」＝現行どおりの経路であり、
            // 測り損ねても結果は変わらず速さだけが落ちる（`SearchEngine::any_name_has_path_sep`
            // の doc）。`false` で初期化すると、測る前に読む経路が生まれたとき**結果が壊れる側**
            // へ倒れる。
            any_name_has_path_sep: true,
            // incremental cache は Default（空 query / 空候補 / mode 未設定）で初期化し、
            // 構築直後の初回検索は必ず full scan になる（#601）。
            incremental_cache: IncrementalCache::default(),
        };
        // **3 本のアリーナの連結バイト列を 1 パスずつ舐める。** `entry_view` で 1 件ずつ
        // 解決する形は、オフセット表と blob の 2 読みが 31 万件ぶん走って**構築を数倍に伸ばす**。
        // 額の正本は
        // `PERFORMANCE.md`「採用: パスクエリで name の Fuzzy スコアリングを行わない」
        // の「対価」節である（**ここへ写さない**）。
        //
        // **これで取りこぼさない**——照合が読む文字列（`entry_view` が返す 2 本）は、どれも
        // この 3 本のいずれかの部分文字列である。`lower_names` の `None` は表示名を指す
        // （1 本目）。file name 側は `Text` なら 3 本目、共有旗が立てば解決後の `lower_name` と
        // 同一なので 1 本目か 2 本目に在る。
        //
        // **区切りを含まない名前しか無いのに `true` へ倒れることはあっても、逆は無い。**
        // 倒れる向きが「飛ばさない＝現行どおり」なので、境界をまたいだ偽の一致
        // （アリーナは要素の区切りを持たない）が結果を壊すことはない。
        engine.any_name_has_path_sep = contains_path_sep(engine.entries.names_blob())
            || contains_path_sep(engine.lower_names.blob())
            || contains_path_sep(engine.lower_file_names.blob());
        engine
    }

    /// kana_lower_names を**常に**構築する（migemo 有効相当）。テスト・ベンチ・convenience 用。
    /// 本番のインデックス構築は config 由来の migemo フラグを渡す [`Self::new_with_migemo`] /
    /// [`Self::new_with_cached_masks`] を使い、migemo 無効時は kana を構築しない（issue #337）。
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self::new_with_migemo(entries, true)
    }

    /// `migemo_enabled` に応じて kana_lower_names の構築要否を決めて構築する。
    /// false のとき kana は空（migemo 無効ユーザーの死蔵メモリ ~2.1–2.7MB/50k を削る）。
    pub fn new_with_migemo(entries: Vec<AppEntry>, migemo_enabled: bool) -> Self {
        let (lower_names, lower_file_names, kana_lower_names) =
            compute_wave1(&entries, migemo_enabled);
        let (char_masks, file_name_char_masks) = compute_wave2(&lower_names, &lower_file_names);
        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            IndexTree::build(entries),
            DerivedStrings::Measured {
                lower_names,
                lower_file_names,
            },
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }

    /// [`IndexMaterial`] から構築する。**crate 外から索引を建てる入口はこれだけである**——`new_from_tree` / `new_with_cached_masks` は `pub(crate)` へ下げてある（`new` / `new_with_migemo` は `Vec<AppEntry>` から建てる別系統で、テストと計測ハーネスが使う）。**「唯一の入口」と無条件に書いてはならない**（かつてそう書いていた）: 下げる前は木とマスクをほどいた組を crate 外から渡せ、**長さの検証は `assemble` の `debug_assert` だけ＝release では何も守っていなかった**。
    ///
    /// **派生データの有無で分岐するのはここだけである。** かつては同じ `match` が `PrebuiltIndex` / `Engine` / `SearchEngine` の 3 層・5 か所の呼び出し点へ写っていた（そのうち 1 か所だけを直す案を却下した経緯は [`IndexMaterial`] の doc）。
    pub fn from_material(material: IndexMaterial, migemo_enabled: bool) -> Self {
        let (tree, masks) = material.into_parts();
        match masks {
            Some(masks) => Self::new_with_cached_masks(tree, masks, migemo_enabled),
            None => Self::new_from_tree(tree, migemo_enabled),
        }
    }

    /// 派生文字列を持たない木から構築する。**入口は [`Self::from_material`] であり、ここはその「派生データ無し」側の実装である。**
    ///
    /// **「cache-miss はもうここを通らない」と無条件に書いてはならない**（かつてそう書いており、`RETROSPECTIVE.md` が前サイクルの取りこぼしとして記録している）。保存側が派生データを返さなかった cache-miss は今もここへ来る——条件の正本は `indexer::save_cache_sorted` の分岐である。
    ///
    /// **Wave 1 の材料はフルパスを要求する**（`lower_file_name` は `target_path` を取る）ため、
    /// 導出のあいだだけ実体へ戻す。**木専用の導出を書き起こさない**——規則が 2 つになると
    /// 片方の経路だけが静かにすれる。**この根拠は「ここが遅くてよい」に依存しない**
    /// ——実体化の対価は `IndexTree::materialize` の doc が持つ。
    pub(crate) fn new_from_tree(tree: IndexTree, migemo_enabled: bool) -> Self {
        let (lower_names, lower_file_names, kana_lower_names) =
            wave1_from_tree(&tree, migemo_enabled);
        let (char_masks, file_name_char_masks) = compute_wave2(&lower_names, &lower_file_names);
        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            tree,
            DerivedStrings::Measured {
                lower_names,
                lower_file_names,
            },
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }

    /// キャッシュから読み込んだデータを使って SearchEngine を構築する。
    ///
    /// - `char_masks` / `file_name_char_masks`: Wave 2 の再計算をスキップ
    /// - `cached_lower`: `Some` なら Wave 1 の再計算もスキップ。**variant が意味を分ける**
    ///   ——`Collapsed`（v6）は共有判定もスキップし、`Raw`（v5/v4）は `assemble` が測って潰す。
    ///   `None`（v3 フォールバック）は Wave 1 を通常通り並列実行する
    /// - `migemo_enabled`: false のとき kana_lower_names を構築しない（空、issue #337）。
    ///   **全経路で**このフラグを反映する
    ///
    /// **`normalized_keys` は受け取らない。** v5 でオンディスク形式から落とし、検索時に
    /// `target_path` から導出する形へ移した。v4 バイト列を読んだ場合も当該フィールドは
    /// 捨てるだけで、`lower_names` / `lower_file_names` が揃っていれば Wave 1 は
    /// スキップされたままである（v4 ユーザーの初回起動が遅くならない）。
    pub(crate) fn new_with_cached_masks(
        tree: IndexTree,
        masks: CachedMasks,
        migemo_enabled: bool,
    ) -> Self {
        // **組のまま受け取る。** 3 引数へほどく形だと `char_masks` と `file_name_char_masks` が
        // 同型（`Vec<u64>`）で隣接し、**取り違えてもコンパイルが通る**——そして誤ったマスクを
        // 持つ索引は panic せず、Fuzzy pre-filter の false negative（検索結果が静かに欠ける）
        // としてしか現れない。`CachedMasks` はこの 4 本を「同じ導出から出た組」として束ねる
        // ためにある型なので、境界を跨ぐ手前でほどかない。
        let CachedMasks {
            char_masks,
            file_name_char_masks,
            lower: cached_lower,
        } = masks;
        // kana は毎起動再計算する（キャッシュに持たない）。migemo 無効時は空のアリーナのまま。
        let kana_for_cached = |tree: &IndexTree| {
            if migemo_enabled {
                // **添字で並列化する。** 名前はアリーナの切り出しゆえ `par_iter` を持たない
                // （持たせるには `IndexedParallelIterator` を自前で書くことになり、
                // 得るものは同じ分割である）。
                //
                // **塊ごとに組んでから併合する。** アリーナへの push は逐次だが、
                // **ここを逐次化してはならない**——この経路はキャッシュヒット起動が毎回通り、
                // `to_kana` の全件適用が秒オーダーで**毎起動に**乗る（額は `PERFORMANCE.md`
                // 「採用: `kana_lower_names` も文字列アリーナで持つ」が正本）。
                // `chunks` + `collect` は順序を保つので、併合は受け取った順に繋ぐだけでよい。
                //
                // **この結線を守るのは `kana_column_survives_chunked_parallel_merge` である**
                // （`search/tests/build.rs`）。既存の A/B 突き合わせでは足りない——あちらの
                // fixture は数件しか無く、[`KANA_CHUNK`] に対して塊が 1 つしか出ないので、
                // **塊の順序を反転する変異を当てても緑のまま通る**（2026-08-12 実測）。
                let chunks: Vec<(String, Vec<u32>)> = (0..tree.len())
                    .into_par_iter()
                    .chunks(KANA_CHUNK)
                    .map(|idxs| {
                        let mut blob = String::new();
                        let mut offsets = Vec::with_capacity(idxs.len());
                        for i in idxs {
                            blob.push_str(&to_kana(&to_lower_folded(tree.name_at(i))));
                            offsets.push(blob.len() as u32);
                        }
                        (blob, offsets)
                    })
                    .collect();
                NameArena::from_chunks(chunks)
            } else {
                NameArena::new()
            }
        };

        let (derived, kana_lower_names) = match cached_lower {
            // v6: 潰し済み。`assemble` は測り直さない。
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => {
                let kana = kana_for_cached(&tree);
                (
                    DerivedStrings::Collapsed {
                        lower_names,
                        lower_file_names,
                    },
                    kana,
                )
            }
            // v5/v4: Wave 1 はスキップできるが、共有判定は `assemble` が行う。
            // postcard デシリアライズ後の String は capacity == len のため
            // into_boxed_str は再アロケーションを伴わない。
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => {
                let kana = kana_for_cached(&tree);
                (
                    DerivedStrings::Measured {
                        lower_names: lower_names
                            .into_iter()
                            .map(String::into_boxed_str)
                            .collect(),
                        lower_file_names: lower_file_names
                            .into_iter()
                            .map(|o| o.map(String::into_boxed_str))
                            .collect(),
                    },
                    kana,
                )
            }
            // v3 フォールバック: Wave 1 を並列実行（migemo フラグを反映）。
            None => {
                // v3 は派生文字列を一切持たないので Wave 1 を走らせる（実体へ戻す理由は
                // [`wave1_from_tree`] の doc）。v3 は、ロードの旧版枝が現行版へ書き戻すまでの
                // 1 回だけ通る経路である。
                let (lower_names, lower_file_names, kana) = wave1_from_tree(&tree, migemo_enabled);
                (
                    DerivedStrings::Measured {
                        lower_names,
                        lower_file_names,
                    },
                    kana,
                )
            }
        };

        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            tree,
            derived,
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **長さがずれた入力では、添字の panic ではなく名前つきの診断が先に出る。**
    ///
    /// `assemble` の潰しの `match` は `lower_names[i]` / `lower_file_names[i]` /
    /// `entries.name_at(i)` を引くので、長さの検証がそれより後ろへ落ちると**添字 panic が
    /// 診断を追い越す**——出るのは「index out of bounds」だけになり、どの列がずれたのかを
    /// 報せない。
    ///
    /// **この順序は散文ではなくここが持つ。** かつては `assemble` の中のコメントが
    /// 「長さの `debug_assert` より後でもある」と書いて守っていたが、潰しの位置を動かす
    /// 変更でそのコメントごと消え、順序を検算する手段が残らなかった（2026-08-12 のレビューで
    /// 論点になった）。落として文言を見る形なら、位置を動かした瞬間に落ちる。
    #[test]
    #[should_panic(expected = "派生文字列の長さが entries と一致しない")]
    fn mismatched_derived_length_reports_the_diagnostic_not_an_index_panic() {
        let tree = IndexTree::build(vec![AppEntry {
            name: "a".to_string(),
            target_path: "C:\\a".to_string(),
            is_folder: false,
        }]);
        // 木は 1 件、派生文字列は 0 件。
        let _ = SearchEngine::assemble(
            tree,
            DerivedStrings::Measured {
                lower_names: Vec::new(),
                lower_file_names: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
            (NameArena::new(), Vec::new()),
        );
    }

    /// 2 本の派生文字列の長さが**互いに**違うときも、添字ではなく診断が出る
    /// （こちらは [`DerivedStrings::len`] が撃つ——`assemble` の入口で `len()` を呼ぶことが
    /// その検査を通す唯一の経路であり、片方の長さを直接読む形へ変えると素通りする）。
    #[test]
    #[should_panic(expected = "lower_names と lower_file_names の長さが違う")]
    fn mismatched_pair_length_reports_the_diagnostic_not_an_index_panic() {
        let tree = IndexTree::build(vec![AppEntry {
            name: "a".to_string(),
            target_path: "C:\\a".to_string(),
            is_folder: false,
        }]);
        let _ = SearchEngine::assemble(
            tree,
            DerivedStrings::Measured {
                lower_names: vec!["a".into()],
                lower_file_names: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
            (NameArena::new(), Vec::new()),
        );
    }
}
