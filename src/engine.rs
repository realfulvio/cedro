use crate::pdf::{pdf_from_jpeg, pdf_from_text};
use crate::zipstore::zip_store;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageFormat, ImageReader};
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
    Slice { id: "docx", label: "Word" },
    Slice { id: "txt", label: "Testo" },
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
        "wav" | "mp3" | "ogg" | "m4a" | "aac" | "flac" | "opus" => Kind::Audio,
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
    ImageReader::open(path)
        .map_err(|e| err(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| err(e.to_string()))?
        .decode()
        .map_err(|e| err(format!("immagine non letta: {e}")))
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

pub fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn ffmpeg(input: &Path, output: &Path, args: &[&str]) -> CedroResult<()> {
    if !ffmpeg_available() {
        return Err(err("ffmpeg non è nel PATH. Cedro non lo include, per restare piccolo."));
    }
    let status = std::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .args(args)
        .arg(output)
        .status()
        .map_err(|e| err(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(output);
        Err(err("ffmpeg non ha completato la conversione"))
    }
}

fn program_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(key) {
            dirs.push(PathBuf::from(value));
        }
    }
    dirs
}

fn on_path(names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let direct = dir.join(name);
            if direct.is_file() {
                return Some(direct);
            }
            #[cfg(windows)]
            {
                for ext in [".com", ".exe"] {
                    let with = dir.join(format!("{name}{ext}"));
                    if with.is_file() {
                        return Some(with);
                    }
                }
            }
        }
    }
    None
}

fn find_tool(names: &[&str], relative: &[&str]) -> Option<PathBuf> {
    if let Some(found) = on_path(names) {
        return Some(found);
    }
    for root in program_dirs() {
        for rel in relative {
            let path = root.join(rel);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn office_bin() -> Option<PathBuf> {
    find_tool(
        &["soffice.com", "soffice", "libreoffice"],
        &["LibreOffice/program/soffice.com", "LibreOffice/program/soffice.exe"],
    )
}

fn tesseract_bin() -> Option<PathBuf> {
    find_tool(&["tesseract"], &["Tesseract-OCR/tesseract.exe"])
}

fn pdftoppm_bin() -> Option<PathBuf> {
    find_tool(
        &["pdftoppm"],
        &["poppler/Library/bin/pdftoppm.exe", "poppler/bin/pdftoppm.exe"],
    )
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

fn scratch_dir() -> PathBuf {
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("cedro-{}-{tick}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn office_convert(input: &Path, dir: &Path, filter: &str, ext: &str, message: &str) -> CedroResult<Output> {
    let Some(bin) = office_bin() else {
        return Err(err("LibreOffice non trovato. Cedro non lo include: installalo e riprova."));
    };
    let scratch = scratch_dir();
    let profile = scratch.join("profilo");
    let outdir = scratch.join("uscita");
    fs::create_dir_all(&profile).map_err(|e| err(e.to_string()))?;
    fs::create_dir_all(&outdir).map_err(|e| err(e.to_string()))?;
    let mut cmd = std::process::Command::new(&bin);
    hidden(&mut cmd)
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .arg("--headless")
        .arg("--nologo")
        .arg("--nofirststartwizard")
        .arg("--norestore")
        .arg("--convert-to")
        .arg(filter)
        .arg("--outdir")
        .arg(&outdir)
        .arg(input);
    let result = cmd.output();
    let produced = fs::read_dir(&outdir).ok().and_then(|entries| {
        entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.is_file())
    });
    let output = match (result, produced) {
        (Ok(out), Some(produced)) if out.status.success() => {
            let dest = unique_output(dir, &stem(input), ext);
            match fs::copy(&produced, &dest) {
                Ok(_) => Ok(Output { path: dest, message: message.into() }),
                Err(e) => Err(err(e.to_string())),
            }
        }
        (Ok(out), _) => {
            let detail = String::from_utf8_lossy(&out.stderr);
            append_log(detail.trim());
            Err(err("LibreOffice non ha completato la conversione"))
        }
        (Err(e), _) => Err(err(e.to_string())),
    };
    let _ = fs::remove_dir_all(&scratch);
    output
}

fn tesseract_text(bin: &Path, input: &Path) -> CedroResult<String> {
    let run = |lang: Option<&str>| {
        let mut cmd = std::process::Command::new(bin);
        hidden(&mut cmd).arg(input).arg("stdout");
        if let Some(lang) = lang {
            cmd.arg("-l").arg(lang);
        }
        cmd.output()
    };
    let output = run(Some("ita+eng"))
        .ok()
        .filter(|o| o.status.success())
        .or_else(|| run(None).ok().filter(|o| o.status.success()))
        .ok_or_else(|| err("Tesseract non ha completato la lettura"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(err("Nessun testo riconosciuto"));
    }
    Ok(text)
}

fn ocr_image(input: &Path, dir: &Path) -> CedroResult<Output> {
    let Some(bin) = tesseract_bin() else {
        return Err(err("Tesseract non trovato. Cedro non lo include: installalo e riprova."));
    };
    let text = tesseract_text(&bin, input)?;
    let dest = unique_output(dir, &stem(input), "txt");
    write_bytes(&dest, text.as_bytes())?;
    Ok(Output { path: dest, message: "Testo letto con Tesseract".into() })
}

fn ocr_pdf(input: &Path, dir: &Path) -> CedroResult<Output> {
    let Some(ocr) = tesseract_bin() else {
        return Err(err("Tesseract non trovato. Cedro non lo include: installalo e riprova."));
    };
    let Some(ppm) = pdftoppm_bin() else {
        return Err(err("Per l'OCR di un PDF serve anche Poppler (pdftoppm), oltre a Tesseract."));
    };
    let scratch = scratch_dir();
    let prefix = scratch.join("pagina");
    let mut render = std::process::Command::new(&ppm);
    hidden(&mut render)
        .arg("-png")
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg("20")
        .arg(input)
        .arg(&prefix);
    let rendered = render.status();
    if rendered.map(|s| !s.success()).unwrap_or(true) {
        let _ = fs::remove_dir_all(&scratch);
        return Err(err("Poppler non ha reso le pagine del PDF"));
    }
    let mut pages: Vec<PathBuf> = fs::read_dir(&scratch)
        .map_err(|e| err(e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    pages.sort_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.rsplit('-').next())
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0)
    });
    if pages.is_empty() {
        let _ = fs::remove_dir_all(&scratch);
        return Err(err("Il PDF non ha pagine leggibili"));
    }
    let mut body = String::new();
    let mut any = false;
    let mut first_err: Option<String> = None;
    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            body.push_str("\n\n");
        }
        body.push_str(&format!("--- pagina {} ---\n", i + 1));
        match tesseract_text(&ocr, page) {
            Ok(text) => {
                any = true;
                body.push_str(&text);
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.0);
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    if !any {
        return Err(err(first_err.unwrap_or_else(|| "Nessun testo riconosciuto".into())));
    }
    let dest = unique_output(dir, &stem(input), "txt");
    write_bytes(&dest, body.as_bytes())?;
    Ok(Output {
        path: dest,
        message: "Testo OCR con Tesseract. Al massimo 20 pagine.".into(),
    })
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

    if matches!(action, "wav" | "mp3" | "mp4") {
        let ext = action;
        let out = unique_output(&dir, &stem(path), ext);
        let args: &[&str] = match action {
            "wav" => &["-vn", "-c:a", "pcm_s16le"],
            "mp3" => &["-vn", "-c:a", "libmp3lame", "-q:a", "4"],
            "mp4" => &["-c:v", "libx264", "-crf", "23", "-c:a", "aac"],
            _ => &[],
        };
        ffmpeg(path, &out, args)?;
        let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        return Ok(Output { path: out, message: size_message(before, after) });
    }

    if kind == Kind::Text && matches!(action, "txt" | "md" | "csv" | "pdf" | "upper" | "lower") {
        let text = fs::read_to_string(path).map_err(|e| err(format!("testo non letto: {e}")))?;
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
        return office_convert(path, &dir, "pdf", "pdf", "PDF con LibreOffice");
    }
    if kind == Kind::Pdf && action == "docx" {
        return office_convert(path, &dir, "docx", "docx", "Word con LibreOffice. L'impaginazione può cambiare.");
    }
    if kind == Kind::Pdf && action == "txt" {
        return office_convert(path, &dir, "txt:Text", "txt", "Testo estratto con LibreOffice, senza OCR.");
    }
    if action == "ocr" && kind == Kind::Image {
        return ocr_image(path, &dir);
    }
    if action == "ocr" && kind == Kind::Pdf {
        return ocr_pdf(path, &dir);
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
        let bytes = pdf_from_jpeg(&jpeg, w, h);
        let out = unique_output(&dir, &stem(path), "pdf");
        write_bytes(&out, &bytes)?;
        return Ok(Output { path: out, message: format!("PDF {w}×{h}") });
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

pub fn read_quality() -> u8 {
    let path = config_dir().join("config.txt");
    let Ok(text) = fs::read_to_string(path) else {
        return 82;
    };
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("quality=") {
            if let Ok(n) = v.trim().parse::<u8>() {
                return n.clamp(40, 95);
            }
        }
    }
    82
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("cedro-test-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
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
    fn doc_and_pdf_slices() {
        assert!(kind_of(Path::new("lettera.docx")) == Kind::Doc);
        assert_eq!(slices(Kind::Doc, Mode::Convert)[0].id, "pdf");
        assert_eq!(slices(Kind::Pdf, Mode::Convert)[0].id, "docx");
        assert_eq!(slices(Kind::Pdf, Mode::Tools)[0].id, "ocr");
        assert!(slices(Kind::Image, Mode::Tools).iter().any(|s| s.id == "ocr"));
    }
}
