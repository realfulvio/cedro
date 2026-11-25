//! BMG Labtech SMART Control (CLARIOstar Plus e simili): l'Excel di una lettura di fluorescenza a punto
//! finale. Tre fogli: «Protocol Information» (utente, test, ID e impostazioni di lettura), «Microplate
//! End point» (la piastra a griglia, con la media dei bianchi usata) e «Table End point» (una colonna
//! per pozzetto: contenuto, concentrazione standard, diluizione, dato grezzo e dato corretto col bianco).

use super::serie::Serie;
use crate::strumenti::fonte::{numero, Fonte, Foglio};
use crate::strumenti::{tempo, Calcolato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "bmg_labtech_smart_control",
    nome: "BMG Labtech SMART Control",
    produttore: "BMG Labtech",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["xlsx"],
    riconosci,
    leggi,
};

fn foglio<'a>(f: &'a Fonte, nome: &str) -> Option<&'a Foglio> {
    f.fogli.iter().find(|g| g.nome.trim().to_lowercase() == nome)
}

fn riconosci(f: &Fonte) -> u8 {
    if foglio(f, "protocol information").is_some() && foglio(f, "microplate end point").is_some() {
        if foglio(f, "table end point").is_some() {
            95
        } else {
            60
        }
    } else {
        0
    }
}

/// Spazi ripetuti e ai lati tolti: «Focal height  [mm]» e «Focal height [mm]» sono lo stesso nome.
fn normale(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// «480-14»: la lunghezza d'onda e la larghezza di banda.
fn filtro(t: &str, nome: &str) -> Result<(f64, Option<f64>), String> {
    let errore = || format!("il filtro «{nome}» («{t}») non si legge: serve «lunghezza-banda», come 480-14");
    let (a, b) = match t.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (t, None),
    };
    let onda = numero(a).ok_or_else(errore)?;
    let banda = match b {
        Some(b) => Some(numero(b).ok_or_else(errore)?),
        None => None,
    };
    Ok((onda, banda))
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let protocollo = foglio(f, "protocol information").ok_or("manca il foglio «Protocol Information»")?;
    let piastra_fg = foglio(f, "microplate end point").ok_or("manca il foglio «Microplate End point»")?;
    let tabella = foglio(f, "table end point").ok_or("manca il foglio «Table End point»")?;

    // L'intestazione: le righe «Chiave: valore» fino a «Basic settings», poi le impostazioni (chiave e valore).
    let divisore = protocollo.righe.iter().position(|r| r.first().is_some_and(|c| c.trim().starts_with("Basic settings"))).ok_or("nel foglio «Protocol Information» manca «Basic settings»")?;
    let mut nomi: Vec<String> = Vec::new();
    let mut celle: Vec<Option<String>> = Vec::new();
    for r in &protocollo.righe[..divisore] {
        let c = r.first().map(|c| c.trim()).unwrap_or("");
        if c.is_empty() {
            continue;
        }
        let (k, v) = match c.split_once(": ") {
            Some((k, v)) => (k, Some(v.trim().to_string()).filter(|v| !v.is_empty())),
            None => (c, None),
        };
        nomi.push(normale(k.trim_end_matches(':')));
        celle.push(v);
    }
    for r in &protocollo.righe[divisore..] {
        let k = r.first().map(|c| c.trim()).unwrap_or("");
        if k.is_empty() {
            continue;
        }
        nomi.push(normale(k.trim_end_matches(':')));
        celle.push(r.get(1).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()));
    }
    let h = Serie::nuova(&nomi, &celle);
    let serve = |k: &str| h.testo_pieno(&[k]).ok_or_else(|| format!("nell'intestazione del file manca «{k}»"));

    let tipo = serve("Measurement type")?;
    let modo = tipo.split_whitespace().next().unwrap_or("").to_lowercase();
    if modo != "fluorescence" {
        return Err(format!("il tipo di lettura «{tipo}» non è supportato: Cedro legge la fluorescenza di SMART Control"));
    }
    let eccitazione = serve("Excitation")?;
    let emissione = serve("Emission")?;
    let (onda_ex, banda_ex) = filtro(&eccitazione, "Excitation")?;
    let (onda_em, banda_em) = filtro(&emissione, "Emission")?;
    let nome_piastra = serve("Microplate name")?;
    let pozzetti: u32 = nome_piastra.split_whitespace().last().and_then(|n| n.parse().ok()).ok_or_else(|| format!("dal nome della piastra «{nome_piastra}» non ricavo il numero di pozzetti"))?;
    let data = serve("Date")?;
    let ora = serve("Time")?;
    let quando = tempo::iso(&format!("{data} {ora}"), false);
    let utente = h.testo_pieno(&["User"]);
    let id1 = h.testo_pieno(&["ID1"]);
    let id3 = h.testo_pieno(&["ID3"]);
    let nome_test = h.testo_pieno(&["Test Name"]);
    let test_id = h.testo_pieno(&["Test ID"]);
    let percorso = h.testo_pieno(&["Path"]);
    let scansione_alta = h.grezzo("Top optic used").is_some() || nomi.iter().any(|n| n == "Top optic used");
    let altezza = h.num(&["Focal height [mm]"]);
    let lampi = h.num(&["No. of flashes per well"]);
    let guadagno = h.testo_pieno(&["Gain obtained by"]);
    let temperatura = h.num(&["Target temperature [°C]"]);
    let tempo_assestamento = h.num(&["Settling time [s]"]);
    // Le informazioni del dispositivo che lo schema non ha: restano come note.
    let del_dispositivo: Vec<(&str, String)> = [
        ("reading direction setting", "Reading direction"),
        ("dichroic filter setting", "Dichroic filter"),
        ("optic preset name", "Presetname"),
        ("spoon type", "Spoon type"),
        ("well used for focus adjustment", "Well used for focus adjustment"),
        ("focal height obtained by", "Focal height obtained by"),
    ]
    .into_iter()
    .filter_map(|(chiave, nome)| h.testo_pieno(&[nome]).map(|v| (chiave, v)))
    .collect();
    let del_dispositivo_liberi: Vec<(String, crate::strumenti::Dato)> = ["Target concentration O2 [%]", "Target concentration CO2 [%]", "Injection needle holder type"]
        .into_iter()
        .filter_map(|k| h.testo_pieno(&[k]).map(|v| (k.to_string(), numero(&v).map_or_else(|| crate::strumenti::Dato::Testo(v.clone()), crate::strumenti::Dato::Numero))))
        .collect();

    // La tabella: una colonna per pozzetto. Le righe dalla «Well» in giù: nome del campo e valori.
    let ini = tabella.righe.iter().position(|r| r.first().is_some_and(|c| c.trim().starts_with("Well"))).ok_or("nel foglio «Table End point» manca la riga «Well»")?;
    let pozzetti_file: Vec<(usize, String)> = tabella.righe[ini].iter().enumerate().skip(1).map(|(i, c)| (i, c.trim().to_string())).filter(|(_, c)| !c.is_empty()).collect();
    let campi: Vec<(String, &Vec<String>)> = tabella.righe[ini + 1..].iter().filter_map(|r| r.first().map(|c| (normale(c), r))).filter(|(n, _)| !n.is_empty()).collect();
    let campo = |nome: &str, i: usize| -> Option<&str> { campi.iter().find(|(n, _)| n == nome).and_then(|(_, r)| r.get(i)).map(|s| s.trim()).filter(|s| !s.is_empty()) };
    let nome_grezzo = format!("Raw Data ({}/{})", eccitazione, emissione);
    let nome_corretto = campi.iter().map(|(n, _)| n.as_str()).find(|n| n.starts_with("Blank corrected")).map(str::to_string);
    if !campi.iter().any(|(n, _)| *n == nome_grezzo) {
        return Err(format!("nel foglio «Table End point» manca la riga «{nome_grezzo}»"));
    }

    let mut g = Gruppo { titolo: id3.clone().unwrap_or_else(|| "Piastra".into()), quando: quando.clone(), piastra: id3.clone(), pozzetti: Some(pozzetti), ..Default::default() };
    if let Some(u) = &utente {
        g.dati.push(("analyst".into(), u.clone()));
    }
    if let Some(x) = &id1 {
        g.dati.push(("experimental data identifier".into(), x.clone()));
    }
    if let Some(x) = &nome_test {
        g.dati.push(("experiment type".into(), x.clone()));
    }
    if let Some(x) = &test_id {
        g.dati.push(("~Test ID".into(), x.clone()));
    }
    let mut corretti: Vec<(String, f64)> = Vec::new();
    for (i, w) in &pozzetti_file {
        let contenuto = campo("Content", *i).ok_or_else(|| format!("nel file manca il contenuto del pozzetto {w}"))?;
        let Some(grezzo) = campo(&nome_grezzo, *i) else { continue };
        let valore = numero(grezzo).ok_or_else(|| format!("il pozzetto {w} ha un valore che non è un numero: «{grezzo}»"))?;
        let mut m = Misura { id: l.id(), campione: contenuto.to_string(), pozzetto: Some(w.clone()), piastra: id3.clone(), ..Default::default() };
        match contenuto.split_whitespace().next() {
            Some("Blank") => m.ruolo = Ruolo::Bianco,
            Some("Standard") => m.ruolo = Ruolo::Standard,
            Some("Sample") => m.ruolo = Ruolo::Campione,
            _ => {}
        }
        m.ruolo_dal_file = matches!(contenuto.split_whitespace().next(), Some("Blank" | "Standard" | "Sample"));
        m.valori.push(Valore::testo("detection type", "fluorescence", Posto::Impostazione));
        if let Some(t) = temperatura {
            m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Impostazione));
        }
        if let Some(x) = altezza {
            m.valori.push(Valore::num("detector distance setting (plate reader)", x, "mm", Posto::Impostazione));
        }
        if let Some(x) = lampi {
            m.valori.push(Valore::num("number of averages", x, "#", Posto::Impostazione));
        }
        if let Some(x) = &guadagno {
            m.valori.push(Valore::testo("detector gain setting", x, Posto::Impostazione));
        }
        // «Top optic used» è scritto solo quando la lettura è dall'alto.
        m.valori.push(Valore::testo("scan position setting (plate reader)", if scansione_alta { "top scan position (plate reader)" } else { "bottom scan position (plate reader)" }, Posto::Impostazione));
        m.valori.push(Valore::num("detector wavelength setting", onda_em, "nm", Posto::Impostazione));
        if let Some(b) = banda_em {
            m.valori.push(Valore::num("detector bandwidth setting", b, "nm", Posto::Impostazione));
        }
        if let Some(b) = banda_ex {
            m.valori.push(Valore::num("excitation bandwidth setting", b, "nm", Posto::Impostazione));
        }
        m.valori.push(Valore::num("excitation wavelength setting", onda_ex, "nm", Posto::Impostazione));
        if let Some(t) = tempo_assestamento {
            m.valori.push(Valore::num("settling time setting", t, "s", Posto::ImpostazioneAltro));
        }
        for (k, v) in &del_dispositivo {
            m.valori.push(Valore::testo(k, v, Posto::ImpostazioneAltro));
        }
        for (k, d) in &del_dispositivo_liberi {
            m.valori.push(Valore { chiave: k.clone(), dato: d.clone(), unita: String::new(), posto: Posto::ImpostazioneAltro });
        }
        // Il campione: la concentrazione dello standard (ng/mL nel file, pg/mL nello schema) e la diluizione.
        if let Some(c) = campo("Standard Concentrations [ng/mL]", *i).and_then(numero) {
            m.valori.push(Valore::num("mass concentration", c * 1000.0, "pg/mL", Posto::Campione));
        }
        if let Some(d) = campo("Dilutions", *i).and_then(numero) {
            m.valori.push(Valore::num("dilution factor setting", d, "(unitless)", Posto::CampioneAltro));
        }
        m.valori.push(Valore::testo("container identifier", &nome_piastra, Posto::CampioneAltro));
        m.valori.push(Valore::num("fluorescence", valore, "RFU", Posto::Misura));
        // Il resto dell'intestazione che nessuno ha usato diventa nota della misura.
        for (k, d) in h.non_letti(&|k| matches!(k, "ID2" | "Path" | "Basic settings" | "Endpoint settings" | "Optic settings" | "General settings")) {
            m.valori.push(Valore { chiave: k, dato: d, unita: String::new(), posto: Posto::Altro });
        }
        if let Some(c) = nome_corretto.as_deref().and_then(|n| campo(n, *i)).and_then(numero) {
            corretti.push((m.id.clone(), c));
        }
        g.misure.push(m);
    }
    if g.misure.is_empty() {
        return Err("nel file non ci sono pozzetti con un valore".into());
    }

    // I calcoli dello strumento: la media dei bianchi usata (foglio «Microplate End point») e il dato corretto per ogni pozzetto.
    let media_bianchi = piastra_fg.righe.iter().flat_map(|r| r.iter()).filter_map(|c| {
        let c = c.trim();
        let resto = c.strip_prefix("Blank")?;
        (resto.starts_with(char::is_whitespace)).then(|| resto.split_whitespace().last().and_then(numero)).flatten()
    }).next().filter(|v| *v != 0.0);
    let bianchi: Vec<String> = g.misure.iter().filter(|m| m.ruolo == Ruolo::Bianco).map(|m| m.id.clone()).collect();
    let mut da_media: Vec<usize> = Vec::new();
    if let (Some(media), false) = (media_bianchi, bianchi.is_empty()) {
        da_media.push(l.calcolati.len());
        l.calcolati.push(Calcolato { nome: "Average of all blanks used".into(), valore: media, unita: "RFU".into(), sorgenti: bianchi, da: "fluorescence".into(), descrizione: String::new(), da_calcoli: Vec::new() });
    }
    if let Some(nome) = &nome_corretto {
        for (id, v) in corretti {
            l.calcolati.push(Calcolato { nome: nome.clone(), valore: v, unita: "RFU".into(), sorgenti: vec![id], da: "fluorescence".into(), descrizione: String::new(), da_calcoli: da_media.clone() });
        }
    }

    l.strumento.produttore = "BMG LABTECH".into();
    l.strumento.software = "SMART Control".into();
    if let Some(p) = percorso {
        l.sistema.push(Valore::testo("Path", &p, Posto::Altro));
    }
    if let Some(x) = &nome_test {
        l.info.push(("Test".into(), x.clone()));
    }
    if let Some(x) = utente {
        l.info.push(("Utente".into(), x));
    }
    l.info.push(("Piastra".into(), nome_piastra));
    l.principale = "fluorescence".into();
    l.unita = "RFU".into();
    l.gruppi.push(g);
    Ok(())
}
