//! incremental search キャッシュ（prefix 拡張・backspace・mode 変更・dot 遷移）のテスト。

use super::common::{empty_history, make_entries, pool_of};
use crate::indexer::AppEntry;
use crate::search::*;

#[test]
fn incremental_search_gives_correct_results_on_extension() {
    let names = &["Firefox", "Final Cut", "Chrome", "Finder", "Fire TV"];
    let mut engine = SearchEngine::new(make_entries(names));
    let h = empty_history();

    // 連続する monotonic 拡張で incremental パスを使う
    let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
    let _ = engine.search("fir", 8, &h, SearchMode::Fuzzy);
    let incremental = engine.search("fire", 8, &h, SearchMode::Fuzzy);

    // 新鮮なエンジンでの結果と一致するか確認
    let mut fresh = SearchEngine::new(make_entries(names));
    let fresh_result = fresh.search("fire", 8, &h, SearchMode::Fuzzy);

    let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
    let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(inc_names, fresh_names);
}

#[test]
fn incremental_search_fallback_on_backspace() {
    // "Firm" は fuzzy "fir" にマッチするが "fire" にはマッチしない（'e' がない）。
    // "fire" → "fir" はバックスペースなので incremental パスは使えない。
    // full scan にフォールバックしていなければ "Firm" が結果から漏れる。
    let mut engine = SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome", "Firm"]));
    let h = empty_history();

    let _ = engine.search("fire", 8, &h, SearchMode::Fuzzy);
    // "fir" は "fire" の拡張ではない → full scan にフォールバック
    let results = engine.search("fir", 8, &h, SearchMode::Fuzzy);

    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Firefox"),
        "full scan で Firefox が返る必要がある"
    );
    assert!(
        names.contains(&"Firm"),
        "Firm は 'fire' にマッチしないため前回の candidates に不在。full scan でのみ検出可能"
    );
}

#[test]
fn incremental_search_fallback_on_mode_change() {
    let mut engine = SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome"]));
    let h = empty_history();

    let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
    // 同じクエリでもモード変更 → full scan
    let results = engine.search("fi", 8, &h, SearchMode::Prefix);

    // Prefix "fi": "firefox" / "final cut" は先頭一致、"chrome" は不一致
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Firefox"),
        "Prefix 'fi' で Firefox が返る必要がある"
    );
    assert!(
        names.contains(&"Final Cut"),
        "Prefix 'fi' で Final Cut が返る必要がある"
    );
    assert!(
        !names.contains(&"Chrome"),
        "Chrome は 'fi' で始まらないため除外される必要がある"
    );
}

#[test]
fn incremental_search_dot_to_dot_uses_incremental() {
    // dot → dot の拡張は incremental パスを使用できる（no-dot→dot ガードを通過する）。
    // "ssp." → "ssp.e" はどちらもドットを含む単調拡張。
    let entries = vec![
        AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "AnotherApp".to_string(),
            target_path: "C:\\fake\\ssp.data".to_string(),
            is_folder: false,
        },
    ];
    let mut engine = SearchEngine::new(entries.clone());
    let h = empty_history();

    // "ssp." で両エントリが候補にキャッシュされる（prev_candidates に両インデックスが入る）
    let _ = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
    // "ssp.e" はドットあり拡張 → incremental で prev_candidates を再利用
    let incremental = engine.search("ssp.e", 8, &h, SearchMode::Fuzzy);

    // fresh エンジンとの比較で正確性を担保
    let mut fresh = SearchEngine::new(entries);
    let fresh_result = fresh.search("ssp.e", 8, &h, SearchMode::Fuzzy);

    let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
    let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        inc_names, fresh_names,
        "dot→dot incremental は fresh 結果と一致する必要がある"
    );
}

#[test]
fn incremental_search_no_dot_to_dot_falls_back_to_full_scan() {
    // "AnotherApp" は名前では "ssp" にマッチしないが、
    // file_name "ssp.data" は "ssp." クエリにマッチする
    let entries = vec![
        AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "AnotherApp".to_string(),
            target_path: "C:\\fake\\ssp.data".to_string(),
            is_folder: false,
        },
    ];
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();

    // "ssp"（ドットなし）→ prev_candidates には名前マッチした SSP だけが入る
    let _ = engine.search("ssp", 8, &h, SearchMode::Fuzzy);

    // "ssp."（ドットあり）→ no-dot→dot ガードにより full scan
    // AnotherApp の file_name "ssp.data" が "ssp." にマッチするはず
    let results = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"SSP"), "SSP.exe が ssp. にマッチするはず");
    assert!(
        names.contains(&"AnotherApp"),
        "AnotherApp の file_name ssp.data が ssp. にマッチするはず（full scan 必須）"
    );
}

/// パスクエリの収集停止（#1070）を突くための 2 件 fixture。
///
/// - `Chrome` — クエリ `"c"` に**名前で**マッチする。`target_path` は `D:` 配下ゆえ
///   `"c:\"` には**マッチしない**
/// - `Zephyr` — 名前にも file_name にも `c` を含まず、`target_path` だけが `c:\` 配下にある
///
/// この非対称が、read 側（`can_reuse`）だけ述語を落とす変異を観測可能にする
/// ——`"c"` の候補（`Chrome` のみ）を `"c:\"` で再利用すると `Zephyr` が欠ける。
fn path_drift_entries() -> Vec<AppEntry> {
    vec![
        AppEntry {
            name: "Chrome".to_string(),
            target_path: "D:\\apps\\Chrome.lnk".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "Zephyr".to_string(),
            target_path: "C:\\tools\\zephyr.lnk".to_string(),
            is_folder: false,
        },
    ]
}

#[test]
fn path_query_leaves_the_incremental_candidates_empty() {
    // #1070 の新しい不変条件そのもの: パス区切りを含むクエリでは全一致 index を集めない。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let results = engine.search("c:\\", 8, &h, SearchMode::Fuzzy);

    // マッチ 0 件では「空」が自明に成立して述語反転の変異を検出できない。
    // 収集が起きたなら非空になる状態であることを先に固定する。
    assert_eq!(
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Zephyr"],
        "パスクエリは Zephyr にマッチする（収集が起きれば候補は非空になる状態）"
    );
    assert!(
        engine.incremental_cache.prev_candidates.is_empty(),
        "パスクエリでは candidate index を収集しない（読み手 can_reuse が !has_path_sep を要求するため）"
    );
}

#[test]
fn non_path_query_still_populates_the_incremental_candidates() {
    // 逆向き: 述語の反転・収集の無条件停止を捕まえる。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let results = engine.search("c", 8, &h, SearchMode::Fuzzy);

    assert_eq!(
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Chrome"],
        "区切りを含まないクエリは名前で Chrome にマッチする"
    );
    assert!(
        !engine.incremental_cache.prev_candidates.is_empty(),
        "区切りを含まないクエリでは今までどおり全一致 index を収集する（incremental の再利用元）"
    );
}

#[test]
fn path_query_results_are_identical_to_a_fresh_engine() {
    // read 側（can_reuse）だけ述語を落とすドリフトを捕まえる唯一の形。
    // 打鍵列 "c" → "c:\" は can_reuse の他条件（prev_mode 一致 / prev_candidates 非空 /
    // starts_with / dot 単調性 / kana 単調性）を**全部満たす**ため、has_path_sep だけが
    // 分岐点になる。既存の path_match_incremental_cache_monotonic は 2 打鍵ともパスクエリで、
    // 2 回目は自分自身の has_path_sep で必ず落ちるのでこの差を見ない。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let first = engine.search("c", 8, &h, SearchMode::Fuzzy);
    assert!(!first.is_empty(), "1 打鍵目が候補を残す前提を固定する");
    let incremental = engine.search("c:\\", 8, &h, SearchMode::Fuzzy);

    let mut fresh = SearchEngine::new(path_drift_entries());
    let fresh_result = fresh.search("c:\\", 8, &h, SearchMode::Fuzzy);

    let inc: Vec<(&str, &str)> = incremental
        .iter()
        .map(|r| (r.name.as_str(), r.path.as_str()))
        .collect();
    let expected: Vec<(&str, &str)> = fresh_result
        .iter()
        .map(|r| (r.name.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        inc, expected,
        "非パス → パスの遷移でも、結果は新品 engine と順序込みで一致する"
    );
}

#[test]
fn incremental_search_empty_prev_candidates_falls_back() {
    let mut engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
    let h = empty_history();

    // マッチなし → prev_candidates が空になる
    let r1 = engine.search("xyz", 8, &h, SearchMode::Fuzzy);
    assert!(r1.is_empty());

    // "xyzw" は "xyz" の拡張だが prev_candidates 空 → full scan
    let r2 = engine.search("xyzw", 8, &h, SearchMode::Fuzzy);
    assert!(r2.is_empty());

    // キャッシュが壊れていなければ通常クエリも機能する
    let r3 = engine.search("fire", 8, &h, SearchMode::Fuzzy);
    assert!(!r3.is_empty());
    assert_eq!(r3[0].name, "Firefox");
}

/// 分割の容量テスト 2 本の件数（2 の冪の倍数にしない理由は下のテストの doc）。
const SPLIT_FIXTURE_LEN: usize = 5_000;

// **分割が起きることは件数の前提でしか守れない**——葉の数は外から見えないので、
// `MIN_PAR_CANDIDATES` が上がって葉が 1 枚になると、join も葉ごとの予約も走らないまま 2 本とも
// 緑になる。その前提をコンパイル時に固定する。
const _: () = assert!(
    SPLIT_FIXTURE_LEN >= 2 * MIN_PAR_CANDIDATES,
    "SPLIT_FIXTURE_LEN が分割の閾値に届いていない"
);

/// 分割が起きる規模で、次の打鍵まで残る前回候補の容量が長さを大きく超えない。
///
/// 回帰テスト: reduce の join で `extend` の前に `reserve_exact` しないと、葉が倍々で伸ばした
/// `Vec` がそのまま左辺になり、根で 2 の冪まで膨らむ（#1280。額と、膨らんだ版の壁時計は
/// `PERFORMANCE.md`「採用: 検索の rayon の葉ごとの確保を減らす（#1280）」）。**確保回数では捕まらない**——
/// その誤りは回数をむしろ減らす。ゆえに容量そのものを見る。**壁時計の機序を固定するテストではない**
/// （容量だけを縮めても退行は消えなかった）。同じ誤りが残す観測点を CI に置くためにある。
///
/// **件数は 2 の冪にしない。** 葉の長さが 2 の冪だと倍々の成長がちょうど長さに一致し、誤りが
/// 容量に現れない（`8 * MIN_PAR_CANDIDATES` では緑のままだった。誤った版でのこの件数の容量は同じ節）。
/// 分割が起きる前提は [`SPLIT_FIXTURE_LEN`] の直後の `const` assert が固定する。
#[test]
fn split_search_keeps_candidates_capacity_close_to_length() {
    let n = SPLIT_FIXTURE_LEN;
    let owned: Vec<String> = (0..n).map(|i| format!("alpha {i:05}")).collect();
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut engine = SearchEngine::new(make_entries(&names));
    let h = empty_history();
    pool_of(4).install(|| engine.search("al", 10, &h, SearchMode::Fuzzy));
    let prev = &engine.incremental_cache.prev_candidates;
    assert_eq!(prev.len(), n, "全件が一致する前提を固定する");
    assert!(
        prev.capacity() * 4 < prev.len() * 5,
        "前回候補の容量 {} が長さ {} の 1.25 倍以上ある",
        prev.capacity(),
        prev.len()
    );
    // join は左右の順序を保つ（左辺へ右辺を足す）。逆に足しても検索結果は変わらないので、
    // 並びを固定するのはここだけである。
    assert!(
        prev.windows(2).all(|w| w[0] < w[1]),
        "前回候補が添字の昇順に並んでいない"
    );
}

/// 分割が起きる規模のパスクエリで、前回候補を確保しない。
///
/// 回帰テスト: パスクエリは前回候補を集めない（#1070・`IncrementalCache::caches_candidates`）。
/// 葉の予約（#1280）を収集の条件の外へ出すと、集めないのに葉ごとに確保し、その容量が空の
/// `prev_candidates` として次の打鍵まで残る。検索結果は変わらず、パスクエリを打つ確保の検知器
/// （`tests/search_allocations.rs` の `path_query_full_scan_does_not_allocate_per_entry`）も、この誤りを
/// 入れて緑のままだった（#1280 の変異注入）。ゆえに容量で見る。
#[test]
fn path_query_leaves_no_candidates_capacity_when_split() {
    let owned: Vec<String> = (0..SPLIT_FIXTURE_LEN)
        .map(|i| format!("alpha {i:05}"))
        .collect();
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut engine = SearchEngine::new(make_entries(&names));
    let h = empty_history();
    let results = pool_of(4).install(|| engine.search("c:/fake/", 10, &h, SearchMode::Fuzzy));
    assert!(!results.is_empty(), "パスクエリが一致する前提を固定する");
    let prev = &engine.incremental_cache.prev_candidates;
    assert_eq!(
        (prev.len(), prev.capacity()),
        (0, 0),
        "パスクエリで前回候補を確保した"
    );
}

/// 分割されない規模では、葉の予約が候補の件数を超えない。
///
/// 回帰テスト: 葉の予約（#1280）は、分割されれば葉が候補を `MIN_PAR_CANDIDATES` 件以上持つことに
/// 頼っている。分割されない規模（打鍵が進んで前回候補が少ない再利用など）では葉は候補の全件だけを
/// 持つので、固定の件数で予約すると、一致が数件でもその大きさを取って次の打鍵まで残す。
#[test]
fn unsplit_search_does_not_reserve_beyond_the_candidates() {
    let n = MIN_PAR_CANDIDATES / 4;
    let owned: Vec<String> = (0..n).map(|i| format!("alpha {i:05}")).collect();
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut engine = SearchEngine::new(make_entries(&names));
    let _ = engine.search("al", 10, &empty_history(), SearchMode::Fuzzy);
    let prev = &engine.incremental_cache.prev_candidates;
    assert_eq!(prev.len(), n, "全件が一致する前提を固定する");
    assert!(
        prev.capacity() <= n,
        "候補 {n} 件の検索で前回候補を容量 {} まで予約した",
        prev.capacity()
    );
}

/// 前回候補を再利用する検索が、rayon の分割（`MIN_PAR_CANDIDATES`）が起きる規模でも
/// 新品の engine と順序込みで一致する。
///
/// 回帰テスト: 再利用時の走査は添字の範囲を回し、`map` の中で前回候補へ読み替える
/// （`search_with_options` のコメント）。`incremental_search_gives_correct_results_on_extension` の
/// fixture は分割が起きる規模に届かないので、
/// **件数に依存する誤り**（前回候補を途中で打ち切る等）を見られない——このテストが単独で捕まえる
/// のはその型である。**見えるのは、取りこぼしが top-k の選ばれる前回候補の末尾に掛かるときである**
/// （分割の境目の 1 件だけを落とす誤りは、その候補が top-k の外なら結果を変えず、ここでも緑になる）。
///
/// **fixture の形が検出力を決める。** 全件が 1 打鍵目に一致すると前回候補が `0..n` そのものに
/// なり、読み替えを外しても結果が変わらない——ゆえに 3 件に 1 件を一致させず、添字と位置を
/// ずらす。名前は添字が大きいほど辞書順で前に来るので、top-k は前回候補の**末尾**から選ばれ、
/// 範囲を素で回す誤りはちょうどそこを取りこぼす。
#[test]
fn incremental_reuse_matches_a_fresh_engine_when_the_candidates_split() {
    let n = 4 * MIN_PAR_CANDIDATES;
    // `omega` は "al" に一致しない（`l` を持たない）。
    let owned: Vec<String> = (0..n)
        .map(|i| {
            let stem = if i % 3 == 0 { "omega" } else { "alpha" };
            format!("{stem} {:05}", n - i)
        })
        .collect();
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let h = empty_history();
    let mut engine = SearchEngine::new(make_entries(&names));

    let _ = engine.search("al", 200, &h, SearchMode::Fuzzy);
    let prev = &engine.incremental_cache.prev_candidates;
    assert!(
        prev.len() > 2 * MIN_PAR_CANDIDATES,
        "前回候補が分割の閾値を超える前提を固定する（{} 件）",
        prev.len()
    );
    assert!(
        prev.iter().enumerate().any(|(j, &i)| i != j),
        "前回候補の添字が位置とずれている前提を固定する（ずれが無いと読み替えを外しても結果が変わらない）"
    );
    // 2 打鍵目で再利用の判定（`IncrementalCache::can_reuse`）が真であることを固定する。この assert が
    // 無ければ、判定が偽のときは全件走査どうしの比較になり、何も検査しないまま緑になる。**固定するのは
    // 判定であって経路ではない**——呼び出し側が判定を無視して全件走査する退行は結果を変えないので、
    // このテストも見ない（受容する残余）。
    let plan = prepare_query_plan("alp", SearchMode::Fuzzy, &SearchOptions::default())
        .expect("空でないクエリは計画を返す");
    assert!(
        engine.incremental_cache.can_reuse(&plan, SearchMode::Fuzzy),
        "2 打鍵目で再利用の判定が真である前提を固定する"
    );
    let reused = engine.search("alp", 200, &h, SearchMode::Fuzzy);

    let fresh = SearchEngine::new(make_entries(&names)).search("alp", 200, &h, SearchMode::Fuzzy);

    assert_eq!(
        reused, fresh,
        "前回候補を再利用した結果が、新品 engine の全件走査と順序込みで一致しない"
    );
}
