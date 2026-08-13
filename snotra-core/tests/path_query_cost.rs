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
//! **製品レベルの関数はどれも実 `index.bin` を書き換えうる**（`load_or_scan_with_stats` を
//! 通るため。**数を書かない**——1 本足すたびに腐る）。旧版を読んだらその場で現行版へ昇格するので、
//! **旧版のロードを測りたければ先に退避すること**（契約は同関数の doc）。走らせてから気づいても、
//! その版はもう手元に無い。
//!
//! # 層を混ぜない
//!
//! - **製品レベル**: [`measure_path_query_frame_cost`] /
//!   [`measure_path_query_frame_cost_at_operating_point`] / [`measure_recent_history_cost`] は
//!   実 `index.bin` を実起動と同じ経路で読み、`Engine::search` / `SearchEngine::recent_history`
//!   をそのまま叩く。**判定に使うのはこちらである。**
//!   （**数を見出しに書かない**——製品レベルの関数を 1 本足すたびに腐る）
//! - **走査だけを切り出した写し**: [`measure_path_query_sweep_cost`] は `normalized_key` を
//!   保持するか導出するかを比べた反復 2 の記録であり、スコアリング・履歴照合・top-k 組立が
//!   乗っていない。**見積もりが甘くなる**（実測で写し +3 ms に対し製品は +8〜16 ms）。
//!   撤去条件は当該関数の doc に書いてある。
//!
//! 反復 3 の実装前シミュレーション（`TreeIndex` 一式）は、製品に `search/path_store.rs` が
//! 入って重複したため撤去した。再構築のバイト一致を実データ全件で確かめる役目は
//! `search/tests/path.rs` の `path_store_raw_matches_target_path_over_real_index` /
//! `path_store_cursor_matches_normalize_entry_key_over_real_index` が**製品の `PathStore` に
//! 対して**引き継いでいる。

use std::cell::RefCell;
use std::time::Instant;

use rayon::prelude::*;

use snotra_core::config::{Config, SearchModeConfig};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::SearchEngine;

thread_local! {
    /// 導出側の再利用バッファ。**容量を再利用するため暖まった後の確保はゼロ**——
    /// ここを毎回 `String::new()` にすると、測っているものが正規化ではなく確保になる。
    static KEY_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `indexer::normalize_entry_key` と**同じ規則**を、1 文字ずつ書き出す形で再現する。
///
/// ASCII 高速路が入る**前**の形であり、製品にこの形はもう無い（比較の基準として残している）。
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

/// 実 `index.bin` に**載っているときだけ**読み、比較用に「保持していたら」の側も作る。
///
/// **`normalized_keys` は索引にもオンディスクにも既に無い**（v5 で落とした）。保持側は
/// v4 が持っていたものと同じ内容を、ここで 1 度だけ作って再現する——比較の意味は
/// 「事前計算を持つ」対「毎回導出する」であって、どこから来た文字列かではない。
///
/// 走査しない入口（`indexer::load_cached_entries`）を通すのは必須である。
/// [`derives_same_bytes_as_normalize_entry_key`] は `#[ignore]` を持たず既定のスイートで走るため、
/// `load_or_scan_with_stats` だと cold cache の `cargo test` が全走査と `index.bin` の書き込みを
/// 起こす（理由の全文は当該入口の doc）。
fn load_real_index() -> Option<(Vec<AppEntry>, Vec<String>)> {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        return None;
    }
    let entries = indexer::load_cached_entries(scan, config.search.show_hidden_system)?;
    let keys: Vec<String> = entries
        .iter()
        .map(|e| indexer::normalize_entry_key(&e.target_path))
        .collect();
    Some((entries, keys))
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

/// パス区切りを含むクエリの**製品レベル**フレームコスト（実 `index.bin` × `Engine::search`）。
///
/// **既存の計器はどれもここを測れない。** `tests/search_frame_cost.rs` は合成索引であり
/// パス区切りクエリを 1 本も持たない（しかもパス長 66.4 B・浅い一様木で、実運用の
/// 119.3 B・深さ平均 6.05 段とは別物）。`measure_path_query_sweep_cost` は走査だけを
/// 切り出した写しであって、`Engine::search`（config 実効 limit・履歴ブースト・top-k 組立込み）
/// ではない。
///
/// **`has_path_sep` は incremental cache を無条件で無効化する**（`IncrementalCache::can_reuse`）。
/// ゆえにパスを打っている間は毎打鍵が全件走査になり、ここが UI スレッドで払う額そのものになる。
/// クエリは「パスを 1 文字ずつ打っていく」形に並べる——区切りを打った瞬間から全件走査へ
/// 切り替わるので、その前後を同じ表で見られるようにする。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_frame_cost() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    let result =
        indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
    let n = result.material.tree().len();
    let history = HistoryStore::load();
    // **木を `Vec<AppEntry>` へ戻さない。** 実体化と木の再構築で同じ木を 2 度建てることになる（`IndexTree::materialize` の doc）。
    let mut engine = Engine::from_material(result.material, history, config);

    println!("\n=== パスクエリのフレームコスト（実 index.bin・{n} 件・Engine::search）===");
    println!("  query                  results     min      p50      max");
    for query in [
        "users",       // 区切り無し = bitmask pre-filter が効く（比較の基準）
        "c:\\",        // 区切りを打った瞬間から全件走査
        "c:\\users",   //
        "c:\\users\\", //
        "\\program files\\",
        "\\zzz-no-such-path\\",
    ] {
        // 2 回の暖機のあと 20 回。min/p50/max を出す——**平均は出さない**。1 フレームを
        // 落とすかどうかを決めるのは中央値と最悪値であって平均ではない。
        for _ in 0..2 {
            let _ = engine.search(query);
        }
        let mut samples_us = Vec::with_capacity(20);
        let mut results = 0usize;
        for _ in 0..20 {
            let started = Instant::now();
            let out = engine.search(query);
            samples_us.push(started.elapsed().as_micros() as u64);
            results = out.len();
        }
        samples_us.sort_unstable();
        println!(
            "  {query:<22}{results:>7}{:>8}{:>9}{:>9}",
            samples_us[0],
            samples_us[samples_us.len() / 2],
            samples_us[samples_us.len() - 1],
        );
    }
    println!("  （µs。60fps の 1 フレームは 16,700 µs）");
}

/// パスクエリのフレームコストを**実運用点で**測る（#1067）。
///
/// # なぜ [`measure_path_query_frame_cost`] では足りないのか
///
/// あちらは実 config を読むが、**実起動が通る経路を 2 つ落としている**。どちらも
/// パスクエリのコストに効き、しかも**逆向きに効く**ので、差の符号すら予測できない。
///
/// - **PATH エントリのマージを通らない。** `load_or_scan_with_stats` を呼ぶだけで
///   `IndexMaterial::extend_with_path_entries` を通さないため、索引は
///   `sorted_by_path = true`（速い経路）のまま測られる。実起動は `include_path_env` が真なら
///   マージするので、`IndexTree::extend_with_roots` が旗を下ろし、tie-break が
///   `PathStore::cmp_paths` の**両辺フルパス組み立て**へ落ちる
/// - **`normal_mode` が実 config の値とは限らない。** #1057 / #1059 の表は
///   `SNOTRA_CONFIG_DIR` を temp へ向けて `fuzzy` に差し替えて測っている。`substring` では
///   `score_one_entry` の `skip_name` が発火しない（あれは Fuzzy を要求する）
///
/// **旗の実効値は出力に添える。** 添えなければ「どちらの経路を測ったか」が分からず、
/// 数字の帰属ができない（`Engine::sorted_by_path` の doc）。**`include_path_env` が真か**を
/// 代理指標にしてはならない——`extend_with_roots` は空 vec で早期 return するので、
/// PATH が空・全件重複の構成では真のまま旗が立つ（検知器は `search/tests/build.rs` の
/// `path_merge_lowers_the_sort_flag_only_when_it_actually_adds_entries`）。
///
/// # この計器が単独で捕まえる経路
///
/// 2 つあり、**既存のどの計器も見ていない**: (1) PATH マージ後の `sorted_by_path = false` での
/// tie-break、(2) `normal_mode = "substring"` での name スコアリング。
///
/// # この計器が見ないもの
///
/// 検索結果の正しさ（挙動テストの担当）と、実 UI スレッドが本当にこの額を払うか
/// （`smoke:egui` の担当）。**合否も言わない**——出力に添えた諸元を人間が読んで初めて
/// 「実運用点を再現できたか」が確かめられる（`docs/development-principles.md`
/// 「判定を持たない道具を層に数えてよい」）。
///
/// # 実データを直接読む
///
/// **実 `%APPDATA%\Snotra` を直読する**（`SNOTRA_CONFIG_DIR` も temp コピーも使わない）。
/// `HistoryStore::load()` を使うのは #963 の規律（ユニットテストの fixture には使わない）の
/// 内側である——計測ハーネスは実運用の姿を測るのが目的だからで、兄弟の
/// [`measure_recent_history_cost`] と同じ扱いになる。
///
/// **`load_or_scan_with_stats` を構成ごとに 2 回呼ぶ**（`extend_with_path_entries` が材料を
/// 消費するため使い回せない）。旧版の `index.bin` が置かれた機体では現行版への昇格が
/// 走りうる（`//!` の警告）——**その場合は 2 回とも走る**。
///
/// # 足場ではない
///
/// **撤去条件を持たない恒久の計器である。** 実運用点との乖離は #1057 が発見し、#1059 は
/// 過去の表との比較可能性を優先して既存ハーネスを据え置いた。#1067 は 3 サイクル目として
/// 「送る」のをやめ、既存を据え置いたまま別計器を足す側へ倒した。
/// **`RETROSPECTIVE.md` を参照先にしない**——あれはサイクルごとに上書きされるので、
/// 恒久の doc から見出しを引くと次のサイクルで着地しなくなる。
///
/// # 2 軸は対称ではない
///
/// `normal_mode` は `Engine::search` が毎回 config から読む live-read ゆえ同じ `Engine` のまま
/// `update_config` で振れる（`IndexInputs` に含まれないので索引は stale にならない）。
/// `include_path_env` は材料の作り方が変わるので **`Engine` を建て直す**。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_frame_cost_at_operating_point() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    // 履歴の規模は**必ず添える**。過去の表は `SNOTRA_CONFIG_DIR` を temp へ向けており
    // `history.bin` もそちらを向くため、履歴が空で測られた疑いがある——空のマップは
    // hashbrown が `get` の冒頭でハッシュ計算前に短絡するので、履歴照合のコストが丸ごと
    // 消える。**併記しなければこの要因は次の反復でも未統制のまま残る。**
    // 件数は既存の pub API から取る（`recent_launches` は高々 `max` 件を返す契約ゆえ、
    // `usize::MAX` を渡せば全件になる）。**ただし数えられるのは `global` の 1 マップだけである**
    // ——照合は global / query / folder_expansion の 3 種を引くので、諸元は 3 分の 1 しか
    // 添えていない。**「履歴照合が支配項か」を疑う反復では足りない**（残り 2 マップの件数を
    // 返す口が今は無い）。
    let history_global_entries = HistoryStore::load().recent_launches(usize::MAX).len();

    println!("\n=== パスクエリのフレームコスト（実運用点・2×2）===");
    println!(
        "  実 config: normal_mode = {:?} / include_path_env = {} / result_limit = {} / \
         migemo = {} / history_normalization = {:?} / \
         history global {history_global_entries} 件（query / folder_expansion は未計測）",
        config.search.normal_mode,
        config.search.include_path_env,
        config.search.effective_result_limit(),
        config.search.migemo_enabled,
        config.search.history_normalization,
    );

    for merge_path_env in [true, false] {
        // **材料は構成ごとに読み直す**——`extend_with_path_entries` は `IndexMaterial` を
        // 消費する形で木を伸ばすので、使い回すと 2 周目が汚れる。
        let result =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        let mut material = result.material;
        let n_before = material.tree().len();

        // **製品の手順をそのまま写す**（出典: `src-tauri/src/main.rs` の索引ロード直後）。
        // 製品側に共有関数が無い（`indexing.rs::build_index_from_material` は `pub(crate)` かつ
        // `PrebuiltIndex` を返す）ため写しになる。製品自身も同じ 3 手を 2 か所へ書いている。
        let merged = if merge_path_env && config.search.include_path_env {
            let path_entries =
                indexer::scan_path_env(material.tree(), config.search.show_hidden_system);
            let n = path_entries.len();
            material.extend_with_path_entries(path_entries);
            n
        } else {
            0
        };
        let n_after = material.tree().len();

        let mut engine = Engine::from_material(material, HistoryStore::load(), config.clone());

        for mode in [SearchModeConfig::Substring, SearchModeConfig::Fuzzy] {
            let mut cfg = config.clone();
            cfg.search.normal_mode = mode;
            engine.update_config(cfg);

            println!(
                "\n  --- PATH マージ {} （+{merged} 件・木 {n_before} → {n_after}） / \
                 normal_mode = {mode:?} / sorted_by_path = {} ---",
                // **ループ変数ではなく実際に足した件数で刷る。** `include_path_env = false` の
                // 機体では 2 腕が同一の測定になり、ループ変数だと片方が嘘のラベルを名乗る
                // ——この doc が上で「代理指標にしてはならない」と書いた誤りの、出力側の鏡像である。
                if merged > 0 { "あり" } else { "なし" },
                engine.sorted_by_path(),
            );
            println!(
                "  query                  results     min      p50      p90      p99      max"
            );
            for query in PATH_QUERY_ROWS {
                let (results, s) = sample_frame_cost(&mut engine, query);
                println!(
                    "  {query:<22}{results:>7}{:>8}{:>9}{:>9}{:>9}{:>9}",
                    s.min, s.p50, s.p90, s.p99, s.max,
                );
            }
        }
    }
    println!("\n  （µs。60fps の 1 フレームは 16,700 µs。暖機 {WARMUP} 回のあと {SAMPLES} 標本）");
}

/// **どの設定の組み合わせが `c:\` のコストを動かすか**を 1 枚の表で見る（#1067）。
///
/// [`measure_path_query_frame_cost_at_operating_point`] が「開発機の実 config が実際にどこに
/// 居るか」を測るのに対し、こちらは**設定空間のどこに崖があるか**を測る。改修の射程
/// （誰に効くのか）を決めるのに要る——支配項の `cmp_paths` は `include_path_env` が真の
/// ときにしか発火しないので、**既定構成のユーザーには 1 µs も効かない**（`PERFORMANCE.md`）。
///
/// # 掃く軸と、掃かない軸の理由
///
/// - **`include_path_env`** {on, off} — `sorted_by_path` を決める唯一の設定。索引の材料が
///   変わるので `Engine` を建て直す
/// - **`migemo_enabled`** {on, off} — kana 2 列の有無で索引の構築が変わる（`IndexInputs`）。
///   **per-entry のコストは原理的にゼロのはずである**——`kana_query` は `to_kana` 後に ASCII
///   英字が残ると `None` になり（`search/query_plan.rs` の `prepare_query_plan`）、パスクエリは
///   必ず英字を含むので常に `None` になる。ゆえにここで測るのは**常駐が増えたことの間接効果**だけ
/// - **`normal_mode`** {prefix, substring, fuzzy} — `Engine::search` の live-read
/// - **`result_limit`** {8, 200, 1000} — 同じく live-read。top-k の大きさはヒープの深さと
///   `cmp_paths` の呼び出し回数に直結する
///
/// **`show_hidden_system` は掃かない。** `compute_config_hash` の入力なので、振ると cache-miss で
/// 全走査（22〜30 秒）が走り**実 `index.bin` を上書きする**。計器が測定対象の環境を壊す。
///
/// **`migemo_min_chars` / `fuzzy_history_cap_ratio` / `history_normalization` も掃かない。**
/// 前 2 つは上のとおり `kana_query` が常に `None` ゆえ効かず、最後は `adjusted_history_boost` の
/// cap が Fuzzy のときだけ効くもので、上限は履歴照合全体（実測 ≤1 ms）より小さい。
///
/// # 読むクエリを 2 本に絞る
///
/// `c:\`（支配項が出る唯一の行）と `\zzz-no-such-path\`（0 件・**どの軸でも動かないことが正しい**
/// 対照）。36 構成 × 6 クエリでは表が読めない。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_cost_across_settings() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }

    println!("\n=== 設定の組み合わせごとの `c:\\` のフレームコスト（#1067）===");
    println!(
        "  実 config: normal_mode = {:?} / include_path_env = {} / migemo = {} / result_limit = {}",
        config.search.normal_mode,
        config.search.include_path_env,
        config.search.migemo_enabled,
        config.search.effective_result_limit(),
    );
    println!("  PATH  migemo  mode       limit |  c:\\ p50   c:\\ max | zzz p50  zzz max | sorted");

    for merge_path_env in [true, false] {
        for migemo_enabled in [false, true] {
            let result = indexer::load_or_scan_with_stats(
                &config.paths.scan,
                config.search.show_hidden_system,
            );
            let mut material = result.material;
            let merged = if merge_path_env && config.search.include_path_env {
                let path_entries =
                    indexer::scan_path_env(material.tree(), config.search.show_hidden_system);
                let n = path_entries.len();
                material.extend_with_path_entries(path_entries);
                n
            } else {
                0
            };

            // **`migemo_enabled` は索引の構築入力である**（`IndexInputs`）。`update_config` では
            // 反映されないので、ここで材料から建て直す。
            let mut build_config = config.clone();
            build_config.search.migemo_enabled = migemo_enabled;
            let mut engine =
                Engine::from_material(material, HistoryStore::load(), build_config.clone());

            for mode in [
                SearchModeConfig::Prefix,
                SearchModeConfig::Substring,
                SearchModeConfig::Fuzzy,
            ] {
                for limit in [8usize, 200, 1000] {
                    let mut cfg = build_config.clone();
                    cfg.search.normal_mode = mode;
                    cfg.search.result_limit = Some(limit);
                    engine.update_config(cfg);

                    let (_, hit) = sample_frame_cost(&mut engine, "c:\\");
                    let (_, miss) = sample_frame_cost(&mut engine, "\\zzz-no-such-path\\");
                    println!(
                        "  {:<5} {:<7} {:<10} {limit:>5} | {:>8} {:>9} | {:>7} {:>8} | {}",
                        // **ループ変数ではなく実際に足した件数で刷る**（実 config が
                        // `include_path_env = false` なら 2 腕は同一の測定である）。
                        if merged > 0 { "あり" } else { "なし" },
                        if migemo_enabled { "on" } else { "off" },
                        format!("{mode:?}"),
                        hit.p50,
                        hit.max,
                        miss.p50,
                        miss.max,
                        engine.sorted_by_path(),
                    );
                }
            }
        }
    }
    println!("\n  （µs。60fps の 1 フレームは 16,700 µs。暖機 {WARMUP} 回のあと {SAMPLES} 標本）");
}

/// 計測するクエリ。**既存ハーネスと同じ 6 本を同じ順で並べる**（比較可能性）。
const PATH_QUERY_ROWS: [&str; 6] = [
    "users",
    "c:\\",
    "c:\\users",
    "c:\\users\\",
    "\\program files\\",
    "\\zzz-no-such-path\\",
];

/// 暖機の回数。
const WARMUP: usize = 2;
/// 標本数。**既存ハーネスの 20 では max の帰属に足りない**——#1067 の実測で max が
/// 18,642〜22,420 µs と 3 回の実行間で 20% 揺れた。max は受け入れ条件に入っている
/// （2026-08-13 のユーザー決定）ので、p90 / p99 と併せて読めるだけの標本を採る。
const SAMPLES: usize = 100;

/// 1 クエリぶんの分位点（µs）。**平均は出さない**——1 フレームを落とすかを決めるのは
/// 中央値と裾であって平均ではない。
struct FrameSamples {
    min: u64,
    p50: u64,
    p90: u64,
    p99: u64,
    max: u64,
}

/// `engine.search(query)` を暖機のあと [`SAMPLES`] 回叩き、返した件数と分位点を返す。
fn sample_frame_cost(engine: &mut Engine, query: &str) -> (usize, FrameSamples) {
    for _ in 0..WARMUP {
        let _ = engine.search(query);
    }
    let mut samples_us = Vec::with_capacity(SAMPLES);
    let mut results = 0usize;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let out = engine.search(query);
        samples_us.push(started.elapsed().as_micros() as u64);
        results = out.len();
    }
    samples_us.sort_unstable();
    // 分位点は「下から q 割の位置」を最近傍で取る（標本数を変えても意味が動かない形）。
    let at = |q: f64| samples_us[((samples_us.len() - 1) as f64 * q).round() as usize];
    (
        results,
        FrameSamples {
            min: samples_us[0],
            p50: at(0.50),
            p90: at(0.90),
            p99: at(0.99),
            max: samples_us[samples_us.len() - 1],
        },
    )
}

/// `recent_history` の実コスト。**窓を開いた瞬間・クエリ消去時には走らない**（頻度と呼び出し元の正本は `SearchEngine::recent_history` の doc）。**この計測が測るのは「呼ばれたときの 1 回」である。**
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
    let n = result.material.tree().len();
    let engine = SearchEngine::from_material(result.material, config.search.migemo_enabled);
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

/// `normalized_key` を保持するか導出するかを比べた**反復 2 の記録**（走査だけを切り出した写し）。
///
/// **判定にそのまま使わないこと。** ここにはスコアリングも履歴照合も top-k 組立も乗っておらず、
/// 同じ再構築の上にそれらが乗る製品では増分が数倍になる（実測: 写し +3 ms に対し製品 +8〜16 ms）。
/// 製品レベルの相当物は [`measure_path_query_frame_cost`] である。
///
/// **撤去条件**: 3 列（保持 / 導出:素 / 導出:ASCII）はどれも現在の製品の形ではない
/// ——`normalized_keys` は v5 で無くなり、走査は `search/path_store.rs` の `PathCursor` からの
/// 組み立てに移った。`PERFORMANCE.md`「パスクエリ全走査のコスト」が反復 2 の判定記録として
/// 参照されるあいだだけ残す。その節を履歴へ落とすとき、本関数と `sweep_prebuilt` /
/// `sweep_derived` / `normalize_into` / `load_real_index` の `keys` 側を一緒に消す。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_sweep_cost() {
    let Some((entries, keys)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    let n = entries.len();
    println!("\n=== パスクエリ全走査のコスト（実 index.bin・{n} 件）===");
    println!("  needle              保持 (ms)  導出:素 (ms)  導出:ASCII (ms)   hits");

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
        }
        println!("  {needle:<20}{best_pre:>9.1}{best_plain:>13.1}{best_fast:>16.1}{hits:>7}");
    }
}
