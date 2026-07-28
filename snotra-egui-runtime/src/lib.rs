//! Tauri/Taoとegui/softbufferを接続するSnotra専用ランタイム。

mod ime;
mod input;
mod monitor;
mod raster;
mod renderer;
mod repaint;
mod runtime;
mod surface;

pub use input::{key_from_tao, modifiers_from_tao};
pub use renderer::CLEAR_COLOR;
pub use repaint::WindowWaker;
pub use runtime::{EguiRuntime, EguiView, RuntimeError, RuntimeFrame};
pub use surface::is_renderable_extent;
