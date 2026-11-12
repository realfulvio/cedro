//! I vetrini digitali Aperio (.svs): un TIFF (o BigTIFF) con più pagine. La prima è il vetrino a
//! piena risoluzione, a tessere; poi la miniatura, i livelli ridotti (sempre a tessere), l'etichetta
//! e la foto del vetrino intero («macro»).
//!
//! Il livello pieno arriva a miliardi di pixel: non si apre mai intero. Si legge una fila di tessere
//! alla volta, le tessere della fila si decodificano in parallelo su tutti i core e la fila passa a
//! chi scrive il file (PNG, JPEG e TIFF si scrivono a strisce). Le tessere JPEG hanno le tabelle in
//! comune (`JPEGTables`) e i colori già in RGB anche se il JPEG dice YCbCr; quelle JPEG 2000
//! (33003 YCbCr, 33005 RGB) passano da hayro-jpeg2000. Tutto in Rust.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use image::RgbImage;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;
use tiff::ColorType;

/// Le compressioni delle tessere.
const JPEG: u16 = 7;
const JP2K_YCBCR: u16 = 33003;
const JP2K_RGB: u16 = 33005;
/// La fotometria YCbCr (6) del TIFF; RGB è 2.
const PHOTOMETRIC_YCBCR: u16 = 6;

/// Un livello della piramide: una pagina a tessere.
#[derive(Clone, Debug)]
pub struct Level {
    pub ifd: usize,
    pub width: u32,
    pub height: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub compression: u16,
    photometric: u16,
    offsets: Vec<u64>,
    counts: Vec<u64>,
    tables: Option<Vec<u8>>,
}

impl Level {
    pub fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    fn across(&self) -> u32 {
        self.width.div_ceil(self.tile_w)
    }

    fn rows(&self) -> u32 {
        self.height.div_ceil(self.tile_h)
    }
}

/// Un vetrino aperto: dove stanno i livelli e le immagini in più.
#[derive(Clone, Debug)]
pub struct Slide {
    /// Dal più grande al più piccolo.
    pub levels: Vec<Level>,
    pub thumbnail: Option<usize>,
    pub label: Option<usize>,
    pub macro_image: Option<usize>,
    /// Micrometri per pixel del livello pieno.
    pub mpp: Option<f32>,
    /// L'ingrandimento dello scanner (20×, 40×).
    pub magnification: Option<f32>,
}

/// Quale livello si vuole: le tre voci del riquadro «Vetrino».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pick {
    /// Il livello più grande con il lato lungo entro `OVERVIEW_SIDE`.
    Overview,
    /// Il livello più grande che sta comodo in memoria (`MEDIUM_PIXELS`).
    #[default]
    Medium,
    /// Il livello pieno.
    Full,
}

impl Pick {
    pub fn from_value(v: u32) -> Pick {
        match v {
            0 => Pick::Overview,
            2 => Pick::Full,
            _ => Pick::Medium,
        }
    }
}

/// Il lato lungo massimo della «Panoramica».
pub const OVERVIEW_SIDE: u32 = 4096;
/// I pixel massimi della «Media»: circa 300 MB in RGB.
pub const MEDIUM_PIXELS: u64 = 100_000_000;
/// Oltre questi pixel un livello non si apre intero in memoria: si scrive a strisce.
pub const IN_MEMORY_PIXELS: u64 = 160_000_000;

thread_local! {
    /// Il limite di `IN_MEMORY_PIXELS` per questo thread: le prove lo abbassano per passare dalle strisce con file piccoli.
    static MEMORY_LIMIT: std::cell::Cell<u64> = const { std::cell::Cell::new(IN_MEMORY_PIXELS) };
}

/// I pixel massimi di un livello da aprire intero.
pub fn memory_limit() -> u64 {
    MEMORY_LIMIT.with(|m| m.get())
}

#[cfg(test)]
pub fn set_memory_limit(pixels: u64) {
    MEMORY_LIMIT.with(|m| m.set(pixels));
}

fn err(e: impl std::fmt::Display) -> String {
    format!("vetrino non letto: {e}")
}

fn decoder(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    // I vetrini grandi hanno centinaia di migliaia di tessere: l'elenco degli indirizzi supera i limiti di partenza.
    Ok(Decoder::new(BufReader::new(file))
        .map_err(err)?
        .with_limits(Limits::unlimited()))
}

/// I primi byte dicono TIFF o BigTIFF.
fn tiff_head(head: &[u8]) -> bool {
    head.starts_with(b"II*\0")
        || head.starts_with(b"MM\0*")
        || head.starts_with(b"II+\0")
        || head.starts_with(b"MM\0+")
}

/// Vero se il file è un vetrino Aperio: un TIFF la cui prima pagina si descrive come «Aperio».
/// Legge solo l'intestazione e la prima pagina.
pub fn is_slide(path: &Path) -> bool {
    if !tiff_head(&crate::imgfmt::head_of(path)) {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mut d) = Decoder::new(BufReader::new(file)) else {
        return false;
    };
    d.get_tag_ascii_string(Tag::ImageDescription)
        .is_ok_and(|s| s.starts_with("Aperio"))
}

/// Il valore di un campo della descrizione Aperio: `|MPP = 0.4990|AppMag = 20|`.
fn field(desc: &str, key: &str) -> Option<f32> {
    desc.split('|')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .and_then(|(_, v)| v.trim().parse().ok())
}

impl Slide {
    /// Apre il vetrino e ne legge la struttura (nessun pixel).
    pub fn open(path: &Path) -> Result<Slide, String> {
        let mut d = decoder(path)?;
        let first = d
            .get_tag_ascii_string(Tag::ImageDescription)
            .unwrap_or_default();
        if !first.starts_with("Aperio") {
            return Err("non è un vetrino Aperio".into());
        }
        let mut slide = Slide {
            levels: Vec::new(),
            thumbnail: None,
            label: None,
            macro_image: None,
            mpp: field(&first, "MPP"),
            magnification: field(&first, "AppMag"),
        };
        let mut ifd = 0usize;
        loop {
            let desc = d
                .get_tag_ascii_string(Tag::ImageDescription)
                .unwrap_or_default();
            let tiled = d.find_tag(Tag::TileWidth).ok().flatten().is_some();
            if tiled {
                let (width, height) = d.dimensions().map_err(err)?;
                let tile_w = d.get_tag_u32(Tag::TileWidth).map_err(err)?;
                let tile_h = d.get_tag_u32(Tag::TileLength).map_err(err)?;
                let compression = d.get_tag_u32(Tag::Compression).unwrap_or(1) as u16;
                let photometric = d.get_tag_u32(Tag::PhotometricInterpretation).unwrap_or(2) as u16;
                let samples = d.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1);
                let planar = d.get_tag_u32(Tag::PlanarConfiguration).unwrap_or(1);
                let offsets = d.get_tag_u64_vec(Tag::TileOffsets).map_err(err)?;
                let counts = d.get_tag_u64_vec(Tag::TileByteCounts).map_err(err)?;
                let tables = d
                    .get_tag_u8_vec(Tag::JPEGTables)
                    .ok()
                    .filter(|t| t.len() >= 4);
                let level = Level {
                    ifd,
                    width,
                    height,
                    tile_w,
                    tile_h,
                    compression,
                    photometric,
                    offsets,
                    counts,
                    tables,
                };
                let need = level.across() as usize * level.rows() as usize;
                if samples != 3
                    || planar != 1
                    || tile_w == 0
                    || tile_h == 0
                    || level.offsets.len() < need
                    || level.counts.len() < need
                {
                    // Un livello che non sappiamo leggere: si salta, gli altri restano.
                    if ifd == 0 {
                        return Err("il livello pieno non è a colori RGB a 8 bit".into());
                    }
                } else {
                    slide.levels.push(level);
                }
            } else if desc.contains("label") {
                slide.label = Some(ifd);
            } else if desc.contains("macro") {
                slide.macro_image = Some(ifd);
            } else if ifd == 1 {
                slide.thumbnail = Some(ifd);
            }
            if !d.more_images() {
                break;
            }
            d.next_image().map_err(err)?;
            ifd += 1;
        }
        if slide.levels.is_empty() {
            return Err("il vetrino non ha livelli a tessere".into());
        }
        // Dal più grande al più piccolo, qualunque sia l'ordine nel file.
        slide.levels.sort_by_key(|l| std::cmp::Reverse(l.pixels()));
        Ok(slide)
    }

    pub fn full(&self) -> &Level {
        &self.levels[0]
    }

    /// Il livello che corrisponde alla scelta.
    pub fn pick(&self, pick: Pick) -> usize {
        let last = self.levels.len() - 1;
        match pick {
            Pick::Full => 0,
            Pick::Medium => self
                .levels
                .iter()
                .position(|l| l.pixels() <= MEDIUM_PIXELS)
                .unwrap_or(last),
            Pick::Overview => self
                .levels
                .iter()
                .position(|l| l.width.max(l.height) <= OVERVIEW_SIDE)
                .unwrap_or(last),
        }
    }

    /// Il livello da leggere per arrivare a `side` di lato lungo partendo da `from`: il più piccolo che
    /// è ancora almeno grande così (poi si rimpicciolisce di poco), per non leggere pixel inutili.
    pub fn for_side(&self, from: usize, side: u32) -> usize {
        (from..self.levels.len())
            .rev()
            .find(|&i| self.levels[i].width.max(self.levels[i].height) >= side)
            .unwrap_or(from)
    }

    /// La compressione delle tessere, in parole: «JPEG», «JPEG 2000».
    pub fn tiles_name(&self) -> &'static str {
        self.tiles_in(false)
    }

    fn tiles_in(&self, english: bool) -> &'static str {
        match (self.full().compression, english) {
            (JPEG, _) => "JPEG",
            (JP2K_YCBCR | JP2K_RGB, _) => "JPEG 2000",
            (1, false) => "non compresse",
            (1, true) => "uncompressed",
            (5, _) => "LZW",
            (8 | 32946, _) => "Deflate",
            (_, false) => "compresse",
            (_, true) => "compressed",
        }
    }

    /// La riga che descrive il vetrino nella coda: «Aperio · 3 livelli · tessere JPEG · 20× (0,499 µm/px)»;
    /// in inglese «Aperio · 3 levels · JPEG tiles · 20× (0.499 µm/px)».
    pub fn summary(&self, english: bool) -> String {
        let n = self.levels.len();
        let tiles = self.tiles_in(english);
        let mut s = match (english, n) {
            (false, 1) => format!("Aperio · 1 livello · tessere {tiles}"),
            (false, _) => format!("Aperio · {n} livelli · tessere {tiles}"),
            (true, 1) => format!("Aperio · 1 level · {tiles} tiles"),
            (true, _) => format!("Aperio · {n} levels · {tiles} tiles"),
        };
        let num = |v: f32| {
            if english {
                trim_num(v)
            } else {
                trim_num(v).replace('.', ",")
            }
        };
        match (self.magnification, self.mpp) {
            (Some(m), Some(p)) => s.push_str(&format!(" · {}× ({} µm/px)", num(m), num(p))),
            (Some(m), None) => s.push_str(&format!(" · {}×", num(m))),
            (None, Some(p)) => s.push_str(&format!(" · {} µm/px", num(p))),
            _ => {}
        }
        s
    }

    /// Legge un livello intero in memoria.
    pub fn read_level(&self, path: &Path, level: usize) -> Result<RgbImage, String> {
        let l = &self.levels[level];
        if l.pixels() > memory_limit().max(IN_MEMORY_PIXELS) * 2 {
            return Err(format!(
                "il livello {} × {} è troppo grande da aprire intero",
                l.width, l.height
            ));
        }
        let mut out = vec![0u8; l.pixels() as usize * 3];
        let row_bytes = l.width as usize * 3;
        self.read_rows(path, level, |y, rows| {
            let start = y as usize * row_bytes;
            out[start..start + rows.len()].copy_from_slice(rows);
            Ok(())
        })?;
        RgbImage::from_raw(l.width, l.height, out)
            .ok_or_else(|| "misure del livello non valide".into())
    }

    /// Passa a `sink` il livello una fila di tessere alla volta, dall'alto: la y della prima riga e i
    /// pixel RGB delle righe (larghezza intera del livello). Le tessere di una fila si decodificano in
    /// parallelo.
    pub fn read_rows(
        &self,
        path: &Path,
        level: usize,
        mut sink: impl FnMut(u32, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        let l = &self.levels[level];
        // Si legge una fila avanti in un altro thread: mentre chi scrive comprime una fila, la
        // successiva si sta già decodificando.
        let (tx, rx) = std::sync::mpsc::sync_channel::<(u32, Vec<u8>)>(1);
        std::thread::scope(|scope| {
            let reader = scope.spawn(move || {
                self.read_window(path, level, (0, 0, l.width, l.height), |y, rows| {
                    // Chi scrive si è fermato (errore): non serve leggere oltre.
                    tx.send((y, rows.to_vec())).map_err(|_| String::new())
                })
            });
            let mut written = Ok(());
            for (y, rows) in rx.iter() {
                written = sink(y, &rows);
                if written.is_err() {
                    break;
                }
            }
            // Chiudere il canale sblocca il lettore se chi scrive si è fermato prima.
            drop(rx);
            let read = reader.join().unwrap_or_else(|_| Err("lettura interrotta".into()));
            written?;
            read
        })
    }

    /// Una zona di un livello (`x`, `y`, larghezza, altezza in pixel del livello), letta solo dalle tessere che la coprono.
    pub fn read_region(
        &self,
        path: &Path,
        level: usize,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Result<RgbImage, String> {
        let l = &self.levels[level];
        if w == 0 || h == 0 || x.saturating_add(w) > l.width || y.saturating_add(h) > l.height {
            return Err(format!("zona fuori dal livello {} × {}", l.width, l.height));
        }
        if w as u64 * h as u64 > IN_MEMORY_PIXELS * 2 {
            return Err("zona troppo grande da aprire intera".into());
        }
        let mut out = vec![0u8; w as usize * h as usize * 3];
        let row_bytes = w as usize * 3;
        self.read_window(path, level, (x, y, w, h), |top, rows| {
            let start = (top - y) as usize * row_bytes;
            out[start..start + rows.len()].copy_from_slice(rows);
            Ok(())
        })?;
        RgbImage::from_raw(w, h, out).ok_or_else(|| "misure della zona non valide".into())
    }

    /// Il cuore della lettura: la finestra `win` (x, y, larghezza, altezza) a strisce, una fila di tessere alla volta.
    fn read_window(
        &self,
        path: &Path,
        level: usize,
        win: (u32, u32, u32, u32),
        mut sink: impl FnMut(u32, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        let l = &self.levels[level];
        let (wx, wy, ww, wh) = (
            win.0 as usize,
            win.1 as usize,
            win.2 as usize,
            win.3 as usize,
        );
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let lunghezza_file = file.metadata().map_err(|e| e.to_string())?.len();
        let across = l.across() as usize;
        let (tw, th) = (l.tile_w as usize, l.tile_h as usize);
        let (c0, c1) = (wx / tw, (wx + ww).div_ceil(tw));
        let (r0, r1) = (wy / th, (wy + wh).div_ceil(th));
        let cols = c1 - c0;
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, cols.max(1));
        // Per le compressioni rare (LZW, Deflate…) c'è il lettore del crate tiff, uno per thread.
        let generic = !matches!(l.compression, JPEG | JP2K_YCBCR | JP2K_RGB | 1);
        let band_w = ww * 3;
        let mut band = Vec::new();
        for row in r0..r1 {
            // Le righe della finestra che stanno in questa fila di tessere.
            let top = (row * th).max(wy);
            let bottom = ((row + 1) * th).min(wy + wh);
            // Le tessere della fila, lette dal disco in fila (sono di solito una dopo l'altra).
            let mut raw: Vec<Vec<u8>> = Vec::with_capacity(cols);
            if !generic {
                for col in c0..c1 {
                    let i = row * across + col;
                    let (off, len) = (l.offsets[i], l.counts[i]);
                    // Un indirizzo o una grandezza fuori dal file (file rovinato o tagliato): prima di
                    // prendere la memoria, non dopo.
                    if len > 0 && off.checked_add(len).is_none_or(|fine| fine > lunghezza_file) {
                        return Err(format!("tessera {i} fuori dal file (vetrino tagliato o rovinato)"));
                    }
                    let mut buf = vec![0u8; len as usize];
                    if len > 0 {
                        file.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
                        file.read_exact(&mut buf)
                            .map_err(|e| format!("tessera {i} non letta: {e}"))?;
                    }
                    raw.push(buf);
                }
            }
            band.clear();
            band.resize(ww * (bottom - top) * 3, 255);
            // Ogni thread decodifica un gruppo di tessere vicine.
            let per = cols.div_ceil(workers);
            let results: Vec<Result<Vec<(usize, Vec<u8>)>, String>> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..workers)
                    .map(|wk| {
                        let raw = &raw;
                        s.spawn(move || -> Result<Vec<(usize, Vec<u8>)>, String> {
                            let mut done = Vec::new();
                            let mut dec = if generic { Some(decoder(path)?) } else { None };
                            if let Some(d) = dec.as_mut() {
                                d.seek_to_image(l.ifd).map_err(err)?;
                            }
                            for k in wk * per..((wk + 1) * per).min(cols) {
                                let i = row * across + c0 + k;
                                let px = match dec.as_mut() {
                                    Some(d) => chunk_rgb(d, i as u32, tw * th)?,
                                    None => decode_tile(l, &raw[k], tw, th)
                                        .map_err(|e| format!("tessera {i}: {e}"))?,
                                };
                                done.push((c0 + k, px));
                            }
                            Ok(done)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err("decodifica interrotta".into()))
                    })
                    .collect()
            });
            for r in results {
                for (col, px) in r? {
                    // La parte della tessera che cade nella finestra.
                    let tx0 = (col * tw).max(wx);
                    let tx1 = ((col + 1) * tw).min(wx + ww).min(l.width as usize);
                    if tx1 <= tx0 {
                        continue;
                    }
                    let n = (tx1 - tx0) * 3;
                    for yy in top..bottom {
                        let src = (yy - row * th) * tw * 3 + (tx0 - col * tw) * 3;
                        let dst = (yy - top) * band_w + (tx0 - wx) * 3;
                        band[dst..dst + n].copy_from_slice(&px[src..src + n]);
                    }
                }
            }
            sink(top as u32, &band)?;
        }
        Ok(())
    }

    /// Una delle immagini in più (miniatura, etichetta, macro), letta intera: sono piccole.
    pub fn read_extra(&self, path: &Path, ifd: usize) -> Result<RgbImage, String> {
        let mut d = decoder(path)?;
        d.seek_to_image(ifd).map_err(err)?;
        let (w, h) = d.dimensions().map_err(err)?;
        let color = d.colortype().map_err(err)?;
        let DecodingResult::U8(px) = d.read_image().map_err(err)? else {
            return Err("immagine a più di 8 bit".into());
        };
        to_rgb(w, h, color, px).ok_or_else(|| format!("colori non gestiti: {color:?}"))
    }

    /// Una miniatura con il lato lungo vicino a `side`: la miniatura del file se basta, se no il
    /// livello più piccolo, rimpicciolito.
    pub fn preview(&self, path: &Path, side: u32) -> Result<RgbImage, String> {
        let smallest = self.levels.len() - 1;
        let from_thumb = self.thumbnail.and_then(|t| self.read_extra(path, t).ok());
        let img = match from_thumb {
            Some(t)
                if t.width().max(t.height()) >= side
                    || self.levels[smallest].pixels() > 40_000_000 =>
            {
                t
            }
            _ => self.read_level(path, self.for_side(smallest, side))?,
        };
        Ok(if img.width().max(img.height()) > side {
            image::DynamicImage::ImageRgb8(img)
                .thumbnail(side, side)
                .to_rgb8()
        } else {
            img
        })
    }
}

/// Come scrivere un livello troppo grande per la memoria: un formato che si scrive a strisce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Streamed {
    Png,
    /// Con la qualità (fino a 65.535 px di lato).
    Jpeg(u8),
    /// Deflate; BigTIFF quando i pixel non stanno nei 4 GB del TIFF classico.
    Tiff,
}

/// Il lato massimo di un JPEG.
pub const JPEG_MAX_SIDE: u32 = 65_535;

impl Slide {
    /// Scrive un livello in `out` senza aprirlo intero: una fila di tessere alla volta.
    pub fn write_streamed(
        &self,
        path: &Path,
        level: usize,
        out: &Path,
        how: Streamed,
    ) -> Result<(), String> {
        let l = &self.levels[level];
        let file = File::create(out).map_err(|e| e.to_string())?;
        let w = std::io::BufWriter::with_capacity(1 << 20, file);
        match how {
            Streamed::Png => {
                use std::io::Write;
                let mut enc = png::Encoder::new(w, l.width, l.height);
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                // Veloce: sul livello pieno la compressione a un solo core è il collo di bottiglia.
                enc.set_compression(png::Compression::Fast);
                let mut header = enc.write_header().map_err(|e| e.to_string())?;
                let mut stream = header.stream_writer().map_err(|e| e.to_string())?;
                self.read_rows(path, level, |_, rows| {
                    stream.write_all(rows).map_err(|e| e.to_string())
                })?;
                stream.finish().map_err(|e| e.to_string())
            }
            Streamed::Jpeg(quality) => {
                if l.width.max(l.height) > JPEG_MAX_SIDE {
                    return Err(format!("il JPEG si ferma a {JPEG_MAX_SIDE} px di lato"));
                }
                write_jpeg(self, path, level, w, quality)
            }
            // Oltre circa 3,5 GB di pixel serve il BigTIFF (indirizzi a 64 bit).
            Streamed::Tiff => write_tiff(self, path, level, w, l.pixels() * 3 > 3_500_000_000),
        }
    }
}

/// Il JPEG di un livello grande, codificato a fette in parallelo. Ogni fetta (poche file di blocchi da
/// 8 × 8) è un JPEG a sé con le stesse tabelle; i dati delle fette si mettono uno dopo l'altro con i
/// marcatori di ripartenza (RST), che ogni lettore JPEG conosce: è un JPEG normale, e la codifica,
/// la parte lenta, usa tutti i core.
fn write_jpeg<W: std::io::Write>(s: &Slide, path: &Path, level: usize, mut w: W, quality: u8) -> Result<(), String> {
    let l = &s.levels[level];
    let (mcu_w, mcu_h) = jpeg_mcu(quality)?;
    let per_row = l.width.div_ceil(mcu_w);
    // L'intervallo di ripartenza si conta in blocchi e sta in 16 bit.
    let rows_per_slice = (65_535 / per_row).clamp(1, 8);
    let slice_h = (mcu_h * rows_per_slice) as usize;
    let row_bytes = l.width as usize * 3;
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    // Si codificano insieme abbastanza fette da tenere occupati tutti i core.
    let batch = slice_h * workers * 2;
    let mut st = JpegSlices { w: &mut w, height: l.height, restart: (per_row * rows_per_slice) as u16, header: None, written: 0 };
    let mut pending: Vec<u8> = Vec::with_capacity((batch + l.tile_h as usize) * row_bytes);
    s.read_rows(path, level, |_, rows| {
        pending.extend_from_slice(rows);
        if pending.len() / row_bytes >= batch {
            let whole = pending.len() / row_bytes / slice_h * slice_h;
            st.encode(&pending[..whole * row_bytes], l.width, slice_h, quality, workers)?;
            pending.drain(..whole * row_bytes);
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        st.encode(&pending, l.width, slice_h, quality, workers)?;
    }
    let io = |e: std::io::Error| e.to_string();
    w.write_all(&[0xFF, 0xD9]).map_err(io)?;
    w.flush().map_err(io)
}

/// Le misure di un blocco (MCU) del codificatore JPEG, lette da un JPEG di prova.
fn jpeg_mcu(quality: u8) -> Result<(u32, u32), String> {
    let mut probe = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut probe, quality).encode(&[0u8; 16 * 16 * 3], 16, 16, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    let (head, _) = split_jpeg(&probe)?;
    let sof = find_marker(head, 0xC0).ok_or("JPEG senza SOF0")?;
    let n = head[sof + 9] as usize;
    let (mut h, mut v) = (1u32, 1u32);
    for c in 0..n {
        let f = head[sof + 11 + c * 3];
        h = h.max((f >> 4) as u32);
        v = v.max((f & 15) as u32);
    }
    Ok((8 * h, 8 * v))
}

/// Dove inizia (dal byte 0xFF) il segmento `marker` nell'intestazione di un JPEG.
fn find_marker(head: &[u8], marker: u8) -> Option<usize> {
    let mut i = 2;
    while i + 4 <= head.len() {
        if head[i] != 0xFF {
            return None;
        }
        if head[i + 1] == marker {
            return Some(i);
        }
        i += 2 + u16::from_be_bytes([head[i + 2], head[i + 3]]) as usize;
    }
    None
}

/// Un JPEG diviso in intestazione (fino al segmento SOS compreso) e dati compressi (senza EOI).
fn split_jpeg(b: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let sos = find_marker(b, 0xDA).ok_or("JPEG senza SOS")?;
    let start = sos + 2 + u16::from_be_bytes([b[sos + 2], b[sos + 3]]) as usize;
    if b.len() < start + 2 || b[b.len() - 2..] != [0xFF, 0xD9] {
        return Err("JPEG senza EOI".into());
    }
    Ok((&b[..start], &b[start..b.len() - 2]))
}

/// Il JPEG a fette mentre si scrive.
struct JpegSlices<'a, W: std::io::Write> {
    w: &'a mut W,
    height: u32,
    /// Blocchi per fetta (intervallo di ripartenza).
    restart: u16,
    /// L'intestazione della prima fetta senza l'altezza: tutte le altre devono avere le stesse tabelle.
    header: Option<Vec<u8>>,
    written: u32,
}

impl<W: std::io::Write> JpegSlices<'_, W> {
    /// Codifica in parallelo le righe `px` a fette di `slice_h` righe e ne scrive i dati in ordine.
    fn encode(&mut self, px: &[u8], width: u32, slice_h: usize, quality: u8, workers: usize) -> Result<(), String> {
        let row_bytes = width as usize * 3;
        let slices: Vec<&[u8]> = px.chunks(slice_h * row_bytes).collect();
        let mut done: Vec<Result<Vec<u8>, String>> = Vec::with_capacity(slices.len());
        std::thread::scope(|scope| {
            let per = slices.len().div_ceil(workers.max(1)).max(1);
            let handles: Vec<_> = slices
                .chunks(per)
                .map(|group| {
                    scope.spawn(move || {
                        group
                            .iter()
                            .map(|px| {
                                let mut out = Vec::with_capacity(px.len() / 6);
                                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode(px, width, (px.len() / row_bytes) as u32, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
                                Ok(out)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for h in handles {
                done.extend(h.join().unwrap_or_else(|_| vec![Err("codifica interrotta".into())]));
            }
        });
        let io = |e: std::io::Error| e.to_string();
        for jpeg in done {
            let jpeg = jpeg?;
            let (head, data) = split_jpeg(&jpeg)?;
            let sof = find_marker(head, 0xC0).ok_or("JPEG senza SOF0")?;
            let mut plain = head.to_vec();
            plain[sof + 5..sof + 7].copy_from_slice(&[0, 0]);
            match &self.header {
                None => {
                    // L'intestazione vera: l'altezza del livello e l'intervallo di ripartenza (DRI) prima di SOS.
                    let sos = find_marker(head, 0xDA).ok_or("JPEG senza SOS")?;
                    let mut first = head[..sos].to_vec();
                    first[sof + 5..sof + 7].copy_from_slice(&(self.height as u16).to_be_bytes());
                    first.extend_from_slice(&[0xFF, 0xDD, 0, 4]);
                    first.extend_from_slice(&self.restart.to_be_bytes());
                    first.extend_from_slice(&head[sos..]);
                    self.w.write_all(&first).map_err(io)?;
                    self.header = Some(plain);
                }
                Some(h) if *h != plain => return Err("le fette del JPEG hanno tabelle diverse".into()),
                Some(_) => self.w.write_all(&[0xFF, 0xD0 + ((self.written - 1) % 8) as u8]).map_err(io)?,
            }
            self.w.write_all(data).map_err(io)?;
            self.written += 1;
        }
        Ok(())
    }
}

/// Le righe di una striscia del TIFF scritto: poche, così le strisce di una fila si comprimono in parallelo.
const TIFF_STRIP_ROWS: u32 = 16;

/// Un TIFF RGB a strisce, Deflate con il predittore orizzontale; BigTIFF oltre circa 3,5 GB di pixel.
/// Scritto qui (non con l'encoder del crate tiff) per comprimere in parallelo le strisce di ogni fila
/// di tessere: sul livello pieno la compressione è la parte lenta.
fn write_tiff<W: std::io::Write + Seek>(
    s: &Slide,
    path: &Path,
    level: usize,
    mut w: W,
    big: bool,
) -> Result<(), String> {
    let l = &s.levels[level];
    // Tutte le strisce tranne l'ultima hanno le stesse righe: devono dividere esattamente la fila di tessere.
    let strip_rows = if l.tile_h.is_multiple_of(TIFF_STRIP_ROWS) {
        TIFF_STRIP_ROWS
    } else {
        l.tile_h
    };
    let io = |e: std::io::Error| e.to_string();
    // Intestazione: l'indirizzo della pagina si scrive alla fine.
    if big {
        w.write_all(b"II+\0\x08\0\0\0").map_err(io)?;
        w.write_all(&0u64.to_le_bytes()).map_err(io)?;
    } else {
        w.write_all(b"II*\0").map_err(io)?;
        w.write_all(&0u32.to_le_bytes()).map_err(io)?;
    }
    let mut pos: u64 = if big { 16 } else { 8 };
    let mut offsets: Vec<u64> = Vec::new();
    let mut counts: Vec<u64> = Vec::new();
    let row_bytes = l.width as usize * 3;
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    s.read_rows(path, level, |_, rows| {
        let strips: Vec<&[u8]> = rows.chunks(row_bytes * strip_rows as usize).collect();
        let per = strips.len().div_ceil(workers).max(1);
        let packed: Vec<Vec<u8>> = std::thread::scope(|sc| {
            let hs: Vec<_> = strips
                .chunks(per)
                .map(|group| {
                    sc.spawn(move || {
                        group
                            .iter()
                            .map(|st| deflate_strip(st, row_bytes))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            hs.into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        if packed.len() != strips.len() {
            return Err("compressione interrotta".into());
        }
        for data in packed {
            w.write_all(&data).map_err(io)?;
            offsets.push(pos);
            counts.push(data.len() as u64);
            pos += data.len() as u64;
        }
        Ok(())
    })?;
    if !big && pos > u32::MAX as u64 - 1_000_000 {
        return Err("il TIFF supera i 4 GB".into());
    }
    // I valori che non stanno nella voce vanno prima della pagina; si allineano a 2 byte.
    if pos % 2 == 1 {
        w.write_all(&[0]).map_err(io)?;
        pos += 1;
    }
    let mut out_of_line = |w: &mut W, bytes: &[u8]| -> Result<u64, String> {
        let at = pos;
        w.write_all(bytes).map_err(io)?;
        pos += bytes.len() as u64;
        if pos % 2 == 1 {
            w.write_all(&[0]).map_err(io)?;
            pos += 1;
        }
        Ok(at)
    };
    let word = |v: u64| {
        if big {
            v.to_le_bytes().to_vec()
        } else {
            (v as u32).to_le_bytes().to_vec()
        }
    };
    let bits: Vec<u8> = [8u16, 8, 8].iter().flat_map(|v| v.to_le_bytes()).collect();
    let software = b"Cedro\0";
    let offs_bytes: Vec<u8> = offsets.iter().flat_map(|&v| word(v)).collect();
    let count_bytes: Vec<u8> = counts.iter().flat_map(|&v| word(v)).collect();
    let inline = if big { 8 } else { 4 };
    let long_type: u16 = if big { 16 } else { 4 };
    // (tag, tipo, quanti, byte del valore)
    let mut entries: Vec<(u16, u16, u64, Vec<u8>)> = vec![
        (256, 4, 1, l.width.to_le_bytes().to_vec()),
        (257, 4, 1, l.height.to_le_bytes().to_vec()),
        (258, 3, 3, bits),
        (259, 3, 1, 8u16.to_le_bytes().to_vec()),
        (262, 3, 1, 2u16.to_le_bytes().to_vec()),
        (273, long_type, offsets.len() as u64, offs_bytes),
        (277, 3, 1, 3u16.to_le_bytes().to_vec()),
        (278, 4, 1, strip_rows.to_le_bytes().to_vec()),
        (279, long_type, counts.len() as u64, count_bytes),
        (284, 3, 1, 1u16.to_le_bytes().to_vec()),
        (305, 2, software.len() as u64, software.to_vec()),
        (317, 3, 1, 2u16.to_le_bytes().to_vec()),
    ];
    for e in entries.iter_mut() {
        if e.3.len() > inline {
            let at = out_of_line(&mut w, &e.3)?;
            e.3 = word(at);
        } else {
            e.3.resize(inline, 0);
        }
    }
    let ifd = pos;
    if big {
        w.write_all(&(entries.len() as u64).to_le_bytes())
            .map_err(io)?;
    } else {
        w.write_all(&(entries.len() as u16).to_le_bytes())
            .map_err(io)?;
    }
    for (tag, ty, n, val) in &entries {
        w.write_all(&tag.to_le_bytes()).map_err(io)?;
        w.write_all(&ty.to_le_bytes()).map_err(io)?;
        if big {
            w.write_all(&n.to_le_bytes()).map_err(io)?;
        } else {
            w.write_all(&(*n as u32).to_le_bytes()).map_err(io)?;
        }
        w.write_all(val).map_err(io)?;
    }
    w.write_all(&word(0)).map_err(io)?;
    w.seek(SeekFrom::Start(if big { 8 } else { 4 }))
        .map_err(io)?;
    w.write_all(&word(ifd)).map_err(io)?;
    w.flush().map_err(io)
}

/// Una striscia con il predittore orizzontale (ogni campione meno quello del pixel a sinistra) e Deflate.
fn deflate_strip(rows: &[u8], row_bytes: usize) -> Vec<u8> {
    use std::io::Write;
    let mut diff = Vec::with_capacity(rows.len());
    for row in rows.chunks_exact(row_bytes) {
        diff.extend_from_slice(&row[..3.min(row.len())]);
        for i in 3..row.len() {
            diff.push(row[i].wrapping_sub(row[i - 3]));
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(
        Vec::with_capacity(rows.len() / 2),
        flate2::Compression::new(3),
    );
    let _ = z.write_all(&diff);
    z.finish().unwrap_or_default()
}


/// Il vetrino come immagine qualsiasi (OCR, archivi, lettori che vogliono una foto in memoria): il
/// livello «Media», o il più piccolo che arriva a `target_side`.
pub fn open_image(path: &Path, target_side: Option<u32>) -> Result<image::DynamicImage, String> {
    let s = Slide::open(path)?;
    let medium = s.pick(Pick::Medium);
    let level = target_side
        .filter(|v| *v > 0)
        .map(|side| s.for_side(medium, side))
        .unwrap_or(medium);
    s.read_level(path, level)
        .map(image::DynamicImage::ImageRgb8)
}

/// Un numero senza zeri inutili: 20, 0.499.
fn trim_num(v: f32) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// I pixel di un'immagine del crate tiff in RGB a 8 bit.
fn to_rgb(w: u32, h: u32, color: ColorType, px: Vec<u8>) -> Option<RgbImage> {
    let rgb = match color {
        ColorType::RGB(8) => px,
        ColorType::RGBA(8) => px
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        ColorType::Gray(8) => px.iter().flat_map(|&g| [g, g, g]).collect(),
        ColorType::YCbCr(8) => {
            let mut v = px;
            ycbcr_to_rgb(&mut v);
            v
        }
        _ => return None,
    };
    RgbImage::from_raw(w, h, rgb)
}

/// Una tessera letta dal crate tiff (compressioni rare), riportata a RGB e alla misura piena della tessera.
fn chunk_rgb(
    d: &mut Decoder<BufReader<File>>,
    index: u32,
    pixels: usize,
) -> Result<Vec<u8>, String> {
    let color = d.colortype().map_err(err)?;
    let DecodingResult::U8(px) = d.read_chunk(index).map_err(err)? else {
        return Err("tessera a più di 8 bit".into());
    };
    let (cw, ch) = d.chunk_dimensions();
    let mut rgb = to_rgb(cw, ch, color, px)
        .ok_or("colori della tessera non gestiti")?
        .into_raw();
    rgb.resize(pixels * 3, 255);
    Ok(rgb)
}

/// YCbCr (JFIF, valori pieni) in RGB, sul posto.
fn ycbcr_to_rgb(px: &mut [u8]) {
    for p in px.chunks_exact_mut(3) {
        let (y, cb, cr) = (p[0] as f32, p[1] as f32 - 128.0, p[2] as f32 - 128.0);
        p[0] = (y + 1.402 * cr).round().clamp(0.0, 255.0) as u8;
        p[1] = (y - 0.344_136 * cb - 0.714_136 * cr)
            .round()
            .clamp(0.0, 255.0) as u8;
        p[2] = (y + 1.772 * cb).round().clamp(0.0, 255.0) as u8;
    }
}

/// Decodifica una tessera: RGB a 8 bit, `tw` × `th` pixel. Una tessera vuota (0 byte) è bianca.
fn decode_tile(l: &Level, raw: &[u8], tw: usize, th: usize) -> Result<Vec<u8>, String> {
    let want = tw * th * 3;
    if raw.is_empty() {
        return Ok(vec![255; want]);
    }
    let (px, w, h) = match l.compression {
        JPEG => decode_jpeg(l, raw)?,
        JP2K_YCBCR | JP2K_RGB => decode_jp2k(l.compression, raw)?,
        1 => (raw.to_vec(), tw, th),
        other => return Err(format!("compressione {other} non gestita")),
    };
    if (w, h) == (tw, th) && px.len() >= want {
        let mut px = px;
        px.truncate(want);
        return Ok(px);
    }
    // Una tessera con misure diverse (ai bordi alcuni codificatori la tagliano alla parte che c'è):
    // si ricopia riga per riga in una tessera bianca della misura piena, così le righe restano a posto.
    let mut out = vec![255u8; want];
    let (cw, ch) = (w.min(tw), h.min(th));
    for y in 0..ch {
        let src = y * w * 3;
        let Some(riga) = px.get(src..src + cw * 3) else { break };
        out[y * tw * 3..y * tw * 3 + cw * 3].copy_from_slice(riga);
    }
    Ok(out)
}

/// I pixel RGB della tessera JPEG, con larghezza e altezza.
fn decode_jpeg(l: &Level, raw: &[u8]) -> Result<(Vec<u8>, usize, usize), String> {
    use zune_core::colorspace::ColorSpace;
    // Le tabelle in comune vanno davanti alla tessera: tabelle senza la fine (EOI), tessera senza l'inizio (SOI).
    let data: std::borrow::Cow<[u8]> = match &l.tables {
        Some(t) if raw.len() >= 2 => {
            let mut v = Vec::with_capacity(t.len() + raw.len());
            v.extend_from_slice(&t[..t.len() - 2]);
            v.extend_from_slice(&raw[2..]);
            v.into()
        }
        _ => raw.into(),
    };
    let mut dec = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&data[..]));
    dec.decode_headers().map_err(|e| format!("JPEG: {e:?}"))?;
    let (w, h) = dec.dimensions().ok_or("JPEG senza misure")?;
    let input = dec.input_colorspace().unwrap_or(ColorSpace::YCbCr);
    // Con la fotometria RGB i valori sono già RGB, anche se il JPEG dice YCbCr: non si convertono.
    let out = if l.photometric == PHOTOMETRIC_YCBCR {
        ColorSpace::RGB
    } else {
        input
    };
    dec.set_options(zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(out));
    let px = dec.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let px = match out {
        ColorSpace::Luma => px.iter().flat_map(|&g| [g, g, g]).collect(),
        ColorSpace::RGBA => px
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        _ => px,
    };
    Ok((px, w, h))
}

/// Le misure di campionamento delle componenti (XRsiz) dal segmento SIZ del codestream.
fn subsampled_chroma(raw: &[u8]) -> bool {
    // SOC (2) + SIZ: marcatore (2), Lsiz (2), Rsiz (2), 8 campi da 4 byte, Csiz (2), poi 3 byte per componente.
    let base = 2 + 2 + 2 + 2 + 32 + 2;
    let comp = |i: usize| raw.get(base + i * 3 + 1..base + i * 3 + 3);
    match (comp(0), comp(1), comp(2)) {
        (Some(y), Some(cb), Some(cr)) => cb == cr && cb != y,
        _ => false,
    }
}

fn decode_jp2k(compression: u16, raw: &[u8]) -> Result<(Vec<u8>, usize, usize), String> {
    use hayro_jpeg2000::{DecodeSettings, DecoderContext, Image};
    let image =
        Image::new(raw, &DecodeSettings::default()).map_err(|e| format!("JPEG 2000: {e:?}"))?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut ctx = DecoderContext::default();
    let decoded = image
        .decode(&mut ctx)
        .map_err(|e| format!("JPEG 2000: {e:?}"))?;
    let comps = decoded.components().len();
    let mut px = decoded.data_u8();
    match comps {
        1 => px = px.iter().flat_map(|&g| [g, g, g]).collect(),
        3 => {
            // Con la crominanza ridotta hayro la riconosce come YCbCr e la porta già in RGB.
            if compression == JP2K_YCBCR && !subsampled_chroma(raw) {
                ycbcr_to_rgb(&mut px);
            }
        }
        4 => {
            px = px
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect()
        }
        n => return Err(format!("JPEG 2000 con {n} componenti")),
    }
    Ok((px, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/svs")
            .join(name)
    }

    #[test]
    fn reads_the_description_fields() {
        let d = "Aperio Image Library v11.2.1 \r\n46000x32914 [0,0 46000x32914] (240x240) JPEG/RGB Q=30|AppMag = 20|MPP = 0.4990|Filename = CMU-1";
        assert_eq!(field(d, "AppMag"), Some(20.0));
        assert_eq!(field(d, "MPP"), Some(0.499));
        assert_eq!(field(d, "Nope"), None);
        assert_eq!(trim_num(0.499), "0.499");
        assert_eq!(trim_num(20.0), "20");
    }

    #[test]
    fn a_plain_tiff_is_not_a_slide() {
        let dir = std::env::temp_dir().join(format!("cedro-svs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("foto.tif");
        RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
            .save(&p)
            .unwrap();
        assert!(!is_slide(&p));
        assert!(Slide::open(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Il colore medio di un quadrato 240×240 dell'immagine.
    fn mean(img: &RgbImage, x0: u32, y0: u32) -> [f64; 3] {
        let mut sum = [0f64; 3];
        for y in y0..y0 + 240 {
            for x in x0..x0 + 240 {
                let p = img.get_pixel(x, y).0;
                for c in 0..3 {
                    sum[c] += p[c] as f64;
                }
            }
        }
        sum.map(|v| v / (240.0 * 240.0))
    }

    fn close(a: [f64; 3], b: [f64; 3], tol: f64) {
        assert!(
            a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol),
            "{a:?} invece di {b:?}"
        );
    }

    /// I colori medi delle quattro tessere del pezzo di CMU-1, letti con libjpeg (tifffile).
    const CMU1_MEANS: [[f64; 3]; 4] = [
        [133.99, 92.69, 132.4],
        [146.51, 121.29, 150.03],
        [115.17, 68.89, 113.01],
        [138.34, 109.4, 139.59],
    ];

    #[test]
    fn jpeg_tiles_with_shared_tables_keep_their_colors() {
        // Un pezzo vero del vetrino CMU-1 di OpenSlide: tessere JPEG 240×240 con le tabelle in comune,
        // fotometria RGB (i valori non vanno convertiti da YCbCr anche se il JPEG lo dice).
        let p = fixture("cmu1-pezzo.svs");
        assert!(is_slide(&p));
        let s = Slide::open(&p).unwrap();
        assert_eq!(s.tiles_name(), "JPEG");
        assert_eq!(
            s.summary(false),
            "Aperio · 1 livello · tessere JPEG · 20× (0,499 µm/px)"
        );
        assert_eq!(
            s.summary(true),
            "Aperio · 1 level · JPEG tiles · 20× (0.499 µm/px)"
        );
        assert!(s.label.is_some() && s.macro_image.is_some() && s.thumbnail.is_some());
        let img = s.read_level(&p, 0).unwrap();
        assert_eq!(img.dimensions(), (480, 480));
        for (k, (x, y)) in [(0, 0), (240, 0), (0, 240), (240, 240)]
            .into_iter()
            .enumerate()
        {
            close(mean(&img, x, y), CMU1_MEANS[k], 0.6);
        }
        assert_eq!(img.get_pixel(10, 10).0, [132, 67, 122]);
        // Una zona a cavallo di quattro tessere è uguale allo stesso pezzo del livello intero.
        let zone = s.read_region(&p, 0, 200, 190, 100, 120).unwrap();
        assert_eq!(zone.get_pixel(0, 0), img.get_pixel(200, 190));
        assert_eq!(zone.get_pixel(99, 119), img.get_pixel(299, 309));
        assert!(s.read_region(&p, 0, 400, 400, 100, 10).is_err());
        let label = s.read_extra(&p, s.label.unwrap()).unwrap();
        assert_eq!(label.dimensions(), (97, 116));
        assert!(s.preview(&p, 100).unwrap().width() <= 100);
    }

    #[test]
    fn jpeg2000_ycbcr_tiles_become_rgb() {
        // Le stesse quattro tessere codificate come le Aperio 33003: YCbCr senza trasformazione nel codestream.
        let p = fixture("j2k-ycbcr.svs");
        let s = Slide::open(&p).unwrap();
        assert_eq!(s.tiles_name(), "JPEG 2000");
        let img = s.read_level(&p, 0).unwrap();
        for (k, (x, y)) in [(0, 0), (240, 0), (0, 240), (240, 240)]
            .into_iter()
            .enumerate()
        {
            close(mean(&img, x, y), CMU1_MEANS[k], 2.0);
        }
    }

    #[test]
    fn levels_are_picked_by_size() {
        // BigTIFF con tre livelli JPEG in fotometria YCbCr (come li scrive libtiff).
        let p = fixture("piramide.svs");
        let s = Slide::open(&p).unwrap();
        assert_eq!(s.levels.len(), 3);
        assert_eq!((s.full().width, s.full().height), (720, 480));
        assert!(s.levels.windows(2).all(|w| w[0].pixels() > w[1].pixels()));
        assert_eq!(s.pick(Pick::Full), 0);
        // Livelli piccoli: Media e Panoramica prendono il pieno (sta sotto le soglie).
        assert_eq!(s.pick(Pick::Medium), 0);
        assert_eq!(s.pick(Pick::Overview), 0);
        assert_eq!(s.for_side(0, 300), 1);
        assert_eq!(s.for_side(0, 10_000), 0);
        let mut rows = 0;
        let mut sum = [0f64; 3];
        let full = s.full().clone();
        s.read_rows(&p, 0, |y, band| {
            assert_eq!(y, rows);
            rows += (band.len() / (full.width as usize * 3)) as u32;
            for px in band.chunks_exact(3) {
                for c in 0..3 {
                    sum[c] += px[c] as f64;
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, full.height);
        close(
            sum.map(|v| v / full.pixels() as f64),
            [126.84, 86.78, 126.88],
            1.5,
        );
    }

    #[test]
    fn big_levels_are_written_in_strips() {
        let p = fixture("piramide.svs");
        let s = Slide::open(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("cedro-svs-strisce-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let whole = s.read_level(&p, 0).unwrap();
        for (how, name) in [
            (Streamed::Png, "a.png"),
            (Streamed::Tiff, "a.tiff"),
            (Streamed::Jpeg(92), "a.jpg"),
        ] {
            let out = dir.join(name);
            s.write_streamed(&p, 0, &out, how).unwrap();
            let back = image::open(&out)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .to_rgb8();
            assert_eq!(back.dimensions(), (720, 480), "{name}");
            let diff: f64 = back
                .as_raw()
                .iter()
                .zip(whole.as_raw())
                .map(|(a, b)| a.abs_diff(*b) as f64)
                .sum::<f64>()
                / whole.as_raw().len() as f64;
            // PNG e TIFF sono identici; il JPEG perde poco.
            assert!(
                if how == Streamed::Jpeg(92) {
                    diff < 3.0
                } else {
                    diff == 0.0
                },
                "{name}: {diff}"
            );
        }
        // Il JPEG è fatto di fette codificate in parallelo, unite dai marcatori di ripartenza; il
        // codificatore in un colpo solo dà gli stessi pixel.
        let jpeg = std::fs::read(dir.join("a.jpg")).unwrap();
        assert!(jpeg.windows(4).any(|w| w == [0xFF, 0xDD, 0, 4]), "manca DRI");
        // 480 righe a fette di 64 (8 file di blocchi): 8 fette, 7 ripartenze.
        assert_eq!(jpeg.windows(2).filter(|w| w[0] == 0xFF && (0xD0..=0xD7).contains(&w[1])).count(), 7);
        let mut single = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut single, 92).encode_image(&whole).unwrap();
        assert_eq!(image::load_from_memory(&single).unwrap().to_rgb8(), image::open(dir.join("a.jpg")).unwrap().to_rgb8());
        // Lo stesso TIFF in BigTIFF (sui vetrini oltre i 4 GB): il crate tiff lo rilegge uguale.
        let out = dir.join("big.tif");
        write_tiff(
            &s,
            &p,
            0,
            std::io::BufWriter::new(File::create(&out).unwrap()),
            true,
        )
        .unwrap();
        assert_eq!(&std::fs::read(&out).unwrap()[..4], b"II+\0");
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!(back.as_raw(), whole.as_raw());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod prove_revisione {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/svs").join(name)
    }

    #[test]
    fn una_tessera_che_dice_di_essere_enorme_e_un_errore_non_un_crash() {
        let p = fixture("cmu1-pezzo.svs");
        let mut s = Slide::open(&p).unwrap();
        s.levels[0].counts[1] = 1 << 40;
        let e = s.read_level(&p, 0).unwrap_err();
        assert!(e.contains("fuori dal file"), "{e}");
    }

    #[test]
    fn una_tessera_di_bordo_piu_piccola_resta_al_suo_posto() {
        // Una tessera 240×240 codificata con le sole misure che ci sono (100×50): le righe non scivolano.
        let img = RgbImage::from_pixel(100, 50, image::Rgb([200, 30, 30]));
        let mut raw = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut raw, 95).encode_image(&img).unwrap();
        let l = Level { ifd: 0, width: 100, height: 50, tile_w: 240, tile_h: 240, compression: JPEG, photometric: PHOTOMETRIC_YCBCR, offsets: vec![0], counts: vec![0], tables: None };
        let px = decode_tile(&l, &raw, 240, 240).unwrap();
        assert_eq!(px.len(), 240 * 240 * 3);
        let at = |x: usize, y: usize| &px[(y * 240 + x) * 3..(y * 240 + x) * 3 + 3];
        assert!(at(0, 1)[0] > 180 && at(0, 1)[1] < 60, "{:?}", at(0, 1));
        assert!(at(99, 49)[0] > 180, "{:?}", at(99, 49));
        assert_eq!(at(100, 0), [255, 255, 255]);
        assert_eq!(at(0, 50), [255, 255, 255]);
    }
}
