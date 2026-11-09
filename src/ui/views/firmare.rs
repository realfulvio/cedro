//! «Firma documenti» in tre passi (File, Come firmare, PIN e firma) e il riepilogo, «Chiavetta e
//! certificati» e «Opzioni della firma». Disegno dall'anteprima approvata
//! (firma-digitale/anteprima-firma.html).

use super::*;
use crate::firma::{self, pkcs11};
use crate::ui::core::Scroll;
use crate::ui::firmare::{Action, Page, SignFile, State};
use crate::ui::gfx::Rgb;

/// Una chiavetta USB, una carta, una chiave, un lucchetto aperto.
pub const USB: &str = "M9 2h6v6H9zM7 8h10v11a3 3 0 0 1-3 3h-4a3 3 0 0 1-3-3zM12 13v4";
pub const CARD: &str = "M3 6h18v12H3zM6 10h5v4H6zM14 10h4M14 13h3";
pub const KEY: &str = "M14.5 9.5a4 4 0 1 1-5.66 5.66A4 4 0 0 1 14.5 9.5zM14.5 9.5L21 3M18 6l2 2M16 8l1.5 1.5";
pub const UNLOCK: &str = "M5 11h14v10H5zM8 11V7a4 4 0 0 1 7.5-2";
pub const WAND: &str = "M4 20L14 10M15 4v2M15 10v2M12 7h2M16 7h2M18.5 3.5l1 1M19 10l1 1";
const BACK: &str = "M15 6l-6 6 6 6";

fn accent(f: &Frame) -> Rgb {
    f.act_color(Activity::Firma)
}

/// Il pulsante pieno viola (Avanti, Firma).
fn go_button(f: &mut Frame, r: R, label: &str, icon: Option<&str>, act: Act, enabled: bool) {
    let c = accent(f);
    let hot = f.hovered(&act);
    if enabled {
        f.glossy_fill(r, 9.0, c, hot);
    } else {
        f.p.rrect(r.0, r.1, r.2, r.3, 9.0, Some(mix(c, f.pal.surface, 0.55)), None);
    }
    let tw = f.measure(label, 17.0, SEMI);
    let total = tw + if icon.is_some() { 30.0 } else { 0.0 };
    let mut x = r.0 + (r.2 - total) / 2.0;
    if let Some(i) = icon {
        f.icon(i, x, r.1 + (r.3 - 20.0) / 2.0, 20.0, (255, 255, 255));
        x += 30.0;
    }
    f.text(label, x, r.1, tw + 2.0, r.3, 17.0, SEMI, (255, 255, 255));
    if enabled {
        f.focus_ring(r, 9.0, &act);
        f.hit(r, act);
    }
}

/// Un pulsantino a pillola con bordo (Cambia, Automatico, Stemma).
fn small_button(f: &mut Frame, x_right: f32, cy: f32, label: &str, act: Act) -> f32 {
    let w = f.measure(label, 13.5, SANS) + 26.0;
    f.button((x_right - w, cy - 16.0, w, 32.0), label, None, 13.5, act);
    w
}

/// «‹ Firma digitale» e il titolo della pagina. Restituisce la y sotto il titolo.
fn header(f: &mut Frame, r: R, title: &str) -> f32 {
    let pal = f.pal;
    let back = Act::FirmaPage(0);
    let bw = f.measure("Firma digitale", 14.5, SANS) + 30.0;
    if f.hovered(&back) {
        f.p.rrect(r.0 - 6.0, r.1, bw + 8.0, 30.0, 6.0, Some(pal.alt), None);
    }
    f.icon(BACK, r.0 - 4.0, r.1 + 6.0, 18.0, pal.text);
    f.text("Firma digitale", r.0 + 18.0, r.1, bw, 30.0, 14.5, SANS, pal.text);
    f.hit((r.0 - 6.0, r.1, bw + 8.0, 30.0), back);
    f.text(title, r.0, r.1 + 30.0, r.2 - 200.0, 52.0, 32.0, SEMI, pal.text);
    r.1 + 90.0
}

/// I tre passi in alto a destra (stretto: sotto il titolo). Restituisce l'altezza in più che occupa.
fn stepper(f: &mut Frame, r: R, step: u8) -> f32 {
    let pal = f.pal;
    let c = accent(f);
    let labels = ["File", "Come firmare", "PIN e firma"];
    let line = 56.0;
    let widths: Vec<f32> = labels.iter().map(|l| 30.0 + 10.0 + f.measure(l, 15.5, SANS)).collect();
    let total: f32 = widths.iter().sum::<f32>() + line * 2.0 + 24.0 * 2.0;
    let title = f.measure("Firma documenti", 32.0, SEMI);
    let below = total > r.2 - title - 30.0;
    let mut x = if below { r.0 } else { r.0 + r.2 - total };
    let cy = if below { r.1 + 90.0 + 18.0 } else { r.1 + 30.0 + 26.0 };
    for (k, l) in labels.iter().enumerate() {
        let n = k as u8 + 1;
        let on = n == step;
        let done = n < step;
        if on {
            f.p.circle(x + 15.0, cy, 15.0, Some(c), None);
            f.text_c(&n.to_string(), (x, cy - 15.0, 30.0, 30.0), 14.0, SEMI, (255, 255, 255));
        } else {
            f.p.circle(x + 15.0, cy, 14.0, Some(pal.surface), Some((if done { c } else { pal.border }, 1.5)));
            f.text_c(&n.to_string(), (x, cy - 15.0, 30.0, 30.0), 14.0, SEMI, if done { c } else { pal.text });
        }
        let act = Act::FirmaStep(n);
        let tw = f.measure(l, 15.5, if on { SEMI } else { SANS });
        f.text(l, x + 40.0, cy - 14.0, tw + 4.0, 28.0, 15.5, if on { SEMI } else { SANS }, pal.text);
        if done {
            f.hit((x - 4.0, cy - 18.0, tw + 52.0, 36.0), act);
        }
        x += widths[k];
        if k < 2 {
            f.p.fill(x + 12.0, cy - 0.75, line, 1.5, if n < step { c } else { pal.border });
            x += line + 24.0;
        }
    }
    if below {
        48.0
    } else {
        0.0
    }
}

fn doc_chip(f: &mut Frame, x: f32, y: f32, p: &std::path::Path) {
    let l = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let (label, c): (&str, Rgb) = if l.ends_with(".p7m") || l.ends_with(".p7s") {
        ("P7M", (0x5B, 0x4B, 0xD1))
    } else if l.ends_with(".pdf") {
        ("PDF", (0xD6, 0x3A, 0x2F))
    } else if l.ends_with(".xls") || l.ends_with(".xlsx") || l.ends_with(".ods") || l.ends_with(".csv") {
        ("XLS", (0x1E, 0x7E, 0x4F))
    } else if l.ends_with(".doc") || l.ends_with(".docx") || l.ends_with(".odt") || l.ends_with(".rtf") {
        ("DOC", (0x2B, 0x6C, 0xD8))
    } else if l.ends_with(".xml") {
        ("XML", (0x1E, 0x8A, 0x55))
    } else if [".jpg", ".jpeg", ".png", ".tif", ".tiff", ".bmp"].iter().any(|e| l.ends_with(e)) {
        ("IMG", (0xC2, 0x6A, 0x0E))
    } else {
        ("FILE", (0x55, 0x60, 0x70))
    };
    f.p.rrect(x, y, 32.0, 36.0, 4.0, Some(c), None);
    f.text_c(label, (x, y + 20.0, 32.0, 14.0), 8.5, BOLD, (255, 255, 255));
}

fn tag(f: &mut Frame, x_right: f32, cy: f32, text: &str, c: Rgb) -> f32 {
    let pal = f.pal;
    let w = f.measure(text, 13.0, SEMI) + 20.0;
    f.p.rrect(x_right - w, cy - 12.0, w, 24.0, 12.0, Some(mix(c, pal.surface, 0.86)), None);
    f.text_c(text, (x_right - w, cy - 12.0, w, 24.0), 13.0, SEMI, c);
    w
}

fn name_of(p: &std::path::Path) -> String {
    crate::i18n::lit(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

fn names(list: &[String]) -> String {
    match list.len() {
        0 => String::new(),
        1 => list[0].clone(),
        n => format!("{} e {}", list[..n - 1].join(", "), list[n - 1]),
    }
}

/// La riga del file al passo 1: cosa diventa.
fn file_line(f: &SignFile, visible: bool, existing: u8) -> (String, String, &'static str, bool) {
    let out = name_of(&f.out);
    match f.action {
        Action::Pdf => ("diventa ".into(), format!("{out}{}", if visible { ", con la firma visibile" } else { "" }), "PDF firmato", false),
        Action::ConvertPdf => ("lo trasformo in PDF: diventa ".into(), out, "PDF firmato", false),
        Action::P7m if !f.signers.is_empty() && existing == 2 => (format!("già firmato da {}: lo metto in una busta nuova, ", names(&f.signers)), out, "Busta nuova", true),
        Action::P7m => ("diventa ".into(), out, ".p7m", false),
        Action::Parallel => (format!("già firmato da {}: la tua firma va accanto ", names(&f.signers)), if f.signers.len() == 1 { "alla sua".into() } else { "alle loro".into() }, "Seconda firma", true),
        Action::Counter => (format!("già firmato da {}: la tua firma va sopra, come ", names(&f.signers)), "controfirma".into(), "Controfirma", true),
        Action::PdfAgain => (format!("già firmato da {}: aggiungo ", names(&f.signers)), "la tua firma nel PDF".into(), "Seconda firma", true),
    }
}

/// La pagina della sezione che non è l'inizio. Falso se si deve disegnare l'inizio.
pub fn page(f: &mut Frame, app: &App, r: R) -> bool {
    let su = &app.firma.sign;
    // A destra resta il posto della barra di scorrimento.
    let ri = (r.0, r.1, r.2 - 16.0, r.3);
    match su.page {
        Page::Home => false,
        Page::Sign => {
            super::scrolled(f, r, Scroll::FirmaSign, |f| match su.step {
                1 => step1(f, app, ri),
                2 => step2(f, app, ri),
                3 => step3(f, app, ri),
                _ => done(f, app, ri),
            });
            true
        }
        Page::Devices => {
            super::scrolled(f, r, Scroll::FirmaSign, |f| devices(f, app, ri));
            true
        }
        Page::Options => {
            super::scrolled(f, r, Scroll::FirmaSign, |f| options(f, app, ri));
            true
        }
    }
}

fn step1(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let mut y = header(f, r, "Firma documenti");
    y += stepper(f, r, 1);
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 34.0), 14.0);
    f.text("Questi file verranno firmati", r.0, y, r.2, 30.0, 19.0, SEMI, pal.text);
    f.text("Puoi trascinarne altri dentro la finestra in qualsiasi momento.", r.0, y + 30.0, r.2, 22.0, 14.5, SANS, pal.muted);
    y += 64.0;
    let row = 65.0;
    let n = su.files.len();
    if n == 0 {
        let rc = (r.0, y, r.2, 120.0);
        f.p.rrect(rc.0, rc.1, rc.2, rc.3, 12.0, Some(mix(accent(f), pal.surface, 0.96)), None);
        f.p.rrect_line(rc.0, rc.1, rc.2, rc.3, 12.0, mix(accent(f), pal.surface, 0.45), 1.6, 1.0, Some((8.0, 6.0)));
        f.text_c("Nessun file: aggiungili o trascinali qui", rc, 16.0, SANS, pal.muted);
        f.hit(rc, Act::FirmaSignAdd);
        y += 140.0;
    } else {
        let list = (r.0, y, r.2, n as f32 * row + 2.0);
        f.card(list, 12.0);
        let visible = su.s.firma_visible;
        for (k, sf) in su.files.iter().enumerate() {
            let ry = y + 1.0 + k as f32 * row;
            if k > 0 {
                f.hline(list.0, ry, list.2);
            }
            doc_chip(f, r.0 + 18.0, ry + 14.0, &sf.path);
            let tx = r.0 + 70.0;
            let tw = r.2 - 70.0 - 210.0;
            f.text(&name_of(&sf.path), tx, ry + 10.0, tw, 24.0, 15.5, SEMI, pal.text);
            let (a, b, t, warm) = file_line(sf, visible, su.s.firma_existing);
            let aw = f.measure(&a, 13.5, SANS);
            f.text(&crate::i18n::lit(&a), tx, ry + 34.0, aw + 2.0, 20.0, 13.5, SANS, pal.muted);
            f.text(&crate::i18n::lit(&b), tx + aw, ry + 34.0, (tw - aw).max(40.0), 20.0, 13.5, if b.contains('.') { SEMI } else { SANS }, if b.contains('.') { pal.text } else { pal.muted });
            let c = if warm { (0xA8, 0x5A, 0x00) } else { accent(f) };
            let rm = Act::FirmaSignRemove(k);
            let tag_right = r.0 + r.2 - 52.0;
            tag(f, tag_right, ry + row / 2.0, t, c);
            let hot = f.hovered(&rm);
            f.icon(ic::CLOSE, r.0 + r.2 - 40.0, ry + row / 2.0 - 10.0, 20.0, if hot { pal.text } else { pal.muted });
            f.hit((r.0 + r.2 - 46.0, ry + row / 2.0 - 16.0, 32.0, 32.0), rm);
        }
        y += list.3 + 22.0;
    }
    let w1 = f.measure("Aggiungi file", 15.5, SANS) + 58.0;
    f.button((r.0, y, w1, 44.0), "Aggiungi file", Some(ic::ADD), 15.5, Act::FirmaSignAdd);
    let w2 = f.measure("Aggiungi cartella", 15.5, SANS) + 50.0;
    let fa = Act::FirmaSignAddFolder;
    if f.hovered(&fa) {
        f.p.rrect(r.0 + w1 + 12.0, y, w2, 44.0, 8.0, Some(pal.alt), None);
    }
    f.icon(ic::FOLDER, r.0 + w1 + 24.0, y + 12.0, 20.0, pal.text);
    f.text("Aggiungi cartella", r.0 + w1 + 52.0, y, w2, 44.0, 15.5, SANS, pal.text);
    f.hit((r.0 + w1 + 12.0, y, w2, 44.0), fa);
    let gw = 106.0;
    go_button(f, (r.0 + r.2 - gw, y - 3.0, gw, 50.0), "Avanti", None, Act::FirmaStep(2), n > 0);
    let count = if n == 1 { "1 file".to_string() } else { format!("{n} file") };
    f.text_r(&count, (r.0, y, r.2 - gw - 14.0, 44.0), 15.0, SANS, pal.muted);
    y + 60.0 - r.1
}

/// Una scelta grande (dispositivo, formato): icona in alto, titolo, due righe.
fn choice(f: &mut Frame, rc: R, icon: &str, title: &str, text: &str, on: bool, enabled: bool, badge: Option<&str>, act: Act) {
    let pal = f.pal;
    let c = accent(f);
    let hot = enabled && f.hovered(&act);
    if on {
        f.p.rrect(rc.0, rc.1, rc.2, rc.3, 12.0, Some(mix(c, pal.surface, 0.95)), Some((c, 2.0)));
    } else {
        f.card(rc, 12.0);
        if hot {
            f.p.rrect_line(rc.0, rc.1, rc.2, rc.3, 12.0, mix(c, pal.surface, 0.4), 1.5, 1.0, None);
        }
    }
    let fade = if enabled { 0.0 } else { 0.55 };
    let tc = mix(pal.text, pal.surface, fade);
    let mc = mix(pal.muted, pal.surface, fade);
    f.p.rrect(rc.0 + 16.0, rc.1 + 16.0, 38.0, 38.0, 9.0, Some(mix(c, pal.surface, if enabled { 0.88 } else { 0.94 })), None);
    f.icon(icon, rc.0 + 25.0, rc.1 + 25.0, 20.0, mix(c, pal.surface, fade));
    if let Some(b) = badge {
        tag(f, rc.0 + rc.2 - 14.0, rc.1 + 28.0, b, c);
    }
    f.text(title, rc.0 + 16.0, rc.1 + 62.0, rc.2 - 32.0, 26.0, 16.5, SEMI, tc);
    f.para(text, rc.0 + 16.0, rc.1 + 90.0, rc.2 - 32.0, 13.5, SANS, mc, 1.4, 2);
    if enabled {
        f.hit(rc, act);
    }
}

fn step2(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let mut y = header(f, r, "Firma documenti");
    y += stepper(f, r, 2);
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 34.0), 14.0);
    // Con cosa firmi?
    f.text("Con cosa firmi?", r.0, y, r.2 - 200.0, 30.0, 19.0, SEMI, pal.text);
    let rs = Act::FirmaScan;
    let rl = if su.scanning { "Cerco…" } else { "Cerca di nuovo" };
    let rw = f.measure(rl, 14.5, SANS) + 40.0;
    if f.hovered(&rs) {
        f.p.rrect(r.0 + r.2 - rw, y, rw, 30.0, 6.0, Some(pal.alt), None);
    }
    f.icon(ic::REFRESH, r.0 + r.2 - rw + 8.0, y + 6.0, 18.0, pal.text);
    f.text(rl, r.0 + r.2 - rw + 32.0, y, rw, 30.0, 14.5, SANS, pal.text);
    f.hit((r.0 + r.2 - rw, y, rw, 30.0), rs);
    y += 40.0;
    let gap = 14.0;
    let cols = if r.2 >= 900.0 { 3 } else { 2 };
    let cw = (r.2 - (cols - 1) as f32 * gap) / cols as f32;
    let ch = 142.0;
    if su.devices.is_empty() {
        let rc = (r.0, y, r.2, 112.0);
        f.card(rc, 12.0);
        if su.scanning || !su.scanned_once {
            f.spinner(rc.0 + 40.0, rc.1 + 56.0, 12.0, accent(f));
            f.animating = true;
            f.text("Cerco la chiavetta e le smart card…", rc.0 + 70.0, rc.1, rc.2 - 90.0, rc.3, 16.0, SANS, pal.text);
        } else {
            f.icon(USB, rc.0 + 24.0, rc.1 + 22.0, 26.0, accent(f));
            f.text("Non trovo nessuna chiavetta né smart card", rc.0 + 66.0, rc.1 + 16.0, rc.2 - 90.0, 28.0, 16.5, SEMI, pal.text);
            let why = if pkcs11::driver_candidates().is_empty() {
                "Manca il driver della chiavetta (per la Aruba Key: bit4id, che si installa con ArubaSign o dal sito Aruba). Collegala e premi «Cerca di nuovo»; il driver si sceglie anche a mano in Opzioni."
            } else {
                "Collega la chiavetta o inserisci la carta nel lettore, aspetta che la spia smetta di lampeggiare e premi «Cerca di nuovo»."
            };
            f.para(why, rc.0 + 66.0, rc.1 + 46.0, rc.2 - 90.0, 13.5, SANS, pal.muted, 1.4, 3);
        }
        y += rc.3 + 26.0;
    } else {
        for (k, d) in su.devices.iter().enumerate() {
            let col = k % cols;
            let rowi = k / cols;
            let rc = (r.0 + col as f32 * (cw + gap), y + rowi as f32 * (ch + gap), cw, ch);
            let sc = d.signing_cert();
            let (text, enabled) = match sc {
                Some(c) => (format!("{} · collegata · valida fino al {}", c.cert.subject.person(), firma::date(c.cert.not_after)), true),
                None => ("Inserita, ma non ha un certificato di firma: serve solo per entrare nei siti".to_string(), false),
            };
            let icon = if d.kind == pkcs11::DevKind::Card { CARD } else { USB };
            choice(f, rc, icon, &d.title(), &text, enabled && k == su.device, enabled, None, Act::FirmaDevice(k));
        }
        y += su.devices.len().div_ceil(cols) as f32 * (ch + gap) + 12.0;
    }
    // In che formato?
    f.text("In che formato?", r.0, y, r.2, 30.0, 19.0, SEMI, pal.text);
    y += 40.0;
    let fmts: [(&str, &str, &str); 3] = [
        (WAND, "Lo scelgo io per ogni file", "I PDF restano PDF con la firma sul foglio, tutti gli altri diventano .p7m"),
        (ic::ENVELOPE, "Tutto in .p7m", "Busta firmata (CAdES): va bene per qualsiasi file"),
        (ic::FILE, "Tutto in PDF firmato", "Firma PAdES: i file non PDF li converto prima in PDF"),
    ];
    for (k, (icon, t, d)) in fmts.iter().enumerate() {
        let rc = (r.0 + (k % cols) as f32 * (cw + gap), y + (k / cols) as f32 * (ch + gap), cw, ch);
        choice(f, rc, icon, t, d, su.s.firma_format == k as u8, true, (k == 0).then_some("consigliato"), Act::FirmaFormat(k as u8));
    }
    y += 3usize.div_ceil(cols) as f32 * (ch + gap) + 4.0;
    // Dove si vede la firma nel PDF.
    if let Some(pdf) = su.first_pdf() {
        y = placement(f, app, r, y, pdf) + 18.0;
    }
    // Altre opzioni.
    y = more(f, app, r, y) + 22.0;
    let bw = f.measure("Indietro", 15.5, SANS) + 36.0;
    f.button((r.0, y, bw, 44.0), "Indietro", None, 15.5, Act::FirmaStep(1));
    go_button(f, (r.0 + r.2 - 106.0, y - 3.0, 106.0, 50.0), "Avanti", None, Act::FirmaStep(3), su.can_sign());
    if !su.can_sign() && su.scanned_once {
        f.text_r("Serve una chiavetta o una carta con il certificato di firma", (r.0, y, r.2 - 120.0, 44.0), 14.0, SANS, pal.muted);
    }
    y + 60.0 - r.1
}

/// «Dove si vede la firma nel PDF»: la pagina a sinistra con il riquadro, le tre scelte a destra.
fn placement(f: &mut Frame, app: &App, r: R, y: f32, pdf: &SignFile) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let c = accent(f);
    let h = 372.0;
    let rc = (r.0, y, r.2, h);
    f.card(rc, 12.0);
    let tw = f.measure("Dove si vede la firma nel PDF", 16.5, SEMI);
    f.text("Dove si vede la firma nel PDF", rc.0 + 18.0, rc.1 + 8.0, tw + 4.0, 34.0, 16.5, SEMI, pal.text);
    f.text(&name_of(&pdf.path), rc.0 + 30.0 + tw, rc.1 + 9.0, rc.2 - tw - 60.0, 34.0, 13.5, SANS, pal.muted);
    f.hline(rc.0, rc.1 + 48.0, rc.2);
    // La pagina.
    let (page, frac) = su.box_frac().unwrap_or((su.box_page.min(pdf.pages.len().saturating_sub(1)), [0.0; 4]));
    let pb = pdf.pages.get(page).copied().unwrap_or([0.0, 0.0, 595.0, 842.0]);
    let ratio = (pb[3] - pb[1]) / (pb[2] - pb[0]).max(1.0);
    let ph = 254.0f32;
    let pw = (ph / ratio).min(200.0);
    let px = rc.0 + 18.0;
    let py = rc.1 + 66.0;
    f.p.shadow(px, py, pw, ph, 2.0, 8.0, 0.14, 2.0);
    f.p.fill(px, py, pw, ph, (255, 255, 255));
    match app.previews.get(&(pdf.path.clone(), page)).and_then(|i| i.as_ref()) {
        Some(img) => f.p.image(img, px, py, pw, ph, 0.0, crate::ui::gfx::Fit::Contain),
        None => {
            f.spinner(px + pw / 2.0, py + ph / 2.0, 10.0, c);
            f.animating = true;
        }
    }
    let shown = su.s.firma_visible && su.place != 2;
    if shown && frac[2] > frac[0] {
        let bx = (px + frac[0] * pw, py + frac[1] * ph, (frac[2] - frac[0]) * pw, (frac[3] - frac[1]) * ph);
        f.p.rrect(bx.0, bx.1, bx.2, bx.3, 2.0, Some(mix(c, (255, 255, 255), 0.85)), Some((c, 1.5)));
        let fs = (bx.3 * 0.24).clamp(5.0, 9.0);
        f.text_c("Firmato da", (bx.0, bx.1 + bx.3 * 0.12, bx.2, bx.3 * 0.36), fs, SANS, (0x30, 0x2A, 0x70));
        f.text_c(&su.device().and_then(|d| d.signing_cert()).map(|c| c.cert.subject.person()).unwrap_or_else(|| "IL TUO NOME".into()), (bx.0, bx.1 + bx.3 * 0.46, bx.2, bx.3 * 0.36), fs, BOLD, (0x30, 0x2A, 0x70));
    }
    if su.place == 1 && shown {
        f.hit_kind((px, py, pw, ph), Act::FirmaBox, crate::ui::core::HitKind::Slider);
    }
    let pages = pdf.pages.len().max(1);
    let label = format!("pagina {} di {pages}", page + 1);
    let lw = f.measure(&label, 13.0, SANS);
    let ly = py + ph + 8.0;
    let mid = px + pw / 2.0;
    f.text_c(&label, (mid - lw / 2.0, ly, lw, 22.0), 13.0, SANS, pal.muted);
    if su.place == 1 {
        for (d, x, icon) in [(-1, mid - lw / 2.0 - 26.0, "M15 6l-6 6 6 6"), (1, mid + lw / 2.0 + 6.0, "M9 6l6 6-6 6")] {
            let a = Act::FirmaBoxPage(d);
            f.icon(icon, x, ly + 2.0, 18.0, if f.hovered(&a) { pal.text } else { pal.muted });
            f.hit((x - 4.0, ly - 2.0, 26.0, 26.0), a);
        }
    }
    // Le scelte.
    let ox = px + pw + 22.0;
    let ow = rc.0 + rc.2 - 18.0 - ox;
    let field = pdf.fields.first();
    let opts: [(&str, String, bool); 3] = [
        (
            "Nello spazio per la firma",
            match field {
                Some(fl) => format!("Il PDF ha già un riquadro «{}» a pagina {}", fl.name, fl.page),
                None => "Questo PDF non ha uno spazio per la firma: la metto in basso a destra dell'ultima pagina".into(),
            },
            true,
        ),
        ("Dove la metto io", "Trascina il riquadro sulla pagina".into(), true),
        ("Non si vede", "La firma c'è e vale uguale, ma sul foglio non compare".into(), true),
    ];
    let mut oy = rc.1 + 66.0;
    for (k, (t, d, _)) in opts.iter().enumerate() {
        let on = shown && su.place == k as u8 || !su.s.firma_visible && k == 2;
        let orc = (ox, oy, ow, 56.0);
        let act = Act::FirmaPlace(k as u8);
        let hot = f.hovered(&act);
        f.p.rrect(orc.0, orc.1, orc.2, orc.3, 9.0, Some(if on { mix(c, pal.surface, 0.95) } else { pal.surface }), Some((if on { c } else if hot { mix(c, pal.border, 0.5) } else { pal.border }, if on { 1.5 } else { 1.0 })));
        f.radio(orc.0 + 24.0, orc.1 + 28.0, 9.0, on, pal.muted);
        f.text(t, orc.0 + 46.0, orc.1 + 7.0, orc.2 - 60.0, 24.0, 15.0, SEMI, pal.text);
        f.text(&crate::i18n::lit(d), orc.0 + 46.0, orc.1 + 29.0, orc.2 - 60.0, 20.0, 13.0, SANS, pal.muted);
        f.hit(orc, act);
        oy += 64.0;
    }
    let img = if su.s.firma_image.is_empty() { "" } else { ", l'immagine scelta" };
    let note = format!("Nel riquadro: nome, data e ora{img}.");
    let nw = f.measure(&note, 13.5, SANS);
    f.text(&note, ox, oy + 4.0, nw + 4.0, 24.0, 13.5, SANS, pal.muted);
    let ca = Act::FirmaPage(3);
    let cl = "Cambia aspetto";
    let cwid = f.measure(cl, 13.5, SANS);
    f.text(cl, ox + nw + 10.0, oy + 4.0, cwid + 4.0, 24.0, 13.5, if f.hovered(&ca) { SEMI } else { SANS }, pal.primary);
    f.hit((ox + nw + 8.0, oy + 2.0, cwid + 8.0, 28.0), ca);
    y + h
}

/// «Altre opzioni»: chiusa è una riga col riassunto, aperta le scelte.
fn more(f: &mut Frame, app: &App, r: R, y: f32) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let s = &su.s;
    let dest = if s.firma_dir.trim().is_empty() { "accanto all'originale".to_string() } else { format!("in {}", s.firma_dir.trim()) };
    let existing = ["seconda firma accanto", "controfirma sopra", "busta nuova"][s.firma_existing.min(2) as usize];
    let summary = format!("{dest} · file già firmati: {existing}");
    let act = Act::FirmaMore;
    if !su.more {
        let rc = (r.0, y, r.2, 50.0);
        f.card(rc, 10.0);
        if f.hovered(&act) {
            f.p.rrect_line(rc.0, rc.1, rc.2, rc.3, 10.0, pal.border, 1.5, 1.0, None);
        }
        f.icon(ic::SETTINGS, rc.0 + 18.0, rc.1 + 14.0, 20.0, pal.text);
        let tw = f.measure("Altre opzioni", 16.0, SEMI);
        f.text("Altre opzioni", rc.0 + 48.0, rc.1, tw + 4.0, rc.3, 16.0, SEMI, pal.text);
        f.text(&crate::i18n::lit(&summary), rc.0 + 60.0 + tw, rc.1, rc.2 - tw - 110.0, rc.3, 13.5, SANS, pal.muted);
        f.icon(ic::CHEVRON_DOWN, rc.0 + rc.2 - 38.0, rc.1 + 14.0, 20.0, pal.muted);
        f.hit(rc, act);
        return y + 50.0;
    }
    let rows: Vec<(&str, String)> = vec![
        ("Dove salvo", if s.firma_dir.trim().is_empty() { "I file firmati vanno accanto agli originali".into() } else { format!("Nella cartella {}", s.firma_dir.trim()) }),
        ("File già firmati", "Accanto: firma parallela · Sopra: controfirma · Busta nuova: firma sulla busta".into()),
        ("Motivo e luogo", if s.firma_reason.is_empty() && s.firma_location.is_empty() { "Si scrivono nella firma dei PDF (facoltativi)".into() } else { format!("«{}» · «{}»", s.firma_reason, s.firma_location) }),
    ];
    let rh = 66.0;
    let h = 50.0 + rows.len() as f32 * rh + 8.0;
    let rc = (r.0, y, r.2, h);
    f.card(rc, 10.0);
    f.icon(ic::SETTINGS, rc.0 + 18.0, rc.1 + 14.0, 20.0, pal.text);
    f.text("Altre opzioni", rc.0 + 48.0, rc.1, 300.0, 50.0, 16.0, SEMI, pal.text);
    f.icon("M6 15l6-6 6 6", rc.0 + rc.2 - 38.0, rc.1 + 14.0, 20.0, pal.muted);
    f.hit((rc.0, rc.1, rc.2, 50.0), act);
    for (k, (t, d)) in rows.iter().enumerate() {
        let ry = rc.1 + 50.0 + k as f32 * rh;
        f.hline(rc.0 + 18.0, ry, rc.2 - 36.0);
        f.text(t, rc.0 + 18.0, ry + 10.0, rc.2 * 0.5, 24.0, 15.0, SEMI, pal.text);
        f.text(&crate::i18n::lit(d), rc.0 + 18.0, ry + 34.0, rc.2 * 0.55, 20.0, 13.0, SANS, pal.muted);
        let cy = ry + rh / 2.0;
        let right = rc.0 + rc.2 - 18.0;
        match k {
            0 => {
                let w = seg(f, right, cy, &["Accanto", "Altra cartella"], if s.firma_dir.trim().is_empty() { 0 } else { 1 }, |i| Act::FirmaDest(i));
                let _ = w;
            }
            1 => {
                seg(f, right, cy, &["Accanto", "Sopra", "Busta nuova"], s.firma_existing as usize, |i| Act::FirmaExisting(i));
            }
            _ => {
                let fw = (rc.2 * 0.2).clamp(150.0, 220.0);
                f.field((right - fw, cy - 18.0, fw, 36.0), Field::FirmaLocation, &s.firma_location, "Luogo", 14.0, false);
                f.field((right - 2.0 * fw - 10.0, cy - 18.0, fw, 36.0), Field::FirmaReason, &s.firma_reason, "Motivo", 14.0, false);
            }
        }
    }
    y + h
}

/// Le scelte a pulsanti uniti (Accanto | Sopra | Busta nuova). Restituisce la larghezza.
fn seg(f: &mut Frame, right: f32, cy: f32, labels: &[&str], sel: usize, act: impl Fn(u8) -> Act) -> f32 {
    let pal = f.pal;
    let c = accent(f);
    let ws: Vec<f32> = labels.iter().map(|l| f.measure(l, 13.5, SANS) + 26.0).collect();
    let total: f32 = ws.iter().sum();
    let mut x = right - total;
    f.p.rrect(x, cy - 17.0, total, 34.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    for (k, l) in labels.iter().enumerate() {
        let on = k == sel;
        let a = act(k as u8);
        if on {
            f.p.rrect(x + 2.0, cy - 15.0, ws[k] - 4.0, 30.0, 6.0, Some(mix(c, pal.surface, 0.86)), None);
        } else if f.hovered(&a) {
            f.p.rrect(x + 2.0, cy - 15.0, ws[k] - 4.0, 30.0, 6.0, Some(pal.alt), None);
        }
        f.text_c(l, (x, cy - 17.0, ws[k], 34.0), 13.5, if on { SEMI } else { SANS }, if on { c } else { pal.text });
        f.hit((x, cy - 17.0, ws[k], 34.0), a);
        x += ws[k];
    }
    total
}

fn step3(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let c = accent(f);
    let mut y = header(f, r, "Firma documenti");
    y += stepper(f, r, 3);
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 34.0), 14.0);
    let dev = su.device();
    let n = su.files.len();
    let nfile = if n == 1 { "1 file".to_string() } else { format!("{n} file") };
    // Il riassunto, con le parti importanti nel colore della sezione.
    let mut parts: Vec<(String, bool)> = vec![("Firmo ".into(), false), (nfile.clone(), true), (" con la ".into(), false)];
    let who = dev.map(|d| format!("{} di {}", d.title(), d.owner())).unwrap_or_default();
    parts.push((who, true));
    parts.push((". ".into(), false));
    let pdfs = su.files.iter().filter(|x| matches!(x.action, Action::Pdf | Action::ConvertPdf)).count();
    let p7ms = su.files.iter().filter(|x| x.action == Action::P7m).count();
    let visible = su.s.firma_visible && su.place != 2;
    if pdfs > 0 {
        let page = su.box_frac().map(|(p, _)| format!(" a pagina {}", p + 1)).unwrap_or_default();
        let what = if pdfs == 1 { "Il PDF resta un PDF" } else { "I PDF restano PDF" };
        parts.push((format!("{what} con la firma {}", if visible { format!("visibile{page}") } else { "che non si vede sul foglio".into() }), false));
        parts.push((if p7ms > 0 { ", gli altri diventano ".into() } else { ". ".into() }, false));
        if p7ms > 0 {
            parts.push((".p7m".into(), true));
            parts.push((". ".into(), false));
        }
    } else if p7ms > 0 {
        parts.push((if p7ms == 1 { "Diventa una busta ".into() } else { "Diventano buste ".into() }, false));
        parts.push((".p7m".into(), true));
        parts.push((". ".into(), false));
    }
    for x in su.files.iter().filter(|x| matches!(x.action, Action::Parallel | Action::Counter | Action::PdfAgain)) {
        let stem = name_of(&x.path);
        let how = if x.action == Action::Counter { "sopra" } else { "accanto" };
        parts.push((format!("Su «{stem}» la tua firma va {how} a quella di {}. ", crate::firma::title_case(&names(&x.signers))), false));
    }
    // I file già firmati ricevono la firma in più nello stesso file; gli altri diventano file nuovi.
    let again = su.files.iter().filter(|x| matches!(x.action, Action::Parallel | Action::Counter | Action::PdfAgain)).count();
    let fresh = n - again;
    let dir = su.s.firma_dir.trim();
    let dest = match (fresh, again, dir.is_empty()) {
        (_, 0, true) => "Salvo i file firmati accanto agli originali, che non vengono toccati.".to_string(),
        (_, 0, false) => format!("Salvo i file firmati nella cartella {}.", dir),
        (0, 1, true) => "La firma si aggiunge nello stesso file: le firme di prima restano valide.".to_string(),
        (0, _, true) => "La firma si aggiunge negli stessi file: le firme di prima restano valide.".to_string(),
        (0, _, false) => format!("Salvo i file con la firma in più nella cartella {}; gli originali non vengono toccati.", dir),
        (_, _, true) => "I nuovi file firmati vanno accanto agli originali; in quelli già firmati la firma si aggiunge nello stesso file.".to_string(),
        (_, _, false) => format!("Salvo tutto nella cartella {}; gli originali non vengono toccati.", dir),
    };
    parts.push((dest, false));
    let text: String = parts.iter().map(|p| p.0.as_str()).collect();
    let tw = (r.2 - 50.0).min(720.0);
    let th = f.p.wrap_height(&text, 16.5, SANS, tw, 16.5 * 1.65, 8);
    // Stretto: il PIN e il pulsante vanno sotto la scritta.
    let narrow = r.2 < 860.0;
    let ch = 90.0 + th + 1.0 + 92.0 + if narrow { 74.0 } else { 0.0 };
    let rc = (r.0, y, r.2, ch);
    f.card(rc, 12.0);
    f.text("Controlla e firma", rc.0 + 24.0, rc.1 + 18.0, rc.2, 28.0, 17.0, SEMI, pal.text);
    rich(f, &parts, rc.0 + 24.0, rc.1 + 60.0, tw, 16.5, c);
    let py = rc.1 + 78.0 + th;
    f.hline(rc.0, py, rc.2);
    let pinpad = dev.is_some_and(|d| d.pin.pinpad);
    let label = if dev.is_some_and(|d| d.kind == pkcs11::DevKind::Card) { "PIN della carta" } else { "PIN della chiavetta" };
    f.text(label, rc.0 + 24.0, py + 22.0, 240.0, 24.0, 15.0, SEMI, pal.text);
    let tries = dev.map(|d| d.pin.tries_text()).unwrap_or("");
    let locked = dev.is_some_and(|d| d.pin.locked);
    let note = if n > 1 { format!("{tries} · vale per tutti i {n} file") } else { tries.to_string() };
    f.para(&note, rc.0 + 24.0, py + 50.0, if narrow { rc.2 - 48.0 } else { 262.0 }, 13.0, SANS, if locked || dev.is_some_and(|d| d.pin.count_low) { level(f, false) } else { pal.muted }, 1.35, 3);
    let gw = 190.0;
    let (fx, fy, fw) = if narrow { (rc.0 + 24.0, py + 94.0, rc.2 - 48.0 - gw - 14.0) } else { (rc.0 + 300.0, py + 20.0, (rc.2 - 300.0 - gw - 50.0).min(380.0)) };
    if pinpad {
        f.text("Scrivi il PIN sul tastierino del lettore quando te lo chiede.", fx, fy + 2.0, fw, 48.0, 14.5, SANS, pal.muted);
    } else {
        f.field((fx, fy, fw, 50.0), Field::FirmaPin, &su.pin, "Scrivi il PIN", 20.0, true);
    }
    let gl = if su.running { "Firmo…".to_string() } else { format!("Firma {nfile}") };
    go_button(f, (rc.0 + rc.2 - 24.0 - gw, fy, gw, 50.0), &gl, Some(ic::PEN), Act::FirmaGo, !su.running && !locked && (pinpad || !su.pin.is_empty()));
    let mut by = rc.1 + rc.3 + 18.0;
    if su.running {
        let done = su.files.iter().filter(|x| matches!(x.state, State::Done { .. })).count();
        f.spinner(r.0 + 14.0, by + 18.0, 10.0, c);
        f.animating = true;
        f.text(&format!("Firmo il file {} di {n}… non togliere la chiavetta.", (done + 1).min(n)), r.0 + 34.0, by, r.2, 36.0, 15.0, SANS, pal.text);
        by += 48.0;
    } else if let Some(e) = &su.error {
        let er = (r.0, by, r.2, 46.0);
        let bad = f.pal.bad;
        f.p.rrect(er.0, er.1, er.2, er.3, 8.0, Some(mix(bad, pal.surface, 0.88)), Some((mix(bad, pal.surface, 0.5), 1.0)));
        f.icon(ic::WARNING, er.0 + 14.0, er.1 + 12.0, 22.0, bad);
        f.text(&crate::i18n::lit(e), er.0 + 46.0, er.1, er.2 - 60.0, er.3, 15.0, SANS, pal.text);
        by += 60.0;
    }
    let bw = f.measure("Indietro", 15.5, SANS) + 36.0;
    if !su.running {
        f.button((r.0, by, bw, 44.0), "Indietro", None, 15.5, Act::FirmaStep(2));
    }
    by + 60.0 - r.1
}

fn level(f: &Frame, ok: bool) -> Rgb {
    if ok {
        f.pal.ok
    } else if f.theme.dark() {
        (0xF0, 0xB4, 0x4C)
    } else {
        (0xA8, 0x5A, 0x00)
    }
}

/// Un paragrafo con alcune parti in grassetto colorato (va a capo parola per parola).
fn rich(f: &mut Frame, parts: &[(String, bool)], x: f32, y: f32, w: f32, size: f32, c: Rgb) {
    let pal = f.pal;
    let lh = size * 1.65;
    let space = f.measure(" ", size, SANS);
    let (mut cx, mut cy) = (x, y);
    for (t, strong) in parts {
        let weight = if *strong { SEMI } else { SANS };
        let color = if *strong { c } else { pal.text };
        let words: Vec<&str> = t.split(' ').collect();
        for (k, wd) in words.iter().enumerate() {
            if !wd.is_empty() {
                let ww = f.measure(wd, size, weight);
                if cx + ww > x + w && cx > x {
                    cx = x;
                    cy += lh;
                }
                f.text(wd, cx, cy - size * 0.75, ww + 2.0, lh, size, weight, color);
                cx += ww;
            }
            if k + 1 < words.len() {
                cx += space;
            }
        }
    }
}

fn done(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let mut y = header(f, r, "Firma documenti");
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 34.0), 14.0);
    let ok = su.files.iter().filter(|x| matches!(x.state, State::Done { .. })).count();
    let failed = su.files.len() - ok;
    let okc = level(f, failed == 0);
    f.p.circle(r.0 + 28.0, y + 28.0, 28.0, Some(mix(okc, pal.surface, 0.86)), None);
    f.icon(if failed == 0 { ic::CHECK } else { ic::WARNING }, r.0 + 16.0, y + 16.0, 24.0, okc);
    let title = match (ok, failed) {
        (1, 0) => "1 file firmato".to_string(),
        (n, 0) => format!("{n} file firmati"),
        (n, k) => format!("{n} file firmati, {k} non riuscit{}", if k == 1 { "o" } else { "i" }),
    };
    f.text(&title, r.0 + 72.0, y + 2.0, r.2, 32.0, 22.0, SEMI, pal.text);
    let sub = su.device().map(|d| format!("Con la {} di {}", d.title(), d.owner())).unwrap_or_default();
    f.text(&sub, r.0 + 72.0, y + 32.0, r.2, 22.0, 14.5, SANS, pal.muted);
    y += 76.0;
    let row = 65.0;
    let list = (r.0, y, r.2, su.files.len() as f32 * row + 2.0);
    f.card(list, 12.0);
    for (k, sf) in su.files.iter().enumerate() {
        let ry = y + 1.0 + k as f32 * row;
        if k > 0 {
            f.hline(list.0, ry, list.2);
        }
        let shown = if matches!(sf.state, State::Done { .. }) { &sf.out } else { &sf.path };
        doc_chip(f, r.0 + 18.0, ry + 14.0, shown);
        f.text(&name_of(shown), r.0 + 70.0, ry + 10.0, r.2 - 200.0, 24.0, 15.5, SEMI, pal.text);
        let (line, word, good) = match &sf.state {
            State::Done { at, signers } if matches!(sf.action, Action::Parallel | Action::Counter | Action::PdfAgain) => (format!("ora ha {} firme: {}", signers.len(), names(signers)), "Fatto", true),
            State::Done { at, .. } => (format!("firmato alle {}", firma::date_time(*at).split(' ').nth(1).unwrap_or("")), "Fatto", true),
            State::Failed(e) => (e.clone(), "Non firmato", false),
            _ => ("non firmato".into(), "Non firmato", false),
        };
        f.text(&crate::i18n::lit(&line), r.0 + 70.0, ry + 34.0, r.2 - 200.0, 20.0, 13.5, SANS, pal.muted);
        let c = level(f, good);
        let c = if good { c } else { f.pal.bad };
        tag(f, r.0 + r.2 - 18.0, ry + row / 2.0, word, c);
    }
    y += list.3 + 22.0;
    let w1 = f.measure("Apri la cartella", 15.5, SEMI) + 58.0;
    f.primary((r.0, y, w1, 44.0), "Apri la cartella", Some(ic::FOLDER), 15.5, Act::FirmaDoneFolder, true);
    let w2 = f.measure("Guarda il file firmato", 15.5, SANS) + 58.0;
    if ok > 0 {
        f.button((r.0 + w1 + 12.0, y, w2, 44.0), "Guarda il file firmato", Some(ic::SHIELD), 15.5, Act::FirmaDoneView);
    }
    let t = "Torna all'inizio";
    let tw = f.measure(t, 15.5, SANS) + 24.0;
    let a = Act::FirmaPage(0);
    if f.hovered(&a) {
        f.p.rrect(r.0 + r.2 - tw, y, tw, 44.0, 8.0, Some(pal.alt), None);
    }
    f.text_c(t, (r.0 + r.2 - tw, y, tw, 44.0), 15.5, SANS, pal.text);
    f.hit((r.0 + r.2 - tw, y, tw, 44.0), a);
    y + 60.0 - r.1
}

/// Il dispositivo collegato in alto a destra («Aruba Key di MARIO ROSSI · pronta»). Restituisce la larghezza.
pub fn device_chip(f: &mut Frame, app: &App, x_right: f32, y: f32) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let (dot, strong, rest, state): (Rgb, String, String, String) = match su.chip_device() {
        Some(d) if d.signing_cert().is_some() => {
            let left = d.signing_cert().map(|c| (c.cert.not_after - firma::now()).div_euclid(86400)).unwrap_or(999);
            let st = if d.pin.locked {
                "PIN bloccato".to_string()
            } else if su.s.firma_remind && left <= 90 && left >= 0 {
                format!("scade tra {left} giorni")
            } else if left < 0 {
                "certificato scaduto".into()
            } else {
                "pronta".into()
            };
            let good = st == "pronta";
            (if good { pal.ok } else { level(f, false) }, d.title(), format!(" di {}", d.owner()), st)
        }
        Some(d) => (pal.muted, d.title(), String::new(), "solo accesso".into()),
        None if su.scanning || !su.scanned_once => (pal.muted, "Cerco la chiavetta…".into(), String::new(), String::new()),
        None => (pal.muted, "Nessuna chiavetta".into(), String::new(), "collegala".into()),
    };
    let sw = f.measure(&strong, 14.0, SEMI);
    let rw = f.measure(&rest, 14.0, SANS);
    let tw = f.measure(&state, 13.0, SANS);
    let w = 14.0 + 10.0 + 8.0 + sw + rw + 10.0 + tw + 16.0;
    let rc = (x_right - w, y, w, 34.0);
    let act = Act::FirmaPage(2);
    f.p.rrect(rc.0, rc.1, rc.2, rc.3, 17.0, Some(if f.hovered(&act) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
    f.p.circle(rc.0 + 21.0, rc.1 + 17.0, 5.0, Some(dot), Some((mix(dot, pal.surface, 0.6), 2.0)));
    let mut x = rc.0 + 34.0;
    f.text(&strong, x, rc.1, sw + 2.0, rc.3, 14.0, SEMI, pal.text);
    x += sw;
    f.text(&rest, x, rc.1, rw + 2.0, rc.3, 14.0, SANS, pal.text);
    x += rw + 10.0;
    f.text(&state, x, rc.1, tw + 2.0, rc.3, 13.0, SANS, if dot == pal.ok || dot == pal.muted { pal.muted } else { dot });
    f.hit(rc, act);
    w
}

// ---------------------------------------------------------------- chiavetta e certificati

fn devices(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let c = accent(f);
    let mut y = header(f, r, "Chiavetta e certificati");
    let rs = Act::FirmaScan;
    let rl = if su.scanning { "Cerco…" } else { "Cerca di nuovo" };
    let rw = f.measure(rl, 15.0, SANS) + 44.0;
    if f.hovered(&rs) {
        f.p.rrect(r.0 + r.2 - rw, r.1 + 40.0, rw, 34.0, 6.0, Some(pal.alt), None);
    }
    f.icon(ic::REFRESH, r.0 + r.2 - rw + 10.0, r.1 + 47.0, 20.0, pal.text);
    f.text(rl, r.0 + r.2 - rw + 38.0, r.1 + 40.0, rw, 34.0, 15.0, SANS, pal.text);
    f.hit((r.0 + r.2 - rw, r.1 + 40.0, rw, 34.0), rs);
    if let Some((t, bad)) = &su.info {
        let c2 = if *bad { pal.bad } else { pal.ok };
        let ir = (r.0 + r.2 * 0.42, r.1 + 40.0, r.2 * 0.58 - rw - 16.0, 34.0);
        f.p.rrect(ir.0, ir.1, ir.2, ir.3, 8.0, Some(mix(c2, pal.surface, 0.88)), None);
        f.text(&crate::i18n::lit(t), ir.0 + 12.0, ir.1, ir.2 - 44.0, ir.3, 13.5, SANS, pal.text);
        f.icon(ic::CLOSE, ir.0 + ir.2 - 28.0, ir.1 + 9.0, 16.0, pal.muted);
        f.hit((ir.0 + ir.2 - 34.0, ir.1, 34.0, 34.0), Act::FirmaInfoClose);
    }
    let gap = 14.0;
    let narrow = r.2 < 900.0;
    let cols = if narrow { 2 } else { 3 };
    let cw = (r.2 - (cols - 1) as f32 * gap) / cols as f32;
    if su.devices.is_empty() {
        let rc = (r.0, y, r.2, 110.0);
        f.card(rc, 12.0);
        if su.scanning || !su.scanned_once {
            f.spinner(rc.0 + 40.0, rc.1 + 55.0, 12.0, c);
            f.animating = true;
            f.text("Cerco la chiavetta e le smart card…", rc.0 + 70.0, rc.1, rc.2, rc.3, 16.0, SANS, pal.text);
        } else {
            f.text("Nessuna chiavetta né smart card collegata", rc.0 + 24.0, rc.1 + 18.0, rc.2, 28.0, 16.5, SEMI, pal.text);
            let drivers = pkcs11::driver_candidates();
            let t = if drivers.is_empty() {
                "Non trovo il driver della chiavetta: installa quello del tuo certificatore (per la Aruba Key: bit4id) o sceglilo a mano in Opzioni › Driver della chiavetta.".to_string()
            } else {
                format!("Driver trovati: {}. Collega la chiavetta o inserisci la carta e premi «Cerca di nuovo».", drivers.iter().map(|d| pkcs11::driver_name(d)).collect::<Vec<_>>().join(", "))
            };
            f.para(&t, rc.0 + 24.0, rc.1 + 50.0, rc.2 - 48.0, 13.5, SANS, pal.muted, 1.4, 3);
        }
        return y + rc.3 + 20.0 - r.1;
    }
    for (k, d) in su.devices.iter().enumerate() {
        let rc = (r.0 + (k % cols) as f32 * (cw + gap), y + (k / cols) as f32 * 124.0, cw, 110.0);
        let on = k == su.device;
        let act = Act::FirmaDevice(k);
        if on {
            f.p.rrect(rc.0, rc.1, rc.2, rc.3, 12.0, Some(pal.surface), Some((pal.primary, 2.0)));
        } else {
            f.card(rc, 12.0);
        }
        let icon = if d.kind == pkcs11::DevKind::Card { CARD } else { USB };
        f.p.rrect(rc.0 + 16.0, rc.1 + 16.0, 44.0, 44.0, 10.0, Some(mix(pal.primary, pal.surface, 0.88)), None);
        f.icon(icon, rc.0 + 27.0, rc.1 + 27.0, 22.0, pal.primary);
        f.text(&d.title(), rc.0 + 72.0, rc.1 + 14.0, rc.2 - 84.0, 26.0, 16.5, SEMI, pal.text);
        let sub = if d.kind == pkcs11::DevKind::Card { "Lettore di smart card" } else { "Chiavetta USB" };
        f.text(sub, rc.0 + 72.0, rc.1 + 38.0, rc.2 - 84.0, 20.0, 13.0, SANS, pal.muted);
        let (word, good) = if d.signing_cert().is_some() {
            (if d.kind == pkcs11::DevKind::Card { "Inserita · firma" } else { "Collegata · firma" }, true)
        } else {
            (if d.kind == pkcs11::DevKind::Card { "Inserita · solo accesso" } else { "Collegata · solo accesso" }, false)
        };
        let col = if good { pal.ok } else { pal.muted };
        let ww = f.measure(word, 13.0, SEMI) + 20.0 + if good { 12.0 } else { 0.0 };
        f.p.rrect(rc.0 + 16.0, rc.1 + 72.0, ww, 24.0, 12.0, Some(mix(col, pal.surface, 0.86)), None);
        if good {
            f.p.circle(rc.0 + 28.0, rc.1 + 84.0, 4.0, Some(col), None);
        }
        f.text(word, rc.0 + 26.0 + if good { 12.0 } else { 0.0 }, rc.1 + 72.0, ww, 24.0, 13.0, SEMI, if good { col } else { pal.text });
        f.hit(rc, act);
    }
    y += su.devices.len().div_ceil(cols) as f32 * 124.0 + 6.0;
    let Some(d) = su.device() else { return y - r.1 };
    // A sinistra i certificati, a destra PIN e sblocco (stretto: uno sotto l'altro).
    let lw = if narrow { r.2 } else { (r.2 - gap) * 0.66 };
    let rx = if narrow { r.0 } else { r.0 + lw + gap };
    let rwid = if narrow { r.2 } else { r.2 - lw - gap };
    let ch = 152.0;
    let certs: Vec<&pkcs11::DevCert> = d.certs.iter().filter(|c| !c.cert.is_ca).collect();
    let lh = certs.len().max(1) as f32 * ch;
    let lc = (r.0, y, lw, lh);
    f.card(lc, 12.0);
    if certs.is_empty() {
        f.text("Sul dispositivo non ci sono certificati personali.", lc.0 + 18.0, lc.1, lc.2, ch, 15.0, SANS, pal.muted);
    }
    let now = firma::now();
    for (k, dc) in certs.iter().enumerate() {
        let cy = y + k as f32 * ch;
        if k > 0 {
            f.hline(lc.0, cy, lc.2);
        }
        let title = if dc.signing { "Certificato di firma" } else if d.kind == pkcs11::DevKind::Card { "Certificato di accesso (CNS)" } else { "Certificato di accesso" };
        let tw = f.measure(title, 16.5, SEMI);
        f.text(title, lc.0 + 18.0, cy + 14.0, tw + 4.0, 28.0, 16.5, SEMI, pal.text);
        let tagt = if dc.signing { "serve per firmare" } else { "serve per entrare nei siti" };
        let tagw = f.measure(tagt, 13.0, SEMI) + 20.0;
        let tc = if dc.signing { c } else { pal.muted };
        f.p.rrect(lc.0 + 30.0 + tw, cy + 16.0, tagw, 24.0, 12.0, Some(mix(tc, pal.surface, 0.86)), None);
        f.text_c(tagt, (lc.0 + 30.0 + tw, cy + 16.0, tagw, 24.0), 13.0, SEMI, tc);
        let cf = dc.cert.tax_code().map(|t| format!(" · {t}")).unwrap_or_default();
        let line = format!("{}{cf} · rilasciato da {}", dc.cert.subject.person(), dc.cert.issuer.get(crate::firma::oid::ORG).unwrap_or(dc.cert.issuer.common_name()));
        f.text(&crate::i18n::lit(&line), lc.0 + 18.0, cy + 44.0, lc.2 - 36.0, 22.0, 13.5, SANS, pal.muted);
        // La barra della validità.
        let (a, b) = (dc.cert.not_before, dc.cert.not_after);
        let frac = ((now - a) as f32 / (b - a).max(1) as f32).clamp(0.0, 1.0);
        let left = (b - now).div_euclid(86400);
        let warn = left < 180;
        let bc = if left < 0 { pal.bad } else if warn { level(f, false) } else { pal.ok };
        let end = if left < 0 { format!("scaduto il {}", firma::date(b)) } else if warn { format!("scade il {}, tra {left} giorni", firma::date(b)) } else { format!("scade il {}", firma::date(b)) };
        let ew = f.measure(&end, if warn { 15.0 } else { 13.5 }, if warn { SEMI } else { SANS });
        let by = cy + 72.0;
        f.text(&firma::date(a), lc.0 + 18.0, by, 90.0, 22.0, 13.5, SANS, pal.muted);
        let dw = f.measure(&firma::date(a), 13.5, SANS);
        let bx0 = lc.0 + 18.0 + dw + 12.0;
        let bw = lc.2 - (bx0 - lc.0) - ew - 30.0;
        f.p.rrect(bx0, by + 8.0, bw, 6.0, 3.0, Some(pal.border), None);
        f.p.rrect(bx0, by + 8.0, bw * frac, 6.0, 3.0, Some(bc), None);
        f.text_r(&end, (lc.0, by, lc.2 - 18.0, 22.0), if warn { 15.0 } else { 13.5 }, if warn { SEMI } else { SANS }, if warn { bc } else { pal.muted });
        let idx = d.certs.iter().position(|x| x.der == dc.der).unwrap_or(0);
        let mut bx = lc.0 + 18.0;
        for (label, act) in [("Vedi il certificato", Act::FirmaCertView(idx)), ("Controlla se è revocato", Act::FirmaCertRevoca(idx)), ("Salva come .cer", Act::FirmaCertSave(idx))] {
            let w = f.measure(label, 13.5, SANS) + 24.0;
            f.button((bx, cy + 104.0, w, 32.0), label, None, 13.5, act);
            bx += w + 8.0;
        }
    }
    // PIN e sblocco.
    let pc = (rx, if narrow { lc.1 + lc.3 + 14.0 } else { y }, rwid, 212.0);
    f.card(pc, 12.0);
    f.text("PIN e sblocco", pc.0 + 18.0, pc.1 + 14.0, pc.2, 28.0, 16.5, SEMI, pal.text);
    let pin_c = if d.pin.locked || d.pin.count_low { level(f, false) } else { pal.muted };
    f.text(d.pin.tries_text(), pc.0 + 18.0, pc.1 + 42.0, pc.2 - 36.0, 22.0, 13.5, SANS, pin_c);
    for (k, (label, icon)) in [("Cambia il PIN", KEY), ("Sblocca con il PUK", UNLOCK), ("Cambia il PUK", KEY)].iter().enumerate() {
        let ry = pc.1 + 72.0 + k as f32 * 46.0;
        if k > 0 {
            f.hline(pc.0 + 18.0, ry, pc.2 - 36.0);
        }
        let act = Act::FirmaDlg(k as u8);
        if f.hovered(&act) {
            f.p.fill(pc.0 + 8.0, ry + 2.0, pc.2 - 16.0, 42.0, pal.alt);
        }
        f.icon(icon, pc.0 + 18.0, ry + 12.0, 20.0, pal.text);
        f.text(label, pc.0 + 48.0, ry, pc.2 - 60.0, 46.0, 15.0, SANS, pal.text);
        f.hit((pc.0 + 8.0, ry, pc.2 - 16.0, 46.0), act);
    }
    let mut ry = pc.1 + pc.3 + 14.0;
    if let Some(dl) = &su.dialog {
        ry = dialog(f, (rx, ry, rwid, 0.0), dl) + 14.0;
    }
    let ac = (rx, ry, rwid, 76.0);
    f.card(ac, 12.0);
    f.text("Avvisami prima che scada", ac.0 + 18.0, ac.1 + 14.0, ac.2 - 90.0, 24.0, 15.0, SANS, pal.text);
    f.text("Te lo dico quando mancano 3 mesi o meno", ac.0 + 18.0, ac.1 + 38.0, ac.2 - 90.0, 20.0, 13.0, SANS, pal.muted);
    f.switch(ac.0 + ac.2 - 18.0 - 46.0, ac.1 + 25.0, 46.0, 26.0, su.s.firma_remind, Act::FirmaOpt(2), None);
    let bottom = (lc.1 + lc.3).max(ac.1 + ac.3);
    bottom + 20.0 - r.1
}

/// La finestrella per PIN e PUK, sotto «PIN e sblocco». Restituisce dove finisce.
fn dialog(f: &mut Frame, r: R, d: &crate::ui::firmare::PinDialog) -> f32 {
    let pal = f.pal;
    let (title, a, b) = match d.kind {
        0 => ("Cambia il PIN", "PIN attuale", "Nuovo PIN"),
        1 => ("Sblocca con il PUK", "PUK (nella busta del certificatore)", "Nuovo PIN"),
        _ => ("Cambia il PUK", "PUK attuale", "Nuovo PUK"),
    };
    let h = 54.0 + 3.0 * 62.0 + 58.0 + if d.error.is_some() { 30.0 } else { 0.0 };
    let rc = (r.0, r.1, r.2, h);
    f.p.rrect(rc.0, rc.1, rc.2, rc.3, 12.0, Some(pal.surface), Some((pal.primary, 1.5)));
    f.text(title, rc.0 + 18.0, rc.1 + 12.0, rc.2, 30.0, 16.5, SEMI, pal.text);
    let mut y = rc.1 + 50.0;
    for (k, (label, val)) in [(a, &d.old), (b, &d.new), ("Scrivilo di nuovo", &d.again)].iter().enumerate() {
        f.text(label, rc.0 + 18.0, y, rc.2 - 36.0, 20.0, 13.0, SANS, pal.muted);
        f.field((rc.0 + 18.0, y + 22.0, rc.2 - 36.0, 36.0), Field::FirmaDlg(k as u8), val, "", 16.0, true);
        y += 62.0;
    }
    if let Some(e) = &d.error {
        f.text(&crate::i18n::lit(e), rc.0 + 18.0, y, rc.2 - 36.0, 26.0, 13.5, SEMI, pal.bad);
        y += 30.0;
    }
    let bw = f.measure("Annulla", 14.5, SANS) + 30.0;
    f.button((rc.0 + 18.0, y + 6.0, bw, 38.0), "Annulla", None, 14.5, Act::FirmaDlgClose);
    let gw = rc.2 - 36.0 - bw - 10.0;
    f.primary((rc.0 + 28.0 + bw, y + 6.0, gw, 38.0), if d.busy { "Un momento…" } else { "Conferma" }, None, 14.5, Act::FirmaDlgGo, !d.busy);
    rc.1 + rc.3
}

// ---------------------------------------------------------------- opzioni

fn opt_row(f: &mut Frame, x: f32, y: f32, w: f32, title: &str, note: &str, first: bool) {
    let pal = f.pal;
    if !first {
        f.hline(x, y, w);
    }
    let has = !note.is_empty();
    f.text(title, x, y + if has { 8.0 } else { 0.0 }, w * 0.62, if has { 24.0 } else { 54.0 }, 15.0, SANS, pal.text);
    if has {
        f.text(&crate::i18n::lit(note), x, y + 30.0, w * 0.68, 20.0, 13.0, SANS, pal.muted);
    }
}

fn opt_card(f: &mut Frame, rc: R, icon: &str, ic_color: Rgb, title: &str) {
    let pal = f.pal;
    f.card(rc, 12.0);
    f.icon(icon, rc.0 + 18.0, rc.1 + 19.0, 20.0, ic_color);
    f.text(title, rc.0 + 46.0, rc.1 + 12.0, rc.2 - 60.0, 34.0, 16.5, SEMI, pal.text);
}

fn options(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let su = &app.firma.sign;
    let s = &su.s;
    let fu = &app.firma;
    let y = header(f, r, "Opzioni della firma");
    super::notice(f, app, (r.0 + r.2 - 640.0, r.1 - 2.0, 640.0, 34.0), 14.0);
    let gap = 16.0;
    let narrow = r.2 < 900.0;
    let cw = if narrow { r.2 } else { (r.2 - gap) / 2.0 };
    let rh = 55.0;
    let pad = 18.0;
    // Quando firmo.
    let a = (r.0, y, cw, 52.0 + 3.0 * rh + 10.0);
    opt_card(f, a, ic::PEN, accent(f), "Quando firmo");
    let (x, w) = (a.0 + pad, a.2 - 2.0 * pad);
    let mut ry = a.1 + 52.0;
    let fmt = ["Automatico", "Tutto .p7m", "Tutto PDF"][s.firma_format.min(2) as usize];
    let fmt_note = ["PDF in PDF firmato, gli altri in .p7m", "Busta firmata .p7m per tutti i file", "I file non PDF li trasformo in PDF"][s.firma_format.min(2) as usize];
    opt_row(f, x, ry, w, "Formato", fmt_note, true);
    small_button(f, x + w, ry + rh / 2.0, fmt, Act::FirmaFormatMenu);
    ry += rh;
    let suffix = if s.firma_suffix.is_empty() { "_firmato" } else { s.firma_suffix.as_str() };
    opt_row(f, x, ry, w, "Nome dei file firmati", &format!("Determina{suffix}.pdf · Lettera.docx.p7m"), false);
    let fw = 130.0;
    f.field((x + w - fw, ry + 10.0, fw, 34.0), Field::FirmaSuffix, &s.firma_suffix, "_firmato", 14.0, false);
    ry += rh;
    opt_row(f, x, ry, w, "Tieni il PIN per tutta la coda", if s.firma_keep_pin { "Lo dimentico appena ho finito" } else { "Lo ripasso alla chiavetta per ogni file" }, false);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, s.firma_keep_pin, Act::FirmaOpt(0), None);
    // Firma visibile nei PDF.
    let b = if narrow { (r.0, a.1 + a.3 + gap, cw, a.3) } else { (r.0 + cw + gap, y, cw, a.3) };
    opt_card(f, b, ic::IMAGE, (0xD6, 0x3A, 0x2F), "Firma visibile nei PDF");
    let (x, w) = (b.0 + pad, b.2 - 2.0 * pad);
    let mut ry = b.1 + 52.0;
    opt_row(f, x, ry, w, "Si vede sul foglio", if s.firma_visible { "Un riquadro con nome, data e ora" } else { "La firma c'è e vale, ma non compare" }, true);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, s.firma_visible, Act::FirmaOpt(1), None);
    ry += rh;
    let img = if s.firma_image.trim().is_empty() { "Niente".to_string() } else { std::path::Path::new(s.firma_image.trim()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
    opt_row(f, x, ry, w, "Immagine", "Stemma, firma autografa scansionata, o niente", false);
    let iw = f.measure(&img, 13.5, SANS).min(160.0);
    let _ = iw;
    small_button(f, x + w, ry + rh / 2.0, &crate::i18n::lit(&img), Act::FirmaImageMenu);
    ry += rh;
    let corner = ["Ultima pagina, in basso a destra", "Ultima pagina, in basso a sinistra", "Prima pagina, in basso a destra"][s.firma_corner.min(2) as usize];
    opt_row(f, x, ry, w, "Posizione se il PDF non ha lo spazio", corner, false);
    small_button(f, x + w, ry + rh / 2.0, "Cambia", Act::FirmaCornerMenu);
    // Visualizzatore e verifica.
    let y2 = b.1 + b.3 + gap;
    let c = (r.0, y2, cw, 52.0 + 4.0 * rh + 10.0);
    opt_card(f, c, ic::SHIELD, pal.ok, "Visualizzatore e verifica");
    let (x, w) = (c.0 + pad, c.2 - 2.0 * pad);
    let (assoc, pdf, online) = fu.opts;
    let mut ry = c.1 + 52.0;
    opt_row(f, x, ry, w, "Apri con Cedro i file firmati", "Doppio clic su .p7m, .p7s, .m7m, .tsd e .tsr", true);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, assoc, Act::FirmaSetting(0), None);
    ry += rh;
    opt_row(f, x, ry, w, "Anche i PDF firmati", "Se no restano al tuo lettore PDF", false);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, pdf, Act::FirmaSetting(1), None);
    ry += rh;
    let (updated, count) = fu.tl_info;
    let tl = if fu.tl_busy || firma::trust::updating() {
        "Lo sto scaricando…".to_string()
    } else {
        match updated {
            Some(u) => format!("Aggiornato {} · {count} certificatori", firma::relative(u)),
            None => "Non ancora scaricato".into(),
        }
    };
    opt_row(f, x, ry, w, "Elenco europeo dei certificatori", &tl, false);
    small_button(f, x + w, ry + rh / 2.0, "Aggiorna ora", Act::FirmaTrust);
    ry += rh;
    opt_row(f, x, ry, w, "Controlla online la revoca", "Chiedo al certificatore se il certificato è stato revocato", false);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, online, Act::FirmaSetting(2), None);
    // La chiavetta.
    let d = if narrow { (r.0, y2 + 52.0 + 4.0 * rh + 10.0 + gap, cw, 52.0 + 2.0 * rh + 10.0) } else { (r.0 + cw + gap, y2, cw, 52.0 + 2.0 * rh + 10.0) };
    opt_card(f, d, ic::LINK, (0x0E, 0x7C, 0x86), "Chiavetta");
    let (x, w) = (d.0 + pad, d.2 - 2.0 * pad);
    let mut ry = d.1 + 52.0;
    let found = pkcs11::driver_candidates();
    let drv = if !s.firma_driver.trim().is_empty() {
        format!("Scelto a mano: {}", pkcs11::driver_name(std::path::Path::new(s.firma_driver.trim())))
    } else if found.is_empty() {
        "Non trovato: installa quello del certificatore".into()
    } else {
        format!("Trovato da solo: {}", found.iter().map(|p| pkcs11::driver_name(p)).collect::<Vec<_>>().join(", "))
    };
    opt_row(f, x, ry, w, "Driver della chiavetta", &drv, true);
    small_button(f, x + w, ry + rh / 2.0, if s.firma_driver.trim().is_empty() { "A mano" } else { "Automatico" }, Act::FirmaDriver);
    ry += rh;
    opt_row(f, x, ry, w, "Avvisami prima che scada il certificato", "Te lo dico quando mancano 3 mesi o meno", false);
    f.switch(x + w - 46.0, ry + 14.0, 46.0, 26.0, s.firma_remind, Act::FirmaOpt(2), None);
    (c.1 + c.3).max(d.1 + d.3) + 20.0 - r.1
}
