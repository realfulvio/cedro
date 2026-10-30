//! «Rivedi»: la trascrizione da ascoltare e correggere. In alto l'onda dell'audio con i punti da
//! controllare, sotto le frasi con il loro tempo (un clic le fa sentire), a destra i difetti di whisper
//! trovati e il salvataggio. Lo stato e quel che fa stanno in `ui/review.rs`.

use super::*;
use crate::revisione::{Flag, Kind};
use crate::ui::core::HitKind;
use crate::ui::preview::clock;
use crate::ui::review::{Review, FORMATS, ROW_H};

const INSP_W: f32 = 410.0;

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Repeat => "Ripetuta",
        Kind::Loop => "Giro di parole",
        Kind::Typical => "Frase tipica",
        Kind::Silence => "Nel silenzio",
    }
}

fn kind_note(flag: &Flag) -> String {
    match flag.kind {
        Kind::Repeat => format!("La stessa frase {} volte di fila: di solito è whisper che si incanta.", flag.count),
        Kind::Loop => "Un giro di parole che si ripete dentro la frase.".to_string(),
        Kind::Typical => "Una frase che whisper inventa quando non sente parlato.".to_string(),
        Kind::Silence => "Testo dove l'audio è quasi muto: forse non c'è parlato.".to_string(),
    }
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
    let count = if rv.flags.is_empty() { format!("{} frasi · niente da controllare", rv.rows.len()) } else { format!("{} frasi · {} da controllare", rv.rows.len(), open) };
    f.text_r(&count, (x0 + w - 380.0, y0, 380.0, 44.0), 16.0, SANS, pal.muted);
    super::notice(f, app, (x0 + w - 640.0, y0 + 52.0, 640.0, 40.0), 15.0);

    // L'onda.
    let wy = y0 + 56.0;
    wave(f, app, rv, (x0, wy, w, 150.0));
    controls(f, app, rv, (x0, wy + 160.0, w, 46.0));

    let top = wy + 222.0;
    let bottom = f.h - 24.0;
    let list_w = w - INSP_W - 20.0;
    list(f, app, rv, (x0, top, list_w, bottom - top));
    inspector(f, app, rv, (x0 + list_w + 20.0, top, INSP_W, bottom - top));
}

fn wave(f: &mut Frame, app: &App, rv: &Review, r: R) {
    let pal = f.pal;
    f.card(r, 12.0);
    let (x, y, w, h) = (r.0 + 16.0, r.1 + 12.0, r.2 - 32.0, r.3 - 24.0);
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
                let col = if (c as f32 + 0.5) / cols as f32 <= frac { pal.primary } else { mix(pal.primary, pal.surface, 0.55) };
                f.p.fill(cx, y + (h - bh) / 2.0, step - 1.0, bh, col);
            }
        }
        _ => {
            f.spinner(r.0 + r.2 / 2.0 - 90.0, r.1 + r.3 / 2.0, 10.0, pal.primary);
            f.text("Leggo l'onda dell'audio…", r.0 + r.2 / 2.0 - 66.0, r.1, 300.0, r.3, 16.0, SANS, pal.muted);
        }
    }
    // La testina.
    let px = x + w * frac;
    f.p.fill(px - 1.0, y - 4.0, 2.0, h + 8.0, pal.text);
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
            // Il testo.
            let flag = rv.flag_of(i);
            let tx = chip.0 + chip.2 + 16.0;
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
                f.text_c(if open { kind_label(fl.kind) } else { "Sistemato" }, pr, 13.5, SEMI, col);
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

fn inspector(f: &mut Frame, app: &App, rv: &Review, r: R) {
    let _ = app;
    let pal = f.pal;
    f.card(r, 12.0);
    let (cx, cw) = (r.0 + 20.0, r.2 - 40.0);
    let mut y = r.1 + 16.0;
    f.text("Da controllare", cx, y, cw - 80.0, 30.0, 19.0, SEMI, pal.text);
    f.text_r(&rv.open_count().to_string(), (cx + cw - 80.0, y, 80.0, 30.0), 17.0, SEMI, if rv.open_count() > 0 { pal.bad } else { pal.ok });
    y += 38.0;
    let bw = (cw - 20.0) / 3.0;
    f.button((cx, y, bw, 38.0), "Prima", Some(ic::ARROW_LEFT), 14.5, Act::ReviewStep(-1));
    f.button((cx + bw + 10.0, y, bw, 38.0), "Dopo", Some(ic::ARROW_RIGHT), 14.5, Act::ReviewStep(1));
    f.button((cx + 2.0 * (bw + 10.0), y, bw, 38.0), "Ripristina", None, 14.5, if rv.can_undo() { Act::ReviewUndo } else { Act::Nothing });
    y += 48.0;
    let fixable = rv.flags.iter().enumerate().any(|(i, fl)| matches!(fl.kind, Kind::Repeat | Kind::Loop) && rv.is_open(i));
    f.primary((cx, y, cw, 42.0), "Sistema tutte le ripetizioni", Some(ic::CHECK), 15.5, Act::ReviewFixAll, fixable);
    y += 54.0;
    // Salvataggio in fondo; sopra, l'elenco.
    let save_h = 188.0;
    let list_r = (r.0 + 1.0, y, r.2 - 2.0, r.1 + r.3 - save_h - y);
    f.hline(r.0, list_r.1, r.2);
    if rv.flags.is_empty() {
        f.para("Whisper qui non ha lasciato ripetizioni né frasi inventate: si può salvare così.", cx, list_r.1 + 16.0, cw, 15.5, SANS, pal.muted, 1.4, 4);
    }
    let ch = 118.0;
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
            f.para(&kind_note(fl), cx + 20.0, fy + 38.0, cw - 20.0, 13.5, SANS, pal.muted, 1.35, 2);
            f.hit((list_r.0, fy, list_r.2, ch - 44.0), Act::ReviewFlag(i));
            let by = fy + ch - 44.0;
            if open {
                f.primary((cx + 20.0, by, 120.0, 34.0), "Risolvi", None, 14.5, Act::ReviewFix(i), true);
                f.button((cx + 150.0, by, 130.0, 34.0), "Lascia così", None, 14.5, Act::ReviewDismiss(i));
            } else {
                f.text(if rv.dismissed.contains(&i) { "Lasciata com'era" } else { "Sistemata" }, cx + 20.0, by, cw - 20.0, 34.0, 14.5, SANS, col);
            }
        }
        rv.flags.len() as f32 * ch
    });
    // Salva come.
    let sy = r.1 + r.3 - save_h;
    f.hline(r.0, sy, r.2);
    f.text("Salva come", cx, sy + 10.0, cw, 26.0, 15.5, SANS, pal.text);
    let n = FORMATS.len();
    let fw = (cw - (n as f32 - 1.0) * 8.0) / n as f32;
    for (i, (label, _)) in FORMATS.iter().enumerate() {
        let on = rv.format == i;
        let br = (cx + i as f32 * (fw + 8.0), sy + 40.0, fw, 38.0);
        let act = Act::ReviewFormat(i);
        let bg = if on { pal.primary } else if f.hovered(&act) { pal.alt } else { pal.surface };
        f.p.rrect(br.0, br.1, br.2, br.3, 8.0, Some(bg), Some((if on { pal.primary } else { pal.border }, 1.0)));
        f.text_c(label, br, 14.5, SEMI, if on { pal.on_primary } else { pal.text });
        f.focus_ring(br, 8.0, &act);
        f.hit(br, act);
    }
    f.primary((cx, sy + 90.0, cw, 46.0), "Salva", Some(ic::DOWNLOAD), 16.5, Act::ReviewSave, true);
    match &rv.saved {
        Some(p) if !rv.note.is_empty() => {
            f.text(&rv.note, cx, sy + 142.0, cw - 90.0, 34.0, 13.5, SANS, pal.ok);
            f.button((cx + cw - 84.0, sy + 142.0, 84.0, 34.0), "Apri", None, 13.5, Act::OpenPath(p.clone()));
        }
        _ => {
            f.text("Il file nuovo sta accanto all'originale.", cx, sy + 142.0, cw, 34.0, 13.5, SANS, pal.muted);
        }
    }
}
