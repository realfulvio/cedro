//! I moduli (AcroForm): i campi si leggono con il loro posto, si compilano (con l'aspetto già disegnato, così
//! lo vedono tutti i lettori) e se ne aggiungono di nuovi. Un campo può avere più «widget» (i tondini di un
//! gruppo, per esempio): ognuno è un oggetto della pagina.

use std::collections::HashMap;
use std::fmt::Write as _;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, StringFormat};

use super::annot::{fill_op, form_xobject, link_annot, stroke_op, text_obj};
use super::fonts::{deref, dict_of, name_of, num};
use super::geom::Geom;
use super::newtext::{self, FontBank, FontRef, Style};
use super::text::Align;
use super::{FieldKind, Rect};
use crate::pdfedit::num as fmt_num;

/// Un campo del modulo com'è sulla pagina (un widget).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    /// Testo; per una spunta «On»/«Off»; per un gruppo di tondini il valore scelto.
    pub value: String,
    /// Le voci di un elenco.
    pub options: Vec<String>,
    pub page: usize,
    pub rect: Rect,
    /// L'oggetto del widget.
    pub id: ObjectId,
    /// Per una spunta o un tondino: il valore che prende quando è acceso.
    pub export: String,
    pub multiline: bool,
    pub read_only: bool,
    pub max_len: usize,
    pub size: f32,
    pub pushbutton: bool,
}

struct Node {
    /// L'oggetto che porta il nome e il valore (può essere anche il widget).
    id: ObjectId,
    name: String,
    kind: FieldKind,
    ff: i64,
    opts: Vec<String>,
    da: String,
    max_len: usize,
    quad: i64,
    widgets: Vec<ObjectId>,
}

pub fn decode_text(o: &Object) -> String {
    match o {
        Object::String(b, _) => {
            if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
                let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            } else if b.len() >= 3 && b[..3] == [0xEF, 0xBB, 0xBF] {
                String::from_utf8_lossy(&b[3..]).into_owned()
            } else {
                b.iter().map(|c| *c as char).collect()
            }
        }
        Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

fn acroform(doc: &Document) -> Option<Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?;
    let cat = dict_of(doc, root)?;
    dict_of(doc, cat.get(b"AcroForm").ok()?).cloned()
}

fn field_list(doc: &Document) -> Vec<Object> {
    let Some(af) = acroform(doc) else { return Vec::new() };
    match af.get(b"Fields").ok().and_then(|f| deref(doc, f)) {
        Some(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

fn is_widget(d: &Dictionary) -> bool {
    d.get(b"Subtype").ok().and_then(name_of).as_deref() == Some("Widget") || d.has(b"Rect")
}

#[derive(Clone, Default)]
struct Inherit {
    ft: String,
    ff: i64,
    da: String,
    max_len: usize,
    quad: i64,
    opts: Vec<String>,
}

fn walk(doc: &Document, id: ObjectId, prefix: &str, inh: &Inherit, out: &mut Vec<Node>, depth: u8) {
    if depth > 12 {
        return;
    }
    let Ok(d) = doc.get_dictionary(id) else { return };
    let mut inh = inh.clone();
    if let Some(ft) = d.get(b"FT").ok().and_then(name_of) {
        inh.ft = ft;
    }
    if let Some(ff) = d.get(b"Ff").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_i64().ok()) {
        inh.ff = ff;
    }
    if let Some(da) = d.get(b"DA").ok().and_then(|o| deref(doc, o)) {
        inh.da = decode_text(da);
    }
    if let Some(m) = d.get(b"MaxLen").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_i64().ok()) {
        inh.max_len = m.max(0) as usize;
    }
    if let Some(q) = d.get(b"Q").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_i64().ok()) {
        inh.quad = q;
    }
    if let Some(Object::Array(a)) = d.get(b"Opt").ok().and_then(|o| deref(doc, o)) {
        inh.opts = a
            .iter()
            .filter_map(|o| match deref(doc, o)? {
                Object::Array(p) => p.first().map(decode_text),
                o => Some(decode_text(o)),
            })
            .collect();
    }
    let partial = d.get(b"T").ok().and_then(|o| deref(doc, o)).map(decode_text);
    let name = match (&partial, prefix.is_empty()) {
        (Some(p), true) => p.clone(),
        (Some(p), false) => format!("{prefix}.{p}"),
        (None, _) => prefix.to_string(),
    };
    let kids: Vec<ObjectId> = match d.get(b"Kids").ok().and_then(|o| deref(doc, o)) {
        Some(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
        _ => Vec::new(),
    };
    let kid_fields: Vec<ObjectId> = kids.iter().copied().filter(|k| doc.get_dictionary(*k).is_ok_and(|kd| kd.has(b"T") || kd.has(b"Kids") && !kd.has(b"Rect"))).collect();
    let kid_widgets: Vec<ObjectId> = kids.iter().copied().filter(|k| !kid_fields.contains(k)).collect();
    let kind = match inh.ft.as_str() {
        "Tx" => Some(FieldKind::Text),
        "Btn" => Some(if inh.ff & 32768 != 0 {
            FieldKind::Radio
        } else {
            FieldKind::Check
        }),
        "Ch" => Some(FieldKind::List),
        "Sig" => Some(FieldKind::Sign),
        _ => None,
    };
    if let Some(kind) = kind {
        let mut widgets = kid_widgets;
        if kids.is_empty() && is_widget(d) {
            widgets.push(id);
        }
        if !widgets.is_empty() || kid_fields.is_empty() {
            out.push(Node { id, name: name.clone(), kind, ff: inh.ff, opts: inh.opts.clone(), da: inh.da.clone(), max_len: inh.max_len, quad: inh.quad, widgets });
        }
    }
    for k in kid_fields {
        walk(doc, k, &name, &inh, out, depth + 1);
    }
}

fn nodes(doc: &Document) -> Vec<Node> {
    let mut out = Vec::new();
    let af = acroform(doc);
    let mut inh = Inherit::default();
    if let Some(af) = &af {
        if let Some(da) = af.get(b"DA").ok().and_then(|o| deref(doc, o)) {
            inh.da = decode_text(da);
        }
        if let Some(q) = af.get(b"Q").ok().and_then(|o| o.as_i64().ok()) {
            inh.quad = q;
        }
    }
    for f in field_list(doc) {
        if let Object::Reference(id) = f {
            walk(doc, id, "", &inh, &mut out, 0);
        }
    }
    out
}

fn state_names(doc: &Document, widget: ObjectId) -> Vec<String> {
    let Ok(d) = doc.get_dictionary(widget) else { return Vec::new() };
    let Some(ap) = d.get(b"AP").ok().and_then(|a| dict_of(doc, a)) else { return Vec::new() };
    let Some(n) = ap.get(b"N").ok().and_then(|a| dict_of(doc, a)) else { return Vec::new() };
    n.iter().map(|(k, _)| String::from_utf8_lossy(k).into_owned()).collect()
}

fn on_state(doc: &Document, widget: ObjectId) -> String {
    state_names(doc, widget).into_iter().find(|s| s != "Off").unwrap_or_else(|| "Yes".into())
}

fn da_size(da: &str) -> f32 {
    let t: Vec<&str> = da.split_whitespace().collect();
    t.iter().position(|x| *x == "Tf").and_then(|i| i.checked_sub(1)).and_then(|i| t[i].parse().ok()).unwrap_or(0.0)
}

fn da_color(da: &str) -> [u8; 3] {
    let t: Vec<&str> = da.split_whitespace().collect();
    if let Some(i) = t.iter().position(|x| *x == "rg") {
        if i >= 3 {
            let v: Vec<f32> = t[i - 3..i].iter().filter_map(|s| s.parse().ok()).collect();
            if v.len() == 3 {
                return [(v[0] * 255.0) as u8, (v[1] * 255.0) as u8, (v[2] * 255.0) as u8];
            }
        }
    }
    if let Some(i) = t.iter().position(|x| *x == "g") {
        if i >= 1 {
            if let Ok(g) = t[i - 1].parse::<f32>() {
                let v = (g * 255.0) as u8;
                return [v, v, v];
            }
        }
    }
    [0, 0, 0]
}

/// I campi del modulo, uno per widget, con la pagina e il rettangolo come si vede.
pub fn fields(doc: &Document, pages: &[ObjectId]) -> Vec<Field> {
    let mut by_page: HashMap<ObjectId, usize> = HashMap::new();
    for (i, p) in pages.iter().enumerate() {
        let list: Vec<Object> = match doc.get_dictionary(*p).ok().and_then(|d| d.get(b"Annots").ok()).and_then(|a| deref(doc, a)) {
            Some(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for o in list {
            if let Object::Reference(r) = o {
                by_page.insert(r, i);
            }
        }
    }
    let geoms: Vec<Geom> = pages.iter().map(|p| Geom::of(doc, *p)).collect();
    let mut out = Vec::new();
    for n in nodes(doc) {
        let nd = doc.get_dictionary(n.id).ok();
        let v_obj = nd.and_then(|d| d.get(b"V").ok()).and_then(|o| deref(doc, o)).cloned();
        let value_raw = v_obj.as_ref().map(decode_text).unwrap_or_default();
        for w in &n.widgets {
            let Ok(wd) = doc.get_dictionary(*w) else { continue };
            let Some(page) = wd.get(b"P").ok().and_then(|p| p.as_reference().ok()).and_then(|p| pages.iter().position(|x| *x == p)).or_else(|| by_page.get(w).copied()) else { continue };
            let Some(r) = wd.get(b"Rect").ok().and_then(|r| deref(doc, r)).and_then(|r| r.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4) else { continue };
            let rect = geoms[page].rect_to_view(r[0], r[1], r[2], r[3]);
            let export = if matches!(n.kind, FieldKind::Check | FieldKind::Radio) { on_state(doc, *w) } else { String::new() };
            let value = match n.kind {
                FieldKind::Check => {
                    let own = wd.get(b"AS").ok().and_then(name_of);
                    let on = match own {
                        Some(s) => s != "Off",
                        None => !value_raw.is_empty() && value_raw != "Off",
                    };
                    (if on { "On" } else { "Off" }).to_string()
                }
                FieldKind::Radio => {
                    if value_raw == "Off" {
                        String::new()
                    } else {
                        value_raw.clone()
                    }
                }
                _ => value_raw.clone(),
            };
            let da = if n.da.is_empty() { String::new() } else { n.da.clone() };
            out.push(Field {
                name: n.name.clone(),
                kind: n.kind,
                value,
                options: n.opts.clone(),
                page,
                rect,
                id: *w,
                export,
                multiline: n.kind == FieldKind::Text && n.ff & 4096 != 0,
                read_only: n.ff & 1 != 0,
                max_len: n.max_len,
                size: da_size(&da),
                pushbutton: n.kind != FieldKind::Radio && n.ff & 65536 != 0,
            });
        }
    }
    out
}

/// L'aspetto di un campo di testo o di un elenco: sfondo e bordo (da MK), poi il testo.
#[allow(clippy::too_many_arguments)]
fn text_appearance(doc: &mut Document, bank: &mut FontBank, widget: ObjectId, text: &str, multiline: bool, size_da: f32, da: &str, quad: i64, comb: usize) -> Result<ObjectId, String> {
    let wd = doc.get_dictionary(widget).map_err(|e| e.to_string())?.clone();
    let r = wd.get(b"Rect").ok().and_then(|r| deref(doc, r)).and_then(|r| r.as_array().ok()).map(|a| a.iter().filter_map(num).collect::<Vec<f32>>()).filter(|v| v.len() == 4).ok_or("campo senza rettangolo")?;
    let (w, h) = ((r[2] - r[0]).abs().max(1.0), (r[3] - r[1]).abs().max(1.0));
    let mk = wd.get(b"MK").ok().and_then(|m| dict_of(doc, m)).cloned().unwrap_or_default();
    let col = |k: &[u8]| -> Option<[u8; 3]> {
        let a = mk.get(k).ok()?.as_array().ok()?;
        let v: Vec<f32> = a.iter().filter_map(num).collect();
        match v.len() {
            1 => Some([(v[0] * 255.0) as u8; 3]),
            3 => Some([(v[0] * 255.0) as u8, (v[1] * 255.0) as u8, (v[2] * 255.0) as u8]),
            _ => None,
        }
    };
    let mut c = String::new();
    if let Some(bg) = col(b"BG") {
        let _ = writeln!(c, "{} 0 0 {} {} re f", fill_op(bg), fmt_num(w), fmt_num(h));
    }
    if let Some(bc) = col(b"BC") {
        let _ = writeln!(c, "{} 1 w 0.5 0.5 {} {} re S", stroke_op(bc), fmt_num(w - 1.0), fmt_num(h - 1.0));
    }
    let mut fonts = Dictionary::new();
    if !text.is_empty() {
        let size = if size_da > 0.5 {
            size_da
        } else if multiline {
            10.0
        } else {
            (h - 4.0).clamp(5.0, 12.0)
        };
        let color = da_color(da);
        let st = Style { font: FontRef::Family("Arial".into()), bold: false, italic: false, size, color };
        let mut text = text.to_string();
        if !multiline {
            text = text.replace(['\n', '\r'], " ");
        }
        let chars: Vec<(char, Style)> = text.chars().map(|ch| (ch, st.clone())).collect();
        let box_w = (w - 4.0).max(1.0);
        let lines = newtext::layout(&chars, if multiline { box_w } else { 0.0 }, &[], bank);
        let line_h = size * newtext::LEADING;
        let base = if multiline { 2.0 + size * newtext::ASCENT } else { (h - size * (newtext::ASCENT - 0.2)) / 2.0 + size * 0.02 + size * 0.15 };
        let align = match quad {
            1 => Align::Center,
            2 => Align::Right,
            _ => Align::Left,
        };
        let mut res: Vec<(String, ObjectId)> = Vec::new();
        let mut lines = lines;
        if comb > 0 && !multiline {
            // Campo a caselle: ogni lettera al centro della sua casella.
            let cell = w / comb as f32;
            for l in lines.iter_mut() {
                let mut segs = Vec::new();
                for s in l.segs.drain(..) {
                    let n = s.text.chars().count().max(1) as f32;
                    for ch in s.text.chars() {
                        let cw = s.w / n;
                        let mut one = s.clone();
                        one.text = ch.to_string();
                        one.x = segs.len() as f32 * cell + (cell - cw) / 2.0;
                        one.w = cw;
                        segs.push(one);
                    }
                }
                l.segs = segs;
                l.width = w;
            }
        }
        let geom = Geom::local(w, h);
        let ops = newtext::emit(&lines, 2.0, h - base, 0.0, box_w, align, line_h, &geom, &[], bank, doc, &mut res);
        let _ = writeln!(c, "/Tx BMC q 1 1 {} {} re W n", fmt_num(w - 2.0), fmt_num(h - 2.0));
        c.push_str(&ops);
        c.push_str("Q EMC\n");
        for (n, id) in res {
            fonts.set(n, Object::Reference(id));
        }
    }
    let res = dictionary! { "Font" => Object::Dictionary(fonts) };
    Ok(form_xobject(doc, [0.0, 0.0, w, h], None, c, res))
}

fn set_ap(doc: &mut Document, widget: ObjectId, ap: ObjectId) -> Result<(), String> {
    let d = doc.get_dictionary_mut(widget).map_err(|e| e.to_string())?;
    d.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(ap) }));
    Ok(())
}

fn text_value(s: &str) -> Object {
    text_obj(s)
}

/// Il valore di un campo. Le spunte vogliono «On» o «Off»; i tondini il valore scelto (o vuoto per nessuno).
pub fn set_value(doc: &mut Document, bank: &mut FontBank, name: &str, value: &str) -> Result<(), String> {
    let ns = nodes(doc);
    let targets: Vec<&Node> = ns.iter().filter(|n| n.name == name).collect();
    if targets.is_empty() {
        return Err(format!("Il campo «{name}» non c'è."));
    }
    for n in targets {
        match n.kind {
            FieldKind::Text | FieldKind::List => {
                let mut v = value.to_string();
                if n.max_len > 0 {
                    v = v.chars().take(n.max_len).collect();
                }
                doc.get_dictionary_mut(n.id).map_err(|e| e.to_string())?.set("V", text_value(&v));
                let multiline = n.kind == FieldKind::Text && n.ff & 4096 != 0;
                let comb = if n.kind == FieldKind::Text && n.ff & (1 << 24) != 0 { n.max_len } else { 0 };
                let da = if n.da.is_empty() { acroform(doc).and_then(|a| a.get(b"DA").ok().map(decode_text)).unwrap_or_default() } else { n.da.clone() };
                for w in &n.widgets {
                    let ap = text_appearance(doc, bank, *w, &v, multiline, da_size(&da), &da, n.quad, comb)?;
                    set_ap(doc, *w, ap)?;
                }
            }
            FieldKind::Check => {
                let on = value.eq_ignore_ascii_case("on") || value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes");
                for w in &n.widgets {
                    let ex = on_state(doc, *w);
                    let st = if on { ex.clone() } else { "Off".into() };
                    let wd = doc.get_dictionary_mut(*w).map_err(|e| e.to_string())?;
                    wd.set("AS", Object::Name(st.clone().into_bytes()));
                    if *w == n.id || n.widgets.len() == 1 {
                        wd.set("V", Object::Name(st.into_bytes()));
                    }
                }
                if !n.widgets.contains(&n.id) {
                    let st = if on { n.widgets.first().map(|w| on_state(doc, *w)).unwrap_or_else(|| "Yes".into()) } else { "Off".into() };
                    doc.get_dictionary_mut(n.id).map_err(|e| e.to_string())?.set("V", Object::Name(st.into_bytes()));
                }
            }
            FieldKind::Radio => {
                let chosen = if value.is_empty() { "Off".to_string() } else { value.to_string() };
                for w in &n.widgets {
                    let ex = on_state(doc, *w);
                    let st = if ex == chosen { ex } else { "Off".into() };
                    doc.get_dictionary_mut(*w).map_err(|e| e.to_string())?.set("AS", Object::Name(st.into_bytes()));
                }
                doc.get_dictionary_mut(n.id).map_err(|e| e.to_string())?.set("V", Object::Name(chosen.into_bytes()));
            }
            FieldKind::Sign => {}
        }
    }
    Ok(())
}

/// Toglie un widget (e i campi che restano senza widget) dal modulo.
pub fn forget(doc: &mut Document, widget: ObjectId) {
    fn strip(doc: &mut Document, arr_owner: ObjectId, widget: ObjectId, depth: u8) {
        if depth > 12 {
            return;
        }
        let kids: Vec<Object> = match doc.get_dictionary(arr_owner).ok().and_then(|d| d.get(b"Kids").ok()).and_then(|k| deref(doc, k)) {
            Some(Object::Array(a)) => a.clone(),
            _ => return,
        };
        let keep: Vec<Object> = kids.iter().filter(|o| o.as_reference().ok() != Some(widget)).cloned().collect();
        if keep.len() != kids.len() {
            if let Ok(d) = doc.get_dictionary_mut(arr_owner) {
                d.set("Kids", Object::Array(keep.clone()));
            }
        }
        for k in keep {
            if let Object::Reference(r) = k {
                strip(doc, r, widget, depth + 1);
            }
        }
    }
    let root = doc.trailer.get(b"Root").ok().and_then(|r| r.as_reference().ok());
    let Some(root) = root else { return };
    let af_ref = doc.get_dictionary(root).ok().and_then(|c| c.get(b"AcroForm").ok()).cloned();
    let (af_id, mut af) = match af_ref {
        Some(Object::Reference(r)) => match doc.get_dictionary(r) {
            Ok(d) => (Some(r), d.clone()),
            Err(_) => return,
        },
        Some(Object::Dictionary(d)) => (None, d),
        _ => return,
    };
    let fields: Vec<Object> = match af.get(b"Fields").ok().and_then(|f| deref(doc, f)) {
        Some(Object::Array(a)) => a.clone(),
        _ => return,
    };
    let keep: Vec<Object> = fields.iter().filter(|o| o.as_reference().ok() != Some(widget)).cloned().collect();
    for f in &keep {
        if let Object::Reference(r) = f {
            strip(doc, *r, widget, 0);
        }
    }
    af.set("Fields", Object::Array(keep));
    match af_id {
        Some(r) => {
            doc.objects.insert(r, Object::Dictionary(af));
        }
        None => {
            if let Ok(c) = doc.get_dictionary_mut(root) {
                c.set("AcroForm", Object::Dictionary(af));
            }
        }
    }
}

fn ensure_acroform(doc: &mut Document) -> Result<(), String> {
    let root = doc.trailer.get(b"Root").ok().and_then(|r| r.as_reference().ok()).ok_or("catalogo non trovato")?;
    let has = doc.get_dictionary(root).ok().is_some_and(|c| c.has(b"AcroForm"));
    if has {
        return Ok(());
    }
    let helv = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let af = dictionary! { "Fields" => Object::Array(Vec::new()), "DA" => Object::string_literal("/Helv 0 Tf 0 g"), "DR" => dictionary! { "Font" => dictionary! { "Helv" => Object::Reference(helv) } } };
    doc.get_dictionary_mut(root).map_err(|e| e.to_string())?.set("AcroForm", Object::Dictionary(af));
    Ok(())
}

fn add_to_fields(doc: &mut Document, id: ObjectId) -> Result<(), String> {
    ensure_acroform(doc)?;
    let root = doc.trailer.get(b"Root").ok().and_then(|r| r.as_reference().ok()).ok_or("catalogo non trovato")?;
    let af = doc.get_dictionary(root).ok().and_then(|c| c.get(b"AcroForm").ok()).cloned();
    match af {
        Some(Object::Reference(r)) => {
            let d = doc.get_dictionary_mut(r).map_err(|e| e.to_string())?;
            push_field(d, id);
        }
        Some(Object::Dictionary(mut d)) => {
            push_field(&mut d, id);
            doc.get_dictionary_mut(root).map_err(|e| e.to_string())?.set("AcroForm", Object::Dictionary(d));
        }
        _ => {}
    }
    Ok(())
}

fn push_field(d: &mut Dictionary, id: ObjectId) {
    match d.get_mut(b"Fields") {
        Ok(Object::Array(a)) => a.push(Object::Reference(id)),
        _ => d.set("Fields", Object::Array(vec![Object::Reference(id)])),
    }
}

fn unique_name(doc: &Document, wanted: &str, kind: FieldKind) -> String {
    let taken: Vec<String> = nodes(doc).into_iter().map(|n| n.name).collect();
    if kind == FieldKind::Radio && !wanted.is_empty() {
        return wanted.to_string();
    }
    let base = if wanted.trim().is_empty() {
        match kind {
            FieldKind::Text => "Testo",
            FieldKind::Check => "Spunta",
            FieldKind::Radio => "Scelta",
            FieldKind::List => "Elenco",
            FieldKind::Sign => "Firma",
        }
        .to_string()
    } else {
        wanted.trim().to_string()
    };
    if !taken.contains(&base) {
        return base;
    }
    (2..).map(|i| format!("{base}{i}")).find(|n| !taken.contains(n)).unwrap_or(base)
}

/// Un campo nuovo. Per un tondino, `options[0]` è il valore che prende (e un gruppo è fatto di più tondini con lo stesso nome).
#[allow(clippy::too_many_arguments)]
pub fn add_field(doc: &mut Document, bank: &mut FontBank, page: ObjectId, geom: &Geom, rect: Rect, kind: FieldKind, name: &str, options: &[String]) -> Result<ObjectId, String> {
    let name = unique_name(doc, name, kind);
    let file = geom.rect_to_file(&Rect::new(rect.x, rect.y, rect.w.max(8.0), rect.h.max(8.0)));
    let (w, h) = (file[2] - file[0], file[3] - file[1]);
    let page_ref = Object::Reference(page);
    let mk_color = |c: [f32; 3]| Object::Array(c.iter().map(|v| Object::Real(*v)).collect());
    let mut mk = dictionary! { "BC" => mk_color([0.36, 0.38, 0.5]), "BG" => mk_color([0.95, 0.97, 1.0]) };
    let rect_obj = Object::Array(file.iter().map(|v| Object::Real(*v)).collect());
    let (date, _) = crate::pdfdoc::date_now();
    let mut d = dictionary! { "Type" => "Annot", "Subtype" => "Widget", "Rect" => rect_obj, "F" => 4, "P" => page_ref, "M" => Object::String(date.into_bytes(), StringFormat::Literal) };
    match kind {
        FieldKind::Text => {
            d.set("FT", "Tx");
            d.set("T", text_obj(&name));
            d.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
            d.set("MK", Object::Dictionary(mk));
            d.set("V", text_obj(""));
            let id = doc.add_object(Object::Dictionary(d));
            let ap = text_appearance(doc, bank, id, "", false, 0.0, "", 0, 0)?;
            set_ap(doc, id, ap)?;
            finish_widget(doc, page, id)?;
            Ok(id)
        }
        FieldKind::List => {
            d.set("FT", "Ch");
            d.set("Ff", 131072);
            d.set("T", text_obj(&name));
            d.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
            d.set("MK", Object::Dictionary(mk));
            let opts: Vec<Object> = if options.is_empty() { vec![text_obj("Voce 1"), text_obj("Voce 2")] } else { options.iter().map(|o| text_obj(o)).collect() };
            let first = options.first().cloned().unwrap_or_else(|| "Voce 1".into());
            d.set("Opt", Object::Array(opts));
            d.set("V", text_obj(&first));
            let id = doc.add_object(Object::Dictionary(d));
            let ap = text_appearance(doc, bank, id, &first, false, 0.0, "", 0, 0)?;
            set_ap(doc, id, ap)?;
            finish_widget(doc, page, id)?;
            Ok(id)
        }
        FieldKind::Sign => {
            d.set("FT", "Sig");
            d.set("T", text_obj(&name));
            let mut c = String::new();
            let _ = writeln!(c, "{} [3 2] 0 d 1 w 0.5 0.5 {} {} re S", stroke_op([110, 118, 160]), fmt_num(w - 1.0), fmt_num(h - 1.0));
            let size = (h * 0.28).clamp(6.0, 11.0);
            let st = Style { font: FontRef::Family("Arial".into()), bold: false, italic: false, size, color: [110, 118, 160] };
            let chars: Vec<(char, Style)> = "Firma qui".chars().map(|ch| (ch, st.clone())).collect();
            let lines = newtext::layout(&chars, 0.0, &[], bank);
            let mut res: Vec<(String, ObjectId)> = Vec::new();
            let ops = newtext::emit(&lines, 6.0, h - (h / 2.0 + size * 0.3), 0.0, w - 12.0, Align::Left, size * 1.2, &Geom::local(w, h), &[], bank, doc, &mut res);
            c.push_str(&ops);
            let mut fonts = Dictionary::new();
            for (n, id) in res {
                fonts.set(n, Object::Reference(id));
            }
            let ap = form_xobject(doc, [0.0, 0.0, w, h], None, c, dictionary! { "Font" => Object::Dictionary(fonts) });
            d.set("AP", Object::Dictionary(dictionary! { "N" => Object::Reference(ap) }));
            let id = doc.add_object(Object::Dictionary(d));
            finish_widget(doc, page, id)?;
            Ok(id)
        }
        FieldKind::Check => {
            d.set("FT", "Btn");
            d.set("T", text_obj(&name));
            d.set("V", "Off");
            d.set("AS", "Off");
            mk.set("CA", Object::string_literal("4"));
            d.set("MK", Object::Dictionary(mk.clone()));
            d.set("DA", Object::string_literal("/ZaDb 0 Tf 0 g"));
            let (on, off) = button_states(doc, w, h, false);
            d.set("AP", Object::Dictionary(dictionary! { "N" => dictionary! { "Yes" => Object::Reference(on), "Off" => Object::Reference(off) }, "D" => dictionary! { "Yes" => Object::Reference(on), "Off" => Object::Reference(off) } }));
            let id = doc.add_object(Object::Dictionary(d));
            finish_widget(doc, page, id)?;
            Ok(id)
        }
        FieldKind::Radio => {
            // Il gruppo: un campo «padre» con un widget per ogni tondino.
            let value = options.first().cloned().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
                let n = nodes(doc).iter().filter(|n| n.name == name).map(|n| n.widgets.len()).sum::<usize>();
                format!("Scelta{}", n + 1)
            });
            let (on, off) = button_states(doc, w, h, true);
            d.set("AS", "Off");
            d.set("MK", Object::Dictionary(dictionary! { "BC" => mk_color([0.36, 0.38, 0.5]), "BG" => mk_color([0.95, 0.97, 1.0]), "CA" => Object::string_literal("l") }));
            d.set("AP", Object::Dictionary(dictionary! { "N" => Object::Dictionary({
                let mut m = Dictionary::new();
                m.set(value.as_str(), Object::Reference(on));
                m.set("Off", Object::Reference(off));
                m
            }) }));
            let existing = nodes(doc).into_iter().find(|n| n.name == name && n.kind == FieldKind::Radio).map(|n| n.id);
            let parent = match existing {
                Some(p) => p,
                None => {
                    let p = doc.add_object(Object::Dictionary(dictionary! { "FT" => "Btn", "Ff" => 32768 + 16384, "T" => text_obj(&name), "V" => "Off", "Kids" => Object::Array(Vec::new()) }));
                    add_to_fields(doc, p)?;
                    p
                }
            };
            d.set("Parent", Object::Reference(parent));
            let id = doc.add_object(Object::Dictionary(d));
            if let Ok(pd) = doc.get_dictionary_mut(parent) {
                match pd.get_mut(b"Kids") {
                    Ok(Object::Array(a)) => a.push(Object::Reference(id)),
                    _ => pd.set("Kids", Object::Array(vec![Object::Reference(id)])),
                }
            }
            link_annot(doc, page, id)?;
            Ok(id)
        }
    }
}

fn finish_widget(doc: &mut Document, page: ObjectId, id: ObjectId) -> Result<(), String> {
    link_annot(doc, page, id)?;
    add_to_fields(doc, id)
}

/// Gli aspetti di una spunta o di un tondino: acceso e spento.
fn button_states(doc: &mut Document, w: f32, h: f32, round: bool) -> (ObjectId, ObjectId) {
    let bg = [243, 247, 255];
    let bc = [92, 97, 128];
    let mut base = String::new();
    if round {
        let (cx, cy, r) = (w / 2.0, h / 2.0, (w.min(h) / 2.0 - 0.7).max(1.0));
        let k = 0.552_284_7 * r;
        let _ = writeln!(base, "{} {} 1 w {} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h B",
            fill_op(bg), stroke_op(bc),
            fmt_num(cx + r), fmt_num(cy),
            fmt_num(cx + r), fmt_num(cy + k), fmt_num(cx + k), fmt_num(cy + r), fmt_num(cx), fmt_num(cy + r),
            fmt_num(cx - k), fmt_num(cy + r), fmt_num(cx - r), fmt_num(cy + k), fmt_num(cx - r), fmt_num(cy),
            fmt_num(cx - r), fmt_num(cy - k), fmt_num(cx - k), fmt_num(cy - r), fmt_num(cx), fmt_num(cy - r),
            fmt_num(cx + k), fmt_num(cy - r), fmt_num(cx + r), fmt_num(cy - k), fmt_num(cx + r), fmt_num(cy));
    } else {
        let _ = writeln!(base, "{} {} 1 w 0.5 0.5 {} {} re B", fill_op(bg), stroke_op(bc), fmt_num(w - 1.0), fmt_num(h - 1.0));
    }
    let mut on = base.clone();
    if round {
        let (cx, cy, r) = (w / 2.0, h / 2.0, (w.min(h) / 2.0 - 0.7).max(1.0) * 0.5);
        let k = 0.552_284_7 * r;
        let _ = writeln!(on, "0.12 0.14 0.25 rg {} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c f",
            fmt_num(cx + r), fmt_num(cy),
            fmt_num(cx + r), fmt_num(cy + k), fmt_num(cx + k), fmt_num(cy + r), fmt_num(cx), fmt_num(cy + r),
            fmt_num(cx - k), fmt_num(cy + r), fmt_num(cx - r), fmt_num(cy + k), fmt_num(cx - r), fmt_num(cy),
            fmt_num(cx - r), fmt_num(cy - k), fmt_num(cx - k), fmt_num(cy - r), fmt_num(cx), fmt_num(cy - r),
            fmt_num(cx + k), fmt_num(cy - r), fmt_num(cx + r), fmt_num(cy - k), fmt_num(cx + r), fmt_num(cy));
    } else {
        let _ = writeln!(on, "0.12 0.14 0.25 RG {} w 1 J 1 j {} {} m {} {} l {} {} l S", fmt_num((w.min(h) * 0.12).max(1.2)), fmt_num(w * 0.22), fmt_num(h * 0.52), fmt_num(w * 0.42), fmt_num(h * 0.26), fmt_num(w * 0.78), fmt_num(h * 0.76));
    }
    let on = form_xobject(doc, [0.0, 0.0, w, h], None, on, Dictionary::new());
    let off = form_xobject(doc, [0.0, 0.0, w, h], None, base, Dictionary::new());
    (on, off)
}
