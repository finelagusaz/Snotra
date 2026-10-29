//! 名前の照合（Substring の `find`・`.` を含むクエリの file name 照合）の壁時計と、結果の同一性を
//! 実 `index.bin` と合成の索引で測る計器（#1281）。
//!
//! タイミング測定は環境依存ゆえ CI では回さない。手元で release 実行する（コマンドの SSOT は
//! `docs/build-commands.md`）。計測値は `PERFORMANCE.md`。
//!
//! # 何を測るか
//!
//! - **全件走査になる打鍵 1 回**。各標本の前に [`RESET_QUERY`] を打って incremental の再利用を外す
//!   ——外さないと 2 回目以降は前回の一致集合だけを走査し、`.` を初めて打った打鍵や単調でない
//!   編集の後に起きる全件走査を測れない
//! - **結果の同一性**。クエリごとに件数と、`(name, path)` の列を順に畳んだダイジェストを出す。
//!   版の A/B で突き合わせ、検索結果が 1 件も変わらないことを確かめる。**パスの原文は出さない**
//!   ——実 `index.bin` は利用者の実ファイルパスを持つ（`AGENTS.md` の足場の行）
//! - 実 `index.bin` は **PATH マージあり・なし**（PATH 由来のエントリは file name が name と
//!   一致しない）× **Substring・Fuzzy**。`normal_mode` は実 config の値ではなく両方を掃く
//!   ——実運用点が Substring の機体でも、既定の Fuzzy の利用者に効くかを同じ表で見るため
//! - 合成の索引は**フォルダとファイルが半々**で、フォルダには file name と name の共有フラグが
//!   立つ。`search/tests/performance.rs` の `make_bench_entries` はファイルだけで構成され、フラグが
//!   1 件も立たないので、`.` を含むクエリの file name 照合を測れない
//!
//! # 測らないもの
//!
//! - **kana の照合**。実運用点の config で migemo が無効なら一度も走らない（この計器は実 config の
//!   `migemo_enabled` をそのまま使う）
//! - フォルダ展開の中の絞り込み（`folder.rs`）。検索の照合とは別の経路である
//!
//! **実 `index.bin` を書き換えない**——`load_or_scan_with_stats` が cache-miss したら全走査が走り
//! `index.bin` を上書きするので、その時点で assert で止める。

use std::hash::{Hash, Hasher};
use std::time::Instant;

use rustc_hash::FxHasher;
use snotra_core::config::{Config, SearchModeConfig};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};

/// incremental の再利用を外すクエリ。どの計測クエリの接頭辞にもならない。
const RESET_QUERY: &str = "zzzz";
/// 暖機の回数。
const WARMUP: usize = 2;
/// 標本数（p50 を読む）。
const SAMPLES: usize = 30;

/// 実 `index.bin` で回すクエリ。`.` を含まない名前・`.` を含む名前・パスの 3 種。
const REAL_QUERIES: [&str; 12] = [
    "c",
    "code",
    "sett",
    "vis",
    "chrome",
    ".exe",
    "setup.exe",
    ".txt",
    "a.d",
    "c:\\",
    "c:\\users\\",
    "\\bin\\",
];

/// 合成の索引で回すクエリ。
const SYNTHETIC_QUERIES: [&str; 6] = ["vis", "code", "app.lnk", "code.", "s.c", "c:\\data\\"];

/// `query` を全件走査で [`SAMPLES`] 回測り、`(件数, min, p50, ダイジェスト)` を返す（µs）。
///
/// ダイジェストは最後の標本の結果から取る。照合は決定的なので、どの標本でも同じ値になる。
fn sample(engine: &mut Engine, query: &str) -> (usize, u64, u64, u64) {
    for _ in 0..WARMUP {
        let _ = engine.search(RESET_QUERY);
        let _ = engine.search(query);
    }
    let mut samples_us = Vec::with_capacity(SAMPLES);
    let mut results = Vec::new();
    for _ in 0..SAMPLES {
        let _ = engine.search(RESET_QUERY);
        let started = Instant::now();
        results = engine.search(query);
        samples_us.push(started.elapsed().as_micros() as u64);
    }
    samples_us.sort_unstable();
    let mut hasher = FxHasher::default();
    for r in &results {
        r.name.hash(&mut hasher);
        r.path.hash(&mut hasher);
    }
    (
        results.len(),
        samples_us[0],
        samples_us[samples_us.len() / 2],
        hasher.finish(),
    )
}

/// `engine` の `normal_mode` を Substring・Fuzzy の順に切り替えて `queries` を測り、1 行ずつ刷る。
fn report(engine: &mut Engine, base: &Config, label: &str, queries: &[&str]) {
    for mode in [SearchModeConfig::Substring, SearchModeConfig::Fuzzy] {
        let mut config = base.clone();
        config.search.normal_mode = mode;
        engine.update_config(config);
        println!("\n  --- {label} / normal_mode = {mode:?} ---");
        println!("  query           results     min      p50  digest");
        for query in queries {
            let (results, min, p50, digest) = sample(engine, query);
            println!("  {query:<14}{results:>8}{min:>8}{p50:>9}  {digest:016x}");
        }
    }
}

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_match_cost_over_real_index() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    println!(
        "\n=== 名前の照合の壁時計（実 index.bin・migemo = {}・include_path_env = {}）===",
        config.search.migemo_enabled, config.search.include_path_env
    );
    for merge_path_env in [false, true] {
        // **材料は構成ごとに読み直す**——`extend_with_path_entries` は `IndexMaterial` を消費する
        // 形で木を伸ばすので、使い回すと 2 周目が汚れる。
        let loaded =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        assert!(
            loaded.stats.cache_hit,
            "index.bin が cache-miss した（全走査が走り index.bin を書き換えた）——計測を中止する"
        );
        let mut material = loaded.material;
        // 製品の手順の写し（出典: `tests/path_query_cost.rs` の
        // `measure_path_query_frame_cost_at_operating_point`。製品側に共有関数が無い）。
        let merged = if merge_path_env {
            let entries = indexer::scan_path_env(material.tree(), config.search.show_hidden_system);
            let n = entries.len();
            material.extend_with_path_entries(entries);
            n
        } else {
            0
        };
        let n = material.tree().len();
        let mut engine = Engine::from_material(material, HistoryStore::load(), config.clone());
        report(
            &mut engine,
            &config,
            &format!("実 {n} 件（PATH マージ +{merged} 件）"),
            &REAL_QUERIES,
        );
    }
    println!(
        "\n  （µs。各標本の前に {RESET_QUERY:?} で再利用を外す。暖機 {WARMUP} 回・{SAMPLES} 標本）"
    );
}

/// フォルダとファイルが半々の索引。フォルダは name が末尾成分そのものなので、file name と name の
/// 共有フラグが立つ（ファイルは `appN.lnk` で name と一致しない）。
fn folders_and_files(n: usize) -> Vec<AppEntry> {
    let prefixes = [
        "Microsoft",
        "Adobe",
        "Google",
        "Visual",
        "Windows",
        "System",
        "App",
        "Tool",
    ];
    let suffixes = [
        "Studio", "Reader", "Code", "Settings", "Control", "Panel", "Viewer",
    ];
    (0..n)
        .map(|i| {
            let name = format!(
                "{} {} {i}",
                prefixes[i % prefixes.len()],
                suffixes[i % suffixes.len()]
            );
            if i % 2 == 0 {
                AppEntry {
                    target_path: format!("C:\\Data\\Group{}\\{name}", i % 97),
                    name,
                    is_folder: true,
                }
            } else {
                AppEntry {
                    target_path: format!("C:\\Program Files\\App{i}\\app{i}.lnk"),
                    name,
                    is_folder: false,
                }
            }
        })
        .collect()
}

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_match_cost_over_synthetic_folders() {
    let history_path =
        std::env::temp_dir().join(format!("snotra-match-cost-{}-unused", std::process::id()));
    println!("\n=== 名前の照合の壁時計（合成・フォルダとファイルが半々）===");
    for n in [100_000usize, 300_000] {
        let base = Config::normalized_default();
        let mut engine = Engine::new(
            folders_and_files(n),
            HistoryStore::load_in(&history_path),
            base.clone(),
        );
        report(
            &mut engine,
            &base,
            &format!("合成 {n} 件"),
            &SYNTHETIC_QUERIES,
        );
    }
    println!(
        "\n  （µs。各標本の前に {RESET_QUERY:?} で再利用を外す。暖機 {WARMUP} 回・{SAMPLES} 標本）"
    );
}
