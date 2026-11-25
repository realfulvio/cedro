//! Qiagen QIAcuity: la PCR digitale delle nanopiastre. L'export è un CSV che comincia con la riga
//! «sep=,» e ha una riga per pozzetto: concentrazione, partizioni valide, positive e negative, soglia.
//! I nomi delle colonne cambiano con la versione del programma («Concentration (copies/µL)»,
//! «Conc. [copies/µL]», «Conc. [cp/µL] (dPCR reaction)») e il micro è scritto in tanti modi.

use crate::strumenti::fonte::Fonte;
use crate::strumenti::{Calcolato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

use super::serie::Serie;

pub const PROFILO: Profilo = Profilo {
    id: "qiacuity_dpcr",
    nome: "Qiagen QIAcuity dPCR",
    produttore: "Qiagen",
    tecnica: Tecnica::Dpcr,
    estensioni: &["csv"],
    riconosci,
    leggi,
};

const MARCHIO: &str = "Qiacuity Digital PCR System";
const SOFTWARE: &str = "Qiacuity Software Suite";
const STRUMENTO: &str = "Qiacuity dPCR";
const UNITA_CONC: &str = "cp/µL";

const CONCENTRAZIONE: &[&str] = &["Concentration (copies/μL)", "Conc. [copies/μL]", "Conc. [cp/μL] (dPCR reaction)"];
const TOTALE: &[&str] = &["Partitions (valid)", "Partitions (Valid)"];
const POSITIVE: &[&str] = &["Partitions (positive)", "Partitions (Positive)"];
const NEGATIVE: &[&str] = &["Partitions (negative)", "Partitions (Negative)"];

/// I calcoli che lo strumento scrive per pozzetto: colonna, nome, unità e la grandezza da cui vengono.
const CALCOLI: &[(&str, &str, &str, &str)] = &[
    ("CI (95%)", "CI (95%)", "%", "Positive Partition Count"),
    ("SD", "SD", "", "Number Concentration"),
    ("CV%", "CV%", "%", "Number Concentration (#/μL)"),
    ("Mean conc. [copies/μL]", "Mean Concentration", "cp/µL", "Mean Concentration (#/μL)"),
];

/// Le informazioni del campione che l'export scrive nelle colonne (spesso «-»).
const DEL_CAMPIONE: &[&str] = &["IC", "Control type", "Target (IC)", "Target (Control)"];

fn riconosci(f: &Fonte) -> u8 {
    let mut righe = f.testo.lines();
    let prima = righe.next().unwrap_or("").trim().trim_start_matches('\u{feff}');
    if !prima.starts_with("sep=") {
        return 0;
    }
    let testata = righe.next().unwrap_or("");
    if testata.contains("Partitions") || testata.contains("Sample/NTC") {
        95
    } else {
        0
    }
}

/// Il micro in tutte le sue forme (µ, μ, il simbolo di sostituzione di un testo letto male, «Âµ») è il μ greco.
fn pulisci(n: &str) -> String {
    n.replace("Âμ", "μ").replace("Âµ", "μ").replace(['\u{fffd}', 'µ'], "μ").trim().to_string()
}

fn pieno(d: &Serie, k: &str) -> Option<String> {
    d.testo_pieno(&[k]).map(|s| s.trim().to_string())
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    // Il separatore lo dice la prima riga.
    let testo = f.testo.trim_start_matches('\u{feff}');
    let prima = testo.lines().next().unwrap_or("");
    let sep = prima.strip_prefix("sep=").and_then(|s| s.chars().next()).unwrap_or(',');
    let righe = crate::textconv::parse_csv(testo, sep);
    // La prima riga è «sep=,»: i nomi delle colonne sono nella seconda.
    let titoli = righe.get(1).ok_or("nel file non ci sono righe di risultati")?;
    let mut nomi: Vec<String> = titoli.iter().map(|n| pulisci(n)).collect();
    // La colonna dei pozzetti: «Well», o la prima senza nome.
    match nomi.iter().position(|n| n == "Well") {
        Some(i) => nomi[i] = "Well Name".into(),
        None => nomi[0] = "Well Name".into(),
    }
    // Come fa pandas, le colonne con lo stesso nome diventano «CI (95%)», «CI (95%).1»…
    let mut visti: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for n in nomi.iter_mut() {
        let c = visti.entry(n.clone()).or_insert(0);
        if *c > 0 {
            *n = format!("{n}.{c}");
        }
        *c += 1;
    }
    let ha = |k: &str| nomi.iter().any(|n| n == k);
    if !ha("Sample/NTC/Control") {
        return Err("manca la colonna «Sample/NTC/Control»".into());
    }

    let mut misure: Vec<Misura> = Vec::new();
    let mut tipo_piastra: Option<String> = None;
    for r in righe.iter().skip(2) {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let celle: Vec<Option<String>> = (0..nomi.len()).map(|i| r.get(i).cloned()).collect();
        let d = Serie::nuova(&nomi, &celle);
        let pozzetto = pieno(&d, "Well Name").ok_or("nel file manca il valore della colonna «Well»")?;
        let campione = pieno(&d, "Sample/NTC/Control").ok_or("nel file manca il valore della colonna «Sample/NTC/Control»")?;
        let bersaglio = d.testo_pieno(&["Target", "Target (Name)"]).map(|s| s.trim().to_string()).ok_or("nel file manca il valore della colonna «Target»")?;
        let totale = d.num(TOTALE).ok_or("nel file manca il valore della colonna «Partitions (valid)»")?.round();
        let conc = d.num(CONCENTRAZIONE).ok_or("nel file manca il valore della colonna «Concentration (copies/μL)»")?;
        let positive = d.num(POSITIVE).ok_or("nel file manca il valore della colonna «Partitions (positive)»")?.round();
        let negative = d.num(NEGATIVE).map(f64::round);
        let soglia = d.num(&["Threshold"]);
        let tipo = pieno(&d, "Type");
        let piastra = pieno(&d, "Plate ID");
        // Il tipo di piastra lo dice la prima riga che lo scrive: lo si legge senza segnarlo, resta anche tra le informazioni libere.
        if tipo_piastra.is_none() {
            tipo_piastra = nomi.iter().position(|n| n == "Plate type").and_then(|i| r.get(i)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }

        let mut m = Misura { id: l.id(), pozzetto: Some(pozzetto), piastra, campione, bersaglio: Some(bersaglio), ..Default::default() };
        match tipo.as_deref() {
            Some("Sample") => {
                m.ruolo = Ruolo::Campione;
                m.ruolo_dal_file = true;
            }
            Some("Control") => {
                m.ruolo = Ruolo::Controllo;
                m.ruolo_dal_file = true;
            }
            Some("Non Template Control") => {
                m.ruolo = Ruolo::Bianco;
                m.ruolo_dal_file = true;
            }
            Some(altro) => m.valori.push(Valore::testo("sample type", altro, Posto::CampioneAltro)),
            None => {}
        }
        for k in DEL_CAMPIONE {
            if let Some(t) = pieno(&d, k).filter(|t| !matches!(t.as_str(), "-" | "--" | "–")) {
                m.valori.push(Valore::testo(k, &t, Posto::CampioneAltro));
            }
        }
        m.valori.push(Valore::num("total partition count", totale, "#", Posto::Misura));
        m.valori.push(Valore::num("number concentration", conc, UNITA_CONC, Posto::Elaborato));
        m.valori.push(Valore::num("positive partition count", positive, "#", Posto::Elaborato));
        if let Some(n) = negative {
            m.valori.push(Valore::num("negative partition count", n, "#", Posto::Elaborato));
        }
        if let Some(s) = soglia {
            m.valori.push(Valore::num("fluorescence intensity threshold setting", s, "", Posto::Elaborazione));
        }
        for (colonna, nome, unita, da) in CALCOLI {
            if let Some(v) = d.num(&[colonna]) {
                l.calcolati.push(Calcolato { nome: (*nome).into(), valore: v, unita: (*unita).into(), sorgenti: vec![m.id.clone()], da: (*da).into(), descrizione: String::new(), da_calcoli: Vec::new() });
            }
        }
        // Il resto delle colonne (miscela di reazione, nome della piastra, volumi, fattori di conversione…).
        for (k, v) in d.non_letti(&|_| false) {
            m.valori.push(Valore { chiave: k, dato: v, unita: String::new(), posto: Posto::Altro });
        }
        misure.push(m);
    }
    if misure.is_empty() {
        return Err("nel file non ci sono risultati per pozzetto".into());
    }

    // Quanti pozzetti ha la piastra: lo dice il tipo («Nanoplate 8.5K 96-well») o lo dicono i pozzetti usati.
    let usati = misure.iter().filter_map(|m| m.pozzetto.as_deref().and_then(crate::strumenti::pozzetto_rc)).fold((0, 0), |a, (r, c)| (a.0.max(r + 1), a.1.max(c + 1)));
    let dal_tipo = tipo_piastra.as_deref().and_then(|t| {
        let i = t.find("-well")?;
        t[..i].rsplit(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok()
    });
    let pozzetti = dal_tipo.unwrap_or(if usati.0 > 4 || usati.1 > 6 { 96 } else { 24 });

    l.gruppi.push(Gruppo { titolo: "QIAcuity".into(), pozzetti: Some(pozzetti), misure, ..Default::default() });
    l.strumento.produttore = "Qiagen".into();
    l.strumento.marchio = MARCHIO.into();
    l.strumento.software = SOFTWARE.into();
    l.strumento.identificativo = STRUMENTO.into();
    l.principale = "number concentration".into();
    l.unita = UNITA_CONC.into();
    Ok(())
}
