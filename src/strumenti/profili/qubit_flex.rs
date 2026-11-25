//! Thermo Fisher Qubit Flex: il CSV (anche con la riga «sep=,») o l'Excel dei risultati, una riga
//! per provetta. Fluorescenza in RFU; i campi dell'intervallo di lavoro e dei campioni di taratura
//! restano come note.

use super::tabella::Tabella;
use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_fisher_qubit_flex",
    nome: "Thermo Fisher Qubit Flex",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

/// La riga di intestazione: la prima che ha «Test Date» (salta l'eventuale «sep=,»).
fn intestazione(righe: &[Vec<String>]) -> Option<usize> {
    righe.iter().take(5).position(|r| r.iter().any(|c| c.trim() == "Test Date"))
}

fn riconosci(f: &Fonte) -> u8 {
    let righe = f.righe();
    let Some(i) = intestazione(&righe) else { return 0 };
    let t = Tabella::nuova(&righe[i]);
    // Il Qubit 4 ha «Green RFU» e «Emission»; il Flex ha «Sample RFU».
    if t.ha("sample rfu") && t.ha("test date") {
        96
    } else {
        0
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe();
    let i = intestazione(&righe).ok_or("manca la riga con «Test Date»")?;
    let t = Tabella::nuova(&righe[i]);
    let dati: Vec<&Vec<String>> = righe[i + 1..].iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    if dati.is_empty() {
        return Err("nel file non ci sono misure".into());
    }
    let date: Vec<&str> = dati.iter().filter_map(|r| t.cella(r, &["test date"])).collect();
    let gp = tempo::giorno_prima(date.iter().copied());
    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.modello = "Qubit Flex".into();
    l.strumento.software = "Qubit Flex software".into();
    l.strumento.versione = dati.first().and_then(|r| t.cella(r, &["software version"])).unwrap_or_default().to_string();
    l.info.push(("Marchio".into(), "Qubit".into()));
    l.principale = "fluorescence".into();
    l.unita = "RFU".into();
    let mut g = Gruppo { titolo: "Qubit Flex".into(), ..Default::default() };
    g.dati.push(("container type".into(), "tube".into()));
    let mut saggio: Option<String> = None;
    // Le colonne di una nota non vanno ripetute fra le «altre»: si segnano lette.
    t.leggi(&["software version", "excitation", "dilution factor", "sample volume (ul)", "sample volume (µl)", "sample volume (μl)"]);
    for r in dati {
        let Some(campione) = t.prima(r, &["sample id", "sample name"]) else { continue };
        let mut m = Misura { campione: campione.to_string(), luogo: t.cella(r, &["well"]).map(str::to_string), piastra: t.cella(r, &["plate barcode"]).map(str::to_string), quando: t.cella(r, &["test date"]).and_then(|d| tempo::iso(d, gp)), ..Default::default() };
        if saggio.is_none() {
            saggio = t.cella(r, &["assay name"]).map(str::to_string);
        }
        if let Some(b) = t.cella(r, &["run id"]) {
            m.valori.push(Valore::testo("batch identifier", b, Posto::Campione));
        }
        m.valori.push(match t.num(r, &["sample rfu"]) {
            Some(v) => Valore::num("fluorescence", v, "RFU", Posto::Misura),
            None => Valore { chiave: "fluorescence".into(), dato: Dato::Assente(String::new()), unita: "RFU".into(), posto: Posto::Misura },
        });
        // Impostazioni dello strumento.
        for (k, nome) in [("extended low range", "operating minimum"), ("extended high range", "operating maximum"), ("core range", "operating range")] {
            if let Some(v) = t.cella(r, &[k]) {
                m.valori.push(Valore::testo(nome, &format!("{v} ng/µL"), Posto::ImpostazioneAltro));
            }
        }
        if let Some(v) = t.num(r, &["sample volume (ul)", "sample volume (µl)", "sample volume (μl)"]) {
            m.valori.push(Valore::num("sample volume setting", v, "μL", Posto::ImpostazioneAltro));
        }
        if let Some(e) = t.cella(r, &["excitation"]) {
            m.valori.push(Valore::testo("excitation setting", e, Posto::ImpostazioneAltro));
        }
        if let Some(v) = t.num(r, &["dilution factor"]) {
            m.valori.push(Valore::num("dilution factor", v, "", Posto::ImpostazioneAltro));
        }
        // Il campione.
        if let Some(d) = t.cella(r, &["last read standards"]) {
            m.valori.push(Valore::testo("last read standards", &tempo::iso(d, gp).unwrap_or_else(|| d.to_string()), Posto::CampioneAltro));
        }
        if let Some(v) = t.num(r, &["selected samples"]) {
            m.valori.push(Valore::num("selected samples", v, "", Posto::CampioneAltro));
        }
        if let Some(v) = t.num(r, &["qubit tube conc."]) {
            m.valori.push(Valore::num("qubit tube concentration", v, t.cella(r, &["qubit tube conc. units"]).unwrap_or(""), Posto::CampioneAltro));
        }
        for (k, nome) in [("std 1 rfu", "standard 1 concentration"), ("std 2 rfu", "standard 2 concentration"), ("std 3 rfu", "standard 3 concentration")] {
            if let Some(v) = t.num(r, &[k]) {
                m.valori.push(Valore::num(nome, v, "RFU", Posto::CampioneAltro));
            }
        }
        if let Some(v) = t.num(r, &["original sample conc."]) {
            m.valori.push(Valore::num("original sample concentration", v, t.cella(r, &["original sample conc. units"]).unwrap_or(""), Posto::CampioneAltro));
        }
        for (h, c) in t.avanzi(r) {
            m.valori.push(match numero(&c) {
                Some(v) => Valore { chiave: h, dato: Dato::Numero(v), unita: String::new(), posto: Posto::Altro },
                None => Valore::testo(&h, &c, Posto::Altro),
            });
        }
        g.misure.push(m);
    }
    if let Some(s) = saggio {
        g.dati.push(("experiment type".into(), s));
    }
    l.gruppi.push(g);
    Ok(())
}
