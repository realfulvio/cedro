//! MSD Discovery Workbench: l'export di una piastra Meso Scale (CSV o Excel, foglio «Workbench data»).
//! La prima riga dice la piastra («Plate_2BO40AW287»), la seconda le colonne (campione, saggio, pozzetto,
//! punto, segnale…), poi una riga per pozzetto e punto. Le colonne che il programma calcola (segnale
//! corretto, media, CV, recupero, concentrazione calcolata…) diventano i calcoli, con le misure da cui
//! vengono.

use std::collections::HashMap;
use std::rc::Rc;

use super::tabella::Tabella;
use crate::strumenti::calcdocs::{self as cd, Config, Elemento, Ordina, Sorgente, Vista};
use crate::strumenti::fonte::{numero, Fonte, Foglio};
use crate::strumenti::{Calcolato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "msd_workbench",
    nome: "MSD Discovery Workbench",
    produttore: "Meso Scale Diagnostics",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

/// Le colonne che il programma calcola: nel file di Cedro sono calcoli, non note.
const CALCOLATE: &[&str] = &[
    "Adjusted Signal",
    "Mean",
    "Adj. Sig. Mean",
    "Fit Statistic: RSquared",
    "CV",
    "% Recovery",
    "% Recovery Mean",
    "Calc. Concentration",
    "Calc. Conc. Mean",
    "Calc. Conc. CV",
];

/// Le righe del file: il foglio «Workbench data» per Excel, il CSV per il resto.
fn righe(f: &Fonte) -> Option<Vec<Vec<String>>> {
    if f.ext == "xlsx" {
        let g: &Foglio = f.fogli.iter().find(|g| g.nome.trim().eq_ignore_ascii_case("workbench data"))?;
        Some(g.righe.clone())
    } else {
        Some(f.righe_con(','))
    }
}

/// La piastra dalla prima riga: «Plate_2BO40AW287» → «2BO40AW287».
fn piastra(r: &[Vec<String>]) -> Option<String> {
    let prima = r.first()?.first()?.trim();
    prima.contains("Plate").then(|| prima.rsplit('_').next().unwrap_or(prima).trim().to_string())
}

fn riconosci(f: &Fonte) -> u8 {
    if f.ext == "xlsx" {
        let Some(r) = righe(f) else { return 0 };
        return if piastra(&r).is_some() { 95 } else { 40 };
    }
    let prima = f.testo.trim_start_matches('\u{feff}').lines().next().unwrap_or("");
    if !prima.starts_with("Plate_") {
        return 0;
    }
    // La seconda riga ha le colonne: campione, saggio, pozzetto, punto, segnale.
    let Some(testa) = f.testo.trim_start_matches('\u{feff}').lines().nth(1) else { return 0 };
    let t = testa.to_lowercase();
    if ["sample", "assay", "well", "spot", "signal"].iter().all(|k| t.contains(k)) {
        95
    } else {
        0
    }
}

fn ruolo(campione: &str) -> Option<Ruolo> {
    match campione.chars().next()?.to_ascii_lowercase() {
        'c' => Some(Ruolo::Controllo),
        's' => Some(Ruolo::Standard),
        'b' => Some(Ruolo::Bianco),
        'u' => Some(Ruolo::Campione),
        _ => None,
    }
}

// ---------------------------------------------------------------- i calcoli del programma

fn raggruppa<F: Fn(&Elemento) -> Option<String> + 'static>(chiave: F) -> Box<Ordina> {
    Box::new(move |el: &[Elemento], idx: &[usize]| {
        let mut out: Vec<(String, Vec<usize>)> = Vec::new();
        let mut posti: HashMap<String, usize> = HashMap::new();
        for &i in idx {
            if let Some(k) = chiave(&el[i]) {
                match posti.get(&k) {
                    Some(&p) => out[p].1.push(i),
                    None => {
                        posti.insert(k.clone(), out.len());
                        out.push((k, vec![i]));
                    }
                }
            }
        }
        out
    })
}

fn vista(nome: &'static str, chiave: &'static str, sub: Option<Rc<Vista>>) -> Rc<Vista> {
    Rc::new(Vista::nuova(nome, sub, raggruppa(move |e| e.testo(chiave))))
}

fn cfg(nome: &str, valore: &str, v: &Rc<cd::DatiVista>, sorg: Vec<Sorgente>) -> Rc<Config> {
    Rc::new(Config::nuova(nome, valore, v, sorg))
}

fn calcoli(elementi: &[Elemento]) -> Vec<cd::Doc> {
    let tutti: Vec<usize> = (0..elementi.len()).collect();
    let dv = |v: &Rc<Vista>| Rc::new(v.applica(elementi, &tutti));
    let saggio = |sub| vista("assay_identifier", "assay_identifier", sub);
    let campione = |sub| vista("sample_identifier", "sample_identifier", sub);
    let v_campione_saggio = dv(&campione(Some(saggio(None))));
    let v_saggio = dv(&saggio(None));
    let v_pozzetto = dv(&saggio(Some(vista("well_plate_identifier", "well_plate_identifier", Some(vista("location_identifier", "location_identifier", Some(campione(None))))))));
    let luminescenza = || cd::misura("luminescence", "luminescence", false);

    let media = cfg("Mean", "mean", &v_saggio, vec![luminescenza()]);
    let segnale_corretto = cfg("Adjusted Signal", "adjusted_signal", &v_campione_saggio, vec![luminescenza()]);
    let media_corretta = cfg("Adj. Sig. Mean", "adj_sig_mean", &v_saggio, vec![Sorgente::Calcolo(media.clone())]);
    let r2 = cfg("R-Squared", "fit_statistic_rsquared", &v_saggio, vec![luminescenza()]);
    let cv = cfg("CV", "cv", &v_saggio, vec![luminescenza()]);
    let recupero = cfg("% Recovery", "percent_recovery", &v_pozzetto, vec![luminescenza()]);
    let recupero_medio = cfg("% Recovery Mean", "percent_recovery_mean", &v_saggio, vec![Sorgente::Calcolo(recupero.clone())]);
    let concentrazione = cfg("Calc. Concentration", "calc_concentration", &v_pozzetto, vec![luminescenza()]);
    let concentrazione_media = cfg("Calc. Concentration Mean", "calc_conc_mean", &v_saggio, vec![Sorgente::Calcolo(concentrazione.clone())]);
    let concentrazione_cv = cfg("Calc. Conc. CV", "calc_conc_cv", &v_saggio, vec![Sorgente::Calcolo(concentrazione.clone())]);
    cd::costruisci(elementi, &[media, segnale_corretto, media_corretta, r2, cv, recupero, recupero_medio, concentrazione, concentrazione_media, concentrazione_cv])
}

// ---------------------------------------------------------------- la lettura

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let r = righe(f).ok_or("manca il foglio «Workbench data»")?;
    let id_piastra = piastra(&r).ok_or("nella prima riga non trovo il codice della piastra: deve essere come «Plate_2BO40AW287»")?;
    let testa = r.get(1).ok_or("manca la riga con i nomi delle colonne")?;
    let t = Tabella::nuova(testa);
    for colonna in ["Sample", "Well", "Spot", "Signal"] {
        if !t.ha(colonna) {
            return Err(format!("nella tabella manca la colonna «{colonna}»"));
        }
    }
    t.leggi(CALCOLATE);

    // Una misura per riga; i gruppi sono i punti della piastra (un saggio ciascuno).
    struct Riga {
        misura: Misura,
        gruppo: (String, String),
    }
    let mut righe_lette: Vec<Riga> = Vec::new();
    let mut elementi: Vec<Elemento> = Vec::new();
    let (mut max_lettera, mut max_numero) = (0u8, 0u32);
    for riga in &r[2..] {
        if riga.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let campione = t.cella(riga, &["sample"]).ok_or("una riga non ha il campione")?;
        let pozzetto = t.cella(riga, &["well"]).ok_or_else(|| format!("il campione {campione} non ha il pozzetto"))?;
        let punto = t.cella(riga, &["spot"]).ok_or_else(|| format!("il pozzetto {pozzetto} non ha il punto"))?;
        let segnale_testo = t.cella(riga, &["signal"]).unwrap_or("");
        let segnale = numero(segnale_testo).ok_or_else(|| format!("il pozzetto {pozzetto} ha un segnale che non è un numero: «{segnale_testo}»"))?;
        let saggio = t.cella(riga, &["assay"]);
        let intervallo = t.cella(riga, &["detection range"]);
        let diluizione = t.num(riga, &["dilution factor"]).or_else(|| t.num(riga, &["dilution"]));
        let concentrazione = t.num(riga, &["concentration"]);
        let id_campione = format!("{campione}_{pozzetto}");

        let mut m = Misura { id: l.id(), campione: id_campione.clone(), pozzetto: Some(pozzetto.to_string()), luogo: Some(punto.to_string()), piastra: Some(id_piastra.clone()), ..Default::default() };
        if let Some(rl) = ruolo(campione) {
            m.ruolo = rl;
            m.ruolo_dal_file = true;
        }
        m.valori.push(Valore::testo("detection type", "luminescence", Posto::Impostazione));
        if let Some(c) = concentrazione {
            m.valori.push(Valore::num("mass concentration", c, "pg/mL", Posto::Campione));
        }
        if let Some(d) = diluizione {
            m.valori.push(Valore::num("dilution factor setting", d, "", Posto::CampioneAltro));
        }
        m.valori.push(Valore::num("luminescence", segnale, "RLU", Posto::Misura));
        if let Some(x) = intervallo {
            m.valori.push(Valore::testo("detection range", x, Posto::Altro));
        }
        if let Some(x) = saggio {
            m.valori.push(Valore::testo("assay identifier", x, Posto::Altro));
        }
        for (k, v) in t.avanzi(riga) {
            m.valori.push(match numero(&v) {
                Some(x) => Valore::num(&k, x, "", Posto::Altro),
                None => Valore::testo(&k, &v, Posto::Altro),
            });
        }

        // Il pozzetto più grande dice la piastra: lettera per numero, come fa lo strumento.
        let (lettera, numero_pozzetto) = {
            let le: String = pozzetto.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            let n: u32 = pozzetto[le.len()..].parse().unwrap_or(0);
            (le.bytes().next().unwrap_or(b'A').to_ascii_uppercase(), n)
        };
        max_lettera = max_lettera.max(lettera);
        max_numero = max_numero.max(numero_pozzetto);

        // Gli elementi dei calcoli: la misura e quello che il programma ci ha calcolato sopra.
        let mut e = Elemento::nuovo(&m.id);
        e.metti_testo("sample_identifier", Some(&id_campione));
        e.metti_testo("location_identifier", Some(punto));
        e.metti_testo("well_location_identifier", Some(pozzetto));
        e.metti_testo("well_plate_identifier", Some(&id_piastra));
        e.metti_testo("assay_identifier", saggio);
        e.metti_num("luminescence", Some(segnale));
        for (colonna, chiave) in [
            ("Adjusted Signal", "adjusted_signal"),
            ("Mean", "mean"),
            ("Adj. Sig. Mean", "adj_sig_mean"),
            ("Fit Statistic: RSquared", "fit_statistic_rsquared"),
            ("CV", "cv"),
            ("% Recovery", "percent_recovery"),
            ("% Recovery Mean", "percent_recovery_mean"),
            ("Calc. Concentration", "calc_concentration"),
            ("Calc. Conc. Mean", "calc_conc_mean"),
            ("Calc. Conc. CV", "calc_conc_cv"),
        ] {
            e.metti_num(chiave, t.num(riga, &[colonna]));
        }
        elementi.push(e);
        righe_lette.push(Riga { misura: m, gruppo: (saggio.unwrap_or("").to_string(), punto.to_string()) });
    }
    if righe_lette.is_empty() {
        return Err("nel file non ci sono righe con un segnale".into());
    }

    let n_pozzetti = super::gen5::arrotonda_pozzetti((max_lettera.saturating_sub(b'A') as usize + 1) * max_numero as usize);
    // I gruppi, nell'ordine in cui compaiono i punti: il titolo è il saggio, e il punto se il saggio si ripete.
    let mut chiavi: Vec<(String, String)> = Vec::new();
    for x in &righe_lette {
        if !chiavi.contains(&x.gruppo) {
            chiavi.push(x.gruppo.clone());
        }
    }
    let titolo = |c: &(String, String)| {
        let ripetuto = chiavi.iter().filter(|k| k.0 == c.0).count() > 1;
        match (c.0.is_empty(), ripetuto) {
            (true, _) => format!("spot {}", c.1),
            (false, true) => format!("{} · spot {}", c.0, c.1),
            (false, false) => c.0.clone(),
        }
    };
    let mut gruppi: Vec<Gruppo> = chiavi.iter().map(|c| Gruppo { titolo: titolo(c), piastra: Some(id_piastra.clone()), pozzetti: Some(n_pozzetti), ..Default::default() }).collect();
    let mut posizione: Vec<(usize, usize)> = Vec::with_capacity(righe_lette.len());
    for x in righe_lette {
        let gi = chiavi.iter().position(|c| *c == x.gruppo).unwrap_or(0);
        posizione.push((gi, gruppi[gi].misure.len()));
        gruppi[gi].misure.push(x.misura);
    }

    // Quello che il programma ha calcolato: ogni valore con le misure da cui viene.
    let base = l.calcolati.len();
    for d in calcoli(&elementi) {
        let mut sorgenti: Vec<String> = Vec::new();
        let mut da = String::new();
        let mut altri: Vec<usize> = Vec::new();
        for fonte in &d.fonti {
            match fonte {
                cd::Fonte::Misura { elemento, feature } => {
                    let (g, i) = posizione[*elemento];
                    sorgenti.push(gruppi[g].misure[i].id.clone());
                    if da.is_empty() {
                        da = feature.clone();
                    }
                }
                cd::Fonte::Calcolo { posizione: p, .. } => altri.push(base + *p),
            }
        }
        l.calcolati.push(Calcolato { nome: d.nome.clone(), valore: d.valore, unita: String::new(), sorgenti, da, descrizione: String::new(), da_calcoli: altri });
    }

    l.strumento.produttore = "Meso Scale Diagnostics".into();
    l.strumento.software = "MSD Workbench".into();
    l.principale = "luminescence".into();
    l.unita = "RLU".into();
    l.info.push(("Piastra".into(), id_piastra));
    l.gruppi.extend(gruppi);
    Ok(())
}
