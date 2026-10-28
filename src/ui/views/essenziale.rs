//! Tema 1, «Windows essenziale»: attività a sinistra, categorie in schede, coda con ispettore a
//! destra e piede con destinazione e pulsante. La Semplificata ha una sola area operativa.
//! Misure dalla tavola 01 (1536×1024); la coda si allarga, navigazione, ispettore e piede restano.

use super::*;
use crate::ui::core::{HitKind, Scroll};
use crate::ui::model::FileState;

const NAV_W: f32 = 270.0;
const TOP: f32 = 56.0;

fn nav(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.clear(pal.bg);
    f.caption((0.0, 0.0, f.w, TOP));
    f.brand(26.0, 15.0, 26.0, 18.0, 12.0, None);
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
    let item = |f: &mut Frame, y: f32, icon: &str, label: &str, on: bool, act: Act| {
        let r = (12.0, y, 242.0, 54.0);
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.nav_sel), None);
            f.p.rrect(10.0, y + 14.0, 4.0, 26.0, 2.0, Some(pal.primary), None);
        } else if f.hovered(&act) {
            f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.alt), None);
        }
        let c = if on { pal.primary } else { pal.text };
        f.icon(icon, 32.0, y + 15.0, 24.0, c);
        f.text(label, 78.0, y, 170.0, 54.0, 19.0, SANS, c);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    };
    let ys = [78.0, 142.0, 210.0, 278.0];
    for (a, y) in Activity::ALL.iter().zip(ys) {
        item(f, y, a.icon(), a.name(), app.activity == *a, Act::Activity(*a));
    }
    let bottom = f.h - 1024.0;
    f.hline(28.0, 835.0 + bottom, 214.0);
    item(f, 862.0 + bottom, ic::HELP, "Guida", false, Act::OpenGuide);
    item(f, 930.0 + bottom, ic::SETTINGS, "Impostazioni", false, Act::OpenSettings);
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
    let foot_y = h - 166.0;
    let insp_w = 365.0;
    let insp_x = w - 21.0 - insp_w;
    let q_x = 296.0;
    let q_w = insp_x - 18.0 - q_x;
    // Fondo della zona di lavoro, con l'angolo arrotondato in alto a sinistra.
    f.p.rrect(NAV_W, TOP, w - NAV_W + 20.0, h - TOP + 20.0, 10.0, Some(pal.work), Some((pal.border, 1.0)));
    f.text("Conversioni", 298.0, 80.0, 600.0, 56.0, 42.0, SEMI, pal.text);
    super::notice(f, app, (760.0, 88.0, (insp_x + insp_w - 760.0).min(720.0), 44.0), 15.0);

    // Le categorie, schede con icona.
    let mut tx = 298.0;
    for (i, cat) in conversioni::CATEGORIES.iter().enumerate() {
        let on = i == app.category;
        let tw = f.measure(cat.name, 18.0, SANS);
        let count = app.files.iter().filter(|x| !x.voice_only && x.kind == cat.kind).count();
        let badge = (count > 0 && !on).then(|| count.to_string());
        let bw = badge.as_ref().map(|b| f.measure(b, 12.0, SEMI) + 12.0).unwrap_or(0.0).max(if badge.is_some() { 20.0 } else { 0.0 });
        let r = (tx, 150.0, 16.0 + 24.0 + 14.0 + tw + 16.0 + if bw > 0.0 { bw + 6.0 } else { 0.0 }, 62.0);
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3 + 8.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            f.p.fill(r.0 + 1.0, r.1 + r.3 - 2.0, r.2 - 2.0, 12.0, pal.surface);
            f.p.rrect(r.0, r.1 + 49.0, r.2, 3.0, 1.5, Some(pal.primary), None);
        } else if f.hovered(&Act::Category(i)) {
            f.p.rrect(r.0, r.1, r.2, 52.0, 8.0, Some(pal.alt), None);
        }
        let c = if on { pal.primary } else { pal.text };
        f.icon(super::category_icon(cat.kind), tx + 16.0, 164.0, 24.0, c);
        f.text(cat.name, tx + 54.0, 150.0, tw + 4.0, 52.0, 18.0, SANS, c);
        if let Some(badge) = badge {
            let bx = tx + 54.0 + tw + 8.0;
            f.p.rrect(bx, 167.0, bw, 20.0, 10.0, Some(mix(pal.primary, pal.surface, 0.85)), None);
            f.text_c(&badge, (bx, 167.0, bw, 20.0), 12.0, SEMI, pal.primary);
        }
        f.hit((r.0, r.1, r.2, 52.0), Act::Category(i));
        tx += r.2 + 6.0;
    }

    // La coda.
    let q = (q_x, 212.0, q_w, foot_y - 20.0 - 212.0);
    f.card(q, 12.0);
    let visible = app.visible();
    // Barra: Aggiungi file (con la freccia per la cartella), Aggiungi cartella, Rimuovi, conteggio.
    let add = (316.0, 234.0, 197.0, 40.0);
    let hot = f.hovered(&Act::AddFiles);
    f.p.rrect(add.0, add.1, add.2, add.3, 6.0, Some(if hot { mix(pal.primary, (0, 0, 0), 0.12) } else { pal.primary }), None);
    f.icon(ic::ADD, add.0 + 14.0, add.1 + 8.0, 24.0, pal.on_primary);
    f.text("Aggiungi file", add.0 + 46.0, add.1, 112.0, 40.0, 17.0, SEMI, pal.on_primary);
    f.p.fill_a(add.0 + add.2 - 38.0, add.1, 1.0, 40.0, pal.on_primary, 0.35);
    f.icon(ic::CHEVRON_DOWN, add.0 + add.2 - 31.0, add.1 + 8.0, 24.0, pal.on_primary);
    f.hit((add.0, add.1, add.2 - 38.0, add.3), Act::AddFiles);
    f.hit((add.0 + add.2 - 38.0, add.1, 38.0, add.3), Act::AddMenu);
    let mut x = 544.0;
    x += f.tool(x, 234.0, 40.0, "Aggiungi cartella", ic::FOLDER, 16.0, Act::AddFolder, true) + 8.0;
    f.vline(x, 240.0, 30.0);
    x += 20.0;
    f.tool(x, 234.0, 40.0, "Rimuovi", ic::TRASH, 16.0, Act::RemoveChecked, !visible.is_empty() && !app.busy());
    let count = format!("{} file", visible.len());
    f.text_r(&count, (q.0 + q.2 - 140.0, 234.0, 70.0, 40.0), 16.0, SANS, pal.text);
    f.icon(ic::MENU, q.0 + q.2 - 56.0, 242.0, 24.0, pal.text);

    if visible.is_empty() {
        super::drop_zone(f, app, (q.0 + 20.0, 300.0, q.2 - 40.0, q.3 - 108.0), true);
        super::others_link(f, app, q.0 + 24.0, q.1 + q.3 - 50.0, 30.0, 15.0);
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
                f.text(&file.name(), tx, y + 20.0, name_w - 12.0, 24.0, 17.0, SEMI, pal.text);
                let sub = if file.state == FileState::Waiting { file.path.display().to_string() } else { file.message.clone() };
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
        super::others_link(f, app, q.0 + 24.0, q.1 + q.3 - 40.0, 30.0, 15.0);
    }

    // L'ispettore.
    let ir = (insp_x, 212.0, insp_w, foot_y - 20.0 - 212.0);
    f.card(ir, 12.0);
    f.text("Impostazioni di conversione", insp_x + 23.0, 230.0, insp_w - 40.0, 32.0, 20.0, SANS, pal.text);
    let body = (insp_x + 1.0, 272.0, insp_w - 2.0, ir.1 + ir.3 - 272.0 - 8.0);
    super::scrolled(f, body, Scroll::Inspector, |f| super::options_panel(f, app, insp_x + 24.0, 284.0, insp_w - 45.0, OPT_STD, true) + 20.0);

    footer(f, app, foot_y);
}

fn footer(f: &mut Frame, app: &App, y: f32) {
    let pal = f.pal;
    let w = f.w;
    f.p.fill(NAV_W, y, w - NAV_W, f.h - y, pal.bg);
    f.hline(NAV_W, y, w - NAV_W);
    let go_w = 260.0;
    let go_x = w - 24.0 - go_w;
    // Destinazione: si accorcia con la finestra stretta.
    let dest_w = 270.0f32.min((go_x - 40.0 - 552.0 - 330.0).max(150.0));
    f.icon(ic::FOLDER, 306.0, y + 36.0, 24.0, pal.text);
    f.text("Cartella di destinazione", 356.0, y + 25.0, 190.0, 30.0, 16.0, SEMI, pal.text);
    super::dest_select(f, app, (552.0, y + 25.0, dest_w, 43.0), 16.0);
    f.text(&super::dest_note(app), 356.0, y + 70.0, go_x - 380.0, 26.0, 14.5, SANS, pal.muted);
    let ex = 552.0 + dest_w + 18.0;
    f.vline(ex, y + 22.0, 76.0);
    f.icon(ic::COPY, ex + 20.0, y + 30.0, 24.0, pal.text);
    f.text("Se esiste già un file", ex + 63.0, y + 25.0, 160.0, 30.0, 16.0, SEMI, pal.text);
    let sel_x = ex + 63.0 + 157.0;
    let sel_w = (go_x - 26.0 - sel_x).clamp(120.0, 166.0);
    super::exists_select(f, app, (sel_x, y + 25.0, sel_w, 43.0), 16.0);
    f.text(&app.exists_example(), ex + 63.0, y + 66.0, go_x - ex - 80.0, 26.0, 14.5, SANS, pal.muted);
    let label = super::go_label(app);
    let ready = app.busy() || !app.targets().is_empty();
    if app.busy() {
        if let Some(p) = app.progress() {
            f.p.rrect(go_x, y + 100.0, go_w, 6.0, 3.0, Some(pal.border), None);
            f.p.rrect(go_x, y + 100.0, go_w * p, 6.0, 3.0, Some(pal.primary), None);
        }
    }
    f.primary((go_x, y + 24.0, go_w, 74.0), &label, Some(ic::CONVERT), 22.0, Act::Start, ready && !app.busy());
    super::go_extras(f, app, (go_x, y + 24.0, go_w, 74.0 + 14.0), false);
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
    f.p.rrect(card.0, card.1, card.2, card.3, 10.0, Some(pal.surface), Some((pal.border, 1.0)));
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
                f.text(&file.name(), list.0 + 196.0, y + 36.0, list.2 - 520.0, 30.0, 22.0, SEMI, pal.text);
                let sub = if file.state == FileState::Waiting { file.path.display().to_string() } else { file.message.clone() };
                let sub_c = match file.state {
                    FileState::Failed => pal.bad,
                    FileState::Done => pal.ok,
                    _ => pal.muted,
                };
                f.text(&sub, list.0 + 196.0, y + 70.0, list.2 - 520.0, 28.0, 20.0, SANS, sub_c);
                f.row_bar(file, list.0 + 196.0, y + 106.0, list.2 - 520.0, 8.0);
                let (size, _) = super::file_meta(app, i);
                f.text(&size, list.0 + list.2 - 300.0, y, 200.0, 134.0, 20.0, SANS, pal.muted);
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
    let icon = match app.kind() {
        Kind::Image => ic::IMAGE,
        Kind::Audio => ic::AUDIO,
        Kind::Video => ic::VIDEO,
        _ => ic::FILE,
    };
    f.select((inner_x + 164.0, fy, inner_w - 164.0, 68.0), &title, Some(icon), 24.0, Act::FormatMenu);
    let label = super::go_label(app);
    let ready = !app.targets().is_empty();
    f.primary((inner_x, fy + 108.0, inner_w, 88.0), &label, Some(ic::CONVERT), 28.0, Act::Start, ready && !app.busy());
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
