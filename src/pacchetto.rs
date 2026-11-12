//! Il pacchetto dei dati: file e cartelle in uno ZIP «BagIt» (RFC 8493, quello che accettano Zenodo,
//! Dataverse e gli archivi delle biblioteche) con l'impronta SHA-256 di ogni file, la data, quanti sono
//! e quanto pesano, più un LEGGIMI che spiega come controllarli. Chi lo riceve lo verifica con Cedro
//! (o con `sha256sum -c`): un file cambiato anche di un bit si vede subito. Per la firma con la
//! chiavetta lo ZIP va in «Firma digitale» come qualsiasi documento.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Le estensioni dei file di testo (tabelle, sequenze, misure), che compressi pesano molto meno; il resto
/// (immagini, ZIP, video, file dei microscopi) resta com'è, così anche un pacchetto di gigabyte si fa in fretta.
const DI_TESTO: &[&str] = &[
    "txt", "csv", "tsv", "json", "xml", "md", "html", "htm", "log", "fasta", "fa", "fna", "faa", "fastq", "fq", "sam", "vcf", "gff", "gtf", "bed", "pdb", "mzml", "svg", "rtf", "tex", "r", "py", "ipynb",
];

/// Quello che è entrato nel pacchetto.
#[derive(Debug)]
pub struct Fatto {
    pub file: usize,
    pub byte: u64,
    /// L'impronta SHA-256 dello ZIP intero, da citare nell'articolo o nella mail.
    pub impronta: String,
}

/// I file da mettere: i file come sono, le cartelle con tutto quello che c'è dentro (il nome della
/// cartella resta). Ordinati per nome, così due pacchetti degli stessi dati sono uguali.
fn raccogli(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>, String> {
    fn giu(dir: &Path, rel: &str, out: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
        let mut voci: Vec<_> = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.filter_map(Result::ok).collect();
        voci.sort_by_key(|e| e.file_name());
        for e in voci {
            let nome = e.file_name().to_string_lossy().into_owned();
            let p = e.path();
            let tipo = e.file_type().map_err(|e| e.to_string())?;
            if tipo.is_dir() {
                giu(&p, &format!("{rel}/{nome}"), out)?;
            } else if tipo.is_file() || (tipo.is_symlink() && p.is_file()) {
                out.push((p, format!("{rel}/{nome}")));
            } else if tipo.is_symlink() && p.is_dir() {
                // Seguirlo potrebbe girare in tondo; saltarlo perderebbe dati senza dirlo.
                return Err(format!("{rel}/{nome}: è un collegamento a una cartella, metti la cartella vera"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "dati".into());
        if p.is_dir() {
            giu(p, &nome, &mut out)?;
        } else if p.is_file() {
            out.push((p.clone(), nome));
        } else {
            return Err(format!("{nome}: non trovato"));
        }
    }
    // Due file con lo stesso nome da cartelle diverse: il secondo diventa «nome (2).ext».
    // Anche un file che si chiama come una cartella di un altro (dati e dati/x.csv) cambia nome:
    // nello ZIP non possono stare tutti e due.
    let cartelle: std::collections::HashSet<String> =
        out.iter().flat_map(|(_, r)| r.match_indices('/').map(|(i, _)| r[..i].to_lowercase()).collect::<Vec<_>>()).collect();
    let mut visti = std::collections::HashSet::new();
    for (_, rel) in out.iter_mut() {
        let base = rel.clone();
        let mut n = 2;
        while cartelle.contains(&rel.to_lowercase()) || !visti.insert(rel.to_lowercase()) {
            *rel = match base.rsplit_once('.').filter(|(s, _)| !s.ends_with('/')) {
                Some((s, e)) => format!("{s} ({n}).{e}"),
                None => format!("{base} ({n})"),
            };
            n += 1;
        }
    }
    if out.is_empty() {
        return Err("nessun file da mettere nel pacchetto".into());
    }
    Ok(out)
}

/// Il nome di un file nel manifesto: BagIt vuole `%`, a capo e ritorno a capo scritti in codice.
fn nel_manifesto(rel: &str) -> String {
    rel.replace('%', "%25").replace('\n', "%0A").replace('\r', "%0D")
}

fn da_manifesto(rel: &str) -> String {
    rel.replace("%0A", "\n").replace("%0a", "\n").replace("%0D", "\r").replace("%0d", "\r").replace("%25", "%")
}

fn esadecimale(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn oggi() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (y, m, d) = crate::firma::asn1::civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Fa il pacchetto `out` (uno ZIP) con i file e le cartelle dati. `nome` è la cartella dentro lo ZIP.
pub fn crea(paths: &[PathBuf], out: &Path, nome: &str, avanzamento: &dyn Fn(f32)) -> Result<Fatto, String> {
    let voci = raccogli(paths)?;
    let totale: u64 = voci.iter().filter_map(|(p, _)| fs::metadata(p).ok()).map(|m| m.len()).sum();
    let fatto = (|| -> Result<(u64, Vec<(String, String)>), String> {
        let file = fs::File::create(out).map_err(|e| e.to_string())?;
        let mut zw = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let mut manifesto = Vec::new();
        let mut letti = 0u64;
        let mut buf = vec![0u8; 1 << 20];
        for (path, rel) in &voci {
            let ext = Path::new(rel).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let dim = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let metodo = if DI_TESTO.contains(&ext.as_str()) { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
            let livello = (metodo == zip::CompressionMethod::Deflated).then_some(1);
            let opz = zip::write::SimpleFileOptions::default().compression_method(metodo).compression_level(livello).large_file(dim >= u32::MAX as u64);
            zw.start_file(format!("{nome}/data/{rel}"), opz).map_err(|e| e.to_string())?;
            let mut f = fs::File::open(path).map_err(|e| format!("{rel}: {e}"))?;
            let mut h = Sha256::new();
            loop {
                let n = f.read(&mut buf).map_err(|e| format!("{rel}: {e}"))?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                zw.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                letti += n as u64;
                if totale > 0 {
                    avanzamento(letti as f32 / totale as f32 * 0.97);
                }
            }
            manifesto.push((esadecimale(&h.finalize()), rel.clone()));
        }
        // I file della «busta»: cosa c'è, le impronte e come controllarle.
        let righe = |v: &[(String, String)], pref: &str| v.iter().map(|(h, r)| format!("{h}  {pref}{}\n", nel_manifesto(r))).collect::<String>();
        let bagit = "BagIt-Version: 1.0\nTag-File-Character-Encoding: UTF-8\n".to_string();
        let info = format!(
            "Bagging-Date: {}\nPayload-Oxum: {letti}.{}\nBag-Software-Agent: Cedro {} (https://github.com/realfulvio/cedro)\nExternal-Description: {nome}\n",
            oggi(),
            voci.len(),
            env!("CARGO_PKG_VERSION")
        );
        let man = righe(&manifesto, "data/");
        let leggimi = format!(
            "Pacchetto di dati «{nome}» fatto con Cedro il {}.\n\
             {} file in data/, {} byte in tutto.\n\n\
             manifest-sha256.txt ha l'impronta SHA-256 di ogni file: se un file cambia anche di un solo\n\
             bit, l'impronta non torna più. Per controllare:\n\
             - Cedro: ruota sullo ZIP, Strumenti > Verifica;\n\
             - Linux e Mac: nella cartella del pacchetto, sha256sum -c manifest-sha256.txt\n\
               (sul Mac: shasum -a 256 -c manifest-sha256.txt);\n\
             - Windows (PowerShell): Get-FileHash -Algorithm SHA256 data\\nome-del-file\n\n\
             Il formato è BagIt (RFC 8493): lo leggono Zenodo, Dataverse e gli archivi digitali.\n\n\
             ---\n\n\
             Data package «{nome}» made with Cedro on {}. {} files in data/, {} bytes.\n\
             manifest-sha256.txt holds the SHA-256 of every file (check with sha256sum -c manifest-sha256.txt).\n\
             Format: BagIt (RFC 8493).\n",
            oggi(),
            voci.len(),
            letti,
            oggi(),
            voci.len(),
            letti
        );
        let busta = [("bagit.txt", bagit), ("bag-info.txt", info), ("manifest-sha256.txt", man), ("LEGGIMI.txt", leggimi)];
        let opz = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut tag = Vec::new();
        for (n, testo) in &busta {
            zw.start_file(format!("{nome}/{n}"), opz).map_err(|e| e.to_string())?;
            zw.write_all(testo.as_bytes()).map_err(|e| e.to_string())?;
            tag.push((esadecimale(&Sha256::digest(testo.as_bytes())), n.to_string()));
        }
        zw.start_file(format!("{nome}/tagmanifest-sha256.txt"), opz).map_err(|e| e.to_string())?;
        zw.write_all(righe(&tag, "").as_bytes()).map_err(|e| e.to_string())?;
        let mut inner = zw.finish().map_err(|e| e.to_string())?;
        inner.flush().map_err(|e| e.to_string())?;
        Ok((letti, manifesto))
    })();
    let (byte, manifesto) = fatto.inspect_err(|_| {
        let _ = fs::remove_file(out);
    })?;
    // L'impronta dello ZIP intero, per chi lo cita.
    let mut h = Sha256::new();
    let mut f = fs::File::open(out).map_err(|e| e.to_string())?;
    std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;
    avanzamento(1.0);
    Ok(Fatto { file: manifesto.len(), byte, impronta: esadecimale(&h.finalize()) })
}

/// Com'è andata la verifica di un pacchetto.
#[derive(Debug, Default)]
pub struct Verifica {
    pub integri: usize,
    pub diversi: Vec<String>,
    pub mancanti: Vec<String>,
    /// File in data/ che il manifesto non nomina (aggiunti dopo).
    pub in_piu: Vec<String>,
}

impl Verifica {
    pub fn tutto_bene(&self) -> bool {
        self.diversi.is_empty() && self.mancanti.is_empty() && self.in_piu.is_empty()
    }

    /// Il messaggio per chi lo legge: quanti file vanno, e i nomi di quelli che non vanno.
    pub fn messaggio(&self) -> String {
        if self.tutto_bene() {
            return format!("Pacchetto integro: {} file, tutte le impronte SHA-256 tornano.", self.integri);
        }
        let mut s = format!("Pacchetto NON integro: {} file a posto.", self.integri);
        let elenco = |v: &[String]| {
            let mut t = v.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
            if v.len() > 5 {
                t.push_str(&format!(" e altri {}", v.len() - 5));
            }
            t
        };
        if !self.diversi.is_empty() {
            s.push_str(&format!(" Cambiati: {}.", elenco(&self.diversi)));
        }
        if !self.mancanti.is_empty() {
            s.push_str(&format!(" Mancano: {}.", elenco(&self.mancanti)));
        }
        if !self.in_piu.is_empty() {
            s.push_str(&format!(" Aggiunti dopo: {}.", elenco(&self.in_piu)));
        }
        s
    }
}

/// Controlla un pacchetto ZIP: ogni file di data/ deve avere l'impronta scritta nel manifesto.
/// Vanno anche gli ZIP con un `SHA256SUMS` o un `manifest-sha256.txt` fatti da altri programmi.
pub fn verifica(zip_path: &Path, avanzamento: &dyn Fn(f32)) -> Result<Verifica, String> {
    let f = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("non è uno ZIP leggibile: {e}"))?;
    let nomi: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect();
    let manifesto = nomi
        .iter()
        .filter(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            base == "manifest-sha256.txt" || base.eq_ignore_ascii_case("SHA256SUMS") || base.eq_ignore_ascii_case("SHA256SUMS.txt")
        })
        .min_by_key(|n| n.matches('/').count())
        .cloned()
        .ok_or("questo ZIP non è un pacchetto di dati: manca l'elenco delle impronte (manifest-sha256.txt)")?;
    let radice = manifesto.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let mut testo = String::new();
    z.by_name(&manifesto).map_err(|e| e.to_string())?.read_to_string(&mut testo).map_err(|e| e.to_string())?;
    let mut attesi: Vec<(String, String)> = Vec::new();
    for riga in testo.lines() {
        let riga = riga.trim_end_matches('\r');
        let Some((h, rel)) = riga.split_once(char::is_whitespace) else { continue };
        let rel = da_manifesto(rel.trim_start().trim_start_matches('*'));
        let rel = rel.strip_prefix("./").unwrap_or(&rel).to_string();
        if h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()) {
            attesi.push((h.to_ascii_lowercase(), rel));
        }
    }
    let bagit = manifesto.ends_with("manifest-sha256.txt");
    if bagit {
        // In un BagIt il manifesto elenca solo i dati, e ognuno una volta.
        let mut visti = std::collections::HashSet::new();
        attesi.retain(|(_, rel)| rel.starts_with("data/") && visti.insert(rel.clone()));
    }
    if attesi.is_empty() {
        return Err("l'elenco delle impronte è vuoto".into());
    }
    let mut v = Verifica::default();
    let mut buf = vec![0u8; 1 << 20];
    for (i, (h, rel)) in attesi.iter().enumerate() {
        avanzamento(i as f32 / attesi.len() as f32);
        let nome = format!("{radice}{rel}");
        let Ok(mut voce) = z.by_name(&nome) else {
            v.mancanti.push(rel.clone());
            continue;
        };
        let mut hs = Sha256::new();
        let mut rotto = false;
        loop {
            match voce.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => hs.update(&buf[..n]),
                // Un file rovinato dentro lo ZIP è un file cambiato: si va avanti con gli altri.
                Err(_) => {
                    rotto = true;
                    break;
                }
            }
        }
        if !rotto && esadecimale(&hs.finalize()) == *h {
            v.integri += 1;
        } else {
            v.diversi.push(rel.clone());
        }
    }
    // Il tagmanifest protegge il manifesto stesso: chi cambia un dato e la sua riga si vede qui.
    if bagit {
        let mut testo = String::new();
        if z.by_name(&format!("{radice}tagmanifest-sha256.txt")).is_ok_and(|mut f| f.read_to_string(&mut testo).is_ok()) {
            for riga in testo.lines() {
                let Some((h, rel)) = riga.trim_end_matches('\r').split_once(char::is_whitespace) else { continue };
                let rel = da_manifesto(rel.trim_start());
                let mut dati = Vec::new();
                let ok = z.by_name(&format!("{radice}{rel}")).is_ok_and(|mut f| f.read_to_end(&mut dati).is_ok());
                if !ok || !esadecimale(&Sha256::digest(&dati)).eq_ignore_ascii_case(h) {
                    v.diversi.push(rel);
                }
            }
        }
    }
    // Nei pacchetti BagIt tutto quello che sta in data/ deve stare nel manifesto.
    if bagit {
        let dati = format!("{radice}data/");
        for n in &nomi {
            if let Some(rel) = n.strip_prefix(&radice) {
                if n.starts_with(&dati) && !n.ends_with('/') && !attesi.iter().any(|(_, r)| r == rel) {
                    v.in_piu.push(rel.to_string());
                }
            }
        }
    }
    avanzamento(1.0);
    Ok(v)
}

#[cfg(test)]
mod prove {
    use super::*;

    fn cartella(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cedro-pacchetto-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn il_pacchetto_e_un_bagit_e_si_verifica() {
        let d = cartella("bagit");
        fs::create_dir_all(d.join("esperimento/piastra 1")).unwrap();
        fs::write(d.join("esperimento/piastra 1/misure.csv"), "pozzetto;valore\nA1;0,52\n").unwrap();
        fs::write(d.join("esperimento/note.txt"), "tutto ok").unwrap();
        fs::write(d.join("foto.png"), [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
        let zip_path = d.join("out.zip");
        let f = crea(&[d.join("esperimento"), d.join("foto.png")], &zip_path, "esperimento-pacchetto", &|_| {}).unwrap();
        assert_eq!(f.file, 3);
        assert_eq!(f.impronta.len(), 64);
        let mut z = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        let mut man = String::new();
        z.by_name("esperimento-pacchetto/manifest-sha256.txt").unwrap().read_to_string(&mut man).unwrap();
        // L'impronta di «tutto ok», calcolata a parte.
        assert!(man.contains(&format!("{}  data/esperimento/note.txt", esadecimale(&Sha256::digest(b"tutto ok")))), "{man}");
        assert!(man.contains("  data/esperimento/piastra 1/misure.csv\n") && man.contains("  data/foto.png\n"));
        let mut info = String::new();
        z.by_name("esperimento-pacchetto/bag-info.txt").unwrap().read_to_string(&mut info).unwrap();
        let byte = 8 + 7 + "pozzetto;valore\nA1;0,52\n".len();
        assert!(info.contains(&format!("Payload-Oxum: {byte}.3")), "{info}");
        assert!(z.by_name("esperimento-pacchetto/tagmanifest-sha256.txt").is_ok());
        let v = verifica(&zip_path, &|_| {}).unwrap();
        assert!(v.tutto_bene() && v.integri == 3, "{v:?}");
        assert!(v.messaggio().starts_with("Pacchetto integro: 3 file"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn un_file_cambiato_o_aggiunto_si_vede() {
        let d = cartella("rotto");
        let zip_path = d.join("rotto.zip");
        {
            let mut zw = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            let buono = esadecimale(&Sha256::digest(b"uno"));
            let altro = esadecimale(&Sha256::digest(b"due"));
            zw.start_file("p/manifest-sha256.txt", o).unwrap();
            zw.write_all(format!("{buono}  data/a.txt\n{altro}  data/b.txt\n{altro}  data/c.txt\n").as_bytes()).unwrap();
            zw.start_file("p/data/a.txt", o).unwrap();
            zw.write_all(b"uno").unwrap();
            zw.start_file("p/data/b.txt", o).unwrap();
            zw.write_all(b"DUE").unwrap();
            zw.start_file("p/data/intruso.txt", o).unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        let v = verifica(&zip_path, &|_| {}).unwrap();
        assert_eq!(v.integri, 1);
        assert_eq!(v.diversi, vec!["data/b.txt"]);
        assert_eq!(v.mancanti, vec!["data/c.txt"]);
        assert_eq!(v.in_piu, vec!["data/intruso.txt"]);
        assert_eq!(v.messaggio(), "Pacchetto NON integro: 1 file a posto. Cambiati: data/b.txt. Mancano: data/c.txt. Aggiunti dopo: data/intruso.txt.");
        // Uno ZIP qualsiasi non è un pacchetto.
        let semplice = d.join("semplice.zip");
        {
            let mut zw = zip::ZipWriter::new(fs::File::create(&semplice).unwrap());
            zw.start_file("a.txt", zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(b"a").unwrap();
            zw.finish().unwrap();
        }
        assert!(verifica(&semplice, &|_| {}).unwrap_err().contains("non è un pacchetto"));
        let _ = fs::remove_dir_all(&d);
    }
}
