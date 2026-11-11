//! DICOM: le immagini della radiologia (anche veterinaria), lette e riscritte in Rust.
//! Qui c'è il lettore della struttura (i «tag»), la riscrittura del file e i dati che servono a
//! disegnarlo; `pixel` decodifica le immagini, `anon` toglie i dati personali.
//!
//! Le sintassi lette: Implicit e Explicit VR Little Endian, Explicit VR Big Endian, Deflated, e tutte
//! quelle compresse (i fotogrammi restano nei loro frammenti e li decodifica `pixel`).

pub mod anon;
pub mod jpeg12;
pub mod jpegls;
pub mod pixel;

use std::io::Read;

/// Come sono scritti i tag dopo l'intestazione.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Syntax {
    ImplicitLE,
    ExplicitLE,
    ExplicitBE,
}

impl Syntax {
    fn explicit(self) -> bool {
        self != Syntax::ImplicitLE
    }
    fn big(self) -> bool {
        self == Syntax::ExplicitBE
    }
}

pub const TS_IMPLICIT: &str = "1.2.840.10008.1.2";
pub const TS_EXPLICIT: &str = "1.2.840.10008.1.2.1";
pub const TS_DEFLATED: &str = "1.2.840.10008.1.2.1.99";
pub const TS_BIG: &str = "1.2.840.10008.1.2.2";

/// Il valore di un elemento.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// I byte così come sono nel file (nell'ordine dei byte della sintassi).
    Bytes(Vec<u8>),
    /// Una sequenza: ogni voce è un insieme di elementi.
    Seq(Vec<Vec<Element>>),
    /// I pixel compressi: la tabella degli offset e poi i frammenti, ognuno com'è nel file.
    Encapsulated(Vec<Vec<u8>>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub tag: u32,
    pub vr: [u8; 2],
    pub value: Value,
}

impl Element {
    pub fn bytes(&self) -> &[u8] {
        match &self.value {
            Value::Bytes(b) => b,
            _ => &[],
        }
    }
}

/// Un file DICOM letto.
#[derive(Clone, Debug)]
pub struct DicomFile {
    /// Il gruppo 0002 (sempre Explicit VR Little Endian).
    pub meta: Vec<Element>,
    pub body: Vec<Element>,
    pub syntax: Syntax,
    /// La sintassi del file (Transfer Syntax UID), senza spazi né zeri in fondo.
    pub ts: String,
}

pub const fn tag(group: u16, elem: u16) -> u32 {
    ((group as u32) << 16) | elem as u32
}

const ITEM: u32 = tag(0xFFFE, 0xE000);
const ITEM_END: u32 = tag(0xFFFE, 0xE00D);
const SEQ_END: u32 = tag(0xFFFE, 0xE0DD);
pub const PIXEL_DATA: u32 = tag(0x7FE0, 0x0010);
const UNDEFINED: u32 = 0xFFFF_FFFF;

/// I VR che in Explicit VR hanno 2 byte riservati e la lunghezza su 4 byte.
fn long_vr(vr: [u8; 2]) -> bool {
    matches!(&vr, b"OB" | b"OD" | b"OF" | b"OL" | b"OV" | b"OW" | b"SQ" | b"SV" | b"UC" | b"UN" | b"UR" | b"UT" | b"UV")
}

/// Il VR dei tag che Cedro legge o cambia, per i file in Implicit VR (dove il VR non è scritto).
pub fn dictionary_vr(t: u32) -> [u8; 2] {
    if t & 0xFFFF == 0 {
        return *b"UL";
    }
    let vr: &[u8; 2] = match t {
        0x0008_0005 | 0x0008_0008 | 0x0008_0060 | 0x0010_0040 | 0x0010_2203 | 0x0010_2298 | 0x0012_0062 | 0x0028_0004 | 0x0028_0301 | 0x0028_2114 | 0x0018_0015 => b"CS",
        0x0008_0016 | 0x0008_0018 | 0x0008_1150 | 0x0008_1155 | 0x0020_000D | 0x0020_000E | 0x0020_0052 | 0x0040_A124 | 0x0088_0140 | 0x3006_0024 | 0x0008_0014 | 0x0020_0200 => b"UI",
        0x0008_0012 | 0x0008_0020 | 0x0008_0021 | 0x0008_0022 | 0x0008_0023 | 0x0008_0024 | 0x0008_0025 | 0x0010_0030 | 0x0040_0244 | 0x0040_0002 | 0x0032_1050 | 0x0032_0032 | 0x0038_0020 => b"DA",
        0x0008_0013 | 0x0008_0030 | 0x0008_0031 | 0x0008_0032 | 0x0008_0033 | 0x0010_0032 | 0x0040_0245 => b"TM",
        0x0008_002A | 0x0018_9074 | 0x0040_A032 => b"DT",
        0x0008_0050 | 0x0008_0094 | 0x0008_1010 | 0x0010_2154 | 0x0010_2160 | 0x0010_2180 | 0x0020_0010 | 0x0040_0242 | 0x0040_0243 | 0x0040_0253 => b"SH",
        0x0008_0070 | 0x0008_0080 | 0x0008_1030 | 0x0008_103E | 0x0008_1040 | 0x0008_1080 | 0x0008_1090 | 0x0010_0020 | 0x0010_0021 | 0x0010_1000 | 0x0010_1040 | 0x0010_1080
        | 0x0010_1081 | 0x0010_1090 | 0x0010_2150 | 0x0010_2152 | 0x0010_2201 | 0x0010_2292 | 0x0010_2299 | 0x0012_0063 | 0x0018_1000 | 0x0018_1030 | 0x0032_1033
        | 0x0038_0300 | 0x0038_0400 | 0x0018_1020 | 0x0028_1055 | 0x0040_0254 | 0x0040_1001 | 0x0032_1060 => b"LO",
        0x0008_0090 | 0x0008_1048 | 0x0008_1050 | 0x0008_1060 | 0x0008_1070 | 0x0010_0010 | 0x0010_1001 | 0x0010_1005 | 0x0010_1060 | 0x0010_2297 | 0x0032_1032 | 0x0040_0006
        | 0x0070_0084 | 0x0040_A075 | 0x0040_A123 | 0x0008_009C => b"PN",
        0x0008_0081 | 0x0008_0092 | 0x0008_2111 | 0x0040_0280 => b"ST",
        0x0010_21B0 | 0x0010_4000 | 0x0020_4000 | 0x0032_4000 => b"LT",
        0x0010_1010 => b"AS",
        0x0010_1020 | 0x0010_1030 | 0x0018_0050 | 0x0018_0088 | 0x0018_1164 | 0x0028_0030 | 0x0028_1050 | 0x0028_1051 | 0x0028_1052 | 0x0028_1053 | 0x0020_0032 | 0x0020_0037
        | 0x0018_0060 | 0x0028_0034 => b"DS",
        0x0020_0011 | 0x0020_0012 | 0x0020_0013 | 0x0028_0008 | 0x0028_0009 => b"IS",
        0x0028_0002 | 0x0028_0006 | 0x0028_0010 | 0x0028_0011 | 0x0028_0100 | 0x0028_0101 | 0x0028_0102 | 0x0028_0103 | 0x0028_1101 | 0x0028_1102 | 0x0028_1103 => b"US",
        0x0028_1201..=0x0028_1203 => b"OW",
        0x0008_1110 | 0x0008_1111 | 0x0008_1115 | 0x0008_1120 | 0x0008_1140 | 0x0008_1032 | 0x0008_2112 | 0x0010_1002 | 0x0010_2202 | 0x0010_2293 | 0x0010_2294 | 0x0040_0275
        | 0x0040_0260 | 0x0040_A730 | 0x0028_3010 | 0x0008_9215 | 0x0040_0100 | 0x0018_9346 => b"SQ",
        PIXEL_DATA => b"OW",
        _ => b"UN",
    };
    *vr
}

/// Vero se i byte sembrano un DICOM: «DICM» dopo i 128 byte del preambolo, o un gruppo 0002/0008
/// scritto proprio all'inizio (i file vecchi senza preambolo).
pub fn sniff(head: &[u8]) -> bool {
    if head.get(128..132) == Some(b"DICM") {
        return true;
    }
    if head.len() < 8 {
        return false;
    }
    let group = u16::from_le_bytes([head[0], head[1]]);
    let elem = u16::from_le_bytes([head[2], head[3]]);
    // Senza preambolo: deve partire da un tag basso del gruppo 0002 o 0008, con un VR scritto o
    // una lunghezza piccola (Implicit VR).
    if (group == 0x0002 || group == 0x0008) && elem <= 0x0020 {
        let vr_ok = head[4].is_ascii_uppercase() && head[5].is_ascii_uppercase();
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        return vr_ok || len < 256;
    }
    false
}

enum Step {
    El(Element),
    ItemEnd,
    Skip,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    syntax: Syntax,
}

impl<'a> Reader<'a> {
    fn u16(&mut self) -> Option<u16> {
        let s = self.b.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(if self.syntax.big() { u16::from_be_bytes([s[0], s[1]]) } else { u16::from_le_bytes([s[0], s[1]]) })
    }
    fn u32(&mut self) -> Option<u32> {
        let s = self.b.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(if self.syntax.big() { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) })
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
    /// Il tag dei delimitatori (sempre in little endian nei file little endian, come gli altri).
    fn tag(&mut self) -> Option<u32> {
        let g = self.u16()?;
        let e = self.u16()?;
        Some(tag(g, e))
    }
    fn peek_tag(&self) -> Option<u32> {
        let mut r = Reader { b: self.b, pos: self.pos, syntax: self.syntax };
        r.tag()
    }

    /// Legge gli elementi fino a `end` (o fino a un delimitatore di voce, che consuma).
    fn elements(&mut self, end: usize, depth: u32) -> Result<Vec<Element>, String> {
        let mut out = Vec::new();
        while self.pos + 8 <= end {
            match self.one(end, depth)? {
                Step::El(e) => out.push(e),
                Step::ItemEnd => break,
                Step::Skip => {}
            }
        }
        Ok(out)
    }

    fn one(&mut self, end: usize, depth: u32) -> Result<Step, String> {
        {
            let start = self.pos;
            let Some(t) = self.tag() else { return Ok(Step::ItemEnd) };
            if t == ITEM_END {
                self.u32();
                return Ok(Step::ItemEnd);
            }
            if t == SEQ_END || t == ITEM {
                // Un delimitatore fuori posto: lo si salta.
                self.u32();
                return Ok(Step::Skip);
            }
            let (vr, len) = if self.syntax.explicit() {
                let v = self.take(2).ok_or("tag troncato")?;
                let mut vr = [v[0], v[1]];
                if !(vr[0].is_ascii_uppercase() && vr[1].is_ascii_uppercase()) {
                    // Alcuni file dicono Explicit ma hanno tag in Implicit: si torna indietro e si legge così.
                    self.pos = start + 4;
                    let len = self.u32().ok_or("tag troncato")?;
                    vr = dictionary_vr(t);
                    (vr, len)
                } else if long_vr(vr) {
                    self.take(2).ok_or("tag troncato")?;
                    (vr, self.u32().ok_or("tag troncato")?)
                } else {
                    (vr, self.u16().ok_or("tag troncato")? as u32)
                }
            } else {
                let len = self.u32().ok_or("tag troncato")?;
                (dictionary_vr(t), len)
            };
            let value = self.value(t, vr, len, end, depth)?;
            let vr = if matches!(value, Value::Seq(_)) { *b"SQ" } else { vr };
            Ok(Step::El(Element { tag: t, vr, value }))
        }
    }

    fn value(&mut self, t: u32, vr: [u8; 2], len: u32, end: usize, depth: u32) -> Result<Value, String> {
        if depth > 32 {
            return Err("sequenze troppo annidate".into());
        }
        if t == PIXEL_DATA && len == UNDEFINED {
            return self.fragments();
        }
        if len == UNDEFINED {
            // Una sequenza (o un UN che contiene una sequenza in Implicit VR).
            let saved = self.syntax;
            if &vr == b"UN" {
                self.syntax = Syntax::ImplicitLE;
            }
            let items = self.items(self.b.len(), true, depth);
            self.syntax = saved;
            return items.map(Value::Seq);
        }
        let len = len as usize;
        let stop = self.pos.checked_add(len).filter(|e| *e <= self.b.len()).ok_or("file troncato")?;
        let looks_like_items = self.b.get(self.pos..self.pos + 4).is_some_and(|h| {
            let g = if self.syntax.big() { u16::from_be_bytes([h[0], h[1]]) } else { u16::from_le_bytes([h[0], h[1]]) };
            let e = if self.syntax.big() { u16::from_be_bytes([h[2], h[3]]) } else { u16::from_le_bytes([h[2], h[3]]) };
            tag(g, e) == ITEM
        });
        if &vr == b"SQ" || (len >= 8 && looks_like_items && matches!(&vr, b"UN")) {
            let items = self.items(stop, false, depth)?;
            self.pos = stop;
            return Ok(Value::Seq(items));
        }
        let _ = end;
        let v = self.take(len).ok_or("file troncato")?.to_vec();
        Ok(Value::Bytes(v))
    }

    fn items(&mut self, stop: usize, undefined: bool, depth: u32) -> Result<Vec<Vec<Element>>, String> {
        let mut items = Vec::new();
        while self.pos + 8 <= stop {
            let t = self.tag().ok_or("sequenza troncata")?;
            let len = self.u32().ok_or("sequenza troncata")?;
            if t == SEQ_END {
                break;
            }
            if t != ITEM {
                return Err("sequenza non valida".into());
            }
            if len == UNDEFINED {
                items.push(self.elements(stop, depth + 1)?);
            } else {
                let item_end = self.pos.checked_add(len as usize).filter(|e| *e <= self.b.len()).ok_or("voce troncata")?;
                let els = self.elements(item_end, depth + 1)?;
                self.pos = item_end;
                items.push(els);
            }
        }
        let _ = undefined;
        Ok(items)
    }

    fn fragments(&mut self) -> Result<Value, String> {
        let mut frags = Vec::new();
        loop {
            let Some(t) = self.tag() else { break };
            let len = self.u32().ok_or("pixel troncati")?;
            if t == SEQ_END {
                break;
            }
            if t != ITEM {
                return Err("frammenti dei pixel non validi".into());
            }
            let n = (len as usize).min(self.b.len().saturating_sub(self.pos));
            frags.push(self.take(n).ok_or("pixel troncati")?.to_vec());
        }
        Ok(Value::Encapsulated(frags))
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let mut e = s.len();
    while e > 0 && (s[e - 1] == b' ' || s[e - 1] == 0) {
        e -= 1;
    }
    let mut b = 0;
    while b < e && s[b] == b' ' {
        b += 1;
    }
    &s[b..e]
}

/// Un testo DICOM: UTF-8 se lo è, altrimenti Latin-1.
pub fn text(b: &[u8]) -> String {
    let b = trim(b);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|c| *c as char).collect(),
    }
}

impl DicomFile {
    pub fn read(bytes: &[u8]) -> Result<DicomFile, String> {
        let (mut pos, preamble) = if bytes.get(128..132) == Some(b"DICM") { (132, true) } else { (0, false) };
        // Il gruppo 0002, se c'è, è sempre Explicit VR Little Endian.
        let mut meta = Vec::new();
        let starts_meta = bytes.get(pos..pos + 2) == Some(&[0x02, 0x00]);
        if starts_meta {
            let mut r = Reader { b: bytes, pos, syntax: Syntax::ExplicitLE };
            while r.pos + 8 <= bytes.len() && r.peek_tag().is_some_and(|t| t >> 16 == 0x0002) {
                match r.one(bytes.len(), 0)? {
                    Step::El(e) => meta.push(e),
                    Step::ItemEnd => break,
                    Step::Skip => {}
                }
            }
            pos = r.pos;
        } else if preamble {
            return Err("intestazione DICOM non valida".into());
        }
        let ts = meta.iter().find(|e| e.tag == tag(0x0002, 0x0010)).map(|e| text(e.bytes())).unwrap_or_default();
        let (syntax, deflated) = match ts.as_str() {
            TS_IMPLICIT => (Syntax::ImplicitLE, false),
            TS_BIG => (Syntax::ExplicitBE, false),
            TS_DEFLATED => (Syntax::ExplicitLE, true),
            "" => {
                // Senza intestazione: si guarda il primo tag del corpo.
                let h = bytes.get(pos..pos + 8).ok_or("file DICOM vuoto")?;
                if h[4].is_ascii_uppercase() && h[5].is_ascii_uppercase() {
                    if h[0] == 0 && h[1] != 0 {
                        (Syntax::ExplicitBE, false)
                    } else {
                        (Syntax::ExplicitLE, false)
                    }
                } else {
                    (Syntax::ImplicitLE, false)
                }
            }
            _ => (Syntax::ExplicitLE, false),
        };
        let inflated;
        let (data, start) = if deflated {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(&bytes[pos..]).read_to_end(&mut out).map_err(|e| format!("DICOM compresso non letto: {e}"))?;
            inflated = out;
            (&inflated[..], 0)
        } else {
            (bytes, pos)
        };
        let mut r = Reader { b: data, pos: start, syntax };
        let body = r.elements(data.len(), 0)?;
        if body.is_empty() {
            return Err("nessun dato nel file DICOM".into());
        }
        // Un file deflated si riscrive Explicit VR Little Endian, non compresso.
        let ts = if deflated { TS_EXPLICIT.to_string() } else if ts.is_empty() { syntax_uid(syntax).to_string() } else { ts };
        Ok(DicomFile { meta, body, syntax, ts })
    }

    pub fn open(path: &std::path::Path) -> Result<DicomFile, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        DicomFile::read(&bytes)
    }

    pub fn get(&self, t: u32) -> Option<&Element> {
        find(&self.body, t)
    }

    pub fn string(&self, t: u32) -> Option<String> {
        let e = self.get(t)?;
        let s = text(e.bytes());
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    pub fn u16(&self, t: u32) -> Option<u16> {
        let b = self.get(t)?.bytes();
        let s = b.get(0..2)?;
        Some(if self.syntax.big() { u16::from_be_bytes([s[0], s[1]]) } else { u16::from_le_bytes([s[0], s[1]]) })
    }

    /// I numeri di un DS o IS («1.5\2»).
    pub fn numbers(&self, t: u32) -> Vec<f64> {
        self.string(t).map(|s| s.split('\\').filter_map(|v| v.trim().parse::<f64>().ok()).collect()).unwrap_or_default()
    }

    pub fn number(&self, t: u32) -> Option<f64> {
        self.numbers(t).first().copied()
    }

    /// Il file scritto di nuovo: intestazione aggiornata, stessa sintassi (i deflated tornano non compressi).
    pub fn write(&self) -> Vec<u8> {
        let mut meta = self.meta.clone();
        set_meta(&mut meta, tag(0x0002, 0x0010), ui_bytes(&self.ts));
        if find(&meta, tag(0x0002, 0x0001)).is_none() {
            meta.push(Element { tag: tag(0x0002, 0x0001), vr: *b"OB", value: Value::Bytes(vec![0, 1]) });
        }
        if let Some(cls) = self.get(tag(0x0008, 0x0016)) {
            let v = cls.bytes().to_vec();
            if find(&meta, tag(0x0002, 0x0002)).is_none() {
                meta.push(Element { tag: tag(0x0002, 0x0002), vr: *b"UI", value: Value::Bytes(v) });
            }
        }
        if let Some(sop) = self.get(tag(0x0008, 0x0018)) {
            set_meta(&mut meta, tag(0x0002, 0x0003), sop.bytes().to_vec());
        }
        if find(&meta, tag(0x0002, 0x0012)).is_none() {
            meta.push(Element { tag: tag(0x0002, 0x0012), vr: *b"UI", value: Value::Bytes(ui_bytes("1.2.826.0.1.3680043.10.1561.1")) });
        }
        meta.retain(|e| e.tag != tag(0x0002, 0x0000));
        meta.sort_by_key(|e| e.tag);
        let mut group = Vec::new();
        for e in &meta {
            write_element(&mut group, e, Syntax::ExplicitLE);
        }
        let mut out = vec![0u8; 128];
        out.extend_from_slice(b"DICM");
        write_element(&mut out, &Element { tag: tag(0x0002, 0x0000), vr: *b"UL", value: Value::Bytes((group.len() as u32).to_le_bytes().to_vec()) }, Syntax::ExplicitLE);
        out.extend_from_slice(&group);
        for e in &self.body {
            write_element(&mut out, e, self.syntax);
        }
        out
    }
}

pub fn syntax_uid(s: Syntax) -> &'static str {
    match s {
        Syntax::ImplicitLE => TS_IMPLICIT,
        Syntax::ExplicitLE => TS_EXPLICIT,
        Syntax::ExplicitBE => TS_BIG,
    }
}

fn set_meta(meta: &mut Vec<Element>, t: u32, v: Vec<u8>) {
    match meta.iter_mut().find(|e| e.tag == t) {
        Some(e) => e.value = Value::Bytes(v),
        None => meta.push(Element { tag: t, vr: *b"UI", value: Value::Bytes(v) }),
    }
}

pub fn find(list: &[Element], t: u32) -> Option<&Element> {
    list.iter().find(|e| e.tag == t)
}

/// Un UID come byte, con lo zero finale se la lunghezza è dispari.
pub fn ui_bytes(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if v.len() % 2 == 1 {
        v.push(0);
    }
    v
}

/// Un testo come byte, con lo spazio finale se la lunghezza è dispari.
pub fn text_bytes(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if v.len() % 2 == 1 {
        v.push(b' ');
    }
    v
}

fn put16(out: &mut Vec<u8>, v: u16, s: Syntax) {
    out.extend_from_slice(&if s.big() { v.to_be_bytes() } else { v.to_le_bytes() });
}

fn put32(out: &mut Vec<u8>, v: u32, s: Syntax) {
    out.extend_from_slice(&if s.big() { v.to_be_bytes() } else { v.to_le_bytes() });
}

fn put_tag(out: &mut Vec<u8>, t: u32, s: Syntax) {
    put16(out, (t >> 16) as u16, s);
    put16(out, t as u16, s);
}

fn write_element(out: &mut Vec<u8>, e: &Element, s: Syntax) {
    put_tag(out, e.tag, s);
    match &e.value {
        Value::Bytes(b) => {
            let mut b = b.clone();
            if b.len() % 2 == 1 {
                b.push(if matches!(&e.vr, b"UI" | b"OB" | b"UN") { 0 } else { b' ' });
            }
            if s.explicit() {
                let vr = if &e.vr == b"SQ" { *b"UN" } else { e.vr };
                out.extend_from_slice(&vr);
                if long_vr(vr) {
                    out.extend_from_slice(&[0, 0]);
                    put32(out, b.len() as u32, s);
                } else if b.len() > 0xFFFF {
                    // Troppo lungo per un VR corto: lo si scrive come UN, che ha la lunghezza lunga.
                    let at = out.len() - 2;
                    out[at..].copy_from_slice(b"UN");
                    out.extend_from_slice(&[0, 0]);
                    put32(out, b.len() as u32, s);
                } else {
                    put16(out, b.len() as u16, s);
                }
            } else {
                put32(out, b.len() as u32, s);
            }
            out.extend_from_slice(&b);
        }
        Value::Seq(items) => {
            if s.explicit() {
                out.extend_from_slice(b"SQ");
                out.extend_from_slice(&[0, 0]);
            }
            put32(out, UNDEFINED, s);
            for item in items {
                put_tag(out, ITEM, s);
                put32(out, UNDEFINED, s);
                for el in item {
                    write_element(out, el, s);
                }
                put_tag(out, ITEM_END, s);
                put32(out, 0, s);
            }
            put_tag(out, SEQ_END, s);
            put32(out, 0, s);
        }
        Value::Encapsulated(frags) => {
            if s.explicit() {
                out.extend_from_slice(b"OB");
                out.extend_from_slice(&[0, 0]);
            }
            put32(out, UNDEFINED, s);
            for f in frags {
                put_tag(out, ITEM, s);
                let mut f = f.clone();
                if f.len() % 2 == 1 {
                    f.push(0);
                }
                put32(out, f.len() as u32, s);
                out.extend_from_slice(&f);
            }
            put_tag(out, SEQ_END, s);
            put32(out, 0, s);
        }
    }
}

/// I dati che Cedro mostra di un esame (anteprima e messaggi): senza dati personali.
pub fn summary(f: &DicomFile) -> String {
    let modality = f.string(tag(0x0008, 0x0060)).unwrap_or_default();
    let species = f.string(tag(0x0010, 0x2201)).unwrap_or_default();
    let body = f.string(tag(0x0018, 0x0015)).unwrap_or_default();
    let rows = f.u16(tag(0x0028, 0x0010)).unwrap_or(0);
    let cols = f.u16(tag(0x0028, 0x0011)).unwrap_or(0);
    let frames = f.number(tag(0x0028, 0x0008)).unwrap_or(1.0) as u32;
    let mut parts: Vec<String> = Vec::new();
    if !modality.is_empty() {
        parts.push(modality_name(&modality).to_string());
    }
    if !body.is_empty() {
        parts.push(body.to_lowercase());
    }
    if !species.is_empty() {
        parts.push(species);
    }
    if cols > 0 && rows > 0 {
        parts.push(format!("{cols}×{rows}"));
    }
    if frames > 1 {
        parts.push(format!("{frames} fotogrammi"));
    }
    parts.join(" · ")
}

pub fn modality_name(m: &str) -> &str {
    match m {
        "CT" => "TC",
        "MR" => "RM",
        "CR" | "DX" | "RG" => "RX",
        "US" => "Ecografia",
        "MG" => "Mammografia",
        "XA" | "RF" => "Fluoroscopia",
        "NM" => "Medicina nucleare",
        "PT" => "PET",
        "SM" => "Vetrino",
        "OP" | "XC" | "ES" => "Foto",
        "SC" | "OT" => "Immagine",
        other => other,
    }
}

#[cfg(test)]
pub(crate) mod prove {
    use super::*;

    pub fn el(t: u32, vr: &[u8; 2], v: &[u8]) -> Element {
        let mut b = v.to_vec();
        if b.len() % 2 == 1 {
            b.push(if vr == b"UI" { 0 } else { b' ' });
        }
        Element { tag: t, vr: *vr, value: Value::Bytes(b) }
    }

    /// Un piccolo DICOM in scala di grigi a 16 bit con segno (come una TC), in Implicit VR.
    pub fn ct(rows: u16, cols: u16, values: &[i16], syntax: Syntax) -> Vec<u8> {
        let mut px = Vec::new();
        for v in values {
            px.extend_from_slice(&if syntax.big() { v.to_be_bytes() } else { v.to_le_bytes() });
        }
        let u16b = |v: u16| if syntax.big() { v.to_be_bytes().to_vec() } else { v.to_le_bytes().to_vec() };
        let body = vec![
            el(tag(0x0008, 0x0016), b"UI", b"1.2.840.10008.5.1.4.1.1.2"),
            el(tag(0x0008, 0x0018), b"UI", b"1.2.3.4.5.6.7"),
            el(tag(0x0008, 0x0020), b"DA", b"20240315"),
            el(tag(0x0008, 0x0060), b"CS", b"CT"),
            el(tag(0x0008, 0x0080), b"LO", b"Clinica Veterinaria San Rocco"),
            el(tag(0x0008, 0x0090), b"PN", b"Bianchi^Laura"),
            el(tag(0x0010, 0x0010), b"PN", b"Fido^Rossi"),
            el(tag(0x0010, 0x0020), b"LO", b"MC380260000123456"),
            el(tag(0x0010, 0x0030), b"DA", b"20170210"),
            el(tag(0x0010, 0x0040), b"CS", b"M"),
            el(tag(0x0010, 0x2201), b"LO", b"Canine"),
            el(tag(0x0010, 0x2292), b"LO", b"Labrador Retriever"),
            el(tag(0x0010, 0x2297), b"PN", b"Rossi^Mario"),
            el(tag(0x0010, 0x2299), b"LO", b"Allevamento del Po"),
            Element {
                tag: tag(0x0008, 0x1120),
                vr: *b"SQ",
                value: Value::Seq(vec![vec![el(tag(0x0008, 0x1150), b"UI", b"1.2.3"), el(tag(0x0008, 0x1155), b"UI", b"1.2.3.4.5.6.99")]]),
            },
            el(tag(0x0009, 0x0010), b"LO", b"PRIVATO"),
            el(tag(0x0009, 0x1001), b"LO", b"Rossi Mario 333 1234567"),
            el(tag(0x0020, 0x000D), b"UI", b"1.2.3.4.5"),
            el(tag(0x0020, 0x000E), b"UI", b"1.2.3.4.5.6"),
            Element { tag: tag(0x0028, 0x0002), vr: *b"US", value: Value::Bytes(u16b(1)) },
            el(tag(0x0028, 0x0004), b"CS", b"MONOCHROME2"),
            Element { tag: tag(0x0028, 0x0010), vr: *b"US", value: Value::Bytes(u16b(rows)) },
            Element { tag: tag(0x0028, 0x0011), vr: *b"US", value: Value::Bytes(u16b(cols)) },
            el(tag(0x0028, 0x0030), b"DS", b"0.5\\0.5"),
            Element { tag: tag(0x0028, 0x0100), vr: *b"US", value: Value::Bytes(u16b(16)) },
            Element { tag: tag(0x0028, 0x0101), vr: *b"US", value: Value::Bytes(u16b(16)) },
            Element { tag: tag(0x0028, 0x0102), vr: *b"US", value: Value::Bytes(u16b(15)) },
            Element { tag: tag(0x0028, 0x0103), vr: *b"US", value: Value::Bytes(u16b(1)) },
            el(tag(0x0028, 0x1050), b"DS", b"40"),
            el(tag(0x0028, 0x1051), b"DS", b"400"),
            el(tag(0x0028, 0x1052), b"DS", b"0"),
            el(tag(0x0028, 0x1053), b"DS", b"1"),
            Element { tag: PIXEL_DATA, vr: *b"OW", value: Value::Bytes(px) },
        ];
        let meta = vec![
            el(tag(0x0002, 0x0002), b"UI", b"1.2.840.10008.5.1.4.1.1.2"),
            el(tag(0x0002, 0x0003), b"UI", b"1.2.3.4.5.6.7"),
        ];
        DicomFile { meta, body, syntax, ts: syntax_uid(syntax).into() }.write()
    }

    #[test]
    fn si_riscrive_uguale_in_tutte_le_sintassi() {
        for s in [Syntax::ImplicitLE, Syntax::ExplicitLE, Syntax::ExplicitBE] {
            let bytes = ct(2, 2, &[-1000, 0, 40, 3000], s);
            assert!(sniff(&bytes));
            let f = DicomFile::read(&bytes).unwrap();
            assert_eq!(f.syntax, s);
            assert_eq!(f.string(tag(0x0010, 0x0010)).as_deref(), Some("Fido^Rossi"));
            assert_eq!(f.u16(tag(0x0028, 0x0010)), Some(2));
            assert_eq!(f.numbers(tag(0x0028, 0x0030)), vec![0.5, 0.5]);
            let seq = f.get(tag(0x0008, 0x1120)).unwrap();
            assert!(matches!(&seq.value, Value::Seq(items) if items.len() == 1 && items[0].len() == 2));
            let again = DicomFile::read(&f.write()).unwrap();
            assert_eq!(again.body, f.body);
        }
    }

    #[test]
    fn un_file_troncato_non_fa_cadere_il_programma() {
        let bytes = ct(4, 4, &[0; 16], Syntax::ExplicitLE);
        for cut in [10, 130, 140, 200, bytes.len() - 7] {
            let _ = DicomFile::read(&bytes[..cut]);
        }
    }
}
