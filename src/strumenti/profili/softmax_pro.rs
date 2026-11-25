//! Molecular Devices SoftMax Pro: il file di testo (TXT) che il programma esporta. Comincia con
//! «##BLOCKS= N» e ha N blocchi chiusi da «~End»: le piastre («Plate:», assorbanza, fluorescenza o
//! luminescenza, in punto finale, cinetica o spettro, scritte «a piastra» o «a colonne»), i gruppi
//! («Group:», i campioni con i loro pozzetti e i calcoli del programma) e le note. L'ultima riga dice
//! quando il file è stato salvato.

use std::collections::{HashMap, HashSet};

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Curva, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore, PRINCIPALE_CURVA};

pub const PROFILO: Profilo = Profilo {
    id: "moldev_softmax_pro",
    nome: "Molecular Devices SoftMax Pro",
    produttore: "Molecular Devices",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["txt"],
    riconosci,
    leggi,
};

fn riconosci(f: &Fonte) -> u8 {
    let prima = f.testo.lines().map(|r| r.trim_start_matches('\u{feff}').trim()).find(|r| !r.is_empty());
    match prima {
        Some(r) if r.starts_with("##BLOCKS=") => 98,
        _ => 0,
    }
}

const VERSIONE_ESPORTAZIONE: &str = "1.3";
const VALORI_NON_NUMERO: [&str; 3] = ["Masked", "Range?", "Error"];

// ---------------------------------------------------------------------------------------------
// Piccoli aiuti
// ---------------------------------------------------------------------------------------------

/// Il numero come lo scrive Python per un float («525.0»).
fn py_float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e16 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// «A», «B»… «Z», «AA»: la lettera della riga di una piastra (0 è A).
fn lettere(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Colonne e righe di una piastra da tanti pozzetti.
fn dimensioni(pozzetti: usize) -> Option<(usize, usize)> {
    Some(match pozzetti {
        6 => (3, 2),
        12 => (4, 3),
        24 => (6, 4),
        48 => (8, 6),
        96 => (12, 8),
        384 => (24, 16),
        1536 => (48, 32),
        _ => return None,
    })
}

/// Una cella che pandas legge come «manca» (vuota, «NaN», «N/A»…).
fn e_na(c: &str) -> bool {
    matches!(c, "" | "#N/A" | "#N/A N/A" | "#NA" | "-1.#IND" | "-1.#QNAN" | "-NaN" | "-nan" | "1.#IND" | "1.#QNAN" | "<NA>" | "N/A" | "NA" | "NULL" | "NaN" | "None" | "n/a" | "nan" | "null")
}

fn valore_num(c: &str) -> Option<f64> {
    numero(c.trim())
}

/// Il testo di un valore che non è un numero, per dire che cosa c'era al suo posto (vuoto se la cella era vuota).
fn testo_mancante(c: &str) -> String {
    let t = c.trim();
    if e_na(t) {
        String::new()
    } else {
        t.to_string()
    }
}

fn intero(v: Option<&str>, nome: &str) -> Result<i64, String> {
    let t = v.ok_or_else(|| format!("manca «{nome}» nell'intestazione della piastra"))?.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Ok(n);
    }
    match valore_num(t) {
        Some(f) if f == f.trunc() => Ok(f as i64),
        _ => Err(format!("«{nome}» non è un numero intero: «{t}»")),
    }
}

fn decimale(v: Option<&str>, nome: &str) -> Result<f64, String> {
    let t = v.ok_or_else(|| format!("manca «{nome}» nell'intestazione della piastra"))?;
    valore_num(t).ok_or_else(|| format!("«{nome}» non è un numero: «{t}»"))
}

/// «00:01:30» o «01:30» in secondi.
fn tempo_in_secondi(t: &str) -> Result<f64, String> {
    let t = t.trim();
    let p: Vec<&str> = t.split(':').collect();
    let n = |x: &str| x.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
    match p.as_slice() {
        [h, m, s] => match (n(h), n(m), n(s)) {
            (Some(h), Some(m), Some(s)) => Ok(h * 3600.0 + m * 60.0 + s),
            _ => Err(format!("il tempo «{t}» non è nel formato ore:minuti:secondi")),
        },
        [m, s] => match (n(m), n(s)) {
            (Some(m), Some(s)) => Ok(m * 60.0 + s),
            _ => Err(format!("il tempo «{t}» non è nel formato minuti:secondi")),
        },
        _ => Err(format!("il tempo «{t}» non è nel formato ore:minuti:secondi")),
    }
}

// ---------------------------------------------------------------------------------------------
// Il lettore di righe
// ---------------------------------------------------------------------------------------------

struct Lettore<'a> {
    r: Vec<&'a str>,
    i: usize,
}

impl<'a> Lettore<'a> {
    fn nuovo(r: &[&'a str]) -> Lettore<'a> {
        Lettore { r: r.to_vec(), i: 0 }
    }

    fn esiste(&self) -> bool {
        self.i < self.r.len()
    }

    fn pop(&mut self) -> Option<&'a str> {
        let l = self.r.get(self.i).copied();
        if l.is_some() {
            self.i += 1;
        }
        l
    }

    fn vuota(&self) -> bool {
        self.r.get(self.i).is_some_and(|l| l.trim().is_empty())
    }

    fn salta_vuote(&mut self) {
        while self.esiste() && self.vuota() {
            self.i += 1;
        }
    }

    /// Le righe fino alla prossima vuota (saltando le vuote prima e dopo).
    fn blocco(&mut self) -> Vec<&'a str> {
        self.salta_vuote();
        let mut v = Vec::new();
        while self.esiste() && !self.vuota() {
            if let Some(l) = self.pop() {
                v.push(l);
            }
        }
        self.salta_vuote();
        v
    }
}

// ---------------------------------------------------------------------------------------------
// L'intestazione della piastra
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Formato {
    Tempo,
    Piastra,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tipo {
    Punto,
    Cinetica,
    Spettro,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dati {
    Grezzi,
    Ridotti,
    Entrambi,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

    /// L'unità come la scrive lo strumento (le assorbanze sono in unità di assorbanza).
    fn unita(self) -> &'static str {
        match self {
            Modo::Assorbanza => "OD",
            Modo::Fluorescenza => "RFU",
            Modo::Luminescenza => "RLU",
        }
    }

    fn cubo_cinetica(self) -> (&'static str, &'static str) {
        match self {
            Modo::Assorbanza => ("absorption profile data cube", "ULTRAVIOLET_ABSORBANCE_CUBE_DETECTOR"),
            Modo::Fluorescenza => ("fluorescence emission profile data cube", "FLUORESCENCE_CUBE_DETECTOR"),
            Modo::Luminescenza => ("luminescence profile data cube", "LUMINESCENCE_CUBE_DETECTOR"),
        }
    }
}

struct Testa {
    nome: String,
    formato: Formato,
    tipo: Tipo,
    dati: Dati,
    punti: usize,
    /// Le lunghezze d'onda (per gli spettri, quelle della scansione).
    onde: Vec<f64>,
    colonne: usize,
    pozzetti: usize,
    righe: usize,
    modo: Modo,
    /// «Absorbance», «Kinetic Fluorescence»…
    lettura: String,
    tempo_lettura: Option<f64>,
    intervallo: Option<f64>,
    /// Lettura dal basso (fluorescenza).
    dal_basso: Option<bool>,
    medie: Option<f64>,
    guadagno: Option<String>,
    eccitazione: Option<Vec<i64>>,
    taglio: Option<Vec<i64>>,
}

fn lista_interi(testo: &str, nome: &str) -> Result<Vec<i64>, String> {
    testo.split_whitespace().map(|x| intero(Some(x), nome)).collect()
}

fn testa(riga: &str) -> Result<Testa, String> {
    let h: Vec<Option<String>> = riga.split('\t').map(|c| Some(c.trim().to_string()).filter(|c| !c.is_empty())).collect();
    let c = |i: usize| h.get(i).and_then(|x| x.as_deref());
    let modo = match c(5) {
        Some("Absorbance") => Modo::Assorbanza,
        Some("Fluorescence") => Modo::Fluorescenza,
        Some("Luminescence") => Modo::Luminescenza,
        Some(x) => return Err(format!("il modo di lettura «{x}» non è supportato: Cedro legge assorbanza, fluorescenza e luminescenza")),
        None => return Err("nell'intestazione della piastra manca il modo di lettura".into()),
    };
    let troppo_corta = |n: usize| if h.len() < n { Err(format!("l'intestazione della piastra è troppo corta ({} campi, ne servono {n})", h.len())) } else { Ok(()) };
    // Dove stanno i campi nei tre tipi di intestazione.
    let (i_dati, i_punti, i_tempo, i_interv, i_inizio, i_n_onde, i_onde, i_pozzetti, i_righe) = match modo {
        Modo::Assorbanza => {
            troppo_corta(21)?;
            (6, 8, 9, 10, 11, 14, 15, 18, 20)
        }
        Modo::Fluorescenza => {
            troppo_corta(31)?;
            (7, 9, 10, 11, 12, 15, 16, 19, 30)
        }
        Modo::Luminescenza => {
            troppo_corta(19)?;
            (6, 8, 9, 10, 11, 14, 15, 18, if h.len() >= 30 { 29 } else { 20 })
        }
    };
    if modo == Modo::Luminescenza && h.len() < 30 {
        troppo_corta(21)?;
    }
    let versione = c(2).unwrap_or("");
    if versione != VERSIONE_ESPORTAZIONE {
        return Err(format!("la versione dell'esportazione «{versione}» non è supportata: serve la {VERSIONE_ESPORTAZIONE}"));
    }
    let tipo = match c(4) {
        Some("Endpoint") => Tipo::Punto,
        Some("Kinetic") => Tipo::Cinetica,
        Some("Spectrum") => Tipo::Spettro,
        x => return Err(format!("Cedro legge le piastre in punto finale, in cinetica e in spettro: «{}» no", x.unwrap_or(""))),
    };
    let dati = match c(i_dati) {
        Some("Raw") => Dati::Grezzi,
        Some("Reduced") => Dati::Ridotti,
        Some("Both") => Dati::Entrambi,
        x => return Err(format!("il tipo di dati «{}» non è supportato: servono Raw, Reduced o Both", x.unwrap_or(""))),
    };
    if tipo == Tipo::Spettro && modo != Modo::Assorbanza {
        return Err(format!("gli spettri di {} non sono supportati: Cedro legge solo quelli di assorbanza", if modo == Modo::Fluorescenza { "fluorescenza" } else { "luminescenza" }));
    }
    let formato = match c(3) {
        Some("TimeFormat") => Formato::Tempo,
        Some("PlateFormat") => Formato::Piastra,
        x => return Err(format!("il formato di esportazione «{}» non è supportato: servono TimeFormat o PlateFormat", x.unwrap_or(""))),
    };
    let punti = intero(c(i_punti), "kinetic_points")?;
    let mut n_onde = c(i_n_onde).and_then(|t| t.parse::<i64>().ok()).filter(|n| *n != 0).unwrap_or(1);
    if modo == Modo::Assorbanza && tipo == Tipo::Spettro {
        n_onde = punti;
    }
    let onde: Vec<f64> = if modo == Modo::Assorbanza && tipo == Tipo::Spettro {
        let (a, b, p) = (decimale(c(i_inizio), "start wavelength")?, decimale(c(i_inizio + 1), "end wavelength")?, decimale(c(i_inizio + 2), "wavelength step")?);
        if p == 0.0 {
            return Err("il passo della scansione in lunghezza d'onda è zero".into());
        }
        (0..((b - a) / p) as i64 + 1).map(|i| a + p * i as f64).collect()
    } else {
        match c(i_onde) {
            Some(t) => t.split_whitespace().map(|x| decimale(Some(x), "wavelength")).collect::<Result<_, _>>()?,
            None => Vec::new(),
        }
    };
    if onde.len() as i64 != n_onde {
        return Err(format!("le lunghezze d'onda non sono quante dovrebbero: attese {n_onde}, trovate {}", onde.len()));
    }
    let pozzetti = intero(c(i_pozzetti), "num_wells")? as usize;
    let (colonne, _) = dimensioni(pozzetti).ok_or_else(|| format!("numero di pozzetti sconosciuto: {pozzetti} (sono ammessi 6, 12, 24, 48, 96, 384, 1536)"))?;
    let righe = intero(c(i_righe), "num_rows")?.max(0) as usize;
    let lettura = format!("{}{}", if tipo == Tipo::Cinetica { "Kinetic " } else { "" }, c(5).unwrap_or(""));
    let mut t = Testa {
        nome: c(1).ok_or("la piastra non ha il nome")?.to_string(),
        formato,
        tipo,
        dati,
        punti: punti.max(0) as usize,
        onde,
        colonne,
        pozzetti,
        righe,
        modo,
        lettura,
        tempo_lettura: c(i_tempo).and_then(valore_num),
        intervallo: c(i_interv).and_then(valore_num),
        dal_basso: None,
        medie: None,
        guadagno: None,
        eccitazione: None,
        taglio: None,
    };
    match modo {
        Modo::Fluorescenza => {
            t.dal_basso = Some(match c(6) {
                Some("TRUE") => true,
                Some("FALSE") => false,
                x => return Err(format!("«{}» non è una posizione di lettura valida: servono TRUE o FALSE", x.unwrap_or(""))),
            });
            let ecc = c(20).ok_or("mancano le lunghezze d'onda di eccitazione")?;
            let ecc = lista_interi(ecc, "excitation wavelengths")?;
            if ecc.len() as i64 != n_onde {
                return Err(format!("le lunghezze d'onda di eccitazione non sono quante dovrebbero: attese {n_onde}, trovate {}", ecc.len()));
            }
            t.eccitazione = Some(ecc);
            if let Some(f) = c(22) {
                let f = lista_interi(f, "cutoff filters")?;
                if f.len() as i64 != n_onde {
                    return Err(format!("i filtri di taglio non sono quanti dovrebbero: attesi {n_onde}, trovati {}", f.len()));
                }
                t.taglio = Some(f);
            }
            t.medie = Some(decimale(c(25), "reads_per_well")?);
            t.guadagno = c(26).map(str::to_string);
        }
        Modo::Luminescenza => {
            if h.len() >= 30 {
                t.medie = Some(intero(c(24), "reads_per_well")? as f64);
                t.guadagno = c(25).map(str::to_string);
            }
        }
        Modo::Assorbanza => {}
    }
    Ok(t)
}

// ---------------------------------------------------------------------------------------------
// I dati della piastra
// ---------------------------------------------------------------------------------------------

/// Un pozzetto a una lunghezza d'onda: il valore, o la cinetica, e quello che il gruppo ne dice.
struct Elem {
    onda: f64,
    temperatura: Option<f64>,
    valore: Option<f64>,
    /// Quello che c'era al posto del numero (vuoto se la cella era vuota).
    grezzo: String,
    tempi: Vec<f64>,
    serie: Vec<Option<f64>>,
    campione: Option<String>,
    gruppo: Option<String>,
    errori: Vec<(String, String)>,
    extra: Vec<(String, String)>,
    /// Il file non riporta i dati grezzi: il valore non c'è.
    non_riportato: bool,
}

impl Elem {
    fn nuovo(onda: f64, temperatura: Option<f64>, cella: &str) -> Elem {
        let v = valore_num(cella);
        Elem { onda, temperatura, valore: v, grezzo: if v.is_none() { testo_mancante(cella) } else { String::new() }, tempi: Vec::new(), serie: Vec::new(), campione: None, gruppo: None, errori: Vec::new(), extra: Vec::new(), non_riportato: false }
    }
}

/// I pozzetti letti a una lunghezza d'onda.
struct Serie {
    onda: f64,
    elem: HashMap<String, Elem>,
}

struct Piastra {
    testa: Testa,
    serie: Vec<Serie>,
    ridotti: Vec<(String, f64)>,
}

impl Piastra {
    fn esiste(&self, pos: &str) -> bool {
        self.serie.iter().any(|s| s.elem.contains_key(pos))
    }

    fn elementi_mut(&mut self, pos: &str) -> Vec<&mut Elem> {
        self.serie.iter_mut().filter_map(|s| s.elem.get_mut(pos)).collect()
    }

    /// I pozzetti che ci sono, per righe.
    fn pozzetti(&self) -> Vec<String> {
        let (c, r) = dimensioni(self.testa.pozzetti).unwrap_or((12, 8));
        let mut v = Vec::new();
        for riga in 0..r {
            for col in 1..=c {
                let p = format!("{}{col}", lettere(riga));
                if self.esiste(&p) {
                    v.push(p);
                }
            }
        }
        v
    }
}

/// Una tabella di numeri: i nomi delle colonne e le righe, della stessa larghezza.
struct Mat {
    nomi: Vec<String>,
    righe: Vec<Vec<String>>,
}

/// Le righe di una piastra (tante quante la piastra ne ha), con i nomi delle colonne dati.
fn matrice(r: &mut Lettore, t: &Testa, nomi: &[&str]) -> Result<Mat, String> {
    let (_, n_righe) = dimensioni(t.pozzetti).ok_or("non riesco a capire le dimensioni della piastra")?;
    let mut linee = Vec::new();
    for i in 0..n_righe {
        match r.pop() {
            Some(l) if !l.is_empty() => linee.push(l),
            _ => return Err(format!("nella tabella delle misure mancano righe: attese {n_righe}, trovate {i}")),
        }
    }
    r.salta_vuote();
    let mut righe: Vec<Vec<String>> = linee.iter().map(|l| l.split('\t').map(str::to_string).collect()).collect();
    let larghezza = righe.iter().map(Vec::len).max().unwrap_or(0);
    for x in righe.iter_mut() {
        x.resize(larghezza, String::new());
    }
    // I nomi delle colonne e i dati hanno la stessa larghezza: si taglia quello che avanza.
    let mut nomi: Vec<String> = nomi.iter().map(|n| n.trim().to_string()).collect();
    if nomi.len() >= larghezza {
        nomi.truncate(larghezza);
    } else {
        for x in righe.iter_mut() {
            x.truncate(nomi.len());
        }
    }
    Ok(Mat { nomi, righe })
}

/// La prima riga con qualcosa dentro: la temperatura sta nella sua seconda cella e il tempo nella prima.
fn prima_valida(m: &Mat, con_tempo: bool) -> Result<(Option<f64>, Option<f64>), String> {
    let riga = m.righe.iter().find(|r| r.iter().any(|c| !e_na(c))).ok_or("la tabella delle misure è vuota")?;
    let temperatura = riga.get(1).and_then(|c| valore_num(c));
    let tempo = match riga.first() {
        Some(c) if con_tempo && !e_na(c) => Some(tempo_in_secondi(c)?),
        _ => None,
    };
    Ok((temperatura, tempo))
}

/// I pozzetti di una lunghezza d'onda in una matrice: dalle colonne `ini` a `fine` dei dati.
fn serie_da_matrice(t: &Testa, temperatura: Option<f64>, tempo: Option<f64>, onda: f64, m: &Mat, ini: usize, fine: usize) -> Serie {
    let mut elem = HashMap::new();
    for (ri, riga) in m.righe.iter().enumerate() {
        for ci in ini..fine {
            let col = 2 + ci;
            if col >= riga.len() {
                break;
            }
            let pos = format!("{}{}", lettere(ri), m.nomi[col]);
            let cella = &riga[col];
            if valore_num(cella).is_none() && tempo.is_some() {
                continue;
            }
            let mut e = Elem::nuovo(onda, temperatura, cella);
            if let Some(tp) = tempo {
                e.tempi.push(tp);
                e.serie.push(e.valore);
            }
            elem.insert(pos, e);
        }
    }
    let _ = t;
    Serie { onda, elem }
}

fn aggiungi_punti_cinetici(s: &mut Serie, tempo: f64, m: &Mat, ini: usize, fine: usize) -> Result<(), String> {
    for (ri, riga) in m.righe.iter().enumerate() {
        for ci in ini..fine {
            let col = 2 + ci;
            if col >= riga.len() {
                break;
            }
            let pos = format!("{}{}", lettere(ri), m.nomi[col]);
            let Some(e) = s.elem.get_mut(&pos) else { continue };
            let Some(v) = valore_num(&riga[col]) else {
                return Err(format!("nella cinetica manca la misura del pozzetto {pos} a {} s", py_float(tempo)));
            };
            e.tempi.push(tempo);
            e.serie.push(Some(v));
        }
    }
    Ok(())
}

/// I dati di una piastra scritta «a piastra» (PlateFormat): una matrice per lunghezza d'onda o per tempo.
fn dati_a_piastra(r: &mut Lettore, t: &Testa) -> Result<(Vec<Serie>, Vec<(String, f64)>), String> {
    let nomi_riga = r.pop().ok_or("mancano le colonne dei dati della piastra")?;
    let nomi: Vec<&str> = nomi_riga.split('\t').collect();
    let mut serie: Vec<Serie> = Vec::new();
    if t.tipo == Tipo::Spettro {
        for &onda in &t.onde {
            let m = matrice(r, t, &nomi)?;
            let (temperatura, _) = prima_valida(&m, false)?;
            let fine = m.nomi.len().saturating_sub(2);
            serie.push(serie_da_matrice(t, temperatura, None, onda, &m, 0, fine));
        }
        r.salta_vuote();
    } else {
        let m = matrice(r, t, &nomi)?;
        let (temperatura, tempo) = prima_valida(&m, true)?;
        for (i, &onda) in t.onde.iter().enumerate() {
            let ini = i * (t.colonne + 1);
            serie.push(serie_da_matrice(t, temperatura, tempo, onda, &m, ini, ini + t.colonne));
        }
        if tempo.is_some() && t.punti > 1 {
            for _ in 1..t.punti {
                let m = matrice(r, t, &nomi)?;
                let tp = match m.righe.first().and_then(|x| x.first()) {
                    Some(c) if !e_na(c) => tempo_in_secondi(c)?,
                    _ => return Err("nella cinetica manca il tempo di una lettura".into()),
                };
                for (i, s) in serie.iter_mut().enumerate() {
                    let ini = i * (t.colonne + 1);
                    aggiungi_punti_cinetici(s, tp, &m, ini, ini + t.colonne)?;
                }
            }
        }
    }
    // I dati ridotti (la tabella dei valori che il programma ha ridotto), se ci sono.
    let mut ridotti = Vec::new();
    if r.esiste() {
        let nomi_riga = r.pop().ok_or("mancano le colonne dei dati ridotti")?;
        let nomi: Vec<&str> = nomi_riga.split('\t').collect();
        let m = matrice(r, t, &nomi)?;
        for (ri, riga) in m.righe.iter().enumerate() {
            for ci in 0..t.colonne {
                let col = 2 + ci;
                if col >= riga.len() {
                    break;
                }
                if let Some(v) = valore_num(&riga[col]) {
                    ridotti.push((format!("{}{}", lettere(ri), m.nomi[col]), v));
                }
            }
        }
    }
    Ok((serie, ridotti))
}

/// Le righe «a colonne» di un blocco di dati: la matrice in pezzi, con le colonne dei nomi.
fn righe_a_colonne(linee: &[&str], nomi: &[&str]) -> Result<Vec<Vec<String>>, String> {
    let righe: Vec<Vec<String>> = linee.iter().map(|l| l.split('\t').map(str::to_string).collect()).collect();
    let larghezza = righe.iter().map(Vec::len).max().unwrap_or(0);
    if larghezza != nomi.len() {
        return Err(format!("formato non valido: i dati hanno {larghezza} colonne e le intestazioni {}", nomi.len()));
    }
    Ok(righe.into_iter().map(|mut r| {
        r.resize(larghezza, String::new());
        r
    }).collect())
}

/// I dati di una piastra scritta «a colonne» (TimeFormat): una riga per lunghezza d'onda o per tempo.
fn dati_a_colonne(r: &mut Lettore, t: &Testa) -> Result<(Vec<Serie>, Vec<(String, f64)>), String> {
    let mut serie: Vec<Serie> = Vec::new();
    if matches!(t.dati, Dati::Grezzi | Dati::Entrambi) {
        let nomi_riga = r.pop().ok_or("mancano le colonne dei dati della piastra")?;
        let nomi: Vec<&str> = nomi_riga.split('\t').map(str::trim).collect();
        for &onda in &t.onde {
            let linee: Vec<&str> = if t.tipo == Tipo::Spettro {
                r.salta_vuote();
                let l = r.pop();
                r.salta_vuote();
                l.into_iter().collect()
            } else {
                r.blocco()
            };
            if linee.is_empty() {
                return Err("nel file mancano i dati delle misure di una lunghezza d'onda".into());
            }
            let righe = righe_a_colonne(&linee, &nomi)?;
            if t.tipo != Tipo::Cinetica && righe.len() != 1 {
                return Err("nel file ci sono più righe di misure per la stessa lunghezza d'onda".into());
            }
            let temperatura = righe[0].get(1).and_then(|c| valore_num(c));
            let mut elem = HashMap::new();
            for (i, nome) in nomi.iter().enumerate().skip(2) {
                let mut e = Elem::nuovo(onda, temperatura, &righe[0][i]);
                if t.tipo == Tipo::Cinetica {
                    for riga in &righe {
                        e.tempi.push(tempo_in_secondi(&riga[0])?);
                        e.serie.push(valore_num(&riga[i]));
                    }
                }
                elem.insert(nome.to_string(), e);
            }
            serie.push(Serie { onda, elem });
        }
    } else {
        // Solo dati ridotti: i valori grezzi non ci sono, ma i pozzetti sì.
        for &onda in &t.onde {
            let mut elem = HashMap::new();
            for riga in 0..t.righe {
                for col in 1..=t.colonne {
                    let mut e = Elem::nuovo(onda, None, "");
                    e.non_riportato = true;
                    e.grezzo = "Not reported".into();
                    elem.insert(format!("{}{col}", lettere(riga)), e);
                }
            }
            serie.push(Serie { onda, elem });
        }
    }
    let mut ridotti = Vec::new();
    if r.esiste() {
        let nomi_riga = r.pop().ok_or("mancano le colonne dei dati ridotti")?;
        let nomi: Vec<&str> = nomi_riga.split('\t').map(str::trim).collect();
        let dati = r.pop().ok_or("mancano i dati ridotti")?;
        let celle: Vec<&str> = dati.split('\t').collect();
        if celle.len() != nomi.len() {
            return Err(format!("i dati ridotti hanno {} colonne e le intestazioni {}", celle.len(), nomi.len()));
        }
        for i in 2..(2 + t.pozzetti).min(nomi.len()) {
            if let Some(v) = valore_num(celle[i]) {
                ridotti.push((nomi[i].to_string(), v));
            }
        }
    }
    Ok((serie, ridotti))
}

fn piastra(b: &[&str]) -> Result<Piastra, String> {
    let mut r = Lettore::nuovo(b);
    r.salta_vuote();
    let riga = r.pop().ok_or("manca l'intestazione della piastra")?;
    let t = testa(riga)?;
    let (serie, ridotti) = match t.formato {
        Formato::Piastra => dati_a_piastra(&mut r, &t)?,
        Formato::Tempo => dati_a_colonne(&mut r, &t)?,
    };
    Ok(Piastra { testa: t, serie, ridotti })
}

// ---------------------------------------------------------------------------------------------
// I gruppi: i campioni e i calcoli del programma
// ---------------------------------------------------------------------------------------------

/// Una tabella di testo: i nomi delle colonne e le celle (vuote o spazi sono `None`).
struct Tab {
    nomi: Vec<String>,
    righe: Vec<Vec<Option<String>>>,
    /// Le celle scritte proprio vuote (due tabulazioni di fila): diverse da quelle con solo uno spazio,
    /// che il programma scrive per dire «come sopra» dentro lo stesso campione.
    vuote: Vec<Vec<bool>>,
}

fn tabella(linee: &[&str]) -> Option<Tab> {
    let (prima, resto) = linee.split_first()?;
    let mut nomi: Vec<String> = Vec::new();
    for n in prima.split('\t') {
        let n = n.trim().to_string();
        // Due colonne con lo stesso nome: la seconda è «nome.1», come fa pandas.
        let mut candidato = n.clone();
        let mut k = 0;
        while nomi.contains(&candidato) {
            k += 1;
            candidato = format!("{n}.{k}");
        }
        nomi.push(candidato);
    }
    let righe: Vec<Vec<Option<String>>> = resto
        .iter()
        .map(|l| {
            let mut v: Vec<Option<String>> = l.split('\t').map(|c| Some(c.trim().to_string()).filter(|c| !c.is_empty())).collect();
            v.resize(nomi.len(), None);
            v
        })
        .collect();
    let vuote: Vec<Vec<bool>> = resto
        .iter()
        .map(|l| {
            let mut v: Vec<bool> = l.split('\t').map(str::is_empty).collect();
            v.resize(nomi.len(), false);
            v
        })
        .collect();
    // Le colonne tutte vuote si buttano.
    let tieni: Vec<usize> = (0..nomi.len()).filter(|&i| righe.iter().any(|r| r[i].is_some())).collect();
    Some(Tab {
        nomi: tieni.iter().map(|&i| nomi[i].clone()).collect(),
        righe: righe.iter().map(|r| tieni.iter().map(|&i| r[i].clone()).collect()).collect(),
        vuote: vuote.iter().map(|r| tieni.iter().map(|&i| r[i]).collect()).collect(),
    })
}

/// Un valore di un gruppo: un numero o un testo.
#[derive(Clone)]
enum Cella {
    Numero(f64),
    Testo(String),
}

/// Un valore calcolato per un campione, valido per questi pozzetti.
struct ElemGruppo {
    piastra: Option<String>,
    posizioni: Vec<String>,
    /// Il nome del calcolo e il suo valore (−0 se il programma ha scritto un testo al suo posto).
    voci: Vec<(String, f64)>,
    errori: Vec<(String, String)>,
    extra: Vec<(String, String)>,
    campione: String,
}

struct DatiGruppo {
    nome: String,
    elementi: Vec<ElemGruppo>,
    /// Come sono calcolate le colonne (nome e formula).
    formule: HashMap<String, String>,
    /// I riassunti: nome, valore e come sono calcolati.
    riassunti: Vec<(String, f64, String)>,
}

fn gruppo(b: &[&str]) -> Result<DatiGruppo, String> {
    let mut r = Lettore::nuovo(b);
    let nome_riga = r.pop().ok_or("manca il nome del gruppo")?;
    let nome = nome_riga.strip_prefix("Group: ").unwrap_or(nome_riga).trim().to_string();
    let dati = r.blocco();
    let formule_righe = r.blocco();
    let mut riassunti = Vec::new();
    // Dopo le formule il blocco dei riassunti: «Nome<TAB>descrizione<TAB>valore<TAB>formula».
    while r.esiste() && !r.vuota() {
        let Some(l) = r.pop() else { break };
        let p: Vec<&str> = l.split('\t').collect();
        if p.len() >= 4 && !p[0].is_empty() && !p[3].is_empty() && !p[2].is_empty() && p[2].bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            if let Ok(v) = p[2].parse::<f64>() {
                riassunti.push((p[0].to_string(), v, p[3].to_string()));
            }
        }
    }
    let mut formule = HashMap::new();
    if let Some(t) = tabella(&formule_righe) {
        if let (Some(a), Some(b)) = (t.nomi.iter().position(|n| n == "Formula Name"), t.nomi.iter().position(|n| n == "Formula")) {
            for riga in &t.righe {
                if let (Some(n), Some(f)) = (&riga[a], &riga[b]) {
                    formule.insert(n.clone(), f.clone());
                }
            }
        }
    }
    let elementi = match tabella(&dati) {
        Some(t) if !t.righe.is_empty() && t.nomi.iter().any(|n| n == "Sample") => elementi_gruppo(&t, &nome)?,
        _ => Vec::new(),
    };
    Ok(DatiGruppo { nome, elementi, formule, riassunti })
}

fn elementi_gruppo(t: &Tab, nome_gruppo: &str) -> Result<Vec<ElemGruppo>, String> {
    let idx = |n: &str| t.nomi.iter().position(|x| x == n);
    let i_campione = idx("Sample").ok_or("manca la colonna Sample")?;
    let i_pozzetto = idx("Well").or_else(|| idx("Wells"));
    let i_piastra = idx("WellPlateName");
    // Una colonna è «calcolata» se ha almeno un numero (o «Masked», «Range?», «Error»).
    let calcolata: Vec<bool> = (0..t.nomi.len()).map(|c| t.righe.iter().any(|r| r[c].as_deref().is_some_and(|v| valore_num(v).is_some() || VALORI_NON_NUMERO.contains(&v)))).collect();
    let colonne: Vec<usize> = (0..t.nomi.len()).filter(|&c| !matches!(t.nomi[c].as_str(), "Sample" | "Well" | "Wells" | "WellPlateName")).collect();
    // I campioni: la cella Sample vale fino alla prossima.
    let mut campione_corrente: Option<String> = None;
    let mut per_campione: Vec<(String, Vec<usize>)> = Vec::new();
    for (ri, riga) in t.righe.iter().enumerate() {
        if let Some(s) = &riga[i_campione] {
            campione_corrente = Some(s.clone());
        }
        let Some(s) = &campione_corrente else { continue };
        match per_campione.iter_mut().find(|(n, _)| n == s) {
            Some((_, v)) => v.push(ri),
            None => per_campione.push((s.clone(), vec![ri])),
        }
    }
    let mut out = Vec::new();
    for (campione, righe) in per_campione {
        // Per ogni colonna i valori del campione, in ordine: una cella vuota è lo stesso valore del pozzetto di prima.
        let mut per_colonna: Vec<Vec<ElemGruppo>> = colonne.iter().map(|_| Vec::new()).collect();
        for &ri in &righe {
            let riga = &t.righe[ri];
            let pozzetto = i_pozzetto.and_then(|i| riga[i].clone()).ok_or_else(|| format!("nel gruppo «{nome_gruppo}» una riga non ha il pozzetto"))?;
            let piastra = i_piastra.and_then(|i| riga[i].clone());
            for (k, &c) in colonne.iter().enumerate() {
                let risultato = riga[c].as_deref().map(|v| valore_num(v).map(Cella::Numero).unwrap_or_else(|| Cella::Testo(v.to_string())));
                match risultato {
                    Some(res) => {
                        let nome = t.nomi[c].clone();
                        let mut e = ElemGruppo { piastra: piastra.clone(), posizioni: vec![pozzetto.clone()], voci: Vec::new(), errori: Vec::new(), extra: Vec::new(), campione: campione.clone() };
                        if calcolata[c] {
                            match &res {
                                Cella::Numero(x) => e.voci.push((nome, *x)),
                                Cella::Testo(s) => {
                                    e.errori.push((s.clone(), nome.clone()));
                                    e.voci.push((nome, -0.0));
                                }
                            }
                        } else {
                            let s = match &res {
                                Cella::Numero(_) => riga[c].clone().unwrap_or_default(),
                                Cella::Testo(s) => s.clone(),
                            };
                            e.extra.push((nome, s));
                        }
                        per_colonna[k].push(e);
                    }
                    None => {
                        // Una cella con solo uno spazio vale «come sopra»: il valore del campione si estende a questo pozzetto
                        // (media, SD, CV, ma anche note come «FAIL» scritte solo sulla prima riga).
                        // Una cella proprio vuota in una colonna di note non dice niente: non si copia l'etichetta del pozzetto di prima.
                        if calcolata[c] || !t.vuote[ri][c] {
                            if let Some(ultimo) = per_colonna[k].last_mut() {
                                ultimo.posizioni.push(pozzetto.clone());
                            }
                        }
                    }
                }
            }
        }
        out.extend(per_colonna.into_iter().flatten());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Il file intero
// ---------------------------------------------------------------------------------------------

struct Blocchi<'a> {
    blocchi: Vec<Vec<&'a str>>,
    ultima: Option<&'a str>,
}

fn dividi(testo: &str) -> Result<Blocchi<'_>, String> {
    let mut righe = testo.split('\n');
    let prima = righe.next().unwrap_or("").trim_start_matches('\u{feff}').trim();
    let n: usize = prima
        .strip_prefix("##BLOCKS=")
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| format!("la prima riga dovrebbe dire quanti blocchi ci sono («##BLOCKS= N»): «{prima}»"))?;
    let resto: Vec<&str> = righe.collect();
    let mut i = 0;
    while i < resto.len() && resto[i].trim().is_empty() {
        i += 1;
    }
    let mut blocchi = Vec::new();
    for _ in 0..n {
        let mut b = Vec::new();
        while i < resto.len() && !resto[i].contains("~End") {
            b.push(resto[i]);
            i += 1;
        }
        if i < resto.len() {
            i += 1;
        }
        while i < resto.len() && resto[i].trim().is_empty() {
            i += 1;
        }
        blocchi.push(b);
    }
    Ok(Blocchi { blocchi, ultima: resto.get(i).copied() })
}

/// Un secondo come lo scrive Gen5 («22.0»)… qui serve per «525.0nm».
fn nome_onda(v: f64) -> String {
    py_float(v)
}

/// Il titolo del gruppo di misure di una piastra a una lunghezza d'onda.
fn titolo(t: &Testa, onda: f64) -> String {
    if t.onde.len() > 1 {
        format!("{} · {} nm", t.nome, if onda.fract() == 0.0 { format!("{}", onda as i64) } else { format!("{onda}") })
    } else {
        t.nome.clone()
    }
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let file = dividi(&f.testo)?;
    let mut piastre: Vec<Piastra> = Vec::new();
    let mut gruppi: Vec<DatiGruppo> = Vec::new();
    for b in &file.blocchi {
        let prima = b.iter().find(|r| !r.trim().is_empty()).copied().unwrap_or("");
        if prima.starts_with("Group") {
            gruppi.push(gruppo(b)?);
        } else if prima.starts_with("Plate") {
            let p = piastra(b)?;
            if piastre.iter().any(|x| x.testa.nome == p.testa.nome) {
                return Err(format!("le piastre del file devono avere nomi diversi: «{}» è ripetuto", p.testa.nome));
            }
            piastre.push(p);
        } else if !prima.starts_with("Note") {
            return Err(format!("un blocco del file comincia con «{prima}»: dovrebbe cominciare con Group, Plate o Note"));
        }
    }
    // Le date: «Original Filename: …; Date Last Saved: 11/27/2023 10:43:25 AM».
    let salvato = file.ultima.and_then(|u| u.split_once("Date Last Saved: ")).map(|(_, d)| d.trim().to_string()).filter(|d| !d.is_empty());
    let originale = file.ultima.and_then(|u| u.split_once("Original Filename: ")).map(|(_, d)| d.split(';').next().unwrap_or("").trim().to_string()).filter(|d| !d.is_empty());

    // I gruppi dicono di che campione è ogni pozzetto.
    let unica = piastre.len() == 1;
    let nome_unico = piastre.first().map(|p| p.testa.nome.clone());
    for g in gruppi.iter_mut() {
        for e in g.elementi.iter_mut() {
            if unica {
                e.piastra = nome_unico.clone();
            }
            let Some(nome_piastra) = e.piastra.clone() else { continue };
            let pi = piastre.iter().position(|p| p.testa.nome == nome_piastra).ok_or_else(|| format!("il gruppo «{}» cita la piastra «{nome_piastra}» che nel file non c'è", g.nome))?;
            let spettro = piastre[pi].testa.tipo == Tipo::Spettro;
            let mut fatti: HashSet<String> = HashSet::new();
            for pos in &e.posizioni {
                for el in piastre[pi].elementi_mut(pos) {
                    el.gruppo = Some(g.nome.clone());
                    el.campione = Some(e.campione.clone());
                    // Negli spettri gli errori vanno solo al primo punto di ogni pozzetto.
                    if !spettro || !fatti.contains(pos) {
                        el.errori.extend(e.errori.iter().cloned());
                        if spettro {
                            fatti.insert(pos.clone());
                        }
                    }
                    for (k, v) in &e.extra {
                        match el.extra.iter_mut().find(|(a, _)| a == k) {
                            Some(x) => x.1 = v.clone(),
                            None => el.extra.push((k.clone(), v.clone())),
                        }
                    }
                }
            }
        }
    }

    l.strumento.produttore = "Molecular Devices".into();
    l.strumento.software = "SoftMax Pro".into();
    l.info.push(("Versione dell'esportazione".into(), VERSIONE_ESPORTAZIONE.into()));
    if let Some(o) = &originale {
        l.info.push(("File originale".into(), o.clone()));
    }
    if let Some(d) = &salvato {
        l.info.push(("Ultimo salvataggio".into(), d.clone()));
    }
    let quando_base = salvato.as_deref().and_then(|d| tempo::iso(d, tempo::giorno_prima([d])));

    // Le misure.
    let mut ids: HashMap<(usize, String), Vec<String>> = HashMap::new();
    for (pi, p) in piastre.iter().enumerate() {
        let t = &p.testa;
        let quando = quando_base.as_ref().map(|q| match t.tempo_lettura.filter(|s| *s != 0.0) {
            Some(s) => tempo::piu_secondi(q, s).unwrap_or_else(|| q.clone()),
            None => q.clone(),
        });
        let posizioni = p.pozzetti();
        if t.tipo == Tipo::Spettro {
            let mut gruppo_misure = Gruppo { titolo: t.nome.clone(), quando: quando.clone(), piastra: Some(t.nome.clone()), pozzetti: Some(t.pozzetti as u32), ..Default::default() };
            for pos in &posizioni {
                let el: Vec<&Elem> = p.serie.iter().filter_map(|s| s.elem.get(pos)).collect();
                if el.iter().all(|e| e.valore.is_none()) {
                    continue;
                }
                let primo = el[0];
                let mut m = base(t, pos, primo);
                m.valori.push(Valore::testo("detection type", &t.lettura, Posto::Impostazione));
                aggiungi_temperatura(&mut m, primo.temperatura);
                m.spettro = el.iter().map(|e| (e.onda, e.valore.unwrap_or(f64::NAN))).collect();
                m.curva = Some(Curva { chiave: "absorption spectrum data cube".into(), etichetta: "absorbance-spectrum".into(), cinetica: false, concetto: "absorbance".into(), unita: t.modo.unita().into() });
                for e in &el {
                    for (er, ft) in &e.errori {
                        m.note.push((er.clone(), ft.clone()));
                    }
                    if e.valore.is_none() {
                        m.note.push((e.grezzo.clone(), format!("{}nm", nome_onda(e.onda))));
                    }
                }
                finisci(&mut m, primo);
                m.id = l.id();
                ids.entry((pi, pos.clone())).or_default().push(m.id.clone());
                gruppo_misure.misure.push(m);
            }
            if !gruppo_misure.misure.is_empty() {
                l.gruppi.push(gruppo_misure);
            }
            continue;
        }
        for (wi, s) in p.serie.iter().enumerate() {
            let mut gr = Gruppo { titolo: titolo(t, s.onda), quando: quando.clone(), piastra: Some(t.nome.clone()), pozzetti: Some(t.pozzetti as u32), ..Default::default() };
            for pos in &posizioni {
                let Some(e) = s.elem.get(pos) else { continue };
                let mut m = base(t, pos, e);
                m.valori.push(Valore::testo("detection type", &t.lettura, Posto::Impostazione));
                if !(t.modo == Modo::Luminescenza && s.onda == 0.0) {
                    m.valori.push(Valore::num("detector wavelength setting", s.onda, "nm", Posto::Impostazione));
                }
                if let Some(x) = t.eccitazione.as_ref().and_then(|v| v.get(wi)) {
                    m.valori.push(Valore::num("excitation wavelength setting", *x as f64, "nm", Posto::Impostazione));
                }
                if let Some(x) = t.taglio.as_ref().and_then(|v| v.get(wi)) {
                    m.valori.push(Valore::num("wavelength filter cutoff setting", *x as f64, "nm", Posto::Impostazione));
                }
                if let Some(basso) = t.dal_basso {
                    m.valori.push(Valore::testo("scan position setting (plate reader)", if basso { "bottom scan position (plate reader)" } else { "top scan position (plate reader)" }, Posto::Impostazione));
                }
                if let Some(x) = t.medie {
                    m.valori.push(Valore::num("number of averages", x, "#", Posto::Impostazione));
                }
                if let Some(g) = &t.guadagno {
                    m.valori.push(Valore::testo("detector gain setting", g, Posto::Impostazione));
                }
                if t.tipo == Tipo::Cinetica {
                    if let Some(x) = t.tempo_lettura {
                        m.valori.push(Valore::num("total measurement time setting", x, "s", Posto::Impostazione));
                    }
                    if let Some(x) = t.intervallo {
                        m.valori.push(Valore::num("read interval setting", x, "s", Posto::Impostazione));
                    }
                    m.valori.push(Valore::num("number of scans setting", t.punti as f64, "#", Posto::Impostazione));
                }
                aggiungi_temperatura(&mut m, e.temperatura);
                if t.tipo == Tipo::Cinetica {
                    let (chiave, etichetta) = t.modo.cubo_cinetica();
                    if e.non_riportato {
                        m.spettro = vec![(0.0, f64::NAN)];
                        m.note.push((e.grezzo.clone(), t.lettura.clone()));
                    } else {
                        m.spettro = e.tempi.iter().copied().zip(e.serie.iter().map(|v| v.unwrap_or(f64::NAN))).collect();
                    }
                    m.curva = Some(Curva { chiave: chiave.into(), etichetta: etichetta.into(), cinetica: true, concetto: t.modo.concetto().into(), unita: t.modo.unita().into() });
                } else {
                    let dato = match e.valore {
                        Some(v) => Dato::Numero(v),
                        None => Dato::Assente(e.grezzo.clone()),
                    };
                    m.valori.push(Valore { chiave: t.modo.concetto().into(), dato, unita: t.modo.unita().into(), posto: Posto::Misura });
                }
                finisci(&mut m, e);
                m.id = l.id();
                ids.entry((pi, pos.clone())).or_default().push(m.id.clone());
                gr.misure.push(m);
            }
            if !gr.misure.is_empty() {
                l.gruppi.push(gr);
            }
        }
    }
    if l.gruppi.is_empty() {
        return Err("i dati non contengono misure valide".into());
    }

    // I calcoli del programma: i valori ridotti e quelli dei gruppi.
    let sorgenti = |pi: usize, pos: &str| -> Vec<String> { ids.get(&(pi, pos.to_string())).cloned().unwrap_or_default() };
    for (pi, p) in piastre.iter().enumerate() {
        for (pos, v) in &p.ridotti {
            let s = sorgenti(pi, pos);
            if s.is_empty() {
                continue;
            }
            l.calcolati.push(Calcolato { nome: "Reduced".into(), valore: *v, unita: String::new(), sorgenti: s, da: p.testa.lettura.to_lowercase(), descrizione: String::new() });
        }
    }
    for g in &gruppi {
        for e in &g.elementi {
            let Some(pn) = &e.piastra else { continue };
            let Some(pi) = piastre.iter().position(|p| &p.testa.nome == pn) else { continue };
            let mut src: Vec<String> = Vec::new();
            for pos in &e.posizioni {
                src.extend(sorgenti(pi, pos));
            }
            for (nome, v) in &e.voci {
                l.calcolati.push(Calcolato { nome: nome.clone(), valore: *v, unita: String::new(), sorgenti: src.clone(), da: piastre[pi].testa.lettura.to_lowercase(), descrizione: g.formule.get(nome).cloned().unwrap_or_default() });
            }
        }
        if !g.riassunti.is_empty() {
            // I riassunti valgono per tutti i pozzetti del gruppo.
            let mut visti: HashSet<(usize, String)> = HashSet::new();
            let mut src: Vec<String> = Vec::new();
            let mut da = String::new();
            for e in &g.elementi {
                let Some(pn) = &e.piastra else { continue };
                let Some(pi) = piastre.iter().position(|p| &p.testa.nome == pn) else { continue };
                if da.is_empty() {
                    da = piastre[pi].testa.lettura.to_lowercase();
                }
                for pos in &e.posizioni {
                    if visti.insert((pi, pos.clone())) {
                        src.extend(sorgenti(pi, pos));
                    }
                }
            }
            for (nome, v, formula) in &g.riassunti {
                l.calcolati.push(Calcolato { nome: nome.clone(), valore: *v, unita: String::new(), sorgenti: src.clone(), da: da.clone(), descrizione: formula.clone() });
            }
        }
    }

    // Il valore da mostrare sulla piastra: quello della prima piastra con un valore per pozzetto, altrimenti la curva.
    match piastre.iter().find(|p| p.testa.tipo == Tipo::Punto).map(|p| p.testa.modo) {
        Some(m) => {
            l.principale = m.concetto().into();
            l.unita = m.unita().into();
        }
        None => {
            l.principale = PRINCIPALE_CURVA.into();
            l.unita = l.curva().map(|c| c.unita.clone()).unwrap_or_default();
        }
    }
    Ok(())
}

/// La misura con il suo posto, il campione e il gruppo.
fn base(t: &Testa, pos: &str, e: &Elem) -> Misura {
    let mut m = Misura { pozzetto: Some(pos.to_string()), piastra: Some(t.nome.clone()), campione: e.campione.clone().unwrap_or_else(|| format!("{} {pos}", t.nome)), ..Default::default() };
    if let Some(g) = &e.gruppo {
        m.valori.push(Valore::testo("group identifier", g, Posto::CampioneAltro));
    }
    m
}

fn aggiungi_temperatura(m: &mut Misura, t: Option<f64>) {
    // Una temperatura di zero vuol dire che lo strumento non l'ha letta.
    if let Some(t) = t.filter(|t| *t != 0.0) {
        m.valori.push(Valore::num("compartment temperature", t, "degC", Posto::Misura));
    }
}

/// Gli errori del gruppo sul pozzetto e le altre colonne del gruppo.
fn finisci(m: &mut Misura, e: &Elem) {
    for er in &e.errori {
        if !m.note.contains(er) {
            m.note.push(er.clone());
        }
    }
    for (k, v) in &e.extra {
        m.valori.push(Valore::testo(k, v, Posto::Altro));
    }
}
