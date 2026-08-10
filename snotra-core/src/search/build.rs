//! `SearchEngine` の構築（インデックス構築時のみ実行される責務を search.rs から分離・#598）。
//!
//! Wave 1（文字列正規化）→ Wave 2（文字ビットマスク）→ kana マスクの並列構築と、
//! IndexCache 復元経路（v4 ヒット時 Wave 1 スキップ / v3 fallback）を担う。全コンストラクタは
//! `assemble` に集約し、並列 Vec 長・kana 系 2 本の {0, entries.len()} 不変条件を一元検証する。
//! 検索ホットパス（`search_with_options` 系）は親 `search.rs` に残す。

use rayon::prelude::*;

use crate::index_tree::IndexTree;
use crate::indexer::{AppEntry, CachedLower, CachedMasks, IndexMaterial, LowerFileName};
use crate::query::{
    file_char_mask, lower_file_name, measure_derived_sharing, name_char_mask, to_kana,
    to_lower_folded,
};

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
    Collapsed {
        lower_names: Vec<Option<String>>,
        lower_file_names: Vec<LowerFileName>,
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
/// いずれも構築後に伸長しないため `Box<str>` で保持する（容量ワード 8B/要素を節約）。
///
/// **`normalized_keys` はここに無い**——`target_path` からの導出に置き換えて索引から外した
/// （実測 35.56 MiB。経緯は `PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を
/// 保持するか導出するか」）。
type Wave1Strings = (Vec<Box<str>>, Vec<Option<Box<str>>>, Vec<Box<str>>);

/// Wave 1: entries から文字列正規化データを並列構築する。
/// lower_names / lower_file_names / kana_lower_names は entries への純粋な map であり
/// 相互依存がないため rayon::join で並列構築する。
/// `migemo_enabled` が false の場合、kana_lower_names は空 Vec（migemo 無効ユーザーの
/// 死蔵メモリを削るため、issue #337）。空 Vec の検索ループ側ガードは search_with_options 参照。
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
            // migemo 無効時は kana を構築しない（空 Vec）。
            if migemo_enabled {
                entries
                    .iter()
                    .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
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

/// migemo 有効時の kana pre-filter 用並列 Vec を構築する。kana 未構築時は空 Vec を保つ。
fn compute_kana_char_masks(kana_lower_names: &[Box<str>]) -> Vec<u64> {
    kana_lower_names
        .iter()
        .map(|name| kana_char_mask(name))
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
        kana: (Vec<Box<str>>, Vec<u64>),
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
        // **3 つめ（測り直すか）は `match` の中から出す。** 外で `matches!` を取ると、variant を
        // 足したときに既定値へ黙って落ちる——「測らない」へ倒れれば削減だけが減り、「測る」へ
        // 倒れれば結果が壊れる。どちらも沈黙するので、コンパイラに列挙させる。
        let (mut lower_names, mut lower_file_names, needs_measuring) = match derived {
            DerivedStrings::Measured {
                lower_names,
                lower_file_names,
            } => (
                lower_names.into_iter().map(Some).collect::<Vec<_>>(),
                lower_file_names,
                true,
            ),
            // **展開に測定は要らない。** 記録側が `measure_derived_sharing` で測った結果が
            // そのまま表現になっている——ここで測り直すと、`None` どうしの一致で誤った旗が立つ。
            DerivedStrings::Collapsed {
                lower_names,
                lower_file_names,
            } => {
                let files = lower_file_names
                    .into_iter()
                    .enumerate()
                    .map(|(i, f)| match f {
                        LowerFileName::Text(s) => Some(s.into_boxed_str()),
                        LowerFileName::Absent => None,
                        LowerFileName::SameAsLowerName => {
                            entries.mark_file_name_is_lower_name(i);
                            None
                        }
                    })
                    .collect();
                (
                    lower_names
                        .into_iter()
                        .map(|o| o.map(String::into_boxed_str))
                        .collect(),
                    files,
                    false,
                )
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
        // kana 系 Vec は {0, entries.len()} を許す（migemo 無効時は空 Vec、issue #337）。
        debug_assert!(
            (kana_lower_names.is_empty() && kana_char_masks.is_empty())
                || (kana_lower_names.len() == entries.len()
                    && kana_char_masks.len() == entries.len()),
            "SearchEngine: kana parallel Vecs must both be empty or match entries length"
        );

        // 重複する派生文字列を落とす。**共有は鎖になっている**:
        //
        //   `lower_file_names[i]` → `lower_names[i]` → `entries[i].name`
        //
        // 実測の削減は前段 9.71 MiB / 255,961 ブロック、後段 9.80 MiB / 270,355 ブロック。
        // 読み替えはどちらも `entry_view` の 1 点で、そこでも同じ順に解決する。
        //
        // **判定を全部済ませてから落とす。** 先に `lower_names[i]` を潰すと、前段の比較相手が
        // 消えて file name 側の共有を取りこぼす（結果は正しいまま削減だけが減るので、
        // **テストでは捕まらない**種類の誤りである）。
        //
        // **ビットマスクより後に置く。** `file_name_char_masks` は完全な文字列から導出されて
        // いなければならず（cache 経由でも Wave 2 経由でも `assemble` 到達時には確定済み）、
        // 先に潰すと `file_char_mask(None) == 0` になって pre-filter が **false negative** を
        // 出す（`compute_wave2` の不変条件）。**長さの `debug_assert` より後でもある**——
        // 長さがずれた入力では、添字の panic ではなく上の診断が先に出るべきである。
        //
        // **`is_folder` で分岐しない。** 一致は indexer の名前導出規則の帰結として実データの
        // folder 100% に成り立つが、`SearchEngine::new` は任意の `AppEntry` を受け取れる。
        // 測って落とすので、外れた入力は文字列を持ち続けるだけで**結果は変わらない**。
        //
        // **この形が最も単純で、かつ速い形でもある。** 構築は 57 → 75 ms（各 3 回の最小値）
        // 増えるが、比較を rayon へ出しても、比較と解放の両方を出しても**最小値は 75 ms のまま
        // だった**（3 変種を同一セッションで実測）。並列化では取り戻せないので、いちばん短い
        // 形を残してある。増分の機序（比較か、255,961 個の `Box<str>` の解放か）は
        // **切り分けていない**——ここに書けるのは「並列化は効かない」までである。
        // **`Collapsed` はここを通らない**（旗は展開のその場で立て終えている）。通しても
        // 結果は変わらない——潰れた要素は下の `let Some(..) else` で素通りする——が、
        // 312,690 回の比較が丸ごと無駄になる（`DerivedStrings` の doc）。
        let measured_count = if needs_measuring { entries.len() } else { 0 };
        for i in 0..measured_count {
            // ここでの `lower_names[i]` は必ず `Some`（`Measured` を `map(Some)` した直後であり、
            // このループは各 `i` を 1 度しか通らない）。
            let Some(lower_name) = lower_names[i].as_deref() else {
                continue;
            };
            // **判定は `query::measure_derived_sharing` が正本である。** 記録側
            // （`indexer::save_cache_sorted_in`）と同じ関数を通ることだけが、ディスクとメモリで
            // 潰れ方が一致する根拠になる——片方だけ書き換えると、索引はディスクの潰し方を
            // 信じて読み替えるので**結果が静かにずれる**。上に書いた「判定を全部済ませてから
            // 落とす」も、両方を同時に返すあの関数が構造として保っている。
            let sharing = measure_derived_sharing(
                &entries.get(i).name,
                lower_name,
                lower_file_names[i].as_deref(),
            );

            if sharing.file_name_is_lower_name {
                entries.mark_file_name_is_lower_name(i);
                lower_file_names[i] = None;
            }
            if sharing.lower_name_is_name {
                lower_names[i] = None;
            }
        }

        // **合流点は `assemble` の末尾そのものである。** 両経路の分岐は上のループの有無だけに
        // 絞ってあり、組み立てを別関数へ切り出すと「長さの検証と `shrink_to_fit` を迂回して
        // 直接呼ぶ」経路が表現できてしまう（doc で禁じるより構造で消すほうが強い）。
        Self {
            entries,
            lower_names,
            lower_file_names,
            char_masks,
            file_name_char_masks,
            kana_lower_names,
            kana_char_masks,
            // incremental cache は Default（空 query / 空候補 / mode 未設定）で初期化し、
            // 構築直後の初回検索は必ず full scan になる（#601）。
            incremental_cache: IncrementalCache::default(),
        }
    }

    /// kana_lower_names を**常に**構築する（migemo 有効相当）。テスト・ベンチ・convenience 用。
    /// 本番のインデックス構築は config 由来の migemo フラグを渡す [`Self::new_with_migemo`] /
    /// [`Self::new_with_cached_masks`] を使い、migemo 無効時は kana を構築しない（issue #337）。
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self::new_with_migemo(entries, true)
    }

    /// `migemo_enabled` に応じて kana_lower_names の構築要否を決めて構築する。
    /// false のとき kana は空 Vec（migemo 無効ユーザーの死蔵メモリ ~2.1–2.7MB/50k を削る）。
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
    /// - `migemo_enabled`: false のとき kana_lower_names を構築しない（空 Vec、issue #337）。
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
        // kana は毎起動再計算する（キャッシュに持たない）。migemo 無効時は空 Vec のまま。
        let kana_for_cached = |tree: &IndexTree| {
            if migemo_enabled {
                tree.names
                    .par_iter()
                    .map(|n| to_kana(&to_lower_folded(n)).into_boxed_str())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
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
