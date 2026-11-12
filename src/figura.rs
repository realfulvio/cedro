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
    /// Per le foto della fotocamera del microscopio (che non dicono quanto è grande un pixel):
    /// l'obiettivo usato (0 = nessuna barra su quelle foto) e i micron di un pixel con il 10x, come li
    /// ha scritti la persona.
    pub obiettivo: u32,
    pub calibrazione_10x: String,
}

impl Opzioni {
    /// I micron di un pixel delle foto senza misure, se obiettivo e calibrazione ci sono.
    fn foto_um(&self) -> Option<f64> {
        pm_per_px(&self.calibrazione_10x, self.obiettivo).map(|pm| pm as f64 / 1e6)
    }
}

impl Default for Opzioni {
    fn default() -> Self {
        Opzioni { larghezza_mm: 175, dpi: 300, lettere: true, barra: true, obiettivo: 0, calibrazione_10x: String::new() }
    }
}

/// Un pannello: l'immagine e, per la microscopia e i vetrini, i micron di un suo pixel.
pub struct Pannello {
    pub img: RgbImage,
    pub um_per_px: Option<f64>,
    pub nome: String,
    /// Come è stato fatto il pannello (proiezione, contrasto con i suoi limiti, serie e istante): va
    /// nella legenda, perché chi legge l'articolo deve saperlo.
    pub note: Vec<String>,
    /// Una foto della fotocamera senza misure: solo a queste vale la calibrazione scritta a mano.
    pub foto: bool,
}

/// La figura fatta.
pub struct Figura {
    pub img: RgbImage,
    /// Per ogni pannello: la lettera, il nome del file, la barra (se c'è) e come è stato fatto.
    pub pannelli: Vec<(char, String, Option<String>, String)>,
    pub colonne: usize,
}

/// Il pannello di un file: la microscopia con i canali a colori e la proiezione dei piani Z come
/// nelle conversioni, il resto come lo apre Cedro (DICOM con la finestra giusta, SVG, RAW…).
/// I picometri di un pixel con l'obiettivo `ingrandimento`, dalla calibrazione fatta con il 10x
/// (scritta come la scrive una persona: «0,65» o «0.65» µm). `None` se non è un numero sensato.
pub fn pm_per_px(calibrazione_10x: &str, ingrandimento: u32) -> Option<u32> {
    let um: f64 = calibrazione_10x.trim().trim_end_matches("µm").trim().replace(',', ".").parse().ok()?;
    if !(0.001..=100.0).contains(&um) || ingrandimento == 0 {
        return None;
    }
    Some((um * 10.0 / ingrandimento as f64 * 1e6).round() as u32)
}

pub fn pannello(path: &Path) -> Result<Pannello, String> {
    let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if microscopia::is_stack(path) {
        let pila = Pila::open(path)?;
        let canali = pila.canali_di(path, 0, Piani::Massimo)?;
        let mut note = Vec::new();
        if pila.serie.len() > 1 {
            note.push(format!("immagine 1 di {} («{}»)", pila.serie.len(), pila.serie.first().cloned().unwrap_or_default()));
        }
        if pila.t > 1 {
            note.push(format!("istante 1 di {}", pila.t));
        }
        if pila.z > 1 {
            note.push(format!("proiezione massima di {} piani Z", pila.z));
        }
        // Il contrasto cambia i valori: nella legenda vanno i limiti di ogni canale.
        let contrasto = !pila.naturale();
        let limiti = microscopia::limiti_di(&pila, &canali, contrasto);
        if contrasto {
            note.push(format!("contrasto automatico lineare per canale (0,5% dei pixel sotto e 0,2% sopra saturati): {}", microscopia::descrivi_limiti(&pila, &limiti)));
        }
        if let Some(a) = &pila.avviso {
            note.push(a.clone());
        }
        let img = microscopia::composita_con(&pila, &canali, &limiti);
        return Ok(Pannello { img, um_per_px: pila.um_per_px, nome, note, foto: false });
    }
    // Un vetrino: il livello giusto per 6000 pixel e la misura del pixel dalla descrizione Aperio.
    if crate::svs::is_slide(path) {
        let s = crate::svs::Slide::open(path).map_err(|e| format!("{nome}: {e}"))?;
        let livello = s.for_side(s.pick(crate::svs::Pick::Medium), 6000);
        let mut img = s.read_level(path, livello).map_err(|e| format!("{nome}: {e}"))?;
        if img.width().max(img.height()) > 6000 {
            img = DynamicImage::ImageRgb8(img).thumbnail(6000, 6000).to_rgb8();
        }
        let um = s.mpp.map(|m| f64::from(m) * f64::from(s.full().width) / f64::from(img.width().max(1)));
        return Ok(Pannello { img, um_per_px: um, nome, note: Vec::new(), foto: false });
    }
    // Una foto enorme non serve intera: 6000 pixel bastano per 17,5 cm a 600 dpi (4134).
    let img = crate::imgfmt::open(path, true, Some(6000)).map_err(|e| format!("{nome}: {e}"))?;
    // Solo le foto (JPEG, PNG, TIFF…) possono venire dalla fotocamera del microscopio; un DICOM, un SVG o
    // un PDF no.
    let foto = matches!(crate::engine::extension(path).as_str(), "jpg" | "jpeg" | "png" | "tif" | "tiff" | "bmp" | "webp" | "heic" | "heif");
    Ok(Pannello { img: senza_trasparenza(img), um_per_px: None, nome, note: Vec::new(), foto })
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

    let mut righe: Vec<Vec<(RgbImage, char, String, Option<String>, String)>> = Vec::new();
    for (i, p) in pannelli.into_iter().enumerate() {
        let ch = (b'A' + i as u8) as char;
        let mut img = alla_larghezza(&p.img, casella);
        let mut barra = None;
        let mut note = p.note;
        if opz.barra {
            // La misura del file prima di tutto; la calibrazione scritta a mano solo per le foto della
            // fotocamera (mai per un vetrino, un grafico vettoriale o una microscopia che ha la sua).
            let manuale = if p.foto && p.um_per_px.is_none() { opz.foto_um() } else { None };
            if let Some(um) = p.um_per_px.or(manuale) {
                // Un pixel del pannello copre più (o meno) micron di uno del file.
                let um_pannello = um * p.img.width() as f64 / img.width() as f64;
                barra = microscopia::disegna_barra_sola(&mut img, um_pannello);
                if barra.is_some() && manuale.is_some() {
                    note.push(format!("barra dalla calibrazione scritta a mano (obiettivo {}x)", opz.obiettivo));
                }
            }
        }
        if opz.lettere {
            lettera(&mut img, ch, margine_lettera, margine_lettera, 10.0, opz.dpi);
        }
        if i % colonne == 0 {
            righe.push(Vec::new());
        }
        if let Some(r) = righe.last_mut() {
            r.push((img, ch, p.nome, barra, note.join(" · ")));
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
        for (c, (img, ch, nome, barra, note)) in riga.into_iter().enumerate() {
            let x = c as u32 * (casella + spazio);
            image::imageops::replace(&mut tela, &img, i64::from(x), i64::from(y));
            pannelli_fatti.push((ch, nome, barra, note));
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
    for (ch, nome, barra, note) in &f.pannelli {
        s.push_str(&format!("({ch}) {nome}"));
        if let Some(b) = barra {
            s.push_str(&format!(" · barra di scala {b}"));
        }
        if !note.is_empty() {
            s.push_str(&format!(" · {note}"));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn la_calibrazione_del_10x_vale_per_gli_altri_obiettivi() {
        assert_eq!(pm_per_px("0,65", 10), Some(650_000));
        assert_eq!(pm_per_px("0.65 µm", 40), Some(162_500));
        assert_eq!(pm_per_px(" 1,3 ", 4), Some(3_250_000));
        assert_eq!(pm_per_px("", 10), None);
        assert_eq!(pm_per_px("abc", 10), None);
        assert_eq!(pm_per_px("0,65", 0), None);
        // Una foto senza misure prende la barra dall'obiettivo.
        let opz = Opzioni { larghezza_mm: 85, obiettivo: 10, calibrazione_10x: "0,65".into(), ..Opzioni::default() };
        let f = componi(vec![Pannello { img: tinta(1004, 500, [0, 0, 0]), um_per_px: None, nome: "foto.jpg".into(), note: Vec::new(), foto: true }], &opz).unwrap();
        assert!(f.pannelli[0].2.is_some(), "{:?}", f.pannelli[0]);
    }

    fn tinta(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb(c))
    }

    #[test]
    fn i_pannelli_vanno_in_griglia_con_lettere_e_barra() {
        let opz = Opzioni { larghezza_mm: 85, dpi: 300, ..Opzioni::default() };
        let pannelli = vec![
            Pannello { img: tinta(400, 300, [0, 0, 0]), um_per_px: Some(0.5), nome: "a.ome.tif".into(), note: Vec::new(), foto: false },
            Pannello { img: tinta(200, 200, [250, 250, 250]), um_per_px: None, nome: "b.png".into(), note: Vec::new(), foto: true },
            Pannello { img: tinta(100, 50, [0, 0, 0]), um_per_px: None, nome: "c.jpg".into(), note: Vec::new(), foto: true },
            Pannello { img: tinta(100, 100, [0, 0, 0]), um_per_px: None, nome: "d.jpg".into(), note: Vec::new(), foto: true },
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

#[cfg(test)]
mod prove_revisione {
    use super::*;

    #[test]
    fn il_vetrino_ha_la_sua_barra_e_non_quella_della_fotocamera() {
        // CMU-1 a 0,499 µm per pixel, con una calibrazione della fotocamera (40x) che non c'entra.
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/svs/cmu1-pezzo.svs");
        let pan = pannello(&p).unwrap();
        assert!(!pan.foto);
        assert!(pan.um_per_px.is_some_and(|u| (u - 0.499).abs() < 1e-3), "{:?}", pan.um_per_px);
        let opz = Opzioni { larghezza_mm: 85, obiettivo: 40, calibrazione_10x: "0,65".into(), ..Opzioni::default() };
        let f = componi(vec![pan], &opz).unwrap();
        // 480 px × 0,499 µm in 1004 px: 0,239 µm per pixel del pannello, la barra tonda è 50 µm (con la
        // fotocamera sarebbe stata 10 µm).
        assert_eq!(f.pannelli[0].2.as_deref(), Some("50 µm"));
        assert!(!f.pannelli[0].3.contains("calibrazione"));
    }

    #[test]
    fn la_legenda_dice_proiezione_contrasto_e_istante() {
        let dir = std::env::temp_dir().join(format!("cedro-figura-rev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = crate::microscopia::prove::ome(&dir);
        let pan = pannello(&p).unwrap();
        let foto = Pannello { img: RgbImage::from_pixel(300, 200, image::Rgb([200, 200, 200])), um_per_px: None, nome: "foto.jpg".into(), note: Vec::new(), foto: true };
        let opz = Opzioni { larghezza_mm: 85, obiettivo: 10, calibrazione_10x: "0,65".into(), ..Opzioni::default() };
        let f = componi(vec![pan, foto], &opz).unwrap();
        let leg = legenda(&f, &opz);
        assert!(leg.contains("proiezione massima di 3 piani Z") && leg.contains("contrasto automatico lineare per canale") && leg.contains("DAPI ") && leg.contains("GFP "), "{leg}");
        assert!(leg.contains("(B) foto.jpg · barra di scala") && leg.contains("calibrazione scritta a mano (obiettivo 10x)"), "{leg}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
