//! `SearchEngine` の構築（インデックス構築時のみ実行される責務を search.rs から分離・#598）。
//!
//! Wave 1（文字列正規化）→ Wave 2（文字ビットマスク）→ kana マスクの並列構築と、
//! IndexCache 復元経路（v4 ヒット時 Wave 1 スキップ / v3 fallback）を担う。全コンストラクタは
//! `assemble` に集約し、並列 Vec 長・kana 系 2 本の {0, entries.len()} 不変条件を一元検証する。
//! 検索ホットパス（`search_with_options` 系）は親 `search.rs` に残す。

use rayon::prelude::*;

use crate::indexer::{AppEntry, normalize_entry_key};
use crate::query::{file_char_mask, lower_file_name, name_char_mask, to_kana, to_lower_folded};

use super::{IncrementalCache, SearchEngine, kana_char_mask};

/// Wave 1 の出力: `(lower_names, lower_file_names, normalized_keys, kana_lower_names)`。
/// いずれも構築後に伸長しないため `Box<str>` で保持する（容量ワード 8B/要素を節約）。
type Wave1Strings = (
    Vec<Box<str>>,
    Vec<Option<Box<str>>>,
    Vec<Box<str>>,
    Vec<Box<str>>,
);

/// Wave 1: entries から文字列正規化データを並列構築する。
/// lower_names / lower_file_names / normalized_keys / kana_lower_names は
/// entries への純粋な map であり相互依存がないため rayon::join で並列構築する。
/// `migemo_enabled` が false の場合、kana_lower_names は空 Vec（migemo 無効ユーザーの
/// 死蔵メモリを削るため、issue #337）。空 Vec の検索ループ側ガードは search_with_options 参照。
fn compute_wave1(entries: &[AppEntry], migemo_enabled: bool) -> Wave1Strings {
    let ((lower_names, lower_file_names), (normalized_keys, kana_lower_names)) = rayon::join(
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
            rayon::join(
                || {
                    entries
                        .iter()
                        .map(|e| normalize_entry_key(&e.target_path).into_boxed_str())
                        .collect::<Vec<_>>()
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
            )
        },
    );
    (
        lower_names,
        lower_file_names,
        normalized_keys,
        kana_lower_names,
    )
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
        entries: Vec<AppEntry>,
        lower_names: Vec<Box<str>>,
        lower_file_names: Vec<Option<Box<str>>>,
        normalized_keys: Vec<Box<str>>,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
        kana: (Vec<Box<str>>, Vec<u64>),
    ) -> Self {
        let (mut kana_lower_names, mut kana_char_masks) = kana;
        let mut entries = entries;
        let mut lower_names = lower_names;
        let mut lower_file_names = lower_file_names;
        let mut normalized_keys = normalized_keys;
        let mut char_masks = char_masks;
        let mut file_name_char_masks = file_name_char_masks;
        entries.shrink_to_fit();
        lower_names.shrink_to_fit();
        lower_file_names.shrink_to_fit();
        normalized_keys.shrink_to_fit();
        char_masks.shrink_to_fit();
        file_name_char_masks.shrink_to_fit();
        kana_lower_names.shrink_to_fit();
        kana_char_masks.shrink_to_fit();
        debug_assert!(
            lower_names.len() == entries.len()
                && lower_file_names.len() == entries.len()
                && normalized_keys.len() == entries.len()
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
        Self {
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
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
        let (lower_names, lower_file_names, normalized_keys, kana_lower_names) =
            compute_wave1(&entries, migemo_enabled);
        let (char_masks, file_name_char_masks) = compute_wave2(&lower_names, &lower_file_names);
        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }

    /// キャッシュから読み込んだデータを使って SearchEngine を構築する。
    ///
    /// - `char_masks` / `file_name_char_masks`: Wave 2 の再計算をスキップ
    /// - `cached_lower_names` / `cached_lower_file_names` / `cached_normalized_keys`:
    ///   v4+ キャッシュヒット時に Some → Wave 1 の再計算もスキップ（A-3）
    ///   v3 フォールバック時は None → Wave 1 を通常通り並列実行
    /// - `migemo_enabled`: false のとき kana_lower_names を構築しない（空 Vec、issue #337）。
    ///   v4 パス（再計算）・v3 フォールバックの**両方**でこのフラグを反映する。
    pub fn new_with_cached_masks(
        entries: Vec<AppEntry>,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
        cached_lower_names: Option<Vec<String>>,
        cached_lower_file_names: Option<Vec<Option<String>>>,
        cached_normalized_keys: Option<Vec<String>>,
        migemo_enabled: bool,
    ) -> Self {
        let (lower_names, lower_file_names, normalized_keys, kana_lower_names) =
            if let (Some(ln), Some(lfn), Some(nk)) = (
                cached_lower_names,
                cached_lower_file_names,
                cached_normalized_keys,
            ) {
                // A-3: v4 キャッシュヒット → Wave 1 完全スキップ（kana_lower_names は毎起動再計算）。
                // キャッシュ由来の Vec<String> を Box<str> へ移す。postcard デシリアライズ後の
                // String は capacity == len のため into_boxed_str は再アロケーションを伴わない。
                // migemo 無効時は kana を再計算せず空 Vec のままにする。
                let kana = if migemo_enabled {
                    entries
                        .par_iter()
                        .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let ln = ln
                    .into_iter()
                    .map(String::into_boxed_str)
                    .collect::<Vec<_>>();
                let lfn = lfn
                    .into_iter()
                    .map(|o| o.map(String::into_boxed_str))
                    .collect::<Vec<_>>();
                let nk = nk
                    .into_iter()
                    .map(String::into_boxed_str)
                    .collect::<Vec<_>>();
                (ln, lfn, nk, kana)
            } else {
                // v3 フォールバック: Wave 1 を並列実行（migemo フラグを反映）
                compute_wave1(&entries, migemo_enabled)
            };

        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }
}
