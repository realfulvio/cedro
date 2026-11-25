//! Roche Cedex HiRes: CSV o Excel con una riga per campione e le colonne del sistema (numero, nome,
//! descrizione) ripetute in ogni riga.

use super::conta::{in_gruppo, nota, Conta};
use super::tabella::Tabella;
use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "roche_cedex_hires",
    nome: "Roche Cedex HiRes",
    produttore: "Roche",
    tecnica: Tecnica::Contacellule,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

/// Alcuni file scrivono «identifer»: la colonna è la stessa.
fn righe(f: &Fonte) -> Vec<Vec<String>> {
    let mut r: Vec<Vec<String>> = f.righe_con(',');
    if let Some(prima) = r.first_mut() {
        for c in prima.iter_mut() {
            *c = c.trim_start_matches('\u{feff}').replace("identifer", "identifier");
        }
    }
    r
}

fn riconosci(f: &Fonte) -> u8 {
    let r = righe(f);
    if r.first().is_some_and(|p| p.iter().any(|c| c.trim() == "Cedex ID")) && r.first().is_some_and(|p| p.iter().any(|c| c.trim().eq_ignore_ascii_case("Viable Cell Conc."))) {
        92
    } else {
        0
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let r = righe(f);
    let t = Tabella::nuova(r.first().ok_or("file vuoto")?);
    let dati: Vec<&Vec<String>> = r[1..].iter().filter(|x| x.iter().any(|c| !c.trim().is_empty())).collect();
    let primo = dati.first().ok_or("nel file non ci sono campioni")?;
    l.strumento.produttore = "Roche".into();
    l.strumento.modello = "Cedex HiRes".into();
    l.strumento.software = "Cedex software".into();
    l.strumento.marchio = "Cedex".into();
    l.strumento.identificativo = t.cella(primo, &["cedex id"]).unwrap_or_default().to_string();
    l.strumento.inventario = t.cella(primo, &["system name"]).unwrap_or_default().to_string();
    l.strumento.descrizione = t.cella(primo, &["system description"]).unwrap_or_default().to_string();
    for (nome, chiave) in [("default system", "Default system"), ("default system (processed)", "Default system (processed)"), ("system description (processed)", "System description (processed)")] {
        if let Some(v) = t.cella(primo, &[nome]) {
            l.dispositivo.push(nota(chiave, v, Posto::Altro));
        }
    }
    // Le altre colonne del sistema ripetono le prime: non sono note.
    t.leggi(&["cedex id (processed)", "system name (processed)"]);
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();
    let milioni = |v: Option<f64>| v.map(super::conta::milioni);
    let mut gruppi = Vec::new();
    for x in dati {
        let Some(campione) = t.cella(x, &["sample identifier"]) else { continue };
        // Le date sono all'americana («6-18-2019 7:46:47 AM»).
        let quando = t.cella(x, &["date finished"]).and_then(|d| tempo::iso(d, false));
        let mut m = Misura { id: l.id(), campione: campione.to_string(), quando, ..Default::default() };
        if let Some(b) = t.cella(x, &["reactor identifier"]) {
            m.valori.push(Valore::testo("batch identifier", b, Posto::Campione));
        }
        if let Some(g) = t.cella(x, &["data set name"]) {
            m.valori.push(Valore::testo("group identifier", g, Posto::CampioneAltro));
        }
        if let Some(d) = t.cella(x, &["sample draw time"]) {
            m.valori.push(Valore::testo("sample draw time", &tempo::iso(d, false).unwrap_or_else(|| d.to_string()), Posto::CampioneAltro));
        }
        let c = Conta {
            dispositivo: Some("cell counter"),
            tipo_cellula: t.cella(x, &["cell type name"]).map(str::to_string),
            diluizione: t.num(x, &["dilution"]),
            volume: t.num(x, &["sample volume"]),
            vitalita: t.num(x, &["viability"]),
            densita_vive: milioni(t.num(x, &["viable cell conc."])),
            densita_morte: milioni(t.num(x, &["dead cell conc."])),
            densita_totale: milioni(t.num(x, &["total cell conc."])),
            vive: t.num(x, &["viable cell count"]),
            morte: t.num(x, &["dead cell count"]),
            totale: t.num(x, &["total cell count"]),
            diam_medio_vive: t.num(x, &["avg diameter"]),
            ..Default::default()
        };
        c.applica(&mut m);
        for (nome, chiave, unita) in [
            ("avg compactness", "average compactness", "(unitless)"),
            ("avg area", "average area", "(unitless)"),
            ("avg perimeter", "average perimeter", "µm"),
            ("avg segm. area", "average segment area", "(unitless)"),
            ("total object count", "total object count", "cell"),
            ("std dev.", "standard deviation", "cell"),
            ("aggregate rate", "aggregate rate", "%"),
        ] {
            if let Some(v) = t.num(x, &[nome]) {
                m.valori.push(Valore::num(chiave, v, unita, Posto::ElaboratoAltro));
            }
        }
        let mut dati = vec![("analyst".to_string(), t.cella(x, &["username"]).unwrap_or("Cedex").to_string())];
        for (nome, chiave) in [("workarea name", "Workarea name"), ("comment", "Comment")] {
            if let Some(v) = t.cella(x, &[nome]) {
                dati.push((chiave.to_string(), v.to_string()));
            }
        }
        for (k, v) in t.avanzi(x) {
            m.valori.push(nota(&k, &v, Posto::Altro));
        }
        in_gruppo(&mut gruppi, "Cedex HiRes", dati, m);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono campioni".into());
    }
    l.gruppi.extend(gruppi);
    Ok(())
}
