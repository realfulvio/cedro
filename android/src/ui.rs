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

pub mod anim;
pub mod tinte;
mod casa;
mod web;
mod condivisione;
pub mod firma;
mod m3;
pub mod labs;

/// Quante attività ha questa build (la Firma digitale è l'ultima).
pub fn attivita_n() -> usize {
    casa::attivita().len()
}
mod opzioni;
mod pdf;
mod rivedi;
pub use pdf::{Campo, Pagina, StatoPdf, Tasto};
pub use rivedi::{da_controllare, FoglioRiv};

pub type R = (f32, f32, f32, f32);

/// Catalogo Android esplicito, indipendente dal sistema usato per compilare il renderer.
pub const TEMI: [ThemeId; 9] = cedro::ui::theme::ANDROID_THEMES;

/// La chiave salvata in tema.txt, valida anche nelle anteprime e nei test su host.
pub fn tema_da_chiave(key: &str) -> ThemeId {
    ThemeId::from_android_key(key)
}
/// Le prime quattro sono le tinte del Cedro di partenza; le altre i temi di prima.
const TINTE: usize = 4;

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

/// Stato e lista uscite viaggiano insieme: nessun interleaving fra code distinte.
#[derive(Clone)]
pub struct Completamento {
    pub batch:Option<crate::web::Batch>,
    pub id: u32,
    pub stato: Stato,
    pub uscite: Option<Vec<crate::publication::Riga>>,
    pub sorgente: Option<PathBuf>,
}
impl Completamento {
    pub fn semplice(id:u32,stato:Stato)->Self{Self{batch:None,id,stato,uscite:None,sorgente:None}}
    pub fn recuperato(id:u32,r:crate::publication::Registro)->Self{
        let outputs=r.righe();
        if let Some((path,nome,_))=outputs.first().cloned(){
            let msg=if outputs.iter().any(|u|u.2.is_some()){format!("{} · {}",r.messaggio,cedro::i18n::tr("Salvataggio incompleto: copie disponibili in Cedro"))}else{r.messaggio};
            Self{batch:None,id,stato:Stato::Fatto{path,nome,msg},uscite:Some(outputs),sorgente:r.lavoro.sorgente}
        }else{
            Self{batch:None,id,stato:Stato::Errore(r.messaggio),uscite:None,sorgente:r.lavoro.sorgente}
        }
    }
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
    /// Un file firmato da aprire nel visualizzatore.
    Firmati,
    /// Il file originale di una firma staccata (.p7s) o di una marca a parte (.tsr).
    Originale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Azione {
    Web(web::WebAz),
    Scheda(Scheda),
    Tessera(usize),
    /// «Altro»: la ruota con tutti i formati.
    Altro,
    Esci,
    Fetta(usize),
    /// Scelta esplicita nel catalogo del foglio A, senza passare dalla ruota.
    FoglioFormato(u8, usize),
    Modo(u8),
    Vai,
    Cursore,
    /// Il modello di trascrizione: 0 Veloce (small), 1 Buona (medium), 2 Massima (large-v3-turbo).
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
    CopiaRapporto(u32),
    IndietroFoglio,
    ApriScelta,
    ChiudiScelta,
    Sorgente(Sorgente),
    Recente(usize),
    Apri(u32),
    Annulla(u32),
    ApriUscita(u32, usize),
    CondividiUscita(u32, usize),
    ErroreUscita(u32, usize),
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
    /// La firma digitale: la sezione e il visualizzatore.
    Firma(firma::FirmaAz),
    /// Laboratorio: anteprima e caso, senza I/O nel loop UI.
    Labs(labs::LabAz),
    Pacchetto,
    /// Ferma il tocco senza compiere azioni.
    Niente,
}

/// Un lavoro da far partire col motore.
pub struct Compito {
    pub voce: crate::worker::Voce,
    pub web: Option<crate::web::Scelte>,
    pub id: u32,
    pub file: PathBuf,
    pub azione: &'static str,
    pub qualita: u8,
    pub prog: Arc<AtomicU32>,
    pub opts: Option<Box<cedro::Options>>,
    pub unisci: Vec<(PathBuf, Option<cedro::PageSel>)>,
}

pub enum Effetto {
    SalvaRete,
    Annulla(u32),
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
    Converti { voce:crate::worker::Voce, web:Option<crate::web::Scelte>, id: u32, file: PathBuf, azione: &'static str, qualita: u8, prog: Arc<AtomicU32>, opts: Option<Box<cedro::Options>>, unisci: Vec<(PathBuf, Option<cedro::PageSel>)> },
    /// Prova la password del PDF: se va, se ne fa una copia senza.
    Sblocca(String),
    /// Legge l'onda dell'audio di una revisione (percorso, durata).
    OndaRevisione(PathBuf, f64),
    /// Le impostazioni sono cambiate: vanno ricordate (`imp` nel motore, modello e «se esiste già» qui).
    SalvaImpostazioni,
    SalvaCaso,
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
    /// Come `Scrivi`, poi apre (1) o condivide (2) il file scritto.
    ScriviPoi { id: u32, nome: String, dati: Vec<u8>, poi: u8 },
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
    static CATALOGO: std::sync::OnceLock<Vec<(Kind,Vec<Riga>)>> = std::sync::OnceLock::new();
    CATALOGO.get_or_init(||[Kind::Image,Kind::Pdf,Kind::Doc,Kind::Text,Kind::Audio,Kind::Video,Kind::Archive,Kind::Other].into_iter().map(|k|{
        let mut rows=Vec::new();
        for (conv,entries) in [(true,cedro::panel_slices(k)),(false,cedro::slices(k,Mode::Tools))] {
            for entry in entries {
                if entry.id=="none" || rows.iter().any(|r:&Riga|r.id==entry.id){continue;}
                let v=cedro::ui::voci::voce(k,entry.id);
                rows.push(r(entry.id,v.label,v.hint,conv));
            }
        }
        if k==Kind::Pdf {rows.push(r("pages","Pagine","Ruota, togli, numeri",false));rows.push(r("firme","Firme","Verifica chi ha firmato",false));}
        if matches!(k,Kind::Audio|Kind::Video){rows.push(r("trascrivi","Testo","Dalla voce, offline",false));}
        (k,rows)
    }).collect()).iter().find(|(k,_)|*k==kind).map(|(_,rows)|rows.iter().collect()).unwrap_or_default()
}

fn avviso_motore(msg:&str)->bool {
    msg.contains("Attenzione:") || msg.contains("restano solo nel file originale") || msg.contains("non è una diagnosi") || msg.contains("non sono stati copiati")
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
        "half" => ic::DOWNLOAD,
        "strip" => cedro::ui::voci::voce(Kind::Image, "strip").glyph.stroke,
        "rot90" => ic::CONVERT,
        "xlsx" => pdf::I_EXCEL,
        "merge" => pdf::I_UNISCI,
        "split" => pdf::I_DIVIDI,
        "pages" => pdf::I_PAGINE,
        "firme" => ic::SHIELD,
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
    pub rete:u8,
    pub download_attesa:Option<crate::web::AttesaDownload>,
    pub web:crate::web::Scelte,
    pub componente:Option<crate::web::Componente>,
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
    /// Modello di trascrizione scelto (indice in `trascrizione::MODELS`: 0 Veloce, 1 Buona, 2 Massima).
    pub modello: usize,
    /// Quante persone parlano (trascrizione): 0 Automatico, 1 una sola (niente riconoscimento), 2..=8.
    pub voci: u8,
    pub referto: bool,
    pub tracce: std::collections::HashMap<PathBuf, rivedi::Traccia>,
    pub lavori: Vec<Lavoro>,
    pub labs: labs::Laboratorio,
    pub uscite: std::collections::HashMap<u32, Vec<(PathBuf, String, Option<String>)>>,
    chiavi_lavori: std::collections::HashMap<u32,String>,
    lavori_rimossi: std::collections::HashSet<String>,
    annullamenti: std::collections::HashSet<u32>,
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
    /// Le piccole animazioni (fogli, schede, tocchi).
    pub anim: anim::Anim,
    /// La firma digitale (stato del computer: verifiche, ultimi file, visualizzatore).
    pub firma: cedro::ui::firma::FirmaUi,
    /// Le pagine del documento firmato mostrato.
    pub fpag: firma::Pagine,
    /// Sveglia il ciclo degli eventi (una verifica finita in un thread).
    pub sveglia: Arc<dyn Fn() + Send + Sync>,
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
    pub anteprima_tras: Option<rivedi::Rivedi>,
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
    /// Cosa sta facendo una trascrizione prima della prima percentuale (scarica il modello, rete interrotta…).
    pub passi: std::collections::HashMap<u32, String>,
    pub casa: casa::Casa,
    /// Mentre si disegna il pannello chiaro del nuovo Cedro: il colore della pagina (le altre funzioni leggono i colori
    /// da `pal()`, che lì dà la palette chiara con questo accento).
    pannello: Option<(u8, u8, u8)>,
    /// Lavori, Aspetto o Informazioni sopra la finestra principale, con la freccia per tornare.
    pub sopra: Option<Scheda>,
    /// Le pagine dei PDF in coda.
    pub pdf_pagine: std::collections::HashMap<PathBuf, u32>,
}

/// Le lingue tra cui scegliere: chiave nel file, nome (quelli delle lingue non si traducono).
pub const LINGUE: [(&str, &str); 7] = [("auto", "Lingua del telefono"), ("it", "Italiano"), ("en", "English"), ("es", "Español"), ("de", "Deutsch"), ("fr", "Français"), ("pt", "Português")];

impl App {
    /// Mette in uso la lingua scelta (per «auto», quella del telefono).
    pub fn applica_lingua(&self) {
        cedro::i18n::set(cedro::i18n::Lang::ALL.into_iter().find(|l| l.code() == self.lingua).unwrap_or(self.lingua_telefono));
    }

    pub fn new(tema: ThemeId) -> App {
        App {
            web:Default::default(),componente:None,rete:0,download_attesa:None,
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
            modello: modello_iniziale(),
            voci: 0,
            referto: false,
            tracce: Default::default(),
            lavori: Vec::new(),
            uscite: Default::default(),
            chiavi_lavori: Default::default(),
            lavori_rimossi: Default::default(),
            annullamenti: Default::default(),
            copiato: None,
            recenti: Vec::new(),
            dal_foglio: false,
            prossimo: 1,
            scorri: 0.0,
            scorri_max: 0.0,
            trascina: None,
            trascina_x: None,
            premuto: None,
            anim: anim::Anim::default(),
            firma: cedro::ui::firma::FirmaUi::default(),
            fpag: firma::Pagine::default(),
            sveglia: Arc::new(|| {}),
            colpi: Vec::new(),
            fasce: Vec::new(),
            cursore: (0.0, 0.0, 1.0, 1.0),
            ora: Instant::now(),
            margini: (0.0, 0.0, 0.0, 0.0),
            pdf: StatoPdf::default(),
            campo: None,
            mostra_tastiera: false,
            rivedi: None,
            anteprima_tras: None,
            opzioni: None,
            opzioni_salvate: None,
            impostazioni: false,
            labs: labs::Laboratorio::default(),
            imp: cedro::Settings::default(),
            modello_nome: String::new(),
            esiste: cedro::Exists::Number,
            altri: Vec::new(),
            casa_aperta: true,
            partenze: std::collections::HashMap::new(),
            passi: std::collections::HashMap::new(),
            casa: casa::Casa::default(),
            pannello: None,
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

    pub fn identita(&mut self,id:u32)->Option<crate::publication::Identita>{
        let l=self.lavori.iter().find(|l|l.id==id)?;
        let chiave=self.chiavi_lavori.entry(id).or_insert_with(||format!("{:x}-{id:x}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos())).clone();
        Some(crate::publication::Identita{chiave,origine:l.origine.clone(),etichetta:l.etichetta.clone(),conv:l.conv,quando:l.quando,sorgente:l.sorgente.clone(),caso:self.labs.casi.get(&id).cloned().unwrap_or_default()})
    }

    pub fn completa(&mut self,e:Completamento){
        if let Some(outputs)=e.uscite{self.uscite.insert(e.id,outputs);}
        if let Some(source)=e.sorgente{if let Some(l)=self.lavori.iter_mut().find(|l|l.id==e.id){l.sorgente=Some(source);}}
        self.finisce(e.id,e.stato);
    }

    pub fn recupera(&mut self,r:crate::publication::Registro){
        if self.lavori_rimossi.contains(&r.lavoro.chiave){return;}
        let id=match self.chiavi_lavori.iter().find(|(_,k)|k.as_str()==r.lavoro.chiave.as_str()).map(|(id,_)|*id){
            Some(id)=>id,
            None=>{
                let (id,_)=self.nuova_importazione();
                self.chiavi_lavori.insert(id,r.lavoro.chiave.clone());
                if let Some(l)=self.lavori.iter_mut().find(|l|l.id==id){l.origine=r.lavoro.origine.clone();l.etichetta=r.lavoro.etichetta.clone();l.conv=r.lavoro.conv;l.quando=r.lavoro.quando;}
                id
            }
        };
        if !r.lavoro.caso.is_empty(){self.labs.casi.insert(id,r.lavoro.caso.clone());}
        self.completa(Completamento::recuperato(id,r));
    }

    /// Un lavoro è finito (o fallito).
    pub fn finisce(&mut self, id: u32, stato: Stato) -> Effetto {
        let stato = if matches!(&stato,Stato::Errore(e) if e=="nessun parlato riconosciuto nell'audio") {
            let secs=self.lavori.iter().find(|l|l.id==id).and_then(|l|l.sorgente.as_ref()).and_then(|p|self.tracce.get(p)).map_or(0.0,|t|t.secs);
            Stato::Fatto {path:PathBuf::new(),nome:String::new(),msg:format!("{} · {:.1} s. {}",cedro::i18n::tr("Nessun parlato riconosciuto"),secs,cedro::i18n::tr("Prova una registrazione con la voce più vicina al microfono."))}
        } else {stato};
        self.fine_trascrizione(id,matches!(&stato,Stato::Fatto{path,..} if !path.as_os_str().is_empty()));
        self.annullamenti.remove(&id);
        self.salvata(id, &stato);
        if let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.riassunto.as_ref().is_some_and(|r| r.0 == id)) {
            rv.riassunto = None;
            if let Stato::Errore(e) = &stato {
                rv.riassunto_nota = dividi_errore(e).0.to_string();
            }
        }
        if let Some(l) = self.lavori.iter_mut().find(|l| l.id == id) {
            l.stato = stato;
            l.quando = adesso(); // «finito N min fa», non da quando è partito
        }
        Effetto::SalvaLavori
    }

    /// Vero se serve ridisegnare a ogni fotogramma (barra di avanzamento che scorre).
    pub fn animando(&mut self) -> bool {
        if self.anim.in_moto() || self.firma.busy() || self.firma.viewer.is_some() && !self.fpag.finite {
            return true;
        }
        if std::mem::take(&mut self.pdf.ridisegna) {
            return true;
        }
        false
    }

    /// Lavori in corso e avanzamento medio in percentuale (None se ancora ignoto), per la notifica.
    pub fn in_corso(&self) -> (usize, Option<u32>, String) {
        let v: Vec<&Lavoro> = self.lavori.iter().filter(|l| l.stato == Stato::InCorso).collect();
        let note: Vec<u32> = v.iter().map(|l| l.prog.load(Ordering::Relaxed)).filter(|m| *m <= 1000).collect();
        let medio = if note.len() == v.len() && !note.is_empty() { Some(note.iter().sum::<u32>() / note.len() as u32 / 10) } else { None };
        let nome = v.first().map(|l| if l.conv { format!("{} · {}", l.origine, l.etichetta) } else { format!("{}: {}", l.etichetta, l.origine) }).unwrap_or_default();
        (v.len(), medio, nome)
    }

    /// Include gli stati attivi: al riavvio diventano interrotti, mai lavori fantasma.
    pub fn serializza(&self) -> String {
        let rows: Vec<_> = self.lavori.iter().enumerate().filter(|(i,l)|*i<100||l.stato==Stato::InCorso||self.uscite.get(&l.id).is_some_and(|v|v.iter().any(|o|o.2.is_some()))).map(|(_,l)| {
            let (stato,path,nome,msg)=match &l.stato {
                Stato::InCorso => ("attivo",PathBuf::new(),String::new(),String::new()),
                Stato::Errore(e) => ("errore",PathBuf::new(),String::new(),e.clone()),
                Stato::Fatto {path,nome,msg} => ("fatto",path.clone(),nome.clone(),msg.clone()),
            };
            serde_json::json!({"chiave":self.chiavi_lavori.get(&l.id),"quando":l.quando,"origine":l.origine,"etichetta":l.etichetta,"conv":l.conv,"stato":stato,"path":path,"nome":nome,"msg":msg,"sorgente":l.sorgente,"controlli":l.controlli,"caso":self.labs.casi.get(&l.id),"uscite":self.uscite.get(&l.id)})
        }).collect();
        let kept:std::collections::HashSet<_>=rows.iter().filter_map(|r|r["chiave"].as_str().map(str::to_string)).collect();
        let mut rimossi=self.lavori_rimossi.clone();
        for key in self.chiavi_lavori.values(){if !kept.contains(key.as_str()){rimossi.insert(key.clone());}}
        serde_json::json!({"versione":2,"lavori":rows,"rimossi":rimossi}).to_string()
    }

    pub fn nuova_importazione(&mut self) -> (u32, Arc<AtomicU32>) {
        let id=self.prossimo; self.prossimo+=1;
        let prog=Arc::new(AtomicU32::new(u32::MAX));
        self.lavori.insert(0,Lavoro {id,origine:"Importazione".into(),etichetta:"File".into(),conv:false,inizio:Instant::now(),quando:adesso(),stato:Stato::InCorso,prog:prog.clone(),sorgente:None,controlli:None});
        (id,prog)
    }

    pub fn errore_importazione(&mut self, e: String) {
        let id=self.prossimo; self.prossimo+=1;
        self.lavori.insert(0,Lavoro {id,origine:"Importazione".into(),etichetta:"File".into(),conv:false,inizio:Instant::now(),quando:adesso(),stato:Stato::Errore(e),prog:Arc::new(AtomicU32::new(1000)),sorgente:None,controlli:None});
        self.scheda=Scheda::Lavori;
        self.sopra=Some(Scheda::Lavori);
    }

    pub fn carica_lavori(&mut self, testo: &str) {
        if let Ok(v)=serde_json::from_str::<serde_json::Value>(testo) {
            if let Ok(removed)=serde_json::from_value(v["rimossi"].clone()){self.lavori_rimossi=removed;}
            if let Some(rows)=v["lavori"].as_array() {
                for r in rows {
                    let text=|k: &str|r[k].as_str().unwrap_or_default().to_string();
                    let id=self.prossimo; self.prossimo+=1;
                    if let Some(c)=r["caso"].as_str(){self.labs.casi.insert(id,c.to_string());}
                    if let Some(k)=r["chiave"].as_str(){self.chiavi_lavori.insert(id,k.to_string());}
                    let stato=match r["stato"].as_str() {
                        Some("fatto")=>Stato::Fatto {path:PathBuf::from(text("path")),nome:text("nome"),msg:text("msg")},
                        Some("attivo")=>Stato::Errore("Interrotto dalla chiusura di Cedro: avvia di nuovo il lavoro".into()),
                        _=>Stato::Errore(text("msg")),
                    };
                    if let Ok(outputs)=serde_json::from_value::<Vec<(PathBuf,String,Option<String>)>>(r["uscite"].clone()) { self.uscite.insert(id,outputs); }
                    self.lavori.push(Lavoro {id,origine:text("origine"),etichetta:text("etichetta"),conv:r["conv"].as_bool().unwrap_or(false),inizio:Instant::now(),quando:r["quando"].as_u64().unwrap_or(0),stato,prog:Arc::new(AtomicU32::new(1000)),sorgente:r["sorgente"].as_str().map(PathBuf::from),controlli:r["controlli"].as_u64().map(|n|n as usize)});
                }
            }
            return;
        }
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
        match self.pannello {
            Some(c) => tinte::palette_pannello(self.tema, c),
            None => tinte::palette(self.tema),
        }
    }

    /// Cornice scura (o chiara con la tinta «Carta»): decide i toni di tutte le schermate.
    fn scuro(&self) -> bool {
        self.pannello.is_none() && tinte::scuro(self.tema)
    }

    /// Il colore dei punti da controllare (arancio, leggibile sul tema chiaro e su quello scuro).
    fn avviso(&self) -> (u8, u8, u8) {
        if self.scuro() { (0xFF, 0xB4, 0x5C) } else { (0xA8, 0x4E, 0x00) }
    }

    fn ha_strumenti(&self) -> bool {
        list(self.kind, Mode::Tools).first().is_some_and(|s| s.id != "none")
    }

    fn fette(&self) -> Vec<cedro::Slice> {
        let mut l = if self.tema.is_cedro() {
            if self.modo == 1 {
                let mut tools = cedro::slices(self.kind, Mode::Tools).to_vec();
                if self.kind == Kind::Pdf { tools.push(cedro::Slice { id: "pages", label: "Pagine" }); }
                tools
            } else { cedro::conversioni::formats(self.kind).to_vec() }
        } else { list(self.kind, if self.modo == 1 { Mode::Tools } else { Mode::Convert }) };
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
            let pagina = (self.firma.viewer.is_some() || self.pdf.pagina.is_some() && !self.pdf.protetto || self.opzioni.is_some() || self.impostazioni || self.casa_aperta && self.casa.elenco.is_none()) && !self.scelta_file;
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
        if self.labs.caso_aperto {self.labs.caso_aperto=false;self.scorri=0.0;return true;}
        if self.firma.viewer.is_some() {
            self.azione_firma(firma::FirmaAz::Chiudi);
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
            if self.tema.is_cedro() && !self.casa.inizio {
                self.azione_casa(casa::CasaAz::Nav(0));
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
        let opts = Some(Box::new(cedro::caso::opzioni(self.labs.caso.as_ref().map(|c|c.code.as_str()), *o)));
        self.opzioni = None;
        let Some(file) = self.file.clone() else { return Effetto::Niente };
        let lavoro = self.prossimo;
        self.prossimo += 1;
        let prog = Arc::new(AtomicU32::new(u32::MAX));
        if let Some(c)=&self.labs.caso {self.labs.casi.insert(lavoro,c.code.clone());}
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
        let qualita = self.qualita;
        let voce = crate::worker::Voce { modello: self.modello, voci: self.voci.min(8), referto: self.referto };
        let web=id.starts_with("web-").then(||{let mut w=self.web.clone();w.media_quality=self.qualita;w.voce=voce;w});
        Effetto::Converti { voce, web, id: lavoro, file: file.path, azione: id, qualita, prog, opts, unisci }
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
            Azione::Web(az) => self.azione_web(az),
            Azione::Casa(az) => self.azione_casa(az),
            Azione::Firma(az) => self.azione_firma(az),
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
                if self.tema.is_cedro() { self.foglio = true; }
                Effetto::Ridisegna
            }
            Azione::Altro => {
                self.foglio = false;
                self.dal_foglio = true;
                self.scheda = Scheda::Ruota;
                Effetto::Ridisegna
            }
            Azione::Esci => Effetto::Esci,
            Azione::FoglioFormato(m, i) => {
                self.modo = m;
                self.scelta = Some(i);
                self.foglio = false;
                self.dal_foglio = true;
                Effetto::Ridisegna
            }
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
                        Some(riga) if riga.id == "firme" => {
                            // Il PDF nel visualizzatore delle firme (firmato o no: lo dice la fascia).
                            if let Some(f) = self.file.clone() {
                                self.apri_firmato(f.path);
                            }
                            Effetto::Ridisegna
                        }
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
                            let opts = self.opzioni_salvate.as_ref().map(|v| Box::new(cedro::conversioni::options_from(v)));
                            self.avvia_con(f.id, f.label, conv, opts, Vec::new())
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
            Azione::Annulla(id) => {
                if self.lavori.iter().any(|l|l.id==id && l.stato==Stato::InCorso) {
                    self.annullamenti.insert(id);
                    Effetto::Annulla(id)
                } else { Effetto::Niente }
            }
            Azione::ErroreUscita(id,i) => match self.uscite.get(&id).and_then(|v|v.get(i)).and_then(|v|v.2.clone()) {Some(e)=>Effetto::Copia(e),None=>Effetto::Niente},
            Azione::ApriUscita(id,i) | Azione::CondividiUscita(id,i) => {
                match self.uscite.get(&id).and_then(|v|v.get(i)) {
                    Some((p,_,_)) if matches!(a,Azione::ApriUscita(..)) => Effetto::Apri(p.clone()),
                    Some((p,_,_))=>Effetto::Condividi(p.clone()),
                    None=>Effetto::Niente,
                }
            }
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
                for l in &self.lavori{if l.stato!=Stato::InCorso{if let Some(k)=self.chiavi_lavori.get(&l.id){self.lavori_rimossi.insert(k.clone());}}}
                self.lavori.retain(|l| l.stato == Stato::InCorso);
                self.scorri = 0.0;
                Effetto::SalvaLavori
            }
            Azione::CopiaRapporto(id) => {
                let text=self.lavori.iter().find(|l|l.id==id).and_then(|l|if let Stato::Fatto{msg,..}=&l.stato{Some(msg.clone())}else{None}).unwrap_or_default();
                Effetto::Copia(text)
            }
            Azione::Labs(az) => return self.azione_labs(az),
            Azione::Pacchetto => return self.pacchetto_labs(),
            Azione::Niente => Effetto::Niente,
        }
    }

    // ---- Disegno ----

    /// Disegna tutto in un'immagine di `w`×`h` dp (il pittore ha già la scala).
    pub fn disegna(&mut self, p: &mut Painter, w: f32, h: f32) {
        self.ora = Instant::now();
        self.anim.inizio();
        self.colpi.clear();
        self.fasce.clear();
        let pal = self.pal();
        p.clear(pal.bg);
        let (ml, mt, mr, mb) = self.margini;
        p.dx = ml;
        p.dy = mt;
        let (w, h) = (w - ml - mr, h - mt - mb);
        if self.labs.caso_aperto {
            self.pagina_caso(p,w,h);
        } else if self.scelta_file {
            self.pagina_scelta(p, w, h);
        } else if self.firma.viewer.is_some() {
            self.pagina_firma(p, w, h);
        } else if self.rivedi.is_some() {
            self.pagina_rivedi(p, w, h);
        } else if self.impostazioni {
            self.pagina_impostazioni(p, w, h);
        } else if self.opzioni.is_some() && (self.file.is_some() || self.opzioni.as_ref().is_some_and(|o| o.da_casa.is_some())) {
            self.pagina_opzioni(p, w, h);
        } else if let (Some(pg), true) = (self.pdf.pagina, self.file.is_some()) {
            self.pagina_pdf(p, pg, w, h);
        } else if self.casa_aperta {
            // Il nuovo Cedro tiene la barra in basso anche su Lavori e Aspetto (senza voce scelta).
            let hh = if self.tema.is_cedro() && self.sopra.is_some() && !self.casa.guida { h - 66.0 } else { h };
            match self.sopra {
                Some(Scheda::Lavori) => self.pagina_lavori(p, w, hh),
                Some(Scheda::Aspetto) => self.pagina_aspetto(p, w, hh),
                Some(Scheda::Info) => self.pagina_info(p, w, hh),
                _ => self.pagina_casa(p, w, h),
            }
            if hh != h {
                self.barra_cedro(p, w, h, None);
            }
        } else if self.tema.is_cedro() && self.file.is_some() && self.scheda == Scheda::Ruota {
            self.foglio_a(p, w, h);
        } else if self.foglio && self.file.is_some() {
            // I temi precedenti conservano il loro foglio compatto.
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
        self.anim.fine();
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
        let voci = [(Scheda::Ruota, if cedro::i18n::foreign() { "Wheel" } else { "Ruota" }, ic::CONVERT), (Scheda::Lavori, "Lavori", ic::CLOCK), (Scheda::Aspetto, "Aspetto", ic::SETTINGS), (Scheda::Info, "Info", ic::INFO)];
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
        let x_t = if self.foglio && self.tema.is_cedro() {
            self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::CLOSE, Azione::Esci);
            56.0
        } else if self.dal_foglio {
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
            lucido(p, self.scuro(), bt.0, bt.1, bt.2, bt.3, bt.3 / 2.0, pal.primary, prem);
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
            // Trascrizione: tre scelte al posto del cursore.
            p.text("Modello", rc.0, rc.1, 90.0, rc.3, 16.0, Weight::Regular, pal.text, Align::Left);
            let (x0, w) = (rc.0 + 92.0, rc.2 - 92.0);
            let cw = (w - 16.0) / 3.0;
            for (i, nome) in ["Veloce", "Buona", "Massima"].into_iter().enumerate() {
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
        let kc = casa::colore_kind(casa::kind_uscita(self.kind, self.righe_ora().get(match az { Azione::Tessera(i) => i, _ => usize::MAX }).map(|r| r.id).unwrap_or("")), self.scuro());
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
        let download_h=self.pannello_download(p,16.0,BARRA,w-32.0,(0.0,BARRA,w,h-BARRA));
        let componente_h=download_h+self.riga_componente(p,16.0,BARRA+download_h,w-32.0);
        let header=componente_h+if self.labs.filtro.is_some(){64.0}else{0.0};
        if let Some(code)=self.labs.filtro.clone(){
            p.text(&cedro::i18n::lit(&code),16.0,BARRA+componente_h,w-190.0,48.0,14.0,Weight::Semibold,pal.primary,Align::Left);
            self.bottone_a(p,(w-174.0,BARRA+componente_h,158.0,48.0),"Tutti i lavori","",pal.surface,pal.primary,Azione::Labs(labs::LabAz::TuttiLavori),(0.0,BARRA,w,header));
        }
        let vista = (0.0, BARRA+header, w, h - BARRA-header);
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
        let mut ys = [BARRA + header + 6.0 - self.scorri; 2];
        let mut fine = 0.0f32;
        for k in 0..self.lavori.len() {
            if self.labs.filtro.as_ref().is_some_and(|c|self.labs.casi.get(&self.lavori[k].id)!=Some(c)){continue;}
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
        let scuro = self.scuro();
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
        p.text(&processore_in_uso(), x + 16.0, cy - 11.0, w - pw - 28.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
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
            let t = if self.anim.calmo { 0.5 } else { (self.ora.duration_since(l.inizio).as_secs_f32() * 0.9).fract() };
            let sw = w * 0.3;
            p.rrect(x - sw + (w + sw) * t, by, sw, bh, bh / 2.0, Some(accento), None);
            p.pop_clip();
        }
        let resta = if perc == 0 {
            self.passi.get(&l.id).cloned().unwrap_or_else(|| "Accendo il motore…".to_string())
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
        let tempo=quando(l.quando);
        let tempo_h=casa::telefono::altezza_a(p,&tempo,13.0,Weight::Regular,w-28.0,19.0,0)+6.0;
        let note_h=match &l.stato {
            Stato::Fatto{msg,..}=>tempo_h+casa::telefono::altezza_a(p,msg,13.0,Weight::Regular,w-28.0,19.0,0)+6.0,
            _=>0.0,
        };
        let caso_h=if self.labs.casi.contains_key(&l.id){32.0}else{0.0};
        let alt = match &l.stato {
            Stato::InCorso if trascrive => 224.0,
            Stato::InCorso => 148.0,
            // Un link passato a un'altra app: non c'è un file da aprire o condividere.
            Stato::Fatto { path, .. } if path.as_os_str().is_empty() => 70.0,
            Stato::Fatto { .. } if l.sorgente.is_some() && l.controlli.is_some_and(|n| n > 0) => 176.0,
            Stato::Fatto { .. } => 142.0,
            Stato::Errore(_) => 112.0,
        } + note_h + caso_h + if matches!(&l.stato,Stato::Fatto{path,..} if !path.as_os_str().is_empty()){56.0}else{0.0};
        let outputs=self.uscite.get(&l.id).cloned().unwrap_or_default();
        let extra=if outputs.len()>1 || outputs.iter().any(|o|o.2.is_some()) {outputs.len() as f32*82.0} else {0.0};
        sollevato(p, pal, self.scuro(), x, y, w, alt+extra, 16.0);
        let titolo = if l.conv { format!("{} · {}", l.origine, l.etichetta) } else { format!("{}: {}", l.etichetta, l.origine) };
        let (stato_txt, stato_col) = match &l.stato {
            Stato::InCorso => (if self.annullamenti.contains(&l.id) {"Annullamento…"} else {"In corso"}.to_string(), pal.primary),
            Stato::Fatto { .. } if outputs.iter().any(|o|o.2.is_some()) => ("Da salvare".to_string(), self.avviso()),
            Stato::Fatto { msg, .. } if avviso_motore(msg) => ("Avviso".to_string(), self.avviso()),
            Stato::Fatto { path, .. } if path.as_os_str().is_empty() => ("Concluso".into(),pal.muted),
            Stato::Fatto { .. } => ("Fatto".to_string(), pal.ok),
            Stato::Errore(_) => ("Non riuscito".to_string(), pal.bad),
        };
        let sw = p.measure(&stato_txt, 14.0, Weight::Semibold) + 4.0;
        let nome = p.ellipsize(&titolo, 16.0, Weight::Semibold, w - sw - 40.0);
        p.text(&nome, x + 14.0, y + 8.0, w - sw - 40.0, 28.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        p.text(&stato_txt, x + w - sw - 14.0, y + 8.0, sw, 28.0, 14.0, Weight::Semibold, stato_col, Align::Right);
        match &l.stato {
            Stato::InCorso if trascrive => {
                self.avanzamento_trascrizione(p, &l, (x+12.0,y+42.0,w-24.0,alt-110.0));
                self.bottone_a(p,(x+12.0,y+alt-108.0,w-24.0,48.0),"Onda e testo",ic::AUDIO,pal.surface,pal.primary,Azione::Rivedi(l.id),vista);
            },
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
                    let t = if self.anim.calmo { 0.5 } else { (self.ora.duration_since(l.inizio).as_secs_f32() * 0.9).fract() };
                    let sw = bw * 0.38;
                    p.rrect(bx - sw + (bw + sw) * t, by, sw, 8.0, 4.0, Some(pal.primary), None);
                    p.pop_clip();
                    p.text(&self.passi.get(&l.id).cloned().unwrap_or_else(||format!("Lavoro in corso da {s} s")), x + 14.0, y + 58.0, w - 28.0, 24.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                }
            }
            Stato::Fatto { nome, msg, path } => {
                casa::telefono::testo_a(p,&tempo,x+14.0,y+38.0,w-28.0,13.0,Weight::Regular,pal.muted,19.0,0);
                casa::telefono::testo_a(p,msg,x+14.0,y+38.0+tempo_h,w-28.0,13.0,Weight::Regular,if avviso_motore(msg){self.avviso()}else{pal.muted},19.0,0);
                if let Some(code)=self.labs.casi.get(&l.id) {
                    p.text(&cedro::i18n::lit(&format!("{} · {}",cedro::i18n::tr("Caso o campione"),code)),x+14.0,y+38.0+note_h,w-28.0,28.0,13.0,Weight::Semibold,pal.primary,Align::Left);
                }
                if path.as_os_str().is_empty() {
                    return alt;
                }
                p.text(&salvato_in(nome), x + 14.0, y + 54.0 + note_h + caso_h, w - 28.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                if let Some(n) = l.controlli.filter(|n| *n > 0 && l.sorgente.is_some()) {
                    let et = if n == 1 { "1 punto da controllare".to_string() } else { format!("{n} punti da controllare") };
                    let ew = p.measure(&et, 13.0, Weight::Semibold) + 22.0;
                    p.rrect(x + 14.0, y + 80.0 + note_h + caso_h, ew, 28.0, 14.0, Some(pal.alt), None);
                    p.text(&et, x + 14.0, y + 80.0 + note_h + caso_h, ew, 28.0, 13.0, Weight::Semibold, self.avviso(), Align::Center);
                }
                let mut voci = vec![("Apri", Azione::Apri(l.id)), ("Condividi", Azione::Condividi(l.id)), ("Copia", Azione::CopiaRapporto(l.id))];
                if l.sorgente.is_some() {
                    voci.insert(0, ("Rivedi", Azione::Rivedi(l.id)));
                }
                let bw = (w - 38.0) / 2.0;
                for (i, (et, az)) in voci.into_iter().enumerate() {
                    let rc = (x + 14.0 + (i%2) as f32 * (bw + 10.0), y + alt - 14.0 - TOCCO - 56.0 + (i/2) as f32*56.0, bw, TOCCO);
                    let prem = self.premuto == Some(az);
                    let pieno = i == 0;
                    if pieno {
                        lucido(p, self.scuro(), rc.0, rc.1, rc.2, rc.3, 24.0, pal.primary, prem);
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
        if l.stato==Stato::InCorso {
            let rc=(x+14.0,y+alt-58.0,w-28.0,48.0);
            p.rrect(rc.0,rc.1,rc.2,rc.3,24.0,Some(pal.alt),None);
            p.text(if self.annullamenti.contains(&l.id) {"Annullamento…"} else {"Annulla"},rc.0,rc.1,rc.2,rc.3,16.0,Weight::Semibold,pal.primary,Align::Center);
            if let Some(r)=intersezione(rc,vista) { self.colpi.push((Forma::R(r),Azione::Annulla(l.id))); }
        }
        if extra>0.0 {
            for (i,(_,nome,errore)) in outputs.iter().enumerate() {
                let yy=y+alt+i as f32*82.0;
                let label=if errore.is_some() {format!("{} · {}",nome,cedro::i18n::tr("Copia in Cedro"))} else {nome.clone()};
                p.text(&label,x+14.0,yy,w-28.0,24.0,13.0,Weight::Regular,if errore.is_some(){pal.bad}else{pal.muted},Align::Left);
                let mut actions=vec![(0,"Apri",Azione::ApriUscita(l.id,i)),(1,"Condividi",Azione::CondividiUscita(l.id,i))];
                if errore.is_some(){actions.push((2,"Copia errore",Azione::ErroreUscita(l.id,i)));}
                let bw=(w-28.0-10.0*(actions.len() as f32-1.0))/actions.len() as f32;
                for (k,et,az) in actions {
                    let rc=(x+14.0+k as f32*(bw+10.0),yy+26.0,bw,48.0);
                    p.rrect(rc.0,rc.1,rc.2,rc.3,24.0,Some(pal.alt),None);
                    p.text(et,rc.0,rc.1,rc.2,rc.3,15.0,Weight::Semibold,pal.primary,Align::Center);
                    if let Some(r)=intersezione(rc,vista) {self.colpi.push((Forma::R(r),az));}
                }
            }
        }
        alt+extra
    }

    // ----- Informazioni -----

    /// Una scheda con righe «nome … valore». Restituisce l'altezza.
    fn riquadro(&self, p: &mut Painter, (x, y, w): (f32, f32, f32), righe: &[(String, String, Option<bool>)]) -> f32 {
        let pal = self.pal();
        let rh = 42.0;
        let h = righe.len() as f32 * rh + 4.0;
        sollevato(p, pal, self.scuro(), x, y, w, h, 16.0);
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
        if self.tema.is_cedro() { return self.impostazioni_a(p, w, h); }
        let pal = self.pal();
        self.titolo_barra(p, 16.0, w - 32.0, "Aspetto");
        let rh = if h < 420.0 { 60.0 } else { 80.0 };
        let ampio = w > 700.0;
        let cw = if ampio { 520.0 } else { w - 24.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 12.0 };
        p.push_clip(0.0, BARRA, w, h - BARRA);
        let mut y = BARRA - self.scorri;
        // Le quattro tinte di Cedro: un quadrato per ciascuna, con i colori della sua cornice.
        p.text("Tinta di Cedro", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 32.0;
        let tw = (cw - 3.0 * 10.0) / 4.0;
        for (i, tema) in TEMI.into_iter().take(TINTE).enumerate() {
            let tx = x + i as f32 * (tw + 10.0);
            let t = tema.tint();
            let attivo = tema == self.tema;
            p.rrect_vgrad(tx, y, tw, tw, 14.0, t.frame_hi, t.frame);
            p.rrect(tx + tw * 0.14, y + tw * 0.56, tw * 0.72, tw * 0.3, 6.0, Some((255, 255, 255)), Some((t.line, 1.0)));
            p.rrect_vgrad(tx + tw * 0.14, y + tw * 0.16, tw * 0.72, tw * 0.3, 6.0, (0xFF, 0x8F, 0x52), (0xB2, 0x3F, 0x73));
            p.rrect(tx, y, tw, tw, 14.0, None, Some((if attivo { pal.primary } else { pal.border }, if attivo { 3.0 } else { 1.0 })));
            p.text(tinte::nome(tema), tx - 4.0, y + tw + 2.0, tw + 8.0, 22.0, 13.0, if attivo { Weight::Bold } else { Weight::Semibold }, if attivo { pal.text } else { pal.muted }, Align::Center);
            self.colpi.push((Forma::R((tx, y, tw, tw + 26.0)), Azione::Tema(i)));
        }
        y += tw + 34.0;
        p.text("Vista di partenza", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 32.0;
        self.pillole(p, (x, y, cw, TOCCO), &["Semplice", "Completa"], usize::from(!self.imp.simple), |i| Azione::Casa(casa::CasaAz::Vista(i == 1)));
        y += TOCCO + 6.0;
        y += p.text_wrap("Semplice mostra il formato e il pulsante; Completa anche qualità, dimensioni e le altre opzioni.", x + 4.0, y, cw - 8.0, 13.0, Weight::Regular, pal.muted, 19.0, 3) + 12.0;
        p.text("Temi di prima", x + 4.0, y, cw - 8.0, 30.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 32.0;
        for (i, tema) in TEMI.into_iter().enumerate().skip(TINTE) {
            let rc = (x, y + (i - TINTE) as f32 * rh, cw, rh - 8.0);
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
        y += (TEMI.len() - TINTE) as f32 * rh + 4.0;
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
        ThemeId::Cedro | ThemeId::CedroNotte | ThemeId::CedroBosco | ThemeId::CedroCarta | ThemeId::CedroLabs => "Figtree",
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

/// «Processore · 7 thread» come lo ha detto il motore partito (i core accesi cambiano col risparmio energetico).
pub(crate) fn processore_in_uso() -> String {
    match cedro::accel::in_use() {
        Some(u) if !u.gpu && !u.detail.is_empty() => format!("Processore · {}", u.detail),
        _ => format!("Processore · {} thread", cedro::trascrizione::engine_threads()),
    }
}

/// Il modello di trascrizione proposto all'apertura: «Buona» sui telefoni con almeno 8 GB di memoria
/// (i recenti la reggono bene), «Veloce» sui più vecchi. Gli altri restano sempre selezionabili.
fn modello_iniziale() -> usize {
    match cedro::riassunto::total_memory() {
        Some(b) if b >= 7 * 1024 * 1024 * 1024 => 1,
        _ => 0,
    }
}

/// «Salvato in …», o com'è quando il file c'era già e non è stato scritto («Già presente: …»).
pub(crate) fn salvato_in(nome: &str) -> String {
    if nome.starts_with("Già presente") { nome.to_string() } else { format!("Salvato in {nome}") }
}

#[cfg(test)]
mod lavori_regressioni {
    use super::*;
    #[test]
    fn evento_arrivato_dopo_drain_persiste_stato_e_uscite_insieme(){
        let mut app=App::new(ThemeId::Cedro);let (id,_)=app.nuova_importazione();
        let identity=app.identita(id).unwrap();
        let queue=std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
        let (q,b)=(queue.clone(),barrier.clone());
        let writer=std::thread::spawn(move||{
            b.wait();
            q.lock().unwrap().push(Completamento{batch:None,id,stato:Stato::Fatto{path:"content://media/1".into(),nome:"a.pdf".into(),msg:"Diviso".into()},uscite:Some(vec![("content://media/1".into(),"a.pdf".into(),None),("content://media/2".into(),"b.pdf".into(),None)]),sorgente:Some("/privato/audio.wav".into())});
        });
        assert!(std::mem::take(&mut *queue.lock().unwrap()).is_empty());
        barrier.wait();writer.join().unwrap();
        for e in std::mem::take(&mut *queue.lock().unwrap()){app.completa(e);}
        let mut restarted=App::new(ThemeId::Cedro);restarted.carica_lavori(&app.serializza());
        assert_eq!(restarted.lavori.len(),1);let saved_id=restarted.lavori[0].id;
        assert_eq!(restarted.uscite[&saved_id].len(),2);
        assert_eq!(restarted.identita(saved_id).unwrap().chiave,identity.chiave);
        assert_eq!(restarted.lavori[0].sorgente,Some(PathBuf::from("/privato/audio.wav")));
        assert!(matches!(&restarted.lavori[0].stato,Stato::Fatto{..}));
        // Un journal durevole già rappresentato nella UI aggiorna lo stesso lavoro.
        let r=crate::publication::Registro{versione:3,lavoro:identity,forma:crate::publication::Forma::File,gruppo:None,messaggio:"Diviso".into(),pubblico:None,elementi:vec![crate::publication::Elemento{privato:"/privato/a.pdf".into(),relativo:"a.pdf".into(),stato:crate::publication::StatoUscita::Pubblicato(crate::publication::Destinazione{path:"content://media/1".into(),nome:"a.pdf".into(),temporaneo:None}),errore:None}]};
        restarted.recupera(r.clone());restarted.recupera(r.clone());assert_eq!(restarted.lavori.len(),1);
        restarted.lavori_rimossi.insert(r.lavoro.chiave.clone());restarted.lavori.clear();
        let mut cleared=App::new(ThemeId::Cedro);cleared.carica_lavori(&restarted.serializza());
        cleared.recupera(r);assert!(cleared.lavori.is_empty());
    }
    #[test]
    fn riavvio_interrompe_attivi_e_conserva_errori_e_tutte_le_uscite() {
        let mut app=App::new(ThemeId::Cedro);
        let (attivo,_)=app.nuova_importazione();
        let (fallito,_)=app.nuova_importazione();
        app.finisce(fallito,Stato::Errore("errore\ncon dettagli\te tab".into()));
        let (finito,_)=app.nuova_importazione();
        app.uscite.insert(finito,vec![("content://media/1".into(),"pagina1.pdf".into(),None),("/privato/pagina2.pdf".into(),"pagina2.pdf".into(),Some("Disco pieno".into()))]);
        app.finisce(finito,Stato::Fatto {path:"content://media/1".into(),nome:"pagina1.pdf".into(),msg:"Diviso".into()});
        let mut restored=App::new(ThemeId::Cedro);
        restored.carica_lavori(&app.serializza());
        assert_eq!(restored.in_corso().0,0);
        assert_eq!(restored.lavori.len(),3);
        assert!(restored.lavori.iter().any(|l|matches!(&l.stato,Stato::Errore(e) if e.starts_with("Interrotto"))));
        assert!(restored.lavori.iter().any(|l|matches!(&l.stato,Stato::Errore(e) if e=="errore\ncon dettagli\te tab")));
        assert_eq!(restored.uscite.values().next().unwrap().len(),2);
        assert!(app.lavori.iter().any(|l|l.id==attivo&&l.stato==Stato::InCorso));
    }
}

#[cfg(test)]
mod temi_regressioni {
    use super::*;
    use cedro::ui::gfx::{Fonts, GlyphCache};

    #[test]
    fn android_keeps_legacy_theme_selection_save_reload_and_host_rendering() {
        assert_eq!(TEMI.map(ThemeId::key), ["cedro", "cedro-notte", "cedro-bosco", "cedro-carta", "windows-essenziale", "percorso-guidato", "cedro-compatto", "banco-di-lavoro", "documento-al-centro"]);
        let mut cache = GlyphCache::new();
        for (i, theme) in TEMI.into_iter().enumerate() {
            let mut app = App::new(ThemeId::Cedro);
            let saved_key = match app.attiva(Azione::Tema(i)) {
                Effetto::SalvaTema(selected) => selected.key(),
                _ => panic!("la scelta del tema deve richiedere il salvataggio"),
            };
            assert_eq!(saved_key, theme.key());
            let mut restored = App::new(tema_da_chiave(&format!(" {saved_key}\n")));
            assert_eq!(restored.tema, theme);
            restored.anim.calmo = true;
            restored.vai_casa();
            let mut painter = Painter::new(360, 800, 1.0, Fonts::for_theme(restored.tema), &mut cache);
            restored.disegna(&mut painter, 360.0, 800.0);
            assert_eq!(restored.tema, theme);
            assert!(!restored.colpi.is_empty(), "controlli del tema {saved_key}");
        }
        for key in ["bondeno", "cedro-labs", "sconosciuto", ""] {
            assert_eq!(tema_da_chiave(key), ThemeId::Cedro);
        }
    }
}
