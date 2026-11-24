//! Il pannello a destra dell'editor dei PDF: in alto «Proprietà» con la scelta Semplice/Completa, sotto quello
//! che serve allo strumento in mano (colori, dimensioni, cerca e sostituisci, l'elenco dei campi, le aree da
//! oscurare…), in fondo il salvataggio con le sue opzioni. Il corpo scorre; l'intestazione e il piede restano.

use super::pdfeditor::{clip_hits, ghost, icons, input, outline, seg, slim, solid, swatches, tool_icon, veil, E, AMBER, WHITE};
use crate::pdfmod::{form, Align, Color, Edit, FieldKind, FontChoice, MarkKind, ShapeKind};
use crate::ui::assets as ic;
use crate::ui::core::{Act, Frame, Scroll, BOLD, R, SANS, SEMI};
use crate::ui::gfx::Align as TA;
use crate::ui::model::App;
use crate::ui::pdfedact::{EdAct, EdField};
use crate::ui::pdfeditor::{PdfEd, Target, Tool, INK_COLORS, MARK_COLORS, STAMPS};
use crate::ui::theme::mix;

type Rgb = (u8, u8, u8);

const HEAD: f32 = 50.0;
const PAD: f32 = 18.0;
/// Il rosso dei comandi che cancellano (oscura, togli).
const RED: Rgb = (0xE0, 0x3C, 0x48);

/// Dove disegnare i pezzi del corpo: l'inizio, la larghezza e la zona che si vede (per non disegnare quel che
/// è scrollato fuori).
struct Flow {
    x: f32,
    w: f32,
    view: R,
}

impl Flow {
    fn vis(&self, y: f32, h: f32) -> bool {
        y + h >= self.view.1 - 4.0 && y <= self.view.1 + self.view.3 + 4.0
    }
}

fn line_c(e: E) -> Rgb {
    mix(e.e1, WHITE, 0.09)
}

pub(super) fn panel(f: &mut Frame, app: &App, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, e.e1);
    f.p.fill(r.0, r.1, 1.0, r.3, line_c(e));
    let foot_h = foot_height(ed);
    let body = (r.0 + 1.0, r.1 + HEAD, r.2 - 1.0, (r.3 - HEAD - foot_h).max(60.0));
    // Il corpo si disegna per primo: intestazione e piede (che lo coprono) prendono i clic dopo di lui.
    let from = f.hits.len();
    let known = ed.lay.borrow().heights[0];
    let off = f.ui.scroll(Scroll::EdProps).clamp(0.0, (known - body.3).max(0.0));
    let fl = Flow { x: body.0 + PAD, w: body.2 - 2.0 * PAD, view: body };
    f.p.push_clip(body.0, body.1, body.2, body.3);
    let mut y = body.1 + 16.0 - off;
    y = tool_head(f, e, &fl, ed, y);
    y = tool_panel(f, ed, e, &fl, y);
    f.p.pop_clip();
    clip_hits(f, from, body);
    let content = y + off - body.1 + 12.0;
    ed.lay.borrow_mut().heights[0] = content;
    f.scrollbar(body, Scroll::EdProps, content);
    head(f, ed, e, (r.0 + 1.0, r.1, r.2 - 1.0, HEAD));
    foot(f, app, ed, e, (r.0 + 1.0, r.1 + r.3 - foot_h, r.2 - 1.0, foot_h));
}

// ---------------------------------------------------------------------------------------------
// L'intestazione

fn head(f: &mut Frame, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, HEAD, e.e1);
    f.p.fill(r.0, r.1 + HEAD - 1.0, r.2, 1.0, line_c(e));
    f.p.text("Proprietà", r.0 + PAD, r.1, 120.0, HEAD, 16.0, BOLD, WHITE, TA::Left);
    let w = 172.0;
    let tr = (r.0 + r.2 - PAD - w, r.1 + 11.0, w, 28.0);
    veil(f, tr, 8.0, 0.07);
    f.p.rrect(tr.0, tr.1, tr.2, tr.3, 8.0, None, Some((mix(e.e1, WHITE, 0.16), 1.0)));
    for (i, (label, simple)) in [("Semplice", true), ("Completa", false)].into_iter().enumerate() {
        let on = ed.complete != simple;
        let cell = (tr.0 + 2.0 + i as f32 * (w - 4.0) / 2.0, tr.1 + 2.0, (w - 4.0) / 2.0, tr.3 - 4.0);
        let act = Act::Ed(EdAct::Simple(simple));
        let t = f.hover_t(&act, true);
        if on {
            f.p.rrect(cell.0, cell.1, cell.2, cell.3, 6.0, Some(e.acc), None);
        } else if t > 0.01 {
            veil(f, cell, 6.0, 0.08 * t);
        }
        f.p.text(label, cell.0, cell.1, cell.2, cell.3, 12.5, BOLD, if on { e.on_acc } else { e.ink }, TA::Center);
        f.focus_ring(cell, 6.0, &act);
        f.hit(cell, act);
    }
}

// ---------------------------------------------------------------------------------------------
// Pezzi comuni

fn label(f: &mut Frame, e: E, fl: &Flow, y: f32, text: &str) -> f32 {
    if fl.vis(y, 18.0) {
        f.p.text(text, fl.x, y, fl.w, 18.0, 12.5, BOLD, e.muted, TA::Left);
    }
    y + 24.0
}

/// Un paragrafo che va a capo; restituisce dove finisce (con un po' d'aria sotto).
fn note(f: &mut Frame, fl: &Flow, y: f32, text: &str, size: f32, c: Rgb) -> f32 {
    let h = f.p.wrap_height(text, size, SANS, fl.w, size * 1.4, 0);
    if fl.vis(y, h) {
        f.para(text, fl.x, y, fl.w, size, SANS, c, 1.4, 0);
    }
    y + h + 12.0
}

/// Un riquadro colorato con un titolo e una riga sotto (il carattere del testo, le aree da oscurare…).
fn callout(f: &mut Frame, fl: &Flow, y: f32, tint: Rgb, title: &str, sub: &str) -> f32 {
    let tw = fl.w - 28.0;
    let sh = if sub.is_empty() { 0.0 } else { f.p.wrap_height(sub, 12.5, SANS, tw, 12.5 * 1.4, 0) + 4.0 };
    let h = 12.0 + 20.0 + sh + 10.0;
    if fl.vis(y, h) {
        f.p.rrect_a(fl.x, y, fl.w, h, 9.0, tint, 0.13);
        f.p.rrect(fl.x, y, fl.w, h, 9.0, None, Some((mix(tint, (0, 0, 0), 0.25), 1.0)));
        f.p.rrect(fl.x, y + 8.0, 3.0, h - 16.0, 1.5, Some(tint), None);
        f.p.text(title, fl.x + 14.0, y + 10.0, tw, 22.0, 13.5, BOLD, WHITE, TA::Left);
        if !sub.is_empty() {
            f.para(sub, fl.x + 14.0, y + 34.0, tw, 12.5, SANS, (0xD5, 0xCC, 0xE2), 1.4, 0);
        }
    }
    y + h + 14.0
}

fn slider_row(f: &mut Frame, e: E, fl: &Flow, y: f32, name: &str, value: &str, frac: f32, act: EdAct) -> f32 {
    if fl.vis(y, 52.0) {
        f.p.text(name, fl.x, y, fl.w * 0.55, 20.0, 13.0, SEMI, e.ink, TA::Left);
        f.p.text(value, fl.x + fl.w * 0.5, y, fl.w * 0.5, 20.0, 13.0, BOLD, WHITE, TA::Right);
        slim(f, e, (fl.x + 10.0, y + 24.0, fl.w - 20.0, 20.0), frac, Act::Ed(act));
    }
    y + 54.0
}

fn toggle_row(f: &mut Frame, e: E, fl: &Flow, y: f32, title: &str, sub: &str, on: bool, act: EdAct) -> f32 {
    let h = if sub.is_empty() { 40.0 } else { 50.0 };
    if fl.vis(y, h) {
        let act = Act::Ed(act);
        ghost(f, (fl.x - 8.0, y, fl.w + 16.0, h), 8.0, act.clone(), false, true);
        let sw = 44.0;
        let tw = fl.w - sw - 12.0;
        if sub.is_empty() {
            f.p.text(title, fl.x, y, tw, h, 13.5, BOLD, WHITE, TA::Left);
        } else {
            f.p.text(title, fl.x, y + 4.0, tw, 22.0, 13.5, BOLD, WHITE, TA::Left);
            f.p.text(sub, fl.x, y + 26.0, tw, 18.0, 12.0, SANS, e.muted, TA::Left);
        }
        f.switch(fl.x + fl.w - sw, y + (h - 24.0) / 2.0, sw, 24.0, on, act, Some(e.acc));
    }
    y + h + 2.0
}

/// Le scelte a pulsantini (i timbri): un colore davanti al nome; quella scelta ha il bordo dell'accento.
fn chips(f: &mut Frame, e: E, fl: &Flow, mut y: f32, items: &[(String, Rgb, bool, EdAct)], cols: usize) -> f32 {
    let gap = 8.0;
    let w = (fl.w - gap * (cols as f32 - 1.0)) / cols as f32;
    for (i, (name, c, on, act)) in items.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        let r = (fl.x + col as f32 * (w + gap), y + row as f32 * 42.0, w, 36.0);
        if !fl.vis(r.1, r.3) {
            continue;
        }
        let act = Act::Ed(act.clone());
        let t = f.hover_t(&act, true);
        veil(f, r, 8.0, if *on { 0.13 } else { 0.05 + 0.05 * t });
        f.p.rrect(r.0, r.1, r.2, r.3, 8.0, None, Some(if *on { (e.acc, 1.5) } else { (mix(e.e1, WHITE, 0.16), 1.0) }));
        f.p.circle(r.0 + 15.0, r.1 + r.3 / 2.0, 6.0, Some(*c), Some((mix(*c, (0, 0, 0), 0.3), 1.0)));
        f.p.text(name, r.0 + 28.0, r.1, r.2 - 34.0, r.3, 12.5, BOLD, if *on { WHITE } else { e.ink }, TA::Left);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
    y += items.len().div_ceil(cols) as f32 * 42.0;
    y + 4.0
}

/// I pallini dei colori con il loro titolo.
fn colors_block(f: &mut Frame, e: E, fl: &Flow, y: f32, title: &str, list: &[Color], active: Color, make: fn(Color) -> EdAct) -> f32 {
    let y = label(f, e, fl, y, title);
    let opts: Vec<Option<Color>> = list.iter().map(|c| Some(*c)).collect();
    let h = swatches(f, e, fl.x + 4.0, y + 4.0, fl.w - 8.0, &opts, Some(Some(active)), |c| make(c.unwrap_or(active)));
    y + h + 22.0
}

fn two(fl: &Flow, y: f32, h: f32) -> (R, R) {
    let w = (fl.w - 10.0) / 2.0;
    ((fl.x, y, w, h), (fl.x + w + 10.0, y, w, h))
}

/// Un pulsante pieno di un colore a scelta (oscura, segna).
fn solid_c(f: &mut Frame, e: E, r: R, label: &str, c: Rgb, act: Act, enabled: bool) {
    let t = f.hover_t(&act, enabled);
    let press = f.press_t(&act, enabled);
    let rr = Frame::pressed_rect(r, press);
    let fill = if enabled { mix(c, WHITE, 0.08 * t) } else { mix(c, e.e1, 0.7) };
    f.p.rrect(rr.0, rr.1, rr.2, rr.3, 8.0, Some(fill), None);
    f.p.text(label, rr.0 + 8.0, rr.1, rr.2 - 16.0, rr.3, 13.5, BOLD, if enabled { WHITE } else { mix(e.ink, e.e1, 0.45) }, TA::Center);
    if enabled {
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
}

// ---------------------------------------------------------------------------------------------
// Il titolo dello strumento

fn tool_head(f: &mut Frame, e: E, fl: &Flow, ed: &PdfEd, y: f32) -> f32 {
    let about = ed.tool.about();
    let tw = fl.w - 50.0;
    let ah = f.p.wrap_height(about, 12.5, SANS, tw, 12.5 * 1.4, 0);
    let h = (26.0 + ah).max(38.0);
    if fl.vis(y, h) {
        let tile = (fl.x, y, 38.0, 38.0);
        f.p.rrect_a(tile.0, tile.1, tile.2, tile.3, 10.0, e.acc, 0.16);
        f.icon(tool_icon(ed.tool), tile.0 + 8.0, tile.1 + 8.0, 22.0, e.acc);
        f.p.text(ed.tool.label(), fl.x + 50.0, y - 1.0, tw, 22.0, 15.5, BOLD, WHITE, TA::Left);
        f.para(about, fl.x + 50.0, y + 22.0, tw, 12.5, SANS, e.muted, 1.4, 0);
    }
    let y = y + h + 10.0;
    if fl.vis(y, 1.0) {
        f.p.fill(fl.x - PAD + 1.0, y, fl.w + 2.0 * PAD, 1.0, line_c(e));
    }
    y + 16.0
}

// ---------------------------------------------------------------------------------------------
// Quello che serve a ogni strumento

fn tool_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, y: f32) -> f32 {
    match ed.tool {
        Tool::Select => select_panel(f, ed, e, fl, y),
        Tool::EditText => edit_text_panel(f, ed, e, fl, y),
        Tool::AddText => {
            let y = text_controls(f, ed, e, fl, y);
            note(f, fl, y, "Invio va a capo. Clic fuori dal riquadro per finire; Esc annulla.", 12.5, e.muted)
        }
        Tool::Find => find_panel(f, ed, e, fl, y),
        Tool::Highlight => {
            let y = colors_block(f, e, fl, y, "Colore", &MARK_COLORS, ed.mark, EdAct::Mark);
            note(f, fl, y, "L'evidenziatore segue le parole: su più righe diventa un pezzo per riga, come in Word.", 12.5, e.muted)
        }
        Tool::Underline | Tool::Strike => {
            let eff = if ed.mark == MARK_COLORS[0] { INK_COLORS[2] } else { ed.mark };
            colors_block(f, e, fl, y, "Colore", &INK_COLORS, eff, EdAct::Mark)
        }
        Tool::Note => {
            let y = colors_block(f, e, fl, y, "Colore della nota", &MARK_COLORS, ed.mark, EdAct::Mark);
            note(f, fl, y, "Clic sulla pagina e scrivi. L'elenco a sinistra raccoglie tutte le note.", 12.5, e.muted)
        }
        Tool::Pen => {
            let y = colors_block(f, e, fl, y, "Colore", &INK_COLORS, ed.ink, EdAct::Ink);
            slider_row(f, e, fl, y, "Spessore", &format!("{} px", ed.width.round()), (ed.width - 1.0) / 15.0, EdAct::WidthSlider)
        }
        Tool::Stamp => stamp_panel(f, ed, e, fl, y),
        Tool::Image => image_panel(f, ed, e, fl, y),
        Tool::Rect | Tool::Ellipse => {
            let mut y = colors_block(f, e, fl, y, "Colore della linea", &INK_COLORS, ed.ink, EdAct::Ink);
            y = slider_row(f, e, fl, y, "Spessore", &format!("{} px", ed.width.round()), (ed.width - 1.0) / 15.0, EdAct::WidthSlider);
            y = label(f, e, fl, y + 4.0, "Riempimento");
            let mut opts: Vec<Option<Color>> = vec![None];
            opts.extend(MARK_COLORS.iter().map(|c| Some(*c)));
            let h = swatches(f, e, fl.x + 4.0, y + 4.0, fl.w - 8.0, &opts, Some(ed.fill), EdAct::FillColor);
            y + h + 22.0
        }
        Tool::Line | Tool::Arrow => {
            let y = colors_block(f, e, fl, y, "Colore", &INK_COLORS, ed.ink, EdAct::Ink);
            slider_row(f, e, fl, y, "Spessore", &format!("{} px", ed.width.round()), (ed.width - 1.0) / 15.0, EdAct::WidthSlider)
        }
        Tool::Fill => fill_panel(f, ed, e, fl, y),
        Tool::NewText | Tool::NewCheck | Tool::NewRadio | Tool::NewList | Tool::NewSign => new_field_panel(f, ed, e, fl, y),
        Tool::Area => area_panel(f, ed, e, fl, y),
        Tool::SearchRedact => search_redact_panel(f, ed, e, fl, y),
        Tool::Hidden => hidden_panel(f, ed, e, fl, y),
    }
}

/// Cosa è una modifica, per il titolo del pannello quando è scelta.
fn edit_name(e: &Edit) -> (&'static str, &'static str) {
    match e {
        Edit::Mark { kind, .. } => match kind {
            MarkKind::Highlight => ("Evidenziatore", icons::HIGHLIGHT),
            MarkKind::Underline => ("Sottolineato", icons::UNDERLINE),
            MarkKind::Strike => ("Barrato", icons::STRIKE),
        },
        Edit::Note { .. } => ("Nota", icons::NOTE),
        Edit::Pen { .. } => ("Disegno a mano", icons::PENCIL),
        Edit::Stamp { .. } => ("Timbro", icons::STAMP),
        Edit::Shape { kind, .. } => match kind {
            ShapeKind::Rect => ("Rettangolo", icons::RECT),
            ShapeKind::Ellipse => ("Ellisse", icons::ELLIPSE),
            ShapeKind::Line => ("Linea", icons::LINE),
            ShapeKind::Arrow => ("Freccia", icons::ARROW),
        },
        Edit::Image { .. } => ("Immagine", ic::IMAGE),
        Edit::AddText { .. } => ("Testo aggiunto", icons::ADD_TEXT),
        Edit::EditText { .. } => ("Testo modificato", icons::EDIT_TEXT),
        Edit::FieldValue { .. } => ("Campo compilato", icons::FILL),
        Edit::NewField { .. } => ("Campo nuovo", icons::NEW_TEXT),
        Edit::Redact { .. } => ("Area da oscurare", icons::REDACT),
        Edit::DeleteAnnot { .. } => ("Commento tolto", icons::NOTE),
    }
}

fn select_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    if let Some(edit) = ed.sel.and_then(|i| ed.edits.get(i)) {
        let (name, icon) = edit_name(edit);
        if fl.vis(y, 36.0) {
            f.p.rrect_a(fl.x, y, 34.0, 34.0, 9.0, WHITE, 0.08);
            f.icon(icon, fl.x + 8.0, y + 8.0, 18.0, (0xE7, 0xE0, 0xF1));
            f.p.text(name, fl.x + 46.0, y, fl.w - 46.0, 34.0, 14.0, BOLD, WHITE, TA::Left);
        }
        y += 48.0;
        let list: Option<(&[Color], fn(Color) -> EdAct)> = match edit {
            Edit::Mark { kind: MarkKind::Highlight, .. } | Edit::Note { .. } => Some((&MARK_COLORS, EdAct::Mark)),
            Edit::Mark { .. } => Some((&INK_COLORS, EdAct::Mark)),
            Edit::Pen { .. } | Edit::Shape { .. } | Edit::AddText { .. } => Some((&INK_COLORS, EdAct::Ink)),
            _ => None,
        };
        if let (Some((colors, make)), Some(active)) = (list, ed.sel_color()) {
            y = colors_block(f, e, fl, y, "Colore", colors, active, make);
        }
        if ed.sel_text().is_some() {
            y = label(f, e, fl, y, if matches!(edit, Edit::Stamp { .. }) { "Testo del timbro" } else { "Testo" });
            if fl.vis(y, 38.0) {
                input(f, e, (fl.x, y, fl.w, 38.0), EdField::SelText, &ed.field_text(EdField::SelText), "scrivi qui", 13.5, None);
            }
            y += 50.0;
        }
        if fl.vis(y, 40.0) {
            outline(f, e, (fl.x, y, fl.w, 40.0), "Togli questa modifica", 13.5, Act::Ed(EdAct::Delete), true);
        }
        return note(f, fl, y + 50.0, "Trascinala per spostarla. Tasto Canc per toglierla.", 12.5, e.muted);
    }
    if let Some(c) = ed.sel_file.and_then(|id| ed.opened.as_ref().and_then(|o| o.comments.iter().find(|c| c.id == id))) {
        if fl.vis(y, 36.0) {
            f.p.rrect_a(fl.x, y, 34.0, 34.0, 9.0, WHITE, 0.08);
            f.icon(super::pdfeditor::comment_icon(&c.kind), fl.x + 8.0, y + 8.0, 18.0, (0xE7, 0xE0, 0xF1));
            f.p.text(&crate::i18n::lit(&c.kind), fl.x + 46.0, y, fl.w - 46.0, 34.0, 14.0, BOLD, WHITE, TA::Left);
        }
        y += 48.0;
        if !c.author.is_empty() {
            y = note(f, fl, y, &format!("Scritto da {}", crate::i18n::lit(&c.author)), 12.5, e.muted);
        }
        if !c.text.trim().is_empty() {
            y = note(f, fl, y, &crate::i18n::lit(c.text.trim()), 13.0, e.ink);
        }
        if fl.vis(y, 40.0) {
            outline(f, e, (fl.x, y, fl.w, 40.0), "Togli questo commento", 13.5, Act::Ed(EdAct::Delete), true);
        }
        return note(f, fl, y + 50.0, "Era già nel file: si toglie dalla copia che salvi, l'originale resta com'è.", 12.5, e.muted);
    }
    let n = ed.edits.len();
    let msg = match n {
        0 => "Non hai ancora aggiunto niente. Scegli uno strumento in alto e lavora sulla pagina.".to_string(),
        1 => "Hai fatto 1 modifica. Clic su una cosa aggiunta per sceglierla.".to_string(),
        n => format!("Hai fatto {n} modifiche. Clic su una cosa aggiunta per sceglierla."),
    };
    note(f, fl, y, &msg, 13.0, e.muted)
}

fn edit_text_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    match ed.text.as_ref() {
        Some(t) if !matches!(t.target, Target::Note | Target::NoteAgain(_)) => {
            let name = if t.font_name.is_empty() { "Carattere del testo".to_string() } else { crate::i18n::lit(&t.font_name) };
            let title = format!("{name} · {} pt", t.size.round());
            let miss = t.missing();
            if miss.is_empty() {
                y = callout(f, fl, y, e.acc, &title, "Il carattere del documento ha tutte le lettere che servono.");
            } else {
                let list: Vec<String> = miss.iter().take(12).map(|c| c.to_string()).collect();
                y = callout(f, fl, y, AMBER, &title, &format!("Nel carattere del file mancano: {}. Quelle le scrivo con un altro carattere.", list.join(" ")));
            }
            text_controls(f, ed, e, fl, y)
        }
        _ => {
            y = note(f, fl, y, "Clic su un paragrafo della pagina: si scrive lì, il resto non si muove. Invio va a capo, clic fuori per finire; Esc annulla.", 13.0, e.muted);
            note(f, fl, y, "Se il paragrafo è tratteggiato di giallo il suo carattere non si può riscrivere: lo cancelli con «Oscura» e lo rifai con «Aggiungi testo».", 12.5, e.muted)
        }
    }
}

/// Dimensione, colore e (in «Completa») carattere, allineamento, grassetto e corsivo del testo che si scrive.
fn text_controls(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    let t = ed.text.as_ref();
    let size = t.map_or(ed.size, |t| t.size);
    let color = t.map_or(ed.text_ink, |t| t.color);
    let (bold, italic) = t.map_or((ed.bold, ed.italic), |t| (t.bold, t.italic));
    let align = t.map_or(ed.align, |t| t.align);
    let font = t.map_or(&ed.font, |t| &t.font);
    y = slider_row(f, e, fl, y, "Dimensione", &format!("{} pt", size.round()), (size.clamp(6.0, 96.0) - 6.0) / 90.0, EdAct::SizeSlider);
    y = colors_block(f, e, fl, y, "Colore del testo", &INK_COLORS, color, EdAct::Ink);
    if !ed.complete {
        return y;
    }
    y = label(f, e, fl, y, "Carattere");
    if fl.vis(y, 38.0) {
        let name = match font {
            FontChoice::Same => "Quello del file".to_string(),
            FontChoice::Family(n) => crate::i18n::lit(n),
        };
        f.select((fl.x, y, fl.w, 38.0), &name, None, 13.5, Act::Ed(EdAct::FontMenu));
    }
    y += 52.0;
    y = label(f, e, fl, y, "Allineamento");
    if fl.vis(y, 32.0) {
        let items = vec![
            ("Sinistra".to_string(), align == Align::Left, Act::Ed(EdAct::Align(Align::Left))),
            ("Centro".to_string(), align == Align::Center, Act::Ed(EdAct::Align(Align::Center))),
            ("Destra".to_string(), align == Align::Right, Act::Ed(EdAct::Align(Align::Right))),
        ];
        seg(f, e, (fl.x, y, fl.w, 32.0), &items);
    }
    y += 46.0;
    if fl.vis(y, 32.0) {
        let items = vec![("Grassetto".to_string(), bold, Act::Ed(EdAct::Bold)), ("Corsivo".to_string(), italic, Act::Ed(EdAct::Italic))];
        seg(f, e, (fl.x, y, fl.w, 32.0), &items);
    }
    y + 46.0
}

fn find_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    y = label(f, e, fl, y, "Cerca");
    if fl.vis(y, 38.0) {
        input(f, e, (fl.x, y, fl.w, 38.0), EdField::Find, &ed.find, "una parola, per esempio «importo»", 13.5, Some(ic::SEARCH));
    }
    y += 50.0;
    y = label(f, e, fl, y, "Sostituisci con");
    if fl.vis(y, 38.0) {
        input(f, e, (fl.x, y, fl.w, 38.0), EdField::Replace, &ed.replace, "il testo nuovo", 13.5, None);
    }
    y += 50.0;
    let has = !ed.find.trim().is_empty();
    let n = ed.found.len();
    if fl.vis(y, 40.0) {
        let (a, b) = two(fl, y, 40.0);
        solid(f, e, a, if ed.finding { "Cerco…" } else { "Cerca" }, 13.5, Act::Ed(EdAct::FindRun), has && !ed.finding);
        outline(f, e, b, "Sostituisci", 13.5, Act::Ed(EdAct::ReplaceOne), n > 0);
    }
    y += 50.0;
    if fl.vis(y, 40.0) {
        let all = if ed.find_done && n > 0 { format!("Sostituisci tutte ({n})") } else { "Sostituisci tutte".to_string() };
        outline(f, e, (fl.x, y, fl.w, 40.0), &all, 13.5, Act::Ed(EdAct::ReplaceAll), has && !ed.finding);
    }
    y += 52.0;
    if ed.complete {
        y = toggle_row(f, e, fl, y, "Maiuscole e minuscole", "«Rossi» non trova «rossi»", ed.case, EdAct::Case);
        y = toggle_row(f, e, fl, y, "Solo parole intere", "«casa» non trova «casale»", ed.whole, EdAct::Whole);
        y += 6.0;
    }
    if ed.finding {
        return note(f, fl, y, "Cerco in tutte le pagine…", 13.0, e.muted);
    }
    if !ed.find_done {
        return note(f, fl, y, "Vale per tutte le pagine. Ogni parola sostituita si può annullare.", 12.5, e.muted);
    }
    if n == 0 {
        return note(f, fl, y, "Non ho trovato niente.", 13.0, e.muted);
    }
    // «3 trovate» con le frecce per scorrerle.
    if fl.vis(y, 30.0) {
        f.p.text(&if n == 1 { "1 trovata".to_string() } else { format!("{n} trovate") }, fl.x, y, fl.w - 80.0, 30.0, 13.5, BOLD, WHITE, TA::Left);
        for (i, (icon, d)) in [(icons::PREV, -1), (icons::NEXT, 1)].into_iter().enumerate() {
            let br = (fl.x + fl.w - 68.0 + i as f32 * 36.0, y, 32.0, 30.0);
            let act = Act::Ed(EdAct::FindNext(d));
            ghost(f, br, 7.0, act, false, true);
            f.icon(icon, br.0 + 7.0, br.1 + 5.0, 18.0, (0xE7, 0xE0, 0xF1));
        }
    }
    y += 40.0;
    for (i, (p, hit)) in ed.found.iter().enumerate().take(200) {
        let r = (fl.x - 8.0, y, fl.w + 16.0, 36.0);
        if fl.vis(r.1, r.3) {
            let act = Act::Ed(EdAct::FindHit(i));
            ghost(f, r, 7.0, act, i == ed.find_cur, true);
            f.p.text(&format!("pag. {}", p + 1), fl.x, r.1, 54.0, r.3, 12.0, SEMI, e.muted, TA::Left);
            f.p.text(&format!("«{}»", crate::i18n::lit(&hit.text)), fl.x + 56.0, r.1, fl.w - 56.0, r.3, 13.0, SANS, e.ink, TA::Left);
        }
        y += 38.0;
    }
    if n > 200 {
        y = note(f, fl, y + 4.0, &format!("…e altre {}.", n - 200), 12.5, e.muted);
    }
    y
}

fn stamp_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    y = label(f, e, fl, y, "Quale timbro");
    let items: Vec<(String, Rgb, bool, EdAct)> = STAMPS.iter().enumerate().map(|(i, (n, c))| (n.to_string(), (c[0], c[1], c[2]), ed.stamp == i, EdAct::StampPick(i))).collect();
    y = chips(f, e, fl, y, &items, 1 + (fl.w > 300.0) as usize);
    if ed.complete {
        y = label(f, e, fl, y + 10.0, "Oppure scrivi il testo");
        if fl.vis(y, 38.0) {
            input(f, e, (fl.x, y, fl.w, 38.0), EdField::StampText, &ed.stamp_text, "per esempio «PAGATO»", 13.5, None);
        }
        y += 52.0;
    }
    note(f, fl, y, "Clic sulla pagina per metterlo grande com'è, oppure trascina per scegliere la misura.", 12.5, e.muted)
}

fn image_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    y = label(f, e, fl, y, "Quale immagine");
    match &ed.image {
        Some((_, w, h)) => y = callout(f, fl, y, e.ok, "Immagine pronta", &format!("{w} × {h} pixel. Clic sulla pagina per metterla, o trascina per scegliere dove.")),
        None => y = callout(f, fl, y, e.muted, "Nessuna immagine scelta", "Scegli un PNG o un JPEG: gli altri formati diventano PNG da soli."),
    }
    if fl.vis(y, 42.0) {
        solid(f, e, (fl.x, y, fl.w, 42.0), if ed.image.is_some() { "Scegli un'altra immagine…" } else { "Scegli dal PC…" }, 13.5, Act::Ed(EdAct::PickImage), true);
    }
    note(f, fl, y + 54.0, "L'immagine resta com'è, senza stirarsi: si ridimensiona dopo, tenendo le proporzioni.", 12.5, e.muted)
}

fn fill_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    let Some(o) = ed.opened.as_ref() else { return y };
    let list: Vec<(usize, &form::Field)> = o.fields.iter().enumerate().filter(|(_, fld)| !fld.pushbutton).collect();
    if list.is_empty() {
        return note(f, fl, y, "Questo PDF non ha campi da compilare. Nella scheda «Moduli» puoi crearne di nuovi: casella di testo, spunta, scelta, elenco.", 13.0, e.muted);
    }
    let done = list.iter().filter(|(_, fld)| ed.field_filled(fld)).count();
    if fl.vis(y, 24.0) {
        f.p.text("Campi del modulo", fl.x, y, fl.w * 0.6, 22.0, 13.5, BOLD, WHITE, TA::Left);
        f.p.text(&format!("{done} di {} compilati", list.len()), fl.x + fl.w * 0.4, y, fl.w * 0.6, 22.0, 12.5, SEMI, e.muted, TA::Right);
    }
    y += 32.0;
    for (i, fld) in &list {
        let r = (fl.x - 8.0, y, fl.w + 16.0, 36.0);
        if fl.vis(r.1, r.3) {
            let ok = ed.field_filled(fld);
            let act = Act::Ed(EdAct::FieldGo(*i));
            ghost(f, r, 7.0, act, false, true);
            let c = (fl.x + 10.0, y + 18.0);
            if ok {
                f.p.circle(c.0, c.1, 9.0, Some(e.ok), None);
                f.icon(ic::CHECK, c.0 - 6.0, c.1 - 6.0, 12.0, (0x10, 0x2A, 0x20));
            } else {
                f.p.circle(c.0, c.1, 8.5, None, Some((mix(e.e1, WHITE, 0.35), 1.5)));
            }
            f.p.text(&crate::i18n::lit(&fld.name), fl.x + 28.0, r.1, fl.w - 100.0, r.3, 13.0, SEMI, e.ink, TA::Left);
            f.p.text(if ok { "fatto" } else { "vuoto" }, fl.x + fl.w - 60.0, r.1, 60.0, r.3, 12.0, SEMI, if ok { e.ok } else { e.muted }, TA::Right);
        }
        y += 38.0;
    }
    y += 6.0;
    if fl.vis(y, 40.0) {
        let (a, b) = two(fl, y, 40.0);
        outline(f, e, a, "Prossimo vuoto", 13.0, Act::Ed(EdAct::FieldNext), done < list.len());
        outline(f, e, b, "Svuota tutti", 13.0, Act::Ed(EdAct::FieldsClear), ed.edits.iter().any(|x| matches!(x, Edit::FieldValue { .. })));
    }
    y += 54.0;
    note(f, fl, y, "Clic su un campo della pagina per compilarlo. Sono campi veri del PDF: si compilano anche con altri programmi.", 12.5, e.muted)
}

fn new_field_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    if ed.tool == Tool::NewSign {
        y = callout(f, fl, y, e.acc, "Firma digitale", "Qui si mette il riquadro della firma. Firmare è un'attività a parte («Firma digitale»): il file salvato ci arriva già pronto.");
    } else {
        y = note(f, fl, y, "Trascina sulla pagina per scegliere dove sta e quanto è grande; un clic solo lo mette della misura normale.", 13.0, e.muted);
    }
    let mine: Vec<(&str, usize, FieldKind)> = ed
        .edits
        .iter()
        .filter_map(|x| match x {
            Edit::NewField { name, page, kind, .. } => Some((name.as_str(), *page, *kind)),
            _ => None,
        })
        .collect();
    if mine.is_empty() {
        return note(f, fl, y, "Sono campi veri del PDF (AcroForm): chi apre il file li compila anche con altri programmi. Il nome si dà da solo: «Testo1», «Spunta1»…", 12.5, e.muted);
    }
    y = label(f, e, fl, y, "Campi che hai messo");
    for (name, page, kind) in mine {
        if fl.vis(y, 32.0) {
            let kind_name = match kind {
                FieldKind::Text => "casella",
                FieldKind::Check => "spunta",
                FieldKind::Radio => "scelta",
                FieldKind::List => "elenco",
                FieldKind::Sign => "firma",
            };
            f.p.text(&crate::i18n::lit(name), fl.x, y, fl.w * 0.55, 30.0, 13.0, SEMI, e.ink, TA::Left);
            f.p.text(&format!("{kind_name} · pag. {}", page + 1), fl.x + fl.w * 0.4, y, fl.w * 0.6, 30.0, 12.0, SANS, e.muted, TA::Right);
        }
        y += 32.0;
    }
    y + 6.0
}

fn area_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    let pend = ed.pending_redacts();
    let done = ed.edits.iter().filter(|x| matches!(x, Edit::Redact { applied: true, .. })).count();
    let title = if pend > 0 {
        if pend == 1 { "1 area da oscurare".to_string() } else { format!("{pend} aree da oscurare") }
    } else if done > 0 {
        if done == 1 { "1 area oscurata".to_string() } else { format!("{done} aree oscurate") }
    } else {
        "Nessuna area scelta".to_string()
    };
    y = callout(f, fl, y, RED, &title, "Finché non premi «Applica» il testo c'è ancora: le aree sono tratteggiate sulla pagina.");
    if fl.vis(y, 44.0) {
        solid_c(f, e, (fl.x, y, fl.w, 44.0), &if pend > 0 { format!("Applica l'oscuramento ({pend})") } else { "Applica l'oscuramento".to_string() }, RED, Act::Ed(EdAct::ApplyRedact), pend > 0);
    }
    y += 54.0;
    if pend > 0 && fl.vis(y, 38.0) {
        outline(f, e, (fl.x, y, fl.w, 38.0), "Togli le aree segnate", 13.0, Act::Ed(EdAct::ClearRedact), true);
        y += 48.0;
    }
    note(f, fl, y + 4.0, "Cancella davvero: toglie dal file il testo e le immagini sotto il riquadro, così non si recuperano né copiando né cercando.", 12.5, e.muted)
}

fn search_redact_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    y = label(f, e, fl, y, "Cosa cerco");
    for (i, p) in crate::pdfmod::find::Pattern::ALL.iter().enumerate() {
        let r = (fl.x, y, fl.w, 40.0);
        if fl.vis(r.1, r.3) {
            let marked = ed.marked(*p);
            let busy = ed.pat_which == Some(Some(*p));
            let act = Act::Ed(EdAct::Pattern(i));
            let t = f.hover_t(&act, true);
            veil(f, r, 8.0, if marked.is_some() { 0.13 } else { 0.05 + 0.05 * t });
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, None, Some(if marked.is_some() { (RED, 1.5) } else { (mix(e.e1, WHITE, 0.16), 1.0) }));
            f.p.text(p.label(), r.0 + 14.0, r.1, r.2 - 100.0, r.3, 13.5, BOLD, WHITE, TA::Left);
            let state = match (marked, busy) {
                (_, true) => "cerco…".to_string(),
                (Some(0), _) => "nessuno".to_string(),
                (Some(n), _) => format!("{n} segnati"),
                (None, _) => "cerca".to_string(),
            };
            f.p.text(&state, r.0 + r.2 - 110.0, r.1, 96.0, r.3, 12.0, SEMI, if marked.is_some_and(|n| n > 0) { RED } else { e.muted }, TA::Right);
            f.focus_ring(r, 8.0, &act);
            f.hit(r, act);
        }
        y += 46.0;
    }
    y = label(f, e, fl, y + 6.0, "Oppure una parola");
    if fl.vis(y, 38.0) {
        let bw = 88.0;
        input(f, e, (fl.x, y, fl.w - bw - 8.0, 38.0), EdField::Free, &ed.free, "per esempio «Rossi»", 13.5, None);
        solid_c(f, e, (fl.x + fl.w - bw, y, bw, 38.0), "Segna", RED, Act::Ed(EdAct::MarkFree), !ed.free.trim().is_empty() && ed.pat_which != Some(None));
    }
    y += 48.0;
    if let Some(n) = ed.free_n {
        y = note(f, fl, y, &if n == 0 { "Non l'ho trovata.".to_string() } else if n == 1 { "Segnata 1 area.".to_string() } else { format!("Segnate {n} aree.") }, 12.5, e.muted);
    }
    let pend = ed.pending_redacts();
    y += 6.0;
    if fl.vis(y, 44.0) {
        solid_c(f, e, (fl.x, y, fl.w, 44.0), &if pend > 0 { format!("Applica l'oscuramento ({pend})") } else { "Applica l'oscuramento".to_string() }, RED, Act::Ed(EdAct::ApplyRedact), pend > 0);
    }
    y += 54.0;
    if pend > 0 && fl.vis(y, 38.0) {
        outline(f, e, (fl.x, y, fl.w, 38.0), "Togli le aree segnate", 13.0, Act::Ed(EdAct::ClearRedact), true);
        y += 48.0;
    }
    note(f, fl, y, "Le parole trovate si segnano da sole sulla pagina. Controlla, poi applica: dopo non si torna indietro nel file salvato.", 12.5, e.muted)
}

fn hidden_panel(f: &mut Frame, ed: &PdfEd, e: E, fl: &Flow, mut y: f32) -> f32 {
    let Some(o) = ed.opened.as_ref() else { return y };
    let h = &o.hidden;
    y = callout(f, fl, y, e.acc, "Cosa porta con sé questo file", "Letto dal PDF, prima di salvare.");
    let yes = |b: bool| if b { "presenti" } else { "nessuno" };
    let rows: [(&str, String); 7] = [
        ("Autore", if h.author.is_empty() { "—".into() } else { crate::i18n::lit(&h.author) }),
        ("Titolo", if h.title.is_empty() { "—".into() } else { crate::i18n::lit(&h.title) }),
        ("Creato con", if h.creator.is_empty() { "—".into() } else { crate::i18n::lit(&h.creator) }),
        ("Programma PDF", if h.producer.is_empty() { "—".into() } else { crate::i18n::lit(&h.producer) }),
        ("Dati XMP", yes(h.xmp).to_string()),
        ("Allegati", if h.attachments == 0 { "nessuno".into() } else { h.attachments.to_string() }),
        ("Script", if h.javascript { "presenti".into() } else { "nessuno".into() }),
    ];
    for (k, v) in rows {
        if fl.vis(y, 28.0) {
            f.p.text(k, fl.x, y, fl.w * 0.4, 28.0, 12.5, SEMI, e.muted, TA::Left);
            f.p.text(&v, fl.x + fl.w * 0.38, y, fl.w * 0.62, 28.0, 13.0, BOLD, WHITE, TA::Right);
        }
        y += 30.0;
    }
    y += 6.0;
    toggle_row(f, e, fl, y, "Toglili quando salvo", "autore, programma e dati nascosti", ed.strip, EdAct::Strip)
}

// ---------------------------------------------------------------------------------------------
// Il piede: salvataggio

fn foot_height(ed: &PdfEd) -> f32 {
    if ed.saved.is_some() && !ed.saving {
        return 206.0;
    }
    let mut h = 224.0;
    if ed.options_open {
        h += 108.0;
    }
    if ed.pending_redacts() > 0 {
        h += 50.0;
    }
    h
}

fn foot(f: &mut Frame, app: &App, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, mix(e.e1, WHITE, 0.03));
    f.p.fill(r.0, r.1, r.2, 1.0, line_c(e));
    let fl = Flow { x: r.0 + PAD, w: r.2 - 2.0 * PAD, view: r };
    let mut y = r.1 + 14.0;
    if let (Some(path), false) = (ed.saved.as_ref(), ed.saving) {
        f.p.circle(fl.x + 13.0, y + 14.0, 13.0, Some(e.ok), None);
        f.icon(ic::CHECK, fl.x + 6.0, y + 7.0, 14.0, (0x10, 0x2A, 0x20));
        f.p.text("PDF salvato", fl.x + 36.0, y, fl.w - 36.0, 28.0, 17.0, BOLD, WHITE, TA::Left);
        y += 36.0;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let msg = if ed.save_msg.is_empty() { format!("Salvato come {}.", crate::i18n::lit(&name)) } else { format!("Salvato come {}: {}.", crate::i18n::lit(&name), ed.save_msg) };
        y = note(f, &fl, y, &msg, 12.5, (0xD5, 0xCC, 0xE2));
        let (a, b) = two(&fl, y, 40.0);
        solid(f, e, a, "Apri il PDF", 13.5, Act::Ed(EdAct::OpenSaved), true);
        outline(f, e, b, "Apri la cartella", 13.5, Act::Ed(EdAct::OpenFolder), true);
        y += 46.0;
        let kr = (fl.x, y, fl.w, 34.0);
        let act = Act::Ed(EdAct::KeepEditing);
        ghost(f, kr, 7.0, act, false, true);
        f.p.text("Continua a modificare", kr.0, kr.1, kr.2, kr.3, 13.0, BOLD, e.acc, TA::Center);
        return;
    }
    // «Opzioni di salvataggio ⌄»
    let or = (fl.x - 8.0, y, fl.w + 16.0, 34.0);
    let oact = Act::Ed(EdAct::Options);
    ghost(f, or, 7.0, oact, false, true);
    f.p.text("Opzioni di salvataggio", fl.x, or.1, fl.w - 30.0, or.3, 13.5, BOLD, WHITE, TA::Left);
    let turn = if ed.options_open { 180.0 } else { 0.0 };
    f.p.icon_turned(ic::CHEVRON_DOWN, fl.x + fl.w - 18.0, or.1 + 8.0, 18.0, (0xD5, 0xCC, 0xE2), turn);
    y += 40.0;
    if ed.options_open {
        y = toggle_row(f, e, &fl, y, "Appiattisci commenti e campi", "diventano parte della pagina", ed.flatten, EdAct::Flatten);
        y = toggle_row(f, e, &fl, y, "Togli autore e dati nascosti", "programma, XMP, allegati", ed.strip, EdAct::Strip);
        y += 6.0;
    }
    // Dove va e cosa succede se c'è già: due elenchi affiancati, con il nome sopra.
    let (a, b) = two(&fl, y + 20.0, 36.0);
    f.p.text("Cartella di destinazione", a.0, y, a.2 + 4.0, 18.0, 11.5, SEMI, e.muted, TA::Left);
    f.p.text("Se esiste già", b.0, y, b.2, 18.0, 11.5, SEMI, e.muted, TA::Left);
    crate::ui::views::dest_select(f, app, a, 12.5);
    crate::ui::views::exists_select(f, app, b, 12.5);
    y += 20.0 + 36.0 + 12.0;
    // Aree segnate ma non ancora oscurate: nel file resterebbero solo segnate, il testo sotto si legge ancora.
    let pend = ed.pending_redacts();
    if pend > 0 {
        let msg = if pend == 1 { "1 area è solo segnata: il testo sotto resta leggibile finché non applichi l'oscuramento.".to_string() } else { format!("{pend} aree sono solo segnate: il testo sotto resta leggibile finché non applichi l'oscuramento.") };
        y = note(f, &fl, y - 4.0, &msg, 12.0, AMBER) - 4.0;
    }
    let n = ed.unsaved();
    let label = if ed.saving {
        "Salvo il PDF…".to_string()
    } else if n > 0 {
        format!("Salva il PDF ({n})")
    } else {
        "Salva il PDF".to_string()
    };
    let sr = (fl.x, y, fl.w, 46.0);
    solid(f, e, sr, &label, 15.0, Act::Ed(EdAct::Save), !ed.saving && ed.opened.is_some());
    if ed.saving {
        f.spinner(sr.0 + 22.0, sr.1 + sr.3 / 2.0, 8.0, e.on_acc);
        f.animating = true;
    }
    y += 54.0;
    let out = format!("Esce come {}", crate::i18n::lit(&ed.out_name()));
    note(f, &fl, y, &out, 12.0, e.muted);
}
