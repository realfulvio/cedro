//! I pixel di un DICOM: decodifica dei fotogrammi (non compressi, RLE, JPEG, JPEG lossless, JPEG-LS,
//! JPEG 2000), la «finestra» che li porta a 8 bit e la versione a 16 bit per chi fa ricerca.

use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbImage};

use super::jpegls::MAX_SAMPLES;
use super::{tag, text, DicomFile, Element, Value, PIXEL_DATA};

/// Come è fatta l'immagine.
#[derive(Clone, Debug)]
pub struct Info {
    pub rows: u32,
    pub cols: u32,
    pub samples: u32,
    pub bits_alloc: u32,
    pub bits_stored: u32,
    pub high_bit: u32,
    pub signed: bool,
    pub photometric: String,
    pub planar: u32,
    /// I fotogrammi che ci sono davvero (al massimo quanti ne dice il file).
    pub frames: u32,
    /// Quanti fotogrammi dice il file: se sono più di `frames`, il file è incompleto.
    pub declared_frames: u32,
    pub slope: f64,
    pub intercept: f64,
    /// Le finestre scritte nel file (centro, ampiezza).
    pub windows: Vec<(f64, f64)>,
    /// Dimensione del pixel in mm (riga, colonna), se il file la dice.
    pub spacing: Option<(f64, f64)>,
    pub modality: String,
    /// La tabella VOI LUT del file, usata quando non ci sono finestre (molte radiografie).
    pub voi_lut: Option<VoiLut>,
    /// Vero se il file chiede di mostrare l'immagine al contrario (Presentation LUT Shape INVERSE su
    /// un MONOCHROME2). Nei MONOCHROME1 l'INVERSE è la stessa inversione, non una seconda.
    pub inverse: bool,
}

/// Una tabella VOI LUT: dal valore (dopo la scala del file) al grigio.
#[derive(Clone, Debug)]
pub struct VoiLut {
    pub first: i64,
    pub bits: u32,
    pub data: Vec<u16>,
}

impl VoiLut {
    /// Il grigio a 8 bit (prima di un'eventuale inversione).
    fn apply(&self, x: f64) -> u8 {
        let last = self.data.len().saturating_sub(1) as i64;
        let i = (x.round() as i64 - self.first).clamp(0, last) as usize;
        let top = ((1u64 << self.bits) - 1).max(1) as f64;
        (self.data.get(i).copied().unwrap_or(0) as f64 / top * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

/// I numeri di un DS o IS scritto in un elemento.
fn numbers_of(e: &Element) -> Vec<f64> {
    text(e.bytes()).split('\\').filter_map(|v| v.trim().parse::<f64>().ok()).collect()
}

/// Nei multi-frame «Enhanced» (TC e RM nuove) scala, finestra e misure del pixel non sono al primo
/// livello ma nei Functional Groups: prima quelli comuni, poi quelli del primo fotogramma.
fn functional(f: &DicomFile, inner: u32, t: u32) -> Vec<f64> {
    for groups in [tag(0x5200, 0x9229), tag(0x5200, 0x9230)] {
        let Some(Element { value: Value::Seq(items), .. }) = f.get(groups) else { continue };
        let Some(Element { value: Value::Seq(seq), .. }) = items.first().and_then(|i| super::find(i, inner)) else { continue };
        if let Some(e) = seq.first().and_then(|i| super::find(i, t)) {
            let v = numbers_of(e);
            if !v.is_empty() {
                return v;
            }
        }
    }
    Vec::new()
}

/// Le parole a 16 bit di un elemento, nell'ordine dei byte del file.
fn words(f: &DicomFile, b: &[u8]) -> Vec<u16> {
    let big = f.syntax == super::Syntax::ExplicitBE;
    b.chunks_exact(2).map(|c| if big { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect()
}

/// La prima tabella della VOI LUT Sequence (0028,3010), se è leggibile.
fn voi_lut(f: &DicomFile, signed: bool) -> Option<VoiLut> {
    let Some(Element { value: Value::Seq(items), .. }) = f.get(tag(0x0028, 0x3010)) else { return None };
    let item = items.first()?;
    let desc = words(f, super::find(item, tag(0x0028, 0x3002))?.bytes());
    let mut data = words(f, super::find(item, tag(0x0028, 0x3006))?.bytes());
    if desc.len() < 3 || data.is_empty() {
        return None;
    }
    let entries = if desc[0] == 0 { 65536 } else { desc[0] as usize };
    data.truncate(entries);
    let first = if signed { desc[1] as i16 as i64 } else { desc[1] as i64 };
    // Bit dichiarati fuori misura: si guardano i valori.
    let bits = if (1..=16).contains(&desc[2]) { desc[2] as u32 } else { 16 - data.iter().max().copied().unwrap_or(1).max(1).leading_zeros() };
    Some(VoiLut { first, bits: bits.max(1), data })
}

/// Quanti byte occupa un fotogramma non compresso (0 se non si sa dire).
fn frame_size(rows: u32, cols: u32, samples: u32, bits_alloc: u32, photometric: &str) -> u64 {
    let n = rows as u64 * cols as u64;
    if photometric == "YBR_FULL_422" && samples == 3 && bits_alloc == 8 {
        return n * 2;
    }
    n * samples as u64 * (bits_alloc / 8).max(1) as u64
}

impl Info {
    pub fn of(f: &DicomFile) -> Result<Info, String> {
        let rows = f.u16(tag(0x0028, 0x0010)).ok_or("il DICOM non ha un'immagine")? as u32;
        let cols = f.u16(tag(0x0028, 0x0011)).ok_or("il DICOM non ha un'immagine")? as u32;
        if rows == 0 || cols == 0 {
            return Err("il DICOM non ha un'immagine".into());
        }
        let bits_alloc = f.u16(tag(0x0028, 0x0100)).unwrap_or(16) as u32;
        let bits_stored = f.u16(tag(0x0028, 0x0101)).map(|b| b as u32).unwrap_or(bits_alloc).clamp(1, bits_alloc.max(1));
        let high_bit = f.u16(tag(0x0028, 0x0102)).map(|b| b as u32).unwrap_or(bits_stored - 1);
        let or_functional = |v: Vec<f64>, inner: u32, t: u32| if v.is_empty() { functional(f, inner, t) } else { v };
        let centers = or_functional(f.numbers(tag(0x0028, 0x1050)), tag(0x0028, 0x9132), tag(0x0028, 0x1050));
        let widths = or_functional(f.numbers(tag(0x0028, 0x1051)), tag(0x0028, 0x9132), tag(0x0028, 0x1051));
        let windows = centers.iter().zip(widths.iter()).filter(|(_, w)| **w >= 1.0).map(|(c, w)| (*c, *w)).collect();
        let spacing = {
            let s = f.numbers(tag(0x0028, 0x0030));
            let s = if s.len() >= 2 { s } else { f.numbers(tag(0x0018, 0x1164)) };
            let s = if s.len() >= 2 { s } else { functional(f, tag(0x0028, 0x9110), tag(0x0028, 0x0030)) };
            (s.len() >= 2 && s[0] > 0.0 && s[1] > 0.0).then(|| (s[0], s[1]))
        };
        let samples = f.u16(tag(0x0028, 0x0002)).unwrap_or(1).max(1) as u32;
        let signed = f.u16(tag(0x0028, 0x0103)).unwrap_or(0) == 1;
        let photometric = f.string(tag(0x0028, 0x0004)).unwrap_or_else(|| "MONOCHROME2".into());
        // I fotogrammi dichiarati possono essere più di quelli nel file (un file incompleto, o un numero
        // sbagliato): si contano quelli che ci sono, così non si cerca un fotogramma che non esiste.
        let declared_frames = (f.number(tag(0x0028, 0x0008)).unwrap_or(1.0) as u32).max(1);
        let frames = match pixel_element(f).map(|e| &e.value) {
            Ok(Value::Bytes(b)) if bits_alloc == 1 => {
                let bits = rows as u64 * cols as u64 * samples as u64;
                declared_frames.min((b.len() as u64 * 8 / bits.max(1)).min(u32::MAX as u64) as u32)
            }
            Ok(Value::Bytes(b)) => {
                let size = frame_size(rows, cols, samples, bits_alloc, &photometric).max(1);
                declared_frames.min((b.len() as u64 / size).min(u32::MAX as u64) as u32)
            }
            // Ogni fotogramma ha almeno un frammento (il primo è la tabella degli offset).
            Ok(Value::Encapsulated(fr)) => declared_frames.min(fr.len().saturating_sub(1).min(u32::MAX as usize) as u32),
            _ => declared_frames,
        }
        .max(1);
        let slope = f.number(tag(0x0028, 0x1053)).or_else(|| functional(f, tag(0x0028, 0x9145), tag(0x0028, 0x1053)).first().copied());
        let intercept = f.number(tag(0x0028, 0x1052)).or_else(|| functional(f, tag(0x0028, 0x9145), tag(0x0028, 0x1052)).first().copied());
        let inverse = photometric == "MONOCHROME2" && f.string(tag(0x2050, 0x0020)).is_some_and(|s| s.eq_ignore_ascii_case("INVERSE"));
        Ok(Info {
            rows,
            cols,
            samples,
            bits_alloc,
            bits_stored,
            high_bit,
            signed,
            photometric,
            planar: f.u16(tag(0x0028, 0x0006)).unwrap_or(0) as u32,
            frames,
            declared_frames,
            slope: slope.filter(|s| *s != 0.0).unwrap_or(1.0),
            intercept: intercept.unwrap_or(0.0),
            windows,
            spacing,
            modality: f.string(tag(0x0008, 0x0060)).unwrap_or_default(),
            voi_lut: voi_lut(f, signed),
            inverse,
        })
    }

    fn pixels(&self) -> usize {
        self.rows as usize * self.cols as usize
    }

    pub fn monochrome(&self) -> bool {
        self.photometric.starts_with("MONOCHROME")
    }
}

/// Un fotogramma decodificato.
pub enum Frame {
    /// I valori memorizzati (con il segno già esteso), prima della scala del file.
    Gray(Vec<i32>),
    /// RGB a 8 bit, un pixel dopo l'altro.
    Color(Vec<u8>),
}

/// La finestra con cui portare i grigi a 8 bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Window {
    /// Quella scritta nel file (o automatica se manca).
    File,
    /// Centro e ampiezza, in unità Hounsfield per la TC.
    Preset(f64, f64),
    /// Dal più scuro al più chiaro dell'immagine (tolti gli estremi).
    Auto,
}

/// Le finestre classiche della TC, in unità Hounsfield (centro, ampiezza).
pub const LUNG: Window = Window::Preset(-600.0, 1500.0);
pub const BONE: Window = Window::Preset(400.0, 1800.0);
pub const SOFT: Window = Window::Preset(50.0, 400.0);
pub const BRAIN: Window = Window::Preset(40.0, 80.0);

fn ts_name(ts: &str) -> &str {
    match ts {
        "1.2.840.10008.1.2.4.50" => "JPEG",
        "1.2.840.10008.1.2.4.51" => "JPEG a 12 bit",
        "1.2.840.10008.1.2.4.57" | "1.2.840.10008.1.2.4.70" => "JPEG lossless",
        "1.2.840.10008.1.2.4.80" | "1.2.840.10008.1.2.4.81" => "JPEG-LS",
        "1.2.840.10008.1.2.4.90" | "1.2.840.10008.1.2.4.91" | "1.2.840.10008.1.2.4.92" | "1.2.840.10008.1.2.4.93" => "JPEG 2000",
        "1.2.840.10008.1.2.4.201" | "1.2.840.10008.1.2.4.202" | "1.2.840.10008.1.2.4.203" => "JPEG 2000 HT",
        "1.2.840.10008.1.2.5" => "RLE",
        "1.2.840.10008.1.2.4.100" | "1.2.840.10008.1.2.4.101" | "1.2.840.10008.1.2.4.102" | "1.2.840.10008.1.2.4.103" | "1.2.840.10008.1.2.4.104" | "1.2.840.10008.1.2.4.105"
        | "1.2.840.10008.1.2.4.106" => "MPEG (video)",
        _ => ts,
    }
}

fn pixel_element(f: &DicomFile) -> Result<&Element, String> {
    f.get(PIXEL_DATA)
        .or_else(|| f.get(tag(0x7FE0, 0x0008)))
        .or_else(|| f.get(tag(0x7FE0, 0x0009)))
        .ok_or_else(|| "il DICOM non contiene un'immagine (forse è un referto o un elenco)".to_string())
}

/// Il fotogramma `index` del file.
pub fn frame(f: &DicomFile, info: &Info, index: usize) -> Result<Frame, String> {
    // Grigi (1), colori (3) o colori con un canale in più (4, se ne usano tre): 2 non ha senso.
    if !matches!(info.samples, 1 | 3 | 4) {
        return Err(format!("immagine con {} campioni per pixel: non letta", info.samples));
    }
    let el = pixel_element(f)?;
    match &el.value {
        Value::Bytes(b) => native(f, info, b, index),
        Value::Encapsulated(frags) => {
            let data = frame_fragments(frags, info.frames as usize, index)?;
            match decode_compressed(f, info, &data)? {
                Frame::Gray(v) if info.photometric == "PALETTE COLOR" => Ok(Frame::Color(palette(f, &v)?)),
                fr => Ok(fr),
            }
        }
        Value::Seq(_) => Err("pixel non validi".into()),
    }
}

/// I byte compressi del fotogramma: dalla tabella degli offset, un frammento per fotogramma, o
/// riconoscendo dove comincia ogni immagine.
fn frame_fragments(frags: &[Vec<u8>], frames: usize, index: usize) -> Result<Vec<u8>, String> {
    let (bot, data) = frags.split_first().ok_or("pixel vuoti")?;
    if data.is_empty() {
        return Err("pixel vuoti".into());
    }
    if frames <= 1 {
        return Ok(data.concat());
    }
    if bot.len() >= 4 * frames {
        let offs: Vec<usize> = bot.chunks(4).take(frames).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize).collect();
        // Gli offset contano anche gli 8 byte dell'intestazione di ogni frammento.
        let mut starts = Vec::new();
        let mut at = 0usize;
        for (i, fr) in data.iter().enumerate() {
            starts.push((at, i));
            at += fr.len() + 8;
        }
        let first = starts.iter().position(|(a, _)| *a == offs[index]);
        let next = if index + 1 < frames { starts.iter().position(|(a, _)| *a == offs[index + 1]) } else { Some(data.len()) };
        if let (Some(a), Some(b)) = (first, next) {
            if a < b {
                return Ok(data[a..b].concat());
            }
        }
    }
    if data.len() == frames {
        return Ok(data[index].clone());
    }
    // Si raggruppano i frammenti a partire da quelli che cominciano un'immagine.
    let starts_image = |d: &Vec<u8>| d.starts_with(&[0xFF, 0xD8]) || d.starts_with(&[0xFF, 0x4F]) || d.starts_with(&[0, 0, 0, 0x0C]);
    let mut groups: Vec<Vec<u8>> = Vec::new();
    for d in data {
        if groups.is_empty() || starts_image(d) {
            groups.push(d.clone());
        } else if let Some(last) = groups.last_mut() {
            last.extend_from_slice(d);
        }
    }
    groups.into_iter().nth(index).ok_or_else(|| format!("fotogramma {} non trovato", index + 1))
}

/// Uno spostamento a destra che con un numero di bit esagerato (dal file) dà zero invece di cadere.
fn shr(v: u32, by: u32) -> u32 {
    v.checked_shr(by).unwrap_or(0)
}

fn sign_extend(v: u32, bits: u32, signed: bool) -> i32 {
    let mask = if bits >= 32 { u32::MAX } else { (1u32 << bits) - 1 };
    let v = v & mask;
    if signed && bits < 32 && v & (1 << (bits - 1)) != 0 {
        (v | !mask) as i32
    } else {
        v as i32
    }
}

fn native(f: &DicomFile, info: &Info, b: &[u8], index: usize) -> Result<Frame, String> {
    let big = f.syntax == super::Syntax::ExplicitBE;
    // In big endian i pixel a 8 bit dichiarati OW sono scambiati a coppie di byte.
    let swapped: Vec<u8>;
    let b = if big && info.bits_alloc == 8 && pixel_element(f).map(|e| &e.vr == b"OW").unwrap_or(false) {
        swapped = b.chunks(2).flat_map(|c| if c.len() == 2 { vec![c[1], c[0]] } else { c.to_vec() }).collect();
        &swapped[..]
    } else {
        b
    };
    let n = info.pixels();
    let s = info.samples as usize;
    if info.photometric == "YBR_FULL_422" && s == 3 && info.bits_alloc == 8 {
        // Due pixel in quattro byte: Y1 Y2 Cb Cr.
        let size = n * 2;
        let part = index.checked_mul(size).and_then(|a| b.get(a..a.checked_add(size)?)).ok_or("pixel troncati")?;
        let mut rgb = Vec::with_capacity(n * 3);
        for q in part.chunks(4) {
            if q.len() < 4 {
                break;
            }
            for y in [q[0], q[1]] {
                rgb.extend_from_slice(&ycc(y, q[2], q[3]));
            }
        }
        rgb.resize(n * 3, 0);
        return Ok(Frame::Color(rgb));
    }
    if info.bits_alloc == 1 {
        // I fotogrammi a 1 bit sono uno di seguito all'altro, senza arrotondare al byte.
        let first = index.checked_mul(n * s).ok_or("pixel troncati")?;
        if (first + n * s).div_ceil(8) > b.len() {
            return Err("pixel troncati".into());
        }
        let vals = (first..first + n).map(|i| ((b[i / 8] >> (i % 8)) & 1) as i32).collect();
        return Ok(Frame::Gray(vals));
    }
    let bytes = (info.bits_alloc / 8).max(1) as usize;
    let size = n * s * bytes;
    let part = index.checked_mul(size).and_then(|a| b.get(a..a.checked_add(size)?)).ok_or("pixel troncati: il file è incompleto")?;
    let raw = |i: usize| -> u32 {
        let c = &part[i * bytes..(i + 1) * bytes];
        match (bytes, big) {
            (1, _) => c[0] as u32,
            (2, false) => u16::from_le_bytes([c[0], c[1]]) as u32,
            (2, true) => u16::from_be_bytes([c[0], c[1]]) as u32,
            (_, false) => u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
            (_, true) => u32::from_be_bytes([c[0], c[1], c[2], c[3]]),
        }
    };
    let shift = (info.high_bit + 1).saturating_sub(info.bits_stored);
    if s == 1 {
        let vals = (0..n).map(|i| sign_extend(shr(raw(i), shift), info.bits_stored, info.signed)).collect::<Vec<_>>();
        if info.photometric == "PALETTE COLOR" {
            return Ok(Frame::Color(palette(f, &vals)?));
        }
        return Ok(Frame::Gray(vals));
    }
    // A colori: interleaved (RGBRGB) o a piani (RRR…GGG…BBB…).
    let down = info.bits_stored.saturating_sub(8);
    let to8 = |v: u32| shr(v, down).min(255) as u8;
    let mut rgb = Vec::with_capacity(n * 3);
    for i in 0..n {
        let (a, b2, c) = if info.planar == 1 { (raw(i), raw(n + i), raw(2 * n + i)) } else { (raw(i * s), raw(i * s + 1), raw(i * s + 2)) };
        let (a, b2, c) = (to8(shr(a, shift)), to8(shr(b2, shift)), to8(shr(c, shift)));
        if info.photometric.starts_with("YBR") {
            rgb.extend_from_slice(&ycc(a, b2, c));
        } else {
            rgb.extend_from_slice(&[a, b2, c]);
        }
    }
    Ok(Frame::Color(rgb))
}

fn ycc(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (y as f32, cb as f32 - 128.0, cr as f32 - 128.0);
    let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    [c(y + 1.402 * cr), c(y - 0.344_136 * cb - 0.714_136 * cr), c(y + 1.772 * cb)]
}

/// I colori di un DICOM a tavolozza (PALETTE COLOR).
fn palette(f: &DicomFile, idx: &[i32]) -> Result<Vec<u8>, String> {
    let big = f.syntax == super::Syntax::ExplicitBE;
    let words = |b: &[u8]| -> Vec<u16> { b.chunks(2).filter(|c| c.len() == 2).map(|c| if big { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect() };
    let desc = |t: u32| -> Result<(usize, i32, u32), String> {
        let d = words(f.get(t).ok_or("tavolozza mancante")?.bytes());
        if d.len() < 3 {
            return Err("tavolozza non valida".into());
        }
        let entries = if d[0] == 0 { 65536 } else { d[0] as usize };
        Ok((entries, d[1] as i16 as i32, d[2] as u32))
    };
    let mut luts = Vec::new();
    for (dt, lt) in [(0x0028_1101, 0x0028_1201), (0x0028_1102, 0x0028_1202), (0x0028_1103, 0x0028_1203)] {
        let (entries, first, bits) = desc(dt)?;
        let data = f.get(lt).ok_or("tavolozza mancante")?.bytes();
        let vals: Vec<u8> = if bits == 8 && data.len() >= entries && data.len() < entries * 2 {
            data[..entries].to_vec()
        } else {
            let w = words(data);
            w.iter().map(|v| if bits <= 8 { (*v & 0xFF) as u8 } else { (*v >> 8) as u8 }).collect()
        };
        luts.push((first, vals));
    }
    let mut rgb = Vec::with_capacity(idx.len() * 3);
    for v in idx {
        for (first, lut) in &luts {
            let i = (*v - *first).clamp(0, lut.len().saturating_sub(1) as i32) as usize;
            rgb.push(lut.get(i).copied().unwrap_or(0));
        }
    }
    Ok(rgb)
}

fn decode_compressed(f: &DicomFile, info: &Info, data: &[u8]) -> Result<Frame, String> {
    match f.ts.as_str() {
        "1.2.840.10008.1.2.5" => rle(info, data),
        "1.2.840.10008.1.2.4.50" | "1.2.840.10008.1.2.4.51" | "1.2.840.10008.1.2.4.57" | "1.2.840.10008.1.2.4.70" => jpeg(info, data),
        "1.2.840.10008.1.2.4.80" | "1.2.840.10008.1.2.4.81" => {
            let img = super::jpegls::decode(data)?;
            from_samples(info, img.width, img.height, img.components, img.samples.iter().map(|v| *v as u32), info.bits_stored)
        }
        "1.2.840.10008.1.2.4.90" | "1.2.840.10008.1.2.4.91" | "1.2.840.10008.1.2.4.92" | "1.2.840.10008.1.2.4.93" | "1.2.840.10008.1.2.4.201" | "1.2.840.10008.1.2.4.202"
        | "1.2.840.10008.1.2.4.203" => j2k(info, data),
        other => {
            // Alcuni file dichiarano una sintassi sbagliata: si prova dai primi byte.
            if data.starts_with(&[0xFF, 0xD8, 0xFF, 0xF7]) {
                let img = super::jpegls::decode(data)?;
                return from_samples(info, img.width, img.height, img.components, img.samples.iter().map(|v| *v as u32), info.bits_stored);
            }
            if data.starts_with(&[0xFF, 0xD8]) {
                return jpeg(info, data);
            }
            if data.starts_with(&[0xFF, 0x4F]) || data.starts_with(&[0, 0, 0, 0x0C]) {
                return j2k(info, data);
            }
            Err(format!("compressione DICOM non ancora letta: {}", ts_name(other)))
        }
    }
}

/// I campioni decodificati (interleaved) diventano un fotogramma.
fn from_samples(info: &Info, w: u32, h: u32, comps: u32, samples: impl Iterator<Item = u32>, bits: u32) -> Result<Frame, String> {
    if w != info.cols || h != info.rows {
        return Err(format!("immagine di {w}×{h} invece di {}×{}", info.cols, info.rows));
    }
    let n = info.pixels();
    if comps == 1 {
        let mut vals: Vec<i32> = samples.take(n).map(|v| sign_extend(v, info.bits_stored, info.signed)).collect();
        vals.resize(n, 0);
        return Ok(Frame::Gray(vals));
    }
    // Oltre gli 8 bit si tengono quelli più significativi.
    let shift = bits.saturating_sub(8);
    let mut rgb: Vec<u8> = samples.take(n * comps as usize).enumerate().filter(|(i, _)| (*i % comps as usize) < 3).map(|(_, v)| (v >> shift).min(255) as u8).collect();
    rgb.resize(n * 3, 0);
    Ok(Frame::Color(rgb))
}

fn jpeg(info: &Info, data: &[u8]) -> Result<Frame, String> {
    let sof = super::jpeg12::sof(data);
    if matches!(sof, Some((0xC0 | 0xC1, 12))) {
        // JPEG Extended a 12 bit: il decoder comune si ferma agli 8 bit. I lossless (SOF3) a 12 bit
        // invece li legge lui.
        let img = super::jpeg12::decode(data)?;
        return from_samples(info, img.width, img.height, img.components, img.samples.iter().map(|v| *v as u32), 12);
    }
    let precision = sof.map(|(_, p)| p).unwrap_or(8);
    let mut d = jpeg_decoder::Decoder::new(std::io::Cursor::new(data));
    d.read_info().map_err(|e| format!("JPEG nel DICOM non letto: {e}"))?;
    if let Some(m) = d.info() {
        if m.width as u32 != info.cols || m.height as u32 != info.rows {
            return Err(format!("immagine di {}×{} invece di {}×{}", m.width, m.height, info.cols, info.rows));
        }
    }
    if let Some(t) = jpeg_transform(data, &info.photometric) {
        d.set_color_transform(t);
    }
    let px = d.decode().map_err(|e| format!("JPEG nel DICOM non letto: {e}"))?;
    let meta = d.info().ok_or("JPEG senza intestazione")?;
    let (w, h) = (meta.width as u32, meta.height as u32);
    // Con «niente trasformazione di colore» (APP14 a 0) i tre canali sono già RGB; il decoder li
    // avrebbe trattati come YCbCr solo se il file lo chiede.
    let comps = match meta.pixel_format {
        jpeg_decoder::PixelFormat::L8 | jpeg_decoder::PixelFormat::L16 => 1usize,
        jpeg_decoder::PixelFormat::RGB24 => 3,
        jpeg_decoder::PixelFormat::CMYK32 => return Err("JPEG CMYK nel DICOM non letto".into()),
    };
    // Nei lossless con precisione diversa da 8 (anche sotto, e anche a colori) il decoder dà due byte
    // per campione, nell'ordine dei byte del computer, qualunque formato dichiari.
    let n = w as usize * h as usize * comps;
    if n > 0 && px.len() >= 2 * n {
        let it = px.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]]) as u32);
        return from_samples(info, w, h, comps as u32, it, precision);
    }
    from_samples(info, w, h, comps as u32, px.iter().map(|v| *v as u32), 8)
}

fn j2k(info: &Info, data: &[u8]) -> Result<Frame, String> {
    let img = hayro_jpeg2000::Image::new(data, &hayro_jpeg2000::DecodeSettings::default()).map_err(|e| format!("JPEG 2000 nel DICOM non letto: {e:?}"))?;
    let (w, h) = (img.width(), img.height());
    // Un file rovinato può dichiarare dimensioni enormi: si controlla prima di decodificare.
    if w != info.cols || h != info.rows {
        return Err(format!("immagine di {w}×{h} invece di {}×{}", info.cols, info.rows));
    }
    let mut ctx = hayro_jpeg2000::DecoderContext::default();
    let dec = img.decode(&mut ctx).map_err(|e| format!("JPEG 2000 nel DICOM non letto: {e:?}"))?;
    let comps = dec.components();
    if comps.is_empty() {
        return Err("JPEG 2000 vuoto".into());
    }
    let n = (w * h) as usize;
    if comps.len() == 1 || info.samples == 1 {
        let s = comps[0].samples();
        let depth = comps[0].bit_depth() as u32;
        // Il decoder restituisce i valori senza segno (spostati di metà scala se il file ha il segno).
        // Conta il segno scritto nel JPEG 2000, non quello del DICOM: alcuni apparecchi li fanno discordare.
        let signed = j2k_signed(data).unwrap_or(info.signed);
        let half = if signed { 1i64 << depth.saturating_sub(1) } else { 0 };
        let mut vals: Vec<i32> = s.iter().take(n).map(|v| {
            let v = v.round() as i64;
            if signed {
                (v - half) as i32
            } else if info.signed {
                // JPEG 2000 senza segno in un DICOM con segno: come fa pydicom, vale il DICOM.
                sign_extend(v.max(0) as u32, info.bits_stored, true)
            } else {
                v as i32
            }
        }).collect();
        vals.resize(n, 0);
        if w != info.cols || h != info.rows {
            return Err(format!("immagine di {w}×{h} invece di {}×{}", info.cols, info.rows));
        }
        return Ok(Frame::Gray(vals));
    }
    let rgb = dec.data_u8();
    let c = comps.len() as u32;
    from_samples(info, w, h, c, rgb.iter().map(|v| *v as u32), 8)
}

/// Il segno del primo componente, dal marcatore SIZ del codestream.
fn j2k_signed(data: &[u8]) -> Option<bool> {
    let at = data.windows(4).position(|w| w == [0xFF, 0x4F, 0xFF, 0x51])? + 2;
    data.get(at + 40).map(|ssiz| ssiz & 0x80 != 0)
}

/// Se il JPEG a tre canali va letto così com'è (RGB) o convertito da YCbCr. I marcatori del JPEG
/// (JFIF, Adobe) hanno la precedenza; senza, vale quello che dice il DICOM.
fn jpeg_transform(data: &[u8], photometric: &str) -> Option<jpeg_decoder::ColorTransform> {
    let mut i = 2usize;
    while i + 4 <= data.len() && data[i] == 0xFF {
        let m = data[i + 1];
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let seg = data.get(i + 4..i + 2 + len)?;
        match m {
            0xE0 if seg.starts_with(b"JFIF") => return None,
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => return None,
            0xC0..=0xCF if !matches!(m, 0xC4 | 0xC8 | 0xCC) && seg.len() >= 6 && seg[5] != 3 => return None,
            0xDA => break,
            _ => {}
        }
        i += 2 + len;
    }
    Some(if photometric.starts_with("RGB") { jpeg_decoder::ColorTransform::None } else { jpeg_decoder::ColorTransform::YCbCr })
}

/// RLE (PackBits): un segmento per ogni byte di ogni campione.
fn rle(info: &Info, data: &[u8]) -> Result<Frame, String> {
    if data.len() < 64 {
        return Err("RLE non valido".into());
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let offs: Vec<usize> = (0..count.min(15)).map(|i| u32::from_le_bytes(data[4 + i * 4..8 + i * 4].try_into().unwrap()) as usize).collect();
    let n = info.pixels();
    let bytes = (info.bits_alloc / 8).max(1) as usize;
    let s = info.samples as usize;
    let needed = bytes * s;
    if offs.len() < needed {
        return Err("RLE incompleto".into());
    }
    // Un byte compresso vale al massimo 64 byte (una serie di 128 byte uguali scritta in due): misure
    // più grandi di così sono di un file rovinato, e non si chiede memoria per niente.
    let most = data.len().saturating_mul(64).saturating_add(1024);
    if n > MAX_SAMPLES || n.saturating_mul(needed) > most {
        return Err("RLE: le misure dell'immagine non tornano con i dati".into());
    }
    let mut segs: Vec<Vec<u8>> = Vec::new();
    for (i, start) in offs.iter().enumerate().take(needed) {
        let end = offs.get(i + 1).copied().unwrap_or(data.len()).min(data.len());
        let src = data.get(*start..end).ok_or("RLE troncato")?;
        let mut out = Vec::with_capacity(n);
        let mut p = 0;
        while p < src.len() && out.len() < n {
            let c = src[p] as i8;
            p += 1;
            if c >= 0 {
                let k = c as usize + 1;
                out.extend_from_slice(src.get(p..p + k).unwrap_or(&src[p.min(src.len())..]));
                p += k;
            } else if c != -128 {
                let k = (1 - c as i32) as usize;
                if let Some(v) = src.get(p) {
                    out.extend(std::iter::repeat_n(*v, k));
                }
                p += 1;
            }
        }
        out.resize(n, 0);
        segs.push(out);
    }
    let sample = |c: usize, i: usize| -> u32 {
        let mut v = 0u32;
        for k in 0..bytes {
            v = (v << 8) | segs[c * bytes + k][i] as u32;
        }
        v
    };
    let shift = (info.high_bit + 1).saturating_sub(info.bits_stored);
    if s == 1 {
        return Ok(Frame::Gray((0..n).map(|i| sign_extend(shr(sample(0, i), shift), info.bits_stored, info.signed)).collect()));
    }
    let down = info.bits_stored.saturating_sub(8);
    let to8 = |v: u32| shr(shr(v, shift), down).min(255) as u8;
    let mut rgb = Vec::with_capacity(n * 3);
    for i in 0..n {
        let (a, b, c) = (to8(sample(0, i)), to8(sample(1, i)), to8(sample(2, i)));
        if info.photometric.starts_with("YBR") {
            rgb.extend_from_slice(&ycc(a, b, c));
        } else {
            rgb.extend_from_slice(&[a, b, c]);
        }
    }
    Ok(Frame::Color(rgb))
}

/// I valori «veri» (unità Hounsfield per la TC) dei grigi.
pub fn modality(info: &Info, vals: &[i32]) -> Vec<f64> {
    vals.iter().map(|v| *v as f64 * info.slope + info.intercept).collect()
}

/// Dal più scuro al più chiaro, tolto lo 0,5% agli estremi (righelli, bordi neri).
fn auto_window(vals: &[f64]) -> (f64, f64) {
    if vals.is_empty() {
        return (0.0, 1.0);
    }
    let mut s: Vec<f64> = vals.iter().step_by((vals.len() / 200_000).max(1)).copied().collect();
    s.sort_by(|a, b| a.total_cmp(b));
    let lo = s[s.len() / 200];
    let hi = s[(s.len() - 1) - s.len() / 200];
    let (lo, hi) = if hi > lo { (lo, hi) } else { (s[0], s[s.len() - 1].max(s[0] + 1.0)) };
    ((lo + hi) / 2.0, hi - lo)
}

/// Vero se vale quello che dice il file (e non una finestra della TC o quella automatica).
fn file_window(info: &Info, w: Window) -> bool {
    match w {
        Window::File => true,
        Window::Preset(..) => !(info.modality == "CT" || info.intercept != 0.0),
        Window::Auto => false,
    }
}

/// La finestra che si userà davvero.
pub fn effective_window(info: &Info, vals: &[f64], w: Window) -> (f64, f64) {
    match w {
        // Le finestre della TC hanno senso solo dove i valori sono unità Hounsfield.
        Window::Preset(c, wd) if info.modality == "CT" || info.intercept != 0.0 => (c, wd),
        Window::Preset(..) | Window::File => info.windows.first().copied().unwrap_or_else(|| auto_window(vals)),
        Window::Auto => auto_window(vals),
    }
}

/// Il fotogramma a 8 bit, pronto da salvare in qualunque formato.
pub fn render(info: &Info, fr: &Frame, w: Window) -> DynamicImage {
    match fr {
        Frame::Color(rgb) => DynamicImage::ImageRgb8(RgbImage::from_raw(info.cols, info.rows, rgb.clone()).unwrap_or_else(|| RgbImage::new(info.cols, info.rows))),
        Frame::Gray(v) => {
            let m = modality(info, v);
            let invert = info.photometric == "MONOCHROME1" || info.inverse;
            // Senza finestre scritte, la tabella VOI LUT del file (se c'è) dice come vedere i grigi.
            let lut = info.voi_lut.as_ref().filter(|_| info.windows.is_empty() && file_window(info, w));
            let (c, wd) = effective_window(info, &m, w);
            let lo = c - 0.5 - (wd - 1.0) / 2.0;
            let span = (wd - 1.0).max(1.0);
            let px: Vec<u8> = m
                .iter()
                .map(|x| {
                    let y = match lut {
                        Some(l) => l.apply(*x),
                        None => {
                            let y = ((x - (c - 0.5)) / span + 0.5) * 255.0;
                            let y = if *x <= lo { 0.0 } else { y.clamp(0.0, 255.0) };
                            y.round() as u8
                        }
                    };
                    if invert {
                        255 - y
                    } else {
                        y
                    }
                })
                .collect();
            DynamicImage::ImageLuma8(GrayImage::from_raw(info.cols, info.rows, px).unwrap_or_else(|| GrayImage::new(info.cols, info.rows)))
        }
    }
}

/// Il fotogramma a 16 bit con i valori del file: per chi misura sull'immagine. Ritorna anche di
/// quanto sono spostati (32768 quando i valori possono essere negativi, come gli Hounsfield; o quanto
/// serve perché stiano nei 16 bit) e se qualche valore è stato tagliato perché non ci stava.
/// I MONOCHROME1 non si invertono: i valori restano quelli del file.
pub fn render16(info: &Info, fr: &Frame) -> (DynamicImage, i64, bool) {
    match fr {
        Frame::Color(_) => (render(info, fr, Window::File), 0, false),
        Frame::Gray(v) => {
            let m: Vec<i64> = modality(info, v).iter().map(|x| x.round() as i64).collect();
            let lo = m.iter().copied().min().unwrap_or(0);
            let hi = m.iter().copied().max().unwrap_or(0);
            let offset = if lo >= 0 && hi <= 65535 && (!info.signed || hi > 32767) {
                0
            } else if lo >= -32768 && hi <= 32767 {
                // La scelta di sempre per gli Hounsfield: 0 HU diventa 32768.
                32768
            } else if hi - lo <= 65535 {
                -lo
            } else if lo < 0 {
                32768
            } else {
                0
            };
            let clipped = lo + offset < 0 || hi + offset > 65535;
            let px: Vec<u16> = m.iter().map(|x| (x + offset).clamp(0, 65535) as u16).collect();
            let img: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_raw(info.cols, info.rows, px).unwrap_or_else(|| ImageBuffer::new(info.cols, info.rows));
            (DynamicImage::ImageLuma16(img), offset, clipped)
        }
    }
}

/// Il primo fotogramma con la finestra del file: per le anteprime e per chi apre il file come foto.
pub fn open_first(f: &DicomFile) -> Result<DynamicImage, String> {
    let info = Info::of(f)?;
    let fr = frame(f, &info, 0)?;
    Ok(render(&info, &fr, Window::File))
}

/// Vero se le scritte nei pixel (nome, data) sono dichiarate o probabili (ecografie, catture di schermo;
/// e le radiografie, che hanno spesso nome e data nell'immagine, quando il file non dice nulla).
pub fn burned_in(f: &DicomFile) -> bool {
    let flag = f.string(tag(0x0028, 0x0301));
    let declared = flag.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("YES"));
    let modality = f.string(tag(0x0008, 0x0060)).unwrap_or_default();
    declared || matches!(modality.as_str(), "US" | "SC" | "OT" | "XC" | "ES") || (flag.is_none() && matches!(modality.as_str(), "CR" | "DX" | "RG" | "MG"))
}

#[allow(dead_code)]
fn _uses(_: &Element) -> String {
    text(&[])
}

#[cfg(test)]
mod prove {
    use super::super::prove::ct;
    use super::super::Syntax;
    use super::*;

    #[test]
    fn la_tc_si_vede_con_la_finestra_del_file_e_con_quella_dell_osso() {
        for s in [Syntax::ImplicitLE, Syntax::ExplicitLE, Syntax::ExplicitBE] {
            let f = DicomFile::read(&ct(1, 4, &[-1000, -160, 40, 240], s)).unwrap();
            let info = Info::of(&f).unwrap();
            let fr = frame(&f, &info, 0).unwrap();
            let Frame::Gray(v) = &fr else { panic!() };
            assert_eq!(v, &vec![-1000, -160, 40, 240]);
            // Finestra del file: centro 40, ampiezza 400 → da -160 a 240.
            let img = render(&info, &fr, Window::File).to_luma8();
            assert_eq!(img.as_raw()[0], 0);
            assert_eq!(img.as_raw()[3], 255);
            assert!((126..=129).contains(&img.as_raw()[2]));
            // Finestra polmone: l'aria non è più nera piena.
            let lung = render(&info, &fr, LUNG).to_luma8();
            assert!(lung.as_raw()[0] > 0);
            let (img16, off, _) = render16(&info, &fr);
            assert_eq!(off, 32768);
            assert_eq!(img16.to_luma16().as_raw()[0], (32768 - 1000) as u16);
        }
    }

    fn us(t: u32, v: u16) -> Element {
        Element { tag: t, vr: *b"US", value: Value::Bytes(v.to_le_bytes().to_vec()) }
    }

    fn tx(t: u32, vr: &[u8; 2], v: &str) -> Element {
        Element { tag: t, vr: *vr, value: Value::Bytes(super::super::text_bytes(v)) }
    }

    fn sq(t: u32, items: Vec<Vec<Element>>) -> Element {
        Element { tag: t, vr: *b"SQ", value: Value::Seq(items) }
    }

    /// Un DICOM in grigio senza segno, `rows`×`cols`, con i pixel dati e qualche campo in più.
    fn gray(rows: u16, cols: u16, alloc: u16, stored: u16, extra: Vec<Element>, px: Value, ts: &str) -> DicomFile {
        let mut body = vec![
            us(tag(0x0028, 0x0002), 1),
            tx(tag(0x0028, 0x0004), b"CS", "MONOCHROME2"),
            us(tag(0x0028, 0x0010), rows),
            us(tag(0x0028, 0x0011), cols),
            us(tag(0x0028, 0x0100), alloc),
            us(tag(0x0028, 0x0101), stored),
            us(tag(0x0028, 0x0102), stored - 1),
            us(tag(0x0028, 0x0103), 0),
            Element { tag: PIXEL_DATA, vr: if alloc > 8 { *b"OW" } else { *b"OB" }, value: px },
        ];
        for e in extra {
            body.retain(|b| b.tag != e.tag);
            body.push(e);
        }
        body.sort_by_key(|e| e.tag);
        DicomFile { meta: vec![], body, syntax: Syntax::ExplicitLE, ts: ts.into() }
    }

    fn words(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn grays(f: &DicomFile, index: usize) -> Vec<i32> {
        let info = Info::of(f).unwrap();
        match frame(f, &info, index).unwrap() {
            Frame::Gray(v) => v,
            Frame::Color(_) => panic!("a colori"),
        }
    }

    /// Un JPEG lossless (SOF3, predittore 1) in grigio a `p` bit: come lo scrivono gli apparecchi delle
    /// radiografie. La tabella di Huffman ha 17 simboli con codici di 5 bit (il simbolo s ha codice s).
    fn lossless(img: &[u16], w: usize, h: usize, p: u8) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xC3, 0, 11, p, (h >> 8) as u8, h as u8, (w >> 8) as u8, w as u8, 1, 1, 0x11, 0];
        let mut counts = [0u8; 16];
        counts[4] = 17;
        out.extend_from_slice(&[0xFF, 0xC4, 0, 36, 0x00]);
        out.extend_from_slice(&counts);
        out.extend(0..17u8);
        out.extend_from_slice(&[0xFF, 0xDA, 0, 8, 1, 1, 0x00, 1, 0, 0]);
        let mut bits: Vec<u8> = Vec::new();
        let mut put = |v: u32, n: u32| {
            for i in (0..n).rev() {
                bits.push(((v >> i) & 1) as u8);
            }
        };
        for y in 0..h {
            for x in 0..w {
                let pred = if x == 0 && y == 0 {
                    1i32 << (p - 1)
                } else if y == 0 {
                    img[x - 1] as i32
                } else if x == 0 {
                    img[(y - 1) * w] as i32
                } else {
                    img[y * w + x - 1] as i32
                };
                let mut d = (img[y * w + x] as i32 - pred).rem_euclid(65536);
                if d >= 32768 {
                    d -= 65536;
                }
                let s = if d == 0 { 0 } else { 32 - d.unsigned_abs().leading_zeros() };
                put(s, 5);
                if s > 0 && s < 16 {
                    put(if d > 0 { d as u32 } else { (d - 1) as u32 & ((1 << s) - 1) }, s);
                }
            }
        }
        while bits.len() % 8 != 0 {
            bits.push(1);
        }
        for c in bits.chunks(8) {
            let b = c.iter().fold(0u8, |a, v| (a << 1) | v);
            out.push(b);
            if b == 0xFF {
                out.push(0);
            }
        }
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    #[test]
    fn il_jpeg_lossless_si_legge_uguale_a_ogni_profondita() {
        // Le radiografie CR/DX sono quasi sempre JPEG lossless a 10, 12 o 16 bit: prima i valori
        // uscivano con i byte scambiati e i 12 bit finivano nel decoder sbagliato.
        let (w, h) = (16usize, 12usize);
        for p in [6u8, 8, 10, 12, 16] {
            let mask = ((1u32 << p) - 1) as u16;
            let img: Vec<u16> = (0..w * h).map(|i| (((i / w) * 37 + (i % w) * 91 + (i * 7) % 13) as u32 * (1 << (p - 6)) & mask as u32) as u16).collect();
            let alloc = if p > 8 { 16 } else { 8 };
            let f = gray(h as u16, w as u16, alloc, p as u16, vec![], Value::Encapsulated(vec![vec![], lossless(&img, w, h, p)]), "1.2.840.10008.1.2.4.70");
            let expected: Vec<i32> = img.iter().map(|v| *v as i32).collect();
            assert_eq!(grays(&f, 0), expected, "{p} bit");
        }
    }

    #[test]
    fn due_campioni_per_pixel_non_fanno_cadere_il_programma() {
        for planar in [0, 1] {
            let f = gray(2, 2, 8, 8, vec![us(tag(0x0028, 0x0002), 2), us(tag(0x0028, 0x0006), planar)], Value::Bytes(vec![0; 8]), TS);
            let info = Info::of(&f).unwrap();
            assert!(frame(&f, &info, 0).is_err());
        }
    }

    const TS: &str = super::super::TS_EXPLICIT;

    #[test]
    fn un_rle_con_misure_enormi_non_chiede_gigabyte() {
        // Un frammento di 70 byte che dice 65535×65535 (o 8000×8000): prima si chiedevano 4 GB per segmento.
        let mut rle = vec![0u8; 64];
        rle[0] = 1;
        rle[4] = 64;
        rle.extend_from_slice(&[0x81, 0, 0x81, 0, 0x81, 0]);
        for side in [65535u16, 8000] {
            let f = gray(side, side, 8, 8, vec![], Value::Encapsulated(vec![vec![], rle.clone()]), "1.2.840.10008.1.2.5");
            let info = Info::of(&f).unwrap();
            assert!(frame(&f, &info, 0).is_err(), "{side}");
        }
        // Le misure giuste si leggono ancora: 4×4 tutti a 7.
        let mut ok = vec![0u8; 64];
        ok[0] = 1;
        ok[4] = 64;
        ok.extend_from_slice(&[0xF1, 7]);
        let f = gray(4, 4, 8, 8, vec![], Value::Encapsulated(vec![vec![], ok]), "1.2.840.10008.1.2.5");
        assert_eq!(grays(&f, 0), vec![7; 16]);
    }

    #[test]
    fn si_contano_i_fotogrammi_che_ci_sono_davvero() {
        // Il file dice 5 immagini ma ne ha 2: si leggono quelle 2, e si sa che ne mancano.
        let f = gray(2, 2, 8, 8, vec![tx(tag(0x0028, 0x0008), b"IS", "5")], Value::Bytes(vec![1, 2, 3, 4, 5, 6, 7, 8]), TS);
        let info = Info::of(&f).unwrap();
        assert_eq!((info.frames, info.declared_frames), (2, 5));
        assert_eq!(grays(&f, 1), vec![5, 6, 7, 8]);
        // Un numero di immagini esagerato non fa preparare miliardi di fotogrammi.
        let f = gray(2, 2, 8, 8, vec![tx(tag(0x0028, 0x0008), b"IS", "4000000000")], Value::Bytes(vec![0; 4]), TS);
        assert_eq!(Info::of(&f).unwrap().frames, 1);
    }

    #[test]
    fn le_tc_enhanced_hanno_scala_e_finestra_nei_functional_groups() {
        let shared = sq(
            tag(0x5200, 0x9229),
            vec![vec![
                sq(tag(0x0028, 0x9110), vec![vec![tx(tag(0x0028, 0x0030), b"DS", "0.7\\0.7")]]),
                sq(tag(0x0028, 0x9132), vec![vec![tx(tag(0x0028, 0x1050), b"DS", "40"), tx(tag(0x0028, 0x1051), b"DS", "400")]]),
                sq(tag(0x0028, 0x9145), vec![vec![tx(tag(0x0028, 0x1052), b"DS", "-1024"), tx(tag(0x0028, 0x1053), b"DS", "1")]]),
            ]],
        );
        let f = gray(1, 2, 16, 16, vec![tx(tag(0x0008, 0x0060), b"CS", "CT"), shared], Value::Bytes(words(&[0, 1064])), TS);
        let info = Info::of(&f).unwrap();
        assert_eq!(info.intercept, -1024.0);
        assert_eq!(info.windows, vec![(40.0, 400.0)]);
        assert_eq!(info.spacing, Some((0.7, 0.7)));
        let img = render(&info, &frame(&f, &info, 0).unwrap(), BONE).to_luma8();
        // −1024 HU nero anche con la finestra dell'osso: la scala è quella giusta.
        assert_eq!(img.as_raw()[0], 0);
    }

    #[test]
    fn inverse_si_rispetta_ma_non_inverte_due_volte_i_monochrome1() {
        let px = Value::Bytes(vec![0, 255]);
        let see = |extra: Vec<Element>| {
            let f = gray(1, 2, 8, 8, extra, px.clone(), TS);
            let info = Info::of(&f).unwrap();
            render(&info, &frame(&f, &info, 0).unwrap(), Window::File).to_luma8().into_raw()
        };
        let normal = see(vec![]);
        assert!(normal[0] < normal[1]);
        let inverse = tx(tag(0x2050, 0x0020), b"CS", "INVERSE");
        let shown = see(vec![inverse.clone()]);
        assert!(shown[0] > shown[1], "MONOCHROME2 con INVERSE: {shown:?}");
        // Nelle DX i MONOCHROME1 hanno INVERSE per regola: è la stessa inversione.
        let shown = see(vec![inverse, tx(tag(0x0028, 0x0004), b"CS", "MONOCHROME1")]);
        assert!(shown[0] > shown[1], "MONOCHROME1 con INVERSE: {shown:?}");
    }

    #[test]
    fn senza_finestre_vale_la_voi_lut_del_file() {
        let lut = sq(
            tag(0x0028, 0x3010),
            vec![vec![
                Element { tag: tag(0x0028, 0x3002), vr: *b"US", value: Value::Bytes(words(&[3, 0, 8])) },
                Element { tag: tag(0x0028, 0x3006), vr: *b"OW", value: Value::Bytes(words(&[0, 255, 128])) },
            ]],
        );
        let f = gray(1, 3, 16, 12, vec![lut], Value::Bytes(words(&[0, 1, 2])), TS);
        let info = Info::of(&f).unwrap();
        assert_eq!(render(&info, &frame(&f, &info, 0).unwrap(), Window::File).to_luma8().into_raw(), vec![0, 255, 128]);
        // La finestra automatica la ignora: il più chiaro è bianco.
        assert_eq!(render(&info, &frame(&f, &info, 0).unwrap(), Window::Auto).to_luma8().into_raw()[2], 255);
    }

    #[test]
    fn i_sedici_bit_non_tagliano_i_valori_che_ci_stanno() {
        // Senza segno con la scala che porta a −10…60000: prima lo spostamento di 32768 tagliava i chiari.
        let f = gray(1, 2, 16, 16, vec![tx(tag(0x0028, 0x1052), b"DS", "-10")], Value::Bytes(words(&[0, 60010])), TS);
        let info = Info::of(&f).unwrap();
        let (img, offset, clipped) = render16(&info, &frame(&f, &info, 0).unwrap());
        assert_eq!((offset, clipped), (10, false));
        assert_eq!(img.to_luma16().into_raw(), vec![0, 60010]);
        // Valori senza segno normali: niente spostamento.
        let f = gray(1, 2, 16, 16, vec![], Value::Bytes(words(&[0, 65535])), TS);
        let info = Info::of(&f).unwrap();
        assert_eq!(render16(&info, &frame(&f, &info, 0).unwrap()).1, 0);
    }

    #[test]
    fn un_high_bit_esagerato_non_fa_cadere_il_programma() {
        let f = gray(1, 2, 8, 8, vec![us(tag(0x0028, 0x0102), 200)], Value::Bytes(vec![1, 2]), TS);
        assert_eq!(grays(&f, 0), vec![0, 0]);
    }

    #[test]
    fn le_radiografie_senza_dichiarazione_avvisano_delle_scritte() {
        let with = |m: &str, flag: Option<&str>| {
            let mut extra = vec![tx(tag(0x0008, 0x0060), b"CS", m)];
            if let Some(v) = flag {
                extra.push(tx(tag(0x0028, 0x0301), b"CS", v));
            }
            burned_in(&gray(1, 1, 8, 8, extra, Value::Bytes(vec![0, 0]), TS))
        };
        assert!(with("DX", None));
        assert!(with("CR", None));
        assert!(!with("DX", Some("NO")));
        assert!(with("DX", Some("YES")));
        assert!(!with("CT", None));
        assert!(with("US", Some("NO")));
    }

    /// Confronto con pydicom: `CEDRO_DICOM_DUMP=cartella cargo test -- --ignored scarica`.
    #[test]
    #[ignore]
    fn scarica_i_fotogrammi() {
        let Ok(dir) = std::env::var("CEDRO_DICOM_DUMP") else { return };
        let out = std::path::Path::new(&dir).join("out");
        std::fs::create_dir_all(&out).unwrap();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            eprintln!("> {name}");
            let res = DicomFile::open(&p).and_then(|f| {
                let info = Info::of(&f)?;
                let mut all = Vec::new();
                for i in 0..info.frames as usize {
                    match frame(&f, &info, i)? {
                        Frame::Gray(v) => all.extend(v.iter().flat_map(|x| x.to_le_bytes())),
                        Frame::Color(v) => all.extend(v.iter().flat_map(|x| (*x as i32).to_le_bytes())),
                    }
                }
                Ok(all)
            });
            match res {
                Ok(b) => std::fs::write(out.join(format!("{name}.raw")), b).unwrap(),
                Err(e) => std::fs::write(out.join(format!("{name}.err")), e).unwrap(),
            }
        }
    }
}
