//! Il pannello Conversioni: categorie, formati, opzioni e conversione di una coda di file.
//! Il modello è uguale su Windows e su Linux: i due pannelli disegnano le stesse opzioni e passano
//! i valori scelti al motore con `options_from`.

use std::path::{Path, PathBuf};

use crate::engine::{self, Encoding, Exists, Kind, Mode, PageSel, PageSize, Slice};
use crate::pdfpages::Plan;

/// Una voce del menu a sinistra del pannello.
pub struct Category {
    pub kind: Kind,
    pub name: &'static str,
    pub hint: &'static str,
    pub accepts: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Image, name: "Immagini", hint: "Cambia formato, ridimensiona e raddrizza le foto.", accepts: "PNG, JPEG, WebP, AVIF, JPEG XL, SVG, BMP, GIF, TIFF, ICO, TGA, QOI, PNM, HDR, OpenEXR, DDS" },
    Category { kind: Kind::Doc, name: "Documenti", hint: "Da Word, ODT e RTF a PDF.", accepts: "DOCX, DOC, ODT, RTF" },
    Category { kind: Kind::Pdf, name: "PDF", hint: "Da PDF a Word modificabile, a immagini o a testo.", accepts: "PDF" },
    Category { kind: Kind::Text, name: "Testo", hint: "TXT, Markdown, CSV, JSON e HTML.", accepts: "TXT, MD, CSV, JSON, HTML, XML" },
    Category { kind: Kind::Audio, name: "Audio", hint: "MP3, WAV, FLAC, Opus e AIFF.", accepts: "WAV, MP3, OGG, Opus, M4A, AAC, FLAC, AIFF, CAF" },
    Category { kind: Kind::Video, name: "Video", hint: "MP4, WebM, MOV, MKV e AVI.", accepts: "MP4, WebM, MOV, MKV, AVI" },
    Category { kind: Kind::Archive, name: "Archivi", hint: "Converte i file dentro uno ZIP e lo rifà.", accepts: "ZIP" },
];

/// I formati di uscita di una categoria.
pub fn formats(kind: Kind) -> &'static [Slice] {
    engine::panel_slices(kind)
}

/// Gli strumenti di una categoria (comprimi, ruota, OCR…). Vuoto se non ce ne sono.
pub fn tools(kind: Kind) -> &'static [Slice] {
    let list = engine::slices(kind, Mode::Tools);
    if list.iter().any(|s| s.id == "none") {
        &[]
    } else if kind == Kind::Pdf {
        // Unisci e Dividi hanno i loro pulsanti in cima al pannello, non stanno tra i formati.
        &list[..list.iter().position(|s| is_page_action(kind, s.id)).unwrap_or(list.len())]
    } else {
        list
    }
}

/// Le voci dei PDF che hanno il loro pulsante in cima al pannello invece di stare tra i formati:
/// unire, dividere, le pagine (ruotare, togliere, numeri, filigrana) e comprimere.
pub const PAGE_ACTIONS: [&str; 4] = ["merge", "split", "pages", "compress"];

/// Vero per le voci dei PDF con il loro pulsante («compress» delle immagini è un formato come gli altri).
pub fn is_page_action(kind: Kind, action: &str) -> bool {
    kind == Kind::Pdf && PAGE_ACTIONS.contains(&action)
}

/// Un formato proposto dalla modalità Semplice: nome e una riga in parole comuni.
pub struct SimpleFormat {
    pub id: &'static str,
    pub title: &'static str,
    pub note: &'static str,
}

/// I formati che la modalità Semplice mette in primo piano per una categoria (sempre formati del motore).
pub fn simple_formats(kind: Kind) -> &'static [SimpleFormat] {
    const IMAGE: &[SimpleFormat] = &[
        SimpleFormat { id: "webp", title: "WebP", note: "leggera, per il web" },
        SimpleFormat { id: "jpeg", title: "JPEG", note: "va bene dappertutto" },
        SimpleFormat { id: "png", title: "PNG", note: "nitida, senza perdite" },
        SimpleFormat { id: "pdf", title: "PDF", note: "un documento" },
    ];
    const DOC: &[SimpleFormat] = &[SimpleFormat { id: "pdf", title: "PDF", note: "si apre ovunque" }];
    const PDF: &[SimpleFormat] = &[
        SimpleFormat { id: "docx", title: "Word", note: "da modificare" },
        SimpleFormat { id: "jpg", title: "Immagini", note: "una per pagina" },
        SimpleFormat { id: "txt", title: "Testo", note: "solo le parole" },
        SimpleFormat { id: "merge", title: "Un solo PDF", note: "li unisce in fila" },
        SimpleFormat { id: "split", title: "Dividi", note: "una pagina per file" },
        SimpleFormat { id: "compress", title: "Più piccolo", note: "per l'email" },
    ];
    const TEXT: &[SimpleFormat] = &[
        SimpleFormat { id: "pdf", title: "PDF", note: "si apre ovunque" },
        SimpleFormat { id: "txt", title: "TXT", note: "testo semplice" },
        SimpleFormat { id: "html", title: "HTML", note: "pagina web" },
    ];
    const AUDIO: &[SimpleFormat] = &[
        SimpleFormat { id: "mp3", title: "MP3", note: "leggero, va ovunque" },
        SimpleFormat { id: "wav", title: "WAV", note: "qualità piena" },
    ];
    const VIDEO: &[SimpleFormat] = &[
        SimpleFormat { id: "mp4", title: "MP4", note: "va ovunque" },
        SimpleFormat { id: "mp3", title: "Solo audio", note: "senza immagini" },
    ];
    const ARCHIVE: &[SimpleFormat] = &[
        SimpleFormat { id: "z-webp", title: "Foto in WebP", note: "più leggere" },
        SimpleFormat { id: "z-jpeg", title: "Foto in JPEG", note: "vanno dappertutto" },
        SimpleFormat { id: "z-mp3", title: "Audio in MP3", note: "più leggero" },
        SimpleFormat { id: "z-pdf", title: "Documenti in PDF", note: "si aprono ovunque" },
    ];
    match kind {
        Kind::Image => IMAGE,
        Kind::Doc => DOC,
        Kind::Pdf => PDF,
        Kind::Text => TEXT,
        Kind::Audio => AUDIO,
        Kind::Video => VIDEO,
        Kind::Archive => ARCHIVE,
        Kind::Other => &[],
    }
}

/// La categoria dei file lasciati nel pannello Semplice: quella del primo file riconosciuto.
pub fn kind_of_files(paths: &[PathBuf]) -> Option<Kind> {
    paths.iter().map(|p| engine::kind_of(p)).find(|k| *k != Kind::Other)
}

/// Il nome che il pannello mostra per un formato o uno strumento, per esempio «WebP».
pub fn action_label(kind: Kind, action: &str) -> &'static str {
    formats(kind).iter().chain(engine::slices(kind, Mode::Tools).iter()).find(|s| s.id == action).map(|s| s.label).unwrap_or("")
}

/// I file di `paths` che appartengono alla categoria.
pub fn belonging(kind: Kind, paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().filter(|p| engine::kind_of(p) == kind).cloned().collect()
}

/// Una scelta di un'opzione (per esempio "150 dpi").
pub struct Choice {
    pub label: &'static str,
    pub value: u32,
}

/// Come si sceglie il valore di un'opzione.
pub enum Control {
    /// Acceso (1) o spento (0).
    Switch,
    /// Una voce di un elenco.
    Choice(&'static [Choice]),
    /// Un numero in un intervallo, con l'unità di misura.
    Range { min: u32, max: u32, unit: &'static str },
    /// Un testo libero (per esempio le pagine: `1-3,5`); `hint` dice cosa scrivere.
    Text { hint: &'static str },
    /// Un testo da non mostrare mentre si scrive.
    Password,
    /// Una voce tra poche, tutte in vista (pulsanti affiancati).
    Segments(&'static [Choice]),
    /// Un posto sulla pagina, da 0 a 8: la griglia 3 × 3 (in alto a sinistra … in basso a destra).
    Position,
}

/// Tutte le opzioni che il motore capisce.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Opt {
    Straighten,
    WhiteBackground,
    MaxSide,
    PageSize,
    Password,
    Dpi,
    PageRange,
    OnePerPage,
    Ocr,
    Tables,
    Columns,
    EmbedFonts,
    Bookmarks,
    PdfA,
    Encoding,
    CsvSeparator,
    PrettyJson,
    HeaderRow,
    Mp3Kbps,
    Channels,
    Normalize,
    TrimStart,
    TrimSilence,
    KeepTags,
    VideoHeight,
    Fps,
    Mute,
    FastStart,
    NameTemplate,
    MergeName,
    FileBookmarks,
    KeepBookmarks,
    KeepForms,
    DuplexBlank,
    OpenPassword,
    SplitHow,
    SplitEvery,
    SplitRanges,
    SplitRemove,
    SplitName,
    PageNumbers,
    NumberPos,
    NumberFormat,
    NumberStart,
    NumberSize,
    Watermark,
    WatermarkText,
    WatermarkOpacity,
    WatermarkAngle,
    WatermarkPages,
    SqueezeLevel,
    StripMeta,
    BwScans,
}

/// Una riga della zona Opzioni del pannello.
pub struct OptionDef {
    pub opt: Opt,
    pub title: &'static str,
    pub note: &'static str,
    pub control: Control,
    /// Il valore di partenza (per `Password` non conta).
    pub default: u32,
}

const fn sw(opt: Opt, title: &'static str, note: &'static str, on: bool) -> OptionDef {
    OptionDef { opt, title, note, control: Control::Switch, default: on as u32 }
}

const fn pick(opt: Opt, title: &'static str, note: &'static str, choices: &'static [Choice], default: u32) -> OptionDef {
    OptionDef { opt, title, note, control: Control::Choice(choices), default }
}

const MAX_SIDE: &[Choice] = &[
    Choice { label: "Originali", value: 0 },
    Choice { label: "3840 px", value: 3840 },
    Choice { label: "1920 px", value: 1920 },
    Choice { label: "1280 px", value: 1280 },
    Choice { label: "800 px", value: 800 },
];
const PAGE_SIZE: &[Choice] = &[
    Choice { label: "A4", value: 0 },
    Choice { label: "A3", value: 1 },
    Choice { label: "Lettera", value: 2 },
];
const DPI: &[Choice] = &[
    Choice { label: "72 dpi", value: 72 },
    Choice { label: "144 dpi", value: 144 },
    Choice { label: "200 dpi", value: 200 },
    Choice { label: "300 dpi", value: 300 },
];
const SILENCE: &[Choice] = &[
    Choice { label: "No", value: 0 },
    Choice { label: "Sotto -30 dB", value: 30 },
    Choice { label: "Sotto -40 dB", value: 40 },
    Choice { label: "Sotto -50 dB", value: 50 },
];
const ENCODING: &[Choice] = &[
    Choice { label: "UTF-8", value: 0 },
    Choice { label: "Windows-1252", value: 1 },
    Choice { label: "ISO-8859-1", value: 2 },
];
const SEPARATOR: &[Choice] = &[
    Choice { label: "Come l'originale", value: 0 },
    Choice { label: "Punto e virgola ;", value: 1 },
    Choice { label: "Virgola ,", value: 2 },
    Choice { label: "Tabulazione", value: 3 },
];
const KBPS: &[Choice] = &[
    Choice { label: "96 kbps", value: 96 },
    Choice { label: "128 kbps", value: 128 },
    Choice { label: "160 kbps", value: 160 },
    Choice { label: "192 kbps", value: 192 },
    Choice { label: "256 kbps", value: 256 },
    Choice { label: "320 kbps", value: 320 },
];
const CHANNELS: &[Choice] = &[
    Choice { label: "Come l'originale", value: 0 },
    Choice { label: "Stereo", value: 2 },
    Choice { label: "Mono", value: 1 },
];
const VIDEO_HEIGHT: &[Choice] = &[
    Choice { label: "Come l'originale", value: 0 },
    Choice { label: "1080p", value: 1080 },
    Choice { label: "720p", value: 720 },
    Choice { label: "480p", value: 480 },
];
const FPS: &[Choice] = &[
    Choice { label: "Come l'originale", value: 0 },
    Choice { label: "30", value: 30 },
    Choice { label: "25", value: 25 },
    Choice { label: "24", value: 24 },
];

const STRAIGHTEN: OptionDef = sw(Opt::Straighten, "Raddrizza le foto", "Corregge la rotazione dei file del telefono", true);
const WHITE_BG: OptionDef = sw(Opt::WhiteBackground, "Sfondo bianco", "Al posto della trasparenza, nel JPEG", true);
const IMAGE_SIZE: OptionDef = pick(Opt::MaxSide, "Dimensioni", "Lato lungo massimo; le più piccole restano come sono", MAX_SIDE, 0);
const PAGE: OptionDef = pick(Opt::PageSize, "Formato pagina", "", PAGE_SIZE, 0);
const PASSWORD: OptionDef = OptionDef { opt: Opt::Password, title: "Password del PDF", note: "Vuota = nessuna protezione", control: Control::Password, default: 0 };
const PDF_DPI: OptionDef = pick(Opt::Dpi, "Risoluzione immagini", "Più dpi, più nitide e più pesanti", DPI, 144);
const PDF_PAGES: OptionDef = OptionDef {
    opt: Opt::PageRange,
    title: "Pagine",
    note: "Vuoto = tutte. Per esempio 1-3,5 oppure 4-",
    control: Control::Text { hint: "tutte, oppure 1-3,5" },
    default: 0,
};
const ONE_PER_PAGE: OptionDef = sw(Opt::OnePerPage, "Una immagine per pagina", "Spenta: una sola immagine lunga", true);
const OCR: OptionDef = sw(Opt::Ocr, "Leggi le scansioni (OCR)", "Solo dove manca il testo", true);
const TABLES: OptionDef = sw(Opt::Tables, "Ricostruisci le tabelle", "Celle, sfondi e bordi", true);
const COLUMNS: OptionDef = sw(Opt::Columns, "Tieni le colonne", "Il testo a colonne resta a colonne; spento va in colonna singola", true);
const EMBED_FONTS: OptionDef = sw(Opt::EmbedFonts, "Incorpora i caratteri", "Il PDF si vede uguale ovunque e il testo si copia", true);
const BOOKMARKS: OptionDef = sw(Opt::Bookmarks, "Conserva i segnalibri", "Dai titoli del documento", true);
const PDF_A: OptionDef = sw(Opt::PdfA, "PDF/A per l'archivio", "PDF/A-1b: font incorporati, profilo colore, metadati", false);
const TEXT_ENCODING: OptionDef = pick(Opt::Encoding, "Codifica", "Del file scritto", ENCODING, 0);
const CSV_SEPARATOR: OptionDef = pick(Opt::CsvSeparator, "Separatore CSV", "", SEPARATOR, 0);
const PRETTY_JSON: OptionDef = sw(Opt::PrettyJson, "Rendi leggibile il JSON", "Rientri e a capo", true);
const HEADER_ROW: OptionDef = sw(Opt::HeaderRow, "Prima riga come titoli", "Per le tabelle e il JSON dal CSV", true);
const MP3_KBPS: OptionDef = pick(Opt::Mp3Kbps, "Qualità MP3", "", KBPS, 128);
const AUDIO_CHANNELS: OptionDef = pick(Opt::Channels, "Canali", "", CHANNELS, 0);
const NORMALIZE: OptionDef = sw(Opt::Normalize, "Normalizza il volume", "Porta il picco al massimo", false);
const TRIM_START: OptionDef = OptionDef { opt: Opt::TrimStart, title: "Taglia l'inizio", note: "Secondi saltati all'inizio", control: Control::Range { min: 0, max: 30, unit: " s" }, default: 0 };
const TRIM_SILENCE: OptionDef = pick(Opt::TrimSilence, "Taglia il silenzio iniziale", "Il resto resta com'è", SILENCE, 0);
const KEEP_TAGS: OptionDef = sw(Opt::KeepTags, "Mantieni titolo e artista", "Tag del file", true);
const HEIGHT: OptionDef = pick(Opt::VideoHeight, "Risoluzione", "Si rimpicciolisce, non si ingrandisce", VIDEO_HEIGHT, 0);
const VIDEO_FPS: OptionDef = pick(Opt::Fps, "Fotogrammi al secondo", "Massimo", FPS, 0);
const MUTE: OptionDef = sw(Opt::Mute, "Togli l'audio", "", false);
const OPUS_KBPS_CHOICES: &[Choice] = &[
    Choice { label: "64 kbps", value: 64 },
    Choice { label: "96 kbps", value: 96 },
    Choice { label: "128 kbps", value: 128 },
    Choice { label: "160 kbps", value: 160 },
    Choice { label: "192 kbps", value: 192 },
];
const OPUS_KBPS: OptionDef = pick(Opt::Mp3Kbps, "Qualità Opus", "", OPUS_KBPS_CHOICES, 128);
const NAME_TEMPLATE: OptionDef = OptionDef {
    opt: Opt::NameTemplate,
    title: "Nome del file",
    note: "Vuoto = come l'originale. Usa %nome%, %data%, %ora%, %ext%",
    control: Control::Text { hint: "per esempio %nome%_%data%" },
    default: 0,
};
/// Come dividere: i valori sono quelli di `options_from`.
pub const SPLIT_HOW_CHOICES: &[Choice] = &[
    Choice { label: "Ogni pagina", value: 0 },
    Choice { label: "Ogni N", value: 1 },
    Choice { label: "Intervalli", value: 2 },
    Choice { label: "Ai segnalibri", value: 3 },
    Choice { label: "Togli pagine", value: 4 },
];
pub const SPLIT_HOW: OptionDef = OptionDef { opt: Opt::SplitHow, title: "Come dividere", note: "", control: Control::Segments(SPLIT_HOW_CHOICES), default: 2 };
pub const SPLIT_RANGES: OptionDef = OptionDef {
    opt: Opt::SplitRanges,
    title: "Intervalli",
    note: "Un file per ogni gruppo, per esempio 1-3, 4-6, 7-. Segue i tagli della striscia.",
    control: Control::Text { hint: "1-3, 4-6, 7-" },
    default: 0,
};
const SPLIT_EVERY: OptionDef = OptionDef { opt: Opt::SplitEvery, title: "Pagine per file", note: "L'ultimo file prende quelle che restano", control: Control::Range { min: 1, max: 50, unit: "" }, default: 2 };
const SPLIT_REMOVE: OptionDef = OptionDef {
    opt: Opt::SplitRemove,
    title: "Pagine da togliere",
    note: "Esce un PDF solo, senza queste pagine. Per esempio 2, 5-6",
    control: Control::Text { hint: "2, 5-6" },
    default: 0,
};
const SPLIT_NAME: OptionDef = OptionDef {
    opt: Opt::SplitName,
    title: "Nome dei file",
    note: "Vuoto = nome-parte-1, nome-parte-2… Usa %nome%, %n%, %titolo%",
    control: Control::Text { hint: "%nome%-parte-%n%" },
    default: 0,
};
const MERGE_NAME: OptionDef = OptionDef {
    opt: Opt::MergeName,
    title: "Nome del file unito",
    note: "Vuoto = il nome del primo file con «-unito»",
    control: Control::Text { hint: "nome del primo file-unito" },
    default: 0,
};
const FILE_BOOKMARKS: OptionDef = sw(Opt::FileBookmarks, "Un segnalibro per ogni file", "Nel PDF unito si salta da un file all'altro", true);
const KEEP_BOOKMARKS: OptionDef = sw(Opt::KeepBookmarks, "Tieni i segnalibri dei file", "Finiscono sotto il segnalibro del loro file", true);
const KEEP_FORMS: OptionDef = sw(Opt::KeepForms, "Tieni moduli e link", "Campi compilabili e collegamenti interni", true);
const DUPLEX_BLANK: OptionDef = sw(Opt::DuplexBlank, "Pagina bianca per il fronte-retro", "Dopo i file con le pagine dispari: ognuno parte su un foglio nuovo", false);
const OPEN_PASSWORD: OptionDef = OptionDef { opt: Opt::OpenPassword, title: "Password per aprire", note: "Solo per i PDF protetti", control: Control::Password, default: 0 };
const OUT_PASSWORD: OptionDef = OptionDef { opt: Opt::Password, title: "Proteggi con password", note: "Vuota = nessuna protezione", control: Control::Password, default: 0 };

pub const PAGE_NUMBERS: OptionDef = sw(Opt::PageNumbers, "Numeri di pagina", "Es. «Pagina 1 di 8», sopra ogni pagina", false);
pub const NUMBER_POS: OptionDef = OptionDef { opt: Opt::NumberPos, title: "Posizione", note: "", control: Control::Position, default: 7 };
pub const NUMBER_FORMAT_CHOICES: &[Choice] = &[Choice { label: "1", value: 0 }, Choice { label: "Pagina 1 di N", value: 1 }, Choice { label: "1 / N", value: 2 }];
pub const NUMBER_FORMAT: OptionDef = OptionDef { opt: Opt::NumberFormat, title: "Formato", note: "", control: Control::Segments(NUMBER_FORMAT_CHOICES), default: 1 };
pub const NUMBER_START: OptionDef = OptionDef { opt: Opt::NumberStart, title: "Primo numero", note: "Vuoto = 1", control: Control::Text { hint: "1" }, default: 0 };
pub const NUMBER_SIZE: OptionDef = OptionDef { opt: Opt::NumberSize, title: "Corpo", note: "", control: Control::Range { min: 7, max: 24, unit: "pt" }, default: 11 };
pub const WATERMARK: OptionDef = sw(Opt::Watermark, "Filigrana", "Testo grande e chiaro sopra le pagine", false);
pub const WATERMARK_TEXT: OptionDef = OptionDef { opt: Opt::WatermarkText, title: "Testo della filigrana", note: "", control: Control::Text { hint: "BOZZA" }, default: 0 };
pub const WATERMARK_OPACITY: OptionDef = OptionDef { opt: Opt::WatermarkOpacity, title: "Opacità", note: "", control: Control::Range { min: 5, max: 80, unit: "%" }, default: 22 };
pub const WATERMARK_ANGLE_CHOICES: &[Choice] = &[Choice { label: "Diagonale", value: 0 }, Choice { label: "Orizzontale", value: 1 }];
pub const WATERMARK_ANGLE: OptionDef = OptionDef { opt: Opt::WatermarkAngle, title: "Direzione", note: "", control: Control::Segments(WATERMARK_ANGLE_CHOICES), default: 0 };
pub const WATERMARK_PAGES: OptionDef = OptionDef {
    opt: Opt::WatermarkPages,
    title: "Pagine con la filigrana",
    note: "Vuoto = tutte. Per esempio 1-3,5",
    control: Control::Text { hint: "tutte" },
    default: 0,
};
pub const SQUEEZE_CHOICES: &[Choice] = &[Choice { label: "Leggera", value: 0 }, Choice { label: "Equilibrata", value: 1 }, Choice { label: "Forte", value: 2 }];
pub const SQUEEZE_LEVEL: OptionDef = OptionDef {
    opt: Opt::SqueezeLevel,
    title: "Livello",
    note: "Leggera: ripulisce senza toccare le immagini. Equilibrata: immagini a 150 dpi, giusta per l'email. Forte: 100 dpi, il più piccolo.",
    control: Control::Segments(SQUEEZE_CHOICES),
    default: 1,
};
const STRIP_META: OptionDef = sw(Opt::StripMeta, "Togli metadati", "Autore, programma, data di modifica", true);
const BW_SCANS: OptionDef = sw(Opt::BwScans, "Scansioni in bianco e nero", "Restano nitide e pesano molto meno", true);

const FAST_START: OptionDef = sw(Opt::FastStart, "Ottimizza per il web", "L'avvio parte subito", false);

/// Le opzioni che valgono per questa categoria e questa voce; il motore le usa tutte.
pub fn option_defs(kind: Kind, action: &str) -> Vec<&'static OptionDef> {
    let mut defs = format_option_defs(kind, action);
    if action != "none" && !is_page_action(kind, action) {
        defs.push(&NAME_TEMPLATE);
    }
    defs
}

fn format_option_defs(kind: Kind, action: &str) -> Vec<&'static OptionDef> {
    match (kind, action) {
        (Kind::Image, "ocr") => vec![],
        (Kind::Image, "jpeg" | "compress" | "pdf" | "pnm") => vec![&IMAGE_SIZE, &STRAIGHTEN, &WHITE_BG],
        (Kind::Image, _) => vec![&IMAGE_SIZE, &STRAIGHTEN],
        (Kind::Doc, "pdf") => vec![&PAGE, &EMBED_FONTS, &BOOKMARKS, &PDF_A, &PASSWORD],
        (Kind::Pdf, "jpg" | "png") => vec![&PDF_DPI, &PDF_PAGES, &ONE_PER_PAGE],
        (Kind::Pdf, "docx") => vec![&OCR, &TABLES, &COLUMNS, &PDF_PAGES],
        (Kind::Pdf, "txt") => vec![&OCR, &PDF_PAGES],
        (Kind::Pdf, "merge") => vec![&MERGE_NAME, &FILE_BOOKMARKS, &KEEP_BOOKMARKS, &KEEP_FORMS, &DUPLEX_BLANK, &OUT_PASSWORD, &OPEN_PASSWORD],
        (Kind::Pdf, "pages") => vec![
            &PAGE_NUMBERS, &NUMBER_POS, &NUMBER_FORMAT, &NUMBER_START, &NUMBER_SIZE,
            &WATERMARK, &WATERMARK_TEXT, &WATERMARK_OPACITY, &WATERMARK_ANGLE, &WATERMARK_PAGES,
            &KEEP_BOOKMARKS, &KEEP_FORMS, &OUT_PASSWORD, &OPEN_PASSWORD,
        ],
        (Kind::Pdf, "compress") => vec![&SQUEEZE_LEVEL, &STRIP_META, &BW_SCANS, &OUT_PASSWORD, &OPEN_PASSWORD],
        (Kind::Pdf, "split") => vec![&SPLIT_HOW, &SPLIT_EVERY, &SPLIT_RANGES, &SPLIT_REMOVE, &SPLIT_NAME, &KEEP_BOOKMARKS, &KEEP_FORMS, &OUT_PASSWORD, &OPEN_PASSWORD],
        (Kind::Text, "pdf") => vec![&PAGE, &EMBED_FONTS, &PDF_A],
        (Kind::Text, "csv") => vec![&TEXT_ENCODING, &CSV_SEPARATOR],
        (Kind::Text, "json") => vec![&TEXT_ENCODING, &PRETTY_JSON, &HEADER_ROW],
        (Kind::Text, "md" | "html") => vec![&TEXT_ENCODING, &HEADER_ROW],
        (Kind::Text, _) => vec![&TEXT_ENCODING],
        (Kind::Audio | Kind::Video, "mp3") => vec![&MP3_KBPS, &AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE, &KEEP_TAGS],
        (Kind::Audio, "wav" | "aiff") => vec![&AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE],
        (Kind::Audio, "flac") => vec![&AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE, &KEEP_TAGS],
        (Kind::Audio, "opus") => vec![&OPUS_KBPS, &AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE, &KEEP_TAGS],
        (Kind::Archive, _) => vec![&IMAGE_SIZE],
        (Kind::Video, "mp4") => vec![&HEIGHT, &VIDEO_FPS, &MUTE, &FAST_START],
        _ => vec![],
    }
}

/// Vero se l'opzione si mostra con i valori di ora: i campi di «Dividi» seguono il modo scelto.
pub fn shown(def: &OptionDef, values: &Values) -> bool {
    let how = values.num(Opt::SplitHow, SPLIT_HOW.default);
    match def.opt {
        Opt::SplitEvery => how == 1,
        Opt::SplitRanges => how == 2,
        Opt::SplitRemove => how == 4,
        // Un file solo esce già con il nome dell'originale e «-senza-pagine».
        Opt::SplitName => how != 4,
        // Le scelte dei numeri e della filigrana si vedono quando sono accesi.
        Opt::NumberPos | Opt::NumberFormat | Opt::NumberStart | Opt::NumberSize => values.get(&PAGE_NUMBERS) == 1,
        Opt::WatermarkText | Opt::WatermarkOpacity | Opt::WatermarkAngle | Opt::WatermarkPages => values.get(&WATERMARK) == 1,
        _ => true,
    }
}

/// Come rispondere se il nome di uscita esiste già, nell'ordine in cui si mostrano.
pub const EXISTS_CHOICES: &[(&str, Exists)] = &[
    ("Aggiungi -2, -3…", Exists::Number),
    ("Sostituisci il file", Exists::Replace),
    ("Salta il file", Exists::Skip),
    ("Chiedi ogni volta", Exists::Ask),
];

/// I valori scelti nel pannello.
#[derive(Clone, Debug)]
pub struct Values {
    nums: Vec<(Opt, u32)>,
    texts: Vec<(Opt, String)>,
    /// Se il nome esiste già.
    pub exists: Exists,
}

impl Default for Values {
    fn default() -> Self {
        Values::new()
    }
}

impl Values {
    pub const fn new() -> Self {
        Values { nums: Vec::new(), texts: Vec::new(), exists: Exists::Number }
    }

    pub fn get(&self, def: &OptionDef) -> u32 {
        self.nums.iter().find(|(o, _)| *o == def.opt).map(|(_, v)| *v).unwrap_or(def.default)
    }

    pub fn set(&mut self, def: &OptionDef, value: u32) {
        match self.nums.iter_mut().find(|(o, _)| *o == def.opt) {
            Some(slot) => slot.1 = value,
            None => self.nums.push((def.opt, value)),
        }
    }

    /// Il testo di un'opzione `Text` o `Password` (vuoto se non scritto).
    pub fn text(&self, def: &OptionDef) -> &str {
        self.text_of(def.opt)
    }

    fn text_of(&self, opt: Opt) -> &str {
        self.texts.iter().find(|(o, _)| *o == opt).map(|(_, t)| t.as_str()).unwrap_or("")
    }

    pub fn set_text(&mut self, def: &OptionDef, text: &str) {
        match self.texts.iter_mut().find(|(o, _)| *o == def.opt) {
            Some(slot) => slot.1 = text.to_string(),
            None => self.texts.push((def.opt, text.to_string())),
        }
    }

    fn num(&self, opt: Opt, default: u32) -> u32 {
        self.nums.iter().find(|(o, _)| *o == opt).map(|(_, v)| *v).unwrap_or(default)
    }
}

/// Se i valori scritti a mano sono usabili per la voce scelta: dice cosa non va (per esempio le pagine).
/// Un campo di un'altra voce non conta: lì non si vede e non si può correggere.
pub fn validate(values: &Values, kind: Kind, action: &str) -> Result<(), String> {
    if option_defs(kind, action).iter().any(|d| d.opt == Opt::PageRange) {
        PageSel::parse(values.text_of(Opt::PageRange))?;
    }
    if kind == Kind::Pdf && action == "pages" {
        if values.get(&PAGE_NUMBERS) == 1 {
            let start = values.text_of(Opt::NumberStart).trim();
            if !start.is_empty() && start.parse::<u32>().ok().filter(|n| *n >= 1).is_none() {
                return Err("Il primo numero deve essere un numero da 1 in su.".into());
            }
        }
        if values.get(&WATERMARK) == 1 {
            PageSel::parse(values.text_of(Opt::WatermarkPages))?;
        }
    }
    if kind == Kind::Pdf && action == "split" {
        let how = values.num(Opt::SplitHow, SPLIT_HOW.default);
        // Si controlla la scrittura; le pagine che mancano le dice il motore, file per file.
        let check = |text: &str, empty: &str| -> Result<(), String> {
            if text.trim().is_empty() {
                return Err(empty.into());
            }
            for it in text.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()) {
                PageSel::parse(it)?;
            }
            Ok(())
        };
        match how {
            2 => check(values.text_of(Opt::SplitRanges), "Scrivi gli intervalli, per esempio 1-3, 4-6, 7-, oppure taglia sulla striscia.")?,
            4 => check(values.text_of(Opt::SplitRemove), "Scrivi le pagine da togliere, per esempio 2, 5-6.")?,
            _ => {}
        }
    }
    Ok(())
}

/// Le opzioni del motore per i valori scelti; le opzioni non scelte restano com'erano.
pub fn options_from(values: &Values) -> engine::Options {
    let mut o = engine::Options::default();
    let on = |opt: Opt, default: bool| values.num(opt, default as u32) != 0;
    o.exists = values.exists;
    o.straighten = on(Opt::Straighten, true);
    o.white_background = on(Opt::WhiteBackground, true);
    o.max_side = Some(values.num(Opt::MaxSide, 0)).filter(|v| *v > 0);
    o.page_size = match values.num(Opt::PageSize, 0) {
        1 => PageSize::A3,
        2 => PageSize::Letter,
        _ => PageSize::A4,
    };
    o.password = Some(values.text_of(Opt::Password).to_string()).filter(|p| !p.is_empty());
    o.dpi = Some(values.num(Opt::Dpi, 144));
    o.pages = PageSel::parse(values.text_of(Opt::PageRange)).ok().flatten();
    o.columns = on(Opt::Columns, true);
    o.embed_fonts = on(Opt::EmbedFonts, true);
    o.bookmarks = on(Opt::Bookmarks, true);
    o.pdfa = on(Opt::PdfA, false);
    o.trim_silence_db = Some(values.num(Opt::TrimSilence, 0)).filter(|v| *v > 0);
    o.one_image_per_page = on(Opt::OnePerPage, true);
    o.ocr = on(Opt::Ocr, true);
    o.tables = on(Opt::Tables, true);
    o.encoding = match values.num(Opt::Encoding, 0) {
        1 => Encoding::Windows1252,
        2 => Encoding::Latin1,
        _ => Encoding::Utf8,
    };
    o.csv_separator = match values.num(Opt::CsvSeparator, 0) {
        1 => Some(';'),
        2 => Some(','),
        3 => Some('\t'),
        _ => None,
    };
    o.pretty_json = on(Opt::PrettyJson, true);
    o.header_row = on(Opt::HeaderRow, true);
    o.mp3_kbps = Some(values.num(Opt::Mp3Kbps, 128));
    o.channels = Some(values.num(Opt::Channels, 0) as u8).filter(|v| *v > 0);
    o.normalize = on(Opt::Normalize, false);
    o.trim_start_secs = values.num(Opt::TrimStart, 0);
    o.keep_tags = on(Opt::KeepTags, true);
    o.video_height = Some(values.num(Opt::VideoHeight, 0)).filter(|v| *v > 0);
    o.fps = Some(values.num(Opt::Fps, 0)).filter(|v| *v > 0);
    o.mute = on(Opt::Mute, false);
    o.faststart = on(Opt::FastStart, false);
    o.name_template = Some(values.text_of(Opt::NameTemplate).trim().to_string()).filter(|t| !t.is_empty());
    let text = |opt: Opt| Some(values.text_of(opt).trim().to_string()).filter(|t| !t.is_empty());
    let pp = &mut o.pdf_pages;
    pp.settings.file_bookmarks = on(Opt::FileBookmarks, true);
    pp.settings.keep_bookmarks = on(Opt::KeepBookmarks, true);
    pp.settings.keep_forms = on(Opt::KeepForms, true);
    pp.settings.duplex_blank = on(Opt::DuplexBlank, false);
    pp.open_password = Some(values.text_of(Opt::OpenPassword).to_string()).filter(|p| !p.is_empty());
    pp.merge_name = text(Opt::MergeName);
    pp.split_name = text(Opt::SplitName);
    pp.plan = match values.num(Opt::SplitHow, SPLIT_HOW.default) {
        0 => Plan::EveryPage,
        1 => Plan::Every(values.num(Opt::SplitEvery, 2).max(1)),
        3 => Plan::Bookmarks,
        4 => Plan::Remove(values.text_of(Opt::SplitRemove).to_string()),
        _ => Plan::Ranges(values.text_of(Opt::SplitRanges).to_string()),
    };
    let pe = &mut o.pdf_edit;
    pe.stamp.numbers = on(Opt::PageNumbers, false).then(|| crate::pdfedit::Numbers {
        pos: values.num(Opt::NumberPos, NUMBER_POS.default).min(8) as u8,
        format: values.num(Opt::NumberFormat, NUMBER_FORMAT.default).min(2) as u8,
        start: values.text_of(Opt::NumberStart).trim().parse::<u32>().ok().filter(|n| *n >= 1).unwrap_or(1),
        size: values.num(Opt::NumberSize, NUMBER_SIZE.default),
    });
    pe.stamp.watermark = on(Opt::Watermark, false).then(|| crate::pdfedit::Watermark {
        // Vuoto vale quello che il campo mostra in grigio.
        text: Some(values.text_of(Opt::WatermarkText).trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| crate::i18n::tr("BOZZA").into_owned()),
        opacity: values.num(Opt::WatermarkOpacity, WATERMARK_OPACITY.default),
        diagonal: values.num(Opt::WatermarkAngle, 0) == 0,
        pages: values.text_of(Opt::WatermarkPages).trim().to_string(),
    });
    pe.squeeze = Some(crate::pdfedit::Squeeze {
        level: values.num(Opt::SqueezeLevel, SQUEEZE_LEVEL.default).min(2) as u8,
        strip_meta: on(Opt::StripMeta, true),
        bw_scans: on(Opt::BwScans, true),
    });
    o
}

/// Esegue `action` su un file. Con `dest` le copie nuove finiscono in quella cartella
/// invece che accanto all'originale; il nome non ne sovrascrive mai un altro.
pub fn convert_one(path: &Path, action: &str, quality: u8, dest: Option<&Path>, opts: &engine::Options) -> Result<String, String> {
    convert_one_out(path, action, quality, dest, opts).map(|(_, message)| message)
}

/// Come `convert_one`, ma dice anche quale file ha creato.
pub fn convert_one_out(path: &Path, action: &str, quality: u8, dest: Option<&Path>, opts: &engine::Options) -> Result<(PathBuf, String), String> {
    // «Accanto all'originale» nel pannello vuol dire proprio accanto, qualunque cartella usi la ruota.
    let beside = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    engine::perform_with(path, action, quality, Some(dest.unwrap_or(beside)), opts).map(|out| (out.path, out.message)).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn every_def() -> Vec<&'static OptionDef> {
        let mut all: Vec<&'static OptionDef> = Vec::new();
        for c in CATEGORIES {
            let ids = formats(c.kind).iter().chain(tools(c.kind).iter()).map(|s| s.id).chain(PAGE_ACTIONS.into_iter().filter(|_| c.kind == Kind::Pdf));
            for id in ids {
                for d in option_defs(c.kind, id) {
                    if !all.iter().any(|x| x.opt == d.opt) {
                        all.push(d);
                    }
                }
            }
        }
        all
    }

    #[test]
    fn every_category_has_formats() {
        for c in CATEGORIES {
            assert!(!formats(c.kind).is_empty(), "{}", c.name);
        }
    }

    #[test]
    fn belonging_filters_by_kind() {
        let files = vec![PathBuf::from("a.png"), PathBuf::from("b.pdf"), PathBuf::from("c.jpg")];
        assert_eq!(belonging(Kind::Image, &files).len(), 2);
        assert_eq!(belonging(Kind::Pdf, &files), vec![PathBuf::from("b.pdf")]);
    }

    #[test]
    fn tools_hide_the_placeholder() {
        assert!(tools(Kind::Video).is_empty());
        assert!(!tools(Kind::Image).is_empty());
    }

    #[test]
    fn defaults_leave_the_engine_as_it_was() {
        let opts = options_from(&Values::default());
        // I valori di partenza del pannello coincidono con quelli di sempre del motore.
        // Solo «Dividi» parte diverso: il pannello propone gli intervalli, la ruota una pagina per file.
        let pdf_pages = engine::PdfPages { plan: Plan::Ranges(String::new()), ..Default::default() };
        // La compressione dei PDF ha sempre le sue scelte (la ruota invece usa quelle delle impostazioni).
        let pdf_edit = engine::PdfEdit { squeeze: Some(crate::pdfedit::Squeeze::default()), ..Default::default() };
        let expected = engine::Options { dpi: Some(144), mp3_kbps: Some(128), pdf_pages, pdf_edit, ..Default::default() };
        assert_eq!(opts, expected);
    }

    #[test]
    fn every_option_has_a_working_default_and_reaches_the_engine() {
        let defs = every_def();
        let wanted = [
            Opt::Straighten, Opt::WhiteBackground, Opt::MaxSide, Opt::PageSize, Opt::Password, Opt::Dpi, Opt::PageRange,
            Opt::OnePerPage, Opt::Ocr, Opt::Tables, Opt::Columns, Opt::EmbedFonts, Opt::Bookmarks, Opt::PdfA, Opt::Encoding, Opt::CsvSeparator, Opt::PrettyJson, Opt::HeaderRow,
            Opt::Mp3Kbps, Opt::Channels, Opt::Normalize, Opt::TrimStart, Opt::TrimSilence, Opt::KeepTags, Opt::VideoHeight, Opt::Fps,
            Opt::Mute, Opt::FastStart, Opt::NameTemplate, Opt::MergeName, Opt::FileBookmarks, Opt::KeepBookmarks, Opt::KeepForms, Opt::DuplexBlank,
            Opt::OpenPassword, Opt::SplitHow, Opt::SplitEvery, Opt::SplitRanges, Opt::SplitRemove, Opt::SplitName,
            Opt::PageNumbers, Opt::NumberPos, Opt::NumberFormat, Opt::NumberStart, Opt::NumberSize, Opt::Watermark, Opt::WatermarkText,
            Opt::WatermarkOpacity, Opt::WatermarkAngle, Opt::WatermarkPages, Opt::SqueezeLevel, Opt::StripMeta, Opt::BwScans,
        ];
        for opt in wanted {
            assert!(defs.iter().any(|d| d.opt == opt), "{opt:?} non compare in nessun pannello");
        }
        let base = options_from(&Values::default());
        for def in defs {
            let mut values = Values::default();
            // I campi di «Dividi» contano solo nel loro modo: si confrontano con quel modo e basta.
            if matches!(def.opt, Opt::SplitRemove | Opt::SplitEvery) {
                let mode = if def.opt == Opt::SplitRemove { 4 } else { 1 };
                let mut plain = Values::default();
                plain.set(&SPLIT_HOW, mode);
                values.set(&SPLIT_HOW, mode);
                values.set_text(def, "2-4");
                values.set(def, 5);
                assert_ne!(options_from(&values), options_from(&plain), "{:?} non arriva al motore", def.opt);
                continue;
            }
            // Le scelte dei numeri e della filigrana contano con il loro interruttore acceso.
            let parent = match def.opt {
                Opt::NumberPos | Opt::NumberFormat | Opt::NumberStart | Opt::NumberSize => Some(&PAGE_NUMBERS),
                Opt::WatermarkText | Opt::WatermarkOpacity | Opt::WatermarkAngle | Opt::WatermarkPages => Some(&WATERMARK),
                _ => None,
            };
            let base = match parent {
                Some(p) => {
                    let mut plain = Values::default();
                    plain.set(p, 1);
                    values.set(p, 1);
                    options_from(&plain)
                }
                None => base.clone(),
            };
            match &def.control {
                Control::Switch => {
                    assert!(def.default <= 1);
                    values.set(def, 1 - def.default);
                }
                Control::Choice(choices) => {
                    assert!(choices.iter().any(|c| c.value == def.default), "{:?}: il valore di partenza non è tra le scelte", def.opt);
                    let other = choices.iter().find(|c| c.value != def.default).expect("almeno due scelte");
                    values.set(def, other.value);
                }
                Control::Range { min, max, .. } => {
                    assert!(def.default >= *min && def.default <= *max);
                    values.set(def, *max);
                }
                Control::Segments(choices) => {
                    let other = choices.iter().find(|c| c.value != def.default).expect("almeno due scelte");
                    values.set(def, other.value);
                }
                Control::Text { .. } => values.set_text(def, if def.opt == Opt::NumberStart { "5" } else { "2-4" }),
                Control::Password => values.set_text(def, "segreto"),
                Control::Position => values.set(def, (def.default + 1) % 9),
            }
            assert_ne!(options_from(&values), base, "{:?} non arriva al motore", def.opt);
        }
        let mut values = Values::default();
        values.exists = Exists::Ask;
        assert_eq!(options_from(&values).exists, Exists::Ask);
        let mut values = Values::default();
        values.set_text(&PDF_PAGES, "3-1");
        assert!(validate(&values, Kind::Pdf, "docx").is_err());
        assert!(validate(&values, Kind::Image, "jpeg").is_ok(), "le pagine non contano per le immagini");
        values.set_text(&PDF_PAGES, "1-3,5");
        assert!(validate(&values, Kind::Pdf, "docx").is_ok());
    }

    #[test]
    fn options_depend_on_the_chosen_format() {
        let keys = |k, a| option_defs(k, a).iter().map(|d| d.opt).collect::<Vec<_>>();
        assert!(keys(Kind::Pdf, "jpg").contains(&Opt::Dpi));
        assert!(!keys(Kind::Pdf, "docx").contains(&Opt::Dpi));
        assert!(keys(Kind::Pdf, "docx").contains(&Opt::Tables));
        assert!(keys(Kind::Image, "jpeg").contains(&Opt::WhiteBackground));
        assert!(!keys(Kind::Image, "png").contains(&Opt::WhiteBackground));
        assert_eq!(keys(Kind::Image, "ocr"), vec![Opt::NameTemplate]);
        assert!(keys(Kind::Text, "csv").contains(&Opt::CsvSeparator));
        assert!(keys(Kind::Video, "mp3").contains(&Opt::Mp3Kbps));
        assert!(keys(Kind::Audio, "wav").contains(&Opt::Channels) && !keys(Kind::Audio, "wav").contains(&Opt::Mp3Kbps));
    }

    #[test]
    fn output_goes_to_the_chosen_folder() {
        let base = std::env::temp_dir().join(format!("cedro-conv-{}", std::process::id()));
        let dest = base.join("fuori");
        fs::create_dir_all(&base).unwrap();
        let src = base.join("nota.txt");
        fs::write(&src, "ciao\nmondo\n").unwrap();
        let opts = engine::Options::default();
        convert_one(&src, "md", 82, Some(&dest), &opts).unwrap();
        assert!(dest.join("nota.md").is_file());
        assert!(!base.join("nota.md").exists());
        assert!(src.is_file());
        // Una seconda conversione non sovrascrive: il nome prende il -2.
        convert_one(&src, "md", 82, Some(&dest), &opts).unwrap();
        assert!(dest.join("nota-2.md").is_file());
        // Con «sostituisci» il file esistente viene rimpiazzato.
        let replace = engine::Options { exists: engine::Exists::Replace, ..opts };
        convert_one(&src, "md", 82, Some(&dest), &replace).unwrap();
        assert!(!dest.join("nota-3.md").exists());
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod simple_tests {
    use super::*;

    #[test]
    fn simple_formats_are_real_engine_formats() {
        for cat in CATEGORIES {
            let simple = simple_formats(cat.kind);
            assert!(!simple.is_empty());
            for f in simple {
                // Unire e dividere sono voci delle pagine del PDF, non formati: il motore le ha tra gli strumenti.
                let known = formats(cat.kind).iter().any(|s| s.id == f.id) || (is_page_action(cat.kind, f.id) && engine::slices(cat.kind, Mode::Tools).iter().any(|s| s.id == f.id));
                assert!(known, "{} manca in {}", f.id, cat.name);
            }
        }
    }

    #[test]
    fn kind_of_files_skips_unknown() {
        let list = vec![PathBuf::from("a.xyz"), PathBuf::from("b.png")];
        assert!(kind_of_files(&list) == Some(Kind::Image));
    }
}
