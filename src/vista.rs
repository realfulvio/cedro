//! La scelta dell'interfaccia: due anteprime animate (Completa e Semplice) e il pulsante «Fine».
//! Si apre al termine dell'installazione su Windows e al primo avvio su Linux. Come la Guida, qui c'è
//! tutto quello che si vede e come si muove; le finestre prestano solo le primitive di `guida::Canvas`.
//! Le coordinate sono in pixel logici: la finestra è `W` x `H`.

use crate::guida::{Align, Canvas, Rgb};
use crate::settings::{read_settings, write_settings};

pub const W: f32 = 780.0;
pub const H: f32 = 480.0;

const BLUE: Rgb = (0x00, 0x5E, 0x8E);
const INK: Rgb = (0x19, 0x19, 0x19);
const GREY: Rgb = (0x45, 0x5A, 0x64);
const LINE: Rgb = (0xD3, 0xDD, 0xE6);
const TINT: Rgb = (0xE3, 0xF0, 0xF8);
const SIDE: Rgb = (0xF3, 0xF6, 0xF9);
const WHITE: Rgb = (0xFF, 0xFF, 0xFF);
const SOFT: Rgb = (0xD6, 0xEB, 0xF6);

/// Quanto dura un giro dell'animazione.
const LOOP_MS: u64 = 9000;

const CARD_Y: f32 = 92.0;
const CARD_H: f32 = 304.0;
const CARD_W: f32 = (W - 56.0 - 16.0) / 2.0;
const BTN: (f32, f32, f32, f32) = (W - 28.0 - 120.0, H - 56.0, 120.0, 38.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Complete,
    Simple,
}

/// Cosa c'è sotto un clic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Card(Choice),
    Done,
}

fn card_x(c: Choice) -> f32 {
    match c {
        Choice::Complete => 28.0,
        Choice::Simple => 28.0 + CARD_W + 16.0,
    }
}

pub fn hit(x: f32, y: f32) -> Option<Hit> {
    let inside = |rx: f32, ry: f32, rw: f32, rh: f32| x >= rx && x < rx + rw && y >= ry && y < ry + rh;
    if inside(BTN.0, BTN.1, BTN.2, BTN.3) {
        return Some(Hit::Done);
    }
    [Choice::Complete, Choice::Simple].into_iter().find(|c| inside(card_x(*c), CARD_Y, CARD_W, CARD_H)).map(Hit::Card)
}

/// Salva la scelta: accende o spegne la modalità Semplice e ricorda che la domanda è stata fatta.
pub fn save(choice: Choice) {
    let mut s = read_settings();
    s.simple = choice == Choice::Simple;
    s.view_asked = true;
    let _ = write_settings(&s);
}

/// Finestra chiusa senza premere «Fine»: la scelta resta com'è, ma non si chiede più.
pub fn mark_asked() {
    let mut s = read_settings();
    if !s.view_asked {
        s.view_asked = true;
        let _ = write_settings(&s);
    }
}

/// La scelta di partenza: quella già fatta, altrimenti la Semplice (consigliata).
pub fn initial() -> Choice {
    let s = read_settings();
    if s.view_asked && !s.simple {
        Choice::Complete
    } else {
        Choice::Simple
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

/// Il puntatore del mouse: da dove parte e dove si ferma durante il giro, in frazioni del riquadro.
fn pointer(t: f32) -> Option<(f32, f32)> {
    const PATH: [(f32, f32, f32); 8] = [
        (0.00, 0.82, 0.92),
        (0.09, 0.55, 0.28),
        (0.24, 0.55, 0.28),
        (0.34, 0.30, 0.55),
        (0.40, 0.30, 0.55),
        (0.58, 0.66, 0.90),
        (0.70, 0.66, 0.90),
        (0.92, 0.66, 0.90),
    ];
    if !(0.02..0.92).contains(&t) {
        return None;
    }
    for w in PATH.windows(2) {
        let ((t0, x0, y0), (t1, x1, y1)) = (w[0], w[1]);
        if t <= t1 {
            let k = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
            let k = k * k * (3.0 - 2.0 * k);
            return Some((lerp(x0, x1, k), lerp(y0, y1, k)));
        }
    }
    None
}

fn chip(c: &mut dyn Canvas, x: f32, y: f32, label: &str, on: bool) -> f32 {
    let w = c.text_width(label, 9.5, false) + 14.0;
    c.round(x, y, w, 18.0, 0.0, Some(if on { BLUE } else { WHITE }), Some(if on { BLUE } else { LINE }));
    c.text(label, x, y, w, 18.0, 9.5, false, if on { WHITE } else { INK }, Align::Center, false);
    w + 5.0
}

fn frame(c: &mut dyn Canvas, x: f32, y: f32, w: f32, h: f32) {
    c.round(x, y, w, h, 0.0, Some(WHITE), Some(LINE));
    c.fill(x, y, w, 18.0, BLUE);
    c.text("Conversioni di Cedro", x + 8.0, y, w - 16.0, 18.0, 10.0, true, WHITE, Align::Left, false);
}

fn pointer_draw(c: &mut dyn Canvas, x: f32, y: f32, w: f32, h: f32, t: f32) {
    if let Some((fx, fy)) = pointer(t) {
        let (px, py) = (x + fx * w, y + fy * h);
        c.poly(&[(px, py), (px, py + 14.0), (px + 4.0, py + 10.5), (px + 8.5, py + 10.5)], INK, WHITE);
    }
}

/// L'interfaccia completa: elenco a sinistra, tutto in vista.
fn mini_complete(c: &mut dyn Canvas, x: f32, y: f32, w: f32, h: f32, t: f32) {
    frame(c, x, y, w, h);
    c.clip(x, y, w, h);
    c.fill(x + 1.0, y + 18.0, 62.0, h - 19.0, SIDE);
    for (i, name) in ["Immagini", "Documenti", "PDF", "Testo", "Audio", "Video", "YouTube"].iter().enumerate() {
        let iy = y + 22.0 + i as f32 * 19.0;
        if i == 0 {
            c.fill(x + 1.0, iy - 2.0, 62.0, 19.0, BLUE);
        }
        c.text(name, x + 8.0, iy - 2.0, 54.0, 19.0, 9.0, i == 0, if i == 0 { WHITE } else { INK }, Align::Left, false);
    }
    let (cx, cw) = (x + 72.0, w - 80.0);
    c.round(cx, y + 26.0, cw, 24.0, 0.0, Some(TINT), Some(BLUE));
    c.text("Trascina qui i file", cx, y + 26.0, cw, 24.0, 10.0, true, BLUE, Align::Center, false);
    if t >= 0.10 {
        for (i, name) in ["vacanza.jpg", "tramonto.png"].iter().enumerate() {
            let fy = y + 56.0 + i as f32 * 18.0;
            c.round(cx, fy, cw, 16.0, 0.0, Some(WHITE), Some(LINE));
            c.text(name, cx + 6.0, fy, cw - 12.0, 16.0, 9.0, false, INK, Align::Left, false);
        }
    }
    if t >= 0.25 {
        c.text("CONVERTI IN", cx, y + 96.0, cw, 12.0, 8.0, true, GREY, Align::Left, false);
        let mut fx = cx;
        for name in ["PNG", "JPEG", "WebP", "BMP", "GIF", "PDF"] {
            fx += chip(c, fx, y + 110.0, name, name == "WebP" && t >= 0.40);
        }
    }
    if t >= 0.45 {
        c.text("STRUMENTI · OPZIONI · QUALITÀ", cx, y + 136.0, cw, 12.0, 8.0, true, GREY, Align::Left, false);
        let mut fx = cx;
        for name in ["Comprimi", "B/N", "OCR"] {
            fx += chip(c, fx, y + 150.0, name, false);
        }
        let mut fx = cx;
        for name in ["Dimensioni: Originali", "Qualità 82"] {
            fx += chip(c, fx, y + 172.0, name, false);
        }
    }
    if t >= 0.65 {
        c.fill(cx, y + h - 30.0, cw, 22.0, BLUE);
        c.text("Converti 2 file · WebP", cx, y + h - 30.0, cw, 22.0, 10.5, true, WHITE, Align::Center, false);
    }
    pointer_draw(c, x, y, w, h, t);
    c.unclip();
}

/// L'interfaccia Semplice: tre schede in alto, riquadri grandi, un solo pulsante.
fn mini_simple(c: &mut dyn Canvas, x: f32, y: f32, w: f32, h: f32, t: f32) {
    frame(c, x, y, w, h);
    c.clip(x, y, w, h);
    c.fill(x + 1.0, y + 18.0, w - 2.0, 22.0, SIDE);
    let tab = (w - 2.0) / 3.0;
    for (i, name) in ["Converti file", "Video dal web", "Trascrivi"].iter().enumerate() {
        let tx = x + 1.0 + i as f32 * tab;
        if i == 0 {
            c.fill(tx, y + 18.0, tab, 22.0, WHITE);
            c.fill(tx, y + 38.0, tab, 2.0, BLUE);
        }
        c.text(name, tx, y + 18.0, tab, 20.0, 9.5, true, if i == 0 { BLUE } else { INK }, Align::Center, false);
    }
    let (cx, cw) = (x + 16.0, w - 32.0);
    c.text("Cosa vuoi convertire?", cx, y + 46.0, cw, 20.0, 14.0, true, INK, Align::Left, false);
    c.round(cx, y + 70.0, cw, 30.0, 0.0, Some(TINT), Some(BLUE));
    c.text("Trascina qui i file", cx, y + 70.0, cw, 30.0, 11.0, true, BLUE, Align::Center, false);
    if t >= 0.10 {
        for (i, name) in ["vacanza.jpg", "tramonto.png"].iter().enumerate() {
            let fy = y + 104.0 + i as f32 * 17.0;
            c.round(cx, fy, cw, 15.0, 0.0, Some(WHITE), Some(LINE));
            c.text(name, cx + 6.0, fy, cw - 12.0, 15.0, 9.0, false, INK, Align::Left, false);
        }
    }
    if t >= 0.25 {
        let gap = 6.0;
        let kw = (cw - 2.0 * gap) / 3.0;
        for (i, (title, note)) in [("WebP", "leggera, per il web"), ("JPEG", "va ovunque"), ("PNG", "senza perdite")].iter().enumerate() {
            let on = i == 0 && t >= 0.40;
            let kx = cx + i as f32 * (kw + gap);
            c.round(kx, y + 142.0, kw, 42.0, 0.0, Some(if on { BLUE } else { WHITE }), Some(if on { BLUE } else { LINE }));
            c.text(title, kx + 7.0, y + 144.0, kw - 10.0, 20.0, 12.0, true, if on { WHITE } else { INK }, Align::Left, false);
            c.text(note, kx + 7.0, y + 164.0, kw - 10.0, 16.0, 8.0, false, if on { SOFT } else { GREY }, Align::Left, false);
        }
    }
    if t >= 0.65 {
        c.fill(cx, y + h - 34.0, cw, 26.0, BLUE);
        c.text("Converti 2 file · WebP", cx, y + h - 34.0, cw, 26.0, 12.0, true, WHITE, Align::Center, false);
    }
    pointer_draw(c, x, y, w, h, t);
    c.unclip();
}

fn card(c: &mut dyn Canvas, which: Choice, chosen: Choice, t: f32) {
    let on = which == chosen;
    let (x, y) = (card_x(which), CARD_Y);
    c.round(x, y, CARD_W, CARD_H, 0.0, Some(if on { TINT } else { WHITE }), Some(if on { BLUE } else { LINE }));
    if on {
        c.round(x + 1.0, y + 1.0, CARD_W - 2.0, CARD_H - 2.0, 0.0, None, Some(BLUE));
    }
    // Il tondino della scelta.
    c.round(x + 14.0, y + 14.0, 18.0, 18.0, 9.0, Some(WHITE), Some(BLUE));
    if on {
        c.round(x + 18.0, y + 18.0, 10.0, 10.0, 5.0, Some(BLUE), None);
    }
    let (title, note) = match which {
        Choice::Complete => ("Completa", "Tutte le opzioni subito in vista: formati, strumenti, qualità, dove salvare."),
        Choice::Simple => ("Semplice", "Solo l'essenziale: trascini i file, scegli in che cosa trasformarli, premi il pulsante."),
    };
    c.text(title, x + 40.0, y + 10.0, 160.0, 26.0, 19.0, true, INK, Align::Left, false);
    if which == Choice::Simple {
        let w = c.text_width("consigliata", 10.0, true) + 16.0;
        c.fill(x + CARD_W - w - 12.0, y + 14.0, w, 18.0, BLUE);
        c.text("consigliata", x + CARD_W - w - 12.0, y + 14.0, w, 18.0, 10.0, true, WHITE, Align::Center, false);
    }
    c.text(note, x + 14.0, y + 40.0, CARD_W - 28.0, 38.0, 12.5, false, GREY, Align::Left, true);
    let (mx, my, mw, mh) = (x + 12.0, y + 84.0, CARD_W - 24.0, CARD_H - 96.0);
    match which {
        Choice::Complete => mini_complete(c, mx, my, mw, mh, t),
        Choice::Simple => mini_simple(c, mx, my, mw, mh, t),
    }
}

pub fn draw(c: &mut dyn Canvas, chosen: Choice, now: u64) {
    let t = (now % LOOP_MS) as f32 / LOOP_MS as f32;
    c.fill(0.0, 0.0, W, H, WHITE);
    c.text("Come vuoi usare Cedro?", 28.0, 18.0, W - 56.0, 34.0, 25.0, true, BLUE, Align::Left, false);
    c.text("Si può cambiare quando vuoi da Impostazioni > Generale. Le funzioni sono le stesse.", 28.0, 54.0, W - 56.0, 24.0, 14.0, false, GREY, Align::Left, false);
    card(c, Choice::Complete, chosen, t);
    card(c, Choice::Simple, chosen, t);
    c.text("I componenti pesanti (trascrizione, video dal web) si scaricano solo al primo uso.", 28.0, H - 58.0, W - 28.0 - 120.0 - 40.0, 40.0, 12.0, false, GREY, Align::Left, true);
    c.round(BTN.0, BTN.1, BTN.2, BTN.3, 0.0, Some(BLUE), None);
    c.text("Fine", BTN.0, BTN.1, BTN.2, BTN.3, 15.0, true, WHITE, Align::Center, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_land_on_cards_and_button() {
        assert_eq!(hit(card_x(Choice::Complete) + 10.0, CARD_Y + 10.0), Some(Hit::Card(Choice::Complete)));
        assert_eq!(hit(card_x(Choice::Simple) + 10.0, CARD_Y + 10.0), Some(Hit::Card(Choice::Simple)));
        assert_eq!(hit(BTN.0 + 5.0, BTN.1 + 5.0), Some(Hit::Done));
        assert_eq!(hit(2.0, 2.0), None);
    }

    #[test]
    fn pointer_stays_inside_the_mock_window() {
        for i in 0..100 {
            if let Some((x, y)) = pointer(i as f32 / 100.0) {
                assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y));
            }
        }
    }
}
