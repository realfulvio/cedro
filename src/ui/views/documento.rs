//! Tema 5, «Documento al centro»: il file scelto in anteprima grande, l'elenco dei file a
//! sinistra, le impostazioni a destra. Misure dalla tavola 05 (1536×1024); l'anteprima si allarga.

use super::*;
use crate::ui::core::Scroll;
use crate::ui::gfx::{Fit, Weight};
use crate::ui::model::FileState;

const BAR: f32 = 66.0;
const LIST_W: f32 = 272.0;
const INSP_W: f32 = 402.0;
const FOOT: f32 = 76.0;

fn top_bar(f: &mut Frame, app: &App, simple: bool) {
    let pal = f.pal;
    f.p.fill(0.0, 0.0, f.w, BAR, pal.surface);
    f.hline(0.0, BAR - 1.0, f.w);
    f.caption((0.0, 0.0, f.w, BAR));
    f.brand(24.0, 14.0, 38.0, 30.0, 12.0, None);
    let mut x = LIST_W - 4.0;
    for a in Activity::ALL {
        let on = app.activity == a;
        let tw = f.measure(a.name(), 17.0, if on { SEMI } else { SANS });
        let w = tw + 78.0;
        let act = Act::Activity(a);
        let r = (x, 4.0, w, BAR - 5.0);
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3 + 4.0, 8.0, Some(pal.alt), None);
            f.p.fill(r.0, BAR - 3.0, r.2, 3.0, pal.primary);
        } else if f.hovered(&act) {
            f.p.rrect(r.0, r.1, r.2, r.3 - 6.0, 8.0, Some(mix(pal.alt, pal.surface, 0.5)), None);
        }
        let c = if on { pal.primary } else { pal.text };
        f.icon(a.icon(), x + 18.0, 18.0, 28.0, c);
        f.text(a.name(), x + 56.0, 0.0, tw + 6.0, BAR - 3.0, 17.0, if on { SEMI } else { SANS }, c);
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
        x += w + 10.0;
    }
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
    let gx = f.w - 3.0 * 46.0 - 52.0;
    for (i, (icon, act)) in [(ic::HELP, Act::OpenGuide), (ic::SETTINGS, Act::OpenSettings)].into_iter().enumerate() {
        let cx = gx - i as f32 * 46.0;
        if f.hovered(&act) {
            f.p.circle(cx + 14.0, 24.0, 20.0, Some(pal.alt), None);
        }
        f.icon(icon, cx, 10.0, 28.0, pal.text);
        f.hit((cx - 8.0, 4.0, 44.0, 40.0), act);
    }
    if simple && app.activity == Activity::Convert {
        let sx = gx - 46.0 - 16.0 - 300.0;
        super::essenziale::mode_switch(f, (sx.max(x + 6.0), 8.0, 300.0, 46.0));
    }
}

fn file_list(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.vline(r.0 + r.2 - 1.0, r.1, r.3);
    f.primary((r.0 + 18.0, r.1 + 19.0, r.2 - 36.0, 46.0), "Aggiungi file", Some(ic::ADD), 18.0, Act::AddFiles, true);
    let vis = app.visible();
    let list = (r.0, r.1 + 84.0, r.2 - 1.0, r.3 - 84.0 - 30.0);
    let rh = 80.0;
    super::scrolled(f, list, Scroll::Queue, |f| {
        for (n, &i) in vis.iter().enumerate() {
            let y = list.1 + n as f32 * rh;
            let file = &app.files[i];
            let on = app.selected == Some(i);
            let act = Act::Select(i);
            if on {
                f.p.rrect(r.0 + 6.0, y + 3.0, r.2 - 14.0, rh - 6.0, 8.0, Some(pal.alt), None);
            } else if f.hovered(&act) {
                f.p.rrect(r.0 + 6.0, y + 3.0, r.2 - 14.0, rh - 6.0, 8.0, Some(mix(pal.alt, pal.surface, 0.5)), None);
            }
            f.hit((r.0, y, r.2 - 1.0, rh), act);
            f.kind_icon(file.kind, r.0 + 20.0, y + 18.0, 44.0);
            f.text(&file.name(), r.0 + 78.0, y + 12.0, r.2 - 78.0 - 52.0, 28.0, 17.0, SEMI, pal.text);
            let (size, extra) = super::file_meta(app, i);
            let meta = match file.state {
                FileState::Waiting => [extra, size].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  "),
                _ => file.message.clone(),
            };
            f.text(&meta, r.0 + 78.0, y + 40.0, r.2 - 78.0 - 52.0, 26.0, 14.5, SANS, if file.state == FileState::Waiting { pal.muted } else { f.state_color(file.state) });
            let cx = r.0 + r.2 - 34.0;
            if file.state == FileState::Waiting {
                let rm = Act::Remove(i);
                if f.hovered(&rm) {
                    f.p.circle(cx, y + rh / 2.0, 15.0, Some(pal.border), None);
                }
                f.icon(ic::CLOSE, cx - 9.0, y + rh / 2.0 - 9.0, 18.0, pal.muted);
                f.hit((cx - 16.0, y + rh / 2.0 - 16.0, 32.0, 32.0), rm);
            } else {
                f.state_icon(file.state, cx, y + rh / 2.0, 11.0);
            }
        }
        vis.len() as f32 * rh
    });
    super::others_link(f, app, r.0 + 18.0, r.1 + r.3 - 28.0, 22.0, 13.0);
}

fn nav_button(f: &mut Frame, x: f32, y: f32, left: bool, act: Act, enabled: bool) {
    let pal = f.pal;
    if enabled && f.hovered(&act) {
        f.p.circle(x + 15.0, y + 15.0, 17.0, Some(pal.alt), None);
    }
    let c = if enabled { pal.text } else { mix(pal.muted, pal.surface, 0.4) };
    f.icon(if left { ic::ARROW_LEFT } else { ic::ARROW_RIGHT }, x + 3.0, y + 3.0, 24.0, c);
    if enabled {
        f.hit((x - 4.0, y - 4.0, 38.0, 38.0), act);
    }
}

fn preview(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let vis = app.visible();
    let Some(i) = app.selected.filter(|i| vis.contains(i)) else {
        f.p.fill(r.0, r.1, r.2, r.3, mix(pal.border, pal.bg, 0.45));
        super::drop_zone(f, app, (r.0 + 40.0, r.1 + 40.0, r.2 - 80.0, r.3 - 80.0), true);
        return;
    };
    let file = &app.files[i];
    // Linguetta con il nome e barra delle pagine.
    f.p.fill(r.0, r.1, r.2, 100.0, pal.bg);
    let tab = (r.0 + 14.0, r.1 + 12.0, (f.measure(&file.name(), 17.0, SEMI) + 80.0).min(r.2 - 28.0), 46.0);
    f.p.rrect(tab.0, tab.1, tab.2, tab.3 + 8.0, 8.0, Some(pal.surface), None);
    f.kind_icon(file.kind, tab.0 + 14.0, tab.1 + 8.0, 30.0);
    f.text(&file.name(), tab.0 + 54.0, tab.1, tab.2 - 60.0, tab.3, 17.0, SEMI, pal.text);
    let pages = app.page_count(i).unwrap_or(1).max(1);
    let page = app.preview_page.min(pages - 1);
    let by = r.1 + 62.0;
    nav_button(f, r.0 + 20.0, by, true, Act::PagePrev, page > 0);
    nav_button(f, r.0 + 62.0, by, false, Act::PageNext, page + 1 < pages);
    f.numbox((r.0 + 112.0, by - 2.0, 56.0, 34.0), &(page + 1).to_string(), 16.0);
    f.text(&format!("/ {pages}"), r.0 + 180.0, by - 2.0, 80.0, 34.0, 16.0, SANS, pal.muted);
    let area = (r.0, r.1 + 100.0, r.2, r.3 - 100.0);
    f.p.fill(area.0, area.1, area.2, area.3, mix(pal.border, pal.bg, 0.45));
    let img = app.previews.get(&(file.path.clone(), page)).and_then(|o| o.clone()).or_else(|| app.media_of(&file.path).and_then(|m| m.thumb.clone()));
    let Some(img) = img else {
        let msg = match file.kind {
            crate::engine::Kind::Pdf | crate::engine::Kind::Image => "Preparo l'anteprima…",
            _ => "Anteprima non disponibile per questo tipo di file.",
        };
        f.kind_icon(file.kind, area.0 + area.2 / 2.0 - 40.0, area.1 + area.3 / 2.0 - 80.0, 80.0);
        f.text_c(msg, (area.0, area.1 + area.3 / 2.0 + 20.0, area.2, 34.0), 17.0, SANS, pal.muted);
        return;
    };
    let (iw, ih) = (img.w as f32, img.h as f32);
    let pw = (area.2 - 80.0).min(860.0).max(200.0);
    let (pw, ph) = if file.kind == crate::engine::Kind::Image {
        // Le foto stanno per intero nel riquadro.
        let k = (pw / iw).min((area.3 - 60.0) / ih);
        (iw * k, ih * k)
    } else {
        (pw, pw * ih / iw)
    };
    let px = area.0 + (area.2 - pw) / 2.0;
    super::scrolled(f, area, Scroll::Page, |f| {
        let py = area.1 + 30.0;
        f.p.shadow(px, py, pw, ph, 2.0, 14.0, 0.25, 4.0);
        f.p.rrect(px, py, pw, ph, 2.0, Some((255, 255, 255)), None);
        f.p.image(&img, px, py, pw, ph, 2.0, Fit::Contain);
        ph + 60.0
    });
}

fn inspector(f: &mut Frame, app: &App, r: R, simple: bool) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.vline(r.0, r.1, r.3);
    let (x, w) = (r.0 + 28.0, r.2 - 56.0);
    let vis = app.visible();
    let sel = app.selected.filter(|i| vis.contains(i));
    let id = app.action.as_str();
    if simple {
        f.text("Converti il documento", x, r.1 + 34.0, w, 54.0, 27.0, Weight::SerifBold, pal.text);
        let card = (x - 6.0, r.1 + 112.0, w + 12.0, 82.0);
        f.p.rrect(card.0, card.1, card.2, card.3, 8.0, Some(pal.alt), None);
        if let Some(i) = sel {
            let file = &app.files[i];
            f.kind_icon(file.kind, card.0 + 16.0, card.1 + 14.0, 54.0);
            let name = if vis.len() > 1 { format!("{} e altri {}", file.name(), vis.len() - 1) } else { file.name() };
            f.text(&name, card.0 + 86.0, card.1 + 10.0, card.2 - 96.0, 34.0, 21.0, SEMI, pal.text);
            let (size, extra) = super::file_meta(app, i);
            f.text(&[extra, size].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  "), card.0 + 86.0, card.1 + 44.0, card.2 - 96.0, 28.0, 16.0, SANS, pal.muted);
        }
        f.text("Formato di output", x, r.1 + 226.0, w, 30.0, 18.0, SEMI, pal.text);
        let title = super::action_title(app, true);
        f.select((x, r.1 + 262.0, w, 62.0), &title, None, 19.0, Act::FormatMenu);
        let line = format!("{} · {}", if app.customized() { "Impostazioni personalizzate" } else { "Impostazioni consigliate" }, "tutte le pagine");
        f.text(&line, x, r.1 + 336.0, w, 30.0, 16.0, SANS, pal.muted);
        let ready = !app.targets().is_empty();
        let go = (x, r.1 + 390.0, w, 66.0);
        if app.busy() {
            f.p.rrect(go.0, go.1, go.2, go.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            f.spinner(go.0 + 40.0, go.1 + go.3 / 2.0, 14.0, pal.primary);
            f.text("Conversione in corso", go.0 + 72.0, go.1, go.2 - 80.0, go.3, 19.0, SANS, pal.muted);
            if let Some(p) = app.progress() {
                f.p.rrect(go.0, go.1 + go.3 + 10.0, go.2, 5.0, 2.5, Some(pal.border), None);
                f.p.rrect(go.0, go.1 + go.3 + 10.0, go.2 * p, 5.0, 2.5, Some(pal.primary), None);
            }
        } else {
            f.primary(go, &super::go_label(app), Some(ic::CONVERT), 22.0, Act::Start, ready);
        }
        let mut y = go.1 + go.3 + 40.0;
        if !super::notice(f, app, (x - 6.0, y, w + 12.0, 52.0), 14.0) {
            if app.customized() {
                let tw = f.measure("Torna alle consigliate", 16.0, SEMI);
                f.p.text("Torna alle consigliate", x, y, tw + 2.0, 26.0, 16.0, SEMI, pal.primary, Align::Left);
                f.hit((x, y, tw, 26.0), Act::ResetRecommended);
                y += 30.0;
            }
        }
        let _ = y;
        return;
    }
    // Standard: da che formato a quale, poi le opzioni del motore.
    f.p.rrect(x - 6.0, r.1 + 22.0, w + 12.0, 86.0, 8.0, None, None);
    if let Some(i) = sel {
        f.kind_icon(app.files[i].kind, x, r.1 + 30.0, 56.0);
        f.text(app.files[i].path.extension().and_then(|e| e.to_str()).unwrap_or("").to_uppercase().as_str(), x + 68.0, r.1 + 30.0, 80.0, 56.0, 21.0, SEMI, pal.text);
    }
    f.icon(ic::ARROW_RIGHT, x + w / 2.0 - 16.0, r.1 + 44.0, 32.0, pal.text);
    let target = id.to_uppercase();
    let tx = x + w - 168.0;
    format_icon_small(f, id, tx, r.1 + 30.0, 56.0);
    f.text(&target, tx + 66.0, r.1 + 30.0, 100.0, 56.0, 21.0, SEMI, pal.text);
    let body = (r.0 + 1.0, r.1 + 108.0, r.2 - 2.0, r.3 - 108.0 - 130.0);
    super::scrolled(f, body, Scroll::Inspector, |f| super::options_panel(f, app, x, body.1 + 8.0, w, OPT_STD, true) + 16.0);
    let by = r.1 + r.3 - 120.0;
    f.p.rrect(x - 6.0, by, w + 12.0, 96.0, 8.0, Some(mix(pal.primary, pal.surface, 0.9)), None);
    f.icon(ic::HELP, x + 10.0, by + 16.0, 30.0, pal.primary);
    if !super::notice(f, app, (x - 6.0, by, w + 12.0, 96.0), 14.0) {
        f.text("Anteprima dell'originale", x + 56.0, by + 10.0, w - 60.0, 30.0, 17.0, SEMI, pal.text);
        f.para("Stai visualizzando il documento originale. Nessuna conversione è ancora stata eseguita.", x + 56.0, by + 40.0, w - 62.0, 14.5, SANS, pal.muted, 1.35, 3);
    }
}

fn format_icon_small(f: &mut Frame, id: &str, x: f32, y: f32, s: f32) {
    match id {
        "docx" | "doc" => f.word_icon(x, y, s),
        "txt" | "html" | "rtf" => f.text_icon(x, y, s),
        "pdf" => f.pdf_icon(x + s * 0.12, y + s * 0.05, s * 0.9),
        _ => f.image_icon(x, y, s),
    }
}

fn footer(f: &mut Frame, app: &App, y: f32, simple: bool) {
    let pal = f.pal;
    f.p.fill(0.0, y, f.w, f.h - y, pal.surface);
    f.hline(0.0, y, f.w);
    f.icon(ic::FOLDER, 26.0, y + 24.0, 30.0, pal.text);
    f.text("Cartella di destinazione", 68.0, y, 190.0, FOOT, 15.0, SANS, pal.text);
    let dw = 360.0;
    super::dest_select(f, app, (266.0, y + 16.0, dw, 44.0), 16.0);
    if f.hovered(&Act::DestPick) {
        f.p.circle(660.0 + 14.0, y + 38.0, 20.0, Some(pal.alt), None);
    }
    f.icon(ic::FOLDER, 660.0, y + 24.0, 28.0, pal.text);
    f.hit((650.0, y + 14.0, 48.0, 48.0), Act::DestPick);
    if !simple {
        let ex = 730.0;
        f.vline(ex - 14.0, y + 14.0, 48.0);
        f.text("Se esiste già", ex, y, 120.0, FOOT, 16.0, SANS, pal.text);
        super::exists_select(f, app, (ex + 124.0, y + 16.0, 170.0, 44.0), 16.0);
    }
    if !simple {
        let go = (f.w - 20.0 - 250.0, y + 8.0, 250.0, 60.0);
        if app.busy() {
            f.p.rrect(go.0, go.1, go.2, go.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            f.spinner(go.0 + 36.0, go.1 + go.3 / 2.0, 13.0, pal.primary);
            f.text("In corso…", go.0 + 64.0, go.1, go.2 - 70.0, go.3, 18.0, SANS, pal.muted);
            if let Some(p) = app.progress() {
                f.p.rrect(go.0, go.1 + go.3 + 4.0, go.2 * p, 3.0, 1.5, Some(pal.primary), None);
            }
        } else {
            f.primary(go, &super::go_label(app), Some(ic::CONVERT), 19.0, Act::Start, !app.targets().is_empty());
        }
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let simple = f.simple;
    f.p.clear(pal.bg);
    top_bar(f, app, simple);
    if app.activity != Activity::Convert {
        let r = (28.0, BAR + 20.0, f.w - 56.0, f.h - BAR - 36.0);
        match app.activity {
            Activity::Transcribe => transcribe_page(f, app, r, simple),
            Activity::YouTube => links_page(f, app, r, false, simple),
            _ => links_page(f, app, r, true, simple),
        }
        return;
    }
    let foot_y = f.h - FOOT;
    let ix = f.w - INSP_W;
    if simple {
        preview(f, app, (0.0, BAR, ix, foot_y - BAR));
    } else {
        file_list(f, app, (0.0, BAR, LIST_W, foot_y - BAR));
        preview(f, app, (LIST_W, BAR, ix - LIST_W, foot_y - BAR));
    }
    inspector(f, app, (ix, BAR, INSP_W, foot_y - BAR), simple);
    if simple {
        // «Aggiungi file» resta sempre a portata di mano, anche con tanti file.
        f.button((26.0, foot_y - 70.0, 190.0, 46.0), "Aggiungi file", Some(ic::ADD), 17.0, Act::AddFiles);
        super::others_link(f, app, 232.0, foot_y - 58.0, 24.0, 14.0);
    }
    footer(f, app, foot_y, simple);
}
