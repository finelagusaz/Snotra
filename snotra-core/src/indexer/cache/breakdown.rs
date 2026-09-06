//! 計測専用——`index.bin` のオンディスク内訳を、読める版ごとに項目別のバイト数へ分解する。
//!
//! **親（`super`）の private なオンディスク構造体を直接読む。** 別 crate の統合テストからは
//! 届かず、代用すると測る対象が製品からずれる（`crate::search` の `footprint` と同じ配置理由）。
//! 分解は読んだ組から postcard の線上長を数え直す形で行い、`index.bin` を書かない。

use serde::Serialize;
use std::path::Path;

use crate::binfmt::try_deserialize_with_header;
use crate::index_tree::NameArena;
use crate::str_arena::{LowerFileColumn, LowerNameColumn};

use super::super::AppEntry;

use super::{
    INDEX_CACHE_VERSION, INDEX_MAGIC, IndexCache, IndexCacheV4, IndexCacheV5, IndexCacheV6,
    cache_bin_file_in,
};

/// `index.bin` の 1 項目が占めるオンディスクのバイト数。
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct CacheByteRow {
    pub label: &'static str,
    pub bytes: usize,
    pub items: usize,
}

/// `index.bin` のバイト内訳。
///
/// **`residual` が 0 でなければ帰属が誤っている。** postcard は struct に枠を持たず、
/// フィールドの連結がそのまま payload になる——ゆえに項目別の長さの和は payload 長と
/// **一致しなければならない**。この検算が無い内訳は、正しい帰属と誤った帰属を区別できない
/// （`tests/memory_footprint.rs` のフェーズ内訳が残余を出すのと同じ理由）。
#[doc(hidden)]
#[derive(Debug)]
pub struct CacheByteBreakdown {
    /// 実際に読めた形式のバージョン。**現行版とは限らない。**
    ///
    /// この計器はファイルを直接読むだけで、旧版を現行版へ書き戻さない（それをするのは
    /// `load_cache_in` の旧版枝である・[`LegacyUpgrade`] の doc）。ゆえに製品がまだ一度も
    /// ロードしていない `index.bin` はここでは旧版のまま現れる。かつては書き戻しの契機が
    /// 索引の中身の変化に括り付いており、**旧版が何日でも残った**——2026-08-07 に実測した
    /// （v5 導入後の実 `index.bin` が v4 のままで、`normalized_keys` を毎起動で読んで捨てて
    /// いた）。**この値を読まずに内訳を解釈しないこと。**
    pub version: u32,
    /// `index.bin` のファイル長（ヘッダ 8 バイトを含む）。
    pub file_len: usize,
    /// postcard payload の長さ（`file_len` − ヘッダ）。
    pub payload_len: usize,
    /// `IndexCache` のフィールド別バイト数。
    pub rows: Vec<CacheByteRow>,
    /// `payload_len` − Σ`rows`。**0 でなければ帰属が誤っている。**
    pub residual: i64,
    /// `entries` の内部内訳（`name` / `target_path` / `is_folder`）。
    pub entry_rows: Vec<CacheByteRow>,
    /// `entries` のバイト数 − Σ`entry_rows`。**0 でなければ帰属が誤っている。**
    pub entry_residual: i64,
}

/// 内訳計器から見た「エントリの持ち方」。版によって形が違うので、両方を数えられる形にする。
///
/// **v7 が来た瞬間に `None` を返す作りにしてはならない。** この計器は形式を変える判断の一次
/// 証拠であり、新形式で黙れば**削減した直後にだけ測れなくなる**（同じ形の失敗が
/// `PERFORMANCE.md` に記録されている）。
enum EntryRepr<'a> {
    /// v6 以下: `AppEntry` の列を丸ごと持つ（`target_path` が実体で入っている）。
    Flat(&'a [AppEntry]),
    /// v7: 木の列。`target_path` は親と拡張子 id に置き換わり、実体は根のぶんだけ `table` に残る。
    ///
    /// **`sorted_by_path` も持つ。** 1 バイトしか無いが、`IndexCache` に居る以上
    /// 帰属しなければ残余が 0 にならない——そして残余の検算は、列を 1 本落とした誤りと
    /// 旗を落とした誤りを区別しない（実際に 5 列だけを数えて +1 B で落ちた）。
    Tree {
        /// **アリーナだが、線上のバイト列は `seq of str` のままである**（[`NameArena`]）。
        /// ゆえに `serialized_len` の勘定も帰属の割り方も v7 の頃と変わらない。
        names: &'a NameArena,
        is_folder: &'a [bool],
        parent: &'a [u32],
        aux: &'a [u32],
        table: &'a [String],
        sorted_by_path: bool,
    },
}

impl EntryRepr<'_> {
    fn count(&self) -> usize {
        match self {
            Self::Flat(e) => e.len(),
            Self::Tree { names, .. } => names.len(),
        }
    }

    fn top_row(&self) -> Option<CacheByteRow> {
        // 版ごとに違うラベル。**リテラルを他所へ書き写さない**——照合に使うと、片方の版でだけ
        // 一致しなくなる形の不変条件が生まれる（呼び出し側はバイト数を move の前に読むので、
        // ラベルで探し直す必要は無い）。
        let label = match self {
            Self::Flat(_) => "entries",
            Self::Tree { .. } => "木（5 列 + 整列の旗）",
        };
        let bytes = match self {
            Self::Flat(e) => serialized_len(e)?,
            Self::Tree {
                names,
                is_folder,
                parent,
                aux,
                table,
                sorted_by_path,
            } => {
                serialized_len(names)?
                    + serialized_len(is_folder)?
                    + serialized_len(parent)?
                    + serialized_len(aux)?
                    + serialized_len(table)?
                    + serialized_len(sorted_by_path)?
            }
        };
        Some(CacheByteRow {
            label,
            bytes,
            items: self.count(),
        })
    }

    /// 上の 1 行をさらに割った内訳。**算術で出すが、和が上の実測値と一致することで
    /// 裏打ちされる**（一致しなければ `entry_residual` に現れる）。
    fn sub_rows(&self) -> Vec<CacheByteRow> {
        let n = self.count();
        let strs = |v: &[String]| -> usize { v.iter().map(|s| postcard_str_len(s)).sum() };
        match self {
            Self::Flat(entries) => vec![
                CacheByteRow {
                    label: "entries: 長さプレフィックス",
                    bytes: varint_len(n),
                    items: 1,
                },
                CacheByteRow {
                    label: "entries[].name",
                    bytes: entries.iter().map(|e| postcard_str_len(&e.name)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "entries[].target_path",
                    bytes: entries
                        .iter()
                        .map(|e| postcard_str_len(&e.target_path))
                        .sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "entries[].is_folder",
                    bytes: n,
                    items: n,
                },
            ],
            Self::Tree {
                names,
                is_folder,
                parent,
                aux,
                table,
                // **`..` を書かない。** 値は読まない（旗は真偽によらず 1 バイト）が、列を
                // 足したときにここを触り忘れたらコンパイルを止めるのが網羅的分解の役目
                // である——`footprint_rows` と同じ規律（`search/footprint.rs` の `//!`）。
                // 残余の検算は `#[ignore]` の計器でしか走らないので、
                // 落とすとコンパイラの検出が手作業へ格下げされる。
                sorted_by_path: _,
            } => vec![
                CacheByteRow {
                    label: "木: 長さプレフィックス（5 列）",
                    bytes: varint_len(n) * 4 + varint_len(table.len()),
                    items: 5,
                },
                CacheByteRow {
                    label: "sorted_by_path（整列の旗）",
                    bytes: 1,
                    items: 1,
                },
                CacheByteRow {
                    label: "is_folder",
                    bytes: is_folder.len(),
                    items: n,
                },
                CacheByteRow {
                    label: "names",
                    bytes: (0..names.len())
                        .map(|i| postcard_str_len(names.get(i)))
                        .sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "parent",
                    bytes: parent.iter().map(|v| varint_len(*v as usize)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "aux",
                    bytes: aux.iter().map(|v| varint_len(*v as usize)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "table（拡張子 + 根のフルパス）",
                    bytes: strs(table),
                    items: table.len(),
                },
            ],
        }
    }
}

/// postcard の LEB128 varint が `v` を表すのに使うバイト数。
fn varint_len(mut v: usize) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// 文字列 1 件を postcard へ書いたときの長さ（長さの varint + 本体）。
///
/// 括り出してあるのは、`sub_rows` の中だけで同じ 2 項式が 3 回書かれていたためである
/// ——新しい表現を足すたびに 4 回目・5 回目と増える形だった。
#[inline]
fn postcard_str_len(s: &str) -> usize {
    varint_len(s.len()) + s.len()
}

/// 値を postcard へ書いたときの長さ（バッファは即座に捨てる）。
fn serialized_len<T: Serialize>(value: &T) -> Option<usize> {
    postcard::to_allocvec(value).ok().map(|v| v.len())
}

/// `dir` の `index.bin` を読み、フィールド別のバイト内訳を返す。
///
/// **オンディスク形式を変える判断の唯一の一次証拠である。** 常駐の内訳
/// （`SearchEngine::footprint_rows`）はメモリが「持たない」ことを学んだ後の姿を映すので、
/// ディスクが何を持ち続けているかは**そちらからは原理的に見えない**（`target_path` は
/// 常駐 0.01 MiB に対しディスクは全文を持つ）。
///
/// **現行版だけでなく旧版も読む。ただし製品のフォールバック鎖（`load_cache_in`）より
/// 狭い**——最古の版まではたどらない（読める版の一覧はこの関数の分岐が正本。書き写すと
/// 版を足したときに片方だけ腐る）。現行版だけを読む形にしてはならない——実運用点の
/// ファイルが旧版のまま留まることは実際に起きるので、そこで `None` を返す計器は
/// **一番測りたい相手にだけ黙る**。
///
/// **この関数が読めないほど古い版では、今もそう黙る。** 製品が読めて計器が読めない版の
/// 幅がその盲点であり、`None` は「壊れている」ではなく「この計器の射程の外」を意味する。
/// 読める版を増やすときは `load_cache_in` の鎖と揃えること。読めた版は
/// [`CacheByteBreakdown::version`] が返す。
///
/// **撤去条件**: オンディスク形式の削減を打ち切ったとき（＝`INDEX_CACHE_VERSION` をこれ以上
/// 形式縮小のために上げないと決めたとき）。それまでは各反復の前後で天井と実績を突き合わせる。
#[doc(hidden)]
pub fn cache_byte_breakdown_in(dir: &Path) -> Option<CacheByteBreakdown> {
    let bf = cache_bin_file_in(dir);
    let bytes = bf.load_bytes()?;
    let file_len = bytes.len();

    if let Ok(c) =
        try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
    {
        drop(bytes);
        return build_breakdown(
            INDEX_CACHE_VERSION,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Tree {
                names: &c.names,
                is_folder: &c.is_folder,
                parent: &c.parent,
                aux: &c.aux,
                table: &c.table,
                sorted_by_path: c.sorted_by_path,
            },
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Collapsed {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    // **v6 を落とさない。** v7 が現行になった瞬間、v6 は「実運用点に実際に置かれている版」に
    // なった——ここを飛ばすと、置き換えようとしている当の形式でだけ計器が黙る
    // （実測で踏んだ: 実 `index.bin` が v6 のとき「読めなかったためスキップ」と出た）。
    if let Ok(c) = try_deserialize_with_header::<IndexCacheV6>(&bytes, INDEX_MAGIC, 6) {
        drop(bytes);
        return build_breakdown(
            6,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Collapsed {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    if let Ok(c) = try_deserialize_with_header::<IndexCacheV5>(&bytes, INDEX_MAGIC, 5) {
        drop(bytes);
        return build_breakdown(
            5,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Raw {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    if let Ok(c) = try_deserialize_with_header::<IndexCacheV4>(&bytes, INDEX_MAGIC, 4) {
        drop(bytes);
        return build_breakdown(
            4,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Raw {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            Some(&c.normalized_keys),
        );
    }

    None
}

/// 派生文字列 2 本の、版ごとの表現。**`build_breakdown` へは表現だけを渡す**——行の生成を
/// 呼び出し側へ出すと、フィールドの並び順が文書の申し合わせに落ちる。postcard は struct に
/// 枠を持たないので、**2 行を入れ替えても長さの和は変わり**、残余 0 の検算はその誤りを
/// 捕まえない（捕まえるのは項目の欠落と重複だけである）。
enum LowerRepr<'a> {
    /// v6 以降: 潰し済み。**列の型で受ける**（線上表現は `Vec<Option<String>>` /
    /// `Vec<LowerFileName>` のままだが、手に持っている物体は列である）。
    Collapsed {
        names: &'a LowerNameColumn,
        files: &'a LowerFileColumn,
    },
    /// v5 / v4: 全件が実体を持つ。**両版で型も数え方も同一**ゆえ 1 つの variant で足りる
    /// （版そのものは [`CacheByteBreakdown::version`] が正本として持つ）。
    Raw {
        names: &'a [String],
        files: &'a [Option<String>],
    },
}

/// 読めた版によらず同じ帰属を組む。**スライスで受ける**ので、現行の `Cow<[T]>` も旧版の
/// `Vec<T>` も同じ経路を通る（postcard はどちらも `serialize_seq` へ委譲し、バイト列は
/// 一致する）。
#[allow(clippy::too_many_arguments)]
fn build_breakdown(
    version: u32,
    file_len: usize,
    built_at: u64,
    config_hash: u64,
    entries: EntryRepr<'_>,
    char_masks: &[u64],
    file_name_char_masks: &[u64],
    lower: LowerRepr<'_>,
    normalized_keys: Option<&[String]>,
) -> Option<CacheByteBreakdown> {
    let (lower_names_row, lower_file_names_row) = match lower {
        LowerRepr::Collapsed { names, files } => (
            CacheByteRow {
                label: "lower_names（潰し済み）",
                bytes: serialized_len(&names)?,
                // **実体を持つ件数を出す**（列の長さではない）。潰れた分は 1 バイトの
                // タグにしかならないので、件数と長さの比が共有の効きを表す。
                items: names.count_present(),
            },
            CacheByteRow {
                label: "lower_file_names（3 状態）",
                bytes: serialized_len(&files)?,
                items: files.count_text(),
            },
        ),
        LowerRepr::Raw { names, files } => (
            CacheByteRow {
                label: "lower_names（全件実体）",
                bytes: serialized_len(&names)?,
                items: names.len(),
            },
            CacheByteRow {
                label: "lower_file_names（全件実体）",
                bytes: serialized_len(&files)?,
                items: files.iter().filter(|s| s.is_some()).count(),
            },
        ),
    };
    // **バイト数はここで読んでおく。** `rows` へ move した行を後からラベルの文字列照合で
    // 探し直す形も書けるが、それは「行を作る側と探す側で版ごとのラベルが一致し続ける」という
    // 不変条件を新設し、外したときは `?` の無言の `None` として出る。`usize` は `Copy` なので
    // move の前に読めば、その不変条件ごと要らなくなる。
    let top_row = entries.top_row()?;
    let entries_bytes = top_row.bytes;
    let mut rows = vec![
        CacheByteRow {
            label: "built_at",
            bytes: serialized_len(&built_at)?,
            items: 1,
        },
        top_row,
        CacheByteRow {
            label: "config_hash",
            bytes: serialized_len(&config_hash)?,
            items: 1,
        },
        CacheByteRow {
            label: "char_masks",
            bytes: serialized_len(&char_masks)?,
            items: char_masks.len(),
        },
        CacheByteRow {
            label: "file_name_char_masks",
            bytes: serialized_len(&file_name_char_masks)?,
            items: file_name_char_masks.len(),
        },
        lower_names_row,
        lower_file_names_row,
    ];
    if let Some(keys) = normalized_keys {
        rows.push(CacheByteRow {
            label: "normalized_keys（v4 のみ・読んで捨てる）",
            bytes: serialized_len(&keys)?,
            items: keys.len(),
        });
    }

    let payload_len = file_len - 8;
    let attributed: usize = rows.iter().map(|r| r.bytes).sum();
    let residual = payload_len as i64 - attributed as i64;

    let entry_rows = entries.sub_rows();
    let entry_attributed: usize = entry_rows.iter().map(|r| r.bytes).sum();
    let entry_residual = entries_bytes as i64 - entry_attributed as i64;

    Some(CacheByteBreakdown {
        version,
        file_len,
        payload_len,
        rows,
        residual,
        entry_rows,
        entry_residual,
    })
}
