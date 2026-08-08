//! 構築（`assemble` / 各コンストラクタ）の不変条件のテスト。
//!
//! 検索の結果は正しいまま余剰容量だけが常駐する、という失敗は**挙動テストで捕まらない**。
//! ここはその 1 点を守る。

use super::common::{make_entries, real_index_entries, real_scanned_entries};
use crate::index_tree::IndexTree;
use crate::indexer::{AppEntry, CachedLower, CachedMasks, LowerFileName};
use crate::search::*;

/// 潰れの 4 種の出現件数。**非空虚検査の材料**——どれも起きない fixture では「全件一致」が
/// 空虚になるので、呼び出し側がこれを検算する。
///
/// **無名の 4-tuple にしてはならない。** 4 つとも `usize` で、取り違えても型は何も言わず、
/// 非空虚検査は「どれかが 0 でないこと」しか見ないので**入れ替わっても緑になる**。
#[derive(Debug, Clone, Copy)]
struct CollapseCounts {
    /// `lower_name` が `name` と同一で潰れた件数。
    shared_name: usize,
    /// `lower_file_name` が `lower_name` と同一で旗が立った件数。
    shared_file: usize,
    /// file name 成分が無い件数（`LowerFileName::Absent` 相当）。
    absent: usize,
    /// file name が独自の文字列として残った件数（`LowerFileName::Text` 相当）。
    text: usize,
}

/// 保存側の導出と `assemble` の測り直しが、同じ入力に対して **`entry_view` の読み替えまで
/// 一致する**ことを全件で確かめる。
///
/// 反復 11 で cache-miss の枝が `new_from_tree`（`Measured` を `assemble` が測って潰す）から
/// `new_with_cached_masks`（保存が返す `Collapsed` をそのまま使う）へ移った。一致は
/// **構成的には示せる**——`name` は木が逐語で持ち、`lower_file_name` の材料は原文と組み直しが
/// バイト一致し（`path_store_raw_matches_target_path_over_real_index` が実データ全件で接地）、
/// 判定は両経路とも `query::measure_derived_sharing` の 1 か所を通る。ここが足すのは
/// **その 3 本のどれかが将来切れたときに気づく**という機構である。
///
/// **突き合わせるのは `entry_view` である。** 読み替えの単一点であり、表現の差が製品に
/// 見える形になるのはここだけである。生の `lower_names[i]` を比べる形にすると
/// 「どちらも潰れているが旗が違う」食い違いを取り逃す。
///
/// **保存側は実経路（[`crate::indexer::derive_columns`]）を通す**——潰し方を書き起こすと、
/// 比べているのが 2 つの実装ではなくテストの中の 1 つの実装になる。**`index.bin` は書かない**
/// （書いたものと返したものが同一であることは `indexer.rs` の往復テストが別に pin する）。
fn assert_save_and_assemble_agree(
    label: &str,
    entries: Vec<AppEntry>,
    migemo_enabled: bool,
) -> CollapseCounts {
    let n = entries.len();

    // A: 旧 cache-miss 経路。木を実体化して Wave 1/2 を建て直し、`assemble` が測って潰す。
    let a = SearchEngine::new_from_tree(IndexTree::build(entries.clone()), migemo_enabled);
    // B: 新 cache-miss 経路。保存側が導出した `Collapsed` をそのまま索引の表現にする。
    let (tree, masks) = crate::indexer::derive_columns(entries).into_cached_masks();
    let b = SearchEngine::new_with_cached_masks(tree, masks, migemo_enabled);

    assert_engines_agree(label, migemo_enabled, &a, &b, n);
    collapse_census(&b, 0..n)
}

/// 2 つの索引を全件で突き合わせる（比較だけ・返り値なし）。
///
/// **同じ突き合わせを 2 度書くと、片方だけが緩む**ので 1 か所に置く。**数え上げはここに
/// 混ぜない**——[`collapse_census`] が単一の索引に対して行う統計であり、比較を 1 ミリも
/// 必要としない（範囲を切るのは範囲を知っている呼び出し側の仕事である）。
fn assert_engines_agree(
    label: &str,
    migemo_enabled: bool,
    a: &SearchEngine,
    b: &SearchEngine,
    n: usize,
) {
    assert_eq!(a.entries.len(), n, "{label}: A の件数が想定と違う");
    assert_eq!(b.entries.len(), n, "{label}: B の件数が想定と違う");
    // **kana 系 2 本は「両方空 or 両方 n」を、A と B の両方について見る。** 長さの一致だけでは
    // 足りない——両方が `n-1` でも通ってしまい、`assemble` の `debug_assert` が守っている性質を
    // 検査済みだと読ませてしまう。下の per-entry が添字を引く前提でもある。
    for (side, engine) in [("A", a), ("B", b)] {
        for (col, len) in [
            ("kana_lower_names", engine.kana_lower_names.len()),
            ("kana_char_masks", engine.kana_char_masks.len()),
        ] {
            assert!(
                len == 0 || len == n,
                "{label}/migemo={migemo_enabled}: {side} の {col} が 0 でも {n} でもない（{len}）"
            );
        }
    }
    assert_eq!(
        a.kana_lower_names.len(),
        b.kana_lower_names.len(),
        "{label}/migemo={migemo_enabled}: A と B で kana の構築有無が違う"
    );

    for i in 0..n {
        let (va, vb) = (a.entry_view(i), b.entry_view(i));
        assert_eq!(
            va.lower_name, vb.lower_name,
            "{label}/migemo={migemo_enabled}: index {i} の lower_name がずれている"
        );
        assert_eq!(
            va.lower_file_name, vb.lower_file_name,
            "{label}/migemo={migemo_enabled}: index {i} の lower_file_name がずれている"
        );
        assert_eq!(
            va.entry.file_name_is_lower_name, vb.entry.file_name_is_lower_name,
            "{label}/migemo={migemo_enabled}: index {i} の共有の旗がずれている"
        );
        // **表現そのものも比べる。** 読み替えが一致していても、片方だけが実体を持ち続けて
        // いれば削減が失われている（結果は正しいままなので挙動テストでは捕まらない）。
        assert_eq!(
            a.lower_names[i].is_none(),
            b.lower_names[i].is_none(),
            "{label}/migemo={migemo_enabled}: index {i} の lower_names の潰れ方がずれている"
        );
        assert_eq!(
            a.lower_file_names[i].is_none(),
            b.lower_file_names[i].is_none(),
            "{label}/migemo={migemo_enabled}: index {i} の lower_file_names の潰れ方がずれている"
        );
        // マスクは潰す前の完全な文字列から導出されるので、両経路で一致しなければならない。
        assert_eq!(
            a.char_masks[i], b.char_masks[i],
            "{label}/migemo={migemo_enabled}: index {i} の char_mask がずれている"
        );
        assert_eq!(
            a.file_name_char_masks[i], b.file_name_char_masks[i],
            "{label}/migemo={migemo_enabled}: index {i} の file_name_char_mask がずれている"
        );
        // **kana 系 2 本も突き合わせる。** ここも同じ導出の 2 実装である——A 側は
        // `compute_wave1` が実体化した `AppEntry.name` から、B 側は `kana_for_cached` が
        // `tree.names` から導く。比べないと、`migemo_enabled == true` の腕で**追加検証される
        // assertion が 1 件も無い**（migemo 利用者だけがローマ字検索で候補を取り逃す退行は、
        // migemo 無効で回る計測環境からは見えない）。空 Vec は上で長さを検証済み。
        if !a.kana_lower_names.is_empty() {
            assert_eq!(
                a.kana_lower_names[i], b.kana_lower_names[i],
                "{label}/migemo={migemo_enabled}: index {i} の kana_lower_name がずれている"
            );
            assert_eq!(
                a.kana_char_masks[i], b.kana_char_masks[i],
                "{label}/migemo={migemo_enabled}: index {i} の kana_char_mask がずれている"
            );
        }
    }
    println!("{label}/migemo={migemo_enabled}: {n} 件一致");
}

/// 1 つの索引の `range` について潰れの 4 種を数える。
///
/// **範囲を切るのは呼び出し側である。** PATH 併合版は追記した範囲だけを数えたい——全件で
/// 数えると base のエントリだけでカウンタが埋まり、**追記側が 1 度も判定を下さなくても
/// 非空虚検査が緑になる**（`extend_cached_masks` を検算しているつもりで検算していない状態）。
///
/// **3 種は排他である**ことを `else if` の連なりで表す。独立した 3 本の加算にして否定を
/// 各腕へ配る形だと、4 種目を足すときに 1 本忘れれば二重計上になり、合計を誰も検算して
/// いないので気づかない。`shared_name` は別軸なので連なりに入れない。
fn collapse_census(engine: &SearchEngine, range: std::ops::Range<usize>) -> CollapseCounts {
    let mut c = CollapseCounts {
        shared_name: 0,
        shared_file: 0,
        absent: 0,
        text: 0,
    };
    for i in range {
        let view = engine.entry_view(i);
        c.shared_name += usize::from(engine.lower_names[i].is_none());
        if view.entry.file_name_is_lower_name {
            c.shared_file += 1;
        } else if view.lower_file_name.is_none() {
            c.absent += 1;
        } else if view.lower_file_name.is_some_and(|f| f != view.lower_name) {
            c.text += 1;
        }
    }
    c
}

/// 合成 fixture で、潰れの 4 種すべてを 1 度以上通す（既定スイート）。
#[test]
fn save_side_collapse_and_assemble_measurement_agree_at_entry_view() {
    let mut entries = vec![
        // file name 成分が**無い**（`Path::file_name()` が `None`）→ `Absent`。
        // 実データでは稀ゆえ、合成でしか安定して通せない腕である。
        AppEntry {
            name: "C:".to_string(),
            target_path: "C:\\".to_string(),
            is_folder: true,
        },
        // 末尾成分と一致する folder → `SameAsLowerName`（実データの folder はほぼこれ）。
        AppEntry {
            name: "apps".to_string(),
            target_path: "C:\\apps".to_string(),
            is_folder: true,
        },
        // 拡張子ゆえ file name が別物 → `Text`。かつ `name` は大文字を含むので
        // `lower_name` は実体を持つ（＝ `lower_names[i]` が `Some` の側）。
        AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\Firefox.lnk".to_string(),
            is_folder: false,
        },
        // 既に小文字の folder → `lower_name` が `name` と同一（`None` の側）。
        AppEntry {
            name: "projects".to_string(),
            target_path: "C:\\projects".to_string(),
            is_folder: true,
        },
        // 非 ASCII（アクセント畳み込みが効く側）。
        AppEntry {
            name: "Café".to_string(),
            target_path: "C:\\apps\\Café.lnk".to_string(),
            is_folder: false,
        },
    ];
    // **製品と同じ並びで木を建てる**（`save_cache_sorted_in` の呼び出し元は必ず通す）。
    crate::indexer::sort_entries_canonical(&mut entries);

    // **migemo の両設定を通す。** kana は `assemble` の外で確定するので潰し方は同じはずだが、
    // 計測環境が `false` に寄っている以上、通していない側は「壊れても気づかない側」である。
    for migemo_enabled in [false, true] {
        let c = assert_save_and_assemble_agree("synthetic", entries.clone(), migemo_enabled);
        assert!(
            c.shared_name > 0 && c.shared_file > 0 && c.absent > 0 && c.text > 0,
            "潰れの 4 種が揃っていない fixture では一致が空虚である（{c:?}）"
        );
    }
}

/// **保存が返した派生データの直後に PATH エントリを併合する経路**（`indexer::merge_path_entries`）。
///
/// **この組み合わせは反復 11 で初めて生きた。** それ以前は `cached_masks` が `None` だったので `extend_cached_masks` は**呼ばれず**、`new_from_tree` が拡張後の木から Wave 1/2 を導出していた——PATH エントリぶんも自動的に整合していた。今は `Some` で返るので「マスクへ追記 → 木へ根として追加 → `new_from_cache`」の順に変わる。**上の 2 本の検知器はどちらもここを迂回する**（`new_with_cached_masks` を直接呼び、PATH 併合を通らない）。
///
/// 守るのは 2 つ。(1) 追記側（`extend_cached_masks`）の潰し方が、拡張後の木から導出した結果と一致すること。(2) **追記した 2 列と木の長さが揃うこと**——`assemble` の長さ検証は `debug_assert` ゆえ release では消え、ずれは添字 panic か沈黙の食い違いになる。**追記を欠く変異で実際に赤くなることを確かめてある**（`migemo_enabled` の両設定で）。
///
/// **起動経路と背景の再構築（`drain_index`）は同じ `merge_path_entries` を通る**ので、この 1 本が両方を覆う。**そこを迂回して併合するコードは書ける**（`IndexTree::extend_with_roots` は `pub`）——閉じているのは現存する呼び出し点であって、この検知器の射程もそこまでである。
///
/// **`masks` が `None` の枝はここを通らない。** [`merge_path_entries_extends_the_tree_even_without_masks`] がそちらを持つ——木への追加が `if let Some` の内側へ入る誤りに、この検知器は原理的に当たらない（`Some` の枝しか通らないため。変異を注入して、あちらだけが赤くなることを実測した）。
#[test]
fn path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree() {
    let mut base = vec![
        AppEntry {
            name: "apps".to_string(),
            target_path: "C:\\apps".to_string(),
            is_folder: true,
        },
        AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\Firefox.lnk".to_string(),
            is_folder: false,
        },
    ];
    crate::indexer::sort_entries_canonical(&mut base);

    // PATH エントリは**すべて根として**足される（親を解決しない・`extend_with_roots` の doc）。
    // 潰れの 3 種を通す形にしてある——`name` の大小と file name の一致／不一致で、追記側が
    // 実際に判定を下す。
    let path_entries = vec![
        // `lower_name` が実体を持ち（大文字）、file name は拡張子ぶん別物 → `Text`
        AppEntry {
            name: "Node".to_string(),
            target_path: "C:\\tools\\node.exe".to_string(),
            is_folder: false,
        },
        // `lower_name` が `name` と同一、file name も一致 → `None` + `SameAsLowerName`
        AppEntry {
            name: "git".to_string(),
            target_path: "C:\\bin\\git".to_string(),
            is_folder: false,
        },
        // 別ドライブ・全大文字の拡張子
        AppEntry {
            name: "CURL".to_string(),
            target_path: "D:\\utils\\CURL.EXE".to_string(),
            is_folder: false,
        },
    ];
    let total = base.len() + path_entries.len();

    for migemo_enabled in [false, true] {
        // B: 現行の製品経路。**併合は `indexer::merge_path_entries` そのものを通す**——手で 2 手を並べると、この検知器が測るのは製品コードではなく「テストの中の写し」になる（起動経路と drain 経路が同じ関数を通ることが長さの揃う根拠なので、その関数を迂回した検算には意味が無い）。
        let (mut tree, masks) = crate::indexer::derive_columns(base.clone()).into_cached_masks();
        // `merge_path_entries` が `&mut Option<_>` を取るので、ここだけ包み直しが要る（製品の呼び出し点はどれも `Option` の束縛を持っているため、そちらには要らない）。
        let mut masks = Some(masks);
        crate::indexer::merge_path_entries(&mut tree, &mut masks, path_entries.clone());
        let b = SearchEngine::new_with_cached_masks(tree, masks.unwrap(), migemo_enabled);

        // A: 変更前の cache-miss。拡張後の木から Wave 1/2 を導出する。
        let mut tree_a = IndexTree::build(base.clone());
        tree_a.extend_with_roots(path_entries.clone());
        let a = SearchEngine::new_from_tree(tree_a, migemo_enabled);

        assert_engines_agree("path-merge", migemo_enabled, &a, &b, total);
        // **数えるのは追記した範囲だけである。** 全件で数えると base の 2 件だけで
        // カウンタが埋まり、**追記側が 1 度も判定を下さなくても緑になる**——この検知器が
        // 名指ししている当の相手を見ないまま「空虚ではない」と報告する形になる。
        let c = collapse_census(&b, base.len()..total);
        assert!(
            c.shared_name > 0 && c.shared_file > 0 && c.text > 0,
            "追記側（`extend_cached_masks`）が判定を 1 つも下していない fixture では\
             一致が空虚である（追記範囲で {c:?}）"
        );
    }
}

/// **`masks` が `None` の枝でも木は伸びる**（`indexer::merge_path_entries`）。
///
/// **この腕は到達可能な製品経路である。** `Config::config_dir` が引けないとき `indexer::rebuild_and_save` は派生データを返さず、`src-tauri` の再構築は木だけを持って `PrebuiltIndex::from_tree` へ落ちる。派生文字列を持たない古い版を読んだキャッシュヒットも同じ形になる。**「起こりえない状態のテスト」として削らないこと。**
///
/// 守るのは 1 つ: 木への追加が `masks` の有無に**引きずられない**こと。追加を `if let Some` の内側へ書いてしまうと、`None` の枝では PATH エントリが索引から丸ごと落ちる——**panic も型エラーも出ず、PATH 経由でしか届かないプログラムが検索から静かに消える**だけである（上の検知器は `Some` の枝しか通らないので、この誤りには当たらない）。
///
/// **効いているのは `tree.len()` の等値検査である。** 下の A/B 比較は `masks` が `None` のとき A 側と同じ計算になるので原理的に失敗しない——**それでも置いてあるのは、この検知器が「木が伸びる」だけでなく「伸びた結果が正しい木である」も名乗るためである**。`assert_engines_agree` を落として `len` だけにすると、根として足すべきものを親解決して足す変更（＝別の壊し方）を見逃す。
#[test]
fn merge_path_entries_extends_the_tree_even_without_masks() {
    let mut base = vec![
        AppEntry {
            name: "apps".to_string(),
            target_path: "C:\\apps".to_string(),
            is_folder: true,
        },
        AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\Firefox.lnk".to_string(),
            is_folder: false,
        },
    ];
    crate::indexer::sort_entries_canonical(&mut base);

    let path_entries = vec![
        AppEntry {
            name: "Node".to_string(),
            target_path: "C:\\tools\\node.exe".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "git".to_string(),
            target_path: "C:\\bin\\git".to_string(),
            is_folder: false,
        },
    ];
    let total = base.len() + path_entries.len();

    for migemo_enabled in [false, true] {
        // B: `masks` を持たない製品経路。
        let mut tree = IndexTree::build(base.clone());
        crate::indexer::merge_path_entries(&mut tree, &mut None, path_entries.clone());
        assert_eq!(
            tree.len(),
            total,
            "masks が None のとき木が伸びていない（追加が `if let Some` の内側に入った合図）"
        );
        let b = SearchEngine::new_from_tree(tree, migemo_enabled);

        // A: 木への追加を直に書いた版。
        let mut tree_a = IndexTree::build(base.clone());
        tree_a.extend_with_roots(path_entries.clone());
        let a = SearchEngine::new_from_tree(tree_a, migemo_enabled);

        assert_engines_agree("path-merge-no-masks", migemo_enabled, &a, &b, total);
    }
}

/// 実データの全件で同じことを確かめる（規模と、合成では作れないパスの形）。
///
/// **原文はファイルシステムの走査から取る。`index.bin` から取ってはならない。** v7 は
/// `target_path` を持たないので、そこから取ると A 側の `materialize` が組み直し対組み直しの
/// **不動点**になり、どれだけ壊れても落ちない（件数つきの成功メッセージまで出る）。走査から
/// 取れば、A 側の `materialize(build(原文))` が原文と一致することまでこの 1 本が覆う
/// ——それが候補表の ⚠「保存側の潰し方が `assemble` の測り直しと一致するか」の核である。
///
/// 合成 fixture では作れないものを足す——深い階層・非 ASCII・空白・大文字の拡張子、
/// 根と非根が混ざった木。`#[ignore]` は実環境依存ゆえで、CI の保証は上の合成が持つ。
#[test]
#[ignore = "実ファイルシステムの全走査・手元で明示的に走らせる"]
fn save_side_collapse_agrees_with_assemble_over_real_index() {
    let Some(entries) = real_scanned_entries() else {
        return;
    };
    let c = assert_save_and_assemble_agree("real", entries, false);
    // **`Absent` は要求しない**（実データにはほぼ現れない・合成が担う）。**条件に無い項目を
    // メッセージへ数え上げない**——`Absent 0` が並ぶと、要求していないものが原因だと誤読させる。
    assert!(
        c.shared_name > 0 && c.shared_file > 0 && c.text > 0,
        "実データで潰れが 1 件も起きないのは異常である（lower_name 共有 {} / \
         file_name 共有 {} / Text {}）",
        c.shared_name,
        c.shared_file,
        c.text,
    );
    println!("実データ {c:?}");
}

/// `len` より大きい容量を持つ Vec を作る（`index.bin` 由来の Vec が持つ余剰の再現）。
/// `with_capacity` 後の `extend` は再確保しないため、容量はそのまま残る。
fn oversized<T>(items: Vec<T>) -> Vec<T> {
    let mut v = Vec::with_capacity(items.len() * 4 + 64);
    v.extend(items);
    v
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// 余剰容量は「検索は正しいが常駐だけが増える」形の劣化ゆえ、挙動テストでは捕まらない。
///
/// 経路は `new_with_cached_masks` の v4 ヒット枝を選ぶ——`Vec<String>` → `Vec<Box<str>>` の
/// 変換が確保ブロックを再利用して余剰を持ち越すため、実運用で余剰が最も乗る経路である。
///
/// **7 行のうち 6 行と `entries` の 1 行は機序が違う。** 派生文字列とマスクの 6 本は
/// `assemble` の `shrink_to_fit` が消えたら落ちる。`entries` だけは落ちない——`PathStore::adopt`
/// の 2 つの `collect` は長さの分かった iterator から `len` ちょうどで確保するので、入力側の
/// 余剰は原理的に伝播せず `paths.shrink_to_fit()` は no-op である（v7 で第 1 引数が
/// `IndexTree` になったときにそうなった。`paths.shrink_to_fit()` を外して実測済み）。
/// **それでも 1 行を残すのは、守っている対象が呼び出しではなく「索引に余剰が無い」性質
/// だからである**——`adopt` が exact でなくなった日に、この行だけが気づく。
#[test]
fn assemble_shrinks_parallel_vecs_to_fit() {
    let names = ["Firefox", "Chrome", "Notepad"];
    let lower = ["firefox", "chrome", "notepad"];
    let entries = make_entries(&names);
    let n = entries.len();

    let engine = SearchEngine::new_with_cached_masks(
        IndexTree::build(entries),
        CachedMasks {
            char_masks: oversized(vec![0u64; n]),
            file_name_char_masks: oversized(vec![0u64; n]),
            // **`Raw`（v5/v4 相当）で渡す。** 余剰容量が最も乗るのはこの経路である
            // ——`Vec<String>` → `Vec<Box<str>>` の変換が確保ブロックを再利用して持ち越す。
            lower: Some(CachedLower::Raw {
                lower_names: oversized(owned(&lower)),
                lower_file_names: oversized(lower.iter().map(|s| Some(s.to_string())).collect()),
            }),
        },
        true, // migemo 有効 = kana 系 2 本も構築される
    );

    // `Vec::shrink_to_fit` の契約は「len へできる限り近づける」であり厳密一致ではないが、
    // std の `RawVec::shrink` は要求サイズちょうどで capacity を張り直す。ここが落ちたら
    // `shrink_to_fit` が消えたか、アロケータの契約が変わったかのどちらかである。
    let actual = [
        ("entries", engine.entries.capacity()),
        ("lower_names", engine.lower_names.capacity()),
        ("lower_file_names", engine.lower_file_names.capacity()),
        ("char_masks", engine.char_masks.capacity()),
        (
            "file_name_char_masks",
            engine.file_name_char_masks.capacity(),
        ),
        ("kana_lower_names", engine.kana_lower_names.capacity()),
        ("kana_char_masks", engine.kana_char_masks.capacity()),
    ];
    for (label, capacity) in actual {
        assert_eq!(capacity, n, "{label} に余剰容量が残っている（len = {n}）");
    }
}

/// **`Collapsed`（v6 キャッシュ）経路を通す唯一の検知器。**
///
/// 他の旗の検知器（`shared_file_name_flag_is_measured_not_inferred_from_is_folder` /
/// `shared_lower_name_chain_collapses_both_links`）は**どちらも `Measured` 経路**を通るので、
/// v6 の展開が壊れてもこの 2 本は緑のままである。ここが守るのは
/// **`LowerFileName` の 3 状態が `entry_view` の読み替えへ正しく着地すること**——とくに
/// `Absent` が「file name 成分が無い」のまま残り、`SameAsLowerName` と混ざらないこと。
///
/// **測り直しの有無はここでは測れない**（測り直しても結果は変わらない・`DerivedStrings` の
/// doc）。索引の内部表現（`is_none` と旗）まで見るのは、**検索結果が正しいまま削減だけが
/// 失われる**形の退行を捕まえるためである。
#[test]
fn collapsed_cache_is_not_remeasured_and_absent_file_names_stay_absent() {
    let entries = vec![
        // 1. file name 成分が**無い**（`Absent`）かつ `lower_name` も潰れている。
        //    **素朴な `None == None` 比較で旗が立つのはこのエントリである。**
        AppEntry {
            name: "docs".to_string(),
            target_path: "C:\\docs".to_string(),
            is_folder: true,
        },
        // 2. file name が `lower_name` と同一（`SameAsLowerName`）。旗が立つべき。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
        // 3. file name が独自の文字列（`Text`）。
        AppEntry {
            name: "firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        },
    ];
    let n = entries.len();

    // **添字の対応が命綱である。** `Collapsed` の列は添字で `entries` に対応づけられるので、
    // 木を建てる段（`IndexTree::build`）と索引側へ移す段（`PathStore::adopt`）はどちらも
    // 入力順を保つ（前者は enumerate 順に push、後者は zip）。
    let engine = SearchEngine::new_with_cached_masks(
        IndexTree::build(entries),
        CachedMasks {
            char_masks: vec![0u64; n],
            file_name_char_masks: vec![0u64; n],
            lower: Some(CachedLower::Collapsed {
                // すべて `None` = `name` と同一（3 件とも既に小文字）。
                lower_names: vec![None, None, None],
                lower_file_names: vec![
                    LowerFileName::Absent,
                    LowerFileName::SameAsLowerName,
                    LowerFileName::Text("firefox.lnk".to_string()),
                ],
            }),
        },
        false,
    );

    // 1: **旗が立ってはならない。** ここが `Some("docs")` になる実装が、この PR が
    // 「結果を壊しうる唯一の点」と名指したものである。
    let absent = engine.entry_view(0);
    assert_eq!(absent.lower_name, "docs");
    assert_eq!(
        absent.lower_file_name, None,
        "`Absent` は「`lower_name` と同一」ではない——測り直す実装だとここが Some になる"
    );
    assert!(!absent.entry.file_name_is_lower_name);

    // 2: 旗が立ち、`lower_name` へ解決される。
    let shared = engine.entry_view(1);
    assert_eq!(shared.lower_file_name, Some("same"));
    assert!(shared.entry.file_name_is_lower_name);
    assert!(
        engine.lower_file_names[1].is_none(),
        "共有するエントリの文字列は落ちていなければ削減にならない"
    );

    // 3: 独自の文字列がそのまま残る。
    let text = engine.entry_view(2);
    assert_eq!(text.lower_file_name, Some("firefox.lnk"));
    assert!(!text.entry.file_name_is_lower_name);

    // 記録側の潰し方がそのまま索引の表現になっている（測り直していれば `Some` が復活する）。
    assert!(
        engine.lower_names.iter().all(|n| n.is_none()),
        "`None` = name と同一。展開で実体を作り直してはならない"
    );
}

/// **`lower_file_name` の共有は `is_folder` からの推論ではなく、測った結果である。**
///
/// 実データでは folder の 100%（255,961/255,961）が `lower_file_name == lower_name` になるが、
/// それは indexer が folder の `name` に `file_name()` を使う規則の帰結であって、
/// `SearchEngine::new` が受け取る `AppEntry` の性質ではない。`is_folder` で分岐する実装に
/// 差し替えたら、下の 1 件目が `Some("tail")` ではなく `Some("alias")` を返して落ちる。
///
/// 2 件目は成立する側で、旗が立って `lower_file_names` から文字列が落ちることを見る
/// ——**両方を置く**のは、片方だけだと「常に共有しない」実装も「常に共有する」実装も
/// 通ってしまうためである。
///
/// **migemo の両設定を通す。** kana 系 2 本は `assemble` の**外**で確定してから渡されるので
/// 潰す位置の不変条件は同じはずだが、実運用の config が片方に寄っている以上（計測環境は
/// `migemo_enabled = false`）、通していない側は「壊れても気づかない側」である。
#[test]
fn shared_file_name_flag_is_measured_not_inferred_from_is_folder() {
    let entries = vec![
        // name が末尾成分と一致しない folder。indexer は作らないが API は受け取れる。
        AppEntry {
            name: "alias".to_string(),
            target_path: "C:\\real\\tail".to_string(),
            is_folder: true,
        },
        // 一致する folder（実データの姿）。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
    ];

    for migemo_enabled in [false, true] {
        let engine = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled);

        let mismatched = engine.entry_view(0);
        assert_eq!(mismatched.lower_name, "alias", "migemo={migemo_enabled}");
        assert_eq!(
            mismatched.lower_file_name,
            Some("tail"),
            "migemo={migemo_enabled}: 末尾成分と name が違う folder で共有してはならない"
        );
        assert!(!mismatched.entry.file_name_is_lower_name);
        assert!(
            engine.lower_file_names[0].is_some(),
            "migemo={migemo_enabled}: 共有しないエントリの文字列を落としてはならない"
        );

        let shared = engine.entry_view(1);
        assert_eq!(
            shared.lower_file_name,
            Some("same"),
            "migemo={migemo_enabled}"
        );
        assert!(shared.entry.file_name_is_lower_name);
        assert!(
            engine.lower_file_names[1].is_none(),
            "migemo={migemo_enabled}: 共有するエントリの `Box<str>` は落ちていなければ削減にならない"
        );
    }
}

/// **共有は鎖になっている**: `lower_file_names[i]` → `lower_names[i]` → `entries[i].name`。
/// 両方の輪が同時に外れるエントリ（既に小文字の folder）で、`entry_view` が鎖を上から解いて
/// 元のバイトを返すことを固定する。
///
/// **鎖の順序を誤ると削減だけが減り、結果は正しいまま**である——`assemble` が先に
/// `lower_names` を潰すと file name 側の比較相手が消えて共有を取りこぼすが、`entry_view` は
/// どちらでも正しい値を返すので**挙動だけを見るテストでは捕まらない**。ゆえに索引が実際に
/// 文字列を落としたか（`is_none`）まで見る。
#[test]
fn shared_lower_name_chain_collapses_both_links() {
    let entries = vec![
        // 既に小文字の folder: `lower_name == name` かつ `lower_file_name == lower_name`。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
        // 大文字始まりの folder: `lower_name != name` だが `lower_file_name == lower_name`。
        AppEntry {
            name: "Alias".to_string(),
            target_path: "C:\\real\\Alias".to_string(),
            is_folder: true,
        },
    ];

    for migemo_enabled in [false, true] {
        let engine = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled);

        let both = engine.entry_view(0);
        assert_eq!(both.lower_name, "same", "migemo={migemo_enabled}");
        assert_eq!(
            both.lower_file_name,
            Some("same"),
            "migemo={migemo_enabled}"
        );
        assert!(
            engine.lower_names[0].is_none(),
            "migemo={migemo_enabled}: name と同一なら鎖の上段が落ちる"
        );
        assert!(
            engine.lower_file_names[0].is_none(),
            "migemo={migemo_enabled}: 上段が落ちても下段は落ちなければならない（順序の検知器）"
        );

        let folded = engine.entry_view(1);
        assert_eq!(folded.lower_name, "alias", "migemo={migemo_enabled}");
        assert_eq!(
            folded.lower_file_name,
            Some("alias"),
            "migemo={migemo_enabled}"
        );
        assert!(
            engine.lower_names[1].is_some(),
            "migemo={migemo_enabled}: 小文字化で変わる名前は自前の文字列を持つ"
        );
        assert!(
            engine.lower_file_names[1].is_none(),
            "migemo={migemo_enabled}: 上段が残っていても下段は共有できる"
        );
    }
}

/// 共有による読み替えが、潰す前の導出と 1 バイトも違わないことを実インデックスの全件で確かめる。
///
/// **共有は最適化であって意味の変更ではない。** `entry_view` が返す 2 つの文字列は、索引が
/// それを持っていようといまいと導出（`to_lower_folded` / `query::lower_file_name`）と
/// 一致しなければならない——ずれると `lower_name` はタイブレークの並びが、`lower_file_name` は
/// `has_dot` クエリの拡張子マッチが**静かに**変わる（クラッシュせず、順位だけが動く）。
/// 実インデックスが無ければ自動スキップする corpus であり、機構としての保証は上の合成
/// fixture のほうが持つ（`real_index_entries` の doc）。
#[test]
fn entry_view_shared_strings_match_derivation_over_real_index() {
    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    // `new_with_migemo` は entries を消費するので、期待値は先に導出しておく。
    let expected: Vec<(String, Option<String>)> = entries
        .iter()
        .map(|e| {
            (
                crate::query::to_lower_folded(&e.name),
                crate::query::lower_file_name(&e.target_path),
            )
        })
        .collect();
    let engine = SearchEngine::new_with_migemo(entries, false);

    let (mut shared_name, mut shared_file_name) = (0usize, 0usize);
    for (i, (want_name, want_file_name)) in expected.iter().enumerate() {
        let view = engine.entry_view(i);
        assert_eq!(
            view.lower_name, want_name,
            "index {i} で lower_name の読み替えが導出とずれている"
        );
        assert_eq!(
            view.lower_file_name,
            want_file_name.as_deref(),
            "index {i} で lower_file_name の読み替えが導出とずれている"
        );
        shared_name += usize::from(engine.lower_names[i].is_none());
        shared_file_name += usize::from(view.entry.file_name_is_lower_name);
    }
    let pct = |k: usize| k as f64 * 100.0 / expected.len().max(1) as f64;
    println!(
        "{} 件で読み替えが導出と一致しました（lower_name の共有 {shared_name} 件・{:.1}% / \
         lower_file_name の共有 {shared_file_name} 件・{:.1}%）。",
        expected.len(),
        pct(shared_name),
        pct(shared_file_name),
    );
}
