//! Le sezioni della finestra «Impostazioni» (Windows e Linux): ognuna ha la sua icona e il suo
//! colore, come le attività e i tipi di file nei temi. Qui si disegna la piastrella colorata che
//! le due finestre mettono accanto al nome della sezione.

use super::assets as ic;
use super::gfx::{Fonts, GlyphCache, Painter, Rgb};
use super::theme::mix;

/// Icona e colore della sezione (il nome è quello italiano, come in `SECTIONS`).
pub fn look(name: &str) -> (&'static str, Rgb) {
    match name {
        "Generale" => (ic::SETTINGS, (0x25, 0x63, 0xB8)),
        "Aspetto" => (PALETTE, (0x7A, 0x3F, 0xC8)),
        "Ruota" => (WHEEL, (0xD4, 0x57, 0x0E)),
        "Salvataggio" => (ic::FOLDER, (0x1F, 0x8A, 0x62)),
        "Qualità" => (ic::IMAGE, (0x0D, 0x7C, 0x8F)),
        "OCR" => (ic::SEARCH, (0xD2, 0x38, 0x2C)),
        "Aggiornamenti" => (ic::DOWNLOAD, (0x4F, 0x46, 0xE5)),
        "Sistema" => (ic::CHIP, (0x55, 0x65, 0x7A)),
        _ => (ic::INFO, (0xA2, 0x76, 0x0A)),
    }
}

/// Una tavolozza: il contorno e tre pallini di colore.
const PALETTE: &str = "M12 3a9 9 0 1 0 0 18c1.4 0 2-1 2-2s-1-1.6-1-2.6 1-1.4 2-1.4h2a4 4 0 0 0 4-4c0-4.4-4-8-9-8z M7.5 11.5h.1 M10.5 7.5h.1 M15.5 7.5h.1";
/// La ruota: un cerchio con i raggi.
const WHEEL: &str = "M3.0 12.0a9.0 9.0 0 1 0 18.0 0a9.0 9.0 0 1 0 -18.0 0z M9.0 12.0a3.0 3.0 0 1 0 6.0 0a3.0 3.0 0 1 0 -6.0 0z M12 3v6M12 15v6M3 12h6M15 12h6";

/// La piastrella della sezione su un fondo `bg`: `size` pixel di lato (già moltiplicati per la scala).
/// Piena del colore (icona bianca) se `on`, appena tinta se no. Restituisce i pixel BGRA premoltiplicati.
pub fn tile_bgra(name: &str, on: bool, size: u32, scale: f32, bg: Rgb) -> Vec<u8> {
    let (icon, c) = look(name);
    let mut cache = GlyphCache::new();
    let mut p = Painter::new(size, size, scale, Fonts::current(), &mut cache);
    p.clear(bg);
    let s = size as f32 / scale;
    let pad = 2.0;
    let t = s - 2.0 * pad;
    let rad = (t * 0.26).round();
    let isz = (t * 0.62).round();
    let o = pad + (t - isz) / 2.0;
    if on {
        p.rrect(pad, pad, t, t, rad, Some(c), None);
        p.icon(icon, o, o, isz, (255, 255, 255));
    } else {
        p.rrect(pad, pad, t, t, rad, Some(mix(c, (255, 255, 255), 0.86)), None);
        p.icon(icon, o, o, isz, c);
    }
    p.bgra()
}
