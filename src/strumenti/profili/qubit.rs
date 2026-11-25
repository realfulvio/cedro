//! Thermo Fisher Qubit 4: il CSV o l'Excel dei risultati, una riga per provetta.

use crate::strumenti::fonte::{colonna, numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_fisher_qubit4",
    nome: "Thermo Fisher Qubit 4",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let testa = f.righe().into_iter().next().unwrap_or_default();
    let ok = ["Run ID", "Assay Name", "Test Name", "Test Date"].iter().all(|k| colonna(&testa, &[k]).is_some()) && testa.iter().any(|c| c.contains("tube conc"));
    if ok {
        95
    } else {
        0
    }
}

/// La data della prova: l'ordine giorno/mese è quello per cui coincide con la data del Run ID.
fn quando(t: &str, run: &str, giorno_prima: bool) -> Option<String> {
    let run = tempo::iso(run, true).map(|r| r[..10].to_string());
    for gp in [giorno_prima, !giorno_prima] {
        if let (Some(i), Some(r)) = (tempo::iso(t, gp), &run) {
            if i.starts_with(r.as_str()) {
                return Some(i);
            }
        }
    }
    tempo::iso(t, giorno_prima)
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe();
    let testa = righe.first().ok_or("file vuoto")?;
    let c = |k: &str| colonna(testa, &[k]);
    let nome = c("Test Name").ok_or("manca la colonna Test Name")?;
    let data = c("Test Date").ok_or("manca la colonna Test Date")?;
    let run = c("Run ID");
    let saggio = c("Assay Name");
    let emissione = c("Emission").ok_or("manca la colonna Emission")?;
    let eccitazione = c("Excitation");
    let diluizione = c("Dilution Factor");
    let volume = colonna(testa, &["Sample Volume (µL)", "Sample Volume (μL)", "Sample Volume (uL)"]);
    let tubo = testa.iter().position(|h| h.trim().starts_with("Qubit") && h.contains("tube conc"));
    let originale = c("Original sample conc.");
    // «Units» sta dopo la colonna a cui si riferisce.
    let unita = |i: Option<usize>| i.and_then(|i| testa.get(i + 1)).filter(|h| h.trim() == "Units").and_then(|_| i).map(|i| i + 1);
    let (u_tubo, u_orig) = (unita(tubo), unita(originale));
    let cella = |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
    let gp = tempo::giorno_prima(righe[1..].iter().filter_map(|r| r.get(data)).map(String::as_str));

    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.modello = "Qubit 4".into();
    l.strumento.software = "Qubit 4 software".into();
    l.info.push(("Marchio".into(), "Qubit".into()));
    l.principale = "fluorescence".into();
    l.unita = "RFU".into();
    let mut g = Gruppo { titolo: "Qubit 4".into(), ..Default::default() };
    for r in &righe[1..] {
        let campione = cella(r, Some(nome));
        if campione.is_empty() {
            continue;
        }
        let em = cella(r, Some(emissione));
        let colonna_valore = match em.to_ascii_lowercase().as_str() {
            "green" => "Green RFU",
            "far red" => "Far Red RFU",
            _ => return Err(format!("emissione non riconosciuta: «{em}»")),
        };
        let run_id = cella(r, run);
        let mut m = Misura { campione, quando: quando(&cella(r, Some(data)), &run_id, gp), ..Default::default() };
        if let Some(a) = saggio {
            let t = cella(r, Some(a));
            if g.dati.is_empty() && !t.is_empty() {
                g.dati.push(("experiment type".into(), t));
            }
        }
        if !run_id.is_empty() {
            m.valori.push(Valore::testo("batch identifier", &run_id, Posto::Campione));
        }
        m.valori.push(match c(colonna_valore).and_then(|i| numero(&cella(r, Some(i)))) {
            Some(v) => Valore::num("fluorescence", v, "RFU", Posto::Misura),
            None => Valore { chiave: "fluorescence".into(), dato: Dato::Assente(String::new()), unita: "RFU".into(), posto: Posto::Misura },
        });
        if let Some(v) = numero(&cella(r, volume)) {
            m.valori.push(Valore::num("sample volume setting", v, "μL", Posto::ImpostazioneAltro));
        }
        let e = cella(r, eccitazione);
        if !e.is_empty() {
            m.valori.push(Valore::testo("excitation setting", &e, Posto::ImpostazioneAltro));
        }
        m.valori.push(Valore::testo("emission setting", &em, Posto::ImpostazioneAltro));
        if let Some(v) = numero(&cella(r, diluizione)) {
            m.valori.push(Valore::num("dilution factor", v, "", Posto::ImpostazioneAltro));
        }
        if let Some(v) = numero(&cella(r, tubo)) {
            m.valori.push(Valore::num("qubit tube concentration", v, &cella(r, u_tubo), Posto::CampioneAltro));
        }
        for (k, nome) in [("Std 1 RFU", "standard 1 concentration"), ("Std 2 RFU", "standard 2 concentration"), ("Std 3 RFU", "standard 3 concentration")] {
            if let Some(v) = c(k).and_then(|i| numero(&cella(r, Some(i)))) {
                m.valori.push(Valore::num(nome, v, "RFU", Posto::CampioneAltro));
            }
        }
        if let Some(v) = numero(&cella(r, originale)) {
            m.valori.push(Valore::num("original sample concentration", v, &cella(r, u_orig), Posto::CampioneAltro));
        }
        g.misure.push(m);
    }
    l.gruppi.push(g);
    Ok(())
}
