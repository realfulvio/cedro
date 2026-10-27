//! La firma in fondo alle finestre: «Cedro 0.17.1 · Powered by Luca real>hcok_ A.», col logo disegnato
//! alla misura del testo. È una scritta statica a disegno proprio (`SS_OWNERDRAW`): la finestra madre
//! passa qui il suo `WM_DRAWITEM`. Con il tema Bondeno davanti c'è lo stemma del Comune.

use windows::Win32::Foundation::{COLORREF, LPARAM, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_GETFONT};
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;

/// Lo stile di una scritta statica a disegno proprio.
pub const SS_OWNERDRAW: u32 = 0xD;

/// Il giallo del cursore e il grigio azzurro di «real».
const LEMON: COLORREF = COLORREF(0x0030C2F2); // #F2C230
const REAL_GREY: COLORREF = COLORREF(0x00786B5A); // #5A6B78

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Disegna la firma nel controllo `di`. `version` aggiunge «Cedro x.y.z ·» davanti; `suffix` va in coda (grigio).
/// Con `ente`, con il tema Bondeno, davanti a tutto c'è lo stemma; senza `version` anche «Comune di Bondeno ·»
/// (con la versione la riga non ci starebbe accanto ai pulsanti).
pub unsafe fn draw(di: &DRAWITEMSTRUCT, bg: COLORREF, fg: COLORREF, primary: COLORREF, version: bool, suffix: &str, ente: bool) {
    let hdc = di.hDC;
    let rc: RECT = di.rcItem;
    let brush = CreateSolidBrush(bg);
    FillRect(hdc, &rc, brush);
    let _ = DeleteObject(brush.into());

    let font = HFONT(SendMessageW(di.hwndItem, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0 as *mut _);
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    let mut tm = TEXTMETRICW::default();
    let _ = GetTextMetricsW(hdc, &mut tm);
    let top = rc.top + ((rc.bottom - rc.top) - tm.tmHeight) / 2;
    let baseline = (top + tm.tmAscent) as f32;
    let em = (tm.tmHeight - tm.tmInternalLeading) as f32;
    // Il logo alto quanto le lettere del testo: le aste (40 unità) come le maiuscole, i tratti sottili come il carattere.
    let scale = em * 0.82 / cedro::logo::HEIGHT;
    let gap = (em * 0.3).round() as i32;

    let mut x = rc.left;
    let text = |s: &str, color: COLORREF, x: &mut i32| {
        let w = wide(s);
        SetTextColor(hdc, color);
        let _ = TextOutW(hdc, *x, top, &w);
        let mut size = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &w, &mut size);
        *x += size.cx;
    };
    if ente && cedro::edizione::bondeno() {
        let side = ((rc.bottom - rc.top) as f32).min(em * 1.7).round();
        let y = rc.top as f32 + ((rc.bottom - rc.top) as f32 - side) / 2.0;
        crate::wingfx::draw_stemma(hdc, cedro::edizione::Stemma::Solo, x as f32, y, side, side);
        x += side as i32 + gap;
    }
    if version {
        text(&format!("Cedro {}", cedro::update::CURRENT), primary, &mut x);
        text("  ·  ", fg, &mut x);
    }
    if ente && !version && cedro::edizione::bondeno() {
        text("Comune di Bondeno", primary, &mut x);
        text("  ·  ", fg, &mut x);
    }
    text("Powered by Luca", fg, &mut x);
    x += gap;
    let width = cedro::logo::width(true) * scale;
    crate::wingfx::draw_logo(hdc, x as f32, baseline, scale, true, primary, LEMON, REAL_GREY);
    x += width.ceil() as i32 + gap;
    text("A.", fg, &mut x);
    if !suffix.is_empty() {
        text(suffix, fg, &mut x);
    }
    SelectObject(hdc, old);
}
