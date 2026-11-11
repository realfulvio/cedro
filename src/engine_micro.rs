//! Le conversioni delle immagini di microscopia (OME-TIFF, ImageJ, TIFF a 16 bit): i canali sovrapposti
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

/// Un lavoro: un istante (e un piano, se si vogliono separati) con i suoi file di uscita.
struct Lavoro {
    t: usize,
    z: Option<usize>,
    uscite: Vec<(Pezzo, PathBuf)>,
}

fn nome_file(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect::<String>().trim_matches('_').to_string()
}

fn run(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> CedroResult<Output> {
    let pila = Pila::open(path).map_err(err)?;
    let (format, ext, label) = image_target(action).ok_or_else(|| err(format!("azione sconosciuta: {action}")))?;
    let base = if label.is_empty() { stem(path) } else { format!("{}-{label}", stem(path)) };
    let sedici = opts.micro_16bit && matches!(action, "png" | "tiff");
    let quality = if action == "compress" { quality.min(crate::settings::read_settings().compress_quality()) } else { quality };
    let piani = if opts.micro_z == 1 { Piani::Centrale } else { Piani::Massimo };
    let istanti: Vec<usize> = if opts.micro_all_t { (0..pila.t).collect() } else { vec![0] };
    let zeta: Vec<Option<usize>> = if opts.micro_z == 2 && pila.z > 1 { (0..pila.z).map(Some).collect() } else { vec![None] };
    let uno_solo = pila.canali.len() == 1;
    // Cosa esce per ogni piano: i 16 bit sono sempre un file per canale (niente colori).
    let pezzi: Vec<Pezzo> = if sedici {
        (0..pila.canali.len()).map(Pezzo::Canale).collect()
    } else {
        match opts.micro_channels {
            1 if !uno_solo => (0..pila.canali.len()).map(Pezzo::Canale).collect(),
            2 if !uno_solo => std::iter::once(Pezzo::Composito).chain((0..pila.canali.len()).map(Pezzo::Canale)).collect(),
            _ => vec![Pezzo::Composito],
        }
    };

    // I nomi si scelgono qui, in ordine; lettura e scrittura vanno poi in parallelo.
    let cifre_t = pila.t.to_string().len().max(2);
    let cifre_z = pila.z.to_string().len().max(2);
    let mut lavori: Vec<Lavoro> = Vec::new();
    let mut salti = 0usize;
    if action != "pdf" {
        for &t in &istanti {
            for &z in &zeta {
                let mut uscite = Vec::new();
                for &pezzo in &pezzi {
                    let mut nome = base.clone();
                    if istanti.len() > 1 {
                        nome.push_str(&format!("-t{:0cifre_t$}", t + 1));
                    }
                    if let Some(z) = z {
                        nome.push_str(&format!("-z{:0cifre_z$}", z + 1));
                    }
                    if let Pezzo::Canale(c) = pezzo {
                        // Un solo canale di un file con un solo canale non ha bisogno del suo nome.
                        if !(uno_solo && pezzi.len() == 1) {
                            nome.push_str(&format!("-{}", nome_file(&pila.canali[c].nome)));
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
                    lavori.push(Lavoro { t, z, uscite });
                }
            }
        }
        if lavori.is_empty() {
            return Err(err(if salti > 0 { "i file esistono già: saltati" } else { "niente da scrivere" }));
        }
    }

    let con_barra = opts.micro_scale && !sedici;
    let barra = pila.um_per_px.filter(|_| con_barra);
    let scritta_barra = std::sync::Mutex::new(None::<String>);
    let contrasto = opts.micro_contrast && !pila.naturale();

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
    let con_scala = |img: DynamicImage| -> DynamicImage {
        let mut rgb = img.to_rgb8();
        if let Some(um) = barra {
            if let Some(t) = microscopia::disegna_barra(&mut rgb, um) {
                *scritta_barra.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
            }
        }
        DynamicImage::ImageRgb8(rgb)
    };
    // I piani di un lavoro.
    let piani_di = |t: usize, z: Option<usize>| -> Result<Vec<Piano>, String> {
        match z {
            Some(z) => pila.canali_di_z(path, z, t),
            None => pila.canali_di(path, t, piani),
        }
    };
    let disegna = |c: &[Piano], pezzo: Pezzo| -> DynamicImage {
        match pezzo {
            Pezzo::Composito => ritocca(con_scala(DynamicImage::ImageRgb8(microscopia::composita(&pila, c, contrasto)))),
            Pezzo::Canale(i) if sedici => ritocca(microscopia::sedici_bit(&pila, &c[i])),
            Pezzo::Canale(i) => {
                // Un canale a parte è in grigio, ma con la barra serve il colore (RGB grigio).
                let g = microscopia::grigio_di(&pila, &c[i], contrasto);
                ritocca(if barra.is_some() { con_scala(g) } else { g })
            }
        }
    };

    let notes = |n_piani: usize| -> Vec<String> {
        let mut v = Vec::new();
        if sedici {
            v.push("valori originali a 16 bit, un file per canale".to_string());
        } else {
            match (opts.micro_channels, pila.canali.len()) {
                (_, 1) => {}
                (1, n) => v.push(format!("{n} canali, un file per canale")),
                (2, n) => v.push(format!("{n} canali sovrapposti e uno per file")),
                (_, n) => v.push(format!("{n} canali sovrapposti")),
            }
            if pila.z > 1 && n_piani > 0 {
                v.push(match (opts.micro_z, n_piani) {
                    (2, _) => format!("un file per piano Z ({})", pila.z),
                    (1, _) => "piano Z centrale".to_string(),
                    _ => format!("proiezione massima di {} piani Z", pila.z),
                });
            }
            if contrasto {
                v.push("contrasto automatico".to_string());
            }
        }
        if pila.t > 1 && !opts.micro_all_t {
            v.push("solo il primo istante".to_string());
        }
        v
    };

    let (messaggio, primo) = if action == "pdf" {
        let out = reserve(dir, &base, "pdf")?;
        let mut jobs: Vec<(usize, Option<usize>)> = Vec::new();
        for &t in &istanti {
            for &z in &zeta {
                jobs.push((t, z));
            }
        }
        let indici: Vec<usize> = (0..jobs.len()).collect();
        let pagine = in_parallel(&indici, |k| {
            let (t, z) = jobs[k];
            let c = piani_di(t, z)?;
            let mut pagine = Vec::new();
            for &pezzo in &pezzi {
                let img = disegna(&c, pezzo);
                pagine.push(jpeg_bytes(&img, 90, opts.white_background).map_err(|e| e.to_string())?);
            }
            Ok(pagine)
        })
        .map_err(err)?;
        let pagine: Vec<_> = pagine.into_iter().flatten().collect();
        write_bytes(&out, &crate::pdf::pdf_from_jpegs(&pagine))?;
        let mut m = if pagine.len() == 1 { "PDF".to_string() } else { format!("PDF di {} pagine, un'immagine per pagina", pagine.len()) };
        for n in notes(1) {
            m.push_str(" · ");
            m.push_str(&n);
        }
        (m, out)
    } else {
        let indici: Vec<usize> = (0..lavori.len()).collect();
        let risultato = in_parallel(&indici, |k| {
            let l = &lavori[k];
            let c = piani_di(l.t, l.z)?;
            let mut misure = (0, 0);
            for (pezzo, out) in &l.uscite {
                let img = disegna(&c, *pezzo);
                misure = (img.width(), img.height());
                match action {
                    "avif" => std::fs::write(out, crate::imgfmt::avif_bytes(&img, quality)?).map_err(|e| e.to_string())?,
                    "jxl" => std::fs::write(out, crate::imgfmt::jxl_bytes(&img)?).map_err(|e| e.to_string())?,
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
        for n in notes(1) {
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
}
