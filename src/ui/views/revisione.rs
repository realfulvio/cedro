//! «Rivedi»: la trascrizione da ascoltare e correggere. In alto l'onda dell'audio con i punti da
//! controllare, sotto le frasi con il loro tempo (un clic le fa sentire), a destra i difetti di whisper
//! trovati e il salvataggio. Con le voci riconosciute c'è anche chi parla: la striscia colorata sotto
//! l'onda, il nome davanti a ogni frase e la scheda «Chi parla» per dare i nomi e unire le voci.
//! Lo stato e quel che fa stanno in `ui/review.rs`.

use super::*;
use crate::revisione::{Flag, Kind};
use crate::ui::core::HitKind;
use crate::ui::gfx::Rgb;
use crate::ui::preview::clock;
use crate::ui::review::{Review, FORMATS, ROW_H};

const INSP_W: f32 = 410.0;
/// L'altezza della striscia di chi parla, con gli orari sotto.
const LANE_H: f32 = 40.0;

/// Il chip di una voce: fondo chiaro della voce, testo scuro (o acceso nei temi scuri).
pub(super) fn chip_colors(f: &Frame, v: usize) -> (Rgb, Rgb) {
    let c = f.voice(v);
    let dark = f.theme.dark();
    (mix(c, f.pal.surface, if dark { 0.78 } else { 0.86 }), if dark { c } else { mix(c, (0, 0, 0), 0.25) })
}

/// Il colore della voce che parla a `centis` (quello del tema se non si sa).
fn voice_at(f: &Frame, rv: &Review, centis: i64) -> Option<Rgb> {
    let next = rv.rows.partition_point(|r| r.t0 <= centis);
    let row = next.checked_sub(1).map(|i| &rv.rows[i])?;
    (centis <= row.t1 && !row.removed).then_some(row.speaker?).map(|v| f.voice(v as usize))
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Repeat => "Ripetuta",
        Kind::Loop => "Giro di parole",
        Kind::Typical => "Frase tipica",
        Kind::Silence => "Nel silenzio",
        Kind::Number => "Numeri e misure",
        Kind::Meaning => "Parola decisiva",
    }
}

fn kind_note(flag: &Flag) -> String {
    match flag.kind {
        Kind::Repeat => format!("La stessa frase {} volte di fila: di solito è whisper che si incanta.", flag.count),
        Kind::Loop => "Un giro di parole che si ripete dentro la frase.".to_string(),
        Kind::Typical => "Una frase che whisper inventa quando non sente parlato.".to_string(),
        Kind::Silence => "Testo dove l'audio è quasi muto: forse non c'è parlato.".to_string(),
        Kind::Number => format!("Riascolta e conferma {} in «{}»", quoted(&flag.words), flag.snippet),
        Kind::Meaning => format!("Cambia il senso se sentita male: {} in «{}»", quoted(&flag.words), flag.snippet),
    }
}

fn quoted(words: &[String]) -> String {
    words.iter().take(6).map(|w| format!("«{w}»")).collect::<Vec<_>>().join(", ")
}

/// Il triangolo del Play o le due stanghette della pausa.
fn play_glyph(f: &mut Frame, cx: f32, cy: f32, r: f32, c: crate::ui::gfx::Rgb, pause: bool) {
    if pause {
        f.p.fill(cx - r * 0.7, cy - r * 0.8, r * 0.5, r * 1.6, c);
        f.p.fill(cx + r * 0.2, cy - r * 0.8, r * 0.5, r * 1.6, c);
    } else {
        f.p.poly(&[(cx - r * 0.6, cy - r), (cx + r, cy), (cx - r * 0.6, cy + r)], c, 1.0);
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let Some(rv) = app.review.as_ref() else { return };
    let pal = f.pal;
    f.p.clear(pal.bg);
    f.caption((0.0, 0.0, f.w, 56.0));
    f.brand(26.0, 15.0, 26.0, 18.0, 12.0, None);
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
    let (x0, w) = (28.0, f.w - 56.0);
    let y0 = 72.0;
    // Intestazione: il ritorno, il nome, il conto.
    f.button((x0, y0, 250.0, 44.0), "Torna alle trascrizioni", Some(ic::ARROW_LEFT), 16.0, Act::ReviewClose);
    let name = rv.source.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    f.text(&name, x0 + 270.0, y0, w - 270.0 - 380.0, 44.0, 24.0, SEMI, pal.text);
    let open = rv.open_count();
    let mut count = format!("{} frasi", rv.rows.len());
    let voices = rv.live_voices().len();
    if voices >= 2 {
        count.push_str(&format!(" · {voices} interlocutori"));
    }
    if rv.flags.is_empty() {
        count.push_str(" · niente da controllare");
    } else {
        count.push_str(&format!(" · {open} da controllare"));
    }
    f.text_r(&count, (x0 + w - 380.0, y0, 380.0, 44.0), 16.0, SANS, pal.muted);
    super::notice(f, app, (x0 + w - 640.0, y0 + 52.0, 640.0, 40.0), 15.0);

    // L'onda.
    let wy = y0 + 56.0;
    let lane = if rv.names.is_empty() { 0.0 } else { LANE_H };
    wave(f, app, rv, (x0, wy, w, 150.0 + lane));
    controls(f, app, rv, (x0, wy + 160.0 + lane, w, 46.0));

    let top = wy + 222.0 + lane;
    let bottom = f.h - 24.0;
    let list_w = w - INSP_W - 20.0;
    list(f, app, rv, (x0, top, list_w, bottom - top));
    inspector(f, app, rv, (x0 + list_w + 20.0, top, INSP_W, bottom - top));
}

fn wave(f: &mut Frame, app: &App, rv: &Review, r: R) {
    let pal = f.pal;
    f.card(r, 12.0);
    let lane = if rv.names.is_empty() { 0.0 } else { LANE_H };
    let (x, y, w, h) = (r.0 + 16.0, r.1 + 12.0, r.2 - 32.0, r.3 - 24.0 - lane);
    let secs = rv.secs.max(0.1);
    let pos = app.review_position();
    let frac = (pos / secs).clamp(0.0, 1.0) as f32;
    let at = |centis: i64| x + w * ((centis as f64 / 100.0 / secs).clamp(0.0, 1.0) as f32);
    // I punti da controllare ancora aperti, dietro l'onda.
    for (i, fl) in rv.flags.iter().enumerate() {
        if !rv.is_open(i) {
            continue;
        }
        let (Some(a), Some(b)) = (fl.segs.first().and_then(|s| rv.rows.get(*s)), fl.segs.last().and_then(|s| rv.rows.get(*s))) else { continue };
        let (xa, xb) = (at(a.t0), at(b.t1).max(at(a.t0) + 4.0));
        f.p.rrect_a(xa, y, xb - xa, h, 3.0, pal.bad, if i == rv.cur { 0.28 } else { 0.14 });
    }
    match &rv.bars {
        Some(bars) if !bars.is_empty() => {
            let step = 3.0;
            let cols = (w / step).floor().max(1.0) as usize;
            for c in 0..cols {
                let a = c * bars.len() / cols;
                let b = ((c + 1) * bars.len() / cols).max(a + 1).min(bars.len());
                let v = bars[a..b].iter().cloned().fold(0.0f32, f32::max).clamp(0.0, 1.0);
                let bh = (v * h).max(2.0);
                let cx = x + c as f32 * step;
                let at = (c as f64 + 0.5) / cols as f64 * secs * 100.0;
                let base = voice_at(f, rv, at as i64).unwrap_or(pal.primary);
                let col = if (c as f32 + 0.5) / cols as f32 <= frac { base } else { mix(base, pal.surface, 0.55) };
                f.p.fill(cx, y + (h - bh) / 2.0, step - 1.0, bh, col);
            }
        }
        _ if rv.wave_failed => {}
        _ => {
            f.spinner(r.0 + r.2 / 2.0 - 90.0, r.1 + r.3 / 2.0, 10.0, pal.primary);
            f.text("Leggo l'onda dell'audio…", r.0 + r.2 / 2.0 - 66.0, r.1, 300.0, r.3, 16.0, SANS, pal.muted);
        }
    }
    // La striscia di chi parla, con gli orari sotto.
    if lane > 0.0 {
        let ly = y + h + 10.0;
        f.p.rrect(x, ly, w, 14.0, 4.0, Some(pal.alt), None);
        for row in rv.rows.iter().filter(|r| !r.removed) {
            let Some(v) = row.speaker else { continue };
            let (a, b) = (at(row.t0), at(row.t1));
            f.p.fill(a, ly, (b - a - 1.0).max(1.0), 14.0, f.voice(v as usize));
        }
        for k in 0..=4 {
            let t = clock(secs * k as f64 / 4.0);
            let lx = x + w * k as f32 / 4.0;
            let tr = match k {
                0 => (lx, ly + 16.0, 80.0, 16.0),
                4 => (lx - 80.0, ly + 16.0, 80.0, 16.0),
                _ => (lx - 40.0, ly + 16.0, 80.0, 16.0),
            };
            let align = match k {
                0 => Align::Left,
                4 => Align::Right,
                _ => Align::Center,
            };
            f.p.text(&t, tr.0, tr.1, tr.2, tr.3, 12.0, SANS, pal.muted, align);
        }
    }
    // La testina.
    let px = x + w * frac;
    f.p.fill(px - 1.0, y - 4.0, 2.0, h + 8.0 + if lane > 0.0 { 16.0 } else { 0.0 }, pal.text);
    f.hit_kind((x - 10.0, r.1, w + 20.0, r.3), Act::ReviewSeek, HitKind::Slider);
}

fn controls(f: &mut Frame, app: &App, rv: &Review, r: R) {
    let pal = f.pal;
    let cy = r.1 + r.3 / 2.0;
    let playing = app.review_playing();
    let act = Act::ReviewPlay;
    if f.hovered(&act) {
        f.p.circle(r.0 + 24.0, cy, 27.0, Some(mix(pal.primary, pal.bg, 0.8)), None);
    }
    f.p.circle(r.0 + 24.0, cy, 23.0, Some(pal.primary), None);
    if playing && app.review_loading() {
        f.spinner(r.0 + 24.0, cy, 10.0, pal.on_primary);
    } else {
        play_glyph(f, r.0 + 24.0, cy, 9.0, pal.on_primary, playing);
    }
    f.focus_ring((r.0, cy - 24.0, 48.0, 48.0), 24.0, &act);
    f.hit((r.0 - 4.0, cy - 28.0, 56.0, 56.0), act);
    let mut x = r.0 + 64.0;
    x += f.tool(x, cy - 20.0, 40.0, "Indietro di 10 secondi", ic::ARROW_LEFT, 15.0, Act::ReviewSkip(-10), true) + 6.0;
    x += f.tool(x, cy - 20.0, 40.0, "Avanti di 10 secondi", ic::ARROW_RIGHT, 15.0, Act::ReviewSkip(10), true) + 14.0;
    let time = format!("{} / {}", clock(app.review_position()), clock(rv.secs));
    f.text(&time, x, r.1, 200.0, r.3, 17.0, SEMI, pal.text);
    // A destra: Segui, Solo da controllare.
    let mut rx = r.0 + r.2;
    let sw = |f: &mut Frame, rx: &mut f32, label: &str, on: bool, act: Act| {
        let tw = f.measure(label, 15.0, SANS);
        *rx -= 44.0;
        f.switch(*rx, cy - 13.0, 44.0, 26.0, on, act, None);
        *rx -= tw + 12.0;
        f.text(label, *rx, r.1, tw + 4.0, r.3, 15.0, SANS, pal.text);
        *rx -= 26.0;
    };
    sw(f, &mut rx, "Solo da controllare", rv.only_flagged, Act::ReviewOnly);
    sw(f, &mut rx, "Segui l'audio", rv.follow, Act::ReviewFollow);
}

fn list(f: &mut Frame, app: &App, rv: &Review, r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(pal.surface), Some((pal.border, 1.0)));
    let edit_h = 96.0;
    let area = (r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, r.3 - edit_h - 2.0);
    let rows = rv.visible();
    let playing = app.review_playing().then(|| rv.row_at(app.review_position())).flatten();
    if rows.is_empty() {
        f.text_c("Nessuna frase da mostrare.", area, 17.0, SANS, pal.muted);
    }
    super::scrolled(f, area, Scroll::Review, |f| {
        for (n, &i) in rows.iter().enumerate() {
            let y = area.1 + n as f32 * ROW_H;
            if !f.p.shows_y(y, ROW_H) {
                continue;
            }
            let row = &rv.rows[i];
            let sel = rv.sel == Some(i);
            if playing == Some(i) {
                f.p.fill_a(area.0, y, area.2, ROW_H, pal.primary, 0.10);
            } else if sel {
                f.p.fill(area.0, y, area.2, ROW_H, pal.alt);
            }
            if n > 0 {
                f.hline(area.0, y, area.2);
            }
            // Il tempo: un clic fa sentire da qui.
            let chip = (area.0 + 14.0, y + (ROW_H - 32.0) / 2.0, 82.0, 32.0);
            let act = Act::ReviewRow(i);
            let hot = f.hovered(&act);
            f.p.rrect(chip.0, chip.1, chip.2, chip.3, 16.0, Some(if hot { pal.primary } else { mix(pal.primary, pal.surface, 0.86) }), None);
            f.text_c(&clock(row.t0 as f64 / 100.0), chip, 14.5, SEMI, if hot { pal.on_primary } else { pal.primary });
            f.hit(chip, act);
            // Chi parla: il nome nel suo colore (un clic lo cambia solo qui). Se è lo stesso della frase
            // sopra non si ripete.
            let mut tx = chip.0 + chip.2 + 16.0;
            if !rv.names.is_empty() {
                let prev = n.checked_sub(1).and_then(|k| rows.get(k)).and_then(|k| rv.rows.get(*k)).and_then(|p| p.speaker);
                if let Some(v) = row.speaker {
                    // Se è la stessa voce della frase sopra il nome non si ripete: resta un pallino, e il chip
                    // intero compare passandoci sopra o sulla frase scelta, così si può cambiare anche lì.
                    let (bg, fg) = chip_colors(f, v as usize);
                    let name = rv.name(v);
                    let cw = (f.measure(&name, 13.5, SEMI) + 34.0).min(180.0);
                    let cr = (tx, y + (ROW_H - 28.0) / 2.0, cw, 28.0);
                    let act = Act::ReviewSpeakerMenu(i);
                    let hot = f.hovered(&act);
                    if prev != Some(v) || hot || sel {
                        f.p.rrect(cr.0, cr.1, cr.2, cr.3, 14.0, Some(if hot { mix(bg, fg, 0.12) } else { bg }), None);
                        f.p.circle(cr.0 + 13.0, cr.1 + 14.0, 4.5, Some(f.voice(v as usize)), None);
                        f.p.text(&crate::i18n::lit(&name), cr.0 + 24.0, cr.1, cr.2 - 32.0, cr.3, 13.5, SEMI, fg, Align::Left);
                    } else {
                        f.p.circle(cr.0 + 13.0, cr.1 + 14.0, 4.5, Some(mix(f.voice(v as usize), pal.surface, 0.45)), None);
                    }
                    f.focus_ring(cr, 14.0, &act);
                    f.hit(cr, act);
                } else if row.speaker.is_none() {
                    // Una frase senza voce (nessun turno la copriva): un chip vuoto per assegnarla.
                    let cr = (tx, y + (ROW_H - 28.0) / 2.0, 96.0, 28.0);
                    let act = Act::ReviewSpeakerMenu(i);
                    f.p.rrect_line(cr.0, cr.1, cr.2, cr.3, 14.0, pal.muted, 1.2, 1.0, Some((4.0, 3.0)));
                    f.text_c("Chi parla?", cr, 13.0, SANS, pal.muted);
                    f.hit(cr, act);
                }
                tx += 190.0;
            }
            // Il testo.
            let flag = rv.flag_of(i);
            let pill_w = if flag.is_some() { 132.0 } else { 0.0 };
            let tw = area.0 + area.2 - tx - 56.0 - pill_w - 12.0;
            let c = if row.removed { pal.muted } else { pal.text };
            let shown = if row.removed { format!("(tolta) {}", row.text) } else { row.text.clone() };
            let one = f.measure(&shown, 16.5, SANS) <= tw;
            f.para(&shown, tx, y + if one { (ROW_H - 22.0) / 2.0 } else { 12.0 }, tw, 16.5, SANS, c, 1.35, 2);
            f.hit((tx, y, tw, ROW_H), Act::ReviewPick(i));
            if let Some(fi) = flag {
                let fl = &rv.flags[fi];
                let open = rv.is_open(fi);
                let col = if open { pal.bad } else { pal.ok };
                let pr = (area.0 + area.2 - 56.0 - pill_w, y + (ROW_H - 28.0) / 2.0, pill_w, 28.0);
                f.p.rrect(pr.0, pr.1, pr.2, pr.3, 14.0, Some(mix(col, pal.surface, 0.88)), Some((mix(col, pal.surface, 0.5), 1.0)));
                let chiuso = if matches!(fl.kind, Kind::Number | Kind::Meaning) { "Controllato" } else { "Sistemato" };
                f.text_c(if open { kind_label(fl.kind) } else { chiuso }, pr, 13.5, SEMI, col);
                f.hit(pr, Act::ReviewFlag(fi));
            }
            // Togli (o rimetti) la frase.
            let del = Act::ReviewDelete(i);
            let dr = (area.0 + area.2 - 44.0, y + (ROW_H - 32.0) / 2.0, 32.0, 32.0);
            if f.hovered(&del) {
                f.p.circle(dr.0 + 16.0, dr.1 + 16.0, 17.0, Some(pal.alt), None);
            }
            f.icon(if row.removed { ic::ADD } else { ic::TRASH }, dr.0 + 6.0, dr.1 + 6.0, 20.0, pal.muted);
            f.hit(dr, del);
        }
        rows.len() as f32 * ROW_H
    });
    // La frase da correggere.
    let ey = r.1 + r.3 - edit_h;
    f.hline(r.0, ey, r.2);
    match rv.sel.filter(|i| *i < rv.rows.len()) {
        Some(i) => {
            f.text(&format!("Correggi la frase delle {}", clock(rv.rows[i].t0 as f64 / 100.0)), r.0 + 18.0, ey + 8.0, r.2 - 36.0, 28.0, 15.0, SANS, pal.muted);
            f.field((r.0 + 14.0, ey + 38.0, r.2 - 28.0, 46.0), Field::Review(i), &rv.rows[i].text, "", 17.0, false);
        }
        None => f.text("Scegli una frase per correggerla; il tempo a sinistra la fa sentire.", r.0 + 18.0, ey, r.2 - 36.0, edit_h, 16.0, SANS, pal.muted),
    }
}

/// Il Riassunto AI c'è sui PC (sul telefono il modello sarebbe troppo pesante).
const SUMMARY: bool = !cfg!(target_os = "android");

fn inspector(f: &mut Frame, app: &App, rv: &Review, r: R) {
    f.card(r, 12.0);
    let (cx, cw) = (r.0 + 20.0, r.2 - 40.0);
    let mut y = r.1 + 16.0;
    let voices = !rv.names.is_empty();
    // Le schede: «Da controllare», «Chi parla» (con le voci) e «Riassunto».
    let list: Vec<u8> = [0u8, 1, 2].into_iter().filter(|t| (*t != 1 || voices) && (*t != 2 || SUMMARY)).collect();
    if list.len() > 1 {
        tabs(f, rv, &list, (cx - 2.0, r.1 + 14.0, cw + 4.0, 38.0));
        y = r.1 + 14.0 + 38.0 + 14.0;
    }
    // Salvataggio in fondo (con le voci, anche l'interruttore per scriverle; poi quello del riassunto).
    let save_h = 188.0 + if voices { 40.0 } else { 0.0 } + if SUMMARY { 40.0 } else { 0.0 };
    let bottom = r.1 + r.3 - save_h;
    match rv.tab {
        1 if voices => voice_cards(f, rv, (r.0 + 1.0, y, r.2 - 2.0, bottom - y)),
        2 if SUMMARY => super::riassunto::pane(f, app, rv, (r.0 + 1.0, y, r.2 - 2.0, bottom - y)),
        _ => checks(f, rv, r, y, bottom, list.len() == 1),
    }
    save_box(f, rv, (r.0, bottom, r.2, save_h), voices);
}

/// Le schede in alto a destra: «Da controllare» (col numero dei punti aperti), «Chi parla», «Riassunto».
fn tabs(f: &mut Frame, rv: &Review, list: &[u8], r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let part = r.2 / list.len() as f32;
    let size = if list.len() > 2 { 14.0 } else { 14.5 };
    for (k, &t) in list.iter().enumerate() {
        let tr = (r.0 + k as f32 * part, r.1, part, r.3);
        let act = Act::ReviewTab(t);
        let on = rv.tab == t;
        if on {
            f.p.rrect(tr.0, tr.1, tr.2, tr.3, 8.0, Some(pal.primary), None);
        } else if f.hovered(&act) {
            f.p.rrect(tr.0 + 1.0, tr.1 + 1.0, tr.2 - 2.0, tr.3 - 2.0, 7.0, Some(pal.alt), None);
        }
        let fg = if on { pal.on_primary } else { pal.text };
        let label = match t {
            0 => "Da controllare",
            1 => "Chi parla",
            _ => "Riassunto",
        };
        let open = rv.open_count();
        let badge = (t == 0 && open > 0).then(|| open.to_string());
        let tw = f.measure(label, size, SEMI);
        let bw = badge.as_ref().map_or(0.0, |b| f.measure(b, 12.0, SEMI) + 14.0);
        let total = tw + if badge.is_some() { bw + 8.0 } else { 0.0 };
        let x = tr.0 + (tr.2 - total) / 2.0;
        f.p.text(label, x, tr.1, tw + 2.0, tr.3, size, SEMI, fg, Align::Left);
        if let Some(b) = badge {
            let br = (x + tw + 8.0, tr.1 + (tr.3 - 18.0) / 2.0, bw, 18.0);
            f.p.rrect(br.0, br.1, br.2, br.3, 9.0, Some(pal.bad), None);
            f.text_c(&b, br, 12.0, SEMI, (255, 255, 255));
        }
        f.focus_ring(tr, 8.0, &act);
        f.hit(tr, act);
    }
}

/// «Da controllare»: i difetti di whisper trovati, da risolvere o lasciare.
fn checks(f: &mut Frame, rv: &Review, r: R, mut y: f32, bottom: f32, title: bool) {
    let pal = f.pal;
    let (cx, cw) = (r.0 + 20.0, r.2 - 40.0);
    if title {
        f.text("Da controllare", cx, y, cw - 80.0, 30.0, 19.0, SEMI, pal.text);
        f.text_r(&rv.open_count().to_string(), (cx + cw - 80.0, y, 80.0, 30.0), 17.0, SEMI, if rv.open_count() > 0 { pal.bad } else { pal.ok });
        y += 38.0;
    }
    let bw = (cw - 20.0) / 3.0;
    f.button((cx, y, bw, 38.0), "Prima", Some(ic::ARROW_LEFT), 14.5, Act::ReviewStep(-1));
    f.button((cx + bw + 10.0, y, bw, 38.0), "Dopo", Some(ic::ARROW_RIGHT), 14.5, Act::ReviewStep(1));
    f.button((cx + 2.0 * (bw + 10.0), y, bw, 38.0), "Ripristina", None, 14.5, if rv.can_undo() { Act::ReviewUndo } else { Act::Nothing });
    y += 48.0;
    let fixable = rv.flags.iter().enumerate().any(|(i, fl)| matches!(fl.kind, Kind::Repeat | Kind::Loop) && rv.is_open(i));
    f.primary((cx, y, cw, 42.0), "Sistema tutte le ripetizioni", Some(ic::CHECK), 15.5, Act::ReviewFixAll, fixable);
    y += 54.0;
    let list_r = (r.0 + 1.0, y, r.2 - 2.0, (bottom - y).max(0.0));
    f.hline(r.0, list_r.1, r.2);
    if rv.flags.is_empty() {
        f.para("Whisper qui non ha lasciato ripetizioni né frasi inventate: si può salvare così.", cx, list_r.1 + 16.0, cw, 15.5, SANS, pal.muted, 1.4, 4);
    }
    // I referti hanno la frase nella nota (fino a tre righe): la scheda è più alta.
    let referti = rv.flags.iter().any(|f| matches!(f.kind, Kind::Number | Kind::Meaning));
    let (ch, righe) = if referti { (150.0, 3) } else { (118.0, 2) };
    super::scrolled(f, list_r, Scroll::ReviewFlags, |f| {
        for (i, fl) in rv.flags.iter().enumerate() {
            let fy = list_r.1 + i as f32 * ch;
            if !f.p.shows_y(fy, ch) {
                continue;
            }
            let open = rv.is_open(i);
            let cur = rv.cur == i;
            if cur {
                f.p.fill_a(list_r.0, fy, list_r.2, ch, pal.primary, 0.08);
            }
            f.hline(list_r.0, fy, list_r.2);
            let col = if open { pal.bad } else { pal.ok };
            f.p.circle(cx + 6.0, fy + 22.0, 5.0, Some(col), None);
            let at = fl.segs.first().and_then(|s| rv.rows.get(*s)).map(|row| clock(row.t0 as f64 / 100.0)).unwrap_or_default();
            f.text(&format!("{} · {at}", kind_label(fl.kind)), cx + 20.0, fy + 8.0, cw - 20.0, 28.0, 15.5, SEMI, pal.text);
            // La nota sta tra il titolo e i pulsanti: le righe in più si tagliano, non finiscono sotto.
            f.para(&kind_note(fl), cx + 20.0, fy + 38.0, cw - 20.0, 13.5, SANS, pal.muted, 1.35, righe);
            f.hit((list_r.0, fy, list_r.2, ch - 44.0), Act::ReviewFlag(i));
            let by = fy + ch - 44.0;
            let referto = matches!(fl.kind, Kind::Number | Kind::Meaning);
            if open && referto {
                // Il referto non si «risolve» da solo: si riascolta, si corregge sul posto o si conferma.
                f.primary((cx + 20.0, by, 120.0, 34.0), "Confermo", Some(ic::CHECK), 14.5, Act::ReviewDismiss(i), true);
                f.button((cx + 150.0, by, 130.0, 34.0), "Ascolta", None, 14.5, Act::ReviewFlag(i));
            } else if open {
                f.primary((cx + 20.0, by, 120.0, 34.0), "Risolvi", None, 14.5, Act::ReviewFix(i), true);
                f.button((cx + 150.0, by, 130.0, 34.0), "Lascia così", None, 14.5, Act::ReviewDismiss(i));
            } else {
                let done = match (referto, rv.dismissed.contains(&i)) {
                    (true, true) => "Confermata",
                    (true, false) => "Corretta",
                    (false, true) => "Lasciata com'era",
                    (false, false) => "Sistemata",
                };
                f.text(done, cx + 20.0, by, cw - 20.0, 34.0, 14.5, SANS, col);
            }
        }
        rv.flags.len() as f32 * ch
    });
}

/// I minuti di parlato, come nel Word.
fn minutes(secs: f64) -> String {
    let m = (secs / 60.0).round() as u64;
    if m == 0 {
        "meno di 1 min".to_string()
    } else {
        format!("{m} min")
    }
}

/// «Chi parla»: una scheda per voce con il nome da scrivere, ▶ per sentirla e «Unisci a…».
fn voice_cards(f: &mut Frame, rv: &Review, r: R) {
    let pal = f.pal;
    let (cx, cw) = (r.0 + 18.0, r.2 - 36.0);
    let live = rv.live_voices();
    let card_h = 84.0;
    let note = "Il nome cambia in tutte le frasi. Per una frase sola: clic sul suo nome a sinistra.";
    super::scrolled(f, r, Scroll::ReviewVoices, |f| {
        let mut y = r.1;
        for &v in &live {
            if f.p.shows_y(y, card_h) {
                voice_card(f, rv, v, (cx, y, cw, card_h), live.len() > 1);
            }
            y += card_h + 10.0;
        }
        y += f.para(note, cx, y + 2.0, cw, 13.5, SANS, pal.muted, 1.4, 3) + 12.0;
        y - r.1
    });
}

fn voice_card(f: &mut Frame, rv: &Review, v: u8, r: R, can_merge: bool) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.surface), Some((pal.border, 1.0)));
    let color = f.voice(v as usize);
    let (x, y) = (r.0 + 12.0, r.1 + 10.0);
    f.p.rrect(x, y + 9.0, 14.0, 14.0, 4.0, Some(color), None);
    // Il nome: si scrive sul posto, come nell'anteprima (riga tratteggiata sotto).
    let nr = (x + 24.0, y, r.2 - 24.0 - 24.0 - 44.0, 32.0);
    name_field(f, rv, v, nr);
    // ▶: fa sentire la frase più lunga di questa voce.
    let act = Act::ReviewVoicePlay(v);
    let (pcx, pcy) = (r.0 + r.2 - 12.0 - 16.0, y + 16.0);
    f.p.circle(pcx, pcy, 16.0, Some(if f.hovered(&act) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
    f.p.poly(&[(pcx - 3.5, pcy - 6.0), (pcx + 6.0, pcy), (pcx - 3.5, pcy + 6.0)], pal.text, 1.0);
    f.focus_ring((pcx - 16.0, pcy - 16.0, 32.0, 32.0), 16.0, &act);
    f.hit((pcx - 18.0, pcy - 18.0, 36.0, 36.0), act);
    // Sotto: quante frasi, quanto ha parlato, «Unisci a…».
    let (n, secs) = rv.voice_stats(v);
    let sy = y + 40.0;
    let count = if n == 1 { "1 frase".to_string() } else { format!("{n} frasi") };
    f.text(&format!("{count} · {}", minutes(secs)), nr.0, sy, nr.2, 24.0, 13.0, SANS, pal.muted);
    if can_merge {
        let label = "Unisci a…";
        let tw = f.measure(label, 13.5, SEMI);
        let mr = (r.0 + r.2 - 12.0 - tw - 8.0, sy - 2.0, tw + 8.0, 28.0);
        let act = Act::ReviewMergeMenu(v);
        if f.hovered(&act) {
            f.p.rrect(mr.0, mr.1, mr.2, mr.3, 6.0, Some(pal.alt), None);
        }
        f.p.text(label, mr.0 + 4.0, mr.1, tw + 2.0, mr.3, 13.5, SEMI, pal.primary, Align::Left);
        f.focus_ring(mr, 6.0, &act);
        f.hit(mr, act);
    }
}

/// Il campo del nome di una voce: testo in grassetto con una riga tratteggiata sotto (piena mentre si scrive).
fn name_field(f: &mut Frame, rv: &Review, v: u8, r: R) {
    let pal = f.pal;
    let field = Field::VoiceName(v);
    let editing = f.ui.edit == Some(field);
    let text = rv.names.get(v as usize).cloned().unwrap_or_default();
    let shown = if text.trim().is_empty() && !editing { rv.name(v) } else { text };
    let size = 15.5;
    if editing {
        f.p.fill(r.0, r.1 + r.3 - 2.0, r.2, 1.5, pal.primary);
    } else {
        let mut dx = r.0;
        while dx < r.0 + r.2 {
            f.p.fill(dx, r.1 + r.3 - 2.0, 4.0f32.min(r.0 + r.2 - dx), 1.5, pal.border);
            dx += 7.0;
        }
    }
    f.p.push_clip(r.0, r.1, r.2, r.3);
    if editing && f.ui.sel_all && !shown.is_empty() {
        let tw = f.measure(&shown, size, SEMI).min(r.2);
        f.p.rrect_a(r.0 - 1.0, r.1 + r.3 * 0.18, tw + 2.0, r.3 * 0.6, 3.0, pal.primary, 0.35);
    }
    let caret_text: String = shown.chars().take(f.ui.caret.min(shown.chars().count())).collect();
    let caret_x = f.measure(&caret_text, size, SEMI);
    let shift = if editing && caret_x > r.2 - 4.0 { caret_x - (r.2 - 4.0) } else { 0.0 };
    let (asc, desc) = f.p.fonts().extents(crate::ui::gfx::text_size(size) * f.p.scale, SEMI);
    let base = r.1 + (r.3 - (asc + desc) / f.p.scale) / 2.0 + asc / f.p.scale;
    f.p.text_at(&shown, r.0 - shift, base, size, SEMI, pal.text);
    if editing && (f.ms / 530).is_multiple_of(2) {
        f.p.rrect_a(r.0 - shift + caret_x, r.1 + r.3 * 0.2, 1.5, r.3 * 0.56, 0.0, pal.text, 1.0);
    }
    f.p.pop_clip();
    let act = Act::Edit(field);
    f.hit_kind(r, act, HitKind::Text);
}

/// In fondo: gli interruttori «Scrivi chi parla nel file» (con le voci) e «Metti il riassunto in cima al
/// file» (spento e sbiadito finché il riassunto non c'è), il formato e Salva.
fn save_box(f: &mut Frame, rv: &Review, r: R, voices: bool) {
    let pal = f.pal;
    let (cx, cw) = (r.0 + 20.0, r.2 - 40.0);
    f.hline(r.0, r.1, r.2);
    let mut sy = r.1 + 10.0;
    if voices {
        f.switch(cx, sy + 4.0, 44.0, 24.0, rv.write_voices, Act::ReviewWriteVoices, None);
        f.text("Scrivi chi parla nel file", cx + 56.0, sy, cw - 56.0, 32.0, 14.5, SANS, pal.text);
        sy += 40.0;
    }
    if SUMMARY {
        let ready = rv.summary.is_some();
        if ready {
            f.switch(cx, sy + 4.0, 44.0, 24.0, rv.summary_on_top, Act::SummaryOnTop, None);
        } else {
            // Come nell'anteprima: la levetta spenta e sbiadita, che non si può toccare.
            f.p.rrect(cx, sy + 4.0, 44.0, 24.0, 12.0, Some(mix(pal.border, pal.surface, 0.5)), None);
            f.p.circle(cx + 12.0, sy + 16.0, 9.0, Some(pal.surface), None);
        }
        f.text("Metti il riassunto in cima al file", cx + 56.0, sy, cw - 56.0, 32.0, 14.5, SANS, if ready { pal.text } else { mix(pal.muted, pal.surface, 0.35) });
        sy += 40.0;
    }
    f.text("Salva come", cx, sy, cw, 26.0, 15.5, SANS, pal.text);
    let n = FORMATS.len();
    let fw = (cw - (n as f32 - 1.0) * 8.0) / n as f32;
    for (i, (label, _)) in FORMATS.iter().enumerate() {
        let on = rv.format == i;
        let br = (cx + i as f32 * (fw + 8.0), sy + 30.0, fw, 38.0);
        let act = Act::ReviewFormat(i);
        let bg = if on { pal.primary } else if f.hovered(&act) { pal.alt } else { pal.surface };
        f.p.rrect(br.0, br.1, br.2, br.3, 8.0, Some(bg), Some((if on { pal.primary } else { pal.border }, 1.0)));
        f.text_c(label, br, 14.5, SEMI, if on { pal.on_primary } else { pal.text });
        f.focus_ring(br, 8.0, &act);
        f.hit(br, act);
    }
    f.primary((cx, sy + 80.0, cw, 46.0), "Salva", Some(ic::DOWNLOAD), 16.5, Act::ReviewSave, true);
    match &rv.saved {
        Some(p) if !rv.note.is_empty() => {
            f.text(&rv.note, cx, sy + 132.0, cw - 90.0, 34.0, 13.5, SANS, pal.ok);
            f.button((cx + cw - 84.0, sy + 132.0, 84.0, 34.0), "Apri", None, 13.5, Act::OpenPath(p.clone()));
        }
        _ => {
            f.text("Il file nuovo sta accanto all'originale.", cx, sy + 132.0, cw, 34.0, 13.5, SANS, pal.muted);
        }
    }
}
