//! Cedro 4 Labs, «Converti dati e piastre»: lo stato della finestra dei dati degli strumenti. Il file
//! di uno strumento (lettore di piastre, qPCR, spettrofotometro, contacellule…) si riconosce dal
//! contenuto; al centro si vede la piastra con i valori veri e i ruoli, a destra si sceglie cosa esce.
//! Qui c'è solo lo stato e quello che fanno i comandi; si disegna in `views/strumenti.rs`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::model::{App, Event, Lab};
use crate::strumenti::{self, Lettura, Ruolo, Scelte};

/// Le schede del centro.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Vista {
    Piastra,
    Elenco,
    Strumento,
    Allotrope,
}

impl Vista {
    pub fn nome(self) -> &'static str {
        match self {
            Vista::Piastra => "Piastra",
            Vista::Elenco => "Elenco",
            Vista::Strumento => "Strumento",
            Vista::Allotrope => "Allotrope",
        }
    }
}

/// Le tre scelte di «Semplice».
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Pronta {
    Excel,
    Csv,
    Allotrope,
}

/// Le uscite di «Completa».
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Uscita {
    Excel,
    Csv,
    Allotrope,
    Piatto,
}

/// Quello che fanno i comandi della finestra.
#[derive(Clone, PartialEq, Debug)]
pub enum StrumAct {
    Vista(Vista),
    /// Un'altra piastra (o gruppo) della stessa lettura.
    Gruppo(usize),
    /// Il riquadro dei pozzetti: si preme e si trascina (`HitKind::Slider`).
    Piastra,
    /// Tutti i pozzetti di una riga o di una colonna.
    Riga(usize),
    Colonna(usize),
    Tutti,
    Nessuno,
    /// Il ruolo dei pozzetti scelti: B, S, C, R o `-` (campione).
    Ruolo(char),
    Daltonici(bool),
    MenoBianco(bool),
    Pronta(Pronta),
    Esce(Uscita),
    Calcoli,
    Percorso(bool),
    Controlla,
    /// Apre l'elenco degli strumenti per rileggere il file con un altro profilo.
    CambiaStrumento,
    /// Rilegge il file con questo profilo («» = quello riconosciuto).
    Profilo(String),
}

/// Quello che si ricorda di ogni file.
#[derive(Clone, Debug)]
pub struct PerFile {
    /// La lettura rifatta con uno strumento scelto a mano, o l'errore.
    pub riletto: Option<Result<Arc<Lettura>, String>>,
    pub vista: Vista,
    pub gruppo: usize,
    /// I pozzetti scelti (riga, colonna) sulla piastra mostrata.
    pub sel: BTreeSet<(usize, usize)>,
    /// I ruoli segnati sui pozzetti: (pozzetto, lettera).
    pub ruoli: Vec<(String, char)>,
    /// L'identificativo del profilo scelto a mano; vuoto = riconosciuto.
    pub profilo: String,
    /// Si sta rileggendo il file.
    pub rileggendo: bool,
}

impl Default for PerFile {
    fn default() -> PerFile {
        PerFile { riletto: None, vista: Vista::Piastra, gruppo: 0, sel: BTreeSet::new(), ruoli: Vec::new(), profilo: String::new(), rileggendo: false }
    }
}

/// Lo stato della finestra: le scelte comuni a tutti i file e quelle di ognuno.
#[derive(Clone, Debug)]
pub struct Stato {
    pub files: HashMap<PathBuf, PerFile>,
    /// Le uscite e le opzioni: valgono per tutti i file (i ruoli e lo strumento sono di ogni file).
    pub scelte: Scelte,
    /// In «Semplice» si sceglie una cosa sola.
    pub pronta: Pronta,
    /// Il pozzetto da cui parte un trascinamento (riga, colonna), e se il mouse si è mosso da lì.
    pub drag: Option<(usize, usize)>,
    pub mosso: bool,
    /// Il pozzetto sotto il mouse, per la riga in basso.
    pub sopra: Option<(usize, usize)>,
}

impl Default for Stato {
    fn default() -> Stato {
        Stato { files: HashMap::new(), scelte: Scelte::default(), pronta: Pronta::Excel, drag: None, mosso: false, sopra: None }
    }
}

/// La lettura che si mostra: quella rifatta a mano se c'è, altrimenti quella riconosciuta.
impl App {
    /// La lettura di uno strumento per un file, se la pagina è «Converti dati e piastre» e il file è l'export di uno strumento.
    pub fn strum_lettura(&self, path: &Path) -> Option<Arc<Lettura>> {
        if self.lab != Lab::Dati {
            return None;
        }
        self.strum_lettura_qualsiasi(path)
    }

    fn strum_lettura_qualsiasi(&self, path: &Path) -> Option<Arc<Lettura>> {
        if let Some(Ok(l)) = self.strum.files.get(path).and_then(|f| f.riletto.as_ref()) {
            return Some(l.clone());
        }
        self.media.get(path).and_then(|m| m.strumento.clone())
    }

    /// Il file in evidenza, se è l'export di uno strumento (e allora la pagina cambia faccia).
    pub fn strum_corrente(&self) -> Option<(usize, Arc<Lettura>)> {
        let vis = self.visible();
        let i = self.selected.filter(|i| vis.contains(i)).or(vis.first().copied())?;
        Some((i, self.strum_lettura(&self.files[i].path)?))
    }

    /// Lo stato del file in evidenza.
    pub fn strum_file(&self, path: &Path) -> PerFile {
        self.strum.files.get(path).cloned().unwrap_or_default()
    }

    fn strum_file_mut(&mut self, path: &Path) -> &mut PerFile {
        self.strum.files.entry(path.to_path_buf()).or_default()
    }

    /// Le scelte per un file: le uscite comuni, più i suoi ruoli e il suo strumento.
    pub fn strum_scelte_per(&self, path: &Path) -> Scelte {
        let mut s = self.strum.scelte.clone();
        // In «Semplice» vale la scelta pronta, non le caselle di «Completa».
        if self.simple_view {
            let p = self.strum.pronta;
            s.excel = p != Pronta::Csv;
            s.csv = p == Pronta::Csv;
            s.asm = p == Pronta::Allotrope;
            s.piatto = false;
        }
        if let Some(f) = self.strum.files.get(path) {
            s.ruoli = f.ruoli.clone();
            s.profilo = f.profilo.clone();
        }
        s
    }

    /// Una chiave che cambia quando cambia quello che esce: serve a rifare i file già fatti.
    pub fn strum_chiave(&self) -> String {
        if self.lab != Lab::Dati {
            return String::new();
        }
        let mut v: Vec<String> = self.files.iter().filter(|f| self.strum_lettura(&f.path).is_some()).map(|f| format!("{}:{:?}", f.path.display(), self.strum_scelte_per(&f.path))).collect();
        v.sort();
        v.join("|")
    }

    /// Il gruppo mostrato (limitato a quelli che ci sono).
    pub fn strum_gruppo(&self, path: &Path, l: &Lettura) -> usize {
        self.strum.files.get(path).map(|f| f.gruppo).unwrap_or(0).min(l.gruppi.len().saturating_sub(1))
    }

    /// Chiede di rileggere il file con un altro profilo, in fondo.
    pub fn strum_rileggi(&mut self, path: &Path, profilo: &str) {
        let f = self.strum_file_mut(path);
        f.profilo = profilo.to_string();
        f.sel.clear();
        f.ruoli.clear();
        f.gruppo = 0;
        if profilo.is_empty() {
            f.riletto = None;
            f.rileggendo = false;
            return;
        }
        f.rileggendo = true;
        self.loader.strumento(path, profilo);
    }

    /// È arrivata la lettura con il profilo scelto a mano.
    pub fn strum_riletto(&mut self, path: &Path, profilo: &str, esito: Result<Arc<Lettura>, String>) {
        let f = self.strum_file_mut(path);
        // Nel frattempo si è scelto un altro profilo: questa risposta è vecchia.
        if f.profilo != profilo {
            return;
        }
        f.rileggendo = false;
        f.riletto = Some(esito);
    }

    /// Un comando della finestra.
    pub fn strum_act(&mut self, a: StrumAct) {
        let Some((i, l)) = self.strum_corrente() else { return };
        let path = self.files[i].path.clone();
        match a {
            StrumAct::Vista(v) => self.strum_file_mut(&path).vista = v,
            StrumAct::Gruppo(g) => {
                let f = self.strum_file_mut(&path);
                f.gruppo = g.min(l.gruppi.len().saturating_sub(1));
                f.sel.clear();
            }
            StrumAct::Piastra => {}
            StrumAct::Riga(r) => {
                let (_, c) = l.formato(self.strum_gruppo(&path, &l));
                let g = self.strum_gruppo(&path, &l);
                let f = self.strum_file_mut(&path);
                f.sel = (0..c).filter(|&k| ha_pozzetto(&l, g, r, k)).map(|k| (r, k)).collect();
            }
            StrumAct::Colonna(c) => {
                let g = self.strum_gruppo(&path, &l);
                let (r, _) = l.formato(g);
                let f = self.strum_file_mut(&path);
                f.sel = (0..r).filter(|&k| ha_pozzetto(&l, g, k, c)).map(|k| (k, c)).collect();
            }
            StrumAct::Tutti => {
                let g = self.strum_gruppo(&path, &l);
                let (r, c) = l.formato(g);
                let f = self.strum_file_mut(&path);
                f.sel = (0..r).flat_map(|a| (0..c).map(move |b| (a, b))).filter(|&(a, b)| ha_pozzetto(&l, g, a, b)).collect();
            }
            StrumAct::Nessuno => self.strum_file_mut(&path).sel.clear(),
            StrumAct::Ruolo(c) => {
                let ruolo = strumenti::scelte::ruolo_da_lettera(c).unwrap_or(Ruolo::Campione);
                let f = self.strum_file_mut(&path);
                let scelti: Vec<(usize, usize)> = f.sel.iter().copied().collect();
                let mut s = Scelte { ruoli: std::mem::take(&mut f.ruoli), ..Scelte::default() };
                for (r, k) in scelti {
                    s.segna(&strumenti::pozzetto_nome(r, k), ruolo);
                }
                f.ruoli = s.ruoli;
            }
            StrumAct::Daltonici(d) => self.strum.scelte.daltonici = d,
            StrumAct::MenoBianco(m) => self.strum.scelte.meno_bianco = m,
            StrumAct::Pronta(p) => self.strum.pronta = p,
            StrumAct::Esce(u) => {
                let s = &mut self.strum.scelte;
                match u {
                    Uscita::Excel => s.excel = !s.excel,
                    Uscita::Csv => s.csv = !s.csv,
                    Uscita::Allotrope => {
                        s.asm = !s.asm;
                        // Il CSV piatto è di Allotrope: spento lui, spento anche quello. La scheda sparisce con lui.
                        if !s.asm {
                            s.piatto = false;
                        }
                    }
                    Uscita::Piatto => s.piatto = !s.piatto,
                }
                if !s.asm && self.strum.files.get(&path).is_some_and(|f| f.vista == Vista::Allotrope) {
                    self.strum_file_mut(&path).vista = Vista::Piastra;
                }
            }
            StrumAct::Calcoli => self.strum.scelte.asm_calcoli = !self.strum.scelte.asm_calcoli,
            StrumAct::Percorso(p) => self.strum.scelte.asm_percorso = p,
            StrumAct::Controlla => self.strum.scelte.asm_controlla = !self.strum.scelte.asm_controlla,
            StrumAct::CambiaStrumento => {}
            StrumAct::Profilo(id) => self.strum_rileggi(&path, &id),
        }
    }

    /// Il mouse preme sulla piastra: parte la scelta di un pozzetto (o di un rettangolo, se si trascina).
    pub fn strum_premi(&mut self, p: (usize, usize)) {
        self.strum.drag = Some(p);
        self.strum.mosso = false;
    }

    /// Il mouse si muove tenendo premuto: se esce dal pozzetto di partenza è un trascinamento.
    pub fn strum_muovi(&mut self, p: (usize, usize)) {
        let Some(a) = self.strum.drag else { return };
        if p != a || self.strum.mosso {
            self.strum.mosso = true;
            self.strum_trascina(a, p);
        }
    }

    /// Il mouse è rilasciato: se non si è mosso è un clic su un pozzetto.
    pub fn strum_rilascia(&mut self) {
        if let Some(a) = self.strum.drag.take() {
            if !self.strum.mosso {
                self.strum_clic(a);
            }
        }
        self.strum.mosso = false;
    }

    /// Il mouse si trascina sulla piastra: la scelta è il rettangolo da dove è partito a qui.
    pub fn strum_trascina(&mut self, a: (usize, usize), b: (usize, usize)) {
        let Some((i, l)) = self.strum_corrente() else { return };
        let path = self.files[i].path.clone();
        let g = self.strum_gruppo(&path, &l);
        let (r0, r1) = (a.0.min(b.0), a.0.max(b.0));
        let (c0, c1) = (a.1.min(b.1), a.1.max(b.1));
        let f = self.strum_file_mut(&path);
        f.sel = (r0..=r1).flat_map(|r| (c0..=c1).map(move |c| (r, c))).filter(|&(r, c)| ha_pozzetto(&l, g, r, c)).collect();
    }

    /// Un clic fermo su un pozzetto: lo sceglie o lo toglie dalla scelta.
    pub fn strum_clic(&mut self, p: (usize, usize)) {
        let Some((i, l)) = self.strum_corrente() else { return };
        let path = self.files[i].path.clone();
        let g = self.strum_gruppo(&path, &l);
        if !ha_pozzetto(&l, g, p.0, p.1) {
            return;
        }
        let f = self.strum_file_mut(&path);
        if !f.sel.remove(&p) {
            f.sel.insert(p);
        }
    }
}

/// Il pozzetto (riga, colonna) ha una misura nel gruppo.
pub fn ha_pozzetto(l: &Lettura, g: usize, r: usize, c: usize) -> bool {
    pozzetto_di(l, g, r, c).is_some()
}

/// La misura del pozzetto (riga, colonna) del gruppo.
pub fn pozzetto_di(l: &Lettura, g: usize, r: usize, c: usize) -> Option<&strumenti::Misura> {
    l.gruppi.get(g)?.misure.iter().find(|m| !m.e_spettro() && m.pozzetto.as_deref().and_then(strumenti::pozzetto_rc) == Some((r, c)))
}

/// Legge un file di strumento, se lo è: lo fa il thread dei file, non quello della finestra.
pub fn leggi(path: &Path, bytes: u64) -> Option<Arc<Lettura>> {
    // Gli export degli strumenti sono di qualche MB: un testo enorme non si prova neppure.
    if bytes > 48 << 20 {
        return None;
    }
    let ext = crate::engine::extension(path);
    if !matches!(ext.as_str(), "csv" | "tsv" | "txt" | "xlsx" | "xlsm" | "xls" | "zip") {
        return None;
    }
    strumenti::leggi_file(path).ok().map(Arc::new)
}

/// L'evento per la rilettura con un profilo scelto.
pub fn rileggi_evento(path: &Path, profilo: &str) -> Event {
    let esito = strumenti::Fonte::apri(path).and_then(|f| strumenti::scelte::leggi_scelto(&f, profilo)).map(Arc::new);
    Event::Strumento(path.to_path_buf(), profilo.to_string(), esito)
}
