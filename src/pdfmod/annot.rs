//! Commenti, evidenziatore, forme, timbri, immagini: ogni modifica diventa un'annotazione del PDF con il suo
//! aspetto già disegnato (così la vedono tutti i lettori, anche quelli che non sanno disegnarla), oppure,
//! per le immagini e i testi aggiunti, parte del contenuto della pagina.

use std::collections::HashSet;
use std::fmt::Write as _;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use super::geom::Geom;
use super::{Color, MarkKind, Rect, ShapeKind};
use crate::pdfedit::num;

pub(crate) fn rgb(c: Color) -> Object {
    Object::Array(c.iter().map(|v| Object::Real(*v as f32 / 255.0)).collect())
}

pub(crate) fn fill_op(c: Color) -> String {
    format!("{} {} {} rg", num(c[0] as f32 / 255.0), num(c[1] as f32 / 255.0), num(c[2] as f32 / 255.0))
}

pub(crate) fn stroke_op(c: Color) -> String {
    format!("{} {} {} RG", num(c[0] as f32 / 255.0), num(c[1] as f32 / 255.0), num(c[2] as f32 / 255.0))
}

/// Un testo per il PDF: ASCII com'è, altrimenti UTF-16 con l'indicatore.
pub(crate) fn text_obj(s: &str) -> Object {
    if s.is_ascii() {
        return Object::String(s.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut v = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        v.extend_from_slice(&u.to_be_bytes());
    }
    Object::String(v, StringFormat::Hexadecimal)
}

fn rect_obj(r: [f32; 4]) -> Object {
    Object::Array(r.iter().map(|v| Object::Real(*v)).collect())
}

/// Un oggetto «Form» (un disegno riusabile) con il suo riquadro e le sue risorse.
pub(crate) fn form_xobject(doc: &mut Document, bbox: [f32; 4], matrix: Option<[f32; 6]>, content: String, res: Dictionary) -> ObjectId {
    let mut d = lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect_obj(bbox), "Resources" => Object::Dictionary(res) };
    if let Some(m) = matrix {
        d.set("Matrix", Object::Array(m.iter().map(|v| Object::Real(*v)).collect()));
    }
    let mut s = Stream::new(d, content.into_bytes());
    let _ = s.compress();
    doc.add_object(Object::Stream(s))
}

/// Mette l'annotazione tra quelle della pagina.
pub(crate) fn add_annot(doc: &mut Document, page: ObjectId, annot: Dictionary) -> Result<ObjectId, String> {
    let id = doc.add_object(Object::Dictionary(annot));
    link_annot(doc, page, id)?;
    Ok(id)
}

/// Mette tra le annotazioni della pagina un oggetto già creato.
pub(crate) fn link_annot(doc: &mut Document, page: ObjectId, id: ObjectId) -> Result<(), String> {
    let existing = doc.get_dictionary(page).map_err(|e| e.to_string())?.get(b"Annots").ok().cloned();
    match existing {
        Some(Object::Reference(r)) => {
            if let Ok(Object::Array(a)) = doc.get_object_mut(r) {
                a.push(Object::Reference(id));
            }
        }
        Some(Object::Array(mut a)) => {
            a.push(Object::Reference(id));
            doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Annots", Object::Array(a));
        }
        _ => doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Annots", Object::Array(vec![Object::Reference(id)])),
    }
    Ok(())
}

/// Aggiunge operazioni in fondo al contenuto della pagina. La prima volta il contenuto di prima si chiude
/// tra «q» e «Q»: quello che c'era non può cambiare lo stato di chi viene dopo.
pub(crate) fn push_content(doc: &mut Document, page: ObjectId, ops: &str, wrapped: &mut HashSet<ObjectId>) -> Result<(), String> {
    if ops.trim().is_empty() {
        return Ok(());
    }
    let mut parts: Vec<Object> = Vec::new();
    let old = doc.get_dictionary(page).map_err(|e| e.to_string())?.get(b"Contents").ok().cloned();
    let existing: Vec<Object> = match old {
        Some(Object::Reference(r)) => match doc.get_object(r) {
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![Object::Reference(r)],
        },
        Some(Object::Array(a)) => a,
        _ => Vec::new(),
    };
    if wrapped.insert(page) {
        let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let close = doc.add_object(Stream::new(Dictionary::new(), b"Q\n".to_vec()));
        parts.push(Object::Reference(open));
        parts.extend(existing);
        parts.push(Object::Reference(close));
    } else {
        parts = existing;
    }
    let mut s = Stream::new(Dictionary::new(), ops.as_bytes().to_vec());
    let _ = s.compress();
    let mine = doc.add_object(Object::Stream(s));
    parts.push(Object::Reference(mine));
    doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Contents", Object::Array(parts));
    Ok(())
}

fn base_dict(subtype: &str, rect: [f32; 4], color: Color) -> Dictionary {
    let (date, _) = crate::pdfdoc::date_now();
    lopdf::dictionary! {
        "Type" => "Annot", "Subtype" => subtype, "Rect" => rect_obj(rect), "C" => rgb(color), "F" => 4,
        "M" => Object::String(date.into_bytes(), StringFormat::Literal),
    }
}

/// Un aspetto disegnato nelle coordinate locali del rettangolo visto (origine in basso a sinistra, y in su).
/// Dà l'oggetto dell'aspetto e il rettangolo da mettere nell'annotazione (nel file).
fn view_ap(doc: &mut Document, geom: &Geom, view: Rect, content: String, res: Dictionary) -> (ObjectId, [f32; 4]) {
    let file = geom.rect_to_file(&view);
    let m = geom.m;
    let ap = form_xobject(doc, [0.0, 0.0, view.w, view.h], Some([m[0], m[1], m[2], m[3], 0.0, 0.0]), content, res);
    (ap, file)
}

fn ap_dict(ap: ObjectId) -> Object {
    Object::Dictionary(lopdf::dictionary! { "N" => Object::Reference(ap) })
}

/// Evidenziatore, sottolineato e barrato.
pub fn mark(doc: &mut Document, page: ObjectId, geom: &Geom, kind: MarkKind, rects: &[Rect], color: Color, note: &str) -> Result<ObjectId, String> {
    if rects.is_empty() {
        return Err("niente da segnare".into());
    }
    let mut quad: Vec<Object> = Vec::new();
    let mut content = String::new();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut res = Dictionary::new();
    match kind {
        MarkKind::Highlight => {
            res.set("ExtGState", Object::Dictionary(lopdf::dictionary! { "GS" => lopdf::dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "ca" => 1.0, "CA" => 1.0 } }));
            content.push_str(&format!("/GS gs {}\n", fill_op(color)));
        }
        _ => content.push_str(&format!("{} 1.2 w\n", stroke_op(color))),
    }
    for r in rects {
        let (tl, tr, bl, br) = (geom.to_file(r.x, r.y), geom.to_file(r.right(), r.y), geom.to_file(r.x, r.bottom()), geom.to_file(r.right(), r.bottom()));
        for p in [tl, tr, bl, br] {
            quad.push(Object::Real(p.0));
            quad.push(Object::Real(p.1));
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        match kind {
            MarkKind::Highlight => {
                let _ = writeln!(content, "{} {} m {} {} l {} {} l {} {} l h f", num(bl.0), num(bl.1), num(br.0), num(br.1), num(tr.0), num(tr.1), num(tl.0), num(tl.1));
            }
            MarkKind::Underline => {
                let (a, b) = (geom.to_file(r.x, r.bottom() - 1.2), geom.to_file(r.right(), r.bottom() - 1.2));
                let _ = writeln!(content, "{} {} m {} {} l S", num(a.0), num(a.1), num(b.0), num(b.1));
            }
            MarkKind::Strike => {
                let (a, b) = (geom.to_file(r.x, r.y + r.h * 0.52), geom.to_file(r.right(), r.y + r.h * 0.52));
                let _ = writeln!(content, "{} {} m {} {} l S", num(a.0), num(a.1), num(b.0), num(b.1));
            }
        }
    }
    let bbox = [x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0];
    let ap = form_xobject(doc, bbox, None, content, res);
    let subtype = match kind {
        MarkKind::Highlight => "Highlight",
        MarkKind::Underline => "Underline",
        MarkKind::Strike => "StrikeOut",
    };
    let mut d = base_dict(subtype, bbox, color);
    d.set("QuadPoints", Object::Array(quad));
    d.set("AP", ap_dict(ap));
    if !note.is_empty() {
        d.set("Contents", text_obj(note));
    }
    add_annot(doc, page, d)
}

/// Un'area segnata per l'oscuramento (ancora da applicare): riquadro rosso tratteggiato con un velo.
pub fn redact_mark(doc: &mut Document, page: ObjectId, geom: &Geom, rect: Rect) -> Result<ObjectId, String> {
    let (w, h) = (rect.w.max(2.0), rect.h.max(2.0));
    let view = Rect::new(rect.x, rect.y, w, h);
    let mut res = Dictionary::new();
    res.set("ExtGState", Object::Dictionary(lopdf::dictionary! { "GS" => lopdf::dictionary! { "Type" => "ExtGState", "ca" => 0.14, "CA" => 1.0 } }));
    let red = [214, 40, 57];
    let mut c = String::new();
    let _ = writeln!(c, "/GS gs {} 0 0 {} {} re f", fill_op(red), num(w), num(h));
    let _ = writeln!(c, "{} 1.2 w [4 3] 0 d 0.6 0.6 {} {} re S", stroke_op(red), num(w - 1.2), num(h - 1.2));
    let (ap, file) = view_ap(doc, geom, view, c, res);
    let mut d = base_dict("Redact", file, red);
    d.set("Contents", text_obj(&crate::i18n::tr_string("Da oscurare")));
    d.set("AP", ap_dict(ap));
    add_annot(doc, page, d)
}

/// La nota: un fumetto giallo di 20 punti con il testo.
pub fn note(doc: &mut Document, page: ObjectId, geom: &Geom, x: f32, y: f32, text: &str, color: Color) -> Result<ObjectId, String> {
    let view = Rect::new(x, y, 20.0, 20.0);
    let mut c = String::new();
    let _ = writeln!(c, "{} 0.75 0.75 0.75 RG 1 w 1.5 3 17 14 re B", fill_op(color));
    let _ = writeln!(c, "0.2 0.2 0.2 RG 1 w 4.5 12.5 m 15.5 12.5 l S 4.5 9.5 m 15.5 9.5 l S 4.5 6.5 m 11 6.5 l S");
    let _ = writeln!(c, "{} 0.4 0.4 0.4 RG 1 w 4 3 m 4 0.5 l 8 3 l B", fill_op(color));
    let (ap, file) = view_ap(doc, geom, view, c, Dictionary::new());
    let mut d = base_dict("Text", file, color);
    d.set("Name", "Note");
    d.set("Contents", text_obj(text));
    d.set("Open", false);
    d.set("F", 4 | 8);
    d.set("AP", ap_dict(ap));
    add_annot(doc, page, d)
}

/// La matita: una linea a mano libera.
pub fn pen(doc: &mut Document, page: ObjectId, geom: &Geom, points: &[(f32, f32)], color: Color, width: f32) -> Result<ObjectId, String> {
    if points.len() < 2 {
        return Err("tratto troppo corto".into());
    }
    let w = width.max(0.5);
    let mut view = Rect::new(points[0].0, points[0].1, 0.0, 0.0);
    for p in points {
        view = view.union(&Rect::new(p.0, p.1, 0.0, 0.0));
    }
    let view = view.inflate(w + 1.0);
    let local = |p: &(f32, f32)| (p.0 - view.x, view.bottom() - p.1);
    let mut c = format!("{} {} w 1 J 1 j\n", stroke_op(color), num(w));
    for (i, p) in points.iter().enumerate() {
        let (x, y) = local(p);
        let _ = writeln!(c, "{} {} {}", num(x), num(y), if i == 0 { "m" } else { "l" });
    }
    c.push_str("S\n");
    let (ap, file) = view_ap(doc, geom, view, c, Dictionary::new());
    let mut ink = Vec::new();
    for p in points {
        let f = geom.to_file(p.0, p.1);
        ink.push(Object::Real(f.0));
        ink.push(Object::Real(f.1));
    }
    let mut d = base_dict("Ink", file, color);
    d.set("InkList", Object::Array(vec![Object::Array(ink)]));
    d.set("BS", Object::Dictionary(lopdf::dictionary! { "W" => w }));
    d.set("AP", ap_dict(ap));
    add_annot(doc, page, d)
}

/// Il timbro: il riquadro con il testo, nel colore scelto.
pub fn stamp(doc: &mut Document, page: ObjectId, geom: &Geom, rect: Rect, label: &str, color: Color) -> Result<ObjectId, String> {
    let (w, h) = (rect.w.max(20.0), rect.h.max(12.0));
    let view = Rect::new(rect.x, rect.y, w, h);
    let label = label.to_uppercase();
    let tw = crate::pdfedit::text_width(&label, true) / 1000.0;
    let size = (h * 0.52).min((w - 16.0) / tw.max(0.1)).max(4.0);
    let mut c = String::new();
    let lw = (h * 0.07).clamp(1.2, 3.0);
    let _ = writeln!(c, "{} {} w {} {} {} {} re S", stroke_op(color), num(lw), num(lw / 2.0), num(lw / 2.0), num(w - lw), num(h - lw));
    let _ = writeln!(c, "BT {} /F1 {} Tf {} {} Td {} Tj ET", fill_op(color), num(size), num((w - tw * size) / 2.0), num((h - size * 0.72) / 2.0), crate::pdfedit::pdf_string(&label));
    let res = lopdf::dictionary! { "Font" => lopdf::dictionary! { "F1" => lopdf::dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding" } } };
    let (ap, file) = view_ap(doc, geom, view, c, res);
    let mut d = base_dict("Stamp", file, color);
    d.set("Name", "Cedro");
    d.set("Contents", text_obj(&label));
    d.set("AP", ap_dict(ap));
    add_annot(doc, page, d)
}

/// Rettangolo, ellisse, linea o freccia.
#[allow(clippy::too_many_arguments)]
pub fn shape(doc: &mut Document, page: ObjectId, geom: &Geom, kind: ShapeKind, a: (f32, f32), b: (f32, f32), color: Color, fill: Option<Color>, width: f32) -> Result<ObjectId, String> {
    let w = width.max(0.5);
    let core = Rect::between(a, b);
    let head = if kind == ShapeKind::Arrow { (w * 4.0).max(9.0) } else { 0.0 };
    let view = core.inflate(w + head);
    let (view, core) = (Rect::new(view.x, view.y, view.w.max(4.0), view.h.max(4.0)), core);
    let local = |p: (f32, f32)| (p.0 - view.x, view.bottom() - p.1);
    let mut c = String::new();
    let _ = writeln!(c, "{} {} w 1 J 1 j", stroke_op(color), num(w));
    let mut d;
    match kind {
        ShapeKind::Rect | ShapeKind::Ellipse => {
            if let Some(f) = fill {
                let _ = writeln!(c, "{}", fill_op(f));
            }
            let paint = if fill.is_some() { "B" } else { "S" };
            let (x, y) = local((core.x, core.bottom()));
            let (cw, ch) = (core.w, core.h);
            if kind == ShapeKind::Rect {
                let _ = writeln!(c, "{} {} {} {} re {paint}", num(x), num(y), num(cw), num(ch));
            } else {
                let k = 0.552_284_7;
                let (cx, cy, rx, ry) = (x + cw / 2.0, y + ch / 2.0, cw / 2.0, ch / 2.0);
                let _ = writeln!(
                    c,
                    "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h {paint}",
                    num(cx + rx), num(cy),
                    num(cx + rx), num(cy + ry * k), num(cx + rx * k), num(cy + ry), num(cx), num(cy + ry),
                    num(cx - rx * k), num(cy + ry), num(cx - rx), num(cy + ry * k), num(cx - rx), num(cy),
                    num(cx - rx), num(cy - ry * k), num(cx - rx * k), num(cy - ry), num(cx), num(cy - ry),
                    num(cx + rx * k), num(cy - ry), num(cx + rx), num(cy - ry * k), num(cx + rx), num(cy)
                );
            }
            let (ap, file) = view_ap(doc, geom, view, c, Dictionary::new());
            d = base_dict(if kind == ShapeKind::Rect { "Square" } else { "Circle" }, file, color);
            d.set("BS", Object::Dictionary(lopdf::dictionary! { "W" => w }));
            if let Some(f) = fill {
                d.set("IC", rgb(f));
            }
            let infl = w + head;
            d.set("RD", Object::Array(vec![Object::Real(infl); 4]));
            d.set("AP", ap_dict(ap));
        }
        ShapeKind::Line | ShapeKind::Arrow => {
            let (p, q) = (local(a), local(b));
            let _ = writeln!(c, "{} {} m {} {} l S", num(p.0), num(p.1), num(q.0), num(q.1));
            if kind == ShapeKind::Arrow {
                let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                let len = (dx * dx + dy * dy).sqrt().max(0.1);
                let (ux, uy) = (dx / len, dy / len);
                let hl = head.min(len);
                for s in [-1.0f32, 1.0] {
                    let ang: f32 = 0.45 * s;
                    let (sn, cs) = ang.sin_cos();
                    let (rx, ry) = (ux * cs - uy * sn, ux * sn + uy * cs);
                    let _ = writeln!(c, "{} {} m {} {} l S", num(q.0), num(q.1), num(q.0 - rx * hl), num(q.1 - ry * hl));
                }
            }
            let (ap, file) = view_ap(doc, geom, view, c, Dictionary::new());
            d = base_dict("Line", file, color);
            let (fa, fb) = (geom.to_file(a.0, a.1), geom.to_file(b.0, b.1));
            d.set("L", Object::Array(vec![Object::Real(fa.0), Object::Real(fa.1), Object::Real(fb.0), Object::Real(fb.1)]));
            d.set("BS", Object::Dictionary(lopdf::dictionary! { "W" => w }));
            if kind == ShapeKind::Arrow {
                d.set("LE", Object::Array(vec![Object::Name(b"None".to_vec()), Object::Name(b"OpenArrow".to_vec())]));
            }
            d.set("AP", ap_dict(ap));
        }
    }
    add_annot(doc, page, d)
}

/// Un'immagine letta e pronta per il PDF.
pub struct Prepared {
    pub w: u32,
    pub h: u32,
    /// JPEG com'è (nessuna ricodifica) oppure pixel RGB.
    pub jpeg: Option<Vec<u8>>,
    pub gray_jpeg: bool,
    pub rgb: Vec<u8>,
    pub alpha: Option<Vec<u8>>,
}

pub fn prepare_image(data: &[u8]) -> Result<Prepared, String> {
    use image::ImageDecoder;
    let reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().map_err(|e| e.to_string())?;
    let is_jpeg = reader.format() == Some(image::ImageFormat::Jpeg);
    let mut dec = reader.into_decoder().map_err(|e| format!("immagine non letta: {e}"))?;
    let orient = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| format!("immagine non letta: {e}"))?;
    let turned = orient != image::metadata::Orientation::NoTransforms;
    img.apply_orientation(orient);
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("immagine vuota".into());
    }
    let comps = if is_jpeg && !turned { crate::pdfdoc::jpeg_info(data).map(|i| i.2) } else { None };
    if let Some(c @ (1 | 3)) = comps {
        return Ok(Prepared { w, h, jpeg: Some(data.to_vec()), gray_jpeg: c == 1, rgb: Vec::new(), alpha: None });
    }
    let rgba = img.to_rgba8();
    let has_alpha = rgba.pixels().any(|p| p.0[3] != 255);
    let rgb: Vec<u8> = rgba.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2]]).collect();
    let alpha = has_alpha.then(|| rgba.pixels().map(|p| p.0[3]).collect());
    Ok(Prepared { w, h, jpeg: None, gray_jpeg: false, rgb, alpha })
}

fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// Mette l'immagine nel file e la disegna nel rettangolo.
pub fn image(doc: &mut Document, page: ObjectId, geom: &Geom, rect: Rect, data: &[u8], counter: usize, wrapped: &mut HashSet<ObjectId>) -> Result<(), String> {
    let p = prepare_image(data)?;
    let mut dict = lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => p.w as i64, "Height" => p.h as i64, "BitsPerComponent" => 8 };
    let stream = if let Some(j) = &p.jpeg {
        dict.set("ColorSpace", if p.gray_jpeg { "DeviceGray" } else { "DeviceRGB" });
        dict.set("Filter", "DCTDecode");
        Stream::new(dict, j.clone()).with_compression(false)
    } else {
        dict.set("ColorSpace", "DeviceRGB");
        dict.set("Filter", "FlateDecode");
        if let Some(a) = &p.alpha {
            let sm = lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => p.w as i64, "Height" => p.h as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Filter" => "FlateDecode" };
            let sm_id = doc.add_object(Object::Stream(Stream::new(sm, deflate(a)).with_compression(false)));
            dict.set("SMask", Object::Reference(sm_id));
        }
        Stream::new(dict, deflate(&p.rgb)).with_compression(false)
    };
    let img_id = doc.add_object(Object::Stream(stream));
    let name = format!("CedroIm{counter}");
    super::newtext::add_resource(doc, page, "XObject", &name, Object::Reference(img_id))?;
    // L'immagine occupa il quadrato dell'unità: gli assi sono quelli della pagina vista.
    let bl = geom.to_file(rect.x, rect.bottom());
    let br = geom.to_file(rect.right(), rect.bottom());
    let tl = geom.to_file(rect.x, rect.y);
    let ops = format!("q {} {} {} {} {} {} cm /{name} Do Q\n", num(br.0 - bl.0), num(br.1 - bl.1), num(tl.0 - bl.0), num(tl.1 - bl.1), num(bl.0), num(bl.1));
    push_content(doc, page, &ops, wrapped)
}

/// Toglie un'annotazione che il file aveva già (e il suo fumetto, se c'è).
pub fn delete(doc: &mut Document, page: ObjectId, id: (u32, u16)) -> Result<(), String> {
    let existing = doc.get_dictionary(page).map_err(|e| e.to_string())?.get(b"Annots").ok().cloned();
    let (arr_id, mut arr) = match existing {
        Some(Object::Reference(r)) => match doc.get_object(r) {
            Ok(Object::Array(a)) => (Some(r), a.clone()),
            _ => return Ok(()),
        },
        Some(Object::Array(a)) => (None, a),
        _ => return Ok(()),
    };
    let popup = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Popup").ok()).and_then(|p| p.as_reference().ok());
    arr.retain(|o| o.as_reference().ok() != Some(id) && o.as_reference().ok() != popup);
    match arr_id {
        Some(r) => {
            doc.objects.insert(r, Object::Array(arr));
        }
        None => doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Annots", Object::Array(arr)),
    }
    Ok(())
}
