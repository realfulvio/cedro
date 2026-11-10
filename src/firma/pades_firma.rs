//! Firmare un PDF (PAdES): la firma va in un'aggiunta in coda al file (aggiornamento incrementale),
//! così il PDF di prima resta byte per byte com'era e le firme che c'erano restano valide.
//! Si usa lo spazio per la firma che il PDF ha già, oppure un riquadro nuovo dove dice chi firma,
//! oppure una firma che non si vede.

use super::asn1::R;
use super::firmare::{self, Key};
use lopdf::{Dictionary, Object, ObjectId};

/// Quanti byte si tengono per la busta della firma (certificati compresi).
const RESERVED: usize = 24 * 1024;

/// Uno spazio per la firma ancora vuoto, già nel PDF.
#[derive(Clone, Debug, PartialEq)]
pub struct EmptyField {
    pub name: String,
    /// Da 1.
    pub page: u32,
    /// In punti PDF (x0, y0, x1, y1).
    pub rect: [f32; 4],
}

/// Dove va la firma nel PDF.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// Nello spazio per la firma che c'è già.
    Field(String),
    /// In un riquadro nuovo: pagina (da 1) e rettangolo in punti PDF (x0, y0, x1, y1).
    Rect { page: u32, rect: [f32; 4] },
    /// La firma c'è ma sul foglio non si vede.
    Invisible,
}

/// Cosa si scrive nel riquadro e nella firma.
#[derive(Clone, Debug, Default)]
pub struct Look {
    pub name: String,
    pub reason: String,
    pub location: String,
    pub when: i64,
    /// Un'immagine a sinistra (stemma o firma scansionata): larghezza, altezza, RGB e trasparenza.
    pub image: Option<(u32, u32, Vec<u8>, Option<Vec<u8>>)>,
}

/// La grandezza di una pagina (larghezza, altezza in punti) e il suo riquadro di partenza.
pub fn page_box(doc: &lopdf::Document, page: ObjectId) -> [f32; 4] {
    let mut id = page;
    for _ in 0..16 {
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(a) = d.get(b"CropBox").or_else(|_| d.get(b"MediaBox")).and_then(|o| doc.dereference(o).map(|x| x.1)).and_then(|o| o.as_array()) {
            let v: Vec<f32> = a.iter().filter_map(|o| doc.dereference(o).ok().and_then(|x| x.1.as_float().ok())).collect();
            if v.len() == 4 {
                return [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])];
            }
        }
        match d.get(b"Parent").and_then(|p| p.as_reference()) {
            Ok(p) => id = p,
            Err(_) => break,
        }
    }
    [0.0, 0.0, 595.0, 842.0]
}

fn text_of(o: &Object) -> String {
    match o.as_str() {
        Ok(b) if b.starts_with(&[0xFE, 0xFF]) => String::from_utf16_lossy(&b[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect::<Vec<_>>()),
        Ok(b) => b.iter().map(|&c| crate::doctext::cp1252(c)).collect(),
        Err(_) => String::new(),
    }
}

/// Tutti i campi del modulo, anche dentro i gruppi (Kids), con il nome completo «a.b».
fn all_fields(doc: &lopdf::Document) -> Vec<(ObjectId, String)> {
    let mut out = Vec::new();
    let Ok(root) = doc.trailer.get(b"Root").and_then(|r| r.as_reference()) else { return out };
    let Ok(cat) = doc.get_dictionary(root) else { return out };
    let Ok((_, Object::Dictionary(af))) = cat.get(b"AcroForm").and_then(|o| doc.dereference(o)) else { return out };
    let Ok((_, Object::Array(fields))) = af.get(b"Fields").and_then(|o| doc.dereference(o)) else { return out };
    let mut stack: Vec<(ObjectId, String, u32)> = fields.iter().filter_map(|f| f.as_reference().ok()).map(|id| (id, String::new(), 0)).collect();
    while let Some((id, prefix, depth)) = stack.pop() {
        if depth > 20 {
            continue;
        }
        let Ok(d) = doc.get_dictionary(id) else { continue };
        let t = d.get(b"T").map(text_of).unwrap_or_default();
        let full = match (prefix.is_empty(), t.is_empty()) {
            (_, true) => prefix.clone(),
            (true, false) => t,
            (false, false) => format!("{prefix}.{t}"),
        };
        if !full.is_empty() && d.get(b"T").is_ok() {
            out.push((id, full.clone()));
        }
        if let Ok(kids) = d.get(b"Kids").and_then(|k| doc.dereference(k)).and_then(|x| x.1.as_array()) {
            for k in kids.iter().filter_map(|k| k.as_reference().ok()) {
                stack.push((k, full.clone(), depth + 1));
            }
        }
    }
    out
}

fn is_sig_field(doc: &lopdf::Document, mut id: ObjectId) -> bool {
    for _ in 0..20 {
        let Ok(d) = doc.get_dictionary(id) else { return false };
        if let Ok(ft) = d.get(b"FT").and_then(|o| o.as_name()) {
            return ft == b"Sig";
        }
        match d.get(b"Parent").and_then(|p| p.as_reference()) {
            Ok(p) => id = p,
            Err(_) => return false,
        }
    }
    false
}

/// Il riquadro di un campo: nel campo stesso o nel primo figlio (widget), con la pagina.
fn widget_of(doc: &lopdf::Document, id: ObjectId, pages: &[(u32, ObjectId)]) -> Option<(ObjectId, u32, [f32; 4])> {
    let d = doc.get_dictionary(id).ok()?;
    let mut ids = vec![id];
    if let Ok(kids) = d.get(b"Kids").and_then(|k| doc.dereference(k)).and_then(|x| x.1.as_array()) {
        ids = kids.iter().filter_map(|k| k.as_reference().ok()).collect();
    }
    for w in ids {
        let Ok(wd) = doc.get_dictionary(w) else { continue };
        let Ok(rect) = wd.get(b"Rect").and_then(|o| doc.dereference(o).map(|x| x.1)).and_then(|o| o.as_array()) else { continue };
        let v: Vec<f32> = rect.iter().filter_map(|o| o.as_float().ok()).collect();
        if v.len() != 4 {
            continue;
        }
        let page = wd.get(b"P").ok().and_then(|p| p.as_reference().ok()).and_then(|p| pages.iter().find(|x| x.1 == p).map(|x| x.0)).or_else(|| {
            pages.iter().find(|(_, pid)| {
                doc.get_dictionary(*pid).ok().and_then(|pd| pd.get(b"Annots").ok()).and_then(|a| doc.dereference(a).ok()).and_then(|x| x.1.as_array().ok()).is_some_and(|a| a.iter().any(|o| o.as_reference().ok() == Some(w)))
            }).map(|x| x.0)
        })?;
        return Some((w, page, [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]));
    }
    None
}

/// Gli spazi per la firma ancora vuoti (per «Nello spazio per la firma»).
pub fn empty_fields(data: &[u8]) -> Vec<EmptyField> {
    let Ok(doc) = lopdf::Document::load_mem(data) else { return Vec::new() };
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let mut out = Vec::new();
    for (id, name) in all_fields(&doc) {
        let Ok(d) = doc.get_dictionary(id) else { continue };
        if d.get(b"V").is_ok() || !is_sig_field(&doc, id) {
            continue;
        }
        if let Some((_, page, rect)) = widget_of(&doc, id, &pages) {
            if rect[2] - rect[0] > 1.0 && rect[3] - rect[1] > 1.0 {
                out.push(EmptyField { name, page, rect });
            }
        }
    }
    out.sort_by(|a, b| a.page.cmp(&b.page).then(b.rect[1].total_cmp(&a.rect[1])));
    out
}

/// Quante pagine ha il PDF e quanto sono grandi (per mettere il riquadro).
pub fn pages(data: &[u8]) -> R<Vec<[f32; 4]>> {
    let doc = lopdf::Document::load_mem(data).map_err(|e| format!("PDF illeggibile: {e}"))?;
    Ok(doc.get_pages().values().map(|id| page_box(&doc, *id)).collect())
}

/// Il posto proposto per un riquadro nuovo: in basso a destra dell'ultima pagina.
pub fn default_rect(page_box: [f32; 4]) -> [f32; 4] {
    let (w, h) = (200.0f32.min((page_box[2] - page_box[0]) * 0.45), 64.0);
    let x1 = page_box[2] - 42.0;
    let y0 = page_box[1] + 52.0;
    [x1 - w, y0, x1, y0 + h]
}

// ---- Scrittura degli oggetti PDF ----

fn name_bytes(out: &mut Vec<u8>, n: &[u8]) {
    out.push(b'/');
    for &c in n {
        if c.is_ascii_alphanumeric() || b"-_.+:".contains(&c) {
            out.push(c);
        } else {
            out.extend(format!("#{c:02X}").as_bytes());
        }
    }
}

fn hex(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'<');
    for b in s {
        out.extend(format!("{b:02X}").as_bytes());
    }
    out.push(b'>');
}

fn real(v: f32) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn ser(out: &mut Vec<u8>, o: &Object) {
    match o {
        Object::Null => out.extend(b"null"),
        Object::Boolean(b) => out.extend(if *b { &b"true"[..] } else { &b"false"[..] }),
        Object::Integer(i) => out.extend(i.to_string().as_bytes()),
        Object::Real(f) => out.extend(real(*f).as_bytes()),
        Object::Name(n) => name_bytes(out, n),
        Object::String(s, _) => hex(out, s),
        Object::Array(a) => {
            out.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                ser(out, x);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => ser_dict(out, d),
        Object::Stream(s) => {
            let mut d = s.dict.clone();
            d.set("Length", s.content.len() as i64);
            ser_dict(out, &d);
            out.extend(b"\nstream\n");
            out.extend(&s.content);
            out.extend(b"\nendstream");
        }
        Object::Reference((n, g)) => out.extend(format!("{n} {g} R").as_bytes()),
    }
}

fn ser_dict(out: &mut Vec<u8>, d: &Dictionary) {
    out.extend(b"<<");
    for (k, v) in d.iter() {
        name_bytes(out, k);
        out.push(b' ');
        ser(out, v);
    }
    out.extend(b">>");
}

/// Una stringa di testo PDF: latina se basta, altrimenti UTF-16.
fn text_string(s: &str) -> Object {
    if s.chars().all(|c| (c as u32) < 128) {
        Object::String(s.as_bytes().to_vec(), lopdf::StringFormat::Literal)
    } else {
        let mut b = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            b.extend(u.to_be_bytes());
        }
        Object::String(b, lopdf::StringFormat::Hexadecimal)
    }
}

/// «D:20261109224130+01'00'» nell'ora italiana.
pub fn pdf_date(t: i64) -> String {
    let l = super::local(t);
    let off = (super::italy(t) - t) / 3600;
    format!("D:{:04}{:02}{:02}{:02}{:02}{:02}+{:02}'00'", l.y, l.mo, l.d, l.h, l.mi, l.s, off)
}

/// Testo per i caratteri standard (WinAnsi), con le parentesi protette.
fn winansi(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.chars() {
        let b = to_cp1252(c).unwrap_or(b'?');
        if b == b'(' || b == b')' || b == b'\\' {
            out.push(b'\\');
        }
        out.push(b);
    }
    out
}

fn to_cp1252(c: char) -> Option<u8> {
    if (c as u32) < 0x80 {
        return Some(c as u8);
    }
    (0x80u8..=0xFF).find(|&b| crate::doctext::cp1252(b) == c)
}

/// Larghezza approssimata del testo in Helvetica (unità da 1000).
fn helv_width(s: &str, size: f32, bold: bool) -> f32 {
    let w: f32 = s
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '\'' | '.' | ',' | ':' | ';' | '!' | '|' => 230.0,
            'f' | 't' | 'r' | 'I' | ' ' | '(' | ')' | '/' => 300.0,
            'm' | 'w' | 'M' | 'W' => 850.0,
            'A'..='Z' => 690.0,
            '0'..='9' => 556.0,
            _ => 540.0,
        })
        .sum();
    w * size / 1000.0 * if bold { 1.06 } else { 1.0 }
}

/// Il disegno del riquadro: immagine a sinistra, poi «Firmato digitalmente da», il nome, data e ora,
/// motivo e luogo. Il testo si rimpicciolisce per starci.
fn appearance(w: f32, h: f32, look: &Look, has_image: bool) -> Vec<u8> {
    let mut lines: Vec<(String, bool)> = vec![("Firmato digitalmente da".into(), false), (look.name.clone(), true), (format!("Data: {}", super::date_time(look.when)), false)];
    if !look.reason.is_empty() {
        lines.push((format!("Motivo: {}", look.reason), false));
    }
    if !look.location.is_empty() {
        lines.push((format!("Luogo: {}", look.location), false));
    }
    let pad = 4.0;
    let img_w = if has_image { (h - 2.0 * pad).min(w * 0.33) } else { 0.0 };
    let tx = pad + if has_image { img_w + 6.0 } else { 0.0 };
    let tw = (w - tx - pad).max(10.0);
    let n = lines.len() as f32;
    let mut size = ((h - 2.0 * pad) / (n * 1.18)).min(11.0);
    for (t, b) in &lines {
        let need = helv_width(t, if *b { size * 1.15 } else { size }, *b);
        if need > tw {
            size = size * tw / need;
        }
    }
    let size = size.max(3.0);
    let mut s = Vec::new();
    if has_image {
        let ih = img_w;
        s.extend(format!("q {} 0 0 {} {} {} cm /Im1 Do Q\n", real(img_w), real(ih), real(pad), real((h - ih) / 2.0)).as_bytes());
    }
    let total = n * size * 1.18 + size * 0.15;
    let mut y = h - (h - total) / 2.0 - size;
    s.extend(b"BT 0.1 0.1 0.18 rg\n");
    for (t, b) in &lines {
        let fs = if *b { size * 1.15 } else { size };
        s.extend(format!("/{} {} Tf 1 0 0 1 {} {} Tm (", if *b { "F2" } else { "F1" }, real(fs), real(tx), real(y)).as_bytes());
        s.extend(winansi(t));
        s.extend(b") Tj\n");
        y -= size * 1.18;
    }
    s.extend(b"ET\n");
    s
}

/// Il PDF firmato: l'originale com'è, più l'aggiunta con la firma.
pub fn sign(data: &[u8], key: &dyn Key, place: &Place, look: &Look) -> R<Vec<u8>> {
    let doc = lopdf::Document::load_mem(data).map_err(|e| format!("il PDF non si legge: {e}"))?;
    if doc.trailer.get(b"Encrypt").is_ok() {
        return Err("il PDF è protetto con una password: toglila prima (Conversioni › PDF › Sblocca) e poi firmalo".into());
    }
    let root = doc.trailer.get(b"Root").and_then(|r| r.as_reference()).map_err(|_| "PDF senza catalogo".to_string())?;
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    if pages.is_empty() {
        return Err("il PDF non ha pagine".into());
    }
    let size = doc.trailer.get(b"Size").and_then(|s| s.as_i64()).unwrap_or(0).max(doc.max_id as i64 + 1) as u32;
    let mut next = size;
    let mut alloc = || {
        let id = (next, 0u16);
        next += 1;
        id
    };
    // Gli oggetti da scrivere nell'aggiunta (quelli rifatti tengono il loro numero).
    let mut objs: Vec<(ObjectId, Vec<u8>)> = Vec::new();
    let sig_id = alloc();
    let font_id = alloc();
    let bold_id = alloc();
    let mut catalog = doc.get_dictionary(root).map_err(|_| "PDF senza catalogo".to_string())?.clone();
    // Il modulo (AcroForm): dentro il catalogo o a parte.
    let (af_id, mut af): (Option<ObjectId>, Dictionary) = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(r)) => (Some(*r), doc.get_dictionary(*r).cloned().unwrap_or_default()),
        Ok(Object::Dictionary(d)) => (None, d.clone()),
        _ => (None, Dictionary::new()),
    };
    af.set("SigFlags", 3);
    let all = all_fields(&doc);
    let field_count = all.len();
    let mut new_field = None;
    let (widget, page_no, rect): (Option<ObjectId>, u32, [f32; 4]) = match place {
        Place::Field(name) => {
            let (fid, _) = all.iter().find(|(id, n)| n == name && is_sig_field(&doc, *id)).ok_or_else(|| format!("nel PDF non c'è lo spazio per la firma «{name}»"))?;
            let fd = doc.get_dictionary(*fid).map_err(|e| e.to_string())?;
            if fd.get(b"V").is_ok() {
                return Err(format!("lo spazio «{name}» è già firmato"));
            }
            let (w, p, r) = widget_of(&doc, *fid, &pages).ok_or("lo spazio per la firma non ha un riquadro")?;
            let mut fd = fd.clone();
            fd.set("V", Object::Reference(sig_id));
            if w == *fid {
                (Some(*fid), p, r).also(|_| new_field = Some((*fid, fd)))
            } else {
                let mut buf = Vec::new();
                ser_dict(&mut buf, &fd);
                objs.push((*fid, buf));
                (Some(w), p, r)
            }
        }
        Place::Rect { page, rect } => {
            let id = alloc();
            (Some(id), (*page).clamp(1, pages.len() as u32), *rect)
        }
        Place::Invisible => {
            let id = alloc();
            (Some(id), 1, [0.0; 4])
        }
    };
    let widget = widget.unwrap();
    let page_id = pages.iter().find(|x| x.0 == page_no).map(|x| x.1).unwrap_or(pages[0].1);
    let visible = rect[2] - rect[0] > 1.0 && rect[3] - rect[1] > 1.0;
    // Il disegno del riquadro.
    let mut ap = None;
    if visible {
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let ap_id = alloc();
        let img_id = look.image.as_ref().map(|_| alloc());
        let mut res = format!("<</Font<</F1 {} 0 R/F2 {} 0 R>>", font_id.0, bold_id.0);
        if let Some(i) = img_id {
            res.push_str(&format!("/XObject<</Im1 {} 0 R>>", i.0));
        }
        res.push_str(">>");
        let body = appearance(w, h, look, img_id.is_some());
        let mut buf = format!("<</Type/XObject/Subtype/Form/BBox[0 0 {} {}]/Resources{}/Length {}>>\nstream\n", real(w), real(h), res, body.len()).into_bytes();
        buf.extend(&body);
        buf.extend(b"\nendstream");
        objs.push((ap_id, buf));
        if let (Some(iid), Some((iw, ih, rgb, alpha))) = (img_id, &look.image) {
            let mut smask = String::new();
            if let Some(a) = alpha {
                let sid = alloc();
                let z = deflate(a);
                let mut b = format!("<</Type/XObject/Subtype/Image/Width {iw}/Height {ih}/ColorSpace/DeviceGray/BitsPerComponent 8/Filter/FlateDecode/Length {}>>\nstream\n", z.len()).into_bytes();
                b.extend(&z);
                b.extend(b"\nendstream");
                objs.push((sid, b));
                smask = format!("/SMask {} 0 R", sid.0);
            }
            let z = deflate(rgb);
            let mut b = format!("<</Type/XObject/Subtype/Image/Width {iw}/Height {ih}/ColorSpace/DeviceRGB/BitsPerComponent 8/Filter/FlateDecode{smask}/Length {}>>\nstream\n", z.len()).into_bytes();
            b.extend(&z);
            b.extend(b"\nendstream");
            objs.push((iid, b));
        }
        ap = Some(ap_id);
    }
    objs.push((font_id, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding/WinAnsiEncoding>>".to_vec()));
    objs.push((bold_id, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica-Bold/Encoding/WinAnsiEncoding>>".to_vec()));
    // Il riquadro (widget), unito al campo se il campo è nuovo.
    let mut wd = match (&new_field, place) {
        (Some((_, d)), _) => d.clone(),
        (None, Place::Field(_)) => doc.get_dictionary(widget).cloned().unwrap_or_default(),
        _ => {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Annot".to_vec()));
            d.set("Subtype", Object::Name(b"Widget".to_vec()));
            d.set("FT", Object::Name(b"Sig".to_vec()));
            let mut n = field_count + 1;
            let names: Vec<&String> = all.iter().map(|x| &x.1).collect();
            while names.iter().any(|x| **x == format!("Firma{n}")) {
                n += 1;
            }
            d.set("T", text_string(&format!("Firma{n}")));
            d.set("V", Object::Reference(sig_id));
            d.set("Rect", Object::Array(rect.iter().map(|v| Object::Real(*v)).collect()));
            d.set("P", Object::Reference(page_id));
            d
        }
    };
    // Stampabile e bloccato.
    wd.set("F", 132);
    if let Some(a) = ap {
        let mut apd = Dictionary::new();
        apd.set("N", Object::Reference(a));
        wd.set("AP", Object::Dictionary(apd));
    }
    let mut buf = Vec::new();
    ser_dict(&mut buf, &wd);
    objs.push((widget, buf));
    let brand_new = !matches!(place, Place::Field(_));
    if brand_new {
        // Il riquadro nell'elenco della pagina e il campo nell'elenco del modulo.
        let mut pd = doc.get_dictionary(page_id).map_err(|e| e.to_string())?.clone();
        match pd.get(b"Annots") {
            Ok(Object::Reference(r)) => {
                let r = *r;
                let mut a = doc.get_object(r).ok().and_then(|o| o.as_array().ok()).cloned().unwrap_or_default();
                a.push(Object::Reference(widget));
                let mut b = Vec::new();
                ser(&mut b, &Object::Array(a));
                objs.push((r, b));
            }
            Ok(Object::Array(a)) => {
                let mut a = a.clone();
                a.push(Object::Reference(widget));
                pd.set("Annots", Object::Array(a));
                let mut b = Vec::new();
                ser_dict(&mut b, &pd);
                objs.push((page_id, b));
            }
            _ => {
                pd.set("Annots", Object::Array(vec![Object::Reference(widget)]));
                let mut b = Vec::new();
                ser_dict(&mut b, &pd);
                objs.push((page_id, b));
            }
        }
        match af.get(b"Fields") {
            Ok(Object::Reference(r)) => {
                let r = *r;
                let mut a = doc.get_object(r).ok().and_then(|o| o.as_array().ok()).cloned().unwrap_or_default();
                a.push(Object::Reference(widget));
                let mut b = Vec::new();
                ser(&mut b, &Object::Array(a));
                objs.push((r, b));
            }
            Ok(Object::Array(a)) => {
                let mut a = a.clone();
                a.push(Object::Reference(widget));
                af.set("Fields", Object::Array(a));
            }
            _ => af.set("Fields", Object::Array(vec![Object::Reference(widget)])),
        }
    }
    match af_id {
        Some(id) => {
            let mut b = Vec::new();
            ser_dict(&mut b, &af);
            objs.push((id, b));
        }
        None => {
            catalog.set("AcroForm", Object::Dictionary(af));
            let mut b = Vec::new();
            ser_dict(&mut b, &catalog);
            objs.push((root, b));
        }
    }
    // Il dizionario della firma, con i buchi per /ByteRange e /Contents.
    let mut sd = b"<</Type/Sig/Filter/Adobe.PPKLite/SubFilter/ETSI.CAdES.detached/ByteRange[0 ".to_vec();
    let br_at = sd.len();
    sd.extend(format!("{:<10} {:<10} {:<10}]", 0, 0, 0).as_bytes());
    sd.extend(b"/Contents ");
    let ct_at = sd.len();
    sd.push(b'<');
    sd.extend(std::iter::repeat_n(b'0', RESERVED * 2));
    sd.push(b'>');
    let mut extra = Dictionary::new();
    extra.set("M", Object::String(pdf_date(look.when).into_bytes(), lopdf::StringFormat::Literal));
    if !look.name.is_empty() {
        extra.set("Name", text_string(&look.name));
    }
    if !look.reason.is_empty() {
        extra.set("Reason", text_string(&look.reason));
    }
    if !look.location.is_empty() {
        extra.set("Location", text_string(&look.location));
    }
    let mut eb = Vec::new();
    ser_dict(&mut eb, &extra);
    sd.extend(&eb[2..eb.len() - 2]);
    sd.extend(b">>");
    objs.push((sig_id, sd));

    // L'aggiunta in coda.
    let mut out = data.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    objs.sort_by_key(|x| x.0);
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    let mut sig_pos = 0;
    for (id, body) in &objs {
        offsets.push((id.0, out.len()));
        let head = format!("{} {} obj\n", id.0, id.1);
        if *id == sig_id {
            sig_pos = out.len() + head.len();
        }
        out.extend(head.as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let prev = doc.xref_start;
    let stream_xref = matches!(doc.reference_table.cross_reference_type, lopdf::xref::XrefType::CrossReferenceStream);
    let mut trailer = Dictionary::new();
    trailer.set("Size", next as i64 + if stream_xref { 1 } else { 0 });
    trailer.set("Root", Object::Reference(root));
    if let Ok(i) = doc.trailer.get(b"Info") {
        trailer.set("Info", i.clone());
    }
    if let Ok(i) = doc.trailer.get(b"ID") {
        trailer.set("ID", i.clone());
    }
    trailer.set("Prev", prev as i64);
    let xref_at = out.len();
    if stream_xref {
        let xid = next;
        offsets.push((xid, xref_at));
        offsets.sort();
        let mut rows = Vec::new();
        let mut index = Vec::new();
        for run in runs(&offsets) {
            index.push(Object::Integer(run[0].0 as i64));
            index.push(Object::Integer(run.len() as i64));
            for (_, off) in run {
                rows.push(1u8);
                rows.extend((*off as u32).to_be_bytes());
                rows.extend([0u8, 0]);
            }
        }
        trailer.set("Type", Object::Name(b"XRef".to_vec()));
        trailer.set("W", Object::Array(vec![Object::Integer(1), Object::Integer(4), Object::Integer(2)]));
        trailer.set("Index", Object::Array(index));
        trailer.set("Length", rows.len() as i64);
        out.extend(format!("{xid} 0 obj\n").as_bytes());
        ser_dict(&mut out, &trailer);
        out.extend(b"\nstream\n");
        out.extend(&rows);
        out.extend(b"\nendstream\nendobj\n");
    } else {
        offsets.sort();
        out.extend(b"xref\n");
        for run in runs(&offsets) {
            out.extend(format!("{} {}\n", run[0].0, run.len()).as_bytes());
            for (_, off) in run {
                out.extend(format!("{off:010} 00000 n\r\n").as_bytes());
            }
        }
        out.extend(b"trailer\n");
        ser_dict(&mut out, &trailer);
        out.push(b'\n');
    }
    out.extend(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());

    // ByteRange: tutto tranne il buco di /Contents (i < > compresi nel buco).
    let hole_start = sig_pos + ct_at;
    let hole_end = hole_start + RESERVED * 2 + 2;
    let range = [0, hole_start, hole_end, out.len() - hole_end];
    let br = format!("{:<10} {:<10} {:<10}]", range[1], range[2], range[3]);
    out[sig_pos + br_at..sig_pos + br_at + br.len()].copy_from_slice(br.as_bytes());
    let digest = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(&out[..hole_start]);
        h.update(&out[hole_end..]);
        h.finalize().to_vec()
    };
    let cms = firmare::pades_cms(&digest, key)?;
    if cms.len() > RESERVED {
        return Err("la firma è troppo grande per lo spazio riservato nel PDF".into());
    }
    let hexed: String = cms.iter().map(|b| format!("{b:02X}")).collect();
    out[hole_start + 1..hole_start + 1 + hexed.len()].copy_from_slice(hexed.as_bytes());
    Ok(out)
}

/// Le righe del xref a gruppi di numeri consecutivi.
fn runs(offsets: &[(u32, usize)]) -> Vec<&[(u32, usize)]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=offsets.len() {
        if i == offsets.len() || offsets[i].0 != offsets[i - 1].0 + 1 {
            out.push(&offsets[start..i]);
            start = i;
        }
    }
    out
}

fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

trait Also: Sized {
    fn also(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
impl<T> Also for T {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::firma::firmare::{self_signed, SoftKey};
    use crate::firma::verify::{verify_bytes, Opts};
    use crate::firma::Level;

    fn soft(cn: &str, seed: u8) -> SoftKey {
        let key = p256::ecdsa::SigningKey::from_bytes(&[seed; 32].into()).unwrap();
        SoftKey { cert: self_signed(cn, &key), key }
    }

    fn data(n: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/firma").join(n)).unwrap()
    }

    fn look(n: &str) -> Look {
        Look { name: n.into(), reason: "Approvazione".into(), location: "Bondeno".into(), when: crate::firma::now(), image: Some((2, 2, vec![200; 12], Some(vec![255; 4]))) }
    }

    fn doc_ok(r: &crate::firma::Report, who: &str) -> crate::firma::Signer {
        let s = r.signers.iter().find(|s| s.name.contains(who)).unwrap_or_else(|| panic!("manca {who}: {}", r.detail)).clone();
        let c = s.checks.iter().find(|c| c.what == "Documento").unwrap();
        assert_eq!(c.level, Level::Ok, "{who}: {} {:?}", c.text, s.checks.iter().map(|c| format!("{}:{}", c.what, c.text)).collect::<Vec<_>>());
        s
    }

    /// Un PDF di una pagina, con o senza uno spazio per la firma già pronto.
    pub(crate) fn blank_pdf(with_field: bool) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(lopdf::Stream::new(Dictionary::new(), b"BT /F1 12 Tf 72 720 Td (Determina) Tj ET".to_vec()));
        let page_id = doc.new_object_id();
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("MediaBox", Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]));
        page.set("Contents", Object::Reference(content));
        let mut cat = lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if with_field {
            let field_id = doc.new_object_id();
            let mut field = Dictionary::new();
            field.set("Type", Object::Name(b"Annot".to_vec()));
            field.set("Subtype", Object::Name(b"Widget".to_vec()));
            field.set("FT", Object::Name(b"Sig".to_vec()));
            field.set("T", Object::String(b"Firma del Dirigente".to_vec(), lopdf::StringFormat::Literal));
            field.set("Rect", Object::Array(vec![350.into(), 60.into(), 550.into(), 120.into()]));
            field.set("P", Object::Reference(page_id));
            doc.objects.insert(field_id, Object::Dictionary(field));
            page.set("Annots", Object::Array(vec![Object::Reference(field_id)]));
            let mut af = Dictionary::new();
            af.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
            cat.set("AcroForm", af);
        }
        doc.objects.insert(page_id, Object::Dictionary(page));
        let mut pages_d = Dictionary::new();
        pages_d.set("Type", Object::Name(b"Pages".to_vec()));
        pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_d.set("Count", 1);
        doc.objects.insert(pages_id, Object::Dictionary(pages_d));
        let cat = doc.add_object(cat);
        doc.trailer.set("Root", cat);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();
        pdf
    }

    #[test]
    fn riquadro_nuovo_e_seconda_firma() {
        let pdf = blank_pdf(false);
        let mario = soft("MARIO ROSSI", 7);
        let boxes = pages(&pdf).unwrap();
        let one = sign(&pdf, &mario, &Place::Rect { page: boxes.len() as u32, rect: default_rect(boxes[boxes.len() - 1]) }, &look("MARIO ROSSI")).unwrap();
        assert!(one.starts_with(&pdf));
        let r = verify_bytes("x.pdf", &one, &Opts { online: false, ..Default::default() });
        let s = doc_ok(&r, "MARIO");
        let p = s.place.unwrap();
        assert!(p.visible);
        assert_eq!(p.field.as_deref(), Some("Firma1"));
        assert!(s.kind.starts_with("PAdES"), "{}", s.kind);

        // Seconda firma, invisibile: la prima resta valida.
        let giulia = soft("GIULIA BIANCHI", 9);
        let two = sign(&one, &giulia, &Place::Invisible, &look("GIULIA BIANCHI")).unwrap();
        let r = verify_bytes("x.pdf", &two, &Opts { online: false, ..Default::default() });
        assert_eq!(r.signers.len(), 2);
        doc_ok(&r, "MARIO");
        let g = doc_ok(&r, "GIULIA");
        assert!(!g.place.unwrap().visible);
        assert!(lopdf::Document::load_mem(&two).is_ok());
    }

    #[test]
    fn su_pdf_gia_firmato_con_xref_stream() {
        // La determina firmata da pyhanko (xref a flusso): aggiungo una firma.
        let pdf = data("determina_firmata.pdf");
        let anna = soft("ANNA VERDI", 11);
        let out = sign(&pdf, &anna, &Place::Rect { page: 1, rect: [300.0, 80.0, 520.0, 140.0] }, &look("ANNA VERDI")).unwrap();
        let r = verify_bytes("d.pdf", &out, &Opts { online: false, ..Default::default() });
        assert_eq!(r.signers.len(), 2);
        doc_ok(&r, "ANNA");
        let first = &r.signers[0];
        assert_eq!(first.checks[0].level, Level::Ok, "{:?}", first.checks.iter().map(|c| format!("{}:{}", c.what, c.text)).collect::<Vec<_>>());
    }

    #[test]
    fn spazio_gia_pronto() {
        let pdf = blank_pdf(true);
        let f = empty_fields(&pdf);
        assert_eq!(f, vec![EmptyField { name: "Firma del Dirigente".into(), page: 1, rect: [350.0, 60.0, 550.0, 120.0] }]);
        let mario = soft("MARIO ROSSI", 7);
        let out = sign(&pdf, &mario, &Place::Field("Firma del Dirigente".into()), &look("MARIO ROSSI")).unwrap();
        let r = verify_bytes("x.pdf", &out, &Opts { online: false, ..Default::default() });
        let s = doc_ok(&r, "MARIO");
        assert_eq!(s.place.as_ref().unwrap().field.as_deref(), Some("Firma del Dirigente"));
        assert!(s.place.unwrap().visible);
        assert!(empty_fields(&out).is_empty());
    }
}
