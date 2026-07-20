//! migemo（ローマ字→かな）検索・単調性・条件付き kana 構築のテスト。

use super::common::{empty_history, make_entries};
use crate::indexer::AppEntry;
use crate::search::*;

fn migemo_config() -> SearchOptions {
    SearchOptions {
        migemo_enabled: true,
        migemo_min_chars: 2,
        ..SearchOptions::default()
    }
}

#[test]
fn kana_search_disabled_by_default() {
    // migemo_enabled=false（デフォルト）では "dokyu" で "ドキュメント" がヒットしない
    let entries = make_entries(&["ドキュメント", "Documents"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let results = engine.search_with_options(
        "dokyu",
        8,
        &h,
        SearchMode::Substring,
        SearchOptions::default(), // migemo_enabled=false
    );
    assert!(
        results.is_empty(),
        "migemo 無効時は 'dokyu' でカタカナ名がヒットしてはならない"
    );
}

#[test]
fn kana_search_matches_katakana_entry() {
    // "dokyu" で "ドキュメント" がヒットする
    let entries = make_entries(&["ドキュメント", "Documents"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let results =
        engine.search_with_options("dokyu", 8, &h, SearchMode::Substring, migemo_config());
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"ドキュメント"),
        "'dokyu' で ドキュメント がヒットするはず: {:?}",
        names
    );
}

#[test]
fn fuzzy_kana_match_skips_latin_bitmask_prefilter() {
    // "chatto" と "tyatto" は kana に正規化するとどちらも「ちゃっと」になる。
    // 直接のラテン文字集合は異なるため、Fuzzy の bitmask pre-filter は
    // kana_query がある経路を早期棄却してはならない。
    let mut engine = SearchEngine::new(make_entries(&["tyatto"]));
    let plan = prepare_query_plan("chatto", SearchMode::Fuzzy, &migemo_config())
        .expect("非空クエリは QueryPlan を生成する");
    let kana_query_mask = plan
        .kana_query_mask
        .expect("完全に kana へ変換できるクエリは kana mask を持つ");

    assert_ne!(
        plan.query_mask & engine.char_masks[0],
        plan.query_mask,
        "tyatto はラテン文字 mask では chatto を満たさない"
    );
    assert_eq!(
        kana_query_mask & engine.kana_char_masks[0],
        kana_query_mask,
        "kana mask は chatto と tyatto の共通 kana を通す"
    );
    let results = engine.search_with_options(
        "chatto",
        8,
        &empty_history(),
        SearchMode::Fuzzy,
        migemo_config(),
    );

    assert_eq!(results.len(), 1, "kana 経由で tyatto がヒットするはず");
    assert_eq!(results[0].name, "tyatto");
}

#[test]
fn kana_search_no_false_positive_for_ascii_entry() {
    // "dokyu" で "Documents" はヒットしない
    let entries = make_entries(&["ドキュメント", "Documents"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let results =
        engine.search_with_options("dokyu", 8, &h, SearchMode::Substring, migemo_config());
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        !names.contains(&"Documents"),
        "'dokyu' で ASCII エントリ Documents がヒットしてはならない: {:?}",
        names
    );
}

#[test]
fn kana_search_direct_match_ranks_above_kana_match() {
    // 直接 Substring マッチ(5000) が kana マッチ(4500) より常に上位に来ることを確認する。
    // "どきゅめんと"（ひらがな名）を含むエントリに対して:
    //   - "どきゅ"（直接クエリ）→ Substring スコア 5000
    //   - "dokyu"（kana クエリ）→ kana_substring_score 4500
    // 直接クエリの方が kana クエリより高スコアになる。
    let entries = vec![
        AppEntry {
            name: "どきゅめんと".to_string(),
            target_path: "C:\\fake\\a.lnk".to_string(),
            is_folder: false,
        },
        // kana マッチ専用エントリ（直接クエリでは name がひらがなのみなのでマッチしない）
        AppEntry {
            name: "ドキュメント".to_string(),
            target_path: "C:\\fake\\b.lnk".to_string(),
            is_folder: false,
        },
    ];
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();

    // "どきゅめんと" に対して直接 Substring マッチしたスコアは 5000（先頭一致）
    let direct =
        engine.search_with_options("どきゅ", 8, &h, SearchMode::Substring, migemo_config());

    // "dokyu" → kana_query = "どきゅ" → kana_substring_score = 4500
    let kana = engine.search_with_options("dokyu", 8, &h, SearchMode::Substring, migemo_config());

    // 両方の結果が存在することを確認
    assert!(
        !direct.is_empty(),
        "直接クエリで どきゅめんと がヒットするはず"
    );
    assert!(
        !kana.is_empty(),
        "kana クエリで どきゅめんと / ドキュメント がヒットするはず"
    );

    // 直接クエリで "どきゅめんと" がトップに来る（Substring の先頭一致 = 高スコア）
    assert_eq!(
        direct[0].name, "どきゅめんと",
        "直接クエリ先頭一致は どきゅめんと が最高スコア"
    );

    // kana マッチのスコア(4500) < 直接マッチのスコア(5000) の確認:
    // 同じエントリが両クエリでヒットするとき、直接クエリ側の方が先頭になる。
    // "どきゅめんと" は直接 Substring(5000) でも kana_substring(4500) でもヒットするが、
    // 直接クエリ時は primary_score が Some なので kana_score は使われない（OR 関係）。
    // よって primary_score（5000）> kana_score（4500）の順序不変条件が成立する。
    let direct_names: Vec<&str> = direct.iter().map(|r| r.name.as_str()).collect();
    let kana_names: Vec<&str> = kana.iter().map(|r| r.name.as_str()).collect();
    assert!(
        direct_names.contains(&"どきゅめんと"),
        "直接クエリで どきゅめんと がヒットするはず: {:?}",
        direct_names
    );
    assert!(
        kana_names.contains(&"どきゅめんと") || kana_names.contains(&"ドキュメント"),
        "kana クエリで少なくとも一方がヒットするはず: {:?}",
        kana_names
    );
}

#[test]
fn kana_search_min_chars_blocks_short_query() {
    // min_chars=2 のとき 1文字クエリ "a" → "あ" はヒットしない
    let entries = make_entries(&["あいうえお"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let config = SearchOptions {
        migemo_enabled: true,
        migemo_min_chars: 2,
        ..SearchOptions::default()
    };
    let results = engine.search_with_options("a", 8, &h, SearchMode::Substring, config);
    assert!(
        results.is_empty(),
        "1文字クエリは migemo_min_chars=2 でブロックされるはず"
    );
}

#[test]
fn kana_search_partial_romaji_not_used() {
    // "dok" → "どk"（'k' が残る） → kana_query が None になりヒットしない
    let entries = make_entries(&["ドキュメント"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let results = engine.search_with_options("dok", 8, &h, SearchMode::Substring, migemo_config());
    // "dok" は部分ローマ字で変換後に 'k' が残るため kana_query=None
    // ドキュメント は Substring("dok") にも直接マッチしないため空
    assert!(
        results.is_empty(),
        "部分ローマ字 'dok' では kana マッチもしないはず: {:?}",
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn incremental_kana_min_chars_crossing_does_not_lose_kana_match() {
    // "d" (min_chars=2 未満 → kana_query=None) → "do" (min_chars=2 到達 → kana_query=Some("ど"))
    // 前回の prev_candidates には kana マッチが含まれていない。
    // full scan にフォールバックしなければ "ドキュメント" が欠落する。
    let entries = make_entries(&["ドキュメント", "Discord"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let cfg = migemo_config();

    // "d" → kana_query=None（min_chars=2 未満）
    let _ = engine.search_with_options("d", 8, &h, SearchMode::Substring, cfg);

    // "do" → kana_query=Some("ど")。前回 prev_kana_query=None なので full scan。
    let results = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"ドキュメント"),
        "min_chars 超え遷移で ドキュメント がヒットするはず（full scan フォールバック必須）: {:?}",
        names
    );
}

#[test]
fn incremental_kana_ascii_residue_clearing_does_not_lose_kana_match() {
    // "dok" (ASCII 残留 'k' → kana_query=None) → "dokyu" (完全変換 → kana_query=Some("どきゅ"))
    // prev_candidates に kana マッチが含まれていないため full scan が必要。
    let entries = make_entries(&["ドキュメント", "Docking Station"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let cfg = migemo_config();

    // "dok" → kana_query=None（"どk" に ASCII 残留）
    let _ = engine.search_with_options("dok", 8, &h, SearchMode::Substring, cfg);

    // "dokyu" → kana_query=Some("どきゅ")。prev_kana_query=None なので full scan。
    let results = engine.search_with_options("dokyu", 8, &h, SearchMode::Substring, cfg);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"ドキュメント"),
        "ASCII 残留解消遷移で ドキュメント がヒットするはず（full scan フォールバック必須）: {:?}",
        names
    );
}

#[test]
fn incremental_kana_to_kana_reuses_cache() {
    // "do" (kana_query=Some) → "doku" (kana_query=Some) は incremental で正しく動作する。
    // kana→kana の拡張なので prev_candidates を再利用して問題ない。
    let entries = make_entries(&["ドキュメント", "Discord", "Chrome"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let cfg = migemo_config();

    let _ = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
    let incremental = engine.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

    // fresh エンジンとの比較で正確性を担保
    let mut fresh = SearchEngine::new(make_entries(&["ドキュメント", "Discord", "Chrome"]));
    let fresh_result = fresh.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

    let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
    let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        inc_names, fresh_names,
        "kana→kana incremental は fresh 結果と一致する必要がある"
    );
}

#[test]
fn incremental_kana_non_monotonic_n_vowel_falls_back() {
    // "kan" → kana="かん", "kana" → kana="かな"
    // ローマ字→かな変換は非単調: 末尾の「ん」が「な」に変わる。
    // "かな" は "かん" の prefix 拡張ではないため incremental は使えない。
    // "かなめ" のようなエントリは "かん" でヒットしないが "かな" でヒットする。
    let entries = make_entries(&["かなめ", "かんな", "Kanata"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let cfg = migemo_config();

    // "kan" → kana_query=Some("かん")
    let r1 = engine.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);
    let names1: Vec<&str> = r1.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names1.contains(&"かんな"),
        "\"kan\"(かん) で「かんな」がヒットするはず: {:?}",
        names1
    );

    // "kana" → kana_query=Some("かな")。"かな" は "かん" の prefix ではない → full scan。
    let r2 = engine.search_with_options("kana", 8, &h, SearchMode::Substring, cfg);
    let names2: Vec<&str> = r2.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names2.contains(&"かなめ"),
        "非単調遷移 kan→kana で「かなめ」がヒットするはず（full scan フォールバック必須）: {:?}",
        names2
    );
}

#[test]
fn incremental_kana_monotonic_extension_reuses_cache() {
    // "ka" → kana="か", "kan" → kana="かん"
    // "かん" は "か" の prefix 拡張 → incremental が使える。
    let entries = make_entries(&["かなめ", "かんな", "Chrome"]);
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();
    let cfg = migemo_config();

    let _ = engine.search_with_options("ka", 8, &h, SearchMode::Substring, cfg);
    let incremental = engine.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);

    let mut fresh = SearchEngine::new(make_entries(&["かなめ", "かんな", "Chrome"]));
    let fresh_result = fresh.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);

    let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
    let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        inc_names, fresh_names,
        "か→かん は単調拡張なので incremental は fresh と一致するはず"
    );
}

#[test]
fn migemo_disabled_build_leaves_kana_empty() {
    // migemo OFF で構築 → kana 未構築。その後 migemo ON で検索しても
    // panic せず、kana マッチは出ない（空ガード）。
    let entries = make_entries(&["ドキュメント", "Documents"]);
    let mut engine = SearchEngine::new_with_migemo(entries, false);
    assert!(engine.kana_lower_names.is_empty());
    assert!(engine.kana_char_masks.is_empty());
    let results = engine.search_with_options(
        "dokyu",
        8,
        &empty_history(),
        SearchMode::Substring,
        migemo_config(),
    );
    assert!(
        results.is_empty(),
        "kana 未構築時は migemo 検索が空（panic せず）: {:?}",
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn migemo_enabled_build_matches_kana() {
    // migemo ON で構築 → kana 構築済み → ローマ字検索がヒット
    let entries = make_entries(&["ドキュメント", "Documents"]);
    let mut engine = SearchEngine::new_with_migemo(entries, true);
    assert_eq!(engine.kana_lower_names.len(), engine.entries.len());
    assert_eq!(engine.kana_char_masks.len(), engine.entries.len());
    let results = engine.search_with_options(
        "dokyu",
        8,
        &empty_history(),
        SearchMode::Substring,
        migemo_config(),
    );
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"ドキュメント"),
        "migemo ON 構築で ドキュメント がヒット: {:?}",
        names
    );
}

#[test]
fn kana_index_follows_migemo_on_off_on() {
    // on→off→on の各構築で kana 挙動が追従する（必須条件 #3）
    let names = &["ドキュメント"];
    fn romaji_hits(engine: &mut SearchEngine) -> bool {
        !engine
            .search_with_options(
                "dokyu",
                8,
                &empty_history(),
                SearchMode::Substring,
                SearchOptions {
                    migemo_enabled: true,
                    migemo_min_chars: 2,
                    ..SearchOptions::default()
                },
            )
            .is_empty()
    }
    let mut on1 = SearchEngine::new_with_migemo(make_entries(names), true);
    assert!(romaji_hits(&mut on1), "on(1): kana ヒットするはず");
    let mut off = SearchEngine::new_with_migemo(make_entries(names), false);
    assert!(!romaji_hits(&mut off), "off: kana 未構築でヒットしない");
    let mut on2 = SearchEngine::new_with_migemo(make_entries(names), true);
    assert!(romaji_hits(&mut on2), "on(2): 再構築で kana 復活");
}

#[test]
fn incremental_with_kana_disabled_build_no_panic() {
    // kana 空（migemo off 構築）+ 検索時 migemo on + incremental シーケンスでも
    // panic せず、kana-off fresh エンジンと一致する（cache-check 由来の追加テスト）。
    let names = &["ドキュメント", "Discord", "Chrome"];
    let cfg = migemo_config();
    let h = empty_history();
    let mut engine = SearchEngine::new_with_migemo(make_entries(names), false);
    let _ = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
    let inc = engine.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

    let mut fresh = SearchEngine::new_with_migemo(make_entries(names), false);
    let fresh_r = fresh.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

    let inc_names: Vec<&str> = inc.iter().map(|r| r.name.as_str()).collect();
    let fresh_names: Vec<&str> = fresh_r.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        inc_names, fresh_names,
        "kana 空 + incremental は fresh と一致"
    );
}
