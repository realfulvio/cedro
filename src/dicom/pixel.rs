//! I pixel di un DICOM: decodifica dei fotogrammi (non compressi, RLE, JPEG, JPEG lossless, JPEG-LS,
//! JPEG 2000), la «finestra» che li porta a 8 bit e la versione a 16 bit per chi fa ricerca.

use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbImage};

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
    pub frames: u32,
    pub slope: f64,
    pub intercept: f64,
    /// Le finestre scritte nel file (centro, ampiezza).
    pub windows: Vec<(f64, f64)>,
    /// Dimensione del pixel in mm (riga, colonna), se il file la dice.
    pub spacing: Option<(f64, f64)>,
    pub modality: String,
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
        let centers = f.numbers(tag(0x0028, 0x1050));
        let widths = f.numbers(tag(0x0028, 0x1051));
        let windows = centers.iter().zip(widths.iter()).filter(|(_, w)| **w >= 1.0).map(|(c, w)| (*c, *w)).collect();
        let spacing = {
            let s = f.numbers(tag(0x0028, 0x0030));
            let s = if s.len() >= 2 { s } else { f.numbers(tag(0x0018, 0x1164)) };
            (s.len() >= 2 && s[0] > 0.0 && s[1] > 0.0).then(|| (s[0], s[1]))
        };
        Ok(Info {
            rows,
            cols,
            samples: f.u16(tag(0x0028, 0x0002)).unwrap_or(1).max(1) as u32,
            bits_alloc,
            bits_stored,
            high_bit,
            signed: f.u16(tag(0x0028, 0x0103)).unwrap_or(0) == 1,
            photometric: f.string(tag(0x0028, 0x0004)).unwrap_or_else(|| "MONOCHROME2".into()),
            planar: f.u16(tag(0x0028, 0x0006)).unwrap_or(0) as u32,
            frames: (f.number(tag(0x0028, 0x0008)).unwrap_or(1.0) as u32).max(1),
            slope: f.number(tag(0x0028, 0x1053)).filter(|s| *s != 0.0).unwrap_or(1.0),
            intercept: f.number(tag(0x0028, 0x1052)).unwrap_or(0.0),
            windows,
            spacing,
            modality: f.string(tag(0x0008, 0x0060)).unwrap_or_default(),
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
        let part = b.get(index * size..(index + 1) * size).ok_or("pixel troncati")?;
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
        let first = index * n * s;
        if (first + n * s).div_ceil(8) > b.len() {
            return Err("pixel troncati".into());
        }
        let vals = (first..first + n).map(|i| ((b[i / 8] >> (i % 8)) & 1) as i32).collect();
        return Ok(Frame::Gray(vals));
    }
    let bytes = (info.bits_alloc / 8).max(1) as usize;
    let size = n * s * bytes;
    let part = b.get(index * size..(index + 1) * size).ok_or("pixel troncati: il file è incompleto")?;
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
        let vals = (0..n).map(|i| sign_extend(raw(i) >> shift, info.bits_stored, info.signed)).collect::<Vec<_>>();
        if info.photometric == "PALETTE COLOR" {
            return Ok(Frame::Color(palette(f, &vals)?));
        }
        return Ok(Frame::Gray(vals));
    }
    // A colori: interleaved (RGBRGB) o a piani (RRR…GGG…BBB…).
    let down = info.bits_stored.saturating_sub(8);
    let to8 = |v: u32| (v >> down).min(255) as u8;
    let mut rgb = Vec::with_capacity(n * 3);
    for i in 0..n {
        let (a, b2, c) = if info.planar == 1 { (raw(i), raw(n + i), raw(2 * n + i)) } else { (raw(i * s), raw(i * s + 1), raw(i * s + 2)) };
        let (a, b2, c) = (to8(a >> shift), to8(b2 >> shift), to8(c >> shift));
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
    if super::jpeg12::precision(data) == Some(12) {
        // JPEG Extended a 12 bit: il decoder comune si ferma agli 8 bit.
        let img = super::jpeg12::decode(data)?;
        return from_samples(info, img.width, img.height, img.components, img.samples.iter().map(|v| *v as u32), 12);
    }
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
    match meta.pixel_format {
        jpeg_decoder::PixelFormat::L8 => from_samples(info, w, h, 1, px.iter().map(|v| *v as u32), 8),
        jpeg_decoder::PixelFormat::L16 => {
            let it = px.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]]) as u32);
            from_samples(info, w, h, 1, it, info.bits_stored)
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            // Con «niente trasformazione di colore» (APP14 a 0) i tre canali sono già RGB; il decoder
            // li avrebbe trattati come YCbCr solo se il file lo chiede.
            from_samples(info, w, h, 3, px.iter().map(|v| *v as u32), 8)
        }
        jpeg_decoder::PixelFormat::CMYK32 => Err("JPEG CMYK nel DICOM non letto".into()),
    }
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
    let mut segs: Vec<Vec<u8>> = Vec::new();
    for (i, start) in offs.iter().enumerate() {
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
    let bytes = (info.bits_alloc / 8).max(1) as usize;
    let s = info.samples as usize;
    if segs.len() < bytes * s {
        return Err("RLE incompleto".into());
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
        return Ok(Frame::Gray((0..n).map(|i| sign_extend(sample(0, i) >> shift, info.bits_stored, info.signed)).collect()));
    }
    let down = info.bits_stored.saturating_sub(8);
    let to8 = |v: u32| ((v >> shift) >> down).min(255) as u8;
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
            let (c, wd) = effective_window(info, &m, w);
            let invert = info.photometric == "MONOCHROME1";
            let lo = c - 0.5 - (wd - 1.0) / 2.0;
            let span = (wd - 1.0).max(1.0);
            let px: Vec<u8> = m
                .iter()
                .map(|x| {
                    let y = ((x - (c - 0.5)) / span + 0.5) * 255.0;
                    let y = if *x <= lo { 0.0 } else { y.clamp(0.0, 255.0) };
                    let y = y.round() as u8;
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
/// quanto sono spostati (32768 quando i valori possono essere negativi, come gli Hounsfield).
pub fn render16(info: &Info, fr: &Frame) -> (DynamicImage, i64) {
    match fr {
        Frame::Color(_) => (render(info, fr, Window::File), 0),
        Frame::Gray(v) => {
            let m = modality(info, v);
            let offset = if m.iter().any(|x| *x < 0.0) || info.signed { 32768 } else { 0 };
            let px: Vec<u16> = m.iter().map(|x| (x.round() as i64 + offset).clamp(0, 65535) as u16).collect();
            let img: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_raw(info.cols, info.rows, px).unwrap_or_else(|| ImageBuffer::new(info.cols, info.rows));
            (DynamicImage::ImageLuma16(img), offset)
        }
    }
}

/// Il primo fotogramma con la finestra del file: per le anteprime e per chi apre il file come foto.
pub fn open_first(f: &DicomFile) -> Result<DynamicImage, String> {
    let info = Info::of(f)?;
    let fr = frame(f, &info, 0)?;
    Ok(render(&info, &fr, Window::File))
}

/// Vero se le scritte nei pixel (nome, data) sono dichiarate o probabili (ecografie, catture di schermo).
pub fn burned_in(f: &DicomFile) -> bool {
    let declared = f.string(tag(0x0028, 0x0301)).is_some_and(|v| v.eq_ignore_ascii_case("YES"));
    let modality = f.string(tag(0x0008, 0x0060)).unwrap_or_default();
    declared || matches!(modality.as_str(), "US" | "SC" | "OT" | "XC" | "ES")
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
            let (img16, off) = render16(&info, &fr);
            assert_eq!(off, 32768);
            assert_eq!(img16.to_luma16().as_raw()[0], (32768 - 1000) as u16);
        }
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
