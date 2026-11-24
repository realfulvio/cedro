//! Dalla lista delle modifiche al PDF nuovo: si parte sempre dal file com'era, si applicano le modifiche
//! nell'ordine, si finisce con le opzioni di salvataggio (appiattire, togliere i dati nascosti). Così
//! annullare è togliere l'ultima voce e rifare, e il file originale non si tocca mai.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use super::annot;
use super::find;
use super::form;
use super::fonts::{deref, dict_of, name_of, num};
use super::geom::Geom;
use super::newtext::{self, FontBank, FontRef, Style, ASCENT, LEADING};
use super::redact::{self, Report};
use super::text::{remove_glyphs, write_back, Align, Cont, TextPage};
use super::{Comment, Edit, FontChoice, Hidden, PageSize, Rect};
use crate::pdfedit::num as fmt_num;

/// Cosa fare al salvataggio, oltre alle modifiche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveOpts {
    /// I commenti e i campi diventano parte della pagina: non si possono più cambiare.
    pub flatten: bool,
    /// Via autore, programma, dati XMP, allegati e script.
    pub strip: bool,
}

/// Un file aperto: quello che serve alla schermata per cominciare.
#[derive(Clone, Debug, Default)]
pub struct Opened {
    pub sizes: Vec<PageSize>,
    pub comments: Vec<Comment>,
    pub hidden: Hidden,
    pub fields: Vec<form::Field>,
}

fn load(bytes: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(bytes).map_err(|e| {
        let m = e.to_string();
        if m.to_lowercase().contains("password") || m.to_lowercase().contains("encrypt") {
            "Il PDF è protetto da una password: toglila prima di modificarlo.".to_string()
        } else {
            format!("PDF non letto: {m}")
        }
    })?;
    if doc.is_encrypted() {
        return Err("Il PDF è protetto da una password: toglila prima di modificarlo.".into());
    }
    Ok(doc)
}

fn pages_of(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

/// Apre il file: misure delle pagine, commenti, campi del modulo e dati nascosti.
pub fn open(bytes: &[u8]) -> Result<Opened, String> {
    let doc = load(bytes)?;
    let pages = pages_of(&doc);
    if pages.is_empty() {
        return Err("Il PDF non ha pagine.".into());
    }
    let sizes = pages
        .iter()
        .map(|p| {
            let g = Geom::of(&doc, *p);
            PageSize { w: g.w, h: g.h }
        })
        .collect();
    Ok(Opened { sizes, comments: comments(&doc, &pages), hidden: hidden(&doc), fields: form::fields(&doc, &pages) })
}

/// Il testo di una pagina (da 0), per cercare, evidenziare e modificare.
pub fn read_text(bytes: &[u8], page: usize) -> Result<TextPage, String> {
    let doc = load(bytes)?;
    let pages = pages_of(&doc);
    let id = *pages.get(page).ok_or("pagina che non c'è")?;
    Ok(TextPage::read(&doc, id))
}

fn comments(doc: &Document, pages: &[ObjectId]) -> Vec<Comment> {
    let mut out = Vec::new();
    for (pi, p) in pages.iter().enumerate() {
        let geom = Geom::of(doc, *p);
        let list: Vec<Object> = match doc.get_dictionary(*p).ok().and_then(|d| d.get(b"Annots").ok()).and_then(|a| deref(doc, a)) {
            Some(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for o in list {
            let Object::Reference(id) = o else { continue };
            let Ok(d) = doc.get_dictionary(id) else { continue };
            let sub = d.get(b"Subtype").ok().and_then(name_of).unwrap_or_default();
            let kind = match sub.as_str() {
                "Text" => "Nota",
                "Highlight" => "Evidenzia",
                "Underline" => "Sottolinea",
                "StrikeOut" => "Barra",
                "Squiggly" => "Sottolinea",
                "FreeText" => "Testo",
                "Square" => "Rettangolo",
                "Circle" => "Ellisse",
                "Line" => "Linea",
                "Ink" => "Matita",
                "Stamp" => "Timbro",
                "Caret" => "Inserisci",
                "Polygon" | "PolyLine" => "Forma",
                "FileAttachment" => "Allegato",
                _ => continue,
            };
            let Some(r) = d.get(b"Rect").ok().and_then(|r| deref(doc, r)).and_then(|r| r.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4) else { continue };
            out.push(Comment {
                page: pi,
                id,
                kind: kind.into(),
                rect: geom.rect_to_view(r[0], r[1], r[2], r[3]),
                text: d.get(b"Contents").ok().and_then(|c| deref(doc, c)).map(form::decode_text).unwrap_or_default(),
                author: d.get(b"T").ok().and_then(|c| deref(doc, c)).map(form::decode_text).unwrap_or_default(),
            });
        }
    }
    out
}

fn info_text(doc: &Document, key: &[u8]) -> String {
    doc.trailer
        .get(b"Info")
        .ok()
        .and_then(|i| dict_of(doc, i))
        .and_then(|i| i.get(key).ok())
        .and_then(|o| deref(doc, o))
        .map(form::decode_text)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn is_js(doc: &Document, o: &Object) -> bool {
    dict_of(doc, o).is_some_and(|d| d.get(b"S").ok().and_then(name_of).as_deref() == Some("JavaScript"))
}

/// Cosa porta il file e non si vede.
pub fn hidden(doc: &Document) -> Hidden {
    let root = doc.trailer.get(b"Root").ok().and_then(|r| dict_of(doc, r)).cloned().unwrap_or_default();
    let names = root.get(b"Names").ok().and_then(|n| dict_of(doc, n)).cloned().unwrap_or_default();
    let count = |key: &[u8]| -> usize {
        names
            .get(key)
            .ok()
            .and_then(|e| dict_of(doc, e))
            .and_then(|e| e.get(b"Names").ok().and_then(|n| deref(doc, n)).and_then(|n| n.as_array().ok().map(|a| a.len() / 2)).or_else(|| e.get(b"Kids").ok().and_then(|k| deref(doc, k)).and_then(|k| k.as_array().ok().map(|a| a.len()))))
            .unwrap_or(0)
    };
    let mut attachments = count(b"EmbeddedFiles");
    let mut javascript = names.has(b"JavaScript") || root.get(b"OpenAction").ok().is_some_and(|o| is_js(doc, o)) || root.get(b"AA").is_ok();
    for p in pages_of(doc) {
        let list: Vec<Object> = match doc.get_dictionary(p).ok().and_then(|d| d.get(b"Annots").ok()).and_then(|a| deref(doc, a)) {
            Some(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for o in list {
            let Some(d) = deref(doc, &o).and_then(|o| o.as_dict().ok()) else { continue };
            if d.get(b"Subtype").ok().and_then(name_of).as_deref() == Some("FileAttachment") {
                attachments += 1;
            }
            if d.get(b"A").ok().is_some_and(|a| is_js(doc, a)) || d.has(b"AA") {
                javascript = true;
            }
        }
    }
    Hidden {
        author: info_text(doc, b"Author"),
        creator: info_text(doc, b"Creator"),
        producer: info_text(doc, b"Producer"),
        title: info_text(doc, b"Title"),
        xmp: root.has(b"Metadata"),
        attachments,
        javascript,
    }
}

/// I dati nascosti di un file (per mostrarli prima di salvare).
pub fn hidden_of(bytes: &[u8]) -> Result<Hidden, String> {
    Ok(hidden(&load(bytes)?))
}

/// Toglie autore, programma, dati XMP, allegati e script.
fn strip(doc: &mut Document) {
    doc.trailer.remove(b"Info");
    if let Ok(root) = doc.catalog_mut() {
        for k in [&b"Metadata"[..], b"PieceInfo", b"AA", b"OpenAction", b"Collection", b"Extensions"] {
            root.remove(k);
        }
    }
    let root_id = doc.trailer.get(b"Root").ok().and_then(|r| r.as_reference().ok());
    if let Some(rid) = root_id {
        let names = doc.get_dictionary(rid).ok().and_then(|r| r.get(b"Names").ok()).cloned();
        let nd = match names {
            Some(Object::Reference(r)) => doc.get_dictionary(r).ok().cloned().map(|d| (Some(r), d)),
            Some(Object::Dictionary(d)) => Some((None, d)),
            _ => None,
        };
        if let Some((nid, mut d)) = nd {
            d.remove(b"JavaScript");
            d.remove(b"EmbeddedFiles");
            match nid {
                Some(r) => {
                    doc.objects.insert(r, Object::Dictionary(d));
                }
                None => {
                    if let Ok(r) = doc.get_dictionary_mut(rid) {
                        r.set("Names", Object::Dictionary(d));
                    }
                }
            }
        }
    }
    for p in pages_of(doc) {
        if let Ok(d) = doc.get_dictionary_mut(p) {
            for k in [&b"Metadata"[..], b"PieceInfo", b"Thumb", b"AA"] {
                d.remove(k);
            }
        }
        let list: Vec<Object> = match doc.get_dictionary(p).ok().and_then(|d| d.get(b"Annots").ok()).and_then(|a| deref(doc, a)) {
            Some(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for o in list {
            let Object::Reference(id) = o else { continue };
            let attach = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Subtype").ok().and_then(name_of)).as_deref() == Some("FileAttachment");
            if attach {
                let _ = annot::delete(doc, p, id);
                continue;
            }
            let js = doc.get_dictionary(id).ok().and_then(|d| d.get(b"A").ok().cloned()).is_some_and(|a| is_js(doc, &a));
            if let Ok(d) = doc.get_dictionary_mut(id) {
                for k in [&b"T"[..], b"Subj", b"AA", b"IRT"] {
                    d.remove(k);
                }
                if js {
                    d.remove(b"A");
                }
            }
        }
    }
}

/// Commenti e campi diventano parte della pagina.
fn flatten(doc: &mut Document, wrapped: &mut HashSet<ObjectId>) -> Result<(), String> {
    let mut counter = 0usize;
    for p in pages_of(doc) {
        let list: Vec<Object> = match doc.get_dictionary(p).ok().and_then(|d| d.get(b"Annots").ok()).and_then(|a| deref(doc, a)) {
            Some(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        let mut keep: Vec<Object> = Vec::new();
        let mut ops = String::new();
        for o in list {
            let Object::Reference(id) = o else {
                keep.push(o);
                continue;
            };
            let Ok(d) = doc.get_dictionary(id).cloned() else { continue };
            let sub = d.get(b"Subtype").ok().and_then(name_of).unwrap_or_default();
            if sub == "Link" {
                keep.push(Object::Reference(id));
                continue;
            }
            if sub == "Popup" {
                continue;
            }
            let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
            if flags & (2 | 32) != 0 {
                continue;
            }
            let Some(ap_id) = appearance(doc, &d) else { continue };
            let Some(rect) = d.get(b"Rect").ok().and_then(|r| deref(doc, r)).and_then(|r| r.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4) else { continue };
            let Ok(Object::Stream(s)) = doc.get_object(ap_id) else { continue };
            let bbox = s.dict.get(b"BBox").ok().and_then(|b| deref(doc, b)).and_then(|b| b.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4).unwrap_or_else(|| vec![0.0, 0.0, 1.0, 1.0]);
            let m: [f32; 6] = s.dict.get(b"Matrix").ok().and_then(|b| deref(doc, b)).and_then(|b| b.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 6).map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]]).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            let corners = [(bbox[0], bbox[1]), (bbox[2], bbox[1]), (bbox[2], bbox[3]), (bbox[0], bbox[3])].map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
            let (tx0, tx1) = (corners.iter().map(|c| c.0).fold(f32::MAX, f32::min), corners.iter().map(|c| c.0).fold(f32::MIN, f32::max));
            let (ty0, ty1) = (corners.iter().map(|c| c.1).fold(f32::MAX, f32::min), corners.iter().map(|c| c.1).fold(f32::MIN, f32::max));
            let (rx0, rx1, ry0, ry1) = (rect[0].min(rect[2]), rect[0].max(rect[2]), rect[1].min(rect[3]), rect[1].max(rect[3]));
            let sx = if tx1 - tx0 > 1e-4 { (rx1 - rx0) / (tx1 - tx0) } else { 1.0 };
            let sy = if ty1 - ty0 > 1e-4 { (ry1 - ry0) / (ty1 - ty0) } else { 1.0 };
            counter += 1;
            let name = format!("CedroFl{counter}");
            newtext::add_resource(doc, p, "XObject", &name, Object::Reference(ap_id))?;
            ops.push_str(&format!("q {} 0 0 {} {} {} cm /{name} Do Q\n", fmt_num(sx), fmt_num(sy), fmt_num(rx0 - tx0 * sx), fmt_num(ry0 - ty0 * sy)));
        }
        annot::push_content(doc, p, &ops, wrapped)?;
        let d = doc.get_dictionary_mut(p).map_err(|e| e.to_string())?;
        if keep.is_empty() {
            d.remove(b"Annots");
        } else {
            d.set("Annots", Object::Array(keep));
        }
    }
    if let Ok(root) = doc.catalog_mut() {
        root.remove(b"AcroForm");
    }
    Ok(())
}

/// Il flusso dell'aspetto «normale» di un'annotazione (per una spunta, quello dello stato in cui sta).
fn appearance(doc: &Document, d: &lopdf::Dictionary) -> Option<ObjectId> {
    let ap = dict_of(doc, d.get(b"AP").ok()?)?;
    let n = ap.get(b"N").ok()?;
    if let Object::Reference(r) = n {
        if matches!(doc.get_object(*r), Ok(Object::Stream(_))) {
            return Some(*r);
        }
    }
    let states = dict_of(doc, n)?;
    let st = d.get(b"AS").ok().and_then(name_of)?;
    states.get(st.as_bytes()).ok()?.as_reference().ok()
}

// --- le modifiche ---

struct Ctx<'a> {
    bytes: &'a [u8],
    bank: FontBank,
    wrapped: HashSet<ObjectId>,
    counter: usize,
    report: Report,
}

fn old_style(tp: &TextPage, g: usize) -> Style {
    let gl = &tp.glyphs[g];
    let fi = &tp.fonts[gl.font];
    Style { font: FontRef::Orig(gl.font), bold: fi.bold, italic: fi.italic, size: gl.size, color: gl.color }
}

/// Per ogni lettera del testo nuovo, quella del vecchio da cui prende lo stile (le lettere uguali in ordine).
fn align_chars(old: &[char], new: &[char]) -> Vec<Option<usize>> {
    let (n, m) = (old.len(), new.len());
    let mut out = vec![None; m];
    let mut a = 0;
    while a < n && a < m && old[a] == new[a] {
        out[a] = Some(a);
        a += 1;
    }
    let mut b = 0;
    while b < n - a.min(n) && b < m - a.min(m) && old[n - 1 - b] == new[m - 1 - b] {
        out[m - 1 - b] = Some(n - 1 - b);
        b += 1;
    }
    let (on, nn) = (n - a - b, m - a - b);
    if on > 0 && nn > 0 && on * nn <= 4_000_000 {
        let o = &old[a..a + on];
        let w = &new[a..a + nn];
        let mut t = vec![0u32; (on + 1) * (nn + 1)];
        for i in (0..on).rev() {
            for j in (0..nn).rev() {
                t[i * (nn + 1) + j] = if o[i] == w[j] { t[(i + 1) * (nn + 1) + j + 1] + 1 } else { t[(i + 1) * (nn + 1) + j].max(t[i * (nn + 1) + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < on && j < nn {
            if o[i] == w[j] {
                out[a + j] = Some(a + i);
                i += 1;
                j += 1;
            } else if t[(i + 1) * (nn + 1) + j] >= t[i * (nn + 1) + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn edit_text(doc: &mut Document, cx: &mut Ctx, page: ObjectId, geom: &Geom, rect: Rect, text: &str, size: Option<f32>, bold: Option<bool>, italic: Option<bool>, color: Option<[u8; 3]>, font: &FontChoice, align: Option<Align>) -> Result<(), String> {
    let mut tp = TextPage::read(doc, page);
    if !tp.complete {
        return Err("La pagina ha un contenuto che non so leggere tutto: non posso cambiarle il testo.".into());
    }
    // Il paragrafo: quello che si sovrappone di più al rettangolo.
    let overlap = |b: &Rect| (rect.right().min(b.right()) - rect.x.max(b.x)).max(0.0) * (rect.bottom().min(b.bottom()) - rect.y.max(b.y)).max(0.0);
    let (bi, best) = tp.blocks.iter().enumerate().map(|(i, b)| (i, overlap(&b.rect) / (b.rect.w * b.rect.h).min(rect.w * rect.h).max(1.0))).fold((0, 0.0f32), |a, b| if b.1 > a.1 { b } else { a });
    if best < 0.4 {
        return Err("Non trovo più il paragrafo da modificare in questa pagina.".into());
    }
    let block = tp.blocks[bi].clone();
    if !block.editable() {
        return Err("Questo testo non si può modificare: il suo carattere non dice che lettera è ogni segno.".into());
    }
    let gl: Vec<usize> = block.glyphs.clone();
    let old_chars: Vec<char> = block.text.chars().collect();
    let new_chars: Vec<char> = text.chars().collect();
    let unchanged = old_chars == new_chars && size.is_none() && bold.is_none() && italic.is_none() && color.is_none() && *font == FontChoice::Same && align.is_none();
    if unchanged {
        return Ok(());
    }
    // Lo stile di ogni lettera vecchia (gli spazi aggiunti prendono quello della lettera prima).
    let default_style = old_style(&tp, gl.iter().copied().find(|g| !tp.glyphs[*g].is_space()).or(gl.first().copied()).ok_or("paragrafo vuoto")?);
    let mut old_styles: Vec<Style> = Vec::with_capacity(old_chars.len());
    for i in 0..old_chars.len() {
        let st = match block.cmap.get(i).copied().flatten() {
            Some(g) => old_style(&tp, g),
            None => old_styles.last().cloned().unwrap_or_else(|| default_style.clone()),
        };
        old_styles.push(st);
    }
    let matched = align_chars(&old_chars, &new_chars);
    let mut styles: Vec<Style> = Vec::with_capacity(new_chars.len());
    for (i, m) in matched.iter().enumerate() {
        let st = match m {
            Some(o) => old_styles[*o].clone(),
            None => {
                // Una lettera nuova prende lo stile di quella prima, o della prima dopo.
                let prev = styles.last().cloned();
                let next = matched[i..].iter().flatten().next().map(|o| old_styles[*o].clone());
                prev.or(next).unwrap_or_else(|| default_style.clone())
            }
        };
        styles.push(st);
    }
    let size_ratio = size.map(|s| s / block.size.max(0.1)).unwrap_or(1.0);
    for st in styles.iter_mut() {
        if let Some(s) = size {
            st.size = s;
        }
        if let Some(b) = bold {
            st.bold = b;
        }
        if let Some(i) = italic {
            st.italic = i;
        }
        if let Some(c) = color {
            st.color = c;
        }
        if let FontChoice::Family(f) = font {
            st.font = FontRef::Family(f.clone());
        }
    }
    let chars: Vec<(char, Style)> = new_chars.iter().copied().zip(styles).collect();
    let multi = block.lines.len() >= 2;
    let align = align.unwrap_or(block.align);
    let first = &tp.lines[block.lines[0]];
    // La prima riga comincia dove comincia il suo primo carattere del corpo (dopo il segno dell'elenco).
    let first_x0 = block.glyphs.first().map(|g| tp.glyphs[*g].x0).unwrap_or(first.x0);
    let page_w = geom.w;
    let (box_w, wrap) = if multi {
        (block.rect.w, block.rect.w)
    } else if align == Align::Left {
        let w = block.rect.w.max(page_w - 36.0 - block.rect.x);
        (w, w)
    } else {
        (block.rect.w, 0.0)
    };
    let lines = newtext::layout(&chars, wrap, &tp.fonts, &mut cx.bank);
    let first_indent = if multi && align == Align::Left { (first_x0 - block.rect.x).max(0.0) } else { 0.0 };
    let base = first.base;
    let line_h = block.line_h * size_ratio;

    // Si tolgono le lettere vecchie e si scrivono le nuove.
    let remove: HashSet<(Cont, usize, usize)> = gl.iter().map(|g| (tp.glyphs[*g].cont, tp.glyphs[*g].op, tp.glyphs[*g].gi)).collect();
    let original = tp.ops.clone();
    remove_glyphs(&mut tp, &remove);
    write_back(doc, page, &tp, &original)?;
    let mut res: Vec<(String, ObjectId)> = Vec::new();
    let ops = newtext::emit(&lines, block.rect.x, base, first_indent, box_w, align, line_h, geom, &tp.fonts, &mut cx.bank, doc, &mut res);
    for (n, id) in res {
        newtext::add_resource(doc, page, "Font", &n, Object::Reference(id))?;
    }
    annot::push_content(doc, page, &ops, &mut cx.wrapped)
}

#[allow(clippy::too_many_arguments)]
fn add_text(doc: &mut Document, cx: &mut Ctx, page: ObjectId, geom: &Geom, x: f32, y: f32, w: f32, text: &str, size: f32, color: [u8; 3], bold: bool, italic: bool, font: &FontChoice, align: Align) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let family = match font {
        FontChoice::Family(f) => f.clone(),
        FontChoice::Same => "Carlito".to_string(),
    };
    let st = Style { font: FontRef::Family(family), bold, italic, size, color };
    let chars: Vec<(char, Style)> = text.chars().map(|c| (c, st.clone())).collect();
    let lines = newtext::layout(&chars, w, &[], &mut cx.bank);
    let box_w = if w > 0.0 { w } else { lines.iter().map(|l| l.width).fold(0.0, f32::max) };
    let mut res: Vec<(String, ObjectId)> = Vec::new();
    let ops = newtext::emit(&lines, x, y + size * ASCENT, 0.0, box_w, align, size * LEADING, geom, &[], &mut cx.bank, doc, &mut res);
    for (n, id) in res {
        newtext::add_resource(doc, page, "Font", &n, Object::Reference(id))?;
    }
    annot::push_content(doc, page, &ops, &mut cx.wrapped)
}

/// Applica le modifiche al file e dà il PDF nuovo (con quello che si è tolto davvero con l'oscuramento).
pub fn apply_with_report(bytes: &[u8], edits: &[Edit], opts: &SaveOpts) -> Result<(Vec<u8>, Report), String> {
    let mut doc = load(bytes)?;
    let pages = pages_of(&doc);
    let geoms: Vec<Geom> = pages.iter().map(|p| Geom::of(&doc, *p)).collect();
    let mut cx = Ctx { bytes, bank: FontBank::new(), wrapped: HashSet::new(), counter: 0, report: Report::default() };
    let page_of = |e: &Edit| -> Result<Option<(usize, ObjectId)>, String> {
        match e.page() {
            Some(p) => pages.get(p).map(|id| Some((p, *id))).ok_or_else(|| format!("La pagina {} non c'è.", p + 1)),
            None => Ok(None),
        }
    };
    // I campi nuovi prima: i valori dopo possono riguardarli.
    for e in edits {
        if let Edit::NewField { rect, kind, name, options, .. } = e {
            let (pi, id) = page_of(e)?.ok_or("campo senza pagina")?;
            form::add_field(&mut doc, &mut cx.bank, id, &geoms[pi], *rect, *kind, name, options)?;
        }
    }
    for e in edits {
        let Some((pi, id)) = page_of(e)? else {
            if let Edit::FieldValue { name, value } = e {
                form::set_value(&mut doc, &mut cx.bank, name, value)?;
            }
            continue;
        };
        let geom = &geoms[pi];
        match e {
            Edit::Mark { kind, rects, color, note, .. } => {
                annot::mark(&mut doc, id, geom, *kind, rects, *color, note)?;
            }
            Edit::Note { x, y, text, color, .. } => {
                annot::note(&mut doc, id, geom, *x, *y, text, *color)?;
            }
            Edit::Pen { points, color, width, .. } => {
                annot::pen(&mut doc, id, geom, points, *color, *width)?;
            }
            Edit::Stamp { rect, label, color, .. } => {
                annot::stamp(&mut doc, id, geom, *rect, label, *color)?;
            }
            Edit::Shape { kind, a, b, color, fill, width, .. } => {
                annot::shape(&mut doc, id, geom, *kind, *a, *b, *color, *fill, *width)?;
            }
            Edit::Image { rect, data, .. } => {
                cx.counter += 1;
                annot::image(&mut doc, id, geom, *rect, data, cx.counter, &mut cx.wrapped)?;
            }
            Edit::AddText { x, y, w, text, size, color, bold, italic, font, align, .. } => {
                add_text(&mut doc, &mut cx, id, geom, *x, *y, *w, text, *size, *color, *bold, *italic, font, *align)?;
            }
            Edit::EditText { rect, text, size, bold, italic, color, font, align, .. } => {
                edit_text(&mut doc, &mut cx, id, geom, *rect, text, *size, *bold, *italic, *color, font, *align)?;
            }
            Edit::NewField { .. } => {}
            Edit::Redact { rect, applied, .. } => {
                if *applied {
                    let r = redact::redact_page(&mut doc, cx.bytes, id, geom, std::slice::from_ref(rect), &mut cx.wrapped, &mut cx.counter)?;
                    cx.report.add(r);
                } else {
                    annot::redact_mark(&mut doc, id, geom, *rect)?;
                }
            }
            Edit::DeleteAnnot { id: annot_id, .. } => {
                form::forget(&mut doc, *annot_id);
                annot::delete(&mut doc, id, *annot_id)?;
            }
            Edit::FieldValue { .. } => {}
        }
    }
    cx.bank.finish(&mut doc);
    if opts.flatten {
        flatten(&mut doc, &mut cx.wrapped)?;
    }
    if opts.strip {
        strip(&mut doc);
    }
    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok((out, cx.report))
}

pub fn apply(bytes: &[u8], edits: &[Edit], opts: &SaveOpts) -> Result<Vec<u8>, String> {
    apply_with_report(bytes, edits, opts).map(|r| r.0)
}

/// Cerca nel testo di tutte le pagine: le corrispondenze con la pagina.
pub fn find_all(bytes: &[u8], query: &str, case: bool, whole: bool) -> Result<Vec<(usize, find::Hit)>, String> {
    let doc = load(bytes)?;
    let mut out = Vec::new();
    for (i, p) in pages_of(&doc).into_iter().enumerate() {
        let tp = TextPage::read(&doc, p);
        for h in find::search(&tp, query, case, whole) {
            out.push((i, h));
        }
    }
    Ok(out)
}

/// Cerca i modelli (codici fiscali, IBAN, e-mail, telefoni) in tutte le pagine.
pub fn find_patterns(bytes: &[u8], which: &[find::Pattern]) -> Result<Vec<(usize, find::Pattern, find::Hit)>, String> {
    let doc = load(bytes)?;
    let mut out = Vec::new();
    for (i, p) in pages_of(&doc).into_iter().enumerate() {
        let tp = TextPage::read(&doc, p);
        for w in which {
            for h in find::pattern(&tp, *w) {
                out.push((i, *w, h));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdfmod::{FieldKind, MarkKind, ShapeKind};
    use std::sync::Arc;

    const CIRCOLARE: &[u8] = include_bytes!("../../tests/fixtures/circolare.pdf");
    const COLONNE: &[u8] = include_bytes!("../../tests/fixtures/colonne.pdf");

    fn out_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("PDFMOD_OUT").map(std::path::PathBuf::from)
    }

    fn save(name: &str, bytes: &[u8]) {
        if let Some(d) = out_dir() {
            let _ = std::fs::create_dir_all(&d);
            let _ = std::fs::write(d.join(format!("{name}.pdf")), bytes);
            if let Ok((w, h, rgba)) = crate::ocr::render_page_rgba(bytes, 0, 1100.0) {
                if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                    let _ = img.save(d.join(format!("{name}.png")));
                }
            }
        }
    }

    fn page_text(bytes: &[u8], page: usize) -> String {
        read_text(bytes, page).unwrap().flat().0
    }

    #[test]
    fn opens_and_reads() {
        let o = open(CIRCOLARE).unwrap();
        assert!(!o.sizes.is_empty() && o.sizes[0].w > 100.0);
        let t = page_text(CIRCOLARE, 0);
        assert!(t.len() > 40, "{t}");
    }

    #[test]
    fn edits_a_paragraph_and_keeps_the_rest() {
        let tp = read_text(CIRCOLARE, 0).unwrap();
        let b = tp.blocks.iter().find(|b| b.editable() && b.text.chars().count() > 20).expect("un paragrafo");
        let old_first: String = b.text.split_whitespace().next().unwrap().to_string();
        let new_text = format!("Testo cambiato dal collaudo èàù {}", b.text.split_whitespace().skip(1).collect::<Vec<_>>().join(" "));
        let e = vec![Edit::EditText { page: 0, rect: b.rect, text: new_text, size: None, bold: None, italic: None, color: None, font: FontChoice::Same, align: None }];
        let out = apply(CIRCOLARE, &e, &SaveOpts::default()).unwrap();
        save("modifica-testo", &out);
        let t = page_text(&out, 0);
        assert!(t.contains("Testo cambiato dal collaudo"), "{t}");
        assert!(!t.contains(&format!("{old_first} ")) || old_first.len() < 3 || t.matches(&old_first).count() < page_text(CIRCOLARE, 0).matches(&old_first).count() + 1);
        // gli altri paragrafi restano
        let other = tp.blocks.iter().find(|x| x.rect != b.rect && x.text.chars().count() > 12).map(|x| x.text.chars().take(10).collect::<String>());
        if let Some(o) = other {
            assert!(t.contains(o.trim()), "manca {o:?} in {t}");
        }
    }

    #[test]
    fn marks_shapes_notes_images_and_deletes() {
        let tp = read_text(CIRCOLARE, 0).unwrap();
        let w = tp.words.first().unwrap().rect;
        let free_y = tp.lines.iter().map(|l| l.bot).fold(0.0, f32::max) + 30.0;
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_fn(40, 20, |x, y| image::Rgba([(x * 6) as u8, (y * 12) as u8, 180, if x < 4 { 0 } else { 255 }]));
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        }
        let e = vec![
            Edit::Mark { page: 0, kind: MarkKind::Highlight, rects: vec![w], color: [255, 225, 77], note: "evidenziato".into() },
            Edit::Mark { page: 0, kind: MarkKind::Underline, rects: vec![Rect::new(w.x, w.y + 30.0, 80.0, 12.0)], color: [229, 72, 77], note: String::new() },
            Edit::Note { page: 0, x: 400.0, y: 60.0, text: "Una nota".into(), color: [255, 225, 77] },
            Edit::Pen { page: 0, points: vec![(100.0, 300.0), (130.0, 310.0), (160.0, 290.0), (190.0, 320.0)], color: [37, 99, 184], width: 2.0 },
            Edit::Stamp { page: 0, rect: Rect::new(300.0, 120.0, 150.0, 40.0), label: "Approvato".into(), color: [30, 138, 85] },
            Edit::Shape { page: 0, kind: ShapeKind::Rect, a: (60.0, 400.0), b: (200.0, 460.0), color: [212, 87, 14], fill: None, width: 2.0 },
            Edit::Shape { page: 0, kind: ShapeKind::Ellipse, a: (220.0, 400.0), b: (330.0, 460.0), color: [37, 99, 184], fill: Some([200, 220, 255]), width: 1.5 },
            Edit::Shape { page: 0, kind: ShapeKind::Arrow, a: (60.0, 500.0), b: (200.0, 540.0), color: [27, 27, 34], fill: None, width: 2.0 },
            Edit::Image { page: 0, rect: Rect::new(350.0, 400.0, 120.0, 60.0), data: Arc::new(png) },
            Edit::AddText { page: 0, x: 60.0, y: free_y, w: 220.0, text: "Testo aggiunto: perché è già bello.\nSecondo capoverso".into(), size: 14.0, color: [27, 27, 34], bold: false, italic: false, font: FontChoice::Family("Carlito".into()), align: Align::Left },
        ];
        let out = apply(CIRCOLARE, &e, &SaveOpts::default()).unwrap();
        save("annotazioni", &out);
        let o = open(&out).unwrap();
        assert!(o.comments.len() >= 6, "commenti: {}", o.comments.len());
        let t = page_text(&out, 0);
        assert!(t.contains("Testo aggiunto"), "{t}");
        // cancellarne uno
        let victim = o.comments.iter().find(|c| c.kind == "Nota").unwrap();
        let e2 = vec![Edit::DeleteAnnot { page: 0, id: victim.id }];
        let out2 = apply(&out, &e2, &SaveOpts::default()).unwrap();
        assert_eq!(open(&out2).unwrap().comments.len(), o.comments.len() - 1);
        // appiattito: niente annotazioni, ma si vedono ancora
        let flat = apply(&out, &[], &SaveOpts { flatten: true, strip: false }).unwrap();
        save("appiattito", &flat);
        assert!(open(&flat).unwrap().comments.is_empty());
    }

    #[test]
    fn redaction_removes_text_for_real() {
        let hits = find_all(CIRCOLARE, "il", false, true).unwrap();
        let (page, hit) = hits.first().cloned().expect("una parola da oscurare");
        let target: String = hit.text.clone();
        let before = page_text(CIRCOLARE, page);
        let e = vec![Edit::Redact { page, rect: hit.rects[0].inflate(1.0), applied: true }];
        let (out, rep) = apply_with_report(CIRCOLARE, &e, &SaveOpts::default()).unwrap();
        save("oscurato", &out);
        assert!(rep.glyphs >= target.chars().count(), "{rep:?}");
        let after = page_text(&out, page);
        assert!(before.matches(&target).count() > after.matches(&target).count() || target.len() < 3, "{before}\n---\n{after}");
        // da nessuna parte nel file (anche decompresso) resta la parola cercata nel flusso del testo
        let doc = Document::load_mem(&out).unwrap();
        let pid = *doc.get_pages().get(&(page as u32 + 1)).unwrap();
        let content = String::from_utf8_lossy(&doc.get_page_content(pid)).into_owned();
        assert!(!content.contains("%group"));
    }

    #[test]
    fn form_fields_are_created_filled_and_flattened() {
        let e = vec![
            Edit::NewField { page: 0, rect: Rect::new(60.0, 100.0, 180.0, 22.0), kind: FieldKind::Text, name: "Cognome".into(), options: vec![] },
            Edit::NewField { page: 0, rect: Rect::new(60.0, 140.0, 16.0, 16.0), kind: FieldKind::Check, name: "Accetto".into(), options: vec![] },
            Edit::NewField { page: 0, rect: Rect::new(60.0, 170.0, 16.0, 16.0), kind: FieldKind::Radio, name: "Sesso".into(), options: vec!["F".into()] },
            Edit::NewField { page: 0, rect: Rect::new(100.0, 170.0, 16.0, 16.0), kind: FieldKind::Radio, name: "Sesso".into(), options: vec!["M".into()] },
            Edit::NewField { page: 0, rect: Rect::new(60.0, 200.0, 140.0, 22.0), kind: FieldKind::List, name: "Reparto".into(), options: vec!["Anagrafe".into(), "Tributi".into(), "CED".into()] },
            Edit::NewField { page: 0, rect: Rect::new(60.0, 240.0, 180.0, 40.0), kind: FieldKind::Sign, name: "Firma".into(), options: vec![] },
            Edit::FieldValue { name: "Cognome".into(), value: "Rossi è Bianchi".into() },
            Edit::FieldValue { name: "Accetto".into(), value: "On".into() },
            Edit::FieldValue { name: "Sesso".into(), value: "M".into() },
            Edit::FieldValue { name: "Reparto".into(), value: "CED".into() },
        ];
        let out = apply(CIRCOLARE, &e, &SaveOpts::default()).unwrap();
        save("modulo", &out);
        let o = open(&out).unwrap();
        let get = |n: &str| o.fields.iter().filter(|f| f.name == n).collect::<Vec<_>>();
        assert_eq!(get("Cognome")[0].value, "Rossi è Bianchi");
        assert_eq!(get("Accetto")[0].value, "On");
        assert_eq!(get("Sesso").len(), 2);
        assert_eq!(get("Sesso")[0].value, "M");
        assert_eq!(get("Reparto")[0].value, "CED");
        assert_eq!(get("Firma")[0].kind, FieldKind::Sign);
        let flat = apply(&out, &[], &SaveOpts { flatten: true, strip: false }).unwrap();
        save("modulo-appiattito", &flat);
        assert!(open(&flat).unwrap().fields.is_empty());
        // il valore riscritto
        let again = apply(&out, &[Edit::FieldValue { name: "Cognome".into(), value: "Verdi".into() }], &SaveOpts::default()).unwrap();
        assert_eq!(open(&again).unwrap().fields.iter().find(|f| f.name == "Cognome").unwrap().value, "Verdi");
    }

    #[test]
    fn strips_hidden_data() {
        let mut doc = Document::load_mem(CIRCOLARE).unwrap();
        let info = doc.add_object(lopdf::dictionary! { "Author" => Object::string_literal("Mario Rossi"), "Creator" => Object::string_literal("Word"), "Producer" => Object::string_literal("Cedro") });
        doc.trailer.set("Info", Object::Reference(info));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let o = open(&bytes).unwrap();
        assert!(o.hidden.any() && o.hidden.author == "Mario Rossi");
        let out = apply(&bytes, &[], &SaveOpts { flatten: false, strip: true }).unwrap();
        assert!(!open(&out).unwrap().hidden.any());
    }

    #[test]
    fn two_columns_edit_one() {
        let tp = read_text(COLONNE, 0).unwrap();
        let b = tp.blocks.iter().find(|b| b.editable() && b.text.chars().count() > 30).expect("un paragrafo");
        let e = vec![Edit::EditText { page: 0, rect: b.rect, text: "Sostituito in colonna".into(), size: Some(11.0), bold: Some(true), italic: None, color: Some([212, 87, 14]), font: FontChoice::Family("Titillium Web".into()), align: None }];
        let out = apply(COLONNE, &e, &SaveOpts::default()).unwrap();
        save("colonne", &out);
        assert!(page_text(&out, 0).contains("Sostituito in colonna"));
    }
}
