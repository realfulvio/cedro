//! Il motore dei calcoli derivati degli strumenti: da dove viene ogni valore che lo strumento calcola
//! (la media del Ct di un campione, il Δ-Ct, la quantità) e quali misure o altri calcoli l'hanno
//! prodotto. Segue il modello di allotropy (`calcdocs`): i valori stanno su «elementi» (una misura),
//! raggruppati da «viste» annidate (campione → bersaglio → misura), e ogni «configurazione» dice che
//! valore leggere, su quale vista e da quali sorgenti (misure o altre configurazioni) dipende.
//!
//! Qui il lavoro è tutto di lettura: il risultato è l'elenco dei calcoli, nell'ordine in cui lo
//! strumento li presenta, con le fonti di ciascuno.

use std::collections::HashMap;
use std::rc::Rc;

/// Un valore di un elemento: un numero o un testo.
#[derive(Clone, Debug, PartialEq)]
pub enum Cd {
    Num(f64),
    Testo(String),
}

/// Una misura con i suoi valori, per i calcoli.
#[derive(Clone, Debug)]
pub struct Elemento {
    pub id: String,
    pub dati: HashMap<String, Cd>,
}

impl Elemento {
    pub fn nuovo(id: &str) -> Elemento {
        Elemento { id: id.to_string(), dati: HashMap::new() }
    }
    pub fn metti_num(&mut self, k: &str, v: Option<f64>) {
        if let Some(v) = v {
            self.dati.insert(k.to_string(), Cd::Num(v));
        }
    }
    pub fn metti_testo(&mut self, k: &str, v: Option<&str>) {
        if let Some(v) = v {
            self.dati.insert(k.to_string(), Cd::Testo(v.to_string()));
        }
    }
    /// Il valore come testo (i numeri come li scrive Python: «3000.0»).
    pub fn testo(&self, k: &str) -> Option<String> {
        match self.dati.get(k)? {
            Cd::Testo(t) => Some(t.clone()),
            Cd::Num(n) => Some(crate::strumenti::fonte::py_float_str(*n)),
        }
    }
    /// Il valore come numero, se lo è (o se è un testo che lo sembra).
    pub fn numero(&self, k: &str) -> Option<f64> {
        match self.dati.get(k)? {
            Cd::Num(n) => Some(*n),
            Cd::Testo(t) => t.trim().replace(',', ".").parse::<f64>().ok(),
        }
    }
}

type Voce = (String, String);

/// Le chiavi che dicono dove siamo nelle viste: («sample_id», «A»), («target_dna», «X»).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chiavi(Vec<Voce>);

impl Chiavi {
    fn e_vuota(&self) -> bool {
        self.0.is_empty()
    }
    fn aggiungi(&self, nome: &str, valore: &str) -> Chiavi {
        let mut v = self.0.clone();
        v.push((nome.to_string(), valore.to_string()));
        Chiavi(v)
    }
    fn accoda(&self, altre: &Chiavi) -> Chiavi {
        let mut v = self.0.clone();
        v.extend(altre.0.iter().cloned());
        Chiavi(v)
    }
    fn prendi(&self, nome: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == nome).map(|(_, v)| v.as_str())
    }
    /// Cambia il valore di una chiave; se non c'è la mette in testa.
    fn sovrascrivi(&self, nome: &str, valore: &str) -> Chiavi {
        if self.prendi(nome).is_none() {
            let mut v = vec![(nome.to_string(), valore.to_string())];
            v.extend(self.0.iter().cloned());
            return Chiavi(v);
        }
        Chiavi(self.0.iter().map(|(n, v)| if n == nome { (n.clone(), valore.to_string()) } else { (n.clone(), v.clone()) }).collect())
    }
    fn togli(&self, nome: &str) -> Chiavi {
        Chiavi(self.0.iter().filter(|(n, _)| n != nome).cloned().collect())
    }
    fn chiave_stringa(&self) -> String {
        self.0.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("|")
    }
}

/// Come si raggruppano gli elementi di una vista: un nome per ogni gruppo, nell'ordine in cui compaiono.
pub type Ordina = dyn Fn(&[Elemento], &[usize]) -> Vec<(String, Vec<usize>)>;

/// Una vista: raggruppa gli elementi per una chiave (il campione, il bersaglio, la misura).
pub struct Vista {
    pub nome: String,
    pub sub: Option<Rc<Vista>>,
    pub ordina: Box<Ordina>,
    /// Il valore a cui si porta la chiave di questa vista (il campione di riferimento).
    pub riferimento: Option<String>,
    /// Vista del riferimento (il bersaglio di riferimento): senza riferimento è vuota.
    pub e_riferimento: bool,
}

impl Vista {
    pub fn nuova(nome: &str, sub: Option<Rc<Vista>>, ordina: Box<Ordina>) -> Vista {
        Vista { nome: nome.to_string(), sub, ordina, riferimento: None, e_riferimento: false }
    }
    pub fn con_riferimento(mut self, r: Option<String>) -> Vista {
        self.riferimento = r;
        self
    }
    pub fn come_riferimento(mut self, e: bool) -> Vista {
        self.e_riferimento = e;
        self
    }

    /// Le chiavi che questa vista (e le sue sotto-viste) riconosce nelle chiavi date.
    fn filtra(&self, chiavi: &Chiavi) -> Chiavi {
        if self.e_riferimento && self.riferimento.is_none() {
            return Chiavi::default();
        }
        let sotto = match &self.sub {
            Some(s) => s.filtra(chiavi),
            None => Chiavi::default(),
        };
        let mut f = match chiavi.prendi(&self.nome) {
            Some(v) => sotto.sovrascrivi(&self.nome, v),
            None => sotto,
        };
        if let Some(r) = &self.riferimento {
            if f.prendi(&self.nome).is_some() {
                f = f.sovrascrivi(&self.nome, r);
            }
        }
        f
    }

    /// Applica la vista agli elementi (gli indici dati).
    pub fn applica(self: &Rc<Vista>, elementi: &[Elemento], idx: &[usize]) -> DatiVista {
        if self.e_riferimento && self.riferimento.is_none() {
            return DatiVista { vista: self.clone(), voci: Vec::new() };
        }
        let voci = (self.ordina)(elementi, idx)
            .into_iter()
            .map(|(k, v)| {
                let nodo = match &self.sub {
                    Some(s) => Nodo::Vista(s.applica(elementi, &v)),
                    None => Nodo::Foglie(v),
                };
                (k, nodo)
            })
            .collect();
        DatiVista { vista: self.clone(), voci }
    }
}

pub enum Nodo {
    Vista(DatiVista),
    Foglie(Vec<usize>),
}

/// Gli elementi già raggruppati da una vista.
pub struct DatiVista {
    vista: Rc<Vista>,
    voci: Vec<(String, Nodo)>,
}

enum Elemento_<'a> {
    Vista(&'a DatiVista),
    Foglie(&'a [usize]),
}

impl DatiVista {
    fn chiavi(&self) -> Vec<Chiavi> {
        let mut out = Vec::new();
        for (k, nodo) in &self.voci {
            let c = Chiavi::default().aggiungi(&self.vista.nome, k);
            match nodo {
                Nodo::Vista(v) => {
                    for sotto in v.chiavi() {
                        out.push(c.accoda(&sotto));
                    }
                }
                Nodo::Foglie(_) => out.push(c),
            }
        }
        out
    }

    fn prendi(&self, chiavi: &Chiavi) -> Option<Elemento_<'_>> {
        if chiavi.e_vuota() {
            return Some(Elemento_::Vista(self));
        }
        let v = chiavi.prendi(&self.vista.nome)?;
        let resto = chiavi.togli(&self.vista.nome);
        let (_, nodo) = self.voci.iter().find(|(k, _)| k == v)?;
        match nodo {
            Nodo::Vista(s) => s.prendi(&resto),
            Nodo::Foglie(f) => Some(Elemento_::Foglie(f)),
        }
    }
}

/// Da dove viene un calcolo.
pub enum Sorgente {
    /// Un valore delle misure (il Ct, il segnale normalizzato).
    Misura { nome: String, valore: String, obbligatoria: bool },
    /// Un altro calcolo.
    Calcolo(Rc<Config>),
}

pub fn misura(nome: &str, valore: &str, obbligatoria: bool) -> Sorgente {
    Sorgente::Misura { nome: nome.to_string(), valore: valore.to_string(), obbligatoria }
}

/// Un calcolo da cercare: il nome, il valore da leggere nel primo elemento della vista, le sorgenti.
pub struct Config {
    pub nome: String,
    pub valore: String,
    pub vista: Rc<DatiVista>,
    pub sorgenti: Vec<Sorgente>,
    pub unita: Option<String>,
    pub descrizione: Option<String>,
    /// Chiave del valore che dà la descrizione, se cambia da un elemento all'altro.
    pub descrizione_da: Option<String>,
    pub obbligatoria: bool,
    /// Se manca il calcolo, le sue sorgenti passano al calcolo che lo cita.
    pub opzionale: bool,
}

impl Config {
    pub fn nuova(nome: &str, valore: &str, vista: &Rc<DatiVista>, sorgenti: Vec<Sorgente>) -> Config {
        Config { nome: nome.to_string(), valore: valore.to_string(), vista: vista.clone(), sorgenti, unita: None, descrizione: None, descrizione_da: None, obbligatoria: false, opzionale: false }
    }
    pub fn opzionale(mut self) -> Config {
        self.opzionale = true;
        self
    }
    pub fn unita(mut self, u: &str) -> Config {
        self.unita = Some(u.to_string());
        self
    }
    pub fn descrizione(mut self, d: &str) -> Config {
        self.descrizione = Some(d.to_string());
        self
    }
    pub fn descrizione_da(mut self, k: &str) -> Config {
        self.descrizione_da = Some(k.to_string());
        self
    }
}

/// Da dove viene un valore calcolato.
#[derive(Clone, Debug, PartialEq)]
pub enum Fonte {
    /// Una misura (indice dell'elemento) e che cosa se n'è usato.
    Misura { elemento: usize, feature: String },
    /// Un altro calcolo (posizione nell'elenco finale).
    Calcolo { posizione: usize, feature: String },
}

/// Un valore calcolato, con le sue fonti.
#[derive(Clone, Debug)]
pub struct Doc {
    pub nome: String,
    pub valore: f64,
    pub fonti: Vec<Fonte>,
    pub unita: Option<String>,
    pub descrizione: Option<String>,
}

struct Bozza {
    nome: String,
    valore: f64,
    /// Le fonti con i calcoli ancora come indici della bozza.
    fonti: Vec<Fonte>,
    unita: Option<String>,
    descrizione: Option<String>,
}

struct Motore<'a> {
    elementi: &'a [Elemento],
    bozze: Vec<Bozza>,
    cache: HashMap<String, Option<usize>>,
}

impl Motore<'_> {
    fn chiave_cache(c: &Config, chiavi: &Chiavi) -> String {
        format!("{} {} {}", c.nome, c.valore, chiavi.chiave_stringa())
    }

    fn doc(&mut self, c: &Config, chiavi: &Chiavi) -> Option<usize> {
        let k = Self::chiave_cache(c, chiavi);
        if let Some(Some(i)) = self.cache.get(&k) {
            return Some(*i);
        }
        let r = self.doc_interno(c, chiavi);
        self.cache.insert(k, r);
        r
    }

    fn doc_interno(&mut self, c: &Config, chiavi: &Chiavi) -> Option<usize> {
        let foglie: Vec<usize> = match c.vista.prendi(chiavi)? {
            Elemento_::Foglie(f) => f.to_vec(),
            Elemento_::Vista(_) => return None,
        };
        let primo = *foglie.first()?;
        let valore = self.elementi[primo].numero(&c.valore)?;
        let mut fonti: Vec<Fonte> = Vec::new();
        for s in &c.sorgenti {
            let trovate = self.fonti_di(s, chiavi, &foglie);
            if !trovate.is_empty() {
                fonti.extend(trovate);
            } else if matches!(s, Sorgente::Misura { obbligatoria: true, .. }) || matches!(s, Sorgente::Calcolo(x) if x.obbligatoria) {
                return None;
            }
        }
        if fonti.is_empty() {
            return None;
        }
        let descrizione = match (&c.descrizione, &c.descrizione_da) {
            (Some(d), _) => Some(d.clone()),
            (None, Some(k)) => self.elementi[primo].testo(k),
            _ => None,
        };
        self.bozze.push(Bozza { nome: c.nome.clone(), valore, fonti, unita: c.unita.clone(), descrizione });
        Some(self.bozze.len() - 1)
    }

    fn fonti_di(&mut self, s: &Sorgente, genitore: &Chiavi, elementi: &[usize]) -> Vec<Fonte> {
        match s {
            Sorgente::Misura { nome, valore, .. } => elementi.iter().filter(|&&e| self.elementi[e].numero(valore).is_some()).map(|&e| Fonte::Misura { elemento: e, feature: nome.clone() }).collect(),
            Sorgente::Calcolo(c) => self.fonti_calcolo(c, genitore, elementi),
        }
    }

    fn fonti_calcolo(&mut self, c: &Rc<Config>, genitore: &Chiavi, elementi: &[usize]) -> Vec<Fonte> {
        let chiavi = c.vista.vista.filtra(genitore);
        let sotto: Vec<Chiavi> = match c.vista.prendi(&chiavi) {
            Some(Elemento_::Vista(v)) => v.chiavi(),
            Some(Elemento_::Foglie(_)) => vec![Chiavi::default()],
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        for sk in sotto {
            let nuove = chiavi.accoda(&sk);
            if let Some(i) = self.doc(c, &nuove) {
                out.push(Fonte::Calcolo { posizione: i, feature: self.bozze[i].nome.clone() });
            } else if c.opzionale {
                for s in &c.sorgenti {
                    out.extend(self.fonti_di(s, &nuove, elementi));
                }
            }
        }
        out
    }
}

/// Costruisce i calcoli di tutte le configurazioni, nell'ordine in cui li presenta lo strumento:
/// ogni calcolo seguito dai calcoli da cui dipende che non sono ancora stati elencati.
pub fn costruisci(elementi: &[Elemento], configs: &[Rc<Config>]) -> Vec<Doc> {
    let mut m = Motore { elementi, bozze: Vec::new(), cache: HashMap::new() };
    let mut principali: Vec<usize> = Vec::new();
    for c in configs {
        for chiavi in c.vista.chiavi() {
            if let Some(i) = m.doc(c, &chiavi) {
                principali.push(i);
            }
        }
    }
    // Scorre in profondità: il calcolo, poi quelli da cui viene (una volta sola).
    let mut ordine: Vec<usize> = Vec::new();
    let mut fatti = vec![false; m.bozze.len()];
    fn visita(i: usize, bozze: &[Bozza], fatti: &mut [bool], ordine: &mut Vec<usize>) {
        if fatti[i] {
            return;
        }
        fatti[i] = true;
        ordine.push(i);
        for f in &bozze[i].fonti {
            if let Fonte::Calcolo { posizione, .. } = f {
                visita(*posizione, bozze, fatti, ordine);
            }
        }
    }
    for i in principali {
        visita(i, &m.bozze, &mut fatti, &mut ordine);
    }
    let mut posizione = vec![usize::MAX; m.bozze.len()];
    for (p, &i) in ordine.iter().enumerate() {
        posizione[i] = p;
    }
    ordine
        .iter()
        .map(|&i| {
            let b = &m.bozze[i];
            Doc {
                nome: b.nome.clone(),
                valore: b.valore,
                fonti: b.fonti.iter().map(|f| match f {
                    Fonte::Calcolo { posizione: p, feature } => Fonte::Calcolo { posizione: posizione[*p], feature: feature.clone() },
                    altro => altro.clone(),
                }).collect(),
                unita: b.unita.clone(),
                descrizione: b.descrizione.clone(),
            }
        })
        .collect()
}
