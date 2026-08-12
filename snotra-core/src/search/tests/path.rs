//! パスマッチング（区切り正規化・履歴キー統一・incremental 無効化）のテスト。

use super::common::{empty_history, real_index_entries};
use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::search::*;

fn make_entry(name: &str, path: &str) -> AppEntry {
    AppEntry {
        name: name.to_string(),
        target_path: path.to_string(),
        is_folder: false,
    }
}

#[test]
fn path_match_substring_finds_entry_by_path_segment() {
    let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("tool\\editor", 8, &empty_history(), SearchMode::Substring);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "app");
}

#[test]
fn path_match_score_below_name_match() {
    // "editor" → name="editor" に Substring マッチ (score 5000系)
    //         → name="app" はマッチしない（path にも "editor" を含むがクエリにパス区切りなし）
    // パス区切りなしのクエリではパスマッチは試行されない
    let entries = vec![
        make_entry("editor", "C:\\tool\\editor\\editor.exe"),
        make_entry("app", "C:\\tool\\editor\\app.exe"),
    ];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("editor", 8, &empty_history(), SearchMode::Substring);
    // "editor" は name マッチ、"app" は name にもパスにも（パス区切りなしで試行されない）マッチしない
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "editor");

    // パス区切りありのクエリで比較: "tool\\editor" は両方のパスにマッチするが、
    // "editor" は name にも Substring マッチ → name_score(5000系) > path_score(3000系)
    let entries2 = vec![
        make_entry("editor", "C:\\tool\\editor\\editor.exe"),
        make_entry("app", "C:\\tool\\editor\\app.exe"),
    ];
    let mut engine2 = SearchEngine::new(entries2);
    let results2 = engine2.search("tool\\editor", 8, &empty_history(), SearchMode::Substring);
    assert_eq!(results2.len(), 2);
    // "editor" の lower_name は "editor"。クエリ "tool\editor" に対して
    // Substring マッチ: "editor".find("tool\editor") → None（クエリが長い）
    // 両方ともパスマッチのみ → path_score で順序が決まる
    // path_score は byte_position で比較 — 同じパスプレフィックスなのでスコア同等
    // タイブレーク: lower_name 昇順 → "app" < "editor"
    assert_eq!(results2[0].name, "app");
    assert_eq!(results2[1].name, "editor");
}

#[test]
fn path_match_slash_normalized() {
    let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
    let mut engine = SearchEngine::new(entries);
    // `/` で入力しても `\` に正規化されてマッチする
    let results = engine.search("tool/editor", 8, &empty_history(), SearchMode::Substring);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "app");
}

#[test]
fn path_match_receives_history_boost() {
    let entries = vec![
        make_entry("app1", "C:\\tool\\editor\\app1.exe"),
        make_entry("app2", "C:\\tool\\editor\\app2.exe"),
    ];
    let mut history = HistoryStore::empty();
    for _ in 0..5 {
        history.record_launch("C:\\tool\\editor\\app1.exe", "");
    }
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("tool\\editor", 8, &history, SearchMode::Substring);
    assert_eq!(results.len(), 2);
    // app1 は history boost で上位
    assert_eq!(results[0].name, "app1");
    assert_eq!(results[1].name, "app2");
}

#[test]
fn path_match_incremental_cache_monotonic() {
    let entries = vec![
        make_entry("app", "C:\\tool\\editor\\app.exe"),
        make_entry("other", "C:\\other\\other.exe"),
    ];
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();

    // 1回目: "tool\\" でパスマッチ
    let r1 = engine.search("tool\\", 8, &h, SearchMode::Substring);
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].name, "app");

    // 2回目: "tool\\ed" に拡張 → パス区切りを含むため incremental は無効化される
    //（IncrementalCache::can_reuse の !has_path_sep ガード）。fresh scan と一致することを検証。
    let r2 = engine.search("tool\\ed", 8, &h, SearchMode::Substring);
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0].name, "app");

    // 比較: fresh engine と同じ結果になること
    let mut fresh = SearchEngine::new(vec![
        make_entry("app", "C:\\tool\\editor\\app.exe"),
        make_entry("other", "C:\\other\\other.exe"),
    ]);
    let fresh_result = fresh.search("tool\\ed", 8, &h, SearchMode::Substring);
    assert_eq!(r2.len(), fresh_result.len());
    assert_eq!(r2[0].name, fresh_result[0].name);
}

#[test]
fn path_match_no_match_returns_empty() {
    let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("xyz\\abc", 8, &empty_history(), SearchMode::Substring);
    assert!(results.is_empty());
}

#[test]
fn path_match_fuzzy_mode_skips_bitmask_prefilter() {
    // name="zzz" はクエリ "tool\\editor" の文字 (t,o,l,e,d,i,r) を含まない
    // → 通常ならビットマスクで除外されるが、has_path_sep でスキップされパスマッチする
    let entries = vec![make_entry("zzz", "C:\\tool\\editor\\zzz.exe")];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("tool\\editor", 8, &empty_history(), SearchMode::Fuzzy);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "zzz");
}

#[test]
fn path_match_yen_sign_normalized() {
    // ¥（U+00A5）は日本語 Windows でバックスラッシュとして使われる
    let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search(
        "tool\u{00a5}editor",
        8,
        &empty_history(),
        SearchMode::Substring,
    );
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "app");
}

#[test]
fn path_match_consecutive_spaces_preserved() {
    // パス成分に連続スペースを含む場合、normalize_query() で潰されない
    let entries = vec![make_entry("app", "C:\\My  Tools\\app.exe")];
    let mut engine = SearchEngine::new(entries);
    let results = engine.search("My  Tools\\", 8, &empty_history(), SearchMode::Substring);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "app");
}

#[test]
fn path_match_incremental_disabled_avoids_accent_false_negative() {
    // incremental cache が有効だった場合の false negative を検証する。
    // entry path に "café" を含み、"cafe\\" (no accent) → "café\\" (with accent)
    // の遷移で norm_query は両方 "cafe\\" だが、path_query は異なる。
    // incremental 無効化により、full scan で正しくマッチする。
    let entries = vec![
        make_entry("app", "C:\\café\\app.exe"),
        make_entry("other", "C:\\other\\other.exe"),
    ];
    let mut engine = SearchEngine::new(entries);
    let h = empty_history();

    // 1回目: "cafe\\" — normalized_key は "c:\café\app.exe" (accent preserved)
    // path_query "cafe\\" は "café" にマッチしない
    let r1 = engine.search("cafe\\", 8, &h, SearchMode::Substring);
    assert!(r1.is_empty());

    // 2回目: "café\\" — path_query "café\\" は "café" にマッチすべき
    // incremental が有効だと前回の空結果を再利用して false negative になる
    let r2 = engine.search("café\\", 8, &h, SearchMode::Substring);
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0].name, "app");
}

#[test]
fn path_match_history_key_unified_across_separators() {
    // tool/editor と tool\editor で履歴バケットが統一される
    let entries = vec![
        make_entry("app1", "C:\\tool\\editor\\app1.exe"),
        make_entry("app2", "C:\\tool\\editor\\app2.exe"),
    ];
    let mut history = HistoryStore::empty();
    // tool/editor（スラッシュ）で起動記録
    history.record_launch("C:\\tool\\editor\\app1.exe", "tool/editor");

    let mut engine = SearchEngine::new(entries);
    // tool\editor（バックスラッシュ）で検索 → 履歴が効くべき
    let results = engine.search("tool\\editor", 8, &history, SearchMode::Substring);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "app1"); // history boost で上位
}

// --- PathCursor（祖先の鎖の持ち回り）の正しさ ---

/// 鎖の当たり外れを両方通す fixture。**兄弟の連続**（当たり）と**部分木の割り込み**
/// （外れ）を意図的に混ぜる。区切り `\`(0x5C) は `-`(0x2D) や `.`(0x2E) より大きいので、
/// `C:\a` の直後に `C:\a-x` が割り込んで `C:\a` が鎖から押し出される並びになる。
fn cursor_fixture() -> Vec<AppEntry> {
    let folders = [
        "C:\\a",
        "C:\\a-x",
        "C:\\a-x\\deep",
        "C:\\a-x\\deep\\deeper",
        "C:\\a.y",
        "C:\\a\\bin",
        "C:\\a\\bin\\sub",
        "C:\\a\\lib",
        "C:\\b",
        "C:\\b\\bin",
        // 親が集合に無い（側テーブル行き＝自分がフルパスを持つ）。
        "D:\\orphan\\deep\\dir",
    ];
    let files = [
        "C:\\a-x\\deep\\deeper\\tool.exe",
        "C:\\a\\bin\\tool.exe",
        "C:\\a\\lib\\tool.dll",
        "C:\\b\\bin\\tool.exe",
    ];
    let mut entries: Vec<AppEntry> = folders
        .iter()
        .map(|p| AppEntry {
            name: p.rsplit('\\').next().unwrap().to_string(),
            target_path: (*p).to_string(),
            is_folder: true,
        })
        .chain(files.iter().map(|p| {
            let tail = p.rsplit('\\').next().unwrap();
            AppEntry {
                name: tail.rsplit_once('.').unwrap().0.to_string(),
                target_path: (*p).to_string(),
                is_folder: false,
            }
        }))
        .collect();
    entries.sort_by(|a, b| a.target_path.cmp(&b.target_path));
    entries
}

/// **カーソルは最適化であって意味の変更ではない。** 鎖の状態に依らない素直な組み立て
/// （`PathStore::normalized_into`）と 1 バイトも違わないことを、走査順を変えて固定する。
///
/// 3 通りを通すのが要点である——順方向は鎖が当たり続ける経路、逆順と乱順は毎回外れて
/// 全書き直しへ落ちる経路。**片方だけ通しても、巻き戻しの誤りか全書き直しの誤りかの
/// どちらかが隠れる。** 現物（`normalize_entry_key`）との一致も同時に確かめる。
#[test]
fn path_store_cursor_matches_full_rebuild() {
    use crate::indexer::normalize_entry_key;
    use crate::search::path_store::{PathCursor, PathStore};

    let entries = cursor_fixture();
    let expected: Vec<String> = entries
        .iter()
        .map(|e| normalize_entry_key(&e.target_path))
        .collect();
    let n = entries.len();
    let store = PathStore::build(entries);

    // 乱順は固定の擬似乱数（決定的でなければ失敗が再現しない）。
    let mut scrambled: Vec<usize> = (0..n).collect();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for i in (1..n).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        scrambled.swap(i, (seed % (i as u64 + 1)) as usize);
    }

    let orders: [(&str, Vec<usize>); 3] = [
        ("順方向", (0..n).collect()),
        ("逆順", (0..n).rev().collect()),
        ("乱順", scrambled),
    ];
    let mut full = String::new();
    for (label, order) in orders {
        // 走査ごとに鎖を空から始める（カーソルの寿命 = 1 走査）。
        let mut cursor = PathCursor::new();
        for i in order {
            store.normalized_into(&mut full, i);
            let via_cursor = cursor.normalized(&store, i).to_string();
            assert_eq!(
                via_cursor, full,
                "{label}: カーソルと素直な組み立てがずれた（index {i}）"
            );
            assert_eq!(
                via_cursor, expected[i],
                "{label}: 現物とずれた（index {i}）"
            );
        }
    }
}

/// 原文の再構築が `target_path` と 1 バイトも違わないことを、実データの全件で確かめる。
///
/// **正規化版の一致だけでは足りない**——tie-break の遅い経路（`PathStore::cmp_paths`）と
/// 表示パス（`SearchResult.path`）は原文のバイトに載る。1 バイトのずれは、同スコアの並びが
/// 入れ替わる・表示されたパスで開けない、という形で現れる。
///
/// **原文はファイルシステムを走査して取る。`index.bin` から取ってはならない。**
/// v7 の `index.bin` は `target_path` を持たないので、そこから読んだ値は既に
/// `raw_path_into` の出力である——それを組み直しと突き合わせても
/// **「組み直し対組み直し」の不動点を見るだけ**で、`raw_path_into` がどれだけ壊れていても
/// assert は落ちない（成功メッセージまで出る。実際にその形で一度書かれていた）。
/// スキャナの `target_path` だけが、木を 1 度も通っていない原文である。
///
/// **`#[ignore]` はそのコスト**（実測 75 秒の全走査）。実インデックスの有無ではなく
/// config の scan パスの有無で成否が決まるので、CI では走らせない。
#[test]
#[ignore = "実データ照合。全走査 75 秒ゆえ手元で明示実行する"]
fn path_store_raw_matches_target_path_over_real_index() {
    use crate::search::path_store::PathStore;

    // **原文はファイルシステムの走査から取る**（`index.bin` から取ると不動点になる）。
    // 走査・整列・非空の検算は `common::real_scanned_entries` が持つ——**規則の写しを
    // ここへ書かないこと**。
    let Some(entries) = super::common::real_scanned_entries() else {
        return;
    };

    // `build` は `entries` を消費するので、比較相手は先に取り分ける。
    let expected: Vec<String> = entries.iter().map(|e| e.target_path.clone()).collect();
    let store = PathStore::build(entries);
    let mut buf = String::new();
    for (i, want) in expected.iter().enumerate() {
        store.raw_into(&mut buf, i);
        assert_eq!(&buf, want, "原文の再構築がずれている（index {i}）");
    }
    println!(
        "{} 件で原文の再構築が、走査した target_path とバイト一致しました。",
        expected.len()
    );
}

/// PATH の篩のキーが `normalize_file_name_key_into` と一致することを、実データの全件で確かめる。
///
/// **機構としての保証は合成 fixture のほう**（`index_tree.rs` の
/// `file_key_matches_normalize_file_name_key_on_both_arms`）が持つ。ここが足すのは規模と、
/// 合成では作れない実際のパスの形（深い階層・非 ASCII・空白・大文字の拡張子）である。
///
/// **入力が `index.bin` 由来でも循環しない。** 篩は木の `name` + 拡張子から、比較相手は
/// フルパスの末尾成分から導くので、突き合わせているのは 2 つの正規化関数が末尾で一致するか
/// であって、組み直しの正しさではない。
#[test]
fn index_tree_file_key_matches_normalize_file_name_key_over_real_index() {
    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let expected: Vec<String> = entries
        .iter()
        .map(|e| {
            let mut b = String::new();
            crate::indexer::normalize_file_name_key_into(&mut b, &e.target_path);
            b
        })
        .collect();
    let tree = crate::index_tree::IndexTree::build(entries);

    let (mut roots, mut children) = (0usize, 0usize);
    let (mut buf, mut seg) = (String::new(), String::new());
    let parent = tree.columns().parent.to_vec();
    for (i, want) in expected.iter().enumerate() {
        if parent[i] == crate::index_tree::NO_PARENT {
            roots += 1;
        } else {
            children += 1;
        }
        tree.file_key_into(&mut buf, &mut seg, i);
        assert_eq!(&buf, want, "篩のキーが正規化とずれている（index {i}）");
    }
    // **両腕が通ったことを数える。** 根は実データで 0.1% 未満しか居ないので、数えないと
    // 「根の腕は 1 度も走らなかった」を一致と読み違える。
    assert!(roots > 0 && children > 0, "片方の腕しか通っていない");
    println!(
        "{} 件で篩のキーが一致しました（根 {roots} 件 / 非根 {children} 件）。",
        expected.len()
    );
}

/// 正規化キーの組み立てが `normalize_entry_key` と 1 バイトも違わないことを、実インデックスの
/// 全件で確かめる。**組み立てるのは製品が実際に通る [`PathCursor`] のほうである**——全件走査は
/// カーソル経由であり、素直な組み立て（`PathStore::normalized_into`）はその参照実装にすぎない。
///
/// ここが 1 バイトずれると履歴照合が沈黙で外れる（クラッシュせず検索結果も返り、ブーストだけが
/// 消える）。走査順は製品と同じ昇順＝鎖が当たり続ける経路で、外れる経路は
/// [`path_store_cursor_matches_full_rebuild`] が逆順・乱順で受け持つ。
#[test]
fn path_store_cursor_matches_normalize_entry_key_over_real_index() {
    use crate::indexer::normalize_entry_key;
    use crate::search::path_store::{PathCursor, PathStore};

    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let expected: Vec<String> = entries
        .iter()
        .map(|e| normalize_entry_key(&e.target_path))
        .collect();
    let store = PathStore::build(entries);
    let mut cursor = PathCursor::new();
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            cursor.normalized(&store, i),
            want.as_str(),
            "カーソルの組み立てが現物とずれている（index {i}）"
        );
    }
    println!(
        "{} 件でカーソルの組み立てが normalize_entry_key と一致しました。",
        expected.len()
    );
}

/// **パスクエリでも、表示名への Fuzzy マッチは失われない。**
///
/// `score_one_entry` はパスクエリのとき name の Fuzzy スコアリングを飛ばすが、それが許される
/// のは「区切りを含む needle は名前に部分列として存在しえない」ときだけである（#1057）。
/// **表示名が区切りを含むエントリではその前提が崩れる**ので、飛ばす条件は
/// `SearchEngine::any_name_has_path_sep` が測った実測値で閉じている。
///
/// このテストは**その閉じ方が効いていることの唯一の検知器**である——実装当時、素朴に
/// `has_path_sep` だけで飛ばす変異を当てると既存テストと clippy は全て通り、これだけが落ちた
/// （実測）。パスにはわざと当たらないクエリを使うので、**name の Fuzzy が唯一のマッチ経路**である。
#[test]
fn fuzzy_name_match_survives_when_display_name_contains_path_separator() {
    // 表示名が `\` を含む。target_path は別ドライブにしてパスマッチを成立させない。
    let entries = vec![make_entry("c:\\tool", "D:\\other\\app.exe")];
    let mut engine = SearchEngine::new(entries);

    let results = engine.search("c:\\tool", 8, &empty_history(), SearchMode::Fuzzy);

    assert_eq!(
        results.len(),
        1,
        "表示名への Fuzzy マッチが失われた（パスは当たらないので唯一の経路である）"
    );
    assert_eq!(results[0].name, "c:\\tool");
}

/// **PATH 併合が積むような形のエントリで、旗が静かに立たない。**
///
/// 上のテストは旗が**落ちる**向き（結果が壊れる）を守る。こちらは**立つ**向きを守る——旗が
/// 立つと `score_one_entry` は全件で name の Fuzzy スコアリングを通し、**削減だけが全損する**。
/// 結果は正しいままなので、既存の挙動テストは 1 本も落ちない（#1057 のレビュー M-3）。
///
/// **併合の経路そのものを通す。** PATH 併合は `IndexTree::extend_with_roots` が表示名をファイル名、
/// `target_path` をフルパスにして根として積む形で、**`IndexTree::build` は通らない**（別実装）。
/// ゆえに `build` だけのテストでは併合側の変異を 1 つも捕まえられない。先例は
/// `search/tests/build.rs` の PATH マージ検証（同じ `build` → `extend_with_roots` →
/// `new_from_tree` の並び）。
///
/// **実 index のテスト（[`skipping_name_scoring_changes_nothing_over_real_index`]）では代用できない**
/// ——`index.bin` は PATH 併合の**前**に書かれるので、あちらの入力に PATH エントリは 1 件も
/// 含まれない。
#[test]
fn path_env_merge_does_not_raise_any_name_has_path_sep() {
    use crate::index_tree::IndexTree;

    let base = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
    // PATH 由来の形: 表示名はファイル名、`target_path` はフルパス。根として積まれる。
    let path_entries = vec![
        make_entry("node.exe", "C:\\Program Files\\nodejs\\node.exe"),
        make_entry("git.exe", "C:\\Program Files\\Git\\cmd\\git.exe"),
    ];
    let mut tree = IndexTree::build(base);
    tree.extend_with_roots(path_entries);
    let engine = SearchEngine::new_from_tree(tree, false);

    assert!(
        !engine.any_name_has_path_sep,
        "PATH 併合で旗が立った。name の Fuzzy スコアリングが全件で走り #1057 の削減が\
         全損する（**結果は正しいままなので他のテストは 1 本も落ちない**）"
    );
}

/// **実 index 全件で、name スコアリングを飛ばしても結果が集合・順序とも変わらない。**
///
/// 比較相手は「旗を強制的に立てて最適化を殺した同じエンジン」である（#1057）。合成 fixture では
/// なく実運用点の 31 万件で突き合わせるのが要点で、**件数の一致では足りない**——パスマッチは
/// 同点が出やすく、tie-break の経路が変われば順序だけが動く。ゆえに `(name, path)` の列を
/// 順序ごと比べる。
#[test]
#[ignore = "実インデックス依存。手元で release 実行する"]
fn skipping_name_scoring_changes_nothing_over_real_index() {
    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let n = entries.len();
    let mut optimized = SearchEngine::new_with_migemo(entries.clone(), false);
    let mut baseline = SearchEngine::new_with_migemo(entries, false);
    // 旗を立てる = 「区切りを含む表示名が在る」＝ 飛ばさない＝ 改修前と同じ経路。
    baseline.any_name_has_path_sep = true;
    assert!(
        !optimized.any_name_has_path_sep,
        "実運用点では区切りを含む表示名は 0 件のはずで、立っていたら前提が崩れている"
    );

    let history = empty_history();
    let queries = [
        "users",
        "c:\\",
        "c:\\users",
        "c:\\users\\",
        "\\program files\\",
        "\\zzz-no-such-path\\",
        "c:\\windows\\system32\\",
        "\\appdata\\local\\",
        "notepad.exe",
        "c:\\notepad.exe",
    ];
    for q in queries {
        let want: Vec<(String, String)> = baseline
            .search(q, 200, &history, SearchMode::Fuzzy)
            .into_iter()
            .map(|r| (r.name, r.path))
            .collect();
        let got: Vec<(String, String)> = optimized
            .search(q, 200, &history, SearchMode::Fuzzy)
            .into_iter()
            .map(|r| (r.name, r.path))
            .collect();
        assert_eq!(want, got, "クエリ {q:?} で結果または順序が変わった");
        println!("  {q:?}: {} 件が集合・順序とも一致", want.len());
    }
    println!(
        "{n} 件の実インデックスで、全 {} クエリが一致しました。",
        queries.len()
    );
}
