//! Immagini di microscopia in TIFF: OME-TIFF (quello dei microscopi e di Bio-Formats), TIFF di ImageJ
//! o Fiji e i TIFF a 16 bit di camere scientifiche. Un file ha canali (colori di fluorescenza),
//! piani Z (la profondità) e istanti nel tempo; per vederlo bene servono i colori dei canali, la
//! proiezione dei piani Z e il contrasto automatico, perché a 16 bit un'immagine di fluorescenza
//! «a 8 bit» è quasi nera. La barra di scala si disegna se il file dice quanto è grande un pixel.
//!
//! Tutto in memoria per un piano alla volta: la proiezione massima legge i piani uno per uno e tiene
//! solo il massimo; i canali vanno in parallelo, ognuno con il suo lettore.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image::{DynamicImage, ImageBuffer, Luma, Rgb, RgbImage};
use tiff::decoder::{ifd::Value, Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

/// Un canale: il nome (DAPI, GFP…) e il colore con cui si accende.
#[derive(Clone, Debug, PartialEq)]
pub struct Canale {
    pub nome: String,
    pub colore: (u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origine {
    /// Con OME-XML nella descrizione.
    Ome,
    /// Con la descrizione di ImageJ/Fiji.
    ImageJ,
    /// Un TIFF a 16 bit senza metadati.
    Semplice,
}

/// Com'è fatto il file: tutto quello che serve per sceglierne i piani, senza i pixel.
#[derive(Clone, Debug)]
pub struct Pila {
    pub origine: Origine,
    pub larghezza: u32,
    pub altezza: u32,
    pub canali: Vec<Canale>,
    pub z: usize,
    pub t: usize,
    /// Micron per pixel (X), se il file lo dice.
    pub um_per_px: Option<f64>,
    /// Micron tra un piano Z e il successivo.
    pub um_z: Option<f64>,
    pub bit: u8,
    /// Le pagine TIFF del file.
    pub pagine: usize,
    /// I campioni di ogni pagina (3 per un RGB).
    campioni: usize,
    planare: bool,
    /// Per ogni passo dell'ordine delle pagine: quante pagine si saltano passando a quello dopo.
    passo_c: usize,
    passo_z: usize,
    passo_t: usize,
    /// Per OME con `TiffData` esplicito: (t, z, c) → pagina.
    mappa: Option<Vec<((usize, usize, usize), usize)>>,
    pub obiettivo: Option<String>,
}

fn aperto(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    Decoder::new(BufReader::new(file)).map(|d| d.with_limits(Limits::unlimited())).map_err(|e| e.to_string())
}

fn razionale(v: Value) -> Option<f64> {
    match v {
        Value::Rational(n, d) if d != 0 => Some(f64::from(n) / f64::from(d)),
        Value::RationalBig(n, d) if d != 0 => Some(n as f64 / d as f64),
        Value::Float(f) => Some(f64::from(f)),
        Value::Double(f) => Some(f),
        _ => None,
    }
}

/// La descrizione (OME-XML o ImageJ) della prima pagina, se è di un tipo che riconosciamo, e le sue misure.
struct Testa {
    descrizione: String,
    larghezza: u32,
    altezza: u32,
    bit: u8,
    campioni: usize,
    formato_intero: bool,
}

fn testa(d: &mut Decoder<BufReader<File>>) -> Option<Testa> {
    let (w, h) = d.dimensions().ok()?;
    let descrizione = d.get_tag_ascii_string(Tag::ImageDescription).unwrap_or_default();
    let (bit, campioni) = match d.colortype().ok()? {
        tiff::ColorType::Gray(b) => (b, 1),
        tiff::ColorType::RGB(b) => (b, 3),
        tiff::ColorType::RGBA(b) => (b, 4),
        _ => return None,
    };
    let formato_intero = d.get_tag_u32(Tag::SampleFormat).map_or(true, |f| f == 1);
    Some(Testa { descrizione, larghezza: w, altezza: h, bit, campioni, formato_intero })
}

/// Vero se il file è di microscopia (e quindi lo legge questo modulo invece del lettore comune): ha
/// la descrizione OME o ImageJ, oppure è un TIFF a 16 bit in toni di grigio. Un vetrino Aperio no.
pub fn is_stack(path: &Path) -> bool {
    let head = crate::imgfmt::head_of(path);
    let tiff = head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") || head.starts_with(b"II+\0") || head.starts_with(b"MM\0+");
    if !tiff {
        return false;
    }
    let Ok(mut d) = aperto(path) else { return false };
    let Some(t) = testa(&mut d) else { return false };
    if t.descrizione.starts_with("Aperio") {
        return false;
    }
    t.descrizione.contains("<OME") || t.descrizione.starts_with("ImageJ=") || (t.campioni == 1 && t.bit == 16)
}

impl Pila {
    pub fn open(path: &Path) -> Result<Pila, String> {
        let mut d = aperto(path)?;
        let t = testa(&mut d).ok_or("TIFF di un tipo che Cedro non legge ancora (serve grigio o RGB)")?;
        let mut pagine = 1;
        while d.more_images() {
            d.next_image().map_err(|e| e.to_string())?;
            pagine += 1;
            if pagine > 200_000 {
                return Err("troppe pagine".into());
            }
        }
        d.seek_to_image(0).map_err(|e| e.to_string())?;
        let planare = d.get_tag_u32(Tag::PlanarConfiguration).unwrap_or(1) == 2;
        let mut pila = Pila {
            origine: Origine::Semplice,
            larghezza: t.larghezza,
            altezza: t.altezza,
            canali: Vec::new(),
            z: pagine,
            t: 1,
            um_per_px: None,
            um_z: None,
            bit: if t.formato_intero { t.bit } else { 32 },
            pagine,
            campioni: t.campioni,
            planare,
            passo_c: 1,
            passo_z: 1,
            passo_t: pagine,
            mappa: None,
            obiettivo: None,
        };
        if t.descrizione.contains("<OME") {
            pila.origine = Origine::Ome;
            pila.leggi_ome(&t.descrizione);
        } else if t.descrizione.starts_with("ImageJ=") {
            pila.origine = Origine::ImageJ;
            pila.leggi_imagej(&t.descrizione, &mut d);
        }
        if pila.canali.is_empty() {
            pila.canali = canali_standard(t.campioni.max(1), &[]);
            pila.z = (pagine).max(1);
            pila.t = 1;
            (pila.passo_c, pila.passo_z, pila.passo_t) = (1, 1, pagine.max(1));
        }
        // Le dimensioni dichiarate non devono superare le pagine che ci sono davvero.
        let ifd_c = pila.canali.len().div_ceil(pila.campioni).max(1);
        if pila.mappa.is_none() && ifd_c * pila.z * pila.t > pagine {
            // Un OME diviso in più file, o tagliato: si tiene quello che c'è, come piani Z.
            pila.canali = canali_standard(pila.campioni.max(1), &[]);
            pila.z = pagine;
            pila.t = 1;
            (pila.passo_c, pila.passo_z, pila.passo_t) = (1, 1, pagine);
        }
        Ok(pila)
    }

    fn leggi_ome(&mut self, xml: &str) {
        let Ok(doc) = roxmltree::Document::parse_with_options(xml.trim(), roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX, ..Default::default() }) else { return };
        // Il primo Image con i suoi Pixels (i file con più serie: la prima).
        let Some(px) = doc.descendants().find(|n| n.has_tag_name("Pixels")) else { return };
        let num = |a: &str| px.attribute(a).and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0);
        let size_c = num("SizeC").unwrap_or(1);
        self.z = num("SizeZ").unwrap_or(1);
        self.t = num("SizeT").unwrap_or(1);
        let ordine = px.attribute("DimensionOrder").unwrap_or("XYZCT").to_ascii_uppercase();
        let ifd_c = size_c.div_ceil(self.campioni.max(1)).max(1);
        let dim = |c: char| match c {
            'C' => ifd_c,
            'Z' => self.z,
            _ => self.t,
        };
        let mut passo = 1usize;
        for c in ordine.chars().skip(2).take(3) {
            match c {
                'C' => self.passo_c = passo,
                'Z' => self.passo_z = passo,
                'T' => self.passo_t = passo,
                _ => {}
            }
            passo *= dim(c);
        }
        let micron = |v: Option<f64>, unita: Option<&str>| -> Option<f64> {
            let v = v.filter(|v| *v > 0.0 && v.is_finite())?;
            Some(match unita.unwrap_or("µm") {
                "nm" => v / 1000.0,
                "mm" => v * 1000.0,
                "cm" => v * 10_000.0,
                "m" => v * 1_000_000.0,
                "pm" => v / 1_000_000.0,
                "Å" => v / 10_000.0,
                _ => v,
            })
        };
        let f = |a: &str| px.attribute(a).and_then(|v| v.parse::<f64>().ok());
        self.um_per_px = micron(f("PhysicalSizeX"), px.attribute("PhysicalSizeXUnit"));
        self.um_z = micron(f("PhysicalSizeZ"), px.attribute("PhysicalSizeZUnit"));
        let nomi: Vec<(String, Option<(u8, u8, u8)>)> = px
            .children()
            .filter(|n| n.has_tag_name("Channel"))
            .map(|c| {
                let nome = c.attribute("Name").or_else(|| c.attribute("Fluor")).unwrap_or("").to_string();
                // OME: intero con segno, RGBA.
                let colore = c.attribute("Color").and_then(|v| v.parse::<i64>().ok()).map(|v| {
                    let v = v as i32 as u32;
                    ((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8)
                });
                (nome, colore)
            })
            .collect();
        let mut canali = canali_standard(size_c, &nomi);
        // Un Channel con SamplesPerPixel=3 vale come tre canali R, G, B.
        if self.campioni == 3 && size_c == 3 && nomi.len() <= 1 {
            canali = canali_standard(3, &[]);
        }
        self.canali = canali;
        self.obiettivo = doc
            .descendants()
            .find(|n| n.has_tag_name("Objective"))
            .map(|o| [o.attribute("Model"), o.attribute("NominalMagnification")].iter().flatten().copied().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty());
        // TiffData esplicito: (FirstT, FirstZ, FirstC, IFD, PlaneCount).
        let dati: Vec<_> = px.children().filter(|n| n.has_tag_name("TiffData")).collect();
        let esplicito = dati.iter().any(|n| n.attribute("FirstZ").or_else(|| n.attribute("FirstC")).or_else(|| n.attribute("FirstT")).is_some() || n.attribute("IFD").is_some_and(|v| v != "0"));
        let esterni = dati.iter().any(|n| n.children().any(|c| c.has_tag_name("UUID") && c.attribute("FileName").is_some()));
        if esplicito && !esterni {
            let mut mappa = Vec::new();
            for n in dati {
                let g = |a: &str| n.attribute(a).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let (t0, z0, c0, ifd) = (g("FirstT"), g("FirstZ"), g("FirstC"), g("IFD"));
                let quanti = n.attribute("PlaneCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(if n.attribute("IFD").is_some() { 1 } else { usize::MAX });
                // Un blocco di piani consecutivi in ordine DimensionOrder, a partire da (t0, z0, c0).
                let (mut t, mut z, mut c) = (t0, z0, c0);
                for k in 0..quanti.min(self.z * self.t * ifd_c) {
                    mappa.push(((t, z, c), ifd + k));
                    // Avanza secondo l'ordine: il primo dopo XY è il più veloce.
                    let mut riporto = true;
                    for ch in ordine.chars().skip(2).take(3) {
                        if !riporto {
                            break;
                        }
                        let (v, max) = match ch {
                            'C' => (&mut c, ifd_c),
                            'Z' => (&mut z, self.z),
                            _ => (&mut t, self.t),
                        };
                        *v += 1;
                        if *v >= max {
                            *v = 0;
                        } else {
                            riporto = false;
                        }
                    }
                }
            }
            if !mappa.is_empty() {
                self.mappa = Some(mappa);
            }
        }
    }

    fn leggi_imagej(&mut self, desc: &str, d: &mut Decoder<BufReader<File>>) {
        let campo = |k: &str| desc.lines().find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('=')).map(str::trim));
        let intero = |k: &str| campo(k).and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0);
        let mut c = intero("channels").unwrap_or(1);
        if self.campioni > 1 && c == 1 {
            c = self.campioni;
        }
        self.z = intero("slices").unwrap_or(1);
        self.t = intero("frames").unwrap_or(1);
        // Senza channels/slices/frames le «images» sono piani Z.
        if intero("channels").is_none() && intero("slices").is_none() && intero("frames").is_none() {
            self.z = intero("images").unwrap_or(self.pagine);
        }
        // ImageJ: canali, poi Z, poi tempo.
        let ifd_c = c.div_ceil(self.campioni.max(1)).max(1);
        (self.passo_c, self.passo_z, self.passo_t) = (1, ifd_c, ifd_c * self.z);
        let unita = campo("unit").map(|u| u.replace("\\u00B5", "µ").replace("micron", "µm").replace("um", "µm").replace("µµ", "µ"));
        // La risoluzione è pixel per unità: micron per pixel = 1 / XResolution.
        if let Some(x) = d.find_tag(Tag::XResolution).ok().flatten().and_then(razionale).filter(|v| *v > 0.0) {
            let um = match unita.as_deref() {
                Some("µm") | Some("µm\u{0}") => Some(1.0 / x),
                Some("nm") => Some(1.0 / x / 1000.0),
                Some("mm") => Some(1000.0 / x),
                Some("cm") => Some(10_000.0 / x),
                Some("inch") | Some("in") => Some(25_400.0 / x),
                _ => None,
            };
            self.um_per_px = um.filter(|v| v.is_finite() && *v > 0.0);
        }
        if let (Some(s), Some(u)) = (campo("spacing").and_then(|v| v.parse::<f64>().ok()), unita.as_deref()) {
            self.um_z = match u {
                "µm" => Some(s),
                "nm" => Some(s / 1000.0),
                "mm" => Some(s * 1000.0),
                _ => None,
            };
        }
        // I nomi dei canali non ci sono: colori standard, con la LUT se è il caso.
        self.canali = canali_standard(c, &[]);
    }

    /// Che pagina e che campione portano il canale `c` del piano `z` all'istante `t`.
    fn pagina(&self, c: usize, z: usize, t: usize) -> Option<(usize, usize)> {
        let (cp, campione) = (c / self.campioni.max(1), c % self.campioni.max(1));
        let pagina = match &self.mappa {
            Some(m) => m.iter().find(|((mt, mz, mc), _)| (*mt, *mz, *mc) == (t, z, cp)).map(|(_, p)| *p)?,
            None => cp * self.passo_c + z * self.passo_z + t * self.passo_t,
        };
        (pagina < self.pagine).then_some((pagina, campione))
    }

    pub fn campioni(&self) -> usize {
        self.campioni
    }

    /// Foto già a colori (un RGB a 8 bit): niente contrasto, si vede com'è.
    pub fn naturale(&self) -> bool {
        self.campioni >= 3 && self.bit == 8 && self.canali.len() == 3 && self.origine != Origine::Ome
            || self.campioni >= 3 && self.bit == 8 && self.canali.iter().map(|c| c.colore).eq([(255, 0, 0), (0, 255, 0), (0, 0, 255)])
    }

    /// Le righe che descrivono il file, in italiano e in inglese.
    pub fn riassunto(&self) -> [String; 2] {
        let line = |en: bool| {
            let mut p: Vec<String> = Vec::new();
            p.push(
                match self.origine {
                    Origine::Ome => "OME-TIFF",
                    Origine::ImageJ => "TIFF di ImageJ",
                    Origine::Semplice => "TIFF a 16 bit",
                }
                .into(),
            );
            let n = self.canali.len();
            if n > 1 || self.origine == Origine::Ome {
                p.push(if en { format!("{n} channel{}", if n == 1 { "" } else { "s" }) } else { format!("{n} canal{}", if n == 1 { "e" } else { "i" }) });
            }
            if self.z > 1 {
                p.push(if en { format!("{} Z planes", self.z) } else { format!("{} piani Z", self.z) });
            }
            if self.t > 1 {
                p.push(if en { format!("{} time points", self.t) } else { format!("{} istanti", self.t) });
            }
            p.push(if en { format!("{}-bit", self.bit) } else { format!("{} bit", self.bit) });
            p.push(format!("{}×{}", self.larghezza, self.altezza));
            if let Some(um) = self.um_per_px {
                let s = format!("{um:.4}").trim_end_matches('0').trim_end_matches('.').to_string();
                p.push(if en { format!("{s} µm/pixel") } else { format!("{} µm/pixel", s.replace('.', ",")) });
            }
            p.join(" · ")
        };
        [line(false), line(true)]
    }
}

// ------------------------------------------------------------------ Colori

/// I colori di default quando il file non li dice: dal nome (DAPI blu, GFP verde, mCherry rosso,
/// 647 magenta…) o, per un nome che non dice nulla, nell'ordine di ImageJ.
pub fn canali_standard(n: usize, nomi: &[(String, Option<(u8, u8, u8)>)]) -> Vec<Canale> {
    const ORDINE: [(u8, u8, u8); 7] = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255), (0, 255, 255), (255, 0, 255), (255, 255, 0)];
    (0..n)
        .map(|i| {
            let (nome, colore) = nomi.get(i).cloned().unwrap_or_default();
            let colore = colore
                .filter(|c| *c != (255, 255, 255) || n == 1)
                .or_else(|| colore_dal_nome(&nome))
                .unwrap_or(if n == 1 { (255, 255, 255) } else { ORDINE[i % ORDINE.len()] });
            Canale { nome: if nome.trim().is_empty() { format!("C{}", i + 1) } else { nome.trim().to_string() }, colore }
        })
        .collect()
}

fn colore_dal_nome(nome: &str) -> Option<(u8, u8, u8)> {
    let n = nome.to_ascii_lowercase();
    let ha = |parole: &[&str]| parole.iter().any(|p| n.contains(p));
    if ha(&["dapi", "hoechst", "bisbenzimide", "draq", "blue", "blu", "405", "pacific"]) {
        return Some((64, 128, 255));
    }
    if ha(&["gfp", "fitc", "alexa 488", "alexa488", "af488", "488", "green", "verde", "yfp"]) {
        return Some((0, 255, 0));
    }
    if ha(&["tritc", "cy3", "rfp", "mcherry", "texas", "561", "568", "594", "555", "546", "red", "rosso", "dsred", "rhod"]) {
        return Some((255, 40, 40));
    }
    if ha(&["cy5", "647", "633", "640", "650", "far red", "farred", "alexa 647"]) {
        return Some((255, 0, 255));
    }
    if ha(&["cy7", "750", "780", "nir"]) {
        return Some((255, 255, 255));
    }
    if ha(&["brightfield", "bf", "dic", "phase", "trans", "campo chiaro", "contrasto"]) {
        return Some((255, 255, 255));
    }
    // Una lunghezza d'onda sola, per esempio «Ex 520».
    let numero: Option<u32> = n.split(|c: char| !c.is_ascii_digit()).filter(|s| s.len() == 3).find_map(|s| s.parse().ok());
    numero.filter(|v| (380..=800).contains(v)).map(|nm| lunghezza_d_onda(nm as f32))
}

/// Il colore di una lunghezza d'onda tra 380 e 800 nm (oltre il rosso: rosso scuro).
fn lunghezza_d_onda(nm: f32) -> (u8, u8, u8) {
    let (r, g, b) = match nm {
        x if x < 440.0 => ((440.0 - x) / 60.0, 0.0, 1.0),
        x if x < 490.0 => (0.0, (x - 440.0) / 50.0, 1.0),
        x if x < 510.0 => (0.0, 1.0, (510.0 - x) / 20.0),
        x if x < 580.0 => ((x - 510.0) / 70.0, 1.0, 0.0),
        x if x < 645.0 => (1.0, (645.0 - x) / 65.0, 0.0),
        _ => (1.0, 0.0, 0.0),
    };
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (f(r), f(g), f(b))
}

// ------------------------------------------------------------------ Pixel

/// Un canale proiettato: i valori e il fondo scala (255 per i dati a 8 bit, 65535 per gli altri).
pub struct Piano {
    pub dati: Vec<u16>,
    pub scala: u16,
}

/// Cosa fare dei piani Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piani {
    /// Il valore più alto lungo Z, pixel per pixel.
    Massimo,
    /// Il piano di mezzo.
    Centrale,
}

impl Pila {
    /// Un piano (canale `c`, piano `z`, istante `t`) letto dal file.
    fn leggi_piano(&self, d: &mut Decoder<BufReader<File>>, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        let (pagina, campione) = self.pagina(c, z, t).ok_or("piano non trovato nel file")?;
        d.seek_to_image(pagina).map_err(|e| e.to_string())?;
        let (w, h) = d.dimensions().map_err(|e| e.to_string())?;
        if (w, h) != (self.larghezza, self.altezza) {
            return Err(format!("la pagina {} ha misure diverse ({w}×{h})", pagina + 1));
        }
        let n = w as usize * h as usize;
        let letto = d.read_image().map_err(|e| format!("pagina {} non letta: {e}", pagina + 1))?;
        let campioni = self.campioni.max(1);
        // Il campione desiderato come sequenza lineare.
        let prendi = |len: usize, at: &dyn Fn(usize) -> u16| -> Vec<u16> { (0..len).map(at).collect() };
        let indice = |i: usize| if self.planare { campione * n + i } else { i * campioni + campione };
        let (dati, scala): (Vec<u16>, u16) = match letto {
            DecodingResult::U8(v) => (prendi(n, &|i| u16::from(v[indice(i)])), 255),
            DecodingResult::U16(v) => (prendi(n, &|i| v[indice(i)]), 65535),
            DecodingResult::I16(v) => (prendi(n, &|i| (i32::from(v[indice(i)]) + 32768) as u16), 65535),
            DecodingResult::I8(v) => (prendi(n, &|i| (i16::from(v[indice(i)]) + 128) as u16), 255),
            DecodingResult::U32(v) => (prendi(n, &|i| (v[indice(i)] >> 16) as u16), 65535),
            DecodingResult::I32(v) => (prendi(n, &|i| ((i64::from(v[indice(i)]) + 2_147_483_648) >> 16) as u16), 65535),
            DecodingResult::F32(v) => {
                let alto = v.iter().copied().filter(|x| x.is_finite()).fold(0.0f32, f32::max);
                let k = if alto <= 1.0 { 65535.0 } else { 1.0 };
                (prendi(n, &|i| (v[indice(i)].clamp(0.0, 65535.0 / k) * k) as u16), 65535)
            }
            DecodingResult::F64(v) => {
                let alto = v.iter().copied().filter(|x| x.is_finite()).fold(0.0f64, f64::max);
                let k = if alto <= 1.0 { 65535.0 } else { 1.0 };
                (prendi(n, &|i| (v[indice(i)].clamp(0.0, 65535.0 / k) * k) as u16), 65535)
            }
            _ => return Err("tipo di pixel non letto".into()),
        };
        Ok(Piano { dati, scala })
    }

    /// Il canale `c` all'istante `t`, con i piani Z uniti come scelto.
    pub fn canale(&self, path: &Path, c: usize, t: usize, piani: Piani) -> Result<Piano, String> {
        let mut d = aperto(path)?;
        match piani {
            Piani::Centrale => self.leggi_piano(&mut d, c, self.z / 2, t),
            Piani::Massimo => {
                let mut totale = self.leggi_piano(&mut d, c, 0, t)?;
                for z in 1..self.z {
                    let p = self.leggi_piano(&mut d, c, z, t)?;
                    for (a, b) in totale.dati.iter_mut().zip(&p.dati) {
                        if *b > *a {
                            *a = *b;
                        }
                    }
                }
                Ok(totale)
            }
        }
    }

    /// Un solo piano Z (per «un file per piano»).
    pub fn piano_z(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        self.leggi_piano(&mut aperto(path)?, c, z, t)
    }

    /// Tutti i canali di un istante, ognuno su un thread.
    pub fn canali_di(&self, path: &Path, t: usize, piani: Piani) -> Result<Vec<Piano>, String> {
        self.in_parallelo((0..self.canali.len()).collect(), |c| self.canale(path, c, t, piani))
    }

    /// Un piano Z di tutti i canali.
    pub fn canali_di_z(&self, path: &Path, z: usize, t: usize) -> Result<Vec<Piano>, String> {
        self.in_parallelo((0..self.canali.len()).collect(), |c| self.piano_z(path, c, z, t))
    }

    fn in_parallelo<T: Send>(&self, voci: Vec<usize>, lavoro: impl Fn(usize) -> Result<T, String> + Sync) -> Result<Vec<T>, String> {
        if voci.len() <= 1 {
            return voci.into_iter().map(lavoro).collect();
        }
        std::thread::scope(|s| {
            let ths: Vec<_> = voci.iter().map(|c| s.spawn(|| lavoro(*c))).collect();
            ths.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("lettura interrotta".into()))).collect()
        })
    }
}

// ------------------------------------------------------------------ Immagine

/// I limiti del contrasto automatico: il valore sotto cui sta lo 0,5% dei pixel e quello sopra cui
/// sta lo 0,2% (come «Auto» di ImageJ, più rispettoso dei fondi scuri).
pub fn limiti(p: &Piano) -> (u16, u16) {
    let mut istogramma = vec![0u32; p.scala as usize + 1];
    for v in &p.dati {
        istogramma[(*v).min(p.scala) as usize] += 1;
    }
    let tot = p.dati.len() as u64;
    let (mut basso, mut alto) = (0u16, p.scala);
    let mut somma = 0u64;
    for (v, n) in istogramma.iter().enumerate() {
        somma += u64::from(*n);
        if somma * 1000 >= tot * 5 {
            basso = v as u16;
            break;
        }
    }
    let mut somma = 0u64;
    for (v, n) in istogramma.iter().enumerate().rev() {
        somma += u64::from(*n);
        if somma * 1000 > tot * 2 {
            alto = v as u16;
            break;
        }
    }
    if alto <= basso {
        alto = basso.saturating_add(1).min(p.scala.max(1));
    }
    (basso, alto)
}

/// I canali sovrapposti, ognuno acceso del suo colore (somma, come il «composito» di ImageJ).
pub fn composita(pila: &Pila, canali: &[Piano], contrasto: bool) -> RgbImage {
    let (w, h) = (pila.larghezza, pila.altezza);
    let n = w as usize * h as usize;
    let mut acc = vec![[0u16; 3]; n];
    for (c, piano) in canali.iter().enumerate() {
        let colore = pila.canali.get(c).map_or((255, 255, 255), |c| c.colore);
        let (basso, alto) = if contrasto { limiti(piano) } else { (0, piano.scala) };
        let scala = 65536.0 / f64::from(alto.saturating_sub(basso).max(1));
        // Tavola di conversione: valore → 0..=255 (una sola volta per canale).
        let tavola: Vec<u8> = (0..=piano.scala as usize)
            .map(|v| {
                let x = ((v as f64 - f64::from(basso)) * scala / 256.0).clamp(0.0, 255.0);
                x as u8
            })
            .collect();
        for (px, v) in acc.iter_mut().zip(&piano.dati) {
            let x = u16::from(tavola[(*v).min(piano.scala) as usize]);
            px[0] += x * u16::from(colore.0) / 255;
            px[1] += x * u16::from(colore.1) / 255;
            px[2] += x * u16::from(colore.2) / 255;
        }
    }
    let mut out = RgbImage::new(w, h);
    for (dst, a) in out.pixels_mut().zip(&acc) {
        *dst = Rgb([a[0].min(255) as u8, a[1].min(255) as u8, a[2].min(255) as u8]);
    }
    out
}

/// Un canale a 16 bit con i valori originali (i dati a 8 bit si portano a 16 moltiplicando per 257).
pub fn sedici_bit(pila: &Pila, piano: &Piano) -> DynamicImage {
    let k = if piano.scala == 255 { 257 } else { 1 };
    let buf: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_raw(pila.larghezza, pila.altezza, piano.dati.iter().map(|v| v.saturating_mul(k)).collect()).unwrap_or_else(|| ImageBuffer::new(1, 1));
    DynamicImage::ImageLuma16(buf)
}

/// Un canale in grigio con il suo contrasto.
pub fn grigio_di(pila: &Pila, piano: &Piano, contrasto: bool) -> DynamicImage {
    let (basso, alto) = if contrasto { limiti(piano) } else { (0, piano.scala) };
    let scala = 65536.0 / f64::from(alto.saturating_sub(basso).max(1));
    let tavola: Vec<u8> = (0..=piano.scala as usize).map(|v| ((v as f64 - f64::from(basso)) * scala / 256.0).clamp(0.0, 255.0) as u8).collect();
    let buf = ImageBuffer::from_raw(pila.larghezza, pila.altezza, piano.dati.iter().map(|v| tavola[(*v).min(piano.scala) as usize]).collect()).unwrap_or_else(|| image::GrayImage::new(1, 1));
    DynamicImage::ImageLuma8(buf)
}

// ------------------------------------------------------------------ Barra di scala

/// Le lunghezze «tonde» (1, 2, 5 × 10^k micron) che una barra può avere.
fn lunghezze_tonde() -> impl Iterator<Item = f64> {
    (-3..=5).flat_map(|k| [1.0, 2.0, 5.0].map(|m| m * 10f64.powi(k)))
}

/// La lunghezza della barra (micron) e la scritta: la più lunga tonda sotto un quinto della larghezza.
pub fn misura_barra(larghezza: u32, um_per_px: f64) -> Option<(f64, String)> {
    let massimo = f64::from(larghezza) * um_per_px * 0.22;
    let um = lunghezze_tonde().take_while(|v| *v <= massimo).last()?;
    let px = um / um_per_px;
    if px < 8.0 {
        return None;
    }
    let testo = if um < 1.0 {
        format!("{} nm", (um * 1000.0).round() as u32)
    } else if um >= 1000.0 {
        format!("{} mm", (um / 1000.0).round() as u32)
    } else {
        format!("{} µm", um.round() as u32)
    };
    Some((um, testo))
}

/// Le lettere della barra: 5×7 punti. Servono cifre, µ, m, n e lo spazio.
fn lettera(c: char) -> [u8; 7] {
    match c {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'µ' => [0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b11101, 0b10000],
        'm' => [0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101],
        'n' => [0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
        _ => [0; 7],
    }
}

/// Disegna la barra in basso a destra: bianca con il contorno nero, con la scritta sopra.
/// Ritorna la scritta (per il messaggio) se la scala c'è ed è abbastanza grande da disegnare.
pub fn disegna_barra(img: &mut RgbImage, um_per_px: f64) -> Option<String> {
    let (w, h) = img.dimensions();
    let (um, testo) = misura_barra(w, um_per_px)?;
    let px = (um / um_per_px).round() as i64;
    let lato = w.min(h).max(1) as f64;
    let spessore = ((lato / 110.0).round() as i64).max(3);
    let k = ((lato / 260.0).round() as i64).max(2);
    let margine = (lato / 28.0).round().max(8.0) as i64;
    let (x1, y1) = (i64::from(w) - margine, i64::from(h) - margine);
    let (x0, y0) = (x1 - px, y1 - spessore);
    let mut macchia = |x: i64, y: i64, colore: [u8; 3]| {
        if x >= 0 && y >= 0 && x < i64::from(w) && y < i64::from(h) {
            img.put_pixel(x as u32, y as u32, Rgb(colore));
        }
    };
    let contorno = (k / 2).max(1);
    for y in y0 - contorno..y1 + contorno {
        for x in x0 - contorno..x1 + contorno {
            macchia(x, y, [0, 0, 0]);
        }
    }
    for y in y0..y1 {
        for x in x0..x1 {
            macchia(x, y, [255, 255, 255]);
        }
    }
    // La scritta, centrata sopra la barra: contorno nero, poi bianco.
    let larghezza_testo = testo.chars().count() as i64 * 6 * k - k;
    let tx = x0 + (px - larghezza_testo) / 2;
    let ty = y0 - contorno - 4 * k - 7 * k;
    for (passo, colore) in [(true, [0u8, 0, 0]), (false, [255, 255, 255])] {
        for (i, ch) in testo.chars().enumerate() {
            let righe = lettera(ch);
            for (r, riga) in righe.iter().enumerate() {
                for col in 0..5 {
                    if riga & (1 << (4 - col)) == 0 {
                        continue;
                    }
                    let (bx, by) = (tx + (i as i64 * 6 + col) * k, ty + r as i64 * k);
                    let (da, a) = if passo { (-contorno, k + contorno) } else { (0, k) };
                    for y in by + da..by + a {
                        for x in bx + da..bx + a {
                            macchia(x, y, colore);
                        }
                    }
                }
            }
        }
    }
    Some(testo)
}

/// Il file come un'immagine sola, per anteprime e per i formati che non conoscono la microscopia:
/// canali sovrapposti sul piano centrale, con il contrasto e senza barra di scala.
pub fn apri(path: &Path) -> Result<DynamicImage, String> {
    let pila = Pila::open(path)?;
    let canali = pila.canali_di(path, 0, Piani::Centrale)?;
    Ok(DynamicImage::ImageRgb8(composita(&pila, &canali, !pila.naturale())))
}

#[cfg(test)]
pub(crate) mod prove {
    use super::*;
    use tiff::encoder::{colortype, TiffEncoder};

    /// Un OME-TIFF 16 bit di 2 canali × 3 piani Z, 12×8 pixel, 0,5 µm per pixel: il canale 0 (DAPI) ha un
    /// punto che si sposta tra i piani, il canale 1 (GFP) una riga. Ritorna il percorso.
    pub fn ome(dir: &Path) -> std::path::PathBuf {
        let (w, h) = (12u32, 8u32);
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><OME xmlns="http://www.openmicroscopy.org/Schemas/OME/2016-06"><Image ID="Image:0"><Pixels ID="Pixels:0" DimensionOrder="XYZCT" Type="uint16" SizeX="{w}" SizeY="{h}" SizeZ="3" SizeC="2" SizeT="1" PhysicalSizeX="0.5" PhysicalSizeXUnit="uum" PhysicalSizeY="0.5" PhysicalSizeZ="2"><Channel ID="Channel:0:0" Name="DAPI" SamplesPerPixel="1"/><Channel ID="Channel:0:1" Name="GFP" SamplesPerPixel="1"/><TiffData/></Pixels></Image></OME>"#
        );
        let path = dir.join("ome.ome.tif");
        let mut f = File::create(&path).unwrap();
        let mut enc = TiffEncoder::new(&mut f).unwrap();
        // Ordine XYZCT: Z il più veloce, poi C.
        for c in 0..2usize {
            for z in 0..3usize {
                let mut px = vec![100u16; (w * h) as usize];
                if c == 0 {
                    px[(2 * w) as usize + 2 + z] = 4000 + (z as u16) * 100;
                } else {
                    for x in 0..w as usize {
                        px[(5 * w) as usize + x] = 2000;
                    }
                }
                let mut img = enc.new_image::<colortype::Gray16>(w, h).unwrap();
                if c == 0 && z == 0 {
                    img.encoder().write_tag(Tag::ImageDescription, xml.as_str()).unwrap();
                }
                img.write_data(&px).unwrap();
            }
        }
        drop(enc);
        drop(f);
        // Gli scrittori veri mettono «µm» in UTF-8 (due byte) dove la nostra prova scrive «uum» (tre).
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes.windows(5).position(|w| w == b"=\"uum").unwrap();
        bytes.splice(at + 2..at + 5, "µm".bytes());
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn un_ome_tiff_si_legge_con_canali_piani_e_scala() {
        let dir = std::env::temp_dir().join(format!("cedro-micro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = ome(&dir);
        assert!(is_stack(&p));
        let pila = Pila::open(&p).unwrap();
        assert_eq!((pila.larghezza, pila.altezza, pila.z, pila.t), (12, 8, 3, 1));
        assert_eq!(pila.canali.iter().map(|c| c.nome.as_str()).collect::<Vec<_>>(), ["DAPI", "GFP"]);
        assert_eq!(pila.canali[0].colore, (64, 128, 255));
        assert_eq!(pila.canali[1].colore, (0, 255, 0));
        assert_eq!(pila.um_per_px, Some(0.5));
        assert_eq!(pila.um_z, Some(2.0));
        assert_eq!(pila.bit, 16);
        // La proiezione massima del DAPI ha tutti e tre i punti, con il valore del piano più alto.
        let max = pila.canale(&p, 0, 0, Piani::Massimo).unwrap();
        let accesi: Vec<u16> = max.dati.iter().copied().filter(|v| *v > 100).collect();
        assert_eq!(accesi, [4000, 4100, 4200]);
        // Il piano centrale ne ha uno solo.
        let centro = pila.canale(&p, 0, 0, Piani::Centrale).unwrap();
        assert_eq!(centro.dati.iter().filter(|v| **v > 100).count(), 1);
        // Il GFP sta nell'altra metà del file, non si confonde con il DAPI.
        let gfp = pila.canale(&p, 1, 0, Piani::Massimo).unwrap();
        assert_eq!(gfp.dati.iter().filter(|v| **v == 2000).count(), 12);
        // L'immagine: azzurro-blu per il DAPI, verde per il GFP.
        let img = composita(&pila, &[max, gfp], true);
        assert_eq!(img.get_pixel(5, 5)[1], 255, "la riga del GFP è verde acceso");
        assert!(img.get_pixel(2, 2)[2] > 200, "il punto del DAPI è blu");
        assert_eq!(img.get_pixel(11, 0).0, [0, 0, 0], "il fondo è nero");
        let riga = pila.riassunto();
        assert_eq!(riga[0], "OME-TIFF · 2 canali · 3 piani Z · 16 bit · 12×8 · 0,5 µm/pixel");
        assert!(riga[1].contains("2 channels") && riga[1].contains("3 Z planes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_barra_ha_una_lunghezza_tonda_e_si_disegna() {
        let (um, testo) = misura_barra(2048, 0.325).unwrap();
        assert_eq!((um, testo.as_str()), (100.0, "100 µm"));
        assert_eq!(misura_barra(2048, 0.0002).map(|(_, t)| t).as_deref(), Some("50 nm"));
        assert_eq!(misura_barra(4000, 5.0).map(|(_, t)| t).as_deref(), Some("2 mm"));
        assert!(misura_barra(10, 100.0).is_some_and(|(um, _)| um > 0.0) || misura_barra(10, 100.0).is_none());
        let mut img = RgbImage::from_pixel(1024, 768, Rgb([10, 10, 10]));
        assert_eq!(disegna_barra(&mut img, 0.325).as_deref(), Some("50 µm"));
        let bianchi = img.pixels().filter(|p| p.0 == [255, 255, 255]).count();
        assert!(bianchi > 154 * 6, "la barra (154 px) e le lettere: {bianchi}");
        if let Ok(dir) = std::env::var("CEDRO_PROVA_FOTO") {
            img.save(std::path::Path::new(&dir).join("barra.png")).unwrap();
        }
        assert!(img.get_pixel(990, 738).0 == [255, 255, 255]);
    }

    #[test]
    fn imagej_ha_canali_scala_e_ordine_proprio() {
        let dir = std::env::temp_dir().join(format!("cedro-micro-ij-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("fiji.tif");
        let desc = "ImageJ=1.53t\nimages=4\nchannels=2\nslices=2\nhyperstack=true\nunit=micron\nspacing=1.5\nmode=composite\n";
        let mut f = File::create(&p).unwrap();
        let mut enc = TiffEncoder::new(&mut f).unwrap();
        for i in 0..4u16 {
            let mut img = enc.new_image::<colortype::Gray16>(6, 4).unwrap();
            if i == 0 {
                img.encoder().write_tag(Tag::ImageDescription, desc).unwrap();
                img.encoder().write_tag(Tag::XResolution, tiff::encoder::Rational { n: 4, d: 1 }).unwrap();
            }
            img.write_data(&[(i + 1) * 1000; 24]).unwrap();
        }
        drop(enc);
        let pila = Pila::open(&p).unwrap();
        assert_eq!(pila.origine, Origine::ImageJ);
        assert_eq!((pila.canali.len(), pila.z, pila.t), (2, 2, 1));
        assert_eq!(pila.um_per_px, Some(0.25));
        assert_eq!(pila.um_z, Some(1.5));
        // ImageJ: canale più veloce. Pagine 0,1 = z0 (c0, c1); 2,3 = z1.
        assert_eq!(pila.canale(&p, 0, 0, Piani::Centrale).unwrap().dati[0], 3000, "il piano centrale di 2 è z=1");
        assert_eq!(pila.canale(&p, 1, 0, Piani::Massimo).unwrap().dati[0], 4000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_tiff_a_colori_a_8_bit_non_e_microscopia() {
        let dir = std::env::temp_dir().join(format!("cedro-micro-rgb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("foto.tif");
        let mut f = File::create(&p).unwrap();
        TiffEncoder::new(&mut f).unwrap().write_image::<colortype::RGB8>(4, 4, &[7u8; 48]).unwrap();
        assert!(!is_stack(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
