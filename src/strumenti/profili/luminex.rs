//! Luminex xPONENT e INTELLIFLEX: il CSV (o l'Excel) che il programma esporta dopo una lettura multiplex a sfere
//! (MAGPIX, LX 200, FLEXMAP 3D, INTELLIFLEX). Ci sono due forme:
//!
//! - l'export a tabelle («DataType: Median», «DataType: Count»…): l'intestazione della corsa, le calibrazioni e
//!   una tabella per grandezza con una riga per pozzetto e una colonna per analita;
//! - l'export piatto (una riga per pozzetto, colonne «R25: RP1 MEDIAN»…), anche come foglio «Results» di Excel.
//!
//! Ogni pozzetto è una misura con i suoi analiti: sfera, conteggio, statistiche della fluorescenza (mediana,
//! media, CV…) e i valori che il programma ha calcolato (Net MFI, concentrazione…). I valori che mancano o non sono
//! numeri si scrivono come mancanti, con l'errore che dice quale.

use std::collections::{HashMap, HashSet};

use crate::strumenti::fonte::{dividi_riga, numero, Fonte};
use crate::strumenti::{tempo, Analita, CalcoloAnalita, Calibrazione, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Statistica, Tecnica, Valore};

pub const XPONENT: Profilo = Profilo { id: "luminex_xponent", nome: "Luminex xPONENT", produttore: "Luminex", tecnica: Tecnica::Multiplex, estensioni: &["csv", "xlsx"], riconosci: riconosci_xponent, leggi: leggi_xponent };
pub const INTELLIFLEX: Profilo = Profilo { id: "luminex_intelliflex", nome: "Luminex INTELLIFLEX", produttore: "Luminex", tecnica: Tecnica::Multiplex, estensioni: &["csv", "xlsx"], riconosci: riconosci_intelliflex, leggi: leggi_intelliflex };

const SOFTWARE_XPONENT: &str = "xPONENT";
const SOFTWARE_INTELLIFLEX: &str = "INTELLIFLEX";

/// Le statistiche della fluorescenza: la tabella, il ruolo in Allotrope e l'unità.
const STATISTICHE: &[(&str, &str, &str)] = &[
    ("Median", "median", "RFU"),
    ("Mean", "arithmetic mean", "RFU"),
    ("%CV", "coefficient of variation", "(unitless)"),
    ("Standard Deviation", "standard deviation", "(unitless)"),
    ("Peak", "mode value", "RFU"),
    ("Trimmed Peak", "trimmed mode value", "RFU"),
    ("Trimmed Count", "trimmed count", "#"),
    ("Trimmed Standard Deviation", "trimmed standard deviation", "(unitless)"),
    ("Trimmed Mean", "trimmed arithmetic mean", "RFU"),
];

/// Altri nomi con cui il programma scrive le stesse tabelle.
const ALIAS: &[(&str, &str)] = &[("Std Dev", "Standard Deviation"), ("Trimmed Std Dev", "Trimmed Standard Deviation"), ("% CV", "%CV")];

/// I valori che il programma calcola per analita: la tabella e l'unità.
const CALCOLATI: &[(&str, &str)] = &[
    ("Net MFI", "RFU"),
    ("Avg Net MFI", "RFU"),
    ("Result", "(unitless)"),
    ("Avg Result", "(unitless)"),
    ("Test Result", "(unitless)"),
    ("% Recovery", "(unitless)"),
    ("%CV of Replicates", "(unitless)"),
    ("Background", "RFU"),
    ("Net Normalized Median", "RFU"),
    ("Net Average Median", "RFU"),
    ("Net Normalized Average Median", "RFU"),
    ("Ratio", "(unitless)"),
    ("Trimmed %CV", "(unitless)"),
];

/// Le tabelle che il file ha e questo lettore non usa come dati dei pozzetti (o che usa in un altro modo).
const ALTRE_TABELLE: &[&str] = &["Count", "Units", "Dilution Factor", "Warnings/Errors", "Comments", "Standard Expected Concentration", "Control Expected Concentration"];

/// Le colonne di ogni pozzetto dell'export piatto che non sono analiti.
const EXTRA_POZZETTO: &[&str] = &["WELL TYPE", "WELL STATUS", "WELL ACQUISITION START", "WELL ACQUISITION END", "TOTAL ACTIVE EVENTS", "TOTAL CLASSIFIED EVENTS", "TOTAL GATED EVENTS", "COUNT %CV", "ACQUISITION DURATION"];

/// Di quelle, le colonne che sono numeri.
const EXTRA_NUMERI: &[&str] = &["TOTAL ACTIVE EVENTS", "TOTAL CLASSIFIED EVENTS", "TOTAL GATED EVENTS", "COUNT %CV", "ACQUISITION DURATION"];

/// Le parole che compongono il nome di una grandezza nelle colonne dell'export piatto («NET NORMALIZED MEDIAN»).
const PAROLE_METRICA: &[&str] = &[
    "MEAN", "MEDIAN", "COUNT", "PEAK", "STDEV", "STANDARD", "DEVIATION", "%CV", "TRIMMED", "NET", "MFI", "AVERAGE", "AVG", "RMS", "MODE", "TRMEAN", "TRIMSTDEV", "NORMALIZED", "DILUTION", "FACTOR", "SETTING", "UNITS", "ANALYTE", "NAME", "REGION", "BACKGROUND", "REPLICATE",
    "REPLICATES", "RATIO", "RESULT", "RECOVERY", "RANGE", "COMMENTS", "EXPECTED", "CONCENTRATION",
];

/// Le colonne fisse dell'export piatto senza il prefisso «R25: RP1».
const COLONNE_FISSE: &[&str] = &["INSTRUMENT TYPE", "SERIAL NUMBER", "SOFTWARE VERSION", "PLATE NAME", "PLATE START", "WELL LOCATION", "SAMPLE ID"];

/// Le chiavi dell'intestazione che hanno già un posto nella lettura.
const LETTE: &[&str] = &[
    "Program", "ProtocolPlate", "Date", "ModelName", "SampleVolume", "MaxSampleUptakeVolume", "Build", "SN", "ProtocolName", "ProtocolVersion", "Batch", "BatchStartTime", "ProtocolReporterGain", "ProtocolOperatingMode", "ComputerName", "Operator", "Country Code", "ProtocolDevelopingCompany", "Version", "BatchStopTime",
    "ProtocolDescription",
];

/// Le impostazioni dell'intestazione che si ripetono su ogni pozzetto: dello strumento e del campione.
const IMPOSTAZIONI_POZZETTO: &[&str] = &["ProtocolHeater", "DDGate", "SampleTimeout", "ProtocolAnalysis", "ProtocolMicrosphere", "PlateReadDirection"];
const CAMPIONE_POZZETTO: &[&str] = &["BatchDescription", "PanelName", "BeadType"];

/// I valori che il programma scrive per dire «non c'è» (pandas li legge come vuoti).
const NA: &[&str] = &["", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan", "1.#IND", "1.#QNAN", "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a", "nan", "null"];

fn e_na(t: &str) -> bool {
    NA.contains(&t.trim())
}

// ---------------------------------------------------------------- il riconoscimento

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Forma {
    /// Le tabelle «DataType: …».
    Tabelle,
    /// Una riga per pozzetto.
    Piatta,
}

fn e_piatta(testa: &[String]) -> bool {
    ["INSTRUMENT TYPE", "WELL LOCATION", "SAMPLE ID"].iter().all(|c| testa.iter().any(|t| t.trim() == *c))
}

/// La forma del file e le prime righe che servono per dire che strumento è: l'intestazione e la prima riga di dati
/// (export piatto) o la riga «Program» (tabelle).
fn forma(f: &Fonte) -> Option<(Forma, Vec<String>, Vec<String>)> {
    if f.foglio() {
        let r = f.foglio_nome("Results")?;
        let testa = r.righe.first()?;
        return e_piatta(testa).then(|| (Forma::Piatta, testa.clone(), r.righe.get(1).cloned().unwrap_or_default()));
    }
    let mut righe = f.testo.lines().filter(|l| !l.trim().is_empty());
    let prima = righe.next()?.trim_start_matches('\u{feff}');
    let testa = dividi_riga(prima, ',');
    if e_piatta(&testa) {
        let dati = righe.next().map(|r| dividi_riga(r, ',')).unwrap_or_default();
        return Some((Forma::Piatta, testa, dati));
    }
    if testa.first().is_some_and(|c| c.trim() == "Program") && f.testo.contains("DataType:") {
        return Some((Forma::Tabelle, testa.clone(), testa));
    }
    None
}

/// Il file dice di essere di un INTELLIFLEX (il modello nella riga «Program», o la colonna del tipo di strumento).
fn e_intelliflex(forma: Forma, testa: &[String], riga: &[String]) -> bool {
    match forma {
        Forma::Tabelle => riga.get(3).is_some_and(|m| m.to_ascii_lowercase().contains("intelliflex")),
        Forma::Piatta => ["INSTRUMENT TYPE", "MODEL NAME"].iter().any(|nome| testa.iter().position(|t| t.trim() == *nome).and_then(|i| riga.get(i)).is_some_and(|v| v.to_ascii_lowercase().contains("intelliflex"))),
    }
}

fn riconosci_xponent(f: &Fonte) -> u8 {
    match forma(f) {
        Some((forma, testa, riga)) => match (e_intelliflex(forma, &testa, &riga), forma) {
            (true, _) => 60,
            (false, Forma::Tabelle) => 92,
            (false, Forma::Piatta) => 85,
        },
        None => 0,
    }
}

fn riconosci_intelliflex(f: &Fonte) -> u8 {
    match forma(f) {
        Some((forma, testa, riga)) => match (e_intelliflex(forma, &testa, &riga), forma) {
            (true, Forma::Tabelle) => 93,
            (true, Forma::Piatta) => 94,
            (false, _) => 55,
        },
        None => 0,
    }
}

fn leggi_xponent(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    leggi(f, l, SOFTWARE_XPONENT)
}

fn leggi_intelliflex(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    leggi(f, l, SOFTWARE_INTELLIFLEX)
}

// ---------------------------------------------------------------- le tabelle

/// Una tabella: la prima cella di ogni riga è la chiave (il pozzetto, il campione), le altre stanno sotto le colonne.
#[derive(Default, Debug)]
struct Tabella {
    /// I nomi delle colonne (senza la chiave); vuoto per una colonna senza nome.
    colonne: Vec<String>,
    chiavi: Vec<String>,
    celle: Vec<Vec<String>>,
    per_chiave: HashMap<String, usize>,
    per_colonna: HashMap<String, usize>,
}

impl Tabella {
    /// Un nome ripetuto prende il numero («analita.1»), come fa pandas leggendo il CSV.
    fn nuova(nomi: Vec<String>, righe: Vec<Vec<String>>) -> Tabella {
        let mut t = Tabella::default();
        let mut visti: HashMap<String, usize> = HashMap::new();
        for n in nomi {
            let n = n.trim().to_string();
            if n.is_empty() {
                t.colonne.push(n);
                continue;
            }
            let k = visti.entry(n.clone()).or_insert(0);
            let nome = if *k == 0 { n.clone() } else { format!("{n}.{k}") };
            *k += 1;
            t.per_colonna.entry(nome.clone()).or_insert(t.colonne.len());
            t.colonne.push(nome);
        }
        for mut r in righe {
            if r.is_empty() {
                continue;
            }
            let chiave = r.remove(0).trim().to_string();
            t.per_chiave.entry(chiave.clone()).or_insert(t.chiavi.len());
            t.chiavi.push(chiave);
            r.resize(t.colonne.len(), String::new());
            t.celle.push(r);
        }
        t
    }

    fn cella(&self, chiave: &str, colonna: &str) -> Option<&str> {
        let r = *self.per_chiave.get(chiave)?;
        let c = *self.per_colonna.get(colonna)?;
        Some(self.celle[r].get(c).map(String::as_str).unwrap_or(""))
    }

    /// Tutte le righe di una chiave (un pozzetto può avere più messaggi).
    fn righe_di<'a>(&'a self, chiave: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.chiavi.iter().enumerate().filter(move |(_, k)| k.as_str() == chiave).map(|(i, _)| i)
    }
}

/// Una calibrazione come sta nel file, prima di mettere le date in ISO.
struct CalGrezza {
    nome: String,
    esito: Option<String>,
    quando: String,
    scadenza: Option<String>,
    altro: Vec<(String, String)>,
}

/// Il file letto in forma unica, qualunque fosse la forma.
struct Dati {
    /// L'intestazione: ogni chiave con le celle che le stanno dopo.
    intest: Vec<(String, Vec<String>)>,
    calibrazioni: Vec<CalGrezza>,
    /// Il minimo di eventi per sfera dichiarato, se il file lo dichiara.
    minimo: Option<f64>,
    sezioni: HashMap<String, Tabella>,
    /// I nomi delle tabelle nell'ordine del file.
    nomi: Vec<String>,
    avvisi: Vec<String>,
}

impl Dati {
    fn valori(&self, k: &str) -> Option<&Vec<String>> {
        self.intest.iter().find(|(c, _)| c == k).map(|(_, v)| v)
    }

    /// La prima cella dopo la chiave, se c'è e non dice «non c'è».
    fn primo(&self, k: &str) -> Option<String> {
        self.n(k, 0)
    }

    fn n(&self, k: &str, i: usize) -> Option<String> {
        self.valori(k).and_then(|v| v.get(i)).map(|t| t.trim().to_string()).filter(|t| !e_na(t))
    }
}

/// Una riga senza dati: vuota, o fatta solo di virgole e virgolette.
fn vuota(riga: &str) -> bool {
    riga.chars().all(|c| matches!(c, ',' | '"' | ' ' | '\t'))
}

// ---------------------------------------------------------------- l'export a tabelle

fn carica_tabelle(f: &Fonte) -> Result<Dati, String> {
    let linee: Vec<&str> = f.testo.lines().collect();
    let fine_int = linee.iter().position(|l| l.contains("Most Recent Calibration and Control Results") || l.contains("Most Recent Calibration and Verification Results")).ok_or("nel file manca la sezione «Most Recent Calibration and Control Results»")?;
    let mut avvisi = Vec::new();

    // L'intestazione: una chiave e le sue celle per riga.
    let mut intest: Vec<(String, Vec<String>)> = Vec::new();
    for riga in &linee[..fine_int] {
        let mut c = dividi_riga(riga, ',');
        let chiave = c.remove(0).trim().to_string();
        if !chiave.is_empty() {
            intest.push((chiave, c));
        }
    }

    // Le calibrazioni: «Last <nome>,<esito> <data>» o «Last <nome>,<data>».
    let mut i = fine_int + 1;
    let mut calibrazioni = Vec::new();
    while i < linee.len() && !vuota(linee[i]) {
        let c = dividi_riga(linee[i], ',');
        i += 1;
        if c.len() < 2 {
            return Err(format!("alla riga delle calibrazioni «{}» mi aspettavo almeno due colonne", c.first().map(String::as_str).unwrap_or("")));
        }
        let nome = c[0].replace("Last", "").trim().to_string();
        let info = c[1].trim();
        let prima = info.split_whitespace().next().unwrap_or("");
        let (esito, quando) = if ["Passed", "Failed", "Calibrated", "Verified"].contains(&prima) {
            match info.split_once(char::is_whitespace) {
                Some((e, q)) => (Some(e.to_string()), q.trim().to_string()),
                None => (None, String::new()),
            }
        } else {
            (None, info.to_string())
        };
        if tempo::iso(&quando, false).or_else(|| tempo::iso(&quando, true)).is_none() {
            avvisi.push(format!("la calibrazione «{nome}» ha un risultato che non capisco («{info}»): non l'ho scritta"));
            continue;
        }
        calibrazioni.push(CalGrezza { nome, esito, quando, scadenza: None, altro: Vec::new() });
    }

    // I campioni e gli eventi minimi: «Samples,96,Min Events,100,Per Bead».
    let riga_campioni = (i..linee.len()).find(|&k| {
        let t = linee[k].trim_start();
        t.starts_with("Samples,") || t.starts_with("\"Samples\",")
    });
    let riga_campioni = riga_campioni.ok_or("nel file manca la riga «Samples» con gli eventi minimi")?;
    let campi: Vec<String> = linee[riga_campioni].replace('"', "").split(',').map(str::to_string).collect();
    let minimo = match campi.get(3).map(|t| t.trim()) {
        None => return Err(format!("nella riga «{}» non trovo gli eventi minimi", linee[riga_campioni])),
        Some("") => Some(100.0),
        Some(t) => numero(t),
    };

    // I risultati.
    let inizio = (riga_campioni + 1..linee.len()).find(|&k| linee[k].trim_matches(|c| matches!(c, ',' | '"' | ' ')) == "Results").ok_or("nel file manca la sezione «Results»")?;
    let mut k = inizio + 1;
    let mut sezioni: HashMap<String, Tabella> = HashMap::new();
    let mut nomi = Vec::new();
    while k < linee.len() && vuota(linee[k]) {
        k += 1;
    }
    while k < linee.len() && !linee[k].contains("-- CRC --") {
        let titolo = dividi_riga(linee[k], ',');
        if titolo.first().map(|c| c.trim()) != Some("DataType:") {
            return Err(format!("alla riga {} mi aspettavo il titolo di una tabella («DataType:»), c'è «{}»", k + 1, linee[k]));
        }
        let nome = titolo.get(1).map(|t| t.trim().to_string()).unwrap_or_default();
        k += 1;
        let mut righe: Vec<Vec<String>> = Vec::new();
        while k < linee.len() && !vuota(linee[k]) {
            righe.push(dividi_riga(linee[k], ','));
            k += 1;
        }
        if !righe.is_empty() {
            let mut testa = righe.remove(0);
            testa.remove(0);
            nomi.push(nome.clone());
            sezioni.insert(nome, Tabella::nuova(testa, righe));
        }
        while k < linee.len() && vuota(linee[k]) {
            k += 1;
        }
    }
    Ok(Dati { intest, calibrazioni, minimo, sezioni, nomi, avvisi })
}

// ---------------------------------------------------------------- l'export piatto

/// «R25: RP1 MEDIAN»: il numero della regione e del reporter, uno spazio e la grandezza.
fn e_colonna_analita(c: &str) -> bool {
    let c = c.trim();
    let Some(r) = c.strip_prefix('R') else { return false };
    let cifre = r.chars().take_while(char::is_ascii_digit).count();
    let Some(r) = r[cifre..].strip_prefix(':').filter(|_| cifre > 0) else { return false };
    let senza = r.trim_start();
    let Some(r) = senza.strip_prefix("RP").filter(|_| senza.len() < r.len()) else { return false };
    let cifre = r.chars().take_while(char::is_ascii_digit).count();
    cifre > 0 && r[cifre..].starts_with(char::is_whitespace)
}

/// Divide il nome di una colonna in analita e grandezza: la grandezza è la coda più lunga fatta solo di parole note.
fn dividi_colonna(col: &str) -> Option<(String, String)> {
    let parti: Vec<&str> = col.trim().split(' ').filter(|p| !p.is_empty()).collect();
    if parti.len() < 2 {
        return None;
    }
    let parola = |p: &str| {
        let u = p.to_uppercase();
        PAROLE_METRICA.contains(&u.as_str()) || (u.starts_with('(') && u.ends_with(')'))
    };
    let i = (1..parti.len()).find(|&i| parti[i..].iter().all(|p| parola(*p)))?;
    Some((parti[..i].join(" "), parti[i..].join(" ")))
}

/// Il nome della tabella per una grandezza dell'export piatto.
fn nome_tabella(metrica: &str) -> String {
    let u = metrica.to_uppercase();
    for (suffisso, nome) in [("AVERAGE MFI", "Avg Net MFI"), ("REPLICATE %CV", "%CV of Replicates"), ("NET NORMALIZED MEDIAN", "Net Normalized Median"), ("NET MEDIAN", "Net MFI"), ("TRIMMED %CV", "Trimmed %CV"), ("%CV", "%CV")] {
        if u.trim().ends_with(suffisso) {
            return nome.to_string();
        }
    }
    // Senza la parentesi in coda («RATIO (RP1/RP2)» → «Ratio»).
    let base = match metrica.rfind('(') {
        Some(p) if metrica.trim_end().ends_with(')') => metrica[..p].trim(),
        _ => metrica.trim(),
    };
    titolo(base)
}

/// «NET AVERAGE MEDIAN» → «Net Average Median».
fn titolo(t: &str) -> String {
    t.split(' ')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(a) => a.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Le righe di un export piatto a 96 o 384 pozzetti.
fn pozzetti_della_piastra(posizioni: &[&str]) -> f64 {
    let valide: HashSet<String> = posizioni
        .iter()
        .map(|p| p.trim().to_uppercase())
        .filter(|p| {
            let lettere = p.chars().take_while(char::is_ascii_alphabetic).count();
            lettere > 0 && p.len() > lettere && p[lettere..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    if valide.len() <= 96 {
        96.0
    } else {
        384.0
    }
}

/// Il nome del computer dal nome del file: i file esportati su una cartella di rete si chiamano come il suo percorso
/// con i separatori sostituiti da trattini bassi («____uspltmrlwg00060_results___piastra.xlsx»).
fn computer_dal_nome(nome: &str) -> String {
    let sotto = nome.chars().take_while(|&c| c == '_').count();
    if sotto < 2 {
        return String::new();
    }
    let resto = &nome[sotto..];
    match resto.find('_') {
        Some(p) if p > 0 => resto[..p].to_string(),
        _ => String::new(),
    }
}

fn carica_piatto(f: &Fonte) -> Result<Dati, String> {
    let mut righe: Vec<Vec<String>> = if f.foglio() { f.foglio_nome("Results").map(|s| s.righe.clone()).ok_or("nel file Excel manca il foglio «Results»")? } else { f.righe_con(',') };
    // La riga della firma elettronica in fondo non è un dato.
    if let Some(p) = righe.iter().position(|r| r.first().is_some_and(|c| c.trim().starts_with("--DOCUMENT SIGNATURE--"))) {
        righe.truncate(p);
    }
    let testa: Vec<String> = righe.first().ok_or("il file è vuoto")?.iter().map(|c| c.trim().to_string()).collect();
    let dati: Vec<&Vec<String>> = righe[1..].iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    if dati.is_empty() {
        return Err("nel file non ci sono pozzetti".into());
    }
    let mut indice: HashMap<&str, usize> = HashMap::new();
    for (i, c) in testa.iter().enumerate() {
        indice.entry(c.as_str()).or_insert(i);
    }
    let cella = |r: &[String], nome: &str| -> String { indice.get(nome).and_then(|&i| r.get(i)).map(|t| t.trim().to_string()).unwrap_or_default() };
    let primo = |nome: &str| -> String {
        let v = cella(dati[0], nome);
        if e_na(&v) {
            String::new()
        } else {
            v
        }
    };
    for c in ["WELL LOCATION", "SAMPLE ID"] {
        if !indice.contains_key(c) {
            return Err(format!("nel file manca la colonna «{c}»"));
        }
    }
    let posizioni: Vec<String> = dati.iter().map(|r| cella(r, "WELL LOCATION")).collect();
    let campioni: Vec<String> = dati.iter().map(|r| cella(r, "SAMPLE ID")).collect();
    let chiavi: Vec<String> = posizioni.iter().map(|p| format!("1(1,{p})")).collect();

    // Gli analiti, le grandezze e le colonne che le portano.
    let col_prefissate = testa.iter().any(|c| e_colonna_analita(c));
    let mut analiti: Vec<String> = Vec::new();
    let mut mappa: HashMap<(String, String), usize> = HashMap::new();
    let mut grandezze: Vec<String> = Vec::new();
    let mut meta: HashMap<(String, String), usize> = HashMap::new();
    for (i, c) in testa.iter().enumerate() {
        if col_prefissate {
            if !e_colonna_analita(c) {
                continue;
            }
        } else if COLONNE_FISSE.contains(&c.as_str()) {
            continue;
        }
        let Some((analita, metrica)) = dividi_colonna(c) else { continue };
        let m = metrica.to_uppercase();
        if m == "ANALYTE NAME" || m == "REGION" {
            meta.insert((analita, m), i);
            continue;
        }
        mappa.insert((analita.clone(), metrica.clone()), i);
        if m.ends_with("COUNT") && !analiti.contains(&analita) {
            analiti.push(analita);
        }
        if !grandezze.contains(&metrica) {
            grandezze.push(metrica);
        }
    }
    if analiti.is_empty() {
        return Err("nel file non trovo nessun analita: mi aspettavo colonne come «R25: RP1 COUNT»".into());
    }
    let numero_cella = |r: &[String], i: usize| -> String {
        let t = r.get(i).map(|t| t.trim()).unwrap_or("");
        if e_na(t) {
            "NaN".to_string()
        } else {
            t.to_string()
        }
    };

    // I nomi veri degli analiti e le loro sfere, se il file li dà.
    let primo_valido = |i: usize| dati.iter().filter_map(|r| r.get(i)).map(|t| t.trim()).find(|t| !e_na(t)).map(str::to_string);
    let mut nomi_analiti: Vec<String> = Vec::new();
    let mut sfere: Vec<Option<String>> = Vec::new();
    let mut usati: HashSet<String> = HashSet::new();
    for a in &analiti {
        let nome = meta.get(&(a.clone(), "ANALYTE NAME".into())).and_then(|&i| primo_valido(i)).filter(|n| n != a);
        let regione = meta.get(&(a.clone(), "REGION".into())).and_then(|&i| primo_valido(i)).map(|r| match numero(&r) {
            Some(x) if x.fract() == 0.0 => format!("{}", x as i64),
            _ => r,
        });
        let mut scelto = nome.unwrap_or_else(|| a.clone());
        if usati.contains(&scelto) {
            scelto = if usati.contains(a) { format!("{scelto} {a}") } else { a.clone() };
        }
        usati.insert(scelto.clone());
        nomi_analiti.push(scelto);
        sfere.push(regione);
    }
    let con_meta = !meta.is_empty();

    // Una tabella per grandezza.
    let mut sezioni: HashMap<String, Tabella> = HashMap::new();
    let mut nomi: Vec<String> = Vec::new();
    for g in &grandezze {
        if g.to_uppercase().trim() == "UNITS" {
            continue;
        }
        let colonne_g: Vec<Option<usize>> = analiti.iter().map(|a| mappa.get(&(a.clone(), g.clone())).copied()).collect();
        if colonne_g.iter().all(Option::is_none) {
            continue;
        }
        let e_conteggio = g == "COUNT";
        let mut nomi_col: Vec<String> = vec!["Sample".into()];
        nomi_col.extend(nomi_analiti.iter().cloned());
        nomi_col.push("Total Events".into());
        if e_conteggio {
            nomi_col.extend(EXTRA_POZZETTO.iter().filter(|c| indice.contains_key(**c)).map(|c| c.to_string()));
        }
        let mut tab_righe: Vec<Vec<String>> = Vec::new();
        for (ri, r) in dati.iter().enumerate() {
            let mut c: Vec<String> = vec![chiavi[ri].clone(), campioni[ri].clone()];
            let valori: Vec<String> = colonne_g.iter().map(|i| i.map(|i| numero_cella(r, i)).unwrap_or_default()).collect();
            c.extend(valori.iter().cloned());
            if e_conteggio {
                let totale = match indice.get("TOTAL EVENTS") {
                    Some(&i) => numero_cella(r, i),
                    None => format!("{}", valori.iter().filter_map(|v| numero(v)).sum::<f64>()),
                };
                c.push(totale);
                for extra in EXTRA_POZZETTO.iter().filter(|c| indice.contains_key(**c)) {
                    let v = cella(r, extra);
                    c.push(if e_na(&v) { String::new() } else { v });
                }
            } else {
                c.push(String::new());
            }
            tab_righe.push(c);
        }
        let nome = nome_tabella(g);
        nomi.push(nome.clone());
        sezioni.insert(nome, Tabella::nuova(nomi_col, tab_righe));
    }
    // La diluizione.
    let diluizioni: Vec<Vec<String>> = dati.iter().enumerate().map(|(ri, r)| vec![chiavi[ri].clone(), campioni[ri].clone(), cella(r, "DILUTION")]).collect();
    sezioni.insert("Dilution Factor".into(), Tabella::nuova(vec!["Sample".into(), "Dilution Factor".into()], diluizioni.into_iter().map(|mut r| {
        if e_na(&r[2]) {
            r[2].clear();
        }
        r
    }).collect()));
    // Le sfere.
    let mut righe_unita = vec![vec!["BeadID:".to_string()]];
    if con_meta {
        righe_unita[0].extend(sfere.iter().map(|s| s.clone().unwrap_or_else(|| "N/A".into())));
    }
    righe_unita.push(vec!["Units:".to_string()]);
    sezioni.insert("Units".into(), Tabella::nuova(if con_meta { nomi_analiti.clone() } else { Vec::new() }, righe_unita));

    // L'intestazione.
    let mut intest: Vec<(String, Vec<String>)> = vec![("Program".into(), vec![SOFTWARE_XPONENT.into()])];
    let modello = primo("MODEL NAME");
    if !modello.is_empty() {
        intest.push(("ModelName".into(), vec![modello]));
    }
    let versione = primo("SOFTWARE VERSION");
    intest.push(("Build".into(), vec![versione]));
    intest.push(("SN".into(), vec![primo("SERIAL NUMBER")]));
    let n_pozzetti = pozzetti_della_piastra(&posizioni.iter().map(String::as_str).collect::<Vec<_>>());
    intest.push(("ProtocolPlate".into(), vec!["Name".into(), String::new(), "Type".into(), format!("{n_pozzetti}")]));
    intest.push(("ComputerName".into(), vec![computer_dal_nome(&f.nome)]));
    intest.push(("BatchStartTime".into(), vec![primo("PLATE START")]));
    for (colonna, chiave) in [
        ("PLATE NAME", "Batch"),
        ("PROTOCOL NAME", "ProtocolName"),
        ("PROTOCOL VERSION", "ProtocolVersion"),
        ("PANEL NAME", "PanelName"),
        ("BEAD TYPE", "BeadType"),
        ("PLATE END", "BatchStopTime"),
        ("PLATE STATUS", "PlateStatus"),
        ("PLATE TYPE NAME", "PlateTypeName"),
        ("UPTAKE VOLUME", "MaxSampleUptakeVolume"),
        ("ACTIVE USER", "Operator"),
        ("AUTHORIZED USER", "AuthorizedUser"),
        ("SIGNING USER", "SigningUser"),
        ("PLATE GEOMETRY", "PlateGeometry"),
        ("NORMALIZATION REGION", "NormalizationRegion"),
        ("DD GATE LOW", "DDGateLow"),
        ("DD GATE HIGH", "DDGateHigh"),
        ("PLATE HEATER TEMP", "PlateHeaterTemp"),
        ("OPERATING MODE", "OperatingMode"),
    ] {
        let v = primo(colonna);
        if !v.is_empty() {
            intest.push((chiave.into(), vec![v]));
        }
    }

    // Le calibrazioni e le verifiche.
    let mut calibrazioni = Vec::new();
    for (nome, stato, quando, lotto, scadenza, scaduto) in [
        ("Calibration", "CALIBRATION STATE", "CALIBRATION TIMESTAMP", "CALIBRATION LOT", "CALIBRATION LOT EXPIRATION", "CALIBRATION LOT EXPIRED"),
        ("Verification", "VERIFICATION STATE", "VERIFICATION TIMESTAMP", "VERIFICATION LOT", "VERIFICATION LOT EXPIRATION", "VERIFICATION LOT EXPIRED"),
        ("Fluidics Verification 1", "FLUIDICS VER STATE", "FLUIDICS VER TIMESTAMP", "FLUIDICS 1 VER LOT", "FLUIDICS 1 VER LOT EXPIRATION", "FLUIDICS 1 VER LOT EXPIRED"),
        ("Fluidics Verification 2", "FLUIDICS VER STATE", "FLUIDICS VER TIMESTAMP", "FLUIDICS 2 VER LOT", "FLUIDICS 2 VER LOT EXPIRATION", "FLUIDICS 2 VER LOT EXPIRED"),
    ] {
        let q = primo(quando);
        if q.is_empty() {
            continue;
        }
        let mut altro = Vec::new();
        let lot = primo(lotto);
        if !lot.is_empty() {
            altro.push(("lot number".to_string(), lot));
        }
        let sc = primo(scaduto);
        if !sc.is_empty() {
            let normale = match sc.to_ascii_lowercase().as_str() {
                "true" => "True".to_string(),
                "false" => "False".to_string(),
                _ => sc,
            };
            altro.push(("lot expired".to_string(), normale));
        }
        calibrazioni.push(CalGrezza { nome: nome.into(), esito: Some(primo(stato)).filter(|s| !s.is_empty()), quando: q, scadenza: Some(primo(scadenza)).filter(|s| !s.is_empty()), altro });
    }
    Ok(Dati { intest, calibrazioni, minimo: None, sezioni, nomi, avvisi: Vec::new() })
}

// ---------------------------------------------------------------- la lettura

/// L'ordine delle date: il giorno viene prima del mese solo se una data lo dice (una cifra sopra 12); altrimenti è
/// il formato americano dei programmi Luminex.
fn giorno_prima(date: &[&str]) -> bool {
    for d in date {
        let primo = d.split_whitespace().next().unwrap_or("");
        let sep = if primo.contains('/') {
            '/'
        } else if primo.contains('.') {
            '.'
        } else {
            continue;
        };
        let n: Vec<u32> = primo.split(sep).filter_map(|x| x.parse().ok()).collect();
        if n.len() == 3 {
            if (13..=31).contains(&n[0]) {
                return true;
            }
            if (13..=31).contains(&n[1]) {
                return false;
            }
        }
    }
    false
}

/// «1(1,A1)» → («1,A1», «A1»): il pozzetto come lo chiamano i messaggi e come lo chiama la piastra.
fn posizione(loc: &str) -> Option<(String, String)> {
    let mut da = 0;
    while let Some(p) = loc[da..].find('(').map(|p| p + da) {
        da = p + 1;
        // Prima della parentesi un numero.
        if !loc[..p].chars().next_back().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some(q) = loc[p..].find(')').map(|q| q + p) else { return None };
        let dentro = &loc[p + 1..q];
        if let Some((a, b)) = dentro.split_once(',') {
            if !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()) && !b.is_empty() && b.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some((format!("{a},{b}"), b.to_string()));
            }
        }
    }
    None
}

fn leggi(f: &Fonte, l: &mut Lettura, software: &str) -> Result<(), String> {
    let (forma, _, _) = forma(f).ok_or("il file non ha la forma di un export di xPONENT o INTELLIFLEX")?;
    let d = match forma {
        Forma::Tabelle => carica_tabelle(f)?,
        Forma::Piatta => carica_piatto(f)?,
    };
    costruisci(d, l, software)
}

fn costruisci(d: Dati, l: &mut Lettura, software: &str) -> Result<(), String> {
    let mancanti: Vec<&str> = ["Count", "Units", "Dilution Factor"].into_iter().filter(|s| !d.sezioni.contains_key(*s)).collect();
    if !mancanti.is_empty() {
        return Err(format!("nel file mancano le tabelle: {}", mancanti.join(", ")));
    }
    // Le tabelle delle statistiche (con i nomi alternativi).
    let cerca = |nome: &str| d.sezioni.get(nome).or_else(|| ALIAS.iter().filter(|(_, canonico)| *canonico == nome).find_map(|(alias, _)| d.sezioni.get(*alias)));
    let statistiche: Vec<(&str, &str, &Tabella)> = STATISTICHE.iter().filter_map(|(nome, ruolo, unita)| cerca(*nome).map(|t| (*ruolo, *unita, t))).collect();
    if statistiche.is_empty() {
        return Err("nel file non c'è nessuna tabella di statistiche (Median, Mean, %CV…)".into());
    }
    let conteggi = &d.sezioni["Count"];
    let sfere = &d.sezioni["Units"];
    if !sfere.per_chiave.contains_key("BeadID:") {
        return Err("nella tabella «Units» manca la riga «BeadID:»".into());
    }
    let diluizioni = &d.sezioni["Dilution Factor"];
    let avvisi_e_errori = d.sezioni.get("Warnings/Errors");
    let commenti = d.sezioni.get("Comments");
    let attese_standard = d.sezioni.get("Standard Expected Concentration");
    let attese_controllo = d.sezioni.get("Control Expected Concentration");
    let col_campione = conteggi.per_colonna.get("Sample").copied().ok_or("nella tabella «Count» manca la colonna «Sample»")?;
    let col_totale = conteggi.per_colonna.get("Total Events").copied().ok_or("nella tabella «Count» manca la colonna «Total Events»")?;
    let metadati: HashSet<&str> = ["Sample", "Total Events"].into_iter().chain(EXTRA_POZZETTO.iter().copied()).collect();
    let analiti_nomi: Vec<String> = conteggi.colonne.iter().filter(|c| !c.is_empty() && !metadati.contains(c.as_str())).cloned().collect();
    let extra_col: Vec<(usize, &String)> = conteggi.colonne.iter().enumerate().filter(|(_, c)| EXTRA_POZZETTO.contains(&c.as_str())).collect();

    // L'intestazione.
    let modello = d.primo("ModelName").or_else(|| d.n("Program", 2));
    let pozzetti = d.n("ProtocolPlate", 3).and_then(|t| numero(&t)).ok_or("l'intestazione non dice quanti pozzetti ha la piastra («ProtocolPlate»)")?;
    let inizio = d.primo("BatchStartTime").ok_or("nell'intestazione manca «BatchStartTime»")?;
    let fine = d.primo("BatchStopTime");
    let mut date: Vec<&str> = vec![inizio.as_str()];
    if let Some(f) = &fine {
        date.push(f);
    }
    for c in &d.calibrazioni {
        date.push(&c.quando);
        if let Some(s) = &c.scadenza {
            date.push(s);
        }
    }
    let gp = giorno_prima(&date);
    let iso = |t: &str| tempo::iso(t, gp);
    let quando = iso(&inizio);
    let volume = d.primo("SampleVolume").or_else(|| d.primo("MaxSampleUptakeVolume")).and_then(|t| t.split_whitespace().next().and_then(numero));
    let guadagno = d.primo("ProtocolReporterGain").or_else(|| d.primo("ProtocolOperatingMode"));
    let nome_piastra = d.primo("Batch");

    // Le calibrazioni.
    for c in &d.calibrazioni {
        let Some(q) = iso(&c.quando) else {
            l.avvisi.push(format!("la data «{}» della calibrazione «{}» non la capisco: non l'ho scritta", c.quando, c.nome));
            continue;
        };
        l.calibrazioni.push(Calibrazione {
            nome: c.nome.clone(),
            quando: q,
            esito: c.esito.clone(),
            scadenza: c.scadenza.as_deref().and_then(&iso),
            altro: c.altro.iter().map(|(k, v)| Valore::testo(k, v, Posto::Altro)).collect(),
        });
    }
    l.avvisi.extend(d.avvisi.iter().cloned());

    // Le impostazioni che si ripetono su ogni pozzetto.
    let impostazioni: Vec<Valore> = IMPOSTAZIONI_POZZETTO.iter().filter_map(|k| d.primo(k).map(|v| Valore::testo(k, &v, Posto::ImpostazioneAltro))).collect();
    let del_campione: Vec<Valore> = CAMPIONE_POZZETTO.iter().filter_map(|k| d.primo(k).map(|v| Valore::testo(k, &v, Posto::CampioneAltro))).collect();

    // I pozzetti.
    let mut misure: Vec<Misura> = Vec::new();
    for (ri, loc) in conteggi.chiavi.iter().enumerate() {
        let riga = &conteggi.celle[ri];
        let (pozzetto_msg, id_pozzetto) = posizione(loc).ok_or_else(|| format!("il pozzetto «{loc}» non ha una posizione che capisco (mi aspettavo qualcosa come «1(1,A1)»)"))?;
        let campione = riga[col_campione].trim().to_string();
        let mut m = Misura { id: l.id(), campione: campione.clone(), pozzetto: Some(id_pozzetto), quando: quando.clone(), ..Default::default() };

        // I messaggi dello strumento su questo pozzetto.
        if let Some(t) = avvisi_e_errori {
            for r in t.righe_di(&pozzetto_msg) {
                if let Some(msg) = t.per_colonna.get("Message").and_then(|&c| t.celle[r].get(c)) {
                    m.note.push((msg.trim().to_string(), String::new()));
                }
            }
        }

        // Le impostazioni e il campione.
        if let Some(g) = &guadagno {
            m.valori.push(Valore::testo("detector gain setting", g, Posto::Impostazione));
        }
        if let Some(v) = volume {
            m.valori.push(Valore::num("sample volume setting", v, "μL", Posto::Impostazione));
        }
        match diluizioni.cella(loc, "Dilution Factor") {
            Some(t) if numero(t).is_some() => m.valori.push(Valore::num("dilution factor setting", numero(t).unwrap_or(0.0), "(unitless)", Posto::Impostazione)),
            Some(_) | None => {
                if diluizioni.per_chiave.get(loc).is_none() {
                    return Err(format!("nella tabella «Dilution Factor» manca il pozzetto «{loc}»"));
                }
                m.valori.push(Valore { chiave: "dilution factor setting".into(), dato: Dato::Assente("Not reported in file".into()), unita: "(unitless)".into(), posto: Posto::Impostazione });
            }
        }
        if let Some(min) = d.minimo {
            m.valori.push(Valore::num("minimum assay bead count threshold setting", min, "#", Posto::Impostazione));
        }
        m.valori.extend(impostazioni.iter().cloned());
        m.valori.extend(del_campione.iter().cloned());
        match numero(&riga[col_totale]) {
            Some(n) => m.valori.push(Valore::num("assay bead count", n, "#", Posto::Misura)),
            None => m.valori.push(Valore { chiave: "assay bead count".into(), dato: Dato::Assente("Not reported".into()), unita: "#".into(), posto: Posto::Misura }),
        }
        for (c, nome) in &extra_col {
            let t = riga[*c].trim();
            if e_na(t) {
                continue;
            }
            match numero(t).filter(|_| EXTRA_NUMERI.contains(&nome.as_str())) {
                Some(n) => m.valori.push(Valore::num(nome, n, "", Posto::Altro)),
                None => m.valori.push(Valore::testo(nome, t, Posto::Altro)),
            }
        }

        // Gli analiti.
        for a in &analiti_nomi {
            let mut analita = Analita {
                id: l.id(),
                nome: a.clone(),
                sfera: Some(sfere.cella("BeadID:", a).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "N/A".into())),
                // Un «NaN» nel file è un conteggio che lo strumento non ha saputo dare: resta NaN (nell'ASM diventa -0).
                conteggio: conteggi.cella(loc, a).and_then(|t| numero(t).or_else(|| (t.trim().eq_ignore_ascii_case("nan")).then_some(f64::NAN))),
                ..Default::default()
            };
            // Statistiche e valori calcolati: un valore vuoto è «non riportato», uno che non è un numero «NaN».
            let verifica = |grezzo: &str, ruolo: &str, note: &mut Vec<(String, String)>| -> f64 {
                let v = numero(grezzo);
                if grezzo.is_empty() {
                    note.push(("Not reported".into(), format!("{a} <{ruolo}>")));
                } else if v.is_none() {
                    note.push(("NaN".into(), format!("{a} <{ruolo}>")));
                }
                v.unwrap_or(f64::NAN)
            };
            for (ruolo, unita, t) in &statistiche {
                let grezzo = t.cella(loc, a).unwrap_or("").trim();
                let v = verifica(grezzo, &format!("{ruolo} role"), &mut m.note);
                analita.statistiche.push(Statistica { ruolo: (*ruolo).into(), valore: v, unita: (*unita).into() });
            }
            for (nome, unita) in CALCOLATI {
                let Some(t) = d.sezioni.get(*nome) else { continue };
                // Per pozzetto, o per campione se la tabella ne ha una riga sola per campione (le medie delle repliche).
                let chiave = if t.per_chiave.contains_key(loc) {
                    loc.as_str()
                } else if t.per_chiave.contains_key(&campione) {
                    campione.as_str()
                } else {
                    continue;
                };
                let Some(grezzo) = t.cella(chiave, a).map(str::trim) else { continue };
                let v = verifica(grezzo, *nome, &mut m.note);
                analita.calcolati.push(CalcoloAnalita { nome: (*nome).into(), valore: v, unita: (*unita).into() });
            }
            // Il resto: commento, concentrazione attesa, unità.
            if let Some(c) = commenti.and_then(|t| t.cella(loc, a)).map(str::trim).filter(|c| !c.is_empty()) {
                analita.altro.push(Valore::testo("comment", c, Posto::Altro));
            }
            let attesa = |t: &Tabella| t.cella(&campione, a).and_then(numero).filter(|v| *v != 0.0);
            if let Some(t) = attese_standard.filter(|t| t.per_chiave.contains_key(&campione)) {
                if let Some(v) = attesa(t) {
                    analita.altro.push(Valore::num("standard expected concentration", v, "", Posto::Altro));
                }
            } else if let Some(t) = attese_controllo.filter(|t| t.per_chiave.contains_key(&campione)) {
                if let Some(v) = attesa(t) {
                    analita.altro.push(Valore::num("control expected concentration", v, "", Posto::Altro));
                }
            }
            if let Some(u) = sfere.cella("Units:", a).map(str::trim).filter(|u| !u.is_empty() && !e_na(u)) {
                analita.altro.push(Valore::testo("units", u, Posto::Altro));
            }
            m.analiti.push(analita);
        }
        misure.push(m);
    }

    // Il gruppo: la piastra letta.
    let mut g = Gruppo { titolo: nome_piastra.clone().unwrap_or_else(|| "Piastra".into()), quando: quando.clone(), pozzetti: Some(pozzetti as u32), misure, ..Default::default() };
    if let Some(o) = d.primo("Operator") {
        g.dati.push(("analyst".into(), o));
    }
    if let Some(v) = d.primo("ProtocolName") {
        g.dati.push(("analytical method identifier".into(), v));
    }
    if let Some(v) = d.primo("ProtocolVersion") {
        g.dati.push(("method version".into(), v));
    }
    if let Some(v) = &nome_piastra {
        g.dati.push(("experimental data identifier".into(), v.clone()));
    }
    for (k, v) in [("~BatchStopTime", fine.clone()), ("~ProtocolDescription", d.primo("ProtocolDescription"))] {
        if let Some(v) = v {
            g.dati.push((k.into(), v));
        }
    }
    l.gruppi.push(g);

    // Lo strumento e il programma.
    l.strumento.produttore = "Luminex".into();
    l.strumento.modello = modello.unwrap_or_default();
    l.strumento.seriale = d.primo("SN").unwrap_or_default();
    l.strumento.software = software.into();
    l.strumento.versione = d.primo("Build").unwrap_or_default();
    l.strumento.istanza = d.primo("ComputerName").unwrap_or_default();
    // Le informazioni dell'intestazione che nessuno ha usato.
    for k in ["Country Code", "ProtocolDevelopingCompany", "Version"] {
        if let Some(v) = d.primo(k) {
            l.dispositivo.push(Valore::testo(k, &v, Posto::Altro));
        }
    }
    for (k, _) in &d.intest {
        if LETTE.contains(&k.as_str()) || IMPOSTAZIONI_POZZETTO.contains(&k.as_str()) || CAMPIONE_POZZETTO.contains(&k.as_str()) || l.dispositivo.iter().any(|v| v.chiave == *k) {
            continue;
        }
        if let Some(v) = d.primo(k) {
            let valore = match numero(&v).filter(|_| matches!(k.as_str(), "DDGateLow" | "DDGateHigh" | "PlateHeaterTemp")) {
                Some(n) => Valore::num(k, n, "", Posto::Altro),
                None => Valore::testo(k, &v, Posto::Altro),
            };
            l.dispositivo.push(valore);
        }
    }
    // Le informazioni per il foglio «Strumento».
    let mut protocollo = d.primo("ProtocolName").unwrap_or_default();
    if let Some(v) = d.primo("ProtocolVersion") {
        protocollo = if protocollo.is_empty() { format!("v{v}") } else { format!("{protocollo} (v{v})") };
    }
    for (k, v) in [
        ("Protocollo", Some(protocollo).filter(|p| !p.is_empty())),
        ("Note del protocollo", d.primo("ProtocolDescription")),
        ("Pannello", d.primo("PanelName")),
        ("Tipo di sfere", d.primo("BeadType")),
        ("Operatore", d.primo("Operator")),
        ("Piastra", nome_piastra),
        ("Inizio della lettura", quando.as_deref().map(leggibile)),
        ("Fine della lettura", fine.as_deref().and_then(&iso).as_deref().map(leggibile)),
        ("Guadagno del rivelatore", guadagno),
        ("Eventi minimi per sfera", d.minimo.map(|m| format!("{m}"))),
    ] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            l.info.push((k.into(), v));
        }
    }
    // Le tabelle del file che non ho letto: le dico, non le perdo in silenzio.
    let lette: HashSet<&str> = STATISTICHE.iter().map(|s| s.0).chain(ALIAS.iter().map(|a| a.0)).chain(CALCOLATI.iter().map(|c| c.0)).chain(ALTRE_TABELLE.iter().copied()).collect();
    let non_lette: Vec<&str> = d.nomi.iter().map(String::as_str).filter(|n| !lette.contains(n)).collect();
    if !non_lette.is_empty() {
        l.avvisi.push(format!("non ho letto queste tabelle del file: {}", non_lette.join(", ")));
    }
    l.principale = "assay bead count".into();
    l.unita = "#".into();
    Ok(())
}

/// «2024-10-18T11:36:00+00:00» → «2024-10-18 11:36:00».
fn leggibile(iso: &str) -> String {
    iso.replace('T', " ").trim_end_matches("+00:00").to_string()
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn posizione_del_pozzetto() {
        assert_eq!(posizione("1(1,A1)"), Some(("1,A1".to_string(), "A1".to_string())));
        assert_eq!(posizione("12(2,H12)"), Some(("2,H12".to_string(), "H12".to_string())));
        assert_eq!(posizione("A1"), None);
    }

    #[test]
    fn colonne_dell_export_piatto() {
        assert!(e_colonna_analita("R25: RP1 MEDIAN"));
        assert!(!e_colonna_analita("WELL LOCATION"));
        assert_eq!(dividi_colonna("R25: RP1 NET NORMALIZED MEDIAN"), Some(("R25: RP1".to_string(), "NET NORMALIZED MEDIAN".to_string())));
        assert_eq!(dividi_colonna("R25: RP1 RATIO (RP1/RP2)"), Some(("R25: RP1".to_string(), "RATIO (RP1/RP2)".to_string())));
        assert_eq!(nome_tabella("RATIO (RP1/RP2)"), "Ratio");
        assert_eq!(nome_tabella("NET AVERAGE MEDIAN"), "Net Average Median");
        assert_eq!(nome_tabella("AVERAGE MFI"), "Avg Net MFI");
        assert_eq!(nome_tabella("REPLICATE %CV"), "%CV of Replicates");
    }

    #[test]
    fn data_con_il_mese_prima() {
        assert!(!giorno_prima(&["12/8/2025 13:22"]));
        assert!(giorno_prima(&["12/8/2025 13:22", "25/8/2025 9:00"]));
        assert_eq!(computer_dal_nome("____uspltmrlwg00060_results___piastra.xlsx"), "uspltmrlwg00060");
        assert_eq!(computer_dal_nome("piastra.xlsx"), "");
    }
}
