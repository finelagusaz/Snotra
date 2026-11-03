//! パスクエリ（`has_path_sep`）の走査コスト実測ハーネス。
//!
//! パスクエリは Fuzzy ビットマスク pre-filter を**スキップする**（`snotra-core/src/search/query_plan.rs` の `//!`）。ゆえに全エントリの `normalized_key` に対する
//! 部分文字列検索が毎打鍵で走る、索引規模がそのまま乗る唯一の経路である。
//! `search/tests/performance.rs` の `bench_fuzzy_path_query_scaling` もパス区切りのクエリを持つが、
//! 合成索引の `SearchEngine` 層である。**実 `index.bin` × 製品の層で測るのはここだけ**である。
//!
//! **`measure_path_query_frame_cost_at_operating_point` は実起動の経路を再現する**（#1067）: PATH マージ
//! （`IndexMaterial::extend_with_path_entries`）を通し、実 config の `normal_mode` をそのまま使う。
//! `measure_path_query_frame_cost` の側は据え置く——過去の全表との比較可能性がそこに掛かっている。
//! 2 つの計測コードは同一構成で一致することを実測してある（`PERFORMANCE.md`「実運用点のパスクエリのフレームコスト」）。
//! **フラグ（`Engine::sorted_by_path`）を出力に添えるのが要点である**——`cmp_paths` の経路がそれで変わり、
//! `c:\` の p50 が数千 µs 動く。
//!
//! **実 `index.bin` に対して測るのが要点である。** パス長がコストを支配し、
//! 実運用点の平均 119.3 B は合成ラダー（66.4 B）の約 2 倍ある（`PERFORMANCE.md`「索引の常駐の内訳」）。実インデックスが無い環境では自動スキップする。
//!
//! タイミング測定は環境依存ゆえ CI では回さない。手元で release 実行する（コマンドの
//! SSOT は `docs/build-commands.md`）。
//!
//! **製品レベルの関数はどれも実 `index.bin` を書き換えうる**（`load_or_scan_with_stats` を
//! 通るため。**数を書かない**——1 本足すたびに古くなる）。旧版を読んだらその場で現行版へ昇格するので、
//! **旧版のロードを測りたければ先に退避すること**（契約は同関数の doc）。走らせてから気づいても、
//! その版はもう手元に無い。
//!
//! # 層を混ぜない
//!
//! 計測は**製品レベル**だけである。実 `index.bin` を実起動と同じ経路で読み、`Engine::search` /
//! `SearchEngine::recent_history` をそのまま叩く。走査だけを切り出した写し
//! （`normalized_key` を保持するか導出するかを比べた反復 2 の記録）は、3 列のどれも製品の形では
//! なくなったので撤去した（#1391）。数字の正本は次の節である:
//! `PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」
//!
//! 反復 3 の実装前シミュレーション（`TreeIndex` 一式）は、製品に `search/path_store.rs` が
//! 入って重複したため撤去した。再構築のバイト一致を実データ全件で確かめる役目は
//! `search/tests/path.rs` の `path_store_raw_matches_target_path_over_real_index` /
//! `path_store_cursor_matches_normalize_entry_key_over_real_index` が**製品の `PathStore` に
//! 対して**引き継いでいる。

use std::time::Instant;

use snotra_core::config::{Config, SearchModeConfig};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::SearchEngine;

/// `indexer::normalize_entry_key` と**同じ規則**を、1 文字ずつ書き出す形で再現する。
///
/// ASCII 高速路が入る**前**の形であり、製品にこの形はもう無い。**高速路が規則を変えていないことを
/// 確かめる基準として残している**（`derives_same_bytes_as_normalize_entry_key`）。
/// `normalize_entry_key` の規則を変えたら、ここも同時に直す。
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

/// 上の写し（ASCII 高速路の無い形）と、出荷する実装（高速路あり）が 1 バイトも違わないことを、
/// 実インデックスの全パスで固定する。**高速路の正しさを実データで見るのはこのテストだけである**
/// （`indexer/keys.rs` にユニットテストは無い）。計測ではないので `#[ignore]` を持たない。
#[test]
fn derives_same_bytes_as_normalize_entry_key() {
    let Some(entries) = load_real_index() else {
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

/// 実 `index.bin` に**載っているときだけ**読む。
///
/// 走査しない入口（`indexer::load_cached_entries`）を通すのは必須である。
/// [`derives_same_bytes_as_normalize_entry_key`] は `#[ignore]` を持たず既定のスイートで走るため、
/// `load_or_scan_with_stats` だと cold cache の `cargo test` が全走査と `index.bin` の書き込みを
/// 起こす（理由の全文は当該入口の doc）。
fn load_real_index() -> Option<Vec<AppEntry>> {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        return None;
    }
    indexer::load_cached_entries(scan, config.search.show_hidden_system)
}

/// パス区切りを含むクエリの**製品レベル**フレームコスト（実 `index.bin` × `Engine::search`）。
///
/// **既存の計測コードはどれもここを測れない。** `tests/search_frame_cost.rs` は合成索引であり
/// パス区切りクエリを 1 本も持たない（しかもパス長 66.4 B・浅い一様ツリーで、実運用の
/// 119.3 B・深さ平均 6.05 段とは別物）。
///
/// **`has_path_sep` は incremental cache を無条件で無効化する**（`IncrementalCache::can_reuse`）。
/// ゆえにパスを打っている間は毎打鍵が全件走査になり、ここが検索 worker で 1 打鍵ごとに払う額そのものになる
/// （検索は #1004 から UI スレッドではなく worker で走る）。
///
/// **撤去条件**: 過去の表との比較が要らなくなったとき（この計測コードの値を引く `PERFORMANCE.md` の節を
/// 履歴へ落とす変更と同じ PR で）消す。実運用点に追従する側は `measure_path_query_frame_cost_at_operating_point` である。
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
    // **ツリーを `Vec<AppEntry>` へ戻さない。** 実体化とツリーの再構築で同じツリーを 2 度建てることになる（`IndexTree::materialize` の doc）。
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
        // 2 回のウォームアップのあと 20 回。min/p50/max を出す——**平均は出さない**。1 フレームを
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
///   マージするので、`IndexTree::extend_with_roots` がフラグを下ろし、tie-break が
///   `PathStore::cmp_paths` の**両辺フルパス組み立て**へ落ちる
/// - **`normal_mode` が実 config の値とは限らない。** #1057 / #1059 の表は
///   `SNOTRA_CONFIG_DIR` を temp へ向けて `fuzzy` に差し替えて測っている。`substring` では
///   `score_one_entry` の `skip_name` が発火しない（あれは Fuzzy を要求する）
///
/// **フラグの実効値は出力に添える。** 添えなければ「どちらの経路を測ったか」が分からず、
/// 数字の帰属ができない（`Engine::sorted_by_path` の doc）。**`include_path_env` が真か**を
/// 代理指標にしてはならない——`extend_with_roots` は空 vec で早期 return するので、
/// PATH が空・全件重複の構成ではフラグが立ったまま残る（検知器は `search/tests/build.rs` の
/// `path_merge_lowers_the_sort_flag_only_when_it_actually_adds_entries`）。
///
/// # この計測コードが単独で検出する経路
///
/// 2 つあり、**既存のどの計測コードも見ていない**: (1) PATH マージ後の `sorted_by_path = false` での
/// tie-break、(2) `normal_mode = "substring"` での name スコアリング。
///
/// # この計測コードが見ないもの
///
/// 検索結果の正しさ（挙動テストの担当）と、実 UI スレッドが本当にこの額を払うか
/// （`smoke:egui` の担当）。**合否も言わない**——出力に添えた諸元を人間が読んで初めて
/// 「実運用点を再現できたか」が確かめられる。判定を持たないツールも層に数えてよい、という原則に依っている
/// （`docs/development-principles.md`「検証の層と、層と層の隙間」）。
///
/// # 実データを直接読む
///
/// **実 `%APPDATA%\Snotra` を直読する**（`SNOTRA_CONFIG_DIR` も temp コピーも使わない）。
/// `HistoryStore::load()` を使うのは #963 の規律（ユニットテストの fixture には使わない）の
/// 内側である——計測ハーネスは実運用の姿を測るのが目的だからで、兄弟の
/// [`measure_recent_history_cost`] と同じ扱いになる。
///
/// **`load_or_scan_with_stats` は `Engine` を 2 つ建てるので合わせて 2 回呼ぶ**（構成ごとに
/// 1 回。`extend_with_path_entries` が材料を消費するため使い回せない）。旧版の `index.bin` が
/// 置かれた機体では現行版への昇格が走りうる（`//!` の警告）——**その場合は 2 回とも走る**。
///
/// # 足場ではない
///
/// **撤去条件を持たない恒久の計測コードである。** 実運用点との乖離は #1057 が発見し、#1059 は
/// 過去の表との比較可能性を優先して既存ハーネスを据え置いた。#1067 は 3 サイクル目として
/// 「送る」のをやめ、既存を据え置いたまま別計測コードを足す側を選んだ。
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
        // 消費する形でツリーを伸ばすので、使い回すと 2 周目が汚れる。
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
                "\n  --- PATH マージ {}{} （+{merged} 件・ツリー {n_before} → {n_after}） / \
                 normal_mode = {mode:?} / sorted_by_path = {} ---",
                // **ループ変数ではなく実際に足した件数で刷る。** `include_path_env = false` の
                // 機体では 2 条件が同一の測定になり、ループ変数だと片方が嘘のラベルを示す
                // ——この doc が上で「代理指標にしてはならない」と書いた誤りの、出力側の鏡像である。
                if merged > 0 { "あり" } else { "なし" },
                // **2 条件が同一構成になる場合はそう書く。** `include_path_env = false` の機体
                // では「マージ なし」の表が 2 度出るので、読み手が理由を導出せずに済むようにする。
                if merge_path_env && merged == 0 {
                    "（`include_path_env` が偽ゆえ下の条件と同一構成）"
                } else {
                    ""
                },
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
    println!(
        "\n  （µs。60fps の 1 フレームは 16,700 µs。ウォームアップ {WARMUP} 回のあと {SAMPLES} 標本）"
    );
}

/// **どの設定の組み合わせが `c:\` のコストを動かすか**を 1 枚の表で見る（#1067）。
///
/// [`measure_path_query_frame_cost_at_operating_point`] が「開発機の実 config が実際にどこに
/// 居るか」を測るのに対し、こちらは**設定空間のどこに崖があるか**を測る。改修の適用範囲
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
/// 全走査（22〜30 秒）が走り**実 `index.bin` を上書きする**。計測コードが測定対象の環境を壊す。
///
/// **`migemo_min_chars` / `fuzzy_history_cap_ratio` / `history_normalization` も掃かない。**
/// 前 2 つは上のとおり `kana_query` が常に `None` ゆえ効かず、最後は `adjusted_history_boost` の
/// cap が Fuzzy のときだけ効くもので、上限は履歴照合全体（実測 ≤1 ms）より小さい。
///
/// # 読むクエリを 2 本に絞る
///
/// `c:\`（支配項が出る唯一の行）と `\zzz-no-such-path\`（0 件・**どの軸でも動かないことが正しい**
/// 対照）。36 構成 × 6 クエリでは表が読めない。
///
/// **撤去条件**: 掃いている軸（`normal_mode` / `result_limit` / `include_path_env`）のどれかを製品から
/// 消す変更と同じ PR で、消すか軸を直す。#1067 の判断には使い終えたが、改修の効く範囲を決め直すときに使う。
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
                        // `include_path_env = false` なら 2 条件は同一の測定である）。
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
    println!(
        "\n  （µs。60fps の 1 フレームは 16,700 µs。ウォームアップ {WARMUP} 回のあと {SAMPLES} 標本）"
    );
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

/// ウォームアップの回数。
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

/// `engine.search(query)` をウォームアップのあと [`SAMPLES`] 回叩き、返した件数と分位点を返す。
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

/// `recent_history` の実コスト。**ウィンドウを開いた瞬間・クエリ消去時には走らない**（頻度と呼び出し元の正本は `SearchEngine::recent_history` の doc）。**この計測が測るのは「呼ばれたときの 1 回」である。**
///
/// `recent_launches` が返すのは高々 `recent_limit` 件（既定 8）だが、照合のために**全エントリ**の
/// 正規化キーを導出して引く（照合表は履歴の側の高々 `recent_limit` 件で組む）。`recent_history` を
/// 実 `index.bin` で測るのはここだけである。
///
/// **撤去条件**: `SearchEngine::recent_history` を製品から消す変更と同じ PR で消す。
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
    println!(
        "  = 呼ばれたときの 1 回の額（{n} 件の正規化キーを導出して照合する）"
    );
}
