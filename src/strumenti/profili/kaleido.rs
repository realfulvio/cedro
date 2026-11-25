//! Revvity Kaleido (EnSight): il CSV che il programma esporta dopo una lettura di una piastra:
//! assorbanza, fluorescenza, luminescenza o immagini. Due modi di scriverlo, secondo la versione di
//! Kaleido (l'ultima riga dice «Exported with Kaleido 2.0…» o «3.0…»/«3.5…»): la piastra dei
//! risultati, le informazioni della misura, la mappa della piastra e le impostazioni della lettura.
//! Per le immagini la piastra dei risultati dà il file dell'immagine di ogni pozzetto, e le
//! piastre dell'analisi (versione 2) i valori calcolati sulle immagini.

use std::collections::HashSet;

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "revvity_kaleido",
    nome: "Revvity Kaleido",
    produttore: "Revvity",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["csv"],
    riconosci,
    leggi,
};

fn ultima_riga(f: &Fonte) -> Option<&str> {
    f.testo.trim_end().rsplit('\n').next().map(|r| r.trim())
}

fn riconosci(f: &Fonte) -> u8 {
    if ultima_riga(f).is_some_and(|r| r.contains("Exported with Kaleido")) {
        95
    } else {
        0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tipo {
    Assorbanza,
    Fluorescenza,
    Luminescenza,
    Immagini,
}

impl Tipo {
    fn da_testo(t: &str) -> Option<Tipo> {
        let t = t.to_lowercase();
        if t.contains("fluorescence") || t.contains("alpha") {
            Some(Tipo::Fluorescenza)
        } else if t.contains("abs") {
            Some(Tipo::Assorbanza)
        } else if t.contains("luminescence") {
            Some(Tipo::Luminescenza)
        } else if t.contains("img") {
            Some(Tipo::Immagini)
        } else {
            None
        }
    }

    fn concetto(self) -> &'static str {
        match self {
            Tipo::Assorbanza => "absorbance",
            Tipo::Fluorescenza => "fluorescence",
            Tipo::Luminescenza => "luminescence",
            Tipo::Immagini => "image file",
        }
    }

    fn unita(self) -> &'static str {
        match self {
            Tipo::Assorbanza => "OD",
            Tipo::Fluorescenza => "RFU",
            Tipo::Luminescenza => "RLU",
            Tipo::Immagini => "",
        }
    }

    fn rilevazione(self) -> &'static str {
        match self {
            Tipo::Assorbanza => "Absorbance",
            Tipo::Fluorescenza => "Fluorescence",
            Tipo::Luminescenza => "Luminescence",
            Tipo::Immagini => "optical-imaging",
        }
    }
}

fn celle(riga: &str) -> Vec<&str> {
    riga.split(',').map(str::trim).collect()
}

fn e_lettera(c: &str) -> bool {
    !c.is_empty() && c.len() <= 2 && c.chars().all(|c| c.is_ascii_uppercase())
}

/// Una piastra a griglia: la riga d'intestazione (vuoto, 1, 2, 3…) e le righe A, B, C… con le loro celle.
struct Griglia {
    colonne: usize,
    righe: Vec<(String, Vec<String>)>,
    /// La riga dopo l'ultima della griglia.
    fine: usize,
}

/// La prima griglia che comincia da `da` in poi.
fn griglia(righe: &[&str], da: usize) -> Option<Griglia> {
    let i = (da..righe.len()).find(|&i| {
        let c = celle(righe[i]);
        c.first().is_some_and(|x| x.is_empty()) && numero(c.get(1).copied().unwrap_or("")) == Some(1.0) && numero(c.get(2).copied().unwrap_or("")) == Some(2.0)
    })?;
    let c = celle(righe[i]);
    let mut n = 0;
    while c.get(n + 1).and_then(|x| numero(x)) == Some((n + 1) as f64) {
        n += 1;
    }
    let mut out = Vec::new();
    let mut j = i + 1;
    while j < righe.len() {
        let r = celle(righe[j]);
        if !r.first().is_some_and(|x| e_lettera(x)) {
            break;
        }
        out.push((r[0].to_string(), (1..=n).map(|k| r.get(k).copied().unwrap_or("").to_string()).collect()));
        j += 1;
    }
    Some(Griglia { colonne: n, righe: out, fine: j })
}

/// Le coppie «chiave, valore» di un tratto di righe: la chiave in minuscolo e senza i due punti, il
/// valore è la prima cella piena dopo la chiave.
fn coppie(righe: &[&str]) -> Vec<(String, String)> {
    righe
        .iter()
        .filter_map(|r| {
            let c = celle(r);
            let k = c.first()?.trim_end_matches(':').trim().to_lowercase();
            let v = c.iter().skip(1).find(|x| !x.is_empty())?;
            (!k.is_empty()).then(|| (k, v.to_string()))
        })
        .collect()
}

fn primo<'a>(c: &'a [(String, String)], k: &str) -> Option<&'a str> {
    c.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
}

fn inizia(righe: &[&str], da: usize, prefisso: &str) -> Option<usize> {
    (da..righe.len()).find(|&i| righe[i].starts_with(prefisso))
}

/// Il ruolo dalla mappa della piastra: «S3», «U12», «B1»; «-» è un pozzetto senza campione.
fn ruolo(v: &str) -> Option<Ruolo> {
    let lettere: String = v.chars().take_while(|c| c.is_ascii_uppercase()).collect();
    let resto = &v[lettere.len()..];
    if resto.is_empty() || !resto.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    match lettere.as_str() {
        "B" => Some(Ruolo::Bianco),
        "S" => Some(Ruolo::Standard),
        "U" => Some(Ruolo::Campione),
        "C" | "E" | "ZL" | "ZH" | "LB" | "LC" | "LH" => Some(Ruolo::Controllo),
        _ => None,
    }
}

/// «465nm» o «735nm» (con altro testo in un'altra cella): il numero.
fn lunghezza(v: &str) -> Option<f64> {
    numero(v.trim().trim_end_matches("nm").trim())
}

fn dato(v: &str) -> Dato {
    match v.to_lowercase().as_str() {
        "true" => Dato::Bool(true),
        "false" => Dato::Bool(false),
        _ => numero(v).map_or_else(|| Dato::Testo(v.to_string()), Dato::Numero),
    }
}

struct Canale {
    nome: String,
    onda: Option<f64>,
    potenza: Option<f64>,
    esposizione: Option<f64>,
    fuoco: Option<f64>,
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    // Kaleido scrive spazi che non spezzano la riga («Results for\u{a0}ABS mono 1»): si leggono come spazi.
    let testo = f.testo.trim_start_matches('\u{feff}').replace('\u{a0}', " ");
    let righe: Vec<&str> = testo.lines().map(|r| r.trim_end_matches('\r')).collect();
    let ultima = righe.iter().rev().find(|r| !r.trim().is_empty()).copied().unwrap_or("");
    let versione: String = ultima.split("Exported with Kaleido").nth(1).map(|r| r.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect()).unwrap_or_default();
    let v2 = versione.starts_with("2.");
    let v3 = versione.starts_with("3.");
    if !v2 && !v3 {
        return Err(format!("la versione di Kaleido «{versione}» non è fra quelle che Cedro legge: la 2 e la 3"));
    }

    // Il tipo di misura: «Results for ABS mono 1», «Result for Luminescence 1».
    let i_risultati = (0..righe.len()).find(|&i| righe[i].starts_with("Results for ") || righe[i].starts_with("Result for ")).ok_or("non trovo la riga «Results for…» con il tipo di misura")?;
    let tipo_testo = {
        let t = righe[i_risultati].split_once(" for ").map(|(_, r)| r).unwrap_or("").split(',').next().unwrap_or("").trim();
        match t.rsplit_once(' ') {
            Some((a, b)) if b.chars().all(|c| c.is_ascii_digit()) => a.trim().to_string(),
            _ => t.to_string(),
        }
    };
    let tipo = Tipo::da_testo(&tipo_testo).ok_or_else(|| format!("il tipo di misura «{tipo_testo}» non si riconosce: cerco fluorescenza, assorbanza, luminescenza o immagini"))?;

    // La piastra dei risultati e il codice a barre.
    let (barcode, da_griglia) = if v2 {
        let i = inizia(&righe, i_risultati, "Barcode:").ok_or("non trovo il codice a barre della piastra")?;
        (righe[i]["Barcode:".len()..].split(',').next().unwrap_or("").trim().to_string(), i + 1)
    } else {
        let i = inizia(&righe, i_risultati, "Barcode").ok_or("non trovo il codice a barre della piastra")?;
        let j = (i + 1..righe.len()).find(|&j| !righe[j].trim().is_empty()).ok_or("non trovo il codice a barre della piastra")?;
        (celle(righe[j])[0].to_string(), j + 1)
    };
    if barcode.is_empty() {
        return Err("il codice a barre della piastra è vuoto".into());
    }
    let risultati = griglia(&righe, da_griglia).ok_or("non trovo la piastra dei risultati")?;

    // Le informazioni della misura (versione 2: «Measurement Basic Information», 3: «Measurement Information»).
    let (titolo_misura, titolo_piastra) = if v2 { ("Measurement Basic Information", "Plate Type") } else { ("Measurement Information", "Plate Type Information") };
    let i_misura = inizia(&righe, risultati.fine, titolo_misura).ok_or_else(|| format!("non trovo la sezione «{titolo_misura}»"))?;
    let i_tipo_piastra = inizia(&righe, i_misura + 1, titolo_piastra).ok_or_else(|| format!("non trovo la sezione «{titolo_piastra}»"))?;
    let info = coppie(&righe[i_misura + 1..i_tipo_piastra]);
    let serve = |k: &str| primo(&info, k).ok_or_else(|| format!("nella sezione «{titolo_misura}» manca «{k}»"));
    let seriale = serve("instrument serial number")?.to_string();
    let iniziata = serve("measurement started")?.to_string();
    let firma_protocollo = serve("protocol signature")?.to_string();
    let firma_misura = serve("measurement signature")?.to_string();
    let quando = tempo::iso(&iniziata, false);

    // I risultati dell'analisi delle immagini (versione 2): una piastra per parametro.
    let mut analisi: Vec<(String, Griglia)> = Vec::new();
    let mut i = risultati.fine;
    while let Some(k) = inizia(&righe, i, "Barcode:").filter(|k| *k < i_misura) {
        let parametro = righe[k].split(',').find_map(|c| c.trim().strip_prefix("AnalysisParameter:")).map(|p| p.trim().to_string());
        let Some(g) = griglia(&righe, k + 1) else { break };
        i = g.fine;
        if let Some(p) = parametro {
            analisi.push((p, g));
        }
    }

    // La mappa della piastra.
    let i_mappa = inizia(&righe, i_tipo_piastra, "Platemap").ok_or("non trovo la sezione «Platemap»")?;
    let mappa = griglia(&righe, i_mappa + 1).ok_or("non trovo la mappa della piastra")?;
    let nome_campione = |lettera: &str, colonna: usize| -> Option<&str> { mappa.righe.iter().find(|(l, _)| l == lettera).and_then(|(_, c)| c.get(colonna)).map(|s| s.as_str()).filter(|s| !s.is_empty()) };

    // Le impostazioni della lettura.
    let (inizio_imp, fine_imp) = if v2 { ("Measurements:", "Analysis:") } else { ("Details of Measurement Sequence", "Post Processing Sequence") };
    let i_imp = inizia(&righe, i_mappa, inizio_imp).ok_or_else(|| format!("non trovo la sezione «{inizio_imp}»"))?;
    let i_fine_imp = inizia(&righe, i_imp + 1, fine_imp).ok_or_else(|| format!("non trovo la sezione «{fine_imp}»"))?;
    let imp = coppie(&righe[i_imp + 1..i_fine_imp]);
    let mut letti: HashSet<&str> = ["tech", "operation", "meas"].into_iter().collect();
    let chiede = |k: &'static str, letti: &mut HashSet<&str>| -> Option<String> {
        letti.insert(k);
        primo(&imp, k).map(str::to_string)
    };

    let mut valori_dispositivo: Vec<Valore> = Vec::new();
    let mut impostazioni: Vec<Valore> = Vec::new();
    let altezza = chiede("measurement height [mm]", &mut letti).and_then(|v| numero(&v));
    let lampi = chiede("number of flashes", &mut letti).and_then(|v| numero(&v));
    let distanza = chiede("distance between plate and detector [mm]", &mut letti).and_then(|v| numero(&v));
    let posizione = chiede("excitation / emission", &mut letti);
    let emissione = chiede("emission wavelength [nm]", &mut letti).and_then(|v| lunghezza(&v));
    let eccitazione = chiede("excitation wavelength [nm]", &mut letti).and_then(|v| lunghezza(&v));
    let fuoco_altezza = chiede("focus height [µm]", &mut letti).and_then(|v| numero(&v));
    let canali = {
        for k in ["channel", "excitation power [%]", "exposure time [ms]", "additional focus offset [mm]"] {
            letti.insert(k);
        }
        let mut v: Vec<Canale> = Vec::new();
        for (k, x) in &imp {
            match k.as_str() {
                "channel" => v.push(Canale { nome: x.clone(), onda: None, potenza: None, esposizione: None, fuoco: None }),
                "excitation wavelength [nm]" => {
                    if let Some(c) = v.last_mut() {
                        c.onda = lunghezza(x);
                    }
                }
                "excitation power [%]" => {
                    if let Some(c) = v.last_mut() {
                        c.potenza = numero(x);
                    }
                }
                "exposure time [ms]" => {
                    if let Some(c) = v.last_mut() {
                        c.esposizione = numero(x);
                    }
                }
                "additional focus offset [mm]" => {
                    if let Some(c) = v.last_mut() {
                        c.fuoco = numero(x);
                    }
                }
                _ => {}
            }
        }
        v
    };
    if tipo == Tipo::Immagini && canali.is_empty() {
        return Err("per le immagini non trovo nessun canale nelle impostazioni della lettura".into());
    }
    // Quello che lo schema non ha: note dell'impostazione per ogni misura.
    for (chiave, k) in [("number of flashes integrated", "number of flashes integrated"), ("flash power", "flash power"), ("measurement mode setting", "measurement mode"), ("filter set", "filter set")] {
        if let Some(v) = chiede(k, &mut letti) {
            let d = if k == "filter set" || k == "measurement mode" { Dato::Testo(v) } else { dato(&v) };
            valori_dispositivo.push(Valore { chiave: chiave.into(), dato: d, unita: String::new(), posto: Posto::ImpostazioneAltro });
        }
    }
    for k in ["start plate repeat each [s]", "number of plate repeats", "sequence executed by"] {
        if let Some(v) = primo(&info, k) {
            valori_dispositivo.push(Valore { chiave: k.into(), dato: dato(v), unita: String::new(), posto: Posto::ImpostazioneAltro });
        }
    }
    let booleane: Vec<(&str, Option<String>)> = [("generate bright field", "generate bright field"), ("digital phase contrast", "digital phase contrast"), ("generate dpc", "generate dpc")].into_iter().map(|(a, k)| (a, chiede(k, &mut letti))).collect();
    // Le impostazioni che nessuno ha letto diventano note della misura.
    for (k, v) in &imp {
        if !letti.contains(k.as_str()) {
            impostazioni.push(Valore { chiave: k.clone(), dato: dato(v), unita: String::new(), posto: Posto::Altro });
        }
    }

    // Le impostazioni della misura per tipo (assorbanza: la lunghezza è quella d'eccitazione).
    let scansione = match posizione.as_deref() {
        Some(p) => Some(match p.to_uppercase().as_str() {
            "TOP" => "top scan position (plate reader)",
            "BOTTOM" => "bottom scan position (plate reader)",
            _ => return Err(format!("«{p}» non è una posizione di scansione valida: serve TOP o BOTTOM")),
        }),
        None => None,
    };
    let onda_rivelatore = match tipo {
        Tipo::Fluorescenza => emissione,
        Tipo::Assorbanza => eccitazione,
        _ => None,
    };
    let distanza_rivelatore = if tipo == Tipo::Luminescenza { distanza } else { altezza };

    // I pozzetti della piastra: righe per colonne del tipo di piastra (una lettura può coprirne solo una parte).
    let tipo_piastra = coppie(&righe[i_tipo_piastra..i_mappa]);
    let pozzetti_piastra = primo(&tipo_piastra, "number of rows").and_then(numero).zip(primo(&tipo_piastra, "number of columns").and_then(numero)).map(|(r, c)| (r * c) as u32).filter(|n| *n >= (risultati.righe.len() * risultati.colonne) as u32);
    let mut g = Gruppo { titolo: barcode.clone(), quando: quando.clone(), piastra: Some(barcode.clone()), pozzetti: Some(pozzetti_piastra.unwrap_or((risultati.righe.len() * risultati.colonne) as u32)), ..Default::default() };
    g.dati.push(("analytical method identifier".into(), firma_protocollo));
    g.dati.push(("experimental data identifier".into(), firma_misura));
    g.dati.push(("experiment type".into(), tipo_testo.clone()));
    for (k, e) in [("protocol owner", "~protocol owner"), ("measurement finished", "~measurement finished"), ("protocol name", "~protocol name")] {
        if let Some(v) = primo(&info, k) {
            g.dati.push((e.into(), v.to_string()));
        }
    }

    let mut parametri_numerici: Vec<String> = Vec::new();
    for (lettera, cs) in &risultati.righe {
        for (c, cella) in cs.iter().enumerate() {
            if cella.is_empty() {
                continue;
            }
            let pozzetto = format!("{lettera}{}", c + 1);
            let campione = nome_campione(lettera, c).filter(|s| *s != "-").map_or_else(|| format!("{barcode}_{pozzetto}"), str::to_string);
            let ruolo = nome_campione(lettera, c).filter(|s| *s != "-").and_then(ruolo);
            let nuova = |l: &mut Lettura| -> Misura {
                let mut m = Misura { id: l.id(), campione: campione.clone(), pozzetto: Some(pozzetto.clone()), piastra: Some(barcode.clone()), quando: quando.clone(), ..Default::default() };
                if let Some(r) = ruolo {
                    m.ruolo = r;
                    m.ruolo_dal_file = true;
                }
                m
            };
            if tipo != Tipo::Immagini {
                let mut m = nuova(l);
                m.valori.push(Valore::testo("detection type", tipo.rilevazione(), Posto::Impostazione));
                if let Some(x) = lampi {
                    m.valori.push(Valore::num("number of averages", x, "#", Posto::Impostazione));
                }
                if let Some(x) = distanza_rivelatore {
                    m.valori.push(Valore::num("detector distance setting (plate reader)", x, "mm", Posto::Impostazione));
                }
                if let Some(x) = scansione {
                    m.valori.push(Valore::testo("scan position setting (plate reader)", x, Posto::Impostazione));
                }
                if let Some(x) = onda_rivelatore {
                    m.valori.push(Valore::num("detector wavelength setting", x, "nm", Posto::Impostazione));
                }
                if let Some(x) = eccitazione.filter(|_| tipo != Tipo::Luminescenza) {
                    m.valori.push(Valore::num("excitation wavelength setting", x, "nm", Posto::Impostazione));
                }
                m.valori.extend(valori_dispositivo.iter().cloned());
                for (k, v) in &booleane {
                    if let Some(v) = v {
                        m.valori.push(Valore { chiave: (*k).into(), dato: dato(v), unita: String::new(), posto: Posto::ImpostazioneAltro });
                    }
                }
                m.valori.extend(impostazioni.iter().cloned());
                match numero(cella) {
                    Some(x) => m.valori.push(Valore::num(tipo.concetto(), x, tipo.unita(), Posto::Misura)),
                    None => m.valori.push(Valore { chiave: tipo.concetto().into(), dato: Dato::Assente(cella.clone()), unita: tipo.unita().into(), posto: Posto::Misura }),
                }
                g.misure.push(m);
                continue;
            }
            // Immagini: una misura per canale; l'immagine e i valori calcolati sono del pozzetto, e stanno sul primo canale.
            for (n, canale) in canali.iter().enumerate() {
                let mut m = nuova(l);
                m.valori.push(Valore::testo("detection type", tipo.rilevazione(), Posto::Impostazione));
                if let Some(x) = fuoco_altezza {
                    m.valori.push(Valore::num("detector distance setting (plate reader)", x / 1000.0, "mm", Posto::Impostazione));
                }
                if let Some(x) = canale.onda {
                    m.valori.push(Valore::num("excitation wavelength setting", x, "nm", Posto::Impostazione));
                }
                m.valori.push(Valore::testo(if canale.nome == "BRIGHTFIELD" { "transmitted light setting" } else { "fluorescent tag setting" }, &if canale.nome == "BRIGHTFIELD" { "brightfield".to_string() } else { canale.nome.clone() }, Posto::ImpostazioneAltro));
                if let Some(x) = canale.esposizione {
                    m.valori.push(Valore::num("exposure duration setting", x, "ms", Posto::ImpostazioneAltro));
                }
                if let Some(x) = canale.potenza {
                    m.valori.push(Valore::num("illumination setting", x, "%", Posto::ImpostazioneAltro));
                }
                m.valori.push(Valore::num("magnification setting", 4.0, "(unitless)", Posto::ImpostazioneAltro));
                if let Some(x) = canale.fuoco {
                    m.valori.push(Valore::num("additional focus offset", x, "mm", Posto::ImpostazioneAltro));
                }
                m.valori.extend(valori_dispositivo.iter().cloned());
                if canale.nome == "BRIGHTFIELD" {
                    for (k, v) in &booleane {
                        if let Some(v) = v {
                            m.valori.push(Valore { chiave: (*k).into(), dato: dato(v), unita: String::new(), posto: Posto::ImpostazioneAltro });
                        }
                    }
                }
                m.valori.extend(impostazioni.iter().cloned());
                if n == 0 {
                    m.valori.push(Valore::testo("experimental data identifier", cella, Posto::Immagine));
                    for (parametro, a) in &analisi {
                        let Some(v) = a.righe.iter().find(|(l, _)| l == lettera).and_then(|(_, cs)| cs.get(c)).filter(|v| !v.is_empty()) else { continue };
                        let numerico = a.righe.iter().flat_map(|(_, cs)| cs.iter()).any(|v| numero(v).is_some());
                        match (numerico, numero(v)) {
                            (true, Some(x)) => {
                                if !parametri_numerici.contains(parametro) {
                                    parametri_numerici.push(parametro.clone());
                                }
                                m.valori.push(Valore::num(parametro, x, "", Posto::ElaboratoAltro));
                            }
                            (true, None) => {}
                            (false, _) => m.valori.push(Valore::testo(parametro, v, Posto::ElaboratoAltro)),
                        }
                    }
                }
                g.misure.push(m);
            }
        }
    }
    if g.misure.is_empty() {
        return Err("nella piastra dei risultati non ci sono pozzetti con un valore".into());
    }

    l.strumento.produttore = "Revvity".into();
    l.strumento.identificativo = "EnSight".into();
    l.strumento.modello = "EnSight".into();
    l.strumento.seriale = seriale;
    l.strumento.software = "Kaleido".into();
    l.strumento.versione = versione;
    // Le altre informazioni della misura che lo schema non ha: restano come note del file.
    for (k, v) in &info {
        if !matches!(k.as_str(), "software version" | "barcode" | "instrument serial number" | "measurement started" | "protocol signature" | "measurement signature" | "protocol owner" | "measurement finished" | "protocol name" | "sequence executed by" | "number of plate repeats" | "start plate repeat each [s]") {
            l.sistema.push(Valore::testo(k, v, Posto::Altro));
        }
    }
    if let Some(v) = primo(&info, "protocol name") {
        l.info.push(("Protocollo".into(), v.to_string()));
    }
    if let Some(v) = primo(&info, "protocol owner") {
        l.info.push(("Proprietario del protocollo".into(), v.to_string()));
    }
    l.info.push(("Tipo di misura".into(), tipo_testo));
    l.info.push(("Codice a barre".into(), barcode));
    if let Some(v) = primo(&info, "measurement finished") {
        l.info.push(("Misura finita".into(), v.to_string()));
    }
    if let Some(v) = primo(&tipo_piastra, "plate type name") {
        l.info.push(("Tipo di piastra".into(), v.to_string()));
    }
    if tipo == Tipo::Immagini {
        l.principale = parametri_numerici.first().cloned().unwrap_or_else(|| "excitation wavelength setting".into());
        l.unita = if parametri_numerici.is_empty() { "nm".into() } else { String::new() };
    } else {
        l.principale = tipo.concetto().into();
        l.unita = tipo.unita().into();
    }
    l.gruppi.push(g);
    Ok(())
}
