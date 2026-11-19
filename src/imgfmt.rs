//! Formati di foto che il crate `image` non legge o non scrive da solo, tutti in Rust:
//! SVG (resvg), AVIF (ravif per scrivere, avif-decode con rav1d per leggere) e JPEG XL
//! (jxl-oxide per leggere, zune-jpegxl per scrivere, senza perdita) e HEIC/HEIF delle foto dell'iPhone
//! (heifer per leggere, con i colori portati a sRGB). Qui c'è anche il riconoscimento
//! del tipo di file dal contenuto.

#[cfg(test)]
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
    let _ = crate::input_file::File::open(path).and_then(|f| f.take(132).read_to_end(&mut buf));
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
    if (kind == "tiff" || ext == "mrxs" || ext == "vsi") && crate::svs::is_slide(path) {
        return crate::svs::open_image(path, target_side);
    }
    // Microscopia (OME-TIFF, ImageJ, 16 bit): canali a colori e contrasto, non i 16 bit schiacciati.
    if (matches!(kind, "tiff" | "nd2" | "lif" | "czi") || ext == "vsi") && crate::microscopia::is_stack(path) {
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
        _ => reader(path)?
            .decode()
            .map_err(|e| format!("immagine non letta: {e}")),
    }
}

/// Stessa precedenza di ImageReader::open: estensione come fallback, magic bytes se riconosciuti.
/// Serve anche a TGA, che non ha una firma sufficiente per with_guessed_format da solo.
pub(crate) fn reader(path:&Path)->Result<image::ImageReader<std::io::BufReader<crate::input_file::File>>,String> {
    let mut reader=image::ImageReader::new(std::io::BufReader::new(crate::input_file::File::open(path).map_err(|e|e.to_string())?));
    if let Ok(format)=image::ImageFormat::from_path(path){reader.set_format(format);}
    reader.with_guessed_format().map_err(|e|e.to_string())
}

/// I caratteri del sistema, cercati una volta sola: leggerli tutti a ogni SVG costava decimi di secondo
/// per file (una cartella di icone, le anteprime). Servono anche ai PDF da Word ed Excel.
pub fn system_fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = std::sync::OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            #[cfg(not(target_os = "android"))]
            db.load_system_fonts();
            #[cfg(target_os = "android")]
            for dir in ["/system/fonts", "/product/fonts", "/system_ext/fonts"] {
                db.load_fonts_dir(dir);
            }
            std::sync::Arc::new(db)
        })
        .clone()
}

/// Un SVG disegnato su una tela trasparente: il lato lungo è `target_side`, o la misura del disegno
/// portata ad almeno 1024 px, perché un logo da 24 px sia utile.
fn open_svg(path: &Path, target_side: Option<u32>) -> Result<DynamicImage, String> {
    let data = crate::input_file::read(path).map_err(|e| e.to_string())?;
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb = system_fonts();
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
    let data = crate::input_file::read(path).map_err(|e| e.to_string())?;
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
    let data = crate::input_file::read(path).map_err(|e| e.to_string())?;
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
    let image = jxl_oxide::JxlImage::builder().read(crate::input_file::File::open(path).map_err(|e|e.to_string())?).map_err(|e| format!("JPEG XL non letto: {e}"))?;
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
        ridimensiona_entro(img, 256, 256, image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    }
}

/// Le misure di un'immagine che sta dentro `w`×`h` senza deformarsi (come `DynamicImage::resize`).
pub fn misure_entro(width: u32, height: u32, w: u32, h: u32) -> (u32, u32) {
    let ratio = (w as f64 / width.max(1) as f64).min(h as f64 / height.max(1) as f64);
    let nw = ((width as f64 * ratio).round() as u64).clamp(1, u32::MAX as u64) as u32;
    let nh = ((height as f64 * ratio).round() as u64).clamp(1, u32::MAX as u64) as u32;
    (nw, nh)
}

/// Come `DynamicImage::resize`: dentro `w`×`h`, senza deformare, ma con `ridimensiona`.
pub fn ridimensiona_entro(img: &DynamicImage, w: u32, h: u32, filtro: image::imageops::FilterType) -> DynamicImage {
    if (w, h) == (img.width(), img.height()) {
        return img.clone();
    }
    let (nw, nh) = misure_entro(img.width(), img.height(), w, h);
    ridimensiona(img, nw, nh, filtro)
}

/// Come `DynamicImage::thumbnail` (media dei punti che finiscono in ciascun punto nuovo), ma con `ridimensiona`.
pub fn miniatura(img: &DynamicImage, w: u32, h: u32) -> DynamicImage {
    let (nw, nh) = misure_entro(img.width(), img.height(), w, h);
    if nw >= img.width() || nh >= img.height() {
        return img.thumbnail(w, h);
    }
    ridimensiona_con(img, nw, nh, pic_scale::ResamplingFunction::Box).unwrap_or_else(|| img.thumbnail(w, h))
}

/// Ridimensiona a `w`×`h` con pic-scale (SIMD e più thread, la stessa libreria con cui hayro scala le immagini
/// dei PDF): 10–20 volte più veloce di `image::imageops::resize`, con lo stesso risultato (±1 sul valore di un canale).
/// La trasparenza si scala premoltiplicata: niente aloni scuri sui bordi. I formati a 16 bit e a virgola mobile
/// passano da `image`.
pub fn ridimensiona(img: &DynamicImage, w: u32, h: u32, filtro: image::imageops::FilterType) -> DynamicImage {
    ridimensiona_con(img, w.max(1), h.max(1), filtro_pic(filtro)).unwrap_or_else(|| img.resize_exact(w, h, filtro))
}

fn ridimensiona_con(img: &DynamicImage, w: u32, h: u32, f: pic_scale::ResamplingFunction) -> Option<DynamicImage> {
    match img {
        DynamicImage::ImageRgb8(p) => RgbImage::from_raw(w, h, scala::<3>(p.as_raw(), p.width(), p.height(), w, h, f)?).map(DynamicImage::ImageRgb8),
        DynamicImage::ImageRgba8(p) => RgbaImage::from_raw(w, h, scala::<4>(p.as_raw(), p.width(), p.height(), w, h, f)?).map(DynamicImage::ImageRgba8),
        DynamicImage::ImageLuma8(p) => GrayImage::from_raw(w, h, scala::<1>(p.as_raw(), p.width(), p.height(), w, h, f)?).map(DynamicImage::ImageLuma8),
        DynamicImage::ImageLumaA8(p) => GrayAlphaImage::from_raw(w, h, scala::<2>(p.as_raw(), p.width(), p.height(), w, h, f)?).map(DynamicImage::ImageLumaA8),
        _ => None,
    }
}

fn filtro_pic(filtro: image::imageops::FilterType) -> pic_scale::ResamplingFunction {
    use image::imageops::FilterType;
    use pic_scale::ResamplingFunction as R;
    match filtro {
        FilterType::Nearest => R::Nearest,
        FilterType::Triangle => R::Bilinear,
        FilterType::CatmullRom => R::CatmullRom,
        FilterType::Gaussian => R::Gaussian,
        FilterType::Lanczos3 => R::Lanczos3,
    }
}

/// `ridimensiona` per un'immagine RGB (le foto dei vetrini, le figure).
pub fn ridimensiona_rgb(img: &RgbImage, w: u32, h: u32, filtro: image::imageops::FilterType) -> RgbImage {
    let (w, h) = (w.max(1), h.max(1));
    scala::<3>(img.as_raw(), img.width(), img.height(), w, h, filtro_pic(filtro))
        .and_then(|px| RgbImage::from_raw(w, h, px))
        .unwrap_or_else(|| image::imageops::resize(img, w, h, filtro))
}

/// `ridimensiona` per un'immagine in grigi (le pagine per l'OCR).
pub fn ridimensiona_grigio(img: &GrayImage, w: u32, h: u32, filtro: image::imageops::FilterType) -> GrayImage {
    let (w, h) = (w.max(1), h.max(1));
    scala::<1>(img.as_raw(), img.width(), img.height(), w, h, filtro_pic(filtro))
        .and_then(|px| GrayImage::from_raw(w, h, px))
        .unwrap_or_else(|| image::imageops::resize(img, w, h, filtro))
}

/// I pixel a `N` canali da `sw`×`sh` a `dw`×`dh` (con 2 e 4 canali l'ultimo è la trasparenza, scalata premoltiplicata).
fn scala<const N: usize>(px: &[u8], sw: u32, sh: u32, dw: u32, dh: u32, f: pic_scale::ResamplingFunction) -> Option<Vec<u8>> {
    use pic_scale::{ImageSize, ImageStore, ImageStoreMut, Scaler, ThreadingPolicy};
    let (sw, sh, dw, dh) = (sw as usize, sh as usize, dw as usize, dh as usize);
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || px.len() != sw.checked_mul(sh)?.checked_mul(N)? {
        return None;
    }
    let mut out = vec![0u8; dw.checked_mul(dh)?.checked_mul(N)?];
    let mut scaler = Scaler::new(f);
    scaler.set_threading_policy(ThreadingPolicy::Adaptive);
    let (from, to) = (ImageSize::new(sw, sh), ImageSize::new(dw, dh));
    macro_rules! con {
        ($n:literal, $plan:expr) => {{
            let src = ImageStore::<u8, $n>::from_slice(px, sw, sh).ok()?;
            let mut dst = ImageStoreMut::<u8, $n>::from_slice(&mut out, dw, dh).ok()?;
            $plan.ok()?.resample(&src, &mut dst).ok()?;
        }};
    }
    match N {
        1 => con!(1, scaler.plan_planar_resampling(from, to)),
        2 => con!(2, scaler.plan_gray_alpha_resampling(from, to, true)),
        3 => con!(3, scaler.plan_rgb_resampling(from, to)),
        4 => con!(4, scaler.plan_rgba_resampling(from, to, true)),
        _ => return None,
    }
    Some(out)
}

/// Un JPEG da pixel RGB (3 byte a punto) o grigi (1 byte), con il codificatore jpeg-encoder (AVX2 dove c'è):
/// circa il doppio più veloce di quello di `image`, a parità di qualità e con lo stesso campionamento 4:4:4
/// (i colori non si impastano). `stretto` (per «Comprimi», dove conta il peso): il codificatore di `image` di
/// sempre, qualche percento più leggero e senza tenere in memoria tutta la foto da codificare (jpeg-encoder con
/// le tabelle su misura ne terrebbe una copia in più, 100 MB per una foto da 12 MP).
pub fn jpeg(pixel: &[u8], w: u32, h: u32, grigio: bool, qualita: u8, stretto: bool) -> Result<Vec<u8>, String> {
    let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
        return Err(format!("JPEG troppo grande: {w} × {h} punti (al massimo 65 535 per lato)"));
    };
    let canali = if grigio { 1 } else { 3 };
    if pixel.len() != w as usize * h as usize * canali {
        return Err("JPEG: pixel non validi".into());
    }
    let mut out = Vec::with_capacity(pixel.len() / 8 + 1024);
    if stretto {
        let tipo = if grigio { image::ExtendedColorType::L8 } else { image::ExtendedColorType::Rgb8 };
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, qualita.clamp(1, 100)).encode(pixel, w, h, tipo).map_err(|e| format!("JPEG non scritto: {e}"))?;
        return Ok(out);
    }
    let mut enc = jpeg_encoder::Encoder::new(&mut out, qualita.clamp(1, 100));
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    let tipo = if grigio { jpeg_encoder::ColorType::Luma } else { jpeg_encoder::ColorType::Rgb };
    enc.encode(pixel, w16, h16, tipo).map_err(|e| format!("JPEG non scritto: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    #[cfg(unix)]
    fn tga_extension_survives_grant_reader_and_magic_still_wins() {
        use std::os::fd::AsRawFd;
        let dir=std::env::temp_dir().join(format!("cedro-tga-grant-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let plain=dir.join("source.tga");
        let expected=image::RgbImage::from_pixel(3,2,image::Rgb([17,83,201]));
        expected.save_with_format(&plain,image::ImageFormat::Tga).unwrap();
        let file=std::sync::Arc::new(fs::File::open(&plain).unwrap());let fd=file.as_raw_fd();
        let alias=dir.join("granted.tga");std::os::unix::fs::symlink(format!("/proc/self/fd/{fd}"),&alias).unwrap();crate::input_file::register(&alias,fd,file);
        for path in [&plain,&alias] {
            assert_eq!(open(path,false,None).unwrap().to_rgb8(),expected);
            assert_eq!(crate::ocr::open_upright(path).unwrap().to_rgb8(),expected);
            assert_eq!(crate::ui::media::image_thumb(path,64).unwrap().1,(3,2));
        }
        let wrong=dir.join("png-named-tga.tga");expected.save_with_format(&wrong,image::ImageFormat::Png).unwrap();
        assert_eq!(open(&wrong,false,None).unwrap().to_rgb8(),expected);
        fs::remove_dir_all(dir).unwrap();
    }

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

    fn sfumata(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) * 7 % 256) as u8]))
    }

    #[test]
    fn ridimensiona_come_image() {
        let img = DynamicImage::ImageRgb8(sfumata(400, 300));
        for f in [image::imageops::FilterType::Triangle, image::imageops::FilterType::CatmullRom, image::imageops::FilterType::Lanczos3] {
            let a = ridimensiona(&img, 133, 100, f).to_rgb8();
            let b = img.resize_exact(133, 100, f).to_rgb8();
            assert_eq!(a.dimensions(), (133, 100));
            // I bordi a righe (canale blu) differiscono di poco: conta che la media resti quella.
            let diff: f64 = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.as_raw().len() as f64;
            assert!(diff < 2.0, "{f:?}: differenza media {diff}");
        }
        assert_eq!(ridimensiona_entro(&img, 100, 100, image::imageops::FilterType::Lanczos3).dimensions(), (100, 75));
        assert_eq!(miniatura(&img, 40, 40).dimensions(), (40, 30));
        // Più grande dell'originale: la miniatura resta com'era (come `thumbnail`).
        assert_eq!(miniatura(&img, 800, 800).dimensions(), img.thumbnail(800, 800).dimensions());
    }

    #[test]
    fn ridimensiona_senza_aloni_scuri() {
        // Metà rossa piena, metà trasparente (nera sotto): il bordo non deve scurirsi.
        let img = RgbaImage::from_fn(64, 64, |x, _| if x < 32 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 0, 0]) });
        let out = ridimensiona(&DynamicImage::ImageRgba8(img), 21, 21, image::imageops::FilterType::Lanczos3).to_rgba8();
        for p in out.pixels().filter(|p| p[3] > 40) {
            assert!(p[0] > 200, "alone scuro: {p:?}");
        }
    }

    #[test]
    fn jpeg_si_rilegge_uguale() {
        let img = sfumata(120, 80);
        for stretto in [false, true] {
            let b = jpeg(img.as_raw(), 120, 80, false, 90, stretto).unwrap();
            let back = image::load_from_memory(&b).unwrap().to_rgb8();
            assert_eq!(back.dimensions(), (120, 80));
            let mse: f64 = img.as_raw().iter().zip(back.as_raw()).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum::<f64>() / img.as_raw().len() as f64;
            let psnr = 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10();
            assert!(psnr > 30.0, "PSNR {psnr}");
        }
        let grigio = GrayImage::from_fn(50, 40, |x, y| image::Luma([(x * 5 + y) as u8]));
        let b = jpeg(grigio.as_raw(), 50, 40, true, 80, false).unwrap();
        assert_eq!(image::load_from_memory(&b).unwrap().color(), image::ColorType::L8);
        assert!(jpeg(&[0; 12], 2, 2, false, 90, false).is_ok());
        assert!(jpeg(&[0; 11], 2, 2, false, 90, false).is_err());
        assert!(jpeg(&[], 70_000, 0, false, 90, false).is_err());
    }
}
