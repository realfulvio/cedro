use std::path::Path;
use std::sync::OnceLock;

use image::DynamicImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

const DETECTION: &[u8] = include_bytes!("../models/text-detection.onnx");
const RECOGNITION: &[u8] = include_bytes!("../models/text-recognition.onnx");

pub fn image_text(path: &Path) -> Result<String, String> {
    recognize(&open_upright(path)?)
}

/// Apre un'immagine e la raddrizza secondo l'orientamento EXIF, come fanno le foto del telefono.
pub fn open_upright(path: &Path) -> Result<DynamicImage, String> {
    use image::{ImageDecoder, ImageReader};
    let mut decoder = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| format!("immagine non letta: {e}"))?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| format!("immagine non letta: {e}"))?;
    if let Some(o) = orientation {
        image.apply_orientation(o);
    }
    Ok(image)
}


pub fn pdf_pages_text(pdf: &[u8]) -> Result<String, String> {
    let mut body = String::new();
    let mut any = false;
    let pages = render_pdf_images(pdf, |_| true, OCR_SCALE, crate::settings::read_settings().ocr_pages(), |i, image| {
        if i > 0 {
            body.push_str("\n\n");
        }
        body.push_str(&format!("--- pagina {} ---\n", i + 1));
        match recognize(&image) {
            Ok(text) => {
                any = true;
                body.push_str(&text);
            }
            Err(_) => body.push_str("(nessun testo)"),
        }
        Ok(())
    })?;
    if pages == 0 {
        return Err("il PDF non ha pagine".into());
    }
    if !any {
        return Err("nessun testo letto nelle pagine".into());
    }
    Ok(body)
}

/// Legge con l'OCR solo le pagine (da 0) per cui `wanted` è vero, fino al limite delle impostazioni.
/// Per ogni pagina: `None` se non è stata letta, altrimenti il testo (vuoto se non trovato).
pub fn pdf_pages_where(pdf: &[u8], wanted: impl Fn(usize) -> bool) -> Result<Vec<Option<String>>, String> {
    let mut texts = vec![None; pdf_page_count(pdf)?];
    render_pdf_images(pdf, &wanted, OCR_SCALE, crate::settings::read_settings().ocr_pages(), |i, image| {
        texts[i] = Some(recognize(&image).unwrap_or_default());
        Ok(())
    })?;
    Ok(texts)
}

/// Una pagina (da 0) disegnata con il lato lungo di circa `side` pixel, in RGBA: per l'anteprima.
pub fn render_page_rgba(bytes: &[u8], page: usize, side: f32) -> Result<(u32, u32, Vec<u8>), String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non disegnato: {e:?}"))?;
    let pages = pdf.pages();
    let p = pages.get(page).ok_or("pagina che non c'è")?;
    let (w, h) = p.render_dimensions();
    let scale = side / w.max(h).max(1.0);
    let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
    let rendered = render(p, &RenderCache::new(), &InterpreterSettings::default(), &RenderSettings::default(), &pixmap);
    let (w, h) = (rendered.width() as u32, rendered.height() as u32);
    if w == 0 || h == 0 {
        return Err("pagina non disegnata: pagina vuota".into());
    }
    // I pixel così come sono, senza passare da un PNG.
    Ok((w, h, rendered.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha)))
}

/// I pixel di una pagina disegnata, come li darebbe il suo PNG (RGB, o RGBA se c'è trasparenza), senza codificarla.
pub(crate) fn pixmap_image(pixmap: hayro::vello_cpu::Pixmap) -> Result<DynamicImage, String> {
    use hayro::vello_cpu::Pixels;
    let (w, h) = (pixmap.width() as u32, pixmap.height() as u32);
    if w == 0 || h == 0 {
        return Err("pagina non disegnata: pagina vuota".into());
    }
    let image = match pixmap.try_take_rgb8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha) {
        Pixels::Rgb8(data) => image::RgbImage::from_raw(w, h, data).map(DynamicImage::ImageRgb8),
        Pixels::Rgba8(data) => image::RgbaImage::from_raw(w, h, data).map(DynamicImage::ImageRgba8),
    };
    image.ok_or_else(|| "pagina non disegnata".to_string())
}

pub fn pdf_page_count(bytes: &[u8]) -> Result<usize, String> {
    use hayro::hayro_syntax::Pdf;
    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non disegnato: {e:?}"))?;
    Ok(pdf.pages().len())
}

/// Una riga letta dall'OCR, con il riquadro in pixel dell'immagine (origine in alto a sinistra).
#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Il testo di un'immagine, con le righe unite in paragrafi.
fn recognize(image: &DynamicImage) -> Result<String, String> {
    let text = paragraphs_from_lines(&image_lines(image)?);
    if text.trim().is_empty() {
        Err("nessun testo riconosciuto".into())
    } else {
        Ok(text)
    }
}

/// Le righe di testo di un'immagine. Su Windows legge l'OCR di Windows in italiano,
/// su Linux Tesseract nella lingua scelta se c'è; altrimenti il modello latino incluso.
pub fn image_lines(image: &DynamicImage) -> Result<Vec<Line>, String> {
    #[cfg(windows)]
    if let Ok(lines) = windows_lines(image) {
        return Ok(lines);
    }
    #[cfg(not(windows))]
    if let Some(lines) = tesseract_lines(image) {
        return Ok(lines);
    }
    ocrs_lines(image)
}

/// Unisce le righe in paragrafi: un paragrafo finisce dopo una riga corta
/// o prima di uno spazio verticale più alto di una riga.
pub fn paragraphs_from_lines(lines: &[Line]) -> String {
    let lines: Vec<&Line> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let right = lines.iter().map(|l| l.x + l.w).fold(0.0f32, f32::max);
    let left = lines.iter().map(|l| l.x).fold(f32::MAX, f32::min);
    let full = (right - left).max(1.0);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let text = line.text.trim();
        if i > 0 {
            let prev = lines[i - 1];
            let gap = line.y - (prev.y + prev.h);
            let short = prev.x + prev.w < left + full * 0.8;
            if short || gap > prev.h.max(line.h) * 0.9 {
                out.push('\n');
            } else {
                join_line(&mut out, text);
                continue;
            }
        }
        out.push_str(text);
    }
    out
}

/// Unisce le righe di un testo estratto dal PDF, senza la loro posizione:
/// una riga lunga continua nella successiva, una corta o vuota chiude il paragrafo.
pub fn paragraphs_from_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0).max(1);
    let mut out = String::new();
    let mut open = false;
    for line in lines {
        if line.is_empty() {
            open = false;
            continue;
        }
        if open {
            join_line(&mut out, line);
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
        open = line.chars().count() * 10 >= longest * 7;
    }
    out
}

fn join_line(out: &mut String, next: &str) {
    let hyphen = out.ends_with('-')
        && out[..out.len() - 1].chars().last().is_some_and(char::is_alphabetic)
        && next.chars().next().is_some_and(char::is_lowercase);
    if hyphen {
        out.pop();
    } else if !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(next);
}

fn ocrs_lines(image: &DynamicImage) -> Result<Vec<Line>, String> {
    use ocrs::TextItem;
    crate::accel::apply();
    // Il modello latino legge meglio pagine larghe circa 1000 pixel.
    let factor = (1000.0 / image.width().max(1) as f32).min(1.0);
    let small;
    let image = if factor < 1.0 {
        small = image.resize(
            (image.width() as f32 * factor) as u32,
            (image.height() as f32 * factor) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        &small
    } else {
        image
    };
    let engine = engine()?;
    let rgb = image.to_rgb8();
    let source = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| e.to_string())?;
    let input = engine.prepare_input(source).map_err(|e| e.to_string())?;
    let words = engine.detect_words(&input).map_err(|e| e.to_string())?;
    let rects = engine.find_text_lines(&input, &words);
    let found = engine.recognize_text(&input, &rects).map_err(|e| e.to_string())?;
    Ok(found
        .into_iter()
        .flatten()
        .map(|line| {
            let r = line.bounding_rect();
            Line {
                text: line.to_string(),
                x: r.left() as f32 / factor,
                y: r.top() as f32 / factor,
                w: r.width() as f32 / factor,
                h: r.height() as f32 / factor,
            }
        })
        .collect())
}

/// Le lingue che Tesseract ha installate.
#[cfg(not(windows))]
fn tesseract_langs() -> &'static [String] {
    static LANGS: OnceLock<Vec<String>> = OnceLock::new();
    LANGS.get_or_init(|| {
        std::process::Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().skip(1).map(|l| l.trim().to_string()).collect())
            .unwrap_or_default()
    })
}

/// I modelli di Tesseract per la lingua scelta nelle impostazioni; `None` se mancano (si usa il modello latino incluso).
#[cfg(not(windows))]
fn tesseract_lang(setting: &str, installed: &[String]) -> Option<&'static str> {
    let has = |l: &str| installed.iter().any(|x| x == l);
    match setting {
        "en" if has("eng") => Some("eng"),
        "en" => None,
        // «Automatica» sceglie da sola tra italiano e inglese.
        "auto" if has("ita") && has("eng") => Some("ita+eng"),
        _ if has("ita") => Some("ita"),
        _ => None,
    }
}

#[cfg(not(windows))]
fn tesseract_lines(image: &DynamicImage) -> Option<Vec<Line>> {
    use std::process::Command;
    let lang = tesseract_lang(&crate::settings::read_settings().ocr_lang, tesseract_langs())?;
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    // BMP: niente compressione da fare e disfare per un file che vive un istante.
    let tmp = std::env::temp_dir().join(format!("cedro-ocr-{}-{n}.bmp", std::process::id()));
    image.to_rgb8().save(&tmp).ok()?;
    let out = Command::new("tesseract")
        .arg(&tmp)
        .arg("-")
        .args(["-l", lang, "--psm", "3", "tsv"])
        .output();
    let _ = std::fs::remove_file(&tmp);
    let out = out.ok().filter(|o| o.status.success())?;
    Some(parse_tsv(&String::from_utf8_lossy(&out.stdout)))
}

/// Le righe dal TSV di Tesseract: livello 4 per il riquadro della riga, livello 5 per le parole.
#[cfg(not(windows))]
fn parse_tsv(tsv: &str) -> Vec<Line> {
    let mut lines: Vec<(String, Line)> = Vec::new();
    for row in tsv.lines().skip(1) {
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let key = format!("{}-{}-{}-{}", cols[1], cols[2], cols[3], cols[4]);
        let num = |i: usize| cols[i].trim().parse::<f32>().unwrap_or(0.0);
        match cols[0] {
            "4" => lines.push((key, Line { text: String::new(), x: num(6), y: num(7), w: num(8), h: num(9) })),
            "5" => {
                let word = cols[11].trim();
                if word.is_empty() {
                    continue;
                }
                if let Some((_, line)) = lines.iter_mut().rev().find(|(k, _)| *k == key) {
                    if !line.text.is_empty() {
                        line.text.push(' ');
                    }
                    line.text.push_str(word);
                }
            }
            _ => {}
        }
    }
    lines.into_iter().map(|(_, l)| l).filter(|l| !l.text.is_empty()).collect()
}

/// Vero se Windows ha la lingua italiana per l'OCR.
#[cfg(windows)]
pub fn windows_italian_installed() -> bool {
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine as WinOcr;
    Language::CreateLanguage(&HSTRING::from("it")).and_then(|l| WinOcr::IsLanguageSupported(&l)).unwrap_or(false)
}

/// L'OCR di Windows in italiano. Se manca la lingua, quello delle lingue dell'utente.
#[cfg(windows)]
fn windows_engine() -> Result<windows::Media::Ocr::OcrEngine, String> {
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine as WinOcr;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    let e = |e: windows::core::Error| format!("OCR di Windows: {e}");
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    // «Automatica» e «Italiano» provano l'italiano; «Inglese» l'inglese.
    let tag = if crate::settings::read_settings().ocr_lang == "en" { "en-US" } else { "it" };
    let wanted = Language::CreateLanguage(&HSTRING::from(tag)).map_err(e)?;
    if WinOcr::IsLanguageSupported(&wanted).map_err(e)? {
        WinOcr::TryCreateFromLanguage(&wanted).map_err(e)
    } else {
        WinOcr::TryCreateFromUserProfileLanguages().map_err(e)
    }
}

#[cfg(windows)]
fn windows_lines(image: &DynamicImage) -> Result<Vec<Line>, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine as WinOcr;
    use windows::Security::Cryptography::CryptographicBuffer;

    let e = |e: windows::core::Error| format!("OCR di Windows: {e}");
    let engine = windows_engine()?;
    let max = WinOcr::MaxImageDimension().map_err(e)? as f32;
    let factor = (max / image.width().max(image.height()).max(1) as f32).min(1.0);
    let small;
    let image = if factor < 1.0 {
        small = image.resize(
            (image.width() as f32 * factor) as u32,
            (image.height() as f32 * factor) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        &small
    } else {
        image
    };
    let mut bgra = image.to_rgba8();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    let buffer = CryptographicBuffer::CreateFromByteArray(bgra.as_raw()).map_err(e)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        bgra.width() as i32,
        bgra.height() as i32,
    )
    .map_err(e)?;
    let result = engine.RecognizeAsync(&bitmap).map_err(e)?.get().map_err(e)?;
    let mut lines = Vec::new();
    for line in result.Lines().map_err(e)? {
        let text = line.Text().map_err(e)?.to_string();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, 0.0f32, 0.0f32);
        for word in line.Words().map_err(e)? {
            let r = word.BoundingRect().map_err(e)?;
            x0 = x0.min(r.X);
            y0 = y0.min(r.Y);
            x1 = x1.max(r.X + r.Width);
            y1 = y1.max(r.Y + r.Height);
        }
        if x1 > x0 {
            lines.push(Line { text, x: x0 / factor, y: y0 / factor, w: (x1 - x0) / factor, h: (y1 - y0) / factor });
        }
    }
    Ok(lines)
}

fn engine() -> Result<&'static OcrEngine, String> {
    static ENGINE: OnceLock<Result<OcrEngine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let detection = Model::load(DETECTION.to_vec()).map_err(|e| format!("modello OCR: {e}"))?;
            let recognition = Model::load(RECOGNITION.to_vec()).map_err(|e| format!("modello OCR: {e}"))?;
            OcrEngine::new(OcrEngineParams {
                detection_model: Some(detection),
                recognition_model: Some(recognition),
                ..Default::default()
            })
            .map_err(|e| format!("OCR non pronto: {e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Circa 300 dpi: la risoluzione con cui l'OCR legge meglio una pagina.
const OCR_SCALE: f32 = 300.0 / 72.0;

/// Tutte le pagine del PDF disegnate in PNG, una alla volta, con la scala scelta (1.0 = 72 dpi, 2.0 = 144 dpi).
pub fn pdf_page_pngs_scaled(bytes: &[u8], scale: f32, wanted: impl Fn(usize) -> bool, each: impl FnMut(usize, Vec<u8>) -> Result<(), String>) -> Result<usize, String> {
    render_pdf(bytes, wanted, scale, usize::MAX, each)
}

/// Disegna le pagine volute, una alla volta, e le passa a `each` come PNG:
/// in memoria resta una pagina sola. Ritorna quante pagine ha disegnato.
fn render_pdf(
    bytes: &[u8],
    wanted: impl Fn(usize) -> bool,
    scale: f32,
    max: usize,
    mut each: impl FnMut(usize, Vec<u8>) -> Result<(), String>,
) -> Result<usize, String> {
    render_pdf_pixmaps(bytes, wanted, scale, max, |i, rendered| each(i, rendered.into_png().map_err(|e| format!("pagina non disegnata: {e}"))?))
}

/// Come `render_pdf`, ma passa a `each` i pixel della pagina: per l'OCR non serve un PNG da decodificare.
fn render_pdf_images(
    bytes: &[u8],
    wanted: impl Fn(usize) -> bool,
    scale: f32,
    max: usize,
    mut each: impl FnMut(usize, DynamicImage) -> Result<(), String>,
) -> Result<usize, String> {
    render_pdf_pixmaps(bytes, wanted, scale, max, |i, rendered| each(i, pixmap_image(rendered)?))
}

fn render_pdf_pixmaps(
    bytes: &[u8],
    wanted: impl Fn(usize) -> bool,
    scale: f32,
    max: usize,
    mut each: impl FnMut(usize, hayro::vello_cpu::Pixmap) -> Result<(), String>,
) -> Result<usize, String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non disegnato: {e:?}"))?;
    let cache = RenderCache::new();
    let interpreter = InterpreterSettings::default();
    let settings = RenderSettings::default();
    let mut done = 0;
    let totale = pdf.pages().iter().enumerate().filter(|(i, _)| wanted(*i)).take(max).count().max(1);
    for (i, page) in pdf.pages().iter().enumerate().filter(|(i, _)| wanted(*i)).take(max) {
        // Una pagina enorme (una scansione salvata a 72 dpi) non supera un A4 a 300 dpi.
        let (w, h) = page.render_dimensions();
        let scale = scale.min(3508.0 / w.max(h).max(1.0));
        let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
        let rendered = render(page, &cache, &interpreter, &settings, &pixmap);
        each(i, rendered)?;
        done += 1;
        crate::progress::report(done as f32 / totale as f32);
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn tesseract_reads_the_language_chosen_in_the_settings() {
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let both = langs(&["eng", "ita", "osd"]);
        assert_eq!(tesseract_lang("auto", &both), Some("ita+eng"));
        assert_eq!(tesseract_lang("it", &both), Some("ita"));
        assert_eq!(tesseract_lang("en", &both), Some("eng"));
        // Senza il modello voluto si usa quello latino incluso.
        assert_eq!(tesseract_lang("en", &langs(&["ita"])), None);
        assert_eq!(tesseract_lang("auto", &langs(&["ita"])), Some("ita"));
        assert_eq!(tesseract_lang("auto", &langs(&["eng"])), None);
        assert_eq!(tesseract_lang("it", &[]), None);
    }

    #[test]
    fn a_rendered_page_gives_the_same_pixels_as_its_png() {
        use hayro::vello_cpu::color::palette::css::WHITE;
        use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
        let pdf = crate::pdf::pdf_from_text("Una riga di prova\nE un'altra, più lunga, per avere dei grigi.");
        let doc = hayro::hayro_syntax::Pdf::new(pdf).unwrap();
        let pages = doc.pages();
        let page = pages.get(0).unwrap();
        let draw = || {
            let pixmap = PixmapSettings { x_scale: 1.5, y_scale: 1.5, bg_color: WHITE };
            render(page, &RenderCache::new(), &hayro::hayro_interpret::InterpreterSettings::default(), &RenderSettings::default(), &pixmap)
        };
        let from_png = image::load_from_memory(&draw().into_png().unwrap()).unwrap();
        let direct = pixmap_image(draw()).unwrap();
        assert_eq!(from_png.color(), direct.color());
        assert_eq!(from_png.as_bytes(), direct.as_bytes());
        let (w, h, rgba) = render_page_rgba(&crate::pdf::pdf_from_text("x"), 0, 100.0).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.chunks_exact(4).all(|p| p[3] == 255));
    }
}
