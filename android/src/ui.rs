//! L'interfaccia di Cedro per il tocco, in unità dp (1 dp = 1/160 di pollice). Niente di Android qui
//! dentro: la finestra passa tocchi e dimensioni, qui si disegna (con il pittore di `cedro::ui`) e si decide
//! cosa fare. Ogni controllo che si vede è collegato al motore: se non lo è, non c'è.
//!
//! Due schermi: il **foglio** che sale dal basso dopo «Condividi» con le azioni per quel tipo di file, e la
//! **ruota** a tutto schermo con spicchi da pollice (anche per aprire Cedro dall'icona). Sopra tutto
//! può salire il foglio dell'**aspetto** (i cinque temi di Cedro). I bersagli sono sempre ≥ 48 dp.

use std::path::PathBuf;
use std::time::Instant;

use cedro::ui::assets as ic;
use cedro::ui::gfx::{Align, Fit, Image, Painter, Rgb, Weight};
use cedro::ui::theme::{mix, Palette, ThemeId};
use cedro::ui::views::uses_quality;
use cedro::ui::wheel::list;
use cedro::{Kind, Mode};

pub type R = (f32, f32, f32, f32);

/// I temi che si possono scegliere (quello del Comune è dell'edizione dell'ente, non del telefono).
pub const TEMI: [ThemeId; 5] =
    [ThemeId::WindowsEssenziale, ThemeId::PercorsoGuidato, ThemeId::CedroCompatto, ThemeId::BancoDiLavoro, ThemeId::DocumentoAlCentro];

const BARRA: f32 = 56.0;
const TOCCO: f32 = 48.0;
const DURATA_FOGLIO: f32 = 0.26;

/// Il file che si sta lavorando.
#[derive(Clone)]
pub struct Archivio {
    pub path: PathBuf,
    pub nome: String,
    pub dim: u64,
}

#[derive(Clone, PartialEq)]
pub enum Lavoro {
    Fermo,
    InCorso(String),
    Fatto { nome: String, msg: String },
    Errore(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Schermo {
    Foglio,
    Ruota,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Azione {
    Riga(usize),
    TutteLeAzioni,
    Fine,
    Fetta(usize),
    Modo(u8),
    Vai,
    Cursore,
    Aspetto,
    Tema(usize),
    ChiudiAspetto,
    IndietroFoglio,
    Ancora,
    /// Un'area che non fa niente ma ferma il tocco (il foglio stesso).
    Niente,
}

pub enum Effetto {
    Niente,
    Ridisegna,
    /// Da eseguire col motore: azione, nome in chiaro, qualità.
    Converti { id: &'static str, etichetta: String, qualita: u8 },
    /// Chiude Cedro: si torna all'app di prima.
    Esci,
    /// Il tema è cambiato: va ricordato.
    SalvaTema(ThemeId),
}

#[derive(Clone, Copy)]
enum Forma {
    R(R),
    Fetta { cx: f32, cy: f32, ri: f32, ro: f32, a0: f32, a1: f32 },
}

impl Forma {
    fn dentro(&self, x: f32, y: f32) -> bool {
        match *self {
            Forma::R((rx, ry, rw, rh)) => x >= rx && x < rx + rw && y >= ry && y < ry + rh,
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

/// Una riga del foglio delle azioni.
struct Riga {
    id: &'static str,
    titolo: &'static str,
    sotto: &'static str,
}

const fn r(id: &'static str, titolo: &'static str, sotto: &'static str) -> Riga {
    Riga { id, titolo, sotto }
}

/// Le azioni da mettere nel foglio per un tipo di file: solo quelle che il motore sa fare.
fn righe(kind: Kind) -> Vec<&'static Riga> {
    static IMG: [Riga; 5] = [
        r("jpeg", "In JPG", "Foto più leggere"),
        r("png", "In PNG", "Senza perdita di qualità"),
        r("webp", "In WebP", "Per il web"),
        r("pdf", "In PDF", "Un foglio con l'immagine"),
        r("compress", "Comprimi", "Occupa meno spazio"),
    ];
    static PDF: [Riga; 4] = [
        r("docx", "In Word", "Con riconoscimento del testo"),
        r("txt", "Testo semplice", "Estrae il testo"),
        r("jpg", "In immagini JPG", "Dalle pagine"),
        r("png", "In immagini PNG", "Dalle pagine"),
    ];
    static TXT: [Riga; 3] = [r("pdf", "In PDF", "Un documento da stampare"), r("html", "In HTML", "Per il web"), r("md", "In Markdown", "Testo semplice con segni")];
    static AUD: [Riga; 2] = [r("mp3", "In MP3", "Più leggero"), r("wav", "In WAV", "Senza compressione")];
    static VID: [Riga; 2] = [r("mp4", "In MP4", "Più leggero"), r("mp3", "Solo audio", "MP3 dal video")];
    static DOC: [Riga; 1] = [r("pdf", "In PDF", "Da documento")];
    let tutte: &'static [Riga] = match kind {
        Kind::Image => &IMG,
        Kind::Pdf => &PDF,
        Kind::Text => &TXT,
        Kind::Audio => &AUD,
        Kind::Video => &VID,
        Kind::Doc => &DOC,
        Kind::Other => &[],
    };
    let ids: Vec<&'static str> = [Mode::Convert, Mode::Tools].into_iter().flat_map(|m| list(kind, m)).map(|s| s.id).collect();
    tutte.iter().filter(|r| ids.contains(&r.id)).collect()
}

fn icona_di(id: &str) -> &'static str {
    match id {
        "webp" | "bmp" | "gray" | "jpeg" | "jpg" | "png" | "tiff" | "gif" => ic::IMAGE,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => ic::VIDEO,
        "wav" | "mp3" | "ogg" | "m4a" | "aac" | "flac" => ic::AUDIO,
        "compress" | "half" | "strip" => ic::DOWNLOAD,
        "rot90" => ic::CONVERT,
        "ocr" => ic::SEARCH,
        "docx" | "txt" | "html" | "md" | "csv" | "json" | "pdf" | "upper" | "lower" => ic::FILE,
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
        Kind::Other => "File",
    }
}

fn peso(b: u64) -> String {
    if b >= 1_000_000 {
        format!("{:.1} MB", b as f64 / 1e6).replace('.', ",")
    } else {
        format!("{} KB", (b / 1000).max(1))
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub struct App {
    pub tema: ThemeId,
    pub file: Option<Archivio>,
    pub kind: Kind,
    pub miniatura: Option<Image>,
    pub schermo: Schermo,
    pub aspetto: bool,
    pub modo: u8,
    pub scelta: Option<usize>,
    pub qualita: u8,
    pub lavoro: Lavoro,
    /// Si è arrivati alla ruota dal foglio: «indietro» ci torna.
    pub dal_foglio: bool,
    pub apertura: Instant,
    pub apertura_aspetto: Instant,
    premuto: Option<Azione>,
    colpi: Vec<(Forma, Azione)>,
    cursore: R,
    /// Istante attuale per le animazioni (fissato a ogni disegno).
    ora: Instant,
    /// Barre di sistema e gesti, in dp: sinistra, alto, destra, basso.
    pub margini: (f32, f32, f32, f32),
}

impl App {
    pub fn new(tema: ThemeId) -> App {
        let ora = Instant::now();
        App {
            tema,
            file: None,
            kind: Kind::Image,
            miniatura: None,
            schermo: Schermo::Ruota,
            aspetto: false,
            modo: 0,
            scelta: None,
            qualita: 82,
            lavoro: Lavoro::Fermo,
            dal_foglio: false,
            apertura: ora,
            apertura_aspetto: ora,
            premuto: None,
            colpi: Vec::new(),
            cursore: (0.0, 0.0, 1.0, 1.0),
            ora,
            margini: (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Imposta il file ricevuto: da «Condividi» si parte dal foglio, dall'icona dalla ruota.
    pub fn imposta_file(&mut self, f: Archivio, kind: Kind, da_condivisione: bool) {
        self.file = Some(f);
        self.kind = kind;
        self.modo = 0;
        self.scelta = None;
        self.lavoro = Lavoro::Fermo;
        self.miniatura = None;
        self.schermo = if da_condivisione && !righe(kind).is_empty() { Schermo::Foglio } else { Schermo::Ruota };
        self.dal_foglio = false;
        self.apertura = Instant::now();
    }

    /// Vero se serve ridisegnare a ogni fotogramma (animazione in corso).
    pub fn animando(&self) -> bool {
        let t = |i: Instant| i.elapsed().as_secs_f32() < DURATA_FOGLIO + 0.05;
        matches!(self.lavoro, Lavoro::InCorso(_)) || (self.schermo == Schermo::Foglio && t(self.apertura)) || (self.aspetto && t(self.apertura_aspetto))
    }

    fn pal(&self) -> &'static Palette {
        self.tema.palette()
    }

    fn ha_strumenti(&self) -> bool {
        list(self.kind, Mode::Tools).first().is_some_and(|s| s.id != "none")
    }

    fn fette(&self) -> Vec<cedro::Slice> {
        let l = list(self.kind, if self.modo == 1 { Mode::Tools } else { Mode::Convert });
        if l.first().is_some_and(|s| s.id == "none") {
            Vec::new()
        } else {
            l
        }
    }

    // ---- Tocco ----

    pub fn tocca_giu(&mut self, x: f32, y: f32) -> Effetto {
        let (x, y) = (x - self.margini.0, y - self.margini.1);
        self.premuto = self.trova(x, y);
        if self.premuto == Some(Azione::Cursore) {
            self.imposta_cursore(x);
        }
        Effetto::Ridisegna
    }

    pub fn tocca_muovi(&mut self, x: f32, _y: f32) -> Effetto {
        let x = x - self.margini.0;
        if self.premuto == Some(Azione::Cursore) {
            self.imposta_cursore(x);
            return Effetto::Ridisegna;
        }
        Effetto::Niente
    }

    pub fn tocca_su(&mut self, x: f32, y: f32) -> Effetto {
        let (x, y) = (x - self.margini.0, y - self.margini.1);
        let giu = self.premuto.take();
        let su = self.trova(x, y);
        match (giu, su) {
            (Some(a), Some(b)) if a == b => self.attiva(a),
            _ => Effetto::Ridisegna,
        }
    }

    pub fn tocca_annulla(&mut self) {
        self.premuto = None;
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
        if self.aspetto {
            self.aspetto = false;
            return true;
        }
        if self.lavoro != Lavoro::Fermo && !matches!(self.lavoro, Lavoro::InCorso(_)) {
            self.lavoro = Lavoro::Fermo;
            return true;
        }
        if self.schermo == Schermo::Ruota && self.dal_foglio {
            self.schermo = Schermo::Foglio;
            self.dal_foglio = false;
            self.apertura = Instant::now();
            return true;
        }
        false
    }

    fn avvia(&mut self, id: &'static str, etichetta: &str) -> Effetto {
        if matches!(self.lavoro, Lavoro::InCorso(_)) {
            return Effetto::Niente;
        }
        self.lavoro = Lavoro::InCorso(etichetta.to_string());
        Effetto::Converti { id, etichetta: etichetta.to_string(), qualita: self.qualita }
    }

    fn attiva(&mut self, a: Azione) -> Effetto {
        match a {
            Azione::Riga(i) => match righe(self.kind).get(i).copied() {
                Some(riga) if self.file.is_some() => self.avvia(riga.id, riga.titolo),
                _ => Effetto::Niente,
            },
            Azione::TutteLeAzioni => {
                self.schermo = Schermo::Ruota;
                self.dal_foglio = true;
                Effetto::Ridisegna
            }
            Azione::Fine => Effetto::Esci,
            Azione::Fetta(i) => {
                if self.file.is_some() && !matches!(self.lavoro, Lavoro::InCorso(_)) {
                    self.scelta = Some(i);
                    self.lavoro = Lavoro::Fermo;
                }
                Effetto::Ridisegna
            }
            Azione::Modo(m) => {
                self.modo = m;
                self.scelta = None;
                if !matches!(self.lavoro, Lavoro::InCorso(_)) {
                    self.lavoro = Lavoro::Fermo;
                }
                Effetto::Ridisegna
            }
            Azione::Vai => match self.scelta.and_then(|i| self.fette().get(i).copied()) {
                Some(f) => self.avvia(f.id, f.label),
                None => Effetto::Niente,
            },
            Azione::Cursore => Effetto::Ridisegna,
            Azione::Aspetto => {
                self.aspetto = true;
                self.apertura_aspetto = Instant::now();
                Effetto::Ridisegna
            }
            Azione::ChiudiAspetto => {
                self.aspetto = false;
                Effetto::Ridisegna
            }
            Azione::Tema(i) => {
                self.tema = TEMI[i];
                Effetto::SalvaTema(self.tema)
            }
            Azione::IndietroFoglio => {
                self.indietro();
                Effetto::Ridisegna
            }
            Azione::Ancora => {
                self.lavoro = Lavoro::Fermo;
                Effetto::Ridisegna
            }
            Azione::Niente => Effetto::Niente,
        }
    }

    // ---- Disegno ----

    /// Disegna tutto in un'immagine di `w`×`h` dp (il pittore ha già la scala).
    pub fn disegna(&mut self, p: &mut Painter, w: f32, h: f32) {
        self.ora = Instant::now();
        self.colpi.clear();
        let pal = self.pal();
        p.clear(pal.bg);
        // Dentro le barre di sistema: si disegna nell'area libera, i fogli si allungano sotto la barra dei gesti.
        let (ml, mt, mr, mb) = self.margini;
        p.dx = ml;
        p.dy = mt;
        let (w, h) = (w - ml - mr, h - mt - mb);
        match self.schermo {
            Schermo::Ruota => self.ruota(p, w, h),
            Schermo::Foglio => self.foglio(p, w, h),
        }
        if self.aspetto {
            self.foglio_temi(p, w, h);
        }
    }

    fn barra(&mut self, p: &mut Painter, w: f32) {
        let pal = self.pal();
        if self.dal_foglio {
            self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::IndietroFoglio);
            p.text("Tutte le azioni", 56.0, 0.0, w - 130.0, BARRA, 20.0, Weight::Semibold, pal.text, Align::Left);
        } else {
            p.symbol(18.0, 14.0, 28.0);
            p.text("Cedro", 54.0, 0.0, w - 120.0, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
        }
        self.bottone_icona(p, (w - 52.0, 4.0, TOCCO, TOCCO), ic::SETTINGS, Azione::Aspetto);
    }

    fn bottone_icona(&mut self, p: &mut Painter, rc: R, icona: &str, az: Azione) {
        let pal = self.pal();
        if self.premuto == Some(az) {
            p.rrect_a(rc.0, rc.1, rc.2, rc.3, rc.2 / 2.0, pal.primary, 0.14);
        }
        p.icon(icona, rc.0 + (rc.2 - 26.0) / 2.0, rc.1 + (rc.3 - 26.0) / 2.0, 26.0, pal.text);
        self.colpi.push((Forma::R(rc), az));
    }

    // ----- La ruota -----

    fn ruota(&mut self, p: &mut Painter, w: f32, h: f32) {
        self.barra(p, w);
        let orizz = w > h * 1.15;
        let ha_file = self.file.is_some();
        let tabs_on = ha_file && self.ha_strumenti();
        let fette = if ha_file { self.fette() } else { list(Kind::Image, Mode::Convert) };
        let pann_h = if ha_file { 150.0 } else { 100.0 };

        let (centro, ro, col): ((f32, f32), f32, R);
        if orizz {
            let r0 = ((h - BARRA - 24.0) / 2.0).min(w * 0.44).max(80.0);
            centro = (16.0 + r0, BARRA + (h - BARRA) / 2.0);
            ro = r0;
            let x = centro.0 + ro + 28.0;
            col = (x, BARRA + 6.0, w - x - 16.0, h - BARRA - 18.0);
        } else {
            let sotto = pann_h + if tabs_on { TOCCO + 14.0 } else { 0.0 } + 16.0;
            let libero = h - BARRA - sotto - 4.0;
            let r0 = ((w - 28.0) / 2.0).min(libero / 2.0).max(80.0);
            centro = (w / 2.0, BARRA + 2.0 + libero / 2.0);
            ro = r0;
            col = (16.0, h - sotto, w - 32.0, sotto - 12.0);
        }
        self.disco(p, centro, ro, &fette, ha_file);

        // Colonna dei comandi: il pannello e, sotto, il selettore dei modi.
        let (x, mut y, cw, ch) = col;
        if orizz {
            y += (ch - pann_h - if tabs_on { TOCCO + 14.0 } else { 0.0 }).max(0.0) / 2.0;
        }
        self.pannello(p, (x, y, cw, pann_h));
        if tabs_on {
            self.modi(p, (x, y + pann_h + 14.0, cw, TOCCO));
        }
    }

    fn disco(&mut self, p: &mut Painter, (cx, cy): (f32, f32), ro: f32, fette: &[cedro::Slice], attivo: bool) {
        let pal = self.pal();
        let ri = ro * 0.42;
        let n = fette.len().max(1);
        let tau = std::f32::consts::TAU;
        let sweep = tau / n as f32;
        p.disc_a(cx, cy + 5.0, ro + 6.0, (0, 0, 0), 0.07);
        p.disc_a(cx, cy + 2.0, ro + 2.0, (0, 0, 0), 0.06);
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
            let bordo = sector(cx, cy, ri + corner, ro - corner, a0, a1, gap + corner);
            let _ = bordo;
            if attivo {
                self.colpi.push((Forma::Fetta { cx, cy, ri, ro: ro + 4.0, a0, a1 }, Azione::Fetta(i)));
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
        // Il file al centro.
        let rc = ri - 7.0;
        p.disc_a(cx, cy + 3.0, rc + 6.0, (0, 0, 0), 0.10);
        p.circle(cx, cy, rc + 4.0, Some(pal.surface), None);
        match (&self.miniatura, attivo) {
            (Some(img), true) => p.image(img, cx - rc, cy - rc, rc * 2.0, rc * 2.0, rc, Fit::Cover),
            _ => {
                p.circle(cx, cy, rc, Some(pal.alt), None);
                let k = if attivo { icona_tipo(self.kind) } else { ic::FILE };
                p.icon(k, cx - 20.0, cy - 20.0, 40.0, pal.primary);
            }
        }
    }

    fn modi(&mut self, p: &mut Painter, rc: R) {
        let pal = self.pal();
        p.rrect(rc.0, rc.1, rc.2, rc.3, rc.3 / 2.0, Some(pal.alt), None);
        let mw = (rc.2 - 8.0) / 2.0;
        for (i, nome) in ["Converti", "Strumenti"].into_iter().enumerate() {
            let rr = (rc.0 + 4.0 + i as f32 * mw, rc.1 + 4.0, mw, rc.3 - 8.0);
            let on = self.modo == i as u8;
            if on {
                p.rrect(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, Some(pal.primary), None);
            } else if self.premuto == Some(Azione::Modo(i as u8)) {
                p.rrect_a(rr.0, rr.1, rr.2, rr.3, rr.3 / 2.0, pal.primary, 0.14);
            }
            p.text(nome, rr.0, rr.1, rr.2, rr.3, 16.0, Weight::Semibold, if on { pal.on_primary } else { pal.text }, Align::Center);
            // Il bersaglio è tutta l'altezza del selettore (48 dp).
            self.colpi.push((Forma::R((rr.0, rc.1, rr.2, rc.3)), Azione::Modo(i as u8)));
        }
    }

    fn pannello(&mut self, p: &mut Painter, (x, y, w, h): R) {
        let pal = self.pal();
        let Some(file) = self.file.clone() else {
            p.text("Nessun file", x, y, w, 32.0, 20.0, Weight::Semibold, pal.text, Align::Center);
            p.text_wrap("Da qualsiasi app scegli Condividi e poi Cedro: qui compariranno le azioni per quel file.", x + 8.0, y + 40.0, w - 16.0, 15.0, Weight::Regular, pal.muted, 21.0, 3);
            return;
        };
        match self.lavoro.clone() {
            Lavoro::Fermo => {}
            _ => {
                self.esito(p, x, y, w);
                return;
            }
        }
        let fette = self.fette();
        match self.scelta.and_then(|i| fette.get(i).copied()) {
            Some(f) => {
                let titolo = if self.modo == 1 { format!("Applica: {}", f.label) } else { format!("Converti in {}", f.label) };
                let qualita = uses_quality(self.kind, f.id);
                // Senza il cursore il titolo sta a metà tra l'alto e il pulsante.
                let ty = if qualita { y } else { y + (h - 56.0 - 30.0) / 2.0 };
                p.text(&titolo, x, ty, w, 30.0, 19.0, Weight::Semibold, pal.text, Align::Center);
                if qualita {
                    self.slider(p, (x, y + 38.0, w, TOCCO));
                }
                let bt = (x, y + h - 56.0, w, 56.0);
                let prem = self.premuto == Some(Azione::Vai);
                p.rrect(bt.0, bt.1, bt.2, bt.3, 28.0, Some(if prem { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary }), None);
                let et = if self.modo == 1 { "Applica" } else { "Converti" };
                let tw = p.measure(et, 18.0, Weight::Semibold);
                let ix = bt.0 + (bt.2 - tw - 34.0) / 2.0;
                p.icon(ic::CONVERT, ix, bt.1 + 15.0, 26.0, pal.on_primary);
                p.text(et, ix + 34.0, bt.1, tw + 4.0, bt.3, 18.0, Weight::Semibold, pal.on_primary, Align::Left);
                self.colpi.push((Forma::R(bt), Azione::Vai));
            }
            None => {
                let invito = if self.modo == 1 { "Scegli uno strumento" } else { "Scegli un formato" };
                p.text(invito, x, y + 6.0, w, 34.0, 20.0, Weight::Semibold, pal.text, Align::Center);
                let info = format!("{} · {} · {}", file.nome, nome_tipo(self.kind), peso(file.dim));
                p.text(&info, x, y + 44.0, w, 26.0, 14.0, Weight::Regular, pal.muted, Align::Center);
                p.text("Tocca uno spicchio della ruota.", x, y + 72.0, w, 26.0, 14.0, Weight::Regular, pal.muted, Align::Center);
            }
        }
    }

    /// Il cursore della qualità (solo dove il motore la usa).
    fn slider(&mut self, p: &mut Painter, rc: R) {
        let pal = self.pal();
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

    /// Lavoro in corso, finito o fallito: testo e pulsanti. Restituisce l'altezza usata.
    fn esito(&mut self, p: &mut Painter, x: f32, y: f32, w: f32) -> f32 {
        let pal = self.pal();
        match self.lavoro.clone() {
            Lavoro::InCorso(et) => {
                p.text(&format!("Converto in {et}…"), x, y + 4.0, w, 34.0, 19.0, Weight::Semibold, pal.text, Align::Center);
                if let Some(f) = &self.file {
                    p.text(&f.nome, x, y + 38.0, w, 24.0, 14.0, Weight::Regular, pal.muted, Align::Center);
                }
                let (bx, by, bw) = (x + 8.0, y + 76.0, w - 16.0);
                p.rrect(bx, by, bw, 8.0, 4.0, Some(pal.alt), None);
                p.push_clip(bx, by, bw, 8.0);
                let t = (self.ora.duration_since(self.apertura).as_secs_f32() * 0.9).fract();
                let sw = bw * 0.38;
                p.rrect(bx - sw + (bw + sw) * t, by, sw, 8.0, 4.0, Some(pal.primary), None);
                p.pop_clip();
                92.0
            }
            Lavoro::Fatto { nome, msg } => {
                let tw = p.measure("Fatto", 21.0, Weight::Semibold);
                let x0 = x + (w - tw - 34.0) / 2.0;
                p.circle(x0 + 12.0, y + 18.0, 12.0, Some(pal.ok), None);
                p.icon(ic::CHECK, x0 + 4.0, y + 10.0, 16.0, pal.surface);
                p.text("Fatto", x0 + 34.0, y, tw + 6.0, 36.0, 21.0, Weight::Semibold, pal.ok, Align::Left);
                p.text(&format!("Salvato in Documenti: {nome}"), x, y + 40.0, w, 22.0, 15.0, Weight::Regular, pal.text, Align::Center);
                p.text(&msg.replace(" → ", " a "), x, y + 62.0, w, 20.0, 13.0, Weight::Regular, pal.muted, Align::Center);
                self.coppia_pulsanti(p, x, y + 92.0, w, "Un'altra azione");
                148.0
            }
            Lavoro::Errore(m) => {
                p.icon(ic::WARNING, x + w / 2.0 - 12.0, y + 4.0, 24.0, pal.bad);
                p.text_wrap(&m, x + 8.0, y + 34.0, w - 16.0, 14.0, Weight::Regular, pal.bad, 19.0, 2);
                self.coppia_pulsanti(p, x, y + 92.0, w, "Riprova");
                148.0
            }
            Lavoro::Fermo => 0.0,
        }
    }

    fn coppia_pulsanti(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, secondo: &str) {
        let pal = self.pal();
        let bw = (w - 12.0) / 2.0;
        let (a, b) = ((x, y, bw, 52.0), (x + bw + 12.0, y, bw, 52.0));
        let prem = self.premuto == Some(Azione::Ancora);
        p.rrect(a.0, a.1, a.2, a.3, 26.0, Some(if prem { pal.alt } else { pal.surface }), Some((pal.primary, 1.5)));
        p.text(secondo, a.0, a.1, a.2, a.3, 16.0, Weight::Semibold, pal.primary, Align::Center);
        let prem = self.premuto == Some(Azione::Fine);
        p.rrect(b.0, b.1, b.2, b.3, 26.0, Some(if prem { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary }), None);
        p.text("Chiudi", b.0, b.1, b.2, b.3, 16.0, Weight::Semibold, pal.on_primary, Align::Center);
        self.colpi.push((Forma::R(a), Azione::Ancora));
        self.colpi.push((Forma::R(b), Azione::Fine));
    }

    // ----- I fogli dal basso -----

    /// Sfondo scurito, foglio e maniglia. Restituisce dove cominciare a disegnare dentro il foglio: (x, y, larghezza).
    fn foglio_base(&mut self, p: &mut Painter, w: f32, h: f32, t: f32, contenuto: f32, chiude: Azione) -> (f32, f32, f32) {
        let pal = self.pal();
        let (ml, mt, mr, mb) = self.margini;
        p.fill_a(-ml, -mt, w + ml + mr, h + mt + mb, (0, 0, 0), 0.42 * t);
        self.colpi.push((Forma::R((-ml, -mt, w + ml + mr, h + mt + mb)), chiude));
        let sw = w.min(560.0);
        let x0 = (w - sw) / 2.0;
        let totale = contenuto + 26.0 + 14.0;
        let y = h - totale + (1.0 - t) * (totale + 30.0);
        p.shadow(x0, y, sw, totale + 40.0 + mb, 24.0, 24.0, 0.25, -4.0);
        p.rrect(x0, y, sw, totale + 40.0 + mb, 24.0, Some(pal.surface), None);
        p.rrect(x0 + sw / 2.0 - 18.0, y + 9.0, 36.0, 4.0, 2.0, Some(pal.border), None);
        self.colpi.push((Forma::R((x0, y, sw, totale + 40.0)), Azione::Niente));
        (x0 + 16.0, y + 26.0, sw - 32.0)
    }

    fn tempo(&self, da: Instant) -> f32 {
        ease(self.ora.duration_since(da).as_secs_f32() / DURATA_FOGLIO)
    }

    fn foglio(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.ora = Instant::now();
        let t = self.tempo(self.apertura);
        // Dietro, il file in grande.
        let (iw, ih) = (w.min(560.0) - 48.0, (h * 0.42).min(w - 48.0));
        let (ix, iy) = ((w - iw) / 2.0, h * 0.08 + 8.0);
        match &self.miniatura {
            Some(img) => p.image(img, ix, iy, iw, ih, 16.0, Fit::Contain),
            None => {
                p.rrect(ix, iy, iw, ih, 16.0, Some(pal.alt), None);
                p.icon(icona_tipo(self.kind), ix + iw / 2.0 - 28.0, iy + ih / 2.0 - 28.0, 56.0, pal.primary);
            }
        }
        let Some(file) = self.file.clone() else { return };
        let tutte = righe(self.kind);
        let lavorando = self.lavoro != Lavoro::Fermo;
        let intest = 64.0;
        let rh = 58.0;
        let ripiano = 34.0;
        let contenuto;
        let mut nrighe = tutte.len();
        if lavorando {
            contenuto = intest + 12.0 + 148.0;
        } else {
            let fisso = intest + 10.0 + ripiano + 8.0;
            let max = (((h - 20.0 - 40.0 - fisso) / rh).floor() as usize).max(2);
            // L'ultima riga è sempre «Tutte le azioni».
            nrighe = nrighe.min(max - 1);
            contenuto = fisso + (nrighe + 1) as f32 * rh;
        }
        let (x, mut y, sw) = self.foglio_base(p, w, h, t, contenuto, Azione::Fine);
        // Intestazione: miniatura, nome, tipo e peso.
        match &self.miniatura {
            Some(img) => p.image(img, x, y + 4.0, 52.0, 52.0, 10.0, Fit::Cover),
            None => {
                p.rrect(x, y + 4.0, 52.0, 52.0, 10.0, Some(pal.alt), None);
                p.icon(icona_tipo(self.kind), x + 14.0, y + 18.0, 24.0, pal.primary);
            }
        }
        p.text(&file.nome, x + 64.0, y + 4.0, sw - 64.0, 28.0, 17.0, Weight::Semibold, pal.text, Align::Left);
        p.text(&format!("{} · {}", nome_tipo(self.kind), peso(file.dim)), x + 64.0, y + 30.0, sw - 64.0, 24.0, 14.0, Weight::Regular, pal.muted, Align::Left);
        y += intest + 6.0;
        if lavorando {
            self.esito(p, x, y + 4.0, sw);
            return;
        }
        for (i, riga) in tutte.iter().take(nrighe).enumerate() {
            let ry = y + i as f32 * rh;
            self.riga_vista(p, (x, ry, sw, rh - 6.0), icona_di(riga.id), riga.titolo, riga.sotto, Azione::Riga(i), false);
        }
        let ry = y + nrighe as f32 * rh;
        self.riga_vista(p, (x, ry, sw, rh - 6.0), ic::CONVERT, "Tutte le azioni", "La ruota con tutti i formati", Azione::TutteLeAzioni, false);
        p.text("Le copie vanno nella cartella Documenti.", x, ry + rh + 2.0, sw, ripiano - 4.0, 13.0, Weight::Regular, pal.muted, Align::Center);
    }

    #[allow(clippy::too_many_arguments)]
    fn riga_vista(&mut self, p: &mut Painter, rc: R, icona: &str, titolo: &str, sotto: &str, az: Azione, scelta: bool) {
        let pal = self.pal();
        let prem = self.premuto == Some(az);
        let fondo = if scelta { pal.primary } else if prem { mix(pal.alt, pal.primary, 0.2) } else { pal.bg };
        p.rrect(rc.0, rc.1, rc.2, rc.3, 14.0, Some(fondo), Some((pal.border, 1.0)));
        let fg = if scelta { pal.on_primary } else { pal.text };
        let sg = if scelta { pal.on_primary } else { pal.muted };
        p.rrect(rc.0 + 10.0, rc.1 + (rc.3 - 34.0) / 2.0, 34.0, 34.0, 10.0, Some(if scelta { mix(pal.primary, pal.on_primary, 0.25) } else { pal.alt }), None);
        p.icon(icona, rc.0 + 16.0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, if scelta { pal.on_primary } else { pal.primary });
        if sotto.is_empty() {
            p.text(titolo, rc.0 + 56.0, rc.1, rc.2 - 100.0, rc.3, 17.0, Weight::Semibold, fg, Align::Left);
        } else {
            p.text(titolo, rc.0 + 56.0, rc.1 + 4.0, rc.2 - 100.0, 26.0, 17.0, Weight::Semibold, fg, Align::Left);
            p.text(sotto, rc.0 + 56.0, rc.1 + 26.0, rc.2 - 100.0, 20.0, 13.0, Weight::Regular, sg, Align::Left);
        }
        if scelta {
            p.icon(ic::CHECK, rc.0 + rc.2 - 36.0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, fg);
        } else {
            p.icon(ic::ARROW_RIGHT, rc.0 + rc.2 - 34.0, rc.1 + (rc.3 - 20.0) / 2.0, 20.0, sg);
        }
        // Il bersaglio copre tutta la riga con lo spazio fra una e l'altra (≥ 48 dp).
        self.colpi.push((Forma::R((rc.0, rc.1 - 3.0, rc.2, rc.3 + 6.0)), az));
    }

    fn foglio_temi(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let t = self.tempo(self.apertura_aspetto);
        let rh = if h < 480.0 { 50.0 } else { 58.0 };
        let contenuto = 44.0 + TEMI.len() as f32 * rh + 6.0;
        let (x, y, sw) = self.foglio_base(p, w, h, t, contenuto, Azione::ChiudiAspetto);
        p.text("Aspetto", x, y, sw, 40.0, 21.0, Weight::Semibold, pal.text, Align::Left);
        for (i, tema) in TEMI.into_iter().enumerate() {
            let rc = (x, y + 44.0 + i as f32 * rh, sw, rh - 6.0);
            let attivo = tema == self.tema;
            let prem = self.premuto == Some(Azione::Tema(i));
            let pa = tema.palette();
            p.rrect(rc.0, rc.1, rc.2, rc.3, 14.0, Some(if prem { mix(pal.alt, pal.primary, 0.2) } else { pal.bg }), Some((if attivo { pal.primary } else { pal.border }, if attivo { 2.0 } else { 1.0 })));
            // Una tavolozza in miniatura: sfondo, tinta e colore del tema.
            p.circle(rc.0 + 28.0, rc.1 + rc.3 / 2.0, 15.0, Some(pa.bg), Some((pa.border, 1.5)));
            p.disc_a(rc.0 + 28.0, rc.1 + rc.3 / 2.0, 9.0, pa.primary, 1.0);
            p.text(tema.name(), rc.0 + 56.0, rc.1, rc.2 - 100.0, rc.3, 17.0, Weight::Semibold, pal.text, Align::Left);
            if attivo {
                p.icon(ic::CHECK, rc.0 + rc.2 - 38.0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, pal.primary);
            }
            self.colpi.push((Forma::R((rc.0, rc.1 - 3.0, rc.2, rc.3 + 6.0)), Azione::Tema(i)));
        }
    }
}

/// Il contorno della fetta tra due raggi e due angoli, con uno spazio costante `gap` da ogni lato.
fn sector(cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32, gap: f32) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    let steps = 24;
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

#[allow(dead_code)]
fn _usa(_: Rgb) {}
