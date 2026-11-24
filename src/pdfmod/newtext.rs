//! Il testo nuovo (aggiunto o riscritto): si impagina con le larghezze vere dei caratteri, si va a capo
//! alla larghezza data e si scrive nella pagina. Se il carattere del documento ha le lettere che servono
//! si usa quello; altrimenti un carattere di Cedro (Carlito o il suo sostituto), ridotto ai soli glifi usati.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use subsetter::GlyphRemapper;

use super::fonts::FontInfo;
use super::geom::Geom;
use super::text::Align;
use super::Color;
use crate::pdfedit::num;
use crate::ufficio::font::{FontId, Fonti};

/// Dall'alto di un riquadro di testo alla prima linea di scrittura, e il passo tra due righe, in corpi.
pub const ASCENT: f32 = 0.9;
pub const LEADING: f32 = 1.2;

/// Come si scrive un carattere.
#[derive(Clone, Debug, PartialEq)]
pub enum FontRef {
    /// Un carattere già nella pagina (posto in `TextPage::fonts`).
    Orig(usize),
    /// Una famiglia di Cedro.
    Family(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub font: FontRef,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    Orig(usize),
    Bank(FontId),
}

#[derive(Clone, Debug)]
pub struct Seg {
    pub text: String,
    pub x: f32,
    pub w: f32,
    pub sel: Sel,
    pub size: f32,
    pub color: Color,
}

#[derive(Clone, Debug, Default)]
pub struct LineOut {
    pub segs: Vec<Seg>,
    pub width: f32,
}

struct Used {
    remap: GlyphRemapper,
    map: BTreeMap<u16, (u16, char)>,
    obj: ObjectId,
    name: String,
}

/// I caratteri che Cedro aggiunge al file: uno per famiglia e stile, ridotto ai glifi usati.
pub struct FontBank {
    pub fonti: Fonti,
    used: HashMap<FontId, Used>,
    /// Famiglie già trovate: nome → id.
    ids: HashMap<(String, bool, bool), FontId>,
}

impl Default for FontBank {
    fn default() -> Self {
        FontBank::new()
    }
}

impl FontBank {
    pub fn new() -> FontBank {
        FontBank { fonti: Fonti::new(), used: HashMap::new(), ids: HashMap::new() }
    }

    pub fn id(&mut self, family: &str, bold: bool, italic: bool) -> FontId {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(i) = self.ids.get(&key) {
            return *i;
        }
        let id = self.fonti.get(family, bold, italic);
        self.ids.insert(key, id);
        id
    }

    /// Il carattere che disegna `c` partendo da `id`: lui stesso o un sostituto che ha il simbolo.
    pub fn pick(&mut self, id: FontId, c: char) -> FontId {
        if c.is_whitespace() || self.fonti.face(id).has(c) {
            return id;
        }
        self.fonti.runs(id, &c.to_string()).first().map(|r| r.0).unwrap_or(id)
    }

    pub fn glyph_w(&self, id: FontId, c: char) -> f32 {
        let f = self.fonti.face(id);
        f.glyph(c).map(|g| g.1).unwrap_or_else(|| f.glyph_width(0))
    }

    pub fn family_of(&self, id: FontId) -> String {
        self.fonti.face(id).family.clone()
    }

    /// Il codice di `c` nel font ridotto, e il nome della risorsa; l'oggetto si crea al primo uso.
    pub fn code(&mut self, doc: &mut Document, id: FontId, c: char) -> u16 {
        let gid = self.fonti.face(id).glyph(c).map(|g| g.0).unwrap_or(0);
        let u = self.used.entry(id).or_insert_with(|| Used { remap: GlyphRemapper::new(), map: BTreeMap::new(), obj: doc.add_object(Object::Null), name: String::new() });
        let n = u.remap.remap(gid);
        if gid != 0 {
            u.map.entry(n).or_insert((gid, c));
        }
        n
    }

    pub fn resource(&mut self, doc: &mut Document, id: FontId) -> (String, ObjectId) {
        let k = self.used.len();
        let u = self.used.entry(id).or_insert_with(|| Used { remap: GlyphRemapper::new(), map: BTreeMap::new(), obj: doc.add_object(Object::Null), name: String::new() });
        if u.name.is_empty() {
            u.name = format!("CedroT{}", k + 1);
        }
        (u.name.clone(), u.obj)
    }

    /// Scrive nel file i caratteri ridotti.
    pub fn finish(self, doc: &mut Document) {
        let FontBank { fonti, used, .. } = self;
        for (id, u) in used {
            let face = fonti.face(id);
            let data = match subsetter::subset(face.data(), face.index(), &u.remap) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let tag: String = face.name.bytes().chain(u.name.bytes()).fold(0u32, |a, c| a.wrapping_mul(31).wrapping_add(c as u32)).to_string().bytes().cycle().take(6).map(|c| (b'A' + (c % 26)) as char).collect();
            let name = format!("{tag}+{}", face.name);
            let mut file = Stream::new(lopdf::dictionary! { "Length1" => data.len() as i64 }, data);
            let _ = file.compress();
            let file_id = doc.add_object(Object::Stream(file));
            let mut flags = 32;
            if face.serif {
                flags |= 2;
            }
            if face.fixed {
                flags |= 1;
            }
            if face.italic {
                flags |= 64;
            }
            let descriptor = doc.add_object(lopdf::dictionary! {
                "Type" => "FontDescriptor", "FontName" => Object::Name(name.clone().into_bytes()), "Flags" => flags as i64,
                "FontBBox" => vec![Object::Integer(face.bbox[0] as i64), Object::Integer(face.bbox[1] as i64), Object::Integer(face.bbox[2] as i64), Object::Integer(face.bbox[3] as i64)],
                "ItalicAngle" => if face.synth_italic { -12.0 } else { face.italic_angle },
                "Ascent" => face.ascent as i64, "Descent" => -(face.descent as i64), "CapHeight" => face.cap_height as i64,
                "StemV" => if face.bold { 120 } else { 80 }, "FontFile2" => Object::Reference(file_id),
            });
            let mut w: Vec<Object> = Vec::new();
            for (n, (g, _)) in &u.map {
                w.push(Object::Integer(*n as i64));
                w.push(Object::Array(vec![Object::Integer(face.glyph_width(*g).round() as i64)]));
            }
            let cid = doc.add_object(lopdf::dictionary! {
                "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => Object::Name(name.clone().into_bytes()),
                "CIDSystemInfo" => lopdf::dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
                "FontDescriptor" => Object::Reference(descriptor), "CIDToGIDMap" => "Identity", "DW" => face.glyph_width(0).round() as i64, "W" => Object::Array(w),
            });
            let mut cmap = String::from(
                "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
            );
            let entries: Vec<(&u16, &(u16, char))> = u.map.iter().filter(|(n, _)| **n != 0).collect();
            for chunk in entries.chunks(100) {
                let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
                for (n, (_, c)) in chunk {
                    let mut units = [0u16; 2];
                    let hex: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
                    let _ = writeln!(cmap, "<{n:04X}> <{hex}>");
                }
                cmap.push_str("endbfchar\n");
            }
            cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
            let mut tu = Stream::new(Dictionary::new(), cmap.into_bytes());
            let _ = tu.compress();
            let tu_id = doc.add_object(Object::Stream(tu));
            doc.objects.insert(
                u.obj,
                Object::Dictionary(lopdf::dictionary! {
                    "Type" => "Font", "Subtype" => "Type0", "BaseFont" => Object::Name(name.into_bytes()), "Encoding" => "Identity-H",
                    "DescendantFonts" => vec![Object::Reference(cid)], "ToUnicode" => Object::Reference(tu_id),
                }),
            );
        }
    }
}

/// La famiglia di Cedro più somigliante a un carattere del documento.
pub fn family_for(fi: &FontInfo) -> &'static str {
    let l = fi.base.to_ascii_lowercase();
    if l.contains("calibri") || l.contains("carlito") {
        "Carlito"
    } else if l.contains("cambria") || l.contains("caladea") {
        "Cambria"
    } else if l.contains("arial") || l.contains("helvetica") || l.contains("arimo") || l.contains("liberation sans") {
        "Arial"
    } else if l.contains("times") || l.contains("tinos") || l.contains("liberation serif") {
        "Times New Roman"
    } else if l.contains("courier") || fi.mono {
        "Courier New"
    } else if fi.serif {
        "Times New Roman"
    } else {
        "Carlito"
    }
}

/// Come uno stile scrive un carattere: con quale carattere e quanto è largo (in punti).
fn pick(c: char, st: &Style, fonts: &[FontInfo], bank: &mut FontBank) -> (Sel, f32) {
    if let FontRef::Orig(i) = &st.font {
        if let Some(fi) = fonts.get(*i) {
            if fi.obj.is_some() && fi.bold == st.bold && fi.italic == st.italic {
                if let Some(code) = fi.rev.get(&c) {
                    return (Sel::Orig(*i), fi.width(*code) * st.size / 1000.0);
                }
                if c == '\u{a0}' {
                    if let Some(code) = fi.rev.get(&' ') {
                        return (Sel::Orig(*i), fi.width(*code) * st.size / 1000.0);
                    }
                }
            }
        }
    }
    let family = match &st.font {
        FontRef::Family(f) => f.clone(),
        FontRef::Orig(i) => fonts.get(*i).map(|f| family_for(f).to_string()).unwrap_or_else(|| "Carlito".into()),
    };
    let id = bank.id(&family, st.bold, st.italic);
    let id = bank.pick(id, c);
    (Sel::Bank(id), bank.glyph_w(id, c) * st.size / 1000.0)
}

/// Impagina: va a capo alla larghezza `width` (0 = mai). I '\n' sono a capo obbligati.
pub fn layout(chars: &[(char, Style)], width: f32, fonts: &[FontInfo], bank: &mut FontBank) -> Vec<LineOut> {
    struct Item {
        c: char,
        sel: Sel,
        w: f32,
        size: f32,
        color: Color,
    }
    let mut paras: Vec<Vec<Item>> = vec![Vec::new()];
    for (c, st) in chars {
        if *c == '\n' {
            paras.push(Vec::new());
            continue;
        }
        let (sel, w) = pick(*c, st, fonts, bank);
        paras.last_mut().unwrap_or(&mut Vec::new()).push(Item { c: *c, sel, w, size: st.size, color: st.color });
    }
    let mut out: Vec<LineOut> = Vec::new();
    let flush = |items: &[Item], out: &mut Vec<LineOut>| {
        // Tra gli spazi in fondo: non contano nella larghezza.
        let mut end = items.len();
        while end > 0 && items[end - 1].c == ' ' {
            end -= 1;
        }
        let mut line = LineOut::default();
        let mut x = 0.0;
        let mut cur: Option<Seg> = None;
        for it in &items[..end] {
            match &mut cur {
                Some(s) if s.sel == it.sel && (s.size - it.size).abs() < 0.01 && s.color == it.color => {
                    s.text.push(it.c);
                    s.w += it.w;
                }
                _ => {
                    if let Some(s) = cur.take() {
                        line.segs.push(s);
                    }
                    cur = Some(Seg { text: it.c.to_string(), x, w: it.w, sel: it.sel, size: it.size, color: it.color });
                }
            }
            x += it.w;
        }
        if let Some(s) = cur.take() {
            line.segs.push(s);
        }
        line.width = x;
        out.push(line);
    };
    for items in paras {
        if width <= 0.0 {
            flush(&items, &mut out);
            continue;
        }
        let mut start = 0;
        let mut line_w = 0.0;
        let mut last_break: Option<usize> = None;
        let mut i = 0;
        while i < items.len() {
            let it = &items[i];
            if it.c == ' ' {
                last_break = Some(i);
            }
            if line_w + it.w > width + 0.01 && i > start && it.c != ' ' {
                // Si va a capo all'ultimo spazio; se non c'è, in mezzo alla parola.
                let cut = match last_break.filter(|b| *b > start) {
                    Some(b) => b + 1,
                    None => i,
                };
                flush(&items[start..cut], &mut out);
                start = cut;
                line_w = items[start..i].iter().map(|x| x.w).sum();
                last_break = None;
                continue;
            }
            line_w += it.w;
            i += 1;
        }
        flush(&items[start..], &mut out);
    }
    out
}

fn literal(bytes: &[u8]) -> String {
    let mut s = String::from("(");
    for &b in bytes {
        match b {
            b'\\' | b'(' | b')' => {
                s.push('\\');
                s.push(b as char);
            }
            32..=126 => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{b:03o}");
            }
        }
    }
    s.push(')');
    s
}

fn color_op(c: Color) -> String {
    format!("{} {} {} rg", num(c[0] as f32 / 255.0), num(c[1] as f32 / 255.0), num(c[2] as f32 / 255.0))
}

/// Le righe impaginate come operazioni del PDF. `x`, `base` è la linea della prima riga, in punti della
/// pagina vista; `box_w` e `align` danno il posto di ogni riga; `line_h` è il passo. Dà le operazioni e i
/// caratteri da registrare tra le risorse della pagina (nome → oggetto).
#[allow(clippy::too_many_arguments)]
pub fn emit(
    lines: &[LineOut],
    x: f32,
    base: f32,
    first_indent: f32,
    box_w: f32,
    align: Align,
    line_h: f32,
    geom: &Geom,
    fonts: &[FontInfo],
    bank: &mut FontBank,
    doc: &mut Document,
    res: &mut Vec<(String, ObjectId)>,
) -> String {
    let mut ops = String::new();
    for (li, line) in lines.iter().enumerate() {
        let y = base + li as f32 * line_h;
        let indent = if li == 0 { first_indent } else { 0.0 };
        let left = match align {
            Align::Left => x + indent,
            Align::Center => x + (box_w - line.width) / 2.0,
            Align::Right => x + box_w - line.width,
        };
        for seg in &line.segs {
            if seg.text.is_empty() {
                continue;
            }
            let (px, py) = geom.to_file(left + seg.x, y);
            let m = geom.m;
            let (name, shown, skew, bold_w) = match seg.sel {
                Sel::Orig(i) => {
                    let fi = &fonts[i];
                    let id = fi.obj.unwrap_or((0, 0));
                    let name = format!("CedroO{}", id.0);
                    if !res.iter().any(|(n, _)| *n == name) {
                        res.push((name.clone(), id));
                    }
                    let bytes = fi.encode(&seg.text).unwrap_or_default();
                    (name, literal(&bytes), 0.0, 0.0)
                }
                Sel::Bank(fid) => {
                    let (name, obj) = bank.resource(doc, fid);
                    if !res.iter().any(|(n, _)| *n == name) {
                        res.push((name.clone(), obj));
                    }
                    let mut hex = String::from("<");
                    for c in seg.text.chars() {
                        let n = bank.code(doc, fid, c);
                        let _ = write!(hex, "{n:04X}");
                    }
                    hex.push('>');
                    let face = bank.fonti.face(fid);
                    (name, hex, if face.synth_italic { 0.2 } else { 0.0 }, if face.synth_bold { seg.size * 0.03 } else { 0.0 })
                }
            };
            // La matrice del testo: gli assi della pagina vista (anche girata) più un'eventuale inclinazione.
            let (a, b) = (m[0], m[1]);
            let (c, d) = (m[2] + skew * m[0], m[3] + skew * m[1]);
            let _ = write!(ops, "BT {} ", color_op(seg.color));
            if bold_w > 0.0 {
                let _ = write!(ops, "{} 2 Tr {} w ", color_op(seg.color).replace(" rg", " RG"), num(bold_w));
            }
            let _ = writeln!(ops, "/{name} {} Tf {} {} {} {} {} {} Tm {shown} Tj ET", num(seg.size), num(a), num(b), num(c), num(d), num(px), num(py));
        }
    }
    ops
}

/// Aggiunge alle risorse della pagina (una copia sua, anche se erano condivise o ereditate) una voce.
pub fn add_resource(doc: &mut Document, page: ObjectId, category: &str, name: &str, value: Object) -> Result<(), String> {
    let mut res = super::text::page_resources(doc, page);
    let mut sub = match res.get(category.as_bytes()) {
        Ok(Object::Reference(r)) => doc.get_dictionary(*r).cloned().unwrap_or_default(),
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    sub.set(name, value);
    res.set(category, Object::Dictionary(sub));
    doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Resources", Object::Dictionary(res));
    Ok(())
}
