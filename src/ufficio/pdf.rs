//! Il PDF delle pagine impaginate: caratteri ridotti ai soli glifi usati (testo copiabile con
//! ToUnicode), immagini, link cliccabili, segnalibri e, su richiesta, PDF/A-1b.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use subsetter::GlyphRemapper;

use super::font::{Face, FontId};
use super::pagina::{Dash, Op, Stampato, Target};
use crate::pdfdoc::{date_now, pdf_string, srgb_profile, stream, xmp};

pub struct Opzioni {
    /// Caratteri dentro il file; senza, il lettore usa quelli che ha (stessi nomi e larghezze).
    pub embed: bool,
    pub bookmarks: bool,
    pub pdfa: bool,
    pub links: bool,
}

/// Come un font entra nel PDF.
enum Mode {
    /// Ridotto ai glifi usati, con i glifi rinumerati: la mappa e il font ridotto.
    Subset(GlyphRemapper, Vec<u8>),
    /// Intero, con i glifi del file (se la riduzione non riesce).
    Full,
    /// Non incorporato: WinAnsi e le larghezze vere.
    Named,
}

struct FontOut {
    mode: Mode,
    /// glifo scritto nel PDF → (glifo del font, carattere).
    used: BTreeMap<u16, (u16, char)>,
    /// byte WinAnsi → carattere (solo `Named`).
    ansi: BTreeMap<u8, char>,
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

fn win_ansi_ok(c: char) -> bool {
    c == '?' || crate::pdf::win_ansi(&c.to_string()) != b"?"
}

fn color(c: (u8, u8, u8)) -> String {
    let f = |v: u8| {
        let x = v as f32 / 255.0;
        if v == 0 || v == 255 {
            format!("{}", v / 255)
        } else {
            format!("{x:.3}")
        }
    };
    format!("{} {} {}", f(c.0), f(c.1), f(c.2))
}

fn hex_string(s: &str) -> String {
    let mut out = String::from("<");
    for b in s.bytes() {
        let _ = write!(out, "{b:02X}");
    }
    out.push('>');
    out
}

pub fn scrivi(doc: &Stampato, o: &Opzioni) -> Result<Vec<u8>, String> {
    let embed = o.embed || o.pdfa;
    let fonts = &doc.fonts;

    // ---- primo giro: quali glifi usa ogni font
    let mut used_fonts: Vec<FontId> = Vec::new();
    let mut chars: HashMap<FontId, Vec<char>> = HashMap::new();
    for page in &doc.pages {
        for op in &page.ops {
            if let Op::Text { font, text, .. } = op {
                if !used_fonts.contains(font) {
                    used_fonts.push(*font);
                }
                chars.entry(*font).or_default().extend(text.chars());
            }
        }
    }
    let mut outs: HashMap<FontId, FontOut> = HashMap::new();
    for &f in &used_fonts {
        let face = fonts.face(f);
        let all_ansi = chars.get(&f).is_none_or(|v| v.iter().all(|c| win_ansi_ok(*c)));
        let mode = if !embed && all_ansi {
            Mode::Named
        } else {
            let mut r = GlyphRemapper::new();
            for c in chars.get(&f).into_iter().flatten() {
                if let Some((g, _)) = face.glyph(*c) {
                    r.remap(g);
                }
            }
            match subsetter::subset(face.data(), face.index(), &r) {
                Ok(data) => Mode::Subset(r, data),
                Err(_) => Mode::Full,
            }
        };
        outs.insert(f, FontOut { mode, used: BTreeMap::new(), ansi: BTreeMap::new() });
    }

    // ---- oggetti
    let mut b = Builder { objects: Vec::new() };
    let catalog = b.reserve();
    let pages_id = b.reserve();
    let page_ids: Vec<(usize, usize)> = doc.pages.iter().map(|_| (b.reserve(), b.reserve())).collect();
    let font_ids: HashMap<FontId, usize> = used_fonts.iter().map(|f| (*f, b.reserve())).collect();
    let img_ids: Vec<usize> = doc.images.iter().map(|_| b.reserve()).collect();
    let font_res: String = used_fonts.iter().map(|f| format!("/F{f} {} 0 R ", font_ids[f])).collect();

    // I punti d'arrivo dei link interni.
    let mut anchors: HashMap<&str, (usize, f32)> = HashMap::new();
    for (i, p) in doc.pages.iter().enumerate() {
        for (name, y) in &p.anchors {
            anchors.entry(name.as_str()).or_insert((i, p.h - y));
        }
    }

    for (i, page) in doc.pages.iter().enumerate() {
        let h = page.h;
        let mut ops = String::new();
        let mut used_images: Vec<usize> = Vec::new();
        for op in &page.ops {
            match op {
                Op::Rect { x, y, w, h: rh, color: c } => {
                    let _ = writeln!(ops, "{} rg {x:.2} {:.2} {w:.2} {rh:.2} re f", color(*c), h - y - rh);
                }
                Op::Line { x1, y1, x2, y2, width, color: c, dash } => {
                    let d = match dash {
                        Dash::Solid => String::new(),
                        Dash::Dashed => format!("[{:.2} {:.2}] 0 d ", width * 3.0, width * 2.0),
                        Dash::Dotted => format!("[{:.2} {:.2}] 0 d 1 J ", 0.01, width * 2.0),
                    };
                    let _ = writeln!(ops, "q {} RG {width:.2} w {d}{x1:.2} {:.2} m {x2:.2} {:.2} l S Q", color(*c), h - y1, h - y2);
                }
                Op::Image { index, x, y, w, h: ih } => {
                    let _ = writeln!(ops, "q {w:.2} 0 0 {ih:.2} {x:.2} {:.2} cm /Im{index} Do Q", h - y - ih);
                    if !used_images.contains(index) {
                        used_images.push(*index);
                    }
                }
                Op::Clip { x, y, w, h: ch } => {
                    let _ = writeln!(ops, "q {x:.2} {:.2} {w:.2} {ch:.2} re W n", h - y - ch);
                }
                Op::Unclip => ops.push_str("Q\n"),
                Op::Text { x, y, font, size, color: c, text } => {
                    if text.is_empty() {
                        continue;
                    }
                    let face: &Face = fonts.face(*font);
                    let out = outs.get_mut(font).expect("font contato");
                    let shown = match &mut out.mode {
                        Mode::Named => {
                            let bytes = crate::pdf::win_ansi(text);
                            for (byte, ch) in bytes.iter().zip(text.chars()) {
                                out.ansi.entry(*byte).or_insert(ch);
                            }
                            hex_string_bytes(&bytes)
                        }
                        Mode::Subset(r, _) => {
                            let mut hex = String::from("<");
                            for ch in text.chars() {
                                let g = face.glyph(ch).map_or(0, |g| g.0);
                                let n = r.get(g).unwrap_or(0);
                                if g != 0 {
                                    out.used.entry(n).or_insert((g, ch));
                                }
                                let _ = write!(hex, "{n:04X}");
                            }
                            hex.push('>');
                            hex
                        }
                        Mode::Full => {
                            let mut hex = String::from("<");
                            for ch in text.chars() {
                                let g = face.glyph(ch).map_or(0, |g| g.0);
                                if g != 0 {
                                    out.used.entry(g).or_insert((g, ch));
                                }
                                let _ = write!(hex, "{g:04X}");
                            }
                            hex.push('>');
                            hex
                        }
                    };
                    let skew = if face.synth_italic { 0.2 } else { 0.0 };
                    let bold = if face.synth_bold { format!("{} RG 2 Tr {:.3} w ", color(*c), size * 0.03) } else { String::new() };
                    let _ = writeln!(ops, "BT {} rg {bold}/F{font} {size:.2} Tf 1 0 {skew} 1 {x:.2} {:.2} Tm {shown} Tj ET", color(*c), h - y);
                }
            }
        }
        let (page_id, content_id) = page_ids[i];
        b.set(content_id, stream("", ops.as_bytes(), true));
        let xobjects = if used_images.is_empty() {
            String::new()
        } else {
            format!(" /XObject << {} >>", used_images.iter().map(|k| format!("/Im{k} {} 0 R", img_ids[*k])).collect::<Vec<_>>().join(" "))
        };
        // I link: verso un sito, o verso un segnalibro del documento.
        let mut annots = Vec::new();
        if o.links {
            for l in &page.links {
                let rect = format!("[{:.2} {:.2} {:.2} {:.2}]", l.x, h - l.y - l.h, l.x + l.w, h - l.y);
                let action = match &l.target {
                    Target::Uri(u) => format!("/A << /S /URI /URI {} >>", hex_string(u)),
                    Target::Anchor(a) => match anchors.get(a.as_str()) {
                        Some((p, y)) => format!("/Dest [{} 0 R /XYZ 0 {:.1} 0]", page_ids[*p].0, y + 4.0),
                        None => continue,
                    },
                };
                annots.push(b.add(format!("<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] /F 4 {action} >>").into_bytes()));
            }
        }
        let annots = if annots.is_empty() { String::new() } else { format!(" /Annots [{}]", annots.iter().map(|a| format!("{a} 0 R")).collect::<Vec<_>>().join(" ")) };
        b.set(
            page_id,
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents {content_id} 0 R /Resources << /Font << {font_res}>>{xobjects} >>{annots} >>",
                page.w, page.h
            )
            .into_bytes(),
        );
    }

    // immagini
    for (img, id) in doc.images.iter().zip(&img_ids) {
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8",
            img.px.0,
            img.px.1,
            if img.gray { "/DeviceGray" } else { "/DeviceRGB" }
        );
        b.set(*id, if img.jpeg { stream(&format!("{dict} /Filter /DCTDecode"), &img.data, false) } else { stream(&dict, &img.data, true) });
    }

    // font
    for f in &used_fonts {
        let face = fonts.face(*f);
        let out = &outs[f];
        let mut flags = if face.serif { 2 } else { 0 } | if face.fixed { 1 } else { 0 };
        if face.italic {
            flags |= 64;
        }
        let descriptor_body = |file: Option<usize>, name: &str| {
            let mut flags = flags;
            flags |= 32;
            format!(
                "<< /Type /FontDescriptor /FontName /{name} /Flags {flags} /FontBBox [{:.0} {:.0} {:.0} {:.0}] /ItalicAngle {:.1} \
/Ascent {:.0} /Descent {:.0} /CapHeight {:.0} /StemV {}{} >>",
                face.bbox[0],
                face.bbox[1],
                face.bbox[2],
                face.bbox[3],
                if face.synth_italic { -12.0 } else { face.italic_angle },
                face.ascent,
                -face.descent,
                face.cap_height,
                if face.bold { 120 } else { 80 },
                file.map_or(String::new(), |f| format!(" /FontFile2 {f} 0 R"))
            )
        };
        match &out.mode {
            Mode::Named => {
                let style = match (face.synth_bold, face.synth_italic) {
                    (true, true) => ",BoldItalic",
                    (true, false) => ",Bold",
                    (false, true) => ",Italic",
                    _ => "",
                };
                let name = format!("{}{style}", face.name);
                let descriptor = b.add(descriptor_body(None, &name).into_bytes());
                let (first, last) = (out.ansi.keys().next().copied().unwrap_or(32), out.ansi.keys().last().copied().unwrap_or(32));
                let widths: String = (first..=last).map(|code| out.ansi.get(&code).and_then(|c| face.glyph(*c)).map_or(0.0, |g| g.1)).map(|w| format!("{w:.0} ")).collect();
                b.set(
                    font_ids[f],
                    format!(
                        "<< /Type /Font /Subtype /TrueType /BaseFont /{name} /FirstChar {first} /LastChar {last} /Widths [{widths}] /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R >>"
                    )
                    .into_bytes(),
                );
            }
            Mode::Subset(..) | Mode::Full => {
                let data: &[u8] = match &out.mode {
                    Mode::Subset(_, data) => data,
                    _ => face.data(),
                };
                // Sei lettere a caso davanti al nome: è un sottoinsieme (come chiede la norma).
                let tag: String = face.name.bytes().chain(f.to_string().bytes()).fold(0u32, |a, c| a.wrapping_mul(31).wrapping_add(c as u32)).to_string().bytes().cycle().take(6).map(|c| (b'A' + (c % 26)) as char).collect();
                let name = if matches!(out.mode, Mode::Subset(..)) { format!("{tag}+{}", face.name) } else { face.name.clone() };
                let file = b.add(stream(&format!("/Length1 {}", data.len()), data, true));
                let descriptor = b.add(descriptor_body(Some(file), &name).into_bytes());
                let widths: String = out.used.iter().map(|(n, (g, _))| format!("{n} [{:.0}] ", face.glyph_width(*g))).collect();
                let cid = b.add(
                    format!(
                        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
/FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /DW {:.0} /W [{widths}] >>",
                        face.glyph_width(0)
                    )
                    .into_bytes(),
                );
                let unicode = b.add(stream("", to_unicode(&out.used).as_bytes(), true));
                b.set(
                    font_ids[f],
                    format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>").into_bytes(),
                );
            }
        }
    }

    // segnalibri
    let marks: Vec<(usize, &super::pagina::Mark)> = doc.pages.iter().enumerate().flat_map(|(i, p)| p.marks.iter().map(move |m| (i, m))).collect();
    let mut catalog_extra = String::new();
    if o.bookmarks && !marks.is_empty() {
        let root = b.reserve();
        let ids: Vec<usize> = marks.iter().map(|_| b.reserve()).collect();
        let mut stack: Vec<usize> = Vec::new();
        let mut parent: Vec<Option<usize>> = Vec::new();
        for (i, (_, m)) in marks.iter().enumerate() {
            while stack.last().is_some_and(|p| marks[*p].1.level >= m.level) {
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
        for (i, (page, m)) in marks.iter().enumerate() {
            let siblings = children(parent[i]);
            let at = siblings.iter().position(|s| *s == i).unwrap_or(0);
            let mut d = format!(
                "<< /Title {} /Parent {} 0 R /Dest [{} 0 R /XYZ 0 {:.1} 0]",
                pdf_string(&m.title),
                parent[i].map_or(root, |p| ids[p]),
                page_ids[*page].0,
                doc.pages[*page].h - m.y + 6.0
            );
            if at > 0 {
                let _ = write!(d, " /Prev {} 0 R", ids[siblings[at - 1]]);
            }
            if at + 1 < siblings.len() {
                let _ = write!(d, " /Next {} 0 R", ids[siblings[at + 1]]);
            }
            let kids = children(Some(i));
            if let (Some(first), Some(last)) = (kids.first(), kids.last()) {
                let _ = write!(d, " /First {} 0 R /Last {} 0 R /Count {}", ids[*first], ids[*last], count_below(i));
            }
            d.push_str(" >>");
            b.set(ids[i], d.into_bytes());
        }
        let tops = children(None);
        b.set(root, format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>", ids[tops[0]], ids[*tops.last().expect("voce")], marks.len()).into_bytes());
        catalog_extra.push_str(&format!(" /Outlines {root} 0 R /PageMode /UseOutlines"));
    }

    // info, PDF/A
    let (info_date, iso_date) = date_now();
    let info = b.add(format!("<< /Title {} /Producer (Cedro) /Creator (Cedro) /CreationDate ({info_date}) /ModDate ({info_date}) >>", pdf_string(&doc.title)).into_bytes());
    if o.pdfa {
        let metadata = b.add(stream("/Type /Metadata /Subtype /XML", xmp(&doc.title, &iso_date).as_bytes(), false));
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
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root {catalog} 0 R /Info {info} 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n", b.objects.len() + 1).as_bytes());
    Ok(out)
}

fn hex_string_bytes(bytes: &[u8]) -> String {
    let mut out = String::from("<");
    for b in bytes {
        let _ = write!(out, "{b:02X}");
    }
    out.push('>');
    out
}

fn to_unicode(used: &BTreeMap<u16, (u16, char)>) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &(u16, char))> = used.iter().filter(|(n, _)| **n != 0).collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(out, "{} beginbfchar", chunk.len());
        for (n, (_, c)) in chunk {
            let mut units = [0u16; 2];
            let hex: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(out, "<{n:04X}> <{hex}>");
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

#[cfg(test)]
mod tests {
    use super::super::font::Fonti;
    use super::super::pagina::{Link, Mark, Pagina, BLACK};
    use super::*;

    fn sample() -> Stampato {
        let mut fonts = Fonti::builtin();
        let r = fonts.get("Calibri", false, false);
        let bi = fonts.get("Calibri", true, true);
        let mut p = Pagina::new(595.0, 842.0);
        p.ops.push(Op::Rect { x: 50.0, y: 50.0, w: 100.0, h: 20.0, color: (200, 230, 255) });
        p.ops.push(Op::Text { x: 56.0, y: 64.0, font: r, size: 11.0, color: BLACK, text: "Delibera è già città".into() });
        p.ops.push(Op::Text { x: 56.0, y: 90.0, font: bi, size: 14.0, color: (200, 0, 0), text: "Titolo".into() });
        p.ops.push(Op::Line { x1: 50.0, y1: 100.0, x2: 300.0, y2: 100.0, width: 0.5, color: BLACK, dash: Dash::Dashed });
        p.ops.push(Op::Clip { x: 0.0, y: 0.0, w: 10.0, h: 10.0 });
        p.ops.push(Op::Unclip);
        p.links.push(Link { x: 56.0, y: 54.0, w: 80.0, h: 12.0, target: Target::Uri("https://example.org/a b".into()) });
        p.links.push(Link { x: 56.0, y: 80.0, w: 80.0, h: 12.0, target: Target::Anchor("fine".into()) });
        p.marks.push(Mark { level: 1, title: "Titolo".into(), y: 80.0 });
        let mut q = Pagina::new(842.0, 595.0);
        q.anchors.push(("fine".into(), 100.0));
        q.ops.push(Op::Text { x: 56.0, y: 64.0, font: r, size: 11.0, color: BLACK, text: "Seconda pagina".into() });
        q.marks.push(Mark { level: 2, title: "Sotto".into(), y: 50.0 });
        Stampato { pages: vec![p, q], images: Vec::new(), fonts, title: "Prova".into() }
    }

    #[test]
    fn subset_fonts_text_links_and_bookmarks() {
        let doc = sample();
        let pdf = scrivi(&doc, &Opzioni { embed: true, bookmarks: true, pdfa: false, links: true }).unwrap();
        let has = |n: &[u8]| pdf.windows(n.len()).any(|w| w == n);
        assert!(has(b"/FontFile2") && has(b"/ToUnicode") && has(b"/Subtype /Link") && has(b"/URI"));
        let d = lopdf::Document::load_mem(&pdf).unwrap();
        assert_eq!(d.get_pages().len(), 2);
        let t = d.extract_text(&[1]).unwrap();
        assert!(t.contains("Delibera è già città") && t.contains("Titolo"), "{t}");
        // Il carattere ridotto pesa molto meno del Titillium intero (circa 60 KB).
        assert!(pdf.len() < 30_000, "{}", pdf.len());
        // Il link interno porta alla seconda pagina; i segnalibri sono due, uno sotto l'altro.
        assert!(has(b"/Dest ["));
        let outlines = d.catalog().unwrap().get(b"Outlines").unwrap().as_reference().unwrap();
        assert_eq!(d.get_dictionary(outlines).unwrap().get(b"Count").unwrap().as_i64().unwrap(), 2);
        // La pagina orizzontale resta orizzontale.
        let p2 = d.get_pages()[&2];
        let media = d.get_dictionary(p2).unwrap().get(b"MediaBox").unwrap().as_array().unwrap().clone();
        assert_eq!(media[2].as_float().unwrap(), 842.0);
    }

    #[test]
    fn without_embedding_the_fonts_are_named_and_pdfa_embeds_anyway() {
        let doc = sample();
        let pdf = scrivi(&doc, &Opzioni { embed: false, bookmarks: false, pdfa: false, links: false }).unwrap();
        let has = |p: &[u8], n: &[u8]| p.windows(n.len()).any(|w| w == n);
        assert!(!has(&pdf, b"/FontFile2") && has(&pdf, b"/WinAnsiEncoding") && !has(&pdf, b"/Annots"));
        assert!(lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap().contains("città"));
        let pdfa = scrivi(&doc, &Opzioni { embed: false, bookmarks: true, pdfa: true, links: true }).unwrap();
        assert!(has(&pdfa, b"/FontFile2") && has(&pdfa, b"GTS_PDFA1") && has(&pdfa, b"/F 4"));
    }
}
