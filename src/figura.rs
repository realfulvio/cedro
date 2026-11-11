//! La figura per un articolo: le immagini scelte in una griglia di pannelli larga quanto una colonna
//! (8,5 cm) o una pagina (17,5 cm) della rivista, alla risoluzione chiesta (300 o 600 dpi), con le
//! lettere A, B, C in alto a sinistra e la barra di scala sui pannelli di microscopia. Esce un TIFF
//! con i dpi scritti dentro (quello che chiedono le riviste) e una legenda di testo con la lunghezza
//! di ogni barra, da copiare nella didascalia.

use std::path::Path;

use image::{imageops::FilterType, DynamicImage, RgbImage};

use crate::microscopia::{self, Piani, Pila};

/// Le scelte della figura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opzioni {
    /// La larghezza in millimetri: 85 una colonna, 175 la pagina intera.
    pub larghezza_mm: u32,
    pub dpi: u32,
    /// Le lettere A, B, C sui pannelli.
    pub lettere: bool,
    /// La barra di scala sui pannelli che dicono quanto è grande un pixel.
    pub barra: bool,
}

impl Default for Opzioni {
    fn default() -> Self {
        Opzioni { larghezza_mm: 175, dpi: 300, lettere: true, barra: true }
    }
}

/// Un pannello: l'immagine e, per la microscopia, i micron di un suo pixel.
pub struct Pannello {
    pub img: RgbImage,
    pub um_per_px: Option<f64>,
    pub nome: String,
}

/// La figura fatta.
pub struct Figura {
    pub img: RgbImage,
    /// Per ogni pannello: la lettera, il nome del file e la barra (se c'è).
    pub pannelli: Vec<(char, String, Option<String>)>,
    pub colonne: usize,
}

/// Il pannello di un file: la microscopia con i canali a colori e la proiezione dei piani Z come
/// nelle conversioni, il resto come lo apre Cedro (DICOM con la finestra giusta, SVG, RAW…).
pub fn pannello(path: &Path) -> Result<Pannello, String> {
    let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if microscopia::is_stack(path) {
        let pila = Pila::open(path)?;
        let canali = pila.canali_di(path, 0, Piani::Massimo)?;
        let img = microscopia::composita(&pila, &canali, !pila.naturale());
        return Ok(Pannello { img, um_per_px: pila.um_per_px, nome });
    }
    // Un vetrino o una foto enorme non serve intero: 6000 pixel bastano per 17,5 cm a 600 dpi (4134).
    let img = crate::imgfmt::open(path, true, Some(6000)).map_err(|e| format!("{nome}: {e}"))?;
    Ok(Pannello { img: senza_trasparenza(img), um_per_px: None, nome })
}

fn senza_trasparenza(img: DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    // Le parti trasparenti su bianco, come la pagina della rivista.
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u32::from(p[3]);
        for k in 0..3 {
            o[k] = ((u32::from(p[k]) * a + 255 * (255 - a)) / 255) as u8;
        }
    }
    out
}

/// Quante colonne per `n` pannelli: in fila fino a 3, poi griglie il più possibile quadrate.
pub fn colonne_per(n: usize) -> usize {
    match n {
        0 | 1 => 1,
        2 | 3 => n,
        4 => 2,
        5 | 6 | 9 => 3,
        _ => 4,
    }
}

fn px(mm: f32, dpi: u32) -> u32 {
    (mm / 25.4 * dpi as f32).round().max(1.0) as u32
}

/// Ridimensiona alla larghezza della casella: per ingrandire molto (un'immagine piccola su una pagina)
/// i pixel restano quadrati, senza sfumature che non c'erano.
fn alla_larghezza(img: &RgbImage, larghezza: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let altezza = ((h as f64 * larghezza as f64 / w.max(1) as f64).round() as u32).max(1);
    if (w, h) == (larghezza, altezza) {
        return img.clone();
    }
    let filtro = if larghezza >= w * 2 { FilterType::Nearest } else { FilterType::Lanczos3 };
    image::imageops::resize(img, larghezza, altezza, filtro)
}

const FIGTREE_BOLD: &[u8] = include_bytes!("../assets/fonts/Figtree-Bold.ttf");

/// La lettera del pannello, disegnata con un carattere vero (Figtree grassetto) e fusa sull'immagine.
fn lettera(img: &mut RgbImage, ch: char, x: u32, y: u32, punti: f32, dpi: u32) {
    use resvg::usvg;
    static DB: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();
    let db = DB
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(FIGTREE_BOLD.to_vec());
            std::sync::Arc::new(db)
        })
        .clone();
    let corpo = punti / 72.0 * dpi as f32;
    let lato = (corpo * 1.3).ceil() as u32;
    // Bianca sui fondi scuri (fluorescenza), nera su quelli chiari (istologia, grafici).
    let (w, h) = img.dimensions();
    let mut somma = 0u64;
    let mut n = 0u64;
    for yy in y..(y + lato).min(h) {
        for xx in x..(x + lato).min(w) {
            let p = img.get_pixel(xx, yy);
            somma += (u64::from(p[0]) * 299 + u64::from(p[1]) * 587 + u64::from(p[2]) * 114) / 1000;
            n += 1;
        }
    }
    let chiara = n == 0 || somma / n.max(1) < 128;
    let colore = if chiara { "#FFFFFF" } else { "#000000" };
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{lato}\" height=\"{lato}\"><text x=\"0\" y=\"{:.1}\" font-family=\"Figtree\" font-weight=\"700\" font-size=\"{corpo:.1}\" fill=\"{colore}\">{ch}</text></svg>",
        corpo * 0.95
    );
    let opt = usvg::Options { fontdb: db, ..Default::default() };
    let Ok(tree) = usvg::Tree::from_str(&svg, &opt) else { return };
    let Some(mut pm) = resvg::tiny_skia::Pixmap::new(lato, lato) else { return };
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    for (i, p) in pm.pixels().iter().enumerate() {
        let a = u32::from(p.alpha());
        if a == 0 {
            continue;
        }
        let (xx, yy) = (x + i as u32 % lato, y + i as u32 / lato);
        if xx >= w || yy >= h {
            continue;
        }
        let d = img.get_pixel_mut(xx, yy);
        // Alfa premoltiplicata: colore + fondo × (1 − alfa).
        let s = [p.red(), p.green(), p.blue()];
        for k in 0..3 {
            d[k] = (u32::from(s[k]) + u32::from(d[k]) * (255 - a) / 255).min(255) as u8;
        }
    }
}

/// Mette insieme i pannelli, nell'ordine dato.
pub fn componi(pannelli: Vec<Pannello>, opz: &Opzioni) -> Result<Figura, String> {
    if pannelli.is_empty() {
        return Err("nessuna immagine per la figura".into());
    }
    if pannelli.len() > 26 {
        return Err("una figura tiene al massimo 26 pannelli (A-Z)".into());
    }
    let larghezza = px(opz.larghezza_mm as f32, opz.dpi);
    let spazio = px(2.0, opz.dpi);
    let colonne = colonne_per(pannelli.len());
    let casella = (larghezza - spazio * (colonne as u32 - 1)) / colonne as u32;
    let margine_lettera = px(1.2, opz.dpi);

    let mut righe: Vec<Vec<(RgbImage, char, String, Option<String>)>> = Vec::new();
    for (i, p) in pannelli.into_iter().enumerate() {
        let ch = (b'A' + i as u8) as char;
        let mut img = alla_larghezza(&p.img, casella);
        let mut barra = None;
        if opz.barra {
            if let Some(um) = p.um_per_px {
                // Un pixel del pannello copre più (o meno) micron di uno del file.
                let um_pannello = um * p.img.width() as f64 / img.width() as f64;
                barra = microscopia::disegna_barra_sola(&mut img, um_pannello);
            }
        }
        if opz.lettere {
            lettera(&mut img, ch, margine_lettera, margine_lettera, 10.0, opz.dpi);
        }
        if i % colonne == 0 {
            righe.push(Vec::new());
        }
        if let Some(r) = righe.last_mut() {
            r.push((img, ch, p.nome, barra));
        }
    }
    let altezze: Vec<u32> = righe.iter().map(|r| r.iter().map(|(img, ..)| img.height()).max().unwrap_or(0)).collect();
    let altezza = altezze.iter().sum::<u32>() + spazio * (righe.len() as u32).saturating_sub(1);
    if larghezza as u64 * altezza as u64 > 400_000_000 {
        return Err("figura troppo grande: meno pannelli o immagini meno alte".into());
    }
    let mut tela = RgbImage::from_pixel(larghezza, altezza, image::Rgb([255, 255, 255]));
    let mut y = 0u32;
    let mut pannelli_fatti = Vec::new();
    for (r, riga) in righe.into_iter().enumerate() {
        for (c, (img, ch, nome, barra)) in riga.into_iter().enumerate() {
            let x = c as u32 * (casella + spazio);
            image::imageops::replace(&mut tela, &img, i64::from(x), i64::from(y));
            pannelli_fatti.push((ch, nome, barra));
        }
        y += altezze[r] + spazio;
    }
    Ok(Figura { img: tela, pannelli: pannelli_fatti, colonne })
}

/// Il TIFF della figura: RGB a 8 bit, compresso senza perdite (LZW), con i dpi scritti dentro.
pub fn tiff(img: &RgbImage, dpi: u32) -> Result<Vec<u8>, String> {
    use tiff::encoder::{colortype, Compression, Rational, TiffEncoder};
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out).map_err(|e| e.to_string())?.with_compression(Compression::Lzw);
    let mut im = enc.new_image::<colortype::RGB8>(img.width(), img.height()).map_err(|e| e.to_string())?;
    im.resolution(tiff::tags::ResolutionUnit::Inch, Rational { n: dpi, d: 1 });
    im.write_data(img.as_raw()).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// La legenda da incollare nella didascalia: misure della figura e, per pannello, file e barra.
pub fn legenda(f: &Figura, opz: &Opzioni) -> String {
    let cm = |mm: f32| format!("{:.1}", mm / 10.0).replace('.', ",");
    let altezza_mm = f.img.height() as f32 / opz.dpi as f32 * 25.4;
    let mut s = format!("Figura: {} × {} cm, {} dpi, {} pannelli\n\n", cm(opz.larghezza_mm as f32), cm(altezza_mm), opz.dpi, f.pannelli.len());
    for (ch, nome, barra) in &f.pannelli {
        s.push_str(&format!("({ch}) {nome}"));
        if let Some(b) = barra {
            s.push_str(&format!(" · barra di scala {b}"));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod prove {
    use super::*;

    fn tinta(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb(c))
    }

    #[test]
    fn i_pannelli_vanno_in_griglia_con_lettere_e_barra() {
        let opz = Opzioni { larghezza_mm: 85, dpi: 300, ..Opzioni::default() };
        let pannelli = vec![
            Pannello { img: tinta(400, 300, [0, 0, 0]), um_per_px: Some(0.5), nome: "a.ome.tif".into() },
            Pannello { img: tinta(200, 200, [250, 250, 250]), um_per_px: None, nome: "b.png".into() },
            Pannello { img: tinta(100, 50, [0, 0, 0]), um_per_px: None, nome: "c.jpg".into() },
            Pannello { img: tinta(100, 100, [0, 0, 0]), um_per_px: None, nome: "d.jpg".into() },
        ];
        let f = componi(pannelli, &opz).unwrap();
        // 85 mm a 300 dpi = 1004 px; 2×2 con 2 mm (24 px) in mezzo: caselle di 490.
        assert_eq!(f.colonne, 2);
        assert_eq!(f.img.width(), 1004);
        // Ogni riga è alta quanto il suo pannello più alto (B e D, quadrati).
        assert_eq!(f.img.height(), 490 + 24 + 490);
        // A: fondo nero, lettera bianca; B: fondo chiaro, lettera nera.
        let lettera_in = |x0: u32, y0: u32, bianca: bool| (x0..x0 + 40).any(|x| (y0..y0 + 45).any(|y| f.img.get_pixel(x, y)[0] == if bianca { 255 } else { 0 }));
        assert!(lettera_in(14, 14, true));
        assert!(lettera_in(490 + 24 + 14, 14, false));
        // La barra su A: 400 px × 0,5 µm = 200 µm nel pannello di 490 px.
        assert!(f.pannelli[0].2.is_some(), "{:?}", f.pannelli[0]);
        assert_eq!(f.pannelli[1].2, None);
        let leg = legenda(&f, &opz);
        assert!(leg.starts_with("Figura: 8,5 × ") && leg.contains("(A) a.ome.tif · barra di scala") && leg.contains("(D) d.jpg"), "{leg}");
        // Il TIFF ha i dpi.
        let t = tiff(&f.img, 300).unwrap();
        let mut d = tiff::decoder::Decoder::new(std::io::Cursor::new(t)).unwrap();
        assert_eq!(d.dimensions().unwrap(), (1004, f.img.height()));
        assert!(matches!(d.get_tag(tiff::tags::Tag::XResolution).unwrap(), tiff::decoder::ifd::Value::Rational(300, 1)));
    }

    #[test]
    fn colonne_sensate() {
        assert_eq!((1..=10).map(colonne_per).collect::<Vec<_>>(), [1, 2, 3, 2, 3, 3, 4, 4, 3, 4]);
    }
}
