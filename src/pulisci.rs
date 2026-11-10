//! «Togli GPS e dati nascosti» per le foto, come Scrambled Exif: la foto resta nel suo formato e
//! non si ricomprime. Si tolgono solo i blocchi di dati (EXIF con posizione, fotocamera e data, XMP,
//! IPTC, commenti, miniatura); l'immagine e il profilo colore restano identici.
//! L'orientamento è l'unico dato EXIF che resta, in un EXIF nuovo con quel solo campo: senza, le
//! foto del telefono girate apparirebbero storte.

/// Cosa è stato tolto, in parole (per il messaggio finale).
#[derive(Default, Debug)]
pub struct Tolto {
    pub gps: bool,
    pub fotocamera: bool,
    pub data: bool,
    pub miniatura: bool,
    pub xmp: bool,
    pub iptc: bool,
    pub testi: bool,
    /// Dati attaccati dopo la fine dell'immagine (il video di una foto in movimento, tracce dei produttori).
    pub coda: bool,
    pub altro_exif: bool,
}

impl Tolto {
    pub fn frase(&self) -> String {
        let mut parti = Vec::new();
        if self.gps {
            parti.push("GPS");
        }
        if self.fotocamera {
            parti.push("fotocamera");
        }
        if self.data {
            parti.push("data di scatto");
        }
        if self.miniatura {
            parti.push("miniatura");
        }
        if self.xmp {
            parti.push("XMP");
        }
        if self.iptc {
            parti.push("IPTC");
        }
        if self.testi {
            parti.push("commenti");
        }
        if self.coda {
            parti.push("dati in coda");
        }
        if parti.is_empty() && self.altro_exif {
            parti.push("dati EXIF");
        }
        let tr = |s: &str| crate::i18n::tr(s).into_owned();
        if parti.is_empty() {
            tr("Nessun dato nascosto: copia identica")
        } else {
            let parti: Vec<String> = parti.into_iter().map(tr).collect();
            format!("{}: {}", tr("Tolti"), parti.join(", "))
        }
    }
}

/// Ripulisce i byte di un file immagine. `None`: formato che non si sa ripulire senza ricomprimere.
pub fn pulisci(ext: &str, bytes: &[u8]) -> Option<Result<(Vec<u8>, Tolto), String>> {
    match ext {
        "jpg" | "jpeg" | "jfif" => Some(jpeg(bytes)),
        "png" => Some(png(bytes)),
        "webp" => Some(webp(bytes)),
        // Formati senza posto per EXIF o GPS: la copia è uguale.
        "gif" | "bmp" | "tga" | "qoi" | "pnm" | "ppm" | "pgm" | "pbm" | "ico" | "hdr" => Some(Ok((bytes.to_vec(), Tolto::default()))),
        _ => None,
    }
}

// ---------- EXIF (struttura TIFF) ----------

struct Tiff<'a> {
    d: &'a [u8],
    le: bool,
}

impl Tiff<'_> {
    fn u16(&self, o: usize) -> Option<u16> {
        let b = self.d.get(o..o + 2)?;
        Some(if self.le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    }
    fn u32(&self, o: usize) -> Option<u32> {
        let b = self.d.get(o..o + 4)?;
        Some(if self.le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    }
}

/// Legge un blocco EXIF (dalla «II*»/«MM*» in poi): segna in `t` cosa c'è e ridà l'orientamento.
fn leggi_exif(tiff: &[u8], t: &mut Tolto) -> u16 {
    t.altro_exif = true;
    let le = match tiff.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return 1,
    };
    let x = Tiff { d: tiff, le };
    let Some(ifd0) = x.u32(4) else { return 1 };
    let ifd0 = ifd0 as usize;
    let Some(n) = x.u16(ifd0) else { return 1 };
    let mut orient = 1;
    for i in 0..n as usize {
        let e = ifd0 + 2 + i * 12;
        let Some(tag) = x.u16(e) else { break };
        match tag {
            0x0112 => orient = x.u16(e + 8).unwrap_or(1),
            0x010F | 0x0110 | 0xA434 => t.fotocamera = true,
            0x0132 | 0x8769 => t.data = true,
            0x8825 => t.gps = true,
            _ => {}
        }
    }
    // Dopo IFD0 c'è IFD1 se il file ha la miniatura.
    if let Some(next) = x.u32(ifd0 + 2 + n as usize * 12) {
        if next != 0 {
            t.miniatura = true;
        }
    }
    if (1..=8).contains(&orient) {
        orient
    } else {
        1
    }
}

/// Un EXIF con il solo orientamento (big endian), dalla «MM*» in poi.
fn exif_solo_orientamento(o: u16) -> Vec<u8> {
    let mut v = b"MM\x00\x2a\x00\x00\x00\x08".to_vec();
    v.extend_from_slice(&1u16.to_be_bytes());
    v.extend_from_slice(&0x0112u16.to_be_bytes());
    v.extend_from_slice(&3u16.to_be_bytes());
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&o.to_be_bytes());
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(&0u32.to_be_bytes());
    v
}

// ---------- JPEG ----------

fn jpeg(b: &[u8]) -> Result<(Vec<u8>, Tolto), String> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return Err("non è un JPEG valido".into());
    }
    let mut t = Tolto::default();
    let mut orient = 1;
    let mut keep: Vec<&[u8]> = Vec::new();
    let mut i = 2;
    let mut rest_from = None;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return Err("JPEG rovinato".into());
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        // Inizio dei dati dell'immagine: da qui si copia tutto com'è.
        if m == 0xDA || m == 0xD9 {
            rest_from = Some(i);
            break;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            keep.push(&b[i..i + 2]);
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let end = i + 2 + len;
        if len < 2 || end > b.len() {
            return Err("JPEG rovinato".into());
        }
        let data = &b[i + 4..end];
        let drop = match m {
            0xE1 if data.starts_with(b"Exif\0\0") => {
                orient = leggi_exif(&data[6..], &mut t);
                true
            }
            0xE1 if data.starts_with(b"http://ns.adobe.com/") => {
                t.xmp = true;
                true
            }
            0xED => {
                t.iptc = true;
                true
            }
            0xFE => {
                t.testi = true;
                true
            }
            // APP3…APP13 e APP15: dati di fotocamere e programmi; il profilo colore (APP2),
            // JFIF (APP0) e Adobe (APP14) restano.
            0xE3..=0xEC | 0xEF => {
                t.altro_exif = true;
                true
            }
            _ => false,
        };
        if !drop {
            keep.push(&b[i..end]);
        }
        i = end;
    }
    let Some(rest) = rest_from else { return Err("JPEG senza immagine".into()) };
    let mut out = Vec::with_capacity(b.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    // JFIF davanti, se c'è; poi l'orientamento; poi il resto.
    let (app0, others): (Vec<&[u8]>, Vec<&[u8]>) = keep.into_iter().partition(|s| s.len() > 1 && s[1] == 0xE0);
    for s in app0 {
        out.extend_from_slice(s);
    }
    if orient != 1 {
        let mut seg = b"Exif\0\0".to_vec();
        seg.extend(exif_solo_orientamento(orient));
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((seg.len() + 2) as u16).to_be_bytes());
        out.extend(seg);
    }
    for s in others {
        out.extend_from_slice(s);
    }
    // Dopo l'immagine ci possono essere dati attaccati: il video di una foto in movimento
    // (Motion Photo), le tracce dei produttori. Le altre immagini JPEG (MPF) restano; il resto va via.
    let mut fine = b.len();
    if let Some(mut e) = fine_jpeg(b, rest) {
        while b[e..].starts_with(&[0xFF, 0xD8]) {
            match fine_jpeg(b, e + 2) {
                Some(e2) => e = e2,
                // Un'immagine che non si sa leggere: si lascia tutto com'è.
                None => {
                    e = b.len();
                    break;
                }
            }
        }
        // Un po' di zeri in fondo non sono dati nascosti: la copia resta uguale.
        if b[e..].iter().any(|&x| x != 0) {
            fine = e;
            t.coda = true;
        }
    }
    out.extend_from_slice(&b[rest..fine]);
    Ok((out, t))
}

/// Dove finisce un JPEG: l'indice subito dopo il suo EOI. `da` è l'inizio dei segmenti (dopo «FFD8»).
/// Passa i dati compressi (con gli stuzzicadenti «FF00» e i riavvii) e le immagini a più passate.
fn fine_jpeg(b: &[u8], da: usize) -> Option<usize> {
    let mut i = da;
    while i + 2 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        match b[i + 1] {
            0xFF => i += 1,
            0xD9 => return Some(i + 2),
            0xD0..=0xD8 | 0x01 | 0x00 => i += 2,
            m => {
                let len = u16::from_be_bytes([*b.get(i + 2)?, *b.get(i + 3)?]) as usize;
                if len < 2 {
                    return None;
                }
                i += 2 + len;
                if m == 0xDA {
                    // Dati compressi: fino al prossimo segnalibro vero.
                    while i + 1 < b.len() && !(b[i] == 0xFF && !matches!(b[i + 1], 0x00 | 0xFF | 0xD0..=0xD7)) {
                        i += 1;
                    }
                }
            }
        }
    }
    None
}

// ---------- PNG ----------

fn png(b: &[u8]) -> Result<(Vec<u8>, Tolto), String> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !b.starts_with(SIG) {
        return Err("non è un PNG valido".into());
    }
    let mut t = Tolto::default();
    let mut out = SIG.to_vec();
    let mut i = SIG.len();
    let mut orient = 1;
    while i + 12 <= b.len() {
        let len = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        let end = i + 12 + len;
        if end > b.len() {
            return Err("PNG rovinato".into());
        }
        let kind = &b[i + 4..i + 8];
        let data = &b[i + 8..i + 8 + len];
        let drop = match kind {
            b"eXIf" => {
                orient = leggi_exif(data, &mut t);
                true
            }
            b"iTXt" if data.starts_with(b"XML:com.adobe.xmp\0") => {
                t.xmp = true;
                true
            }
            b"tEXt" | b"zTXt" | b"iTXt" => {
                t.testi = true;
                true
            }
            b"tIME" => {
                t.data = true;
                true
            }
            _ => false,
        };
        if kind == b"IDAT" && orient != 1 {
            // L'orientamento va prima dei dati dell'immagine.
            out.extend(png_chunk(b"eXIf", &exif_solo_orientamento(orient)));
            orient = 1;
        }
        if !drop {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
        if kind == b"IEND" {
            break;
        }
    }
    Ok((out, t))
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = (data.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(data);
    let crc = crc32(&v[4..]);
    v.extend_from_slice(&crc.to_be_bytes());
    v
}

fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in d {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

// ---------- WebP ----------

fn webp(b: &[u8]) -> Result<(Vec<u8>, Tolto), String> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return Err("non è un WebP valido".into());
    }
    let mut t = Tolto::default();
    let mut body = Vec::with_capacity(b.len());
    let mut i = 12;
    let mut vp8x_at = None;
    let mut orient = 1;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let end = (i + 8 + len + (len & 1)).min(b.len());
        if i + 8 + len > b.len() {
            return Err("WebP rovinato".into());
        }
        let kind = &b[i..i + 4];
        match kind {
            b"EXIF" => {
                let data = &b[i + 8..i + 8 + len];
                let data = data.strip_prefix(b"Exif\0\0").unwrap_or(data);
                orient = leggi_exif(data, &mut t);
            }
            b"XMP " => t.xmp = true,
            _ => {
                if kind == b"VP8X" {
                    vp8x_at = Some(body.len());
                }
                body.extend_from_slice(&b[i..end]);
            }
        }
        i = end;
    }
    if orient != 1 {
        let data = exif_solo_orientamento(orient);
        body.extend_from_slice(b"EXIF");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data);
        if data.len() & 1 == 1 {
            body.push(0);
        }
    }
    // Le bandierine di VP8X dicono se ci sono EXIF (0x08) e XMP (0x04).
    if let Some(at) = vp8x_at {
        if let Some(flags) = body.get_mut(at + 8) {
            *flags &= !0x04;
            if orient == 1 {
                *flags &= !0x08;
            }
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend(body);
    Ok((out, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exif_con_gps(orient: u16) -> Vec<u8> {
        // II*, IFD0 con Make, Orientation, GPS; poi IFD1 (miniatura).
        let mut v = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
        v.extend_from_slice(&3u16.to_le_bytes());
        for (tag, typ, val) in [(0x010Fu16, 2u16, 0u32), (0x0112, 3, orient as u32), (0x8825, 4, 0)] {
            v.extend_from_slice(&tag.to_le_bytes());
            v.extend_from_slice(&typ.to_le_bytes());
            v.extend_from_slice(&1u32.to_le_bytes());
            v.extend_from_slice(&val.to_le_bytes());
        }
        v.extend_from_slice(&100u32.to_le_bytes());
        v
    }

    fn jpeg_di_prova(orient: u16) -> (Vec<u8>, Vec<u8>) {
        let mut img = Vec::new();
        image::DynamicImage::new_rgb8(8, 6).write_to(&mut std::io::Cursor::new(&mut img), image::ImageFormat::Jpeg).unwrap();
        let mut seg = b"Exif\0\0".to_vec();
        seg.extend(exif_con_gps(orient));
        let mut b = img[..2].to_vec();
        b.extend_from_slice(&[0xFF, 0xE1]);
        b.extend_from_slice(&((seg.len() + 2) as u16).to_be_bytes());
        b.extend(seg);
        b.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x07]);
        b.extend_from_slice(b"ciao!");
        b.extend_from_slice(&img[2..]);
        (b, img)
    }

    #[test]
    fn il_jpeg_perde_gps_e_commenti_ma_non_l_immagine() {
        let (b, _) = jpeg_di_prova(1);
        let (out, t) = jpeg(&b).unwrap();
        assert!(t.gps && t.fotocamera && t.miniatura && t.testi);
        assert!(!out.windows(6).any(|w| w == b"Exif\0\0"));
        assert!(!out.windows(5).any(|w| w == b"ciao!"));
        assert!(out.len() < b.len());
        let a = image::load_from_memory(&b).unwrap().to_rgb8();
        let c = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(a.as_raw(), c.as_raw(), "i pixel devono restare identici");
        assert!(t.frase().contains("GPS"));
    }

    fn video_in_coda() -> Vec<u8> {
        // Un riquadro «ftyp» e dentro una posizione: come il video di una Motion Photo.
        let mut v = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom".to_vec();
        v.extend_from_slice(b"+45.0000+011.0000/");
        v
    }

    #[test]
    fn il_video_in_coda_di_una_foto_in_movimento_va_via() {
        let (mut b, _) = jpeg_di_prova(1);
        b.extend(video_in_coda());
        let (out, t) = jpeg(&b).unwrap();
        assert!(t.coda && t.frase().contains("coda"));
        assert!(!out.windows(4).any(|w| w == b"ftyp") && !out.windows(7).any(|w| w == b"+45.000"));
        assert_eq!(&out[out.len() - 2..], &[0xFF, 0xD9]);
        let a = image::load_from_memory(&b).unwrap().to_rgb8();
        let c = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(a.as_raw(), c.as_raw());
    }

    #[test]
    fn le_altre_immagini_dopo_la_prima_restano_e_il_video_dopo_di_loro_va_via() {
        let (mut b, img) = jpeg_di_prova(1);
        b.extend_from_slice(&img);
        let con_seconda = b.len();
        b.extend(video_in_coda());
        let (out, t) = jpeg(&b).unwrap();
        assert!(t.coda);
        // Le due immagini restano (la seconda intera), il video no.
        assert!(out.len() > con_seconda - 400);
        assert!(out.ends_with(&img[img.len() - 16..]));
        assert!(!out.windows(4).any(|w| w == b"ftyp"));
    }

    #[test]
    fn gli_zeri_in_fondo_non_cambiano_la_copia() {
        let (mut b, _) = jpeg_di_prova(1);
        let pulito = jpeg(&b).unwrap().0;
        b.extend_from_slice(&[0, 0, 0]);
        let (out, t) = jpeg(&b).unwrap();
        assert!(!t.coda);
        assert_eq!(out.len(), pulito.len() + 3);
    }

    #[test]
    fn la_fine_di_un_jpeg_a_piu_passate_si_trova() {
        // Due passate (SOS), i dati con «FF00» e un riavvio, un DHT in mezzo, poi l'EOI e della coda.
        let b = [
            0xFFu8, 0xD8, 0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02, 0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF, 0xC4, 0x00, 0x04, 0xAA, 0xBB, 0xFF, 0xDA, 0x00,
            0x04, 0x01, 0x02, 0x78, 0xFF, 0xD9, b'x', b'y',
        ];
        assert_eq!(fine_jpeg(&b, 2), Some(30));
        assert_eq!(fine_jpeg(&b[..20], 2), None);
    }

    #[test]
    fn l_orientamento_resta() {
        let (b, _) = jpeg_di_prova(6);
        let (out, _) = jpeg(&b).unwrap();
        let mut t = Tolto::default();
        let at = out.windows(6).position(|w| w == b"Exif\0\0").expect("serve l'EXIF con l'orientamento");
        assert_eq!(leggi_exif(&out[at + 6..], &mut t), 6);
        assert!(!t.gps && !t.fotocamera);
    }

    #[test]
    fn il_png_perde_i_testi() {
        let mut img = Vec::new();
        image::DynamicImage::new_rgb8(4, 4).write_to(&mut std::io::Cursor::new(&mut img), image::ImageFormat::Png).unwrap();
        let ihdr_end = 8 + 12 + 13;
        let mut b = img[..ihdr_end].to_vec();
        b.extend(png_chunk(b"tEXt", b"Author\0Mario"));
        b.extend(png_chunk(b"eXIf", &exif_con_gps(1)));
        b.extend_from_slice(&img[ihdr_end..]);
        let (out, t) = png(&b).unwrap();
        assert!(t.testi && t.gps);
        assert_eq!(out, img);
        image::load_from_memory(&out).unwrap();
    }

    #[test]
    fn senza_dati_la_copia_e_identica() {
        let mut img = Vec::new();
        image::DynamicImage::new_rgb8(4, 4).write_to(&mut std::io::Cursor::new(&mut img), image::ImageFormat::Png).unwrap();
        let (out, t) = png(&img).unwrap();
        assert_eq!(out, img);
        assert!(t.frase().starts_with("Nessun dato"));
    }
}
