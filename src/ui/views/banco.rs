//! Tema 4, «Banco di lavoro»: scuro, attività in alto, categorie a sinistra, tabella dei file con
//! operazione e stato, ispettore a destra e piede con la destinazione e l'avanzamento complessivo.
//! Misure dalla tavola 04 (1536×1024): la tabella si allarga, il resto resta fermo.

use super::*;
use crate::ui::core::Scroll;
use crate::ui::model::FileState;

const BAR: f32 = 80.0;
const NAV_W: f32 = 216.0;
const INSP_W: f32 = 462.0;
const FOOT: f32 = 168.0;

fn top_bar(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.fill(0.0, 0.0, f.w, BAR, pal.bg);
    f.caption((0.0, 0.0, f.w, BAR));
    f.hline(0.0, BAR - 1.0, f.w);
    f.brand(20.0, 17.0, 46.0, 30.0, 14.0, None);
    f.vline(188.0, 24.0, 34.0);
    let mut x = 204.0;
    for a in Activity::ALL {
        let on = app.activity == a;
        let size = if on { 25.0 } else { 18.0 };
        let tw = f.measure(a.name(), size, SANS);
        let isz = if on { 0.0 } else { 30.0 };
        let w = tw + isz + if on { 0.0 } else { 14.0 } + 34.0;
        let r = (x, 0.0, w, BAR - 1.0);
        let act = Act::Activity(a);
        if !on && f.hovered(&act) {
            f.p.fill_a(r.0, r.1, r.2, r.3, pal.alt, 0.6);
        }
        let c = if on { pal.text } else { mix(pal.text, pal.muted, 0.35) };
        let mut tx = x + 17.0;
        if !on {
            f.icon(a.icon(), tx, 25.0, 30.0, c);
            tx += isz + 14.0;
        }
        f.text(a.name(), tx, 0.0, tw + 6.0, BAR - 3.0, size, SANS, c);
        if on {
            f.p.fill(x, BAR - 4.0, w, 3.0, pal.primary);
        }
        f.focus_ring(r, 4.0, &act);
        f.hit(r, act);
        x += w + 12.0;
    }
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
}

fn categories(f: &mut Frame, app: &App, bottom: f32) {
    let pal = f.pal;
    f.p.fill(0.0, BAR, NAV_W, bottom - BAR, pal.bg);
    f.vline(NAV_W - 1.0, BAR, bottom - BAR);
    let icons = [ic::IMAGE, ic::FILE, ic::FILE, ic::MENU, ic::AUDIO, ic::VIDEO];
    for (i, cat) in conversioni::CATEGORIES.iter().enumerate() {
        let y = BAR + 20.0 + i as f32 * 62.0;
        let on = i == app.category && app.activity == Activity::Convert;
        let act = Act::Category(i);
        let r = (0.0, y, NAV_W - 6.0, 56.0);
        if on {
            f.p.rrect(r.0 - 8.0, r.1, r.2 + 8.0, r.3, 8.0, Some(mix(pal.primary, pal.bg, 0.72)), None);
            f.p.rrect(0.0, y + 12.0, 5.0, 32.0, 2.5, Some(pal.primary), None);
        } else if f.hovered(&act) {
            f.p.rrect(r.0 - 8.0, r.1, r.2 + 8.0, r.3, 8.0, Some(pal.alt), None);
        }
        f.icon(icons[i], 26.0, y + 14.0, 28.0, pal.text);
        f.text(cat.name, 80.0, y, 110.0, 56.0, 19.0, SANS, pal.text);
        let count = app.files.iter().filter(|x| !x.voice_only && x.kind == cat.kind).count();
        if count > 0 && !on {
            f.p.circle(NAV_W - 28.0, y + 28.0, 4.0, Some(pal.primary), None);
        }
        f.focus_ring(r, 8.0, &act);
        f.hit(r, act);
    }
}

fn state_cell(f: &mut Frame, file: &crate::ui::model::FileItem, x: f32, y: f32, h: f32, size: f32) {
    let c = f.state_color(file.state);
    f.state_icon(file.state, x + 17.0, y + h / 2.0, 15.0);
    let word = Frame::state_word(file.state);
    f.text(word, x + 46.0, y, 200.0, h, size, SANS, c);
}

fn table(f: &mut Frame, app: &App, r: R, simple: bool) {
    let pal = f.pal;
    f.card(r, 4.0);
    f.p.fill(r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, 47.0, mix(pal.alt, pal.surface, 0.55));
    let visible = app.visible();
    let all = !visible.is_empty() && visible.iter().all(|&i| app.files[i].checked);
    let stat_w = if simple { 360.0 } else { 232.0 };
    let op_w = if simple { 0.0 } else { 168.0 };
    let col_stat = r.0 + r.2 - stat_w;
    let col_op = col_stat - op_w;
    f.checkbox(r.0 + 14.0, r.1 + 13.0, 22.0, all, Some(Act::CheckAll));
    f.vline(r.0 + 52.0, r.1 + 1.0, 47.0);
    f.text("File", r.0 + 66.0, r.1, 100.0, 48.0, 17.0, SANS, pal.text);
    f.icon("M6 15l6-6 6 6", r.0 + 108.0, r.1 + 12.0, 24.0, pal.text);
    if !simple {
        f.vline(col_op, r.1 + 1.0, 47.0);
        f.text("Operazione", col_op + 16.0, r.1, 140.0, 48.0, 17.0, SANS, pal.text);
    }
    f.vline(col_stat, r.1 + 1.0, 47.0);
    f.text("Stato", col_stat + 16.0, r.1, 140.0, 48.0, 17.0, SANS, pal.text);
    f.hline(r.0, r.1 + 48.0, r.2);
    if visible.is_empty() {
        super::drop_zone(f, app, (r.0 + 24.0, r.1 + 72.0, r.2 - 48.0, (r.3 - 96.0).max(120.0)), true);
        return;
    }
    let rh = if simple { 106.0 } else { 90.0 };
    let body = (r.0 + 1.0, r.1 + 49.0, r.2 - 2.0, r.3 - 50.0);
    let op = if super::is_tool(app) { app.action_label().to_string() } else { super::action_title(app, false) };
    super::scrolled(f, body, Scroll::Queue, |f| {
        for (n, &i) in visible.iter().enumerate() {
            let y = body.1 + n as f32 * rh;
            if !f.p.shows_y(y, rh) {
                continue;
            }
            let file = &app.files[i];
            if file.state == FileState::Running {
                f.p.fill_a(r.0 + 1.0, y, r.2 - 2.0, rh, pal.primary, 0.12);
            } else if app.selected == Some(i) {
                f.p.fill_a(r.0 + 1.0, y, r.2 - 2.0, rh, pal.primary, 0.07);
            }
            f.hit((r.0, y, r.2, rh), Act::Select(i));
            f.hline(r.0, y + rh - 1.0, r.2);
            f.checkbox(r.0 + 14.0, y + rh / 2.0 - 11.0, 22.0, file.checked, Some(Act::Check(i)));
            let (tw, th) = if simple { (106.0, 80.0) } else { (70.0, 62.0) };
            f.thumb(app, i, (r.0 + 64.0, y + (rh - th) / 2.0, tw, th), 4.0);
            let tx = r.0 + 64.0 + tw + 20.0;
            let w = (col_op - tx - 12.0).max(80.0);
            if simple {
                f.text(&file.name(), tx, y, if op_w == 0.0 { col_stat - tx - 12.0 } else { w }, rh, 22.0, SANS, pal.text);
            } else {
                f.text(&file.name(), tx, y + 14.0, w, 30.0, 19.0, SANS, pal.text);
                let (size, extra) = super::file_meta(app, i);
                let meta = [size, extra].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ");
                f.text(&meta, tx, y + 46.0, w, 26.0, 16.0, SANS, pal.muted);
                f.vline(col_op, y, rh);
                f.text(&op, col_op + 16.0, y, op_w - 20.0, rh, 18.0, SANS, pal.text);
            }
            f.vline(col_stat, y, rh);
            state_cell(f, file, col_stat + 14.0, y, rh, if simple { 22.0 } else { 18.0 });
            if file.state == FileState::Failed || file.state == FileState::Done {
                let note = file.message.clone();
                if !simple && !note.is_empty() {
                    f.text(&note, col_stat + 60.0, y + rh - 30.0, stat_w - 70.0, 22.0, 13.0, SANS, pal.muted);
                }
            }
            if let Some(o) = &file.output {
                f.hit((col_stat, y, stat_w, rh), Act::OpenPath(o.clone()));
            }
        }
        visible.len() as f32 * rh
    });
}

/// L'ispettore: formato, qualità, opzioni e, sotto, gli strumenti della categoria.
fn inspector(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.bg);
    f.vline(r.0, r.1, r.3);
    let cat = &conversioni::CATEGORIES[app.category];
    let icons = [ic::IMAGE, ic::FILE, ic::FILE, ic::MENU, ic::AUDIO, ic::VIDEO];
    f.icon(icons[app.category], r.0 + 28.0, r.1 + 24.0, 34.0, pal.primary);
    f.text(&format!("Impostazioni {}", cat.name.to_lowercase()), r.0 + 78.0, r.1 + 16.0, r.2 - 100.0, 50.0, 25.0, SEMI, pal.text);
    let body = (r.0 + 1.0, r.1 + 80.0, r.2 - 2.0, r.3 - 90.0);
    let (x, w) = (r.0 + 28.0, r.2 - 56.0);
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut y = body.1 + 6.0;
        y += super::options_panel(f, app, x, y, w, OPT_STD, true) + 6.0;
        let tools = conversioni::tools(app.kind());
        if !tools.is_empty() {
            f.hline(x, y, w);
            y += 22.0;
            f.text("Strumenti", x, y, 110.0, 32.0, 19.0, SEMI, pal.text);
            f.text("(al posto della conversione)", x + 108.0, y + 3.0, w - 108.0, 28.0, 15.0, SANS, pal.muted);
            y += 44.0;
            let home_id = app.default_action(false);
            let home = conversioni::formats(app.kind()).iter().find(|s| s.id == home_id).map(|s| s.id).unwrap_or("");
            for t in tools {
                let on = app.action == t.id;
                let act = Act::Action(if on { home } else { t.id });
                f.checkbox(x, y + 2.0, 24.0, on, None);
                f.text(t.label, x + 40.0, y, w - 44.0, 28.0, 17.0, SANS, pal.text);
                f.hit((x - 6.0, y - 4.0, w + 6.0, 36.0), act);
                y += 42.0;
            }
        }
        y - body.1 + 20.0
    });
}

fn footer(f: &mut Frame, app: &App, y: f32, simple: bool) {
    let pal = f.pal;
    f.p.fill(0.0, y, f.w, f.h - y, pal.bg);
    f.hline(0.0, y, f.w);
    let go = (f.w - 24.0 - 252.0, y + 48.0, 252.0, 60.0);
    let (done, total) = app.job.as_ref().map(|j| (j.done, j.total)).unwrap_or((0, app.targets().len()));
    // Avanzamento complessivo: file finiti sul totale, non una stima.
    let prog_w = (go.0 - 30.0 - 24.0 - if simple { 460.0 } else { 0.0 }).clamp(200.0, 360.0);
    let px = if simple { 24.0 } else { go.0 - 30.0 - prog_w };
    if !simple {
        f.vline(px - 18.0, y + 22.0, 100.0);
    }
    let head = if app.busy() || done > 0 { format!("{done} di {total} completati") } else { format!("{total} file pronti") };
    if simple {
        f.text(&head, px, y + 28.0, prog_w, 34.0, 22.0, SANS, pal.text);
    } else {
        f.text("Progresso complessivo", px, y + 26.0, prog_w, 30.0, 17.0, SANS, pal.text);
        f.text(&head, px, y + 56.0, prog_w, 30.0, 20.0, SANS, pal.text);
    }
    let by = y + if simple { 82.0 } else { 100.0 };
    let bw = prog_w - 56.0;
    f.p.rrect(px, by, bw, 8.0, 4.0, Some(pal.alt), None);
    let p = app.progress().unwrap_or(0.0).clamp(0.0, 1.0);
    f.p.rrect(px, by, (bw * p).max(if p > 0.0 { 8.0 } else { 0.0 }), 8.0, 4.0, Some(pal.primary), None);
    if app.busy() {
        f.text(&format!("{}%", (p * 100.0).round() as u32), px + bw + 14.0, by - 12.0, 52.0, 30.0, 17.0, SANS, pal.text);
    }
    if simple {
        let mx = px + prog_w + 40.0;
        let line = super::summary_line(app);
        f.icon(ic::COPY, mx, y + 58.0, 28.0, pal.muted);
        f.text(&line, mx + 40.0, y + 48.0, go.0 - mx - 60.0, 60.0, 16.0, SANS, pal.text);
    } else {
        // Destinazione e «se esiste già».
        let right = px - 36.0;
        let ex_w = 245.0;
        let ex_x = right - ex_w;
        f.vline(ex_x - 16.0, y + 22.0, 100.0);
        f.text("In caso di file già esistente", ex_x, y + 26.0, ex_w, 30.0, 17.0, SANS, pal.text);
        super::exists_select(f, app, (ex_x, y + 60.0, ex_w, 48.0), 17.0);
        let dw = ex_x - 34.0 - 24.0;
        f.text("Cartella di destinazione", 24.0, y + 26.0, dw, 30.0, 17.0, SANS, pal.text);
        let browse = 112.0;
        super::dest_select(f, app, (24.0, y + 60.0, (dw - browse - 12.0).max(140.0), 48.0), 17.0);
        f.button((24.0 + (dw - browse - 12.0).max(140.0) + 12.0, y + 60.0, browse, 48.0), "Sfoglia…", None, 17.0, Act::DestPick);
    }
    if app.busy() {
        let r = go;
        f.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
        f.spinner(r.0 + 40.0, r.1 + r.3 / 2.0, 14.0, pal.primary);
        f.text("Conversione in corso", r.0 + 70.0, r.1, r.2 - 76.0, r.3, 18.0, SANS, pal.muted);
    } else {
        let label = super::go_label(app);
        f.primary(go, &label, Some(ic::CONVERT), 20.0, Act::Start, !app.targets().is_empty());
    }
    super::go_extras(f, app, go, false);
}

pub fn draw(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let simple = f.simple;
    f.p.clear(pal.work);
    top_bar(f, app);
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
    if simple {
        let y = BAR + 34.0;
        f.text("Converti in", 30.0, y + 8.0, 150.0, 56.0, 21.0, SANS, pal.text);
        let title = super::action_title(app, true);
        f.select((164.0, y + 2.0, 264.0, 56.0), &title, None, 19.0, Act::FormatMenu);
        f.button((446.0, y + 2.0, 230.0, 58.0), "Aggiungi file", Some(ic::ADD), 20.0, Act::AddFiles);
        super::notice(f, app, (700.0, y + 4.0, (f.w - 700.0 - 450.0).max(120.0), 54.0), 15.0);
        super::essenziale::mode_switch(f, (f.w - 440.0, y - 6.0, 420.0, 62.0));
        let top = y + 100.0;
        table(f, app, (30.0, top, f.w - 60.0, foot_y - top - 30.0), true);
        super::others_link(f, app, 34.0, foot_y - 28.0, 24.0, 15.0);
    } else {
        categories(f, app, foot_y);
        let ix = f.w - INSP_W;
        inspector(f, app, (ix, BAR, INSP_W, foot_y - BAR));
        let cx = NAV_W + 18.0;
        let cw = ix - cx - 14.0;
        let mut x = cx;
        f.button((x, BAR + 22.0, 165.0, 44.0), "Aggiungi file", Some(ic::ADD), 17.0, Act::AddFiles);
        x += 177.0;
        f.button((x, BAR + 22.0, 192.0, 44.0), "Aggiungi cartella", Some(ic::ADD), 17.0, Act::AddFolder);
        x += 204.0;
        f.button((x, BAR + 22.0, 134.0, 44.0), "Rimuovi", Some(ic::TRASH), 17.0, Act::RemoveChecked);
        let n = app.visible().len();
        f.text_r(&format!("{n} file nella coda"), (cx + cw - 220.0, BAR + 22.0, 214.0, 44.0), 17.0, SANS, pal.text);
        super::notice(f, app, (cx + 560.0, BAR + 22.0, (cw - 560.0 - 230.0).max(0.0), 44.0), 14.0);
        table(f, app, (cx, BAR + 86.0, cw, foot_y - BAR - 86.0 - 18.0), false);
    }
    footer(f, app, foot_y, simple);
}
