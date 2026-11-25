//! Tecan Magellan: l'Excel che esporta Magellan, una colonna per etichetta di misura (Label1,
//! Label2… o «Raw data»), la colonna «Well positions» e, sotto i dati, le righe di metadati che
//! cominciano con «Date of measurement».

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "tecan_magellan",
    nome: "Tecan Magellan",
    produttore: "Tecan",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["xlsx"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    if f.fogli.len() != 1 {
        return 0;
    }
    let righe = &f.fogli[0].righe;
    let pozzetti = righe.iter().take(50).any(|r| r.iter().any(|c| c.contains("Well positions")));
    let data = righe.iter().any(|r| r.first().is_some_and(|c| c.starts_with("Date of measurement")));
    match (pozzetti, data) {
        (true, true) => 95,
        (true, false) | (false, true) => 60,
        _ => 0,
    }
}

#[derive(Debug)]
struct Impostazioni {
    modo: String,
    lunghezza_onda: f64,
    lampi: Option<f64>,
    piastra: String,
    temperatura: Option<f64>,
    altro: Vec<(String, String)>,
}

/// Un pozzetto scritto «A1»: lettere e almeno una cifra.
fn e_pozzetto(t: &str) -> bool {
    let t = t.trim();
    let l = t.bytes().take_while(u8::is_ascii_uppercase).count();
    l > 0 && t.as_bytes().get(l).is_some_and(u8::is_ascii_digit)
}

fn temperatura(t: &str) -> Option<f64> {
    let t = t.trim().strip_suffix("°C")?.trim();
    t.parse().ok()
}

fn prefisso<'a>(righe: &'a [String], chiave: &str) -> Option<&'a str> {
    righe.iter().find_map(|l| l.strip_prefix(chiave).map(str::trim))
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = &f.fogli.first().ok_or("il file non ha fogli")?.righe;
    let taglio = righe.iter().position(|r| r.first().is_some_and(|c| c.starts_with("Date of measurement"))).ok_or("mancano le righe di metadati (Date of measurement)")?;
    let testa = &righe[0];
    let col_pozzetti = testa.iter().position(|c| c.trim() == "Well positions").ok_or("manca la colonna «Well positions»")?;
    let col_piastra = testa.iter().position(|c| c.trim() == "Plate");
    // Le temperature delle misure stanno sotto l'intestazione: la prima cella «23.4 °C» di ogni colonna.
    let mut temperature: Vec<(String, f64)> = Vec::new();
    for (c, nome) in testa.iter().enumerate() {
        if let Some(t) = righe[1..taglio].iter().filter_map(|r| r.get(c)).find_map(|v| temperatura(v)) {
            temperature.push((nome.trim().to_string(), t));
        }
    }
    let dati: Vec<&Vec<String>> = righe[1..taglio].iter().filter(|r| r.get(col_pozzetti).is_some_and(|p| e_pozzetto(p))).collect();
    if dati.is_empty() {
        return Err("nel file non ci sono pozzetti".into());
    }
    let piastra_unica = col_piastra.and_then(|c| {
        let mut v: Vec<&str> = dati.iter().filter_map(|r| r.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        v.sort_unstable();
        v.dedup();
        (v.len() == 1).then(|| v[0].to_string())
    });

    // I metadati.
    let meta: Vec<String> = righe[taglio..].iter().filter_map(|r| r.first()).map(|s| s.trim_end().to_string()).filter(|s| !s.trim().is_empty()).collect();
    let mut it = 0usize;
    let ora = meta.get(it).ok_or("manca la data della misura")?;
    it += 1;
    let (giorno, orario) = ora.strip_prefix("Date of measurement: ").and_then(|r| r.split_once("/Time of measurement: ")).ok_or("data della misura non leggibile")?;
    let quando = tempo::iso(&format!("{giorno} {orario}"), true);
    let mut metodo = None;
    while meta.get(it).is_some_and(|s| s.ends_with(".mth")) {
        metodo = Some(meta[it].clone());
        it += 1;
    }
    let mut dati_sperimentali = None;
    while meta.get(it).is_some_and(|s| s.ends_with(".wsp")) {
        dati_sperimentali = Some(meta[it].clone());
        it += 1;
    }
    // Le lunghezze d'onda («485nm») si saltano.
    let e_nm = |s: &str| {
        let t = s.trim();
        let cifre = t.bytes().take_while(u8::is_ascii_digit).count();
        cifre > 0 && t[cifre..].trim_start() == "nm"
    };
    while meta.get(it).is_some_and(|s| e_nm(s)) {
        it += 1;
    }
    let analista = meta.get(it).ok_or("manca l'analista")?.trim().to_string();
    let dispositivo = meta.get(it + 1).ok_or("manca il nome dello strumento")?.trim().to_string();
    let seriale = meta.get(it + 2).and_then(|s| s.split_once(": ")).map(|(_, s)| s.trim().to_string()).ok_or("manca il numero di serie")?;
    it += 3;
    let resto = &meta[it..];

    // Le impostazioni di ogni misura, nell'ordine delle etichette.
    let mut impostazioni: Vec<(String, Impostazioni)> = Vec::new();
    let e_temp = |s: &str| s.starts_with("Meas. temperature: ") && s.trim_end().ends_with("°C");
    if !resto.iter().any(|s| e_temp(s)) {
        let r: Vec<String> = resto.iter().map(|s| s.trim().to_string()).collect();
        if !r.iter().any(|s| s == "Absorbance") {
            return Err("modo di misura non supportato (solo assorbanza)".into());
        }
        let onda = prefisso(&r, "Measurement wavelength:").and_then(|s| s.split_whitespace().next()).and_then(numero).ok_or("manca la lunghezza d'onda")?;
        let etichetta = prefisso(&r, "Label:").ok_or("manca l'etichetta della misura")?.to_string();
        let descrizione = prefisso(&r, "Plate Description:").ok_or("manca la descrizione della piastra")?.to_string();
        let piastra = descrizione.strip_prefix('[').and_then(|s| s.split_once(']')).map(|(a, _)| a.to_string()).unwrap_or_else(|| descrizione.clone());
        let mut altro = vec![("Plate Description".to_string(), descrizione)];
        for k in ["Range:", "Action:"] {
            if let Some(v) = prefisso(&r, k).filter(|v| !v.is_empty()) {
                altro.push((k.trim_end_matches(':').to_string(), v.to_string()));
            }
        }
        let t = temperature.iter().find(|(n, _)| *n == etichetta).map(|(_, t)| *t);
        impostazioni.push((etichetta, Impostazioni { modo: "Absorbance".into(), lunghezza_onda: onda, lampi: None, piastra, temperatura: t, altro }));
    } else {
        let mut blocchi: Vec<Vec<&str>> = Vec::new();
        let mut i = 0;
        while i < resto.len() && !e_temp(&resto[i]) {
            let e_misura = |s: &str| s.strip_prefix("Measurement ").is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()) && r.chars().count() >= 2);
            if e_misura(&resto[i]) {
                i += 1;
            }
            let mut b = Vec::new();
            while i < resto.len() {
                let riga = resto[i].as_str();
                i += 1;
                b.push(riga);
                if riga.starts_with("Unit:") && riga.trim().len() > 5 {
                    break;
                }
            }
            blocchi.push(b);
        }
        let mut etichette: Vec<(String, f64)> = Vec::new();
        while i < resto.len() && e_temp(&resto[i]) {
            let r = resto[i].trim().strip_prefix("Meas. temperature: ").unwrap_or("");
            let (nome, t) = r.rsplit_once(": ").ok_or("temperatura non leggibile")?;
            etichette.push((nome.to_string(), temperatura(t).ok_or("temperatura non leggibile")?));
            i += 1;
        }
        if etichette.len() != blocchi.len() {
            return Err("il numero di temperature non è quello delle misure".into());
        }
        for (b, (nome, t)) in blocchi.iter().zip(etichette) {
            let coppie: Vec<(String, String)> = b.iter().filter_map(|r| r.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
            let prendi = |k: &str| coppie.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
            let modo = prendi("Measurement mode").ok_or("manca il modo di misura")?.to_string();
            if !modo.eq_ignore_ascii_case("absorbance") {
                return Err(format!("modo di misura non supportato: {modo} (solo assorbanza)"));
            }
            let onda = prendi("Measurement wavelength").and_then(|s| s.split_whitespace().next()).and_then(numero).ok_or("manca la lunghezza d'onda")?;
            let piastra = prendi("Plate definition file").ok_or("manca il file della piastra")?.split('.').next().unwrap_or("").to_string();
            let letti = ["Measurement mode", "Measurement wavelength", "Number of flashes", "Plate definition file", "Unit"];
            let altro = coppie.iter().filter(|(k, _)| !letti.contains(&k.as_str())).cloned().collect();
            impostazioni.push((nome, Impostazioni { modo, lunghezza_onda: onda, lampi: prendi("Number of flashes").and_then(numero), piastra, temperatura: Some(t), altro }));
        }
    }

    l.strumento.produttore = "Tecan".into();
    l.strumento.identificativo = dispositivo;
    l.strumento.seriale = seriale;
    l.strumento.software = "Magellan".into();
    l.principale = "absorbance".into();
    l.unita = "OD".into();
    l.info.push(("Analista".into(), analista.clone()));
    if let Some(m) = &metodo {
        l.info.push(("Metodo".into(), m.clone()));
    }
    if let Some(d) = &dati_sperimentali {
        l.info.push(("Area di lavoro".into(), d.clone()));
    }
    if let Some(q) = &quando {
        l.info.push(("Data della misura".into(), q.clone()));
    }

    for (etichetta, imp) in &impostazioni {
        let colonna = testa.iter().position(|c| c.trim() == etichetta).ok_or_else(|| format!("nel file manca la colonna «{etichetta}»"))?;
        let mut g = Gruppo { titolo: format!("{} nm", imp.lunghezza_onda), quando: quando.clone(), pozzetti: Some(dati.len() as u32), ..Default::default() };
        if impostazioni.iter().filter(|(_, i)| i.lunghezza_onda == imp.lunghezza_onda).count() > 1 {
            g.titolo = format!("{} · {}", etichetta, g.titolo);
        }
        g.dati.push(("analyst".into(), analista.clone()));
        if let Some(m) = &metodo {
            g.dati.push(("analytical method identifier".into(), m.clone()));
        }
        if let Some(d) = &dati_sperimentali {
            g.dati.push(("experimental data identifier".into(), d.clone()));
        }
        for r in &dati {
            let pozzetto = r[col_pozzetti].trim().to_string();
            let piastra = col_piastra
                .and_then(|c| r.get(c))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| piastra_unica.clone())
                .unwrap_or_else(|| imp.piastra.clone());
            let cella = r.get(colonna).map(|s| s.trim()).unwrap_or("");
            let mut m = Misura {
                campione: format!("{piastra}_{pozzetto}"),
                pozzetto: Some(pozzetto),
                piastra: Some(piastra),
                quando: None,
                ..Default::default()
            };
            m.valori.push(Valore { chiave: "detection type".into(), dato: Dato::Testo(imp.modo.clone()), unita: String::new(), posto: Posto::Impostazione });
            if let Some(t) = imp.temperatura {
                m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Impostazione));
            }
            m.valori.push(match numero(cella) {
                Some(v) => Valore::num("absorbance", v, "OD", Posto::Misura),
                None => Valore { chiave: "absorbance".into(), dato: Dato::Assente(cella.to_string()), unita: "OD".into(), posto: Posto::Misura },
            });
            if let Some(n) = imp.lampi {
                m.valori.push(Valore::num("number of averages", n, "#", Posto::Impostazione));
            }
            m.valori.push(Valore::num("detector wavelength setting", imp.lunghezza_onda, "nm", Posto::Impostazione));
            for (k, v) in &imp.altro {
                m.valori.push(Valore::testo(k, v, Posto::Altro));
            }
            g.misure.push(m);
        }
        g.piastra = g.misure.first().and_then(|m| m.piastra.clone());
        l.gruppi.push(g);
    }
    Ok(())
}
