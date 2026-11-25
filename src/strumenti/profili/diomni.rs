//! Thermo Fisher Diomni: l'Excel che il programma Diomni esporta dopo una corsa di PCR in tempo reale
//! (QuantStudio 7 Pro): un foglio «Target_Call» (versione 4.2.0) o «Target Call» (4.3.0) con le
//! righe d'intestazione «chiave, valore» e, dopo la riga «Well», una riga per pozzetto e bersaglio
//! con l'esito (Cq, Call, Amp score…).

use super::tabella::Tabella;
use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_fisher_diomni",
    nome: "Thermo Fisher Diomni",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Qpcr,
    estensioni: &["xlsx"],
    riconosci,
    leggi,
};

fn foglio(f: &Fonte) -> Option<&crate::strumenti::fonte::Foglio> {
    f.fogli.iter().find(|g| g.nome == "Target_Call" || g.nome == "Target Call")
}

/// La riga della tabella: quella che comincia con «Well».
fn riga_well(righe: &[Vec<String>]) -> Option<usize> {
    righe.iter().position(|r| r.first().is_some_and(|c| c.trim() == "Well"))
}

fn riconosci(f: &Fonte) -> u8 {
    let Some(g) = foglio(f) else { return 0 };
    if riga_well(&g.righe).is_some() {
        95
    } else {
        40
    }
}

/// Il ruolo dalla colonna «Sample type».
fn ruolo(t: &str) -> Option<Ruolo> {
    let t = t.to_uppercase();
    if t.contains("UNKNOWN") {
        Some(Ruolo::Campione)
    } else if t.contains("NTC") || t.contains("NEGATIVE") {
        Some(Ruolo::ControlloNegativo)
    } else if t.contains("STANDARD") {
        Some(Ruolo::Standard)
    } else if t.contains("CONTROL") || t.contains("POSITIVE") {
        Some(Ruolo::ControlloPositivo)
    } else {
        None
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let g = foglio(f).ok_or("manca il foglio «Target_Call» o «Target Call»")?;
    let righe = &g.righe;
    let w = riga_well(righe).ok_or("non trovo la tabella dei risultati (la riga che comincia con «Well»)")?;

    // L'intestazione: chiave nella prima colonna, valore nella seconda.
    let testa_file: Vec<(String, Option<String>)> = righe[..w]
        .iter()
        .filter_map(|r| {
            let k = r.first()?.trim();
            (!k.is_empty()).then(|| (k.to_string(), r.get(1).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())))
        })
        .collect();
    let h = |k: &str| testa_file.iter().find(|(a, _)| a == k).and_then(|(_, v)| v.as_deref());

    let nome_file = h("File Name").map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string());
    let blocco = h("Block Type").unwrap_or("");
    let n_pozzetti: u32 = if blocco.contains("384") { 384 } else { 96 };
    let volume = h("Sample Volume").and_then(numero).or_else(|| super::quantstudio::volume_pozzetto(blocco)).unwrap_or(15.0);
    let quando = h("Run End Data/Time").or_else(|| h("Run End Date/Time")).and_then(|d| tempo::iso(d, false));
    let analista = h("Operator").map(str::to_string);
    let barcode = h("Barcode").map(str::to_string);
    let metodo = h("Quantification Cycle Method");
    let passivo = h("Passive Reference");

    let t = Tabella::nuova(&righe[w]);
    let mut gruppi: Vec<Gruppo> = Vec::new();
    for r in &righe[w + 1..] {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let Some(pozzetto) = t.cella(r, &["well"]) else { continue };
        let campione = t.cella(r, &["sample"]).unwrap_or(pozzetto).to_string();
        let bersaglio = t.cella(r, &["target"]).unwrap_or("N/A").to_string();
        let gi = match gruppi.iter().position(|g| g.titolo == bersaglio) {
            Some(i) => i,
            None => {
                let mut g = Gruppo { titolo: bersaglio.clone(), quando: quando.clone(), piastra: barcode.clone(), pozzetti: Some(n_pozzetti), ..Default::default() };
                if let Some(a) = &analista {
                    g.dati.push(("analyst".into(), a.clone()));
                }
                g.dati.push(("experimental data identifier".into(), nome_file.clone().unwrap_or_else(|| "N/A".into())));
                g.dati.push(("experiment type".into(), "qPCR experiment".into()));
                g.dati.push(("well volume".into(), format!("{volume}")));
                gruppi.push(g);
                gruppi.len() - 1
            }
        };
        let mut m = Misura { id: l.id(), campione, pozzetto: Some(pozzetto.to_string()), luogo: Some(pozzetto.to_string()), piastra: barcode.clone(), bersaglio: Some(bersaglio), quando: quando.clone(), ..Default::default() };
        if let Some(rl) = t.cella(r, &["sample type"]).and_then(ruolo) {
            m.ruolo = rl;
            m.ruolo_dal_file = true;
        }
        if let Some(x) = t.cella(r, &["reporter"]) {
            m.valori.push(Valore::testo("reporter dye setting", x, Posto::Impostazione));
        }
        if let Some(x) = passivo {
            m.valori.push(Valore::testo("passive reference dye setting", x, Posto::Impostazione));
        }
        if let Some(x) = metodo {
            m.valori.push(Valore::testo("measurement method identifier", x, Posto::Impostazione));
        }
        // Il Cq: un numero, o «UNDETERMINED» quando il pozzetto non ha amplificato.
        match t.cella(r, &["cq"]) {
            Some(c) => match numero(c) {
                Some(x) => m.valori.push(Valore::num("cycle threshold result (qPCR)", x, "", Posto::Elaborato)),
                None => m.valori.push(Valore { chiave: "cycle threshold result (qPCR)".into(), dato: Dato::Assente(c.to_string()), unita: String::new(), posto: Posto::Elaborato }),
            },
            None => {}
        }
        if let Some(x) = t.num(r, &["delta rn"]) {
            m.valori.push(Valore::num("baseline corrected reporter result", x, "", Posto::Elaborato));
        }
        // Le valutazioni del programma: sono informazioni sul risultato.
        for (colonna, chiave, e_numero) in [
            ("call", "call", false),
            ("cq confidence", "cq confidence", true),
            ("amp score", "amp score", true),
            ("amp status", "amp status", false),
            ("quality issue(s)", "quality issues", false),
            ("manual call", "manual call", false),
            ("comment", "comment", false),
        ] {
            let Some(c) = t.cella(r, &[colonna]) else { continue };
            match (e_numero, numero(c)) {
                (true, Some(x)) => m.valori.push(Valore::num(chiave, x, "", Posto::ElaboratoAltro)),
                _ => m.valori.push(Valore::testo(chiave, c, Posto::ElaboratoAltro)),
            }
        }
        // Un pozzetto escluso dall'analisi lo si vede: «Omit» a TRUE.
        if t.cella(r, &["omit"]).is_some_and(|o| o.eq_ignore_ascii_case("true")) {
            m.valori.push(Valore::testo("omit", "TRUE", Posto::ElaboratoAltro));
        }
        for (k, v) in t.avanzi(r) {
            m.valori.push(match numero(&v) {
                Some(x) => Valore::num(&k, x, "", Posto::ElaboratoAltro),
                None => Valore::testo(&k, &v, Posto::ElaboratoAltro),
            });
        }
        gruppi[gi].misure.push(m);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono risultati per pozzetto".into());
    }

    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.identificativo = h("Instrument Name").unwrap_or("").to_string();
    l.strumento.modello = h("Instrument Type").unwrap_or("").to_string();
    l.strumento.seriale = h("Instrument Serial Number").unwrap_or("").to_string();
    l.strumento.software = h("Software name").unwrap_or("Diomni").to_string();
    l.strumento.versione = h("Software version").unwrap_or("").to_string();
    for (etichetta, chiave) in [
        ("File della corsa", "File Name"),
        ("Tipo di blocco", "Block Type"),
        ("Codice a barre", "Barcode"),
        ("Inizio della corsa", "Run Start Date/Time"),
        ("Fine della corsa", "Run End Data/Time"),
        ("Durata", "Run Duration"),
        ("Metodo di quantificazione", "Quantification Cycle Method"),
        ("Saggio", "Assay name"),
        ("Versione del saggio", "Assay version"),
    ] {
        if let Some(v) = h(chiave).or_else(|| (chiave == "Run End Data/Time").then(|| h("Run End Date/Time")).flatten()) {
            l.info.push((etichetta.into(), v.to_string()));
        }
    }
    l.principale = "cycle threshold result (qPCR)".into();
    l.unita = String::new();
    l.gruppi.extend(gruppi);
    Ok(())
}
