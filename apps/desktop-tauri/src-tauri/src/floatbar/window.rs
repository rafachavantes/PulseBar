//! Detached "FloatBar" window: a small always-on-top transparent strip
//! that shows remaining capacity per provider. Runs as an auxiliary
//! Tauri window labeled `floatbar`, independent of the main surface
//! state machine.

use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::geometry_store;

pub const FLOATBAR_LABEL: &str = "floatbar";
pub const FLOAT_BAR_CONFIG_CHANGED_EVENT: &str = "float-bar-config-changed";
const FLOATBAR_DEFAULT_WIDTH_H: f64 = 360.0;
const FLOATBAR_DEFAULT_HEIGHT_H: f64 = 36.0;
const FLOATBAR_DEFAULT_WIDTH_V: f64 = 80.0;
const FLOATBAR_DEFAULT_HEIGHT_V: f64 = 280.0;

/// Initial dimensions (logical pixels) for the floating bar given an
/// orientation string. Unknown values fall back to horizontal so callers
/// don't have to pre-validate.
pub fn initial_size(orientation: &str) -> (f64, f64) {
    match orientation {
        "vertical" => (FLOATBAR_DEFAULT_WIDTH_V, FLOATBAR_DEFAULT_HEIGHT_V),
        _ => (FLOATBAR_DEFAULT_WIDTH_H, FLOATBAR_DEFAULT_HEIGHT_H),
    }
}

/// Convert a 0..=100 opacity value to a Win32 SetLayeredWindowAttributes
/// alpha byte (0..=255). Values below 30 are clamped so the bar is never
/// fully invisible — that would be a usability footgun.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn opacity_to_alpha(opacity: u8) -> u8 {
    let clamped = opacity.clamp(30, 100);
    ((clamped as u32) * 255 / 100) as u8
}

/// Open the floating-bar window, or focus + reapply attributes if already
/// open. Position is restored from the geometry store keyed by
/// `floatbar`; on first launch the window is centered horizontally near
/// the top of the primary monitor.
pub fn show(
    app: &tauri::AppHandle,
    opacity: u8,
    orientation: &str,
    style: &str,
    click_through: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLOATBAR_LABEL) {
        apply_no_activate(&window);
        apply_opacity(&window, opacity);
        apply_click_through(&window, click_through);
        window.show().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let (w, h) = initial_size(orientation);
    let url =
        WebviewUrl::App(format!("index.html?window=floatbar&orientation={orientation}").into());

    let builder = tauri::WebviewWindowBuilder::new(app, FLOATBAR_LABEL, url)
        .title("PulseBar Float Bar")
        .inner_size(w, h)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true);

    // WebView2 only honors an alpha (transparent) background when the native
    // window is itself created transparent. Tauri cfg-gates this builder API
    // off on macOS unless `macos-private-api` is enabled, so keep the Windows
    // fix out of the macOS validation path.
    #[cfg(windows)]
    let builder = builder.transparent(true);

    let win = builder
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    // Restore prior geometry if we have one. Otherwise, taskbar style opens
    // near the bottom while the original floating style keeps its top-center
    // placement.
    if let Some(g) = geometry_store::load_entry(FLOATBAR_LABEL) {
        // Clamp the restored rect into a current monitor work area. Without
        // this, a saved position from an unplugged/rearranged monitor can put
        // the bar fully off-screen with no taskbar/Alt-Tab/title bar to recover
        // it — and hide() would persist those off-screen coords.
        let stored_w = g.width.map(|v| v as f64).unwrap_or(w);
        let stored_h = g.height.map(|v| v as f64).unwrap_or(h);
        let monitors = floatbar_monitor_work_areas(&win);
        let (cx, cy, cw, ch) =
            clamp_floatbar_to_monitors(&monitors, g.x as f64, g.y as f64, stored_w, stored_h);
        let _ = win.set_position(LogicalPosition::new(cx, cy));
        let _ = win.set_size(LogicalSize::new(cw, ch));
    } else if let Ok(Some(monitor)) = win.primary_monitor() {
        let scale = win.scale_factor().unwrap_or(1.0);
        let mon_x = monitor.position().x as f64 / scale;
        let mon_y = monitor.position().y as f64 / scale;
        let mon_w = monitor.size().width as f64 / scale;
        let mon_h = monitor.size().height as f64 / scale;
        let x = mon_x + (mon_w - w) / 2.0;
        let y = if style == "taskbar" {
            mon_y + mon_h - h - 8.0
        } else {
            mon_y + 8.0
        };
        let _ = win.set_position(LogicalPosition::new(x.max(mon_x), y.max(mon_y)));
    }

    apply_no_activate(&win);
    apply_opacity(&win, opacity);
    apply_click_through(&win, click_through);
    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

/// A monitor work area expressed in LOGICAL pixels.
#[derive(Clone, Copy, Debug)]
struct LogicalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn logical_point_in(rect: &LogicalRect, x: f64, y: f64) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// Collect each connected monitor's work area in logical pixels (physical work
/// area divided by that monitor's own scale factor).
fn floatbar_monitor_work_areas(win: &tauri::WebviewWindow) -> Vec<LogicalRect> {
    let Ok(monitors) = win.available_monitors() else {
        return Vec::new();
    };
    monitors
        .iter()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let scale = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            let wa = monitor.work_area();
            LogicalRect {
                x: wa.position.x as f64 / scale,
                y: wa.position.y as f64 / scale,
                width: wa.size.width as f64 / scale,
                height: wa.size.height as f64 / scale,
            }
        })
        .collect()
}

/// Clamp a logical window rect so it stays visible within one of `monitors`.
///
/// Picks the monitor containing the window's top-left (then its centre, then
/// the first monitor as a fallback), shrinks the size to fit that work area if
/// needed, and clamps the position so the whole window stays on-screen. With no
/// monitor info the rect is returned unchanged.
fn clamp_floatbar_to_monitors(
    monitors: &[LogicalRect],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> (f64, f64, f64, f64) {
    let Some(&first) = monitors.first() else {
        return (x, y, width, height);
    };

    let target = monitors
        .iter()
        .find(|m| logical_point_in(m, x, y))
        .or_else(|| {
            monitors
                .iter()
                .find(|m| logical_point_in(m, x + width / 2.0, y + height / 2.0))
        })
        .copied()
        .unwrap_or(first);

    let w = width.min(target.width).max(1.0);
    let h = height.min(target.height).max(1.0);
    // `max(target.*)` guards against a work area smaller than the window.
    let max_x = (target.x + target.width - w).max(target.x);
    let max_y = (target.y + target.height - h).max(target.y);
    let cx = x.clamp(target.x, max_x);
    let cy = y.clamp(target.y, max_y);
    (cx, cy, w, h)
}

/// Hide / destroy the floating bar.
pub fn hide(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLOATBAR_LABEL) {
        // Persist position before closing so it reopens in place.
        remember_geometry(&window);
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Capture current position into the geometry store under the floatbar key.
///
/// Accepts any Tauri window handle (`Window` from event callbacks or
/// `WebviewWindow` from `get_webview_window`), since `WindowEvent`
/// callbacks deliver a `&Window` while imperative call sites have a
/// `&WebviewWindow`.
pub fn remember_geometry<R: tauri::Runtime, M: WindowGeometry<R>>(window: &M) {
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    geometry_store::save_entry(
        FLOATBAR_LABEL,
        geometry_store::StoredGeometry {
            x: (pos.x as f64 / scale).round() as i32,
            y: (pos.y as f64 / scale).round() as i32,
            width: Some((size.width as f64 / scale).round() as u32),
            height: Some((size.height as f64 / scale).round() as u32),
        },
    );
}

/// Subset of `tauri::WebviewWindow` / `tauri::Window` used by
/// [`remember_geometry`]. Both types implement the underlying methods, but
/// they don't share a public trait — this private trait bridges them so we
/// can be called from `WindowEvent` (which delivers `&Window`) and from
/// imperative paths (which hold `&WebviewWindow`).
pub trait WindowGeometry<R: tauri::Runtime> {
    fn outer_position(&self) -> tauri::Result<tauri::PhysicalPosition<i32>>;
    fn outer_size(&self) -> tauri::Result<tauri::PhysicalSize<u32>>;
    fn scale_factor(&self) -> tauri::Result<f64>;
}

impl<R: tauri::Runtime> WindowGeometry<R> for tauri::WebviewWindow<R> {
    fn outer_position(&self) -> tauri::Result<tauri::PhysicalPosition<i32>> {
        tauri::WebviewWindow::outer_position(self)
    }
    fn outer_size(&self) -> tauri::Result<tauri::PhysicalSize<u32>> {
        tauri::WebviewWindow::outer_size(self)
    }
    fn scale_factor(&self) -> tauri::Result<f64> {
        tauri::WebviewWindow::scale_factor(self)
    }
}

impl<R: tauri::Runtime> WindowGeometry<R> for tauri::Window<R> {
    fn outer_position(&self) -> tauri::Result<tauri::PhysicalPosition<i32>> {
        tauri::Window::outer_position(self)
    }
    fn outer_size(&self) -> tauri::Result<tauri::PhysicalSize<u32>> {
        tauri::Window::outer_size(self)
    }
    fn scale_factor(&self) -> tauri::Result<f64> {
        tauri::Window::scale_factor(self)
    }
}

/// Resize the floatbar to the given logical dimensions and re-assert the
/// native interaction invariants in the same step.
///
/// A resize goes through `SetWindowPos`/frame changes, which can drop the
/// extended window styles, so the no-activate and click-through flags must be
/// re-applied afterwards. Keeping both halves here gives callers (including the
/// webview) a single canonical "the bar changed size" entry point instead of
/// pairing a JS `setSize` with a separate native repair command.
pub fn resize(
    window: &tauri::WebviewWindow,
    width: f64,
    height: f64,
    click_through: bool,
) -> Result<(), String> {
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    apply_no_activate(window);
    apply_click_through(window, click_through);
    Ok(())
}

/// Apply the current opacity setting to an existing floatbar window via
/// `SetLayeredWindowAttributes`. No-op on non-Windows platforms.
pub fn apply_opacity(window: &tauri::WebviewWindow, opacity: u8) {
    let _ = (window, opacity);
    #[cfg(windows)]
    {
        use raw_window_handle::HasWindowHandle;
        let alpha = opacity_to_alpha(opacity);
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
            return;
        };
        unsafe {
            // Ensure WS_EX_LAYERED is set so SetLayeredWindowAttributes works.
            const WS_EX_LAYERED: isize = 0x00080000;
            let ex = GetWindowLongPtrW(h.hwnd.get(), GWL_EXSTYLE);
            if ex & WS_EX_LAYERED == 0 {
                set_extended_style(h.hwnd.get(), ex | WS_EX_LAYERED);
            }
            const LWA_ALPHA: u32 = 0x00000002;
            SetLayeredWindowAttributes(h.hwnd.get(), 0, alpha, LWA_ALPHA);
        }
    }
}

/// Keep the floatbar from activating when it is shown or clicked. This makes
/// it behave like a desktop widget that visually sits above the taskbar without
/// stealing focus from the active app.
pub fn apply_no_activate(window: &tauri::WebviewWindow) {
    let _ = window;
    #[cfg(windows)]
    {
        use raw_window_handle::HasWindowHandle;
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
            return;
        };
        unsafe {
            const WS_EX_NOACTIVATE: isize = 0x08000000;
            let ex = GetWindowLongPtrW(h.hwnd.get(), GWL_EXSTYLE);
            if ex & WS_EX_NOACTIVATE == 0 {
                set_extended_style(h.hwnd.get(), ex | WS_EX_NOACTIVATE);
            }
        }
    }
}

/// Toggle click-through (`WS_EX_TRANSPARENT`). When enabled, mouse events
/// pass through to the window beneath — true overlay mode.
pub fn apply_click_through(window: &tauri::WebviewWindow, click_through: bool) {
    let _ = (window, click_through);
    #[cfg(windows)]
    {
        use raw_window_handle::HasWindowHandle;
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
            return;
        };
        unsafe {
            const WS_EX_LAYERED: isize = 0x00080000;
            const WS_EX_TRANSPARENT: isize = 0x00000020;
            let ex = GetWindowLongPtrW(h.hwnd.get(), GWL_EXSTYLE);
            let mut new_ex = ex | WS_EX_LAYERED;
            if click_through {
                new_ex |= WS_EX_TRANSPARENT;
            } else {
                new_ex &= !WS_EX_TRANSPARENT;
            }
            if new_ex != ex {
                set_extended_style(h.hwnd.get(), new_ex);
            }
        }
    }
}

#[cfg(windows)]
const GWL_EXSTYLE: i32 = -20;

#[cfg(windows)]
unsafe fn set_extended_style(hwnd: isize, ex_style: isize) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;
        const SWP_FRAMECHANGED: u32 = 0x0020;
        let flags = SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
        SetWindowPos(hwnd, 0, 0, 0, 0, 0, flags);
    }
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new: isize) -> isize;
    fn SetLayeredWindowAttributes(hwnd: isize, color_key: u32, alpha: u8, flags: u32) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        hwnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_to_alpha_clamps_low_values() {
        assert_eq!(opacity_to_alpha(0), opacity_to_alpha(30));
        assert_eq!(opacity_to_alpha(10), opacity_to_alpha(30));
    }

    #[test]
    fn opacity_to_alpha_full_is_255() {
        assert_eq!(opacity_to_alpha(100), 255);
    }

    #[test]
    fn opacity_to_alpha_is_monotonic() {
        let a = opacity_to_alpha(30);
        let b = opacity_to_alpha(60);
        let c = opacity_to_alpha(100);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn opacity_to_alpha_midpoint() {
        // 50% should be roughly half of 255.
        let alpha = opacity_to_alpha(50);
        assert!((125..=130).contains(&alpha), "got {alpha}");
    }

    #[test]
    fn initial_size_picks_orientation() {
        assert_eq!(
            initial_size("horizontal"),
            (FLOATBAR_DEFAULT_WIDTH_H, FLOATBAR_DEFAULT_HEIGHT_H)
        );
        assert_eq!(
            initial_size("vertical"),
            (FLOATBAR_DEFAULT_WIDTH_V, FLOATBAR_DEFAULT_HEIGHT_V)
        );
        // Unknown values fall through to horizontal so a corrupted setting
        // can't yield an unreadable strip.
        assert_eq!(
            initial_size("diagonal"),
            (FLOATBAR_DEFAULT_WIDTH_H, FLOATBAR_DEFAULT_HEIGHT_H)
        );
    }

    fn hd() -> LogicalRect {
        LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1040.0,
        }
    }

    #[test]
    fn clamp_keeps_on_screen_position_untouched() {
        let (x, y, w, h) = clamp_floatbar_to_monitors(&[hd()], 800.0, 20.0, 360.0, 36.0);
        assert_eq!((x, y, w, h), (800.0, 20.0, 360.0, 36.0));
    }

    #[test]
    fn clamp_pulls_offscreen_bar_back_into_view() {
        // Saved far off to the left/top (e.g. an unplugged external monitor).
        let (x, y, w, h) = clamp_floatbar_to_monitors(&[hd()], -5000.0, -3000.0, 360.0, 36.0);
        assert!(x >= 0.0 && x + w <= 1920.0, "x={x} w={w}");
        assert!(y >= 0.0 && y + h <= 1040.0, "y={y} h={h}");
    }

    #[test]
    fn clamp_pulls_bar_back_from_bottom_right() {
        let (x, y, w, h) = clamp_floatbar_to_monitors(&[hd()], 9000.0, 9000.0, 360.0, 36.0);
        assert!(x + w <= 1920.0, "x={x} w={w}");
        assert!(y + h <= 1040.0, "y={y} h={h}");
    }

    #[test]
    fn clamp_prefers_monitor_containing_the_point() {
        let primary = hd();
        let secondary = LogicalRect {
            x: 1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        // A point on the secondary monitor stays there.
        let (x, _, _, _) =
            clamp_floatbar_to_monitors(&[primary, secondary], 2500.0, 40.0, 360.0, 36.0);
        assert!(
            x >= 1920.0,
            "should stay on the secondary monitor, got x={x}"
        );
    }

    #[test]
    fn clamp_shrinks_window_larger_than_work_area() {
        let small = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let (x, y, w, h) = clamp_floatbar_to_monitors(&[small], 50.0, 50.0, 360.0, 300.0);
        assert!(w <= 200.0 && h <= 100.0);
        assert!(x >= 0.0 && x + w <= 200.0);
        assert!(y >= 0.0 && y + h <= 100.0);
    }

    #[test]
    fn clamp_without_monitors_is_identity() {
        let (x, y, w, h) = clamp_floatbar_to_monitors(&[], 123.0, 456.0, 360.0, 36.0);
        assert_eq!((x, y, w, h), (123.0, 456.0, 360.0, 36.0));
    }
}
