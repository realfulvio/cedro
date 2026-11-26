//! Cedro 4 Labs: cornice scura, le pagine del laboratorio a sinistra, il file al centro sopra un
//! alone dei suoi stessi colori e a destra un pannello chiaro con i comandi veri del motore.
//! «Semplice» mostra solo le scelte pronte, «Completa» tutte le opzioni. Misure su 1536×1024:
//! barra e pannello restano, il centro si allarga.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use super::*;
use crate::ui::core::Scroll;
use crate::ui::gfx::{Fit, Image, Rgb};
use crate::ui::model::{FileState, Lab};
use crate::ui::theme::{LABS_FRAME, LABS_FRAME_HI, LABS_INK, LABS_LEAF, LABS_MUTED, LABS_STAGE};
use crate::ui::voci;

const TOP: f32 = 64.0;
const WHITE: Rgb = (255, 255, 255);

pub(super) const OPT_LABS: OptStyle = OptStyle { label: 15.0, note: 14.0, control_h: 42.0, gap: 20.0, visible: 0, switch_w: 52.0, switch_h: 28.0 };

const ICON_HOME: &str = "M4 11 12 4l8 7v9h-5v-6H9v6H4z";
const ICON_FIGURE: &str = "M3 4h8v7H3z M13 4h8v7h-8z M3 13h18v7H3z M14 17h5";
const ICON_TABLE: &str = "M3 5h18v14H3z M3 10h18 M9 5v14";

pub(super) fn lab_icon(lab: Lab) -> &'static str {
    match lab {
        Lab::Home => ICON_HOME,
        Lab::Immagini => ic::IMAGE,
        Lab::Anonimizza => ic::SHIELD,
        Lab::Figura => ICON_FIGURE,
        Lab::Dati => ICON_TABLE,
        Lab::Referti => ic::PEN,
        Lab::Trascrivi => ic::TRANSCRIBE,
        Lab::Converti => ic::CONVERT,
        Lab::Pdf => ic::FILE,
        Lab::Video => ic::WEB,
        Lab::Firma => ic::PEN,
    }
}

thread_local! {
    /// L'alone di ogni file già visto: la miniatura ridotta a pochi pixel.
    static HALOS: RefCell<HashMap<PathBuf, (u32, u32, Image)>> = RefCell::new(HashMap::new());
}

pub(super) fn halo(app: &App, i: usize) -> Option<Image> {
    let file = app.files.get(i)?;
    let thumb = app.media_of(&file.path)?.thumb.as_ref()?;
    HALOS.with(|h| {
        let mut h = h.borrow_mut();
        if let Some((w, hh, img)) = h.get(&file.path) {
            if (*w, *hh) == (thumb.w, thumb.h) {
                return Some(img.clone());
            }
        }
        if h.len() > 64 {
            h.clear();
        }
        let img = thumb.tiny(8, 6)?;
        h.insert(file.path.clone(), (thumb.w, thumb.h, img.clone()));
        Some(img)
    })
}

/// La pagina su cui si sta lavorando (per i colori).
pub(super) fn colors(app: &App) -> (Rgb, Rgb) {
    crate::ui::theme::lab_colors(app.lab)
}

/// La barra del titolo: marchio, caso, «Lavori», dove sta il lavoro e i comandi della finestra.
pub(super) fn title_bar(f: &mut Frame, app: &App) {
    f.p.clear(LABS_FRAME);
    f.p.rrect_vgrad(0.0, 0.0, f.w, (f.h * 0.5).round(), 0.0, LABS_FRAME_HI, LABS_FRAME);
    f.caption((0.0, 0.0, f.w, TOP));
    f.p.symbol(24.0, 17.0, 30.0);
    f.p.text("Cedro", 64.0, 17.0, 120.0, 30.0, 22.0, BOLD, LABS_INK, Align::Left);
    let cw = f.measure("Cedro", 22.0, BOLD);
    f.p.text("4 Labs", 64.0 + cw + 8.0, 17.0, 120.0, 30.0, 22.0, BOLD, LABS_LEAF, Align::Left);
    f.window_controls(f.w, 0.0, 48.0, LABS_INK, LABS_FRAME_HI);
    // Mentre lavora o scarica un componente, la riga dice cosa usa; a riposo che tutto resta sul PC.
    let _ = match super::cedro::status_line(f, app, &crate::ui::theme::TINT_MELANZANA, false, 32.0) {
        Some(x) => x,
        None => {
            // Cedro lavora tutto sul computer: lo si dice in alto, dove in laboratorio lo si cerca.
            let local = "Niente esce da questo PC";
            let lw = f.measure(local, 15.0, SEMI);
            let lx = f.w - 3.0 * 46.0 - 26.0 - lw;
            f.p.disc_a(lx - 16.0, 32.0, 9.0, LABS_LEAF, 0.22);
            f.p.circle(lx - 16.0, 32.0, 5.0, Some(LABS_LEAF), None);
            f.p.text(local, lx, 17.0, lw + 4.0, 30.0, 15.0, SEMI, LABS_LEAF, Align::Left);
            lx - 16.0 - 9.0
        }
    };
}



/// L'elenco dei lavori di oggi: cosa si è fatto, per quale caso, i nomi dei file usciti e, per ognuno,
/// il testo da incollare nel quaderno.
pub(super) fn lavori_page(f: &mut Frame, app: &App, area: R) {
    let pal = f.pal;
    f.p.rrect(area.0, area.1, area.2, area.3, 16.0, Some(pal.work), None);
    let list = crate::caso::di_oggi(&app.lavori);
    let inner = (area.0 + 32.0, area.1 + 26.0, area.2 - 64.0, area.3 - 40.0);
    f.text("Lavori di oggi", inner.0, inner.1, inner.2 - 300.0, 44.0, 32.0, BOLD, pal.text);
    // Il registro di tutti i lavori ricordati (non solo di oggi), in Excel con i filtri: per una pratica o un controllo.
    let reg = (inner.0 + inner.2 - 280.0, inner.1 + 2.0, 280.0, 42.0);
    f.primary(reg, "Registro in Excel", None, 15.5, Act::LavoriEsporta, !app.lavori.is_empty());
    let sub = if list.is_empty() { "Ancora nessuno. Ogni lavoro finisce qui, con i nomi dei file usciti." } else { "Gli originali non sono mai stati toccati." };
    f.text(sub, inner.0, inner.1 + 46.0, inner.2 - 300.0, 26.0, 16.5, SANS, pal.muted);
    let mut top = inner.1 + 88.0;
    match &app.lavori_esportato {
        Some(Ok(p)) => {
            let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let riga = format!("{} {} · {}", crate::i18n::tr("Registro salvato:"), nome, super::super::model::short_dir(p.parent().unwrap_or(p)));
            f.text(&riga, inner.0, top - 10.0, inner.2 - 140.0, 26.0, 15.0, SEMI, pal.ok);
            f.button((inner.0 + inner.2 - 120.0, top - 14.0, 120.0, 34.0), "Apri", None, 14.5, Act::OpenPath(p.clone()));
            top += 30.0;
        }
        Some(Err(e)) => {
            f.text(&format!("{} {e}", crate::i18n::tr("Registro non salvato:")), inner.0, top - 10.0, inner.2, 26.0, 15.0, SEMI, pal.bad);
            top += 30.0;
        }
        None => {}
    }
    let body = (inner.0, top, inner.2, inner.3 - (top - inner.1));
    let list: Vec<(usize, &crate::caso::Lavoro)> = list.into_iter().enumerate().collect();
    crate::ui::views::scrolled(f, body, Scroll::Lavori, |f| {
        let mut y = body.1;
        for (i, l) in &list {
            let shown = l.outputs.len().min(6);
            let rows = shown + usize::from(l.outputs.len() > shown);
            let h = 46.0 + rows as f32 * 22.0 + 16.0;
            f.p.rrect_a(body.0, y + h - 1.0, body.2, 1.0, 0.0, pal.border, 1.0);
            let head = format!("{}  ·  {}", l.ora(), crate::i18n::lit(&crate::i18n::tr_string(&l.what)));
            f.p.text(&head, body.0, y + 8.0, body.2 - 260.0, 28.0, 17.0, BOLD, pal.text, Align::Left);
            if !l.caso.is_empty() {
                let hw = f.measure(&head, 17.0, BOLD);
                f.p.text(&format!("{} {}", crate::i18n::lit(&crate::i18n::tr_string("Caso")), l.caso), body.0 + hw + 16.0, y + 8.0, (body.2 - 250.0 - hw - 16.0 - 12.0).clamp(20.0, 260.0), 28.0, 15.0, SEMI, pal.muted, Align::Left);
            }
            for (k, name) in l.outputs.iter().take(shown).enumerate() {
                f.p.text(name, body.0 + 4.0, y + 44.0 + k as f32 * 22.0, body.2 - 280.0, 22.0, 15.0, SANS, pal.text, Align::Left);
            }
            if l.outputs.len() > shown {
                let more = crate::i18n::lit(&crate::i18n::tr_string(&format!("… e altri {} file", l.outputs.len() - shown)));
                f.p.text(&more, body.0 + 4.0, y + 44.0 + shown as f32 * 22.0, body.2 - 280.0, 22.0, 15.0, SANS, pal.muted, Align::Left);
            }
            let copied = app.quaderno_copiato == Some(*i);
            let label = if copied { "Copiato" } else { "Copia per il quaderno" };
            f.button((body.0 + body.2 - 250.0, y + 6.0, 240.0, 38.0), label, None, 15.0, Act::Quaderno(*i));
            y += h + 6.0;
        }
        y - body.1
    });
}

/// La finestrella del caso: il codice (entra nei nomi dei file) e una nota libera.
pub fn caso_dialog(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.35);
    f.hit((0.0, 0.0, f.w, f.h), Act::CasoChiudi);
    let w = 560.0f32.min(f.w - 32.0);
    // La descrizione può andare su tre righe (spagnolo, tedesco…): se ne tiene lo spazio.
    let h = 348.0;
    let (x, y) = ((f.w - w) / 2.0, ((f.h - h) / 2.0).max(8.0));
    f.p.shadow(x, y, w, h, 12.0, 18.0, 0.35, 8.0);
    f.card((x, y, w, h), 12.0);
    f.hit((x, y, w, h), Act::Nothing);
    let (cx, cw) = (x + 28.0, w - 56.0);
    let mut cy = y + 24.0;
    f.text("Caso o campione", cx, cy, cw, 34.0, 24.0, SEMI, pal.text);
    cy += 40.0;
    f.para("Se c'è, il codice entra nei nomi dei file che Cedro scrive, dopo la data. Resta finché non lo togli.", cx, cy, cw, 15.0, SANS, pal.muted, 1.4, 3);
    cy += 66.0;
    f.text("Codice", cx, cy, cw, 22.0, 14.0, SEMI, pal.muted);
    cy += 24.0;
    f.field((cx, cy, cw, 42.0), Field::CasoCodice, &app.caso_bozza.0, "V-2026-0457", 16.0, false);
    cy += 54.0;
    f.text("Nota (per te, non entra nei nomi)", cx, cy, cw, 22.0, 14.0, SEMI, pal.muted);
    cy += 24.0;
    f.field((cx, cy, cw, 42.0), Field::CasoNota, &app.caso_bozza.1, "Luna, labrador", 16.0, false);
    cy += 62.0;
    let bw = 130.0;
    if app.caso.is_some() {
        f.button((cx, cy, 150.0, 46.0), "Togli il caso", None, 16.0, Act::CasoTogli);
    }
    f.button((cx + cw - 2.0 * bw - 12.0, cy, bw, 46.0), "Annulla", None, 16.0, Act::CasoChiudi);
    f.primary((cx + cw - bw, cy, bw, 46.0), "Metti", None, 16.0, Act::CasoOk, !crate::caso::pulisci(&app.caso_bozza.0).is_empty());
}

pub fn draw(f: &mut Frame, app: &App) {
    super::labsx::draw(f, app);
}


/// «Firma e manda referti» e «Trascrivi e riassumi»: le pagine di Cedro su una scheda chiara.
pub(super) fn page(f: &mut Frame, app: &App, area: R) {
    let pal = f.pal;
    f.p.rrect(area.0, area.1, area.2, area.3, 16.0, Some(pal.work), None);
    let mut r = (area.0 + 28.0, area.1 + 22.0, area.2 - 56.0, area.3 - 40.0);
    if app.lab == Lab::Referti {
        // Le altre cose che si fanno con un referto stanno in altre pagine: qui le scorciatoie.
        let mut x = r.0;
        f.text("Anche:", x, r.1, 70.0, 40.0, 15.0, SEMI, pal.muted);
        x += f.measure("Anche:", 15.0, SEMI) + 14.0;
        // Le scorciatoie per radiografie e vetrini solo se in coda c'è un DICOM o un vetrino.
        let scorciatoie = [("Radiografie per il proprietario", Lab::Immagini, "rx"), ("Pacchetto per un consulto", Lab::Immagini, "consult"), ("PDF in Word", Lab::Pdf, "docx")];
        let (dicom, vetrini) = (app.first_dicom().is_some(), app.first_slide().is_some());
        for (label, lab, id) in scorciatoie.into_iter().filter(|(_, _, id)| match *id {
            "rx" => dicom,
            "consult" => vetrini,
            _ => true,
        }) {
            let w = f.measure(label, 15.0, SEMI) + 32.0;
            f.button((x, r.1, w, 40.0), label, None, 15.0, Act::LabGo(lab, id));
            x += w + 10.0;
        }
        r = (r.0, r.1 + 56.0, r.2, r.3 - 56.0);
        firma_page(f, app, r, f.simple);
    } else {
        transcribe_page(f, app, r, f.simple);
    }
}

pub(super) fn viewer(f: &mut Frame, app: &App, i: usize, area: R) {
    let file = &app.files[i];
    if matches!(file.kind, Kind::Audio | Kind::Video | Kind::Other | Kind::Archive) {
        super::documento::media_card(f, app, i, area);
        return;
    }
    let pages = app.page_count(i).unwrap_or(1).max(1);
    let page = app.preview_page.min(pages - 1);
    let img = app.previews.get(&(file.path.clone(), page)).and_then(|o| o.clone()).or_else(|| app.media_of(&file.path).and_then(|m| m.thumb.clone()));
    let Some(img) = img else {
        // Un file che non si è potuto leggere (troncato, danneggiato) lo dice, invece di girare per sempre.
        if app.previews_failed.contains(&(file.path.clone(), page)) {
            f.p.text("Anteprima non disponibile: il file sembra danneggiato o incompleto.", area.0, area.1 + area.3 / 2.0 - 15.0, area.2, 30.0, 17.0, SANS, LABS_MUTED, Align::Center);
            return;
        }
        f.spinner(area.0 + area.2 / 2.0, area.1 + area.3 / 2.0 - 20.0, 16.0, LABS_INK);
        f.p.text("Preparo l'anteprima…", area.0, area.1 + area.3 / 2.0 + 10.0, area.2, 30.0, 17.0, SANS, LABS_MUTED, Align::Center);
        f.animating = true;
        return;
    };
    let (iw, ih) = (img.w.max(1) as f32, img.h.max(1) as f32);
    if file.kind == Kind::Image {
        // Le immagini stanno per intero nel riquadro.
        let k = (area.2 / iw).min(area.3 / ih);
        let (pw, ph) = ((iw * k).floor(), (ih * k).floor());
        let (px, py) = (area.0 + (area.2 - pw) / 2.0, area.1 + (area.3 - ph) / 2.0);
        f.p.shadow(px, py, pw, ph, 12.0, 22.0, 0.55, 10.0);
        f.p.image(&img, px, py, pw, ph, 12.0, Fit::Cover);
        f.p.rrect_line(px, py, pw, ph, 12.0, WHITE, 1.0, 0.12, None);
        return;
    }
    // Documenti, PDF e testi: la pagina larga, che scorre, con i comandi per sfogliare.
    let pw = (area.2 - 60.0).min(860.0).max(200.0);
    let ph = pw * ih / iw;
    let px = area.0 + (area.2 - pw) / 2.0;
    super::scrolled(f, area, Scroll::Page, |f| {
        let py = area.1 + 6.0;
        f.p.shadow(px, py, pw, ph, 4.0, 18.0, 0.5, 8.0);
        f.p.rrect(px, py, pw, ph, 4.0, Some(WHITE), None);
        f.p.image(&img, px, py, pw, ph, 4.0, Fit::Contain);
        ph + 30.0
    });
    if pages > 1 {
        let (bx, by) = (area.0 + 4.0, area.1 + 4.0);
        let label = format!("{} / {pages}", page + 1);
        let lw = f.measure(&label, 15.0, SEMI);
        f.p.rrect_a(bx, by, 88.0 + lw, 40.0, 20.0, LABS_STAGE, 0.82);
        for (k, (icon, act, on)) in [(ic::ARROW_LEFT, Act::PagePrev, page > 0), (ic::ARROW_RIGHT, Act::PageNext, page + 1 < pages)].into_iter().enumerate() {
            let ax = bx + 8.0 + k as f32 * (40.0 + lw);
            if on && f.hovered(&act) {
                f.p.disc_a(ax + 14.0, by + 20.0, 16.0, WHITE, 0.16);
            }
            f.icon(icon, ax + 2.0, by + 8.0, 24.0, if on { LABS_INK } else { mix(LABS_INK, LABS_STAGE, 0.6) });
            if on {
                f.hit((ax - 4.0, by, 36.0, 40.0), act);
            }
        }
        f.p.text(&label, bx + 40.0, by, lw + 8.0, 40.0, 15.0, SEMI, LABS_INK, Align::Center);
    }
}

/// Il nome del file scelto e che cos'è, in cima al pannello. Restituisce l'altezza usata.
pub(super) fn file_head(f: &mut Frame, app: &App, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let vis = app.visible();
    match app.selected.filter(|i| vis.contains(i)).or(vis.first().copied()) {
        Some(i) => {
            super::file_name(f, app, i, x, y, w, 28.0, 19.0, BOLD, pal.text);
            let file = &app.files[i];
            // Fatto, ma con scelte diverse da quelle di adesso (un ruolo, lo strumento): «Converti» lo rifà.
            let superato = file.state == FileState::Done && !app.is_fresh_done(i);
            let line = if superato {
                "Fatto con altre scelte: «Converti» lo rifà con quelle di adesso.".to_string()
            } else if file.state != FileState::Waiting {
                file.message.clone()
            } else {
                let media = app.media_of(&file.path);
                if media.is_some_and(|m| m.slide.is_some() || m.dicom.is_some() || m.micro.is_some()) {
                    super::waiting_line(app, i)
                } else {
                    let (size, extra) = super::file_meta(app, i);
                    [extra, size].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ")
                }
            };
            let c = match file.state {
                _ if superato => pal.muted,
                FileState::Failed => pal.bad,
                FileState::Done => pal.ok,
                _ => pal.muted,
            };
            28.0 + f.para(&line, x, y + 30.0, w, 14.5, SANS, c, 1.35, 3) + 8.0
        }
        None => {
            f.text(app.lab.name(), x, y, w, 28.0, 19.0, BOLD, pal.text);
            f.text("Nessun file in coda", x, y + 30.0, w, 22.0, 14.5, SANS, pal.muted);
            60.0
        }
    }
}

/// Semplice | Completa: quanti comandi vedere.
pub(super) fn mode_switch(f: &mut Frame, r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.alt), None);
    let half = (r.2 - 6.0) / 2.0;
    for (k, (label, simple)) in [("Semplice", true), ("Completa", false)].into_iter().enumerate() {
        let b = (r.0 + 3.0 + k as f32 * half, r.1 + 3.0, half, r.3 - 6.0);
        let on = f.simple == simple;
        let act = Act::SetSimple(simple);
        if on {
            f.p.shadow(b.0, b.1, b.2, b.3, 8.0, 4.0, 0.14, 1.0);
            f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(pal.surface), None);
        }
        let hot = !on && f.hovered(&act);
        f.text_c(label, b, 15.0, SEMI, if on || hot { pal.text } else { pal.muted });
        f.focus_ring(b, 8.0, &act);
        f.hit(b, act);
    }
}

/// Le scelte pronte di una pagina: sempre voci che il motore ha per quel tipo di file.
pub(super) fn choices(app: &App) -> Vec<&'static str> {
    let kind = app.kind();
    let tools = app.tools();
    let have = |id: &str| conversioni::formats(kind).iter().any(|s| s.id == id) || tools.iter().any(|s| s.id == id);
    let pick = |ids: &[&'static str]| ids.iter().copied().filter(|id| have(id)).collect::<Vec<_>>();
    match app.lab {
        Lab::Immagini => pick(&["jpeg", "tiff", "png", "pdf", "tiles", "mitosi", "consult", "rx"]),
        Lab::Anonimizza => pick(&["strip"]),
        Lab::Figura => pick(&["figure"]),
        Lab::Dati => pick(&["xlsx", "csv"]),
        _ => app.simple_choices().iter().map(|c| c.id).collect(),
    }
}

/// Una riga da scegliere: l'iconcina colorata, il nome, una riga che spiega.
pub(super) fn choice_row(f: &mut Frame, r: R, voce: &voci::Voce, label: &str, hint: &str, on: bool, act: Act, deep: Rgb) {
    let pal = f.pal;
    let t = f.hover_t(&act, !on);
    if on {
        f.p.shadow(r.0, r.1, r.2, r.3, 12.0, 6.0, 0.12, 3.0);
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(deep, pal.surface, 0.91)), Some((deep, 1.5)));
    } else {
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(pal.surface), Some((mix(pal.border, deep, 0.55 * t), 1.5)));
    }
    f.voce_tile(r.0 + 12.0, r.1 + (r.3 - 38.0) / 2.0, 38.0, voce);
    let (tx, tw) = (r.0 + 62.0, r.2 - 74.0);
    if hint.is_empty() {
        f.text(label, tx, r.1, tw, r.3, 16.5, SEMI, pal.text);
    } else {
        f.text(label, tx, r.1 + 9.0, tw, 24.0, 16.5, SEMI, pal.text);
        f.text(hint, tx, r.1 + 32.0, tw, 20.0, 14.0, SANS, pal.muted);
    }
    f.focus_ring(r, 12.0, &act);
    f.hit(r, act);
}

/// Il pulsante grande, del colore della pagina.
pub(super) fn go_button(f: &mut Frame, app: &App, r: R, deep: Rgb) {
    let pal = f.pal;
    // In Cedro 4 Labs il pulsante dice «Esegui su N file» per ogni comando (come nell'anteprima approvata).
    let label = if f.theme == crate::ui::theme::ThemeId::CedroLabs && app.job.is_none() && !app.finished() && matches!(app.activity, crate::ui::model::Activity::Convert) {
        match app.todo().len() {
            0 => crate::i18n::lit(&crate::i18n::tr_string("Esegui")),
            1 => crate::i18n::lit(&crate::i18n::tr_string("Esegui su 1 file")),
            n => format!("{} {n} {}", crate::i18n::lit(&crate::i18n::tr_string("Esegui su")), crate::i18n::lit(&crate::i18n::tr_string("file"))),
        }
    } else {
        super::go_label(app)
    };
    let enabled = !app.busy() && !app.targets().is_empty();
    let act = Act::Start;
    if enabled {
        let t = f.hover_t(&act, true);
        let press = f.press_t(&act, true);
        let b = Frame::pressed_rect(r, press);
        f.p.shadow(b.0, b.1, b.2, b.3, 13.0, 9.0 * (1.0 + 0.3 * t - 0.6 * press), 0.26, 5.0);
        f.p.rrect_vgrad(b.0, b.1, b.2, b.3, 13.0, mix(deep, WHITE, 0.16 + 0.06 * t), mix(deep, (0, 0, 0), 0.04));
        f.p.rrect_line(b.0, b.1, b.2, b.3, 13.0, WHITE, 1.0, 0.2, None);
    } else {
        f.p.rrect(r.0, r.1, r.2, r.3, 13.0, Some(mix(deep, pal.surface, 0.6)), None);
    }
    let icon = super::go_icon(app);
    let tw = f.measure(&label, 19.0, BOLD);
    let x = r.0 + (r.2 - tw - 38.0) / 2.0;
    if app.busy() {
        f.spinner(x + 12.0, r.1 + r.3 / 2.0, 10.0, WHITE);
    } else {
        let ip = f.icon_t(&act, enabled);
        f.icon_live(icon, x, r.1 + (r.3 - 26.0) / 2.0, 26.0, WHITE, ip);
    }
    f.p.text(&label, x + 38.0, r.1, tw + 4.0, r.3, 19.0, BOLD, WHITE, Align::Left);
    f.focus_ring(r, 13.0, &act);
    if enabled {
        f.hit(r, act);
    }
    if app.busy() {
        if let Some(p) = app.progress() {
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2, 5.0, 2.5, Some(pal.border), None);
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2 * p.clamp(0.0, 1.0), 5.0, 2.5, Some(deep), None);
        }
    }
}

