//! Thermo Fisher Genesys 30: la scansione di uno spettro (CSV o TSV): poche righe di intestazione
//! («Scan», «Mode», «Lower», «Upper», «Length») e poi la tabella lunghezza d'onda, assorbanza, %T.

use crate::strumenti::fonte::{coppie, numero, Fonte};
use crate::strumenti::{tempo, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_fisher_genesys30",
    nome: "Thermo Fisher Genesys 30",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv", "tsv"],
    riconosci,
    leggi,
};

fn separatore(f: &Fonte) -> char {
    if f.ext == "tsv" {
        '\t'
    } else {
        ','
    }
}

/// La riga con i nomi delle colonne: comincia con «wavelength(nm)».
fn inizio_tabella(righe: &[Vec<String>]) -> Option<usize> {
    righe.iter().position(|r| r.first().is_some_and(|c| c.trim().to_lowercase().starts_with("wavelength")))
}

fn riconosci(f: &Fonte) -> u8 {
    let righe = f.righe_con(separatore(f));
    let Some(i) = inizio_tabella(&righe) else { return 0 };
    let intest = coppie(&righe[..i]);
    let ha = |k: &str| intest.iter().any(|(a, _)| a == k);
    if ha("Scan") && ha("Mode") && ha("Lower") && ha("Upper") {
        92
    } else {
        0
    }
}

/// «Scan_20230914_160142.csv» → il tipo («Scan») e la data («2023-09-14T16:01:42+00:00»).
fn dal_nome(n: &str) -> (Option<String>, Option<String>) {
    let p: Vec<&str> = n.splitn(3, '_').collect();
    if p.len() < 3 {
        return (None, None);
    }
    let (data, ora) = (p[1], p[2].split('.').next().unwrap_or(""));
    let cifre = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let quando = (cifre(data, 8) && cifre(ora, 6)).then(|| format!("{}-{}-{}T{}:{}:{}+00:00", &data[..4], &data[4..6], &data[6..], &ora[..2], &ora[2..4], &ora[4..]));
    (Some(p[0].to_string()), quando)
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = f.righe_con(separatore(f));
    let i = inizio_tabella(&righe).ok_or("manca la tabella con «wavelength(nm)»")?;
    let intest = coppie(&righe[..i]);
    let chiave = |k: &str| intest.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let testa: Vec<String> = righe[i].iter().map(|c| c.trim().to_lowercase()).collect();
    let abs = testa.iter().position(|c| c == "abs").ok_or("manca la colonna ABS")?;
    let spettro: Vec<(f64, f64)> = righe[i + 1..].iter().filter_map(|r| Some((numero(r.first()?)?, numero(r.get(abs)?)?))).collect();
    if spettro.is_empty() {
        return Err("nello spettro non ci sono punti".into());
    }
    let (tipo, quando) = chiave("Scan").map(|n| dal_nome(&n)).unwrap_or((None, None));
    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.modello = "Genesys 30".into();
    l.strumento.software = "Genesys 30 instrument software".into();
    l.info.push(("Marchio".into(), "Genesys".into()));
    l.principale = "absorbance".into();
    l.unita = "AU".into();
    let mut m = Misura { quando, spettro, ..Default::default() };
    if let Some(mode) = chiave("Mode") {
        m.valori.push(Valore::testo("detection type", &mode, Posto::Impostazione));
    }
    for (k, nome) in [("Lower", "operating minimum"), ("Upper", "operating maximum")] {
        if let Some(v) = chiave(k).and_then(|v| numero(&v)) {
            m.valori.push(Valore::num(nome, v, "nm", Posto::ImpostazioneAltro));
        }
    }
    if let Some(v) = chiave("Length").and_then(|v| numero(&v)) {
        m.valori.push(Valore::num("number of sampled points", v, "", Posto::ImpostazioneAltro));
    }
    for (k, v) in &intest {
        if !matches!(k.as_str(), "Scan" | "Mode" | "Lower" | "Upper" | "Length") && !v.is_empty() {
            m.valori.push(Valore::testo(k, v, Posto::ImpostazioneAltro));
        }
    }
    let mut g = Gruppo { titolo: tipo.clone().unwrap_or_else(|| "Genesys 30".into()), ..Default::default() };
    if let Some(t) = tipo {
        g.dati.push(("experiment type".into(), t));
    }
    g.dati.push(("container type".into(), "tube".into()));
    g.misure.push(m);
    l.gruppi.push(g);
    Ok(())
}
