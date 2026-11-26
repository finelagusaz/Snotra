//! この crate が stderr へ書く行の、唯一の出口（`trace_line!`）。
//!
//! 1 行を `String` へ組んでから、`write_all` 1 回で書く。std の `Stderr` はバッファを持たず、
//! `eprintln!` は書式の断片ごとに書くので、1 行が引数の数に応じた回数の書き込みに分かれる。
//! リダイレクト先の書き込みには 1 回ごとに単価がかかる（#1542。数は `PERFORMANCE.md`
//! 「採用: egui の計装の 1 行を 1 回の書き込みにする（#1542）」）。
//!
//! 同じ形は `src-tauri/src/trace.rs` の `render_line` / `write_line` にもある。共有しないのは、
//! 共有できるのが `write_all` の 1 行だけだからである。組み立ては別物で（あちらは `[trace] ` +
//! JSON + `seq` + heap、こちらは書式文字列そのままの素の行）、依存は `src-tauri` → この crate の
//! 向きにしか無い。共有すると、描画ランタイムの公開面に診断の出口を足すことになる。
//!
//! 有効判定（`crate::env::trace_hatch_enabled` など）は呼び出し側に置く。ここでは env を読まない。
//! `eprintln!` が戻らないことは `lib.rs` の `#![deny(clippy::print_stderr)]` が見張る。

use std::fmt;
use std::io::Write;

/// 1 行（書式の結果 + `\n`）を `String` へ組む。
fn render_line(args: fmt::Arguments<'_>) -> String {
    format!("{args}\n")
}

/// 組んだ 1 行を 1 回の `write_all` で書く。失敗は捨てる。診断用の出口なので、書けなくても
/// 製品を止めない（`eprintln!` は書き込みの失敗で panic し、release では `panic = "abort"` により
/// プロセスごと止まる）。
fn write_line<W: Write>(w: &mut W, line: &str) {
    let _ = w.write_all(line.as_bytes());
}

/// `args` を 1 行に組み、`w` へ 1 回で書く。
fn emit_to<W: Write>(w: &mut W, args: fmt::Arguments<'_>) {
    write_line(w, &render_line(args));
}

/// `args` を 1 行に組み、stderr へ 1 回で書く（`trace_line!` の展開先）。
pub(crate) fn emit(args: fmt::Arguments<'_>) {
    emit_to(&mut std::io::stderr(), args);
}

/// `eprintln!` と同じ書式で 1 行を stderr へ書く。書き込みは 1 回である。
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

    /// `SNOTRA_EGUI_PAINT` と同じ数（13）の引数と書式指定を持つ 1 行を書く。
    macro_rules! paint_like {
        ($sink:ident, $($head:tt)*) => {
            $sink!(
                $($head)*
                "SNOTRA_EGUI_PAINT win={} tess_ms={:.2} raster_ms={:.2} total_ms={:.2} meshes={} px={} \
                 surface_kib={} atlas={}x{} atlas_kib={} tex_font_kib={} tex_other_kib={} tex_other_n={}",
                "main", 1.234_f64, 5.678_f64, 9.0_f64, 12_usize, 480_000_u64,
                1875_usize, 2048_usize, 64_usize, 512_usize, 512_usize, 3_usize, 2_usize,
            )
        };
    }

    #[test]
    fn emit_to_writes_one_line_in_exactly_one_write() {
        let mut w = CountingWriter::default();
        emit_to(&mut w, paint_like!(format_args,));
        let expected = format!("{}\n", paint_like!(format,));
        assert_eq!(w.writes, 1, "1 行は 1 回の write で書く");
        assert_eq!(
            w.bytes,
            expected.as_bytes(),
            "バイト列は書式の結果 + 改行 1 つ"
        );
    }

    /// 対照: 同じ行を `writeln!` で書くと、断片ごとの書き込みに分かれる。
    /// 数える仕組みが分割を見分けられることを確かめる（これが 1 なら上のテストは何も測っていない）。
    #[test]
    fn counting_writer_distinguishes_fragmented_writes() {
        let mut w = CountingWriter::default();
        paint_like!(writeln, &mut w,).unwrap();
        assert!(
            w.writes >= 2,
            "writeln! は断片ごとに書く（{} 回）",
            w.writes
        );
    }

    #[test]
    fn render_line_ends_with_exactly_one_newline() {
        let line = render_line(format_args!(
            "SNOTRA_EGUI_INPUT {} ts_ms={} {}",
            "take", 7, "x=1"
        ));
        assert_eq!(line, "SNOTRA_EGUI_INPUT take ts_ms=7 x=1\n");
    }
}
