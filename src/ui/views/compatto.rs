//! Tema 3, «Cedro compatto»: scuro e stretto. È la finestrella che si apre dall'icona vicino
//! all'orologio (`draw`) e anche la finestra principale del tema (`draw_full`), con le stesse
//! parti: schede, file, «Converti in», destinazione e un pulsante grande. Misure dalla tavola 03.

use super::*;
use crate::ui::core::Scroll;
use crate::ui::model::FileState;

const MARGIN: f32 = 22.0;
const COL_MAX: f32 = 640.0;

pub fn size(simple: bool) -> (f32, f32) {
    (480.0, if simple { 780.0 } else { 900.0 })
}

pub fn draw(f: &mut Frame, app: &App) {
    frame(f, app, false);
}

pub fn draw_full(f: &mut Frame, app: &App) {
    frame(f, app, true);
}

fn header(f: &mut Frame, full: bool, x: f32, w: f32) {
    let pal = f.pal;
    f.p.clear(pal.bg);
    f.caption((0.0, 0.0, f.w, 62.0));
    f.brand(x, 15.0, 32.0, 24.0, 12.0, None);
    let mut right = x + w;
    if full {
        right = f.w;
        f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
        right -= 3.0 * 46.0 + 8.0;
    } else {
        let act = Act::Close;
        if f.hovered(&act) {
            f.p.circle(right - 14.0, 31.0, 18.0, Some(pal.alt), None);
        }
        f.icon(ic::CLOSE, right - 26.0, 19.0, 24.0, pal.text);
        f.hit((right - 38.0, 10.0, 40.0, 40.0), act);
        right -= 52.0;
    }
    let gear = Act::OpenSettings;
    if f.hovered(&gear) {
        f.p.circle(right - 14.0, 31.0, 18.0, Some(pal.alt), None);
    }
    f.icon(ic::SETTINGS, right - 26.0, 19.0, 24.0, pal.text);
    f.hit((right - 38.0, 10.0, 40.0, 40.0), gear);
}

/// Una scheda a due, tre o quattro segmenti (la voce scelta piena di colore).
fn segments(f: &mut Frame, r: R, items: &[(&str, &str, Act, bool)]) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let w = r.2 / items.len() as f32;
    for (i, (label, icon, act, on)) in items.iter().enumerate() {
        let b = (r.0 + i as f32 * w, r.1, w, r.3);
        if *on {
            f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(pal.primary), None);
        } else if f.hovered(act) {
            f.p.rrect(b.0 + 2.0, b.1 + 2.0, b.2 - 4.0, b.3 - 4.0, 6.0, Some(pal.alt), None);
        }
        let c = if *on { pal.on_primary } else { pal.text };
        let size = if w < 120.0 { 13.5 } else { 15.5 };
        let tw = f.measure(label, size, if *on { SEMI } else { SANS });
        let show_icon = w > 135.0;
        let isz = 22.0;
        let total = tw + if show_icon { isz + 8.0 } else { 0.0 };
        let mut x = b.0 + (b.2 - total) / 2.0;
        if show_icon {
            f.icon(icon, x, b.1 + (b.3 - isz) / 2.0, isz, c);
            x += isz + 8.0;
        }
        f.text(label, x, b.1, tw + 4.0, b.3, size, if *on { SEMI } else { SANS }, c);
        f.focus_ring(b, 8.0, act);
        f.hit(b, act.clone());
    }
}

fn frame(f: &mut Frame, app: &App, full: bool) {
    let pal = f.pal;
    let simple = f.simple;
    let cw = (f.w - 2.0 * MARGIN).min(COL_MAX);
    let x = (f.w - cw) / 2.0;
    header(f, full, if full { x } else { MARGIN }, if full { cw } else { f.w - 2.0 * MARGIN });
    let mut y = 66.0;
    if full {
        let tabs: Vec<(&str, &str, Act, bool)> = Activity::ALL.iter().map(|a| (a.name(), a.icon(), Act::Activity(*a), app.activity == *a)).collect();
        segments(f, (x, y, cw, 46.0), &tabs);
        y += 56.0;
        if simple && app.activity == Activity::Convert {
            super::essenziale::mode_switch(f, (x, y, cw, 46.0));
            y += 56.0;
        }
    } else {
        let tab = f.ui.compact_tab;
        if simple && tab == 0 {
            super::essenziale::mode_switch(f, (x, y, cw, 48.0));
        } else {
            segments(f, (x, y, cw, 46.0), &[("File", ic::FILE, Act::CompactTab(0), tab == 0), ("Link", ic::LINK, Act::CompactTab(1), tab == 1), ("Trascrivi", ic::TRANSCRIBE, Act::CompactTab(2), tab == 2)]);
        }
        y += 58.0;
        if tab == 1 {
            let yt = app.activity == Activity::YouTube;
            segments(f, (x, y, cw, 38.0), &[("YouTube", ic::VIDEO, Act::Activity(Activity::YouTube), yt), ("Altri siti", ic::WEB, Act::Activity(Activity::Web), !yt)]);
            y += 48.0;
        }
    }
    let links_h = 52.0;
    if app.activity != Activity::Convert {
        let r = (x, y + 4.0, cw, f.h - y - 4.0 - links_h - 8.0);
        match app.activity {
            Activity::Transcribe => transcribe_page(f, app, r, simple),
            Activity::YouTube => links_page(f, app, r, false, simple),
            _ => links_page(f, app, r, true, simple),
        }
        bottom_links(f, app, x, f.h - links_h, cw, full);
        return;
    }
    let go_h = 56.0;
    let foot = f.h - links_h - go_h - 22.0;
    let body = (x, y, cw, foot - y - 4.0);
    super::scrolled(f, body, Scroll::Inspector, |f| convert_body(f, app, x, y, cw, simple));
    // Il pulsante grande e la riga di sotto restano fermi.
    let go = (x, foot + 10.0, cw, go_h);
    if app.busy() {
        f.p.rrect(go.0, go.1, go.2, go.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
        f.spinner(go.0 + 40.0, go.1 + go_h / 2.0, 14.0, pal.primary);
        f.text(&super::go_label(app), go.0 + 70.0, go.1, go.2 - 80.0, go_h, 19.0, SANS, pal.muted);
        if let Some(p) = app.progress() {
            f.p.rrect(go.0, go.1 + go_h + 6.0, go.2, 4.0, 2.0, Some(pal.border), None);
            f.p.rrect(go.0, go.1 + go_h + 6.0, go.2 * p, 4.0, 2.0, Some(pal.primary), None);
        }
    } else {
        f.primary(go, &super::go_label(app), Some(ic::CONVERT), 21.0, Act::Start, !app.targets().is_empty());
    }
    bottom_links(f, app, x, f.h - links_h, cw, full);
}

/// La riga di sotto: «Apri pannello completo» e «Guida» (e, in Semplificata, le altre strade).
fn bottom_links(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, full: bool) {
    let pal = f.pal;
    f.hline(x, y, w);
    let simple = f.simple;
    let mut items: Vec<(&str, &str, Act)> = Vec::new();
    if !full && simple && app.activity == Activity::Convert {
        items.push(("Scarica da web", ic::LINK, Act::CompactTab(1)));
        items.push(("Trascrivi", ic::TRANSCRIBE, Act::CompactTab(2)));
    }
    if !full {
        items.push(("Apri pannello completo", ic::ZOOM_IN, Act::OpenFull));
    }
    items.push(("Guida", ic::HELP, Act::OpenGuide));
    let size = 14.5;
    let total: f32 = items.iter().map(|(l, _, _)| f.measure(l, size, SANS) + 36.0).sum::<f32>() + 14.0 * (items.len() as f32 - 1.0);
    let mut cx = if items.len() <= 2 { x } else { x + (w - total).max(0.0) / 2.0 };
    for (i, (label, icon, act)) in items.iter().enumerate() {
        let tw = f.measure(label, size, SANS);
        let hot = f.hovered(act);
        let c = if hot { pal.text } else { pal.primary };
        let at_end = i == items.len() - 1 && items.len() <= 2 && items.len() > 1;
        let wd = tw + 30.0;
        let bx = if at_end { x + w - wd } else { cx };
        f.icon(icon, bx, y + 16.0, 20.0, c);
        f.text(label, bx + 28.0, y + 8.0, tw + 4.0, 36.0, size, SANS, c);
        f.hit((bx - 4.0, y + 8.0, wd + 8.0, 36.0), act.clone());
        cx += wd + 14.0;
    }
}

fn convert_body(f: &mut Frame, app: &App, x: f32, y0: f32, w: f32, simple: bool) -> f32 {
    let pal = f.pal;
    let mut y = y0;
    if super::notice(f, app, (x, y, w, 46.0), 14.0) {
        y += 56.0;
    }
    let vis = app.visible();
    if vis.is_empty() {
        super::drop_zone(f, app, (x, y, w, 170.0), false);
        y += 182.0;
    } else {
        let cat = &conversioni::CATEGORIES[app.category];
        if simple {
            let head = format!("{} {} pront{}", vis.len(), cat.name.to_lowercase(), if vis.len() == 1 { "o" } else { "i" });
            f.text(&head, x, y, w, 30.0, 17.0, SEMI, pal.text);
            y += 36.0;
        }
        for &i in &vis {
            let file = &app.files[i];
            f.p.rrect(x, y, w, 62.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            f.thumb(app, i, (x + 8.0, y + 8.0, 46.0, 46.0), 5.0);
            f.text(&file.name(), x + 66.0, y + 8.0, w - 66.0 - 56.0, 26.0, 15.5, SEMI, pal.text);
            let (size, extra) = super::file_meta(app, i);
            let sub = if file.state == FileState::Waiting { [size, extra].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ") } else { file.message.clone() };
            f.text(&sub, x + 66.0, y + 34.0, w - 66.0 - 56.0, 22.0, 13.5, SANS, if file.state == FileState::Waiting { pal.muted } else { f.state_color(file.state) });
            let cx = x + w - 30.0;
            if file.state == FileState::Waiting {
                let rm = Act::Remove(i);
                if f.hovered(&rm) {
                    f.p.circle(cx, y + 31.0, 15.0, Some(pal.alt), None);
                }
                f.icon(ic::CLOSE, cx - 9.0, y + 22.0, 18.0, pal.text);
                f.hit((cx - 17.0, y + 14.0, 34.0, 34.0), rm);
            } else {
                f.state_icon(file.state, cx, y + 31.0, 11.0);
                if let Some(o) = &file.output {
                    f.hit((cx - 17.0, y + 14.0, 34.0, 34.0), Act::OpenPath(o.clone()));
                }
            }
            y += 70.0;
        }
        // «Aggiungi file» c'è sempre, anche con la coda piena.
        f.tool(x + 6.0, y, 36.0, "Aggiungi file", ic::ADD, 15.0, Act::AddFiles, true);
        super::others_link(f, app, x + 190.0, y + 6.0, 24.0, 13.5);
        y += 48.0;
    }
    y += 6.0;
    if simple {
        f.text("Converti in", x, y, w, 26.0, 16.0, SEMI, pal.text);
        let title = super::action_title(app, true);
        f.select((x, y + 30.0, w, 56.0), &title, None, 18.0, Act::FormatMenu);
        y += 100.0;
        f.text(&super::summary_line(app), x, y, w, 26.0, 14.0, SANS, pal.muted);
        y += 30.0;
        if app.customized() {
            let tw = f.measure("Torna alle consigliate", 14.5, SEMI);
            f.p.text("Torna alle consigliate", x, y, tw + 2.0, 24.0, 14.5, SEMI, pal.primary, Align::Left);
            f.hit((x, y, tw, 24.0), Act::ResetRecommended);
            y += 30.0;
        }
        return y - y0 + 10.0;
    }
    // Standard: da cosa, a cosa, qualità, altre opzioni, destinazione.
    f.text("Converti in", x, y, w, 26.0, 16.0, SEMI, pal.text);
    let half = (w - 56.0) / 2.0;
    let from = format!("{} {}", vis.len().max(1), conversioni::CATEGORIES[app.category].name.to_lowercase());
    f.p.rrect(x, y + 30.0, half, 42.0, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
    f.text(&from, x + 14.0, y + 30.0, half - 20.0, 42.0, 15.0, SANS, pal.muted);
    f.icon(ic::ARROW_RIGHT, x + half + 14.0, y + 39.0, 24.0, pal.muted);
    let title = super::action_title(app, false);
    f.select((x + half + 56.0, y + 30.0, half, 42.0), &title, None, 15.5, Act::FormatMenu);
    y += 72.0 + 18.0;
    let st = OptStyle { label: 15.5, note: 13.5, control_h: 40.0, gap: 20.0, visible: 0, switch_w: 48.0, switch_h: 26.0 };
    if super::uses_quality(app.kind(), &app.action) {
        y += super::quality(f, app, x, y, w, st, false) + 14.0;
    }
    let defs = app.option_defs();
    if !defs.is_empty() {
        let open = f.ui.is_open(Fold::MoreOptions);
        let r = (x, y, w, 46.0);
        f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
        if f.hovered(&Act::Fold(Fold::MoreOptions)) {
            f.p.rrect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, 6.0, Some(pal.alt), None);
        }
        f.icon(ic::SETTINGS, x + 14.0, y + 11.0, 24.0, pal.text);
        f.text(&format!("Altre opzioni ({})", defs.len()), x + 50.0, y, w - 100.0, 46.0, 16.0, SANS, pal.text);
        f.icon(if open { "M6 15l6-6 6 6" } else { ic::CHEVRON_DOWN }, x + w - 38.0, y + 11.0, 24.0, pal.text);
        f.hit(r, Act::Fold(Fold::MoreOptions));
        y += 58.0;
        if open {
            for def in &defs {
                y += super::option(f, app, def, x + 4.0, y, w - 8.0, st) + st.gap;
            }
        }
    }
    y += 6.0;
    let half = (w - 12.0) / 2.0;
    f.text("Cartella di destinazione", x, y, half, 24.0, 13.5, SEMI, pal.text);
    f.text("In caso di conflitto", x + half + 12.0, y, half, 24.0, 13.5, SEMI, pal.text);
    super::dest_select(f, app, (x, y + 28.0, half, 42.0), 14.5);
    super::exists_select(f, app, (x + half + 12.0, y + 28.0, half, 42.0), 14.5);
    y += 78.0;
    y - y0 + 10.0
}
