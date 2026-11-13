//! Lo stato comune a tutti i temi: la coda dei file, le scelte, il lavoro in corso, i link da
//! scaricare e la trascrizione. Cambiare tema o modalità non tocca niente di tutto questo.
//! I lavori girano in thread a parte e lasciano gli eventi su un canale; chi ospita la finestra
//! riceve una chiamata a `wake` e chiama `App::pump` dal thread dell'interfaccia.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use crate::conversioni::{self, OptionDef, Values, CATEGORIES};
use crate::engine::{self, Exists, ExistsAsk, ExistsChoice, Kind};
use crate::trascrizione;
use crate::youtube;

use super::gfx::Image;
use super::media;

/// Le quattro attività della finestra.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Activity {
    Convert,
    Transcribe,
    YouTube,
    Web,
    Firma,
}

impl Activity {
    pub const ALL: [Activity; 5] = [Activity::Convert, Activity::Transcribe, Activity::YouTube, Activity::Web, Activity::Firma];

    pub fn name(self) -> &'static str {
        match self {
            Activity::Convert => "Conversioni",
            Activity::Transcribe => "Trascrizione",
            Activity::YouTube => "YouTube",
            Activity::Web => "Video dal web",
            Activity::Firma => "Firma digitale",
        }
    }

    pub fn icon(self) -> &'static str {
        use super::assets::*;
        match self {
            Activity::Convert => CONVERT,
            Activity::Transcribe => TRANSCRIBE,
            Activity::YouTube => VIDEO,
            Activity::Web => WEB,
            Activity::Firma => SHIELD,
        }
    }
}

/// Le pagine di Cedro 4 Labs: «Inizio», le sei del laboratorio e le due del Cedro di sempre.
/// Ognuna è un modo di arrivare a un'attività, a un tipo di file e a una voce che il motore ha già.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lab {
    Home,
    Immagini,
    Anonimizza,
    Figura,
    Dati,
    Referti,
    Trascrivi,
    Converti,
    Pdf,
    /// Il Cedro di partenza: YouTube e gli altri siti in una pagina sola.
    Video,
    /// Il Cedro di partenza: firma e verifica.
    Firma,
}

impl Lab {
    /// Le pagine del Cedro di partenza, nell'ordine della barra.
    pub const CEDRO: [Lab; 6] = [Lab::Home, Lab::Converti, Lab::Pdf, Lab::Trascrivi, Lab::Video, Lab::Firma];

    pub const LAB: [Lab; 6] = [Lab::Immagini, Lab::Anonimizza, Lab::Figura, Lab::Dati, Lab::Referti, Lab::Trascrivi];
    pub const STANDARD: [Lab; 2] = [Lab::Converti, Lab::Pdf];

    pub fn name(self) -> &'static str {
        match self {
            Lab::Home => "Inizio",
            Lab::Immagini => "Salva immagini e vetrini",
            Lab::Anonimizza => "Togli i nomi",
            Lab::Figura => "Prepara una figura",
            Lab::Dati => "Converti dati e piastre",
            Lab::Referti => "Firma e manda referti",
            Lab::Trascrivi => "Trascrivi e riassumi",
            Lab::Converti => "Converti file",
            Lab::Pdf => "Lavora sui PDF",
            Lab::Video => "Video dal web",
            Lab::Firma => "Firma",
        }
    }

    /// Il nome nella barra del Cedro di partenza.
    pub fn cedro_name(self) -> &'static str {
        match self {
            Lab::Converti => "Converti",
            Lab::Pdf => "PDF",
            Lab::Trascrivi => "Trascrivi",
            _ => self.name(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileState {
    Waiting,
    Running,
    Done,
    Skipped,
    Failed,
}

/// Un file nella coda.
#[derive(Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub kind: Kind,
    /// Solo per la trascrizione (vocali .opus, .amr…): nessuna categoria lo converte.
    pub voice_only: bool,
    pub state: FileState,
    /// Cosa dire accanto al file: «In attesa», «Fatto · …», l'errore.
    pub message: String,
    /// Il file scritto, quando è finito.
    pub output: Option<PathBuf>,
    /// Spuntato nella tabella: la conversione prende solo quelli spuntati.
    pub checked: bool,
    /// Le scelte con cui il file è stato fatto: se cambiano, si rifà.
    pub done_key: Option<String>,
    /// Quanto è fatto del file in lavoro (0..1), quando il motore lo sa: la barra nella riga.
    pub percent: Option<f32>,
    /// Unire i PDF: le pagine di questo file che entrano («tutte» se vuoto), per esempio `1-3,5`.
    pub pages: String,
}

impl FileItem {
    /// Il nome da mostrare: segnato come testo dell'utente, che la traduzione lascia com'è (vedi `i18n::lit`).
    pub fn name(&self) -> String {
        crate::i18n::lit(self.path.file_name().and_then(|n| n.to_str()).unwrap_or("file"))
    }
}

/// Un link nella coda di YouTube o di «Video dal web».
#[derive(Clone, Debug)]
pub struct LinkItem {
    pub url: String,
    pub title: String,
    pub detail: String,
    pub state: FileState,
    pub message: String,
    pub percent: f32,
}

/// Le scelte della scheda YouTube (e di quella dei video dal web, che ne ha una copia sua).
#[derive(Clone, Debug)]
pub struct Links {
    pub web: bool,
    pub items: Vec<LinkItem>,
    /// Quello che si sta scrivendo nel campo del link.
    pub typed: String,
    pub audio: bool,
    /// Indici in `VIDEO_QUALITY`, `AUDIO_QUALITY`, nei formati e nei sottotitoli.
    pub quality: usize,
    pub format: usize,
    pub subtitles: usize,
    pub thumbnail: bool,
    pub chapters: bool,
    pub split_chapters: bool,
    pub skip_sponsors: bool,
    pub normalize: bool,
    pub playlist: bool,
    pub skip_existing: bool,
    pub dest: PathBuf,
    pub log: String,
    pub note: String,
    pub progress: f32,
}

pub const VIDEO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Fino a 1080p · consigliato", 1080), ("720p · file leggero", 720), ("480p · file piccolo", 480)];
pub const AUDIO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Buona · consigliata", 1), ("Leggera", 2)];
pub const VIDEO_FORMATS: &[&str] = &["mp4", "mkv", "webm"];
pub const AUDIO_FORMATS: &[&str] = &["mp3", "m4a", "opus", "wav", "flac"];
pub const SUBTITLES: &[(&str, u32)] = &[("Nessuno", 0), ("Italiano", 1), ("Italiano e inglese", 2), ("Tutti", 3)];

pub const WEB_HINT: &str = "Incolla il link della pagina che contiene il video (RaiPlay, Vimeo, Internet Archive e oltre mille altri siti).";
pub const WEB_NOTE: &str = "Funziona con i siti che yt-dlp riconosce. Se un sito non è supportato, o il video è protetto o chiede un accesso, lo vedi accanto al link.";
pub const YT_HINT: &str = "Incolla il link di un video o di una playlist di YouTube (o di un altro sito).";

/// La cartella Download dell'utente.
pub fn downloads_dir() -> PathBuf {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    home.map(|h| PathBuf::from(h).join(if cfg!(windows) { "Downloads" } else { "Scaricati" }))
        .filter(|p| p.is_dir())
        .or_else(|| {
            #[cfg(not(windows))]
            return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).filter(|p| p.is_dir());
            #[cfg(windows)]
            None
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Links {
    pub fn new(web: bool) -> Links {
        Links {
            web,
            items: Vec::new(),
            typed: String::new(),
            audio: false,
            quality: 1,
            format: 0,
            subtitles: 0,
            thumbnail: true,
            chapters: true,
            split_chapters: false,
            skip_sponsors: false,
            normalize: false,
            playlist: true,
            skip_existing: true,
            dest: downloads_dir(),
            log: String::new(),
            note: String::new(),
            progress: 0.0,
        }
    }

    pub fn qualities(&self) -> &'static [(&'static str, u32)] {
        if self.audio {
            AUDIO_QUALITY
        } else {
            VIDEO_QUALITY
        }
    }

    pub fn formats(&self) -> &'static [&'static str] {
        if self.audio {
            AUDIO_FORMATS
        } else {
            VIDEO_FORMATS
        }
    }

    /// Passa da video a solo audio (o il contrario) con le scelte consigliate di quel tipo.
    pub fn set_audio(&mut self, audio: bool) {
        if self.audio != audio {
            self.audio = audio;
            self.quality = 1;
            self.format = 0;
        }
    }

    /// Aggiunge i link trovati nel testo; restituisce quanti sono nuovi.
    pub fn add(&mut self, text: &str) -> usize {
        let mut added = 0;
        // Anche dalla scheda YouTube si scaricano tutti i siti che yt-dlp conosce (Veoh, Vimeo…).
        for url in youtube::urls_in(text, true) {
            if self.items.iter().any(|r| r.url == url) {
                continue;
            }
            self.items.push(LinkItem { url, title: String::new(), detail: String::new(), state: FileState::Waiting, message: "In attesa".into(), percent: 0.0 });
            added += 1;
        }
        added
    }

    pub fn request(&self) -> youtube::Request {
        let mut r = youtube::Request::new(self.items.iter().map(|i| i.url.clone()).collect(), self.audio, self.dest.clone());
        r.quality = self.qualities().get(self.quality).map(|q| q.1).unwrap_or(r.quality);
        r.format = self.formats().get(self.format).copied().unwrap_or(self.formats()[0]).to_string();
        r.subtitles = if self.audio { 0 } else { SUBTITLES.get(self.subtitles).map(|s| s.1).unwrap_or(0) };
        r.thumbnail = self.thumbnail;
        r.chapters = self.chapters;
        r.split_chapters = self.split_chapters;
        r.skip_sponsors = !self.web && self.skip_sponsors;
        r.normalize = self.audio && self.normalize;
        r.playlist = self.playlist;
        r.skip_existing = self.skip_existing;
        r
    }

    fn log_line(&mut self, text: &str) {
        self.log.push_str(text);
        self.log.push('\n');
        if self.log.len() > 40_000 {
            let cut = self.log.len() - 30_000;
            let at = self.log.char_indices().map(|(i, _)| i).find(|i| *i >= cut).unwrap_or(0);
            self.log.drain(..at);
        }
    }

    /// Riepilogo in parole delle scelte (modalità Semplificata).
    pub fn summary(&self) -> String {
        let q = self.qualities().get(self.quality).map(|q| q.0).unwrap_or("");
        let f = self.formats().get(self.format).copied().unwrap_or("").to_uppercase();
        format!("{} {f} · {} · in {}", if self.audio { "Audio" } else { "Video" }, q.split(" · ").next().unwrap_or(q), short_dir(&self.dest))
    }
}

/// Le velocità di ascolto: etichetta e percento.
pub const SPEEDS: [(&str, u32); 4] = [("0,75×", 75), ("1×", 100), ("1,5×", 150), ("2×", 200)];

/// Le scelte della trascrizione.
#[derive(Clone, Debug)]
pub struct Transcribe {
    pub format: trascrizione::Format,
    pub model: usize,
    pub log: Vec<trascrizione::Line>,
    /// Avanzamento del file in lavoro (0..100).
    pub percent: u8,
    /// Quando il file in lavoro ha dato la prima percentuale, e quale: da qui il tempo che resta.
    pub since: Option<(std::time::Instant, u8)>,
    /// Quante persone parlano: 0 Automatico, 1 nessun riconoscimento, poi il numero.
    pub speakers: u8,
    /// La finestrella «Quante persone parlano?» è aperta.
    pub asking: bool,
    /// I termini dei referti veterinari suggeriti al motore (spunta nella finestrella, ricordata).
    pub glossario_vet: bool,
}

/// Il lavoro in corso.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JobKind {
    Convert,
    Transcribe,
    Download { web: bool },
}

pub struct Job {
    pub kind: JobKind,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub cancel: Arc<AtomicBool>,
    /// Le scelte in vigore all'avvio e i file in lavoro (indici in `files`).
    pub key: String,
    pub targets: Vec<usize>,
    /// Quanto è fatto del file in lavoro (0..1), per la barra del lavoro intero.
    pub part: f32,
    /// Quando è partito il file in lavoro, per il tempo che resta.
    pub file_started: Option<std::time::Instant>,
}

/// Il testo di un panic preso da `catch_unwind`, scritto anche nel registro.
pub fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    let what = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
    crate::append_log(&format!("errore interno: {what}"));
    if what.is_empty() { "Errore interno".to_string() } else { format!("Errore interno: {what}") }
}

/// Esegue `f` e trasforma un panic (un file che manda in crisi un decodificatore) in un errore: il lavoro
/// va avanti con gli altri file e la finestra non resta «in corso» per sempre.
pub fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| Err(panic_text(&*p)))
}

/// Il tempo che resta, in parole: «circa 12 min», «meno di un minuto».
pub fn time_left(secs: f32) -> String {
    let mins = (secs / 60.0).round() as u32;
    if secs < 60.0 {
        "meno di un minuto".into()
    } else if mins < 90 {
        format!("circa {mins} min")
    } else {
        format!("circa {} h {:02} min", mins / 60, mins % 60)
    }
}

/// Cosa i thread di lavoro dicono all'interfaccia.
pub enum Event {
    Started(PathBuf),
    Finished(PathBuf, Result<(PathBuf, String), String>),
    /// Il file di uscita esiste già e la regola è «Chiedi ogni volta».
    Ask(PathBuf, Sender<ExistsChoice>),
    TranscribeLog(trascrizione::Line),
    TranscribePercent(usize, u8),
    /// L'avanzamento (0..1) del file che si sta convertendo.
    ConvertPercent(PathBuf, f32),
    Link(bool, youtube::Event),
    LinksFinished(bool, Result<Vec<PathBuf>, String>),
    /// Il thread di lavoro si è interrotto per un errore interno: i file ancora in attesa o in corso falliscono.
    Aborted(String),
    JobDone,
    Media(PathBuf, media::Loaded),
    Preview(PathBuf, usize, Option<Image>),
    /// La miniatura di una pagina (la striscia di «Dividi», le pagine del file unito).
    PageThumb(PathBuf, usize, Option<Image>),
    /// «Comprimi»: quanto pesa il file e quanto peserebbe compresso (byte), per la chiave delle scelte.
    Estimate(PathBuf, String, Result<(u64, u64), String>),
    /// «Pagine»: la pagina vera con numeri e filigrana, per la chiave delle scelte.
    StampPreview(String, Option<Image>),
    /// «Rivedi»: l'onda dell'audio del file che si sta rivedendo.
    Wave(PathBuf, Option<Vec<f32>>),
    /// «Riassunto AI»: cosa arriva dal motore per questo file.
    Summary(PathBuf, crate::riassunto::Progress),
}

/// Cosa fa la scheda PDF: converte, unisce, divide, cambia le pagine o comprime.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PdfMode {
    Convert,
    Merge,
    Split,
    Pages,
    Compress,
}

/// «Pagine»: le pagine di un PDF come diventeranno, nell'ordine nuovo.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Board {
    /// (pagina dell'originale da 1, gradi in più, tolta).
    pub pages: Vec<(u32, i32, bool)>,
    /// Le pagine scelte (posti in `pages`, da 0).
    pub sel: Vec<usize>,
}

impl Board {
    pub fn new(total: usize) -> Board {
        Board { pages: (1..=total as u32).map(|p| (p, 0, false)).collect(), sel: Vec::new() }
    }

    /// Le pagine che si salvano: (pagina da 1, gradi).
    pub fn kept(&self) -> Vec<(u32, i32)> {
        self.pages.iter().filter(|p| !p.2).map(|p| (p.0, p.1)).collect()
    }

    /// Vero se è com'era: niente spostato, girato o tolto.
    pub fn untouched(&self) -> bool {
        self.pages.iter().enumerate().all(|(k, p)| p.0 == k as u32 + 1 && p.1.rem_euclid(360) == 0 && !p.2)
    }
}

/// Una domanda in attesa di risposta (file che esiste già).
pub struct Ask {
    pub path: PathBuf,
    pub reply: Sender<ExistsChoice>,
}

/// Un messaggio da far vedere (un errore, un avviso, «fatto»).
#[derive(Clone, Debug)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

pub struct App {
    pub activity: Activity,
    /// Cedro 4 Labs: la pagina scelta nella barra di sinistra.
    pub lab: Lab,
    pub files: Vec<FileItem>,
    pub category: usize,
    /// Il formato o lo strumento scelto (id del motore).
    pub action: String,
    pub values: Values,
    pub quality: u8,
    /// `None` = accanto all'originale.
    pub dest: Option<PathBuf>,
    pub job: Option<Job>,
    /// Il file messo in evidenza (ispettore, anteprima del documento).
    pub selected: Option<usize>,
    pub transcribe: Transcribe,
    pub youtube: Links,
    pub web: Links,
    pub notice: Option<Notice>,
    pub ask: Option<Ask>,
    /// Miniature, misure e numero di pagine dei file già letti.
    pub media: HashMap<PathBuf, media::Loaded>,
    /// Pagine dei documenti già disegnate per l'anteprima: (file, pagina) → immagine.
    pub previews: HashMap<(PathBuf, usize), Option<Image>>,
    /// Le pagine dell'anteprima che non si sono potute disegnare (niente rotellina che gira per sempre).
    pub previews_failed: std::collections::HashSet<(PathBuf, usize)>,
    /// La pagina dell'anteprima del documento.
    pub preview_page: usize,
    /// Il file audio o video che suona nell'anteprima (tasto Play).
    pub player: Option<super::player::Player>,
    /// Dove sta il cursore della barra mentre lo si trascina (0..1).
    pub media_scrub: Option<f32>,
    /// Trascrivi: 0 solo onda, 1 onda e testo, 2 solo testo.
    pub tr_view: u8,
    /// Velocità (indice in `SPEEDS`) e volume (0..1) di ascolto dell'audio in anteprima.
    pub play_speed: u8,
    pub play_vol: f32,
    /// Le miniature piccole delle pagine dei PDF: (file, pagina) → immagine (`None` = in arrivo).
    pub page_thumbs: HashMap<(PathBuf, usize), Option<Image>>,
    /// «Dividi»: i tagli scelti sulla striscia, file per file (un file nuovo dopo queste pagine, da 1).
    pub cuts: HashMap<PathBuf, Vec<u32>>,
    /// La conversione scelta prima di passare a Unisci o Dividi: «Converti» la ritrova.
    last_convert: String,
    /// Il Cedro di partenza: la voce della pagina PDF (Unisci, Dividi…) mentre si guarda «Converti».
    cedro_pdf_action: Option<String>,
    /// La finestra è in Semplificata (la tiene a giorno lo schermo): «Dividi» lì è una pagina per file.
    pub simple_view: bool,
    /// «Pagine»: le pagine di ogni PDF come le si è sistemate.
    pub boards: HashMap<PathBuf, Board>,
    /// «Pagine»: la pagina (posto tra quelle che restano, da 0) dell'anteprima con numeri e filigrana.
    pub stamp_page: usize,
    /// «Pagine»: l'anteprima con numeri e filigrana (chiave chiesta, ultima immagine arrivata).
    pub stamp_view: (String, Option<Image>),
    /// «Rivedi»: la trascrizione aperta per la revisione.
    pub review: Option<super::review::Review>,
    /// Dove sta il cursore della barra di «Rivedi» mentre lo si trascina (0..1).
    pub review_scrub: Option<f32>,
    /// «Riassunto AI»: quello in corso (uno alla volta) e l'ultimo non riuscito.
    pub summary_job: Option<super::summary::Job>,
    pub summary_failure: Option<(PathBuf, crate::riassunto::Failure)>,
    /// «Comprimi»: (file, chiave delle scelte) → (peso, peso compresso); `None` = si sta calcolando.
    pub estimates: HashMap<(PathBuf, String), Option<Result<(u64, u64), String>>>,
    /// «Firma digitale»: i file da verificare, gli ultimi aperti e il visualizzatore.
    pub firma: super::firma::FirmaUi,
    media_asked: std::collections::HashSet<PathBuf>,
    pub(super) tx: Sender<Event>,
    rx: Receiver<Event>,
    pub(super) wake: Arc<dyn Fn() + Send + Sync>,
    loader: media::Loader,
    pdfwork: super::pdfwork::Worker,
}

/// «C:\Users\Mario\Documenti\Cedro» → «Cedro»; vuoto se non c'è un nome.
pub fn short_dir(p: &Path) -> String {
    p.file_name().and_then(|n| n.to_str()).map(str::to_string).unwrap_or_else(|| p.display().to_string())
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", b / 1024.0 / 1024.0 / 1024.0).replace('.', ",")
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0).replace('.', ",")
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} byte")
    }
}

impl App {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> App {
        let (tx, rx) = channel();
        let loader = media::Loader::new(tx.clone(), wake.clone());
        let pdfwork = super::pdfwork::Worker::new(tx.clone(), wake.clone());
        let mut values = Values::new();
        let st = crate::read_settings();
        if st.replace {
            values.exists = Exists::Replace;
        }
        // La calibrazione del microscopio si scrive una volta e resta.
        values.set_text(&conversioni::FIG_CALIBRATION, &st.calibrazione_10x);
        values.set_text(&conversioni::RX_CLINIC, &st.nome_clinica);
        let mut app = App {
            activity: Activity::Convert,
            lab: Lab::Home,
            files: Vec::new(),
            category: 0,
            action: String::new(),
            values,
            quality: engine::read_quality(),
            dest: None,
            job: None,
            selected: None,
            transcribe: Transcribe { format: trascrizione::Format::Word, model: trascrizione::default_model(), log: Vec::new(), percent: 0, since: None, speakers: 0, asking: false, glossario_vet: crate::read_settings().glossario_vet },
            youtube: Links::new(false),
            web: Links::new(true),
            notice: None,
            ask: None,
            media: HashMap::new(),
            previews: HashMap::new(),
            previews_failed: Default::default(),
            preview_page: 0,
            player: None,
            media_scrub: None,
            tr_view: 1,
            play_speed: 1,
            play_vol: 0.8,
            page_thumbs: HashMap::new(),
            cuts: HashMap::new(),
            last_convert: String::new(),
            cedro_pdf_action: None,
            simple_view: false,
            boards: HashMap::new(),
            stamp_page: 0,
            stamp_view: (String::new(), None),
            estimates: HashMap::new(),
            review: None,
            review_scrub: None,
            summary_job: None,
            summary_failure: None,
            firma: Default::default(),
            media_asked: Default::default(),
            tx,
            rx,
            wake,
            loader,
            pdfwork,
        };
        app.action = app.default_action(false);
        app
    }

    pub fn kind(&self) -> Kind {
        CATEGORIES[self.category].kind
    }

    /// Cedro 4 Labs: apre una pagina, cioè sceglie l'attività, il tipo di file e la voce che le servono.
    /// I file in coda restano; se ce n'è uno del tipo giusto la pagina parte da quello.
    pub fn set_lab(&mut self, lab: Lab, simple: bool) {
        let has = |app: &App, k: Kind| app.files.iter().any(|f| !f.voice_only && f.kind == k);
        let cat = |app: &mut App, k: Kind| {
            if let Some(i) = CATEGORIES.iter().position(|c| c.kind == k) {
                app.set_category(i, simple);
            }
        };
        match lab {
            // «Inizio» è la porta dei file da convertire: firma e trascrizione hanno la loro pagina.
            Lab::Home => self.activity = Activity::Convert,
            Lab::Video => self.activity = Activity::YouTube,
            Lab::Referti | Lab::Firma => {
                self.activity = Activity::Firma;
                if !self.firma.sign.scanned_once {
                    let wake = self.wake.clone();
                    self.firma.sign.scan(wake);
                }
            }
            Lab::Trascrivi => self.activity = Activity::Transcribe,
            Lab::Immagini | Lab::Figura => {
                self.activity = Activity::Convert;
                cat(self, Kind::Image);
                if lab == Lab::Figura {
                    self.set_action("figure");
                } else if matches!(self.action.as_str(), "figure" | "strip") || (simple && self.action == self.default_action(true)) {
                    // In «Semplice» la pagina offre JPEG, TIFF, PNG e PDF: il WebP consigliato da
                    // «Converti file» qui non c'è, e la scheda direbbe che è una voce scelta in «Completa».
                    self.action = if simple { "jpeg".to_string() } else { self.default_action(false) };
                }
            }
            Lab::Anonimizza => {
                self.activity = Activity::Convert;
                // Il tipo resta quello di adesso se si può anonimizzare; se no il primo che c'è in coda.
                let ok = [Kind::Image, Kind::Pdf, Kind::Doc];
                if !ok.contains(&self.kind()) || !has(self, self.kind()) {
                    let k = ok.into_iter().find(|k| has(self, *k)).unwrap_or(Kind::Image);
                    cat(self, k);
                }
                self.set_action("strip");
            }
            Lab::Dati => {
                self.activity = Activity::Convert;
                cat(self, Kind::Text);
                // Con soli Excel in coda «Porta in Excel» fallirebbe: si parte dal CSV.
                let mut tabelle = self.files.iter().filter(|f| !f.voice_only && f.kind == Kind::Text).peekable();
                let solo_fogli = tabelle.peek().is_some() && tabelle.all(|f| foglio_di_calcolo(&f.path));
                if solo_fogli && self.action != "csv" {
                    self.set_action("csv");
                } else if !matches!(self.action.as_str(), "xlsx" | "csv") {
                    self.set_action("xlsx");
                }
            }
            Lab::Pdf => {
                self.activity = Activity::Convert;
                cat(self, Kind::Pdf);
                if self.action == "strip" {
                    self.action = self.default_action(simple);
                }
            }
            Lab::Converti => {
                self.activity = Activity::Convert;
                // Arrivando da «Converti dati» senza tabelle in coda la pagina resterebbe vuota: si passa
                // al primo tipo di file che c'è davvero.
                if !has(self, self.kind()) {
                    if let Some(k) = self.files.iter().find(|f| !f.voice_only && CATEGORIES.iter().any(|c| c.kind == f.kind)).map(|f| f.kind) {
                        cat(self, k);
                    }
                }
                if matches!((self.kind(), self.action.as_str()), (Kind::Image, "figure") | (_, "strip")) {
                    self.action = self.default_action(simple);
                }
            }
        }
        self.lab = lab;
        if self.activity != Activity::Firma && self.selected.is_none_or(|s| !self.visible().contains(&s)) {
            self.selected = self.visible().first().copied();
            self.preview_page = 0;
        }
    }

    /// Cedro 4 Labs: tiene la pagina d'accordo con quello che si sta facendo davvero. Se dal menu dei
    /// formati si sceglie «Figura», o si lascia un PDF sulla pagina delle immagini, la barra lo segue.
    pub fn lab_sync(&mut self) {
        if self.lab == Lab::Home && self.activity == Activity::Convert {
            return;
        }
        self.lab = match self.activity {
            Activity::Firma => Lab::Referti,
            Activity::Transcribe => Lab::Trascrivi,
            Activity::YouTube | Activity::Web => self.lab,
            Activity::Convert => match (self.kind(), self.action.as_str(), self.lab) {
                (Kind::Image | Kind::Pdf | Kind::Doc, "strip", _) => Lab::Anonimizza,
                (Kind::Image, "figure", _) => Lab::Figura,
                (Kind::Image, _, Lab::Converti) => Lab::Converti,
                (Kind::Image, _, _) => Lab::Immagini,
                (Kind::Text, "xlsx" | "csv", Lab::Dati) => Lab::Dati,
                (Kind::Pdf, _, Lab::Converti) => Lab::Converti,
                (Kind::Pdf, _, _) => Lab::Pdf,
                _ => Lab::Converti,
            },
        };
    }

    /// Il Cedro di partenza, «Converti» con un PDF in vista: si passa al primo tipo in coda che non è un
    /// PDF (le immagini se non ce n'è).
    pub fn cedro_leave_pdf(&mut self, simple: bool) {
        // Tornando su «PDF» si ritrova la voce lasciata (Unisci, Dividi…), non quella di partenza.
        self.cedro_pdf_action = Some(self.action.clone());
        let kind = self.files.iter().find(|f| !f.voice_only && f.kind != Kind::Pdf).map(|f| f.kind).unwrap_or(Kind::Image);
        if let Some(i) = CATEGORIES.iter().position(|c| c.kind == kind) {
            self.set_category(i, simple);
        }
        self.lab = Lab::Converti;
        self.selected = self.visible().first().copied();
        self.preview_page = 0;
    }

    /// Il Cedro di partenza, tornando su «PDF»: rimette la voce che c'era quando si è andati su «Converti».
    pub fn cedro_back_to_pdf(&mut self) {
        if let Some(a) = self.cedro_pdf_action.take() {
            if self.kind() == Kind::Pdf {
                self.action = a;
            }
        }
    }

    /// Il Cedro di partenza: come `lab_sync`, con le sue sei pagine. I PDF hanno la loro pagina, gli altri
    /// file stanno in «Converti»; YouTube e gli altri siti sono «Video dal web».
    pub fn cedro_sync(&mut self) {
        if self.lab == Lab::Home && self.activity == Activity::Convert {
            return;
        }
        self.lab = match self.activity {
            Activity::Firma => Lab::Firma,
            Activity::Transcribe => Lab::Trascrivi,
            Activity::YouTube | Activity::Web => Lab::Video,
            Activity::Convert if self.kind() == Kind::Pdf => Lab::Pdf,
            Activity::Convert => Lab::Converti,
        };
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Il primo formato della categoria (in Semplificata quello consigliato).
    pub fn default_action(&self, simple: bool) -> String {
        let kind = self.kind();
        if simple {
            if let Some(f) = conversioni::simple_formats(kind).first() {
                return f.id.to_string();
            }
        }
        conversioni::formats(kind).first().map(|s| s.id.to_string()).unwrap_or_default()
    }

    /// Le scelte della Semplificata per la categoria: «Un solo PDF» solo con almeno due PDF in coda.
    pub fn simple_choices(&self) -> Vec<&'static conversioni::SimpleFormat> {
        let many = self.visible().len() >= 2;
        conversioni::simple_formats(self.kind()).iter().filter(|f| f.id != "merge" || many).collect()
    }

    /// I file che l'attività di ora prende dalla coda: la categoria scelta, oppure (trascrizione)
    /// tutto quello che ha una voce.
    pub fn visible(&self) -> Vec<usize> {
        let kind = self.kind();
        (0..self.files.len())
            .filter(|&i| {
                let f = &self.files[i];
                match self.activity {
                    Activity::Transcribe => trascrizione::transcribable(&f.path),
                    _ => !f.voice_only && f.kind == kind,
                }
            })
            .collect()
    }

    /// Quanti file della coda appartengono alle altre categorie (la vista non li mostra).
    pub fn others(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (ci, c) in CATEGORIES.iter().enumerate() {
            if ci == self.category {
                continue;
            }
            let n = self.files.iter().filter(|f| !f.voice_only && f.kind == c.kind).count();
            if n > 0 {
                out.push((ci, n));
            }
        }
        out
    }

    pub fn set_category(&mut self, index: usize, simple: bool) {
        if index < CATEGORIES.len() && index != self.category {
            // Cedro 4 Labs: su «Togli i nomi» i file di un altro tipo (dal chip «In coda anche» o appena
            // aggiunti) si anonimizzano anche loro, se quel tipo lo permette, invece di cambiare pagina.
            let strip = self.lab == Lab::Anonimizza && self.action == "strip";
            self.category = index;
            self.action = self.default_action(simple);
            if strip && (conversioni::formats(self.kind()).iter().any(|s| s.id == "strip") || self.tools().iter().any(|s| s.id == "strip")) {
                self.action = "strip".to_string();
            }
            self.selected = self.visible().first().copied();
            self.preview_page = 0;
        }
    }

    /// Cambia la voce scelta; un'opzione mai toccata riparte dal suo valore consigliato.
    pub fn set_action(&mut self, id: &str) {
        if !conversioni::is_page_action(self.kind(), &self.action) {
            self.last_convert = self.action.clone();
        }
        self.action = id.to_string();
    }

    /// Cosa fa adesso la scheda PDF.
    pub fn pdf_mode(&self) -> PdfMode {
        match self.action.as_str() {
            "merge" => PdfMode::Merge,
            "split" => PdfMode::Split,
            "pages" => PdfMode::Pages,
            "compress" => PdfMode::Compress,
            _ => PdfMode::Convert,
        }
    }

    /// I cinque pulsanti in cima alla scheda PDF. «Converti» torna al formato scelto prima.
    pub fn set_pdf_mode(&mut self, mode: PdfMode, simple: bool) {
        match mode {
            PdfMode::Merge => self.set_action("merge"),
            PdfMode::Split => self.set_action("split"),
            PdfMode::Pages => self.set_action("pages"),
            PdfMode::Compress => self.set_action("compress"),
            PdfMode::Convert if self.pdf_mode() != PdfMode::Convert => {
                let back = std::mem::take(&mut self.last_convert);
                let known = conversioni::formats(self.kind()).iter().chain(conversioni::tools(self.kind())).any(|s| s.id == back);
                self.action = if known { back } else { self.default_action(simple) };
                if conversioni::is_page_action(self.kind(), &self.action) {
                    self.action = conversioni::formats(self.kind()).first().map(|s| s.id.to_string()).unwrap_or_default();
                }
            }
            PdfMode::Convert => {}
        }
    }

    /// Sposta un file della vista di `delta` posti (l'ordine della coda è l'ordine del PDF unito).
    pub fn move_file(&mut self, index: usize, delta: i32) {
        let vis = self.visible();
        let Some(pos) = vis.iter().position(|&i| i == index) else { return };
        let to = (pos as i32 + delta).clamp(0, vis.len() as i32 - 1) as usize;
        self.move_file_to(index, to);
    }

    /// Porta un file della vista al posto `to` (da 0) della vista: gli altri scalano.
    pub fn move_file_to(&mut self, index: usize, to: usize) {
        if self.busy() {
            return;
        }
        let vis = self.visible();
        let Some(from) = vis.iter().position(|&i| i == index) else { return };
        let to = to.min(vis.len().saturating_sub(1));
        if from == to {
            return;
        }
        // Si riordinano solo i posti della vista: i file delle altre categorie restano dove sono.
        let mut order: Vec<FileItem> = vis.iter().map(|&i| self.files[i].clone()).collect();
        let moved = order.remove(from);
        order.insert(to, moved);
        let selected_path = self.selected.and_then(|s| self.files.get(s)).map(|f| f.path.clone());
        for (slot, item) in vis.iter().zip(order) {
            self.files[*slot] = item;
        }
        self.selected = selected_path.and_then(|p| self.files.iter().position(|f| f.path == p));
    }

    /// Le miniature delle pagine `pages` (da 0) di un PDF, chieste una volta sola.
    pub fn want_page_thumbs(&mut self, path: &Path, pages: impl IntoIterator<Item = usize>) {
        let missing: Vec<usize> = pages.into_iter().filter(|p| !self.page_thumbs.contains_key(&(path.to_path_buf(), *p))).collect();
        if missing.is_empty() {
            return;
        }
        for p in &missing {
            self.page_thumbs.insert((path.to_path_buf(), *p), None);
        }
        self.loader.page_thumbs(path, missing);
    }

    /// I tagli che la striscia mostra per un file: i suoi, o quelli che vengono dal modo scelto.
    pub fn shown_cuts(&self, path: &Path, total: u32) -> Vec<u32> {
        if let Some(c) = self.cuts.get(path) {
            return c.clone();
        }
        let plan = conversioni::options_from(&self.values).pdf_pages.plan;
        match crate::pdfpages::groups(&plan, total, None) {
            Ok(groups) if !matches!(plan, crate::pdfpages::Plan::Remove(_) | crate::pdfpages::Plan::Bookmarks) => {
                let mut cuts: Vec<u32> = groups.iter().filter_map(|g| g.pages.last().copied()).filter(|p| *p < total).collect();
                // Solo gruppi di fila: «1-2, 5-6» non sono tagli.
                let contiguous = groups.windows(2).all(|w| w[0].pages.last().map(|p| p + 1) == w[1].pages.first().copied());
                if !contiguous {
                    cuts.clear();
                }
                cuts
            }
            _ => Vec::new(),
        }
    }

    /// Un clic tra due pagine della striscia: mette o toglie il taglio dopo `after` (da 1) e scrive
    /// gli intervalli che ne vengono, così campo e striscia dicono la stessa cosa.
    pub fn toggle_cut(&mut self, index: usize, after: u32) {
        let Some(path) = self.files.get(index).map(|f| f.path.clone()) else { return };
        let total = self.page_count(index).unwrap_or(0) as u32;
        if total < 2 || after == 0 || after >= total {
            return;
        }
        let mut cuts = self.shown_cuts(&path, total);
        match cuts.iter().position(|c| *c == after) {
            Some(i) => {
                cuts.remove(i);
            }
            None => cuts.push(after),
        }
        cuts.sort_unstable();
        let text = crate::pdfpages::cuts_text(&cuts, total);
        self.cuts.insert(path, cuts);
        self.values.set(&conversioni::SPLIT_HOW, 2);
        self.values.set_text(&conversioni::SPLIT_RANGES, &text);
    }

    /// Il campo «Intervalli» scritto a mano vale di nuovo per il file in evidenza: i suoi tagli si tolgono.
    pub fn ranges_typed(&mut self) {
        if let Some(path) = self.selected.and_then(|i| self.files.get(i)).map(|f| f.path.clone()) {
            self.cuts.remove(&path);
        }
    }

    /// «Pagine»: le pagine del file `i` come le si è sistemate (com'erano, se non le si è toccate).
    pub fn board(&self, i: usize) -> Option<Board> {
        let path = &self.files.get(i)?.path;
        if let Some(b) = self.boards.get(path) {
            return Some(b.clone());
        }
        self.page_count(i).map(Board::new)
    }

    fn board_mut(&mut self, i: usize) -> Option<&mut Board> {
        let n = self.page_count(i)?;
        let path = self.files.get(i)?.path.clone();
        Some(self.boards.entry(path).or_insert_with(|| Board::new(n)))
    }

    /// Un clic su una pagina: la sceglie o la toglie dalla scelta.
    pub fn page_click(&mut self, i: usize, pos: usize) {
        if self.busy() {
            return;
        }
        let Some(b) = self.board_mut(i) else { return };
        if pos >= b.pages.len() {
            return;
        }
        match b.sel.iter().position(|&s| s == pos) {
            Some(k) => {
                b.sel.remove(k);
            }
            None => b.sel.push(pos),
        }
        b.sel.sort_unstable();
        // L'anteprima con numeri e filigrana va sulla pagina toccata, se resta.
        if let Some(b) = self.board(i) {
            if !b.pages[pos].2 {
                self.stamp_page = b.pages[..pos].iter().filter(|p| !p.2).count();
            }
        }
    }

    /// Tutte le pagine scelte, o nessuna se lo erano già tutte.
    pub fn page_select_all(&mut self, i: usize) {
        let Some(b) = self.board_mut(i) else { return };
        b.sel = if b.sel.len() == b.pages.len() { Vec::new() } else { (0..b.pages.len()).collect() };
    }

    /// Gira le pagine scelte di 90° (`dir` 1 orario, -1 antiorario).
    pub fn pages_rotate(&mut self, i: usize, dir: i32) {
        if self.busy() {
            return;
        }
        let Some(b) = self.board_mut(i) else { return };
        for &s in &b.sel {
            if let Some(p) = b.pages.get_mut(s) {
                p.1 = (p.1 + 90 * dir).rem_euclid(360);
            }
        }
    }

    /// Toglie le pagine scelte; se lo erano già tutte, le rimette.
    pub fn pages_remove(&mut self, i: usize) {
        if self.busy() {
            return;
        }
        let Some(b) = self.board_mut(i) else { return };
        let all_gone = !b.sel.is_empty() && b.sel.iter().all(|&s| b.pages.get(s).is_some_and(|p| p.2));
        for &s in &b.sel.clone() {
            if let Some(p) = b.pages.get_mut(s) {
                p.2 = !all_gone;
            }
        }
        // Non si tolgono tutte: un PDF senza pagine non esiste.
        if b.pages.iter().all(|p| p.2) {
            for &s in &b.sel.clone() {
                b.pages[s].2 = false;
            }
            self.notice = Some(Notice { text: "Deve restare almeno una pagina.".into(), error: true });
        }
    }

    /// Sposta la pagina dal posto `from` al posto `to` (trascinata): la scelta la segue.
    pub fn pages_move(&mut self, i: usize, from: usize, to: usize) {
        if self.busy() {
            return;
        }
        let Some(b) = self.board_mut(i) else { return };
        let n = b.pages.len();
        if from >= n || from == to.min(n - 1) {
            return;
        }
        let to = to.min(n - 1);
        let chosen: Vec<(u32, i32, bool)> = b.sel.iter().map(|&s| b.pages[s]).collect();
        let moved = b.pages.remove(from);
        b.pages.insert(to, moved);
        b.sel = b.pages.iter().enumerate().filter(|(_, p)| chosen.contains(p)).map(|(k, _)| k).collect();
    }

    /// Torna alle pagine com'erano.
    pub fn pages_reset(&mut self, i: usize) {
        if let Some(path) = self.files.get(i).map(|f| f.path.clone()) {
            self.boards.remove(&path);
        }
    }

    /// La chiave delle scelte di «Comprimi» (per le stime).
    pub fn squeeze_key(&self) -> String {
        let opts = conversioni::options_from(&self.values);
        format!("{:?}|{:?}", opts.pdf_edit.squeeze, opts.pdf_pages.open_password)
    }

    /// La stima del peso compresso del file `i` con le scelte di adesso: `None` se non c'è ancora.
    pub fn estimate(&self, i: usize) -> Option<&Result<(u64, u64), String>> {
        let path = self.files.get(i)?.path.clone();
        self.estimates.get(&(path, self.squeeze_key()))?.as_ref()
    }

    /// Chiede le stime che mancano per i PDF della vista (una alla volta, in un thread a parte).
    pub fn want_estimates(&mut self) {
        let key = self.squeeze_key();
        let opts = conversioni::options_from(&self.values);
        let squeeze = opts.pdf_edit.squeeze.unwrap_or_else(engine::saved_squeeze);
        let password = opts.pdf_pages.open_password.clone();
        for i in self.visible() {
            let path = self.files[i].path.clone();
            let k = (path.clone(), key.clone());
            if !self.estimates.contains_key(&k) {
                self.estimates.insert(k, None);
                self.pdfwork.estimate(&path, squeeze, password.clone(), &key);
            }
        }
        // Le stime ancora in corso con un'altra chiave non arriveranno più: si tolgono, così si richiedono se si torna lì.
        self.estimates.retain(|(_, k), v| v.is_some() || *k == key);
    }

    /// Le scelte di «Pagine» per il file `i`: pagine, numeri, filigrana.
    pub fn stamp_key(&self, i: usize) -> Option<String> {
        let file = self.files.get(i)?;
        let b = self.board(i)?;
        let opts = conversioni::options_from(&self.values);
        Some(format!("{}|{:?}|{:?}|{}|{:?}", file.path.display(), b.kept(), opts.pdf_edit.stamp, self.stamp_page, opts.pdf_pages.open_password))
    }

    /// Chiede la pagina vera con numeri e filigrana per il file `i`, se le scelte sono cambiate.
    pub fn want_stamp_preview(&mut self, i: usize) {
        let opts = conversioni::options_from(&self.values);
        if opts.pdf_edit.stamp.is_empty() {
            return;
        }
        let Some(b) = self.board(i) else { return };
        let kept = b.kept();
        self.stamp_page = self.stamp_page.min(kept.len().saturating_sub(1));
        let Some(key) = self.stamp_key(i) else { return };
        if self.stamp_view.0 == key {
            return;
        }
        self.stamp_view.0 = key.clone();
        let path = self.files[i].path.clone();
        self.pdfwork.stamp(&path, kept, opts.pdf_edit.stamp, self.stamp_page, opts.pdf_pages.open_password, &key);
    }

    /// «Estrai»: le pagine scelte del file `i` in un PDF nuovo (girate come si vedono, senza numeri né filigrana).
    pub fn pages_extract(&mut self, i: usize) {
        if self.busy() {
            return;
        }
        let Some(b) = self.board(i) else { return };
        let list: Vec<(u32, i32)> = b.sel.iter().filter_map(|&s| b.pages.get(s)).map(|p| (p.0, p.1)).collect();
        if list.is_empty() {
            self.notice = Some(Notice { text: "Scegli prima le pagine da estrarre.".into(), error: false });
            return;
        }
        let path = self.files[i].path.clone();
        let cancel = self.begin(&[i], JobKind::Convert);
        let dest = self.dest.clone();
        let mut opts = self.exists_options(conversioni::options_from(&self.values));
        opts.pdf_edit.pages = list;
        opts.pdf_edit.stamp = Default::default();
        opts.pdf_edit.suffix = "-estratte".into();
        let quality = self.quality;
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let result = match crate::resources::acquire(Some(&cancel)) {
                Some(_turn) => {
                    let _ = tx.send(Event::Started(path.clone()));
                    wake();
                    guarded(|| conversioni::convert_one_out(&path, "pages", quality, dest.as_deref(), &opts))
                }
                None => Err("Annullato".into()),
            };
            let _ = tx.send(Event::Finished(path, result));
            wake();
            let _ = tx.send(Event::JobDone);
            wake();
        });
    }

    /// Il nome di ciò che si sta per fare, per il pulsante («WebP», «Comprimi»…).
    /// Il nome dell'azione scelta come lo dice il menu (per esempio «Ruota di 90° a destra»).
    pub fn action_label(&self) -> &'static str {
        self.voce(&self.action).label
    }

    /// L'iconcina colorata dell'azione scelta.
    pub fn action_voce(&self) -> super::voci::Voce {
        self.voce(&self.action)
    }

    /// La voce del menu per l'azione `id` con i file in coda: «Togli i dati nascosti» sui vetrini
    /// digitali è la copia anonima del vetrino, non il GPS delle foto.
    pub fn voce(&self, id: &str) -> super::voci::Voce {
        let mut v = super::voci::voce(self.kind(), id);
        if id == "strip" && self.kind() == Kind::Image {
            let immagini: Vec<&Path> = self.files.iter().filter(|f| f.kind == Kind::Image).map(|f| f.path.as_path()).collect();
            let vetrini = immagini.iter().filter(|p| self.media_of(p).is_some_and(|m| m.slide.is_some())).count();
            if vetrini > 0 && vetrini == immagini.len() {
                v.label = "Copia anonima del vetrino";
                v.hint = "via etichetta, foto del vetrino, nomi e date";
            } else if vetrini > 0 {
                v.label = "Togli nomi e dati nascosti";
                v.hint = "vetrini: copia anonima · foto: GPS e data";
            }
        }
        v
    }

    pub fn option_defs(&self) -> Vec<&'static OptionDef> {
        let mut defs = conversioni::option_defs(self.kind(), &self.action);
        // Con un vetrino in coda il riquadro «Vetrino» va in cima.
        if self.kind() == Kind::Image && self.activity == Activity::Convert && self.first_slide().is_some() {
            let mut all = conversioni::slide_option_defs(&self.action);
            all.append(&mut defs);
            defs = all;
        }
        // Con un DICOM in coda il riquadro «DICOM» (finestra, serie, anonimizza) va in cima.
        if self.kind() == Kind::Image && self.activity == Activity::Convert && self.first_dicom().is_some() {
            let mut all = conversioni::dicom_option_defs(&self.action);
            all.retain(|d| !defs.iter().any(|x| x.opt == d.opt));
            all.append(&mut defs);
            defs = all;
        }
        // Con un'immagine di microscopia: canali, piani Z, contrasto e barra di scala, in cima.
        if self.kind() == Kind::Image && self.activity == Activity::Convert {
            if let Some((m, _)) = self.first_micro() {
                let mut all = conversioni::micro_option_defs(&self.action, m.canali, m.z, m.t);
                all.retain(|d| !defs.iter().any(|x| x.opt == d.opt));
                all.append(&mut defs);
                defs = all;
            }
        }
        defs.into_iter().filter(|d| conversioni::shown(d, &self.values)).collect()
    }

    /// La prima immagine di microscopia della coda, con quante ce ne sono.
    pub fn first_micro(&self) -> Option<(&media::MicroInfo, usize)> {
        let mut found = self.files.iter().filter_map(|f| self.media_of(&f.path).and_then(|m| m.micro.as_ref()));
        let first = found.next()?;
        Some((first, 1 + found.count()))
    }

    /// Il primo DICOM della coda, con quanti ce ne sono.
    pub fn first_dicom(&self) -> Option<(&media::DicomInfo, usize)> {
        let mut found = self.files.iter().filter_map(|f| self.media_of(&f.path).and_then(|m| m.dicom.as_ref()));
        let first = found.next()?;
        Some((first, 1 + found.count()))
    }

    /// Gli strumenti del pannello: quelli dei vetrini solo se in coda c'è un vetrino.
    pub fn tools(&self) -> Vec<crate::engine::Slice> {
        let vetrini = self.first_slide().is_some();
        let dicom = self.first_dicom().is_some();
        conversioni::tools(self.kind())
            .iter()
            .filter(|s| vetrini || !crate::engine::SOLO_VETRINI.contains(&s.id))
            .filter(|s| dicom || !crate::engine::SOLO_DICOM.contains(&s.id))
            .copied()
            .collect()
    }

    /// Il primo vetrino digitale (.svs) della coda, con quanti ce ne sono.
    pub fn first_slide(&self) -> Option<(&media::SlideInfo, usize)> {
        let mut found = self.files.iter().filter_map(|f| self.media_of(&f.path).and_then(|m| m.slide.as_ref()));
        let first = found.next()?;
        Some((first, 1 + found.count()))
    }

    /// Vero se le opzioni scelte non sono quelle consigliate (la Semplificata lo dice).
    pub fn customized(&self) -> bool {
        self.option_defs().iter().any(|d| match d.control {
            conversioni::Control::Text { .. } | conversioni::Control::Password => !self.values.text(d).is_empty(),
            _ => self.values.get(d) != d.default,
        }) || self.quality != engine::read_quality()
    }

    /// Torna ai valori consigliati (comando esplicito, non un effetto del cambio di modalità).
    pub fn reset_recommended(&mut self) {
        let exists = self.values.exists;
        self.values = Values::new();
        self.values.exists = exists;
        self.quality = engine::read_quality();
    }

    /// Aggiunge file (e il contenuto delle cartelle, anche delle sottocartelle) alla coda. Se i file nuovi sono
    /// di un'altra categoria, la vista passa a quella: altrimenti resterebbero invisibili.
    pub fn add_files(&mut self, paths: Vec<PathBuf>, simple: bool) -> usize {
        let mut expanded = Vec::new();
        for p in paths {
            if p.is_dir() {
                expanded.extend(crate::cartelle::file_in_cartella(&p, 20_000));
            } else if p.is_file() {
                expanded.push(p);
            }
        }
        let mut first: Option<(Kind, bool)> = None;
        let mut added = 0;
        let transcribing = self.activity == Activity::Transcribe;
        // I doppioni si cercano in un insieme: con migliaia di file il confronto uno a uno pesa.
        let mut known: std::collections::HashSet<PathBuf> = self.files.iter().map(|f| f.path.clone()).collect();
        for path in expanded {
            let voice_only = trascrizione::only_transcribable(&path);
            let kind = engine::kind_of(&path);
            if (kind == Kind::Other && !voice_only) || known.contains(&path) {
                continue;
            }
            known.insert(path.clone());
            first.get_or_insert((kind, voice_only || (transcribing && trascrizione::transcribable(&path))));
            self.loader.request(&path);
            self.files.push(FileItem { path, kind, voice_only, state: FileState::Waiting, message: "In attesa".into(), output: None, checked: true, done_key: None, percent: None, pages: String::new() });
            added += 1;
        }
        match first {
            Some((_, true)) => self.activity = Activity::Transcribe,
            Some((kind, false)) => {
                if self.activity != Activity::Convert && self.activity != Activity::Transcribe {
                    self.activity = Activity::Convert;
                }
                if self.activity == Activity::Convert && self.kind() != kind {
                    if let Some(i) = CATEGORIES.iter().position(|c| c.kind == kind) {
                        self.set_category(i, simple);
                    }
                }
            }
            None => {}
        }
        if self.selected.is_none_or(|s| !self.visible().contains(&s)) {
            self.selected = self.visible().first().copied();
        }
        if added == 0 && first.is_none() {
            self.notice = Some(Notice { text: "Nessun file che Cedro sappia convertire.".into(), error: false });
        } else {
            self.notice = None;
        }
        added
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.files.len() && !self.busy() {
            self.files.remove(index);
            self.selected = match self.selected {
                Some(s) if s == index => self.visible().first().copied(),
                Some(s) if s > index => Some(s - 1),
                other => other,
            };
            self.prune_media();
        }
    }

    /// Dimentica miniature, fotogrammi e pagine dei file non più in coda: la memoria non cresce a ogni lotto.
    fn prune_media(&mut self) {
        let live: std::collections::HashSet<&Path> = self.files.iter().map(|f| f.path.as_path()).collect();
        self.media.retain(|p, _| live.contains(p.as_path()));
        self.previews.retain(|(p, _), _| live.contains(p.as_path()));
        self.page_thumbs.retain(|(p, _), _| live.contains(p.as_path()));
        self.cuts.retain(|p, _| live.contains(p.as_path()));
    }

    /// Toglie dalla coda i file spuntati della vista (o tutti quelli della vista se nessuno è spuntato).
    pub fn remove_checked(&mut self) {
        if self.busy() {
            return;
        }
        let visible = self.visible();
        let any = visible.iter().any(|&i| self.files[i].checked);
        let mut keep = Vec::new();
        for (i, f) in self.files.drain(..).enumerate() {
            if !(visible.contains(&i) && (f.checked || !any)) {
                keep.push(f);
            }
        }
        self.files = keep;
        self.selected = self.visible().first().copied();
        self.prune_media();
    }

    pub fn toggle_checked(&mut self, index: usize) {
        if let Some(f) = self.files.get_mut(index) {
            f.checked = !f.checked;
        }
    }

    pub fn toggle_all(&mut self) {
        let visible = self.visible();
        let all = visible.iter().all(|&i| self.files[i].checked);
        for i in visible {
            self.files[i].checked = !all;
        }
    }

    /// I file che il pulsante principale lavorerebbe adesso.
    pub fn targets(&self) -> Vec<usize> {
        let visible = self.visible();
        // «Pagine» lavora sul PDF che è sul banco, uno alla volta.
        if self.activity == Activity::Convert && self.kind() == Kind::Pdf && self.action == "pages" && !self.simple_view {
            return self.selected.filter(|i| visible.contains(i)).or(visible.first().copied()).into_iter().collect();
        }
        visible.into_iter().filter(|&i| self.files[i].checked).collect()
    }

    /// Le scelte che cambiano il risultato: se cambiano, i file già fatti si rifanno.
    fn settings_key(&self) -> String {
        // Per unire contano anche l'ordine dei file e le loro pagine; per dividere i tagli.
        let pages = if conversioni::is_page_action(self.kind(), &self.action) && self.activity == Activity::Convert {
            let vis = self.visible();
            let order: Vec<String> = vis.iter().map(|&i| format!("{}#{}", self.files[i].path.display(), self.files[i].pages)).collect();
            let mut cuts: Vec<String> = self.cuts.iter().map(|(p, c)| format!("{}:{c:?}", p.display())).collect();
            cuts.sort();
            let mut boards: Vec<String> = self.boards.iter().filter(|(_, b)| !b.untouched()).map(|(p, b)| format!("{}:{:?}", p.display(), b.kept())).collect();
            boards.sort();
            format!("{order:?}{cuts:?}{boards:?}")
        } else {
            String::new()
        };
        format!(
            "{:?}|{}|{}|{}|{:?}|{:?}|{}|{pages}",
            self.activity,
            self.category,
            self.action,
            self.quality,
            self.dest,
            self.values,
            format!("{:?}{}", self.transcribe.format, self.transcribe.model)
        )
    }

    /// Il file è già fatto con le scelte di adesso.
    fn is_fresh_done(&self, i: usize) -> bool {
        matches!(self.files[i].state, FileState::Done | FileState::Skipped) && self.files[i].done_key.as_deref() == Some(self.settings_key().as_str())
    }

    /// I file che il pulsante lavorerebbe adesso, senza quelli già fatti con le stesse scelte.
    pub fn todo(&self) -> Vec<usize> {
        self.targets().into_iter().filter(|&i| !self.is_fresh_done(i)).collect()
    }

    /// Il lavoro è finito e non c'è altro da fare: il pulsante diventa «Apri risultato».
    pub fn finished(&self) -> bool {
        matches!(self.activity, Activity::Convert | Activity::Transcribe)
            && !self.busy()
            && self.todo().is_empty()
            && self.targets().iter().any(|&i| self.files[i].output.is_some())
    }

    /// Cosa apre «Apri risultato»: il file, o la cartella se i file sono più di uno.
    pub fn result_path(&self) -> Option<PathBuf> {
        let mut outs: Vec<&PathBuf> = self.targets().into_iter().filter_map(|i| self.files[i].output.as_ref()).collect();
        // Il PDF unito è lo stesso per tutti i file; i pezzi di un PDF diviso stanno nella cartella.
        outs.dedup();
        if self.action == "split" && self.activity == Activity::Convert {
            return outs.first().and_then(|o| o.parent()).map(|d| d.to_path_buf());
        }
        match outs.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            [first, ..] => first.parent().map(|d| d.to_path_buf()),
        }
    }

    /// «Ricomincia»: svuota la lista.
    pub fn restart(&mut self) {
        if self.busy() {
            return;
        }
        let gone = self.targets();
        let mut i = 0;
        self.files.retain(|_| {
            let keep = !gone.contains(&i);
            i += 1;
            keep
        });
        self.selected = None;
        self.notice = None;
        self.prune_media();
    }

    /// Il tempo che resta al file in trascrizione, in parole («circa 4 min»): solo dopo un po' di lavoro
    /// fatto, perché le prime percentuali (modello che si scalda) darebbero stime a caso.
    pub fn transcribe_left(&self) -> Option<String> {
        let (t0, p0) = self.transcribe.since?;
        let p = self.transcribe.percent;
        let spent = t0.elapsed().as_secs_f32();
        if p >= 100 || p < p0.saturating_add(5) || spent < 10.0 {
            return None;
        }
        let rate = (p - p0) as f32 / spent;
        Some(time_left((100 - p) as f32 / rate))
    }

    /// La spia: cosa accelera il lavoro in corso (solo conversioni e trascrizioni).
    pub fn spia(&self) -> Option<crate::accel::InUse> {
        match self.job.as_ref().map(|j| j.kind) {
            Some(JobKind::Convert | JobKind::Transcribe) => crate::accel::in_use(),
            _ => None,
        }
    }

    pub fn media_of(&self, path: &Path) -> Option<&media::Loaded> {
        self.media.get(path)
    }

    /// Chiede una volta sola le informazioni di un file che non è in coda (il documento del visualizzatore).
    pub fn want_media(&mut self, path: &Path) {
        if !self.media.contains_key(path) && self.media_asked.insert(path.to_path_buf()) {
            self.loader.request(path);
        }
    }

    /// Chiede di disegnare una pagina del documento per l'anteprima (fuori dal thread della finestra).
    pub fn want_preview(&mut self, path: &Path, page: usize) {
        let key = (path.to_path_buf(), page);
        if !self.previews.contains_key(&key) {
            // Il caricatore tiene in coda solo l'ultima richiesta: le altre ancora vuote sono state annullate.
            self.previews.retain(|_, v| v.is_some());
            self.previews.insert(key.clone(), None);
            // Si tengono poche pagine: l'anteprima non deve riempire la memoria. Via per prime quelle di altri
            // documenti e le pagine più lontane da quella richiesta.
            if self.previews.len() > 8 {
                let mut old: Vec<(bool, usize, (PathBuf, usize))> = self
                    .previews
                    .iter()
                    .filter(|(k, v)| **k != key && v.is_some())
                    .map(|(k, _)| (k.0 == path, k.1.abs_diff(page), k.clone()))
                    .collect();
                old.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
                let extra = self.previews.len() - 8;
                for (_, _, k) in old.into_iter().take(extra) {
                    self.previews.remove(&k);
                }
            }
            let previews = &self.previews;
            self.previews_failed.retain(|k| previews.contains_key(k));
            self.loader.preview(path, page);
        }
    }

    /// Vero mentre la pagina dell'anteprima è chiesta e non ancora arrivata (falso se non si è potuta disegnare).
    pub fn preview_loading(&self, path: &Path, page: usize) -> bool {
        let key = (path.to_path_buf(), page);
        matches!(self.previews.get(&key), Some(None)) && !self.previews_failed.contains(&key)
    }

    /// Il testo dell'errore se le scelte scritte a mano non vanno (per esempio le pagine).
    pub fn validate(&self) -> Result<(), String> {
        if !(self.simple_view && self.action == "split") {
            conversioni::validate(&self.values, self.kind(), &self.action)?;
        }
        if self.kind() == Kind::Image && self.action == "rx" && !self.targets().iter().any(|&i| crate::rx::e_dicom(&self.files[i].path)) {
            return Err("Spunta almeno una radiografia DICOM.".into());
        }
        if self.kind() == Kind::Image && self.action == "figure" && self.targets().len() > 26 {
            return Err("Una figura tiene al massimo 26 pannelli (dalla A alla Z).".into());
        }
        if self.kind() == Kind::Pdf && self.action == "merge" {
            let targets = self.targets();
            if targets.len() < 2 {
                return Err("Per unire servono almeno due PDF spuntati.".into());
            }
            for &i in &targets {
                let f = &self.files[i];
                let sel = engine::PageSel::parse(&f.pages).map_err(|e| format!("{}: {e}", f.name()))?;
                if let (Some(max), Some(n)) = (sel.and_then(|s| s.max_page()), self.page_count(i)) {
                    if max > n {
                        return Err(format!("{} ha {n} pagine: la pagina {max} non c'è.", f.name()));
                    }
                }
            }
        }
        if self.option_defs().iter().any(|d| d.opt == conversioni::Opt::PageRange) {
            // Le pagine chieste devono esserci in ogni PDF della coda.
            if let Ok(Some(sel)) = engine::PageSel::parse(self.values_text(conversioni::Opt::PageRange)) {
                for &i in &self.targets() {
                    let f = &self.files[i];
                    if let Some(media::Loaded { pages: Some(n), .. }) = self.media.get(&f.path) {
                        if let Some(max) = sel.max_page() {
                            if max > *n {
                                return Err(format!("{} ha {n} pagine: la pagina {max} non c'è.", f.name()));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn values_text(&self, opt: conversioni::Opt) -> &str {
        self.option_defs().into_iter().find(|d| d.opt == opt).map(|d| self.values.text(d)).unwrap_or("")
    }

    fn exists_options(&self, mut opts: engine::Options) -> engine::Options {
        if opts.exists == Exists::Ask {
            let tx = self.tx.clone();
            let wake = self.wake.clone();
            opts.on_exists = Some(ExistsAsk::new(move |path| {
                let (reply, rx) = channel();
                let _ = tx.send(Event::Ask(path.to_path_buf(), reply));
                wake();
                rx.recv().unwrap_or(ExistsChoice::KeepBoth)
            }));
        }
        opts
    }

    /// Il pulsante principale dell'attività di ora.
    pub fn start(&mut self) {
        match self.activity {
            Activity::Convert => self.start_convert(),
            Activity::Transcribe => self.ask_voices(),
            Activity::YouTube => self.start_links(false),
            Activity::Web => self.start_links(true),
            Activity::Firma => {}
        }
    }

    /// Ferma il lavoro dove si può (i download; le conversioni finiscono il file in corso).
    pub fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn begin(&mut self, files: &[usize], kind: JobKind) -> Arc<AtomicBool> {
        for &i in files {
            let f = &mut self.files[i];
            f.state = FileState::Waiting;
            f.message = "In attesa".into();
            f.output = None;
            f.done_key = None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job { kind, total: files.len(), done: 0, failed: 0, cancel: cancel.clone(), key: self.settings_key(), targets: files.to_vec(), part: 0.0, file_started: None });
        self.notice = None;
        cancel
    }

    fn start_convert(&mut self) {
        if self.busy() {
            return;
        }
        if let Err(e) = self.validate() {
            self.notice = Some(Notice { text: e, error: true });
            return;
        }
        let targets = self.todo();
        if targets.is_empty() {
            self.notice = Some(Notice { text: "Aggiungi prima qualche file.".into(), error: false });
            return;
        }
        if self.kind() == Kind::Image && self.action == "figure" {
            let cal = self.values.text(&conversioni::FIG_CALIBRATION).trim().to_string();
            let mut st = crate::read_settings();
            if st.calibrazione_10x != cal {
                st.calibrazione_10x = cal;
                let _ = crate::settings::write_settings(&st);
            }
        }
        if self.kind() == Kind::Image && self.action == "rx" {
            let clinica = self.values.text(&conversioni::RX_CLINIC).trim().to_string();
            let mut st = crate::read_settings();
            if st.nome_clinica != clinica {
                st.nome_clinica = clinica;
                let _ = crate::settings::write_settings(&st);
            }
        }
        if (self.kind() == Kind::Pdf && self.action == "merge") || (self.kind() == Kind::Image && matches!(self.action.as_str(), "figure" | "rx")) {
            self.start_merge();
            return;
        }
        let paths: Vec<PathBuf> = targets.iter().map(|&i| self.files[i].path.clone()).collect();
        // «Comprimi»: il livello scelto diventa quello della ruota.
        if self.action == "compress" && !self.simple_view {
            if let Some(sq) = conversioni::options_from(&self.values).pdf_edit.squeeze {
                let mut st = crate::read_settings();
                if st.compress != sq.level {
                    st.compress = sq.level;
                    let _ = crate::settings::write_settings(&st);
                }
            }
        }
        // «Dividi»: chi ha i suoi tagli sulla striscia si divide lì, gli altri come dice il pannello.
        let cuts: HashMap<PathBuf, Vec<u32>> = if self.action == "split" && !self.simple_view { self.cuts.clone() } else { HashMap::new() };
        // «Pagine»: chi ha le pagine sistemate sul banco le salva così, gli altri tutte.
        let boards: HashMap<PathBuf, Vec<(u32, i32)>> =
            if self.action == "pages" { self.boards.iter().filter(|(_, b)| !b.untouched()).map(|(p, b)| (p.clone(), b.kept())).collect() } else { HashMap::new() };
        let cancel = self.begin(&targets, JobKind::Convert);
        let action = self.action.clone();
        let quality = self.quality;
        let dest = self.dest.clone();
        let mut opts = self.exists_options(conversioni::options_from(&self.values));
        if self.simple_view && action == "split" {
            opts.pdf_pages.plan = crate::pdfpages::Plan::EveryPage;
        }
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            for path in paths {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(Event::Finished(path, Err("Annullato".into())));
                    continue;
                }
                // «In corso» solo quando il lavoro parte davvero: se un altro lavoro occupa il turno, il file resta «In attesa».
                let result = match crate::resources::acquire(Some(&cancel)) {
                    Some(_turn) => {
                        let _ = tx.send(Event::Started(path.clone()));
                        wake();
                        crate::accel::clear_in_use();
                        let (tx, wake, file, last) = (tx.clone(), wake.clone(), path.clone(), std::cell::Cell::new(-1i32));
                        // Un messaggio per ogni punto percentuale in più, non uno per fotogramma.
                        crate::progress::con(
                            move |p| {
                                let n = (p * 100.0) as i32;
                                if n != last.replace(n) {
                                    let _ = tx.send(Event::ConvertPercent(file.clone(), p));
                                    wake();
                                }
                            },
                            || {
                                let mut opts = opts.clone();
                                if let Some(c) = cuts.get(&path) {
                                    opts.pdf_pages.plan = crate::pdfpages::Plan::Cuts(c.clone());
                                }
                                if let Some(list) = boards.get(&path) {
                                    opts.pdf_edit.pages = list.clone();
                                }
                                guarded(|| conversioni::convert_one_out(&path, &action, quality, dest.as_deref(), &opts))
                            },
                        )
                    }
                    None => Err("Annullato".into()),
                };
                let _ = tx.send(Event::Finished(path, result));
                wake();
            }
            let _ = tx.send(Event::JobDone);
            wake();
        });
    }

    /// «Unisci» e «Figura»: un lavoro solo per tutti i file spuntati, nell'ordine della coda; ogni file
    /// riceve lo stesso risultato.
    fn start_merge(&mut self) {
        let action = self.action.clone();
        // Anche quelli già fatti: il PDF unito si rifà sempre con tutti.
        let targets = self.targets();
        let sources: Vec<(PathBuf, Option<engine::PageSel>)> = targets.iter().map(|&i| (self.files[i].path.clone(), engine::PageSel::parse(&self.files[i].pages).ok().flatten())).collect();
        let cancel = self.begin(&targets, JobKind::Convert);
        let dest = self.dest.clone();
        let opts = self.exists_options(conversioni::options_from(&self.values));
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let paths: Vec<PathBuf> = sources.iter().map(|(p, _)| p.clone()).collect();
            let result = match crate::resources::acquire(Some(&cancel)) {
                Some(_turn) => {
                    for p in &paths {
                        let _ = tx.send(Event::Started(p.clone()));
                    }
                    wake();
                    let (tx, wake, file, last) = (tx.clone(), wake.clone(), paths[0].clone(), std::cell::Cell::new(-1i32));
                    crate::progress::con(
                        move |p| {
                            let n = (p * 100.0) as i32;
                            if n != last.replace(n) {
                                let _ = tx.send(Event::ConvertPercent(file.clone(), p));
                                wake();
                            }
                        },
                        || {
                            guarded(|| {
                                // «Accanto all'originale» vuol dire accanto al primo file.
                                let beside = sources[0].0.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
                                let out_dir = Some(dest.as_deref().unwrap_or(&beside));
                                let done = match action.as_str() {
                                    "figure" => engine::figure_with(&paths, out_dir, &opts),
                                    "rx" => engine::rx_with(&paths, out_dir, &opts),
                                    _ => engine::merge_with(&sources, out_dir, &opts),
                                };
                                done.map(|o| (o.path, o.message)).map_err(|e| e.0)
                            })
                        },
                    )
                }
                None => Err("Annullato".into()),
            };
            for p in paths {
                let _ = tx.send(Event::Finished(p, result.clone()));
            }
            wake();
            let _ = tx.send(Event::JobDone);
            wake();
        });
    }

    /// Prima di trascrivere si chiede quante persone parlano (si parte da Automatico).
    fn ask_voices(&mut self) {
        if self.busy() {
            return;
        }
        if self.todo().is_empty() {
            self.notice = Some(Notice { text: "Aggiungi prima un audio o un video.".into(), error: false });
            return;
        }
        self.transcribe.asking = true;
    }

    /// La risposta della finestrella: si trascrive con quel numero di voci.
    pub fn voices_go(&mut self) {
        if std::mem::take(&mut self.transcribe.asking) {
            self.start_transcribe();
        }
    }

    pub fn start_transcribe(&mut self) {
        if self.busy() {
            return;
        }
        let targets = self.todo();
        if targets.is_empty() {
            self.notice = Some(Notice { text: "Aggiungi prima un audio o un video.".into(), error: false });
            return;
        }
        let paths: Vec<PathBuf> = targets.iter().map(|&i| self.files[i].path.clone()).collect();
        let cancel = self.begin(&targets, JobKind::Transcribe);
        self.transcribe.log.clear();
        self.transcribe.percent = 0;
        self.transcribe.since = None;
        let settings = crate::read_settings();
        let request = trascrizione::Request { format: self.transcribe.format, model: self.transcribe.model, gpu: settings.gpu, avx512: settings.avx512, speakers: self.transcribe.speakers.min(trascrizione::MAX_VOICES as u8), glossario: crate::settings::Settings { glossario_vet: self.transcribe.glossario_vet, ..settings.clone() }.glossario() };
        let dest = self.dest.clone();
        let opts = self.exists_options(conversioni::options_from(&self.values));
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let send = |e: Event| {
                let _ = tx.send(e);
                wake();
            };
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                trascrizione::run_cancellable(&paths, &request, dest.as_deref(), &opts, &cancel, &|event| match event {
                    trascrizione::Event::Log(line) => send(Event::TranscribeLog(line)),
                    trascrizione::Event::Percent { index, percent } => send(Event::TranscribePercent(index, percent)),
                    trascrizione::Event::Finished { path, result } => send(Event::Finished(path, result)),
                })
            }));
            if let Err(p) = run {
                send(Event::Aborted(panic_text(&*p)));
            }
            send(Event::JobDone);
        });
    }

    pub fn links(&self, web: bool) -> &Links {
        if web {
            &self.web
        } else {
            &self.youtube
        }
    }

    pub fn links_mut(&mut self, web: bool) -> &mut Links {
        if web {
            &mut self.web
        } else {
            &mut self.youtube
        }
    }

    /// Aggiunge i link scritti nel campo; se non ce ne sono, spiega perché.
    pub fn add_typed_links(&mut self, web: bool) {
        let l = self.links_mut(web);
        let typed = std::mem::take(&mut l.typed);
        if typed.trim().is_empty() {
            return;
        }
        if l.add(&typed) > 0 {
            l.note.clear();
        } else {
            let note = if l.items.iter().any(|r| typed.contains(&r.url)) { "Questo link è già in coda.".to_string() } else { youtube::link_note(&typed, true).to_string() };
            l.note = note.clone();
            l.typed = typed;
            self.notice = Some(Notice { text: note, error: true });
        }
    }

    fn start_links(&mut self, web: bool) {
        if self.busy() {
            if matches!(self.job.as_ref().map(|j| j.kind), Some(JobKind::Download { .. })) {
                self.cancel();
            }
            return;
        }
        self.add_typed_links(web);
        let l = self.links_mut(web);
        if l.items.is_empty() {
            l.note = if web { "Incolla prima il link di una pagina con un video.".into() } else { "Incolla prima un link di YouTube.".into() };
            return;
        }
        for r in l.items.iter_mut() {
            r.state = FileState::Waiting;
            r.message = "In attesa".into();
            r.percent = 0.0;
        }
        l.progress = 0.0;
        l.note.clear();
        let req = l.request();
        let total = req.urls.len();
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job { kind: JobKind::Download { web }, total, done: 0, failed: 0, cancel: cancel.clone(), key: String::new(), targets: Vec::new(), part: 0.0, file_started: None });
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Keep the updater out through download, conversion and temporary-file cleanup.
                let Some(_job) = crate::resources::acquire(Some(&cancel)) else {
                    let message = "Annullato".to_string();
                    let _ = tx.send(Event::Aborted(message.clone()));
                    return Err(message);
                };
                youtube::run(
                    &req,
                    &mut |e| {
                        let _ = tx.send(Event::Link(web, e));
                        wake();
                    },
                    &cancel,
                )
            }));
            let result = run.unwrap_or_else(|p| {
                let msg = panic_text(&*p);
                let _ = tx.send(Event::Aborted(msg.clone()));
                Err(msg)
            });
            let _ = tx.send(Event::LinksFinished(web, result));
            let _ = tx.send(Event::JobDone);
            wake();
        });
    }

    /// La risposta alla domanda «il file esiste già».
    pub fn answer(&mut self, choice: ExistsChoice) {
        if let Some(ask) = self.ask.take() {
            let _ = ask.reply.send(choice);
        }
    }

    /// Applica gli eventi arrivati dai thread. Vero se qualcosa è cambiato.
    pub fn pump(&mut self) -> bool {
        let mut changed = self.firma.pump();
        if let Some(t) = self.firma.sign.reminder.take() {
            self.notice = Some(Notice { text: t, error: true });
            changed = true;
        }
        while let Ok(e) = self.rx.try_recv() {
            changed = true;
            match e {
                Event::Started(path) => {
                    if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
                        f.state = FileState::Running;
                        f.message = "In corso".into();
                        f.percent = None;
                    }
                    if let Some(job) = self.job.as_mut() {
                        job.part = 0.0;
                        job.file_started = Some(std::time::Instant::now());
                    }
                }
                Event::ConvertPercent(path, p) => {
                    let eta = self.job.as_ref().and_then(|j| j.file_started).map(|t| t.elapsed().as_secs_f32());
                    if let Some(f) = self.files.iter_mut().find(|f| f.path == path && f.state == FileState::Running) {
                        f.percent = Some(p);
                        f.message = match eta {
                            // Un tempo che resta ha senso solo dopo un po' di lavoro fatto.
                            Some(spent) if p >= 0.02 && spent >= 8.0 => format!("In corso · {:.0}% · {}", p * 100.0, time_left(spent * (1.0 - p) / p)),
                            _ => format!("In corso · {:.0}%", p * 100.0),
                        };
                    }
                    if let Some(job) = self.job.as_mut() {
                        job.part = p;
                    }
                }
                Event::Finished(path, result) => {
                    let mut failed = false;
                    let key = self.job.as_ref().map(|j| j.key.clone());
                    if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
                        f.percent = None;
                        match result {
                            Ok((out, msg)) => {
                                f.state = FileState::Done;
                                f.done_key = key;
                                f.message = format!("Fatto · {msg}");
                                f.output = Some(out);
                            }
                            // Un file saltato perché esiste già è una scelta, non un errore.
                            Err(msg) if msg.contains("saltato") => {
                                f.state = FileState::Skipped;
                                f.done_key = key;
                                f.message = format!("Saltato · {msg}");
                            }
                            Err(msg) => {
                                f.state = FileState::Failed;
                                f.message = msg;
                                failed = true;
                            }
                        }
                    }
                    if let Some(job) = self.job.as_mut() {
                        job.part = 0.0;
                        job.done += 1;
                        job.failed += failed as usize;
                    }
                    self.transcribe.percent = 0;
                    self.transcribe.since = None;
                }
                Event::Ask(path, reply) => self.ask = Some(Ask { path, reply }),
                Event::TranscribeLog(line) => {
                    self.transcribe.log.push(line);
                    if self.transcribe.log.len() > 2000 {
                        self.transcribe.log.drain(..500);
                    }
                }
                Event::TranscribePercent(index, percent) => {
                    self.transcribe.percent = percent;
                    if self.transcribe.since.is_none() {
                        self.transcribe.since = Some((std::time::Instant::now(), percent));
                    }
                    if let Some(job) = self.job.as_mut() {
                        job.part = percent as f32 / 100.0;
                    }
                    let running: Vec<usize> = self.job.as_ref().map(|j| j.targets.clone()).unwrap_or_default();
                    if let Some(&i) = running.get(index) {
                        let f = &mut self.files[i];
                        if f.state == FileState::Waiting || f.state == FileState::Running {
                            f.state = FileState::Running;
                            f.message = format!("In corso · {percent}%");
                        }
                    }
                }
                Event::Link(web, ev) => {
                    let l = self.links_mut(web);
                    match ev {
                        youtube::Event::Log(text) => l.log_line(&text),
                        youtube::Event::Progress { index, total, percent, label } => {
                            l.progress = ((index as f32 + percent / 100.0) / total.max(1) as f32).clamp(0.0, 1.0);
                            if let Some(r) = l.items.get_mut(index) {
                                r.state = FileState::Running;
                                r.percent = percent;
                                r.message = format!("{percent:.0}%");
                            }
                            l.note = label;
                        }
                        youtube::Event::File(_) => {}
                        youtube::Event::Info { index, info } => {
                            if let Some(r) = l.items.get_mut(index) {
                                r.detail = match info.entries {
                                    Some(n) => format!("{} · playlist di {n} video", info.author),
                                    None => format!("{} · {}", info.author, youtube::clock(info.seconds)),
                                };
                                r.title = info.title;
                            }
                        }
                        youtube::Event::LinkDone { index, result } => {
                            if let Some(r) = l.items.get_mut(index) {
                                match result {
                                    Ok(n) => {
                                        r.state = FileState::Done;
                                        r.message = if n > 1 { format!("Fatto · {n} file") } else { "Fatto".into() };
                                    }
                                    Err(e) => {
                                        r.state = FileState::Failed;
                                        r.message = e;
                                    }
                                }
                            }
                            if let Some(job) = self.job.as_mut() {
                                job.done += 1;
                            }
                        }
                    }
                }
                Event::LinksFinished(web, result) => {
                    let l = self.links_mut(web);
                    match result {
                        Ok(files) => {
                            l.note = format!("Fatto: {} file in {}", files.len(), l.dest.display());
                            l.log_line("Finito");
                        }
                        Err(e) => {
                            l.log_line(&e);
                            l.note = e;
                        }
                    }
                }
                Event::Aborted(msg) => {
                    let kind = self.job.as_ref().map(|j| j.kind);
                    let targets = self.job.as_ref().map(|j| j.targets.clone()).unwrap_or_default();
                    let mut stopped = 0;
                    for i in targets {
                        if let Some(f) = self.files.get_mut(i).filter(|f| matches!(f.state, FileState::Waiting | FileState::Running)) {
                            f.state = FileState::Failed;
                            f.message = msg.clone();
                            f.percent = None;
                            stopped += 1;
                        }
                    }
                    if let Some(job) = self.job.as_mut() {
                        job.done += stopped;
                        job.failed += stopped;
                    }
                    if let Some(JobKind::Download { web }) = kind {
                        for r in self.links_mut(web).items.iter_mut().filter(|r| matches!(r.state, FileState::Waiting | FileState::Running)) {
                            r.state = FileState::Failed;
                            r.message = msg.clone();
                        }
                    }
                }
                Event::JobDone => {
                    crate::accel::clear_in_use();
                    if let Some(job) = self.job.take() {
                        if matches!(job.kind, JobKind::Convert | JobKind::Transcribe) {
                            let text = if job.failed > 0 {
                                format!("{} di {} file non riusciti: il motivo è accanto a ciascuno.", job.failed, job.total)
                            } else {
                                format!("Fatto: {} file.", job.total)
                            };
                            self.notice = Some(Notice { text, error: job.failed > 0 });
                        }
                    }
                }
                Event::Media(path, loaded) => {
                    self.media.insert(path, loaded);
                }
                Event::Preview(path, page, img) => {
                    let key = (path, page);
                    if let Some(slot) = self.previews.get_mut(&key) {
                        if img.is_none() {
                            self.previews_failed.insert(key);
                        }
                        *slot = img;
                    }
                }
                Event::PageThumb(path, page, img) => {
                    if let Some(slot) = self.page_thumbs.get_mut(&(path, page)) {
                        *slot = img;
                    }
                }
                Event::Estimate(path, key, result) => {
                    if let Some(slot) = self.estimates.get_mut(&(path, key)) {
                        *slot = Some(result);
                    }
                }
                Event::StampPreview(key, img) => {
                    if self.stamp_view.0 == key {
                        self.stamp_view.1 = img;
                    }
                }
                Event::Wave(path, bars) => self.review_wave(&path, bars),
                Event::Summary(path, p) => self.summary_event(&path, p),
            }
        }
        changed
    }

    /// File finiti / totale del lavoro in corso (0..1): file completati sul totale, non stime.
    pub fn progress(&self) -> Option<f32> {
        self.job.as_ref().map(|j| match j.kind {
            JobKind::Download { web } => self.links(web).progress,
            JobKind::Convert | JobKind::Transcribe => (j.done as f32 + j.part.clamp(0.0, 1.0)) / j.total.max(1) as f32,
        })
    }

    /// Dove salva, in parole: «Accanto agli originali» oppure il nome della cartella.
    pub fn dest_label(&self) -> String {
        match &self.dest {
            None => "Accanto all'originale".into(),
            Some(p) => p.display().to_string(),
        }
    }

    pub fn exists_label(&self) -> &'static str {
        conversioni::EXISTS_CHOICES.iter().find(|(_, e)| *e == self.values.exists).map(|(l, _)| *l).unwrap_or("")
    }

    /// Il nome che avrebbe la copia con la regola «Aggiungi -2» (per l'esempio sotto la scelta).
    pub fn exists_example(&self) -> String {
        let (name, ext) = self.example_name();
        match self.values.exists {
            Exists::Number if ext.is_empty() => format!("{} {}-2", crate::i18n::tr("Esempio:"), crate::i18n::lit(&crate::i18n::tr(name))),
            Exists::Number => format!("{} {}-2.{ext}", crate::i18n::tr("Esempio:"), crate::i18n::lit(&crate::i18n::tr(name))),
            Exists::Replace => "Il file con lo stesso nome viene sostituito.".into(),
            Exists::Skip => "Il file con lo stesso nome resta com'è.".into(),
            Exists::Ask => "Cedro te lo chiede file per file.".into(),
        }
    }

    /// Il nome (con «nomefile» al posto di quello vero) e l'estensione di quello che esce con la voce scelta.
    pub fn example_name(&self) -> (&'static str, &str) {
        // Unire e dividere fanno PDF; le altre voci corte sono l'estensione.
        let ext = if self.action.len() <= 5 && !conversioni::is_page_action(self.kind(), &self.action) { self.action.as_str() } else { "pdf" };
        // Gli strumenti delle immagini hanno il loro nome e la loro estensione (come in engine).
        // Le voci del laboratorio hanno nomi loro (come in engine).
        let lab = match (self.kind(), self.action.as_str()) {
            (crate::engine::Kind::Image, "figure") => Some(("nomefile-figura", "tiff")),
            (crate::engine::Kind::Image, "tiles") => Some(("nomefile-tessere", "")),
            (crate::engine::Kind::Image, "mitosi") => Some(("nomefile-conta-mitotica", "jpg")),
            (crate::engine::Kind::Image, "consult") => Some(("caso-1a2b3c-consulto", "zip")),
            (crate::engine::Kind::Image, "rx") => Some(("nomeanimale-radiografie", "pdf")),
            _ => None,
        };
        let (name, ext) = match (self.kind(), ext) {
            _ if lab.is_some() => lab.unwrap_or_default(),
            (crate::engine::Kind::Image, "strip") => ("nomefile-pulito", "jpg"),
            (crate::engine::Kind::Pdf, "strip") => ("nomefile-anonimo", "pdf"),
            (crate::engine::Kind::Doc, "strip") => ("nomefile-anonimo", "docx"),
            (crate::engine::Kind::Image, "compress") => ("nomefile-compresso", "jpg"),
            (crate::engine::Kind::Image, "half") => ("nomefile-meta", "png"),
            (crate::engine::Kind::Image, "gray") => ("nomefile-bn", "png"),
            (crate::engine::Kind::Image, "rot90") => ("nomefile-ruotato", "png"),
            (crate::engine::Kind::Image, "ocr") => ("nomefile", "txt"),
            (crate::engine::Kind::Image, "jpeg") => ("nomefile", "jpg"),
            (_, e) => ("nomefile", e),
        };
        (name, ext)
    }

    /// Play o pausa per il file scelto nell'anteprima.
    pub fn media_toggle(&mut self) {
        let Some(i) = self.selected else { return };
        let Some(file) = self.files.get(i) else { return };
        if let Some(p) = self.player.as_mut().filter(|p| p.path == file.path) {
            p.toggle();
            return;
        }
        let duration = self.media.get(&file.path).and_then(|m| m.about.as_ref()).and_then(|a| a.duration);
        let mut p = super::player::Player::start(&file.path, file.kind == Kind::Video, duration, self.wake.clone());
        p.set_speed(SPEEDS[(self.play_speed as usize).min(SPEEDS.len() - 1)].1);
        p.set_volume((self.play_vol * 100.0).round() as u32);
        self.player = Some(p);
    }

    /// Salta di `secs` secondi (anche indietro) nel file in anteprima.
    pub fn media_skip(&mut self, secs: i32) {
        let Some(i) = self.selected else { return };
        let Some(file) = self.files.get(i) else { return };
        if self.player.as_ref().is_none_or(|p| p.path != file.path) {
            self.media_toggle();
        }
        if let Some(p) = self.player.as_mut() {
            let to = p.position() + secs as f64;
            p.seek(to);
        }
    }

    /// Dove cominciano le frasi del file scelto, in secondi: quelle in arrivo se lavora, quelle salvate se è finito.
    pub fn phrase_starts(&self) -> Vec<f64> {
        let Some(file) = self.selected.and_then(|i| self.files.get(i)) else { return Vec::new() };
        match file.state {
            FileState::Running => self.transcribe.log.iter().filter(|l| l.tone == trascrizione::Tone::Speech).filter_map(|l| trascrizione::parse_clock(&l.key)).collect(),
            FileState::Done => crate::revisione::load(&file.path).map(|s| s.segments.iter().map(|g| g.t0 as f64 / 100.0).collect()).unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Salta alla frase prima (`dir` < 0: l'inizio di quella che si sente, o della precedente se è appena partita) o dopo.
    pub fn media_phrase(&mut self, dir: i32) {
        let starts = self.phrase_starts();
        let Some(i) = self.selected else { return };
        let Some(file) = self.files.get(i) else { return };
        let pos = self.player.as_ref().filter(|p| p.path == file.path).map_or(0.0, |p| p.position());
        let to = if dir < 0 {
            starts.iter().rev().find(|&&t| t < pos - 1.5).copied().or_else(|| starts.first().map(|_| 0.0))
        } else {
            starts.iter().find(|&&t| t > pos + 0.05).copied()
        };
        let Some(to) = to else { return };
        if self.player.as_ref().is_none_or(|p| p.path != file.path) {
            self.media_toggle();
        }
        if let Some(p) = self.player.as_mut() {
            p.seek(to);
        }
    }

    pub fn set_play_speed(&mut self, k: u8) {
        self.play_speed = k.min(SPEEDS.len() as u8 - 1);
        let pct = SPEEDS[self.play_speed as usize].1;
        if let Some(p) = self.player.as_mut() {
            p.set_speed(pct);
        }
    }

    pub fn set_play_volume(&mut self, v: f32) {
        self.play_vol = v.clamp(0.0, 1.0);
        let pct = (self.play_vol * 100.0).round() as u32;
        if let Some(p) = self.player.as_mut() {
            p.set_volume(pct);
        }
    }

    /// Salta al punto `frac` (0..1) del file in anteprima; parte se non suonava ancora.
    pub fn media_seek(&mut self, frac: f32) {
        let Some(i) = self.selected else { return };
        let Some(file) = self.files.get(i) else { return };
        let Some(d) = self.media.get(&file.path).and_then(|m| m.about.as_ref()).and_then(|a| a.duration) else { return };
        if self.player.as_ref().is_none_or(|p| p.path != file.path) {
            self.media_toggle();
        }
        if let Some(p) = self.player.as_mut() {
            p.seek(frac.clamp(0.0, 1.0) as f64 * d);
        }
    }

    pub fn page_count(&self, index: usize) -> Option<usize> {
        self.files.get(index).and_then(|f| self.media.get(&f.path)).and_then(|m| m.pages)
    }
}

#[cfg(test)]
mod tests {
    use super::time_left;

    #[test]
    fn labs_pages_follow_what_is_being_done() {
        use super::*;
        let mut app = App::new(Arc::new(|| {}));
        assert_eq!(app.lab, Lab::Home);
        // A «Inizio» si resta finché non si sceglie una pagina.
        app.lab_sync();
        assert_eq!(app.lab, Lab::Home);
        app.set_lab(Lab::Figura, false);
        assert_eq!((app.activity, app.kind(), app.action.as_str()), (Activity::Convert, Kind::Image, "figure"));
        // Dal menu dei formati si passa a un formato qualunque: la barra torna su «Salva immagini e vetrini».
        app.set_action("png");
        app.lab_sync();
        assert_eq!(app.lab, Lab::Immagini);
        app.set_lab(Lab::Anonimizza, false);
        assert_eq!(app.action, "strip");
        app.set_lab(Lab::Immagini, false);
        assert_ne!(app.action, "strip");
        app.set_lab(Lab::Dati, true);
        assert_eq!((app.kind(), app.action.as_str()), (Kind::Text, "xlsx"));
        app.set_lab(Lab::Pdf, false);
        assert_eq!(app.kind(), Kind::Pdf);
        app.set_lab(Lab::Trascrivi, false);
        app.lab_sync();
        assert_eq!((app.activity, app.lab), (Activity::Transcribe, Lab::Trascrivi));
        for lab in Lab::LAB.into_iter().chain(Lab::STANDARD) {
            assert!(!lab.name().is_empty());
        }
        // In «Semplice» le immagini partono da JPEG (una delle voci della pagina), non dal WebP di «Converti file».
        app.set_lab(Lab::Immagini, true);
        app.action = app.default_action(true);
        app.set_lab(Lab::Immagini, true);
        assert_eq!(app.action, "jpeg");
        app.set_lab(Lab::Anonimizza, true);
        app.set_lab(Lab::Immagini, true);
        assert_eq!(app.action, "jpeg");
        // Su «Togli i nomi» il passaggio ai PDF in coda resta su «Togli i nomi».
        app.set_lab(Lab::Anonimizza, false);
        let pdf = CATEGORIES.iter().position(|c| c.kind == Kind::Pdf).unwrap();
        app.set_category(pdf, false);
        app.lab_sync();
        assert_eq!((app.kind(), app.action.as_str(), app.lab), (Kind::Pdf, "strip", Lab::Anonimizza));
    }

    #[test]
    fn time_left_is_in_words() {
        assert_eq!(time_left(30.0), "meno di un minuto");
        assert_eq!(time_left(12.0 * 60.0), "circa 12 min");
        assert_eq!(time_left(95.0 * 60.0), "circa 1 h 35 min");
    }

    #[test]
    fn a_panic_becomes_an_error() {
        assert_eq!(super::guarded(|| Ok::<_, String>(7)), Ok(7));
        let err = super::guarded::<()>(|| panic!("decodificatore in crisi")).unwrap_err();
        assert!(err.contains("decodificatore in crisi"), "{err}");
    }

    #[test]
    fn queued_downloads_cancel_without_starting_helpers() {
        use super::*;
        // No network or helper startup is allowed while another job owns the gate.
        let _owner = crate::resources::acquire(None).unwrap();
        for web in [false, true] {
            let mut app = App::new(Arc::new(|| {}));
            app.links_mut(web).typed = "https://www.youtube.com/watch?v=test".into();
            app.start_links(web);
            assert!(app.busy());
            assert!(matches!(app.rx.recv_timeout(std::time::Duration::from_millis(150)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)));
            app.cancel();
            let mut events = Vec::new();
            loop {
                let event = app.rx.recv_timeout(std::time::Duration::from_secs(5)).expect("download cancellation stalled");
                let done = matches!(event, Event::JobDone);
                assert!(!matches!(event, Event::Link(_, _)), "download ran while queued");
                events.push(event);
                if done { break; }
            }
            for event in events { app.tx.send(event).unwrap(); }
            app.pump();
            assert!(!app.busy());
            assert_eq!(app.links(web).note, "Annullato");
            assert_eq!(app.links(web).items[0].state, FileState::Failed);
            assert_eq!(app.links(web).items[0].message, "Annullato");
        }
    }

    #[test]
    fn a_board_keeps_order_turns_and_removals() {
        let mut b = super::Board::new(4);
        assert!(b.untouched());
        b.pages[1].2 = true;
        b.pages[2].1 = 90;
        b.pages.swap(0, 3);
        assert!(!b.untouched());
        assert_eq!(b.kept(), vec![(4, 0), (3, 90), (1, 0)]);
        // Trascinata di un posto e mezzo a destra: va due posti più in là, sulla stessa riga.
        assert_eq!(crate::ui::views::page_drop(0, 225.0, 0.0, 150.0, 222.0, 5), 2);
        assert_eq!(crate::ui::views::page_drop(4, 0.0, 230.0, 150.0, 222.0, 5), 9);
        assert_eq!(crate::ui::views::page_drop(1, -900.0, 0.0, 150.0, 222.0, 5), 0);
    }

    #[test]
    fn an_aborted_job_does_not_stay_busy() {
        use super::*;
        let mut app = App::new(Arc::new(|| {}));
        let path = PathBuf::from("/non/esiste/foto.jpg");
        app.files.push(FileItem { path, kind: Kind::Image, voice_only: false, state: FileState::Running, message: "In corso".into(), output: None, checked: true, done_key: None, percent: Some(0.4), pages: String::new() });
        app.job = Some(Job { kind: JobKind::Transcribe, total: 1, done: 0, failed: 0, cancel: Arc::new(AtomicBool::new(false)), key: String::new(), targets: vec![0], part: 0.0, file_started: None });
        app.tx.send(Event::Aborted("Errore interno".into())).unwrap();
        app.tx.send(Event::JobDone).unwrap();
        app.pump();
        assert!(!app.busy());
        assert_eq!(app.files[0].state, FileState::Failed);
        assert!(app.notice.as_ref().is_some_and(|n| n.error));
    }

    #[test]
    fn a_page_that_cannot_be_drawn_stops_loading() {
        use super::*;
        let mut app = App::new(Arc::new(|| {}));
        let path = PathBuf::from("/non/esiste/rotto.pdf");
        app.want_preview(&path, 0);
        assert!(app.preview_loading(&path, 0));
        app.tx.send(Event::Preview(path.clone(), 0, None)).unwrap();
        app.pump();
        assert!(!app.preview_loading(&path, 0), "la rotellina girerebbe per sempre");
        // Un'altra pagina chiesta dopo: la prima si potrà richiedere di nuovo.
        app.want_preview(&path, 1);
        assert!(app.preview_loading(&path, 1));
        assert!(!app.previews_failed.contains(&(path.clone(), 0)) || app.previews.contains_key(&(path.clone(), 0)));
    }
}

/// Vero per un foglio di calcolo (Excel, Calc): «Porta in Excel» non ha senso, si porta in CSV.
pub fn foglio_di_calcolo(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "xlsx" | "xlsm" | "ods"))
}
