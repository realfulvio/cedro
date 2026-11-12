//! Lettore dei file CZI di Zeiss (ZEN, LSM 8xx, Axio Observer/Imager, Axio Scan, Celldiscoverer).
//!
//! Un CZI è una fila di segmenti, ognuno con un'intestazione di 32 byte (nome di 16 byte, spazio
//! occupato e spazio usato). In testa c'è `ZISRAWFILE` con gli indirizzi della directory e dei
//! metadati. La directory (`ZISRAWDIRECTORY`) elenca i sottoblocchi (`ZISRAWSUBBLOCK`): ognuno è un
//! rettangolo di pixel con le sue coordinate (X, Y, canale C, piano Z, istante T, scena S, tessera M,
//! e le meno usate R, I, H, V) e la sua compressione (nessuna, JPEG, LZW, JPEG XR, zstd). Un'immagine
//! «a mosaico» si ricompone mettendo le tessere al loro posto; i vetrini hanno anche i livelli ridotti
//! (piramide), tessere con meno pixel di quelli che coprono. I metadati sono XML: misura del pixel,
//! nomi e colori dei canali, obiettivo, nomi delle scene.
//!
//! Il formato lo descrive Zeiss («ZISRAW (CZI) File Format Design specification») e lo legge la sua
//! libreria aperta libCZI: i file di prova si confrontano pixel per pixel con libCZI (pylibCZIrw).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::microscopia::{NomeColore, Piano};

/// Vero se i primi byte sono quelli di un CZI.
pub fn riconosci(head: &[u8]) -> bool {
    head.starts_with(b"ZISRAWFILE")
}

/// I pixel oltre questo numero non stanno in un piano: si legge un livello ridotto (diviso per i
/// campioni, tre negli RGB).
const MASSIMO_PX: u64 = 100_000_000;

/// Gli assi che finiscono negli «istanti», in ordine (il primo è il più esterno).
const ASSI_ALTRI: [u8; 5] = *b"TRIHV";

/// Il tipo dei pixel di un sottoblocco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tipo {
    Gray8,
    Gray16,
    Gray32,
    Gray32Float,
    Bgr24,
    Bgr48,
    Bgr96Float,
    Bgra32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Campione {
    U8,
    U16,
    U32,
    F32,
}

impl Tipo {
    fn da(n: i32) -> Result<Tipo, String> {
        Ok(match n {
            0 => Tipo::Gray8,
            1 => Tipo::Gray16,
            2 => Tipo::Gray32Float,
            3 => Tipo::Bgr24,
            4 => Tipo::Bgr48,
            8 => Tipo::Bgr96Float,
            9 => Tipo::Bgra32,
            12 => Tipo::Gray32,
            10 | 11 => return Err("pixel complessi (trasformate di Fourier) non letti".into()),
            13 => return Err("pixel a 64 bit non letti".into()),
            _ => return Err(format!("tipo di pixel {n} sconosciuto")),
        })
    }
    /// I campioni che si vedono (l'alfa no).
    fn componenti(self) -> usize {
        match self {
            Tipo::Bgr24 | Tipo::Bgr48 | Tipo::Bgr96Float | Tipo::Bgra32 => 3,
            _ => 1,
        }
    }
    /// I campioni salvati per pixel.
    fn salvati(self) -> usize {
        match self {
            Tipo::Bgra32 => 4,
            t => t.componenti(),
        }
    }
    fn campione(self) -> Campione {
        match self {
            Tipo::Gray8 | Tipo::Bgr24 | Tipo::Bgra32 => Campione::U8,
            Tipo::Gray16 | Tipo::Bgr48 => Campione::U16,
            Tipo::Gray32 => Campione::U32,
            Tipo::Gray32Float | Tipo::Bgr96Float => Campione::F32,
        }
    }
    fn byte_campione(self) -> usize {
        match self.campione() {
            Campione::U8 => 1,
            Campione::U16 => 2,
            Campione::U32 | Campione::F32 => 4,
        }
    }
    fn byte_pixel(self) -> usize {
        self.salvati() * self.byte_campione()
    }
    fn bit(self) -> u8 {
        match self.campione() {
            Campione::U8 => 8,
            _ => 16,
        }
    }
}

/// Un sottoblocco della directory: dove sta, com'è fatto e dove va.
#[derive(Clone, Debug)]
struct Blocco {
    pos: u64,
    tipo: i32,
    compressione: i32,
    /// Il rettangolo che copre, in pixel del livello pieno.
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    /// I pixel salvati (meno di `w`×`h` nei livelli ridotti).
    sw: u32,
    sh: u32,
    c: Option<i32>,
    z: Option<i32>,
    s: Option<i32>,
    m: Option<i32>,
    /// T, R, I, H, V.
    altri: [Option<i32>; 5],
    parte: i32,
}

impl Blocco {
    /// Di quanto è ridotto: 1 al livello pieno, 2, 3, 4… nella piramide.
    fn livello(&self) -> u32 {
        let z = self.w as f64 / f64::from(self.sw.max(1));
        if z < 1.5 {
            1
        } else {
            z.round() as u32
        }
    }
}

fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn i64_at(b: &[u8], o: usize) -> Option<i64> {
    Some(i64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    Some(f32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Una voce della directory (schema «DV», o il vecchio «DE» a misura fissa) e quanti byte occupa.
fn voce(b: &[u8]) -> Result<(Blocco, usize), String> {
    let corta = || "directory del file CZI tagliata".to_string();
    match b.get(..2) {
        Some(b"DV") => {
            let n = i32_at(b, 28).ok_or_else(corta)?;
            if !(0..=40).contains(&n) {
                return Err("directory del file CZI non valida".into());
            }
            let lunghezza = 32 + 20 * n as usize;
            if b.len() < lunghezza {
                return Err(corta());
            }
            let mut k = Blocco {
                pos: i64_at(b, 6).ok_or_else(corta)?.max(0) as u64,
                tipo: i32_at(b, 2).ok_or_else(corta)?,
                compressione: i32_at(b, 18).ok_or_else(corta)?,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                sw: 0,
                sh: 0,
                c: None,
                z: None,
                s: None,
                m: None,
                altri: [None; 5],
                parte: i32_at(b, 14).ok_or_else(corta)?,
            };
            for d in 0..n as usize {
                let o = 32 + 20 * d;
                let nome = b[o];
                let inizio = i32_at(b, o + 4).unwrap_or(0);
                let misura = i32_at(b, o + 8).unwrap_or(0);
                let _coordinata = f32_at(b, o + 12);
                let salvata = i32_at(b, o + 16).unwrap_or(0);
                // Il nome è di quattro caratteri, il primo è la lettera dell'asse.
                if b[o + 1] != 0 {
                    continue;
                }
                match nome {
                    b'X' => (k.x, k.w, k.sw) = (i64::from(inizio), i64::from(misura), salvata.max(0) as u32),
                    b'Y' => (k.y, k.h, k.sh) = (i64::from(inizio), i64::from(misura), salvata.max(0) as u32),
                    b'C' => k.c = Some(inizio),
                    b'Z' => k.z = Some(inizio),
                    b'S' => k.s = Some(inizio),
                    b'M' => k.m = Some(inizio),
                    a => {
                        if let Some(i) = ASSI_ALTRI.iter().position(|x| *x == a) {
                            k.altri[i] = Some(inizio);
                        }
                    }
                }
            }
            Ok((k, lunghezza))
        }
        Some(b"DE") => {
            if b.len() < 128 {
                return Err(corta());
            }
            let g = |o| i32_at(b, o).unwrap_or(0);
            let k = Blocco {
                pos: i64_at(b, 80).unwrap_or(0).max(0) as u64,
                tipo: g(2),
                compressione: g(72),
                x: i64::from(g(16)),
                y: i64::from(g(24)),
                w: i64::from(g(20)),
                h: i64::from(g(28)),
                sw: g(6).max(0) as u32,
                sh: g(10).max(0) as u32,
                c: Some(g(32)),
                z: Some(g(40)),
                s: Some(g(56)),
                m: Some(g(76)),
                altri: [Some(g(48)), Some(g(60)), Some(g(64)), Some(g(108)), None],
                parte: g(88),
            };
            Ok((k, 128))
        }
        _ => Err("voce della directory CZI sconosciuta".into()),
    }
}

/// Un'immagine del file: una scena (i CZI con più posizioni o più aree del vetrino ne hanno più d'una).
#[derive(Clone, Debug)]
pub struct Scena {
    /// Il nome della scena («P1», «Scena 2»…).
    pub nome: String,
    pub larghezza: u32,
    pub altezza: u32,
    /// I canali: negli RGB (luce trasmessa, vetrini) ogni canale dà tre colori.
    pub canali: Vec<NomeColore>,
    pub z: usize,
    /// Tempo e gli altri assi messi in fila: (lettera, quanti).
    pub altri: Vec<(u8, usize)>,
    pub bit: u8,
    pub um_per_px: Option<f64>,
    pub um_z: Option<f64>,
    pub obiettivo: Option<String>,
    pub rgb: bool,
    /// Letta ridotta perché troppo grande: (quante volte, larghezza e altezza piene).
    pub ridotta: Option<(u32, u32, u32)>,
    x0: i64,
    y0: i64,
    /// Il livello della piramide letto e la riduzione in più fatta qui.
    livello: u32,
    resto: u32,
    blocchi: Vec<Blocco>,
    componenti: usize,
    /// Il valore più basso di C, Z e degli altri assi (non sempre partono da 0).
    c0: i32,
    z0: i32,
    altri0: Vec<(usize, i32, usize)>,
    /// Il tipo dei pixel di ogni canale.
    tipi: Vec<Tipo>,
}

#[derive(Debug)]
pub struct Czi {
    pub scene: Vec<Scena>,
    /// Quello che non torna nel file e va detto (un CZI diviso in più file, una scena illeggibile…).
    pub avviso: Option<String>,
}

fn leggi_a(f: &mut BufReader<File>, pos: u64, n: usize) -> Result<Vec<u8>, String> {
    f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
    let mut b = vec![0u8; n];
    f.read_exact(&mut b).map_err(|_| "file CZI troncato".to_string())?;
    Ok(b)
}

/// Il nome del segmento all'indirizzo `pos` e lo spazio che occupa.
fn segmento(f: &mut BufReader<File>, pos: u64) -> Result<([u8; 16], u64, u64), String> {
    let t = leggi_a(f, pos, 32)?;
    let mut id = [0u8; 16];
    id.copy_from_slice(&t[..16]);
    Ok((id, i64_at(&t, 16).unwrap_or(0).max(0) as u64, i64_at(&t, 24).unwrap_or(0).max(0) as u64))
}

fn nome_segmento(id: &[u8; 16]) -> &[u8] {
    let fine = id.iter().position(|b| *b == 0).unwrap_or(16);
    &id[..fine]
}

/// La directory: dall'indirizzo scritto in testa o, se manca o è rovinata, cercando i sottoblocchi
/// uno per uno (un file salvato a metà).
fn directory(f: &mut BufReader<File>, pos: u64, lunghezza_file: u64) -> Result<Vec<Blocco>, String> {
    if pos > 0 && pos + 32 < lunghezza_file {
        if let Ok((id, _, usato)) = segmento(f, pos) {
            if nome_segmento(&id) == b"ZISRAWDIRECTORY" {
                let usato = usato.min(lunghezza_file - pos - 32).min(1 << 30);
                let d = leggi_a(f, pos + 32, usato as usize)?;
                let n = i32_at(&d, 0).unwrap_or(0).max(0) as usize;
                let mut o = 128;
                let mut out = Vec::with_capacity(n.min(1 << 20));
                for _ in 0..n {
                    let Some(resto) = d.get(o..) else { break };
                    let Ok((b, l)) = voce(resto) else { break };
                    out.push(b);
                    o += l;
                }
                if out.len() == n {
                    return Ok(out);
                }
            }
        }
    }
    // Senza directory: i segmenti in fila dal primo.
    let mut out = Vec::new();
    let mut p = 0u64;
    while p + 32 <= lunghezza_file {
        let Ok((id, occupato, _)) = segmento(f, p) else { break };
        let nome = nome_segmento(&id);
        if !nome.starts_with(b"ZISRAW") && nome != b"DELETED" {
            break;
        }
        if nome == b"ZISRAWSUBBLOCK" {
            if let Ok(t) = leggi_a(f, p + 32, 16 + 32 + 20 * 40) {
                if let Ok((mut b, _)) = voce(&t[16..]) {
                    b.pos = p;
                    out.push(b);
                }
            }
        }
        if occupato == 0 {
            break;
        }
        p = p.saturating_add(32 + occupato);
    }
    if out.is_empty() {
        return Err("il file CZI non ha immagini (directory mancante o rovinata)".into());
    }
    Ok(out)
}

/// I metadati XML.
fn metadati(f: &mut BufReader<File>, pos: u64, lunghezza_file: u64) -> Option<String> {
    if pos == 0 || pos + 32 + 256 > lunghezza_file {
        return None;
    }
    let (id, _, _) = segmento(f, pos).ok()?;
    if nome_segmento(&id) != b"ZISRAWMETADATA" {
        return None;
    }
    let t = leggi_a(f, pos + 32, 8).ok()?;
    let n = i32_at(&t, 0)?.max(0) as u64;
    if n == 0 || pos + 32 + 256 + n > lunghezza_file || n > 256 << 20 {
        return None;
    }
    let b = leggi_a(f, pos + 32 + 256, n as usize).ok()?;
    let s = String::from_utf8_lossy(&b).into_owned();
    Some(s.trim_start_matches('\u{feff}').trim_end_matches('\0').to_string())
}

/// Quello che serve dei metadati.
#[derive(Default, Debug)]
struct Info {
    um_x: Option<f64>,
    um_z: Option<f64>,
    canali: Vec<NomeColore>,
    bit: Option<u8>,
    obiettivo: Option<String>,
    scene: BTreeMap<i32, String>,
}

fn figlio<'a, 'i>(n: roxmltree::Node<'a, 'i>, nome: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == nome)
}

fn percorso<'a, 'i>(n: roxmltree::Node<'a, 'i>, p: &[&str]) -> Option<roxmltree::Node<'a, 'i>> {
    p.iter().try_fold(n, |n, nome| figlio(n, nome))
}

fn testo(n: roxmltree::Node, nome: &str) -> Option<String> {
    figlio(n, nome).and_then(|c| c.text()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// «#AARRGGBB» o «#RRGGBB».
fn colore(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    let h = match h.len() {
        8 => &h[2..],
        6 => h,
        _ => return None,
    };
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn leggi_info(xml: &str) -> Info {
    let mut info = Info::default();
    let opzioni = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 20_000_000, ..Default::default() };
    let Ok(doc) = roxmltree::Document::parse_with_options(xml, opzioni) else { return info };
    let Some(meta) = figlio(doc.root_element(), "Metadata") else { return info };
    // Le misure sono in metri.
    if let Some(items) = percorso(meta, &["Scaling", "Items"]) {
        for d in items.children().filter(|c| c.tag_name().name() == "Distance") {
            let v = testo(d, "Value").and_then(|v| v.parse::<f64>().ok()).map(|m| m * 1e6).filter(|v| v.is_finite() && *v > 0.0);
            match d.attribute("Id") {
                Some("X") => info.um_x = v,
                Some("Z") => info.um_z = v,
                _ => {}
            }
        }
    }
    let immagine = percorso(meta, &["Information", "Image"]);
    if let Some(im) = immagine {
        info.bit = testo(im, "ComponentBitCount").and_then(|v| v.parse().ok()).filter(|b| (1..=32).contains(b));
        if let Some(ch) = percorso(im, &["Dimensions", "Channels"]) {
            for c in ch.children().filter(|c| c.tag_name().name() == "Channel") {
                let nome = c.attribute("Name").map(str::to_string).filter(|n| !n.trim().is_empty()).or_else(|| testo(c, "Fluor")).unwrap_or_default();
                info.canali.push((nome, testo(c, "Color").and_then(|s| colore(&s))));
            }
        }
        if let Some(sc) = percorso(im, &["Dimensions", "S", "Scenes"]) {
            for (k, s) in sc.children().filter(|c| c.tag_name().name() == "Scene").enumerate() {
                let i = s.attribute("Index").and_then(|v| v.parse().ok()).unwrap_or(k as i32);
                if let Some(n) = s.attribute("Name").map(str::trim).filter(|n| !n.is_empty()) {
                    info.scene.insert(i, n.to_string());
                }
            }
        }
    }
    // I colori scelti in ZEN per vedere i canali, quando le informazioni non li hanno.
    if let Some(ds) = percorso(meta, &["DisplaySetting", "Channels"]) {
        for (k, c) in ds.children().filter(|c| c.tag_name().name() == "Channel").enumerate() {
            let col = testo(c, "Color").and_then(|s| colore(&s));
            let nome = c.attribute("Name").map(str::to_string).or_else(|| testo(c, "ShortName")).unwrap_or_default();
            match info.canali.get_mut(k) {
                Some(v) => {
                    if v.1.is_none() {
                        v.1 = col;
                    }
                    if v.0.trim().is_empty() {
                        v.0 = nome;
                    }
                }
                None => info.canali.push((nome, col)),
            }
        }
    }
    if let Some(obj) = percorso(meta, &["Information", "Instrument", "Objectives"]).and_then(|o| figlio(o, "Objective")) {
        info.obiettivo = obj
            .attribute("Name")
            .map(str::to_string)
            .or_else(|| testo(obj, "Model"))
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty() && s != "none");
    }
    info
}

impl Czi {
    pub fn open(path: &Path) -> Result<Czi, String> {
        Czi::open_con(path, MASSIMO_PX)
    }

    /// Con un limite diverso ai pixel di un piano (per le prove).
    fn open_con(path: &Path, massimo_px: u64) -> Result<Czi, String> {
        let mut f = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
        let lunghezza_file = f.get_ref().metadata().map_err(|e| e.to_string())?.len();
        let (id, _, _) = segmento(&mut f, 0)?;
        if nome_segmento(&id) != b"ZISRAWFILE" {
            return Err("non è un file CZI".into());
        }
        let testa = leggi_a(&mut f, 32, 80)?;
        let pos_dir = i64_at(&testa, 52).unwrap_or(0).max(0) as u64;
        let pos_meta = i64_at(&testa, 60).unwrap_or(0).max(0) as u64;
        let mut blocchi = directory(&mut f, pos_dir, lunghezza_file)?;
        let info = metadati(&mut f, pos_meta, lunghezza_file).map(|x| leggi_info(&x)).unwrap_or_default();

        // Un CZI diviso in più file (ZEN li chiama «parti»): qui c'è solo la prima.
        let altre_parti = blocchi.iter().any(|b| b.parte != 0);
        // Le voci senza senso (rovinate) si lasciano fuori: una tessera non salva più pixel di quelli che
        // copre, e mai più di 256 milioni.
        blocchi.retain(|b| b.parte == 0 && b.w > 0 && b.h > 0 && b.sw > 0 && b.sh > 0 && i64::from(b.sw) <= b.w && i64::from(b.sh) <= b.h && u64::from(b.sw) * u64::from(b.sh) <= 1 << 28);
        if blocchi.is_empty() {
            return Err("il file CZI non ha immagini".into());
        }
        let mut avvisi: Vec<String> = Vec::new();
        if altre_parti {
            avvisi.push("il CZI è diviso in più file: letta solo questa parte".into());
        }

        // Gli assi: da dove partono e quanti valori hanno, uguali per tutte le scene.
        let estremi = |v: &mut dyn Iterator<Item = i32>| -> (i32, usize) {
            let v: Vec<i32> = v.collect();
            match (v.iter().min(), v.iter().max()) {
                (Some(a), Some(b)) => (*a, (b - a) as usize + 1),
                _ => (0, 1),
            }
        };
        let (c0, nc) = estremi(&mut blocchi.iter().filter_map(|b| b.c));
        let (z0, nz) = estremi(&mut blocchi.iter().filter_map(|b| b.z));
        if nc > 4096 || nz > 100_000 {
            return Err("directory del file CZI non valida".into());
        }
        let mut altri0 = Vec::new();
        for (i, _) in ASSI_ALTRI.iter().enumerate() {
            let (a, n) = estremi(&mut blocchi.iter().filter_map(|b| b.altri[i]));
            if n > 1 {
                altri0.push((i, a, n));
            }
        }
        if altri0.iter().map(|(_, _, n)| *n).product::<usize>() > 10_000_000 {
            return Err("directory del file CZI non valida".into());
        }

        let mut per_scena: BTreeMap<Option<i32>, Vec<Blocco>> = BTreeMap::new();
        for b in blocchi {
            per_scena.entry(b.s).or_default().push(b);
        }
        let quante = per_scena.len();
        let mut scene = Vec::new();
        for (numero, (s, mut bl)) in per_scena.into_iter().enumerate() {
            let nome = s.and_then(|s| info.scene.get(&s).cloned()).unwrap_or_else(|| if quante > 1 { format!("Scena {}", numero + 1) } else { String::new() });
            match Scena::nuova(nome.clone(), &mut bl, &info, c0, nc, z0, nz, &altri0, massimo_px) {
                Ok(sc) => scene.push(sc),
                Err(e) => avvisi.push(format!("scena «{nome}» non letta ({e})")),
            }
        }
        if scene.is_empty() {
            return Err(avvisi.pop().unwrap_or_else(|| "il file CZI non ha immagini".into()));
        }
        Ok(Czi { scene, avviso: (!avvisi.is_empty()).then(|| avvisi.join("; ")) })
    }
}

impl Scena {
    #[allow(clippy::too_many_arguments)]
    fn nuova(nome: String, bl: &mut [Blocco], info: &Info, c0: i32, nc: usize, z0: i32, nz: usize, altri0: &[(usize, i32, usize)], massimo_px: u64) -> Result<Scena, String> {
        // Il rettangolo della scena: le tessere del livello pieno.
        let pieni: Vec<&Blocco> = bl.iter().filter(|b| b.livello() == 1).collect();
        let base: Vec<&Blocco> = if pieni.is_empty() { bl.iter().collect() } else { pieni };
        let x0 = base.iter().map(|b| b.x).min().unwrap_or(0);
        let y0 = base.iter().map(|b| b.y).min().unwrap_or(0);
        let x1 = base.iter().map(|b| b.x + b.w).max().unwrap_or(0);
        let y1 = base.iter().map(|b| b.y + b.h).max().unwrap_or(0);
        let (lw, lh) = ((x1 - x0) as u64, (y1 - y0) as u64);
        if lw == 0 || lh == 0 || lw > 1 << 24 || lh > 1 << 24 {
            return Err("misure dell'immagine non valide".into());
        }
        // Il tipo dei pixel di ogni canale.
        let mut tipi = Vec::with_capacity(nc);
        for c in 0..nc {
            let b = base.iter().find(|b| b.c.unwrap_or(c0) - c0 == c as i32).or(base.first()).ok_or("scena vuota")?;
            tipi.push(Tipo::da(b.tipo)?);
        }
        let componenti = tipi.iter().map(|t| t.componenti()).max().unwrap_or(1);
        // Troppo grande per stare in un piano: il livello della piramide più vicino, e il resto qui.
        let massimo = (massimo_px / componenti as u64).max(1);
        let mut f = 1u32;
        while lw.div_ceil(u64::from(f)) * lh.div_ceil(u64::from(f)) > massimo {
            f += 1;
        }
        let mut livelli: Vec<u32> = bl.iter().map(Blocco::livello).collect();
        livelli.sort_unstable();
        livelli.dedup();
        // Il livello più fine che basta; se anche il più fine è già più piccolo, quello.
        let livello = livelli.iter().copied().filter(|l| *l <= f).max().or_else(|| livelli.first().copied()).unwrap_or(1);
        let resto = f.div_ceil(livello);
        let fattore = livello * resto;
        let mut scelti: Vec<Blocco> = bl.iter().filter(|b| b.livello() == livello).cloned().collect();
        if scelti.is_empty() {
            return Err("nessuna tessera al livello scelto".into());
        }
        // Le tessere si posano in ordine di M, come fa ZEN: dove si sovrappongono vince l'ultima.
        scelti.sort_by_key(|b| b.m.unwrap_or(i32::MIN));
        let larghezza = lw.div_ceil(u64::from(fattore)) as u32;
        let altezza = lh.div_ceil(u64::from(fattore)) as u32;

        let rgb = componenti == 3;
        let mut canali = Vec::new();
        for c in 0..nc {
            let (n, col) = info.canali.get(c).cloned().unwrap_or_default();
            if rgb {
                for (lettera, colore) in [("R", (255, 0, 0)), ("G", (0, 255, 0)), ("B", (0, 0, 255))] {
                    canali.push((if nc == 1 || n.trim().is_empty() { lettera.to_string() } else { format!("{n} {lettera}") }, Some(colore)));
                }
            } else {
                canali.push((n, col));
            }
        }
        let bit_tipo = tipi.iter().map(|t| t.bit()).max().unwrap_or(16);
        let bit = info.bit.filter(|b| *b <= bit_tipo && tipi.iter().all(|t| matches!(t.campione(), Campione::U8 | Campione::U16))).unwrap_or(bit_tipo);
        let mut altri: Vec<(u8, usize)> = altri0.iter().map(|(i, _, n)| (ASSI_ALTRI[*i], *n)).collect();
        if altri.is_empty() {
            altri.push((b'T', 1));
        }
        Ok(Scena {
            nome,
            larghezza,
            altezza,
            canali,
            z: nz,
            altri,
            bit,
            um_per_px: info.um_x.map(|u| u * f64::from(fattore)),
            um_z: info.um_z.filter(|_| nz > 1),
            obiettivo: info.obiettivo.clone(),
            rgb,
            ridotta: (fattore > 1).then_some((fattore, lw as u32, lh as u32)),
            x0,
            y0,
            livello,
            resto,
            blocchi: scelti,
            componenti,
            c0,
            z0,
            altri0: altri0.to_vec(),
            tipi,
        })
    }

    /// Tempo e gli altri assi messi in fila: il numero degli «istanti».
    pub fn t(&self) -> usize {
        self.altri.iter().map(|(_, n)| *n).product::<usize>().max(1)
    }

    /// Il canale `c` (negli RGB un colore) del piano `z` all'istante `t`.
    pub fn piano(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        let k = self.componenti.max(1);
        self.piani(path, c / k, z, t)?.into_iter().nth(c % k).ok_or_else(|| "canale non trovato".to_string())
    }

    /// I campioni che dà un canale del file: 3 negli RGB.
    pub fn componenti(&self) -> usize {
        self.componenti
    }

    /// Tutti i colori del canale `c` del file, nel piano `z` all'istante `t`.
    pub fn piani(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Vec<Piano>, String> {
        let tipo = *self.tipi.get(c).ok_or("canale non trovato")?;
        let cv = self.c0 + c as i32;
        let zv = self.z0 + z.min(self.z.saturating_sub(1)) as i32;
        // L'istante in fila → un valore per asse (l'ultimo asse è il più veloce).
        let mut voluti: Vec<(usize, i32)> = Vec::new();
        let mut resto = t;
        for (i, a, n) in self.altri0.iter().rev() {
            voluti.push((*i, a + (resto % n) as i32));
            resto /= n;
        }
        let blocchi: Vec<&Blocco> = self
            .blocchi
            .iter()
            .filter(|b| b.c.is_none_or(|v| v == cv) && b.z.is_none_or(|v| v == zv) && voluti.iter().all(|(i, v)| b.altri[*i].is_none_or(|x| x == *v)))
            .collect();
        let n = self.larghezza as usize * self.altezza as usize;
        Ok(match tipo.campione() {
            Campione::U8 => self.componi::<u8>(path, &blocchi, tipo)?.into_iter().map(|p| Piano { dati: p.into_iter().map(u16::from).collect(), scala: 255 }).collect(),
            Campione::U16 => self.componi::<u16>(path, &blocchi, tipo)?.into_iter().map(|dati| Piano { dati, scala: 65535 }).collect(),
            Campione::U32 => self.componi::<u32>(path, &blocchi, tipo)?.into_iter().map(|p| Piano { dati: crate::microscopia::da_u32(&p), scala: 65535 }).collect(),
            Campione::F32 => self.componi::<f32>(path, &blocchi, tipo)?.into_iter().map(|p| Piano { dati: crate::microscopia::da_f32(&p), scala: 65535 }).collect(),
        })
        .map(|mut v: Vec<Piano>| {
            // Un canale grigio in un file con canali RGB: lo stesso piano per i tre colori.
            while v.len() < self.componenti {
                let copia = v.first().map_or(Piano { dati: vec![0; n], scala: 65535 }, |p| Piano { dati: p.dati.clone(), scala: p.scala });
                v.push(copia);
            }
            v
        })
    }

    /// Le tessere posate al loro posto, una immagine per colore. Si decodificano a gruppi in parallelo
    /// e si posano in ordine, così dove si sovrappongono vince sempre la stessa.
    fn componi<T: Pixel>(&self, path: &Path, blocchi: &[&Blocco], tipo: Tipo) -> Result<Vec<Vec<T>>, String> {
        let (w, h) = (self.larghezza as usize, self.altezza as usize);
        let k = tipo.componenti();
        let mut out: Vec<Vec<T>> = (0..k).map(|_| vec![T::default(); w * h]).collect();
        let fili = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
        for gruppo in blocchi.chunks(fili * 2) {
            let decodificati: Vec<Result<Vec<Vec<T>>, String>> = if gruppo.len() == 1 {
                vec![decodifica_blocco::<T>(path, gruppo[0], tipo)]
            } else {
                std::thread::scope(|s| {
                    let ths: Vec<_> = gruppo.iter().map(|b| s.spawn(move || decodifica_blocco::<T>(path, b, tipo))).collect();
                    ths.into_iter().map(|t| t.join().unwrap_or_else(|_| Err("lettura interrotta".into()))).collect()
                })
            };
            for (b, d) in gruppo.iter().zip(decodificati) {
                let d = d?;
                for (o, p) in out.iter_mut().zip(&d) {
                    self.posa(o, p, b);
                }
            }
        }
        Ok(out)
    }

    /// Mette i pixel di una tessera nell'immagine, riducendoli se serve (media dei pixel che cadono
    /// nello stesso punto).
    fn posa<T: Pixel>(&self, out: &mut [T], p: &[T], b: &Blocco) {
        let (w, h) = (self.larghezza as i64, self.altezza as i64);
        let (sw, sh) = (b.sw as usize, b.sh as usize);
        let fattore = i64::from(self.livello * self.resto);
        if fattore == 1 && b.w == sw as i64 && b.h == sh as i64 {
            // Pixel per pixel, riga per riga.
            let (dx, dy) = (b.x - self.x0, b.y - self.y0);
            let x_da = (-dx).max(0);
            let x_a = (sw as i64).min(w - dx);
            if x_a <= x_da {
                return;
            }
            for r in 0..sh as i64 {
                let y = dy + r;
                if y < 0 || y >= h {
                    continue;
                }
                let o = (y * w + dx + x_da) as usize;
                let s = r as usize * sw + x_da as usize;
                let n = (x_a - x_da) as usize;
                out[o..o + n].copy_from_slice(&p[s..s + n]);
            }
            return;
        }
        // Ridotta: ogni pixel d'uscita copre `fattore` pixel pieni; la tessera ne copre b.w con sw pixel.
        let fx = sw as f64 / b.w as f64;
        let fy = sh as f64 / b.h as f64;
        let f = fattore as f64;
        let ox_da = ((b.x - self.x0) as f64 / f).floor().max(0.0) as i64;
        let ox_a = (((b.x + b.w - self.x0) as f64 / f).ceil() as i64).min(w);
        let oy_da = ((b.y - self.y0) as f64 / f).floor().max(0.0) as i64;
        let oy_a = (((b.y + b.h - self.y0) as f64 / f).ceil() as i64).min(h);
        // Per ogni colonna d'uscita, le colonne della tessera che ci cadono.
        let intervallo = |o: i64, inizio: i64, origine: i64, scala: f64, n: usize| -> (usize, usize) {
            let a = ((origine + o * fattore - inizio) as f64 * scala).floor().max(0.0) as usize;
            let b = (((origine + (o + 1) * fattore - inizio) as f64 * scala).ceil().max(0.0) as usize).min(n);
            (a.min(n), b.max(a.min(n)))
        };
        let colonne: Vec<(usize, usize)> = (ox_da..ox_a).map(|ox| intervallo(ox, b.x, self.x0, fx, sw)).collect();
        for oy in oy_da..oy_a {
            let (ra, rb) = intervallo(oy, b.y, self.y0, fy, sh);
            if rb <= ra {
                continue;
            }
            for (i, &(ca, cb)) in colonne.iter().enumerate() {
                if cb <= ca {
                    continue;
                }
                // Il pixel d'uscita è coperto solo in parte dalla tessera: la media di quella parte.
                let mut somma = 0.0f64;
                for r in ra..rb {
                    somma += p[r * sw + ca..r * sw + cb].iter().map(|v| v.a_f64()).sum::<f64>();
                }
                let n = ((rb - ra) * (cb - ca)) as f64;
                out[(oy * w + ox_da + i as i64) as usize] = T::da_f64(somma / n);
            }
        }
    }
}

/// Un tipo di campione: si somma per le medie e torna indietro arrotondato.
trait Pixel: Copy + Default + Send + Sync + 'static {
    fn a_f64(self) -> f64;
    fn da_f64(v: f64) -> Self;
    fn leggi(b: &[u8]) -> Self;
}

impl Pixel for u8 {
    fn a_f64(self) -> f64 {
        f64::from(self)
    }
    fn da_f64(v: f64) -> Self {
        v.round().clamp(0.0, 255.0) as u8
    }
    fn leggi(b: &[u8]) -> Self {
        b[0]
    }
}

impl Pixel for u16 {
    fn a_f64(self) -> f64 {
        f64::from(self)
    }
    fn da_f64(v: f64) -> Self {
        v.round().clamp(0.0, 65535.0) as u16
    }
    fn leggi(b: &[u8]) -> Self {
        u16::from_le_bytes([b[0], b[1]])
    }
}

impl Pixel for u32 {
    fn a_f64(self) -> f64 {
        f64::from(self)
    }
    fn da_f64(v: f64) -> Self {
        v.round().clamp(0.0, f64::from(u32::MAX)) as u32
    }
    fn leggi(b: &[u8]) -> Self {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}

impl Pixel for f32 {
    fn a_f64(self) -> f64 {
        f64::from(self)
    }
    fn da_f64(v: f64) -> Self {
        v as f32
    }
    fn leggi(b: &[u8]) -> Self {
        f32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}

/// I byte compressi di un sottoblocco.
fn dati_blocco(path: &Path, b: &Blocco) -> Result<Vec<u8>, String> {
    let mut f = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let lunghezza_file = f.get_ref().metadata().map_err(|e| e.to_string())?.len();
    let (id, _, _) = segmento(&mut f, b.pos)?;
    if nome_segmento(&id) != b"ZISRAWSUBBLOCK" {
        return Err("tessera del file CZI non trovata".into());
    }
    let t = leggi_a(&mut f, b.pos + 32, 16 + 32)?;
    let meta = i32_at(&t, 0).unwrap_or(0).max(0) as u64;
    let quanti = i64_at(&t, 8).unwrap_or(0).max(0) as u64;
    let voce = match &t[16..18] {
        b"DV" => 32 + 20 * i32_at(&t, 16 + 28).unwrap_or(0).clamp(0, 40) as u64,
        _ => 128,
    };
    let inizio = b.pos + 32 + (16 + voce).max(256) + meta;
    if inizio + quanti > lunghezza_file || quanti > 2 << 30 {
        return Err("file CZI troncato".into());
    }
    leggi_a(&mut f, inizio, quanti as usize)
}

/// I byte di una tessera decompressi, nell'ordine del tipo (BGR per i colori), larghi `sw`×`sh`.
fn decomprimi(dati: Vec<u8>, b: &Blocco, tipo: Tipo) -> Result<Vec<u8>, String> {
    let (sw, sh) = (b.sw as usize, b.sh as usize);
    let attesi = sw * sh * tipo.byte_pixel();
    let grezzi = match b.compressione {
        0 => dati,
        1 => jpeg(&dati, tipo, sw, sh)?,
        2 => weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8).decode(&dati).map_err(|e| format!("LZW: {e}"))?,
        4 => jpeg_xr(&dati, tipo, sw, sh)?,
        5 => zstd(&dati, attesi)?,
        6 => {
            // zstd1: una piccola intestazione (1 byte, o 3 se c'è la scelta «byte alti e bassi separati»).
            let (testa, alti_bassi) = match dati.first() {
                Some(1) => (1, false),
                Some(3) if dati.get(1) == Some(&1) && dati.len() >= 3 => (3, dati[2] & 1 == 1),
                _ => return Err("intestazione zstd della tessera non valida".into()),
            };
            let d = zstd(&dati[testa..], attesi)?;
            if alti_bassi && tipo.byte_campione() == 2 {
                // Prima tutti i byte bassi, poi tutti gli alti.
                let n = d.len() / 2;
                let mut v = Vec::with_capacity(n * 2);
                for i in 0..n {
                    v.push(d[i]);
                    v.push(d[n + i]);
                }
                v
            } else {
                d
            }
        }
        c if c >= 1000 => return Err(format!("compressione {c} di sistema non letta")),
        c if c >= 100 => return Err(format!("compressione {c} della fotocamera non letta")),
        c => return Err(format!("compressione {c} non letta")),
    };
    // Come libCZI: se i dati sono di più si taglia, se sono di meno il resto è nero.
    let mut g = grezzi;
    g.resize(attesi, 0);
    Ok(g)
}

fn zstd(dati: &[u8], attesi: usize) -> Result<Vec<u8>, String> {
    let mut d = ruzstd::decoding::StreamingDecoder::new(dati).map_err(|e| format!("zstd: {e}"))?;
    let mut out = Vec::with_capacity(attesi);
    // Mai più di quello che serve (più un byte, per accorgersi del troppo).
    (&mut d).take(attesi as u64 + 1).read_to_end(&mut out).map_err(|e| format!("zstd: {e}"))?;
    Ok(out)
}

fn jpeg(dati: &[u8], tipo: Tipo, sw: usize, sh: usize) -> Result<Vec<u8>, String> {
    use zune_core::colorspace::ColorSpace;
    let grigio = tipo.componenti() == 1;
    if tipo.campione() != Campione::U8 {
        return Err("JPEG a più di 8 bit nel CZI non letto".into());
    }
    let mut dec = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(dati));
    dec.decode_headers().map_err(|e| format!("JPEG: {e:?}"))?;
    let (w, h) = dec.dimensions().ok_or("JPEG senza misure")?;
    let spazio = if grigio { ColorSpace::Luma } else { ColorSpace::RGB };
    dec.set_options(zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(spazio));
    let px = dec.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let n = if grigio { 1 } else { 3 };
    let mut out = vec![0u8; sw * sh * tipo.salvati()];
    for y in 0..h.min(sh) {
        for x in 0..w.min(sw) {
            let s = (y * w + x) * n;
            let o = (y * sw + x) * tipo.salvati();
            if grigio {
                out[o] = px[s];
            } else {
                // Nel CZI i colori vanno in ordine BGR.
                out[o] = px[s + 2];
                out[o + 1] = px[s + 1];
                out[o + 2] = px[s];
            }
        }
    }
    Ok(out)
}

fn jpeg_xr(dati: &[u8], tipo: Tipo, sw: usize, sh: usize) -> Result<Vec<u8>, String> {
    use openreadout_jpegxr::SampleType;
    let massimo = (sw * sh * 16).max(1 << 20);
    let im = openreadout_jpegxr::decode(dati, massimo).map_err(|e| format!("JPEG XR: {e}"))?;
    let (w, h, n) = (im.width as usize, im.height as usize, im.channels as usize);
    let campione = match im.sample {
        SampleType::U8 => Campione::U8,
        SampleType::U16 => Campione::U16,
        SampleType::F32 => Campione::F32,
        s => return Err(format!("JPEG XR con campioni {s:?} non letto")),
    };
    if campione != tipo.campione() || n < tipo.componenti() {
        return Err("JPEG XR diverso dal tipo della tessera".into());
    }
    let bc = tipo.byte_campione();
    let bp = tipo.byte_pixel();
    let mut out = vec![0u8; sw * sh * bp];
    for y in 0..h.min(sh) {
        for x in 0..w.min(sw) {
            let s = (y * w + x) * n * bc;
            let o = (y * sw + x) * bp;
            if tipo.componenti() == 1 {
                out[o..o + bc].copy_from_slice(&im.data[s..s + bc]);
            } else {
                for k in 0..3 {
                    // Nel CZI i colori vanno in ordine BGR.
                    let da = if im.bgr { k } else { 2 - k };
                    out[o + k * bc..o + (k + 1) * bc].copy_from_slice(&im.data[s + da * bc..s + (da + 1) * bc]);
                }
            }
        }
    }
    Ok(out)
}

/// Una tessera letta, decompressa e divisa per colore (R, G, B negli RGB).
fn decodifica_blocco<T: Pixel>(path: &Path, b: &Blocco, tipo_scena: Tipo) -> Result<Vec<Vec<T>>, String> {
    let tipo = Tipo::da(b.tipo)?;
    if tipo.campione() != tipo_scena.campione() || tipo.componenti() != tipo_scena.componenti() {
        return Err("tessere di tipo diverso nello stesso canale".into());
    }
    let grezzi = decomprimi(dati_blocco(path, b)?, b, tipo)?;
    let bc = tipo.byte_campione();
    let bp = tipo.byte_pixel();
    let n = b.sw as usize * b.sh as usize;
    let k = tipo.componenti();
    // BGR → R, G, B.
    Ok((0..k)
        .map(|c| {
            let da = if k == 3 { 2 - c } else { 0 };
            (0..n).map(|i| T::leggi(&grezzi[i * bp + da * bc..])).collect()
        })
        .collect())
}

#[cfg(test)]
pub(crate) mod prove {
    use super::*;

    /// Scrive un CZI con le tessere date: (x, y, w, h, sw, sh, dims extra, tipo, compressione, dati).
    pub struct Tessera {
        pub x: i32,
        pub y: i32,
        pub w: i32,
        pub h: i32,
        pub sw: i32,
        pub sh: i32,
        pub dims: Vec<(u8, i32)>,
        pub tipo: i32,
        pub compressione: i32,
        pub dati: Vec<u8>,
    }

    fn voce_dv(t: &Tessera, pos: u64) -> Vec<u8> {
        let mut dims: Vec<(u8, i32, i32, i32)> = vec![(b'X', t.x, t.w, t.sw), (b'Y', t.y, t.h, t.sh)];
        dims.extend(t.dims.iter().map(|(n, v)| (*n, *v, 1, 1)));
        let mut v = Vec::new();
        v.extend(b"DV");
        v.extend(t.tipo.to_le_bytes());
        v.extend((pos as i64).to_le_bytes());
        v.extend(0i32.to_le_bytes());
        v.extend(t.compressione.to_le_bytes());
        v.extend([0u8; 6]);
        v.extend((dims.len() as i32).to_le_bytes());
        for (n, a, s, st) in dims {
            v.extend([n, 0, 0, 0]);
            v.extend(a.to_le_bytes());
            v.extend(s.to_le_bytes());
            v.extend(0f32.to_le_bytes());
            v.extend(st.to_le_bytes());
        }
        v
    }

    fn segmento(id: &str, dati: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let mut nome = [0u8; 16];
        nome[..id.len()].copy_from_slice(id.as_bytes());
        v.extend(nome);
        let occupato = dati.len().div_ceil(32) * 32;
        v.extend((occupato as i64).to_le_bytes());
        v.extend((dati.len() as i64).to_le_bytes());
        v.extend(dati);
        v.resize(32 + occupato, 0);
        v
    }

    pub fn scrivi(path: &Path, tessere: &[Tessera], xml: &str) {
        let mut file = segmento("ZISRAWFILE", &[0u8; 512]);
        let mut voci = Vec::new();
        for t in tessere {
            let pos = file.len() as u64;
            let voce = voce_dv(t, pos);
            let mut d = Vec::new();
            d.extend(0i32.to_le_bytes());
            d.extend(0i32.to_le_bytes());
            d.extend((t.dati.len() as i64).to_le_bytes());
            d.extend(&voce);
            d.resize((16 + voce.len()).max(256), 0);
            d.extend(&t.dati);
            file.extend(segmento("ZISRAWSUBBLOCK", &d));
            voci.push(voce);
        }
        let pos_meta = file.len() as u64;
        let mut m = Vec::new();
        m.extend((xml.len() as i32).to_le_bytes());
        m.extend(0i32.to_le_bytes());
        m.resize(256, 0);
        m.extend(xml.as_bytes());
        file.extend(segmento("ZISRAWMETADATA", &m));
        let pos_dir = file.len() as u64;
        let mut d = Vec::new();
        d.extend((voci.len() as i32).to_le_bytes());
        d.resize(128, 0);
        for v in &voci {
            d.extend(v);
        }
        file.extend(segmento("ZISRAWDIRECTORY", &d));
        // In testa: versione 1.0, indirizzi della directory e dei metadati.
        file[32..36].copy_from_slice(&1i32.to_le_bytes());
        file[32 + 52..32 + 60].copy_from_slice(&(pos_dir as i64).to_le_bytes());
        file[32 + 60..32 + 68].copy_from_slice(&(pos_meta as i64).to_le_bytes());
        std::fs::write(path, file).unwrap();
    }

    fn u16le(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    pub const XML: &str = r##"<ImageDocument><Metadata><Information><Image><ComponentBitCount>12</ComponentBitCount><Dimensions><Channels><Channel Id="Channel:0" Name="DAPI"><Color>#FF0000FF</Color></Channel><Channel Id="Channel:1" Name="EGFP"><Color>#FF00FF5B</Color></Channel></Channels><S><Scenes><Scene Index="0" Name="P1"/><Scene Index="1" Name="P2"/></Scenes></S></Dimensions></Image><Instrument><Objectives><Objective Id="Objective:1" Name="Plan-Apochromat  20x/0.8 M27"/></Objectives></Instrument></Information><Scaling><Items><Distance Id="X"><Value>3.25E-07</Value></Distance><Distance Id="Y"><Value>3.25E-07</Value></Distance><Distance Id="Z"><Value>1.5E-06</Value></Distance></Items></Scaling></Metadata></ImageDocument>"##;

    /// Un CZI con due scene. P1: 16 bit, 2 canali × 2 piani Z, 8×4 in due tessere 4×4 affiancate
    /// (la seconda compressa zstd0, la prima zstd1 con i byte separati), valore = c·1000 + z·100 +
    /// y·10 + x. P2: una tessera 3×2 non compressa che parte da (100, 50), valore 7.
    pub fn czi(dir: &Path) -> std::path::PathBuf {
        let mut tessere = Vec::new();
        for c in 0..2i32 {
            for z in 0..2i32 {
                for k in 0..2i32 {
                    let v: Vec<u16> = (0..4).flat_map(|y| (0..4).map(move |x| (c * 1000 + z * 100 + y * 10 + x + k * 4) as u16)).collect();
                    let grezzi = u16le(&v);
                    let dati = if k == 0 {
                        let (bassi, alti): (Vec<u8>, Vec<u8>) = grezzi.chunks(2).map(|p| (p[0], p[1])).unzip();
                        let mut d = vec![3u8, 1, 1];
                        d.extend(ruzstd::encoding::compress_to_vec([bassi, alti].concat().as_slice(), ruzstd::encoding::CompressionLevel::Fastest));
                        d
                    } else {
                        ruzstd::encoding::compress_to_vec(grezzi.as_slice(), ruzstd::encoding::CompressionLevel::Fastest)
                    };
                    tessere.push(Tessera { x: 1000 + k * 4, y: 2000, w: 4, h: 4, sw: 4, sh: 4, dims: vec![(b'C', c), (b'Z', z), (b'S', 0), (b'T', 0), (b'M', k)], tipo: 1, compressione: if k == 0 { 6 } else { 5 }, dati });
                }
            }
        }
        for c in 0..2i32 {
            for z in 0..2i32 {
                tessere.push(Tessera { x: 100, y: 50, w: 3, h: 2, sw: 3, sh: 2, dims: vec![(b'C', c), (b'Z', z), (b'S', 1), (b'T', 0)], tipo: 1, compressione: 0, dati: u16le(&[7; 6]) });
            }
        }
        let p = dir.join("prova.czi");
        scrivi(&p, &tessere, XML);
        p
    }

    fn cartella(nome: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cedro-czi-{nome}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn un_czi_si_legge_con_scene_tessere_e_metadati() {
        let dir = cartella("base");
        let p = czi(&dir);
        assert!(riconosci(&std::fs::read(&p).unwrap()[..16]));
        let f = Czi::open(&p).unwrap();
        assert!(f.avviso.is_none(), "{:?}", f.avviso);
        assert_eq!(f.scene.iter().map(|s| s.nome.as_str()).collect::<Vec<_>>(), ["P1", "P2"]);
        let a = &f.scene[0];
        assert_eq!((a.larghezza, a.altezza, a.z, a.t(), a.bit, a.rgb), (8, 4, 2, 1, 12, false));
        assert_eq!(a.canali, [("DAPI".to_string(), Some((0, 0, 255))), ("EGFP".to_string(), Some((0, 255, 91)))]);
        assert!((a.um_per_px.unwrap() - 0.325).abs() < 1e-9);
        assert!((a.um_z.unwrap() - 1.5).abs() < 1e-9);
        assert_eq!(a.obiettivo.as_deref(), Some("Plan-Apochromat 20x/0.8 M27"));
        let pz = a.piano(&p, 1, 1, 0).unwrap();
        assert_eq!(pz.scala, 65535);
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(pz.dati[y * 8 + x], (1100 + y * 10 + x) as u16, "({x}, {y})");
            }
        }
        let b = &f.scene[1];
        assert_eq!((b.larghezza, b.altezza), (3, 2));
        assert_eq!(b.piano(&p, 0, 1, 0).unwrap().dati, vec![7; 6]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_rgb_si_legge_in_ordine_r_g_b() {
        let dir = cartella("rgb");
        let p = dir.join("rgb.czi");
        // 2×1 BGR24: (B, G, R) = (10, 20, 30) e (40, 50, 60).
        scrivi(&p, &[Tessera { x: 0, y: 0, w: 2, h: 1, sw: 2, sh: 1, dims: vec![(b'C', 0)], tipo: 3, compressione: 0, dati: vec![10, 20, 30, 40, 50, 60] }], "<ImageDocument/>");
        let f = Czi::open(&p).unwrap();
        let s = &f.scene[0];
        assert!(s.rgb);
        assert_eq!(s.canali.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["R", "G", "B"]);
        let v: Vec<Vec<u16>> = s.piani(&p, 0, 0, 0).unwrap().into_iter().map(|p| p.dati).collect();
        assert_eq!(v, [vec![30, 60], vec![20, 50], vec![10, 40]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_piramide_si_usa_quando_l_immagine_e_troppo_grande() {
        let dir = cartella("piramide");
        let p = dir.join("vetrino.czi");
        // 400×400 con i livelli 1:2, 1:4 e 1:8, ognuno di un valore suo. Il livello pieno non si legge
        // mai: dichiara i suoi pixel ma non li ha.
        let t = |sw: i32, v: u8| Tessera { x: 0, y: 0, w: 400, h: 400, sw, sh: sw, dims: vec![(b'C', 0)], tipo: 0, compressione: 0, dati: if sw == 400 { vec![] } else { vec![v; (sw * sw) as usize] } };
        scrivi(&p, &[t(400, 1), t(200, 2), t(100, 4), t(50, 8)], "<ImageDocument/>");
        // Al massimo 10.000 pixel: serve 1:4, e c'è.
        let s = &Czi::open_con(&p, 10_000).unwrap().scene[0];
        assert_eq!((s.livello, s.resto, s.ridotta), (4, 1, Some((4, 400, 400))));
        assert_eq!((s.larghezza, s.altezza), (100, 100));
        assert!(s.piano(&p, 0, 0, 0).unwrap().dati.iter().all(|x| *x == 4));
        // Al massimo 20.000 pixel: serve 1:3, che non c'è: livello 1:2 ridotto ancora di 2.
        let s = &Czi::open_con(&p, 20_000).unwrap().scene[0];
        assert_eq!((s.livello, s.resto, s.ridotta), (2, 2, Some((4, 400, 400))));
        assert!(s.piano(&p, 0, 0, 0).unwrap().dati.iter().all(|x| *x == 2));
        // Senza limite: il livello pieno, che qui è tagliato (nero, come fa libCZI).
        let s = &Czi::open(&p).unwrap().scene[0];
        assert_eq!((s.livello, s.ridotta, s.larghezza), (1, None, 400));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ridotta_fa_la_media_dei_pixel() {
        let dir = cartella("media");
        let p = dir.join("media.czi");
        // 110×110 Gray8 a righe alterne 0 e 200, in 11 strisce: con 5.000 pixel al massimo si riduce
        // di 2 e ogni pixel vale la media (100).
        let mut tessere = Vec::new();
        for k in 0..11 {
            let dati: Vec<u8> = (0..10).flat_map(|y| std::iter::repeat(if y % 2 == 0 { 0 } else { 200 }).take(110)).collect();
            tessere.push(Tessera { x: 0, y: k * 10, w: 110, h: 10, sw: 110, sh: 10, dims: vec![(b'M', k)], tipo: 0, compressione: 0, dati });
        }
        scrivi(&p, &tessere, "<ImageDocument/>");
        let s = &Czi::open_con(&p, 5_000).unwrap().scene[0];
        assert_eq!(s.ridotta, Some((2, 110, 110)));
        assert_eq!((s.larghezza, s.altezza), (55, 55));
        let v = s.piano(&p, 0, 0, 0).unwrap();
        assert!(v.dati.iter().all(|x| *x == 100), "{:?}", &v.dati[..10]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn firma(v: &[u16]) -> u64 {
        v.iter().enumerate().map(|(i, x)| u64::from(*x) * (i as u64 % 251 + 1)).sum()
    }

    /// Tessere JPEG XR scritte con jxrlib (imagecodecs), a qualità 85: i pixel devono essere quelli
    /// che dà jxrlib, uno per uno (qui la loro firma).
    #[test]
    fn jpeg_xr_come_jxrlib() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/micro");
        let p = dir.join("jxr-grigio16.czi");
        let s = &Czi::open(&p).unwrap().scene[0];
        assert_eq!((s.larghezza, s.altezza, s.bit), (200, 96, 16));
        let v = s.piano(&p, 0, 0, 0).unwrap().dati;
        assert_eq!(v[..4], [2780, 2885, 2708, 2994]);
        assert_eq!(firma(&v), 4_592_939_041);
        let p = dir.join("jxr-rgb8.czi");
        let s = &Czi::open(&p).unwrap().scene[0];
        assert!(s.rgb);
        let v: Vec<u16> = s.piani(&p, 0, 0, 0).unwrap().into_iter().flat_map(|p| p.dati).collect();
        assert_eq!(v[..4], [160, 182, 174, 188]);
        assert_eq!(firma(&v), 900_404_798);
    }

    #[test]
    fn un_file_rovinato_da_un_errore_e_non_si_blocca() {
        let dir = cartella("rovinato");
        let p = czi(&dir);
        let b = std::fs::read(&p).unwrap();
        // Tagliato a metà: senza directory si cercano i sottoblocchi uno per uno.
        std::fs::write(&p, &b[..b.len() / 2]).unwrap();
        let _ = Czi::open(&p).map(|f| f.scene.iter().map(|s| s.piano(&p, 0, 0, 0).map(|x| x.dati.len())).collect::<Vec<_>>());
        // Byte a caso: mai un panico.
        let mut x = 12345u64;
        for _ in 0..100 {
            let mut c = b.clone();
            for _ in 0..8 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let i = (x >> 33) as usize % c.len();
                c[i] = (x >> 17) as u8;
            }
            std::fs::write(&p, &c).unwrap();
            if let Ok(f) = Czi::open_con(&p, 1 << 20) {
                for s in &f.scene {
                    for k in 0..s.canali.len() {
                        let _ = s.piano(&p, k, 0, 0);
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prova su un file vero (`CEDRO_CZI=file.czi`): scrive i pixel di ogni scena in ordine
    /// T, Z, C, Y, X in `file.czi.<n>.u16` e stampa le misure.
    #[test]
    #[ignore]
    fn file_vero() {
        let Ok(p) = std::env::var("CEDRO_CZI") else { return };
        let p = std::path::PathBuf::from(p);
        let f = Czi::open(&p).unwrap();
        println!("avviso: {:?}", f.avviso);
        for (k, s) in f.scene.iter().enumerate() {
            println!("{k}: «{}» {}x{} z={} t={} altri={:?} canali={:?} bit={} um={:?} umz={:?} obiettivo={:?} ridotta={:?}", s.nome, s.larghezza, s.altezza, s.z, s.t(), s.altri, s.canali, s.bit, s.um_per_px, s.um_z, s.obiettivo, s.ridotta);
            let mut out = Vec::new();
            for t in 0..s.t() {
                for z in 0..s.z {
                    for c in 0..s.canali.len() {
                        out.extend(s.piano(&p, c, z, t).unwrap().dati.iter().flat_map(|v| v.to_le_bytes()));
                    }
                }
            }
            std::fs::write(format!("{}.{k}.u16", p.display()), out).unwrap();
        }
    }
}
