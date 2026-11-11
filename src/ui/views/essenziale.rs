//! Tema 1, «Windows essenziale»: attività a sinistra, categorie in schede, coda con ispettore a
//! destra e piede con destinazione e pulsante. La Semplificata ha una sola area operativa.
//! Misure dalla tavola 01 (1536×1024); la coda si allarga, navigazione, ispettore e piede restano.

use super::*;
use crate::ui::core::{HitKind, Scroll};
use crate::ui::model::{FileState, PdfMode};
use crate::ui::gfx::Rgb;

const NAV_W: f32 = 270.0;
const TOP: f32 = 56.0;

fn nav(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.clear(pal.bg);
    f.caption((0.0, 0.0, f.w, TOP));
    f.brand(26.0, 15.0, 26.0, 18.0, 12.0, None);
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
    // Le attività stanno ogni 64: il riquadro della scelta scivola da una all'altra.
    let ys = [78.0, 142.0, 206.0, 270.0, 334.0];
    let acts = f.theme.activities();
    let ys = &ys[..acts.len()];
    let cur = acts.iter().position(|a| *a == app.activity).unwrap_or(0);
    let at = f.glide(&Act::Activity(Activity::Convert), cur as f32).clamp(0.0, (ys.len() - 1) as f32);
    let (i0, i1) = (at.floor() as usize, at.ceil() as usize);
    let k = at - i0 as f32;
    let py = ys[i0] + (ys[i1] - ys[i0]) * k;
    let accent = mix(f.act_color(acts[i0]), f.act_color(acts[i1]), k);
    f.p.rrect(12.0, py, 242.0, 54.0, 10.0, Some(mix(accent, pal.surface, 0.88)), None);
    f.p.rrect(10.0, py + 14.0, 4.0, 26.0, 2.0, Some(accent), None);
    let item = |f: &mut Frame, y: f32, icon: &str, label: &str, on: bool, act: Act, tint: Option<Rgb>| {
        let r = (12.0, y, 242.0, 54.0);
        let t = f.hover_t(&act, !on);
        let lift = super::super::anim::back_out(f.icon_t(&act, true)) * 2.0;
        f.hover_fill(r, 10.0, pal.alt, t);
        let c = if on && tint.is_none() { pal.primary } else { pal.text };
        if let Some(t) = tint {
            // L'icona dentro una piastrella del colore dell'attività: piena quando è scelta.
            f.tile(26.0, y + 9.0 - lift, 36.0, icon, t, on);
        } else {
            f.icon(icon, 32.0, y + 15.0 - lift, 24.0, c);
        }
        f.text(label, 78.0, y, 170.0, 54.0, 19.0, if on { SEMI } else { SANS }, c);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    };
    for (a, y) in acts.iter().zip(ys.iter().copied()) {
        let tint = Some(f.act_color(*a));
        item(f, y, a.icon(), a.name(), app.activity == *a, Act::Activity(*a), tint);
    }
    let bottom = f.h - 1024.0;
    f.hline(28.0, 835.0 + bottom, 214.0);
    item(f, 862.0 + bottom, ic::HELP, "Guida", false, Act::OpenGuide, None);
    item(f, 930.0 + bottom, ic::SETTINGS, "Impostazioni", false, Act::OpenSettings, None);
}

pub fn draw(f: &mut Frame, app: &App) {
    nav(f, app);
    match (app.activity, f.simple) {
        (Activity::Convert, false) => standard(f, app),
        (Activity::Convert, true) => simplified(f, app),
        (a, simple) => {
            let pal = f.pal;
            let (x, y) = (NAV_W, TOP);
            f.p.rrect(x, y, f.w - x + 20.0, f.h - y + 20.0, 10.0, Some(pal.work), Some((pal.border, 1.0)));
            let r = (x + 28.0, y + 24.0, f.w - x - 52.0, f.h - y - 44.0);
            match a {
                Activity::Transcribe => transcribe_page(f, app, r, simple),
                Activity::Firma => firma_page(f, app, r, simple),
                Activity::YouTube => links_page(f, app, r, false, simple),
                Activity::Web => links_page(f, app, r, true, simple),
                Activity::Convert => {}
            }
        }
    }
}

fn standard(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let (w, h) = (f.w, f.h);
    let foot_y = h - 156.0;
    let insp_w = 365.0;
    let insp_x = w - 21.0 - insp_w;
    let q_x = 296.0;
    let q_w = insp_x - 18.0 - q_x;
    // Fondo della zona di lavoro, con l'angolo arrotondato in alto a sinistra.
    f.p.rrect(NAV_W, TOP, w - NAV_W + 20.0, h - TOP + 20.0, 10.0, Some(pal.work), Some((pal.border, 1.0)));
    f.text("Conversioni", 296.0, 80.0, 600.0, 56.0, 42.0, SEMI, pal.text);
    super::notice(f, app, (760.0, 88.0, (insp_x + insp_w - 760.0).min(720.0), 44.0), 15.0);

    // Le categorie, schede con icona. Prima le misure, poi la scheda scelta (che scivola dalla
    // vecchia alla nuova prendendone il colore), poi icone e nomi sopra.
    let mut tabs: Vec<(R, f32, Option<String>, f32)> = Vec::new();
    let mut tx = 296.0;
    for (i, cat) in conversioni::CATEGORIES.iter().enumerate() {
        let on = i == app.category;
        // Nel tema base la scheda scelta è in semigrassetto: la misura deve già tenerne conto.
        let tw = f.measure(cat.name, 18.0, SEMI);
        let count = app.files.iter().filter(|x| !x.voice_only && x.kind == cat.kind).count();
        let badge = (count > 0 && !on).then(|| count.to_string());
        let bw = badge.as_ref().map(|b| f.measure(b, 12.0, SEMI) + 12.0).unwrap_or(0.0).max(if badge.is_some() { 20.0 } else { 0.0 });
        let r = (tx, 150.0, 16.0 + 24.0 + 14.0 + tw + 16.0 + if bw > 0.0 { bw + 6.0 } else { 0.0 }, 62.0);
        tabs.push((r, tw, badge, bw));
        tx += r.2 + 6.0;
    }
    let cats = conversioni::CATEGORIES;
    let last = (tabs.len() - 1) as f32;
    let at = f.glide(&Act::Category(usize::MAX), app.category as f32).clamp(0.0, last);
    let (i0, i1) = (at.floor() as usize, at.ceil() as usize);
    let k = at - i0 as f32;
    let (a, b) = (tabs[i0].0, tabs[i1].0);
    let sel = (a.0 + (b.0 - a.0) * k, 150.0, a.2 + (b.2 - a.2) * k, 62.0);
    let kc_sel = mix(f.kind_color(cats[i0].kind), f.kind_color(cats[i1].kind), k);
    for (i, (r, _, _, _)) in tabs.iter().enumerate() {
        if i != app.category {
            let t = f.hover_t(&Act::Category(i), true);
            let kc = f.kind_color(cats[i].kind);
            f.hover_fill((r.0, r.1, r.2, 52.0), 8.0, mix(kc, pal.surface, 0.9), t);
        }
    }
    f.p.rrect(sel.0, sel.1, sel.2, sel.3 + 8.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    f.p.fill(sel.0 + 1.0, sel.1 + sel.3 - 2.0, sel.2 - 2.0, 12.0, pal.surface);
    f.p.rrect(sel.0, sel.1 + 49.0, sel.2, 3.0, 1.5, Some(kc_sel), None);
    for (i, (cat, (r, tw, badge, bw))) in cats.iter().zip(tabs).enumerate() {
        let on = i == app.category;
        // Nel tema base ogni tipo di file ha il suo colore (icona, linea sotto la scheda, numero).
        let kc = f.kind_color(cat.kind);
        let ip = f.icon_t(&Act::Category(i), !on);
        f.icon_live(super::category_icon(cat.kind), r.0 + 16.0, 164.0, 24.0, kc, ip);
        f.text(cat.name, r.0 + 54.0, 150.0, tw + 4.0, 52.0, 18.0, if on { SEMI } else { SANS }, pal.text);
        if let Some(badge) = badge {
            let bx = r.0 + 54.0 + tw + 8.0;
            f.p.rrect(bx, 167.0, bw, 20.0, 10.0, Some(mix(kc, pal.surface, 0.85)), None);
            f.text_c(&badge, (bx, 167.0, bw, 20.0), 12.0, SEMI, kc);
        }
        f.hit((r.0, r.1, r.2, 52.0), Act::Category(i));
    }

    // La coda.
    let q = (q_x, 212.0, q_w, foot_y - 20.0 - 212.0);
    f.card(q, 12.0);
    let visible = app.visible();
    let pdf_mode = if super::pagine::shows_modes(f, app) { app.pdf_mode() } else { PdfMode::Convert };
    // Barra: Aggiungi file (con la freccia per la cartella), Aggiungi cartella, Rimuovi, conteggio.
    // «Pagine» ha al suo posto il PDF sul banco e i comandi delle pagine.
    let pages_mode = pdf_mode == PdfMode::Pages && !visible.is_empty();
    if !pages_mode {
        let add = (316.0, 234.0, 222.0, 40.0);
        let t = f.hover_t(&Act::AddFiles, true);
        let press = f.press_t(&Act::AddFiles, true);
        let ip = f.icon_t(&Act::AddFiles, true);
        f.glossy_fill_t(add, 6.0, pal.primary, t, press);
        f.icon_live(ic::ADD, add.0 + 14.0, add.1 + 8.0, 24.0, pal.on_primary, ip);
        f.text("Aggiungi file", add.0 + 46.0, add.1, 134.0, 40.0, 17.0, SEMI, pal.on_primary);
        f.p.fill_a(add.0 + add.2 - 38.0, add.1, 1.0, 40.0, pal.on_primary, 0.35);
        let ip = f.icon_t(&Act::AddMenu, true);
        f.icon_live(ic::CHEVRON_DOWN, add.0 + add.2 - 31.0, add.1 + 8.0, 24.0, pal.on_primary, ip);
        f.hit((add.0, add.1, add.2 - 38.0, add.3), Act::AddFiles);
        f.hit((add.0 + add.2 - 38.0, add.1, 38.0, add.3), Act::AddMenu);
        // Il conteggio sta a destra; se la finestra è stretta i due comandi restano con la sola icona
        // (prima «Aggiungi cartella», che è anche nella freccia di «Aggiungi file») invece di finirci sopra.
        let count = format!("{} file", visible.len());
        let count_w = f.measure(&count, 16.0, SANS);
        let count_x = q.0 + q.2 - 70.0 - count_w;
        let tool_w = |f: &Frame, label: &str| 48.0 + f.measure(label, 16.0, SANS);
        let fits = |f: &Frame, a: &str, b: &str| 569.0 + tool_w(f, a) + 28.0 + tool_w(f, b) + 12.0 <= count_x;
        let (folder, remove) = if fits(f, "Aggiungi cartella", "Rimuovi") {
            ("Aggiungi cartella", "Rimuovi")
        } else if fits(f, "", "Rimuovi") {
            ("", "Rimuovi")
        } else {
            ("", "")
        };
        let mut x = 569.0;
        x += f.tool(x, 234.0, 40.0, folder, ic::FOLDER, 16.0, Act::AddFolder, true) + 8.0;
        f.vline(x, 240.0, 30.0);
        x += 20.0;
        f.tool(x, 234.0, 40.0, remove, ic::TRASH, 16.0, Act::RemoveChecked, !visible.is_empty() && !app.busy());
        f.text_r(&count, (count_x - 4.0, 234.0, count_w + 4.0, 40.0), 16.0, SANS, pal.text);
        f.icon(ic::MENU, q.0 + q.2 - 56.0, 242.0, 24.0, pal.text);
    }

    if visible.is_empty() {
        super::drop_zone(f, app, (q.0 + 20.0, 300.0, q.2 - 40.0, q.3 - 108.0), true);
        super::others_link(f, app, q.0 + 20.0, q.1 + q.3 - 50.0, 30.0, 15.0);
    } else if pdf_mode == PdfMode::Pages {
        pages_card(f, app, q);
    } else if pdf_mode == PdfMode::Compress {
        compress_queue(f, app, q, &visible);
        super::others_link(f, app, q.0 + 20.0, q.1 + q.3 - 40.0, 30.0, 15.0);
    } else if pdf_mode != PdfMode::Convert {
        page_queue(f, app, q, &visible, pdf_mode == PdfMode::Merge);
        super::others_link(f, app, q.0 + 20.0, q.1 + q.3 - 40.0, 30.0, 15.0);
    } else {
        // Intestazione della tabella.
        let all = visible.iter().all(|&i| app.files[i].checked);
        f.checkbox(q.0 + 20.0, 306.0, 22.0, all, Some(Act::CheckAll));
        let name_w = (q.2 - 62.0 - 12.0 - 108.0 - 24.0 - 132.0 - 150.0 - 60.0).max(160.0);
        let col_size = q.0 + 62.0 + 12.0 + 108.0 + 24.0 + name_w;
        f.text("Nome file", q.0 + 74.0, 296.0, 200.0, 42.0, 15.0, SANS, pal.text);
        f.text("Dimensione", col_size, 296.0, 130.0, 42.0, 15.0, SANS, pal.text);
        f.text("Risoluzione", col_size + 132.0, 296.0, 140.0, 42.0, 15.0, SANS, pal.text);
        let list = (q.0, 338.0, q.2, q.1 + q.3 - 338.0 - 44.0);
        let rows = visible.clone();
        super::scrolled(f, list, Scroll::Queue, |f| {
            for (n, &i) in rows.iter().enumerate() {
                let y = list.1 + n as f32 * 96.0;
                if !f.p.shows_y(y, 96.0) {
                    continue;
                }
                let file = &app.files[i];
                f.hline(q.0, y, q.2);
                if app.selected == Some(i) {
                    f.p.fill(q.0 + 1.0, y + 1.0, q.2 - 2.0, 95.0, mix(pal.alt, pal.surface, 0.5));
                }
                f.hit((q.0, y, q.2, 96.0), Act::Select(i));
                f.checkbox(q.0 + 20.0, y + 37.0, 22.0, file.checked, Some(Act::Check(i)));
                f.thumb(app, i, (q.0 + 74.0, y + 13.0, 108.0, 70.0), 4.0);
                let tx = q.0 + 206.0;
                super::file_name(f, app, i, tx, y + 20.0, name_w - 12.0, 24.0, 17.0, SEMI, pal.text);
                let sub = if file.state == FileState::Waiting { super::waiting_line(app, i) } else { file.message.clone() };
                let sub_c = match file.state {
                    FileState::Failed => pal.bad,
                    FileState::Done => pal.ok,
                    _ => pal.muted,
                };
                f.text(&sub, tx, y + 48.0, name_w - 12.0, 22.0, 16.0, SANS, sub_c);
                f.row_bar(file, tx, y + 77.0, name_w - 12.0, 6.0);
                let (size, extra) = super::file_meta(app, i);
                f.text(&size, col_size, y, 130.0, 96.0, 16.0, SANS, pal.muted);
                f.text(&extra, col_size + 132.0, y, 140.0, 96.0, 16.0, SANS, pal.muted);
                let cx = q.0 + q.2 - 44.0;
                if file.state == FileState::Waiting {
                    let act = Act::Remove(i);
                    if f.hovered(&act) {
                        f.p.circle(cx + 12.0, y + 48.0, 18.0, Some(pal.alt), None);
                    }
                    f.icon(ic::CLOSE, cx, y + 36.0, 24.0, pal.text);
                    f.hit((cx - 8.0, y + 28.0, 40.0, 40.0), act);
                } else {
                    f.state_icon(file.state, cx + 12.0, y + 48.0, 14.0);
                    if let Some(out) = &file.output {
                        f.hit((cx - 8.0, y + 28.0, 40.0, 40.0), Act::OpenPath(out.clone()));
                    }
                }
            }
            let end = list.1 + rows.len() as f32 * 96.0;
            f.hline(q.0, end, q.2);
            rows.len() as f32 * 96.0 + 1.0
        });
        super::others_link(f, app, q.0 + 20.0, q.1 + q.3 - 40.0, 30.0, 15.0);
    }

    // L'ispettore.
    let ir = (insp_x, 212.0, insp_w, foot_y - 20.0 - 212.0);
    f.card(ir, 12.0);
    let modes = super::pagine::shows_modes(f, app);
    let head = if modes { "Cosa vuoi fare" } else { "Impostazioni di conversione" };
    f.text(head, insp_x + 24.0, 234.0, insp_w - 48.0, 40.0, 18.0, SEMI, pal.text);
    let top = if modes { 282.0 } else { 282.0 };
    let body = (insp_x + 1.0, top, insp_w - 2.0, ir.1 + ir.3 - top - 8.0);
    super::scrolled(f, body, Scroll::Inspector, |f| super::options_panel_ex(f, app, insp_x + 24.0, if modes { 290.0 } else { 298.0 }, insp_w - 48.0, OPT_STD, true, false) + 20.0);

    footer(f, app, foot_y);
}

/// La coda di «Unisci» (ordine, maniglia, frecce, pagine di ogni file, il file unito) e di «Dividi»
/// (la riga scelta si apre sulla striscia delle pagine). Misure della tavola A.
fn page_queue(f: &mut Frame, app: &App, q: R, rows: &[usize], merge: bool) {
    let pal = f.pal;
    const RH: f32 = 96.0;
    let all = rows.iter().all(|&i| app.files[i].checked);
    f.checkbox(q.0 + 20.0, 306.0, 22.0, all, Some(Act::CheckAll));
    let col_pages = q.0 + q.2 - 276.0;
    let col_size = q.0 + q.2 - 156.0;
    let name_x = if merge { q.0 + 156.0 } else { q.0 + 150.0 };
    let name_w = (col_pages - name_x - 20.0).max(140.0);
    f.text(if merge { "Ordine e nome file" } else { "Nome file" }, q.0 + 74.0, 296.0, 260.0, 42.0, 15.0, SANS, pal.text);
    f.text("Pagine", col_pages + 4.0, 296.0, 110.0, 42.0, 15.0, SANS, pal.text);
    f.text("Dimensione", col_size, 296.0, 130.0, 42.0, 15.0, SANS, pal.text);
    let list = (q.0, 338.0, q.2, q.1 + q.3 - 338.0 - 44.0);
    let split_i = if merge { None } else { super::pagine::split_file(app) };
    let targets = app.targets();
    let rows = rows.to_vec();
    super::scrolled(f, list, Scroll::Queue, |f| {
        let mut y = list.1;
        let mut dragged: Option<(usize, usize, f32)> = None;
        let row = |f: &mut Frame, n: usize, i: usize, y: f32, lifted: bool| {
            let file = &app.files[i];
            let open = split_i == Some(i);
            if lifted {
                f.p.shadow(q.0 + 8.0, y, q.2 - 16.0, RH, 8.0, 14.0, 0.25, 6.0);
                f.p.rrect(q.0 + 8.0, y, q.2 - 16.0, RH, 8.0, Some(pal.surface), Some((pal.primary, 1.5)));
            } else {
                f.hline(q.0, y, q.2);
                if app.selected == Some(i) || open {
                    f.p.fill(q.0 + 1.0, y + 1.0, q.2 - 2.0, RH - 1.0, mix(pal.alt, pal.surface, 0.5));
                }
                f.hit((q.0, y, q.2, RH), Act::Select(i));
            }
            f.checkbox(q.0 + 20.0, y + 37.0, 22.0, file.checked, Some(Act::Check(i)));
            let sub_c = match file.state {
                FileState::Failed => pal.bad,
                FileState::Done => pal.ok,
                _ => pal.muted,
            };
            if merge {
                super::pagine::grip(f, app, i, q.0 + 50.0, y + 48.0, RH);
                let pos = targets.iter().position(|&t| t == i);
                let mark = pos.map(|p| super::pagine::file_color(pal, p));
                f.thumb(app, i, (q.0 + 82.0, y + 13.0, 52.0, 70.0), 4.0);
                if let Some(c) = mark {
                    f.p.rrect(q.0 + 82.0, y + 13.0, 4.0, 70.0, 2.0, Some(c), None);
                }
                if let Some(p) = pos {
                    super::pagine::order_badge(f, p, q.0 + 82.0, y + 20.0, 14.0);
                }
                f.text(&file.name(), name_x, y + 20.0, name_w, 24.0, 17.0, SEMI, pal.text);
                f.text(&super::pagine::merge_sub(app, i), name_x, y + 48.0, name_w, 22.0, 16.0, SANS, sub_c);
                f.row_bar(file, name_x, y + 77.0, name_w, 6.0);
                f.field((col_pages, y + 26.0, 100.0, 40.0), Field::FilePages(i), &file.pages, "tutte", 15.0, false);
                if app.selected == Some(i) || lifted {
                    super::pagine::order_arrows(f, app, i, q.0 + q.2 - 104.0, y + 48.0, 24.0);
                }
            } else {
                f.thumb(app, i, (q.0 + 74.0, y + 13.0, 52.0, 70.0), 4.0);
                f.text(&file.name(), name_x, y + 20.0, name_w, 24.0, 17.0, SEMI, pal.text);
                let sub = if file.state == FileState::Waiting { super::waiting_line(app, i) } else { file.message.clone() };
                f.text(&sub, name_x, y + 48.0, name_w, 22.0, 16.0, SANS, sub_c);
                f.row_bar(file, name_x, y + 77.0, name_w, 6.0);
                let pages = app.page_count(i).map(|n| n.to_string()).unwrap_or_default();
                f.text(&pages, col_pages + 4.0, y, 100.0, RH, 16.0, SANS, pal.muted);
            }
            let (size, _) = super::file_meta(app, i);
            f.text(&size, col_size, y, 110.0, RH, 16.0, SANS, pal.muted);
            let cx = q.0 + q.2 - 44.0;
            if file.state == FileState::Waiting {
                let act = Act::Remove(i);
                if f.hovered(&act) {
                    f.p.circle(cx + 12.0, y + 48.0, 18.0, Some(pal.alt), None);
                }
                f.icon(ic::CLOSE, cx, y + 36.0, 24.0, pal.text);
                f.hit((cx - 8.0, y + 28.0, 40.0, 40.0), act);
            } else {
                f.state_icon(file.state, cx + 12.0, y + 48.0, 14.0);
                if let Some(out) = &file.output {
                    f.hit((cx - 8.0, y + 28.0, 40.0, 40.0), Act::OpenPath(out.clone()));
                }
            }
            let _ = n;
        };
        for (n, &i) in rows.iter().enumerate() {
            if merge {
                if let Some(dy) = super::pagine::drag_offset(f, i) {
                    dragged = Some((n, i, dy));
                }
            }
            let open = split_i == Some(i);
            let strip_h = if open { 
                // La striscia si misura disegnandola fuori vista la prima volta: qui basta l'altezza calcolata.
                strip_height(app, i, q.2 - 110.0)
            } else {
                0.0
            };
            let h = RH + strip_h;
            if f.p.shows_y(y, h) {
                if open {
                    f.p.fill(q.0 + 1.0, y + 1.0, q.2 - 2.0, h - 1.0, mix(pal.alt, pal.surface, 0.5));
                }
                row(f, n, i, y, false);
                if open {
                    super::pagine::split_strip(f, app, i, q.0 + 74.0, y + RH + 8.0, q.2 - 110.0, 86.0);
                }
            }
            y += h;
        }
        f.hline(q.0, y, q.2);
        if let Some((n, i, dy)) = dragged {
            // Dove andrà: la linea tra due righe, e la riga sollevata che segue il mouse.
            let to = (n as i32 + (dy / RH).round() as i32).clamp(0, rows.len() as i32 - 1) as usize;
            let line_y = list.1 + if to > n { (to + 1) as f32 * RH } else { to as f32 * RH };
            f.p.rrect(q.0 + 12.0, line_y - 2.0, q.2 - 24.0, 4.0, 2.0, Some(pal.primary), None);
            let ry = (list.1 + n as f32 * RH + dy).clamp(list.1 - RH / 2.0, y - RH / 2.0);
            row(f, n, i, ry, true);
        }
        let mut end = y + 18.0;
        if merge {
            end += f.para(
                "Trascina le righe dalla maniglia oppure usa le frecce: l'ordine della coda è l'ordine delle pagine nel file unito. Nel campo «Pagine» puoi scrivere per esempio 1-3,5.",
                q.0 + 24.0, end, q.2 - 48.0, 15.0, SANS, pal.muted, 1.45, 3,
            ) + 22.0;
            end += super::pagine::merged_strip(f, app, q.0 + 24.0, end, q.2 - 48.0, 46.0);
        } else if rows.len() > 1 {
            end += f.para(
                "La striscia si apre sotto il file selezionato: i tagli valgono per quel file, il campo «Intervalli» a destra per tutti quelli senza tagli.",
                q.0 + 24.0, end, q.2 - 48.0, 15.0, SANS, pal.muted, 1.45, 3,
            );
        }
        end - list.1 + 16.0
    });
}

/// «Pagine» al posto della coda: il PDF sul banco (si cambia dall'elenco), la barra dei comandi e le
/// pagine; con numeri o filigrana accesi, la pagina vera in grande e la striscia. Misure della tavola A.
fn pages_card(f: &mut Frame, app: &App, q: R) {
    let pal = f.pal;
    let Some(i) = super::pagine::split_file(app) else { return };
    let name_w = 330.0f32.min(q.2 - 520.0).max(180.0);
    f.select((q.0 + 20.0, 228.0, name_w, 44.0), &app.files[i].name(), None, 16.0, Act::PagesFileMenu);
    let tools_w = super::pagine::pages_tools(f, app, i, q.0 + q.2 - 20.0, 230.0, 40.0, 16.0);
    let count_x = q.0 + 20.0 + name_w + 12.0;
    super::pagine::pages_count(f, app, i, count_x, 228.0, (q.0 + q.2 - 20.0 - tools_w - 12.0 - count_x).max(60.0), 44.0, 15.0);
    f.hline(q.0 + 20.0, 286.0, q.2 - 40.0);
    let stamped = !conversioni::options_from(&app.values).pdf_edit.stamp.is_empty();
    let hint_y = q.1 + q.3 - 50.0;
    let list = (q.0 + 1.0, 288.0, q.2 - 2.0, hint_y - 288.0 - 4.0);
    super::scrolled(f, list, Scroll::Queue, |f| {
        if stamped {
            let big = (list.3 - 150.0).clamp(220.0, 410.0);
            super::pagine::stamp_view(f, app, i, q.0 + 24.0, list.1 + 12.0, q.2 - 48.0, big, 64.0) + 24.0
        } else {
            // Il banco centrato nella scheda, con le pagine più grandi che ci stanno tutte.
            let n = app.board(i).map_or(1, |b| b.pages.len().min(super::BOARD_MAX));
            let pw = board_page_width(q.2 - 68.0, list.3 - 57.0 - 70.0, n);
            let per_row = (((q.2 - 68.0 + 38.0) / (pw + 38.0)) as usize).clamp(1, n.max(1));
            let used = per_row as f32 * (pw + 38.0) - 38.0;
            let x = q.0 + ((q.2 - used) / 2.0).round();
            super::pagine::board(f, app, i, x, list.1 + 57.0, used, pw) + 70.0
        }
    });
    let hint = if stamped {
        "Tocca una pagina della striscia per vederla in grande; i comandi in alto valgono per quella."
    } else {
        "Clic su una pagina per sceglierla (ancora per toglierla dalla scelta), trascina per spostarla. I pulsanti in alto valgono per la scelta: la ruotano di 90°, la tolgono o la copiano in un PDF nuovo."
    };
    f.para(hint, q.0 + 24.0, hint_y, q.2 - 48.0, 15.0, SANS, pal.muted, 1.4, 2);
}

/// La larghezza delle pagine del banco: la più grande (fino al lato delle miniature, 240 px in
/// altezza) con cui `n` pagine stanno tutte in `w` × `h`; se non ci stanno, 140 e si scorre.
fn board_page_width(w: f32, h: f32, n: usize) -> f32 {
    let n = n.max(1);
    let mut pw = 176.0f32;
    while pw > 140.0 {
        let per_row = (((w + 38.0) / (pw + 38.0)) as usize).max(1);
        if n.div_ceil(per_row) as f32 * ((pw * 1.36).round() + 70.0) <= h {
            return pw;
        }
        pw -= 4.0;
    }
    140.0
}

/// La coda di «Comprimi»: per ogni PDF il peso adesso, la stima dopo e il risparmio; sotto il totale.
fn compress_queue(f: &mut Frame, app: &App, q: R, rows: &[usize]) {
    let pal = f.pal;
    const RH: f32 = 112.0;
    let all = rows.iter().all(|&i| app.files[i].checked);
    f.checkbox(q.0 + 20.0, 306.0, 22.0, all, Some(Act::CheckAll));
    let bars_x = q.0 + (q.2 * 0.56).round();
    let bars_w = (q.2 - 140.0 - (bars_x - q.0) - 24.0).clamp(120.0, 210.0);
    let name_w = bars_x - (q.0 + 138.0) - 24.0;
    f.text("Nome file", q.0 + 74.0, 296.0, 200.0, 42.0, 15.0, SANS, pal.text);
    f.text("Peso adesso e stima dopo", bars_x, 296.0, 260.0, 42.0, 15.0, SANS, pal.text);
    f.text("Risparmio", q.0 + q.2 - 130.0, 296.0, 100.0, 42.0, 15.0, SANS, pal.text);
    let max = rows.iter().filter_map(|&i| app.media_of(&app.files[i].path).map(|m| m.bytes)).max().unwrap_or(1);
    let total_h = 110.0;
    let list = (q.0, 338.0, q.2, q.1 + q.3 - 338.0 - 44.0 - total_h);
    let rows = rows.to_vec();
    super::scrolled(f, list, Scroll::Queue, |f| {
        for (n, &i) in rows.iter().enumerate() {
            let y = list.1 + n as f32 * RH;
            if !f.p.shows_y(y, RH) {
                continue;
            }
            let file = &app.files[i];
            f.hline(q.0, y, q.2);
            if app.selected == Some(i) {
                f.p.fill(q.0 + 1.0, y + 1.0, q.2 - 2.0, RH - 1.0, mix(pal.alt, pal.surface, 0.5));
            }
            f.hit((q.0, y, q.2, RH), Act::Select(i));
            f.checkbox(q.0 + 20.0, y + 45.0, 22.0, file.checked, Some(Act::Check(i)));
            f.thumb(app, i, (q.0 + 68.0, y + 18.0, 52.0, 70.0), 4.0);
            f.text(&file.name(), q.0 + 138.0, y + 26.0, name_w, 24.0, 17.0, SEMI, pal.text);
            let sub_c = match file.state {
                FileState::Failed => pal.bad,
                FileState::Done => pal.ok,
                _ => pal.muted,
            };
            f.text(&super::pagine::squeeze_sub(app, i), q.0 + 138.0, y + 56.0, name_w, 22.0, 16.0, SANS, sub_c);
            f.row_bar(file, q.0 + 138.0, y + 88.0, name_w, 6.0);
            super::pagine::squeeze_bars(f, app, i, bars_x, y + 20.0, bars_w, max, 14.0);
            super::pagine::saving_pill(f, app, i, q.0 + q.2 - 60.0, y + RH / 2.0, 16.0);
            let cx = q.0 + q.2 - 44.0;
            if file.state == FileState::Waiting {
                let act = Act::Remove(i);
                if f.hovered(&act) {
                    f.p.circle(cx + 12.0, y + RH / 2.0, 18.0, Some(pal.alt), None);
                }
                f.icon(ic::CLOSE, cx, y + RH / 2.0 - 12.0, 24.0, pal.text);
                f.hit((cx - 8.0, y + RH / 2.0 - 20.0, 40.0, 40.0), act);
            } else {
                f.state_icon(file.state, cx + 12.0, y + RH / 2.0, 14.0);
                if let Some(out) = &file.output {
                    f.hit((cx - 8.0, y + RH / 2.0 - 20.0, 40.0, 40.0), Act::OpenPath(out.clone()));
                }
            }
        }
        f.hline(q.0, list.1 + rows.len() as f32 * RH, q.2);
        rows.len() as f32 * RH + 1.0
    });
    let ty = list.1 + list.3;
    f.hline(q.0 + 1.0, ty, q.2 - 2.0);
    match super::pagine::squeeze_total(app) {
        Some((b, a, real)) => {
            let from = format!("Da {} a", crate::engine::size_words(b));
            let to = if real { crate::engine::size_words(a) } else { format!("circa {}", crate::engine::size_words(a)) };
            let fw = f.measure(&from, 18.0, SANS) + f.measure(" ", 18.0, SANS);
            f.text(&from, q.0 + 24.0, ty + 14.0, fw + 2.0, 30.0, 18.0, SANS, pal.text);
            f.text(&to, q.0 + 24.0 + fw, ty + 14.0, 260.0, 30.0, 18.0, BOLD, pal.primary);
            let note = if real { "Pesi veri dei file salvati." } else { "Stima vera, fatta comprimendo in memoria." };
            f.text_r(note, (q.0 + 24.0 + fw + 200.0, ty + 16.0, q.2 - 48.0 - fw - 200.0, 26.0), 14.0, SANS, pal.muted);
        }
        None => {
            let text = if app.targets().is_empty() { "Spunta i PDF da comprimere." } else { "Si calcolano le stime…" };
            f.text(text, q.0 + 24.0, ty + 14.0, q.2 - 48.0, 30.0, 18.0, SANS, pal.muted);
        }
    }
    f.para("Se un file non si riesce a ridurre, resta com'è e la riga lo dice: non viene mai salvato un PDF più pesante dell'originale.", q.0 + 24.0, ty + 54.0, q.2 - 48.0, 15.0, SANS, pal.muted, 1.4, 2);
}

/// L'altezza della striscia di «Dividi» sotto la riga (le stesse misure di `pagine::split_strip`).
fn strip_height(app: &App, i: usize, w: f32) -> f32 {
    let Some(total) = app.page_count(i) else { return 34.0 };
    let pw = 86.0f32;
    let ph = (pw * 1.35).round();
    let unit = pw + (pw * 0.5).round();
    let per_row = ((w / unit) as usize).max(1);
    let shown = total.min(super::STRIP_MAX);
    let rows = shown.div_ceil(per_row);
    let more = if total > shown { 28.0 } else { 0.0 };
    8.0 + 30.0 + rows as f32 * (ph + 6.0 + 18.0 + 4.0 + 22.0 + 14.0) + more + 8.0
}

fn footer(f: &mut Frame, app: &App, y: f32) {
    let pal = f.pal;
    let w = f.w;
    // Il piede è una fascia chiara appoggiata sopra la zona di lavoro.
    f.p.shadow(NAV_W, y, w - NAV_W, f.h - y, 0.0, 8.0, 0.07, -2.0);
    f.p.fill(NAV_W, y, w - NAV_W, f.h - y, pal.surface);
    f.hline(NAV_W, y, w - NAV_W);
    // Griglia del piede: etichetta (riga di 20) sopra la scelta (42), nota sotto la scelta, tutto
    // a filo con il contenuto della coda (x 316). Il pulsante va dall'etichetta al fondo della scelta.
    let go_w = 240.0;
    let go_x = w - 24.0 - go_w;
    let (lab_y, ctl_y, note_y) = (y + 20.0, y + 46.0, y + 96.0);
    let col = |f: &mut Frame, x: f32, icon: &str, label: &str| {
        f.icon(icon, x, lab_y, 20.0, pal.muted);
        f.text(label, x + 28.0, lab_y, 300.0, 20.0, 15.0, SEMI, pal.text);
    };
    let x1 = 316.0;
    let dest_w = 300.0f32.min(go_x - 48.0 - 200.0 - 48.0 - x1).max(200.0);
    col(f, x1, ic::FOLDER, "Cartella di destinazione");
    super::dest_select(f, app, (x1, ctl_y, dest_w, 42.0), 16.0);
    let ex = x1 + dest_w + 24.0;
    f.vline(ex, y + 20.0, 110.0);
    let x2 = ex + 24.0;
    let sel_w = (go_x - 48.0 - x2).clamp(160.0, 200.0);
    f.para(&super::dest_note(app), x1, note_y, ex - 24.0 - x1, 14.0, SANS, pal.muted, 1.4, 2);
    col(f, x2, ic::COPY, "Se esiste già");
    super::exists_select(f, app, (x2, ctl_y, sel_w, 42.0), 16.0);
    f.para(&app.exists_example(), x2, note_y, sel_w + 40.0, 14.0, SANS, pal.muted, 1.4, 2);
    let label = super::go_label(app);
    let ready = app.busy() || !app.targets().is_empty();
    let go = (go_x, lab_y, go_w, ctl_y + 42.0 - lab_y);
    if app.busy() {
        if let Some(p) = app.progress() {
            f.p.rrect(go_x, note_y, go_w, 6.0, 3.0, Some(pal.border), None);
            f.p.rrect(go_x, note_y, go_w * p, 6.0, 3.0, Some(pal.primary), None);
        }
    }
    f.primary(go, &label, Some(super::go_icon(app)), 20.0, Act::Start, ready && !app.busy());
    super::go_extras(f, app, (go.0, go.1, go.2, go.3 - 2.0), false);
}

fn simplified(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let (w, h) = (f.w, f.h);
    let card_x = 296.0;
    let card_w = w - 22.0 - card_x;
    f.p.rrect(272.0, 64.0, w - 272.0 - 14.0, h - 64.0 - 16.0, 10.0, Some(pal.work), Some((pal.border, 1.0)));
    f.text("Conversioni", 314.0, 92.0, 500.0, 66.0, 50.0, SEMI, pal.text);
    mode_switch(f, (w - 466.0, 96.0, 427.0, 61.0));
    // Correzione 6: «Aggiungi file» resta sempre raggiungibile, anche a coda piena.
    let add = (w - 716.0, 104.0, 220.0, 46.0);
    f.button(add, "Aggiungi file", Some(ic::ADD), 18.0, Act::AddFiles);
    let card = (card_x, 188.0, card_w, h - 188.0 - 27.0);
    f.card(card, 10.0);
    let inner_x = card_x + 28.0;
    let inner_w = card_w - 56.0;
    let visible = app.visible();
    let list_h = (h - 1024.0 + 407.0).max(160.0);
    if visible.is_empty() {
        super::drop_zone(f, app, (inner_x, 215.0, inner_w, list_h), true);
    } else {
        let list = (inner_x, 215.0, inner_w, list_h);
        f.p.rrect(list.0, list.1, list.2, list.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
        let rows = visible.clone();
        super::scrolled(f, (list.0 + 1.0, list.1 + 1.0, list.2 - 2.0, list.3 - 2.0), Scroll::Queue, |f| {
            for (n, &i) in rows.iter().enumerate() {
                let y = list.1 + n as f32 * 134.0;
                if !f.p.shows_y(y, 134.0) {
                    continue;
                }
                let file = &app.files[i];
                if n > 0 {
                    f.hline(list.0, y, list.2);
                }
                f.thumb(app, i, (list.0 + 26.0, y + 18.0, 140.0, 98.0), 6.0);
                super::file_name(f, app, i, list.0 + 196.0, y + 36.0, list.2 - 520.0, 30.0, 22.0, SEMI, pal.text);
                let sub = if file.state == FileState::Waiting { super::waiting_line(app, i) } else { file.message.clone() };
                let sub_c = match file.state {
                    FileState::Failed => pal.bad,
                    FileState::Done => pal.ok,
                    _ => pal.muted,
                };
                f.text(&sub, list.0 + 196.0, y + 70.0, list.2 - 520.0, 28.0, 20.0, SANS, sub_c);
                f.row_bar(file, list.0 + 196.0, y + 106.0, list.2 - 520.0, 8.0);
                let (size, _) = super::file_meta(app, i);
                f.text(&size, list.0 + list.2 - 300.0, y, 200.0, 134.0, 20.0, SANS, pal.muted);
                // «Un solo PDF»: il numero del posto e le frecce per cambiare l'ordine.
                if app.kind() == Kind::Pdf && app.action == "merge" && rows.len() > 1 {
                    super::pagine::grip(f, app, i, list.0 + 1.0, y + 67.0, 134.0);
                    super::pagine::order_badge(f, n, list.0 + 30.0, y + 24.0, 16.0);
                    super::pagine::order_arrows(f, app, i, list.0 + list.2 - 150.0, y + 67.0, 30.0);
                    super::pagine::drop_line(f, n, i, rows.len(), list.1, 134.0, list.0, list.2);
                }
                let cx = list.0 + list.2 - 60.0;
                if file.state == FileState::Waiting {
                    let act = Act::Remove(i);
                    if f.hovered(&act) {
                        f.p.circle(cx + 15.0, y + 67.0, 22.0, Some(pal.alt), None);
                    }
                    f.icon(ic::CLOSE, cx, y + 52.0, 30.0, pal.text);
                    f.hit((cx - 6.0, y + 46.0, 42.0, 42.0), act);
                } else {
                    f.state_icon(file.state, cx + 15.0, y + 67.0, 17.0);
                    if let Some(out) = &file.output {
                        f.hit((cx - 6.0, y + 46.0, 42.0, 42.0), Act::OpenPath(out.clone()));
                    }
                }
            }
            rows.len() as f32 * 134.0
        });
    }
    let y0 = 215.0 + list_h;
    let others_w = super::others_link(f, app, inner_x, y0 + 8.0, 28.0, 16.0);
    let _ = others_w;
    // «Converti in:» con la scelta grande.
    let fy = y0 + 46.0;
    f.text("Converti in:", inner_x + 2.0, fy, 160.0, 68.0, 24.0, SEMI, pal.text);
    let title = super::action_title(app, true);
    f.select_voce((inner_x + 164.0, fy, inner_w - 164.0, 68.0), &title, app.action_voce(), 24.0, Act::FormatMenu);
    let label = super::go_label(app);
    let ready = !app.targets().is_empty();
    let go = (inner_x, fy + 108.0, inner_w, 88.0);
    f.primary(go, &label, Some(super::go_icon(app)), 28.0, Act::Start, ready && !app.busy());
    super::go_extras(f, app, (inner_x, fy + 108.0, inner_w, 88.0 + 8.0), false);
    if app.busy() {
        if let Some(p) = app.progress() {
            f.p.rrect(inner_x, fy + 108.0 + 96.0, inner_w * p, 5.0, 2.5, Some(pal.primary), None);
        }
    }
    let sy = fy + 108.0 + 88.0 + 30.0 + if app.spia().is_some() || app.finished() { 30.0 } else { 0.0 };
    if !super::notice(f, app, (inner_x, sy - 8.0, inner_w, 46.0), 17.0) {
        let line = super::summary_line(app);
        f.text_c(&line, (inner_x, sy, inner_w, 30.0), 20.0, SANS, pal.muted);
        if app.customized() {
            let tw = f.measure("Torna alle consigliate", 16.0, SEMI);
            let r = (inner_x + (inner_w - tw) / 2.0, sy + 32.0, tw, 24.0);
            f.p.text("Torna alle consigliate", r.0, r.1, r.2 + 2.0, r.3, 16.0, SEMI, pal.primary, Align::Left);
            f.hit(r, Act::ResetRecommended);
        }
    }
    let _ = HitKind::Button;
}

/// Il selettore Semplificata | Standard (in alto a destra nelle viste semplificate).
pub fn mode_switch(f: &mut Frame, r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let half = r.2 / 2.0;
    for (i, (label, icon, simple)) in [("Semplificata", ic::MENU, true), ("Standard", ic::SETTINGS, false)].into_iter().enumerate() {
        let x = r.0 + i as f32 * half;
        let on = f.simple == simple;
        if on {
            f.p.rrect(x - 1.0, r.1 - 1.0, half + 2.0, r.3 + 2.0, 8.0, Some(mix(pal.alt, pal.surface, 0.4)), Some((pal.primary, 1.5)));
        }
        let c = if on { pal.primary } else { pal.text };
        let size = (r.3 * 0.31).round();
        let tw = f.measure(label, size, if on { SEMI } else { SANS });
        let isz = (size * 1.25).round();
        let sx = x + (half - tw - isz - 14.0) / 2.0;
        f.icon(icon, sx, r.1 + (r.3 - isz) / 2.0, isz, c);
        f.text(label, sx + isz + 14.0, r.1, tw + 4.0, r.3, size, if on { SEMI } else { SANS }, c);
        f.hit((x, r.1, half, r.3), Act::SetSimple(simple));
    }
}
