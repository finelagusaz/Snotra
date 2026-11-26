//! Tauri/Taoとegui/softbufferを接続するSnotra専用ランタイム。

// stderr へ書く行は `stderr_line::trace_line!` を通す（1 行を 1 回の書き込みにする・#1542）。
#![deny(clippy::print_stderr)]

mod env;
mod ime;
mod input;
mod keyboard;
mod monitor;
mod proof;
mod raster;
mod renderer;
mod repaint;
mod runtime;
mod stderr_line;
mod surface;

pub use input::modifiers_from_tao;
pub use proof::{EventLoopProof, on_event_loop};
pub use renderer::CLEAR_COLOR;
pub use repaint::WindowWaker;
pub use runtime::{EguiRuntime, EguiView, RuntimeError, RuntimeFrame};
pub use surface::is_renderable_extent;
