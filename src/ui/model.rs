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
}

impl Activity {
    pub const ALL: [Activity; 4] = [Activity::Convert, Activity::Transcribe, Activity::YouTube, Activity::Web];

    pub fn name(self) -> &'static str {
        match self {
            Activity::Convert => "Conversioni",
            Activity::Transcribe => "Trascrizione",
            Activity::YouTube => "YouTube",
            Activity::Web => "Video dal web",
        }
    }

    pub fn icon(self) -> &'static str {
        use super::assets::*;
        match self {
            Activity::Convert => CONVERT,
            Activity::Transcribe => TRANSCRIBE,
            Activity::YouTube => VIDEO,
            Activity::Web => WEB,
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

/// Le scelte della trascrizione.
#[derive(Clone, Debug)]
pub struct Transcribe {
    pub format: trascrizione::Format,
    pub model: usize,
    pub log: Vec<trascrizione::Line>,
    /// Avanzamento del file in lavoro (0..100).
    pub percent: u8,
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
}

/// Cosa i thread di lavoro dicono all'interfaccia.
pub enum Event {
    Started(PathBuf),
    Finished(PathBuf, Result<(PathBuf, String), String>),
    /// Il file di uscita esiste già e la regola è «Chiedi ogni volta».
    Ask(PathBuf, Sender<ExistsChoice>),
    TranscribeLog(trascrizione::Line),
    TranscribePercent(usize, u8),
    Link(bool, youtube::Event),
    LinksFinished(bool, Result<Vec<PathBuf>, String>),
    JobDone,
    Media(PathBuf, media::Loaded),
    Preview(PathBuf, usize, Option<Image>),
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
    /// La pagina dell'anteprima del documento.
    pub preview_page: usize,
    /// Il file audio o video che suona nell'anteprima (tasto Play).
    pub player: Option<super::player::Player>,
    /// Dove sta il cursore della barra mentre lo si trascina (0..1).
    pub media_scrub: Option<f32>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
    loader: media::Loader,
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
        let mut values = Values::new();
        if crate::read_settings().replace {
            values.exists = Exists::Replace;
        }
        let mut app = App {
            activity: Activity::Convert,
            files: Vec::new(),
            category: 0,
            action: String::new(),
            values,
            quality: engine::read_quality(),
            dest: None,
            job: None,
            selected: None,
            transcribe: Transcribe { format: trascrizione::Format::Word, model: trascrizione::default_model(), log: Vec::new(), percent: 0 },
            youtube: Links::new(false),
            web: Links::new(true),
            notice: None,
            ask: None,
            media: HashMap::new(),
            previews: HashMap::new(),
            preview_page: 0,
            player: None,
            media_scrub: None,
            tx,
            rx,
            wake,
            loader,
        };
        app.action = app.default_action(false);
        app
    }

    pub fn kind(&self) -> Kind {
        CATEGORIES[self.category].kind
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
            self.category = index;
            self.action = self.default_action(simple);
            self.selected = self.visible().first().copied();
            self.preview_page = 0;
        }
    }

    /// Cambia la voce scelta; un'opzione mai toccata riparte dal suo valore consigliato.
    pub fn set_action(&mut self, id: &str) {
        self.action = id.to_string();
    }

    /// Il nome di ciò che si sta per fare, per il pulsante («WebP», «Comprimi»…).
    pub fn action_label(&self) -> &'static str {
        conversioni::action_label(self.kind(), &self.action)
    }

    pub fn option_defs(&self) -> Vec<&'static OptionDef> {
        conversioni::option_defs(self.kind(), &self.action)
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

    /// Aggiunge file (e il contenuto delle cartelle, un livello) alla coda. Se i file nuovi sono
    /// di un'altra categoria, la vista passa a quella: altrimenti resterebbero invisibili.
    pub fn add_files(&mut self, paths: Vec<PathBuf>, simple: bool) -> usize {
        let mut expanded = Vec::new();
        for p in paths {
            if p.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&p) {
                    let mut inner: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
                    inner.sort();
                    expanded.extend(inner);
                }
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
            self.files.push(FileItem { path, kind, voice_only, state: FileState::Waiting, message: "In attesa".into(), output: None, checked: true, done_key: None });
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
        visible.into_iter().filter(|&i| self.files[i].checked).collect()
    }

    /// Le scelte che cambiano il risultato: se cambiano, i file già fatti si rifanno.
    fn settings_key(&self) -> String {
        format!(
            "{:?}|{}|{}|{}|{:?}|{:?}|{}",
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
        let outs: Vec<&PathBuf> = self.targets().into_iter().filter_map(|i| self.files[i].output.as_ref()).collect();
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
            self.loader.preview(path, page);
        }
    }

    /// Il testo dell'errore se le scelte scritte a mano non vanno (per esempio le pagine).
    pub fn validate(&self) -> Result<(), String> {
        conversioni::validate(&self.values, self.kind(), &self.action)?;
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
            Activity::Transcribe => self.start_transcribe(),
            Activity::YouTube => self.start_links(false),
            Activity::Web => self.start_links(true),
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
        self.job = Some(Job { kind, total: files.len(), done: 0, failed: 0, cancel: cancel.clone(), key: self.settings_key(), targets: files.to_vec() });
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
        let paths: Vec<PathBuf> = targets.iter().map(|&i| self.files[i].path.clone()).collect();
        let cancel = self.begin(&targets, JobKind::Convert);
        let action = self.action.clone();
        let quality = self.quality;
        let dest = self.dest.clone();
        let opts = self.exists_options(conversioni::options_from(&self.values));
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            for path in paths {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(Event::Finished(path, Err("Annullato".into())));
                    continue;
                }
                let _ = tx.send(Event::Started(path.clone()));
                wake();
                crate::accel::clear_in_use();
                let result = conversioni::convert_one_out(&path, &action, quality, dest.as_deref(), &opts);
                let _ = tx.send(Event::Finished(path, result));
                wake();
            }
            let _ = tx.send(Event::JobDone);
            wake();
        });
    }

    fn start_transcribe(&mut self) {
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
        let settings = crate::read_settings();
        let request = trascrizione::Request { format: self.transcribe.format, model: self.transcribe.model, gpu: settings.gpu, avx512: settings.avx512 };
        let dest = self.dest.clone();
        let opts = self.exists_options(conversioni::options_from(&self.values));
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let send = |e: Event| {
                let _ = tx.send(e);
                wake();
            };
            trascrizione::run_cancellable(&paths, &request, dest.as_deref(), &opts, &cancel, &|event| match event {
                trascrizione::Event::Log(line) => send(Event::TranscribeLog(line)),
                trascrizione::Event::Percent { index, percent } => send(Event::TranscribePercent(index, percent)),
                trascrizione::Event::Finished { path, result } => send(Event::Finished(path, result)),
            });
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
        self.job = Some(Job { kind: JobKind::Download { web }, total, done: 0, failed: 0, cancel: cancel.clone(), key: String::new(), targets: Vec::new() });
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let result = youtube::run(
                &req,
                &mut |e| {
                    let _ = tx.send(Event::Link(web, e));
                    wake();
                },
                &cancel,
            );
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
        let mut changed = false;
        while let Ok(e) = self.rx.try_recv() {
            changed = true;
            match e {
                Event::Started(path) => {
                    if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
                        f.state = FileState::Running;
                        f.message = "In corso".into();
                    }
                }
                Event::Finished(path, result) => {
                    let mut failed = false;
                    let key = self.job.as_ref().map(|j| j.key.clone());
                    if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
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
                        job.done += 1;
                        job.failed += failed as usize;
                    }
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
                    if let Some(slot) = self.previews.get_mut(&(path, page)) {
                        *slot = img;
                    }
                }
            }
        }
        changed
    }

    /// File finiti / totale del lavoro in corso (0..1): file completati sul totale, non stime.
    pub fn progress(&self) -> Option<f32> {
        self.job.as_ref().map(|j| match j.kind {
            JobKind::Download { web } => self.links(web).progress,
            _ => j.done as f32 / j.total.max(1) as f32,
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
        let ext = if self.action.len() <= 5 { self.action.as_str() } else { "pdf" };
        match self.values.exists {
            Exists::Number => format!("Esempio: nomefile-2.{ext}"),
            Exists::Replace => "Il file con lo stesso nome viene sostituito.".into(),
            Exists::Skip => "Il file con lo stesso nome resta com'è.".into(),
            Exists::Ask => "Cedro te lo chiede file per file.".into(),
        }
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
        self.player = Some(super::player::Player::start(&file.path, file.kind == Kind::Video, duration, self.wake.clone()));
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
