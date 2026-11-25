//! BMG Labtech MARS (PHERAstar, CLARIOstar, FLUOstar…): il CSV di un'unica lettura. In alto le righe
//! d'intestazione «Chiave: valore» (utente, percorso, nome del test, data, ora, ID1-ID3), il tipo di
//! lettura (assorbanza, fluorescenza, luminescenza) e la riga «Raw Data (filtri)»; poi la tabella dei
//! valori: righe A-P, colonne 1-48, con le celle vuote dove il pozzetto non è stato letto.

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "bmg_mars",
    nome: "BMG Labtech MARS",
    produttore: "BMG Labtech",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["csv"],
    riconosci,
    leggi,
};

/// La riga «Raw Data (…)»: all'inizio, con o senza una virgola prima.
fn e_raw(r: &str) -> bool {
    r.trim().trim_start_matches(',').trim_start().starts_with("Raw Data")
}

fn riconosci(f: &Fonte) -> u8 {
    let mut chiave_valore = false;
    let mut test = false;
    for r in f.testo.lines().take(40) {
        let t = r.trim_start_matches('\u{feff}');
        if e_raw(t) {
            return if chiave_valore && test { 95 } else if chiave_valore { 55 } else { 0 };
        }
        if let Some((k, v)) = t.split_once(':') {
            if !k.trim().is_empty() && v.starts_with([' ', '\t']) {
                chiave_valore = true;
            }
        }
        if t.starts_with("Test name:") {
            test = true;
        }
    }
    0
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modo {
    Assorbanza,
    Fluorescenza,
    Luminescenza,
}

impl Modo {
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

/// I filtri della riga «Raw Data»: l'emissione (o la lunghezza d'onda) e l'eccitazione.
fn filtri(riga: &str, modo: Modo) -> Result<(Option<f64>, Option<f64>, Option<String>), String> {
    let errore = || format!("la riga «{}» non è in un formato che Cedro conosce", riga.trim().trim_end_matches(','));
    let dopo = riga.trim().trim_start_matches(',').trim_start().strip_prefix("Raw Data").ok_or_else(errore)?;
    let dentro = dopo.trim_start().strip_prefix('(').and_then(|r| r.split_once(')')).map(|(a, _)| a.trim()).ok_or_else(errore)?;
    if dentro == "No filter" {
        return Ok((None, None, None));
    }
    // Un numero senza segno né esponente: «485», «12.5».
    let n = |s: &str| {
        let s = s.trim();
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'.') && s.bytes().filter(|b| *b == b'.').count() <= 1 && !s.starts_with('.')).then(|| s.parse::<f64>().ok()).flatten()
    };
    if let Some((a, b)) = dentro.split_once('/') {
        return match (n(a), n(b)) {
            (Some(ex), Some(em)) => Ok((Some(em), Some(ex), None)),
            _ => Err(errore()),
        };
    }
    let parti: Vec<&str> = dentro.split_whitespace().collect();
    match parti.as_slice() {
        [a] => n(a).map(|em| (Some(em), None, None)).ok_or_else(errore),
        // Fluorescenza: eccitazione ed emissione. Assorbanza: la lunghezza d'onda e un secondo numero
        // (l'indice della misura) che resta scritto com'è.
        [a, b] => match (n(a), n(b)) {
            (Some(x), Some(_)) if modo == Modo::Assorbanza => Ok((Some(x), None, Some(format!("{a} {b}")))),
            (Some(ex), Some(em)) => Ok((Some(em), Some(ex), None)),
            _ => Err(errore()),
        },
        _ => Err(errore()),
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let testo = f.testo.trim_start_matches('\u{feff}');
    let linee: Vec<&str> = testo.lines().collect();
    let fine_intestazione = linee.iter().position(|r| e_raw(r)).ok_or("nel file non c'è la riga «Raw Data»")?;
    let intestazione = &linee[..=fine_intestazione];

    // Le righe d'intestazione: una virgola finale non conta; ogni cella «Chiave: valore» è una voce
    // (le chiavi in maiuscolo), le altre (il tipo di lettura) non hanno valore.
    let mut voci: Vec<(String, Option<String>)> = Vec::new();
    for r in intestazione {
        let r = r.trim_end();
        let r = r.strip_suffix(',').map(str::trim_end).unwrap_or(r);
        for cella in crate::textconv::parse_csv(r, ',').into_iter().flatten() {
            let cella = cella.trim();
            if cella.is_empty() {
                continue;
            }
            let (k, v) = match cella.split_once(": ") {
                Some((k, v)) => (k.trim().to_uppercase(), Some(v.trim().to_string()).filter(|v| !v.is_empty())),
                None => (cella.to_uppercase(), None),
            };
            match voci.iter_mut().find(|(a, _)| *a == k) {
                Some(x) => x.1 = v,
                None => voci.push((k, v)),
            }
        }
    }
    if !voci.iter().any(|(_, v)| v.is_some()) {
        return Err("non riesco a leggere l'intestazione: nessuna riga «Chiave: valore»".into());
    }
    let prendi = |k: &str| voci.iter().find(|(a, _)| a == k).and_then(|(_, v)| v.as_deref());
    let serve = |k: &str| prendi(k).ok_or_else(|| format!("nell'intestazione del file manca «{}»", k.to_lowercase()));

    // Il tipo di lettura lo dice il testo dell'intestazione: una sola delle tre parole.
    let minuscolo = intestazione.join("\n").to_lowercase();
    let modi: Vec<Modo> = [("absorbance", Modo::Assorbanza), ("fluorescence", Modo::Fluorescenza), ("luminescence", Modo::Luminescenza)].into_iter().filter(|(p, _)| minuscolo.contains(p)).map(|(_, m)| m).collect();
    let [modo] = modi.as_slice() else {
        return Err("non capisco il tipo di lettura dall'intestazione: serve una sola tra assorbanza, fluorescenza e luminescenza".into());
    };
    let modo = *modo;
    let (lunghezza, eccitazione, filtro_grezzo) = filtri(linee[fine_intestazione], modo)?;

    let utente = serve("USER")?.to_string();
    let nome_test = serve("TEST NAME")?.to_string();
    let data = serve("DATE")?;
    let ora = serve("TIME")?;
    let id1 = serve("ID1")?.to_string();
    let id2 = prendi("ID2").map(str::to_string);
    let quando = tempo::iso(&format!("{data} {ora}"), false);

    // La tabella: dopo le righe vuote (o di sole virgole), fino alla prima riga vuota.
    let e_vuota_o_virgole = |r: &str| r.chars().all(|c| c == ',' || c == '"' || c.is_whitespace());
    let mut i = fine_intestazione + 1;
    while i < linee.len() && e_vuota_o_virgole(linee[i]) {
        i += 1;
    }
    let blocco: Vec<Vec<String>> = linee[i..].iter().take_while(|r| !r.trim().is_empty()).map(|r| crate::textconv::parse_csv(r, ',').into_iter().next().unwrap_or_default()).collect();
    let (testa, righe) = blocco.split_first().ok_or("manca la tabella dei valori dopo «Raw Data»")?;
    let colonne: Vec<(usize, String)> = testa.iter().enumerate().skip(1).map(|(c, n)| (c, n.trim().to_string())).filter(|(_, n)| !n.is_empty()).collect();
    let n_pozzetti = (righe.len() * colonne.len()) as u32;

    let mut g = Gruppo { titolo: id1.clone(), piastra: Some(id1.clone()), quando, pozzetti: Some(n_pozzetti), ..Default::default() };
    g.dati.push(("analyst".into(), utente.clone()));
    g.dati.push(("experiment type".into(), nome_test.clone()));
    if let Some(d) = &id2 {
        g.dati.push(("experimental data identifier".into(), d.clone()));
    }
    // Le voci che nessuno ha usato (numero della corsa, ID3…) sono informazioni libere della lettura.
    for (k, v) in &voci {
        if matches!(k.as_str(), "USER" | "PATH" | "TEST NAME" | "DATE" | "TIME" | "ID1" | "ID2") {
            continue;
        }
        if let Some(v) = v {
            g.dati.push((format!("~{k}"), v.clone()));
        }
    }
    if let Some(r) = &filtro_grezzo {
        g.dati.push(("~RAW DATA FILTERS".into(), format!("({r})")));
    }
    for riga in righe {
        let nome_riga = riga.first().map(|s| s.trim()).unwrap_or("");
        if nome_riga.is_empty() {
            continue;
        }
        for (c, nome_colonna) in &colonne {
            let Some(cella) = riga.get(*c).map(|s| s.trim()).filter(|s| !s.is_empty()) else { continue };
            let pozzetto = format!("{nome_riga}{nome_colonna}");
            let mut m = Misura { id: l.id(), campione: format!("{id1} {pozzetto}"), pozzetto: Some(pozzetto), piastra: Some(id1.clone()), ..Default::default() };
            m.valori.push(Valore::testo("detection type", modo.concetto(), Posto::Impostazione));
            if let Some(w) = lunghezza {
                m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
            }
            if let Some(e) = eccitazione {
                m.valori.push(Valore::num("excitation wavelength setting", e, "nm", Posto::Impostazione));
            }
            m.valori.push(match numero(cella) {
                Some(v) => Valore::num(modo.concetto(), v, modo.unita(), Posto::Misura),
                None => Valore { chiave: modo.concetto().into(), dato: Dato::Assente(cella.to_string()), unita: modo.unita().into(), posto: Posto::Misura },
            });
            g.misure.push(m);
        }
    }
    if g.misure.is_empty() {
        return Err("nel file non ci sono valori".into());
    }

    l.strumento.produttore = "BMG LABTECH".into();
    l.strumento.software = "BMG MARS".into();
    l.principale = modo.concetto().into();
    l.unita = modo.unita().into();
    l.info.push(("Test".into(), nome_test));
    l.info.push(("Utente".into(), utente));
    if let Some(p) = prendi("PATH") {
        l.info.push(("Cartella dei dati".into(), p.to_string()));
    }
    l.gruppi.push(g);
    Ok(())
}
