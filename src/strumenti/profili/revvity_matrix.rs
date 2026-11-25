//! Revvity Matrix (Cellaca): CSV o Excel, o con le righe d'intestazione (piastra, diluizione, saggio,
//! ora, numero di serie, operatore, versione) sopra la tabella, o la sola tabella dei pozzetti.

use super::conta::{nota, Conta};
use super::tabella::Tabella;
use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "revvity_matrix",
    nome: "Revvity Matrix (Cellaca)",
    produttore: "Revvity",
    tecnica: Tecnica::Contacellule,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

fn righe(f: &Fonte) -> Vec<Vec<String>> {
    f.righe_con(',').into_iter().map(|r| r.iter().map(|c| c.trim_start_matches('\u{feff}').to_string()).collect()).collect()
}

/// La riga con «Row» e «Column»: da lì comincia la tabella dei pozzetti.
fn inizio_dati(r: &[Vec<String>]) -> Option<usize> {
    r.iter().position(|x| x.iter().any(|c| c.trim() == "Row") && x.iter().any(|c| c.trim() == "Column"))
}

fn riconosci(f: &Fonte) -> u8 {
    let r = righe(f);
    let Some(prima) = r.first() else { return 0 };
    let ha = |n: &str| prima.iter().any(|c| c.trim().eq_ignore_ascii_case(n));
    if prima.first().is_some_and(|c| c.trim() == "Plate Name") && inizio_dati(&r).is_some() {
        return 92;
    }
    if ha("Well Name") && (ha("Live Count") || ha("Live Cells/mL") || ha("Live Concentration (E6)")) {
        return 92;
    }
    0
}

/// «Dilution: 2» → «2».
fn dopo_i_due_punti(v: &str) -> &str {
    v.rsplit(':').next().unwrap_or(v).trim()
}

/// Gli errori dello strumento sono messaggi attaccati uno all'altro («No Cells found in F2 Image C,
/// Intensity range to lowNo Cells found in F2 Image D…»): si separano all'inizio di ognuno.
fn errori(s: &str) -> Vec<String> {
    const INIZIO: &str = "No Cells found";
    if s.matches(INIZIO).count() > 1 {
        s.split(INIZIO).filter(|x| !x.trim().is_empty()).map(|x| format!("{INIZIO}{}", x.trim_end_matches(',').trim_end())).collect()
    } else {
        vec![s.trim().to_string()]
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let r = righe(f);
    let (intestazioni, testa_riga) = match inizio_dati(&r).filter(|_| r.first().and_then(|x| x.first()).is_some_and(|c| c.trim() == "Plate Name")) {
        Some(i) => {
            let h: Vec<(String, String)> = r[..i].iter().filter_map(|x| {
                let (k, v) = (x.first()?.trim(), x.get(1)?.trim());
                (!k.is_empty() && !v.is_empty()).then(|| (k.to_string(), if matches!(k, "Dilution" | "Assay Name") { dopo_i_due_punti(v).to_string() } else { v.to_string() }))
            }).collect();
            (h, i)
        }
        None => (Vec::new(), 0),
    };
    let chiavi: Vec<String> = intestazioni.iter().map(|(k, _)| k.clone()).collect();
    let valori: Vec<String> = intestazioni.iter().map(|(_, v)| v.clone()).collect();
    let h = Tabella::nuova(&chiavi);
    let t = Tabella::nuova(r.get(testa_riga).ok_or("file vuoto")?);
    let righe_dati: Vec<&Vec<String>> = r[testa_riga + 1..].iter().filter(|x| x.iter().any(|c| !c.trim().is_empty())).collect();
    if righe_dati.is_empty() {
        return Err("nel file non ci sono pozzetti".into());
    }
    l.strumento.produttore = "Revvity".into();
    l.strumento.software = "Revvity Matrix".into();
    l.strumento.versione = h.cella(&valori, &["version"]).unwrap_or_default().to_string();
    l.strumento.seriale = h.cella(&valori, &["instrument sn"]).unwrap_or_default().to_string();
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();
    let operatore = h.cella(&valori, &["operator"]).unwrap_or("Revvity Matrix").to_string();
    let saggio = h.cella(&valori, &["assay name"]).map(str::to_string);
    let piastra = h.cella(&valori, &["plate name"]).map(str::to_string);
    let diluizione = h.num(&valori, &["dilution"]);
    let quando = h.cella(&valori, &["results timestamp"]).and_then(|d| tempo::iso(d, false));
    let scansione = h.cella(&valori, &["scanresult timestamp"]).map(str::to_string);
    // Un Excel con la vitalità scritta come frazione (0,87) e non come percentuale: si porta a 0-100 come fa allotropy.
    let scala = f.foglio() && intestazioni.is_empty() && righe_dati.first().and_then(|x| t.cella(x, &["viability", "viability (%)"])).is_some_and(|s| !s.contains('%') && super::super::fonte::numero(s).is_some_and(|v| v < 1.0));
    let milioni = |v: Option<f64>| v.map(super::conta::milioni);
    let mut gruppi = Vec::new();
    for x in righe_dati {
        let Some(campione) = t.cella(x, &["well name"]) else { continue };
        let mut m = Misura { id: l.id(), campione: campione.to_string(), quando: quando.clone(), ..Default::default() };
        let concentrazione = |e6: &str, ml: &str| t.num(x, &[e6]).or_else(|| milioni(t.num(x, &[ml])));
        let c = Conta {
            dispositivo: Some("cell counter"),
            diluizione,
            vitalita: t.num(x, &["viability", "viability (%)"]).map(|v| if scala { v * 100.0 } else { v }),
            densita_vive: concentrazione("live concentration (e6)", "live cells/ml"),
            densita_morte: concentrazione("dead concentration (e6)", "dead cells/ml"),
            densita_totale: concentrazione("total concentration (e6)", "total cells/ml"),
            vive: t.num(x, &["live count"]),
            morte: t.num(x, &["dead count"]),
            totale: t.num(x, &["total count"]),
            diam_medio: t.num(x, &["total mean size", "total diameter"]),
            diam_medio_vive: t.num(x, &["live mean size", "live diameter"]),
            diam_medio_morte: t.num(x, &["dead mean size", "dead diameter"]),
            ..Default::default()
        };
        c.applica(&mut m);
        for (nome, chiave, unita) in [("aggregates (%)", "cell aggregation percentage", "%"), ("aggregate size", "aggregate size", "µm"), ("aggregate count", "aggregate count", "cell")] {
            if let Some(v) = t.num(x, &[nome]) {
                m.valori.push(Valore::num(chiave, v, unita, Posto::ElaboratoAltro));
            }
        }
        for (nome, chiave) in [("row", "Row"), ("column", "Column")] {
            if let Some(v) = t.cella(x, &[nome]) {
                m.valori.push(Valore::testo(chiave, v, Posto::CampioneAltro));
            }
        }
        if let Some(p) = &piastra {
            m.valori.push(Valore::testo("Well Plate Identifier", p, Posto::CampioneAltro));
        }
        if let Some(s) = &scansione {
            m.valori.push(Valore::testo("Scanresult Timestamp", s, Posto::Altro));
        }
        if let Some(e) = t.prima(x, &["errors:", "errors"]) {
            m.note.extend(errori(e).into_iter().map(|e| (e, "Cell Counting".to_string())));
        }
        for (k, v) in t.avanzi(x) {
            m.valori.push(nota(&k, &v, Posto::Altro));
        }
        for (k, v) in h.avanzi(&valori) {
            m.valori.push(nota(&k, &v, Posto::Altro));
        }
        let mut dati = vec![("analyst".to_string(), operatore.clone())];
        if let Some(s) = &saggio {
            dati.push(("assay identifier".into(), s.clone()));
        }
        super::conta::in_gruppo(&mut gruppi, "Revvity Matrix", dati, m);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono pozzetti con un nome".into());
    }
    l.gruppi.extend(gruppi);
    Ok(())
}
