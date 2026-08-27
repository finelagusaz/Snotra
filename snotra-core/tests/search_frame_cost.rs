//! #634 G-SYNC: egui 直 `Engine` 同期 search のフレームコスト実測ハーネス。
//!
//! egui 経路（`src-tauri/src/egui_shell/launcher_controller/` の `run_search_with`）は trailing
//! debounce 1 回の `engine.search` を UI スレッド同期で呼ぶ。本ハーネスはその
//! フレーム占有の支配項 `Engine::search` を、合成インデックスの規模別に実測する。
//!
//! タイミング測定は CI 共有ランナーで不安定なため `#[ignore]`。手元で release 実行する:
//! `cargo test -p snotra-core --release --test search_frame_cost -- --ignored --nocapture`
//!
//! 合成インデックスの生成規則は #532 Phase 1 スパイクの検証 Engine（#660 で撤去）と
//! 同系: 実在風の固定 6 件 + 連番エントリ + 履歴ブースト 5 件。CJK クエリも規模に
//! 比例させるため、連番の 1/8 を日本語名にする（スパイクからの意図的拡張）。
//!
//! `src/search/tests/performance.rs` の `bench_search` とは測る層が異なる（重複でない）:
//! あちらは `SearchEngine::search`（下層 API・固定 limit 10・空履歴・Fuzzy 固定）の
//! スケーリング、こちらは `Engine::search`（facade・config 実効 limit・履歴ブースト込み）
//! ＝ egui view の実呼び出し形のフレームコスト。

use std::time::Instant;

use snotra_core::{config::Config, engine::Engine, history::HistoryStore, indexer::AppEntry};

fn build_engine(entry_count: usize) -> Engine {
    let mut entries = vec![
        AppEntry {
            name: "Visual Studio Code".to_owned(),
            target_path: "C:/Program Files/Microsoft VS Code/Code.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "Windows Terminal".to_owned(),
            target_path: "C:/Program Files/WindowsApps/wt.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "ドキュメント".to_owned(),
            target_path: "C:/Users/User/Documents".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "ダウンロード".to_owned(),
            target_path: "C:/Users/User/Downloads".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "Snotra Settings".to_owned(),
            target_path: "snotra-settings.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "日本語入力テスト".to_owned(),
            target_path: "C:/Snotra/Verification/JapaneseInput.exe".to_owned(),
            is_folder: false,
        },
    ];
    for index in entries.len()..entry_count {
        let (name, path) = if index % 8 == 0 {
            (
                format!("日本語ツール {index:05}"),
                format!("C:/Snotra/Verification/長いパス/サブフォルダ/ツール{index:05}.exe"),
            )
        } else {
            (
                format!("Benchmark App {index:05}"),
                format!("C:/Snotra/Verification/App{index:05}.exe"),
            )
        };
        entries.push(AppEntry {
            name,
            target_path: path,
            is_folder: false,
        });
    }

    let history_path = std::env::temp_dir().join(format!(
        "snotra-search-frame-cost-{}-unused",
        std::process::id()
    ));
    let history = HistoryStore::load_in(&history_path);
    let mut engine = Engine::new(entries, history, Config::normalized_default());
    for path in [
        "C:/Program Files/Microsoft VS Code/Code.exe",
        "C:/Program Files/WindowsApps/wt.exe",
        "C:/Users/User/Documents",
        "C:/Users/User/Downloads",
        "snotra-settings.exe",
    ] {
        engine.record_launch(path, "");
    }
    engine
}

/// クエリ別に warmup 2 回 + 計測 20 回を回し、min/p50/max（µs）を採取する。
fn measure(engine: &mut Engine, query: &str) -> (usize, u64, u64, u64) {
    for _ in 0..2 {
        let _ = engine.search(query);
    }
    let mut samples_us = Vec::with_capacity(20);
    let mut result_count = 0usize;
    for _ in 0..20 {
        let started = Instant::now();
        let results = engine.search(query);
        samples_us.push(started.elapsed().as_micros() as u64);
        result_count = results.len();
    }
    samples_us.sort_unstable();
    (
        result_count,
        samples_us[0],
        samples_us[samples_us.len() / 2],
        samples_us[samples_us.len() - 1],
    )
}

#[test]
#[ignore = "タイミング測定（#634 G-SYNC）。手元 release 実行専用: --ignored --nocapture"]
fn measure_search_frame_cost() {
    // クエリの狙い: 全件級マッチ（a/app＝スコアリング+ソートの最悪系）、
    // 部分マッチ（99）、CJK 大量マッチ（ツール）、CJK 少数（ドキュメント）、
    // 完全ミス（zzz）、長クエリ。
    let queries = [
        "a",
        "app",
        "99",
        "ツール",
        "ドキュメント",
        "zzz_no_match",
        "benchmark app 099",
    ];
    for &entry_count in &[1_000usize, 10_000, 50_000, 100_000] {
        let mut engine = build_engine(entry_count);
        eprintln!("=== entries={entry_count} ===");
        for query in queries {
            let (results, min_us, p50_us, max_us) = measure(&mut engine, query);
            eprintln!(
                "query={query:<18} results={results:>5}  min={min_us:>6}µs  p50={p50_us:>6}µs  max={max_us:>6}µs"
            );
        }
    }
}
