//! Pixel-level tray icon renderer, decoupled from any platform icon API.
//!
//! Returns raw RGBA bytes so callers (egui tray manager, Tauri shell, tests)
//! can adapt the result to their own icon type without pulling in extra deps.
//!
//! The icon is rendered at 2x the historical logical size (a cheap
//! supersample): Windows downscales the tray icon to the DPI-appropriate size
//! (~16-32px), so handing it a higher-resolution bitmap yields anti-aliased,
//! sharper glyphs and bars at every display scale. Layout constants are kept
//! in the original 32px space via [`px`] so proportions are unchanged.

use image::{ImageBuffer, Rgba, RgbaImage};

use super::severity::Severity;

/// Side length of the generated tray icon in pixels (2x supersample of the
/// original 32px design so downscaling stays crisp on 125/150/200% displays).
pub const TRAY_ICON_SIZE: u32 = 64;

/// Map a coordinate from the original 32px design space into the current
/// supersampled canvas so all layout math stays proportional to [`TRAY_ICON_SIZE`].
#[inline]
fn px(n: u32) -> u32 {
    n * TRAY_ICON_SIZE / 32
}

fn desaturate_if_error((r, g, b): (u8, u8, u8), has_error: bool) -> (u8, u8, u8) {
    if has_error {
        let gray = ((r as u16 + g as u16 + b as u16) / 3) as u8;
        (gray, gray, gray)
    } else {
        (r, g, b)
    }
}

/// Render a usage-bar tray icon as raw RGBA bytes.
///
/// - `session_fill`: primary bar fill length (0–100, a *display* percentage
///   that may be "remaining").
/// - `session_severity`: colour for the primary bar, always derived from the
///   **used** percentage so a healthy account never renders as critical.
/// - `weekly`: optional secondary bar as `(fill, severity)`. When `Some`, two
///   thin bars are drawn (session top, weekly bottom); when `None`, a single
///   thick bar is drawn instead.
/// - `has_error`: desaturate all bar colours to grey to signal an error state.
///
/// Returns `(rgba_bytes, width, height)` for a [`TRAY_ICON_SIZE`]²  icon.
pub fn render_bar_icon_rgba(
    session_fill: f64,
    session_severity: Severity,
    weekly: Option<(f64, Severity)>,
    has_error: bool,
) -> (Vec<u8>, u32, u32) {
    const SZ: u32 = TRAY_ICON_SIZE;
    let mut img: RgbaImage = ImageBuffer::new(SZ, SZ);

    for pixel in img.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 0]);
    }

    let bg_alpha: u8 = if has_error { 180 } else { 255 };
    let bg_color = Rgba([60, 60, 70, bg_alpha]);
    for y in px(2)..SZ - px(2) {
        for x in px(2)..SZ - px(2) {
            img.put_pixel(x, y, bg_color);
        }
    }

    let bar_left = px(4);
    let bar_right = SZ - px(4);
    let bar_width = bar_right - bar_left;

    let fill_px = |pct: f64| ((pct.clamp(0.0, 100.0) / 100.0) * bar_width as f64) as u32;

    let mut draw_bar = |y_start: u32, y_end: u32, pct: f64, severity: Severity| {
        let (r, g, b) = desaturate_if_error(severity.color(), has_error);
        let fill_end = (bar_left + fill_px(pct)).min(bar_right);
        for y in y_start..y_end {
            for x in bar_left..bar_right {
                img.put_pixel(x, y, Rgba([80, 80, 90, 255]));
            }
        }
        for y in y_start..y_end {
            for x in bar_left..fill_end {
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
    };

    match weekly {
        Some((weekly_fill, weekly_severity)) => {
            draw_bar(px(8), px(15), session_fill, session_severity); // session (top, thicker)
            draw_bar(px(18), px(23), weekly_fill, weekly_severity); // weekly (bottom, thinner)
        }
        None => {
            draw_bar(px(10), px(22), session_fill, session_severity); // single thick bar (centred)
        }
    }

    (img.into_raw(), SZ, SZ)
}

/// Render a compact numeric percent tray icon as raw RGBA bytes.
///
/// - `percent`: the number to display (a *display* percentage, may be remaining).
/// - `severity`: glyph colour, always derived from the **used** percentage.
/// - `has_error`: desaturate to grey.
pub fn render_percent_icon_rgba(
    percent: f64,
    severity: Severity,
    has_error: bool,
) -> (Vec<u8>, u32, u32) {
    const SZ: u32 = TRAY_ICON_SIZE;
    let mut img: RgbaImage = ImageBuffer::new(SZ, SZ);

    for pixel in img.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 0]);
    }

    let bg_alpha: u8 = if has_error { 180 } else { 255 };
    for y in px(2)..SZ - px(2) {
        for x in px(2)..SZ - px(2) {
            img.put_pixel(x, y, Rgba([60, 60, 70, bg_alpha]));
        }
    }

    let pct = percent.clamp(0.0, 100.0).round() as u32;
    let text = if pct >= 100 {
        "100".to_string()
    } else {
        format!("{pct}%")
    };
    let glyph_width = 3u32;
    let glyph_gap = px(1).max(1);
    // Scale the 3x5 bitmap font up for the supersampled canvas; wider strings
    // use a smaller scale so three glyphs still fit.
    let scale = if text.len() >= 3 {
        px(2).max(1)
    } else {
        px(3).max(1)
    };
    let text_width = text.len() as u32 * glyph_width * scale + (text.len() as u32 - 1) * glyph_gap;
    let text_height = 5 * scale;
    let start_x = (SZ.saturating_sub(text_width)) / 2;
    let start_y = (SZ.saturating_sub(text_height)) / 2;

    let (r, g, b) = desaturate_if_error(severity.color(), has_error);
    let color = Rgba([r, g, b, 255]);
    // Near-black drop shadow (dev-core #0A0F0B) lifts the glyphs off the grey
    // plate for legibility once Windows downscales the icon.
    let shadow = Rgba([10, 15, 12, 220]);
    let shadow_offset = px(1).max(1);

    let mut x = start_x;
    for ch in text.chars() {
        draw_glyph(
            &mut img,
            ch,
            x + shadow_offset,
            start_y + shadow_offset,
            scale,
            shadow,
        );
        draw_glyph(&mut img, ch, x, start_y, scale, color);
        x += glyph_width * scale + glyph_gap;
    }

    (img.into_raw(), SZ, SZ)
}

fn draw_glyph(img: &mut RgbaImage, ch: char, x: u32, y: u32, scale: u32, color: Rgba<u8>) {
    let Some(rows) = glyph_rows(ch) else {
        return;
    };
    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..3 {
            let bit = 1 << (2 - col);
            if row & bit == 0 {
                continue;
            }
            for yy in 0..scale {
                for xx in 0..scale {
                    let px = x + col * scale + xx;
                    let py = y + row_idx as u32 * scale + yy;
                    if px < TRAY_ICON_SIZE && py < TRAY_ICON_SIZE {
                        img.put_pixel(px, py, color);
                    }
                }
            }
        }
    }
}

fn glyph_rows(ch: char) -> Option<[u8; 5]> {
    Some(match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_produces_correct_dimensions() {
        let (rgba, w, h) = render_bar_icon_rgba(50.0, Severity::Healthy, None, false);
        assert_eq!(w, TRAY_ICON_SIZE);
        assert_eq!(h, TRAY_ICON_SIZE);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn render_two_bar_has_correct_size() {
        let (rgba, w, h) =
            render_bar_icon_rgba(30.0, Severity::Healthy, Some((60.0, Severity::Warn)), false);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    // A pixel that sits inside the single centred bar track (design-space 8,16
    // scaled to the current canvas).
    fn bar_sample_index(w: u32) -> usize {
        let x = px(8);
        let y = px(16);
        ((y * w + x) * 4) as usize
    }

    #[test]
    fn zero_fill_gives_gray_only_bar() {
        let (rgba, w, _h) = render_bar_icon_rgba(0.0, Severity::Healthy, None, false);
        let idx = bar_sample_index(w);
        // Should be the gray track colour, not a usage colour.
        assert_eq!(rgba[idx], 80); // R
        assert_eq!(rgba[idx + 1], 80); // G
        assert_eq!(rgba[idx + 2], 90); // B
    }

    #[test]
    fn full_fill_uses_severity_color_not_fill_percent() {
        // Decoupling guard (#5): a full-length bar coloured Healthy must render
        // green, NOT the red a "100% used" reading would have produced.
        let (rgba, w, _h) = render_bar_icon_rgba(100.0, Severity::Healthy, None, false);
        let idx = bar_sample_index(w);
        let (er, eg, eb) = Severity::Healthy.color();
        assert_eq!(rgba[idx], er);
        assert_eq!(rgba[idx + 1], eg);
        assert_eq!(rgba[idx + 2], eb);

        let (rgba_crit, _, _) = render_bar_icon_rgba(100.0, Severity::Critical, None, false);
        let (cr, cg, cb) = Severity::Critical.color();
        assert_eq!(rgba_crit[idx], cr);
        assert_eq!(rgba_crit[idx + 1], cg);
        assert_eq!(rgba_crit[idx + 2], cb);
    }

    #[test]
    fn error_state_desaturates_colors() {
        let (normal, w, _) = render_bar_icon_rgba(100.0, Severity::Critical, None, false);
        let (error, _, _) = render_bar_icon_rgba(100.0, Severity::Critical, None, true);
        let idx = bar_sample_index(w);
        assert_ne!(normal[idx], normal[idx + 1]); // colour has distinct channels
        assert_eq!(error[idx], error[idx + 1]); // grey: R == G
        assert_eq!(error[idx + 1], error[idx + 2]); // grey: G == B
    }

    #[test]
    fn percent_icon_produces_correct_dimensions() {
        let (rgba, w, h) = render_percent_icon_rgba(72.0, Severity::Warn, false);
        assert_eq!(w, TRAY_ICON_SIZE);
        assert_eq!(h, TRAY_ICON_SIZE);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn percent_icon_draws_visible_text() {
        let (rgba, _, _) = render_percent_icon_rgba(72.0, Severity::Warn, false);
        assert!(rgba.chunks_exact(4).any(|px| px[3] == 255 && px[0] != 60));
    }

    #[test]
    fn percent_icon_clamps_to_hundred() {
        let (rgba, w, h) = render_percent_icon_rgba(125.0, Severity::Critical, false);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }
}
