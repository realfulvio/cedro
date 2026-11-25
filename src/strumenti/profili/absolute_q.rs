//! Thermo Fisher QuantStudio Absolute Q: la PCR digitale (ogni pozzetto è diviso in ventimila partizioni,
//! positive o negative). Quattro forme di file:
//!
//! - il CSV dei risultati: una riga per pozzetto, con le righe di gruppo «Average» o «Pooled»;
//! - il «summary» (anche dentro uno ZIP): più coloranti affiancati, che qui diventano una riga ciascuno;
//! - il CSV dei canali («Channels»): i conteggi per ogni combinazione di coloranti;
//! - il CSV con la fluorescenza di ogni partizione («Index»): i conteggi si ricavano dalle colonne `_pos`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Cursor, Read};

use crate::strumenti::fonte::numero;
use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Calcolato, Cubo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

use super::serie::Serie;

pub const PROFILO: Profilo = Profilo {
    id: "appbio_absolute_q",
    nome: "Thermo Fisher QuantStudio Absolute Q",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Dpcr,
    estensioni: &["csv", "zip"],
    riconosci,
    leggi,
};

const MARCHIO: &str = "QuantStudio Absolute Q Digital PCR System";
const SOFTWARE: &str = "QuantStudio Absolute Q Digital PCR Software";
const POZZETTI: u32 = 16;
const CONCENTRAZIONE: &[&str] = &["Conc. cp/uL", "Copies per microliter", "Conc."];
const UNITA_CONC: &str = "cp/µL";

// ---------------------------------------------------------------- il file

/// Lo ZIP dell'export: dentro c'è un solo file «…_summary.csv», gli altri si ignorano.
fn riassunto_nello_zip(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let nomi: Vec<String> = (0..z.len()).filter_map(|i| z.by_index(i).ok().and_then(|f| f.name().ok().map(|n| n.to_string()))).filter(|n| n.ends_with("_summary.csv")).collect();
    match nomi.len() {
        0 => Ok(None),
        1 => {
            let mut dati = Vec::new();
            z.by_name(&nomi[0]).map_err(|e| e.to_string())?.read_to_end(&mut dati).map_err(|e| e.to_string())?;
            Ok(Some(dati))
        }
        _ => Err("nello ZIP ci sono più file «summary»: ne serve uno solo".into()),
    }
}

/// Il testo del CSV da leggere: il file stesso o il «summary» dello ZIP.
fn testo_del_csv(f: &Fonte) -> Result<Cow<'_, str>, String> {
    if f.ext == "zip" {
        let dati = riassunto_nello_zip(&f.bytes)?.ok_or("nello ZIP non c'è nessun file «summary»")?;
        return Ok(Cow::Owned(crate::engine::decode_text(&dati).replace("\r\n", "\n").replace('\r', "\n")));
    }
    Ok(Cow::Borrowed(&f.testo))
}

fn riconosci(f: &Fonte) -> u8 {
    if f.ext == "zip" {
        return match riassunto_nello_zip(&f.bytes) {
            Ok(Some(_)) => 95,
            _ => 0,
        };
    }
    // Le prime due righe bastano: i nomi delle colonne (o i coloranti e poi i nomi).
    let mut nomi: Vec<String> = Vec::new();
    for riga in f.testo.lines().take(2) {
        nomi.extend(riga.split(',').map(|c| c.trim().trim_matches('"').to_string()));
    }
    let ha = |k: &str| nomi.iter().any(|n| n == k);
    let ok = if ha("Instrument") && ha("Plate") {
        ha("Dye") || ha("Target") || ha("Channels") || nomi.iter().any(|n| n.contains("_target"))
    } else {
        ha("Run name") && ha("Target") && ha("Total")
    };
    if ok {
        95
    } else {
        0
    }
}

// ---------------------------------------------------------------- la tabella

/// Il file come tabella lunga: una riga per pozzetto e colorante.
struct Tabella {
    nomi: Vec<String>,
    righe: Vec<Vec<Option<String>>>,
    /// Le colonne che il summary ha una volta sola per pozzetto (non per colorante).
    comuni: Vec<String>,
}

fn vuota(r: &[String]) -> bool {
    r.iter().all(|c| c.trim().is_empty())
}

fn cella(r: &[String], i: usize) -> Option<String> {
    r.get(i).cloned()
}

fn carica(testo: &str) -> Result<Tabella, String> {
    let righe = crate::textconv::parse_csv(testo, ',');
    let titoli = righe.first().ok_or("file vuoto")?;
    let info = righe.get(1).ok_or("file vuoto")?;
    let primo = titoli.iter().position(|t| !t.trim().is_empty()).ok_or("file vuoto")?;
    let corpo: Vec<&Vec<String>> = righe[1..].iter().take_while(|r| !vuota(r)).collect();

    let mut t = if info.get(primo).map(|s| s.trim()) != Some("Target") {
        // Il CSV dei risultati: la prima riga ha i nomi.
        let nomi: Vec<String> = titoli.iter().map(|s| s.trim().to_string()).collect();
        let righe = corpo.iter().map(|r| (0..nomi.len()).map(|i| cella(r, i)).collect()).collect();
        Tabella { nomi, righe, comuni: Vec::new() }
    } else {
        sommario(titoli, info, &corpo)?
    };

    // I nomi che cambiano da un programma all'altro.
    let ha = |t: &Tabella, k: &str| t.nomi.iter().any(|n| n == k);
    for (da, a) in [("Name", "Sample"), ("Well ID", "Well"), ("Run name", "Run")] {
        if ha(&t, da) && !ha(&t, a) {
            for n in t.nomi.iter_mut().chain(t.comuni.iter_mut()) {
                if n == da {
                    *n = a.to_string();
                }
            }
        }
    }
    if !ha(&t, "Sample") {
        return Err("nel file manca la colonna «Sample» (o «Name»)".into());
    }
    Ok(t)
}

/// Il summary: sopra i nomi dei coloranti, sotto i nomi delle colonne; ogni colorante ha il suo blocco da
/// «Target» a «Threshold». Ogni blocco diventa le righe di un colorante, con le colonne comuni accanto.
fn sommario(titoli: &[String], info: &[String], corpo: &[&Vec<String>]) -> Result<Tabella, String> {
    let coloranti: Vec<&str> = titoli.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
    let inizi: Vec<usize> = info.iter().enumerate().filter(|(_, c)| c.trim() == "Target").map(|(i, _)| i).collect();
    let fini: Vec<usize> = info.iter().enumerate().filter(|(_, c)| c.trim() == "Threshold").map(|(i, _)| i).collect();
    if inizi.is_empty() || inizi.len() != fini.len() || inizi.len() != coloranti.len() {
        return Err("non riesco a capire i blocchi dei coloranti (da «Target» a «Threshold»)".into());
    }
    let larghezza = fini[0] + 1 - inizi[0];
    for (s, e) in inizi.iter().zip(&fini) {
        if e + 1 < *s || e + 1 - s != larghezza {
            return Err(format!("i blocchi dei coloranti devono avere tutti {larghezza} colonne"));
        }
    }
    let (inizio, fine) = (inizi[0], *fini.last().unwrap_or(&0));
    let comuni_idx: Vec<usize> = (0..info.len()).filter(|i| *i < inizio || *i > fine).collect();
    let comuni: Vec<String> = comuni_idx.iter().map(|&i| info[i].trim().to_string()).collect();
    let blocco: Vec<String> = info[inizio..=inizio + larghezza - 1].iter().map(|s| s.trim().to_string()).collect();
    let mut nomi = comuni.clone();
    nomi.extend(blocco.iter().cloned());
    nomi.push("Dye".into());

    // Le righe di dati (la prima di `corpo` è quella dei nomi). Il gruppo, lasciato vuoto, è quello sopra.
    let gruppo = comuni.iter().position(|n| n == "Group");
    let mut base: Vec<Vec<Option<String>>> = Vec::new();
    let mut ultimo: Option<String> = None;
    for r in corpo.iter().skip(1) {
        let mut b: Vec<Option<String>> = comuni_idx.iter().map(|&i| cella(r, i)).collect();
        if let Some(g) = gruppo {
            match b[g].as_deref().filter(|s| !s.trim().is_empty()) {
                Some(v) => ultimo = Some(v.to_string()),
                None => b[g] = ultimo.clone(),
            }
        }
        base.push(b);
    }
    let mut righe = Vec::new();
    for (k, colorante) in coloranti.iter().enumerate() {
        for (b, r) in base.iter().zip(corpo.iter().skip(1)) {
            let celle: Vec<Option<String>> = (0..larghezza).map(|j| cella(r, inizi[k] + j)).collect();
            // Un colorante senza nessun dato per questa riga non è stato letto.
            if celle.iter().all(|c| c.as_deref().is_none_or(|s| s.trim().is_empty())) {
                continue;
            }
            let mut riga = b.clone();
            riga.extend(celle);
            riga.push(Some(colorante.to_string()));
            righe.push(riga);
        }
    }
    Ok(Tabella { nomi, righe, comuni })
}

// ---------------------------------------------------------------- i calcoli

#[derive(Clone, Copy, PartialEq)]
enum Da {
    Misure,
    Calcoli,
}

struct Rif {
    colonne: &'static [&'static str],
    nome: &'static str,
    da: Da,
    /// Le grandezze da cui viene, separate da una virgola.
    grandezze: &'static str,
    unita: &'static str,
}

const fn rif(colonne: &'static [&'static str], nome: &'static str, da: Da, grandezze: &'static str, unita: &'static str) -> Rif {
    Rif { colonne, nome, da, grandezze, unita }
}

const MEDIA: &[Rif] = &[
    rif(&["Total"], "Average Total", Da::Misure, "total partition count", "#"),
    rif(CONCENTRAZIONE, "Average Concentration", Da::Misure, "number concentration", UNITA_CONC),
    rif(&["SD"], "SD Concentration", Da::Misure, "number concentration", UNITA_CONC),
    rif(&["Positives"], "Average Positives", Da::Misure, "positive partition count", "#"),
    rif(&["CV%"], "CV", Da::Calcoli, "Average Concentration,SD Concentration", "%"),
    rif(&["95%DeltaLCI"], "Average 95%DeltaLCI", Da::Misure, "95%DeltaLCI", ""),
    rif(&["95%DeltaUCI"], "Average 95%DeltaUCI", Da::Misure, "95%DeltaUCI", ""),
];

const POOLED: &[Rif] = &[
    rif(&["Total"], "Pooled Total", Da::Misure, "total partition count", "#"),
    rif(&["Conc. cp/uL"], "Pooled Concentration", Da::Misure, "number concentration", UNITA_CONC),
    rif(&["Positives"], "Pooled Positives", Da::Misure, "positive partition count", "#"),
];

const CONTEGGI: &str = "total partition count,positive partition count";

/// I calcoli di ogni pozzetto e colorante. Allotropy elenca due volte Lambda_95%DeltaLCI e Lambda_95%DeltaUCI:
/// qui una sola.
const INDIVIDUALI: &[Rif] = &[
    rif(&["95%DeltaLCI"], "95%DeltaLCI", Da::Misure, CONTEGGI, ""),
    rif(&["95%DeltaUCI"], "95%DeltaUCI", Da::Misure, CONTEGGI, ""),
    rif(&["Precision %"], "Precision %", Da::Misure, CONTEGGI, ""),
    rif(&["Lambda (cp/Rxn)"], "Lambda (cp/Rxn)", Da::Misure, CONTEGGI, ""),
    rif(&["Lambda_95%DeltaLCI"], "Lambda_95%DeltaLCI", Da::Misure, CONTEGGI, ""),
    rif(&["Lambda_95%DeltaUCI"], "Lambda_95%DeltaUCI", Da::Misure, CONTEGGI, ""),
    rif(&["MeanQC"], "MeanQC", Da::Misure, "QC Channel", ""),
    rif(&["CV_QC"], "CV_QC", Da::Misure, "QC Channel", ""),
];

fn calcoli_di<'a>(tabella: &'a [Rif], d: &Serie) -> Vec<(&'a Rif, f64)> {
    tabella.iter().filter_map(|r| d.num(r.colonne).map(|v| (r, v))).collect()
}

/// Il nome della colonna come lo chiave Allotropy («Conc. cp/uL,Copies per microliter,Conc.» per la concentrazione).
fn chiave_colonna(r: &Rif) -> String {
    r.colonne.join(",")
}

// ---------------------------------------------------------------- le misure

/// Una misura letta, con quello che serve per i gruppi e i calcoli.
struct Letta {
    m: Misura,
    run: String,
    gruppo: String,
    bersaglio: String,
    colorante: String,
    strumento: String,
}

fn numero_cella(d: &Serie, ks: &[&str]) -> Option<f64> {
    d.num(ks)
}

fn pieno(d: &Serie, k: &str) -> Option<String> {
    d.testo_pieno(&[k]).map(|s| s.trim().to_string())
}

fn manca(k: &str) -> String {
    format!("nel file manca il valore della colonna «{k}»")
}

/// Il pozzetto della riga: la colonna «Well», o il «Sample A1» del nome del campione.
fn pozzetto(d: &Serie, campione: &str) -> Result<String, String> {
    if let Some(w) = pieno(d, "Well") {
        return Ok(w);
    }
    if let Some(r) = campione.strip_prefix("Sample ") {
        let w: String = r.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if !w.is_empty() {
            return Ok(w);
        }
    }
    Err(format!("non riesco a ricavare il pozzetto dal campione «{campione}»"))
}

/// Il 5/19/2023 di Absolute Q è maggio, il 19 maggio: finché una data non dice altro il mese viene prima.
fn giorno_prima(date: &[&str]) -> bool {
    for d in date {
        let primo = d.trim().split_whitespace().next().unwrap_or("");
        let n: Vec<u32> = primo.split('/').filter_map(|x| x.parse().ok()).collect();
        if n.len() == 3 && n[0] > 12 {
            return true;
        }
    }
    false
}

fn quando(testo: Option<String>, gp: bool) -> Result<Option<String>, String> {
    match testo.filter(|t| !t.trim().is_empty()) {
        None => Ok(None),
        Some(t) => tempo::iso(&t, gp).map(Some).ok_or_else(|| format!("non capisco la data «{t}»")),
    }
}

fn arrotonda(x: f64) -> f64 {
    x.round()
}

/// Le righe di un pozzetto (una per colorante): una misura ciascuna, con i calcoli del pozzetto.
fn letta_da_riga(l: &mut Lettura, d: &Serie, gp: bool) -> Result<Letta, String> {
    let campione = d.grezzo("Sample").map(|s| s.trim().to_string()).unwrap_or_default();
    let pozzo = pozzetto(d, &campione)?;
    let piastra = pieno(d, "Plate");
    let gruppo = d.grezzo("Group").map(|s| s.trim().to_string()).unwrap_or_default();
    let bersaglio = pieno(d, "Target").unwrap_or_else(|| "N/A".into());
    let run = pieno(d, "Run").ok_or_else(|| manca("Run"))?;
    let strumento = pieno(d, "Instrument").unwrap_or_else(|| "N/A".into());
    let ora = quando(d.testo(&["Date"]), gp)?;
    let totale = arrotonda(numero_cella(d, &["Total"]).ok_or_else(|| manca("Total"))?);
    let colorante = ["Dye", "Channels"].iter().find_map(|k| pieno(d, k)).ok_or_else(|| manca("Dye"))?;
    let conc = numero_cella(d, CONCENTRAZIONE).ok_or_else(|| manca("Conc."))?;
    let positive = arrotonda(numero_cella(d, &["Positives", "Count"]).ok_or_else(|| manca("Positives"))?);
    let ci = d.num(&["95%CI"]);
    let soglia = d.num(&["Threshold"]);

    let id = l.id();
    let mut m = Misura { id: id.clone(), pozzetto: Some(pozzo), piastra, campione, bersaglio: Some(bersaglio.clone()), quando: ora, ..Default::default() };
    m.valori.push(Valore::testo("reporter dye setting", &colorante, Posto::Impostazione));
    m.valori.push(Valore::num("total partition count", totale, "#", Posto::Misura));
    m.valori.push(Valore::num("number concentration", conc, UNITA_CONC, Posto::Elaborato));
    m.valori.push(Valore::num("positive partition count", positive, "#", Posto::Elaborato));
    if let Some(x) = ci {
        m.valori.push(Valore::num("confidence interval (95%)", x, "#", Posto::Elaborato));
    }
    if let Some(x) = soglia {
        m.valori.push(Valore::num("fluorescence intensity threshold setting", x, "", Posto::Elaborazione));
    }
    for (r, v) in calcoli_di(INDIVIDUALI, d) {
        l.calcolati.push(Calcolato { nome: r.nome.into(), valore: v, unita: r.unita.into(), sorgenti: vec![id.clone()], da: r.grandezze.into(), descrizione: String::new(), da_calcoli: Vec::new() });
    }
    // Il resto delle colonne (qualità, rapporto di diluizione, numero di copie…) è informazione libera.
    for (k, v) in d.non_letti(&|_| false) {
        m.valori.push(Valore { chiave: k, dato: v, unita: String::new(), posto: Posto::Altro });
    }
    Ok(Letta { m, run, gruppo, bersaglio, colorante, strumento })
}

// ---------------------------------------------------------------- la fluorescenza di ogni partizione

/// Le righe del file con la fluorescenza di ogni partizione, divise per pozzetto. Il file ha decine di migliaia
/// di righe per pozzetto: si legge una riga alla volta, senza tenerle tutte come testo.
struct PartizioneRiga {
    indice: f64,
    fluorescenza: Vec<f64>,
    positiva: Vec<Option<bool>>,
}

struct PozzoCubo {
    /// La riga con l'indice più basso: da lì si leggono le informazioni del pozzetto.
    prima: Vec<Option<String>>,
    prima_indice: f64,
    righe: Vec<PartizioneRiga>,
}

fn campi(riga: &str) -> Vec<String> {
    if riga.contains('"') {
        crate::textconv::parse_csv(riga, ',').into_iter().next().unwrap_or_default()
    } else {
        riga.split(',').map(str::to_string).collect()
    }
}

fn leggi_partizioni(testo: &str, l: &mut Lettura, avvisi: &mut Vec<String>) -> Result<Vec<Letta>, String> {
    let mut righe = testo.lines();
    let nomi: Vec<String> = campi(righe.next().ok_or("file vuoto")?).iter().map(|s| s.trim().to_string()).collect();
    let nomi: Vec<String> = nomi.into_iter().map(|n| if n == "Well ID" { "Well".to_string() } else if n == "Name" { "Sample".to_string() } else if n == "Run name" { "Run".to_string() } else { n }).collect();
    let pos = |k: &str| nomi.iter().position(|n| n == k);
    let i_indice = pos("Index").ok_or("manca la colonna «Index»")?;
    let i_pozzo = pos("Well").ok_or("manca la colonna «Well ID»")?;
    // I coloranti: nomi di tre lettere maiuscole. Quelli con la colonna `_target` sono i bersagli, l'altro (ROX) è il passivo.
    let coloranti: Vec<(usize, String)> = nomi.iter().enumerate().filter(|(_, n)| n.chars().count() == 3 && **n == n.to_uppercase()).map(|(i, n)| (i, n.clone())).collect();
    let bersagli: Vec<&(usize, String)> = coloranti.iter().filter(|(_, n)| pos(&format!("{n}_target")).is_some()).collect();
    let passivi: Vec<&(usize, String)> = coloranti.iter().filter(|(_, n)| pos(&format!("{n}_target")).is_none()).collect();
    if passivi.len() != 1 {
        return Err(format!("ci deve essere un solo colorante di riferimento (senza «_target»), nel file ce ne sono {}", passivi.len()));
    }
    let passivo = passivi[0];
    for (_, n) in &bersagli {
        if pos(&format!("{n}_pos")).is_none() {
            return Err(format!("manca la colonna «{n}_pos»"));
        }
    }
    let parsa = |s: Option<&String>| s.and_then(|s| numero(s)).unwrap_or(f64::NAN);
    let e_vero = |s: Option<&String>| match s.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };
    let i_pos: Vec<Option<usize>> = bersagli.iter().map(|(_, n)| pos(&format!("{n}_pos"))).collect();

    let mut ordine: Vec<String> = Vec::new();
    let mut pozzi: HashMap<String, PozzoCubo> = HashMap::new();
    for riga in righe {
        if riga.trim().is_empty() || riga.trim_matches(',').trim().is_empty() {
            continue;
        }
        let c = campi(riga);
        let Some(pozzo) = c.get(i_pozzo).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else { continue };
        let indice = c.get(i_indice).and_then(|s| numero(s)).ok_or("la colonna «Index» ha un valore che non è un numero")?;
        let riga_cubo = PartizioneRiga {
            indice,
            fluorescenza: bersagli.iter().map(|(i, _)| parsa(c.get(*i))).chain(std::iter::once(parsa(c.get(passivo.0)))).collect(),
            positiva: i_pos.iter().map(|i| e_vero(i.and_then(|i| c.get(i)))).collect(),
        };
        match pozzi.get_mut(&pozzo) {
            Some(p) => {
                if indice < p.prima_indice {
                    p.prima = c.iter().cloned().map(Some).collect();
                    p.prima_indice = indice;
                }
                p.righe.push(riga_cubo);
            }
            None => {
                ordine.push(pozzo.clone());
                pozzi.insert(pozzo, PozzoCubo { prima: c.iter().cloned().map(Some).collect(), prima_indice: indice, righe: vec![riga_cubo] });
            }
        }
    }
    if ordine.is_empty() {
        return Err("nel file non ci sono righe di partizioni".into());
    }

    // Le date: l'ordine giorno/mese.
    let date: Vec<&str> = pozzi.values().filter_map(|p| pos("Date").and_then(|i| p.prima.get(i)).and_then(|c| c.as_deref())).collect();
    let gp = giorno_prima(&date);

    let mut fuori = Vec::new();
    let mut lette: Vec<Letta> = Vec::new();
    for nome in &ordine {
        let p = pozzi.get_mut(nome).ok_or("pozzetto perso")?;
        p.righe.sort_by(|a, b| a.indice.total_cmp(&b.indice));
        let totale = p.righe.iter().map(|r| r.indice).fold(f64::MIN, f64::max);
        let d = Serie::nuova(&nomi, &p.prima);
        let campione = d.grezzo("Sample").map(|s| s.trim().to_string()).unwrap_or_default();
        if campione.is_empty() {
            return Err(manca("Sample"));
        }
        let piastra = pieno(&d, "Plate");
        let run = pieno(&d, "Run").ok_or_else(|| manca("Run"))?;
        let strumento = pieno(&d, "Instrument").unwrap_or_else(|| "N/A".into());
        let ora = quando(d.testo(&["Date"]), gp)?;
        let gruppo = d.grezzo("Group").map(|s| s.trim().to_string()).unwrap_or_default();
        let ci = d.num(&["95%CI"]);
        let conc = d.num(CONCENTRAZIONE);
        d.segna("Well");
        d.segna("Index");
        let x: Vec<f64> = p.righe.iter().map(|r| r.indice).collect();
        let passiva: Vec<f64> = p.righe.iter().map(|r| *r.fluorescenza.last().unwrap_or(&f64::NAN)).collect();
        for (k, (_, colorante)) in bersagli.iter().enumerate() {
            let bersaglio = pieno(&d, &format!("{colorante}_target")).ok_or_else(|| manca(&format!("{colorante}_target")))?;
            let soglia = d.num(&[&format!("{colorante}_threshold")]).ok_or_else(|| manca(&format!("{colorante}_threshold")))?;
            let positive = p.righe.iter().filter(|r| r.positiva[k] == Some(true)).count() as f64;
            let negative = p.righe.iter().filter(|r| r.positiva[k] == Some(false)).count() as f64;
            if positive + negative != totale {
                fuori.push(format!("{nome} · {colorante}: positive {positive} e negative {negative} su {totale} partizioni: la colonna «{colorante}_pos» ha valori diversi da True e False"));
            }
            let id = l.id();
            let mut m = Misura { id, pozzetto: Some(nome.clone()), piastra: piastra.clone(), campione: campione.clone(), bersaglio: Some(bersaglio.clone()), quando: ora.clone(), ..Default::default() };
            m.valori.push(Valore::testo("reporter dye setting", colorante, Posto::Impostazione));
            m.valori.push(Valore::testo("passive reference dye setting", &passivo.1, Posto::Impostazione));
            m.valori.push(Valore::num("total partition count", totale, "#", Posto::Misura));
            m.valori.push(match conc {
                Some(c) => Valore::num("number concentration", c, UNITA_CONC, Posto::Elaborato),
                None => Valore { chiave: "number concentration".into(), dato: Dato::Assente("N/A".into()), unita: UNITA_CONC.into(), posto: Posto::Elaborato },
            });
            m.valori.push(Valore::num("positive partition count", positive, "#", Posto::Elaborato));
            m.valori.push(Valore::num("negative partition count", negative, "#", Posto::Elaborato));
            if let Some(x) = ci {
                m.valori.push(Valore::num("confidence interval (95%)", x, "#", Posto::Elaborato));
            }
            m.valori.push(Valore::num("fluorescence intensity threshold setting", soglia, "", Posto::Elaborazione));
            m.cubi.push(Cubo {
                chiave: "reporter dye data cube".into(),
                etichetta: "reporter dye".into(),
                dimensione: "cycle count".into(),
                unita_dimensione: "#".into(),
                intero: true,
                concetto: "reporter dye fluorescence".into(),
                unita: "RFU".into(),
                x: x.clone(),
                y: p.righe.iter().map(|r| r.fluorescenza[k]).collect(),
            });
            m.cubi.push(Cubo {
                chiave: "passive reference dye data cube".into(),
                etichetta: "passive reference dye".into(),
                dimensione: "cycle count".into(),
                unita_dimensione: "#".into(),
                intero: true,
                concetto: "passive reference dye fluorescence".into(),
                unita: "RFU".into(),
                x: x.clone(),
                y: passiva.clone(),
            });
            // Le informazioni che restano (la partizione scartata…) sono libere; le colonne delle partizioni no.
            let salta = |n: &str| {
                n == "Index" || coloranti.iter().any(|(_, c)| n == c || n == format!("{c}_pos") || n == format!("{c}_target") || n == format!("{c}_threshold"))
            };
            for (k2, v) in d.non_letti(&salta) {
                m.valori.push(Valore { chiave: k2, dato: v, unita: String::new(), posto: Posto::Altro });
            }
            lette.push(Letta { m, run: run.clone(), gruppo: gruppo.clone(), bersaglio, colorante: colorante.clone(), strumento: strumento.clone() });
        }
    }
    avvisi.extend(fuori);
    Ok(lette)
}

// ---------------------------------------------------------------- la lettura

/// Un gruppo di calcoli: la riga «Average» o «Pooled» di un gruppo e di un bersaglio.
struct CalcoliGruppo {
    gruppo: String,
    bersaglio: String,
    voci: Vec<(&'static Rif, f64)>,
}

/// I calcoli dello strumento sui gruppi di pozzetti (medie, deviazioni, CV) con i pozzetti da cui vengono.
fn calcoli_dei_gruppi(t: &Tabella, righe_gruppo: &[&Vec<Option<String>>], lette: &[Letta], l: &mut Lettura) -> Result<(), String> {
    let mut ids: HashMap<String, Vec<String>> = HashMap::new();
    for x in lette {
        ids.entry(format!("{}_{}", x.gruppo, x.bersaglio)).or_default().push(x.m.id.clone());
        ids.entry(x.gruppo.clone()).or_default().push(x.m.id.clone());
    }
    let mut gruppi: Vec<CalcoliGruppo> = Vec::new();
    // Le colonne che il summary scrive una volta sola per pozzetto: la media del totale è la stessa per ogni bersaglio.
    let mut comuni: Vec<(String, Vec<(&'static Rif, f64)>)> = Vec::new();
    for r in righe_gruppo {
        let d = Serie::nuova(&t.nomi, r);
        let tabella: &'static [Rif] = match pieno(&d, "Well").as_deref() {
            Some("Average") => MEDIA,
            Some("Pooled") => POOLED,
            _ => &[],
        };
        let (Some(gruppo), Some(bersaglio)) = (pieno(&d, "Group"), pieno(&d, "Target")) else { continue };
        let mut propri = Vec::new();
        for (rif, v) in calcoli_di(tabella, &d) {
            if !t.comuni.contains(&chiave_colonna(rif)) {
                propri.push((rif, v));
                continue;
            }
            let voci = match comuni.iter_mut().find(|(g, _)| *g == gruppo) {
                Some(e) => &mut e.1,
                None => {
                    comuni.push((gruppo.clone(), Vec::new()));
                    &mut comuni.last_mut().unwrap().1
                }
            };
            match voci.iter().find(|(r, _)| r.nome == rif.nome) {
                Some((_, v0)) if *v0 != v => return Err(format!("il gruppo «{gruppo}» ha due valori diversi per «{}»: {v0} e {v}", chiave_colonna(rif))),
                Some(_) => {}
                None => voci.push((rif, v)),
            }
        }
        gruppi.push(CalcoliGruppo { gruppo, bersaglio, voci: propri });
    }

    let scrivi = |l: &mut Lettura, voci: &[(&'static Rif, f64)], sorgenti: &[String]| {
        let mut indici: HashMap<&str, usize> = HashMap::new();
        for (rif, v) in voci {
            let (sorg, da_calcoli) = match rif.da {
                Da::Misure => {
                    if sorgenti.is_empty() {
                        continue;
                    }
                    (sorgenti.to_vec(), Vec::new())
                }
                Da::Calcoli => {
                    let d: Vec<usize> = rif.grandezze.split(',').filter_map(|n| indici.get(n).copied()).collect();
                    if d.is_empty() {
                        continue;
                    }
                    (Vec::new(), d)
                }
            };
            indici.insert(rif.nome, l.calcolati.len());
            let da = if rif.da == Da::Misure { rif.grandezze } else { "" };
            l.calcolati.push(Calcolato { nome: rif.nome.into(), valore: *v, unita: rif.unita.into(), sorgenti: sorg, da: da.into(), descrizione: String::new(), da_calcoli });
        }
    };
    for g in &gruppi {
        let nome = g.gruppo.split('(').next().unwrap_or("").trim();
        let chiave = if g.bersaglio.is_empty() { g.gruppo.clone() } else { format!("{nome}_{}", g.bersaglio) };
        let sorgenti = ids.get(&chiave).cloned().unwrap_or_default();
        scrivi(l, &g.voci, &sorgenti);
    }
    for (gruppo, voci) in &comuni {
        let sorgenti = ids.get(gruppo).cloned().unwrap_or_default();
        scrivi(l, voci, &sorgenti);
    }
    Ok(())
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let testo = testo_del_csv(f)?;
    let prima = testo.lines().next().unwrap_or("");
    let con_partizioni = campi(prima).iter().any(|c| c.trim() == "Index");
    let mut avvisi: Vec<String> = Vec::new();

    let lette: Vec<Letta> = if con_partizioni {
        leggi_partizioni(&testo, l, &mut avvisi)?
    } else {
        let t = carica(&testo)?;
        let ha_canali = t.nomi.iter().any(|n| n == "Channels");
        let ha_pozzo = t.nomi.iter().any(|n| n == "Well");
        let date: Vec<&str> = t.nomi.iter().position(|n| n == "Date").map(|i| t.righe.iter().filter_map(|r| r.get(i).and_then(|c| c.as_deref())).collect()).unwrap_or_default();
        let gp = giorno_prima(&date);
        let mut lette: Vec<Letta> = Vec::new();
        let mut righe_gruppo: Vec<&Vec<Option<String>>> = Vec::new();
        for r in &t.righe {
            let d = Serie::nuova(&t.nomi, r);
            if pieno(&d, "Sample").is_none() {
                righe_gruppo.push(r);
                continue;
            }
            // Come fa Allotropy: nei file per canali contano le righe con il canale, negli altri quelle col pozzetto.
            if (ha_canali && pieno(&d, "Channels").is_none()) || (!ha_canali && ha_pozzo && pieno(&d, "Well").is_none()) {
                continue;
            }
            lette.push(letta_da_riga(l, &d, gp)?);
        }
        if t.nomi.iter().any(|n| n == "Group") {
            calcoli_dei_gruppi(&t, &righe_gruppo, &lette, l)?;
        }
        lette
    };
    if lette.is_empty() {
        return Err("nel file non ci sono risultati per pozzetto".into());
    }

    // Una piastra per colorante: i pozzetti con il loro bersaglio.
    let corse: Vec<String> = lette.iter().map(|x| x.run.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let strumento = lette.first().map(|x| x.strumento.clone()).unwrap_or_default();
    let mut gruppi: Vec<((String, String, String), Gruppo)> = Vec::new();
    for x in lette {
        let piastra = x.m.piastra.clone().unwrap_or_default();
        let chiave = (x.run.clone(), piastra.clone(), x.colorante.clone());
        let i = match gruppi.iter().position(|(k, _)| *k == chiave) {
            Some(i) => i,
            None => {
                let mut titolo = vec![];
                if corse.len() > 1 {
                    titolo.push(x.run.clone());
                }
                if !piastra.is_empty() {
                    titolo.push(piastra.clone());
                }
                titolo.push(x.colorante.clone());
                let g = Gruppo {
                    titolo: titolo.join(" · "),
                    piastra: x.m.piastra.clone(),
                    pozzetti: Some(POZZETTI),
                    dati: vec![("experimental data identifier".into(), x.run.clone())],
                    ..Default::default()
                };
                gruppi.push((chiave, g));
                gruppi.len() - 1
            }
        };
        gruppi[i].1.misure.push(x.m);
    }
    l.gruppi = gruppi.into_iter().map(|(_, g)| g).collect();

    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.marchio = MARCHIO.into();
    l.strumento.software = SOFTWARE.into();
    if !strumento.is_empty() && strumento != "N/A" {
        l.strumento.identificativo = strumento;
    }
    if corse.len() == 1 {
        l.info.push(("Esperimento".into(), corse[0].clone()));
    }
    l.principale = "number concentration".into();
    l.unita = UNITA_CONC.into();
    l.avvisi.extend(avvisi);
    Ok(())
}
