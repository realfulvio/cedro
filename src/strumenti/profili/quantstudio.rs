//! Applied Biosystems QuantStudio (qPCR): l'export dei risultati, un txt a sezioni «[Results]» o un
//! Excel con un foglio per sezione. Si leggono i pozzetti e i bersagli (Sample Setup), i risultati
//! per pozzetto (Ct, Rn, quantità, RQ…) e, se ci sono, il numero di cicli dall'amplificazione. Le
//! curve ciclo per ciclo (amplificazione, multicomponente, fusione) restano nel file dello
//! strumento: Cedro non le copia in Excel né in Allotrope.

use std::collections::HashMap;

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "appbio_quantstudio",
    nome: "Applied Biosystems QuantStudio",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Qpcr,
    estensioni: &["txt", "xlsx"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    if f.foglio() {
        let nomi: Vec<String> = f.fogli.iter().map(|s| s.nome.trim().trim_matches(['[', ']']).to_string()).collect();
        let ok = nomi.iter().any(|n| n == "Results") && f.fogli.iter().take(1).any(|s| s.righe.iter().take(60).any(|r| r.first().is_some_and(|k| k.trim() == "Instrument Type") && r.get(1).is_some_and(|v| v.contains("QuantStudio"))));
        return if ok { 95 } else { 0 };
    }
    let testa: Vec<&str> = f.testo.lines().take(80).collect();
    let strumento = testa.iter().any(|l| l.starts_with("* Instrument Type") && l.contains("QuantStudio"));
    let sezione = f.testo.lines().any(|l| l.trim() == "[Results]");
    match (strumento, sezione) {
        (true, true) => 95,
        (true, false) => 70,
        (false, true) if testa.iter().any(|l| l.starts_with("* ") && l.contains(" = ")) => 60,
        _ => 0,
    }
}

#[derive(Default)]
struct Sezione {
    colonne: Vec<String>,
    righe: Vec<Vec<String>>,
}

impl Sezione {
    fn riga<'a>(&'a self, r: &'a [String]) -> HashMap<&'a str, &'a str> {
        self.colonne.iter().zip(r.iter()).map(|(c, v)| (c.as_str(), v.as_str())).collect()
    }
}

#[derive(Default)]
struct Esperimento {
    intestazione: Vec<(String, String)>,
    sezioni: Vec<(String, Sezione)>,
    meta_risultati: Vec<(String, String)>,
}

impl Esperimento {
    fn chiave(&self, k: &[&str]) -> Option<&str> {
        k.iter().find_map(|k| self.intestazione.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())).filter(|v| !v.is_empty())
    }
    fn sezione(&self, nome: &str) -> Option<&Sezione> {
        self.sezioni.iter().find(|(n, _)| n == nome).map(|(_, s)| s)
    }
}

/// Il numero del pozzetto: «1» o «1.0» (alcuni export lo scrivono così).
fn id_pozzetto(s: &str) -> Option<i64> {
    let t = s.trim();
    t.parse::<i64>().ok().or_else(|| t.parse::<f64>().ok().filter(|x| x.fract() == 0.0 && x.abs() < 1e12).map(|x| x as i64))
}

fn senza_virgolette(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(t).to_string()
}

fn carica_txt(testo: &str) -> Result<Esperimento, String> {
    let righe: Vec<&str> = testo.lines().collect();
    let mut e = Esperimento::default();
    let mut i = 0;
    while i < righe.len() && !righe[i].trim_start().starts_with('[') {
        if let Some((k, v)) = righe[i].split_once('=') {
            e.intestazione.push((k.replace('*', "").trim().to_string(), v.trim().to_string()));
        }
        i += 1;
    }
    while i < righe.len() {
        let titolo = righe[i].trim();
        i += 1;
        let Some(nome) = titolo.strip_prefix('[').and_then(|t| t.rsplit_once(']')).map(|(n, _)| n.to_string()) else { continue };
        let mut dati: Vec<&str> = Vec::new();
        while i < righe.len() && !righe[i].trim().is_empty() && !righe[i].trim_start().starts_with('[') {
            dati.push(righe[i]);
            i += 1;
        }
        while i < righe.len() && righe[i].trim().is_empty() {
            i += 1;
        }
        let mut meta: Vec<&str> = Vec::new();
        while i < righe.len() && !righe[i].trim().is_empty() && !righe[i].trim_start().starts_with('[') {
            meta.push(righe[i]);
            i += 1;
        }
        let mut it = dati.into_iter().map(|l| l.split('\t').map(senza_virgolette).collect::<Vec<_>>());
        let colonne = it.next().unwrap_or_default();
        let rows: Vec<Vec<String>> = it.collect();
        if nome == "Results" {
            e.meta_risultati = meta.iter().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
        }
        e.sezioni.push((nome, Sezione { colonne, righe: rows }));
    }
    Ok(e)
}

fn carica_xlsx(f: &Fonte) -> Result<Esperimento, String> {
    let mut e = Esperimento::default();
    for (n, foglio) in f.fogli.iter().enumerate() {
        let r = &foglio.righe;
        let vuota = |x: &Vec<String>| x.first().is_none_or(|c| c.trim().is_empty());
        let mut i = 0;
        let mut pairs: Vec<(String, String)> = Vec::new();
        while i < r.len() && !vuota(&r[i]) {
            pairs.push((r[i][0].trim().to_string(), r[i].get(1).map(|v| v.trim().to_string()).unwrap_or_default()));
            i += 1;
        }
        if n == 0 {
            e.intestazione = pairs;
        }
        while i < r.len() && r[i].iter().all(|c| c.trim().is_empty()) {
            i += 1;
        }
        let colonne: Vec<String> = r.get(i).cloned().unwrap_or_default().iter().map(|c| c.trim().to_string()).collect();
        i += 1;
        let mut rows = Vec::new();
        while i < r.len() && r[i].first().is_some_and(|c| id_pozzetto(c).is_some()) {
            rows.push(r[i].iter().map(|c| c.trim().to_string()).collect());
            i += 1;
        }
        let nome = foglio.nome.trim().trim_matches(['[', ']']).to_string();
        if nome == "Results" {
            for x in &r[i.min(r.len())..] {
                if let Some(k) = x.first().filter(|k| !k.trim().is_empty()) {
                    e.meta_risultati.push((k.trim().to_string(), x.get(1).map(|v| v.trim().to_string()).unwrap_or_default()));
                }
            }
        }
        e.sezioni.push((nome, Sezione { colonne, righe: rows }));
    }
    Ok(e)
}

fn volume_pozzetto(blocco: &str) -> Option<f64> {
    // «0.2mL» o «0.1-mL» nel nome del blocco: da mL a μL.
    let b = blocco.as_bytes();
    for i in 0..b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit() && b[i - 1] != b'.') {
            let fine = blocco[i..].find(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(blocco.len(), |k| i + k);
            let num = &blocco[i..fine];
            if num.contains('.') && (blocco[fine..].starts_with("mL") || blocco[fine..].starts_with("-mL")) {
                return num.parse::<f64>().ok().map(|v| v * 1000.0);
            }
        }
    }
    if blocco.contains("384-Well Block") {
        Some(40.0)
    } else if blocco.contains("Taqman Array Card") {
        Some(1.5)
    } else {
        None
    }
}

fn ruolo_da_task(t: &str) -> Option<Ruolo> {
    match t.trim() {
        "UNKNOWN" => Some(Ruolo::Campione),
        "STANDARD" => Some(Ruolo::Standard),
        "NTC" | "POSITIVE_CONTROL" | "IPC" | "BLOCKED_IPC" | "BLOCKEDIPC" | "PC_ALLELE_1" | "PC_ALLELE_2" | "PC_ALLELE_BOTH" | "POSITIVE_11" | "POSITIVE_22" | "POSITIVE_12" | "Accuracy Control" => Some(Ruolo::Controllo),
        _ => None,
    }
}

/// Come si scrive il Ct nel file: «CT», «Ct», «Crt», «Cq» (anche dopo il prefisso dell'allele).
fn colonna_ct(colonne: &[String]) -> Option<String> {
    colonne.iter().find_map(|c| ["Ct", "CT", "Crt", "CRT", "Cq", "CQ"].iter().find(|s| c.ends_with(*s)).map(|s| s.to_string()))
}

fn maiuscola(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

struct Voce {
    well: i64,
    bersaglio: String,
    campione: String,
    reporter: Option<String>,
    posizione: Option<String>,
    quencher: Option<String>,
    task: String,
    gruppo: Option<String>,
    colori: Vec<(String, String)>,
}

fn pieno(m: &HashMap<&str, &str>, k: &str) -> Option<String> {
    m.get(k).map(|v| v.trim()).filter(|v| !v.is_empty()).map(str::to_string)
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let e = if f.foglio() { carica_xlsx(f)? } else { carica_txt(&f.testo)? };
    let risultati = e.sezione("Results").ok_or("manca la sezione Results")?;
    let tipo = e.chiave(&["Experiment Type"]).map(|s| s.trim().to_string()).ok_or("manca il tipo di esperimento")?;
    let (tipo_asm, genotipi) = match tipo.as_str() {
        "Standard Curve" => ("standard curve qPCR experiment", false),
        "Relative Standard Curve" => ("relative standard curve qPCR experiment", false),
        "Comparative Cт (ΔΔCт)" => ("comparative CT qPCR experiment", false),
        "Melt Curve" => ("melt curve qPCR experiment", false),
        "Genotyping" => ("genotyping qPCR experiment", true),
        "Presence/Absence" => ("presence/absence qPCR experiment", false),
        altro => return Err(format!("tipo di esperimento non riconosciuto: «{altro}»")),
    };
    let fine = e.chiave(&["Experiment Run End Time", "Experiment Run Stop Time"]).ok_or("manca l'ora della corsa")?.to_string();
    let quando = tempo::iso(&fine, false);

    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.identificativo = e.chiave(&["Instrument Name"]).unwrap_or("").to_string();
    l.strumento.modello = e.chiave(&["Instrument Type"]).unwrap_or("").to_string();
    l.strumento.seriale = e.chiave(&["Instrument Serial Number"]).unwrap_or("").to_string();
    l.strumento.software = "Thermo QuantStudio".into();
    l.strumento.versione = "1.0".into();
    l.principale = "cycle threshold result (qPCR)".into();
    l.unita = String::new();
    l.info.push(("Tipo di esperimento".into(), tipo.clone()));
    for (k, nome) in [("Experiment Name", "Esperimento"), ("Chemistry", "Chimica"), ("Passive Reference", "Riferimento passivo"), ("Quantification Cycle Method", "Metodo di quantificazione"), ("Block Type", "Blocco")] {
        if let Some(v) = e.chiave(&[k]) {
            l.info.push((nome.into(), v.to_string()));
        }
    }
    if let Some(q) = &quando {
        l.info.push(("Fine della corsa".into(), q.clone()));
    }
    l.avvisi.push("Le curve ciclo per ciclo (amplificazione, multicomponente, fusione) non vengono copiate: restano nel file dello strumento.".into());

    // I pozzetti e i bersagli: da Sample Setup, altrimenti dai risultati.
    let base = e.sezione("Sample Setup").filter(|s| !s.righe.is_empty()).unwrap_or(risultati);
    let mut voci: Vec<Voce> = Vec::new();
    for r in &base.righe {
        let m = base.riga(r);
        let Some(well) = m.get("Well").and_then(|w| id_pozzetto(w)) else { continue };
        let colori = |m: &HashMap<&str, &str>| -> Vec<(String, String)> {
            [("Sample Color", "sample color"), ("Biogroup Color", "biogroup color"), ("Target Color", "target color")].iter().filter_map(|(k, n)| pieno(m, k).map(|v| (n.to_string(), v))).collect()
        };
        if genotipi {
            let snp = pieno(&m, "SNP Assay Name").ok_or_else(|| format!("manca il nome dell'SNP nel pozzetto {well}"))?;
            for a in ["Allele1", "Allele2"] {
                let nome = pieno(&m, &format!("{a} Name")).ok_or_else(|| format!("manca il nome dell'allele nel pozzetto {well}"))?;
                voci.push(Voce {
                    well,
                    bersaglio: format!("{snp}-{nome}"),
                    campione: pieno(&m, "Sample Name").unwrap_or_default(),
                    reporter: pieno(&m, &format!("{a} Reporter")),
                    posizione: pieno(&m, "Well Position"),
                    quencher: pieno(&m, "Quencher").or_else(|| pieno(&m, &format!("{a} Quencher"))),
                    task: pieno(&m, "Task").unwrap_or_default(),
                    gruppo: pieno(&m, "Biogroup Name"),
                    colori: colori(&m),
                });
            }
        } else if let Some(t) = pieno(&m, "Target Name") {
            voci.push(Voce {
                well,
                bersaglio: t,
                campione: pieno(&m, "Sample Name").unwrap_or_default(),
                reporter: pieno(&m, "Reporter"),
                posizione: pieno(&m, "Well Position"),
                quencher: pieno(&m, "Quencher"),
                task: pieno(&m, "Task").unwrap_or_default(),
                gruppo: pieno(&m, "Biogroup Name"),
                colori: colori(&m),
            });
        }
    }

    // I risultati: per pozzetto e bersaglio (con l'allele per il genotipo).
    let ct_col = colonna_ct(&risultati.colonne).ok_or("nei risultati non si trova la colonna del Ct")?;
    let ct_pref = maiuscola(&ct_col);
    let prefissi: Vec<String> = if genotipi {
        risultati.colonne.iter().filter_map(|c| c.strip_suffix(&format!(" {ct_pref}")).filter(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_')).map(|p| format!("{p} "))).collect()
    } else {
        vec![String::new()]
    };
    let ct_key = ct_col.clone();
    let chiave_bersaglio = if genotipi { "SNP Assay Name" } else { "Target Name" };
    let mut esiti: HashMap<(i64, String), HashMap<String, String>> = HashMap::new();
    for r in &risultati.righe {
        let m = risultati.riga(r);
        let Some(well) = m.get("Well").and_then(|w| id_pozzetto(w)) else { continue };
        let Some(t) = pieno(&m, chiave_bersaglio) else { continue };
        for p in &prefissi {
            let k = format!("{t}{}", if p.is_empty() { String::new() } else { format!("-{p}") }).replace(' ', "");
            let mut riga: HashMap<String, String> = HashMap::new();
            for (c, v) in &m {
                // I valori dell'allele per intero con il prefisso tolto; gli altri così come sono.
                let nome = if !p.is_empty() { c.strip_prefix(p.as_str()).unwrap_or(c) } else { c };
                if !v.trim().is_empty() {
                    riga.entry(nome.to_string()).or_insert_with(|| v.trim().to_string());
                }
            }
            esiti.insert((well, k), riga);
        }
    }

    // I cicli totali dall'amplificazione.
    let mut cicli: HashMap<(i64, String), f64> = HashMap::new();
    if let Some(a) = e.sezione("Amplification Data") {
        for r in &a.righe {
            let m = a.riga(r);
            let (Some(w), Some(t), Some(c)) = (m.get("Well").and_then(|w| id_pozzetto(w)), pieno(&m, "Target Name"), m.get("Cycle").and_then(|c| numero(c))) else { continue };
            let x = cicli.entry((w, t)).or_insert(c);
            if c > *x {
                *x = c;
            }
        }
    }

    let blocco = e.chiave(&["Block Type"]).unwrap_or("").to_string();
    let n_pozzetti = ["96", "384"].iter().find(|n| blocco.contains(*n)).and_then(|n| n.parse::<u32>().ok());
    let referenza_bersaglio = e.meta_risultati.iter().find(|(k, _)| k == "Endogenous Control").map(|(_, v)| v.clone());
    let referenza_campione = e.meta_risultati.iter().find(|(k, _)| k == "Reference Sample").map(|(_, v)| v.clone());

    let mut gruppi: Vec<Gruppo> = Vec::new();
    // (indice gruppo, indice misura) → i valori del risultato, per i calcoli.
    let mut righe_risultato: Vec<((usize, usize), String, String, String, HashMap<String, String>)> = Vec::new();
    for v in &voci {
        let chiave = v.bersaglio.replace(' ', "");
        let Some(r) = esiti.get(&(v.well, chiave)) else { continue };
        let gi = match gruppi.iter().position(|g| g.titolo == v.bersaglio) {
            Some(i) => i,
            None => {
                let mut g = Gruppo { titolo: v.bersaglio.clone(), quando: quando.clone(), pozzetti: n_pozzetti, ..Default::default() };
                if let Some(a) = e.chiave(&["Experiment User Name"]) {
                    g.dati.push(("analyst".into(), a.to_string()));
                }
                g.dati.push(("experimental data identifier".into(), e.chiave(&["Experiment Name"]).unwrap_or("N/A").to_string()));
                g.dati.push(("experiment type".into(), tipo_asm.to_string()));
                g.dati.push(("container type".into(), "PCR reaction block".into()));
                if let Some(vol) = volume_pozzetto(&blocco) {
                    g.dati.push(("well volume".into(), format!("{vol}")));
                }
                gruppi.push(g);
                gruppi.len() - 1
            }
        };
        let num = |k: &str| r.get(k).and_then(|t| numero(t));
        let mut m = Misura {
            campione: if v.campione.is_empty() { "N/A".into() } else { v.campione.clone() },
            pozzetto: v.posizione.clone().or_else(|| Some(v.well.to_string())),
            luogo: Some(v.well.to_string()),
            bersaglio: Some(v.bersaglio.clone()),
            quando: quando.clone(),
            ..Default::default()
        };
        if let Some(rl) = ruolo_da_task(&v.task) {
            m.ruolo = rl;
            m.ruolo_dal_file = true;
        }
        // Impostazioni (documento di controllo).
        if let Some(c) = cicli.get(&(v.well, v.bersaglio.clone())) {
            m.valori.push(Valore::num("total cycle number setting", *c, "", Posto::Impostazione));
        }
        if let Some(c) = e.chiave(&["Chemistry"]) {
            m.valori.push(Valore::testo("qPCR detection chemistry", c, Posto::Impostazione));
        }
        if let Some(q) = &v.quencher {
            m.valori.push(Valore::testo("quencher dye setting", q, Posto::Impostazione));
        }
        if let Some(q) = &v.reporter {
            m.valori.push(Valore::testo("reporter dye setting", q, Posto::Impostazione));
        }
        if let Some(q) = e.chiave(&["Passive Reference"]) {
            m.valori.push(Valore::testo("passive reference dye setting", q, Posto::Impostazione));
        }
        if let Some(q) = e.chiave(&["Quantification Cycle Method"]) {
            m.valori.push(Valore::testo("measurement method identifier", q, Posto::Impostazione));
        }
        // Campione.
        if let Some(g) = &v.gruppo {
            m.valori.push(Valore::testo("group identifier", g, Posto::CampioneAltro));
        }
        m.valori.push(Valore { chiave: "well identifier".into(), dato: Dato::Numero(v.well as f64), unita: String::new(), posto: Posto::CampioneAltro });
        for (k, c) in &v.colori {
            m.valori.push(Valore::testo(k, c, Posto::CampioneAltro));
        }
        // Dati elaborati.
        let ct_cella = r.get(&ct_key).cloned().unwrap_or_default();
        match numero(&ct_cella) {
            Some(x) => m.valori.push(Valore::num("cycle threshold result (qPCR)", x, "", Posto::Elaborato)),
            None if !ct_cella.is_empty() => m.valori.push(Valore { chiave: "cycle threshold result (qPCR)".into(), dato: Dato::Assente(ct_cella.clone()), unita: String::new(), posto: Posto::Elaborato }),
            None => {}
        }
        if let Some(x) = num("Rn") {
            m.valori.push(Valore::num("normalized reporter result", x, "", Posto::Elaborato));
        }
        if let Some(x) = num("Delta Rn") {
            m.valori.push(Valore::num("baseline corrected reporter result", x, "", Posto::Elaborato));
        }
        if let Some(t) = r.get("Call") {
            m.valori.push(Valore::testo("genotyping determination result", t, Posto::Elaborato));
        }
        // Elaborazione.
        let soglia = format!("{ct_pref} Threshold");
        m.valori.push(match num(&soglia) {
            Some(x) => Valore::num("cycle threshold value setting (qPCR)", x, "", Posto::Elaborazione),
            None => Valore { chiave: "cycle threshold value setting (qPCR)".into(), dato: Dato::Assente(String::new()), unita: String::new(), posto: Posto::Elaborazione },
        });
        let si_no = |t: &str| match t.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "si" | "sì" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        };
        if let Some(b) = r.get(&format!("Automatic {ct_pref} Threshold")).and_then(|t| si_no(t)) {
            m.valori.push(Valore::bool("automatic cycle threshold enabled setting", b, Posto::Elaborazione));
        }
        if let Some(b) = r.get("Automatic Baseline").and_then(|t| si_no(t)) {
            m.valori.push(Valore::bool("automatic baseline determination enabled setting", b, Posto::Elaborazione));
        }
        if let Some(x) = num("Baseline Start") {
            m.valori.push(Valore::num("baseline determination start cycle setting", x, "#", Posto::Elaborazione));
        }
        if let Some(x) = num("Baseline End") {
            m.valori.push(Valore::num("baseline determination end cycle setting", x, "#", Posto::Elaborazione));
        }
        if let Some(x) = num("Threshold Value") {
            m.valori.push(Valore::num("genotyping determination method setting", x, "", Posto::Elaborazione));
        }
        // Il resto dello strumento (qualità, esclusioni, allarmi) resta come nota della misura.
        let usate: [&str; 3] = ["Rn", "Delta Rn", "Call"];
        let calcolati_dal_file = ["Quantity", "Quantity Mean", "Quantity SD", "Ct Mean", "Ct SD", "CT Mean", "CT SD", "Crt Mean", "Crt SD", "Cq Mean", "Cq SD", "Delta Ct Mean", "Delta Ct SE", "Delta Delta Ct", "RQ", "RQ Min", "RQ Max", "Rn Mean", "Rn SD", "Y-Intercept", "R(superscript 2)", "Slope", "Efficiency", "Amp Score", "Cq Conf"];
        let ignorate = ["Well", "Well Position", "Sample Name", "Target Name", "SNP Assay Name", "Task", "Reporter", "Quencher", "Automatic Baseline", "Baseline Start", "Baseline End", "Threshold Value"];
        let mut resto: Vec<(&String, &String)> = r.iter().filter(|(k, _)| !usate.contains(&k.as_str()) && !calcolati_dal_file.contains(&k.as_str()) && !ignorate.contains(&k.as_str()) && **k != ct_key && **k != soglia && !k.starts_with("Automatic ") && !k.ends_with(&format!(" {ct_pref}"))).collect();
        resto.sort();
        for (k, t) in resto {
            m.valori.push(match numero(t) {
                Some(x) => Valore { chiave: k.clone(), dato: Dato::Numero(x), unita: String::new(), posto: Posto::Altro },
                None => Valore::testo(k, t, Posto::Altro),
            });
        }
        let mi = gruppi[gi].misure.len();
        righe_risultato.push(((gi, mi), v.campione.clone(), v.bersaglio.clone(), v.task.clone(), r.clone()));
        gruppi[gi].misure.push(m);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono risultati per pozzetto".into());
    }
    for g in gruppi.iter_mut() {
        let t = g.titolo.clone();
        g.titolo = t;
    }
    // Gli identificativi, nell'ordine dei gruppi.
    for g in gruppi.iter_mut() {
        for m in g.misure.iter_mut() {
            m.id = l.id();
        }
    }
    let id_di = |gi: usize, mi: usize| gruppi[gi].misure[mi].id.clone();

    // Quello che lo strumento calcola sulle repliche: media, deviazione, RQ… con le misure da cui viene.
    let stessi = |campione: &str, bersaglio: &str| -> Vec<String> { righe_risultato.iter().filter(|r| r.1 == campione && r.2 == bersaglio).map(|r| id_di(r.0 .0, r.0 .1)).collect() };
    let mut fatti: Vec<(String, String, String, String)> = Vec::new();
    let mut calcolati: Vec<Calcolato> = Vec::new();
    let aggiungi = |nome: &str, chiave: &str, da: &str, sorgenti: Vec<String>, valore: f64, calcolati: &mut Vec<Calcolato>, fatti: &mut Vec<(String, String, String, String)>, k1: &str, k2: &str| {
        let k = (nome.to_string(), k1.to_string(), k2.to_string(), format!("{chiave}:{valore}"));
        if fatti.contains(&k) || sorgenti.is_empty() {
            return;
        }
        fatti.push(k);
        calcolati.push(Calcolato { nome: nome.to_string(), valore, unita: String::new(), sorgenti, da: da.to_string(), descrizione: String::new() });
    };
    let ct_nome = "cycle threshold result";
    let rn_nome = "normalized reporter result";
    for (pos, campione, bersaglio, task, r) in &righe_risultato {
        let proprio = vec![id_di(pos.0, pos.1)];
        let val = |k: &str| r.get(k).and_then(|t| numero(t));
        let del_gruppo = stessi(campione, bersaglio);
        let ref_stesso = referenza_bersaglio.as_ref().map(|t| stessi(campione, t)).unwrap_or_default();
        let ref_campione: Vec<String> = match (&referenza_campione, &referenza_bersaglio) {
            (Some(rc), Some(rt)) => stessi(rc, bersaglio).into_iter().chain(stessi(rc, rt)).collect(),
            _ => Vec::new(),
        };
        let per_bersaglio: Vec<String> = righe_risultato.iter().filter(|x| x.2 == *bersaglio && x.3 == *task).map(|x| id_di(x.0 .0, x.0 .1)).collect();
        for (nome, col, da) in [("quantity", "Quantity", ct_nome), ("amplification score", "Amp Score", ct_nome), ("cq confidence", "Cq Conf", ct_nome)] {
            if let Some(x) = val(col) {
                aggiungi(nome, col, da, proprio.clone(), x, &mut calcolati, &mut fatti, &id_di(pos.0, pos.1), "");
            }
        }
        for (nome, col) in [("y intercept", "Y-Intercept"), ("slope", "Slope"), ("r^2", "R(superscript 2)"), ("efficiency", "Efficiency")] {
            if let Some(x) = val(col) {
                aggiungi(nome, col, ct_nome, per_bersaglio.clone(), x, &mut calcolati, &mut fatti, bersaglio, task);
            }
        }
        let mean = format!("{ct_pref} Mean");
        let sd = format!("{ct_pref} SD");
        let dct_mean = format!("Delta {ct_pref} Mean");
        let dct_se = format!("Delta {ct_pref} SE");
        let ddct = format!("Delta Delta {ct_pref}");
        let con_ref: Vec<String> = del_gruppo.iter().cloned().chain(ref_stesso.iter().cloned()).collect();
        let con_ref_campione: Vec<String> = con_ref.iter().cloned().chain(ref_campione.iter().cloned()).collect();
        let voci_calcolo: Vec<(&str, String, &str, &Vec<String>)> = vec![
            ("quantity mean", "Quantity Mean".into(), ct_nome, &del_gruppo),
            ("quantity sd", "Quantity SD".into(), ct_nome, &del_gruppo),
            ("ct mean", mean.clone(), ct_nome, &del_gruppo),
            ("ct sd", sd.clone(), ct_nome, &del_gruppo),
            ("rn mean", "Rn Mean".into(), rn_nome, &del_gruppo),
            ("rn sd", "Rn SD".into(), rn_nome, &del_gruppo),
            ("delta ct mean", dct_mean.clone(), ct_nome, &con_ref),
            ("delta ct se", dct_se.clone(), ct_nome, &con_ref),
            ("delta delta ct", ddct.clone(), ct_nome, &con_ref_campione),
            ("rq", "RQ".into(), ct_nome, &con_ref_campione),
            ("rq min", "RQ Min".into(), ct_nome, &con_ref_campione),
            ("rq max", "RQ Max".into(), ct_nome, &con_ref_campione),
        ];
        for (nome, col, da, sorg) in voci_calcolo {
            if let Some(x) = val(&col) {
                aggiungi(nome, &col, da, sorg.clone(), x, &mut calcolati, &mut fatti, campione, bersaglio);
            }
        }
    }
    l.calcolati = calcolati;
    l.gruppi = gruppi;
    Ok(())
}
