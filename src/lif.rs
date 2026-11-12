//! Lettore dei file LIF di Leica (LAS X e LAS AF): un «progetto» con più immagini (serie).
//!
//! In testa c'è un XML in UTF-16 che descrive le immagini: per ognuna i canali (`ChannelDescription`,
//! con la LUT di colore e lo scarto in byte del canale) e le dimensioni (`DimensionDescription`:
//! X, Y, Z, tempo, lunghezze d'onda, tessere…, ognuna con quanti elementi, quanto è lunga e di
//! quanti byte si sposta). Poi i blocchi di memoria, ognuno con il suo nome (`MemBlock_N`), che
//! tengono i pixel di un'immagine. Un pixel è all'indirizzo «scarto del canale + Σ indice × passo».
//! Il formato lo descrive Leica («Leica Image File Formats»); i file di prova si confrontano con la
//! libreria `liffile`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::microscopia::{NomeColore, Piano};

/// Vero se i primi byte sono quelli di un LIF: `70 00 00 00`, lunghezza, `2A`.
pub fn riconosci(head: &[u8]) -> bool {
    head.len() >= 9 && head[..4] == [0x70, 0, 0, 0] && head[8] == 0x2A
}

/// Una dimensione dell'immagine.
#[derive(Clone, Debug)]
struct Dim {
    id: u32,
    n: usize,
    lunghezza: f64,
    unita: String,
    passo: u64,
}

#[derive(Clone, Debug)]
struct CanaleLif {
    scarto: u64,
    byte: usize,
    decimale: bool,
}

/// Un'immagine (serie) del progetto.
#[derive(Clone, Debug)]
pub struct Immagine {
    /// Il percorso nel progetto: «Cartella/Serie003».
    pub nome: String,
    pub larghezza: u32,
    pub altezza: u32,
    pub z: usize,
    /// Tempo, lunghezze d'onda, tessere… messi in fila (il primo è il più esterno).
    pub altri: Vec<(u32, usize)>,
    pub canali: Vec<NomeColore>,
    pub bit: u8,
    pub um_per_px: Option<f64>,
    pub um_z: Option<f64>,
    pub obiettivo: Option<String>,
    /// I tre canali sono un RGB (ChannelTag 1, 2, 3 a 8 bit).
    pub rgb: bool,
    x: Dim,
    righe: Dim,
    dim_z: Option<Dim>,
    dims_altri: Vec<Dim>,
    lista_canali: Vec<CanaleLif>,
    inizio: u64,
}

#[derive(Debug)]
pub struct Lif {
    pub immagini: Vec<Immagine>,
}

fn leggi_u32(f: &mut impl Read) -> Option<u32> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}

fn leggi_u8(f: &mut impl Read) -> Option<u8> {
    let mut b = [0u8; 1];
    f.read_exact(&mut b).ok()?;
    Some(b[0])
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

/// Micron per unità: i LIF scrivono le lunghezze in metri, a volte in micron.
fn in_micron(unita: &str) -> Option<f64> {
    match unita.trim() {
        "m" => Some(1e6),
        "mm" => Some(1e3),
        "um" | "µm" | "μm" | "micron" => Some(1.0),
        "nm" => Some(1e-3),
        _ => None,
    }
}

/// Il passo tra un elemento e il successivo, in micron.
fn passo_um(d: &Dim) -> Option<f64> {
    (d.n > 1).then(|| in_micron(&d.unita).map(|k| d.lunghezza.abs() / (d.n - 1) as f64 * k)).flatten().filter(|v| v.is_finite() && *v > 0.0)
}

fn colore_lut(lut: &str) -> Option<(u8, u8, u8)> {
    Some(match lut.trim().to_ascii_lowercase().as_str() {
        "red" => (255, 0, 0),
        "green" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "cyan" => (0, 255, 255),
        "magenta" => (255, 0, 255),
        "yellow" => (255, 255, 0),
        "gray" | "grey" | "white" => (255, 255, 255),
        _ => return None,
    })
}

fn attr_num<T: std::str::FromStr>(n: roxmltree::Node, a: &str) -> Option<T> {
    n.attribute(a)?.trim().parse().ok()
}

/// Il primo discendente con quel nome.
fn trova<'a, 'i>(n: roxmltree::Node<'a, 'i>, nome: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.descendants().find(|d| d.has_tag_name(nome))
}

/// I figli diretti con quel nome.
fn figli<'a, 'i>(n: roxmltree::Node<'a, 'i>, nome: &'a str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.has_tag_name(nome))
}

fn figlio<'a, 'i>(n: roxmltree::Node<'a, 'i>, nome: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(nome))
}

/// Un'immagine dal suo `Element` (che ha `Data/Image` e `Memory`).
fn immagine(el: roxmltree::Node, nome: String, blocchi: &HashMap<String, (u64, u64)>) -> Result<Option<Immagine>, String> {
    let Some(img) = figlio(el, "Data").and_then(|d| figlio(d, "Image")) else { return Ok(None) };
    let Some(desc) = figlio(img, "ImageDescription") else { return Ok(None) };
    let Some(memoria) = figlio(el, "Memory") else { return Ok(None) };
    let grandezza: u64 = attr_num(memoria, "Size").unwrap_or(0);
    if grandezza == 0 {
        return Ok(None);
    }
    let id = memoria.attribute("MemoryBlockID").unwrap_or("");
    let Some(&(inizio, dentro)) = blocchi.get(id) else { return Err(format!("nel file LIF manca il blocco di «{nome}»")) };

    let mut dims: Vec<Dim> = Vec::new();
    for d in figlio(desc, "Dimensions").into_iter().flat_map(|d| figli(d, "DimensionDescription")) {
        let n: usize = attr_num(d, "NumberOfElements").unwrap_or(1);
        if n == 0 {
            return Err(format!("dimensione vuota in «{nome}»"));
        }
        dims.push(Dim {
            id: attr_num(d, "DimID").unwrap_or(0),
            n,
            lunghezza: attr_num(d, "Length").unwrap_or(0.0),
            unita: d.attribute("Unit").unwrap_or("").to_string(),
            passo: attr_num(d, "BytesInc").unwrap_or(0),
        });
    }
    let mut canali_xml: Vec<roxmltree::Node> = figlio(desc, "Channels").into_iter().flat_map(|c| figli(c, "ChannelDescription")).collect();
    canali_xml.sort_by_key(|c| attr_num::<u64>(*c, "BytesInc").unwrap_or(0));
    if canali_xml.is_empty() || dims.is_empty() {
        return Ok(None);
    }
    let mut lista = Vec::new();
    let mut bit = 8u8;
    for c in &canali_xml {
        let risoluzione: u32 = attr_num(*c, "Resolution").unwrap_or(8);
        let decimale = attr_num::<u32>(*c, "DataType").unwrap_or(0) == 1;
        let byte = match risoluzione {
            1..=8 if !decimale => 1,
            1..=16 if !decimale => 2,
            17..=32 => 4,
            _ => return Err(format!("pixel di {risoluzione} bit: Cedro non li legge ancora")),
        };
        bit = bit.max(if decimale { 32 } else { risoluzione as u8 });
        lista.push(CanaleLif { scarto: attr_num(*c, "BytesInc").unwrap_or(0), byte, decimale });
    }
    // Il nome di ogni canale: il colorante, se c'è; poi i nomi delle bande del rilevatore.
    let tinte: Vec<String> = canali_xml
        .iter()
        .map(|c| {
            figli(*c, "ChannelProperty")
                .find(|p| figlio(*p, "Key").and_then(|k| k.text()).is_some_and(|k| k.trim() == "DyeName"))
                .and_then(|p| figlio(p, "Value").and_then(|v| v.text()))
                .map(|v| v.trim().trim_start_matches("Leica/").trim().to_string())
                .unwrap_or_default()
        })
        .collect();
    let bande: Vec<String> = img
        .children()
        .filter(|a| a.has_tag_name("Attachment") && a.attribute("Name") == Some("HardwareSetting"))
        .filter_map(|a| figlio(a, "ATLConfocalSettingDefinition"))
        .flat_map(|d| d.descendants().filter(|m| m.has_tag_name("MultiBand")).filter_map(|m| m.attribute("DyeName")).filter(|s| !s.trim().is_empty()).map(|s| s.trim().trim_start_matches("Leica/").trim().to_string()).collect::<Vec<_>>())
        .collect();
    let tag: Vec<u32> = canali_xml.iter().map(|c| attr_num(*c, "ChannelTag").unwrap_or(0)).collect();
    let rgb = lista.len() == 3 && tag == [1, 2, 3] && bit == 8;
    let canali: Vec<NomeColore> = canali_xml
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut nome = tinte[i].clone();
            if nome.is_empty() && bande.len() == canali_xml.len() {
                nome = bande[i].clone();
            }
            let colore = match tag[i] {
                1 => Some((255, 0, 0)),
                2 => Some((0, 255, 0)),
                3 => Some((0, 0, 255)),
                _ => c.attribute("LUTName").and_then(colore_lut),
            };
            if nome.is_empty() && rgb {
                nome = ["R", "G", "B"][i].to_string();
            }
            (nome, colore)
        })
        .collect();

    // X in orizzontale; in verticale Y, o (per le scansioni XZ e XT) la dimensione col passo più corto.
    let x = dims.iter().position(|d| d.id == 1).unwrap_or(0);
    let x = dims.remove(x);
    let spaziale = |d: &Dim| matches!(d.id, 2 | 3);
    let righe = dims.iter().enumerate().filter(|(_, d)| spaziale(d)).min_by_key(|(_, d)| d.passo).or_else(|| dims.iter().enumerate().min_by_key(|(_, d)| d.passo)).map(|(i, _)| i);
    let righe = match righe {
        Some(i) => dims.remove(i),
        None => Dim { id: 2, n: 1, lunghezza: 0.0, unita: String::new(), passo: 0 },
    };
    // La pila: Z, o Y per le serie di piani XZ.
    let dim_z = dims.iter().position(spaziale).map(|i| dims.remove(i));
    let mut altri: Vec<Dim> = dims.into_iter().filter(|d| d.n > 1).collect();
    altri.sort_by_key(|d| std::cmp::Reverse(d.passo));
    if x.n as u64 * righe.n as u64 > 1 << 30 {
        return Err(format!("«{nome}» è troppo grande ({}×{})", x.n, righe.n));
    }

    // L'ultimo byte che si legge deve stare nel blocco.
    let mut ultimo = lista.iter().map(|c| c.scarto + c.byte as u64).max().unwrap_or(0);
    for d in [Some(&x), Some(&righe), dim_z.as_ref()].into_iter().flatten().chain(altri.iter()) {
        ultimo += (d.n as u64 - 1) * d.passo;
    }
    if ultimo > grandezza.min(dentro) {
        return Err(format!("le misure di «{nome}» non stanno nei suoi dati"));
    }

    let obiettivo = trova(img, "ATLConfocalSettingDefinition")
        .or_else(|| trova(img, "ATLCameraSettingDefinition"))
        .and_then(|d| d.attribute("ObjectiveName"))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty());

    Ok(Some(Immagine {
        nome,
        larghezza: x.n as u32,
        altezza: righe.n as u32,
        z: dim_z.as_ref().map_or(1, |d| d.n),
        altri: altri.iter().map(|d| (d.id, d.n)).collect(),
        canali,
        bit,
        um_per_px: passo_um(&x),
        um_z: dim_z.as_ref().and_then(passo_um),
        obiettivo,
        rgb,
        x,
        righe,
        dim_z,
        dims_altri: altri,
        lista_canali: lista,
        inizio,
    }))
}

fn raccogli(el: roxmltree::Node, percorso: &str, blocchi: &HashMap<String, (u64, u64)>, out: &mut Vec<Immagine>, profondita: u32) -> Result<(), String> {
    if profondita > 64 || out.len() > 10_000 {
        return Ok(());
    }
    let nome = el.attribute("Name").unwrap_or("").trim().to_string();
    let percorso = if percorso.is_empty() { nome.clone() } else { format!("{percorso}/{nome}") };
    if let Some(i) = immagine(el, percorso.clone(), blocchi)? {
        out.push(i);
    }
    for figlio_el in figlio(el, "Children").into_iter().flat_map(|c| figli(c, "Element")) {
        raccogli(figlio_el, &percorso, blocchi, out, profondita + 1)?;
    }
    Ok(())
}

impl Lif {
    pub fn open(path: &Path) -> Result<Lif, String> {
        let mut f = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
        let lunghezza_file = f.get_ref().metadata().map_err(|e| e.to_string())?.len();
        let (a, _, b) = (leggi_u32(&mut f), leggi_u32(&mut f), leggi_u8(&mut f));
        if a != Some(0x70) || b != Some(0x2A) {
            return Err("non è un file LIF".into());
        }
        let caratteri = leggi_u32(&mut f).ok_or("file LIF troncato")? as u64;
        if caratteri * 2 > lunghezza_file.min(256 << 20) {
            return Err("intestazione del file LIF non valida".into());
        }
        let mut xml = vec![0u8; caratteri as usize * 2];
        f.read_exact(&mut xml).map_err(|_| "file LIF troncato")?;
        let xml = utf16(&xml);
        let opzioni = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 20_000_000, ..Default::default() };
        let doc = roxmltree::Document::parse_with_options(&xml, opzioni).map_err(|e| format!("intestazione del file LIF non letta: {e}"))?;
        let radice = doc.root_element();
        let versione: u32 = attr_num(radice, "Version").unwrap_or(2);

        // I blocchi di memoria, uno dopo l'altro fino in fondo.
        let mut blocchi: HashMap<String, (u64, u64)> = HashMap::new();
        let mut pos = f.stream_position().map_err(|e| e.to_string())?;
        while pos + 9 < lunghezza_file {
            let testa = leggi_u32(&mut f);
            let _ = leggi_u32(&mut f);
            let segno = leggi_u8(&mut f);
            if testa != Some(0x70) || segno != Some(0x2A) {
                break;
            }
            let grandezza = if versione >= 2 {
                let mut b = [0u8; 8];
                if f.read_exact(&mut b).is_err() {
                    break;
                }
                u64::from_le_bytes(b)
            } else {
                match leggi_u32(&mut f) {
                    Some(v) => u64::from(v),
                    None => break,
                }
            };
            if leggi_u8(&mut f) != Some(0x2A) {
                break;
            }
            let Some(n) = leggi_u32(&mut f) else { break };
            if n > 4096 {
                break;
            }
            let mut nome = vec![0u8; n as usize * 2];
            if f.read_exact(&mut nome).is_err() {
                break;
            }
            let inizio = f.stream_position().map_err(|e| e.to_string())?;
            let fine = inizio.saturating_add(grandezza);
            // Un file tagliato: il blocco vale per quello che c'è.
            blocchi.insert(utf16(&nome), (inizio, grandezza.min(lunghezza_file.saturating_sub(inizio))));
            if fine >= lunghezza_file {
                break;
            }
            f.seek(SeekFrom::Start(fine)).map_err(|e| e.to_string())?;
            pos = fine;
        }

        let mut immagini = Vec::new();
        let elemento = figlio(radice, "Element").ok_or("il file LIF non ha immagini")?;
        raccogli(elemento, "", &blocchi, &mut immagini, 0)?;
        // Il nome della radice è quello del progetto: non serve ripeterlo.
        let radice_nome = elemento.attribute("Name").unwrap_or("").trim().to_string();
        for i in &mut immagini {
            if let Some(r) = i.nome.strip_prefix(&format!("{radice_nome}/")) {
                i.nome = r.to_string();
            }
        }
        if immagini.is_empty() {
            return Err("il file LIF non ha immagini".into());
        }
        Ok(Lif { immagini })
    }
}

impl Immagine {
    /// Tempo, tessere e il resto messi in fila: il numero degli «istanti».
    pub fn t(&self) -> usize {
        self.altri.iter().map(|(_, n)| *n).product::<usize>().max(1)
    }

    /// Il canale `c` del piano `z` all'istante `t`.
    pub fn piano(&self, path: &Path, c: usize, z: usize, t: usize) -> Result<Piano, String> {
        let canale = self.lista_canali.get(c).ok_or("canale non trovato")?;
        let mut base = self.inizio + canale.scarto;
        if let Some(d) = &self.dim_z {
            base += z.min(d.n - 1) as u64 * d.passo;
        }
        let mut resto = t;
        for d in self.dims_altri.iter().rev() {
            base += (resto % d.n) as u64 * d.passo;
            resto /= d.n;
        }
        let (w, h) = (self.x.n, self.righe.n);
        let bpc = canale.byte;
        let riga = (w as u64 - 1) * self.x.passo + bpc as u64;
        let mut f = BufReader::with_capacity(1 << 20, File::open(path).map_err(|e| e.to_string())?);
        let mut grezzo: Vec<u8> = Vec::new();
        // Righe una dopo l'altra: un'unica lettura.
        let di_fila = self.righe.passo == riga || h == 1;
        if di_fila {
            grezzo.resize((riga * h as u64) as usize, 0);
            f.seek(SeekFrom::Start(base)).map_err(|e| e.to_string())?;
            f.read_exact(&mut grezzo).map_err(|_| "file LIF troncato")?;
        } else {
            grezzo.reserve((riga * h as u64) as usize);
            let mut r = vec![0u8; riga as usize];
            for y in 0..h as u64 {
                f.seek(SeekFrom::Start(base + y * self.righe.passo)).map_err(|e| e.to_string())?;
                f.read_exact(&mut r).map_err(|_| "file LIF troncato")?;
                grezzo.extend_from_slice(&r);
            }
        }
        let valore = |y: usize, x: usize| -> &[u8] {
            let o = y * riga as usize + x * self.x.passo as usize;
            &grezzo[o..o + bpc]
        };
        let dati: Vec<u16> = if canale.decimale {
            let v: Vec<f32> = (0..h).flat_map(|y| (0..w).map(move |x| (y, x))).map(|(y, x)| f32::from_le_bytes(valore(y, x).try_into().unwrap_or([0; 4]))).collect();
            let alto = v.iter().copied().filter(|x| x.is_finite()).fold(0.0f32, f32::max);
            let k = if alto > 0.0 && alto <= 1.0 { 65535.0 } else { 1.0 };
            v.iter().map(|x| if x.is_finite() { (x * k).clamp(0.0, 65535.0) as u16 } else { 0 }).collect()
        } else {
            let mut d = Vec::with_capacity(w * h);
            for y in 0..h {
                for x in 0..w {
                    let b = valore(y, x);
                    d.push(match bpc {
                        1 => u16::from(b[0]),
                        2 => u16::from_le_bytes([b[0], b[1]]),
                        _ => (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 16) as u16,
                    });
                }
            }
            d
        };
        Ok(Piano { dati, scala: if bpc == 1 { 255 } else { 65535 } })
    }
}

#[cfg(test)]
pub(crate) mod prove {
    use super::*;

    fn u16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    /// Un LIF con due immagini: «Serie1», 16 bit, 2 canali (DAPI blu, GFP verde) × 3 piani Z × 2 istanti,
    /// 6×4 pixel, 0,25 µm per pixel; e «Cartella/XZ», una scansione XZ a 8 bit di 5×3. Il valore di un
    /// pixel dice dove sta: canale·1000 + t·100 + z·10 + x (+5 sulla seconda riga).
    pub fn lif(dir: &Path) -> std::path::PathBuf {
        let (w, h, z, t) = (6usize, 4usize, 3usize, 2usize);
        let piano = w * h * 2;
        let mut dati1: Vec<u8> = Vec::new();
        // Ordine: X, Y, canali, Z, T.
        for ti in 0..t {
            for zi in 0..z {
                for c in 0..2 {
                    for y in 0..h {
                        for x in 0..w {
                            let v = (c * 1000 + ti * 100 + zi * 10 + x + if y == 1 { 5 } else { 0 }) as u16;
                            dati1.extend(v.to_le_bytes());
                        }
                    }
                }
            }
        }
        // XZ: X 5, Z 3, canale unico, a 8 bit.
        let dati2: Vec<u8> = (0..3u8).flat_map(|zz| (0..5u8).map(move |x| zz * 10 + x)).collect();
        let canale = |inc: usize, lut: &str, dye: &str| {
            format!(
                "<ChannelDescription DataType=\"0\" ChannelTag=\"0\" Resolution=\"12\" NameOfMeasuredQuantity=\"\" Min=\"0\" Max=\"4095\" Unit=\"\" LUTName=\"{lut}\" IsLUTInverted=\"0\" BytesInc=\"{inc}\" BitInc=\"0\"><ChannelProperty><Key>DyeName</Key><Value>Leica/{dye}</Value></ChannelProperty></ChannelDescription>"
            )
        };
        let dim = |id: u32, n: usize, len: f64, unit: &str, inc: usize| format!("<DimensionDescription DimID=\"{id}\" NumberOfElements=\"{n}\" Origin=\"0\" Length=\"{len:e}\" Unit=\"{unit}\" BitInc=\"0\" BytesInc=\"{inc}\"/>");
        let xml = format!(
            "<LMSDataContainerHeader Version=\"2\"><Element Name=\"progetto\" Visibility=\"1\"><Data><Experiment/></Data><Memory Size=\"0\" MemoryBlockID=\"MemBlock_1\"/><Children>\
             <Element Name=\"Serie1\"><Data><Image><ImageDescription><Channels>{}{}</Channels><Dimensions>{}{}{}{}</Dimensions></ImageDescription>\
             <Attachment Name=\"HardwareSetting\"><ATLConfocalSettingDefinition ObjectiveName=\"HC PL APO   63x/1.40 OIL\"/></Attachment></Image></Data>\
             <Memory Size=\"{}\" MemoryBlockID=\"MemBlock_2\"/><Children/></Element>\
             <Element Name=\"Cartella\"><Data><Experiment/></Data><Memory Size=\"0\" MemoryBlockID=\"MemBlock_3\"/><Children>\
             <Element Name=\"XZ\"><Data><Image><ImageDescription><Channels><ChannelDescription DataType=\"0\" ChannelTag=\"0\" Resolution=\"8\" LUTName=\"Gray\" BytesInc=\"0\"/></Channels><Dimensions>{}{}</Dimensions></ImageDescription></Image></Data>\
             <Memory Size=\"{}\" MemoryBlockID=\"MemBlock_4\"/><Children/></Element></Children></Element>\
             </Children></Element></LMSDataContainerHeader>",
            canale(0, "Blue", "DAPI"),
            canale(w * h * 2, "Green", "GFP"),
            dim(1, w, 0.25e-6 * (w - 1) as f64, "m", 2),
            dim(2, h, 0.25e-6 * (h - 1) as f64, "m", w * 2),
            dim(3, z, 1.0e-6 * (z - 1) as f64, "m", piano * 2),
            dim(4, t, 30.0, "s", piano * 2 * z),
            dati1.len(),
            dim(1, 5, 4.0, "um", 1),
            dim(3, 3, 2.0, "um", 5),
            dati2.len()
        );
        let mut file = Vec::new();
        let x = u16le(&xml);
        file.extend(0x70u32.to_le_bytes());
        file.extend((x.len() as u32 + 5).to_le_bytes());
        file.push(0x2A);
        file.extend((x.len() as u32 / 2).to_le_bytes());
        file.extend(x);
        for (nome, dati) in [("MemBlock_1", &Vec::new()), ("MemBlock_2", &dati1), ("MemBlock_3", &Vec::new()), ("MemBlock_4", &dati2)] {
            let n = u16le(nome);
            file.extend(0x70u32.to_le_bytes());
            file.extend(((n.len() + 14) as u32).to_le_bytes());
            file.push(0x2A);
            file.extend((dati.len() as u64).to_le_bytes());
            file.push(0x2A);
            file.extend((n.len() as u32 / 2).to_le_bytes());
            file.extend(n);
            file.extend(dati.iter());
        }
        let p = dir.join("progetto.lif");
        std::fs::write(&p, file).unwrap();
        p
    }

    #[test]
    fn un_lif_si_legge_con_le_sue_immagini() {
        let dir = std::env::temp_dir().join(format!("cedro-lif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = lif(&dir);
        assert!(riconosci(&std::fs::read(&p).unwrap()[..16]));
        let l = Lif::open(&p).unwrap();
        assert_eq!(l.immagini.iter().map(|i| i.nome.as_str()).collect::<Vec<_>>(), ["Serie1", "Cartella/XZ"]);
        let a = &l.immagini[0];
        assert_eq!((a.larghezza, a.altezza, a.z, a.t(), a.bit), (6, 4, 3, 2, 12));
        assert_eq!(a.canali, [("DAPI".to_string(), Some((0, 0, 255))), ("GFP".to_string(), Some((0, 255, 0)))]);
        assert!((a.um_per_px.unwrap() - 0.25).abs() < 1e-9);
        assert!((a.um_z.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(a.obiettivo.as_deref(), Some("HC PL APO 63x/1.40 OIL"));
        let pz = a.piano(&p, 1, 2, 1).unwrap();
        assert_eq!((pz.dati[0], pz.dati[3], pz.dati[6 + 3], pz.scala), (1120, 1123, 1128, 65535));
        let b = &l.immagini[1];
        assert_eq!((b.larghezza, b.altezza, b.z, b.t(), b.bit), (5, 3, 1, 1, 8));
        assert_eq!(b.um_per_px, Some(1.0));
        let pb = b.piano(&p, 0, 0, 0).unwrap();
        assert_eq!((pb.dati[4], pb.dati[5 + 2], pb.scala), (4, 12, 255));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prova su un file vero (`CEDRO_LIF=file.lif`): scrive i pixel di ogni immagine in ordine
    /// T, Z, C, Y, X in `file.lif.<n>.u8` e stampa le misure.
    #[test]
    #[ignore]
    fn file_vero() {
        let Ok(p) = std::env::var("CEDRO_LIF") else { return };
        let p = std::path::PathBuf::from(p);
        let l = Lif::open(&p).unwrap();
        for (k, i) in l.immagini.iter().enumerate() {
            println!("{k}: {} {}x{} z={} t={} altri={:?} canali={:?} bit={} um={:?} umz={:?} obiettivo={:?}", i.nome, i.larghezza, i.altezza, i.z, i.t(), i.altri, i.canali, i.bit, i.um_per_px, i.um_z, i.obiettivo);
            let mut out = Vec::new();
            for t in 0..i.t() {
                for z in 0..i.z {
                    for c in 0..i.canali.len() {
                        out.extend(i.piano(&p, c, z, t).unwrap().dati.iter().flat_map(|v| v.to_le_bytes()));
                    }
                }
            }
            std::fs::write(format!("{}.{k}.u16", p.display()), out).unwrap();
        }
    }
}
