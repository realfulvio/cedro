//! Cedro 4 Labs, la finestra unica: l'elenco di tutti i file in coda (di ogni tipo), i filtri, il caso di
//! ogni file, i dati personali e le impronte. Qui c'è lo stato e la logica; come si vede sta in
//! `views::labsx`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::datipers::Dati;
use super::gfx::Rgb;
use super::model::{App, FileState, Lab};
use crate::engine::Kind;

/// Il tipo di un file come lo vede un laboratorio (non quello del motore): cosa dice la colonna «Tipo».
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tipo {
    Vetrino,
    Dicom,
    Immagine,
    Microscopia,
    Tabella,
    Audio,
    Office,
    Pdf,
    Video,
    Archivio,
}

impl Tipo {
    pub const TUTTI: [Tipo; 10] = [Tipo::Vetrino, Tipo::Dicom, Tipo::Immagine, Tipo::Microscopia, Tipo::Tabella, Tipo::Audio, Tipo::Office, Tipo::Pdf, Tipo::Video, Tipo::Archivio];

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Vetrino => "Vetrino",
            Tipo::Dicom => "DICOM",
            Tipo::Immagine => "Immagine",
            Tipo::Microscopia => "Microscopia",
            Tipo::Tabella => "Tabella",
            Tipo::Audio => "Audio",
            Tipo::Office => "Word/Office",
            Tipo::Pdf => "PDF",
            Tipo::Video => "Video",
            Tipo::Archivio => "Archivio",
        }
    }

    pub fn colore(self) -> Rgb {
        match self {
            Tipo::Vetrino => (0xA8, 0x3C, 0x78),
            Tipo::Dicom => (0x2F, 0x6F, 0xB3),
            Tipo::Immagine => (0x0A, 0x73, 0xA8),
            Tipo::Microscopia => (0x1F, 0x8F, 0x55),
            Tipo::Tabella => (0xA6, 0x62, 0x00),
            Tipo::Audio => (0x7B, 0x4F, 0xC9),
            Tipo::Office => (0x2B, 0x57, 0x9A),
            Tipo::Pdf => (0xC8, 0x32, 0x3C),
            Tipo::Video => (0x1E, 0x7A, 0x55),
            Tipo::Archivio => (0x58, 0x6B, 0x80),
        }
    }
}

/// Il filtro per stato nella barra di sinistra.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatoFil {
    DaFare,
    Fatti,
    Errori,
}

/// Cosa mostra il pannello di destra, una cosa alla volta.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pannello {
    /// I file scelti: cosa sono, che dati portano, cosa ci si può fare.
    #[default]
    Dettagli,
    /// Il comando scelto in alto, con le sue scelte e «Esegui».
    Azione,
    /// Le procedure salvate.
    Procedure,
}

/// Una cosa fatta a un file: quale comando, quando e dove è finito il risultato.
#[derive(Clone, Debug)]
pub struct Fatto {
    pub lab: Lab,
    pub when: u64,
    pub out: Option<PathBuf>,
}

/// Una procedura in corso: a che passo siamo e su quali file (quelli spuntati quando è partita).
#[derive(Clone, Debug)]
pub struct ProcCorso {
    pub proc: usize,
    pub passo: usize,
    pub files: Vec<PathBuf>,
    pub falliti: usize,
}

/// Il nome stabile di un comando, per le procedure salvate.
pub fn lab_id(lab: Lab) -> &'static str {
    match lab {
        Lab::Home => "Home",
        Lab::Immagini => "Immagini",
        Lab::Anonimizza => "Anonimizza",
        Lab::Figura => "Figura",
        Lab::Dati => "Dati",
        Lab::Referti => "Referti",
        Lab::Trascrivi => "Trascrivi",
        Lab::Converti => "Converti",
        Lab::Pdf => "Pdf",
        Lab::Video => "Video",
        Lab::Firma => "Firma",
    }
}

pub fn lab_da_id(id: &str) -> Option<Lab> {
    [Lab::Home, Lab::Immagini, Lab::Anonimizza, Lab::Figura, Lab::Dati, Lab::Referti, Lab::Trascrivi, Lab::Converti, Lab::Pdf, Lab::Video, Lab::Firma].into_iter().find(|l| lab_id(*l) == id)
}

/// Le pagine intere (firma e trascrizione) non hanno le scelte nel pannello: non si salvano come procedura.
pub fn lab_a_pagina(lab: Lab) -> bool {
    matches!(lab, Lab::Referti | Lab::Trascrivi)
}

/// L'impronta SHA-256 di un file: si calcola a parte, i file di un vetrino pesano gigabyte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sha {
    InCorso,
    Pronta(String),
    Non,
}

#[derive(Default)]
pub struct LabsX {
    pub pannello: Pannello,
    /// Il pannello di destra è chiuso: la tabella prende tutto lo spazio.
    pub panel_chiuso: bool,
    /// L'ultimo file toccato col clic: da lì parte la selezione con Maiusc.
    pub ancora: Option<usize>,
    /// Quante tappe sono fatte per ogni caso.
    pub tappe: HashMap<String, u8>,
    /// La scheda del caso scelto è aperta.
    pub scheda: bool,
    /// Il file di cui si sta leggendo l'errore.
    pub errore: Option<usize>,
    pub procedure: Vec<crate::caso::Procedura>,
    /// La procedura scelta nel pannello.
    pub proc_scelta: usize,
    pub proc_corso: Option<ProcCorso>,
    /// Il nome della procedura che si sta salvando (finestrella aperta se `Some`).
    pub proc_nome: Option<String>,
    pub tipo: Option<Tipo>,
    /// Filtro per caso: `Some("")` = solo i file senza caso.
    pub caso: Option<String>,
    pub stato: Option<StatoFil>,
    pub ricerca: String,
    pub registro_aperto: bool,
    pub aiuto: bool,
    /// L'anteprima grande del file evidenziato, sopra la finestra.
    pub anteprima: bool,
    /// Righe alte (guanti, schermo a sfioramento) invece che compatte.
    pub comode: bool,
    /// Il caso di ogni file; la stringa vuota è «nessun caso» deciso apposta.
    pub caso_di: HashMap<PathBuf, String>,
    /// I casi conosciuti su questo computer.
    pub casi: Vec<crate::caso::Caso>,
    /// Da quando (secondi dal 1970) il file è in coda.
    pub dalle: HashMap<PathBuf, u64>,
    pub sha: HashMap<PathBuf, Sha>,
    /// Cosa si è fatto del file, comando per comando, per la colonna «Già fatto».
    pub fatto: HashMap<PathBuf, Vec<Fatto>>,
    /// La finestrella del caso è stata aperta da «Assegna caso» → «Nuovo caso…»: alla conferma il caso va ai file scelti.
    pub assegna_dopo: bool,
}

impl LabsX {
    pub fn nuovo() -> LabsX {
        LabsX { casi: crate::caso::casi(), tappe: crate::caso::tappe(), procedure: crate::caso::procedure(), ..Default::default() }
    }
}

/// L'impronta SHA-256 di un file, in esadecimale.
pub fn sha256_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn ora_ora() -> u64 {
    crate::nametemplate::now().unix
}

impl App {
    /// Un file è entrato in coda: da quando, e il caso di adesso se c'è.
    pub(super) fn lx_nuovo_file(&mut self, path: &Path) {
        self.lx.dalle.entry(path.to_path_buf()).or_insert_with(ora_ora);
        // Il caso si decide quando il file entra: se più tardi si sceglie un altro caso, il file non cambia.
        let code = self.caso.as_ref().map(|c| c.code.clone()).unwrap_or_default();
        if !code.is_empty() {
            self.lx_tappa_almeno(&code, 2);
        }
        self.lx.caso_di.entry(path.to_path_buf()).or_insert(code);
    }

    /// Il tipo e la sigla da mostrare («SVS», «DICOM», «JPEG»…).
    pub fn lx_tipo(&self, i: usize) -> (Tipo, String) {
        let f = &self.files[i];
        let ext = crate::engine::extension(&f.path);
        let up = |e: &str| e.to_ascii_uppercase();
        let m = self.media_of(&f.path);
        let sigla = |e: &str| match e {
            "jpg" | "jpeg" | "jfif" => "JPEG".to_string(),
            "tif" | "tiff" => "TIFF".to_string(),
            "docx" | "doc" | "docm" => "Word".to_string(),
            "xlsx" | "xls" | "xlsm" | "ods" => "Excel".to_string(),
            "pptx" | "ppt" | "pptm" => "Office".to_string(),
            "" => "FILE".to_string(),
            e => up(e),
        };
        if f.voice_only {
            return (Tipo::Audio, sigla(&ext));
        }
        match f.kind {
            Kind::Image => {
                if m.is_some_and(|m| m.slide.is_some()) {
                    return (Tipo::Vetrino, sigla(&ext));
                }
                if m.is_some_and(|m| m.dicom.is_some()) || matches!(ext.as_str(), "dcm" | "dicom") {
                    return (Tipo::Dicom, "DICOM".into());
                }
                if m.is_some_and(|m| m.micro.is_some()) || matches!(ext.as_str(), "czi" | "nd2" | "lif" | "oib" | "oif" | "lsm") {
                    return (Tipo::Microscopia, sigla(&ext));
                }
                (Tipo::Immagine, sigla(&ext))
            }
            Kind::Pdf => (Tipo::Pdf, "PDF".into()),
            Kind::Doc => (Tipo::Office, sigla(&ext)),
            Kind::Text => {
                if crate::ui::model::foglio_di_calcolo(&f.path) {
                    (Tipo::Tabella, sigla(&ext))
                } else {
                    (Tipo::Tabella, up(&ext))
                }
            }
            Kind::Audio => (Tipo::Audio, sigla(&ext)),
            Kind::Video => (Tipo::Video, sigla(&ext)),
            Kind::Archive => (Tipo::Archivio, sigla(&ext)),
            Kind::Other => (Tipo::Archivio, sigla(&ext)),
        }
    }

    /// Cosa si sa dei dati personali del file (vuoto finché non è stato letto).
    pub fn lx_dati(&self, i: usize) -> Dati {
        self.files.get(i).and_then(|f| self.media_of(&f.path)).and_then(|m| m.dati.clone()).unwrap_or_default()
    }

    /// Il codice del caso di un file: quello scelto per lui, altrimenti il caso di adesso, altrimenti niente.
    pub fn lx_caso_di(&self, path: &Path) -> String {
        match self.lx.caso_di.get(path) {
            Some(c) => c.clone(),
            None => self.caso.as_ref().map(|c| c.code.clone()).unwrap_or_default(),
        }
    }

    /// I file della tabella: quelli che passano i filtri e la ricerca, nell'ordine della coda.
    pub fn lx_righe(&self) -> Vec<usize> {
        let q = self.lx.ricerca.trim().to_lowercase();
        (0..self.files.len())
            .filter(|&i| {
                let f = &self.files[i];
                if let Some(t) = self.lx.tipo {
                    if self.lx_tipo(i).0 != t {
                        return false;
                    }
                }
                if let Some(c) = &self.lx.caso {
                    if &self.lx_caso_di(&f.path) != c {
                        return false;
                    }
                }
                if let Some(s) = self.lx.stato {
                    let ok = match s {
                        StatoFil::DaFare => matches!(f.state, FileState::Waiting | FileState::Running),
                        StatoFil::Fatti => matches!(f.state, FileState::Done | FileState::Skipped),
                        StatoFil::Errori => f.state == FileState::Failed,
                    };
                    if !ok {
                        return false;
                    }
                }
                if !q.is_empty() {
                    let hay = format!("{} {} {}", f.name(), self.lx_caso_di(&f.path), self.lx_tipo(i).1).to_lowercase();
                    if !q.split_whitespace().all(|w| hay.contains(w)) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn lx_conta_tipi(&self) -> Vec<(Tipo, usize)> {
        let mut n: HashMap<Tipo, usize> = HashMap::new();
        for i in 0..self.files.len() {
            *n.entry(self.lx_tipo(i).0).or_default() += 1;
        }
        Tipo::TUTTI.into_iter().filter_map(|t| n.get(&t).map(|c| (t, *c))).collect()
    }

    /// I casi dei file in coda con quanti file hanno, e quanti non ne hanno (dal più usato).
    pub fn lx_conta_casi(&self) -> (Vec<(String, usize)>, usize) {
        let mut v: Vec<(String, usize)> = Vec::new();
        let mut senza = 0;
        for f in &self.files {
            let c = self.lx_caso_di(&f.path);
            if c.is_empty() {
                senza += 1;
            } else if let Some(e) = v.iter_mut().find(|(k, _)| *k == c) {
                e.1 += 1;
            } else {
                v.push((c, 1));
            }
        }
        v.sort_by(|a, b| b.1.cmp(&a.1));
        (v, senza)
    }

    /// (da fare, fatti, con errori).
    pub fn lx_conta_stati(&self) -> (usize, usize, usize) {
        let mut r = (0, 0, 0);
        for f in &self.files {
            match f.state {
                FileState::Waiting | FileState::Running => r.0 += 1,
                FileState::Done | FileState::Skipped => r.1 += 1,
                FileState::Failed => r.2 += 1,
            }
        }
        r
    }

    /// I file spuntati, di ogni tipo.
    pub fn lx_scelti(&self) -> Vec<usize> {
        (0..self.files.len()).filter(|&i| self.files[i].checked).collect()
    }

    /// Un comando prende questo file?
    pub fn lx_prende(&self, lab: Lab, i: usize) -> bool {
        let f = &self.files[i];
        match lab {
            Lab::Trascrivi => crate::trascrizione::transcribable(&f.path),
            _ if f.voice_only => false,
            Lab::Immagini | Lab::Figura => f.kind == Kind::Image,
            Lab::Anonimizza => matches!(f.kind, Kind::Image | Kind::Pdf | Kind::Doc),
            Lab::Dati => f.kind == Kind::Text,
            Lab::Referti => matches!(f.kind, Kind::Pdf | Kind::Doc),
            Lab::Pdf => f.kind == Kind::Pdf,
            Lab::Converti | Lab::Home | Lab::Video | Lab::Firma => !matches!(f.kind, Kind::Other),
        }
    }

    /// Quanti dei file spuntati prende un comando.
    pub fn lx_per_comando(&self, lab: Lab) -> usize {
        self.lx_scelti().into_iter().filter(|&i| self.lx_prende(lab, i)).count()
    }

    /// Il caso va ai file spuntati (o al file evidenziato se nessuno lo è); `None` = nessun caso.
    pub fn lx_assegna(&mut self, code: Option<&str>) {
        let mut idx = self.lx_scelti();
        if idx.is_empty() {
            idx.extend(self.selected);
        }
        let code = code.map(crate::caso::pulisci).unwrap_or_default();
        for i in idx {
            if let Some(f) = self.files.get(i) {
                self.lx.caso_di.insert(f.path.clone(), code.clone());
            }
        }
        if !code.is_empty() {
            self.lx_ricorda_caso(&code, "");
        }
    }

    /// Un caso nuovo entra tra quelli conosciuti; se c'è già, la nota si aggiorna (se ce n'è una).
    pub fn lx_ricorda_caso(&mut self, code: &str, note: &str) {
        let Some(c) = crate::caso::Caso::new(code, note) else { return };
        match self.lx.casi.iter_mut().find(|k| k.code == c.code) {
            Some(k) => {
                if !c.note.is_empty() {
                    k.note = c.note;
                }
            }
            None => self.lx.casi.push(c),
        }
        crate::caso::salva_casi(&self.lx.casi);
    }

    /// La nota del caso (la riga che lo spiega), se la si conosce.
    pub fn lx_nota_caso(&self, code: &str) -> String {
        self.lx.casi.iter().find(|c| c.code == code).map(|c| c.note.clone()).or_else(|| self.caso.as_ref().filter(|c| c.code == code).map(|c| c.note.clone())).unwrap_or_default()
    }

    /// Frecce su e giù: la selezione passa al file prima o dopo (con Maiusc si allarga).
    pub fn lx_muovi(&mut self, d: i32, shift: bool) {
        let rows = self.lx_righe();
        if rows.is_empty() {
            return;
        }
        let pos = self.selected.and_then(|s| rows.iter().position(|&r| r == s));
        let next = match pos {
            None => 0,
            Some(p) => (p as i32 + d).clamp(0, rows.len() as i32 - 1) as usize,
        };
        if shift && self.lx.ancora.is_none() {
            self.lx.ancora = self.selected;
        }
        self.lx_clic_riga(rows[next], shift, false);
        if shift {
            self.selected = Some(rows[next]);
        }
    }

    /// Spazio: aggiunge o toglie il file evidenziato dalla selezione.
    pub fn lx_spunta_evidenziato(&mut self) {
        if let Some(i) = self.selected {
            self.lx_clic_riga(i, false, true);
        }
    }

    /// Il comando scelto è una pagina intera (firma, trascrizione): non c'è il pannello.
    pub fn lx_pagina(&self) -> bool {
        lab_a_pagina(self.lab) && self.lx.pannello == Pannello::Azione && !self.lavori_aperti
    }

    /// «Pacchetto per consulto»: sceglie le immagini e i vetrini del caso e apre la voce «consulto» del comando
    /// «Immagini e vetrini» (copie senza nomi, ZIP con le impronte SHA-256).
    pub fn lx_pacchetto(&mut self, simple: bool) {
        let Some(code) = self.lx.caso.clone().filter(|c| !c.is_empty()) else { return };
        let idx: Vec<usize> = (0..self.files.len()).filter(|&i| self.lx_caso_di(&self.files[i].path) == code && self.lx_prende(Lab::Immagini, i)).collect();
        if idx.is_empty() {
            self.notice = Some(super::model::Notice { text: crate::i18n::lit(&crate::i18n::tr_string("In questo caso non ci sono immagini né vetrini da mettere nel pacchetto.")), error: true });
            return;
        }
        for (k, f) in self.files.iter_mut().enumerate() {
            f.checked = idx.contains(&k);
        }
        self.selected = idx.first().copied();
        self.lx.scheda = false;
        self.set_lab(Lab::Immagini, simple);
        self.set_action("consult");
        self.lx.pannello = Pannello::Azione;
        self.lx.panel_chiuso = false;
    }

    /// L'impronta del file evidenziato: si fa partire una volta, in un thread a parte.
    pub fn lx_chiedi_sha(&mut self) {
        let Some(path) = self.selected.and_then(|i| self.files.get(i)).map(|f| f.path.clone()) else { return };
        if self.lx.sha.contains_key(&path) {
            return;
        }
        self.lx.sha.insert(path.clone(), Sha::InCorso);
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let h = sha256_file(&path);
            let _ = tx.send(super::model::Event::Sha(path, h));
            wake();
        });
    }

    /// Frase corta per la barra di stato: «3 selezionati · 3,06 GB».
    pub fn lx_scelti_frase(&self) -> String {
        let n = self.lx_scelti().len();
        let bytes: u64 = self.lx_scelti().iter().filter_map(|&i| self.media_of(&self.files[i].path)).map(|m| m.bytes).sum();
        let peso = if bytes > 0 { format!(" · {}", super::model::human_size(bytes)) } else { String::new() };
        format!("{n} {}{peso}", if n == 1 { "selezionato" } else { "selezionati" })
    }

    /// Un file è stato lavorato dal comando di adesso: si ricorda per la colonna «Già fatto».
    pub(super) fn lx_segna_fatto(&mut self, path: &Path, out: Option<PathBuf>) {
        let lab = if self.lab == Lab::Home { Lab::Converti } else { self.lab };
        let when = ora_ora();
        let v = self.lx.fatto.entry(path.to_path_buf()).or_default();
        v.retain(|x| x.lab != lab);
        v.push(Fatto { lab, when, out });
        let caso = self.lx.caso_di.get(path).cloned().unwrap_or_default();
        if !caso.is_empty() {
            let min = match lab {
                Lab::Referti | Lab::Trascrivi => 4,
                Lab::Firma => 5,
                _ => 3,
            };
            self.lx_tappa_almeno(&caso, min);
        }
    }

    /// Le tappe fatte di un caso (da 0 a 6).
    pub fn lx_tappe_di(&self, code: &str) -> u8 {
        self.lx.tappe.get(code).copied().unwrap_or(1).min(crate::caso::TAPPE.len() as u8)
    }

    /// Un caso va avanti fino a quella tappa almeno (non torna mai indietro da solo).
    pub fn lx_tappa_almeno(&mut self, code: &str, n: u8) {
        if self.lx_tappe_di(code) < n || !self.lx.tappe.contains_key(code) {
            let v = self.lx_tappe_di(code).max(n);
            self.lx.tappe.insert(code.to_string(), v);
            crate::caso::salva_tappe(&self.lx.tappe);
        }
    }

    /// Un clic su una tappa: il caso è fatto fino a lì; un secondo clic la toglie.
    pub fn lx_tappa_clic(&mut self, code: &str, k: u8) {
        let fatte = self.lx_tappe_di(code);
        let nuove = if fatte == k + 1 { k } else { k + 1 };
        self.lx.tappe.insert(code.to_string(), nuove);
        crate::caso::salva_tappe(&self.lx.tappe);
    }

    /// Un clic su una riga della tabella, come in Esplora file: da solo sceglie quel file, con Ctrl lo aggiunge
    /// o lo toglie, con Maiusc sceglie tutto da dove si era partiti. La casella a sinistra fa lo stesso.
    pub fn lx_clic_riga(&mut self, i: usize, shift: bool, ctrl: bool) {
        if i >= self.files.len() {
            return;
        }
        let righe = self.lx_righe();
        if shift {
            let da = self.lx.ancora.filter(|a| righe.contains(a)).or(self.selected).unwrap_or(i);
            let (p, q) = (righe.iter().position(|&r| r == da).unwrap_or(0), righe.iter().position(|&r| r == i).unwrap_or(0));
            let (a, b) = (p.min(q), p.max(q));
            for f in self.files.iter_mut() {
                f.checked = false;
            }
            for &r in &righe[a..=b] {
                self.files[r].checked = true;
            }
        } else if ctrl {
            self.files[i].checked = !self.files[i].checked;
            self.lx.ancora = Some(i);
        } else {
            for (k, f) in self.files.iter_mut().enumerate() {
                f.checked = k == i;
            }
            self.lx.ancora = Some(i);
        }
        if self.files[i].checked {
            self.selected = Some(i);
        } else {
            self.selected = self.lx_scelti().last().copied();
        }
        self.preview_page = 0;
        self.stamp_page = 0;
    }

    /// Tutti i file della tabella (o nessuno, se erano già tutti scelti).
    pub fn lx_scegli_tutti(&mut self) {
        let righe = self.lx_righe();
        let tutti = !righe.is_empty() && righe.iter().all(|&i| self.files[i].checked);
        for f in self.files.iter_mut() {
            f.checked = false;
        }
        if !tutti {
            for &i in &righe {
                self.files[i].checked = true;
            }
            self.selected = righe.first().copied();
        }
    }

    /// «Togli» e Canc: toglie dalla coda (non dal disco) solo i file scelti nella tabella; senza scelti niente.
    pub fn lx_togli_scelti(&mut self) {
        if self.busy() {
            return;
        }
        let scelti = self.lx_scelti();
        if scelti.is_empty() {
            return;
        }
        let mut k = 0;
        self.files.retain(|_| {
            let tieni = !scelti.contains(&k);
            k += 1;
            tieni
        });
        self.selected = None;
        self.lx.ancora = None;
        self.lx.errore = None;
        self.prune_media_pub();
    }

    /// Quello che i filtri nascondono non resta scelto: il pannello e i comandi contano e lavorano solo le righe che si vedono.
    pub fn lx_pota(&mut self) {
        let righe = self.lx_righe();
        for (i, f) in self.files.iter_mut().enumerate() {
            if f.checked && !righe.contains(&i) {
                f.checked = false;
            }
        }
        if let Some(s) = self.selected {
            if !righe.contains(&s) {
                self.selected = self.files.iter().position(|f| f.checked);
            }
        }
    }

    /// Si sceglie un caso a sinistra: la tabella mostra i suoi file e i file nuovi entrano lì.
    /// `None` = tutti i file, `Some("")` = solo quelli senza caso: in tutti e due i casi i file nuovi non hanno caso.
    pub fn lx_scegli_caso(&mut self, code: Option<String>) {
        self.lx.caso = code.clone();
        self.lx.scheda = false;
        let code = code.unwrap_or_default();
        let nuovo = if code.is_empty() { None } else { crate::caso::Caso::new(&code, &self.lx_nota_caso(&code)) };
        if nuovo != self.caso {
            self.caso = nuovo;
            crate::caso::salva(self.caso.as_ref());
        }
        self.lx_pota();
    }

    /// Dei file scelti, quanti il comando non prende (restano fuori) e di che tipo.
    pub fn lx_restano_fuori(&self, lab: Lab) -> (usize, Vec<Tipo>) {
        let mut n = 0;
        let mut tipi: Vec<Tipo> = Vec::new();
        for i in self.lx_scelti() {
            if !self.lx_prende(lab, i) {
                n += 1;
                let t = self.lx_tipo(i).0;
                if !tipi.contains(&t) {
                    tipi.push(t);
                }
            }
        }
        (n, tipi)
    }

    /// I casi per la colonna di sinistra: quelli conosciuti e quelli dei file in coda, con quanti file hanno.
    pub fn lx_casi_elenco(&self) -> Vec<(String, String, usize)> {
        let (in_coda, _) = self.lx_conta_casi();
        let mut v: Vec<(String, String, usize)> = self.lx.casi.iter().map(|c| (c.code.clone(), c.note.clone(), in_coda.iter().find(|(k, _)| *k == c.code).map(|x| x.1).unwrap_or(0))).collect();
        for (c, n) in in_coda {
            if !v.iter().any(|x| x.0 == c) {
                let nota = self.lx_nota_caso(&c);
                v.push((c, nota, n));
            }
        }
        v
    }

    /// I lavori fatti per un caso, dal più nuovo.
    pub fn lx_lavori_del_caso(&self, code: &str) -> Vec<&crate::caso::Lavoro> {
        self.lavori.iter().rev().filter(|l| l.caso == code).collect()
    }

    // ------------------------------------------------------------ procedure

    /// Il passo che rifarebbe quello che si vede adesso: il comando, la voce e le sue scelte.
    pub fn lx_passo_attuale(&self) -> Option<crate::caso::Passo> {
        if lab_a_pagina(self.lab) || matches!(self.lab, Lab::Home | Lab::Video | Lab::Firma) {
            return None;
        }
        let mut p = crate::caso::Passo { lab: lab_id(self.lab).to_string(), azione: self.action.clone(), ..Default::default() };
        for def in self.option_defs() {
            let nome = format!("{:?}", def.opt);
            match def.control {
                crate::conversioni::Control::Password => {}
                crate::conversioni::Control::Text { .. } => p.testi.push((nome, self.values.text(def).to_string())),
                _ => p.nums.push((nome, self.values.get(def))),
            }
        }
        Some(p)
    }

    /// Che cosa dice un passo, in parole: «Togli i nomi · consulto» o «Salva immagini e vetrini · JPEG».
    pub fn lx_passo_titolo(&self, p: &crate::caso::Passo) -> (String, String) {
        let lab = lab_da_id(&p.lab).unwrap_or(Lab::Converti);
        let voce = self.voce(&p.azione);
        (crate::i18n::tr_string(lab.cedro_name()), crate::i18n::tr_string(voce.label))
    }

    /// Si ricorda quello che si vede adesso come procedura nuova (con quel nome).
    pub fn lx_salva_procedura(&mut self, nome: &str) -> bool {
        let nome = nome.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(passo) = self.lx_passo_attuale() else { return false };
        if nome.is_empty() {
            return false;
        }
        match self.lx.procedure.iter_mut().find(|p| p.nome == nome) {
            Some(p) => p.passi = vec![passo],
            None => self.lx.procedure.push(crate::caso::Procedura { nome, passi: vec![passo] }),
        }
        crate::caso::salva_procedure(&self.lx.procedure);
        self.lx.proc_scelta = self.lx.procedure.len().saturating_sub(1);
        true
    }

    pub fn lx_togli_procedura(&mut self, k: usize) {
        if k < self.lx.procedure.len() {
            self.lx.procedure.remove(k);
            crate::caso::salva_procedure(&self.lx.procedure);
            self.lx.proc_scelta = self.lx.proc_scelta.min(self.lx.procedure.len().saturating_sub(1));
        }
    }

    /// Un passo può lavorare almeno uno dei file scelti?
    fn lx_passo_prende(&self, p: &crate::caso::Passo, scelti: &[usize]) -> bool {
        lab_da_id(&p.lab).is_some_and(|lab| scelti.iter().any(|&i| self.lx_prende(lab, i)))
    }

    /// Quanti dei file scelti la procedura lavora (almeno con un passo).
    pub fn lx_procedura_su(&self, k: usize) -> usize {
        let Some(proc) = self.lx.procedure.get(k) else { return 0 };
        self.lx_scelti().into_iter().filter(|&i| proc.passi.iter().any(|p| lab_da_id(&p.lab).is_some_and(|l| self.lx_prende(l, i)))).count()
    }

    /// Fa partire la procedura sui file scelti: un passo dopo l'altro.
    pub fn lx_procedura_via(&mut self) {
        if self.busy() || self.lx.proc_corso.is_some() {
            return;
        }
        let k = self.lx.proc_scelta;
        let scelti = self.lx_scelti();
        let Some(proc) = self.lx.procedure.get(k) else { return };
        if !proc.passi.iter().any(|p| self.lx_passo_prende(p, &scelti)) {
            self.notice = Some(super::model::Notice { text: crate::i18n::lit(&crate::i18n::tr_string("Questa procedura non lavora nessuno dei file scelti.")), error: true });
            return;
        }
        let files = scelti.iter().map(|&i| self.files[i].path.clone()).collect();
        self.lx.proc_corso = Some(ProcCorso { proc: k, passo: 0, files, falliti: 0 });
        self.lx_procedura_passo();
    }

    /// Il passo di adesso: sceglie i file che prende, regola comando e opzioni e parte. Salta i passi senza file.
    pub fn lx_procedura_passo(&mut self) {
        loop {
            let Some(pc) = self.lx.proc_corso.clone() else { return };
            let Some(proc) = self.lx.procedure.get(pc.proc).cloned() else {
                self.lx.proc_corso = None;
                return;
            };
            let Some(passo) = proc.passi.get(pc.passo).cloned() else {
                // Finita: si rimettono le spunte com'erano.
                for f in self.files.iter_mut() {
                    f.checked = pc.files.contains(&f.path);
                }
                self.lx.proc_corso = None;
                let (n, nome) = (proc.passi.len(), proc.nome);
                let text = if pc.falliti > 0 {
                    format!("{} «{nome}»: {} {}.", crate::i18n::tr_string("Procedura finita con errori"), pc.falliti, crate::i18n::tr_string("file non riusciti"))
                } else {
                    format!("{} «{nome}»: {n} {}.", crate::i18n::tr_string("Procedura fatta"), crate::i18n::tr_string(if n == 1 { "passo" } else { "passi" }))
                };
                self.notice = Some(super::model::Notice { text: crate::i18n::lit(&text), error: pc.falliti > 0 });
                return;
            };
            let Some(lab) = lab_da_id(&passo.lab) else {
                self.lx.proc_corso = Some(ProcCorso { passo: pc.passo + 1, ..pc });
                continue;
            };
            for i in 0..self.files.len() {
                let dentro = pc.files.contains(&self.files[i].path);
                self.files[i].checked = dentro && self.lx_prende(lab, i);
            }
            if !self.files.iter().any(|f| f.checked) {
                self.lx.proc_corso = Some(ProcCorso { passo: pc.passo + 1, ..pc });
                continue;
            }
            let simple = self.simple_view;
            self.set_lab(lab, simple);
            self.set_action(&passo.azione);
            for def in self.option_defs() {
                let nome = format!("{:?}", def.opt);
                if let Some((_, v)) = passo.nums.iter().find(|(k, _)| *k == nome) {
                    self.values.set(def, *v);
                }
                if let Some((_, t)) = passo.testi.iter().find(|(k, _)| *k == nome) {
                    self.values.set_text(def, t);
                }
            }
            self.lx.pannello = Pannello::Azione;
            self.notice = None;
            self.start();
            if !self.busy() {
                // Non è partito (una scelta non valida, per esempio): la procedura si ferma qui e l'avviso dice perché.
                for f in self.files.iter_mut() {
                    f.checked = pc.files.contains(&f.path);
                }
                self.lx.proc_corso = None;
            }
            return;
        }
    }

    /// Un lavoro è finito: se c'era una procedura, passa al passo dopo.
    pub(super) fn lx_procedura_avanti(&mut self, falliti: usize) {
        if let Some(pc) = self.lx.proc_corso.as_mut() {
            pc.passo += 1;
            pc.falliti += falliti;
            self.lx_procedura_passo();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_impronta_di_un_file_piccolo() {
        let dir = std::env::temp_dir().join(format!("cedro-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert_eq!(sha256_file(&dir.join("non-c-e")), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn coda(n: usize) -> App {
        let mut app = App::new(std::sync::Arc::new(|| {}));
        let dir = std::env::temp_dir().join(format!("cedro-lx-coda-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for k in 0..n {
            let p = dir.join(format!("riga{k}.csv"));
            std::fs::write(&p, "a,b\n1,2\n").unwrap();
            paths.push(p);
        }
        app.add_files(paths, true);
        app
    }

    fn scelti(app: &App) -> Vec<usize> {
        (0..app.files.len()).filter(|&i| app.files[i].checked).collect()
    }

    #[test]
    fn la_scelta_e_una_sola_come_in_esplora_file() {
        let mut app = coda(5);
        // Da solo: sceglie quella riga e nessun'altra.
        app.lx_clic_riga(1, false, false);
        assert_eq!(scelti(&app), vec![1]);
        app.lx_clic_riga(3, false, false);
        assert_eq!(scelti(&app), vec![3]);
        // Ctrl aggiunge e toglie.
        app.lx_clic_riga(0, false, true);
        assert_eq!(scelti(&app), vec![0, 3]);
        app.lx_clic_riga(3, false, true);
        assert_eq!(scelti(&app), vec![0]);
        // Maiusc sceglie da dove si era partiti, anche all'indietro.
        app.lx_clic_riga(2, false, false);
        app.lx_clic_riga(4, true, false);
        assert_eq!(scelti(&app), vec![2, 3, 4]);
        app.lx_clic_riga(0, true, false);
        assert_eq!(scelti(&app), vec![0, 1, 2]);
        // «Scegli tutti» sceglie tutto, la seconda volta toglie.
        app.lx_scegli_tutti();
        assert_eq!(scelti(&app).len(), 5);
        app.lx_scegli_tutti();
        assert!(scelti(&app).is_empty());
    }

    #[test]
    fn i_filtri_tolgono_la_scelta_dalle_righe_nascoste() {
        let mut app = coda(4);
        for f in app.files.iter_mut() {
            f.checked = true;
        }
        assert_eq!(scelti(&app).len(), 4);
        // La ricerca nasconde due righe: restano scelte solo quelle che si vedono.
        app.lx.ricerca = "riga2".into();
        app.lx_pota();
        assert_eq!(scelti(&app), vec![2]);
        assert_eq!(app.selected, Some(2));
        // Una ricerca che non trova niente non lascia niente scelto.
        app.lx.ricerca = "zzz".into();
        app.lx_pota();
        assert!(scelti(&app).is_empty());
        assert_eq!(app.selected, None);
    }

    #[test]
    fn togli_toglie_solo_i_file_scelti() {
        let mut app = coda(4);
        for f in app.files.iter_mut() {
            f.checked = false;
        }
        // Senza scelti non si toglie niente, qualunque sia il comando aperto.
        app.set_lab(Lab::Anonimizza, false);
        app.lx_togli_scelti();
        assert_eq!(app.files.len(), 4);
        // Scelto solo il terzo: se ne va solo lui.
        app.lx_clic_riga(2, false, false);
        app.lx_togli_scelti();
        let nomi: Vec<String> = app.files.iter().map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(nomi, vec!["riga0.csv", "riga1.csv", "riga3.csv"]);
    }

    #[test]
    fn un_passo_ricorda_il_comando_e_la_voce() {
        let mut app = coda(1);
        app.set_lab(Lab::Anonimizza, false);
        let p = app.lx_passo_attuale().expect("Togli i nomi si può salvare");
        assert_eq!(p.lab, lab_id(Lab::Anonimizza));
        assert_eq!(p.azione, app.action);
        // Le pagine intere non sono procedure.
        app.set_lab(Lab::Trascrivi, false);
        assert!(app.lx_passo_attuale().is_none());
        assert_eq!(lab_da_id(lab_id(Lab::Anonimizza)), Some(Lab::Anonimizza));
    }

    #[test]
    fn i_tipi_hanno_un_nome_e_un_colore_ciascuno() {
        let mut colori: Vec<Rgb> = Tipo::TUTTI.iter().map(|t| t.colore()).collect();
        colori.sort();
        colori.dedup();
        assert_eq!(colori.len(), Tipo::TUTTI.len());
    }
}
