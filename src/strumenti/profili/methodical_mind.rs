//! MSD Methodical Mind: il lettore di luminescenza a piastre di Meso Scale Discovery. L'export è un
//! TXT con le righe d'intestazione («Barcode1 :», «Read Time:»…), la riga «==========Data==========» e
//! la tabella dei valori (righe A-H, colonne 1-12) con a volte più righe per ogni riga di pozzetti
//! (una per punto, nelle piastre multi-spot); poi le sezioni di elaborazione, che non servono. Se la
//! piastra è letta in più tabelle (una per punto) il file ripete l'intestazione da «Stack ID».

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

use super::serie::Serie;

pub const PROFILO: Profilo = Profilo {
    id: "methodical_mind",
    nome: "MSD Methodical Mind",
    produttore: "Meso Scale Discovery",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["txt"],
    riconosci,
    leggi,
};

/// «==========Data==========»: un titolo di sezione è fatto di uguali, il nome e altri uguali.
fn e_titolo(r: &str, nome: &str) -> bool {
    let r = r.trim_end();
    let s = r.trim_start_matches('=');
    s.len() < r.len() && s.strip_prefix(nome).is_some_and(|x| !x.is_empty() && x.bytes().all(|b| b == b'='))
}

fn riconosci(f: &Fonte) -> u8 {
    let mut intestazione = false;
    for r in f.testo.lines().take(50) {
        if e_titolo(r, "Data") {
            return if intestazione { 95 } else { 60 };
        }
        if r.starts_with("Stack ID") || r.starts_with("Barcode1") {
            intestazione = true;
        }
    }
    0
}

/// Una tabella di dati con le righe nominate come i pozzetti.
struct Tavola {
    /// I nomi delle colonne (senza spazi) che hanno almeno un valore.
    colonne: Vec<String>,
    /// L'etichetta di riga (le righe senza etichetta prendono quella sopra) e le celle di ogni colonna.
    righe: Vec<(String, Vec<Option<String>>)>,
}

struct Blocco {
    /// Le righe d'intestazione «chiave: valore» (solo con un valore), nell'ordine del file.
    testata: Vec<(String, String)>,
    tavola: Tavola,
}

fn valida_etichetta(e: &str) -> bool {
    e.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn leggi_tavola(righe: &[&str]) -> Result<Tavola, String> {
    // Le righe vuote le salta chi legge il CSV; quelle di soli tab no (il tab è il separatore).
    let utili: Vec<&&str> = righe.iter().filter(|r| r.contains('\t') || !r.trim().is_empty()).collect();
    let testa: Vec<&str> = utili.first().ok_or("manca la tabella dei dati della piastra")?.split('\t').collect();
    let celle: Vec<Vec<&str>> = utili[1..].iter().map(|r| r.split('\t').collect()).collect();
    let n = testa.len();
    // Una colonna senza nemmeno un valore (di solito quella dopo l'ultimo tab) non conta.
    let piena = |c: usize| celle.iter().any(|r| r.get(c).is_some_and(|v| !v.is_empty()));
    let tenute: Vec<usize> = (1..n).filter(|c| piena(*c)).collect();
    let colonne = tenute.iter().map(|c| testa[*c].trim().to_string()).collect();
    let mut sopra = String::new();
    let mut out = Vec::new();
    for r in &celle {
        let e = r.first().map(|s| s.trim()).unwrap_or("");
        if !e.is_empty() {
            sopra = e.to_string();
        }
        out.push((sopra.clone(), tenute.iter().map(|c| r.get(*c).filter(|v| !v.is_empty()).map(|v| v.to_string())).collect()));
    }
    Ok(Tavola { colonne, righe: out })
}

/// Le piastre del file: ognuna con la sua intestazione e la sua tabella. Dalla seconda in poi
/// l'intestazione parte da «Stack ID» (la riga «FileName» c'è solo la prima volta). Le cifre della
/// legenda dei punti («Spot / Legend») servono a controllare quante tabelle devono esserci.
fn blocchi(testo: &str) -> Result<(Vec<Blocco>, Vec<u64>), String> {
    let linee: Vec<&str> = testo.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut i = 0;
    let mut out = Vec::new();
    let mut punti = Vec::new();
    while i < linee.len() {
        let inizio = i;
        while i < linee.len() && !e_titolo(linee[i], "Data") {
            i += 1;
        }
        let mut testata: Vec<&str> = linee[inizio..i].to_vec();
        punti.clear();
        if let Some(k) = (0..testata.len()).find(|k| testata[*k].starts_with("Spot") && testata.get(k + 1).is_some_and(|n| n.starts_with("Legend"))) {
            let fine = testata.len();
            let legenda = testata[k..fine - 1].join("\n");
            let mut cifre = String::new();
            for c in legenda.chars().chain(std::iter::once(' ')) {
                if c.is_ascii_digit() {
                    cifre.push(c);
                } else if !cifre.is_empty() {
                    punti.extend(cifre.parse::<u64>());
                    cifre.clear();
                }
            }
            punti.sort_unstable();
            testata = [&testata[..k], &testata[fine - 1..]].concat();
        }
        let mut coppie: Vec<(String, String)> = Vec::new();
        for r in testata.iter().filter(|r| !r.contains("Digital_Signature")) {
            let r = r.replace("\\t", "\t");
            let Some((k, v)) = r.split_once(':') else { continue };
            let (k, v) = (k.trim(), v.trim());
            if v.is_empty() {
                continue;
            }
            match coppie.iter_mut().find(|(a, _)| a == k) {
                Some(c) => c.1 = v.to_string(),
                None => coppie.push((k.to_string(), v.to_string())),
            }
        }
        // La riga «Data» e la tabella, fino alla sezione dopo.
        i = (i + 1).min(linee.len());
        let da = i;
        while i < linee.len() && !linee[i].starts_with("===") {
            i += 1;
        }
        let tavola = leggi_tavola(&linee[da..i])?;
        out.push(Blocco { testata: coppie, tavola });
        while i < linee.len() && !linee[i].starts_with("Stack ID") {
            i += 1;
        }
    }
    if out.is_empty() {
        return Err("manca la tabella dei dati della piastra".into());
    }
    Ok((out, punti))
}

/// I valori che si trovano in questi campi dell'intestazione, in ordine di file: numeri se lo sono.
fn campi(h: &Serie, nomi: &[&str], chiavi: &[String]) -> Vec<(String, Dato)> {
    chiavi
        .iter()
        .filter(|k| nomi.contains(&k.as_str()))
        .filter_map(|k| h.grezzo(k).filter(|v| !v.trim().is_empty()).map(|v| (k.clone(), numero(v).map_or_else(|| Dato::Testo(v.trim().to_string()), Dato::Numero))))
        .collect()
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let (piastre, punti) = blocchi(&f.testo)?;
    if !punti.is_empty() && piastre.len() > 1 && piastre.len() != punti.len() {
        return Err(format!("la legenda degli spot ne dice {} ma nel file ci sono {} tabelle di dati", punti.len(), piastre.len()));
    }
    let mancante = |k: &str| format!("nell'intestazione del file manca «{k}»");

    // Le informazioni comuni a tutte le misure di una piastra (campione, dispositivo, altro, ora,
    // analista) sono quelle dell'ultima tabella con il suo codice, come fa allotropy.
    struct Comune {
        piastra: String,
        quando: Option<String>,
        n_pozzetti: u32,
        analista: Option<String>,
        dispositivo: Vec<(String, Dato)>,
        campione: Vec<(String, Dato)>,
        altro: Vec<(String, Dato)>,
    }
    let mut comuni: Vec<Comune> = Vec::new();
    // Un gruppo per piastra e per spot (una tabella per spot, o più righe per ogni riga di pozzetti):
    // così la mappa di ogni spot ha i suoi valori.
    let mut gruppi: Vec<((String, String), Gruppo)> = Vec::new();
    for (n, p) in piastre.iter().enumerate() {
        let chiavi: Vec<String> = p.testata.iter().map(|(k, _)| k.clone()).collect();
        let celle: Vec<Option<String>> = p.testata.iter().map(|(_, v)| Some(v.clone())).collect();
        let h = Serie::nuova(&chiavi, &celle);
        if n == 0 {
            // Le informazioni dello strumento le dice la prima intestazione.
            let nome_file = h.testo_pieno(&["FileName"]).ok_or_else(|| mancante("FileName"))?;
            let versione = h.testo_pieno(&["Version"]).ok_or_else(|| mancante("Version"))?;
            let modello = h.testo_pieno(&["Model"]).ok_or_else(|| mancante("Model"))?;
            let seriale = h.testo_pieno(&["Serial #"]).ok_or_else(|| mancante("Serial #"))?;
            l.strumento.produttore = "Meso Scale Discovery".into();
            l.strumento.modello = modello;
            l.strumento.seriale = seriale;
            l.strumento.software = versione.clone();
            l.strumento.versione = versione;
            l.info.push(("File dello strumento".into(), nome_file));
        }
        let barcode = h.testo_pieno(&["Barcode1"]).ok_or_else(|| mancante("Barcode1"))?;
        let piastra = barcode.trim_matches(['<', '>']).to_string();
        let spot = h.testo_pieno(&["SpotID"]).and_then(|s| numero(&s)).filter(|v| v.fract() == 0.0 && *v != 0.0).map(|v| (v as i64).to_string());
        let lettura = h.testo_pieno(&["Read Time"]).ok_or_else(|| mancante("Read Time"))?;
        let analista = h.testo_pieno(&["User"]);
        let campione = campi(&h, &["Barcode2", "Barcode3", "Plate #", "Stack ID"], &chiavi);
        let dispositivo = campi(&h, &["Orient", "Spots Per Well", "Det Param"], &chiavi);
        let altro = h.non_letti(&|k| matches!(k, "Type" | "Wells Per Col" | "Wells Per Row"));

        // Le etichette di riga che sono pozzetti, nell'ordine in cui compaiono.
        let mut etichette: Vec<&str> = Vec::new();
        for (e, _) in &p.tavola.righe {
            if valida_etichetta(e) && !etichette.contains(&e.as_str()) {
                etichette.push(e);
            }
        }
        let n_pozzetti = (etichette.len() * p.tavola.colonne.len()) as u32;
        let comune = Comune { piastra: piastra.clone(), quando: tempo::iso(&lettura, false), n_pozzetti, analista, dispositivo, campione, altro };
        match comuni.iter().position(|c| c.piastra == piastra) {
            Some(i) => comuni[i] = comune,
            None => comuni.push(comune),
        }

        for e in &etichette {
            for (indice, (_, riga)) in p.tavola.righe.iter().filter(|(x, _)| x == e).enumerate() {
                for (c, cella) in riga.iter().enumerate() {
                    let Some(cella) = cella.as_deref().map(str::trim).filter(|s| !s.is_empty()) else { continue };
                    let pozzetto = format!("{e}{}", p.tavola.colonne[c]);
                    let v = numero(cella).ok_or_else(|| format!("la cella {pozzetto} non è un numero: «{cella}»"))?;
                    let luogo = spot.clone().unwrap_or_else(|| (indice + 1).to_string());
                    let mut m = Misura {
                        id: l.id(),
                        campione: format!("{piastra}_{pozzetto}"),
                        pozzetto: Some(pozzetto),
                        piastra: Some(piastra.clone()),
                        luogo: Some(luogo.clone()),
                        ..Default::default()
                    };
                    m.valori.push(Valore::testo("detection type", "Luminescence", Posto::Impostazione));
                    m.valori.push(Valore::num("luminescence", v, "RLU", Posto::Misura));
                    let chiave = (piastra.clone(), luogo);
                    match gruppi.iter().position(|(k, _)| *k == chiave) {
                        Some(i) => gruppi[i].1.misure.push(m),
                        None => gruppi.push((chiave, Gruppo { titolo: piastra.clone(), piastra: Some(piastra.clone()), misure: vec![m], ..Default::default() })),
                    }
                }
            }
        }
    }
    let gruppi: Vec<((String, String), Gruppo)> = {
        let spot_di = |p: &str| gruppi.iter().filter(|((c, _), _)| c == p).count();
        let titoli: Vec<String> = gruppi.iter().map(|((p, s), _)| if spot_di(p) > 1 { format!("{p} · spot {s}") } else { p.clone() }).collect();
        gruppi
            .into_iter()
            .zip(titoli)
            .map(|((k, mut g), titolo)| {
                let c = comuni.iter().find(|c| c.piastra == k.0).expect("ogni gruppo ha una piastra");
                g.titolo = titolo;
                g.quando = c.quando.clone();
                g.pozzetti = Some(c.n_pozzetti);
                if let Some(a) = &c.analista {
                    g.dati.push(("analyst".into(), a.clone()));
                }
                for m in g.misure.iter_mut() {
                    for (k, d) in &c.dispositivo {
                        m.valori.push(Valore { chiave: k.clone(), dato: d.clone(), unita: String::new(), posto: Posto::ImpostazioneAltro });
                    }
                    for (k, d) in &c.campione {
                        m.valori.push(Valore { chiave: k.clone(), dato: d.clone(), unita: String::new(), posto: Posto::CampioneAltro });
                    }
                    for (k, d) in &c.altro {
                        m.valori.push(Valore { chiave: k.clone(), dato: d.clone(), unita: String::new(), posto: Posto::Altro });
                    }
                }
                (k, g)
            })
            .collect()
    };
    if gruppi.is_empty() {
        return Err("nel file non ci sono valori di luminescenza".into());
    }
    l.gruppi.extend(gruppi.into_iter().map(|(_, g)| g));
    l.principale = "luminescence".into();
    l.unita = "RLU".into();
    Ok(())
}
