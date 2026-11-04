//! L'interfaccia di Cedro per il tocco, in unità dp (1 dp = 1/160 di pollice). Niente di Android qui
//! dentro: la finestra passa tocchi e dimensioni, qui si disegna (con il pittore di `cedro::ui`) e si decide
//! cosa fare. Ogni controllo che si vede è collegato al motore: se non lo è, non c'è.
//!
//! Tre schede in basso: **Ruota** (la ruota a tutta larghezza, col file nel centro), **Lavori** (avanzamento,
//! risultati con Apri e Condividi, cronologia) e **Aspetto** (i cinque temi). Due schermate a tutto schermo
//! salgono sopra: **Cosa ne faccio?** dopo «Condividi» e **Scegli il file**. Bersagli sempre ≥ 48 dp.
//! In orizzontale e sui tablet la ruota sta a sinistra e le opzioni a destra.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cedro::ui::assets as ic;
use cedro::ui::gfx::{Align, Fit, Image, Painter, Weight};
use cedro::ui::theme::{mix, Palette, ThemeId};
use cedro::ui::views::uses_quality;
use cedro::ui::wheel::list;
use cedro::{Kind, Mode};

mod casa;
mod opzioni;
mod pdf;
mod rivedi;
pub use pdf::{Campo, Pagina, StatoPdf, Tasto};
pub use rivedi::{da_controllare, FoglioRiv};

pub type R = (f32, f32, f32, f32);

/// I temi che si possono scegliere (quello del Comune è dell'edizione dell'ente, non del telefono).
pub const TEMI: [ThemeId; 5] =
    [ThemeId::WindowsEssenziale, ThemeId::PercorsoGuidato, ThemeId::CedroCompatto, ThemeId::BancoDiLavoro, ThemeId::DocumentoAlCentro];

const BARRA: f32 = 56.0;
const TOCCO: f32 = 48.0;
const NAV: f32 = 64.0;
const BIANCO: (u8, u8, u8) = (255, 255, 255);

/// Il file che si sta lavorando.
#[derive(Clone)]
pub struct Archivio {
    pub path: PathBuf,
    pub nome: String,
    pub dim: u64,
}

/// Un file già usato, per «Ultimi file».
#[derive(Clone)]
pub struct Recente {
    pub path: PathBuf,
    pub nome: String,
    pub kind: Kind,
    pub thumb: Option<Image>,
}

#[derive(Clone, PartialEq)]
pub enum Stato {
    InCorso,
    Fatto { path: PathBuf, nome: String, msg: String },
    Errore(String),
}

#[derive(Clone)]
pub struct Lavoro {
    pub id: u32,
    pub origine: String,
    pub etichetta: String,
    pub conv: bool,
    pub inizio: Instant,
    /// Secondi dal 1970, per «2 min fa».
    pub quando: u64,
    pub stato: Stato,
    /// Avanzamento in millesimi (0..=1000); `u32::MAX` finché il motore non lo sa.
    pub prog: Arc<AtomicU32>,
    /// La trascrizione: l'audio di partenza, per «Rivedi».
    pub sorgente: Option<PathBuf>,
    /// I punti da controllare trovati nella trascrizione.
    pub controlli: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheda {
    Ruota,
    Lavori,
    Aspetto,
    Info,
}

/// Cosa far cercare al selettore di file di Android.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sorgente {
    Foto,
    Documenti,
    Audio,
    Altri,
    /// Un link (dagli appunti): «Video dal web».
    Link,
    /// Altri PDF da unire (scelta multipla).
    PdfAltri,
    /// File per la coda della finestra principale (scelta multipla).
    Coda,
    /// Tutti i file di una cartella, per la coda.
    Cartella,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Azione {
    Scheda(Scheda),
    Tessera(usize),
    /// «Altro»: la ruota con tutti i formati.
    Altro,
    Esci,
    Fetta(usize),
    Modo(u8),
    Vai,
    Cursore,
    /// Il modello di trascrizione: 0 Veloce (tiny), 1 Buona (base).
    Modello(usize),
    Tema(usize),
    /// «Caratteri»: Normali (falso) o Grandi (vero).
    Caratteri(bool),
    /// 0 = lingua del telefono, 1 = italiano, 2 = inglese.
    Lingua(usize),
    /// «Copia le informazioni».
    CopiaInfo,
    /// L'icona sulla scheda di un lavoro non riuscito: copia l'errore con tutti i dettagli.
    CopiaErrore(u32),
    IndietroFoglio,
    ApriScelta,
    ChiudiScelta,
    Sorgente(Sorgente),
    Recente(usize),
    Apri(u32),
    Condividi(u32),
    Svuota,
    /// Le schermate dei PDF.
    Pdf(pdf::PdfAz),
    /// Un campo di testo: la tastiera scrive lì.
    Campo(Campo),
    /// «Rivedi» sulla scheda di una trascrizione.
    Rivedi(u32),
    /// Dentro «Rivedi trascrizione».
    Riv(rivedi::RivAz),
    /// «Altre opzioni» e «Impostazioni».
    Opz(opzioni::OpzAz),
    /// Il pulsante a fianco di «Converti»: apre «Altre opzioni».
    AltreOpzioni,
    /// La finestra principale (dall'icona di Cedro).
    Casa(casa::CasaAz),
    /// Un'area che non fa niente ma ferma il tocco.
    Niente,
}

/// Un lavoro da far partire col motore.
pub struct Compito {
    pub id: u32,
    pub file: PathBuf,
    pub azione: &'static str,
    pub qualita: u8,
    pub prog: Arc<AtomicU32>,
    pub opts: Option<Box<cedro::Options>>,
    pub unisci: Vec<(PathBuf, Option<cedro::PageSel>)>,
}

pub enum Effetto {
    Niente,
    /// Più lavori, uno dopo l'altro (il pulsante grande della finestra principale).
    Tanti(Vec<Compito>),
    /// Il link dagli appunti va nel campo di «Video dal web».
    Incolla,
    /// Miniatura e misure di file messi in coda.
    LeggiVoci(Vec<PathBuf>),
    /// Pagine e segnalibri di un PDF aperto dalla finestra principale, e i PDF da unire.
    LeggiPdf(PathBuf, Vec<PathBuf>),
    Ridisegna,
    /// Da eseguire col motore.
    /// `opts`: le scelte delle schermate (None = quelle di sempre); `unisci`: i PDF da unire, in ordine, con le pagine.
    Converti { id: u32, file: PathBuf, azione: &'static str, qualita: u8, prog: Arc<AtomicU32>, opts: Option<Box<cedro::Options>>, unisci: Vec<(PathBuf, Option<cedro::PageSel>)> },
    /// Prova la password del PDF: se va, se ne fa una copia senza.
    Sblocca(String),
    /// Legge l'onda dell'audio di una revisione (percorso, durata).
    OndaRevisione(PathBuf, f64),
    /// Le impostazioni sono cambiate: vanno ricordate (`imp` nel motore, modello e «se esiste già» qui).
    SalvaImpostazioni,
    /// Scrive un file nuovo (la revisione salvata) e lo consegna come il risultato del lavoro `id`.
    Scrivi { id: u32, nome: String, dati: Vec<u8> },
    /// Chiude Cedro: si torna all'app di prima.
    Esci,
    /// Il tema è cambiato: va ricordato.
    SalvaTema(ThemeId),
    /// La misura dei caratteri è cambiata: va ricordata.
    SalvaCaratteri(bool),
    /// La lingua è cambiata («auto», «it» o «en»): va ricordata.
    SalvaLingua(&'static str),
    /// L'elenco dei lavori è cambiato: va ricordato.
    SalvaLavori,
    /// Mette un testo negli appunti.
    Copia(String),
    /// Il Riassunto AI di una trascrizione (lavoro `id`): le righe per il modello e i nomi delle voci.
    Riassumi { id: u32, source: PathBuf, lines: Vec<String>, names: Vec<String>, prog: Arc<AtomicU32> },
    /// Apre il selettore di file di Android.
    Scegli(Sorgente),
    /// Carica un file già noto (da «Ultimi file»).
    UsaFile(PathBuf),
    Apri(PathBuf),
    Condividi(PathBuf),
}

#[derive(Clone, Copy)]
enum Forma {
    R(R),
    Fetta { cx: f32, cy: f32, ri: f32, ro: f32, a0: f32, a1: f32 },
    Cerchio { cx: f32, cy: f32, r: f32 },
}

impl Forma {
    fn dentro(&self, x: f32, y: f32) -> bool {
        match *self {
            Forma::R((rx, ry, rw, rh)) => x >= rx && x < rx + rw && y >= ry && y < ry + rh,
            Forma::Cerchio { cx, cy, r } => (x - cx).powi(2) + (y - cy).powi(2) <= r * r,
            Forma::Fetta { cx, cy, ri, ro, a0, a1 } => {
                let (dx, dy) = (x - cx, y - cy);
                let r = (dx * dx + dy * dy).sqrt();
                if r < ri || r > ro {
                    return false;
                }
                let tau = std::f32::consts::TAU;
                let a = (dy.atan2(dx) - a0).rem_euclid(tau);
                a < (a1 - a0)
            }
        }
    }
}

/// Una tessera della schermata «Cosa ne faccio?».
struct Riga {
    id: &'static str,
    titolo: &'static str,
    sotto: &'static str,
    /// Vero se è un cambio di formato («Converti in X»); falso per le azioni («Comprimi»).
    conv: bool,
    /// Arrivata con questa versione: la tessera lo dice.
    nuovo: bool,
}

const fn r(id: &'static str, titolo: &'static str, sotto: &'static str, conv: bool) -> Riga {
    Riga { id, titolo, sotto, conv, nuovo: false }
}

const fn rn(id: &'static str, titolo: &'static str, sotto: &'static str, conv: bool) -> Riga {
    Riga { id, titolo, sotto, conv, nuovo: true }
}

/// Un «file» che in realtà è un link (http/https): si scarica, non si converte.
pub fn e_link(path: &Path) -> bool {
    let t = path.to_string_lossy();
    t.starts_with("https://") || t.starts_with("http://")
}

/// Le azioni di «Video dal web»: ogni tessera chiama `webvideo`, o yt-dlp per YouTube (l'azione comincia con «web-»).
fn righe_link(path: &Path) -> Vec<&'static Riga> {
    // Senza yt-dlp nell'APK (la build per il catalogo F-Droid) YouTube passa a un'app che lo scarica (NewPipe, YTDLnis…).
    if !cfg!(feature = "youtube") && cedro::youtube::is_youtube_url(&path.to_string_lossy()) {
        static YT: [Riga; 1] = [r("web-altra-app", "Apri con un'altra app", "NewPipe, YTDLnis…", false)];
        return YT.iter().collect();
    }
    static WEB: [Riga; 4] = [
        r("web-video", "Video", "Il migliore che c'è", false),
        r("web-audio", "Solo audio", "MP3 dal video", false),
        r("web-leggero", "Video leggero", "Meno spazio", false),
        r("web-testo", "Testo", "Dalla voce, offline", false),
    ];
    WEB.iter().collect()
}

/// Le azioni da mettere nelle tessere per un tipo di file: solo quelle che il motore sa fare.
fn righe(kind: Kind) -> Vec<&'static Riga> {
    static IMG: [Riga; 9] = [
        r("jpeg", "JPG", "Più leggera", true),
        r("png", "PNG", "Senza perdita", true),
        r("webp", "WebP", "Per il web", true),
        rn("avif", "AVIF", "Ancora più leggera", true),
        rn("jxl", "JPEG XL", "Il JPEG nuovo", true),
        r("pdf", "PDF", "Un foglio", true),
        r("compress", "Comprimi", "Meno spazio", false),
        r("ocr", "Testo", "Riconosci scritte", false),
        r("half", "Dimezza", "Metà dimensioni", false),
    ];
    static PDF: [Riga; 10] = [
        r("docx", "Word", "Con testo", true),
        rn("xlsx", "Excel", "Le tabelle", true),
        r("txt", "Testo", "Estrae le parole", true),
        r("jpg", "JPG", "Una per pagina", true),
        rn("merge", "Unisci", "Con altri PDF", false),
        rn("split", "Dividi", "In più file", false),
        rn("pages", "Pagine", "Ruota, togli, numeri", false),
        rn("compress", "Comprimi", "Meno spazio", false),
        r("ocr", "Leggi", "Riconosci scritte", false),
        r("png", "PNG", "Una per pagina", true),
    ];
    static ARC: [Riga; 6] = [
        r("z-webp", "Foto in WebP", "Le immagini dello ZIP", false),
        r("z-jpeg", "Foto in JPEG", "Le immagini dello ZIP", false),
        r("z-png", "Foto in PNG", "Le immagini dello ZIP", false),
        r("z-mp3", "Audio in MP3", "L'audio dello ZIP", false),
        r("z-wav", "Audio in WAV", "L'audio dello ZIP", false),
        r("z-pdf", "Documenti in PDF", "Word, testo, foto", false),
    ];
    static TXT: [Riga; 3] = [r("pdf", "PDF", "Da stampare", true), r("html", "HTML", "Per il web", true), r("md", "Markdown", "Testo con segni", true)];
    static AUD: [Riga; 6] = [
        r("mp3", "MP3", "Più leggero", true),
        r("wav", "WAV", "Senza compressione", true),
        rn("flac", "FLAC", "Senza perdita, leggero", true),
        rn("opus", "Opus", "Il più leggero", true),
        rn("aiff", "AIFF", "Per Mac e studi", true),
        r("trascrivi", "Testo", "Dalla voce, offline", false),
    ];
    static VID: [Riga; 3] = [r("mp4", "MP4", "Più leggero", true), r("mp3", "Solo audio", "MP3 dal video", false), r("trascrivi", "Testo", "Dalla voce, offline", false)];
    static DOC: [Riga; 1] = [r("pdf", "PDF", "Da documento", true)];
    let tutte: &'static [Riga] = match kind {
        Kind::Image => &IMG,
        Kind::Pdf => &PDF,
        Kind::Text => &TXT,
        Kind::Audio => &AUD,
        Kind::Video => &VID,
        Kind::Doc => &DOC,
        Kind::Archive => &ARC,
        Kind::Other => &[],
    };
    let ids: Vec<&'static str> = [Mode::Convert, Mode::Tools].into_iter().flat_map(|m| list(kind, m)).map(|s| s.id).chain(cedro::panel_slices(kind).iter().map(|s| s.id)).collect();
    // «trascrivi» non è nel catalogo del motore di conversione: è whisper.cpp dentro l'APK; «pages» è la schermata Pagine.
    tutte.iter().filter(|r| ids.contains(&r.id) || r.id == "trascrivi" || (kind == Kind::Pdf && r.id == "pages")).collect()
}

fn sotto_altro(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "BMP, GIF, TIFF…",
        _ => "Tutte le azioni",
    }
}

fn icona_di(id: &str) -> &'static str {
    match id {
        "web-video" | "web-leggero" | "web-altra-app" => ic::VIDEO,
        "web-audio" => ic::AUDIO,
        "webp" | "bmp" | "gray" | "jpeg" | "jpg" | "png" | "tiff" | "gif" => ic::IMAGE,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => ic::VIDEO,
        "wav" | "mp3" | "ogg" | "m4a" | "aac" | "flac" | "opus" | "aiff" | "z-mp3" | "z-wav" => ic::AUDIO,
        "avif" | "jxl" | "z-webp" | "z-jpeg" | "z-png" => ic::IMAGE,
        "zip-tutti" => ic::FOLDER,
        "compress" => pdf::I_COMPRIMI,
        "half" | "strip" => ic::DOWNLOAD,
        "rot90" => ic::CONVERT,
        "xlsx" => pdf::I_EXCEL,
        "merge" => pdf::I_UNISCI,
        "split" => pdf::I_DIVIDI,
        "pages" => pdf::I_PAGINE,
        "ocr" | "txt" => ic::SEARCH,
        "trascrivi" | "web-testo" => ic::TRANSCRIBE,
        _ => ic::FILE,
    }
}

fn icona_tipo(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => ic::IMAGE,
        Kind::Audio => ic::AUDIO,
        Kind::Video => ic::VIDEO,
        _ => ic::FILE,
    }
}

fn nome_tipo(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "Immagine",
        Kind::Pdf => "PDF",
        Kind::Text => "Testo",
        Kind::Audio => "Audio",
        Kind::Video => "Video",
        Kind::Doc => "Documento",
        Kind::Archive => "Archivio",
        Kind::Other => "File",
    }
}

fn estensione(nome: &str) -> String {
    Path::new(nome).extension().and_then(|e| e.to_str()).unwrap_or("").to_uppercase()
}

fn peso(b: u64) -> String {
    if b >= 1_000_000 {
        format!("{:.1} MB", b as f64 / 1e6).replace('.', ",")
    } else {
        format!("{} KB", (b / 1000).max(1))
    }
}

pub fn adesso() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn quando(t: u64) -> String {
    let d = adesso().saturating_sub(t);
    match d {
        0..=59 => "adesso".into(),
        60..=3599 => format!("{} min fa", d / 60),
        3600..=86399 => format!("{} h fa", d / 3600),
        _ => format!("{} g fa", d / 86400),
    }
}

/// Un errore è «messaggio breve» oppure «messaggio breve\n---\ndettagli»: sulla scheda va solo il messaggio.
fn dividi_errore(m: &str) -> (&str, &str) {
    m.split_once("\n---\n").unwrap_or((m, ""))
}

// ---- Lo stile curato dei temi del computer (v0.33): riquadri sollevati, pulsanti sfumati, piastrelle ----

/// Un riquadro sollevato, come `Frame::card` del computer: ombra morbida, fondo della superficie, filo leggero.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sollevato(p: &mut Painter, pal: &Palette, scuro: bool, x: f32, y: f32, w: f32, h: f32, r: f32) {
    p.shadow(x, y, w, h, r, 10.0, if scuro { 0.28 } else { 0.10 }, 3.0);
    let bordo = if scuro { pal.border } else { mix(pal.border, pal.surface, 0.25) };
    p.rrect(x, y, w, h, r, Some(pal.surface), Some((bordo, 1.0)));
}

/// Il fondo di un pulsante pieno in rilievo (`glossy_fill` del computer): ombra, sfumatura dall'alto,
/// filo chiaro sul bordo; premuto è più scuro.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lucido(p: &mut Painter, scuro: bool, x: f32, y: f32, w: f32, h: f32, r: f32, c: (u8, u8, u8), premuto: bool) {
    let spread = (h * 0.14).clamp(4.0, 10.0);
    p.shadow(x, y, w, h, r, spread, if scuro { 0.35 } else { 0.2 }, spread * 0.4);
    let (su, giu) = if premuto { (mix(c, (255, 255, 255), 0.04), mix(c, (0, 0, 0), 0.2)) } else { (mix(c, (255, 255, 255), 0.16), mix(c, (0, 0, 0), 0.06)) };
    p.rrect_vgrad(x, y, w, h, r, su, giu);
    p.rrect_line(x, y, w, h, r, (255, 255, 255), 1.0, 0.12, None);
}

/// La piastrella colorata con l'icona (`tile` del computer): piena con l'icona bianca quando è scelta,
/// appena tinta con l'icona del colore quando no.
#[allow(clippy::too_many_arguments)]
pub(crate) fn piastrella(p: &mut Painter, pal: &Palette, scuro: bool, x: f32, y: f32, s: f32, icona: &str, c: (u8, u8, u8), on: bool) {
    let r = (s * 0.27).round();
    let is = (s * 0.62).round();
    let pad = (s - is) / 2.0;
    if on {
        p.shadow(x, y, s, s, r, 5.0, if scuro { 0.3 } else { 0.12 }, 2.0);
        p.rrect_vgrad(x, y, s, s, r, mix(c, (255, 255, 255), 0.12), mix(c, (0, 0, 0), 0.06));
        p.icon(icona, x + pad, y + pad, is, (255, 255, 255));
    } else {
        p.rrect(x, y, s, s, r, Some(mix(c, pal.surface, if scuro { 0.8 } else { 0.86 })), None);
        p.icon(icona, x + pad, y + pad, is, c);
    }
}

fn intersezione(a: R, b: R) -> Option<R> {
    let (x0, y0) = (a.0.max(b.0), a.1.max(b.1));
    let (x1, y1) = ((a.0 + a.2).min(b.0 + b.2), (a.1 + a.3).min(b.1 + b.3));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// Le informazioni sul telefono e su ciò che Cedro usa (le legge `glue`, la scheda «Info» le mostra).
#[derive(Clone, Default)]
pub struct Info {
    /// La versione dell'APK (versionName e versionCode).
    pub versione: String,
    /// Telefono, Android, processore, core, memoria, scheda video.
    pub generali: Vec<(String, String)>,
    /// Istruzioni del processore e se ci sono.
    pub istruzioni: Vec<(String, bool)>,
    /// Accelerazioni: nome, a cosa servono, se sono in uso.
    pub accelerazioni: Vec<(String, String, bool)>,
}

impl Info {
    /// Il testo da copiare e mandare in caso di problemi (nella lingua in uso).
    pub fn testo(&self) -> String {
        let t = cedro::i18n::tr_string;
        let mut out = format!("Cedro {}\n", self.versione);
        for (k, v) in &self.generali {
            out += &format!("{}: {v}\n", t(k));
        }
        let chips: Vec<String> = self.istruzioni.iter().map(|(n, si)| format!("{n} {}", t(if *si { "sì" } else { "no" }))).collect();
        out += &format!("{}: {}\n", t("Istruzioni"), chips.join(", "));
        for (k, v, _) in &self.accelerazioni {
            out += &format!("{}: {}\n", t(k), t(v));
        }
        out
    }
}

pub struct App {
    pub info: Info,
    pub tema: ThemeId,
    /// Caratteri grandi (+2, come sul computer).
    pub grandi: bool,
    /// «auto», «it» o «en».
    pub lingua: &'static str,
    /// La lingua del telefono, per «auto».
    pub lingua_telefono: cedro::i18n::Lang,
    pub file: Option<Archivio>,
    pub kind: Kind,
    pub miniatura: Option<Image>,
    pub onda: Option<Vec<f32>>,
    pub scheda: Scheda,
    /// «Cosa ne faccio?» a tutto schermo (dopo «Condividi»).
    pub foglio: bool,
    /// «Scegli il file» a tutto schermo.
    pub scelta_file: bool,
    pub modo: u8,
    /// La fetta scelta nella ruota.
    pub scelta: Option<usize>,
    /// La tessera scelta in «Cosa ne faccio?».
    pub tessera: usize,
    pub qualita: u8,
    /// Modello di trascrizione scelto (indice in `trascrizione::MODELS`: 0 Veloce, 1 Buona).
    pub modello: usize,
    /// Quante persone parlano (trascrizione): 0 Automatico, 1 una sola (niente riconoscimento), 2..=8.
    pub voci: u8,
    pub lavori: Vec<Lavoro>,
    /// L'ultimo errore copiato: la sua icona diventa una spunta.
    copiato: Option<u32>,
    pub recenti: Vec<Recente>,
    /// Si è arrivati alla ruota dal foglio: «indietro» ci torna.
    pub dal_foglio: bool,
    prossimo: u32,
    scorri: f32,
    scorri_max: f32,
    trascina: Option<(f32, f32, bool)>,
    /// Il trascinamento orizzontale di una fascia che scorre: (fascia, x di partenza, y di partenza, scorrimento di partenza, mosso).
    trascina_x: Option<(usize, f32, f32, f32, bool)>,
    premuto: Option<Azione>,
    colpi: Vec<(Forma, Azione)>,
    /// Le fasce che scorrono di lato col dito (rifatte a ogni disegno): riquadro e numero della fascia.
    fasce: Vec<(R, usize)>,
    cursore: R,
    /// Istante attuale per le animazioni (fissato a ogni disegno).
    ora: Instant,
    /// Barre di sistema e gesti, in dp: sinistra, alto, destra, basso.
    pub margini: (f32, f32, f32, f32),
    /// Le schermate dei PDF e le loro scelte.
    pub pdf: StatoPdf,
    /// Il campo di testo in cui scrive la tastiera.
    pub campo: Option<Campo>,
    /// Toccato un campo: la tastiera va mostrata (anche se il sistema l'aveva chiusa col tasto indietro).
    pub mostra_tastiera: bool,
    /// «Rivedi trascrizione», a tutto schermo.
    pub rivedi: Option<rivedi::Rivedi>,
    /// «Altre opzioni», e le scelte dell'ultima volta.
    pub opzioni: Option<opzioni::Opzioni>,
    opzioni_salvate: Option<cedro::conversioni::Values>,
    /// «Impostazioni» aperte.
    pub impostazioni: bool,
    /// Le impostazioni del motore (qualità, DPI, OCR), il nome a modello e cosa fare se il file esiste già.
    pub imp: cedro::Settings,
    pub modello_nome: String,
    pub esiste: cedro::Exists,
    /// Gli altri file condivisi insieme al primo (per farne uno ZIP).
    pub altri: Vec<PathBuf>,
    /// La finestra principale del tema, aperta dall'icona (falso quando si arriva da «Condividi»).
    pub casa_aperta: bool,
    /// Per ogni trascrizione in corso: quando ha dato la prima percentuale, e quale (per il tempo che resta).
    pub partenze: std::collections::HashMap<u32, (Instant, u32)>,
    pub casa: casa::Casa,
    /// Lavori, Aspetto o Informazioni sopra la finestra principale, con la freccia per tornare.
    pub sopra: Option<Scheda>,
    /// Le pagine dei PDF in coda.
    pub pdf_pagine: std::collections::HashMap<PathBuf, u32>,
}

/// Le lingue tra cui scegliere: chiave nel file, nome (quelli delle lingue non si traducono).
pub const LINGUE: [(&str, &str); 3] = [("auto", "Lingua del telefono"), ("it", "Italiano"), ("en", "English")];

impl App {
    /// Mette in uso la lingua scelta (per «auto», quella del telefono).
    pub fn applica_lingua(&self) {
        cedro::i18n::set(match self.lingua {
            "it" => cedro::i18n::Lang::It,
            "en" => cedro::i18n::Lang::En,
            _ => self.lingua_telefono,
        });
    }

    pub fn new(tema: ThemeId) -> App {
        App {
            info: Info::default(),
            tema,
            grandi: false,
            lingua: "auto",
            lingua_telefono: cedro::i18n::Lang::It,
            file: None,
            kind: Kind::Image,
            miniatura: None,
            onda: None,
            scheda: Scheda::Ruota,
            foglio: false,
            scelta_file: false,
            modo: 0,
            scelta: None,
            tessera: 0,
            qualita: 82,
            modello: 1,
            voci: 0,
            lavori: Vec::new(),
            copiato: None,
            recenti: Vec::new(),
            dal_foglio: false,
            prossimo: 1,
            scorri: 0.0,
            scorri_max: 0.0,
            trascina: None,
            trascina_x: None,
            premuto: None,
            colpi: Vec::new(),
            fasce: Vec::new(),
            cursore: (0.0, 0.0, 1.0, 1.0),
            ora: Instant::now(),
            margini: (0.0, 0.0, 0.0, 0.0),
            pdf: StatoPdf::default(),
            campo: None,
            mostra_tastiera: false,
            rivedi: None,
            opzioni: None,
            opzioni_salvate: None,
            impostazioni: false,
            imp: cedro::Settings::default(),
            modello_nome: String::new(),
            esiste: cedro::Exists::Number,
            altri: Vec::new(),
            casa_aperta: true,
            partenze: std::collections::HashMap::new(),
            casa: casa::Casa::default(),
            sopra: None,
            pdf_pagine: std::collections::HashMap::new(),
        }
    }

    /// Imposta il file: da «Condividi» si parte da «Cosa ne faccio?», dal resto dalla ruota.
    pub fn imposta_file(&mut self, f: Archivio, kind: Kind, da_condivisione: bool) {
        self.file = Some(f);
        self.kind = kind;
        self.modo = 0;
        self.tessera = 0;
        self.scelta = if self.fette().is_empty() { None } else { Some(0) };
        self.miniatura = None;
        self.onda = None;
        self.scelta_file = false;
        self.scheda = Scheda::Ruota;
        if da_condivisione {
            self.casa_aperta = false;
            self.sopra = None;
        }
        self.foglio = da_condivisione && (!righe(kind).is_empty() || self.file.as_ref().is_some_and(|f| e_link(&f.path)));
        self.altri.clear();
        self.dal_foglio = false;
        self.pdf = StatoPdf::default();
        self.campo = None;
        self.opzioni = None;
        self.altri.clear();
    }

    /// Un lavoro è finito (o fallito).
    pub fn finisce(&mut self, id: u32, stato: Stato) -> Effetto {
        self.salvata(id, &stato);
        if let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.riassunto.as_ref().is_some_and(|r| r.0 == id)) {
            rv.riassunto = None;
            if let Stato::Errore(e) = &stato {
                rv.riassunto_nota = dividi_errore(e).0.to_string();
            }
        }
        if let Some(l) = self.lavori.iter_mut().find(|l| l.id == id) {
            l.stato = stato;
        }
        Effetto::SalvaLavori
    }

    /// Vero se serve ridisegnare a ogni fotogramma (barra di avanzamento che scorre).
    pub fn animando(&mut self) -> bool {
        if std::mem::take(&mut self.pdf.ridisegna) || self.rivedi.as_ref().is_some_and(|r| r.suona()) || (self.casa_aperta && self.sopra.is_none() && self.lavori.iter().any(|l| l.stato == Stato::InCorso)) {
            return true;
        }
        self.lavori.iter().any(|l| l.stato == Stato::InCorso) && !self.scelta_file && (self.scheda == Scheda::Lavori || self.scheda == Scheda::Ruota)
    }

    /// Lavori in corso e avanzamento medio in percentuale (None se ancora ignoto), per la notifica.
    pub fn in_corso(&self) -> (usize, Option<u32>, String) {
        let v: Vec<&Lavoro> = self.lavori.iter().filter(|l| l.stato == Stato::InCorso).collect();
        let note: Vec<u32> = v.iter().map(|l| l.prog.load(Ordering::Relaxed)).filter(|m| *m <= 1000).collect();
        let medio = if note.len() == v.len() && !note.is_empty() { Some(note.iter().sum::<u32>() / note.len() as u32 / 10) } else { None };
        let nome = v.first().map(|l| if l.conv { format!("{} · {}", l.origine, l.etichetta) } else { format!("{}: {}", l.etichetta, l.origine) }).unwrap_or_default();
        (v.len(), medio, nome)
    }

    /// Il cronologico da salvare su disco: i lavori finiti, uno per riga.
    pub fn serializza(&self) -> String {
        let pulisci = |s: &str| s.replace(['\t', '\n'], " ");
        self.lavori
            .iter()
            .filter_map(|l| match &l.stato {
                Stato::Fatto { path, msg, nome } => Some(format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    l.quando,
                    pulisci(&l.origine),
                    pulisci(&l.etichetta),
                    l.conv as u8,
                    path.display(),
                    pulisci(msg),
                    pulisci(nome),
                    l.sorgente.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                    l.controlli.map(|n| n.to_string()).unwrap_or_default()
                )),
                _ => None,
            })
            .take(30)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn carica_lavori(&mut self, testo: &str) {
        for riga in testo.lines() {
            let c: Vec<&str> = riga.split('\t').collect();
            if c.len() < 6 {
                continue;
            }
            let path = PathBuf::from(c[4]);
            let indirizzo = c[4].starts_with("content://");
            if !indirizzo && !path.exists() {
                continue;
            }
            let nome = match c.get(6) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => path.strip_prefix("/storage/emulated/0").map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            };
            self.lavori.push(Lavoro {
                id: self.prossimo,
                origine: c[1].into(),
                etichetta: c[2].into(),
                conv: c[3] == "1",
                inizio: Instant::now(),
                quando: c[0].parse().unwrap_or(0),
                stato: Stato::Fatto { path, nome, msg: c[5].into() },
                prog: Arc::new(AtomicU32::new(1000)),
                sorgente: c.get(7).filter(|s| !s.is_empty()).map(PathBuf::from).filter(|p| cedro::revisione::has(p)),
                controlli: c.get(8).and_then(|n| n.parse().ok()),
            });
            self.prossimo += 1;
        }
    }

    fn pal(&self) -> &'static Palette {
        self.tema.palette()
    }

    /// Il colore dei punti da controllare (arancio, leggibile sul tema chiaro e su quello scuro).
    fn avviso(&self) -> (u8, u8, u8) {
        if self.tema.dark() { (0xFF, 0xB4, 0x5C) } else { (0xA8, 0x4E, 0x00) }
    }

    fn ha_strumenti(&self) -> bool {
        list(self.kind, Mode::Tools).first().is_some_and(|s| s.id != "none")
    }

    fn fette(&self) -> Vec<cedro::Slice> {
        let mut l = list(self.kind, if self.modo == 1 { Mode::Tools } else { Mode::Convert });
        if l.first().is_some_and(|s| s.id == "none") {
            return Vec::new();
        }
        // La trascrizione (whisper.cpp nell'APK) non è nel catalogo del motore: sulla ruota è lo spicchio «Testo».
        if self.modo == 0 && matches!(self.kind, Kind::Audio | Kind::Video) && !self.file.as_ref().is_some_and(|f| e_link(&f.path)) {
            l.push(cedro::Slice { id: "trascrivi", label: "Testo" });
        }
        l
    }

    /// Le tessere del foglio per il file (o il link) di adesso.
    fn righe_ora(&self) -> Vec<&'static Riga> {
        if self.file.as_ref().is_some_and(|f| e_link(&f.path)) {
            righe_link(&self.file.as_ref().map(|f| f.path.clone()).unwrap_or_default())
        } else if !self.altri.is_empty() {
            // Più file condivisi insieme: per prima cosa uno ZIP con tutti.
            static ZIP: Riga = rn("zip-tutti", "ZIP", "Tutti in un archivio", false);
            std::iter::once(&ZIP).chain(righe(self.kind)).collect()
        } else {
            righe(self.kind)
        }
    }

    /// L'azione scelta ora è la trascrizione (si sceglie il modello al posto della qualità)?
    fn modello_serve(&self) -> bool {
        self.file.is_some()
            && if self.foglio {
                self.righe_ora().get(self.tessera).is_some_and(|r| r.id == "trascrivi" || r.id == "web-testo")
            } else {
                self.scelta.and_then(|i| self.fette().get(i).copied()).is_some_and(|f| f.id == "trascrivi")
            }
    }

    /// Il cursore Qualità (o la scelta del modello) serve per l'azione scelta ora?
    fn qualita_serve(&self) -> bool {
        if self.file.is_none() {
            return false;
        }
        if self.modello_serve() {
            return true;
        }
        if self.foglio {
            return self.righe_ora().get(self.tessera).is_some_and(|r| uses_quality(self.kind, r.id));
        }
        self.scelta.and_then(|i| self.fette().get(i).copied()).is_some_and(|f| uses_quality(self.kind, f.id))
    }

    // ---- Tocco ----

    pub fn tocca_giu(&mut self, x: f32, y: f32) -> Effetto {
        let (x, y) = (x - self.margini.0, y - self.margini.1);
        self.premuto = self.trova(x, y);
        if self.rivedi.is_some() {
            self.trascina = Some((y, self.scorri_rivedi_ora(), false));
            return Effetto::Ridisegna;
        }
        if self.casa_aperta && self.casa.elenco.is_some() {
            self.trascina = Some((y, self.casa.elenco_scorri, false));
            return Effetto::Ridisegna;
        }
        if let Some(Azione::Pdf(pdf::PdfAz::Maniglia(i))) = self.premuto {
            self.pdf.trascina = Some((i, y, y));
            return Effetto::Ridisegna;
        }
        // Una fascia che scorre di lato (le categorie, la striscia dei file): si decide al primo movimento.
        self.trascina_x = if self.casa_aperta && !(self.pdf.protetto && self.file.is_some()) {
            self.fasce.iter().find(|(r, _)| Forma::R(*r).dentro(x, y)).map(|(_, f)| (*f, x, y, self.casa.fx[*f], false))
        } else {
            None
        };
        self.trascina = Some((y, self.scorri, false));
        if self.premuto == Some(Azione::Cursore) {
            self.imposta_cursore(x);
        }
        Effetto::Ridisegna
    }

    pub fn tocca_muovi(&mut self, x: f32, y: f32) -> Effetto {
        let (x, y) = (x - self.margini.0, y - self.margini.1);
        if self.premuto == Some(Azione::Cursore) {
            self.imposta_cursore(x);
            return Effetto::Ridisegna;
        }
        if let Some((i, y0, _)) = self.pdf.trascina {
            self.pdf.trascina = Some((i, y0, y));
            return Effetto::Ridisegna;
        }
        if let Some((f, x0, y0, s0, mosso)) = self.trascina_x {
            let (dx, dy) = (x - x0, y - y0);
            if mosso || (dx.abs() > 10.0 && dx.abs() > dy.abs()) {
                self.casa.fx[f] = (s0 - dx).clamp(0.0, self.casa.fx_max[f]);
                self.trascina_x = Some((f, x0, y0, s0, true));
                self.trascina = None;
                self.premuto = None;
                return Effetto::Ridisegna;
            }
            if dy.abs() > 10.0 {
                // Il dito va in su o in giù: scorre la pagina, non la fascia.
                self.trascina_x = None;
            }
        }
        if let (Some((y0, s0, mosso)), true) = (self.trascina, self.casa_aperta && self.casa.elenco.is_some()) {
            if mosso || (y - y0).abs() > 10.0 {
                self.casa.elenco_scorri = (s0 - (y - y0)).clamp(0.0, self.casa.elenco_max);
                self.trascina = Some((y0, s0, true));
                self.premuto = None;
                return Effetto::Ridisegna;
            }
            return Effetto::Niente;
        }
        if let (Some((y0, s0, mosso)), true) = (self.trascina, self.rivedi.is_some()) {
            if mosso || (y - y0).abs() > 10.0 {
                self.scorri_rivedi(y - y0, s0);
                self.trascina = Some((y0, s0, true));
                self.premuto = None;
                return Effetto::Ridisegna;
            }
            return Effetto::Niente;
        }
        if let Some((y0, s0, mosso)) = self.trascina {
            let pagina = (self.pdf.pagina.is_some() && !self.pdf.protetto || self.opzioni.is_some() || self.impostazioni || self.casa_aperta && self.casa.elenco.is_none()) && !self.scelta_file;
            if (mosso || (y - y0).abs() > 10.0) && (pagina || matches!(self.scheda, Scheda::Lavori | Scheda::Info | Scheda::Aspetto) && !self.foglio && !self.scelta_file) {
                self.scorri = (s0 - (y - y0)).clamp(0.0, self.scorri_max);
                self.trascina = Some((y0, s0, true));
                self.premuto = None;
                return Effetto::Ridisegna;
            }
        }
        Effetto::Niente
    }

    pub fn tocca_su(&mut self, x: f32, y: f32) -> Effetto {
        let (x, y) = (x - self.margini.0, y - self.margini.1);
        let giu = self.premuto.take();
        if let Some((f, x0, _, s0, true)) = self.trascina_x {
            // L'ultimo tratto arriva con il sollevamento (gli spostamenti veloci vengono raggruppati).
            self.casa.fx[f] = (s0 - (x - x0)).clamp(0.0, self.casa.fx_max[f]);
        }
        let mosso = self.trascina.take().is_some_and(|t| t.2) | self.trascina_x.take().is_some_and(|t| t.4);
        if let Some((i, y0, y1)) = self.pdf.trascina.take() {
            self.pdf.lascia(i, y1 - y0);
            return Effetto::Ridisegna;
        }
        let su = self.trova(x, y);
        match (giu, su) {
            (Some(a), Some(b)) if a == b && !mosso => self.attiva(a),
            _ => Effetto::Ridisegna,
        }
    }

    pub fn tocca_annulla(&mut self) {
        self.premuto = None;
        self.trascina = None;
        self.trascina_x = None;
        self.pdf.trascina = None;
    }

    fn trova(&self, x: f32, y: f32) -> Option<Azione> {
        self.colpi.iter().rev().find(|(f, _)| f.dentro(x, y)).map(|(_, a)| *a)
    }

    fn imposta_cursore(&mut self, x: f32) {
        let (cx, _, cw, _) = self.cursore;
        let f = ((x - cx) / cw).clamp(0.0, 1.0);
        self.qualita = (40.0 + f * 55.0).round() as u8;
    }

    /// Il tasto «indietro» del telefono. Vero se l'abbiamo usato; falso = chiudi l'attività.
    pub fn indietro(&mut self) -> bool {
        if self.scelta_file {
            self.scelta_file = false;
            return true;
        }
        if self.campo.take().is_some() {
            return true;
        }
        if self.rivedi.is_some() {
            self.azione_rivedi(rivedi::RivAz::Chiudi);
            return true;
        }
        if self.impostazioni || self.opzioni.is_some() {
            self.azione_opzioni(opzioni::OpzAz::Indietro);
            return true;
        }
        if self.pdf.protetto {
            self.annulla_password();
            return true;
        }
        if self.pdf.pagina.take().is_some() {
            self.scorri = 0.0;
            return true;
        }
        if self.casa_aperta {
            if self.casa.elenco.take().is_some() {
                return true;
            }
            if self.casa.guida || self.sopra.is_some() {
                self.azione_casa(casa::CasaAz::Indietro);
                return true;
            }
            if self.tema == ThemeId::PercorsoGuidato && self.casa.passo > 0 {
                self.azione_casa(casa::CasaAz::Passo(-1));
                return true;
            }
            return false;
        }
        if self.foglio {
            return false;
        }
        if self.scheda != Scheda::Ruota {
            self.scheda = Scheda::Ruota;
            return true;
        }
        if self.dal_foglio {
            self.foglio = true;
            self.dal_foglio = false;
            return true;
        }
        false
    }

    fn avvia(&mut self, id: &'static str, etichetta: &str, conv: bool) -> Effetto {
        self.avvia_con(id, etichetta, conv, None, Vec::new())
    }

    fn avvia_con(&mut self, id: &'static str, etichetta: &str, conv: bool, opts: Option<Box<cedro::Options>>, unisci: Vec<(PathBuf, Option<cedro::PageSel>)>) -> Effetto {
        // Le Impostazioni valgono per ogni lavoro: se il nome esiste già e il nome a modello.
        let mut o = opts.unwrap_or_default();
        o.exists = self.esiste;
        if o.name_template.as_deref().is_none_or(|t| t.trim().is_empty()) && !self.modello_nome.trim().is_empty() {
            o.name_template = Some(self.modello_nome.clone());
        }
        let opts = Some(o);
        self.opzioni = None;
        let Some(file) = self.file.clone() else { return Effetto::Niente };
        let lavoro = self.prossimo;
        self.prossimo += 1;
        let prog = Arc::new(AtomicU32::new(u32::MAX));
        let sorgente = (id == "trascrivi").then(|| file.path.clone());
        self.lavori.insert(0, Lavoro { id: lavoro, origine: file.nome.clone(), etichetta: etichetta.into(), conv, inizio: Instant::now(), quando: adesso(), stato: Stato::InCorso, prog: prog.clone(), sorgente, controlli: None });
        self.scorri = 0.0;
        self.scheda = Scheda::Lavori;
        if self.casa_aperta {
            self.sopra = Some(Scheda::Lavori);
        }
        self.foglio = false;
        self.dal_foglio = false;
        self.pdf.pagina = None;
        self.campo = None;
        // Per «trascrivi» il campo qualità porta il modello scelto (bit bassi) e quante persone parlano (bit alti).
        let qualita = if id == "trascrivi" || id == "web-testo" { self.modello as u8 | self.voci.min(8) << 4 } else { self.qualita };
        Effetto::Converti { id: lavoro, file: file.path, azione: id, qualita, prog, opts, unisci }
    }

    fn attiva(&mut self, a: Azione) -> Effetto {
        // Toccare altrove chiude la tastiera (tranne «mostra la password», che si usa scrivendo).
        if !matches!(a, Azione::Campo(_) | Azione::Pdf(pdf::PdfAz::MostraPw)) {
            self.campo = None;
        }
        match a {
            Azione::Pdf(az) => self.azione_pdf(az),
            Azione::Riv(az) => self.azione_rivedi(az),
            Azione::Opz(az) => self.azione_opzioni(az),
            Azione::Casa(az) => self.azione_casa(az),
            Azione::AltreOpzioni => {
                let id = if self.foglio { self.righe_ora().get(self.tessera).map(|r| r.id) } else { self.scelta.and_then(|i| self.fette().get(i).map(|f| f.id)) };
                self.apri_opzioni(id)
            }
            Azione::Rivedi(id) => self.apri_rivedi(id),
            Azione::Campo(c) => {
                self.campo = Some(c);
                self.mostra_tastiera = true;
                Effetto::Ridisegna
            }
            Azione::Scheda(s) => {
                // «Ruota» riporta sempre alla ruota vuota, pronta per un nuovo file.
                if s == Scheda::Ruota {
                    self.file = None;
                    self.scelta = None;
                    self.miniatura = None;
                    self.onda = None;
                    self.modo = 0;
                    self.tessera = 0;
                    self.foglio = false;
                    self.dal_foglio = false;
                    self.scelta_file = false;
                }
                self.scheda = s;
                self.scorri = 0.0;
                Effetto::Ridisegna
            }
            Azione::Tessera(i) => {
                self.tessera = i;
                Effetto::Ridisegna
            }
            Azione::Altro => {
                self.foglio = false;
                self.dal_foglio = true;
                self.scheda = Scheda::Ruota;
                Effetto::Ridisegna
            }
            Azione::Esci => Effetto::Esci,
            Azione::Fetta(i) => {
                if self.file.is_some() {
                    self.scelta = Some(i);
                }
                Effetto::Ridisegna
            }
            Azione::Modo(m) => {
                self.modo = m;
                self.scelta = if self.fette().is_empty() { None } else { Some(0) };
                Effetto::Ridisegna
            }
            Azione::Vai => {
                if self.foglio {
                    match self.righe_ora().get(self.tessera).copied() {
                        Some(riga) if self.kind == Kind::Pdf && pdf::schermata(riga.id).is_some() => self.apri_pagina(pdf::schermata(riga.id).unwrap_or(pdf::Pagina::Excel)),
                        Some(riga) if riga.id == "zip-tutti" => {
                            let tutti: Vec<(PathBuf, Option<cedro::PageSel>)> = self.file.iter().map(|f| f.path.clone()).chain(self.altri.iter().cloned()).map(|p| (p, None)).collect();
                            let n = tutti.len();
                            self.avvia_con("zip-tutti", &format!("ZIP di {n} file"), false, None, tutti)
                        }
                        Some(riga) => self.avvia(riga.id, riga.titolo, riga.conv),
                        None => Effetto::Niente,
                    }
                } else {
                    match self.scelta.and_then(|i| self.fette().get(i).copied()) {
                        Some(f) if self.kind == Kind::Pdf && pdf::schermata(f.id).is_some() => self.apri_pagina(pdf::schermata(f.id).unwrap_or(pdf::Pagina::Excel)),
                        Some(f) => {
                            let conv = self.modo == 0;
                            self.avvia(f.id, f.label, conv)
                        }
                        None => Effetto::Niente,
                    }
                }
            }
            Azione::Cursore => Effetto::Ridisegna,
            Azione::Modello(m) => {
                self.modello = m;
                Effetto::Ridisegna
            }
            Azione::Tema(i) => {
                self.tema = TEMI[i];
                Effetto::SalvaTema(self.tema)
            }
            Azione::Caratteri(g) => {
                self.grandi = g;
                Effetto::SalvaCaratteri(g)
            }
            Azione::CopiaInfo => Effetto::Copia(self.info.testo()),
            Azione::Lingua(i) => {
                self.lingua = LINGUE[i].0;
                self.applica_lingua();
                Effetto::SalvaLingua(self.lingua)
            }
            Azione::IndietroFoglio => {
                self.indietro();
                Effetto::Ridisegna
            }
            Azione::ApriScelta => {
                self.scelta_file = true;
                Effetto::Ridisegna
            }
            Azione::ChiudiScelta => {
                self.scelta_file = false;
                Effetto::Ridisegna
            }
            Azione::Sorgente(s) => Effetto::Scegli(s),
            Azione::Recente(i) => match self.recenti.get(i) {
                Some(r) => Effetto::UsaFile(r.path.clone()),
                None => Effetto::Niente,
            },
            Azione::Apri(id) | Azione::Condividi(id) => {
                let path = self.lavori.iter().find(|l| l.id == id).and_then(|l| match &l.stato {
                    Stato::Fatto { path, .. } => Some(path.clone()),
                    _ => None,
                });
                match (path, matches!(a, Azione::Apri(_))) {
                    (Some(p), true) => Effetto::Apri(p),
                    (Some(p), false) => Effetto::Condividi(p),
                    _ => Effetto::Niente,
                }
            }
            Azione::CopiaErrore(id) => match self.lavori.iter().find(|l| l.id == id) {
                Some(l) => {
                    let testo = self.rapporto_errore(l);
                    self.copiato = Some(id);
                    Effetto::Copia(testo)
                }
                None => Effetto::Niente,
            },
            Azione::Svuota => {
                self.lavori.retain(|l| l.stato == Stato::InCorso);
                self.scorri = 0.0;
                Effetto::SalvaLavori
            }
            Azione::Niente => Effetto::Niente,
        }
    }

    // ---- Disegno ----

    /// Disegna tutto in un'immagine di `w`×`h` dp (il pittore ha già la scala).
    pub fn disegna(&mut self, p: &mut Painter, w: f32, h: f32) {
        self.ora = Instant::now();
        self.colpi.clear();
        self.fasce.clear();
        let pal = self.pal();
        p.clear(pal.bg);
        let (ml, mt, mr, mb) = self.margini;
        p.dx = ml;
        p.dy = mt;
        let (w, h) = (w - ml - mr, h - mt - mb);
        if self.scelta_file {
            self.pagina_scelta(p, w, h);
        } else if self.rivedi.is_some() {
            self.pagina_rivedi(p, w, h);
        } else if self.impostazioni {
            self.pagina_impostazioni(p, w, h);
        } else if self.opzioni.is_some() && (self.file.is_some() || self.opzioni.as_ref().is_some_and(|o| o.da_casa.is_some())) {
            self.pagina_opzioni(p, w, h);
        } else if let (Some(pg), true) = (self.pdf.pagina, self.file.is_some()) {
            self.pagina_pdf(p, pg, w, h);
        } else if self.casa_aperta {
            match self.sopra {
                Some(Scheda::Lavori) => self.pagina_lavori(p, w, h),
                Some(Scheda::Aspetto) => self.pagina_aspetto(p, w, h),
                Some(Scheda::Info) => self.pagina_info(p, w, h),
                _ => self.pagina_casa(p, w, h),
            }
        } else if self.foglio && self.file.is_some() {
            self.pagina_foglio(p, w, h);
        } else {
            let corpo = h - NAV;
            match self.scheda {
                Scheda::Ruota => self.ruota(p, w, corpo),
                Scheda::Lavori => self.pagina_lavori(p, w, corpo),
                Scheda::Info => self.pagina_info(p, w, corpo),
                Scheda::Aspetto => self.pagina_aspetto(p, w, corpo),
            }
            self.barra_schede(p, w, h);
        }
        if self.pdf.protetto && self.file.is_some() && !self.scelta_file {
            self.foglio_password(p, w, h);
        }
    }

    fn bottone_icona(&mut self, p: &mut Painter, rc: R, icona: &str, az: Azione) {
        let pal = self.pal();
        if self.premuto == Some(az) {
            p.rrect_a(rc.0, rc.1, rc.2, rc.3, rc.2 / 2.0, pal.primary, 0.14);
        }
        p.icon(icona, rc.0 + (rc.2 - 26.0) / 2.0, rc.1 + (rc.3 - 26.0) / 2.0, 26.0, pal.text);
        self.colpi.push((Forma::R(rc), az));
    }

    fn titolo_barra(&mut self, p: &mut Painter, x: f32, w: f32, titolo: &str) {
        let pal = self.pal();
        // Sopra la finestra principale: la freccia per tornarci.
        let (x, w) = if self.casa_aperta && self.sopra.is_some() {
            self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::Casa(casa::CasaAz::Indietro));
            (60.0, w - (60.0 - x))
        } else {
            (x, w)
        };
        p.text(titolo, x, 0.0, w, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
    }

    /// La barra in basso: Ruota, Lavori, Aspetto.
    fn barra_schede(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let (ml, _, mr, mb) = self.margini;
        let y = h - NAV;
        p.fill(-ml, y, w + ml + mr, NAV + mb, pal.surface);
        p.fill(-ml, y, w + ml + mr, 1.0, pal.border);
        let voci = [(Scheda::Ruota, if cedro::i18n::english() { "Wheel" } else { "Ruota" }, ic::CONVERT), (Scheda::Lavori, "Lavori", ic::CLOCK), (Scheda::Aspetto, "Aspetto", ic::SETTINGS), (Scheda::Info, "Info", ic::INFO)];
        let cw = w / 4.0;
        let attivi = self.lavori.iter().filter(|l| l.stato == Stato::InCorso).count();
        for (i, (s, nome, icona)) in voci.into_iter().enumerate() {
            let x = i as f32 * cw;
            let on = self.scheda == s;
            let prem = self.premuto == Some(Azione::Scheda(s));
            let col = if on { pal.primary } else { pal.muted };
            if on {
                p.rrect(x + cw / 2.0 - 30.0, y + 6.0, 60.0, 30.0, 15.0, Some(mix(pal.surface, pal.primary, 0.16)), None);
            } else if prem {
                p.rrect_a(x + cw / 2.0 - 30.0, y + 6.0, 60.0, 30.0, 15.0, pal.primary, 0.10);
            }
            p.icon(icona, x + cw / 2.0 - 12.0, y + 9.0, 24.0, col);
            p.text(&if s == Scheda::Ruota { cedro::i18n::lit(nome) } else { nome.to_string() }, x, y + 38.0, cw, 20.0, 13.0, if on { Weight::Semibold } else { Weight::Regular }, col, Align::Center);
            if s == Scheda::Lavori && attivi > 0 {
                let (bx, by) = (x + cw / 2.0 + 8.0, y + 5.0);
                p.circle(bx + 8.0, by + 8.0, 9.0, Some(pal.primary), None);
                p.text(&attivi.to_string(), bx - 2.0, by - 1.0, 20.0, 20.0, 12.0, Weight::Semibold, pal.on_primary, Align::Center);
            }
            self.colpi.push((Forma::R((x, y, cw, NAV)), Azione::Scheda(s)));
        }
    }

    // ----- L'anteprima del file (nel centro della ruota e nella scheda grande) -----

    /// Il file in un riquadro. `tonda`: dentro un cerchio, la foto riempie; altrimenti sta tutta dentro.
    fn anteprima(&mut self, p: &mut Painter, (x, y, w, h): R, raggio: f32, tonda: bool) {
        let pal = self.pal();
        p.rrect(x, y, w, h, raggio, Some(pal.alt), None);
        match (&self.miniatura, &self.onda) {
            (Some(img), _) => {
                if self.kind == Kind::Pdf {
                    // La prima pagina, intera, su carta bianca.
                    p.rrect(x, y, w, h, raggio, Some(BIANCO), None);
                    let pad = if tonda { w * 0.08 } else { 6.0 };
                    p.image(img, x + pad, y + pad, w - 2.0 * pad, h - 2.0 * pad, 2.0, Fit::Contain);
                } else {
                    p.image(img, x, y, w, h, raggio, if tonda { Fit::Cover } else { Fit::Contain });
                }
            }
            (None, Some(onda)) => {
                let n = onda.len().max(1);
                let (area_w, area_h) = (w * if tonda { 0.74 } else { 0.8 }, h * 0.5);
                let (ax, ay) = (x + (w - area_w) / 2.0, y + h / 2.0 - area_h / 2.0 - if tonda { 6.0 } else { 0.0 });
                let slot = area_w / n as f32;
                for (i, v) in onda.iter().enumerate() {
                    let bh = (v * area_h).max(3.0);
                    p.rrect(ax + i as f32 * slot + slot * 0.18, ay + (area_h - bh) / 2.0, slot * 0.64, bh, slot * 0.32, Some(pal.primary), None);
                }
            }
            _ => {
                let s = if tonda { 36.0 } else { 56.0 };
                p.icon(icona_tipo(self.kind), x + (w - s) / 2.0, y + (h - s) / 2.0 - if tonda { 8.0 } else { 0.0 }, s, pal.primary);
            }
        }
    }

    // ----- La ruota -----

    fn ruota(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let orizz = w > h * 1.2;
        let ha_file = self.file.is_some();
        let tabs_on = ha_file && self.ha_strumenti();
        let fette = if ha_file { self.fette() } else { list(Kind::Image, Mode::Convert) };
        let serve_q = self.qualita_serve();
        let ultimi = !self.recenti.is_empty();

        // Barra: nome del file (o «Cedro») e, se il file ha strumenti, il selettore Converti / Strumenti.
        let x_t = if self.dal_foglio {
            self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::IndietroFoglio);
            56.0
        } else {
            16.0
        };
        let (col_x, col_w) = if orizz { (0.0, 0.0) } else { (0.0, w) };
        let _ = (col_x, col_w);

        let (centro, ro, x0, w0, mut y0): ((f32, f32), f32, f32, f32, f32);
        if orizz {
            let d = (h - 16.0).min(w * 0.52).max(160.0);
            ro = d / 2.0;
            centro = (8.0 + ro, h / 2.0);
            x0 = d + 24.0;
            w0 = w - x0 - 16.0;
            y0 = 0.0;
        } else {
            let ultimi_h = if ultimi { 94.0 } else { 0.0 };
            let ctrl = |u: f32| (if serve_q { 52.0 } else { 0.0 }) + if ha_file { 66.0 } else { 44.0 } + u;
            let mut mostra = ultimi;
            let mut d = (w - 8.0).min(h - BARRA - ctrl(ultimi_h) - 8.0);
            if d < 230.0 && mostra {
                mostra = false;
                d = (w - 8.0).min(h - BARRA - ctrl(0.0) - 8.0);
            }
            let _ = mostra;
            let d = d.clamp(180.0, w - 8.0);
            ro = d / 2.0;
            centro = (w / 2.0, BARRA + 2.0 + ro);
            x0 = 16.0;
            w0 = w - 32.0;
            y0 = BARRA + 2.0 + d + 8.0;
        }

        // Titolo e modi.
        let (tx, tw) = if orizz { (x0, w0) } else { (x_t, w - x_t - 16.0) };
        let tw_mod = if tabs_on { 188.0 } else { 0.0 };
        let (titolo, sotto) = match &self.file {
            Some(f) => (f.nome.clone(), format!("{} · {}", nome_tipo(self.kind), peso(f.dim))),
            None => ("Cedro".into(), "Converti i tuoi file".into()),
        };
        if ha_file {
            p.text(&titolo, tx, 6.0, tw - tw_mod - 8.0, 26.0, 18.0, Weight::Semibold, pal.text, Align::Left);
            p.text(&sotto, tx, 30.0, tw - tw_mod - 8.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        } else {
            if !self.dal_foglio {
                p.symbol(16.0, 14.0, 28.0);
                p.text("Cedro", if orizz { x0 + 38.0 } else { 54.0 }, 0.0, 200.0, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
            }
        }
        if tabs_on {
            self.modi(p, (tx + tw - tw_mod, 4.0, tw_mod, TOCCO));
        }

        if orizz {
            y0 = BARRA + 4.0;
        }
        self.disco(p, centro, ro, &fette, ha_file);

        // Sotto (o a destra) i comandi.
        let mut y = y0;
        let spazio = if orizz { h } else { h };
        if ha_file {
            if serve_q {
                self.slider(p, (x0, y, w0, TOCCO));
                y += 52.0;
            }
            self.pulsante_vai(p, (x0, y, w0, 56.0));
            y += 66.0;
        } else {
            let msg = "Tocca il cerchio per scegliere un file, oppure condividilo a Cedro da un'altra app.";
            let usata = p.text_wrap(msg, x0, y, w0, 14.0, Weight::Regular, pal.muted, 20.0, 2);
            y += (usata + 6.0).max(44.0);
        }
        if ultimi && (orizz || y + 94.0 <= spazio + 2.0) {
            self.riga_ultimi(p, x0, y, w0, spazio - y);
        }
    }

    fn disco(&mut self, p: &mut Painter, (cx, cy): (f32, f32), ro: f32, fette: &[cedro::Slice], attivo: bool) {
        let pal = self.pal();
        let ri = ro * 0.52;
        let n = fette.len().max(1);
        let tau = std::f32::consts::TAU;
        let sweep = tau / n as f32;
        p.disc_a(cx, cy + 5.0, ro + 4.0, (0, 0, 0), 0.07);
        p.disc_a(cx, cy + 2.0, ro + 1.0, (0, 0, 0), 0.06);
        let (corner, gap) = (10.0, 3.0);
        let dimm = if attivo { 1.0 } else { 0.55 };
        for (i, f) in fette.iter().enumerate() {
            let a0 = -std::f32::consts::FRAC_PI_2 + i as f32 * sweep;
            let a1 = a0 + sweep;
            let sel = attivo && self.scelta == Some(i);
            let premuta = self.premuto == Some(Azione::Fetta(i));
            let fondo = if sel {
                pal.primary
            } else if premuta {
                mix(pal.alt, pal.primary, 0.28)
            } else {
                mix(pal.surface, pal.alt, 0.6)
            };
            let pts = sector(cx, cy, ri + corner, ro - corner, a0, a1, gap + corner);
            p.round_poly(&pts, corner, fondo, dimm);
            if attivo {
                self.colpi.push((Forma::Fetta { cx, cy, ri, ro: ro + 2.0, a0, a1 }, Azione::Fetta(i)));
            }
            let mid = (a0 + a1) / 2.0;
            let rm = (ri + ro) / 2.0;
            let (tx, ty) = (cx + rm * mid.cos(), cy + rm * mid.sin());
            let fg = if sel { pal.on_primary } else { pal.text };
            let ts = (ro / 170.0 * 15.5).clamp(13.0, 17.0);
            let ancho = (2.0 * rm * (sweep / 2.0).sin()).min(ro - ri).max(48.0);
            p.icon(icona_di(f.id), tx - 13.0, ty - 27.0, 26.0, if attivo { fg } else { pal.muted });
            p.text(f.label, tx - ancho / 2.0, ty + 1.0, ancho, 26.0, ts, Weight::Semibold, if attivo { fg } else { pal.muted }, Align::Center);
        }
        // Il centro è il posto del file.
        let rc = ri - 7.0;
        p.disc_a(cx, cy + 3.0, rc + 6.0, (0, 0, 0), 0.10);
        p.circle(cx, cy, rc + 4.0, Some(pal.surface), None);
        if self.file.is_some() {
            self.anteprima(p, (cx - rc, cy - rc, rc * 2.0, rc * 2.0), rc, true);
            // Tipo e peso, leggibili sopra qualunque anteprima.
            if let Some(f) = &self.file {
                let et = format!("{} · {}", estensione(&f.nome), peso(f.dim));
                let tw = p.measure(&et, 12.0, Weight::Semibold) + 18.0;
                p.rrect_a(cx - tw / 2.0, cy + rc * 0.46, tw, 22.0, 11.0, (0, 0, 0), 0.62);
                p.text(&et, cx - tw / 2.0, cy + rc * 0.46, tw, 22.0, 12.0, Weight::Semibold, BIANCO, Align::Center);
            }
        } else {
            p.rrect_line(cx - rc, cy - rc, rc * 2.0, rc * 2.0, rc, pal.primary, 2.0, 1.0, Some((7.0, 6.0)));
            p.icon(ic::ADD, cx - 18.0, cy - 36.0, 36.0, pal.primary);
            p.text("Scegli un file", cx - rc, cy + 4.0, rc * 2.0, 24.0, 14.0, Weight::Semibold, pal.text, Align::Center);
        }
        if self.premuto == Some(Azione::ApriScelta) {
            p.disc_a(cx, cy, rc, pal.primary, 0.14);
        }
        self.colpi.push((Forma::Cerchio { cx, cy, r: rc + 4.0 }, Azione::ApriScelta));
    }

    fn modi(&mut self, p: &mut Painter, rc: R) {
        let pal = self.pal();
        p.rrect(rc.0, rc.1 + 4.0, rc.2, rc.3 - 8.0, (rc.3 - 8.0) / 2.0, Some(pal.alt), None);
        let mw = (rc.2 - 8.0) / 2.0;
        for (i, nome) in ["Converti", "Strumenti"].into_iter().enumerate() {
            let rr = (rc.0 + 4.0 + i as f32 * mw, rc.1 + 7.0, mw, rc.3 - 14.0);
            let on = self.modo == i as u8;
            if on {
                p.rrect(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, Some(pal.primary), None);
            } else if self.premuto == Some(Azione::Modo(i as u8)) {
                p.rrect_a(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, pal.primary, 0.14);
            }
            p.text(nome, rr.0, rr.1, rr.2, rr.3, 14.0, Weight::Semibold, if on { pal.on_primary } else { pal.text }, Align::Center);
            self.colpi.push((Forma::R((rr.0, rc.1, rr.2, rc.3)), Azione::Modo(i as u8)));
        }
    }

    /// L'azione scelta ora ha «Altre opzioni» (è un formato del pannello Conversioni).
    fn ha_opzioni(&self) -> bool {
        if self.file.as_ref().is_none_or(|f| e_link(&f.path)) {
            return false;
        }
        let id = if self.foglio { self.righe_ora().get(self.tessera).map(|r| r.id) } else { self.scelta.and_then(|i| self.fette().get(i).map(|f| f.id)) };
        let Some(id) = id else { return false };
        opzioni::formati(self.kind).iter().any(|s| s.id == id || (id == "jpg" && s.id == "jpeg"))
    }

    /// Il pulsante grande «Converti in X», con a fianco quello di «Altre opzioni» se servono.
    fn pulsante_vai(&mut self, p: &mut Painter, bt: R) {
        let pal = self.pal();
        let bt = if self.ha_opzioni() {
            let g = (bt.0, bt.1, bt.3, bt.3);
            let az = Azione::AltreOpzioni;
            p.circle(g.0 + g.2 / 2.0, g.1 + g.3 / 2.0, g.3 / 2.0 - 1.0, Some(if self.premuto == Some(az) { pal.alt } else { pal.surface }), Some((pal.primary, 2.0)));
            p.icon(ic::SETTINGS, g.0 + g.2 / 2.0 - 13.0, g.1 + g.3 / 2.0 - 13.0, 26.0, pal.primary);
            self.colpi.push((Forma::R(g), az));
            (bt.0 + bt.3 + 10.0, bt.1, bt.2 - bt.3 - 10.0, bt.3)
        } else {
            bt
        };
        let testo = if self.foglio {
            self.righe_ora().get(self.tessera).map(|r| match self.pdf.etichetta_vai(r.id).filter(|_| self.kind == Kind::Pdf) {
                Some(t) => t,
                None if r.conv => format!("Converti in {}", r.titolo),
                None => r.titolo.to_string(),
            })
        } else {
            self.scelta.and_then(|i| self.fette().get(i).copied()).map(|f| if self.modo == 1 { format!("Applica: {}", f.label) } else { format!("Converti in {}", f.label) })
        };
        let attivo = testo.is_some();
        let prem = attivo && self.premuto == Some(Azione::Vai);
        let fg = if attivo { pal.on_primary } else { pal.muted };
        if attivo {
            lucido(p, self.tema.dark(), bt.0, bt.1, bt.2, bt.3, bt.3 / 2.0, pal.primary, prem);
        } else {
            p.rrect(bt.0, bt.1, bt.2, bt.3, bt.3 / 2.0, Some(pal.alt), None);
        }
        let et = testo.unwrap_or_else(|| "Scegli uno strumento".into());
        let tw = p.measure(&et, 18.0, Weight::Semibold).min(bt.2 - 70.0);
        let ix = bt.0 + (bt.2 - tw - 34.0) / 2.0;
        p.icon(ic::CONVERT, ix, bt.1 + (bt.3 - 26.0) / 2.0, 26.0, fg);
        p.text(&et, ix + 34.0, bt.1, tw + 4.0, bt.3, 18.0, Weight::Semibold, fg, Align::Left);
        if attivo {
            self.colpi.push((Forma::R(bt), Azione::Vai));
        }
    }

    /// Il cursore della qualità (solo dove il motore la usa).
    fn slider(&mut self, p: &mut Painter, rc: R) {
        let pal = self.pal();
        if self.modello_serve() {
            // Trascrizione: due scelte al posto del cursore.
            p.text("Modello", rc.0, rc.1, 90.0, rc.3, 16.0, Weight::Regular, pal.text, Align::Left);
            let (x0, w) = (rc.0 + 92.0, rc.2 - 92.0);
            let cw = (w - 8.0) / 2.0;
            for (i, nome) in ["Veloce", "Buona"].into_iter().enumerate() {
                let cx = x0 + i as f32 * (cw + 8.0);
                let on = self.modello == i;
                p.rrect(cx, rc.1 + 4.0, cw, rc.3 - 8.0, (rc.3 - 8.0) / 2.0, Some(if on { pal.primary } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, 1.0)));
                p.text(nome, cx, rc.1 + 4.0, cw, rc.3 - 8.0, 15.0, Weight::Semibold, if on { pal.on_primary } else { pal.text }, Align::Center);
                self.colpi.push((Forma::R((cx, rc.1, cw, rc.3)), Azione::Modello(i)));
            }
            return;
        }
        p.text("Qualità", rc.0, rc.1, 90.0, rc.3, 16.0, Weight::Regular, pal.text, Align::Left);
        p.text(&self.qualita.to_string(), rc.0 + rc.2 - 40.0, rc.1, 40.0, rc.3, 16.0, Weight::Semibold, pal.text, Align::Right);
        let (tx, tw) = (rc.0 + 92.0, rc.2 - 92.0 - 52.0);
        let cy = rc.1 + rc.3 / 2.0;
        let f = (self.qualita as f32 - 40.0) / 55.0;
        p.rrect(tx, cy - 3.0, tw, 6.0, 3.0, Some(pal.border), None);
        p.rrect(tx, cy - 3.0, tw * f, 6.0, 3.0, Some(pal.primary), None);
        p.disc_a(tx + tw * f, cy + 1.5, 15.0, (0, 0, 0), 0.15);
        p.circle(tx + tw * f, cy, 13.0, Some(pal.surface), Some((pal.primary, 3.0)));
        self.cursore = (tx, cy, tw, 0.0);
        self.colpi.push((Forma::R((tx - 16.0, rc.1, tw + 32.0, rc.3)), Azione::Cursore));
    }

    /// «Ultimi file»: una riga di miniature (quante ne stanno in `w`).
    fn riga_ultimi(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, _spazio: f32) {
        let pal = self.pal();
        p.text("Ultimi file", x, y, w, 22.0, 13.0, Weight::Semibold, pal.muted, Align::Left);
        let cella = 64.0;
        let n = (((w + 8.0) / (cella + 8.0)).floor() as usize).max(1).min(self.recenti.len());
        let gap = if n > 1 { ((w - n as f32 * cella) / (n as f32 - 1.0)).min(14.0) } else { 0.0 };
        for i in 0..n {
            let rc = (x + i as f32 * (cella + gap), y + 24.0, cella, cella);
            self.cella_recente(p, i, rc, 10.0);
        }
    }

    fn cella_recente(&mut self, p: &mut Painter, i: usize, rc: R, raggio: f32) {
        let pal = self.pal();
        let rec = self.recenti[i].clone();
        p.rrect(rc.0, rc.1, rc.2, rc.3, raggio, Some(pal.alt), Some((pal.border, 1.0)));
        if let Some(img) = &rec.thumb {
            p.image(img, rc.0, rc.1, rc.2, rc.3, raggio, Fit::Cover);
        } else {
            p.icon(icona_tipo(rec.kind), rc.0 + rc.2 / 2.0 - 12.0, rc.1 + rc.3 / 2.0 - 20.0, 24.0, pal.primary);
        }
        let nome = p.ellipsize(&rec.nome, 10.0, Weight::Semibold, rc.2 - 8.0);
        p.rrect_a(rc.0 + 2.0, rc.1 + rc.3 - 18.0, rc.2 - 4.0, 16.0, raggio - 4.0, (0, 0, 0), 0.55);
        p.text(&nome, rc.0 + 4.0, rc.1 + rc.3 - 18.0, rc.2 - 8.0, 16.0, 10.0, Weight::Semibold, BIANCO, Align::Center);
        if self.premuto == Some(Azione::Recente(i)) {
            p.rrect_a(rc.0, rc.1, rc.2, rc.3, raggio, pal.primary, 0.2);
        }
        // Il bersaglio è almeno 48 dp anche con celle piccole.
        let (bw, bh) = (rc.2.max(TOCCO), rc.3.max(TOCCO));
        self.colpi.push((Forma::R((rc.0 - (bw - rc.2) / 2.0, rc.1 - (bh - rc.3) / 2.0, bw, bh)), Azione::Recente(i)));
    }

    // ----- «Cosa ne faccio?» -----

    fn pagina_foglio(&mut self, p: &mut Painter, w: f32, h: f32) {
        let orizz = w > h * 1.2;
        let file = self.file.clone().unwrap();
        self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::CLOSE, Azione::Esci);
        self.titolo_barra(p, 60.0, w - 72.0, "Cosa ne faccio?");
        let tutte = self.righe_ora();
        let serve_q = self.qualita_serve();

        let (bx, by, bw, bh, gx, gy, gw, gh): (f32, f32, f32, f32, f32, f32, f32, f32);
        let q_h = if serve_q { 52.0 } else { 0.0 };
        if orizz {
            let lw = (w * 0.38).max(220.0);
            bx = 12.0;
            by = BARRA;
            bw = lw - 12.0;
            bh = h - BARRA - 12.0 - q_h - 66.0;
            gx = lw + 8.0;
            gy = BARRA;
            gw = w - gx - 12.0;
            gh = h - BARRA - 12.0;
        } else {
            bx = 12.0;
            by = BARRA;
            bw = w - 24.0;
            bh = (h * 0.23).max(120.0);
            gx = 12.0;
            gy = BARRA + bh + 10.0;
            gw = w - 24.0;
            gh = h - gy - q_h - 66.0 - 8.0;
        }
        // Il file in grande, con nome, tipo e peso.
        self.anteprima(p, (bx, by, bw, bh), 16.0, false);
        let et = if e_link(&file.path) { "Video dal web".to_string() } else { format!("{} · {}", nome_tipo(self.kind), peso(file.dim)) };
        p.rrect_a(bx + 8.0, by + bh - 54.0, bw - 16.0, 46.0, 12.0, (0, 0, 0), 0.6);
        p.text(&file.nome, bx + 18.0, by + bh - 54.0, bw - 36.0, 26.0, 16.0, Weight::Semibold, BIANCO, Align::Left);
        p.text(&et, bx + 18.0, by + bh - 32.0, bw - 36.0, 20.0, 13.0, Weight::Regular, (235, 235, 240), Align::Left);

        // Le azioni, due colonne, tutte a vista.
        let con_altro = !e_link(&file.path);
        let n = tutte.len() + usize::from(con_altro);
        let righe_n = n.div_ceil(2);
        let gap = 8.0;
        let th = ((gh - gap * (righe_n as f32 - 1.0)) / righe_n as f32).clamp(52.0, 96.0);
        let tw = (gw - gap) / 2.0;
        for i in 0..n {
            let (c, rr) = (i % 2, i / 2);
            let rc = (gx + c as f32 * (tw + gap), gy + rr as f32 * (th + gap), tw, th);
            if i < tutte.len() {
                let t = tutte[i];
                self.tessera_vista(p, rc, icona_di(t.id), t.titolo, t.sotto, self.tessera == i, t.nuovo, Azione::Tessera(i));
            } else {
                self.tessera_vista(p, rc, ic::CONVERT, "Altro", sotto_altro(self.kind), false, false, Azione::Altro);
            }
        }

        // Qualità e pulsante, sempre in basso.
        let (cx, cw) = if orizz { (bx, bw) } else { (16.0, w - 32.0) };
        let mut cy = h - 66.0 - 8.0 - q_h;
        if orizz {
            cy = h - 12.0 - 66.0 - q_h + 10.0;
        }
        if serve_q {
            self.slider(p, (cx, cy, cw, TOCCO));
            cy += 52.0;
        }
        self.pulsante_vai(p, (cx, cy + if orizz { 0.0 } else { 4.0 }, cw, 56.0));
    }

    #[allow(clippy::too_many_arguments)]
    fn tessera_vista(&mut self, p: &mut Painter, rc: R, icona: &str, titolo: &str, sotto: &str, scelta: bool, nuovo: bool, az: Azione) {
        let pal = self.pal();
        let prem = self.premuto == Some(az);
        let fondo = if scelta { pal.primary } else if prem { mix(pal.surface, pal.primary, 0.14) } else { pal.surface };
        p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(fondo), Some((if scelta { pal.primary } else { pal.border }, 1.0)));
        let fg = if scelta { pal.on_primary } else { pal.text };
        let sg = if scelta { mix(pal.primary, pal.on_primary, 0.8) } else { pal.muted };
        // Il colore del tipo di file che esce (il colore del tema resta per la tessera scelta).
        let kc = casa::colore_kind(casa::kind_uscita(self.kind, self.righe_ora().get(match az { Azione::Tessera(i) => i, _ => usize::MAX }).map(|r| r.id).unwrap_or("")), self.tema.dark());
        let kc = if matches!(az, Azione::Tessera(_)) { kc } else { pal.primary };
        let caja = if scelta { mix(pal.primary, pal.on_primary, 0.25) } else { mix(pal.surface, kc, 0.16) };
        if rc.3 >= 84.0 {
            p.rrect(rc.0 + 12.0, rc.1 + 12.0, 38.0, 38.0, 11.0, Some(caja), None);
            p.icon(icona, rc.0 + 19.0, rc.1 + 19.0, 24.0, if scelta { pal.on_primary } else { kc });
            p.text(titolo, rc.0 + 12.0, rc.1 + rc.3 - 46.0, rc.2 - 20.0, 24.0, 17.0, Weight::Semibold, fg, Align::Left);
            p.text(sotto, rc.0 + 12.0, rc.1 + rc.3 - 26.0, rc.2 - 20.0, 20.0, 12.0, Weight::Regular, sg, Align::Left);
        } else {
            p.rrect(rc.0 + 10.0, rc.1 + (rc.3 - 36.0) / 2.0, 36.0, 36.0, 10.0, Some(caja), None);
            p.icon(icona, rc.0 + 16.0, rc.1 + (rc.3 - 24.0) / 2.0, 24.0, if scelta { pal.on_primary } else { kc });
            p.text(titolo, rc.0 + 54.0, rc.1 + rc.3 / 2.0 - 21.0, rc.2 - 60.0, 22.0, 16.0, Weight::Semibold, fg, Align::Left);
            p.text(sotto, rc.0 + 54.0, rc.1 + rc.3 / 2.0 + 1.0, rc.2 - 60.0, 20.0, 11.0, Weight::Regular, sg, Align::Left);
            if nuovo {
                let tw = p.measure(titolo, 16.0, Weight::Semibold);
                let nx = rc.0 + 54.0 + tw + 6.0;
                let nw = p.measure("NUOVO", 9.0, Weight::Semibold) + 2.0;
                if nx + nw < rc.0 + rc.2 - 4.0 {
                    p.text("NUOVO", nx, rc.1 + rc.3 / 2.0 - 21.0, nw, 22.0, 9.0, Weight::Semibold, if scelta { pal.on_primary } else { pal.ok }, Align::Left);
                }
            }
        }
        self.colpi.push((Forma::R(rc), az));
    }

    // ----- «Scegli il file» -----

    fn pagina_scelta(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let orizz = w > h * 1.2;
        self.titolo_barra(p, 16.0, w - 80.0, "Scegli il file");
        self.bottone_icona(p, (w - 52.0, 4.0, TOCCO, TOCCO), ic::CLOSE, Azione::ChiudiScelta);
        let voci = [(Sorgente::Foto, "Foto", ic::IMAGE), (Sorgente::Documenti, "Documenti", ic::FILE), (Sorgente::Audio, "Audio", ic::AUDIO), (Sorgente::Altri, "Altri file", ic::FOLDER), (Sorgente::Link, "Link dagli appunti", ic::VIDEO)];
        let (colonne, th) = if orizz { (4usize, 64.0) } else { (2usize, 72.0) };
        let gap = 10.0;
        let tw = (w - 24.0 - gap * (colonne as f32 - 1.0)) / colonne as f32;
        for (i, (s, nome, icona)) in voci.into_iter().enumerate() {
            let rc = (12.0 + (i % colonne) as f32 * (tw + gap), BARRA + (i / colonne) as f32 * (th + gap), tw, th);
            let az = Azione::Sorgente(s);
            let prem = self.premuto == Some(az);
            p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(if prem { mix(pal.surface, pal.primary, 0.14) } else { pal.surface }), Some((pal.border, 1.0)));
            p.rrect(rc.0 + 12.0, rc.1 + (th - 40.0) / 2.0, 40.0, 40.0, 12.0, Some(pal.alt), None);
            p.icon(icona, rc.0 + 20.0, rc.1 + (th - 24.0) / 2.0, 24.0, pal.primary);
            let righe = p.wrap_lines(nome, 17.0, Weight::Semibold, rc.2 - 70.0, 2).len().max(1) as f32;
            p.text_wrap(nome, rc.0 + 62.0, rc.1 + (th - righe * 22.0) / 2.0, rc.2 - 70.0, 17.0, Weight::Semibold, pal.text, 22.0, 2);
            self.colpi.push((Forma::R(rc), az));
        }
        let righe_voci = 5usize.div_ceil(colonne) as f32;
        let y = BARRA + righe_voci * (th + gap) + 8.0;
        p.text("Recenti", 16.0, y, w - 32.0, 24.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        if self.recenti.is_empty() {
            p.text_wrap("I file che scegli o condividi a Cedro compaiono qui.", 16.0, y + 34.0, w - 32.0, 14.0, Weight::Regular, pal.muted, 20.0, 2);
            return;
        }
        let cols = if orizz { 6usize } else { 3usize };
        let gapc = 10.0;
        let cw = (w - 24.0 - gapc * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw.min(112.0);
        let spazio_righe = (((h - y - 32.0 + gapc) / (ch + gapc)).floor() as usize).max(1);
        let n = self.recenti.len().min(cols * spazio_righe);
        for i in 0..n {
            let rc = (12.0 + (i % cols) as f32 * (cw + gapc), y + 30.0 + (i / cols) as f32 * (ch + gapc), cw, ch);
            self.cella_recente(p, i, rc, 14.0);
        }
    }

    // ----- Lavori -----

    fn pagina_lavori(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.titolo_barra(p, 16.0, w - 140.0, "Lavori");
        if self.lavori.iter().any(|l| l.stato != Stato::InCorso) {
            let rc = (w - 108.0, 4.0, 100.0, TOCCO);
            let prem = self.premuto == Some(Azione::Svuota);
            if prem {
                p.rrect_a(rc.0, rc.1, rc.2, rc.3, 24.0, pal.primary, 0.14);
            }
            p.text("Svuota", rc.0, rc.1, rc.2, rc.3, 15.0, Weight::Semibold, pal.primary, Align::Center);
            self.colpi.push((Forma::R(rc), Azione::Svuota));
        }
        let vista = (0.0, BARRA, w, h - BARRA);
        if self.lavori.is_empty() {
            p.icon(ic::CLOCK, w / 2.0 - 22.0, BARRA + (h - BARRA) / 2.0 - 70.0, 44.0, pal.muted);
            p.text("Ancora niente", 0.0, BARRA + (h - BARRA) / 2.0 - 16.0, w, 30.0, 20.0, Weight::Semibold, pal.text, Align::Center);
            p.text_wrap("Qui vedi i file in lavorazione, i risultati e la cronologia.", 32.0, BARRA + (h - BARRA) / 2.0 + 20.0, w - 64.0, 14.0, Weight::Regular, pal.muted, 20.0, 2);
            self.scorri_max = 0.0;
            return;
        }
        let ampio = w > 700.0;
        let cw = if ampio { (w - 32.0 - 12.0) / 2.0 } else { w - 24.0 };
        let colonne = if ampio { 2 } else { 1 };
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut ys = [BARRA + 6.0 - self.scorri; 2];
        let mut fine = 0.0f32;
        for k in 0..self.lavori.len() {
            let c = if colonne == 2 { if ys[0] <= ys[1] { 0 } else { 1 } } else { 0 };
            let x = if ampio { 16.0 + c as f32 * (cw + 12.0) } else { 12.0 };
            let alt = self.scheda_lavoro(p, k, x, ys[c], cw, vista);
            ys[c] += alt + 10.0;
            fine = fine.max(ys[c]);
        }
        p.pop_clip();
        self.scorri_max = (fine + self.scorri - (h - 6.0)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
    }

    /// Il testo da mandare quando un lavoro non riesce: cosa si faceva, l'errore con i dettagli e il telefono.
    fn rapporto_errore(&self, l: &Lavoro) -> String {
        let Stato::Errore(m) = &l.stato else { return String::new() };
        let (breve, dettagli) = dividi_errore(m);
        let titolo = if l.conv { format!("{} · {}", l.origine, l.etichetta) } else { format!("{}: {}", l.etichetta, l.origine) };
        let mut out = format!("Errore di Cedro per Android\nLavoro: {titolo}\nQuando: {}\nErrore: {breve}\n", quando(l.quando));
        if !dettagli.is_empty() {
            out += &format!("{dettagli}\n");
        }
        out + "\n" + &self.info.testo()
    }

    /// Una scheda di lavoro. Restituisce l'altezza.
    /// Il riquadro dell'avanzamento di una trascrizione, come sul computer (v0.32): cosa fa e la percentuale
    /// grande, su cosa lavora (processore e thread), la barra, il tempo che resta.
    fn avanzamento_trascrizione(&mut self, p: &mut Painter, l: &Lavoro, r: R) {
        let pal = self.pal();
        let scuro = self.tema.dark();
        let accento = casa::colore_att(casa::Att::Tras, scuro);
        let m = l.prog.load(Ordering::Relaxed);
        let pronto = m <= 1000;
        if pronto && !self.partenze.contains_key(&l.id) {
            self.partenze.insert(l.id, (self.ora, m));
        }
        p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(mix(accento, pal.surface, if scuro { 0.86 } else { 0.93 })), Some((mix(accento, pal.surface, 0.72), 1.0)));
        let (x, w) = (r.0 + 14.0, r.2 - 28.0);
        let perc = if pronto { m / 10 } else { 0 };
        let pct = format!("{perc}%");
        let pw = p.measure(&pct, 26.0, Weight::Semibold);
        p.text(&pct, x + w - pw - 2.0, r.1 + 8.0, pw + 4.0, 36.0, 26.0, Weight::Semibold, accento, Align::Left);
        let cosa = if perc == 0 { "Si prepara" } else { "Trascrive" };
        p.text(cosa, x, r.1 + 8.0, w - pw - 12.0, 22.0, 15.0, Weight::Semibold, pal.text, Align::Left);
        // La spia: su Android la trascrizione usa il processore (l'Adreno non ha il Vulkan che serve).
        let cy = r.1 + 41.0;
        p.disc_a(x + 4.0, cy, 6.5, pal.ok, 0.2);
        p.circle(x + 4.0, cy, 3.5, Some(pal.ok), None);
        p.text(&format!("Processore · {} thread", cedro::trascrizione::engine_threads()), x + 16.0, cy - 11.0, w - pw - 28.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        // La barra: binario chiaro, riempimento, un filo di luce e la testa chiara.
        let (by, bh) = (r.1 + 60.0, 12.0);
        p.rrect(x, by, w, bh, bh / 2.0, Some(mix(accento, pal.surface, 0.80)), None);
        let fw = (w * perc as f32 / 100.0).max(if perc > 0 { bh } else { 0.0 });
        if fw > 0.0 {
            p.rrect(x, by, fw, bh, bh / 2.0, Some(accento), None);
            p.rrect_a(x + 4.0, by + 2.0, (fw - 8.0).max(0.0), 3.0, 1.5, (255, 255, 255), 0.28);
            if fw > bh + 6.0 {
                p.disc_a(x + fw - bh / 2.0, by + bh / 2.0, 2.5, (255, 255, 255), 0.9);
            }
        } else {
            // Prima della prima percentuale (modello che si scarica o si carica) la barra scorre.
            p.push_clip(x, by, w, bh);
            let t = (self.ora.duration_since(l.inizio).as_secs_f32() * 0.9).fract();
            let sw = w * 0.3;
            p.rrect(x - sw + (w + sw) * t, by, sw, bh, bh / 2.0, Some(accento), None);
            p.pop_clip();
        }
        let resta = if perc == 0 {
            "Accendo il motore…".to_string()
        } else {
            self.partenze.get(&l.id).and_then(|&(t0, m0)| {
                let spesi = self.ora.duration_since(t0).as_secs_f32();
                (m >= m0 + 50 && spesi >= 10.0 && m < 1000).then(|| format!("Restano {}", cedro::ui::model::time_left((1000 - m) as f32 * spesi / (m - m0) as f32)))
            })
            .unwrap_or_else(|| "Calcolo il tempo che resta…".into())
        };
        p.text(&resta, x, by + bh + 6.0, w, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
    }

    fn scheda_lavoro(&mut self, p: &mut Painter, k: usize, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let l = self.lavori[k].clone();
        let trascrive = l.sorgente.is_some();
        let alt = match &l.stato {
            Stato::InCorso if trascrive => 168.0,
            Stato::InCorso => 92.0,
            // Un link passato a un'altra app: non c'è un file da aprire o condividere.
            Stato::Fatto { path, .. } if path.as_os_str().is_empty() => 70.0,
            Stato::Fatto { .. } if l.sorgente.is_some() && l.controlli.is_some_and(|n| n > 0) => 176.0,
            Stato::Fatto { .. } => 142.0,
            Stato::Errore(_) => 112.0,
        };
        sollevato(p, pal, self.tema.dark(), x, y, w, alt, 16.0);
        let titolo = if l.conv { format!("{} · {}", l.origine, l.etichetta) } else { format!("{}: {}", l.etichetta, l.origine) };
        let (stato_txt, stato_col) = match &l.stato {
            Stato::InCorso => ("In corso".to_string(), pal.primary),
            Stato::Fatto { .. } => ("Fatto".to_string(), pal.ok),
            Stato::Errore(_) => ("Non riuscito".to_string(), pal.bad),
        };
        let sw = p.measure(&stato_txt, 14.0, Weight::Semibold) + 4.0;
        let nome = p.ellipsize(&titolo, 16.0, Weight::Semibold, w - sw - 40.0);
        p.text(&nome, x + 14.0, y + 8.0, w - sw - 40.0, 28.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        p.text(&stato_txt, x + w - sw - 14.0, y + 8.0, sw, 28.0, 14.0, Weight::Semibold, stato_col, Align::Right);
        match &l.stato {
            Stato::InCorso if trascrive => self.avanzamento_trascrizione(p, &l, (x + 12.0, y + 42.0, w - 24.0, alt - 54.0)),
            Stato::InCorso => {
                let (bx, by, bw) = (x + 14.0, y + 44.0, w - 28.0);
                p.rrect(bx, by, bw, 8.0, 4.0, Some(pal.alt), None);
                let m = l.prog.load(Ordering::Relaxed);
                let s = self.ora.duration_since(l.inizio).as_secs();
                if m <= 1000 {
                    p.rrect(bx, by, (bw * m as f32 / 1000.0).max(8.0), 8.0, 4.0, Some(pal.primary), None);
                    let resto = if m >= 50 { format!(" · circa {} s alla fine", (s as f32 * (1000 - m) as f32 / m as f32).round() as u64) } else { String::new() };
                    p.text(&format!("{}%{resto}", m / 10), x + 14.0, y + 58.0, w - 28.0, 24.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                } else {
                    p.push_clip(bx, by, bw, 8.0);
                    let t = (self.ora.duration_since(l.inizio).as_secs_f32() * 0.9).fract();
                    let sw = bw * 0.38;
                    p.rrect(bx - sw + (bw + sw) * t, by, sw, 8.0, 4.0, Some(pal.primary), None);
                    p.pop_clip();
                    p.text(&format!("Lavoro in corso da {s} s"), x + 14.0, y + 58.0, w - 28.0, 24.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                }
            }
            Stato::Fatto { nome, msg, path } => {
                p.text(&format!("{} · {}", msg.replace(" → ", " a "), quando(l.quando)), x + 14.0, y + 34.0, w - 28.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                if path.as_os_str().is_empty() {
                    return alt;
                }
                p.text(&format!("Salvato in {nome}"), x + 14.0, y + 54.0, w - 28.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                if let Some(n) = l.controlli.filter(|n| *n > 0 && l.sorgente.is_some()) {
                    let et = if n == 1 { "1 punto da controllare".to_string() } else { format!("{n} punti da controllare") };
                    let ew = p.measure(&et, 13.0, Weight::Semibold) + 22.0;
                    p.rrect(x + 14.0, y + 80.0, ew, 28.0, 14.0, Some(pal.alt), None);
                    p.text(&et, x + 14.0, y + 80.0, ew, 28.0, 13.0, Weight::Semibold, self.avviso(), Align::Center);
                }
                let mut voci = vec![("Apri", Azione::Apri(l.id)), ("Condividi", Azione::Condividi(l.id))];
                if l.sorgente.is_some() {
                    voci.insert(0, ("Rivedi", Azione::Rivedi(l.id)));
                }
                let bw = (w - 28.0 - 10.0 * (voci.len() as f32 - 1.0)) / voci.len() as f32;
                for (i, (et, az)) in voci.into_iter().enumerate() {
                    let rc = (x + 14.0 + i as f32 * (bw + 10.0), y + alt - 14.0 - TOCCO, bw, TOCCO);
                    let prem = self.premuto == Some(az);
                    let pieno = i == 0;
                    if pieno {
                        lucido(p, self.tema.dark(), rc.0, rc.1, rc.2, rc.3, 24.0, pal.primary, prem);
                    } else {
                        p.rrect(rc.0, rc.1, rc.2, rc.3, 24.0, Some(if prem { pal.alt } else { pal.surface }), Some((pal.primary, 1.5)));
                    }
                    p.text(et, rc.0, rc.1, rc.2, rc.3, 16.0, Weight::Semibold, if pieno { pal.on_primary } else { pal.primary }, Align::Center);
                    if let Some(r) = intersezione(rc, vista) {
                        self.colpi.push((Forma::R(r), az));
                    }
                }
            }
            Stato::Errore(m) => {
                let (breve, _) = dividi_errore(m);
                p.text_wrap(breve, x + 14.0, y + 38.0, w - 28.0 - 52.0, 13.0, Weight::Regular, pal.bad, 18.0, 3);
                let az = Azione::CopiaErrore(l.id);
                let rc = (x + w - 14.0 - 48.0, y + alt - 10.0 - 48.0, 48.0, 48.0);
                if self.premuto == Some(az) {
                    p.rrect(rc.0, rc.1, rc.2, rc.3, 24.0, Some(pal.alt), None);
                }
                let icona = if self.copiato == Some(l.id) { ic::CHECK } else { ic::COPY };
                p.icon(icona, rc.0 + 12.0, rc.1 + 12.0, 24.0, pal.primary);
                if let Some(r) = intersezione(rc, vista) {
                    self.colpi.push((Forma::R(r), az));
                }
            }
        }
        alt
    }

    // ----- Informazioni -----

    /// Una scheda con righe «nome … valore». Restituisce l'altezza.
    fn riquadro(&self, p: &mut Painter, (x, y, w): (f32, f32, f32), righe: &[(String, String, Option<bool>)]) -> f32 {
        let pal = self.pal();
        let rh = 42.0;
        let h = righe.len() as f32 * rh + 4.0;
        sollevato(p, pal, self.tema.dark(), x, y, w, h, 16.0);
        for (i, (k, v, si)) in righe.iter().enumerate() {
            let ry = y + 2.0 + i as f32 * rh;
            if i > 0 {
                p.fill(x + 14.0, ry, w - 28.0, 1.0, pal.border);
            }
            p.text(k, x + 14.0, ry, w * 0.4, rh, 15.0, Weight::Regular, pal.muted, Align::Left);
            let col = match si {
                Some(true) => (0x12, 0x80, 0x5c),
                Some(false) => pal.muted,
                None => pal.text,
            };
            p.text(v, x + w * 0.4 + 6.0, ry, w * 0.6 - 20.0, rh, 15.0, Weight::Semibold, col, Align::Right);
        }
        h
    }

    fn pagina_info(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.titolo_barra(p, 16.0, w - 32.0, "Informazioni");
        let ampio = w > 700.0;
        let cw = if ampio { 520.0 } else { w - 24.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 12.0 };
        let vista = (0.0, BARRA, w, h - BARRA);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut y = BARRA + 4.0 - self.scorri;
        let generali: Vec<_> = self.info.generali.iter().map(|(k, v)| (k.clone(), v.clone(), None)).collect();
        y += self.riquadro(p, (x, y, cw), &generali) + 8.0;
        p.text("Istruzioni del processore", x + 4.0, y, cw, 26.0, 13.0, Weight::Semibold, pal.muted, Align::Left);
        y += 28.0;
        let istr: Vec<_> = self.info.istruzioni.iter().map(|(n, si)| (n.clone(), (if *si { "sì" } else { "no" }).to_string(), Some(*si))).collect();
        y += self.riquadro(p, (x, y, cw), &istr) + 8.0;
        p.text("Accelerazioni usate", x + 4.0, y, cw, 26.0, 13.0, Weight::Semibold, pal.muted, Align::Left);
        y += 28.0;
        let acc: Vec<_> = self.info.accelerazioni.iter().map(|(n, v, on)| (n.clone(), v.clone(), Some(*on))).collect();
        y += self.riquadro(p, (x, y, cw), &acc) + 10.0;
        let rc = (x, y, cw, 48.0);
        let prem = self.premuto == Some(Azione::CopiaInfo);
        p.rrect(rc.0, rc.1, rc.2, rc.3, 14.0, Some(if prem { mix(pal.primary, pal.surface, 0.2) } else { pal.primary }), None);
        p.text("Copia le informazioni", rc.0, rc.1, rc.2, rc.3, 16.0, Weight::Semibold, pal.on_primary, Align::Center);
        self.colpi.push((Forma::R((rc.0, rc.1, rc.2, rc.3)), Azione::CopiaInfo));
        y += rc.3 + 14.0;
        p.pop_clip();
        self.scorri_max = (y + self.scorri - h).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
    }

    // ----- Aspetto -----

    fn pagina_aspetto(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.titolo_barra(p, 16.0, w - 32.0, "Aspetto");
        let rh = if h < 420.0 { 60.0 } else { 80.0 };
        let ampio = w > 700.0;
        let cw = if ampio { 520.0 } else { w - 24.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 12.0 };
        p.push_clip(0.0, BARRA, w, h - BARRA);
        let mut y = BARRA - self.scorri;
        p.text("Tema", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 32.0;
        for (i, tema) in TEMI.into_iter().enumerate() {
            let rc = (x, y + i as f32 * rh, cw, rh - 8.0);
            let attivo = tema == self.tema;
            let prem = self.premuto == Some(Azione::Tema(i));
            let pa = tema.palette();
            p.rrect(rc.0, rc.1, rc.2, rc.3, 18.0, Some(if prem { mix(pal.alt, pal.primary, 0.2) } else { pal.surface }), Some((if attivo { pal.primary } else { pal.border }, if attivo { 2.0 } else { 1.0 })));
            let cy = rc.1 + rc.3 / 2.0;
            p.circle(rc.0 + 32.0, cy, 18.0, Some(pa.bg), Some((pa.border, 1.5)));
            p.disc_a(rc.0 + 32.0, cy, 10.0, pa.primary, 1.0);
            // Nome, carattere e «Àa» nel carattere di quel tema.
            let prima = p.swap_fonts(cedro::ui::gfx::Fonts::for_theme(tema));
            let aw = p.measure("Àa", 24.0, Weight::Bold) + 8.0;
            p.text(tema.name(), rc.0 + 62.0, cy - 24.0, rc.2 - 80.0 - aw, 26.0, 17.0, Weight::Semibold, pal.text, Align::Left);
            p.text(nome_carattere(tema), rc.0 + 62.0, cy + 1.0, rc.2 - 80.0 - aw, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
            p.text("Àa", rc.0 + rc.2 - 14.0 - aw, rc.1, aw, rc.3, 24.0, Weight::Bold, pal.text, Align::Right);
            p.swap_fonts(prima);
            self.colpi.push((Forma::R((rc.0, rc.1 - 2.0, rc.2, rc.3 + 4.0)), Azione::Tema(i)));
        }
        y += TEMI.len() as f32 * rh + 4.0;
        p.text("Caratteri", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 32.0;
        self.pillole(p, (x, y, cw, TOCCO), &["Normali", "Grandi"], usize::from(self.grandi), |i| Azione::Caratteri(i == 1));
        y += TOCCO + 6.0;
        y += p.text_wrap("Ogni tema ha il suo carattere, come sul computer. «Grandi» ingrandisce tutto il testo di Cedro.", x + 4.0, y, cw - 8.0, 13.0, Weight::Regular, pal.muted, 19.0, 3) + 10.0;
        p.text("Lingua", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        for (i, (chiave, nome)) in LINGUE.iter().enumerate() {
            let rc = (x, y + 32.0 + i as f32 * 56.0, cw, 48.0);
            let attivo = self.lingua == *chiave;
            let prem = self.premuto == Some(Azione::Lingua(i));
            p.rrect(rc.0, rc.1, rc.2, rc.3, 14.0, Some(if prem { mix(pal.alt, pal.primary, 0.2) } else { pal.surface }), Some((if attivo { pal.primary } else { pal.border }, if attivo { 2.0 } else { 1.0 })));
            p.text(&if i == 0 { nome.to_string() } else { cedro::i18n::lit(nome) }, rc.0 + 18.0, rc.1, rc.2 - 70.0, rc.3, 17.0, Weight::Semibold, pal.text, Align::Left);
            if attivo {
                p.icon(ic::CHECK, rc.0 + rc.2 - 40.0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, pal.primary);
            }
            self.colpi.push((Forma::R((rc.0, rc.1 - 2.0, rc.2, rc.3 + 4.0)), Azione::Lingua(i)));
        }
        let mut y = y + 32.0 + LINGUE.len() as f32 * 56.0 + 6.0;
        let rc = (x, y, cw, 64.0);
        let az = Azione::Opz(opzioni::OpzAz::ApriImpostazioni);
        p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(if self.premuto == Some(az) { mix(pal.alt, pal.primary, 0.2) } else { pal.surface }), Some((pal.border, 1.0)));
        p.text("Impostazioni", rc.0 + 18.0, rc.1 + 8.0, rc.2 - 70.0, 26.0, 17.0, Weight::Semibold, pal.text, Align::Left);
        p.text("Nome dei file, qualità, OCR", rc.0 + 18.0, rc.1 + 34.0, rc.2 - 70.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        p.icon("M9 6l6 6-6 6", rc.0 + rc.2 - 40.0, rc.1 + 20.0, 24.0, pal.muted);
        self.colpi.push((Forma::R(rc), az));
        y += 76.0;
        let fine = y + p.text_wrap("Le copie vanno nella cartella Documenti. Il tema scelto resta tra un avvio e l'altro.", x + 4.0, y, cw - 8.0, 13.0, Weight::Regular, pal.muted, 19.0, 3);
        p.pop_clip();
        self.scorri_max = (fine + 16.0 + self.scorri - h).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
    }

    /// Una scelta a pillole («Normali | Grandi»): la voce `scelta` è piena.
    fn pillole(&mut self, p: &mut Painter, rc: R, voci: &[&str], scelta: usize, az: impl Fn(usize) -> Azione) {
        let pal = self.pal();
        p.rrect(rc.0, rc.1, rc.2, rc.3, rc.3 / 2.0, Some(pal.alt), None);
        let n = voci.len().max(1) as f32;
        // Larghezze uguali se le scritte ci stanno, altrimenti in proporzione alle scritte.
        let misure: Vec<f32> = voci.iter().map(|v| p.measure(v, 15.0, Weight::Semibold) + 20.0).collect();
        let uguale = (rc.2 - 8.0) / n;
        let larghe: Vec<f32> = if misure.iter().all(|m| *m <= uguale) {
            vec![uguale; voci.len()]
        } else {
            let tot: f32 = misure.iter().sum();
            misure.iter().map(|m| m / tot * (rc.2 - 8.0)).collect()
        };
        let mut px = rc.0 + 4.0;
        for (i, nome) in voci.iter().enumerate() {
            let vw = larghe[i];
            let rr = (px, rc.1 + 4.0, vw, rc.3 - 8.0);
            px += vw;
            let on = i == scelta;
            let a = az(i);
            if on {
                p.rrect(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, Some(pal.primary), None);
            } else if self.premuto == Some(a) {
                p.rrect_a(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, pal.primary, 0.14);
            }
            p.text(nome, rr.0 + 4.0, rr.1, rr.2 - 8.0, rr.3, 15.0, Weight::Semibold, if on { pal.on_primary } else { pal.text }, Align::Center);
            self.colpi.push((Forma::R((rr.0, rc.1, rr.2, rc.3)), a));
        }
    }
}

/// Il carattere di ogni tema, come lo dice «Aspetto».
fn nome_carattere(t: ThemeId) -> &'static str {
    match t {
        ThemeId::WindowsEssenziale => "Roboto, quello del telefono",
        ThemeId::PercorsoGuidato => "Atkinson Hyperlegible",
        ThemeId::CedroCompatto => "Figtree",
        ThemeId::BancoDiLavoro => "IBM Plex Sans",
        ThemeId::DocumentoAlCentro => "Source Sans 3",
        ThemeId::Bondeno => "Titillium Web",
    }
}

/// Il contorno della fetta tra due raggi e due angoli, con uno spazio costante `gap` da ogni lato.
fn sector(cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32, gap: f32) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    // Un punto ogni 2 px circa sull'arco esterno: con pochi punti una fetta grande (una sola azione = tutto il giro) diventa un poligono.
    let steps = ((a1 - a0).abs() * r1 / 2.0).ceil().clamp(24.0, 1440.0) as usize;
    let (s0, s1) = ((gap / r1.max(gap + 1.0)).asin(), (gap / r0.max(gap + 1.0)).asin());
    for k in 0..=steps {
        let a = a0 + s0 + (a1 - a0 - 2.0 * s0) * k as f32 / steps as f32;
        pts.push((cx + r1 * a.cos(), cy + r1 * a.sin()));
    }
    for k in (0..=steps).rev() {
        let a = a0 + s1 + (a1 - a0 - 2.0 * s1) * k as f32 / steps as f32;
        pts.push((cx + r0 * a.cos(), cy + r0 * a.sin()));
    }
    pts
}
