//! Unire e dividere i PDF: i tre pulsanti «Cosa vuoi fare», la striscia delle pagine con le forbici,
//! le pagine del file unito e l'ordine dei file. Le misure vengono dalla tavola A (essenziale);
//! gli altri temi usano gli stessi pezzi, più piccoli, nel loro pannello delle opzioni.

use super::*;
use crate::engine::PageSel;
use crate::ui::gfx::{Fit, Rgb};
use crate::ui::theme::Palette;
use crate::ui::model::{PdfMode, FileState};
use std::path::Path;

pub const MERGE: &str = "M6 3v5a4 4 0 0 0 4 4h4a4 4 0 0 1 4 4v5M18 3v5a4 4 0 0 1-4 4M15 18l3 3 3-3";
pub const SCISSORS: &str = "M4 6a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0-5 0zM4 18a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0-5 0zM8.2 7.6 20 18M8.2 16.4 20 6";
pub const GRIP: &str = "M9 6v.1M15 6v.1M9 12v.1M15 12v.1M9 18v.1M15 18v.1";
pub const UP: &str = "M6 15l6-6 6 6";
pub const PAGES: &str = "M7 3h10a1 1 0 0 1 1 1v13H6V4a1 1 0 0 1 1-1zM6 17v3a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1v-3M9 8h6M9 12h6";
pub const SHRINK: &str = "M4 9h5V4M20 9h-5V4M4 15h5v5M20 15h-5v5M9 9 3 3M15 9l6-6M9 15l-6 6M15 15l6 6";
pub const ROTATE_R: &str = "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7";
pub const ROTATE_L: &str = "M4 11a8 8 0 1 1 2.3 5.7M4 4v7h7";
pub const EXTRACT: &str = "M4 4h10v10H4zM10 10h10v10H10zM14 14l-3-3";

/// Quante pagine mostra al massimo la striscia di «Dividi» (oltre si scrivono gli intervalli).
pub const STRIP_MAX: usize = 60;
/// Quante pagine del file unito si disegnano al massimo.
pub const MERGED_MAX: usize = 48;
/// Quante pagine mostra al massimo il banco di «Pagine».
pub const BOARD_MAX: usize = 300;

/// Vero se la scheda PDF mostra i cinque pulsanti (Standard, Conversioni).
pub fn shows_modes(f: &Frame, app: &App) -> bool {
    !f.simple && app.activity == Activity::Convert && app.kind() == Kind::Pdf
}

/// L'icona del pulsante grande per quello che fa adesso la scheda.
pub fn go_icon(app: &App) -> &'static str {
    if app.activity != Activity::Convert || app.kind() != Kind::Pdf {
        return ic::CONVERT;
    }
    match app.pdf_mode() {
        PdfMode::Merge => MERGE,
        PdfMode::Split => SCISSORS,
        PdfMode::Pages => PAGES,
        PdfMode::Compress => SHRINK,
        PdfMode::Convert => ic::CONVERT,
    }
}

/// Il colore del file `n` della coda (il segno sulle sue pagine nel file unito).
pub fn file_color(pal: &Palette, n: usize) -> Rgb {
    [pal.primary, pal.ok, (0xB4, 0x63, 0x0F), (0x7B, 0x4F, 0xB3)][n % 4]
}

/// Le pagine (da 0) del file `i` che entrano nel file unito.
pub fn merged_pages(app: &App, i: usize, n: usize) -> Vec<usize> {
    let sel = app.files.get(i).and_then(|f| PageSel::parse(&f.pages).ok().flatten());
    (0..n).filter(|p| sel.as_ref().is_none_or(|s| s.contains(*p))).collect()
}

/// «Diventa le pagine 5–6»: dove finisce il file `n` della coda nel file unito.
pub fn merged_place(app: &App, targets: &[usize], i: usize) -> Option<String> {
    let mut start = 1;
    for &t in targets {
        let count = merged_pages(app, t, app.page_count(t)?).len();
        if t == i {
            return Some(match count {
                0 => "Nessuna pagina entra nel file unito".into(),
                1 => format!("Diventa la pagina {start}"),
                _ => format!("Diventa le pagine {start}–{}", start + count - 1),
            });
        }
        start += count;
    }
    None
}

/// Il file in evidenza per «Dividi»: quello scelto, altrimenti il primo della coda.
pub fn split_file(app: &App) -> Option<usize> {
    let vis = app.visible();
    app.selected.filter(|i| vis.contains(i)).or(vis.first().copied())
}

/// Un foglio: la miniatura della pagina, o il foglio bianco finché arriva. `mark` è il segno del file.
pub fn paper(f: &mut Frame, app: &App, path: &Path, page: usize, r: R, mark: Option<Rgb>) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 3.0, Some((255, 255, 255)), None);
    if let Some(Some(img)) = app.page_thumbs.get(&(path.to_path_buf(), page)) {
        f.p.image(img, r.0, r.1, r.2, r.3, 3.0, Fit::Contain);
    } else {
        // Righe finte, come nella tavola, finché la pagina non è pronta.
        let lines = ((r.3 - 20.0) / 12.0).max(2.0) as usize;
        for k in 0..lines {
            let lw = if k == 0 { 0.42 } else if k % 5 == 4 { 0.47 } else { 0.66 - (k * 7 % 14) as f32 / 100.0 };
            let y = r.1 + 10.0 + k as f32 * (r.3 - 20.0) / lines as f32;
            f.p.rrect(r.0 + r.2 * 0.12, y, r.2 * lw, if k == 0 { 3.0 } else { 2.0 }, 1.0, Some(mix(pal.muted, (255, 255, 255), 0.7)), None);
        }
    }
    f.p.rrect_line(r.0, r.1, r.2, r.3, 3.0, pal.border, 1.0, 1.0, None);
    if let Some(c) = mark {
        f.p.rrect(r.0, r.1, 4.0, r.3, 2.0, Some(c), None);
    }
}

/// I cinque pulsanti grandi, tre sopra e due sotto: Converti, Unisci, Dividi, Pagine, Comprimi.
/// `h` è l'altezza di riferimento di un pulsante; restituisce l'altezza usata.
pub fn modes(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, h: f32) -> f32 {
    let pal = f.pal;
    let mode = app.pdf_mode();
    let gap = 8.0;
    let bw = (w - 2.0 * gap) / 3.0;
    let bh = (h * 0.8).round();
    let isz = (bh * 0.37).round();
    let all = [(PdfMode::Convert, ic::CONVERT, "Converti"), (PdfMode::Merge, MERGE, "Unisci"), (PdfMode::Split, SCISSORS, "Dividi"), (PdfMode::Pages, PAGES, "Pagine"), (PdfMode::Compress, SHRINK, "Comprimi")];
    for (k, (m, icon, label)) in all.into_iter().enumerate() {
        let r = (x + (k % 3) as f32 * (bw + gap), y + (k / 3) as f32 * (bh + gap), bw, bh);
        let act = Act::PdfMode(m);
        let on = mode == m;
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.alt), Some((pal.primary, 2.0)));
        } else {
            let bg = if f.hovered(&act) { mix(pal.alt, pal.surface, 0.5) } else { pal.surface };
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(bg), Some((pal.border, 1.0)));
        }
        let c = if on { pal.primary } else { pal.text };
        let top = r.1 + (bh - isz - 4.0 - 20.0) / 2.0;
        f.icon(icon, r.0 + (bw - isz) / 2.0, top, isz, c);
        f.text_c(label, (r.0, top + isz + 4.0, bw, 20.0), (bh * 0.23).clamp(13.0, 15.0), if on { SEMI } else { SANS }, c);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
    2.0 * bh + gap
}

fn caption(f: &mut Frame, text: &str, x: f32, y: f32, w: f32) {
    let pal = f.pal;
    // Si traduce prima di passare alle maiuscole: la tabella ha i testi come sono scritti qui.
    f.text(&crate::i18n::lit(&crate::i18n::tr(text).to_uppercase()), x, y, w, 20.0, 13.5, SEMI, pal.muted);
}

/// Le pagine del file unito, una dopo l'altra con il segno del loro file. Restituisce l'altezza.
pub fn merged_strip(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, pw: f32) -> f32 {
    let pal = f.pal;
    let targets = app.targets();
    let mut pages: Vec<(usize, usize)> = Vec::new();
    let mut total = 0;
    for (n, &i) in targets.iter().enumerate() {
        let Some(count) = app.page_count(i) else { continue };
        for p in merged_pages(app, i, count) {
            total += 1;
            if pages.len() < MERGED_MAX {
                pages.push((n, p));
            }
        }
        let _ = i;
    }
    caption(f, if total > 0 { "Il file unito" } else { "Il file unito: si aspettano le pagine" }, x, y, w);
    if pages.is_empty() {
        return 24.0;
    }
    let ph = (pw * 1.35).round();
    let gap = 6.0;
    let per_row = (((w + gap) / (pw + gap)) as usize).max(1);
    let rows_max = 3;
    let shown = pages.len().min(per_row * rows_max);
    let top = y + 28.0;
    for (k, &(n, p)) in pages[..shown].iter().enumerate() {
        let (col, row) = (k % per_row, k / per_row);
        let px = x + col as f32 * (pw + gap);
        let py = top + row as f32 * (ph + 24.0);
        let i = targets[n];
        let path = app.files[i].path.clone();
        paper(f, app, &path, p, (px, py, pw, ph), Some(file_color(pal, n)));
        f.text_c(&(k + 1).to_string(), (px, py + ph + 2.0, pw, 18.0), 12.0, SANS, pal.muted);
    }
    let rows = shown.div_ceil(per_row);
    let mut h = 28.0 + rows as f32 * (ph + 24.0);
    if total > shown {
        f.text(&format!("e altre {} pagine, {total} in tutto", total - shown), x, y + h, w, 22.0, 14.0, SANS, pal.muted);
        h += 26.0;
    }
    h
}

/// La striscia di «Dividi»: le pagine del file `i`, con le forbici tra una e l'altra.
/// Un clic tra due pagine mette o toglie il taglio. Restituisce l'altezza.
pub fn split_strip(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32, pw: f32) -> f32 {
    let pal = f.pal;
    let Some(total) = app.page_count(i) else {
        caption(f, "Si leggono le pagine…", x, y, w);
        return 26.0;
    };
    let path = app.files[i].path.clone();
    let opts = conversioni::options_from(&app.values);
    let removing = match &opts.pdf_pages.plan {
        crate::pdfpages::Plan::Remove(t) => PageSel::parse(t).ok().flatten(),
        _ => None,
    };
    let by_marks = matches!(opts.pdf_pages.plan, crate::pdfpages::Plan::Bookmarks) && !app.cuts.contains_key(&path);
    caption(f, if removing.is_some() { "Le pagine segnate si tolgono" } else { "Tocca tra due pagine per tagliare" }, x, y, w);
    let cuts = if removing.is_some() || by_marks { Vec::new() } else { app.shown_cuts(&path, total as u32) };
    let ph = (pw * 1.35).round();
    let cw = (pw * 0.5).round();
    let unit = pw + cw;
    let per_row = ((w / unit) as usize).max(1);
    let shown = total.min(STRIP_MAX);
    let row_h = ph + 6.0 + 18.0 + 4.0 + 22.0 + 14.0;
    let top = y + 30.0;
    let k_size = (cw * 0.72).min(32.0);
    for k in 0..shown {
        let (col, row) = (k % per_row, k / per_row);
        let px = x + col as f32 * unit;
        let py = top + row as f32 * row_h;
        let page = k as u32 + 1;
        let part = cuts.iter().filter(|c| **c < page).count();
        let gone = removing.as_ref().is_some_and(|s| s.contains(k));
        paper(f, app, &path, k, (px, py, pw, ph), None);
        if gone {
            f.p.fill_a(px, py, pw, ph, pal.surface, 0.6);
            f.icon(ic::CLOSE, px + pw / 2.0 - 14.0, py + ph / 2.0 - 14.0, 28.0, pal.bad);
        }
        f.text_c(&page.to_string(), (px, py + ph + 6.0, pw, 18.0), 13.0, SANS, pal.muted);
        let (label, c) = if gone {
            ("Tolta".to_string(), pal.bad)
        } else if by_marks || removing.is_some() {
            (String::new(), pal.muted)
        } else {
            (format!("Parte {}", part + 1), file_color(pal, part))
        };
        if !label.is_empty() {
            let lw = f.measure(&label, 12.5, SEMI) + 16.0;
            let lr = (px + (pw - lw) / 2.0, py + ph + 28.0, lw, 22.0);
            f.p.rrect(lr.0, lr.1, lr.2, lr.3, 4.0, Some(mix(c, pal.surface, 0.86)), None);
            f.text_c(&label, lr, 12.5, SEMI, c);
        }
        // Le forbici dopo la pagina (non dopo l'ultima).
        if page < total as u32 && k + 1 < shown && removing.is_none() {
            let r = (px + pw, py, cw, ph);
            let act = Act::Cut(i, page);
            let on = cuts.contains(&page);
            let hot = f.hovered(&act);
            let c = if on { pal.bad } else if hot { pal.primary } else { mix(pal.muted, pal.surface, 0.35) };
            let mx = r.0 + cw / 2.0;
            if on {
                f.p.rrect_line(mx - 1.0, r.1 + 4.0, 2.0, r.3 - 8.0, 0.0, pal.bad, 2.0, 1.0, Some((6.0, 4.0)));
            }
            let cy = r.1 + r.3 / 2.0;
            f.p.circle(mx, cy, k_size / 2.0, Some(pal.surface), Some((c, 1.0)));
            let isz = k_size * 0.62;
            f.icon(SCISSORS, mx - isz / 2.0, cy - isz / 2.0, isz, c);
            f.hit(r, act);
        }
    }
    let rows = shown.div_ceil(per_row);
    let mut h = 30.0 + rows as f32 * row_h;
    if total > shown {
        f.para(&format!("Si vedono le prime {shown} pagine di {total}: per le altre scrivi gli intervalli a destra."), x, y + h, w, 14.0, SANS, pal.muted, 1.4, 2);
        h += 28.0;
    }
    h
}

/// Il numero tondo con la posizione nel file unito.
pub fn order_badge(f: &mut Frame, n: usize, cx: f32, cy: f32, r: f32) {
    let pal = f.pal;
    f.p.circle(cx, cy, r, Some(file_color(pal, n)), None);
    f.text_c(&(n + 1).to_string(), (cx - r, cy - r, 2.0 * r, 2.0 * r), (r * 1.0).round(), BOLD, (255, 255, 255));
}

/// Le frecce su e giù di un file della coda. `x` è il bordo sinistro, larghe `2 * s + 4`.
pub fn order_arrows(f: &mut Frame, app: &App, i: usize, x: f32, cy: f32, s: f32) {
    let pal = f.pal;
    let vis = app.visible();
    let pos = vis.iter().position(|&v| v == i).unwrap_or(0);
    for (k, (icon, d, enabled)) in [(UP, -1, pos > 0), (ic::CHEVRON_DOWN, 1, pos + 1 < vis.len())].into_iter().enumerate() {
        let r = (x + k as f32 * (s + 4.0), cy - s / 2.0, s, s);
        let act = Act::MoveFile(i, d);
        if enabled && f.hovered(&act) {
            f.p.circle(r.0 + s / 2.0, cy, s / 2.0 + 2.0, Some(pal.alt), None);
        }
        let c = if enabled { pal.text } else { mix(pal.muted, pal.surface, 0.55) };
        f.icon(icon, r.0 + s * 0.1, r.1 + s * 0.1, s * 0.8, c);
        if enabled && !app.busy() {
            f.hit(r, act);
        }
    }
}

/// La maniglia per trascinare la riga `i` (alta `row_h`).
pub fn grip(f: &mut Frame, app: &App, i: usize, x: f32, cy: f32, row_h: f32) {
    let pal = f.pal;
    let act = Act::RowDrag(i, row_h as u16);
    let c = if f.hovered(&act) || f.ui.drag.as_ref().is_some_and(|(a, _)| *a == act) { pal.primary } else { mix(pal.muted, pal.surface, 0.25) };
    f.icon(GRIP, x, cy - 12.0, 24.0, c);
    if !app.busy() {
        f.hit_kind((x - 4.0, cy - 22.0, 32.0, 44.0), act, crate::ui::core::HitKind::Slider);
    }
}

/// Di quante righe è stata trascinata la riga `i` (per disegnarla dove sta andando).
pub fn drag_offset(f: &Frame, i: usize) -> Option<f32> {
    match f.ui.drag.as_ref() {
        Some((Act::RowDrag(d, _), _)) if *d == i => Some(f.ui.drag_y.1 - f.ui.drag_y.0),
        _ => None,
    }
}

/// Mentre si trascina la riga `n` (file `i`) di una coda di righe alte `rh` che parte da `top`:
/// la linea dove andrà a finire.
pub fn drop_line(f: &mut Frame, n: usize, i: usize, rows: usize, top: f32, rh: f32, x: f32, w: f32) {
    let Some(dy) = drag_offset(f, i) else { return };
    let to = (n as i32 + (dy / rh).round() as i32).clamp(0, rows as i32 - 1) as usize;
    if to == n {
        return;
    }
    let y = top + if to > n { (to + 1) as f32 * rh } else { to as f32 * rh };
    let pal = f.pal;
    f.p.rrect(x + 12.0, y - 2.0, w - 24.0, 4.0, 2.0, Some(pal.primary), None);
}

/// Il pannello delle opzioni dei temi senza coda grande: l'ordine dei file con le pagine (Unisci)
/// o la striscia del file in evidenza (Dividi). Restituisce l'altezza.
pub fn panel_extras(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle) -> f32 {
    let pal = f.pal;
    match app.pdf_mode() {
        PdfMode::Convert => 0.0,
        PdfMode::Merge => {
            let targets = app.targets();
            let vis = app.visible();
            f.text("Ordine dei file", x, y, w, 24.0, st.label, SANS, pal.text);
            let mut cy = y + 30.0;
            let rh = st.control_h + 6.0;
            let mut lifted = None;
            let row = |f: &mut Frame, n: usize, i: usize, cy: f32, up: bool| {
                let file = &app.files[i];
                let r = (x, cy, w, st.control_h);
                if up {
                    f.p.shadow(r.0, r.1, r.2, r.3, 6.0, 10.0, 0.25, 4.0);
                    f.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((pal.primary, 1.5)));
                } else {
                    f.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
                }
                grip(f, app, i, x + 4.0, cy + r.3 / 2.0, rh);
                let on = targets.contains(&i);
                let badge_n = targets.iter().position(|&t| t == i).unwrap_or(n);
                let bx = x + 46.0;
                if on {
                    order_badge(f, badge_n, bx, cy + r.3 / 2.0, 11.0);
                } else {
                    f.p.circle(bx, cy + r.3 / 2.0, 11.0, None, Some((pal.border, 1.0)));
                }
                let pages_w = 74.0;
                let name_w = w - 64.0 - pages_w - 60.0;
                let c = if on { pal.text } else { pal.muted };
                f.text(&file.name(), x + 64.0, cy, name_w, r.3, st.note + 1.0, SEMI, c);
                f.field((x + w - 60.0 - pages_w, cy + 5.0, pages_w, r.3 - 10.0), Field::FilePages(i), &file.pages, "tutte", st.note, false);
                order_arrows(f, app, i, x + w - 54.0, cy + r.3 / 2.0, 24.0);
            };
            let top = cy;
            for (n, &i) in vis.iter().enumerate() {
                if let Some(dy) = drag_offset(f, i) {
                    lifted = Some((n, i, dy));
                }
                row(f, n, i, cy, false);
                cy += rh;
            }
            if let Some((n, i, dy)) = lifted {
                // La riga trascinata segue il mouse; la linea dice dove andrà.
                let to = (n as i32 + (dy / rh).round() as i32).clamp(0, vis.len() as i32 - 1) as usize;
                let line_y = top + if to > n { (to + 1) as f32 * rh } else { to as f32 * rh } - 3.0;
                f.p.rrect(x, line_y - 2.0, w, 4.0, 2.0, Some(pal.primary), None);
                let ry = (top + n as f32 * rh + dy).clamp(top - rh / 2.0, cy - rh / 2.0);
                row(f, n, i, ry, true);
            }
            cy += 4.0;
            cy += f.para("Trascina dalla maniglia o usa le frecce: l'ordine qui è l'ordine delle pagine nel file unito. In «tutte» puoi scrivere per esempio 1-3,5.", x, cy, w, st.note, SANS, pal.muted, 1.45, 3) + 12.0;
            cy += merged_strip(f, app, x, cy, w, 34.0);
            cy - y
        }
        PdfMode::Pages => {
            let Some(i) = split_file(app) else { return 0.0 };
            f.select((x, y, w, st.control_h), &app.files[i].name(), None, 16.0, Act::PagesFileMenu);
            let mut cy = y + st.control_h + 10.0;
            pages_count(f, app, i, x, cy, w, 26.0, st.note);
            cy += 30.0;
            let th = 34.0;
            pages_tools(f, app, i, x + w, cy, th, st.note);
            cy += th + 14.0;
            if conversioni::options_from(&app.values).pdf_edit.stamp.is_empty() {
                cy += board(f, app, i, x + 6.0, cy + 6.0, w - 12.0, 46.0) + 10.0;
                cy += f.para("Tocca le pagine per sceglierle, trascinale per spostarle.", x, cy, w, st.note, SANS, pal.muted, 1.45, 2) + 6.0;
            } else {
                cy += stamp_view(f, app, i, x, cy, w, (w * 0.95).min(300.0), 34.0) + 8.0;
            }
            cy - y
        }
        PdfMode::Compress => {
            let targets = app.targets();
            let mut cy = y;
            for i in app.visible() {
                let on = targets.contains(&i);
                f.text(&app.files[i].name(), x, cy, w - 110.0, 24.0, st.note + 1.0, SEMI, if on { pal.text } else { pal.muted });
                saving_pill(f, app, i, x + w, cy + 12.0, st.note);
                let line = match squeeze_numbers(app, i) {
                    Some((b, a, true, false)) => format!("Da {} a {}", crate::engine::size_words(b), crate::engine::size_words(a)),
                    Some((b, a, false, false)) => format!("Da {} a circa {}", crate::engine::size_words(b), crate::engine::size_words(a)),
                    _ => squeeze_sub(app, i),
                };
                f.text(&line, x, cy + 24.0, w, 22.0, st.note, SANS, pal.muted);
                cy += 54.0;
            }
            cy += f.para("Un PDF che non si riesce a ridurre resta com'è: non se ne salva mai uno più pesante.", x, cy, w, st.note, SANS, pal.muted, 1.45, 3) + 6.0;
            cy - y
        }
        PdfMode::Split => {
            let Some(i) = split_file(app) else { return 0.0 };
            f.text(&app.files[i].name(), x, y, w, 24.0, st.label, SEMI, pal.text);
            let h = split_strip(f, app, i, x, y + 30.0, w, 46.0);
            let mut cy = y + 30.0 + h;
            if app.visible().len() > 1 {
                cy += f.para("I tagli valgono per il file scelto nella coda, gli intervalli per gli altri.", x, cy, w, st.note, SANS, pal.muted, 1.45, 2) + 6.0;
            }
            cy - y
        }
    }
}

/// Il testo del pulsante grande per unire e dividere; `None` per le conversioni.
pub fn go_label(app: &App) -> Option<String> {
    if app.activity != Activity::Convert || app.kind() != Kind::Pdf {
        return None;
    }
    let targets = app.targets();
    let n = targets.len();
    match app.pdf_mode() {
        PdfMode::Convert => None,
        PdfMode::Merge => Some(if n < 2 { "Unisci".into() } else { format!("Unisci {n} PDF") }),
        PdfMode::Compress if app.simple_view => Some("Riduci il peso".into()),
        PdfMode::Compress => Some(if n == 0 { "Comprimi".into() } else { format!("Comprimi {n} PDF") }),
        PdfMode::Pages => Some(match n {
            0 => "Salva".into(),
            1 => match app.board(targets[0]).map(|b| b.kept().len()) {
                Some(1) => "Salva 1 pagina".into(),
                Some(k) => format!("Salva {k} pagine"),
                None => "Salva".into(),
            },
            _ => format!("Salva {n} PDF"),
        }),
        PdfMode::Split => {
            if n != 1 {
                return Some(if n == 0 { "Dividi".into() } else { format!("Dividi {n} PDF") });
            }
            let i = targets[0];
            let Some(total) = app.page_count(i) else { return Some("Dividi".into()) };
            if app.simple_view {
                return Some(format!("Dividi in {total} PDF"));
            }
            let path = &app.files[i].path;
            let plan = match app.cuts.get(path) {
                Some(c) => crate::pdfpages::Plan::Cuts(c.clone()),
                None => conversioni::options_from(&app.values).pdf_pages.plan,
            };
            Some(match plan {
                crate::pdfpages::Plan::Bookmarks => "Dividi ai segnalibri".into(),
                crate::pdfpages::Plan::Remove(_) => "Togli le pagine".into(),
                plan => match crate::pdfpages::groups(&plan, total as u32, None) {
                    Ok(g) if g.len() == 1 => "Dividi in 1 PDF".into(),
                    Ok(g) => format!("Dividi in {} PDF", g.len()),
                    Err(_) => "Dividi".into(),
                },
            })
        }
    }
}

/// Lo stato di un file per la riga di «Unisci»: dove finisce nel file unito, finché aspetta.
pub fn merge_sub(app: &App, i: usize) -> String {
    let file = &app.files[i];
    if file.state != FileState::Waiting {
        return file.message.clone();
    }
    let targets = app.targets();
    if !targets.contains(&i) {
        return "Non spuntato: resta fuori".into();
    }
    merged_place(app, &targets, i).unwrap_or_else(|| "Si leggono le pagine…".into())
}

/// Dove finisce una pagina del banco trascinata di (`dx`, `dy`): il posto più vicino.
pub fn page_drop(pos: usize, dx: f32, dy: f32, uw: f32, uh: f32, per_row: usize) -> usize {
    let per_row = per_row.max(1) as i32;
    let (col, row) = (pos as i32 % per_row, pos as i32 / per_row);
    let ncol = (col + (dx / uw.max(1.0)).round() as i32).clamp(0, per_row - 1);
    let nrow = (row + (dy / uh.max(1.0)).round() as i32).max(0);
    (nrow * per_row + ncol) as usize
}

/// Una pagina girata di `quarters` quarti di giro, grande quanto sta nel riquadro e centrata.
/// Restituisce il riquadro del foglio disegnato.
pub fn paper_turned(f: &mut Frame, app: &App, path: &Path, page: usize, r: R, quarters: i32) -> R {
    let pal = f.pal;
    let img = app.page_thumbs.get(&(path.to_path_buf(), page)).and_then(|i| i.as_ref()).map(|i| i.turned(quarters));
    // Finché la miniatura non c'è, un foglio A4 (girato se serve).
    let (iw, ih) = img.as_ref().map(|i| (i.w as f32, i.h as f32)).unwrap_or(if quarters % 2 == 0 { (210.0, 297.0) } else { (297.0, 210.0) });
    let k = (r.2 / iw).min(r.3 / ih);
    let (w, h) = ((iw * k).round(), (ih * k).round());
    let pr = (r.0 + ((r.2 - w) / 2.0).round(), r.1 + ((r.3 - h) / 2.0).round(), w, h);
    f.p.rrect(pr.0, pr.1, pr.2, pr.3, 3.0, Some((255, 255, 255)), None);
    match img {
        Some(img) => f.p.image(&img, pr.0, pr.1, pr.2, pr.3, 3.0, Fit::Contain),
        None => {
            let lines = ((pr.3 - 20.0) / 12.0).max(2.0) as usize;
            for k in 0..lines {
                let lw = if k == 0 { 0.42 } else { 0.66 - (k * 7 % 14) as f32 / 100.0 };
                let y = pr.1 + 10.0 + k as f32 * (pr.3 - 20.0) / lines as f32;
                f.p.rrect(pr.0 + pr.2 * 0.12, y, pr.2 * lw, 2.0, 1.0, Some(mix(pal.muted, (255, 255, 255), 0.7)), None);
            }
        }
    }
    f.p.rrect_line(pr.0, pr.1, pr.2, pr.3, 3.0, pal.border, 1.0, 1.0, None);
    pr
}

/// «Pagina 2» o «Pagine 2, 5, 7» (accorciato oltre le prime sei).
pub fn pages_words(pages: &[u32]) -> String {
    let list: Vec<String> = pages.iter().take(6).map(|p| p.to_string()).collect();
    let more = if pages.len() > 6 { format!(" e altre {}", pages.len() - 6) } else { String::new() };
    match pages.len() {
        0 => String::new(),
        1 => format!("Pagina {}", pages[0]),
        _ => format!("Pagine {}{more}", list.join(", ")),
    }
}

/// Un comando della barra di «Pagine»: icona (e testo), sfondo chiaro come nella tavola. Restituisce la larghezza.
fn chip(f: &mut Frame, x: f32, y: f32, h: f32, label: &str, icon: &str, size: f32, act: Act, enabled: bool) -> f32 {
    let pal = f.pal;
    let isz = (h * 0.5).round();
    let tw = if label.is_empty() { 0.0 } else { f.measure(label, size, SEMI) + 8.0 };
    let w = 12.0 + isz + tw + 12.0;
    let r = (x, y, w, h);
    if enabled {
        let bg = if f.hovered(&act) { mix(pal.alt, pal.primary, 0.12) } else { pal.alt };
        f.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(bg), None);
    }
    let c = if enabled { pal.primary } else { mix(pal.muted, pal.surface, 0.3) };
    f.icon(icon, x + 12.0, y + (h - isz) / 2.0, isz, c);
    if !label.is_empty() {
        f.text(label, x + 12.0 + isz + 8.0, y, tw, h, size, SEMI, c);
    }
    if enabled {
        f.focus_ring(r, 6.0, &act);
        f.hit(r, act);
    }
    w
}

/// La barra di «Pagine»: gira a sinistra, gira a destra, Togli, Estrai. `x` è il bordo destro.
/// Restituisce la larghezza usata.
pub fn pages_tools(f: &mut Frame, app: &App, i: usize, right: f32, y: f32, h: f32, size: f32) -> f32 {
    let b = app.board(i).unwrap_or_default();
    let on = !b.sel.is_empty() && !app.busy();
    let all_gone = on && b.sel.iter().all(|&s| b.pages.get(s).is_some_and(|p| p.2));
    let remove = if all_gone { "Rimetti" } else { "Togli" };
    let isz = (h * 0.5).round();
    let wid = |f: &Frame, label: &str| 12.0 + isz + if label.is_empty() { 0.0 } else { f.measure(label, size, SEMI) + 8.0 } + 12.0;
    let total = wid(f, "") * 2.0 + wid(f, remove) + wid(f, "Estrai") + 3.0 * 8.0;
    let mut x = right - total;
    x += chip(f, x, y, h, "", ROTATE_L, size, Act::PagesRotate(i, -1), on) + 8.0;
    x += chip(f, x, y, h, "", ROTATE_R, size, Act::PagesRotate(i, 1), on) + 8.0;
    x += chip(f, x, y, h, remove, ic::TRASH, size, Act::PagesRemove(i), on) + 8.0;
    chip(f, x, y, h, "Estrai", EXTRACT, size, Act::PagesExtract(i), on);
    total
}

/// «6 pagine · 2 scelte»: si tocca per sceglierle tutte (o nessuna).
pub fn pages_count(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32, h: f32, size: f32) {
    let pal = f.pal;
    let Some(b) = app.board(i) else {
        f.text("Si leggono le pagine…", x, y, w, h, size, SANS, pal.muted);
        return;
    };
    let n = b.pages.len();
    let text = match b.sel.len() {
        0 => format!("{n} pagine"),
        1 => format!("{n} pagine · 1 scelta"),
        s => format!("{n} pagine · {s} scelte"),
    };
    let tw = f.measure(&text, size, SANS).min(w);
    let act = Act::PagesAll(i);
    let c = if f.hovered(&act) { pal.primary } else { pal.muted };
    f.text(&text, x, y, w, h, size, SANS, c);
    f.hit((x - 4.0, y, tw + 8.0, h), act);
}

/// Il banco di «Pagine»: le pagine del file `i` come diventeranno, da scegliere con un clic e da
/// trascinare. `pw` è la larghezza di un foglio. Restituisce l'altezza.
pub fn board(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32, pw: f32) -> f32 {
    let pal = f.pal;
    let Some(b) = app.board(i) else {
        caption(f, "Si leggono le pagine…", x, y, w);
        return 26.0;
    };
    let path = app.files[i].path.clone();
    let ph = (pw * 1.36).round();
    let small = pw < 80.0;
    let (gap_x, tag_h) = (if small { 14.0 } else { 38.0 }, if small { 22.0 } else { 70.0 });
    let (uw, uh) = (pw + gap_x, ph + tag_h);
    let per_row = (((w + gap_x) / uw) as usize).max(1);
    let shown = b.pages.len().min(BOARD_MAX);
    let dragging = match f.ui.drag.as_ref() {
        Some((Act::PageDrag(di, pos, ..), _)) if *di == i => Some((*pos, f.ui.drag_x.1 - f.ui.drag_x.0, f.ui.drag_y.1 - f.ui.drag_y.0)),
        _ => None,
    };
    let cell = |f: &mut Frame, k: usize, cx: f32, cy: f32, lifted: bool| {
        let (page, rot, gone) = b.pages[k];
        let sel = b.sel.contains(&k);
        let r = (cx, cy, pw, ph);
        if lifted {
            f.p.shadow(r.0, r.1, r.2, r.3, 4.0, 14.0, 0.3, 6.0);
        }
        if sel {
            f.p.rrect(r.0 - 6.0, r.1 - 6.0, r.2 + 12.0, r.3 + 12.0, 6.0, Some(mix(pal.primary, pal.surface, 0.9)), Some((pal.primary, 3.0)));
        }
        let pr = paper_turned(f, app, &path, page as usize - 1, r, rot / 90);
        if gone {
            f.p.fill_a(pr.0, pr.1, pr.2, pr.3, pal.surface, 0.62);
            f.p.line(pr.0 - 4.0, pr.1 + pr.3 * 0.72, pr.0 + pr.2 + 4.0, pr.1 + pr.3 * 0.28, pal.bad, 3.0);
        }
        if !lifted {
            let (tag, c, wt) = if small {
                // Nei pannelli stretti basta il numero, colorato se la pagina è tolta o girata.
                (page.to_string(), if gone { pal.bad } else if rot != 0 { pal.primary } else { pal.muted }, if gone || rot != 0 { SEMI } else { SANS })
            } else if gone {
                (format!("Tolta · {page}"), pal.bad, SEMI)
            } else if rot != 0 {
                (format!("Ruotata {rot}° · {page}"), pal.muted, SANS)
            } else {
                (page.to_string(), pal.muted, SANS)
            };
            f.text_c(&tag, (cx - gap_x / 2.0, cy + ph + if small { 2.0 } else { 8.0 }, uw, 20.0), if small { 11.5 } else { 13.0 }, wt, c);
            let act = Act::PageDrag(i, k, uw as u16, uh as u16, per_row as u16);
            if !sel && f.hovered(&act) && dragging.is_none() {
                f.p.rrect_line(r.0 - 4.0, r.1 - 4.0, r.2 + 8.0, r.3 + 8.0, 5.0, pal.primary, 1.5, 0.6, None);
            }
            if !app.busy() {
                f.hit_kind((r.0 - 6.0, r.1 - 6.0, r.2 + 12.0, r.3 + 12.0), act, crate::ui::core::HitKind::Slider);
            }
        }
    };
    for k in 0..shown {
        let (cx, cy) = (x + (k % per_row) as f32 * uw, y + (k / per_row) as f32 * uh);
        if !f.p.shows_y(cy - 8.0, uh) {
            continue;
        }
        if dragging.is_some_and(|d| d.0 == k) {
            // Il posto da cui è partita resta segnato, vuoto.
            f.p.rrect_line(cx, cy, pw, ph, 4.0, pal.border, 1.5, 1.0, Some((6.0, 4.0)));
            let act = Act::PageDrag(i, k, uw as u16, uh as u16, per_row as u16);
            f.hit_kind((cx - 6.0, cy - 6.0, pw + 12.0, ph + 12.0), act, crate::ui::core::HitKind::Slider);
            continue;
        }
        cell(f, k, cx, cy, false);
    }
    if let Some((pos, dx, dy)) = dragging {
        if dx.abs() >= 6.0 || dy.abs() >= 6.0 {
            let to = page_drop(pos, dx, dy, uw, uh, per_row).min(shown.saturating_sub(1));
            // La linea dove andrà: prima del posto `to` (dopo, se si va avanti).
            let (tc, tr) = (to % per_row, to / per_row);
            let lx = x + tc as f32 * uw + if to > pos { pw + gap_x / 2.0 } else { -gap_x / 2.0 };
            f.p.rrect(lx - 2.0, y + tr as f32 * uh - 6.0, 4.0, ph + 12.0, 2.0, Some(pal.primary), None);
        }
        let (cx, cy) = (x + (pos % per_row) as f32 * uw + dx, y + (pos / per_row) as f32 * uh + dy);
        cell(f, pos, cx, cy, true);
    }
    let rows = shown.div_ceil(per_row);
    let mut h = rows as f32 * uh;
    if b.pages.len() > shown {
        f.text(&format!("Si vedono le prime {shown} pagine di {}.", b.pages.len()), x, y + h, w, 22.0, 14.0, SANS, pal.muted);
        h += 26.0;
    }
    h
}

/// «Pagine» con numeri o filigrana: la pagina vera in grande e sotto la striscia delle pagine.
/// `big` è l'altezza della pagina grande. Restituisce l'altezza.
pub fn stamp_view(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32, big: f32, pw: f32) -> f32 {
    let pal = f.pal;
    let Some(b) = app.board(i) else { return 0.0 };
    let path = app.files[i].path.clone();
    let bw = (big / 1.414).round();
    let r = (x + ((w - bw) / 2.0).round(), y, bw, big);
    match &app.stamp_view.1 {
        Some(img) => {
            let k = (r.2 / img.w as f32).min(r.3 / img.h as f32);
            let (dw, dh) = ((img.w as f32 * k).round(), (img.h as f32 * k).round());
            let pr = (x + ((w - dw) / 2.0).round(), y + ((big - dh) / 2.0).round(), dw, dh);
            f.p.shadow(pr.0, pr.1, pr.2, pr.3, 3.0, 12.0, 0.18, 4.0);
            f.p.image(img, pr.0, pr.1, pr.2, pr.3, 3.0, Fit::Contain);
            f.p.rrect_line(pr.0, pr.1, pr.2, pr.3, 3.0, pal.border, 1.0, 1.0, None);
        }
        None => {
            f.p.rrect(r.0, r.1, r.2, r.3, 3.0, Some((255, 255, 255)), Some((pal.border, 1.0)));
            f.text_c("Si prepara l'anteprima…", r, 14.0, SANS, pal.muted);
        }
    }
    let kept: Vec<usize> = (0..b.pages.len()).filter(|&k| !b.pages[k].2).collect();
    let n = app.stamp_page.min(kept.len().saturating_sub(1));
    f.text_c(&format!("Anteprima della pagina {} di {}", n + 1, kept.len()), (x, y + big + 8.0, w, 22.0), 14.0, SANS, pal.muted);
    // La striscia: le pagine che restano, nell'ordine nuovo. Un clic la mostra in grande e la sceglie.
    let ph = (pw * 1.4).round();
    let gap = 8.0;
    let per_row = (((w + gap) / (pw + gap)) as usize).max(1);
    let top = y + big + 40.0;
    for (n2, &k) in kept.iter().enumerate().take(BOARD_MAX) {
        let (cx, cy) = (x + (n2 % per_row) as f32 * (pw + gap), top + (n2 / per_row) as f32 * (ph + gap + 4.0));
        if !f.p.shows_y(cy, ph) {
            continue;
        }
        let (page, rot, _) = b.pages[k];
        let r = (cx, cy, pw, ph);
        let act = Act::StampPick(i, k);
        if n2 == n {
            f.p.rrect(r.0 - 4.0, r.1 - 4.0, r.2 + 8.0, r.3 + 8.0, 5.0, None, Some((pal.primary, 3.0)));
        } else if f.hovered(&act) {
            f.p.rrect(r.0 - 3.0, r.1 - 3.0, r.2 + 6.0, r.3 + 6.0, 5.0, None, Some((pal.primary, 1.0)));
        }
        paper_turned(f, app, &path, page as usize - 1, r, rot / 90);
        f.hit((r.0 - 4.0, r.1 - 4.0, r.2 + 8.0, r.3 + 8.0), act);
    }
    let rows = kept.len().min(BOARD_MAX).div_ceil(per_row);
    big + 40.0 + rows as f32 * (ph + gap + 4.0)
}

/// «Cosa succederà»: le pagine tolte, girate e scelte, e quante ne restano. Restituisce l'altezza.
pub fn pages_summary(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle) -> f32 {
    let pal = f.pal;
    let Some(i) = split_file(app) else { return 0.0 };
    let Some(b) = app.board(i) else { return 0.0 };
    f.text("Cosa succederà", x, y, w, 24.0, st.label, SEMI, pal.text);
    let mut cy = y + 32.0;
    let pill = |f: &mut Frame, text: &str, c: Rgb, cy: f32| {
        let tw = f.measure(text, st.note, SEMI) + 20.0;
        let r = (x + w - tw, cy + 1.0, tw, 24.0);
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(c, pal.surface, 0.86)), None);
        f.text_c(text, r, st.note, SEMI, c);
        tw
    };
    let line = |f: &mut Frame, left: &str, cy: f32, right_w: f32| {
        f.text(left, x, cy, w - right_w - 10.0, 26.0, st.note + 1.0, SANS, pal.text);
    };
    let gone: Vec<u32> = b.pages.iter().filter(|p| p.2).map(|p| p.0).collect();
    if !gone.is_empty() {
        let pw = pill(f, if gone.len() == 1 { "Tolta" } else { "Tolte" }, pal.bad, cy);
        line(f, &pages_words(&gone), cy, pw);
        cy += 34.0;
    }
    for angle in [90, 180, 270] {
        let turned: Vec<u32> = b.pages.iter().filter(|p| !p.2 && p.1 == angle).map(|p| p.0).collect();
        if !turned.is_empty() {
            let label = format!("{} {angle}°", if turned.len() == 1 { "Ruotata" } else { "Ruotate" });
            let pw = pill(f, &label, pal.primary, cy);
            line(f, &pages_words(&turned), cy, pw);
            cy += 34.0;
        }
    }
    let moved = b.pages.iter().filter(|p| !p.2).map(|p| p.0).collect::<Vec<_>>().windows(2).any(|w| w[0] > w[1]);
    if moved {
        let pw = pill(f, "Spostate", pal.ok, cy);
        line(f, "Ordine nuovo", cy, pw);
        cy += 34.0;
    }
    if !b.sel.is_empty() {
        let chosen: Vec<u32> = b.sel.iter().filter_map(|&s| b.pages.get(s)).map(|p| p.0).collect();
        let tw = f.measure("Scelte", st.note, SANS);
        f.text_r("Scelte", (x + w - tw - 4.0, cy, tw + 4.0, 26.0), st.note, SANS, pal.muted);
        line(f, &pages_words(&chosen), cy, tw);
        cy += 34.0;
    }
    if b.untouched() && b.sel.is_empty() {
        cy += f.para("Nessuna modifica alle pagine: sceglile a sinistra con un clic.", x, cy, w, st.note, SANS, pal.muted, 1.4, 3) + 8.0;
    }
    let kept = b.kept().len();
    f.text("Risultato", x, cy, w / 2.0, 26.0, st.note + 1.0, SEMI, pal.text);
    f.text_r(&if kept == 1 { "1 pagina".to_string() } else { format!("{kept} pagine") }, (x + w / 2.0, cy, w / 2.0, 26.0), st.note + 1.0, SEMI, pal.text);
    cy += 30.0;
    if !b.untouched() {
        let label = "Torna alle pagine com'erano";
        let tw = f.measure(label, st.note, SEMI);
        let act = Act::PagesReset(i);
        let c = if f.hovered(&act) { mix(pal.primary, pal.text, 0.3) } else { pal.primary };
        f.text(label, x, cy, tw + 4.0, 24.0, st.note, SEMI, c);
        f.hit((x - 4.0, cy, tw + 8.0, 24.0), act);
        cy += 28.0;
    }
    cy - y
}

/// Peso adesso e dopo per una riga di «Comprimi»: (peso, peso dopo o stima, è vero e non stimato, già leggero).
pub fn squeeze_numbers(app: &App, i: usize) -> Option<(u64, u64, bool, bool)> {
    let file = &app.files[i];
    let before = app.media_of(&file.path).map(|m| m.bytes).filter(|b| *b > 0);
    if file.state == FileState::Done && app.action == "compress" {
        if let (Some(out), Some(b)) = (&file.output, before) {
            if *out == file.path {
                return Some((b, b, true, true));
            }
            if let Ok(m) = std::fs::metadata(out) {
                return Some((b, m.len(), true, false));
            }
        }
    }
    match app.estimate(i)? {
        Ok((b, a)) => Some((*b, (*a).min(*b), false, *a >= *b)),
        Err(_) => None,
    }
}

/// La riga sotto il nome in «Comprimi»: pagine e cosa si sa della stima.
pub fn squeeze_sub(app: &App, i: usize) -> String {
    let file = &app.files[i];
    if file.state != FileState::Waiting {
        return file.message.clone();
    }
    let pages = app.page_count(i).map(|n| if n == 1 { "1 pagina".to_string() } else { format!("{n} pagine") }).unwrap_or_default();
    let what = match app.estimate(i) {
        None => "si calcola la stima…".to_string(),
        Some(Err(e)) => e.clone(),
        Some(Ok((b, a))) if a >= b => "già leggero, resta com'è".into(),
        Some(Ok(_)) => "stima pronta".into(),
    };
    if pages.is_empty() {
        what
    } else {
        format!("{pages} · {what}")
    }
}

/// La pillola del risparmio: «−62%», o «Già leggero».
pub fn saving_pill(f: &mut Frame, app: &App, i: usize, right: f32, cy: f32, size: f32) {
    let pal = f.pal;
    let Some((b, a, _, light)) = squeeze_numbers(app, i) else { return };
    let (text, c) = if light { ("Già leggero".to_string(), pal.muted) } else { (format!("−{}%", 100 - (a * 100 / b.max(1)).min(100)), pal.ok) };
    let tw = f.measure(&text, size, SEMI) + 20.0;
    let r = (right - tw, cy - 13.0, tw, 26.0);
    f.p.rrect(r.0, r.1, r.2, r.3, 13.0, Some(mix(c, pal.surface, 0.85)), None);
    f.text_c(&text, r, size, SEMI, c);
}

/// Le due barre di una riga di «Comprimi»: adesso e dopo, sulla scala del file più pesante.
pub fn squeeze_bars(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32, max: u64, size: f32) {
    let pal = f.pal;
    let before = app.media_of(&app.files[i].path).map(|m| m.bytes).unwrap_or(0);
    let scale = |v: u64| (v as f32 / max.max(1) as f32).clamp(0.02, 1.0) * w;
    f.p.rrect(x, y, w, 10.0, 5.0, Some(mix(pal.border, pal.surface, 0.35)), None);
    f.p.rrect(x, y, scale(before), 10.0, 5.0, Some(mix(pal.muted, pal.surface, 0.45)), None);
    f.text(&format!("{} adesso", crate::engine::size_words(before)), x, y + 13.0, w, 20.0, size, SANS, pal.muted);
    f.p.rrect(x, y + 42.0, w, 10.0, 5.0, Some(mix(pal.border, pal.surface, 0.35)), None);
    match squeeze_numbers(app, i) {
        Some((_, a, real, _)) => {
            f.p.rrect(x, y + 42.0, scale(a), 10.0, 5.0, Some(pal.primary), None);
            let t = if real { format!("{} dopo", crate::engine::size_words(a)) } else { format!("circa {}", crate::engine::size_words(a)) };
            f.text(&t, x, y + 55.0, w, 20.0, size, SEMI, pal.primary);
        }
        None => f.text("stima in corso…", x, y + 55.0, w, 20.0, size, SANS, pal.muted),
    }
}

/// «Da 165 KB a circa 73 KB» per i PDF spuntati (vuoto finché manca una stima).
pub fn squeeze_total(app: &App) -> Option<(u64, u64, bool)> {
    let (mut before, mut after, mut real) = (0u64, 0u64, true);
    let targets = app.targets();
    if targets.is_empty() {
        return None;
    }
    for i in targets {
        let (b, a, r, _) = squeeze_numbers(app, i)?;
        before += b;
        after += a;
        real &= r;
    }
    Some((before, after, real))
}
