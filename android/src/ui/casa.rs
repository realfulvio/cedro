//! La finestra principale, quella che si apre dall'icona di Cedro: le disposizioni del computer portate sul
//! telefono, una per tema (Essenziale, Guidato a passi, Compatto, Banco di lavoro, Documento al centro).
//! Quattro attività (Conversioni, Trascrizione, YouTube, Video dal web), una coda di file per categoria,
//! l'ispettore con formato e opzioni (`cedro::conversioni`) e il pulsante grande in basso.
//! Il colore del tema va solo sul pulsante principale e sugli elementi attivi; ogni tipo di file e ogni
//! attività ha il suo colore (più chiaro nei temi scuri). «Condividi» invece apre la ruota (`ui.rs`).

use std::collections::HashMap;
use std::path::PathBuf;

use cedro::conversioni::{self, Control, OptionDef, Values};
use cedro::ui::gfx::{Align, Fit, Image, Painter, Weight};
use cedro::ui::theme::{mix, ThemeId};
use cedro::ui::voci;
use cedro::Kind;

use super::opzioni::OpzAz;
use super::pdf::Campo;
use super::{ic, App, Archivio, Azione, Effetto, Forma, Lavoro, Scheda, Sorgente, Stato, R, TOCCO};

type Rgb = (u8, u8, u8);

/// Una riga del foglio a comparsa: nome, iconcina (colore, tratto, parte piena) e due parole grigie.
struct RigaElenco {
    nome: String,
    icona: Option<(Rgb, &'static str, &'static str)>,
    nota: &'static str,
}

impl RigaElenco {
    fn semplice(nome: &str, nota: &'static str) -> RigaElenco {
        RigaElenco { nome: nome.to_string(), icona: None, nota }
    }
}

/// Un file in coda.
#[derive(Clone)]
pub struct Voce {
    pub path: PathBuf,
    pub nome: String,
    pub dim: u64,
    pub kind: Kind,
    pub thumb: Option<Image>,
    pub misure: Option<(u32, u32)>,
    pub scelto: bool,
    /// Il lavoro partito per questo file (per lo stato nel Banco di lavoro).
    pub lavoro: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Att {
    Conv,
    Tras,
    Yt,
    Web,
    Firma,
}

impl Att {
    fn nome(self) -> &'static str {
        match self {
            Att::Conv => "Conversioni",
            Att::Tras => "Trascrizione",
            Att::Yt => "YouTube",
            Att::Web => "Video dal web",
            Att::Firma => "Firma digitale",
        }
    }

    /// Il nome corto, quando le schede non stanno tutte.
    fn corto(self) -> &'static str {
        match self {
            Att::Conv => "Converti",
            Att::Tras => "Trascrivi",
            Att::Web => "Web",
            Att::Firma => "Firma",
            a => a.nome(),
        }
    }

    fn icona(self) -> &'static str {
        match self {
            Att::Conv => ic::CONVERT,
            Att::Tras => ic::TRANSCRIBE,
            Att::Yt => I_YT,
            Att::Web => ic::WEB,
            Att::Firma => ic::SHIELD,
        }
    }
}

/// Le attività di questa build (YouTube solo con yt-dlp nell'APK).
pub fn attivita() -> Vec<Att> {
    if cfg!(feature = "youtube") {
        vec![Att::Conv, Att::Tras, Att::Yt, Att::Web, Att::Firma]
    } else {
        vec![Att::Conv, Att::Tras, Att::Web, Att::Firma]
    }
}

const I_YT: &str = "M3 7a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3v10a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3zM10 9l5 3-5 3z";
const I_PDF: &str = "M6 3h8l4 4v14H6zM14 3v5h4M9 13h6M9 17h4";
const I_TXT: &str = "M5 6h14M5 11h14M5 16h9";
const I_ZIP: &str = "M6 3h12v18H6zM12 3v2M12 7v2M12 11v2M11 15h2v3h-2z";
const I_FRECCIA: &str = "M5 12h14M13 6l6 6-6 6";
const I_GIU: &str = "M6 9l6 6 6-6";
const I_SPUNTA: &str = "M5 12l5 5 9-10";
const I_SCUDO: &str = "M5 12l5 5 9-10";

/// La chiave di una categoria nelle scelte della finestra principale.
pub fn ck(k: Kind) -> u8 {
    conversioni::CATEGORIES.iter().position(|c| c.kind == k).unwrap_or(255) as u8
}

/// Il colore di un tipo di file: uguale in tutti i temi, più chiaro in quelli scuri.
pub fn colore_kind(k: Kind, scuro: bool) -> Rgb {
    schiarisci(cedro::ui::theme::kind_color(k), scuro)
}

/// Nei temi scuri i colori di tipi e attività sono un po' più chiari, come sul computer.
fn schiarisci(c: Rgb, scuro: bool) -> Rgb {
    if scuro { mix(c, (255, 255, 255), 0.28) } else { c }
}

/// Il viola della firma digitale.
pub fn colore_firma(scuro: bool) -> Rgb {
    colore_att(Att::Firma, scuro)
}

/// Il colore di un'attività: gli stessi del computer (Conversioni blu, Trascrizione arancio, YouTube rosso, Web verde).
pub fn colore_att(a: Att, scuro: bool) -> Rgb {
    use cedro::ui::model::Activity;
    let att = match a {
        Att::Conv => Activity::Convert,
        Att::Tras => Activity::Transcribe,
        Att::Yt => Activity::YouTube,
        Att::Web => Activity::Web,
        Att::Firma => Activity::Firma,
    };
    schiarisci(cedro::ui::theme::activity_color(att), scuro)
}

pub fn icona_kind(k: Kind) -> &'static str {
    match k {
        Kind::Image => ic::IMAGE,
        Kind::Doc => ic::FILE,
        Kind::Pdf => I_PDF,
        Kind::Text => I_TXT,
        Kind::Audio => ic::AUDIO,
        Kind::Video => ic::VIDEO,
        Kind::Archive => I_ZIP,
        Kind::Other => ic::FILE,
    }
}

/// Il tipo di file che esce da un formato (per il colore delle tessere dei formati).
pub(super) fn kind_uscita(da: Kind, id: &str) -> Kind {
    match id {
        "pdf" | "z-pdf" => Kind::Pdf,
        "docx" => Kind::Doc,
        "xlsx" | "txt" | "html" | "md" | "csv" | "json" => Kind::Text,
        "mp3" | "wav" | "flac" | "opus" | "aiff" | "z-mp3" | "z-wav" => Kind::Audio,
        "mp4" => Kind::Video,
        "jpg" | "png" | "jpeg" | "webp" | "z-webp" | "z-jpeg" | "z-png" => Kind::Image,
        _ => da,
    }
}

fn plurale(k: Kind, n: usize) -> String {
    let (uno, molti) = match k {
        Kind::Image => ("immagine", "immagini"),
        Kind::Doc => ("documento", "documenti"),
        Kind::Pdf => ("PDF", "PDF"),
        Kind::Text => ("testo", "testi"),
        Kind::Audio => ("audio", "audio"),
        Kind::Video => ("video", "video"),
        Kind::Archive => ("archivio", "archivi"),
        Kind::Other => ("file", "file"),
    };
    format!("{n} {}", if n == 1 { uno } else { molti })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CasaAz {
    Att(usize),
    Categoria(usize),
    Aggiungi,
    Cartella,
    Spunta(usize),
    Togli(usize),
    /// Una voce della striscia del Documento al centro.
    Mostra(usize),
    /// Apre un elenco a comparsa.
    Apri(Elenco),
    /// Una voce dell'elenco a comparsa aperto.
    Voce(usize),
    ChiudiElenco,
    Interruttore(usize),
    AltreOpzioni,
    Vai,
    Passo(i8),
    Formato(usize),
    Modello(usize),
    Web(usize),
    Incolla,
    Impostazioni,
    Lavori,
    Guida,
    Indietro,
    /// «Altri file in coda»: va alla prima categoria con dei file.
    AltraCategoria(usize),
    /// Un file di «Ultimi file» va in coda.
    Recente(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Elenco {
    Formato,
    /// L'opzione a scelta dell'ispettore (indice nelle definizioni).
    Opzione(usize),
    Esiste,
    Attivita,
    /// Quante persone parlano (trascrizione).
    Voci,
}

pub struct Casa {
    pub att: Att,
    pub categoria: Kind,
    pub coda: Vec<Voce>,
    /// L'operazione scelta per ogni categoria (un formato o uno strumento).
    pub operazione: HashMap<u8, &'static str>,
    pub valori: HashMap<u8, Values>,
    pub elenco: Option<Elenco>,
    /// Il passo del Percorso guidato (0, 1, 2).
    pub passo: u8,
    /// Il file mostrato in grande nel Documento al centro.
    pub mostrato: usize,
    pub link: String,
    pub web: usize,
    pub guida: bool,
    /// Lo scorrimento di lato delle fasce col dito: 0 = le categorie, 1 = la striscia dei file del Documento al centro.
    pub fx: [f32; 2],
    pub fx_max: [f32; 2],
    /// La categoria già portata a vista nella fascia (quando cambia, la fascia la segue).
    segui: Option<Kind>,
    /// Il Documento al centro mostra tutti i file nella striscia (la categoria segue quello mostrato).
    pub tutti: bool,
    /// Lo scorrimento dell'elenco a comparsa.
    pub elenco_scorri: f32,
    pub elenco_max: f32,
}

impl Default for Casa {
    fn default() -> Self {
        Casa { att: Att::Conv, categoria: Kind::Image, coda: Vec::new(), operazione: HashMap::new(), valori: HashMap::new(), elenco: None, passo: 0, mostrato: 0, link: String::new(), web: 0, guida: false, fx: [0.0; 2], fx_max: [0.0; 2], segui: None, tutti: false, elenco_scorri: 0.0, elenco_max: 0.0 }
    }
}

const WEB: [(&str, &str, &str); 4] = [("web-video", "Video", "Il migliore che c'è"), ("web-audio", "Solo audio", "MP3 dal video"), ("web-leggero", "Video leggero", "Meno spazio"), ("web-testo", "Testo", "Dalla voce, offline")];

impl Casa {
    fn categorie() -> &'static [conversioni::Category] {
        conversioni::CATEGORIES
    }

    /// I file della coda per l'attività di ora.
    fn visibili(&self) -> Vec<usize> {
        (0..self.coda.len())
            .filter(|i| {
                let k = self.coda[*i].kind;
                match self.att {
                    Att::Conv => k == self.categoria || self.tutti,
                    Att::Tras => matches!(k, Kind::Audio | Kind::Video),
                    _ => false,
                }
            })
            .collect()
    }

    fn scelti(&self) -> Vec<usize> {
        self.visibili().into_iter().filter(|i| self.coda[*i].scelto && (self.att != Att::Conv || self.coda[*i].kind == self.categoria)).collect()
    }

    fn quanti(&self, k: Kind) -> usize {
        self.coda.iter().filter(|v| v.kind == k).count()
    }

    /// L'operazione scelta per la categoria di ora (di partenza: il primo formato del pannello).
    fn operazione(&self) -> &'static str {
        self.operazione.get(&ck(self.categoria)).copied().unwrap_or_else(|| conversioni::formats(self.categoria).first().map(|s| s.id).unwrap_or("none"))
    }

    fn valori(&self) -> Values {
        self.valori.get(&ck(self.categoria)).cloned().unwrap_or_default()
    }

    /// Le opzioni dell'operazione di ora, quelle che si vedono.
    fn defs(&self) -> Vec<&'static OptionDef> {
        let v = self.valori();
        conversioni::option_defs(self.categoria, self.operazione()).into_iter().filter(|d| d.opt != conversioni::Opt::NameTemplate && conversioni::shown(d, &v)).collect()
    }

    /// Il nome dell'operazione («PNG», «Comprimi»).
    fn etichetta(&self) -> &'static str {
        voci::voce(self.categoria, self.operazione()).label
    }

    /// L'altra categoria con dei file, per «Altri file in coda».
    fn altra(&self) -> Option<(usize, Kind, usize)> {
        Casa::categorie().iter().enumerate().find(|(_, c)| c.kind != self.categoria && self.quanti(c.kind) > 0).map(|(i, c)| (i, c.kind, self.quanti(c.kind)))
    }
}

impl App {
    fn scuro(&self) -> bool {
        self.tema.dark()
    }

    /// Torna alla finestra principale (dall'icona di Cedro).
    pub fn vai_casa(&mut self) {
        self.casa_aperta = true;
        self.sopra = None;
        self.file = None;
        self.foglio = false;
        self.dal_foglio = false;
        self.scelta_file = false;
        self.pdf = super::StatoPdf::default();
        self.opzioni = None;
        self.rivedi = None;
        self.scorri = 0.0;
    }

    /// File nuovi in coda (scelti con «Aggiungi file» o da una cartella).
    pub fn aggiungi_coda(&mut self, files: Vec<(PathBuf, String, Kind)>) {
        let c = &mut self.casa;
        let mut primo = None;
        for (path, nome, kind) in files {
            if c.coda.iter().any(|v| v.path == path) {
                continue;
            }
            let dim = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            primo.get_or_insert(kind);
            c.coda.push(Voce { path, nome, dim, kind, thumb: None, misure: None, scelto: true, lavoro: None });
        }
        // La categoria segue i file appena aggiunti, se quella di ora è vuota.
        if let Some(k) = primo {
            if c.att == Att::Conv && c.quanti(c.categoria) == 0 && k != Kind::Other {
                c.categoria = k;
            }
            if c.att == Att::Tras && !matches!(k, Kind::Audio | Kind::Video) {
                c.att = Att::Conv;
                c.categoria = k;
            }
        }
    }

    /// La miniatura e le misure di un file in coda.
    pub fn info_voce(&mut self, path: &std::path::Path, thumb: Option<Image>, misure: Option<(u32, u32)>) {
        for v in self.casa.coda.iter_mut().filter(|v| v.path == path) {
            v.thumb = thumb.clone();
            v.misure = misure;
        }
    }

    /// Il testo del link dagli appunti.
    pub fn incolla(&mut self, t: String) {
        if let Some(url) = cedro::webvideo::find_url(&t) {
            self.casa.link = url;
        }
    }

    pub(super) fn tasto_casa(&mut self, t: super::Tasto) -> bool {
        let s = &mut self.casa.link;
        match t {
            super::Tasto::Canc => {
                s.pop();
            }
            super::Tasto::Car(c) if !c.is_control() && s.len() < 2000 => s.push(c),
            _ => return false,
        }
        true
    }

    pub(super) fn azione_casa(&mut self, az: CasaAz) -> Effetto {
        let att = attivita();
        match az {
            CasaAz::Att(i) => {
                if let Some(a) = att.get(i) {
                    self.casa.att = *a;
                    self.casa.passo = 0;
                    self.casa.elenco = None;
                    self.scorri = 0.0;
                }
            }
            CasaAz::Categoria(i) => {
                if let Some(c) = Casa::categorie().get(i) {
                    self.casa.categoria = c.kind;
                    self.casa.mostrato = 0;
                    self.scorri = 0.0;
                }
            }
            CasaAz::AltraCategoria(i) => return self.azione_casa(CasaAz::Categoria(i)),
            CasaAz::Aggiungi => return Effetto::Scegli(Sorgente::Coda),
            CasaAz::Cartella => return Effetto::Scegli(Sorgente::Cartella),
            CasaAz::Spunta(i) => {
                if let Some(v) = self.casa.coda.get_mut(i) {
                    v.scelto = !v.scelto;
                }
            }
            CasaAz::Togli(i) => {
                if i < self.casa.coda.len() {
                    self.casa.coda.remove(i);
                    self.casa.mostrato = 0;
                }
            }
            CasaAz::Mostra(i) => {
                // Nel Documento al centro la categoria segue il file mostrato.
                self.casa.mostrato = i;
                if let Some(j) = self.casa.visibili().get(i).copied() {
                    self.casa.categoria = self.casa.coda[j].kind;
                    // Resta quel file (non il primo dello stesso tipo), anche se cambiando categoria cambiano i visibili.
                    self.casa.mostrato = self.casa.visibili().iter().position(|v| *v == j).unwrap_or(0);
                }
            }
            CasaAz::Recente(i) => {
                if let Some(r) = self.recenti.get(i).cloned() {
                    self.aggiungi_coda(vec![(r.path.clone(), r.nome.clone(), r.kind)]);
                    if let Some(v) = self.casa.coda.iter_mut().find(|v| v.path == r.path) {
                        v.thumb = r.thumb.clone();
                    }
                    return Effetto::LeggiVoci(vec![r.path]);
                }
            }
            CasaAz::Apri(e) => {
                self.casa.elenco = Some(e);
                self.casa.elenco_scorri = 0.0;
            }
            CasaAz::ChiudiElenco => self.casa.elenco = None,
            CasaAz::Voce(k) => {
                let e = self.casa.elenco.take();
                match e {
                    Some(Elenco::Formato) => {
                        let tutte = self.operazioni();
                        if let Some(id) = tutte.get(k).map(|s| s.0) {
                            self.casa.operazione.insert(ck(self.casa.categoria), id);
                        }
                    }
                    Some(Elenco::Opzione(d)) => {
                        let defs = self.casa.defs();
                        if let Some(def) = defs.get(d) {
                            if let Control::Choice(c) | Control::Segments(c) = &def.control {
                                if let Some(ch) = c.get(k) {
                                    let cat = self.casa.categoria;
                                    let mut v = self.casa.valori();
                                    v.set(def, ch.value);
                                    self.casa.valori.insert(ck(cat), v);
                                }
                            }
                        }
                    }
                    Some(Elenco::Esiste) => {
                        self.esiste = [cedro::Exists::Number, cedro::Exists::Replace, cedro::Exists::Skip][k.min(2)];
                        return Effetto::SalvaImpostazioni;
                    }
                    Some(Elenco::Attivita) => return self.azione_casa(CasaAz::Att(k)),
                    Some(Elenco::Voci) => self.voci = (k as u8).min(8),
                    None => {}
                }
            }
            CasaAz::Interruttore(d) => {
                let defs = self.casa.defs();
                if let Some(def) = defs.get(d) {
                    let cat = self.casa.categoria;
                    let mut v = self.casa.valori();
                    let ora = v.get(def);
                    v.set(def, (ora == 0) as u32);
                    self.casa.valori.insert(ck(cat), v);
                }
            }
            CasaAz::AltreOpzioni => {
                let cat = self.casa.categoria;
                let f = super::opzioni::formati(cat);
                let formato = f.iter().position(|s| s.id == self.casa.operazione()).unwrap_or(0);
                self.opzioni = Some(super::opzioni::Opzioni { formato, valori: self.casa.valori(), da_casa: Some(cat) });
                self.scorri = 0.0;
            }
            CasaAz::Formato(k) => {
                let tutte = self.operazioni();
                if let Some(id) = tutte.get(k).map(|s| s.0) {
                    self.casa.operazione.insert(ck(self.casa.categoria), id);
                }
            }
            CasaAz::Modello(m) => self.modello = m,
            CasaAz::Web(k) => self.casa.web = k,
            CasaAz::Incolla => return Effetto::Incolla,
            CasaAz::Passo(d) => {
                let p = (self.casa.passo as i8 + d).clamp(0, 2) as u8;
                self.casa.passo = p;
                self.scorri = 0.0;
            }
            CasaAz::Vai => return self.vai_casa_lavori(),
            CasaAz::Impostazioni => {
                self.sopra = Some(Scheda::Aspetto);
                self.scorri = 0.0;
            }
            CasaAz::Lavori => {
                self.sopra = Some(Scheda::Lavori);
                self.scorri = 0.0;
            }
            CasaAz::Guida => self.casa.guida = true,
            CasaAz::Indietro => {
                if self.casa.guida {
                    self.casa.guida = false;
                } else {
                    self.sopra = None;
                }
                self.scorri = 0.0;
            }
        }
        Effetto::Ridisegna
    }

    /// Le operazioni della categoria: i formati e gli strumenti (id, nome).
    fn operazioni(&self) -> Vec<(&'static str, &'static str)> {
        let k = self.casa.categoria;
        conversioni::formats(k).iter().chain(conversioni::tools(k)).filter(|s| s.id != "none").map(|s| (s.id, s.label)).collect()
    }

    /// Il pulsante grande: un lavoro per ogni file scelto (o il link).
    fn vai_casa_lavori(&mut self) -> Effetto {
        let att = self.casa.att;
        if att == Att::Firma {
            return Effetto::Scegli(super::Sorgente::Firmati);
        }
        if matches!(att, Att::Yt | Att::Web) {
            let link = self.casa.link.trim().to_string();
            if cedro::webvideo::find_url(&link).is_none() {
                return Effetto::Niente;
            }
            let (id, nome, _) = WEB[self.casa.web.min(3)];
            let host = link.split_once("://").map(|(_, r)| r).unwrap_or(&link).split('/').next().unwrap_or("").to_string();
            self.file = Some(Archivio { path: PathBuf::from(&link), nome: host, dim: 0 });
            self.kind = Kind::Video;
            let e = self.avvia(id, nome, false);
            self.file = None;
            self.sopra = Some(Scheda::Lavori);
            return e;
        }
        let scelti = self.casa.scelti();
        if scelti.is_empty() {
            return Effetto::Niente;
        }
        let (id, etichetta, conv): (&'static str, &str, bool) = if att == Att::Tras { ("trascrivi", "Testo", false) } else { (self.casa.operazione(), self.casa.etichetta(), conversioni::formats(self.casa.categoria).iter().any(|s| s.id == self.casa.operazione())) };
        // Le operazioni dei PDF con la loro schermata (Unisci, Dividi, Pagine, Comprimi) la aprono, con il primo file.
        if att == Att::Conv && self.casa.categoria == Kind::Pdf && super::pdf::schermata(id).is_some() {
            let v = self.casa.coda[scelti[0]].clone();
            self.file = Some(Archivio { path: v.path.clone(), nome: v.nome.clone(), dim: v.dim });
            self.kind = Kind::Pdf;
            self.miniatura = v.thumb.clone();
            self.pdf = super::StatoPdf::default();
            if id == "merge" {
                let altri: Vec<(PathBuf, String)> = scelti[1..].iter().map(|i| (self.casa.coda[*i].path.clone(), self.casa.coda[*i].nome.clone())).collect();
                let e = self.apri_pagina(super::pdf::Pagina::Unisci);
                if !altri.is_empty() {
                    self.aggiungi_pdf(altri.clone());
                    return Effetto::LeggiPdf(v.path, altri.into_iter().map(|a| a.0).collect());
                }
                return e;
            }
            self.apri_pagina(super::pdf::schermata(id).unwrap_or(super::pdf::Pagina::Excel));
            return Effetto::LeggiPdf(v.path, Vec::new());
        }
        let opts = if att == Att::Tras { cedro::Options::default() } else { conversioni::options_from(&self.casa.valori()) };
        let mut compiti = Vec::new();
        for i in scelti {
            let v = self.casa.coda[i].clone();
            self.file = Some(Archivio { path: v.path.clone(), nome: v.nome.clone(), dim: v.dim });
            self.kind = v.kind;
            if let Effetto::Converti { id: lavoro, file, azione, qualita, prog, opts, unisci } = self.avvia_con(id, etichetta, conv, Some(Box::new(opts.clone())), Vec::new()) {
                self.casa.coda[i].lavoro = Some(lavoro);
                compiti.push(super::Compito { id: lavoro, file, azione, qualita, prog, opts, unisci });
            }
        }
        self.file = None;
        // Il Banco di lavoro mostra lo stato nella tabella; gli altri temi passano ai Lavori.
        self.sopra = if self.tema == ThemeId::BancoDiLavoro { None } else { Some(Scheda::Lavori) };
        Effetto::Tanti(compiti)
    }

    // ================================================================ disegno

    /// Per le fotografie della `vista`: l'attività `k` della finestra principale.
    pub fn vista_attivita(&mut self, k: usize) {
        self.azione_casa(CasaAz::Att(k));
    }

    /// Per le fotografie della `vista`: il foglio dei formati aperto.
    pub fn vista_formati(&mut self) {
        self.casa.elenco = Some(Elenco::Formato);
    }

    pub(super) fn pagina_casa(&mut self, p: &mut Painter, w: f32, h: f32) {
        if self.casa.guida {
            return self.pagina_guida(p, w, h);
        }
        self.casa.tutti = false;
        match self.tema {
            ThemeId::PercorsoGuidato => self.casa_guidato(p, w, h),
            ThemeId::CedroCompatto => self.casa_compatto(p, w, h),
            ThemeId::BancoDiLavoro => self.casa_banco(p, w, h),
            ThemeId::DocumentoAlCentro => self.casa_documento(p, w, h),
            _ => self.casa_essenziale(p, w, h),
        }
        if self.casa.elenco.is_some() {
            self.foglio_elenco(p, w, h);
        }
    }

    // ---- pezzi comuni ----

    /// Il riquadro colorato con l'icona (`.kt`).
    fn kt(&self, p: &mut Painter, x: f32, y: f32, lato: f32, col: Rgb, icona: &str) {
        let fondo = mix(self.pal().surface, col, 0.16);
        p.rrect(x, y, lato, lato, lato * 0.27, Some(fondo), None);
        let s = lato * 0.62;
        p.icon(icona, x + (lato - s) / 2.0, y + (lato - s) / 2.0, s, col);
    }

    /// L'iconcina di una voce del menu formati: piastrella appena tinta, icona del colore della voce.
    #[allow(clippy::too_many_arguments)]
    fn tessera(&self, p: &mut Painter, x: f32, y: f32, lato: f32, col: Rgb, tratto: &str, pieno: &str) {
        p.rrect(x, y, lato, lato, (lato * 0.27).round(), Some(mix(col, self.pal().surface, if self.scuro() { 0.8 } else { 0.86 })), None);
        let s = (lato * 0.62).round();
        let d = (lato - s) / 2.0;
        if !pieno.is_empty() {
            p.icon_fill(pieno, x + d, y + d, s, col);
        }
        p.icon(tratto, x + d, y + d, s, col);
    }

    /// L'etichetta colorata del formato (`.kb`): restituisce la larghezza.
    fn kb(&self, p: &mut Painter, x: f32, cy: f32, testo: &str, col: Rgb) -> f32 {
        let tw = p.measure(testo, 10.0, Weight::Bold) + 10.0;
        p.rrect(x, cy - 9.0, tw, 18.0, 5.0, Some(mix(self.pal().surface, col, 0.15)), None);
        p.text(testo, x, cy - 9.0, tw, 18.0, 10.0, Weight::Bold, col, Align::Center);
        tw
    }

    fn logo(&mut self, p: &mut Painter, x: f32, y: f32, h: f32, con_nome: bool, dim: f32) {
        let pal = self.pal();
        p.symbol(x, y + (h - 28.0) / 2.0, 28.0);
        if con_nome {
            p.text("Cedro", x + 36.0, y, 140.0, h, dim, Weight::Bold, pal.text, Align::Left);
        }
    }

    /// Ingranaggio (Impostazioni) e orologio dei Lavori, a destra della testata. Restituisce dove finiscono.
    fn testata_destra(&mut self, p: &mut Painter, w: f32, y: f32, h: f32, aiuto: bool) -> f32 {
        let pal = self.pal();
        let mut x = w - 4.0 - TOCCO;
        self.bottone_icona(p, (x, y + (h - TOCCO) / 2.0, TOCCO, TOCCO), ic::SETTINGS, Azione::Casa(CasaAz::Impostazioni));
        if aiuto {
            x -= TOCCO;
            self.bottone_icona(p, (x, y + (h - TOCCO) / 2.0, TOCCO, TOCCO), ic::HELP, Azione::Casa(CasaAz::Guida));
        }
        if !self.lavori.is_empty() {
            x -= TOCCO;
            self.bottone_icona(p, (x, y + (h - TOCCO) / 2.0, TOCCO, TOCCO), ic::CLOCK, Azione::Casa(CasaAz::Lavori));
            let attivi = self.lavori.iter().filter(|l| l.stato == Stato::InCorso).count();
            if attivi > 0 {
                p.circle(x + 36.0, y + (h - TOCCO) / 2.0 + 12.0, 9.0, Some(pal.primary), None);
                p.text(&attivi.to_string(), x + 27.0, y + (h - TOCCO) / 2.0 + 3.0, 18.0, 18.0, 11.0, Weight::Semibold, pal.on_primary, Align::Center);
            }
        }
        x
    }

    /// Un riquadro a tendina («sel»): testo, prefisso grigio, freccia. Senza azione non ha la freccia.
    #[allow(clippy::too_many_arguments)]
    fn sel(&mut self, p: &mut Painter, rc: R, prefisso: &str, testo: &str, raggio: f32, az: Option<Azione>, dim: f32) {
        let pal = self.pal();
        let prem = az.is_some() && self.premuto == az;
        p.rrect(rc.0, rc.1, rc.2, rc.3, raggio, Some(if prem { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        let mut x = rc.0 + 12.0;
        // La tendina del formato: davanti al nome l'iconcina colorata, come nel menu.
        if az == Some(Azione::Casa(CasaAz::Apri(Elenco::Formato))) && testo == self.casa.etichetta() {
            let v = voci::voce(self.casa.categoria, self.casa.operazione());
            let ts = (rc.3 - 14.0).min(28.0);
            self.tessera(p, x - 2.0, rc.1 + (rc.3 - ts) / 2.0, ts, schiarisci(v.color, self.scuro()), v.glyph.stroke, v.glyph.fill);
            x += ts + 8.0;
        }
        if !prefisso.is_empty() {
            let pw = p.measure(prefisso, dim, Weight::Regular) + 6.0;
            p.text(prefisso, x, rc.1, pw, rc.3, dim, Weight::Regular, pal.muted, Align::Left);
            x += pw;
        }
        let largo = rc.0 + rc.2 - x - if az.is_some() { 30.0 } else { 10.0 };
        p.text(testo, x, rc.1, largo, rc.3, dim, Weight::Regular, pal.text, Align::Left);
        if let Some(a) = az {
            p.icon(I_GIU, rc.0 + rc.2 - 26.0, rc.1 + rc.3 / 2.0 - 8.0, 16.0, pal.muted);
            self.colpi.push((Forma::R(rc), a));
        }
    }

    /// Il pulsante principale («.cta»), l'unico col colore del tema.
    #[allow(clippy::too_many_arguments)]
    fn cta(&mut self, p: &mut Painter, rc: R, raggio: f32, icona: Option<&str>, testo: &str, az: Option<Azione>, dim: f32) {
        let pal = self.pal();
        let fg = if az.is_some() { pal.on_primary } else { pal.muted };
        if az.is_none() {
            p.rrect(rc.0, rc.1, rc.2, rc.3, raggio, Some(pal.alt), None);
        } else {
            super::lucido(p, self.scuro(), rc.0, rc.1, rc.2, rc.3, raggio, pal.primary, self.premuto == az);
        }
        let tw = p.measure(testo, dim, Weight::Bold).min(rc.2 - 50.0);
        let iw = if icona.is_some() { 32.0 } else { 0.0 };
        let x0 = rc.0 + (rc.2 - tw - iw) / 2.0;
        if let Some(i) = icona {
            p.icon(i, x0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, fg);
        }
        p.text(testo, x0 + iw, rc.1, tw + 4.0, rc.3, dim, Weight::Bold, fg, Align::Left);
        if let Some(a) = az {
            self.colpi.push((Forma::R(rc), a));
        }
    }

    /// La miniatura di un file in coda (o l'icona del suo tipo).
    fn miniatura_voce(&self, p: &mut Painter, v: &Voce, rc: R, raggio: f32) {
        let col = colore_kind(v.kind, self.scuro());
        match &v.thumb {
            Some(img) => {
                p.rrect(rc.0, rc.1, rc.2, rc.3, raggio, Some((255, 255, 255)), None);
                p.image(img, rc.0, rc.1, rc.2, rc.3, raggio, if v.kind == Kind::Image { Fit::Cover } else { Fit::Contain });
            }
            None => {
                p.rrect(rc.0, rc.1, rc.2, rc.3, raggio, Some(mix(self.pal().surface, col, 0.16)), None);
                let s = rc.3.min(rc.2) * 0.5;
                p.icon(icona_kind(v.kind), rc.0 + (rc.2 - s) / 2.0, rc.1 + (rc.3 - s) / 2.0, s, col);
            }
        }
    }

    /// «57 KB · 1600 × 1200», con l'etichetta del formato davanti.
    fn info_voce_riga(&self, p: &mut Painter, v: &Voce, x: f32, cy: f32, w: f32, dim: f32, compatte: bool) {
        let pal = self.pal();
        let col = colore_kind(v.kind, self.scuro());
        let est = std::path::Path::new(&v.nome).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
        let bw = if est.is_empty() { 0.0 } else { self.kb(p, x, cy, &est, col) + 6.0 };
        let mut t = super::peso(v.dim);
        if let Some((a, b)) = v.misure {
            t += &if compatte { format!(" · {a}×{b}") } else { format!(" · {a} × {b}") };
        }
        p.text(&t, x + bw, cy - 10.0, w - bw, 20.0, dim, Weight::Regular, pal.muted, Align::Left);
    }

    /// Il quadratino di spunta.
    fn spunta(&mut self, p: &mut Painter, x: f32, cy: f32, on: bool, az: Azione) {
        let pal = self.pal();
        if on {
            p.rrect(x, cy - 11.0, 22.0, 22.0, 5.0, Some(pal.primary), None);
            p.icon(I_SPUNTA, x + 3.0, cy - 8.0, 16.0, pal.on_primary);
        } else {
            p.rrect(x, cy - 11.0, 22.0, 22.0, 5.0, Some(pal.surface), Some((pal.muted, 1.5)));
        }
        self.colpi.push((Forma::R((x - 13.0, cy - 24.0, 48.0, 48.0)), az));
    }

    fn interruttore_casa(&self, p: &mut Painter, x: f32, cy: f32, on: bool) {
        let pal = self.pal();
        p.rrect(x, cy - 14.0, 46.0, 28.0, 14.0, Some(if on { pal.primary } else { pal.border }), None);
        p.circle(if on { x + 32.0 } else { x + 14.0 }, cy, 10.0, Some(if on { pal.on_primary } else { pal.surface }), None);
    }

    /// Le opzioni principali dell'ispettore: la seconda a tendina e il primo interruttore.
    fn principali(&self) -> (Option<(usize, &'static OptionDef)>, Option<(usize, &'static OptionDef)>) {
        let defs = self.casa.defs();
        let scelta = defs.iter().enumerate().find(|(_, d)| matches!(d.control, Control::Choice(_) | Control::Segments(_))).map(|(i, d)| (i, *d));
        let sw = defs.iter().enumerate().find(|(_, d)| matches!(d.control, Control::Switch)).map(|(i, d)| (i, *d));
        (scelta, sw)
    }

    fn testo_scelta(&self, d: &OptionDef) -> String {
        let v = self.casa.valori().get(d);
        match &d.control {
            Control::Choice(c) | Control::Segments(c) => c.iter().find(|x| x.value == v).map(|x| x.label.to_string()).unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn altre_n(&self) -> usize {
        let (a, b) = self.principali();
        self.casa.defs().len().saturating_sub(a.is_some() as usize + b.is_some() as usize)
    }

    fn testo_esiste(&self) -> &'static str {
        match self.esiste {
            cedro::Exists::Replace => "Sostituisci",
            cedro::Exists::Skip => "Salta",
            _ => "-2",
        }
    }

    /// Una riga «titolo, spiegazione, interruttore» (Raddrizza le foto…). Restituisce l'altezza.
    fn riga_interruttore(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, k: usize, d: &OptionDef, dim: f32) -> f32 {
        let pal = self.pal();
        let on = self.casa.valori().get(d) != 0;
        let h = if d.note.is_empty() { 36.0 } else { 46.0 };
        p.text(d.title, x, y, w - 60.0, 24.0, dim, Weight::Regular, pal.text, Align::Left);
        if !d.note.is_empty() {
            p.text(d.note, x, y + 22.0, w - 60.0, 20.0, 12.0, Weight::Regular, pal.muted, Align::Left);
        }
        self.interruttore_casa(p, x + w - 46.0, y + h / 2.0, on);
        self.colpi.push((Forma::R((x, y, w, h)), Azione::Casa(CasaAz::Interruttore(k))));
        h
    }

    /// «Altri file in coda: 1 PDF».
    fn altri_in_coda(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, dim: f32, align_destra: bool) -> f32 {
        let pal = self.pal();
        let Some((i, k, n)) = self.casa.altra() else { return 0.0 };
        let a = "Altri file in coda: ";
        let b = if k == Kind::Pdf { format!("{n} PDF") } else { plurale(k, n) };
        let (aw, bw) = (p.measure(a, dim, Weight::Regular), p.measure(&b, dim, Weight::Bold));
        let x0 = if align_destra { x + w - aw - bw } else { x };
        p.text(a, x0, y, aw + 2.0, 22.0, dim, Weight::Regular, pal.muted, Align::Left);
        p.text(&b, x0 + aw, y, bw + 4.0, 22.0, dim, Weight::Bold, pal.primary, Align::Left);
        p.fill(x0 + aw, y + 19.0, bw, 1.2, pal.primary);
        self.colpi.push((Forma::R((x0 + aw - 6.0, y - 12.0, bw + 12.0, 46.0)), Azione::Casa(CasaAz::AltraCategoria(i))));
        22.0
    }

    /// «Ultimi file»: quelli adatti a quello che si fa ora e non ancora in coda, da aggiungere con un tocco.
    fn ultimi_casa(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let adatti: Vec<usize> = (0..self.recenti.len())
            .filter(|i| {
                let r = &self.recenti[*i];
                let k = r.kind;
                let ok = match self.casa.att {
                    Att::Conv => self.tema == ThemeId::DocumentoAlCentro || k == self.casa.categoria || self.casa.coda.is_empty(),
                    Att::Tras => matches!(k, Kind::Audio | Kind::Video),
                    _ => false,
                };
                ok && k != Kind::Other && !self.casa.coda.iter().any(|v| v.path == r.path)
            })
            .collect();
        if adatti.is_empty() {
            return 0.0;
        }
        let t = self.toni();
        p.text("Ultimi file", x + 4.0, y, w, 24.0, 14.0, Weight::Semibold, t.variante, Align::Left);
        let cella = 68.0;
        let n = (((w + 10.0) / (cella + 10.0)).floor() as usize).max(1).min(adatti.len());
        for (k, i) in adatti.iter().take(n).copied().enumerate() {
            let rc = (x + k as f32 * (cella + 10.0), y + 30.0, cella, cella);
            let r = self.recenti[i].clone();
            let col = colore_kind(r.kind, self.scuro());
            let (cont, _) = super::m3::contenitore(col, t.fondo, self.scuro());
            p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(cont), None);
            match &r.thumb {
                Some(img) => p.image(img, rc.0, rc.1, rc.2, rc.3, 16.0, Fit::Cover),
                None => p.icon(icona_kind(r.kind), rc.0 + 20.0, rc.1 + 20.0, 28.0, col),
            }
            // Il «+» in un cerchio pieno: tocca e il file va in coda.
            p.circle(rc.0 + rc.2 - 12.0, rc.1 + 12.0, 11.0, Some(pal.primary), None);
            p.icon(ic::ADD, rc.0 + rc.2 - 20.0, rc.1 + 4.0, 16.0, pal.on_primary);
            let nome = p.ellipsize(&r.nome, 11.5, Weight::Regular, rc.2);
            p.text(&nome, rc.0, rc.1 + rc.3 + 4.0, rc.2, 18.0, 11.5, Weight::Regular, t.variante, Align::Center);
            self.colpo((rc.0, rc.1, rc.2, rc.3 + 22.0), Azione::Casa(CasaAz::Recente(i)), vista);
        }
        30.0 + cella + 24.0 + 16.0
    }

    /// Il testo del pulsante grande.
    fn testo_cta(&self) -> (String, &'static str) {
        match self.casa.att {
            Att::Yt | Att::Web => ("Scarica".into(), ic::DOWNLOAD),
            Att::Firma => ("Scegli il file firmato".into(), ic::SHIELD),
            Att::Tras => {
                let n = self.casa.scelti().len();
                (if n == 1 { "Trascrivi 1 file".into() } else { format!("Trascrivi {n} file") }, ic::TRANSCRIBE)
            }
            Att::Conv => {
                let n = self.casa.scelti().len();
                let op = self.casa.operazione();
                if self.casa.categoria == Kind::Pdf && super::pdf::schermata(op).is_some() {
                    return (self.casa.etichetta().to_string(), ic::CONVERT);
                }
                (if n == 1 { "Converti 1 file".into() } else { format!("Converti {n} file") }, ic::CONVERT)
            }
        }
    }

    fn cta_pronta(&self) -> bool {
        match self.casa.att {
            Att::Yt | Att::Web => cedro::webvideo::find_url(&self.casa.link).is_some(),
            Att::Firma => true,
            Att::Conv if self.casa.categoria == Kind::Pdf && self.casa.operazione() == "merge" => self.casa.scelti().len() >= 2 || !self.casa.scelti().is_empty(),
            _ => !self.casa.scelti().is_empty(),
        }
    }

    /// Fissa lo scorrimento della fascia `f` larga `vis` con il contenuto largo `tot`; `voluto` = (inizio, fine) di
    /// una voce da portare a vista quando la categoria cambia. Registra la fascia per il dito. Restituisce lo scorrimento.
    fn fascia(&mut self, f: usize, rc: R, vis: f32, tot: f32, voluto: Option<(f32, f32)>) -> f32 {
        let max = (tot - vis).max(0.0);
        self.casa.fx_max[f] = max;
        let mut s = self.casa.fx[f];
        if f == 0 && self.casa.segui != Some(self.casa.categoria) {
            self.casa.segui = Some(self.casa.categoria);
            if let Some((a, b)) = voluto {
                if a < s {
                    s = a - 24.0;
                } else if b > s + vis {
                    s = b - vis + 24.0;
                }
            }
        }
        s = s.clamp(0.0, max);
        self.casa.fx[f] = s;
        if max > 0.0 {
            self.fasce.push((rc, f));
        }
        s
    }

    /// Le larghezze dei gettoni delle categorie (per la fascia che scorre).
    fn larghi_categorie(&self, p: &Painter, dim: f32, base: f32, pallino: f32, grassetto: bool) -> Vec<f32> {
        Casa::categorie()
            .iter()
            .map(|c| {
                let on = c.kind == self.casa.categoria;
                let tw = p.measure(c.name, dim, if on && grassetto { Weight::Semibold } else { Weight::Regular });
                base + tw + if self.casa.quanti(c.kind) > 0 && !on { pallino } else { 0.0 }
            })
            .collect()
    }

    /// Dove sta la categoria scelta nella fascia: (inizio, fine) dal bordo sinistro.
    fn pos_scelta(&self, larghi: &[f32], passo: f32) -> Option<(f32, f32)> {
        let i = Casa::categorie().iter().position(|c| c.kind == self.casa.categoria)?;
        let a: f32 = larghi[..i].iter().map(|l| l + passo).sum();
        Some((a, a + larghi[i]))
    }

    /// Le schede delle categorie (Essenziale), che scorrono di lato col dito: restituisce l'altezza.
    /// Le sfumature ai lati di una fascia che scorre, dove c'è ancora contenuto nascosto.
    fn sfumature(&self, p: &mut Painter, rc: R, s: f32, max: f32, bg: Rgb) {
        let passi = 8;
        for k in 0..passi {
            let a = 0.9 * (1.0 - k as f32 / passi as f32);
            if s > 0.5 {
                p.rrect_a(rc.0 + k as f32 * 3.0, rc.1, 3.0, rc.3 - 1.0, 0.0, bg, a);
            }
            if s < max - 0.5 {
                p.rrect_a(rc.0 + rc.2 - (k + 1) as f32 * 3.0, rc.1, 3.0, rc.3 - 1.0, 0.0, bg, a);
            }
        }
    }

    /// Il campo del link e le quattro scelte di «Video dal web» e YouTube. Restituisce l'altezza.
    fn corpo_web(&mut self, p: &mut Painter, x: f32, y0: f32, w: f32, vista: R) -> f32 {
        if self.casa.att == Att::Firma {
            return self.corpo_firma(p, x, y0, w, vista);
        }
        let pal = self.pal();
        let mut y = y0;
        let yt = self.casa.att == Att::Yt;
        y += p.text_wrap(if yt { "Incolla il link di un video di YouTube: Cedro lo scarica con yt-dlp." } else { "Incolla il link di una pagina con un video: Cedro trova il file e lo scarica." }, x, y, w, 15.0, Weight::Regular, pal.muted, 21.0, 3) + 10.0;
        let link = self.casa.link.clone();
        self.campo_testo(p, (x, y, w - 112.0, 52.0), Campo::Link, &link, "https://…", "", false, vista);
        let m3 = self.tema == ThemeId::WindowsEssenziale;
        let t = self.toni();
        let b = (x + w - 104.0, y + 4.0, 104.0, 44.0);
        let a = Azione::Casa(CasaAz::Incolla);
        if m3 {
            p.rrect(b.0, b.1, b.2, b.3, 22.0, Some(if self.premuto == Some(a) { mix(t.alto, t.testo, 0.08) } else { t.alto }), None);
        } else {
            p.rrect(b.0, b.1 - 4.0, b.2, 52.0, 12.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        }
        p.text("Incolla", b.0, b.1, b.2, b.3, 15.0, Weight::Semibold, pal.text, Align::Center);
        self.colpo(b, a, vista);
        y += 64.0;
        let col = colore_att(self.casa.att, self.scuro());
        let cw = (w - 10.0) / 2.0;
        for (k, (id, nome, sotto)) in WEB.iter().enumerate() {
            let rc = (x + (k % 2) as f32 * (cw + 10.0), y + (k / 2) as f32 * 80.0, cw, 72.0);
            let on = self.casa.web == k;
            let c = if *id == "web-testo" { colore_att(Att::Tras, self.scuro()) } else { col };
            if m3 {
                p.rrect(rc.0, rc.1, rc.2, rc.3, if on { 24.0 } else { 16.0 }, Some(if on { t.pri_c } else { t.basso }), None);
            } else {
                p.rrect(rc.0, rc.1, rc.2, rc.3, 12.0, Some(if on { mix(pal.surface, pal.primary, 0.08) } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
            }
            self.kt(p, rc.0 + 12.0, rc.1 + 11.0, 26.0, c, super::icona_di(id));
            let fg = if m3 && on { t.su_pri_c } else { pal.text };
            p.text(nome, rc.0 + 46.0, rc.1 + 9.0, rc.2 - 52.0, 28.0, 16.0, Weight::Semibold, fg, Align::Left);
            p.text(sotto, rc.0 + 12.0, rc.1 + 42.0, rc.2 - 18.0, 22.0, 13.0, Weight::Regular, if m3 && on { fg } else { pal.muted }, Align::Left);
            self.colpo(rc, Azione::Casa(CasaAz::Web(k)), vista);
        }
        y += 164.0;
        if self.casa.web == 3 {
            y += self.scelta_modello(p, x, y, w, vista);
        }
        y - y0
    }

    /// Veloce, Buona o Massima, per la trascrizione. Restituisce l'altezza.
    fn scelta_modello(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        p.text("Modello", x, y, w, 26.0, 14.0, Weight::Semibold, pal.muted, Align::Left);
        self.pillole_in(p, (x, y + 28.0, w, TOCCO), &["Veloce", "Buona", "Massima"], self.modello.min(2), |i| Azione::Casa(CasaAz::Modello(i)), vista);
        // Chi parla, come sul computer: con più persone il testo dice chi ha detto cosa.
        let cy = y + 28.0 + TOCCO + 10.0;
        p.text("Chi parla", x, cy, w, 26.0, 14.0, Weight::Semibold, pal.muted, Align::Left);
        let testo = match self.voci {
            0 => "Automatico".to_string(),
            1 => "Una persona sola".to_string(),
            n => format!("{n} persone"),
        };
        let rc = (x, cy + 28.0, w, 42.0);
        self.sel_in(p, rc, &testo, Azione::Casa(CasaAz::Apri(Elenco::Voci)), vista);
        28.0 + TOCCO + 10.0 + 28.0 + 42.0 + 8.0
    }

    /// L'ispettore della Trascrizione: modello e cosa esce. Restituisce l'altezza.
    fn ispettore_trascrizione(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let col = colore_att(Att::Tras, self.scuro());
        // Con i caratteri grandi la nota va a capo: il riquadro cresce con lei.
        const NOTA: &str = "Esce un file di testo con le frasi; poi «Rivedi» dai Lavori.";
        let hn = p.wrap_height(NOTA, 12.0, Weight::Regular, w - 28.0, 18.0, 3);
        let ch = 52.0 + 28.0 + TOCCO + 10.0 + 28.0 + 42.0 + 8.0 + hn + 12.0;
        super::sollevato(p, pal, self.tema.dark(), x, y, w, ch, 10.0);
        self.kt(p, x + 14.0, y + 12.0, 32.0, col, ic::TRANSCRIBE);
        p.text("Impostazioni della trascrizione", x + 56.0, y + 12.0, w - 70.0, 32.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        let h = self.scelta_modello(p, x + 14.0, y + 52.0, w - 28.0, vista);
        p.text_wrap(NOTA, x + 14.0, y + 52.0 + h, w - 28.0, 12.0, Weight::Regular, pal.muted, 18.0, 3);
        ch + 8.0
    }

    // ---- Essenziale ----

    fn casa_essenziale(&mut self, p: &mut Painter, w: f32, h: f32) {
        // Il tema base sul telefono: Material 3 Expressive (titolo grande, chip, superfici tonali, pillole).
        let pal = self.pal();
        let t = self.toni();
        let scuro = self.scuro();
        let att = attivita();
        let nav = 64.0;
        p.clear(t.fondo);
        // Testata: simbolo e nome, Lavori e Impostazioni; sotto il titolo grande dell'attività.
        self.logo(p, 18.0, 0.0, 52.0, true, 18.0);
        self.testata_destra(p, w, 0.0, 52.0, false);
        p.text(self.casa.att.nome(), 18.0, 54.0, w - 36.0, 44.0, 32.0, Weight::Bold, t.testo, Align::Left);
        let mut y = 106.0;
        if self.casa.att == Att::Conv {
            y += self.chip_categorie(p, 16.0, y, w - 32.0) + 12.0;
        }
        let firma = self.casa.att == Att::Firma;
        let piede = match self.casa.att {
            Att::Firma => 0.0,
            Att::Conv => 112.0,
            _ => 84.0,
        };
        let vista = (0.0, y, w, h - nav - piede - y);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let x = 16.0;
        let cw = w - 32.0;
        let mut cy = y + 2.0 - self.scorri;
        if matches!(self.casa.att, Att::Yt | Att::Web | Att::Firma) {
            cy += self.corpo_web(p, x, cy, cw, vista);
        } else {
            let vis = self.casa.visibili();
            if vis.is_empty() {
                // Vuoto: un invito, con i due pulsanti dentro.
                let testo = if self.casa.att == Att::Tras { "Aggiungi un audio o un video da trascrivere, oppure condividilo con Cedro da un'altra app." } else { "Aggiungi dei file, oppure condividili con Cedro da un'altra app." };
                let th = p.wrap_height(testo, 15.0, Weight::Regular, cw - 40.0, 21.0, 3);
                let bh = 20.0 + th + 16.0 + 44.0 + 20.0;
                p.rrect(x, cy, cw, bh, 24.0, Some(t.basso), None);
                p.text_wrap(testo, x + 20.0, cy + 20.0, cw - 40.0, 15.0, Weight::Regular, t.variante, 21.0, 3);
                let by = cy + 20.0 + th + 16.0;
                let bw = self.pillola_piccola(p, x + 20.0, by, ic::ADD, "Aggiungi file", Azione::Casa(CasaAz::Aggiungi), vista);
                self.tonale(p, x + 28.0 + bw, by, Some(ic::FOLDER), "Cartella", Azione::Casa(CasaAz::Cartella), vista);
                cy += bh + 12.0;
            } else {
                // I file: un elenco tonale, righe da 72 con la miniatura e la spunta tonda.
                let rh = 72.0;
                let lh = rh * vis.len() as f32 + 2.0 * (vis.len() as f32 - 1.0);
                p.rrect(x, cy, cw, lh, 24.0, Some(t.basso), None);
                for (k, i) in vis.iter().copied().enumerate() {
                    let v = self.casa.coda[i].clone();
                    let ry = cy + k as f32 * (rh + 2.0);
                    if k > 0 {
                        p.fill(x, ry - 2.0, cw, 2.0, t.fondo);
                    }
                    let a = Azione::Casa(CasaAz::Spunta(i));
                    if self.premuto == Some(a) {
                        p.rrect(x, ry, cw, rh, if vis.len() == 1 { 24.0 } else { 6.0 }, Some(mix(t.basso, t.testo, 0.05)), None);
                    }
                    self.miniatura_voce(p, &v, (x + 10.0, ry + 10.0, 52.0, 52.0), 14.0);
                    p.text(&v.nome, x + 76.0, ry + 12.0, cw - 130.0, 24.0, 15.5, Weight::Semibold, t.testo, Align::Left);
                    self.info_voce_riga(p, &v, x + 76.0, ry + 48.0, cw - 130.0, 12.5, false);
                    super::m3::spunta_tonda(p, pal, &t, x + cw - 30.0, ry + rh / 2.0, v.scelto);
                    self.colpo((x, ry, cw, rh), a, vista);
                }
                cy += lh + 10.0;
                let bw = self.tonale(p, x, cy, Some(ic::ADD), "Aggiungi file", Azione::Casa(CasaAz::Aggiungi), vista);
                self.tonale(p, x + bw + 8.0, cy, Some(ic::FOLDER), "Cartella", Azione::Casa(CasaAz::Cartella), vista);
                cy += 56.0;
            }
            let a = self.altri_in_coda(p, x + 4.0, cy, cw, 13.5, false);
            if a > 0.0 {
                cy += a + 12.0;
            }
            cy += self.ultimi_casa(p, x, cy, cw, vista);
            // Che cosa farne.
            p.text(if self.casa.att == Att::Tras { "Come trascrivo" } else { "Cosa ne faccio" }, x + 4.0, cy + 4.0, cw, 24.0, 14.0, Weight::Semibold, t.variante, Align::Left);
            cy += 34.0;
            if self.casa.att == Att::Tras {
                cy += self.trascrivi_m3(p, x, cy, cw, vista);
            } else {
                cy += self.cosa_ne_faccio(p, x, cy, cw, vista);
            }
        }
        p.pop_clip();
        self.scorri_max = (cy + 12.0 + self.scorri - (vista.1 + vista.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        // Il piede: dove finisce il file, poi la pillola grande.
        if !firma {
            let fy = h - nav - piede;
            // Una sfumatura sopra il piede: il contenuto ci scorre sotto.
            for k in 0..8 {
                p.rrect_a(0.0, fy - 16.0 + k as f32 * 2.0, w, 2.0, 0.0, t.fondo, (k as f32 + 1.0) / 9.0);
            }
            p.fill(0.0, fy, w, piede, t.fondo);
            let mut by = fy + 4.0;
            if self.casa.att == Att::Conv {
                let e = match self.esiste {
                    cedro::Exists::Replace => "sostituisco",
                    cedro::Exists::Skip => "lo salto",
                    _ => "aggiungo -2",
                };
                let a = "In Documenti/Cedro · se c'è già ";
                let (aw, ew) = (p.measure(a, 13.0, Weight::Regular), p.measure(e, 13.0, Weight::Bold));
                let x0 = (w - aw - ew) / 2.0;
                p.text(a, x0, by, aw + 2.0, 32.0, 13.0, Weight::Regular, t.variante, Align::Left);
                p.text(e, x0 + aw, by, ew + 4.0, 32.0, 13.0, Weight::Bold, pal.primary, Align::Left);
                self.colpi.push((Forma::R((x0 + aw - 12.0, by - 6.0, ew + 24.0, 44.0)), Azione::Casa(CasaAz::Apri(Elenco::Esiste))));
                by += 36.0;
            }
            let (tc, i) = self.testo_cta();
            let pronto = self.cta_pronta();
            self.pillola(p, (16.0, by, w - 32.0, 56.0), Some(i), &tc, pronto.then_some(Azione::Casa(CasaAz::Vai)), None, (0.0, 0.0, w, h));
        }
        // La barra di navigazione.
        let voci: Vec<(&str, &str, Rgb)> = att.iter().map(|a| (a.corto(), a.icona(), colore_att(*a, scuro))).collect();
        let scelta = att.iter().position(|a| *a == self.casa.att).unwrap_or(0);
        let voci_conv: Vec<(&str, &str, Rgb)> = voci.iter().map(|(n, i, c)| (*n, *i, if *c == colore_att(Att::Conv, scuro) { pal.primary } else { *c })).collect();
        self.barra_nav(p, w, h, &voci_conv, scelta, |k| Azione::Casa(CasaAz::Att(k)));
    }

    /// Una pillola piena piccola (44) col colore del tema. Restituisce la larghezza.
    fn pillola_piccola(&mut self, p: &mut Painter, x: f32, y: f32, icona: &str, testo: &str, az: Azione, vista: R) -> f32 {
        let pal = self.pal();
        let w = p.measure(testo, 14.5, Weight::Semibold) + 54.0;
        let premuto = self.premuto == Some(az);
        p.rrect(x, y, w, 44.0, 22.0, Some(if premuto { mix(pal.primary, (0, 0, 0), 0.14) } else { pal.primary }), None);
        p.icon(icona, x + 14.0, y + 12.0, 20.0, pal.on_primary);
        p.text(testo, x + 40.0, y, w - 50.0, 44.0, 14.5, Weight::Semibold, pal.on_primary, Align::Left);
        self.colpo((x, y, w, 44.0), az, vista);
        w
    }

    /// Le categorie come chip, in una fascia che scorre di lato. Restituisce l'altezza.
    fn chip_categorie(&mut self, p: &mut Painter, x: f32, y: f32, w: f32) -> f32 {
        let t = self.toni();
        let scuro = self.scuro();
        let passo = 8.0;
        let larghi: Vec<f32> = Casa::categorie().iter().map(|c| Self::largo_chip(p, c.name, self.casa.quanti(c.kind))).collect();
        let tot = larghi.iter().sum::<f32>() + passo * (larghi.len().saturating_sub(1)) as f32;
        let voluto = self.pos_scelta(&larghi, passo);
        let fascia = (0.0, y, x + w + 16.0, 36.0);
        let s = self.fascia(0, fascia, w, tot, voluto);
        let mut cx = x - s;
        p.push_clip(fascia.0, fascia.1, fascia.2, fascia.3);
        for (i, c) in Casa::categorie().iter().enumerate() {
            let on = c.kind == self.casa.categoria;
            let col = colore_kind(c.kind, scuro);
            let ww = Self::chip(p, &t, cx, y, icona_kind(c.kind), col, c.name, on, self.casa.quanti(c.kind), scuro);
            self.colpo((cx, y - 6.0, ww, 48.0), Azione::Casa(CasaAz::Categoria(i)), fascia);
            cx += ww + passo;
        }
        p.pop_clip();
        self.sfumature(p, fascia, s, self.casa.fx_max[0], t.fondo);
        36.0
    }

    /// «Cosa ne faccio»: formato, la prima opzione a scelta, il primo interruttore e le altre opzioni,
    /// come righe grandi in un contenitore tonale. Restituisce l'altezza.
    fn cosa_ne_faccio(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let t = self.toni();
        let scuro = self.scuro();
        let (scelta, sw) = self.principali();
        let pad = 6.0;
        let rh = 68.0;
        let mut righe_h = rh;
        if scelta.is_some() {
            righe_h += rh + pad;
        }
        let sw_h = sw.map(|(_, d)| {
            let nh = if d.note.is_empty() { 0.0 } else { p.wrap_height(d.note, 12.5, Weight::Regular, w - 2.0 * pad - 96.0, 17.0, 3) };
            (40.0 + nh + 8.0).max(64.0)
        });
        if let Some(hh) = sw_h {
            righe_h += hh + pad;
        }
        let n = self.altre_n();
        if n > 0 {
            righe_h += 56.0 + pad;
        }
        let tot = righe_h + 2.0 * pad;
        p.rrect(x, y, w, tot, 24.0, Some(t.basso), None);
        let (ix, iw) = (x + pad, w - 2.0 * pad);
        let mut cy = y + pad;
        // Il formato, con l'iconcina e le due parole del menu.
        let v = voci::voce(self.casa.categoria, self.casa.operazione());
        let etichetta = if v.hint.is_empty() { "Formato".to_string() } else { format!("Formato · {}", v.hint) };
        self.campo(p, (ix, cy, iw, rh), Some((schiarisci(v.color, scuro), v.glyph.stroke, v.glyph.fill)), &etichetta, v.label, I_GIU, Azione::Casa(CasaAz::Apri(Elenco::Formato)), vista);
        cy += rh + pad;
        if let Some((k, d)) = scelta {
            let tx = self.testo_scelta(d);
            let hint = voci::suggerimento(d.opt, self.casa.valori().get(d));
            let etichetta = if hint.is_empty() { d.title.to_string() } else { format!("{} · {hint}", d.title) };
            let icona = voci::opzione(d.opt).map(|o| (schiarisci(o.color, scuro), o.glyph.stroke, o.glyph.fill));
            self.campo(p, (ix, cy, iw, rh), icona, &etichetta, &tx, I_GIU, Azione::Casa(CasaAz::Apri(Elenco::Opzione(k))), vista);
            cy += rh + pad;
        }
        if let Some((k, d)) = sw {
            let on = self.casa.valori().get(d) != 0;
            cy += self.riga_si_no(p, ix, cy, iw, d.title, d.note, on, 100 + k as u64, Azione::Casa(CasaAz::Interruttore(k)), vista) + pad;
        }
        if n > 0 {
            self.campo(p, (ix, cy, iw, 56.0), None, "", &format!("Altre opzioni ({n})"), "M9 6l6 6-6 6", Azione::Casa(CasaAz::AltreOpzioni), vista);
        }
        tot + 12.0
    }

    /// «Come trascrivo»: modello (tre pillole) e chi parla. Restituisce l'altezza.
    fn trascrivi_m3(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let t = self.toni();
        let pad = 6.0;
        let tot = 2.0 * pad + 104.0 + pad + 68.0;
        p.rrect(x, y, w, tot, 24.0, Some(t.basso), None);
        let (ix, iw) = (x + pad, w - 2.0 * pad);
        p.rrect(ix, y + pad, iw, 104.0, 18.0, Some(t.fondo), None);
        p.text("Modello", ix + 16.0, y + pad + 8.0, iw, 22.0, 12.0, Weight::Regular, t.variante, Align::Left);
        // Gruppo di pulsanti collegati: la scelta è piena, le altre tonali.
        let gy = y + pad + 38.0;
        let gw = (iw - 32.0 - 8.0) / 3.0;
        let scelta = self.modello.min(2);
        let nomi = ["Veloce", "Buona", "Massima"];
        let gx = self.anim.verso(super::anim::Chiave::Interruttore(3), ix + 16.0 + scelta as f32 * (gw + 4.0), 200.0);
        for k in 0..nomi.len() {
            let rc = (ix + 16.0 + k as f32 * (gw + 4.0), gy, gw, 48.0);
            let r = if k == scelta { 24.0 } else { 12.0 };
            p.rrect(rc.0, rc.1, rc.2, rc.3, r, Some(t.alto), None);
            self.colpo(rc, Azione::Casa(CasaAz::Modello(k)), vista);
        }
        p.rrect(gx, gy, gw, 48.0, 24.0, Some(pal.primary), None);
        for (k, nome) in nomi.iter().enumerate() {
            let rc = (ix + 16.0 + k as f32 * (gw + 4.0), gy, gw, 48.0);
            let on = (gx - rc.0).abs() < gw / 2.0;
            p.text(nome, rc.0, rc.1, rc.2, rc.3, 14.5, Weight::Semibold, if on { pal.on_primary } else { t.testo }, Align::Center);
        }
        let testo = match self.voci {
            0 => "Automatico: le conta Cedro".to_string(),
            1 => "Una persona sola".to_string(),
            n => format!("{n} persone"),
        };
        let col = colore_att(Att::Tras, self.scuro());
        self.campo(p, (ix, y + pad + 104.0 + pad, iw, 68.0), Some((col, ic::TRANSCRIBE, "")), "Chi parla", &testo, I_GIU, Azione::Casa(CasaAz::Apri(Elenco::Voci)), vista);
        tot + 8.0
    }

    /// «In Documenti/Cedro» e «Se esiste -2 ▾».
    fn riga_destinazione(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, h: f32, raggio: f32) {
        let a = (w - 8.0) * 0.58;
        self.sel(p, (x, y, a, h), "In", "Documenti/Cedro", raggio, None, 13.0);
        let e = self.testo_esiste();
        self.sel(p, (x + a + 8.0, y, w - a - 8.0, h), "Se esiste", e, raggio, Some(Azione::Casa(CasaAz::Apri(Elenco::Esiste))), 13.0);
    }

    /// Una tendina dentro la zona che scorre.
    fn sel_in(&mut self, p: &mut Painter, rc: R, testo: &str, az: Azione, vista: R) {
        let n0 = self.colpi.len();
        self.sel(p, rc, "", testo, 8.0, Some(az), 15.0);
        let nuovi: Vec<(Forma, Azione)> = self.colpi.drain(n0..).collect();
        for (f, a) in nuovi {
            if let Forma::R(r) = f {
                self.colpo(r, a, vista);
            }
        }
    }

    // ---- Percorso guidato ----

    fn casa_guidato(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let scuro = self.scuro();
        // Testata: simbolo, attività a tendina, aiuto e impostazioni.
        self.logo(p, 16.0, 0.0, 56.0, false, 20.0);
        let att = self.casa.att;
        let tw = p.measure(att.nome(), 15.0, Weight::Regular) + 72.0;
        let b = (54.0, 8.0, tw, 40.0);
        let a = Azione::Casa(CasaAz::Apri(Elenco::Attivita));
        p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        self.kt(p, b.0 + 8.0, b.1 + 7.0, 26.0, colore_att(att, scuro), att.icona());
        p.text(att.nome(), b.0 + 40.0, b.1, tw - 60.0, b.3, 15.0, Weight::Regular, pal.text, Align::Left);
        p.icon(I_GIU, b.0 + b.2 - 22.0, b.1 + 12.0, 16.0, pal.muted);
        self.colpi.push((Forma::R(b), a));
        self.testata_destra(p, w, 0.0, 56.0, true);
        // La firma digitale non ha passi: si sceglie il file e si guarda.
        if att == Att::Firma {
            let vista = (0.0, 64.0, w, h - 64.0 - 86.0);
            p.push_clip(vista.0, vista.1, vista.2, vista.3);
            let y = vista.1 + 8.0 - self.scorri;
            let fine = y + self.corpo_firma(p, 16.0, y, w - 32.0, vista);
            p.pop_clip();
            self.scorri_max = (fine + self.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
            self.scorri = self.scorri.min(self.scorri_max);
            p.fill(0.0, h - 86.0, w, 1.0, pal.border);
            let (t, i) = self.testo_cta();
            self.cta(p, (16.0, h - 72.0, w - 32.0, 54.0), 6.0, Some(i), &t, Some(Azione::Casa(CasaAz::Vai)), 18.0);
            return;
        }
        // I tre passi
        let passo = self.casa.passo;
        let etichette = match att {
            Att::Conv => ["File", "Operazione", "Salvataggio"],
            Att::Tras => ["File", "Modello", "Salvataggio"],
            _ => ["Link", "Cosa scarico", "Salvataggio"],
        };
        let mut sx = 16.0;
        let sy = 76.0;
        let larghi: Vec<f32> = etichette.iter().enumerate().map(|(i, e)| 36.0 + p.measure(e, 14.0, if i as u8 == passo { Weight::Bold } else { Weight::Regular })).collect();
        let linea = ((w - 32.0 - larghi.iter().sum::<f32>()) / 2.0).max(10.0);
        for (i, e) in etichette.iter().enumerate() {
            let i8 = i as u8;
            let (fondo, bordo, fg) = if i8 == passo { (Some(pal.primary), pal.primary, pal.on_primary) } else if i8 < passo { (Some(pal.surface), pal.primary, pal.primary) } else { (None, pal.muted, pal.muted) };
            p.circle(sx + 15.0, sy, 14.0, fondo, Some((bordo, 2.0)));
            p.text(&(i + 1).to_string(), sx, sy - 15.0, 30.0, 30.0, 14.0, Weight::Bold, fg, Align::Center);
            p.text(e, sx + 36.0, sy - 12.0, larghi[i], 24.0, 14.0, if i8 == passo { Weight::Bold } else { Weight::Regular }, if i8 == passo { pal.primary } else { pal.muted }, Align::Left);
            sx += larghi[i] + 4.0;
            if i < 2 {
                p.fill(sx, sy - 1.0, linea - 8.0, 2.0, if i8 < passo { pal.primary } else { pal.border });
                sx += linea - 4.0;
            }
        }
        let piede = 96.0;
        let vista = (0.0, 96.0, w, h - 96.0 - piede);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let x = 16.0;
        let cw = w - 32.0;
        let mut y = 112.0 - self.scorri;
        let (titolo, sotto) = match (att, passo) {
            (Att::Conv | Att::Tras, 0) => (if att == Att::Tras { "Cosa vuoi trascrivere?" } else { "Quali file vuoi convertire?" }, "Scegli i file oppure condividili con Cedro da un'altra app."),
            (Att::Conv, 1) => ("Cosa vuoi ottenere?", "Scegli il formato di destinazione."),
            (Att::Tras, 1) => ("Quanto bene?", "«Veloce» finisce prima, «Massima» sbaglia meno parole."),
            (_, 0) => ("Quale video?", "Incolla il link copiato dal browser o da un'altra app."),
            (_, 1) => ("Cosa vuoi scaricare?", "Scegli che cosa tenere del video."),
            _ => ("Dove lo salvo?", "Controlla e conferma."),
        };
        let th = p.text_wrap(titolo, x, y, cw, 29.0, Weight::Bold, pal.text, 34.0, 3);
        y += th + 8.0;
        y += p.text_wrap(sotto, x, y, cw, 16.0, Weight::Regular, pal.muted, 22.0, 3) + 14.0;
        match (att, passo) {
            (Att::Conv | Att::Tras, 0) => {
                let b = (x, y, cw, 48.0);
                let a = Azione::Casa(CasaAz::Aggiungi);
                p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
                let tw = p.measure("Aggiungi file", 16.0, Weight::Regular);
                p.icon(ic::ADD, b.0 + (b.2 - tw - 30.0) / 2.0, b.1 + 13.0, 22.0, pal.text);
                p.text("Aggiungi file", b.0 + (b.2 - tw - 30.0) / 2.0 + 30.0, b.1, tw + 4.0, b.3, 16.0, Weight::Regular, pal.text, Align::Left);
                self.colpo(b, a, vista);
                y += 60.0;
                let vis = self.casa.visibili();
                if !vis.is_empty() {
                    let rh = 72.0;
                    super::sollevato(p, pal, self.tema.dark(), x, y, cw, rh * vis.len() as f32, 10.0);
                    for (k, i) in vis.iter().copied().enumerate() {
                        let v = self.casa.coda[i].clone();
                        let ry = y + k as f32 * rh;
                        if k > 0 {
                            p.fill(x, ry, cw, 1.0, pal.border);
                        }
                        self.miniatura_voce(p, &v, (x + 14.0, ry + 12.0, 72.0, 48.0), 8.0);
                        p.text(&v.nome, x + 98.0, ry + 12.0, cw - 160.0, 24.0, 16.0, Weight::Bold, pal.text, Align::Left);
                        self.info_voce_riga(p, &v, x + 98.0, ry + 48.0, cw - 160.0, 12.0, false);
                        let t = (x + cw - 56.0, ry + 12.0, 48.0, 48.0);
                        p.icon(ic::CLOSE, t.0 + 12.0, t.1 + 12.0, 24.0, pal.text);
                        self.colpo(t, Azione::Casa(CasaAz::Togli(i)), vista);
                    }
                    y += rh * vis.len() as f32 + 8.0;
                }
                y += self.altri_in_coda(p, x, y, cw, 14.0, false) + 10.0;
                y += self.ultimi_casa(p, x, y, cw, vista);
            }
            (Att::Conv, 1) => {
                // Il riassunto dei file scelti.
                let vis = self.casa.scelti();
                let tot: u64 = vis.iter().map(|i| self.casa.coda[*i].dim).sum();
                super::sollevato(p, pal, self.tema.dark(), x, y, cw, 64.0, 10.0);
                for (k, i) in vis.iter().take(3).enumerate() {
                    let v = self.casa.coda[*i].clone();
                    let mx = x + 12.0 + k as f32 * 20.0;
                    p.rrect(mx - 2.0, y + 13.0, 38.0, 38.0, 9.0, Some(pal.surface), None);
                    self.miniatura_voce(p, &v, (mx, y + 15.0, 34.0, 34.0), 8.0);
                }
                let tx = x + 12.0 + vis.len().min(3) as f32 * 20.0 + 26.0;
                let col = colore_kind(self.casa.categoria, scuro);
                p.text(&plurale(self.casa.categoria, vis.len()), tx, y + 10.0, cw - (tx - x) - 110.0, 24.0, 16.0, Weight::Bold, col, Align::Left);
                let est: Vec<String> = vis.iter().filter_map(|i| std::path::Path::new(&self.casa.coda[*i].nome).extension().map(|e| e.to_string_lossy().to_uppercase())).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
                p.text(&format!("{} · {} in tutto", est.join(", "), super::peso(tot)), tx, y + 34.0, cw - (tx - x) - 110.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                let cf = (x + cw - 108.0, y + 10.0, 100.0, 44.0);
                p.text("Cambia file", cf.0, cf.1, cf.2, cf.3, 15.0, Weight::Bold, pal.primary, Align::Right);
                self.colpo(cf, Azione::Casa(CasaAz::Passo(-1)), vista);
                y += 76.0;
                // I formati consigliati, due per riga.
                let semplici = conversioni::simple_formats(self.casa.categoria);
                let tutte = self.operazioni();
                let op = self.casa.operazione();
                let gw = (cw - 10.0) / 2.0;
                for (k, sf) in semplici.iter().enumerate() {
                    let Some(pos) = tutte.iter().position(|t| t.0 == sf.id) else { continue };
                    let rc = (x + (k % 2) as f32 * (gw + 10.0), y + (k / 2) as f32 * 76.0, gw, 68.0);
                    let on = op == sf.id;
                    p.rrect(rc.0, rc.1, rc.2, rc.3, 10.0, Some(if on { pal.alt } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
                    let kc = colore_kind(kind_uscita(self.casa.categoria, sf.id), scuro);
                    self.kt(p, rc.0 + 10.0, rc.1 + 10.0, 26.0, kc, icona_kind(kind_uscita(self.casa.categoria, sf.id)));
                    p.text(sf.title, rc.0 + 44.0, rc.1 + 8.0, rc.2 - 50.0, 28.0, 17.0, Weight::Bold, pal.text, Align::Left);
                    p.text(&super::pdf::maiuscola_pub(sf.note), rc.0 + 10.0, rc.1 + 40.0, rc.2 - 16.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                    self.colpo(rc, Azione::Casa(CasaAz::Formato(pos)), vista);
                }
                y += semplici.len().div_ceil(2) as f32 * 76.0 + 4.0;
                let et = self.casa.etichetta();
                p.text(&format!("Impostazioni consigliate · Converti in {et}"), x, y, cw, 24.0, 15.0, Weight::Regular, pal.muted, Align::Left);
                y += 32.0;
                let (_, sw) = self.principali();
                let ch = 122.0 + if sw.is_some() { 50.0 } else { 0.0 };
                super::sollevato(p, pal, self.tema.dark(), x, y, cw, ch, 10.0);
                p.text(&format!("Opzioni di conversione · {et}"), x + 12.0, y + 8.0, cw - 24.0, 28.0, 16.0, Weight::Bold, pal.text, Align::Left);
                self.sel_in(p, (x + 12.0, y + 42.0, cw - 24.0, 44.0), "Altri formati e strumenti", Azione::Casa(CasaAz::Apri(Elenco::Formato)), vista);
                let mut iy = y + 94.0;
                if let Some((k, d)) = sw {
                    iy += self.riga_interruttore(p, x + 12.0, iy, cw - 24.0, k, d, 15.0);
                }
                let n = self.altre_n();
                if n > 0 {
                    p.text(&format!("Altre opzioni ({n})"), x + 12.0, iy, cw - 50.0, 28.0, 15.0, Weight::Semibold, pal.primary, Align::Left);
                    self.colpo((x, iy - 8.0, cw, 44.0), Azione::Casa(CasaAz::AltreOpzioni), vista);
                }
                y += ch;
            }
            (Att::Tras, 1) => {
                y += self.scelta_modello(p, x, y, cw, vista);
                y += p.text_wrap("Esce un file di testo con le frasi e i tempi; dai Lavori si apre «Rivedi» per ascoltarle e correggerle.", x, y, cw, 14.0, Weight::Regular, pal.muted, 20.0, 4);
            }
            (_, 0) => {
                let link = self.casa.link.clone();
                self.campo_testo(p, (x, y, cw - 112.0, 52.0), Campo::Link, &link, "https://…", "", false, vista);
                let b = (x + cw - 104.0, y, 104.0, 52.0);
                p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
                p.text("Incolla", b.0, b.1, b.2, b.3, 15.0, Weight::Semibold, pal.text, Align::Center);
                self.colpo(b, Azione::Casa(CasaAz::Incolla), vista);
                y += 60.0;
            }
            (_, 1) => {
                let col = colore_att(att, scuro);
                for (k, (id, nome, sotto)) in WEB.iter().enumerate() {
                    let rc = (x, y, cw, 64.0);
                    let on = self.casa.web == k;
                    p.rrect(rc.0, rc.1, rc.2, rc.3, 10.0, Some(if on { pal.alt } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
                    self.kt(p, rc.0 + 12.0, rc.1 + 16.0, 32.0, if *id == "web-testo" { colore_att(Att::Tras, scuro) } else { col }, super::icona_di(id));
                    p.text(nome, rc.0 + 56.0, rc.1 + 8.0, rc.2 - 70.0, 26.0, 17.0, Weight::Bold, pal.text, Align::Left);
                    p.text(sotto, rc.0 + 56.0, rc.1 + 34.0, rc.2 - 70.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                    self.colpo(rc, Azione::Casa(CasaAz::Web(k)), vista);
                    y += 72.0;
                }
                if self.casa.web == 3 {
                    y += self.scelta_modello(p, x, y, cw, vista);
                }
            }
            _ => {
                // Salvataggio: dove, se esiste già, e il riassunto.
                super::sollevato(p, pal, self.tema.dark(), x, y, cw, 64.0, 10.0);
                p.icon(ic::FOLDER, x + 14.0, y + 20.0, 24.0, pal.text);
                p.text("Documenti/Cedro", x + 50.0, y + 8.0, cw - 60.0, 26.0, 16.0, Weight::Bold, pal.text, Align::Left);
                p.text("Le copie nuove vanno qui; l'originale non si tocca.", x + 50.0, y + 34.0, cw - 60.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                y += 76.0;
                p.text("Se il file esiste già", x, y, cw, 26.0, 14.0, Weight::Semibold, pal.muted, Align::Left);
                let k = match self.esiste {
                    cedro::Exists::Replace => 1,
                    cedro::Exists::Skip => 2,
                    _ => 0,
                };
                self.pillole_in(p, (x, y + 28.0, cw, TOCCO), &["Numera -2, -3", "Sostituisci", "Salta"], k, |i| Azione::Opz(OpzAz::Esiste(i as u8)), vista);
                y += 28.0 + TOCCO + 14.0;
                let riassunto = match att {
                    Att::Conv => format!("{} in {}", plurale(self.casa.categoria, self.casa.scelti().len()), self.casa.etichetta()),
                    Att::Tras => format!("{} in testo, modello {}", plurale(Kind::Audio, self.casa.scelti().len()), ["Veloce", "Buona", "Massima"][self.modello.min(2)]),
                    _ => format!("{}: {}", WEB[self.casa.web.min(3)].1, self.casa.link),
                };
                y += p.text_wrap(&riassunto, x, y, cw, 15.0, Weight::Semibold, pal.text, 21.0, 3);
            }
        }
        p.pop_clip();
        self.scorri_max = (y + self.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        // Il piede
        let fy = h - piede;
        p.fill(0.0, fy, w, 1.0, pal.border);
        if passo == 0 {
            let t = "L'originale resta al sicuro.";
            let tw = p.measure(t, 15.0, Weight::Regular);
            p.icon(I_SCUDO, (w - tw - 28.0) / 2.0, fy + 10.0, 20.0, pal.ok);
            p.text(t, (w - tw - 28.0) / 2.0 + 28.0, fy + 8.0, tw + 4.0, 24.0, 15.0, Weight::Regular, pal.muted, Align::Left);
        }
        let by = fy + if passo == 0 { 36.0 } else { 18.0 };
        let pronto = match (att, passo) {
            (Att::Conv | Att::Tras, 0) => !self.casa.scelti().is_empty(),
            (Att::Yt | Att::Web, 0) => cedro::webvideo::find_url(&self.casa.link).is_some(),
            _ => self.cta_pronta(),
        };
        let (testo, icona, az) = if passo < 2 { ("Continua".to_string(), I_FRECCIA, CasaAz::Passo(1)) } else {
            let (t, i) = self.testo_cta();
            (t, i, CasaAz::Vai)
        };
        if passo > 0 {
            let ib = (16.0, by, (w - 42.0) * 0.38, 54.0);
            let a = Azione::Casa(CasaAz::Passo(-1));
            p.rrect(ib.0, ib.1, ib.2, ib.3, 6.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
            p.text("Indietro", ib.0, ib.1, ib.2, ib.3, 17.0, Weight::Bold, pal.text, Align::Center);
            self.colpi.push((Forma::R(ib), a));
            let cb = (ib.0 + ib.2 + 10.0, by, w - 32.0 - ib.2 - 10.0, 54.0);
            self.cta_destra(p, cb, &testo, icona, pronto.then_some(Azione::Casa(az)), passo < 2);
        } else {
            self.cta_destra(p, (16.0, by, w - 32.0, 54.0), &testo, icona, pronto.then_some(Azione::Casa(az)), true);
        }
    }

    /// Il pulsante grande con la freccia dopo il testo («Continua →»).
    fn cta_destra(&mut self, p: &mut Painter, rc: R, testo: &str, icona: &str, az: Option<Azione>, dopo: bool) {
        if !dopo {
            return self.cta(p, rc, 6.0, Some(icona), testo, az, 17.0);
        }
        let pal = self.pal();
        let fg = if az.is_some() { pal.on_primary } else { pal.muted };
        if az.is_none() {
            p.rrect(rc.0, rc.1, rc.2, rc.3, 6.0, Some(pal.alt), None);
        } else {
            super::lucido(p, self.scuro(), rc.0, rc.1, rc.2, rc.3, 6.0, pal.primary, self.premuto == az);
        }
        let tw = p.measure(testo, 18.0, Weight::Bold);
        let x0 = rc.0 + (rc.2 - tw - 30.0) / 2.0;
        p.text(testo, x0, rc.1, tw + 4.0, rc.3, 18.0, Weight::Bold, fg, Align::Left);
        p.icon(icona, x0 + tw + 8.0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, fg);
        if let Some(a) = az {
            self.colpi.push((Forma::R(rc), a));
        }
    }

    // ---- Compatto ----

    fn casa_compatto(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let scuro = self.scuro();
        self.logo(p, 16.0, 0.0, 58.0, true, 22.0);
        self.testata_destra(p, w, 0.0, 58.0, false);
        let x = 16.0;
        let cw = w - 32.0;
        // Le attività, a segmenti.
        let att = attivita();
        let sy = 62.0;
        super::sollevato(p, pal, self.tema.dark(), x, sy, cw, 48.0, 8.0);
        let sw = (cw - 8.0) / att.len() as f32;
        // Con quattro attività le scritte si rimpiccioliscono finché stanno nel loro segmento.
        let dim = [14.0, 13.0, 12.0, 11.0].into_iter().find(|d| att.iter().all(|a| p.measure(if *a == Att::Web { "Web" } else { a.nome() }, *d, Weight::Semibold) + 18.0 <= sw)).unwrap_or(11.0);
        for (k, a) in att.iter().enumerate() {
            let on = *a == self.casa.att;
            let rc = (x + 4.0 + k as f32 * sw, sy + 4.0, sw, 40.0);
            let nome = match a {
                Att::Web => "Web",
                _ => a.nome(),
            };
            let col = colore_att(*a, scuro);
            if on {
                super::lucido(p, scuro, rc.0, rc.1, rc.2, rc.3, 6.0, col, false);
                p.text(nome, rc.0, rc.1, rc.2, rc.3, dim, Weight::Semibold, (255, 255, 255), Align::Center);
            } else {
                let tw = p.measure(nome, dim, Weight::Regular);
                let tx = rc.0 + (rc.2 - tw - 12.0) / 2.0;
                p.circle(tx + 3.0, rc.1 + rc.3 / 2.0, 3.5, Some(col), None);
                p.text(nome, tx + 12.0, rc.1, tw + 4.0, rc.3, dim, Weight::Regular, pal.text, Align::Left);
            }
            self.colpi.push((Forma::R(rc), Azione::Casa(CasaAz::Att(k))));
        }
        let piede = 120.0;
        let vista = (0.0, sy + 56.0, w, h - sy - 56.0 - piede);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut y = vista.1 - self.scorri;
        if matches!(self.casa.att, Att::Yt | Att::Web | Att::Firma) {
            y += self.corpo_web(p, x, y, cw, vista);
        } else {
            let vis = self.casa.visibili();
            for i in vis.iter().copied() {
                let v = self.casa.coda[i].clone();
                p.rrect(x, y, cw, 66.0, 10.0, Some(pal.surface), Some((if v.scelto { pal.border } else { mix(pal.border, pal.bg, 0.5) }, 1.0)));
                self.miniatura_voce(p, &v, (x + 9.0, y + 9.0, 48.0, 48.0), 8.0);
                p.text(&v.nome, x + 68.0, y + 8.0, cw - 124.0, 26.0, 17.0, Weight::Semibold, if v.scelto { pal.text } else { pal.muted }, Align::Left);
                self.info_voce_riga(p, &v, x + 68.0, y + 46.0, cw - 124.0, 13.0, false);
                let t = (x + cw - 52.0, y + 9.0, 48.0, 48.0);
                p.icon(ic::CLOSE, t.0 + 12.0, t.1 + 12.0, 22.0, pal.text);
                self.colpo(t, Azione::Casa(CasaAz::Togli(i)), vista);
                self.colpo((x, y, cw - 56.0, 66.0), Azione::Casa(CasaAz::Spunta(i)), vista);
                y += 74.0;
            }
            let a = (x, y, 160.0, 44.0);
            p.icon(ic::ADD, a.0, a.1 + 11.0, 22.0, pal.text);
            p.text("Aggiungi file", a.0 + 32.0, a.1, 140.0, a.3, 16.0, Weight::Regular, pal.text, Align::Left);
            self.colpo(a, Azione::Casa(CasaAz::Aggiungi), vista);
            self.altri_in_coda(p, x + 150.0, y + 11.0, cw - 150.0, 13.0, true);
            y += 52.0;
            y += self.ultimi_casa(p, x, y, cw, vista);
            if self.casa.att == Att::Tras {
                y += self.ispettore_trascrizione(p, x, y, cw, vista);
            } else {
                p.text("Converti in", x, y, cw, 28.0, 17.0, Weight::Bold, pal.text, Align::Left);
                y += 34.0;
                let col = colore_kind(self.casa.categoria, scuro);
                let half = (cw - 36.0) / 2.0;
                p.rrect(x, y, half, 44.0, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
                self.kt(p, x + 10.0, y + 9.0, 26.0, col, icona_kind(self.casa.categoria));
                p.text(&plurale(self.casa.categoria, self.casa.scelti().len()), x + 44.0, y, half - 50.0, 44.0, 15.0, Weight::Regular, col, Align::Left);
                p.icon(I_FRECCIA, x + half + 6.0, y + 10.0, 24.0, pal.text);
                let et = self.casa.etichetta();
                self.sel_in(p, (x + half + 36.0, y, half, 44.0), et, Azione::Casa(CasaAz::Apri(Elenco::Formato)), vista);
                y += 52.0;
                let n = self.casa.defs().len();
                let b = (x, y, cw, 50.0);
                let a = Azione::Casa(CasaAz::AltreOpzioni);
                p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
                p.icon(ic::SETTINGS, b.0 + 12.0, b.1 + 13.0, 24.0, pal.text);
                p.text(&format!("Altre opzioni ({n})"), b.0 + 44.0, b.1, b.2 - 70.0, b.3, 15.0, Weight::Regular, pal.text, Align::Left);
                p.icon(I_GIU, b.0 + b.2 - 28.0, b.1 + 17.0, 16.0, pal.muted);
                self.colpo(b, a, vista);
                y += 62.0;
                let half = (cw - 10.0) / 2.0;
                p.text("Cartella di destinazione", x, y, half, 22.0, 13.0, Weight::Semibold, pal.text, Align::Left);
                p.text("In caso di conflitto", x + half + 10.0, y, half, 22.0, 13.0, Weight::Semibold, pal.text, Align::Left);
                y += 26.0;
                let n0 = self.colpi.len();
                self.sel(p, (x, y, half, 44.0), "", "Documenti/Cedro", 8.0, None, 15.0);
                let e = match self.esiste {
                    cedro::Exists::Replace => "Sostituisci",
                    cedro::Exists::Skip => "Salta",
                    _ => "Aggiungi -2",
                };
                self.sel(p, (x + half + 10.0, y, half, 44.0), "", e, 8.0, Some(Azione::Casa(CasaAz::Apri(Elenco::Esiste))), 15.0);
                let nuovi: Vec<(Forma, Azione)> = self.colpi.drain(n0..).collect();
                for (f, a) in nuovi {
                    if let Forma::R(r) = f {
                        self.colpo(r, a, vista);
                    }
                }
                y += 52.0;
            }
        }
        p.pop_clip();
        self.scorri_max = (y + self.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        let fy = h - piede;
        let (t, i) = self.testo_cta();
        let pronto = self.cta_pronta();
        self.cta(p, (x, fy + 12.0, cw, 56.0), 10.0, Some(i), &t, pronto.then_some(Azione::Casa(CasaAz::Vai)), 18.0);
        p.fill(x, fy + 80.0, cw, 1.0, pal.border);
        let a = (x, fy + 82.0, 120.0, 38.0);
        p.icon(ic::HELP, a.0, a.1 + 9.0, 20.0, pal.primary);
        p.text("Aiuto", a.0 + 28.0, a.1, 90.0, a.3, 15.0, Weight::Regular, pal.primary, Align::Left);
        self.colpi.push((Forma::R(a), Azione::Casa(CasaAz::Guida)));
    }

    // ---- Banco di lavoro ----

    fn casa_banco(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let scuro = self.scuro();
        self.logo(p, 16.0, 0.0, 54.0, true, 22.0);
        self.testata_destra(p, w, 0.0, 54.0, false);
        // Le attività: scritte con il pallino, sottolineata quella attiva.
        let att = attivita();
        let ay = 54.0;
        p.fill(0.0, ay + 39.0, w, 1.0, pal.border);
        let mut ax = 16.0;
        // Le scritte intere se ci stanno; altrimenti corte, poi più piccole e più vicine.
        let (corti, dim, passo) = [(false, 15.0, 34.0), (true, 15.0, 34.0), (true, 14.0, 28.0), (true, 13.0, 24.0), (true, 12.0, 20.0)]
            .into_iter()
            .find(|(c, d, g)| att.iter().map(|a| p.measure(if *c { a.corto() } else { a.nome() }, *d, Weight::Semibold) + g).sum::<f32>() <= w - 16.0)
            .unwrap_or((true, 12.0, 18.0));
        for (k, a) in att.iter().enumerate() {
            let on = *a == self.casa.att;
            let nome = if corti { a.corto() } else { a.nome() };
            let tw = p.measure(nome, dim, if on { Weight::Semibold } else { Weight::Regular });
            let col = colore_att(*a, scuro);
            p.circle(ax + 4.0, ay + 20.0, 4.0, Some(col), None);
            p.text(nome, ax + 14.0, ay, tw + 4.0, 40.0, dim, if on { Weight::Semibold } else { Weight::Regular }, if on { pal.text } else { pal.muted }, Align::Left);
            if on {
                p.fill(ax, ay + 37.0, tw + 14.0, 2.0, col);
            }
            self.colpi.push((Forma::R((ax - 4.0, ay, tw + passo - 12.0, 40.0)), Azione::Casa(CasaAz::Att(k))));
            ax += tw + passo;
        }
        let conv = self.casa.att == Att::Conv;
        let foglio_h = if conv { 236.0 } else if self.casa.att == Att::Tras { 228.0 } else { 0.0 };
        let piede = 78.0;
        let x = 12.0;
        let cw = w - 24.0;
        let vista = (0.0, ay + 40.0, w, h - ay - 40.0 - piede - foglio_h);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut y = vista.1 + 10.0 - self.scorri;
        if matches!(self.casa.att, Att::Yt | Att::Web | Att::Firma) {
            y += self.corpo_web(p, x, y, cw, vista);
        } else {
            if conv {
                // Le categorie, a gettoni che scorrono di lato col dito.
                let passo = 6.0;
                let larghi = self.larghi_categorie(p, 14.0, 44.0, 12.0, false);
                let tot = larghi.iter().sum::<f32>() + passo * (larghi.len().saturating_sub(1)) as f32;
                let voluto = self.pos_scelta(&larghi, passo);
                let fascia = super::intersezione((0.0, y - 4.0, w, 46.0), vista).unwrap_or((0.0, y - 4.0, w, 0.0));
                let s = self.fascia(0, fascia, cw, tot, voluto);
                let mut cx = x - s;
                for (i, c) in Casa::categorie().iter().enumerate() {
                    let on = c.kind == self.casa.categoria;
                    let col = colore_kind(c.kind, scuro);
                    let tw = p.measure(c.name, 14.0, Weight::Regular);
                    let n = self.casa.quanti(c.kind);
                    let ww = larghi[i];
                    p.rrect(cx, y, ww, 38.0, 8.0, Some(if on { mix(pal.surface, col, 0.14) } else { pal.surface }), Some((if on { col } else { pal.border }, 1.0)));
                    self.kt(p, cx + 6.0, y + 6.0, 26.0, col, icona_kind(c.kind));
                    p.text(c.name, cx + 38.0, y, tw + 4.0, 38.0, 14.0, Weight::Regular, pal.text, Align::Left);
                    if n > 0 && !on {
                        p.circle(cx + ww - 10.0, y + 19.0, 3.5, Some(pal.primary), None);
                    }
                    self.colpo((cx, y - 4.0, ww, 46.0), Azione::Casa(CasaAz::Categoria(i)), vista);
                    cx += ww + passo;
                }
                self.sfumature(p, (0.0, y - 4.0, w, 46.0), s, self.casa.fx_max[0], pal.bg);
                y += 48.0;
            }
            // Aggiungi file, cartella, quanti in coda.
            let mut bx = x;
            for (t, a) in [("+ Aggiungi file", CasaAz::Aggiungi), ("+ Cartella", CasaAz::Cartella)] {
                let tw = p.measure(t, 14.0, Weight::Regular) + 20.0;
                let az = Azione::Casa(a);
                p.rrect(bx, y, tw, 38.0, 4.0, Some(if self.premuto == Some(az) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
                p.text(t, bx, y, tw, 38.0, 14.0, Weight::Regular, pal.text, Align::Center);
                self.colpo((bx, y - 4.0, tw, 46.0), az, vista);
                bx += tw + 6.0;
            }
            let vis = self.casa.visibili();
            p.text(&format!("{} file in coda", vis.len()), x, y, cw, 38.0, 13.0, Weight::Regular, pal.muted, Align::Right);
            y += 48.0;
            // La tabella: spunta, file, operazione, stato.
            let rh = 66.0;
            let hh = 38.0;
            let col_op = x + cw * 0.56;
            let col_st = x + cw * 0.74;
            super::sollevato(p, pal, self.tema.dark(), x, y, cw, hh + rh * vis.len().max(1) as f32, 6.0);
            p.fill(x + 1.0, y + 1.0, cw - 2.0, hh - 1.0, mix(pal.surface, pal.alt, 0.6));
            let tutti = !vis.is_empty() && vis.iter().all(|i| self.casa.coda[*i].scelto);
            p.rrect(x + 10.0, y + 8.0, 22.0, 22.0, 5.0, Some(if tutti { pal.primary } else { pal.surface }), if tutti { None } else { Some((pal.muted, 1.5)) });
            if tutti {
                p.icon(I_SPUNTA, x + 13.0, y + 11.0, 16.0, pal.on_primary);
            }
            p.text("File ▴", x + 44.0, y, 100.0, hh, 13.0, Weight::Regular, pal.muted, Align::Left);
            p.text("Operaz.", col_op, y, 80.0, hh, 13.0, Weight::Regular, pal.muted, Align::Left);
            p.text("Stato", col_st, y, 80.0, hh, 13.0, Weight::Regular, pal.muted, Align::Left);
            let ty = y + hh;
            if vis.is_empty() {
                p.text("Aggiungi dei file o condividili con Cedro.", x + 12.0, ty, cw - 24.0, rh, 13.0, Weight::Regular, pal.muted, Align::Left);
            }
            let et = if self.casa.att == Att::Tras { "Testo" } else { self.casa.etichetta() };
            let col_et = if self.casa.att == Att::Tras { colore_att(Att::Tras, scuro) } else { colore_kind(kind_uscita(self.casa.categoria, self.casa.operazione()), scuro) };
            for (k, i) in vis.iter().copied().enumerate() {
                let v = self.casa.coda[i].clone();
                let ry = ty + k as f32 * rh;
                p.fill(x, ry, cw, 1.0, pal.border);
                if v.scelto {
                    p.fill(x + 1.0, ry + 1.0, cw - 2.0, rh - 1.0, mix(pal.surface, pal.primary, 0.06));
                }
                self.spunta(p, x + 10.0, ry + rh / 2.0, v.scelto, Azione::Casa(CasaAz::Spunta(i)));
                self.miniatura_voce(p, &v, (x + 42.0, ry + 15.0, 36.0, 36.0), 6.0);
                p.text(&v.nome, x + 84.0, ry + 8.0, col_op - x - 90.0, 22.0, 14.0, Weight::Regular, pal.text, Align::Left);
                self.info_voce_riga(p, &v, x + 84.0, ry + 42.0, col_op - x - 90.0, 11.0, true);
                p.text(et, col_op, ry, col_st - col_op - 4.0, rh, 14.0, Weight::Semibold, col_et, Align::Left);
                let (st, sc) = self.stato_voce(&v);
                p.text(&st, col_st, ry, x + cw - col_st - 6.0, rh, 13.0, Weight::Regular, sc, Align::Left);
                self.colpo((x + 40.0, ry, cw - 40.0, rh), Azione::Casa(CasaAz::Spunta(i)), vista);
            }
            y = ty + rh * vis.len().max(1) as f32 + 8.0;
            y += self.altri_in_coda(p, x, y, cw, 13.0, false) + 8.0;
            y += self.ultimi_casa(p, x, y, cw, vista);
        }
        p.pop_clip();
        self.scorri_max = (y + self.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        // Il foglio dal basso con l'ispettore.
        let fy = h - piede - foglio_h;
        if foglio_h > 0.0 {
            p.shadow(0.0, fy, w, foglio_h + 20.0, 20.0, 12.0, 0.25, -4.0);
            p.rrect(0.0, fy, w, foglio_h + 30.0, 20.0, Some(pal.surface), None);
            p.rrect(w / 2.0 - 20.0, fy + 8.0, 40.0, 5.0, 3.0, Some(pal.border), None);
            let sx = 16.0;
            let sw = w - 32.0;
            if conv {
                let col = colore_kind(self.casa.categoria, scuro);
                self.kt(p, sx, fy + 22.0, 34.0, col, icona_kind(self.casa.categoria));
                let nome = Casa::categorie().iter().find(|c| c.kind == self.casa.categoria).map(|c| c.name.to_lowercase()).unwrap_or_default();
                p.text(&format!("Impostazioni {nome}"), sx + 44.0, fy + 22.0, sw - 44.0, 34.0, 18.0, Weight::Bold, pal.text, Align::Left);
                let half = (sw - 8.0) / 2.0;
                let (scelta, _) = self.principali();
                // Il formato (gli strumenti sono qui sotto, a gettoni).
                let formati = conversioni::formats(self.casa.categoria);
                let op = self.casa.operazione();
                let et_f = formati.iter().find(|s| s.id == op).map(|s| s.label).unwrap_or("Strumento");
                self.sel(p, (sx, fy + 66.0, half, 40.0), "", et_f, 6.0, Some(Azione::Casa(CasaAz::Apri(Elenco::Formato))), 15.0);
                match scelta {
                    Some((k, d)) => {
                        let t = self.testo_scelta(d);
                        self.sel(p, (sx + half + 8.0, fy + 66.0, half, 40.0), "", &t, 6.0, Some(Azione::Casa(CasaAz::Apri(Elenco::Opzione(k)))), 15.0);
                    }
                    None => {
                        let n = self.casa.defs().len();
                        self.sel(p, (sx + half + 8.0, fy + 66.0, half, 40.0), "", &format!("Opzioni ({n})"), 6.0, Some(Azione::Casa(CasaAz::AltreOpzioni)), 15.0);
                    }
                }
                let strumenti = conversioni::tools(self.casa.categoria);
                if !strumenti.is_empty() {
                    let tw = p.measure("Strumenti", 14.0, Weight::Bold);
                    p.text("Strumenti", sx, fy + 114.0, tw + 4.0, 24.0, 14.0, Weight::Bold, pal.text, Align::Left);
                    p.text("(al posto della conversione)", sx + tw + 6.0, fy + 114.0, sw - tw, 24.0, 14.0, Weight::Regular, pal.muted, Align::Left);
                    let tutte = self.operazioni();
                    let mut tx = sx;
                    let mut ty = fy + 142.0;
                    for s in strumenti {
                        let Some(pos) = tutte.iter().position(|t| t.0 == s.id) else { continue };
                        let ww = p.measure(s.label, 13.0, Weight::Regular) + 22.0;
                        if tx + ww > sx + sw {
                            tx = sx;
                            ty += 40.0;
                        }
                        let on = op == s.id;
                        p.rrect(tx, ty, ww, 34.0, 4.0, Some(if on { mix(pal.surface, pal.primary, 0.16) } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, 1.0)));
                        p.text(s.label, tx, ty, ww, 34.0, 13.0, Weight::Regular, if on { pal.primary } else { pal.text }, Align::Center);
                        self.colpi.push((Forma::R((tx, ty - 4.0, ww, 42.0)), Azione::Casa(CasaAz::Formato(pos))));
                        tx += ww + 6.0;
                    }
                }
            } else {
                let vista_f = (0.0, fy, w, foglio_h);
                self.kt(p, sx, fy + 22.0, 34.0, colore_att(Att::Tras, scuro), ic::TRANSCRIBE);
                p.text("Impostazioni trascrizione", sx + 44.0, fy + 22.0, sw - 44.0, 34.0, 18.0, Weight::Bold, pal.text, Align::Left);
                self.scelta_modello(p, sx, fy + 64.0, sw, vista_f);
            }
        }
        // Il piede: avanzamento complessivo e pulsante.
        let py = h - piede;
        p.fill(0.0, py, w, piede, pal.bg);
        p.fill(0.0, py, w, 1.0, pal.border);
        let (t, i) = self.testo_cta();
        if self.casa.att == Att::Firma {
            // La firma non ha file in coda: solo il pulsante, largo.
            self.cta(p, (16.0, py + 13.0, w - 32.0, 52.0), 6.0, Some(i), &t, Some(Azione::Casa(CasaAz::Vai)), 17.0);
            return;
        }
        let tw = p.measure(&t, 17.0, Weight::Bold) + 64.0;
        let pronto = self.cta_pronta();
        self.cta(p, (w - 16.0 - tw, py + 13.0, tw, 52.0), 6.0, None, &t, pronto.then_some(Azione::Casa(CasaAz::Vai)), 17.0);
        let _ = i;
        let (testo, frazione) = self.avanzamento();
        p.text("Progresso complessivo", 16.0, py + 12.0, w - tw - 40.0, 18.0, 12.0, Weight::Regular, pal.muted, Align::Left);
        p.text(&testo, 16.0, py + 30.0, w - tw - 40.0, 22.0, 15.0, Weight::Regular, pal.text, Align::Left);
        let bw = w - tw - 48.0;
        p.rrect(16.0, py + 58.0, bw, 5.0, 3.0, Some(pal.alt), None);
        if frazione > 0.0 {
            p.rrect(16.0, py + 58.0, bw * frazione, 5.0, 3.0, Some(pal.primary), None);
        }
    }

    /// Lo stato di un file in coda: in attesa, a che punto, fatto o non riuscito.
    fn stato_voce(&self, v: &Voce) -> (String, Rgb) {
        let pal = self.pal();
        let Some(l) = v.lavoro.and_then(|id| self.lavori.iter().find(|l| l.id == id)) else { return ("◷ In attesa".into(), pal.muted) };
        match &l.stato {
            Stato::InCorso => {
                let m = l.prog.load(std::sync::atomic::Ordering::Relaxed);
                (if m <= 1000 { format!("{}%", m / 10) } else { "In corso".into() }, pal.primary)
            }
            Stato::Fatto { .. } => ("✓ Fatto".into(), pal.ok),
            Stato::Errore(_) => ("Non riuscito".into(), pal.bad),
        }
    }

    /// «3 file pronti», «1 di 3 fatti» e la frazione per la barra.
    fn avanzamento(&self) -> (String, f32) {
        let vis = self.casa.visibili();
        let con: Vec<&Lavoro> = vis.iter().filter_map(|i| self.casa.coda[*i].lavoro).filter_map(|id| self.lavori.iter().find(|l| l.id == id)).collect();
        if con.is_empty() {
            let n = self.casa.scelti().len();
            return (if n == 1 { "1 file pronto".into() } else { format!("{n} file pronti") }, 0.0);
        }
        let fatti = con.iter().filter(|l| l.stato != Stato::InCorso).count();
        let somma: f32 = con
            .iter()
            .map(|l| match l.stato {
                Stato::InCorso => (l.prog.load(std::sync::atomic::Ordering::Relaxed).min(1000) as f32 / 1000.0).max(0.0),
                _ => 1.0,
            })
            .sum();
        (format!("{fatti} di {} fatti", con.len()), somma / con.len() as f32)
    }

    // ---- Documento al centro ----

    fn casa_documento(&mut self, p: &mut Painter, w: f32, h: f32) {
        self.casa.tutti = true;
        let pal = self.pal();
        let scuro = self.scuro();
        p.fill(-self.margini.0, -self.margini.1, w + self.margini.0 + self.margini.2, 54.0 + self.margini.1, pal.surface);
        self.logo(p, 16.0, 0.0, 54.0, true, 22.0);
        self.testata_destra(p, w, 0.0, 54.0, false);
        // Le attività: schede con l'icona.
        let att = attivita();
        let ay = 54.0;
        p.fill(0.0, ay, w, 46.0, pal.surface);
        p.fill(0.0, ay + 45.0, w, 1.0, pal.border);
        let mut ax = 8.0;
        // Schede intere se ci stanno; altrimenti nomi corti; altrimenti solo l'icona per quelle non scelte.
        let largo = |p: &Painter, a: &Att, corti: bool, sola_icona: bool, on: bool| -> f32 {
            if sola_icona && !on {
                40.0
            } else {
                p.measure(if corti { a.corto() } else { a.nome() }, 14.0, Weight::Semibold) + 44.0
            }
        };
        let (corti, sola_icona) = [(false, false), (true, false), (false, true), (true, true)]
            .into_iter()
            .find(|(c, i)| att.iter().map(|a| largo(p, a, *c, *i, *a == self.casa.att) + 4.0).sum::<f32>() <= w - 12.0)
            .unwrap_or((true, true));
        for (k, a) in att.iter().enumerate() {
            let on = *a == self.casa.att;
            let col = colore_att(*a, scuro);
            let nome = if corti { a.corto() } else { a.nome() };
            let ww = largo(p, a, corti, sola_icona, on);
            if on {
                p.fill(ax, ay, ww, 45.0, mix(pal.surface, pal.primary, 0.08));
                p.fill(ax, ay + 42.0, ww, 3.0, pal.primary);
                p.icon(a.icona(), ax + 8.0, ay + 12.0, 20.0, pal.primary);
            } else {
                self.kt(p, ax + 6.0, ay + 10.0, 24.0, col, a.icona());
            }
            if on || !sola_icona {
                p.text(nome, ax + 36.0, ay, ww - 38.0, 45.0, 14.0, if on { Weight::Semibold } else { Weight::Regular }, if on { pal.primary } else { pal.text }, Align::Left);
            }
            self.colpi.push((Forma::R((ax, ay, ww.max(TOCCO), 46.0)), Azione::Casa(CasaAz::Att(k))));
            ax += ww + 4.0;
        }
        let conv_o_tras = matches!(self.casa.att, Att::Conv | Att::Tras);
        let mut y = ay + 46.0;
        if !conv_o_tras {
            let vista = (0.0, y, w, h - y - 80.0);
            p.push_clip(vista.0, vista.1, vista.2, vista.3);
            let fine = y + 14.0 - self.scorri + self.corpo_web(p, 16.0, y + 14.0 - self.scorri, w - 32.0, vista);
            p.pop_clip();
            self.scorri_max = (fine + self.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
            self.scorri = self.scorri.min(self.scorri_max);
            self.tieni_campo(vista);
            let (t, i) = self.testo_cta();
            let pronto = self.cta_pronta();
            self.cta(p, (16.0, h - 68.0, w - 32.0, 54.0), 8.0, Some(i), &t, pronto.then_some(Azione::Casa(CasaAz::Vai)), 18.0);
            return;
        }
        // La striscia dei file: + e le schede dei file.
        let vis = self.casa.visibili();
        p.fill(0.0, y, w, 72.0, pal.surface);
        p.fill(0.0, y + 71.0, w, 1.0, pal.border);
        let add = (12.0, y + 12.0, 48.0, 48.0);
        p.rrect(add.0, add.1, add.2, add.3, 10.0, Some(if self.premuto == Some(Azione::Casa(CasaAz::Aggiungi)) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        p.icon(ic::ADD, add.0 + 12.0, add.1 + 12.0, 24.0, pal.text);
        self.colpi.push((Forma::R(add), Azione::Casa(CasaAz::Aggiungi)));
        let mostrato = vis.get(self.casa.mostrato).copied().or_else(|| vis.first().copied());
        // La striscia scorre di lato col dito quando i file non ci stanno.
        let fascia = (66.0, y, w - 66.0, 72.0);
        let tot = vis.len() as f32 * 194.0 - 8.0;
        let sf = self.fascia(1, fascia, w - 66.0 - 12.0, tot + 4.0, None);
        let mut fx = 70.0 - sf;
        p.push_clip(fascia.0, fascia.1, fascia.2, fascia.3);
        for (k, i) in vis.iter().copied().enumerate() {
            let v = self.casa.coda[i].clone();
            let on = Some(i) == mostrato;
            let rc = (fx, y + 10.0, 186.0, 52.0);
            p.rrect(rc.0, rc.1, rc.2, rc.3, 10.0, Some(pal.surface), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
            self.miniatura_voce(p, &v, (rc.0 + 8.0, rc.1 + 8.0, 36.0, 36.0), 6.0);
            p.text(&v.nome, rc.0 + 52.0, rc.1 + 4.0, rc.2 - 58.0, 22.0, 14.0, Weight::Semibold, if v.scelto { pal.text } else { pal.muted }, Align::Left);
            self.info_voce_riga(p, &v, rc.0 + 52.0, rc.1 + 36.0, rc.2 - 58.0, 11.0, true);
            self.colpo(rc, Azione::Casa(CasaAz::Mostra(k)), fascia);
            fx += rc.2 + 8.0;
        }
        p.pop_clip();
        self.sfumature(p, fascia, sf, self.casa.fx_max[1], pal.surface);
        y += 72.0;
        // Il pannello in basso.
        let tras = self.casa.att == Att::Tras;
        let (_, sw) = self.principali();
        let pannello = if tras { 196.0 } else { 204.0 + if sw.is_some() { 40.0 } else { 0.0 } };
        let py = h - pannello;
        // Il foglio al centro: il file in grande.
        let work = mix(pal.bg, pal.alt, 0.5);
        p.fill(0.0, y, w, py - y, work);
        match mostrato {
            Some(i) => {
                let v = self.casa.coda[i].clone();
                p.text(&v.nome, 14.0, y + 8.0, w - 110.0, 26.0, 15.0, Weight::Semibold, pal.text, Align::Left);
                let pagine = if v.kind == Kind::Pdf { self.pdf_pagine.get(&v.path).copied().unwrap_or(1) } else { 1 };
                p.text(&format!("1 / {pagine}"), w - 90.0, y + 8.0, 76.0, 26.0, 14.0, Weight::Regular, pal.muted, Align::Right);
                let area = (14.0, y + 42.0, w - 28.0, py - y - 52.0);
                if area.3 > 40.0 {
                    match &v.thumb {
                        Some(img) => {
                            let k = (area.2 / img.w as f32).min(area.3 / img.h as f32);
                            let (iw, ih) = (img.w as f32 * k, img.h as f32 * k);
                            let (ix, iy) = (area.0 + (area.2 - iw) / 2.0, area.1 + (area.3 - ih) / 2.0);
                            p.shadow(ix, iy, iw, ih, 2.0, 10.0, 0.18, 6.0);
                            p.image(img, ix, iy, iw, ih, 2.0, Fit::Contain);
                        }
                        None => {
                            let col = colore_kind(v.kind, scuro);
                            p.icon(icona_kind(v.kind), area.0 + area.2 / 2.0 - 32.0, area.1 + area.3 / 2.0 - 32.0, 64.0, col);
                        }
                    }
                }
            }
            None => {
                let t = if tras { "Aggiungi un audio o un video con +, oppure condividilo con Cedro." } else { "Aggiungi un file con +, oppure condividilo con Cedro da un'altra app." };
                let usata = p.text_wrap(t, 32.0, y + 40.0, w - 64.0, 15.0, Weight::Regular, pal.muted, 22.0, 3);
                let vista = (0.0, y, w, py - y);
                self.ultimi_casa(p, 32.0, y + 60.0 + usata, w - 64.0, vista);
            }
        }
        p.fill(0.0, py, w, pannello, pal.surface);
        p.fill(0.0, py, w, 1.0, pal.border);
        let x = 16.0;
        let cw = w - 32.0;
        let mut cy = py + 12.0;
        let vista = (0.0, py, w, pannello);
        if tras {
            cy += self.scelta_modello(p, x, cy, cw, vista);
        } else {
            // «JPG → PNG» e la tendina del formato.
            let da = mostrato.map(|i| std::path::Path::new(&self.casa.coda[i].nome).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()).unwrap_or_else(|| Casa::categorie().iter().find(|c| c.kind == self.casa.categoria).map(|c| c.name.to_string()).unwrap_or_default());
            let a_kind = kind_uscita(self.casa.categoria, self.casa.operazione());
            let et = self.casa.etichetta();
            let mut fx = x;
            for (n, (t, k)) in [(da.as_str(), self.casa.categoria), (et, a_kind)].into_iter().enumerate() {
                self.kt(p, fx, cy + 4.0, 32.0, colore_kind(k, scuro), icona_kind(k));
                let tw = p.measure(t, 17.0, Weight::Semibold).min(90.0);
                p.text(t, fx + 38.0, cy, tw + 4.0, 40.0, 17.0, Weight::Semibold, pal.text, Align::Left);
                fx += 38.0 + tw + 10.0;
                if n == 0 {
                    p.icon(I_FRECCIA, fx, cy + 8.0, 24.0, pal.text);
                    fx += 34.0;
                }
            }
            let sw_w = 110.0f32.min(w - fx - 16.0).max(80.0);
            self.sel(p, (x + cw - sw_w, cy, sw_w, 40.0), "", et, 8.0, Some(Azione::Casa(CasaAz::Apri(Elenco::Formato))), 15.0);
            cy += 50.0;
            if let Some((k, d)) = sw {
                cy += self.riga_interruttore(p, x, cy, cw, k, d, 15.0) - 8.0;
            }
            // Qui c'è solo l'interruttore: le altre sono tutte in «Altre opzioni».
            let n = self.casa.defs().len() - sw.is_some() as usize;
            p.text(&format!("Altre opzioni ({n})"), x, cy, cw - 40.0, 36.0, 15.0, Weight::Semibold, pal.text, Align::Left);
            p.icon(I_GIU, x + cw - 20.0, cy + 10.0, 16.0, pal.muted);
            self.colpi.push((Forma::R((x, cy, cw, 36.0)), Azione::Casa(CasaAz::AltreOpzioni)));
            cy += 40.0;
        }
        self.riga_destinazione(p, x, cy, cw, 38.0, 8.0);
        cy += 46.0;
        let (t, i) = self.testo_cta();
        let pronto = self.cta_pronta();
        self.cta(p, (x, cy, cw, 54.0), 8.0, Some(i), &t, pronto.then_some(Azione::Casa(CasaAz::Vai)), 18.0);
    }

    // ---- L'elenco a comparsa ----

    fn foglio_elenco(&mut self, p: &mut Painter, w: f32, h: f32) {
        let Some(e) = self.casa.elenco else { return };
        let (ml, mt, mr, mb) = self.margini;
        let velo = self.anim.comparsa(super::anim::Chiave::Foglio, 220.0);
        p.rrect_a(-ml, -mt, w + ml + mr, h + mt + mb, 0.0, (8, 12, 18), 0.42 * velo);
        self.colpi.push((Forma::R((-ml, -mt, w + ml + mr, h + mt + mb)), Azione::Casa(CasaAz::ChiudiElenco)));
        let scuro = self.scuro();
        let mut gruppi: Vec<(usize, &'static str)> = Vec::new();
        let (titolo, voci, attuale): (String, Vec<RigaElenco>, Option<usize>) = match e {
            Elenco::Formato => {
                let tutte = self.operazioni();
                let cat = self.casa.categoria;
                let n_formati = conversioni::formats(cat).iter().filter(|s| s.id != "none").count();
                let op = self.casa.operazione();
                if n_formati < tutte.len() {
                    gruppi.push((0, "Converti in"));
                    gruppi.push((n_formati, voci::tools_heading(cat)));
                }
                let righe = tutte
                    .iter()
                    .map(|(id, _)| {
                        let v = voci::voce(cat, id);
                        RigaElenco { nome: v.label.to_string(), icona: Some((schiarisci(v.color, scuro), v.glyph.stroke, v.glyph.fill)), nota: v.hint }
                    })
                    .collect();
                ("Formato".into(), righe, tutte.iter().position(|t| t.0 == op))
            }
            Elenco::Opzione(k) => {
                let defs = self.casa.defs();
                match defs.get(k) {
                    Some(d) => match &d.control {
                        Control::Choice(c) | Control::Segments(c) => {
                            let v = self.casa.valori().get(d);
                            (d.title.to_string(), c.iter().map(|x| RigaElenco::semplice(x.label, voci::suggerimento(d.opt, x.value))).collect(), c.iter().position(|x| x.value == v))
                        }
                        _ => (String::new(), Vec::new(), None),
                    },
                    None => (String::new(), Vec::new(), None),
                }
            }
            Elenco::Esiste => {
                let k = match self.esiste {
                    cedro::Exists::Replace => 1,
                    cedro::Exists::Skip => 2,
                    _ => 0,
                };
                use cedro::Exists as X;
                (
                    "Se il file esiste già".into(),
                    vec![
                        RigaElenco::semplice("Aggiungi -2, -3", voci::suggerimento_esiste(X::Number)),
                        RigaElenco::semplice("Sostituisci il file", voci::suggerimento_esiste(X::Replace)),
                        RigaElenco::semplice("Salta il file", voci::suggerimento_esiste(X::Skip)),
                    ],
                    Some(k),
                )
            }
            Elenco::Voci => {
                let mut v = vec![RigaElenco::semplice("Automatico: le conta Cedro", ""), RigaElenco::semplice("Una persona sola", "")];
                v.extend((2..=8).map(|n| RigaElenco { nome: format!("{n} persone"), icona: None, nota: "" }));
                ("Quante persone parlano?".into(), v, Some(self.voci as usize))
            }
            Elenco::Attivita => {
                let att = attivita();
                ("Attività".into(), att.iter().map(|a| RigaElenco { nome: a.nome().to_string(), icona: Some((colore_att(*a, scuro), a.icona(), "")), nota: "" }).collect(), att.iter().position(|a| *a == self.casa.att))
            }
        };
        let con_note = voci.iter().any(|v| !v.nota.is_empty());
        let t = self.toni();
        let rh = if con_note { 64.0 } else { 56.0 };
        let gh = 36.0;
        let alto = 72.0 + voci.len() as f32 * rh + gruppi.len() as f32 * gh + 12.0;
        // Il foglio sale dal basso (angoli da 28, maniglia 32×4, fondo tonale).
        let entra = self.anim.comparsa(super::anim::Chiave::Foglio, 220.0);
        let fh = alto.min(h * 0.82);
        self.casa.elenco_max = (alto - fh).max(0.0);
        self.casa.elenco_scorri = self.casa.elenco_scorri.clamp(0.0, self.casa.elenco_max);
        let su = self.casa.elenco_scorri;
        let fy = h - fh * super::anim::esci(entra);
        p.rrect(-ml, fy, w + ml + mr, fh + mb + 40.0, 28.0, Some(t.basso), None);
        self.colpi.push((Forma::R((0.0, fy, w, fh)), Azione::Niente));
        p.rrect(w / 2.0 - 16.0, fy + 12.0, 32.0, 4.0, 2.0, Some(mix(t.basso, t.variante, 0.55)), None);
        p.text(&titolo, 24.0, fy + 26.0, w - 48.0, 32.0, 22.0, Weight::Semibold, t.testo, Align::Left);
        let vista = (0.0, fy + 66.0, w, fh - 66.0);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut ry = fy + 66.0 - su;
        for (k, riga) in voci.iter().enumerate() {
            if let Some((_, g)) = gruppi.iter().find(|(at, _)| *at == k) {
                p.text(g, 24.0, ry + 8.0, w - 48.0, gh - 8.0, 13.5, Weight::Semibold, t.variante, Align::Left);
                ry += gh;
            }
            let on = attuale == Some(k);
            let a = Azione::Casa(CasaAz::Voce(k));
            if on {
                p.rrect(12.0, ry + 2.0, w - 24.0, rh - 4.0, 20.0, Some(t.pri_c), None);
            } else if self.premuto == Some(a) {
                p.rrect(12.0, ry + 2.0, w - 24.0, rh - 4.0, 20.0, Some(mix(t.basso, t.testo, 0.06)), None);
            }
            let mut tx = 28.0;
            if let Some((col, tratto, pieno)) = riga.icona {
                let (cont, _) = super::m3::contenitore(col, t.fondo, scuro);
                let (ix, iy) = (24.0, ry + (rh - 40.0) / 2.0);
                p.rrect(ix, iy, 40.0, 40.0, 12.0, Some(if on { t.fondo } else { cont }), None);
                if !pieno.is_empty() {
                    p.icon_fill(pieno, ix + 9.0, iy + 9.0, 22.0, col);
                }
                p.icon(tratto, ix + 9.0, iy + 9.0, 22.0, col);
                tx = 78.0;
            }
            let largo = w - tx - 60.0;
            let fg = if on { t.su_pri_c } else { t.testo };
            let peso = if on { Weight::Bold } else { Weight::Regular };
            if riga.nota.is_empty() {
                p.text(&riga.nome, tx, ry, largo, rh, 16.0, peso, fg, Align::Left);
            } else {
                p.text(&riga.nome, tx, ry + 10.0, largo, 24.0, 16.0, peso, fg, Align::Left);
                p.text(riga.nota, tx, ry + 34.0, largo, 20.0, 13.0, Weight::Regular, if on { fg } else { t.variante }, Align::Left);
            }
            if on {
                p.icon(ic::CHECK, w - 52.0, ry + (rh - 22.0) / 2.0, 22.0, fg);
            }
            self.colpo((0.0, ry, w, rh), a, vista);
            ry += rh;
        }
        p.pop_clip();
    }

    // ---- La guida ----

    fn pagina_guida(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::Casa(CasaAz::Indietro));
        p.text("Come si usa Cedro", 60.0, 0.0, w - 72.0, 56.0, 21.0, Weight::Semibold, pal.text, Align::Left);
        let passi = [
            (Att::Conv, "Conversioni", "Aggiungi i file (o condividili con Cedro), scegli il formato e tocca «Converti». Le copie nuove vanno in Documenti/Cedro; l'originale non cambia."),
            (Att::Tras, "Trascrizione", "Un audio o un video diventa testo, sul telefono e senza internet. Dai Lavori «Rivedi» fa ascoltare le frasi e sistema le ripetizioni."),
            (Att::Web, "Video dal web", "Copia il link di una pagina con un video, toccate «Incolla» e scegli cosa tenere: il video, solo l'audio, una versione leggera o il testo."),
        ];
        let mut y = 72.0;
        let scuro = self.scuro();
        for (a, t, s) in passi {
            self.kt(p, 16.0, y, 34.0, colore_att(a, scuro), a.icona());
            p.text(t, 60.0, y, w - 76.0, 34.0, 17.0, Weight::Semibold, pal.text, Align::Left);
            y += 40.0;
            y += p.text_wrap(s, 60.0, y, w - 76.0, 14.0, Weight::Regular, pal.muted, 20.0, 6) + 18.0;
        }
        p.text_wrap("Da un'altra app, «Condividi» → Cedro apre la ruota con il file al centro: tocca uno spicchio e «Converti».", 16.0, y, w - 32.0, 14.0, Weight::Regular, pal.muted, 20.0, 4);
        let _ = h;
    }
}

