//! Win32 monitor helpers for multi-monitor window positioning.
//!
//! All coordinates are in physical (screen) pixels.

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Monitor work area in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WorkArea {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// Clamp a point so that a window of given size stays within this work area.
    /// If the window is wider/taller than the work area, aligns to top-left
    /// (the window will overflow to the right/bottom but the title area remains accessible).
    pub fn clamp(&self, x: i32, y: i32, win_w: i32, win_h: i32) -> (i32, i32) {
        let max_x = (self.right - win_w).max(self.left);
        let max_y = (self.bottom - win_h).max(self.top);
        let cx = x.max(self.left).min(max_x);
        let cy = y.max(self.top).min(max_y);
        (cx, cy)
    }

    /// Center coordinates for a window of given size within this work area.
    pub fn center(&self, win_w: i32, win_h: i32) -> (i32, i32) {
        let cx = self.left + (self.width() - win_w) / 2;
        let cy = self.top + (self.height() - win_h) / 2;
        (cx, cy)
    }
}

/// Get the work area of the monitor containing the mouse cursor.
/// Falls back to primary monitor if cursor position cannot be retrieved.
#[cfg(windows)]
pub fn cursor_monitor_work_area() -> Option<WorkArea> {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return primary_monitor_work_area();
        }
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        work_area_from_hmonitor(hmon)
    }
}

/// Get the work area of the primary monitor.
#[cfg(windows)]
pub fn primary_monitor_work_area() -> Option<WorkArea> {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        work_area_from_hmonitor(hmon)
    }
}

/// Get the work area of the monitor containing the given physical point.
///
/// **可視中の位置クランプ（#738）はこちらを使い、`window_monitor_work_area` を使わない。**
/// `MonitorFromWindow` は**ウィンドウ全体の矩形**の重なりでモニターを選ぶため、上下に並んだ
/// モニターで status 行 + toast 行が出て下側へ大きく伸びると基準が下側へ移り、**行が出た
/// だけでバーが隣モニターへ飛ぶ**（`SPEC.md` §4.7「バーの位置は行の出没で動かさない」を
/// 破る）。呼び出し側は `layout::bar_rect_center` が返すバー矩形の中心を渡す——バー矩形は
/// 行の出没で変わらないので、基準モニターも動かない。
///
/// `MONITOR_DEFAULTTONEAREST` ゆえ、点がどのモニターにも乗っていなくても最寄りを返す
/// （完全に画面外へドラッグされた窓を復帰させるのに要る）。
#[cfg(windows)]
pub fn point_monitor_work_area(x: i32, y: i32) -> Option<WorkArea> {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        work_area_from_hmonitor(hmon)
    }
}

/// Get the work area of the monitor containing the given window.
#[cfg(windows)]
pub fn window_monitor_work_area(hwnd_raw: isize) -> Option<WorkArea> {
    unsafe {
        let hmon = MonitorFromWindow(HWND(hwnd_raw as *mut _), MONITOR_DEFAULTTONEAREST);
        work_area_from_hmonitor(hmon)
    }
}

#[cfg(windows)]
unsafe fn work_area_from_hmonitor(
    hmon: windows::Win32::Graphics::Gdi::HMONITOR,
) -> Option<WorkArea> {
    unsafe {
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return None;
        }
        let rc = mi.rcWork;
        Some(WorkArea {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WorkArea;

    /// 単一モニター相当の標準的な作業領域（1920x1080、原点は左上）。
    fn area() -> WorkArea {
        WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }

    #[test]
    fn clamp_within_bounds_is_unchanged() {
        let a = area();
        assert_eq!(a.clamp(100, 100, 400, 300), (100, 100));
    }

    #[test]
    fn clamp_position_beyond_bottom_right_clamps_to_max() {
        let a = area();
        // max_x = 1920-400=1520, max_y = 1080-300=780
        assert_eq!(a.clamp(2000, 2000, 400, 300), (1520, 780));
    }

    #[test]
    fn clamp_negative_position_clamps_to_top_left() {
        let a = area();
        assert_eq!(a.clamp(-500, -500, 400, 300), (0, 0));
    }

    #[test]
    fn clamp_exact_boundary_is_stable() {
        // x/y already equal max_x/max_y: applying clamp again must not shift by one.
        let a = area();
        assert_eq!(a.clamp(1520, 780, 400, 300), (1520, 780));
    }

    #[test]
    fn clamp_window_wider_than_work_area_aligns_to_left() {
        // win_w(2000) > width(1920): max_x = (1920-2000).max(left) = left(0)
        let a = area();
        assert_eq!(a.clamp(500, 100, 2000, 300), (0, 100));
    }

    #[test]
    fn clamp_window_taller_than_work_area_aligns_to_top() {
        let a = area();
        assert_eq!(a.clamp(100, 500, 400, 2000), (100, 0));
    }

    #[test]
    fn clamp_negative_origin_monitor() {
        // secondary monitor positioned to the left of the primary (negative coordinates).
        let a = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(a.clamp(-1000, 500, 400, 300), (-1000, 500));
        // beyond this monitor's own right edge clamps to its own max_x (-400), not 0.
        assert_eq!(a.clamp(500, 500, 400, 300), (-400, 500));
    }

    #[test]
    fn center_normal_case() {
        let a = area();
        assert_eq!(a.center(800, 600), (560, 240));
    }

    #[test]
    fn center_truncates_odd_remainder_toward_left_top() {
        // width 1921 - win_w 800 = 1121 (odd); integer division truncates 560.5 -> 560.
        let a = WorkArea {
            left: 0,
            top: 0,
            right: 1921,
            bottom: 1080,
        };
        assert_eq!(a.center(800, 600), (560, 240));
    }

    #[test]
    fn center_window_larger_than_work_area_goes_negative() {
        // unlike clamp, center has no lower bound: an oversized window centers off-screen.
        let a = area();
        assert_eq!(a.center(2000, 1200), (-40, -60));
    }

    #[test]
    fn center_negative_origin_monitor() {
        let a = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(a.center(800, 600), (-1360, 240));
    }
}
