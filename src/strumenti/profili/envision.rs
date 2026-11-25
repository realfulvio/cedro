//! PerkinElmer EnVision (EnVision Workstation): il CSV che il programma esporta dopo una lettura.
//! Una sequenza di piastre: ognuna ha «Plate information» (la piastra, il codice a barre, la
//! temperatura, l'etichetta e il detector), le «Background information» (da quali letture viene) e la
//! griglia dei valori. Le piastre dei risultati («Results for …») sono le letture; quelle dei calcoli
//! («Calculated results: Calc 1: …», con la formula) vengono dalle letture elencate nelle informazioni
//! di fondo. In fondo: informazioni dell'assay, protocollo, tipo di piastra, mappa della piastra,
//! etichette di misura con i loro filtri, strumento e versione del programma.

use std::collections::HashMap;

use crate::strumenti::fonte::{colonna, numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "perkin_elmer_envision",
    nome: "PerkinElmer EnVision",
    produttore: "PerkinElmer",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["csv"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    if f.foglio() {
        return 0;
    }
    let inizio = f.inizio(20);
    if !inizio.lines().any(|r| r.trim_start_matches('\u{feff}').trim().starts_with("Plate information")) {
        return 0;
    }
    if f.testo.contains("Exported with EnVision") {
        95
    } else if f.testo.contains("Basic assay information") {
        70
    } else {
        0
    }
}

type Riga = Vec<String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Modo {
    Assorbanza,
    Fluorescenza,
    Luminescenza,
}

impl Modo {
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

// ---------------------------------------------------------------------------------------------
// Il testo come righe di celle
// ---------------------------------------------------------------------------------------------

/// Una riga del CSV: le celle tra virgolette («="8/22/2023 10:50:28 AM"») con le virgolette doppie dentro.
fn dividi(riga: &str, sep: char) -> Riga {
    let mut out = Vec::new();
    let mut campo = String::new();
    let mut tra_virgolette = false;
    let mut it = riga.chars().peekable();
    while let Some(c) = it.next() {
        if tra_virgolette {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    campo.push('"');
                    it.next();
                } else {
                    tra_virgolette = false;
                }
            } else {
                campo.push(c);
            }
        } else if c == '"' && campo.is_empty() {
            tra_virgolette = true;
        } else if c == sep {
            out.push(std::mem::take(&mut campo));
        } else {
            campo.push(c);
        }
    }
    out.push(campo);
    out
}

/// Il separatore dei campi: la virgola, o il punto e virgola dei programmi con le impostazioni italiane.
fn separatore(testo: &str) -> char {
    let riga = testo.lines().find(|r| r.trim_start().starts_with("Plate,") || r.trim_start().starts_with("Plate;")).unwrap_or("");
    if riga.matches(';').count() > riga.matches(',').count() {
        ';'
    } else {
        ','
    }
}

/// Una riga proprio vuota (senza nemmeno le virgole).
fn vuota(r: &[String]) -> bool {
    r.len() <= 1 && r.iter().all(|c| c.trim().is_empty())
}

/// Una riga senza dati (anche se ha le virgole).
fn senza_dati(r: &[String]) -> bool {
    r.iter().all(|c| c.trim().is_empty())
}

fn cella0(r: &[String]) -> &str {
    r.first().map(|c| c.trim().trim_start_matches('\u{feff}')).unwrap_or("")
}

fn cella(r: &[String], i: usize) -> &str {
    r.get(i).map(|c| c.trim()).unwrap_or("")
}

/// Il codice a barre come lo scrive EnVision: `="1234"`, o `=""` se manca.
fn pulisci(v: &str) -> String {
    let t = v.trim();
    match t.strip_prefix("=\"") {
        Some(resto) => resto.strip_suffix('"').unwrap_or(resto).trim().to_string(),
        None => t.to_string(),
    }
}

fn e_lettera(c: &str) -> bool {
    !c.is_empty() && c.len() <= 2 && c.chars().all(|c| c.is_ascii_uppercase())
}

/// A, B… Z, AA, AB… per le righe di una piastra senza i nomi.
fn lettere(i: usize) -> String {
    let mut n = i + 1;
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Le coppie «chiave, valore» di un tratto di righe: la chiave senza i due punti, il valore è la prima cella piena dopo.
fn kv(righe: &[Riga]) -> Vec<(String, String)> {
    righe
        .iter()
        .filter_map(|r| {
            let k = cella0(r).trim_end_matches(':').trim().to_string();
            let v = r.iter().skip(1).map(|c| c.trim()).find(|c| !c.is_empty())?;
            (!k.is_empty()).then(|| (k, pulisci(v)))
        })
        .collect()
}

fn voce<'a>(v: &'a [(String, String)], k: &str) -> Option<&'a str> {
    let k = k.to_ascii_lowercase();
    v.iter().find(|(a, _)| a.to_ascii_lowercase() == k).map(|(_, b)| b.as_str()).filter(|b| !b.is_empty() && !b.eq_ignore_ascii_case("n/a"))
}

/// I blocchi di righe separati da righe vuote.
fn blocchi(righe: &[Riga]) -> Vec<&[Riga]> {
    let mut out = Vec::new();
    let mut da = None;
    for (i, r) in righe.iter().enumerate() {
        if senza_dati(r) {
            if let Some(d) = da.take() {
                out.push(&righe[d..i]);
            }
        } else if da.is_none() {
            da = Some(i);
        }
    }
    if let Some(d) = da {
        out.push(&righe[d..]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Le piastre
// ---------------------------------------------------------------------------------------------

/// Una griglia di valori: le righe con la loro lettera e le celle 1, 2, 3…
struct Griglia {
    righe: Vec<(String, Vec<String>)>,
}

/// La griglia di un blocco di righe: con l'intestazione (vuoto, 01, 02…) le righe cominciano con la
/// lettera del pozzetto (che può saltare: A, E, I, M); senza, sono solo numeri e le righe sono A, B, C…
fn griglia(blocco: &[Riga]) -> Result<Griglia, String> {
    let Some(prima) = blocco.first() else { return Err("la griglia dei valori è vuota".into()) };
    let con_testa = cella0(prima).is_empty() && numero(cella(prima, 1)) == Some(1.0) && blocco.get(1).is_some_and(|r| e_lettera(cella0(r)));
    if con_testa {
        let mut colonne = 0;
        while numero(cella(prima, colonne + 1)) == Some((colonne + 1) as f64) {
            colonne += 1;
        }
        let righe = blocco[1..].iter().take_while(|r| e_lettera(cella0(r))).map(|r| (cella0(r).to_string(), (1..=colonne).map(|k| cella(r, k).to_string()).collect())).collect();
        return Ok(Griglia { righe });
    }
    let colonne = blocco.iter().map(|r| r.iter().rposition(|c| !c.trim().is_empty()).map(|p| p + 1).unwrap_or(0)).max().unwrap_or(0);
    if colonne == 0 {
        return Err("la griglia dei valori è vuota".into());
    }
    Ok(Griglia { righe: blocco.iter().enumerate().map(|(k, r)| (lettere(k), (0..colonne).map(|j| cella(r, j).to_string()).collect())).collect() })
}

struct Sfondo {
    piastra: String,
    etichetta: String,
    misinfo: String,
}

struct Piastra {
    numero: String,
    barcode: String,
    campi: Vec<(String, String)>,
    /// L'etichetta di misura («AC HTRF Laser [Eu](1)»): c'è solo nelle piastre dei risultati.
    etichetta: Option<String>,
    misinfo: String,
    /// La formula: c'è solo nelle piastre dei calcoli.
    formula: Option<String>,
    sfondo: Vec<Sfondo>,
    titolo: String,
    griglia: Griglia,
}

fn leggi_piastre(righe: &[Riga], i: &mut usize) -> Result<Vec<Piastra>, String> {
    let n = righe.len();
    let mut out = Vec::new();
    loop {
        while *i < n && senza_dati(&righe[*i]) {
            *i += 1;
        }
        if *i >= n || !cella0(&righe[*i]).starts_with("Plate information") {
            break;
        }
        let riga_titolo = *i + 1;
        *i += 1;
        let testa = righe.get(*i).filter(|r| cella0(r) == "Plate").ok_or_else(|| format!("alla riga {riga_titolo} dopo «Plate information» manca l'intestazione «Plate,Repeat,Barcode…»"))?;
        let valori = righe.get(*i + 1).filter(|r| !senza_dati(r)).ok_or_else(|| format!("alla riga {} manca la riga con i dati della piastra", *i + 2))?;
        *i += 2;
        let campi: Vec<(String, String)> = testa.iter().enumerate().filter(|(_, k)| !k.trim().is_empty()).map(|(j, k)| (k.trim().to_string(), cella(valori, j).to_string())).collect();
        let numero_piastra = voce(&campi, "Plate").ok_or("una piastra non ha il numero")?.to_string();
        let barcode = pulisci(voce(&campi, "Barcode").unwrap_or(""));
        let etichetta = voce(&campi, "Label").map(str::to_string);
        let misinfo = voce(&campi, "Measinfo").unwrap_or("").to_string();
        let formula = voce(&campi, "Formula").map(str::to_string);
        if etichetta.is_none() && formula.is_none() {
            return Err(format!("la piastra {numero_piastra} non ha né «Label» né «Formula»: non so se è una lettura o un calcolo"));
        }

        while *i < n && senza_dati(&righe[*i]) {
            *i += 1;
        }
        let mut sfondo = Vec::new();
        if *i < n && cella0(&righe[*i]).starts_with("Background information") {
            *i += 1;
            let testa = righe.get(*i).ok_or("mancano le intestazioni delle informazioni di fondo")?;
            let (cp, ce, cm) = (colonna(testa, &["Plate"]), colonna(testa, &["Label"]), colonna(testa, &["MeasInfo"]));
            *i += 1;
            while *i < n && !senza_dati(&righe[*i]) {
                let r = &righe[*i];
                sfondo.push(Sfondo { piastra: cp.map(|c| cella(r, c)).unwrap_or("").to_string(), etichetta: ce.map(|c| cella(r, c)).unwrap_or("").to_string(), misinfo: cm.map(|c| cella(r, c)).unwrap_or("").to_string() });
                *i += 1;
            }
        }

        while *i < n && senza_dati(&righe[*i]) {
            *i += 1;
        }
        let mut titolo = String::new();
        if *i < n {
            let c = cella0(&righe[*i]);
            if c.starts_with("Results") || c.starts_with("Calculated results") {
                titolo = c.to_string();
                *i += 1;
            }
        }
        let da = *i;
        while *i < n && !vuota(&righe[*i]) && !cella0(&righe[*i]).starts_with("Plate information") && !cella0(&righe[*i]).starts_with("Basic assay information") {
            *i += 1;
        }
        let g = griglia(&righe[da..*i]).map_err(|e| format!("piastra {numero_piastra}: {e}"))?;
        out.push(Piastra { numero: numero_piastra, barcode, campi, etichetta, misinfo, formula, sfondo, titolo, griglia: g });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Le sezioni in fondo al file
// ---------------------------------------------------------------------------------------------

/// Le sezioni del file dopo le piastre: il titolo (minuscolo, senza i due punti) e le righe.
fn sezioni(righe: &[Riga]) -> Vec<(String, Vec<Riga>)> {
    let mut out: Vec<(String, Vec<Riga>)> = Vec::new();
    for r in righe {
        let c = cella0(r);
        let e_titolo = !c.is_empty() && (c.ends_with(':') || c == "Basic assay information" || c == "Protocol information") && r.iter().skip(1).all(|x| x.trim().is_empty());
        if e_titolo {
            out.push((c.trim_end_matches(':').trim().to_lowercase(), Vec::new()));
        } else if let Some(u) = out.last_mut() {
            u.1.push(r.clone());
        }
    }
    out
}

fn sez<'a>(s: &'a [(String, Vec<Riga>)], nome: &str) -> Option<&'a [Riga]> {
    s.iter().find(|(n, _)| n == nome).map(|(_, r)| r.as_slice())
}

struct Etichetta {
    nome: String,
    props: Vec<(String, String)>,
}

struct Filtro {
    nome: String,
    onda: Option<f64>,
    banda: Option<f64>,
}

/// Il numero dopo «CWL=», «BW=», «Longpass=»: i filtri esportati hanno spazi in mezzo alle cifre («BW=7 5nm»).
fn dopo(desc: &str, tag: &str) -> Option<f64> {
    let p = desc.find(tag)? + tag.len();
    let t: String = desc[p..].chars().take_while(|c| c.is_ascii_digit() || *c == ' ' || *c == '.').filter(|c| *c != ' ').collect();
    t.parse().ok()
}

fn filtro(blocco: &[Riga]) -> Filtro {
    let desc = voce(&kv(&blocco[1..]), "Description").unwrap_or("").to_string();
    let onda = dopo(&desc, "CWL=").or_else(|| dopo(&desc, "Longpass="));
    Filtro { nome: cella0(&blocco[0]).to_string(), onda, banda: dopo(&desc, "BW=") }
}

/// Il nome dell'etichetta senza il numero della ripetizione: «Alexa-488(1)» → «Alexa-488».
fn senza_indice(l: &str) -> &str {
    let t = l.trim();
    if let Some(p) = t.rfind('(') {
        if t.ends_with(')') && t[p + 1..t.len() - 1].chars().all(|c| c.is_ascii_digit()) && p > 0 {
            return t[..p].trim_end();
        }
    }
    t
}

/// Come l'etichetta si legge: dal nome (come fa allotropy) e, se non basta, da come è fatta.
fn modo_etichetta(nome: &str, e: Option<&Etichetta>) -> Modo {
    let minuscolo = nome.to_lowercase();
    let mut trovati: Vec<Modo> = Vec::new();
    if minuscolo.contains("absorbance") || minuscolo.contains("absorbtion") || nome.contains("ABS") {
        trovati.push(Modo::Assorbanza);
    }
    if minuscolo.contains("luminescence") || nome.contains("LUM") {
        trovati.push(Modo::Luminescenza);
    }
    if minuscolo.contains("fluorescence") {
        trovati.push(Modo::Fluorescenza);
    }
    if trovati.is_empty() {
        let b = nome.as_bytes();
        if b.len() >= 4 && b[0] == b'A' && b[1..4].iter().all(u8::is_ascii_digit) {
            trovati.push(Modo::Assorbanza);
        }
    }
    if trovati.len() == 1 {
        return trovati[0];
    }
    // Il nome non lo dice: com'è fatta l'etichetta. Un'etichetta che ha solo il filtro d'eccitazione o la
    // lunghezza d'onda del monocromatore, senza emissione, è un'assorbanza; con solo il tempo di misura e il
    // diaframma è una luminescenza; il resto (fluorescenza, AlphaScreen) è fluorescenza.
    if let Some(e) = e {
        let ha = |k: &str| voce(&e.props, k).is_some();
        let emissione = ha("Ems. filter") || ha("2nd ems. filter") || ha("Emission wavelength") || ha("Top mirror") || ha("Emission time");
        if !emissione && (ha("Exc. filter") || ha("Excitation wavelength")) && !ha("Excitation time") {
            return Modo::Assorbanza;
        }
        if !emissione && ha("Measurement time") && !ha("Exc. filter") && !ha("Excitation time") {
            return Modo::Luminescenza;
        }
    }
    Modo::Fluorescenza
}

fn ruolo_codice(c: &str) -> Option<Ruolo> {
    let c = c.trim();
    for (prefisso, r) in [("STD", Ruolo::Standard), ("BL", Ruolo::Bianco), ("LB", Ruolo::Bianco), ("CTL", Ruolo::Controllo), ("LC", Ruolo::Controllo), ("LH", Ruolo::Controllo), ("ZH", Ruolo::Controllo), ("ZL", Ruolo::Controllo), ("UNK", Ruolo::Campione), ("S", Ruolo::Campione)] {
        if c.starts_with(prefisso) {
            return Some(r);
        }
    }
    None
}

/// La mappa della piastra: per ogni piastra il codice di ogni pozzetto («STD1», «UNK12»).
fn mappe(righe: &[Riga]) -> HashMap<String, HashMap<(String, usize), String>> {
    let mut out: HashMap<String, HashMap<(String, usize), String>> = HashMap::new();
    let mut i = 0;
    while i < righe.len() {
        if cella0(&righe[i]) != "Plate" {
            i += 1;
            continue;
        }
        let num = righe[i].iter().skip(1).map(|c| c.trim()).find(|c| !c.is_empty()).unwrap_or("").to_string();
        i += 1;
        while i < righe.len() && cella0(&righe[i]) != "Plate" && !(cella0(&righe[i]).is_empty() && numero(cella(&righe[i], 1)) == Some(1.0)) {
            i += 1;
        }
        if i >= righe.len() || cella0(&righe[i]) == "Plate" {
            continue;
        }
        let da = i;
        i += 1;
        while i < righe.len() && e_lettera(cella0(&righe[i])) {
            i += 1;
        }
        if let Ok(g) = griglia(&righe[da..i]) {
            let m = out.entry(num).or_default();
            for (lettera, celle) in &g.righe {
                for (k, c) in celle.iter().enumerate() {
                    let c = c.trim();
                    if !c.is_empty() && c != "-" && c != "''" {
                        m.insert((lettera.clone(), k + 1), c.to_string());
                    }
                }
            }
        }
    }
    out
}

/// «0.1 s», «6.5 mm», «100%», «2»: un numero con la sua unità, o il testo com'è («0 0 0», «Laser»).
fn valore_libero(k: &str, v: &str, posto: Posto) -> Valore {
    let t = v.trim();
    let parole: Vec<&str> = t.split_whitespace().collect();
    match parole.as_slice() {
        [una] => {
            if let Some(x) = numero(una) {
                return Valore::num(k, x, if una.ends_with('%') { "%" } else { "" }, posto);
            }
        }
        [num, unita] if unita.chars().all(|c| c.is_alphabetic() || c == '/') && unita.len() <= 5 => {
            if let Some(x) = numero(num) {
                return Valore::num(k, x, unita, posto);
            }
        }
        _ => {}
    }
    Valore::testo(k, t, posto)
}

/// Le impostazioni dentro la riga delle informazioni di misura: «De=1st Ex=Top Em=Top Wdw=1 (14)».
fn token<'a>(misinfo: &'a str, k: &str) -> Option<&'a str> {
    misinfo.split_whitespace().find_map(|t| t.strip_prefix(k)).filter(|v| !v.is_empty() && *v != "N/A")
}

fn posizione(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "top" => Some("top scan position (plate reader)"),
        "bottom" | "btm" => Some("bottom scan position (plate reader)"),
        _ => None,
    }
}

struct Fatta {
    numero: String,
    etichetta: String,
    misinfo: String,
    modo: Modo,
    ids: HashMap<String, String>,
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let sep = separatore(&f.testo);
    let righe: Vec<Riga> = f.testo.lines().map(|r| dividi(r.trim_end_matches('\r'), sep)).collect();
    let mut i = 0;
    let piastre = leggi_piastre(&righe, &mut i)?;
    while i < righe.len() && senza_dati(&righe[i]) {
        i += 1;
    }
    if piastre.is_empty() {
        return Err("non trovo nessuna piastra: manca «Plate information»".into());
    }
    if i >= righe.len() || !cella0(&righe[i]).starts_with("Basic assay information") {
        let t = righe.get(i).map(|r| r.join(sep.to_string().as_str())).unwrap_or_default();
        let t: String = t.chars().take(60).collect();
        return Err(format!("alla riga {} mi aspettavo «Plate information» (la piastra successiva) o «Basic assay information», c'è «{t}»: il titolo «Plate information» manca o il file è incompleto", i + 1));
    }
    let s = sezioni(&righe[i..]);

    // Le informazioni dell'assay e del protocollo.
    let assay = sez(&s, "basic assay information").map(kv).unwrap_or_default();
    let protocollo = sez(&s, "protocol").map(kv).unwrap_or_default();
    let tipo_piastra = sez(&s, "plate type").map(kv).unwrap_or_default();
    let strumento = sez(&s, "instrument").map(kv).unwrap_or_default();
    let mappa = sez(&s, "platemap").map(mappe).unwrap_or_default();

    let etichette: Vec<Etichetta> = sez(&s, "labels").map(|r| blocchi(r).into_iter().map(|b| Etichetta { nome: cella0(&b[0]).to_string(), props: kv(&b[1..]) }).collect()).unwrap_or_default();
    let filtri: Vec<Filtro> = sez(&s, "filters").map(|r| blocchi(r).into_iter().map(filtro).collect()).unwrap_or_default();
    let trova_filtro = |nome: &str| filtri.iter().find(|f| f.nome == nome);
    let specchi: Vec<(String, Vec<(String, String)>)> = sez(&s, "mirror modules").map(|r| blocchi(r).into_iter().map(|b| (cella0(&b[0]).to_string(), kv(&b[1..]))).collect()).unwrap_or_default();
    let diaframmi: Vec<(String, Vec<(String, String)>)> = sez(&s, "aperture").map(|r| blocchi(r).into_iter().map(|b| (cella0(&b[0]).to_string(), kv(&b[1..]))).collect()).unwrap_or_default();

    // Le date: giorno prima del mese o mese prima, secondo le date del file.
    let date: Vec<&str> = piastre.iter().filter_map(|p| voce(&p.campi, "Measurement date")).collect();
    let giorno_prima = tempo::giorno_prima(date.iter().copied());
    let quando_di = |p: &Piastra| voce(&p.campi, "Measurement date").and_then(|d| tempo::iso(d, giorno_prima));

    let pozzetti = voce(&tipo_piastra, "Number of the wells in the plate").and_then(numero).filter(|x| *x > 0.0).map(|x| x as u32);
    let nome_tipo_piastra = voce(&tipo_piastra, "Name of the plate type").map(str::to_string);

    // Le letture: una per piastra dei risultati.
    let mut gruppi: Vec<Gruppo> = Vec::new();
    let mut fatte: Vec<Fatta> = Vec::new();
    let mut principale: Option<Modo> = None;
    let mut unita_file: Vec<(Modo, String)> = Vec::new();
    for p in piastre.iter().filter(|p| p.etichetta.is_some()) {
        let nome_etichetta = p.etichetta.clone().unwrap_or_default();
        let base = senza_indice(&nome_etichetta).to_string();
        let blocco = etichette.iter().find(|e| e.nome == base).or_else(|| etichette.iter().find(|e| nome_etichetta.starts_with(&e.nome)));
        if blocco.is_none() && !etichette.is_empty() {
            l.avvisi.push(format!("l'etichetta «{base}» non è descritta nella sezione «Labels»: non ho le sue impostazioni di misura"));
        }
        let modo = modo_etichetta(&base, blocco);
        principale.get_or_insert(modo);
        let proprieta = blocco.map(|b| b.props.as_slice()).unwrap_or(&[]);
        let nome_piastra = if p.barcode.is_empty() { format!("Plate {}", p.numero) } else { p.barcode.clone() };

        // Il detector: «De=1st» è il primo, «De=2nd» il secondo.
        let de = token(&p.misinfo, "De=");
        let filtro_emissione = match de {
            Some("1st") => voce(proprieta, "Ems. filter"),
            Some("2nd") => voce(proprieta, "2nd ems. filter"),
            _ => None,
        }
        .and_then(trova_filtro);
        let filtro_eccitazione = voce(proprieta, "Exc. filter").and_then(trova_filtro);
        let onda_eccitazione = voce(proprieta, "Excitation wavelength").and_then(numero).or_else(|| filtro_eccitazione.and_then(|f| f.onda));
        let banda_eccitazione = if voce(proprieta, "Excitation wavelength").is_some() { None } else { filtro_eccitazione.and_then(|f| f.banda) };
        let onda_emissione = voce(proprieta, "Emission wavelength").and_then(numero).or_else(|| filtro_emissione.and_then(|f| f.onda));
        let banda_emissione = if voce(proprieta, "Emission wavelength").is_some() { None } else { filtro_emissione.and_then(|f| f.banda) };
        let scansione = voce(proprieta, "Using of emission filter").and_then(posizione).or_else(|| token(&p.misinfo, "Em=").and_then(posizione));

        // Il titolo del gruppo: la piastra, l'etichetta, il canale scritto sopra la griglia.
        let canale = p.titolo.split_once(" - ").map(|(_, c)| c.rsplit_once(" (").map(|(a, _)| a).unwrap_or(c).trim().to_string()).filter(|c| !c.is_empty());
        if let Some(u) = p.titolo.rsplit_once('(').and_then(|(_, u)| u.strip_suffix(')')) {
            if !unita_file.iter().any(|(m, x)| *m == modo && x == u) {
                unita_file.push((modo, u.to_string()));
            }
        }
        let altre_dello_stesso = piastre.iter().filter(|q| q.numero == p.numero && q.etichetta.as_deref() == Some(nome_etichetta.as_str())).count();
        let mut parti: Vec<String> = vec![if p.barcode.is_empty() { format!("Plate {}", p.numero) } else { format!("Plate {} · {}", p.numero, p.barcode) }, base.clone()];
        match (&canale, altre_dello_stesso > 1, de) {
            (Some(c), _, _) => parti.push(c.clone()),
            (None, true, Some(d)) => parti.push(format!("detector {d}")),
            _ => {}
        }
        let quando = quando_di(p);
        let mut g = Gruppo { titolo: parti.join(" · "), quando: quando.clone(), piastra: Some(nome_piastra.clone()), pozzetti, ..Default::default() };
        if let Some(x) = voce(&assay, "Protocol ID") {
            g.dati.push(("analytical method identifier".into(), x.to_string()));
        }
        if let Some(x) = voce(&assay, "Assay ID") {
            g.dati.push(("experimental data identifier".into(), x.to_string()));
        }
        for k in ["Protocol Name", "Assay Started", "Assay Finished", "Assay Exported"] {
            if let Some(x) = voce(&assay, k) {
                g.dati.push((format!("~{k}"), x.to_string()));
            }
        }

        let ruoli = mappa.get(&p.numero);
        let temperatura = voce(&p.campi, "Chamber temperature at start").and_then(numero);
        let altezza = voce(&p.campi, "Measured height").and_then(numero);
        let lampi = voce(proprieta, "Number of flashes").and_then(numero);
        let guadagno = voce(proprieta, "Reference AD gain").map(str::to_string);
        const DEL_FILE: [&str; 10] = ["Plate", "Repeat", "Barcode", "Measured height", "Chamber temperature at start", "Measurement date", "Group", "Label", "Measinfo", "Formula"];
        const DELLE_PROPRIETA: [&str; 11] = ["Exc. filter", "Ems. filter", "2nd ems. filter", "Using of emission filter", "Number of flashes", "Reference AD gain", "Excitation wavelength", "Emission wavelength", "Last edited", "Last edited by", "Factory preset"];

        let mut ids: HashMap<String, String> = HashMap::new();
        for (lettera, celle) in &p.griglia.righe {
            for (k, c) in celle.iter().enumerate() {
                let c = c.trim();
                if c.is_empty() {
                    continue;
                }
                let pozzetto = format!("{lettera}{}", k + 1);
                let mut m = Misura { id: l.id(), campione: format!("{nome_piastra} {pozzetto}"), pozzetto: Some(pozzetto.clone()), piastra: Some(nome_piastra.clone()), quando: quando.clone(), ..Default::default() };
                let codice = ruoli.and_then(|r| r.get(&(lettera.clone(), k + 1)));
                if let Some(r) = codice.and_then(|c| ruolo_codice(c)) {
                    m.ruolo = r;
                    m.ruolo_dal_file = true;
                }
                m.valori.push(Valore::testo("detection type", modo.nome(), Posto::Impostazione));
                if let Some(t) = temperatura {
                    m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Misura));
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
                if let Some(x) = scansione {
                    m.valori.push(Valore::testo("scan position setting (plate reader)", x, Posto::Impostazione));
                }
                // L'assorbanza si misura alla lunghezza d'onda del filtro (o del monocromatore) d'eccitazione.
                if modo == Modo::Assorbanza {
                    if let Some(o) = onda_eccitazione {
                        m.valori.push(Valore::num("detector wavelength setting", o, "nm", Posto::Impostazione));
                    }
                    if let Some(b) = banda_eccitazione {
                        m.valori.push(Valore::num("detector bandwidth setting", b, "nm", Posto::Impostazione));
                    }
                } else {
                    if let Some(o) = onda_emissione {
                        m.valori.push(Valore::num("detector wavelength setting", o, "nm", Posto::Impostazione));
                    }
                    if let Some(b) = banda_emissione {
                        m.valori.push(Valore::num("detector bandwidth setting", b, "nm", Posto::Impostazione));
                    }
                    if let Some(o) = onda_eccitazione {
                        m.valori.push(Valore::num("excitation wavelength setting", o, "nm", Posto::Impostazione));
                    }
                    if let Some(b) = banda_eccitazione {
                        m.valori.push(Valore::num("excitation bandwidth setting", b, "nm", Posto::Impostazione));
                    }
                }
                // Il resto della riga «Plate information»: temperature e umidità, scansione, cinetica.
                for (nome, v) in p.campi.iter().filter(|(k, v)| !DEL_FILE.contains(&k.as_str()) && !v.is_empty() && !v.eq_ignore_ascii_case("n/a")) {
                    let minuscolo = nome.to_lowercase();
                    let unita = if minuscolo.contains("temperature") {
                        "°C"
                    } else if minuscolo.contains("humidity") {
                        "%"
                    } else {
                        ""
                    };
                    m.valori.push(match numero(v) {
                        Some(x) => Valore::num(nome, x, unita, Posto::ImpostazioneAltro),
                        None => Valore::testo(nome, v, Posto::Altro),
                    });
                }
                // Le impostazioni dell'etichetta (luce, specchi, tempi, diaframma…) e della piastra.
                for (nome, v) in proprieta.iter().filter(|(k, v)| !DELLE_PROPRIETA.contains(&k.as_str()) && !v.is_empty() && !v.eq_ignore_ascii_case("n/a")) {
                    m.valori.push(valore_libero(nome, v, Posto::ImpostazioneAltro));
                }
                for k in ["Number of rows", "Number of columns", "Height of the plate"] {
                    if let Some(v) = voce(&tipo_piastra, k) {
                        m.valori.push(valore_libero(k, v, Posto::ImpostazioneAltro));
                    }
                }
                if let Some(x) = &nome_tipo_piastra {
                    m.valori.push(Valore::testo("Name of the plate type", x, Posto::CampioneAltro));
                }
                if let Some(x) = voce(&p.campi, "Group").and_then(numero) {
                    m.valori.push(Valore::num("group identifier", x, "", Posto::CampioneAltro));
                }
                if let Some(x) = voce(&p.campi, "Repeat").and_then(numero) {
                    m.valori.push(Valore::num("Repeat", x, "", Posto::CampioneAltro));
                }
                if let Some(c) = codice {
                    m.valori.push(Valore::testo("platemap code", c, Posto::CampioneAltro));
                }
                if !p.misinfo.is_empty() {
                    m.valori.push(Valore::testo("Measinfo", &p.misinfo, Posto::Altro));
                }
                // Il valore: un numero, o il testo che lo strumento ha scritto al suo posto.
                match numero(c) {
                    Some(x) => m.valori.push(Valore::num(modo.concetto(), x, modo.unita(), Posto::Misura)),
                    None => m.valori.push(Valore { chiave: modo.concetto().into(), dato: Dato::Assente(c.to_string()), unita: modo.unita().into(), posto: Posto::Misura }),
                }
                ids.insert(pozzetto, m.id.clone());
                g.misure.push(m);
            }
        }
        if g.misure.is_empty() {
            l.avvisi.push(format!("la piastra {} ({base}) non ha pozzetti con un valore", p.numero));
            continue;
        }
        fatte.push(Fatta { numero: p.numero.clone(), etichetta: nome_etichetta, misinfo: norm(&p.misinfo), modo, ids });
        gruppi.push(g);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono letture con pozzetti: ci sono solo piastre di calcolo o vuote".into());
    }
    let principale = principale.unwrap_or(Modo::Fluorescenza);

    // I calcoli: ogni valore di una piastra di calcolo viene dalle letture elencate nelle sue informazioni di fondo.
    for p in piastre.iter().filter(|p| p.formula.is_some()) {
        let formula = p.formula.clone().unwrap_or_default();
        let nome = formula.split_once('=').map(|(a, _)| a.trim()).unwrap_or(formula.trim()).to_string();
        let mut origini: Vec<&Fatta> = Vec::new();
        for s in &p.sfondo {
            if let Some(x) = fatte.iter().find(|x| x.numero == s.piastra && x.etichetta.contains(&s.etichetta) && x.misinfo == norm(&s.misinfo)) {
                if !origini.iter().any(|o| std::ptr::eq(*o, x)) {
                    origini.push(x);
                }
            }
        }
        if origini.is_empty() {
            l.avvisi.push(format!("il calcolo «{nome}» della piastra {} non ha le letture da cui viene: non l'ho scritto", p.numero));
            continue;
        }
        let mut senza_origine = 0;
        for (lettera, celle) in &p.griglia.righe {
            for (k, c) in celle.iter().enumerate() {
                let Some(valore) = numero(c) else { continue };
                let pozzetto = format!("{lettera}{}", k + 1);
                let sorgenti: Vec<String> = origini.iter().filter_map(|x| x.ids.get(&pozzetto).cloned()).collect();
                if sorgenti.is_empty() {
                    senza_origine += 1;
                    continue;
                }
                l.calcolati.push(Calcolato { nome: nome.clone(), valore, unita: String::new(), sorgenti, da: origini[0].modo.concetto().into(), descrizione: formula.clone(), da_calcoli: Vec::new() });
            }
        }
        if senza_origine > 0 {
            l.avvisi.push(format!("{senza_origine} valori del calcolo «{nome}» non hanno una lettura nello stesso pozzetto e non sono stati scritti"));
        }
    }

    // Lo strumento e il programma.
    l.strumento.produttore = "PerkinElmer".into();
    l.strumento.modello = "EnVision".into();
    if let Some(x) = voce(&strumento, "Nickname") {
        l.strumento.identificativo = x.to_string();
    }
    if let Some(x) = voce(&strumento, "Serial number").or_else(|| voce(&assay, "Serial#")) {
        l.strumento.seriale = x.to_string();
    }
    l.strumento.software = "EnVision Workstation".into();
    if let Some(r) = righe.iter().find(|r| cella0(r).starts_with("Exported with ")) {
        let t = cella0(r);
        if let Some((a, v)) = t.strip_prefix("Exported with ").and_then(|x| x.split_once(" version ")) {
            l.strumento.software = a.trim().to_string();
            l.strumento.versione = v.trim().to_string();
        }
    }

    // Per il foglio «Strumento»: protocollo, assay, piastra, etichette, filtri.
    if let Some(x) = voce(&assay, "Protocol Name").or_else(|| voce(&protocollo, "Protocol name")) {
        l.info.push(("Protocollo".into(), match voce(&assay, "Protocol ID") {
            Some(id) => format!("{x} ({id})"),
            None => x.to_string(),
        }));
    }
    if let Some(x) = voce(&assay, "Assay ID") {
        l.info.push(("Assay".into(), x.to_string()));
    }
    if let (Some(a), b) = (voce(&assay, "Assay Started"), voce(&assay, "Assay Finished")) {
        l.info.push(("Avvio e fine dell'assay".into(), format!("{a} → {}", b.unwrap_or("—"))));
    }
    if let Some(x) = voce(&protocollo, "Mode of measurement") {
        l.info.push(("Modo di misura".into(), norm(x)));
    }
    if let Some(x) = voce(&protocollo, "Protocol notes") {
        l.info.push(("Note del protocollo".into(), x.to_string()));
    }
    if let Some(x) = &nome_tipo_piastra {
        l.info.push(("Tipo di piastra".into(), x.clone()));
    }
    let descrizioni: Vec<String> = etichette
        .iter()
        .map(|e| {
            let resto: Vec<String> = e.props.iter().filter(|(k, v)| !v.is_empty() && !matches!(k.as_str(), "Last edited" | "Last edited by" | "Factory preset")).map(|(k, v)| format!("{k}: {v}")).collect();
            format!("«{}» {}", e.nome, resto.join(", "))
        })
        .collect();
    if !descrizioni.is_empty() {
        l.info.push(("Etichette di misura".into(), descrizioni.join(" · ")));
    }
    if !filtri.is_empty() {
        let v: Vec<String> = filtri
            .iter()
            .map(|f| match (f.onda, f.banda) {
                (Some(o), Some(b)) => format!("{} ({o} nm ±{b})", f.nome),
                (Some(o), None) => format!("{} (da {o} nm)", f.nome),
                _ => f.nome.clone(),
            })
            .collect();
        l.info.push(("Filtri ottici".into(), v.join(", ")));
    }
    if !specchi.is_empty() {
        let v: Vec<String> = specchi.iter().map(|(n, p)| match voce(p, "Description") {
            Some(d) => format!("{n} ({d})"),
            None => n.clone(),
        }).collect();
        l.info.push(("Specchi".into(), v.join(", ")));
    }
    if !diaframmi.is_empty() {
        l.info.push(("Diaframmi".into(), diaframmi.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", ")));
    }
    if !unita_file.is_empty() {
        let v: Vec<String> = unita_file
            .iter()
            .map(|(m, u)| {
                let nome = match m {
                    Modo::Assorbanza => "assorbanza",
                    Modo::Fluorescenza => "fluorescenza",
                    Modo::Luminescenza => "luminescenza",
                };
                format!("{u} ({nome})")
            })
            .collect();
        l.info.push(("Unità dello strumento".into(), v.join(", ")));
    }
    l.principale = principale.concetto().into();
    l.unita = principale.unita().into();
    l.gruppi.extend(gruppi);
    Ok(())
}

#[cfg(test)]
mod prove {
    use super::*;
    use crate::strumenti::leggi_con;

    const PIASTRA: &str = "Plate information\nPlate,Repeat,Barcode,Measured height,Chamber temperature at start,Group,Label,Measinfo,Measurement date\n1,1,\"=\"\"B1\"\"\",11.9,23.5,1,A450(1),De=1st Ex=Btm Em=N/A Wdw=N/A (8),1/15/2024 10:30:00 AM\n\nBackground information\nPlate,Label,Result,Signal,Flashes/Time,Meastime,MeasInfo\n1,A450,0,29500,300,00:00:00.000,De=1st Ex=Btm Em=N/A Wdw=N/A (8)\n\nResults for A450(1) - channel 1 (A)\n,01,02,03,\nA,0.15,0.085,,\nC,1.8,OVRFLW,0.5,\n\n";
    const CODA: &str = "Basic assay information\nAssay ID: ,,,,101\nProtocol ID: ,,,,100250\n\nPlate type:\nName of the plate type,,,,96 General\nNumber of the wells in the plate,,,,96\n\nLabels:\nA450,,,,2000014\nExc. filter,,,,BFP 450\nNumber of flashes,,,,300\n\nFilters:\nBFP 450,,,,216\nFilter type,,,,Emission\nDescription,,,,M450 CWL=450nm BW=1 0nm Tmin=60%\n\nInstrument:\nSerial number,,,,1050242\nNickname,,,,EnVision#Alpha\n\nExported with EnVision Workstation version 1.14.3049.1642\n";

    fn leggi_testo(t: &str) -> Result<Lettura, String> {
        leggi_con(&PROFILO, &Fonte::da_bytes("prova.csv", t.as_bytes().to_vec()))
    }

    #[test]
    fn legge_una_piastra_con_righe_che_saltano() {
        let l = leggi_testo(&format!("{PIASTRA}{CODA}")).unwrap();
        assert_eq!(l.strumento.seriale, "1050242");
        assert_eq!(l.strumento.identificativo, "EnVision#Alpha");
        assert_eq!(l.strumento.versione, "1.14.3049.1642");
        let g = &l.gruppi[0];
        // Le righe del file sono A e C: i pozzetti restano A1, A2, C1, C2, C3 (non A, B).
        let pozzetti: Vec<&str> = g.misure.iter().filter_map(|m| m.pozzetto.as_deref()).collect();
        assert_eq!(pozzetti, ["A1", "A2", "C1", "C2", "C3"]);
        assert_eq!(g.misure[0].campione, "B1 A1");
        assert_eq!(g.misure[0].valore("absorbance").and_then(Valore::numero), Some(0.15));
        // Il testo dello strumento al posto del numero resta com'è.
        assert!(matches!(&g.misure[3].valore("absorbance").unwrap().dato, Dato::Assente(t) if t == "OVRFLW"));
        // Il filtro d'eccitazione dà la lunghezza d'onda dell'assorbanza; gli spazi in mezzo alle cifre non contano.
        assert_eq!(g.misure[0].valore("detector wavelength setting").and_then(Valore::numero), Some(450.0));
        assert_eq!(g.misure[0].valore("detector bandwidth setting").and_then(Valore::numero), Some(10.0));
    }

    #[test]
    fn la_piastra_senza_titolo_e_un_errore_chiaro() {
        // Dopo la piastra c'è l'intestazione di un'altra senza il titolo «Plate information» (file troncato).
        let t = format!("{PIASTRA}Plate,Repeat,Barcode\n1,1,x\n\n{CODA}");
        let e = leggi_testo(&t).unwrap_err();
        assert!(e.contains("Plate information"), "{e}");
    }
}
