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

/// Il logo di «hcok» (vedi `cedro::logo`) con GDI+. `left` e `baseline` sono in pixel: il bordo sinistro del disegno e la
/// linea di base delle lettere. Con `with_real` la scritta «real» sta attaccata davanti.
pub fn draw_logo(hdc: HDC, left: f32, baseline: f32, scale: f32, with_real: bool, ink: COLORREF, accent: COLORREF, real: COLORREF) {
    use cedro::logo::{items, span, Shape, Tone, BASELINE};
    startup();
    unsafe {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut g) != Status(0) || g.is_null() {
            return;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
        let (l, _) = span(with_real);
        let ox = left - l * scale;
        let oy = baseline - BASELINE * scale;
        for (item, dx) in items(with_real) {
            let color = match item.tone {
                Tone::Ink => ink,
                Tone::Accent => accent,
                Tone::Real => real,
            };
            let px = |x: f32| ox + (x + dx) * scale;
            let py = |y: f32| oy + y * scale;
            if let Shape::Rect(x, y, w, h) = item.shape {
                let mut brush: *mut GpSolidFill = std::ptr::null_mut();
                GdipCreateSolidFill(argb(color, 255), &mut brush);
                GdipFillRectangle(g, brush as *mut GpBrush, px(x), py(y), w * scale, h * scale);
                GdipDeleteBrush(brush as *mut GpBrush);
                continue;
            }
            let mut pen: *mut GpPen = std::ptr::null_mut();
            GdipCreatePen1(argb(color, 255), item.width * scale, UnitPixel, &mut pen);
            GdipSetPenLineJoin(pen, LineJoinMiter);
            match item.shape {
                Shape::Line(x1, y1, x2, y2) => {
                    GdipDrawLine(g, pen, px(x1), py(y1), px(x2), py(y2));
                }
                Shape::Arc(cx, cy, r, start, sweep) => {
                    GdipDrawArc(g, pen, px(cx - r), py(cy - r), 2.0 * r * scale, 2.0 * r * scale, start, sweep);
                }
                Shape::Circle(cx, cy, r) => {
                    GdipDrawEllipse(g, pen, px(cx - r), py(cy - r), 2.0 * r * scale, 2.0 * r * scale);
                }
                Shape::Curve(p) => {
                    GdipDrawBezier(g, pen, px(p[0].0), py(p[0].1), px(p[1].0), py(p[1].1), px(p[2].0), py(p[2].1), px(p[3].0), py(p[3].1));
                }
                Shape::Poly(p) => {
                    let pts = [PointF { X: px(p[0].0), Y: py(p[0].1) }, PointF { X: px(p[1].0), Y: py(p[1].1) }, PointF { X: px(p[2].0), Y: py(p[2].1) }];
                    GdipDrawLines(g, pen, pts.as_ptr(), 3);
                }
                Shape::Rect(..) => {}
            }
            GdipDeletePen(pen);
        }
        GdipDeleteGraphics(g);
    }
}

/// `PixelFormat32bppPARGB` di GDI+ (Gdipluspixelformats.h): BGRA con l'alfa premoltiplicato.
const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

/// Uno stemma del Comune (vedi `cedro::edizione`) nel riquadro `x`, `y`, `w`, `h` in pixel, ricampionato bene.
/// Senza il tema Bondeno non disegna niente.
pub fn draw_stemma(hdc: HDC, stemma: cedro::edizione::Stemma, x: f32, y: f32, w: f32, h: f32) {
    let Some(img) = cedro::edizione::immagine(stemma) else { return };
    startup();
    unsafe {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut g) != Status(0) || g.is_null() {
            return;
        }
        // GDI+ legge i pixel dove stanno: la bitmap non copia niente e si butta subito dopo.
        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let stride = (img.w * 4) as i32;
        if GdipCreateBitmapFromScan0(img.w as i32, img.h as i32, stride, PIXEL_FORMAT_32BPP_PARGB, Some(img.bgra.as_ptr()), &mut bitmap) == Status(0)
            && !bitmap.is_null()
        {
            GdipSetInterpolationMode(g, InterpolationModeHighQualityBicubic);
            GdipSetPixelOffsetMode(g, PixelOffsetModeHighQuality);
            GdipDrawImageRectI(g, bitmap as *mut GpImage, x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32);
            GdipDisposeImage(bitmap as *mut GpImage);
        }
        GdipDeleteGraphics(g);
    }
}
