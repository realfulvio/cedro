//! Tema 2, «Percorso guidato»: tre passi (File, Operazione, Salvataggio), un solo compito per
//! schermata, niente barra laterale. Misure dalla tavola 02 (1536×1024): il contenuto sta in una
//! colonna centrata che si allarga fino a 1237, intestazione e piede restano fermi.

use super::*;
use crate::ui::core::Scroll;
use crate::ui::model::FileState;

const FOOT: f32 = 100.0;

fn chrome(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.clear(pal.bg);
    f.caption((0.0, 0.0, f.w, 62.0));
    f.brand(30.0, 16.0, 30.0, 22.0, 12.0, None);
    // L'attività si cambia da qui: il percorso vale per tutte e quattro.
    let label = app.activity.name();
    let sw = f.measure(label, 16.0, SANS) + 92.0;
    f.select((176.0, 12.0, sw, 40.0), label, Some(app.activity.icon()), 16.0, Act::ActivityMenu);
    f.window_controls(f.w, 0.0, 48.0, pal.text, pal.alt);
    let gx = f.w - 3.0 * 46.0 - 56.0;
    let hot = f.hovered(&Act::OpenSettings);
    if hot {
        f.p.circle(gx + 14.0, 24.0, 20.0, Some(pal.alt), None);
    }
    f.icon(ic::SETTINGS, gx, 10.0, 28.0, pal.text);
    f.hit((gx - 8.0, 4.0, 44.0, 40.0), Act::OpenSettings);
    let hx = gx - 48.0;
    if f.hovered(&Act::OpenGuide) {
        f.p.circle(hx + 14.0, 24.0, 20.0, Some(pal.alt), None);
    }
    f.icon(ic::HELP, hx, 10.0, 28.0, pal.text);
    f.hit((hx - 8.0, 4.0, 44.0, 40.0), Act::OpenGuide);
}

fn stepper(f: &mut Frame, y: f32, step: u8, reach: u8) {
    let pal = f.pal;
    let cx = f.w / 2.0;
    let names = ["File", "Operazione", "Salvataggio"];
    let xs = [cx - 321.0, cx - 58.0, cx + 205.0];
    let lw: Vec<f32> = names.iter().enumerate().map(|(i, n)| f.measure(n, 18.0, if i as u8 == step { SEMI } else { SANS })).collect();
    for i in 0..3usize {
        let on = i as u8 == step;
        let done = (i as u8) < step;
        let (ccx, ccy) = (xs[i], y);
        if on {
            f.p.circle(ccx, ccy, 20.0, Some(pal.primary), None);
        } else if done {
            f.p.circle(ccx, ccy, 19.0, None, Some((pal.ok, 2.0)));
        } else {
            f.p.circle(ccx, ccy, 19.0, None, Some((pal.muted, 2.0)));
        }
        let c = if on { pal.on_primary } else if done { pal.ok } else { pal.muted };
        f.text_c(&(i + 1).to_string(), (ccx - 20.0, ccy - 20.0, 40.0, 40.0), 18.0, SEMI, c);
        let tc = if on { pal.primary } else if done { pal.text } else { pal.muted };
        f.text(names[i], ccx + 34.0, ccy - 20.0, lw[i] + 6.0, 40.0, 18.0, if on { SEMI } else { SANS }, tc);
        if i < 2 {
            let x0 = ccx + 34.0 + lw[i] + 18.0;
            let x1 = xs[i + 1] - 34.0;
            f.p.fill(x0, ccy - 1.0, (x1 - x0).max(0.0), 2.0, if done { pal.primary } else { pal.border });
        }
        if i as u8 <= reach && !on {
            f.hit((ccx - 24.0, ccy - 24.0, lw[i] + 70.0, 48.0), Act::Step(i as u8));
        }
    }
}

fn title(f: &mut Frame, x: f32, y: f32, t: &str, sub: &str, simple: bool) {
    let pal = f.pal;
    f.text(t, x, y, f.w - 2.0 * x, 70.0, if simple { 58.0 } else { 54.0 }, BOLD, pal.text);
    f.text(sub, x, y + 70.0, f.w - 2.0 * x, 40.0, 25.0, SANS, pal.muted);
}

/// La scheda con il file (o i file) scelti e «Cambia file».
fn summary_card(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.card(r, 10.0);
    let vis = app.visible();
    let sel = app.selected.filter(|i| vis.contains(i)).or(vis.first().copied());
    let Some(i) = sel else { return };
    let file = &app.files[i];
    f.kind_icon(file.kind, r.0 + 36.0, r.1 + 14.0, r.3 - 28.0);
    let (size, extra) = super::file_meta(app, i);
    let name = if vis.len() > 1 { format!("{} e altri {}", file.name(), vis.len() - 1) } else { file.name() };
    f.text(&name, r.0 + 106.0, r.1 + 10.0, r.2 - 400.0, 36.0, 24.0, SEMI, pal.text);
    let meta = [size, extra].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ");
    f.text(&meta, r.0 + 106.0, r.1 + 44.0, r.2 - 400.0, 28.0, 17.0, SANS, pal.muted);
    let b = (r.0 + r.2 - 186.0, r.1 + (r.3 - 46.0) / 2.0, 168.0, 46.0);
    f.button(b, "Cambia file", Some(ic::FOLDER), 17.0, Act::Step(0));
}

fn format_icon(f: &mut Frame, id: &str, x: f32, y: f32, s: f32) {
    let pal = f.pal;
    match id {
        "docx" | "doc" => f.word_icon(x, y, s),
        "txt" | "html" | "rtf" => f.text_icon(x, y, s),
        "pdf" => f.pdf_icon(x + s * 0.12, y + s * 0.05, s * 0.9),
        "mp3" | "wav" | "m4a" | "ogg" | "flac" => f.icon(ic::AUDIO, x + s * 0.1, y + s * 0.1, s * 0.8, pal.primary),
        "mp4" | "mkv" => f.icon(ic::VIDEO, x + s * 0.1, y + s * 0.1, s * 0.8, pal.primary),
        _ => f.image_icon(x, y, s),
    }
}

fn choice_cards(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let list = conversioni::simple_formats(app.kind());
    let n = list.len().max(1) as f32;
    let gap = 14.0;
    let w = (r.2 - gap * (n - 1.0)) / n;
    for (i, sf) in list.iter().enumerate() {
        let x = r.0 + i as f32 * (w + gap);
        let on = app.action == sf.id;
        let act = Act::Action(sf.id);
        let bg = if on { mix(pal.primary, pal.surface, 0.9) } else if f.hovered(&act) { pal.alt } else { pal.surface };
        f.p.rrect(x, r.1, w, r.3, 10.0, Some(bg), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
        let isz = (r.3 * 0.38).clamp(48.0, 84.0);
        format_icon(f, sf.id, x + 26.0, r.1 + (r.3 - isz) / 2.0, isz);
        let tx = x + 26.0 + isz + 26.0;
        let tw = (x + w - 30.0 - tx).max(60.0);
        f.text(sf.title, tx, r.1 + r.3 * 0.18, tw, 34.0, 25.0, SEMI, pal.text);
        f.para(sf.note, tx, r.1 + r.3 * 0.18 + 42.0, tw, 19.0, SANS, pal.muted, 1.35, 3);
        f.radio(x + w - 32.0, r.1 + 32.0, 13.0, on, pal.muted);
        f.focus_ring((x, r.1, w, r.3), 10.0, &act);
        f.hit((x, r.1, w, r.3), act);
    }
}

fn step_files(f: &mut Frame, app: &App, x: f32, w: f32, top: f32, bottom: f32, simple: bool) {
    let pal = f.pal;
    title(f, x, top, "Quali file vuoi convertire?", "Trascina i file qui oppure scegli dal computer.", simple);
    super::notice(f, app, (x + w - 560.0, top + 8.0, 560.0, 44.0), 15.0);
    let y = top + 130.0;
    let visible = app.visible();
    f.button((x + w - 220.0, top + 62.0, 220.0, 46.0), "Aggiungi file", Some(ic::ADD), 17.0, Act::AddFiles);
    let h = (bottom - y - 36.0).max(160.0);
    if visible.is_empty() {
        super::drop_zone(f, app, (x, y, w, h), true);
    } else {
        f.card((x, y, w, h), 10.0);
        let rows = visible.clone();
        super::scrolled(f, (x + 1.0, y + 1.0, w - 2.0, h - 2.0), Scroll::Queue, |f| {
            for (n, &i) in rows.iter().enumerate() {
                let ry = y + n as f32 * 100.0;
                if !f.p.shows_y(ry, 100.0) {
                    continue;
                }
                let file = &app.files[i];
                if n > 0 {
                    f.hline(x, ry, w);
                }
                f.thumb(app, i, (x + 24.0, ry + 14.0, 110.0, 72.0), 6.0);
                f.text(&file.name(), x + 158.0, ry + 20.0, w - 560.0, 30.0, 21.0, SEMI, pal.text);
                f.text(&file.path.display().to_string(), x + 158.0, ry + 52.0, w - 560.0, 26.0, 16.0, SANS, pal.muted);
                let (size, extra) = super::file_meta(app, i);
                f.text(&size, x + w - 360.0, ry, 130.0, 100.0, 17.0, SANS, pal.muted);
                f.text(&extra, x + w - 230.0, ry, 130.0, 100.0, 17.0, SANS, pal.muted);
                let act = Act::Remove(i);
                if f.hovered(&act) {
                    f.p.circle(x + w - 48.0, ry + 50.0, 18.0, Some(pal.alt), None);
                }
                f.icon(ic::CLOSE, x + w - 60.0, ry + 38.0, 24.0, pal.text);
                f.hit((x + w - 70.0, ry + 28.0, 44.0, 44.0), act);
            }
            rows.len() as f32 * 100.0
        });
    }
    super::others_link(f, app, x + 4.0, y + h + 6.0, 28.0, 16.0);
}

fn step_action(f: &mut Frame, app: &App, x: f32, w: f32, top: f32, bottom: f32, simple: bool) {
    let pal = f.pal;
    let sub = if simple { "Scegli il formato di destinazione." } else { "Scegli il formato di destinazione e configura le opzioni." };
    title(f, x, top, "Cosa vuoi ottenere?", sub, simple);
    super::notice(f, app, (x + w - 560.0, top + 8.0, 560.0, 44.0), 15.0);
    let mut y = top + 130.0;
    summary_card(f, app, (x, y, w, 82.0));
    y += 94.0;
    let cards_h = if simple { ((bottom - y - 120.0) * 0.5).clamp(170.0, 260.0) } else { 190.0 };
    choice_cards(f, app, (x, y, w, cards_h));
    y += cards_h + 14.0;
    if simple {
        let line = format!("{} · {}", if app.customized() { "Impostazioni personalizzate" } else { "Impostazioni consigliate" }, app.action_label());
        f.icon(ic::SETTINGS, f.w / 2.0 - f.measure(&line, 19.0, SANS) / 2.0 - 36.0, y + 6.0, 28.0, pal.ok);
        f.text_c(&line, (x, y, w, 40.0), 19.0, SANS, pal.muted);
        if app.customized() {
            let tw = f.measure("Torna alle consigliate", 16.0, SEMI);
            let r = (x + (w - tw) / 2.0, y + 44.0, tw, 24.0);
            f.p.text("Torna alle consigliate", r.0, r.1, r.2 + 2.0, r.3, 16.0, SEMI, pal.primary, Align::Left);
            f.hit(r, Act::ResetRecommended);
        }
        return;
    }
    let card = (x, y, w, (bottom - y).max(120.0));
    f.card(card, 10.0);
    let head = if super::is_tool(app) { format!("Opzioni · {}", app.action_label()) } else { format!("Opzioni di conversione · {}", app.action_label()) };
    f.text(&head, x + 28.0, y + 14.0, w - 420.0, 36.0, 21.0, SEMI, pal.text);
    let sw = 340.0f32.min(w - 100.0);
    f.select((x + w - sw - 24.0, y + 12.0, sw, 40.0), "Altri formati e strumenti", None, 16.0, Act::FormatMenu);
    let body = (x + 1.0, y + 62.0, w - 2.0, card.3 - 66.0);
    let cw = (w - 56.0).min(720.0);
    super::scrolled(f, body, Scroll::Inspector, |f| super::options_panel(f, app, x + 28.0, y + 70.0, cw, OPT_STD, false) + 20.0);
}

fn step_save(f: &mut Frame, app: &App, x: f32, w: f32, top: f32, bottom: f32, simple: bool) {
    let pal = f.pal;
    title(f, x, top, "Dove salvo i file?", "Scegli la cartella: l'originale non viene mai toccato.", simple);
    super::notice(f, app, (x + w - 560.0, top + 8.0, 560.0, 44.0), 15.0);
    let mut y = top + 130.0;
    let card = (x, y, w, if simple { 150.0 } else { 196.0 });
    f.card(card, 10.0);
    f.icon(ic::FOLDER, x + 30.0, y + 30.0, 30.0, pal.primary);
    f.text("Cartella di destinazione", x + 82.0, y + 24.0, 340.0, 40.0, 20.0, SEMI, pal.text);
    let sw = (w - 560.0).clamp(260.0, 520.0);
    super::dest_select(f, app, (x + 440.0, y + 20.0, sw, 48.0), 18.0);
    f.text(&super::dest_note(app), x + 82.0, y + 74.0, w - 110.0, 30.0, 16.0, SANS, pal.muted);
    if !simple {
        f.hline(x + 24.0, y + 116.0, w - 48.0);
        f.icon(ic::COPY, x + 30.0, y + 138.0, 30.0, pal.primary);
        f.text("Se esiste già un file", x + 82.0, y + 132.0, 340.0, 40.0, 20.0, SEMI, pal.text);
        super::exists_select(f, app, (x + 440.0, y + 130.0, 240.0, 48.0), 18.0);
        f.text(&app.exists_example(), x + 700.0, y + 130.0, w - 720.0, 48.0, 16.0, SANS, pal.muted);
    }
    y += card.3 + 18.0;
    // I file con il loro stato (durante e dopo il lavoro).
    let targets = app.targets();
    let rows: Vec<usize> = if app.busy() || app.files.iter().any(|f| f.state != FileState::Waiting) { app.visible() } else { targets };
    let h = (bottom - y).max(100.0);
    f.card((x, y, w, h), 10.0);
    super::scrolled(f, (x + 1.0, y + 1.0, w - 2.0, h - 2.0), Scroll::Queue, |f| {
        for (n, &i) in rows.iter().enumerate() {
            let ry = y + n as f32 * 66.0;
            if !f.p.shows_y(ry, 66.0) {
                continue;
            }
            let file = &app.files[i];
            if n > 0 {
                f.hline(x, ry, w);
            }
            f.kind_icon(file.kind, x + 24.0, ry + 11.0, 44.0);
            f.text(&file.name(), x + 88.0, ry + 8.0, w - 420.0, 30.0, 18.0, SEMI, pal.text);
            let msg = if file.state == FileState::Waiting { "Pronto".to_string() } else { file.message.clone() };
            f.text(&msg, x + 88.0, ry + 36.0, w - 420.0, 24.0, 15.0, SANS, f.state_color(file.state));
            f.text_r(Frame::state_word(file.state), (x + w - 230.0, ry, 150.0, 66.0), 16.0, SANS, f.state_color(file.state));
            f.state_icon(file.state, x + w - 44.0, ry + 33.0, 13.0);
            if let Some(o) = &file.output {
                f.hit((x + w - 300.0, ry, 300.0, 66.0), Act::OpenPath(o.clone()));
            }
        }
        rows.len() as f32 * 66.0
    });
}

pub fn draw(f: &mut Frame, app: &App) {
    chrome(f, app);
    let pal = f.pal;
    let simple = f.simple;
    if app.activity != Activity::Convert {
        let r = (60.0, 76.0, f.w - 120.0, f.h - 100.0);
        match app.activity {
            Activity::Transcribe => transcribe_page(f, app, r, simple),
            Activity::YouTube => links_page(f, app, r, false, simple),
            _ => links_page(f, app, r, true, simple),
        }
        return;
    }
    let has_files = !app.visible().is_empty();
    let step = if has_files { f.ui.step.min(2) } else { 0 };
    let reach = if has_files { 2 } else { 0 };
    stepper(f, 85.0, step, reach);
    if simple {
        super::essenziale::mode_switch(f, (f.w - 400.0, 118.0, 340.0, 52.0));
    }
    let w = (f.w - 300.0).clamp(560.0, 1237.0);
    let x = (f.w - w) / 2.0;
    let top = if simple { 120.0 } else { 128.0 };
    let foot_y = f.h - FOOT;
    let bottom = foot_y - 22.0;
    match step {
        0 => step_files(f, app, x, w, top, bottom, simple),
        1 => step_action(f, app, x, w, top, bottom, simple),
        _ => step_save(f, app, x, w, top, bottom, simple),
    }
    // Il piede.
    f.hline(60.0, foot_y, f.w - 120.0);
    let by = foot_y + 22.0;
    if step > 0 {
        f.button((78.0, by, 191.0, 59.0), "Indietro", Some(ic::ARROW_LEFT), 20.0, Act::Back);
    }
    let note = if step == 2 && app.busy() { "Sto lavorando sui file…".to_string() } else { "L'originale resta al sicuro.".to_string() };
    let nw = f.measure(&note, 17.0, SANS);
    let nx = (f.w - nw - 40.0) / 2.0;
    f.icon(ic::CHECK, nx, by + 17.0, 26.0, pal.ok);
    f.text(&note, nx + 38.0, by, nw + 4.0, 59.0, 17.0, SANS, pal.muted);
    let bw = 253.0;
    let br = (f.w - 77.0 - bw, by, bw, 59.0);
    if step < 2 {
        f.primary(br, "Continua", Some(ic::ARROW_RIGHT), 21.0, Act::Next, has_files);
    } else {
        let label = super::go_label(app);
        let ready = !app.targets().is_empty() && !app.busy();
        f.primary(br, &label, Some(ic::CONVERT), 21.0, Act::Start, ready);
        super::go_extras(f, app, br, true);
        if app.busy() {
            if let Some(p) = app.progress() {
                f.p.rrect(br.0, br.1 + br.3 + 8.0, bw, 5.0, 2.5, Some(pal.border), None);
                f.p.rrect(br.0, br.1 + br.3 + 8.0, bw * p, 5.0, 2.5, Some(pal.primary), None);
            }
        }
    }
}
