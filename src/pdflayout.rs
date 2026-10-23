//! PDF in Word fedele all'originale: il testo del PDF, con font, corpo, colore e posizione,
//! sopra uno sfondo con tutto il resto della pagina (immagini, riquadri, disegni).

use std::fmt::Write as _;

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    interpret_page, TransformExt, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, Paint, SoftMask,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{BezPath, Rect};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

use crate::zipstore::zip_archive;

/// Un carattere del PDF, in punti con l'origine in alto a sinistra della pagina.
struct Glyph {
    text: String,
    x: f64,
    /// Linea di base.
    y: f64,
    size: f64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
    font: String,
}

#[derive(Default)]
struct Collector {
    glyphs: Vec<Glyph>,
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Invisible) {
            return;
        }
        let color = match &props.paint {
            Paint::Color(c) => {
                let [r, g, b, _] = c.to_rgba().components();
                [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8]
            }
            _ => [0, 0, 0],
        };
        for glyph in run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else { continue };
            let text = match unicode {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            if text.chars().all(|c| c == '\0') {
                continue;
            }
            // L'outline ha 1000 unità per em: la scala della trasformazione è il corpo in punti.
            let t = props.transform * glyph.transform();
            let [a, b, c, d, e, f] = t.as_coeffs();
            if b.abs() > 0.01 * a.abs() || c.abs() > 0.01 * d.abs() {
                continue; // testo ruotato: resta nello sfondo non c'è, lo si perde, ma è raro
            }
            let size = (a * d - b * c).abs().sqrt() * 1000.0;
            if size < 1.0 {
                continue;
            }
            let (mut font, mut bold, mut italic) = (String::new(), false, false);
            if let hayro::hayro_interpret::font::Glyph::Outline(o) = &**glyph {
                if let Some(data) = o.font_data() {
                    font = data.postscript_name.unwrap_or_default();
                    bold = data.weight.is_some_and(|w| w >= 600);
                    italic = data.is_italic;
                }
            }
            self.glyphs.push(Glyph { text, x: e, y: f, size, color, bold, italic, font });
        }
    }
}

/// Una riga di testo con lo stesso aspetto: diventa un riquadro nel Word.
struct Line {
    x: f64,
    baseline: f64,
    width: f64,
    size: f64,
    runs: Vec<Run>,
}

struct Run {
    text: String,
    font: String,
    size: f64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
}

fn group_lines(mut glyphs: Vec<Glyph>) -> Vec<Line> {
    // Prima per riga di base, poi da sinistra.
    glyphs.sort_by(|a, b| {
        let tol = 0.35 * a.size.min(b.size);
        if (a.y - b.y).abs() > tol {
            a.y.partial_cmp(&b.y).unwrap()
        } else {
            a.x.partial_cmp(&b.x).unwrap()
        }
    });
    let mut lines: Vec<Line> = Vec::new();
    let mut last_x = 0.0;
    for g in glyphs {
        let new_line = match lines.last() {
            None => true,
            Some(l) => (g.y - l.baseline).abs() > 0.35 * g.size.min(l.size) || g.x - last_x > 1.5 * g.size || g.x < l.x - 0.5,
        };
        // Un glifo molto a sinistra dell'ultimo, sulla stessa riga: altra colonna.
        let new_line = new_line || lines.last().is_some_and(|_| g.x + 0.3 * g.size < last_x);
        if new_line {
            lines.push(Line { x: g.x, baseline: g.y, width: 0.0, size: g.size, runs: Vec::new() });
        }
        let line = lines.last_mut().unwrap();
        // Un vuoto largo come una parola, senza spazio nel PDF, diventa uno spazio.
        if !new_line && g.x - last_x > 0.9 * g.size && !g.text.starts_with(' ') {
            if let Some(run) = line.runs.last_mut() {
                if !run.text.ends_with(' ') {
                    run.text.push(' ');
                }
            }
        }
        let same = line.runs.last().is_some_and(|r| {
            r.font == g.font && (r.size - g.size).abs() < 0.2 && r.color == g.color && r.bold == g.bold && r.italic == g.italic
        });
        if same {
            line.runs.last_mut().unwrap().text.push_str(&g.text);
        } else {
            line.runs.push(Run {
                text: g.text.clone(),
                font: g.font.clone(),
                size: g.size,
                color: g.color,
                bold: g.bold,
                italic: g.italic,
            });
        }
        line.size = line.size.max(g.size);
        line.width = (g.x - line.x) + g.size * 0.6 * g.text.chars().count() as f64;
        last_x = g.x;
    }
    lines.retain(|l| l.runs.iter().any(|r| !r.text.trim().is_empty()));
    lines
}

/// Il nome del carattere per Word: senza il prefisso del sottoinsieme e senza lo stile.
fn family(postscript: &str) -> String {
    let name = postscript.split('+').next_back().unwrap_or(postscript);
    let base = name.split(['-', ',']).next().unwrap_or(name);
    let low = base.to_ascii_lowercase();
    let mapped = if low.contains("times") {
        "Times New Roman"
    } else if low.contains("arial") || low.contains("helvetica") || low.is_empty() {
        "Arial"
    } else if low.contains("courier") {
        "Courier New"
    } else if low.contains("calibri") {
        "Calibri"
    } else if low.contains("verdana") {
        "Verdana"
    } else if low.contains("tahoma") {
        "Tahoma"
    } else if low.contains("georgia") {
        "Georgia"
    } else {
        // «OpenSans» diventa «Open Sans».
        let mut out = String::new();
        for (i, c) in base.chars().enumerate() {
            if i > 0 && c.is_uppercase() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push(c);
        }
        return out;
    };
    mapped.to_string()
}

fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Toglie tutto il testo dal PDF (da BT a ET, anche dentro i moduli), per disegnare lo sfondo.
fn strip_text(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use lopdf::content::Content;
    use lopdf::{Document, Object};

    fn clean(data: &[u8]) -> Option<Vec<u8>> {
        let mut content = Content::decode(data).ok()?;
        let mut inside = false;
        content.operations.retain(|op| match op.operator.as_str() {
            "BT" => {
                inside = true;
                false
            }
            "ET" => {
                inside = false;
                false
            }
            _ => !inside,
        });
        content.encode().ok()
    }

    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF non letto: {e}"))?;
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    for page in pages {
        if let Ok(data) = doc.get_page_content(page) {
            if let Some(new) = clean(&data) {
                let _ = doc.change_page_content(page, new);
            }
        }
    }
    for object in doc.objects.values_mut() {
        if let Object::Stream(stream) = object {
            let is_form = stream.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Form");
            if is_form {
                let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                if let Some(new) = clean(&data) {
                    stream.dict.remove(b"Filter");
                    stream.dict.remove(b"DecodeParms");
                    stream.set_plain_content(new);
                }
            }
        }
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Una pagina del Word.
pub enum Page {
    /// Testo e sfondo come nel PDF.
    Layout(LayoutPage),
    /// Pagina senza testo da copiare: non c'è niente da ricostruire.
    Empty,
}

pub struct LayoutPage {
    width: f64,
    height: f64,
    lines: Vec<Line>,
    background: Vec<u8>,
}

/// Legge le pagine del PDF. `Page::Empty` per quelle senza testo incorporato.
pub fn read_pages(bytes: &[u8]) -> Result<Vec<Page>, String> {
    let stripped = strip_text(bytes)?;
    let original = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non letto: {e:?}"))?;
    let background = Pdf::new(stripped).map_err(|e| format!("PDF non letto: {e:?}"))?;
    let cache = RenderCache::new();
    let interp_cache = InterpreterCache::new();
    let settings = InterpreterSettings::default();
    let mut pages = Vec::new();
    for (page, bg_page) in original.pages().iter().zip(background.pages().iter()) {
        let (w, h) = page.render_dimensions();
        let ctm = page.initial_transform(true).to_kurbo();
        let mut ctx = Context::new(ctm, Rect::new(0.0, 0.0, w as f64, h as f64), &interp_cache, page.xref(), settings.clone());
        let mut collector = Collector::default();
        interpret_page(page, &mut ctx, &mut collector);
        let lines = group_lines(collector.glyphs);
        if lines.is_empty() {
            pages.push(Page::Empty);
            continue;
        }
        let scale = (150.0 / 72.0f32).min(3000.0 / w.max(h).max(1.0));
        let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
        let rendered = render(bg_page, &cache, &settings, &RenderSettings::default(), &pixmap);
        let png = rendered.into_png().map_err(|e| format!("pagina non disegnata: {e}"))?;
        let rgb = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgb8();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        pages.push(Page::Layout(LayoutPage { width: w as f64, height: h as f64, lines, background: jpeg }));
    }
    Ok(pages)
}

const EMU_PER_PT: f64 = 12700.0;

fn twips(pt: f64) -> i64 {
    (pt * 20.0).round() as i64
}

fn frame_xml(line: &Line, page_w: f64) -> String {
    let size = line.size;
    let line_h = size * 1.25;
    let top = (line.baseline - size * 1.0).max(0.0);
    // Il riquadro è più largo del testo: se Word usa un carattere di ripiego, la riga non va a capo.
    let width = (line.width * 1.3 + size * 3.0).min((page_w - line.x - 1.0).max(size * 2.0));
    let mut out = String::new();
    let _ = write!(
        out,
        "<w:p><w:pPr><w:framePr w:w=\"{}\" w:h=\"{}\" w:hRule=\"exact\" w:wrap=\"around\" w:vAnchor=\"page\" w:hAnchor=\"page\" w:x=\"{}\" w:y=\"{}\"/>\
<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/><w:ind w:left=\"0\" w:right=\"0\"/></w:pPr>",
        twips(width),
        twips(line_h),
        twips(line.x),
        twips(top),
        twips(line_h)
    );
    for run in &line.runs {
        let half_points = (run.size * 2.0).round().max(2.0) as i64;
        let font = xml(&family(&run.font));
        let _ = write!(
            out,
            "<w:r><w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>{}{}<w:color w:val=\"{:02X}{:02X}{:02X}\"/><w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            if run.bold { "<w:b/>" } else { "" },
            if run.italic { "<w:i/>" } else { "" },
            run.color[0],
            run.color[1],
            run.color[2],
            xml(&run.text)
        );
    }
    out.push_str("</w:p>");
    out
}

fn background_xml(id: usize, w: f64, h: f64) -> String {
    let (cx, cy) = ((w * EMU_PER_PT) as i64, (h * EMU_PER_PT) as i64);
    format!(
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"0\" behindDoc=\"1\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">\
<wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"page\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"page\"><wp:posOffset>0</wp:posOffset></wp:positionV>\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapNone/><wp:docPr id=\"{id}\" name=\"Sfondo {id}\"/><wp:cNvGraphicFramePr/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"sfondo{id}.jpg\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdPage{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    )
}

/// Il file Word. Le pagine con testo hanno l'aspetto del PDF; per le altre, `fallback`
/// dà il testo letto con l'OCR, in paragrafi, su una pagina normale.
pub fn docx(pages: &[Page], fallback: impl Fn(usize) -> String) -> Vec<u8> {
    let mut body = String::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut rels = String::new();
    for (i, page) in pages.iter().enumerate() {
        match page {
            Page::Layout(p) => {
                for line in &p.lines {
                    body.push_str(&frame_xml(line, p.width));
                }
                let id = i + 1;
                media.push((format!("word/media/pagina{id}.jpg"), p.background.clone()));
                let _ = write!(
                    rels,
                    "<Relationship Id=\"rIdPage{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/pagina{id}.jpg\"/>"
                );
                // L'ultimo paragrafo della pagina porta lo sfondo e la fine della sezione.
                let _ = write!(
                    body,
                    "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/><w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"0\" w:right=\"0\" w:bottom=\"0\" w:left=\"0\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr></w:pPr>{}</w:p>",
                    twips(p.width),
                    twips(p.height),
                    background_xml(id, p.width, p.height)
                );
            }
            Page::Empty => {
                for para in fallback(i).split('\n') {
                    let _ = write!(body, "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml(para));
                }
                body.push_str("<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr></w:pPr></w:p>");
            }
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}</w:body></w:document>"
    );
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let doc_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>{rels}</Relationships>"
    );
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:lang w:val=\"it-IT\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
</w:styles>";
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ];
    for (name, data) in &media {
        entries.push((name.as_str(), data.as_slice()));
    }
    zip_archive(&entries)
}
