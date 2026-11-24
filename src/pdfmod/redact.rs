//! L'oscuramento vero: dentro le aree si tolgono dal contenuto i caratteri (uno per uno) e i pixel delle
//! immagini, poi si disegna il riquadro nero. Quello che sta sotto non c'è più nel file: non basta
//! coprirlo. Si tolgono anche i commenti e i campi che stanno sotto, e la miniatura della pagina.

use std::collections::HashSet;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::annot;
use super::fonts::{deref, dict_of, name_of, num};
use super::geom::Geom;
use super::text::{remove_glyphs, write_back, Cont, ImageUse, TextPage};
use super::Rect;
use crate::pdfedit::num as fmt_num;

/// Cosa si è tolto davvero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub glyphs: usize,
    pub images: usize,
    pub annots: usize,
}

impl Report {
    pub fn add(&mut self, o: Report) {
        self.glyphs += o.glyphs;
        self.images += o.images;
        self.annots += o.annots;
    }
}

fn inside(rects: &[Rect], r: &Rect) -> bool {
    let area = (r.w * r.h).max(1e-3);
    rects.iter().any(|a| {
        if a.contains(r.x + r.w / 2.0, r.y + r.h / 2.0) {
            return true;
        }
        let w = (a.right().min(r.right()) - a.x.max(r.x)).max(0.0);
        let h = (a.bottom().min(r.bottom()) - a.y.max(r.y)).max(0.0);
        w * h >= 0.25 * area
    })
}

/// Oscura le aree `rects` (pagina vista) di una pagina. `orig` è il file com'era all'apertura (serve a leggere
/// le immagini compresse in modi che lopdf non scioglie: JPEG, JPEG 2000, JBIG2, CCITT).
pub fn redact_page(doc: &mut Document, orig: &[u8], page: ObjectId, geom: &Geom, rects: &[Rect], wrapped: &mut HashSet<ObjectId>, counter: &mut usize) -> Result<Report, String> {
    if rects.is_empty() {
        return Ok(Report::default());
    }
    let mut rep = Report::default();
    let mut tp = TextPage::read(doc, page);
    if !tp.complete {
        return Err("La pagina ha un contenuto che non so leggere tutto: non posso oscurarla con sicurezza.".into());
    }
    let original = tp.ops.clone();

    // 1. Le immagini: ognuna toccata diventa una copia con i pixel dell'area tolti.
    let hits: Vec<ImageUse> = tp
        .images
        .iter()
        .filter(|u| {
            let xs = u.quad.iter().map(|p| p.0);
            let ys = u.quad.iter().map(|p| p.1);
            let b = Rect::new(xs.clone().fold(f32::MAX, f32::min), ys.clone().fold(f32::MAX, f32::min), 0.0, 0.0);
            let b = Rect::new(b.x, b.y, xs.fold(f32::MIN, f32::max) - b.x, ys.fold(f32::MIN, f32::max) - b.y);
            rects.iter().any(|r| r.intersects(&b))
        })
        .cloned()
        .collect();
    let mut pdf: Option<hayro::hayro_syntax::Pdf> = None;
    for u in &hits {
        let Some(id) = u.id else {
            // Un'immagine dentro il flusso: si azzerano i suoi byte.
            if let Some(op) = tp.ops.get_mut(&u.cont).and_then(|o| o.get_mut(u.op)) {
                zero_inline(op);
                rep.images += 1;
            }
            continue;
        };
        let new_obj = match blank_image(doc, orig, &mut pdf, id, &u.quad, rects) {
            Ok(s) => s,
            Err(_) => black_pixel(),
        };
        let new_id = doc.add_object(Object::Stream(new_obj));
        *counter += 1;
        let name = format!("CedroRd{counter}");
        match u.cont {
            Cont::Page => super::newtext::add_resource(doc, page, "XObject", &name, Object::Reference(new_id))?,
            Cont::Form(fid) => add_form_resource(doc, page, fid, &name, new_id)?,
        }
        if let Some(op) = tp.ops.get_mut(&u.cont).and_then(|o| o.get_mut(u.op)) {
            if let Some(first) = op.operands.first_mut() {
                *first = Object::Name(name.into_bytes());
            }
        }
        rep.images += 1;
    }

    // 2. I caratteri.
    let mut remove: HashSet<(Cont, usize, usize)> = HashSet::new();
    for g in &tp.glyphs {
        if inside(rects, &g.rect()) {
            remove.insert((g.cont, g.op, g.gi));
        }
    }
    rep.glyphs = remove.len();
    if !remove.is_empty() {
        remove_glyphs(&mut tp, &remove);
    }
    write_back(doc, page, &tp, &original)?;

    // 3. Il riquadro nero.
    let mut ops = String::from("q 0 0 0 rg\n");
    for r in rects {
        let c = [geom.to_file(r.x, r.y), geom.to_file(r.right(), r.y), geom.to_file(r.right(), r.bottom()), geom.to_file(r.x, r.bottom())];
        ops.push_str(&format!("{} {} m {} {} l {} {} l {} {} l h f\n", fmt_num(c[0].0), fmt_num(c[0].1), fmt_num(c[1].0), fmt_num(c[1].1), fmt_num(c[2].0), fmt_num(c[2].1), fmt_num(c[3].0), fmt_num(c[3].1)));
    }
    ops.push_str("Q\n");
    annot::push_content(doc, page, &ops, wrapped)?;

    // 4. Commenti e campi sotto l'area, e la miniatura.
    rep.annots = remove_annots(doc, page, geom, rects);
    if let Ok(p) = doc.get_dictionary_mut(page) {
        p.remove(b"Thumb");
    }
    Ok(rep)
}

fn remove_annots(doc: &mut Document, page: ObjectId, geom: &Geom, rects: &[Rect]) -> usize {
    let files: Vec<[f32; 4]> = rects.iter().map(|r| geom.rect_to_file(r)).collect();
    let list: Vec<Object> = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Annots").ok()).cloned() {
        Some(Object::Array(a)) => a,
        Some(Object::Reference(r)) => match doc.get_object(r) {
            Ok(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let mut n = 0;
    for o in list {
        let Object::Reference(id) = o else { continue };
        let Some(d) = doc.get_dictionary(id).ok() else { continue };
        let Some(r) = d.get(b"Rect").ok().and_then(|r| r.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4) else { continue };
        let (x0, x1, y0, y1) = (r[0].min(r[2]), r[0].max(r[2]), r[1].min(r[3]), r[1].max(r[3]));
        let hit = files.iter().any(|f| x0 < f[2] && f[0] < x1 && y0 < f[3] && f[1] < y1);
        if hit {
            super::form::forget(doc, id);
            let _ = annot::delete(doc, page, id);
            n += 1;
        }
    }
    n
}

fn zero_inline(op: &mut lopdf::content::Operation) {
    for o in op.operands.iter_mut() {
        if let Object::String(b, _) = o {
            if b.len() > 16 {
                b.iter_mut().for_each(|x| *x = 0);
            }
        }
        if let Object::Stream(s) = o {
            let n = s.content.len();
            s.content = vec![0; n];
        }
    }
}

fn black_pixel() -> Stream {
    Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, vec![0u8])
}

/// Mette una risorsa tra quelle di un oggetto «Form» (una copia sua, anche se le prendeva dalla pagina).
fn add_form_resource(doc: &mut Document, page: ObjectId, form: ObjectId, name: &str, value: ObjectId) -> Result<(), String> {
    let mut res = match doc.get_object(form) {
        Ok(Object::Stream(s)) => s.dict.get(b"Resources").ok().and_then(|r| dict_of(doc, r)).cloned(),
        _ => None,
    }
    .unwrap_or_else(|| super::text::page_resources(doc, page));
    let mut sub = res.get(b"XObject").ok().and_then(|o| dict_of(doc, o)).cloned().unwrap_or_default();
    sub.set(name, Object::Reference(value));
    res.set("XObject", Object::Dictionary(sub));
    if let Ok(Object::Stream(s)) = doc.get_object_mut(form) {
        s.dict.set("Resources", Object::Dictionary(res));
        Ok(())
    } else {
        Err("oggetto del disegno non trovato".into())
    }
}

#[derive(Clone, Debug)]
enum Cs {
    Gray,
    Rgb,
    Cmyk,
    Indexed { base: Box<Cs>, lookup: Vec<u8> },
}

impl Cs {
    fn comps(&self) -> usize {
        match self {
            Cs::Gray | Cs::Indexed { .. } => 1,
            Cs::Rgb => 3,
            Cs::Cmyk => 4,
        }
    }
}

fn color_space(doc: &Document, o: &Object, depth: u8) -> Option<Cs> {
    if depth > 4 {
        return None;
    }
    let o = deref(doc, o)?;
    if let Some(n) = name_of(o) {
        return match n.as_str() {
            "DeviceGray" | "G" | "CalGray" => Some(Cs::Gray),
            "DeviceRGB" | "RGB" | "CalRGB" => Some(Cs::Rgb),
            "DeviceCMYK" | "CMYK" => Some(Cs::Cmyk),
            _ => None,
        };
    }
    let a = o.as_array().ok()?;
    let head = name_of(a.first()?)?;
    match head.as_str() {
        "CalGray" => Some(Cs::Gray),
        "CalRGB" => Some(Cs::Rgb),
        "ICCBased" => {
            let s = match deref(doc, a.get(1)?)? {
                Object::Stream(s) => s,
                _ => return None,
            };
            match s.dict.get(b"N").ok().and_then(|n| n.as_i64().ok())? {
                1 => Some(Cs::Gray),
                3 => Some(Cs::Rgb),
                4 => Some(Cs::Cmyk),
                _ => None,
            }
        }
        "Indexed" | "I" => {
            let base = color_space(doc, a.get(1)?, depth + 1)?;
            let lookup = match deref(doc, a.get(3)?)? {
                Object::String(b, _) => b.clone(),
                Object::Stream(s) => s.decompressed_content().ok()?,
                _ => return None,
            };
            Some(Cs::Indexed { base: Box::new(base), lookup })
        }
        _ => None,
    }
}

fn to_rgb(cs: &Cs, s: &[u8]) -> [u8; 3] {
    match cs {
        Cs::Gray => [s[0], s[0], s[0]],
        Cs::Rgb => [s[0], s[1], s[2]],
        Cs::Cmyk => super::text::cmyk(s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0, s[3] as f32 / 255.0),
        Cs::Indexed { base, lookup } => {
            let n = base.comps();
            let i = s[0] as usize * n;
            if i + n <= lookup.len() {
                to_rgb(base, &lookup[i..i + n])
            } else {
                [0, 0, 0]
            }
        }
    }
}

/// I campioni di un'immagine, uno per componente, portati a 8 bit (gli indici di una tavolozza restano com'erano).
fn unpack(data: &[u8], w: usize, h: usize, comps: usize, bpc: usize, indexed: bool) -> Vec<u8> {
    let row_bits = w * comps * bpc;
    let row_bytes = row_bits.div_ceil(8);
    let mut out = Vec::with_capacity(w * h * comps);
    let max = ((1u32 << bpc.min(16)) - 1) as f32;
    for y in 0..h {
        let row = data.get(y * row_bytes..).unwrap_or(&[]);
        for i in 0..w * comps {
            let v: u32 = match bpc {
                8 => row.get(i).copied().unwrap_or(0) as u32,
                16 => row.get(i * 2).copied().unwrap_or(0) as u32,
                _ => {
                    let bit = i * bpc;
                    let byte = row.get(bit / 8).copied().unwrap_or(0) as u32;
                    (byte >> (8 - bpc - bit % 8)) & ((1 << bpc) - 1)
                }
            };
            out.push(match bpc {
                8 => v as u8,
                16 => v as u8,
                _ if indexed => v as u8,
                _ => (v as f32 / max * 255.0).round() as u8,
            });
        }
    }
    out
}

fn is_image_filter(f: &str) -> bool {
    matches!(f, "DCTDecode" | "DCT" | "JPXDecode" | "JBIG2Decode" | "CCITTFaxDecode" | "CCF")
}

/// L'immagine con i pixel delle aree tolti (diventano neri).
fn blank_image(doc: &Document, orig: &[u8], pdf: &mut Option<hayro::hayro_syntax::Pdf>, id: ObjectId, quad: &[(f32, f32); 4], rects: &[Rect]) -> Result<Stream, String> {
    let stream = match doc.get_object(id) {
        Ok(Object::Stream(s)) => s,
        _ => return Err("immagine non trovata".into()),
    };
    let dict = &stream.dict;
    let get_i = |k: &[u8]| dict.get(k).ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_i64().ok());
    let (w, h) = (get_i(b"Width").ok_or("larghezza")? as usize, get_i(b"Height").ok_or("altezza")? as usize);
    if w == 0 || h == 0 || w * h > 400_000_000 {
        return Err("immagine fuori misura".into());
    }
    let stencil = dict.get(b"ImageMask").ok().and_then(|o| o.as_bool().ok()).unwrap_or(false);
    let mut bpc = get_i(b"BitsPerComponent").unwrap_or(if stencil { 1 } else { 8 }) as usize;
    let filters: Vec<String> = match dict.get(b"Filter").ok().and_then(|o| deref(doc, o)) {
        Some(Object::Array(a)) => a.iter().filter_map(name_of).collect(),
        Some(o) => name_of(o).into_iter().collect(),
        None => Vec::new(),
    };
    let mut cs = if stencil { Some(Cs::Gray) } else { dict.get(b"ColorSpace").ok().and_then(|c| color_space(doc, c, 0)) };
    let (mut w, mut h) = (w, h);
    let data: Vec<u8>;
    if filters.iter().any(|f| is_image_filter(f)) {
        if pdf.is_none() {
            *pdf = hayro::hayro_syntax::Pdf::new(orig.to_vec()).ok();
        }
        let p = pdf.as_ref().ok_or("file non riletto")?;
        let st: hayro::hayro_syntax::object::Stream<'_> = p.xref().get(hayro::hayro_syntax::object::ObjectIdentifier::new(id.0, id.1 as i32)).ok_or("immagine non riletta")?;
        let params = hayro::hayro_syntax::object::stream::ImageDecodeParams {
            is_indexed: matches!(cs, Some(Cs::Indexed { .. })),
            bpc: Some(bpc as u8),
            num_components: cs.as_ref().map(|c| c.comps() as u8),
            target_dimension: None,
            width: w as u32,
            height: h as u32,
        };
        let dec = st.decoded_image(&params).map_err(|_| "immagine non sciolta".to_string())?;
        if let Some(d) = &dec.image_data {
            w = d.width as usize;
            h = d.height as usize;
            bpc = d.bits_per_component as usize;
            if cs.is_none() {
                cs = match d.color_space {
                    Some(hayro::hayro_syntax::object::stream::ImageColorSpace::Gray) => Some(Cs::Gray),
                    Some(hayro::hayro_syntax::object::stream::ImageColorSpace::Rgb) => Some(Cs::Rgb),
                    Some(hayro::hayro_syntax::object::stream::ImageColorSpace::Cmyk) => Some(Cs::Cmyk),
                    _ => None,
                };
            }
        }
        data = dec.data.into_owned();
    } else {
        data = stream.decompressed_content().map_err(|e| e.to_string()).map(|d| if d.is_empty() { stream.content.clone() } else { d })?;
    }
    let cs = cs.ok_or("spazio colore non supportato")?;
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return Err("profondità non supportata".into());
    }
    let invert = dict.get(b"Decode").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).is_some_and(|d| d.len() >= 2 && d[0] > d[1]);
    // Quali pixel stanno dentro le aree: il quadrato dell'unità dell'immagine arriva alla pagina con i suoi 4 angoli.
    let (bl, br, tl) = (quad[0], quad[1], quad[3]);
    let (ux, uy) = (br.0 - bl.0, br.1 - bl.1);
    let (vx, vy) = (tl.0 - bl.0, tl.1 - bl.1);
    let det = ux * vy - uy * vx;
    if det.abs() < 1e-6 {
        return Err("immagine senza superficie".into());
    }
    let hit = |px: usize, py: usize| -> bool {
        // il centro del pixel → punto della pagina vista
        let (u, v) = ((px as f32 + 0.5) / w as f32, 1.0 - (py as f32 + 0.5) / h as f32);
        let (x, y) = (bl.0 + u * ux + v * vx, bl.1 + u * uy + v * vy);
        rects.iter().any(|r| r.inflate(1.0).contains(x, y))
    };
    if stencil {
        // La maschera resta una maschera: i punti nell'area si dipingono (0 se «0 1», 1 se «1 0»).
        let samples = unpack(&data, w, h, 1, bpc, true);
        let paint = if invert { 1u8 } else { 0u8 };
        let row_bytes = w.div_ceil(8);
        let mut out = vec![0u8; row_bytes * h];
        for y in 0..h {
            for x in 0..w {
                let s = if hit(x, y) { paint } else { (samples[y * w + x] != 0) as u8 };
                if s != 0 {
                    out[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        let mut d = dict.clone();
        d.remove(b"Filter");
        d.remove(b"DecodeParms");
        d.set("Width", w as i64);
        d.set("Height", h as i64);
        d.set("BitsPerComponent", 1);
        let mut s = Stream::new(d, out);
        let _ = s.compress();
        return Ok(s);
    }
    let comps = cs.comps();
    let indexed = matches!(cs, Cs::Indexed { .. });
    let samples = unpack(&data, w, h, comps, bpc, indexed);
    if samples.len() < w * h * comps {
        return Err("dati dell'immagine corti".into());
    }
    let gray = matches!(cs, Cs::Gray);
    let mut out = Vec::with_capacity(w * h * if gray { 1 } else { 3 });
    for y in 0..h {
        for x in 0..w {
            if hit(x, y) {
                out.extend_from_slice(if gray { &[0] } else { &[0, 0, 0] });
                continue;
            }
            let s = &samples[(y * w + x) * comps..(y * w + x + 1) * comps];
            let mut rgb = to_rgb(&cs, s);
            if invert && !indexed {
                rgb = [255 - rgb[0], 255 - rgb[1], 255 - rgb[2]];
            }
            if gray {
                out.push(rgb[0]);
            } else {
                out.extend_from_slice(&rgb);
            }
        }
    }
    let mut d = Dictionary::new();
    for (k, v) in dict.iter() {
        if matches!(k.as_slice(), b"SMask" | b"Mask" | b"Intent" | b"Name" | b"Metadata" | b"OC") {
            d.set(k.clone(), v.clone());
        }
    }
    d.set("Type", "XObject");
    d.set("Subtype", "Image");
    d.set("Width", w as i64);
    d.set("Height", h as i64);
    d.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
    d.set("BitsPerComponent", 8);
    let mut s = Stream::new(d, out);
    let _ = s.compress();
    Ok(s)
}
