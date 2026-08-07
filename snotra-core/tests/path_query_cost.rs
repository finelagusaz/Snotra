//! パスクエリ（`has_path_sep`）の走査コスト実測ハーネス。
//!
//! パスクエリは Fuzzy ビットマスク pre-filter を**スキップする**（`snotra-core/CLAUDE.md`
//! 「モジュール構成」の search.rs 節）。ゆえに全エントリの `normalized_key` に対する
//! 部分文字列検索が毎打鍵で走る、索引規模がそのまま乗る唯一の経路である。
//! それでいて `search/tests/performance.rs` の bench 群も
//! `tests/search_frame_cost.rs` もパス区切りを含むクエリを 1 つも持っていない。
//!
//! **実 `index.bin` に対して測るのが要点である。** パス長がコストを支配し、
//! 実運用点の平均 119.3 B は合成ラダー（66.4 B）の約 2 倍ある（`PERFORMANCE.md`
//! 「索引の常駐の内訳」）。実インデックスが無い環境では自動スキップする。
//!
//! タイミング測定は環境依存ゆえ CI では回さない。手元で release 実行する（コマンドの
//! SSOT は `docs/build-commands.md`）。
//!
//! 比較する 2 つは `normalized_keys` を保持するか導出するかの差である:
//!
//! - **保持**: 事前計算済みの `normalized_key` へ直接 `find`（現行）
//! - **導出**: スレッドローカルの再利用バッファへ `normalize_entry_key` で詰め直してから `find`
//!
//! 差が「`normalized_keys`（実測 35.56 MiB）を捨てる代わりに毎打鍵で払う額」である。

use std::cell::RefCell;
use std::time::Instant;

use rayon::prelude::*;

use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::SearchEngine;

thread_local! {
    /// 導出側の再利用バッファ。**容量を再利用するため暖まった後の確保はゼロ**——
    /// ここを毎回 `String::new()` にすると、測っているものが正規化ではなく確保になる。
    static KEY_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `indexer::normalize_entry_key` と**同じ規則**を、確保済みバッファへ書き出す。
///
/// 製品側にこの形の関数はまだ無い（本ハーネスは導入前の見積もりを取るためにある）。
/// 規則がずれたら測っているものが別物になるので、`normalize_entry_key` を変更したら
/// ここも同時に直す——現物との一致は `derives_same_bytes_as_normalize_entry_key` が固定する。
fn normalize_into(buf: &mut String, path: &str) {
    buf.clear();
    let trimmed = path.trim();
    buf.reserve(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '/' {
            buf.push('\\');
        } else {
            buf.extend(ch.to_lowercase());
        }
    }
}

/// **出荷する実装そのもの**（`indexer::normalize_entry_key_into`）。写しではない。
/// 判定の根拠になる数字は、測る対象を製品と同一にしてから採る。
fn normalize_into_ascii_fast(buf: &mut String, path: &str) {
    indexer::normalize_entry_key_into(buf, path);
}

/// 上の写しが現物と 1 バイトも違わないことを、実インデックスの全パスで固定する。
/// **ここがずれると以降の測定値は別物の計測になる。**
#[test]
fn derives_same_bytes_as_normalize_entry_key() {
    let Some((entries, _)) = load_real_index() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let mut buf = String::new();
    let mut fast = String::new();
    let mut non_ascii = 0usize;
    for entry in &entries {
        let expected = indexer::normalize_entry_key(&entry.target_path);
        normalize_into(&mut buf, &entry.target_path);
        normalize_into_ascii_fast(&mut fast, &entry.target_path);
        assert_eq!(
            buf, expected,
            "写しが現物とずれている: {}",
            entry.target_path
        );
        assert_eq!(
            fast, expected,
            "ASCII 高速路が現物とずれている: {}",
            entry.target_path
        );
        if !entry.target_path.trim().is_ascii() {
            non_ascii += 1;
        }
    }
    println!(
        "{} 件のパスで写し 2 種と現物が一致しました（非 ASCII を含むパス: {non_ascii} 件・{:.1}%）。",
        entries.len(),
        non_ascii as f64 * 100.0 / entries.len() as f64
    );
}

/// 実 `index.bin` を実起動と同じ経路で読み、比較用に「保持していたら」の側も作る。
///
/// **`normalized_keys` は索引にもオンディスクにも既に無い**（v5 で落とした）。保持側は
/// v4 が持っていたものと同じ内容を、ここで 1 度だけ作って再現する——比較の意味は
/// 「事前計算を持つ」対「毎回導出する」であって、どこから来た文字列かではない。
fn load_real_index() -> Option<(Vec<AppEntry>, Vec<String>)> {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        return None;
    }
    let result = indexer::load_or_scan_with_stats(scan, config.search.show_hidden_system);
    let keys: Vec<String> = result
        .entries
        .iter()
        .map(|e| indexer::normalize_entry_key(&e.target_path))
        .collect();
    Some((result.entries, keys))
}

/// 全件走査 1 回の壁時計を返す（`find` の結果は数え上げて捨てない）。
///
/// **`par_iter` で測るのが要点である。** 製品の候補走査は rayon 並列（`search.rs` の
/// `into_par_iter`）ゆえ、単スレッドで測ると 1 打鍵あたりの絶対値をコア数ぶん過大に見る。
/// 倍率は単スレッドでも同じだが、採否を決めるのは絶対値のほうである。
fn sweep_prebuilt(keys: &[String], needle: &str) -> (f64, usize) {
    let start = Instant::now();
    let hits = keys.par_iter().filter(|k| k.contains(needle)).count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

/// 導出側の全件走査。`normalize` に写しの実装を差し替えて 2 変種を同条件で測る。
fn sweep_derived(
    entries: &[AppEntry],
    needle: &str,
    normalize: fn(&mut String, &str),
) -> (f64, usize) {
    let start = Instant::now();
    // バッファは rayon の worker ごとに 1 本（`search/scoring.rs` の thread-local `MATCHER` と
    // 同じ形）。worker 数ぶんしか確保されないので、常駐への寄与は無視できる。
    let hits = entries
        .par_iter()
        .filter(|e| {
            KEY_BUF.with(|cell| {
                let mut buf = cell.borrow_mut();
                normalize(&mut buf, &e.target_path);
                buf.contains(needle)
            })
        })
        .count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

// ---------------------------------------------------------------------------
// 木表現（親 index + 末尾成分）の再構築コスト
// ---------------------------------------------------------------------------

/// `target_path` を「親 index + 末尾成分」で持つ案を、**コストを測るためだけに**最小再現する。
/// 製品にこの形はまだ無い——本ハーネスは実装前の見積もりを取るためにある。
///
/// 末尾成分は folder なら `name` そのもの、file なら `name` + intern した拡張子。どちらも
/// 実データで 100% 成立することを `tests/memory_footprint.rs` の構造前提が測っている。
struct TreeIndex {
    /// 親の index。`-1` は親を持たない（製品では側テーブルにフルパスを持つ・実データで 96 件）。
    parent: Vec<i32>,
    /// 拡張子 intern 表の id。folder と拡張子なし file は `0`（空文字）。
    ext_id: Vec<u16>,
    ext_table: Vec<String>,
}

/// 親 index を引く手段。**`entries` がソート済みなら二分探索が使える**——
/// `sort_entries_canonical` は `target_path` のバイト順で並べ、dedup により
/// `target_path` は一意ゆえ、第 1 キーだけで全順序が決まる。
///
/// 二分探索を使う理由は実測である: 312k 件ぶんの `HashMap` は ~119 B のキーを
/// 全件ハッシュして確保も伴い、実測 249.6 ms（`SearchEngine` 構築は現状 2〜3 ms）。
/// 二分探索はハッシュも確保もせず、文字列比較は先頭で分岐して早く終わる。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ParentLookup {
    /// ソート済み前提の二分探索。
    BinarySearch,
    /// 同上を rayon で並列に回す。導出（親と拡張子）は互いに独立ゆえ分割でき、
    /// `SearchEngine` 構築の他の段（Wave 1/2）が既に rayon 並列であるのと同じ形になる。
    BinarySearchParallel,
    /// 任意順を受け付ける照合表（ソートされていない入力用のフォールバック）。
    HashMap,
}

impl TreeIndex {
    /// `entries` がソート済みなら二分探索、でなければ照合表を選ぶ。
    /// **ソート済みかは仮定せず O(n) で確かめる**——外れた入力に二分探索を当てると
    /// 親を取り違え、エラーにならずに黙って別の木ができる。
    fn choose_lookup(entries: &[AppEntry]) -> ParentLookup {
        if entries
            .windows(2)
            .all(|w| w[0].target_path <= w[1].target_path)
        {
            ParentLookup::BinarySearchParallel
        } else {
            ParentLookup::HashMap
        }
    }

    fn build(entries: &[AppEntry]) -> Self {
        Self::build_with(entries, Self::choose_lookup(entries))
    }

    fn build_with(entries: &[AppEntry], lookup: ParentLookup) -> Self {
        use std::collections::HashMap;

        let by_path: HashMap<&str, usize> = if lookup == ParentLookup::HashMap {
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.target_path.as_str(), i))
                .collect()
        } else {
            HashMap::new()
        };
        let find_parent = |par: &str| -> Option<usize> {
            match lookup {
                ParentLookup::BinarySearch | ParentLookup::BinarySearchParallel => entries
                    .binary_search_by(|e| e.target_path.as_str().cmp(par))
                    .ok(),
                ParentLookup::HashMap => by_path.get(par).copied(),
            }
        };
        let mut parent = vec![-1i32; entries.len()];
        let mut ext_id = vec![0u16; entries.len()];
        let mut ext_table = vec![String::new()];
        let mut ext_ids: HashMap<&str, u16> = HashMap::new();

        // 並列版は導出（親と拡張子・互いに独立）だけ rayon へ出し、intern は後段で 1 回だけ
        // 直列に通す。**構築の他の段（Wave 1/2）は既に rayon 並列ゆえ、これが製品の形である。**
        if lookup == ParentLookup::BinarySearchParallel {
            let resolved: Vec<(i32, &str)> = (0..entries.len())
                .into_par_iter()
                .map(|i| match resolve_one(entries, &find_parent, i) {
                    Some((pi, ext)) => (pi as i32, ext),
                    None => (-1, ""),
                })
                .collect();
            for (i, (pi, ext)) in resolved.into_iter().enumerate() {
                parent[i] = pi;
                if pi >= 0 && !ext.is_empty() {
                    let id = *ext_ids.entry(ext).or_insert_with(|| {
                        ext_table.push(ext.to_string());
                        (ext_table.len() - 1) as u16
                    });
                    ext_id[i] = id;
                }
            }
            return Self {
                parent,
                ext_id,
                ext_table,
            };
        }

        for i in 0..entries.len() {
            let Some((pi, ext)) = resolve_one(entries, &find_parent, i) else {
                continue;
            };
            parent[i] = pi as i32;
            if !ext.is_empty() {
                let id = *ext_ids.entry(ext).or_insert_with(|| {
                    ext_table.push(ext.to_string());
                    (ext_table.len() - 1) as u16
                });
                ext_id[i] = id;
            }
        }
        Self {
            parent,
            ext_id,
            ext_table,
        }
    }
}

/// エントリ 1 件の親 index と拡張子を導出する。**直列版と並列版が同じ導出を通る**ための
/// 唯一の定義であり、片方だけを変えると木が 2 種類できる。
fn resolve_one<'a>(
    entries: &'a [AppEntry],
    find_parent: &impl Fn(&str) -> Option<usize>,
    i: usize,
) -> Option<(usize, &'a str)> {
    let e = &entries[i];
    let path = e.target_path.as_str();
    // **`Path` を経由しない。** Windows の `Path` は `OsStr`（WTF-8）を包み、
    // `to_str()` が UTF-8 検証の走査を全件に乗せる。パスは既に `&str` である。
    let cut = path.rfind(['\\', '/'])?;
    let tail = &path[cut + 1..];
    // 親はドライブ直下だけ区切りを含む（`C:\`）。それ以外は含まない（`C:\foo`）。
    // ここは `Path::parent` の意味論の写しゆえ、**現物との一致は
    // `tree_raw_reconstruction_is_byte_identical_to_target_path` が全件で固定する。**
    let par_end = if cut == 0 || path.as_bytes()[cut - 1] == b':' {
        cut + 1
    } else {
        cut
    };
    let par = &path[..par_end];
    let pi = find_parent(par)?;
    // **連結して比べ直さない。** `par` も `tail` も `path` の部分スライスなので、
    // 確かめるのは間に挟まる区切りだけでよい（1 エントリ 1 確保が消える）。
    let sep = &path[par_end..cut + 1];
    if sep != "\\" && !(sep.is_empty() && par.ends_with(['\\', '/'])) {
        return None;
    }
    let ext = tail.strip_prefix(e.name.as_str())?;
    Some((pi, ext))
}

/// 1 セグメントを `/` → `\` だけ直して**追記**する（`clear` も `trim` も小文字化もしない）。
///
/// **ASCII は一括で動かす。** 1 文字ずつ `push` すると `String: Extend<char>` が毎回 UTF-8
/// 符号化の分岐を通り 2.5-3 倍遅くなる——`normalize_entry_key_into` が明記している落とし穴で
/// あり、計器がここを踏むと木の再構築コストではなく**書き方の差**を測ってしまう（実際に踏んだ）。
/// ASCII の小文字化は呼び出し側が最後に 1 回 `make_ascii_lowercase` でまとめて当てる。
fn push_segment(buf: &mut String, s: &str) {
    if s.is_ascii() {
        let mut rest = s;
        while let Some(pos) = rest.find('/') {
            buf.push_str(&rest[..pos]);
            buf.push('\\');
            rest = &rest[pos + 1..];
        }
        buf.push_str(rest);
    } else {
        // 非 ASCII はここで小文字化まで済ませる。末尾の `make_ascii_lowercase` は ASCII バイト
        // しか触らないため、ここで書いた結果に二重適用しても変わらない（冪等）。
        for ch in s.chars() {
            if ch == '/' {
                buf.push('\\');
            } else {
                buf.extend(ch.to_lowercase());
            }
        }
    }
}

/// 木を辿って正規化キーを組み立てる。**再構築 → 正規化の二段払いにはしない**——
/// 正規化バッファへ直接書き出す 1 パスであり、増分はセグメントぶんのランダム読みだけである。
/// これが出荷する形ゆえ、測る対象もこれにする。
fn reconstruct_normalized_into(buf: &mut String, entries: &[AppEntry], tree: &TreeIndex, i: usize) {
    // 根まで辿って段を積む。深さは実データで最大 17 段（`memory_footprint.rs` 実測）。
    let mut stack = [0u32; 64];
    let mut depth = 0usize;
    let mut cur = i;
    while tree.parent[cur] >= 0 && depth < stack.len() {
        stack[depth] = cur as u32;
        depth += 1;
        cur = tree.parent[cur] as usize;
    }
    buf.clear();
    // 根（製品では側テーブルのフルパス）。`trim` は現物が全体に当てるのと同じ規則で、
    // 先頭側はここ・末尾側は最終セグメントが担う。
    push_segment(buf, entries[cur].target_path.trim());
    for d in (0..depth).rev() {
        let idx = stack[d] as usize;
        if buf.as_bytes().last() != Some(&b'\\') {
            buf.push('\\');
        }
        push_segment(buf, &entries[idx].name);
        let ext = &tree.ext_table[tree.ext_id[idx] as usize];
        if !ext.is_empty() {
            push_segment(buf, ext);
        }
    }
    buf.make_ascii_lowercase();
}

/// 木を辿って **原文のまま**（小文字化も `/` → `\` 変換も trim もせず）パスを組み立てる。
///
/// **正規化版とは別に要る。** `target_path` の読み手は 2 種類あり、`with_normalized_key` が
/// 作る小文字キー（履歴照合・パスマッチ）に対して、tie-break（`ScoredEntry::cmp`）と
/// `SearchResult.path` の clone は原文のバイトを要求する。正規化版を流用すると
/// 表示パスが小文字に化け、順序も別物になる。
fn reconstruct_raw_into(buf: &mut String, entries: &[AppEntry], tree: &TreeIndex, i: usize) {
    let mut stack = [0u32; 64];
    let mut depth = 0usize;
    let mut cur = i;
    while tree.parent[cur] >= 0 && depth < stack.len() {
        stack[depth] = cur as u32;
        depth += 1;
        cur = tree.parent[cur] as usize;
    }
    buf.clear();
    buf.push_str(&entries[cur].target_path);
    for d in (0..depth).rev() {
        let idx = stack[d] as usize;
        if buf.as_bytes().last() != Some(&b'\\') {
            buf.push('\\');
        }
        buf.push_str(&entries[idx].name);
        // 空文字（folder・拡張子なし file）の `push_str` は no-op ゆえ分岐を置かない。
        buf.push_str(&tree.ext_table[tree.ext_id[idx] as usize]);
    }
}

/// 原文の再構築が `target_path` と 1 バイトも違わないことを全パスで固定する。
/// tie-break と表示パスがこの一致に載るため、**正規化版の一致だけでは足りない**。
#[test]
fn tree_raw_reconstruction_is_byte_identical_to_target_path() {
    let Some((entries, _)) = load_real_index() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let tree = TreeIndex::build(&entries);
    let mut buf = String::new();
    for (i, entry) in entries.iter().enumerate() {
        reconstruct_raw_into(&mut buf, &entries, &tree, i);
        assert_eq!(
            buf, entry.target_path,
            "原文の再構築がずれている（index {i}）"
        );
    }
    println!(
        "{} 件のパスで原文の再構築が target_path とバイト一致しました。",
        entries.len()
    );
}

/// 木の構築コスト。**起動のたびに払う額**であり、`SearchEngine` 構築が現状 2〜3 ms である
/// ことに対して無視できるかは測らないと分からない（312k 件ぶんの `HashMap` を組む）。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_tree_build_cost() {
    let Some((entries, _)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    println!(
        "\n=== 木の構築コスト（実 index.bin・{} 件）===",
        entries.len()
    );
    println!("  選ばれた手段: {:?}", TreeIndex::choose_lookup(&entries));
    for lookup in [
        ParentLookup::HashMap,
        ParentLookup::BinarySearch,
        ParentLookup::BinarySearchParallel,
    ] {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            let tree = TreeIndex::build_with(&entries, lookup);
            best = best.min(start.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(&tree);
        }
        println!("  {lookup:?}: 最小 {best:.1} ms（SearchEngine 構築は現状 2〜3 ms）");
    }

    // 2 つの手段が**同じ木を作る**ことを確かめる。速いほうを採るのは、結果が同じときだけ
    // 意味がある——ソート済み前提が外れれば二分探索は黙って別の親を返す。
    let a = TreeIndex::build_with(&entries, ParentLookup::HashMap);
    for other in [
        ParentLookup::BinarySearch,
        ParentLookup::BinarySearchParallel,
    ] {
        let b = TreeIndex::build_with(&entries, other);
        assert_eq!(a.parent, b.parent, "{other:?} が照合表と違う木を作っている");
        assert_eq!(a.ext_id, b.ext_id, "{other:?} の拡張子 id が照合表と違う");
    }
}

/// 木からの再構築が現物と 1 バイトも違わないことを、実インデックスの全パスで固定する。
/// **ここがずれると以降の測定値は別物の計測になる**（`derives_same_bytes_as_normalize_entry_key`
/// と同じ役割で、対象が木表現の側）。
#[test]
fn tree_reconstruction_derives_same_bytes_as_normalize_entry_key() {
    let Some((entries, _)) = load_real_index() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let tree = TreeIndex::build(&entries);
    let mut buf = String::new();
    let mut rooted = 0usize;
    for (i, entry) in entries.iter().enumerate() {
        let expected = indexer::normalize_entry_key(&entry.target_path);
        reconstruct_normalized_into(&mut buf, &entries, &tree, i);
        assert_eq!(
            buf, expected,
            "木からの再構築がずれている: {}",
            entry.target_path
        );
        if tree.parent[i] < 0 {
            rooted += 1;
        }
    }
    println!(
        "{} 件のパスで木からの再構築が現物と一致しました（側テーブル行き {rooted} 件・{:.1}%・\
         intern した拡張子 {} 種）。",
        entries.len(),
        rooted as f64 * 100.0 / entries.len() as f64,
        tree.ext_table.len() - 1,
    );
}

/// 木表現側の全件走査。`sweep_derived` と同条件（同じ needle・同じ `par_iter`・
/// 同じスレッドローカルバッファ）で測る。
fn sweep_tree(entries: &[AppEntry], tree: &TreeIndex, needle: &str) -> (f64, usize) {
    let start = Instant::now();
    let hits = (0..entries.len())
        .into_par_iter()
        .filter(|&i| {
            KEY_BUF.with(|cell| {
                let mut buf = cell.borrow_mut();
                reconstruct_normalized_into(&mut buf, entries, tree, i);
                buf.contains(needle)
            })
        })
        .count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

/// `recent_history` と**同じ形**の全件走査（キー導出 → 照合表への hash 引き）を、現行の導出と
/// 木からの再構築で同条件に測る。パスクエリ側は `contains` で終わるのに対しこちらは hash 引きで
/// 終わる——キー導出の増分がこの形でも同じ額かは、片方を測って他方へ外挿してよい話ではない。
fn sweep_recent_shape(
    entries: &[AppEntry],
    tree: Option<&TreeIndex>,
    wanted: &std::collections::HashMap<&str, usize>,
) -> (f64, usize) {
    let start = Instant::now();
    let hits = (0..entries.len())
        .into_par_iter()
        .filter(|&i| {
            KEY_BUF.with(|cell| {
                let mut buf = cell.borrow_mut();
                match tree {
                    Some(t) => reconstruct_normalized_into(&mut buf, entries, t, i),
                    None => indexer::normalize_entry_key_into(&mut buf, &entries[i].target_path),
                }
                wanted.contains_key(buf.as_str())
            })
        })
        .count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

/// 窓を開くたびに走る `recent_history` の形で、木表現の増分を測る。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_recent_history_shape_with_tree() {
    let Some((entries, _)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    let config = Config::load();
    let history = HistoryStore::load();
    let limit = config.search.recent_limit.unwrap_or(8);
    let recent = history.recent_launches(limit);
    let wanted: std::collections::HashMap<&str, usize> = recent
        .iter()
        .enumerate()
        .map(|(rank, path)| (*path, rank))
        .collect();
    let tree = TreeIndex::build(&entries);
    let n = entries.len();

    println!(
        "\n=== recent_history の形の全件走査（実 index.bin・{n} 件・照合表 {} 件）===",
        wanted.len()
    );
    let (mut best_now, mut best_tree) = (f64::MAX, f64::MAX);
    let (mut h_now, mut h_tree) = (0usize, 0usize);
    for _ in 0..3 {
        let (ms, h) = sweep_recent_shape(&entries, None, &wanted);
        best_now = best_now.min(ms);
        h_now = h;
        let (ms, h) = sweep_recent_shape(&entries, Some(&tree), &wanted);
        best_tree = best_tree.min(ms);
        h_tree = h;
    }
    assert_eq!(h_now, h_tree, "現行と木で一致数が違う（再現がずれている）");
    println!(
        "  現行の導出 {best_now:.1} ms / 木からの再構築 {best_tree:.1} ms（{:+.1} ms・{:.2}x）\
         、一致 {h_now} 件",
        best_tree - best_now,
        best_tree / best_now,
    );
}

/// 空クエリ（窓を開いた瞬間・クエリ消去時）に走る `recent_history` の実コスト。
///
/// `recent_launches` が返すのは高々 `recent_limit` 件（既定 8）だが、現行の実装は照合表を
/// **全エントリぶん**組み立てる。`normalized_keys` を廃止するなら、この経路も
/// 走査時導出へ移ることになるため、先に現状を測る。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_recent_history_cost() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    let result =
        indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
    let n = result.entries.len();
    let engine = match result.cached_masks {
        Some(m) => SearchEngine::new_with_cached_masks(
            result.entries,
            m.char_masks,
            m.file_name_char_masks,
            m.lower_names,
            m.lower_file_names,
            config.search.migemo_enabled,
        ),
        None => SearchEngine::new_with_migemo(result.entries, config.search.migemo_enabled),
    };
    // **ここは実 `history.bin` を読むのが正しい**（#963 でユニットテスト側の fixture は
    // 空へ移したが、計測ハーネスは実運用の姿を測るのが目的である）。統合テストは
    // 別クレートゆえ `HistoryStore::empty()`（`#[cfg(test)]`）に手も届かない。
    let history = HistoryStore::load();
    let limit = config.search.recent_limit.unwrap_or(8);

    println!("\n=== 空クエリの履歴候補（recent_history）のコスト（実 index.bin・{n} 件）===");
    let mut best = f64::MAX;
    let mut hits = 0usize;
    for _ in 0..5 {
        let start = Instant::now();
        let out = engine.recent_history(&history, limit);
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        hits = out.len();
    }
    println!("  recent_limit = {limit}, 返した件数 = {hits}, 最小 {best:.1} ms");
    println!("  = 窓を開くたび・クエリを消すたびに払う額（{n} 件ぶんの照合表を毎回組む現行の形）");
}

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_sweep_cost() {
    let Some((entries, keys)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    let n = entries.len();
    let tree = TreeIndex::build(&entries);
    println!("\n=== パスクエリ全走査のコスト（実 index.bin・{n} 件）===");
    println!(
        "  needle              保持 (ms)  導出:素 (ms)  導出:ASCII (ms)  木:再構築 (ms)  \
         木/導出   hits"
    );

    // 一致数が 0 のもの・多いものを混ぜる。`find` は一致で打ち切るため、
    // **一致が多いほど走査は速く終わる**——一致数を併記しないと倍率を読み違える。
    for needle in [
        "\\workspace\\",
        "\\users\\",
        "\\node_modules\\",
        "\\zzz-no-such-path\\",
    ] {
        // 各 3 回の最小値を採る（外れ値は他プロセスの割り込みで上振れする側にしか出ない）。
        let mut best_pre = f64::MAX;
        let mut best_plain = f64::MAX;
        let mut best_fast = f64::MAX;
        let mut best_tree = f64::MAX;
        let mut hits = 0usize;
        for _ in 0..3 {
            let (ms, h) = sweep_prebuilt(&keys, needle);
            best_pre = best_pre.min(ms);
            hits = h;
            let (ms, h2) = sweep_derived(&entries, needle, normalize_into);
            best_plain = best_plain.min(ms);
            assert_eq!(h, h2, "保持と導出:素 で一致数が違う（写しがずれている）");
            let (ms, h3) = sweep_derived(&entries, needle, normalize_into_ascii_fast);
            best_fast = best_fast.min(ms);
            assert_eq!(h, h3, "保持と導出:ASCII で一致数が違う（写しがずれている）");
            let (ms, h4) = sweep_tree(&entries, &tree, needle);
            best_tree = best_tree.min(ms);
            assert_eq!(h, h4, "保持と木:再構築 で一致数が違う（再現がずれている）");
        }
        // 比べる相手は「保持」ではなく**現行の導出**である——`normalized_keys` は v5 で
        // 既に無く、木表現が置き換えるのは導出:ASCII の側だからである。
        println!(
            "  {needle:<20}{best_pre:>9.1}{best_plain:>13.1}{best_fast:>16.1}{best_tree:>15.1}\
             {:>9.2}x{hits:>7}",
            best_tree / best_fast,
        );
    }
}
