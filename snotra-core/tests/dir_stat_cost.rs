//! ディレクトリ 1 件あたりの「列挙」と「属性の読み取り」の費用差を実測するハーネス。
//!
//! # 何を決めるための計器か
//!
//! 背景再スキャンは全ディレクトリを `read_dir` して子を列挙している（実測 約 22 秒・#1001）。
//! 索引が持つのは `name` / `target_path` / `is_folder` だけで中身を持たないので、索引にとっての
//! 「変更」は**直下の追加・削除・改名**だけである。そしてディレクトリの mtime はちょうどそれで
//! 動き、中身だけの変更でも孫の変更でも動かない（2026-08-09 実測）。
//!
//! ゆえに「保存した木を辿り、mtime が保存時刻より新しいディレクトリだけ `read_dir` する」形が
//! 考えられる。**ただし mtime は上へ伝播しないので、全ディレクトリを訪れること自体は消せない。**
//! 消せるのは訪れた先での列挙だけであり、**削減の天井は丸ごと「属性の読み取りが列挙より
//! どれだけ安いか」に乗る**。この計器はその比だけを測る。
//!
//! # 判定基準は測る前に置く（後から解釈しないため）
//!
//! - 比 ≤ 0.3 … 設計を進めてよい
//! - 比 0.3〜0.6 … 微妙。全走査の間引きと並べて比べる
//! - 比 ≥ 0.6 … この案は却下し、間引きへ倒す
//!
//! # 表現を 1 つだけ測ってはならない
//!
//! `std::fs::metadata` は Windows ではハンドルを開く（`CreateFileW`）。`GetFileAttributesExW` は
//! 開かない。**片方だけ測ると、たまたま遅い実装を根拠に案ごと捨てることになる。** ここでは 4 つ測る。
//!
//! タイミングは環境依存ゆえ CI では回さない。手元で release 実行する
//! （コマンドの SSOT は `docs/build-commands.md`）。実 `index.bin` が無い環境では自動スキップする。
//!
//! # 出した答えと、このファイルの撤去条件
//!
//! 2026-08-09 の初回実測は判定基準の「微妙」帯に落ち、**mtime 差分スキャンは棚上げになった**。
//! 数字と判断の正本は `ADR-mtime-differential-scan-ceiling` である（ここへ数字を写さない
//! ——環境で数倍ぶれる値であり、写した瞬間に腐る）。
//!
//! **棚上げであって永久却下ではないので、このファイルは残す。** 全走査の間引きが入ったあとに
//! 残る「N 日に 1 回」の代金を、同じ器で測り直すためである。
//!
//! **撤去条件**: #1001 が閉じ、かつ間引き後の代金を測り直す必要が無いと判断した時点で、
//! **このファイルごと消す**（撤去対象はこのファイル 1 つだけで、製品コードには何も足していない）。
//! そのとき `docs/build-commands.md` のコマンド行も同時に消すこと。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use snotra_core::config::Config;
use snotra_core::indexer;

/// 1 つの表現の測定結果。**エラー件数を必ず持つ**——アクセス拒否や消えたディレクトリが
/// 多いと「速い」が「何もしていない」に化ける。
struct Measured {
    label: &'static str,
    elapsed: Duration,
    ok: usize,
    err: usize,
    /// 列挙系だけが持つ、見えた子の総数。0 なら列挙できていない。
    children: usize,
}

impl Measured {
    fn ns_per_dir(&self, n: usize) -> f64 {
        self.elapsed.as_nanos() as f64 / n.max(1) as f64
    }
}

/// `read_dir` して全エントリを回す。**現状の基準**であり、比の分母になる。
fn measure_read_dir(dirs: &[PathBuf]) -> Measured {
    let start = Instant::now();
    let (mut ok, mut err, mut children) = (0, 0, 0);
    for d in dirs {
        match std::fs::read_dir(d) {
            Ok(rd) => {
                ok += 1;
                children += rd.flatten().count();
            }
            Err(_) => err += 1,
        }
    }
    Measured {
        label: "read_dir（全エントリを回す・現状）",
        elapsed: start.elapsed(),
        ok,
        err,
        children,
    }
}

/// `read_dir` に加えて子 1 件ごとに `DirEntry::metadata()` を呼ぶ。
///
/// **Windows では `FindNextFileW` が返した情報の使い回しで追加の syscall が無い**はずで、
/// 真なら上との差はほぼ 0 になる。真であることは「変わったディレクトリ側の費用は
/// 列挙のままで増えない」を意味する。
fn measure_read_dir_with_child_meta(dirs: &[PathBuf]) -> Measured {
    let start = Instant::now();
    let (mut ok, mut err, mut children) = (0, 0, 0);
    for d in dirs {
        match std::fs::read_dir(d) {
            Ok(rd) => {
                ok += 1;
                for e in rd.flatten() {
                    if e.metadata().is_ok() {
                        children += 1;
                    }
                }
            }
            Err(_) => err += 1,
        }
    }
    Measured {
        label: "read_dir + 子ごとに DirEntry::metadata()",
        elapsed: start.elapsed(),
        ok,
        err,
        children,
    }
}

/// `std::fs::metadata(dir)` で mtime を取る。Windows ではハンドルを開く経路。
fn measure_fs_metadata(dirs: &[PathBuf]) -> Measured {
    let start = Instant::now();
    let (mut ok, mut err) = (0, 0);
    for d in dirs {
        match std::fs::metadata(d).and_then(|m| m.modified()) {
            Ok(_) => ok += 1,
            Err(_) => err += 1,
        }
    }
    Measured {
        label: "std::fs::metadata(dir).modified()",
        elapsed: start.elapsed(),
        ok,
        err,
        children: 0,
    }
}

/// `GetFileAttributesExW` で mtime を取る。ハンドルを開かない経路。
#[cfg(windows)]
fn measure_get_file_attributes_ex(dirs: &[PathBuf]) -> Measured {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Storage::FileSystem::{
        GetFileAttributesExW, GetFileExInfoStandard, WIN32_FILE_ATTRIBUTE_DATA,
    };
    use windows::core::PCWSTR;

    let start = Instant::now();
    let (mut ok, mut err) = (0, 0);
    // **バッファは使い回す。** 1 件ごとに Vec を確保すると、測っているものが
    // 属性の読み取りではなく確保になる。
    let mut wide: Vec<u16> = Vec::with_capacity(320);
    for d in dirs {
        wide.clear();
        wide.extend(d.as_os_str().encode_wide());
        wide.push(0);
        let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
        let r = unsafe {
            GetFileAttributesExW(
                PCWSTR(wide.as_ptr()),
                GetFileExInfoStandard,
                (&raw mut data).cast(),
            )
        };
        if r.is_ok() {
            // 読んだ値を使う（最適化で消えないように）。
            ok += (data.ftLastWriteTime.dwLowDateTime != u32::MAX) as usize;
        } else {
            err += 1;
        }
    }
    Measured {
        label: "GetFileAttributesExW（ハンドルを開かない）",
        elapsed: start.elapsed(),
        ok,
        err,
        children: 0,
    }
}

#[cfg(not(windows))]
fn measure_get_file_attributes_ex(_dirs: &[PathBuf]) -> Measured {
    Measured {
        label: "GetFileAttributesExW（Windows 以外では測らない）",
        elapsed: Duration::ZERO,
        ok: 0,
        err: 0,
        children: 0,
    }
}

fn report(m: &Measured, n: usize, baseline_ns: f64) {
    let ns = m.ns_per_dir(n);
    let ratio = if baseline_ns > 0.0 {
        ns / baseline_ns
    } else {
        0.0
    };
    println!(
        "  {:<44} {:>9.0} ms  {:>8.0} ns/dir  比 {:>5.2}  ok {:>7} / err {:<6} 子 {}",
        m.label,
        m.elapsed.as_secs_f64() * 1000.0,
        ns,
        ratio,
        m.ok,
        m.err,
        m.children
    );
}

/// 実 `index.bin` が持つディレクトリ全件に対して 4 つの表現を測る。
///
/// **順序を入れ替えて 2 巡する。** ファイルシステムキャッシュは 1 巡目で暖まるので、
/// 1 巡目の順序がそのまま結果の順序差に化ける。各表現の**最小値**を採る。
#[test]
#[ignore = "手元 release 実行専用（実 index.bin と実ファイルシステムを読む）"]
fn measure_dir_stat_cost_over_real_index() {
    let config = Config::load();
    let Some(entries) =
        indexer::load_cached_entries(&config.paths.scan, config.search.show_hidden_system)
    else {
        println!(
            "実 index.bin が読めない（不在・config hash 不一致・破損のいずれか）ためスキップする"
        );
        return;
    };

    let dirs: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.is_folder)
        .map(|e| PathBuf::from(&e.target_path))
        .collect();
    let n = dirs.len();
    println!(
        "=== 対象: 実 index.bin のフォルダ {n} 件 / 全エントリ {} 件 ===",
        entries.len()
    );
    if n == 0 {
        println!("フォルダが 1 件も無い（include_folders がすべて false）ためスキップする");
        return;
    }
    drop(entries);

    // **暖機を 1 巡入れる。** 入れないと 1 番目に測った表現だけがコールドの罰を受ける。
    let warm = measure_read_dir(&dirs);
    println!(
        "  暖機（read_dir 1 巡）: {:.0} ms・ok {} / err {}",
        warm.elapsed.as_secs_f64() * 1000.0,
        warm.ok,
        warm.err
    );
    println!();

    let round1 = [
        measure_read_dir(&dirs),
        measure_read_dir_with_child_meta(&dirs),
        measure_fs_metadata(&dirs),
        measure_get_file_attributes_ex(&dirs),
    ];
    let round2 = [
        measure_get_file_attributes_ex(&dirs),
        measure_fs_metadata(&dirs),
        measure_read_dir_with_child_meta(&dirs),
        measure_read_dir(&dirs),
    ];

    // 同じ表現同士を突き合わせて最小を採る（round2 は逆順なので添字を反転する）。
    let best: Vec<&Measured> = (0..4)
        .map(|i| {
            let a = &round1[i];
            let b = &round2[3 - i];
            if a.elapsed <= b.elapsed { a } else { b }
        })
        .collect();

    let baseline_ns = best[0].ns_per_dir(n);
    println!("=== 各表現の最小値（2 巡・順序を逆にして測った）===");
    for m in &best {
        report(m, n, baseline_ns);
    }
    println!();

    let stat_ns = best[2].ns_per_dir(n).min(best[3].ns_per_dir(n));
    let ratio = stat_ns / baseline_ns.max(1.0);
    println!("=== 判定（基準は測る前に置いた・この計器の doc が正本）===");
    println!("  属性の読み取り ÷ 列挙 = {ratio:.2}");
    let verdict = if ratio <= 0.3 {
        "≤ 0.3 → 設計を進めてよい"
    } else if ratio < 0.6 {
        "0.3〜0.6 → 微妙。全走査の間引きと並べて比べる"
    } else {
        "≥ 0.6 → この案は却下し、間引きへ倒す"
    };
    println!("  {verdict}");
    println!(
        "  参考: 現状の全走査 {:.1} 秒相当がこの比で置き換わると {:.1} 秒（変更のあった\
         ディレクトリの列挙ぶんは別途上乗せ）",
        best[0].elapsed.as_secs_f64(),
        best[0].elapsed.as_secs_f64() * ratio
    );
    println!();
    println!("  ※ err が多い表現は「速い」ではなく「何もしていない」——上の err 列で確かめること");
    println!("  ※ この数字を恒久文書へ書かない（ファイルシステムキャッシュの温度で数倍ぶれる）");
}
