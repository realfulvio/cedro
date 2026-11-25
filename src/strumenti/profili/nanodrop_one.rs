//! Thermo Fisher NanoDrop One: il CSV o l'Excel con una riga per campione, le assorbanze (A260,
//! A280…), i rapporti (A260/A280) e la concentrazione degli acidi nucleici.

use crate::strumenti::fonte::{colonna, numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_fisher_nanodrop_one",
    nome: "Thermo Fisher NanoDrop One",
    produttore: "ThermoFisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let testa = f.righe().into_iter().next().unwrap_or_default();
    let ha = |p: &str| testa.iter().any(|c| c.trim().starts_with(p));
    if ha("Sample Name") && ha("Nucleic Acid") && ha("Baseline Correction") && testa.iter().any(|c| c.trim() == "A260/A280") {
        95
    } else {
        0
    }
}

/// «A260» → 260.
fn lunghezza_onda(c: &str) -> Option<f64> {
    let c = c.trim().strip_prefix('A')?;
    (c.len() == 3 && c.bytes().all(|b| b.is_ascii_digit())).then(|| c.parse().ok()).flatten()
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe();
    let testa = righe.first().ok_or("file vuoto")?;
    let data = colonna(testa, &["Date"]).ok_or("manca la colonna Date")?;
    let nome = colonna(testa, &["Sample Name"]).ok_or("manca la colonna Sample Name")?;
    let acido = testa.iter().position(|c| c.trim().starts_with("Nucleic Acid") && c.contains('(') && !c.contains("Factor")).ok_or("manca la colonna Nucleic Acid(…)")?;
    let unita_acido = testa[acido].split_once('(').and_then(|(_, r)| r.rsplit_once(')')).map(|(u, _)| u.trim().to_string()).ok_or("manca l'unità degli acidi nucleici")?;
    let fattore = colonna(testa, &["Nucleic Acid Factor"]);
    let correzione = testa.iter().position(|c| c.trim().starts_with("Baseline Correction")).ok_or("manca la colonna Baseline Correction")?;
    let base = colonna(testa, &["Baseline Absorbance"]);
    let assorbanze: Vec<(usize, f64)> = testa.iter().enumerate().filter_map(|(i, c)| lunghezza_onda(c).map(|w| (i, w))).collect();
    let rapporti: Vec<(usize, String)> = testa.iter().enumerate().filter(|(_, c)| c.split_once('/').is_some_and(|(a, b)| lunghezza_onda(a).is_some() && lunghezza_onda(b).is_some())).map(|(i, c)| (i, c.trim().to_string())).collect();
    if assorbanze.is_empty() {
        return Err("nel file non ci sono assorbanze (A260, A280…)".into());
    }
    let gia: Vec<usize> = [Some(data), Some(nome), Some(acido), fattore, Some(correzione), base].into_iter().flatten().chain(assorbanze.iter().map(|a| a.0)).chain(rapporti.iter().map(|a| a.0)).collect();
    // Tipo di saggio: la prima parola del nome del foglio o del file («dsDNA 7_19_2023…»).
    let saggio = f.fogli.first().map(|s| s.nome.as_str()).unwrap_or(f.nome.as_str()).split_whitespace().next().unwrap_or("").to_string();
    let gp = tempo::giorno_prima(righe[1..].iter().filter_map(|r| r.get(data)).map(String::as_str));

    l.strumento.produttore = "ThermoFisher Scientific".into();
    l.strumento.modello = "NanoDrop One".into();
    l.strumento.software = "NanoDrop One software".into();
    l.info.push(("Marchio".into(), "NanoDrop".into()));
    l.principale = "absorbance".into();
    l.unita = "AU".into();
    let mut g = Gruppo { titolo: if saggio.is_empty() { "NanoDrop One".into() } else { saggio.clone() }, ..Default::default() };
    if !saggio.is_empty() {
        g.dati.push(("experiment type".into(), saggio));
    }
    let cella = |r: &Vec<String>, i: usize| r.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
    for r in &righe[1..] {
        let campione = cella(r, nome);
        if campione.is_empty() {
            continue;
        }
        let quando = tempo::iso(&cella(r, data), gp);
        let concentrazione = numero(&cella(r, acido));
        let mut ids: Vec<(f64, String)> = Vec::new();
        for &(col, w) in &assorbanze {
            let mut m = Misura { id: l.id(), campione: campione.clone(), quando: quando.clone(), ..Default::default() };
            ids.push((w, m.id.clone()));
            m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
            if let Some(v) = numero(&cella(r, correzione)) {
                m.valori.push(Valore::num("electronic absorbance reference bandwidth setting", v, "nm", Posto::Impostazione));
            }
            if let Some(v) = fattore.and_then(|i| numero(&cella(r, i))) {
                m.valori.push(Valore::num("nucleic acid factor", v, "", Posto::ImpostazioneAltro));
            }
            m.valori.push(match numero(&cella(r, col)) {
                Some(v) => Valore::num("absorbance", v, "AU", Posto::Misura),
                None => Valore { chiave: "absorbance".into(), dato: Dato::Assente(String::new()), unita: "AU".into(), posto: Posto::Misura },
            });
            if let Some(v) = concentrazione {
                m.valori.push(Valore::num("mass concentration", v, &unita_acido, Posto::Elaborato));
            }
            if let Some(v) = base.and_then(|i| numero(&cella(r, i))) {
                m.valori.push(Valore::num("baseline absorbance", v, "AU", Posto::Altro));
            }
            // Le altre colonne piene (corretta, impurità…) restano come note della misura.
            for (i, h) in testa.iter().enumerate() {
                let t = cella(r, i);
                if !gia.contains(&i) && !t.is_empty() && !h.trim().is_empty() {
                    m.valori.push(match numero(&t) {
                        Some(v) => Valore { chiave: h.trim().to_string(), dato: Dato::Numero(v), unita: String::new(), posto: Posto::Altro },
                        None => Valore::testo(h.trim(), &t, Posto::Altro),
                    });
                }
            }
            g.misure.push(m);
        }
        for (col, nome_rapporto) in &rapporti {
            let Some(v) = numero(&cella(r, *col)) else { continue };
            let sorgenti: Vec<String> = nome_rapporto.split('/').filter_map(lunghezza_onda).filter_map(|w| ids.iter().find(|(x, _)| *x == w).map(|(_, id)| id.clone())).collect();
            l.calcolati.push(Calcolato { nome: nome_rapporto.clone(), valore: v, unita: String::new(), sorgenti, da: "absorbance".into() });
        }
    }
    l.gruppi.push(g);
    Ok(())
}
