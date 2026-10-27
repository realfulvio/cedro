//! Le pagine che le tavole non disegnano: Trascrizione, YouTube e Video dal web. Sono le stesse
//! in tutti i temi (con i colori del tema) e hanno le funzioni reali del motore: nessuna scelta
//! in più di quelle che `trascrizione` e `youtube` sanno fare.

use super::*;
use crate::trascrizione::{self, Tone};
use crate::ui::core::{LinkFlag, LinkMenu, Scroll};
use crate::ui::model::{FileState, Links, SUBTITLES};

const INSP_W: f32 = 380.0;

/// Il titolo della pagina con l'avviso accanto; restituisce la y dove comincia il resto.
fn header(f: &mut Frame, app: &App, r: R, title: &str, simple: bool) -> f32 {
    let pal = f.pal;
    let size = if simple { 40.0 } else { 34.0 };
    f.text(title, r.0, r.1, 420.0, 50.0, size, SEMI, pal.text);
    let nx = r.0 + f.measure(title, size, SEMI) + 32.0;
    super::notice(f, app, (nx, r.1 + 4.0, (r.0 + r.2 - nx).min(760.0), 42.0), 15.0);
    r.1 + 66.0
}

/// Il riquadro «Cosa sta succedendo» con il pulsante Copia. Le righe arrivano dal motore.
fn log_box(f: &mut Frame, r: R, lines: &[(Tone, String, String)]) {
    let pal = f.pal;
    f.card(r, 10.0);
    f.text("Cosa sta succedendo", r.0 + 18.0, r.1 + 8.0, r.2 - 140.0, 32.0, 16.0, SEMI, pal.text);
    f.tool(r.0 + r.2 - 110.0, r.1 + 6.0, 34.0, "Copia", ic::COPY, 15.0, Act::CopyLog, !lines.is_empty());
    let body = (r.0 + 1.0, r.1 + 44.0, r.2 - 2.0, r.3 - 50.0);
    if lines.is_empty() {
        f.text("Qui compaiono i passi del lavoro e gli eventuali errori.", r.0 + 18.0, body.1, r.2 - 36.0, 26.0, 14.5, SANS, pal.muted);
        return;
    }
    let row = 22.0;
    let total = lines.len() as f32 * row;
    // Si vede sempre la fine: lo scorrimento parte dal fondo.
    let max = (total - body.3).max(0.0);
    let off = f.ui.scroll(Scroll::Log);
    let start_y = body.1 - (max - off.min(max));
    f.p.push_clip(body.0, body.1, body.2, body.3);
    for (i, (tone, key, text)) in lines.iter().enumerate() {
        let y = start_y + i as f32 * row;
        if y + row < body.1 || y > body.1 + body.3 {
            continue;
        }
        let mut x = r.0 + 18.0;
        if !key.is_empty() {
            let kc = if *tone == Tone::Error { pal.bad } else { pal.primary };
            f.p.text(key, x, y, 200.0, row, 14.0, SEMI, kc, Align::Left);
            x += f.measure(key, 14.0, SEMI) + 8.0;
        }
        let c = match tone {
            Tone::Error => pal.bad,
            Tone::Note => pal.muted,
            _ => pal.text,
        };
        f.p.text(text, x, y, r.0 + r.2 - 18.0 - x, row, 14.0, SANS, c, Align::Left);
    }
    f.p.pop_clip();
    // Lo scorrimento qui conta al contrario (0 = fondo): la rotella verso l'alto mostra le righe vecchie.
    f.scrolls.push((body, Scroll::Log, max));
}

fn transcribe_lines(app: &App) -> Vec<(Tone, String, String)> {
    app.transcribe.log.iter().map(|l| (l.tone, l.key.clone(), l.text.clone())).collect()
}

fn link_lines(l: &Links) -> Vec<(Tone, String, String)> {
    l.log.lines().map(|s| (if s.starts_with("Errore") { Tone::Error } else { Tone::Note }, String::new(), s.to_string())).collect()
}

/// La riga di un file da trascrivere (o di un link): nome, sotto lo stato, a destra l'icona.
fn job_row(f: &mut Frame, r: R, title: &str, sub: &str, state: FileState, percent: Option<f32>, big: bool, remove: Option<Act>, open: Option<Act>) {
    let pal = f.pal;
    let (ts, ss) = if big { (20.0, 17.0) } else { (16.5, 14.5) };
    let right = 150.0;
    f.text(title, r.0 + 18.0, r.1 + r.3 * 0.16, r.2 - right - 24.0, r.3 * 0.36, ts, SEMI, pal.text);
    let sub_c = match state {
        FileState::Failed => pal.bad,
        FileState::Done => pal.ok,
        _ => pal.muted,
    };
    f.text(sub, r.0 + 18.0, r.1 + r.3 * 0.52, r.2 - right - 24.0, r.3 * 0.32, ss, SANS, sub_c);
    if let (FileState::Running, Some(p)) = (state, percent) {
        let bw = right - 60.0;
        let bx = r.0 + r.2 - right;
        f.p.rrect(bx, r.1 + r.3 / 2.0 - 3.0, bw, 6.0, 3.0, Some(pal.border), None);
        f.p.rrect(bx, r.1 + r.3 / 2.0 - 3.0, bw * (p / 100.0).clamp(0.0, 1.0), 6.0, 3.0, Some(pal.primary), None);
        f.text_r(&format!("{}%", p.round() as u32), (bx + bw, r.1, 50.0, r.3), 14.0, SANS, pal.muted);
        return;
    }
    let cx = r.0 + r.2 - 40.0;
    let cy = r.1 + r.3 / 2.0;
    match (state, remove) {
        (FileState::Waiting, Some(act)) => {
            if f.hovered(&act) {
                f.p.circle(cx, cy, 18.0, Some(pal.alt), None);
            }
            f.icon(ic::CLOSE, cx - 11.0, cy - 11.0, 22.0, pal.text);
            f.hit((cx - 20.0, cy - 20.0, 40.0, 40.0), act);
        }
        _ => {
            f.state_icon(state, cx, cy, 13.0);
            if let Some(o) = open {
                f.hit((cx - 20.0, cy - 20.0, 40.0, 40.0), o);
            }
        }
    }
}

/// Il pulsante principale delle pagine: avvia, oppure annulla il lavoro in corso.
fn go_or_cancel(f: &mut Frame, app: &App, r: R, size: f32, icon: &str, ready: bool) {
    if app.busy() {
        let pal = f.pal;
        f.button(r, "Annulla", Some(ic::CLOSE), size, Act::Cancel);
        if let Some(p) = app.progress() {
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2, 5.0, 2.5, Some(pal.border), None);
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2 * p.clamp(0.0, 1.0), 5.0, 2.5, Some(pal.primary), None);
        }
    } else {
        let label = super::go_label(app);
        f.primary(r, &label, Some(icon), size, Act::Start, ready);
    }
}

/// La scelta di due voci affiancate (Word | Testo, Video | Solo audio).
fn pair(f: &mut Frame, r: R, labels: [&str; 2], icons: [&str; 2], first_on: bool, acts: [Act; 2], size: f32) {
    let pal = f.pal;
    let half = (r.2 - 10.0) / 2.0;
    for i in 0..2 {
        let on = (i == 0) == first_on;
        let b = (r.0 + i as f32 * (half + 10.0), r.1, half, r.3);
        let hot = f.hovered(&acts[i]);
        let bg = if on { mix(pal.primary, pal.surface, 0.88) } else if hot { pal.alt } else { pal.surface };
        f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(bg), Some((if on { pal.primary } else { pal.border }, if on { 1.6 } else { 1.0 })));
        let isz = (size * 1.3).round();
        let tw = f.measure(labels[i], size, if on { SEMI } else { SANS });
        let sx = b.0 + (b.2 - tw - isz - 10.0) / 2.0;
        let c = if on { pal.primary } else { pal.text };
        f.icon(icons[i], sx, b.1 + (b.3 - isz) / 2.0, isz, c);
        f.text(labels[i], sx + isz + 10.0, b.1, tw + 4.0, b.3, size, if on { SEMI } else { SANS }, c);
        f.focus_ring(b, 8.0, &acts[i]);
        f.hit(b, acts[i].clone());
    }
}

/// La precisione della trascrizione con il peso del modello se va ancora scaricato.
fn model_note(app: &App) -> String {
    let m = &trascrizione::MODELS[app.transcribe.model.min(trascrizione::MODELS.len() - 1)];
    if trascrizione::model_ready(app.transcribe.model) {
        format!("{} · pronto sul computer", m.note)
    } else {
        format!("{} · al primo uso si scarica ({})", m.note, trascrizione::size_text(m.megabytes))
    }
}

fn model_name(app: &App) -> &'static str {
    trascrizione::MODELS[app.transcribe.model.min(trascrizione::MODELS.len() - 1)].name
}

/// La coda dei file da trascrivere, con la zona vuota quando non c'è niente.
fn transcribe_queue(f: &mut Frame, app: &App, r: R, big: bool) {
    let rows = app.visible();
    if rows.is_empty() {
        super::drop_zone(f, app, r, big);
        return;
    }
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let rh = if big { 92.0 } else { 70.0 };
    let running = app.files.iter().position(|x| x.state == FileState::Running);
    super::scrolled(f, (r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, r.3 - 2.0), Scroll::Queue, |f| {
        for (n, &i) in rows.iter().enumerate() {
            let y = r.1 + n as f32 * rh;
            if n > 0 {
                f.hline(r.0, y, r.2);
            }
            let file = &app.files[i];
            let isz = rh * 0.5;
            f.kind_icon(file.kind, r.0 + 16.0, y + (rh - isz) / 2.0, isz);
            let (size, _) = super::file_meta(app, i);
            let sub = if file.state == FileState::Waiting && !size.is_empty() { format!("{} · {size}", file.path.parent().map(|p| p.display().to_string()).unwrap_or_default()) } else { file.message.clone() };
            let pct = if running == Some(i) { Some(app.transcribe.percent as f32) } else { None };
            let open = file.output.clone().map(Act::OpenPath);
            job_row(f, (r.0 + isz + 16.0, y, r.2 - isz - 16.0, rh), &file.name(), &sub, file.state, pct, big, (!app.busy()).then_some(Act::Remove(i)), open);
        }
        rows.len() as f32 * rh
    });
}

/// La pagina della Trascrizione dentro il riquadro `r`.
pub fn transcribe_page(f: &mut Frame, app: &App, r: R, simple: bool) {
    if r.2 < 720.0 {
        return narrow_transcribe(f, app, r, simple);
    }
    let pal = f.pal;
    let top = header(f, app, r, "Trascrizione", simple);
    let ready = !app.targets().is_empty();
    if simple {
        // Una colonna: i file, il risultato, la precisione, il pulsante; lo stato sempre in vista.
        let w = r.2.min(980.0);
        let x = r.0 + (r.2 - w) / 2.0;
        f.button((x + w - 200.0, r.1 + 2.0, 200.0, 46.0), "Aggiungi file", Some(ic::ADD), 17.0, Act::AddFiles);
        let list_h = (r.1 + r.3 - top - 330.0).max(140.0);
        transcribe_queue(f, app, (x, top, w, list_h), true);
        let mut y = top + list_h + 22.0;
        f.text("Risultato:", x, y, 160.0, 56.0, 21.0, SEMI, pal.text);
        let word = app.transcribe.format == trascrizione::Format::Word;
        pair(f, (x + 170.0, y, w - 170.0, 56.0), ["Documento Word", "Testo semplice"], [ic::FILE, ic::MENU], word, [Act::TrFormat(true), Act::TrFormat(false)], 18.0);
        y += 74.0;
        f.text("Precisione:", x, y, 160.0, 50.0, 21.0, SEMI, pal.text);
        f.select((x + 170.0, y, 280.0, 50.0), model_name(app), None, 18.0, Act::TrModelMenu);
        f.text(&model_note(app), x + 470.0, y, w - 470.0, 50.0, 16.0, SANS, pal.muted);
        y += 72.0;
        go_or_cancel(f, app, (x, y, w, 76.0), 24.0, ic::TRANSCRIBE, ready);
        y += 96.0;
        let last = app.transcribe.log.last().map(|l| if l.key.is_empty() { l.text.clone() } else { format!("{} {}", l.key, l.text) });
        let line = last.unwrap_or_else(|| super::dest_note(app).replace("I file convertiti verranno salvati", "I testi verranno salvati"));
        let err = app.transcribe.log.last().is_some_and(|l| l.tone == Tone::Error);
        f.text_c(&line, (x, y, w, 28.0), 17.0, SANS, if err { pal.bad } else { pal.muted });
        return;
    }
    let left_w = r.2 - INSP_W - 20.0;
    // Barra della coda.
    let mut x = r.0;
    f.primary((x, top, 190.0, 42.0), "Aggiungi file", Some(ic::ADD), 16.0, Act::AddFiles, true);
    x += 206.0;
    x += f.tool(x, top + 1.0, 40.0, "Aggiungi cartella", ic::FOLDER, 15.0, Act::AddFolder, true) + 6.0;
    f.tool(x, top + 1.0, 40.0, "Rimuovi", ic::TRASH, 15.0, Act::RemoveChecked, !app.visible().is_empty() && !app.busy());
    let n = app.visible().len();
    f.text_r(&format!("{n} file"), (r.0 + left_w - 120.0, top, 120.0, 42.0), 15.0, SANS, pal.muted);
    let log_h = 190.0;
    let qy = top + 58.0;
    let qh = (r.1 + r.3 - qy - log_h - 18.0).max(120.0);
    transcribe_queue(f, app, (r.0, qy, left_w, qh), false);
    log_box(f, (r.0, qy + qh + 18.0, left_w, log_h), &transcribe_lines(app));

    // L'ispettore: risultato, precisione, destinazione, pulsante.
    let ix = r.0 + left_w + 20.0;
    let ir = (ix, top, INSP_W, r.1 + r.3 - top);
    f.card(ir, 12.0);
    let (cx, cw) = (ix + 24.0, INSP_W - 48.0);
    let mut y = top + 20.0;
    f.text("Impostazioni di trascrizione", cx, y, cw, 30.0, 19.0, SEMI, pal.text);
    y += 50.0;
    f.text("Risultato", cx, y, cw, 24.0, 16.0, SANS, pal.text);
    let word = app.transcribe.format == trascrizione::Format::Word;
    pair(f, (cx, y + 30.0, cw, 46.0), ["Word", "Testo"], [ic::FILE, ic::MENU], word, [Act::TrFormat(true), Act::TrFormat(false)], 16.0);
    y += 30.0 + 46.0 + 26.0;
    f.text("Precisione", cx, y, cw, 24.0, 16.0, SANS, pal.text);
    let m = &trascrizione::MODELS[app.transcribe.model.min(trascrizione::MODELS.len() - 1)];
    f.select((cx, y + 30.0, cw, 42.0), &format!("{} · {}", m.name, trascrizione::size_text(m.megabytes)), None, 16.0, Act::TrModelMenu);
    y += 30.0 + 42.0 + 8.0;
    y += f.para(&model_note(app), cx, y, cw, 14.0, SANS, pal.muted, 1.4, 2) + 22.0;
    f.text("Cartella di destinazione", cx, y, cw, 24.0, 16.0, SANS, pal.text);
    super::dest_select(f, app, (cx, y + 30.0, cw, 42.0), 16.0);
    y += 30.0 + 42.0 + 8.0;
    let note = super::dest_note(app).replace("I file convertiti verranno salvati", "I testi verranno salvati");
    f.para(&note, cx, y, cw, 14.0, SANS, pal.muted, 1.4, 3);
    let gy = ir.1 + ir.3 - 24.0 - 64.0 - 14.0;
    go_or_cancel(f, app, (cx, gy, cw, 64.0), 20.0, ic::TRANSCRIBE, ready);
}

/// Il campo del link con «Aggiungi» accanto.
fn link_input(f: &mut Frame, l: &Links, r: R, big: bool) {
    let pal = f.pal;
    let web = l.web;
    let bw = if big { 170.0 } else { 140.0 };
    let size = if big { 18.0 } else { 16.0 };
    let hint = if web { "https://… (la pagina del video)" } else { "https://www.youtube.com/watch?v=…" };
    f.field((r.0, r.1, r.2 - bw - 12.0, r.3), Field::Link(web), &l.typed, hint, size, false);
    f.icon(ic::LINK, r.0 + r.2 - bw - 12.0 - 36.0, r.1 + (r.3 - 22.0) / 2.0, 22.0, pal.muted);
    f.primary((r.0 + r.2 - bw, r.1, bw, r.3), "Aggiungi", Some(ic::ADD), size, Act::LinkAdd(web), !l.typed.trim().is_empty());
}

fn links_list(f: &mut Frame, app: &App, l: &Links, r: R, big: bool, scroll: bool) {
    let pal = f.pal;
    let web = l.web;
    if l.items.is_empty() {
        f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(mix(pal.alt, pal.surface, 0.4)), None);
        f.p.rrect_line(r.0, r.1, r.2, r.3, 10.0, mix(pal.primary, pal.surface, 0.35), 2.0, 1.0, Some((9.0, 6.0)));
        let cy = r.1 + r.3 / 2.0;
        let isz = if big { 40.0 } else { 32.0 };
        f.icon(if web { ic::WEB } else { ic::VIDEO }, r.0 + (r.2 - isz) / 2.0, cy - isz - 8.0, isz, pal.primary);
        f.text_c("Nessun link in coda", (r.0, cy, r.2, 30.0), if big { 21.0 } else { 18.0 }, SEMI, pal.text);
        let hint = if web { super::super::model::WEB_HINT } else { super::super::model::YT_HINT };
        f.text_c(hint, (r.0 + 16.0, cy + 30.0, r.2 - 32.0, 26.0), if big { 16.0 } else { 14.0 }, SANS, pal.muted);
        return;
    }
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let rh = if big { 88.0 } else { 68.0 };
    let items = l.items.clone();
    let busy = app.busy();
    let mut body = |f: &mut Frame| {
        for (n, it) in items.iter().enumerate() {
            let y = r.1 + n as f32 * rh;
            if n > 0 {
                f.hline(r.0, y, r.2);
            }
            let isz = rh * 0.42;
            f.icon(if web { ic::WEB } else { ic::VIDEO }, r.0 + 18.0, y + (rh - isz) / 2.0, isz, pal.primary);
            let title = if it.title.is_empty() { it.url.clone() } else { it.title.clone() };
            let sub = match (it.state, it.detail.is_empty()) {
                (FileState::Waiting, false) => format!("{} · {}", it.detail, it.message),
                _ => it.message.clone(),
            };
            let pct = (it.state == FileState::Running).then_some(it.percent);
            job_row(f, (r.0 + isz + 22.0, y, r.2 - isz - 22.0, rh), &title, &sub, it.state, pct, big, (!busy).then_some(Act::LinkRemove(web, n)), None);
        }
        items.len() as f32 * rh
    };
    if scroll {
        super::scrolled(f, (r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, r.3 - 2.0), Scroll::Links, body);
    } else {
        body(f);
    }
}

/// Tutte le scelte del download (tipo, formato, qualità, sottotitoli, opzioni, cartella), dall'alto
/// `y0`; restituisce la y dove finiscono. Le usano sia l'ispettore sia la finestrella stretta.
fn link_settings(f: &mut Frame, l: &Links, cx: f32, y0: f32, cw: f32) -> f32 {
    let pal = f.pal;
    let web = l.web;
    let mut y = y0;
    pair(f, (cx, y, cw, 46.0), ["Video", "Solo audio"], [ic::VIDEO, ic::AUDIO], !l.audio, [Act::LinkAudio(web, false), Act::LinkAudio(web, true)], 16.0);
    y += 46.0 + 22.0;
    let pick = |f: &mut Frame, y: f32, label: &str, value: &str, which: LinkMenu| -> f32 {
        f.text(label, cx, y, cw, 24.0, 16.0, SANS, pal.text);
        f.select((cx, y + 28.0, cw, 42.0), value, None, 16.0, Act::LinkMenu(web, which));
        28.0 + 42.0 + 18.0
    };
    y += pick(f, y, "Salva come", &l.formats().get(l.format).copied().unwrap_or("").to_uppercase(), LinkMenu::Format);
    y += pick(f, y, "Qualità", l.qualities().get(l.quality).map(|q| q.0).unwrap_or(""), LinkMenu::Quality);
    if !l.audio {
        y += pick(f, y, "Sottotitoli", SUBTITLES.get(l.subtitles).map(|s| s.0).unwrap_or(""), LinkMenu::Subtitles);
    }
    let flag = |f: &mut Frame, y: f32, label: &str, on: bool, which: LinkFlag| -> f32 {
        f.text(label, cx, y, cw - 68.0, 28.0, 15.5, SANS, pal.text);
        f.switch(cx + cw - 52.0, y, 52.0, 28.0, on, Act::LinkFlag(web, which), None);
        42.0
    };
    y += flag(f, y, "Copertina nel file", l.thumbnail, LinkFlag::Thumbnail);
    y += flag(f, y, "Capitoli e descrizione nel file", l.chapters, LinkFlag::Chapters);
    // Le altre in «Altre opzioni»: quelle che valgono solo per YouTube o solo per l'audio compaiono lì dove servono.
    let mut more: Vec<(&str, bool, LinkFlag)> = vec![("Dividi per capitoli", l.split_chapters, LinkFlag::Split)];
    if !web {
        more.push(("Salta le parti sponsorizzate", l.skip_sponsors, LinkFlag::Sponsors));
    }
    if l.audio {
        more.push(("Livella il volume", l.normalize, LinkFlag::Normalize));
    }
    more.push((if web { "Raccolte: scarica tutto" } else { "Playlist: scarica tutta" }, l.playlist, LinkFlag::Playlist));
    more.push(("Salta i già scaricati", l.skip_existing, LinkFlag::SkipExisting));
    let open = f.ui.is_open(Fold::LinkMore);
    f.hline(cx, y, cw);
    y += 8.0;
    f.text(&format!("Altre opzioni ({})", more.len()), cx, y, cw - 40.0, 34.0, 16.0, SEMI, pal.text);
    f.icon(if open { "M6 15l6-6 6 6" } else { ic::CHEVRON_DOWN }, cx + cw - 28.0, y + 5.0, 24.0, pal.text);
    f.hit((cx, y, cw, 34.0), Act::Fold(Fold::LinkMore));
    y += 46.0;
    if open {
        for (label, on, which) in more {
            y += flag(f, y, label, on, which);
        }
    }
    f.text("Cartella di destinazione", cx, y, cw, 24.0, 16.0, SANS, pal.text);
    let dr = (cx, y + 28.0, cw, 42.0);
    f.select(dr, &crate::ui::model::short_dir(&l.dest), Some(ic::FOLDER), 16.0, Act::LinkDest(web));
    y += 28.0 + 42.0 + 8.0;
    y += f.para(&l.dest.display().to_string(), cx, y, cw, 13.5, SANS, pal.muted, 1.4, 2);
    y
}

/// La pagina di YouTube (`web` = false) o di Video dal web (`web` = true).
pub fn links_page(f: &mut Frame, app: &App, r: R, web: bool, simple: bool) {
    if r.2 < 720.0 {
        return narrow_links(f, app, r, web, simple);
    }
    let pal = f.pal;
    let l = app.links(web);
    let title = if web { "Video dal web" } else { "YouTube" };
    let top = header(f, app, r, title, simple);
    let ready = !l.items.is_empty() || !l.typed.trim().is_empty();
    if simple {
        let w = r.2.min(980.0);
        let x = r.0 + (r.2 - w) / 2.0;
        link_input(f, l, (x, top, w, 56.0), true);
        let list_h = (r.1 + r.3 - top - 76.0 - 300.0).max(140.0);
        links_list(f, app, l, (x, top + 76.0, w, list_h), true, true);
        let mut y = top + 76.0 + list_h + 22.0;
        f.text("Scarica:", x, y, 160.0, 56.0, 21.0, SEMI, pal.text);
        pair(f, (x + 170.0, y, w - 170.0, 56.0), ["Video", "Solo audio"], [ic::VIDEO, ic::AUDIO], !l.audio, [Act::LinkAudio(web, false), Act::LinkAudio(web, true)], 18.0);
        y += 80.0;
        go_or_cancel(f, app, (x, y, w, 76.0), 24.0, ic::DOWNLOAD, ready);
        y += 96.0;
        let line = if l.note.is_empty() { l.summary() } else { l.note.clone() };
        f.text_c(&line, (x, y, w, 28.0), 17.0, SANS, pal.muted);
        let tw = f.measure("Cambia cartella", 15.0, SEMI);
        let cr = (x + (w - tw) / 2.0, y + 32.0, tw, 24.0);
        f.p.text("Cambia cartella", cr.0, cr.1, cr.2 + 2.0, cr.3, 15.0, SEMI, pal.primary, Align::Left);
        f.hit(cr, Act::LinkDest(web));
        return;
    }
    let left_w = r.2 - INSP_W - 20.0;
    link_input(f, l, (r.0, top, left_w, 46.0), false);
    let log_h = 190.0;
    let qy = top + 62.0;
    let qh = (r.1 + r.3 - qy - log_h - 18.0).max(120.0);
    links_list(f, app, l, (r.0, qy, left_w, qh), false, true);
    log_box(f, (r.0, qy + qh + 18.0, left_w, log_h), &link_lines(l));

    let ix = r.0 + left_w + 20.0;
    let ir = (ix, top, INSP_W, r.1 + r.3 - top);
    f.card(ir, 12.0);
    let (cx, cw) = (ix + 24.0, INSP_W - 48.0);
    let body = (ix + 1.0, top + 1.0, INSP_W - 2.0, ir.3 - 120.0);
    let lc = l.clone();
    super::scrolled(f, body, Scroll::Inspector, |f| {
        f.text("Impostazioni del download", cx, top + 20.0, cw, 30.0, 19.0, SEMI, pal.text);
        link_settings(f, &lc, cx, top + 68.0, cw) - top + 16.0
    });
    let gy = ir.1 + ir.3 - 24.0 - 64.0 - 14.0;
    f.hline(ix, gy - 22.0, INSP_W);
    go_or_cancel(f, app, (cx, gy, cw, 64.0), 20.0, ic::DOWNLOAD, ready);
}

/// Le righe dei file da trascrivere in una colonna stretta (senza scorrimento proprio).
fn narrow_files(f: &mut Frame, app: &App, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let rows = app.visible();
    if rows.is_empty() {
        super::drop_zone(f, app, (x, y, w, 150.0), false);
        return 150.0;
    }
    let rh = 66.0;
    f.p.rrect(x, y, w, rows.len() as f32 * rh, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    let running = app.files.iter().position(|x| x.state == FileState::Running);
    for (n, &i) in rows.iter().enumerate() {
        let ry = y + n as f32 * rh;
        if n > 0 {
            f.hline(x, ry, w);
        }
        let file = &app.files[i];
        f.kind_icon(file.kind, x + 12.0, ry + 14.0, 38.0);
        let (size, _) = super::file_meta(app, i);
        let sub = if file.state == FileState::Waiting { size } else { file.message.clone() };
        let pct = if running == Some(i) { Some(app.transcribe.percent as f32) } else { None };
        job_row(f, (x + 56.0, ry, w - 56.0, rh), &file.name(), &sub, file.state, pct, false, (!app.busy()).then_some(Act::Remove(i)), file.output.clone().map(Act::OpenPath));
    }
    rows.len() as f32 * rh
}

fn narrow_transcribe(f: &mut Frame, app: &App, r: R, simple: bool) {
    let pal = f.pal;
    let mut top = r.1;
    if super::notice(f, app, (r.0, top, r.2, 44.0), 14.0) {
        top += 54.0;
    }
    let gh = 56.0;
    let go_y = r.1 + r.3 - gh - 10.0;
    let body = (r.0, top, r.2, go_y - top - 14.0);
    let (x, w) = (r.0, r.2);
    let lines = transcribe_lines(app);
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut y = body.1;
        f.button((x, y, 168.0, 40.0), "Aggiungi file", Some(ic::ADD), 15.0, Act::AddFiles);
        f.text_r(&format!("{} file", app.visible().len()), (x + w - 100.0, y, 100.0, 40.0), 14.0, SANS, pal.muted);
        y += 52.0;
        y += narrow_files(f, app, x, y, w) + 20.0;
        f.text("Risultato", x, y, w, 24.0, 15.0, SEMI, pal.text);
        let word = app.transcribe.format == trascrizione::Format::Word;
        pair(f, (x, y + 28.0, w, 44.0), ["Word", "Testo"], [ic::FILE, ic::MENU], word, [Act::TrFormat(true), Act::TrFormat(false)], 15.0);
        y += 72.0 + 20.0;
        f.text("Precisione", x, y, w, 24.0, 15.0, SEMI, pal.text);
        f.select((x, y + 28.0, w, 42.0), model_name(app), None, 15.0, Act::TrModelMenu);
        y += 70.0 + 8.0;
        y += f.para(&model_note(app), x, y, w, 13.5, SANS, pal.muted, 1.4, 3) + 16.0;
        f.text("Cartella di destinazione", x, y, w, 24.0, 15.0, SEMI, pal.text);
        super::dest_select(f, app, (x, y + 28.0, w, 42.0), 15.0);
        y += 70.0 + 8.0;
        y += f.para(&super::dest_note(app).replace("I file convertiti verranno salvati", "I testi verranno salvati"), x, y, w, 13.5, SANS, pal.muted, 1.4, 3) + 16.0;
        if !simple {
            log_box(f, (x, y, w, 170.0), &lines);
            y += 186.0;
        }
        y - body.1
    });
    go_or_cancel(f, app, (r.0, go_y, r.2, gh), 18.0, ic::TRANSCRIBE, !app.targets().is_empty());
}

fn narrow_links(f: &mut Frame, app: &App, r: R, web: bool, simple: bool) {
    let pal = f.pal;
    let l = app.links(web);
    let mut top = r.1;
    if super::notice(f, app, (r.0, top, r.2, 44.0), 14.0) {
        top += 54.0;
    }
    let gh = 56.0;
    let go_y = r.1 + r.3 - gh - 10.0;
    let body = (r.0, top, r.2, go_y - top - 14.0);
    let (x, w) = (r.0, r.2);
    let lc = l.clone();
    let ready = !l.items.is_empty() || !l.typed.trim().is_empty();
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let l = &lc;
        let mut y = body.1;
        link_input(f, l, (x, y, w, 46.0), false);
        y += 58.0;
        let lh = (l.items.len().max(1) as f32 * 68.0).max(150.0);
        links_list(f, app, l, (x, y, w, lh), false, false);
        y += lh + 20.0;
        if simple {
            f.text("Scarica", x, y, w, 24.0, 15.0, SEMI, pal.text);
            pair(f, (x, y + 28.0, w, 44.0), ["Video", "Solo audio"], [ic::VIDEO, ic::AUDIO], !l.audio, [Act::LinkAudio(web, false), Act::LinkAudio(web, true)], 15.0);
            y += 72.0 + 14.0;
            let line = if l.note.is_empty() { l.summary() } else { l.note.clone() };
            y += f.para(&line, x, y, w, 14.0, SANS, pal.muted, 1.4, 3) + 8.0;
        } else {
            y = link_settings(f, l, x, y, w) + 12.0;
            log_box(f, (x, y, w, 170.0), &link_lines(l));
            y += 186.0;
        }
        y - body.1
    });
    go_or_cancel(f, app, (r.0, go_y, r.2, gh), 18.0, ic::DOWNLOAD, ready);
}
