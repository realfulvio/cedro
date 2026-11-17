//! I file Olympus / Evident cellSens (.vsi), degli scanner VS120 e VS200 e dei microscopi con cellSens.
//!
//! Il `.vsi` è un TIFF piccolo: dentro ci sono le miniature (la foto del vetrino, l'etichetta, una
//! per ogni scansione) e l'albero dei metadati. I pixel veri stanno accanto, nella cartella
//! `_<nome>_`: una sottocartella `stackNNNNN` per immagine (etichetta, panoramica, ogni area
//! scansionata), ognuna con un file `frame_t*.ets`.
//!
//! Un `.ets` comincia con l'intestazione «SIS» (quante dimensioni hanno le coordinate delle tessere,
//! dove sta l'intestazione «ETS» e dove sta l'elenco delle tessere). L'intestazione «ETS» dice il tipo
//! di pixel, i campioni per pixel, la compressione (grezzo, JPEG, JPEG 2000, JPEG senza perdita, PNG,
//! BMP), la misura delle tessere, il colore del fondo, se c'è la piramide e le misure vere
//! dell'immagine (larghezza, altezza e gli altri assi: tempo, piani Z, canali). Ogni tessera
//! dell'elenco ha le sue coordinate (x, y, gli altri assi, il livello della piramide per ultimo),
//! l'indirizzo e la grandezza. È lo schema documentato da Bio-Formats (CellSensReader) e da
//! slideio; le misure e l'ordine degli assi sono stati verificati pixel per pixel con slideio sui
//! campioni pubblici di Evident e Zenodo (vedi le prove in fondo).
//!
//! Le scansioni in campo chiaro (RGB a 8 bit) diventano un vetrino come gli Aperio (`svs::Slide`);
//! le fluorescenze (canali a 16 bit) diventano una pila di microscopia come gli Zeiss CZI.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Le compressioni delle tessere ETS.
pub const RAW: u32 = 0;
pub const JPEG: u32 = 2;
pub const JP2K: u32 = 3;
pub const JPEG_LOSSLESS: u32 = 5;
pub const PNG: u32 = 8;
pub const BMP: u32 = 9;

/// I tipi di pixel ETS che leggiamo.
pub const U8: u32 = 2;
pub const U16: u32 = 4;

/// Oltre questi pixel per piano una fluorescenza si legge da un livello ridotto della piramide.
const MASSIMO_PX: u64 = 100_000_000;

/// Un file `.ets`: com'è fatta l'immagine e dove sta ogni tessera.
#[derive(Debug)]
pub struct Ets {
    pub path: PathBuf,
    pub tipo: u32,
    /// Campioni per pixel dentro una tessera (3 per un RGB, 1 per un canale di fluorescenza).
    pub campioni: u32,
    pub compressione: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub larghezza: u32,
    pub altezza: u32,
    /// Le misure degli assi oltre X e Y, nell'ordine del file (tempo, piani Z, canali quando sono tre).
    pub assi: Vec<u32>,
    pub livelli: u32,
    /// Il colore del fondo dove lo scanner non ha fotografato.
    pub fondo: [u8; 3],
    /// (livello, piano, tessera x, tessera y) → (indirizzo, byte).
    tessere: HashMap<(u32, u32, u32, u32), (u64, u32)>,
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8])))
}

fn err(e: impl std::fmt::Display) -> String {
    format!("VSI: {e}")
}

impl Ets {
    /// Legge le intestazioni e l'elenco delle tessere (nessun pixel).
    pub fn open(path: &Path) -> Result<Ets, String> {
        let mut f = File::open(path).map_err(err)?;
        let lunghezza = f.metadata().map_err(err)?.len();
        let mut h = [0u8; 64];
        f.read_exact(&mut h).map_err(|_| err("file .ets troppo corto"))?;
        if &h[..4] != b"SIS\0" {
            return Err(err("non è un file .ets"));
        }
        let ndim = u32_at(&h, 12).unwrap_or(0) as usize;
        let (ets_off, ets_n) = (u64_at(&h, 16).unwrap_or(0), u32_at(&h, 24).unwrap_or(0) as u64);
        let (tiles_off, ntiles) = (u64_at(&h, 32).unwrap_or(0), u32_at(&h, 40).unwrap_or(0) as u64);
        if !(3..=16).contains(&ndim) {
            return Err(err("numero di dimensioni non valido"));
        }
        if !(40..=1 << 16).contains(&ets_n) || ets_off.checked_add(ets_n).is_none_or(|e| e > lunghezza) {
            return Err(err("intestazione ETS fuori dal file"));
        }
        let voce = 4 + 4 * ndim as u64 + 16;
        if ntiles == 0 || ntiles > 50_000_000 || tiles_off.checked_add(ntiles * voce).is_none_or(|e| e > lunghezza) {
            return Err(err("elenco delle tessere fuori dal file"));
        }
        let mut e = vec![0u8; ets_n as usize];
        f.seek(SeekFrom::Start(ets_off)).map_err(err)?;
        f.read_exact(&mut e).map_err(err)?;
        if &e[..4] != b"ETS\0" {
            return Err(err("intestazione ETS mancante"));
        }
        let tipo = u32_at(&e, 8).unwrap_or(0);
        let campioni = u32_at(&e, 12).unwrap_or(0);
        let compressione = u32_at(&e, 20).unwrap_or(u32::MAX);
        let (tile_w, tile_h) = (u32_at(&e, 28).unwrap_or(0), u32_at(&e, 32).unwrap_or(0));
        if !(1..=16384).contains(&tile_w) || !(1..=16384).contains(&tile_h) || !(1..=4).contains(&campioni) {
            return Err(err("misure delle tessere non valide"));
        }
        let fondo = [e.get(108).copied().unwrap_or(255), e.get(109).copied().unwrap_or(255), e.get(110).copied().unwrap_or(255)];
        // Le misure vere: quante, poi larghezza, altezza e gli altri assi.
        let quante = u32_at(&e, 184).unwrap_or(0) as usize;
        let misure: Vec<u32> = if (2..=12).contains(&quante) { (0..quante).filter_map(|i| u32_at(&e, 188 + 4 * i)).collect() } else { Vec::new() };
        let mut assi: Vec<u32> = misure.get(2..).map(<[u32]>::to_vec).unwrap_or_default();
        // Gli assi con misura si leggono nelle coordinate dopo x e y; l'ultima coordinata è il livello.
        if assi.len() + 3 > ndim || assi.contains(&0) {
            assi.clear();
        }
        let mut grezzo = vec![0u8; (ntiles * voce) as usize];
        f.seek(SeekFrom::Start(tiles_off)).map_err(err)?;
        f.read_exact(&mut grezzo).map_err(err)?;
        let mut tessere = HashMap::with_capacity(ntiles as usize);
        let (mut max_x, mut max_y, mut livelli) = (0u32, 0u32, 0u32);
        for k in 0..ntiles as usize {
            let b = &grezzo[k * voce as usize..(k + 1) * voce as usize];
            let c = |i: usize| u32_at(b, 4 + 4 * i).unwrap_or(0);
            let (tx, ty, livello) = (c(0), c(1), c(ndim - 1));
            let off = u64_at(b, 4 + 4 * ndim).unwrap_or(0);
            let n = u32_at(b, 12 + 4 * ndim).unwrap_or(0);
            if livello > 40 || off.checked_add(n as u64).is_none_or(|fine| fine > lunghezza) {
                continue;
            }
            let mut piano = 0u32;
            let mut fuori = false;
            for (i, misura) in assi.iter().enumerate() {
                let v = c(2 + i);
                fuori |= v >= *misura;
                piano = piano * misura + v;
            }
            if fuori {
                continue;
            }
            if livello == 0 {
                max_x = max_x.max(tx + 1);
                max_y = max_y.max(ty + 1);
            }
            livelli = livelli.max(livello + 1);
            tessere.insert((livello, piano, tx, ty), (off, n));
        }
        if tessere.is_empty() {
            return Err(err("nessuna tessera leggibile"));
        }
        let (larghezza, altezza) = match (misure.first(), misure.get(1)) {
            (Some(&w), Some(&h)) if w > 0 && h > 0 => (w, h),
            _ => (max_x * tile_w, max_y * tile_h),
        };
        Ok(Ets { path: path.to_path_buf(), tipo, campioni, compressione, tile_w, tile_h, larghezza, altezza, assi, livelli, fondo, tessere })
    }

    /// Le misure del livello `k` della piramide (ogni livello è la metà del precedente, arrotondata in giù).
    pub fn misure(&self, k: u32) -> (u32, u32) {
        ((self.larghezza >> k).max(1), (self.altezza >> k).max(1))
    }

    /// Quanti piani (tutte le combinazioni degli assi oltre X e Y).
    pub fn piani(&self) -> u32 {
        self.assi.iter().product::<u32>().max(1)
    }

    /// Indirizzo e grandezza di una tessera; `None` se lo scanner non l'ha fotografata.
    pub fn tessera(&self, livello: u32, piano: u32, tx: u32, ty: u32) -> Option<(u64, u32)> {
        self.tessere.get(&(livello, piano, tx, ty)).copied()
    }

    /// Tutte le tessere: indirizzo e grandezza (per la copia anonima).
    pub fn tutte(&self) -> impl Iterator<Item = (u64, u32)> + '_ {
        self.tessere.values().copied()
    }

    /// Vero se è una foto RGB a 8 bit (campo chiaro, etichetta, panoramica).
    pub fn rgb8(&self) -> bool {
        self.tipo == U8 && self.campioni == 3
    }

    /// Una tessera decodificata: `tile_w × tile_h × campioni` valori (gli 8 bit portati a u16 così come sono).
    pub fn decodifica(&self, f: &mut File, livello: u32, piano: u32, tx: u32, ty: u32) -> Result<Option<Vec<u16>>, String> {
        let Some((off, n)) = self.tessera(livello, piano, tx, ty) else { return Ok(None) };
        if n == 0 {
            return Ok(None);
        }
        let mut raw = vec![0u8; n as usize];
        f.seek(SeekFrom::Start(off)).map_err(err)?;
        f.read_exact(&mut raw).map_err(err)?;
        let (tw, th, k) = (self.tile_w as usize, self.tile_h as usize, self.campioni as usize);
        let want = tw * th * k;
        let (px, w, h): (Vec<u16>, usize, usize) = match self.compressione {
            RAW => {
                let v: Vec<u16> = match self.tipo {
                    U8 => raw.iter().map(|&b| u16::from(b)).collect(),
                    U16 => raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
                    t => return Err(err(format!("tipo di pixel {t} non gestito"))),
                };
                (v, tw, th)
            }
            JPEG | JPEG_LOSSLESS => {
                let mut d = jpeg_decoder::Decoder::new(std::io::Cursor::new(&raw));
                let px = d.decode().map_err(|e| err(format!("JPEG: {e}")))?;
                let info = d.info().ok_or_else(|| err("JPEG senza misure"))?;
                let (w, h) = (info.width as usize, info.height as usize);
                let v: Vec<u16> = match info.pixel_format {
                    jpeg_decoder::PixelFormat::L16 => px.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect(),
                    jpeg_decoder::PixelFormat::L8 if k == 3 => px.iter().flat_map(|&g| [u16::from(g); 3]).collect(),
                    jpeg_decoder::PixelFormat::CMYK32 => return Err(err("JPEG CMYK non gestito")),
                    _ => px.iter().map(|&b| u16::from(b)).collect(),
                };
                (v, w, h)
            }
            JP2K => {
                use hayro_jpeg2000::{DecodeSettings, DecoderContext, Image};
                let image = Image::new(&raw, &DecodeSettings::default()).map_err(|e| err(format!("JPEG 2000: {e:?}")))?;
                let (w, h) = (image.width() as usize, image.height() as usize);
                let mut ctx = DecoderContext::default();
                let decoded = image.decode(&mut ctx).map_err(|e| err(format!("JPEG 2000: {e:?}")))?;
                let comps = decoded.components();
                let massimo = |bit: u8| ((1u32 << bit.clamp(1, 16)) - 1) as f32;
                let mut v = vec![0u16; w * h * k];
                for (ci, comp) in comps.iter().take(k).enumerate() {
                    let m = massimo(comp.bit_depth());
                    for (i, s) in comp.samples().iter().take(w * h).enumerate() {
                        v[i * k + ci] = s.round().clamp(0.0, m) as u16;
                    }
                }
                if comps.len() == 1 && k == 3 {
                    for i in 0..w * h {
                        v[i * 3 + 1] = v[i * 3];
                        v[i * 3 + 2] = v[i * 3];
                    }
                }
                (v, w, h)
            }
            PNG | BMP => {
                let img = image::load_from_memory(&raw).map_err(|e| err(format!("tessera: {e}")))?;
                let (w, h) = (img.width() as usize, img.height() as usize);
                let v: Vec<u16> = match (k, self.tipo) {
                    (3, _) => img.to_rgb8().into_raw().into_iter().map(u16::from).collect(),
                    (_, U16) => img.to_luma16().into_raw(),
                    _ => img.to_luma8().into_raw().into_iter().map(u16::from).collect(),
                };
                (v, w, h)
            }
            c => return Err(err(format!("compressione {c} non gestita"))),
        };
        if (w, h) == (tw, th) && px.len() >= want {
            let mut px = px;
            px.truncate(want);
            return Ok(Some(px));
        }
        // Una tessera più piccola (ai bordi): si ricopia riga per riga in una tessera della misura piena.
        let mut out = vec![0u16; want];
        let (cw, ch) = (w.min(tw), h.min(th));
        for y in 0..ch {
            let Some(riga) = px.get(y * w * k..(y * w + cw) * k) else { break };
            out[y * tw * k..(y * tw + cw) * k].copy_from_slice(riga);
        }
        Ok(Some(out))
    }

    /// Un piano intero di un livello, un campione alla volta: `campioni` immagini `w × h`.
    pub fn leggi_piano(&self, livello: u32, piano: u32) -> Result<Vec<Vec<u16>>, String> {
        let (w, h) = self.misure(livello);
        let (tw, th, k) = (self.tile_w as usize, self.tile_h as usize, self.campioni as usize);
        let (across, rows) = (w.div_ceil(self.tile_w), h.div_ceil(self.tile_h));
        let mut out: Vec<Vec<u16>> = (0..k).map(|_| vec![0u16; w as usize * h as usize]).collect();
        let coords: Vec<(u32, u32)> = (0..rows).flat_map(|ty| (0..across).map(move |tx| (tx, ty))).collect();
        let fili = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16).min(coords.len().max(1));
        let per = coords.len().div_ceil(fili);
        let pezzi: Vec<Result<Vec<((u32, u32), Vec<u16>)>, String>> = std::thread::scope(|s| {
            let h: Vec<_> = coords
                .chunks(per.max(1))
                .map(|gruppo| {
                    s.spawn(move || -> Result<Vec<((u32, u32), Vec<u16>)>, String> {
                        let mut f = File::open(&self.path).map_err(err)?;
                        let mut v = Vec::new();
                        for &(tx, ty) in gruppo {
                            if let Some(px) = self.decodifica(&mut f, livello, piano, tx, ty)? {
                                v.push(((tx, ty), px));
                            }
                        }
                        Ok(v)
                    })
                })
                .collect();
            h.into_iter().map(|t| t.join().unwrap_or_else(|_| Err(err("lettura interrotta")))).collect()
        });
        let (w, h) = (w as usize, h as usize);
        for p in pezzi {
            for ((tx, ty), px) in p? {
                let (x0, y0) = (tx as usize * tw, ty as usize * th);
                let cw = tw.min(w.saturating_sub(x0));
                for y in 0..th.min(h.saturating_sub(y0)) {
                    for x in 0..cw {
                        for (c, piano) in out.iter_mut().enumerate() {
                            piano[(y0 + y) * w + x0 + x] = px[(y * tw + x) * k + c];
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Il .vsi e la sua cartella

/// Che cosa è un'immagine del .vsi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ruolo {
    Etichetta,
    Panoramica,
    Scansione,
}

/// Un'immagine del .vsi: il suo nome (se il .vsi lo dice), il ruolo, il file .ets e i micron per pixel.
#[derive(Debug)]
pub struct Immagine {
    pub nome: String,
    pub ruolo: Ruolo,
    pub ets: Arc<Ets>,
    pub um_per_px: Option<f64>,
}

/// La cartella dei pixel: `_<nome>_` accanto al .vsi.
pub fn cartella(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("_{stem}_"))
}

/// I file .ets della cartella, uno per `stackNNNNN` in ordine di numero (i `blob_*.ets` sono mappe di
/// messa a fuoco, non immagini).
pub fn file_ets(path: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(cartella(path)) else { return Vec::new() };
    let mut stack: Vec<(u64, PathBuf)> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().into_owned();
            let n: u64 = nome.strip_prefix("stack")?.parse().ok()?;
            e.file_type().ok()?.is_dir().then_some((n, e.path()))
        })
        .collect();
    stack.sort();
    stack
        .into_iter()
        .filter_map(|(_, d)| {
            let mut ets: Vec<PathBuf> = std::fs::read_dir(&d)
                .ok()?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("frame_t")) && crate::engine::extension(p) == "ets")
                .collect();
            ets.sort();
            ets.into_iter().next()
        })
        .collect()
}

/// Vero se è un .vsi con accanto la sua cartella e almeno un .ets.
pub fn is_vsi(path: &Path) -> bool {
    crate::engine::extension(path) == "vsi" && crate::svs::tiff_head(&crate::imgfmt::head_of(path)) && !file_ets(path).is_empty()
}

/// Le immagini nominate dentro il .vsi, in ordine, con i micron per pixel (campo 2019, due double
/// subito prima del nome, campo 2030). Maschere e mappe di fuoco non hanno pixel: si saltano.
fn nomi(vsi: &[u8]) -> Vec<(String, Option<f64>)> {
    const NOME: [u8; 8] = [0x00, 0x20, 0x00, 0x00, 0xee, 0x07, 0x00, 0x00];
    const PIXEL: [u8; 8] = [0x04, 0x01, 0x00, 0x00, 0xe3, 0x07, 0x00, 0x00];
    let mut out = Vec::new();
    let mut ultimo_px: Option<(usize, f64)> = None;
    let mut i = 0;
    while i + 16 <= vsi.len() {
        if vsi[i..i + 8] == PIXEL && u32_at(vsi, i + 12) == Some(16) {
            let v = f64::from_le_bytes(vsi[i + 16..i + 24].try_into().unwrap_or([0; 8]));
            if v.is_finite() && v > 0.0 && v < 1000.0 {
                ultimo_px = Some((i, v));
            }
            i += 24;
            continue;
        }
        if vsi[i..i + 8] == NOME {
            let n = u32_at(vsi, i + 12).unwrap_or(0) as usize;
            if n > 0 && n <= 512 && n % 2 == 0 && i + 16 + n <= vsi.len() {
                let u: Vec<u16> = vsi[i + 16..i + 16 + n].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                let nome = String::from_utf16_lossy(&u).trim_end_matches('\0').trim().to_string();
                let senza_pixel = ["Sample Mask", "FocusMap", "FocusPoints", "Focus Map", "Focus Points"].iter().any(|s| nome.contains(s));
                if !nome.is_empty() && !senza_pixel {
                    // I micron per pixel valgono se stanno poco prima del nome (nello stesso blocco).
                    let um = ultimo_px.filter(|(p, _)| i - p < 16384).map(|(_, v)| v);
                    out.push((nome, um));
                }
                ultimo_px = None;
                i += 16 + n;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Il ruolo dal nome: «Label» e «Overview» sono le foto della fotocamera del vetrino.
fn ruolo_dal_nome(nome: &str) -> Ruolo {
    let n = nome.to_ascii_lowercase();
    if n == "label" || n.starts_with("label ") || n == "etichetta" {
        Ruolo::Etichetta
    } else if n == "overview" || n.starts_with("overview ") || n == "macro" || n == "panoramica" {
        Ruolo::Panoramica
    } else {
        Ruolo::Scansione
    }
}

/// Un .vsi aperto: le sue immagini, nell'ordine delle cartelle `stack`.
#[derive(Debug)]
pub struct Vsi {
    pub immagini: Vec<Immagine>,
}

impl Vsi {
    pub fn open(path: &Path) -> Result<Vsi, String> {
        let ets = file_ets(path);
        if ets.is_empty() {
            return Err(err(format!("manca la cartella {} con i pixel accanto al file", cartella(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
        }
        let testa = std::fs::read(path).map_err(err)?;
        let nomi = nomi(&testa);
        // I nomi valgono solo se sono tanti quante le cartelle: se no (cartelle mancanti) l'ordine non è sicuro.
        let con_nomi = nomi.len() == ets.len();
        let mut immagini = Vec::new();
        let mut saltate = Vec::new();
        for (k, p) in ets.iter().enumerate() {
            match Ets::open(p) {
                Ok(e) => {
                    let (nome, um) = if con_nomi { nomi[k].clone() } else { (String::new(), None) };
                    let ruolo = if con_nomi { ruolo_dal_nome(&nome) } else { Ruolo::Scansione };
                    immagini.push(Immagine { nome, ruolo, ets: Arc::new(e), um_per_px: um });
                }
                Err(e) => saltate.push(e),
            }
        }
        if immagini.is_empty() {
            return Err(saltate.into_iter().next().unwrap_or_else(|| err("nessuna immagine leggibile")));
        }
        if !con_nomi {
            // Senza nomi: le foto RGB piccole accanto a scansioni più grandi non sono scansioni.
            let piu_grande = immagini.iter().map(|i| i.ets.larghezza as u64 * i.ets.altezza as u64).max().unwrap_or(0);
            let fluo = immagini.iter().any(|i| !i.ets.rgb8());
            for i in &mut immagini {
                if fluo && i.ets.rgb8() {
                    i.ruolo = Ruolo::Panoramica;
                } else if (i.ets.larghezza as u64 * i.ets.altezza as u64) * 4 < piu_grande && i.ets.rgb8() {
                    i.ruolo = Ruolo::Panoramica;
                }
            }
        }
        Ok(Vsi { immagini })
    }

    /// Le scansioni (le immagini vere), nell'ordine del file.
    pub fn scansioni(&self) -> Vec<&Immagine> {
        let v: Vec<&Immagine> = self.immagini.iter().filter(|i| i.ruolo == Ruolo::Scansione).collect();
        if v.is_empty() {
            // Un .vsi con solo la panoramica: meglio quella che niente.
            return self.immagini.iter().max_by_key(|i| i.ets.larghezza as u64 * i.ets.altezza as u64).into_iter().collect();
        }
        v
    }

    /// La scansione principale: la più grande (la prima, a parità).
    pub fn principale(&self) -> Option<&Immagine> {
        let s = self.scansioni();
        let max = s.iter().map(|i| i.ets.larghezza as u64 * i.ets.altezza as u64).max()?;
        s.into_iter().find(|i| i.ets.larghezza as u64 * i.ets.altezza as u64 == max)
    }
}

/// Vero se il .vsi è un vetrino in campo chiaro (la scansione principale è RGB a 8 bit).
pub fn is_brightfield(path: &Path) -> bool {
    is_vsi(path) && Vsi::open(path).ok().and_then(|v| v.principale().map(|i| i.ets.rgb8())).unwrap_or(false)
}

/// Vero se il .vsi è una fluorescenza o un'immagine a più di 8 bit (va nella microscopia).
pub fn is_fluorescenza(path: &Path) -> bool {
    is_vsi(path) && Vsi::open(path).ok().and_then(|v| v.principale().map(|i| !i.ets.rgb8())).unwrap_or(false)
}

// ---------------------------------------------------------------------------------------------
// Campo chiaro: il vetrino

/// Le pagine TIFF del .vsi con le miniature: la prima è la foto del vetrino; poi una per immagine
/// nominata (etichetta, panoramica, scansioni), con in mezzo le minuscole mappe di fuoco.
fn pagine_miniature(path: &Path) -> Vec<(usize, u32, u32)> {
    let Ok(file) = File::open(path) else { return Vec::new() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let Ok(d) = tiff::decoder::Decoder::new(std::io::BufReader::new(file)) else { return Vec::new() };
    let mut d = d.with_limits(crate::svs::tiff_limits(len));
    let mut out = Vec::new();
    for i in 0..256 {
        if let Ok((w, h)) = d.dimensions() {
            out.push((i, w, h));
        }
        if !d.more_images() || d.next_image().is_err() {
            break;
        }
    }
    out
}

/// Il vetrino di un .vsi in campo chiaro: la scansione principale con la sua piramide.
pub fn open_slide(path: &Path) -> Result<crate::svs::Slide, String> {
    use crate::svs::{Level, Slide, Source, Vendor};
    let v = Vsi::open(path)?;
    let principale = v.principale().ok_or_else(|| err("nessuna scansione"))?;
    let e = &principale.ets;
    if !e.rgb8() {
        return Err(err("la scansione non è a colori RGB a 8 bit (è una fluorescenza)"));
    }
    let (compression, photometric) = match e.compressione {
        JPEG => (crate::svs::JPEG, crate::svs::PHOTOMETRIC_YCBCR),
        JP2K => (crate::svs::JP2K_RGB, 2),
        RAW => (1, 2),
        c => return Err(err(format!("compressione {c} non gestita per i vetrini"))),
    };
    let piano_zero = 0;
    let mut levels = Vec::new();
    for k in 0..e.livelli {
        let (w, h) = e.misure(k);
        let (across, rows) = (w.div_ceil(e.tile_w), h.div_ceil(e.tile_h));
        let n = across as usize * rows as usize;
        let (mut offsets, mut counts) = (vec![0u64; n], vec![0u64; n]);
        let mut presenti = 0;
        for ty in 0..rows {
            for tx in 0..across {
                if let Some((o, c)) = e.tessera(k, piano_zero, tx, ty) {
                    let i = (ty * across + tx) as usize;
                    offsets[i] = o;
                    counts[i] = c as u64;
                    presenti += 1;
                }
            }
        }
        if presenti == 0 {
            continue;
        }
        levels.push(Level { ifd: k as usize, width: w, height: h, tile_w: e.tile_w, tile_h: e.tile_h, compression, photometric, offsets, counts, tables: None, source: Source::Vsi(Arc::new(e.path.clone())) });
    }
    if levels.is_empty() {
        return Err(err("la scansione non ha tessere"));
    }
    // Le miniature: la prima pagina è la foto del vetrino; l'etichetta e la miniatura della scansione
    // si riconoscono dall'ordine delle immagini nominate.
    let pagine: Vec<(usize, u32, u32)> = pagine_miniature(path);
    let nominate: Vec<usize> = pagine.iter().skip(1).filter(|(_, w, h)| (*w).min(*h) > 16).map(|(i, _, _)| *i).collect();
    let (mut label, mut thumbnail) = (None, None);
    if nominate.len() == v.immagini.len() {
        for (img, &pag) in v.immagini.iter().zip(&nominate) {
            if img.ruolo == Ruolo::Etichetta {
                label = Some(pag);
            }
            if std::ptr::eq(img, principale) {
                thumbnail = Some(pag);
            }
        }
    }
    let um = principale.um_per_px.map(|u| u as f32);
    Ok(Slide {
        levels,
        thumbnail,
        label,
        macro_image: pagine.first().map(|_| 0),
        mpp: um,
        // L'obiettivo dal nome della scansione («20x_BF_01»): i micron per pixel cambiano da scanner a scanner.
        magnification: obiettivo(&principale.nome),
        vendor: Vendor::Olympus,
        blobs: Vec::new(),
        fill: e.fondo,
    })
}

/// L'obiettivo scritto all'inizio del nome di una scansione: «20x_BF_01» → 20, «40x» → 40.
fn obiettivo(nome: &str) -> Option<f32> {
    let testa: String = nome.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let resto = &nome.trim()[testa.len()..];
    if !resto.starts_with(['x', 'X']) {
        return None;
    }
    testa.parse::<f32>().ok().filter(|m| (1.0..=150.0).contains(m))
}

/// Le pagine del .vsi da cancellare nella copia anonima: la foto del vetrino (la prima) e l'etichetta.
pub fn pagine_private(path: &Path) -> Vec<usize> {
    let Ok(v) = Vsi::open(path) else { return vec![0] };
    let pagine = pagine_miniature(path);
    let nominate: Vec<usize> = pagine.iter().skip(1).filter(|(_, w, h)| (*w).min(*h) > 16).map(|(i, _, _)| *i).collect();
    let mut out = vec![0];
    if nominate.len() == v.immagini.len() {
        for (img, &pag) in v.immagini.iter().zip(&nominate) {
            if img.ruolo != Ruolo::Scansione {
                out.push(pag);
            }
        }
    } else if nominate.len() > 1 {
        // Senza la corrispondenza sicura si cancellano tutte le miniature tranne quelle delle scansioni grandi:
        // meglio una miniatura in meno che un'etichetta con il nome rimasta.
        out.extend(nominate.iter().take(nominate.len().saturating_sub(v.scansioni().len())));
    }
    out
}

/// La copia anonima: il .vsi con la foto del vetrino, l'etichetta e l'autore cancellati, e la cartella
/// dei pixel con le tessere dell'etichetta e della panoramica sbiancate.
pub fn copia_anonima(path: &Path, out: &Path) -> Result<Vec<&'static str>, String> {
    let v = Vsi::open(path)?;
    let (da, a) = (cartella(path), cartella(out));
    if a.exists() {
        return Err(format!("la cartella {} esiste già", a.display()));
    }
    let fatto = (|| -> Result<Vec<&'static str>, String> {
        fn copia(da: &Path, a: &Path) -> Result<(), String> {
            std::fs::create_dir_all(a).map_err(|e| e.to_string())?;
            for e in std::fs::read_dir(da).map_err(|e| e.to_string())? {
                let e = e.map_err(|e| e.to_string())?;
                let tipo = e.file_type().map_err(|e| e.to_string())?;
                if tipo.is_dir() {
                    copia(&e.path(), &a.join(e.file_name()))?;
                } else if tipo.is_file() {
                    std::fs::copy(e.path(), a.join(e.file_name())).map_err(|e| e.to_string())?;
                } else {
                    return Err(format!("{}: file non regolare nella cartella del vetrino", e.path().display()));
                }
            }
            Ok(())
        }
        copia(&da, &a)?;
        std::fs::copy(path, out).map_err(|e| e.to_string())?;
        let mut tolti = crate::svs_lab::sbianca_pagine(out, &pagine_private(path))?;
        // Le tessere delle foto della fotocamera: un JPEG bianco al posto di ognuna (zeri se non ci sta).
        let mut sbiancate = false;
        for img in v.immagini.iter().filter(|i| i.ruolo != Ruolo::Scansione) {
            let rel = img.ets.path.strip_prefix(&da).map_err(|_| "file .ets fuori dalla cartella del vetrino")?;
            let dest = a.join(rel);
            let bianco = if img.ets.compressione == JPEG && img.ets.rgb8() {
                let mut b = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 50)
                    .encode_image(&image::RgbImage::from_pixel(img.ets.tile_w, img.ets.tile_h, image::Rgb([255, 255, 255])))
                    .map_err(|e| e.to_string())?;
                b
            } else {
                Vec::new()
            };
            sbianca_ets(&dest, &bianco, img.ets.compressione == RAW)?;
            sbiancate = true;
        }
        if sbiancate && !tolti.contains(&"etichetta") {
            tolti.insert(0, "etichetta");
        }
        Ok(tolti)
    })();
    if fatto.is_err() {
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_dir_all(&a);
    }
    fatto
}

/// Le tessere di un .ets cancellate: i loro byte diventano zeri (o bianco, se grezze) e, se c'è un JPEG
/// bianco, va in fondo al file e tutte le voci dell'elenco puntano lì, così gli altri lettori vedono
/// una foto bianca invece di tessere rotte.
fn sbianca_ets(path: &Path, bianco: &[u8], grezzo: bool) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let lunghezza = f.metadata().map_err(|e| e.to_string())?.len();
    let mut h = [0u8; 64];
    f.read_exact(&mut h).map_err(|e| e.to_string())?;
    let ndim = u32_at(&h, 12).unwrap_or(0) as usize;
    let (tiles_off, ntiles) = (u64_at(&h, 32).unwrap_or(0), u32_at(&h, 40).unwrap_or(0) as u64);
    let voce = 4 + 4 * ndim as u64 + 16;
    if !(3..=16).contains(&ndim) || tiles_off.checked_add(ntiles * voce).is_none_or(|e| e > lunghezza) {
        return Err(err("elenco delle tessere fuori dal file"));
    }
    let mut elenco = vec![0u8; (ntiles * voce) as usize];
    f.seek(SeekFrom::Start(tiles_off)).map_err(|e| e.to_string())?;
    f.read_exact(&mut elenco).map_err(|e| e.to_string())?;
    let nuovo = (!bianco.is_empty()).then_some(lunghezza);
    for k in 0..ntiles as usize {
        let v = &mut elenco[k * voce as usize..(k + 1) * voce as usize];
        let (po, pn) = (4 + 4 * ndim, 12 + 4 * ndim);
        let off = u64_at(v, po).unwrap_or(0);
        let n = u32_at(v, pn).unwrap_or(0) as u64;
        if n > 0 && off.checked_add(n).is_some_and(|e| e <= lunghezza) {
            let riempi = if grezzo { 255 } else { 0 };
            let mut resto = n;
            let mut pos = off;
            while resto > 0 {
                let m = resto.min(1 << 20);
                f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
                f.write_all(&vec![riempi; m as usize]).map_err(|e| e.to_string())?;
                pos += m;
                resto -= m;
            }
        }
        if let Some(o) = nuovo {
            v[po..po + 8].copy_from_slice(&o.to_le_bytes());
            v[pn..pn + 4].copy_from_slice(&(bianco.len() as u32).to_le_bytes());
        }
    }
    if nuovo.is_some() {
        f.seek(SeekFrom::Start(tiles_off)).map_err(|e| e.to_string())?;
        f.write_all(&elenco).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        f.write_all(bianco).map_err(|e| e.to_string())?;
    }
    f.flush().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Fluorescenza: la pila di microscopia

/// Una scansione a più canali vista come pila: assi, livello scelto, nomi dei canali.
#[derive(Debug)]
pub struct Scena {
    pub nome: String,
    pub ets: Arc<Ets>,
    /// Il livello della piramide che si legge (0 = pieno; più alto se l'immagine è troppo grande).
    pub livello: u32,
    pub larghezza: u32,
    pub altezza: u32,
    pub canali: usize,
    pub z: usize,
    pub t: usize,
    pub um_per_px: Option<f64>,
    pub bit: u8,
    /// Letta ridotta: (quante volte, larghezza e altezza piene).
    pub ridotta: Option<(u32, u32, u32)>,
    /// I nomi dei canali (dal nome della scansione, «20x_DAPI, FITC, Cy3, Cy5_01»).
    pub nomi_canali: Vec<String>,
}

impl Scena {
    pub fn da(img: &Immagine) -> Scena {
        let e = img.ets.clone();
        // Gli assi oltre X e Y: l'ultimo sono i canali; con tre, tempo, piani Z e canali (verificato con slideio).
        let (t, z, c) = match e.assi.as_slice() {
            [] => (1, 1, 1),
            [c] => (1, 1, *c),
            [z, c] => (1, *z, *c),
            [rest @ .., z, c] => (rest.iter().product::<u32>(), *z, *c),
        };
        let canali = if e.campioni > 1 { e.campioni as usize } else { c as usize };
        let mut livello = 0;
        while livello + 1 < e.livelli && {
            let (w, h) = e.misure(livello);
            w as u64 * h as u64 > MASSIMO_PX
        } {
            livello += 1;
        }
        let (larghezza, altezza) = e.misure(livello);
        let nomi_canali = nomi_canali(&img.nome, canali);
        Scena {
            nome: img.nome.clone(),
            livello,
            larghezza,
            altezza,
            canali,
            z: z as usize,
            t: t as usize,
            um_per_px: img.um_per_px.map(|u| u * f64::from(1u32 << livello)),
            bit: if e.tipo == U8 { 8 } else { 16 },
            ridotta: (livello > 0).then_some((1 << livello, e.larghezza, e.altezza)),
            nomi_canali,
            ets: e,
        }
    }

    /// Il canale `c` del piano `z` all'istante `t`.
    pub fn piano(&self, c: usize, z: usize, t: usize) -> Result<crate::microscopia::Piano, String> {
        let e = &self.ets;
        let scala = if e.tipo == U8 { 255 } else { 65535 };
        if e.campioni > 1 {
            let p = self.indice(0, z, t);
            let mut v = e.leggi_piano(self.livello, p)?;
            if c >= v.len() {
                return Err("canale non trovato".into());
            }
            return Ok(crate::microscopia::Piano { dati: v.swap_remove(c), scala });
        }
        let p = self.indice(c, z, t);
        let mut v = e.leggi_piano(self.livello, p)?;
        Ok(crate::microscopia::Piano { dati: v.swap_remove(0), scala })
    }

    /// L'indice del piano nel file: gli assi nell'ordine del file, l'ultimo il più veloce.
    fn indice(&self, c: usize, z: usize, t: usize) -> u32 {
        let e = &self.ets;
        let c = if e.campioni > 1 { 0 } else { c };
        let (c, z, t) = (c.min(self.canali.saturating_sub(1)) as u32, z.min(self.z.saturating_sub(1)) as u32, t.min(self.t.saturating_sub(1)) as u32);
        match e.assi.len() {
            0 => 0,
            1 => c,
            2 => z * e.assi[1] + c,
            _ => {
                // Il «tempo» può essere più assi in fila: qui conta solo il prodotto, nell'ordine del file.
                let zc = e.assi[e.assi.len() - 2] * e.assi[e.assi.len() - 1];
                t * zc + z * e.assi[e.assi.len() - 1] + c
            }
        }
    }
}

/// I nomi dei canali dal nome della scansione: «20x_DAPI, FITC, Cy3, Cy5_01» → DAPI, FITC, Cy3, Cy5.
fn nomi_canali(nome: &str, n: usize) -> Vec<String> {
    let mut s = nome.trim();
    // Via l'obiettivo davanti («20x_») e il numero dell'area in fondo («_01»).
    if let Some((testa, resto)) = s.split_once('_') {
        if testa.to_ascii_lowercase().trim_end_matches('x').chars().all(|c| c.is_ascii_digit() || c == '.') && !testa.is_empty() {
            s = resto;
        }
    }
    if let Some((resto, coda)) = s.rsplit_once('_') {
        if coda.chars().all(|c| c.is_ascii_digit()) && !coda.is_empty() {
            s = resto;
        }
    }
    let parti: Vec<String> = s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    if parti.len() == n && n > 1 {
        parti
    } else {
        Vec::new()
    }
}

/// Le scansioni del .vsi come pile (le fluorescenze), con i nomi per l'elenco delle serie.
pub fn scene(path: &Path) -> Result<Vec<Scena>, String> {
    let v = Vsi::open(path)?;
    Ok(v.scansioni().into_iter().map(Scena::da).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un .ets finto: tessere grezze a 16 bit, assi (t, z, c), livello in fondo alle coordinate.
    pub(crate) fn ets_finto(path: &Path, w: u32, h: u32, tile: u32, assi: &[u32], valore: impl Fn(u32, u32, u32, u32) -> u16) {
        let ndim = 2 + assi.len() + 1;
        let (across, rows) = (w.div_ceil(tile), h.div_ceil(tile));
        let piani: u32 = assi.iter().product::<u32>().max(1);
        let mut ets = vec![0u8; 228];
        ets[..4].copy_from_slice(b"ETS\0");
        let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut ets, 4, 0x30006);
        put(&mut ets, 8, U16);
        put(&mut ets, 12, 1);
        put(&mut ets, 16, 1);
        put(&mut ets, 20, RAW);
        put(&mut ets, 28, tile);
        put(&mut ets, 32, tile);
        put(&mut ets, 36, 1);
        put(&mut ets, 184, 2 + assi.len() as u32);
        put(&mut ets, 188, w);
        put(&mut ets, 192, h);
        for (i, a) in assi.iter().enumerate() {
            put(&mut ets, 196 + 4 * i, *a);
        }
        let mut dati = Vec::new();
        let mut voci = Vec::new();
        let base = 64 + ets.len() as u64;
        for p in 0..piani {
            for ty in 0..rows {
                for tx in 0..across {
                    let off = base + dati.len() as u64;
                    for y in 0..tile {
                        for x in 0..tile {
                            dati.extend_from_slice(&valore(p, tx * tile + x, ty * tile + y, 0).to_le_bytes());
                        }
                    }
                    let mut v = vec![0u8; 4];
                    v.extend_from_slice(&tx.to_le_bytes());
                    v.extend_from_slice(&ty.to_le_bytes());
                    let mut resto = p;
                    let mut coord = vec![0u32; assi.len()];
                    for i in (0..assi.len()).rev() {
                        coord[i] = resto % assi[i];
                        resto /= assi[i];
                    }
                    for c in coord {
                        v.extend_from_slice(&c.to_le_bytes());
                    }
                    v.extend_from_slice(&0u32.to_le_bytes());
                    v.extend_from_slice(&off.to_le_bytes());
                    v.extend_from_slice(&((tile * tile * 2) as u32).to_le_bytes());
                    v.extend_from_slice(&0u32.to_le_bytes());
                    voci.push(v);
                }
            }
        }
        let tiles_off = base + dati.len() as u64;
        let mut f = Vec::new();
        f.extend_from_slice(b"SIS\0");
        f.extend_from_slice(&64u32.to_le_bytes());
        f.extend_from_slice(&2u32.to_le_bytes());
        f.extend_from_slice(&(ndim as u32).to_le_bytes());
        f.extend_from_slice(&64u64.to_le_bytes());
        f.extend_from_slice(&(ets.len() as u32).to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&tiles_off.to_le_bytes());
        f.extend_from_slice(&(voci.len() as u32).to_le_bytes());
        f.resize(64, 0);
        f.extend_from_slice(&ets);
        f.extend_from_slice(&dati);
        for v in voci {
            f.extend_from_slice(&v);
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, f).unwrap();
    }

    #[test]
    fn le_tessere_si_rimettono_al_posto_giusto_con_gli_assi() {
        let dir = std::env::temp_dir().join(format!("cedro-vsi-{}", std::process::id()));
        let p = dir.join("_x_/stack1/frame_t_0.ets");
        // 2 istanti, 3 piani Z, 2 canali; 70×50 pixel in tessere da 32 (l'ultima colonna e riga a metà).
        ets_finto(&p, 70, 50, 32, &[2, 3, 2], |piano, x, y, _| (piano * 1000 + y * 70 + x) as u16);
        let e = Ets::open(&p).unwrap();
        assert_eq!((e.larghezza, e.altezza, e.assi.clone(), e.piani()), (70, 50, vec![2, 3, 2], 12));
        let img = Immagine { nome: "20x_DAPI, GFP_01".into(), ruolo: Ruolo::Scansione, ets: Arc::new(e), um_per_px: Some(0.5) };
        let s = Scena::da(&img);
        assert_eq!((s.canali, s.z, s.t), (2, 3, 2));
        assert_eq!(s.nomi_canali, vec!["DAPI".to_string(), "GFP".to_string()]);
        // Canale 1, piano Z 2, istante 1 → piano del file (1·3 + 2)·2 + 1 = 11.
        let piano = s.piano(1, 2, 1).unwrap();
        assert_eq!(piano.dati.len(), 70 * 50);
        assert_eq!(piano.dati[0], 11_000);
        assert_eq!(piano.dati[49 * 70 + 69], (11_000 + 49 * 70 + 69) as u16);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn l_obiettivo_viene_dal_nome() {
        assert_eq!(obiettivo("20x_BF_01"), Some(20.0));
        assert_eq!(obiettivo("40X"), Some(40.0));
        assert_eq!(obiettivo("488/640_50um_1x"), None);
        assert_eq!(obiettivo("Overview"), None);
    }

    #[test]
    fn i_nomi_dei_canali_vengono_dal_nome_della_scansione() {
        assert_eq!(nomi_canali("20x_DAPI, FITC, Cy3, Cy5_01", 4), vec!["DAPI", "FITC", "Cy3", "Cy5"]);
        assert!(nomi_canali("20x_BF_01", 3).is_empty());
        assert!(nomi_canali("488/640_50um_1x", 2).is_empty());
    }

    #[test]
    fn un_ets_rovinato_non_fa_danni() {
        let dir = std::env::temp_dir().join(format!("cedro-vsi-rov-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("frame_t.ets");
        std::fs::write(&p, b"SIS\0\x40\0\0\0\x02\0\0\0\xff\xff\xff\xff").unwrap();
        assert!(Ets::open(&p).is_err());
        let mut b = vec![0u8; 64];
        b[..4].copy_from_slice(b"SIS\0");
        b[12] = 4;
        b[16] = 64;
        b[24] = 0xff;
        b[25] = 0xff;
        std::fs::write(&p, &b).unwrap();
        assert!(Ets::open(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Prova a mano sui campioni veri: `CEDRO_VSI=/percorso/file.vsi cargo test --lib -- --ignored vsi_veri --nocapture`.
    #[test]
    #[ignore]
    fn vsi_veri_a_mano() {
        let Ok(elenco) = std::env::var("CEDRO_VSI") else { return };
        for p in elenco.split(':') {
            let p = Path::new(p);
            let v = Vsi::open(p).unwrap();
            for i in &v.immagini {
                println!("{} «{}» {:?} {}×{} assi={:?} livelli={} tipo={} camp={} compr={} um={:?}", p.display(), i.nome, i.ruolo, i.ets.larghezza, i.ets.altezza, i.ets.assi, i.ets.livelli, i.ets.tipo, i.ets.campioni, i.ets.compressione, i.um_per_px);
            }
        }
    }
}

#[cfg(test)]
mod confronto {
    use super::*;

    /// Scrive per il confronto con slideio: `CEDRO_VSI_CONFRONTO=file.vsi:cartella_uscita`.
    #[test]
    #[ignore]
    fn vsi_confronto_a_mano() {
        let Ok(v) = std::env::var("CEDRO_VSI_CONFRONTO") else { return };
        let (p, out) = v.split_once(':').unwrap();
        let (p, out) = (Path::new(p), Path::new(out));
        std::fs::create_dir_all(out).unwrap();
        if crate::svs::is_slide(p) {
            let s = crate::svs::Slide::open(p).unwrap();
            println!("vetrino {} livelli, mpp {:?}, ingr {:?}, label {:?} macro {:?} thumb {:?}", s.levels.len(), s.mpp, s.magnification, s.label, s.macro_image, s.thumbnail);
            for (k, l) in s.levels.iter().enumerate() {
                println!("  livello {k}: {}×{}", l.width, l.height);
            }
            let k = 2.min(s.levels.len() - 1);
            s.read_level(p, k).unwrap().save(out.join(format!("livello{k}.png"))).unwrap();
            let l0 = &s.levels[0];
            s.read_region(p, 0, l0.width / 2, l0.height / 2, 1000, 700).unwrap().save(out.join("zona0.png")).unwrap();
            if let Some(l) = s.label {
                s.read_extra(p, l).unwrap().save(out.join("etichetta.png")).unwrap();
            }
        } else {
            let pile = crate::microscopia::Pila::open_tutte(p).unwrap();
            for (k, pila) in pile.iter().enumerate() {
                println!("pila {k}: {}×{} canali {:?} z {} t {} bit {} um {:?} avviso {:?}", pila.larghezza, pila.altezza, pila.canali, pila.z, pila.t, pila.bit, pila.um_per_px, pila.avviso);
            }
            let pila = &pile[0];
            let c = pila.canali.len() - 1;
            let (z, t) = (pila.z / 2, pila.t.saturating_sub(1));
            let piano = pila.piano_z(p, c, z, t).unwrap();
            let byte: Vec<u8> = piano.dati.iter().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(out.join(format!("piano_c{c}_z{z}_t{t}.u16")), byte).unwrap();
        }
    }
}
