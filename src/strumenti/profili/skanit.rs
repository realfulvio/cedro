//! Thermo Fisher SkanIt (Varioskan LUX e gli altri lettori di piastre): l'Excel che il programma esporta
//! dopo una lettura. Un foglio per passo del protocollo («Absorbance 1_01», «Luminescence 1_01», poi i
//! passi di calcolo: «Blank Subtraction 1_02», «Concentration mg_mL_07»…) con le piastre a griglia
//! (A–H per 1–12, o 384), e i fogli «General information», «Session information», «Instrument
//! information» e «Protocol parameters» con il resto.

use std::collections::HashMap;

use crate::strumenti::fonte::{numero, Fonte, Foglio};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "thermo_skanit",
    nome: "Thermo Fisher SkanIt",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["xlsx"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let prima = f.fogli.first().and_then(|g| g.righe.iter().find(|r| r.iter().any(|c| !c.trim().is_empty()))).and_then(|r| r.first()).is_some_and(|c| c.trim() == "Measurement results");
    let completo = f.foglio_nome("Session information").is_some() && f.foglio_nome("Instrument information").is_some();
    match (completo, prima) {
        (true, true) => 95,
        (true, false) => 80,
        (false, true) => 70,
        _ => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modo {
    Assorbanza,
    Fluorescenza,
    Luminescenza,
}

impl Modo {
    fn da_nome(s: &str) -> Option<Modo> {
        let s = s.trim().to_lowercase();
        if s.starts_with("absorbance") {
            Some(Modo::Assorbanza)
        } else if s.starts_with("fluorescence") {
            Some(Modo::Fluorescenza)
        } else if s.starts_with("luminescence") {
            Some(Modo::Luminescenza)
        } else {
            None
        }
    }

    /// Come SkanIt e allotropy scrivono il tipo di rilevazione.
    fn nome(self) -> &'static str {
        match self {
            Modo::Assorbanza => "Absorbance",
            Modo::Fluorescenza => "Fluorescence",
            Modo::Luminescenza => "Luminescence",
        }
    }

    fn concetto(self) -> &'static str {
        match self {
            Modo::Assorbanza => "absorbance",
            Modo::Fluorescenza => "fluorescence",
            Modo::Luminescenza => "luminescence",
        }
    }

    fn unita(self) -> &'static str {
        match self {
            Modo::Assorbanza => "OD",
            Modo::Fluorescenza => "RFU",
            Modo::Luminescenza => "RLU",
        }
    }
}

/// Una piastra a griglia di un foglio: l'etichetta della tabella («Abs», «RLU», «Blank subtracted»,
/// «Sample»), la piastra e la lunghezza d'onda a cui appartiene, le righe A, B, C… con le celle 1, 2, 3…
struct Griglia {
    etichetta: String,
    piastra: String,
    onda: Option<f64>,
    colonne: usize,
    righe: Vec<(String, Vec<String>)>,
}

/// «Plate 1», «Blank plate», «Sample Plate»: il nome della piastra scritto da SkanIt sopra la tabella.
fn nome_piastra(t: &str) -> Option<String> {
    let parole: Vec<&str> = t.split_whitespace().collect();
    if parole.is_empty() || parole.len() > 4 {
        return None;
    }
    let i = parole.iter().position(|p| p.trim_matches(|c: char| !c.is_alphanumeric()).eq_ignore_ascii_case("plate"))?;
    let mut v: Vec<&str> = Vec::new();
    if i > 0 && parole[i - 1].chars().all(|c| c.is_alphanumeric() || c == '_') {
        v.push(parole[i - 1]);
    }
    v.push(parole[i].trim_matches(|c: char| !c.is_alphanumeric()));
    if let Some(n) = parole.get(i + 1).filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        v.push(n);
    }
    Some(v.join(" "))
}

/// «Wavelength: 450 nm»: la lunghezza d'onda (zero, quella della luminescenza, è nessuna).
fn riga_onda(t: &str) -> Option<Option<f64>> {
    let resto = t.trim().strip_prefix("Wavelength:")?;
    let x = numero(resto.trim().trim_end_matches("nm").trim());
    Some(x.filter(|x| *x > 0.0))
}

/// La riga d'intestazione di una tabella: l'etichetta e poi 1, 2, 3… Dà il numero di colonne.
fn intestazione(r: &[String]) -> Option<usize> {
    if r.first().map(|c| c.trim()).unwrap_or("").is_empty() {
        return None;
    }
    let mut n = 0;
    while r.get(n + 1).and_then(|c| numero(c)) == Some((n + 1) as f64) {
        n += 1;
    }
    (n >= 2).then_some(n)
}

fn e_lettera(c: Option<&String>) -> Option<String> {
    let t = c?.trim();
    (!t.is_empty() && t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase())).then(|| t.to_string())
}

/// Le piastre a griglia di un foglio, con la piastra e la lunghezza d'onda scritte sopra.
fn griglie(righe: &[Vec<String>]) -> Vec<Griglia> {
    let mut out = Vec::new();
    let mut piastra: Option<String> = None;
    let mut onda: Option<f64> = None;
    // Le prime righe sono «Measurement results», il file, la data e il titolo del passo.
    let mut k = 5;
    while k < righe.len() {
        let r = &righe[k];
        let c0 = r.first().map(|s| s.trim()).unwrap_or("");
        if let Some(o) = riga_onda(c0) {
            onda = o;
        } else if let Some(p) = nome_piastra(c0) {
            piastra = Some(p);
        } else if let Some(n) = intestazione(r) {
            let mut celle = Vec::new();
            let mut j = k + 1;
            while let Some(l) = righe.get(j).and_then(|r| e_lettera(r.first())) {
                celle.push((l, (1..=n).map(|c| righe[j].get(c).map(|s| s.trim().to_string()).unwrap_or_default()).collect()));
                j += 1;
            }
            out.push(Griglia { etichetta: c0.to_string(), piastra: piastra.clone().unwrap_or_else(|| "Plate 1".into()), onda, colonne: n, righe: celle });
            k = j;
            continue;
        }
        k += 1;
    }
    out
}

/// Le voci dei fogli d'informazione: il livello (la colonna della chiave), la chiave e il valore.
/// Le righe che hanno solo un testo in fondo (le note a più righe) continuano la voce di sopra.
fn voci(g: &Foglio) -> Vec<(usize, String, String)> {
    let mut out: Vec<(usize, String, String)> = Vec::new();
    for r in &g.righe {
        let Some(l) = r.iter().position(|c| !c.trim().is_empty()) else { continue };
        if l == 0 {
            continue;
        }
        let testo = r.iter().skip(l + 1).rev().find(|c| !c.trim().is_empty()).map(|c| c.trim().to_string());
        if l >= 4 {
            if let Some(ultima) = out.last_mut() {
                if !ultima.2.is_empty() {
                    ultima.2.push('\n');
                }
                ultima.2.push_str(r[l].trim());
            }
            continue;
        }
        out.push((l, r[l].trim().to_string(), testo.unwrap_or_default()));
    }
    out
}

fn voce<'a>(v: &'a [(usize, String, String)], livello: usize, chiave: &str) -> Option<&'a str> {
    v.iter().find(|(l, k, _)| *l == livello && k == chiave).map(|(_, _, t)| t.as_str()).filter(|t| !t.is_empty())
}

/// La prima versione («7.0.0.50») in un testo come «ver. 7.0.0.50».
fn versione(t: &str) -> Option<String> {
    t.split_whitespace().map(|p| p.trim_end_matches(['.', ',', ')'])).find(|p| p.starts_with(|c: char| c.is_ascii_digit()) && p.chars().all(|c| c.is_ascii_digit() || c == '.')).map(str::to_string)
}

/// Il ruolo dal nome del campione: «Std0001», «Un0003», «Blank1», «Ctrl2».
fn ruolo(campione: &str) -> Option<Ruolo> {
    let senza: String = campione.chars().filter(|c| !c.is_ascii_digit()).collect();
    match senza.as_str() {
        "Un" => Some(Ruolo::Campione),
        "Std" => Some(Ruolo::Standard),
        "Blank" => Some(Ruolo::Bianco),
        "Ctrl" => Some(Ruolo::Controllo),
        _ => None,
    }
}

/// «Measurement Time [ms]»: il nome senza l'unità, e l'unità.
fn nome_e_unita(k: &str) -> (String, String) {
    match k.rsplit_once('[') {
        Some((a, b)) if b.ends_with(']') => (a.trim().to_string(), b.trim_end_matches(']').trim().to_string()),
        _ => (k.trim().to_string(), String::new()),
    }
}

/// Le impostazioni dei passi del foglio «Protocol parameters»: il titolo del passo con le sue coppie
/// chiave e valore, e prima le impostazioni generali.
fn protocollo(g: &Foglio) -> (Vec<(String, String)>, HashMap<String, Vec<(String, String)>>) {
    let mut generali = Vec::new();
    let mut passi: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut corrente: Option<String> = None;
    for r in &g.righe {
        let c0 = r.first().map(|c| c.trim()).unwrap_or("");
        if !c0.is_empty() {
            if c0 != "Protocol parameters" {
                corrente = Some(c0.to_string());
            }
            continue;
        }
        let (Some(k), Some(v)) = (r.get(1).map(|c| c.trim()).filter(|c| !c.is_empty()), r.get(4).map(|c| c.trim()).filter(|c| !c.is_empty())) else { continue };
        match &corrente {
            Some(c) => passi.entry(c.clone()).or_default().push((k.to_string(), v.to_string())),
            None => generali.push((k.to_string(), v.to_string())),
        }
    }
    (generali, passi)
}

struct Letta {
    piastra: String,
    pozzetto: String,
    onda: Option<f64>,
    id: String,
    modo: Modo,
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    // I fogli dei passi: quelli che cominciano con «Measurement results».
    let passi: Vec<&Foglio> = f.fogli.iter().filter(|g| g.righe.iter().find(|r| r.iter().any(|c| !c.trim().is_empty())).and_then(|r| r.first()).is_some_and(|c| c.trim() == "Measurement results")).collect();
    let titolo_di = |g: &Foglio| g.righe.get(4).and_then(|r| r.first()).map(|c| c.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| g.nome.trim().to_string());
    let modo_di = |g: &Foglio| Modo::da_nome(&g.nome).or_else(|| Modo::da_nome(&titolo_di(g)));
    if !passi.iter().any(|g| modo_di(g).is_some()) {
        return Err("manca il foglio della lettura: cerco «Absorbance», «Fluorescence» o «Luminescence»".into());
    }

    // L'intestazione: la sessione, lo strumento, il programma.
    let sessione = f.foglio_nome("Session information").map(voci).unwrap_or_default();
    let strumento = f.foglio_nome("Instrument information").map(voci).unwrap_or_default();
    let generale = f.foglio_nome("General information").map(voci).unwrap_or_default();
    let (impostazioni_generali, impostazioni_passi) = f.foglio_nome("Protocol parameters").map(protocollo).unwrap_or_default();

    let prima = passi.iter().find(|g| modo_di(g).is_some()).copied().ok_or("manca il foglio della lettura")?;
    let nome_sessione = voce(&sessione, 1, "Session name").map(str::to_string).or_else(|| prima.righe.get(1).and_then(|r| r.first()).map(|c| c.trim().to_string()).filter(|c| !c.is_empty()));
    let id_sessione = nome_sessione.as_deref().map(|n| n.strip_suffix(".skax").unwrap_or(n).to_string());
    let quando = voce(&sessione, 1, "Execution time").map(str::to_string).or_else(|| prima.righe.get(2).and_then(|r| r.first()).map(|c| c.trim().to_string())).and_then(|d| tempo::iso(&d, false));
    let note_sessione = voce(&sessione, 1, "Session notes").map(str::to_string);

    // Lo strumento: il nome del primo livello (i filtri hanno un loro «Name» più in basso).
    let nome_strumento = voce(&strumento, 1, "Name");
    let seriale = voce(&strumento, 1, "Serial number").map(|s| s.strip_suffix(".0").unwrap_or(s).to_string());
    // Il programma: «SkanIt Software 7.0 RE for Microplate Readers RE, ver. 7.0.0.50».
    let (software, versione_sw) = match voce(&generale, 1, "Report generated with SW version") {
        Some(t) => match t.split_once(',') {
            Some((a, b)) => (Some(a.trim().to_string()), versione(b)),
            None => (Some(t.trim().to_string()), None),
        },
        None => (None, None),
    };
    // Il resto che lo schema non ha: le impostazioni dello strumento per ogni misura, le altre voci per il file.
    let compensazione = voce(&strumento, 1, "Optical response compensation").map(str::to_string);
    let adattatore = voce(&strumento, 2, "Plate adapter name").map(str::to_string);
    let numero_adattatore = voce(&strumento, 2, "Plate adapter number").and_then(numero);
    const LETTE: [&str; 20] = [
        "Wavelength", "Incubator", "Date and time of definition", "Top optics", "Bottom optics", "Dispenser 1", "Dispenser 2", "Module's name", "Module's serial number", "Instrument modules", "Gas control", "Bandwidth", "Position", "Filter", "Name", "Serial number",
        "ESW version", "Optical response compensation", "Plate adapter name", "Plate adapter number",
    ];
    for (_, k, v) in strumento.iter().filter(|(_, k, v)| !LETTE.contains(&k.as_str()) && !v.is_empty()) {
        l.sistema.push(Valore::testo(k, v, Posto::Altro));
    }
    for (_, k, v) in generale.iter().filter(|(_, k, v)| !matches!(k.as_str(), "Report generated with SW version" | "Filter" | "Position") && !v.is_empty()) {
        l.sistema.push(Valore::testo(k, v, Posto::Altro));
    }

    // Le piastre dei fogli: prima i nomi dei campioni (le tabelle «Sample» di ogni piastra), poi le letture.
    let tutte: Vec<(&Foglio, Vec<Griglia>)> = passi.iter().map(|g| (*g, griglie(&g.righe))).collect();
    let mut campioni: HashMap<String, HashMap<(String, usize), String>> = HashMap::new();
    for (_, gs) in &tutte {
        for g in gs.iter().filter(|g| g.etichetta.eq_ignore_ascii_case("sample")) {
            let m = campioni.entry(g.piastra.clone()).or_default();
            for (lettera, celle) in &g.righe {
                for (i, c) in celle.iter().enumerate() {
                    if !c.is_empty() {
                        m.insert((lettera.clone(), i + 1), c.clone());
                    }
                }
            }
        }
    }

    let n_letture = passi.iter().filter(|g| modo_di(g).is_some()).count();
    let mut gruppi: Vec<Gruppo> = Vec::new();
    let mut lette: Vec<Letta> = Vec::new();
    let mut principale: Option<Modo> = None;
    for (g, gs) in &tutte {
        let Some(modo) = modo_di(g) else { continue };
        principale.get_or_insert(modo);
        let titolo_passo = titolo_di(g);
        let del_passo = impostazioni_passi.get(&titolo_passo).cloned().unwrap_or_default();
        let mut trovata = false;
        for gr in gs.iter().filter(|x| !x.etichetta.eq_ignore_ascii_case("sample")) {
            trovata = true;
            // La lunghezza d'onda: quella scritta sopra la tabella, o quella del protocollo.
            let onda = gr.onda.or_else(|| del_passo.iter().find(|(k, _)| nome_e_unita(k).0.eq_ignore_ascii_case("wavelength")).and_then(|(_, v)| numero(v)).filter(|x| *x > 0.0));
            let titolo = {
                let mut parti: Vec<String> = Vec::new();
                if n_letture > 1 {
                    parti.push(titolo_passo.clone());
                }
                parti.push(gr.piastra.clone());
                if let Some(o) = onda.filter(|o| !titolo_passo.contains(&format!("{o}"))) {
                    parti.push(format!("{o} nm"));
                }
                parti.join(" · ")
            };
            let gi = match gruppi.iter().position(|x| x.titolo == titolo) {
                Some(i) => i,
                None => {
                    let mut nuovo = Gruppo { titolo, quando: quando.clone(), piastra: Some(gr.piastra.clone()), pozzetti: Some((gr.righe.len() * gr.colonne) as u32), ..Default::default() };
                    if let Some(x) = &id_sessione {
                        nuovo.dati.push(("experimental data identifier".into(), x.clone()));
                    }
                    if let Some(x) = &note_sessione {
                        nuovo.dati.push(("~Session notes".into(), x.clone()));
                    }
                    gruppi.push(nuovo);
                    gruppi.len() - 1
                }
            };
            let nomi = campioni.get(&gr.piastra);
            for (lettera, celle) in &gr.righe {
                for (i, c) in celle.iter().enumerate() {
                    if c.is_empty() {
                        continue;
                    }
                    let pozzetto = format!("{lettera}{}", i + 1);
                    let campione = nomi.and_then(|m| m.get(&(lettera.clone(), i + 1))).cloned().unwrap_or_else(|| format!("{}_{pozzetto}", gr.piastra));
                    let mut m = Misura { id: l.id(), campione, pozzetto: Some(pozzetto.clone()), piastra: Some(gr.piastra.clone()), quando: quando.clone(), ..Default::default() };
                    if let Some(r) = ruolo(&m.campione) {
                        m.ruolo = r;
                        m.ruolo_dal_file = true;
                    }
                    m.valori.push(Valore::testo("detection type", modo.nome(), Posto::Impostazione));
                    if let Some(o) = onda {
                        m.valori.push(Valore::num("detector wavelength setting", o, "nm", Posto::Impostazione));
                    }
                    if let Some(x) = &compensazione {
                        m.valori.push(Valore::testo("Optical response compensation", x, Posto::ImpostazioneAltro));
                    }
                    if let Some(x) = &adattatore {
                        m.valori.push(Valore::testo("Plate adapter name", x, Posto::ImpostazioneAltro));
                    }
                    if let Some(x) = numero_adattatore {
                        m.valori.push(Valore::num("Plate adapter number", x, "", Posto::ImpostazioneAltro));
                    }
                    // Le impostazioni del passo che non sono la lunghezza d'onda già letta.
                    for (k, v) in &del_passo {
                        let (nome, unita) = nome_e_unita(k);
                        let minuscolo = nome.to_lowercase();
                        if minuscolo == "wavelength" {
                            continue;
                        }
                        if minuscolo.contains("excitation") && minuscolo.contains("wavelength") {
                            if let Some(x) = numero(v) {
                                m.valori.push(Valore::num("excitation wavelength setting", x, "nm", Posto::Impostazione));
                                continue;
                            }
                        }
                        if minuscolo.contains("emission") && minuscolo.contains("wavelength") && onda.is_none() {
                            if let Some(x) = numero(v) {
                                m.valori.push(Valore::num("detector wavelength setting", x, "nm", Posto::Impostazione));
                                continue;
                            }
                        }
                        m.valori.push(match numero(v) {
                            Some(x) => Valore::num(&nome, x, &unita, Posto::ImpostazioneAltro),
                            None => Valore::testo(&nome, v, Posto::ImpostazioneAltro),
                        });
                    }
                    // Il valore: un numero, o il testo che lo strumento ha scritto al suo posto («OVRFLW»).
                    match numero(c) {
                        Some(x) => m.valori.push(Valore::num(modo.concetto(), x, modo.unita(), Posto::Misura)),
                        None => m.valori.push(Valore { chiave: modo.concetto().into(), dato: Dato::Assente(c.clone()), unita: modo.unita().into(), posto: Posto::Misura }),
                    }
                    lette.push(Letta { piastra: gr.piastra.clone(), pozzetto, onda, id: m.id.clone(), modo });
                    gruppi[gi].misure.push(m);
                }
            }
        }
        if !trovata {
            l.avvisi.push(format!("il passo «{titolo_passo}» non ha tabelle di piastra e non è stato letto"));
        }
    }
    if gruppi.iter().all(|g| g.misure.is_empty()) {
        return Err("nel file non ci sono pozzetti con un valore".into());
    }
    let principale = principale.unwrap_or(Modo::Assorbanza);

    // I passi di calcolo: ogni valore di ogni piastra diventa un calcolo che viene dalle letture di quel pozzetto.
    let mut per_pozzetto: HashMap<(&str, &str), Vec<&Letta>> = HashMap::new();
    for x in &lette {
        per_pozzetto.entry((x.piastra.as_str(), x.pozzetto.as_str())).or_default().push(x);
    }
    for (g, gs) in &tutte {
        if modo_di(g).is_some() {
            continue;
        }
        let titolo = titolo_di(g);
        let formula = g.righe.iter().find_map(|r| r.first().and_then(|c| c.trim().strip_prefix("Formula:")).map(|x| x.trim().to_string())).unwrap_or_default();
        let senza_unita = titolo.to_lowercase().starts_with("blank subtraction");
        let unita_titolo = titolo.split_whitespace().last().filter(|u| u.contains('/')).map(str::to_string).unwrap_or_default();
        let mut scritti = 0;
        for gr in gs.iter().filter(|x| !x.etichetta.eq_ignore_ascii_case("sample")) {
            let nome = if gr.etichetta.eq_ignore_ascii_case("value") { titolo.clone() } else { format!("{titolo} – {}", gr.etichetta) };
            for (lettera, celle) in &gr.righe {
                for (i, c) in celle.iter().enumerate() {
                    let Some(valore) = numero(c) else { continue };
                    let pozzetto = format!("{lettera}{}", i + 1);
                    let fonti: Vec<&&Letta> = per_pozzetto.get(&(gr.piastra.as_str(), pozzetto.as_str())).into_iter().flatten().filter(|x| gr.onda.is_none() || x.onda == gr.onda).collect();
                    let Some(prima) = fonti.first() else { continue };
                    let unita = if senza_unita {
                        prima.modo.unita().to_string()
                    } else if gr.etichetta.contains('%') {
                        "%".to_string()
                    } else {
                        unita_titolo.clone()
                    };
                    l.calcolati.push(Calcolato { nome: nome.clone(), valore, unita, sorgenti: fonti.iter().map(|x| x.id.clone()).collect(), da: prima.modo.concetto().into(), descrizione: formula.clone(), da_calcoli: Vec::new() });
                    scritti += 1;
                }
            }
        }
        if scritti == 0 {
            l.avvisi.push(format!("il passo di calcolo «{titolo}» non ha valori per piastra e non è stato letto"));
        }
    }

    // Lo strumento e il programma.
    l.strumento.produttore = "Thermo Fisher Scientific".into();
    if let Some(n) = nome_strumento {
        l.strumento.identificativo = n.to_string();
        l.strumento.modello = n.to_string();
    }
    if let Some(s) = seriale {
        l.strumento.seriale = s;
    }
    if let Some(x) = voce(&strumento, 1, "ESW version") {
        l.strumento.firmware = x.to_string();
    }
    l.strumento.software = software.unwrap_or_else(|| "SkanIt".into());
    if let Some(v) = versione_sw {
        l.strumento.versione = v;
    }

    // Per il foglio «Strumento»: la sessione, i moduli, i filtri, la dotazione.
    if let Some(x) = &nome_sessione {
        l.info.push(("Sessione".into(), x.clone()));
    }
    if let Some(x) = &note_sessione {
        l.info.push(("Note della sessione".into(), x.replace('\n', " · ")));
    }
    if let Some(x) = voce(&sessione, 1, "Executed with") {
        l.info.push(("Eseguita con".into(), x.to_string()));
    }
    let mut moduli: Vec<String> = Vec::new();
    let mut filtri: Vec<String> = Vec::new();
    let mut dotazione: Vec<String> = Vec::new();
    let mut in_filtro = false;
    let mut filtro: (String, String) = Default::default();
    for (livello, k, v) in &strumento {
        if *livello == 1 {
            if in_filtro && !filtro.0.is_empty() {
                filtri.push(if filtro.1.is_empty() { filtro.0.clone() } else { format!("{} (#{})", filtro.0, filtro.1) });
            }
            in_filtro = k == "Filter";
            filtro = Default::default();
            continue;
        }
        match (k.as_str(), in_filtro) {
            ("Name", true) => filtro.0 = v.clone(),
            ("Position", true) => filtro.1 = v.clone(),
            ("Module's name", _) => moduli.push(v.clone()),
            ("Module's serial number", _) => {
                if let Some(u) = moduli.last_mut() {
                    *u = format!("{u} ({v})");
                }
            }
            ("Incubator" | "Gas control" | "Top optics" | "Bottom optics" | "Dispenser" | "Dispenser 1" | "Dispenser 2", _) => dotazione.push(format!("{k}: {v}")),
            _ => {}
        }
    }
    if in_filtro && !filtro.0.is_empty() {
        filtri.push(if filtro.1.is_empty() { filtro.0.clone() } else { format!("{} (#{})", filtro.0, filtro.1) });
    }
    if !moduli.is_empty() {
        l.info.push(("Moduli dello strumento".into(), moduli.join(", ")));
    }
    if !filtri.is_empty() {
        l.info.push(("Filtri ottici".into(), filtri.join(", ")));
    }
    if !dotazione.is_empty() {
        l.info.push(("Dotazione".into(), dotazione.join(" · ")));
    }
    for (k, v) in &impostazioni_generali {
        l.info.push((k.clone(), v.clone()));
    }
    l.principale = principale.concetto().into();
    l.unita = principale.unita().into();
    l.gruppi.extend(gruppi);
    Ok(())
}
