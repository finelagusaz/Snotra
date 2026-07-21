//! Tauri/Taoとegui/wgpuを接続するSnotra専用ランタイム。

mod gpu;
mod ime;
mod input;
mod raster;
mod renderer;
mod repaint;
mod runtime;
mod surface;

pub use gpu::GpuFaultInjection;
pub use input::{key_from_tao, modifiers_from_tao};
pub use runtime::{EguiRuntime, EguiView, RuntimeError, RuntimeFrame};
pub use surface::{SurfaceAction, is_renderable_extent, surface_action};
