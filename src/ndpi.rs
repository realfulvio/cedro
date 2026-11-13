//! I vetrini Hamamatsu NanoZoomer (.ndpi). Fuori sembrano un TIFF classico, ma gli indirizzi sono
//! a 64 bit: dopo le voci di ogni pagina ci sono 4 byte in più per voce con la parte alta del valore
//! (così un file supera i 4 GB). Ogni livello della piramide è un solo JPEG gigantesco con i
//! marcatori di ripartenza (RST): l'indice `McuStarts` dice dove comincia ogni intervallo.
//!
//! Una tessera si ricompone senza decodificare il resto: l'intestazione del JPEG con le misure della
//! tessera, poi gli intervalli della sua colonna, uno per riga di blocchi, separati da marcatori RST
//! rinumerati. È un JPEG normale che qualsiasi lettore (qui zune-jpeg) decodifica da solo, e le
//! tessere di una fila si decodificano in parallelo come per gli Aperio. Tutto da specifica (la
//! stessa che seguono OpenSlide e tifffile), in Rust.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use crate::svs::{Blob, Level, Slide, Source, Vendor, JPEG, PHOTOMETRIC_YCBCR};

/// Il tag che c'è solo negli NDPI (versione del formato).
const TAG_FORMATO: u16 = 65420;
/// L'ingrandimento della pagina: positivo per i livelli, -1 la foto del vetrino, -2 la mappa.
const TAG_LENTE: u16 = 65421;
/// Il piano di fuoco (nm): si tiene solo lo 0.
const TAG_FUOCO: u16 = 65424;
const TAG_MCU_BASSI: u16 = 65426;
const TAG_MCU_ALTI: u16 = 65432;

/// Le righe di intervalli che una tessera mette insieme: abbastanza per tessere di circa 256 righe.
const RIGHE_TESSERA: u32 = 256;

/// Una voce di una pagina, con il valore (o l'indirizzo) già a 64 bit.
#[derive(Clone, Debug)]
struct Voce {
    tag: u16,
    tipo: u16,
    n: u64,
    /// Il valore se ci sta, se no l'indirizzo: 8 byte (4 dalla voce, 4 dall'estensione NDPI).
    valore: [u8; 8],
    /// Dove sta nel file il campo valore/indirizzo della voce (i primi 4 byte).
    pos: u64,
}

/// Una pagina (IFD) dell'NDPI.
#[derive(Clone, Debug)]
struct Pagina {
    voci: Vec<Voce>,
    /// Dove sta il puntatore (8 byte) alla pagina dopo.
    pos_prossima: u64,
    indirizzo: u64,
}

fn grandezza_tipo(tipo: u16) -> u64 {
    match tipo {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 0,
    }
}

impl Voce {
    /// Il valore sta dentro la voce (le stringhe NDPI mai).
    fn dentro(&self) -> bool {
        self.n * grandezza_tipo(self.tipo) <= 4 && self.tipo != 2
    }

    fn indirizzo(&self) -> u64 {
        u64::from_le_bytes(self.valore)
    }

    /// I byte del valore.
    fn byte(&self, f: &mut File) -> Result<Vec<u8>, String> {
        let len = self.n.saturating_mul(grandezza_tipo(self.tipo));
        if len > 256 << 20 {
            return Err(format!("voce {} troppo grande", self.tag));
        }
        if self.dentro() {
            // Un LONG con l'estensione diversa da zero è un valore a 64 bit (indirizzi oltre i 4 GB).
            let n = if self.tipo == 4 && self.n == 1 && self.valore[4..] != [0; 4] { 8 } else { len as usize };
            return Ok(self.valore[..n.max(len as usize).min(8)].to_vec());
        }
        let mut b = vec![0u8; len as usize];
        f.seek(SeekFrom::Start(self.indirizzo())).map_err(|e| e.to_string())?;
        f.read_exact(&mut b).map_err(|e| format!("voce {} non letta: {e}", self.tag))?;
        Ok(b)
    }

    /// I numeri interi della voce (BYTE, SHORT, LONG, LONG8, e il LONG a 64 bit degli NDPI).
    fn interi(&self, f: &mut File) -> Result<Vec<u64>, String> {
        let b = self.byte(f)?;
        Ok(match self.tipo {
            1 | 7 => b.iter().map(|&v| v as u64).collect(),
            3 => b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as u64).collect(),
            4 if b.len() == 8 && self.n == 1 => vec![u64::from_le_bytes(b[..8].try_into().unwrap())],
            4 | 13 => b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as u64).collect(),
            16 | 18 => b.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect(),
            _ => return Err(format!("voce {}: tipo {} non intero", self.tag, self.tipo)),
        })
    }

    /// Un numero qualsiasi (interi, razionali, FLOAT, SLONG).
    fn numero(&self, f: &mut File) -> Option<f64> {
        let b = self.byte(f).ok()?;
        Some(match self.tipo {
            9 => i32::from_le_bytes(b.get(..4)?.try_into().ok()?) as f64,
            8 => i16::from_le_bytes(b.get(..2)?.try_into().ok()?) as f64,
            11 => f32::from_le_bytes(b.get(..4)?.try_into().ok()?) as f64,
            12 => f64::from_le_bytes(b.get(..8)?.try_into().ok()?),
            5 | 10 => {
                let (a, d) = (u32::from_le_bytes(b.get(..4)?.try_into().ok()?), u32::from_le_bytes(b.get(4..8)?.try_into().ok()?));
                if d == 0 {
                    return None;
                }
                if self.tipo == 10 {
                    (a as i32) as f64 / (d as i32) as f64
                } else {
                    a as f64 / d as f64
                }
            }
            _ => *self.interi(f).ok()?.first()? as f64,
        })
    }
}

impl Pagina {
    fn voce(&self, tag: u16) -> Option<&Voce> {
        self.voci.iter().find(|v| v.tag == tag)
    }
}

fn leggi_u64(f: &mut File, pos: u64) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    f.seek(SeekFrom::Start(pos))?;
    f.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// Una pagina in modo NDPI: voci da 12 byte, poi 8 byte della prossima, poi 4 byte per voce.
fn pagina(f: &mut File, indirizzo: u64, lunghezza: u64) -> Result<Pagina, String> {
    if indirizzo < 8 || indirizzo >= lunghezza {
        return Err("pagina fuori dal file".into());
    }
    let mut b2 = [0u8; 2];
    f.seek(SeekFrom::Start(indirizzo)).map_err(|e| e.to_string())?;
    f.read_exact(&mut b2).map_err(|e| e.to_string())?;
    let n = u16::from_le_bytes(b2) as u64;
    if n == 0 || n > 4096 {
        return Err("pagina con un numero di voci strano".into());
    }
    let mut corpo = vec![0u8; (n * 12 + 8 + n * 4) as usize];
    f.read_exact(&mut corpo).map_err(|_| "pagina tagliata".to_string())?;
    let est = &corpo[(n * 12 + 8) as usize..];
    let mut voci = Vec::with_capacity(n as usize);
    for i in 0..n as usize {
        let e = &corpo[i * 12..i * 12 + 12];
        let mut valore = [0u8; 8];
        valore[..4].copy_from_slice(&e[8..12]);
        valore[4..].copy_from_slice(&est[i * 4..i * 4 + 4]);
        voci.push(Voce {
            tag: u16::from_le_bytes([e[0], e[1]]),
            tipo: u16::from_le_bytes([e[2], e[3]]),
            n: u32::from_le_bytes(e[4..8].try_into().unwrap()) as u64,
            valore,
            pos: indirizzo + 2 + i as u64 * 12 + 8,
        });
    }
    let pos_prossima = indirizzo + 2 + n * 12;
    Ok(Pagina { voci, pos_prossima, indirizzo })
}

/// Tutte le pagine dell'NDPI, oppure un errore se il file non lo è.
fn pagine(f: &mut File) -> Result<Vec<Pagina>, String> {
    let lunghezza = f.metadata().map_err(|e| e.to_string())?.len();
    let mut testa = [0u8; 4];
    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    f.read_exact(&mut testa).map_err(|e| e.to_string())?;
    if testa != *b"II*\0" {
        return Err("non è un NDPI (non è un TIFF classico little-endian)".into());
    }
    let mut prossima = leggi_u64(f, 4).map_err(|e| e.to_string())?;
    let mut out: Vec<Pagina> = Vec::new();
    while prossima != 0 {
        if out.len() > 1000 || out.iter().any(|p| p.indirizzo == prossima) {
            return Err("catena di pagine rovinata".into());
        }
        let p = pagina(f, prossima, lunghezza)?;
        if out.is_empty() && p.voce(TAG_FORMATO).is_none() {
            return Err("non è un NDPI".into());
        }
        prossima = leggi_u64(f, p.pos_prossima).map_err(|e| e.to_string())?;
        out.push(p);
    }
    if out.is_empty() {
        return Err("NDPI senza pagine".into());
    }
    Ok(out)
}

/// Vero se il file è un Hamamatsu NDPI: la prima pagina, letta in modo NDPI, ha il tag del formato.
pub fn is_ndpi(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let Ok(lunghezza) = f.metadata().map(|m| m.len()) else { return false };
    let mut testa = [0u8; 4];
    if f.read_exact(&mut testa).is_err() || testa != *b"II*\0" {
        return false;
    }
    let Ok(prima) = leggi_u64(&mut f, 4) else { return false };
    pagina(&mut f, prima, lunghezza).is_ok_and(|p| p.voce(TAG_FORMATO).is_some_and(|v| v.n > 0))
}

/// Un livello NDPI: dove stanno gli intervalli del suo JPEG e come si ricompone una tessera.
#[derive(Debug)]
pub struct Jpeg {
    /// L'intestazione del JPEG fino alla fine del segmento SOS (tabelle, DRI, SOF).
    header: Vec<u8>,
    /// Dove sta il marcatore SOF nell'intestazione.
    sof: usize,
    /// Dove comincia ogni intervallo nel file (indirizzi assoluti). Un solo elemento per i livelli senza tessere.
    starts: Vec<u64>,
    /// Dove finisce il JPEG nel file.
    end: u64,
    /// Intervalli per riga di blocchi (= tessere per fila).
    across: usize,
    /// Righe di blocchi.
    rows: usize,
    /// Righe di blocchi per tessera.
    stack: usize,
    mcu_h: u32,
    tile_w: u32,
    /// Il livello è un JPEG intero (piccolo), senza intervalli da ricomporre.
    whole: bool,
}

/// Quello che serve dell'intestazione di un JPEG.
struct Testa {
    /// Lunghezza fino alla fine del segmento SOS.
    len: usize,
    sof: usize,
    width: u32,
    height: u32,
    mcu_w: u32,
    mcu_h: u32,
    /// Blocchi per intervallo (0 = nessun DRI).
    restart: u32,
}

fn testa_jpeg(b: &[u8]) -> Result<Testa, String> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return Err("il livello non comincia con un JPEG".into());
    }
    let mut i = 2;
    let (mut sof, mut restart) = (None, 0u32);
    let (mut w, mut h, mut mw, mut mh) = (0, 0, 8, 8);
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return Err("intestazione JPEG rovinata".into());
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let seg = b.get(i + 4..i + 2 + len).ok_or("intestazione JPEG tagliata")?;
        match m {
            0xC0 | 0xC1 => {
                if seg.len() < 6 {
                    return Err("SOF corto".into());
                }
                sof = Some(i);
                h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                let nc = seg[5] as usize;
                let (mut hh, mut vv) = (1u32, 1u32);
                for c in 0..nc {
                    let f = *seg.get(6 + c * 3 + 1).ok_or("SOF corto")?;
                    hh = hh.max((f >> 4) as u32);
                    vv = vv.max((f & 15) as u32);
                }
                (mw, mh) = (8 * hh, 8 * vv);
            }
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err("JPEG progressivo o senza perdita: NDPI non gestito".into());
            }
            0xDD => restart = u16::from_be_bytes([*seg.first().unwrap_or(&0), *seg.get(1).unwrap_or(&0)]) as u32,
            0xDA => {
                let sof = sof.ok_or("JPEG senza SOF")?;
                return Ok(Testa { len: i + 2 + len, sof, width: w, height: h, mcu_w: mw, mcu_h: mh, restart });
            }
            _ => {}
        }
        i += 2 + len;
    }
    Err("intestazione JPEG senza SOS".into())
}

/// Cerca nel JPEG (dalla fine dell'intestazione) dove comincia ogni intervallo: subito dopo ogni RST.
fn cerca_intervalli(f: &mut File, da: u64, a: u64, quanti: usize) -> Result<Vec<u64>, String> {
    let mut out = Vec::with_capacity(quanti);
    out.push(da);
    let mut pos = da;
    let mut prima: Option<u8> = None;
    let mut buf = vec![0u8; 8 << 20];
    f.seek(SeekFrom::Start(da)).map_err(|e| e.to_string())?;
    while pos < a && out.len() < quanti {
        let n = ((a - pos) as usize).min(buf.len());
        f.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
        for (k, &c) in buf[..n].iter().enumerate() {
            if prima == Some(0xFF) && (0xD0..=0xD7).contains(&c) {
                out.push(pos + k as u64 + 1);
                if out.len() == quanti {
                    break;
                }
            }
            prima = Some(c);
        }
        pos += n as u64;
    }
    if out.len() != quanti {
        return Err(format!("trovati {} intervalli JPEG su {quanti}", out.len()));
    }
    Ok(out)
}

impl Jpeg {
    /// Le tessere della fila `trow` dalla colonna `c0` alla `c1` (esclusa), già pronte da decodificare.
    pub(crate) fn band(&self, f: &mut File, lunghezza: u64, trow: usize, c0: usize, c1: usize) -> Result<Vec<Vec<u8>>, String> {
        let leggi = |f: &mut File, a: u64, b: u64| -> Result<Vec<u8>, String> {
            if b < a || b > lunghezza || b - a > 2 << 30 {
                return Err("intervallo JPEG fuori dal file (vetrino tagliato o rovinato)".into());
            }
            let mut buf = vec![0u8; (b - a) as usize];
            f.seek(SeekFrom::Start(a)).map_err(|e| e.to_string())?;
            f.read_exact(&mut buf).map_err(|e| format!("vetrino non letto: {e}"))?;
            Ok(buf)
        };
        if self.whole {
            return Ok(vec![leggi(f, self.starts[0], self.end)?]);
        }
        let r0 = trow * self.stack;
        let r1 = (r0 + self.stack).min(self.rows);
        if r0 >= r1 || c0 >= c1 || c1 > self.across {
            return Ok(vec![Vec::new(); c1.saturating_sub(c0)]);
        }
        let fine = |s: usize| if s + 1 < self.starts.len() { self.starts[s + 1] } else { self.end };
        let primo = r0 * self.across + c0;
        let ultimo = (r1 - 1) * self.across + c1 - 1;
        let a = self.starts[primo];
        let buf = leggi(f, a, fine(ultimo))?;
        let mut out = Vec::with_capacity(c1 - c0);
        for col in c0..c1 {
            let mut t = Vec::with_capacity(self.header.len() + buf.len() / (c1 - c0) + 64);
            t.extend_from_slice(&self.header);
            // Le misure della tessera nel SOF: altezza (righe di blocchi davvero presenti) e larghezza.
            let h = ((r1 - r0) as u32 * self.mcu_h) as u16;
            t[self.sof + 5..self.sof + 7].copy_from_slice(&h.to_be_bytes());
            t[self.sof + 7..self.sof + 9].copy_from_slice(&(self.tile_w as u16).to_be_bytes());
            for (k, r) in (r0..r1).enumerate() {
                let s = r * self.across + col;
                let (da, a2) = ((self.starts[s] - a) as usize, (fine(s) - a) as usize);
                let mut seg = buf.get(da..a2).ok_or("intervallo JPEG fuori posto")?;
                // L'intervallo finisce col suo RST (o con la fine del JPEG): si toglie e si rimette rinumerato.
                if seg.len() >= 2 && seg[seg.len() - 2] == 0xFF && matches!(seg[seg.len() - 1], 0xD0..=0xD7 | 0xD9) {
                    seg = &seg[..seg.len() - 2];
                }
                if k > 0 {
                    t.extend_from_slice(&[0xFF, 0xD0 + ((k - 1) % 8) as u8]);
                }
                t.extend_from_slice(seg);
            }
            t.extend_from_slice(&[0xFF, 0xD9]);
            out.push(t);
        }
        Ok(out)
    }
}

/// Apre un NDPI: i livelli (solo il piano di fuoco 0), la foto del vetrino, micron per pixel e ingrandimento.
pub fn open(path: &Path) -> Result<Slide, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let lunghezza = f.metadata().map_err(|e| e.to_string())?.len();
    let pag = pagine(&mut f)?;
    let mut livelli: Vec<(Level, f64, Option<f64>)> = Vec::new();
    let mut blobs = Vec::new();
    let mut macro_image = None;
    for (ifd, p) in pag.iter().enumerate() {
        let num = |f: &mut File, t: u16| p.voce(t).and_then(|v| v.numero(f));
        let lente = num(&mut f, TAG_LENTE).unwrap_or(0.0);
        let (Some(off), Some(cnt)) = (p.voce(273), p.voce(279)) else { continue };
        let (off, cnt) = (off.interi(&mut f)?, cnt.interi(&mut f)?);
        if off.len() != 1 || cnt.len() != 1 {
            return Err("NDPI con più strisce per pagina: non gestito".into());
        }
        let (start, len) = (off[0], cnt[0]);
        if start.checked_add(len).is_none_or(|e| e > lunghezza) {
            return Err("NDPI tagliato: un livello va oltre la fine del file".into());
        }
        if lente == -1.0 {
            macro_image = Some(blobs.len());
            blobs.push(Blob { path: path.to_path_buf(), offset: start, len });
            continue;
        }
        if lente <= 0.0 || num(&mut f, TAG_FUOCO).unwrap_or(0.0) != 0.0 {
            continue;
        }
        let mut w = num(&mut f, 256).unwrap_or(0.0) as u32;
        let mut h = num(&mut f, 257).unwrap_or(0.0) as u32;
        let mut testa = vec![0u8; len.min(1 << 20) as usize];
        f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        f.read_exact(&mut testa).map_err(|e| e.to_string())?;
        let t = testa_jpeg(&testa)?;
        let header = testa[..t.len].to_vec();
        let end = start + len;
        let mcu_w = t.restart * t.mcu_w;
        let across = if mcu_w > 0 { (w / mcu_w) as usize } else { 0 };
        let rows = h.div_ceil(t.mcu_h) as usize;
        let tessere = t.restart > 0 && across > 0 && w % mcu_w == 0 && (across * rows > 1);
        let jpeg = if tessere {
            let quanti = across * rows;
            // Ogni intervallo finisce con un RST (2 byte): più intervalli dei byte è un file rovinato,
            // e l'indice non deve prendere gigabyte di memoria.
            if quanti as u64 > len / 2 {
                return Err("NDPI rovinato: le misure del livello non tornano con i suoi dati".into());
            }
            let indice = (|| -> Option<Vec<u64>> {
                let bassi = p.voce(TAG_MCU_BASSI)?.interi(&mut f).ok()?;
                if bassi.len() != quanti {
                    return None;
                }
                let alti = match p.voce(TAG_MCU_ALTI) {
                    Some(v) => Some(v.interi(&mut f).ok()?).filter(|a| a.len() == quanti)?,
                    None => vec![0; quanti],
                };
                Some(bassi.iter().zip(&alti).map(|(b, a)| start + (b | (a << 32))).collect())
            })();
            // L'indice di Hamamatsu non è sempre giusto: si controlla che alcuni intervalli seguano un RST.
            let buono = indice.as_ref().is_some_and(|s| {
                s[0] == start + t.len as u64
                    && s.windows(2).all(|w| w[0] < w[1])
                    && [1, s.len() / 2, s.len() - 1].iter().all(|&k| {
                        let mut b = [0u8; 2];
                        k == 0 || (f.seek(SeekFrom::Start(s[k] - 2)).is_ok() && f.read_exact(&mut b).is_ok() && b[0] == 0xFF && (0xD0..=0xD7).contains(&b[1]) && (b[1] - 0xD0) as usize == (k - 1) % 8)
                    })
            });
            let starts = match indice {
                Some(s) if buono => s,
                _ => cerca_intervalli(&mut f, start + t.len as u64, end, quanti)?,
            };
            let stack = (RIGHE_TESSERA / t.mcu_h).max(1) as usize;
            Jpeg { sof: t.sof, header, starts, end, across, rows, stack, mcu_h: t.mcu_h, tile_w: mcu_w, whole: false }
        } else {
            // Un livello piccolo senza tessere: un JPEG normale. Le misure vere sono quelle del JPEG.
            if t.width > 0 && t.height > 0 && (t.width, t.height) != (w, h) {
                (w, h) = (t.width, t.height);
            }
            if w as u64 * h as u64 > crate::svs::IN_MEMORY_PIXELS {
                return Err("livello NDPI senza marcatori di ripartenza troppo grande".into());
            }
            Jpeg { sof: t.sof, header: Vec::new(), starts: vec![start], end, across: 1, rows: 1, stack: 1, mcu_h: h, tile_w: w, whole: true }
        };
        let (tile_w, tile_h) = if jpeg.whole { (w, h) } else { (jpeg.tile_w, jpeg.stack as u32 * t.mcu_h) };
        // Micron per pixel dalla risoluzione TIFF (in centimetri per gli NDPI).
        let unita = num(&mut f, 296).unwrap_or(2.0);
        let mpp = num(&mut f, 282).filter(|r| *r > 0.0).map(|r| if unita == 3.0 { 10_000.0 / r } else { 25_400.0 / r });
        let level = Level {
            ifd,
            width: w,
            height: h,
            tile_w,
            tile_h,
            compression: JPEG,
            photometric: PHOTOMETRIC_YCBCR,
            // Le tessere NDPI si trovano con `jpeg.starts`: gli indirizzi TIFF non servono (come MIRAX).
            offsets: Vec::new(),
            counts: Vec::new(),
            tables: None,
            source: Source::Ndpi(Arc::new(jpeg)),
        };
        livelli.push((level, lente, mpp));
    }
    if livelli.is_empty() {
        return Err("NDPI senza livelli".into());
    }
    livelli.sort_by_key(|(l, _, _)| std::cmp::Reverse(l.pixels()));
    let (mag, mpp) = (livelli[0].1, livelli[0].2);
    Ok(Slide {
        levels: livelli.into_iter().map(|(l, _, _)| l).collect(),
        thumbnail: None,
        label: None,
        macro_image,
        mpp: mpp.map(|m| m as f32),
        magnification: Some(mag as f32),
        vendor: Vendor::Hamamatsu,
        blobs,
        fill: [255; 3],
    })
}

/// I tag di testo che possono dire di chi è il vetrino o chi l'ha scansionato: DocumentName,
/// ImageDescription, PageName, DateTime, Artist, HostComputer, il riferimento (nome del vetrino),
/// il numero di serie dello scanner e gli otto codici a barre.
const TAG_PERSONALI: &[u16] = &[269, 270, 285, 306, 315, 316, 65427, 65442, 65468, 65469, 65470, 65471, 65472, 65473, 65474, 65475];

/// Rende anonimo, sul posto, l'NDPI `path` (una copia): la foto del vetrino (con l'etichetta) perde i
/// pixel ed esce dalla catena delle pagine; nome del vetrino, data, codici a barre e numero dello
/// scanner diventano spazi. Dice cosa ha tolto.
pub fn anonimizza_sul_posto(path: &Path) -> Result<Vec<&'static str>, String> {
    let mut f = OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let pag = pagine(&mut f)?;
    let scrivi = |f: &mut File, pos: u64, b: &[u8]| -> Result<(), String> {
        f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        f.write_all(b).map_err(|e| e.to_string())
    };
    let mut tolti = Vec::new();
    let mut via = Vec::new();
    for (i, p) in pag.iter().enumerate() {
        let lente = p.voce(TAG_LENTE).and_then(|v| v.numero(&mut f)).unwrap_or(0.0);
        if lente == -1.0 {
            if let (Some(o), Some(c)) = (p.voce(273), p.voce(279)) {
                let (o, c) = (o.interi(&mut f)?, c.interi(&mut f)?);
                for (o, c) in o.iter().zip(&c) {
                    let (mut pos, mut resto) = (*o, *c);
                    while resto > 0 {
                        let n = resto.min(1 << 20);
                        scrivi(&mut f, pos, &vec![0u8; n as usize])?;
                        pos += n;
                        resto -= n;
                    }
                }
            }
            via.push(i);
            if !tolti.contains(&"foto del vetrino con l'etichetta") {
                tolti.push("foto del vetrino con l'etichetta");
            }
            continue;
        }
        for v in p.voci.iter().filter(|v| TAG_PERSONALI.contains(&v.tag) && matches!(v.tipo, 1 | 2 | 7) && v.n > 0 && v.n <= 1 << 24) {
            let vecchio = v.byte(&mut f)?;
            let nuovo: Vec<u8> = vecchio.iter().map(|b| if *b == 0 { 0 } else { b' ' }).collect();
            if nuovo != vecchio {
                if v.dentro() {
                    scrivi(&mut f, v.pos, &nuovo)?;
                } else {
                    scrivi(&mut f, v.indirizzo(), &nuovo)?;
                }
                let cosa = if v.tag >= 65468 { "codici a barre" } else if v.tag == 306 { "data della scansione" } else { "nome del vetrino, scanner e autore" };
                if !tolti.contains(&cosa) {
                    tolti.push(cosa);
                }
            }
        }
    }
    // Le pagine tolte escono dalla catena: ogni pagina rimasta punta alla prossima rimasta.
    let buone: Vec<usize> = (0..pag.len()).filter(|i| !via.contains(i)).collect();
    if buone.is_empty() {
        return Err("NDPI senza pagine da tenere".into());
    }
    scrivi(&mut f, 4, &pag[buone[0]].indirizzo.to_le_bytes())?;
    for (k, &i) in buone.iter().enumerate() {
        let dopo = buone.get(k + 1).map(|&j| pag[j].indirizzo).unwrap_or(0);
        scrivi(&mut f, pag[i].pos_prossima, &dopo.to_le_bytes())?;
    }
    f.flush().map_err(|e| e.to_string())?;
    Ok(tolti)
}

#[cfg(test)]
mod prove {
    use super::*;

    /// Un JPEG a tessere come quelli degli NDPI, fatto qui: intervalli di `ri` blocchi da 8 × 8 (in
    /// scala di grigi, per avere blocchi da 8), con l'indice McuStarts.
    pub(crate) fn ndpi_finto(dir: &Path, w: u32, h: u32) -> (std::path::PathBuf, image::RgbImage) {
        // L'immagine: sfumature, così ogni tessera è diversa.
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) % 256) as u8]));
        // Un JPEG baseline 4:4:4 con DRI: il codificatore del crate image non mette i RST, quindi si
        // codifica una fila di blocchi alla volta (8 righe) e si uniscono i dati con i RST, come fa `svs`.
        let ri = 4u32; // blocchi per intervallo: tessere da 32 px
        let mut dati = Vec::new();
        let mut starts = Vec::new();
        let mut header = Vec::new();
        let mut k = 0usize;
        for by in 0..h / 8 {
            for bx in 0..w / (8 * ri) {
                let pezzo = image::imageops::crop_imm(&img, bx * 8 * ri, by * 8, 8 * ri, 8).to_image();
                let mut b = Vec::new();
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 95);
                enc.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(72));
                enc.encode(pezzo.as_raw(), 8 * ri, 8, image::ExtendedColorType::Rgb8).unwrap();
                let t = testa_jpeg(&b).unwrap();
                assert_eq!((t.mcu_w, t.mcu_h), (8, 8), "il codificatore deve fare 4:4:4");
                if header.is_empty() {
                    header = b[..t.len].to_vec();
                }
                if k > 0 {
                    dati.extend_from_slice(&[0xFF, 0xD0 + ((k - 1) % 8) as u8]);
                }
                starts.push(dati.len() as u64);
                dati.extend_from_slice(&b[t.len..b.len() - 2]);
                k += 1;
            }
        }
        dati.extend_from_slice(&[0xFF, 0xD9]);
        // L'intestazione: SOF con le misure intere, più il DRI subito prima del SOS.
        let t = testa_jpeg(&header).unwrap();
        let mut testa = header[..t.len].to_vec();
        testa[t.sof + 5..t.sof + 7].copy_from_slice(&(h as u16).to_be_bytes());
        testa[t.sof + 7..t.sof + 9].copy_from_slice(&(w as u16).to_be_bytes());
        let sos = {
            let mut i = 2;
            while testa[i + 1] != 0xDA {
                i += 2 + u16::from_be_bytes([testa[i + 2], testa[i + 3]]) as usize;
            }
            i
        };
        let mut testa2 = testa[..sos].to_vec();
        testa2.extend_from_slice(&[0xFF, 0xDD, 0, 4, 0, ri as u8]);
        testa2.extend_from_slice(&testa[sos..]);
        let mut jpeg = testa2.clone();
        jpeg.extend_from_slice(&dati);
        let mcu: Vec<u64> = starts.iter().map(|s| s + testa2.len() as u64).collect();

        // Il file NDPI: intestazione, JPEG, pagina con le voci (in modo NDPI), indice McuStarts.
        let mut f = Vec::new();
        f.extend_from_slice(b"II*\0");
        f.extend_from_slice(&[0u8; 8]); // prima pagina, si riempie dopo
        let jpeg_at = f.len() as u64;
        f.extend_from_slice(&jpeg);
        let mcu_at = f.len() as u64;
        for m in &mcu {
            f.extend_from_slice(&(*m as u32).to_le_bytes());
        }
        let res_at = f.len() as u64;
        f.extend_from_slice(&40_000u32.to_le_bytes()); // 40.000 px/cm = 0,25 µm/px
        f.extend_from_slice(&1u32.to_le_bytes());
        let rif_at = f.len() as u64;
        f.extend_from_slice(b"Rossi Fido 2026-123\0");
        let ifd = f.len() as u64;
        f[4..12].copy_from_slice(&ifd.to_le_bytes());
        // (tag, tipo, n, valore)
        let voci: Vec<(u16, u16, u32, u64)> = vec![
            (256, 4, 1, w as u64),
            (257, 4, 1, h as u64),
            (259, 3, 1, 7),
            (262, 3, 1, 6),
            (273, 4, 1, jpeg_at),
            (277, 3, 1, 3),
            (279, 4, 1, jpeg.len() as u64),
            (282, 5, 1, res_at),
            (283, 5, 1, res_at),
            (296, 3, 1, 3),
            (TAG_FORMATO, 4, 1, 1),
            (TAG_LENTE, 11, 1, 40f32.to_bits() as u64),
            (TAG_FUOCO, 9, 1, 0),
            (TAG_MCU_BASSI, 4, mcu.len() as u32, mcu_at),
            (65427, 2, 20, rif_at),
        ];
        f.extend_from_slice(&(voci.len() as u16).to_le_bytes());
        for (tag, tipo, n, v) in &voci {
            f.extend_from_slice(&tag.to_le_bytes());
            f.extend_from_slice(&tipo.to_le_bytes());
            f.extend_from_slice(&n.to_le_bytes());
            f.extend_from_slice(&(*v as u32).to_le_bytes());
        }
        f.extend_from_slice(&0u64.to_le_bytes());
        for (_, _, _, v) in &voci {
            f.extend_from_slice(&((*v >> 32) as u32).to_le_bytes());
        }
        let p = dir.join("finto.ndpi");
        std::fs::write(&p, f).unwrap();
        (p, img)
    }

    #[test]
    fn un_ndpi_con_le_misure_rovinate_da_un_errore_e_non_chiede_gigabyte() {
        // Trovato rovinando a caso un NDPI: le misure del livello facevano chiedere 16 GB di memoria e Cedro si chiudeva.
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ndpi-rovinato.ndpi");
        if let Ok(s) = Slide::open(&p) {
            for l in 0..s.levels.len() {
                let _ = s.read_level(&p, l);
            }
        }
    }

    #[test]
    fn un_ndpi_fatto_qui_si_legge_tessera_per_tessera() {
        let d = std::env::temp_dir().join(format!("cedro-ndpi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (p, img) = ndpi_finto(&d, 128, 72);
        assert!(is_ndpi(&p));
        assert!(crate::svs::is_slide(&p));
        let s = Slide::open(&p).unwrap();
        assert_eq!(s.vendor, Vendor::Hamamatsu);
        assert_eq!((s.full().width, s.full().height), (128, 72));
        assert!((s.mpp.unwrap() - 0.25).abs() < 1e-4, "{:?}", s.mpp);
        assert_eq!(s.magnification, Some(40.0));
        let tutto = s.read_level(&p, 0).unwrap();
        let diff = tutto.as_raw().iter().zip(img.as_raw()).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(diff <= 12, "differenza {diff}");
        // Una zona a cavallo di più tessere, uguale allo stesso pezzo del livello intero.
        let z = s.read_region(&p, 0, 20, 10, 60, 40).unwrap();
        assert_eq!(z, image::imageops::crop_imm(&tutto, 20, 10, 60, 40).to_image());
        // La copia anonima perde il nome del vetrino e si legge uguale.
        let out = d.join("anonimo.ndpi");
        std::fs::copy(&p, &out).unwrap();
        let tolti = anonimizza_sul_posto(&out).unwrap();
        assert!(tolti.contains(&"nome del vetrino, scanner e autore"), "{tolti:?}");
        let b = std::fs::read(&out).unwrap();
        assert!(!b.windows(5).any(|w| w == b"Rossi"));
        let s2 = Slide::open(&out).unwrap();
        assert_eq!(s2.read_level(&out, 0).unwrap(), tutto);
        // Dal motore: la copia anonima resta un .ndpi col codice del caso, il consulto ha i suoi 3 file integri.
        let o = crate::engine::Options::default();
        let e = crate::engine::perform_with(&p, "strip", 80, None, &o).unwrap();
        let nome = e.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nome.starts_with("caso-") && nome.ends_with("-anonimo.ndpi"), "{nome}");
        assert!(!std::fs::read(&e.path).unwrap().windows(5).any(|w| w == b"Rossi"));
        let c = crate::engine::perform_with(&p, "consult", 80, None, &o).unwrap();
        let v = crate::pacchetto::verifica(&c.path, &|_| {}).unwrap();
        assert!(v.tutto_bene() && v.integri == 3, "{v:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
