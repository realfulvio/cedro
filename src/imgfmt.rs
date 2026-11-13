//! Formati di foto che il crate `image` non legge o non scrive da solo, tutti in Rust:
//! SVG (resvg), AVIF (ravif per scrivere, avif-decode con rav1d per leggere) e JPEG XL
//! (jxl-oxide per leggere, zune-jpegxl per scrivere, senza perdita) e HEIC/HEIF delle foto dell'iPhone
//! (heifer per leggere, con i colori portati a sRGB). Qui c'è anche il riconoscimento
//! del tipo di file dal contenuto.

use std::fs;
use std::io::Read;
use std::path::Path;

use image::{DynamicImage, GrayAlphaImage, GrayImage, RgbImage, RgbaImage};

/// Il tipo di file letto dai primi byte: l'estensione che dovrebbe avere, o `None` se non si riconosce.
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    let starts = |s: &[u8]| head.starts_with(s);
    let at = |i: usize, s: &[u8]| head.get(i..i + s.len()) == Some(s);
    // DICOM: 128 byte di preambolo e poi «DICM». Va per primo: il preambolo può contenere
    // un'intestazione TIFF (i file «doppi» DICOM/TIFF), ma il contenuto vero è il DICOM.
    if at(128, b"DICM") {
        return Some("dcm");
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some("gif");
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some("webp");
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return Some("wav");
    }
    if starts(b"RIFF") && at(8, b"AVI ") {
        return Some("avi");
    }
    if starts(b"BM") && head.len() > 14 {
        return Some("bmp");
    }
    // Nikon ND2, Leica LIF e Zeiss CZI, i formati dei microscopi.
    if starts(&[0xDA, 0xCE, 0xBE, 0x0A]) {
        return Some("nd2");
    }
    if crate::lif::riconosci(head) {
        return Some("lif");
    }
    if crate::czi::riconosci(head) {
        return Some("czi");
    }
    // Anche BigTIFF (indirizzi a 64 bit: vetrini digitali, immagini oltre i 4 GB).
    if starts(b"II*\0") || starts(b"MM\0*") || starts(b"II+\0") || starts(b"MM\0+") {
        return Some("tiff");
    }
    if starts(&[0, 0, 1, 0]) && head.len() >= 6 && head[4] > 0 {
        return Some("ico");
    }
    if starts(b"qoif") {
        return Some("qoi");
    }
    if starts(b"DDS ") {
        return Some("dds");
    }
    if starts(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some("exr");
    }
    if starts(b"#?RADIANCE") || starts(b"#?RGBE") {
        return Some("hdr");
    }
    if starts(b"P4") || starts(b"P5") || starts(b"P6") || starts(b"P1") || starts(b"P2") || starts(b"P3") {
        return Some("ppm");
    }
    if starts(&[0xFF, 0x0A]) || starts(&[0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A]) {
        return Some("jxl");
    }
    if at(4, b"ftyp") {
        // Il marchio principale e quelli compatibili: un AVIF può dichiararsi «mif1» e dire «avif» dopo.
        let end = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let compatible: Vec<&[u8]> = head.get(16..end.min(head.len())).unwrap_or(&[]).chunks_exact(4).collect();
        let has = |b: &[u8]| at(8, b) || compatible.contains(&b);
        if has(b"avif") || has(b"avis") {
            return Some("avif");
        }
        if has(b"heic") || has(b"heix") || has(b"heim") || has(b"heis") || has(b"hevc") || has(b"hevx") || at(8, b"mif1") || at(8, b"msf1") {
            return Some("heic");
        }
        if at(8, b"M4A ") || at(8, b"M4B ") {
            return Some("m4a");
        }
        if at(8, b"qt  ") {
            return Some("mov");
        }
        return Some("mp4");
    }
    if starts(b"%PDF") {
        return Some("pdf");
    }
    if starts(b"fLaC") {
        return Some("flac");
    }
    if starts(b"ID3") {
        return Some("mp3");
    }
    if starts(b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        return Some("aiff");
    }
    if starts(b"OggS") {
        return Some(if head.windows(8).any(|w| w == b"OpusHead") { "opus" } else { "ogg" });
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some("mkv");
    }
    if starts(b"PK\x03\x04") {
        return Some("zip");
    }
    if starts(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Some("doc");
    }
    if starts(b"{\\rtf") {
        return Some("rtf");
    }
    let text = head.iter().skip_while(|b| b.is_ascii_whitespace() || **b == 0xEF || **b == 0xBB || **b == 0xBF).copied().collect::<Vec<u8>>();
    if text.starts_with(b"<svg") || (text.starts_with(b"<?xml") && head.windows(4).any(|w| w == b"<svg")) {
        return Some("svg");
    }
    None
}

/// I primi byte di un file (fino a 132: il DICOM ha la firma dopo 128 byte di preambolo).
pub fn head_of(path: &Path) -> Vec<u8> {
    let mut buf = Vec::with_capacity(132);
    let _ = fs::File::open(path).and_then(|f| f.take(132).read_to_end(&mut buf));
    buf
}

/// Il tipo del file dal contenuto: l'estensione che dovrebbe avere.
pub fn sniff_file(path: &Path) -> Option<&'static str> {
    sniff(&head_of(path))
}

/// Apre un'immagine di qualunque formato che Cedro legge. `target_side` è il lato lungo voluto per
/// gli SVG, che non hanno una misura loro; gli altri formati ignorano il valore.
pub fn open(path: &Path, straighten: bool, target_side: Option<u32>) -> Result<DynamicImage, String> {
    let head = head_of(path);
    let ext = crate::engine::extension(path);
    let kind: &str = sniff(&head).unwrap_or(ext.as_str());
    // Un vetrino digitale non si apre intero: si prende un livello che sta in memoria.
    if (kind == "tiff" || ext == "mrxs") && crate::svs::is_slide(path) {
        return crate::svs::open_image(path, target_side);
    }
    // Microscopia (OME-TIFF, ImageJ, 16 bit): canali a colori e contrasto, non i 16 bit schiacciati.
    if matches!(kind, "tiff" | "nd2" | "lif" | "czi") && crate::microscopia::is_stack(path) {
        return crate::microscopia::apri(path);
    }
    match kind {
        "svg" | "svgz" => open_svg(path, target_side),
        "avif" => open_avif(path),
        "jxl" => open_jxl(path),
        // Il DICOM: la prima immagine con la finestra scritta nel file.
        "dcm" | "dicom" => crate::dicom::DicomFile::open(path).and_then(|f| crate::dicom::pixel::open_first(&f)),
        // La rotazione delle foto HEIC è nel file (irot/imir) e heifer la applica: l'EXIF non conta.
        "heic" | "heif" | "hif" => open_heic(path, target_side),
        _ if straighten => crate::ocr::open_upright(path),
        _ => image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| format!("immagine non letta: {e}")),
    }
}

/// Un SVG disegnato su una tela trasparente: il lato lungo è `target_side`, o la misura del disegno
/// portata ad almeno 1024 px, perché un logo da 24 px sia utile.
fn open_svg(path: &Path, target_side: Option<u32>) -> Result<DynamicImage, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    // I caratteri del sistema si cercano una volta sola: leggerli tutti a ogni SVG costava decimi di secondo
    // per file (una cartella di icone, le anteprime).
    static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = std::sync::OnceLock::new();
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb = FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            std::sync::Arc::new(db)
        })
        .clone();
    // Il testo dell'SVG usa i caratteri del sistema; se non c'è quello chiesto, vale il primo che si trova.
    let tree = resvg::usvg::Tree::from_data(&data, &opt).map_err(|e| format!("SVG non letto: {e}"))?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let natural = w.max(h);
    let side = target_side.map(|s| s as f32).unwrap_or_else(|| natural.max(1024.0)).clamp(16.0, 16384.0);
    let scale = side / natural;
    let (pw, ph) = (((w * scale).round() as u32).max(1), ((h * scale).round() as u32).max(1));
    if (pw as u64) * (ph as u64) > 150_000_000 {
        return Err("SVG troppo grande da disegnare".into());
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("misura dell'SVG non valida")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia lavora con l'alfa premoltiplicata: si riporta al normale.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage::from_raw(pw, ph, rgba).map(DynamicImage::ImageRgba8).ok_or_else(|| "SVG non disegnato".into())
}

/// Una foto HEIC/HEIF: l'immagine principale, girata come dice il file e con i colori in sRGB (le foto
/// dell'iPhone sono in Display P3). Per una miniatura piccola basta quella che il file porta con sé.
fn open_heic(path: &Path, target_side: Option<u32>) -> Result<DynamicImage, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if let Some(side) = target_side.filter(|s| *s <= 400) {
        if let Ok(Some(t)) = heifer::decode_thumbnail(&data) {
            if t.width.max(t.height) * 4 >= side * 3 {
                let rgba: Vec<u8> = t.to_rgba8();
                if let Some(img) = RgbaImage::from_raw(t.width, t.height, rgba) {
                    return Ok(heic_opaque(img, t.has_alpha));
                }
            }
        }
    }
    let options = heifer::Options::default();
    // Un profilo colore che heifer non sa convertire: i colori restano come sono scritti.
    let image = heifer::decode_srgb_rgba8(&data, &options).or_else(|_| heifer::decode_rgba8(&data, &options)).map_err(|e| format!("HEIC non letto: {e}"))?;
    let (w, h) = (image.width, image.height);
    if image.has_alpha {
        return RgbaImage::from_raw(w, h, image.data).map(DynamicImage::ImageRgba8).ok_or_else(|| "HEIC con misure non valide".into());
    }
    // Senza trasparenza la foto resta RGB: meno memoria, e il JPEG non deve togliere l'alfa.
    RgbImage::from_raw(w, h, image.into_rgb8()).map(DynamicImage::ImageRgb8).ok_or_else(|| "HEIC con misure non valide".into())
}

/// La miniatura del file, come foto RGB se non è trasparente.
fn heic_opaque(img: RgbaImage, alpha: bool) -> DynamicImage {
    if alpha {
        DynamicImage::ImageRgba8(img)
    } else {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(img).to_rgb8())
    }
}

fn open_avif(path: &Path) -> Result<DynamicImage, String> {
    use avif_decode::Image as A;
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let image = avif_decode::Decoder::from_avif(&data).and_then(|d| d.to_image()).map_err(|e| format!("AVIF non letto: {e}"))?;
    let to8 = |v: u16| (v >> 8) as u8;
    let out = match image {
        A::Rgb8(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();
            RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8)
        }
        A::Rgb16(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().flat_map(|p| [to8(p.r), to8(p.g), to8(p.b)]).collect();
            RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8)
        }
        A::Rgba8(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
            RgbaImage::from_raw(w, h, buf).map(DynamicImage::ImageRgba8)
        }
        A::Rgba16(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().flat_map(|p| [to8(p.r), to8(p.g), to8(p.b), to8(p.a)]).collect();
            RgbaImage::from_raw(w, h, buf).map(DynamicImage::ImageRgba8)
        }
        A::Gray8(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().map(|p| p.value()).collect();
            GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8)
        }
        A::Gray16(img) => {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let buf: Vec<u8> = img.pixels().map(|p| to8(p.value())).collect();
            GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8)
        }
    };
    out.ok_or_else(|| "AVIF con misure non valide".into())
}

fn open_jxl(path: &Path) -> Result<DynamicImage, String> {
    let image = jxl_oxide::JxlImage::builder().open(path).map_err(|e| format!("JPEG XL non letto: {e}"))?;
    let (w, h) = (image.width(), image.height());
    if (w as u64) * (h as u64) > 150_000_000 {
        return Err("JPEG XL troppo grande".into());
    }
    let render = image.render_frame(0).map_err(|e| format!("JPEG XL non letto: {e}"))?;
    let mut stream = render.stream();
    let channels = stream.channels() as usize;
    let mut buf = vec![0u8; w as usize * h as usize * channels];
    stream.write_to_buffer::<u8>(&mut buf);
    let out = match channels {
        1 => GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8),
        2 => GrayAlphaImage::from_raw(w, h, buf).map(DynamicImage::ImageLumaA8),
        3 => RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8),
        _ => RgbaImage::from_raw(w, h, buf).map(DynamicImage::ImageRgba8),
    };
    out.ok_or_else(|| "JPEG XL con misure non valide".into())
}

/// AVIF: `quality` va da 40 a 95 come per le altre foto. Con la trasparenza l'alfa viene tenuta.
pub fn avif_bytes(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    // La velocità si abbassa sulle foto grandi: rav1e senza assembly è lento.
    let speed = if w * h > 4_000_000 { 9 } else { 7 };
    let enc = ravif::Encoder::new().with_quality(quality as f32).with_alpha_quality(quality as f32).with_speed(speed);
    let encoded = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let px: Vec<ravif::RGBA8> = rgba.pixels().map(|p| ravif::RGBA8::new(p[0], p[1], p[2], p[3])).collect();
        enc.encode_rgba(ravif::Img::new(&px[..], w, h))
    } else {
        let rgb = img.to_rgb8();
        let px: Vec<ravif::RGB8> = rgb.pixels().map(|p| ravif::RGB8::new(p[0], p[1], p[2])).collect();
        enc.encode_rgb(ravif::Img::new(&px[..], w, h))
    };
    encoded.map(|e| e.avif_file).map_err(|e| format!("AVIF non scritto: {e}"))
}

/// JPEG XL senza perdita (il codificatore in Rust che usiamo non ha la modalità con perdita).
pub fn jxl_bytes(img: &DynamicImage) -> Result<Vec<u8>, String> {
    use zune_core::bit_depth::BitDepth;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::EncoderOptions;
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 2 || h < 2 {
        return Err("JPEG XL vuole un'immagine di almeno 2×2 punti".into());
    }
    let (data, space) = if img.color().has_alpha() { (img.to_rgba8().into_raw(), ColorSpace::RGBA) } else { (img.to_rgb8().into_raw(), ColorSpace::RGB) };
    let opts = EncoderOptions::new(w, h, space, BitDepth::Eight);
    let mut out = Vec::new();
    zune_jpegxl::JxlSimpleEncoder::new(&data, opts).encode(&mut out).map_err(|e| format!("JPEG XL non scritto: {e:?}"))?;
    Ok(out)
}

/// ICO: al massimo 256×256 punti, quindi una foto più grande si rimpicciolisce.
pub fn ico_ready(img: &DynamicImage) -> DynamicImage {
    if img.width() > 256 || img.height() > 256 {
        img.resize(256, 256, image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + 4 * compatible.len();
        let mut b = (size as u32).to_be_bytes().to_vec();
        b.extend_from_slice(b"ftyp");
        b.extend_from_slice(major);
        b.extend_from_slice(&[0, 0, 0, 0]);
        for c in compatible {
            b.extend_from_slice(*c);
        }
        b.extend_from_slice(&[0, 0, 0, 8, b'm', b'e', b't', b'a']);
        b
    }

    #[test]
    fn heic_and_avif_are_told_apart_by_their_brands() {
        assert_eq!(sniff(&ftyp(b"heic", &[b"mif1", b"heic"])), Some("heic"));
        assert_eq!(sniff(&ftyp(b"mif1", &[b"mif1", b"heic"])), Some("heic"));
        // Un AVIF che si dichiara «mif1» e dice «avif» tra i compatibili non è un HEIC.
        assert_eq!(sniff(&ftyp(b"mif1", &[b"mif1", b"avif", b"miaf"])), Some("avif"));
        assert_eq!(sniff(&ftyp(b"avif", &[b"mif1"])), Some("avif"));
        assert_eq!(sniff(&ftyp(b"isom", &[b"mp41"])), Some("mp4"));
    }

    #[test]
    fn heic_photo_opens_with_its_colors() {
        let dir = std::env::temp_dir().join(format!("cedro-heic-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // Anche con l'estensione sbagliata: conta il contenuto.
        for name in ["foto.heic", "foto.jpg"] {
            let path = dir.join(name);
            fs::write(&path, include_bytes!("../tests/fixtures/foto.heic")).unwrap();
            assert_eq!(crate::engine::kind_of(&path), crate::engine::Kind::Image);
            let img = open(&path, true, None).unwrap();
            assert_eq!((img.width(), img.height()), (96, 64));
            assert!(!img.color().has_alpha());
            let rgb = img.to_rgb8();
            let (left, right) = (rgb.get_pixel(10, 32), rgb.get_pixel(85, 32));
            assert!(left[0] > 190 && left[2] < 80, "{left:?}");
            assert!(right[2] > 180 && right[0] < 80, "{right:?}");
        }
        // Un file troncato non manda in crisi il lettore: dice che non lo legge.
        let path = dir.join("rotto.heic");
        fs::write(&path, &include_bytes!("../tests/fixtures/foto.heic")[..300]).unwrap();
        assert!(open(&path, false, None).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
