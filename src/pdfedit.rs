//! Le pagine e il peso dei PDF. Ruotare e togliere pagine passa da `pdfpages::build` (si copiano solo le
//! pagine che restano); qui si girano, si timbrano con numeri e filigrana e si comprimono.
//! I timbri sono un piccolo strato di testo in Helvetica, che ogni lettore ha già: niente font da incorporare,
//! e gli accenti italiani ci sono (WinAnsi). La compressione riduce le immagini grandi, ricomprime i flussi,
//! toglie i doppioni e quello che non serve, e scrive il file con gli oggetti impacchettati.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use sha2::{Digest, Sha256};

use crate::engine::PageSel;

/// I numeri di pagina.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Numbers {
    /// Dove, da 0 a 8: in alto a sinistra, in alto al centro… in basso a destra.
    pub pos: u8,
    /// 0 «1», 1 «Pagina 1 di 8», 2 «1 / 8».
    pub format: u8,
    /// Il numero della prima pagina.
    pub start: u32,
    /// Il corpo del carattere, in punti.
    pub size: u32,
}

impl Default for Numbers {
    fn default() -> Self {
        Numbers { pos: 7, format: 1, start: 1, size: 11 }
    }
}

/// Un testo grande e chiaro sopra le pagine («BOZZA», «COPIA»).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Watermark {
    pub text: String,
    /// Opacità in percento.
    pub opacity: u32,
    /// In diagonale (dal basso a sinistra all'alto a destra) oppure orizzontale.
    pub diagonal: bool,
    /// Le pagine che la ricevono, per esempio `1-3,5`; vuoto = tutte.
    pub pages: String,
}

impl Default for Watermark {
    fn default() -> Self {
        Watermark { text: "BOZZA".into(), opacity: 22, diagonal: true, pages: String::new() }
    }
}

/// Cosa aggiungere alle pagine.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Stamp {
    pub numbers: Option<Numbers>,
    pub watermark: Option<Watermark>,
}

impl Stamp {
    pub fn is_empty(&self) -> bool {
        self.numbers.is_none() && self.watermark.as_ref().is_none_or(|w| w.text.trim().is_empty())
    }
}

/// Il testo del numero della pagina `n` (da 1) su `total`, nel formato scelto.
pub fn number_text(format: u8, n: u32, total: u32) -> String {
    match format {
        0 => n.to_string(),
        2 => format!("{n} / {total}"),
        _ => crate::i18n::tr("Pagina %n di %t").replace("%n", &n.to_string()).replace("%t", &total.to_string()),
    }
}

/// Gira le pagine (`rotations[i]` in gradi, multipli di 90, per la pagina i del PDF) e le timbra.
/// `bytes` è un PDF appena costruito da `pdfpages::build`.
pub fn finish(bytes: &[u8], rotations: &[i32], stamp: &Stamp) -> Result<Vec<u8>, String> {
    if rotations.iter().all(|r| r.rem_euclid(360) == 0) && stamp.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF non letto: {e}"))?;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for (i, &id) in pages.iter().enumerate() {
        let turn = rotations.get(i).copied().unwrap_or(0);
        if turn.rem_euclid(360) == 0 {
            continue;
        }
        let page = doc.get_dictionary_mut(id).map_err(|e| e.to_string())?;
        let old = page.get(b"Rotate").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0) as i32;
        page.set("Rotate", ((old + turn).rem_euclid(360) / 90 * 90) as i64);
    }
    if !stamp.is_empty() {
        stamp_pages(&mut doc, &pages, stamp)?;
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// La cornice di una pagina come la si vede: (x0, y0, larghezza, altezza) e rotazione in gradi.
fn page_frame(doc: &Document, id: ObjectId) -> ((f32, f32, f32, f32), i32) {
    let Ok(page) = doc.get_dictionary(id) else { return ((0.0, 0.0, 595.0, 842.0), 0) };
    let lookup = |key: &[u8]| -> Option<Object> {
        if let Ok(v) = page.get(key) {
            return Some(v.clone());
        }
        let mut node = page.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        for _ in 0..32 {
            let d = doc.get_dictionary(node?).ok()?;
            if let Ok(v) = d.get(key) {
                return Some(v.clone());
            }
            node = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        None
    };
    let rect = |o: Object| -> Option<(f32, f32, f32, f32)> {
        let o = match o {
            Object::Reference(r) => doc.get_object(r).ok()?.clone(),
            o => o,
        };
        let a = o.as_array().ok()?;
        let n: Vec<f32> = a.iter().filter_map(|v| v.as_float().ok().or_else(|| v.as_i64().ok().map(|i| i as f32))).collect();
        (n.len() == 4).then(|| {
            let (x0, x1) = (n[0].min(n[2]), n[0].max(n[2]));
            let (y0, y1) = (n[1].min(n[3]), n[1].max(n[3]));
            (x0, y0, x1 - x0, y1 - y0)
        })
    };
    let r = lookup(b"CropBox").and_then(rect).or_else(|| lookup(b"MediaBox").and_then(rect)).filter(|r| r.2 > 1.0 && r.3 > 1.0).unwrap_or((0.0, 0.0, 595.0, 842.0));
    let rot = lookup(b"Rotate").and_then(|o| o.as_i64().ok()).unwrap_or(0) as i32;
    (r, rot.rem_euclid(360) / 90 * 90)
}

/// La matrice che porta lo spazio della pagina come la si vede (origine in basso a sinistra) in quello del PDF.
fn display_matrix(r: (f32, f32, f32, f32), rot: i32) -> ([f32; 6], f32, f32) {
    let (x0, y0, w, h) = r;
    match rot {
        90 => ([0.0, 1.0, -1.0, 0.0, x0 + w, y0], h, w),
        180 => ([-1.0, 0.0, 0.0, -1.0, x0 + w, y0 + h], w, h),
        270 => ([0.0, -1.0, 1.0, 0.0, x0, y0 + h], h, w),
        _ => ([1.0, 0.0, 0.0, 1.0, x0, y0], w, h),
    }
}

/// Larghezze di Helvetica (millesimi del corpo) per i caratteri da 32 a 126; gli accentati come la lettera base.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // spazio … /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : … @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // A-Z
    278, 278, 278, 469, 556, 333, // [ … `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // a-z
    334, 260, 334, 584,
];

/// Quanto è largo il testo in Helvetica, in millesimi del corpo (la neretta è un poco più larga).
pub fn text_width(text: &str, bold: bool) -> f32 {
    let base = |c: char| -> char {
        match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'À' | 'Á' | 'Â' | 'Ä' => 'A',
            'È' | 'É' | 'Ê' | 'Ë' => 'E',
            'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
            'Ò' | 'Ó' | 'Ô' | 'Ö' => 'O',
            'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
            c => c,
        }
    };
    let w: f32 = text
        .chars()
        .map(|c| {
            let c = base(c) as u32;
            if (32..=126).contains(&c) {
                HELVETICA[(c - 32) as usize] as f32
            } else {
                556.0
            }
        })
        .sum();
    if bold {
        w * 1.07
    } else {
        w
    }
}

fn pdf_string(text: &str) -> String {
    let mut s = String::from("(");
    for b in crate::pdf::win_ansi(text) {
        match b {
            b'\\' | b'(' | b')' => {
                s.push('\\');
                s.push(b as char);
            }
            32..=126 => s.push(b as char),
            _ => s.push_str(&format!("\\{b:03o}")),
        }
    }
    s.push(')');
    s
}

fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Mette i numeri e la filigrana sulle pagine, sopra il contenuto (che resta chiuso tra q e Q).
fn stamp_pages(doc: &mut Document, pages: &[ObjectId], stamp: &Stamp) -> Result<(), String> {
    let total = pages.len() as u32;
    let regular = doc.add_object(lopdf::dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let bold = doc.add_object(lopdf::dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding" });
    let wm = stamp.watermark.as_ref().filter(|w| !w.text.trim().is_empty());
    let alpha = wm.map(|w| w.opacity.clamp(1, 100) as f32 / 100.0).unwrap_or(1.0);
    let gs = doc.add_object(lopdf::dictionary! { "Type" => "ExtGState", "ca" => alpha, "CA" => alpha });
    let wm_pages = match wm {
        Some(w) => PageSel::parse(&w.pages)?,
        None => None,
    };
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    for (i, &id) in pages.iter().enumerate() {
        let (frame, rot) = page_frame(doc, id);
        let (m, w, h) = display_matrix(frame, rot);
        let mut ops = format!("Q\nq {} {} {} {} {} {} cm\n", num(m[0]), num(m[1]), num(m[2]), num(m[3]), num(m[4]), num(m[5]));
        if let Some(wm) = wm.filter(|_| wm_pages.as_ref().is_none_or(|s| s.contains(i))) {
            let text = wm.text.trim();
            let units = text_width(text, true) / 1000.0;
            let (angle, room) = if wm.diagonal {
                ((h).atan2(w), (w * w + h * h).sqrt() * 0.62)
            } else {
                (0.0, w * 0.78)
            };
            let size = (room / units.max(0.1)).clamp(8.0, 220.0);
            let (s, c) = angle.sin_cos();
            ops.push_str(&format!(
                "q /CedroGS gs 0.70 0.15 0.12 rg {} {} {} {} {} {} cm BT /CedroW {} Tf {} {} Td {} Tj ET Q\n",
                num(c), num(s), num(-s), num(c), num(w / 2.0), num(h / 2.0),
                num(size), num(-units * size / 2.0), num(-size * 0.35), pdf_string(text)
            ));
        }
        if let Some(n) = &stamp.numbers {
            let text = number_text(n.format, n.start.saturating_add(i as u32), total.saturating_add(n.start).saturating_sub(1));
            let size = n.size.clamp(6, 48) as f32;
            let tw = text_width(&text, false) / 1000.0 * size;
            let margin = (w.min(h) * 0.05).clamp(18.0, 36.0);
            let x = match n.pos % 3 {
                0 => margin,
                1 => (w - tw) / 2.0,
                _ => w - margin - tw,
            };
            let y = match n.pos.min(8) / 3 {
                0 => h - margin - size * 0.72,
                1 => (h - size * 0.72) / 2.0,
                _ => margin,
            };
            ops.push_str(&format!("q 0.2 0.2 0.2 rg BT /CedroN {} Tf {} {} Td {} Tj ET Q\n", num(size), num(x), num(y), pdf_string(&text)));
        }
        ops.push_str("Q\n");
        let mine = doc.add_object(Stream::new(Dictionary::new(), ops.into_bytes()));
        // Il contenuto di prima resta tra «q» e il primo «Q» del nostro strato.
        let mut parts = vec![Object::Reference(open)];
        let old = doc.get_dictionary(id).map_err(|e| e.to_string())?.get(b"Contents").ok().cloned();
        match old {
            Some(Object::Reference(r)) => match doc.get_object(r) {
                Ok(Object::Array(a)) => parts.extend(a.iter().cloned()),
                _ => parts.push(Object::Reference(r)),
            },
            Some(Object::Array(a)) => parts.extend(a),
            _ => {}
        }
        parts.push(Object::Reference(mine));
        add_resources(doc, id, regular, bold, gs)?;
        doc.get_dictionary_mut(id).map_err(|e| e.to_string())?.set("Contents", parts);
    }
    Ok(())
}

/// Aggiunge alle risorse della pagina i due caratteri e lo stato grafico (anche quando sono condivise o ereditate).
fn add_resources(doc: &mut Document, page: ObjectId, regular: ObjectId, bold: ObjectId, gs: ObjectId) -> Result<(), String> {
    let (frame_res, _) = (doc.get_dictionary(page).ok().and_then(|p| p.get(b"Resources").ok().cloned()), ());
    // Una copia propria delle risorse per la pagina: quelle condivise o ereditate non si toccano.
    let mut res = match frame_res {
        Some(Object::Reference(r)) => doc.get_dictionary(r).cloned().unwrap_or_default(),
        Some(Object::Dictionary(d)) => d,
        _ => {
            let mut node = doc.get_dictionary(page).ok().and_then(|p| p.get(b"Parent").ok()).and_then(|p| p.as_reference().ok());
            let mut found = Dictionary::new();
            for _ in 0..32 {
                let Some(n) = node else { break };
                let Ok(d) = doc.get_dictionary(n) else { break };
                if let Ok(r) = d.get(b"Resources") {
                    found = match r {
                        Object::Reference(r) => doc.get_dictionary(*r).cloned().unwrap_or_default(),
                        Object::Dictionary(d) => d.clone(),
                        _ => Dictionary::new(),
                    };
                    break;
                }
                node = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
            }
            found
        }
    };
    let sub = |doc: &Document, res: &Dictionary, key: &[u8]| -> Dictionary {
        match res.get(key) {
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).cloned().unwrap_or_default(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        }
    };
    let mut fonts = sub(doc, &res, b"Font");
    fonts.set("CedroN", regular);
    fonts.set("CedroW", bold);
    let mut states = sub(doc, &res, b"ExtGState");
    states.set("CedroGS", gs);
    res.set("Font", fonts);
    res.set("ExtGState", states);
    doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Resources", res);
    Ok(())
}

/// Quanto comprimere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Squeeze {
    /// 0 Leggera (solo flussi e pulizia), 1 Equilibrata (immagini a 150 dpi), 2 Forte (100 dpi).
    pub level: u8,
    /// Toglie autore, programma, date e i metadati XMP.
    pub strip_meta: bool,
    /// Le pagine scansionate quasi solo bianche e nere diventano a un bit: nitide e leggere.
    pub bw_scans: bool,
}

impl Default for Squeeze {
    fn default() -> Self {
        Squeeze { level: 1, strip_meta: true, bw_scans: true }
    }
}

/// Il PDF compresso; il chiamante tiene l'originale se non viene più piccolo.
pub fn compress(mut doc: Document, s: &Squeeze) -> Result<Vec<u8>, String> {
    doc.trailer.remove(b"Encrypt");
    // Le risoluzioni si misurano sulla pagina più grande: un'immagine non serve più fitta di così.
    let side = doc
        .get_pages()
        .values()
        .map(|&id| {
            let (r, _) = page_frame(&doc, id);
            r.2.max(r.3)
        })
        .fold(0.0f32, f32::max)
        .max(200.0);
    let masks: HashSet<ObjectId> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            Object::Stream(st) => st.dict.get(b"SMask").ok().and_then(|m| m.as_reference().ok()),
            _ => None,
        })
        .collect();
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let Some(Object::Stream(st)) = doc.objects.get(&id) else { continue };
        let ty = st.dict.get(b"Type").and_then(|t| t.as_name()).unwrap_or(b"");
        if ty == b"XRef" || ty == b"ObjStm" {
            continue;
        }
        let is_image = st.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Image".as_slice());
        let new = if is_image {
            squeeze_image(&doc, st, s, side, masks.contains(&id))
        } else if ty == b"Metadata" {
            None
        } else {
            reflate(st)
        };
        if let Some(n) = new {
            doc.objects.insert(id, Object::Stream(n));
        }
    }
    // Miniature delle pagine e, se chiesto, i metadati.
    for id in doc.get_pages().into_values().collect::<Vec<_>>() {
        if let Ok(p) = doc.get_dictionary_mut(id) {
            p.remove(b"Thumb");
            if s.strip_meta {
                p.remove(b"PieceInfo");
            }
        }
    }
    if s.strip_meta {
        doc.trailer.remove(b"Info");
        if let Ok(cat) = doc.catalog_mut() {
            cat.remove(b"Metadata");
            cat.remove(b"PieceInfo");
        }
    }
    dedupe(&mut doc);
    doc.prune_objects();
    Ok(save_packed(&doc))
}

/// Un flusso ricompresso al meglio (Flate senza predittori, o senza filtro), se viene più piccolo.
fn reflate(st: &Stream) -> Option<Stream> {
    let plain = match st.dict.get(b"Filter") {
        Err(_) => st.content.clone(),
        Ok(f) => {
            let list: Vec<&[u8]> = match f {
                Object::Name(n) => vec![n.as_slice()],
                Object::Array(a) => a.iter().map(|x| x.as_name()).collect::<Result<_, _>>().ok()?,
                _ => return None,
            };
            // Solo i filtri senza perdite che lopdf sa togliere, e senza predittori.
            if list.is_empty() || !list.iter().all(|f| matches!(*f, b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode")) || st.dict.get(b"DecodeParms").is_ok() {
                return None;
            }
            st.decompressed_content().ok()?
        }
    };
    let packed = deflate(&plain);
    if packed.len() + 16 >= st.content.len() {
        return None;
    }
    let mut dict = st.dict.clone();
    dict.set("Filter", "FlateDecode");
    dict.remove(b"DecodeParms");
    let mut n = Stream::new(dict, packed);
    n.allows_compression = false;
    Some(n)
}

/// I filtri di un flusso, in ordine di decodifica.
fn filters_of(d: &Dictionary) -> Option<Vec<Vec<u8>>> {
    match d.get(b"Filter") {
        Err(_) => Some(Vec::new()),
        Ok(Object::Name(f)) => Some(vec![f.clone()]),
        Ok(Object::Array(a)) => a.iter().map(|x| x.as_name().map(|n| n.to_vec())).collect::<Result<_, _>>().ok(),
        _ => None,
    }
}

/// Toglie i filtri senza perdite che precedono il JPEG (per esempio ASCII85 davanti a DCT).
fn strip_prefix_filters(st: &Stream, prefix: &[Vec<u8>]) -> Option<Vec<u8>> {
    if prefix.is_empty() {
        return Some(st.content.clone());
    }
    let mut dict = Dictionary::new();
    dict.set("Filter", prefix.iter().map(|f| Object::Name(f.clone())).collect::<Vec<_>>());
    Stream::new(dict, st.content.clone()).decompressed_content().ok()
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::best());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// I canali dello spazio colore di un'immagine (1 grigio, 3 colore), se è uno di quelli che si sanno rifare.
fn channels(doc: &Document, cs: &Object) -> Option<u8> {
    let cs = match cs {
        Object::Reference(r) => doc.get_object(*r).ok()?,
        o => o,
    };
    match cs {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Array(a) if a.len() == 2 && a[0].as_name().ok() == Some(b"ICCBased".as_slice()) => {
            let st = match &a[1] {
                Object::Reference(r) => doc.get_object(*r).ok()?.as_stream().ok()?,
                _ => return None,
            };
            match st.dict.get(b"N").ok()?.as_i64().ok()? {
                1 => Some(1),
                3 => Some(3),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Un'immagine rifatta più leggera: più piccola se è più fitta del necessario, in JPEG, o a un bit se è
/// una scansione in bianco e nero. `None` se non si sa rifare o non guadagna.
fn squeeze_image(doc: &Document, st: &Stream, s: &Squeeze, side_pt: f32, is_mask: bool) -> Option<Stream> {
    let d = &st.dict;
    if s.level == 0 && !s.bw_scans {
        return reflate(st);
    }
    let get = |k: &[u8]| d.get(k).ok().and_then(|o| o.as_i64().ok());
    if d.get(b"ImageMask").and_then(|o| o.as_bool()).unwrap_or(false) || d.get(b"Decode").is_ok() || d.get(b"Matte").is_ok() || d.get(b"Mask").is_ok() {
        return reflate(st);
    }
    let (w, h) = (get(b"Width")? as u32, get(b"Height")? as u32);
    if get(b"BitsPerComponent")? != 8 || w == 0 || h == 0 || w as u64 * h as u64 > 120_000_000 {
        return reflate(st);
    }
    let n = if is_mask { 1 } else { channels(doc, d.get(b"ColorSpace").ok()?)? };
    let filters = filters_of(d)?;
    if d.get(b"DecodeParms").is_ok() && filters.last().map(|f| f.as_slice()) != Some(b"DCTDecode") {
        return reflate(st);
    }
    let img: image::DynamicImage = match filters.last().map(|f| f.as_slice()) {
        Some(b"DCTDecode") => {
            let data = strip_prefix_filters(st, &filters[..filters.len() - 1])?;
            let i = image::load_from_memory_with_format(&data, image::ImageFormat::Jpeg).ok()?;
            if n == 1 {
                image::DynamicImage::ImageLuma8(i.to_luma8())
            } else {
                if i.color().channel_count() < 3 {
                    return None;
                }
                image::DynamicImage::ImageRgb8(i.to_rgb8())
            }
        }
        last if last.is_none() || filters.iter().all(|f| matches!(f.as_slice(), b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode")) => {
            let raw = if filters.is_empty() { st.content.clone() } else { st.decompressed_content().ok()? };
            if raw.len() < (w * h * n as u32) as usize {
                return None;
            }
            let raw = raw[..(w * h * n as u32) as usize].to_vec();
            if n == 1 {
                image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, raw)?)
            } else {
                image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, raw)?)
            }
        }
        _ => return None,
    };
    let fit = |dpi: f32| -> (u32, u32) {
        let max = (side_pt / 72.0 * dpi).round().max(64.0) as u32;
        if w.max(h) <= max + max / 10 {
            (w, h)
        } else {
            let k = max as f32 / w.max(h) as f32;
            (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1))
        }
    };
    let resize = |img: &image::DynamicImage, (nw, nh): (u32, u32)| -> image::DynamicImage {
        if (nw, nh) == (w, h) {
            img.clone()
        } else {
            img.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
        }
    };
    let mut dict = d.clone();
    dict.remove(b"DecodeParms");
    let encoded: Vec<u8>;
    if !is_mask && s.bw_scans && is_bilevel(&img) {
        // A un bit serve più fitta: 300 dpi tengono le lettere nitide.
        let small = resize(&img, fit(300.0)).to_luma8();
        let (bw, bh) = small.dimensions();
        let row = bw.div_ceil(8) as usize;
        let mut bits = vec![0u8; row * bh as usize];
        for (x, y, p) in small.enumerate_pixels() {
            if p.0[0] >= 128 {
                bits[y as usize * row + x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
        encoded = deflate(&bits);
        dict.set("Width", bw as i64);
        dict.set("Height", bh as i64);
        dict.set("BitsPerComponent", 1);
        dict.set("ColorSpace", "DeviceGray");
        dict.set("Filter", "FlateDecode");
    } else if s.level == 0 {
        return reflate(st);
    } else {
        let (dpi, quality) = if s.level >= 2 { (100.0, 55) } else { (150.0, 75) };
        let small = resize(&img, fit(dpi));
        let (sw, sh) = (small.width(), small.height());
        if is_mask {
            // La trasparenza resta senza perdite: il JPEG la sporcherebbe ai bordi.
            encoded = deflate(small.to_luma8().as_raw());
            dict.set("Filter", "FlateDecode");
        } else {
            let mut out = Vec::new();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
            let ok = if n == 1 { enc.encode_image(&image::DynamicImage::ImageLuma8(small.to_luma8())) } else { enc.encode_image(&image::DynamicImage::ImageRgb8(small.to_rgb8())) };
            ok.ok()?;
            encoded = out;
            dict.set("Filter", "DCTDecode");
        }
        dict.set("Width", sw as i64);
        dict.set("Height", sh as i64);
        dict.set("BitsPerComponent", 8);
    }
    // Rifare un JPEG con perdite vale solo se si guadagna davvero (almeno un decimo).
    if encoded.len() + 64 >= st.content.len() || encoded.len() as f32 > st.content.len() as f32 * 0.9 {
        return reflate(st);
    }
    let mut out = Stream::new(dict, encoded);
    out.allows_compression = false;
    Some(out)
}

/// Vero se l'immagine è una scansione di sole cose bianche e nere (testo, moduli): pochissimi grigi e niente colore.
fn is_bilevel(img: &image::DynamicImage) -> bool {
    // Si guardano i pixel veri a passo fisso: rimpicciolendo, le righe sottili diventerebbero grigie.
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let step = ((w as u64 * h as u64 / 250_000) as f64).sqrt().max(1.0) as u32;
    let (mut total, mut mid, mut colored) = (0u32, 0u32, 0u32);
    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            let [r, g, b] = rgb.get_pixel(x, y).0;
            let l = (r as u32 * 3 + g as u32 * 6 + b as u32) / 10;
            total += 1;
            if (56..200).contains(&l) {
                mid += 1;
            }
            if r.abs_diff(g).max(g.abs_diff(b)).max(r.abs_diff(b)) > 40 {
                colored += 1;
            }
        }
    }
    let total = total.max(1) as f32;
    mid as f32 / total < 0.04 && (colored as f32 / total) < 0.01
}

/// I flussi uguali (lo stesso logo su ogni pagina, lo stesso carattere più volte) restano uno solo.
fn dedupe(doc: &mut Document) {
    let mut seen: HashMap<Vec<u8>, ObjectId> = HashMap::new();
    let mut same: HashMap<ObjectId, ObjectId> = HashMap::new();
    for (&id, obj) in &doc.objects {
        let Object::Stream(st) = obj else { continue };
        let mut dict = st.dict.clone();
        dict.remove(b"Length");
        let mut h = Sha256::new();
        let mut head = Vec::new();
        write_object(&mut head, &Object::Dictionary(dict));
        h.update(&head);
        h.update(b"\0");
        h.update(&st.content);
        let key = h.finalize().to_vec();
        match seen.get(&key) {
            Some(&first) => {
                same.insert(id, first);
            }
            None => {
                seen.insert(key, id);
            }
        }
    }
    if same.is_empty() {
        return;
    }
    fn swap(o: &mut Object, same: &HashMap<ObjectId, ObjectId>) {
        match o {
            Object::Reference(r) => {
                if let Some(n) = same.get(r) {
                    *r = *n;
                }
            }
            Object::Array(a) => a.iter_mut().for_each(|x| swap(x, same)),
            Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| swap(x, same)),
            Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| swap(x, same)),
            _ => {}
        }
    }
    for o in doc.objects.values_mut() {
        swap(o, &same);
    }
    for (_, v) in doc.trailer.iter_mut() {
        swap(v, &same);
    }
    for id in same.keys() {
        doc.objects.remove(id);
    }
}

/// Scrive un oggetto PDF (non un flusso) in forma compatta.
fn write_object(out: &mut Vec<u8>, o: &Object) {
    match o {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Object::Real(r) => {
            let v = if r.is_finite() { *r } else { 0.0 };
            if v.fract() == 0.0 && v.abs() < 1e9 {
                out.extend_from_slice((v as i64).to_string().as_bytes());
            } else {
                out.extend_from_slice(num_long(v).as_bytes());
            }
        }
        Object::Name(n) => {
            out.push(b'/');
            for &b in n {
                if (0x21..=0x7E).contains(&b) && !b"()<>[]{}/%#".contains(&b) {
                    out.push(b);
                } else {
                    out.extend_from_slice(format!("#{b:02X}").as_bytes());
                }
            }
        }
        Object::String(s, StringFormat::Hexadecimal) => {
            out.push(b'<');
            for b in s {
                out.extend_from_slice(format!("{b:02X}").as_bytes());
            }
            out.push(b'>');
        }
        Object::String(s, StringFormat::Literal) => {
            out.push(b'(');
            for &b in s {
                match b {
                    b'\\' | b'(' | b')' => {
                        out.push(b'\\');
                        out.push(b);
                    }
                    b'\r' => out.extend_from_slice(b"\\r"),
                    _ => out.push(b),
                }
            }
            out.push(b')');
        }
        Object::Array(a) => {
            out.push(b'[');
            for (k, x) in a.iter().enumerate() {
                if k > 0 {
                    out.push(b' ');
                }
                write_object(out, x);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => write_dict(out, d),
        Object::Stream(s) => write_dict(out, &s.dict),
        Object::Reference((id, g)) => out.extend_from_slice(format!("{id} {g} R").as_bytes()),
    }
}

fn num_long(v: f32) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

fn write_dict(out: &mut Vec<u8>, d: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (k, v) in d.iter() {
        write_object(out, &Object::Name(k.clone()));
        out.push(b' ');
        write_object(out, v);
    }
    out.extend_from_slice(b">>");
}

/// Salva con gli oggetti piccoli impacchettati in flussi compressi e l'indice in un flusso (PDF 1.5).
pub fn save_packed(doc: &Document) -> Vec<u8> {
    let version = if doc.version.as_str() < "1.5" { "1.5".to_string() } else { doc.version.clone() };
    let mut out = format!("%PDF-{version}\n%\u{e2}\u{e3}\u{cf}\u{d3}\n").into_bytes();
    // Il segno binario va scritto come byte, non come UTF-8.
    let mark = out.len() - "\u{e2}\u{e3}\u{cf}\u{d3}\n".len();
    out.truncate(mark);
    out.extend_from_slice(&[b'%', 0xE2, 0xE3, 0xCF, 0xD3, b'\n']);
    let mut max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);
    // (tipo, campo 2, campo 3): 1 = (offset, generazione), 2 = (flusso, indice).
    let mut entries: BTreeMap<u32, (u8, u64, u32)> = BTreeMap::new();
    let mut packable: Vec<u32> = Vec::new();
    for (&(id, gen), obj) in &doc.objects {
        match obj {
            Object::Stream(st) => {
                let ty = st.dict.get(b"Type").and_then(|t| t.as_name()).unwrap_or(b"");
                if ty == b"XRef" || ty == b"ObjStm" {
                    continue;
                }
                entries.insert(id, (1, out.len() as u64, gen as u32));
                out.extend_from_slice(format!("{id} {gen} obj\n").as_bytes());
                let mut dict = st.dict.clone();
                dict.set("Length", st.content.len() as i64);
                write_dict(&mut out, &dict);
                out.extend_from_slice(b"\nstream\n");
                out.extend_from_slice(&st.content);
                out.extend_from_slice(b"\nendstream\nendobj\n");
            }
            _ if gen == 0 => packable.push(id),
            _ => {
                entries.insert(id, (1, out.len() as u64, gen as u32));
                out.extend_from_slice(format!("{id} {gen} obj\n").as_bytes());
                write_object(&mut out, obj);
                out.extend_from_slice(b"\nendobj\n");
            }
        }
    }
    for chunk in packable.chunks(200) {
        max_id += 1;
        let stm = max_id;
        let mut head = Vec::new();
        let mut body = Vec::new();
        for (k, id) in chunk.iter().enumerate() {
            head.extend_from_slice(format!("{id} {} ", body.len()).as_bytes());
            write_object(&mut body, &doc.objects[&(*id, 0)]);
            body.push(b'\n');
            entries.insert(*id, (2, stm as u64, k as u32));
        }
        let first = head.len();
        head.extend_from_slice(&body);
        let data = deflate(&head);
        entries.insert(stm, (1, out.len() as u64, 0));
        out.extend_from_slice(format!("{stm} 0 obj\n<</Type/ObjStm/N {}/First {first}/Filter/FlateDecode/Length {}>>\nstream\n", chunk.len(), data.len()).as_bytes());
        out.extend_from_slice(&data);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }
    max_id += 1;
    let xref_id = max_id;
    let xref_at = out.len() as u64;
    entries.insert(xref_id, (1, xref_at, 0));
    let size = max_id + 1;
    let mut rows = Vec::with_capacity(size as usize * 7);
    for id in 0..size {
        let (t, a, b) = entries.get(&id).copied().unwrap_or(if id == 0 { (0, 0, 65535) } else { (0, 0, 0) });
        rows.push(t);
        rows.extend_from_slice(&(a as u32).to_be_bytes());
        rows.extend_from_slice(&(b.min(65535) as u16).to_be_bytes());
    }
    let data = deflate(&rows);
    let mut dict = Dictionary::new();
    dict.set("Type", "XRef");
    dict.set("Size", size as i64);
    dict.set("W", vec![1.into(), 4.into(), 2.into()]);
    dict.set("Filter", "FlateDecode");
    dict.set("Length", data.len() as i64);
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(v) = doc.trailer.get(key) {
            dict.set(key.to_vec(), v.clone());
        }
    }
    out.extend_from_slice(format!("{xref_id} 0 obj\n").as_bytes());
    write_dict(&mut out, &dict);
    out.extend_from_slice(b"\nstream\n");
    out.extend_from_slice(&data);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pages: usize) -> Vec<u8> {
        let blocks = vec![crate::pdfdoc::DocBlock { text: "Documento di prova, con accenti: è à ù.".into(), heading: 0, image: None }];
        let o = crate::pdfdoc::PdfOptions { size: crate::engine::PageSize::A4, embed: false, bookmarks: false, pdfa: false, title: "prova".into() };
        let mut bytes = crate::pdfdoc::build(&blocks, &o).unwrap();
        // Una pagina per blocco non è garantita: si uniscono più copie per averne quante servono.
        let doc = Document::load_mem(&bytes).unwrap();
        if doc.get_pages().len() < pages {
            let parts: Vec<crate::pdfpages::Part> = (0..pages).map(|_| crate::pdfpages::Part { doc: &doc, pages: vec![1], title: "x".into() }).collect();
            bytes = crate::pdfpages::build(&parts, &crate::pdfpages::Settings { file_bookmarks: false, ..Default::default() }).unwrap();
        }
        bytes
    }

    #[test]
    fn rotation_adds_up_and_stays_a_multiple_of_ninety() {
        let pdf = sample(3);
        let out = finish(&pdf, &[90, 0, -90], &Stamp::default()).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let rot: Vec<i64> = doc.get_pages().values().map(|&id| page_frame(&doc, id).1 as i64).collect();
        assert_eq!(rot, vec![90, 0, 270]);
        let again = finish(&out, &[270, 0, 90], &Stamp::default()).unwrap();
        let doc = Document::load_mem(&again).unwrap();
        let rot: Vec<i32> = doc.get_pages().values().map(|&id| page_frame(&doc, id).1).collect();
        assert_eq!(rot, vec![0, 0, 0]);
    }

    #[test]
    fn numbers_and_watermark_are_written_on_every_page() {
        let pdf = sample(3);
        let stamp = Stamp { numbers: Some(Numbers::default()), watermark: Some(Watermark { pages: "2".into(), ..Default::default() }) };
        let out = finish(&pdf, &[0, 90, 0], &stamp).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        for (i, id) in pages.iter().enumerate() {
            let text = String::from_utf8_lossy(&doc.get_page_content(*id).unwrap()).to_string();
            assert!(text.contains(&format!("(Pagina {} di 3)", i + 1)), "pagina {i}: {text}");
            assert_eq!(text.contains("(BOZZA)"), i == 1, "filigrana solo sulla 2");
            assert!(doc.get_page_fonts(*id).unwrap().contains_key(b"CedroN".as_slice()));
        }
        // Il contenuto di prima resta chiuso tra q e Q: il nostro strato parte pulito.
        let first = String::from_utf8_lossy(&doc.get_page_content(pages[0]).unwrap()).to_string();
        assert!(first.starts_with("q\n"));
    }

    #[test]
    fn number_formats() {
        assert_eq!(number_text(0, 3, 9), "3");
        assert_eq!(number_text(2, 3, 9), "3 / 9");
        assert_eq!(number_text(1, 3, 9), "Pagina 3 di 9");
        assert!(text_width("Pagina 1 di 8", false) > text_width("1", false));
    }

    #[test]
    fn display_matrix_turns_with_the_page() {
        let (m, w, h) = display_matrix((0.0, 0.0, 600.0, 800.0), 90);
        assert_eq!((w, h), (800.0, 600.0));
        // L'angolo in basso a sinistra visto va all'angolo in basso a destra del PDF.
        assert_eq!((m[4], m[5]), (600.0, 0.0));
    }

    fn photo_pdf(w: u32, h: u32) -> Vec<u8> {
        // Una pagina A4 con una foto rumorosa grande, salvata senza compressione.
        let mut raw = Vec::with_capacity((w * h * 3) as usize);
        let mut seed = 7u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let n = (seed >> 24) as u8 / 8;
                raw.extend_from_slice(&[(x * 255 / w) as u8 ^ n, (y * 255 / h) as u8, 120u8.wrapping_add(n)]);
            }
        }
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let img = doc.add_object(Stream::new(lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 }, raw));
        let content = doc.add_object(Stream::new(Dictionary::new(), b"q 595 0 0 842 0 0 cm /Im0 Do Q".to_vec()));
        let page = doc.add_object(lopdf::dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "Contents" => content, "Resources" => lopdf::dictionary! { "XObject" => lopdf::dictionary! { "Im0" => img } } });
        doc.objects.insert(pages_id, Object::Dictionary(lopdf::dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let cat = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", cat);
        let info = doc.add_object(lopdf::dictionary! { "Producer" => Object::string_literal("prova") });
        doc.trailer.set("Info", info);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn compression_shrinks_big_photos_and_keeps_a_readable_pdf() {
        let pdf = photo_pdf(2400, 3400);
        let mut last = pdf.len();
        for level in 0..3u8 {
            let doc = Document::load_mem(&pdf).unwrap();
            let out = compress(doc, &Squeeze { level, strip_meta: true, bw_scans: true }).unwrap();
            let back = Document::load_mem(&out).expect("il PDF compresso si rilegge");
            assert_eq!(back.get_pages().len(), 1);
            assert!(back.trailer.get(b"Info").is_err(), "metadati tolti");
            assert!(out.len() < last, "livello {level}: {} non è sotto {last}", out.len());
            last = out.len();
            if level == 2 {
                let page = *back.get_pages().values().next().unwrap();
                let imgs = back.get_page_images(page).unwrap();
                // 100 dpi sul lato di 842 punti: circa 1170 pixel.
                assert!(imgs[0].height <= 1200, "{}", imgs[0].height);
            }
        }
    }

    #[test]
    fn bilevel_scans_become_one_bit() {
        let (w, h) = (1700u32, 2400u32);
        let mut raw = vec![255u8; (w * h) as usize];
        for y in (200..2200).step_by(40) {
            for x in 150..1500 {
                for t in 0..6 {
                    raw[((y + t) * w + x) as usize] = 0;
                }
            }
        }
        let mut doc = Document::load_mem(&photo_pdf(10, 10)).unwrap();
        let id = doc.objects.iter().find(|(_, o)| matches!(o, Object::Stream(s) if s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Image".as_slice()))).map(|(k, _)| *k).unwrap();
        doc.objects.insert(id, Object::Stream(Stream::new(lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, raw)));
        let out = compress(doc, &Squeeze { level: 1, strip_meta: false, bw_scans: true }).unwrap();
        let back = Document::load_mem(&out).unwrap();
        let st = back.objects.values().find_map(|o| match o {
            Object::Stream(s) if s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Image".as_slice()) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(st.unwrap().dict.get(b"BitsPerComponent").unwrap().as_i64().unwrap(), 1);
        assert!(out.len() < 60_000, "{}", out.len());
    }

    #[test]
    fn packed_save_round_trips_strings_and_names() {
        let mut doc = Document::load_mem(&sample(2)).unwrap();
        let id = doc.add_object(lopdf::dictionary! { "A b" => Object::string_literal("(tonde) \\ e à"), "H" => Object::String(vec![0, 255], StringFormat::Hexadecimal), "R" => 0.25 });
        doc.trailer.set("Extra", id);
        let out = save_packed(&doc);
        let back = Document::load_mem(&out).unwrap();
        assert_eq!(back.get_pages().len(), 2);
        let d = back.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"A b").unwrap().as_str().unwrap(), "(tonde) \\ e à".as_bytes());
        assert_eq!(d.get(b"H").unwrap().as_str().unwrap(), &[0, 255]);
    }
}

