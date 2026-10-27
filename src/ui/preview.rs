//! Le anteprime dei file che non sono né PDF né immagini: testo, tabelle CSV e documenti diventano pagine
//! bianche disegnate qui (come la stampa), audio e video una scheda con l'onda, i fotogrammi e i dati
//! del file. Tutto in Rust, nel thread dei file.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::gfx::{Fonts, GlyphCache, Image, Painter, Rgb, Weight};
use crate::engine::{self, Kind};

/// Quello che la scheda di un file audio, video o d'altro tipo mostra.
#[derive(Clone, Default)]
pub struct About {
    /// Secondi.
    pub duration: Option<f64>,
    /// Coppie «etichetta → valore» (l'etichetta in italiano, si traduce quando si disegna).
    pub facts: Vec<(&'static str, String)>,
    /// L'onda dell'audio (0..1).
    pub wave: Option<Vec<f32>>,
    /// Il fotogramma grande, poi la striscia di scene.
    pub frames: Vec<Image>,
    /// Che cos'è il file, in parole («Archivio compresso»).
    pub kind_label: String,
    /// Ultima modifica, gg/mm/aaaa.
    pub modified: Option<String>,
}

/// Misure della pagina (A4 in punti) e margini.
const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 48.0;
const INK: Rgb = (34, 34, 34);
const SOFT: Rgb = (150, 150, 150);
const RULE: Rgb = (221, 221, 221);
const HEAD: Rgb = (241, 235, 244);
/// Oltre questa misura il testo si legge solo in parte: l'anteprima resta svelta.
const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_ROWS: usize = 5000;

enum Op {
    Text { x: f32, base: f32, s: String, size: f32, weight: Weight, color: Rgb },
    Fill { x: f32, y: f32, w: f32, h: f32, color: Rgb },
}

type Pages = Vec<Vec<Op>>;

thread_local! {
    /// L'ultima impaginazione: sfogliare le pagine non rifà tutto il lavoro.
    static LAST: RefCell<Option<(PathBuf, Option<SystemTime>, std::rc::Rc<Pages>)>> = const { RefCell::new(None) };
    static GLYPHS: RefCell<GlyphCache> = RefCell::new(GlyphCache::new());
}

fn fonts() -> &'static Fonts {
    Fonts::current()
}

fn measure(s: &str, size: f32, weight: Weight) -> f32 {
    fonts().measure(s, size, weight)
}

fn ascent(size: f32, weight: Weight) -> f32 {
    fonts().extents(size, weight).0
}

/// Le pagine del file, già impaginate (quelle dell'ultima volta se il file non è cambiato).
fn pages_of(path: &Path) -> Option<std::rc::Rc<Pages>> {
    let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some(p) = LAST.with(|l| l.borrow().as_ref().filter(|(lp, ls, _)| lp == path && *ls == stamp).map(|(_, _, p)| p.clone())) {
        return Some(p);
    }
    let pages = std::rc::Rc::new(layout(path)?);
    LAST.with(|l| *l.borrow_mut() = Some((path.to_path_buf(), stamp, pages.clone())));
    Some(pages)
}

fn layout(path: &Path) -> Option<Pages> {
    match engine::kind_of(path) {
        Kind::Text => {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut std::io::Read::take(std::fs::File::open(path).ok()?, MAX_TEXT as u64), &mut bytes).ok()?;
            let text = engine::decode_text(&bytes);
            match engine::extension(path).as_str() {
                "csv" | "tsv" => Some(table_pages(&text)),
                "json" => Some(text_pages(&crate::textconv::json_format(&text, true).unwrap_or(text))),
                _ => Some(text_pages(&text)),
            }
        }
        Kind::Doc => Some(doc_pages(&crate::doctext::document_blocks(path).ok()?)),
        _ => None,
    }
}

/// Quante pagine ha l'anteprima di un file di testo o di un documento.
pub fn page_count(path: &Path) -> Option<usize> {
    pages_of(path).map(|p| p.len().max(1))
}

/// Una pagina dell'anteprima, con il lato lungo di `side` pixel.
pub fn render_page(path: &Path, page: usize, side: f32) -> Option<Image> {
    let pages = pages_of(path)?;
    let ops = pages.get(page.min(pages.len().saturating_sub(1)))?;
    let scale = side / PAGE_H;
    GLYPHS.with(|g| {
        let mut cache = g.borrow_mut();
        let mut p = Painter::new((PAGE_W * scale).round() as u32, side.round() as u32, scale, fonts(), &mut cache);
        p.clear((255, 255, 255));
        p.push_clip(MARGIN * 0.5, MARGIN * 0.5, PAGE_W - MARGIN, PAGE_H - MARGIN);
        for op in ops {
            match op {
                Op::Text { x, base, s, size, weight, color } => p.text_at(s, *x, *base, *size, *weight, *color),
                Op::Fill { x, y, w, h, color } => p.fill(*x, *y, *w, *h, *color),
            }
        }
        p.pop_clip();
        Some(Image::from_painter(p))
    })
}

// ---- Testo ----

/// Testo com'è, con i numeri di riga; le righe lunghe vanno a capo senza numero.
fn text_pages(text: &str) -> Pages {
    const SIZE: f32 = 10.0;
    const LINE: f32 = 14.5;
    let cw = measure("M", SIZE, Weight::Mono).max(1.0);
    let lines: Vec<&str> = text.lines().collect();
    let digits = lines.len().max(1).to_string().len().max(2);
    let gutter = cw * (digits as f32 + 1.5);
    let cols = (((PAGE_W - 2.0 * MARGIN - gutter) / cw).floor() as usize).max(10);
    let per_page = ((PAGE_H - 2.0 * MARGIN) / LINE).floor() as usize;
    let asc = ascent(SIZE, Weight::Mono);
    let mut pages: Pages = Vec::new();
    let mut page: Vec<Op> = Vec::new();
    let mut row = 0usize;
    let put = |page: &mut Vec<Op>, row: &mut usize, pages: &mut Pages, number: Option<usize>, s: String| {
        if *row == per_page {
            pages.push(std::mem::take(page));
            *row = 0;
        }
        let base = MARGIN + *row as f32 * LINE + asc;
        if let Some(n) = number {
            let label = n.to_string();
            let x = MARGIN + gutter - cw * 1.5 - measure(&label, SIZE, Weight::Mono);
            page.push(Op::Text { x, base, s: label, size: SIZE, weight: Weight::Mono, color: SOFT });
        }
        if !s.is_empty() {
            page.push(Op::Text { x: MARGIN + gutter, base, s, size: SIZE, weight: Weight::Mono, color: INK });
        }
        *row += 1;
    };
    for (n, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.replace('\t', "    ").chars().filter(|c| !c.is_control()).collect();
        if chars.is_empty() {
            put(&mut page, &mut row, &mut pages, Some(n + 1), String::new());
            continue;
        }
        for (i, chunk) in chars.chunks(cols).enumerate() {
            put(&mut page, &mut row, &mut pages, (i == 0).then_some(n + 1), chunk.iter().collect());
        }
    }
    if !page.is_empty() || pages.is_empty() {
        pages.push(page);
    }
    pages
}

// ---- Tabelle ----

fn is_number(s: &str) -> bool {
    let t: String = s.trim().chars().filter(|c| !matches!(c, ' ' | '€' | '%' | '$' | '\'')).collect();
    !t.is_empty() && t.chars().any(|c| c.is_ascii_digit()) && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+'))
}

/// CSV e TSV come tabella: la prima riga fa da intestazione e torna in cima a ogni pagina.
fn table_pages(text: &str) -> Pages {
    const SIZE: f32 = 9.0;
    const ROW: f32 = 19.0;
    const PAD: f32 = 5.0;
    let sep = crate::textconv::detect_separator(text);
    let mut rows = crate::textconv::parse_csv(text, sep);
    rows.truncate(MAX_ROWS);
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    if rows.is_empty() {
        return text_pages(text);
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let avail = PAGE_W - 2.0 * MARGIN;
    let mut widths = vec![30.0f32; cols];
    for (i, r) in rows.iter().take(200).enumerate() {
        for (c, cell) in r.iter().enumerate() {
            let w = measure(cell.trim(), SIZE, if i == 0 { Weight::Semibold } else { Weight::Regular }) + 2.0 * PAD;
            widths[c] = widths[c].max(w.min(180.0));
        }
    }
    let total: f32 = widths.iter().sum();
    if total > avail {
        widths.iter_mut().for_each(|w| *w *= avail / total);
    }
    let table_w: f32 = widths.iter().sum();
    let asc = ascent(SIZE, Weight::Regular);
    let fit = |s: &str, w: f32, weight: Weight| -> String {
        let s = s.trim().replace(['\n', '\r'], " ");
        if measure(&s, SIZE, weight) <= w {
            return s;
        }
        let mut out: String = String::new();
        for ch in s.chars() {
            if measure(&format!("{out}{ch}…"), SIZE, weight) > w {
                break;
            }
            out.push(ch);
        }
        out + "…"
    };
    let draw_row = |page: &mut Vec<Op>, y: f32, r: &[String], header: bool| {
        if header {
            page.push(Op::Fill { x: MARGIN, y, w: table_w, h: ROW, color: HEAD });
        }
        let weight = if header { Weight::Semibold } else { Weight::Regular };
        let mut x = MARGIN;
        for (c, w) in widths.iter().enumerate() {
            let cell = r.get(c).map(String::as_str).unwrap_or("");
            let s = fit(cell, w - 2.0 * PAD, weight);
            let tx = if !header && is_number(cell) { x + w - PAD - measure(&s, SIZE, weight) } else { x + PAD };
            page.push(Op::Text { x: tx, base: y + (ROW - SIZE * 1.2) / 2.0 + asc, s, size: SIZE, weight, color: INK });
            x += w;
        }
    };
    let grid = |page: &mut Vec<Op>, top: f32, n: usize| {
        let bottom = top + n as f32 * ROW;
        for i in 0..=n {
            page.push(Op::Fill { x: MARGIN, y: top + i as f32 * ROW, w: table_w, h: 0.6, color: RULE });
        }
        let mut x = MARGIN;
        for w in widths.iter().chain(std::iter::once(&0.0)) {
            page.push(Op::Fill { x, y: top, w: 0.6, h: bottom - top, color: RULE });
            x += w;
        }
    };
    let per_page = (((PAGE_H - 2.0 * MARGIN) / ROW).floor() as usize).max(3) - 1;
    let (head, body) = (&rows[0], &rows[1..]);
    let mut pages = Vec::new();
    let chunks: Vec<&[Vec<String>]> = if body.is_empty() { vec![&[][..]] } else { body.chunks(per_page).collect() };
    for chunk in chunks {
        let mut page = Vec::new();
        draw_row(&mut page, MARGIN, head, true);
        for (i, r) in chunk.iter().enumerate() {
            draw_row(&mut page, MARGIN + (i + 1) as f32 * ROW, r, false);
        }
        grid(&mut page, MARGIN, chunk.len() + 1);
        pages.push(page);
    }
    pages
}

// ---- Documenti ----

fn wrap(s: &str, size: f32, weight: Weight, w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && measure(&next, size, weight) > w {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        } else {
            cur = next;
        }
        // Una parola più lunga della riga si spezza.
        while measure(&cur, size, weight) > w && cur.chars().count() > 1 {
            let mut head = String::new();
            for ch in cur.chars() {
                if measure(&format!("{head}{ch}"), size, weight) > w {
                    break;
                }
                head.push(ch);
            }
            if head.is_empty() {
                break;
            }
            cur = cur[head.len()..].to_string();
            lines.push(head);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Word, ODT, RTF e DOC: titoli e paragrafi su pagine come la stampa.
fn doc_pages(blocks: &[crate::pdfdoc::DocBlock]) -> Pages {
    let width = PAGE_W - 2.0 * MARGIN;
    let bottom = PAGE_H - MARGIN;
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut y = MARGIN;
    for b in blocks {
        let (size, weight, before, after) = match b.heading {
            1 => (18.0, Weight::SerifBold, 10.0, 6.0),
            2 => (14.0, Weight::SerifBold, 9.0, 4.0),
            3 => (12.0, Weight::SerifBold, 8.0, 3.0),
            _ => (10.5, Weight::Serif, 0.0, 6.0),
        };
        let line = size * 1.45;
        if b.text.trim().is_empty() {
            y += line * 0.6;
            continue;
        }
        if y > MARGIN {
            y += before;
        }
        let asc = ascent(size, weight);
        let lines = wrap(&b.text, size, weight, width);
        // Un titolo non resta da solo in fondo alla pagina.
        let keep = if b.heading > 0 { line * (lines.len() as f32 + 2.0) } else { line };
        if y + keep > bottom && !page.is_empty() {
            pages.push(std::mem::take(&mut page));
            y = MARGIN;
        }
        for l in lines {
            if y + line > bottom {
                pages.push(std::mem::take(&mut page));
                y = MARGIN;
            }
            page.push(Op::Text { x: MARGIN, base: y + asc, s: l, size, weight, color: INK });
            y += line;
        }
        y += after;
    }
    if !page.is_empty() || pages.is_empty() {
        pages.push(page);
    }
    pages
}

// ---- Audio, video e altri file ----

/// I dati di un file audio o video letti dal contenitore: durata, codifica, canali, frequenza.
struct Probe {
    duration: Option<f64>,
    audio: Option<(String, Option<usize>, Option<u32>)>,
}

fn probe(path: &Path) -> Option<Probe> {
    use symphonia::core::codecs::CODEC_TYPE_NULL;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&engine::extension(path));
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
    let mut duration: Option<f64> = None;
    let mut audio = None;
    for t in probed.format.tracks() {
        let p = &t.codec_params;
        if let (Some(n), Some(tb)) = (p.n_frames, p.time_base) {
            let time = tb.calc_time(n);
            let secs = time.seconds as f64 + time.frac;
            if secs > 0.0 {
                duration = Some(duration.map_or(secs, |d| d.max(secs)));
            }
        }
        if p.codec != CODEC_TYPE_NULL && audio.is_none() {
            let name = symphonia::default::get_codecs().get_codec(p.codec).map(|d| d.short_name.to_uppercase()).unwrap_or_default();
            let name = match name.as_str() {
                n if n.starts_with("PCM") => "PCM".to_string(),
                "VORBIS" => "Vorbis".to_string(),
                _ => name,
            };
            audio = Some((name, p.channels.map(|c| c.count()), p.sample_rate));
        }
    }
    Some(Probe { duration, audio })
}

fn channels_word(n: Option<usize>) -> String {
    match n {
        Some(1) => crate::i18n::tr_string("mono"),
        Some(2) => crate::i18n::tr_string("stereo"),
        Some(n) => crate::i18n::tr_string(&format!("{n} canali")),
        None => String::new(),
    }
}

/// Minuti e secondi (o ore) di una durata.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn khz(rate: u32) -> String {
    let k = rate as f64 / 1000.0;
    if (k - k.round()).abs() < 0.05 { format!("{} kHz", k.round()) } else { format!("{k:.1} kHz").replace('.', ",") }
}

pub fn about_audio(path: &Path, bytes: u64) -> About {
    let p = probe(path);
    let duration = p.as_ref().and_then(|p| p.duration);
    let mut facts = Vec::new();
    if let Some((name, ch, rate)) = p.as_ref().and_then(|p| p.audio.clone()) {
        let ch = channels_word(ch);
        facts.push(("Formato", if ch.is_empty() { name } else { format!("{name}, {ch}") }));
        if let Some(r) = rate {
            facts.push(("Frequenza", khz(r)));
        }
    }
    if let Some(d) = duration.filter(|d| *d > 0.5) {
        facts.push(("Qualità", format!("{} kbps", (bytes as f64 * 8.0 / d / 1000.0).round() as u64)));
    }
    facts.push(("Peso", super::model::human_size(bytes)));
    About { duration, facts, wave: crate::audio::waveform(path, 160), ..Default::default() }
}

/// Il nome della codifica video, letto dall'intestazione del file.
fn video_codec(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(256 * 1024).read_to_end(&mut head).ok()?;
    if head.len() >= 8 && &head[4..8] == b"ftyp" {
        let file = std::fs::File::open(path).ok()?;
        let size = file.metadata().ok()?.len();
        if let Ok(reader) = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size) {
            for t in reader.tracks().values() {
                match t.media_type() {
                    Ok(mp4::MediaType::H264) => return Some("H.264".into()),
                    Ok(mp4::MediaType::H265) => return Some("H.265".into()),
                    Ok(mp4::MediaType::VP9) => return Some("VP9".into()),
                    _ => {}
                }
            }
        }
    }
    let find = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    for (needle, name) in [
        (&b"V_MPEG4/ISO/AVC"[..], "H.264"),
        (b"V_MPEGH/ISO/HEVC", "H.265"),
        (b"V_VP9", "VP9"),
        (b"V_VP8", "VP8"),
        (b"V_AV1", "AV1"),
        (b"avc1", "H.264"),
        (b"hvc1", "H.265"),
        (b"hev1", "H.265"),
        (b"vp09", "VP9"),
        (b"av01", "AV1"),
        (b"H264", "H.264"),
        (b"h264", "H.264"),
        (b"XVID", "Xvid"),
        (b"DIVX", "DivX"),
        (b"DX50", "DivX"),
        (b"MJPG", "Motion JPEG"),
    ] {
        if find(needle) {
            return Some(name.into());
        }
    }
    None
}

/// Fotogrammi del video ai secondi chiesti e misura vera, dal decodificatore del sistema su cui gira Cedro.
#[allow(unused_variables)]
fn frames_at(path: &Path, at: &[f64]) -> (Vec<(u32, u32, Vec<u8>)>, Option<(u32, u32)>) {
    #[cfg(windows)]
    let peek = crate::video::peek_frames(path, at);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let peek = crate::linux_video::peek_frames(path, at);
    #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
    let peek: Result<crate::softvideo::Peek, String> = crate::softvideo::peek(path, at);
    match peek {
        Ok(p) => (p.frames, Some(p.size).filter(|s| s.0 > 0 && s.1 > 0)),
        Err(_) => (Vec::new(), None),
    }
}

fn mp4_info(path: &Path) -> Option<((u32, u32), f64, f64)> {
    let file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let reader = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).ok()?;
    let t = reader.tracks().values().find(|t| matches!(t.track_type(), Ok(mp4::TrackType::Video)))?;
    Some(((t.width() as u32, t.height() as u32), t.frame_rate(), t.duration().as_secs_f64()))
}

pub fn about_video(path: &Path, bytes: u64) -> About {
    let p = probe(path);
    let mp4 = mp4_info(path);
    let duration = p.as_ref().and_then(|p| p.duration).or(mp4.map(|m| m.2)).filter(|d| *d > 0.0);
    // Il fotogramma grande non è il primo, che spesso è nero; poi la striscia lungo tutto il video.
    let at: Vec<f64> = match duration {
        Some(d) => vec![(d * 0.1).min(10.0), d * 0.3, d * 0.5, d * 0.7, d * 0.9],
        None => vec![1.0],
    };
    let (raw, decoded) = frames_at(path, &at);
    let mut facts = Vec::new();
    let size = mp4.map(|m| m.0).filter(|s| s.0 > 0 && s.1 > 0).or(decoded);
    let fps = mp4.map(|m| m.1).filter(|f| *f > 0.0);
    if let Some((w, h)) = size {
        let fps = fps.map(|f| format!(", {} fps", if (f - f.round()).abs() < 0.05 { format!("{}", f.round()) } else { format!("{f:.2}").replace('.', ",") })).unwrap_or_default();
        facts.push(("Immagine", format!("{w} × {h}{fps}")));
    }
    let audio = p.as_ref().and_then(|p| p.audio.as_ref()).map(|a| a.0.clone()).filter(|a| !a.is_empty());
    let codec = match (video_codec(path), audio) {
        (Some(v), Some(a)) => Some(format!("{v} + {a}")),
        (Some(v), None) => Some(v),
        (None, Some(a)) => Some(crate::i18n::tr_string(&format!("audio {a}"))),
        (None, None) => None,
    };
    if let Some(c) = codec {
        facts.push(("Codec", c));
    }
    facts.push(("Peso", super::model::human_size(bytes)));
    let frames = raw.into_iter().filter_map(|(w, h, rgba)| Image::from_rgba(w, h, &rgba)).collect();
    About { duration, facts, frames, ..Default::default() }
}

fn kind_label(path: &Path) -> String {
    let ext = engine::extension(path);
    let words = match ext.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" => "Archivio compresso",
        "exe" | "msi" | "appimage" | "deb" | "rpm" | "apk" | "flatpak" => "Programma",
        "xls" | "xlsx" | "ods" => "Foglio di calcolo",
        "ppt" | "pptx" | "odp" => "Presentazione",
        "eml" | "msg" => "Messaggio di posta",
        "svg" | "psd" | "ai" | "heic" | "avif" | "raw" | "cr2" | "nef" => "Immagine",
        "ttf" | "otf" | "woff" | "woff2" => "Carattere",
        "" => "File senza estensione",
        _ => return crate::i18n::tr_string(&format!("File di tipo {}", ext.to_uppercase())),
    };
    crate::i18n::tr_string(words)
}

/// gg/mm/aaaa di un istante (calendario gregoriano, ora UTC).
fn date(t: SystemTime) -> Option<String> {
    let days = (t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    Some(format!("{d:02}/{m:02}/{y}"))
}

pub fn about_other(path: &Path) -> About {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(date);
    About { kind_label: kind_label(path), modified, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_clock() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_792_800_000);
        assert_eq!(date(t).as_deref(), Some("24/10/2026"));
        assert_eq!(clock(768.0), "12:48");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    #[test]
    fn text_csv_and_doc_pages() {
        let long: String = (1..=120).map(|i| format!("riga {i}\n")).collect();
        assert!(text_pages(&long).len() >= 2);
        let csv = "Mese;Pezzi\nGennaio;412\nFebbraio;455\n";
        let pages = table_pages(csv);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].iter().any(|o| matches!(o, Op::Text { s, .. } if s == "412")));
        let blocks: Vec<crate::pdfdoc::DocBlock> = (0..200).map(|i| crate::pdfdoc::DocBlock { text: format!("Paragrafo {i} con un po' di testo per andare a capo."), heading: if i % 20 == 0 { 1 } else { 0 } }).collect();
        assert!(doc_pages(&blocks).len() >= 3);
        assert!(is_number("1.234,50 €") && !is_number("Nord"));
    }
}
