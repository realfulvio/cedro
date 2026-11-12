//! Lettore dei file ND2 di Nikon (NIS-Elements, versione 3: quelli di NIS-Elements 4 e successivi).
//!
//! Il file è una serie di blocchi con l'intestazione `DA CE BE 0A, lunghezza del nome, lunghezza dei
//! dati`; in fondo c'è la mappa dei blocchi (nome, posizione, grandezza) e 40 byte che dicono dov'è.
//! I metadati sono nel formato «lite variant» di Nikon: alberi di numeri, testi (UTF-16) e livelli.
//! Ogni fotogramma (`ImageDataSeq|N!`) tiene tutti i canali, un pixel dopo l'altro; i fotogrammi si
//! ordinano per cicli (tempo, posizioni, piani Z) nell'ordine dichiarato dall'esperimento.
//! Il formato lo descrivono il codice di Nikon (`nd2readsdk`) e le librerie libere `nd2` (Python) e
//! Bio-Formats; qui è scritto da capo, con i file di prova confrontati con `nd2`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::microscopia::{canali_standard, Canale, NomeColore, Piano};

const MAGIC: [u8; 4] = [0xDA, 0xCE, 0xBE, 0x0A];
const FIRMA_MAPPA: &[u8] = b"ND2 CHUNK MAP SIGNATURE 0000001!";
const FIRMA_ELENCO: &[u8] = b"ND2 FILEMAP SIGNATURE NAME 0001!";
const FIRMA_FILE: &[u8] = b"ND2 FILE SIGNATURE CHUNK NAME01!";
/// Il messaggio per gli ND2 dei primi NIS-Elements (immagini in JPEG 2000).
pub const VECCHIO: &str = "ND2 di vecchio tipo (NIS-Elements 2, immagini in JPEG 2000): Cedro non lo legge ancora; da NIS-Elements si può esportare in TIFF";
/// I fotogrammi non devono superare questa grandezza: un file rovinato non deve riempire la memoria.
const MAX_FOTOGRAMMA: u64 = 1 << 32;

/// Vero se i primi byte sono quelli di un ND2.
pub fn riconosci(head: &[u8]) -> bool {
    head.starts_with(&MAGIC)
}

// ------------------------------------------------------------------ Lite variant

/// Un valore dei metadati: un numero, un testo, dei byte o un livello con i suoi elementi per nome.
#[derive(Clone, Debug)]
pub enum Lv {
    Num(f64),
    Testo(String),
    Byte(Vec<u8>),
    Livello(Vec<(String, Lv)>),
}

impl Lv {
    /// L'elemento con quel nome (nei livelli).
    pub fn get(&self, nome: &str) -> Option<&Lv> {
        match self {
            Lv::Livello(v) => v.iter().find(|(n, _)| n == nome).map(|(_, x)| x),
            _ => None,
        }
    }

    /// Un percorso di nomi: `x.get_in(&["sPicturePlanes", "uiCount"])`.
    pub fn percorso(&self, nomi: &[&str]) -> Option<&Lv> {
        nomi.iter().try_fold(self, |lv, n| lv.get(n))
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Lv::Num(v) => Some(*v),
            _ => None,
        }
    }

    pub fn testo(&self) -> Option<&str> {
        match self {
            Lv::Testo(s) => Some(s),
            _ => None,
        }
    }

    /// Gli elementi di un livello (in ordine, qualunque sia il nome).
    pub fn figli(&self) -> &[(String, Lv)] {
        match self {
            Lv::Livello(v) => v,
            _ => &[],
        }
    }
}

struct Lettore<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Lettore<'a> {
    fn prendi(&mut self, n: usize) -> Result<&'a [u8], String> {
        let fine = self.p.checked_add(n).filter(|f| *f <= self.d.len()).ok_or("metadati troncati")?;
        let s = &self.d[self.p..fine];
        self.p = fine;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.prendi(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.prendi(4)?.try_into().map_err(|_| "metadati troncati")?))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.prendi(8)?.try_into().map_err(|_| "metadati troncati")?))
    }
}

fn utf16(b: &[u8]) -> String {
    let unita: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&unita)
}

/// Il testo UTF-16 che finisce al primo zero (per i valori di tipo stringa).
fn testo_nul(l: &mut Lettore) -> Result<String, String> {
    let mut v: Vec<u16> = Vec::new();
    loop {
        let c = l.prendi(2);
        let Ok(c) = c else { break };
        let u = u16::from_le_bytes([c[0], c[1]]);
        if u == 0 {
            break;
        }
        v.push(u);
    }
    Ok(String::from_utf16_lossy(&v))
}

fn elemento(l: &mut Lettore, profondita: u32) -> Result<Option<(String, Lv)>, String> {
    if profondita > 48 {
        return Err("metadati troppo annidati".into());
    }
    let start = l.p;
    if l.p >= l.d.len() {
        return Ok(None);
    }
    let tipo = l.u8()?;
    let lung_nome = l.u8()? as usize;
    // Un blocco compresso: 10 byte di intestazione, poi un flusso zlib con altri metadati dentro.
    if tipo == 76 {
        l.prendi(10)?;
        let dati = l.d.get(l.p..).ok_or("metadati troncati")?;
        let mut fuori = Vec::new();
        flate2::read::ZlibDecoder::new(dati).take(1 << 28).read_to_end(&mut fuori).map_err(|e| format!("metadati compressi non letti: {e}"))?;
        let mut sub = Lettore { d: &fuori, p: 0 };
        return elemento(&mut sub, profondita + 1);
    }
    let nome = utf16(l.prendi(lung_nome * 2)?);
    let nome = nome.trim_end_matches('\0').to_string();
    let valore = match tipo {
        1 => Lv::Num(f64::from(l.u8()?)),
        2 => Lv::Num(f64::from(l.u32()? as i32)),
        3 => Lv::Num(f64::from(l.u32()?)),
        4 => Lv::Num(l.u64()? as i64 as f64),
        5 | 7 => Lv::Num(l.u64()? as f64),
        6 => Lv::Num(f64::from_le_bytes(l.prendi(8)?.try_into().map_err(|_| "metadati troncati")?)),
        8 => Lv::Testo(testo_nul(l)?),
        9 => {
            let n = usize::try_from(l.u64()?).map_err(|_| "metadati troppo grandi")?;
            Lv::Byte(l.prendi(n)?.to_vec())
        }
        11 => {
            let n = l.u32()? as usize;
            let lunghezza = usize::try_from(l.u64()?).map_err(|_| "metadati troppo grandi")?;
            if n > 4_000_000 {
                return Err("livello di metadati troppo grande".into());
            }
            let fine = start.checked_add(lunghezza).filter(|f| *f <= l.d.len() && *f >= l.p).ok_or("livello di metadati fuori misura")?;
            let mut sub = Lettore { d: &l.d[..fine], p: l.p };
            let mut figli = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                match elemento(&mut sub, profondita + 1)? {
                    Some(x) => figli.push(x),
                    None => break,
                }
            }
            // Dopo gli elementi c'è la tavola dei loro indirizzi: 8 byte per elemento.
            l.p = fine;
            l.prendi(n * 8)?;
            Lv::Livello(figli)
        }
        t => return Err(format!("tipo di metadato sconosciuto ({t})")),
    };
    Ok(Some((nome, valore)))
}

/// Decodifica un blocco di metadati (un solo elemento in testa, di solito un livello).
pub fn decodifica(dati: &[u8]) -> Result<Lv, String> {
    let mut l = Lettore { d: dati, p: 0 };
    let (_, v) = elemento(&mut l, 0)?.ok_or("metadati vuoti")?;
    // Il livello di testa porta il nome del tipo (SLxImageAttributes…): si guarda dentro.
    Ok(v)
}

/// I metadati dei file della versione 2 (NIS-Elements 3): lo stesso albero, scritto in XML
/// (`<nome runtype="tipo" value="…"/>`, i livelli con i figli, `no_name` per gli elementi di un elenco).
pub fn decodifica_xml(dati: &[u8]) -> Result<Lv, String> {
    let testo = if dati.starts_with(&[0xFF, 0xFE]) || (dati.len() > 1 && dati[1] == 0 && dati[0] == b'<') {
        utf16(dati.strip_prefix(&[0xFF, 0xFE]).unwrap_or(dati))
    } else {
        String::from_utf8_lossy(dati).into_owned()
    };
    let testo = testo.trim_end_matches('\0');
    let testo = match testo.find("?>") {
        Some(i) if testo.trim_start().starts_with("<?xml") => &testo[i + 2..],
        _ => testo,
    };
    let opzioni = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 4_000_000, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(testo, opzioni).map_err(|e| format!("metadati XML non letti: {e}"))?;
    fn nodo(n: roxmltree::Node, profondita: u32) -> Lv {
        let tipo = n.attribute("runtype").unwrap_or("");
        let valore = n.attribute("value").unwrap_or("");
        match tipo {
            "bool" => Lv::Num(if valore.eq_ignore_ascii_case("true") || valore == "1" { 1.0 } else { 0.0 }),
            "CLxByteArray" => Lv::Byte(valore.as_bytes().to_vec()),
            "CLxStringW" | "unknown" => Lv::Testo(valore.to_string()),
            "double" | "float" | "lx_int32" | "lx_int64" | "lx_uint32" | "lx_uint64" => Lv::Num(valore.trim().parse().unwrap_or(f64::NAN)),
            _ if profondita > 48 => Lv::Livello(Vec::new()),
            _ => Lv::Livello(
                n.children()
                    .filter(|c| c.is_element())
                    .map(|c| {
                        let nome = c.tag_name().name();
                        (if nome == "no_name" { String::new() } else { nome.to_string() }, nodo(c, profondita + 1))
                    })
                    .collect(),
            ),
        }
    }
    let radice = doc.root_element();
    let mut v = nodo(radice, 0);
    if radice.tag_name().name() != "variant" {
        return Ok(Lv::Livello(vec![(radice.tag_name().name().to_string(), v)]));
    }
    // `<variant><no_name …>…</no_name></variant>`: conta solo quello che c'è dentro.
    if let Lv::Livello(figli) = &mut v {
        if figli.len() == 1 && figli[0].0.is_empty() {
            return Ok(figli.remove(0).1);
        }
    }
    Ok(v)
}

/// Un blocco di metadati, in «lite variant» (versione 3) o in XML (versione 2).
fn metadati(dati: &[u8]) -> Result<Lv, String> {
    let inizio = dati.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    if dati[inizio..].starts_with(b"<") || dati.starts_with(&[0xFF, 0xFE, b'<']) {
        decodifica_xml(dati)
    } else {
        decodifica(dati)
    }
}

// ------------------------------------------------------------------ Il file

/// I cicli dell'esperimento, dal più esterno al più interno.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asse {
    Tempo,
    Posizione,
    Z,
    Altro,
}

#[derive(Debug)]
pub struct Nd2 {
    pub larghezza: u32,
    pub altezza: u32,
    larghezza_byte: usize,
    componenti: usize,
    byte_per_componente: usize,
    decimale: bool,
    compresso: bool,
    /// Dove comincia il blocco di ogni fotogramma.
    fotogrammi: HashMap<usize, u64>,
    pub cicli: Vec<(Asse, usize)>,
    pub canali: Vec<Canale>,
    pub bit: u8,
    pub um_per_px: Option<f64>,
    pub um_z: Option<f64>,
    pub obiettivo: Option<String>,
    /// 3 per le foto a colori (un RGB), 1 per i canali di fluorescenza.
    pub campioni: usize,
    /// L'acquisizione si è fermata a metà di un giro: gli ultimi piani mancano (e sono neri).
    pub interrotta: bool,
}

fn leggi_blocco(f: &mut File, pos: u64, nome_atteso: Option<&[u8]>) -> Result<Vec<u8>, String> {
    f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
    let mut h = [0u8; 16];
    f.read_exact(&mut h).map_err(|_| "file ND2 troncato")?;
    if h[..4] != MAGIC {
        return Err("blocco ND2 non valido".into());
    }
    let nome = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as u64;
    let dati = u64::from_le_bytes(h[8..16].try_into().map_err(|_| "file ND2 troncato")?);
    if dati > MAX_FOTOGRAMMA {
        return Err("blocco ND2 troppo grande".into());
    }
    // Le misure vengono dal file: prima di prendere la memoria si guarda che ci stiano davvero.
    let resto = f.metadata().map_err(|e| e.to_string())?.len().saturating_sub(pos + 16);
    if nome.saturating_add(dati) > resto {
        return Err("file ND2 troncato".into());
    }
    let mut n = vec![0u8; nome as usize];
    f.read_exact(&mut n).map_err(|_| "file ND2 troncato")?;
    if let Some(atteso) = nome_atteso {
        if !n.starts_with(atteso) {
            return Err("blocco ND2 inatteso".into());
        }
    }
    let mut out = vec![0u8; dati as usize];
    f.read_exact(&mut out).map_err(|_| "file ND2 troncato")?;
    Ok(out)
}

fn mappa_blocchi(f: &mut File) -> Result<HashMap<Vec<u8>, (u64, u64)>, String> {
    f.seek(SeekFrom::End(-40)).map_err(|_| "file ND2 troppo corto")?;
    let mut coda = [0u8; 40];
    f.read_exact(&mut coda).map_err(|_| "file ND2 troppo corto")?;
    if &coda[..32] != FIRMA_MAPPA {
        return Err("manca la mappa dei blocchi in fondo al file ND2 (file non finito?)".into());
    }
    let pos = u64::from_le_bytes(coda[32..40].try_into().map_err(|_| "file ND2 troppo corto")?);
    let dati = leggi_blocco(f, pos, Some(FIRMA_ELENCO))?;
    let mut out = HashMap::new();
    let mut p = 0usize;
    while p < dati.len() {
        let Some(fine) = dati[p..].iter().position(|b| *b == b'!') else { break };
        let nome = dati[p..p + fine + 1].to_vec();
        p += fine + 1;
        if nome == FIRMA_MAPPA {
            break;
        }
        let voce = dati.get(p..p + 16).ok_or("mappa dei blocchi troncata")?;
        let off = u64::from_le_bytes(voce[..8].try_into().map_err(|_| "mappa troncata")?);
        let dim = u64::from_le_bytes(voce[8..].try_into().map_err(|_| "mappa troncata")?);
        out.insert(nome, (off, dim));
        p += 16;
    }
    Ok(out)
}

fn colore_nikon(v: f64) -> Option<(u8, u8, u8)> {
    // Il colore è un intero 0x00BBGGRR; 0 e il bianco pieno vogliono dire «non scelto».
    let c = v as u32;
    (c != 0).then_some(((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8))
}

fn asse_di(tipo: u32) -> Asse {
    match tipo {
        1 | 8 | 9 => Asse::Tempo,
        2 | 3 => Asse::Posizione,
        4 | 10 => Asse::Z,
        _ => Asse::Altro,
    }
}

/// Un ciclo dell'esperimento: il tipo di Nikon, quante volte, il passo Z e la profondità.
struct Ciclo {
    tipo: u32,
    n: usize,
    passo_z: f64,
    livello: i32,
}

/// Gli elementi di un elenco: i figli di un livello, o i byte di un elenco di sì/no.
fn validi(v: Option<&Lv>) -> Option<Vec<bool>> {
    match v? {
        Lv::Byte(b) => Some(b.iter().map(|x| *x != 0).collect()),
        Lv::Livello(f) => Some(f.iter().map(|(_, x)| x.num().is_some_and(|n| n != 0.0)).collect()),
        Lv::Num(n) => Some(vec![*n != 0.0]),
        Lv::Testo(_) => None,
    }
}

/// I cicli dell'esperimento, dal più esterno al più interno, come li conta NIS-Elements: un ciclo
/// vuoto chiude il suo ramo, quello spettrale (i canali) non conta, e due cicli dello stesso tipo
/// allo stesso livello valgono per uno (il più lungo).
fn leggi_cicli(esp: &Lv, livello: i32, dest: &mut Vec<Ciclo>) {
    let tipo = esp.get("eType").and_then(Lv::num).unwrap_or(0.0) as u32;
    let Some(p) = esp.get("uLoopPars").filter(|p| !p.figli().is_empty()) else { return };
    if tipo > 10 {
        return;
    }
    // Nei file della versione 2 i parametri stanno in un elemento senza nome.
    let p = if p.figli().len() == 1 && p.figli()[0].0.is_empty() { &p.figli()[0].1 } else { p };
    let conta = |p: &Lv| p.get("uiCount").and_then(Lv::num).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(0.0) as usize;
    let n = match tipo {
        // Posizioni: quelle segnate come valide.
        2 => {
            let punti = match p.get("Points") {
                Some(l) => l.figli().len(),
                None => p.get("dPosX").map_or(0, |l| l.figli().len()),
            };
            match validi(esp.get("pItemValid")).filter(|v| !v.is_empty()) {
                Some(v) => v.iter().take(punti).filter(|b| **b).count(),
                None => punti,
            }
        }
        // Tempo a più fasi: la somma delle fasi valide.
        8 => {
            let ok = validi(p.get("pPeriodValid")).unwrap_or_default();
            p.get("pPeriod").map_or(0, |l| l.figli().iter().zip(ok).filter(|(_, v)| *v).map(|((_, x), _)| conta(x)).sum())
        }
        _ => conta(p),
    };
    if n == 0 {
        return;
    }
    let mut livello = livello;
    if tipo == 6 {
        livello -= 1;
    } else {
        let z = matches!(tipo, 4 | 10);
        let mut passo_z = if z { p.get("dZStep").and_then(Lv::num).unwrap_or(0.0) } else { 0.0 };
        if z && passo_z == 0.0 && n > 1 {
            let (lo, hi) = (p.get("dZLow").and_then(Lv::num).unwrap_or(0.0), p.get("dZHigh").and_then(Lv::num).unwrap_or(0.0));
            passo_z = (hi - lo).abs() / (n - 1) as f64;
        }
        let ciclo = Ciclo { tipo, n, passo_z: passo_z.abs(), livello };
        match dest.last_mut() {
            Some(u) if u.livello >= livello => {
                if u.livello == livello && u.tipo == tipo && u.n < n {
                    *u = ciclo;
                }
            }
            _ => dest.push(ciclo),
        }
    }
    if let Some(next) = esp.get("ppNextLevelEx") {
        for (_, sub) in next.figli() {
            leggi_cicli(sub, livello + 1, dest);
        }
    }
}

impl Nd2 {
    pub fn open(path: &Path) -> Result<Nd2, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let testa = leggi_blocco(&mut f, 0, Some(FIRMA_FILE))?;
        let versione = testa.get(..6).and_then(|v| std::str::from_utf8(v).ok()).unwrap_or("");
        let v3 = versione.starts_with("Ver3");
        if !v3 && !versione.starts_with("Ver2") {
            return Err(VECCHIO.into());
        }
        let lv = |nome: &str| if v3 { nome.replace('!', "LV!") } else { nome.to_string() };
        let mappa = mappa_blocchi(&mut f)?;
        let blocco = |f: &mut File, nome: &[u8]| -> Result<Vec<u8>, String> {
            let (off, _) = mappa.get(nome).ok_or_else(|| format!("nel file ND2 manca il blocco {}", String::from_utf8_lossy(nome)))?;
            leggi_blocco(f, *off, None)
        };
        let attr = metadati(&blocco(&mut f, lv("ImageAttributes!").as_bytes())?)?;
        let attr = attr.get("SLxImageAttributes").unwrap_or(&attr);
        let num = |k: &str| attr.get(k).and_then(Lv::num);
        let larghezza = num("uiWidth").ok_or("larghezza non trovata")? as u32;
        let altezza = num("uiHeight").ok_or("altezza non trovata")? as u32;
        let componenti = num("uiComp").unwrap_or(1.0) as usize;
        let bpc_bit = num("uiBpcInMemory").unwrap_or(16.0) as usize;
        let larghezza_byte = num("uiWidthBytes").unwrap_or((larghezza as f64) * (bpc_bit / 8) as f64 * componenti as f64) as usize;
        let sequenza = num("uiSequenceCount").unwrap_or(1.0) as usize;
        // 0 senza perdite (zlib), 1 con perdita (JPEG 2000), 2 nessuna.
        let compressione = num("eCompression").unwrap_or(2.0) as i32;
        if compressione == 1 {
            return Err("ND2 compresso con perdita (JPEG 2000): Cedro non lo legge ancora; da NIS-Elements si può esportare in TIFF".into());
        }
        if larghezza == 0 || altezza == 0 || componenti == 0 || ![8, 16, 32].contains(&bpc_bit) || (larghezza as u64) * (altezza as u64) * (componenti as u64) > (1 << 30) {
            return Err("misure del file ND2 non valide".into());
        }
        if larghezza_byte < larghezza as usize * (bpc_bit / 8) * componenti {
            return Err("righe del file ND2 più corte del previsto".into());
        }
        if (larghezza_byte as u64).saturating_mul(altezza as u64) > MAX_FOTOGRAMMA {
            return Err("misure del file ND2 non valide".into());
        }
        // I bit veri del sensore (12, 14) dentro i 16 della memoria: servono per il fondo scala.
        let significativi = num("uiBpcSignificant").map(|v| v as usize).filter(|v| *v > 8 && *v < bpc_bit && bpc_bit == 16);

        // Canali, colori, calibrazione, obiettivo.
        let meta = blocco(&mut f, if v3 { b"ImageMetadataSeqLV|0!" } else { b"ImageMetadataSeq|0!" }).and_then(|b| metadati(&b)).ok();
        let meta = meta.as_ref().map(|m| m.get("SLxPictureMetadata").unwrap_or(m));
        let piani = meta.and_then(|m| m.percorso(&["sPicturePlanes"]));
        let n_piani = piani.and_then(|p| p.get("uiCount")).and_then(Lv::num).unwrap_or(1.0).max(1.0) as usize;
        let n_piani = if componenti.is_multiple_of(n_piani) { n_piani } else { componenti };
        let per_piano = componenti / n_piani;
        let descrizioni: Vec<NomeColore> = (0..n_piani)
            .map(|i| {
                let p = piani.and_then(|p| p.get("sPlaneNew")).and_then(|l| l.get(&format!("a{i}")).or_else(|| l.figli().get(i).map(|(_, x)| x)));
                let nome = p.and_then(|p| p.get("sDescription")).and_then(Lv::testo).unwrap_or("").to_string();
                let colore = p.and_then(|p| p.get("uiColor")).and_then(Lv::num).and_then(colore_nikon);
                (nome, colore)
            })
            .collect();
        let mut elenco: Vec<NomeColore> = Vec::new();
        for (nome, colore) in &descrizioni {
            if per_piano == 1 {
                elenco.push((nome.clone(), *colore));
            } else {
                for k in 0..per_piano {
                    let (suffisso, rgb) = match (per_piano, k) {
                        (3, 0) => ("R".to_string(), (255, 0, 0)),
                        (3, 1) => ("G".to_string(), (0, 255, 0)),
                        (3, 2) => ("B".to_string(), (0, 0, 255)),
                        _ => (format!("{}", k + 1), (255, 255, 255)),
                    };
                    elenco.push((format!("{nome} {suffisso}").trim().to_string(), Some(rgb)));
                }
            }
        }
        let mut canali = canali_standard(componenti, &elenco);
        // I nomi vuoti dei file a colori diventano R, G, B.
        if per_piano == 3 && n_piani == 1 {
            for (c, n) in canali.iter_mut().zip(["R", "G", "B"]) {
                c.nome = n.to_string();
            }
        }
        let calibrato = meta.and_then(|m| m.get("bCalibrated")).and_then(Lv::num).unwrap_or(1.0) != 0.0;
        let um_per_px = meta.and_then(|m| m.get("dCalibration")).and_then(Lv::num).filter(|v| calibrato && *v > 0.0 && v.is_finite());
        let obiettivo = meta.and_then(|m| m.get("wsObjectiveName")).and_then(Lv::testo).map(str::to_string).filter(|s| !s.is_empty());

        // I cicli e l'indirizzo dei fotogrammi.
        let mut lista: Vec<Ciclo> = Vec::new();
        if let Ok(b) = blocco(&mut f, lv("ImageMetadata!").as_bytes()) {
            if let Ok(esp) = metadati(&b) {
                leggi_cicli(esp.get("SLxExperiment").unwrap_or(&esp), 0, &mut lista);
            }
        }
        let mut fotogrammi = HashMap::new();
        for (nome, (off, _)) in &mappa {
            if let Some(n) = nome.strip_prefix(b"ImageDataSeq|").and_then(|r| r.strip_suffix(b"!")).and_then(|r| std::str::from_utf8(r).ok()).and_then(|r| r.parse::<usize>().ok()) {
                fotogrammi.insert(n, *off);
            }
        }
        if fotogrammi.is_empty() {
            return Err("il file ND2 non ha immagini".into());
        }
        let um_z = lista.iter().find(|c| matches!(c.tipo, 4 | 10)).map(|c| c.passo_z).filter(|s| *s > 0.0);
        let mut interrotta = false;
        let mut cicli: Vec<(Asse, usize)> = lista.iter().map(|c| (asse_di(c.tipo), c.n)).collect();
        let sequenza = sequenza.max(fotogrammi.keys().max().map_or(0, |m| m + 1)).max(1);
        let totale = cicli.iter().map(|(_, n)| *n).try_fold(1usize, |a, n| a.checked_mul(n)).unwrap_or(usize::MAX);
        let interni = cicli.iter().skip(1).map(|(_, n)| *n).try_fold(1usize, |a, n| a.checked_mul(n)).unwrap_or(usize::MAX);
        if cicli.is_empty() {
            cicli = vec![(if sequenza > 1 { Asse::Tempo } else { Asse::Z }, sequenza)];
        } else if totale > sequenza && interni <= sequenza {
            // Un'acquisizione fermata prima della fine: il ciclo più esterno ha meno giri, e l'ultimo
            // può essere a metà (i fotogrammi che mancano restano neri).
            cicli[0].1 = sequenza.div_ceil(interni);
            interrotta = sequenza % interni != 0;
        } else if totale < sequenza && sequenza % totale == 0 {
            // Un ciclo che non conosciamo: lo si mette fuori, come un tempo.
            cicli.insert(0, (Asse::Altro, sequenza / totale));
        } else if totale != sequenza {
            cicli = vec![(Asse::Z, sequenza)];
        }
        let float = bpc_bit == 32;
        let bit = significativi.unwrap_or(bpc_bit) as u8;
        let campioni = if per_piano == 3 && n_piani == 1 && bpc_bit == 8 { 3 } else { 1 };
        Ok(Nd2 {
            larghezza,
            altezza,
            larghezza_byte,
            componenti,
            byte_per_componente: bpc_bit / 8,
            decimale: float,
            compresso: compressione == 0,
            fotogrammi,
            cicli,
            canali,
            bit,
            um_per_px,
            um_z,
            obiettivo,
            campioni,
            interrotta,
        })
    }

    pub fn z(&self) -> usize {
        self.cicli.iter().filter(|(a, _)| *a == Asse::Z).map(|(_, n)| *n).product::<usize>().max(1)
    }

    /// Tempo, posizioni e il resto messi in fila: l'indice «istante» di Cedro.
    pub fn t(&self) -> usize {
        self.cicli.iter().filter(|(a, _)| *a != Asse::Z).map(|(_, n)| *n).product::<usize>().max(1)
    }

    pub fn posizioni(&self) -> usize {
        self.cicli.iter().filter(|(a, _)| *a == Asse::Posizione).map(|(_, n)| *n).product::<usize>().max(1)
    }

    pub fn fotogrammi(&self) -> usize {
        self.fotogrammi.len()
    }

    /// Il numero del fotogramma per il piano `z` dell'istante `t`: i cicli in fila, l'ultimo più veloce.
    pub fn indice(&self, z: usize, t: usize) -> usize {
        let mut resto_t = t;
        let mut coord = vec![0usize; self.cicli.len()];
        // Gli istanti si scompongono sui cicli che non sono Z, dall'ultimo al primo.
        for (i, (a, n)) in self.cicli.iter().enumerate().rev() {
            if *a == Asse::Z {
                coord[i] = z.min(n.saturating_sub(1));
            } else {
                coord[i] = resto_t % n;
                resto_t /= n;
            }
        }
        let mut idx = 0usize;
        for (i, (_, n)) in self.cicli.iter().enumerate() {
            idx = idx * n + coord[i];
        }
        idx
    }

    /// Tutti i canali di un fotogramma.
    pub fn fotogramma(&self, path: &Path, indice: usize) -> Result<Vec<Piano>, String> {
        let (w, h) = (self.larghezza as usize, self.altezza as usize);
        let grezzo = match self.fotogrammi.get(&indice) {
            None => vec![0u8; self.larghezza_byte * h],
            Some(off) => {
                let mut f = File::open(path).map_err(|e| e.to_string())?;
                f.seek(SeekFrom::Start(*off)).map_err(|e| e.to_string())?;
                let mut hd = [0u8; 16];
                f.read_exact(&mut hd).map_err(|_| "file ND2 troncato")?;
                if hd[..4] != MAGIC {
                    return Err(format!("il fotogramma {} non è dove dice la mappa", indice + 1));
                }
                let nome = u64::from(u32::from_le_bytes([hd[4], hd[5], hd[6], hd[7]]));
                let dati = u64::from_le_bytes(hd[8..16].try_into().map_err(|_| "file ND2 troncato")?);
                // Davanti ai pixel ci sono 8 byte con l'istante di acquisizione.
                f.seek(SeekFrom::Start(off + 16 + nome + 8)).map_err(|e| e.to_string())?;
                let atteso = self.larghezza_byte * h;
                if self.compresso {
                    let mut compresso = vec![0u8; dati.saturating_sub(8).min(MAX_FOTOGRAMMA) as usize];
                    f.read_exact(&mut compresso).map_err(|_| "file ND2 troncato")?;
                    let mut fuori = Vec::with_capacity(atteso);
                    flate2::read::ZlibDecoder::new(&compresso[..]).take(atteso as u64).read_to_end(&mut fuori).map_err(|e| format!("fotogramma {} non decompresso: {e}", indice + 1))?;
                    fuori.resize(atteso, 0);
                    fuori
                } else {
                    let mut v = vec![0u8; atteso];
                    f.read_exact(&mut v).map_err(|_| "file ND2 troncato")?;
                    v
                }
            }
        };
        let bpc = self.byte_per_componente;
        let componenti = self.componenti;
        let valore = |riga: usize, x: usize, c: usize| -> &[u8] {
            let o = riga * self.larghezza_byte + (x * componenti + c) * bpc;
            &grezzo[o..o + bpc]
        };
        let mut out: Vec<Piano> = Vec::with_capacity(componenti);
        for c in 0..componenti {
            let dati: Vec<u16> = if self.decimale {
                // Numeri decimali: tra 0 e 1 si portano a tutta scala, altrimenti si tengono come sono.
                let f: Vec<f32> = (0..h).flat_map(|r| (0..w).map(move |x| (r, x))).map(|(r, x)| f32::from_le_bytes(valore(r, x, c).try_into().unwrap_or([0; 4]))).collect();
                crate::microscopia::da_f32(&f)
            } else if bpc == 4 {
                let v: Vec<u32> = (0..h).flat_map(|r| (0..w).map(move |x| (r, x))).map(|(r, x)| u32::from_le_bytes(valore(r, x, c).try_into().unwrap_or([0; 4]))).collect();
                crate::microscopia::da_u32(&v)
            } else {
                let mut d = Vec::with_capacity(w * h);
                for r in 0..h {
                    for x in 0..w {
                        let b = valore(r, x, c);
                        d.push(match bpc {
                            1 => u16::from(b[0]),
                            _ => u16::from_le_bytes([b[0], b[1]]),
                        });
                    }
                }
                d
            };
            out.push(Piano { dati, scala: if bpc == 1 { 255 } else { 65535 } });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod prove {
    use super::*;
    use std::io::Write;

    // ---- Un piccolo scrittore di ND2 (per le prove): blocchi, metadati lite variant e mappa.

    fn nome16(n: &str) -> Vec<u8> {
        n.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
    }

    fn voce(tipo: u8, nome: &str, dati: &[u8]) -> Vec<u8> {
        let n = nome16(nome);
        let mut v = vec![tipo, (n.len() / 2) as u8];
        v.extend(n);
        v.extend(dati);
        v
    }

    pub fn num(nome: &str, v: f64) -> Vec<u8> {
        voce(6, nome, &v.to_le_bytes())
    }
    fn intero(nome: &str, v: u32) -> Vec<u8> {
        voce(3, nome, &v.to_le_bytes())
    }
    fn booleano(nome: &str, v: bool) -> Vec<u8> {
        voce(1, nome, &[u8::from(v)])
    }
    fn testo(nome: &str, s: &str) -> Vec<u8> {
        voce(8, nome, &nome16(s))
    }
    fn livello(nome: &str, figli: &[Vec<u8>]) -> Vec<u8> {
        let n = nome16(nome);
        let intest = 2 + n.len() + 4 + 8;
        let corpo: usize = figli.iter().map(Vec::len).sum();
        let mut v = vec![11, (n.len() / 2) as u8];
        v.extend(n);
        v.extend((figli.len() as u32).to_le_bytes());
        v.extend(((intest + corpo) as u64).to_le_bytes());
        for f in figli {
            v.extend(f);
        }
        v.extend(vec![0u8; figli.len() * 8]);
        v
    }

    fn blocco(nome: &str, dati: &[u8]) -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.extend((nome.len() as u32).to_le_bytes());
        v.extend((dati.len() as u64).to_le_bytes());
        v.extend(nome.as_bytes());
        v.extend(dati);
        v
    }

    /// Un ND2 v3 con tempo (2) e Z (3), due canali interleaved 16 bit di 8×6 pixel; il valore di un
    /// pixel dice dove sta: canale·1000 + t·100 + z·10 (+1 per il pixel in alto a sinistra).
    pub fn nd2(dir: &Path, compresso: bool) -> std::path::PathBuf {
        scrivi(dir, compresso, None)
    }

    /// Un ND2 come `nd2` ma fermato dopo `fotogrammi` fotogrammi.
    pub fn scrivi_fermato(dir: &Path, fotogrammi: usize) -> std::path::PathBuf {
        scrivi_n(dir, false, None, fotogrammi)
    }

    /// Lo stesso file nella versione 2, con i metadati in XML.
    fn xml_v2(nome: &str, corpo: &str) -> String {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><variant version=\"1.0\"><no_name runtype=\"{nome}\">{corpo}</no_name></variant>")
    }
    fn x(nome: &str, tipo: &str, v: impl std::fmt::Display) -> String {
        format!("<{nome} runtype=\"{tipo}\" value=\"{v}\"/>")
    }

    fn scrivi(dir: &Path, compresso: bool, v2: Option<()>) -> std::path::PathBuf {
        scrivi_n(dir, compresso, v2, 6)
    }

    /// Come `scrivi`, con solo i primi `fotogrammi` fotogrammi (un'acquisizione fermata prima).
    fn scrivi_n(dir: &Path, compresso: bool, v2: Option<()>, fotogrammi: usize) -> std::path::PathBuf {
        let (w, h, t, z) = (8usize, 6usize, 2usize, 3usize);
        let attr = livello("SLxImageAttributes", &[intero("uiWidth", w as u32), intero("uiWidthBytes", (w * 4) as u32), intero("uiHeight", h as u32), intero("uiComp", 2), intero("uiBpcInMemory", 16), intero("uiBpcSignificant", 12), intero("uiSequenceCount", fotogrammi as u32), intero("eCompression", if compresso { 0 } else { 2 })]);
        let piano = |d: &str, colore: u32| livello(if d == "DAPI" { "a0" } else { "a1" }, &[testo("sDescription", d), intero("uiColor", colore)]);
        let meta = livello(
            "SLxPictureMetadata",
            &[
                num("dCalibration", 0.65),
                booleano("bCalibrated", true),
                testo("wsObjectiveName", "Plan Fluor 20x"),
                livello("sPicturePlanes", &[intero("uiCount", 2), livello("sPlaneNew", &[piano("DAPI", 0xFF8040), piano("GFP", 0x00FF00)])]),
            ],
        );
        let ciclo_z = livello("", &[intero("eType", 4), livello("uLoopPars", &[intero("uiCount", z as u32), num("dZStep", 1.5)])]);
        let esp = livello(
            "SLxExperiment",
            &[intero("eType", 1), livello("uLoopPars", &[intero("uiCount", t as u32)]), livello("ppNextLevelEx", &[ciclo_z])],
        );
        let mut file: Vec<u8> = Vec::new();
        let mut mappa: Vec<(String, u64, u64)> = Vec::new();
        let mut aggiungi = |file: &mut Vec<u8>, nome: &str, dati: &[u8]| {
            let off = file.len() as u64;
            let b = blocco(nome, dati);
            mappa.push((nome.to_string(), off, b.len() as u64));
            file.extend(b);
        };
        let mut firma = if v2.is_some() { b"Ver2.1".to_vec() } else { b"Ver3.0".to_vec() };
        firma.resize(64, 0);
        file.extend(blocco(std::str::from_utf8(FIRMA_FILE).unwrap(), &firma));
        if v2.is_some() {
            let attr = xml_v2(
                "SLxImageAttributes",
                &[x("uiWidth", "lx_uint32", w), x("uiWidthBytes", "lx_uint32", w * 4), x("uiHeight", "lx_uint32", h), x("uiComp", "lx_uint32", 2), x("uiBpcInMemory", "lx_uint32", 16), x("uiSequenceCount", "lx_uint32", t * z), x("eCompression", "lx_int32", if compresso { 0 } else { 2 })].concat(),
            );
            let piano = |a: &str, d: &str, c: u32| format!("<{a} runtype=\"SLxPicturePlaneDesc\">{}{}</{a}>", x("sDescription", "CLxStringW", d), x("uiColor", "lx_uint32", c));
            let meta = xml_v2(
                "SLxPictureMetadata",
                &format!(
                    "{}{}{}<sPicturePlanes runtype=\"SLxPicturePlanes\">{}<sPlaneNew runtype=\"CLxListVariant\">{}{}</sPlaneNew></sPicturePlanes>",
                    x("dCalibration", "double", 0.65),
                    x("bCalibrated", "bool", "true"),
                    x("wsObjectiveName", "CLxStringW", "Plan Fluor 20x"),
                    x("uiCount", "lx_uint32", 2),
                    piano("a0", "DAPI", 0xFF8040),
                    piano("a1", "GFP", 0x00FF00)
                ),
            );
            // Tempo con dentro le posizioni (2 punti, uno solo valido) e poi Z: le posizioni valgono 1.
            let z_loop = format!("<no_name runtype=\"RLxExperiment\">{}<uLoopPars runtype=\"CLxListVariant\"><no_name runtype=\"RLxExpZStackLoop\">{}{}</no_name></uLoopPars></no_name>", x("eType", "lx_uint32", 4), x("uiCount", "lx_uint32", z), x("dZStep", "double", 1.5));
            let xy_loop = format!(
                "<no_name runtype=\"RLxExperiment\">{}<uLoopPars runtype=\"CLxListVariant\"><no_name runtype=\"RLxExpXYPosLoop\"><Points runtype=\"CLxListVariant\"><no_name runtype=\"P\">{}</no_name><no_name runtype=\"P\">{}</no_name></Points></no_name></uLoopPars><pItemValid runtype=\"CLxListVariant\">{}{}</pItemValid><ppNextLevelEx runtype=\"CLxListVariant\">{z_loop}</ppNextLevelEx></no_name>",
                x("eType", "lx_uint32", 2),
                x("dPosX", "double", 1),
                x("dPosX", "double", 2),
                x("no_name", "bool", "true"),
                x("no_name", "bool", "false")
            );
            let esp = xml_v2("RLxExperiment", &format!("{}<uLoopPars runtype=\"CLxListVariant\"><no_name runtype=\"RLxExpTimeLoop\">{}</no_name></uLoopPars><ppNextLevelEx runtype=\"CLxListVariant\">{xy_loop}</ppNextLevelEx>", x("eType", "lx_uint32", 1), x("uiCount", "lx_uint32", t)));
            aggiungi(&mut file, "ImageAttributes!", attr.as_bytes());
            aggiungi(&mut file, "ImageMetadataSeq|0!", meta.as_bytes());
            aggiungi(&mut file, "ImageMetadata!", esp.as_bytes());
        } else {
            aggiungi(&mut file, "ImageAttributesLV!", &attr);
            aggiungi(&mut file, "ImageMetadataSeqLV|0!", &meta);
            aggiungi(&mut file, "ImageMetadataLV!", &esp);
        }
        for ti in 0..t {
            for zi in 0..z {
                let idx = ti * z + zi;
                if idx >= fotogrammi {
                    continue;
                }
                let mut px = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        for c in 0..2usize {
                            let v = (c * 1000 + ti * 100 + zi * 10 + usize::from(x == 0 && y == 0)) as u16;
                            px.extend(v.to_le_bytes());
                        }
                    }
                }
                let mut dati = vec![0u8; 8];
                if compresso {
                    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
                    e.write_all(&px).unwrap();
                    dati.extend(e.finish().unwrap());
                } else {
                    dati.extend(px);
                }
                aggiungi(&mut file, &format!("ImageDataSeq|{idx}!"), &dati);
            }
        }
        let pos_mappa = file.len() as u64;
        let mut elenco = Vec::new();
        for (n, off, dim) in &mappa {
            elenco.extend(n.as_bytes());
            elenco.extend(off.to_le_bytes());
            elenco.extend(dim.to_le_bytes());
        }
        elenco.extend(FIRMA_MAPPA);
        elenco.extend([0u8; 16]);
        file.extend(blocco(std::str::from_utf8(FIRMA_ELENCO).unwrap(), &elenco));
        file.extend(FIRMA_MAPPA);
        file.extend(pos_mappa.to_le_bytes());
        let p = dir.join(format!("{}{}{}.nd2", if compresso { "zlib" } else { "grezzo" }, if v2.is_some() { "-v2" } else { "" }, if fotogrammi < t * z { "-fermato" } else { "" }));
        std::fs::write(&p, file).unwrap();
        p
    }

    #[test]
    fn un_nd2_si_legge_con_canali_cicli_e_scala() {
        for (compresso, v2) in [(false, None), (true, None), (false, Some(())), (true, Some(()))] {
            let dir = std::env::temp_dir().join(format!("cedro-nd2-{}-{compresso}-{}", std::process::id(), v2.is_some()));
            std::fs::create_dir_all(&dir).unwrap();
            let p = scrivi(&dir, compresso, v2);
            let n = Nd2::open(&p).unwrap();
            assert_eq!((n.larghezza, n.altezza, n.z(), n.t(), n.fotogrammi()), (8, 6, 3, 2, 6));
            assert_eq!(n.canali.iter().map(|c| c.nome.as_str()).collect::<Vec<_>>(), ["DAPI", "GFP"]);
            assert_eq!(n.canali[0].colore, (0x40, 0x80, 0xFF), "il colore di Nikon è BGR");
            assert_eq!(n.um_per_px, Some(0.65));
            assert_eq!(n.um_z, Some(1.5));
            assert_eq!(n.obiettivo.as_deref(), Some("Plan Fluor 20x"));
            // Il fotogramma del piano z=2 dell'istante t=1 è il 5° (1·3+2).
            assert_eq!(n.posizioni(), 1, "una sola posizione valida");
            assert_eq!(n.indice(2, 1), 5);
            let f = n.fotogramma(&p, n.indice(2, 1)).unwrap();
            assert_eq!(f.len(), 2);
            assert_eq!((f[0].dati[0], f[0].dati[1], f[1].dati[0], f[1].dati[1]), (121, 120, 1121, 1120));
            assert_eq!(f[0].scala, 65535);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Prova su un file vero (`CEDRO_ND2=file.nd2`): scrive i pixel in ordine T, Z, C, Y, X in
    /// `file.nd2.u16` e stampa le misure, per confrontarli con un altro lettore.
    #[test]
    #[ignore]
    fn file_vero() {
        let Ok(p) = std::env::var("CEDRO_ND2") else { return };
        let p = std::path::PathBuf::from(p);
        let n = Nd2::open(&p).unwrap();
        println!("{}x{} z={} t={} pos={} canali={:?} bit={} um={:?} umz={:?} obiettivo={:?} cicli={:?}", n.larghezza, n.altezza, n.z(), n.t(), n.posizioni(), n.canali.iter().map(|c| (c.nome.clone(), c.colore)).collect::<Vec<_>>(), n.bit, n.um_per_px, n.um_z, n.obiettivo, n.cicli);
        let mut out = Vec::new();
        for t in 0..n.t() {
            for z in 0..n.z() {
                for piano in n.fotogramma(&p, n.indice(z, t)).unwrap() {
                    for v in piano.dati {
                        out.extend(v.to_le_bytes());
                    }
                }
            }
        }
        std::fs::write(p.with_extension("nd2.u16"), out).unwrap();
    }

    #[test]
    fn i_cicli_si_contano_come_in_nis_elements() {
        let l = |figli: Vec<(&str, Lv)>| Lv::Livello(figli.into_iter().map(|(n, v)| (n.to_string(), v)).collect());
        let n = |v: f64| Lv::Num(v);
        let ciclo = |tipo: f64, conta: f64, dentro: Vec<Lv>| {
            l(vec![("eType", n(tipo)), ("uLoopPars", l(vec![("uiCount", n(conta)), ("dZStep", n(-0.5))])), ("ppNextLevelEx", Lv::Livello(dentro.into_iter().map(|v| (String::new(), v)).collect()))])
        };
        // Tempo 5 → Z 4 (passo negativo: conta il valore), con un ciclo spettrale in mezzo che non conta.
        let esp = ciclo(1.0, 5.0, vec![ciclo(6.0, 3.0, vec![ciclo(4.0, 4.0, vec![])])]);
        let mut d = Vec::new();
        leggi_cicli(&esp, 0, &mut d);
        assert_eq!(d.iter().map(|c| (c.tipo, c.n, c.livello)).collect::<Vec<_>>(), [(1, 5, 0), (4, 4, 1)]);
        assert_eq!(d[1].passo_z, 0.5);
        // Un ciclo vuoto chiude il ramo: lo Z dentro non si conta.
        let esp = ciclo(1.0, 0.0, vec![ciclo(4.0, 4.0, vec![])]);
        let mut d = Vec::new();
        leggi_cicli(&esp, 0, &mut d);
        assert!(d.is_empty());
        // Tempo a fasi: contano solo le fasi valide.
        let fasi = l(vec![
            ("eType", n(8.0)),
            ("uLoopPars", l(vec![("pPeriod", l(vec![("", l(vec![("uiCount", n(10.0))])), ("", l(vec![("uiCount", n(7.0))])), ("", l(vec![("uiCount", n(2.0))]))])), ("pPeriodValid", Lv::Byte(vec![1, 0, 1]))])),
        ]);
        let mut d = Vec::new();
        leggi_cicli(&fasi, 0, &mut d);
        assert_eq!(d.iter().map(|c| c.n).collect::<Vec<_>>(), [12]);
    }
}

#[cfg(test)]
mod prove_revisione {
    use super::*;

    #[test]
    fn un_acquisizione_fermata_a_meta_resta_tempo_per_z() {
        // 2 istanti × 3 piani, fermata al 5° fotogramma: non diventa una pila di 5 piani Z.
        let dir = std::env::temp_dir().join(format!("cedro-nd2-fermato-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = prove::scrivi_fermato(&dir, 5);
        let n = Nd2::open(&p).unwrap();
        assert_eq!((n.z(), n.t(), n.fotogrammi()), (3, 2, 5));
        assert!(n.interrotta);
        assert_eq!(n.fotogramma(&p, n.indice(1, 1)).unwrap()[0].dati[0], 111);
        // Il piano che manca è nero.
        assert!(n.fotogramma(&p, n.indice(2, 1)).unwrap()[0].dati.iter().all(|v| *v == 0));
        let pila = crate::microscopia::Pila::open(&p).unwrap();
        assert!(pila.avviso.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_blocco_che_dice_misure_assurde_non_prende_memoria() {
        let dir = std::env::temp_dir().join(format!("cedro-nd2-assurdo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("rotto.nd2");
        let mut b = MAGIC.to_vec();
        b.extend(0xFFFF_FFF0u32.to_le_bytes());
        b.extend((1u64 << 31).to_le_bytes());
        b.extend([0u8; 64]);
        std::fs::write(&p, b).unwrap();
        assert!(Nd2::open(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
