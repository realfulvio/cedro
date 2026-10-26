//! Forme morbide (angoli arrotondati, cerchi, pillole) con GDI+, per i controlli disegnati a mano:
//! interruttori, pulsanti, etichette e menu delle finestre di Cedro.

use std::sync::OnceLock;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::GdiPlus::*;

fn startup() {
    static TOKEN: OnceLock<usize> = OnceLock::new();
    TOKEN.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        let mut output = GdiplusStartupOutput::default();
        GdiplusStartup(&mut token, &input, &mut output);
        token
    });
}

/// COLORREF (0x00BBGGRR) → ARGB di GDI+.
fn argb(color: COLORREF, alpha: u8) -> u32 {
    let c = color.0;
    ((alpha as u32) << 24) | ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff)
}

/// Un rettangolo con gli angoli di raggio `radius` (in pixel), pieno e/o con il bordo.
/// `fill` e `line` sono (colore, spessore) opzionali.
pub fn round_rect(hdc: HDC, r: &RECT, radius: f32, fill: Option<COLORREF>, line: Option<(COLORREF, f32)>) {
    startup();
    unsafe {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut g) != Status(0) || g.is_null() {
            return;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        // Il bordo sta dentro il rettangolo: si rientra di mezzo spessore.
        let half = line.map(|(_, w)| w / 2.0).unwrap_or(0.0);
        let x = r.left as f32 + half;
        let y = r.top as f32 + half;
        let w = (r.right - r.left) as f32 - 2.0 * half;
        let h = (r.bottom - r.top) as f32 - 2.0 * half;
        let d = (radius * 2.0).min(w).min(h).max(0.0);
        let mut path: *mut GpPath = std::ptr::null_mut();
        GdipCreatePath(FillModeAlternate, &mut path);
        if d <= 0.5 {
            GdipAddPathRectangle(path, x, y, w, h);
        } else {
            GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
        }
        GdipClosePathFigure(path);
        if let Some(color) = fill {
            let mut brush: *mut GpSolidFill = std::ptr::null_mut();
            GdipCreateSolidFill(argb(color, 255), &mut brush);
            GdipFillPath(g, brush as *mut GpBrush, path);
            GdipDeleteBrush(brush as *mut GpBrush);
        }
        if let Some((color, width)) = line {
            let mut pen: *mut GpPen = std::ptr::null_mut();
            GdipCreatePen1(argb(color, 255), width, UnitPixel, &mut pen);
            GdipDrawPath(g, pen, path);
            GdipDeletePen(pen);
        }
        GdipDeletePath(path);
        GdipDeleteGraphics(g);
    }
}

/// Un cerchio pieno con il centro in (`cx`, `cy`).
pub fn circle(hdc: HDC, cx: f32, cy: f32, radius: f32, color: COLORREF) {
    startup();
    unsafe {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut g) != Status(0) || g.is_null() {
            return;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        let mut brush: *mut GpSolidFill = std::ptr::null_mut();
        GdipCreateSolidFill(argb(color, 255), &mut brush);
        GdipFillEllipse(g, brush as *mut GpBrush, cx - radius, cy - radius, radius * 2.0, radius * 2.0);
        GdipDeleteBrush(brush as *mut GpBrush);
        GdipDeleteGraphics(g);
    }
}
