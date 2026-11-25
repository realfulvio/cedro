//! Thermo Fisher NanoDrop 8000 e NanoDrop Eight: il testo a tabulazioni con una riga per campione.
//! L'8000 scrive le assorbanze (A260, A280, o le coppie «nm 1 / abs 1»), la concentrazione e i
//! rapporti; l'Eight aggiunge un'intestazione («Application:», «Serial number:») e lo spettro intero.
//! Le assorbanze sono una misura per lunghezza d'onda; la concentrazione sta sulla misura della
//! lunghezza d'onda con cui lo strumento l'ha calcolata (260 per il DNA e l'RNA, 280 per le proteine).

use super::tabella::{norma, Tabella};
use crate::strumenti::fonte::{coppie, numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const OTTO_MILA: Profilo = Profilo {
    id: "thermo_fisher_nanodrop_8000",
    nome: "Thermo Fisher NanoDrop 8000",
    produttore: "ThermoFisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["txt", "tsv", "csv"],
    riconosci: riconosci_8000,
    leggi: leggi_8000,
};

pub const EIGHT: Profilo = Profilo {
    id: "thermo_fisher_nanodrop_eight",
    nome: "Thermo Fisher NanoDrop Eight",
    produttore: "ThermoFisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["txt", "tsv"],
    riconosci: riconosci_eight,
    leggi: leggi_eight,
};

/// Le assorbanze di una riga: le coppie «nm i / abs i» e le colonne A230, A260, A280.
fn assorbanze(t: &Tabella, r: &[String]) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = Vec::new();
    let mut metti = |w: f64, a: f64| match v.iter_mut().find(|(x, _)| *x == w) {
        Some(p) => p.1 = a,
        None => v.push((w, a)),
    };
    let mut i = 1;
    while t.ha(&format!("nm {i}")) {
        if let (Some(w), Some(a)) = (t.num(r, &[&format!("nm {i}")]), t.num(r, &[&format!("abs {i}")])) {
            metti(w, a);
        }
        i += 1;
    }
    for (w, nomi) in [(230.0, &["a230"][..]), (260.0, &["a260"]), (280.0, &["a280", "a280 10mm"])] {
        if let Some(a) = t.num(r, nomi) {
            metti(w, a);
        }
    }
    v
}

/// Con quale lunghezza d'onda lo strumento ha calcolato la concentrazione: quella del cursore, se
/// la scrive; altrimenti 280 per le proteine (se c'è A280) e 260 per DNA e RNA.
fn lunghezza_cattura(t: &Tabella, r: &[String], tipo: Option<&str>, ass: &[(f64, f64)]) -> f64 {
    if let Some(w) = t.num(r, &["cursor nm"]) {
        return w;
    }
    let acido_nucleico = tipo.is_some_and(|x| x.contains("NA"));
    if !acido_nucleico && ass.iter().any(|(w, _)| *w == 280.0) {
        280.0
    } else {
        260.0
    }
}

/// I rapporti scritti dallo strumento (A260/280, A260/230) e le formule («Formula Name 1», «Formula
/// Value 1»), ognuno con le misure delle lunghezze d'onda da cui viene.
fn calcolati(t: &Tabella, r: &[String], misure: &[(f64, String)], l: &mut Lettura) {
    let sorgente = |w: f64| misure.iter().find(|(x, _)| *x == w).map(|(_, id)| id.clone());
    for (a, b) in [(260.0, 280.0), (260.0, 230.0)] {
        let (na, nb) = (a as u32, b as u32);
        let v = t.num(r, &[&format!("{na}/{nb}"), &format!("a{na}/a{nb}")]);
        if let Some(v) = v.filter(|v| *v != 0.0) {
            l.calcolati.push(Calcolato { nome: format!("A{na}/{nb}"), valore: v, unita: String::new(), sorgenti: [a, b].into_iter().filter_map(sorgente).collect(), da: "absorbance".into(), descrizione: String::new(), da_calcoli: Vec::new() });
        }
    }
    for i in 1..=t.testa.len() {
        let Some(v) = t.num(r, &[&format!("formula value {i}")]) else { continue };
        let nome = t.cella(r, &[&format!("formula name {i}")]).map(str::to_string).unwrap_or_else(|| format!("Formula {i}"));
        let formula = t.cella(r, &[&format!("formula {i}")]).unwrap_or_default().to_string();
        let sorgenti: Vec<String> = misure.iter().filter(|(w, _)| formula.contains(&format!("{w}"))).map(|(_, id)| id.clone()).collect();
        l.calcolati.push(Calcolato { nome, valore: v, unita: String::new(), sorgenti, da: "absorbance".into(), descrizione: String::new(), da_calcoli: Vec::new() });
    }
}

/// Un valore che lo strumento scrive e lo schema non ha: numero (con l'unità se è un'assorbanza) o testo.
fn nota(h: &str, c: &str) -> Valore {
    let assorbanza = {
        let b = h.to_lowercase();
        b.contains("abs") || b.ends_with(" raw")
    };
    match numero(c) {
        Some(v) => Valore { chiave: h.to_string(), dato: Dato::Numero(v), unita: if assorbanza { "AU".into() } else { String::new() }, posto: Posto::Altro },
        None => Valore::testo(h, c, Posto::Altro),
    }
}

/// Le misure di una riga: una per lunghezza d'onda. La concentrazione sta su quella di cattura.
struct Riga<'a> {
    campione: &'a str,
    piastra: Option<&'a str>,
    luogo: Option<&'a str>,
    quando: Option<String>,
    concentrazione: Option<(f64, String)>,
    /// Note sui dati elaborati: E1%, coefficiente di estinzione, fattore.
    elaborate: Vec<Valore>,
    /// Note sul campione.
    del_campione: Vec<Valore>,
    cursore: Option<f64>,
    cattura: f64,
}

fn misure_riga(riga: &Riga, ass: &[(f64, f64)], avanzi: &[(String, String)], l: &mut Lettura) -> Vec<Misura> {
    let mut out = Vec::new();
    for &(w, a) in ass {
        let mut m = Misura { id: l.id(), campione: riga.campione.into(), piastra: riga.piastra.map(str::to_string), luogo: riga.luogo.map(str::to_string), quando: riga.quando.clone(), ..Default::default() };
        m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
        if let Some(c) = riga.cursore {
            m.valori.push(Valore::num("Cursor Pos.", c, "nm", Posto::ImpostazioneAltro));
        }
        m.valori.extend(riga.del_campione.iter().cloned());
        m.valori.push(Valore::num("absorbance", a, "AU", Posto::Misura));
        if w == riga.cattura {
            if let Some((c, u)) = &riga.concentrazione {
                m.valori.push(Valore::num("mass concentration", *c, u, Posto::Elaborato));
                m.valori.extend(riga.elaborate.iter().cloned());
            }
        }
        for (h, c) in avanzi {
            m.valori.push(nota(h, c));
        }
        out.push(m);
    }
    out
}

/// Il gruppo di (analista, tipo di saggio): lo crea se non c'è.
fn gruppo<'a>(gruppi: &'a mut Vec<((String, String), Gruppo)>, analista: &str, tipo: &str, titolo: &str) -> &'a mut Gruppo {
    let chiave = (analista.to_string(), tipo.to_string());
    if let Some(i) = gruppi.iter().position(|(k, _)| *k == chiave) {
        return &mut gruppi[i].1;
    }
    let mut g = Gruppo { titolo: if tipo.is_empty() { titolo.to_string() } else { tipo.to_string() }, ..Default::default() };
    if !analista.is_empty() {
        g.dati.push(("analyst".into(), analista.into()));
    }
    if !tipo.is_empty() {
        g.dati.push(("experiment type".into(), tipo.into()));
    }
    gruppi.push((chiave, g));
    &mut gruppi.last_mut().unwrap().1
}

fn righe_piene(righe: &[Vec<String>]) -> Vec<&Vec<String>> {
    righe.iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect()
}

// ---------------------------------------------------------------- NanoDrop 8000

fn riconosci_8000(f: &Fonte) -> u8 {
    let testa = f.righe_con('\t');
    let Some(r) = testa.iter().take(5).find(|r| r.len() > 1 && r.iter().any(|c| norma(c) == "plate id")) else { return 0 };
    let t = Tabella::nuova(r);
    if t.ha("well") && t.ha("sample id") && t.ha("user id") && t.ha("date") {
        90
    } else {
        0
    }
}

fn leggi_8000(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe_con('\t');
    let i = righe.iter().position(|r| r.iter().any(|c| norma(c) == "plate id")).ok_or("manca la riga di intestazione con «Plate ID»")?;
    let t = Tabella::nuova(&righe[i]);
    let dati = righe_piene(&righe[i + 1..]);
    if dati.is_empty() {
        return Err("nel file non ci sono misure".into());
    }
    let date: Vec<String> = dati.iter().map(|r| format!("{} {}", t.cella(r, &["date"]).unwrap_or_default(), t.cella(r, &["time"]).unwrap_or_default())).collect();
    let gp = tempo::giorno_prima(date.iter().map(String::as_str));
    l.strumento.produttore = "ThermoFisher Scientific".into();
    l.strumento.modello = "NanoDrop 8000".into();
    l.strumento.identificativo = "NanoDrop".into();
    l.principale = "absorbance".into();
    l.unita = "AU".into();
    let mut gruppi: Vec<((String, String), Gruppo)> = Vec::new();
    for (r, data) in dati.iter().zip(&date) {
        let ass = assorbanze(&t, r);
        let tipo = t.cella(r, &["na type"]);
        let cattura = lunghezza_cattura(&t, r, tipo, &ass);
        let unita = t.cella(r, &["units"]);
        let conc = t.num(r, &["conc.", "conc", "concentration"]);
        let mut elaborate = Vec::new();
        for (v, nome, u) in [("e 1%", "E1%", ""), ("ext. coeff x10e3", "Ext. Coeff x10e3", ""), ("ext.c. (l/(mol*cm))", "Ext.C. (L/(mol*cm))", ""), ("conc. factor (ng/ul)", "Conc. Factor", "ng/ul")] {
            if let Some(x) = t.num(r, &[v]) {
                elaborate.push(Valore::num(nome, x, u, Posto::ElaboratoAltro));
            }
        }
        let mut del_campione = Vec::new();
        for (v, nome) in [("mol. wt. kda", "Mol. Wt. kDa"), ("m.w. (g/mol)", "M.W. (g/mol)")] {
            if let Some(x) = t.num(r, &[v]) {
                del_campione.push(Valore::num(nome, x, "", Posto::CampioneAltro));
            }
        }
        // Le colonne che servono solo ai calcoli sono già lette: non diventano note.
        t.leggi(&["date", "time", "cursor nm", "260/280", "260/230", "a260/a280", "a260/a230"]);
        let cursore = t.num(r, &["cursor pos."]);
        let analista = t.cella(r, &["user id"]).unwrap_or_default().to_string();
        let riga = Riga {
            campione: t.cella(r, &["sample id"]).unwrap_or_default(),
            piastra: t.cella(r, &["plate id"]),
            luogo: t.cella(r, &["well"]),
            quando: tempo::iso(data, gp),
            concentrazione: conc.zip(unita).map(|(c, u)| (c, u.to_string())),
            elaborate,
            del_campione,
            cursore,
            cattura,
        };
        for k in 1..=t.testa.len() {
            t.leggi(&[&format!("formula {k}"), &format!("formula name {k}"), &format!("formula value {k}")]);
        }
        let avanzi = t.avanzi(r);
        let tipo = tipo.unwrap_or_default().to_string();
        let misure = misure_riga(&riga, &ass, &avanzi, l);
        let ids: Vec<(f64, String)> = ass.iter().map(|(w, _)| *w).zip(misure.iter().map(|m| m.id.clone())).collect();
        calcolati(&t, r, &ids, l);
        gruppo(&mut gruppi, &analista, &tipo, "NanoDrop 8000").misure.extend(misure);
    }
    if gruppi.iter().all(|(_, g)| g.misure.is_empty()) {
        return Err("nel file non ci sono assorbanze (A260, A280…)".into());
    }
    l.gruppi.extend(gruppi.into_iter().map(|(_, g)| g));
    Ok(())
}

// ---------------------------------------------------------------- NanoDrop Eight

fn riconosci_eight(f: &Fonte) -> u8 {
    let righe = f.righe_con('\t');
    let intest = coppie(&righe.iter().take(5).filter(|r| r.iter().filter(|c| !c.trim().is_empty()).count() <= 2).cloned().collect::<Vec<_>>());
    let ha = |k: &str| intest.iter().any(|(a, _)| a.eq_ignore_ascii_case(k));
    let trovate = ["application", "serial number", "user name"].iter().filter(|k| ha(k)).count();
    if trovate >= 2 {
        85
    } else {
        0
    }
}

/// «ng/µL» come lo scrive l'intestazione, anche con i caratteri sbagliati dei vecchi file («ng/âµl»).
fn colonna_concentrazione(h: &str) -> Option<String> {
    let b = h.trim().to_lowercase();
    if b.len() > 14 || !b.ends_with('l') {
        return None;
    }
    let (num, _) = b.split_once('/')?;
    let numeratore = ["ng", "pg", "mg", "μg", "µg", "ug", "g"].iter().find(|x| num.ends_with(**x))?;
    let denominatore = if b.ends_with("ml") { "mL" } else if b.ends_with("ul") || b.ends_with("µl") || b.ends_with("μl") { "µL" } else { "L" };
    let numeratore = if *numeratore == "ug" { "µg" } else if *numeratore == "μg" { "µg" } else { numeratore };
    Some(format!("{numeratore}/{denominatore}"))
}

fn leggi_eight(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe_con('\t');
    // L'intestazione: righe «Chiave:  valore»; poi la riga con i nomi delle colonne.
    let mut i = 0;
    while i < righe.len() {
        let celle: Vec<&str> = righe[i].iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect();
        let e_intestazione = match celle.as_slice() {
            [k, _] => k.ends_with(':'),
            [una] => una.contains(':'),
            _ => false,
        };
        if !celle.is_empty() && !e_intestazione {
            break;
        }
        i += 1;
    }
    if i >= righe.len() {
        return Err("nel file non c'è la tabella delle misure".into());
    }
    let intest = coppie(&righe[..i]);
    let chiave = |k: &str| intest.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let t = Tabella::nuova(&righe[i]);
    let dati = righe_piene(&righe[i + 1..]);
    if dati.is_empty() {
        return Err("nel file non ci sono misure".into());
    }
    let tipo = chiave("application").unwrap_or_default();
    let utente = chiave("user name");
    l.strumento.produttore = "ThermoFisher Scientific".into();
    l.strumento.modello = "NanoDrop Eight".into();
    l.strumento.identificativo = "NanoDrop".into();
    l.strumento.seriale = chiave("serial number").unwrap_or_default();
    for (k, v) in &intest {
        if !matches!(k.to_lowercase().as_str(), "serial number" | "application" | "user name") && !v.is_empty() {
            l.info.push((k.clone(), v.clone()));
        }
    }
    l.principale = "absorbance".into();
    l.unita = "AU".into();
    let conc_col = t.testa.iter().enumerate().find_map(|(c, h)| colonna_concentrazione(h).map(|u| (c, u)));
    // Le colonne con un numero per nome sono lo spettro.
    let spettro_col: Vec<(usize, f64)> = t.testa.iter().enumerate().filter_map(|(c, h)| numero(h).filter(|_| h.chars().next().is_some_and(|x| x.is_ascii_digit())).map(|w| (c, w))).collect();
    let date: Vec<String> = dati.iter().map(|r| t.cella(r, &["date & time", "date"]).unwrap_or_default().replace(['(', ')'], "")).collect();
    let gp = tempo::giorno_prima(date.iter().map(String::as_str));
    t.leggi(&["date", "date & time", "a260/a230", "a260/a280", "sample", "username"]);
    let mut gruppi: Vec<((String, String), Gruppo)> = Vec::new();
    for (r, data) in dati.iter().zip(&date) {
        let ass = assorbanze(&t, r);
        let cattura = lunghezza_cattura(&t, r, Some(tipo.as_str()), &ass);
        let concentrazione = conc_col.as_ref().and_then(|(c, u)| r.get(*c).and_then(|x| numero(x)).map(|v| (v, u.clone())));
        if let Some((c, _)) = &conc_col {
            t.usate.borrow_mut().insert(*c);
        }
        let spettro: Vec<(f64, f64)> = spettro_col.iter().filter_map(|(c, w)| r.get(*c).and_then(|x| numero(x)).map(|a| (*w, a))).collect();
        for (c, _) in &spettro_col {
            t.usate.borrow_mut().insert(*c);
        }
        let analista = utente.clone().or_else(|| t.cella(r, &["username"]).map(str::to_string)).unwrap_or_default();
        let nome = t.cella(r, &["sample name", "sample"]);
        let campione = t.cella(r, &["sample id", "uid"]).unwrap_or_default();
        let luogo = t.cella(r, &["location"]);
        let mut del_campione = Vec::new();
        if let Some(n) = nome {
            del_campione.push(Valore::testo("Sample Name", n, Posto::CampioneAltro));
        }
        let riga = Riga { campione, piastra: None, luogo, quando: tempo::iso(data, gp), concentrazione, elaborate: Vec::new(), del_campione, cursore: None, cattura };
        let avanzi = t.avanzi(r);
        let mut misure = misure_riga(&riga, &ass, &avanzi, l);
        let ids: Vec<(f64, String)> = ass.iter().map(|(w, _)| *w).zip(misure.iter().map(|m| m.id.clone())).collect();
        calcolati(&t, r, &ids, l);
        if !spettro.is_empty() {
            misure.push(Misura { id: l.id(), campione: campione.into(), luogo: luogo.map(str::to_string), quando: riga.quando.clone(), spettro, ..Default::default() });
        }
        gruppo(&mut gruppi, &analista, &tipo, "NanoDrop Eight").misure.extend(misure);
    }
    if gruppi.iter().all(|(_, g)| g.misure.is_empty()) {
        return Err("nel file non ci sono assorbanze (A260, A280…)".into());
    }
    l.gruppi.extend(gruppi.into_iter().map(|(_, g)| g));
    Ok(())
}
