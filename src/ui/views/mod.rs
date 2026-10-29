//! Le disposizioni dei temi. Ogni tema disegna la sua finestra con le misure delle tavole del kit
//! (canvas 1536×1024) e la allarga o la stringe tenendo fermi navigazione e piede: cresce la parte
//! centrale. Qui ci sono anche i pezzi che tutti usano: le opzioni del formato scelto, le pagine
//! di trascrizione e download, gli avvisi e la domanda «il file esiste già».

mod activity;
mod banco;
mod compatto;
mod documento;
mod essenziale;
mod guidato;
mod pagine;

pub use activity::{links_page, transcribe_page};
pub use pagine::{merged_pages, page_drop, split_file, BOARD_MAX, MERGE as ICON_MERGE, MERGED_MAX, SCISSORS as ICON_SCISSORS, SHRINK as ICON_SHRINK, STRIP_MAX};

use super::assets as ic;
use super::core::{Act, Field, Fold, Frame, Scroll, BOLD, R, SANS, SEMI};
use super::gfx::Align;
use super::model::{Activity, App};
use super::screen::Surface;
use super::theme::{mix, ThemeId};
use crate::conversioni::{self, Control, OptionDef};
use crate::engine::{ExistsChoice, Kind};

/// Quanto è grande la finestra appena aperta, in unità di disegno.
pub fn preferred_size(theme: ThemeId, simple: bool, surface: Surface) -> (f32, f32) {
    match (theme, surface) {
        (_, Surface::Compact) | (ThemeId::CedroCompatto, Surface::Main) => compatto::size(simple),
        _ => (1536.0, 1024.0),
    }
}

/// Sotto questa misura le disposizioni si accavallano: la finestra più piccola si disegna rimpicciolita
/// (vedi `Screen::fit_scale`) invece di stringere i pezzi.
pub fn min_size(theme: ThemeId, surface: Surface) -> (f32, f32) {
    match (theme, surface) {
        (_, Surface::Compact) | (ThemeId::CedroCompatto, Surface::Main) => (480.0, 560.0),
        _ => (1400.0, 920.0),
    }
}

pub fn draw(f: &mut Frame, app: &App, surface: Surface) {
    if surface == Surface::Compact {
        compatto::draw(f, app);
        return;
    }
    match f.theme {
        ThemeId::WindowsEssenziale | ThemeId::Bondeno => essenziale::draw(f, app),
        ThemeId::PercorsoGuidato => guidato::draw(f, app),
        ThemeId::CedroCompatto => compatto::draw_full(f, app),
        ThemeId::BancoDiLavoro => banco::draw(f, app),
        ThemeId::DocumentoAlCentro => documento::draw(f, app),
    }
}

/// Quello che sta sopra a tutto: i file trascinati, la domanda sul file che esiste, l'avviso.
pub fn overlays(f: &mut Frame, app: &App) {
    let pal = f.pal;
    if f.ui.dropping {
        f.p.fill_a(0.0, 0.0, f.w, f.h, pal.primary, 0.10);
        let (w, h) = (520.0f32.min(f.w - 40.0), 150.0);
        let (x, y) = ((f.w - w) / 2.0, (f.h - h) / 2.0);
        f.p.rrect(x, y, w, h, 14.0, Some(pal.surface), None);
        f.p.rrect_line(x, y, w, h, 14.0, pal.primary, 2.5, 1.0, Some((10.0, 6.0)));
        f.icon(ic::ADD, x + w / 2.0 - 18.0, y + 28.0, 36.0, pal.primary);
        f.text_c("Lascia qui i file da aggiungere", (x, y + 78.0, w, 40.0), 22.0, SEMI, pal.text);
    }
    if let Some(ask) = &app.ask {
        f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.35);
        f.hit((0.0, 0.0, f.w, f.h), Act::Nothing);
        let (w, h) = (620.0f32.min(f.w - 40.0), 250.0);
        let (x, y) = ((f.w - w) / 2.0, (f.h - h) / 2.0);
        f.p.shadow(x, y, w, h, 12.0, 16.0, 0.3, 6.0);
        f.card((x, y, w, h), 12.0);
        f.icon(ic::COPY, x + 28.0, y + 30.0, 28.0, pal.primary);
        f.text("Il file esiste già", x + 70.0, y + 26.0, w - 100.0, 36.0, 24.0, SEMI, pal.text);
        let name = ask.path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        f.para(&format!("«{name}» c'è già nella cartella di destinazione. Cosa vuoi fare?"), x + 70.0, y + 72.0, w - 100.0, 17.0, SANS, pal.muted, 1.4, 3);
        let bw = (w - 56.0 - 24.0) / 3.0;
        let by = y + h - 76.0;
        f.button((x + 28.0, by, bw, 48.0), "Salta", None, 17.0, Act::Answer(ExistsChoice::Skip));
        f.button((x + 40.0 + bw, by, bw, 48.0), "Tieni entrambi", None, 17.0, Act::Answer(ExistsChoice::KeepBoth));
        f.primary((x + 52.0 + 2.0 * bw, by, bw, 48.0), "Sostituisci", None, 17.0, Act::Answer(ExistsChoice::Replace), true);
    }
}

/// La riga dell'avviso (errore in rosso, «fatto» in verde) con la crocetta per chiuderla.
pub fn notice(f: &mut Frame, app: &App, r: R, size: f32) -> bool {
    let Some(n) = &app.notice else { return false };
    let pal = f.pal;
    let c = if n.error { pal.bad } else { pal.ok };
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(mix(c, pal.surface, 0.88)), Some((mix(c, pal.surface, 0.5), 1.0)));
    let isz = (size * 1.3).round();
    f.icon(if n.error { ic::WARNING } else { ic::CHECK }, r.0 + 14.0, r.1 + (r.3 - isz) / 2.0, isz, c);
    f.text(&n.text, r.0 + 24.0 + isz, r.1, r.2 - 80.0 - isz, r.3, size, SANS, pal.text);
    let cr = (r.0 + r.2 - 40.0, r.1 + (r.3 - 32.0) / 2.0, 32.0, 32.0);
    f.icon(ic::CLOSE, cr.0 + 6.0, cr.1 + 6.0, 20.0, pal.muted);
    f.hit(cr, Act::DismissNotice);
    true
}

/// Le misure dei pezzi delle opzioni (le tavole cambiano poco da un tema all'altro).
#[derive(Clone, Copy)]
pub struct OptStyle {
    pub label: f32,
    pub note: f32,
    pub control_h: f32,
    pub gap: f32,
    /// Opzioni prima di «Altre opzioni» (0 = tutte in vista).
    pub visible: usize,
    pub switch_w: f32,
    pub switch_h: f32,
}

pub const OPT_STD: OptStyle = OptStyle { label: 16.0, note: 14.0, control_h: 42.0, gap: 26.0, visible: 2, switch_w: 52.0, switch_h: 28.0 };

/// Vero se il motore usa la qualità per la voce scelta (JPEG, compressione, PDF di immagini, video).
pub fn uses_quality(kind: Kind, action: &str) -> bool {
    matches!((kind, action), (Kind::Image, "jpeg" | "compress" | "pdf") | (Kind::Pdf, "jpg") | (Kind::Video, "mp4"))
}

/// Il nome della voce scelta per la scelta «Formato».
pub fn action_title(app: &App, simple: bool) -> String {
    let kind = app.kind();
    if simple {
        if let Some(sf) = conversioni::simple_formats(kind).iter().find(|s| s.id == app.action) {
            return sf.title.to_string();
        }
    }
    app.action_label().to_string()
}

/// Il cursore della qualità con il numero accanto e le due parole agli estremi.
pub fn quality(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle, ends: bool) -> f32 {
    let pal = f.pal;
    f.text("Qualità", x, y, w - 80.0, 24.0, st.label, SANS, pal.text);
    f.numbox((x + w - 60.0, y - 10.0, 60.0, 40.0), &app.quality.to_string(), 17.0);
    let frac = (app.quality as f32 - 40.0) / 55.0;
    f.slider((x, y + 36.0, w, 24.0), frac, Act::Quality);
    let mut h = 64.0;
    if ends {
        f.text("Minore dimensione", x, y + 62.0, w / 2.0, 20.0, st.note, SANS, pal.muted);
        f.text_r("Qualità superiore", (x + w / 2.0, y + 62.0, w / 2.0, 20.0), st.note, SANS, pal.muted);
        h += 22.0;
    }
    h
}

/// Una opzione del motore (levetta, scelta, cursore, campo). Restituisce l'altezza usata.
pub fn option(f: &mut Frame, app: &App, def: &'static OptionDef, x: f32, y: f32, w: f32, st: OptStyle) -> f32 {
    let pal = f.pal;
    let note_lines = |f: &mut Frame, text: &str, top: f32, width: f32| -> f32 {
        if text.is_empty() {
            0.0
        } else {
            f.para(text, x, top, width, st.note, SANS, pal.muted, 1.45, 3)
        }
    };
    match &def.control {
        Control::Switch => {
            let on = app.values.get(def) != 0;
            f.text(def.title, x, y, w - st.switch_w - 16.0, 26.0, st.label + 1.0, SANS, pal.text);
            f.switch(x + w - st.switch_w, y - 1.0, st.switch_w, st.switch_h, on, Act::OptSwitch(def.opt), None);
            34.0 + note_lines(f, def.note, y + 34.0, w - st.switch_w - 16.0)
        }
        Control::Choice(choices) => {
            let v = app.values.get(def);
            let label = choices.iter().find(|c| c.value == v).map(|c| c.label).unwrap_or("");
            f.text(def.title, x, y, w, 24.0, st.label, SANS, pal.text);
            f.select((x, y + 28.0, w, st.control_h), label, None, 16.0, Act::OptMenu(def.opt));
            let top = y + 28.0 + st.control_h + 8.0;
            28.0 + st.control_h + 8.0 + note_lines(f, def.note, top, w)
        }
        Control::Range { min, max, unit } => {
            let v = app.values.get(def);
            f.text(def.title, x, y, w - 90.0, 24.0, st.label, SANS, pal.text);
            f.numbox((x + w - 80.0, y - 8.0, 80.0, 38.0), &format!("{v}{unit}"), 16.0);
            let frac = (v - min) as f32 / (*max - *min).max(1) as f32;
            f.slider((x, y + 36.0, w, 24.0), frac, Act::OptSlider(def.opt));
            66.0 + note_lines(f, def.note, y + 66.0, w)
        }
        Control::Text { hint } => {
            f.text(def.title, x, y, w, 24.0, st.label, SANS, pal.text);
            f.field((x, y + 28.0, w, st.control_h), Field::Opt(def.opt), app.values.text(def), hint, 16.0, false);
            let top = y + 28.0 + st.control_h + 8.0;
            28.0 + st.control_h + 8.0 + note_lines(f, def.note, top, w)
        }
        Control::Segments(choices) => {
            let v = app.values.get(def);
            f.text(def.title, x, y, w, 24.0, st.label, SANS, pal.text);
            // A gruppi di tre per riga, come nella tavola: «Ogni pagina · Ogni N · Intervalli», poi gli altri.
            let seg_h = (st.control_h - 2.0).max(34.0);
            let mut top = y + 28.0;
            for row in choices.chunks(3) {
                f.p.rrect(x, top, w, seg_h, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
                let cw = w / row.len() as f32;
                for (k, c) in row.iter().enumerate() {
                    let r = (x + k as f32 * cw, top, cw, seg_h);
                    let act = Act::OptPick(def.opt, c.value);
                    let on = c.value == v;
                    if on {
                        f.p.rrect(r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, r.3 - 2.0, 5.0, Some(pal.alt), None);
                        f.p.fill(r.0 + 1.0, r.1 + r.3 - 3.0, r.2 - 2.0, 3.0, pal.primary);
                    } else if f.hovered(&act) {
                        f.p.rrect(r.0 + 1.0, r.1 + 1.0, r.2 - 2.0, r.3 - 2.0, 5.0, Some(mix(pal.alt, pal.surface, 0.5)), None);
                    }
                    if k > 0 {
                        f.vline(r.0, r.1 + 1.0, r.3 - 2.0);
                    }
                    f.text_c(c.label, (r.0 + 4.0, r.1, r.2 - 8.0, r.3), st.note + 1.0, if on { SEMI } else { SANS }, if on { pal.primary } else { pal.text });
                    f.focus_ring(r, 6.0, &act);
                    f.hit(r, act);
                }
                top += seg_h + 6.0;
            }
            top - y + 2.0 + note_lines(f, def.note, top + 2.0, w)
        }
        Control::Position => {
            // La pagina in piccolo con i nove punti: angoli, lati e centro.
            let v = app.values.get(def);
            f.text(def.title, x, y + 20.0, w - 110.0, 24.0, st.label, SANS, pal.text);
            let (cw, ch, gap) = (28.0, 20.0, 4.0);
            let gx = x + w - (cw * 3.0 + gap * 2.0);
            f.p.rrect(gx - 5.0, y - 5.0, cw * 3.0 + gap * 2.0 + 10.0, ch * 3.0 + gap * 2.0 + 10.0, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
            for k in 0..9u32 {
                let r = (gx + (k % 3) as f32 * (cw + gap), y + (k / 3) as f32 * (ch + gap), cw, ch);
                let act = Act::OptPick(def.opt, k);
                let on = k == v;
                let fill = if on {
                    pal.primary
                } else if f.hovered(&act) {
                    mix(pal.alt, pal.border, 0.5)
                } else {
                    pal.alt
                };
                f.p.rrect(r.0, r.1, r.2, r.3, 4.0, Some(fill), None);
                f.focus_ring(r, 4.0, &act);
                f.hit(r, act);
            }
            let used = ch * 3.0 + gap * 2.0 + 12.0;
            used + note_lines(f, def.note, y + used, w)
        }
        Control::Password => {
            f.text(def.title, x, y, w, 24.0, st.label, SANS, pal.text);
            f.field((x, y + 28.0, w, st.control_h), Field::Opt(def.opt), app.values.text(def), "nessuna", 16.0, true);
            let top = y + 28.0 + st.control_h + 8.0;
            28.0 + st.control_h + 8.0 + note_lines(f, def.note, top, w)
        }
    }
}

/// Formato, qualità e opzioni della voce scelta, con «Altre opzioni» che si apre.
/// Restituisce l'altezza disegnata (per lo scorrimento).
pub fn options_panel(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle, with_format: bool) -> f32 {
    options_panel_ex(f, app, x, y, w, st, with_format, true)
}

/// Come `options_panel`; nella scheda PDF in cima ci sono i tre pulsanti «Cosa vuoi fare». `in_panel`
/// dice se l'ordine dei file (Unisci) e la striscia delle pagine (Dividi) stanno qui, con la scritta
/// sopra i pulsanti, o se il tema li mette nella sua coda e scrive «Cosa vuoi fare» per conto suo.
pub fn options_panel_ex(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle, with_format: bool, in_panel: bool) -> f32 {
    let pal = f.pal;
    let mut cy = y;
    let modes = with_format && pagine::shows_modes(f, app);
    if modes {
        if in_panel {
            f.text("Cosa vuoi fare", x, cy, w, 24.0, st.label, SANS, pal.text);
            cy += 28.0;
        }
        cy += pagine::modes(f, app, x, cy, w, (st.control_h * 1.85).round()) + 16.0;
        f.hline(x - 23.0, cy, w + 44.0);
        cy += 16.0;
        if in_panel {
            let h = pagine::panel_extras(f, app, x, cy, w, st);
            if h > 0.0 {
                cy += h + st.gap;
            }
        }
    }
    if with_format && (!modes || app.pdf_mode() == super::model::PdfMode::Convert) {
        f.text("Formato", x, cy, w, 24.0, st.label, SANS, pal.text);
        let title = action_title(app, f.simple);
        f.select((x, cy + 28.0, w, st.control_h), &title, None, 16.0, Act::FormatMenu);
        cy += 28.0 + st.control_h + st.gap;
    }
    if uses_quality(app.kind(), &app.action) {
        cy += quality(f, app, x, cy + 4.0, w, st, true) + st.gap;
    }
    if modes && app.pdf_mode() == super::model::PdfMode::Pages {
        let h = pagine::pages_summary(f, app, x, cy, w, st);
        if h > 0.0 {
            cy += h + 4.0;
            f.hline(x, cy, w);
            cy += 14.0;
        }
    }
    let defs = app.option_defs();
    // «Pagine» e «Comprimi» mostrano subito le loro opzioni; password e segnalibri stanno in «Altre opzioni».
    let lead = match (app.kind(), app.action.as_str()) {
        (Kind::Pdf, "pages") => defs.iter().position(|d| d.opt == conversioni::Opt::KeepBookmarks),
        (Kind::Pdf, "compress") => defs.iter().position(|d| d.opt == conversioni::Opt::Password),
        _ => None,
    };
    let shown = match lead {
        Some(n) if st.visible != 0 => n,
        _ if st.visible == 0 => defs.len(),
        _ => st.visible.min(defs.len()),
    };
    for def in &defs[..shown] {
        cy += option(f, app, def, x, cy, w, st) + st.gap;
    }
    if defs.len() > shown {
        let open = f.ui.is_open(Fold::MoreOptions);
        f.hline(x, cy - st.gap / 2.0, w);
        let r = (x, cy, w, 34.0);
        if f.hovered(&Act::Fold(Fold::MoreOptions)) {
            f.p.rrect(r.0 - 6.0, r.1, r.2 + 12.0, r.3, 6.0, Some(pal.alt), None);
        }
        f.text(&format!("Altre opzioni ({})", defs.len() - shown), x, cy, w - 40.0, 34.0, st.label + 1.0, SEMI, pal.text);
        let chev = if open { "M6 15l6-6 6 6" } else { ic::CHEVRON_DOWN };
        f.icon(chev, x + w - 30.0, cy + 5.0, 24.0, pal.text);
        f.hit(r, Act::Fold(Fold::MoreOptions));
        cy += 34.0 + 14.0;
        if open {
            for def in &defs[shown..] {
                cy += option(f, app, def, x, cy, w, st) + st.gap;
            }
        }
    } else if defs.is_empty() && !uses_quality(app.kind(), &app.action) {
        f.para("Questa voce non ha altre opzioni.", x, cy, w, st.note, SANS, pal.muted, 1.4, 2);
        cy += 24.0;
    }
    cy - y
}

/// La scelta della destinazione (con la riga che spiega dove finiscono i file).
pub fn dest_select(f: &mut Frame, app: &App, r: R, size: f32) {
    let label = match &app.dest {
        None => "Accanto all'originale".to_string(),
        Some(p) => super::model::short_dir(p),
    };
    f.select(r, &label, None, size, Act::DestMenu);
}

pub fn dest_note(app: &App) -> String {
    if app.activity == Activity::Convert && app.kind() == Kind::Pdf {
        match (app.pdf_mode(), &app.dest) {
            (super::model::PdfMode::Merge, None) => return "Il file unito verrà salvato nella cartella del primo file.".into(),
            (super::model::PdfMode::Merge, Some(p)) => return format!("Il file unito verrà salvato in {}.", p.display()),
            (super::model::PdfMode::Split, None) => return "I file nuovi verranno salvati nella stessa cartella dell'originale.".into(),
            (super::model::PdfMode::Split, Some(p)) => return format!("I file nuovi verranno salvati in {}.", p.display()),
            (super::model::PdfMode::Pages, None) => return "Il file nuovo viene salvato accanto all'originale.".into(),
            (super::model::PdfMode::Pages, Some(p)) => return format!("Il file nuovo viene salvato in {}.", p.display()),
            (super::model::PdfMode::Compress, None) => return "I file compressi vengono salvati accanto agli originali.".into(),
            (super::model::PdfMode::Compress, Some(p)) => return format!("I file compressi vengono salvati in {}.", p.display()),
            _ => {}
        }
    }
    match &app.dest {
        None => "I file convertiti verranno salvati nella stessa cartella dei file originali.".into(),
        Some(p) => format!("I file convertiti verranno salvati in {}.", p.display()),
    }
}

pub fn exists_select(f: &mut Frame, app: &App, r: R, size: f32) {
    let short = match app.values.exists {
        crate::engine::Exists::Number => "Aggiungi -2",
        crate::engine::Exists::Replace => "Sostituisci",
        crate::engine::Exists::Skip => "Salta",
        crate::engine::Exists::Ask => "Chiedi",
    };
    f.select(r, short, None, size, Act::ExistsMenu);
}

/// Il testo del pulsante principale: «Converti 3 file», «Trascrivi 2 file», «Annulla»…
pub fn go_label(app: &App) -> String {
    if let Some(job) = &app.job {
        return match job.kind {
            super::model::JobKind::Download { .. } => "Annulla".into(),
            _ => format!("{} di {}…", job.done, job.total),
        };
    }
    if app.finished() {
        let one = match pagine::go_label(app).map(|_| app.pdf_mode()) {
            Some(super::model::PdfMode::Merge) => true,
            Some(super::model::PdfMode::Split) => false,
            _ => app.targets().len() <= 1,
        };
        return if one { "Apri risultato".into() } else { "Apri cartella".into() };
    }
    let n = if matches!(app.activity, Activity::Convert | Activity::Transcribe) { app.todo().len() } else { app.targets().len() };
    match app.activity {
        Activity::Transcribe if n == 0 => "Trascrivi".into(),
        Activity::Transcribe => format!("Trascrivi {n} file"),
        Activity::YouTube | Activity::Web => "Scarica".into(),
        Activity::Convert => {
            if let Some(label) = pagine::go_label(app) {
                return label;
            }
            if n == 0 {
                "Converti".into()
            } else if n == 1 {
                "Converti 1 file".into()
            } else {
                format!("Converti {n} file")
            }
        }
    }
}

/// L'icona della scheda di una categoria (una per ogni `Kind`: aggiungere una categoria non deve fare uscire dai bordi).
pub fn category_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => ic::IMAGE,
        Kind::Doc | Kind::Pdf => ic::FILE,
        Kind::Text => ic::MENU,
        Kind::Audio => ic::AUDIO,
        Kind::Video => ic::VIDEO,
        Kind::Archive | Kind::Other => ic::FOLDER,
    }
}

/// Sotto il pulsante grande: durante il lavoro la spia (puntino + istruzione in uso, solo se c'è),
/// a lavoro finito il link «Ricomincia». `r` è il pulsante; `side` mette la riga alla sua sinistra.
pub fn go_extras(f: &mut Frame, app: &App, r: R, side: bool) {
    let pal = f.pal;
    let size = 14.0;
    if let Some(u) = app.spia() {
        let text = format!("{} · {}", if u.gpu { "GPU" } else { "CPU" }, u.detail);
        let tw = f.measure(&text, size, SANS).min(if side { 190.0 } else { r.2 - 30.0 });
        let dot = if u.gpu { pal.primary } else { pal.ok };
        let (x, y) = if side { (r.0 - 24.0 - tw - 14.0, r.1 + r.3 / 2.0 - 11.0) } else { (r.0 + (r.2 - tw - 14.0) / 2.0, r.1 + r.3 + 14.0) };
        f.p.rrect(x, y + 7.0, 8.0, 8.0, 4.0, Some(dot), None);
        f.p.text(&text, x + 14.0, y, tw + 2.0, 22.0, size, SANS, pal.muted, Align::Left);
    } else if app.finished() {
        let label = "Ricomincia";
        let tw = f.measure(label, size, SEMI);
        let (x, y) = if side { (r.0 - 24.0 - tw, r.1 + r.3 / 2.0 - 11.0) } else { (r.0 + (r.2 - tw) / 2.0, r.1 + r.3 + 14.0) };
        let hot = f.hovered(&Act::Restart);
        let c = if hot { mix(pal.primary, pal.text, 0.3) } else { pal.primary };
        f.p.text(label, x, y, tw + 2.0, 22.0, size, SEMI, c, Align::Left);
        f.p.fill(x, y + 19.0, tw, 1.0, c);
        f.hit((x - 6.0, y - 4.0, tw + 12.0, 30.0), Act::Restart);
    }
}

/// Riepilogo in parole di dove e come salva (Semplificata): veritiero, mai «consigliate» se non lo sono.
pub fn summary_line(app: &App) -> String {
    if app.kind() == Kind::Pdf && app.action == "compress" && app.activity == Activity::Convert {
        let many = app.targets().len() > 1;
        return match pagine::squeeze_total(app) {
            Some((b, a, _)) if a * 100 < b * 67 => {
                let r = format!("{:.1}", b as f64 / a.max(1) as f64).replace('.', ",");
                if many { format!("I PDF diventano circa {r} volte più piccoli e restano leggibili. Gli originali non si toccano.") } else { format!("Il PDF diventa circa {r} volte più piccolo e resta leggibile. L'originale non si tocca.") }
            }
            Some((b, a, _)) if a < b => {
                let p = 100 - a * 100 / b.max(1);
                if many { format!("I PDF diventano circa il {p}% più leggeri e restano leggibili. Gli originali non si toccano.") } else { format!("Il PDF diventa circa il {p}% più leggero e resta leggibile. L'originale non si tocca.") }
            }
            Some(_) => if many { "I PDF sono già leggeri: restano come sono.".into() } else { "Il PDF è già leggero: resta com'è.".into() },
            None => "Si calcola quanto si risparmia…".into(),
        };
    }
    let dest = match &app.dest {
        None => "Copie accanto agli originali".to_string(),
        Some(p) => format!("Copie in {}", super::model::short_dir(p)),
    };
    let opts = if app.customized() { "Impostazioni personalizzate" } else { "Impostazioni consigliate" };
    format!("{dest} · {opts}.")
}

/// «1 archivio», «2 immagini»: la categoria al singolare o al plurale secondo il numero.
pub fn count_noun(n: usize, category: &str) -> String {
    let one = match category {
        "Immagini" => "immagine",
        "Documenti" => "documento",
        "Archivi" => "archivio",
        "Testo" => "testo",
        _ => "",
    };
    if n == 1 && !one.is_empty() {
        format!("1 {one}")
    } else {
        format!("{n} {}", category.to_lowercase())
    }
}

/// I file della coda che appartengono a un'altra categoria: «Altri file in coda: 2 PDF».
pub fn others_link(f: &mut Frame, app: &App, x: f32, y: f32, h: f32, size: f32) -> f32 {
    let others = app.others();
    if others.is_empty() || app.activity != Activity::Convert {
        return 0.0;
    }
    let pal = f.pal;
    let mut cx = x;
    f.text("Altri file in coda:", cx, y, 200.0, h, size, SANS, pal.muted);
    cx += f.measure("Altri file in coda:", size, SANS) + 10.0;
    for (ci, n) in others {
        let label = count_noun(n, conversioni::CATEGORIES[ci].name);
        let w = f.measure(&label, size, SEMI) + 8.0;
        let r = (cx - 4.0, y, w, h);
        let c = if f.hovered(&Act::Category(ci)) { mix(pal.primary, pal.text, 0.3) } else { pal.primary };
        f.p.text(&label, cx, y, w, h, size, SEMI, c, Align::Left);
        f.p.fill(cx, y + h * 0.78, w - 8.0, 1.0, c);
        f.hit(r, Act::Category(ci));
        cx += w + 10.0;
    }
    cx - x
}

/// La zona vuota dove si lasciano i file (bordo tratteggiato, icona, due righe).
pub fn drop_zone(f: &mut Frame, app: &App, r: R, big: bool) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(mix(pal.alt, pal.surface, 0.4)), None);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 10.0, mix(pal.primary, pal.surface, 0.35), 2.0, 1.0, Some((9.0, 6.0)));
    let isz = if big { 44.0 } else { 32.0 };
    let cy = r.1 + r.3 / 2.0;
    f.icon(ic::DOWNLOAD, r.0 + r.2 / 2.0 - isz / 2.0, cy - isz - 10.0, isz, pal.primary);
    let cat = &conversioni::CATEGORIES[app.category];
    let (t, s) = match app.activity {
        Activity::Transcribe => ("Trascina qui audio e video", "MP3, WAV, M4A, OGG, OPUS, FLAC, WMA, AMR e video".to_string()),
        _ => ("Trascina qui i file oppure fai clic", format!("{} · Accetta: {}", cat.hint, cat.accepts)),
    };
    f.text_c(t, (r.0, cy, r.2, 30.0), if big { 22.0 } else { 18.0 }, SEMI, pal.text);
    f.text_c(&s, (r.0 + 12.0, cy + 30.0, r.2 - 24.0, 26.0), if big { 16.0 } else { 14.0 }, SANS, pal.muted);
    f.hit(r, Act::AddFiles);
}

/// La miniatura con le misure e lo stato, una riga della coda (usata dai temi chiari).
pub fn file_meta(app: &App, index: usize) -> (String, String) {
    let f = &app.files[index];
    let m = app.media_of(&f.path);
    let size = m.map(|m| super::model::human_size(m.bytes)).unwrap_or_default();
    let extra = match (m.and_then(|m| m.size), m.and_then(|m| m.pages)) {
        (Some((w, h)), _) => format!("{w} × {h}"),
        (_, Some(p)) => format!("{p} pagine"),
        _ => String::new(),
    };
    (size, extra)
}

/// La zona elastica: con la finestra stretta, il numero si accorcia ma non va sotto `min`.
pub fn flex(total: f32, fixed: f32, min: f32) -> f32 {
    (total - fixed).max(min)
}

/// Sposta le zone cliccabili aggiunte da `from` in poi e taglia quelle fuori dal riquadro visibile.
pub fn shift_hits(f: &mut Frame, from: usize, dy: f32, clip: R) {
    let mut keep = Vec::new();
    for mut h in f.hits.drain(from..) {
        h.r.1 += dy;
        let top = h.r.1.max(clip.1);
        let bottom = (h.r.1 + h.r.3).min(clip.1 + clip.3);
        if bottom > top {
            h.r.3 = bottom - top;
            h.r.1 = top;
            keep.push(h);
        }
    }
    f.hits.extend(keep);
}

/// Disegna dentro una zona che scorre: `body` riceve la y di partenza e restituisce l'altezza.
pub fn scrolled(f: &mut Frame, r: R, which: Scroll, body: impl FnOnce(&mut Frame) -> f32) {
    let off = f.ui.scroll(which);
    f.p.push_clip(r.0, r.1, r.2, r.3);
    f.p.dy -= off;
    let from = f.hits.len();
    let content = body(f);
    f.p.dy += off;
    f.p.pop_clip();
    let max = (content - r.3).max(0.0);
    shift_hits(f, from, -off, r);
    let _ = max;
    f.scrollbar(r, which, content);
}

/// Il nome del tasto «Strumenti» di un tema, se la voce è uno strumento e non un formato.
pub fn is_tool(app: &App) -> bool {
    conversioni::tools(app.kind()).iter().any(|s| s.id == app.action)
}

/// L'ordine dei file (Unisci) o la striscia delle pagine (Dividi) nella scheda PDF; 0 per il resto.
pub fn page_extras(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, st: OptStyle) -> f32 {
    if f.simple || app.activity != Activity::Convert || app.kind() != Kind::Pdf {
        return 0.0;
    }
    pagine::panel_extras(f, app, x, y, w, st)
}

/// L'icona del pulsante grande: unire e dividere hanno la loro.
pub fn go_icon(app: &App) -> &'static str {
    pagine::go_icon(app)
}

pub const BOLD_W: super::gfx::Weight = BOLD;
