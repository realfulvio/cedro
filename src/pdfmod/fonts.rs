//! Come un carattere del PDF si legge: le larghezze dei glifi, che lettera è ogni codice e come si scrive
//! un testo nuovo con lo stesso carattere (se ha le lettere che servono).

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// Un carattere (font) di una pagina.
#[derive(Clone, Debug, Default)]
pub struct FontInfo {
    /// Il nome della risorsa nella pagina («F1»).
    pub key: Vec<u8>,
    /// L'oggetto del carattere nel file (per riusarlo in un testo nuovo).
    pub obj: Option<ObjectId>,
    /// Il nome senza il prefisso dei sottoinsiemi.
    pub base: String,
    /// Due byte per codice (Type0), altrimenti uno.
    pub two_byte: bool,
    /// Larghezza di ogni codice, in millesimi del corpo.
    pub widths: HashMap<u32, f32>,
    pub default_w: f32,
    /// Che testo è ogni codice.
    pub uni: HashMap<u32, String>,
    /// Il codice con cui si scrive un carattere.
    pub rev: HashMap<char, u32>,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub mono: bool,
    pub embedded: bool,
    /// Misure verticali in millesimi: sopra la riga e sotto (negativo).
    pub ascent: f32,
    pub descent: f32,
    /// Sappiamo che lettera è ogni codice (altrimenti il testo non si può riscrivere).
    pub known: bool,
    /// Scrittura verticale (non si modifica).
    pub vertical: bool,
}

pub fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// L'oggetto, seguendo i rimandi.
pub fn deref<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    doc.dereference(o).ok().map(|(_, o)| o)
}

pub fn dict_of<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, o)? {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

pub fn name_of(o: &Object) -> Option<String> {
    o.as_name().ok().map(|n| String::from_utf8_lossy(n).into_owned())
}

fn strip_subset(name: &str) -> &str {
    match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => rest,
        _ => name,
    }
}

/// Un numero esadecimale `<00A1>` come numero e come numero di byte.
fn hex_tok(t: &str) -> Option<(u32, usize)> {
    let h = t.strip_prefix('<')?.strip_suffix('>')?;
    if h.is_empty() || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((u32::from_str_radix(&h[..h.len().min(8)], 16).ok()?, h.len().div_ceil(2)))
}

fn utf16_hex(t: &str) -> Option<String> {
    let h = t.strip_prefix('<')?.strip_suffix('>')?;
    if h.len() % 4 != 0 && h.len() != 2 {
        return None;
    }
    if h.len() == 2 {
        return u8::from_str_radix(h, 16).ok().map(|b| (b as char).to_string());
    }
    let units: Vec<u16> = (0..h.len()).step_by(4).filter_map(|i| u16::from_str_radix(&h[i..i + 4], 16).ok()).collect();
    Some(String::from_utf16_lossy(&units))
}

/// Il ToUnicode: codice → testo, e quanti byte ha il codice se lo dice.
pub fn parse_to_unicode(data: &[u8]) -> (HashMap<u32, String>, Option<usize>) {
    let text = String::from_utf8_lossy(data);
    let toks: Vec<&str> = text.split_whitespace().collect();
    let mut map = HashMap::new();
    let mut width = None;
    let mut i = 0;
    while i < toks.len() {
        match toks[i] {
            "begincodespacerange" => {
                i += 1;
                while i < toks.len() && toks[i] != "endcodespacerange" {
                    if let Some((_, n)) = hex_tok(toks[i]) {
                        width = Some(n);
                    }
                    i += 1;
                }
            }
            "beginbfchar" => {
                i += 1;
                while i + 1 < toks.len() && toks[i] != "endbfchar" {
                    if let (Some((code, _)), Some(s)) = (hex_tok(toks[i]), utf16_hex(toks[i + 1])) {
                        map.insert(code, s);
                    }
                    i += 2;
                }
            }
            "beginbfrange" => {
                i += 1;
                while i + 2 < toks.len() && toks[i] != "endbfrange" {
                    let (Some((lo, _)), Some((hi, _))) = (hex_tok(toks[i]), hex_tok(toks[i + 1])) else {
                        i += 1;
                        continue;
                    };
                    if toks[i + 2] == "[" || toks[i + 2].starts_with('[') {
                        // [<d1> <d2> …]
                        let mut j = i + 2;
                        let mut code = lo;
                        let first = toks[j].trim_start_matches('[');
                        if let Some(s) = utf16_hex(first) {
                            map.insert(code, s);
                            code += 1;
                        }
                        j += 1;
                        while j < toks.len() && code <= hi {
                            let t = toks[j].trim_end_matches(']');
                            if let Some(s) = utf16_hex(t) {
                                map.insert(code, s);
                                code += 1;
                            }
                            if toks[j].ends_with(']') {
                                j += 1;
                                break;
                            }
                            j += 1;
                        }
                        i = j;
                    } else {
                        if let Some(s) = utf16_hex(toks[i + 2]) {
                            if hi >= lo && hi - lo < 0x10000 {
                                let mut chars: Vec<char> = s.chars().collect();
                                for code in lo..=hi {
                                    map.insert(code, chars.iter().collect());
                                    if let Some(last) = chars.last_mut() {
                                        *last = char::from_u32(*last as u32 + 1).unwrap_or(*last);
                                    }
                                }
                            }
                        }
                        i += 3;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    (map, width)
}

fn stream_bytes(doc: &Document, o: &Object) -> Option<Vec<u8>> {
    match deref(doc, o)? {
        Object::Stream(s) => s.decompressed_content().ok().or_else(|| Some(s.content.clone())),
        _ => None,
    }
}

/// Larghezze di Helvetica (millesimi) per un carattere, quando il PDF non le dice.
fn std_width(base: &str, c: char) -> f32 {
    let l = base.to_ascii_lowercase();
    if l.contains("courier") || l.contains("mono") {
        return 600.0;
    }
    let w = crate::pdfedit::text_width(&c.to_string(), l.contains("bold")) / 1.0;
    if l.contains("times") {
        w * 0.9
    } else {
        w
    }
}

pub fn load(doc: &Document, dict: &Dictionary, key: &[u8], obj: Option<ObjectId>) -> FontInfo {
    let mut f = FontInfo { key: key.to_vec(), obj, default_w: 0.0, known: true, ..Default::default() };
    let subtype = dict.get(b"Subtype").ok().and_then(name_of).unwrap_or_default();
    f.base = strip_subset(&dict.get(b"BaseFont").ok().and_then(name_of).unwrap_or_default()).to_string();
    let lower = f.base.to_ascii_lowercase();
    f.bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy") || lower.contains("semibold");
    f.italic = lower.contains("italic") || lower.contains("oblique");
    f.mono = lower.contains("courier") || lower.contains("mono") || lower.contains("consolas");
    f.serif = ["times", "serif", "georgia", "garamond", "cambria", "palatino", "book", "minion"].iter().any(|k| lower.contains(k)) && !lower.contains("sans");

    let to_unicode = dict.get(b"ToUnicode").ok().and_then(|o| stream_bytes(doc, o)).map(|d| parse_to_unicode(&d));
    let mut desc: Option<&Dictionary> = dict.get(b"FontDescriptor").ok().and_then(|o| dict_of(doc, o));
    let mut descendant: Option<&Dictionary> = None;
    if subtype == "Type0" {
        f.two_byte = true;
        descendant = dict.get(b"DescendantFonts").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_array().ok()).and_then(|a| a.first()).and_then(|o| dict_of(doc, o));
        if let Some(d) = descendant {
            desc = d.get(b"FontDescriptor").ok().and_then(|o| dict_of(doc, o)).or(desc);
        }
        if let Some(Object::Name(n)) = dict.get(b"Encoding").ok() {
            let n = String::from_utf8_lossy(n);
            f.vertical = n.ends_with("-V");
        }
        if let Some((_, Some(w))) = &to_unicode {
            f.two_byte = *w >= 2;
        }
    }
    if let Some(d) = desc {
        let flags = d.get(b"Flags").ok().and_then(num).unwrap_or(0.0) as u32;
        if flags & 1 != 0 {
            f.mono = true;
        }
        if flags & 2 != 0 {
            f.serif = true;
        }
        if flags & 64 != 0 {
            f.italic = true;
        }
        if flags & 0x40000 != 0 {
            f.bold = true;
        }
        f.ascent = d.get(b"Ascent").ok().and_then(num).unwrap_or(0.0);
        f.descent = d.get(b"Descent").ok().and_then(num).unwrap_or(0.0);
        f.embedded = d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3");
        if let Some(mw) = d.get(b"MissingWidth").ok().and_then(num) {
            f.default_w = mw;
        }
    }
    if f.ascent <= 0.0 || f.ascent > 1500.0 {
        f.ascent = 800.0;
    }
    if f.descent >= 0.0 || f.descent < -800.0 {
        f.descent = -210.0;
    }

    // Larghezze.
    if let Some(cid) = descendant {
        f.default_w = cid.get(b"DW").ok().and_then(num).unwrap_or(1000.0);
        if let Some(w) = cid.get(b"W").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_array().ok()) {
            let mut i = 0;
            while i < w.len() {
                let Some(first) = num(&w[i]) else {
                    i += 1;
                    continue;
                };
                match w.get(i + 1).and_then(|o| deref(doc, o)) {
                    Some(Object::Array(list)) => {
                        for (k, v) in list.iter().enumerate() {
                            if let Some(v) = deref(doc, v).and_then(num) {
                                f.widths.insert(first as u32 + k as u32, v);
                            }
                        }
                        i += 2;
                    }
                    Some(last) if num(last).is_some() => {
                        let (last, width) = (num(last).unwrap_or(first), w.get(i + 2).and_then(num).unwrap_or(f.default_w));
                        if last >= first && last - first < 70000.0 {
                            for c in first as u32..=last as u32 {
                                f.widths.insert(c, width);
                            }
                        }
                        i += 3;
                    }
                    _ => i += 1,
                }
            }
        }
    } else {
        let first = dict.get(b"FirstChar").ok().and_then(num).unwrap_or(0.0) as u32;
        if let Some(w) = dict.get(b"Widths").ok().and_then(|o| deref(doc, o)).and_then(|o| o.as_array().ok()) {
            for (k, v) in w.iter().enumerate() {
                if let Some(v) = deref(doc, v).and_then(num) {
                    f.widths.insert(first + k as u32, v);
                }
            }
        }
    }

    // Che lettera è ogni codice.
    if let Some((map, _)) = &to_unicode {
        f.uni = map.clone();
    }
    if !f.two_byte {
        if let Ok(enc) = dict.get_font_encoding(doc) {
            for code in 0u32..256 {
                if f.uni.contains_key(&code) {
                    continue;
                }
                if let Ok(s) = enc.bytes_to_string(&[code as u8]) {
                    if !s.is_empty() && s != "\u{FFFD}" && s.chars().all(|c| c != '\0') {
                        f.uni.insert(code, s);
                    }
                }
            }
        }
        // Senza Widths e senza descrittore: i caratteri standard (Helvetica, Courier…).
        if f.widths.is_empty() {
            for (code, s) in &f.uni {
                if let Some(c) = s.chars().next() {
                    f.widths.insert(*code, std_width(&f.base, c));
                }
            }
            if f.widths.is_empty() {
                f.default_w = 556.0;
            }
        }
    } else if f.uni.is_empty() {
        // Identity senza ToUnicode: non si sa che lettera sia.
        f.known = false;
    }
    for (code, s) in &f.uni {
        let mut it = s.chars();
        if let (Some(c), None) = (it.next(), it.next()) {
            f.rev.entry(c).and_modify(|old| *old = (*old).min(*code)).or_insert(*code);
        }
    }
    f
}

impl FontInfo {
    /// Quanti byte ha un codice.
    pub fn code_len(&self) -> usize {
        if self.two_byte {
            2
        } else {
            1
        }
    }

    pub fn width(&self, code: u32) -> f32 {
        self.widths.get(&code).copied().unwrap_or(self.default_w)
    }

    /// Il testo di un codice (quadratino se non si sa).
    pub fn text_of(&self, code: u32) -> String {
        self.uni.get(&code).cloned().unwrap_or_else(|| if code == 32 && !self.two_byte { " ".into() } else { "\u{FFFD}".into() })
    }

    /// Si può scrivere tutto questo testo con questo carattere? Dà i byte da mettere tra parentesi.
    pub fn encode(&self, text: &str) -> Option<Vec<u8>> {
        if !self.known || self.vertical {
            return None;
        }
        let mut out = Vec::new();
        for c in text.chars() {
            let code = *self.rev.get(&c)?;
            if self.two_byte {
                out.extend_from_slice(&(code as u16).to_be_bytes());
            } else {
                out.push(code as u8);
            }
        }
        Some(out)
    }

    /// Larghezza in punti di un testo (solo se tutte le lettere si conoscono).
    pub fn text_width(&self, text: &str, size: f32) -> Option<f32> {
        let mut w = 0.0;
        for c in text.chars() {
            w += self.width(*self.rev.get(&c)?);
        }
        Some(w * size / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_unicode_map() {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n2 beginbfchar\n<0003> <0020>\n<0024> <0041>\nendbfchar\n1 beginbfrange\n<0030> <0032> <0061>\nendbfrange\n1 beginbfrange\n<0040> <0041> [<0062> <00630064>]\nendbfrange endcmap";
        let (m, w) = parse_to_unicode(cmap);
        assert_eq!(w, Some(2));
        assert_eq!(m[&0x24], "A");
        assert_eq!(m[&0x31], "b");
        assert_eq!(m[&0x32], "c");
        assert_eq!(m[&0x40], "b");
        assert_eq!(m[&0x41], "cd");
    }
}
