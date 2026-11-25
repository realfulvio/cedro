//! Applied Biosystems QuantStudio Design & Analysis Software: l'export in Excel (un foglio per sezione:
//! Results, Amplification Data, Replicate Group Result, Standard Curve Result…). Il programma ne scrive
//! sette tipi: curva standard, quantificazione relativa, genotipizzazione, curva di fusione,
//! presenza/assenza e l'analisi primaria (con o senza i fogli di dettaglio).
//!
//! Si leggono i pozzetti e i bersagli, i risultati per pozzetto (Cq, soglia, linea di base, qualità) e i
//! calcoli che il programma fa sulle repliche (media, deviazione, quantità, RQ…) con le misure da cui
//! vengono. Le curve ciclo per ciclo (amplificazione, multicomponente, fusione) restano nel file dello
//! strumento. Gli export del vecchio formato (con il foglio «Sample Setup») li legge il profilo
//! QuantStudio.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::strumenti::calcdocs::{self as cd, Config, DatiVista, Elemento, Ordina, Sorgente, Vista};
use crate::strumenti::fonte::{numero, Foglio, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "appbio_quantstudio_designandanalysis",
    nome: "Applied Biosystems QuantStudio Design & Analysis",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Qpcr,
    estensioni: &["xlsx", "xls"],
    riconosci,
    leggi,
};

// ---------------------------------------------------------------- riconoscimento

fn chiave_in_testa<'a>(f: &'a Foglio, k: &str) -> Option<&'a str> {
    f.righe.iter().take(100).find(|r| r.first().is_some_and(|c| c.trim() == k)).and_then(|r| r.get(1)).map(String::as_str)
}

/// Un export di Design & Analysis: foglio «Results» (o «Primary_result») con in testa lo strumento
/// QuantStudio. Con il foglio «Sample Setup» è il vecchio formato, a meno che il file non nomini il
/// programma Design & Analysis.
pub(super) fn riconosci(f: &Fonte) -> u8 {
    if !f.foglio() {
        return 0;
    }
    let Some(res) = f.fogli.iter().find(|s| matches!(s.nome.trim(), "Results" | "Primary_result")) else { return 0 };
    if !chiave_in_testa(res, "Instrument Type").is_some_and(|v| v.contains("QuantStudio")) {
        return 0;
    }
    match f.fogli.iter().find(|s| s.nome.trim() == "Sample Setup") {
        None => 97,
        Some(setup) => {
            let nome = chiave_in_testa(setup, "Experiment File Name").or_else(|| chiave_in_testa(res, "Experiment File Name"));
            if nome.is_some_and(|n| n.contains("Design & Analysis")) {
                96
            } else {
                0
            }
        }
    }
}

// ---------------------------------------------------------------- fogli come li legge pandas

/// I testi che pandas legge come «mancante» (NA): una cella con uno di questi non ha valore.
const NA: [&str; 19] = ["", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan", "1.#IND", "1.#QNAN", "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a", "nan", "null"];

type C = Option<String>;

fn cella(s: &str) -> C {
    if NA.contains(&s) {
        None
    } else {
        Some(s.to_string())
    }
}

#[derive(Clone, Default)]
struct Tab {
    colonne: Vec<String>,
    righe: Vec<Vec<C>>,
}

impl Tab {
    fn idx(&self, nome: &str) -> Option<usize> {
        self.colonne.iter().position(|c| c == nome)
    }
    fn e_vuota(&self) -> bool {
        self.righe.is_empty()
    }
    /// Mette a mancante la colonna (la crea se non c'è).
    fn azzera(&mut self, nome: &str) -> usize {
        let i = match self.idx(nome) {
            Some(i) => i,
            None => {
                self.colonne.push(nome.to_string());
                self.colonne.len() - 1
            }
        };
        for r in &mut self.righe {
            if r.len() <= i {
                r.resize(i + 1, None);
            }
            r[i] = None;
        }
        i
    }
}

/// Il foglio diviso in blocco d'intestazione (le righe prima della prima con la prima cella vuota) e tabella
/// (la prima riga è fatta di nomi di colonna).
fn spezza(f: &Foglio) -> Result<(Vec<Vec<C>>, Tab), String> {
    let mut righe: Vec<Vec<C>> = f.righe.iter().map(|r| r.iter().map(|c| cella(c)).collect()).collect();
    let senza_primo = |r: &Vec<C>| r.first().is_none_or(|c| c.is_none());
    let h = righe.iter().position(senza_primo).ok_or_else(|| format!("nel foglio «{}» manca la riga vuota tra l'intestazione e la tabella", f.nome))?;
    let mut i = h;
    while i < righe.len() && senza_primo(&righe[i]) {
        i += 1;
    }
    if i >= righe.len() {
        return Err(format!("il foglio «{}» non ha la tabella", f.nome));
    }
    let mut coda = righe.split_off(i);
    righe.truncate(h);
    let dati = coda.split_off(1);
    let colonne = coda[0].iter().map(|c| c.as_deref().unwrap_or("None").trim().to_string()).collect();
    Ok((righe, Tab { colonne, righe: dati }))
}

/// Come SeriesData di allotropy: una riga con nomi e valori, che tiene il conto delle chiavi lette.
struct Riga<'a> {
    nomi: &'a [String],
    celle: &'a [C],
    letti: RefCell<Vec<bool>>,
}

impl<'a> Riga<'a> {
    fn nuova(nomi: &'a [String], celle: &'a [C]) -> Riga<'a> {
        Riga { nomi, celle, letti: RefCell::new(vec![false; nomi.len()]) }
    }

    fn segna(&self, k: &str) {
        let mut l = self.letti.borrow_mut();
        for (i, n) in self.nomi.iter().enumerate() {
            if n == k {
                l[i] = true;
            }
        }
    }

    fn grezzo(&self, k: &str) -> Option<&'a str> {
        self.segna(k);
        let i = self.nomi.iter().position(|n| n == k)?;
        self.celle.get(i)?.as_deref()
    }

    /// Il primo valore presente tra le chiavi, come testo.
    fn testo(&self, ks: &[&str]) -> Option<String> {
        ks.iter().find_map(|k| self.grezzo(k).map(str::to_string))
    }

    fn num(&self, ks: &[&str]) -> Option<f64> {
        ks.iter().find_map(|k| self.grezzo(k).and_then(numero))
    }

    fn intero(&self, ks: &[&str]) -> Option<i64> {
        ks.iter().find_map(|k| {
            let t = self.grezzo(k)?.trim();
            t.parse::<i64>().ok().or_else(|| numero(t).map(|x| x.trunc() as i64))
        })
    }

    fn si_no(&self, ks: &[&str]) -> Option<bool> {
        ks.iter().find_map(|k| self.grezzo(k).map(|v| matches!(v.to_lowercase().as_str(), "yes" | "y" | "true" | "t" | "1")))
    }

    /// Tutto quello che non è stato letto (e ha un valore), nell'ordine delle colonne: i numeri come numeri.
    fn non_letti(&self, salta: &[&str]) -> Vec<(String, Dato)> {
        for k in salta {
            self.segna(k);
        }
        let l = self.letti.borrow();
        self.nomi
            .iter()
            .enumerate()
            .filter(|(i, _)| !l[*i])
            .filter_map(|(i, n)| {
                let v = self.celle.get(i)?.as_deref()?;
                Some((n.clone(), numero(v).map_or_else(|| Dato::Testo(v.to_string()), Dato::Numero)))
            })
            .collect()
    }
}

fn valore_da(k: &str, d: &Dato, posto: Posto) -> Valore {
    Valore { chiave: k.to_string(), dato: d.clone(), unita: String::new(), posto }
}

// ---------------------------------------------------------------- l'intestazione

struct Testata {
    tempo: String,
    pozzetti: u32,
    id_strumento: String,
    modello: String,
    seriale: String,
    metodo: String,
    chimica: Option<String>,
    passivo: Option<String>,
    barcode: Option<String>,
    analista: Option<String>,
    file: String,
    software: Option<String>,
    versione: Option<String>,
    blocco: String,
    durata: Option<String>,
    coperchio: Option<f64>,
    inizio: Option<String>,
    vol_campione: Option<f64>,
    analisi: Option<String>,
    esportato: Option<String>,
    extra: Vec<(String, Dato)>,
}

/// 96 o 384 pozzetti dal nome del blocco («96-Well 0.2-mL Block», «384-Well Block», «Fast 96-Well Block»).
fn conta_pozzetti(blocco: &str) -> Option<u32> {
    let mut num = String::new();
    for c in blocco.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        if num == "96" || num == "384" {
            return num.parse().ok();
        }
        num.clear();
    }
    // La piastra TaqMan Array Card ha 384 pozzetti.
    blocco.to_lowercase().contains("array card").then_some(384)
}

/// Il nome del programma e la versione da «Design & Analysis Software v2.7.0».
fn programma(s: &str) -> Option<(String, String)> {
    s.rmatch_indices(" v").map(|(p, _)| p).find(|&p| p + 2 < s.len()).map(|p| (s[..p].to_string(), s[p + 2..].to_string()))
}

impl Testata {
    fn leggi(r: &Riga) -> Result<Testata, String> {
        let sw = r.testo(&["Software Name and Version"]).unwrap_or_default();
        let (software, versione) = match programma(&sw) {
            Some((n, v)) => (Some(n), Some(v)),
            None => (r.testo(&["Software name"]), r.testo(&["Software version"])),
        };
        r.testo(&["PCR Stage/Step Number"]);
        let blocco = r.testo(&["Block Type"]).ok_or("nel file manca il tipo di blocco (Block Type)")?;
        let tempo = r.testo(&["Run End Date/Time", "Run End Data/Time", "Experiment Run End Time"]).ok_or("nel file manca la data e ora della fine della corsa")?;
        let pozzetti = conta_pozzetti(&blocco).ok_or_else(|| format!("blocco «{blocco}» non supportato: Cedro legge le piastre da 96 e 384 pozzetti"))?;
        let id_strumento = r.testo(&["Instrument Name"]).unwrap_or_else(|| "N/A".into());
        let modello = r.testo(&["Instrument Type"]).unwrap_or_else(|| "N/A".into());
        let seriale = r.testo(&["Instrument Serial Number"]).unwrap_or_else(|| "N/A".into());
        let metodo = r.testo(&["Quantification Cycle Method"]).ok_or("nel file manca il metodo di quantificazione (Quantification Cycle Method)")?;
        let chimica = r.testo(&["Chemistry"]);
        let passivo = r.testo(&["Passive Reference"]);
        let barcode = r.testo(&["Barcode", "Experiment Barcode"]);
        let analista = r.testo(&["Operator", "User Name"]);
        let file_completo = r.testo(&["File Name", "Experiment File Name"]).ok_or("nel file manca il nome dell'esperimento (File Name)")?;
        let file = file_completo.rsplit(['\\', '/']).next().unwrap_or(&file_completo).to_string();
        r.testo(&["Block Serial Number"]);
        r.testo(&["Heated Cover Serial Number"]);
        let durata = r.testo(&["Run Duration"]);
        let coperchio = r.num(&["Cover Temperature"]);
        let inizio = r.testo(&["Run Start Date/Time"]);
        let vol_campione = r.num(&["Sample Volume"]);
        let analisi = r.testo(&["Analysis Date/Time"]);
        let esportato = r.testo(&["Exported On"]);
        let extra = r.non_letti(&[]);
        Ok(Testata { tempo, pozzetti, id_strumento, modello, seriale, metodo, chimica, passivo, barcode, analista, file, software, versione, blocco, durata, coperchio, inizio, vol_campione, analisi, esportato, extra })
    }
}

/// Il volume di un pozzetto dal blocco, in μL.
fn volume(blocco: &str) -> Option<f64> {
    super::quantstudio::volume_pozzetto(blocco).or_else(|| blocco.to_lowercase().contains("array card").then_some(1.5))
}

// ---------------------------------------------------------------- i risultati

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tipo {
    CurvaStandard,
    QuantificazioneRelativa,
    Genotipizzazione,
    CurvaFusione,
    PresenzaAssenza,
    AnalisiPrimariaSemplice,
    AnalisiPrimaria,
}

impl Tipo {
    fn asm(self) -> &'static str {
        match self {
            Tipo::CurvaStandard => "standard curve qPCR experiment",
            Tipo::QuantificazioneRelativa => "relative standard curve qPCR experiment",
            Tipo::Genotipizzazione => "genotyping qPCR experiment",
            Tipo::CurvaFusione => "melt curve qPCR experiment",
            Tipo::PresenzaAssenza => "presence/absence qPCR experiment",
            Tipo::AnalisiPrimariaSemplice | Tipo::AnalisiPrimaria => "primary analysis experiment",
        }
    }
    fn nome(self) -> &'static str {
        match self {
            Tipo::CurvaStandard => "Curva standard",
            Tipo::QuantificazioneRelativa => "Quantificazione relativa",
            Tipo::Genotipizzazione => "Genotipizzazione",
            Tipo::CurvaFusione => "Curva di fusione",
            Tipo::PresenzaAssenza => "Presenza/assenza",
            Tipo::AnalisiPrimariaSemplice | Tipo::AnalisiPrimaria => "Analisi primaria",
        }
    }
    fn con_chiamata(self) -> bool {
        matches!(self, Tipo::Genotipizzazione | Tipo::PresenzaAssenza)
    }
}

/// Le colonne numeriche dei risultati: la chiave del valore nei calcoli e i nomi della colonna.
const NUMERI: [(&str, &[&str]); 25] = [
    ("quantity", &["Quantity"]),
    ("quantity_mean", &["Quantity Mean"]),
    ("quantity_sd", &["Quantity SD"]),
    ("ct_mean", &["Cq Mean"]),
    ("eq_ct_mean", &["EqCq Mean"]),
    ("adj_eq_ct_mean", &["Adjusted EqCq Mean"]),
    ("ct_sd", &["Cq SD"]),
    ("ct_se", &["Cq SE"]),
    ("delta_ct_mean", &["Delta EqCq Mean"]),
    ("delta_ct_se", &["Delta EqCq SE"]),
    ("delta_ct_sd", &["Delta EqCq SD"]),
    ("delta_delta_ct", &["Delta Delta EqCq"]),
    ("rq", &["Rq"]),
    ("rq_min", &["Rq Min"]),
    ("rq_max", &["Rq Max"]),
    ("rn_mean", &["Rn Mean"]),
    ("rn_sd", &["Rn SD"]),
    ("y_intercept", &["Y-Intercept"]),
    ("r_squared", &["R2", "R(superscript 2)"]),
    ("slope", &["Slope"]),
    ("efficiency", &["Efficiency"]),
    ("amp_score", &["Amp Score"]),
    ("cq_conf", &["Cq Confidence"]),
    ("standard_deviation", &["Standard Deviation"]),
    ("standard_error", &["Standard Error"]),
];

/// Una misura: un bersaglio in un pozzetto.
struct Voce {
    id: i64,
    bersaglio: String,
    campione: String,
    posizione: String,
    reporter: Option<String>,
    quencher: Option<String>,
    ruolo: Option<Ruolo>,
    cicli: Option<f64>,
    soglia: Option<f64>,
    ct: Option<f64>,
    ct_testo: Option<String>,
    auto_soglia: Option<bool>,
    auto_base: Option<bool>,
    rn: Option<f64>,
    drn: Option<f64>,
    base_ini: Option<i64>,
    base_fine: Option<i64>,
    chiamata: Option<String>,
    metodo_geno: Option<f64>,
    numeri: Vec<(&'static str, f64)>,
    /// Tutto quello che il file dice del campione e non ha un posto suo (note del campione in Allotrope).
    custom: Vec<(String, Dato)>,
}

fn ruolo_asm(r: Option<Ruolo>) -> Option<&'static str> {
    r.map(|r| r.asm())
}

/// I pozzetti numerati: i numeri del file, oppure (nomi come «A01» o «A1a1») un numero per ogni nome,
/// in ordine alfabetico.
struct Pozzetti {
    mappa: Option<HashMap<String, i64>>,
}

impl Pozzetti {
    fn id(&self, v: &str) -> Option<i64> {
        match &self.mappa {
            Some(m) => m.get(v).copied(),
            None => {
                let t = v.trim();
                t.parse::<i64>().ok().or_else(|| numero(t).filter(|x| x.fract() == 0.0).map(|x| x as i64))
            }
        }
    }
}

struct Libro<'a> {
    f: &'a Fonte,
}

impl<'a> Libro<'a> {
    fn foglio(&self, nome: &str) -> Option<&'a Foglio> {
        let alias = if nome == "Results" { "Primary_result" } else { "" };
        self.f.fogli.iter().find(|s| s.nome.trim() == nome).or_else(|| if alias.is_empty() { None } else { self.f.fogli.iter().find(|s| s.nome.trim() == alias && !self.f.fogli.iter().any(|x| x.nome.trim() == "Results")) })
    }
    fn ha(&self, nome: &str) -> bool {
        self.foglio(nome).is_some()
    }
    fn tab(&self, nome: &str) -> Result<Option<Tab>, String> {
        match self.foglio(nome) {
            Some(f) => Ok(Some(spezza(f)?.1)),
            None => Ok(None),
        }
    }
    fn tab_piena(&self, nome: &str) -> Result<Tab, String> {
        let t = self.tab(nome)?.ok_or_else(|| format!("nel file manca il foglio «{nome}»"))?;
        if t.e_vuota() {
            return Err(format!("il foglio «{nome}» è vuoto"));
        }
        Ok(t)
    }
}

/// Se i pozzetti dei risultati non sono numeri, li numera per tutto il libro.
fn numera_pozzetti(libro: &Libro, risultati: &Tab) -> Pozzetti {
    let primo = risultati.idx("Well").and_then(|i| risultati.righe.first().and_then(|r| r.get(i).cloned().flatten()));
    let numerico = |v: &str| numero(v.trim()).is_some();
    match primo {
        Some(p) if !numerico(&p) => {
            let mut tutti: Vec<String> = Vec::new();
            for f in &libro.f.fogli {
                if let Ok((_, t)) = spezza(f) {
                    if let Some(i) = t.idx("Well") {
                        for r in &t.righe {
                            if let Some(Some(w)) = r.get(i) {
                                tutti.push(w.clone());
                            }
                        }
                    }
                }
            }
            tutti.sort();
            tutti.dedup();
            Pozzetti { mappa: Some(tutti.into_iter().enumerate().map(|(i, w)| (w, i as i64 + 1)).collect()) }
        }
        _ => Pozzetti { mappa: None },
    }
}

/// Aggiunge alla tabella dei risultati le colonne di un altro foglio, riga per riga con lo stesso
/// campione e bersaglio (come fa il programma: il valore del gruppo di repliche sta su ogni pozzetto).
fn aggiungi_dati(dati: &mut Tab, extra: &Tab, colonne: &[&str]) -> Result<(), String> {
    let (Some(cs), Some(ct)) = (dati.idx("Sample"), dati.idx("Target")) else { return Err("nei risultati mancano le colonne Sample e Target".into()) };
    let (Some(es), Some(et)) = (extra.idx("Sample"), extra.idx("Target")) else { return Err("nel foglio dei gruppi mancano le colonne Sample e Target".into()) };
    let cols: Vec<usize> = colonne.iter().map(|c| dati.azzera(c)).collect();
    let ecols: Vec<Option<usize>> = colonne.iter().map(|c| extra.idx(c)).collect();
    // Gli indici dei pozzetti per (campione, bersaglio).
    let mut per_chiave: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, r) in dati.righe.iter().enumerate() {
        if let (Some(Some(s)), Some(Some(t))) = (r.get(cs), r.get(ct)) {
            per_chiave.entry((s.clone(), t.clone())).or_default().push(i);
        }
    }
    for er in &extra.righe {
        let (Some(Some(s)), Some(Some(t))) = (er.get(es), er.get(et)) else { continue };
        let Some(righe) = per_chiave.get(&(s.clone(), t.clone())) else { continue };
        for &ri in righe {
            for (c, e) in cols.iter().zip(&ecols) {
                let v = e.and_then(|e| er.get(e).cloned().flatten());
                let r = &mut dati.righe[ri];
                if r.len() <= *c {
                    r.resize(*c + 1, None);
                }
                r[*c] = v;
            }
        }
    }
    Ok(())
}

/// La tabella dei risultati del tipo di esperimento, con le colonne dei fogli dei gruppi.
fn tabella_risultati(libro: &Libro, tipo: Tipo) -> Result<Tab, String> {
    let foglio = if tipo == Tipo::CurvaStandard { "Standard Curve Result" } else { "Results" };
    let mut dati = libro.tab_piena(foglio)?;
    match tipo {
        Tipo::QuantificazioneRelativa => {
            aggiungi_dati(&mut dati, &libro.tab_piena("Replicate Group Result")?, &["Cq SE"])?;
            aggiungi_dati(&mut dati, &libro.tab_piena("RQ Replicate Group Result")?, &["EqCq Mean", "Adjusted EqCq Mean", "Delta EqCq Mean", "Delta EqCq SD", "Delta EqCq SE", "Delta Delta EqCq", "Rq", "Rq Min", "Rq Max"])?;
        }
        Tipo::PresenzaAssenza => {
            aggiungi_dati(&mut dati, &libro.tab_piena("Target Call")?, &["Call"])?;
        }
        Tipo::Genotipizzazione => {
            let gr = libro.tab_piena("Genotyping Result")?;
            let Some(si) = gr.idx("SNP Assay") else { return Err("nel foglio Genotyping Result manca la colonna SNP Assay".into()) };
            let mut esteso = Tab { colonne: gr.colonne.clone(), righe: Vec::new() };
            let ti = esteso.azzera("Target");
            for (n, r) in gr.righe.iter().enumerate() {
                let snp = r.get(si).cloned().flatten().ok_or_else(|| format!("nel foglio Genotyping Result manca lo SNP alla riga {}", n + 1))?;
                for allele in ["Allele 1", "Allele 2"] {
                    let mut nuova = r.clone();
                    if nuova.len() <= ti {
                        nuova.resize(ti + 1, None);
                    }
                    nuova[ti] = Some(format!("{snp}-{allele}"));
                    esteso.righe.push(nuova);
                }
            }
            aggiungi_dati(&mut dati, &esteso, &["Call"])?;
        }
        Tipo::AnalisiPrimaria => {
            aggiungi_dati(&mut dati, &libro.tab_piena("Replicate Group Result")?, &["Cq Mean", "Cq SD", "Cq SE"])?;
        }
        Tipo::AnalisiPrimariaSemplice => {
            if dati.idx("Well").is_none() {
                let Some(pi) = dati.idx("Well Position") else { return Err("nei risultati mancano le colonne Well e Well Position".into()) };
                let wi = dati.azzera("Well");
                let mut viste: Vec<String> = Vec::new();
                for r in dati.righe.iter_mut() {
                    let p = r.get(pi).cloned().flatten();
                    if let Some(p) = p {
                        let n = match viste.iter().position(|x| *x == p) {
                            Some(n) => n,
                            None => {
                                viste.push(p);
                                viste.len() - 1
                            }
                        };
                        r[wi] = Some(format!("{}", n + 1));
                    }
                }
            }
            for nome in ["Sample", "Target"] {
                if dati.idx(nome).is_none() {
                    if let Some(i) = dati.idx(&format!("{nome} Name")) {
                        dati.colonne[i] = nome.to_string();
                    }
                }
            }
            if dati.idx("Target").is_none() {
                let ti = dati.azzera("Target");
                for r in dati.righe.iter_mut() {
                    r[ti] = Some("N/A".into());
                }
            }
        }
        Tipo::CurvaFusione => {}
        Tipo::CurvaStandard => {}
    }
    Ok(dati)
}

/// I cicli totali per pozzetto e bersaglio: il massimo del numero di ciclo nei dati di amplificazione.
fn cicli_totali(libro: &Libro, poz: &Pozzetti) -> Result<HashMap<(i64, String), f64>, String> {
    let mut out: HashMap<(i64, String), f64> = HashMap::new();
    let Some(t) = libro.tab("Amplification Data")? else { return Ok(out) };
    let (Some(wi), Some(ti), Some(ci)) = (t.idx("Well"), t.idx("Target"), t.idx("Cycle Number").or_else(|| t.idx("Cycle"))) else { return Ok(out) };
    for r in &t.righe {
        let (Some(Some(w)), Some(Some(tg)), Some(Some(c))) = (r.get(wi), r.get(ti), r.get(ci)) else { continue };
        let (Some(id), Some(c)) = (poz.id(w), numero(c)) else { continue };
        let x = out.entry((id, tg.clone())).or_insert(c);
        if c > *x {
            *x = c;
        }
    }
    Ok(out)
}

fn voci_da(libro: &Libro, tipo: Tipo, dati: &Tab, poz: &Pozzetti, cicli: &HashMap<(i64, String), f64>) -> Result<Vec<Voce>, String> {
    let Some(wi) = dati.idx("Well") else { return Err("nei risultati manca la colonna Well".into()) };
    let _ = libro;
    // I pozzetti in ordine crescente, le righe di ognuno nell'ordine del file.
    let mut per_pozzetto: Vec<(i64, Vec<usize>)> = Vec::new();
    for (i, r) in dati.righe.iter().enumerate() {
        let Some(Some(w)) = r.get(wi) else { continue };
        let Some(id) = poz.id(w) else { continue };
        match per_pozzetto.iter_mut().find(|(k, _)| *k == id) {
            Some((_, v)) => v.push(i),
            None => per_pozzetto.push((id, vec![i])),
        }
    }
    per_pozzetto.sort_by_key(|(k, _)| *k);
    let mut voci: Vec<Voce> = Vec::new();
    for (id, righe) in per_pozzetto {
        // Un bersaglio per pozzetto: se si ripete, vale l'ultima riga ma resta il posto della prima.
        let mut del_pozzetto: Vec<Voce> = Vec::new();
        for ri in righe {
            let riga = Riga::nuova(&dati.colonne, &dati.righe[ri]);
            let v = voce_da(tipo, &riga, id, poz, cicli)?;
            match del_pozzetto.iter_mut().find(|x| x.bersaglio == v.bersaglio) {
                Some(x) => *x = v,
                None => del_pozzetto.push(v),
            }
        }
        voci.extend(del_pozzetto);
    }
    Ok(voci)
}

fn voce_da(tipo: Tipo, r: &Riga, id: i64, poz: &Pozzetti, cicli: &HashMap<(i64, String), f64>) -> Result<Voce, String> {
    r.grezzo("Well");
    let bersaglio = r.testo(&["Target"]).ok_or_else(|| format!("nel pozzetto {id} manca il bersaglio (Target)"))?;
    let posizione = match r.testo(&["Well Position"]) {
        Some(p) => p,
        None if poz.mappa.is_some() => r.grezzo("Well").unwrap_or_default().to_string(),
        None => return Err(format!("nel pozzetto {id} manca la posizione (Well Position)")),
    };
    // Il risultato.
    let soglia = r.num(&["Threshold", "Ct Threshold"]);
    let ct_cella = ["Cq", "CT", "Ct"].iter().find_map(|k| r.grezzo(k).map(str::to_string));
    let ct = ["Cq", "CT", "Ct"].iter().find_map(|k| r.grezzo(k).and_then(numero));
    let auto_soglia = r.si_no(&["Auto Threshold", "Automatic Ct Threshold"]);
    let auto_base = r.si_no(&["Auto Baseline", "Automatic Baseline"]);
    let rn = r.num(&["Rn"]);
    let drn = r.num(&["Delta Rn"]);
    let base_ini = r.intero(&["Baseline Start"]);
    let base_fine = r.intero(&["Baseline End"]);
    let (chiamata, metodo_geno) = if tipo.con_chiamata() { (r.testo(&["Call"]), r.num(&["Threshold"])) } else { (None, None) };
    let mut numeri: Vec<(&'static str, f64)> = Vec::new();
    for (k, cols) in NUMERI {
        if let Some(x) = r.num(cols) {
            numeri.push((k, x));
        }
    }
    let quantita = numeri.iter().find(|(k, _)| *k == "quantity").map(|(_, v)| *v);
    // Il campione e il resto.
    let campione = r.testo(&["Sample"]).unwrap_or_else(|| posizione.clone());
    let reporter = r.testo(&["Reporter"]);
    let quencher = r.testo(&["Quencher"]);
    let task = r.testo(&["Task"]);
    let ruolo = task.as_deref().and_then(super::quantstudio::ruolo_da_task_esatto);
    let mut custom = r.non_letti(&["Dye", "Standard Deviation", "Standard Error"]);
    if ct.is_none() {
        if let Some(q) = quantita {
            custom.push(("quantity".into(), Dato::Numero(q)));
        }
    }
    let cicli = cicli.get(&(id, bersaglio.clone())).copied();
    Ok(Voce { id, bersaglio, campione, posizione, reporter, quencher, ruolo, cicli, soglia, ct, ct_testo: ct_cella, auto_soglia, auto_base, rn, drn, base_ini, base_fine, chiamata, metodo_geno, numeri, custom })
}

// ---------------------------------------------------------------- i calcoli sulle repliche

fn raggruppa<F: Fn(&Elemento) -> Option<String> + 'static>(chiave: F) -> Box<Ordina> {
    Box::new(move |el: &[Elemento], idx: &[usize]| {
        let mut out: Vec<(String, Vec<usize>)> = Vec::new();
        let mut posti: HashMap<String, usize> = HashMap::new();
        for &i in idx {
            if let Some(k) = chiave(&el[i]) {
                match posti.get(&k) {
                    Some(&p) => out[p].1.push(i),
                    None => {
                        posti.insert(k.clone(), out.len());
                        out.push((k, vec![i]));
                    }
                }
            }
        }
        out
    })
}

fn v_campione(sub: Option<Rc<Vista>>, rif: Option<&str>) -> Rc<Vista> {
    Rc::new(Vista::nuova("sample_id", sub, raggruppa(|e| e.testo("sample_identifier").filter(|s| !s.is_empty()))).con_riferimento(rif.map(str::to_string)))
}

fn v_bersaglio(sub: Option<Rc<Vista>>, rif: Option<&str>, e_rif: bool, nera: Option<&str>) -> Rc<Vista> {
    let nera = nera.map(str::to_string);
    Rc::new(
        Vista::nuova(
            "target_dna",
            sub,
            raggruppa(move |e| {
                let t = e.testo("target_dna_description").filter(|s| !s.is_empty())?;
                if nera.as_deref() == Some(t.as_str()) {
                    None
                } else {
                    Some(t)
                }
            }),
        )
        .con_riferimento(rif.map(str::to_string))
        .come_riferimento(e_rif),
    )
}

fn v_uuid() -> Rc<Vista> {
    Rc::new(Vista::nuova("uuid", None, raggruppa(|e| Some(e.id.clone()))))
}

fn v_standard() -> Rc<Vista> {
    Rc::new(Vista::nuova("target_dna", None, raggruppa(|e| e.testo("target_dna_description").filter(|_| e.testo("sample_role_type").as_deref() == Some("standard sample role")))))
}

fn ctr() -> Sorgente {
    cd::misura("cycle threshold result", "cycle_threshold_result", true)
}

fn calc(c: &Rc<Config>) -> Sorgente {
    Sorgente::Calcolo(c.clone())
}

fn cfg(nome: &str, valore: &str, vista: &Rc<DatiVista>, sorg: Vec<Sorgente>) -> Rc<Config> {
    Rc::new(Config::nuova(nome, valore, vista, sorg))
}

/// Un calcolo che viene dal Cq delle misure (media, deviazione, punteggio…).
fn da_ct(nome: &str, valore: &str, vista: &Rc<DatiVista>) -> Rc<Config> {
    cfg(nome, valore, vista, vec![ctr()])
}

fn calcoli(tipo: Tipo, elementi: &[Elemento], r_campione: Option<&str>, r_bersaglio: Option<&str>) -> Vec<cd::Doc> {
    let tutti: Vec<usize> = (0..elementi.len()).collect();
    let dv = |v: &Rc<Vista>| Rc::new(v.applica(elementi, &tutti));
    let sid_tdna = dv(&v_campione(Some(v_bersaglio(None, None, false, None)), None));
    let sid_tdna_uuid = dv(&v_campione(Some(v_bersaglio(Some(v_uuid()), None, false, None)), None));
    let configs: Vec<Rc<Config>> = match tipo {
        Tipo::CurvaStandard => {
            let tdna = dv(&v_standard());
            let quantita = cfg("quantity", "quantity", &sid_tdna_uuid, vec![ctr(), calc(&da_ct("y intercept", "y_intercept", &tdna)), calc(&da_ct("slope", "slope", &tdna))]);
            vec![
                quantita.clone(),
                da_ct("amplification score", "amp_score", &sid_tdna_uuid),
                da_ct("cq confidence", "cq_conf", &sid_tdna_uuid),
                cfg("quantity mean", "quantity_mean", &sid_tdna, vec![calc(&quantita)]),
                cfg("quantity sd", "quantity_sd", &sid_tdna, vec![calc(&quantita)]),
                da_ct("ct mean", "ct_mean", &sid_tdna),
                da_ct("ct sd", "ct_sd", &sid_tdna),
                da_ct("y intercept", "y_intercept", &tdna),
                da_ct("r^2", "r_squared", &tdna),
                da_ct("slope", "slope", &tdna),
                da_ct("efficiency", "efficiency", &tdna),
                da_ct("standard deviation", "standard_deviation", &sid_tdna_uuid),
                da_ct("standard error", "standard_error", &sid_tdna_uuid),
            ]
        }
        Tipo::QuantificazioneRelativa => {
            let tdna = dv(&v_standard());
            let sid_ref_tdna = dv(&v_campione(Some(v_bersaglio(None, None, false, None)), r_campione));
            let sid_tdna_ref = dv(&v_campione(Some(v_bersaglio(None, r_bersaglio, true, None)), None));
            let sid_tdna_nera = dv(&v_campione(Some(v_bersaglio(None, None, false, r_bersaglio)), None));
            let quantita = cfg("quantity", "quantity", &sid_tdna_uuid, vec![ctr(), calc(&da_ct("y intercept", "y_intercept", &tdna)), calc(&da_ct("slope", "slope", &tdna))]);
            let eq = |v: &Rc<DatiVista>| cfg("equivalent ct mean", "eq_ct_mean", v, vec![calc(&da_ct("ct mean", "ct_mean", &sid_tdna))]);
            let adj = |v: &Rc<DatiVista>| Rc::new(Config::nuova("adjusted equivalent ct mean", "adj_eq_ct_mean", v, vec![calc(&eq(&sid_tdna))]).opzionale());
            let delta = |v: &Rc<DatiVista>| cfg("delta equivalent ct mean", "delta_ct_mean", v, vec![calc(&adj(&sid_tdna)), calc(&adj(&sid_tdna_ref))]);
            let rq = || {
                let ddct = cfg("delta delta equivalent ct", "delta_delta_ct", &sid_tdna, vec![calc(&delta(&sid_tdna)), calc(&delta(&sid_ref_tdna))]);
                cfg("rq", "rq", &sid_tdna, vec![calc(&ddct)])
            };
            let rel_rq = || cfg("relative rq", "rq", &sid_tdna, vec![calc(&cfg("quantity mean", "quantity_mean", &sid_tdna, vec![calc(&quantita)]))]);
            vec![
                quantita.clone(),
                da_ct("amplification score", "amp_score", &sid_tdna_uuid),
                da_ct("cq confidence", "cq_conf", &sid_tdna_uuid),
                da_ct("ct mean", "ct_mean", &sid_tdna),
                da_ct("ct sd", "ct_sd", &sid_tdna),
                cfg("delta equivalent ct sd", "delta_ct_sd", &sid_tdna, vec![calc(&da_ct("ct sd", "ct_sd", &sid_tdna)), calc(&da_ct("ct sd", "ct_sd", &sid_tdna_ref))]),
                cfg("delta equivalent ct se", "delta_ct_se", &sid_tdna, vec![calc(&da_ct("ct se", "ct_se", &sid_tdna)), calc(&da_ct("ct se", "ct_se", &sid_tdna_ref))]),
                cfg("relative rq min", "rq_min", &sid_tdna, vec![calc(&rel_rq())]),
                cfg("relative rq max", "rq_max", &sid_tdna, vec![calc(&rel_rq())]),
                cfg("rq min", "rq_min", &sid_tdna_nera, vec![calc(&rq())]),
                cfg("rq max", "rq_max", &sid_tdna_nera, vec![calc(&rq())]),
            ]
        }
        Tipo::PresenzaAssenza => vec![
            cfg("quantity", "quantity", &sid_tdna_uuid, vec![ctr()]),
            da_ct("amplification score", "amp_score", &sid_tdna_uuid),
            da_ct("cq confidence", "cq_conf", &sid_tdna_uuid),
            cfg("rn mean", "rn_mean", &sid_tdna, vec![cd::misura("normalized reporter result", "normalized_reporter_result", false)]),
            cfg("rn sd", "rn_sd", &sid_tdna, vec![cd::misura("normalized reporter result", "normalized_reporter_result", false)]),
        ],
        Tipo::AnalisiPrimaria => vec![da_ct("ct mean", "ct_mean", &sid_tdna), da_ct("ct sd", "ct_sd", &sid_tdna), da_ct("ct se", "ct_se", &sid_tdna)],
        Tipo::Genotipizzazione | Tipo::CurvaFusione | Tipo::AnalisiPrimariaSemplice => Vec::new(),
    };
    cd::costruisci(elementi, &configs)
}

// ---------------------------------------------------------------- la lettura

fn si_no_py(v: Option<bool>) -> Option<bool> {
    v
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let libro = Libro { f };
    // Il vecchio formato (con «Sample Setup») lo legge il profilo QuantStudio: qui cambia solo il programma.
    if libro.ha("Sample Setup") {
        super::quantstudio::leggi(f, l)?;
        l.strumento.software = "Design & Analysis Software".into();
        l.strumento.versione = "1.x".into();
        return Ok(());
    }
    let res = libro.foglio("Results").ok_or("nel file manca il foglio Results")?;
    let (intest, _) = spezza(res)?;
    let nomi: Vec<String> = intest.iter().map(|r| r.first().cloned().flatten().unwrap_or_else(|| "None".into()).trim().to_string()).collect();
    let valori: Vec<C> = intest.iter().map(|r| r.get(1).cloned().flatten()).collect();
    let riga_testata = Riga::nuova(&nomi, &valori);
    let plugin = riga_testata.testo(&["Plugin Name and Version"]);
    let t = Testata::leggi(&riga_testata)?;

    // Il tipo di esperimento, dai fogli presenti e dai plugin che hanno fatto l'analisi.
    let primaria_ok = |p: &Option<String>| match p {
        None => true,
        Some(p) => {
            let b = p.as_bytes();
            let ver = p.strip_prefix("Primary Analysis v").map(|s| s.split('.').collect::<Vec<_>>());
            b.starts_with(b"Primary Analysis v") && ver.is_some_and(|v| v.len() == 3 && v.iter().all(|x| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit())))
        }
    };
    let con = |p: &str| plugin.as_deref().is_none_or(|x| x.contains(p));
    let solo_results = f.fogli.len() == 1 && libro.ha("Results");
    let tipo = if libro.ha("Standard Curve Result") && con("Standard Curve") {
        Tipo::CurvaStandard
    } else if libro.ha("RQ Replicate Group Result") && con("Relative Quantification") {
        Tipo::QuantificazioneRelativa
    } else if libro.ha("Genotyping Result") && con("Genotyping") {
        Tipo::Genotipizzazione
    } else if libro.ha("Melt Curve Raw") && libro.ha("Melt Curve Result") {
        Tipo::CurvaFusione
    } else if libro.ha("Target Call") && con("Presence Absence") {
        Tipo::PresenzaAssenza
    } else if solo_results && primaria_ok(&plugin) {
        Tipo::AnalisiPrimariaSemplice
    } else if ["Results", "Amplification Data", "Multicomponent", "Replicate Group Result"].iter().all(|n| libro.ha(n)) && plugin.is_some() && primaria_ok(&plugin) {
        Tipo::AnalisiPrimaria
    } else {
        return Err("non si riesce a capire il tipo di esperimento dai fogli del file".into());
    };

    let dati = tabella_risultati(&libro, tipo)?;
    let poz = numera_pozzetti(&libro, &dati);
    let cicli = cicli_totali(&libro, &poz)?;
    let voci = voci_da(&libro, tipo, &dati, &poz, &cicli)?;
    if voci.is_empty() {
        return Err("nel file non ci sono risultati per pozzetto".into());
    }

    // Il campione e il bersaglio di riferimento della quantificazione relativa.
    let (r_campione, r_bersaglio) = if tipo == Tipo::QuantificazioneRelativa { riferimenti(&libro)? } else { (None, None) };

    // Lo strumento e il programma.
    let quando = tempo::iso(&t.tempo, false);
    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.identificativo = t.id_strumento.clone();
    l.strumento.modello = t.modello.clone();
    l.strumento.seriale = t.seriale.clone();
    l.strumento.software = t.software.clone().unwrap_or_default();
    l.strumento.versione = t.versione.clone().unwrap_or_default();
    l.principale = "cycle threshold result (qPCR)".into();
    l.unita = String::new();
    l.info.push(("Tipo di esperimento".into(), tipo.nome().into()));
    if let (Some(n), Some(v)) = (&t.software, &t.versione) {
        l.info.push(("Programma".into(), format!("{n} {v}")));
    }
    if let Some(p) = &plugin {
        l.info.push(("Analisi".into(), p.clone()));
    }
    l.info.push(("Esperimento".into(), t.file.clone()));
    for (k, v) in [("Chimica", &t.chimica), ("Riferimento passivo", &t.passivo), ("Operatore", &t.analista), ("Codice a barre", &t.barcode), ("Inizio della corsa", &t.inizio), ("Durata", &t.durata), ("Analisi eseguita", &t.analisi), ("Esportato il", &t.esportato)] {
        if let Some(v) = v {
            l.info.push((k.into(), v.clone()));
        }
    }
    l.info.push(("Metodo di quantificazione".into(), t.metodo.clone()));
    l.info.push(("Blocco".into(), t.blocco.clone()));
    if let Some(q) = &quando {
        l.info.push(("Fine della corsa".into(), q.clone()));
    }
    l.avvisi.push("Le curve ciclo per ciclo (amplificazione, multicomponente, fusione) non vengono copiate: restano nel file dello strumento.".into());
    for (k, d) in &t.extra {
        l.sistema.push(valore_da(k, d, Posto::Altro));
    }

    // I gruppi, uno per bersaglio.
    let vol = volume(&t.blocco);
    let mut gruppi: Vec<Gruppo> = Vec::new();
    let mut posti: Vec<(usize, usize)> = Vec::new();
    let mut elementi: Vec<Elemento> = Vec::new();
    for v in &voci {
        let gi = match gruppi.iter().position(|g| g.titolo == v.bersaglio) {
            Some(i) => i,
            None => {
                let mut g = Gruppo { titolo: v.bersaglio.clone(), quando: quando.clone(), pozzetti: Some(t.pozzetti), ..Default::default() };
                if let Some(a) = &t.analista {
                    g.dati.push(("analyst".into(), a.clone()));
                }
                g.dati.push(("experimental data identifier".into(), t.file.clone()));
                g.dati.push(("experiment type".into(), tipo.asm().into()));
                g.dati.push(("container type".into(), "PCR reaction block".into()));
                if let Some(x) = vol {
                    g.dati.push(("well volume".into(), format!("{x}")));
                }
                // Le note della corsa, che lo schema non ha.
                if let Some(d) = &t.durata {
                    g.dati.push(("~total measurement duration setting".into(), d.clone()));
                }
                if let Some(c) = t.coperchio {
                    g.dati.push(("~Cover Temperature".into(), format!("{c}")));
                }
                if let Some(i) = &t.inizio {
                    g.dati.push(("~Run Start Date/Time".into(), i.clone()));
                }
                gruppi.push(g);
                gruppi.len() - 1
            }
        };
        let mut m = Misura {
            campione: v.campione.clone(),
            pozzetto: Some(v.posizione.clone()),
            piastra: t.barcode.clone(),
            luogo: Some(v.id.to_string()),
            bersaglio: Some(v.bersaglio.clone()),
            quando: quando.clone(),
            ..Default::default()
        };
        if let Some(rl) = v.ruolo {
            m.ruolo = rl;
            m.ruolo_dal_file = true;
        }
        // Impostazioni (documento di controllo).
        if let Some(c) = v.cicli {
            m.valori.push(Valore::num("total cycle number setting", c, "", Posto::Impostazione));
        }
        if let Some(c) = &t.chimica {
            m.valori.push(Valore::testo("qPCR detection chemistry", c, Posto::Impostazione));
        }
        if let Some(q) = &v.quencher {
            m.valori.push(Valore::testo("quencher dye setting", q, Posto::Impostazione));
        }
        if let Some(q) = &v.reporter {
            m.valori.push(Valore::testo("reporter dye setting", q, Posto::Impostazione));
        }
        if let Some(q) = &t.passivo {
            m.valori.push(Valore::testo("passive reference dye setting", q, Posto::Impostazione));
        }
        m.valori.push(Valore::testo("measurement method identifier", &t.metodo, Posto::Impostazione));
        // Note sul campione: quello che il file dice e lo schema non ha, e il volume del campione.
        for (k, d) in v.custom.iter().filter(|(k, _)| !matches!(k.as_str(), "Amp Status" | "Tm1" | "Tm2" | "Tm3" | "Tm4")) {
            m.valori.push(valore_da(k, d, Posto::CampioneAltro));
        }
        if let Some(x) = t.vol_campione {
            m.valori.push(Valore::num("sample volume setting", x, "", Posto::CampioneAltro));
        }
        // Dati elaborati.
        match v.ct {
            Some(x) => m.valori.push(Valore::num("cycle threshold result (qPCR)", x, "", Posto::Elaborato)),
            None => {
                // «UNDETERMINED»: allotropy non scrive il dato, Cedro lo fa vedere in tabella e lo tiene tra le note.
                if let Some(c) = &v.ct_testo {
                    m.valori.push(Valore::testo("cycle threshold result (qPCR)", c, Posto::Altro));
                }
            }
        }
        if let Some(x) = v.rn {
            m.valori.push(Valore::num("normalized reporter result", x, "", Posto::Elaborato));
        }
        if let Some(x) = v.drn {
            m.valori.push(Valore::num("baseline corrected reporter result", x, "", Posto::Elaborato));
        }
        if let Some(c) = &v.chiamata {
            m.valori.push(Valore::testo("genotyping determination result", c, Posto::Elaborato));
        }
        for k in ["Amp Status", "Tm1", "Tm2", "Tm3", "Tm4"] {
            if let Some((_, d)) = v.custom.iter().find(|(n, _)| n == k) {
                m.valori.push(valore_da(k, d, Posto::ElaboratoAltro));
            }
        }
        // Elaborazione: soglia, linea di base e come l'ha scelta il programma.
        m.valori.push(match v.soglia {
            Some(x) => Valore::num("cycle threshold value setting (qPCR)", x, "", Posto::Elaborazione),
            None => Valore { chiave: "cycle threshold value setting (qPCR)".into(), dato: Dato::Assente(String::new()), unita: String::new(), posto: Posto::Elaborazione },
        });
        if let Some(b) = si_no_py(v.auto_soglia) {
            m.valori.push(Valore::bool("automatic cycle threshold enabled setting", b, Posto::Elaborazione));
        }
        if let Some(b) = si_no_py(v.auto_base) {
            m.valori.push(Valore::bool("automatic baseline determination enabled setting", b, Posto::Elaborazione));
        }
        if let Some(x) = v.base_ini {
            m.valori.push(Valore::num("baseline determination start cycle setting", x as f64, "#", Posto::Elaborazione));
        }
        if let Some(x) = v.base_fine {
            m.valori.push(Valore::num("baseline determination end cycle setting", x as f64, "#", Posto::Elaborazione));
        }
        if let Some(x) = v.metodo_geno {
            m.valori.push(Valore::num("genotyping determination method setting", x, "", Posto::Elaborazione));
        }
        // Come e quando il programma ha elaborato i dati.
        if let Some(r) = &r_bersaglio {
            m.valori.push(Valore::testo("reference dna description", r, Posto::ElaborazioneAltro));
        }
        if let Some(r) = &r_campione {
            m.valori.push(Valore::testo("reference sample description", r, Posto::ElaborazioneAltro));
        }
        if let Some(a) = &t.analisi {
            m.valori.push(Valore::testo("data processing time", a, Posto::ElaborazioneAltro));
        }
        if let Some(a) = &t.esportato {
            m.valori.push(Valore::testo("Exported On", a, Posto::ElaborazioneAltro));
        }
        for k in ["prfdrop", "BADROX", "CQCONF", "NOISE", "OUTLIERRG"] {
            let d = v.custom.iter().find(|(n, _)| n == k).map(|(_, d)| d.clone()).unwrap_or_else(|| Dato::Testo("N".into()));
            m.valori.push(valore_da(k, &d, Posto::ElaborazioneAltro));
        }
        let omit = v.custom.iter().find(|(n, _)| n == "Omit").map(|(_, d)| d.clone()).unwrap_or(Dato::Bool(false));
        m.valori.push(valore_da("Omit", &omit, Posto::ElaborazioneAltro));
        for k in ["Analysis Type", "RQ Min/Max Confidence Level"] {
            if let Some((_, d)) = t.extra.iter().find(|(n, _)| n == k) {
                m.valori.push(valore_da(k, d, Posto::ElaborazioneAltro));
            }
        }
        // L'elemento per i calcoli.
        let mut e = Elemento::nuovo(&format!("{}", elementi.len()));
        e.metti_testo("target_dna_description", Some(&v.bersaglio));
        e.metti_testo("sample_identifier", Some(&v.campione));
        e.metti_testo("sample_role_type", ruolo_asm(v.ruolo));
        e.metti_num("cycle_threshold_result", v.ct);
        e.metti_num("normalized_reporter_result", v.rn);
        for (k, x) in &v.numeri {
            e.metti_num(k, Some(*x));
        }
        elementi.push(e);
        posti.push((gi, gruppi[gi].misure.len()));
        gruppi[gi].misure.push(m);
    }

    // Gli identificativi, nell'ordine dei gruppi.
    for g in gruppi.iter_mut() {
        for m in g.misure.iter_mut() {
            m.id = l.id();
        }
    }
    let id_di = |e: usize| gruppi[posti[e].0].misure[posti[e].1].id.clone();

    // Quello che il programma calcola sulle repliche.
    let docs = calcoli(tipo, &elementi, r_campione.as_deref(), r_bersaglio.as_deref());
    for d in &docs {
        let mut sorgenti: Vec<String> = Vec::new();
        let mut da = String::new();
        let mut altri: Vec<usize> = Vec::new();
        for fonte in &d.fonti {
            match fonte {
                cd::Fonte::Misura { elemento, feature } => {
                    sorgenti.push(id_di(*elemento));
                    if da.is_empty() {
                        da = feature.clone();
                    }
                }
                cd::Fonte::Calcolo { posizione, .. } => altri.push(*posizione),
            }
        }
        l.calcolati.push(Calcolato { nome: d.nome.clone(), valore: d.valore, unita: String::new(), sorgenti, da, descrizione: String::new(), da_calcoli: altri });
    }
    l.gruppi = gruppi;
    Ok(())
}

/// Il campione e il bersaglio di riferimento della quantificazione relativa, dal foglio dei gruppi.
fn riferimenti(libro: &Libro) -> Result<(Option<String>, Option<String>), String> {
    let t = libro.tab_piena("RQ Replicate Group Result")?;
    let (Some(rq), Some(si), Some(ti)) = (t.idx("Rq"), t.idx("Sample"), t.idx("Target")) else { return Err("nel foglio RQ Replicate Group Result mancano le colonne Rq, Sample e Target".into()) };
    if !t.righe.iter().any(|r| r.get(rq).is_some_and(|c| c.is_some())) {
        return Ok((None, None));
    }
    // Il campione con Rq uguale a 1.
    let mut campioni: Vec<Option<String>> = Vec::new();
    for r in &t.righe {
        if r.get(rq).cloned().flatten().and_then(|x| numero(&x)) == Some(1.0) {
            let s = r.get(si).cloned().flatten();
            if !campioni.contains(&s) {
                campioni.push(s);
            }
        }
    }
    if campioni.len() != 1 {
        return Err("non si capisce qual è il campione di riferimento: nel foglio RQ Replicate Group Result una sola riga deve avere Rq uguale a 1".into());
    }
    let r_campione = campioni.remove(0).unwrap_or_else(|| "None".into());
    // Il bersaglio senza Rq in tutti i campioni.
    let mut per_campione: Vec<(String, Vec<String>)> = Vec::new();
    for r in &t.righe {
        let Some(Some(s)) = r.get(si) else { continue };
        let senza_rq = r.get(rq).is_none_or(|c| c.is_none());
        let voce = match per_campione.iter_mut().find(|(k, _)| k == s) {
            Some(v) => v,
            None => {
                per_campione.push((s.clone(), Vec::new()));
                per_campione.last_mut().unwrap()
            }
        };
        if senza_rq {
            if let Some(Some(tg)) = r.get(ti) {
                if !voce.1.contains(tg) {
                    voce.1.push(tg.clone());
                }
            }
        }
    }
    let mut comuni: Vec<String> = per_campione.first().map(|(_, v)| v.clone()).unwrap_or_default();
    for (_, v) in per_campione.iter().skip(1) {
        comuni.retain(|x| v.contains(x));
    }
    match comuni.len() {
        0 => Ok((Some(r_campione), None)),
        1 => Ok((Some(r_campione), comuni.pop())),
        _ => Err("non si capisce qual è il bersaglio di riferimento: nel foglio RQ Replicate Group Result deve essercene uno solo senza Rq".into()),
    }
}
