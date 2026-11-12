//! Cedro 4 Labs: cornice scura, le pagine del laboratorio a sinistra, il file al centro sopra un
//! alone dei suoi stessi colori e a destra un pannello chiaro con i comandi veri del motore.
//! «Semplice» mostra solo le scelte pronte, «Completa» tutte le opzioni. Misure su 1536×1024:
//! barra e pannello restano, il centro si allarga.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::*;
use crate::ui::core::Scroll;
use crate::ui::gfx::{Fit, Image, Rgb};
use crate::ui::model::{FileState, Lab};
use crate::ui::theme::{lab_colors, LABS_FRAME, LABS_FRAME_HI, LABS_INK, LABS_LEAF, LABS_MUTED, LABS_STAGE};
use crate::ui::voci;

const TOP: f32 = 64.0;
const RAIL: f32 = 300.0;
const PAD: f32 = 12.0;
const SIDE: f32 = 412.0;
const STRIP: f32 = 96.0;
const WHITE: Rgb = (255, 255, 255);

const OPT_LABS: OptStyle = OptStyle { label: 15.0, note: 14.0, control_h: 42.0, gap: 20.0, visible: 0, switch_w: 52.0, switch_h: 28.0 };

const ICON_HOME: &str = "M4 11 12 4l8 7v9h-5v-6H9v6H4z";
const ICON_FIGURE: &str = "M3 4h8v7H3z M13 4h8v7h-8z M3 13h18v7H3z M14 17h5";
const ICON_TABLE: &str = "M3 5h18v14H3z M3 10h18 M9 5v14";

fn lab_icon(lab: Lab) -> &'static str {
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

/// Le sei immagini di «Inizio» (disegnate per Cedro, 480×360).
fn tile_image(i: usize) -> Option<Image> {
    static IMAGES: OnceLock<Vec<Option<Image>>> = OnceLock::new();
    const DATA: [&[u8]; 6] = [
        include_bytes!("../../../assets/labs/vetrino.jpg"),
        include_bytes!("../../../assets/labs/radiografia.jpg"),
        include_bytes!("../../../assets/labs/microscopia.jpg"),
        include_bytes!("../../../assets/labs/tabella.jpg"),
        include_bytes!("../../../assets/labs/referto.jpg"),
        include_bytes!("../../../assets/labs/audio.jpg"),
    ];
    IMAGES
        .get_or_init(|| {
            DATA.iter()
                .map(|d| {
                    let img = image::load_from_memory(d).ok()?.to_rgba8();
                    Image::from_rgba(img.width(), img.height(), img.as_raw())
                })
                .collect()
        })
        .get(i)
        .cloned()
        .flatten()
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

/// La pagina su cui si sta lavorando (per i colori): a «Inizio» con un file quella del marchio.
fn colors(app: &App) -> (Rgb, Rgb) {
    lab_colors(app.lab)
}

fn frame(f: &mut Frame, app: &App) {
    f.p.clear(LABS_FRAME);
    f.p.rrect_vgrad(0.0, 0.0, f.w, (f.h * 0.5).round(), 0.0, LABS_FRAME_HI, LABS_FRAME);
    f.caption((0.0, 0.0, f.w, TOP));
    f.p.symbol(24.0, 17.0, 30.0);
    f.p.text("Cedro", 64.0, 17.0, 120.0, 30.0, 22.0, BOLD, LABS_INK, Align::Left);
    let cw = f.measure("Cedro", 22.0, BOLD);
    f.p.text("4 Labs", 64.0 + cw + 8.0, 17.0, 120.0, 30.0, 22.0, BOLD, LABS_LEAF, Align::Left);
    f.window_controls(f.w, 0.0, 48.0, LABS_INK, LABS_FRAME_HI);
    // Cedro lavora tutto sul computer: lo si dice in alto, dove in laboratorio lo si cerca.
    let local = "Niente esce da questo PC";
    let lw = f.measure(local, 15.0, SEMI);
    let lx = f.w - 3.0 * 46.0 - 26.0 - lw;
    f.p.disc_a(lx - 16.0, 32.0, 9.0, LABS_LEAF, 0.22);
    f.p.circle(lx - 16.0, 32.0, 5.0, Some(LABS_LEAF), None);
    f.p.text(local, lx, 17.0, lw + 4.0, 30.0, 15.0, SEMI, LABS_LEAF, Align::Left);

    let mut y = TOP + 4.0;
    rail_item(f, y, Lab::Home, Lab::Home.name(), app.lab == Lab::Home, Act::Lab(Lab::Home));
    y += 54.0;
    f.p.text("Laboratorio", 24.0, y + 8.0, RAIL - 40.0, 26.0, 14.5, SEMI, LABS_MUTED, Align::Left);
    y += 38.0;
    for lab in Lab::LAB {
        rail_item(f, y, lab, lab.name(), app.lab == lab, Act::Lab(lab));
        y += 54.0;
    }
    f.p.text("Cedro di sempre", 24.0, y + 8.0, RAIL - 40.0, 26.0, 14.5, SEMI, LABS_MUTED, Align::Left);
    y += 38.0;
    for lab in Lab::STANDARD {
        rail_item(f, y, lab, lab.name(), app.lab == lab, Act::Lab(lab));
        y += 54.0;
    }
    for (k, (label, icon, act)) in [("Impostazioni", ic::SETTINGS, Act::OpenSettings), ("Guida", ic::HELP, Act::OpenGuide)].into_iter().enumerate() {
        let r = (12.0, f.h - PAD - 88.0 + k as f32 * 44.0, RAIL - 24.0, 40.0);
        let t = f.hover_t(&act, true);
        if t > 0.01 {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, WHITE, 0.07 * t);
        }
        f.icon(icon, r.0 + 14.0, r.1 + 9.0, 22.0, LABS_MUTED);
        f.p.text(label, r.0 + 50.0, r.1, r.2 - 56.0, r.3, 16.0, SANS, LABS_MUTED, Align::Left);
        f.focus_ring(r, 10.0, &act);
        f.hit(r, act);
    }
}

/// Una voce della barra: l'icona su una piastrella sfumata del suo colore, e la voce scelta che si
/// accende di quel colore.
fn rail_item(f: &mut Frame, y: f32, lab: Lab, label: &str, on: bool, act: Act) {
    let (deep, bright) = lab_colors(lab);
    let r = (12.0, y, RAIL - 24.0, 50.0);
    if on {
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(bright, LABS_FRAME, 0.74)), Some((mix(bright, LABS_FRAME, 0.45), 1.0)));
    } else {
        let t = f.hover_t(&act, true);
        if t > 0.01 {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 12.0, WHITE, 0.07 * t);
        }
    }
    let (tx, ty, ts) = (r.0 + 8.0, y + 8.0, 34.0);
    f.p.shadow(tx, ty, ts, ts, 10.0, 5.0, 0.3, 2.0);
    f.p.rrect_vgrad(tx, ty, ts, ts, 10.0, bright, deep);
    f.p.rrect_line(tx, ty, ts, ts, 10.0, WHITE, 1.0, 0.22, None);
    f.icon(lab_icon(lab), tx + 7.0, ty + 7.0, 20.0, WHITE);
    let c = if on { WHITE } else { LABS_INK };
    // Si traduce prima di andare a capo: le righe spezzate non si ritroverebbero nella tabella.
    let label = &crate::i18n::tr_string(label);
    let (lx, lw) = (r.0 + 54.0, r.2 - 62.0);
    let size = 16.0;
    if f.measure(label, size, SEMI) <= lw {
        f.p.text(label, lx, y, lw, r.3, size, SEMI, c, Align::Left);
    } else {
        // Le voci lunghe vanno su due righe invece di finire con i puntini.
        let lines = f.p.wrap_lines(label, size, SEMI, lw, 2);
        let lh = 19.0;
        let top = y + (r.3 - lh * lines.len() as f32) / 2.0;
        for (k, line) in lines.iter().enumerate() {
            f.p.text(line, lx, top + k as f32 * lh, lw, lh, size, SEMI, c, Align::Left);
        }
    }
    f.focus_ring(r, 12.0, &act);
    f.hit(r, act);
}

pub fn draw(f: &mut Frame, app: &App) {
    frame(f, app);
    let area = (RAIL, TOP, f.w - RAIL - PAD, f.h - TOP - PAD);
    match app.lab {
        Lab::Home if app.files.is_empty() => home(f, app, area),
        Lab::Home => {
            let stage = (area.0, area.1, area.2 - SIDE - PAD, area.3);
            stage_draw(f, app, stage);
            file_card(f, app, (area.0 + area.2 - SIDE, area.1, SIDE, area.3));
        }
        Lab::Referti | Lab::Trascrivi => page(f, app, area),
        _ => {
            let stage = (area.0, area.1, area.2 - SIDE - PAD, area.3);
            stage_draw(f, app, stage);
            side_panel(f, app, (area.0 + area.2 - SIDE, area.1, SIDE, area.3));
        }
    }
}

/// «Firma e manda referti» e «Trascrivi e riassumi»: le pagine di Cedro su una scheda chiara.
fn page(f: &mut Frame, app: &App, area: R) {
    let pal = f.pal;
    f.p.rrect(area.0, area.1, area.2, area.3, 16.0, Some(pal.work), None);
    let mut r = (area.0 + 28.0, area.1 + 22.0, area.2 - 56.0, area.3 - 40.0);
    if app.lab == Lab::Referti {
        // Le altre cose che si fanno con un referto stanno in altre pagine: qui le scorciatoie.
        let mut x = r.0;
        f.text("Anche:", x, r.1, 70.0, 40.0, 15.0, SEMI, pal.muted);
        x += f.measure("Anche:", 15.0, SEMI) + 14.0;
        for (label, lab, id) in [("Radiografie per il proprietario", Lab::Immagini, "rx"), ("Pacchetto per un consulto", Lab::Immagini, "consult"), ("PDF in Word", Lab::Pdf, "docx")] {
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

fn home(f: &mut Frame, app: &App, area: R) {
    let x = area.0 + 12.0;
    let w = area.2 - 24.0;
    f.p.text("Che file hai?", x, area.1 + 6.0, w, 50.0, 38.0, BOLD, LABS_INK, Align::Left);
    f.p.text("Trascinalo qui dentro, oppure scegli il tipo. Se sai già cosa fare, parti dalle voci a sinistra.", x, area.1 + 58.0, w, 28.0, 17.0, SANS, LABS_MUTED, Align::Left);
    let tiles: [(&str, &str, Lab); 6] = [
        ("Vetrino", "SVS", Lab::Immagini),
        ("Radiografia", "DICOM", Lab::Immagini),
        ("Microscopia", "ND2, LIF, TIFF a 16 bit", Lab::Immagini),
        ("Tabella", "CSV, Excel", Lab::Dati),
        ("Referto", "PDF, Word", Lab::Referti),
        ("Audio", "Dettato o riunione", Lab::Trascrivi),
    ];
    let gap = 18.0;
    let top = area.1 + 110.0;
    let tw = ((w - 2.0 * gap) / 3.0).floor();
    let th = ((area.3 - 110.0 - 64.0 - gap) / 2.0).clamp(150.0, 280.0).floor();
    for (i, (name, kinds, lab)) in tiles.into_iter().enumerate() {
        let (deep, _) = lab_colors(lab);
        let act = Act::LabGo(Lab::Home, kinds);
        let t = f.hover_t(&act, true);
        let tx = x + (i % 3) as f32 * (tw + gap);
        let ty = top + (i / 3) as f32 * (th + gap) - 4.0 * t;
        f.p.shadow(tx, ty, tw, th, 16.0, 12.0 + 6.0 * t, 0.45, 8.0);
        f.p.rrect(tx, ty, tw, th, 16.0, Some(LABS_STAGE), None);
        if let Some(img) = tile_image(i) {
            f.p.image(&img, tx, ty, tw, th, 16.0, Fit::Cover);
        }
        // La didascalia sfuma nel colore del tipo di file: fasce sempre più piene, poi il fondo arrotondato.
        let c = mix(deep, (0, 0, 0), 0.06);
        let (g0, g1) = (ty + th * 0.42, ty + th - 16.0);
        let bands = 28;
        let bh = (g1 - g0) / bands as f32;
        for k in 0..bands {
            let a = ((k + 1) as f32 / bands as f32).powf(1.5) * 0.96;
            f.p.fill_a(tx, g0 + k as f32 * bh, tw, bh + 0.6, c, a);
        }
        f.p.push_clip(tx, ty + th - 16.0, tw, 16.0);
        f.p.rrect_a(tx, ty + th - 48.0, tw, 48.0, 16.0, c, 0.96);
        f.p.pop_clip();
        f.p.rrect_line(tx, ty, tw, th, 16.0, WHITE, 1.0, 0.14, None);
        f.p.text(name, tx + 20.0, ty + th - 66.0, tw - 40.0, 32.0, 25.0, BOLD, WHITE, Align::Left);
        f.p.text(kinds, tx + 20.0, ty + th - 36.0, tw - 40.0, 22.0, 15.0, SEMI, WHITE, Align::Left);
        f.focus_ring((tx, ty, tw, th), 16.0, &act);
        f.hit((tx, ty, tw, th), act);
    }
    let by = top + 2.0 * th + gap + 16.0;
    let q = "Foto, documenti, video o archivi?";
    f.p.text(q, x, by, w, 26.0, 16.0, SANS, LABS_MUTED, Align::Left);
    let qx = x + f.measure(q, 16.0, SANS) + 8.0;
    let link = "Converti file";
    let lw = f.measure(link, 16.0, SEMI);
    let act = Act::Lab(Lab::Converti);
    let c = lab_colors(Lab::Converti).1;
    f.p.text(link, qx, by, lw + 4.0, 26.0, 16.0, SEMI, c, Align::Left);
    f.p.fill(qx, by + 23.0, lw, 1.0, c);
    f.focus_ring((qx - 4.0, by - 2.0, lw + 8.0, 30.0), 6.0, &act);
    f.hit((qx - 4.0, by - 2.0, lw + 8.0, 30.0), act);
    let _ = app;
}

/// Il centro: il file scelto, grande, sopra l'alone dei suoi colori; sotto, la striscia dei file in coda.
pub(super) fn stage_draw(f: &mut Frame, app: &App, r: R) {
    let vis = app.visible();
    let sel = app.selected.filter(|i| vis.contains(i)).or(vis.first().copied());
    f.p.rrect(r.0, r.1, r.2, r.3, 16.0, Some(LABS_STAGE), None);
    if let Some(img) = sel.and_then(|i| halo(app, i)) {
        f.p.image(&img, r.0, r.1, r.2, r.3, 16.0, Fit::Cover);
        // Una pagina bianca abbaglierebbe: più il file è chiaro, più l'alone si spegne.
        f.p.rrect_a(r.0, r.1, r.2, r.3, 16.0, LABS_STAGE, 0.34 + 0.5 * img.luma().powf(1.5));
    }
    let Some(i) = sel else {
        // Senza file il fondo prende appena il colore della pagina, invece di restare un buco nero.
        let (_, bright) = colors(app);
        f.p.rrect_vgrad(r.0, r.1, r.2, r.3, 16.0, mix(bright, LABS_STAGE, 0.84), mix(bright, LABS_STAGE, 0.95));
        empty(f, app, (r.0 + 28.0, r.1 + 28.0, r.2 - 56.0, r.3 - 56.0));
        return;
    };
    viewer(f, app, i, (r.0 + 18.0, r.1 + 18.0, r.2 - 36.0, r.3 - 18.0 - STRIP));
    strip(f, app, (r.0, r.1 + r.3 - STRIP, r.2, STRIP), &vis, i);
}

/// Niente file: la zona dove lasciarli, con le scritte chiare sulla cornice scura.
fn empty(f: &mut Frame, app: &App, r: R) {
    let (_, bright) = colors(app);
    let t = f.hover_t(&Act::AddFiles, true);
    f.p.rrect_a(r.0, r.1, r.2, r.3, 14.0, WHITE, 0.03 + 0.03 * t);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 14.0, mix(bright, WHITE, 0.3), 2.0, 0.55, Some((9.0, 6.0)));
    let cy = r.1 + r.3 / 2.0;
    f.p.disc_a(r.0 + r.2 / 2.0, cy - 64.0, 46.0, bright, 0.22);
    f.p.circle(r.0 + r.2 / 2.0, cy - 64.0, 36.0, Some(bright), None);
    f.icon(ic::DOWNLOAD, r.0 + r.2 / 2.0 - 20.0, cy - 84.0, 40.0, WHITE);
    f.p.text("Trascina qui i file oppure fai clic", r.0, cy, r.2, 32.0, 23.0, SEMI, LABS_INK, Align::Center);
    let cat = &conversioni::CATEGORIES[app.category];
    let accepts = match app.lab {
        Lab::Immagini | Lab::Figura => "Vetrini SVS, DICOM, ND2, LIF, TIFF a 16 bit, PNG, JPEG e le altre immagini".to_string(),
        Lab::Anonimizza => "DICOM, vetrini SVS, foto, PDF, Word".to_string(),
        Lab::Dati => "CSV, TSV, Excel, ODS, JSON".to_string(),
        Lab::Converti => "Immagini, documenti, PDF, testi, audio, video, archivi".to_string(),
        _ => format!("{} · {}", cat.hint, cat.accepts),
    };
    let lines = f.p.wrap_lines(&accepts, 16.0, SANS, r.2 - 80.0, 3);
    for (k, line) in lines.iter().enumerate() {
        f.p.text(line, r.0 + 40.0, cy + 40.0 + k as f32 * 23.0, r.2 - 80.0, 23.0, 16.0, SANS, LABS_MUTED, Align::Center);
    }
    f.focus_ring(r, 14.0, &Act::AddFiles);
    f.hit(r, Act::AddFiles);
    // I file in coda di un altro tipo non spariscono: si dice quanti sono e ci si arriva con un clic.
    others(f, app, r.0 + 24.0, r.1 + r.3 - 50.0, r.2 - 48.0);
}

/// «In coda anche: 2 PDF, 1 Audio»: ogni voce porta ai file di quel tipo.
fn others(f: &mut Frame, app: &App, x: f32, y: f32, w: f32) -> f32 {
    let list = app.others();
    if list.is_empty() || app.activity != Activity::Convert {
        return 0.0;
    }
    let head = "In coda anche:";
    f.p.text(head, x, y, w, 26.0, 14.5, SANS, LABS_MUTED, Align::Left);
    let mut cx = x + f.measure(head, 14.5, SANS) + 10.0;
    for (ci, n) in list {
        let label = format!("{n} {}", conversioni::CATEGORIES[ci].name);
        let lw = f.measure(&label, 14.5, SEMI);
        if cx + lw > x + w {
            break;
        }
        let act = Act::Category(ci);
        let hot = f.hovered(&act);
        f.p.rrect_a(cx - 8.0, y - 1.0, lw + 16.0, 28.0, 14.0, WHITE, if hot { 0.2 } else { 0.1 });
        f.p.text(&label, cx, y, lw + 4.0, 26.0, 14.5, SEMI, LABS_INK, Align::Left);
        f.hit((cx - 8.0, y - 1.0, lw + 16.0, 28.0), act);
        cx += lw + 26.0;
    }
    cx - x
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

/// La striscia dei file in coda: miniature, «Aggiungi», quanti sono.
fn strip(f: &mut Frame, app: &App, r: R, vis: &[usize], sel: usize) {
    let (_, bright) = colors(app);
    let (tw, th, gap) = (88.0, 60.0, 10.0);
    let y = r.1 + (r.3 - th) / 2.0;
    let count = if vis.len() == 1 { "1 file".to_string() } else { format!("{} file", vis.len()) };
    let cw = f.measure(&count, 14.5, SEMI);
    let add_w = f.measure("Aggiungi", 15.0, SEMI) + 58.0;
    let room = r.2 - 36.0 - add_w - gap - cw - 24.0;
    let fit = ((room / (tw + gap)).floor() as usize).max(1);
    // Se non ci stanno tutte, la striscia parte in modo che quella scelta si veda.
    let pos = vis.iter().position(|&v| v == sel).unwrap_or(0);
    let start = if vis.len() > fit { (pos + 1).saturating_sub(fit).min(vis.len() - fit) } else { 0 };
    let mut x = r.0 + 18.0;
    for &i in vis.iter().skip(start).take(fit) {
        let file = &app.files[i];
        let on = i == sel;
        let act = Act::Select(i);
        let t = f.hover_t(&act, !on);
        let ty = y - if on { 2.0 } else { 0.0 };
        f.thumb(app, i, (x, ty, tw, th), 9.0);
        if on {
            f.p.rrect_line(x - 3.0, ty - 3.0, tw + 6.0, th + 6.0, 12.0, bright, 2.5, 1.0, None);
        } else {
            f.p.rrect_a(x, ty, tw, th, 9.0, LABS_STAGE, 0.42 * (1.0 - t));
        }
        f.hit((x, ty, tw, th), act);
        if file.state == FileState::Waiting {
            // La crocetta per togliere il file: su quello scelto, o sotto il mouse.
            let rm = Act::Remove(i);
            if on || t > 0.5 || f.hovered(&rm) {
                let (cx, cy) = (x + tw - 12.0, ty + 12.0);
                f.p.disc_a(cx, cy, 10.0, LABS_STAGE, if f.hovered(&rm) { 0.95 } else { 0.7 });
                f.icon(ic::CLOSE, cx - 7.0, cy - 7.0, 14.0, WHITE);
                f.hit((cx - 12.0, cy - 12.0, 24.0, 24.0), rm);
            }
        } else {
            f.state_icon(file.state, x + tw - 13.0, ty + 13.0, 9.0);
            if let Some(out) = &file.output {
                f.hit((x + tw - 26.0, ty, 26.0, 26.0), Act::OpenPath(out.clone()));
            }
        }
        if let Some(p) = file.percent.filter(|_| file.state == FileState::Running) {
            f.p.rrect_a(x + 6.0, ty + th - 10.0, tw - 12.0, 4.0, 2.0, WHITE, 0.3);
            f.p.rrect(x + 6.0, ty + th - 10.0, (tw - 12.0) * p.clamp(0.0, 1.0), 4.0, 2.0, Some(bright), None);
        }
        x += tw + gap;
    }
    let add = (x, y, add_w, th);
    let act = Act::AddFiles;
    let t = f.hover_t(&act, true);
    f.p.rrect_a(add.0, add.1, add.2, add.3, 9.0, WHITE, 0.05 + 0.07 * t);
    f.p.rrect_line(add.0, add.1, add.2, add.3, 9.0, WHITE, 1.5, 0.32, Some((6.0, 5.0)));
    f.icon(ic::ADD, add.0 + 14.0, add.1 + (th - 22.0) / 2.0, 22.0, LABS_INK);
    f.p.text("Aggiungi", add.0 + 44.0, add.1, add.2 - 50.0, th, 15.0, SEMI, LABS_INK, Align::Left);
    f.focus_ring(add, 9.0, &act);
    f.hit(add, act);
    f.p.text(&count, r.0 + r.2 - 18.0 - cw - 4.0, y, cw + 4.0, th, 14.5, SEMI, LABS_MUTED, Align::Right);
    let ox = add.0 + add.2 + 18.0;
    let ow = r.0 + r.2 - 18.0 - cw - 20.0 - ox;
    if ow > 160.0 {
        others(f, app, ox, y + (th - 26.0) / 2.0, ow);
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
            let line = if file.state != FileState::Waiting {
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
                FileState::Failed => pal.bad,
                FileState::Done => pal.ok,
                _ => pal.muted,
            };
            28.0 + f.para(&line, x, y + 30.0, w, 14.5, SANS, c, 1.35, 2) + 8.0
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
fn choices(app: &App) -> Vec<&'static str> {
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
    let label = super::go_label(app);
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

/// Il pannello dei comandi: il file, Semplice | Completa, le scelte o tutte le opzioni del motore,
/// dove si salva, e in fondo il pulsante con l'esito.
fn side_panel(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = colors(app);
    f.p.rrect(r.0, r.1, r.2, r.3, 16.0, Some(pal.surface), None);
    let (x, w) = (r.0 + 24.0, r.2 - 48.0);
    let mut y = r.1 + 20.0;
    y += file_head(f, app, x, y, w) + 6.0;
    mode_switch(f, (x, y, w, 42.0));
    y += 42.0 + 14.0;
    let foot_h = 142.0;
    let foot_y = r.1 + r.3 - foot_h;
    let body = (r.0 + 1.0, y, r.2 - 2.0, foot_y - y);
    if f.simple {
        let ids = choices(app);
        let kind = app.kind();
        super::scrolled(f, body, Scroll::Inspector, |f| {
            let mut cy = body.1 + 2.0;
            for id in &ids {
                let v = voci::voce(kind, id);
                choice_row(f, (x, cy, w, 62.0), &v, v.label, v.hint, app.action == *id, Act::Action(id), deep);
                cy += 70.0;
            }
            if !ids.iter().any(|id| app.action == *id) {
                // Una voce scelta in «Completa» resta valida: lo si dice, invece di cambiarla di nascosto.
                let line = format!("Stai usando una voce scelta in «Completa»: {}.", super::action_title(app, false));
                cy += f.para(&line, x, cy + 4.0, w, 14.5, SANS, pal.muted, 1.4, 3) + 12.0;
            }
            cy += 6.0 + f.para(&super::summary_line(app), x, cy + 6.0, w, 14.5, SANS, pal.muted, 1.4, 4);
            cy - body.1 + 16.0
        });
    } else {
        super::scrolled(f, body, Scroll::Inspector, |f| {
            let mut cy = body.1 + 4.0;
            // «Togli i nomi» e «Prepara una figura» sono già la scelta: niente menu dei formati, si dice che cosa fanno.
            let fixed = matches!(app.lab, Lab::Anonimizza | Lab::Figura);
            if fixed {
                let v = app.action_voce();
                f.text(v.label, x, cy, w, 26.0, 17.0, SEMI, pal.text);
                cy += 30.0;
                if !v.hint.is_empty() {
                    cy += f.para(v.hint, x, cy, w, 14.5, SANS, pal.muted, 1.4, 3) + 16.0;
                }
            }
            cy += super::options_panel(f, app, x, cy, w, OPT_LABS, !fixed);
            f.hline(x, cy, w);
            cy += 16.0;
            f.text("Salvataggio", x, cy, w, 24.0, 15.0, SEMI, pal.muted);
            cy += 32.0;
            f.text("Cartella", x, cy, 110.0, 42.0, 15.0, SEMI, pal.text);
            super::dest_select(f, app, (x + 116.0, cy, w - 116.0, 42.0), 15.0);
            cy += 48.0;
            cy += f.para(&super::dest_note(app), x, cy, w, 14.0, SANS, pal.muted, 1.4, 3) + 14.0;
            f.text("Se esiste già", x, cy, 110.0, 42.0, 15.0, SEMI, pal.text);
            super::exists_select(f, app, (x + 116.0, cy, w - 116.0, 42.0), 15.0);
            cy += 48.0;
            cy += f.para(&app.exists_example(), x, cy, w, 14.0, SANS, pal.muted, 1.4, 2);
            cy - body.1 + 18.0
        });
    }
    f.hline(r.0 + 1.0, foot_y, r.2 - 2.0);
    let go = (x, foot_y + 18.0, w, 58.0);
    go_button(f, app, go, deep);
    if !super::notice(f, app, (x - 6.0, go.1 + go.3 + 12.0, w + 12.0, 44.0), 14.0) {
        super::go_extras(f, app, (go.0, go.1, go.2, go.3 + 4.0), false);
    }
}

/// «Inizio» con un file: che cosa ci si può fare, e in quale pagina.
pub(super) fn file_card(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 16.0, Some(pal.surface), None);
    let (x, w) = (r.0 + 24.0, r.2 - 48.0);
    let mut y = r.1 + 20.0;
    y += file_head(f, app, x, y, w) + 10.0;
    f.text("Con questo file puoi", x, y, w, 30.0, 21.0, BOLD, pal.text);
    y += 42.0;
    let vis = app.visible();
    let Some(i) = app.selected.filter(|i| vis.contains(i)).or(vis.first().copied()) else { return };
    let file = &app.files[i];
    let kind = file.kind;
    let media = app.media_of(&file.path);
    let slide = media.is_some_and(|m| m.slide.is_some());
    let dicom = media.is_some_and(|m| m.dicom.is_some());
    // (pagina, voce del motore, come chiamarla qui: vuoto = il nome della voce)
    let mut rows: Vec<(Lab, &'static str, &'static str)> = Vec::new();
    if file.voice_only || app.activity == Activity::Transcribe || matches!(kind, Kind::Audio | Kind::Video) {
        rows.push((Lab::Trascrivi, "", "Trascrivi e riassumi"));
    }
    match kind {
        Kind::Image => {
            rows.push((Lab::Immagini, "jpeg", "Salva in JPEG, TIFF o PNG"));
            if slide {
                rows.extend([(Lab::Immagini, "tiles", ""), (Lab::Immagini, "mitosi", ""), (Lab::Immagini, "consult", "")]);
            }
            if dicom {
                rows.push((Lab::Immagini, "rx", ""));
            }
            rows.push((Lab::Figura, "figure", "Mettila in una figura"));
            rows.push((Lab::Anonimizza, "strip", if dicom || slide { "Togli nomi e dati del proprietario" } else { "" }));
        }
        Kind::Pdf => rows.extend([(Lab::Pdf, "docx", "Porta in Word"), (Lab::Pdf, "merge", "Unisci a un altro PDF"), (Lab::Pdf, "split", "Dividi"), (Lab::Pdf, "compress", "Comprimi"), (Lab::Anonimizza, "strip", "")]),
        Kind::Doc => rows.extend([(Lab::Converti, "pdf", "Porta in PDF"), (Lab::Anonimizza, "strip", "")]),
        Kind::Text => rows.extend([(Lab::Dati, "xlsx", "Porta in Excel"), (Lab::Dati, "csv", "CSV pulito"), (Lab::Converti, "pdf", "Porta in PDF")]),
        Kind::Audio => rows.push((Lab::Converti, "mp3", "Converti in MP3, WAV, FLAC…")),
        Kind::Video => rows.push((Lab::Converti, "mp4", "Converti in MP4")),
        Kind::Archive => rows.push((Lab::Converti, "z-webp", "Converti i file dentro lo ZIP")),
        Kind::Other => {}
    }
    let body = (r.0 + 1.0, y, r.2 - 2.0, r.1 + r.3 - y - 12.0);
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut cy = body.1;
        for (lab, id, name) in rows {
            let (deep, bright) = lab_colors(lab);
            let act = Act::LabGo(lab, id);
            let row = (x - 8.0, cy, w + 16.0, 62.0);
            let t = f.hover_t(&act, true);
            f.hover_fill(row, 12.0, pal.alt, t);
            f.p.rrect_vgrad(x, cy + 13.0, 36.0, 36.0, 10.0, bright, deep);
            f.icon(lab_icon(lab), x + 8.0, cy + 21.0, 20.0, WHITE);
            let label = if name.is_empty() { voci::voce(kind, id).label } else { name };
            f.text(label, x + 50.0, cy + 9.0, w - 54.0, 24.0, 16.5, SEMI, pal.text);
            let sub = format!("in «{}»", lab.name());
            f.text(&sub, x + 50.0, cy + 32.0, w - 54.0, 20.0, 14.0, SANS, pal.muted);
            f.focus_ring(row, 12.0, &act);
            f.hit(row, act);
            cy += 64.0;
            f.hline(x, cy - 1.0, w);
        }
        cy - body.1 + 12.0
    });
}
