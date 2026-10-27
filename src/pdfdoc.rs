//! PDF di testo con i caratteri incorporati (TrueType, testo copiabile con ToUnicode), i segnalibri
//! dai titoli e, su richiesta, il formato PDF/A-1b: metadati XMP, profilo colore sRGB e font incorporati.

use std::collections::BTreeMap;
use std::io::Write;

use skrifa::instance::{LocationRef, Size};
use skrifa::charmap::Charmap;
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::engine::PageSize;

static REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");

/// Un paragrafo del documento; `heading` 1-3 per i titoli, 0 per il testo. Con `image` è un'immagine (il testo è vuoto).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocBlock {
    pub text: String,
    pub heading: u8,
    pub image: Option<DocImage>,
}

/// Un'immagine del documento, pronta per il PDF: un JPEG così com'è oppure i pixel (RGB o grigi) da comprimere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocImage {
    pub data: Vec<u8>,
    pub jpeg: bool,
    pub gray: bool,
    pub px: (u32, u32),
    /// Dimensione nel documento in centesimi di punto; (0, 0) se non è scritta (vale 96 dpi).
    pub size_cpt: (u32, u32),
    /// 0 a sinistra, 1 al centro, 2 a destra.
    pub align: u8,
}

impl DocImage {
    /// Dai byte del file (JPEG, PNG, GIF, BMP, WebP, TIFF). `size_pt`: la dimensione scritta nel documento.
    pub fn from_bytes(bytes: &[u8], size_pt: Option<(f32, f32)>, align: u8) -> Option<DocImage> {
        let size_cpt = size_pt.filter(|(w, h)| *w > 0.5 && *h > 0.5).map_or((0, 0), |(w, h)| ((w * 100.0) as u32, (h * 100.0) as u32));
        // Un JPEG RGB o grigio va nel PDF com'è: niente perdita di qualità e file piccolo.
        if let Some((w, h, comps)) = jpeg_info(bytes) {
            if comps == 1 || comps == 3 {
                return Some(DocImage { data: bytes.to_vec(), jpeg: true, gray: comps == 1, px: (w, h), size_cpt, align });
            }
        }
        let img = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.decode().ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        // La trasparenza va sul bianco della pagina.
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for p in rgba.pixels() {
            let a = p[3] as u32;
            for c in 0..3 {
                data.push(((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        Some(DocImage { data, jpeg: false, gray: false, px: (w, h), size_cpt, align })
    }

    /// Larghezza e altezza in punti.
    fn points(&self) -> (f32, f32) {
        if self.size_cpt.0 > 0 && self.size_cpt.1 > 0 {
            (self.size_cpt.0 as f32 / 100.0, self.size_cpt.1 as f32 / 100.0)
        } else {
            (self.px.0 as f32 * 0.75, self.px.1 as f32 * 0.75)
        }
    }
}

/// Larghezza, altezza e componenti di colore di un JPEG (dal segmento SOF).
fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if matches!(m, 0xD8 | 0x01 | 0xD0..=0xD7) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return (w > 0 && h > 0).then_some((w, h, b[i + 9]));
        }
        i += 2 + len;
    }
    None
}

pub struct PdfOptions {
    pub size: PageSize,
    /// Font TrueType incorporato; altrimenti Helvetica, che il lettore ha già.
    pub embed: bool,
    pub bookmarks: bool,
    pub pdfa: bool,
    pub title: String,
}

struct Face {
    data: &'static [u8],
    name: &'static str,
    upem: f32,
    /// glifo → carattere, per i soli caratteri usati.
    used: BTreeMap<u16, char>,
    /// Tabelle del font lette una volta sola: si misurano tutti i caratteri del documento.
    charmap: Charmap<'static>,
    metrics: GlyphMetrics<'static>,
}

impl Face {
    fn new(data: &'static [u8], name: &'static str) -> Result<Face, String> {
        let font = FontRef::new(data).map_err(|e| format!("font non letto: {e}"))?;
        let upem = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as f32;
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(Size::unscaled(), LocationRef::default()));
        Ok(Face { data, name, upem, used: BTreeMap::new(), charmap, metrics })
    }

    fn font(&self) -> FontRef<'static> {
        FontRef::new(self.data).expect("font già letto")
    }

    fn glyph(&self, c: char) -> u16 {
        self.charmap.map(c).map_or(0, |g| g.to_u32() as u16)
    }

    /// Larghezza in unità di 1/1000 em.
    fn width(&self, gid: u16) -> f32 {
        self.metrics.advance_width(GlyphId::new(gid as u32)).unwrap_or(0.0) * 1000.0 / self.upem
    }

    /// Larghezza di un carattere in unità di 1/1000 em.
    fn char_width(&self, c: char) -> f32 {
        self.width(self.glyph(c))
    }
}

struct Line {
    text: String,
    bold: bool,
    size: f32,
    y: f32,
}

struct Page {
    lines: Vec<Line>,
    /// Immagini: indice, x, y (in basso), larghezza, altezza.
    images: Vec<(usize, f32, f32, f32, f32)>,
}

impl Page {
    fn new() -> Page {
        Page { lines: Vec::new(), images: Vec::new() }
    }
    fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.images.is_empty()
    }
}

struct Mark {
    level: u8,
    title: String,
    page: usize,
    y: f32,
}

struct Builder {
    objects: Vec<Vec<u8>>,
}

impl Builder {
    fn reserve(&mut self) -> usize {
        self.objects.push(Vec::new());
        self.objects.len()
    }
    fn set(&mut self, id: usize, body: Vec<u8>) {
        self.objects[id - 1] = body;
    }
    fn add(&mut self, body: Vec<u8>) -> usize {
        self.objects.push(body);
        self.objects.len()
    }
}

fn stream(dict: &str, data: &[u8], compress: bool) -> Vec<u8> {
    let (extra, body) = if compress {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = enc.write_all(data);
        (" /Filter /FlateDecode", enc.finish().unwrap_or_else(|_| data.to_vec()))
    } else {
        ("", data.to_vec())
    };
    let mut out = format!("<< {dict}{extra} /Length {} >>\nstream\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out.extend_from_slice(b"\nendstream");
    out
}

fn pdf_string(text: &str) -> String {
    // UTF-16BE con BOM: vale per ogni carattere.
    let mut out = String::from("<FEFF");
    for u in text.encode_utf16() {
        out.push_str(&format!("{u:04X}"));
    }
    out.push('>');
    out
}

fn date_now() -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86400) as i64, secs % 86400);
    // Da giorni dal 1970 a data civile (algoritmo di Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    let (h, m, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    (
        format!("D:{year:04}{month:02}{day:02}{h:02}{m:02}{s:02}Z"),
        format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z"),
    )
}

/// Il profilo sRGB (ICC v2, matrici e curve a gamma 2,2) che il PDF/A chiede come profilo di uscita.
fn srgb_profile() -> Vec<u8> {
    fn s15(v: f64) -> [u8; 4] {
        ((v * 65536.0).round() as i32).to_be_bytes()
    }
    fn xyz(v: [f64; 3]) -> Vec<u8> {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        for c in v {
            t.extend_from_slice(&s15(c));
        }
        t
    }
    fn text(s: &str) -> Vec<u8> {
        let mut t = b"text\0\0\0\0".to_vec();
        t.extend_from_slice(s.as_bytes());
        t.push(0);
        t
    }
    let description = {
        let name = b"sRGB IEC61966-2.1\0";
        let mut t = b"desc\0\0\0\0".to_vec();
        t.extend_from_slice(&(name.len() as u32).to_be_bytes());
        t.extend_from_slice(name);
        t.extend_from_slice(&[0u8; 8]); // codice Unicode e lunghezza
        t.extend_from_slice(&[0u8; 3]); // codice ScriptCode e lunghezza
        t.extend_from_slice(&[0u8; 67]);
        t
    };
    let gamma = {
        let mut t = b"curv\0\0\0\0".to_vec();
        t.extend_from_slice(&1u32.to_be_bytes());
        t.extend_from_slice(&0x0233u16.to_be_bytes()); // 2,2 in u8Fixed8
        t
    };
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", description),
        (b"cprt", text("Cedro, libero uso")),
        (b"wtpt", xyz([0.9642, 1.0, 0.8249])),
        (b"rXYZ", xyz([0.43607, 0.22249, 0.01392])),
        (b"gXYZ", xyz([0.38515, 0.71687, 0.09708])),
        (b"bXYZ", xyz([0.14307, 0.06061, 0.71410])),
        (b"rTRC", gamma.clone()),
        (b"gTRC", gamma.clone()),
        (b"bTRC", gamma),
    ];
    let table_len = 4 + tags.len() * 12;
    let mut offset = 128 + table_len;
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut data = Vec::new();
    for (sig, body) in &tags {
        table.extend_from_slice(*sig);
        table.extend_from_slice(&(offset as u32).to_be_bytes());
        table.extend_from_slice(&(body.len() as u32).to_be_bytes());
        data.extend_from_slice(body);
        while data.len() % 4 != 0 {
            data.push(0);
        }
        offset = 128 + table_len + data.len();
    }
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&((128 + table_len + data.len()) as u32).to_be_bytes());
    header[8..12].copy_from_slice(&0x0210_0000u32.to_be_bytes()); // versione 2.1
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(b"RGB ");
    header[20..24].copy_from_slice(b"XYZ ");
    header[24..36].copy_from_slice(&[0x07, 0xE8, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]); // 1 gennaio 2024
    header[36..40].copy_from_slice(b"acsp");
    header[68..72].copy_from_slice(&s15(0.9642));
    header[72..76].copy_from_slice(&s15(1.0));
    header[76..80].copy_from_slice(&s15(0.8249));
    let mut out = header;
    out.extend_from_slice(&table);
    out.extend_from_slice(&data);
    out
}

fn xmp(title: &str, iso_date: &str) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
<rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n\
<pdfaid:part>1</pdfaid:part>\n<pdfaid:conformance>B</pdfaid:conformance>\n</rdf:Description>\n\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{t}</rdf:li></rdf:Alt></dc:title>\n</rdf:Description>\n\
<rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n\
<pdf:Producer>Cedro</pdf:Producer>\n</rdf:Description>\n\
<rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n\
<xmp:CreateDate>{d}</xmp:CreateDate>\n<xmp:ModifyDate>{d}</xmp:ModifyDate>\n<xmp:CreatorTool>Cedro</xmp:CreatorTool>\n</rdf:Description>\n\
</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>",
        t = esc(title),
        d = iso_date
    )
}

fn cmap(face: &Face) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // Il glifo 0 (quadratino) è di tutti i caratteri che il font non ha: non dice quale copiare.
    let entries: Vec<(&u16, &char)> = face.used.iter().filter(|(gid, _)| **gid != 0).collect();
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, c) in chunk {
            let mut units = [0u16; 2];
            let hex: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
            out.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

/// Spezza un paragrafo in righe che stanno nella `max` larghezza (parole, e a metà parola se serve).
/// Le larghezze si sommano carattere per carattere mentre la riga cresce (non c'è crenatura):
/// niente da rimisurare a ogni parola.
fn wrap(face: &Face, text: &str, size: f32, scale: f32, max: f32) -> Vec<String> {
    let fits = |units: f32| units * size / 1000.0 * scale <= max;
    let space = face.char_width(' ');
    let mut lines = Vec::new();
    let mut current = String::new();
    // Larghezza di `current` in 1/1000 em.
    let mut current_units = 0.0f32;
    let mut widths: Vec<f32> = Vec::new();
    for word in text.split(' ') {
        widths.clear();
        widths.extend(word.chars().map(|c| face.char_width(c)));
        let start = if current.is_empty() { 0.0 } else { current_units + space };
        let candidate = widths.iter().fold(start, |sum, w| sum + w);
        if fits(candidate) || current.is_empty() && word.is_empty() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            current_units = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        // Una parola più larga della riga si spezza.
        let mut piece = String::new();
        let mut piece_units = 0.0f32;
        for (c, w) in word.chars().zip(&widths) {
            if !fits(piece_units + w) && !piece.is_empty() {
                lines.push(std::mem::take(&mut piece));
                piece_units = 0.0;
            }
            piece.push(c);
            piece_units += w;
        }
        current = piece;
        current_units = piece_units;
    }
    lines.push(current);
    lines
}

pub fn build(blocks: &[DocBlock], o: &PdfOptions) -> Result<Vec<u8>, String> {
    let embed = o.embed || o.pdfa;
    let (width, height): (f32, f32) = match o.size {
        PageSize::A4 => (595.0, 842.0),
        PageSize::A3 => (842.0, 1191.0),
        PageSize::Letter => (612.0, 792.0),
    };
    let margin = 56.7f32;
    let mut regular = Face::new(REGULAR, "TitilliumWeb-Regular")?;
    let mut bold = Face::new(BOLD, "TitilliumWeb-Bold")?;
    // Senza font incorporato il lettore usa Helvetica, un poco più larga: si lascia margine.
    let scale = if embed { 1.0 } else { 1.12 };
    let max = width - 2.0 * margin;

    // ---- impaginazione
    let mut pages: Vec<Page> = vec![Page::new()];
    let mut marks: Vec<Mark> = Vec::new();
    let mut y = height - margin;
    let mut imgs: Vec<&DocImage> = Vec::new();
    for block in blocks {
        if let Some(img) = &block.image {
            // Grande al massimo quanto lo spazio della pagina, mai ingrandita.
            let (w0, h0) = img.points();
            let s = (max / w0).min((height - 2.0 * margin) / h0).min(1.0);
            let (w, h) = (w0 * s, h0 * s);
            let gap = 6.0;
            if y - h - gap < margin && !pages.last().map_or(true, Page::is_empty) {
                pages.push(Page::new());
                y = height - margin;
            } else if !pages.last().map_or(true, Page::is_empty) {
                y -= gap;
            }
            y -= h;
            let x = margin + (max - w) * img.align.min(2) as f32 / 2.0;
            pages.last_mut().expect("pagina").images.push((imgs.len(), x, y, w, h));
            imgs.push(img);
            y -= gap;
            continue;
        }
        let (size, is_bold, before) = match block.heading {
            1 => (20.0f32, true, 12.0f32),
            2 => (16.0, true, 9.0),
            3 => (13.5, true, 6.0),
            _ => (11.0, false, 0.0),
        };
        let leading = size * 1.3;
        let face = if is_bold { &bold } else { &regular };
        // Le tabulazioni diventano spazi e gli altri caratteri di controllo spariscono: nel font non hanno un glifo (sarebbero quadratini).
        let text: String = block.text.replace('\t', "    ").chars().filter(|c| !c.is_control()).collect();
        let wrapped = if text.is_empty() { vec![String::new()] } else { wrap(face, &text, size, scale, max) };
        // Un titolo non resta solo in fondo alla pagina.
        let need = leading * if block.heading > 0 { 2.5 } else { 1.0 } + before;
        if y - need < margin && !pages.last().map_or(true, Page::is_empty) {
            pages.push(Page::new());
            y = height - margin;
        } else if !pages.last().map_or(true, Page::is_empty) {
            y -= before;
        }
        if block.heading > 0 && !text.trim().is_empty() {
            marks.push(Mark { level: block.heading, title: text.trim().to_string(), page: pages.len() - 1, y: y.min(height - 10.0) });
        }
        for line in wrapped {
            if y - leading < margin && !pages.last().map_or(true, Page::is_empty) {
                pages.push(Page::new());
                y = height - margin;
            }
            y -= leading;
            pages.last_mut().expect("pagina").lines.push(Line { text: line, bold: is_bold, size, y });
        }
    }

    // ---- oggetti
    let mut b = Builder { objects: Vec::new() };
    let catalog = b.reserve();
    let pages_id = b.reserve();
    let page_ids: Vec<(usize, usize)> = pages.iter().map(|_| (b.reserve(), b.reserve())).collect();
    let (font1, font2) = (b.reserve(), b.reserve());
    let img_ids: Vec<usize> = imgs.iter().map(|_| b.reserve()).collect();
    let mut resources_fonts = format!("/F1 {font1} 0 R /F2 {font2} 0 R");

    // contenuti (e glifi usati)
    for (i, page) in pages.iter().enumerate() {
        let mut ops = String::from("0 g\n");
        for (k, x, y, w, h) in &page.images {
            ops.push_str(&format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im{k} Do Q\n"));
        }
        for line in &page.lines {
            if line.text.is_empty() {
                continue;
            }
            let face = if line.bold { &mut bold } else { &mut regular };
            let name = if line.bold { "F2" } else { "F1" };
            let shown = if embed {
                let mut hex = String::from("<");
                for c in line.text.chars() {
                    let gid = face.glyph(c);
                    face.used.entry(gid).or_insert(c);
                    hex.push_str(&format!("{gid:04X}"));
                }
                hex.push('>');
                hex
            } else {
                let mut lit = String::from("(");
                for byte in crate::pdf::win_ansi(&line.text) {
                    match byte {
                        b'(' | b')' | b'\\' => {
                            lit.push('\\');
                            lit.push(byte as char);
                        }
                        0x20..=0x7E => lit.push(byte as char),
                        n => lit.push_str(&format!("\\{n:03o}")),
                    }
                }
                lit.push(')');
                lit
            };
            ops.push_str(&format!("BT /{name} {} Tf 1 0 0 1 {} {:.2} Tm {shown} Tj ET\n", line.size, margin, line.y));
        }
        let (page_id, content_id) = page_ids[i];
        b.set(content_id, stream("", ops.as_bytes(), true));
        let xobjects = if page.images.is_empty() {
            String::new()
        } else {
            format!(" /XObject << {} >>", page.images.iter().map(|(k, ..)| format!("/Im{k} {} 0 R", img_ids[*k])).collect::<Vec<_>>().join(" "))
        };
        b.set(
            page_id,
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {width} {height}] /Contents {content_id} 0 R /Resources << /Font << {resources_fonts} >>{xobjects} >> >>"
            )
            .into_bytes(),
        );
    }

    // immagini
    for (img, id) in imgs.iter().zip(&img_ids) {
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8",
            img.px.0,
            img.px.1,
            if img.gray { "/DeviceGray" } else { "/DeviceRGB" }
        );
        b.set(*id, if img.jpeg { stream(&format!("{dict} /Filter /DCTDecode"), &img.data, false) } else { stream(&dict, &img.data, true) });
    }

    // font
    if embed {
        for (id, face) in [(font1, &regular), (font2, &bold)] {
            let font = face.font();
            let metrics = font.metrics(Size::unscaled(), LocationRef::default());
            let scale_em = 1000.0 / face.upem;
            let bbox = metrics.bounds.map_or([-200.0, -300.0, 1200.0, 1000.0], |b| {
                [b.x_min * scale_em, b.y_min * scale_em, b.x_max * scale_em, b.y_max * scale_em]
            });
            let file = b.add(stream(&format!("/Length1 {}", face.data.len()), face.data, true));
            let descriptor = b.add(
                format!(
                    "<< /Type /FontDescriptor /FontName /{} /Flags 32 /FontBBox [{:.0} {:.0} {:.0} {:.0}] /ItalicAngle 0 \
/Ascent {:.0} /Descent {:.0} /CapHeight {:.0} /StemV 80 /FontFile2 {file} 0 R >>",
                    face.name,
                    bbox[0],
                    bbox[1],
                    bbox[2],
                    bbox[3],
                    metrics.ascent * scale_em,
                    metrics.descent * scale_em,
                    metrics.cap_height.unwrap_or(metrics.ascent * 0.7) * scale_em
                )
                .into_bytes(),
            );
            let widths: String = face.used.keys().map(|g| format!("{g} [{:.0}] ", face.width(*g))).collect();
            let cid = b.add(
                format!(
                    "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
/FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /DW 500 /W [{widths}] >>",
                    face.name
                )
                .into_bytes(),
            );
            let unicode = b.add(stream("", cmap(face).as_bytes(), true));
            b.set(
                id,
                format!("<< /Type /Font /Subtype /Type0 /BaseFont /{} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>", face.name)
                    .into_bytes(),
            );
        }
    } else {
        b.set(font1, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
        b.set(font2, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    }
    resources_fonts.clear();

    // segnalibri
    let mut catalog_extra = String::new();
    if o.bookmarks && !marks.is_empty() {
        let root = b.reserve();
        let ids: Vec<usize> = marks.iter().map(|_| b.reserve()).collect();
        // Il genitore di ogni voce è l'ultima voce con un livello più alto.
        let mut stack: Vec<usize> = Vec::new();
        let mut parent: Vec<Option<usize>> = Vec::new();
        for (i, m) in marks.iter().enumerate() {
            while stack.last().is_some_and(|p| marks[*p].level >= m.level) {
                stack.pop();
            }
            parent.push(stack.last().copied());
            stack.push(i);
        }
        let children = |p: Option<usize>| -> Vec<usize> { (0..marks.len()).filter(|i| parent[*i] == p).collect() };
        let count_below = |i: usize| -> usize {
            let mut n = 0;
            let mut todo = children(Some(i));
            while let Some(c) = todo.pop() {
                n += 1;
                todo.extend(children(Some(c)));
            }
            n
        };
        for (i, m) in marks.iter().enumerate() {
            let siblings = children(parent[i]);
            let at = siblings.iter().position(|s| *s == i).unwrap_or(0);
            let mut d = format!(
                "<< /Title {} /Parent {} 0 R /Dest [{} 0 R /XYZ {} {:.1} 0]",
                pdf_string(&m.title),
                parent[i].map_or(root, |p| ids[p]),
                page_ids[m.page].0,
                margin,
                m.y + m.level as f32 * 2.0
            );
            if at > 0 {
                d.push_str(&format!(" /Prev {} 0 R", ids[siblings[at - 1]]));
            }
            if at + 1 < siblings.len() {
                d.push_str(&format!(" /Next {} 0 R", ids[siblings[at + 1]]));
            }
            let kids = children(Some(i));
            if let (Some(first), Some(last)) = (kids.first(), kids.last()) {
                d.push_str(&format!(" /First {} 0 R /Last {} 0 R /Count {}", ids[*first], ids[*last], count_below(i)));
            }
            d.push_str(" >>");
            b.set(ids[i], d.into_bytes());
        }
        let tops = children(None);
        b.set(
            root,
            format!(
                "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
                ids[tops[0]],
                ids[*tops.last().expect("voce")],
                marks.len()
            )
            .into_bytes(),
        );
        catalog_extra.push_str(&format!(" /Outlines {root} 0 R /PageMode /UseOutlines"));
    }

    // info, PDF/A
    let (info_date, iso_date) = date_now();
    let info = b.add(
        format!("<< /Title {} /Producer (Cedro) /Creator (Cedro) /CreationDate ({info_date}) /ModDate ({info_date}) >>", pdf_string(&o.title))
            .into_bytes(),
    );
    if o.pdfa {
        let metadata = b.add(stream("/Type /Metadata /Subtype /XML", xmp(&o.title, &iso_date).as_bytes(), false));
        let icc = b.add(stream("/N 3", &srgb_profile(), true));
        catalog_extra.push_str(&format!(
            " /Metadata {metadata} 0 R /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) \
/RegistryName (http://www.color.org) /Info (sRGB IEC61966-2.1) /DestOutputProfile {icc} 0 R >>]"
        ));
    }
    let kids = page_ids.iter().map(|(p, _)| format!("{p} 0 R")).collect::<Vec<_>>().join(" ");
    b.set(pages_id, format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", page_ids.len()).into_bytes());
    b.set(catalog, format!("<< /Type /Catalog /Pages {pages_id} 0 R /Lang (it-IT){catalog_extra} >>").into_bytes());

    // ---- file
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in b.objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", b.objects.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    let id: String = {
        let mut seed = (out.len() as u64) ^ 0x9E37_79B9_7F4A_7C15 ^ iso_date.bytes().fold(0u64, |a, c| a.wrapping_mul(31).wrapping_add(c as u64));
        (0..16)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                format!("{:02X}", (seed >> 33) as u8)
            })
            .collect()
    };
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {catalog} 0 R /Info {info} 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n",
            b.objects.len() + 1
        )
        .as_bytes(),
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks() -> Vec<DocBlock> {
        let mut v = vec![
            DocBlock { text: "Delibera di giunta".into(), heading: 1, image: None },
            DocBlock { text: "Premesse è già così, perché città".into(), heading: 0, image: None },
            DocBlock { text: "Dispositivo".into(), heading: 2, image: None },
        ];
        for n in 0..120 {
            v.push(DocBlock { text: format!("Riga {n}: il Comune di Bondeno approva il documento."), heading: 0, image: None });
        }
        v.push(DocBlock { text: "Allegati".into(), heading: 1, image: None });
        v.push(DocBlock { text: "Fine.".into(), heading: 0, image: None });
        v
    }

    fn options(embed: bool, pdfa: bool) -> PdfOptions {
        PdfOptions { size: PageSize::A4, embed, bookmarks: true, pdfa, title: "Delibera 12".into() }
    }

    #[test]
    fn embedded_font_text_is_copyable_and_bookmarks_work() {
        let pdf = build(&blocks(), &options(true, false)).unwrap();
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        assert!(doc.get_pages().len() >= 2);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("Delibera di giunta"), "{text}");
        assert!(text.contains("perché città"), "{text}");
        // Il font è dentro il file.
        assert!(pdf.windows(10).any(|w| w == b"/FontFile2"));
        assert!(!pdf.windows(10).any(|w| w == b"/Helvetica"));
        // Segnalibri: tre voci, «Dispositivo» è figlia di «Delibera di giunta».
        let outlines = doc.catalog().unwrap().get(b"Outlines").unwrap().as_reference().unwrap();
        let root = doc.get_dictionary(outlines).unwrap();
        assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), 3);
        let first = doc.get_dictionary(root.get(b"First").unwrap().as_reference().unwrap()).unwrap();
        assert_eq!(first.get(b"Count").unwrap().as_i64().unwrap(), 1);
        assert!(first.get(b"First").is_ok() && first.get(b"Next").is_ok());
        let title = first.get(b"Title").unwrap().as_str().unwrap();
        assert_eq!(&title[..2], &[0xFE, 0xFF]);
    }

    #[test]
    fn tabs_become_spaces_not_missing_glyphs() {
        let mut o = options(true, false);
        o.bookmarks = false;
        let pdf = build(&[DocBlock { text: "nome\tcittà\u{1}".into(), heading: 0, image: None }], &o).unwrap();
        let text = lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap();
        assert!(text.contains("nome    città"), "{text:?}");
        assert!(!text.contains('\t') && !text.contains('\u{1}'));
    }

    #[test]
    fn plain_fonts_and_no_bookmarks_still_make_a_valid_pdf() {
        let mut o = options(false, false);
        o.bookmarks = false;
        let pdf = build(&blocks(), &o).unwrap();
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        assert!(doc.catalog().unwrap().get(b"Outlines").is_err());
        assert!(pdf.windows(10).any(|w| w == b"/Helvetica"));
        assert!(!pdf.windows(10).any(|w| w == b"/FontFile2"));
        assert!(doc.extract_text(&[1]).unwrap().contains("Delibera di giunta"));
    }

    #[test]
    fn pdfa_has_metadata_color_profile_and_embedded_fonts() {
        // PDF/A impone il font incorporato anche se non lo si è chiesto.
        let pdf = build(&blocks(), &options(false, true)).unwrap();
        // Un esemplare resta nella cartella temporanea, per controllarlo con altri strumenti (pdffonts, gs…).
        let _ = std::fs::write(std::env::temp_dir().join("cedro-pdfa-sample.pdf"), &pdf);
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        let catalog = doc.catalog().unwrap();
        assert!(catalog.get(b"Metadata").is_ok() && catalog.get(b"OutputIntents").is_ok());
        let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"<pdfaid:part>1</pdfaid:part>") && has(b"<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(has(b"GTS_PDFA1") && has(b"/FontFile2") && has(b"/ToUnicode"));
        let profile = srgb_profile();
        assert_eq!(&profile[36..40], b"acsp");
        assert_eq!(u32::from_be_bytes(profile[0..4].try_into().unwrap()) as usize, profile.len());
    }

    /// Il modo di prima: ogni riga candidata rimisurata da capo.
    fn wrap_by_measuring(face: &Face, text: &str, size: f32, scale: f32, max: f32) -> Vec<String> {
        let width = |t: &str| t.chars().map(|c| face.char_width(c)).sum::<f32>() * size / 1000.0 * scale;
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in text.split(' ') {
            let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
            if width(&candidate) <= max || current.is_empty() && word.is_empty() {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut piece = String::new();
            for c in word.chars() {
                if width(&format!("{piece}{c}")) > max && !piece.is_empty() {
                    lines.push(std::mem::take(&mut piece));
                }
                piece.push(c);
            }
            current = piece;
        }
        lines.push(current);
        lines
    }

    #[test]
    fn wrapping_with_running_widths_matches_measuring_each_line() {
        let face = Face::new(REGULAR, "TitilliumWeb-Regular").unwrap();
        let texts = [
            "Il Comune di Bondeno approva il documento allegato, con le modifiche richieste dalla commissione.".repeat(7),
            format!("Prima  doppio spazio {} poi una parola lunghissima e la fine", "abcdefghij".repeat(30)),
            " inizia con uno spazio e finisce con due  ".to_string(),
            "αβγ città perché € — «virgolette»".repeat(9),
            String::new(),
        ];
        for text in &texts {
            for (size, scale, max) in [(11.0, 1.0, 481.6), (20.0, 1.12, 481.6), (13.5, 1.0, 40.0), (11.0, 1.0, 3.0)] {
                assert_eq!(wrap(&face, text, size, scale, max), wrap_by_measuring(&face, text, size, scale, max), "{text:?} {size} {max}");
            }
        }
    }

    #[test]
    fn characters_missing_from_the_font_are_not_copied_as_another() {
        let mut o = options(true, false);
        o.bookmarks = false;
        let pdf = build(&[DocBlock { text: "ab 漢字 cd".into(), heading: 0, image: None }], &o).unwrap();
        let text = lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap();
        assert!(text.contains("ab") && text.contains("cd"), "{text:?}");
        // Prima i due ideogrammi (glifo 0) si copiavano entrambi come il primo.
        assert!(!text.contains("漢漢"), "{text:?}");
    }

    #[test]
    fn long_words_and_page_sizes_fit() {
        let long = DocBlock { text: "x".repeat(400), heading: 0, image: None };
        for size in [PageSize::A4, PageSize::A3, PageSize::Letter] {
            let o = PdfOptions { size, ..options(true, false) };
            let pdf = build(&[long.clone()], &o).unwrap();
            let doc = lopdf::Document::load_mem(&pdf).unwrap();
            let page = doc.get_pages()[&1];
            let media = doc.get_dictionary(page).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
            let w = media[2].as_float().unwrap();
            assert!(w == 595.0 || w == 842.0 || w == 612.0);
            assert_eq!(doc.extract_text(&[1]).unwrap().replace('\n', "").trim().len(), 400);
        }
    }
}
