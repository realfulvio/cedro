//! La schermata «Modifica» della scheda PDF, come nell'anteprima approvata: una riga con il nome del modo e le
//! schede, sotto la barra degli strumenti (icona sopra, nome sotto), a sinistra le pagine e i commenti, al
//! centro le pagine vere, a destra le proprietà e il salvataggio, in fondo la riga di stato. È scura in tutte
//! le tinte (il colore della zona del file) con l'accento della tinta. Qui ci sono la cornice, le barre e i
//! pezzi che usano anche `pdfedpage` (le pagine) e `pdfedprops` (il pannello di destra).

use std::sync::OnceLock;

use super::cedro;
use crate::pdfmod::Color;
use crate::ui::assets as ic;
use crate::ui::core::{Act, Field, Frame, HitKind, Scroll, BOLD, R, SANS, SEMI};
use crate::ui::gfx::{Align as TA, Fit};
use crate::ui::model::App;
use crate::ui::pdfedact::{EdAct, EdField};
use crate::ui::pdfeditor::{CommentRow, EdTab, PdfEd, Tool};
use crate::ui::theme::{mix, Palette, ThemeId};

pub(super) type Rgb = (u8, u8, u8);

pub(super) const WHITE: Rgb = (255, 255, 255);
/// Il blu del riquadro intorno al testo che si scrive e a quello scelto.
pub(super) const BLUE: Rgb = (0x4C, 0x9B, 0xF5);
/// L'ambra degli avvisi.
pub(super) const AMBER: Rgb = (0xF2, 0xC1, 0x4B);

/// Quanto sono alte le barre.
pub(super) const ROW: f32 = 46.0;
pub(super) const TOOLS: f32 = 80.0;
pub(super) const STATUS: f32 = 38.0;
pub(super) const LEFT: f32 = 188.0;

/// I colori dell'editor in una tinta.
#[derive(Clone, Copy)]
pub(super) struct E {
    /// Il fondo dei pannelli e delle barre.
    pub e1: Rgb,
    /// Il fondo della zona della pagina.
    pub e2: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    pub acc: Rgb,
    /// Le scritte sopra l'accento.
    pub on_acc: Rgb,
    pub ok: Rgb,
}

const STAGES: [Rgb; 4] = [(0x09, 0x07, 0x0C), (0x06, 0x09, 0x0D), (0x06, 0x0A, 0x08), (0x1A, 0x16, 0x22)];
const ACCENTS: [Rgb; 4] = [(0xB0, 0x8C, 0xF5), (0x7D, 0xB4, 0xF5), (0x4F, 0xD1, 0xA1), (0xB4, 0x93, 0xF0)];

fn tint_index(theme: ThemeId) -> usize {
    match theme {
        ThemeId::CedroNotte => 1,
        ThemeId::CedroBosco => 2,
        ThemeId::CedroCarta => 3,
        _ => 0,
    }
}

pub(super) fn colors(theme: ThemeId) -> E {
    let i = tint_index(theme);
    E { e1: mix(STAGES[i], WHITE, 0.10), e2: STAGES[i], ink: (0xEE, 0xE9, 0xF5), muted: (0xA9, 0x9F, 0xB8), acc: ACCENTS[i], on_acc: (0x1A, 0x12, 0x28), ok: (0x4B, 0xD9, 0x9E) }
}

/// La tavolozza con cui si disegnano i campi, i menu e i pulsanti dentro l'editor (così hanno l'aspetto scuro).
fn palette(theme: ThemeId) -> &'static Palette {
    static P: OnceLock<[Palette; 4]> = OnceLock::new();
    let all = P.get_or_init(|| {
        [0, 1, 2, 3].map(|i| {
            let e1 = mix(STAGES[i], WHITE, 0.10);
            Palette {
                bg: STAGES[i],
                surface: mix(e1, WHITE, 0.06),
                alt: mix(e1, WHITE, 0.12),
                text: (0xEE, 0xE9, 0xF5),
                muted: (0xA9, 0x9F, 0xB8),
                primary: ACCENTS[i],
                on_primary: (0x1A, 0x12, 0x28),
                border: mix(e1, WHITE, 0.15),
                ok: (0x4B, 0xD9, 0x9E),
                bad: (0xFF, 0x8A, 0x8F),
                work: e1,
                nav_sel: mix(e1, WHITE, 0.12),
            }
        })
    });
    &all[tint_index(theme)]
}

pub(super) fn rgb(c: Color) -> Rgb {
    (c[0], c[1], c[2])
}

/// Un velo bianco trasparente sopra il fondo (il passaggio del mouse, la voce scelta).
pub(super) fn veil(f: &mut Frame, r: R, rad: f32, a: f32) {
    f.p.rrect_a(r.0, r.1, r.2, r.3, rad, WHITE, a);
}

/// Tiene i clic registrati da `from` in poi dentro `view` (quel che scorre fuori dalla zona non deve rubare i clic
/// alle barre che gli stanno intorno).
pub(super) fn clip_hits(f: &mut Frame, from: usize, view: R) {
    let mut i = from;
    while i < f.hits.len() {
        let r = f.hits[i].r;
        let (x0, y0) = (r.0.max(view.0), r.1.max(view.1));
        let (x1, y1) = ((r.0 + r.2).min(view.0 + view.2), (r.1 + r.3).min(view.1 + view.3));
        if x1 <= x0 || y1 <= y0 {
            f.hits.remove(i);
        } else {
            f.hits[i].r = (x0, y0, x1 - x0, y1 - y0);
            i += 1;
        }
    }
}

/// Un comando senza riquadro: il velo sotto il mouse e quando è scelto; senza `enabled` non risponde.
pub(super) fn ghost(f: &mut Frame, r: R, rad: f32, act: Act, on: bool, enabled: bool) {
    let t = f.hover_t(&act, enabled);
    let press = f.press_t(&act, enabled);
    let rr = Frame::pressed_rect(r, press);
    if on {
        veil(f, rr, rad, 0.14);
    } else if t > 0.01 {
        veil(f, rr, rad, 0.09 * t);
    }
    if enabled {
        f.focus_ring(r, rad, &act);
        f.hit(r, act);
    }
}

/// Un pulsante pieno dell'accento (Salva, Applica, Segna).
pub(super) fn solid(f: &mut Frame, e: E, r: R, label: &str, size: f32, act: Act, enabled: bool) {
    let t = f.hover_t(&act, enabled);
    let press = f.press_t(&act, enabled);
    let rr = Frame::pressed_rect(r, press);
    let c = if enabled { mix(e.acc, WHITE, 0.08 * t) } else { mix(e.acc, e.e1, 0.65) };
    f.p.rrect(rr.0, rr.1, rr.2, rr.3, 8.0, Some(c), None);
    let tc = if enabled { e.on_acc } else { mix(e.ink, e.e1, 0.4) };
    f.p.text(label, rr.0 + 8.0, rr.1, rr.2 - 16.0, rr.3, size, BOLD, tc, TA::Center);
    if enabled {
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
}

/// Un pulsante con il bordo (Applica, Togli, Apri).
pub(super) fn outline(f: &mut Frame, e: E, r: R, label: &str, size: f32, act: Act, enabled: bool) {
    let t = f.hover_t(&act, enabled);
    let press = f.press_t(&act, enabled);
    let rr = Frame::pressed_rect(r, press);
    f.p.rrect(rr.0, rr.1, rr.2, rr.3, 8.0, None, Some((mix(e.e1, WHITE, 0.20 + 0.10 * t), 1.0)));
    if t > 0.01 {
        veil(f, rr, 8.0, 0.06 * t);
    }
    let tc = if enabled { e.ink } else { e.muted };
    f.p.text(label, rr.0 + 8.0, rr.1, rr.2 - 16.0, rr.3, size, SEMI, tc, TA::Center);
    if enabled {
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
}

/// Una riga di scelte vicine (Sinistra | Centro | Destra): quella scelta ha il bordo dell'accento.
pub(super) fn seg(f: &mut Frame, e: E, r: R, items: &[(String, bool, Act)]) {
    let n = items.len().max(1) as f32;
    let gap = 6.0;
    let w = (r.2 - gap * (n - 1.0)) / n;
    for (i, (label, on, act)) in items.iter().enumerate() {
        let cell = (r.0 + i as f32 * (w + gap), r.1, w, r.3);
        let t = f.hover_t(act, true);
        if *on {
            veil(f, cell, 7.0, 0.14);
            f.p.rrect(cell.0, cell.1, cell.2, cell.3, 7.0, None, Some((e.acc, 1.0)));
        } else {
            veil(f, cell, 7.0, 0.05 + 0.06 * t);
            f.p.rrect(cell.0, cell.1, cell.2, cell.3, 7.0, None, Some((mix(e.e1, WHITE, 0.16), 1.0)));
        }
        f.p.text(label, cell.0 + 4.0, cell.1, cell.2 - 8.0, cell.3, 13.0, if *on { BOLD } else { SEMI }, if *on { WHITE } else { e.ink }, TA::Center);
        f.focus_ring(cell, 7.0, act);
        f.hit(cell, act.clone());
    }
}

/// I pallini dei colori in fila: quello scelto ha l'anello dell'accento. Restituisce l'altezza usata.
pub(super) fn swatches(f: &mut Frame, e: E, x: f32, y: f32, w: f32, colors: &[Option<Color>], active: Option<Option<Color>>, make: impl Fn(Option<Color>) -> EdAct) -> f32 {
    let (d, gap) = (28.0, 12.0);
    let per_row = (((w + gap) / (d + gap)).floor() as usize).max(1);
    for (i, c) in colors.iter().enumerate() {
        let (cx, cy) = (x + (i % per_row) as f32 * (d + gap) + d / 2.0, y + (i / per_row) as f32 * (d + 10.0) + d / 2.0);
        let act = Act::Ed(make(*c));
        let on = active == Some(*c);
        let t = f.hover_t(&act, true);
        if on {
            f.p.circle(cx, cy, d / 2.0 + 4.0, None, Some((e.acc, 2.0)));
        } else if t > 0.01 {
            f.p.circle(cx, cy, d / 2.0 + 4.0, None, Some((mix(e.e1, WHITE, 0.3), 1.5)));
        }
        match c {
            Some(c) => f.p.circle(cx, cy, d / 2.0, Some(rgb(*c)), Some((mix(rgb(*c), (0, 0, 0), 0.35), 1.0))),
            None => {
                // «Nessuno»: un cerchio vuoto con la barra obliqua.
                f.p.circle(cx, cy, d / 2.0 - 1.0, None, Some((mix(e.e1, WHITE, 0.45), 1.5)));
                f.p.line(cx - 7.0, cy + 7.0, cx + 7.0, cy - 7.0, mix(e.e1, WHITE, 0.45), 1.5);
            }
        }
        f.focus_ring((cx - d / 2.0, cy - d / 2.0, d, d), d / 2.0, &act);
        f.hit((cx - d / 2.0 - 3.0, cy - d / 2.0 - 3.0, d + 6.0, d + 6.0), act);
    }
    let rows = colors.len().div_ceil(per_row).max(1);
    rows as f32 * (d + 10.0) - 10.0
}

/// Un cursore sottile (traccia e pallino dell'accento); il clic e il trascinamento li prende `ed_slide`.
pub(super) fn slim(f: &mut Frame, e: E, r: R, frac: f32, act: Act) {
    let cy = r.1 + r.3 / 2.0;
    let frac = frac.clamp(0.0, 1.0);
    f.p.rrect(r.0, cy - 2.5, r.2, 5.0, 2.5, Some(mix(e.e1, WHITE, 0.16)), None);
    f.p.rrect(r.0, cy - 2.5, (r.2 * frac).max(5.0), 5.0, 2.5, Some(mix(e.acc, e.e1, 0.35)), None);
    let kx = r.0 + r.2 * frac;
    let hot = f.hovered(&act) || f.ui.drag.as_ref().is_some_and(|d| d.0 == act);
    f.p.circle(kx, cy, if hot { 10.0 } else { 9.0 }, Some(e.acc), Some((mix(e.acc, (0, 0, 0), 0.3), 1.0)));
    f.hit_kind((r.0 - 10.0, r.1 - 4.0, r.2 + 20.0, r.3 + 8.0), act, HitKind::Slider);
}

/// Un campo in cui si scrive, con un'icona a sinistra (il «cerca» in alto, i campi del pannello).
pub(super) fn input(f: &mut Frame, e: E, r: R, field: EdField, text: &str, placeholder: &str, size: f32, icon: Option<&str>) {
    let editing = f.ui.edit == Some(Field::Ed(field));
    veil(f, r, 7.0, if editing { 0.10 } else { 0.07 });
    let line = if editing { (e.acc, 2.0) } else { (mix(e.e1, WHITE, 0.16), 1.0) };
    f.p.rrect(r.0, r.1, r.2, r.3, 7.0, None, Some(line));
    let mut pad = 12.0;
    if let Some(d) = icon {
        f.icon(d, r.0 + 10.0, r.1 + (r.3 - 16.0) / 2.0, 16.0, e.muted);
        pad = 34.0;
    }
    let room = r.2 - pad - 10.0;
    f.p.push_clip(r.0 + 2.0, r.1, r.2 - 4.0, r.3);
    let shown = text.to_string();
    let caret_text: String = shown.chars().take(f.ui.caret.min(shown.chars().count())).collect();
    let caret_x = f.measure(&crate::i18n::lit(&caret_text), size, SANS);
    let shift = if editing && caret_x > room { caret_x - room } else { 0.0 };
    if shown.is_empty() && !editing {
        f.p.text(placeholder, r.0 + pad, r.1, room, r.3, size, SANS, e.muted, TA::Left);
    } else {
        if editing && f.ui.sel_all && !shown.is_empty() {
            let tw = f.measure(&crate::i18n::lit(&shown), size, SANS).min(room);
            f.p.rrect_a(r.0 + pad - 2.0, r.1 + r.3 * 0.2, tw + 4.0, r.3 * 0.6, 3.0, e.acc, 0.35);
        }
        let (asc, desc) = f.p.fonts().extents(crate::ui::gfx::text_size(size) * f.p.scale, SANS);
        let base = r.1 + (r.3 - (asc + desc) / f.p.scale) / 2.0 + asc / f.p.scale;
        f.p.text_at(&shown, r.0 + pad - shift, base, size, SANS, e.ink);
        if editing && (f.ms / 530).is_multiple_of(2) {
            f.p.rrect_a(r.0 + pad - shift + caret_x, r.1 + r.3 * 0.22, 1.5, r.3 * 0.56, 0.0, e.ink, 1.0);
        }
    }
    f.p.pop_clip();
    if editing {
        f.animating = true;
    }
    f.hit_kind(r, Act::Edit(Field::Ed(field)), HitKind::Text);
}

/// Le icone a linea (24×24) delle schede e degli strumenti.
pub(super) mod icons {
    pub const BACK: &str = "M6 3h8l4 4v14H6zM14 3v4h4";
    pub const UNDO: &str = "M9 14L4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11";
    pub const REDO: &str = "M15 14l5-5-5-5M20 9H9.5a5.5 5.5 0 0 0 0 11H13";
    pub const EDIT_TEXT: &str = "M5 4h7M8.5 4v14M5 18h7M16 21l4-4-2-2-4 4v2z";
    pub const ADD_TEXT: &str = "M4 4h16v16H4zM9 9h6M12 9v7";
    pub const FIND: &str = "M4 10a6 6 0 1 0 12 0a6 6 0 1 0-12 0zM15 15l6 6";
    pub const HIGHLIGHT: &str = "M9 11l-6 6v3h9l3-3M22 12l-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4";
    pub const UNDERLINE: &str = "M6 4v6a6 6 0 0 0 12 0V4M4 20h16";
    pub const STRIKE: &str = "M16 4H9a3 3 0 0 0-2.83 4M14 12a4 4 0 0 1 0 8H6M4 12h16";
    pub const NOTE: &str = "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z";
    pub const PENCIL: &str = "M17 3l4 4L7 21H3v-4zM15 5l4 4";
    pub const STAMP: &str = "M5 22h14M5 18h14v-2.5a2.5 2.5 0 0 0-2.5-2.5h-9A2.5 2.5 0 0 0 5 15.5zM14 13V8.5C14 7 15 7 15 5a3 3 0 0 0-6 0c0 2 1 2 1 3.5V13";
    pub const RECT: &str = "M4 5h16v14H4z";
    pub const ELLIPSE: &str = "M3 12a9 7 0 1 0 18 0a9 7 0 1 0-18 0z";
    pub const LINE: &str = "M5 19L19 5";
    pub const ARROW: &str = "M5 19L19 5M9 5h10v10";
    pub const FILL: &str = "M3 5h13v6H3zM11 14l8 3-3.5 1.5L14 22z";
    pub const NEW_TEXT: &str = "M5 4h1a3 3 0 0 1 3 3 3 3 0 0 1 3-3h1M13 20h-1a3 3 0 0 1-3-3 3 3 0 0 1-3 3H5M5 16H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h1M13 8h7a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-7M9 7v10";
    pub const CHECKBOX: &str = "M4 4h16v16H4zM8 12l3 3 5-6";
    pub const RADIO: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0zM9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0z";
    pub const LIST: &str = "M4 5h16v14H4zM4 11h16M14 7.5l2 2 2-2";
    pub const SIGN: &str = "M3 20c3 0 4-8 6-8s0 8 3 8 3-6 5-6 1 4 4 4M3 22h18";
    pub const REDACT: &str = "M4 6h16M4 12h4M4 18h16M11 9h9v6h-9z";
    pub const SEARCH_REDACT: &str = "M4 10a6 6 0 1 0 12 0a6 6 0 1 0-12 0zM15 15l6 6M7 10h6";
    pub const HIDDEN: &str = "M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68M6.61 6.61A13.53 13.53 0 0 0 2 11s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61M2 2l20 20M14.12 14.12a3 3 0 1 1-4.24-4.24";
    pub const PAGES: &str = "M6 3h8l4 4v14H6zM14 3v4h4M9 13h6M9 17h4";
    pub const PREV: &str = "M15 6l-6 6 6 6";
    pub const NEXT: &str = "M9 6l6 6-6 6";
    pub const PLUS: &str = "M12 5v14M5 12h14";
    pub const MINUS: &str = "M5 12h14";
    pub const FIT: &str = "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5";
    pub const ONE: &str = "M7 3h10v18H7z";
    pub const TWO: &str = "M3 4h8v16H3zM13 4h8v16h-8z";
}

pub(super) fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => icons::FILL,
        Tool::EditText => icons::EDIT_TEXT,
        Tool::AddText => icons::ADD_TEXT,
        Tool::Find => icons::FIND,
        Tool::Highlight => icons::HIGHLIGHT,
        Tool::Underline => icons::UNDERLINE,
        Tool::Strike => icons::STRIKE,
        Tool::Note => icons::NOTE,
        Tool::Pen => icons::PENCIL,
        Tool::Stamp => icons::STAMP,
        Tool::Image => ic::IMAGE,
        Tool::Rect => icons::RECT,
        Tool::Ellipse => icons::ELLIPSE,
        Tool::Line => icons::LINE,
        Tool::Arrow => icons::ARROW,
        Tool::Fill => icons::FILL,
        Tool::NewText => icons::NEW_TEXT,
        Tool::NewCheck => icons::CHECKBOX,
        Tool::NewRadio => icons::RADIO,
        Tool::NewList => icons::LIST,
        Tool::NewSign => icons::SIGN,
        Tool::Area => icons::REDACT,
        Tool::SearchRedact => icons::SEARCH_REDACT,
        Tool::Hidden => icons::HIDDEN,
    }
}

/// L'icona di un commento secondo il suo nome nell'elenco («Evidenzia», «Nota»…).
pub(super) fn comment_icon(kind: &str) -> &'static str {
    match kind {
        "Evidenzia" | "Evidenziatore" | "Highlight" => icons::HIGHLIGHT,
        "Sottolinea" | "Underline" => icons::UNDERLINE,
        "Barra" | "Barrato" | "StrikeOut" => icons::STRIKE,
        "Nota" | "Text" => icons::NOTE,
        "Matita" | "Ink" => icons::PENCIL,
        "Timbro" | "Stamp" => icons::STAMP,
        "Rettangolo" | "Square" => icons::RECT,
        "Ellisse" | "Circle" => icons::ELLIPSE,
        "Linea" | "Line" => icons::LINE,
        "Freccia" => icons::ARROW,
        _ => icons::NOTE,
    }
}

/// Cosa si vede quando l'editor è aperto: tutta la finestra (nelle tinte di Cedro dentro la cornice, accanto
/// alla barra, nelle altre disposizioni a tutto schermo).
pub fn draw(f: &mut Frame, app: &App) {
    let Some(ed) = app.ed.as_deref() else { return };
    let e = colors(f.theme);
    let area = if f.theme.is_cedro() {
        cedro::frame(f, app);
        (cedro::RAIL, cedro::TOP, f.w - cedro::RAIL - cedro::PAD, f.h - cedro::TOP - cedro::PAD)
    } else {
        plain_frame(f, e);
        (10.0, 52.0, f.w - 20.0, f.h - 62.0)
    };
    let keep = f.pal;
    f.pal = palette(f.theme);
    editor(f, app, ed, e, area);
    f.pal = keep;
}

/// Le altre disposizioni: il fondo scuro, il marchio e i pulsanti della finestra.
fn plain_frame(f: &mut Frame, e: E) {
    f.p.clear(mix(e.e2, WHITE, 0.03));
    f.caption((0.0, 0.0, f.w, 48.0));
    f.p.symbol(18.0, 10.0, 28.0);
    f.p.text("Cedro", 54.0, 8.0, 140.0, 32.0, 21.0, BOLD, e.ink, TA::Left);
    f.window_controls(f.w, 0.0, 48.0, e.ink, e.e1);
}

fn editor(f: &mut Frame, app: &App, ed: &PdfEd, e: E, area: R) {
    let (x0, y0, w, h) = area;
    f.p.rrect(x0, y0, w, h, 14.0, Some(e.e1), Some((mix(e.e1, WHITE, 0.09), 1.0)));
    f.p.push_clip(x0 + 1.0, y0 + 1.0, w - 2.0, h - 2.0);
    let rows = ed.comments();
    top_bar(f, ed, e, (x0, y0, w, ROW), rows.len());
    toolbar(f, ed, e, (x0, y0 + ROW, w, TOOLS));
    let body_y = y0 + ROW + TOOLS;
    let body_h = h - ROW - TOOLS - STATUS;
    let rw = (w * 0.283).clamp(322.0, 400.0);
    left_panel(f, ed, e, (x0, body_y, LEFT, body_h), &rows);
    super::pdfedpage::canvas(f, app, ed, e, (x0 + LEFT, body_y, w - LEFT - rw, body_h));
    super::pdfedprops::panel(f, app, ed, e, (x0 + w - rw, body_y, rw, body_h));
    status_bar(f, ed, e, (x0, y0 + h - STATUS, w, STATUS));
    f.p.pop_clip();
    if ed.leave.is_some() {
        leave_dialog(f, ed, e);
    }
}

// ---------------------------------------------------------------------------------------------
// La riga in alto

fn top_bar(f: &mut Frame, ed: &PdfEd, e: E, r: R, comments: usize) {
    f.p.fill(r.0, r.1 + r.3 - 1.0, r.2, 1.0, mix(e.e1, WHITE, 0.09));
    // «PDF / Modifica ⌄»: torna alla scheda PDF o passa a un altro modo.
    let mut x = r.0 + 8.0;
    let act = Act::Ed(EdAct::LeaveMenu);
    let open = f.ui.menu.as_ref().is_some_and(|m| m.items.first().is_some_and(|i| i.1 == Act::Ed(EdAct::Leave(None))));
    let (pdf_w, edit_w) = (f.measure("PDF", 14.0, SANS), f.measure("Modifica", 14.0, BOLD));
    let bw = 9.0 + 17.0 + 7.0 + pdf_w + 14.0 + edit_w + 7.0 + 15.0 + 9.0;
    let br = (x, r.1 + 7.0, bw, 32.0);
    ghost(f, br, 7.0, act, open, true);
    let cy = r.1 + r.3 / 2.0;
    f.icon(icons::BACK, x + 9.0, cy - 8.5, 17.0, (0xE9, 0xE3, 0xF2));
    let mut tx = x + 9.0 + 17.0 + 7.0;
    f.p.text("PDF", tx, br.1, pdf_w + 2.0, br.3, 14.0, SANS, (0xE9, 0xE3, 0xF2), TA::Left);
    tx += pdf_w + 5.0;
    f.p.text("/", tx, br.1, 8.0, br.3, 14.0, SANS, mix(e.e1, WHITE, 0.4), TA::Left);
    tx += 9.0;
    f.p.text("Modifica", tx, br.1, edit_w + 2.0, br.3, 14.0, BOLD, WHITE, TA::Left);
    tx += edit_w + 7.0;
    f.icon(ic::CHEVRON_DOWN, tx, cy - 7.5, 15.0, (0xE9, 0xE3, 0xF2));
    x += bw + 6.0;
    f.p.fill(x, r.1 + 11.0, 1.0, r.3 - 22.0, mix(e.e1, WHITE, 0.09));
    x += 9.0;
    // Le schede.
    let pending = ed.pending_redacts();
    for t in EdTab::ALL {
        let on = ed.tab == t;
        let count = match t {
            EdTab::Comments => comments,
            EdTab::Redact => pending,
            EdTab::Forms => ed.filled_fields(),
            _ => 0,
        };
        let lw = f.measure(t.label(), 14.5, BOLD);
        let badge_w = if count > 0 { f.measure(&count.to_string(), 11.5, BOLD).max(7.0) + 12.0 } else { 0.0 };
        let tw = 30.0 + lw + if count > 0 { 7.0 + badge_w } else { 0.0 };
        let tr = (x, r.1, tw, r.3);
        let act = Act::Ed(EdAct::Tab(t));
        let h = f.hover_t(&act, true);
        let c = if on { WHITE } else { mix((0xB2, 0xA7, 0xC2), WHITE, h) };
        f.p.text(t.label(), x + 15.0, r.1, lw + 2.0, r.3, 14.5, if on { BOLD } else { SEMI }, c, TA::Left);
        if count > 0 {
            let bx = x + 15.0 + lw + 7.0;
            let (bg, fg) = if on { (e.acc, e.on_acc) } else { (mix(e.e1, WHITE, 0.16), (0xE8, 0xE0, 0xF2)) };
            f.p.rrect(bx, cy - 9.0, badge_w, 18.0, 9.0, Some(bg), None);
            f.p.text(&count.to_string(), bx, cy - 9.0, badge_w, 18.0, 11.5, BOLD, fg, TA::Center);
        }
        if on {
            f.p.rrect(x + 8.0, r.1 + r.3 - 2.5, tw - 16.0, 2.5, 1.25, Some(e.acc), None);
        }
        f.focus_ring(tr, 4.0, &act);
        f.hit(tr, act);
        x += tw;
    }
    // A destra: annulla, ripeti, cerca.
    let mut rx = r.0 + r.2 - 10.0;
    let find_w = 214.0f32.min((rx - x - 90.0).max(120.0));
    rx -= find_w;
    let fr = (rx, r.1 + 7.0, find_w, 32.0);
    let hits = if ed.find_done && !ed.found.is_empty() { ed.found.len().to_string() } else { String::new() };
    let tfr = if hits.is_empty() { fr } else { (fr.0, fr.1, fr.2 - 30.0, fr.3) };
    input(f, e, fr, EdField::TopFind, &ed.find, "Cerca nel PDF", 14.0, Some(ic::SEARCH));
    if !hits.is_empty() {
        let _ = tfr;
        f.p.text(&hits, fr.0 + fr.2 - 34.0, fr.1, 28.0, fr.3, 12.5, BOLD, e.acc, TA::Right);
    }
    rx -= 8.0;
    for (icon, act, enabled) in [(icons::REDO, EdAct::Redo, !ed.redo.is_empty()), (icons::UNDO, EdAct::Undo, !ed.undo.is_empty())] {
        rx -= 34.0;
        let br = (rx, r.1 + 6.0, 34.0, 34.0);
        let act = Act::Ed(act);
        ghost(f, br, 7.0, act, false, enabled);
        f.icon(icon, rx + 8.5, br.1 + 8.5, 17.0, if enabled { (0xE9, 0xE3, 0xF2) } else { mix(e.e1, WHITE, 0.25) });
        rx -= 2.0;
    }
}

// ---------------------------------------------------------------------------------------------
// Gli strumenti

fn toolbar(f: &mut Frame, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, mix(e.e1, WHITE, 0.035));
    f.p.fill(r.0, r.1 + r.3 - 1.0, r.2, 1.0, mix(e.e1, WHITE, 0.09));
    let mut x = r.0 + 12.0;
    for (gi, group) in ed.tab.tools().iter().enumerate() {
        if gi > 0 {
            x += 4.0;
            f.p.fill(x, r.1 + 12.0, 1.0, r.3 - 24.0, mix(e.e1, WHITE, 0.09));
            x += 7.0;
        }
        for &t in group.iter() {
            // Le righe escono già tradotte: `lit` evita che passino da `tr` una seconda volta.
            let lines: Vec<String> = f.p.wrap_lines(t.label(), 12.5, SEMI, 80.0, 2).iter().map(|l| crate::i18n::lit(l)).collect();
            let lw = lines.iter().map(|l| f.measure(l, 12.5, SEMI)).fold(0.0f32, f32::max);
            let bw = (lw + 20.0).clamp(70.0, 98.0);
            let br = (x, r.1 + 7.0, bw, r.3 - 14.0);
            let on = ed.tool == t;
            let act = Act::Ed(EdAct::Tool(t));
            ghost(f, br, 8.0, act, on, true);
            if on {
                f.p.rrect(br.0 + 6.0, br.1 + br.3 - 2.5, br.2 - 12.0, 2.5, 1.25, Some(e.acc), None);
            }
            let block = 23.0 + 5.0 + lines.len() as f32 * 14.5;
            let top = br.1 + (br.3 - block) / 2.0 - 1.0;
            f.icon(tool_icon(t), br.0 + (bw - 23.0) / 2.0, top, 23.0, if on { e.acc } else { (0xE7, 0xE0, 0xF1) });
            for (k, l) in lines.iter().enumerate() {
                f.p.text(l, br.0 + 4.0, top + 28.0 + k as f32 * 14.5, bw - 8.0, 14.5, 12.5, SEMI, if on { WHITE } else { (0xDD, 0xD5, 0xE9) }, TA::Center);
            }
            x += bw + 2.0;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pagine e commenti a sinistra

fn left_panel(f: &mut Frame, ed: &PdfEd, e: E, r: R, rows: &[CommentRow]) {
    f.p.fill(r.0 + r.2 - 1.0, r.1, 1.0, r.3, mix(e.e1, WHITE, 0.09));
    // Le due schede.
    let half = (r.2 - 1.0) / 2.0;
    for (i, (label, icon, count)) in [("Pagine", icons::PAGES, 0usize), ("Commenti", icons::NOTE, rows.len())].into_iter().enumerate() {
        let on = ed.left as usize == i;
        let tr = (r.0 + i as f32 * half, r.1, half, 38.0);
        let act = Act::Ed(EdAct::Left(i as u8));
        let hov = f.hover_t(&act, true);
        let c = if on { WHITE } else { mix((0xB2, 0xA7, 0xC2), WHITE, hov) };
        let lw = f.measure(label, 13.5, BOLD);
        let badge_w = if count > 0 { f.measure(&count.to_string(), 11.0, BOLD).max(6.0) + 10.0 } else { 0.0 };
        let total = lw + if count > 0 { 6.0 + badge_w } else { 0.0 };
        let sx = tr.0 + (tr.2 - total) / 2.0;
        f.p.text(label, sx, tr.1, lw + 2.0, tr.3, 13.5, if on { BOLD } else { SEMI }, c, TA::Left);
        let _ = icon;
        if count > 0 {
            f.p.rrect(sx + lw + 6.0, tr.1 + 10.0, badge_w, 17.0, 8.5, Some(mix(e.e1, WHITE, 0.16)), None);
            f.p.text(&count.to_string(), sx + lw + 6.0, tr.1 + 10.0, badge_w, 17.0, 11.0, BOLD, c, TA::Center);
        }
        if on {
            f.p.rrect(tr.0 + 8.0, tr.1 + tr.3 - 2.5, tr.2 - 16.0, 2.5, 1.25, Some(e.acc), None);
        }
        f.hit(tr, act);
    }
    f.p.fill(r.0, r.1 + 37.0, r.2 - 1.0, 1.0, mix(e.e1, WHITE, 0.09));
    let body = (r.0, r.1 + 38.0, r.2 - 1.0, r.3 - 38.0);
    if ed.left == 0 {
        thumbs(f, ed, e, body);
    } else {
        comment_list(f, ed, e, body, rows);
    }
}

fn thumbs(f: &mut Frame, ed: &PdfEd, e: E, r: R) {
    let n = ed.page_count();
    ed.lay.borrow_mut().thumb_view = r;
    if n == 0 {
        return;
    }
    let off = f.ui.scroll(Scroll::EdThumbs).clamp(0.0, (ed.lay.borrow().heights[1] - r.3).max(0.0));
    let from = f.hits.len();
    let counts = ed.per_page();
    let cur = ed.lay.borrow().cur;
    let tw = 112.0;
    let mut y = r.1 + 14.0 - off;
    f.p.push_clip(r.0, r.1, r.2, r.3);
    let mut wanted = Vec::new();
    for p in 0..n {
        let (pw, ph) = ed.size_of(p);
        let th = (tw * ph / pw.max(1.0)).round().clamp(40.0, 260.0);
        let cell_h = 5.0 + th + 6.0 + 16.0 + 5.0;
        let cell = (r.0 + (r.2 - tw - 10.0) / 2.0, y, tw + 10.0, cell_h);
        if cell.1 + cell.3 > r.1 && cell.1 < r.1 + r.3 {
            let act = Act::Ed(EdAct::Thumb(p));
            let on = cur == p;
            ghost(f, cell, 6.0, act, on, true);
            let img = (cell.0 + 5.0, cell.1 + 5.0, tw, th);
            f.p.shadow(img.0, img.1, img.2, img.3, 2.0, 8.0, 0.55, 5.0);
            f.p.rrect(img.0, img.1, img.2, img.3, 2.0, Some(WHITE), None);
            match ed.thumbs.get(&p) {
                Some((_, im)) => f.p.image(im, img.0, img.1, img.2, img.3, 2.0, Fit::Contain),
                None => wanted.push(p),
            }
            if ed.thumbs.get(&p).is_some_and(|t| t.0 != ed.rev) {
                wanted.push(p);
            }
            if on {
                f.p.rrect(img.0 - 1.5, img.1 - 1.5, img.2 + 3.0, img.3 + 3.0, 3.0, None, Some((e.acc, 2.5)));
            } else {
                f.p.rrect(img.0, img.1, img.2, img.3, 2.0, None, Some((mix(e.e1, WHITE, 0.16), 1.0)));
            }
            f.p.text(&(p + 1).to_string(), cell.0, img.1 + th + 6.0, cell.2, 16.0, 12.5, BOLD, if on { WHITE } else { (0xB9, 0xAE, 0xCA) }, TA::Center);
            if let Some(&c) = counts.get(&p) {
                let bw = f.measure(&c.to_string(), 10.5, BOLD).max(6.0) + 11.0;
                f.p.rrect(img.0 + img.2 - bw - 3.0, img.1 + 3.0, bw, 17.0, 8.5, Some(e.acc), Some(((0, 0, 0), 1.0)));
                f.p.text(&c.to_string(), img.0 + img.2 - bw - 3.0, img.1 + 3.0, bw, 17.0, 10.5, BOLD, e.on_acc, TA::Center);
            }
        }
        y += cell_h + 16.0;
    }
    f.p.pop_clip();
    clip_hits(f, from, r);
    {
        let mut lay = ed.lay.borrow_mut();
        lay.thumbs = wanted;
        lay.heights[1] = y + off - r.1 + 4.0;
    }
    f.scrollbar(r, Scroll::EdThumbs, y + off - r.1 + 4.0);
}

fn comment_list(f: &mut Frame, ed: &PdfEd, e: E, r: R, rows: &[CommentRow]) {
    let cur = ed.lay.borrow().cur;
    let (x, w) = (r.0 + 10.0, r.2 - 20.0);
    // Tutte | Pagina N.
    let items = vec![("Tutte".to_string(), !ed.only_page, Act::Ed(EdAct::OnlyPage(false))), (format!("Pagina {}", cur + 1), ed.only_page, Act::Ed(EdAct::OnlyPage(true)))];
    seg(f, e, (x, r.1 + 12.0, w, 30.0), &items);
    let list = (r.0, r.1 + 52.0, r.2, r.3 - 52.0);
    let shown: Vec<(usize, &CommentRow)> = rows.iter().enumerate().filter(|(_, row)| !ed.only_page || row.page == cur).collect();
    if shown.is_empty() {
        let msg = if ed.only_page { "Nessun commento su questa pagina." } else { "Nessun commento. Evidenzia, scrivi una nota o metti un timbro dalla scheda «Commenti»." };
        f.para(msg, x + 4.0, list.1 + 10.0, w - 8.0, 12.5, SANS, e.muted, 1.4, 6);
        return;
    }
    let off = f.ui.scroll(Scroll::EdComments).clamp(0.0, (ed.lay.borrow().heights[2] - list.3).max(0.0));
    let from = f.hits.len();
    f.p.push_clip(list.0, list.1, list.2, list.3);
    let mut y = list.1 - off;
    for (i, row) in &shown {
        let rh = 48.0;
        let cell = (x, y, w, rh);
        if y + rh > list.1 && y < list.1 + list.3 {
            let act = Act::Ed(EdAct::CommentRow(*i));
            let on = row.index.is_some() && row.index == ed.sel || row.from_file.is_some() && row.from_file == ed.sel_file;
            let del = Act::Ed(EdAct::CommentDelete(*i));
            let hot = f.hovered(&act) || f.hovered(&del);
            ghost(f, cell, 7.0, act, on, true);
            if on {
                f.p.rrect(cell.0, cell.1 + 6.0, 2.5, cell.3 - 12.0, 1.25, Some(e.acc), None);
            }
            let ib = (x + 8.0, y + 10.0, 28.0, 28.0);
            f.p.rrect(ib.0, ib.1, ib.2, ib.3, 7.0, Some(mix(e.e1, WHITE, 0.10)), None);
            f.icon(comment_icon(&row.kind), ib.0 + 6.0, ib.1 + 6.0, 16.0, (0xE7, 0xE0, 0xF1));
            if let Some(c) = row.color {
                f.p.circle(ib.0 + ib.2 - 1.0, ib.1 + ib.3 - 1.0, 5.5, Some(rgb(c)), Some((e.e1, 2.0)));
            }
            let tw = w - 8.0 - 28.0 - 10.0 - if hot { 26.0 } else { 6.0 };
            // Il tipo a sinistra (non si taglia), la pagina a destra: sotto il cestino si nasconde.
            let pw = 46.0;
            f.p.text(&row.kind, x + 46.0, y + 6.0, tw - pw, 18.0, 13.0, BOLD, WHITE, TA::Left);
            if !hot {
                f.p.text(&format!("pag. {}", row.page + 1), x + w - 6.0 - pw, y + 7.0, pw, 18.0, 11.5, SEMI, e.muted, TA::Right);
            }
            let sub = if row.text.trim().is_empty() { String::new() } else { crate::i18n::lit(row.text.replace('\n', " ").trim()) };
            f.p.text(&sub, x + 46.0, y + 24.0, tw, 17.0, 12.0, SANS, e.muted, TA::Left);
            if hot {
                let tr = (x + w - 30.0, y + 12.0, 24.0, 24.0);
                let t = f.hover_t(&del, true);
                if t > 0.01 {
                    veil(f, tr, 6.0, 0.12 * t);
                }
                f.icon(ic::TRASH, tr.0 + 4.0, tr.1 + 4.0, 16.0, mix(e.muted, WHITE, t));
                f.hit(tr, del);
            }
        }
        y += rh + 4.0;
    }
    f.p.pop_clip();
    clip_hits(f, from, list);
    ed.lay.borrow_mut().heights[2] = y + off - list.1 + 6.0;
    f.scrollbar(list, Scroll::EdComments, y + off - list.1 + 6.0);
}

// ---------------------------------------------------------------------------------------------
// La riga di stato

fn status_bar(f: &mut Frame, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, 1.0, mix(e.e1, WHITE, 0.09));
    let cy = r.1 + r.3 / 2.0;
    let name = crate::i18n::lit(&ed.name);
    let nw = f.measure(&name, 13.0, BOLD).min(r.2 * 0.4);
    f.p.text(&name, r.0 + 14.0, r.1, nw + 2.0, r.3, 13.0, BOLD, WHITE, TA::Left);
    let (pages, changes) = ed.status_parts();
    let mut sx = r.0 + 14.0 + nw;
    for part in [" · ", pages.as_str(), " · ", changes.as_str()] {
        let pw = f.measure(part, 13.0, SEMI);
        f.p.text(part, sx, r.1, pw + 2.0, r.3, 13.0, SEMI, (0xB2, 0xA7, 0xC2), TA::Left);
        sx += pw;
    }
    // A destra, da destra a sinistra.
    let lay = ed.lay.borrow();
    let (cur, k) = (lay.cur, lay.k);
    drop(lay);
    let n = ed.page_count();
    let mut x = r.0 + r.2 - 10.0;
    // Una pagina | due pagine.
    let vw = 66.0;
    x -= vw;
    f.p.rrect(x, cy - 13.0, vw, 26.0, 7.0, Some(mix(e.e1, WHITE, 0.07)), None);
    for (i, (icon, two)) in [(icons::ONE, false), (icons::TWO, true)].into_iter().enumerate() {
        let br = (x + 2.0 + i as f32 * 31.0, cy - 11.0, 31.0, 22.0);
        let act = Act::Ed(EdAct::TwoPages(two));
        let on = ed.two == two;
        let t = f.hover_t(&act, true);
        if on {
            veil(f, br, 5.0, 0.18);
        } else if t > 0.01 {
            veil(f, br, 5.0, 0.08 * t);
        }
        f.icon(icon, br.0 + 7.5, br.1 + 3.0, 16.0, if on { WHITE } else { (0xD5, 0xCC, 0xE2) });
        f.hit(br, act);
    }
    x -= 10.0;
    f.p.fill(x, cy - 10.0, 1.0, 20.0, mix(e.e1, WHITE, 0.12));
    x -= 6.0;
    // Adatta, più, percentuale, meno.
    let pct = format!("{}%", (k * 75.0).round().max(1.0) as u32);
    for (icon, act) in [(icons::FIT, EdAct::ZoomFit), (icons::PLUS, EdAct::ZoomIn)] {
        x -= 28.0;
        let br = (x, cy - 14.0, 28.0, 28.0);
        let act = Act::Ed(act);
        ghost(f, br, 6.0, act, false, true);
        f.icon(icon, x + 5.5, cy - 8.5, 17.0, (0xD5, 0xCC, 0xE2));
        x -= 2.0;
    }
    x -= 48.0;
    f.p.text(&pct, x, r.1, 48.0, r.3, 13.0, BOLD, WHITE, TA::Center);
    x -= 28.0;
    let br = (x, cy - 14.0, 28.0, 28.0);
    ghost(f, br, 6.0, Act::Ed(EdAct::ZoomOut), false, true);
    f.icon(icons::MINUS, x + 5.5, cy - 8.5, 17.0, (0xD5, 0xCC, 0xE2));
    x -= 10.0;
    f.p.fill(x, cy - 10.0, 1.0, 20.0, mix(e.e1, WHITE, 0.12));
    x -= 6.0;
    // Pagina precedente, «2 / 3», seguente.
    x -= 28.0;
    let br = (x, cy - 14.0, 28.0, 28.0);
    ghost(f, br, 6.0, Act::Ed(EdAct::Next), false, cur + 1 < n);
    f.icon(icons::NEXT, x + 5.5, cy - 8.5, 17.0, if cur + 1 < n { (0xD5, 0xCC, 0xE2) } else { mix(e.e1, WHITE, 0.25) });
    x -= 54.0;
    f.p.text(&format!("{} / {}", (cur + 1).min(n.max(1)), n), x, r.1, 54.0, r.3, 13.0, BOLD, WHITE, TA::Center);
    x -= 28.0;
    let br = (x, cy - 14.0, 28.0, 28.0);
    ghost(f, br, 6.0, Act::Ed(EdAct::Prev), false, cur > 0);
    f.icon(icons::PREV, x + 5.5, cy - 8.5, 17.0, if cur > 0 { (0xD5, 0xCC, 0xE2) } else { mix(e.e1, WHITE, 0.25) });
}

// ---------------------------------------------------------------------------------------------
// «Ci sono modifiche non salvate»

fn leave_dialog(f: &mut Frame, ed: &PdfEd, e: E) {
    f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.5);
    f.hit((0.0, 0.0, f.w, f.h), Act::Nothing);
    let (w, h) = (560.0f32.min(f.w - 40.0), 232.0);
    let (x, y) = ((f.w - w) / 2.0, (f.h - h) / 2.0);
    f.p.shadow(x, y, w, h, 14.0, 22.0, 0.5, 8.0);
    f.p.rrect(x, y, w, h, 14.0, Some(mix(e.e1, WHITE, 0.05)), Some((mix(e.e1, WHITE, 0.2), 1.0)));
    f.p.circle(x + 44.0, y + 46.0, 20.0, Some(mix(AMBER, e.e1, 0.78)), None);
    f.icon(ic::WARNING, x + 32.0, y + 34.0, 24.0, AMBER);
    f.p.text("Ci sono modifiche non salvate", x + 80.0, y + 24.0, w - 108.0, 30.0, 20.0, BOLD, WHITE, TA::Left);
    let n = ed.unsaved();
    let out = crate::i18n::lit(&ed.out_name());
    let text = if n == 1 {
        format!("C'è 1 modifica non salvata nel file. La salvo in «{out}» prima di uscire? L'originale non si tocca.")
    } else {
        format!("Ci sono {n} modifiche non salvate nel file. Le salvo in «{out}» prima di uscire? L'originale non si tocca.")
    };
    f.para(&text, x + 80.0, y + 62.0, w - 108.0, 15.0, SANS, (0xD5, 0xCC, 0xE2), 1.4, 4);
    let by = y + h - 70.0;
    let bw = (w - 56.0 - 24.0) / 3.0;
    outline(f, e, (x + 28.0, by, bw, 44.0), "Resta qui", 15.0, Act::Ed(EdAct::LeaveStay), true);
    outline(f, e, (x + 40.0 + bw, by, bw, 44.0), "Esci senza salvare", 15.0, Act::Ed(EdAct::LeaveDiscard), true);
    solid(f, e, (x + 52.0 + 2.0 * bw, by, bw, 44.0), "Salva ed esci", 15.0, Act::Ed(EdAct::LeaveSave), !ed.saving);
}
