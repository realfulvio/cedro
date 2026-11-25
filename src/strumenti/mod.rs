//! I file degli strumenti di laboratorio: lettori di piastre, qPCR e PCR digitale, spettrofotometri
//! e fluorimetri, contacellule, multiplex. Cedro riconosce lo strumento dal contenuto del file, ne
//! legge le misure con i dati della lettura e le scrive in un Excel pulito (Mappa, Pozzetti,
//! Strumento) e, a richiesta, in Allotrope (ASM, lo standard dei dati di laboratorio) e in un CSV
//! con una riga per misura. I valori dello strumento non si toccano: quello che calcola Cedro sta
//! sempre a parte e lo dice.
//!
//! I profili seguono quelli di allotropy (Benchling, licenza MIT), la libreria di riferimento per
//! Allotrope, e si provano sui suoi file d'esempio pubblici: vedi `tests/strumenti.rs`.

pub mod asm;
mod calcdocs;
pub mod excel;
pub mod fonte;
mod profili;
pub mod scelte;
mod tempo;

use std::path::Path;

pub use fonte::Fonte;
pub use scelte::Scelte;

/// La tecnica, cioè quale schema di Allotrope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tecnica {
    LettorePiastre,
    Qpcr,
    Dpcr,
    Spettrofotometria,
    Contacellule,
    Multiplex,
}

impl Tecnica {
    /// Il nome per le persone.
    pub fn nome(self) -> &'static str {
        match self {
            Tecnica::LettorePiastre => "Lettore di piastre",
            Tecnica::Qpcr => "qPCR",
            Tecnica::Dpcr => "PCR digitale",
            Tecnica::Spettrofotometria => "Spettrofotometria",
            Tecnica::Contacellule => "Conta cellule",
            Tecnica::Multiplex => "Multiplex",
        }
    }
}

/// Il ruolo di un pozzetto o di un campione.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Ruolo {
    #[default]
    Campione,
    Bianco,
    Standard,
    Controllo,
    ControlloPositivo,
    ControlloNegativo,
    Riferimento,
}

impl Ruolo {
    /// Il ruolo nello schema di Allotrope.
    pub fn asm(self) -> &'static str {
        match self {
            Ruolo::Campione => "unknown sample role",
            Ruolo::Bianco => "blank role",
            Ruolo::Standard => "standard sample role",
            Ruolo::Controllo | Ruolo::ControlloPositivo | Ruolo::ControlloNegativo => "control sample role",
            Ruolo::Riferimento => "reference sample role",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Ruolo::Campione => "campione",
            Ruolo::Bianco => "bianco",
            Ruolo::Standard => "standard",
            Ruolo::Controllo => "controllo",
            Ruolo::ControlloPositivo => "controllo positivo",
            Ruolo::ControlloNegativo => "controllo negativo",
            Ruolo::Riferimento => "riferimento",
        }
    }

    /// La lettera sul pozzetto.
    pub fn lettera(self) -> &'static str {
        match self {
            Ruolo::Campione => "",
            Ruolo::Bianco => "B",
            Ruolo::Standard => "S",
            Ruolo::Controllo | Ruolo::ControlloPositivo | Ruolo::ControlloNegativo => "C",
            Ruolo::Riferimento => "R",
        }
    }

    /// Il ruolo scritto dagli strumenti («Standard», «NTC», «Blank», «Unknown»…).
    pub fn da_testo(t: &str) -> Option<Ruolo> {
        let t = t.trim().to_ascii_lowercase();
        let t = t.as_str();
        Some(match t {
            "" => return None,
            "unknown" | "unkn" | "sample" | "samples" | "unk" | "u" | "campione" => Ruolo::Campione,
            "blank" | "blk" | "bianco" | "b" | "background" => Ruolo::Bianco,
            "standard" | "std" | "s" | "stds" => Ruolo::Standard,
            "ntc" | "negative control" | "neg" | "negative" | "nc" | "neg ctrl" | "negctrl" | "n" => Ruolo::ControlloNegativo,
            "positive control" | "pos" | "positive" | "pc" | "pos ctrl" | "posctrl" | "ipc" => Ruolo::ControlloPositivo,
            "control" | "ctrl" | "controllo" | "c" | "nac" | "qc" => Ruolo::Controllo,
            "reference" | "ref" => Ruolo::Riferimento,
            _ if t.starts_with("std") || t.starts_with("standard") => Ruolo::Standard,
            _ if t.starts_with("blank") || t.starts_with("blk") => Ruolo::Bianco,
            _ if t.starts_with("ntc") || t.starts_with("neg") => Ruolo::ControlloNegativo,
            _ if t.starts_with("pos") => Ruolo::ControlloPositivo,
            _ if t.starts_with("ctrl") || t.starts_with("control") => Ruolo::Controllo,
            _ => return None,
        })
    }
}

/// Dove sta un valore nel file Allotrope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posto {
    /// Nel documento della misura (assorbanza, fluorescenza…).
    Misura,
    /// Nei dati elaborati dallo strumento (Ct, concentrazioni, vitalità…).
    Elaborato,
    /// Nelle impostazioni della lettura (lunghezza d'onda, filtri…).
    Impostazione,
    /// Nelle impostazioni dell'elaborazione dello strumento (soglia, fattori…).
    Elaborazione,
    /// Nel documento del campione.
    Campione,
    /// Note dello strumento che lo schema non ha (custom information della misura).
    Altro,
    /// Note sul campione (custom information del documento del campione).
    CampioneAltro,
    /// Note sulle impostazioni (custom information del documento di controllo dello strumento).
    ImpostazioneAltro,
    /// Note sui dati elaborati (custom information del documento dei dati elaborati).
    ElaboratoAltro,
    /// Note sulle impostazioni dell'elaborazione (custom information del documento «data processing»).
    ElaborazioneAltro,
    /// Nel documento dell'immagine (contacellule): per esempio il nome del file dei dati.
    Immagine,
    /// Note sulle immagini (custom information del documento dell'immagine).
    ImmagineAltro,
}

/// Un valore: numero con unità, testo, o assente (lo schema lo vuole ma il file non lo ha).
#[derive(Clone, Debug, PartialEq)]
pub enum Dato {
    Numero(f64),
    Testo(String),
    /// Sì o no (impostazioni come «soglia automatica»).
    Bool(bool),
    /// Manca nel file: in Allotrope -0 con la nota, in Excel la cella vuota o la parola dello strumento.
    Assente(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Valore {
    /// La chiave di Allotrope («absorbance», «cycle threshold result (qPCR)»…) o il nome dello strumento per `Posto::Altro`.
    pub chiave: String,
    pub dato: Dato,
    pub unita: String,
    pub posto: Posto,
}

impl Valore {
    pub fn num(chiave: &str, v: f64, unita: &str, posto: Posto) -> Valore {
        Valore { chiave: chiave.into(), dato: Dato::Numero(v), unita: unita.into(), posto }
    }
    pub fn testo(chiave: &str, t: &str, posto: Posto) -> Valore {
        Valore { chiave: chiave.into(), dato: Dato::Testo(t.into()), unita: String::new(), posto }
    }
    pub fn bool(chiave: &str, v: bool, posto: Posto) -> Valore {
        Valore { chiave: chiave.into(), dato: Dato::Bool(v), unita: String::new(), posto }
    }
    pub fn numero(&self) -> Option<f64> {
        match self.dato {
            Dato::Numero(v) => Some(v),
            _ => None,
        }
    }
}

/// Che cosa sono i punti di una curva di un pozzetto: la cinetica (un valore ogni tanti secondi) o
/// uno spettro (un valore per lunghezza d'onda). Per Allotrope è un cubo di dati.
#[derive(Clone, Debug, PartialEq)]
pub struct Curva {
    /// La chiave del cubo nello schema («absorption profile data cube»).
    pub chiave: String,
    /// L'etichetta del cubo («600.0», «absorbance-spectrum»).
    pub etichetta: String,
    /// Cinetica (il tempo in secondi sull'asse) o spettro (le lunghezze d'onda in nm).
    pub cinetica: bool,
    /// Il concetto del valore («absorbance», «fluorescence», «luminescence»).
    pub concetto: String,
    /// L'unità come la scrive lo strumento («OD», «RFU», «RLU»).
    pub unita: String,
}

impl Curva {
    /// Il valore della curva che si mostra sulla piastra: l'ultimo punto della cinetica, il massimo dello spettro.
    pub fn riassunto(&self, punti: &[(f64, f64)]) -> Option<f64> {
        let validi = punti.iter().map(|(_, y)| *y).filter(|y| y.is_finite());
        if self.cinetica {
            punti.iter().rev().map(|(_, y)| *y).find(|y| y.is_finite())
        } else {
            validi.fold(None, |m: Option<f64>, y| Some(m.map_or(y, |m| m.max(y))))
        }
    }

    /// Il nome di quel valore per chi legge.
    pub fn nome_riassunto(&self) -> &'static str {
        if self.cinetica {
            "ultimo punto della cinetica"
        } else {
            "massimo dello spettro"
        }
    }
}

/// Una misura: un pozzetto letto una volta, oppure un campione.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Misura {
    pub id: String,
    /// Il pozzetto (A1, H12) se c'è una piastra.
    pub pozzetto: Option<String>,
    pub piastra: Option<String>,
    pub campione: String,
    pub ruolo: Ruolo,
    /// Il ruolo è scritto nel file (altrimenti è quello di base).
    pub ruolo_dal_file: bool,
    /// Bersaglio (qPCR, PCR digitale) o analita.
    pub bersaglio: Option<String>,
    /// L'identificativo del posto come lo scrive lo strumento quando non è il nome del pozzetto
    /// (QuantStudio numera i pozzetti: «1» è A1).
    pub luogo: Option<String>,
    /// Data e ora ISO della misura, se il file la dice.
    pub quando: Option<String>,
    pub valori: Vec<Valore>,
    /// Le note dello strumento sulla misura (errori, «Undetermined», valori fuori scala).
    pub note: Vec<(String, String)>,
    /// Lo spettro di assorbanza (lunghezza d'onda in nm, assorbanza in unità di assorbanza), per gli
    /// spettrofotometri che lo scrivono. Una misura con lo spettro e nessun valore è lo spettro e basta.
    pub spettro: Vec<(f64, f64)>,
    /// Che cosa sono i punti di `spettro` quando non sono uno spettro di assorbanza dei spettrofotometri
    /// (la cinetica di un pozzetto, lo spettro di fluorescenza…). Un punto senza valore è NaN.
    pub curva: Option<Curva>,
    /// I picchi che lo strumento ha trovato (diffusione dinamica della luce): per ogni picco le sue grandezze
    /// (diametro medio, intensità, massa…) come valori «Altro» con la loro unità.
    pub picchi: Vec<Vec<Valore>>,
    /// I cubi di dati della misura (la fluorescenza di ogni partizione della PCR digitale): per Allotrope.
    pub cubi: Vec<Cubo>,
    /// Gli analiti della misura (multiplex a sfere: Bio-Plex, xPONENT, IntelliFlex): il pozzetto è letto una
    /// volta e dà una riga di statistiche per ogni analita.
    pub analiti: Vec<Analita>,
}

/// Un analita di un pozzetto multiplex: la sfera, quante ne ha contate lo strumento e le statistiche della fluorescenza.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analita {
    /// L'identificativo per Allotrope (anche fonte dei calcoli dello strumento).
    pub id: String,
    pub nome: String,
    /// L'identificativo della sfera (la regione, il codice della microsfera).
    pub sfera: Option<String>,
    pub conteggio: Option<f64>,
    pub statistiche: Vec<Statistica>,
    /// Le altre informazioni dello strumento sull'analita (commenti, concentrazione attesa…).
    pub altro: Vec<Valore>,
}

/// Una statistica della fluorescenza di un analita.
#[derive(Clone, Debug, PartialEq)]
pub struct Statistica {
    /// Il ruolo come lo scrive Allotrope senza «role»: «median», «arithmetic mean», «coefficient of variation».
    pub ruolo: String,
    /// Non un numero quando lo strumento non lo ha dato: in Allotrope è -0 con l'errore.
    pub valore: f64,
    pub unita: String,
}

/// Un cubo di dati di una misura: una grandezza in funzione di una dimensione (l'indice della partizione).
#[derive(Clone, Debug, PartialEq)]
pub struct Cubo {
    /// La chiave del cubo nello schema («reporter dye data cube»).
    pub chiave: String,
    pub etichetta: String,
    /// La dimensione («cycle count») e la sua unità; `intero` se lo schema la vuole intera.
    pub dimensione: String,
    pub unita_dimensione: String,
    pub intero: bool,
    /// La grandezza («reporter dye fluorescence») e la sua unità.
    pub concetto: String,
    pub unita: String,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

impl Misura {
    /// È solo uno spettro: non va nell'elenco dei campioni, ha il suo foglio.
    pub fn e_spettro(&self) -> bool {
        self.valori.is_empty() && !self.spettro.is_empty() && self.curva.is_none()
    }

    /// Il valore di una chiave.
    pub fn valore(&self, chiave: &str) -> Option<&Valore> {
        self.valori.iter().find(|v| v.chiave == chiave)
    }
}

/// Un gruppo di misure lette insieme: una piastra letta a una lunghezza d'onda, una corsa di qPCR.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gruppo {
    /// «Piastra 1 · 450 nm», per le persone.
    pub titolo: String,
    pub quando: Option<String>,
    pub piastra: Option<String>,
    /// I pozzetti della piastra (96, 384…).
    pub pozzetti: Option<u32>,
    /// Dati comuni del gruppo per Allotrope (analista, metodo…): chiave e testo.
    pub dati: Vec<(String, String)>,
    pub misure: Vec<Misura>,
}

/// Un valore calcolato dallo strumento a partire da più misure (medie, rapporti), con le misure da cui viene.
#[derive(Clone, Debug, PartialEq)]
pub struct Calcolato {
    pub nome: String,
    pub valore: f64,
    pub unita: String,
    pub sorgenti: Vec<String>,
    /// Che cosa delle misure è servito («absorbance», «cycle threshold result»).
    pub da: String,
    /// Come l'ha calcolato lo strumento, se lo dice (la formula della colonna: «Average(OD)»).
    pub descrizione: String,
    /// Altri calcoli da cui viene (indici in `Lettura::calcolati`): la media del Ct da cui viene il Δ-Ct.
    pub da_calcoli: Vec<usize>,
}

/// Lo strumento e il programma che hanno scritto il file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strumento {
    pub produttore: String,
    pub modello: String,
    pub identificativo: String,
    pub seriale: String,
    pub software: String,
    pub versione: String,
    /// Il nome del computer o del sistema dati («DESKTOP-A678B9H»).
    pub istanza: String,
    /// Il numero d'inventario dello strumento (nome del sistema, «G037DE019»).
    pub inventario: String,
    pub descrizione: String,
    pub marchio: String,
    /// La versione del firmware dello strumento.
    pub firmware: String,
}

/// Il file letto.
#[derive(Clone, Debug, PartialEq)]
pub struct Lettura {
    /// L'identificativo del profilo («tecan_magellan»).
    pub profilo: &'static str,
    /// «Tecan Magellan» per le persone.
    pub nome: &'static str,
    pub tecnica: Tecnica,
    pub strumento: Strumento,
    pub file: String,
    pub sha256: String,
    /// Le informazioni della lettura per il foglio «Strumento»: etichetta e testo.
    pub info: Vec<(String, String)>,
    /// Le altre informazioni del programma che ha scritto il file, per Allotrope (versione del CSV, 21 CFR Part 11…).
    pub sistema: Vec<Valore>,
    /// Le altre informazioni dello strumento, per Allotrope.
    pub dispositivo: Vec<Valore>,
    pub gruppi: Vec<Gruppo>,
    pub calcolati: Vec<Calcolato>,
    /// La chiave del valore da mostrare sulla piastra e la sua unità come la scrive lo strumento.
    pub principale: String,
    pub unita: String,
    /// Avvisi per chi legge (dati che lo schema vuole e il file non ha…).
    pub avvisi: Vec<String>,
    base_id: String,
    n_id: usize,
}

impl Lettura {
    fn nuova(p: &Profilo, fonte: &Fonte) -> Lettura {
        Lettura {
            profilo: p.id,
            nome: p.nome,
            tecnica: p.tecnica,
            strumento: Strumento { produttore: p.produttore.into(), ..Default::default() },
            file: fonte.nome.clone(),
            sha256: fonte.sha256(),
            info: Vec::new(),
            sistema: Vec::new(),
            dispositivo: Vec::new(),
            gruppi: Vec::new(),
            calcolati: Vec::new(),
            principale: String::new(),
            unita: String::new(),
            avvisi: Vec::new(),
            base_id: fonte.stem().to_uppercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect(),
            n_id: 0,
        }
    }

    /// Il prossimo identificativo di misura: unico e stabile (nome del file e numero).
    pub fn id(&mut self) -> String {
        let id = format!("{}_{}", self.base_id, self.n_id);
        self.n_id += 1;
        id
    }

    /// Tutte le misure.
    pub fn misure(&self) -> impl Iterator<Item = &Misura> {
        self.gruppi.iter().flat_map(|g| g.misure.iter())
    }

    /// Quante misure (gli spettri non contano: sono dati di una misura).
    pub fn conta(&self) -> usize {
        self.misure().filter(|m| !m.e_spettro()).count()
    }

    /// Le misure hanno pozzetti: si mostra la piastra.
    pub fn a_piastra(&self) -> bool {
        self.misure().any(|m| m.pozzetto.is_some())
    }

    /// Righe e colonne della piastra del gruppo `g`: dal numero di pozzetti o dai pozzetti letti.
    pub fn formato(&self, g: usize) -> (usize, usize) {
        let Some(gr) = self.gruppi.get(g) else { return (8, 12) };
        let dai_pozzetti = gr.misure.iter().filter_map(|m| m.pozzetto.as_deref().and_then(pozzetto_rc)).fold((0, 0), |a, (r, c)| (a.0.max(r + 1), a.1.max(c + 1)));
        let formati = [(2, 3), (3, 4), (2, 8), (4, 6), (6, 8), (8, 12), (16, 24), (32, 48)];
        if let Some(n) = gr.pozzetti {
            if let Some(&f) = formati.iter().find(|(r, c)| (r * c) as u32 == n) {
                if f.0 >= dai_pozzetti.0 && f.1 >= dai_pozzetti.1 {
                    return f;
                }
            }
        }
        formati.iter().copied().find(|(r, c)| *r >= dai_pozzetti.0 && *c >= dai_pozzetti.1).unwrap_or((dai_pozzetti.0.max(1), dai_pozzetti.1.max(1)))
    }

    /// Il valore principale di una misura, se è un numero. Per le curve (cinetiche e spettri dei pozzetti)
    /// è l'ultimo punto o il massimo, calcolato da Cedro e detto tale.
    pub fn valore(&self, m: &Misura) -> Option<f64> {
        if self.principale == PRINCIPALE_CURVA {
            return m.curva.as_ref().and_then(|c| c.riassunto(&m.spettro));
        }
        if m.valore(&self.principale).is_none() {
            // Una misura senza il valore principale: la curva, o (piastre lette in modi diversi nello stesso
            // file, assorbanza e luminescenza) il primo valore misurato che ha.
            return m
                .curva
                .as_ref()
                .and_then(|c| c.riassunto(&m.spettro))
                .or_else(|| m.valori.iter().find(|v| v.posto == Posto::Misura && v.chiave != "compartment temperature" && v.numero().is_some()).and_then(Valore::numero));
        }
        m.valore(&self.principale).and_then(Valore::numero)
    }

    /// La prima curva della lettura (cinetica o spettro dei pozzetti), se ce n'è.
    pub fn curva(&self) -> Option<&Curva> {
        self.misure().find_map(|m| m.curva.as_ref())
    }

    /// Il testo del valore principale quando non è un numero («Undetermined», «OVER»).
    pub fn testo(&self, m: &Misura) -> Option<String> {
        match m.valore(&self.principale).map(|v| &v.dato) {
            Some(Dato::Testo(t)) | Some(Dato::Assente(t)) => Some(t.clone()),
            _ => None,
        }
    }
}

/// Il «valore principale» delle letture fatte di curve: non c'è un valore solo per pozzetto, si mostra
/// l'ultimo punto della cinetica o il massimo dello spettro.
pub const PRINCIPALE_CURVA: &str = "@curva";

/// Riga e colonna (da 0) di un pozzetto scritto «A1», «a01», «AA12».
pub fn pozzetto_rc(p: &str) -> Option<(usize, usize)> {
    let p = p.trim();
    let lettere: String = p.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let numero = &p[lettere.len()..];
    if lettere.is_empty() || lettere.len() > 2 || numero.is_empty() || !numero.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let l = lettere.to_ascii_uppercase();
    let b = l.as_bytes();
    let r = if b.len() == 1 { (b[0] - b'A') as usize } else { 26 + (b[1] - b'A') as usize };
    let c: usize = numero.parse().ok()?;
    (c >= 1 && c <= 48 && r < 32).then(|| (r, c - 1))
}

/// Il ruolo di una misura: quello segnato da chi converte sul pozzetto, altrimenti quello del file.
pub fn ruolo_di(m: &Misura, ruoli: &[(String, Ruolo)]) -> Ruolo {
    if let Some(p) = &m.pozzetto {
        let rc = pozzetto_rc(p);
        if let Some((_, r)) = ruoli.iter().find(|(w, _)| pozzetto_rc(w) == rc) {
            return *r;
        }
    }
    m.ruolo
}

/// Il nome del pozzetto come lo scrive Allotrope: «A1», «H12».
pub fn pozzetto_nome(r: usize, c: usize) -> String {
    let l = if r < 26 { ((b'A' + r as u8) as char).to_string() } else { format!("A{}", (b'A' + (r - 26) as u8) as char) };
    format!("{l}{}", c + 1)
}

/// Un profilo: come si riconosce e come si legge il file di uno strumento.
pub struct Profilo {
    pub id: &'static str,
    pub nome: &'static str,
    pub produttore: &'static str,
    pub tecnica: Tecnica,
    /// Le estensioni che il programma dello strumento scrive.
    pub estensioni: &'static [&'static str],
    /// Il punteggio da 0 (non è questo) a 100 (sicuro).
    pub riconosci: fn(&Fonte) -> u8,
    pub leggi: fn(&Fonte, &mut Lettura) -> Result<(), String>,
}

/// Tutti i profili pronti.
pub fn profili() -> &'static [Profilo] {
    profili::TUTTI
}

/// Il profilo con questo identificativo.
pub fn profilo(id: &str) -> Option<&'static Profilo> {
    profili().iter().find(|p| p.id == id)
}

/// Il profilo che riconosce meglio il file, con il punteggio. `None` se nessuno lo riconosce.
pub fn riconosci(fonte: &Fonte) -> Option<(&'static Profilo, u8)> {
    let mut best: Option<(&'static Profilo, u8)> = None;
    for p in profili() {
        if !p.estensioni.contains(&fonte.ext.as_str()) {
            continue;
        }
        let s = (p.riconosci)(fonte);
        if s >= 50 && best.is_none_or(|(_, b)| s > b) {
            best = Some((p, s));
        }
    }
    best
}

/// Il file letto con un profilo preciso.
pub fn leggi_con(p: &Profilo, fonte: &Fonte) -> Result<Lettura, String> {
    let mut l = Lettura::nuova(p, fonte);
    // Chi sceglie lo strumento a mano può scegliere quello sbagliato: un file che non ha la forma attesa
    // dà un errore da mostrare, mai una chiusura del programma.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (p.leggi)(fonte, &mut l))) {
        Ok(r) => r?,
        Err(_) => return Err("il file non ha la forma che questo strumento si aspetta".into()),
    }
    if l.conta() == 0 {
        return Err("nel file non ci sono misure".into());
    }
    if l.principale.is_empty() {
        let p = l.misure().flat_map(|m| m.valori.iter()).find(|v| matches!(v.posto, Posto::Misura | Posto::Elaborato) && v.numero().is_some()).map(|v| v.chiave.clone()).unwrap_or_default();
        l.principale = p;
    }
    // Gli identificativi delle misure: unici e stabili (il nome del file e il numero della misura).
    let mancano: Vec<(usize, usize)> = l.gruppi.iter().enumerate().flat_map(|(g, gr)| gr.misure.iter().enumerate().filter(|(_, m)| m.id.is_empty()).map(move |(i, _)| (g, i))).collect();
    for (g, i) in mancano {
        let id = l.id();
        l.gruppi[g].misure[i].id = id;
    }
    Ok(l)
}

/// Il file letto con il profilo che lo riconosce.
pub fn leggi(fonte: &Fonte) -> Result<Lettura, String> {
    let (p, _) = riconosci(fonte).ok_or("strumento non riconosciuto")?;
    leggi_con(p, fonte)
}

/// Il file su disco letto con il profilo che lo riconosce.
pub fn leggi_file(path: &Path) -> Result<Lettura, String> {
    leggi(&Fonte::apri(path)?)
}

/// Vero se il file sembra l'export di uno strumento che Cedro conosce.
pub fn e_strumento(path: &Path) -> bool {
    Fonte::apri(path).ok().is_some_and(|f| riconosci(&f).is_some())
}
