//! Il Cedro di partenza (2026): la casa di Cedro 4 Labs con le attività di sempre. Cornice scura (o chiara
//! con la tinta «Carta»), a sinistra le sei pagine con le icone a gradiente, al centro il file grande sopra
//! un alone dei suoi colori, a destra un pannello chiaro con tutti i comandi veri del motore: in fondo
//! al pannello, sempre in vista, dove si salva e il pulsante. «Semplice» mostra le scelte pronte,
//! «Completa» tutti i formati e tutte le opzioni, aperti. Misure su 1536×1024: barra e pannello
//! restano, il centro si allarga.
//!
//! I pezzi in comune con Cedro 4 Labs (il file sul suo alone, Semplice | Completa, le righe da scegliere,
//! il pulsante grande) stanno in `labs`; quelli delle pagine di sempre (campi dei link, avanzamento della
//! trascrizione, pagine dei PDF) in `activity` e `pagine`. Sulla zona scura del file si disegnano con i
//! colori di `theme::STAGE`, così si leggono.

use std::sync::OnceLock;

use super::*;
use crate::engine::Slice;
use crate::ui::core::{LinkFlag, LinkMenu, Scroll};
use crate::ui::gfx::{Fit, Image, Rgb};
use crate::ui::model::{FileState, Lab, PdfMode, AUDIO_QUALITY, SUBTITLES, VIDEO_QUALITY};
use crate::ui::theme::{kind_bright, kind_color, page_colors, Tint, STAGE};

const TOP: f32 = 60.0;
const RAIL: f32 = 252.0;
const PAD: f32 = 12.0;
const SIDE: f32 = 404.0;
const STRIP: f32 = 92.0;
/// Barra dei comandi in cima alla zona del file.
const BAR: f32 = 62.0;
/// Il piede del pannello: cartella, «se esiste già», pulsante e riga di quello che esce.
const FOOT: f32 = 214.0;
const WHITE: Rgb = (255, 255, 255);
const STAGE_INK: Rgb = (0xF4, 0xEF, 0xFA);
const STAGE_MUTED: Rgb = (0xB9, 0xAD, 0xCA);

const OPT: OptStyle = OptStyle { label: 15.0, note: 14.0, control_h: 42.0, gap: 18.0, visible: 0, switch_w: 52.0, switch_h: 28.0 };

const ICON_HOME: &str = "M4 11l8-7 8 7v8a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z";
const ICON_CONV: &str = "M4 8h13l-3-3M20 16H7l3 3";
const ICON_PDF: &str = "M7 3h7l4 4v14H7zM14 3v4h4M10 13h5M10 17h3";
const ICON_TR: &str = "M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4";
const ICON_WEB: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18";
const ICON_FIRMA: &str = "M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6zM9 12l2.5 2.5L15.5 10";
const ICON_BIG: &str = "M3 4h18v12H3zM7 20h2M11 20h2M15 20h2";
const ICON_LIST: &str = "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01";
const ICON_KEY: &str = "M9 2h6v6H9zM7 8h10v11a3 3 0 0 1-3 3h-4a3 3 0 0 1-3-3zM12 13v4";

fn page_icon(lab: Lab) -> &'static str {
    match lab {
        Lab::Home => ICON_HOME,
        Lab::Pdf => ICON_PDF,
        Lab::Trascrivi => ICON_TR,
        Lab::Video => ICON_WEB,
        Lab::Firma => ICON_FIRMA,
        _ => ICON_CONV,
    }
}

/// Le sei immagini di «Inizio» (disegnate per Cedro, 480×360), nell'ordine delle tessere.
pub fn tile_image(i: usize) -> Option<Image> {
    static IMAGES: OnceLock<Vec<Option<Image>>> = OnceLock::new();
    const DATA: [&[u8]; 6] = [
        include_bytes!("../../../assets/cedro/immagini.jpg"),
        include_bytes!("../../../assets/cedro/pdf.jpg"),
        include_bytes!("../../../assets/cedro/documenti.jpg"),
        include_bytes!("../../../assets/cedro/audio.jpg"),
        include_bytes!("../../../assets/cedro/video.jpg"),
        include_bytes!("../../../assets/cedro/archivi.jpg"),
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

/// «Scheda video attiva · Quadro RTX 4000»: si legge una volta, in un altro thread (su Windows chiedere
/// la scheda video può prendere un attimo); finché non c'è la riga non si mostra.
fn gpu_name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    static ASKED: std::sync::Once = std::sync::Once::new();
    ASKED.call_once(|| {
        std::thread::spawn(|| {
            let on = crate::read_settings().gpu && crate::accel::hardware().gpu.is_some();
            let name = on.then(|| short_gpu(&crate::sysinfo::info().gpu)).filter(|n| !n.is_empty());
            let _ = NAME.set(name);
        });
    });
    NAME.get().and_then(|n| n.as_deref())
}

/// Il nome della scheda senza la marca davanti, che nella barra non serve.
fn short_gpu(name: &str) -> String {
    let mut s = name.trim();
    for p in ["NVIDIA ", "AMD ", "Intel(R) ", "Intel ", "ATI "] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest;
        }
    }
    if s.contains("sconosciuta") {
        return String::new();
    }
    s.chars().take(40).collect()
}

/// I colori della pagina aperta: quello pieno e quello acceso.
fn colors(app: &App) -> (Rgb, Rgb) {
    page_colors(app.lab)
}

/// Disegna con i colori della zona scura del file (campi, elenchi, pagine dei PDF).
fn on_stage<T>(f: &mut Frame, body: impl FnOnce(&mut Frame) -> T) -> T {
    let keep = f.pal;
    f.pal = &STAGE;
    let out = body(f);
    f.pal = keep;
    out
}

pub fn draw(f: &mut Frame, app: &App) {
    frame(f, app);
    let area = (RAIL, TOP, f.w - RAIL - PAD, f.h - TOP - PAD);
    if app.lab == Lab::Home {
        home(f, app, area);
        return;
    }
    let stage = (area.0, area.1, area.2 - SIDE - PAD, area.3);
    let side = (area.0 + area.2 - SIDE, area.1, SIDE, area.3);
    match app.lab {
        Lab::Firma => firma(f, app, area),
        Lab::Trascrivi => {
            transcribe_stage(f, app, stage);
            transcribe_panel(f, app, side);
        }
        Lab::Video => {
            video_stage(f, app, stage);
            video_panel(f, app, side);
        }
        _ => {
            convert_stage(f, app, stage);
            convert_panel(f, app, side);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// La cornice

fn frame(f: &mut Frame, app: &App) {
    let t = f.theme.tint();
    f.p.clear(t.frame);
    f.p.rrect_vgrad(0.0, 0.0, f.w, (f.h * 0.55).round(), 0.0, t.frame_hi, t.frame);
    f.caption((0.0, 0.0, f.w, TOP));
    f.p.symbol(22.0, 16.0, 28.0);
    f.p.text("Cedro", 58.0, 14.0, 140.0, 32.0, 21.0, BOLD, t.ink, Align::Left);
    f.window_controls(f.w, 0.0, 48.0, t.ink, t.frame_hi);
    if let Some(name) = gpu_name() {
        let head = "Scheda video attiva";
        let tail = format!(" · {name}");
        let hw = f.measure(head, 14.5, SEMI);
        let tw = f.measure(&tail, 14.5, SANS);
        let x = f.w - 3.0 * 46.0 - 24.0 - hw - tw;
        f.p.disc_a(x - 16.0, 30.0, 8.5, t.leaf, 0.22);
        f.p.circle(x - 16.0, 30.0, 4.5, Some(t.leaf), None);
        f.p.text(head, x, 14.0, hw + 4.0, 32.0, 14.5, SEMI, t.leaf, Align::Left);
        f.p.text(&tail, x + hw, 14.0, tw + 4.0, 32.0, 14.5, SANS, t.muted, Align::Left);
    }
    let mut y = TOP + 4.0;
    for lab in Lab::CEDRO {
        rail_item(f, app, t, y, lab);
        y += 54.0;
    }
    for (k, (label, icon, act)) in [("Guida", ic::HELP, Act::OpenGuide), ("Impostazioni", ic::SETTINGS, Act::OpenSettings)].into_iter().enumerate() {
        let r = (12.0, f.h - PAD - 88.0 + k as f32 * 44.0, RAIL - 24.0, 40.0);
        let h = f.hover_t(&act, true);
        if h > 0.01 {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, t.hover, 0.07 * h);
        }
        let c = if h > 0.5 { t.ink } else { t.muted };
        f.icon(icon, r.0 + 14.0, r.1 + 10.0, 20.0, c);
        f.p.text(label, r.0 + 46.0, r.1, r.2 - 52.0, r.3, 15.0, SANS, c, Align::Left);
        f.focus_ring(r, 10.0, &act);
        f.hit(r, act);
    }
}

/// Il lavoro in corso, in percentuale, sulla pagina che l'ha avviato (la voce della barra lo mostra).
fn running_on(app: &App) -> Option<(Lab, f32)> {
    use crate::ui::model::JobKind;
    let job = app.job.as_ref()?;
    let lab = match job.kind {
        JobKind::Download { .. } => Lab::Video,
        JobKind::Transcribe => Lab::Trascrivi,
        JobKind::Convert if app.activity == Activity::Convert && app.kind() == Kind::Pdf => Lab::Pdf,
        JobKind::Convert => Lab::Converti,
    };
    Some((lab, app.progress().unwrap_or(0.0)))
}

/// Una voce della barra: l'icona su una piastrella sfumata del suo colore; quella scelta si accende.
fn rail_item(f: &mut Frame, app: &App, t: &Tint, y: f32, lab: Lab) {
    let (deep, bright) = page_colors(lab);
    let act = Act::Lab(lab);
    let on = app.lab == lab;
    let r = (12.0, y, RAIL - 24.0, 50.0);
    if on {
        let k = if t.light { 0.80 } else { 0.68 };
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(bright, t.frame, k)), Some((mix(bright, t.frame, 0.45), 1.0)));
    } else {
        let h = f.hover_t(&act, true);
        if h > 0.01 {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 12.0, t.hover, 0.07 * h);
        }
    }
    let (tx, ty, ts) = (r.0 + 8.0, y + 8.0, 34.0);
    f.p.shadow(tx, ty, ts, ts, 10.0, 5.0, 0.3, 2.0);
    f.p.rrect_vgrad(tx, ty, ts, ts, 10.0, bright, deep);
    f.p.rrect_line(tx, ty, ts, ts, 10.0, WHITE, 1.0, 0.22, None);
    f.icon(page_icon(lab), tx + 7.0, ty + 7.0, 20.0, WHITE);
    let c = if on { t.sel } else { t.ink };
    let mut lw = r.2 - 62.0;
    if let Some((busy, p)) = running_on(app).filter(|(l, _)| *l == lab) {
        let _ = busy;
        let label = format!("{}%", (p * 100.0).round() as u32);
        let bw = f.measure(&label, 12.5, BOLD) + 14.0;
        let bx = r.0 + r.2 - 10.0 - bw;
        f.p.rrect(bx, y + 15.0, bw, 20.0, 10.0, Some(deep), None);
        f.p.text(&label, bx + 7.0, y + 15.0, bw, 20.0, 12.5, BOLD, WHITE, Align::Left);
        lw -= bw + 8.0;
        f.animating = true;
    }
    f.p.text(&crate::i18n::tr_string(lab.cedro_name()), r.0 + 54.0, y, lw, r.3, 16.0, if on { BOLD } else { SEMI }, c, Align::Left);
    f.focus_ring(r, 12.0, &act);
    f.hit(r, act);
}

// ---------------------------------------------------------------------------------------------
// Inizio

/// Le tessere di «Inizio»: tipo di file, nome, formati, che cosa ci si fa.
const TILES: [(Kind, &str, &str, &str); 6] = [
    (Kind::Image, "Immagini", "PNG, JPEG, WebP, AVIF, TIFF e altri", "Cambia formato, comprimi, leggi il testo"),
    (Kind::Pdf, "PDF", "In Word, Excel, immagini, testo", "Unisci, dividi, pagine, comprimi, OCR"),
    (Kind::Doc, "Documenti e testi", "Word, ODT, RTF, TXT, CSV, JSON", "In PDF, anche PDF/A per l'archivio"),
    (Kind::Audio, "Audio", "MP3, WAV, FLAC, Opus, AIFF", "Converti oppure trascrivi in Word"),
    (Kind::Video, "Video", "MP4, WebM, MOV, MKV, AVI", "Rimpicciolisci, togli l'audio, solo l'audio"),
    (Kind::Archive, "Archivi ZIP", "Converte i file dentro e rifà lo ZIP", "Foto, audio e documenti in un colpo"),
];

fn home(f: &mut Frame, app: &App, area: R) {
    let t = f.theme.tint();
    let x = area.0 + 12.0;
    let w = area.2 - 24.0;
    f.p.text("Trascina qui un file", x, area.1 + 6.0, w, 46.0, 34.0, BOLD, t.ink, Align::Left);
    let sub = "Cedro apre l'attività giusta con il file già dentro. Oppure scegli il tipo, o parti dalle voci a sinistra.";
    f.p.text(sub, x, area.1 + 54.0, w, 26.0, 16.5, SANS, t.muted, Align::Left);
    let gap = 18.0;
    let top = area.1 + 100.0;
    let bottom_h = 64.0;
    let tw = ((w - 2.0 * gap) / 3.0).floor();
    let th = ((area.3 - 100.0 - gap - bottom_h - 24.0) / 2.0).clamp(170.0, 330.0).floor();
    for (i, (kind, name, formats, does)) in TILES.into_iter().enumerate() {
        let deep = kind_color(kind);
        let bright = kind_bright(kind);
        let act = Act::LabGo(Lab::Home, kind_key(kind));
        let h = f.hover_t(&act, true);
        let tx = x + (i % 3) as f32 * (tw + gap);
        let ty = top + (i / 3) as f32 * (th + gap) - 4.0 * h;
        f.p.shadow(tx, ty + 10.0, tw, th - 10.0, 16.0, 14.0 + 6.0 * h, 0.35 + 0.1 * h, 10.0);
        f.p.rrect(tx, ty, tw, th, 16.0, Some((12, 10, 16)), None);
        if let Some(img) = tile_image(i) {
            f.p.image(&img, tx, ty, tw, th, 16.0, Fit::Cover);
        }
        // La didascalia sfuma nel colore del tipo di file: fasce sempre più piene, poi il fondo arrotondato.
        let c = mix(deep, (0, 0, 0), 0.08);
        let (g0, g1) = (ty + th * 0.36, ty + th - 16.0);
        let bands = 30;
        let bh = (g1 - g0) / bands as f32;
        for k in 0..bands {
            let a = ((k + 1) as f32 / bands as f32).powf(1.4) * 0.96;
            f.p.fill_a(tx, g0 + k as f32 * bh, tw, bh + 0.6, c, a);
        }
        f.p.push_clip(tx, ty + th - 16.0, tw, 16.0);
        f.p.rrect_a(tx, ty + th - 48.0, tw, 48.0, 16.0, c, 0.96);
        f.p.pop_clip();
        f.p.rrect_line(tx, ty, tw, th, 16.0, mix(bright, WHITE, 0.4), 1.0, 0.18 + 0.2 * h, None);
        f.p.text(name, tx + 18.0, ty + th - 84.0, tw - 36.0, 30.0, 23.0, BOLD, WHITE, Align::Left);
        f.p.text(formats, tx + 18.0, ty + th - 54.0, tw - 36.0, 20.0, 14.0, SEMI, WHITE, Align::Left);
        f.p.text(does, tx + 18.0, ty + th - 33.0, tw - 36.0, 20.0, 14.0, SANS, mix(WHITE, c, 0.18), Align::Left);
        f.focus_ring((tx, ty, tw, th), 16.0, &act);
        f.hit((tx, ty, tw, th), act);
    }
    // Sotto: il link da scaricare e la firma, le due cose che non partono da un file da convertire.
    let by = top + 2.0 * th + gap + 22.0;
    let lw = ((w - gap) * 0.6).floor();
    let box_bg = |f: &mut Frame, r: R, hot: f32| {
        f.p.rrect_a(r.0, r.1, r.2, r.3, 14.0, (128, 128, 128), 0.12 + 0.08 * hot);
        f.p.rrect_line(r.0, r.1, r.2, r.3, 14.0, t.line, 1.0, 1.0, None);
    };
    let lr = (x, by, lw, bottom_h);
    box_bg(f, lr, 0.0);
    let (vdeep, vbright) = page_colors(Lab::Video);
    f.p.rrect_vgrad(lr.0 + 12.0, lr.1 + 15.0, 34.0, 34.0, 10.0, vbright, vdeep);
    f.icon(ic::LINK, lr.0 + 19.0, lr.1 + 22.0, 20.0, WHITE);
    let go_w = f.measure("Scarica", 15.0, BOLD) + 30.0;
    let field = (lr.0 + 58.0, lr.1 + 11.0, lr.2 - 58.0 - go_w - 22.0, 42.0);
    let typed = app.youtube.typed.clone();
    if t.light {
        f.field(field, Field::Link(false), &typed, "Incolla un link di YouTube o di un altro sito", 15.5, false);
    } else {
        on_stage(f, |f| f.field(field, Field::Link(false), &typed, "Incolla un link di YouTube o di un altro sito", 15.5, false));
    }
    let go = (lr.0 + lr.2 - 12.0 - go_w, lr.1 + 11.0, go_w, 42.0);
    small_go(f, go, "Scarica", vdeep, Act::LabGo(Lab::Video, "link"), !typed.trim().is_empty());
    let fr = (x + lw + gap, by, w - lw - gap, bottom_h);
    let act = Act::Lab(Lab::Firma);
    let hot = f.hover_t(&act, true);
    box_bg(f, fr, hot);
    let (fdeep, fbright) = page_colors(Lab::Firma);
    f.p.rrect_vgrad(fr.0 + 12.0, fr.1 + 15.0, 34.0, 34.0, 10.0, fbright, fdeep);
    f.icon(ICON_FIRMA, fr.0 + 19.0, fr.1 + 22.0, 20.0, WHITE);
    f.p.text("Firma o verifica un documento", fr.0 + 58.0, fr.1 + 10.0, fr.2 - 70.0, 24.0, 16.0, SEMI, t.ink, Align::Left);
    f.p.text("PDF firmati, .p7m, chiavetta o smart card", fr.0 + 58.0, fr.1 + 33.0, fr.2 - 70.0, 20.0, 13.5, SANS, t.muted, Align::Left);
    f.focus_ring(fr, 14.0, &act);
    f.hit(fr, act);
}

fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "img",
        Kind::Pdf => "pdf",
        Kind::Doc => "doc",
        Kind::Audio => "aud",
        Kind::Video => "vid",
        _ => "zip",
    }
}

/// Un pulsante pieno piccolo, del colore `c` (Scarica, Aggiungi).
fn small_go(f: &mut Frame, r: R, label: &str, c: Rgb, act: Act, enabled: bool) {
    if enabled {
        let h = f.hover_t(&act, true);
        f.p.shadow(r.0, r.1, r.2, r.3, 10.0, 6.0, 0.25, 3.0);
        f.p.rrect_vgrad(r.0, r.1, r.2, r.3, 10.0, mix(c, WHITE, 0.18 + 0.08 * h), c);
        f.focus_ring(r, 10.0, &act);
        f.hit(r, act);
    } else {
        f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, c, 0.45);
    }
    f.text_c(label, r, 15.0, BOLD, WHITE);
}

// ---------------------------------------------------------------------------------------------
// La zona del file

/// Il fondo della zona del file: l'alone dell'immagine scelta, o un bagliore del colore della pagina.
fn stage_bg(f: &mut Frame, app: &App, r: R, halo_of: Option<usize>) {
    let t = f.theme.tint();
    f.p.rrect(r.0, r.1, r.2, r.3, 14.0, Some(t.stage), None);
    if let Some(img) = halo_of.and_then(|i| super::labs::halo(app, i)) {
        f.p.image(&img, r.0, r.1, r.2, r.3, 14.0, Fit::Cover);
        // Una pagina bianca abbaglierebbe: più il file è chiaro, più l'alone si spegne.
        f.p.rrect_a(r.0, r.1, r.2, r.3, 14.0, t.stage, 0.36 + 0.5 * img.luma().powf(1.5));
        return;
    }
    let (_, bright) = colors(app);
    let (cx, cy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.4);
    let big = r.3.min(r.2) * 0.62;
    f.p.push_clip(r.0, r.1, r.2, r.3);
    f.p.glow(cx, cy, big, bright, 0.3);
    f.p.pop_clip();
}

/// Un pulsante della barra sulla zona scura: vetro chiaro, oppure pieno del colore della pagina.
fn bar_button(f: &mut Frame, x: f32, y: f32, label: &str, icon: &str, act: Act, primary: Option<Rgb>, enabled: bool) -> f32 {
    let w = 12.0 + 20.0 + if label.is_empty() { 0.0 } else { 8.0 + f.measure(label, 14.5, if primary.is_some() { BOLD } else { SEMI }) } + 13.0;
    let r = (x, y, w, 38.0);
    let h = if enabled { f.hover_t(&act, true) } else { 0.0 };
    match primary {
        Some(c) if enabled => {
            f.p.shadow(r.0, r.1, r.2, r.3, 10.0, 6.0, 0.3, 3.0);
            f.p.rrect_vgrad(r.0, r.1, r.2, r.3, 10.0, mix(c, WHITE, 0.2 + 0.08 * h), c);
        }
        _ => f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, WHITE, 0.1 + 0.08 * h),
    }
    let c = if enabled { WHITE } else { mix(WHITE, (0, 0, 0), 0.55) };
    f.icon(icon, x + 12.0, y + 9.0, 20.0, c);
    if !label.is_empty() {
        f.p.text(label, x + 40.0, y, w - 44.0, 38.0, 14.5, if primary.is_some() { BOLD } else { SEMI }, c, Align::Left);
    }
    if enabled {
        f.focus_ring(r, 10.0, &act);
        f.hit(r, act);
    }
    w
}

/// Aggiungi file, Cartella, Rimuovi. Restituisce dove finiscono.
fn toolbar(f: &mut Frame, app: &App, r: R, list: bool) -> f32 {
    let (deep, _) = colors(app);
    let y = r.1 + 12.0;
    let mut x = r.0 + 14.0;
    x += bar_button(f, x, y, "Aggiungi file", ic::ADD, Act::AddFiles, Some(deep), true) + 6.0;
    x += bar_button(f, x, y, "Cartella", ic::FOLDER, Act::AddFolder, None, true) + 6.0;
    let vis = app.visible();
    let sel = app.selected.filter(|i| vis.contains(i)).or(vis.first().copied());
    let remove = if list { Some(Act::RemoveChecked) } else { sel.filter(|&i| app.files[i].state == FileState::Waiting).map(Act::Remove) };
    let enabled = remove.is_some() && !app.busy() && !vis.is_empty();
    x += bar_button(f, x, y, "Rimuovi", ic::TRASH, remove.unwrap_or(Act::Nothing), None, enabled) + 6.0;
    x
}

/// File grande | Elenco, a destra nella barra.
fn view_switch(f: &mut Frame, right: f32, y: f32) {
    let list = f.ui.is_open(Fold::ListView);
    let r = (right - 82.0, y, 82.0, 38.0);
    f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, WHITE, 0.1);
    for (k, (icon, on)) in [(ICON_BIG, !list), (ICON_LIST, list)].into_iter().enumerate() {
        let b = (r.0 + 3.0 + k as f32 * 38.0, r.1 + 3.0, 38.0, 32.0);
        if on {
            f.p.rrect_a(b.0, b.1, b.2, b.3, 8.0, WHITE, 0.92);
        } else {
            let act = Act::Fold(Fold::ListView);
            let h = f.hover_t(&act, true);
            if h > 0.01 {
                f.p.rrect_a(b.0, b.1, b.2, b.3, 8.0, WHITE, 0.12 * h);
            }
            f.hit(b, act);
        }
        f.icon(icon, b.0 + 10.0, b.1 + 7.0, 18.0, if on { (0x20, 0x17, 0x29) } else { (0xCF, 0xC5, 0xDD) });
    }
}

/// Niente file: la zona dove lasciarli.
fn empty(f: &mut Frame, app: &App, r: R) {
    let (_, bright) = colors(app);
    let h = f.hover_t(&Act::AddFiles, true);
    f.p.rrect_a(r.0, r.1, r.2, r.3, 14.0, WHITE, 0.03 + 0.03 * h);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 14.0, mix(bright, WHITE, 0.3), 2.0, 0.55, Some((9.0, 6.0)));
    let cy = r.1 + r.3 / 2.0;
    f.p.disc_a(r.0 + r.2 / 2.0, cy - 64.0, 46.0, bright, 0.22);
    f.p.circle(r.0 + r.2 / 2.0, cy - 64.0, 36.0, Some(bright), None);
    f.icon(ic::DOWNLOAD, r.0 + r.2 / 2.0 - 20.0, cy - 84.0, 40.0, WHITE);
    f.p.text("Trascina qui i file oppure fai clic", r.0, cy, r.2, 32.0, 23.0, SEMI, STAGE_INK, Align::Center);
    let accepts = match app.lab {
        Lab::Trascrivi => "Audio e video: MP3, WAV, M4A, OGG, FLAC, MP4, MKV, MOV…".to_string(),
        Lab::Pdf => "PDF: da convertire, unire, dividere, sistemare o comprimere".to_string(),
        _ => {
            let cat = &conversioni::CATEGORIES[app.category];
            format!("{} · {}", cat.hint, cat.accepts)
        }
    };
    let lines = f.p.wrap_lines(&accepts, 16.0, SANS, r.2 - 80.0, 3);
    for (k, line) in lines.iter().enumerate() {
        f.p.text(line, r.0 + 40.0, cy + 40.0 + k as f32 * 23.0, r.2 - 80.0, 23.0, 16.0, SANS, STAGE_MUTED, Align::Center);
    }
    f.focus_ring(r, 14.0, &Act::AddFiles);
    f.hit(r, Act::AddFiles);
}

/// La striscia in fondo: le etichette dei tipi in coda (con quanti sono), le miniature, «Aggiungi», il conteggio.
fn strip(f: &mut Frame, app: &App, r: R, vis: &[usize], sel: Option<usize>, kinds: bool, count: &str) {
    let (_, bright) = colors(app);
    let (tw, th, gap) = (86.0, 58.0, 9.0);
    let y = r.1 + (r.3 - th) / 2.0;
    let mut x = r.0 + 16.0;
    if kinds {
        for (ci, cat) in conversioni::CATEGORIES.iter().enumerate() {
            if cat.kind == Kind::Pdf {
                continue;
            }
            let n = app.files.iter().filter(|x| !x.voice_only && x.kind == cat.kind).count();
            if n == 0 && ci != app.category {
                continue;
            }
            let on = ci == app.category;
            let num = n.to_string();
            let nw = f.measure(&num, 14.0, BOLD);
            let lw = f.measure(cat.name, 14.0, BOLD);
            let w = 9.0 + 10.0 + 7.0 + lw + 6.0 + nw + 11.0;
            let b = (x, y + (th - 34.0) / 2.0, w, 34.0);
            let act = Act::Category(ci);
            let h = f.hover_t(&act, !on);
            if on {
                f.p.rrect(b.0, b.1, b.2, b.3, 17.0, Some(WHITE), None);
            } else {
                f.p.rrect_a(b.0, b.1, b.2, b.3, 17.0, WHITE, 0.08 + 0.08 * h);
            }
            f.p.circle(b.0 + 14.0, b.1 + 17.0, 5.0, Some(if on { kind_color(cat.kind) } else { kind_bright(cat.kind) }), None);
            let tc = if on { (0x20, 0x17, 0x29) } else { (0xE9, 0xE2, 0xF2) };
            f.p.text(cat.name, b.0 + 26.0, b.1, lw + 4.0, b.3, 14.0, BOLD, tc, Align::Left);
            f.p.text(&num, b.0 + 26.0 + lw + 6.0, b.1, nw + 4.0, b.3, 14.0, BOLD, mix(tc, if on { WHITE } else { (0, 0, 0) }, 0.3), Align::Left);
            f.focus_ring(b, 17.0, &act);
            f.hit(b, act);
            x += w + 5.0;
        }
        x += 6.0;
    }
    let cw = f.measure(count, 13.5, SEMI);
    let add_w = f.measure("+ Aggiungi", 14.0, SEMI) + 28.0;
    let room = r.0 + r.2 - 16.0 - cw - 14.0 - add_w - gap - x;
    let fit = ((room / (tw + gap)).floor().max(0.0) as usize).min(vis.len());
    let pos = sel.and_then(|s| vis.iter().position(|&v| v == s)).unwrap_or(0);
    let start = if fit > 0 && vis.len() > fit { (pos + 1).saturating_sub(fit).min(vis.len() - fit) } else { 0 };
    for &i in vis.iter().skip(start).take(fit) {
        let file = &app.files[i];
        let on = Some(i) == sel;
        let act = Act::Select(i);
        let h = f.hover_t(&act, !on);
        let ty = y - if on { 2.0 } else { 0.0 };
        f.thumb(app, i, (x, ty, tw, th), 9.0);
        if on {
            f.p.rrect_line(x - 3.0, ty - 3.0, tw + 6.0, th + 6.0, 12.0, bright, 2.5, 1.0, None);
        } else {
            f.p.rrect_a(x, ty, tw, th, 9.0, (9, 7, 12), 0.4 * (1.0 - h));
        }
        f.hit((x, ty, tw, th), act);
        match file.state {
            FileState::Waiting => {}
            FileState::Running => {
                if let Some(p) = file.percent {
                    f.p.rrect(x, ty + th - 5.0, tw * p.clamp(0.0, 1.0), 5.0, 2.0, Some(bright), None);
                }
            }
            _ => {
                f.state_icon(file.state, x + tw - 13.0, ty + 13.0, 9.0);
                if let Some(out) = &file.output {
                    f.hit((x + tw - 26.0, ty, 26.0, 26.0), Act::OpenPath(out.clone()));
                }
            }
        }
        x += tw + gap;
    }
    let add = (x, y, add_w, th);
    let act = Act::AddFiles;
    let h = f.hover_t(&act, true);
    f.p.rrect_a(add.0, add.1, add.2, add.3, 9.0, WHITE, 0.05 + 0.07 * h);
    f.p.rrect_line(add.0, add.1, add.2, add.3, 9.0, WHITE, 1.5, 0.3, Some((6.0, 5.0)));
    f.text_c("+ Aggiungi", add, 14.0, SEMI, (0xE9, 0xE2, 0xF2));
    f.hit(add, act);
    f.p.text(count, r.0 + r.2 - 16.0 - cw - 4.0, y, cw + 4.0, th, 13.5, SEMI, mix(WHITE, (0, 0, 0), 0.25), Align::Right);
}

/// Il file scelto (o il primo della coda).
fn selected(app: &App) -> Option<usize> {
    let vis = app.visible();
    app.selected.filter(|i| vis.contains(i)).or(vis.first().copied())
}

fn count_line(app: &App, vis: &[usize]) -> String {
    if app.lab == Lab::Pdf {
        let pages: usize = vis.iter().filter_map(|&i| app.page_count(i)).sum();
        let n = vis.len();
        let head = if n == 1 { "1 PDF".to_string() } else { format!("{n} PDF") };
        return if pages > 0 { format!("{head} · {pages} pagine") } else { head };
    }
    if vis.len() == 1 {
        "1 file".into()
    } else {
        format!("{} file", vis.len())
    }
}

// ---------------------------------------------------------------------------------------------
// Converti e PDF

fn convert_stage(f: &mut Frame, app: &App, r: R) {
    let vis = app.visible();
    let sel = selected(app);
    let list = f.ui.is_open(Fold::ListView);
    let pdf = app.lab == Lab::Pdf;
    let mode = if pdf { app.pdf_mode() } else { PdfMode::Convert };
    let halo = sel.filter(|&i| !list && app.files[i].kind == Kind::Image);
    stage_bg(f, app, r, halo);
    toolbar(f, app, r, list);
    let right = r.0 + r.2 - 14.0;
    match (pdf, mode, sel) {
        (true, PdfMode::Pages, Some(i)) => {
            on_stage(f, |f| super::pagine::pages_tools(f, app, i, right, r.1 + 12.0, 38.0, 14.5));
        }
        (_, PdfMode::Convert | PdfMode::Compress, _) => view_switch(f, right, r.1 + 12.0),
        _ => {}
    }
    let c = (r.0 + 16.0, r.1 + BAR, r.2 - 32.0, r.3 - BAR - STRIP);
    match sel {
        None => empty(f, app, c),
        Some(_) if list && matches!(mode, PdfMode::Convert | PdfMode::Compress) => file_list(f, app, c, &vis),
        Some(i) => match mode {
            PdfMode::Merge => on_stage(f, |f| {
                super::scrolled(f, c, Scroll::Queue, |f| super::pagine::merged_strip(f, app, c.0 + 8.0, c.1 + 6.0, c.2 - 16.0, 112.0) + 16.0);
            }),
            PdfMode::Split => on_stage(f, |f| {
                let i = super::pagine::split_file(app).unwrap_or(i);
                super::scrolled(f, c, Scroll::Queue, |f| super::pagine::split_strip(f, app, i, c.0 + 8.0, c.1 + 6.0, c.2 - 16.0, 118.0) + 16.0);
            }),
            PdfMode::Pages => on_stage(f, |f| {
                let i = super::pagine::split_file(app).unwrap_or(i);
                // Quale PDF è sul banco (se ce n'è più d'uno si cambia da qui) e quante pagine sono scelte.
                let sw = 320.0f32.min(c.2 * 0.5);
                f.select((c.0, c.1, sw, 38.0), &app.files[i].name(), None, 14.5, Act::PagesFileMenu);
                super::pagine::pages_count(f, app, i, c.0 + sw + 14.0, c.1, c.2 - sw - 18.0, 38.0, 14.5);
                let body = (c.0, c.1 + 48.0, c.2, c.3 - 48.0);
                let stamped = !conversioni::options_from(&app.values).pdf_edit.stamp.is_empty();
                super::scrolled(f, body, Scroll::Queue, |f| {
                    if stamped {
                        super::pagine::stamp_view(f, app, i, body.0 + 8.0, body.1 + 6.0, body.2 - 16.0, (body.3 - 150.0).clamp(220.0, 410.0), 64.0) + 24.0
                    } else {
                        let pw = 128.0;
                        let per_row = (((body.2 - 16.0 + 38.0) / (pw + 38.0)) as usize).max(1);
                        let used = per_row as f32 * (pw + 38.0) - 38.0;
                        super::pagine::board(f, app, i, body.0 + ((body.2 - used) / 2.0).round(), body.1 + 8.0, used, pw) + 40.0
                    }
                });
            }),
            _ => {
                let cap = app.files[i].kind == Kind::Image;
                let v = (c.0, c.1, c.2, c.3 - if cap { 34.0 } else { 0.0 });
                super::labs::viewer(f, app, i, v);
                if cap {
                    who(f, app, i, (c.0, c.1 + c.3 - 30.0, c.2, 26.0));
                }
            }
        },
    }
    let kinds = !pdf;
    strip(f, app, (r.0, r.1 + r.3 - STRIP, r.2, STRIP), &vis, sel, kinds, &count_line(app, &vis));
}

/// Sotto l'immagine: nome e misure.
fn who(f: &mut Frame, app: &App, i: usize, r: R) {
    let name = app.files[i].name();
    let (size, extra) = super::file_meta(app, i);
    let meta = [extra, size].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    let nw = f.measure(&name, 14.0, SEMI).min(r.2 * 0.6);
    let mw = f.measure(&meta, 14.0, SANS);
    let x = r.0 + (r.2 - nw - 10.0 - mw) / 2.0;
    f.p.text(&name, x, r.1, nw + 2.0, r.3, 14.0, SEMI, STAGE_INK, Align::Left);
    f.p.text(&meta, x + nw + 10.0, r.1, mw + 4.0, r.3, 14.0, SANS, STAGE_MUTED, Align::Left);
}

/// L'elenco della coda: spunta, miniatura, nome con lo stato, dimensione, misure, togli.
fn file_list(f: &mut Frame, app: &App, r: R, vis: &[usize]) {
    let (_, bright) = colors(app);
    f.p.rrect_a(r.0, r.1, r.2, r.3, 12.0, (12, 9, 16), 0.72);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 12.0, WHITE, 1.0, 0.1, None);
    let head = (r.0, r.1, r.2, 40.0);
    f.p.rrect(head.0, head.1, head.2, head.3, 12.0, Some((0x17, 0x12, 0x1E)), None);
    let all = vis.iter().all(|&i| app.files[i].checked);
    on_stage(f, |f| f.checkbox(r.0 + 14.0, r.1 + 10.0, 20.0, all, Some(Act::CheckAll)));
    let extra_head = match app.kind() {
        Kind::Audio | Kind::Video => "Durata",
        Kind::Image => "Risoluzione",
        _ => "Pagine",
    };
    let col_x = r.0 + r.2 - 52.0 - 120.0 - 110.0;
    let name_x = r.0 + 54.0 + 84.0;
    for (label, x) in [("Nome file", name_x), ("Dimensione", col_x), (extra_head, col_x + 110.0)] {
        f.p.text(label, x, head.1, 140.0, head.3, 13.0, BOLD, STAGE_MUTED, Align::Left);
    }
    let body = (r.0 + 1.0, r.1 + 40.0, r.2 - 2.0, r.3 - 41.0);
    let rows = vis.to_vec();
    let rh = 66.0;
    on_stage(f, |f| {
        super::scrolled(f, body, Scroll::Queue, |f| {
            for (n, &i) in rows.iter().enumerate() {
                let y = body.1 + n as f32 * rh;
                if !f.p.shows_y(y, rh) {
                    continue;
                }
                let file = &app.files[i];
                f.p.fill_a(r.0, y, r.2, 1.0, WHITE, 0.08);
                if app.selected == Some(i) {
                    f.p.fill_a(r.0 + 1.0, y + 1.0, r.2 - 2.0, rh - 1.0, bright, 0.16);
                }
                f.hit((r.0, y, r.2, rh), Act::Select(i));
                f.checkbox(r.0 + 14.0, y + (rh - 20.0) / 2.0, 20.0, file.checked, Some(Act::Check(i)));
                f.thumb(app, i, (r.0 + 54.0, y + 9.0, 74.0, 48.0), 6.0);
                super::file_name(f, app, i, name_x, y + 10.0, col_x - name_x - 16.0, 24.0, 15.0, BOLD, STAGE_INK);
                let (sub, c) = match file.state {
                    FileState::Done => (file.message.clone(), (0x5B, 0xE0, 0xA3)),
                    FileState::Failed => (file.message.clone(), (0xFF, 0x8A, 0x8F)),
                    FileState::Waiting => ("In attesa".to_string(), STAGE_MUTED),
                    _ => (file.message.clone(), STAGE_MUTED),
                };
                f.p.text(&sub, name_x, y + 34.0, col_x - name_x - 16.0, 22.0, 13.0, SANS, c, Align::Left);
                let (size, extra) = super::file_meta(app, i);
                f.p.text(&size, col_x, y, 104.0, rh, 14.0, SANS, STAGE_INK, Align::Left);
                f.p.text(&extra, col_x + 110.0, y, 118.0, rh, 14.0, SANS, STAGE_INK, Align::Left);
                let cx = r.0 + r.2 - 32.0;
                if file.state == FileState::Waiting {
                    let act = Act::Remove(i);
                    if f.hovered(&act) {
                        f.p.disc_a(cx, y + rh / 2.0, 16.0, WHITE, 0.12);
                    }
                    f.icon(ic::CLOSE, cx - 9.0, y + rh / 2.0 - 9.0, 18.0, STAGE_MUTED);
                    f.hit((cx - 16.0, y + rh / 2.0 - 16.0, 32.0, 32.0), act);
                } else {
                    f.state_icon(file.state, cx, y + rh / 2.0, 10.0);
                    if let Some(out) = &file.output {
                        f.hit((cx - 16.0, y + rh / 2.0 - 16.0, 32.0, 32.0), Act::OpenPath(out.clone()));
                    }
                }
            }
            rows.len() as f32 * rh + 1.0
        });
    });
}

/// In cima al pannello: il file scelto (o cosa c'è in coda) e Semplice | Completa. Restituisce la y sotto.
fn panel_head(f: &mut Frame, app: &App, r: R, title: Option<(String, String)>) -> f32 {
    let pal = f.pal;
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let mut y = r.1 + 16.0;
    match title {
        Some((name, meta)) => {
            f.text(&name, x, y, w, 26.0, 17.0, BOLD, pal.text);
            y += 26.0;
            y += f.para(&meta, x, y, w, 14.0, SANS, pal.muted, 1.35, 2) + 10.0;
        }
        None => y += super::labs::file_head(f, app, x, y, w) + 2.0,
    }
    super::labs::mode_switch(f, (x, y, w, 40.0));
    y += 40.0 + 12.0;
    f.hline(r.0 + 1.0, y, r.2 - 2.0);
    y + 14.0
}

/// Il pannello chiaro: fondo, e la riga sopra il piede.
fn panel_bg(f: &mut Frame, r: R) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 14.0, Some(pal.surface), None);
}

/// Le etichette scritte piccole in grassetto sopra ogni gruppo («Formato», «Qualità»…).
fn label(f: &mut Frame, text: &str, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    f.text(text, x, y, w, 22.0, 14.0, BOLD, pal.muted);
    26.0
}

/// Una fila di chip che va a capo: ogni chip ha il pallino del suo colore. Restituisce l'altezza.
fn chips(f: &mut Frame, x: f32, y: f32, w: f32, items: &[(String, Rgb, bool, Act)]) -> f32 {
    let pal = f.pal;
    let (h, gap) = (36.0, 7.0);
    let (mut cx, mut cy) = (x, y);
    for (label, c, on, act) in items {
        let lw = f.measure(label, 14.5, SEMI);
        let cw = 12.0 + 10.0 + 7.0 + lw + 13.0;
        if cx + cw > x + w && cx > x {
            cx = x;
            cy += h + gap;
        }
        let b = (cx, cy, cw, h);
        let hot = f.hover_t(act, !*on);
        if *on {
            f.p.rrect(b.0, b.1, b.2, b.3, 18.0, Some(mix(*c, pal.surface, 0.88)), Some((*c, 1.6)));
        } else {
            f.p.rrect(b.0, b.1, b.2, b.3, 18.0, Some(pal.surface), Some((mix(pal.border, *c, 0.5 * hot), 1.3)));
        }
        f.p.circle(b.0 + 17.0, b.1 + h / 2.0, 5.0, Some(*c), None);
        f.p.text(label, b.0 + 29.0, b.1, lw + 4.0, h, 14.5, SEMI, if *on { mix(*c, (0, 0, 0), 0.2) } else { pal.text }, Align::Left);
        f.focus_ring(b, 18.0, act);
        f.hit(b, act.clone());
        cx += cw + gap;
    }
    cy + h - y
}

/// Le chip dei formati e degli strumenti del motore per il tipo scelto, nei colori della loro famiglia.
fn format_chips(app: &App, list: &[Slice]) -> Vec<(String, Rgb, bool, Act)> {
    let kind = app.kind();
    list.iter()
        .filter(|s| !conversioni::is_page_action(kind, s.id))
        .map(|s| {
            let v = app.voce(s.id);
            (v.label.to_string(), v.color, app.action == s.id, Act::Action(s.id))
        })
        .collect()
}

/// Le scelte pronte di «Semplice»: righe grandi con l'icona colorata. Restituisce l'altezza.
fn simple_rows(f: &mut Frame, app: &App, x: f32, y: f32, w: f32, deep: Rgb) -> f32 {
    let pal = f.pal;
    let mut cy = y;
    let choices = app.simple_choices();
    for c in &choices {
        let v = app.voce(c.id);
        let on = app.action == c.id;
        super::labs::choice_row(f, (x, cy, w, 60.0), &v, c.title, c.note, on, Act::Action(c.id), deep);
        cy += 68.0;
    }
    if !choices.iter().any(|c| app.action == c.id) {
        // Una voce scelta in «Completa» resta valida: lo si dice, invece di cambiarla di nascosto.
        let line = format!("Stai usando una voce scelta in «Completa»: {}.", super::action_title(app, false));
        cy += f.para(&line, x, cy + 4.0, w, 14.0, SANS, pal.muted, 1.4, 3) + 12.0;
    }
    cy - y
}

fn convert_panel(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = colors(app);
    panel_bg(f, r);
    let pdf = app.lab == Lab::Pdf;
    let merging = pdf && !f.simple && app.pdf_mode() == PdfMode::Merge;
    let head = merging.then(|| {
        let vis = app.visible();
        let pages: usize = vis.iter().filter_map(|&i| app.page_count(i)).sum();
        (format!("{} PDF da unire", vis.len()), if pages > 0 { format!("{pages} pagine in tutto") } else { String::new() })
    });
    let top = panel_head(f, app, r, head);
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let foot_y = r.1 + r.3 - FOOT;
    let body = (r.0 + 1.0, top - 4.0, r.2 - 2.0, foot_y - top + 4.0);
    let kind = app.kind();
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut cy = top;
        if f.simple {
            cy += label(f, if pdf { "Cosa ne faccio" } else { "In cosa lo trasformo" }, x, cy, w);
            cy += simple_rows(f, app, x, cy, w, deep);
            cy += 6.0 + f.para(&super::summary_line(app), x, cy + 6.0, w, 14.0, SANS, pal.muted, 1.4, 4);
            return cy - top + 18.0;
        }
        if pdf {
            cy += label(f, "Cosa vuoi fare", x, cy, w);
            cy += super::pagine::modes(f, app, x, cy, w, 74.0) + 16.0;
            // Le pagine di «Pagine» e «Dividi» stanno grandi al centro: qui non si ripetono.
            if matches!(app.pdf_mode(), PdfMode::Merge | PdfMode::Compress) {
                let h = super::pagine::panel_extras(f, app, x, cy, w, OPT);
                if h > 0.0 {
                    cy += h + OPT.gap;
                }
            } else if app.pdf_mode() == PdfMode::Split && app.visible().len() > 1 {
                cy += f.para("I tagli valgono per il file scelto nella striscia in basso, gli intervalli per gli altri.", x, cy, w, 14.0, SANS, pal.muted, 1.45, 2) + 14.0;
            } else if app.pdf_mode() == PdfMode::Pages {
                cy += f.para("Clic su una pagina per sceglierla, trascina per spostarla. I comandi in alto valgono per la scelta.", x, cy, w, 14.0, SANS, pal.muted, 1.45, 3) + 14.0;
            }
        }
        if !pdf || app.pdf_mode() == PdfMode::Convert {
            let formats = format_chips(app, conversioni::formats(kind));
            cy += label(f, &format!("{} · {} formati", if pdf { "Converti in" } else { "Formato" }, formats.len()), x, cy, w);
            cy += chips(f, x, cy, w, &formats) + 16.0;
            let tools = format_chips(app, conversioni::tools(kind));
            if !tools.is_empty() {
                cy += label(f, "Oppure uno strumento", x, cy, w);
                cy += chips(f, x, cy, w, &tools) + 16.0;
            }
            let v = app.action_voce();
            if !v.hint.is_empty() {
                let line = crate::i18n::lit(&format!("{} · {}", crate::i18n::tr(v.label), crate::i18n::tr(v.hint)));
                cy += f.para(&line, x, cy, w, 14.0, SANS, pal.muted, 1.4, 3) + 14.0;
            }
        }
        cy += super::options_panel(f, app, x, cy, w, OPT, false);
        cy - top + 18.0
    });
    panel_foot(f, app, r, deep, true);
}

/// Il piede del pannello: dove salva, se esiste già, il pulsante e la riga di quello che esce.
fn panel_foot(f: &mut Frame, app: &App, r: R, deep: Rgb, exists: bool) {
    let pal = f.pal;
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let foot_y = r.1 + r.3 - FOOT;
    f.hline(r.0 + 1.0, foot_y, r.2 - 2.0);
    // La cartella ha i nomi più lunghi («Accanto all'originale»): prende più spazio di «Se esiste già».
    let (dw, ew) = if exists { (((w - 10.0) * 0.6).round(), w - 10.0 - ((w - 10.0) * 0.6).round()) } else { (w, 0.0) };
    f.text("Cartella di destinazione", x, foot_y + 12.0, dw, 20.0, 13.0, BOLD, pal.muted);
    super::dest_select(f, app, (x, foot_y + 34.0, dw, 40.0), 14.5);
    if exists {
        f.text("Se esiste già", x + dw + 10.0, foot_y + 12.0, ew, 20.0, 13.0, BOLD, pal.muted);
        super::exists_select(f, app, (x + dw + 10.0, foot_y + 34.0, ew, 40.0), 14.5);
    }
    let go = (x, foot_y + 88.0, w, 56.0);
    super::labs::go_button(f, app, go, deep);
    let line_r = (x - 6.0, go.1 + go.3 + 12.0, w + 12.0, 44.0);
    if !super::notice(f, app, line_r, 14.0) {
        if app.busy() || app.finished() {
            super::go_extras(f, app, (go.0, go.1, go.2, go.3 + 4.0), false);
        } else {
            out_line(f, app, (x, go.1 + go.3 + 14.0, w, 40.0));
        }
    }
}

/// «Esce come Foto-mare.webp»: il nome vero di quello che esce, in grassetto.
fn out_line(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let text = match app.activity {
        Activity::Convert if !(app.kind() == Kind::Pdf && conversioni::is_page_action(app.kind(), &app.action)) => {
            let Some(i) = selected(app) else { return };
            let stem = app.files[i].path.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
            let (name, ext) = app.example_name();
            let out = name.replace("nomefile", &stem);
            let out = if ext.is_empty() { out } else { format!("{out}.{ext}") };
            let out = crate::i18n::lit(&out);
            match app.todo().len() {
                0 | 1 => format!("Esce come {out}"),
                2 => format!("Esce come {out}, e così l'altro"),
                n => format!("Esce come {out}, e così gli altri {}", n - 1),
            }
        }
        Activity::Transcribe => {
            let Some(i) = selected(app) else { return };
            let stem = app.files[i].path.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
            let ext = if app.transcribe.format == crate::trascrizione::Format::Word { "docx" } else { "txt" };
            format!("Esce come {}", crate::i18n::lit(&format!("{stem}.{ext}")))
        }
        // In «Semplice» il riassunto sta già sotto le scelte, nel pannello: qui non si ripete.
        Activity::Convert if f.simple => return,
        _ => super::summary_line(app),
    };
    let lines = f.p.wrap_lines(&crate::i18n::tr_string(&text), 14.0, SANS, r.2, 2);
    for (k, line) in lines.iter().enumerate() {
        f.p.text(line, r.0, r.1 + k as f32 * 20.0, r.2, 20.0, 14.0, SANS, pal.muted, Align::Center);
    }
}

// ---------------------------------------------------------------------------------------------
// Trascrivi

fn transcribe_stage(f: &mut Frame, app: &App, r: R) {
    let vis = app.visible();
    let sel = selected(app);
    stage_bg(f, app, r, None);
    toolbar(f, app, r, false);
    let c = (r.0 + 16.0, r.1 + BAR, r.2 - 32.0, r.3 - BAR - STRIP);
    let Some(i) = sel else {
        empty(f, app, c);
        strip(f, app, (r.0, r.1 + r.3 - STRIP, r.2, STRIP), &vis, sel, false, &count_line(app, &vis));
        return;
    };
    // In alto il file che si ascolta (con play e posizione), sotto il foglio dove compare il testo.
    let wave_h = (c.3 * 0.3).clamp(150.0, 260.0);
    super::documento::media_card(f, app, i, (c.0, c.1, c.2, wave_h));
    let paper = (c.0, c.1 + wave_h + 12.0, c.2, c.3 - wave_h - 12.0);
    paper_box(f, app, paper);
    strip(f, app, (r.0, r.1 + r.3 - STRIP, r.2, STRIP), &vis, sel, false, &count_line(app, &vis));
}

/// Il foglio della trascrizione: le frasi con chi parla mentre arrivano; in fondo lo stato, Rivedi e
/// Riassunto AI quando il testo è pronto.
fn paper_box(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = colors(app);
    f.p.shadow(r.0, r.1, r.2, r.3, 12.0, 18.0, 0.4, 8.0);
    f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(pal.surface), None);
    let st_h = 48.0;
    let text = (r.0, r.1, r.2, r.3 - st_h);
    let lines = super::activity::transcribe_lines(app);
    let said: Vec<_> = lines.iter().filter(|l| l.3.is_some() || l.0 == crate::trascrizione::Tone::Speech).cloned().collect();
    if said.is_empty() {
        let ph = "Il testo compare qui mentre Cedro trascrive, con chi parla. A lavoro finito lo rivedi riascoltando i punti dubbi.";
        f.para(ph, r.0 + 22.0, r.1 + 18.0, r.2 - 44.0, 16.0, SANS, pal.muted, 1.6, 4);
    } else {
        super::activity::log_box(f, (text.0 + 6.0, text.1 + 6.0, text.2 - 12.0, text.3 - 6.0), &said);
    }
    f.hline(r.0, r.1 + r.3 - st_h, r.2);
    let sy = r.1 + r.3 - st_h;
    let last = lines.last();
    let (key, rest) = match last {
        Some(l) if !l.1.is_empty() => (l.1.clone(), l.2.clone()),
        Some(l) => (String::new(), l.2.clone()),
        None => ("Motore".to_string(), super::activity::model_note(app)),
    };
    let mut x = r.0 + 16.0;
    if !key.is_empty() {
        f.text(&key, x, sy, 200.0, st_h, 13.5, BOLD, deep);
        x += f.measure(&key, 13.5, BOLD) + 8.0;
    }
    let right_w = 250.0;
    f.text(&rest, x, sy, r.0 + r.2 - right_w - x, st_h, 13.5, SANS, pal.muted);
    // Rivedi e Riassunto AI per il file scelto, se ha una trascrizione.
    if let Some(i) = selected(app) {
        let file = &app.files[i];
        if matches!(file.state, FileState::Done | FileState::Waiting) && crate::revisione::has(&file.path) {
            let ai_w = if cfg!(target_os = "android") { 0.0 } else { f.measure("Riassunto AI", 13.5, SEMI) + 38.0 };
            let bx = r.0 + r.2 - 12.0 - ai_w;
            if ai_w > 0.0 {
                super::activity::ai_button(f, (bx, sy + 8.0, ai_w, 32.0), 13.5, Act::SummaryOpen(i));
            }
            f.button((bx - 8.0 - 84.0, sy + 8.0, 84.0, 32.0), "Rivedi", None, 13.5, Act::ReviewOpen(i));
        }
    }
    f.tool(r.0 + r.2 - 12.0 - 90.0 - if selected(app).is_some_and(|i| crate::revisione::has(&app.files[i].path)) { 260.0 } else { 0.0 }, sy + 8.0, 32.0, "Copia", ic::COPY, 13.5, Act::CopyLog, !lines.is_empty());
}

fn transcribe_panel(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = colors(app);
    panel_bg(f, r);
    let top = panel_head(f, app, r, None);
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let foot_y = r.1 + r.3 - FOOT;
    let body = (r.0 + 1.0, top - 4.0, r.2 - 2.0, foot_y - top + 4.0);
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut cy = top;
        cy += label(f, "Risultato", x, cy, w);
        let word = app.transcribe.format == crate::trascrizione::Format::Word;
        segmented(f, (x, cy, w, 40.0), &[("Documento Word", word, Act::TrFormat(true)), ("Testo semplice", !word, Act::TrFormat(false))]);
        cy += 40.0 + 20.0;
        cy += label(f, "Precisione", x, cy, w);
        let models = crate::trascrizione::MODELS;
        for (k, m) in models.iter().enumerate() {
            let on = app.transcribe.model.min(models.len() - 1) == k;
            let size = crate::trascrizione::size_text(m.megabytes);
            card_row(f, (x, cy, w, 58.0), m.name, m.note, &size, on, Act::TrModel(k), deep);
            cy += 64.0;
        }
        cy += f.para("Il modello si scarica al primo uso e poi resta sul PC.", x, cy + 2.0, w, 14.0, SANS, pal.muted, 1.4, 2) + 14.0;
        if app.busy() && app.activity == Activity::Transcribe {
            super::activity::transcribe_progress(f, app, (x, cy, w, super::activity::PROGRESS_H));
            cy += super::activity::PROGRESS_H + 14.0;
        }
        cy - top + 18.0
    });
    panel_foot(f, app, r, deep, false);
}

/// Due o tre voci affiancate in un binario (Word | Testo, Video | Solo audio, Firma | Verifica).
fn segmented(f: &mut Frame, r: R, items: &[(&str, bool, Act)]) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.alt), None);
    let n = items.len().max(1) as f32;
    let wv = (r.2 - 6.0) / n;
    for (k, (label, on, act)) in items.iter().enumerate() {
        let b = (r.0 + 3.0 + k as f32 * wv, r.1 + 3.0, wv, r.3 - 6.0);
        if *on {
            f.p.shadow(b.0, b.1, b.2, b.3, 8.0, 4.0, 0.14, 1.0);
            f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(pal.surface), None);
        }
        let hot = !*on && f.hovered(act);
        f.text_c(label, b, 14.5, SEMI, if *on || hot { pal.text } else { pal.muted });
        f.focus_ring(b, 8.0, act);
        f.hit(b, act.clone());
    }
}

/// Una riga da scegliere con il pallino, il nome, una riga che spiega e a destra un numero (il peso).
#[allow(clippy::too_many_arguments)]
fn card_row(f: &mut Frame, r: R, title: &str, note: &str, right: &str, on: bool, act: Act, c: Rgb) {
    let pal = f.pal;
    let h = f.hover_t(&act, !on);
    if on {
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(c, pal.surface, 0.91)), Some((c, 1.6)));
    } else {
        f.p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(pal.surface), Some((mix(pal.border, c, 0.5 * h), 1.4)));
    }
    let cy = r.1 + r.3 / 2.0;
    f.p.circle(r.0 + 22.0, cy, 9.0, Some(pal.surface), Some((if on { c } else { pal.border }, 2.0)));
    if on {
        f.p.circle(r.0 + 22.0, cy, 4.5, Some(c), None);
    }
    let rw = if right.is_empty() { 0.0 } else { f.measure(right, 13.5, SEMI) + 16.0 };
    let tw = r.2 - 44.0 - rw - 10.0;
    if note.is_empty() {
        f.text(title, r.0 + 42.0, r.1, tw, r.3, 15.5, SEMI, pal.text);
    } else {
        f.text(title, r.0 + 42.0, r.1 + 8.0, tw, 22.0, 15.5, SEMI, pal.text);
        f.text(note, r.0 + 42.0, r.1 + 30.0, tw, 20.0, 13.0, SANS, pal.muted);
    }
    if rw > 0.0 {
        f.text_r(right, (r.0 + r.2 - rw - 6.0, r.1, rw, r.3), 13.5, SEMI, pal.muted);
    }
    f.focus_ring(r, 12.0, &act);
    f.hit(r, act);
}

// ---------------------------------------------------------------------------------------------
// Video dal web (YouTube e gli altri siti in una pagina sola: il motore riconosce oltre mille siti)

fn video_stage(f: &mut Frame, app: &App, r: R) {
    let l = &app.youtube;
    let (deep, _) = colors(app);
    stage_bg(f, app, r, None);
    let y = r.1 + 12.0;
    let add_w = f.measure("Aggiungi", 14.5, BOLD) + 54.0;
    let field = (r.0 + 14.0, y, r.2 - 28.0 - add_w - 8.0, 46.0);
    let typed = l.typed.clone();
    on_stage(f, |f| f.field(field, Field::Link(false), &typed, "Incolla un link di YouTube, RaiPlay, Vimeo o di un altro sito", 15.5, false));
    let enabled = !typed.trim().is_empty();
    let b = (r.0 + r.2 - 14.0 - add_w, y, add_w, 46.0);
    if enabled {
        let h = f.hover_t(&Act::LinkAdd(false), true);
        f.p.shadow(b.0, b.1, b.2, b.3, 10.0, 6.0, 0.3, 3.0);
        f.p.rrect_vgrad(b.0, b.1, b.2, b.3, 10.0, mix(deep, WHITE, 0.2 + 0.08 * h), deep);
        f.hit(b, Act::LinkAdd(false));
    } else {
        f.p.rrect_a(b.0, b.1, b.2, b.3, 10.0, WHITE, 0.1);
    }
    f.icon(ic::ADD, b.0 + 14.0, b.1 + 13.0, 20.0, WHITE);
    f.p.text("Aggiungi", b.0 + 40.0, b.1, b.2 - 44.0, b.3, 14.5, BOLD, WHITE, Align::Left);
    let c = (r.0 + 16.0, r.1 + 74.0, r.2 - 32.0, r.3 - 74.0 - 56.0);
    if l.items.is_empty() {
        let (_, bright) = colors(app);
        let cy = c.1 + c.3 / 2.0;
        f.p.disc_a(c.0 + c.2 / 2.0, cy - 64.0, 46.0, bright, 0.22);
        f.p.circle(c.0 + c.2 / 2.0, cy - 64.0, 36.0, Some(bright), None);
        f.icon(ICON_WEB, c.0 + c.2 / 2.0 - 20.0, cy - 84.0, 40.0, WHITE);
        f.p.text("Nessun link in coda", c.0, cy, c.2, 32.0, 23.0, SEMI, STAGE_INK, Align::Center);
        let hint = "Incolla il link di un video o di una playlist: YouTube, RaiPlay, Vimeo, Internet Archive e oltre mille altri siti.";
        let lines = f.p.wrap_lines(hint, 16.0, SANS, c.2 - 80.0, 3);
        for (k, line) in lines.iter().enumerate() {
            f.p.text(line, c.0 + 40.0, cy + 40.0 + k as f32 * 23.0, c.2 - 80.0, 23.0, 16.0, SANS, STAGE_MUTED, Align::Center);
        }
    } else {
        on_stage(f, |f| super::activity::links_list(f, app, l, c, true, true));
    }
    let n = l.items.len();
    let count = match n {
        0 => "Oltre mille siti riconosciuti".to_string(),
        1 => "1 video in coda · oltre mille siti riconosciuti".to_string(),
        n => format!("{n} video in coda · oltre mille siti riconosciuti"),
    };
    f.p.text(&count, r.0 + 18.0, r.1 + r.3 - 48.0, r.2 - 36.0, 36.0, 13.5, SEMI, mix(WHITE, (0, 0, 0), 0.25), Align::Left);
}

fn video_panel(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let l = &app.youtube;
    let (deep, _) = colors(app);
    panel_bg(f, r);
    let n = l.items.len();
    let head = (if n == 1 { "1 video in coda".to_string() } else { format!("{n} video in coda") }, "Da YouTube o da un altro sito".to_string());
    let top = panel_head(f, app, r, Some(head));
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let foot_y = r.1 + r.3 - FOOT;
    let body = (r.0 + 1.0, top - 4.0, r.2 - 2.0, foot_y - top + 4.0);
    let simple = f.simple;
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut cy = top;
        cy += label(f, "Cosa scarico", x, cy, w);
        segmented(f, (x, cy, w, 40.0), &[("Video", !l.audio, Act::LinkAudio(false, false)), ("Solo audio", l.audio, Act::LinkAudio(false, true))]);
        cy += 40.0 + 20.0;
        if simple {
            cy += f.para(&link_summary(l), x, cy, w, 14.0, SANS, pal.muted, 1.4, 3) + 12.0;
            return cy - top + 18.0;
        }
        cy += label(f, "Salva come", x, cy, w);
        let formats: Vec<_> = l.formats().iter().enumerate().map(|(k, s)| (s.to_uppercase(), deep, k == l.format, Act::LinkPick(false, LinkMenu::Format, k))).collect();
        cy += chips(f, x, cy, w, &formats) + 18.0;
        cy += label(f, "Qualità", x, cy, w);
        let qualities = if l.audio { AUDIO_QUALITY } else { VIDEO_QUALITY };
        let q: Vec<_> = qualities.iter().enumerate().map(|(k, s)| (s.0.to_string(), deep, k == l.quality, Act::LinkPick(false, LinkMenu::Quality, k))).collect();
        cy += chips(f, x, cy, w, &q) + 18.0;
        if !l.audio {
            f.text("Sottotitoli", x, cy, 120.0, 40.0, 15.0, SEMI, pal.text);
            f.select((x + 126.0, cy, w - 126.0, 40.0), SUBTITLES.get(l.subtitles).map(|s| s.0).unwrap_or(""), None, 14.5, Act::LinkMenu(false, LinkMenu::Subtitles));
            cy += 40.0 + 16.0;
        }
        let mut flags: Vec<(&str, &str, bool, LinkFlag)> = vec![
            ("Copertina nel file", "", l.thumbnail, LinkFlag::Thumbnail),
            ("Capitoli e descrizione nel file", "", l.chapters, LinkFlag::Chapters),
            ("Dividi per capitoli", "Un file per capitolo", l.split_chapters, LinkFlag::Split),
            ("Salta le parti sponsorizzate", "Vale per i link di YouTube", l.skip_sponsors, LinkFlag::Sponsors),
        ];
        if l.audio {
            flags.push(("Livella il volume", "", l.normalize, LinkFlag::Normalize));
        }
        flags.push(("Playlist e raccolte: scarica tutto", "", l.playlist, LinkFlag::Playlist));
        flags.push(("Salta i già scaricati", "", l.skip_existing, LinkFlag::SkipExisting));
        for (title, note, on, which) in flags {
            cy += switch_row(f, x, cy, w, title, note, on, Act::LinkFlag(false, which));
        }
        cy - top + 18.0
    });
    // Il piede: la cartella dei download (non quella delle conversioni) e il pulsante.
    f.hline(r.0 + 1.0, foot_y, r.2 - 2.0);
    f.text("Cartella di destinazione", x, foot_y + 12.0, w, 20.0, 13.0, BOLD, pal.muted);
    f.select((x, foot_y + 34.0, w, 40.0), &crate::ui::model::short_dir(&l.dest), Some(ic::FOLDER), 14.5, Act::LinkDest(false));
    let go = (x, foot_y + 88.0, w, 56.0);
    link_go(f, app, go, deep);
    if !super::notice(f, app, (x - 6.0, go.1 + go.3 + 12.0, w + 12.0, 44.0), 14.0) {
        let line = if l.note.is_empty() { link_summary(l) } else { l.note.clone() };
        let lines = f.p.wrap_lines(&crate::i18n::tr_string(&line), 14.0, SANS, w, 2);
        for (k, s) in lines.iter().enumerate() {
            f.p.text(s, x, go.1 + go.3 + 14.0 + k as f32 * 20.0, w, 20.0, 14.0, SANS, pal.muted, Align::Center);
        }
    }
}

/// «Video MP4 · Fino a 1080p»: che cosa esce, in breve.
fn link_summary(l: &crate::ui::model::Links) -> String {
    let fmt = l.formats().get(l.format).map(|s| s.to_uppercase()).unwrap_or_default();
    let q = l.qualities().get(l.quality).map(|q| q.0.split(" · ").next().unwrap_or(q.0)).unwrap_or("");
    // Ogni pezzo si traduce da sé: la riga intera non c'è nell'elenco dei testi.
    let what = crate::i18n::tr(if l.audio { "Audio {}" } else { "Video {}" }).replace("{}", &fmt);
    crate::i18n::lit(&format!("{what} · {}", crate::i18n::tr(q)))
}

/// Scarica, oppure Annulla con la barra mentre scarica.
fn link_go(f: &mut Frame, app: &App, r: R, deep: Rgb) {
    let l = &app.youtube;
    let ready = !l.items.is_empty() || !l.typed.trim().is_empty();
    if app.busy() {
        let pal = f.pal;
        f.button(r, "Annulla", Some(ic::CLOSE), 18.0, Act::Cancel);
        if let Some(p) = app.progress() {
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2, 5.0, 2.5, Some(pal.border), None);
            f.p.rrect(r.0, r.1 + r.3 + 8.0, r.2 * p.clamp(0.0, 1.0), 5.0, 2.5, Some(deep), None);
        }
        return;
    }
    let n = l.items.len();
    let label = match (n, l.audio) {
        (0, _) => "Scarica".to_string(),
        (1, false) => "Scarica 1 video".to_string(),
        (1, true) => "Scarica 1 audio".to_string(),
        (n, false) => format!("Scarica {n} video"),
        (n, true) => format!("Scarica {n} audio"),
    };
    let act = Act::Start;
    if ready {
        let h = f.hover_t(&act, true);
        let press = f.press_t(&act, true);
        let b = Frame::pressed_rect(r, press);
        f.p.shadow(b.0, b.1, b.2, b.3, 13.0, 9.0 * (1.0 + 0.3 * h - 0.6 * press), 0.26, 5.0);
        f.p.rrect_vgrad(b.0, b.1, b.2, b.3, 13.0, mix(deep, WHITE, 0.16 + 0.06 * h), mix(deep, (0, 0, 0), 0.04));
        f.focus_ring(r, 13.0, &act);
        f.hit(r, act);
    } else {
        let pal = f.pal;
        f.p.rrect(r.0, r.1, r.2, r.3, 13.0, Some(mix(deep, pal.surface, 0.6)), None);
    }
    let tw = f.measure(&label, 19.0, BOLD);
    let x = r.0 + (r.2 - tw - 38.0) / 2.0;
    f.icon(ic::DOWNLOAD, x, r.1 + (r.3 - 26.0) / 2.0, 26.0, WHITE);
    f.p.text(&label, x + 38.0, r.1, tw + 4.0, r.3, 19.0, BOLD, WHITE, Align::Left);
}

/// Un interruttore con il suo nome e una riga che spiega. Restituisce l'altezza.
#[allow(clippy::too_many_arguments)]
fn switch_row(f: &mut Frame, x: f32, y: f32, w: f32, title: &str, note: &str, on: bool, act: Act) -> f32 {
    let pal = f.pal;
    let h = if note.is_empty() { 46.0 } else { 58.0 };
    let tw = w - 68.0;
    if note.is_empty() {
        f.text(title, x, y, tw, h, 15.0, SEMI, pal.text);
    } else {
        f.text(title, x, y + 6.0, tw, 24.0, 15.0, SEMI, pal.text);
        f.text(note, x, y + 30.0, tw, 20.0, 13.0, SANS, pal.muted);
    }
    f.switch(x + w - 52.0, y + (h - 28.0) / 2.0, 52.0, 28.0, on, act.clone(), None);
    f.hit((x, y, w - 60.0, h), act);
    f.p.fill_a(x, y + h, w, 1.0, pal.border, 1.0);
    h + 1.0
}

// ---------------------------------------------------------------------------------------------
// Firma

/// Firma e verifica: la pagina di Cedro (documenti, chiavetta, verifica) su una scheda chiara grande
/// quanto la zona di lavoro, come in Cedro 4 Labs.
fn firma(f: &mut Frame, app: &App, area: R) {
    let pal = f.pal;
    f.p.rrect(area.0, area.1, area.2, area.3, 14.0, Some(pal.work), None);
    let (deep, bright) = colors(app);
    // Una fascia del colore della pagina in alto, perché si veda dove si è.
    f.p.push_clip(area.0, area.1, area.2, 6.0);
    f.p.rrect_vgrad(area.0, area.1, area.2, 20.0, 14.0, bright, deep);
    f.p.pop_clip();
    let r = (area.0 + 28.0, area.1 + 22.0, area.2 - 56.0, area.3 - 40.0);
    let _ = ICON_KEY;
    super::firma_page(f, app, r, f.simple);
}
