//! Le schermate dei PDF: Unisci, Dividi, Pagine, Comprimi, PDF in Excel e la password per aprire.
//! Ogni scelta finisce nelle `Options` del motore (`pdf_pages`, `pdf_edit`, pagine, password): niente controlli che
//! non facciano davvero qualcosa. Le miniature delle pagine le chiede il disegno (`chiedi`), le prepara `glue`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use cedro::ui::gfx::{Align, Fit, Image, Painter, Weight};
use cedro::ui::theme::mix;

use super::{ic, App, Azione, Effetto, Forma, Sorgente, BARRA, BIANCO, R, TOCCO};

pub const I_EXCEL: &str = "M3 4h18v16H3zM3 10h18M3 15h18M9 4v16";
pub const I_UNISCI: &str = "M6 4v6l6 6 6-6V4M12 16v4";
pub const I_DIVIDI: &str = "M6 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM20 4L8.1 15.9M14.5 14.5L20 20M8.1 8.1L12 12";
pub const I_PAGINE: &str = "M4 3h10v14H4zM10 21h8a2 2 0 0 0 2-2V8";
pub const I_COMPRIMI: &str = "M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7";
const I_RUOTA: &str = "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7";
const I_SPOSTA: &str = "M8 4v16M4 8l4-4 4 4M16 20V4M12 16l4 4 4-4";
const I_NUMERI: &str = "M5 9h14M5 15h14M10 4L8 20M16 4l-2 16";
const I_FILIGRANA: &str = "M12 3s6 7 6 11a6 6 0 0 1-12 0c0-4 6-11 6-11z";
const I_LUCCHETTO: &str = "M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4";
const I_OCCHIO: &str = "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z";
const I_MANIGLIA: &str = "M9 6h.01M9 12h.01M9 18h.01M15 6h.01M15 12h.01M15 18h.01";

/// Altezza del piede con il pulsante grande.
const PIEDE: f32 = 80.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pagina {
    Unisci,
    Dividi,
    Pagine,
    Comprimi,
    Excel,
}

/// La schermata di un'azione PDF della ruota o del foglio (`None`: l'azione parte subito).
pub fn schermata(id: &str) -> Option<Pagina> {
    match id {
        "merge" => Some(Pagina::Unisci),
        "split" => Some(Pagina::Dividi),
        "pages" => Some(Pagina::Pagine),
        "compress" => Some(Pagina::Comprimi),
        "xlsx" => Some(Pagina::Excel),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Campo {
    Password,
    NomeUnito,
    Intervalli,
    ExcelPagine,
    Filigrana,
    /// Le pagine di un PDF da unire.
    PagineDi(usize),
    /// Un testo di «Altre opzioni» (indice nell'elenco di ora).
    Opzione(usize),
    /// Il nome del file a modello, nelle Impostazioni.
    Modello,
    /// Il link di «Video dal web» e di YouTube.
    Link,
    /// Il nome di una voce in «Rivedi» › «Chi parla».
    Voce(u8),
}

/// Quello che arriva dalla tastiera.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tasto {
    Car(char),
    Canc,
    Invio,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PdfAz {
    Indietro,
    Vai,
    Aggiungi,
    Togli(usize),
    Maniglia(usize),
    ModoDividi(u8),
    Taglio(u32),
    Meno,
    Piu,
    PaginaTocco(usize),
    Strumento(u8),
    NumeriOn,
    NumPos(u8),
    NumFormato(u8),
    FiligranaOn,
    Livello(u8),
    Metadati,
    Bn,
    FoglioPerTabella(bool),
    XOcr,
    XValori,
    XRiunisci,
    XFuori,
    Annulla,
    Apri,
    MostraPw,
}

/// Un PDF da unire.
#[derive(Clone)]
pub struct DocUnito {
    pub path: PathBuf,
    pub nome: String,
    pub dim: u64,
    pub pagine: Option<u32>,
    pub thumb: Option<Image>,
    /// Le pagine scelte, come si scrivono («1-3,5»); vuoto = tutte.
    pub sel: String,
}

/// Una pagina in «Pagine»: il numero nel PDF di partenza (da 1), la rotazione e se va tolta.
#[derive(Clone, Copy)]
pub struct Pag {
    pub n: u32,
    pub rot: i32,
    pub via: bool,
}

pub struct StatoPdf {
    pub pagina: Option<Pagina>,
    /// Le pagine del PDF e i segnalibri di primo livello (pagina da 1, titolo), quando si sanno.
    pub totale: Option<u32>,
    pub segnalibri: Vec<(u32, String)>,
    /// Le miniature delle pagine del PDF (da 0) e quelle che il disegno vorrebbe e non ci sono.
    pub miniature: HashMap<u32, Image>,
    pub chiedi: Vec<u32>,
    /// Dove sta il campo in cui si scrive (y, altezza), per tenerlo sopra la tastiera.
    pub(super) campo_vis: Option<(f32, f32)>,
    /// Lo scorrimento è cambiato dopo il disegno: serve un altro giro.
    pub ridisegna: bool,
    // Password
    pub protetto: bool,
    pub password: String,
    /// La password che ha aperto il PDF (serve per unire altri PDF protetti con la stessa).
    pub usata: Option<String>,
    pub mostra_pw: bool,
    pub errore_pw: Option<String>,
    pub sbloccando: bool,
    // Unisci
    pub unisci: Vec<DocUnito>,
    pub nome_unito: String,
    /// La riga che si trascina: indice, y dove è cominciato, y adesso.
    pub trascina: Option<(usize, f32, f32)>,
    riga_h: f32,
    // Dividi
    pub modo_dividi: u8,
    pub tagli: Vec<u32>,
    pub ogni: u32,
    pub intervalli: String,
    // Pagine
    pub ordine: Vec<Pag>,
    pub scelte: HashSet<usize>,
    pub pannello: u8,
    pub sposta: bool,
    pub numeri: bool,
    pub num_pos: u8,
    pub num_formato: u8,
    pub filigrana: bool,
    pub testo_filigrana: String,
    // Comprimi
    pub livello: u8,
    pub metadati: bool,
    pub bn: bool,
    // Excel
    pub x_pagine: String,
    pub x_foglio: bool,
    pub x_ocr: bool,
    pub x_valori: bool,
    pub x_riunisci: bool,
    pub x_fuori: bool,
}

impl Default for StatoPdf {
    fn default() -> Self {
        let o = cedro::Options::default();
        let sq = cedro::Squeeze::default();
        let num = cedro::Numbers::default();
        StatoPdf {
            pagina: None,
            totale: None,
            segnalibri: Vec::new(),
            miniature: HashMap::new(),
            chiedi: Vec::new(),
            campo_vis: None,
            ridisegna: false,
            protetto: false,
            password: String::new(),
            usata: None,
            mostra_pw: false,
            errore_pw: None,
            sbloccando: false,
            unisci: Vec::new(),
            nome_unito: String::new(),
            trascina: None,
            riga_h: 96.0,
            modo_dividi: 0,
            tagli: Vec::new(),
            ogni: 1,
            intervalli: String::new(),
            ordine: Vec::new(),
            scelte: HashSet::new(),
            pannello: 3,
            sposta: false,
            numeri: false,
            num_pos: num.pos,
            num_formato: num.format,
            filigrana: false,
            testo_filigrana: cedro::Watermark::default().text,
            livello: sq.level,
            metadati: sq.strip_meta,
            bn: sq.bw_scans,
            x_pagine: String::new(),
            x_foglio: o.sheet_per_table,
            x_ocr: o.ocr,
            x_valori: o.cell_values,
            x_riunisci: o.merge_tables,
            x_fuori: o.outside_text,
        }
    }
}

impl StatoPdf {
    /// Il testo del pulsante grande del foglio per le azioni che aprono una schermata.
    pub fn etichetta_vai(&self, id: &str) -> Option<String> {
        Some(match id {
            "xlsx" => "Converti in Excel".into(),
            "merge" => "Scegli gli altri PDF".into(),
            "split" => match self.totale {
                Some(n) if n > 1 => format!("Dividi in {n} PDF"),
                _ => "Dividi".into(),
            },
            "pages" => "Apri le pagine".into(),
            "compress" => "Comprimi il PDF".into(),
            _ => return None,
        })
    }

    /// Lascia la riga trascinata di `dy` più giù (o su).
    pub fn lascia(&mut self, i: usize, dy: f32) {
        if i >= self.unisci.len() {
            return;
        }
        let passi = (dy / self.riga_h).round() as isize;
        let j = (i as isize + passi).clamp(0, self.unisci.len() as isize - 1) as usize;
        if j != i {
            let d = self.unisci.remove(i);
            self.unisci.insert(j, d);
        }
    }

    fn testo(&mut self, c: Campo) -> Option<&mut String> {
        Some(match c {
            Campo::Password => &mut self.password,
            Campo::NomeUnito => &mut self.nome_unito,
            Campo::Intervalli => &mut self.intervalli,
            Campo::ExcelPagine => &mut self.x_pagine,
            Campo::Filigrana => &mut self.testo_filigrana,
            Campo::PagineDi(i) => &mut self.unisci.get_mut(i)?.sel,
            Campo::Opzione(_) | Campo::Modello | Campo::Link | Campo::Voce(_) => return None,
        })
    }

    /// I gruppi che farebbe «Dividi» adesso, o perché non si può.
    fn gruppi(&self) -> Result<usize, String> {
        let tot = self.totale.ok_or("Sto leggendo il PDF…")?;
        match self.modo_dividi {
            0 => Ok(self.tagli.len() + 1),
            1 => Ok(tot.div_ceil(self.ogni.max(1)) as usize),
            2 => cedro::pdf_groups(&cedro::Plan::Ranges(self.intervalli.clone()), tot, None).map(|g| g.len()),
            _ if self.segnalibri.is_empty() => Err("Questo PDF non ha segnalibri".into()),
            _ => Ok(self.segnalibri.len()),
        }
    }

    fn piano(&self) -> cedro::Plan {
        match self.modo_dividi {
            0 => cedro::Plan::Cuts(self.tagli.clone()),
            1 => cedro::Plan::Every(self.ogni.max(1)),
            2 => cedro::Plan::Ranges(self.intervalli.clone()),
            _ => cedro::Plan::Bookmarks,
        }
    }

    fn rimaste(&self) -> usize {
        self.ordine.iter().filter(|p| !p.via).count()
    }
}

impl App {
    /// Apre la schermata di un'azione PDF.
    pub(super) fn apri_pagina(&mut self, pg: Pagina) -> Effetto {
        self.pdf.pagina = Some(pg);
        self.scorri = 0.0;
        self.campo = None;
        match pg {
            Pagina::Unisci => {
                self.primo_unito();
                if self.pdf.unisci.len() < 2 {
                    return Effetto::Scegli(Sorgente::PdfAltri);
                }
            }
            Pagina::Pagine => self.prepara_pagine(),
            _ => {}
        }
        Effetto::Ridisegna
    }

    /// Il PDF aperto in testa all'elenco di Unisci, con il nome del file unito.
    fn primo_unito(&mut self) {
        if self.pdf.unisci.is_empty() {
            if let Some(f) = &self.file {
                let nome = f.nome.clone();
                self.pdf.nome_unito = format!("{} unito", nome.strip_suffix(".pdf").or_else(|| nome.strip_suffix(".PDF")).unwrap_or(&nome));
                self.pdf.unisci.push(DocUnito { path: f.path.clone(), nome, dim: f.dim, pagine: self.pdf.totale, thumb: self.miniatura.clone(), sel: String::new() });
            }
        }
    }

    fn prepara_pagine(&mut self) {
        if let (true, Some(n)) = (self.pdf.ordine.is_empty(), self.pdf.totale) {
            self.pdf.ordine = (1..=n).map(|n| Pag { n, rot: 0, via: false }).collect();
        }
    }

    /// Il numero delle pagine del PDF (e i segnalibri) è arrivato.
    pub fn info_pdf(&mut self, totale: u32, segnalibri: Vec<(u32, String)>) {
        self.pdf.totale = Some(totale);
        self.pdf.segnalibri = segnalibri;
        if let Some(d) = self.pdf.unisci.first_mut() {
            if self.file.as_ref().is_some_and(|f| f.path == d.path) {
                d.pagine = Some(totale);
            }
        }
        if self.pdf.pagina == Some(Pagina::Pagine) {
            self.prepara_pagine();
        }
    }

    /// Altri PDF da unire, scelti o condivisi.
    pub fn aggiungi_pdf(&mut self, files: Vec<(PathBuf, String)>) {
        // Il PDF aperto è il primo dell'elenco (anche quando gli altri arrivano insieme a lui da «Condividi»).
        self.primo_unito();
        for (path, nome) in files {
            let dim = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            self.pdf.unisci.push(DocUnito { path, nome, dim, pagine: None, thumb: None, sel: String::new() });
        }
        if self.pdf.pagina != Some(Pagina::Unisci) {
            self.apri_pagina(Pagina::Unisci);
        }
    }

    /// Pagine e miniatura di un PDF da unire.
    pub fn info_unito(&mut self, path: &std::path::Path, pagine: Option<u32>, thumb: Option<Image>) {
        for d in self.pdf.unisci.iter_mut().filter(|d| d.path == path) {
            d.pagine = pagine.or(d.pagine);
            if thumb.is_some() {
                d.thumb = thumb.clone();
            }
        }
    }

    /// Il PDF chiede una password.
    pub fn chiedi_password(&mut self) {
        self.pdf.protetto = true;
        self.pdf.password.clear();
        self.pdf.errore_pw = None;
        self.pdf.sbloccando = false;
        self.campo = Some(Campo::Password);
    }

    /// La password andava: il file è la copia senza password.
    pub fn sbloccato(&mut self, path: PathBuf) {
        let pw = std::mem::take(&mut self.pdf.password);
        self.pdf.protetto = false;
        self.pdf.sbloccando = false;
        self.pdf.usata = Some(pw);
        self.campo = None;
        if let Some(f) = &mut self.file {
            f.path = path;
        }
    }

    pub fn password_sbagliata(&mut self, e: String) {
        self.pdf.sbloccando = false;
        self.pdf.errore_pw = Some(e);
        self.campo = Some(Campo::Password);
    }

    pub(super) fn annulla_password(&mut self) {
        self.pdf = StatoPdf::default();
        self.campo = None;
        self.file = None;
        self.miniatura = None;
        self.foglio = false;
        self.dal_foglio = false;
        self.scelta = None;
        self.scheda = super::Scheda::Ruota;
    }

    /// Un tasto: va nel campo attivo.
    pub fn tasto(&mut self, t: Tasto) -> Effetto {
        let Some(c) = self.campo else { return Effetto::Niente };
        match (c, t) {
            (Campo::Voce(_), Tasto::Invio) => {
                self.campo = None;
                if let Some(rv) = self.rivedi.as_ref() {
                    rv.salva_revisione();
                }
                return Effetto::Ridisegna;
            }
            (Campo::Voce(v), _) => {
                if let Some(rv) = self.rivedi.as_mut() {
                    rv.tasto_nome(v, t);
                }
                return Effetto::Ridisegna;
            }
            (_, Tasto::Invio) if c != Campo::Password => {
                self.campo = None;
                return Effetto::Ridisegna;
            }
            (Campo::Link, _) => {
                self.tasto_casa(t);
                return Effetto::Ridisegna;
            }
            (Campo::Opzione(k), _) => {
                self.tasto_opzione(k, t);
                return Effetto::Ridisegna;
            }
            (Campo::Modello, _) => {
                if let Some(s) = self.testo_opzione(c) {
                    match t {
                        Tasto::Canc => {
                            s.pop();
                        }
                        Tasto::Car(ch) if !ch.is_control() && s.chars().count() < 80 => s.push(ch),
                        _ => {}
                    }
                }
                return Effetto::SalvaImpostazioni;
            }
            _ => {}
        }
        match t {
            Tasto::Invio => {
                self.campo = None;
                if c == Campo::Password {
                    return self.azione_pdf(PdfAz::Apri);
                }
            }
            Tasto::Canc => {
                if let Some(s) = self.pdf.testo(c) {
                    s.pop();
                }
            }
            Tasto::Car(ch) => {
                let ok = match c {
                    Campo::Intervalli | Campo::ExcelPagine | Campo::PagineDi(_) => ch.is_ascii_digit() || matches!(ch, '-' | ',' | ';' | ' '),
                    _ => !ch.is_control(),
                };
                if let (true, Some(s)) = (ok, self.pdf.testo(c)) {
                    if s.chars().count() < 120 {
                        s.push(ch);
                    }
                }
                if c == Campo::Password {
                    self.pdf.errore_pw = None;
                }
            }
        }
        Effetto::Ridisegna
    }

    pub(super) fn azione_pdf(&mut self, az: PdfAz) -> Effetto {
        let s = &mut self.pdf;
        match az {
            PdfAz::Indietro => {
                s.pagina = None;
                self.scorri = 0.0;
            }
            PdfAz::Vai => return self.vai_pdf(),
            PdfAz::Aggiungi => return Effetto::Scegli(Sorgente::PdfAltri),
            PdfAz::Togli(i) => {
                if i < s.unisci.len() {
                    s.unisci.remove(i);
                }
            }
            PdfAz::Maniglia(_) => {}
            PdfAz::ModoDividi(m) => {
                s.modo_dividi = m;
                self.scorri = 0.0;
            }
            PdfAz::Taglio(n) => {
                if let Some(k) = s.tagli.iter().position(|t| *t == n) {
                    s.tagli.remove(k);
                } else {
                    s.tagli.push(n);
                    s.tagli.sort_unstable();
                }
            }
            PdfAz::Meno => s.ogni = s.ogni.saturating_sub(1).max(1),
            PdfAz::Piu => s.ogni = (s.ogni + 1).min(s.totale.unwrap_or(999).max(1)),
            PdfAz::PaginaTocco(i) => {
                if s.sposta && !s.scelte.is_empty() && !s.scelte.contains(&i) {
                    // Le pagine scelte vanno prima di quella toccata, nell'ordine in cui erano.
                    let mut prese: Vec<usize> = s.scelte.iter().copied().collect();
                    prese.sort_unstable();
                    let gruppo: Vec<Pag> = prese.iter().map(|k| s.ordine[*k]).collect();
                    let davanti = prese.iter().filter(|k| **k < i).count();
                    for k in prese.iter().rev() {
                        s.ordine.remove(*k);
                    }
                    let dove = i - davanti;
                    for (j, p) in gruppo.into_iter().enumerate() {
                        s.ordine.insert(dove + j, p);
                    }
                    s.scelte = (dove..dove + prese.len()).collect();
                    s.sposta = false;
                } else if !s.scelte.remove(&i) {
                    s.scelte.insert(i);
                }
            }
            PdfAz::Strumento(t) => match t {
                0 => {
                    for k in s.scelte.iter() {
                        if let Some(p) = s.ordine.get_mut(*k) {
                            p.rot = (p.rot + 90).rem_euclid(360);
                        }
                    }
                }
                1 => {
                    // Togli: se le scelte sono già tolte, tornano.
                    let tutte_via = s.scelte.iter().all(|k| s.ordine.get(*k).is_some_and(|p| p.via));
                    for k in s.scelte.iter() {
                        if let Some(p) = s.ordine.get_mut(*k) {
                            p.via = !tutte_via;
                        }
                    }
                }
                2 => s.sposta = !s.sposta && !s.scelte.is_empty(),
                _ => s.pannello = t,
            },
            PdfAz::NumeriOn => s.numeri = !s.numeri,
            PdfAz::NumPos(k) => {
                s.num_pos = k;
                s.numeri = true;
            }
            PdfAz::NumFormato(k) => {
                s.num_formato = k;
                s.numeri = true;
            }
            PdfAz::FiligranaOn => s.filigrana = !s.filigrana,
            PdfAz::Livello(l) => s.livello = l,
            PdfAz::Metadati => s.metadati = !s.metadati,
            PdfAz::Bn => s.bn = !s.bn,
            PdfAz::FoglioPerTabella(v) => s.x_foglio = v,
            PdfAz::XOcr => s.x_ocr = !s.x_ocr,
            PdfAz::XValori => s.x_valori = !s.x_valori,
            PdfAz::XRiunisci => s.x_riunisci = !s.x_riunisci,
            PdfAz::XFuori => s.x_fuori = !s.x_fuori,
            PdfAz::Annulla => self.annulla_password(),
            PdfAz::Apri => {
                if !s.password.is_empty() && !s.sbloccando {
                    s.sbloccando = true;
                    self.campo = None;
                    return Effetto::Sblocca(s.password.clone());
                }
            }
            PdfAz::MostraPw => s.mostra_pw = !s.mostra_pw,
        }
        Effetto::Ridisegna
    }

    /// Il pulsante grande di una schermata: le scelte diventano `Options` e il lavoro parte.
    fn vai_pdf(&mut self) -> Effetto {
        let Some(pg) = self.pdf.pagina else { return Effetto::Niente };
        let mut o = cedro::Options::default();
        let s = &self.pdf;
        match pg {
            Pagina::Excel => {
                let Ok(sel) = cedro::PageSel::parse(&s.x_pagine) else { return Effetto::Niente };
                o.pages = sel;
                o.ocr = s.x_ocr;
                o.sheet_per_table = s.x_foglio;
                o.cell_values = s.x_valori;
                o.merge_tables = s.x_riunisci;
                o.outside_text = s.x_fuori;
                self.avvia_con("xlsx", "Excel", true, Some(Box::new(o)), Vec::new())
            }
            Pagina::Comprimi => {
                o.pdf_edit.squeeze = Some(cedro::Squeeze { level: s.livello, strip_meta: s.metadati, bw_scans: s.bn });
                self.avvia_con("compress", "Comprimi", false, Some(Box::new(o)), Vec::new())
            }
            Pagina::Dividi => {
                if s.gruppi().is_err() {
                    return Effetto::Niente;
                }
                o.pdf_pages.plan = s.piano();
                self.avvia_con("split", "Dividi", false, Some(Box::new(o)), Vec::new())
            }
            Pagina::Pagine => {
                if s.rimaste() == 0 {
                    return Effetto::Niente;
                }
                o.pdf_edit.pages = s.ordine.iter().filter(|p| !p.via).map(|p| (p.n, p.rot)).collect();
                if s.numeri {
                    o.pdf_edit.stamp.numbers = Some(cedro::Numbers { pos: s.num_pos, format: s.num_formato, ..Default::default() });
                }
                if s.filigrana && !s.testo_filigrana.trim().is_empty() {
                    o.pdf_edit.stamp.watermark = Some(cedro::Watermark { text: s.testo_filigrana.trim().to_string(), ..Default::default() });
                }
                self.avvia_con("pages", "Pagine", false, Some(Box::new(o)), Vec::new())
            }
            Pagina::Unisci => {
                if s.unisci.len() < 2 {
                    return Effetto::Niente;
                }
                let mut sorgenti = Vec::new();
                for d in &s.unisci {
                    match cedro::PageSel::parse(&d.sel) {
                        Ok(sel) => sorgenti.push((d.path.clone(), sel)),
                        Err(_) => return Effetto::Niente,
                    }
                }
                o.pdf_pages.merge_name = Some(s.nome_unito.trim().to_string()).filter(|n| !n.is_empty());
                o.pdf_pages.open_password = s.usata.clone();
                let n = sorgenti.len();
                self.avvia_con("merge", &format!("Unisci {n} PDF"), false, Some(Box::new(o)), sorgenti)
            }
        }
    }

    // ---- Disegno ----

    pub(super) fn pagina_pdf(&mut self, p: &mut Painter, pg: Pagina, w: f32, h: f32) {
        let pal = self.pal();
        let titolo = match pg {
            Pagina::Unisci => "Unisci",
            Pagina::Dividi => "Dividi",
            Pagina::Pagine => "Pagine",
            Pagina::Comprimi => "Comprimi",
            Pagina::Excel => "PDF in Excel",
        };
        let sotto = match pg {
            Pagina::Dividi => self.file.as_ref().map(|f| f.nome.clone()).unwrap_or_default(),
            Pagina::Pagine if !self.pdf.scelte.is_empty() => match self.pdf.scelte.len() {
                1 => "1 scelta".into(),
                n => format!("{n} scelte"),
            },
            _ => String::new(),
        };
        self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::Pdf(PdfAz::Indietro));
        let sw = if sotto.is_empty() { 0.0 } else { p.measure(&sotto, 14.0, Weight::Regular).min(w * 0.45) + 16.0 };
        p.text(titolo, 60.0, 0.0, w - 72.0 - sw, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
        if !sotto.is_empty() {
            p.text(&sotto, w - sw, 0.0, sw - 16.0, BARRA, 14.0, Weight::Regular, pal.muted, Align::Right);
        }
        let piede = if pg == Pagina::Unisci { PIEDE + 26.0 } else { PIEDE };
        let corpo = (0.0, BARRA, w, h - BARRA - piede);
        let ampio = w > 700.0;
        let cw = if ampio { 560.0 } else { w - 32.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 16.0 };
        p.push_clip(corpo.0, corpo.1, corpo.2, corpo.3);
        self.pdf.campo_vis = None;
        let y0 = BARRA + 4.0 - self.scorri;
        let fine = match pg {
            Pagina::Unisci => self.corpo_unisci(p, x, y0, cw, corpo),
            Pagina::Dividi => self.corpo_dividi(p, x, y0, cw, corpo),
            Pagina::Pagine => self.corpo_pagine(p, x, y0, cw, corpo),
            Pagina::Comprimi => self.corpo_comprimi(p, x, y0, cw, corpo),
            Pagina::Excel => self.corpo_excel(p, x, y0, cw, corpo),
        };
        p.pop_clip();
        self.scorri_max = (fine + 12.0 + self.scorri - (corpo.1 + corpo.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(corpo);
        // Il piede: il pulsante grande, sempre in basso.
        let (testo, icona, attivo, nota) = self.piede_pdf(pg);
        let by = h - PIEDE + 12.0;
        if let Some(n) = nota {
            p.text(&n, x, by - 30.0, cw, 24.0, 13.0, Weight::Regular, pal.muted, Align::Center);
        }
        self.pulsante(p, (x, by, cw, 56.0), icona, &testo, attivo.then_some(Azione::Pdf(PdfAz::Vai)));
    }

    /// Il campo in cui si scrive resta a vista (la tastiera accorcia la finestra): si scorre fin lì.
    pub(super) fn tieni_campo(&mut self, corpo: R) {
        if let (Some(_), Some((cy, ch))) = (self.campo, self.pdf.campo_vis) {
            let prima = self.scorri;
            if cy + ch > corpo.1 + corpo.3 - 8.0 {
                self.scorri = (self.scorri + cy + ch - (corpo.1 + corpo.3) + 16.0).min(self.scorri_max);
            } else if cy < corpo.1 + 8.0 {
                self.scorri = (self.scorri - (corpo.1 - cy) - 16.0).max(0.0);
            }
            self.pdf.ridisegna = self.scorri != prima;
        }
    }

    fn piede_pdf(&self, pg: Pagina) -> (String, &'static str, bool, Option<String>) {
        let s = &self.pdf;
        match pg {
            Pagina::Excel => ("Converti in Excel".into(), I_EXCEL, cedro::PageSel::parse(&s.x_pagine).is_ok(), None),
            Pagina::Comprimi => ("Comprimi il PDF".into(), I_COMPRIMI, true, None),
            Pagina::Dividi => match (s.modo_dividi, s.gruppi()) {
                (3, Ok(_)) => ("Dividi ai segnalibri".into(), I_DIVIDI, true, None),
                (_, Ok(n)) if n > 1 => (format!("Dividi in {n} PDF"), I_DIVIDI, true, None),
                (0, Ok(_)) => ("Scegli dove tagliare".into(), I_DIVIDI, false, None),
                (_, Ok(_)) => ("Dividi in 1 PDF".into(), I_DIVIDI, false, None),
                (_, Err(_)) => ("Dividi".into(), I_DIVIDI, false, None),
            },
            Pagina::Pagine => {
                let n = s.rimaste();
                (if n == 1 { "Salva il PDF (1 pagina)".into() } else { format!("Salva il PDF ({n} pagine)") }, ic::CHECK, n > 0 && !s.ordine.is_empty(), None)
            }
            Pagina::Unisci => {
                let ok = s.unisci.len() >= 2 && s.unisci.iter().all(|d| cedro::PageSel::parse(&d.sel).is_ok());
                let tot: Option<u32> = s.unisci.iter().map(|d| d.pagine.map(|n| conta_pagine(&d.sel, n))).sum();
                let nota = match tot {
                    Some(t) if ok => Some(format!("{t} pagine in tutto, salvate in Documenti/Cedro")),
                    _ if s.unisci.len() < 2 => Some("Servono almeno due PDF".into()),
                    _ => None,
                };
                (format!("Unisci {} PDF", s.unisci.len()), I_UNISCI, ok, nota)
            }
        }
    }

    // ---- Pezzi comuni ----

    pub(super) fn etichetta(&self, p: &mut Painter, x: f32, y: f32, w: f32, t: &str) -> f32 {
        p.text(t, x + 2.0, y, w, 28.0, 14.0, Weight::Semibold, self.pal().muted, Align::Left);
        30.0
    }

    /// Il pulsante grande pieno; senza azione è spento.
    fn pulsante(&mut self, p: &mut Painter, bt: R, icona: &str, testo: &str, az: Option<Azione>) {
        let pal = self.pal();
        let prem = az.is_some() && self.premuto == az;
        let fondo = if az.is_none() { pal.alt } else if prem { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary };
        let fg = if az.is_some() { pal.on_primary } else { pal.muted };
        p.rrect(bt.0, bt.1, bt.2, bt.3, bt.3 / 2.0, Some(fondo), None);
        let tw = p.measure(testo, 18.0, Weight::Semibold).min(bt.2 - 70.0);
        let ix = bt.0 + (bt.2 - tw - 34.0) / 2.0;
        p.icon(icona, ix, bt.1 + (bt.3 - 24.0) / 2.0, 24.0, fg);
        p.text(testo, ix + 34.0, bt.1, tw + 4.0, bt.3, 18.0, Weight::Semibold, fg, Align::Left);
        if let Some(a) = az {
            self.colpi.push((Forma::R(bt), a));
        }
    }

    pub(super) fn interruttore(&self, p: &mut Painter, x: f32, cy: f32, on: bool) {
        let pal = self.pal();
        p.rrect(x, cy - 15.0, 52.0, 30.0, 15.0, Some(if on { pal.primary } else { pal.border }), None);
        p.circle(if on { x + 37.0 } else { x + 15.0 }, cy, 11.0, Some(BIANCO), None);
    }

    /// Una riga con titolo, spiegazione e interruttore. Restituisce l'altezza.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn riga_sw(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, titolo: &str, sotto: &str, on: bool, az: Azione, vista: R) -> f32 {
        let pal = self.pal();
        let righe = p.wrap_lines(sotto, 13.0, Weight::Regular, w - 90.0, 2).len().max(1);
        let h = 44.0 + 20.0 * righe as f32;
        let prem = self.premuto == Some(az);
        p.rrect(x, y, w, h, 16.0, Some(if prem { mix(pal.surface, pal.primary, 0.1) } else { pal.surface }), Some((pal.border, 1.0)));
        p.text(titolo, x + 14.0, y + 8.0, w - 90.0, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        p.text_wrap(sotto, x + 14.0, y + 35.0, w - 90.0, 13.0, Weight::Regular, pal.muted, 20.0, 2);
        self.interruttore(p, x + w - 66.0, y + h / 2.0, on);
        self.colpo((x, y, w, h), az, vista);
        h
    }

    pub(super) fn colpo(&mut self, rc: R, az: Azione, vista: R) {
        if let Some(r) = super::intersezione(rc, vista) {
            self.colpi.push((Forma::R(r), az));
        }
    }

    /// Un campo di testo: tocca e scrivi. `nascosto`: puntini al posto delle lettere.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn campo_testo(&mut self, p: &mut Painter, rc: R, c: Campo, testo: &str, segnaposto: &str, suffisso: &str, nascosto: bool, vista: R) {
        let pal = self.pal();
        let attivo = self.campo == Some(c);
        if attivo {
            self.pdf.campo_vis = Some((rc.1, rc.3));
        }
        if self.tema == cedro::ui::theme::ThemeId::WindowsEssenziale {
            // Campo pieno di M3: fondo tonale, la riga del colore principale solo quando si scrive.
            let t = self.toni();
            p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(t.basso), if attivo { Some((pal.primary, 2.0)) } else { None });
        } else {
            p.rrect(rc.0, rc.1, rc.2, rc.3, 14.0, Some(pal.surface), Some((if attivo { pal.primary } else { pal.border }, if attivo { 2.0 } else { 1.0 })));
        }
        let sw = if suffisso.is_empty() { 0.0 } else { p.measure(suffisso, 16.0, Weight::Regular) + 8.0 };
        let mostra: String = if nascosto { "•".repeat(testo.chars().count()) } else { testo.to_string() };
        let vuoto = mostra.is_empty();
        let largo = rc.2 - 32.0 - sw;
        // Il testo lungo mostra la fine, dove si scrive.
        let mut vis = mostra.clone();
        while !vis.is_empty() && p.measure(&vis, 17.0, Weight::Regular) > largo - 4.0 {
            vis.remove(0);
        }
        p.text(if vuoto { segnaposto } else { &vis }, rc.0 + 16.0, rc.1, largo, rc.3, 17.0, Weight::Regular, if vuoto { pal.muted } else { pal.text }, Align::Left);
        if attivo {
            let cx = rc.0 + 16.0 + if vuoto { 0.0 } else { p.measure(&vis, 17.0, Weight::Regular) } + 1.0;
            p.fill(cx, rc.1 + 12.0, 2.0, rc.3 - 24.0, pal.primary);
        }
        if !suffisso.is_empty() {
            p.text(suffisso, rc.0 + rc.2 - 16.0 - sw, rc.1, sw, rc.3, 16.0, Weight::Regular, pal.muted, Align::Right);
        }
        self.colpo(rc, Azione::Campo(c), vista);
    }

    /// Una scelta a pillole dentro una zona che scorre.
    pub(super) fn pillole_in(&mut self, p: &mut Painter, rc: R, voci: &[&str], scelta: usize, az: impl Fn(usize) -> Azione, vista: R) {
        let n0 = self.colpi.len();
        self.pillole(p, rc, voci, scelta, az);
        let nuovi: Vec<(Forma, Azione)> = self.colpi.drain(n0..).collect();
        for (f, a) in nuovi {
            if let Forma::R(r) = f {
                self.colpo(r, a, vista);
            }
        }
    }

    /// Una pagina del PDF (da 0), girata di `rot` gradi, dentro `rc`. Senza miniatura la chiede a `glue`.
    fn pagina_mini(&mut self, p: &mut Painter, rc: R, n: u32, rot: i32, vista: R) {
        let pal = self.pal();
        let visibile = super::intersezione(rc, vista).is_some();
        match self.pdf.miniature.get(&n) {
            Some(img) => {
                let img = img.turned(rot / 90);
                let k = (rc.2 / img.w as f32).min(rc.3 / img.h as f32);
                let (iw, ih) = (img.w as f32 * k, img.h as f32 * k);
                let (ix, iy) = (rc.0 + (rc.2 - iw) / 2.0, rc.1 + (rc.3 - ih) / 2.0);
                p.shadow(ix, iy, iw, ih, 3.0, 3.0, 0.10, 1.0);
                p.rrect(ix, iy, iw, ih, 3.0, Some(BIANCO), Some((pal.border, 1.0)));
                p.image(&img, ix, iy, iw, ih, 3.0, Fit::Contain);
            }
            None => {
                let (pw, ph) = if rot % 180 == 0 { (rc.3 / 1.414, rc.3) } else { (rc.2, rc.2 / 1.414) };
                let (pw, ph) = (pw.min(rc.2), ph.min(rc.3));
                let (ix, iy) = (rc.0 + (rc.2 - pw) / 2.0, rc.1 + (rc.3 - ph) / 2.0);
                p.rrect(ix, iy, pw, ph, 3.0, Some(BIANCO), Some((pal.border, 1.0)));
                for k in 0..4 {
                    p.fill(ix + 8.0, iy + 12.0 + k as f32 * 9.0, pw - 16.0 - (k % 2) as f32 * pw * 0.25, 3.0, (225, 225, 230));
                }
                if visibile && !self.pdf.chiedi.contains(&n) {
                    self.pdf.chiedi.push(n);
                }
            }
        }
    }

    // ---- Unisci ----

    fn corpo_unisci(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        y += p.text_wrap("L'ordine qui sotto è l'ordine delle pagine nel file unito. Tieni premuta la maniglia per spostare un file.", x + 2.0, y, w - 4.0, 14.0, Weight::Regular, pal.muted, 20.0, 3) + 10.0;
        let rh = 88.0;
        self.pdf.riga_h = rh + 10.0;
        let trascina = self.pdf.trascina;
        let docs = self.pdf.unisci.clone();
        for (i, d) in docs.iter().enumerate() {
            let mut ry = y + i as f32 * (rh + 10.0);
            let preso = trascina.is_some_and(|t| t.0 == i);
            if let (true, Some((_, a, b))) = (preso, trascina) {
                ry += b - a;
            }
            if preso {
                continue;
            }
            self.riga_unito(p, i, d, (x, ry, w, rh), false, vista);
        }
        if let Some((i, a, b)) = trascina {
            if let Some(d) = docs.get(i) {
                let ry = y + i as f32 * (rh + 10.0) + (b - a);
                self.riga_unito(p, i, d, (x, ry, w, rh), true, vista);
            }
        }
        y += docs.len() as f32 * (rh + 10.0);
        // «Aggiungi PDF»
        let rc = (x, y, w, 56.0);
        let az = Azione::Pdf(PdfAz::Aggiungi);
        if self.premuto == Some(az) {
            p.rrect_a(rc.0, rc.1, rc.2, rc.3, 16.0, pal.primary, 0.1);
        }
        p.rrect_line(rc.0, rc.1, rc.2, rc.3, 16.0, pal.border, 1.5, 1.0, Some((6.0, 5.0)));
        let tw = p.measure("Aggiungi PDF", 16.0, Weight::Semibold);
        p.icon(ic::ADD, rc.0 + (rc.2 - tw - 30.0) / 2.0, rc.1 + 16.0, 22.0, pal.primary);
        p.text("Aggiungi PDF", rc.0 + (rc.2 - tw - 30.0) / 2.0 + 30.0, rc.1, tw + 4.0, rc.3, 16.0, Weight::Semibold, pal.primary, Align::Left);
        self.colpo(rc, az, vista);
        y += 56.0 + 14.0;
        y += self.etichetta(p, x, y, w, "Nome del file unito");
        let nome = self.pdf.nome_unito.clone();
        self.campo_testo(p, (x, y, w, 52.0), Campo::NomeUnito, &nome, "Nome", ".pdf", false, vista);
        y + 52.0
    }

    fn riga_unito(&mut self, p: &mut Painter, i: usize, d: &DocUnito, rc: R, preso: bool, vista: R) {
        let pal = self.pal();
        if preso {
            p.shadow(rc.0, rc.1, rc.2, rc.3, 16.0, 10.0, 0.16, 6.0);
        }
        p.rrect(rc.0, rc.1, rc.2, rc.3, 16.0, Some(pal.surface), Some((if preso { pal.primary } else { pal.border }, if preso { 2.0 } else { 1.0 })));
        // Maniglia
        p.icon(I_MANIGLIA, rc.0 + 8.0, rc.1 + rc.3 / 2.0 - 12.0, 24.0, if preso { pal.primary } else { pal.muted });
        self.colpo((rc.0, rc.1, 40.0, rc.3), Azione::Pdf(PdfAz::Maniglia(i)), vista);
        // Miniatura
        let (mx, my, mw, mh) = (rc.0 + 40.0, rc.1 + 12.0, 46.0, rc.3 - 24.0);
        p.rrect(mx, my, mw, mh, 4.0, Some(BIANCO), Some((pal.border, 1.0)));
        if let Some(t) = &d.thumb {
            p.image(t, mx + 1.0, my + 1.0, mw - 2.0, mh - 2.0, 3.0, Fit::Contain);
        }
        let tx = mx + mw + 12.0;
        let tw = rc.0 + rc.2 - tx - 96.0;
        p.text(&d.nome, tx, rc.1 + 8.0, tw, 24.0, 15.0, Weight::Semibold, pal.text, Align::Left);
        let info = match d.pagine {
            Some(1) => format!("1 pagina · {}", super::peso(d.dim)),
            Some(n) => format!("{n} pagine · {}", super::peso(d.dim)),
            None => super::peso(d.dim),
        };
        p.text(&info, tx, rc.1 + 31.0, tw, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        // Le pagine: si toccano e si scrivono.
        let campo = Campo::PagineDi(i);
        let attivo = self.campo == Some(campo);
        let errato = cedro::PageSel::parse(&d.sel).is_err();
        let et = if d.sel.trim().is_empty() && !attivo { "Pagine: tutte".to_string() } else { format!("Pagine: {}", d.sel) };
        let ew = (p.measure(&et, 13.0, Weight::Semibold) + 22.0).min(tw + 40.0);
        let chip = (tx, rc.1 + 54.0, ew, 26.0);
        p.rrect(chip.0, chip.1, chip.2, chip.3, 13.0, Some(pal.alt), Some((if errato { pal.bad } else if attivo { pal.primary } else { pal.border }, 1.5)));
        p.text(&et, chip.0 + 10.0, chip.1, chip.2 - 16.0, chip.3, 13.0, Weight::Semibold, if errato { pal.bad } else { pal.primary }, Align::Left);
        if attivo {
            p.fill(chip.0 + chip.2 - 9.0, chip.1 + 6.0, 2.0, 14.0, pal.primary);
            self.pdf.campo_vis = Some((chip.1, chip.3));
        }
        self.colpo((chip.0, chip.1 - 8.0, chip.2, chip.3 + 12.0), Azione::Campo(campo), vista);
        // Numero d'ordine e «togli».
        p.circle(rc.0 + rc.2 - 70.0, rc.1 + rc.3 / 2.0, 15.0, Some(pal.alt), None);
        p.text(&(i + 1).to_string(), rc.0 + rc.2 - 85.0, rc.1 + rc.3 / 2.0 - 15.0, 30.0, 30.0, 15.0, Weight::Semibold, pal.primary, Align::Center);
        let tg = (rc.0 + rc.2 - 48.0, rc.1 + (rc.3 - 48.0) / 2.0, 44.0, 48.0);
        if self.premuto == Some(Azione::Pdf(PdfAz::Togli(i))) {
            p.circle(tg.0 + 22.0, tg.1 + 24.0, 20.0, Some(pal.alt), None);
        }
        p.icon(ic::CLOSE, tg.0 + 11.0, tg.1 + 13.0, 22.0, pal.muted);
        self.colpo(tg, Azione::Pdf(PdfAz::Togli(i)), vista);
    }

    // ---- Dividi ----

    fn corpo_dividi(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let modo = self.pdf.modo_dividi as usize;
        self.pillole_in(p, (x, y, w, TOCCO), &["Tagli", "Ogni N", "Intervalli", "Segnalibri"], modo, |i| Azione::Pdf(PdfAz::ModoDividi(i as u8)), vista);
        y += TOCCO + 12.0;
        let tot = self.pdf.totale.unwrap_or(0);
        match modo {
            0 => {
                y += p.text_wrap("Tocca le forbici tra due pagine per tagliare lì.", x + 2.0, y, w, 14.0, Weight::Regular, pal.muted, 20.0, 2) + 8.0;
                let cols = if w > 500.0 { 6 } else { 4 };
                let gap = 18.0;
                let cw = (w - 8.0 - gap * (cols as f32 - 1.0)) / cols as f32;
                let ch = cw * 1.36;
                let mut gruppo = 1;
                for i in 0..tot {
                    let (c, r) = (i as usize % cols, i as usize / cols);
                    let rc = (x + c as f32 * (cw + gap), y + r as f32 * (ch + 14.0), cw, ch);
                    let primo = i == 0 || self.pdf.tagli.contains(&i);
                    if primo && i > 0 {
                        gruppo += 1;
                    }
                    self.pagina_mini(p, rc, i, 0, vista);
                    if gruppo % 2 == 1 {
                        p.rrect_line(rc.0 - 2.0, rc.1 - 2.0, rc.2 + 4.0, rc.3 + 4.0, 5.0, pal.primary, 2.0, 1.0, None);
                    }
                    p.text(&(i + 1).to_string(), rc.0, rc.1 + rc.3 - 22.0, rc.2, 20.0, 12.0, Weight::Semibold, (90, 90, 100), Align::Center);
                    if primo {
                        let (bx, by) = (rc.0 + 4.0, rc.1 + 4.0);
                        if gruppo % 2 == 1 {
                            p.circle(bx + 10.0, by + 10.0, 10.0, Some(pal.primary), None);
                        } else {
                            p.circle(bx + 10.0, by + 10.0, 10.0, Some(pal.alt), Some((pal.primary, 1.5)));
                        }
                        p.text(&gruppo.to_string(), bx, by, 20.0, 20.0, 11.0, Weight::Semibold, if gruppo % 2 == 1 { pal.on_primary } else { pal.primary }, Align::Center);
                    }
                    if i + 1 < tot {
                        // Le forbici tra questa pagina e la prossima: un file nuovo dopo la pagina i+1.
                        let n = i + 1;
                        let on = self.pdf.tagli.contains(&n);
                        let f = (rc.0 + rc.2 - 2.0, rc.1 + rc.3 / 2.0 - 22.0, 22.0, 44.0);
                        if on {
                            p.rrect(f.0, f.1, f.2, f.3, 8.0, Some(pal.alt), None);
                        }
                        p.icon(I_DIVIDI, f.0 + 2.0, f.1 + 13.0, 18.0, if on { pal.primary } else { pal.border });
                        self.colpo((f.0 - 6.0, f.1, f.2 + 12.0, f.3), Azione::Pdf(PdfAz::Taglio(n)), vista);
                    }
                }
                y += (tot as usize).div_ceil(cols) as f32 * (ch + 14.0);
            }
            1 => {
                y += self.etichetta(p, x, y, w, "Un file ogni");
                let (bx, cy) = (x, y + 30.0);
                for (k, (t, az)) in [("−", PdfAz::Meno), ("+", PdfAz::Piu)].into_iter().enumerate() {
                    let cx = bx + 26.0 + k as f32 * 130.0;
                    let a = Azione::Pdf(az);
                    p.circle(cx, cy, 26.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.primary, 2.0)));
                    p.text(t, cx - 26.0, cy - 26.0, 52.0, 52.0, 24.0, Weight::Bold, pal.primary, Align::Center);
                    self.colpo((cx - 28.0, cy - 28.0, 56.0, 56.0), a, vista);
                }
                p.text(&self.pdf.ogni.to_string(), bx + 52.0, cy - 26.0, 78.0, 52.0, 32.0, Weight::Bold, pal.text, Align::Center);
                p.text("pagine", bx + 172.0, cy - 26.0, 120.0, 52.0, 15.0, Weight::Regular, pal.muted, Align::Left);
                y += 70.0;
                y += p.text_wrap("Con 1 ogni pagina diventa un PDF.", x + 2.0, y, w, 14.0, Weight::Regular, pal.muted, 20.0, 2);
            }
            2 => {
                y += self.etichetta(p, x, y, w, "Gruppi di pagine");
                let t = self.pdf.intervalli.clone();
                self.campo_testo(p, (x, y, w, 52.0), Campo::Intervalli, &t, "1-4, 5-8, 9-", "", false, vista);
                y += 60.0;
                let (msg, col) = match self.pdf.gruppi() {
                    Err(e) if !t.trim().is_empty() => (maiuscola(&e), pal.bad),
                    _ => ("Un file per gruppo. «9-» vuol dire dalla 9 alla fine.".to_string(), pal.muted),
                };
                y += p.text_wrap(&msg, x + 2.0, y, w, 14.0, Weight::Regular, col, 20.0, 3);
            }
            _ => {
                if self.pdf.segnalibri.is_empty() {
                    y += p.text_wrap("Questo PDF non ha segnalibri: scegli un altro modo per dividerlo.", x + 2.0, y, w, 14.0, Weight::Regular, pal.muted, 20.0, 3);
                } else {
                    y += p.text_wrap("Un file per ogni segnalibro principale del PDF.", x + 2.0, y, w, 14.0, Weight::Regular, pal.muted, 20.0, 2) + 8.0;
                    let bm = self.pdf.segnalibri.clone();
                    for (k, (inizio, titolo)) in bm.iter().enumerate() {
                        let fine = bm.get(k + 1).map(|b| b.0 - 1).unwrap_or(tot).max(*inizio);
                        let pag = if fine > *inizio { format!("Pagine {inizio}-{fine}") } else { format!("Pagina {inizio}") };
                        super::sollevato(p, pal, self.scuro(), x, y, w, 60.0, 16.0);
                        let nome = match titolo.as_str() {
                            "" => format!("{}. Senza titolo", k + 1),
                            t if t.starts_with(|c: char| c.is_ascii_digit()) => t.to_string(),
                            t => format!("{}. {t}", k + 1),
                        };
                        p.text(&nome, x + 14.0, y + 6.0, w - 28.0, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
                        p.text(&pag, x + 14.0, y + 31.0, w - 28.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
                        y += 68.0;
                    }
                }
            }
        }
        y
    }

    // ---- Pagine ----

    fn corpo_pagine(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        if self.pdf.ordine.is_empty() {
            p.text("Sto leggendo le pagine…", x, y, w, 40.0, 15.0, Weight::Regular, pal.muted, Align::Left);
            return y + 40.0;
        }
        let msg = if self.pdf.sposta {
            "Tocca la pagina davanti a cui spostare quelle scelte."
        } else {
            "Tocca le pagine per sceglierle, poi usa i pulsanti qui sotto."
        };
        y += p.text_wrap(msg, x + 2.0, y, w, 14.0, Weight::Regular, if self.pdf.sposta { pal.primary } else { pal.muted }, 20.0, 2) + 8.0;
        let cols = if w > 500.0 { 6 } else { 4 };
        let gap = 10.0;
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw * 1.3;
        let ordine = self.pdf.ordine.clone();
        for (k, pg) in ordine.iter().enumerate() {
            let rc = (x + (k % cols) as f32 * (cw + gap), y + (k / cols) as f32 * (ch + gap), cw, ch);
            let sel = self.pdf.scelte.contains(&k);
            if sel {
                p.rrect(rc.0, rc.1, rc.2, rc.3, 10.0, Some(mix(pal.surface, pal.primary, 0.14)), Some((pal.primary, 2.0)));
            }
            let inner = (rc.0 + 6.0, rc.1 + 6.0, rc.2 - 12.0, rc.3 - 26.0);
            self.pagina_mini(p, inner, pg.n - 1, pg.rot, vista);
            if pg.via {
                p.rrect_a(inner.0, inner.1, inner.2, inner.3, 4.0, pal.bg, 0.72);
                p.icon(ic::TRASH, inner.0 + inner.2 / 2.0 - 15.0, inner.1 + inner.3 / 2.0 - 15.0, 30.0, pal.bad);
            }
            let et = if pg.rot != 0 { format!("{} · {}°", pg.n, pg.rot) } else { pg.n.to_string() };
            p.text(&et, rc.0, rc.1 + rc.3 - 21.0, rc.2, 18.0, 12.0, Weight::Semibold, if sel { pal.primary } else { pal.muted }, Align::Center);
            if sel {
                p.circle(rc.0 + rc.2 - 12.0, rc.1 + 12.0, 11.0, Some(pal.primary), None);
                p.icon(ic::CHECK, rc.0 + rc.2 - 20.0, rc.1 + 4.0, 16.0, pal.on_primary);
            }
            self.colpo(rc, Azione::Pdf(PdfAz::PaginaTocco(k)), vista);
        }
        y += ordine.len().div_ceil(cols) as f32 * (ch + gap) + 6.0;
        // Gli strumenti.
        let voci = [("Ruota", I_RUOTA), ("Togli", ic::TRASH), ("Sposta", I_SPOSTA), ("Numeri", I_NUMERI), ("Filigrana", I_FILIGRANA)];
        let bw = w / voci.len() as f32;
        let nessuna = self.pdf.scelte.is_empty();
        for (k, (nome, icona)) in voci.into_iter().enumerate() {
            let rc = (x + k as f32 * bw, y, bw, 62.0);
            let on = (k as u8 == self.pdf.pannello && k >= 3) || (k == 2 && self.pdf.sposta);
            let spento = k < 3 && nessuna;
            let az = Azione::Pdf(PdfAz::Strumento(k as u8));
            if on || self.premuto == Some(az) {
                p.rrect(rc.0 + 3.0, rc.1, rc.2 - 6.0, rc.3, 14.0, Some(mix(pal.surface, pal.primary, if on { 0.16 } else { 0.08 })), None);
            }
            let col = if spento { pal.border } else if on { pal.primary } else { pal.text };
            p.icon(icona, rc.0 + rc.2 / 2.0 - 11.0, rc.1 + 8.0, 22.0, col);
            p.text(nome, rc.0, rc.1 + 34.0, rc.2, 22.0, 12.0, Weight::Semibold, col, Align::Center);
            if !spento {
                self.colpo(rc, az, vista);
            }
        }
        y += 70.0;
        // Il pannello di Numeri o Filigrana.
        let pw = w;
        if self.pdf.pannello == 3 {
            let h = 150.0;
            super::sollevato(p, pal, self.scuro(), x, y, pw, h, 18.0);
            // Dove: una griglia 3×3.
            for k in 0..9u8 {
                let (gx, gy) = (x + 14.0 + (k % 3) as f32 * 18.0, y + 14.0 + (k / 3) as f32 * 18.0);
                let on = k == self.pdf.num_pos && self.pdf.numeri;
                p.rrect(gx, gy, 14.0, 14.0, 3.0, Some(if on { pal.primary } else { pal.alt }), Some((pal.border, 1.0)));
                self.colpo((gx - 2.0, gy - 2.0, 18.0, 18.0), Azione::Pdf(PdfAz::NumPos(k)), vista);
            }
            let posizioni = ["In alto a sinistra", "In alto al centro", "In alto a destra", "A metà a sinistra", "Al centro", "A metà a destra", "In basso a sinistra", "In basso al centro", "In basso a destra"];
            p.text("Numeri di pagina", x + 80.0, y + 10.0, pw - 160.0, 24.0, 16.0, Weight::Semibold, pal.text, Align::Left);
            p.text(posizioni[self.pdf.num_pos.min(8) as usize], x + 80.0, y + 34.0, pw - 160.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
            self.interruttore(p, x + pw - 66.0, y + 32.0, self.pdf.numeri);
            self.colpo((x + pw - 76.0, y + 6.0, 72.0, 52.0), Azione::Pdf(PdfAz::NumeriOn), vista);
            let tot = self.pdf.rimaste().max(1) as u32;
            let mut fx = x + 14.0;
            for k in 0..3u8 {
                let t = numero(k, tot);
                let fw = p.measure(&t, 14.0, Weight::Semibold) + 28.0;
                let rc = (fx, y + 84.0, fw, 44.0);
                let on = self.pdf.num_formato == k;
                p.rrect(rc.0, rc.1, rc.2, rc.3, 22.0, Some(if on { pal.alt } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.5 })));
                p.text(&t, rc.0, rc.1, rc.2, rc.3, 14.0, Weight::Semibold, if on { pal.primary } else { pal.text }, Align::Center);
                self.colpo(rc, Azione::Pdf(PdfAz::NumFormato(k)), vista);
                fx += fw + 8.0;
            }
            y += h;
        } else if self.pdf.pannello == 4 {
            let h = 134.0;
            super::sollevato(p, pal, self.scuro(), x, y, pw, h, 18.0);
            p.text("Filigrana", x + 14.0, y + 10.0, pw - 100.0, 24.0, 16.0, Weight::Semibold, pal.text, Align::Left);
            p.text("Testo chiaro in diagonale su ogni pagina", x + 14.0, y + 34.0, pw - 100.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
            self.interruttore(p, x + pw - 66.0, y + 32.0, self.pdf.filigrana);
            self.colpo((x + pw - 76.0, y + 6.0, 72.0, 52.0), Azione::Pdf(PdfAz::FiligranaOn), vista);
            let t = self.pdf.testo_filigrana.clone();
            self.campo_testo(p, (x + 14.0, y + 68.0, pw - 28.0, 52.0), Campo::Filigrana, &t, "BOZZA", "", false, vista);
            y += h;
        }
        y
    }

    // ---- Comprimi ----

    fn corpo_comprimi(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        y += self.riga_file(p, x, y, w) + 12.0;
        y += self.etichetta(p, x, y, w, "Quanto");
        let livelli = [("Leggera", "Solo pulizia: le immagini restano come sono"), ("Equilibrata", "Immagini a 150 dpi, per lo schermo e la posta"), ("Forte", "Immagini a 100 dpi, il file più piccolo")];
        for (k, (nome, sotto)) in livelli.into_iter().enumerate() {
            let on = self.pdf.livello as usize == k;
            let righe = p.wrap_lines(sotto, 13.0, Weight::Regular, w - 110.0, 2).len().max(1);
            let rc = (x, y, w, 52.0 + 20.0 * righe as f32);
            let az = Azione::Pdf(PdfAz::Livello(k as u8));
            p.rrect(rc.0, rc.1, rc.2, rc.3, 18.0, Some(if on { pal.alt } else if self.premuto == Some(az) { mix(pal.surface, pal.primary, 0.08) } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.5 })));
            let cy = rc.1 + rc.3 / 2.0;
            p.circle(rc.0 + 28.0, cy, 11.0, None, Some((if on { pal.primary } else { pal.muted }, 2.0)));
            if on {
                p.circle(rc.0 + 28.0, cy, 6.0, Some(pal.primary), None);
            }
            p.text(nome, rc.0 + 52.0, rc.1 + 10.0, rc.2 - 110.0, 26.0, 17.0, Weight::Semibold, pal.text, Align::Left);
            p.text_wrap(sotto, rc.0 + 52.0, rc.1 + 38.0, rc.2 - 110.0, 13.0, Weight::Regular, pal.muted, 20.0, 2);
            for b in 0..3 {
                let bh = 10.0 + b as f32 * 6.0;
                p.rrect(rc.0 + rc.2 - 44.0 + b as f32 * 9.0, cy + 11.0 - bh, 6.0, bh, 2.0, Some(if b <= k { pal.primary } else { pal.border }), None);
            }
            self.colpo(rc, az, vista);
            y += rc.3 + 8.0;
        }
        y += 4.0;
        y += self.etichetta(p, x, y, w, "Anche");
        let (m, b) = (self.pdf.metadati, self.pdf.bn);
        y += self.riga_sw(p, x, y, w, "Togli i metadati", "Autore, programma, date", m, Azione::Pdf(PdfAz::Metadati), vista) + 8.0;
        y += self.riga_sw(p, x, y, w, "Scansioni in bianco e nero", "Le pagine quasi solo testo diventano nitide e leggere", b, Azione::Pdf(PdfAz::Bn), vista);
        y
    }

    /// Il file: miniatura, nome, pagine e peso.
    fn riga_file(&mut self, p: &mut Painter, x: f32, y: f32, w: f32) -> f32 {
        let pal = self.pal();
        let h = 76.0;
        super::sollevato(p, pal, self.scuro(), x, y, w, h, 18.0);
        p.rrect(x + 12.0, y + 10.0, 42.0, 56.0, 4.0, Some(BIANCO), Some((pal.border, 1.0)));
        if let Some(img) = self.miniatura.clone() {
            p.image(&img, x + 13.0, y + 11.0, 40.0, 54.0, 3.0, Fit::Contain);
        }
        if let Some(f) = self.file.clone() {
            p.text(&f.nome, x + 66.0, y + 12.0, w - 80.0, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
            let info = match self.pdf.totale {
                Some(1) => format!("PDF · 1 pagina · {}", super::peso(f.dim)),
                Some(n) => format!("PDF · {n} pagine · {}", super::peso(f.dim)),
                None => format!("PDF · {}", super::peso(f.dim)),
            };
            p.text(&info, x + 66.0, y + 38.0, w - 80.0, 22.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        }
        h
    }

    // ---- Excel ----

    fn corpo_excel(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        // Come arriva in Excel: una tabella d'esempio.
        let celle = [["", "A", "B", "C"], ["1", "Voce", "2025", "2026"], ["2", "Personale", "1.204.330,00", "1.251.900,00"], ["3", "Servizi", "842.115,40", "866.020,10"], ["4", "Utenze", "310.880,00", "295.400,00"]];
        let (rh, c0) = (26.0, 28.0);
        let cw = (w - c0) / 3.0;
        p.rrect(x, y, w, rh * 5.0, 10.0, Some(BIANCO), Some((pal.border, 1.0)));
        p.push_clip(x, y, w, rh * 5.0);
        for (r, riga) in celle.iter().enumerate() {
            for (c, t) in riga.iter().enumerate() {
                let (cx, cww) = if c == 0 { (x, c0) } else { (x + c0 + (c - 1) as f32 * cw, cw) };
                let cy = y + r as f32 * rh;
                let testa = r == 0 || c == 0;
                if testa {
                    p.fill(cx, cy, cww, rh, (0xED, 0xEA, 0xF1));
                } else if r == 1 {
                    p.fill(cx, cy, cww, rh, (0xF6, 0xF3, 0xF9));
                }
                p.fill(cx + cww - 1.0, cy, 1.0, rh, (0xE5, 0xE1, 0xEA));
                p.fill(cx, cy + rh - 1.0, cww, 1.0, (0xE5, 0xE1, 0xEA));
                let al = if testa { Align::Center } else if c > 1 && r > 1 { Align::Right } else { Align::Left };
                let peso = if r == 1 && c > 0 { Weight::Bold } else if testa { Weight::Semibold } else { Weight::Regular };
                p.text(t, cx + 5.0, cy, cww - 10.0, rh, 11.0, peso, if testa { (0x6C, 0x65, 0x78) } else { (0x24, 0x1A, 0x31) }, al);
            }
        }
        p.pop_clip();
        y += rh * 5.0 + 6.0;
        p.text("Così arriva in Excel: numeri veri, titoli in grassetto", x, y, w, 22.0, 13.0, Weight::Regular, pal.muted, Align::Center);
        y += 30.0;
        y += self.etichetta(p, x, y, w, "Pagine");
        let t = self.pdf.x_pagine.clone();
        self.campo_testo(p, (x, y, w, 52.0), Campo::ExcelPagine, &t, "Tutte", "es. 1-3,5", false, vista);
        y += 56.0;
        if let Err(e) = cedro::PageSel::parse(&t) {
            y += p.text_wrap(&maiuscola(&e), x + 2.0, y, w, 13.0, Weight::Regular, pal.bad, 18.0, 2) + 4.0;
        }
        y += 4.0;
        y += self.etichetta(p, x, y, w, "Fogli");
        self.pillole_in(p, (x, y, w, TOCCO), &["Un foglio per tabella", "Tutto in uno"], usize::from(!self.pdf.x_foglio), |i| Azione::Pdf(PdfAz::FoglioPerTabella(i == 0)), vista);
        y += TOCCO + 10.0;
        let s = &self.pdf;
        let righe = [
            ("Leggi le scansioni (OCR)", "Solo dove manca il testo", s.x_ocr, PdfAz::XOcr),
            ("Numeri e date come valori", "Si possono sommare e ordinare", s.x_valori, PdfAz::XValori),
            ("Riunisci le tabelle spezzate", "Quando continuano sulla pagina dopo", s.x_riunisci, PdfAz::XRiunisci),
            ("Anche il testo fuori dalle tabelle", "In un foglio a parte", s.x_fuori, PdfAz::XFuori),
        ];
        for (t, sotto, on, az) in righe {
            y += self.riga_sw(p, x, y, w, t, sotto, on, Azione::Pdf(az), vista) + 8.0;
        }
        y
    }

    // ---- Password ----

    pub(super) fn foglio_password(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let (ml, mt, mr, mb) = self.margini;
        p.rrect_a(-ml, -mt, w + ml + mr, h + mt + mb, 0.0, (10, 8, 16), 0.48);
        self.colpi.push((Forma::R((-ml, -mt, w + ml + mr, h + mt + mb)), Azione::Niente));
        let ampio = w > 700.0;
        let fw = if ampio { 520.0 } else { w };
        let fx = (w - fw) / 2.0;
        let errore = self.pdf.errore_pw.clone();
        let fh = 330.0 + if errore.is_some() { 26.0 } else { 0.0 };
        let fy = h - fh;
        p.rrect(fx, fy, fw, fh + mb + 30.0, 28.0, Some(pal.surface), None);
        self.colpi.push((Forma::R((fx, fy, fw, fh)), Azione::Niente));
        p.rrect(fx + fw / 2.0 - 20.0, fy + 10.0, 40.0, 5.0, 3.0, Some(pal.border), None);
        let (x, cw) = (fx + 20.0, fw - 40.0);
        let mut y = fy + 30.0;
        p.rrect(x, y, 56.0, 56.0, 18.0, Some(pal.alt), None);
        p.icon(I_LUCCHETTO, x + 14.0, y + 14.0, 28.0, pal.primary);
        p.text("Questo PDF ha una password", x + 70.0, y + 2.0, cw - 70.0, 30.0, 20.0, Weight::Semibold, pal.text, Align::Left);
        let nome = self.file.as_ref().map(|f| f.nome.clone()).unwrap_or_default();
        p.text(&nome, x + 70.0, y + 30.0, cw - 70.0, 22.0, 14.0, Weight::Regular, pal.muted, Align::Left);
        y += 70.0;
        y += p.text_wrap("Serve per aprirlo. Cedro la usa solo per questo lavoro e non la ricorda.", x, y, cw, 15.0, Weight::Regular, pal.muted, 21.0, 2) + 8.0;
        p.text("Password per aprire", x + 2.0, y, cw, 24.0, 14.0, Weight::Semibold, pal.muted, Align::Left);
        y += 26.0;
        let pw = self.pdf.password.clone();
        let vista = (fx, fy, fw, fh);
        let mostra = self.pdf.mostra_pw;
        self.campo_testo(p, (x, y, cw, 54.0), Campo::Password, &pw, "", "", !mostra, vista);
        let occhio = (x + cw - 52.0, y + 3.0, 48.0, 48.0);
        p.icon(I_OCCHIO, occhio.0 + 12.0, occhio.1 + 12.0, 24.0, if mostra { pal.primary } else { pal.muted });
        self.colpi.push((Forma::R(occhio), Azione::Pdf(PdfAz::MostraPw)));
        y += 62.0;
        if let Some(e) = errore {
            p.text(&maiuscola(&e), x + 2.0, y - 4.0, cw, 24.0, 14.0, Weight::Semibold, pal.bad, Align::Left);
            y += 26.0;
        }
        let bw = (cw - 10.0) * 0.43;
        let ann = (x, y, bw, 50.0);
        let a_ann = Azione::Pdf(PdfAz::Annulla);
        p.rrect(ann.0, ann.1, ann.2, ann.3, 25.0, Some(if self.premuto == Some(a_ann) { pal.alt } else { pal.surface }), Some((pal.primary, 2.0)));
        p.text("Annulla", ann.0, ann.1, ann.2, ann.3, 17.0, Weight::Semibold, pal.primary, Align::Center);
        self.colpi.push((Forma::R(ann), a_ann));
        let apri = (x + bw + 10.0, y, cw - bw - 10.0, 50.0);
        let pronta = !pw.is_empty() && !self.pdf.sbloccando;
        let testo = if self.pdf.sbloccando { "Apro…" } else { "Apri" };
        self.pulsante(p, apri, I_LUCCHETTO, testo, pronta.then_some(Azione::Pdf(PdfAz::Apri)));
    }
}

/// Come `maiuscola`, per le altre schermate.
pub(super) fn maiuscola_pub(s: &str) -> String {
    maiuscola(s)
}

/// I messaggi del motore cominciano in minuscolo: sullo schermo vanno da soli, con la maiuscola.
fn maiuscola(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Il testo d'esempio di un formato dei numeri di pagina.
fn numero(formato: u8, tot: u32) -> String {
    match formato {
        0 => "1".into(),
        2 => format!("1 / {tot}"),
        _ => format!("Pagina 1 di {tot}"),
    }
}

/// Quante pagine di `n` restano con la scelta scritta (tutte se è vuota o sbagliata).
fn conta_pagine(sel: &str, n: u32) -> u32 {
    match cedro::PageSel::parse(sel) {
        Ok(Some(s)) => (0..n as usize).filter(|i| s.contains(*i)).count() as u32,
        _ => n,
    }
}
