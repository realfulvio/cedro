//! «Firma digitale»: la pagina della sezione (trascina, apri e verifica, verifica di più file,
//! ultimi file, opzioni) e il visualizzatore dei file firmati, la finestra a parte che si apre
//! col doppio clic: il documento al centro, in alto la fascia colorata con il giudizio, a destra
//! chi ha firmato. Disegno dall'anteprima approvata (firma-digitale/anteprima-firma.html).

use super::*;
use crate::firma::{self, At, DocKind, Level, Report};
use crate::ui::core::Scroll;
use crate::ui::gfx::Rgb;

/// I colori dei tre giudizi (nei temi scuri più chiari).
pub fn level_color(f: &Frame, l: Level) -> Rgb {
    let dark = f.theme.dark();
    match l {
        Level::Ok => f.pal.ok,
        Level::Warn => {
            if dark {
                (0xF0, 0xB4, 0x4C)
            } else {
                (0xA8, 0x5A, 0x00)
            }
        }
        Level::Bad => f.pal.bad,
    }
}

fn level_mark(l: Level) -> &'static str {
    match l {
        Level::Ok => ic::CHECK,
        Level::Warn => "M12 6v8M12 18v.1",
        Level::Bad => ic::CLOSE,
    }
}

fn pill(f: &mut Frame, x_right: f32, y: f32, h: f32, text: &str, c: Rgb) -> f32 {
    let pal = f.pal;
    let w = f.measure(text, 13.5, SEMI) + 22.0;
    let x = x_right - w;
    f.p.rrect(x, y, w, h, h / 2.0, Some(mix(c, pal.surface, 0.86)), None);
    f.text_c(text, (x, y, w, h), 13.5, SEMI, c);
    w
}

/// Il foglietto colorato con la sigla del tipo di file («P7M», «PDF»), come nell'anteprima.
fn kind_chip(f: &mut Frame, x: f32, y: f32, path: &std::path::Path) {
    let l = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let (label, c): (&str, Rgb) = if l.ends_with(".pdf") {
        ("PDF", (0xD6, 0x3A, 0x2F))
    } else if l.ends_with(".tsd") {
        ("TSD", (0x0E, 0x7C, 0x86))
    } else if l.ends_with(".m7m") {
        ("M7M", (0x0E, 0x7C, 0x86))
    } else if l.ends_with(".tsr") {
        ("TSR", (0x0E, 0x7C, 0x86))
    } else if l.ends_with(".p7s") {
        ("P7S", (0x5B, 0x4B, 0xD1))
    } else if l.ends_with(".xml") || l.contains(".xml.") {
        ("XML", (0x1E, 0x8A, 0x55))
    } else if l.contains(".doc") || l.contains(".odt") {
        ("DOC", (0x2B, 0x6C, 0xD8))
    } else {
        ("P7M", (0x5B, 0x4B, 0xD1))
    };
    f.p.rrect(x, y, 32.0, 34.0, 4.0, Some(c), None);
    f.text_c(label, (x, y + 18.0, 32.0, 14.0), 8.5, BOLD, (255, 255, 255));
}

fn file_name(p: &std::path::Path) -> String {
    crate::i18n::lit(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// La pagina della sezione dentro il riquadro `r` del tema.
pub fn firma_page(f: &mut Frame, app: &App, r: R, simple: bool) {
    if !app.firma.items.is_empty() {
        batch(f, app, r);
        return;
    }
    let pal = f.pal;
    let accent = f.act_color(Activity::Firma);
    let size = if simple { 40.0 } else { 34.0 };
    f.text("Firma digitale", r.0, r.1, 420.0, 50.0, size, SEMI, pal.text);
    let nx = r.0 + f.measure("Firma digitale", size, SEMI) + 32.0;
    super::notice(f, app, (nx, r.1 + 4.0, (r.0 + r.2 - nx).min(760.0), 42.0), 15.0);
    let mut y = r.1 + 66.0;
    let bottom = r.1 + r.3;
    // In alto: trascina qui a sinistra, le due strade a destra (strette: una sotto l'altra).
    let wide = r.2 >= 900.0;
    let top_h = if wide { 320.0 } else { 230.0 };
    let (dz, cards) = if wide {
        let dw = (r.2 * 0.56).round();
        ((r.0, y, dw, top_h), (r.0 + dw + 18.0, y, r.2 - dw - 18.0, top_h))
    } else {
        ((r.0, y, r.2, top_h), (r.0, y + top_h + 16.0, r.2, 110.0))
    };
    drop_zone(f, dz, accent);
    if wide {
        let ch = (cards.3 - 16.0) / 2.0;
        big_card(f, (cards.0, cards.1, cards.2, ch), ic::SHIELD, accent, "Apri e verifica", "Apri un .p7m o un PDF firmato: vedi il documento, chi l'ha firmato e se la firma vale.", Act::FirmaAdd);
        big_card(f, (cards.0, cards.1 + ch + 16.0, cards.2, ch), ic::FOLDER, accent, "Verifica una cartella", "Controllo tutti i file firmati di una cartella, per esempio gli allegati di una PEC, e ti metto in cima quelli da guardare.", Act::FirmaAddFolder);
        y += top_h + 18.0;
    } else {
        let cw = (r.2 - 16.0) / 2.0;
        big_card(f, (r.0, cards.1, cw, cards.3), ic::SHIELD, accent, "Apri e verifica", "Apri un .p7m o un PDF firmato e vedi se la firma vale.", Act::FirmaAdd);
        big_card(f, (r.0 + cw + 16.0, cards.1, cw, cards.3), ic::FOLDER, accent, "Verifica una cartella", "Tutti i file firmati di una cartella, quelli da guardare in cima.", Act::FirmaAddFolder);
        y = cards.1 + cards.3 + 18.0;
    }
    // Le opzioni, in una riga.
    let oh = if wide { 118.0 } else { 220.0 };
    if bottom - y > oh + 120.0 {
        options(f, app, (r.0, y, r.2, oh), wide);
        y += oh + 18.0;
    }
    // Ultimi file.
    if bottom - y > 110.0 {
        recent(f, app, (r.0, y, r.2, bottom - y));
    }
}

/// «Trascina qui i file», con il bottone per sceglierli.
fn drop_zone(f: &mut Frame, r: R, c: Rgb) {
    let pal = f.pal;
    let hot = f.hovered(&Act::FirmaAdd);
    f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(c, pal.surface, if hot { 0.93 } else { 0.96 })), None);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 12.0, mix(c, pal.surface, 0.45), 1.6, 1.0, Some((8.0, 6.0)));
    f.hit(r, Act::FirmaAdd);
    let big = r.3 > 260.0;
    let tile = if big { 84.0 } else { 64.0 };
    let total = tile + 22.0 + 30.0 + 50.0 + if big { 62.0 } else { 0.0 };
    let mut y = r.1 + ((r.3 - total) / 2.0).max(12.0);
    let tx = r.0 + (r.2 - tile) / 2.0;
    f.p.rrect(tx, y, tile, tile, 18.0, Some(mix(c, pal.surface, 0.86)), None);
    f.icon(DOC_DOWN, tx + tile * 0.27, y + tile * 0.27, tile * 0.46, c);
    y += tile + 22.0;
    f.text_c("Trascina qui i file firmati", (r.0, y, r.2, 30.0), 22.0, SEMI, pal.text);
    y += 36.0;
    let w = (r.2 - 80.0).min(380.0);
    f.para("Te li apro e ti dico se le firme valgono: .p7m, .p7s, .m7m, .tsd, .tsr e i PDF firmati.", r.0 + (r.2 - w) / 2.0, y, w, 15.0, SANS, pal.muted, 1.45, 2);
    if big {
        y += 64.0;
        let bw = f.measure("Scegli i file", 15.5, SANS) + 34.0;
        f.button((r.0 + (r.2 - bw) / 2.0, y, bw, 40.0), "Scegli i file", None, 15.5, Act::FirmaAdd);
    }
}

/// Un foglio con la freccia in giù (il file che arriva).
const DOC_DOWN: &str = "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M12 11v6M9.5 14.5L12 17l2.5-2.5";

fn big_card(f: &mut Frame, r: R, icon: &str, c: Rgb, title: &str, text: &str, act: Act) {
    let pal = f.pal;
    let hot = f.hovered(&act);
    f.card(r, 12.0);
    if hot {
        f.p.rrect_line(r.0, r.1, r.2, r.3, 12.0, c, 1.5, 1.0, None);
    }
    let t = 64.0f32.min(r.3 - 30.0);
    f.p.rrect(r.0 + 22.0, r.1 + (r.3 - t) / 2.0, t, t, 14.0, Some(mix(c, pal.surface, 0.88)), None);
    f.icon(icon, r.0 + 22.0 + t * 0.25, r.1 + (r.3 - t) / 2.0 + t * 0.25, t * 0.5, c);
    let tx = r.0 + 22.0 + t + 20.0;
    let tw = r.2 - (tx - r.0) - 50.0;
    let th = f.p.wrap_height(text, 15.0, SANS, tw, 15.0 * 1.4, 3);
    let y = r.1 + (r.3 - 32.0 - th) / 2.0;
    f.text(title, tx, y - 2.0, tw, 30.0, 21.0, SEMI, pal.text);
    f.para(text, tx, y + 32.0, tw, 15.0, SANS, pal.muted, 1.4, 3);
    f.icon("M9 6l6 6-6 6", r.0 + r.2 - 38.0, r.1 + r.3 / 2.0 - 11.0, 22.0, c);
    f.focus_ring(r, 12.0, &act);
    f.hit(r, act);
}

/// Le opzioni della sezione: tre levette e l'elenco europeo dei certificatori.
fn options(f: &mut Frame, app: &App, r: R, wide: bool) {
    let pal = f.pal;
    let fu = &app.firma;
    let (assoc, pdf, online) = fu.opts;
    f.card(r, 12.0);
    let items: [(&str, &str, bool, Act); 3] = [
        ("Apri con Cedro i file firmati", "Doppio clic su .p7m, .p7s, .m7m, .tsd e .tsr", assoc, Act::FirmaSetting(0)),
        ("Anche i PDF firmati", "Se no restano al lettore PDF", pdf, Act::FirmaSetting(1)),
        ("Controlla online la revoca", "Chiedo al certificatore se è stato revocato", online, Act::FirmaSetting(2)),
    ];
    let cols = if wide { 4 } else { 2 };
    let cw = (r.2 - 24.0) / cols as f32;
    let rh = if wide { r.3 } else { r.3 / 2.0 };
    for (k, (title, note, on, act)) in items.into_iter().enumerate() {
        let cx = r.0 + 12.0 + (k % cols) as f32 * cw;
        let cy = r.1 + (k / cols) as f32 * rh;
        if k % cols > 0 {
            f.vline(cx, cy + 18.0, rh - 36.0);
        }
        let x = cx + 12.0;
        f.switch(x, cy + 22.0, 46.0, 26.0, on, act, None);
        // Su colonne strette il titolo va a capo, non si taglia.
        let th = f.p.wrap_height(title, 14.5, SEMI, cw - 82.0, 14.5 * 1.3, 2).max(20.0);
        f.para(title, x + 58.0, cy + 18.0, cw - 82.0, 14.5, SEMI, pal.text, 1.3, 2);
        f.para(note, x + 58.0, cy + 22.0 + th, cw - 82.0, 12.5, SANS, pal.muted, 1.35, if th > 24.0 { 2 } else { 3 });
    }
    let k = 3;
    let cx = r.0 + 12.0 + (k % cols) as f32 * cw;
    let cy = r.1 + (k / cols) as f32 * rh;
    if k % cols > 0 {
        f.vline(cx, cy + 18.0, rh - 36.0);
    }
    let x = cx + 12.0;
    let (updated, count) = fu.tl_info;
    let (t, c) = if fu.tl_busy || firma::trust::updating() {
        ("Lo sto scaricando…".to_string(), pal.muted)
    } else {
        match updated {
            Some(u) => (format!("Aggiornato {}, {count} certificatori", firma::relative(u)), pal.muted),
            None => ("Non ancora scaricato".into(), level_color(f, Level::Warn)),
        }
    };
    f.text("Elenco europeo dei certificatori", x, cy + 18.0, cw - 24.0, 22.0, 14.5, SEMI, pal.text);
    let lh = f.p.wrap_height(&t, 12.5, SANS, cw - 24.0, 12.5 * 1.35, 2).max(17.0);
    f.para(&t, x, cy + 42.0, cw - 24.0, 12.5, SANS, c, 1.35, 2);
    let bw = f.measure("Aggiorna ora", 13.5, SANS) + 46.0;
    f.button((x, cy + 48.0 + lh, bw, 32.0), "Aggiorna ora", Some(ic::REFRESH), 13.5, Act::FirmaTrust);
}

/// «Ultimi file»: i file firmati aperti di recente, con il loro giudizio.
fn recent(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let fu = &app.firma;
    let row = 61.0;
    let fit = (((r.3 - 52.0) / row).floor().max(1.0) as usize).min(fu.recent.len().max(1));
    let h = 52.0 + fit as f32 * row;
    let rc = (r.0, r.1, r.2, h.min(r.3));
    f.card(rc, 12.0);
    f.text("Ultimi file", rc.0 + 18.0, rc.1 + 10.0, rc.2 - 200.0, 32.0, 17.0, SEMI, pal.text);
    if !fu.recent.is_empty() {
        f.text_r("clic per riaprire", (rc.0, rc.1 + 10.0, rc.2 - 18.0, 32.0), 13.0, SANS, pal.muted);
    }
    f.hline(rc.0, rc.1 + 50.0, rc.2);
    if fu.recent.is_empty() {
        f.text("Qui ritrovi i file firmati che apri, con il loro giudizio.", rc.0 + 18.0, rc.1 + 52.0, rc.2 - 36.0, row, 14.5, SANS, pal.muted);
        return;
    }
    for (k, rec) in fu.recent.iter().take(fit).enumerate() {
        let y = rc.1 + 51.0 + k as f32 * row;
        let rr = (rc.0 + 1.0, y, rc.2 - 2.0, row);
        let act = Act::FirmaRecent(k);
        if f.hovered(&act) {
            f.p.fill(rr.0, rr.1, rr.2, rr.3 - 1.0, pal.alt);
        }
        if k > 0 {
            f.hline(rc.0, y, rc.2);
        }
        kind_chip(f, rr.0 + 18.0, y + 13.0, &rec.path);
        let c = level_color(f, rec.level);
        let word = match rec.level {
            Level::Ok => "Firma valida",
            Level::Warn => "Da guardare",
            Level::Bad => "Non valida",
        };
        let when = firma::relative(rec.when).replace("oggi alle ", "oggi ").replace("ieri alle ", "ieri ").replace("il ", "");
        let ww = f.measure(&when, 13.5, SANS);
        f.text_r(&when, (rr.0, y, rr.2 - 18.0, row), 13.5, SANS, pal.muted);
        let pw = pill(f, rr.0 + rr.2 - 18.0 - ww - 16.0, y + (row - 24.0) / 2.0, 24.0, word, c);
        let tx = rr.0 + 68.0;
        let tw = rr.2 - (tx - rr.0) - ww - pw - 60.0;
        f.text(&file_name(&rec.path), tx, y + 9.0, tw, 22.0, 14.5, SEMI, pal.text);
        f.text(&rec.headline, tx, y + 31.0, tw, 20.0, 12.5, SANS, pal.muted);
        f.hit(rr, act);
    }
}

/// «Verifica di più file»: i file controllati, prima quelli da guardare.
fn batch(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let fu = &app.firma;
    let n = fu.items.len();
    // Torna alla pagina della sezione (e svuota l'elenco).
    let back = Act::FirmaClear;
    let bw = f.measure("Firma digitale", 14.5, SANS) + 30.0;
    if f.hovered(&back) {
        f.p.rrect(r.0 - 6.0, r.1, bw + 8.0, 30.0, 6.0, Some(pal.alt), None);
    }
    f.icon("M15 6l-6 6 6 6", r.0 - 4.0, r.1 + 6.0, 18.0, pal.text);
    f.text("Firma digitale", r.0 + 18.0, r.1, bw, 30.0, 14.5, SANS, pal.text);
    f.hit((r.0 - 6.0, r.1, bw + 8.0, 30.0), back);
    f.text("Verifica di più file", r.0, r.1 + 34.0, r.2 - 360.0, 50.0, 34.0, SEMI, pal.text);
    let busy = fu.items.iter().filter(|i| i.report.is_none()).count();
    let bad = fu.items.iter().filter(|i| i.report.as_ref().is_some_and(|r| r.level != Level::Ok)).count();
    let folder = fu.items.first().and_then(|i| i.path.parent()).and_then(|p| p.file_name()).map(|n| crate::i18n::lit(&n.to_string_lossy())).unwrap_or_default();
    let right = if folder.is_empty() { format!("{n} file") } else { format!("{folder} · {n} file") };
    f.text_r(&right, (r.0, r.1 + 34.0, r.2, 50.0), 15.0, SANS, pal.muted);
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 36.0), 14.0);
    let sub = if busy > 0 {
        format!("Ne sto controllando {busy}. Quelli da guardare vanno in cima; clic su un file per aprirlo nel visualizzatore.")
    } else if bad == 0 {
        "Tutte le firme sono a posto. Clic su un file per aprirlo nel visualizzatore.".to_string()
    } else {
        let t = if bad == 1 { "1 file da guardare, in cima".to_string() } else { format!("{bad} file da guardare, in cima") };
        format!("{t}. Clic su un file per aprirlo nel visualizzatore.")
    };
    f.text(&sub, r.0, r.1 + 90.0, r.2, 26.0, 15.0, SANS, pal.muted);
    let row = 61.0;
    let top = r.1 + 126.0;
    let max_h = r.1 + r.3 - top - 70.0;
    let list = (r.0, top, r.2, (n as f32 * row + 2.0).min(max_h));
    f.card(list, 12.0);
    let inner = (list.0 + 1.0, list.1 + 1.0, list.2 - 2.0, list.3 - 2.0);
    let order = fu.sorted();
    super::scrolled(f, inner, Scroll::Firma, |f| {
        for (k, &i) in order.iter().enumerate() {
            let y = inner.1 + k as f32 * row;
            if !f.p.shows_y(y, row) {
                continue;
            }
            let it = &fu.items[i];
            let rr = (inner.0, y, inner.2, row);
            let act = Act::FirmaOpen(i);
            if f.hovered(&act) {
                f.p.fill(rr.0, rr.1, rr.2, rr.3, pal.alt);
            }
            if k > 0 {
                f.hline(rr.0, y, rr.2);
            }
            kind_chip(f, rr.0 + 18.0, y + 13.0, &it.path);
            let tx = rr.0 + 68.0;
            let right = 250.0;
            f.text(&file_name(&it.path), tx, y + 9.0, rr.2 - (tx - rr.0) - right, 22.0, 14.5, SEMI, pal.text);
            let (sub, level, word) = match &it.report {
                None => ("Controllo le firme…".to_string(), None, ""),
                Some(rep) => (
                    row_line(rep),
                    Some(rep.level),
                    match rep.level {
                        Level::Ok => "Valida",
                        Level::Warn => "Da guardare",
                        Level::Bad if rep.signers.is_empty() => "Non firmato",
                        Level::Bad => "Non valida",
                    },
                ),
            };
            f.text(&crate::i18n::lit(&sub), tx, y + 31.0, rr.2 - (tx - rr.0) - right, 20.0, 12.5, SANS, pal.muted);
            match level {
                Some(l) => {
                    let c = if it.report.as_ref().is_some_and(|r| r.unsigned()) { pal.muted } else { level_color(f, l) };
                    let when = it.report.as_ref().and_then(|r| r.signers.iter().filter_map(|s| s.when).min()).map(|t| {
                        let l = firma::local(t);
                        format!("firmato {:02}/{:02}", l.d, l.mo)
                    });
                    let ww = when.as_ref().map(|w| f.measure(w, 13.5, SANS) + 16.0).unwrap_or(0.0);
                    if let Some(w) = &when {
                        f.text_r(w, (rr.0, y, rr.2 - 18.0, row), 13.5, SANS, pal.muted);
                    }
                    pill(f, rr.0 + rr.2 - 18.0 - ww, y + (row - 24.0) / 2.0, 24.0, word, c);
                }
                None => {
                    f.spinner(rr.0 + rr.2 - 40.0, y + row / 2.0, 9.0, pal.primary);
                    f.animating = true;
                }
            }
            f.hit(rr, act);
        }
        order.len() as f32 * row
    });
    let by = list.1 + list.3 + 22.0;
    let ready = fu.items.iter().any(|i| i.report.is_some());
    let mut bx = r.0;
    for (label, icon, act, on) in [
        ("Un rapporto unico in PDF", ic::FILE, Act::FirmaReport, ready),
        ("Estrai tutti i documenti", ic::DOWNLOAD, Act::FirmaExtract, ready),
        ("Aggiungi file", ic::ADD, Act::FirmaAdd, true),
    ] {
        let w = f.measure(label, 15.5, SANS) + 58.0;
        if on {
            f.button((bx, by, w, 42.0), label, Some(icon), 15.5, act);
        } else {
            f.p.rrect(bx, by, w, 42.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            f.text_c(label, (bx, by, w, 42.0), 15.5, SANS, pal.muted);
        }
        bx += w + 12.0;
    }
}

/// La seconda riga di un file controllato: chi ha firmato, o cosa non va.
fn row_line(r: &Report) -> String {
    if r.signers.is_empty() {
        return r.detail.clone();
    }
    let names: Vec<String> = r.signers.iter().map(|s| s.name.clone()).collect();
    match r.level {
        Level::Ok => {
            let ts = if r.signers.iter().all(|s| s.when_certain) { " · con marca temporale" } else { "" };
            if names.len() > 2 {
                format!("{} firme: {}…{ts}", names.len(), names[..2].join(", "))
            } else {
                format!("{}{ts}", names.join(", "))
            }
        }
        _ => {
            let worst = r.signers.iter().find(|s| s.level == r.level);
            let what = worst.and_then(|s| s.checks.iter().find(|c| c.counts && c.level == r.level)).map(|c| match c.what {
                "Documento" => c.text.clone(),
                "Revoca" if c.text.starts_with("revocato") => format!("certificato {}", c.text),
                "Revoca" => format!("revoca {}", c.text),
                "Certificato" if c.text.starts_with("certificat") => c.text.clone(),
                "Certificato" => format!("certificato {}", c.text),
                _ => format!("{}: {}", c.what.to_lowercase(), c.text),
            });
            match (worst, what) {
                (Some(s), Some(why)) if r.signers.len() > 1 => {
                    let verb = if r.level == Level::Bad { "non vale" } else { "da guardare" };
                    format!("{} firme: quella di {} {verb} ({why})", r.signers.len(), s.name)
                }
                (Some(s), Some(why)) => format!("{} · {why}", s.name),
                _ => r.detail.clone(),
            }
        }
    }
}

// ---------------------------------------------------------------- visualizzatore

const BAR: f32 = 48.0;
const TOOLS: f32 = 58.0;
const PANEL_W: f32 = 430.0;
const FOOT: f32 = 34.0;

pub fn viewer_size() -> (f32, f32) {
    (1440.0, 960.0)
}

pub fn draw_viewer(f: &mut Frame, app: &App) {
    let pal = f.pal;
    f.p.clear(pal.bg);
    let Some(v) = app.firma.viewer.as_ref() else { return };
    let name = file_name(&v.path);
    // Barra del titolo.
    f.caption((0.0, 0.0, f.w, BAR));
    f.brand(16.0, 12.0, 24.0, 0.0, 10.0, None);
    let tx = if f.theme == ThemeId::Bondeno { 86.0 } else { 52.0 };
    f.text(&format!("{name} · Cedro"), tx, 0.0, f.w - tx - 160.0, BAR, 15.5, SEMI, pal.text);
    f.window_controls(f.w, 0.0, BAR, pal.text, pal.alt);
    f.hline(0.0, BAR - 1.0, f.w);
    let rep = v.report.clone();
    // Strumenti.
    let ty = BAR;
    f.p.fill(0.0, ty, f.w, TOOLS, pal.surface);
    f.hline(0.0, ty + TOOLS - 1.0, f.w);
    let has_doc = rep.as_ref().is_some_and(|r| !r.doc.is_empty());
    let doc_name = rep.as_ref().map(|r| r.doc_name.clone()).unwrap_or_default();
    let ext = std::path::Path::new(&doc_name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut x = 14.0;
    // Un PDF firmato è già il documento: si salva solo la versione firmata, quando la si guarda.
    let own_pdf = v.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let unsigned_doc = rep.as_ref().is_some_and(|r| r.unsigned());
    let save = if unsigned_doc {
        // Non firmato: il documento è il file stesso, non c'è niente da estrarre.
        None
    } else if own_pdf {
        v.revision.is_some().then(|| "Salva la versione firmata".to_string())
    } else if ext.is_empty() {
        Some("Salva il documento".to_string())
    } else {
        Some(format!("Salva il .{ext}"))
    };
    if let Some(save) = save {
        let w = f.measure(&save, 15.0, SEMI) + 56.0;
        f.primary((x, ty + 9.0, w, 40.0), &save, Some(ic::DOWNLOAD), 15.0, Act::VwSave, has_doc);
        x += w + 10.0;
    }
    if has_doc {
        x += f.tool(x, ty + 9.0, 40.0, "Apri fuori da Cedro", ic::OPEN_OUT, 15.0, Act::VwOpenWith, true) + 8.0;
        x += f.tool(x, ty + 9.0, 40.0, "Stampa", ic::PRINT, 15.0, Act::VwPrint, true) + 8.0;
    }
    let pageable = rep.as_ref().is_some_and(|r| matches!(r.doc_kind, Some(DocKind::Pdf | DocKind::Image | DocKind::Word | DocKind::Office | DocKind::Text | DocKind::Xml)));
    if pageable {
        let zx = x + 12.0;
        f.vline(zx - 10.0, ty + 14.0, 30.0);
        f.button((zx, ty + 9.0, 40.0, 40.0), "", Some(ic::ZOOM_OUT), 15.0, Act::VwZoom(-1));
        f.text_c(&format!("{}%", (v.zoom * 100.0).round()), (zx + 42.0, ty, 64.0, TOOLS), 14.5, SEMI, pal.text);
        f.button((zx + 108.0, ty + 9.0, 40.0, 40.0), "", Some(ic::ZOOM_IN), 15.0, Act::VwZoom(1));
    }
    if !unsigned_doc {
        let mut rx = f.w - 14.0;
        let pw = f.measure("Firme", 15.0, SANS) + 56.0;
        rx -= pw;
        let pact = Act::VwPanel;
        if v.panel {
            f.p.rrect(rx, ty + 9.0, pw, 40.0, 8.0, Some(pal.alt), Some((pal.border, 1.0)));
            f.icon(ic::PANEL, rx + 12.0, ty + 17.0, 24.0, pal.primary);
            f.text("Firme", rx + 42.0, ty + 9.0, pw - 46.0, 40.0, 15.0, SANS, pal.primary);
            f.hit((rx, ty + 9.0, pw, 40.0), pact);
        } else {
            f.button((rx, ty + 9.0, pw, 40.0), "Firme", Some(ic::PANEL), 15.0, pact);
        }
        let rw = f.measure("Rapporto di verifica", 15.0, SANS) + 56.0;
        rx -= rw + 10.0;
        f.button((rx, ty + 9.0, rw, 40.0), "Rapporto di verifica", Some(ic::FILE), 15.0, Act::VwReport);
    }
    // La fascia.
    let by = ty + TOOLS;
    let tw_band = f.w - 80.0 - if rep.as_ref().is_some_and(|r| !r.signers.is_empty()) && !v.panel { 220.0 } else { 20.0 };
    let detail_h = f.p.wrap_height(&rep.as_ref().map(|r| r.detail.clone()).unwrap_or_default(), 14.5, SANS, tw_band, 14.5 * 1.4, 4).max(20.0);
    let band_h = (48.0 + detail_h).max(76.0);
    let (lvl, head, detail) = match &rep {
        None => (None, "Controllo le firme…".to_string(), "Un momento: leggo le buste, i certificati e l'elenco europeo dei certificatori.".to_string()),
        Some(r) => (Some(r.level), r.headline.clone(), r.detail.clone()),
    };
    // Un file senza firme non è un errore: la fascia resta neutra.
    let unsigned = rep.as_ref().is_some_and(|r| r.unsigned());
    let bc = if unsigned { pal.muted } else { lvl.map(|l| level_color(f, l)).unwrap_or(pal.primary) };
    f.p.fill(0.0, by, f.w, band_h, mix(bc, pal.surface, if f.theme.dark() { 0.78 } else { 0.9 }));
    f.p.fill(0.0, by + band_h - 3.0, f.w, 3.0, bc);
    match lvl {
        Some(l) => {
            f.p.circle(38.0, by + band_h / 2.0, 17.0, Some(bc), None);
            let mark = if unsigned { ic::FILE } else { level_mark(l) };
            f.icon(mark, 27.0, by + band_h / 2.0 - 11.0, 22.0, (255, 255, 255));
        }
        None => f.spinner(38.0, by + band_h / 2.0, 14.0, bc),
    }
    let show_btn = rep.as_ref().is_some_and(|r| !r.signers.is_empty()) && !v.panel;
    let tw = f.w - 80.0 - if show_btn { 220.0 } else { 20.0 };
    let hc = if lvl.is_some() && !unsigned { bc } else { pal.text };
    f.text(&head, 70.0, by + 10.0, tw, 28.0, 19.0, BOLD, hc);
    f.para(&detail, 70.0, by + 40.0, tw, 14.5, SANS, pal.text, 1.4, 4);
    if show_btn {
        let n = rep.as_ref().map(|r| r.signers.len()).unwrap_or(0);
        let label = if n > 1 { format!("Mostra le {n} firme") } else { "Mostra la firma".into() };
        let w = f.measure(&label, 15.0, SANS) + 32.0;
        f.button((f.w - w - 20.0, by + 18.0, w, 40.0), &label, None, 15.0, Act::VwPanel);
    }
    if v.checking && rep.is_some() {
        f.spinner(f.w - if show_btn { 260.0 } else { 30.0 }, by + band_h / 2.0, 9.0, bc);
        f.animating = true;
    }
    // Corpo.
    let body_y = by + band_h;
    let body_h = f.h - body_y - FOOT;
    let panel_w = if v.panel && rep.is_some() && !unsigned { PANEL_W.min(f.w * 0.45) } else { 0.0 };
    let pages = v.doc_path.as_ref().and_then(|p| app.media_of(p)).and_then(|m| m.pages).unwrap_or(1);
    let thumbs_w = if pageable && pages > 1 { 92.0 } else { 0.0 };
    f.p.fill(0.0, body_y, f.w, body_h, mix(pal.bg, pal.border, 0.35));
    if thumbs_w > 0.0 {
        thumbs(f, app, (0.0, body_y, thumbs_w, body_h), pages);
    }
    let stage = (thumbs_w, body_y, f.w - thumbs_w - panel_w, body_h);
    stage_draw(f, app, stage);
    if panel_w > 0.0 {
        let pr = (f.w - panel_w, body_y, panel_w, body_h);
        f.p.fill(pr.0, pr.1, pr.2, pr.3, pal.surface);
        f.vline(pr.0, pr.1, pr.3);
        if let Some(r) = &rep {
            panel(f, app, r, pr);
        }
    }
    // Piede.
    let fy = f.h - FOOT;
    f.p.fill(0.0, fy, f.w, FOOT, pal.surface);
    f.hline(0.0, fy, f.w);
    let foot = match &rep {
        // Un file senza firme: elenco UE e revoca non c'entrano.
        Some(r) if r.unsigned() => {
            let mut s = crate::i18n::lit(&r.doc_name).to_string();
            if pageable && pages > 1 {
                s.push_str(&format!(", {pages} pagine"));
            }
            s.push_str(&format!(", {}", super::super::model::human_size(r.doc.len() as u64)));
            s
        }
        Some(r) => {
            let mut s = String::new();
            if !r.doc.is_empty() {
                s.push_str(&format!("Documento dentro: {}", crate::i18n::lit(&r.doc_name)));
                if pageable && pages > 1 {
                    s.push_str(&format!(", {pages} pagine"));
                }
                s.push_str(&format!(", {}", super::super::model::human_size(r.doc.len() as u64)));
                s.push_str(" · ");
            }
            s.push_str(&format!("Controllato {}", firma::relative(r.checked_at)));
            match r.trust_updated {
                Some(t) => s.push_str(&format!(" · elenco UE aggiornato {}", firma::relative(t).replace(" alle ", ", "))),
                None => s.push_str(" · elenco UE non ancora scaricato"),
            }
            if !r.revoca_online() {
                s.push_str(" · revoca non controllata online");
            }
            s
        }
        None => String::new(),
    };
    f.text(&foot, 16.0, fy, f.w - 32.0, FOOT, 13.0, SANS, pal.muted);
    super::notice(f, app, (f.w / 2.0 - 300.0, body_y + 16.0, 600.0, 44.0), 15.0);
}

fn shown_path(app: &App) -> Option<std::path::PathBuf> {
    let v = app.firma.viewer.as_ref()?;
    match &v.revision {
        Some((_, p)) => Some(p.clone()),
        None => v.doc_path.clone(),
    }
}

/// La striscia delle pagine a sinistra.
fn thumbs(f: &mut Frame, app: &App, r: R, pages: usize) {
    let pal = f.pal;
    let Some(v) = app.firma.viewer.as_ref() else { return };
    let Some(path) = shown_path(app) else { return };
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.vline(r.0 + r.2 - 1.0, r.1, r.3);
    let tw = r.2 - 28.0;
    let th = tw * 1.414;
    let row = th + 34.0;
    super::scrolled(f, r, Scroll::FirmaThumbs, |f| {
        for i in 0..pages {
            let y = r.1 + 12.0 + i as f32 * row;
            if !f.p.shows_y(y, row) {
                continue;
            }
            let on = i == v.page;
            let tr = (r.0 + 14.0, y, tw, th);
            f.p.rrect(tr.0, tr.1, tr.2, tr.3, 3.0, Some((255, 255, 255)), Some((if on { pal.primary } else { pal.border }, if on { 2.5 } else { 1.0 })));
            if let Some(Some(img)) = app.page_thumbs.get(&(path.clone(), i)) {
                f.p.image(img, tr.0 + 1.0, tr.1 + 1.0, tr.2 - 2.0, tr.3 - 2.0, 2.0, crate::ui::gfx::Fit::Contain);
            }
            // La spunta sulle pagine con una firma visibile.
            if let Some(rep) = &v.report {
                if let Some(s) = rep.signers.iter().find(|s| s.place.as_ref().is_some_and(|p| p.visible && p.page == Some(i as u32 + 1))) {
                    let c = level_color(f, s.level);
                    f.p.circle(tr.0 + tr.2 - 6.0, tr.1 + tr.3 - 6.0, 9.0, Some(c), None);
                    f.icon(level_mark(s.level), tr.0 + tr.2 - 12.0, tr.1 + tr.3 - 12.0, 12.0, (255, 255, 255));
                }
            }
            f.text_c(&(i + 1).to_string(), (r.0, y + th + 4.0, r.2, 22.0), 13.0, if on { SEMI } else { SANS }, if on { pal.primary } else { pal.muted });
            f.hit((r.0, y, r.2, row), Act::VwPage(i));
        }
        12.0 + pages as f32 * row
    });
}

/// Il documento al centro.
fn stage_draw(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let Some(v) = app.firma.viewer.as_ref() else { return };
    let Some(rep) = v.report.as_ref() else {
        f.spinner(r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, 18.0, pal.primary);
        f.animating = true;
        return;
    };
    let centered = |f: &mut Frame, icon: &str, title: &str, text: &str, button: Option<(&str, Act)>| {
        let w = (r.2 - 80.0).min(560.0);
        let x = r.0 + (r.2 - w) / 2.0;
        let y = r.1 + r.3 * 0.28;
        f.icon(icon, x + w / 2.0 - 22.0, y, 44.0, pal.muted);
        f.text_c(title, (x, y + 60.0, w, 30.0), 19.0, SEMI, pal.text);
        let h = f.para(text, x, y + 98.0, w, 15.0, SANS, pal.muted, 1.45, 6);
        if let Some((label, act)) = button {
            let bw = f.measure(label, 15.0, SEMI) + 48.0;
            f.primary((x + (w - bw) / 2.0, y + 120.0 + h, bw, 42.0), label, None, 15.0, act, true);
        }
    };
    if rep.needs_original {
        centered(f, ic::FILE, "Manca il file originale", "È una firma staccata (.p7s) o una marca a parte (.tsr): la controllo insieme al file che è stato firmato. Di solito sta nella stessa cartella con lo stesso nome.", Some(("Scegli il file originale", Act::VwOriginal)));
        return;
    }
    if rep.doc.is_empty() {
        let t = if rep.error.is_some() { "Non c'è un documento da mostrare" } else { "Il file è vuoto" };
        centered(f, ic::WARNING, t, &rep.detail, None);
        return;
    }
    let kind = rep.doc_kind.unwrap_or(DocKind::Other);
    let Some(path) = shown_path(app) else { return };
    match kind {
        DocKind::Other | DocKind::None => {
            let text = format!(
                "Dentro la busta c'è «{}» ({}). La firma è controllata lo stesso: per vederlo salvalo o aprilo con il programma che hai.",
                crate::i18n::lit(&rep.doc_name),
                super::super::model::human_size(rep.doc.len() as u64)
            );
            centered(f, ic::FILE, "Questo tipo di file non lo so mostrare", &text, Some(("Apri fuori da Cedro", Act::VwOpenWith)));
        }
        _ => {
            // Le pagine una sotto l'altra, la scelta in cima; con lo zoom si allarga.
            let pad = 28.0;
            let mut pw = ((r.2 - 2.0 * pad).min(860.0)) * v.zoom;
            if pw > r.2 - 2.0 * pad && v.zoom <= 1.0 {
                pw = r.2 - 2.0 * pad;
            }
            let img = app.previews.get(&(path.clone(), v.page)).and_then(|i| i.as_ref());
            let ratio = img.map(|i| i.h as f32 / i.w.max(1) as f32).unwrap_or(1.414);
            let ph = pw * ratio;
            let px = r.0 + ((r.2 - pw) / 2.0).max(pad);
            f.p.push_clip(r.0, r.1, r.2, r.3);
            let off = f.ui.scroll(Scroll::FirmaDoc);
            let py = r.1 + pad - off;
            f.p.shadow(px, py, pw, ph, 2.0, 12.0, 0.18, 4.0);
            f.p.fill(px, py, pw, ph, (255, 255, 255));
            match img {
                Some(i) => f.p.image(i, px, py, pw, ph, 0.0, crate::ui::gfx::Fit::Contain),
                None => {
                    f.spinner(px + pw / 2.0, py + ph.min(r.3) / 2.0, 16.0, pal.primary);
                    f.animating = true;
                }
            }
            // Il riquadro delle firme aperte che stanno su questa pagina, nel colore del giudizio.
            if img.is_some() {
                for (i, sg) in rep.signers.iter().enumerate() {
                    let open = v.open.contains(&i) || rep.signers.len() == 1;
                    let Some(pl) = sg.place.as_ref().filter(|p| open && p.visible && p.page == Some(v.page as u32 + 1)) else { continue };
                    let Some(rc) = pl.rect else { continue };
                    let c = level_color(f, sg.level);
                    let (x0, y0) = (px + rc[0] * pw - 4.0, py + rc[1] * ph - 4.0);
                    let (bw, bh) = ((rc[2] - rc[0]) * pw + 8.0, (rc[3] - rc[1]) * ph + 8.0);
                    f.p.rrect(x0, y0, bw, bh, 6.0, None, Some((c, 2.5)));
                }
            }
            // Sopra la versione firmata, la scritta.
            if let Some((i, _)) = &v.revision {
                let who = rep.signers.get(*i).map(|s| firma::title_case(&s.name)).unwrap_or_default();
                let t = format!("Versione firmata da {who}: com'era al momento della firma");
                let w = f.measure(&t, 14.0, SEMI) + 150.0;
                let bx = r.0 + (r.2 - w) / 2.0;
                f.p.rrect(bx, r.1 + 8.0, w, 36.0, 18.0, Some(pal.text), None);
                f.text(&t, bx + 16.0, r.1 + 8.0, w - 140.0, 36.0, 14.0, SEMI, pal.surface);
                f.p.pop_clip();
                f.p.push_clip(r.0, r.1, r.2, r.3);
                let ba = Act::VwRevisionOff;
                f.text("Torna al file", bx + w - 120.0, r.1 + 8.0, 110.0, 36.0, 14.0, BOLD, mix(pal.surface, pal.primary, 0.3));
                f.hit((bx + w - 126.0, r.1 + 8.0, 120.0, 36.0), ba);
            }
            f.p.pop_clip();
            f.scrollbar(r, Scroll::FirmaDoc, ph + 2.0 * pad);
        }
    }
}

fn avatar(f: &mut Frame, cx: f32, cy: f32, rad: f32, initials: &str, l: Level) {
    let pal = f.pal;
    let c = level_color(f, l);
    f.p.circle(cx, cy, rad, Some(mix(c, pal.surface, 0.82)), None);
    f.text_c(initials, (cx - rad, cy - rad, rad * 2.0, rad * 2.0), rad * 0.72, BOLD, mix(c, pal.text, 0.35));
    let mr = rad * 0.42;
    let (mx, my) = (cx + rad * 0.72, cy + rad * 0.72);
    f.p.circle(mx, my, mr + 2.0, Some(pal.surface), None);
    f.p.circle(mx, my, mr, Some(c), None);
    f.icon(level_mark(l), mx - mr * 0.75, my - mr * 0.75, mr * 1.5, (255, 255, 255));
}

/// Il pannello a destra: chi ha firmato, com'è fatto il file, verifica alla data.
fn panel(f: &mut Frame, app: &App, rep: &Report, r: R) {
    let pal = f.pal;
    let Some(v) = app.firma.viewer.as_ref() else { return };
    let x = r.0 + 20.0;
    let w = r.2 - 40.0;
    super::scrolled(f, (r.0 + 1.0, r.1, r.2 - 1.0, r.3), Scroll::FirmaPanel, |f| {
        let mut y = r.1 + 16.0;
        let title = if rep.signers.len() > 1 { "Chi ha firmato" } else { "Firma" };
        if !rep.signers.is_empty() {
            f.text(title, x, y, w - 150.0, 28.0, 18.0, SEMI, pal.text);
            f.text_r(&format!("controllato {}", firma::relative(rep.checked_at).replace("oggi alle", "oggi,")), (x, y, w, 28.0), 13.0, SANS, pal.muted);
            y += 40.0;
        }
        for i in rep.ordered() {
            let s = &rep.signers[i];
            let open = v.open.contains(&i) || rep.signers.len() == 1;
            let indent = if s.counter_of.is_some() { 22.0 } else { 0.0 };
            let (sx, sw) = (x + indent, w - indent);
            let head = (sx - 8.0, y, sw + 16.0, 78.0);
            let act = Act::VwSigner(i);
            let tx = sx + 60.0;
            f.text(&crate::i18n::lit(&s.name), tx, y + 6.0, sw - 90.0, 24.0, 16.0, BOLD, pal.text);
            let when = s.when.map(|t| format!("{}, {}", firma::long_date(t), firma::hm(t))).unwrap_or_default();
            let mut sent = [when, s.role.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            if let Some(p) = s.place.as_ref().filter(|p| p.visible) {
                if let Some(n) = p.page.filter(|_| !sent.contains("pagina")) {
                    sent.push_str(&format!(" · firma visibile a pagina {n}"));
                }
            }
            let sent = crate::i18n::lit(&sent);
            let mh = f.p.wrap_height(&sent, 13.0, SANS, sw - 90.0, 13.0 * 1.4, 3).max(18.0);
            let head = (head.0, head.1, head.2, 58.0 + mh);
            if f.hovered(&act) {
                f.p.rrect(head.0, head.1, head.2, head.3, 10.0, Some(pal.alt), None);
            }
            avatar(f, sx + 24.0, y + head.3 / 2.0, 22.0, &s.initials, s.level);
            f.text(&crate::i18n::lit(&s.name), tx, y + 6.0, sw - 90.0, 24.0, 16.0, BOLD, pal.text);
            f.para(&sent, tx, y + 31.0, sw - 90.0, 13.0, SANS, pal.muted, 1.4, 3);
            f.text(&s.summary, tx, y + 34.0 + mh, sw - 90.0, 22.0, 14.5, SEMI, level_color(f, s.level));
            f.icon(if open { "M6 15l6-6 6 6" } else { ic::CHEVRON_DOWN }, sx + sw - 26.0, y + head.3 / 2.0 - 11.0, 22.0, pal.muted);
            f.hit(head, act);
            y += head.3 + 6.0;
            if open {
                y = signer_body(f, app, rep, i, sx, y, sw);
            }
            f.hline(x, y, w);
            y += 12.0;
        }
        // Le marche sul documento.
        if !rep.timestamps.is_empty() {
            f.text(if rep.timestamps.len() > 1 { "Marche temporali" } else { "Marca temporale" }, x, y, w, 26.0, 16.0, SEMI, pal.text);
            y += 32.0;
            for t in &rep.timestamps {
                let c = level_color(f, t.level);
                f.icon(ic::CLOCK, x, y + 2.0, 20.0, c);
                f.text(&format!("{}, {}", firma::long_date(t.time), firma::hm(t.time)), x + 30.0, y, w - 30.0, 22.0, 14.5, SEMI, pal.text);
                let line = format!("{} · {}", crate::i18n::lit(&t.tsa), t.text);
                let lh = f.p.wrap_height(&line, 13.0, SANS, w - 30.0, 13.0 * 1.4, 4).max(18.0);
                f.para(&line, x + 30.0, y + 24.0, w - 30.0, 13.0, SANS, if t.level == Level::Ok { pal.muted } else { c }, 1.4, 4);
                y += 34.0 + lh;
            }
            f.hline(x, y, w);
            y += 12.0;
        }
        // Com'è fatto il file.
        let envs = rep.layers.iter().filter(|l| l.kind != firma::verify::LayerKind::Document).count();
        let pdf_sigs = rep.layers.last().is_some_and(|l| !l.signers.is_empty());
        if envs > 1 || (envs >= 1 && pdf_sigs) {
            f.text("Com'è fatto il file", x, y, w, 26.0, 16.0, SEMI, pal.text);
            y += 34.0;
            let mut lx = x;
            let mut lw = w;
            let n = rep.layers.len();
            let top = y;
            for (k, l) in rep.layers.iter().enumerate() {
                let is_doc = l.kind == firma::verify::LayerKind::Document;
                let h = 44.0;
                let ly = top + k as f32 * (h + 8.0);
                // Ogni strato dentro il precedente.
                let total_h = (n - k) as f32 * (h + 8.0) - 8.0 + 8.0;
                f.p.rrect(lx, ly, lw, total_h, 8.0, Some(if is_doc { pal.surface } else { mix(f.act_color(Activity::Firma), pal.surface, 0.94) }), Some((pal.border, 1.0)));
                let icon = if is_doc { ic::FILE } else if l.kind == firma::verify::LayerKind::Timestamped { ic::CLOCK } else { ic::ENVELOPE };
                f.icon(icon, lx + 10.0, ly + 11.0, 20.0, f.act_color(Activity::Firma));
                let label = if is_doc { crate::i18n::lit(&l.label) } else { l.label.clone() };
                let small = if l.signers.is_empty() {
                    String::new()
                } else if is_doc {
                    "con dentro la firma di".into()
                } else if l.signers.len() > 1 {
                    "firmata insieme da".into()
                } else {
                    "firmata da".into()
                };
                let lab_w = f.measure(&label, 14.0, SEMI);
                f.text(&label, lx + 38.0, ly, lw - 50.0, h, 14.0, SEMI, pal.text);
                let mut ax = lx + 44.0 + lab_w;
                if !small.is_empty() {
                    f.text(&small, ax, ly, lw, h, 12.5, SANS, pal.muted);
                    ax += f.measure(&small, 12.5, SANS) + 8.0;
                    for &si in &l.signers {
                        if ax + 30.0 > lx + lw {
                            break;
                        }
                        let s = &rep.signers[si];
                        avatar(f, ax + 13.0, ly + h / 2.0, 12.0, &s.initials, s.level);
                        ax += 30.0;
                    }
                }
                lx += 10.0;
                lw -= 20.0;
            }
            y = top + n as f32 * 52.0 + 12.0;
            f.hline(x, y, w);
            y += 12.0;
        }
        // Verifica alla data.
        if !rep.signers.is_empty() {
            f.text("Verifica alla data", x, y, w, 26.0, 16.0, SEMI, pal.text);
            y += 34.0;
            let sel = match v.at {
                At::Now => 0,
                At::Signing => 1,
                At::Custom(_) => 2,
            };
            let labels = ["Oggi", "Quando è stato firmato", "Un altro giorno"];
            let need: Vec<f32> = labels.iter().map(|l| f.measure(l, 13.5, SEMI) + 18.0).collect();
            let total: f32 = need.iter().sum();
            let mut cx = x;
            f.p.rrect(x, y, w, 38.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
            for (k, l) in labels.iter().enumerate() {
                let cw = need[k] * w / total;
                let rr = (cx, y, cw, 38.0);
                let act = Act::VwAt(k as u8);
                if k == sel {
                    f.p.rrect(rr.0 + 2.0, rr.1 + 2.0, rr.2 - 4.0, rr.3 - 4.0, 6.0, Some(pal.primary), None);
                } else if f.hovered(&act) {
                    f.p.rrect(rr.0 + 2.0, rr.1 + 2.0, rr.2 - 4.0, rr.3 - 4.0, 6.0, Some(pal.alt), None);
                }
                f.text_c(l, rr, 13.5, SEMI, if k == sel { pal.on_primary } else { pal.text });
                f.hit(rr, act);
                cx += cw;
            }
            y += 48.0;
            if sel == 2 || f.ui.edit == Some(crate::ui::core::Field::FirmaDate) || !v.at_text.is_empty() && sel == 2 {
                f.field((x, y, w - 130.0, 40.0), crate::ui::core::Field::FirmaDate, &v.at_text, "gg/mm/aaaa", 15.0, false);
                f.primary((x + w - 118.0, y, 118.0, 40.0), "Verifica", None, 15.0, Act::VwAtApply, true);
                y += 48.0;
                if v.at_error {
                    f.text("Scrivi la data come 03/10/2026.", x, y, w, 20.0, 13.0, SANS, f.pal.bad);
                    y += 24.0;
                }
            }
            let note = match v.at {
                At::Now => "Le firme con la marca temporale si controllano all'ora della marca; le altre a oggi.",
                At::Signing => "Si controlla all'ora della marca o, senza marca, a quella scritta da chi ha firmato (non è certa).",
                At::Custom(_) => "Si controlla se le firme valevano quel giorno.",
            };
            y += f.para(note, x, y, w, 13.0, SANS, pal.muted, 1.4, 3) + 16.0;
        }
        y - r.1 + 20.0
    });
}

/// L'interno di una firma aperta: il perché, i quattro controlli, i dettagli tecnici.
fn signer_body(f: &mut Frame, app: &App, rep: &Report, i: usize, x: f32, mut y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let s = &rep.signers[i];
    let v = app.firma.viewer.as_ref();
    let c = level_color(f, s.level);
    // Il perché.
    let th = f.p.wrap_height(&s.why, 14.0, SANS, w - 28.0, 1.45 * 14.0, 30);
    f.p.rrect(x, y, w, th + 22.0, 8.0, Some(mix(c, pal.surface, 0.9)), None);
    f.p.fill(x, y + 6.0, 3.0, th + 10.0, c);
    f.para(&s.why, x + 14.0, y + 11.0, w - 28.0, 14.0, SANS, pal.text, 1.45, 30);
    y += th + 34.0;
    // I quattro controlli, due per riga: il segno e il nome nel colore del giudizio, sotto il dettaglio.
    let cw = (w - 16.0) / 2.0;
    let mut row_y = y;
    for cks in s.checks.chunks(2) {
        let mut row_h: f32 = 0.0;
        for (k, ck) in cks.iter().enumerate() {
            let cx = x + k as f32 * (cw + 16.0);
            let cc = if ck.counts || ck.level == Level::Ok { level_color(f, ck.level) } else { pal.muted };
            f.icon(level_mark(ck.level), cx - 1.0, row_y + 3.0, 16.0, cc);
            f.text(ck.what, cx + 19.0, row_y, cw - 19.0, 22.0, 14.0, BOLD, cc);
            let th = f.p.wrap_height(&ck.text, 12.5, SANS, cw, 12.5 * 1.35, 4).max(17.0);
            f.para(&ck.text, cx, row_y + 21.0, cw, 12.5, SANS, pal.muted, 1.35, 4);
            row_h = row_h.max(21.0 + th);
        }
        row_y += row_h + 10.0;
    }
    y = row_y + 2.0;
    if s.revision_end.is_some() {
        let showing = v.is_some_and(|v| v.revision.as_ref().is_some_and(|(k, _)| *k == i));
        let label = if showing { "Torna al file com'è adesso" } else { "Vedi la versione firmata" };
        let bw = f.measure(label, 14.5, SANS) + 40.0;
        f.button((x, y, bw, 38.0), label, None, 14.5, if showing { Act::VwRevisionOff } else { Act::VwRevision(i) });
        y += 48.0;
    }
    // Dettagli tecnici.
    let open = v.is_some_and(|v| v.tech.contains(&i));
    let act = Act::VwTech(i);
    f.icon(if open { ic::CHEVRON_DOWN } else { "M9 6l6 6-6 6" }, x - 2.0, y + 4.0, 18.0, pal.primary);
    f.text("Dettagli tecnici del certificato", x + 20.0, y, w - 20.0, 26.0, 14.0, SEMI, pal.primary);
    f.hit((x, y, w, 26.0), act);
    y += 32.0;
    if open {
        let kw = 128.0;
        for (k, val) in &s.details {
            let h = f.p.wrap_height(val, 13.0, SANS, w - kw, 13.0 * 1.4, 4).max(18.0);
            f.text(k, x, y, kw - 8.0, 18.0, 12.5, SANS, pal.muted);
            f.para(&crate::i18n::lit(val), x + kw, y, w - kw, 13.0, SANS, pal.text, 1.4, 4);
            y += h + 8.0;
        }
        // La catena: elenco UE › certificatore › persona.
        let mut cx = x;
        let mut cy = y + 4.0;
        for (k, step) in s.chain.iter().enumerate() {
            let t = crate::i18n::lit(step);
            let tw = f.measure(&t, 12.5, SEMI) + 18.0;
            if k > 0 {
                if cx + 16.0 + tw > x + w {
                    cx = x;
                    cy += 34.0;
                } else {
                    f.text("›", cx + 2.0, cy, 14.0, 26.0, 14.0, SANS, pal.muted);
                    cx += 16.0;
                }
            }
            f.p.rrect(cx, cy, tw, 26.0, 13.0, Some(pal.alt), None);
            f.text_c(&t, (cx, cy, tw, 26.0), 12.5, SEMI, pal.text);
            cx += tw + 4.0;
        }
        y = cy + 40.0;
    }
    y
}
