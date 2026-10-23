use crate::pdf::{pdf_from_jpeg, pdf_from_text};
use crate::zipstore::zip_store;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageFormat};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CedroError(pub String);

impl std::fmt::Display for CedroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CedroError {}

pub type CedroResult<T> = Result<T, CedroError>;

fn err(msg: impl Into<String>) -> CedroError {
    CedroError(msg.into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
    Audio,
    Video,
    Pdf,
    Doc,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Convert,
    Tools,
    Actions,
}

#[derive(Clone, Copy)]
pub struct Slice {
    pub id: &'static str,
    pub label: &'static str,
}

const ACTIONS: &[Slice] = &[
    Slice { id: "dup", label: "Duplica" },
    Slice { id: "path", label: "Percorso" },
    Slice { id: "zip", label: "ZIP" },
];
const IMAGE_CONVERT: &[Slice] = &[
    Slice { id: "png", label: "PNG" },
    Slice { id: "jpeg", label: "JPEG" },
    Slice { id: "webp", label: "WebP" },
    Slice { id: "bmp", label: "BMP" },
    Slice { id: "gif", label: "GIF" },
    Slice { id: "tiff", label: "TIFF" },
    Slice { id: "pdf", label: "PDF" },
];
const IMAGE_TOOLS: &[Slice] = &[
    Slice { id: "compress", label: "Comprimi" },
    Slice { id: "half", label: "50%" },
    Slice { id: "gray", label: "B/N" },
    Slice { id: "rot90", label: "Ruota" },
    Slice { id: "strip", label: "Metadati" },
    Slice { id: "ocr", label: "OCR" },
];
const TEXT_CONVERT: &[Slice] = &[
    Slice { id: "txt", label: "TXT" },
    Slice { id: "md", label: "MD" },
    Slice { id: "csv", label: "CSV" },
    Slice { id: "pdf", label: "PDF" },
];
const TEXT_TOOLS: &[Slice] = &[
    Slice { id: "upper", label: "Maiuscolo" },
    Slice { id: "lower", label: "Minuscolo" },
];
const AUDIO_CONVERT: &[Slice] = &[
    Slice { id: "wav", label: "WAV" },
    Slice { id: "mp3", label: "MP3" },
];
const VIDEO_CONVERT: &[Slice] = &[Slice { id: "mp4", label: "MP4" }];
const DOC_CONVERT: &[Slice] = &[Slice { id: "pdf", label: "PDF" }];
const PDF_CONVERT: &[Slice] = &[
    Slice { id: "jpg", label: "JPG" },
    Slice { id: "docx", label: "Word OCR" },
];
const PDF_TOOLS: &[Slice] = &[Slice { id: "ocr", label: "OCR" }];
const EMPTY: &[Slice] = &[Slice { id: "none", label: "Niente qui" }];

pub fn slices(kind: Kind, mode: Mode) -> &'static [Slice] {
    match (kind, mode) {
        (_, Mode::Actions) => ACTIONS,
        (Kind::Image, Mode::Convert) => IMAGE_CONVERT,
        (Kind::Image, Mode::Tools) => IMAGE_TOOLS,
        (Kind::Text, Mode::Convert) => TEXT_CONVERT,
        (Kind::Text, Mode::Tools) => TEXT_TOOLS,
        (Kind::Audio, Mode::Convert) => AUDIO_CONVERT,
        (Kind::Video, Mode::Convert) => VIDEO_CONVERT,
        (Kind::Doc, Mode::Convert) => DOC_CONVERT,
        (Kind::Pdf, Mode::Convert) => PDF_CONVERT,
        (Kind::Pdf, Mode::Tools) => PDF_TOOLS,
        _ => EMPTY,
    }
}

/// Testo di un PDF: quello incorporato, e l'OCR solo per le pagine che ne sono prive.
/// Ritorna il testo, le pagine lette via OCR e quelle da leggere rimaste fuori dal limite.
fn pdf_text_with_ocr(path: &Path) -> Result<(String, usize, usize), String> {
    let mut pages: Vec<String> =
        crate::doctext::pdf_page_texts(path)?.iter().map(|p| crate::ocr::paragraphs_from_text(p)).collect();
    let blank: Vec<bool> = pages.iter().map(|p| p.is_empty()).collect();
    let mut ocr_pages = 0;
    let mut skipped = 0;
    let mut ocr_error = None;
    if blank.iter().any(|b| *b) {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        match crate::ocr::pdf_pages_where(&bytes, |i| blank.get(i).copied().unwrap_or(false)) {
            Ok(read) => {
                for (page, text) in pages.iter_mut().zip(read) {
                    match text {
                        Some(text) if !text.is_empty() => {
                            *page = text;
                            ocr_pages += 1;
                        }
                        None if page.is_empty() => skipped += 1,
                        _ => {}
                    }
                }
            }
            Err(e) => ocr_error = Some(e),
        }
    }
    let text = pages.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n");
    if text.is_empty() {
        return Err(match ocr_error {
            Some(e) => format!("questo PDF non ha testo da copiare; OCR: {e}"),
            None => "questo PDF non ha testo da copiare né da leggere con l'OCR".into(),
        });
    }
    Ok((text, ocr_pages, skipped))
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn kind_of(path: &Path) -> Kind {
    match extension(path).as_str() {
        "pdf" => Kind::Pdf,
        "doc" | "docx" | "odt" | "rtf" => Kind::Doc,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "ico" => Kind::Image,
        "txt" | "md" | "csv" | "tsv" | "json" | "log" | "xml" | "html" => Kind::Text,
        "wav" | "mp3" | "ogg" | "m4a" | "aac" | "flac" => Kind::Audio,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => Kind::Video,
        _ => Kind::Other,
    }
}

pub struct Output {
    pub path: PathBuf,
    pub message: String,
}

pub fn mode_from_vk(vk: u32) -> Option<Mode> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(Mode::Convert),
        0x11 | 0xA2 | 0xA3 => Some(Mode::Tools),
        0x12 | 0xA4 | 0xA5 => Some(Mode::Actions),
        _ => None,
    }
}

fn stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string()
}

pub fn unique_output(dir: &Path, file_stem: &str, ext: &str) -> PathBuf {
    let mut name = format!("{file_stem}.{ext}");
    let mut i = 2u32;
    while dir.join(&name).exists() {
        name = format!("{file_stem}-{i}.{ext}");
        i += 1;
    }
    dir.join(name)
}

fn parent_dir(path: &Path) -> CedroResult<&Path> {
    path.parent().filter(|p| !p.as_os_str().is_empty()).or(Some(Path::new("."))).ok_or_else(|| err("cartella mancante"))
}

fn save_image(img: &DynamicImage, path: &Path, format: ImageFormat, quality: u8) -> CedroResult<()> {
    if format == ImageFormat::Jpeg {
        let rgb = img.to_rgb8();
        let mut file = fs::File::create(path).map_err(|e| err(e.to_string()))?;
        let mut enc = JpegEncoder::new_with_quality(&mut file, quality);
        enc.encode(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
            .map_err(|e| err(e.to_string()))?;
        return Ok(());
    }
    img.save_with_format(path, format).map_err(|e| err(e.to_string()))
}

fn load_image(path: &Path) -> CedroResult<DynamicImage> {
    crate::ocr::open_upright(path).map_err(err)
}

fn jpeg_bytes(img: &DynamicImage, quality: u8) -> CedroResult<(Vec<u8>, u32, u32)> {
    let rgb = img.to_rgb8();
    let mut buf = Cursor::new(Vec::new());
    let mut enc = JpegEncoder::new_with_quality(&mut buf, quality);
    enc.encode(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
        .map_err(|e| err(e.to_string()))?;
    Ok((buf.into_inner(), rgb.width(), rgb.height()))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> CedroResult<()> {
    fs::write(path, bytes).map_err(|e| err(e.to_string()))
}

fn size_message(before: u64, after: u64) -> String {
    let kb = |n: u64| {
        if n < 1024 {
            format!("{n} B")
        } else if n < 1024 * 1024 {
            format!("{} KB", n / 1024)
        } else {
            format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
        }
    };
    if before == 0 {
        return kb(after);
    }
    let pct = (1.0 - after as f64 / before as f64) * 100.0;
    if pct >= 1.0 {
        format!("{} → {} · {:.0}% in meno", kb(before), kb(after), pct)
    } else {
        format!("{} → {}", kb(before), kb(after))
    }
}

pub fn perform(path: &Path, action: &str, quality: u8) -> CedroResult<Output> {
    if !path.exists() {
        return Err(err("file non trovato"));
    }
    let quality = quality.clamp(40, 95);
    let dir = parent_dir(path)?.to_path_buf();
    let before = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let kind = kind_of(path);

    if action == "none" {
        return Err(err("nessuno strumento per questo file"));
    }
    if action == "path" {
        return Ok(Output {
            path: path.to_path_buf(),
            message: path.display().to_string(),
        });
    }
    if action == "dup" {
        let ext = extension(path);
        let out = if ext.is_empty() {
            unique_output(&dir, &format!("{}-copia", stem(path)), "copia")
        } else {
            unique_output(&dir, &format!("{}-copia", stem(path)), &ext)
        };
        fs::copy(path, &out).map_err(|e| err(e.to_string()))?;
        return Ok(Output { path: out, message: "Copia creata".into() });
    }
    if action == "zip" {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let zipped = zip_store(&[(name, bytes.as_slice())]);
        let out = unique_output(&dir, &stem(path), "zip");
        write_bytes(&out, &zipped)?;
        return Ok(Output {
            path: out,
            message: size_message(before, zipped.len() as u64),
        });
    }

    if extension(path) == "opus" && matches!(action, "wav" | "mp3") {
        return Err(err("Opus non è ancora letto"));
    }
    if action == "wav" || action == "mp3" {
        let out = unique_output(&dir, &stem(path), action);
        let result = if action == "wav" {
            crate::audio::to_wav(path, &out)
        } else {
            crate::audio::to_mp3(path, &out, quality)
        };
        if let Err(e) = result {
            let _ = fs::remove_file(&out);
            return Err(err(e));
        }
        let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        return Ok(Output { path: out, message: size_message(before, after) });
    }
    if action == "mp4" {
        let out = unique_output(&dir, &stem(path), "mp4");
        let result = {
            #[cfg(windows)]
            {
                crate::video::to_mp4(path, &out)
            }
            #[cfg(target_os = "linux")]
            {
                crate::linux_video::to_mp4(path, &out)
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            {
                Err::<(), String>("il video MP4 si converte su Windows e su Linux".into())
            }
        };
        if let Err(e) = result {
            let _ = fs::remove_file(&out);
            return Err(err(e));
        }
        let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        return Ok(Output { path: out, message: size_message(before, after) });
    }

    if kind == Kind::Text && matches!(action, "txt" | "md" | "csv" | "pdf" | "upper" | "lower") {
        let text = read_text(path)?;
        if action == "pdf" {
            let bytes = pdf_from_text(&text);
            let out = unique_output(&dir, &stem(path), "pdf");
            write_bytes(&out, &bytes)?;
            return Ok(Output { path: out, message: "PDF di testo".into() });
        }
        let (body, ext, suffix) = match action {
            "upper" => (text.to_uppercase(), extension(path), "-maiuscolo"),
            "lower" => (text.to_lowercase(), extension(path), "-minuscolo"),
            "md" => (text, "md".into(), ""),
            "csv" => (text, "csv".into(), ""),
            _ => (text, "txt".into(), ""),
        };
        let file_stem = if suffix.is_empty() { stem(path) } else { format!("{}{suffix}", stem(path)) };
        let out = unique_output(&dir, &file_stem, if ext.is_empty() { "txt" } else { &ext });
        write_bytes(&out, body.as_bytes())?;
        return Ok(Output { path: out, message: "Testo scritto".into() });
    }

    if kind == Kind::Doc && action == "pdf" {
        let text = crate::doctext::document_text(path).map_err(err)?;
        let bytes = pdf_from_text(&text);
        let out = unique_output(&dir, &stem(path), "pdf");
        write_bytes(&out, &bytes)?;
        return Ok(Output { path: out, message: "PDF di testo".into() });
    }
    if kind == Kind::Pdf && action == "jpg" {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let total = crate::ocr::pdf_page_count(&bytes).map_err(err)?;
        if total == 0 {
            return Err(err("il PDF non ha pagine"));
        }
        let mut first = None;
        crate::ocr::pdf_page_pngs(&bytes, |i, png| {
            let rgb = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgb8();
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, quality).encode_image(&rgb).map_err(|e| e.to_string())?;
            let name = if total == 1 { stem(path) } else { format!("{}-{}", stem(path), i + 1) };
            let out = unique_output(&dir, &name, "jpg");
            write_bytes(&out, &jpeg).map_err(|e| e.to_string())?;
            first.get_or_insert(out);
            Ok(())
        })
        .map_err(err)?;
        let message = if total == 1 { "JPG".to_string() } else { format!("{total} JPG, uno per pagina") };
        return Ok(Output { path: first.unwrap(), message });
    }
    if kind == Kind::Pdf && action == "docx" {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let pages = crate::pdflayout::read_pages(&bytes).map_err(err)?;
        let empty: Vec<bool> = pages.iter().map(|p| matches!(p, crate::pdflayout::Page::Empty)).collect();
        let scanned = empty.iter().filter(|e| **e).count();
        let mut ocr: Vec<Option<String>> = vec![None; pages.len()];
        let mut ocr_error = None;
        if scanned > 0 {
            match crate::ocr::pdf_pages_where(&bytes, |i| empty.get(i).copied().unwrap_or(false)) {
                Ok(read) => ocr = read,
                Err(e) => ocr_error = Some(e),
            }
        }
        let read_scanned = ocr.iter().flatten().filter(|t| !t.is_empty()).count();
        if scanned == pages.len() && read_scanned == 0 {
            return Err(err(match ocr_error {
                Some(e) => format!("questo PDF non ha testo da copiare; OCR: {e}"),
                None => "questo PDF non ha testo da copiare né da leggere con l'OCR".into(),
            }));
        }
        let docx = crate::pdflayout::docx(&pages, |i| ocr.get(i).cloned().flatten().unwrap_or_default());
        let out = unique_output(&dir, &stem(path), "docx");
        write_bytes(&out, &docx)?;
        let message = if scanned == 0 {
            "Word modificabile: titoli, paragrafi e tabelle del PDF".to_string()
        } else {
            format!(
                "Word. {} pagine ricostruite dal PDF, {read_scanned} lette via OCR in italiano (da ricontrollare) su {scanned} senza testo.",
                pages.len() - scanned
            )
        };
        return Ok(Output { path: out, message });
    }
    if kind == Kind::Pdf && action == "txt" {
        let (text, ocr_pages, skipped) = pdf_text_with_ocr(path).map_err(err)?;
        let (bytes, ext) = if action == "docx" {
            (crate::doctext::docx_bytes(&text), "docx")
        } else {
            (text.into_bytes(), "txt")
        };
        let out = unique_output(&dir, &stem(path), ext);
        write_bytes(&out, &bytes)?;
        let mut message = match (action, ocr_pages) {
            ("docx", 0) => "Word di testo. L'impaginazione del PDF non viene ricostruita.".to_string(),
            ("docx", n) => format!("Word di testo, con {n} pagine lette via OCR: controllalo. L'impaginazione non viene ricostruita."),
            (_, 0) => "Testo estratto dal PDF".to_string(),
            (_, n) => format!("Testo estratto dal PDF, con {n} pagine lette via OCR: controllalo."),
        };
        if skipped > 0 {
            message.push_str(&format!(" Mancano {skipped} pagine scansionate: l'OCR ne legge al massimo {}.", crate::ocr::MAX_OCR_PAGES));
        }
        return Ok(Output { path: out, message });
    }
    if action == "ocr" && kind == Kind::Image {
        let text = crate::ocr::image_text(path).map_err(err)?;
        let out = unique_output(&dir, &stem(path), "txt");
        write_bytes(&out, text.as_bytes())?;
        return Ok(Output { path: out, message: "Testo letto".into() });
    }
    if action == "ocr" && kind == Kind::Pdf {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let text = crate::doctext::pdf_text(path).or_else(|_| crate::ocr::pdf_pages_text(&bytes)).map_err(err)?;
        let out = unique_output(&dir, &stem(path), "txt");
        write_bytes(&out, text.as_bytes())?;
        return Ok(Output { path: out, message: "Testo del PDF. Al massimo 100 pagine disegnate.".into() });
    }

    if kind != Kind::Image {
        return Err(err("questa voce non si applica a questo file"));
    }

    let mut img = load_image(path)?;
    match action {
        "half" => {
            let w = (img.width() / 2).max(1);
            let h = (img.height() / 2).max(1);
            img = img.resize(w, h, image::imageops::FilterType::Lanczos3);
        }
        "gray" => img = img.grayscale(),
        "rot90" => img = img.rotate90(),
        _ => {}
    }

    if action == "pdf" {
        let (jpeg, w, h) = jpeg_bytes(&img, 90)?;
        let lines = crate::ocr::image_lines(&img).unwrap_or_default();
        let bytes = pdf_from_jpeg(&jpeg, w, h, &lines);
        let out = unique_output(&dir, &stem(path), "pdf");
        write_bytes(&out, &bytes)?;
        let message = if lines.is_empty() {
            format!("PDF {w}×{h}")
        } else {
            format!("PDF {w}×{h}, con il testo OCR selezionabile e ricercabile")
        };
        return Ok(Output { path: out, message });
    }

    let (format, ext, label_stem) = match action {
        "png" | "strip" => (ImageFormat::Png, "png", if action == "strip" { "pulito" } else { "" }),
        "jpeg" => (ImageFormat::Jpeg, "jpg", ""),
        "compress" => (ImageFormat::Jpeg, "jpg", "compresso"),
        "webp" => (ImageFormat::WebP, "webp", ""),
        "bmp" => (ImageFormat::Bmp, "bmp", ""),
        "gif" => (ImageFormat::Gif, "gif", ""),
        "tiff" => (ImageFormat::Tiff, "tiff", ""),
        "half" => (ImageFormat::Png, "png", "meta"),
        "gray" => (ImageFormat::Png, "png", "bn"),
        "rot90" => (ImageFormat::Png, "png", "ruotato"),
        other => return Err(err(format!("azione sconosciuta: {other}"))),
    };
    let file_stem = if label_stem.is_empty() {
        stem(path)
    } else {
        format!("{}-{label_stem}", stem(path))
    };
    let out = unique_output(&dir, &file_stem, ext);
    let q = if action == "compress" { quality.min(62) } else { quality };
    save_image(&img, &out, format, q)?;
    let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let message = match action {
        "strip" => "Riscritto in PNG, senza metadati".into(),
        "half" => format!("{}×{}", img.width(), img.height()),
        "rot90" => "Ruotato di 90°".into(),
        "gray" => "Bianco e nero".into(),
        "compress" | "jpeg" | "webp" => size_message(before, after),
        _ => format!("{ext} {}×{}", img.width(), img.height()),
    };
    Ok(Output { path: out, message })
}

pub fn config_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("Cedro");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("cedro");
    }
    PathBuf::from(".").join("cedro-config")
}

pub fn same_kind_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let Some(first) = paths.first() else {
        return Vec::new();
    };
    let kind = kind_of(first);
    paths.iter().filter(|path| kind_of(path) == kind).cloned().collect()
}

fn read_text(path: &Path) -> CedroResult<String> {
    let bytes = fs::read(path).map_err(|e| err(format!("testo non letto: {e}")))?;
    Ok(decode_text(&bytes))
}

fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    bytes.iter().copied().map(crate::doctext::cp1252).collect()
}

fn decode_utf16(bytes: &[u8], little: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn read_quality() -> u8 {
    crate::settings::read_settings().quality
}

pub fn append_log(line: &str) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("log.txt");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("cedro-test-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn digital_pdf_to_word_keeps_layout() {
        let dir = scratch();
        let pdf = dir.join("lettera.pdf");
        fs::write(&pdf, pdf_from_text("Perché è già così\nSecondo rigo")).unwrap();
        let docx = perform(&pdf, "docx", 80).expect("Word");
        let xml = crate::doctext::zip_entry_for_test(&docx.path, "word/document.xml");
        assert!(!xml.contains("w:framePr"), "testo bloccato in riquadri");
        assert!(xml.contains("BodyText"), "{xml}");
        assert!(xml.contains("Perché è già così"), "{xml}");
        assert!(xml.contains("Helvetica") || xml.contains("Arial"), "{xml}");
    }

    #[test]
    fn pdf_table_becomes_word_table() {
        let content = "0.12 0.23 0.37 rg 50 700 300 20 re f 0.95 0.96 0.98 rg 50 680 300 20 re f \
0.8 G 0.5 w 50 720 m 350 720 l S 50 700 m 350 700 l S 50 680 m 350 680 l S 50 660 m 350 660 l S \
50 720 m 50 660 l S 250 720 m 250 660 l S 350 720 m 350 660 l S \
BT /F2 12 Tf 1 1 1 rg 56 706 Td (Nome) Tj ET BT /F2 12 Tf 1 1 1 rg 256 706 Td (Voto) Tj ET \
BT /F1 12 Tf 0 g 56 686 Td (Luca) Tj ET BT /F1 12 Tf 0 g 256 686 Td (10) Tj ET \
BT /F1 12 Tf 0 g 56 666 Td (Anna) Tj ET BT /F1 12 Tf 0 g 256 666 Td (9) Tj ET \
BT /F2 18 Tf 0 g 50 750 Td (Titolo) Tj ET BT /F1 10 Tf 0 g 50 600 Td (Testo sotto la tabella.) Tj ET";
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_string(),
        ];
        for (i, o) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in &offsets {
            pdf.extend(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        let dir = scratch();
        let path = dir.join("tabella.pdf");
        fs::write(&path, pdf).unwrap();
        let out = perform(&path, "docx", 80).expect("Word");
        let xml = crate::doctext::zip_entry_for_test(&out.path, "word/document.xml");
        assert_eq!(xml.matches("<w:tr>").count(), 3, "{xml}");
        assert_eq!(xml.matches("<w:gridCol ").count(), 2, "{xml}");
        assert!(xml.contains("w:fill=\"1F3B5E\"") || xml.contains("w:fill=\"1F3A5E\""), "{xml}");
        assert!(xml.contains("Heading1") && xml.contains("Titolo"), "{xml}");
        assert!(xml.contains("<w:b/>"), "grassetto perso");
        assert!(xml.contains("Testo sotto la tabella."), "{xml}");
    }

    #[test]
    fn photo_to_searchable_pdf() {
        let dir = scratch();
        let src = dir.join("foglio.jpg");
        fs::write(&src, include_bytes!("../tests/fixtures/scan.jpg")).unwrap();
        let pdf = perform(&src, "pdf", 80).unwrap();
        let bytes = fs::read(&pdf.path).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"3 Tr"), "manca il testo invisibile");
        let text = crate::doctext::pdf_text(&pdf.path).unwrap();
        assert!(text.contains("Cedro"), "{text}");
    }

    #[test]
    fn lines_become_paragraphs() {
        use crate::ocr::{paragraphs_from_lines, paragraphs_from_text, Line};
        let line = |text: &str, y: f32, w: f32| Line { text: text.into(), x: 10.0, y, w, h: 20.0 };
        let lines = [
            line("Gentile signore,", 0.0, 150.0),
            line("la riunione è spo-", 30.0, 500.0),
            line("stata a giovedì.", 60.0, 140.0),
            line("Saluti", 120.0, 60.0),
        ];
        assert_eq!(
            paragraphs_from_lines(&lines),
            "Gentile signore,\nla riunione è spostata a giovedì.\nSaluti"
        );
        let text = "Gentile signore,\nla riunione del condominio è stata spostata\na giovedì.\n\nSaluti";
        assert_eq!(
            paragraphs_from_text(text),
            "Gentile signore,\nla riunione del condominio è stata spostata a giovedì.\nSaluti"
        );
    }

    #[test]
    fn scanned_pdf_to_word_uses_ocr() {
        let dir = scratch();
        let jpeg = include_bytes!("../tests/fixtures/scan.jpg");
        let pdf = dir.join("scansione.pdf");
        fs::write(&pdf, crate::pdf::pdf_from_jpeg(jpeg, 900, 200, &[])).unwrap();
        let jpg = perform(&pdf, "jpg", 80).expect("JPG");
        assert!(fs::read(&jpg.path).unwrap().starts_with(&[0xff, 0xd8]));
        let docx = perform(&pdf, "docx", 80).expect("OCR");
        let text = crate::doctext::document_text(&docx.path).unwrap();
        assert!(text.contains("Cedro"), "{text}");
    }

    #[test]
    fn jpeg_pdf_zip_and_text() {
        let dir = scratch();
        let src = dir.join("frutto.png");
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 6, Rgb([210, 220, 40])));
        img.save(&src).unwrap();

        let jpeg = perform(&src, "jpeg", 80).unwrap();
        let bytes = fs::read(&jpeg.path).unwrap();
        assert_eq!(&bytes[..2], &[0xff, 0xd8]);

        let pdf = perform(&src, "pdf", 80).unwrap();
        let pdf_bytes = fs::read(&pdf.path).unwrap();
        assert_eq!(&pdf_bytes[..8], b"%PDF-1.4");

        let zip = perform(&src, "zip", 80).unwrap();
        let zip_bytes = fs::read(&zip.path).unwrap();
        assert_eq!(&zip_bytes[..2], &[0x50, 0x4b]);

        let note = dir.join("nota.txt");
        fs::write(&note, "Cedro\nlocale\n").unwrap();
        let upper = perform(&note, "upper", 80).unwrap();
        let body = fs::read_to_string(&upper.path).unwrap();
        assert!(body.contains("CEDRO"));
        let text_pdf = perform(&note, "pdf", 80).unwrap();
        assert!(fs::read(&text_pdf.path).unwrap().starts_with(b"%PDF-1.4"));

        let half = perform(&src, "half", 80).unwrap();
        let small = image::open(&half.path).unwrap();
        assert_eq!(small.width(), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn document_roundtrip_without_office() {
        let dir = std::env::temp_dir().join(format!("cedro-doc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("lettera.docx");
        fs::write(&src, crate::doctext::docx_bytes("Cedro legge il testo")).unwrap();
        let pdf = perform(&src, "pdf", 80).unwrap();
        let pdf_bytes = fs::read(&pdf.path).unwrap();
        assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
        assert!(pdf_bytes.windows(5).any(|w| w == b"Cedro"));
        let txt = perform(&pdf.path, "txt", 80).unwrap();
        assert!(fs::read_to_string(&txt.path).unwrap().contains("Cedro"));
        let docx = perform(&pdf.path, "docx", 80).unwrap();
        assert!(fs::read(&docx.path).unwrap().starts_with(b"PK"));
        let rtf = dir.join("nota.rtf");
        fs::write(&rtf, b"{\rtf1 Cedro rtf}").unwrap();
        let rtf_pdf = perform(&rtf, "pdf", 80).unwrap();
        assert!(fs::read(&rtf_pdf.path).unwrap().windows(5).any(|w| w == b"Cedro"));
        let legacy = dir.join("vecchio.doc");
        write_spec_doc(&legacy);
        let legacy_pdf = perform(&legacy, "pdf", 80).expect("DOC");
        assert!(fs::read(&legacy_pdf.path).unwrap().windows(5).any(|w| w == b"Cedro"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_text_and_opus() {
        let dir = std::env::temp_dir().join(format!("cedro-ansi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ansi = dir.join("citta.txt");
        fs::write(&ansi, b"citt\xe0").unwrap();
        let upper = perform(&ansi, "upper", 80).unwrap();
        assert!(fs::read_to_string(&upper.path).unwrap().contains("CITT\u{00C0}"));
        let utf16 = dir.join("nota.txt");
        let mut wide = vec![0xFF, 0xFE];
        for unit in "Cedro".encode_utf16() {
            wide.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&utf16, wide).unwrap();
        let pdf = perform(&utf16, "pdf", 80).unwrap();
        assert!(fs::read(&pdf.path).unwrap().windows(5).any(|w| w == b"Cedro"));
        assert!(kind_of(Path::new("nota.opus")) == Kind::Other);
        assert_eq!(slices(Kind::Other, Mode::Convert)[0].id, "none");
        let opus = dir.join("nota.opus");
        fs::write(&opus, b"OpusHead").unwrap();
        match perform(&opus, "wav", 80) {
            Ok(_) => panic!("Opus non deve diventare WAV"),
            Err(e) => assert!(e.to_string().contains("Opus"), "{e}"),
        }
        let photo = dir.join("foto.png");
        let other = dir.join("lettera.docx");
        fs::write(&photo, b"png").unwrap();
        fs::write(&other, b"docx").unwrap();
        let kept = same_kind_paths(&[photo.clone(), other, dir.join("altra.png")]);
        assert_eq!(kept, vec![photo, dir.join("altra.png")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn avi_to_mp4_on_windows() {
        let dir = std::env::temp_dir().join(format!("cedro-avi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let avi = dir.join("clip.avi");
        fs::write(&avi, tiny_avi()).unwrap();
        let mp4 = dir.join("clip.mp4");
        crate::video::to_mp4(&avi, &mp4).expect("MP4");
        let bytes = fs::read(&mp4).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"ftyp"), "manca ftyp");
        assert!(mp4_box_len(&bytes, b"mdat") > 32, "mdat vuoto");
        let rgb = dir.join("colori.avi");
        fs::write(&rgb, rgb_avi()).unwrap();
        let rgb_mp4 = dir.join("colori.mp4");
        crate::video::to_mp4(&rgb, &rgb_mp4).expect("MP4 da RGB");
        let rgb_bytes = fs::read(&rgb_mp4).unwrap();
        assert!(mp4_box_len(&rgb_bytes, b"mdat") > 32, "mdat RGB vuoto");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn webm_to_mp4_on_windows() {
        let dir = std::env::temp_dir().join(format!("cedro-webm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in [
            ("vp8.webm", include_bytes!("../tests/fixtures/vp8.webm").as_slice()),
            ("vp9.webm", include_bytes!("../tests/fixtures/vp9.webm").as_slice()),
        ] {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let mp4 = src.with_extension("mp4");
            crate::video::to_mp4(&src, &mp4).unwrap_or_else(|err| panic!("{name}: {err}"));
            let out = fs::read(&mp4).unwrap();
            assert!(out.windows(4).any(|w| w == b"ftyp"), "{name} senza ftyp");
            assert!(mp4_box_len(&out, b"mdat") > 32, "{name} mdat vuoto");
            assert!(out.windows(4).any(|w| w == b"mp4a"), "{name} senza audio");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn video_to_mp4_on_linux() {
        let dir = std::env::temp_dir().join(format!("cedro-linux-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (name, bytes, want_audio) in [
            ("nv12.avi", tiny_avi(), false),
            ("colori.avi", rgb_avi(), false),
            ("vp8.webm", include_bytes!("../tests/fixtures/vp8.webm").to_vec(), true),
            ("vp9.webm", include_bytes!("../tests/fixtures/vp9.webm").to_vec(), true),
        ] {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let mp4 = perform(&src, "mp4", 80).unwrap_or_else(|err| panic!("{name}: {err}"));
            let out = fs::read(&mp4.path).unwrap();
            assert!(out.windows(4).any(|w| w == b"ftyp"), "{name} senza ftyp");
            assert!(mp4_box_len(&out, b"mdat") > 32, "{name} mdat vuoto");
            if want_audio {
                assert!(out.windows(4).any(|w| w == b"mp4a"), "{name} senza audio");
            }
            if let Some(codecs) = probe_codecs(&mp4.path) {
                assert!(codecs.contains("h264"), "{name} ffprobe: {codecs}");
                if want_audio {
                    assert!(codecs.contains("aac"), "{name} ffprobe: {codecs}");
                }
            }
        }
        let made = ffmpeg_clip(&dir.join("h264.mp4"), &["-f", "lavfi", "-i", "testsrc=size=64x64:rate=30:duration=0.1", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-an"]);
        if made {
            let mp4 = perform(&dir.join("h264.mp4"), "mp4", 80).expect("mp4 h264");
            let out = fs::read(&mp4.path).unwrap();
            assert!(mp4_box_len(&out, b"mdat") > 32);
        }
        let with_audio = [
            "-f", "lavfi", "-i", "testsrc=size=64x64:rate=30:duration=0.2",
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=44100:duration=0.2",
            "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
        ];
        for name in ["audio.mp4", "audio.mkv"] {
            let dest = dir.join(name);
            if !ffmpeg_clip(&dest, &with_audio) {
                continue;
            }
            let mp4 = perform(&dest, "mp4", 80).unwrap_or_else(|err| panic!("{name}: {err}"));
            let out = fs::read(&mp4.path).unwrap();
            assert!(mp4_box_len(&out, b"mdat") > 32, "{name} mdat vuoto");
            assert!(out.windows(4).any(|w| w == b"mp4a"), "{name} senza audio");
            if let Some(codecs) = probe_codecs(&mp4.path) {
                assert!(codecs.contains("h264"), "{name} ffprobe: {codecs}");
                assert!(codecs.contains("aac"), "{name} ffprobe: {codecs}");
            }
        }
        let mjpeg = ffmpeg_clip(
            &dir.join("mjpeg.avi"),
            &["-f", "lavfi", "-i", "testsrc=size=64x64:rate=30:duration=0.1", "-c:v", "mjpeg", "-an"],
        );
        if mjpeg {
            let mp4 = perform(&dir.join("mjpeg.avi"), "mp4", 80).expect("avi mjpeg");
            assert!(mp4_box_len(&fs::read(&mp4.path).unwrap(), b"mdat") > 32);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    fn probe_codecs(path: &Path) -> Option<String> {
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0"])
            .arg(path)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    }

    #[cfg(target_os = "linux")]
    fn ffmpeg_clip(dest: &Path, args: &[&str]) -> bool {
        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.arg("-y").args(args).arg(dest);
        cmd.status().map(|status| status.success() && dest.exists()).unwrap_or(false)
    }

    #[test]
    fn audio_without_ffmpeg() {
        let dir = std::env::temp_dir().join(format!("cedro-wav-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("nota.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
        for i in 0..4000 {
            writer.write_sample(((i as f32 * 0.2).sin() * 6000.0) as i16).unwrap();
        }
        writer.finalize().unwrap();
        let mp3 = perform(&wav, "mp3", 80).unwrap();
        assert!(fs::metadata(&mp3.path).unwrap().len() > 64);
        let back = perform(&mp3.path, "wav", 80).unwrap();
        assert!(fs::read(&back.path).unwrap().starts_with(b"RIFF"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn png_crc(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    fn insert_png_text(bytes: &mut Vec<u8>, key: &str, value: &str) {
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "non è un png");
        let mut at = 8usize;
        let iend = loop {
            assert!(at + 12 <= bytes.len(), "png senza IEND");
            let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let next = at + 12 + len;
            assert!(next <= bytes.len(), "chunk png troncato");
            if &bytes[at + 4..at + 8] == b"IEND" {
                break at;
            }
            at = next;
        };
        let mut payload = Vec::with_capacity(key.len() + value.len() + 1);
        payload.extend_from_slice(key.as_bytes());
        payload.push(0);
        payload.extend_from_slice(value.as_bytes());
        let mut chunk = Vec::with_capacity(12 + payload.len());
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(&payload);
        chunk.extend_from_slice(&png_crc(&chunk[4..]).to_be_bytes());
        bytes.splice(iend..iend, chunk);
    }

    #[test]
    fn image_tools_and_formats() {
        let dir = scratch();
        let mut raw = RgbImage::new(8, 6);
        for (x, y, pixel) in raw.enumerate_pixels_mut() {
            *pixel = if x == 1 && y == 2 { Rgb([200, 10, 10]) } else { Rgb([20, 180, 40]) };
        }
        let src = dir.join("foto.png");
        raw.save(&src).unwrap();
        let mut marked = fs::read(&src).unwrap();
        insert_png_text(&mut marked, "Comment", "SECRET");
        fs::write(&src, &marked).unwrap();

        let clean = perform(&src, "strip", 80).unwrap();
        let clean_bytes = fs::read(&clean.path).unwrap();
        assert!(clean_bytes.starts_with(b"\x89PNG"), "strip non è png");
        assert!(!clean_bytes.windows(6).any(|w| w == b"SECRET"), "i metadati sono rimasti");
        let cleaned = image::open(&clean.path).unwrap();
        assert_eq!((cleaned.width(), cleaned.height()), (8, 6));

        let gray = perform(&src, "gray", 80).unwrap();
        let gray_img = image::open(&gray.path).unwrap().to_rgb8();
        assert_eq!(gray_img.dimensions(), (8, 6));
        let px = gray_img.get_pixel(1, 2);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
        assert_ne!(*px, Rgb([200, 10, 10]));

        let turned = perform(&src, "rot90", 80).unwrap();
        let turned_img = image::open(&turned.path).unwrap().to_rgb8();
        assert_eq!(turned_img.dimensions(), (6, 8));
        assert_eq!(*turned_img.get_pixel(3, 1), Rgb([200, 10, 10]));

        for action in ["webp", "gif", "bmp", "tiff"] {
            let made = perform(&src, action, 80).unwrap_or_else(|err| panic!("{action}: {err}"));
            let bytes = fs::read(&made.path).unwrap();
            match action {
                "webp" => assert!(bytes.starts_with(b"RIFF") && bytes.windows(4).any(|w| w == b"WEBP"), "{action}"),
                "gif" => assert!(bytes.starts_with(b"GIF8"), "{action}"),
                "bmp" => assert!(bytes.starts_with(b"BM"), "{action}"),
                "tiff" => assert!(bytes.starts_with(b"II") || bytes.starts_with(b"MM"), "{action}"),
                _ => {}
            }
            let back = perform(&made.path, "png", 80).unwrap_or_else(|err| panic!("{action} png: {err}"));
            let round = image::open(&back.path).unwrap();
            assert_eq!((round.width(), round.height()), (8, 6), "{action}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_audio_to_wav_and_mp3() {
        let dir = scratch();
        for (name, bytes) in [
            ("nota.ogg", include_bytes!("../tests/fixtures/nota.ogg").as_slice()),
            ("nota.flac", include_bytes!("../tests/fixtures/nota.flac").as_slice()),
            ("nota.m4a", include_bytes!("../tests/fixtures/nota.m4a").as_slice()),
            ("nota.aac", include_bytes!("../tests/fixtures/nota.aac").as_slice()),
        ] {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let wav = perform(&src, "wav", 80).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(fs::read(&wav.path).unwrap().starts_with(b"RIFF"), "{name} non è wav");
            let mp3 = perform(&src, "mp3", 80).unwrap_or_else(|err| panic!("{name} mp3: {err}"));
            assert!(fs::metadata(&mp3.path).unwrap().len() > 64, "{name} mp3 vuoto");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn video_with_aac_keeps_audio() {
        let dir = scratch();
        for (name, bytes) in [
            ("audio.mp4", include_bytes!("../tests/fixtures/audio.mp4").as_slice()),
            ("audio.mov", include_bytes!("../tests/fixtures/audio.mov").as_slice()),
            ("audio.mkv", include_bytes!("../tests/fixtures/audio.mkv").as_slice()),
        ] {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let mp4 = perform(&src, "mp4", 80).unwrap_or_else(|err| panic!("{name}: {err}"));
            let out = fs::read(&mp4.path).unwrap();
            assert!(out.windows(4).any(|w| w == b"ftyp"), "{name} senza ftyp");
            assert!(mp4_box_len(&out, b"mdat") > 32, "{name} mdat vuoto");
            assert!(out.windows(4).any(|w| w == b"mp4a"), "{name} senza audio");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mov_and_mkv_to_mp4() {
        let dir = scratch();
        for (name, bytes) in [
            ("clip.mov", include_bytes!("../tests/fixtures/clip.mov").as_slice()),
            ("clip.mkv", include_bytes!("../tests/fixtures/clip.mkv").as_slice()),
        ] {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let mp4 = perform(&src, "mp4", 80).unwrap_or_else(|err| panic!("{name}: {err}"));
            let out = fs::read(&mp4.path).unwrap();
            assert!(out.windows(4).any(|w| w == b"ftyp"), "{name} senza ftyp");
            assert!(mp4_box_len(&out, b"mdat") > 32, "{name} mdat vuoto");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn doc_and_pdf_slices() {
        assert!(kind_of(Path::new("lettera.docx")) == Kind::Doc);
        assert_eq!(slices(Kind::Doc, Mode::Convert)[0].id, "pdf");
        assert_eq!(slices(Kind::Pdf, Mode::Convert)[0].id, "jpg");
        assert_eq!(slices(Kind::Pdf, Mode::Convert)[1].id, "docx");
        assert_eq!(slices(Kind::Pdf, Mode::Convert).len(), 2);
        assert_eq!(slices(Kind::Pdf, Mode::Tools)[0].id, "ocr");
        assert!(slices(Kind::Image, Mode::Tools).iter().any(|s| s.id == "ocr"));
    }
}

#[cfg(test)]
fn write_spec_doc(path: &Path) {
    let (word, table) = spec_doc_streams("Cedro");
    let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    let mut comp = cfb::CompoundFile::create(file).unwrap();
    {
        use std::io::Write;
        let mut stream = comp.create_stream("/WordDocument").unwrap();
        stream.write_all(&word).unwrap();
    }
    {
        use std::io::Write;
        let mut stream = comp.create_stream("/0Table").unwrap();
        stream.write_all(&table).unwrap();
    }
    comp.flush().unwrap();
}

#[cfg(test)]
fn spec_doc_streams(text: &str) -> (Vec<u8>, Vec<u8>) {
    let mut body: Vec<u8> = text.bytes().collect();
    body.push(0x0D);
    let mut fib = Vec::new();
    fib.extend_from_slice(&0xA5ECu16.to_le_bytes());
    fib.extend_from_slice(&0x00C1u16.to_le_bytes());
    fib.extend(std::iter::repeat(0).take(28));
    fib.extend_from_slice(&14u16.to_le_bytes());
    fib.extend(std::iter::repeat(0).take(28));
    fib.extend_from_slice(&22u16.to_le_bytes());
    fib.extend(std::iter::repeat(0).take(88));
    fib.extend_from_slice(&93u16.to_le_bytes());
    let pairs_at = fib.len();
    fib.extend(std::iter::repeat(0).take(93 * 8));
    let text_at = fib.len() as u32;
    fib.extend_from_slice(&body);
    let mut clx = vec![0x02];
    clx.extend_from_slice(&16u32.to_le_bytes());
    clx.extend_from_slice(&0u32.to_le_bytes());
    clx.extend_from_slice(&(body.len() as u32).to_le_bytes());
    clx.extend_from_slice(&0u16.to_le_bytes());
    let fc = (text_at * 2) | (1 << 30);
    clx.extend_from_slice(&fc.to_le_bytes());
    clx.extend_from_slice(&0u16.to_le_bytes());
    fib[pairs_at + 264..pairs_at + 268].copy_from_slice(&0u32.to_le_bytes());
    fib[pairs_at + 268..pairs_at + 272].copy_from_slice(&(clx.len() as u32).to_le_bytes());
    (fib, clx)
}

#[cfg(test)]
fn tiny_avi() -> Vec<u8> {
    fn u32(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&u32(payload.len() as u32));
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        out
    }
    fn list(kind: &[u8; 4], inner: &[u8]) -> Vec<u8> {
        let mut out = b"LIST".to_vec();
        out.extend_from_slice(&u32(4 + inner.len() as u32));
        out.extend_from_slice(kind);
        out.extend_from_slice(inner);
        out
    }
    let width = 64u32;
    let height = 64u32;
    let frame_size = width * height * 3 / 2;
    let mut frame = vec![16u8; frame_size as usize];
    for uv in frame.iter_mut().skip((width * height) as usize) {
        *uv = 128;
    }
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&u32(33_333));
    avih[4..8].copy_from_slice(&u32(frame_size * 30));
    avih[12..16].copy_from_slice(&u32(0x10));
    avih[16..20].copy_from_slice(&u32(2));
    avih[24..28].copy_from_slice(&u32(1));
    avih[28..32].copy_from_slice(&u32(frame_size));
    avih[32..36].copy_from_slice(&u32(width));
    avih[36..40].copy_from_slice(&u32(height));
    let mut strh = vec![0u8; 64];
    strh[0..4].copy_from_slice(b"vids");
    strh[4..8].copy_from_slice(b"NV12");
    strh[20..24].copy_from_slice(&u32(1));
    strh[24..28].copy_from_slice(&u32(30));
    strh[32..36].copy_from_slice(&u32(2));
    strh[36..40].copy_from_slice(&u32(frame_size));
    strh[44..48].copy_from_slice(&u32(frame_size));
    strh[56..60].copy_from_slice(&u32(width));
    strh[60..64].copy_from_slice(&u32(height));
    let mut strf = vec![0u8; 40];
    strf[0..4].copy_from_slice(&u32(40));
    strf[4..8].copy_from_slice(&u32(width));
    strf[8..12].copy_from_slice(&u32(height));
    strf[12..14].copy_from_slice(&1u16.to_le_bytes());
    strf[14..16].copy_from_slice(&12u16.to_le_bytes());
    strf[16..20].copy_from_slice(b"NV12");
    strf[20..24].copy_from_slice(&u32(frame_size));
    let mut hdr = chunk(b"avih", &avih);
    let mut strl = chunk(b"strh", &strh);
    strl.extend(chunk(b"strf", &strf));
    hdr.extend(list(b"strl", &strl));
    let db = chunk(b"00dc", &frame);
    let mut movi_inner = b"movi".to_vec();
    let first = 4u32;
    let second = 4 + db.len() as u32;
    movi_inner.extend_from_slice(&db);
    movi_inner.extend_from_slice(&db);
    let mut idx = Vec::new();
    for offset in [first, second] {
        idx.extend_from_slice(b"00dc");
        idx.extend_from_slice(&u32(0x10));
        idx.extend_from_slice(&u32(offset));
        idx.extend_from_slice(&u32(frame_size));
    }
    let mut body = list(b"hdrl", &hdr);
    body.extend(list(b"movi", &movi_inner[4..]));
    body.extend(chunk(b"idx1", &idx));
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&u32(4 + body.len() as u32));
    file.extend_from_slice(b"AVI ");
    file.extend(body);
    file
}

#[cfg(test)]
fn rgb_avi() -> Vec<u8> {
    fn u32(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&u32(payload.len() as u32));
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        out
    }
    fn list(kind: &[u8; 4], inner: &[u8]) -> Vec<u8> {
        let mut out = b"LIST".to_vec();
        out.extend_from_slice(&u32(4 + inner.len() as u32));
        out.extend_from_slice(kind);
        out.extend_from_slice(inner);
        out
    }
    let width = 64u32;
    let height = 64u32;
    let stride = width * 3;
    let frame_size = stride * height;
    let mut frame = vec![0u8; frame_size as usize];
    for y in 0..height {
        for x in 0..width {
            let at = ((height - 1 - y) * stride + x * 3) as usize;
            frame[at] = 40;
            frame[at + 1] = 180;
            frame[at + 2] = 20;
        }
    }
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&u32(33_333));
    avih[4..8].copy_from_slice(&u32(frame_size * 30));
    avih[12..16].copy_from_slice(&u32(0x10));
    avih[16..20].copy_from_slice(&u32(2));
    avih[24..28].copy_from_slice(&u32(1));
    avih[28..32].copy_from_slice(&u32(frame_size));
    avih[32..36].copy_from_slice(&u32(width));
    avih[36..40].copy_from_slice(&u32(height));
    let mut strh = vec![0u8; 64];
    strh[0..4].copy_from_slice(b"vids");
    strh[4..8].copy_from_slice(b"DIB ");
    strh[20..24].copy_from_slice(&u32(1));
    strh[24..28].copy_from_slice(&u32(30));
    strh[32..36].copy_from_slice(&u32(2));
    strh[36..40].copy_from_slice(&u32(frame_size));
    strh[44..48].copy_from_slice(&u32(frame_size));
    strh[56..60].copy_from_slice(&u32(width));
    strh[60..64].copy_from_slice(&u32(height));
    let mut strf = vec![0u8; 40];
    strf[0..4].copy_from_slice(&u32(40));
    strf[4..8].copy_from_slice(&u32(width));
    strf[8..12].copy_from_slice(&u32(height));
    strf[12..14].copy_from_slice(&1u16.to_le_bytes());
    strf[14..16].copy_from_slice(&24u16.to_le_bytes());
    strf[20..24].copy_from_slice(&u32(frame_size));
    let mut hdr = chunk(b"avih", &avih);
    let mut strl = chunk(b"strh", &strh);
    strl.extend(chunk(b"strf", &strf));
    hdr.extend(list(b"strl", &strl));
    let db = chunk(b"00db", &frame);
    let mut movi_inner = b"movi".to_vec();
    let first = 4u32;
    let second = 4 + db.len() as u32;
    movi_inner.extend_from_slice(&db);
    movi_inner.extend_from_slice(&db);
    let mut idx = Vec::new();
    for offset in [first, second] {
        idx.extend_from_slice(b"00db");
        idx.extend_from_slice(&u32(0x10));
        idx.extend_from_slice(&u32(offset));
        idx.extend_from_slice(&u32(frame_size));
    }
    let mut body = list(b"hdrl", &hdr);
    body.extend(list(b"movi", &movi_inner[4..]));
    body.extend(chunk(b"idx1", &idx));
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&u32(4 + body.len() as u32));
    file.extend_from_slice(b"AVI ");
    file.extend(body);
    file
}

#[cfg(test)]
fn mp4_box_len(bytes: &[u8], name: &[u8; 4]) -> usize {
    let mut i = 0;
    while i + 8 <= bytes.len() {
        let size = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        if size < 8 || i + size > bytes.len() {
            return 0;
        }
        if &bytes[i + 4..i + 8] == name {
            return size;
        }
        i += size;
    }
    0
}
