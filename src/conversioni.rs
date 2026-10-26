//! Il pannello Conversioni: categorie, formati, opzioni e conversione di una coda di file.
//! Il modello è uguale su Windows e su Linux: i due pannelli disegnano le stesse opzioni e passano
//! i valori scelti al motore con `options_from`.

use std::path::{Path, PathBuf};

use crate::engine::{self, Encoding, Exists, Kind, Mode, PageSel, PageSize, Slice};

/// Una voce del menu a sinistra del pannello.
pub struct Category {
    pub kind: Kind,
    pub name: &'static str,
    pub hint: &'static str,
    pub accepts: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Image, name: "Immagini", hint: "Cambia formato, ridimensiona e raddrizza le foto.", accepts: "PNG, JPEG, WebP, BMP, GIF, TIFF" },
    Category { kind: Kind::Doc, name: "Documenti", hint: "Da Word, ODT e RTF a PDF.", accepts: "DOCX, DOC, ODT, RTF" },
    Category { kind: Kind::Pdf, name: "PDF", hint: "Da PDF a Word modificabile, a immagini o a testo.", accepts: "PDF" },
    Category { kind: Kind::Text, name: "Testo", hint: "TXT, Markdown, CSV, JSON e HTML.", accepts: "TXT, MD, CSV, JSON, HTML, XML" },
    Category { kind: Kind::Audio, name: "Audio", hint: "WAV, MP3, OGG, M4A, AAC e FLAC.", accepts: "WAV, MP3, OGG, M4A, AAC, FLAC" },
    Category { kind: Kind::Video, name: "Video", hint: "MP4, WebM, MOV, MKV e AVI.", accepts: "MP4, WebM, MOV, MKV, AVI" },
];

/// I formati di uscita di una categoria.
pub fn formats(kind: Kind) -> &'static [Slice] {
    engine::slices(kind, Mode::Convert)
}

/// Gli strumenti di una categoria (comprimi, ruota, OCR…). Vuoto se non ce ne sono.
pub fn tools(kind: Kind) -> &'static [Slice] {
    let list = engine::slices(kind, Mode::Tools);
    if list.iter().any(|s| s.id == "none") {
        &[]
    } else {
        list
    }
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
}

/// Tutte le opzioni che il motore capisce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
const FAST_START: OptionDef = sw(Opt::FastStart, "Ottimizza per il web", "L'avvio parte subito", false);

/// Le opzioni che valgono per questa categoria e questa voce; il motore le usa tutte.
pub fn option_defs(kind: Kind, action: &str) -> Vec<&'static OptionDef> {
    match (kind, action) {
        (Kind::Image, "ocr") => vec![],
        (Kind::Image, "jpeg" | "compress" | "pdf") => vec![&IMAGE_SIZE, &STRAIGHTEN, &WHITE_BG],
        (Kind::Image, _) => vec![&IMAGE_SIZE, &STRAIGHTEN],
        (Kind::Doc, "pdf") => vec![&PAGE, &EMBED_FONTS, &BOOKMARKS, &PDF_A, &PASSWORD],
        (Kind::Pdf, "jpg" | "png") => vec![&PDF_DPI, &PDF_PAGES, &ONE_PER_PAGE],
        (Kind::Pdf, "docx") => vec![&OCR, &TABLES, &COLUMNS, &PDF_PAGES],
        (Kind::Pdf, "txt") => vec![&OCR, &PDF_PAGES],
        (Kind::Text, "pdf") => vec![&PAGE, &EMBED_FONTS, &PDF_A],
        (Kind::Text, "csv") => vec![&TEXT_ENCODING, &CSV_SEPARATOR],
        (Kind::Text, "json") => vec![&TEXT_ENCODING, &PRETTY_JSON, &HEADER_ROW],
        (Kind::Text, "md" | "html") => vec![&TEXT_ENCODING, &HEADER_ROW],
        (Kind::Text, _) => vec![&TEXT_ENCODING],
        (Kind::Audio | Kind::Video, "mp3") => vec![&MP3_KBPS, &AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE, &KEEP_TAGS],
        (Kind::Audio, "wav") => vec![&AUDIO_CHANNELS, &NORMALIZE, &TRIM_START, &TRIM_SILENCE],
        (Kind::Video, "mp4") => vec![&HEIGHT, &VIDEO_FPS, &MUTE, &FAST_START],
        _ => vec![],
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
    o
}

/// Esegue `action` su un file. Con `dest` le copie nuove finiscono in quella cartella
/// invece che accanto all'originale; il nome non ne sovrascrive mai un altro.
pub fn convert_one(path: &Path, action: &str, quality: u8, dest: Option<&Path>, opts: &engine::Options) -> Result<String, String> {
    // «Accanto all'originale» nel pannello vuol dire proprio accanto, qualunque cartella usi la ruota.
    let beside = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    engine::perform_with(path, action, quality, Some(dest.unwrap_or(beside)), opts).map(|out| out.message).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn every_def() -> Vec<&'static OptionDef> {
        let mut all: Vec<&'static OptionDef> = Vec::new();
        for c in CATEGORIES {
            for s in formats(c.kind).iter().chain(tools(c.kind).iter()) {
                for d in option_defs(c.kind, s.id) {
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
        let expected = engine::Options { dpi: Some(144), mp3_kbps: Some(128), ..Default::default() };
        assert_eq!(opts, expected);
    }

    #[test]
    fn every_option_has_a_working_default_and_reaches_the_engine() {
        let defs = every_def();
        let wanted = [
            Opt::Straighten, Opt::WhiteBackground, Opt::MaxSide, Opt::PageSize, Opt::Password, Opt::Dpi, Opt::PageRange,
            Opt::OnePerPage, Opt::Ocr, Opt::Tables, Opt::Columns, Opt::EmbedFonts, Opt::Bookmarks, Opt::PdfA, Opt::Encoding, Opt::CsvSeparator, Opt::PrettyJson, Opt::HeaderRow,
            Opt::Mp3Kbps, Opt::Channels, Opt::Normalize, Opt::TrimStart, Opt::TrimSilence, Opt::KeepTags, Opt::VideoHeight, Opt::Fps,
            Opt::Mute, Opt::FastStart,
        ];
        for opt in wanted {
            assert!(defs.iter().any(|d| d.opt == opt), "{opt:?} non compare in nessun pannello");
        }
        let base = options_from(&Values::default());
        for def in defs {
            let mut values = Values::default();
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
                Control::Text { .. } => values.set_text(def, "2-4"),
                Control::Password => values.set_text(def, "segreto"),
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
        assert!(keys(Kind::Image, "ocr").is_empty());
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
