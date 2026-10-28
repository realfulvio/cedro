use crate::pdf::pdf_from_jpeg;
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Image,
    Text,
    Audio,
    Video,
    Pdf,
    Doc,
    /// Un archivio ZIP: si convertono i file che contiene.
    Archive,
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
    Slice { id: "html", label: "HTML" },
    Slice { id: "md", label: "MD" },
    Slice { id: "csv", label: "CSV" },
    Slice { id: "json", label: "JSON" },
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
const IMAGE_PANEL: &[Slice] = &[
    Slice { id: "png", label: "PNG" },
    Slice { id: "jpeg", label: "JPEG" },
    Slice { id: "webp", label: "WebP" },
    Slice { id: "avif", label: "AVIF" },
    Slice { id: "jxl", label: "JPEG XL" },
    Slice { id: "bmp", label: "BMP" },
    Slice { id: "gif", label: "GIF" },
    Slice { id: "tiff", label: "TIFF" },
    Slice { id: "ico", label: "ICO" },
    Slice { id: "tga", label: "TGA" },
    Slice { id: "qoi", label: "QOI" },
    Slice { id: "pnm", label: "PNM" },
    Slice { id: "hdr", label: "HDR" },
    Slice { id: "exr", label: "OpenEXR" },
    Slice { id: "pdf", label: "PDF" },
];
const AUDIO_PANEL: &[Slice] = &[
    Slice { id: "mp3", label: "MP3" },
    Slice { id: "wav", label: "WAV" },
    Slice { id: "flac", label: "FLAC" },
    Slice { id: "opus", label: "Opus" },
    Slice { id: "aiff", label: "AIFF" },
];
const ARCHIVE_CONVERT: &[Slice] = &[
    Slice { id: "z-webp", label: "Foto in WebP" },
    Slice { id: "z-jpeg", label: "Foto in JPEG" },
    Slice { id: "z-png", label: "Foto in PNG" },
    Slice { id: "z-mp3", label: "Audio in MP3" },
    Slice { id: "z-wav", label: "Audio in WAV" },
    Slice { id: "z-pdf", label: "Documenti in PDF" },
];
const VIDEO_CONVERT: &[Slice] = &[
    Slice { id: "mp4", label: "MP4" },
    Slice { id: "mp3", label: "Solo audio (MP3)" },
];
const DOC_CONVERT: &[Slice] = &[Slice { id: "pdf", label: "PDF" }];
const PDF_CONVERT: &[Slice] = &[
    Slice { id: "jpg", label: "JPG" },
    Slice { id: "docx", label: "Word OCR" },
    Slice { id: "png", label: "PNG" },
    Slice { id: "txt", label: "Testo" },
];
const PDF_TOOLS: &[Slice] = &[Slice { id: "ocr", label: "OCR" }];
const EMPTY: &[Slice] = &[Slice { id: "none", label: "Niente qui" }];

/// I formati di uscita del pannello Conversioni: quelli della ruota più gli altri, che alla ruota
/// non entrano (la ruota ha una tavola con un numero fisso di fette).
pub fn panel_slices(kind: Kind) -> &'static [Slice] {
    match kind {
        Kind::Image => IMAGE_PANEL,
        Kind::Audio => AUDIO_PANEL,
        other => slices(other, Mode::Convert),
    }
}

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
        (Kind::Archive, Mode::Convert) => ARCHIVE_CONVERT,
        _ => EMPTY,
    }
}

/// Testo di un PDF: quello incorporato, e l'OCR solo per le pagine che ne sono prive.
/// Ritorna il testo, le pagine lette via OCR e quelle da leggere rimaste fuori dal limite.
fn pdf_text_with_ocr(path: &Path, opts: &Options) -> Result<(String, usize, usize), String> {
    let all: Vec<String> = crate::doctext::pdf_page_texts(path)?.iter().map(|p| crate::ocr::paragraphs_from_text(p)).collect();
    let selected = pages_of(&opts.pages, all.len());
    if selected.is_empty() {
        return Err("nessuna pagina in questo intervallo".into());
    }
    let mut pages: Vec<String> = selected.iter().map(|i| all[*i].clone()).collect();
    let mut wanted = vec![false; all.len()];
    for (slot, i) in selected.iter().enumerate() {
        wanted[*i] = pages[slot].is_empty();
    }
    let mut ocr_pages = 0;
    let mut skipped = 0;
    let mut ocr_error = None;
    if opts.ocr && wanted.iter().any(|b| *b) {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        match crate::ocr::pdf_pages_where(&bytes, |i| wanted.get(i).copied().unwrap_or(false)) {
            Ok(read) => {
                for (slot, i) in selected.iter().enumerate() {
                    match read.get(*i).cloned().flatten() {
                        Some(text) if !text.is_empty() => {
                            pages[slot] = text;
                            ocr_pages += 1;
                        }
                        None if pages[slot].is_empty() && wanted[*i] => skipped += 1,
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

/// Il tipo di file secondo l'estensione.
fn kind_by_extension(ext: &str) -> Kind {
    match ext {
        "pdf" => Kind::Pdf,
        "doc" | "docx" | "odt" | "rtf" => Kind::Doc,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "ico" | "svg" | "svgz" | "avif" | "jxl" | "tga" | "qoi" | "pnm" | "ppm" | "pgm" | "pbm" | "hdr" | "exr" | "dds" => Kind::Image,
        "txt" | "md" | "csv" | "tsv" | "json" | "log" | "xml" | "html" => Kind::Text,
        "wav" | "mp3" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "flac" | "aif" | "aiff" | "aifc" | "caf" => Kind::Audio,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => Kind::Video,
        "zip" => Kind::Archive,
        _ => Kind::Other,
    }
}

/// Il tipo di file: dall'estensione, e dal contenuto quando l'estensione manca, non è conosciuta o
/// dice una cosa diversa da quella che il file è (una foto WebP chiamata «.jpg»). I contenitori che
/// possono ospitare tipi diversi (MP4, ZIP, Matroska) si giudicano solo dall'estensione.
pub fn kind_of(path: &Path) -> Kind {
    let by_ext = kind_by_extension(&extension(path));
    let Some(real) = crate::imgfmt::sniff_file(path) else {
        return by_ext;
    };
    let sure = matches!(real, "png" | "jpg" | "gif" | "webp" | "bmp" | "tiff" | "avif" | "jxl" | "qoi" | "dds" | "exr" | "hdr" | "pdf" | "flac" | "wav" | "aiff" | "ogg" | "opus" | "svg");
    let by_content = kind_by_extension(real);
    if by_content != Kind::Other && by_content != by_ext && (by_ext == Kind::Other || sure) {
        return by_content;
    }
    by_ext
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

/// Lo stato dei nomi di uscita per la conversione in corso su questo thread.
#[derive(Default)]
struct Naming {
    exists: Exists,
    ask: Option<ExistsAsk>,
    /// Il file di partenza: non si sostituisce mai.
    source: Option<PathBuf>,
    /// La risposta già data per questo file: si chiede una volta sola.
    decided: Option<ExistsChoice>,
    /// Le copie fatte con un nome nuovo ma da buttare, perché l'utente ha scelto di saltare.
    skipped: Vec<PathBuf>,
    /// Il modello del nome di uscita (`%nome%_%data%`), se scelto.
    template: Option<String>,
}

thread_local! {
    static NAMING: std::cell::RefCell<Naming> = std::cell::RefCell::new(Naming::default());
}

/// «nome.ext», o solo «nome» per i file senza estensione.
fn file_name(file_stem: &str, ext: &str) -> String {
    if ext.is_empty() {
        file_stem.to_string()
    } else {
        format!("{file_stem}.{ext}")
    }
}

fn numbered(dir: &Path, file_stem: &str, ext: &str) -> PathBuf {
    let mut name = file_name(file_stem, ext);
    let mut i = 2u32;
    while dir.join(&name).exists() {
        name = file_name(&format!("{file_stem}-{i}"), ext);
        i += 1;
    }
    dir.join(name)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Il nome di uscita secondo il modello scelto: il nome del file di partenza è sostituito dal modello,
/// e quello che il motore aggiunge in fondo («-compresso», «-2»…) resta.
fn templated_stem(file_stem: &str) -> String {
    NAMING.with(|n| {
        let n = n.borrow();
        let (Some(template), Some(source)) = (n.template.as_deref(), n.source.as_deref()) else {
            return file_stem.to_string();
        };
        let base = stem(source);
        let Some(tail) = file_stem.strip_prefix(base.as_str()) else {
            return file_stem.to_string();
        };
        crate::nametemplate::expand(template, &base, &extension(source), tail, crate::nametemplate::now()).unwrap_or_else(|| file_stem.to_string())
    })
}

/// Il nome per la copia, secondo la scelta per i nomi che esistono già. Con «sostituisci» riusa il
/// nome già presente, ma mai quello del file di partenza: una conversione verso lo stesso formato non
/// deve riscrivere l'originale.
pub fn unique_output(dir: &Path, file_stem: &str, ext: &str) -> PathBuf {
    let templated = templated_stem(file_stem);
    let file_stem = templated.as_str();
    let candidate = dir.join(file_name(file_stem, ext));
    if !candidate.exists() {
        return candidate;
    }
    let (exists, ask, source, decided) = NAMING.with(|n| {
        let n = n.borrow();
        (n.exists, n.ask.clone(), n.source.clone(), n.decided)
    });
    let choice = match exists {
        Exists::Number => ExistsChoice::KeepBoth,
        Exists::Replace => ExistsChoice::Replace,
        Exists::Skip => ExistsChoice::Skip,
        Exists::Ask => decided.unwrap_or_else(|| {
            let answer = ask.map_or(ExistsChoice::KeepBoth, |a| (a.0)(&candidate));
            NAMING.with(|n| n.borrow_mut().decided = Some(answer));
            answer
        }),
    };
    match choice {
        ExistsChoice::Replace if !source.as_deref().is_some_and(|s| same_file(&candidate, s)) => candidate,
        ExistsChoice::Replace | ExistsChoice::KeepBoth => numbered(dir, file_stem, ext),
        ExistsChoice::Skip => {
            let path = numbered(dir, file_stem, ext);
            NAMING.with(|n| n.borrow_mut().skipped.push(path.clone()));
            path
        }
    }
}

/// Il nome per un file scritto fuori da `perform_with` (per esempio una trascrizione), con la stessa
/// regola per i nomi che esistono già. `None` se il file esiste e si è scelto di saltarlo.
pub fn choose_output(source: &Path, dir: &Path, file_stem: &str, ext: &str, opts: &Options) -> Option<PathBuf> {
    let before = NAMING.with(|n| {
        n.replace(Naming { exists: opts.exists, ask: opts.on_exists.clone(), source: Some(source.to_path_buf()), template: opts.name_template.clone(), ..Default::default() })
    });
    let out = unique_output(dir, file_stem, ext);
    let used = NAMING.with(|n| n.replace(before));
    used.skipped.is_empty().then_some(out)
}

/// Il nome di uscita è da saltare (esiste già e si è scelto di non toccarlo): le conversioni lunghe non partono neppure.
fn skipping(out: &Path) -> bool {
    NAMING.with(|n| n.borrow().skipped.iter().any(|p| p == out))
}

/// Scrive il PDF di testo (documenti e testi): font incorporato, segnalibri, PDF/A e password come scelto.
fn write_text_pdf(path: &Path, dir: &Path, blocks: &[crate::pdfdoc::DocBlock], opts: &Options) -> CedroResult<Output> {
    let password = opts.password.as_deref().filter(|p| !p.is_empty());
    if opts.pdfa && password.is_some() {
        return Err(err("il PDF/A non ammette la password: togline una delle due"));
    }
    let options = crate::pdfdoc::PdfOptions {
        size: opts.page_size,
        embed: opts.embed_fonts,
        bookmarks: opts.bookmarks,
        pdfa: opts.pdfa,
        title: stem(path),
    };
    let mut bytes = crate::pdfdoc::build(blocks, &options).map_err(err)?;
    let mut message = String::from("PDF");
    if opts.embed_fonts || opts.pdfa {
        message.push_str(" con i caratteri incorporati");
    }
    if opts.pdfa {
        message.push_str(", PDF/A-1b");
    }
    if opts.bookmarks && blocks.iter().any(|b| b.heading > 0) {
        message.push_str(", con i segnalibri");
    }
    if let Some(password) = password {
        bytes = crate::pdf::protect(&bytes, password).map_err(err)?;
        message.push_str(", protetto con password");
    }
    let out = unique_output(dir, &stem(path), "pdf");
    write_bytes(&out, &bytes)?;
    Ok(Output { path: out, message })
}

/// La cartella dove va la copia: accanto al file, in «Cedro» accanto al file, o quella fissa.
fn output_dir(path: &Path, settings: &crate::settings::Settings) -> CedroResult<PathBuf> {
    use crate::settings::Dest;
    let beside = parent_dir(path)?.to_path_buf();
    let dir = match settings.dest {
        Dest::Same => return Ok(beside),
        Dest::Sub => beside.join("Cedro"),
        Dest::Fixed if settings.dest_dir.trim().is_empty() => return Ok(beside),
        Dest::Fixed => PathBuf::from(settings.dest_dir.trim()),
    };
    fs::create_dir_all(&dir).map_err(|e| err(format!("cartella di destinazione non creata ({}): {e}", dir.display())))?;
    Ok(dir)
}

fn parent_dir(path: &Path) -> CedroResult<&Path> {
    path.parent().filter(|p| !p.as_os_str().is_empty()).or(Some(Path::new("."))).ok_or_else(|| err("cartella mancante"))
}

/// L'immagine senza trasparenza: con `white` i pixel trasparenti diventano bianchi, altrimenti si scarta l'alfa.
fn flatten(img: &DynamicImage, white: bool) -> image::RgbImage {
    if !white || !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        out.put_pixel(x, y, image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
    }
    out
}

fn save_image(img: &DynamicImage, path: &Path, format: ImageFormat, quality: u8, white: bool) -> CedroResult<()> {
    match format {
        ImageFormat::Ico => {
            let small = crate::imgfmt::ico_ready(img);
            return DynamicImage::ImageRgba8(small.to_rgba8()).save_with_format(path, format).map_err(|e| err(e.to_string()));
        }
        ImageFormat::Qoi => {
            let ready = if img.color().has_alpha() { DynamicImage::ImageRgba8(img.to_rgba8()) } else { DynamicImage::ImageRgb8(img.to_rgb8()) };
            return ready.save_with_format(path, format).map_err(|e| err(e.to_string()));
        }
        ImageFormat::Pnm => {
            use image::codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding};
            use image::ImageEncoder;
            let rgb = flatten(img, white);
            let mut file = fs::File::create(path).map_err(|e| err(e.to_string()))?;
            return PnmEncoder::new(&mut file)
                .with_subtype(PnmSubtype::Pixmap(SampleEncoding::Binary))
                .write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
                .map_err(|e| err(e.to_string()));
        }
        ImageFormat::Hdr => return DynamicImage::ImageRgb32F(img.to_rgb32f()).save_with_format(path, format).map_err(|e| err(e.to_string())),
        ImageFormat::OpenExr => return DynamicImage::ImageRgba32F(img.to_rgba32f()).save_with_format(path, format).map_err(|e| err(e.to_string())),
        _ => {}
    }
    if format == ImageFormat::Jpeg {
        let rgb = flatten(img, white);
        let mut file = fs::File::create(path).map_err(|e| err(e.to_string()))?;
        let mut enc = JpegEncoder::new_with_quality(&mut file, quality);
        enc.encode(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
            .map_err(|e| err(e.to_string()))?;
        return Ok(());
    }
    img.save_with_format(path, format).map_err(|e| err(e.to_string()))
}

fn load_image(path: &Path, straighten: bool, target_side: Option<u32>) -> CedroResult<DynamicImage> {
    crate::imgfmt::open(path, straighten, target_side).map_err(err)
}

fn jpeg_bytes(img: &DynamicImage, quality: u8, white: bool) -> CedroResult<(Vec<u8>, u32, u32)> {
    let rgb = flatten(img, white);
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
    perform_into(path, action, quality, None)
}

/// Come `perform`, ma con `out_dir` le copie nuove vengono scritte in quella cartella.
/// Per i nomi che esistono già vale la scelta delle impostazioni (aggiungi -2 o sostituisci).
pub fn perform_into(path: &Path, action: &str, quality: u8, out_dir: Option<&Path>) -> CedroResult<Output> {
    let exists = if crate::settings::read_settings().replace { Exists::Replace } else { Exists::Number };
    perform_with(path, action, quality, out_dir, &Options { exists, ..Options::default() })
}

/// Cosa fare quando il file di uscita esiste già.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Exists {
    /// Aggiunge -2, -3… al nome.
    #[default]
    Number,
    /// Sostituisce il file (mai quello di partenza).
    Replace,
    /// Non converte: il file c'è già.
    Skip,
    /// Chiede a chi ha lanciato la conversione (vedi `Options::on_exists`).
    Ask,
}

/// La risposta alla domanda «il file esiste già: cosa faccio?».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExistsChoice {
    Replace,
    KeepBoth,
    Skip,
}

/// Chi risponde alla domanda quando la scelta è `Exists::Ask`. Gira nel thread della conversione.
#[derive(Clone)]
pub struct ExistsAsk(pub std::sync::Arc<dyn Fn(&Path) -> ExistsChoice + Send + Sync>);

impl ExistsAsk {
    pub fn new(f: impl Fn(&Path) -> ExistsChoice + Send + Sync + 'static) -> Self {
        ExistsAsk(std::sync::Arc::new(f))
    }
}

impl PartialEq for ExistsAsk {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ExistsAsk {}
impl std::fmt::Debug for ExistsAsk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExistsAsk")
    }
}

/// Le pagine scelte di un PDF, come si scrivono: `1-3,5` o `4-` (dalla 4 alla fine).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PageSel(Vec<(usize, usize)>);

impl PageSel {
    /// Il testo vuoto vuol dire tutte le pagine (`None`).
    pub fn parse(text: &str) -> Result<Option<PageSel>, String> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("tutte") {
            return Ok(None);
        }
        let mut ranges = Vec::new();
        for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
            let number = |t: &str| t.trim().parse::<usize>().ok().filter(|n| *n >= 1);
            let range = match part.split_once('-') {
                Some((a, b)) if b.trim().is_empty() => number(a).map(|a| (a, usize::MAX)),
                Some((a, b)) if a.trim().is_empty() => number(b).map(|b| (1, b)),
                Some((a, b)) => number(a).zip(number(b)).filter(|(a, b)| a <= b),
                None => number(part).map(|n| (n, n)),
            };
            ranges.push(range.ok_or_else(|| format!("pagine «{part}» non valide: scrivi per esempio 1-3,5"))?);
        }
        Ok(Some(PageSel(ranges)))
    }

    /// L'ultima pagina chiesta espressamente (da 1); `None` se l'ultimo intervallo va fino alla fine.
    pub fn max_page(&self) -> Option<usize> {
        let max = self.0.iter().map(|(a, b)| if *b == usize::MAX { *a } else { *b }).max()?;
        Some(max)
    }

    /// `index` parte da 0 (la prima pagina è 0).
    pub fn contains(&self, index: usize) -> bool {
        let page = index + 1;
        self.0.iter().any(|(a, b)| page >= *a && page <= *b)
    }
}

/// Le pagine (da 0) di un documento di `total` pagine che la scelta lascia passare.
fn pages_of(sel: &Option<PageSel>, total: usize) -> Vec<usize> {
    (0..total).filter(|i| sel.as_ref().map_or(true, |s| s.contains(*i))).collect()
}

/// Formato della pagina per i documenti convertiti in PDF.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageSize {
    A4,
    A3,
    Letter,
}

/// Codifica dei file di testo scritti.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Windows1252,
    Latin1,
}

/// Opzioni extra del pannello Conversioni. `Default` = il comportamento di sempre.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Options {
    /// PDF in immagini: risoluzione delle pagine in dpi (di norma 144).
    pub dpi: Option<u32>,
    /// Immagini: lato più lungo massimo in pixel; le immagini più piccole restano come sono.
    pub max_side: Option<u32>,
    /// Immagini: applica la rotazione dei dati EXIF (foto del telefono).
    pub straighten: bool,
    /// Immagini: nel JPEG la trasparenza diventa bianca invece che nera.
    pub white_background: bool,
    /// Cosa fare se il nome di uscita esiste già.
    pub exists: Exists,
    /// Chi risponde con `Exists::Ask`.
    pub on_exists: Option<ExistsAsk>,
    /// Documenti in PDF: formato della pagina.
    pub page_size: PageSize,
    /// PDF: legge con l'OCR le pagine senza testo.
    pub ocr: bool,
    /// PDF in Word: ricostruisce le tabelle.
    pub tables: bool,
    /// PDF: solo queste pagine (`None` = tutte).
    pub pages: Option<PageSel>,
    /// PDF in Word: le colonne del testo restano colonne (spenta: una colonna sola, in ordine di lettura).
    pub columns: bool,
    /// Audio: salta il silenzio iniziale, sotto questa soglia in dB sotto il massimo.
    pub trim_silence_db: Option<u32>,
    /// Documenti e testo in PDF: font incorporato nel file.
    pub embed_fonts: bool,
    /// Documenti in PDF: segnalibri dai titoli.
    pub bookmarks: bool,
    /// Documenti e testo in PDF: PDF/A-1b per l'archivio.
    pub pdfa: bool,
    /// PDF in immagini: una immagine per pagina (altrimenti una sola, lunga).
    pub one_image_per_page: bool,
    /// Testo: codifica del file scritto.
    pub encoding: Encoding,
    /// Testo: separatore del CSV scritto (e letto, se non è quello giusto si indovina).
    pub csv_separator: Option<char>,
    /// Testo: JSON con rientri (altrimenti su una riga).
    pub pretty_json: bool,
    /// Testo: la prima riga del CSV contiene i titoli.
    pub header_row: bool,
    /// Audio: bitrate dell'MP3 in kbps.
    pub mp3_kbps: Option<u32>,
    /// Audio: 1 mono, 2 stereo, None come l'originale.
    pub channels: Option<u8>,
    /// Audio: porta il picco al massimo.
    pub normalize: bool,
    /// Audio: salta i primi N secondi.
    pub trim_start_secs: u32,
    /// Audio: titolo, artista e album passano all'MP3.
    pub keep_tags: bool,
    /// Video: altezza massima in pixel.
    pub video_height: Option<u32>,
    /// Video: qualità 40-95 (più alta, file più grande).
    pub video_quality: Option<u8>,
    /// Video: fotogrammi al secondo massimi.
    pub fps: Option<u32>,
    /// Video: senza traccia audio.
    pub mute: bool,
    /// Video: l'MP4 parte subito sul web (moov davanti ai dati).
    pub faststart: bool,
    /// Documenti in PDF: password per aprire il file.
    pub password: Option<String>,
    /// Il nome dei file scritti, a modello: `%nome%`, `%ext%`, `%data%`, `%ora%`, `%unix%`.
    pub name_template: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            dpi: None,
            max_side: None,
            straighten: true,
            white_background: true,
            exists: Exists::Number,
            on_exists: None,
            page_size: PageSize::A4,
            ocr: true,
            tables: true,
            pages: None,
            columns: true,
            trim_silence_db: None,
            embed_fonts: true,
            bookmarks: true,
            pdfa: false,
            one_image_per_page: true,
            encoding: Encoding::Utf8,
            csv_separator: None,
            pretty_json: true,
            header_row: true,
            mp3_kbps: None,
            channels: None,
            normalize: false,
            trim_start_secs: 0,
            keep_tags: true,
            video_height: None,
            video_quality: None,
            fps: None,
            mute: false,
            faststart: false,
            password: None,
            name_template: None,
        }
    }
}

/// Come `perform_into`, con le opzioni extra.
pub fn perform_with(path: &Path, action: &str, quality: u8, out_dir: Option<&Path>, opts: &Options) -> CedroResult<Output> {
    let before = NAMING.with(|n| {
        n.replace(Naming { exists: opts.exists, ask: opts.on_exists.clone(), source: Some(path.to_path_buf()), template: opts.name_template.clone(), ..Default::default() })
    });
    let result = perform_inner(path, action, quality, out_dir, opts);
    let used = NAMING.with(|n| n.replace(before));
    if !used.skipped.is_empty() {
        for file in &used.skipped {
            let _ = fs::remove_file(file);
        }
        return Err(err("il file esiste già: saltato"));
    }
    result
}

fn perform_inner(path: &Path, action: &str, quality: u8, out_dir: Option<&Path>, opts: &Options) -> CedroResult<Output> {
    if !path.exists() {
        return Err(err("file non trovato"));
    }
    let quality = quality.clamp(40, 95);
    let settings = crate::settings::read_settings();
    let dir = match out_dir {
        Some(d) => {
            fs::create_dir_all(d).map_err(|e| err(e.to_string()))?;
            d.to_path_buf()
        }
        // Senza una cartella scelta vale quella delle impostazioni: accanto, in «Cedro» o fissa.
        None => output_dir(path, &settings)?,
    };
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
        // Senza estensione la copia non ne prende una (prima diventava «nome-copia.copia»).
        let out = unique_output(&dir, &format!("{}-copia", stem(path)), &extension(path));
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

    if matches!(action, "wav" | "mp3" | "flac" | "aiff" | "opus") {
        let out = unique_output(&dir, &stem(path), action);
        if skipping(&out) {
            return Err(err("il file esiste già: saltato"));
        }
        let result = match action {
            "wav" => crate::audio::to_wav(path, &out, opts),
            "mp3" => crate::audio::to_mp3(path, &out, quality, opts),
            "flac" => crate::audio::to_flac(path, &out, opts),
            "aiff" => crate::audio::to_aiff(path, &out, opts),
            _ => crate::audio::to_opus(path, &out, quality, opts),
        };
        if let Err(e) = result {
            let _ = fs::remove_file(&out);
            return Err(err(e));
        }
        let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        return Ok(Output { path: out, message: size_message(before, after) });
    }
    if kind == Kind::Archive && action.starts_with("z-") {
        let (out, message) = crate::archive::convert(path, action, quality, &dir, opts).map_err(err)?;
        return Ok(Output { path: out, message });
    }
    if action == "mp4" {
        let out = unique_output(&dir, &stem(path), "mp4");
        if skipping(&out) {
            return Err(err("il file esiste già: saltato"));
        }
        let shape = crate::sink::VideoShape {
            height: opts.video_height,
            fps: opts.fps,
            mute: opts.mute,
            quality,
            gpu: crate::accel::use_gpu(),
        };
        let result = crate::sink::with_shape(shape, || {
            #[cfg(windows)]
            {
                crate::video::to_mp4(path, &out)
            }
            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                crate::linux_video::to_mp4(path, &out)
            }
            #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
            {
                Err::<(), String>("il video MP4 si converte su Windows, su Linux e su Android".into())
            }
        });
        let result = result.and_then(|()| if opts.faststart { crate::mp4fast::make_faststart(&out) } else { Ok(()) });
        if let Err(e) = result {
            let _ = fs::remove_file(&out);
            return Err(err(e));
        }
        let after = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let mut message = size_message(before, after);
        if crate::sink::gpu_used() {
            message.push_str(" · con la scheda video");
        }
        return Ok(Output { path: out, message });
    }

    if kind == Kind::Text && matches!(action, "txt" | "md" | "csv" | "html" | "json" | "pdf" | "upper" | "lower") {
        let text = read_text(path)?;
        if action == "pdf" {
            let blocks: Vec<crate::pdfdoc::DocBlock> =
                text.split('\n').map(|l| crate::pdfdoc::DocBlock { text: l.trim_end().to_string(), heading: 0, image: None }).collect();
            return write_text_pdf(path, &dir, &blocks, opts);
        }
        let from = extension(path);
        let table = |text: &str| {
            let sep = crate::textconv::detect_separator(text);
            crate::textconv::parse_csv(text, sep)
        };
        let (body, ext, suffix, message) = match action {
            "upper" => (text.to_uppercase(), from.clone(), "-maiuscolo", "Testo scritto"),
            "lower" => (text.to_lowercase(), from.clone(), "-minuscolo", "Testo scritto"),
            "md" if from == "csv" => (crate::textconv::markdown_table(&table(&text), opts.header_row), "md".into(), "", "Tabella Markdown"),
            "md" => (text, "md".into(), "", "Testo scritto"),
            "csv" if from == "csv" => {
                let sep = opts.csv_separator.unwrap_or_else(|| crate::textconv::detect_separator(&text));
                (crate::textconv::write_csv(&table(&text), sep), "csv".into(), "", "CSV riscritto")
            }
            "csv" => (text, "csv".into(), "", "Testo scritto"),
            "html" => {
                let body = if from == "csv" {
                    crate::textconv::html_table(&table(&text), opts.header_row)
                } else {
                    crate::textconv::html_paragraphs(&text)
                };
                let mut page = crate::textconv::html_page(&stem(path), &body);
                match opts.encoding {
                    Encoding::Utf8 => {}
                    Encoding::Windows1252 => page = page.replacen("charset=\"utf-8\"", "charset=\"windows-1252\"", 1),
                    Encoding::Latin1 => page = page.replacen("charset=\"utf-8\"", "charset=\"iso-8859-1\"", 1),
                }
                (page, "html".into(), "", "Pagina HTML")
            }
            "json" if from == "json" => (crate::textconv::json_format(&text, opts.pretty_json).map_err(err)?, "json".into(), "-leggibile", "JSON riscritto"),
            "json" if from == "csv" => (crate::textconv::csv_to_json(&table(&text), opts.header_row, opts.pretty_json), "json".into(), "", "JSON dal CSV"),
            "json" => return Err(err("il JSON si ottiene da un file JSON o CSV")),
            _ => (text, "txt".into(), "", "Testo scritto"),
        };
        let suffix = if action == "json" && from == "csv" { "" } else { suffix };
        let file_stem = if suffix.is_empty() { stem(path) } else { format!("{}{suffix}", stem(path)) };
        let out = unique_output(&dir, &file_stem, if ext.is_empty() { "txt" } else { &ext });
        write_bytes(&out, &crate::textconv::encode(&body, opts.encoding))?;
        return Ok(Output { path: out, message: message.into() });
    }

    if kind == Kind::Doc && action == "pdf" {
        let blocks = crate::doctext::document_blocks(path).map_err(err)?;
        return write_text_pdf(path, &dir, &blocks, opts);
    }
    if kind == Kind::Pdf && matches!(action, "jpg" | "png") {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let all = crate::ocr::pdf_page_count(&bytes).map_err(err)?;
        let selected = pages_of(&opts.pages, all);
        let total = selected.len();
        if total == 0 {
            return Err(err("nessuna pagina in questo intervallo"));
        }
        let scale = opts.dpi.map_or(settings.pdf_scale(), |d| d.clamp(36, 600) as f32 / 72.0);
        let encode = |rgb: &image::RgbImage| -> Result<Vec<u8>, String> {
            let mut out = Vec::new();
            if action == "jpg" {
                JpegEncoder::new_with_quality(&mut out, quality).encode_image(rgb).map_err(|e| e.to_string())?;
            } else {
                rgb.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| e.to_string())?;
            }
            Ok(out)
        };
        let mut first = None;
        let mut strip: Vec<image::RgbImage> = Vec::new();
        let mut strip_height = 0u32;
        crate::ocr::pdf_page_pngs_scaled(&bytes, scale, |i| selected.contains(&i), |i, png| {
            let rgb = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgb8();
            if !opts.one_image_per_page {
                strip_height += rgb.height();
                if strip_height > 60000 {
                    return Err("troppe pagine per un'immagine sola: scegli «Una immagine per pagina» o meno dpi".into());
                }
                strip.push(rgb);
                return Ok(());
            }
            let name = if total == 1 { stem(path) } else { format!("{}-{}", stem(path), i + 1) };
            let out = unique_output(&dir, &name, action);
            write_bytes(&out, &encode(&rgb)?).map_err(|e| e.to_string())?;
            first.get_or_insert(out);
            Ok(())
        })
        .map_err(err)?;
        let label = action.to_uppercase();
        if !opts.one_image_per_page {
            let width = strip.iter().map(|p| p.width()).max().unwrap_or(1);
            let mut long = image::RgbImage::from_pixel(width, strip_height.max(1), image::Rgb([255, 255, 255]));
            let mut y = 0;
            for page in &strip {
                image::imageops::replace(&mut long, page, 0, y as i64);
                y += page.height();
            }
            let out = unique_output(&dir, &stem(path), action);
            write_bytes(&out, &encode(&long).map_err(err)?)?;
            return Ok(Output { path: out, message: format!("{label} unico, {total} pagine una sotto l'altra") });
        }
        let message = if total == 1 { label } else { format!("{total} {label}, uno per pagina") };
        let path = first.ok_or_else(|| err("nessuna pagina convertita"))?;
        return Ok(Output { path, message });
    }
    if kind == Kind::Pdf && action == "docx" {
        let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
        let want = |i: usize| opts.pages.as_ref().map_or(true, |s| s.contains(i));
        let read = crate::pdflayout::ReadOptions { tables: opts.tables, columns: opts.columns };
        let every = crate::pdflayout::read_pages(&bytes, &read, want).map_err(err)?;
        let selected = pages_of(&opts.pages, every.len());
        if selected.is_empty() {
            return Err(err("nessuna pagina in questo intervallo"));
        }
        let scan_at: Vec<bool> = every.iter().map(|p| p.is_scan()).collect();
        let pages: Vec<crate::pdflayout::Page> =
            every.into_iter().enumerate().filter(|(i, _)| selected.contains(i)).map(|(_, p)| p).collect();
        let scanned = selected.iter().filter(|i| scan_at[**i]).count();
        let mut ocr: Vec<Option<String>> = vec![None; pages.len()];
        // Le scansioni vanno nel Word come immagini della pagina; con l'OCR acceso il testo letto
        // resta nel testo alternativo, per cercarlo e copiarlo.
        if scanned > 0 && opts.ocr {
            if let Ok(read) = crate::ocr::pdf_pages_where(&bytes, |i| scan_at.get(i).copied().unwrap_or(false)) {
                ocr = selected.iter().map(|i| read.get(*i).cloned().flatten()).collect();
            }
        }
        let docx = crate::pdflayout::docx(&pages, |i| ocr.get(i).cloned().flatten().unwrap_or_default());
        let out = unique_output(&dir, &stem(path), "docx");
        write_bytes(&out, &docx)?;
        let message = if scanned == 0 {
            "Word modificabile: titoli, paragrafi e tabelle del PDF".to_string()
        } else if scanned == pages.len() && opts.ocr {
            "Word con le pagine scansionate come immagini; il testo letto con l'OCR è nel testo alternativo".to_string()
        } else if scanned == pages.len() {
            "Word con le pagine scansionate come immagini".to_string()
        } else {
            format!("Word modificabile; {scanned} pagine scansionate messe come immagini")
        };
        return Ok(Output { path: out, message });
    }
    if kind == Kind::Pdf && action == "txt" {
        let (text, ocr_pages, skipped) = pdf_text_with_ocr(path, opts).map_err(err)?;
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
            message.push_str(&format!(" Mancano {skipped} pagine scansionate: l'OCR ne legge al massimo {}.", settings.ocr_pages()));
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
        let (text, read_by_ocr) = match crate::doctext::pdf_text(path) {
            Ok(text) => (text, false),
            Err(_) => (crate::ocr::pdf_pages_text(&bytes).map_err(err)?, true),
        };
        let out = unique_output(&dir, &stem(path), "txt");
        write_bytes(&out, text.as_bytes())?;
        let mut message = "Testo del PDF.".to_string();
        if read_by_ocr {
            let total = crate::ocr::pdf_page_count(&bytes).unwrap_or(0);
            if total > settings.ocr_pages() {
                message.push_str(&format!(" L'OCR ha letto {} pagine su {total}: le altre mancano.", settings.ocr_pages()));
            }
        }
        return Ok(Output { path: out, message });
    }

    if kind != Kind::Image {
        return Err(err("questa voce non si applica a questo file"));
    }

    let mut img = load_image(path, opts.straighten, opts.max_side)?;
    if let Some(side) = opts.max_side.filter(|s| *s > 0 && img.width().max(img.height()) > *s) {
        img = img.resize(side, side, image::imageops::FilterType::Lanczos3);
    }
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
        let (jpeg, w, h) = jpeg_bytes(&img, 90, opts.white_background)?;
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

    if matches!(action, "avif" | "jxl") {
        let bytes = if action == "avif" { crate::imgfmt::avif_bytes(&img, quality) } else { crate::imgfmt::jxl_bytes(&img) }.map_err(err)?;
        let out = unique_output(&dir, &stem(path), action);
        write_bytes(&out, &bytes)?;
        let message = if action == "avif" { size_message(before, bytes.len() as u64) } else { format!("JPEG XL senza perdita · {}", size_message(before, bytes.len() as u64)) };
        return Ok(Output { path: out, message });
    }

    let (format, ext, label_stem) = match action {
        "ico" => (ImageFormat::Ico, "ico", ""),
        "tga" => (ImageFormat::Tga, "tga", ""),
        "qoi" => (ImageFormat::Qoi, "qoi", ""),
        "pnm" => (ImageFormat::Pnm, "ppm", ""),
        "hdr" => (ImageFormat::Hdr, "hdr", ""),
        "exr" => (ImageFormat::OpenExr, "exr", ""),
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
    let q = if action == "compress" { quality.min(settings.compress_quality()) } else { quality };
    save_image(&img, &out, format, q, opts.white_background)?;
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

pub(crate) fn decode_text(bytes: &[u8]) -> String {
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

/// Una cartella nuova nella cartella temporanea, che solo questo utente può leggere e scrivere.
/// Su Linux /tmp è di tutti: con un nome fisso un altro utente potrebbe preparare lui la cartella
/// (o un collegamento) e cambiare quello che Cedro ci scrive.
pub fn private_temp_dir(prefix: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let dir = base.join(format!("{prefix}-{}-{}-{nanos}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        #[allow(unused_mut)]
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        // `create`, non `create_all`: se esiste già (anche fatta da altri) si prova un altro nome.
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cartella temporanea non creata: {e}")),
        }
    }
    Err("cartella temporanea non creata".into())
}

pub fn append_log(line: &str) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("log.txt");
    // Il registro non cresce per sempre: oltre 1 MB il vecchio diventa log-precedente.txt.
    if fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = fs::rename(&path, dir.join("log-precedente.txt"));
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::pdf_from_text;
    use image::{Rgb, RgbImage};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("cedro-test-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Ogni cartella temporanea è nuova, mai una che c'era già, e su Linux solo nostra.
    #[test]
    fn private_temp_dirs_are_new_and_only_ours() {
        let a = private_temp_dir("cedro-prova-privata").unwrap();
        let b = private_temp_dir("cedro-prova-privata").unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_dir(&a).unwrap().count(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o700);
        }
        let _ = fs::remove_dir_all(a);
        let _ = fs::remove_dir_all(b);
    }

    #[test]
    fn duplicate_without_extension_stays_without() {
        let dir = scratch();
        let src = dir.join("LEGGIMI");
        fs::write(&src, b"testo").unwrap();
        let first = perform_into(&src, "dup", 80, Some(&dir)).unwrap();
        assert_eq!(first.path, dir.join("LEGGIMI-copia"));
        let second = perform_into(&src, "dup", 80, Some(&dir)).unwrap();
        assert_eq!(second.path, dir.join("LEGGIMI-copia-2"));
        assert_eq!(fs::read(&second.path).unwrap(), b"testo");
        let _ = fs::remove_dir_all(dir);
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
        let styles = crate::doctext::zip_entry_for_test(&docx.path, "word/styles.xml");
        assert!(styles.contains("Helvetica") || styles.contains("Arial"), "{styles}");
        // Il Word è compresso: il testo pesa poco.
        let entry = zip::ZipArchive::new(fs::File::open(&docx.path).unwrap()).unwrap().by_name("word/document.xml").unwrap().compression();
        assert_eq!(entry, zip::CompressionMethod::Deflated);
    }

    /// Un PDF di una pagina con Helvetica (F1) e Helvetica-Bold (F2).
    fn pdf_page(content: &str, w: u32, h: u32) -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>"),
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
        pdf
    }

    fn word_xml(name: &str, pdf: Vec<u8>) -> String {
        let dir = scratch();
        let path = dir.join(name);
        fs::write(&path, pdf).unwrap();
        let out = perform(&path, "docx", 80).expect("Word");
        crate::doctext::zip_entry_for_test(&out.path, "word/document.xml")
    }

    #[test]
    fn pdf_columns_become_word_columns() {
        // Due colonne di testo pieno: a sinistra «Sinistra N», a destra «Destra N».
        let mut content = String::from("BT /F2 18 Tf 50 760 Td (Notiziario) Tj ET ");
        let words = "lorem ipsum dolor sit amet consectetur";
        for i in 0..12 {
            let y = 720 - i * 14;
            content.push_str(&format!("BT /F1 10 Tf 50 {y} Td (Sinistra {i} {words}) Tj ET BT /F1 10 Tf 320 {y} Td (Destra {i} {words}) Tj ET "));
        }
        let xml = word_xml("colonne.pdf", pdf_page(&content, 595, 842));
        assert!(xml.contains("<w:cols w:num=\"2\""), "{xml}");
        assert!(xml.contains("w:type=\"column\""), "{xml}");
        // Si legge prima tutta la colonna di sinistra, poi quella di destra.
        let last_left = xml.find("Sinistra 11").unwrap();
        let first_right = xml.find("Destra 0").unwrap();
        assert!(last_left < first_right, "{xml}");
    }

    #[test]
    fn pdf_form_keeps_boxes_and_places_text() {
        // Un modulo: 10x12 caselle, ognuna con l'etichetta piccola e il valore; più una scritta
        // fuori dal foglio, che non si vede e non deve finire nel Word.
        let mut content = String::from("0.5 w ");
        for r in 0..12 {
            for c in 0..10 {
                let (x, y) = (40 + c * 52, 760 - r * 50);
                content.push_str(&format!(
                    "{x} {y} 50 40 re S BT /F1 5 Tf {} {} Td (Casella {r}{c}) Tj ET BT /F2 6 Tf {} {} Td ({c}) Tj ET ",
                    x + 2,
                    y + 33,
                    x + 4,
                    y + 10
                ));
            }
        }
        content.push_str("BT /F1 8 Tf 900 800 Td (NASCOSTO) Tj ET ");
        // Una riga tratteggiata e un riquadro grigio ritagliato a metà.
        content.push_str("[3 2] 0 d 40 150 m 560 150 l S [] 0 d q 40 100 100 20 re W n 0.8 g 40 100 200 20 re f Q");
        let xml = word_xml("modulo.pdf", pdf_page(&content, 595, 842));
        assert!(xml.contains("<w:framePr"), "{xml}");
        // Righe e caselle restano forme di Word, dietro al testo, con il loro tratteggio.
        assert!(xml.contains("<wps:wsp>") && xml.contains("behindDoc=\"1\""), "{xml}");
        assert!(xml.contains("<a:custDash>"), "{xml}");
        // Il riquadro grigio si ferma al ritaglio: 140 pt, non 240.
        assert!(xml.contains(&format!("<a:pt x=\"{}\"", 140 * 12700)), "{xml}");
        assert!(!xml.contains(&format!("<a:pt x=\"{}\"", 240 * 12700)), "{xml}");
        assert!(xml.contains("Casella 75"), "{xml}");
        assert!(!xml.contains("NASCOSTO"), "{xml}");
    }

    #[test]
    fn pdf_lists_become_word_lists() {
        let mut content = String::new();
        let items = [(700, "1. Accendere il PC"), (686, "2. Aprire il portale"), (672, "3. Inviare la richiesta")];
        for (y, t) in items {
            content.push_str(&format!("BT /F1 10 Tf 50 {y} Td ({t}) Tj ET "));
        }
        for (i, t) in ["Monitor", "Stampanti"].iter().enumerate() {
            let y = 640 - i as i32 * 14;
            content.push_str(&format!("BT /F1 10 Tf 50 {y} Td (\\225) Tj ET BT /F1 10 Tf 64 {y} Td ({t}) Tj ET "));
        }
        content.push_str("BT /F1 10 Tf 50 600 Td (Fine del documento con un paragrafo normale.) Tj ET");
        let xml = word_xml("elenchi.pdf", pdf_page(&content, 400, 800));
        assert_eq!(xml.matches("<w:numPr>").count(), 5, "{xml}");
        assert!(xml.contains(">Accendere il PC<") && !xml.contains("1. Accendere"), "{xml}");
        assert!(xml.contains(">Monitor<"), "{xml}");
    }

    #[test]
    fn word_made_pdf_keeps_lists_and_columns() {
        // Esportato da Writer: elenchi puntati su due livelli, elenco numerato, due colonne.
        let pdf = include_bytes!("../tests/fixtures/circolare.pdf").to_vec();
        let xml = word_xml("circolare.pdf", pdf);
        assert!(xml.contains("<w:ilvl w:val=\"1\"/>"), "manca il secondo livello");
        assert!(xml.contains(">Aprire un ticket sul portale.<"), "numero non tolto");
        assert!(xml.contains("<w:cols w:num=\"2\""), "mancano le colonne");
        let xml = word_xml("colonne.pdf", include_bytes!("../tests/fixtures/colonne.pdf").to_vec());
        assert!(xml.contains("<w:cols w:num=\"2\""), "mancano le colonne");
        assert!(xml.contains("<w:drawing>"), "manca il grafico");
        assert!(!xml.contains(">Gen<"), "etichette del grafico fuori dall'immagine");
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
        let pdf = pdf_page(content, 400, 800);
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
        let flat = perform_with(&path, "docx", 80, None, &Options { tables: false, exists: Exists::Replace, ..Default::default() }).expect("Word");
        let xml = crate::doctext::zip_entry_for_test(&flat.path, "word/document.xml");
        assert_eq!(xml.matches("<w:tr>").count(), 0, "senza tabelle");
        assert!(xml.contains("Luca") && xml.contains("Voto"), "il testo della tabella resta");
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
        // La scansione resta un'immagine della pagina, il testo OCR va nel testo alternativo.
        let docx = perform(&pdf, "docx", 80).expect("OCR");
        let xml = crate::doctext::zip_entry_for_test(&docx.path, "word/document.xml");
        assert!(xml.contains("behindDoc=\"1\""), "{xml}");
        let descr = xml.split("descr=\"").nth(1).unwrap_or_default();
        assert!(descr.contains("Cedro"), "{xml}");
    }

    #[test]
    fn pdf_to_jpg_follows_the_dpi_option() {
        let dir = scratch();
        let jpeg = include_bytes!("../tests/fixtures/scan.jpg");
        let pdf = dir.join("scansione.pdf");
        fs::write(&pdf, crate::pdf::pdf_from_jpeg(jpeg, 900, 200, &[])).unwrap();
        let low = perform_with(&pdf, "jpg", 80, None, &Options { dpi: Some(72), ..Default::default() }).unwrap();
        let low_w = image::open(&low.path).unwrap().width();
        let high = perform_with(&pdf, "jpg", 80, None, &Options { dpi: Some(144), ..Default::default() }).unwrap();
        let high_w = image::open(&high.path).unwrap().width();
        assert!(high_w >= low_w * 2 - 2 && high_w <= low_w * 2 + 2, "{low_w} {high_w}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn images_shrink_to_the_longest_side_only_when_larger() {
        let dir = scratch();
        let src = dir.join("grande.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 200, Rgb([10, 90, 140]))).save(&src).unwrap();
        let small = Options { max_side: Some(100), ..Default::default() };
        let out = perform_with(&src, "jpeg", 80, None, &small).unwrap();
        let img = image::open(&out.path).unwrap();
        assert_eq!((img.width(), img.height()), (100, 50));
        let keep = Options { max_side: Some(800), ..Default::default() };
        let out = perform_with(&src, "webp", 80, None, &keep).unwrap();
        assert_eq!(image::open(&out.path).unwrap().width(), 400);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Un JPEG 40×20 con l'orientamento EXIF 6 (da ruotare di 90° per metterlo dritto).
    fn rotated_jpeg(path: &Path) {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(40, 20, Rgb([200, 30, 30])));
        let mut plain = Vec::new();
        img.write_to(&mut Cursor::new(&mut plain), ImageFormat::Jpeg).unwrap();
        let tiff: Vec<u8> = [
            &b"Exif\0\0"[..],
            &[b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0][..],
        ]
        .concat();
        let mut with = plain[..2].to_vec();
        with.extend_from_slice(&[0xFF, 0xE1]);
        with.extend_from_slice(&((tiff.len() + 2) as u16).to_be_bytes());
        with.extend_from_slice(&tiff);
        with.extend_from_slice(&plain[2..]);
        fs::write(path, with).unwrap();
    }

    #[test]
    fn straighten_follows_the_exif_rotation_only_when_asked() {
        let dir = scratch();
        let src = dir.join("telefono.jpg");
        rotated_jpeg(&src);
        let on = perform_with(&src, "png", 80, None, &Options::default()).unwrap();
        let on = image::open(&on.path).unwrap();
        assert_eq!((on.width(), on.height()), (20, 40));
        let off = perform_with(&src, "webp", 80, None, &Options { straighten: false, ..Default::default() }).unwrap();
        let off = image::open(&off.path).unwrap();
        assert_eq!((off.width(), off.height()), (40, 20));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn jpeg_flattens_transparency_to_white_or_black() {
        let dir = scratch();
        let src = dir.join("vuoto.png");
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 0]))).save(&src).unwrap();
        let white = perform_with(&src, "jpeg", 90, None, &Options::default()).unwrap();
        let px = image::open(&white.path).unwrap().to_rgb8().get_pixel(8, 8).0;
        assert!(px.iter().all(|c| *c > 240), "{px:?}");
        let black = perform_with(&src, "jpeg", 90, None, &Options { white_background: false, ..Default::default() }).unwrap();
        let px = image::open(&black.path).unwrap().to_rgb8().get_pixel(8, 8).0;
        assert!(px.iter().all(|c| *c < 20), "{px:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_names_get_a_suffix_or_are_replaced() {
        let dir = scratch();
        let src = dir.join("foto.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))).save(&src).unwrap();
        perform_with(&src, "jpeg", 80, None, &Options::default()).unwrap();
        let second = perform_with(&src, "jpeg", 80, None, &Options::default()).unwrap();
        assert!(second.path.ends_with("foto-2.jpg"));
        let replaced = perform_with(&src, "jpeg", 80, None, &Options { exists: Exists::Replace, ..Default::default() }).unwrap();
        assert!(replaced.path.ends_with("foto.jpg"));
        assert!(!dir.join("foto-3.jpg").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_options_reach_the_written_files() {
        let dir = scratch();
        let csv = dir.join("presenze.csv");
        fs::write(&csv, "nome;città
Luca;Bondeno
").unwrap();
        let comma = perform_with(&csv, "csv", 80, None, &Options { csv_separator: Some(','), ..Default::default() }).unwrap();
        assert_eq!(fs::read_to_string(&comma.path).unwrap(), "nome,città
Luca,Bondeno
");
        let latin = perform_with(&csv, "md", 80, None, &Options { encoding: Encoding::Latin1, ..Default::default() }).unwrap();
        let bytes = fs::read(&latin.path).unwrap();
        assert!(bytes.contains(&0xE0), "città in Latin-1: {bytes:?}");
        assert!(String::from_utf8(bytes).is_err());
        let md = perform_with(&csv, "md", 80, None, &Options { header_row: false, ..Default::default() }).unwrap();
        assert!(fs::read_to_string(&md.path).unwrap().starts_with("|  |  |
| --- | --- |
| nome | città |"));
        let html = perform_with(&csv, "html", 80, None, &Options::default()).unwrap();
        assert!(fs::read_to_string(&html.path).unwrap().contains("<th>nome</th>"));
        let json = perform_with(&csv, "json", 80, None, &Options { pretty_json: false, ..Default::default() }).unwrap();
        assert_eq!(fs::read_to_string(&json.path).unwrap(), "[{\"nome\":\"Luca\",\"città\":\"Bondeno\"}]");

        let raw = dir.join("config.json");
        fs::write(&raw, "{\"a\":[1,2],\"b\":{\"c\":true}}").unwrap();
        let pretty = perform_with(&raw, "json", 80, None, &Options::default()).unwrap();
        assert!(fs::read_to_string(&pretty.path).unwrap().contains("{
  \"a\": [
    1,"));
        assert!(perform_with(&raw, "html", 80, None, &Options::default()).is_ok());
        assert!(perform_with(&dir.join("presenze.csv"), "txt", 80, None, &Options::default()).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Un WAV a 16 bit; con `title` e `artist` aggiunge anche il blocco LIST/INFO.
    fn wav_file(path: &Path, channels: u16, seconds: u32, amplitude: f32, tags: Option<(&str, &str)>) {
        let rate = 44100u32;
        let mut data = Vec::new();
        let mut seed = 12345u32;
        for i in 0..rate * seconds {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 16) as f32 / 65535.0 - 0.5) * 0.5;
            let v = ((i as f32 * 0.05).sin() * 0.5 + noise) * amplitude;
            for _ in 0..channels {
                data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
            }
        }
        let mut riff = Vec::new();
        riff.extend_from_slice(b"WAVEfmt ");
        riff.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, channels] {
            riff.extend_from_slice(&v.to_le_bytes());
        }
        riff.extend_from_slice(&rate.to_le_bytes());
        riff.extend_from_slice(&(rate * 2 * channels as u32).to_le_bytes());
        riff.extend_from_slice(&(2 * channels).to_le_bytes());
        riff.extend_from_slice(&16u16.to_le_bytes());
        if let Some((title, artist)) = tags {
            let item = |id: &[u8], text: &str| {
                let mut v = id.to_vec();
                let mut t = text.as_bytes().to_vec();
                t.push(0);
                if t.len() % 2 == 1 {
                    t.push(0);
                }
                v.extend_from_slice(&(t.len() as u32).to_le_bytes());
                v.extend_from_slice(&t);
                v
            };
            let mut info = b"INFO".to_vec();
            info.extend(item(b"INAM", title));
            info.extend(item(b"IART", artist));
            riff.extend_from_slice(b"LIST");
            riff.extend_from_slice(&(info.len() as u32).to_le_bytes());
            riff.extend_from_slice(&info);
        }
        riff.extend_from_slice(b"data");
        riff.extend_from_slice(&(data.len() as u32).to_le_bytes());
        riff.extend_from_slice(&data);
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(riff.len() as u32).to_le_bytes());
        file.extend_from_slice(&riff);
        fs::write(path, file).unwrap();
    }

    #[test]
    fn audio_options_shape_the_output() {
        let dir = scratch();
        let src = dir.join("voce.wav");
        wav_file(&src, 2, 2, 0.1, None);
        let read = |p: &Path| {
            let mut r = hound::WavReader::open(p).unwrap();
            let spec = r.spec();
            let peak = r.samples::<i16>().map(|s| s.unwrap().unsigned_abs()).max().unwrap_or(0);
            (spec.channels, r.duration(), peak)
        };
        let plain = perform_with(&src, "wav", 80, None, &Options { exists: Exists::Replace, ..Default::default() }).unwrap();
        let (ch, frames, peak) = read(&plain.path);
        assert_eq!((ch, frames), (2, 88200));
        assert!(peak < 6000, "{peak}");
        let shaped = Options { channels: Some(1), normalize: true, trim_start_secs: 1, exists: Exists::Replace, ..Default::default() };
        let out = perform_with(&src, "wav", 80, None, &shaped).unwrap();
        let (ch, frames, peak) = read(&out.path);
        assert_eq!((ch, frames), (1, 44100));
        assert!((31000..=32100).contains(&peak), "{peak}");
        let stereo = dir.join("mono.wav");
        wav_file(&stereo, 1, 1, 0.3, None);
        let up = perform_with(&stereo, "wav", 80, None, &Options { channels: Some(2), exists: Exists::Replace, ..Default::default() }).unwrap();
        assert_eq!(read(&up.path).0, 2);
        assert!(perform_with(&src, "wav", 80, None, &Options { trim_start_secs: 9, ..Default::default() }).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn leading_silence_is_detected_and_cut() {
        let dir = scratch();
        let src = dir.join("parlato.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for _ in 0..8000 * 2 {
            w.write_sample(3i16).unwrap();
        }
        for i in 0..8000 {
            w.write_sample(((i as f32 * 0.3).sin() * 12000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let frames = |p: &Path| hound::WavReader::open(p).unwrap().duration();
        let whole = perform_with(&src, "wav", 80, None, &Options::default()).unwrap();
        assert_eq!(frames(&whole.path), 24000);
        let cut = Options { trim_silence_db: Some(40), exists: Exists::Replace, ..Default::default() };
        let out = perform_with(&src, "wav", 80, None, &cut).unwrap();
        // 1 s di suono e 50 ms di anticipo.
        assert!((8000 + 396..=8000 + 404).contains(&frames(&out.path)), "{}", frames(&out.path));
        let quiet = dir.join("muto.wav");
        let mut w = hound::WavWriter::create(&quiet, spec).unwrap();
        for _ in 0..8000 {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        assert!(perform_with(&quiet, "wav", 80, None, &cut).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn page_ranges_parse_like_they_are_written() {
        let sel = PageSel::parse("1-3, 5;8-").unwrap().unwrap();
        let picked: Vec<usize> = (0..10).filter(|i| sel.contains(*i)).collect();
        assert_eq!(picked, vec![0, 1, 2, 4, 7, 8, 9]);
        assert!(PageSel::parse("").unwrap().is_none() && PageSel::parse("tutte").unwrap().is_none());
        assert!(PageSel::parse("3-1").is_err() && PageSel::parse("a").is_err() && PageSel::parse("0").is_err());
        assert!(PageSel::parse("-2").unwrap().unwrap().contains(1));
    }

    #[test]
    fn existing_names_follow_the_chosen_policy_and_ask_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let dir = scratch();
        let src = dir.join("foto.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))).save(&src).unwrap();
        perform_with(&src, "jpeg", 80, None, &Options::default()).unwrap();
        let jpgs = || fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "jpg")).count();
        assert_eq!(jpgs(), 1);

        let skip = Options { exists: Exists::Skip, ..Default::default() };
        let e = perform_with(&src, "jpeg", 80, None, &skip).err().unwrap();
        assert!(e.0.contains("saltato"), "{}", e.0);
        assert_eq!(jpgs(), 1, "il file saltato non deve restare");

        let asked = Arc::new(AtomicUsize::new(0));
        let answer = |choice: ExistsChoice, asked: &Arc<AtomicUsize>| {
            let asked = asked.clone();
            Options {
                exists: Exists::Ask,
                on_exists: Some(ExistsAsk::new(move |_| {
                    asked.fetch_add(1, Ordering::SeqCst);
                    choice
                })),
                ..Default::default()
            }
        };
        let both = perform_with(&src, "jpeg", 80, None, &answer(ExistsChoice::KeepBoth, &asked)).unwrap();
        assert!(both.path.ends_with("foto-2.jpg"));
        assert_eq!(jpgs(), 2);
        let before = fs::metadata(dir.join("foto.jpg")).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        perform_with(&src, "jpeg", 80, None, &answer(ExistsChoice::Replace, &asked)).unwrap();
        assert_eq!(jpgs(), 2, "sostituire non crea altri file");
        assert!(fs::metadata(dir.join("foto.jpg")).unwrap().modified().unwrap() > before);
        let e = perform_with(&src, "jpeg", 80, None, &answer(ExistsChoice::Skip, &asked)).err().unwrap();
        assert!(e.0.contains("saltato"));
        assert_eq!(asked.load(Ordering::SeqCst), 3, "una domanda per conversione");
        assert_eq!(jpgs(), 2);

        // L'audio da saltare non si converte neppure, e non lascia file.
        let audio = dir.join("nota.flac");
        fs::write(&audio, include_bytes!("../tests/fixtures/nota.flac")).unwrap();
        perform_with(&audio, "wav", 80, None, &Options::default()).unwrap();
        let e = perform_with(&audio, "wav", 80, None, &skip).err().unwrap();
        assert!(e.0.contains("saltato"), "{}", e.0);
        assert!(!dir.join("nota-2.wav").exists());

        // Un PDF di tre pagine in immagini: la domanda è una sola, non una per pagina.
        let text: Vec<String> = (1..=150).map(|n| format!("riga {n}")).collect();
        let pdf = dir.join("lungo.pdf");
        fs::write(&pdf, pdf_from_text(&text.join("\n"))).unwrap();
        let asked = Arc::new(AtomicUsize::new(0));
        let opts = Options { dpi: Some(36), ..answer(ExistsChoice::KeepBoth, &asked) };
        perform_with(&pdf, "png", 80, None, &opts).unwrap();
        perform_with(&pdf, "png", 80, None, &opts).unwrap();
        assert!(asked.load(Ordering::SeqCst) <= 2, "una domanda per file, non per pagina: {}", asked.load(Ordering::SeqCst));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mp3_bitrate_and_tags() {
        let dir = scratch();
        let src = dir.join("delibera.wav");
        wav_file(&src, 2, 3, 0.9, Some(("Delibera 12", "Comune di Bondeno")));
        let small = perform_with(&src, "mp3", 80, None, &Options { mp3_kbps: Some(96), ..Default::default() }).unwrap();
        let big = perform_with(&src, "mp3", 80, None, &Options { mp3_kbps: Some(320), ..Default::default() }).unwrap();
        let (small_len, big_len) = (fs::metadata(&small.path).unwrap().len(), fs::metadata(&big.path).unwrap().len());
        assert!(big_len > small_len * 2, "{small_len} {big_len}");
        let has = |p: &Path, needle: &[u8]| fs::read(p).unwrap().windows(needle.len()).any(|w| w == needle);
        assert!(has(&small.path, b"Delibera 12"));
        assert!(has(&small.path, b"Comune di Bondeno"));
        let bare = perform_with(&src, "mp3", 80, None, &Options { keep_tags: false, exists: Exists::Replace, ..Default::default() }).unwrap();
        assert!(!has(&bare.path, b"Delibera 12"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn long_pdf(dir: &Path) -> PathBuf {
        let text: Vec<String> = (1..=100).map(|n| format!("riga numero {n}")).collect();
        let pdf = dir.join("lungo.pdf");
        fs::write(&pdf, pdf_from_text(&text.join("
"))).unwrap();
        pdf
    }

    #[test]
    fn pdf_pages_option_limits_every_output() {
        let dir = scratch();
        let pdf = long_pdf(&dir);
        let first = Options { pages: PageSel::parse("1").unwrap(), ..Default::default() };
        let one = perform_with(&pdf, "jpg", 80, None, &first).unwrap();
        assert_eq!(one.message, "JPG");
        let all = perform_with(&pdf, "png", 80, None, &Options { dpi: Some(36), ..Default::default() }).unwrap();
        assert!(all.message.starts_with("2 PNG"), "{}", all.message);
        let text = perform_with(&pdf, "txt", 80, None, &first).unwrap();
        let body = fs::read_to_string(&text.path).unwrap();
        assert!(body.contains("riga numero 1"));
        assert!(!body.contains("riga numero 100"), "solo la prima pagina");
        let full = perform_with(&pdf, "txt", 80, None, &Options { exists: Exists::Replace, ..Default::default() }).unwrap();
        assert!(fs::read_to_string(&full.path).unwrap().contains("riga numero 100"));
        let docx = perform_with(&pdf, "docx", 80, None, &first).unwrap();
        let xml = crate::doctext::zip_entry_for_test(&docx.path, "word/document.xml");
        assert!(xml.contains("riga numero 1") && !xml.contains("riga numero 100"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pdf_pages_can_become_one_long_image() {
        let dir = scratch();
        let pdf = long_pdf(&dir);
        let each = Options { dpi: Some(36), pages: PageSel::parse("1-2").unwrap(), ..Default::default() };
        let single = Options { one_image_per_page: false, ..each.clone() };
        let one_page = perform_with(&pdf, "png", 80, None, &Options { pages: PageSel::parse("1").unwrap(), dpi: Some(36), ..Default::default() }).unwrap();
        let page = image::open(&one_page.path).unwrap();
        let long = perform_with(&pdf, "png", 80, None, &single).unwrap();
        let long_img = image::open(&long.path).unwrap();
        assert_eq!(long_img.width(), page.width());
        assert_eq!(long_img.height(), page.height() * 2);
        assert!(long.message.contains("una sotto l'altra"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scanned_pdf_text_comes_only_with_the_ocr_option() {
        let dir = scratch();
        let jpeg = include_bytes!("../tests/fixtures/scan.jpg");
        let pdf = dir.join("scansione.pdf");
        fs::write(&pdf, crate::pdf::pdf_from_jpeg(jpeg, 900, 200, &[])).unwrap();
        let off = Options { ocr: false, exists: Exists::Replace, ..Default::default() };
        // In Word la scansione va comunque come immagine; senza OCR non c'è il testo alternativo.
        let word = perform_with(&pdf, "docx", 80, None, &off).unwrap();
        assert!(word.message.contains("come immagini") && !word.message.contains("OCR"), "{}", word.message);
        assert!(perform_with(&pdf, "txt", 80, None, &off).is_err());
        let on = perform_with(&pdf, "docx", 80, None, &Options { exists: Exists::Replace, ..Default::default() }).unwrap();
        assert!(on.message.contains("testo alternativo"), "{}", on.message);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn document_pdf_follows_page_size_and_password() {
        let dir = scratch();
        let src = dir.join("delibera.rtf");
        fs::write(&src, br"{\rtf1\ansi Delibera segreta del comune}").unwrap();
        let a3 = perform_with(&src, "pdf", 80, None, &Options { page_size: PageSize::A3, ..Default::default() }).unwrap();
        let bytes = fs::read(&a3.path).unwrap();
        assert!(bytes.windows(20).any(|w| w == b"/MediaBox [0 0 842 1".as_slice()));
        let letter = perform_with(&src, "pdf", 80, None, &Options { page_size: PageSize::Letter, ..Default::default() }).unwrap();
        assert!(fs::read(&letter.path).unwrap().windows(18).any(|w| w == b"[0 0 612 792] /Con".as_slice()));

        let locked = Options { password: Some("bondeno".into()), ..Default::default() };
        let out = perform_with(&src, "pdf", 80, None, &locked).unwrap();
        assert!(out.message.contains("password"));
        let bytes = fs::read(&out.path).unwrap();
        assert!(bytes.windows(8).any(|w| w == b"/Encrypt"));
        assert!(!bytes.windows(8).any(|w| w == b"segreta "), "il testo non deve essere in chiaro");
        let mut wrong = lopdf::Document::load_mem(&bytes).unwrap();
        assert!(wrong.decrypt("sbagliata").is_err());
        let mut right = lopdf::Document::load_mem(&bytes).unwrap();
        right.decrypt("bondeno").unwrap();
        let text = right.extract_text(&[1]).unwrap();
        assert!(text.contains("Delibera"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Un PDF di una pagina 600×800 con il contenuto dato, i font F1 (Helvetica) e F2 (grassetto).
    fn one_page_pdf(content: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_string(),
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
        pdf
    }

    #[test]
    fn single_column_joins_the_paragraph_split_by_the_column() {
        let dir = scratch();
        let mut content = String::new();
        for n in 0..14 {
            let y = 700 - n * 14;
            let left = if n == 13 { "fine della colonna sinistra, la frase prosegue".to_string() } else { format!("testo lungo della colonna sinistra, riga {n:02} qui") };
            let right = if n == 0 { "nella colonna destra e finisce col punto.".to_string() } else { format!("testo lungo della colonna destra, riga {n:02} qui") };
            content.push_str(&format!("BT /F1 10 Tf 50 {y} Td ({left}) Tj ET\nBT /F1 10 Tf 320 {y} Td ({right}) Tj ET\n"));
        }
        let path = dir.join("articolo.pdf");
        fs::write(&path, one_page_pdf(&content)).unwrap();
        let xml_of = |columns: bool| {
            let out = perform_with(&path, "docx", 80, None, &Options { columns, exists: Exists::Replace, ..Default::default() }).unwrap();
            crate::doctext::zip_entry_for_test(&out.path, "word/document.xml")
        };
        let kept = xml_of(true);
        assert!(kept.contains("<w:cols ") && !kept.contains("prosegue nella"), "{kept}");
        let flat = xml_of(false);
        assert!(!flat.contains("<w:cols "), "{flat}");
        assert!(flat.contains("la frase prosegue nella colonna destra"), "{flat}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_column_pdf_keeps_columns_or_flows_in_one() {
        let dir = scratch();
        let mut content = String::from("BT /F2 18 Tf 50 750 Td (Titolo a tutta larghezza della pagina del giornale comunale) Tj ET\n");
        for n in 1..=12 {
            let y = 700 - n * 16;
            content.push_str(&format!("BT /F1 10 Tf 50 {y} Td (Sinistra {n} testo lungo della colonna di sinistra qui) Tj ET\n"));
            content.push_str(&format!("BT /F1 10 Tf 320 {y} Td (Destra {n} testo lungo della colonna di destra qui) Tj ET\n"));
        }
        let path = dir.join("giornale.pdf");
        fs::write(&path, one_page_pdf(&content)).unwrap();
        let xml_of = |opts: &Options| {
            let out = perform_with(&path, "docx", 80, None, opts).unwrap();
            crate::doctext::zip_entry_for_test(&out.path, "word/document.xml")
        };
        let at = |xml: &str, needle: &str| xml.find(needle).unwrap_or_else(|| panic!("manca «{needle}»"));

        let kept = xml_of(&Options::default());
        assert!(kept.contains("<w:cols w:num=\"2\""), "{kept}");
        assert!(kept.contains("w:type=\"column\""));
        assert!(kept.contains("<w:type w:val=\"continuous\"/>"), "il titolo sta in una sezione a una colonna");
        assert!(at(&kept, "Titolo a tutta") < at(&kept, "Sinistra 1 "));
        assert!(at(&kept, "Sinistra 12") < at(&kept, "Destra 1 "), "prima tutta la colonna di sinistra");
        assert_eq!(kept.matches("<w:cols ").count(), 1);

        let flat = xml_of(&Options { columns: false, exists: Exists::Replace, ..Default::default() });
        assert!(!flat.contains("<w:cols "), "{flat}");
        assert!(!flat.contains("w:type=\"column\""));
        assert!(at(&flat, "Titolo a tutta") < at(&flat, "Sinistra 1 "));
        assert!(at(&flat, "Sinistra 12") < at(&flat, "Destra 1 "), "in colonna singola ma in ordine di lettura");

        // Con un piede a tutta larghezza sotto le colonne: colonne vere, poi di nuovo una colonna sulla stessa pagina.
        let footed = dir.join("piede.pdf");
        let footer = "BT /F1 10 Tf 50 420 Td (Piede a tutta pagina, dopo le colonne: chiude l'articolo del giornale comunale) Tj ET\n";
        fs::write(&footed, one_page_pdf(&format!("{content}{footer}"))).unwrap();
        let out = perform_with(&footed, "docx", 80, None, &Options::default()).unwrap();
        let xml = crate::doctext::zip_entry_for_test(&out.path, "word/document.xml");
        assert_eq!(xml.matches("<w:cols w:num=\"2\"").count(), 1, "{xml}");
        assert_eq!(xml.matches("<w:sectPr>").count(), 3, "titolo, colonne, piede");
        assert!(at(&xml, "Sinistra 12") < at(&xml, "Destra 1 "));
        assert!(at(&xml, "Destra 12") < at(&xml, "Piede a tutta"));
        assert!(at(&xml, "<w:cols ") < at(&xml, "Piede a tutta"), "il piede sta dopo la fine delle colonne");
        assert!(!xml.contains("destra qui Sinistra") && !xml.contains("sinistra qui Destra"), "le righe di colonne diverse non si uniscono");

        // Un testo a una colonna sola non cambia.
        let single: String = (1..=14).map(|n| format!("BT /F1 10 Tf 50 {} Td (Riga {n} di un testo lungo a tutta pagina, che occupa la larghezza intera della pagina) Tj ET\n", 700 - n * 16)).collect();
        let one = dir.join("uno.pdf");
        fs::write(&one, one_page_pdf(&single)).unwrap();
        let out = perform_with(&one, "docx", 80, None, &Options::default()).unwrap();
        assert!(!crate::doctext::zip_entry_for_test(&out.path, "word/document.xml").contains("<w:cols "));
        let _ = fs::remove_dir_all(&dir);
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
        assert!(kind_of(Path::new("nota.opus")) == Kind::Audio);
        assert_eq!(slices(Kind::Other, Mode::Convert)[0].id, "none");
        // Un file .opus rotto non diventa un WAV vuoto: l'errore è chiaro.
        let opus = dir.join("nota.opus");
        fs::write(&opus, b"OpusHead").unwrap();
        assert!(perform(&opus, "wav", 80).is_err(), "un Opus rotto non deve diventare un WAV");
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

    /// Con la scheda video permessa il video esce comunque: dalla GPU se c'è, altrimenti dal processore.
    /// Con CEDRO_GPU_TEST=1 la GPU deve averlo fatto davvero (prove su NVIDIA, Intel e AMD).
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn video_with_gpu_allowed() {
        let dir = std::env::temp_dir().join(format!("cedro-gpu-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Su Windows la sorgente è un MP4 640×360 fatto col processore: i video di prova sono più piccoli
        // della misura minima della scheda (NVENC parte da circa 145×49) e Windows li darebbe al software.
        #[cfg(windows)]
        let src = {
            let src = dir.join("sorgente.mp4");
            crate::video::synthetic_mp4(&src, 640, 360, 60, false).expect("sorgente");
            src
        };
        #[cfg(target_os = "linux")]
        let src = {
            // 320×240: sotto una certa misura (circa 160×96) le schede AMD non aprono il codificatore.
            let src = dir.join("vp9.webm");
            fs::write(&src, include_bytes!("../tests/fixtures/vp9-320.webm")).unwrap();
            src
        };
        let mp4 = dir.join("convertito.mp4");
        let shape = crate::sink::VideoShape { gpu: true, ..Default::default() };
        let (result, used) = crate::sink::with_shape(shape, || {
            #[cfg(windows)]
            let made = crate::video::to_mp4(&src, &mp4);
            #[cfg(target_os = "linux")]
            let made = crate::linux_video::to_mp4(&src, &mp4);
            (made, crate::sink::gpu_used())
        });
        result.expect("MP4");
        eprintln!("video fatto con la scheda video: {used}");
        if std::env::var_os("CEDRO_GPU_TEST").is_some() {
            assert!(used, "la scheda video non ha codificato");
        }
        let out = fs::read(&mp4).unwrap();
        assert!(mp4_box_len(&out, b"mdat") > 32, "mdat vuoto");
        if let Some(codecs) = probe_codecs(&mp4) {
            assert!(codecs.contains("h264"), "ffprobe: {codecs}");
        }
        // Dove c'è ffmpeg, il video si deve leggere tutto senza errori.
        if let Ok(check) = std::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&mp4).args(["-f", "null", "-"]).output() {
            assert!(check.status.success() && check.stderr.is_empty(), "ffmpeg: {}", String::from_utf8_lossy(&check.stderr));
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

    #[cfg(any(windows, target_os = "linux"))]
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

    /// Le figlie di un box MP4: (tipo, contenuto).
    fn mp4_children(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
            if size < 8 || i + size > data.len() {
                break;
            }
            out.push(([data[i + 4], data[i + 5], data[i + 6], data[i + 7]], &data[i + 8..i + size]));
            i += size;
        }
        out
    }

    fn mp4_find<'a>(data: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
        let (first, rest) = path.split_first()?;
        let child = mp4_children(data).into_iter().find(|(t, _)| t == *first)?.1;
        if rest.is_empty() {
            Some(child)
        } else {
            mp4_find(child, rest)
        }
    }

    struct Mp4Info {
        width: u32,
        height: u32,
        frames: u32,
        seconds: f64,
        has_audio: bool,
        moov_first: bool,
    }

    fn mp4_info(bytes: &[u8]) -> Mp4Info {
        let top: Vec<[u8; 4]> = mp4_children(bytes).into_iter().map(|(t, _)| t).collect();
        let moov_first = top.iter().position(|t| t == b"moov") < top.iter().position(|t| t == b"mdat");
        let moov = mp4_children(bytes).into_iter().find(|(t, _)| t == b"moov").expect("moov").1;
        let mut info = Mp4Info { width: 0, height: 0, frames: 0, seconds: 0.0, has_audio: false, moov_first };
        for (kind, trak) in mp4_children(moov) {
            if &kind != b"trak" {
                continue;
            }
            let hdlr = mp4_find(trak, &[b"mdia", b"hdlr"]).expect("hdlr");
            let handler = &hdlr[8..12];
            if handler == b"soun" {
                info.has_audio = true;
            }
            if handler != b"vide" {
                continue;
            }
            let tkhd = mp4_find(trak, &[b"tkhd"]).expect("tkhd");
            let at = if tkhd[0] == 1 { 88 } else { 76 };
            info.width = u32::from_be_bytes(tkhd[at..at + 4].try_into().unwrap()) >> 16;
            info.height = u32::from_be_bytes(tkhd[at + 4..at + 8].try_into().unwrap()) >> 16;
            let stsz = mp4_find(trak, &[b"mdia", b"minf", b"stbl", b"stsz"]).expect("stsz");
            info.frames = u32::from_be_bytes(stsz[8..12].try_into().unwrap());
            let mdhd = mp4_find(trak, &[b"mdia", b"mdhd"]).expect("mdhd");
            let (scale, duration) = if mdhd[0] == 1 {
                (
                    u32::from_be_bytes(mdhd[20..24].try_into().unwrap()) as f64,
                    u64::from_be_bytes(mdhd[24..32].try_into().unwrap()) as f64,
                )
            } else {
                (
                    u32::from_be_bytes(mdhd[12..16].try_into().unwrap()) as f64,
                    u32::from_be_bytes(mdhd[16..20].try_into().unwrap()) as f64,
                )
            };
            info.seconds = duration / scale;
        }
        info
    }

    #[test]
    fn video_options_shape_every_pipeline() {
        let dir = scratch();
        let fixtures: [(&str, &[u8]); 4] = [
            ("audio.mp4", include_bytes!("../tests/fixtures/audio.mp4")),
            ("audio.mov", include_bytes!("../tests/fixtures/audio.mov")),
            ("audio.mkv", include_bytes!("../tests/fixtures/audio.mkv")),
            ("vp8.webm", include_bytes!("../tests/fixtures/vp8.webm")),
        ];
        for (name, bytes) in fixtures {
            let src = dir.join(name);
            fs::write(&src, bytes).unwrap();
            let plain = perform_with(&src, "mp4", 80, None, &Options::default()).unwrap_or_else(|e| panic!("{name}: {}", e.0));
            let base = mp4_info(&fs::read(&plain.path).unwrap());
            assert!(base.frames >= 4 && base.width >= 32, "{name}: {}x{} {} frame", base.width, base.height, base.frames);
            assert!(base.has_audio, "{name}: manca l'audio");
            assert!(!base.moov_first, "{name}: di norma il moov sta in fondo");

            let target = (base.height * 3 / 4) & !1;
            let small = Options { video_height: Some(target), exists: Exists::Replace, ..Default::default() };
            let out = perform_with(&src, "mp4", 80, None, &small).unwrap_or_else(|e| panic!("{name} ridotto: {}", e.0));
            let info = mp4_info(&fs::read(&out.path).unwrap());
            assert_eq!(info.height, target, "{name}: altezza");
            assert!((info.width as i64 - (base.width * 3 / 4) as i64).abs() <= 2, "{name}: larghezza {} su {}", info.width, base.width);
            assert!(info.frames >= base.frames - 1, "{name}: fotogrammi persi");

            let fps = ((base.frames as f64 / base.seconds) / 2.0).round().max(1.0) as u32;
            let slow = Options { fps: Some(fps), exists: Exists::Replace, ..Default::default() };
            let out = perform_with(&src, "mp4", 80, None, &slow).unwrap_or_else(|e| panic!("{name} fps: {}", e.0));
            let info = mp4_info(&fs::read(&out.path).unwrap());
            assert!(info.frames < base.frames, "{name}: {} fotogrammi con il limite, {} senza", info.frames, base.frames);
            assert!(info.frames as f64 <= info.seconds * fps as f64 + 3.0, "{name}: {} fotogrammi in {} s a {fps} fps", info.frames, info.seconds);

            let mute = Options { mute: true, faststart: true, ..Default::default() };
            let out = perform_with(&src, "mp4", 80, None, &mute).unwrap_or_else(|e| panic!("{name} muto: {}", e.0));
            let info = mp4_info(&fs::read(&out.path).unwrap());
            assert!(!info.has_audio, "{name}: l'audio c'è ancora");
            assert!(info.moov_first, "{name}: il moov deve stare davanti");
            assert_eq!(info.frames, base.frames, "{name}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn video_quality_sets_the_bitrate() {
        let dir = scratch();
        let src = dir.join("rumore.avi");
        fs::write(&src, rgb_avi_with(640, 360, 90, true)).unwrap();
        let low = perform_with(&src, "mp4", 40, None, &Options::default()).unwrap();
        let high = perform_with(&src, "mp4", 95, None, &Options::default()).unwrap();
        let (low_len, high_len) = (fs::metadata(&low.path).unwrap().len(), fs::metadata(&high.path).unwrap().len());
        let (li, hi) = (mp4_info(&fs::read(&low.path).unwrap()), mp4_info(&fs::read(&high.path).unwrap()));
        assert!(high_len as f64 > low_len as f64 * 1.05, "qualità alta {high_len} B ({} fotogrammi, {} s), bassa {low_len} B ({} fotogrammi, {} s)", hi.frames, hi.seconds, li.frames, li.seconds);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_can_be_extracted_from_a_video() {
        let dir = scratch();
        let src = dir.join("audio.mp4");
        fs::write(&src, include_bytes!("../tests/fixtures/audio.mp4")).unwrap();
        let mp3 = perform_with(&src, "mp3", 80, None, &Options::default()).unwrap();
        let bytes = fs::read(&mp3.path).unwrap();
        assert!(bytes.len() > 1000 && (bytes.starts_with(b"ID3") || bytes[0] == 0xFF), "non è un MP3");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn doc_and_pdf_slices() {
        assert!(kind_of(Path::new("lettera.docx")) == Kind::Doc);
        assert_eq!(slices(Kind::Doc, Mode::Convert)[0].id, "pdf");
        assert_eq!(slices(Kind::Pdf, Mode::Convert)[0].id, "jpg");
        assert_eq!(slices(Kind::Pdf, Mode::Convert)[1].id, "docx");
        assert_eq!(slices(Kind::Pdf, Mode::Convert).len(), 4);
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
    rgb_avi_with(64, 64, 2, false)
}

/// Un AVI RGB non compresso: `noisy` riempie i fotogrammi di rumore, per provare la qualità dell'encoder.
#[cfg(test)]
fn rgb_avi_with(width: u32, height: u32, frames: u32, noisy: bool) -> Vec<u8> {
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
    let stride = width * 3;
    let frame_size = stride * height;
    let mut seed = 7u32;
    let mut moved = 0u32;
    let mut make_frame = || {
        moved += 1;
        let mut frame = vec![0u8; frame_size as usize];
        for y in 0..height {
            for x in 0..width {
                let at = ((height - 1 - y) * stride + x * 3) as usize;
                if noisy {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let n = (seed >> 30) as u8;
                    let base = ((((x + moved * 2) / 16 + y / 16) % 16) * 16) as u8;
                    frame[at] = base.wrapping_add(n);
                    frame[at + 1] = base.wrapping_mul(3).wrapping_add(n);
                    frame[at + 2] = (x * 255 / width) as u8 ^ n;
                } else {
                    frame[at] = 40;
                    frame[at + 1] = 180;
                    frame[at + 2] = 20;
                }
            }
        }
        frame
    };
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&u32(33_333));
    avih[4..8].copy_from_slice(&u32(frame_size * 30));
    avih[12..16].copy_from_slice(&u32(0x10));
    avih[16..20].copy_from_slice(&u32(frames));
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
    let mut movi_inner = b"movi".to_vec();
    let mut offsets = Vec::new();
    for _ in 0..frames {
        let db = chunk(b"00db", &make_frame());
        offsets.push(movi_inner.len() as u32);
        movi_inner.extend_from_slice(&db);
    }
    let mut idx = Vec::new();
    for offset in offsets {
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

/// Prove dei formati in più (foto, audio, ZIP, nome a modello): ogni conversione si rilegge e si confronta.
#[cfg(test)]
mod format_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("cedro-fmt-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_png(dir: &Path, name: &str, w: u32, h: u32, alpha: bool) -> PathBuf {
        let mut img = image::RgbaImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) * 255 / (w + h)) as u8, if alpha && x < w / 2 { 128 } else { 255 }]);
        }
        let path = dir.join(name);
        img.save(&path).unwrap();
        path
    }

    fn opts() -> Options {
        Options { exists: Exists::Replace, ..Default::default() }
    }

    #[test]
    fn photo_formats_roundtrip() {
        let dir = scratch();
        let src = sample_png(&dir, "gradiente.png", 64, 48, false);
        let original = image::open(&src).unwrap().to_rgb8();
        // Formati senza perdita: tornano uguali.
        for (action, ext) in [("tga", "tga"), ("qoi", "qoi"), ("pnm", "ppm"), ("tiff", "tiff"), ("bmp", "bmp"), ("jxl", "jxl")] {
            let out = perform_with(&src, action, 80, None, &opts()).unwrap_or_else(|e| panic!("{action}: {e}"));
            assert!(out.path.to_string_lossy().ends_with(ext), "{action}");
            let back = crate::imgfmt::open(&out.path, false, None).unwrap_or_else(|e| panic!("{action} riletto: {e}")).to_rgb8();
            assert_eq!(back.dimensions(), (64, 48), "{action}");
            assert_eq!(back.as_raw(), original.as_raw(), "{action} deve essere senza perdita");
        }
        // ICO e HDR/EXR: stesse misure e colori vicini.
        for (action, tolerance) in [("ico", 0i32), ("hdr", 12), ("exr", 2)] {
            let out = perform_with(&src, action, 80, None, &opts()).unwrap_or_else(|e| panic!("{action}: {e}"));
            let back = crate::imgfmt::open(&out.path, false, None).unwrap_or_else(|e| panic!("{action} riletto: {e}")).to_rgb8();
            assert_eq!(back.dimensions(), (64, 48), "{action}");
            let worst = back.as_raw().iter().zip(original.as_raw()).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(worst <= tolerance, "{action}: scarto massimo {worst}");
        }
    }

    #[test]
    fn avif_keeps_look_and_alpha() {
        let dir = scratch();
        let src = sample_png(&dir, "trasparente.png", 64, 48, true);
        let out = perform_with(&src, "avif", 90, None, &opts()).expect("avif");
        assert!(fs::read(&out.path).unwrap()[4..12].starts_with(b"ftypavif"));
        let back = crate::imgfmt::open(&out.path, false, None).expect("avif riletto").to_rgba8();
        assert_eq!(back.dimensions(), (64, 48));
        let original = image::open(&src).unwrap().to_rgba8();
        // La trasparenza resta: a sinistra mezza, a destra piena.
        assert!((back.get_pixel(5, 5)[3] as i32 - 128).abs() < 20, "alfa sinistra {}", back.get_pixel(5, 5)[3]);
        assert!(back.get_pixel(60, 5)[3] > 235);
        let diff: i64 = back.pixels().zip(original.pixels()).map(|(a, b)| (a[1] as i64 - b[1] as i64).abs()).sum();
        assert!(diff / (64 * 48) < 12, "AVIF troppo lontano dall'originale");
        // E da AVIF si torna a PNG con la ruota di sempre.
        let png = perform_with(&out.path, "png", 80, None, &opts()).expect("avif in png");
        assert!(image::open(&png.path).is_ok());
        assert_eq!(kind_of(&out.path), Kind::Image);
    }

    #[test]
    fn svg_becomes_image_and_pdf() {
        let dir = scratch();
        let svg = dir.join("logo.svg");
        fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="20" height="20" fill="#ff0000"/><circle cx="30" cy="10" r="8" fill="#0000ff"/></svg>"##).unwrap();
        assert_eq!(kind_of(&svg), Kind::Image);
        let out = perform_with(&svg, "png", 80, None, &opts()).expect("svg in png");
        let img = image::open(&out.path).unwrap().to_rgba8();
        // Un logo piccolo si disegna ad almeno 1024 px sul lato lungo, con il fondo trasparente.
        assert_eq!(img.width(), 1024);
        assert_eq!(img.height(), 512);
        assert_eq!(img.get_pixel(100, 250).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(800, 250).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(1000, 10)[3], 0);
        let small = perform_with(&svg, "png", 80, None, &Options { max_side: Some(200), ..opts() }).expect("svg 200");
        assert_eq!(image::open(&small.path).unwrap().width(), 200);
        let pdf = perform_with(&svg, "pdf", 80, None, &opts()).expect("svg in pdf");
        assert!(fs::read(&pdf.path).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn mislabelled_files_are_recognised_by_content() {
        let dir = scratch();
        let png = sample_png(&dir, "vera.png", 32, 32, false);
        let webp = perform_with(&png, "webp", 80, None, &opts()).unwrap().path;
        // Una WebP chiamata «.jpg» e un PNG senza estensione si leggono comunque.
        let fake = dir.join("finta.jpg");
        fs::copy(&webp, &fake).unwrap();
        assert_eq!(kind_of(&fake), Kind::Image);
        let out = perform_with(&fake, "png", 80, None, &opts()).expect("webp col nome sbagliato");
        assert_eq!(image::open(&out.path).unwrap().width(), 32);
        let bare = dir.join("senzaestensione");
        fs::copy(&png, &bare).unwrap();
        assert_eq!(kind_of(&bare), Kind::Image);
        // Un PDF chiamato «.jpg» è un PDF.
        let pdf = perform_with(&png, "pdf", 80, None, &opts()).unwrap().path;
        let pdf_fake = dir.join("scansione.jpg");
        fs::copy(&pdf, &pdf_fake).unwrap();
        assert_eq!(kind_of(&pdf_fake), Kind::Pdf);
        // Un DOCX (uno ZIP) resta un documento, non un archivio.
        let docx = dir.join("lettera.docx");
        fs::write(&docx, crate::zipstore::zip_archive(&[("word/document.xml", b"<w:document/>")])).unwrap();
        assert_eq!(kind_of(&docx), Kind::Doc);
        assert_eq!(kind_of(&dir.join("pacco.zip")), Kind::Archive);
    }

    fn sine_wav(dir: &Path, name: &str, rate: u32, channels: u16, secs: f32) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..(rate as f32 * secs) as u32 {
            let v = ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12000.0) as i16;
            for c in 0..channels {
                w.write_sample(if c == 0 { v } else { v / 2 }).unwrap();
            }
        }
        w.finalize().unwrap();
        path
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect()
    }

    fn rms(samples: &[i16]) -> f64 {
        (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
    }

    #[test]
    fn lossless_audio_roundtrips() {
        let dir = scratch();
        let src = sine_wav(&dir, "la.wav", 44_100, 2, 1.0);
        let original = crate::audio::decode(&src).unwrap();
        for action in ["flac", "aiff"] {
            let out = perform_with(&src, action, 80, None, &opts()).unwrap_or_else(|e| panic!("{action}: {e}"));
            assert_eq!(kind_of(&out.path), Kind::Audio, "{action}");
            let back = crate::audio::decode(&out.path).unwrap_or_else(|e| panic!("{action} riletto: {e}"));
            assert_eq!((back.rate, back.channels), (44_100, 2), "{action}");
            assert_eq!(back.samples, original.samples, "{action} deve essere senza perdita");
        }
        // Dal FLAC si torna a WAV identico.
        let flac = dir.join("la.flac");
        let wav = perform_with(&flac, "wav", 80, Some(&dir.join("indietro")), &opts()).unwrap();
        assert_eq!(crate::audio::decode(&wav.path).unwrap().samples, original.samples);
    }

    #[test]
    fn flac_carries_the_tags() {
        let dir = scratch();
        let src = dir.join("canzone.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 22_050, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..22_050 {
            w.write_sample(((i as f32 * 0.05).sin() * 8000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        // I tag di un file di partenza passano al FLAC: si parte da un FLAC con tag fatto a mano.
        let flac = perform_with(&src, "flac", 80, None, &opts()).unwrap().path;
        let mut bytes = fs::read(&flac).unwrap();
        assert_eq!(&bytes[0..4], b"fLaC");
        // Inserisce un blocco Vorbis comment come lo scrive Cedro, poi rilegge con symphonia.
        let mut comment = Vec::new();
        comment.extend_from_slice(&5u32.to_le_bytes());
        comment.extend_from_slice(b"Cedro");
        comment.extend_from_slice(&1u32.to_le_bytes());
        let c = b"TITLE=Prova";
        comment.extend_from_slice(&(c.len() as u32).to_le_bytes());
        comment.extend_from_slice(c);
        bytes[4] &= 0x7F;
        let mut block = vec![0x80 | 4];
        let len = comment.len() as u32;
        block.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        block.extend_from_slice(&comment);
        bytes.splice(42..42, block);
        let tagged = dir.join("con-tag.flac");
        fs::write(&tagged, bytes).unwrap();
        let pcm = crate::audio::decode(&tagged).unwrap();
        assert_eq!(pcm.samples.len(), 22_050, "il blocco dei tag non rovina il flusso");
        let back = perform_with(&tagged, "flac", 80, Some(&dir.join("giro")), &opts()).unwrap();
        let again = fs::read(&back.path).unwrap();
        let title = b"TITLE=Prova";
        assert!(again.windows(title.len()).any(|w| w == title), "il titolo passa da FLAC a FLAC");
    }

    #[test]
    fn opus_roundtrip_keeps_length_and_loudness() {
        let dir = scratch();
        let src = sine_wav(&dir, "voce.wav", 44_100, 2, 2.0);
        let original = crate::audio::decode(&src).unwrap();
        let out = perform_with(&src, "opus", 80, None, &opts()).expect("opus");
        assert_eq!(kind_of(&out.path), Kind::Audio);
        let head = fs::read(&out.path).unwrap();
        assert!(head.starts_with(b"OggS") && head.windows(8).any(|w| w == b"OpusHead"));
        let back = crate::audio::decode(&out.path).expect("opus riletto");
        assert_eq!((back.rate, back.channels), (48_000, 2));
        let secs = back.samples.len() as f32 / 2.0 / 48_000.0;
        assert!((secs - 2.0).abs() < 0.05, "durata {secs}");
        let (a, b) = (rms(&original.samples), rms(&back.samples));
        assert!((a - b).abs() / a < 0.15, "volume {a} → {b}");
        // E da .opus si converte in MP3 e WAV.
        let wav = perform_with(&out.path, "wav", 80, Some(&dir.join("w")), &opts()).expect("opus in wav");
        assert!(crate::audio::decode(&wav.path).unwrap().samples.len() > 90_000);
        let mp3 = perform_with(&out.path, "mp3", 80, Some(&dir.join("m")), &opts()).expect("opus in mp3");
        assert!(fs::metadata(&mp3.path).unwrap().len() > 1000);
    }

    #[test]
    fn zip_conversion_rebuilds_the_archive() {
        let dir = scratch();
        let png = sample_png(&dir, "a.png", 32, 24, false);
        let wav = sine_wav(&dir, "suono.wav", 22_050, 1, 0.5);
        let bad = dir.join("rotta.png");
        fs::write(&bad, b"non e una foto").unwrap();
        let note = dir.join("note.txt");
        fs::write(&note, "ciao").unwrap();
        let pngb = fs::read(&png).unwrap();
        let wavb = fs::read(&wav).unwrap();
        let zip = dir.join("pacco.zip");
        fs::write(&zip, crate::zipstore::zip_archive(&[("foto/a.png", &pngb), ("foto/rotta.png", b"non e una foto"), ("audio/suono.wav", &wavb), ("note.txt", b"ciao"), ("../fuori.png", &pngb)])).unwrap();
        assert_eq!(kind_of(&zip), Kind::Archive);
        let out = perform_with(&zip, "z-webp", 80, None, &opts()).expect("zip in webp");
        assert!(out.path.to_string_lossy().ends_with("pacco-webp.zip"), "{:?}", out.path);
        assert!(out.message.contains("1 file convertito"), "{}", out.message);
        let names = zip_names(&out.path);
        let mut archive = zip::ZipArchive::new(fs::File::open(&out.path).unwrap()).unwrap();
        assert!(names.contains(&"foto/a.webp".to_string()), "{names:?}");
        assert!(names.contains(&"foto/rotta.png".to_string()), "l'originale non convertibile resta: {names:?}");
        assert!(names.contains(&"audio/suono.wav".to_string()) && names.contains(&"note.txt".to_string()), "{names:?}");
        assert!(names.contains(&"RAPPORTO-CEDRO.txt".to_string()), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("fuori")), "percorso fuori cartella: {names:?}");
        let mut webp = Vec::new();
        std::io::Read::read_to_end(&mut archive.by_name("foto/a.webp").unwrap(), &mut webp).unwrap();
        assert!(webp.starts_with(b"RIFF"));
        // L'audio dello ZIP si converte in MP3.
        let mp3 = perform_with(&zip, "z-mp3", 80, Some(&dir.join("mp3")), &opts()).expect("zip in mp3");
        let names = zip_names(&mp3.path);
        assert!(names.contains(&"audio/suono.mp3".to_string()) && !names.contains(&"audio/suono.wav".to_string()), "{names:?}");
        // Niente da convertire: errore chiaro, non uno ZIP uguale.
        let err = perform_with(&zip, "z-pdf", 80, Some(&dir.join("pdf")), &opts()).err().expect("niente da convertire");
        assert!(err.0.contains("documenti"), "{}", err.0);
    }

    #[test]
    fn zip_conversion_keeps_both_when_names_collide() {
        let dir = scratch();
        let png = fs::read(sample_png(&dir, "a.png", 32, 24, false)).unwrap();
        let webp_src = perform_with(&dir.join("a.png"), "webp", 80, Some(&dir.join("w")), &opts()).unwrap().path;
        let webp = fs::read(webp_src).unwrap();
        for (label, entries) in [("png-prima", [("foto.png", png.as_slice()), ("foto.webp", webp.as_slice())]), ("webp-prima", [("foto.webp", webp.as_slice()), ("foto.png", png.as_slice())])] {
            let zip = dir.join(format!("{label}.zip"));
            fs::write(&zip, crate::zipstore::zip_archive(&entries)).unwrap();
            let out = perform_with(&zip, "z-webp", 80, Some(&dir.join(label)), &opts()).expect(label);
            let names = zip_names(&out.path);
            assert_eq!(names.len(), 2, "{label}: {names:?}");
            assert!(names.contains(&"foto.webp".to_string()), "{label}: {names:?}");
            // L'originale WebP resta intatto, byte per byte.
            let mut archive = zip::ZipArchive::new(fs::File::open(&out.path).unwrap()).unwrap();
            let mut kept = Vec::new();
            std::io::Read::read_to_end(&mut archive.by_name("foto.webp").unwrap(), &mut kept).unwrap();
            assert_eq!(kept, webp, "{label}");
        }
    }

    #[test]
    fn output_names_follow_the_template() {
        let dir = scratch();
        let src = sample_png(&dir, "gita.png", 16, 16, false);
        let out = perform_with(&src, "webp", 80, Some(&dir.join("o")), &Options { name_template: Some("%nome%_%data%".into()), ..opts() }).unwrap();
        let name = out.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("gita_20") && name.ends_with(".webp") && name.len() == "gita_2026-10-28.webp".len(), "{name}");
        // Le parti che aggiunge il motore restano in fondo.
        let small = perform_with(&src, "compress", 80, Some(&dir.join("o")), &Options { name_template: Some("foto %ora%".into()), ..opts() }).unwrap();
        let name = small.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("foto ") && name.ends_with("-compresso.jpg"), "{name}");
        // Un modello vuoto lascia il nome di sempre; un modello con simboli proibiti li pulisce.
        let plain = perform_with(&src, "bmp", 80, Some(&dir.join("o")), &Options { name_template: Some("  ".into()), ..opts() }).unwrap();
        assert_eq!(plain.path.file_name().unwrap(), "gita.bmp");
        let odd = perform_with(&src, "tga", 80, Some(&dir.join("o")), &Options { name_template: Some("a/b:%nome%".into()), ..opts() }).unwrap();
        assert_eq!(odd.path.file_name().unwrap(), "a_b_gita.tga");
    }
}
