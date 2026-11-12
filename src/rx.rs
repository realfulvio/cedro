//! Le radiografie per il proprietario: dai DICOM un PDF da dare a chi porta l'animale, una pagina per
//! immagine, con il logo e il nome della clinica in alto, il nome dell'animale, specie, razza, data,
//! la parte del corpo e la proiezione, e la nota che l'immagine non sostituisce il referto.
//! La pagina si disegna come immagine (A4 a 150 dpi) e va nel PDF come JPEG.

use std::path::{Path, PathBuf};

use image::{imageops::FilterType, DynamicImage, Rgb, RgbImage};
use resvg::usvg;

use crate::dicom::{pixel, tag, DicomFile};

/// A4 verticale a 150 dpi.
const PAGINA: (u32, u32) = (1240, 1754);
const MARGINE: u32 = 90;
const INCHIOSTRO: &str = "#1B2430";
const GRIGIO: &str = "#5B6672";

/// Quello che il proprietario legge di un'immagine.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Scheda {
    pub animale: String,
    pub proprietario: String,
    pub specie: String,
    pub razza: String,
    pub data: String,
    pub parte: String,
    pub proiezione: String,
}

fn parola(s: &str) -> String {
    let s = s.trim().to_lowercase();
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn specie(s: &str) -> String {
    let t = s.trim();
    let it = match t.to_ascii_lowercase().as_str() {
        "canine" | "dog" | "canis familiaris" | "cane" => "Cane",
        "feline" | "cat" | "felis catus" | "gatto" => "Gatto",
        "equine" | "horse" | "cavallo" => "Cavallo",
        "bovine" | "cow" | "bovino" => "Bovino",
        "ovine" | "sheep" => "Pecora",
        "caprine" | "goat" => "Capra",
        "porcine" | "pig" => "Suino",
        "lapine" | "rabbit" | "coniglio" => "Coniglio",
        "avian" | "bird" => "Uccello",
        "ferret" | "furetto" => "Furetto",
        "" => "",
        _ => return parola(t),
    };
    it.to_string()
}

fn parte_del_corpo(s: &str) -> String {
    let t = s.trim();
    let it = match t.to_ascii_uppercase().as_str() {
        "THORAX" | "CHEST" | "TORACE" => "Torace",
        "ABDOMEN" | "ADDOME" => "Addome",
        "SKULL" | "HEAD" | "CRANIO" => "Cranio",
        "PELVIS" | "HIP" | "BACINO" => "Bacino",
        "SPINE" | "CSPINE" | "TSPINE" | "LSPINE" => "Colonna vertebrale",
        "ELBOW" => "Gomito",
        "KNEE" | "STIFLE" => "Ginocchio",
        "SHOULDER" => "Spalla",
        "FOOT" | "PAW" => "Zampa",
        "EXTREMITY" | "LEG" => "Arto",
        "WHOLEBODY" => "Corpo intero",
        "" => "",
        _ => return parola(t),
    };
    it.to_string()
}

fn proiezione(s: &str) -> String {
    let it = match s.trim().to_ascii_uppercase().as_str() {
        "LL" | "LAT" | "LATERAL" => "laterolaterale",
        "RL" | "RLAT" | "RLL" => "laterale destra",
        "LLAT" | "LLL" => "laterale sinistra",
        "VD" => "ventro-dorsale",
        "DV" => "dorso-ventrale",
        "AP" => "antero-posteriore",
        "PA" => "postero-anteriore",
        "CRCD" | "CrCd" => "cranio-caudale",
        "CDCR" => "caudo-craniale",
        _ => return s.trim().to_lowercase(),
    };
    it.to_string()
}

fn data(d: &str) -> String {
    const MESI: [&str; 12] = [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ];
    let d = d.trim();
    if d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()) {
        let (y, m, g) = (
            &d[..4],
            d[4..6].parse::<usize>().unwrap_or(0),
            d[6..].parse::<u32>().unwrap_or(0),
        );
        if (1..=12).contains(&m) && g > 0 {
            return format!("{g} {} {y}", MESI[m - 1]);
        }
    }
    d.to_string()
}

impl Scheda {
    pub fn di(f: &DicomFile) -> Scheda {
        let nome = f.string(tag(0x0010, 0x0010)).unwrap_or_default();
        // In veterinaria il nome è spesso «Animale^Cognome del proprietario».
        let mut parti = nome.split('^').map(str::trim);
        let animale = parti.next().map(parola).unwrap_or_default();
        let cognome = parti.next().map(parola).unwrap_or_default();
        let responsabile = f
            .string(tag(0x0010, 0x2297))
            .map(|s| s.replace('^', " ").trim().to_string())
            .filter(|s| !s.is_empty());
        Scheda {
            animale,
            proprietario: responsabile.unwrap_or(cognome),
            specie: specie(&f.string(tag(0x0010, 0x2201)).unwrap_or_default()),
            razza: parola(&f.string(tag(0x0010, 0x2292)).unwrap_or_default()),
            data: data(
                &f.string(tag(0x0008, 0x0020))
                    .or_else(|| f.string(tag(0x0008, 0x0022)))
                    .unwrap_or_default(),
            ),
            parte: parte_del_corpo(&f.string(tag(0x0018, 0x0015)).unwrap_or_default()),
            proiezione: proiezione(&f.string(tag(0x0018, 0x5101)).unwrap_or_default()),
        }
    }
}

fn opzioni_svg() -> usvg::Options<'static> {
    static DB: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    let db = DB
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(include_bytes!("../assets/fonts/Figtree-Regular.ttf").to_vec());
            db.load_font_data(include_bytes!("../assets/fonts/Figtree-Bold.ttf").to_vec());
            std::sync::Arc::new(db)
        })
        .clone();
    usvg::Options {
        fontdb: db,
        ..Default::default()
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Scrive righe di testo sulla pagina: (testo, y della linea di base, corpo in pixel, grassetto, colore).
fn testo(pagina: &mut RgbImage, x: u32, righe: &[(String, u32, f32, bool, &str)]) {
    let (w, h) = pagina.dimensions();
    let mut svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">"#);
    for (t, y, size, bold, c) in righe {
        if t.is_empty() {
            continue;
        }
        svg.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-family="Figtree" font-weight="{}" font-size="{size}" fill="{c}">{}</text>"#,
            if *bold { 700 } else { 400 },
            xml(t)
        ));
    }
    svg.push_str("</svg>");
    let Ok(tree) = usvg::Tree::from_str(&svg, &opzioni_svg()) else {
        return;
    };
    let Some(mut pm) = resvg::tiny_skia::Pixmap::new(w, h) else {
        return;
    };
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pm.as_mut(),
    );
    for (i, p) in pm.pixels().iter().enumerate() {
        let a = u32::from(p.alpha());
        if a == 0 {
            continue;
        }
        let d = pagina.get_pixel_mut(i as u32 % w, i as u32 / w);
        let s = [p.red(), p.green(), p.blue()];
        for k in 0..3 {
            d[k] = (u32::from(s[k]) + u32::from(d[k]) * (255 - a) / 255).min(255) as u8;
        }
    }
}

fn adatta(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    let s = (w as f32 / img.width() as f32).min(h as f32 / img.height() as f32);
    let (nw, nh) = (
        ((img.width() as f32 * s) as u32).max(1),
        ((img.height() as f32 * s) as u32).max(1),
    );
    image::imageops::resize(img, nw, nh, FilterType::Lanczos3)
}

/// Una pagina: intestazione della clinica, scheda dell'animale, la radiografia e la nota in fondo.
pub fn pagina(
    lastra: &RgbImage,
    s: &Scheda,
    clinica: &str,
    logo: Option<&RgbImage>,
    n: usize,
    di: usize,
) -> RgbImage {
    let (pw, ph) = PAGINA;
    let mut p = RgbImage::from_pixel(pw, ph, Rgb([255, 255, 255]));
    let mut x_testo = MARGINE;
    let alto_testa = 130u32;
    if let Some(l) = logo {
        let l = adatta(l, 360, alto_testa);
        image::imageops::replace(
            &mut p,
            &l,
            i64::from(MARGINE),
            i64::from(MARGINE + (alto_testa - l.height()) / 2),
        );
        x_testo = MARGINE + l.width() + 36;
    }
    let mut righe = vec![(
        clinica.trim().to_string(),
        MARGINE + alto_testa / 2 + 14,
        38.0,
        true,
        INCHIOSTRO,
    )];
    if clinica.trim().is_empty() && logo.is_none() {
        righe.clear();
    }
    testo(&mut p, x_testo, &righe);
    let linea = MARGINE + alto_testa + 28;
    for x in MARGINE..pw - MARGINE {
        for y in linea..linea + 3 {
            p.put_pixel(x, y, Rgb([0xC2, 0x30, 0x6B]));
        }
    }
    // La scheda dell'animale.
    let titolo = if s.animale.is_empty() {
        "Radiografia".to_string()
    } else {
        s.animale.clone()
    };
    let mut sotto: Vec<String> = [s.specie.clone(), s.razza.clone()]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect();
    if !s.proprietario.is_empty() {
        sotto.push(format!("di {}", s.proprietario));
    }
    let esame: Vec<String> = [s.parte.clone(), s.proiezione.clone(), s.data.clone()]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect();
    let y0 = linea + 90;
    testo(
        &mut p,
        MARGINE,
        &[
            (titolo, y0, 58.0, true, INCHIOSTRO),
            (sotto.join(" · "), y0 + 52, 30.0, false, GRIGIO),
            (esame.join(" · "), y0 + 96, 30.0, false, GRIGIO),
        ],
    );
    // La radiografia, su fondo nero come al diafanoscopio.
    let top = y0 + 140;
    let fondo = ph - MARGINE - 70;
    let box_w = pw - 2 * MARGINE;
    let r = adatta(lastra, box_w - 40, fondo - top - 40);
    // Il riquadro nero segue la lastra: niente fasce vuote sopra e sotto.
    let box_h = r.height() + 40;
    for y in top..top + box_h {
        for x in MARGINE..MARGINE + box_w {
            p.put_pixel(x, y, Rgb([0, 0, 0]));
        }
    }
    image::imageops::replace(
        &mut p,
        &r,
        i64::from(MARGINE + (box_w - r.width()) / 2),
        i64::from(top + (box_h - r.height()) / 2),
    );
    let mut piede = vec![(
        "Immagine per il proprietario: non sostituisce il referto del veterinario.".to_string(),
        ph - MARGINE + 4,
        22.0,
        false,
        GRIGIO,
    )];
    if di > 1 {
        piede.push((format!("{n} di {di}"), ph - MARGINE + 4, 22.0, true, GRIGIO));
    }
    testo(&mut p, MARGINE, &piede[..1]);
    if di > 1 {
        testo(&mut p, pw - MARGINE - 90, &piede[1..]);
    }
    p
}

/// Le immagini di un DICOM, con la finestra scelta da chi ha fatto l'esame.
fn lastre(path: &Path) -> Result<(Scheda, Vec<RgbImage>), String> {
    let f = DicomFile::open(path)?;
    let info = pixel::Info::of(&f)?;
    let scheda = Scheda::di(&f);
    let mut v = Vec::new();
    for i in 0..(info.frames as usize).min(50) {
        let fr = pixel::frame(&f, &info, i)?;
        v.push(pixel::render(&info, &fr, pixel::Window::File).to_rgb8());
    }
    Ok((scheda, v))
}

/// Il logo: un'immagine in coda (non DICOM) diventa il logo e si ricorda per le volte dopo; se non
/// c'è, quello ricordato.
pub fn logo(paths: &[PathBuf], salvato: &Path) -> Option<RgbImage> {
    let nuovo = paths
        .iter()
        .find(|p| !e_dicom(p))
        .and_then(|p| crate::imgfmt::open(p, true, Some(1200)).ok());
    if let Some(l) = &nuovo {
        let _ = std::fs::create_dir_all(salvato.parent().unwrap_or(Path::new(".")));
        let _ = sfondo_bianco(l).save_with_format(salvato, image::ImageFormat::Png);
    }
    nuovo
        .map(|l| sfondo_bianco(&l))
        .or_else(|| image::open(salvato).ok().map(|i| i.to_rgb8()))
}

/// Vero se il file è un DICOM (dal contenuto o dall'estensione).
pub fn e_dicom(p: &Path) -> bool {
    crate::imgfmt::sniff_file(p) == Some("dcm")
        || matches!(crate::engine::extension(p).as_str(), "dcm" | "dicom")
}

fn sfondo_bianco(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u32::from(p[3]);
        *o = Rgb([0, 1, 2].map(|k| ((u32::from(p[k]) * a + 255 * (255 - a)) / 255) as u8));
    }
    out
}

/// Il nome del file dal nome dell'animale: «Fido» → «fido», lettere e cifre soltanto.
pub fn nome_file(animale: &str) -> Option<String> {
    let mut s = String::new();
    for c in animale.trim().chars() {
        if c.is_alphanumeric() {
            s.extend(c.to_lowercase());
        } else if !s.is_empty() && !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    (!s.is_empty()).then_some(s)
}

/// Il PDF per il proprietario con tutte le immagini dei DICOM dati (nell'ordine dato). Ritorna i byte,
/// quante pagine e il nome dell'animale (per il nome del file).
pub fn pdf(
    dicom: &[PathBuf],
    clinica: &str,
    logo: Option<&RgbImage>,
    avanzamento: &dyn Fn(f32),
) -> Result<(Vec<u8>, usize, String), String> {
    let mut tutte = Vec::new();
    for (i, p) in dicom.iter().enumerate() {
        let (s, v) = lastre(p).map_err(|e| {
            format!(
                "{}: {e}",
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        })?;
        for img in v {
            tutte.push((s.clone(), img));
        }
        avanzamento(i as f32 / dicom.len() as f32 * 0.5);
    }
    if tutte.is_empty() {
        return Err("nessuna radiografia nei file".into());
    }
    let n = tutte.len();
    let animale = tutte[0].0.animale.clone();
    let mut pagine = Vec::with_capacity(n);
    for (k, (s, img)) in tutte.iter().enumerate() {
        let pg = pagina(img, s, clinica, logo, k + 1, n);
        let mut b = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 90)
            .encode_image(&pg)
            .map_err(|e| e.to_string())?;
        pagine.push((b, pg.width(), pg.height()));
        avanzamento(0.5 + (k + 1) as f32 / n as f32 * 0.5);
    }
    Ok((crate::pdf::pdf_from_jpegs(&pagine), n, animale))
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn i_dati_si_leggono_come_li_dice_un_veterinario() {
        assert_eq!(specie("Canine"), "Cane");
        assert_eq!(specie("FELINE"), "Gatto");
        assert_eq!(parte_del_corpo("THORAX"), "Torace");
        assert_eq!(proiezione("VD"), "ventro-dorsale");
        assert_eq!(data("20261111"), "11 novembre 2026");
        assert_eq!(parola("LABRADOR RETRIEVER"), "Labrador retriever");
    }

    #[test]
    fn la_pagina_ha_logo_testo_e_lastra() {
        let lastra = RgbImage::from_fn(400, 300, |x, _| Rgb([(x % 256) as u8; 3]));
        let logo = RgbImage::from_pixel(200, 100, Rgb([0, 120, 200]));
        let s = Scheda {
            animale: "Fido".into(),
            proprietario: "Rossi".into(),
            specie: "Cane".into(),
            razza: "Labrador".into(),
            data: "11 novembre 2026".into(),
            parte: "Torace".into(),
            proiezione: "laterolaterale".into(),
        };
        let p = pagina(
            &lastra,
            &s,
            "Clinica Veterinaria Estense",
            Some(&logo),
            1,
            2,
        );
        assert_eq!(p.dimensions(), PAGINA);
        // Il logo in alto a sinistra, il testo scuro accanto, la lastra al centro su fondo nero.
        assert_eq!(p.get_pixel(MARGINE + 10, MARGINE + 60).0, [0, 120, 200]);
        let scuri = (MARGINE + 300..MARGINE + 900)
            .flat_map(|x| (MARGINE + 40..MARGINE + 100).map(move |y| (x, y)))
            .filter(|&(x, y)| p.get_pixel(x, y).0[0] < 100)
            .count();
        assert!(scuri > 500, "{scuri}");
        assert_eq!(p.get_pixel(MARGINE + 5, PAGINA.1 / 2).0, [0, 0, 0]);
    }
}

#[cfg(test)]
mod prove_vere {
    /// Con CEDRO_RX=a.dcm,b.dcm,logo.png: il PDF e la prima pagina in PNG accanto al primo file.
    #[test]
    #[ignore]
    fn file_veri() {
        let Ok(list) = std::env::var("CEDRO_RX") else {
            return;
        };
        let paths: Vec<std::path::PathBuf> =
            list.split(',').map(std::path::PathBuf::from).collect();
        let dicom: Vec<_> = paths
            .iter()
            .filter(|p| super::e_dicom(p))
            .cloned()
            .collect();
        let logo = super::logo(&paths, &dicom[0].with_extension("logo.png"));
        let (bytes, n, animale) = super::pdf(
            &dicom,
            "Clinica Veterinaria Estense",
            logo.as_ref(),
            &|_| {},
        )
        .unwrap();
        std::fs::write(dicom[0].with_extension("rx.pdf"), bytes).unwrap();
        let (s, v) = super::lastre(&dicom[0]).unwrap();
        super::pagina(
            &v[0],
            &s,
            "Clinica Veterinaria Estense",
            logo.as_ref(),
            1,
            n,
        )
        .save(dicom[0].with_extension("pagina.png"))
        .unwrap();
        eprintln!("{n} pagine, {animale}");
    }
}
