//! PDF di testo con i caratteri incorporati (TrueType, testo copiabile con ToUnicode), i segnalibri
//! dai titoli e, su richiesta, il formato PDF/A-1b: metadati XMP, profilo colore sRGB e font incorporati.

use std::collections::BTreeMap;
use std::io::Write;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::engine::PageSize;

static REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");

/// Un paragrafo del documento; `heading` 1-3 per i titoli, 0 per il testo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocBlock {
    pub text: String,
    pub heading: u8,
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
}

impl Face {
    fn new(data: &'static [u8], name: &'static str) -> Result<Face, String> {
        let font = FontRef::new(data).map_err(|e| format!("font non letto: {e}"))?;
        let upem = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em as f32;
        Ok(Face { data, name, upem, used: BTreeMap::new() })
    }

    fn font(&self) -> FontRef<'static> {
        FontRef::new(self.data).expect("font già letto")
    }

    fn glyph(&self, c: char) -> u16 {
        self.font().charmap().map(c).map_or(0, |g| g.to_u32() as u16)
    }

    /// Larghezza in unità di 1/1000 em.
    fn width(&self, gid: u16) -> f32 {
        let font = self.font();
        let advance = font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(GlyphId::new(gid as u32));
        advance.unwrap_or(0.0) * 1000.0 / self.upem
    }

    fn text_width(&self, text: &str, size: f32, scale: f32) -> f32 {
        text.chars().map(|c| self.width(self.glyph(c))).sum::<f32>() * size / 1000.0 * scale
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
    let entries: Vec<(&u16, &char)> = face.used.iter().collect();
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
fn wrap(face: &Face, text: &str, size: f32, scale: f32, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if face.text_width(&candidate, size, scale) <= max || current.is_empty() && word.is_empty() {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        // Una parola più larga della riga si spezza.
        let mut piece = String::new();
        for c in word.chars() {
            let next = format!("{piece}{c}");
            if face.text_width(&next, size, scale) > max && !piece.is_empty() {
                lines.push(std::mem::take(&mut piece));
            }
            piece.push(c);
        }
        current = piece;
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
    let mut pages: Vec<Page> = vec![Page { lines: Vec::new() }];
    let mut marks: Vec<Mark> = Vec::new();
    let mut y = height - margin;
    for block in blocks {
        let (size, is_bold, before) = match block.heading {
            1 => (20.0f32, true, 12.0f32),
            2 => (16.0, true, 9.0),
            3 => (13.5, true, 6.0),
            _ => (11.0, false, 0.0),
        };
        let leading = size * 1.3;
        let face = if is_bold { &bold } else { &regular };
        let wrapped = if block.text.is_empty() { vec![String::new()] } else { wrap(face, &block.text, size, scale, max) };
        // Un titolo non resta solo in fondo alla pagina.
        let need = leading * if block.heading > 0 { 2.5 } else { 1.0 } + before;
        if y - need < margin && !pages.last().map_or(true, |p| p.lines.is_empty()) {
            pages.push(Page { lines: Vec::new() });
            y = height - margin;
        } else if !pages.last().map_or(true, |p| p.lines.is_empty()) {
            y -= before;
        }
        if block.heading > 0 && !block.text.trim().is_empty() {
            marks.push(Mark { level: block.heading, title: block.text.trim().to_string(), page: pages.len() - 1, y: y.min(height - 10.0) });
        }
        for line in wrapped {
            if y - leading < margin && !pages.last().map_or(true, |p| p.lines.is_empty()) {
                pages.push(Page { lines: Vec::new() });
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
    let mut resources_fonts = format!("/F1 {font1} 0 R /F2 {font2} 0 R");

    // contenuti (e glifi usati)
    for (i, page) in pages.iter().enumerate() {
        let mut ops = String::from("0 g\n");
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
        b.set(
            page_id,
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {width} {height}] /Contents {content_id} 0 R /Resources << /Font << {resources_fonts} >> >> >>"
            )
            .into_bytes(),
        );
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
            DocBlock { text: "Delibera di giunta".into(), heading: 1 },
            DocBlock { text: "Premesse è già così, perché città".into(), heading: 0 },
            DocBlock { text: "Dispositivo".into(), heading: 2 },
        ];
        for n in 0..120 {
            v.push(DocBlock { text: format!("Riga {n}: il Comune di Bondeno approva il documento."), heading: 0 });
        }
        v.push(DocBlock { text: "Allegati".into(), heading: 1 });
        v.push(DocBlock { text: "Fine.".into(), heading: 0 });
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

    #[test]
    fn long_words_and_page_sizes_fit() {
        let long = DocBlock { text: "x".repeat(400), heading: 0 };
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
