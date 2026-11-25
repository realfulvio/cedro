//! Bio-Rad CFX Maestro: il CSV dei risultati di qPCR, una riga per pozzetto e colorante.

use crate::strumenti::fonte::{colonna, numero, Fonte};
use crate::strumenti::{Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "cfxmaestro",
    nome: "Bio-Rad CFX Maestro",
    produttore: "Bio-Rad",
    tecnica: Tecnica::Qpcr,
    estensioni: &["csv"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let testa = f.righe_con(',').into_iter().next().unwrap_or_default();
    let tutte = ["Well", "Fluor", "Content", "Sample", "Cq", "Cq Mean"];
    if tutte.iter().all(|k| colonna(&testa, &[k]).is_some()) {
        95
    } else {
        0
    }
}

/// Le colonne che finiscono in note della misura, nell'ordine del file.
const ALTRE: [&str; 7] = ["Starting Quantity (SQ)", "Log Starting Quantity", "SQ Std. Dev", "SQ Mean", "Well Note", "Set Point", "Cq Std. Dev"];

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe_con(',');
    let testa = righe.first().ok_or("file vuoto")?;
    let c = |k: &str| colonna(testa, &[k]);
    let pozzetto = c("Well").ok_or("manca la colonna Well")?;
    let colorante = c("Fluor");
    let bersaglio = c("Target");
    let contenuto = c("Content");
    let campione = c("Sample");
    let insieme = c("Biological Set Name");
    let cq = c("Cq").ok_or("manca la colonna Cq")?;
    let ciclo = c("Cycle Number");
    let cella = |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();

    l.strumento.produttore = "Bio-Rad".into();
    l.strumento.software = "CFX Maestro".into();
    l.principale = "cycle threshold result (qPCR)".into();
    l.unita = String::new();
    let mut g = Gruppo { titolo: "Corsa di qPCR".into(), ..Default::default() };
    let mut pozzetti: Vec<String> = Vec::new();
    for r in &righe[1..] {
        let w = cella(r, Some(pozzetto));
        if w.is_empty() {
            continue;
        }
        let nome = cella(r, campione);
        let valore_cq = numero(&cella(r, Some(cq)));
        if nome.is_empty() && valore_cq.is_none() {
            continue;
        }
        if !pozzetti.contains(&w) {
            pozzetti.push(w.clone());
        }
        let mut m = Misura { pozzetto: Some(w), campione: nome, bersaglio: Some(cella(r, bersaglio)).filter(|t| !t.is_empty()), ..Default::default() };
        if let Some(c) = contenuto {
            let testo = cella(r, Some(c));
            m.ruolo = match testo.as_str() {
                "Unkn" => Ruolo::Campione,
                "Std" => Ruolo::Standard,
                "NTC" | "Pos" | "Neg" | "NRT" => Ruolo::Controllo,
                _ => Ruolo::Campione,
            };
            m.ruolo_dal_file = !testo.is_empty();
            if matches!(testo.as_str(), "NTC" | "Pos" | "Neg" | "NRT") {
                m.valori.push(Valore::testo("Content", &testo, Posto::Altro));
            }
        }
        if let Some(i) = insieme {
            let t = cella(r, Some(i));
            if !t.is_empty() {
                m.valori.push(Valore::testo("Biological Set Name", &t, Posto::Altro));
            }
        }
        m.valori.push(Valore::testo("reporter dye setting", &cella(r, colorante), Posto::Impostazione));
        m.valori.push(match valore_cq {
            Some(v) => Valore::num("cycle threshold result (qPCR)", v, "", Posto::Elaborato),
            None => Valore { chiave: "cycle threshold result (qPCR)".into(), dato: Dato::Assente("Undetermined".into()), unita: String::new(), posto: Posto::Elaborato },
        });
        m.valori.push(match ciclo.and_then(|i| r.get(i)).and_then(|t| numero(t)) {
            Some(v) => Valore::num("cycle threshold value setting (qPCR)", v, "", Posto::Elaborazione),
            None => Valore { chiave: "cycle threshold value setting (qPCR)".into(), dato: Dato::Assente(String::new()), unita: String::new(), posto: Posto::Elaborazione },
        });
        for k in ALTRE {
            if let Some(i) = c(k) {
                let t = cella(r, Some(i));
                if t.is_empty() {
                    continue;
                }
                m.valori.push(match numero(&t) {
                    Some(v) => Valore { chiave: k.into(), dato: Dato::Numero(v), unita: String::new(), posto: Posto::Altro },
                    None => Valore::testo(k, &t, Posto::Altro),
                });
            }
        }
        g.misure.push(m);
    }
    g.pozzetti = Some(pozzetti.len() as u32);
    l.gruppi.push(g);
    Ok(())
}
