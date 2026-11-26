//! この crate が stderr へ書く行の出口（`trace_line!`）。
//!
//! 1 行を `String` へ組んでから、`write_all` 1 回で書く。std の `Stderr` はバッファを持たず、
//! `eprintln!` は書式の断片ごとに書くので、1 行が引数の数に応じた回数の書き込みに分かれる。
//! リダイレクト先への書き込みは、1 回ごとに費用がかかる（#1542。数は `PERFORMANCE.md`
//! 「採用: egui の計装の 1 行を 1 回の書き込みにする（#1542）」）。
//!
//! `src-tauri/src/trace.rs` も同じく 1 行を 1 回で書くが、出口は共有しない。依存は `src-tauri` から
//! この crate への一方向で、共有すると描画ランタイムの公開面に診断の出口が加わるためである。
//!
//! 有効判定（`crate::env::trace_hatch_enabled` など）は呼び出し側に置き、ここでは env を読まない。
//! `eprintln!` へ戻らないよう、`lib.rs` が lint で見張る（射程は同ファイルのコメント）。

use std::fmt;
use std::io::Write;

/// `args` と改行を 1 行に組み、`w` へ 1 回の `write_all` で書く。失敗は捨てる。診断用の出口なので、
/// 書けなくても製品を止めない（`eprintln!` は書き込みの失敗で panic し、release では
/// `panic = "abort"` によりプロセスごと止まる）。
fn emit_to<W: Write>(w: &mut W, args: fmt::Arguments<'_>) {
    let _ = w.write_all(format!("{args}\n").as_bytes());
}

/// `args` を 1 行にして stderr へ書く（`trace_line!` の展開先）。
pub(crate) fn emit(args: fmt::Arguments<'_>) {
    emit_to(&mut std::io::stderr(), args);
}

/// `eprintln!` と同じ書式で、1 行を stderr へ 1 回で書く。
macro_rules! trace_line {
    ($($arg:tt)*) => {
        $crate::stderr_line::emit(format_args!($($arg)*))
    };
}
pub(crate) use trace_line;

#[cfg(test)]
mod tests {
    use super::*;

    /// `write` の呼び出し回数を数え、受け取ったバイト列を貯める。
    #[derive(Default)]
    struct CountingWriter {
        writes: usize,
        bytes: Vec<u8>,
    }

    impl Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn emit_to_writes_one_line_in_exactly_one_write() {
        let mut w = CountingWriter::default();
        emit_to(&mut w, format_args!("SNOTRA_X a={} b={:.2}", 1, 2.0_f64));
        assert_eq!(w.writes, 1, "1 行は 1 回の write で書く");
        assert_eq!(w.bytes, b"SNOTRA_X a=1 b=2.00\n", "書式の結果と改行 1 つ");
    }

    /// 対照: 同じ行を `writeln!` で書くと、断片ごとの書き込みに分かれる。
    /// 数える仕組みが分割を見分けられることを確かめる（これが 1 なら上のテストは何も測っていない）。
    #[test]
    fn counting_writer_distinguishes_fragmented_writes() {
        let mut w = CountingWriter::default();
        writeln!(&mut w, "SNOTRA_X a={} b={:.2}", 1, 2.0_f64).unwrap();
        assert!(
            w.writes >= 2,
            "writeln! は断片ごとに書く（{} 回）",
            w.writes
        );
    }
}
