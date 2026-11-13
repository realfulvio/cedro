//! Il lettore RTF: il file diventa un albero di gruppi, poi un interprete con lo stato a pila
//! (carattere, paragrafo, sezione) lo versa nel `Documento` di testo, come fanno docx.rs e odt.rs.
//! Tabelle (anche annidate), etichette degli elenchi già scritte (\listtext), intestazioni e piè,
//! note, campi (pagina, collegamenti), immagini PNG/JPEG e forme con immagine o testo.

use super::documento::*;
use super::pagina::{Dash, Rgb, Target};
use std::collections::HashMap;

// ---------------------------------------------------------------- albero

#[derive(Debug)]
enum Nodo {
    Gruppo(Vec<Nodo>),
    Parola(String, Option<i32>),
    Simbolo(char),
    Testo(Vec<u8>),
    Hex(u8),
    Bin(Vec<u8>),
}

/// Dal file all'albero, senza ricorsione (i file malformati non fanno saltare la pila).
fn albero(b: &[u8]) -> Vec<Nodo> {
    let mut pila: Vec<Vec<Nodo>> = vec![Vec::new()];
    let mut i = 0;
    let mut testo: Vec<u8> = Vec::new();
    let flush = |testo: &mut Vec<u8>, pila: &mut Vec<Vec<Nodo>>| {
        if !testo.is_empty() {
            pila.last_mut().unwrap().push(Nodo::Testo(std::mem::take(testo)));
        }
    };
    while i < b.len() {
        match b[i] {
            b'{' => {
                flush(&mut testo, &mut pila);
                if pila.len() < 4000 {
                    pila.push(Vec::new());
                }
                i += 1;
            }
            b'}' => {
                flush(&mut testo, &mut pila);
                if pila.len() > 1 {
                    let g = pila.pop().unwrap();
                    pila.last_mut().unwrap().push(Nodo::Gruppo(g));
                }
                i += 1;
            }
            b'\\' => {
                flush(&mut testo, &mut pila);
                i += 1;
                let Some(&c) = b.get(i) else { break };
                if c.is_ascii_alphabetic() {
                    let s = i;
                    while i < b.len() && b[i].is_ascii_alphabetic() && i - s < 32 {
                        i += 1;
                    }
                    let nome = String::from_utf8_lossy(&b[s..i]).into_owned();
                    let mut num = None;
                    let ns = i;
                    if i < b.len() && b[i] == b'-' {
                        i += 1;
                    }
                    while i < b.len() && b[i].is_ascii_digit() && i - ns < 11 {
                        i += 1;
                    }
                    if i > ns && !(i == ns + 1 && b[ns] == b'-') {
                        num = std::str::from_utf8(&b[ns..i]).ok().and_then(|t| t.parse::<i64>().ok()).map(|v| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
                    } else {
                        i = ns;
                    }
                    if i < b.len() && b[i] == b' ' {
                        i += 1;
                    }
                    if nome == "bin" {
                        let n = num.unwrap_or(0).max(0) as usize;
                        let e = (i + n).min(b.len());
                        pila.last_mut().unwrap().push(Nodo::Bin(b[i..e].to_vec()));
                        i = e;
                        continue;
                    }
                    pila.last_mut().unwrap().push(Nodo::Parola(nome, num));
                } else if c == b'\'' {
                    let h = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                    if let Some(v) = h {
                        pila.last_mut().unwrap().push(Nodo::Hex(v));
                        i += 3;
                    } else {
                        i += 1;
                    }
                } else {
                    // \ seguito da a capo vale come \par.
                    let s = if c == b'\r' || c == b'\n' { '\n' } else { c as char };
                    pila.last_mut().unwrap().push(Nodo::Simbolo(s));
                    i += 1;
                }
            }
            b'\r' | b'\n' => i += 1,
            c => {
                testo.push(c);
                i += 1;
            }
        }
    }
    flush(&mut testo, &mut pila);
    while pila.len() > 1 {
        let g = pila.pop().unwrap();
        pila.last_mut().unwrap().push(Nodo::Gruppo(g));
    }
    pila.pop().unwrap_or_default()
}

// ---------------------------------------------------------------- stato

#[derive(Clone, Debug)]
struct Car {
    font: i32,
    hp: i32,
    bold: bool,
    italic: bool,
    ul: u8,
    strike: bool,
    color: i32,
    highlight: i32,
    vert: i8,
    caps: bool,
    scaps: bool,
    hidden: bool,
    spacing: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Lato {
    Par(usize),
    Cella(usize),
    Riga(usize),
}

#[derive(Clone, Debug, Default)]
struct ParP {
    allinea: Allinea,
    li: i32,
    ri: i32,
    fi: i32,
    sb: i32,
    sa: i32,
    sb_auto: bool,
    sa_auto: bool,
    sl: i32,
    slmult: bool,
    keepn: bool,
    keep: bool,
    pagebb: bool,
    widctl: bool,
    tabs: Vec<Tab>,
    tab_tipo: Option<TipoTab>,
    tab_leader: Option<char>,
    bordi: [Option<Linea>; 5],
    sfondo: Option<i32>,
    stile: i32,
    outline: Option<u8>,
    intbl: bool,
    itap: i32,
    contestuale: bool,
}

#[derive(Clone, Debug)]
struct Stato {
    car: Car,
    par: ParP,
    uc: usize,
    lato: Option<Lato>,
    nest_props: bool,
}

#[derive(Clone, Debug, Default)]
struct CellaDef {
    destra: i32,
    vmerge: Option<bool>,
    hmerge: Option<bool>,
    valign: u8,
    bordi: [Option<Linea>; 4],
    sfondo: Option<i32>,
    pad: [Option<f32>; 4],
}

#[derive(Clone, Debug, Default)]
struct RigaDef {
    sinistra: i32,
    gaph: i32,
    pad: [Option<f32>; 4],
    altezza: i32,
    intestazione: bool,
    non_dividere: bool,
    allinea: u8,
    bordi: [Option<Linea>; 6],
    celle: Vec<CellaDef>,
}

#[derive(Default)]
struct Livello {
    cella: Vec<Blocco>,
    celle: Vec<Vec<Blocco>>,
    righe: Vec<(RigaDef, Vec<Vec<Blocco>>)>,
    def: RigaDef,
}

/// Dove va il testo: il corpo, un'intestazione, una nota, una casella.
#[derive(Default)]
struct Contesto {
    corpo: Vec<Blocco>,
    livelli: Vec<Livello>,
    runs: Vec<Run>,
    /// Testo, carattere (se l'etichetta ne dice uno) e tabulazione dopo.
    etichetta: Option<(String, Option<Font>, bool)>,
    segnalibri: Vec<String>,
    ancorati: Vec<Mobile>,
}

#[derive(Clone, Debug)]
struct SezP {
    w: f32,
    h: f32,
    margini: [f32; 4],
    testa_y: f32,
    piede_y: f32,
    colonne: u8,
    spazio: f32,
    prima_diversa: bool,
    salto: u8,
    primo: Option<u32>,
    riparte: bool,
    orizzontale: bool,
    testa: Intestazioni,
    piede: Intestazioni,
}

impl Default for SezP {
    fn default() -> Self {
        SezP { w: 612.0, h: 792.0, margini: [90.0, 90.0, 72.0, 72.0], testa_y: 36.0, piede_y: 36.0, colonne: 1, spazio: 36.0, prima_diversa: false, salto: 1, primo: None, riparte: false, orizzontale: false, testa: Intestazioni::default(), piede: Intestazioni::default() }
    }
}

struct FontDef {
    nome: String,
    cp: u32,
}

struct Lettore {
    fonts: HashMap<i32, FontDef>,
    colori: Vec<Option<Rgb>>,
    stili: HashMap<i32, (String, Option<u8>)>,
    /// Le parole di controllo degli stili: (carattere?, numero) → (basato su, parole).
    parole_stili: HashMap<(bool, i32), (Option<i32>, Vec<(String, Option<i32>)>)>,
    /// \defchp e \defpap: quel che \plain e \pard rimettono.
    defchp: Vec<(String, Option<i32>)>,
    defpap: Vec<(String, Option<i32>)>,
    deff: i32,
    ansicpg: u32,
    doc: Documento,
    ctx: Contesto,
    sez_doc: SezP,
    sez: SezP,
    sezioni: Vec<Sezione>,
    /// Byte di testo in attesa (le sequenze \'xx dei caratteri a due byte vanno insieme).
    attesa: Vec<u8>,
    attesa_cp: u32,
    attesa_car: Option<Car>,
    salta: usize,
    alto: Option<u16>,
    link: Option<Target>,
    profondita: usize,
    /// Il numero della nota che si sta leggendo (per \chftn).
    nota_corrente: Option<String>,
}

/// Le parole di uno stile che dicono di lui, non del testo.
const STILE_META: [&str; 22] = ["s", "cs", "ds", "ts", "sbasedon", "snext", "slink", "sautoupd", "shidden", "spersonal", "scompose", "sreply", "sqformat", "spriority", "ssemihidden", "sunhideused", "slocked", "styrsid", "additive", "ls", "ilvl", "par"];

fn car_default(deff: i32) -> Car {
    Car { font: deff, hp: 24, bold: false, italic: false, ul: 0, strike: false, color: 0, highlight: 0, vert: 0, caps: false, scaps: false, hidden: false, spacing: 0.0 }
}

fn par_default() -> ParP {
    ParP { widctl: false, ..Default::default() }
}

fn tw(v: i32) -> f32 {
    v as f32 / 20.0
}

fn codifica(cp: u32) -> Option<&'static encoding_rs::Encoding> {
    use encoding_rs::*;
    Some(match cp {
        874 => WINDOWS_874,
        932 => SHIFT_JIS,
        936 => GBK,
        949 => EUC_KR,
        950 => BIG5,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1252 => WINDOWS_1252,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        866 => IBM866,
        10000 => MACINTOSH,
        65001 => UTF_8,
        _ => return None,
    })
}

fn cp_di_charset(cs: i32, ansi: u32) -> u32 {
    match cs {
        2 => 42,
        77 => 10000,
        128 => 932,
        129 => 949,
        134 => 936,
        136 => 950,
        161 => 1253,
        162 => 1254,
        163 => 1258,
        177 => 1255,
        178 => 1256,
        186 => 1257,
        204 => 1251,
        222 => 874,
        238 => 1250,
        _ => ansi,
    }
}

/// Il testo semplice di un gruppo (nomi nelle tabelle, istruzioni dei campi).
fn testo_di(nodi: &[Nodo], out: &mut String) {
    for n in nodi {
        match n {
            Nodo::Testo(t) => out.extend(t.iter().map(|b| *b as char)),
            Nodo::Hex(h) => out.push(crate::doctext::cp1252(*h)),
            Nodo::Gruppo(g) => testo_di(g, out),
            Nodo::Simbolo(c) if matches!(c, '\\' | '{' | '}') => out.push(*c),
            Nodo::Parola(p, Some(v)) if p == "u" => {
                if let Some(c) = char::from_u32(if *v < 0 { (*v + 65536) as u32 } else { *v as u32 }) {
                    out.push(c);
                }
            }
            _ => {}
        }
    }
}

fn parola0(g: &[Nodo]) -> Option<(&str, bool)> {
    let mut stella = false;
    for n in g.iter().take(3) {
        match n {
            Nodo::Simbolo('*') => stella = true,
            Nodo::Parola(p, _) => return Some((p.as_str(), stella)),
            Nodo::Testo(t) if t.iter().all(|c| c.is_ascii_whitespace()) => {}
            _ => return None,
        }
    }
    None
}

fn ha_parola(g: &[Nodo], nome: &str) -> bool {
    g.iter().any(|n| matches!(n, Nodo::Parola(p, _) if p == nome))
}

fn sotto<'a>(g: &'a [Nodo], nome: &str) -> Option<&'a [Nodo]> {
    g.iter().find_map(|n| match n {
        Nodo::Gruppo(s) if parola0(s).is_some_and(|(p, _)| p == nome) => Some(s.as_slice()),
        _ => None,
    })
}

impl Lettore {
    fn nuovo() -> Self {
        Lettore {
            fonts: HashMap::new(),
            colori: Vec::new(),
            stili: HashMap::new(),
            parole_stili: HashMap::new(),
            defchp: Vec::new(),
            defpap: Vec::new(),
            deff: 0,
            ansicpg: 1252,
            doc: Documento { tab_predefinita: 36.0, ..Default::default() },
            ctx: Contesto::default(),
            sez_doc: SezP::default(),
            sez: SezP::default(),
            sezioni: Vec::new(),
            attesa: Vec::new(),
            attesa_cp: 1252,
            attesa_car: None,
            salta: 0,
            alto: None,
            link: None,
            profondita: 0,
            nota_corrente: None,
        }
    }

    fn colore(&self, i: i32) -> Option<Rgb> {
        self.colori.get(i.max(0) as usize).copied().flatten()
    }

    fn font_di(&self, c: &Car) -> Font {
        Font {
            family: self.fonts.get(&c.font).map(|f| f.nome.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Times New Roman".into()),
            size: (c.hp.max(2) as f32 / 2.0).min(1638.0),
            bold: c.bold,
            italic: c.italic,
            underline: c.ul == 1,
            double_underline: c.ul == 2,
            strike: c.strike,
            color: self.colore(c.color).unwrap_or((0, 0, 0)),
            highlight: (c.highlight > 0).then(|| self.colore(c.highlight)).flatten(),
            vert: c.vert,
            caps: c.caps,
            small_caps: c.scaps,
            spacing: c.spacing,
        }
    }

    fn cp_di(&self, c: &Car) -> u32 {
        self.fonts.get(&c.font).map_or(self.ansicpg, |f| f.cp)
    }

    // ---------------------------------------------------------------- testo

    fn scrivi(&mut self, s: &str, car: &Car) {
        if car.hidden || s.is_empty() {
            return;
        }
        let font = self.font_di(car);
        let link = self.link.clone();
        if let Some(Run { pezzo: Pezzo::Testo(t), font: f, link: l }) = self.ctx.runs.last_mut() {
            if *f == font && *l == link {
                t.push_str(s);
                return;
            }
        }
        self.ctx.runs.push(Run { pezzo: Pezzo::Testo(s.to_string()), font, link });
    }

    fn pezzo(&mut self, p: Pezzo, car: &Car) {
        self.svuota();
        if car.hidden {
            return;
        }
        let font = self.font_di(car);
        self.ctx.runs.push(Run { pezzo: p, font, link: self.link.clone() });
    }

    fn byte(&mut self, b: u8, car: &Car) {
        if self.salta > 0 {
            self.salta -= 1;
            return;
        }
        let cp = self.cp_di(car);
        if !self.attesa.is_empty() && (self.attesa_cp != cp || self.attesa_car.as_ref().is_some_and(|c| c.font != car.font || c.hidden != car.hidden)) {
            self.svuota();
        }
        self.attesa_cp = cp;
        if self.attesa_car.is_none() {
            self.attesa_car = Some(car.clone());
        }
        self.attesa.push(b);
    }

    fn svuota(&mut self) {
        if self.attesa.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.attesa);
        let car = self.attesa_car.take().unwrap_or_else(|| car_default(self.deff));
        let s: String = if self.attesa_cp == 42 {
            bytes.iter().map(|b| char::from_u32(0xF000 + *b as u32).unwrap_or(' ')).collect()
        } else if let Some(enc) = codifica(self.attesa_cp) {
            enc.decode_without_bom_handling(&bytes).0.into_owned()
        } else {
            bytes.iter().map(|b| crate::doctext::cp1252(*b)).collect()
        };
        self.scrivi(&s, &car);
    }

    fn unicode(&mut self, v: i32, st: &Stato) {
        self.svuota();
        let u = if v < 0 { (v + 65536) as u32 } else { v as u32 };
        let mut s = String::new();
        if (0xD800..0xDC00).contains(&u) {
            self.alto = Some(u as u16);
        } else if (0xDC00..0xE000).contains(&u) {
            if let Some(h) = self.alto.take() {
                if let Some(c) = char::decode_utf16([h, u as u16]).next().and_then(|r| r.ok()) {
                    s.push(c);
                }
            }
        } else if let Some(c) = char::from_u32(u) {
            // Nei caratteri simbolo il codice privato resta com'è (lo traduce l'etichetta).
            s.push(c);
        }
        let car = st.car.clone();
        self.scrivi(&s, &car);
        self.salta = st.uc;
    }

    // ---------------------------------------------------------------- paragrafi e tabelle

    fn livello_par(p: &ParP) -> usize {
        if p.itap > 0 {
            p.itap as usize
        } else if p.intbl {
            1
        } else {
            0
        }
    }

    fn paragrafo(&mut self, st: &Stato) -> Paragrafo {
        self.svuota();
        let p = &st.par;
        let runs = std::mem::take(&mut self.ctx.runs);
        let interlinea = if p.sl == 0 {
            Interlinea::Multipla(1.0)
        } else if p.sl < 0 {
            Interlinea::Esatta(tw(-p.sl))
        } else if p.slmult {
            Interlinea::Multipla(p.sl as f32 / 240.0)
        } else {
            Interlinea::Minima(tw(p.sl))
        };
        let (nome, liv_stile) = self.stili.get(&p.stile).cloned().unwrap_or_default();
        let titolo = p.outline.map(|l| l + 1).or(liv_stile).filter(|l| (1..=9).contains(l));
        let et = self.ctx.etichetta.take();
        Paragrafo {
            runs,
            allinea: p.allinea,
            rientro_sx: tw(p.li),
            rientro_dx: tw(p.ri),
            prima_riga: tw(p.fi),
            prima: if p.sb_auto { 14.0 } else { tw(p.sb) },
            dopo: if p.sa_auto { 14.0 } else { tw(p.sa) },
            interlinea,
            insieme_al_seguente: p.keepn,
            righe_insieme: p.keep,
            salto_prima: p.pagebb,
            vedove: p.widctl,
            tabs: p.tabs.clone(),
            etichetta_tab: et.as_ref().is_some_and(|e| e.2),
            etichetta_allinea: Allinea::Sinistra,
            etichetta: et.map(|(t, f, _)| (t, f.unwrap_or_else(|| self.font_di(&st.car)))),
            titolo,
            sfondo: p.sfondo.and_then(|c| self.colore(c)),
            bordi: p.bordi,
            font_fine: self.font_di(&st.car),
            contestuale: p.contestuale,
            stile: nome,
            segnalibri: std::mem::take(&mut self.ctx.segnalibri),
            auto_prima: p.sb_auto,
            auto_dopo: p.sa_auto,
            interlinea_testo: false,
        }
    }

    /// Chiude le tabelle più profonde di `n` e le versa dove stanno.
    fn chiudi_livelli(&mut self, n: usize) {
        while self.ctx.livelli.len() > n {
            let lv = self.ctx.livelli.pop().unwrap();
            let mut celle_aperte = lv.cella;
            let tab = self.tabella(lv.righe);
            let dest = match self.ctx.livelli.last_mut() {
                Some(up) => &mut up.cella,
                None => &mut self.ctx.corpo,
            };
            if let Some(t) = tab {
                dest.push(Blocco::Tab(t));
            }
            // Testo rimasto fuori da una cella (file malformato): non si perde.
            dest.append(&mut celle_aperte);
        }
    }

    fn metti(&mut self, b: Blocco, livello: usize) {
        if livello < self.ctx.livelli.len() {
            self.chiudi_livelli(livello);
        }
        while self.ctx.livelli.len() < livello {
            self.ctx.livelli.push(Livello::default());
        }
        for m in std::mem::take(&mut self.ctx.ancorati) {
            match self.ctx.livelli.last_mut() {
                Some(lv) if livello > 0 => lv.cella.push(Blocco::Mobile(m)),
                _ => self.ctx.corpo.push(Blocco::Mobile(m)),
            }
        }
        match self.ctx.livelli.last_mut() {
            Some(lv) if livello > 0 => lv.cella.push(b),
            _ => self.ctx.corpo.push(b),
        }
    }

    fn fine_par(&mut self, st: &Stato) {
        let p = self.paragrafo(st);
        self.metti(Blocco::Par(p), Self::livello_par(&st.par));
    }

    fn fine_cella(&mut self, st: &Stato, livello: usize) {
        self.svuota();
        let livello = livello.max(1);
        let vuota = self.ctx.livelli.get(livello - 1).is_none_or(|l| l.cella.is_empty());
        if !self.ctx.runs.is_empty() || self.ctx.etichetta.is_some() || vuota || self.ctx.livelli.len() > livello {
            let p = self.paragrafo(st);
            self.metti(Blocco::Par(p), livello);
        }
        if let Some(lv) = self.ctx.livelli.get_mut(livello - 1) {
            let c = std::mem::take(&mut lv.cella);
            lv.celle.push(c);
        }
    }

    fn fine_riga(&mut self, livello: usize) {
        let livello = livello.max(1);
        if self.ctx.livelli.len() > livello {
            self.chiudi_livelli(livello);
        }
        while self.ctx.livelli.len() < livello {
            self.ctx.livelli.push(Livello::default());
        }
        let lv = &mut self.ctx.livelli[livello - 1];
        let celle = std::mem::take(&mut lv.celle);
        if !celle.is_empty() {
            lv.righe.push((lv.def.clone(), celle));
        }
    }

    fn tabella(&self, righe: Vec<(RigaDef, Vec<Vec<Blocco>>)>) -> Option<Tabella> {
        if righe.is_empty() {
            return None;
        }
        // La griglia: tutti i bordi delle celle, in ordine.
        let mut x: Vec<i32> = Vec::new();
        for (d, celle) in &righe {
            x.push(d.sinistra);
            for c in d.celle.iter().take(celle.len().max(1)) {
                x.push(c.destra);
            }
        }
        x.sort_unstable();
        x.dedup_by(|a, b| (*a - *b).abs() <= 2);
        if x.len() < 2 {
            x = vec![0, 2000];
        }
        let idx = |v: i32| x.iter().position(|g| (g - v).abs() <= 2).unwrap_or_else(|| x.iter().rposition(|g| *g <= v).unwrap_or(0));
        let griglia: Vec<f32> = x.windows(2).map(|w| tw(w[1] - w[0]).max(1.0)).collect();
        let ncols = griglia.len();
        let primo = righe[0].0.clone();
        let mut out = Vec::new();
        for (d, celle) in righe {
            let mut row: Vec<Cella> = Vec::new();
            let mut col = 0usize;
            let mut sx = d.sinistra;
            for (k, blocchi) in celle.into_iter().enumerate() {
                let cd = d.celle.get(k).cloned().unwrap_or_else(|| CellaDef { destra: sx + 1440, ..Default::default() });
                let a = if k == 0 { 0 } else { idx(sx).min(ncols.saturating_sub(1)) };
                let b = idx(cd.destra).max(a + 1).min(ncols);
                let span = if col >= ncols { 1 } else { (b.max(col + 1) - col).max(1) };
                let _ = a;
                sx = cd.destra;
                if cd.hmerge == Some(false) {
                    if let Some(prev) = row.last_mut() {
                        prev.span += span;
                        col += span;
                        continue;
                    }
                }
                let pad_l = cd.pad[0].or(d.pad[0]).unwrap_or(tw(d.gaph));
                let pad_r = cd.pad[1].or(d.pad[1]).unwrap_or(tw(d.gaph));
                let pad_t = cd.pad[2].or(d.pad[2]).unwrap_or(0.0);
                let pad_b = cd.pad[3].or(d.pad[3]).unwrap_or(0.0);
                let rb = &d.bordi;
                let mut blocchi = blocchi;
                if blocchi.is_empty() {
                    blocchi.push(Blocco::Par(Paragrafo::default()));
                }
                row.push(Cella {
                    blocchi,
                    span,
                    vmerge: cd.vmerge,
                    sfondo: cd.sfondo.and_then(|c| self.colore(c)),
                    bordi: [cd.bordi[0].or(rb[0]).or(rb[4]), cd.bordi[1].or(rb[1]).or(rb[4]), cd.bordi[2].or(rb[2]).or(rb[5]), cd.bordi[3].or(rb[3]).or(rb[5])],
                    valign: cd.valign,
                    margini: [pad_l, pad_r, pad_t, pad_b],
                    larghezza: None,
                });
                col += span;
            }
            out.push(Riga { celle: row, altezza: tw(d.altezza), intestazione: d.intestazione, non_dividere: d.non_dividere });
        }
        let pad0 = primo.celle.first().and_then(|c| c.pad[0]).or(primo.pad[0]).unwrap_or(tw(primo.gaph));
        Some(Tabella { griglia, righe: out, allinea: primo.allinea, rientro: tw(x[0]) + pad0, relativa: None })
    }

    fn fine_sezione(&mut self, finale: bool) {
        self.svuota();
        self.chiudi_livelli(0);
        for m in std::mem::take(&mut self.ctx.ancorati) {
            self.ctx.corpo.push(Blocco::Mobile(m));
        }
        let blocchi = std::mem::take(&mut self.ctx.corpo);
        if finale && blocchi.is_empty() && !self.sezioni.is_empty() {
            return;
        }
        let s = &self.sez;
        let (mut w, mut h) = (s.w, s.h);
        if s.orizzontale && w < h {
            std::mem::swap(&mut w, &mut h);
        }
        self.sezioni.push(Sezione {
            w,
            h,
            margini: s.margini,
            dist_testa: s.testa_y,
            dist_piede: s.piede_y,
            colonne: s.colonne.max(1),
            spazio_colonne: s.spazio,
            testa: s.testa.clone(),
            piede: s.piede.clone(),
            prima_diversa: s.prima_diversa,
            pagina_nuova: s.salto != 0 || self.sezioni.is_empty(),
            parita: match s.salto {
                2 => 1,
                3 => 2,
                _ => 0,
            },
            primo_numero: if s.riparte || self.sezioni.is_empty() { s.primo } else { None },
            blocchi,
        });
    }

    // ---------------------------------------------------------------- gruppi speciali

    /// Legge un gruppo in un contesto a parte (intestazione, nota, casella) e ne rende i blocchi.
    fn a_parte(&mut self, g: &[Nodo], st: &Stato) -> Vec<Blocco> {
        self.svuota();
        let salvato = std::mem::take(&mut self.ctx);
        let link = self.link.take();
        let mut s2 = st.clone();
        s2.par = par_default();
        self.gruppo(g, &mut s2);
        self.svuota();
        if !self.ctx.runs.is_empty() || self.ctx.etichetta.is_some() {
            self.fine_par(&s2);
        }
        self.chiudi_livelli(0);
        for m in std::mem::take(&mut self.ctx.ancorati) {
            self.ctx.corpo.push(Blocco::Mobile(m));
        }
        let fatto = std::mem::replace(&mut self.ctx, salvato);
        self.link = link;
        fatto.corpo
    }

    fn tabella_font(&mut self, g: &[Nodo]) {
        let voce = |me: &mut Self, v: &[Nodo]| {
            let mut id = None;
            let mut cs = 0;
            let mut cpg = None;
            let mut nome = String::new();
            for n in v {
                match n {
                    Nodo::Parola(p, Some(x)) if p == "f" => id = Some(*x),
                    Nodo::Parola(p, Some(x)) if p == "fcharset" => cs = *x,
                    Nodo::Parola(p, Some(x)) if p == "cpg" => cpg = Some(*x as u32),
                    Nodo::Testo(t) => nome.extend(t.iter().map(|b| *b as char)),
                    Nodo::Hex(h) => nome.push(crate::doctext::cp1252(*h)),
                    _ => {}
                }
            }
            if let Some(id) = id {
                let nome = nome.split(';').next().unwrap_or("").trim().to_string();
                let cp = cpg.unwrap_or_else(|| cp_di_charset(cs, me.ansicpg));
                me.fonts.insert(id, FontDef { nome, cp });
            }
        };
        // Le voci sono gruppi, ma i file scritti a mano le mettono anche di seguito.
        let mut piatta: Vec<&Nodo> = Vec::new();
        for n in g {
            match n {
                Nodo::Gruppo(v) => voce(self, v),
                n => piatta.push(n),
            }
        }
        let mut cur: Vec<Nodo> = Vec::new();
        for n in piatta {
            match n {
                Nodo::Parola(p, x) if p == "f" => {
                    if !cur.is_empty() {
                        voce(self, &cur);
                        cur.clear();
                    }
                    cur.push(Nodo::Parola(p.clone(), *x));
                }
                Nodo::Parola(p, x) => cur.push(Nodo::Parola(p.clone(), *x)),
                Nodo::Testo(t) => cur.push(Nodo::Testo(t.clone())),
                _ => {}
            }
        }
        if !cur.is_empty() {
            voce(self, &cur);
        }
    }

    fn tabella_colori(&mut self, g: &[Nodo]) {
        let (mut r, mut gr, mut b, mut any) = (0, 0, 0, false);
        for n in g {
            match n {
                Nodo::Parola(p, Some(v)) if p == "red" => (r, any) = (*v, true),
                Nodo::Parola(p, Some(v)) if p == "green" => (gr, any) = (*v, true),
                Nodo::Parola(p, Some(v)) if p == "blue" => (b, any) = (*v, true),
                Nodo::Testo(t) => {
                    for c in t {
                        if *c == b';' {
                            self.colori.push(any.then_some((r.clamp(0, 255) as u8, gr.clamp(0, 255) as u8, b.clamp(0, 255) as u8)));
                            (r, gr, b, any) = (0, 0, 0, false);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn tabella_stili(&mut self, g: &[Nodo]) {
        for n in g {
            let Nodo::Gruppo(v) = n else { continue };
            let mut id = 0;
            let mut par = true;
            let mut car = false;
            let mut liv = None;
            let mut base = None;
            let mut parole = Vec::new();
            for x in v {
                match x {
                    Nodo::Parola(p, Some(k)) if p == "s" => id = *k,
                    Nodo::Parola(p, k) if p == "cs" => {
                        par = false;
                        car = true;
                        id = k.unwrap_or(0);
                    }
                    Nodo::Parola(p, _) if p == "ts" || p == "ds" => par = false,
                    Nodo::Parola(p, Some(k)) if p == "sbasedon" => base = Some(*k),
                    Nodo::Parola(p, Some(k)) if p == "outlinelevel" => {
                        liv = u8::try_from(*k + 1).ok();
                        parole.push((p.clone(), Some(*k)));
                    }
                    Nodo::Parola(p, k) if !STILE_META.contains(&p.as_str()) => parole.push((p.clone(), *k)),
                    _ => {}
                }
            }
            if par || car {
                self.parole_stili.insert((car, id), (base, parole));
            }
            if !par {
                continue;
            }
            let mut nome = String::new();
            for x in v {
                match x {
                    Nodo::Testo(t) => nome.extend(t.iter().map(|b| *b as char)),
                    Nodo::Hex(h) => nome.push(crate::doctext::cp1252(*h)),
                    _ => {}
                }
            }
            let nome = nome.split(';').next().unwrap_or("").trim().to_string();
            let low = nome.to_lowercase();
            if liv.is_none() {
                for pre in ["heading ", "titolo ", "überschrift ", "titre ", "título ", "titulo "] {
                    if let Some(k) = low.strip_prefix(pre).and_then(|r| r.trim().parse::<u8>().ok()) {
                        liv = Some(k);
                    }
                }
            }
            self.stili.insert(id, (nome, liv));
        }
    }

    fn immagine(&mut self, g: &[Nodo]) -> Option<(usize, f32, f32)> {
        let mut tipo = "";
        let (mut pw, mut ph, mut gw, mut gh, mut sx, mut sy) = (0, 0, 0, 0, 100, 100);
        let mut hex: Vec<u8> = Vec::new();
        let mut bin: Option<&[u8]> = None;
        for n in g {
            match n {
                Nodo::Parola(p, v) => match (p.as_str(), v) {
                    ("pngblip", _) => tipo = "png",
                    ("jpegblip", _) => tipo = "jpg",
                    ("emfblip", _) => tipo = "emf",
                    ("wmetafile", _) => tipo = "wmf",
                    ("dibitmap", _) => tipo = "dib",
                    ("wbitmap", _) | ("macpict", _) | ("pmmetafile", _) => tipo = "altro",
                    ("picw", Some(x)) => pw = *x,
                    ("pich", Some(x)) => ph = *x,
                    ("picwgoal", Some(x)) => gw = *x,
                    ("pichgoal", Some(x)) => gh = *x,
                    ("picscalex", Some(x)) => sx = *x,
                    ("picscaley", Some(x)) => sy = *x,
                    _ => {}
                },
                Nodo::Testo(t) => hex.extend(t.iter().filter(|c| c.is_ascii_hexdigit())),
                Nodo::Bin(b) => bin = Some(b),
                _ => {}
            }
        }
        let data: Vec<u8> = match bin {
            Some(b) => b.to_vec(),
            None => hex.chunks_exact(2).filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok()).collect(),
        };
        let mut w = if gw > 0 { tw(gw) } else { pw as f32 * 0.75 } * sx.max(1) as f32 / 100.0;
        let mut h = if gh > 0 { tw(gh) } else { ph as f32 * 0.75 } * sy.max(1) as f32 / 100.0;
        let bytes = match tipo {
            "png" | "jpg" => data,
            "dib" => {
                // Un DIB senza intestazione di file: si aggiunge quella del BMP.
                if data.len() < 40 {
                    return None;
                }
                let hdr = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let bpp = u16::from_le_bytes([data[14], data[15]]);
                let used = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);
                let pal = if bpp <= 8 { if used > 0 { used } else { 1 << bpp } } else { 0 };
                let off = 14 + hdr + pal * 4;
                let mut f = b"BM".to_vec();
                f.extend(((14 + data.len()) as u32).to_le_bytes());
                f.extend([0, 0, 0, 0]);
                f.extend(off.to_le_bytes());
                f.extend(data);
                f
            }
            _ => return None,
        };
        let img = crate::pdfdoc::DocImage::from_bytes(&bytes, (w > 0.0 && h > 0.0).then_some((w, h)), 0)?;
        if w <= 0.0 || h <= 0.0 {
            (w, h) = (img.px.0 as f32 * 0.75, img.px.1 as f32 * 0.75);
        }
        self.doc.immagini.push(img);
        Some((self.doc.immagini.len() - 1, w, h))
    }

    /// Una forma (\shp): immagine o casella di testo, ancorata al paragrafo.
    fn forma(&mut self, g: &[Nodo], st: &Stato) {
        let Some(inst) = sotto(g, "shpinst") else {
            if let Some(r) = sotto(g, "shprslt") {
                let mut s2 = st.clone();
                self.gruppo(r, &mut s2);
            }
            return;
        };
        let (mut l, mut t, mut r, mut b) = (0, 0, 0, 0);
        let (mut bx, mut by) = (Base::Colonna, Base::Paragrafo);
        let mut wr = 3;
        let mut dietro = false;
        let mut props: HashMap<String, &[Nodo]> = HashMap::new();
        for n in inst {
            match n {
                Nodo::Parola(p, v) => match (p.as_str(), v) {
                    ("shpleft", Some(x)) => l = *x,
                    ("shptop", Some(x)) => t = *x,
                    ("shpright", Some(x)) => r = *x,
                    ("shpbottom", Some(x)) => b = *x,
                    ("shpbxpage", _) => bx = Base::Pagina,
                    ("shpbxmargin", _) => bx = Base::Margine,
                    ("shpbxcolumn", _) => bx = Base::Colonna,
                    ("shpbypage", _) => by = Base::Pagina,
                    ("shpbymargin", _) => by = Base::Margine,
                    ("shpbypara", _) => by = Base::Paragrafo,
                    ("shpwr", Some(x)) => wr = *x,
                    ("shpfblwtxt", Some(x)) => dietro = *x != 0,
                    _ => {}
                },
                Nodo::Gruppo(sp) if parola0(sp).is_some_and(|(p, _)| p == "sp") => {
                    let mut nome = String::new();
                    if let Some(sn) = sotto(sp, "sn") {
                        testo_di(sn, &mut nome);
                    }
                    if let Some(sv) = sotto(sp, "sv") {
                        props.insert(nome.trim().to_string(), sv);
                    }
                }
                _ => {}
            }
        }
        let pv = |k: &str| -> Option<i32> {
            let mut s = String::new();
            testo_di(props.get(k)?, &mut s);
            s.trim().parse().ok()
        };
        // Posizioni relative scritte nelle proprietà (Word 2007+).
        if let Some(v) = pv("posrelh") {
            bx = match v {
                0 => Base::Margine,
                1 => Base::Pagina,
                _ => Base::Colonna,
            };
        }
        if let Some(v) = pv("posrelv") {
            by = match v {
                0 => Base::Margine,
                1 => Base::Pagina,
                _ => Base::Paragrafo,
            };
        }
        let x = match pv("posh") {
            Some(1) => Riferimento::Allineato(bx, 0),
            Some(2) => Riferimento::Allineato(bx, 1),
            Some(3) => Riferimento::Allineato(bx, 2),
            _ => Riferimento::Spostato(bx, tw(l)),
        };
        let y = match pv("posv") {
            Some(1) => Riferimento::Allineato(by, 0),
            Some(2) => Riferimento::Allineato(by, 1),
            Some(3) => Riferimento::Allineato(by, 2),
            _ => Riferimento::Spostato(by, tw(t)),
        };
        let (w, h) = (tw(r - l).max(1.0), tw(b - t).max(1.0));
        let avvolge = match wr {
            1 => Avvolge::SopraSotto,
            2 | 4 | 5 => Avvolge::Lati,
            _ => Avvolge::Nessuno,
        };
        let contenuto = if let Some(txt) = sotto(inst, "shptxt") {
            let blocchi = self.a_parte(txt, st);
            let fill = (pv("fFilled") != Some(0)).then(|| pv("fillColor").map(|c| ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8)).or(Some((255, 255, 255)))).flatten();
            let line = (pv("fLine") != Some(0)).then(|| Linea {
                width: pv("lineWidth").map_or(0.75, |v| v as f32 / 12700.0),
                color: pv("lineColor").map_or((0, 0, 0), |c| ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8)),
                dash: Dash::Solid,
                double: false,
                space: 0.0,
            });
            MobileContenuto::Testo(blocchi, fill, line)
        } else if let Some(pict) = props.get("pib").and_then(|sv| sv.iter().find_map(|n| if let Nodo::Gruppo(p) = n { (parola0(p).is_some_and(|(x, _)| x == "pict")).then_some(p.as_slice()) } else { None })) {
            match self.immagine(pict) {
                Some((i, _, _)) => MobileContenuto::Immagine(i),
                None => return,
            }
        } else {
            return;
        };
        self.ctx.ancorati.push(Mobile { contenuto, w, h, x, y, avvolge, dietro });
    }

    fn campo(&mut self, g: &[Nodo], st: &Stato) {
        let mut istr = String::new();
        if let Some(inst) = sotto(g, "fldinst") {
            testo_di(inst, &mut istr);
        }
        let istr = istr.trim().to_string();
        let parola = istr.split_whitespace().next().unwrap_or("").to_uppercase();
        let ris = sotto(g, "fldrslt");
        // Il carattere del risultato (la prima parola di carattere dentro il risultato).
        let car_ris = |me: &Self| -> Car {
            let mut s2 = st.clone();
            if let Some(r) = ris {
                for n in r.iter().chain(r.iter().filter_map(|n| if let Nodo::Gruppo(x) = n { Some(x.iter()) } else { None }).flatten()) {
                    if let Nodo::Parola(p, v) = n {
                        me.car_parola(&mut s2.car, p, *v);
                    }
                }
            }
            s2.car
        };
        match parola.as_str() {
            "PAGE" => {
                let c = car_ris(self);
                self.pezzo(Pezzo::NumPagina, &c);
            }
            "NUMPAGES" | "SECTIONPAGES" => {
                let c = car_ris(self);
                self.pezzo(Pezzo::NumPagine, &c);
            }
            "HYPERLINK" => {
                let mut parti = istr.split('"').skip(1).step_by(2);
                let prima = parti.next().unwrap_or("").to_string();
                let anchor = istr.find("\\l").map(|i| istr[i + 2..].trim().trim_matches('"').split('"').next().unwrap_or("").to_string());
                let target = match anchor {
                    Some(a) if !a.is_empty() && !istr.split_whitespace().nth(1).is_some_and(|t| t.starts_with('"')) => Target::Anchor(a),
                    _ if prima.is_empty() => {
                        let t = istr.split_whitespace().nth(1).unwrap_or("").to_string();
                        Target::Uri(t)
                    }
                    _ => Target::Uri(prima),
                };
                let vecchio = self.link.replace(target);
                if let Some(r) = ris {
                    let mut s2 = st.clone();
                    self.gruppo(r, &mut s2);
                }
                self.svuota();
                self.link = vecchio;
            }
            _ => {
                if let Some(r) = ris {
                    let mut s2 = st.clone();
                    self.gruppo(r, &mut s2);
                }
            }
        }
    }

    fn nota(&mut self, g: &[Nodo], st: &Stato) {
        let fine = ha_parola(g, "ftnalt");
        let n = if fine { self.doc.note_fine.len() + 1 } else { self.doc.note.len() + 1 };
        let numero = if fine { super::docx::number_text(n as u32, "lowerRoman") } else { n.to_string() };
        // Il richiamo nel testo, in apice.
        let mut c = st.car.clone();
        c.vert = 1;
        c.hidden = false;
        let font = self.font_di(&c);
        self.svuota();
        let pezzo = if fine { Pezzo::NotaFine(n - 1) } else { Pezzo::Nota(n - 1) };
        // Prima si riserva il posto (le note dentro la nota hanno numeri dopo).
        if fine {
            self.doc.note_fine.push((numero.clone(), Vec::new()));
        } else {
            self.doc.note.push((numero.clone(), Vec::new()));
        }
        self.ctx.runs.push(Run { pezzo, font, link: None });
        let saved_note = (self.nota_corrente.take(), ());
        self.nota_corrente = Some(numero.clone());
        let blocchi = self.a_parte(g, st);
        self.nota_corrente = saved_note.0;
        if fine {
            self.doc.note_fine[n - 1].1 = blocchi;
        } else {
            self.doc.note[n - 1].1 = blocchi;
        }
    }

    // ---------------------------------------------------------------- parole di controllo

    fn car_parola(&self, c: &mut Car, p: &str, v: Option<i32>) -> bool {
        let on = v.is_none_or(|x| x != 0);
        match p {
            "plain" => *c = car_default(self.deff),
            "f" => c.font = v.unwrap_or(self.deff),
            "fs" => c.hp = v.unwrap_or(24),
            "b" => c.bold = on,
            "i" => c.italic = on,
            "ul" | "ulw" | "uld" | "uldash" | "uldashd" | "uldashdd" | "ulth" | "ulwave" | "ulhwave" | "ulldash" | "ulthd" | "ulthdash" => c.ul = if on { 1 } else { 0 },
            "uldb" | "ululdbwave" => c.ul = if on { 2 } else { 0 },
            "ulnone" => c.ul = 0,
            "strike" | "striked" => c.strike = on,
            "cf" => c.color = v.unwrap_or(0),
            "highlight" | "chcbpat" | "cb" => c.highlight = v.unwrap_or(0),
            "super" => c.vert = 1,
            "sub" => c.vert = -1,
            "nosupersub" => c.vert = 0,
            "up" => c.vert = if on { 1 } else { 0 },
            "dn" => c.vert = if on { -1 } else { 0 },
            "caps" => c.caps = on,
            "scaps" => c.scaps = on,
            "v" => c.hidden = on,
            "webhidden" => c.hidden = on,
            "expnd" => c.spacing = v.unwrap_or(0) as f32 / 4.0,
            "expndtw" => c.spacing = tw(v.unwrap_or(0)),
            _ => return false,
        }
        true
    }

    /// Cambia il bordo in definizione (\box: tutti e quattro i lati del paragrafo).
    fn bordo(&mut self, st: &mut Stato, f: impl Fn(&mut Option<Linea>)) {
        let Some(lato) = st.lato else { return };
        let lv = if st.nest_props { Self::livello_par(&st.par).max(2) } else { 1 };
        match lato {
            Lato::Par(9) => (0..4).for_each(|i| f(&mut st.par.bordi[i])),
            Lato::Par(i) => f(&mut st.par.bordi[i]),
            Lato::Cella(i) => f(&mut self.cella_in_def(lv).bordi[i]),
            Lato::Riga(i) => f(&mut self.def_corrente(lv).bordi[i]),
        }
    }

    fn def_corrente(&mut self, livello: usize) -> &mut RigaDef {
        let livello = livello.max(1);
        while self.ctx.livelli.len() < livello {
            self.ctx.livelli.push(Livello::default());
        }
        // La cella in definizione è l'ultima con il bordo destro non ancora scritto.
        &mut self.ctx.livelli[livello - 1].def
    }

    fn cella_in_def(&mut self, livello: usize) -> &mut CellaDef {
        let d = self.def_corrente(livello);
        if d.celle.last().is_none_or(|c| c.destra != i32::MIN) {
            d.celle.push(CellaDef { destra: i32::MIN, ..Default::default() });
        }
        d.celle.last_mut().unwrap()
    }

    fn parola(&mut self, p: &str, v: Option<i32>, st: &mut Stato) {
        if self.salta > 0 {
            self.salta -= 1;
            return;
        }
        if p != "u" {
            self.svuota();
        }
        match p {
            "plain" => {
                st.car = car_default(self.deff);
                for (w, x) in self.defchp.clone() {
                    self.car_parola(&mut st.car, &w, x);
                }
                return;
            }
            "s" => {
                st.par.stile = v.unwrap_or(0);
                self.applica_stile(false, v.unwrap_or(0), st, 0);
                return;
            }
            "cs" => {
                self.applica_stile(true, v.unwrap_or(0), st, 0);
                return;
            }
            _ => {}
        }
        if self.car_parola(&mut st.car, p, v) {
            return;
        }
        let on = v.is_none_or(|x| x != 0);
        let n = v.unwrap_or(0);
        let lv_def = if st.nest_props { Self::livello_par(&st.par).max(2) } else { 1 };
        match p {
            // testo
            "par" => self.fine_par(st),
            "line" => self.pezzo(Pezzo::ACapo, &st.car.clone()),
            "tab" => self.pezzo(Pezzo::Tab, &st.car.clone()),
            "page" => self.pezzo(Pezzo::SaltoPagina, &st.car.clone()),
            "column" => self.pezzo(Pezzo::SaltoColonna, &st.car.clone()),
            "chpgn" => self.pezzo(Pezzo::NumPagina, &st.car.clone()),
            "chftn" => {
                if let Some(num) = self.nota_corrente.clone() {
                    let mut c = st.car.clone();
                    c.vert = 1;
                    self.scrivi(&num, &c);
                }
            }
            "u" => self.unicode(n, st),
            "uc" => st.uc = n.clamp(0, 10) as usize,
            "emdash" => self.scrivi("—", &st.car.clone()),
            "endash" => self.scrivi("–", &st.car.clone()),
            "emspace" => self.scrivi("\u{2003}", &st.car.clone()),
            "enspace" => self.scrivi("\u{2002}", &st.car.clone()),
            "qmspace" => self.scrivi("\u{2005}", &st.car.clone()),
            "bullet" => self.scrivi("•", &st.car.clone()),
            "lquote" => self.scrivi("‘", &st.car.clone()),
            "rquote" => self.scrivi("’", &st.car.clone()),
            "ldblquote" => self.scrivi("“", &st.car.clone()),
            "rdblquote" => self.scrivi("”", &st.car.clone()),
            // paragrafo
            "pard" => {
                st.par = par_default();
                st.lato = None;
                for (w, x) in self.defpap.clone() {
                    self.parola(&w, x, st);
                }
            }
            "ql" => st.par.allinea = Allinea::Sinistra,
            "qc" => st.par.allinea = Allinea::Centro,
            "qr" => st.par.allinea = Allinea::Destra,
            "qj" | "qd" => st.par.allinea = Allinea::Giustifica,
            "li" | "lin" => st.par.li = n,
            "ri" | "rin" => st.par.ri = n,
            "fi" => st.par.fi = n,
            "sb" => st.par.sb = n,
            "sa" => st.par.sa = n,
            "sbauto" => st.par.sb_auto = on,
            "saauto" => st.par.sa_auto = on,
            "sl" => st.par.sl = n,
            "slmult" => st.par.slmult = on,
            "keepn" => st.par.keepn = on,
            "keep" => st.par.keep = on,
            "pagebb" => st.par.pagebb = on,
            "widctlpar" => st.par.widctl = true,
            "nowidctlpar" => st.par.widctl = false,
            "s" => st.par.stile = n,
            "outlinelevel" => st.par.outline = u8::try_from(n).ok().filter(|l| *l < 9),
            "intbl" => st.par.intbl = true,
            "itap" => st.par.itap = n.clamp(0, 32),
            "contextualspace" => st.par.contestuale = true,
            "cbpat" if st.lato.is_none() || matches!(st.lato, Some(Lato::Par(_))) => st.par.sfondo = (n > 0).then_some(n),
            "tqr" => st.par.tab_tipo = Some(TipoTab::Destra),
            "tqc" => st.par.tab_tipo = Some(TipoTab::Centro),
            "tqdec" => st.par.tab_tipo = Some(TipoTab::Decimale),
            "tldot" => st.par.tab_leader = Some('.'),
            "tlhyph" => st.par.tab_leader = Some('-'),
            "tlul" | "tlth" => st.par.tab_leader = Some('_'),
            "tleq" => st.par.tab_leader = Some('='),
            "tx" => {
                let tipo = st.par.tab_tipo.take().unwrap_or(TipoTab::Sinistra);
                let leader = st.par.tab_leader.take();
                // Lo stile e il paragrafo possono ripetere la stessa tabulazione.
                st.par.tabs.retain(|t| (t.pos - tw(n)).abs() > 0.5);
                st.par.tabs.push(Tab { pos: tw(n), tipo, leader });
                st.par.tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            }
            "tb" => {
                st.par.tab_tipo = None;
                st.par.tab_leader = None;
            }
            // bordi
            "brdrt" => st.lato = Some(Lato::Par(0)),
            "brdrb" => st.lato = Some(Lato::Par(1)),
            "brdrl" => st.lato = Some(Lato::Par(2)),
            "brdrr" => st.lato = Some(Lato::Par(3)),
            "brdrbtw" => st.lato = Some(Lato::Par(4)),
            "box" => st.lato = Some(Lato::Par(9)),
            "clbrdrt" => st.lato = Some(Lato::Cella(0)),
            "clbrdrb" => st.lato = Some(Lato::Cella(1)),
            "clbrdrl" => st.lato = Some(Lato::Cella(2)),
            "clbrdrr" => st.lato = Some(Lato::Cella(3)),
            "trbrdrt" => st.lato = Some(Lato::Riga(0)),
            "trbrdrb" => st.lato = Some(Lato::Riga(1)),
            "trbrdrl" => st.lato = Some(Lato::Riga(2)),
            "trbrdrr" => st.lato = Some(Lato::Riga(3)),
            "trbrdrh" => st.lato = Some(Lato::Riga(4)),
            "trbrdrv" => st.lato = Some(Lato::Riga(5)),
            "brdrs" | "brdrth" | "brdrsh" | "brdrdb" | "brdrdot" | "brdrdash" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" | "brdrtriple" | "brdrthtnsg" | "brdrtnthsg" | "brdrtnthtnsg" | "brdrthtnmg" | "brdrtnthmg" | "brdrwavy" | "brdrinset" | "brdroutset" | "brdrengrave" | "brdremboss" | "brdrhair" => {
                let p = p.to_string();
                self.bordo(st, |slot| {
                    let mut l = slot.unwrap_or(Linea { width: 0.5, color: (0, 0, 0), dash: Dash::Solid, double: false, space: 0.0 });
                    l.double = p == "brdrdb" || p == "brdrtriple";
                    l.dash = match p.as_str() {
                        "brdrdot" => Dash::Dotted,
                        "brdrdash" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" => Dash::Dashed,
                        _ => Dash::Solid,
                    };
                    if p == "brdrhair" {
                        l.width = 0.25;
                    }
                    *slot = Some(l);
                });
            }
            "brdrnone" | "brdrnil" | "brdrtbl" => self.bordo(st, |slot| *slot = None),
            "brdrw" => self.bordo(st, |slot| {
                if let Some(l) = slot {
                    l.width = tw(n).max(0.25);
                }
            }),
            "brdrcf" => {
                let c = self.colore(n).unwrap_or((0, 0, 0));
                self.bordo(st, |slot| {
                    if let Some(l) = slot {
                        l.color = c;
                    }
                });
            }
            "brsp" => self.bordo(st, |slot| {
                if let Some(l) = slot {
                    l.space = tw(n);
                }
            }),
            // righe e celle
            "trowd" => {
                *self.def_corrente(lv_def) = RigaDef { gaph: 108, ..Default::default() };
                st.lato = None;
            }
            "trgaph" => self.def_corrente(lv_def).gaph = n,
            "trleft" => self.def_corrente(lv_def).sinistra = n,
            "trrh" => self.def_corrente(lv_def).altezza = n,
            "trhdr" => self.def_corrente(lv_def).intestazione = true,
            "trkeep" => self.def_corrente(lv_def).non_dividere = true,
            "trqc" => self.def_corrente(lv_def).allinea = 1,
            "trqr" => self.def_corrente(lv_def).allinea = 2,
            "trql" => self.def_corrente(lv_def).allinea = 0,
            "trpaddl" => self.def_corrente(lv_def).pad[0] = Some(tw(n)),
            "trpaddr" => self.def_corrente(lv_def).pad[1] = Some(tw(n)),
            "trpaddt" => self.def_corrente(lv_def).pad[2] = Some(tw(n)),
            "trpaddb" => self.def_corrente(lv_def).pad[3] = Some(tw(n)),
            // Word scambia sinistra e sopra nei margini delle celle (\clpadl è quello in alto).
            "clpadl" => self.cella_in_def(lv_def).pad[2] = Some(tw(n)),
            "clpadt" => self.cella_in_def(lv_def).pad[0] = Some(tw(n)),
            "clpadr" => self.cella_in_def(lv_def).pad[1] = Some(tw(n)),
            "clpadb" => self.cella_in_def(lv_def).pad[3] = Some(tw(n)),
            "clvmgf" => self.cella_in_def(lv_def).vmerge = Some(true),
            "clvmrg" => self.cella_in_def(lv_def).vmerge = Some(false),
            "clmgf" => self.cella_in_def(lv_def).hmerge = Some(true),
            "clmrg" => self.cella_in_def(lv_def).hmerge = Some(false),
            "clvertalt" => self.cella_in_def(lv_def).valign = 0,
            "clvertalc" => self.cella_in_def(lv_def).valign = 1,
            "clvertalb" => self.cella_in_def(lv_def).valign = 2,
            "clcbpat" => self.cella_in_def(lv_def).sfondo = (n > 0).then_some(n),
            "cellx" => {
                // Le proprietà dei bordi (\clbrdr…) cadono sulla cella in definizione.
                let c = self.cella_in_def(lv_def);
                c.destra = n;
                st.lato = None;
            }
            "cell" => self.fine_cella(st, 1),
            "nestcell" => self.fine_cella(st, Self::livello_par(&st.par).max(2)),
            "row" => self.fine_riga(1),
            "nestrow" => self.fine_riga(Self::livello_par(&st.par).max(2)),
            // sezioni e pagina
            "paperw" => self.sez_doc.w = tw(n),
            "paperh" => self.sez_doc.h = tw(n),
            "margl" => self.sez_doc.margini[0] = tw(n),
            "margr" => self.sez_doc.margini[1] = tw(n),
            "margt" => self.sez_doc.margini[2] = tw(n),
            "margb" => self.sez_doc.margini[3] = tw(n),
            "landscape" => self.sez_doc.orizzontale = true,
            "facingp" => self.doc.pari_dispari = true,
            "deftab" => self.doc.tab_predefinita = tw(n).max(1.0),
            "ansicpg" => self.ansicpg = n.max(0) as u32,
            "deff" => self.deff = n,
            "pgnstart" => self.sez_doc.primo = Some(n.max(0) as u32),
            "sectd" => {
                let testa = std::mem::take(&mut self.sez.testa);
                let piede = std::mem::take(&mut self.sez.piede);
                self.sez = SezP { testa, piede, ..self.sez_doc.clone() };
                // Le misure del documento valgono finché la sezione non dice altro.
                self.sez.margini = self.sez_doc.margini;
            }
            "sect" => {
                if !self.ctx.runs.is_empty() {
                    self.fine_par(st);
                }
                self.fine_sezione(false);
            }
            "pgwsxn" => self.sez.w = tw(n),
            "pghsxn" => self.sez.h = tw(n),
            "marglsxn" => self.sez.margini[0] = tw(n),
            "margrsxn" => self.sez.margini[1] = tw(n),
            "margtsxn" => self.sez.margini[2] = tw(n),
            "margbsxn" => self.sez.margini[3] = tw(n),
            "lndscpsxn" => self.sez.orizzontale = true,
            "headery" => self.sez.testa_y = tw(n),
            "footery" => self.sez.piede_y = tw(n),
            "cols" => self.sez.colonne = n.clamp(1, 20) as u8,
            "colsx" => self.sez.spazio = tw(n),
            "titlepg" => self.sez.prima_diversa = true,
            "sbknone" => self.sez.salto = 0,
            "sbkpage" => self.sez.salto = 1,
            "sbkodd" => self.sez.salto = 2,
            "sbkeven" => self.sez.salto = 3,
            "sbkcol" => self.sez.salto = 0,
            "pgnrestart" => self.sez.riparte = true,
            "pgnstarts" => self.sez.primo = Some(n.max(0) as u32),
            _ => {}
        }
        // Le misure di pagina scritte prima della prima sezione valgono anche per lei.
        if self.sezioni.is_empty() && matches!(p, "paperw" | "paperh" | "margl" | "margr" | "margt" | "margb" | "landscape" | "pgnstart") {
            match p {
                "paperw" => self.sez.w = tw(n),
                "paperh" => self.sez.h = tw(n),
                "margl" => self.sez.margini[0] = tw(n),
                "margr" => self.sez.margini[1] = tw(n),
                "margt" => self.sez.margini[2] = tw(n),
                "margb" => self.sez.margini[3] = tw(n),
                "landscape" => self.sez.orizzontale = true,
                _ => self.sez.primo = Some(n.max(0) as u32),
            }
        }
    }

    /// Le proprietà di uno stile (prima quelle dello stile su cui si basa).
    fn applica_stile(&mut self, car: bool, id: i32, st: &mut Stato, prof: usize) {
        let Some((base, parole)) = self.parole_stili.get(&(car, id)).cloned() else { return };
        if let Some(b) = base.filter(|b| *b != id && prof < 8) {
            self.applica_stile(car, b, st, prof + 1);
        }
        for (w, x) in parole {
            if car {
                self.car_parola(&mut st.car, &w, x);
            } else if !matches!(w.as_str(), "plain" | "pard" | "s" | "cs") {
                self.parola(&w, x, st);
            }
        }
    }

    // ---------------------------------------------------------------- gruppi

    fn gruppo(&mut self, g: &[Nodo], st: &mut Stato) {
        self.profondita += 1;
        if self.profondita > 300 {
            self.profondita -= 1;
            return;
        }
        for n in g {
            match n {
                Nodo::Gruppo(sub) => {
                    self.svuota();
                    // Un \u seguito dal suo sostituto in un gruppo: il gruppo non conta come sostituto.
                    self.salta = 0;
                    self.sottogruppo(sub, st);
                }
                Nodo::Parola(p, v) => self.parola(p, *v, st),
                Nodo::Simbolo(c) => {
                    if self.salta > 0 {
                        self.salta -= 1;
                        continue;
                    }
                    let car = st.car.clone();
                    match c {
                        '\\' | '{' | '}' => {
                            self.svuota();
                            self.scrivi(&c.to_string(), &car);
                        }
                        '~' => {
                            self.svuota();
                            self.scrivi("\u{a0}", &car);
                        }
                        '-' => {
                            self.svuota();
                            self.scrivi("\u{ad}", &car);
                        }
                        '_' => {
                            self.svuota();
                            self.scrivi("\u{2011}", &car);
                        }
                        '\n' => self.fine_par(st),
                        '\t' => self.pezzo(Pezzo::Tab, &car),
                        _ => {}
                    }
                }
                Nodo::Testo(t) => {
                    let car = st.car.clone();
                    for b in t {
                        self.byte(*b, &car);
                    }
                }
                Nodo::Hex(h) => {
                    let car = st.car.clone();
                    self.byte(*h, &car);
                }
                Nodo::Bin(_) => {}
            }
        }
        self.svuota();
        self.profondita -= 1;
    }

    fn sottogruppo(&mut self, sub: &[Nodo], st: &Stato) {
        let mut s2 = st.clone();
        let Some((p, stella)) = parola0(sub) else {
            self.gruppo(sub, &mut s2);
            return;
        };
        match p {
            "fonttbl" => self.tabella_font(sub),
            "colortbl" => self.tabella_colori(sub),
            "stylesheet" => self.tabella_stili(sub),
            "defchp" | "defpap" => {
                let parole: Vec<(String, Option<i32>)> = sub.iter().filter_map(|n| if let Nodo::Parola(w, x) = n { (w != "defchp" && w != "defpap").then(|| (w.clone(), *x)) } else { None }).collect();
                if p == "defchp" {
                    self.defchp = parole;
                } else {
                    self.defpap = parole;
                }
            }
            "info" => {
                if let Some(t) = sotto(sub, "title") {
                    let mut s = String::new();
                    testo_di(t, &mut s);
                    self.doc.titolo = s.trim().to_string();
                }
            }
            "header" | "headerr" | "headerl" | "headerf" | "footer" | "footerr" | "footerl" | "footerf" => {
                let blocchi = self.a_parte(sub, st);
                let hf = if p.starts_with("header") { &mut self.sez.testa } else { &mut self.sez.piede };
                match p.trim_start_matches("header").trim_start_matches("footer") {
                    "l" => hf.pari = Some(blocchi),
                    "f" => hf.prima = Some(blocchi),
                    "r" => hf.normale = Some(blocchi),
                    _ => {
                        if self.doc.pari_dispari {
                            hf.pari = Some(blocchi.clone());
                        }
                        hf.normale = Some(blocchi);
                    }
                }
            }
            "footnote" => self.nota(sub, st),
            "field" => self.campo(sub, st),
            "pict" => {
                if let Some((img, w, h)) = self.immagine(sub) {
                    self.pezzo(Pezzo::Immagine { img, w, h }, &st.car.clone());
                }
            }
            "shppict" => {
                if let Some(pict) = sotto(sub, "pict") {
                    if let Some((img, w, h)) = self.immagine(pict) {
                        self.pezzo(Pezzo::Immagine { img, w, h }, &st.car.clone());
                    }
                }
            }
            "shp" => self.forma(sub, st),
            "listtext" | "pntext" => {
                // L'etichetta dell'elenco già scritta dal programma che ha salvato il file.
                self.svuota();
                let runs = std::mem::take(&mut self.ctx.runs);
                let link = self.link.take();
                self.gruppo(sub, &mut s2);
                self.svuota();
                let fatti = std::mem::replace(&mut self.ctx.runs, runs);
                self.link = link;
                let mut testo = String::new();
                let mut tab = false;
                let mut font = None;
                for r in &fatti {
                    match &r.pezzo {
                        Pezzo::Testo(t) => {
                            testo.push_str(t);
                            font.get_or_insert_with(|| r.font.clone());
                        }
                        Pezzo::Tab => tab = true,
                        _ => {}
                    }
                }
                let testo: String = testo.chars().map(super::docx::bullet_char).collect::<String>().trim_end().to_string();
                if !testo.is_empty() || tab {
                    // Un'etichetta senza carattere suo (\pard\plain e basta) prende quello del paragrafo.
                    let sua = sub.iter().any(|n| matches!(n, Nodo::Parola(w, _) if matches!(w.as_str(), "f" | "fs" | "b" | "i" | "cf" | "ul" | "caps" | "cs")));
                    self.ctx.etichetta = Some((testo, font.filter(|_| sua), tab));
                }
            }
            "bkmkstart" => {
                let mut s = String::new();
                testo_di(sub, &mut s);
                let s = s.trim().to_string();
                if !s.is_empty() {
                    self.ctx.segnalibri.push(s);
                }
            }
            "nesttableprops" => {
                s2.nest_props = true;
                self.gruppo(sub, &mut s2);
            }
            "object" => {
                if let Some(r) = sotto(sub, "result") {
                    self.gruppo(r, &mut s2);
                }
            }
            "upr" => {
                // Due versioni dello stesso testo: si prende quella Unicode (\*\ud).
                if let Some(ud) = sub.iter().find_map(|n| if let Nodo::Gruppo(x) = n { (parola0(x).is_some_and(|(p, _)| p == "ud")).then_some(x.as_slice()) } else { None }) {
                    self.gruppo(ud, &mut s2);
                }
            }
            "ud" => self.gruppo(sub, &mut s2),
            // Gruppi da non scrivere.
            "nonshppict" | "fldinst" | "themedata" | "colorschememapping" | "latentstyles" | "datastore" | "xmlnstbl" | "rsidtbl" | "generator" | "listtable" | "listoverridetable" | "pgdsctbl" | "mmathPr" | "bkmkend" | "fldtype" | "annotation" | "atnid" | "atnauthor" | "atrfstart" | "atrfend" | "xe" | "tc" | "pn" | "pnseclvl" | "revtbl" | "userprops" | "docvar" | "fchars" | "lchars" | "wgrffmtfilter" | "ftnsep" | "ftnsepc" | "aftnsep" | "aftnsepc" | "template" | "formfield" | "datafield" | "objdata" | "panose" | "falt" | "passwordhash" | "protusertbl" | "filetbl" | "background" | "shpinst" | "shprslt" | "pgptbl" | "oldcprops" | "oldpprops" | "oldtprops" | "oldsprops" => {}
            _ if stella => {}
            _ => self.gruppo(sub, &mut s2),
        }
    }
}

pub fn leggi_rtf(bytes: &[u8]) -> Result<Documento, String> {
    if !bytes.starts_with(b"{\\rtf") {
        let t = bytes.iter().position(|b| !b.is_ascii_whitespace()).map_or(&bytes[..0], |i| &bytes[i..]);
        if !t.starts_with(b"{\\rtf") {
            return Err("il file non è un RTF".into());
        }
    }
    let radice = albero(bytes);
    let doc_g: &[Nodo] = match radice.iter().find(|n| matches!(n, Nodo::Gruppo(_))) {
        Some(Nodo::Gruppo(g)) => g,
        _ => return Err("RTF vuoto".into()),
    };
    let mut l = Lettore::nuovo();
    let mut st = Stato { car: car_default(0), par: par_default(), uc: 1, lato: None, nest_props: false };
    // \deff e la tavola dei caratteri prima di tutto: servono al carattere predefinito.
    for n in doc_g.iter().take(64) {
        match n {
            Nodo::Parola(p, Some(v)) if p == "deff" => l.deff = *v,
            Nodo::Parola(p, Some(v)) if p == "ansicpg" => l.ansicpg = (*v).max(0) as u32,
            _ => {}
        }
    }
    st.car = car_default(l.deff);
    l.gruppo(doc_g, &mut st);
    l.svuota();
    if !l.ctx.runs.is_empty() || l.ctx.etichetta.is_some() {
        l.fine_par(&st);
    }
    l.fine_sezione(true);
    let mut doc = std::mem::take(&mut l.doc);
    doc.sezioni = std::mem::take(&mut l.sezioni);
    if doc.sezioni.is_empty() {
        doc.sezioni.push(Sezione::default());
    }
    Ok(doc)
}

#[cfg(test)]
mod test {
    use super::*;

    fn testo(doc: &Documento) -> String {
        let mut s = String::new();
        for sez in &doc.sezioni {
            for b in &sez.blocchi {
                match b {
                    Blocco::Par(p) => {
                        s.push_str(&p.testo());
                        s.push('\n');
                    }
                    Blocco::Tab(t) => {
                        for r in &t.righe {
                            for c in &r.celle {
                                for b in &c.blocchi {
                                    if let Blocco::Par(p) = b {
                                        s.push_str(&p.testo());
                                    }
                                }
                                s.push('|');
                            }
                            s.push('\n');
                        }
                    }
                    _ => {}
                }
            }
        }
        s
    }

    #[test]
    fn paragrafi_caratteri_e_tabelle() {
        let rtf = br"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\froman Times New Roman;}{\f1\fswiss\fcharset204 Arial;}}{\colortbl;\red255\green0\blue0;}
\paperw11906\paperh16838\margl1134\margr1134
\pard\qc\b Titolo\b0\par
\pard\li720\fi-360 {\listtext 1.\tab}Primo {\cf1 rosso} \'e8 qui\par
{\f1 \'cf\'f0\'e8\'e2\'e5\'f2}\par
caff\u233?\par
\trowd\trgaph108\trleft-108\cellx2000\cellx4000\pard\intbl A\cell B\cell\row
\pard Dopo{\footnote\pard\plain {\super\chftn} Nota.}\par}";
        let doc = leggi_rtf(rtf).unwrap();
        let t = testo(&doc);
        assert!(t.contains("Titolo\n"), "{t}");
        assert!(t.contains("Primo rosso è qui"), "{t}");
        assert!(t.contains("Привет"), "{t}");
        assert!(t.contains("caffé"), "{t}");
        assert!(t.contains("A|B|"), "{t}");
        let sez = &doc.sezioni[0];
        assert!((sez.w - 595.3).abs() < 0.5);
        let Blocco::Par(p) = &sez.blocchi[1] else { panic!() };
        assert_eq!(p.etichetta.as_ref().map(|e| e.0.as_str()), Some("1."));
        assert!(p.etichetta_tab);
        assert!(p.runs.iter().any(|r| r.font.color == (255, 0, 0)));
        let Blocco::Par(p0) = &sez.blocchi[0] else { panic!() };
        assert_eq!(p0.allinea, Allinea::Centro);
        assert!(p0.runs[0].font.bold);
        assert_eq!(doc.note.len(), 1);
        let Blocco::Par(n) = &doc.note[0].1[0] else { panic!() };
        assert!(n.testo().contains("1 Nota."), "{}", n.testo());
    }
}
