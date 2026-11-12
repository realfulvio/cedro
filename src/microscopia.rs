//! Immagini di microscopia: OME-TIFF (quello dei microscopi e di Bio-Formats), TIFF di ImageJ o Fiji,
//! i TIFF a 16 bit di camere scientifiche, gli ND2 di Nikon e i LIF di Leica (lettori in `nd2` e `lif`). Un file ha canali (colori di fluorescenza),
//! piani Z (la profondità) e istanti nel tempo; per vederlo bene servono i colori dei canali, la
//! proiezione dei piani Z e il contrasto automatico, perché a 16 bit un'immagine di fluorescenza
//! «a 8 bit» è quasi nera. La barra di scala si disegna se il file dice quanto è grande un pixel.
//!
//! Tutto in memoria per un piano alla volta: la proiezione massima legge i piani uno per uno e tiene
//! solo il massimo; i canali vanno in parallelo, ognuno con il suo lettore.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

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
    /// Un ND2 di Nikon (NIS-Elements).
    Nikon,
    /// Un LIF di Leica (LAS X): un progetto con più immagini.
    Leica,
}

/// Da dove vengono i pixel.
#[derive(Clone)]
enum Sorgente {
    Tiff,
    Nd2(Arc<crate::nd2::Nd2>),
    Lif(Arc<crate::lif::Immagine>),
}

impl std::fmt::Debug for Sorgente {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Sorgente::Tiff => "Tiff",
            Sorgente::Nd2(_) => "Nd2",
            Sorgente::Lif(_) => "Lif",
        })
    }
}

/// Un asse messo in fila negli «istanti»: quanti sono e come si chiamano (in italiano e in inglese).
pub type AsseT = (&'static str, &'static str, usize);

/// Il nome di un canale e il suo colore, se il file lo dice.
pub type NomeColore = (String, Option<(u8, u8, u8)>);

/// Una voce di `TiffData`: (t, z, c) → pagina.
type Voce = ((usize, usize, usize), usize);

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
    mappa: Option<Vec<Voce>>,
    pub obiettivo: Option<String>,
    sorgente: Sorgente,
    /// Cosa c'è negli istanti quando non sono solo il tempo (posizioni, tessere, lunghezze d'onda).
    pub assi_t: Vec<AsseT>,
    /// Le immagini del file (i LIF ne hanno più d'una) e quale è questa.
    pub serie: Vec<String>,
    pub numero_serie: usize,
    /// Quello che non torna nel file e va detto a chi converte (un OME diviso in più file, una serie
    /// di un LIF saltata perché rovinata…).
    pub avviso: Option<String>,
    /// I TIFF di ImageJ oltre i 4 GB: una pagina sola e i piani uno dopo l'altro, non compressi.
    /// L'indirizzo del primo piano e se i byte sono little-endian.
    contigui: Option<(u64, bool)>,
}

fn aperto(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    Decoder::new(BufReader::new(file)).map(|d| d.with_limits(Limits::unlimited())).map_err(|e| e.to_string())
}

fn razionale(v: Value) -> Option<f64> {
    match v {
        Value::Rational(n, d) if d != 0 => Some(f64::from(n) / f64::from(d)),
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
    if crate::nd2::riconosci(&head) || crate::lif::riconosci(&head) {
        return true;
    }
    // Un ND2 dei primi NIS-Elements (JPEG 2000): lo si prende per dire che non si legge.
    if crate::engine::extension(path) == "nd2" && head.get(4..8) == Some(b"jP  ") {
        return true;
    }
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

/// Il messaggio per le immagini di un LIF lasciate fuori perché rovinate.
fn avviso_lif(saltate: &[(String, String)]) -> Option<String> {
    match saltate {
        [] => None,
        [(n, e)] => Some(format!("1 immagine del progetto non letta: «{n}» ({e})")),
        _ => Some(format!("{} immagini del progetto non lette: {}", saltate.len(), saltate.iter().map(|(n, _)| format!("«{n}»")).collect::<Vec<_>>().join(", "))),
    }
}

impl Pila {
    pub fn open(path: &Path) -> Result<Pila, String> {
        Pila::open_serie(path, 0)
    }

    /// Tutte le immagini del file: una sola, tranne nei LIF (letti una volta sola).
    pub fn open_tutte(path: &Path) -> Result<Vec<Pila>, String> {
        if crate::lif::riconosci(&crate::imgfmt::head_of(path)) {
            let lif = crate::lif::Lif::open(path)?;
            let nomi: Vec<String> = lif.immagini.iter().map(|i| i.nome.clone()).collect();
            let avviso = avviso_lif(&lif.saltate);
            return Ok(lif
                .immagini
                .into_iter()
                .enumerate()
                .map(|(k, i)| {
                    let mut p = Pila::da_lif(i, nomi.clone(), k);
                    p.avviso = avviso.clone();
                    p
                })
                .collect());
        }
        Ok(vec![Pila::open(path)?])
    }

    /// L'immagine numero `serie` del file (conta solo per i LIF).
    pub fn open_serie(path: &Path, serie: usize) -> Result<Pila, String> {
        let head = crate::imgfmt::head_of(path);
        if crate::nd2::riconosci(&head) {
            return Pila::da_nd2(crate::nd2::Nd2::open(path)?);
        }
        if crate::lif::riconosci(&head) {
            let lif = crate::lif::Lif::open(path)?;
            let nomi: Vec<String> = lif.immagini.iter().map(|i| i.nome.clone()).collect();
            let avviso = avviso_lif(&lif.saltate);
            let i = lif.immagini.into_iter().nth(serie).ok_or("immagine non trovata nel file LIF")?;
            let mut p = Pila::da_lif(i, nomi, serie);
            p.avviso = avviso;
            return Ok(p);
        }
        if crate::engine::extension(path) == "nd2" {
            return Err(crate::nd2::VECCHIO.into());
        }
        Pila::da_tiff(path)
    }

    fn vuota(origine: Origine, larghezza: u32, altezza: u32, sorgente: Sorgente) -> Pila {
        Pila {
            origine,
            larghezza,
            altezza,
            canali: Vec::new(),
            z: 1,
            t: 1,
            um_per_px: None,
            um_z: None,
            bit: 16,
            pagine: 0,
            campioni: 1,
            planare: false,
            passo_c: 1,
            passo_z: 1,
            passo_t: 1,
            mappa: None,
            obiettivo: None,
            sorgente,
            assi_t: Vec::new(),
            serie: Vec::new(),
            numero_serie: 0,
            avviso: None,
            contigui: None,
        }
    }

    fn da_nd2(n: crate::nd2::Nd2) -> Result<Pila, String> {
        use crate::nd2::Asse;
        let mut p = Pila::vuota(Origine::Nikon, n.larghezza, n.altezza, Sorgente::Tiff);
        p.canali = n.canali.clone();
        p.z = n.z();
        p.t = n.t();
        p.um_per_px = n.um_per_px;
        p.um_z = n.um_z;
        p.bit = n.bit;
        p.pagine = n.fotogrammi();
        p.campioni = n.campioni;
        p.obiettivo = n.obiettivo.clone();
        if n.interrotta {
            p.avviso = Some("acquisizione interrotta: l'ultimo giro è incompleto e i piani che mancano sono neri".into());
        }
        let fuori_z: Vec<(Asse, usize)> = n.cicli.iter().filter(|(a, k)| *a != Asse::Z && *k > 1).copied().collect();
        if fuori_z.iter().any(|(a, _)| *a != Asse::Tempo) {
            p.assi_t = fuori_z
                .iter()
                .map(|(a, k)| match a {
                    Asse::Posizione => ("posizioni", "positions", *k),
                    Asse::Tempo => ("istanti", "time points", *k),
                    _ => ("cicli", "loops", *k),
                })
                .collect();
        }
        p.sorgente = Sorgente::Nd2(Arc::new(n));
        Ok(p)
    }

    fn da_lif(i: crate::lif::Immagine, serie: Vec<String>, numero: usize) -> Pila {
        let mut p = Pila::vuota(Origine::Leica, i.larghezza, i.altezza, Sorgente::Tiff);
        p.canali = canali_standard(i.canali.len(), &i.canali);
        if i.rgb {
            for (c, n) in p.canali.iter_mut().zip(["R", "G", "B"]) {
                c.nome = n.to_string();
            }
            p.campioni = 3;
        }
        p.z = i.z;
        p.t = i.t();
        p.um_per_px = i.um_per_px;
        p.um_z = i.um_z;
        p.bit = i.bit;
        p.obiettivo = i.obiettivo.clone();
        if i.altri.iter().any(|(id, k)| *id != 4 && *k > 1) {
            p.assi_t = i
                .altri
                .iter()
                .map(|(id, k)| match id {
                    4 => ("istanti", "time points", *k),
                    5 => ("lunghezze d'onda di emissione", "emission wavelengths", *k),
                    9 => ("lunghezze d'onda di eccitazione", "excitation wavelengths", *k),
                    10 => ("tessere", "tiles", *k),
                    6 => ("rotazioni", "rotations", *k),
                    _ => ("cicli", "loops", *k),
                })
                .collect();
        }
        p.serie = serie;
        p.numero_serie = numero;
        p.sorgente = Sorgente::Lif(Arc::new(i));
        p
    }

    fn da_tiff(path: &Path) -> Result<Pila, String> {
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
            sorgente: Sorgente::Tiff,
            assi_t: Vec::new(),
            serie: Vec::new(),
            numero_serie: 0,
            avviso: None,
            contigui: None,
        };
        if t.descrizione.contains("<OME") {
            pila.origine = Origine::Ome;
            let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            pila.leggi_ome(&t.descrizione, &nome);
        } else if t.descrizione.starts_with("ImageJ=") {
            pila.origine = Origine::ImageJ;
            pila.leggi_imagej(&t.descrizione, &mut d);
            if pagine == 1 {
                pila.leggi_contigui(path, &t, &mut d);
            }
        }
        if pila.canali.is_empty() {
            pila.canali = canali_standard(t.campioni.max(1), &[]);
            pila.z = (pagine).max(1);
            pila.t = 1;
            (pila.passo_c, pila.passo_z, pila.passo_t) = (1, 1, pagine.max(1));
        }
        // Le dimensioni dichiarate non devono superare le pagine che ci sono davvero.
        let ifd_c = pila.canali.len().div_ceil(pila.campioni.max(1)).max(1);
        let dichiarate = ifd_c.checked_mul(pila.z).and_then(|v| v.checked_mul(pila.t));
        if pila.mappa.is_none() && dichiarate.is_none_or(|v| v > pila.pagine) {
            // Un OME diviso in più file, o tagliato: si tiene quello che c'è, come piani Z, e lo si dice.
            pila.avviso = pila.avviso.take().or(Some(format!(
                "il file dichiara {} canali × {} piani Z × {} istanti ma ha {} pagine: letto come {} piani di un canale",
                pila.canali.len(),
                pila.z,
                pila.t,
                pila.pagine,
                pila.pagine
            )));
            pila.canali = canali_standard(pila.campioni.max(1), &[]);
            pila.z = pila.pagine;
            pila.t = 1;
            (pila.passo_c, pila.passo_z, pila.passo_t) = (1, 1, pila.pagine);
        }
        Ok(pila)
    }

    fn leggi_ome(&mut self, xml: &str, nome_file: &str) {
        let Ok(doc) = roxmltree::Document::parse_with_options(xml.trim(), roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX, ..Default::default() }) else { return };
        // Il primo Image con i suoi Pixels (i file con più serie: la prima).
        let Some(px) = doc.descendants().find(|n| n.has_tag_name("Pixels")) else { return };
        let num = |a: &str| px.attribute(a).and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 0);
        // Un file rovinato può dichiarare miliardi di canali: oltre 4096 non ha senso (e riempirebbe la memoria).
        let size_c = num("SizeC").unwrap_or(1).min(4096);
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
            passo = passo.saturating_mul(dim(c));
        }
        // Le unità di lunghezza di OME; «pixel» e «reference frame» non sono misure fisiche: niente barra.
        let micron = |v: Option<f64>, unita: Option<&str>| -> Option<f64> {
            let v = v.filter(|v| *v > 0.0 && v.is_finite())?;
            Some(match unita.unwrap_or("µm").trim() {
                "µm" | "μm" | "um" => v,
                "nm" => v / 1000.0,
                "mm" => v * 1000.0,
                "cm" => v * 10_000.0,
                "dm" => v * 100_000.0,
                "m" => v * 1_000_000.0,
                "pm" => v / 1_000_000.0,
                "Å" | "\u{212B}" => v / 10_000.0,
                "in" => v * 25_400.0,
                _ => return None,
            })
        };
        let f = |a: &str| px.attribute(a).and_then(|v| v.parse::<f64>().ok());
        self.um_per_px = micron(f("PhysicalSizeX"), px.attribute("PhysicalSizeXUnit"));
        self.um_z = micron(f("PhysicalSizeZ"), px.attribute("PhysicalSizeZUnit"));
        let nomi: Vec<NomeColore> = px
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
        // Una voce è di questo file se non nomina un file, o nomina questo (con l'UUID dell'OME o con il
        // nome). Bio-Formats scrive sempre `<UUID FileName=…>`, anche per un file solo.
        let uuid_file = doc.root_element().attribute("UUID").map(str::trim);
        let locale = |n: &roxmltree::Node| match n.children().find(|c| c.has_tag_name("UUID")) {
            None => true,
            Some(u) => {
                let uuid = u.text().map(str::trim);
                let nome = u.attribute("FileName").map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f));
                (uuid_file.is_some() && uuid == uuid_file) || nome == Some(nome_file) || (nome.is_none() && uuid_file.is_none())
            }
        };
        let esterni = dati.iter().any(|n| !locale(n));
        let esplicito = esterni || dati.iter().any(|n| n.attribute("FirstZ").or_else(|| n.attribute("FirstC")).or_else(|| n.attribute("FirstT")).is_some() || n.attribute("IFD").is_some_and(|v| v != "0"));
        if !esplicito {
            return;
        }
        let tutti = ifd_c.saturating_mul(self.z).saturating_mul(self.t);
        let mut mappa: Vec<Voce> = Vec::new();
        for n in dati.iter().filter(|n| locale(n)) {
            let g = |a: &str| n.attribute(a).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let (t0, z0, c0, ifd) = (g("FirstT"), g("FirstZ"), g("FirstC"), g("IFD"));
            let quanti = n.attribute("PlaneCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(if n.attribute("IFD").is_some() { 1 } else { usize::MAX });
            // Un blocco di piani consecutivi in ordine DimensionOrder, a partire da (t0, z0, c0); mai
            // oltre le pagine che il file ha davvero.
            let (mut t, mut z, mut c) = (t0, z0, c0);
            for k in 0..quanti.min(tutti).min(self.pagine) {
                let Some(pagina) = ifd.checked_add(k).filter(|p| *p < self.pagine) else { break };
                mappa.push(((t, z, c), pagina));
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
        if mappa.is_empty() {
            return;
        }
        if esterni || mappa.len() < tutti {
            // Un file di un OME-TIFF diviso in più file (o con meno piani di quelli dichiarati): si
            // tengono gli istanti, i piani e i canali che ci sono qui, numerati da capo, e lo si dice.
            let presenti = |f: fn(&Voce) -> usize| {
                let mut v: Vec<usize> = mappa.iter().map(f).collect();
                v.sort_unstable();
                v.dedup();
                v
            };
            let (ts, zs, cs) = (presenti(|v| v.0 .0), presenti(|v| v.0 .1), presenti(|v| v.0 .2));
            let posto = |v: &[usize], x: usize| v.binary_search(&x).unwrap_or(0);
            for voce in mappa.iter_mut() {
                voce.0 = (posto(&ts, voce.0 .0), posto(&zs, voce.0 .1), posto(&cs, voce.0 .2));
            }
            self.avviso = Some(format!(
                "{}: qui {} istanti su {}, {} piani Z su {}, {} canali su {}",
                if esterni { "un file di un OME-TIFF diviso in più file" } else { "il file ha meno piani di quelli dichiarati" },
                ts.len(),
                self.t,
                zs.len(),
                self.z,
                cs.len(),
                ifd_c
            ));
            self.t = ts.len();
            self.z = zs.len();
            if self.campioni <= 1 {
                self.canali = cs.iter().filter_map(|&c| self.canali.get(c).cloned()).collect();
            }
        }
        self.mappa = Some(mappa);
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

    /// ImageJ scrive le pile oltre i 4 GB (il limite del TIFF) con una pagina sola: gli altri piani
    /// seguono il primo, non compressi. Se è così, le pagine sono le `images` della descrizione.
    fn leggi_contigui(&mut self, path: &Path, t: &Testa, d: &mut Decoder<BufReader<File>>) {
        let immagini = t.descrizione.lines().find_map(|l| l.strip_prefix("images=")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(1);
        if immagini <= 1 || self.planare || !matches!(self.bit, 8 | 16 | 32) || d.get_tag_u32(Tag::Compression).unwrap_or(1) != 1 {
            return;
        }
        let (Ok(offsets), Ok(conti)) = (d.get_tag_u64_vec(Tag::StripOffsets), d.get_tag_u64_vec(Tag::StripByteCounts)) else { return };
        let Some(&inizio) = offsets.first() else { return };
        let piano = u64::from(t.larghezza) * u64::from(t.altezza) * self.campioni.max(1) as u64 * u64::from(self.bit / 8);
        // Le strisce del primo piano devono stare una dopo l'altra e fare un piano esatto.
        let mut fine = inizio;
        for (o, c) in offsets.iter().zip(&conti) {
            if *o != fine {
                return;
            }
            fine = fine.saturating_add(*c);
        }
        if fine - inizio != piano || piano == 0 {
            return;
        }
        let lunghezza = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let ci_stanno = usize::try_from(lunghezza.saturating_sub(inizio) / piano).unwrap_or(usize::MAX);
        let n = immagini.min(ci_stanno);
        if n <= 1 {
            return;
        }
        if n < immagini {
            self.avviso = Some(format!("il file è tagliato: {n} piani su {immagini}"));
        }
        self.pagine = n;
        self.contigui = Some((inizio, crate::imgfmt::head_of(path).starts_with(b"II")));
    }

    /// Un piano dei TIFF di ImageJ oltre i 4 GB (vedi `leggi_contigui`): letto direttamente dal file.
    fn leggi_contiguo(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        use std::io::{Read, Seek, SeekFrom};
        let (inizio, le) = self.contigui.ok_or("piano non trovato nel file")?;
        let (pagina, campione) = self.pagina(c, z, t).ok_or("piano non trovato nel file")?;
        let n = self.larghezza as usize * self.altezza as usize;
        let campioni = self.campioni.max(1);
        let byte = usize::from(self.bit / 8).max(1);
        let lato = n * campioni * byte;
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(inizio + pagina as u64 * lato as u64)).map_err(|e| e.to_string())?;
        let mut b = vec![0u8; lato];
        f.read_exact(&mut b).map_err(|_| format!("piano {} oltre la fine del file", pagina + 1))?;
        let x = |i: usize| -> &[u8] {
            let o = (i * campioni + campione) * byte;
            &b[o..o + byte]
        };
        let (dati, scala) = match byte {
            1 => ((0..n).map(|i| u16::from(x(i)[0])).collect(), 255),
            2 => ((0..n).map(|i| if le { u16::from_le_bytes([x(i)[0], x(i)[1]]) } else { u16::from_be_bytes([x(i)[0], x(i)[1]]) }).collect(), 65535),
            _ => {
                // ImageJ a 32 bit è sempre a virgola mobile.
                let v: Vec<f32> = (0..n).map(|i| {
                    let a = [x(i)[0], x(i)[1], x(i)[2], x(i)[3]];
                    if le { f32::from_le_bytes(a) } else { f32::from_be_bytes(a) }
                }).collect();
                (da_f32(&v), 65535)
            }
        };
        Ok(Piano { dati, scala })
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
                    Origine::Nikon if en => "Nikon ND2",
                    Origine::Nikon => "ND2 di Nikon",
                    Origine::Leica if en => "Leica LIF",
                    Origine::Leica => "LIF di Leica",
                }
                .into(),
            );
            if self.serie.len() > 1 {
                let nome = self.serie.get(self.numero_serie).cloned().unwrap_or_default();
                p.push(if en { format!("{} images (this one: {nome})", self.serie.len()) } else { format!("{} immagini (questa: {nome})", self.serie.len()) });
            }
            let n = self.canali.len();
            if n > 1 || self.origine == Origine::Ome {
                p.push(if en { format!("{n} channel{}", if n == 1 { "" } else { "s" }) } else { format!("{n} canal{}", if n == 1 { "e" } else { "i" }) });
            }
            if self.z > 1 {
                p.push(if en { format!("{} Z planes", self.z) } else { format!("{} piani Z", self.z) });
            }
            if self.assi_t.is_empty() {
                if self.t > 1 {
                    p.push(if en { format!("{} time points", self.t) } else { format!("{} istanti", self.t) });
                }
            } else {
                for (it, inglese, n) in &self.assi_t {
                    p.push(format!("{n} {}", if en { inglese } else { it }));
                }
            }
            p.push(if en { format!("{}-bit", self.bit) } else { format!("{} bit", self.bit) });
            p.push(format!("{}×{}", self.larghezza, self.altezza));
            if let Some(um) = self.um_per_px {
                let s = format!("{um:.4}").trim_end_matches('0').trim_end_matches('.').to_string();
                p.push(if en { format!("{s} µm/pixel") } else { format!("{} µm/pixel", s.replace('.', ",")) });
            }
            if let Some(o) = &self.obiettivo {
                p.push(o.clone());
            }
            p.join(" · ")
        };
        [line(false), line(true)]
    }
}

// ------------------------------------------------------------------ Colori

/// I colori di default quando il file non li dice: dal nome (DAPI blu, GFP verde, mCherry rosso,
/// 647 magenta…) o, per un nome che non dice nulla, nell'ordine di ImageJ.
pub fn canali_standard(n: usize, nomi: &[NomeColore]) -> Vec<Canale> {
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
        // `read_image` dei piani separati (PlanarConfiguration 2) legge solo il primo: qui si chiedono tutti.
        let mut letto = DecodingResult::U8(Vec::new());
        let disposizione = d.read_image_to_buffer(&mut letto).map_err(|e| format!("pagina {} non letta: {e}", pagina + 1))?;
        if letto.as_buffer(0).as_bytes().len() < disposizione.complete_len {
            return Err(format!("pagina {} non letta per intero", pagina + 1));
        }
        let campioni = self.campioni.max(1);
        // Il campione desiderato come sequenza lineare; un pixel che manca (file strano) vale 0, mai un panic.
        let indice = |i: usize| if self.planare { campione * n + i } else { i * campioni + campione };
        fn scegli<T: Copy + Default>(v: &[T], n: usize, indice: impl Fn(usize) -> usize) -> Vec<T> {
            (0..n).map(|i| v.get(indice(i)).copied().unwrap_or_default()).collect()
        }
        let (dati, scala): (Vec<u16>, u16) = match letto {
            DecodingResult::U8(v) => (scegli(&v, n, indice).into_iter().map(u16::from).collect(), 255),
            DecodingResult::U16(v) => (scegli(&v, n, indice), 65535),
            DecodingResult::I16(v) => (scegli(&v, n, indice).into_iter().map(|x| (i32::from(x) + 32768) as u16).collect(), 65535),
            DecodingResult::I8(v) => (scegli(&v, n, indice).into_iter().map(|x| (i16::from(x) + 128) as u16).collect(), 255),
            DecodingResult::U32(v) => (da_u32(&scegli(&v, n, indice)), 65535),
            DecodingResult::I32(v) => {
                // Senza negativi sono interi come gli altri; con i negativi si parte dal più basso.
                let v = scegli(&v, n, indice);
                let minimo = v.iter().copied().min().unwrap_or(0).min(0);
                (da_u32(&v.iter().map(|x| (i64::from(*x) - i64::from(minimo)) as u32).collect::<Vec<_>>()), 65535)
            }
            DecodingResult::F32(v) => (da_f32(&scegli(&v, n, indice)), 65535),
            DecodingResult::F64(v) => (da_f32(&scegli(&v, n, indice).into_iter().map(|x| x as f32).collect::<Vec<_>>()), 65535),
            _ => return Err("tipo di pixel non letto".into()),
        };
        Ok(Piano { dati, scala })
    }

    /// Un piano letto dalla sorgente giusta; il lettore TIFF si apre la prima volta e poi si riusa.
    fn piano_uno(&self, path: &Path, d: &mut Option<Decoder<BufReader<File>>>, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        match &self.sorgente {
            Sorgente::Tiff if self.contigui.is_some() => self.leggi_contiguo(path, c, z, t),
            Sorgente::Tiff => {
                let d = match d {
                    Some(d) => d,
                    None => d.insert(aperto(path)?),
                };
                self.leggi_piano(d, c, z, t)
            }
            Sorgente::Nd2(n) => n.fotogramma(path, n.indice(z, t))?.into_iter().nth(c).ok_or_else(|| "canale non trovato".to_string()),
            Sorgente::Lif(i) => i.piano(path, c, z, t),
        }
    }

    /// Il canale `c` all'istante `t`, con i piani Z uniti come scelto.
    pub fn canale(&self, path: &Path, c: usize, t: usize, piani: Piani) -> Result<Piano, String> {
        let mut d = None;
        match piani {
            Piani::Centrale => self.piano_uno(path, &mut d, c, self.z / 2, t),
            Piani::Massimo => {
                let mut totale = self.piano_uno(path, &mut d, c, 0, t)?;
                for z in 1..self.z {
                    massimo(&mut totale, &self.piano_uno(path, &mut d, c, z, t)?);
                }
                Ok(totale)
            }
        }
    }

    /// Un solo piano Z (per «un file per piano»).
    pub fn piano_z(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        self.piano_uno(path, &mut None, c, z, t)
    }

    /// Tutti i canali di un istante, ognuno su un thread. Negli ND2 un fotogramma tiene tutti i
    /// canali: si legge una volta sola, e sono i piani Z a dividersi i thread.
    pub fn canali_di(&self, path: &Path, t: usize, piani: Piani) -> Result<Vec<Piano>, String> {
        if let Sorgente::Nd2(n) = &self.sorgente {
            let zeta: Vec<usize> = match piani {
                Piani::Centrale => vec![self.z / 2],
                Piani::Massimo => (0..self.z).collect(),
            };
            let fili = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8).min(zeta.len());
            let parti = self.in_parallelo((0..fili).collect(), |k| {
                let mut tutti: Option<Vec<Piano>> = None;
                for &z in zeta.iter().skip(k).step_by(fili) {
                    let f = n.fotogramma(path, n.indice(z, t))?;
                    match &mut tutti {
                        None => tutti = Some(f),
                        Some(a) => a.iter_mut().zip(&f).for_each(|(a, b)| massimo(a, b)),
                    }
                }
                tutti.ok_or_else(|| "piano non trovato nel file".to_string())
            })?;
            let mut parti = parti.into_iter();
            let mut out = parti.next().ok_or("piano non trovato nel file")?;
            for p in parti {
                out.iter_mut().zip(&p).for_each(|(a, b)| massimo(a, b));
            }
            return Ok(out);
        }
        self.in_parallelo((0..self.canali.len()).collect(), |c| self.canale(path, c, t, piani))
    }

    /// Un piano Z di tutti i canali.
    pub fn canali_di_z(&self, path: &Path, z: usize, t: usize) -> Result<Vec<Piano>, String> {
        if let Sorgente::Nd2(n) = &self.sorgente {
            return n.fotogramma(path, n.indice(z, t));
        }
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

/// Tiene in `a` il più alto dei due valori, pixel per pixel.
fn massimo(a: &mut Piano, b: &Piano) {
    for (x, y) in a.dati.iter_mut().zip(&b.dati) {
        if *y > *x {
            *x = *y;
        }
    }
}

// ------------------------------------------------------------------ Immagine

/// Interi a 32 bit portati a 16: tali e quali se stanno in 16 bit (i conteggi delle camere, le
/// etichette), altrimenti divisi quanto basta perché il più alto arrivi a fondo scala.
pub(crate) fn da_u32(v: &[u32]) -> Vec<u16> {
    let alto = v.iter().copied().max().unwrap_or(0);
    if alto <= 65535 {
        return v.iter().map(|x| *x as u16).collect();
    }
    v.iter().map(|x| (u64::from(*x) * 65535 / u64::from(alto)) as u16).collect()
}

/// Numeri a virgola mobile portati a 16 bit: tra 0 e 1 a tutta scala, altrimenti tenuti come sono
/// (sotto lo 0 e oltre 65.535 si taglia); quelli non finiti valgono 0.
pub(crate) fn da_f32(v: &[f32]) -> Vec<u16> {
    let alto = v.iter().copied().filter(|x| x.is_finite()).fold(0.0f32, f32::max);
    let k = if alto > 0.0 && alto <= 1.0 { 65535.0 } else { 1.0 };
    v.iter().map(|x| if x.is_finite() { (x * k).clamp(0.0, 65535.0) as u16 } else { 0 }).collect()
}

/// L'istogramma di un piano: quanti pixel per ogni valore da 0 a `scala`.
pub fn istogramma(p: &Piano) -> Vec<u64> {
    let mut ist = Vec::new();
    aggiungi_istogramma(&mut ist, p);
    ist
}

/// Aggiunge a `ist` i pixel di `p` (lo stesso canale in un altro istante o piano): così i limiti del
/// contrasto si calcolano una volta per tutta la pila.
pub fn aggiungi_istogramma(ist: &mut Vec<u64>, p: &Piano) {
    if ist.len() < p.scala as usize + 1 {
        ist.resize(p.scala as usize + 1, 0);
    }
    for v in &p.dati {
        ist[(*v).min(p.scala) as usize] += 1;
    }
}

/// I limiti del contrasto automatico da un istogramma: il valore sotto cui sta lo 0,5% dei pixel e
/// quello sopra cui sta lo 0,2% (come «Auto» di ImageJ, più rispettoso dei fondi scuri).
pub fn limiti_da(istogramma: &[u64]) -> (u16, u16) {
    let scala = istogramma.len().saturating_sub(1).min(65535) as u16;
    let tot: u64 = istogramma.iter().sum();
    let (mut basso, mut alto) = (0u16, scala);
    let mut somma = 0u64;
    for (v, n) in istogramma.iter().enumerate() {
        somma += *n;
        if somma * 1000 >= tot * 5 {
            basso = v as u16;
            break;
        }
    }
    let mut somma = 0u64;
    for (v, n) in istogramma.iter().enumerate().rev() {
        somma += *n;
        if somma * 1000 > tot * 2 {
            alto = v as u16;
            break;
        }
    }
    if alto <= basso {
        alto = basso.saturating_add(1).min(scala.max(1));
    }
    (basso, alto)
}

/// I limiti del contrasto automatico di un piano solo.
pub fn limiti(p: &Piano) -> (u16, u16) {
    limiti_da(&istogramma(p))
}

/// I limiti senza contrasto: tutta la scala dei dati. Per i dati a 12 o 14 bit in 16 bit (ND2, LIF)
/// il fondo scala è quello del sensore, se no l'immagine resta quasi nera.
pub fn limiti_pieni(pila: &Pila, p: &Piano) -> (u16, u16) {
    let alto = if p.scala == 65535 && pila.bit > 8 && pila.bit < 16 { (1u16 << pila.bit) - 1 } else { p.scala };
    (0, alto.max(1))
}

/// I limiti di ogni canale di `canali`: col contrasto automatico, piano per piano; senza, la scala intera.
pub fn limiti_di(pila: &Pila, canali: &[Piano], contrasto: bool) -> Vec<(u16, u16)> {
    canali.iter().map(|p| if contrasto { limiti(p) } else { limiti_pieni(pila, p) }).collect()
}

/// I limiti scritti per chi legge: «DAPI 112–3480, GFP 90–2010».
pub fn descrivi_limiti(pila: &Pila, limiti: &[(u16, u16)]) -> String {
    limiti
        .iter()
        .enumerate()
        .map(|(c, (b, a))| format!("{} {b}–{a}", pila.canali.get(c).map_or_else(|| format!("C{}", c + 1), |c| c.nome.clone())))
        .collect::<Vec<_>>()
        .join(", ")
}

/// La tavola che porta un valore tra `basso` e `alto` a 0..=255.
fn tavola(scala: u16, (basso, alto): (u16, u16)) -> Vec<u8> {
    let k = 65536.0 / f64::from(alto.saturating_sub(basso).max(1));
    (0..=scala as usize).map(|v| ((v as f64 - f64::from(basso)) * k / 256.0).clamp(0.0, 255.0) as u8).collect()
}

/// I canali sovrapposti, ognuno acceso del suo colore (somma, come il «composito» di ImageJ).
pub fn composita(pila: &Pila, canali: &[Piano], contrasto: bool) -> RgbImage {
    composita_con(pila, canali, &limiti_di(pila, canali, contrasto))
}

/// Come `composita`, con i limiti di ogni canale già scelti (gli stessi per tutti gli istanti e i piani).
pub fn composita_con(pila: &Pila, canali: &[Piano], limiti: &[(u16, u16)]) -> RgbImage {
    let (w, h) = (pila.larghezza, pila.altezza);
    let n = w as usize * h as usize;
    let mut acc = vec![[0u32; 3]; n];
    for (c, piano) in canali.iter().enumerate() {
        let colore = pila.canali.get(c).map_or((255, 255, 255), |c| c.colore);
        let lim = limiti.get(c).copied().unwrap_or((0, piano.scala));
        // Tavola di conversione: valore → 0..=255 (una sola volta per canale).
        let tavola = tavola(piano.scala, lim);
        for (px, v) in acc.iter_mut().zip(&piano.dati) {
            let x = u32::from(tavola[(*v).min(piano.scala) as usize]);
            px[0] += x * u32::from(colore.0) / 255;
            px[1] += x * u32::from(colore.1) / 255;
            px[2] += x * u32::from(colore.2) / 255;
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
    let lim = if contrasto { limiti(piano) } else { limiti_pieni(pila, piano) };
    grigio_con(pila, piano, lim)
}

/// Un canale in grigio con i limiti già scelti.
pub fn grigio_con(pila: &Pila, piano: &Piano, lim: (u16, u16)) -> DynamicImage {
    let tavola = tavola(piano.scala, lim);
    let buf = ImageBuffer::from_raw(pila.larghezza, pila.altezza, piano.dati.iter().map(|v| tavola[(*v).min(piano.scala) as usize]).collect()).unwrap_or_else(|| image::GrayImage::new(1, 1));
    DynamicImage::ImageLuma8(buf)
}

/// L'OME-XML minimo (schema 2016-06) di un TIFF a 16 bit con un piano di un canale: Fiji
/// (Bio-Formats) ci trova la misura del pixel in µm e il nome del canale. Tutto in ASCII, come vuole
/// il TIFF: l'unità non si scrive (per lo schema PhysicalSizeX è in µm se non dice altro) e le lettere
/// non ASCII dei nomi diventano riferimenti numerici (`&#181;`).
pub fn ome_xml(nome: &str, canale: &str, larghezza: u32, altezza: u32, um_per_px: Option<f64>) -> String {
    let esc = |s: &str| {
        s.chars()
            .map(|c| match c {
                '&' => "&amp;".to_string(),
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '"' => "&quot;".to_string(),
                c if c.is_ascii_control() => " ".to_string(),
                c if c.is_ascii() => c.to_string(),
                c => format!("&#{};", c as u32),
            })
            .collect::<String>()
    };
    let misura = um_per_px.filter(|u| u.is_finite() && *u > 0.0).map(|u| format!(r#" PhysicalSizeX="{u}" PhysicalSizeY="{u}""#)).unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><OME xmlns="http://www.openmicroscopy.org/Schemas/OME/2016-06" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.openmicroscopy.org/Schemas/OME/2016-06 http://www.openmicroscopy.org/Schemas/OME/2016-06/ome.xsd" Creator="Cedro"><Image ID="Image:0" Name="{}"><Pixels ID="Pixels:0" DimensionOrder="XYZCT" Type="uint16" SizeX="{larghezza}" SizeY="{altezza}" SizeZ="1" SizeC="1" SizeT="1"{misura}><Channel ID="Channel:0:0" Name="{}" SamplesPerPixel="1"/><TiffData IFD="0" PlaneCount="1"/></Pixels></Image></OME>"#,
        esc(nome),
        esc(canale)
    )
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
    barra_con(img, um_per_px, true)
}

/// La barra senza la scritta, come nelle figure degli articoli (la misura va nella didascalia).
pub fn disegna_barra_sola(img: &mut RgbImage, um_per_px: f64) -> Option<String> {
    barra_con(img, um_per_px, false)
}

fn barra_con(img: &mut RgbImage, um_per_px: f64, con_testo: bool) -> Option<String> {
    let (w, h) = img.dimensions();
    let (um, testo) = misura_barra(w, um_per_px)?;
    let px = (um / um_per_px).round() as i64;
    let lato = w.min(h).max(1) as f64;
    let spessore = ((lato / 110.0).round() as i64).max(3);
    // Le lettere: 2 pixel per punto dalle immagini medie in su, 1 per le piccolissime (128 px).
    let k = ((lato / 260.0).round() as i64).max(if lato < 200.0 { 1 } else { 2 });
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
    if !con_testo {
        return Some(testo);
    }
    // La scritta, centrata sopra la barra: contorno nero, poi bianco.
    let larghezza_testo = testo.chars().count() as i64 * 6 * k - k;
    // Centrata sulla barra, ma dentro l'immagine anche quando la barra è più corta della scritta.
    let tx = (x0 + (px - larghezza_testo) / 2).min(i64::from(w) - larghezza_testo - contorno - 1).max(contorno + 1);
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

#[cfg(test)]
mod prove_revisione {
    use super::*;
    use tiff::encoder::{colortype, TiffEncoder};

    fn campione(n: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/micro").join(n)
    }

    #[test]
    fn un_tiff_a_piani_separati_si_legge_tutto() {
        // tifffile scrive così un RGB a 16 bit di forma (3, Y, X): prima tutto R, poi G, poi B.
        let p = campione("planare-rgb16.ome.tif");
        let pila = Pila::open(&p).unwrap();
        assert_eq!(pila.canali.len(), 3);
        let g = pila.canale(&p, 1, 0, Piani::Centrale).unwrap();
        assert_eq!((g.dati[0], g.dati[4 * 24 + 5]), (2000, 60000));
        assert_eq!(pila.canale(&p, 2, 0, Piani::Centrale).unwrap().dati[0], 3000);
        assert_eq!(pila.canali_di(&p, 0, Piani::Massimo).unwrap().len(), 3);
    }

    #[test]
    fn gli_interi_a_32_bit_piccoli_restano_tali_e_quali() {
        let p = campione("u32.ome.tif");
        let pila = Pila::open(&p).unwrap();
        let c = pila.canale(&p, 0, 0, Piani::Centrale).unwrap();
        assert_eq!(c.dati.iter().max(), Some(&1000));
        assert!(c.dati.contains(&300));
        let img = composita(&pila, &[c], true);
        assert!(img.pixels().any(|p| p.0 != [0, 0, 0]), "un'immagine a 32 bit non deve uscire nera");
        assert_eq!(da_u32(&[0, 70_000, 140_000]), [0, 32_767, 65_535]);
    }

    #[test]
    fn l_unita_pixel_non_e_una_misura() {
        let pila = Pila::open(&campione("unita-pixel.ome.tif")).unwrap();
        assert_eq!(pila.um_per_px, None);
    }

    #[test]
    fn un_pezzo_di_un_ome_diviso_in_piu_file() {
        // Solo l'istante 2 di 3, con i suoi due canali: non diventano due piani Z di un canale.
        let p = campione("multi_t1.ome.tif");
        let pila = Pila::open(&p).unwrap();
        assert_eq!(pila.canali.iter().map(|c| c.nome.as_str()).collect::<Vec<_>>(), ["DAPI", "GFP"]);
        assert_eq!((pila.z, pila.t), (1, 1));
        assert!(pila.avviso.as_deref().is_some_and(|a| a.contains("1 istanti su 3")), "{:?}", pila.avviso);
        assert_eq!(pila.canale(&p, 0, 0, Piani::Massimo).unwrap().dati[0], 110);
        assert_eq!(pila.canale(&p, 1, 0, Piani::Massimo).unwrap().dati[0], 111);
    }

    /// Un TIFF di ImageJ come quelli oltre i 4 GB: una pagina sola, i piani (16 bit) uno dopo l'altro.
    fn imagej_contiguo(dir: &Path, nome: &str, immagini: usize, piani_veri: usize) -> std::path::PathBuf {
        let (w, h) = (4u32, 3u32);
        let desc = format!("ImageJ=1.53t\nimages={immagini}\nslices={immagini}\nunit=micron\n\0");
        let piano = (w * h * 2) as usize;
        let n_voci = 10u16;
        let ifd = 8u32;
        let dopo_ifd = ifd + 2 + u32::from(n_voci) * 12 + 4;
        let dati = dopo_ifd + desc.len() as u32;
        let mut b: Vec<u8> = b"II*\0".to_vec();
        b.extend(ifd.to_le_bytes());
        b.extend(n_voci.to_le_bytes());
        let mut voce = |tag: u16, tipo: u16, n: u32, v: u32| {
            b.extend(tag.to_le_bytes());
            b.extend(tipo.to_le_bytes());
            b.extend(n.to_le_bytes());
            b.extend(v.to_le_bytes());
        };
        voce(256, 4, 1, w);
        voce(257, 4, 1, h);
        voce(258, 3, 1, 16);
        voce(259, 3, 1, 1);
        voce(262, 3, 1, 1);
        voce(270, 2, desc.len() as u32, dopo_ifd);
        voce(273, 4, 1, dati);
        voce(277, 3, 1, 1);
        voce(278, 4, 1, h);
        voce(279, 4, 1, piano as u32);
        b.extend(0u32.to_le_bytes());
        b.extend(desc.as_bytes());
        for k in 0..piani_veri {
            for _ in 0..w * h {
                b.extend(((k + 1) as u16 * 100).to_le_bytes());
            }
        }
        let p = dir.join(nome);
        std::fs::write(&p, b).unwrap();
        p
    }

    #[test]
    fn imagej_oltre_i_4_gb_ha_tutti_i_piani() {
        let dir = std::env::temp_dir().join(format!("cedro-micro-contigui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = imagej_contiguo(&dir, "grande.tif", 3, 3);
        let pila = Pila::open(&p).unwrap();
        assert_eq!((pila.z, pila.pagine), (3, 3));
        assert_eq!(pila.piano_z(&p, 0, 2, 0).unwrap().dati[0], 300);
        assert_eq!(pila.canale(&p, 0, 0, Piani::Massimo).unwrap().dati[5], 300);
        // Un file tagliato: i piani che ci sono, e lo si dice.
        let p = imagej_contiguo(&dir, "tagliato.tif", 5, 3);
        let pila = Pila::open(&p).unwrap();
        assert_eq!(pila.z, 3);
        assert!(pila.avviso.as_deref().is_some_and(|a| a.contains("3 piani su 5")), "{:?}", pila.avviso);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn misure_assurde_non_riempiono_la_memoria() {
        let dir = std::env::temp_dir().join(format!("cedro-micro-assurdo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("assurdo.ome.tif");
        let xml = r#"<OME xmlns="http://www.openmicroscopy.org/Schemas/OME/2016-06"><Image ID="Image:0"><Pixels ID="Pixels:0" DimensionOrder="XYZCT" Type="uint16" SizeX="4" SizeY="4" SizeZ="4294967296" SizeC="4000000000" SizeT="4294967296"><TiffData FirstZ="0"/></Pixels></Image></OME>"#;
        let mut f = File::create(&p).unwrap();
        let mut enc = TiffEncoder::new(&mut f).unwrap();
        let mut img = enc.new_image::<colortype::Gray16>(4, 4).unwrap();
        img.encoder().write_tag(Tag::ImageDescription, xml).unwrap();
        img.write_data(&[5u16; 16]).unwrap();
        drop(enc);
        let pila = Pila::open(&p).unwrap();
        assert!(pila.canali.len() <= 4096);
        assert_eq!(pila.canale(&p, 0, 0, Piani::Centrale).unwrap().dati[0], 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn senza_contrasto_i_12_bit_vanno_a_fondo_scala_del_sensore() {
        let mut pila = Pila::vuota(Origine::Nikon, 2, 1, Sorgente::Tiff);
        pila.bit = 12;
        let p = Piano { dati: vec![0, 4095], scala: 65535 };
        assert_eq!(limiti_pieni(&pila, &p), (0, 4095));
        pila.bit = 16;
        assert_eq!(limiti_pieni(&pila, &p), (0, 65535));
    }

    #[test]
    fn i_limiti_comuni_vengono_da_tutti_i_piani() {
        let a = Piano { dati: vec![0; 1000].into_iter().chain([1000]).collect(), scala: 65535 };
        let b = Piano { dati: (0..1000).map(|i| 100 + i as u16 * 10).collect(), scala: 65535 };
        let mut ist = Vec::new();
        aggiungi_istogramma(&mut ist, &a);
        aggiungi_istogramma(&mut ist, &b);
        let (basso, alto) = limiti_da(&ist);
        // L'alto viene dal piano più acceso, non da quello quasi vuoto.
        assert!(alto > 10_000 && basso == 0, "{basso}–{alto}");
    }

    #[test]
    fn l_ome_xml_e_ascii_con_la_misura() {
        let x = ome_xml("prova", "Alexa 488 µ<&>", 10, 20, Some(0.325));
        assert!(x.is_ascii());
        assert!(x.contains(r#"PhysicalSizeX="0.325""#) && x.contains("&#181;&lt;&amp;&gt;"));
        assert!(roxmltree::Document::parse(&x).is_ok());
    }
}
