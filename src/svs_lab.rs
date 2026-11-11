//! Il laboratorio sui vetrini digitali (.svs): la copia anonima del vetrino (via etichetta e foto del
//! vetrino, via nome, data, ora e utente dalle descrizioni), le tessere per l'intelligenza artificiale,
//! la zona per la conta mitotica (2,37 mm², dieci campi) e il pacchetto per un consulto.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};

use resvg::usvg;

use crate::svs::Slide;

// ---------------------------------------------------------------------------------------------
// Copia anonima

/// Le voci della descrizione Aperio che possono dire di chi è il vetrino: si cancellano.
const VOCI_PERSONALI: &[&str] = &["Filename", "Date", "Time", "User", "Barcode", "Patient", "Title", "Case", "Accession", "Slide", "Label"];

struct Voce {
    tag: u16,
    tipo: u16,
    n: u64,
    /// Dove sta il valore (dentro la voce o, se non ci sta, l'indirizzo letto da lì).
    pos_valore: u64,
}

struct Pagina {
    /// Dove comincia la pagina nel file.
    indirizzo: u64,
    /// Dove sta il suo puntatore alla pagina dopo.
    pos_prossima: u64,
    voci: Vec<Voce>,
}

struct Tiff {
    le: bool,
    big: bool,
}

impl Tiff {
    fn u16(&self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        if self.le { u16::from_le_bytes(a) } else { u16::from_be_bytes(a) }
    }
    fn u32(&self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        if self.le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
    }
    fn u64(&self, b: &[u8]) -> u64 {
        let a: [u8; 8] = b[..8].try_into().unwrap_or([0; 8]);
        if self.le { u64::from_le_bytes(a) } else { u64::from_be_bytes(a) }
    }
    fn off(&self, b: &[u8]) -> u64 {
        if self.big { self.u64(b) } else { self.u32(b) as u64 }
    }
    fn scrivi_off(&self, v: u64) -> Vec<u8> {
        match (self.big, self.le) {
            (true, true) => v.to_le_bytes().to_vec(),
            (true, false) => v.to_be_bytes().to_vec(),
            (false, true) => (v as u32).to_le_bytes().to_vec(),
            (false, false) => (v as u32).to_be_bytes().to_vec(),
        }
    }
}

fn leggi_a(f: &mut File, pos: u64, n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
    f.read_exact(&mut b).map_err(|e| format!("TIFF troncato: {e}"))?;
    Ok(b)
}

fn scrivi_a(f: &mut File, pos: u64, b: &[u8]) -> Result<(), String> {
    f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
    f.write_all(b).map_err(|e| e.to_string())
}

fn misura_tipo(tipo: u16) -> u64 {
    match tipo {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 1,
    }
}

fn pagine(f: &mut File) -> Result<(Tiff, Vec<Pagina>), String> {
    let h = leggi_a(f, 0, 16)?;
    let le = match &h[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err("non è un TIFF".into()),
    };
    let t0 = Tiff { le, big: false };
    let big = match t0.u16(&h[2..4]) {
        42 => false,
        43 => true,
        _ => return Err("non è un TIFF".into()),
    };
    let t = Tiff { le, big };
    let mut pos = if big { t.u64(&h[8..16]) } else { t.u32(&h[4..8]) as u64 };
    let (dim_n, dim_voce, dim_off) = if big { (8usize, 20u64, 8usize) } else { (2usize, 12u64, 4usize) };
    let mut out = Vec::new();
    while pos != 0 && out.len() < 10_000 {
        let nb = leggi_a(f, pos, dim_n)?;
        let n = if big { t.u64(&nb) } else { t.u16(&nb) as u64 };
        let corpo = leggi_a(f, pos + dim_n as u64, (n * dim_voce) as usize + dim_off)?;
        let mut voci = Vec::new();
        for i in 0..n as usize {
            let e = &corpo[i * dim_voce as usize..];
            let tag = t.u16(&e[0..2]);
            let tipo = t.u16(&e[2..4]);
            let cnt = if big { t.u64(&e[4..12]) } else { t.u32(&e[4..8]) as u64 };
            let campo = pos + dim_n as u64 + i as u64 * dim_voce + if big { 12 } else { 8 };
            let dentro = misura_tipo(tipo) * cnt <= dim_off as u64;
            let pos_valore = if dentro { campo } else { t.off(&e[if big { 12 } else { 8 }..]) };
            voci.push(Voce { tag, tipo, n: cnt, pos_valore });
        }
        let prossima = t.off(&corpo[(n * dim_voce) as usize..]);
        out.push(Pagina { indirizzo: pos, pos_prossima: pos + dim_n as u64 + n * dim_voce, voci });
        pos = prossima;
    }
    Ok((t, out))
}

fn interi(f: &mut File, t: &Tiff, v: &Voce) -> Result<Vec<u64>, String> {
    let m = misura_tipo(v.tipo);
    let b = leggi_a(f, v.pos_valore, (m * v.n) as usize)?;
    Ok((0..v.n as usize)
        .map(|i| {
            let s = &b[i * m as usize..];
            match v.tipo {
                3 => t.u16(s) as u64,
                16 | 18 => t.u64(s),
                _ => t.u32(s) as u64,
            }
        })
        .collect())
}

/// La descrizione con i valori delle voci personali sostituiti da spazi (stessa lunghezza, così il
/// file non si sposta di un byte).
fn pulisci_descrizione(desc: &str) -> String {
    desc.split('|')
        .map(|parte| match parte.split_once('=') {
            Some((k, v)) if VOCI_PERSONALI.iter().any(|p| k.trim().eq_ignore_ascii_case(p)) => format!("{k}={}", " ".repeat(v.len())),
            _ => parte.to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Rende anonimo, sul posto, il vetrino `path` (una copia: l'originale non si tocca mai): toglie
/// l'etichetta e la foto del vetrino intero (spesso con nome e codice del caso) azzerandone i pixel,
/// e cancella nome del file, data, ora, utente e codici dalle descrizioni. Dice cosa ha tolto.
pub fn anonimizza_sul_posto(path: &Path) -> Result<Vec<&'static str>, String> {
    let mut f = OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let (t, pag) = pagine(&mut f)?;
    let mut tolti = Vec::new();
    let mut via = Vec::new();
    for (i, p) in pag.iter().enumerate() {
        let desc_voce = p.voci.iter().find(|v| v.tag == 270);
        let desc = match desc_voce {
            Some(v) => String::from_utf8_lossy(&leggi_a(&mut f, v.pos_valore, v.n as usize)?).into_owned(),
            None => String::new(),
        };
        let tessere = p.voci.iter().any(|v| v.tag == 322);
        let etichetta = !tessere && i > 0 && desc.contains("label");
        let macro_ = !tessere && i > 0 && desc.contains("macro");
        if etichetta || macro_ {
            tolti.push(if etichetta { "etichetta" } else { "foto del vetrino" });
            // I pixel diventano zeri: anche chi legge il file a mano non li ritrova.
            let off = p.voci.iter().find(|v| v.tag == 273 || v.tag == 324);
            let cnt = p.voci.iter().find(|v| v.tag == 279 || v.tag == 325);
            if let (Some(o), Some(c)) = (off, cnt) {
                let (o, c) = (interi(&mut f, &t, o)?, interi(&mut f, &t, c)?);
                for (o, c) in o.iter().zip(&c) {
                    let mut resto = *c;
                    let mut pos = *o;
                    while resto > 0 {
                        let n = resto.min(1 << 20);
                        scrivi_a(&mut f, pos, &vec![0u8; n as usize])?;
                        pos += n;
                        resto -= n;
                    }
                }
            }
            via.push(i);
            continue;
        }
        if let Some(v) = desc_voce {
            let pulita = pulisci_descrizione(&desc);
            if pulita != desc && pulita.len() == desc.len() && desc.len() == v.n as usize {
                scrivi_a(&mut f, v.pos_valore, pulita.as_bytes())?;
                if !tolti.contains(&"nome, data e utente dalle descrizioni") {
                    tolti.push("nome, data e utente dalle descrizioni");
                }
            }
        }
        // La data della scansione (DateTime) diventa spazi.
        if let Some(v) = p.voci.iter().find(|v| v.tag == 306 && v.tipo == 2 && v.n > 1) {
            scrivi_a(&mut f, v.pos_valore, &vec![b' '; v.n as usize - 1])?;
        }
    }
    // Le pagine tolte escono dalla catena: ogni pagina rimasta punta alla prossima rimasta.
    let buone: Vec<usize> = (0..pag.len()).filter(|i| !via.contains(i)).collect();
    for (k, &i) in buone.iter().enumerate() {
        let dopo = buone.get(k + 1).map(|&j| pag[j].indirizzo).unwrap_or(0);
        scrivi_a(&mut f, pag[i].pos_prossima, &t.scrivi_off(dopo))?;
    }
    f.flush().map_err(|e| e.to_string())?;
    Ok(tolti)
}


/// La copia anonima del vetrino in `out` (l'originale resta com'era).
pub fn copia_anonima(path: &Path, out: &Path) -> Result<Vec<&'static str>, String> {
    fs::copy(path, out).map_err(|e| e.to_string())?;
    anonimizza_sul_posto(out).inspect_err(|_| {
        let _ = fs::remove_file(out);
    })
}

// ---------------------------------------------------------------------------------------------
// Tessere per l'intelligenza artificiale

/// Le scelte delle tessere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tessere {
    /// Il lato di una tessera in pixel (256 o 512).
    pub lato: u32,
    /// L'ingrandimento voluto (20 = 0,5 µm/pixel, 10 = 1 µm/pixel); 0 = come scansionato.
    pub ingrandimento: u32,
    /// Solo le tessere con almeno metà di tessuto (niente vetro bianco).
    pub solo_tessuto: bool,
    /// JPEG (più leggere) invece di PNG.
    pub jpeg: bool,
}

impl Default for Tessere {
    fn default() -> Self {
        Tessere { lato: 256, ingrandimento: 20, solo_tessuto: true, jpeg: false }
    }
}

/// Quanto di un'immagine è tessuto: pixel colorati (l'ematossilina e l'eosina lo sono, il vetro
/// e la polvere no) e non bianchi.
fn tessuto(img: &[u8]) -> f32 {
    let n = img.len() / 3;
    if n == 0 {
        return 0.0;
    }
    let mut t = 0usize;
    for p in img.chunks_exact(3) {
        let (mx, mn) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        if mx - mn >= 20 && mn < 230 {
            t += 1;
        }
    }
    t as f32 / n as f32
}

/// Il livello da leggere e quanto rimpicciolire per avere i micron per pixel voluti.
fn livello_per(s: &Slide, um_voluti: Option<f64>) -> (usize, f64) {
    let (Some(mpp), Some(voluti)) = (s.mpp.map(f64::from), um_voluti) else { return (0, 1.0) };
    let pieno = s.full().width as f64;
    // Il livello più piccolo che è ancora almeno fine quanto serve.
    let mut scelto = (0usize, voluti / mpp);
    for (i, l) in s.levels.iter().enumerate() {
        let um = mpp * pieno / l.width as f64;
        if um <= voluti * 1.02 {
            scelto = (i, (voluti / um).max(1.0));
        }
    }
    scelto
}

/// Fatto delle tessere.
pub struct TessereFatte {
    pub scritte: usize,
    pub scartate: usize,
    pub um_per_px: Option<f64>,
}

/// Taglia il vetrino in tessere `lato`×`lato` dentro la cartella `dir`, con un `tessere.csv` (nome,
/// x e y nel livello pieno, micron per pixel, quanto tessuto). Si legge una fila alla volta.
pub fn tessere(path: &Path, dir: &Path, base: &str, o: &Tessere, avanzamento: &dyn Fn(f32)) -> Result<TessereFatte, String> {
    let s = Slide::open(path)?;
    let voluti = (o.ingrandimento > 0).then(|| 10.0 / o.ingrandimento as f64);
    if voluti.is_some() && s.mpp.is_none() {
        return Err("il vetrino non dice quanto è grande un pixel: scegli «Come scansionato»".into());
    }
    let (livello, scala) = livello_per(&s, voluti);
    let l = s.levels[livello].clone();
    let sorgente = ((o.lato as f64 * scala).round() as u32).max(1);
    let giu = s.full().width as f64 / l.width as f64;
    let um = s.mpp.map(|m| m as f64 * giu * scala);
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut csv = String::from("file;x;y;lato_px;um_per_px;tessuto\n");
    let (mut scritte, mut scartate) = (0usize, 0usize);
    let colonne = l.width / sorgente;
    let righe = l.height / sorgente;
    if colonne == 0 || righe == 0 {
        return Err("il vetrino è più piccolo di una tessera".into());
    }
    let ext = if o.jpeg { "jpg" } else { "png" };
    let fila = sorgente as usize * l.width as usize * 3;
    let mut banda: Vec<u8> = Vec::with_capacity(fila);
    let mut riga_banda = 0u32;
    let w = l.width as usize * 3;
    let mut errore: Option<String> = None;
    s.read_rows(path, livello, |_, rows| {
        let mut rows = rows;
        while !rows.is_empty() && riga_banda < righe {
            let manca = fila - banda.len();
            let prendi = manca.min(rows.len());
            banda.extend_from_slice(&rows[..prendi]);
            rows = &rows[prendi..];
            if banda.len() == fila {
                // Una fila di tessere: si scrivono in parallelo.
                let y0 = riga_banda * sorgente;
                let fatte: Vec<Result<Option<(String, f32)>, String>> = std::thread::scope(|sc| {
                    let lavori = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(colonne as usize).max(1);
                    let per = (colonne as usize).div_ceil(lavori);
                    let banda = &banda;
                    let hs: Vec<_> = (0..lavori)
                        .map(|k| {
                            sc.spawn(move || {
                                let mut v = Vec::new();
                                for c in (k * per)..((k + 1) * per).min(colonne as usize) {
                                    let x0 = c * sorgente as usize;
                                    let mut px = Vec::with_capacity(sorgente as usize * sorgente as usize * 3);
                                    for r in 0..sorgente as usize {
                                        let a = r * w + x0 * 3;
                                        px.extend_from_slice(&banda[a..a + sorgente as usize * 3]);
                                    }
                                    let t = tessuto(&px);
                                    if o.solo_tessuto && t < 0.5 {
                                        v.push(Ok(None));
                                        continue;
                                    }
                                    let mut img = RgbImage::from_raw(sorgente, sorgente, px).expect("misure");
                                    if sorgente != o.lato {
                                        img = image::imageops::resize(&img, o.lato, o.lato, image::imageops::FilterType::Triangle);
                                    }
                                    let (xp, yp) = ((x0 as f64 * giu).round() as u64, (y0 as f64 * giu).round() as u64);
                                    let nome = format!("{base}_x{xp}_y{yp}.{ext}");
                                    let r = if o.jpeg {
                                        let mut b = Vec::new();
                                        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 90)
                                            .encode_image(&img)
                                            .map_err(|e| e.to_string())
                                            .and_then(|_| fs::write(dir.join(&nome), b).map_err(|e| e.to_string()))
                                    } else {
                                        img.save_with_format(dir.join(&nome), image::ImageFormat::Png).map_err(|e| e.to_string())
                                    };
                                    v.push(r.map(|_| Some((format!("{nome};{xp};{yp}"), t))));
                                }
                                v
                            })
                        })
                        .collect();
                    hs.into_iter().flat_map(|h| h.join().unwrap_or_else(|_| vec![Err("tessera non scritta".into())])).collect()
                });
                for r in fatte {
                    match r {
                        Ok(Some((riga, t))) => {
                            scritte += 1;
                            let um = um.map(|u| format!("{u:.4}")).unwrap_or_default();
                            csv.push_str(&format!("{riga};{};{um};{:.2}\n", o.lato, t));
                        }
                        Ok(None) => scartate += 1,
                        Err(e) => {
                            errore.get_or_insert(e);
                        }
                    }
                }
                if let Some(e) = &errore {
                    return Err(e.clone());
                }
                banda.clear();
                riga_banda += 1;
                avanzamento(riga_banda as f32 / righe as f32 * 0.98);
            }
        }
        Ok(())
    })?;
    if let Some(e) = errore {
        return Err(e);
    }
    fs::write(dir.join("tessere.csv"), csv).map_err(|e| e.to_string())?;
    avanzamento(1.0);
    Ok(TessereFatte { scritte, scartate, um_per_px: um })
}

// ---------------------------------------------------------------------------------------------
// Conta mitotica

/// L'area della conta mitotica in veterinaria (Meuten 2016): 2,37 mm², dieci campi a 400x con un
/// oculare da 22 mm. Qui dieci rettangoli in 2 file da 5, di 0,237 mm² ciascuno.
pub const AREA_MM2: f64 = 2.37;
const CAMPO_UM: f64 = 486.8; // √0,237 mm² in micron

/// Quanta ematossilina c'è in un pixel: quanto è «nucleare» (deconvoluzione
/// del colore di Ruifrok, la componente dell'ematossilina).
fn ematossilina(p: &[u8]) -> f32 {
    // Le densità ottiche dell'ematossilina e dell'eosina (Ruifrok e Johnston 2001), il terzo vettore
    // perpendicolare (H, E, H×E normalizzati); qui la matrice inversa, calcolata a parte.
    const INV: [[f32; 3]; 3] = [[1.4891, -1.0823, -0.3319], [-0.1626, 1.1179, -0.0756], [0.5125, -0.2921, 0.9403]];
    let od = |v: u8| -(((v as f32) + 1.0) / 256.0).ln();
    let (r, g, b) = (od(p[0]), od(p[1]), od(p[2]));
    (INV[0][0] * r + INV[1][0] * g + INV[2][0] * b).max(0.0)
}

/// Dove contare: la zona larga 5 campi e alta 2 con più nuclei (la più scura di ematossilina), sul
/// tessuto. Ritorna (x, y, larghezza, altezza) nel livello pieno.
fn zona_densa(s: &Slide, path: &Path) -> Result<(u32, u32, u32, u32), String> {
    let mpp = s.mpp.ok_or("il vetrino non dice quanto è grande un pixel: l'area di 2,37 mm² non si può misurare")? as f64;
    // Un livello piccolo per cercare: circa 8 µm per pixel.
    let (liv, _) = livello_per(s, Some(8.0));
    let liv = if s.levels[liv].pixels() > 60_000_000 { s.levels.len() - 1 } else { liv };
    let img = s.read_level(path, liv)?;
    let giu = s.full().width as f64 / img.width() as f64;
    let um = mpp * giu;
    let (zw, zh) = (((CAMPO_UM * 5.0) / um).round().max(1.0) as usize, ((CAMPO_UM * 2.0) / um).round().max(1.0) as usize);
    let (w, h) = (img.width() as usize, img.height() as usize);
    if zw > w || zh > h {
        return Err("il vetrino è più piccolo dell'area di 2,37 mm²".into());
    }
    // Tabelle delle somme: ematossilina e tessuto.
    let mut se = vec![0f64; (w + 1) * (h + 1)];
    let mut st = vec![0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let (mut re, mut rt) = (0f64, 0f64);
        for x in 0..w {
            let p = img.get_pixel(x as u32, y as u32).0;
            let t = tessuto(&p) as f64;
            re += ematossilina(&p) as f64 * t;
            rt += t;
            se[(y + 1) * (w + 1) + x + 1] = se[y * (w + 1) + x + 1] + re;
            st[(y + 1) * (w + 1) + x + 1] = st[y * (w + 1) + x + 1] + rt;
        }
    }
    let somma = |tab: &[f64], x: usize, y: usize| tab[(y + zh) * (w + 1) + x + zw] - tab[y * (w + 1) + x + zw] - tab[(y + zh) * (w + 1) + x] + tab[y * (w + 1) + x];
    let area = (zw * zh) as f64;
    let mut migliore: Option<(f64, usize, usize)> = None;
    let passo = (zw.min(zh) / 8).max(1);
    for y in (0..=h - zh).step_by(passo) {
        for x in (0..=w - zw).step_by(passo) {
            // Almeno l'80% di tessuto: niente bordi e buchi.
            if somma(&st, x, y) < area * 0.8 {
                continue;
            }
            let v = somma(&se, x, y);
            if migliore.is_none_or(|m| v > m.0) {
                migliore = Some((v, x, y));
            }
        }
    }
    let (_, x, y) = migliore.ok_or("nessuna zona di 2,37 mm² piena di tessuto")?;
    let fw = (CAMPO_UM * 5.0 / mpp).round() as u32;
    let fh = (CAMPO_UM * 2.0 / mpp).round() as u32;
    let fx = ((x as f64 * giu).round() as u32).min(s.full().width.saturating_sub(fw));
    let fy = ((y as f64 * giu).round() as u32).min(s.full().height.saturating_sub(fh));
    Ok((fx, fy, fw, fh))
}

fn rettangolo(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, spessore: u32, c: Rgb<u8>) {
    let (iw, ih) = img.dimensions();
    for t in 0..spessore {
        for xx in x..(x + w).min(iw) {
            for yy in [y + t, (y + h).saturating_sub(1 + t)] {
                if yy < ih {
                    img.put_pixel(xx, yy, c);
                }
            }
        }
        for yy in y..(y + h).min(ih) {
            for xx in [x + t, (x + w).saturating_sub(1 + t)] {
                if xx < iw {
                    img.put_pixel(xx, yy, c);
                }
            }
        }
    }
}

/// Fatto della conta mitotica.
pub struct Conta {
    pub zona: (u32, u32, u32, u32),
    pub um_per_px: f64,
}

/// La zona per la conta mitotica: un JPEG dei 2,37 mm² più ricchi di nuclei, a 0,25 µm per pixel
/// (40x) o alla risoluzione del vetrino, con i 10 campi numerati; e una panoramica con la zona segnata.
pub fn conta_mitotica(path: &Path, out: &Path, dove: &Path) -> Result<Conta, String> {
    let s = Slide::open(path)?;
    let (x, y, w, h) = zona_densa(&s, path)?;
    let mpp = s.mpp.unwrap_or(0.25) as f64;
    // Si legge dal livello pieno; se è più fine di 0,25 µm/pixel si rimpicciolisce.
    let mut img = s.read_region(path, 0, x, y, w, h)?;
    let mut um = mpp;
    if mpp < 0.24 {
        let f = 0.25 / mpp;
        img = image::imageops::resize(&img, (w as f64 / f) as u32, (h as f64 / f) as u32, image::imageops::FilterType::Triangle);
        um = 0.25;
    }
    // I dieci campi: righe sottili gialle e il numero in alto a sinistra di ognuno.
    let (iw, ih) = img.dimensions();
    let spessore = (iw / 1200).max(2);
    let giallo = Rgb([255, 214, 0]);
    let opz = opzioni_svg();
    for r in 0..2u32 {
        for c in 0..5u32 {
            let (cx, cy) = (c * iw / 5, r * ih / 2);
            rettangolo(&mut img, cx, cy, iw / 5, ih / 2, spessore, giallo);
            numero(&mut img, &opz, &(r * 5 + c + 1).to_string(), cx + spessore * 4, cy + spessore * 4, (ih / 2 / 9).max(16));
        }
    }
    let mut b = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 92).encode_image(&img).map_err(|e| e.to_string())?;
    fs::write(out, b).map_err(|e| e.to_string())?;
    // La panoramica con la zona in rosso.
    let mut pano = s.preview(path, 2048)?;
    let f = pano.width() as f64 / s.full().width as f64;
    let sp = (pano.width() / 400).max(2);
    rettangolo(&mut pano, (x as f64 * f) as u32, (y as f64 * f) as u32, ((w as f64 * f) as u32).max(sp * 3), ((h as f64 * f) as u32).max(sp * 3), sp, Rgb([220, 30, 40]));
    let mut b = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 88).encode_image(&pano).map_err(|e| e.to_string())?;
    fs::write(dove, b).map_err(|e| e.to_string())?;
    Ok(Conta { zona: (x, y, w, h), um_per_px: um })
}

fn opzioni_svg() -> usvg::Options<'static> {
    static DB: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();
    let db = DB
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(include_bytes!("../assets/fonts/Figtree-Bold.ttf").to_vec());
            std::sync::Arc::new(db)
        })
        .clone();
    usvg::Options { fontdb: db, ..Default::default() }
}

/// Un numero bianco bordato di nero, alto `alto` pixel, in (x, y).
fn numero(img: &mut RgbImage, opz: &usvg::Options, testo: &str, x: u32, y: u32, alto: u32) {
    let size = alto as f32;
    let w = (size * 0.7 * testo.len() as f32 + size * 0.6).ceil() as u32;
    let h = (size * 1.3).ceil() as u32;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><text x="{}" y="{}" font-family="Figtree" font-weight="700" font-size="{size}" fill="white" stroke="black" stroke-width="{}" paint-order="stroke">{testo}</text></svg>"#,
        size * 0.15,
        size,
        (size / 8.0).max(2.0)
    );
    let Ok(tree) = usvg::Tree::from_str(&svg, opz) else { return };
    let Some(mut pm) = resvg::tiny_skia::Pixmap::new(w, h) else { return };
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    for (i, p) in pm.pixels().iter().enumerate() {
        let a = p.alpha() as u32;
        if a == 0 {
            continue;
        }
        let (px, py) = (x + i as u32 % w, y + i as u32 / w);
        if px >= img.width() || py >= img.height() {
            continue;
        }
        let d = img.get_pixel_mut(px, py);
        let c = p.demultiply();
        for (k, v) in [c.red(), c.green(), c.blue()].into_iter().enumerate() {
            d.0[k] = ((v as u32 * a + d.0[k] as u32 * (255 - a)) / 255) as u8;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pacchetto consulto

/// Il codice del caso: lo stesso per lo stesso file, senza dire niente del nome.
pub fn codice_caso(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("caso-{:02x}{:02x}{:02x}", h[0], h[1], h[2])
}

/// Il pacchetto per un consulto: lo ZIP (con le impronte SHA-256) della copia anonima del vetrino,
/// di una panoramica JPEG e di una scheda di testo. Ritorna quanti file e cosa è stato tolto.
pub fn consulto(path: &Path, out: &Path, codice: &str, avanzamento: &dyn Fn(f32)) -> Result<(usize, Vec<&'static str>), String> {
    let s = Slide::open(path)?;
    let tmp = out.with_file_name(format!(".{codice}-consulto-tmp-{}", std::process::id()));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let fatto = (|| {
        let svs = tmp.join(format!("{codice}.svs"));
        let tolti = copia_anonima(path, &svs)?;
        avanzamento(0.3);
        let pano = s.preview(path, 4096)?;
        let mut b = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 90).encode_image(&pano).map_err(|e| e.to_string())?;
        let jpg = tmp.join(format!("{codice}-panoramica.jpg"));
        fs::write(&jpg, b).map_err(|e| e.to_string())?;
        let l = s.full();
        let mut scheda = format!("Consulto {codice}\n\nVetrino: {} × {} pixel", l.width, l.height);
        if let Some(m) = s.magnification {
            scheda.push_str(&format!(", scansionato a {m}x"));
        }
        if let Some(m) = s.mpp {
            scheda.push_str(&format!(", {:.4} µm per pixel", m).replace('.', ","));
        }
        scheda.push_str(&format!(".\nTolti per la privacy: {}.\n\n", tolti.join(", ")));
        scheda.push_str("Specie:\nRazza:\nEtà:\nSesso:\nSede del prelievo:\nDomanda clinica:\n\n");
        scheda.push_str("Il vetrino si apre con QuPath, ImageScope o Cedro. Le impronte SHA-256 di ogni file sono in manifest-sha256.txt.\n");
        let txt = tmp.join(format!("{codice}-scheda.txt"));
        fs::write(&txt, scheda).map_err(|e| e.to_string())?;
        let files = vec![svs, jpg, txt];
        let n = files.len();
        let nome = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| codice.to_string());
        crate::pacchetto::crea(&files, out, &nome, &|p| avanzamento(0.35 + p * 0.65))?;
        Ok((n, tolti))
    })();
    let _ = fs::remove_dir_all(&tmp);
    fatto
}

#[cfg(test)]
mod prove {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/svs").join(name)
    }

    fn cartella(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cedro-svslab-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn la_descrizione_perde_nome_data_e_utente() {
        let d = "Aperio Image Library v10\r\n46920x33014 JPEG/RGB Q=30|AppMag = 20|Filename = Rossi-Fido|Date = 12/29/09|Time = 09:59:15|User = abc|MPP = 0.4990";
        let p = pulisci_descrizione(d);
        assert_eq!(p.len(), d.len());
        assert!(!p.contains("Rossi") && !p.contains("12/29") && !p.contains("abc"));
        assert!(p.contains("AppMag = 20") && p.contains("MPP = 0.4990"));
    }

    #[test]
    fn la_copia_anonima_si_legge_e_non_ha_etichetta() {
        let d = cartella("anon");
        for f in ["cmu1-pezzo.svs", "piramide.svs"] {
            let src = fixture(f);
            let prima = Slide::open(&src).unwrap();
            let out = d.join(f);
            let tolti = copia_anonima(&src, &out).unwrap();
            let dopo = Slide::open(&out).unwrap();
            assert!(dopo.label.is_none() && dopo.macro_image.is_none(), "{f}: {tolti:?}");
            assert_eq!(dopo.levels.len(), prima.levels.len());
            assert_eq!(dopo.mpp, prima.mpp);
            // I pixel del vetrino sono gli stessi.
            let a = prima.read_level(&src, prima.levels.len() - 1).unwrap();
            let b = dopo.read_level(&out, dopo.levels.len() - 1).unwrap();
            assert_eq!(a, b, "{f}");
            if prima.label.is_some() {
                assert!(tolti.contains(&"etichetta"), "{f}: {tolti:?}");
            }
            // Il nome del file non c'è più.
            let bytes = fs::read(&out).unwrap();
            let testo = String::from_utf8_lossy(&bytes);
            assert!(!testo.contains("Filename = CMU"), "{f}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn tessere_e_csv() {
        let d = cartella("tessere");
        let src = fixture("cmu1-pezzo.svs");
        let o = Tessere { lato: 64, ingrandimento: 0, solo_tessuto: false, jpeg: false };
        let f = tessere(&src, &d.join("t"), "cmu1", &o, &|_| {}).unwrap();
        let s = Slide::open(&src).unwrap();
        let attese = (s.full().width / 64) * (s.full().height / 64);
        assert_eq!(f.scritte as u32, attese);
        let csv = fs::read_to_string(d.join("t/tessere.csv")).unwrap();
        assert!(csv.starts_with("file;x;y;lato_px;um_per_px;tessuto\ncmu1_x0_y0.png;0;0;64;"), "{csv}");
        let t = image::open(d.join("t/cmu1_x64_y0.png")).unwrap().to_rgb8();
        assert_eq!(t, s.read_region(&src, 0, 64, 0, 64, 64).unwrap());
        let _ = fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod prove_vere {
    use super::*;

    /// Con CEDRO_SVS=vetrino.svs: copia anonima, tessere, conta mitotica e consulto sul file vero,
    /// accanto al file.
    #[test]
    #[ignore]
    fn file_vero() {
        let Ok(p) = std::env::var("CEDRO_SVS") else { return };
        let p = PathBuf::from(p);
        let dir = p.with_extension("prove");
        let _ = fs::create_dir_all(&dir);
        let t0 = std::time::Instant::now();
        let c = conta_mitotica(&p, &dir.join("conta.jpg"), &dir.join("dove.jpg")).unwrap();
        eprintln!("conta: zona {:?} a {} µm/px in {:?}", c.zona, c.um_per_px, t0.elapsed());
        let t0 = std::time::Instant::now();
        let f = tessere(&p, &dir.join("tessere"), "t", &Tessere::default(), &|_| {}).unwrap();
        eprintln!("tessere: {} scritte, {} scartate, {:?} µm/px in {:?}", f.scritte, f.scartate, f.um_per_px, t0.elapsed());
        let t0 = std::time::Instant::now();
        let (n, tolti) = consulto(&p, &dir.join("consulto.zip"), &codice_caso(&p), &|_| {}).unwrap();
        eprintln!("consulto: {n} file, tolti {tolti:?} in {:?}", t0.elapsed());
    }
}
