//! Le conversioni delle immagini di microscopia (OME-TIFF, ImageJ, TIFF a 16 bit, ND2, LIF, CZI): i canali sovrapposti
//! a colori o uno per file, i piani Z uniti (proiezione massima) o separati, un file per istante,
//! contrasto automatico, valori originali a 16 bit e barra di scala. Le voci che il TIFF non cambia
//! (OCR, icona, tira via i dati) passano dal percorso comune.

use std::path::{Path, PathBuf};

use image::DynamicImage;

use super::dicom_out::in_parallel;
use super::{err, image_target, jpeg_bytes, reserve, save_image, skipping, stem, unique_output, write_bytes, CedroResult, Options, Output};
use crate::microscopia::{self, Piani, Pila, Piano};

/// `None` se la voce non è di quelle che la microscopia tratta a modo suo.
pub(super) fn perform(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> Option<CedroResult<Output>> {
    if image_target(action).is_none() || matches!(action, "ico" | "strip") {
        return None;
    }
    Some(run(path, action, quality, dir, opts))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pezzo {
    Composito,
    Canale(usize),
}

/// Un lavoro: un istante (e un piano, se si vogliono separati) di un'immagine del file, con i suoi
/// file di uscita.
struct Lavoro {
    s: usize,
    t: usize,
    z: Option<usize>,
    uscite: Vec<(Pezzo, PathBuf)>,
}

fn nome_file(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect::<String>().trim_matches('_').to_string()
}

/// Cosa esce per ogni piano: i 16 bit sono sempre un file per canale (niente colori).
fn pezzi_di(pila: &Pila, sedici: bool, opts: &Options) -> Vec<Pezzo> {
    let uno_solo = pila.canali.len() == 1;
    if sedici {
        (0..pila.canali.len()).map(Pezzo::Canale).collect()
    } else {
        match opts.micro_channels {
            1 if !uno_solo => (0..pila.canali.len()).map(Pezzo::Canale).collect(),
            2 if !uno_solo => std::iter::once(Pezzo::Composito).chain((0..pila.canali.len()).map(Pezzo::Canale)).collect(),
            _ => vec![Pezzo::Composito],
        }
    }
}

fn run(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> CedroResult<Output> {
    // Un LIF è un progetto con più immagini (un CZI può avere più scene): ognuna dà i suoi file.
    let pile = Pila::open_tutte(path).map_err(err)?;
    let pila = &pile[0];
    let (format, ext, label) = image_target(action).ok_or_else(|| err(format!("azione sconosciuta: {action}")))?;
    let base = if label.is_empty() { stem(path) } else { format!("{}-{label}", stem(path)) };
    let sedici = opts.micro_16bit && matches!(action, "png" | "tiff");
    let quality = if action == "compress" { quality.min(crate::settings::read_settings().compress_quality()) } else { quality };
    let piani = if opts.micro_z == 1 { Piani::Centrale } else { Piani::Massimo };
    let istanti_di = |p: &Pila| -> Vec<usize> { if opts.micro_all_t { (0..p.t).collect() } else { vec![0] } };
    let zeta_di = |p: &Pila| -> Vec<Option<usize>> { if opts.micro_z == 2 && p.z > 1 { (0..p.z).map(Some).collect() } else { vec![None] } };
    let pezzi: Vec<Vec<Pezzo>> = pile.iter().map(|p| pezzi_di(p, sedici, opts)).collect();

    // I nomi si scelgono qui, in ordine; lettura e scrittura vanno poi in parallelo.
    let mut lavori: Vec<Lavoro> = Vec::new();
    let mut salti = 0usize;
    if action != "pdf" {
        for (s, p) in pile.iter().enumerate() {
            let base = if pile.len() > 1 { format!("{base}-{}", nome_file(&p.serie.get(s).cloned().unwrap_or_else(|| format!("{}", s + 1)))) } else { base.clone() };
            let (istanti, zeta) = (istanti_di(p), zeta_di(p));
            let cifre_t = p.t.to_string().len().max(2);
            let cifre_z = p.z.to_string().len().max(2);
            let uno_solo = p.canali.len() == 1;
            for &t in &istanti {
                for &z in &zeta {
                    let mut uscite = Vec::new();
                    for &pezzo in &pezzi[s] {
                        let mut nome = base.clone();
                        if istanti.len() > 1 {
                            nome.push_str(&format!("-t{:0cifre_t$}", t + 1));
                        }
                        if let Some(z) = z {
                            nome.push_str(&format!("-z{:0cifre_z$}", z + 1));
                        }
                        if let Pezzo::Canale(c) = pezzo {
                            // Un solo canale di un file con un solo canale non ha bisogno del suo nome.
                            if !(uno_solo && pezzi[s].len() == 1) {
                                nome.push_str(&format!("-{}", nome_file(&p.canali[c].nome)));
                            }
                        }
                        let out = if uscite.is_empty() && lavori.is_empty() { reserve(dir, &nome, ext)? } else { unique_output(dir, &nome, ext) };
                        if skipping(&out) {
                            salti += 1;
                            continue;
                        }
                        uscite.push((pezzo, out));
                    }
                    if !uscite.is_empty() {
                        lavori.push(Lavoro { s, t, z, uscite });
                    }
                }
            }
        }
        if lavori.is_empty() {
            return Err(err(if salti > 0 { "i file esistono già: saltati" } else { "niente da scrivere" }));
        }
    }

    let con_barra = opts.micro_scale && !sedici;
    let barra = |s: usize| pile[s].um_per_px.filter(|_| con_barra);
    let scritta_barra = std::sync::Mutex::new(None::<String>);
    let contrasto = |s: usize| opts.micro_contrast && !pile[s].naturale();

    // I lavori: (immagine, istante, piano Z o tutti), nello stesso ordine dei file e delle pagine.
    let jobs: Vec<(usize, usize, Option<usize>)> = if action == "pdf" {
        let mut v = Vec::new();
        for (s, p) in pile.iter().enumerate() {
            for t in istanti_di(p) {
                for z in zeta_di(p) {
                    v.push((s, t, z));
                }
            }
        }
        v
    } else {
        lavori.iter().map(|l| (l.s, l.t, l.z)).collect()
    };
    // I piani di un lavoro.
    let piani_di = |s: usize, t: usize, z: Option<usize>| -> Result<Vec<Piano>, String> {
        match z {
            Some(z) => pile[s].canali_di_z(path, z, t),
            None => pile[s].canali_di(path, t, piani),
        }
    };

    // Il contrasto automatico: gli stessi limiti per ogni canale in tutti gli istanti e i piani della
    // stessa immagine, così due istanti (o due piani) restano confrontabili. Si leggono prima tutti i
    // piani una volta per l'istogramma; con un lavoro solo i piani letti si tengono per disegnarli.
    let indici: Vec<usize> = (0..jobs.len()).collect();
    let istogrammi: Vec<std::sync::Mutex<Vec<Vec<u64>>>> = pile.iter().map(|_| std::sync::Mutex::new(Vec::new())).collect();
    let tenuti = std::sync::Mutex::new(None::<Vec<Piano>>);
    if jobs.iter().any(|j| contrasto(j.0)) && !sedici {
        in_parallel(&indici, |k| {
            let (s, t, z) = jobs[k];
            if !contrasto(s) {
                return Ok(());
            }
            let c = piani_di(s, t, z)?;
            {
                let mut ist = istogrammi[s].lock().unwrap_or_else(|e| e.into_inner());
                if ist.len() < c.len() {
                    ist.resize(c.len(), Vec::new());
                }
                for (i, p) in c.iter().enumerate() {
                    microscopia::aggiungi_istogramma(&mut ist[i], p);
                }
            }
            if jobs.len() == 1 {
                *tenuti.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
            }
            Ok(())
        })
        .map_err(err)?;
    }
    let limiti: Vec<Option<Vec<(u16, u16)>>> = istogrammi
        .into_iter()
        .map(|m| {
            let ist = m.into_inner().unwrap_or_else(|e| e.into_inner());
            (!ist.is_empty()).then(|| ist.iter().map(|h| microscopia::limiti_da(h)).collect())
        })
        .collect();
    let piani_del = |k: usize| -> Result<Vec<Piano>, String> {
        if let Some(c) = tenuti.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Ok(c);
        }
        let (s, t, z) = jobs[k];
        piani_di(s, t, z)
    };
    let limiti_per = |s: usize, c: &[Piano]| -> Vec<(u16, u16)> {
        match &limiti[s] {
            Some(l) => l.clone(),
            None => microscopia::limiti_di(&pile[s], c, false),
        }
    };

    // Un'immagine pronta: i ritocchi della voce (misure, bianco e nero, rotazione) dopo la barra.
    let ritocca = |mut img: DynamicImage| -> DynamicImage {
        if let Some(side) = opts.max_side.filter(|s| *s > 0 && img.width().max(img.height()) > *s) {
            img = img.resize(side, side, image::imageops::FilterType::Lanczos3);
        }
        match action {
            "half" => img.resize_exact((img.width() / 2).max(1), (img.height() / 2).max(1), image::imageops::FilterType::Lanczos3),
            "gray" => DynamicImage::ImageLuma8(img.to_luma8()),
            "rot90" => img.rotate90(),
            _ => img,
        }
    };
    // Disegnata prima del ridimensionamento: la barra segue la scala dell'immagine intera.
    let con_scala = |img: DynamicImage, s: usize| -> DynamicImage {
        let mut rgb = img.to_rgb8();
        if let Some(um) = barra(s) {
            if let Some(t) = microscopia::disegna_barra(&mut rgb, um) {
                *scritta_barra.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
            }
        }
        DynamicImage::ImageRgb8(rgb)
    };
    let disegna = |s: usize, c: &[Piano], pezzo: Pezzo| -> DynamicImage {
        let p = &pile[s];
        match pezzo {
            Pezzo::Composito => ritocca(con_scala(DynamicImage::ImageRgb8(microscopia::composita_con(p, c, &limiti_per(s, c))), s)),
            Pezzo::Canale(i) if sedici => ritocca(microscopia::sedici_bit(p, &c[i])),
            Pezzo::Canale(i) => {
                // Un canale a parte è in grigio, ma con la barra serve il colore (RGB grigio).
                let lim = limiti_per(s, c).get(i).copied().unwrap_or((0, c[i].scala));
                let g = microscopia::grigio_con(p, &c[i], lim);
                ritocca(if barra(s).is_some() { con_scala(g, s) } else { g })
            }
        }
    };

    let notes = || -> Vec<String> {
        let mut v = Vec::new();
        if pile.len() > 1 {
            v.push(format!("{} immagini del progetto", pile.len()));
        }
        if sedici {
            v.push("valori originali a 16 bit, un file per canale, con la misura del pixel (OME-XML nel TIFF)".to_string());
        } else {
            match (opts.micro_channels, pila.canali.len()) {
                (_, 1) => {}
                (1, n) => v.push(format!("{n} canali, un file per canale")),
                (2, n) => v.push(format!("{n} canali sovrapposti e uno per file")),
                (_, n) => v.push(format!("{n} canali sovrapposti")),
            }
            // In un progetto LIF conta l'immagine con più piani.
            let z = pile.iter().map(|p| p.z).max().unwrap_or(1);
            if z > 1 {
                v.push(match opts.micro_z {
                    2 => format!("un file per piano Z ({z})"),
                    1 => "piano Z centrale".to_string(),
                    _ => format!("proiezione massima di {z} piani Z"),
                });
            }
            // Il contrasto cambia i valori: si dice come, con i limiti di ogni canale.
            match (pile.len(), &limiti[0]) {
                (1, Some(l)) if jobs.len() > 1 => v.push(format!("contrasto automatico, uguale in tutti i file ({})", microscopia::descrivi_limiti(pila, l))),
                (1, Some(l)) => v.push(format!("contrasto automatico ({})", microscopia::descrivi_limiti(pila, l))),
                _ if limiti.iter().any(Option::is_some) => v.push("contrasto automatico, uguale in tutti i file di ogni immagine".to_string()),
                _ => {}
            }
        }
        if pile.iter().any(|p| p.t > 1) && !opts.micro_all_t {
            v.push("solo il primo istante".to_string());
        }
        let mut avvisi: Vec<&str> = pile.iter().filter_map(|p| p.avviso.as_deref()).collect();
        avvisi.dedup();
        v.extend(avvisi.into_iter().map(str::to_string));
        v
    };

    let (messaggio, primo) = if action == "pdf" {
        let out = reserve(dir, &base, "pdf")?;
        let pagine = in_parallel(&indici, |k| {
            let s = jobs[k].0;
            let c = piani_del(k)?;
            let mut pagine = Vec::new();
            for &pezzo in &pezzi[s] {
                let img = disegna(s, &c, pezzo);
                pagine.push(jpeg_bytes(&img, 90, opts.white_background).map_err(|e| e.to_string())?);
            }
            Ok(pagine)
        })
        .map_err(err)?;
        let pagine: Vec<_> = pagine.into_iter().flatten().collect();
        write_bytes(&out, &crate::pdf::pdf_from_jpegs(&pagine))?;
        let mut m = if pagine.len() == 1 { "PDF".to_string() } else { format!("PDF di {} pagine, un'immagine per pagina", pagine.len()) };
        for n in notes() {
            m.push_str(" · ");
            m.push_str(&n);
        }
        (m, out)
    } else {
        let risultato = in_parallel(&indici, |k| {
            let l = &lavori[k];
            let c = piani_del(k)?;
            let mut misure = (0, 0);
            for (pezzo, out) in &l.uscite {
                let img = disegna(l.s, &c, *pezzo);
                misure = (img.width(), img.height());
                match (action, pezzo) {
                    (_, Pezzo::Canale(i)) if sedici => {
                        let p = &pile[l.s];
                        // Il pixel dopo un eventuale ridimensionamento.
                        let um = p.um_per_px.map(|u| u * f64::from(p.larghezza) / f64::from(img.width().max(1)));
                        let nome = out.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        scrivi_sedici(&img, out, action == "tiff", &nome, &p.canali[*i].nome, um)?
                    }
                    ("avif", _) => std::fs::write(out, crate::imgfmt::avif_bytes(&img, quality)?).map_err(|e| e.to_string())?,
                    ("jxl", _) => std::fs::write(out, crate::imgfmt::jxl_bytes(&img)?).map_err(|e| e.to_string())?,
                    _ => save_image(&img, out, format, quality, opts.white_background).map_err(|e| e.to_string())?,
                }
            }
            Ok(misure)
        });
        let misure = match risultato {
            Ok(m) => m,
            Err(e) => {
                for l in &lavori {
                    for (_, p) in &l.uscite {
                        let _ = std::fs::remove_file(p);
                    }
                }
                return Err(err(e));
            }
        };
        let totale: usize = lavori.iter().map(|l| l.uscite.len()).sum();
        let (w, h) = misure[0];
        let up = ext.to_uppercase();
        let mut m = if totale == 1 { format!("{up} {w}×{h}") } else { format!("{totale} {up}, uno per immagine ({w}×{h})") };
        for n in notes() {
            m.push_str(" · ");
            m.push_str(&n);
        }
        (m, lavori[0].uscite[0].1.clone())
    };
    let mut messaggio = messaggio;
    if con_barra {
        match (&*scritta_barra.lock().unwrap_or_else(|e| e.into_inner()), pila.um_per_px) {
            (Some(t), _) => messaggio.push_str(&format!(" · barra di scala {t}")),
            (None, None) => messaggio.push_str(". Il file non dice quanto è grande un pixel: senza barra di scala"),
            _ => {}
        }
    }
    Ok(Output { path: primo, message: messaggio })
}

/// Un canale a 16 bit con i valori originali. Il TIFF porta un OME-XML minimo con la misura del pixel
/// in µm (Fiji con Bio-Formats lo legge) e la risoluzione in pixel per centimetro; il PNG la sua
/// risoluzione in pixel per metro (pHYs).
fn scrivi_sedici(img: &DynamicImage, out: &Path, tiff: bool, nome: &str, canale: &str, um: Option<f64>) -> Result<(), String> {
    let g = img.to_luma16();
    let (w, h) = g.dimensions();
    let um = um.filter(|u| u.is_finite() && *u > 0.0);
    let file = std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| e.to_string())?);
    if tiff {
        use tiff::encoder::{colortype, Compression, Rational, TiffEncoder};
        let mut file = file;
        let mut enc = TiffEncoder::new(&mut file).map_err(|e| e.to_string())?.with_compression(Compression::Lzw);
        let mut im = enc.new_image::<colortype::Gray16>(w, h).map_err(|e| e.to_string())?;
        let xml = microscopia::ome_xml(nome, canale, w, h, um);
        im.encoder().write_tag(tiff::tags::Tag::ImageDescription, xml.as_str()).map_err(|e| e.to_string())?;
        if let Some(u) = um {
            // Pixel per centimetro come frazione: il denominatore si sceglie perché il numeratore stia in 32 bit.
            let per_cm = 10_000.0 / u;
            let d: u32 = if per_cm < 4_000_000.0 { 1000 } else { 1 };
            im.resolution(tiff::tags::ResolutionUnit::Centimeter, Rational { n: (per_cm * f64::from(d)).round().min(f64::from(u32::MAX)) as u32, d });
        }
        im.write_data(g.as_raw()).map_err(|e| e.to_string())?;
        drop(enc);
        use std::io::Write;
        file.flush().map_err(|e| e.to_string())
    } else {
        let mut e = png::Encoder::new(file, w, h);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Sixteen);
        if let Some(u) = um {
            let per_m = (1_000_000.0 / u).round().min(f64::from(u32::MAX)) as u32;
            e.set_pixel_dims(Some(png::PixelDimensions { xppu: per_m, yppu: per_m, unit: png::Unit::Meter }));
        }
        let mut wr = e.write_header().map_err(|e| e.to_string())?;
        let dati: Vec<u8> = g.as_raw().iter().flat_map(|v| v.to_be_bytes()).collect();
        wr.write_image_data(&dati).map_err(|e| e.to_string())?;
        wr.finish().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod prove {
    use super::super::{perform_with, Options};
    use crate::microscopia::prove::ome;

    fn cartella(nome: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cedro-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn un_ome_diventa_un_png_a_colori_con_la_barra() {
        let d = cartella("micro-png");
        let p = ome(&d);
        let out = perform_with(&p, "png", 80, None, &Options::default()).unwrap();
        assert!(out.path.file_name().unwrap().to_string_lossy().ends_with(".png"));
        let img = image::open(&out.path).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (12, 8));
        // 12 px × 0,5 µm = 6 µm: la barra tonda più lunga sotto il 22% è 1 µm (2 px < 8 px: troppo corta), quindi senza barra.
        assert!(out.message.contains("2 canali sovrapposti") && out.message.contains("proiezione massima di 3 piani Z") && out.message.contains("contrasto automatico"), "{}", out.message);
        assert_eq!(img.get_pixel(5, 5)[1], 255);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn canali_separati_sedici_bit_e_un_piano_per_file() {
        let d = cartella("micro-canali");
        let p = ome(&d);
        // Canali separati, un file per piano Z.
        let o = Options { micro_channels: 1, micro_z: 2, ..Options::default() };
        let out = perform_with(&p, "png", 80, None, &o).unwrap();
        let mut nomi: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".png")).collect();
        nomi.sort();
        assert_eq!(nomi.len(), 6, "{nomi:?}");
        assert!(nomi.iter().any(|n| n.contains("-z03-GFP")), "{nomi:?}");
        assert!(out.message.contains("6 PNG"), "{}", out.message);
        // I 16 bit: un file per canale, valori originali.
        let o16 = Options { micro_16bit: true, ..Options::default() };
        let out = perform_with(&p, "tiff", 80, None, &o16).unwrap();
        assert!(out.message.contains("16 bit"), "{}", out.message);
        let dapi = image::open(d.join("ome.ome-DAPI.tiff")).or_else(|_| image::open(&out.path)).unwrap();
        assert!(matches!(dapi, image::DynamicImage::ImageLuma16(_)), "{:?}", dapi.color());
        let valori: Vec<u16> = dapi.to_luma16().pixels().map(|p| p.0[0]).filter(|v| *v > 100).collect();
        assert_eq!(valori, [4000, 4100, 4200]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nd2_e_lif_si_convertono_come_gli_ome() {
        let d = cartella("micro-nd2-lif");
        // ND2: 2 canali, 3 piani Z, 2 istanti; tutti gli istanti, canali separati.
        let p = crate::nd2::prove::nd2(&d, true);
        let pila = crate::microscopia::Pila::open(&p).unwrap();
        assert_eq!(pila.riassunto()[0], "ND2 di Nikon · 2 canali · 3 piani Z · 2 istanti · 12 bit · 8×6 · 0,65 µm/pixel · Plan Fluor 20x");
        let o = Options { micro_channels: 1, micro_all_t: true, ..Options::default() };
        let out = perform_with(&p, "png", 80, None, &o).unwrap();
        assert!(out.message.contains("4 PNG"), "{}", out.message);
        let gfp = image::open(d.join("zlib-t02-GFP.png")).unwrap().to_luma8();
        assert_eq!(gfp.dimensions(), (8, 6));
        // Proiezione massima del secondo istante: il pixel in alto a sinistra (1121) è il più alto.
        assert_eq!(gfp.get_pixel(0, 0)[0], 255);
        // LIF: i file di ogni immagine del progetto (la prima ha 2 istanti).
        let p = crate::lif::prove::lif(&d);
        let out = perform_with(&p, "png", 80, None, &Options::default()).unwrap();
        let tutti: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(out.message.contains("3 PNG") && out.message.contains("2 immagini del progetto"), "{} {tutti:?}", out.message);
        assert_eq!(image::open(d.join("progetto-Serie1-t02.png")).unwrap().to_rgb8().dimensions(), (6, 4));
        assert_eq!(image::open(d.join("progetto-Cartella_XZ.png")).unwrap().to_rgb8().dimensions(), (5, 3));
        let pila = crate::microscopia::Pila::open(&p).unwrap();
        assert_eq!(pila.riassunto()[1], "Leica LIF · 2 images (this one: Serie1) · 2 channels · 3 Z planes · 2 time points · 12-bit · 6×4 · 0.25 µm/pixel · HC PL APO 63x/1.40 OIL");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn czi_si_converte_scena_per_scena() {
        let d = cartella("micro-czi");
        // Due scene: P1 (2 canali, 2 piani Z, mosaico di 2 tessere zstd) e P2.
        let p = crate::czi::prove::czi(&d);
        assert_eq!(crate::engine::kind_of(&p), crate::engine::Kind::Image);
        assert!(crate::microscopia::is_stack(&p));
        let pila = crate::microscopia::Pila::open(&p).unwrap();
        assert_eq!(pila.origine, crate::microscopia::Origine::Zeiss);
        assert_eq!(pila.riassunto()[0], "CZI di Zeiss · 2 immagini (questa: P1) · 2 canali · 2 piani Z · 12 bit · 8×4 · 0,325 µm/pixel · Plan-Apochromat 20x/0.8 M27");
        assert_eq!(pila.riassunto()[1], "Zeiss CZI · 2 images (this one: P1) · 2 channels · 2 Z planes · 12-bit · 8×4 · 0.325 µm/pixel · Plan-Apochromat 20x/0.8 M27");
        let o = Options { micro_channels: 1, ..Options::default() };
        let out = perform_with(&p, "png", 80, None, &o).unwrap();
        let tutti: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(out.message.contains("4 PNG") && out.message.contains("2 immagini del progetto"), "{} {tutti:?}", out.message);
        assert_eq!(image::open(d.join("prova-P1-EGFP.png")).unwrap().to_luma8().dimensions(), (8, 4));
        assert_eq!(image::open(d.join("prova-P2-DAPI.png")).unwrap().to_luma8().dimensions(), (3, 2));
        // L'anteprima: i canali a colori.
        let im = crate::imgfmt::open(&p, false, None).unwrap();
        assert_eq!((im.width(), im.height()), (8, 4));
        // I valori originali a 16 bit: la proiezione massima del canale EGFP (piano Z 2: 1100 + …).
        let o = Options { micro_16bit: true, ..Options::default() };
        perform_with(&p, "tiff", 80, None, &o).unwrap();
        let t = crate::microscopia::Pila::open(&d.join("prova-P1-EGFP.tiff")).unwrap();
        assert_eq!(t.canale(&d.join("prova-P1-EGFP.tiff"), 0, 0, crate::microscopia::Piani::Centrale).unwrap().dati[..3], [1100, 1101, 1102]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_figura_mette_insieme_microscopia_e_foto() {
        let d = cartella("micro-figura");
        let nd2 = crate::nd2::prove::nd2(&d, true);
        let foto = d.join("b-foto.png");
        image::RgbImage::from_pixel(300, 200, image::Rgb([200, 30, 30])).save(&foto).unwrap();
        let o = Options { figura: crate::figura::Opzioni { larghezza_mm: 85, ..Default::default() }, ..Options::default() };
        let out = super::super::figure_with(&[nd2.clone(), foto.clone()], None, &o).unwrap();
        assert!(out.path.ends_with("zlib-figura.tiff"), "{}", out.path.display());
        assert!(out.message.starts_with("Figura 8,5 × ") && out.message.contains("300 dpi, 2 pannelli. 1 con la barra di scala."), "{}", out.message);
        let tiff = image::open(&out.path).unwrap();
        // 85 mm a 300 dpi: 1004 pixel; i dpi sono scritti nel file.
        assert_eq!(tiff.width(), 1004);
        let mut dec = tiff::decoder::Decoder::new(std::fs::File::open(&out.path).unwrap()).unwrap();
        assert_eq!(dec.get_tag_u32(tiff::tags::Tag::ResolutionUnit).unwrap(), 2);
        let legenda = std::fs::read_to_string(d.join("zlib-figura-legenda.txt")).unwrap();
        assert!(legenda.contains("(A) zlib.nd2 · barra di scala") && legenda.contains("(B) b-foto.png"), "{legenda}");
        // Una foto sola dalla ruota: un pannello, senza barra.
        let out = perform_with(&foto, "figure", 80, None, &Options::default()).unwrap();
        assert!(out.message.contains("1 pannello.") && !out.message.contains("barra"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn il_pacchetto_dei_dati_si_fa_e_si_verifica() {
        let d = cartella("pacchetto");
        std::fs::create_dir_all(d.join("piastra")).unwrap();
        std::fs::write(d.join("piastra/misure.csv"), "A1;0,5\n").unwrap();
        std::fs::write(d.join("piastra/note.txt"), "ok").unwrap();
        let out = super::super::package_with(&[d.join("piastra")], None, &Options::default()).unwrap();
        assert!(out.path.ends_with("piastra-pacchetto.zip"), "{}", out.path.display());
        assert!(out.message.starts_with("Pacchetto con 2 file (1 KB), impronta SHA-256 di ognuno. Impronta dello ZIP: "), "{}", out.message);
        let sha = std::fs::read_to_string(d.join("piastra-pacchetto.zip.sha256")).unwrap();
        assert!(sha.ends_with("  piastra-pacchetto.zip\n") && sha.len() == 64 + 2 + "piastra-pacchetto.zip\n".len(), "{sha}");
        let v = perform_with(&out.path, "verify-pack", 80, None, &Options::default()).unwrap();
        assert_eq!(v.message, "Pacchetto integro: 2 file, tutte le impronte SHA-256 tornano.");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod prove_revisione {
    use super::super::{perform_with, Options};
    use std::path::Path;

    fn copia(nome: &str, cartella: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("cedro-rev-{cartella}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(nome);
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/micro").join(nome), &p).unwrap();
        (d, p)
    }

    #[test]
    fn il_contrasto_e_lo_stesso_per_tutti_gli_istanti() {
        // Due istanti con intensità 10 volte diverse: con limiti comuni il secondo resta più scuro.
        let (d, p) = copia("tempo.ome.tif", "tempo");
        let o = Options { micro_all_t: true, ..Options::default() };
        let out = perform_with(&p, "png", 80, None, &o).unwrap();
        assert!(out.message.contains("contrasto automatico, uguale in tutti i file (C1 "), "{}", out.message);
        let a = image::open(d.join("tempo.ome-t01.png")).unwrap().to_rgb8();
        let b = image::open(d.join("tempo.ome-t02.png")).unwrap().to_rgb8();
        let (va, vb) = (a.get_pixel(40, 5)[0], b.get_pixel(40, 5)[0]);
        assert!(va > 100 && vb < va / 4, "t1 {va}, t2 {vb}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn i_16_bit_portano_la_misura_del_pixel() {
        let (d, p) = copia("tempo.ome.tif", "sedici");
        let o = Options { micro_16bit: true, ..Options::default() };
        let out = perform_with(&p, "tiff", 80, None, &o).unwrap();
        // Il TIFF ha l'OME-XML: Cedro (come Bio-Formats) ci ritrova 0,5 µm per pixel e i valori originali.
        let pila = crate::microscopia::Pila::open(&out.path).unwrap();
        assert_eq!(pila.origine, crate::microscopia::Origine::Ome);
        assert_eq!(pila.um_per_px, Some(0.5));
        let piano = pila.canale(&out.path, 0, 0, crate::microscopia::Piani::Centrale).unwrap();
        assert_eq!(piano.dati[63], 6300);
        let mut dec = tiff::decoder::Decoder::new(std::fs::File::open(&out.path).unwrap()).unwrap();
        assert_eq!(dec.get_tag_u32(tiff::tags::Tag::ResolutionUnit).unwrap(), 3);
        // Il PNG a 16 bit: pixel per metro.
        let out = perform_with(&p, "png", 80, None, &o).unwrap();
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out.path).unwrap()));
        let r = dec.read_info().unwrap();
        let dims = r.info().pixel_dims.unwrap();
        assert_eq!((dims.xppu, dims.unit), (2_000_000, png::Unit::Meter));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_pezzo_di_ome_lo_dice_nel_messaggio() {
        let (d, p) = copia("multi_t1.ome.tif", "multi");
        let out = perform_with(&p, "png", 80, None, &Options::default()).unwrap();
        assert!(out.message.contains("2 canali sovrapposti") && out.message.contains("1 istanti su 3"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&d);
    }
}
