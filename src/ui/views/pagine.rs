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

/// Quante pagine mostra al massimo la striscia di «Dividi» (oltre si scrivono gli intervalli).
pub const STRIP_MAX: usize = 60;
/// Quante pagine del file unito si disegnano al massimo.
pub const MERGED_MAX: usize = 48;

/// Vero se la scheda PDF mostra i tre pulsanti (Standard, Conversioni).
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

/// I tre pulsanti grandi: Converti, Unisci, Dividi. Restituisce l'altezza.
pub fn modes(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, h: f32) -> f32 {
    let pal = f.pal;
    let mode = app.pdf_mode();
    let gap = 8.0;
    let bw = (w - 2.0 * gap) / 3.0;
    let isz = (h * 0.33).round();
    for (k, (m, icon, label)) in [(PdfMode::Convert, ic::CONVERT, "Converti"), (PdfMode::Merge, MERGE, "Unisci"), (PdfMode::Split, SCISSORS, "Dividi")].into_iter().enumerate() {
        let r = (x + k as f32 * (bw + gap), y, bw, h);
        let act = Act::PdfMode(m);
        let on = mode == m;
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.alt), Some((pal.primary, 2.0)));
        } else {
            let bg = if f.hovered(&act) { mix(pal.alt, pal.surface, 0.5) } else { pal.surface };
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(bg), Some((pal.border, 1.0)));
        }
        let c = if on { pal.primary } else { pal.text };
        let top = r.1 + (h - isz - 6.0 - 22.0) / 2.0;
        f.icon(icon, r.0 + (bw - isz) / 2.0, top, isz, c);
        f.text_c(label, (r.0, top + isz + 6.0, bw, 22.0), (h * 0.2).clamp(14.0, 16.0), if on { SEMI } else { SANS }, c);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
    h
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
