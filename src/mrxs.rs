//! I vetrini 3DHistech MIRAX (.mrxs). Il file `.mrxs` è quasi vuoto: il vetrino sta nella cartella
//! con lo stesso nome, accanto. Dentro: `Slidedat.ini` (descrizione dei livelli), `Index.dat`
//! (dove sta ogni foto) e i file di dati `Data0000.dat`… con le foto JPEG, PNG o BMP.
//!
//! La fotocamera scatta una foto e ne registra la posizione; ogni foto è divisa in
//! `CameraImageDivisionsPerSide`² immagini, e le foto vicine si sovrappongono un poco. Ogni livello
//! della piramide unisce 2 × 2 immagini del livello sotto. Per avere il vetrino si incollano le
//! immagini (o i loro pezzi, quando un'immagine viene da foto diverse) nella loro posizione, su un
//! fondo del colore che dice lo scanner. È lo stesso schema che segue OpenSlide, rifatto in Rust.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::RgbImage;

use crate::svs::{Blob, Level, Slide, Source, Vendor, JPEG};

/// Le compressioni delle immagini MIRAX che non hanno un numero TIFF: valori nostri, solo per i nomi.
pub const PNG: u16 = 0xFFF1;
pub const BMP: u16 = 0xFFF2;

/// La versione dell'indice che conosciamo.
const VERSIONE_INDICE: &[u8] = b"01.02";

/// Il file `Slidedat.ini`: gruppi di chiavi.
struct Ini(HashMap<String, HashMap<String, String>>);

impl Ini {
    fn leggi(path: &Path) -> Result<Ini, String> {
        let b = fs::read(path).map_err(|e| format!("Slidedat.ini non letto: {e}"))?;
        if b.len() > 4 << 20 {
            return Err("Slidedat.ini troppo grande".into());
        }
        let testo = String::from_utf8_lossy(b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&b)).into_owned();
        let mut gruppi: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut attuale = String::new();
        for riga in testo.lines() {
            let r = riga.trim();
            if r.is_empty() || r.starts_with(';') || r.starts_with('#') {
                continue;
            }
            if let Some(g) = r.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                attuale = g.trim().to_string();
                gruppi.entry(attuale.clone()).or_default();
            } else if let Some((k, v)) = r.split_once('=') {
                gruppi.entry(attuale.clone()).or_default().insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Ok(Ini(gruppi))
    }

    fn get(&self, g: &str, k: &str) -> Option<&str> {
        self.0.get(g)?.get(k).map(|s| s.as_str())
    }

    fn serve(&self, g: &str, k: &str) -> Result<&str, String> {
        self.get(g, k).ok_or_else(|| format!("Slidedat.ini: manca {k} in [{g}]"))
    }

    fn intero(&self, g: &str, k: &str) -> Result<i64, String> {
        self.serve(g, k)?.parse().map_err(|_| format!("Slidedat.ini: {k} non è un numero"))
    }

    fn reale(&self, g: &str, k: &str) -> Result<f64, String> {
        self.serve(g, k)?.replace(',', ".").parse().map_err(|_| format!("Slidedat.ini: {k} non è un numero"))
    }
}

/// La cartella del vetrino: il percorso del `.mrxs` senza estensione.
fn cartella(path: &Path) -> PathBuf {
    path.with_extension("")
}

/// Vero se il file è un `.mrxs` con accanto la sua cartella e il `Slidedat.ini`.
pub fn is_mirax(path: &Path) -> bool {
    crate::engine::extension(path) == "mrxs" && cartella(path).join("Slidedat.ini").is_file()
}

/// Un'immagine nei file di dati.
#[derive(Clone, Copy, Debug)]
struct Immagine {
    file: u32,
    offset: u64,
    len: u64,
}

/// Un pezzo di un'immagine incollato sulla tela del livello.
#[derive(Clone, Copy, Debug)]
struct Pezzo {
    /// Dove va, in pixel del livello (può uscire un poco dai bordi).
    x: i64,
    y: i64,
    /// Da dove si prende nell'immagine e quanto.
    sx: u32,
    sy: u32,
    w: u32,
    h: u32,
    img: u32,
    /// L'ordine in cui si dipinge (dove due pezzi si sovrappongono vince l'ultimo).
    ordine: u64,
}

/// Un livello MIRAX: le immagini e dove vanno.
#[derive(Debug)]
pub struct Griglia {
    files: Vec<PathBuf>,
    immagini: Vec<Immagine>,
    /// Ordinati per `y`.
    pezzi: Vec<Pezzo>,
    /// L'altezza più grande di un pezzo, per cercare quelli che toccano una fascia.
    alto_max: u32,
    formato: u16,
}

/// I numeri di un livello dal suo gruppo in `Slidedat.ini`.
struct Sezione {
    concat: i64,
    overlap_x: f64,
    overlap_y: f64,
    mpp_x: f64,
    fill: [u8; 3],
    formato: u16,
    img_w: i64,
    img_h: i64,
}

/// I parametri di un livello che OpenSlide chiama «slide_zoom_level_params».
#[derive(Clone, Copy)]
struct Param {
    image_concat: i64,
    tile_count_divisor: i64,
    tiles_per_image: i64,
    positions_per_tile: i64,
    advance_x: f64,
    advance_y: f64,
    tile_w: f64,
    tile_h: f64,
}

fn le_i32(f: &mut File) -> Result<i32, String> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).map_err(|_| "Index.dat tagliato".to_string())?;
    Ok(i32::from_le_bytes(b))
}

fn vai(f: &mut File, pos: i64) -> Result<(), String> {
    if pos < 0 {
        return Err("Index.dat: indirizzo negativo".into());
    }
    f.seek(SeekFrom::Start(pos as u64)).map(|_| ()).map_err(|e| e.to_string())
}

/// Un record «non gerarchico» (posizioni, etichetta, foto del vetrino): file, inizio e lunghezza.
fn record(f: &mut File, radice: i64, n: usize, files: usize) -> Result<Immagine, String> {
    vai(f, radice)?;
    let p = le_i32(f)?;
    vai(f, p as i64 + 4 * n as i64)?;
    let p = le_i32(f)?;
    vai(f, p as i64)?;
    if le_i32(f)? != 0 {
        return Err("Index.dat: pagina dati senza lo 0 iniziale".into());
    }
    let p = le_i32(f)?;
    vai(f, p as i64)?;
    if le_i32(f)? < 1 {
        return Err("Index.dat: record vuoto".into());
    }
    le_i32(f)?;
    le_i32(f)?;
    le_i32(f)?;
    let (pos, len, file) = (le_i32(f)?, le_i32(f)?, le_i32(f)?);
    if pos < 0 || len < 0 || file < 0 || file as usize >= files {
        return Err("Index.dat: record fuori posto".into());
    }
    Ok(Immagine { file: file as u32, offset: pos as u64, len: len as u64 })
}

/// Dove sta, tra i record non gerarchici, il gruppo `nome` (e quanti valori ha, e quale indice ha).
fn offset_nome(ini: &Ini, n: i64, nome: &str) -> Option<(usize, usize, i64)> {
    let mut off = 0usize;
    for i in 0..n {
        let v = ini.get("HIERARCHICAL", &format!("NONHIER_{i}_NAME"))?;
        let c: usize = ini.get("HIERARCHICAL", &format!("NONHIER_{i}_COUNT"))?.parse().ok()?;
        if v == nome {
            return Some((off, c, i));
        }
        off += c;
    }
    None
}

/// Dove sta il record del valore `valore` del gruppo `nome`, e la sua sezione.
fn offset_valore(ini: &Ini, n: i64, nome: &str, valore: &str) -> Option<(usize, String)> {
    let (off, c, i) = offset_nome(ini, n, nome)?;
    for k in 0..c {
        if ini.get("HIERARCHICAL", &format!("NONHIER_{i}_VAL_{k}"))? == valore {
            let sez = ini.get("HIERARCHICAL", &format!("NONHIER_{i}_VAL_{k}_SECTION")).unwrap_or("").to_string();
            return Some((off + k, sez));
        }
    }
    None
}

fn formato(nome: &str) -> Result<u16, String> {
    match nome {
        "JPEG" => Ok(JPEG),
        "PNG" => Ok(PNG),
        "BMP24" => Ok(BMP),
        altro => Err(format!("MIRAX: immagini {altro} non gestite")),
    }
}

/// Apre un MIRAX: livelli, posizioni delle immagini, etichetta, foto del vetrino e miniatura.
pub fn open(path: &Path) -> Result<Slide, String> {
    let dir = cartella(path);
    let ini = Ini::leggi(&dir.join("Slidedat.ini"))?;
    let g = "GENERAL";
    let slide_id = ini.serve(g, "SLIDE_ID")?.to_string();
    let images_x = ini.intero(g, "IMAGENUMBER_X")?;
    let images_y = ini.intero(g, "IMAGENUMBER_Y")?;
    let obiettivo = ini.get(g, "OBJECTIVE_MAGNIFICATION").and_then(|s| s.trim_end_matches('x').trim().parse::<f32>().ok()).filter(|m| *m > 0.0);
    let divisions = ini.get(g, "CameraImageDivisionsPerSide").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
    if images_x <= 0 || images_y <= 0 || divisions <= 0 || images_x > 1 << 20 || images_y > 1 << 20 {
        return Err("Slidedat.ini: numero di immagini non valido".into());
    }
    let h = "HIERARCHICAL";
    let hier_count = ini.intero(h, "HIER_COUNT")?;
    let nonhier_count = ini.intero(h, "NONHIER_COUNT")?;
    let zoom = (0..hier_count).find(|i| ini.get(h, &format!("HIER_{i}_NAME")) == Some("Slide zoom level")).ok_or("MIRAX senza livelli di ingrandimento")?;
    if zoom != 0 {
        return Err("MIRAX con i livelli non al primo posto: non gestito".into());
    }
    let index_name = ini.serve(h, "INDEXFILE")?.to_string();
    let livelli = ini.intero(h, &format!("HIER_{zoom}_COUNT"))?;
    if !(1..=40).contains(&livelli) {
        return Err("MIRAX: numero di livelli non valido".into());
    }
    let nfile = ini.intero("DATAFILE", "FILE_COUNT")?;
    if !(1..=100_000).contains(&nfile) {
        return Err("MIRAX: numero di file di dati non valido".into());
    }
    let files: Vec<PathBuf> = (0..nfile).map(|i| ini.serve("DATAFILE", &format!("FILE_{i}")).map(|n| dir.join(n))).collect::<Result<_, _>>()?;
    let mut sezioni = Vec::new();
    for i in 0..livelli {
        let s = ini.serve(h, &format!("HIER_{zoom}_VAL_{i}_SECTION"))?.to_string();
        let bgr = ini.intero(&s, "IMAGE_FILL_COLOR_BGR")? as u32;
        let sez = Sezione {
            concat: ini.intero(&s, "IMAGE_CONCAT_FACTOR")?,
            overlap_x: ini.reale(&s, "OVERLAP_X")?,
            overlap_y: ini.reale(&s, "OVERLAP_Y")?,
            mpp_x: ini.reale(&s, "MICROMETER_PER_PIXEL_X")?,
            fill: [(bgr & 0xFF) as u8, ((bgr >> 8) & 0xFF) as u8, ((bgr >> 16) & 0xFF) as u8],
            formato: formato(ini.serve(&s, "IMAGE_FORMAT")?)?,
            img_w: ini.intero(&s, "DIGITIZER_WIDTH")?,
            img_h: ini.intero(&s, "DIGITIZER_HEIGHT")?,
        };
        if (i == 0 && sez.concat < 0) || (i > 0 && sez.concat <= 0) || sez.img_w <= 0 || sez.img_h <= 0 || sez.img_w > 1 << 16 || sez.img_h > 1 << 16 {
            return Err("Slidedat.ini: livello non valido".into());
        }
        sezioni.push(sez);
    }
    let pos_vimslide = offset_nome(&ini, nonhier_count, "VIMSLIDE_POSITION_BUFFER").map(|o| o.0);
    let pos_stitching = if pos_vimslide.is_none() { offset_nome(&ini, nonhier_count, "StitchingIntensityLayer").map(|o| o.0) } else { None };
    let associata = |valore: &str, chiave: &str| -> Option<usize> {
        let (off, sez) = offset_valore(&ini, nonhier_count, "Scan data layer", valore)?;
        (ini.get(&sez, chiave) == Some("JPEG")).then_some(off)
    };
    let rec_macro = associata("ScanDataLayer_SlideThumbnail", "THUMBNAIL_IMAGE_TYPE");
    let rec_label = associata("ScanDataLayer_SlideBarcode", "BARCODE_IMAGE_TYPE");
    let rec_thumb = associata("ScanDataLayer_SlidePreview", "PREVIEW_IMAGE_TYPE");

    // Le misure del livello pieno: immagini intere, meno la sovrapposizione tra una foto e l'altra
    // (come OpenSlide: la sovrapposizione ha i decimali e ogni passo arrotonda per difetto).
    let s0 = &sezioni[0];
    let somma = |n: i64, lato: i64, overlap: f64| -> i64 {
        (0..n).map(|i| if i % divisions != divisions - 1 || i == n - 1 { lato } else { (lato as f64 - overlap).floor() as i64 }).sum()
    };
    let base_w = somma(images_x, s0.img_w, s0.overlap_x);
    let base_h = somma(images_y, s0.img_h, s0.overlap_y);
    let posizioni_note = pos_vimslide.is_some() || pos_stitching.is_some() || s0.overlap_x != 0.0 || s0.overlap_y != 0.0;
    let mut param = Vec::new();
    let mut esponente = 0i64;
    for s in &sezioni {
        esponente += s.concat;
        if esponente > 30 {
            return Err("MIRAX: troppi livelli".into());
        }
        let image_concat = 1i64 << esponente;
        let positions_per_image = (image_concat / divisions).max(1);
        let (tile_count_divisor, tiles_per_image, positions_per_tile) = if posizioni_note {
            (image_concat.min(divisions), positions_per_image, 1)
        } else {
            (image_concat, 1, positions_per_image)
        };
        let tile_w = s.img_w as f64 / tiles_per_image as f64;
        let tile_h = s.img_h as f64 / tiles_per_image as f64;
        let images_per_position = (divisions / image_concat).max(1) as f64;
        param.push(Param {
            image_concat,
            tile_count_divisor,
            tiles_per_image,
            positions_per_tile,
            advance_x: tile_w - s.overlap_x / images_per_position,
            advance_y: tile_h - s.overlap_y / images_per_position,
            tile_w,
            tile_h,
        });
    }

    let mut idx = File::open(dir.join(&index_name)).map_err(|e| format!("{index_name} non aperto: {e}"))?;
    let mut ver = vec![0u8; VERSIONE_INDICE.len() + slide_id.len()];
    idx.read_exact(&mut ver).map_err(|_| "Index.dat tagliato".to_string())?;
    if &ver[..VERSIONE_INDICE.len()] != VERSIONE_INDICE {
        return Err("Index.dat di una versione che non conosciamo".into());
    }
    if &ver[VERSIONE_INDICE.len()..] != slide_id.as_bytes() {
        return Err("Index.dat non è di questo vetrino (identificativo diverso)".into());
    }
    let hier_root = ver.len() as i64;
    let nonhier_root = hier_root + 4;

    // Le posizioni delle foto (livello pieno): dal record dello scanner o, se manca, dalla griglia teorica.
    let per_lato = images_x / divisions;
    let npos = (per_lato * (images_y / divisions)) as usize;
    let rec_pos = pos_vimslide.or(pos_stitching);
    let posizioni: Vec<(i64, i64)> = if let Some(r) = rec_pos {
        let im = record(&mut idx, nonhier_root, r, files.len())?;
        let grezzo = leggi_pezzo(&files[im.file as usize], im.offset, im.len)?;
        let dati = if pos_stitching.is_some() {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&grezzo[..]).take(npos as u64 * 9 + 1).read_to_end(&mut out).map_err(|e| format!("posizioni MIRAX non lette: {e}"))?;
            out
        } else {
            grezzo
        };
        if dati.len() != npos * 9 {
            return Err("MIRAX: posizioni delle foto di grandezza inattesa".into());
        }
        let c0 = param[0].image_concat;
        dati.chunks_exact(9)
            .map(|r| {
                if r[0] & 0xFE != 0 {
                    return Err("MIRAX: posizione con un segno inatteso".to_string());
                }
                let x = i32::from_le_bytes(r[1..5].try_into().unwrap()) as i64;
                let y = i32::from_le_bytes(r[5..9].try_into().unwrap()) as i64;
                Ok((x * c0, y * c0))
            })
            .collect::<Result<_, _>>()?
    } else {
        (0..npos as i64)
            .map(|i| ((i % per_lato) * (s0.img_w * divisions - s0.overlap_x as i64), (i / per_lato) * (s0.img_h * divisions - s0.overlap_y as i64)))
            .collect()
    };

    let mut blobs = Vec::new();
    let mut associata_blob = |r: Option<usize>| -> Option<usize> {
        let im = record(&mut idx, nonhier_root, r?, files.len()).ok()?;
        blobs.push(Blob { path: files[im.file as usize].clone(), offset: im.offset, len: im.len });
        Some(blobs.len() - 1)
    };
    let macro_image = associata_blob(rec_macro);
    let label = associata_blob(rec_label);
    let thumbnail = associata_blob(rec_thumb);

    // Le pagine dei livelli: per ogni immagine (indice, inizio, lunghezza, file) i pezzi da incollare.
    vai(&mut idx, hier_root)?;
    let mut seek = le_i32(&mut idx)? as i64;
    let mut attive: HashSet<i64> = HashSet::new();
    let mut levels = Vec::new();
    for (z, (lp, sez)) in param.iter().zip(&sezioni).enumerate() {
        vai(&mut idx, seek)?;
        let p = le_i32(&mut idx)?;
        vai(&mut idx, p as i64)?;
        if le_i32(&mut idx)? != 0 {
            return Err("Index.dat: pagina dati senza lo 0 iniziale".into());
        }
        let p = le_i32(&mut idx)?;
        vai(&mut idx, p as i64)?;
        let mut immagini = Vec::new();
        let mut pezzi = Vec::new();
        let mut giri = 0;
        loop {
            giri += 1;
            if giri > 10_000_000 {
                return Err("Index.dat: troppe pagine".into());
            }
            let n = le_i32(&mut idx)?;
            let prossima = le_i32(&mut idx)?;
            if !(0..=1 << 24).contains(&n) {
                return Err("Index.dat: pagina con un numero strano di immagini".into());
            }
            let mut b = vec![0u8; n as usize * 16];
            idx.read_exact(&mut b).map_err(|_| "Index.dat tagliato".to_string())?;
            for r in b.chunks_exact(16) {
                let v = |k: usize| i32::from_le_bytes(r[k * 4..k * 4 + 4].try_into().unwrap());
                let (indice, off, len, file) = (v(0), v(1), v(2), v(3));
                if indice < 0 || off < 0 || len < 0 || file < 0 || file as usize >= files.len() {
                    return Err("Index.dat: immagine fuori posto".into());
                }
                let (x, y) = ((indice as i64) % images_x, (indice as i64) / images_x);
                if y >= images_y || x % lp.image_concat != 0 || y % lp.image_concat != 0 {
                    return Err(format!("Index.dat: immagine {indice} fuori dalla griglia del livello {z}"));
                }
                let img = immagini.len() as u32;
                immagini.push(Immagine { file: file as u32, offset: off as u64, len: len as u64 });
                for yi in 0..lp.tiles_per_image {
                    let yy = y + yi * divisions;
                    if yy >= images_y {
                        break;
                    }
                    for xi in 0..lp.tiles_per_image {
                        let xx = x + xi * divisions;
                        if xx >= images_x {
                            break;
                        }
                        // La posizione nel livello pieno, solo se una delle foto del pezzo c'è davvero.
                        let (xp, yp) = (xx / divisions, yy / divisions);
                        let cp = yp * per_lato + xp;
                        let Some(&(px, py)) = posizioni.get(cp as usize) else { continue };
                        let pos0 = (px + s0.img_w * (xx - xp * divisions), py + s0.img_h * (yy - yp * divisions));
                        if z == 0 {
                            if px == 0 && py == 0 && (xp != 0 || yp != 0) {
                                continue;
                            }
                            attive.insert(cp);
                        } else {
                            let mut c = false;
                            'fuori: for ypp in yp..yp + lp.positions_per_tile {
                                for xpp in xp..xp + lp.positions_per_tile {
                                    if attive.contains(&(ypp * per_lato + xpp)) {
                                        c = true;
                                        break 'fuori;
                                    }
                                }
                            }
                            if !c {
                                continue;
                            }
                        }
                        let (pos_x, pos_y) = (pos0.0 as f64 / lp.image_concat as f64, pos0.1 as f64 / lp.image_concat as f64);
                        let (tx, ty) = (x / lp.tile_count_divisor + xi, y / lp.tile_count_divisor + yi);
                        // Come OpenSlide: la tessera nella griglia (avanzamento) più lo scostamento.
                        let off_x = pos_x - tx as f64 * lp.advance_x;
                        let off_y = pos_y - ty as f64 * lp.advance_y;
                        let gx = (tx as f64 * lp.advance_x + off_x).round() as i64;
                        let gy = (ty as f64 * lp.advance_y + off_y).round() as i64;
                        pezzi.push(Pezzo {
                            x: gx,
                            y: gy,
                            sx: (lp.tile_w * xi as f64).round() as u32,
                            sy: (lp.tile_h * yi as f64).round() as u32,
                            w: lp.tile_w.ceil() as u32,
                            h: lp.tile_h.ceil() as u32,
                            img,
                            ordine: ((ty as u64) << 40) | ((tx as u64) << 20) | (pezzi.len() as u64 & 0xFFFFF),
                        });
                    }
                }
            }
            if prossima == 0 {
                break;
            }
            vai(&mut idx, prossima as i64)?;
        }
        pezzi.sort_by_key(|p| (p.y, p.ordine));
        let alto_max = pezzi.iter().map(|p| p.h).max().unwrap_or(1);
        let (w, hh) = ((base_w / lp.image_concat) as u32, (base_h / lp.image_concat) as u32);
        let griglia = Griglia { files: files.clone(), immagini, pezzi, alto_max, formato: sez.formato };
        levels.push(Level {
            ifd: z,
            width: w,
            height: hh,
            tile_w: sez.img_w as u32,
            tile_h: FASCIA,
            compression: sez.formato,
            photometric: 2,
            offsets: Vec::new(),
            counts: Vec::new(),
            tables: None,
            source: Source::Mirax(Arc::new(griglia)),
        });
        seek += 4;
    }
    if levels.iter().any(|l| l.width == 0 || l.height == 0) {
        levels.retain(|l| l.width > 0 && l.height > 0);
    }
    if levels.is_empty() {
        return Err("MIRAX senza livelli".into());
    }
    Ok(Slide {
        levels,
        thumbnail,
        label,
        macro_image,
        mpp: Some(s0.mpp_x as f32).filter(|m| *m > 0.0),
        magnification: obiettivo,
        vendor: Vendor::Mirax,
        blobs,
        fill: s0.fill,
    })
}

/// Le righe di tela che si preparano alla volta.
const FASCIA: u32 = 512;

fn leggi_pezzo(path: &Path, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    if len > 256 << 20 {
        return Err("immagine MIRAX troppo grande".into());
    }
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let n = f.metadata().map_err(|e| e.to_string())?.len();
    if offset.checked_add(len).is_none_or(|e| e > n) {
        return Err(format!("{}: immagine oltre la fine del file (vetrino incompleto?)", path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default()));
    }
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut b = vec![0u8; len as usize];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

impl Griglia {
    fn decodifica(&self, i: u32) -> Result<RgbImage, String> {
        let im = self.immagini[i as usize];
        let b = leggi_pezzo(&self.files[im.file as usize], im.offset, im.len)?;
        let img = if self.formato == JPEG {
            let mut d = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&b[..]));
            d.set_options(zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGB));
            let px = d.decode().map_err(|e| format!("JPEG MIRAX: {e:?}"))?;
            let (w, h) = d.dimensions().ok_or("JPEG MIRAX senza misure")?;
            RgbImage::from_raw(w as u32, h as u32, px).ok_or("JPEG MIRAX con misure sbagliate")?
        } else {
            image::load_from_memory(&b).map_err(|e| format!("immagine MIRAX: {e}"))?.to_rgb8()
        };
        Ok(img)
    }
}

/// La finestra `win` di un livello MIRAX, a fasce, passata a `sink` come per gli altri vetrini.
pub(crate) fn read_window(g: &Griglia, fill: [u8; 3], win: (u32, u32, u32, u32), mut sink: impl FnMut(u32, &[u8]) -> Result<(), String>) -> Result<(), String> {
    let (wx, wy, ww, wh) = (win.0 as i64, win.1 as i64, win.2 as usize, win.3 as i64);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut cache: HashMap<u32, Arc<RgbImage>> = HashMap::new();
    let mut band = Vec::new();
    let mut top = wy;
    while top < wy + wh {
        let bottom = (top + FASCIA as i64).min(wy + wh);
        // I pezzi che toccano la fascia (e la finestra), nell'ordine in cui si dipingono.
        let da = g.pezzi.partition_point(|p| p.y + (g.alto_max as i64) <= top);
        let mut qui: Vec<&Pezzo> = g.pezzi[da..]
            .iter()
            .take_while(|p| p.y < bottom)
            .filter(|p| p.y + p.h as i64 > top && p.x < wx + ww as i64 && p.x + p.w as i64 > wx)
            .collect();
        qui.sort_by_key(|p| p.ordine);
        // Le immagini da decodificare, in parallelo; quelle già pronte dalla fascia prima restano.
        let servono: Vec<u32> = {
            let mut v: Vec<u32> = qui.iter().map(|p| p.img).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        cache.retain(|k, _| servono.binary_search(k).is_ok());
        let mancano: Vec<u32> = servono.iter().copied().filter(|k| !cache.contains_key(k)).collect();
        let per = mancano.len().div_ceil(workers).max(1);
        let fatte: Vec<Result<Vec<(u32, RgbImage)>, String>> = std::thread::scope(|s| {
            let h: Vec<_> = mancano
                .chunks(per)
                .map(|gruppo| s.spawn(move || gruppo.iter().map(|&i| g.decodifica(i).map(|im| (i, im))).collect::<Result<Vec<_>, _>>()))
                .collect();
            h.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("decodifica interrotta".into()))).collect()
        });
        for r in fatte {
            for (i, im) in r? {
                cache.insert(i, Arc::new(im));
            }
        }
        let rows = (bottom - top) as usize;
        band.clear();
        band.reserve(ww * rows * 3);
        for _ in 0..ww * rows {
            band.extend_from_slice(&fill);
        }
        for p in qui {
            let im = &cache[&p.img];
            // La parte del pezzo che sta nell'immagine, nella fascia e nella finestra.
            let w = p.w.min(im.width().saturating_sub(p.sx)) as i64;
            let h = p.h.min(im.height().saturating_sub(p.sy)) as i64;
            let x0 = p.x.max(wx);
            let x1 = (p.x + w).min(wx + ww as i64);
            let y0 = p.y.max(top);
            let y1 = (p.y + h).min(bottom);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let n = (x1 - x0) as usize * 3;
            for y in y0..y1 {
                let sy = (p.sy as i64 + y - p.y) as u32;
                let sx = (p.sx as i64 + x0 - p.x) as u32;
                let src = (sy as usize * im.width() as usize + sx as usize) * 3;
                let dst = ((y - top) as usize * ww + (x0 - wx) as usize) * 3;
                band[dst..dst + n].copy_from_slice(&im.as_raw()[src..src + n]);
            }
        }
        sink(top as u32, &band)?;
        top = bottom;
    }
    Ok(())
}

/// Le chiavi di `Slidedat.ini` che possono dire di chi è il vetrino o chi l'ha fatto.
fn chiave_personale(k: &str) -> bool {
    let k = k.to_ascii_uppercase();
    matches!(k.as_str(), "SLIDE_NAME" | "PROJECT_NAME" | "SLIDE_CREATIONDATETIME" | "SLIDE_UTC_CREATIONDATETIME")
        || ["BARCODE", "USER", "OPERATOR", "COMMENT", "PATIENT", "AUTHOR", "COMPUTER"].iter().any(|p| k.contains(p)) && !["_TYPE", "_WIDTH", "_HEIGHT", "_FORMAT"].iter().any(|f| k.ends_with(f))
}

/// Una copia anonima del MIRAX in `out` (un `.mrxs` con accanto la sua cartella): l'etichetta e la
/// foto del vetrino (con l'etichetta) diventano bianche, e nome del vetrino, progetto, data, codici a
/// barre e utente spariscono da `Slidedat.ini`. Il vetrino si apre come prima in Cedro, QuPath e
/// OpenSlide. Dice cosa ha tolto.
pub fn copia_anonima(path: &Path, out: &Path) -> Result<Vec<&'static str>, String> {
    let s = open(path)?;
    let (da, a) = (cartella(path), cartella(out));
    if a.exists() {
        return Err(format!("la cartella {} esiste già", a.display()));
    }
    let fatto = (|| -> Result<Vec<&'static str>, String> {
        fs::create_dir_all(&a).map_err(|e| e.to_string())?;
        for e in fs::read_dir(&da).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            if e.file_type().map_err(|e| e.to_string())?.is_file() {
                fs::copy(e.path(), a.join(e.file_name())).map_err(|e| e.to_string())?;
            }
        }
        fs::copy(path, out).map_err(|e| e.to_string())?;
        let mut tolti = Vec::new();
        // L'etichetta e la foto del vetrino: un JPEG bianco delle stesse misure al posto dei pixel, il resto a zero.
        for (idx, cosa) in [(s.label, "etichetta"), (s.macro_image, "foto del vetrino")] {
            let Some(b) = idx.and_then(|i| s.blobs.get(i)) else { continue };
            let (w, h) = crate::svs::read_blob_size(b).unwrap_or((64, 64));
            let mut bianco = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bianco, 50).encode_image(&RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]))).map_err(|e| e.to_string())?;
            let dest = a.join(b.path.file_name().ok_or("file di dati senza nome")?);
            let mut f = fs::OpenOptions::new().write(true).open(&dest).map_err(|e| e.to_string())?;
            f.seek(SeekFrom::Start(b.offset)).map_err(|e| e.to_string())?;
            if bianco.len() as u64 <= b.len {
                bianco.resize(b.len as usize, 0);
            } else {
                // Non ci sta (succede solo con immagini minuscole): solo zeri, l'immagine non si legge più.
                bianco = vec![0; b.len as usize];
            }
            f.write_all(&bianco).map_err(|e| e.to_string())?;
            tolti.push(cosa);
        }
        // Slidedat.ini: le chiavi personali restano ma vuote.
        let ini = a.join("Slidedat.ini");
        // L'identificativo del vetrino (un GUID dello scanner, che riporta al caso) cambia in uno nuovo
        // della stessa lunghezza, in Slidedat.ini e in testa all'indice, che devono restare uguali.
        let letto = Ini::leggi(&ini)?;
        let vecchio_id = letto.get("GENERAL", "SLIDE_ID").unwrap_or("").to_string();
        let nuovo_id = {
            use sha2::{Digest, Sha256};
            let h = Sha256::digest([vecchio_id.as_bytes(), b"cedro-anonimo"].concat());
            let hex: String = h.iter().map(|b| format!("{b:02X}")).collect::<String>().repeat(2);
            hex[..vecchio_id.len().min(hex.len())].to_string()
        };
        let cambia_id = !vecchio_id.is_empty() && nuovo_id.len() == vecchio_id.len() && vecchio_id.is_ascii();
        if cambia_id {
            let indice = a.join(letto.serve("HIERARCHICAL", "INDEXFILE")?);
            let mut f = fs::OpenOptions::new().read(true).write(true).open(&indice).map_err(|e| e.to_string())?;
            let mut testa = vec![0u8; VERSIONE_INDICE.len() + vecchio_id.len()];
            f.read_exact(&mut testa).map_err(|e| e.to_string())?;
            if &testa[VERSIONE_INDICE.len()..] == vecchio_id.as_bytes() {
                f.seek(SeekFrom::Start(VERSIONE_INDICE.len() as u64)).map_err(|e| e.to_string())?;
                f.write_all(nuovo_id.as_bytes()).map_err(|e| e.to_string())?;
            }
        }
        let testo = fs::read(&ini).map_err(|e| e.to_string())?;
        let mut cambiato = false;
        let mut nuovo = String::new();
        for riga in String::from_utf8_lossy(&testo).split_inclusive('\n') {
            match riga.split_once('=') {
                Some((k, v)) if cambia_id && k.trim().trim_start_matches('\u{feff}') == "SLIDE_ID" && v.trim() == vecchio_id => {
                    nuovo.push_str(&riga.replacen(&vecchio_id, &nuovo_id, 1));
                    cambiato = true;
                }
                Some((k, v)) if chiave_personale(k.trim()) && !v.trim().is_empty() => {
                    let fine = if riga.ends_with("\r\n") { "\r\n" } else if riga.ends_with('\n') { "\n" } else { "" };
                    nuovo.push_str(&format!("{}= {fine}", k));
                    cambiato = true;
                }
                _ => nuovo.push_str(riga),
            }
        }
        if cambiato {
            fs::write(&ini, nuovo).map_err(|e| e.to_string())?;
            tolti.push("nome, progetto, data, codici e identificativo del vetrino da Slidedat.ini");
        }
        Ok(tolti)
    })();
    if fatto.is_err() {
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_file(out);
    }
    fatto
}

#[cfg(test)]
mod prove {
    use super::*;

    /// Un MIRAX piccolo fatto qui: `nx` × `ny` immagini JPEG da `iw` × `ih`, due livelli, una foto
    /// per immagine (divisioni 1), posizioni con una sovrapposizione di `ov` pixel, etichetta.
    pub(crate) fn mirax_finto(dir: &Path, nx: i64, ny: i64, iw: u32, ih: u32, ov: i64) -> PathBuf {
        let cart = dir.join("finto");
        fs::create_dir_all(&cart).unwrap();
        let id = "0123456789ABCDEF0123456789ABCDEF";
        let mut dati: Vec<u8> = Vec::new();
        let jpeg = |img: &RgbImage| {
            let mut b = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 95).encode_image(img).unwrap();
            b
        };
        // Livello 0: immagine (x, y) con un colore suo; livello 1: una sola immagine per 2 × 2.
        let colore = |x: i64, y: i64| image::Rgb([(40 + x * 60) as u8, (40 + y * 60) as u8, 128u8]);
        let mut rec0 = Vec::new();
        for y in 0..ny {
            for x in 0..nx {
                let b = jpeg(&RgbImage::from_pixel(iw, ih, colore(x, y)));
                rec0.push(((y * nx + x) as i32, dati.len() as i32, b.len() as i32, 0i32));
                dati.extend_from_slice(&b);
            }
        }
        let mut rec1 = Vec::new();
        for y in (0..ny).step_by(2) {
            for x in (0..nx).step_by(2) {
                let b = jpeg(&RgbImage::from_pixel(iw, ih, colore(x, y)));
                rec1.push(((y * nx + x) as i32, dati.len() as i32, b.len() as i32, 0i32));
                dati.extend_from_slice(&b);
            }
        }
        // Posizioni (flag, x, y) in pixel del livello pieno.
        let pos_at = dati.len() as i32;
        for y in 0..ny {
            for x in 0..nx {
                dati.push(1);
                dati.extend_from_slice(&((x * (iw as i64 - ov)) as i32).to_le_bytes());
                dati.extend_from_slice(&((y * (ih as i64 - ov)) as i32).to_le_bytes());
            }
        }
        let pos_len = dati.len() as i32 - pos_at;
        let label = jpeg(&RgbImage::from_pixel(40, 30, image::Rgb([200, 0, 0])));
        let label_at = dati.len() as i32;
        dati.extend_from_slice(&label);
        fs::write(cart.join("Data0000.dat"), &dati).unwrap();

        // Index.dat: versione, id, radice gerarchica, radice non gerarchica, poi le tabelle.
        let mut ix: Vec<u8> = Vec::new();
        ix.extend_from_slice(b"01.02");
        ix.extend_from_slice(id.as_bytes());
        let hier_ptr_at = ix.len();
        ix.extend_from_slice(&[0; 8]);
        let put = |ix: &mut Vec<u8>, at: usize, v: i32| ix[at..at + 4].copy_from_slice(&v.to_le_bytes());
        // Tabella dei livelli: due puntatori.
        let tab = ix.len();
        put(&mut ix, hier_ptr_at, tab as i32);
        ix.extend_from_slice(&[0; 8]);
        for (k, rec) in [&rec0, &rec1].iter().enumerate() {
            let testa = ix.len();
            put(&mut ix, tab + k * 4, testa as i32);
            ix.extend_from_slice(&0i32.to_le_bytes());
            ix.extend_from_slice(&((testa + 8) as i32).to_le_bytes());
            ix.extend_from_slice(&(rec.len() as i32).to_le_bytes());
            ix.extend_from_slice(&0i32.to_le_bytes());
            for (a, b, c, d) in rec.iter() {
                for v in [a, b, c, d] {
                    ix.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        // Non gerarchici: record 0 = posizioni, record 1 = etichetta.
        let nh_tab = ix.len();
        put(&mut ix, hier_ptr_at + 4, nh_tab as i32);
        ix.extend_from_slice(&[0; 8]);
        for (k, (off, len)) in [(pos_at, pos_len), (label_at, label.len() as i32)].iter().enumerate() {
            let testa = ix.len();
            put(&mut ix, nh_tab + k * 4, testa as i32);
            ix.extend_from_slice(&0i32.to_le_bytes());
            ix.extend_from_slice(&((testa + 8) as i32).to_le_bytes());
            for v in [1i32, 0, 0, 0, *off, *len, 0] {
                ix.extend_from_slice(&v.to_le_bytes());
            }
        }
        fs::write(cart.join("Index.dat"), &ix).unwrap();
        let sez = |c: i32| format!("IMAGE_CONCAT_FACTOR = {c}\nOVERLAP_X = {ov}\nOVERLAP_Y = {ov}\nMICROMETER_PER_PIXEL_X = 0.243\nMICROMETER_PER_PIXEL_Y = 0.243\nIMAGE_FILL_COLOR_BGR = 16777215\nIMAGE_FORMAT = JPEG\nDIGITIZER_WIDTH = {iw}\nDIGITIZER_HEIGHT = {ih}\n");
        let ini = format!(
            "[GENERAL]\nSLIDE_ID = {id}\nSLIDE_NAME = Rossi Fido\nIMAGENUMBER_X = {nx}\nIMAGENUMBER_Y = {ny}\nOBJECTIVE_MAGNIFICATION = 20\nCameraImageDivisionsPerSide = 1\n\
             [HIERARCHICAL]\nINDEXFILE = Index.dat\nHIER_COUNT = 1\nNONHIER_COUNT = 2\nHIER_0_NAME = Slide zoom level\nHIER_0_COUNT = 2\n\
             HIER_0_VAL_0_SECTION = L0\nHIER_0_VAL_1_SECTION = L1\n\
             NONHIER_0_NAME = VIMSLIDE_POSITION_BUFFER\nNONHIER_0_COUNT = 1\nNONHIER_0_VAL_0 = default\n\
             NONHIER_1_NAME = Scan data layer\nNONHIER_1_COUNT = 1\nNONHIER_1_VAL_0 = ScanDataLayer_SlideBarcode\nNONHIER_1_VAL_0_SECTION = BAR\n\
             [BAR]\nBARCODE_IMAGE_TYPE = JPEG\n[DATAFILE]\nFILE_COUNT = 1\nFILE_0 = Data0000.dat\n[L0]\n{}[L1]\n{}",
            sez(0),
            sez(1)
        );
        fs::write(cart.join("Slidedat.ini"), ini).unwrap();
        let p = dir.join("finto.mrxs");
        fs::write(&p, b"").unwrap();
        p
    }

    #[test]
    fn un_mirax_fatto_qui_si_ricompone() {
        let d = std::env::temp_dir().join(format!("cedro-mrxs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let p = mirax_finto(&d, 4, 4, 64, 48, 4);
        assert!(is_mirax(&p) && crate::svs::is_slide(&p));
        let s = Slide::open(&p).unwrap();
        assert_eq!(s.vendor, Vendor::Mirax);
        assert_eq!(s.levels.len(), 2);
        // Quattro immagini da 64 meno tre sovrapposizioni da 4 (divisioni 1: ogni immagine è una foto).
        assert_eq!((s.full().width, s.full().height), (4 * 64 - 3 * 4, 4 * 48 - 3 * 4));
        assert_eq!(s.magnification, Some(20.0));
        assert!(s.label.is_some());
        let img = s.read_level(&p, 0).unwrap();
        // Il centro di ogni immagine ha il suo colore (a meno della compressione).
        for (x, y) in [(0i64, 0i64), (3, 1), (2, 3)] {
            let c = img.get_pixel((x * 60 + 30) as u32, (y * 44 + 24) as u32);
            let atteso = [(40 + x * 60) as i32, (40 + y * 60) as i32, 128];
            for k in 0..3 {
                assert!((c[k] as i32 - atteso[k]).abs() <= 4, "{x},{y}: {c:?} contro {atteso:?}");
            }
        }
        // Una zona letta da sola è uguale allo stesso pezzo del livello intero.
        let z = s.read_region(&p, 0, 50, 30, 120, 100).unwrap();
        assert_eq!(z, image::imageops::crop_imm(&img, 50, 30, 120, 100).to_image());
        // L'etichetta si legge, poi la copia anonima la rende bianca e toglie il nome.
        let lab = s.read_extra(&p, s.label.unwrap()).unwrap();
        assert!(lab.get_pixel(20, 15)[0] > 150 && lab.get_pixel(20, 15)[1] < 60);
        let out = d.join("anonimo.mrxs");
        let tolti = copia_anonima(&p, &out).unwrap();
        assert!(tolti.contains(&"etichetta"), "{tolti:?}");
        let s2 = Slide::open(&out).unwrap();
        let lab2 = s2.read_extra(&out, s2.label.unwrap()).unwrap();
        assert!(lab2.get_pixel(20, 15)[1] > 240, "{:?}", lab2.get_pixel(20, 15));
        let ini_anonimo = fs::read_to_string(d.join("anonimo").join("Slidedat.ini")).unwrap();
        assert!(!ini_anonimo.contains("Rossi") && !ini_anonimo.contains("0123456789ABCDEF0123456789ABCDEF"), "{ini_anonimo}");
        assert!(!fs::read(d.join("anonimo").join("Index.dat")).unwrap().windows(32).any(|w| w == b"0123456789ABCDEF0123456789ABCDEF"));
        assert_eq!(s2.read_level(&out, 1).unwrap(), s.read_level(&p, 1).unwrap());
        // Dal motore: la copia anonima è un .mrxs col codice del caso e la sua cartella; il consulto
        // porta anche la cartella dei dati, con le impronte.
        let o = crate::engine::Options::default();
        let e = crate::engine::perform_with(&p, "strip", 80, None, &o).unwrap();
        let nome = e.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nome.starts_with("caso-") && nome.ends_with("-anonimo.mrxs"), "{nome}");
        assert!(e.path.with_extension("").join("Slidedat.ini").exists() && Slide::open(&e.path).is_ok());
        // Di nuovo: non pesta la cartella di prima.
        let e2 = crate::engine::perform_with(&p, "strip", 80, None, &o).unwrap();
        assert_ne!(e2.path, e.path, "{}", e2.message);
        let c = crate::engine::perform_with(&p, "consult", 80, None, &o).unwrap();
        let v = crate::pacchetto::verifica(&c.path, &|_| {}).unwrap();
        assert!(v.tutto_bene() && v.integri > 3, "{v:?}");
        let _ = fs::remove_dir_all(&d);
    }
}
