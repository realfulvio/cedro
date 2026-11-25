//! Agilent BioTek Gen5: il file di testo (TXT) che Gen5 esporta per una piastra. In testa i dati dello
//! strumento (versione, protocollo, data, lettore), poi «Procedure Details» (le letture: assorbanza,
//! fluorescenza, luminescenza, AlphaLISA, in punto finale, cinetica o spettro), «Layout» (i nomi dei
//! campioni e le concentrazioni), «Results» e le tabelle delle misure. Una piastra per file.

use std::collections::HashMap;

use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Calcolato, Curva, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore, PRINCIPALE_CURVA};

pub const PROFILO: Profilo = Profilo {
    id: "agilent_gen5",
    nome: "Agilent BioTek Gen5",
    produttore: "Agilent BioTek",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["txt"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let prime: Vec<&str> = f.testo.lines().take(6).collect();
    if !prime.iter().any(|r| r.starts_with("Software Version")) {
        return 0;
    }
    let procedura = f.testo.lines().any(|r| r.starts_with("Procedure Details"));
    let lettore = f.testo.lines().take(40).any(|r| r.starts_with("Reader Type:") || r.starts_with("Reader Serial Number:"));
    match (procedura, lettore) {
        (true, true) => 95,
        (true, false) => 75,
        _ => 55,
    }
}

// ---------------------------------------------------------------------------------------------
// Il testo diviso in sezioni
// ---------------------------------------------------------------------------------------------

fn vuota(s: &str) -> bool {
    s.trim().is_empty()
}

fn nome_sezione(riga: &str) -> String {
    for campo in riga.split('\t') {
        let n = campo.trim_matches('"').trim_matches(':');
        if !n.is_empty() {
            return n.to_string();
        }
    }
    String::new()
}

/// Una sezione tutta rientrata di una tabulazione (la tabella dei tempi) si porta a sinistra.
fn togli_tab_iniziale(righe: Vec<String>) -> Vec<String> {
    if righe.len() < 2 {
        return righe;
    }
    let dopo: &[String] = if !righe[0].starts_with('\t') { &righe[1..] } else { &righe[..] };
    let piene: Vec<&String> = dopo.iter().filter(|r| !r.trim().is_empty()).collect();
    if !piene.is_empty() && piene.iter().all(|r| r.starts_with('\t')) {
        return righe.into_iter().map(|r| r.strip_prefix('\t').map(str::to_string).unwrap_or(r)).collect();
    }
    righe
}

#[derive(Default)]
struct Sezioni {
    /// Le coppie dell'intestazione: «Software Version», «Date»…
    intestazione: Vec<(String, String)>,
    /// Le sezioni nell'ordine in cui compaiono (una con lo stesso nome prende il posto della prima).
    sezioni: Vec<(String, Vec<String>)>,
    /// Le tabelle dei tempi delle cinetiche, con il nome della lettura a cui appartengono.
    cinetiche: Vec<(String, Vec<String>)>,
}

impl Sezioni {
    fn metti(v: &mut Vec<(String, Vec<String>)>, nome: String, righe: Vec<String>) {
        match v.iter_mut().find(|(n, _)| *n == nome) {
            Some(e) => e.1 = righe,
            None => v.push((nome, righe)),
        }
    }

    fn sezione(&self, nome: &str) -> Option<&Vec<String>> {
        self.sezioni.iter().find(|(n, _)| n == nome).map(|(_, r)| r)
    }

    fn cinetica(&self, nome: &str) -> Option<&Vec<String>> {
        self.cinetiche.iter().find(|(n, _)| n == nome).map(|(_, r)| r)
    }
}

fn dividi(testo: &str) -> Result<Sezioni, String> {
    let righe: Vec<String> = testo.split('\n').map(|r| r.trim_end_matches('\t').to_string()).collect();
    let inizi: Vec<usize> = righe.iter().enumerate().filter(|(_, r)| r.starts_with("Software Version")).map(|(i, _)| i).collect();
    if inizi.is_empty() {
        return Err("nel file non ci sono dati di una piastra".into());
    }
    if inizi.len() > 1 {
        return Err("il file ha più piastre: Cedro ne legge una per file (da Gen5 si esporta una piastra alla volta)".into());
    }
    let r = &righe[inizi[0]..];
    let mut i = 0usize;
    let mut s = Sezioni::default();
    // L'intestazione arriva fino a «Procedure Details».
    while i < r.len() && !r[i].contains("Procedure Details") {
        if !r[i].is_empty() {
            let mut c = r[i].split('\t');
            let k = c.next().unwrap_or("").to_string();
            let v = c.next().unwrap_or("").to_string();
            s.intestazione.push((k, v));
        }
        i += 1;
    }
    while i < r.len() && !r[i].starts_with("Procedure Details") {
        i += 1;
    }
    if i >= r.len() {
        return Err("manca la sezione «Procedure Details»".into());
    }
    i += 1;
    while i < r.len() && vuota(&r[i]) {
        i += 1;
    }
    let piastra = r.get(i).cloned().unwrap_or_default();
    s.intestazione.push(("Plate Type".into(), piastra));
    let prendi = |i: &mut usize| -> Vec<String> {
        let mut v = Vec::new();
        while *i < r.len() && !vuota(&r[*i]) {
            v.push(r[*i].clone());
            *i += 1;
        }
        v
    };
    let procedura = prendi(&mut i);
    Sezioni::metti(&mut s.sezioni, "Procedure Details".into(), procedura);
    while i < r.len() && vuota(&r[i]) {
        i += 1;
    }

    let mut ultimo_nome: Option<String> = None;
    let mut ultima_lettura: Option<String> = None;
    loop {
        let mut righe_s = prendi(&mut i);
        if righe_s.is_empty() {
            break;
        }
        let mut nome = nome_sezione(&righe_s[0]);
        // «Layout» o «Results» da soli su una riga: i dati stanno nel blocco dopo.
        if righe_s.len() == 1 && (nome == "Layout" || nome == "Results") {
            while i < r.len() && vuota(&r[i]) {
                i += 1;
            }
            let dopo = prendi(&mut i);
            if !dopo.is_empty() {
                righe_s.extend(dopo);
            }
        }
        // «Read 1:450» da solo: il nome della lettura a cui appartiene il blocco dopo.
        if righe_s.len() == 1 && nome.starts_with("Read ") {
            ultima_lettura = Some(nome.clone());
            while i < r.len() && vuota(&r[i]) {
                i += 1;
            }
            righe_s = prendi(&mut i);
            if righe_s.is_empty() {
                continue;
            }
            nome = nome_sezione(&righe_s[0]);
        }
        if nome == "Time" {
            if let Some(l) = ultima_lettura.take() {
                Sezioni::metti(&mut s.cinetiche, l, togli_tab_iniziale(righe_s));
            } else if let Some(n) = &ultimo_nome {
                Sezioni::metti(&mut s.cinetiche, n.clone(), togli_tab_iniziale(righe_s));
            } else {
                Sezioni::metti(&mut s.sezioni, nome, togli_tab_iniziale(righe_s));
            }
        } else {
            Sezioni::metti(&mut s.sezioni, nome.clone(), togli_tab_iniziale(righe_s));
            ultimo_nome = Some(nome);
        }
        while i < r.len() && vuota(&r[i]) {
            i += 1;
        }
    }
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// Numeri
// ---------------------------------------------------------------------------------------------

/// Come `try_float_or_none` di allotropy: la virgola vale il punto; il testo che non è un numero è `None`.
/// Una cella vuota è `Some(NaN)`: non c'è niente da leggere.
fn numero_o_vuoto(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() || mancante(t) {
        return Some(f64::NAN);
    }
    t.replace(',', ".").parse::<f64>().ok()
}

/// Le parole che pandas legge come «manca».
fn mancante(s: &str) -> bool {
    matches!(s, "" | "#N/A" | "#N/A N/A" | "#NA" | "-1.#IND" | "-1.#QNAN" | "-NaN" | "-nan" | "1.#IND" | "1.#QNAN" | "<NA>" | "N/A" | "NA" | "NULL" | "NaN" | "None" | "n/a" | "nan" | "null")
}

fn float_stretto(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn tempo_in_secondi(t: &str) -> Result<f64, String> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(0.0);
    }
    let p: Vec<&str> = t.split(':').collect();
    if p.len() != 3 {
        return Err(format!("tempo non leggibile: «{t}»"));
    }
    let n: Result<Vec<f64>, _> = p.iter().map(|x| x.trim().parse::<f64>()).collect();
    let n = n.map_err(|_| format!("tempo non leggibile: «{t}»"))?;
    Ok(n[0] * 3600.0 + n[1] * 60.0 + n[2])
}

/// Un secondo come lo scrive Python («22.0»).
fn secondi_testo(s: f64) -> String {
    py_float(s)
}

// ---------------------------------------------------------------------------------------------
// L'intestazione
// ---------------------------------------------------------------------------------------------

struct Intestazione {
    versione: String,
    esperimento: Option<String>,
    protocollo: Option<String>,
    quando: Option<String>,
    piastra: Option<String>,
    modello: Option<String>,
    seriale: Option<String>,
    tipo_lettura: Option<String>,
    numero_piastra: Option<String>,
    altri: Vec<(String, String)>,
}

fn intestazione(s: &Sezioni, nome_file: &str) -> Result<Intestazione, String> {
    let prendi = |k: &str| s.intestazione.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    let versione = prendi("Software Version").ok_or("manca «Software Version»")?;
    let data = prendi("Date").ok_or("manca «Date»")?;
    let ora = prendi("Time").ok_or("manca «Time»")?;
    let dt = format!("{data} {ora}");
    let gp = tempo::giorno_prima([dt.as_str()]);
    let quando = tempo::iso(&dt, gp);
    // Il nome del file di Benchling Connect: 240307_114129_BNCH654563_….txt dà l'identificativo della piastra.
    let da_nome = {
        let p: Vec<&str> = nome_file.splitn(4, '_').collect();
        (p.len() == 4 && p[0].len() == 6 && p[1].len() == 6 && p[0].bytes().all(|b| b.is_ascii_digit()) && p[1].bytes().all(|b| b.is_ascii_digit()) && !p[2].is_empty() && nome_file.to_ascii_lowercase().ends_with(".txt")).then(|| p[2].to_string())
    };
    let numero_piastra = prendi("Plate Number");
    let letti = ["Software Version", "Experiment File Path:", "Protocol File Path:", "Date", "Time", "Reader Type:", "Reader Serial Number:", "Plate Type", "Reading Type", "Plate Number"];
    let altri = s.intestazione.iter().filter(|(k, _)| !letti.contains(&k.as_str())).cloned().collect();
    let vuoto = |v: Option<String>| v.filter(|x| !x.trim().is_empty());
    Ok(Intestazione {
        versione,
        esperimento: vuoto(prendi("Experiment File Path:")),
        protocollo: vuoto(prendi("Protocol File Path:")),
        quando,
        piastra: da_nome.or_else(|| numero_piastra.clone()),
        modello: vuoto(prendi("Reader Type:")),
        seriale: vuoto(prendi("Reader Serial Number:")),
        tipo_lettura: vuoto(prendi("Reading Type")),
        numero_piastra: vuoto(numero_piastra),
        altri,
    })
}

// ---------------------------------------------------------------------------------------------
// Le letture («Procedure Details»)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modo {
    Assorbanza,
    Fluorescenza,
    Luminescenza,
    Alpha,
}

impl Modo {
    const TUTTI: [Modo; 4] = [Modo::Assorbanza, Modo::Fluorescenza, Modo::Luminescenza, Modo::Alpha];

    fn nome(self) -> &'static str {
        match self {
            Modo::Assorbanza => "Absorbance",
            Modo::Fluorescenza => "Fluorescence",
            Modo::Luminescenza => "Luminescence",
            Modo::Alpha => "Alpha",
        }
    }

    fn minuscolo(self) -> &'static str {
        match self {
            Modo::Assorbanza => "absorbance",
            Modo::Fluorescenza => "fluorescence",
            Modo::Luminescenza => "luminescence",
            Modo::Alpha => "alpha",
        }
    }

    /// L'unità del valore come la scrive Gen5.
    fn unita(self) -> &'static str {
        match self {
            Modo::Assorbanza => "OD",
            Modo::Fluorescenza | Modo::Alpha => "RFU",
            Modo::Luminescenza => "RLU",
        }
    }

    /// La chiave del cubo della cinetica nello schema.
    fn cubo_cinetica(self) -> &'static str {
        match self {
            Modo::Assorbanza => "absorption profile data cube",
            Modo::Fluorescenza | Modo::Alpha => "fluorescence emission profile data cube",
            Modo::Luminescenza => "luminescence profile data cube",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TipoLettura {
    Endpoint,
    Spettro,
}

#[derive(Clone, Debug, Default)]
struct FiltroSet {
    gain: String,
    emissione: Option<String>,
    eccitazione: Option<String>,
    specchio: Option<String>,
    ottica: Option<String>,
    direzione: Option<&'static str>,
}

fn float_non_nan(s: &str) -> Option<f64> {
    let v = s.trim().replace(',', ".").parse::<f64>().ok()?;
    (!v.is_nan()).then_some(v)
}

impl FiltroSet {
    fn onda_rivelatore(&self) -> Option<f64> {
        self.emissione.as_deref().filter(|e| !e.is_empty()).and_then(|e| float_non_nan(e.split('/').next().unwrap_or("")))
    }
    fn banda_rivelatore(&self) -> Option<f64> {
        self.emissione.as_deref().filter(|e| !e.is_empty()).and_then(|e| e.split('/').nth(1)).and_then(float_non_nan)
    }
    fn onda_eccitazione(&self) -> Option<f64> {
        self.eccitazione.as_deref().filter(|e| !e.is_empty()).and_then(|e| float_non_nan(e.split('/').next().unwrap_or("")))
    }
    fn banda_eccitazione(&self) -> Result<Option<f64>, String> {
        match self.eccitazione.as_deref().filter(|e| !e.is_empty()).and_then(|e| e.split('/').nth(1)) {
            None => Ok(None),
            Some(b) => float_stretto(b).map(Some).ok_or_else(|| format!("larghezza di banda dell'eccitazione non leggibile: «{b}»")),
        }
    }
    fn taglio(&self) -> Result<Option<f64>, String> {
        match self.specchio.as_deref().filter(|e| !e.is_empty()) {
            None => Ok(None),
            Some(m) => {
                let t = m.split(' ').nth(1).ok_or_else(|| format!("filtro dello specchio non leggibile: «{m}»"))?;
                float_stretto(t).map(Some).ok_or_else(|| format!("filtro dello specchio non leggibile: «{m}»"))
            }
        }
    }
    fn posizione(&self) -> Option<&'static str> {
        let mut p = self.ottica.as_deref();
        if let Some(m) = self.specchio.as_deref().filter(|m| !m.is_empty()) {
            p = m.split(' ').next();
        }
        match p {
            Some("Top") => Some("top scan position (plate reader)"),
            Some("Bottom") => Some("bottom scan position (plate reader)"),
            _ => None,
        }
    }
}

#[derive(Default)]
struct Controllo {
    etichetta_passo: Option<String>,
    polarizzazione: bool,
    liste: HashMap<&'static str, Vec<String>>,
    singoli: HashMap<String, String>,
}

const CHIAVI_LISTA: [&str; 10] = ["Emission", "Excitation", "Fixed Excitation", "Fixed Emission", "Emission Start", "Excitation Start", "Optics", "Gain", "Mirror", "Wavelengths"];

impl Controllo {
    fn aggiungi(&mut self, k: &str, v: Vec<String>) {
        if let Some(c) = CHIAVI_LISTA.iter().find(|c| **c == k) {
            self.liste.entry(c).or_default().extend(v);
        } else if let Some(x) = v.into_iter().next() {
            self.singoli.insert(k.to_string(), x);
        }
    }

    fn lista(&self, k: &str) -> Vec<String> {
        self.liste.get(k).cloned().unwrap_or_default()
    }

    fn singolo(&self, k: &str) -> Option<&str> {
        self.singoli.get(k).map(String::as_str)
    }

    fn crea(righe: &[String], modo: Modo) -> Result<Controllo, String> {
        let mut c = Controllo::default();
        for riga in righe {
            let s = riga.trim().trim_matches('"').trim();
            if s == "Fluorescence Polarization" {
                c.polarizzazione = true;
                continue;
            }
            if s.starts_with("Read\t") {
                let p: Vec<&str> = riga.trim().split('\t').collect();
                if p.len() != 2 {
                    return Err("la riga «Read» della lettura non è come Gen5 la scrive".into());
                }
                let titolo = match modo {
                    Modo::Alpha => "Alpha",
                    m => m.nome(),
                };
                c.etichetta_passo = (p[1] != format!("{titolo} Endpoint")).then(|| p[1].to_string());
                continue;
            }
            if s.starts_with("Wavelengths") {
                let dopo = s.split_once(':').map(|(_, d)| d).ok_or("righe «Wavelengths» non leggibili")?;
                c.aggiungi("Wavelengths", dopo.split(", ").map(|x| x.trim().to_string()).collect());
                continue;
            }
            let mut speciale = false;
            for (testo, chiave) in [("Fixed Excitation:", "Fixed Excitation"), ("Fixed Emission:", "Fixed Emission"), ("Emission Start:", "Emission Start"), ("Excitation Start:", "Excitation Start")] {
                if let Some((_, dopo)) = s.split_once(testo) {
                    c.aggiungi(chiave, vec![dopo.trim().to_string()]);
                    speciale = true;
                    break;
                }
            }
            if speciale {
                continue;
            }
            for dato in s.split(",  ") {
                let p: Vec<&str> = dato.split(": ").collect();
                if p.len() != 2 {
                    continue;
                }
                c.aggiungi(p[0], vec![p[1].to_string()]);
            }
        }
        Ok(c)
    }
}

#[derive(Clone, Debug)]
struct Cinetica {
    durata: String,
    intervallo: String,
    letture: u32,
}

fn cinetica_da_procedura(righe: &[String]) -> Result<Option<Cinetica>, String> {
    let Some(riga) = righe.iter().find(|r| r.starts_with("Start Kinetic\t")) else { return Ok(None) };
    let parti: Vec<&str> = riga.trim().split('\t').nth(1).unwrap_or("").split(", ").collect();
    let trova = |k: &str| -> Result<&str, String> {
        let v: Vec<&&str> = parti.iter().filter(|p| p.contains(k)).collect();
        if v.len() != 1 {
            return Err(format!("nella riga della cinetica manca «{k}»"));
        }
        Ok(v[0])
    };
    let durata = trova("Runtime")?.split(' ').nth(1).unwrap_or("").trim().to_string();
    let intervallo = trova("Interval")?.split(' ').nth(1).unwrap_or("").trim().to_string();
    let letture = trova("Reads")?.split(' ').next().unwrap_or("").trim().parse::<u32>().map_err(|_| "numero di letture della cinetica non leggibile".to_string())?;
    Ok(Some(Cinetica { durata, intervallo, letture }))
}

#[derive(Clone, Debug)]
struct Lett {
    modo: Modo,
    tipo: TipoLettura,
    /// Le etichette delle misure (con gli alias), nell'ordine in cui si sono trovate.
    etichette: Vec<String>,
    medie: Option<f64>,
    distanza: Option<f64>,
    velocita: Option<String>,
    filtri: Vec<(String, FiltroSet)>,
    emissione: bool,
    eccitazione: bool,
}

impl Lett {
    fn filtro(&self, etichetta: &str) -> Option<&FiltroSet> {
        self.filtri.iter().find(|(e, _)| e == etichetta).map(|(_, f)| f)
    }
}

fn aggiungi_unico(v: &mut Vec<String>, x: String) {
    if !v.contains(&x) {
        v.push(x);
    }
}

fn letture_da_procedura(righe: &[String]) -> Result<Vec<Lett>, String> {
    let testo = righe.join("\n");
    let tipo = if testo.contains("Area Scan") {
        return Err("Cedro legge le letture in punto finale, le cinetiche e gli spettri; le scansioni dell'area (Area Scan) no".into());
    } else if testo.contains("Spectrum") {
        TipoLettura::Spettro
    } else if testo.contains("Endpoint") {
        TipoLettura::Endpoint
    } else {
        return Err("il tipo di lettura non c'è: servono Endpoint o Spectrum".into());
    };
    let mut modi: Vec<Modo> = Vec::new();
    for m in Modo::TUTTI {
        for suffisso in ["Endpoint", "Spectrum"] {
            let ago = format!("\t{} {}", m.nome(), suffisso);
            modi.extend(std::iter::repeat_n(m, testo.matches(ago.as_str()).count()));
        }
    }
    if modi.is_empty() {
        return Err("il modo di lettura non c'è: servono assorbanza, fluorescenza, luminescenza o AlphaLISA".into());
    }
    if modi.contains(&Modo::Alpha) && modi.contains(&Modo::Fluorescenza) {
        return Err("nello stesso file ci sono AlphaLISA e fluorescenza: Cedro non li legge insieme".into());
    }
    // Le sezioni «Read»: dalla riga che comincia con «Read» e una tabulazione fino alla prossima.
    let e_read = |r: &str| r.trim_start().starts_with("Read\t");
    let mut sezioni: Vec<Vec<String>> = Vec::new();
    for r in righe {
        if e_read(r) {
            sezioni.push(vec![r.clone()]);
        } else if let Some(u) = sezioni.last_mut() {
            u.push(r.clone());
        }
    }
    if modi.len() != sezioni.len() {
        return Err("il numero delle letture non torna con i modi di lettura trovati".into());
    }
    let mut out = Vec::new();
    for (modo, sezione) in modi.into_iter().zip(sezioni) {
        let modo_eff = if modo == Modo::Alpha { Modo::Fluorescenza } else { modo };
        let c = Controllo::crea(&sezione, modo_eff)?;
        let (etichette, alias) = etichette_misure(&c, modo_eff, tipo)?;
        let mut tutte = etichette.clone();
        for a in alias.iter().flat_map(|(_, a)| a.iter()) {
            aggiungi_unico(&mut tutte, a.clone());
        }
        let altezza = c.singolo("Read Height").unwrap_or("").split(' ').next().unwrap_or("").to_string();
        let testo_sezione = sezione.join("\n");
        let filtri = filtri(&etichette, &alias, &c, modo_eff)?;
        out.push(Lett {
            modo: modo_eff,
            tipo,
            etichette: tutte,
            medie: c.singolo("Measurements/Data Point").and_then(|x| x.trim().replace(',', ".").parse().ok()),
            distanza: float_non_nan(&altezza),
            velocita: c.singolo("Read Speed").map(str::to_string),
            filtri,
            emissione: !c.lista("Fixed Excitation").is_empty() || testo_sezione.contains("EM Spectrum"),
            eccitazione: !c.lista("Fixed Emission").is_empty() || testo_sezione.contains("EX Spectrum"),
        });
        // La correzione del cammino ottico è nelle etichette: non serve tenerla.
        let _ = c.singolo("Pathlength Correction");
    }
    Ok(out)
}

type Alias = Vec<(String, Vec<String>)>;

fn etichette_misure(c: &Controllo, modo: Modo, tipo: TipoLettura) -> Result<(Vec<String>, Alias), String> {
    let prefisso = c.etichetta_passo.as_ref().map(|s| format!("{s}:")).unwrap_or_default();
    match modo {
        Modo::Assorbanza => {
            let mut v: Vec<String> = c.lista("Wavelengths").iter().map(|w| format!("{prefisso}{w}")).collect();
            if let Some(pc) = c.singolo("Pathlength Correction") {
                let (t, r) = pc.split_once(" / ").ok_or("correzione del cammino ottico non leggibile")?;
                v.push(format!("{prefisso}{t} [Test]"));
                v.push(format!("{prefisso}{r} [Ref]"));
            }
            if v.is_empty() && tipo == TipoLettura::Spettro {
                v.push(format!("{}:Spectrum", prefisso.trim_end_matches(':')));
            }
            Ok((v, Vec::new()))
        }
        Modo::Fluorescenza | Modo::Alpha => {
            let ecc = prima_non_vuota(&[c.lista("Excitation"), c.lista("Fixed Excitation"), c.lista("Excitation Start")]);
            let emi = prima_non_vuota(&[c.lista("Emission"), c.lista("Fixed Emission"), c.lista("Emission Start")]);
            if ecc.is_empty() || emi.is_empty() {
                return Ok((vec!["Alpha".to_string()], Vec::new()));
            }
            if ecc.len() != emi.len() {
                return Err("le eccitazioni e le emissioni non sono lo stesso numero".into());
            }
            let etichette: Vec<String> = ecc.iter().zip(&emi).map(|(x, m)| format!("{prefisso}{x},{m}")).collect();
            if c.polarizzazione {
                if etichette.len() != 2 {
                    return Err("la polarizzazione di fluorescenza vuole due serie di filtri".into());
                }
                let dir = ["Parallel", "Perpendicular"];
                return Ok((etichette.iter().zip(dir).map(|(e, d)| format!("{e} [{d}]")).collect(), Vec::new()));
            }
            let alias: Alias = ecc.iter().zip(&emi).map(|(x, m)| (format!("{prefisso}{x},{m}"), vec![format!("{prefisso}{},{}", x.split('/').next().unwrap_or(""), m.split('/').next().unwrap_or(""))])).collect();
            Ok((etichette, alias))
        }
        Modo::Luminescenza => {
            let emi = prima_non_vuota(&[c.lista("Emission"), c.lista("Fixed Emission"), c.lista("Emission Start")]);
            let v = emi.iter().map(|e| format!("{prefisso}{}", if matches!(e.as_str(), "Full light" | "Plug" | "Hole") { "Lum" } else { e.as_str() })).collect();
            Ok((v, Vec::new()))
        }
    }
}

fn prima_non_vuota(l: &[Vec<String>]) -> Vec<String> {
    l.iter().find(|v| !v.is_empty()).cloned().unwrap_or_default()
}

fn pulisci_onda(w: Option<String>) -> Option<String> {
    w.map(|w| if let Some((a, _)) = w.split_once(" nm") { a.to_string() } else { w })
}

fn filtri(etichette: &[String], alias: &Alias, c: &Controllo, modo: Modo) -> Result<Vec<(String, FiltroSet)>, String> {
    if modo == Modo::Assorbanza {
        return Ok(Vec::new());
    }
    let gains = c.lista("Gain");
    if etichette.len() != gains.len() {
        return Err(format!("il numero delle misure ({}) non è quello dei guadagni ({})", etichette.len(), gains.len()));
    }
    let emissioni = c.lista("Emission");
    let eccitazioni = c.lista("Excitation");
    let fisse_em = c.lista("Fixed Emission");
    let fisse_ecc = c.lista("Fixed Excitation");
    let inizio_em = c.lista("Emission Start");
    let inizio_ecc = c.lista("Excitation Start");
    let specchi = c.lista("Mirror");
    let ottiche = c.lista("Optics");
    let al = |v: &Vec<String>, i: usize| v.get(i).cloned();
    let mut out: Vec<(String, FiltroSet)> = Vec::new();
    for (i, e) in etichette.iter().enumerate() {
        let f = FiltroSet {
            gain: gains[i].clone(),
            emissione: pulisci_onda(al(&emissioni, i).or_else(|| al(&fisse_em, i)).or_else(|| al(&inizio_em, i))),
            eccitazione: pulisci_onda(al(&eccitazioni, i).or_else(|| al(&fisse_ecc, i)).or_else(|| al(&inizio_ecc, i))),
            specchio: if !specchi.is_empty() && modo == Modo::Fluorescenza { al(&specchi, i) } else { None },
            ottica: al(&ottiche, i),
            direzione: c.polarizzazione.then(|| ["Parallel", "Perpendicular"].get(i).copied()).flatten(),
        };
        out.push((e.clone(), f));
    }
    for (e, a) in alias {
        if let Some(f) = out.iter().find(|(x, _)| x == e).map(|(_, f)| f.clone()) {
            for x in a {
                out.push((x.clone(), f.clone()));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Tabelle: layout e risultati
// ---------------------------------------------------------------------------------------------

struct RigaTab {
    lettera: String,
    valori: Vec<String>,
    etichetta: String,
}

/// Le righe di una matrice di Gen5 (la prima è l'intestazione con i numeri delle colonne): la lettera della
/// riga si porta avanti sulle righe che non l'hanno, l'ultima cella è l'etichetta.
fn righe_matrice(righe: &[String]) -> Vec<RigaTab> {
    let mut out = Vec::new();
    let mut lettera = String::new();
    for r in righe.iter().skip(1) {
        let c: Vec<&str> = r.split('\t').collect();
        if c.len() < 2 {
            continue;
        }
        if !c[0].trim().is_empty() {
            lettera = c[0].trim().to_string();
        }
        out.push(RigaTab { lettera: lettera.clone(), valori: c[1..c.len() - 1].iter().map(|x| x.to_string()).collect(), etichetta: c[c.len() - 1].trim().to_string() });
    }
    out
}

fn nomi_dal_layout(righe: &Option<&Vec<String>>) -> (HashMap<String, String>, HashMap<String, Option<f64>>) {
    let mut nomi: HashMap<String, String> = HashMap::new();
    let mut conc: HashMap<String, Option<f64>> = HashMap::new();
    let Some(l) = righe else { return (nomi, conc) };
    if l.len() < 2 {
        return (nomi, conc);
    }
    for r in righe_matrice(&l[1..]) {
        for (i, v) in r.valori.iter().enumerate() {
            let pozzetto = format!("{}{}", r.lettera, i + 1);
            let vuoto = v.trim().is_empty() || mancante(v.trim());
            if r.etichetta == "Conc/Dil" {
                conc.insert(pozzetto.clone(), if vuoto { None } else { float_non_nan(v) });
            }
            if !vuoto && (r.etichetta == "Name" || (r.etichetta == "Well ID" && !nomi.contains_key(&pozzetto))) {
                nomi.insert(pozzetto, v.trim().to_string());
            }
        }
    }
    (nomi, conc)
}

fn temperatura(righe: Option<&Vec<String>>) -> Result<Option<f64>, String> {
    let Some(r) = righe else { return Ok(None) };
    if r.len() != 1 {
        return Err("la temperatura dovrebbe stare su una riga sola".into());
    }
    let t = r[0].trim().split('\t').next_back().unwrap_or("");
    float_stretto(t).map(Some).ok_or_else(|| format!("temperatura non leggibile: «{t}»"))
}

/// La tabella dei risultati: «Results» se c'è, altrimenti le tabelle con la forma dei risultati unite riga per riga.
fn tabella_risultati(s: &Sezioni) -> Vec<String> {
    if let Some(r) = s.sezione("Results") {
        return r.clone();
    }
    let e_risultati = |sec: &Vec<String>| sec.len() > 2 && sec[1].starts_with("\t1") && sec[2].starts_with("A\t");
    let tabelle: Vec<&Vec<String>> = s.sezioni.iter().filter(|(n, sec)| n != "Layout" && e_risultati(sec)).map(|(_, sec)| sec).collect();
    if tabelle.is_empty() {
        return Vec::new();
    }
    let mut out = vec!["Results".to_string(), tabelle[0][1].clone()];
    for i in 1..tabelle[0].len() - 1 {
        for t in &tabelle {
            if let Some(r) = t.get(i + 1) {
                out.push(r.clone());
            }
        }
    }
    out
}

fn tabelle_per_lettura(s: &Sezioni, indice: usize, etichette: &[String]) -> Vec<String> {
    let mut ordinate: Vec<&String> = etichette.iter().collect();
    ordinate.sort_by(|a, b| (a.len(), a.as_str()).cmp(&(b.len(), b.as_str())));
    let tabelle: Vec<&[String]> = ordinate.iter().filter_map(|e| s.sezione(&format!("Read {indice}:{e}"))).map(|r| &r[1..]).collect();
    if tabelle.is_empty() {
        return tabella_risultati(s);
    }
    let mut out = vec!["Results".to_string(), tabelle[0][0].clone()];
    for i in 0..tabelle[0].len() - 1 {
        for t in &tabelle {
            if let Some(r) = t.get(i + 1) {
                out.push(r.clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// La cinetica
// ---------------------------------------------------------------------------------------------

struct DatiCinetici {
    /// Per ogni pozzetto i valori nel tempo (NaN dove manca).
    pozzetti: Vec<(String, Vec<f64>)>,
    tempi: Vec<f64>,
    errori: Vec<(String, Vec<(String, String)>)>,
}

fn dati_cinetici(righe: &[String]) -> Result<Option<DatiCinetici>, String> {
    if righe.is_empty() {
        return Ok(None);
    }
    let testa: Vec<&str> = righe[0].split('\t').collect();
    let corpo: Vec<Vec<&str>> = righe[1..].iter().filter(|r| !r.is_empty()).map(|r| r.split('\t').collect()).collect();
    fn cella<'a>(r: &[&'a str], c: usize) -> &'a str {
        r.get(c).copied().unwrap_or("").trim()
    }
    // Le colonne tutte vuote e le righe tutte vuote si buttano.
    let colonne: Vec<usize> = (1..testa.len()).filter(|&c| corpo.iter().any(|r| !mancante(cella(r, c)))).collect();
    let righe_buone: Vec<&Vec<&str>> = corpo.iter().filter(|r| colonne.iter().any(|&c| !mancante(cella(r, c)))).collect();
    let mut tempi = Vec::new();
    for r in &righe_buone {
        tempi.push(tempo_in_secondi(cella(r, 0))?);
    }
    let mut pozzetti = Vec::new();
    let mut errori = Vec::new();
    for &c in &colonne {
        let nome = testa[c].trim().to_string();
        // La temperatura sta in una colonna «T° 600» o «T∞ 600».
        let mut ch = nome.chars();
        if ch.next() == Some('T') && ch.next().is_some_and(|x| !x.is_ascii_alphanumeric()) {
            continue;
        }
        let mut valori = Vec::new();
        let mut errs: Vec<(String, String)> = Vec::new();
        for (i, r) in righe_buone.iter().enumerate() {
            let t = cella(r, c);
            if mancante(t) {
                valori.push(f64::NAN);
                continue;
            }
            match float_stretto(t) {
                Some(v) => valori.push(v),
                None => {
                    errs.push((t.to_string(), format!("{}s", secondi_testo(tempi[i]))));
                    valori.push(f64::NAN);
                }
            }
        }
        if !errs.is_empty() {
            errori.push((nome.clone(), errs));
        }
        pozzetti.push((nome, valori));
    }
    Ok(Some(DatiCinetici { pozzetti, tempi, errori }))
}

// ---------------------------------------------------------------------------------------------
// Il contesto di una lettura
// ---------------------------------------------------------------------------------------------

struct Contesto {
    letture: Vec<Lett>,
    cinetica: Option<Cinetica>,
    risultati: Vec<String>,
    dati_cin: Option<DatiCinetici>,
}

impl Contesto {
    fn ha_misure_cinetiche(&self) -> bool {
        self.dati_cin.as_ref().is_some_and(|d| !d.pozzetti.is_empty() && !d.tempi.is_empty())
    }
}

struct Lavoro<'a> {
    s: &'a Sezioni,
    letture: Vec<Lett>,
    cinetica: Option<Cinetica>,
}

impl Lavoro<'_> {
    fn contesto(&self, lett: &Lett, indice: usize, specifica: Option<&str>) -> Result<Contesto, String> {
        let da_fare: Vec<&String> = match specifica {
            Some(s) => lett.etichette.iter().filter(|e| e.as_str() == s).collect(),
            None => lett.etichette.iter().collect(),
        };
        let mut dati_cin: Option<DatiCinetici> = None;
        let mut ha_tempo = false;
        let mut ha_risultati = false;
        for e in &da_fare {
            let chiave = format!("Read {indice}:{e}");
            let righe = self.s.cinetica(&chiave).or_else(|| self.s.cinetica(e));
            if let Some(r) = righe.filter(|r| !r.is_empty()) {
                ha_tempo = true;
                if let Some(d) = dati_cinetici(r)? {
                    if dati_cin.is_none() {
                        dati_cin = Some(d);
                    }
                }
            }
            if self.s.sezione(&chiave).is_some() {
                ha_risultati = true;
            }
        }
        let etichette: Vec<String> = da_fare.iter().map(|e| (*e).clone()).collect();
        let (risultati, combinati) = if ha_tempo {
            if !ha_risultati {
                (tabella_risultati(self.s), false)
            } else {
                (tabelle_per_lettura(self.s, indice, &etichette), false)
            }
        } else if ha_risultati {
            (tabelle_per_lettura(self.s, indice, &etichette), false)
        } else {
            let r = tabella_risultati(self.s);
            let c = !r.is_empty();
            (r, c)
        };
        let mut l = lett.clone();
        if let Some(s) = specifica {
            l.etichette = vec![s.to_string()];
        }
        Ok(Contesto { letture: if combinati { self.letture.clone() } else { vec![l] }, cinetica: if ha_tempo { self.cinetica.clone() } else { None }, risultati, dati_cin })
    }
}

// ---------------------------------------------------------------------------------------------
// Le misure
// ---------------------------------------------------------------------------------------------

fn arrotonda_pozzetti(n: usize) -> u32 {
    for p in [1usize, 2, 4, 6, 8, 12, 24, 48, 72, 96, 384, 1536, 3456] {
        if n <= p {
            return p as u32;
        }
    }
    n as u32
}

fn lettera_pozzetto(p: &str) -> (String, u32) {
    let l: String = p.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let n: u32 = p[l.len()..].parse().unwrap_or(0);
    (l, n)
}

/// Le cifre di un'etichetta che dice la lunghezza d'onda: «260:977 [Test]» → 977.
fn onda_da_etichetta(e: &str) -> Option<f64> {
    float_stretto(e.rsplit(':').next()?.split(' ').next()?)
}

fn lett_per_etichetta<'a>(letture: &'a [Lett], etichetta: &str) -> Result<&'a Lett, String> {
    let cifre = etichetta.find('[').and_then(|i| etichetta[i + 1..].split(']').next()).filter(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit()));
    letture
        .iter()
        .find(|r| r.etichette.iter().any(|e| e == etichetta) || cifre.is_some_and(|c| r.etichette.iter().any(|e| e == c)))
        .ok_or_else(|| format!("per la misura «{etichetta}» non c'è la lettura"))
}

struct Comune<'a> {
    hdr: &'a Intestazione,
    nomi: &'a HashMap<String, String>,
    conc: &'a HashMap<String, Option<f64>>,
    temperatura: Option<f64>,
}

impl Comune<'_> {
    fn piastra(&self) -> String {
        self.hdr.piastra.clone().unwrap_or_default()
    }

    fn campione(&self, pozzetto: &str) -> String {
        self.nomi.get(pozzetto).cloned().unwrap_or_else(|| format!("{} {pozzetto}", self.piastra()))
    }

    /// Una misura del pozzetto con quello che hanno in comune tutte: campione, piastra, temperatura, note.
    fn base(&self, pozzetto: &str) -> Misura {
        let mut m = Misura { campione: self.campione(pozzetto), pozzetto: Some(pozzetto.to_string()), piastra: Some(self.piastra()).filter(|p| !p.is_empty()), ..Default::default() };
        if let Some(Some(c)) = self.conc.get(pozzetto) {
            m.valori.push(Valore::num("Conc/Dil", *c, "", Posto::CampioneAltro));
        }
        if let Some(n) = &self.hdr.numero_piastra {
            m.valori.push(Valore::testo("Plate Number", n, Posto::CampioneAltro));
        }
        m
    }

    /// Le impostazioni della lettura, uguali per ogni pozzetto.
    fn impostazioni(&self, m: &mut Misura, r: &Lett, f: Option<&FiltroSet>, onda_assorbanza: Option<f64>) -> Result<(), String> {
        m.valori.push(Valore::testo("detection type", r.modo.nome(), Posto::Impostazione));
        let onda = f.map(|f| f.onda_rivelatore()).unwrap_or(onda_assorbanza);
        if let Some(w) = onda.or(onda_assorbanza) {
            m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
        }
        if let Some(f) = f {
            if let Some(b) = f.banda_rivelatore() {
                m.valori.push(Valore::num("detector bandwidth setting", b, "nm", Posto::Impostazione));
            }
            if let Some(w) = f.onda_eccitazione() {
                m.valori.push(Valore::num("excitation wavelength setting", w, "nm", Posto::Impostazione));
            }
            if let Some(b) = f.banda_eccitazione()? {
                m.valori.push(Valore::num("excitation bandwidth setting", b, "nm", Posto::Impostazione));
            }
            if let Some(t) = f.taglio()? {
                m.valori.push(Valore::num("wavelength filter cutoff setting", t, "nm", Posto::Impostazione));
            }
            if let Some(p) = f.posizione() {
                m.valori.push(Valore::testo("scan position setting (plate reader)", p, Posto::Impostazione));
            }
            if !f.gain.is_empty() {
                m.valori.push(Valore::testo("detector gain setting", &f.gain, Posto::Impostazione));
            }
            if let Some(d) = f.direzione {
                m.valori.push(Valore::testo("Direction of emitted light", d, Posto::ImpostazioneAltro));
            }
        }
        if let Some(d) = r.distanza {
            m.valori.push(Valore::num("detector distance setting (plate reader)", d, "mm", Posto::Impostazione));
        }
        if let Some(n) = r.medie {
            m.valori.push(Valore::num("number of averages", n, "#", Posto::Impostazione));
        }
        if let Some(v) = &r.velocita {
            m.valori.push(Valore::testo("detector carriage speed setting", v, Posto::Impostazione));
        }
        if let Some(t) = &self.hdr.tipo_lettura {
            m.valori.push(Valore::testo("Reading Type", t, Posto::ImpostazioneAltro));
        }
        if let Some(t) = self.temperatura {
            m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Misura));
        }
        for (k, v) in &self.hdr.altri {
            m.valori.push(Valore::testo(k, v, Posto::Altro));
        }
        Ok(())
    }
}

/// Dov'è il gruppo di una lettura e di un'etichetta: un gruppo per lettura ed etichetta.
struct Gruppi {
    chiavi: Vec<(usize, String)>,
}

impl Gruppi {
    fn indice(&mut self, l: &mut Lettura, quando: &Option<String>, lettura: usize, etichetta: &str, titolo: String) -> usize {
        if let Some(i) = self.chiavi.iter().position(|(r, e)| *r == lettura && e == etichetta) {
            return i;
        }
        self.chiavi.push((lettura, etichetta.to_string()));
        l.gruppi.push(Gruppo { titolo, quando: quando.clone(), ..Default::default() });
        l.gruppi.len() - 1
    }
}

struct Stato<'a> {
    comune: Comune<'a>,
    quando: Option<String>,
    gruppi: Gruppi,
    /// Le misure di ogni pozzetto: (etichetta, identificativo, modo).
    per_pozzetto: Vec<(String, Vec<(String, String, Modo)>)>,
    calcolati: Vec<Calcolato>,
    /// Gli errori che appartengono a una misura già scritta: (identificativo, errore).
    errori_tardivi: Vec<(String, (String, String))>,
}

impl Stato<'_> {
    fn registra(&mut self, pozzetto: &str, etichetta: &str, id: &str, modo: Modo) {
        match self.per_pozzetto.iter_mut().find(|(p, _)| p == pozzetto) {
            Some((_, v)) => v.push((etichetta.to_string(), id.to_string(), modo)),
            None => self.per_pozzetto.push((pozzetto.to_string(), vec![(etichetta.to_string(), id.to_string(), modo)])),
        }
    }

    fn misure_del_pozzetto(&self, p: &str) -> Vec<(String, String, Modo)> {
        self.per_pozzetto.iter().find(|(x, _)| x == p).map(|(_, v)| v.clone()).unwrap_or_default()
    }
}

fn titolo_gruppo(r: &Lett, etichetta: &str, indice: usize, molte: bool) -> String {
    let base = if r.modo == Modo::Assorbanza && onda_da_etichetta(etichetta).is_some() {
        let coda = etichetta.rsplit(':').next().unwrap_or(etichetta);
        match coda.split_once(' ') {
            Some((onda, resto)) => format!("{onda} nm {resto}"),
            None => format!("{coda} nm"),
        }
    } else {
        etichetta.to_string()
    };
    if molte {
        format!("Lettura {indice} · {base}")
    } else {
        base
    }
}

/// Il numero come lo scrive Python per un float.
fn py_float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e16 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

struct Calcolo {
    nome: String,
    valore: f64,
}

/// I valori che Gen5 ha calcolato (e le righe che non sono misure): per pozzetto.
fn aggiungi_calcolo(v: &mut Vec<(String, Vec<Calcolo>)>, pozzetto: &str, nome: &str, valore: f64) {
    let c = Calcolo { nome: nome.to_string(), valore };
    match v.iter_mut().find(|(p, _)| p == pozzetto) {
        Some((_, x)) => x.push(c),
        None => v.push((pozzetto.to_string(), vec![c])),
    }
}

fn aggiungi_errore(v: &mut Vec<(String, Vec<(String, String)>)>, pozzetto: &str, e: (String, String)) {
    match v.iter_mut().find(|(p, _)| p == pozzetto) {
        Some((_, x)) => x.push(e),
        None => v.push((pozzetto.to_string(), vec![e])),
    }
}

/// I calcoli di Gen5 diventano «calcolati» con le misure da cui vengono.
fn scrivi_calcoli(st: &mut Stato, calcoli: Vec<(String, Vec<Calcolo>)>, fonte_fissa: Option<&str>) {
    for (p, v) in calcoli {
        let misure = st.misure_del_pozzetto(&p);
        if misure.is_empty() {
            continue;
        }
        for c in v {
            // Da dove viene: le misure con l'etichetta dentro il nome del calcolo; la lunghezza del cammino
            // ottico viene dalle misure «Test» e «Ref»; altrimenti da tutte.
            let origine: Vec<&(String, String, Modo)> = if c.nome.rsplit(':').next() == Some("Pathlength") {
                misure.iter().filter(|(e, _, _)| e.rsplit(' ').next().is_some_and(|s| s == "[Test]" || s == "[Ref]")).collect()
            } else {
                misure.iter().filter(|(e, _, _)| c.nome.contains(e.as_str())).collect()
            };
            let origine: Vec<&(String, String, Modo)> = if origine.is_empty() { misure.iter().collect() } else { origine };
            // Ogni misura una volta sola, con la grandezza che ha dato.
            let mut sorgenti: Vec<String> = Vec::new();
            for (_, id, _) in &origine {
                if !sorgenti.contains(id) {
                    sorgenti.push(id.clone());
                }
            }
            let da = fonte_fissa.map(str::to_string).unwrap_or_else(|| origine[0].2.minuscolo().to_string());
            st.calcolati.push(Calcolato { nome: c.nome, valore: c.valore, unita: String::new(), sorgenti, da, descrizione: String::new(), da_calcoli: Vec::new() });
        }
    }
}

/// Le misure in punto finale (la matrice dei risultati).
fn elabora_standard(l: &mut Lettura, st: &mut Stato, ctx: &Contesto, indice: usize, molte_letture: bool) -> Result<(), String> {
    if ctx.risultati.first().is_none_or(|r| r.trim() != "Results") {
        return Err("nel file mancano le tabelle dei risultati".into());
    }
    let righe = righe_matrice(&ctx.risultati[1..]);
    let etichette: Vec<&str> = ctx.letture.iter().flat_map(|r| r.etichette.iter().map(String::as_str)).collect();
    // Il numero dei pozzetti: righe diverse per colonne.
    let mut lettere: Vec<&str> = righe.iter().map(|r| r.lettera.as_str()).collect();
    lettere.sort_unstable();
    lettere.dedup();
    let colonne = ctx.risultati.get(1).map(|h| h.split('\t').filter(|c| !c.trim().is_empty()).count()).unwrap_or(0);
    let pozzetti = arrotonda_pozzetti(lettere.len() * colonne);
    let mut errori_calcoli: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut calcoli: Vec<(String, Vec<Calcolo>)> = Vec::new();
    for r in &righe {
        // Un'etichetta che sta tra le misure («Read 1:450» vale 450).
        let etichetta = match r.etichetta.strip_prefix("Read ").and_then(|t| t.split_once(':')) {
            Some((_, e)) => e.to_string(),
            None => r.etichetta.clone(),
        };
        let e_misura = etichette.contains(&etichetta.as_str());
        for (ci, v) in r.valori.iter().enumerate() {
            let pozzetto = format!("{}{}", r.lettera, ci + 1);
            let valore = numero_o_vuoto(v);
            if matches!(valore, Some(x) if x.is_nan()) {
                continue;
            }
            if e_misura {
                let lett = lett_per_etichetta(&ctx.letture, &etichetta)?.clone();
                let mut m = st.comune.base(&pozzetto);
                let f = if lett.modo == Modo::Assorbanza { None } else { Some(lett.filtro(&etichetta).ok_or_else(|| format!("per «{etichetta}» non ci sono i filtri"))?) };
                let onda_ass = if lett.modo == Modo::Assorbanza { onda_da_etichetta(&etichetta) } else { None };
                st.comune.impostazioni(&mut m, &lett, f, onda_ass)?;
                let chiave = lett.modo.minuscolo();
                let chiave = if lett.modo == Modo::Alpha { "fluorescence" } else { chiave };
                m.valori.push(match valore {
                    Some(x) => Valore::num(chiave, x, lett.modo.unita(), Posto::Misura),
                    None => {
                        m.note.push((v.trim().to_string(), lett.modo.minuscolo().to_string()));
                        Valore { chiave: chiave.into(), dato: Dato::Assente(v.trim().to_string()), unita: lett.modo.unita().into(), posto: Posto::Misura }
                    }
                });
                m.id = l.id();
                let gi = st.gruppi.indice(l, &st.quando, indice, &etichetta, titolo_gruppo(&lett, &etichetta, indice, molte_letture));
                l.gruppi[gi].pozzetti = Some(pozzetti);
                l.gruppi[gi].piastra = m.piastra.clone();
                st.registra(&pozzetto, &etichetta, &m.id, lett.modo);
                l.gruppi[gi].misure.push(m);
            } else {
                match valore {
                    Some(x) => aggiungi_calcolo(&mut calcoli, &pozzetto, &etichetta, x),
                    None => aggiungi_errore(&mut errori_calcoli, &pozzetto, (v.trim().to_string(), etichetta.clone())),
                }
            }
        }
    }
    // Gli errori dei valori calcolati vanno alla prima misura del pozzetto.
    for (p, errs) in errori_calcoli {
        if let Some((_, id, _)) = st.misure_del_pozzetto(&p).first() {
            for e in errs {
                st.errori_tardivi.push((id.clone(), e));
            }
        }
    }
    scrivi_calcoli(st, calcoli, None);
    Ok(())
}

fn cubo_cinetica(r: &Lett, etichetta: &str, onda: Option<f64>) -> Curva {
    Curva { chiave: r.modo.cubo_cinetica().into(), etichetta: onda.map(py_float).unwrap_or_else(|| etichetta.to_string()), cinetica: true, concetto: r.modo.minuscolo().replace("alpha", "fluorescence"), unita: r.modo.unita().into() }
}

/// Le cinetiche: un pozzetto, una serie di valori nel tempo.
fn elabora_cinetica(l: &mut Lettura, st: &mut Stato, ctx: &Contesto, indice: usize, molte_letture: bool) -> Result<(), String> {
    let cin = ctx.cinetica.as_ref().ok_or("la cinetica non ha i dati dei tempi")?;
    let dati = ctx.dati_cin.as_ref().ok_or("nel file ci sono i dati della cinetica ma non le misure")?;
    if !ctx.ha_misure_cinetiche() {
        return Err("nel file ci sono i dati della cinetica ma non le misure".into());
    }
    let etichette: Vec<&String> = ctx.letture.iter().flat_map(|r| r.etichette.iter()).collect();
    let etichetta = etichette.first().map(|e| e.to_string()).ok_or("la cinetica non ha etichette")?;
    let lett = lett_per_etichetta(&ctx.letture, &etichetta)?.clone();
    // I calcoli di Gen5 (pendenze, tempi di ritardo…) stanno nella tabella dei risultati.
    let mut calcoli: Vec<(String, Vec<Calcolo>)> = Vec::new();
    if ctx.risultati.first().is_some_and(|r| r.trim() == "Results") {
        for r in righe_matrice(&ctx.risultati[1..]) {
            for (ci, v) in r.valori.iter().enumerate() {
                let x = numero_o_vuoto(v);
                // Come allotropy: i valori che non sono numeri e gli zeri non sono calcoli.
                if let Some(x) = x.filter(|x| !x.is_nan() && *x != 0.0) {
                    aggiungi_calcolo(&mut calcoli, &format!("{}{}", r.lettera, ci + 1), &r.etichetta, x);
                }
            }
        }
    }
    let f = if lett.modo == Modo::Assorbanza { None } else { Some(lett.filtro(&etichetta).ok_or_else(|| format!("per «{etichetta}» non ci sono i filtri"))?) };
    let onda_ass = if lett.modo == Modo::Assorbanza { onda_da_etichetta(&etichetta) } else { None };
    let onda = f.and_then(|f| f.onda_rivelatore()).or(onda_ass);
    let gi = st.gruppi.indice(l, &st.quando, indice, &etichetta, titolo_gruppo(&lett, &etichetta, indice, molte_letture));
    let nomi: Vec<&str> = dati.pozzetti.iter().map(|(p, _)| p.as_str()).collect();
    let (mut max_l, mut max_c) = (0u8, 0u32);
    for p in &nomi {
        let (le, c) = lettera_pozzetto(p);
        max_l = max_l.max(le.bytes().next().unwrap_or(b'A').to_ascii_uppercase());
        max_c = max_c.max(c);
    }
    let conteggio = arrotonda_pozzetti((max_l.saturating_sub(b'A') as usize + 1) * max_c as usize);
    l.gruppi[gi].pozzetti = Some(conteggio);
    for (pozzetto, valori) in &dati.pozzetti {
        let mut m = st.comune.base(pozzetto);
        st.comune.impostazioni(&mut m, &lett, f, onda_ass)?;
        m.valori.push(Valore::num("total measurement time setting", tempo_in_secondi(&cin.durata)?, "s", Posto::Impostazione));
        m.valori.push(Valore::num("read interval setting", tempo_in_secondi(&cin.intervallo)?, "s", Posto::Impostazione));
        m.valori.push(Valore::num("number of scans setting", cin.letture as f64, "#", Posto::Impostazione));
        m.spettro = dati.tempi.iter().copied().zip(valori.iter().copied()).collect();
        m.curva = Some(cubo_cinetica(&lett, &etichetta, onda));
        if let Some((_, errs)) = dati.errori.iter().find(|(p, _)| p == pozzetto) {
            m.note.extend(errs.clone());
        }
        m.piastra = m.piastra.clone();
        m.id = l.id();
        l.gruppi[gi].piastra = m.piastra.clone();
        st.registra(pozzetto, &etichetta, &m.id, lett.modo);
        l.gruppi[gi].misure.push(m);
    }
    scrivi_calcoli(st, calcoli, Some(lett.modo.cubo_cinetica()));
    Ok(())
}

/// Gli spettri: un pozzetto, un valore per lunghezza d'onda.
fn elabora_spettro(l: &mut Lettura, st: &mut Stato, s: &Sezioni, ctx: &Contesto, indice: usize, molte_letture: bool) -> Result<(), String> {
    let sez = s.sezione("Wavelength").ok_or("manca la sezione «Wavelength»")?;
    let lett = ctx.letture.first().ok_or("manca la lettura")?.clone();
    let testa: Vec<&str> = sez[0].split('\t').collect();
    if testa.first().map(|t| t.trim()) != Some("Wavelength") {
        return Err("la tabella degli spettri non comincia con «Wavelength»".into());
    }
    let corpo: Vec<Vec<&str>> = sez[1..].iter().map(|r| r.split('\t').collect()).collect();
    let mut onde = Vec::new();
    for r in &corpo {
        onde.push(float_stretto(r.first().copied().unwrap_or("")).ok_or("le lunghezze d'onda degli spettri non sono numeri")?);
    }
    let (cubo, nome_cubo, concetto) = match (lett.modo, lett.eccitazione && !lett.emissione) {
        (Modo::Assorbanza, _) => ("absorption spectrum data cube", "absorbance-spectrum", "absorbance"),
        (Modo::Fluorescenza | Modo::Alpha, true) => ("fluorescence excitation spectrum data cube", "fluorescence-spectrum", "fluorescence"),
        (Modo::Fluorescenza | Modo::Alpha, false) => ("fluorescence emission spectrum data cube", "fluorescence-spectrum", "fluorescence"),
        (Modo::Luminescenza, true) => ("fluorescence excitation spectrum data cube", "luminescence-spectrum", "luminescence"),
        (Modo::Luminescenza, false) => ("fluorescence emission spectrum data cube", "luminescence-spectrum", "luminescence"),
    };
    let filtro = lett.filtri.first().map(|(_, f)| f);
    let gi = st.gruppi.indice(l, &st.quando, indice, "spettro", if molte_letture { format!("Lettura {indice} · spettri") } else { lett.etichette.first().cloned().unwrap_or_else(|| "spettri".into()) });
    let pozzetti_testa: Vec<&str> = testa.iter().skip(1).map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
    let (mut max_l, mut max_c) = (0u8, 0u32);
    for p in &pozzetti_testa {
        let (le, c) = lettera_pozzetto(p);
        max_l = max_l.max(le.bytes().next().unwrap_or(b'A').to_ascii_uppercase());
        max_c = max_c.max(c);
    }
    l.gruppi[gi].pozzetti = Some(arrotonda_pozzetti((max_l.saturating_sub(b'A') as usize + 1) * max_c as usize));
    for (ci, pozzetto) in testa.iter().enumerate().skip(1) {
        let pozzetto = pozzetto.trim();
        if pozzetto.is_empty() {
            continue;
        }
        let mut punti: Vec<(f64, f64)> = Vec::new();
        let mut errori: Vec<(String, String)> = Vec::new();
        for (ri, r) in corpo.iter().enumerate() {
            let t = r.get(ci).copied().unwrap_or("").trim();
            if mancante(t) {
                continue;
            }
            match float_stretto(t) {
                Some(v) => punti.push((onde[ri], v)),
                None => {
                    errori.push((t.to_string(), format!("{}nm", py_float(onde[ri]))));
                    punti.push((onde[ri], f64::NAN));
                }
            }
        }
        if punti.is_empty() {
            continue;
        }
        let mut m = st.comune.base(pozzetto);
        m.valori.push(Valore::testo("detection type", lett.modo.nome(), Posto::Impostazione));
        if let Some(f) = filtro {
            if lett.eccitazione && !lett.emissione {
                if let Some(w) = f.onda_rivelatore() {
                    m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
                }
            }
            if let Some(b) = f.banda_rivelatore() {
                m.valori.push(Valore::num("detector bandwidth setting", b, "nm", Posto::Impostazione));
            }
            if lett.emissione {
                if let Some(w) = f.onda_eccitazione() {
                    m.valori.push(Valore::num("excitation wavelength setting", w, "nm", Posto::Impostazione));
                }
            }
            if let Some(b) = f.banda_eccitazione()? {
                m.valori.push(Valore::num("excitation bandwidth setting", b, "nm", Posto::Impostazione));
            }
            if let Some(p) = f.posizione() {
                m.valori.push(Valore::testo("scan position setting (plate reader)", p, Posto::Impostazione));
            }
            if !f.gain.is_empty() {
                m.valori.push(Valore::testo("detector gain setting", &f.gain, Posto::Impostazione));
            }
        }
        if let Some(d) = lett.distanza {
            m.valori.push(Valore::num("detector distance setting (plate reader)", d, "mm", Posto::Impostazione));
        }
        if let Some(n) = lett.medie {
            m.valori.push(Valore::num("number of averages", n, "#", Posto::Impostazione));
        }
        if let Some(v) = &lett.velocita {
            m.valori.push(Valore::testo("detector carriage speed setting", v, Posto::Impostazione));
        }
        if let Some(t) = &st.comune.hdr.tipo_lettura {
            m.valori.push(Valore::testo("Reading Type", t, Posto::ImpostazioneAltro));
        }
        if let Some(t) = st.comune.temperatura {
            m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Misura));
        }
        for (k, v) in &st.comune.hdr.altri {
            m.valori.push(Valore::testo(k, v, Posto::Altro));
        }
        m.spettro = punti;
        m.curva = Some(Curva { chiave: cubo.into(), etichetta: nome_cubo.into(), cinetica: false, concetto: concetto.into(), unita: lett.modo.unita().into() });
        m.note.extend(errori);
        m.id = l.id();
        l.gruppi[gi].piastra = m.piastra.clone();
        st.registra(pozzetto, "spettro", &m.id, lett.modo);
        l.gruppi[gi].misure.push(m);
    }
    // I calcoli di Gen5 sugli spettri (massimi, aree…): le righe della tabella dei risultati che non sono misure.
    let mut calcoli: Vec<(String, Vec<Calcolo>)> = Vec::new();
    let mut errori_calcoli: Vec<(String, Vec<(String, String)>)> = Vec::new();
    if ctx.risultati.first().is_some_and(|r| r.trim() == "Results") {
        for r in righe_matrice(&ctx.risultati[1..]) {
            if lett.etichette.contains(&r.etichetta) {
                continue;
            }
            for (ci, v) in r.valori.iter().enumerate() {
                let pozzetto = format!("{}{}", r.lettera, ci + 1);
                match numero_o_vuoto(v) {
                    Some(x) if x.is_nan() => {}
                    Some(x) => aggiungi_calcolo(&mut calcoli, &pozzetto, &r.etichetta, x),
                    None => {
                        aggiungi_errore(&mut errori_calcoli, &pozzetto, (v.trim().to_string(), r.etichetta.clone()));
                        aggiungi_calcolo(&mut calcoli, &pozzetto, &r.etichetta, -0.0);
                    }
                }
            }
        }
    }
    for (p, errs) in errori_calcoli {
        if let Some((_, id, _)) = st.misure_del_pozzetto(&p).first() {
            for e in errs {
                st.errori_tardivi.push((id.clone(), e));
            }
        }
    }
    scrivi_calcoli(st, calcoli, Some(lett.modo.minuscolo()));
    Ok(())
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let s = dividi(&f.testo)?;
    let hdr = intestazione(&s, &f.nome)?;
    let procedura = s.sezione("Procedure Details").ok_or("manca la sezione «Procedure Details»")?;
    let letture = letture_da_procedura(procedura)?;
    let cinetica = cinetica_da_procedura(procedura)?;
    let (nomi, conc) = nomi_dal_layout(&s.sezione("Layout"));
    let temp = temperatura(s.sezione("Actual Temperature"))?;
    let quando = hdr.quando.clone();

    l.strumento.produttore = "Agilent BioTek".into();
    l.strumento.modello = hdr.modello.clone().unwrap_or_default();
    l.strumento.seriale = hdr.seriale.clone().unwrap_or_default();
    l.strumento.software = "Gen5".into();
    l.strumento.versione = hdr.versione.clone();
    if let Some(q) = &quando {
        l.info.push(("Data della misura".into(), q.clone()));
    }
    if let Some(p) = &hdr.protocollo {
        l.info.push(("Protocollo".into(), p.clone()));
    }
    if let Some(e) = &hdr.esperimento {
        l.info.push(("Esperimento".into(), e.clone()));
    }
    if let Some(p) = &hdr.piastra {
        l.info.push(("Piastra".into(), p.clone()));
    }

    let lavoro = Lavoro { s: &s, letture: letture.clone(), cinetica: cinetica.clone() };
    let mut st = Stato { comune: Comune { hdr: &hdr, nomi: &nomi, conc: &conc, temperatura: temp }, quando: quando.clone(), gruppi: Gruppi { chiavi: Vec::new() }, per_pozzetto: Vec::new(), calcolati: Vec::new(), errori_tardivi: Vec::new() };
    let molte = letture.len() > 1;
    let mut combinate: Vec<usize> = Vec::new();
    for (i, lett) in letture.iter().enumerate() {
        let indice = i + 1;
        let prova = lavoro.contesto(lett, indice, None)?;
        let cinetica_ok = prova.ha_misure_cinetiche();
        if cinetica_ok && lett.etichette.len() > 1 {
            // Una lettura cinetica con più filtri: ognuno ha la sua tabella dei tempi.
            let mut ordinate = lett.etichette.clone();
            ordinate.sort_by(|a, b| (a.len(), a.as_str()).cmp(&(b.len(), b.as_str())));
            for e in ordinate {
                let sub = lavoro.contesto(lett, indice, Some(&e))?;
                elabora(l, &mut st, &s, &sub, indice, molte)?;
            }
        } else {
            let combinata = !cinetica_ok && !prova.risultati.is_empty() && tabella_risultati(&s) == prova.risultati;
            if combinata {
                combinate.push(indice);
            } else {
                elabora(l, &mut st, &s, &prova, indice, molte)?;
            }
        }
    }
    if let Some(&primo) = combinate.first() {
        let ctx = lavoro.contesto(&letture[primo - 1], primo, None)?;
        elabora(l, &mut st, &s, &ctx, primo, molte)?;
    }
    if l.gruppi.is_empty() {
        return Err("i dati non contengono misure valide".into());
    }
    // Gli errori arrivati dopo, sulla misura giusta.
    for (id, e) in std::mem::take(&mut st.errori_tardivi) {
        for g in l.gruppi.iter_mut() {
            if let Some(m) = g.misure.iter_mut().find(|m| m.id == id) {
                m.note.push(e.clone());
            }
        }
    }
    l.calcolati.extend(std::mem::take(&mut st.calcolati));
    for g in l.gruppi.iter_mut() {
        if let Some(p) = &hdr.protocollo {
            g.dati.push(("analytical method identifier".into(), p.clone()));
        }
        if let Some(e) = &hdr.esperimento {
            g.dati.push(("experimental data identifier".into(), e.clone()));
        }
    }
    // Il valore principale: la misura, o, per le cinetiche e gli spettri, l'ultimo punto o il massimo.
    let primo_valore = l.misure().flat_map(|m| m.valori.iter()).find(|v| v.posto == Posto::Misura && v.chiave != "compartment temperature" && matches!(v.dato, Dato::Numero(_) | Dato::Assente(_)));
    match primo_valore.map(|v| (v.chiave.clone(), v.unita.clone())) {
        Some((k, u)) => {
            l.principale = k;
            l.unita = u;
        }
        None => {
            l.principale = PRINCIPALE_CURVA.into();
            l.unita = l.curva().map(|c| c.unita.clone()).unwrap_or_default();
        }
    }
    Ok(())
}

fn elabora(l: &mut Lettura, st: &mut Stato, s: &Sezioni, ctx: &Contesto, indice: usize, molte: bool) -> Result<(), String> {
    let spettrale = ctx.letture.first().is_some_and(|r| r.tipo == TipoLettura::Spettro) && s.sezione("Wavelength").is_some();
    if spettrale {
        return elabora_spettro(l, st, s, ctx, indice, molte);
    }
    if ctx.cinetica.is_some() {
        return elabora_cinetica(l, st, ctx, indice, molte);
    }
    elabora_standard(l, st, ctx, indice, molte)
}
