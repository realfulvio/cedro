//! Il vecchio formato di Excel (.xls, BIFF8, Excel 97-2003): valori, formati dei numeri, caratteri,
//! sfondi, bordi, allineamenti, larghezze e altezze, celle unite, collegamenti, immagini e le
//! impostazioni di stampa (area, titoli, carta, scala, margini, intestazione e piè, salti di pagina).

use std::collections::HashMap;
use std::io::Read;

use super::foglio::{indexed_color, tint, Bordo, Carattere, Cartella, Cella, Foglio, HAlign, Stile, VAlign, Valore, MAX_COLS, MAX_ROWS};
use super::pagina::{Dash, Rgb};

/// Un record BIFF con i CONTINUE che lo seguono, come pezzi separati (le stringhe ne tengono conto).
struct Record<'a> {
    kind: u16,
    parts: Vec<&'a [u8]>,
}

impl Record<'_> {
    fn data(&self) -> Vec<u8> {
        self.parts.concat()
    }
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    b.get(i..i + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]))
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn f64_at(b: &[u8], i: usize) -> f64 {
    b.get(i..i + 8).map_or(0.0, |x| f64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
}

/// I record di un flusso a partire da `pos`, fino all'EOF del sottoflusso (i BOF annidati, come i
/// grafici dentro un foglio, si saltano interi).
fn records(s: &[u8], mut pos: usize) -> Vec<Record<'_>> {
    let mut out: Vec<Record> = Vec::new();
    let mut depth = 0i32;
    while pos + 4 <= s.len() {
        let kind = u16_at(s, pos);
        let len = u16_at(s, pos + 2) as usize;
        let data = &s[pos + 4..(pos + 4 + len).min(s.len())];
        pos += 4 + len;
        match kind {
            0x0809 => {
                depth += 1;
                if depth > 1 {
                    continue;
                }
            }
            0x000A => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
                continue;
            }
            _ if depth > 1 => continue,
            // CONTINUE (e il CONTINUE dei disegni) si attacca al record prima.
            0x003C if !out.is_empty() => {
                out.last_mut().unwrap().parts.push(data);
                continue;
            }
            _ => {}
        }
        out.push(Record { kind, parts: vec![data] });
    }
    out
}

/// Legge stringhe che possono passare da un pezzo all'altro: al cambio di pezzo dentro i caratteri
/// c'è un byte nuovo che dice se sono a 8 o a 16 bit.
struct Reader<'a> {
    parts: &'a [&'a [u8]],
    part: usize,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(parts: &'a [&'a [u8]]) -> Self {
        Reader { parts, part: 0, pos: 0 }
    }

    fn next_part(&mut self) -> bool {
        if self.part + 1 < self.parts.len() {
            self.part += 1;
            self.pos = 0;
            true
        } else {
            false
        }
    }

    fn byte(&mut self) -> Option<u8> {
        loop {
            let p = self.parts.get(self.part)?;
            if self.pos < p.len() {
                self.pos += 1;
                return Some(p[self.pos - 1]);
            }
            if !self.next_part() {
                return None;
            }
        }
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }

    fn skip(&mut self, mut n: usize) {
        while n > 0 {
            let Some(p) = self.parts.get(self.part) else { return };
            let left = p.len() - self.pos;
            if left >= n {
                self.pos += n;
                return;
            }
            n -= left;
            if !self.next_part() {
                return;
            }
        }
    }

    /// I caratteri di una stringa: `cch` caratteri, il byte delle opzioni già letto.
    fn chars(&mut self, cch: usize, mut high: bool) -> String {
        let mut units: Vec<u16> = Vec::with_capacity(cch.min(32_768));
        while units.len() < cch {
            let Some(p) = self.parts.get(self.part) else { break };
            if self.pos >= p.len() {
                if !self.next_part() {
                    break;
                }
                // Il pezzo nuovo comincia con le opzioni dei caratteri che restano.
                let Some(f) = self.byte() else { break };
                high = f & 1 != 0;
                continue;
            }
            if high {
                let (Some(a), Some(b)) = (self.byte(), self.byte()) else { break };
                units.push(u16::from_le_bytes([a, b]));
            } else {
                let Some(a) = self.byte() else { break };
                units.push(a as u16);
            }
        }
        String::from_utf16_lossy(&units)
    }

    /// XLUnicodeRichExtendedString (la tabella delle stringhe).
    fn rich_string(&mut self) -> Option<String> {
        let cch = self.u16()? as usize;
        let flags = self.byte()?;
        let runs = if flags & 8 != 0 { self.u16()? as usize } else { 0 };
        let ext = if flags & 4 != 0 { self.u32()? as usize } else { 0 };
        let s = self.chars(cch, flags & 1 != 0);
        self.skip(runs * 4 + ext);
        Some(s)
    }
}

/// XLUnicodeString (lunghezza a 16 bit) dentro un record.
fn unicode_string(b: &[u8], i: usize) -> (String, usize) {
    let cch = u16_at(b, i) as usize;
    let parts = [b.get(i + 2..).unwrap_or(&[])];
    let mut r = Reader::new(&parts);
    let flags = r.byte().unwrap_or(0);
    let s = r.chars(cch, flags & 1 != 0);
    let used = 3 + cch * if flags & 1 != 0 { 2 } else { 1 };
    (s, used)
}

/// ShortXLUnicodeString (lunghezza a 8 bit).
fn short_string(b: &[u8], i: usize) -> String {
    let cch = b.get(i).copied().unwrap_or(0) as usize;
    let parts = [b.get(i + 1..).unwrap_or(&[])];
    let mut r = Reader::new(&parts);
    let flags = r.byte().unwrap_or(0);
    r.chars(cch, flags & 1 != 0)
}

fn rk(v: u32) -> f64 {
    let x = if v & 2 != 0 { ((v as i32) >> 2) as f64 } else { f64::from_bits(((v & 0xFFFF_FFFC) as u64) << 32) };
    if v & 1 != 0 {
        x / 100.0
    } else {
        x
    }
}

struct Palette(Vec<Rgb>);

impl Palette {
    fn get(&self, i: u16) -> Option<Rgb> {
        match i {
            0..=7 => indexed_color(i as usize),
            8..=63 => self.0.get(i as usize - 8).copied().or_else(|| indexed_color(i as usize)),
            _ => None,
        }
    }
}

struct Xf {
    font: u16,
    fmt: u16,
    parent: u16,
    style: bool,
    used: u8,
    align: u8,
    rot: u8,
    indent: u8,
    border1: u32,
    border2: u32,
    fill: u16,
}

fn border(dg: u32, icv: u32, pal: &Palette) -> Option<Bordo> {
    let (width, dash, double) = match dg {
        0 => return None,
        1 => (0.5, Dash::Solid, false),
        2 => (1.0, Dash::Solid, false),
        3 => (0.5, Dash::Dashed, false),
        4 => (0.5, Dash::Dotted, false),
        5 => (1.5, Dash::Solid, false),
        6 => (0.5, Dash::Solid, true),
        7 => (0.25, Dash::Solid, false),
        8 | 10 | 12 | 13 => (1.0, Dash::Dashed, false),
        _ => (0.5, Dash::Dashed, false),
    };
    Some(Bordo { width, color: pal.get(icv as u16).unwrap_or((0, 0, 0)), dash, double })
}

/// Le immagini della cartella (BLIP del gruppo di disegni), nell'ordine dei numeri «pib».
fn blips(data: &[u8]) -> Vec<Option<Vec<u8>>> {
    let mut out = Vec::new();
    fn walk(d: &[u8], out: &mut Vec<Option<Vec<u8>>>) {
        let mut i = 0;
        while i + 8 <= d.len() {
            let verinst = u16_at(d, i);
            let kind = u16_at(d, i + 2);
            let len = u32_at(d, i + 4) as usize;
            let body = &d[i + 8..(i + 8 + len).min(d.len())];
            match kind {
                0xF000 | 0xF001 => walk(body, out),
                0xF007 => {
                    // FBSE: 36 byte di intestazione e il nome, poi (se c'è) la BLIP vera.
                    let name = body.get(33).copied().unwrap_or(0) as usize;
                    let blip = body.get(36 + name..).unwrap_or(&[]);
                    out.push(blip_bytes(blip));
                }
                _ => {}
            }
            let _ = verinst;
            i += 8 + len;
        }
    }
    walk(data, &mut out);
    out
}

/// I byte di un'immagine da una BLIP di Office: PNG e JPEG come sono, il DIB con l'intestazione del BMP.
fn blip_bytes(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 8 {
        return None;
    }
    let inst = u16_at(b, 0) >> 4;
    let kind = u16_at(b, 2);
    let len = u32_at(b, 4) as usize;
    let body = b.get(8..(8 + len).min(b.len()))?;
    let (uids, tag) = match (kind, inst) {
        (0xF01D | 0xF02A, 0x46A | 0x6E2) | (0xF01E, 0x6E0) | (0xF01F, 0x7A8) | (0xF029, 0x6E4) => (16, 1),
        (0xF01D | 0xF02A, 0x46B | 0x6E3) | (0xF01E, 0x6E1) | (0xF01F, 0x7A9) | (0xF029, 0x6E5) => (32, 1),
        // I metafile (EMF, WMF, PICT) non si disegnano: restano fuori.
        _ => return None,
    };
    let data = body.get(uids + tag..)?;
    if kind == 0xF01F {
        // DIB: si aggiunge l'intestazione del file BMP.
        let header_size = u32_at(data, 0) as usize;
        let bits = u16_at(data, 14) as usize;
        let colors = if bits <= 8 { u32_at(data, 32).max(if u32_at(data, 32) == 0 { 1 << bits } else { 0 }) as usize } else { 0 };
        let off = 14 + header_size + colors * 4;
        let mut bmp = Vec::with_capacity(data.len() + 14);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&((data.len() + 14) as u32).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&(off as u32).to_le_bytes());
        bmp.extend_from_slice(data);
        return Some(bmp);
    }
    Some(data.to_vec())
}

/// Le immagini di un foglio: dove sono ancorate e quale BLIP mostrano.
fn sheet_pictures(d: &[u8], out: &mut Vec<(u32, (u16, u16, u16, u16, u16, u16, u16, u16))>) {
    let mut i = 0;
    while i + 8 <= d.len() {
        let kind = u16_at(d, i + 2);
        let len = u32_at(d, i + 4) as usize;
        let body = &d[i + 8..(i + 8 + len).min(d.len())];
        match kind {
            0xF002 | 0xF003 => sheet_pictures(body, out),
            0xF004 => {
                let mut pib = None;
                let mut anchor = None;
                let mut j = 0;
                while j + 8 <= body.len() {
                    let vi = u16_at(body, j);
                    let k = u16_at(body, j + 2);
                    let l = u32_at(body, j + 4) as usize;
                    let b = &body[j + 8..(j + 8 + l).min(body.len())];
                    match k {
                        0xF00B => {
                            let n = (vi >> 4) as usize;
                            for p in 0..n {
                                let id = u16_at(b, p * 6) & 0x3FFF;
                                if id == 0x0104 {
                                    pib = Some(u32_at(b, p * 6 + 2));
                                }
                            }
                        }
                        0xF010 if b.len() >= 18 => {
                            let g = |o: usize| u16_at(b, o);
                            anchor = Some((g(2), g(4), g(6), g(8), g(10), g(12), g(14), g(16)));
                        }
                        _ => {}
                    }
                    j += 8 + l;
                }
                if let (Some(p), Some(a)) = (pib, anchor) {
                    out.push((p, a));
                }
            }
            _ => {}
        }
        i += 8 + len;
    }
}

/// Le aree di un nome (Print_Area, Print_Titles) dalla sua formula: le ptgArea3d e ptgRef3d.
fn name_areas(rgce: &[u8]) -> Vec<(u32, u32, u32, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rgce.len() {
        match rgce[i] {
            0x3B | 0x5B | 0x7B if i + 11 <= rgce.len() => {
                let r1 = u16_at(rgce, i + 3) as u32;
                let r2 = u16_at(rgce, i + 5) as u32;
                let c1 = (u16_at(rgce, i + 7) & 0x3FFF) as u32;
                let c2 = (u16_at(rgce, i + 9) & 0x3FFF) as u32;
                out.push((r1, c1, r2, c2));
                i += 11;
            }
            0x3A | 0x5A | 0x7A if i + 7 <= rgce.len() => {
                let r = u16_at(rgce, i + 3) as u32;
                let c = (u16_at(rgce, i + 5) & 0x3FFF) as u32;
                out.push((r, c, r, c));
                i += 7;
            }
            // ptgMemFunc / ptgMemArea: la lunghezza che segue, poi le aree.
            0x29 | 0x49 | 0x69 => i += 3,
            0x26 | 0x46 | 0x66 => i += 7,
            0x10 => i += 1,
            _ => break,
        }
    }
    out
}

/// Il collegamento di un record HLINK: indirizzo web o posizione nel file.
fn hyperlink(d: &[u8]) -> Option<(u32, u32, u32, u32, String)> {
    let (r1, r2, c1, c2) = (u16_at(d, 0) as u32, u16_at(d, 2) as u32, u16_at(d, 4) as u32, u16_at(d, 6) as u32);
    let mut i = 8 + 16 + 4;
    let flags = u32_at(d, i);
    i += 4;
    let hstring = |i: &mut usize| -> String {
        let n = u32_at(d, *i) as usize;
        *i += 4;
        let units: Vec<u16> = (0..n).map(|k| u16_at(d, *i + k * 2)).take_while(|u| *u != 0).collect();
        *i += n * 2;
        String::from_utf16_lossy(&units)
    };
    if flags & 0x14 != 0 {
        hstring(&mut i);
    }
    if flags & 0x80 != 0 {
        hstring(&mut i);
    }
    let mut url = String::new();
    if flags & 1 != 0 {
        if flags & 0x100 != 0 {
            url = hstring(&mut i);
        } else {
            let clsid = d.get(i..i + 16)?;
            i += 16;
            const URL_MONIKER: [u8; 16] = [0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B];
            const FILE_MONIKER: [u8; 16] = [0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
            if clsid == URL_MONIKER {
                let n = u32_at(d, i) as usize;
                i += 4;
                let units: Vec<u16> = (0..n / 2).map(|k| u16_at(d, i + k * 2)).take_while(|u| *u != 0).collect();
                url = String::from_utf16_lossy(&units);
                i += n;
            } else if clsid == FILE_MONIKER {
                i += 2;
                let n = u32_at(d, i) as usize;
                i += 4;
                let ansi: String = d.get(i..i + n).unwrap_or(&[]).iter().take_while(|b| **b != 0).map(|b| *b as char).collect();
                i += n + 2 + 2 + 20;
                let cb = u32_at(d, i) as usize;
                i += 4;
                url = if cb > 0 {
                    let bytes = u32_at(d, i) as usize;
                    let units: Vec<u16> = (0..bytes / 2).map(|k| u16_at(d, i + 6 + k * 2)).collect();
                    i += cb;
                    String::from_utf16_lossy(&units)
                } else {
                    ansi
                };
            } else {
                return None;
            }
        }
    }
    if flags & 8 != 0 {
        let loc = hstring(&mut i);
        if url.is_empty() {
            url = format!("#{loc}");
        } else {
            url = format!("{url}#{loc}");
        }
    }
    (!url.is_empty()).then_some((r1, c1, r2, c2, url))
}

/// Legge un .xls (BIFF8) per stamparlo.
pub fn leggi_xls(bytes: &[u8], fonts_db: &mut super::font::Fonti) -> Result<Cartella, String> {
    if bytes.len() > 4 && bytes[1] <= 0x08 && matches!(bytes[0], 0x09) {
        return Err("è un .xls di Excel 4 o precedente: aprilo e salvalo in .xlsx".into());
    }
    let mut comp = cfb::CompoundFile::open(std::io::Cursor::new(bytes)).map_err(|_| "il file non è un foglio Excel leggibile".to_string())?;
    let mut s = Vec::new();
    let opened = comp.open_stream("/Workbook").or_else(|_| comp.open_stream("/Book"));
    opened.map_err(|_| "nel file non c'è una cartella di Excel".to_string())?.read_to_end(&mut s).map_err(|e| e.to_string())?;
    if comp.open_stream("/EncryptionInfo").is_ok() || records(&s, 0).iter().any(|r| r.kind == 0x002F) {
        return Err("il file Excel è protetto da password: aprilo e salvalo senza".into());
    }
    let globals = records(&s, 0);
    if globals.first().is_none_or(|r| u16_at(&r.data(), 0) != 0x0600) {
        return Err("è un .xls troppo vecchio (prima di Excel 97): salvalo in .xlsx".into());
    }
    let l = super::numfmt::locale();
    let mut fonts: Vec<Carattere> = Vec::new();
    let mut formats: HashMap<u16, String> = HashMap::new();
    let mut xfs: Vec<Xf> = Vec::new();
    let mut palette = Palette(Vec::new());
    let mut sheets: Vec<(u32, u8, u8, String)> = Vec::new();
    let mut sst: Vec<String> = Vec::new();
    let mut date1904 = false;
    let mut attivo = 0usize;
    let mut names: Vec<(u16, u8, Vec<(u32, u32, u32, u32)>)> = Vec::new();
    let mut drawing_group: Vec<u8> = Vec::new();
    let mut font_colors: Vec<u16> = Vec::new();
    for r in &globals {
        let d = r.data();
        match r.kind {
            0x0022 => date1904 = u16_at(&d, 0) == 1,
            0x0031 => {
                let grbit = u16_at(&d, 2);
                font_colors.push(u16_at(&d, 4));
                fonts.push(Carattere {
                    nome: short_string(&d, 14),
                    size: u16_at(&d, 0) as f32 / 20.0,
                    bold: u16_at(&d, 6) >= 600,
                    italic: grbit & 2 != 0,
                    underline: d.get(10).is_some_and(|u| *u != 0),
                    strike: grbit & 8 != 0,
                    color: (0, 0, 0),
                });
                // Il carattere numero 4 non esiste: dopo il terzo si salta un numero.
                if fonts.len() == 4 {
                    fonts.push(fonts[0].clone());
                    font_colors.push(font_colors[0]);
                }
            }
            0x041E => {
                let (code, _) = unicode_string(&d, 2);
                formats.insert(u16_at(&d, 0), code);
            }
            0x00E0 if d.len() >= 20 => {
                let flags = u16_at(&d, 4);
                xfs.push(Xf {
                    font: u16_at(&d, 0),
                    fmt: u16_at(&d, 2),
                    parent: flags >> 4,
                    style: flags & 4 != 0,
                    align: d[6],
                    rot: d[7],
                    indent: d[8],
                    used: d[9],
                    border1: u32_at(&d, 10),
                    border2: u32_at(&d, 14),
                    fill: u16_at(&d, 18),
                });
            }
            0x0092 => {
                let n = u16_at(&d, 0) as usize;
                palette.0 = (0..n).map(|k| (d.get(2 + k * 4).copied().unwrap_or(0), d.get(3 + k * 4).copied().unwrap_or(0), d.get(4 + k * 4).copied().unwrap_or(0))).collect();
            }
            0x0085 => sheets.push((u32_at(&d, 0), d.get(4).copied().unwrap_or(0), d.get(5).copied().unwrap_or(0), short_string(&d, 6))),
            0x00FC => {
                let parts: Vec<&[u8]> = r.parts.clone();
                let mut rd = Reader::new(&parts);
                rd.skip(4);
                let n = rd.u32().unwrap_or(0) as usize;
                for _ in 0..n.min(10_000_000) {
                    match rd.rich_string() {
                        Some(s) => sst.push(s),
                        None => break,
                    }
                }
            }
            0x003D => attivo = u16_at(&d, 10) as usize,
            0x0018 if d.len() >= 15 => {
                let grbit = u16_at(&d, 0);
                let cch = d[3] as usize;
                let cce = u16_at(&d, 4) as usize;
                let itab = u16_at(&d, 8);
                if grbit & 0x20 != 0 {
                    // Nome di sistema: un carattere di codice (6 Print_Area, 7 Print_Titles).
                    let high = d[14] & 1 != 0;
                    let code = d.get(15).copied().unwrap_or(0);
                    let start = 15 + cch * if high { 2 } else { 1 };
                    let rgce = d.get(start..start + cce).unwrap_or(&[]);
                    names.push((itab, code, name_areas(rgce)));
                }
            }
            0x00EB => drawing_group.extend_from_slice(&d),
            _ => {}
        }
    }
    let pictures = blips(&drawing_group);

    // Gli stili: per ogni XF di cella le parti non sue vengono dallo stile genitore.
    let stile_of = |i: usize| -> Stile {
        let Some(x) = xfs.get(i) else { return Stile::default() };
        let parent = xfs.get(x.parent as usize).filter(|_| !x.style && (x.parent as usize) < xfs.len());
        // Per le celle un bit acceso vuol dire «questa parte è mia»; spento, «come lo stile».
        let own = |bit: u8| x.style || parent.is_none() || x.used & bit != 0;
        let pick = |bit: u8| if own(bit) { x } else { parent.unwrap_or(x) };
        let fx = pick(0x08);
        let nx = pick(0x04);
        let ax = pick(0x10);
        let bx = pick(0x20);
        let px = pick(0x40);
        let fi = fx.font as usize;
        let mut font = fonts.get(fi).cloned().unwrap_or_default();
        font.color = font_colors.get(fi).and_then(|c| palette.get(*c)).unwrap_or((0, 0, 0));
        let b1 = bx.border1;
        let b2 = bx.border2;
        let fls = (px.border2 >> 26) & 0x3F;
        let fore = px.fill & 0x7F;
        let back = (px.fill >> 7) & 0x7F;
        let fill = match fls {
            0 => None,
            1 => palette.get(fore),
            _ => palette.get(fore).or(palette.get(back)).map(|c| tint(c, 0.5)),
        };
        let numfmt = formats.get(&nx.fmt).cloned().unwrap_or_else(|| super::numfmt::builtin(nx.fmt as u32, l));
        Stile {
            font,
            fill,
            bordi: [
                border(b1 & 0xF, (b1 >> 16) & 0x7F, &palette),
                border((b1 >> 4) & 0xF, (b1 >> 23) & 0x7F, &palette),
                border((b1 >> 8) & 0xF, b2 & 0x7F, &palette),
                border((b1 >> 12) & 0xF, (b2 >> 7) & 0x7F, &palette),
            ],
            h: match ax.align & 7 {
                1 => HAlign::Left,
                2 => HAlign::Center,
                3 => HAlign::Right,
                4 => HAlign::Fill,
                5 | 7 => HAlign::Justify,
                6 => HAlign::CenterAcross,
                _ => HAlign::General,
            },
            v: match (ax.align >> 4) & 7 {
                0 => VAlign::Top,
                1 | 3 | 4 => VAlign::Center,
                _ => VAlign::Bottom,
            },
            wrap: ax.align & 8 != 0,
            indent: ax.indent & 0xF,
            shrink: ax.indent & 0x10 != 0,
            rotation: match ax.rot {
                255 => 255,
                r @ 0..=90 => r as i16,
                r @ 91..=180 => -((r as i16) - 90),
                _ => 0,
            },
            numfmt,
        }
    };
    let mut stili: Vec<Stile> = Vec::new();
    let mut xf_map: HashMap<u16, u32> = HashMap::new();
    let mut style_index = |ixfe: u16, stili: &mut Vec<Stile>| -> u32 {
        if let Some(i) = xf_map.get(&ixfe) {
            return *i;
        }
        let s = stile_of(ixfe as usize);
        let i = match stili.iter().position(|x| *x == s) {
            Some(i) => i as u32,
            None => {
                stili.push(s);
                (stili.len() - 1) as u32
            }
        };
        xf_map.insert(ixfe, i);
        i
    };
    // Lo stile 0 è quello della cella normale (XF 15 in Excel).
    let normal = stile_of(15.min(xfs.len().saturating_sub(1)));
    stili.push(normal.clone());
    let mdw = super::foglio::max_digit_px(fonts_db, &fonts.first().cloned().unwrap_or_default());

    let mut fogli = Vec::new();
    let mut worksheet_index = Vec::new();
    for (si, (pos, state, dt, nome)) in sheets.iter().enumerate() {
        // Solo i fogli di lavoro: grafici e macro non si stampano qui.
        if *dt != 0 {
            continue;
        }
        let recs = records(&s, *pos as usize);
        let mut f = Foglio::new(nome);
        f.nascosto = *state != 0;
        f.col_default = super::foglio::base_points(8.0, mdw);
        f.riga_default = 12.75;
        // I margini di Excel per i .xls senza impostazioni: 0,75" ai lati, 1" sopra e sotto.
        f.stampa.margini = [54.0, 54.0, 72.0, 72.0, 36.0, 36.0];
        let mut fit = false;
        let mut setup: Option<Vec<u8>> = None;
        let mut pending_formula: Option<(u32, u32, u32)> = None;
        let mut drawing: Vec<u8> = Vec::new();
        let put = |f: &mut Foglio, r: u32, c: u32, valore: Valore, stile: u32| {
            if r < MAX_ROWS && c < MAX_COLS {
                f.celle.insert((r, c), Cella { valore, stile, mostra: None, link: None });
            }
        };
        for rec in &recs {
            let d = rec.data();
            let rc = || (u16_at(&d, 0) as u32, u16_at(&d, 2) as u32, u16_at(&d, 4));
            match rec.kind {
                0x0055 => f.col_default = super::foglio::base_points(u16_at(&d, 0) as f64, mdw),
                0x0099 => f.col_default = super::foglio::col_points(u16_at(&d, 0) as f64 / 256.0, mdw),
                0x0225 => {
                    let h = u16_at(&d, 2);
                    if h > 0 {
                        f.riga_default = h as f32 / 20.0;
                    }
                }
                0x007D => {
                    let (c1, c2) = (u16_at(&d, 0) as u32, u16_at(&d, 2) as u32);
                    let w = super::foglio::col_points(u16_at(&d, 4) as f64 / 256.0, mdw);
                    let xf = u16_at(&d, 6);
                    let hidden = u16_at(&d, 8) & 1 != 0;
                    let st = if xf == 15 { 0 } else { style_index(xf, &mut stili) };
                    for c in c1..=c2.min(MAX_COLS - 1) {
                        f.colonne.insert(c, (Some(w), hidden, st));
                    }
                }
                0x0208 => {
                    let r = u16_at(&d, 0) as u32;
                    let miy = u16_at(&d, 6);
                    let flags = u32_at(&d, 12);
                    let hidden = flags & 0x20 != 0;
                    let h = (miy & 0x7FFF) as f32 / 20.0;
                    let st = (flags & 0x80 != 0).then(|| style_index(((flags >> 16) & 0xFFF) as u16, &mut stili));
                    if r < MAX_ROWS {
                        f.righe.insert(r, ((h > 0.0).then_some(h), hidden, st));
                    }
                }
                0x00FD => {
                    let (r, c, x) = rc();
                    let i = u32_at(&d, 6) as usize;
                    let st = style_index(x, &mut stili);
                    put(&mut f, r, c, Valore::Testo(sst.get(i).cloned().unwrap_or_default()), st);
                }
                0x0204 => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    put(&mut f, r, c, Valore::Testo(unicode_string(&d, 6).0), st);
                }
                0x0203 => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    put(&mut f, r, c, Valore::Numero(f64_at(&d, 6)), st);
                }
                0x027E => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    put(&mut f, r, c, Valore::Numero(rk(u32_at(&d, 6))), st);
                }
                0x00BD => {
                    let r = u16_at(&d, 0) as u32;
                    let c1 = u16_at(&d, 2) as u32;
                    let n = (d.len().saturating_sub(6)) / 6;
                    for k in 0..n {
                        let x = u16_at(&d, 4 + k * 6);
                        let st = style_index(x, &mut stili);
                        put(&mut f, r, c1 + k as u32, Valore::Numero(rk(u32_at(&d, 6 + k * 6))), st);
                    }
                }
                0x0201 => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    if st != 0 {
                        put(&mut f, r, c, Valore::Vuoto, st);
                    }
                }
                0x00BE => {
                    let r = u16_at(&d, 0) as u32;
                    let c1 = u16_at(&d, 2) as u32;
                    let n = (d.len().saturating_sub(6)) / 2;
                    for k in 0..n {
                        let st = style_index(u16_at(&d, 4 + k * 2), &mut stili);
                        if st != 0 {
                            put(&mut f, r, c1 + k as u32, Valore::Vuoto, st);
                        }
                    }
                }
                0x0205 => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    let v = d.get(6).copied().unwrap_or(0);
                    let valore = if d.get(7) == Some(&1) { Valore::Errore(error_text(v)) } else { Valore::Bool(v != 0) };
                    put(&mut f, r, c, valore, st);
                }
                0x0006 => {
                    let (r, c, x) = rc();
                    let st = style_index(x, &mut stili);
                    if u16_at(&d, 12) == 0xFFFF {
                        match d.get(6) {
                            Some(0) => pending_formula = Some((r, c, st)),
                            Some(1) => put(&mut f, r, c, Valore::Bool(d.get(8) == Some(&1)), st),
                            Some(2) => put(&mut f, r, c, Valore::Errore(error_text(d.get(8).copied().unwrap_or(0))), st),
                            _ => put(&mut f, r, c, Valore::Vuoto, st),
                        }
                    } else {
                        put(&mut f, r, c, Valore::Numero(f64_at(&d, 6)), st);
                    }
                }
                0x0207 => {
                    if let Some((r, c, st)) = pending_formula.take() {
                        let parts: Vec<&[u8]> = rec.parts.clone();
                        let mut rd = Reader::new(&parts);
                        let cch = rd.u16().unwrap_or(0) as usize;
                        let flags = rd.byte().unwrap_or(0);
                        put(&mut f, r, c, Valore::Testo(rd.chars(cch, flags & 1 != 0)), st);
                    }
                }
                0x00E5 => {
                    let n = u16_at(&d, 0) as usize;
                    for k in 0..n {
                        let o = 2 + k * 8;
                        if o + 8 > d.len() {
                            break;
                        }
                        f.unioni.push((u16_at(&d, o) as u32, u16_at(&d, o + 4) as u32, u16_at(&d, o + 2) as u32, u16_at(&d, o + 6) as u32));
                    }
                }
                0x001B => {
                    let n = u16_at(&d, 0) as usize;
                    f.stampa.salti_righe = (0..n).map(|k| u16_at(&d, 2 + k * 6) as u32).filter(|r| *r > 0).collect();
                }
                0x001A => {
                    let n = u16_at(&d, 0) as usize;
                    f.stampa.salti_colonne = (0..n).map(|k| u16_at(&d, 2 + k * 6) as u32).filter(|c| *c > 0).collect();
                }
                0x0014 if !d.is_empty() => f.stampa.testa = unicode_string(&d, 0).0,
                0x0015 if !d.is_empty() => f.stampa.piede = unicode_string(&d, 0).0,
                0x0083 => f.stampa.centra_o = u16_at(&d, 0) != 0,
                0x0084 => f.stampa.centra_v = u16_at(&d, 0) != 0,
                0x0026 => f.stampa.margini[0] = (f64_at(&d, 0) * 72.0) as f32,
                0x0027 => f.stampa.margini[1] = (f64_at(&d, 0) * 72.0) as f32,
                0x0028 => f.stampa.margini[2] = (f64_at(&d, 0) * 72.0) as f32,
                0x0029 => f.stampa.margini[3] = (f64_at(&d, 0) * 72.0) as f32,
                0x002B => f.stampa.griglia = u16_at(&d, 0) != 0,
                0x002A => f.stampa.intestazioni = u16_at(&d, 0) != 0,
                0x0081 => fit = u16_at(&d, 0) & 0x100 != 0,
                0x00A1 => setup = Some(d.clone()),
                0x00EC => drawing.extend_from_slice(&d),
                0x01B8 => {
                    if let Some((r1, c1, r2, c2, url)) = hyperlink(&d) {
                        for r in r1..=r2.min(r1 + 1000) {
                            for c in c1..=c2.min(c1 + 100) {
                                if let Some(cell) = f.celle.get_mut(&(r, c)) {
                                    cell.link = Some(url.clone());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(d) = setup {
            let grbit = u16_at(&d, 10);
            let no_pls = grbit & 4 != 0;
            let no_orient = grbit & 0x40 != 0;
            if !no_pls {
                f.stampa.carta = super::foglio::paper(u16_at(&d, 0) as u32);
                let sc = u16_at(&d, 2) as u32;
                if (10..=400).contains(&sc) {
                    f.stampa.scala = sc;
                }
            }
            if !no_orient {
                f.stampa.orizzontale = Some(grbit & 2 == 0);
            }
            if grbit & 0x80 != 0 {
                f.stampa.primo_numero = Some(u16_at(&d, 4) as u32);
            }
            f.stampa.giu_poi_destra = grbit & 1 == 0;
            if fit {
                f.stampa.adatta = Some((u16_at(&d, 6) as u32, u16_at(&d, 8) as u32));
            }
            f.stampa.margini[4] = (f64_at(&d, 16) * 72.0) as f32;
            f.stampa.margini[5] = (f64_at(&d, 24) * 72.0) as f32;
        }
        // Area di stampa e titoli: i nomi di sistema con il numero del foglio (da 1).
        for (itab, code, areas) in &names {
            if *itab as usize != si + 1 {
                continue;
            }
            match code {
                6 => f.stampa.area = areas.first().copied(),
                7 => {
                    for a in areas {
                        if a.1 == 0 && a.3 >= 255 {
                            f.stampa.titoli_righe = Some((a.0, a.2));
                        } else if a.0 == 0 && a.2 >= 65_535 {
                            f.stampa.titoli_colonne = Some((a.1, a.3));
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some((r1, c1, r2, c2)) = f.stampa.area {
            f.stampa.area = Some((r1, c1, r2.min(MAX_ROWS - 1), c2.min(MAX_COLS - 1)));
        }
        // Le immagini.
        let mut pics = Vec::new();
        sheet_pictures(&drawing, &mut pics);
        for (pib, (c1, dx1, r1, dy1, c2, dx2, r2, dy2)) in pics {
            let Some(Some(bytes)) = pictures.get((pib as usize).wrapping_sub(1)) else { continue };
            let (c1, r1, c2, r2) = (c1 as u32, r1 as u32, c2 as u32, r2 as u32);
            let row_h = |r: u32| if f.row_hidden(r) { 0.0 } else { f.righe.get(&r).and_then(|x| x.0).unwrap_or(f.riga_default) };
            let x0 = f.col_width(c1) * dx1 as f32 / 1024.0;
            let y0 = row_h(r1) * dy1 as f32 / 256.0;
            let w = (c1..c2).map(|c| f.col_width(c)).sum::<f32>() - x0 + f.col_width(c2) * dx2 as f32 / 1024.0;
            let h = (r1..r2).map(row_h).sum::<f32>() - y0 + row_h(r2) * dy2 as f32 / 256.0;
            if w > 0.5 && h > 0.5 {
                if let Some(img) = crate::pdfdoc::DocImage::from_bytes(bytes, Some((w, h)), 0) {
                    f.immagini.push((r1, c1, x0, y0, w, h, img));
                }
            }
        }
        worksheet_index.push(si);
        fogli.push(f);
    }
    if fogli.is_empty() {
        return Err("nessun foglio nel file".into());
    }
    let attivo = worksheet_index.iter().position(|i| *i == attivo).unwrap_or(0);
    Ok(Cartella { fogli, stili, attivo, date1904, testo_fuori: false })
}

fn error_text(code: u8) -> String {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        _ => "#N/A",
    }
    .to_string()
}

/// I fogli di un .xls come righe di celle, per CSV, Markdown, HTML e JSON: le date come le mostra
/// Excel, le percentuali come percentuali.
pub fn tabella(bytes: &[u8]) -> Result<Vec<crate::tabella::Foglio>, String> {
    use crate::tabella::Cella as C;
    let book = leggi_xls(bytes, &mut super::font::Fonti::builtin())?;
    let l = super::numfmt::locale();
    let mut out = Vec::new();
    for f in &book.fogli {
        let rows = f.celle.keys().map(|k| k.0).max().map_or(0, |r| r + 1);
        let mut righe: Vec<Vec<C>> = vec![Vec::new(); rows as usize];
        for ((r, c), cell) in &f.celle {
            let st = book.stile(cell.stile);
            let v = match &cell.valore {
                Valore::Vuoto => C::Vuota,
                Valore::Testo(t) => C::Testo(t.clone()),
                Valore::Bool(b) => C::Testo(if *b { "TRUE" } else { "FALSE" }.into()),
                Valore::Errore(e) => C::Testo(e.clone()),
                Valore::Numero(x) if super::numfmt::is_date(&st.numfmt) => C::Testo(super::numfmt::format(*x, &st.numfmt, book.date1904, l).0),
                Valore::Numero(x) if st.numfmt.contains('%') => C::Percento(*x, st.numfmt.split('.').nth(1).map_or(0, |d| d.chars().take_while(|c| *c == '0').count())),
                Valore::Numero(x) => C::Numero(*x),
            };
            let row = &mut righe[*r as usize];
            if row.len() <= *c as usize {
                row.resize(*c as usize + 1, C::Vuota);
            }
            row[*c as usize] = v;
        }
        out.push(crate::tabella::Foglio { nome: f.nome.clone(), righe });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rk_numbers_and_strings_across_continue() {
        assert_eq!(rk((5 << 2) | 2), 5.0);
        assert_eq!(rk((1234 << 2) | 3), 12.34);
        assert_eq!(rk(((1.5f64.to_bits() >> 32) as u32) & 0xFFFF_FFFC), 1.5);
        // «Ciao» spezzata: due caratteri a 8 bit, poi un CONTINUE che riparte a 16 bit.
        let a: &[u8] = &[4, 0, 0, b'C', b'i'];
        let b: &[u8] = &[1, b'a', 0, b'o', 0];
        let parts = [a, b];
        let mut r = Reader::new(&parts);
        assert_eq!(r.rich_string().as_deref(), Some("Ciao"));
        assert_eq!(name_areas(&[0x3B, 0, 0, 1, 0, 9, 0, 0, 0, 3, 0]), vec![(1, 0, 9, 3)]);
    }
}
