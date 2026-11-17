//! DICOMDIR: l'indice dei CD e delle chiavette della radiologia. Elenca pazienti, studi, serie e
//! immagini, e per ogni immagine il file (spesso senza estensione, in cartelle come `DICOM\ST000\SE000`).
//! Qui si legge l'indice e si trovano i file nell'ordine giusto: paziente, studio, serie, immagine.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{find, items_with_offsets, tag, text, DicomFile, Element};

/// La classe del DICOMDIR (Media Storage Directory Storage).
pub const SOP_DICOMDIR: &str = "1.2.840.10008.1.3.10";

const RECORDS: u32 = tag(0x0004, 0x1220);
const ROOT: u32 = tag(0x0004, 0x1200);
const NEXT: u32 = tag(0x0004, 0x1400);
const IN_USE: u32 = tag(0x0004, 0x1410);
const LOWER: u32 = tag(0x0004, 0x1420);
const TYPE: u32 = tag(0x0004, 0x1430);
const FILE_ID: u32 = tag(0x0004, 0x1500);

/// Quello che c'è sul CD secondo l'indice.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Indice {
    /// I file delle immagini che esistono, in ordine.
    pub immagini: Vec<PathBuf>,
    pub pazienti: usize,
    pub studi: usize,
    pub serie: usize,
    /// File indicati dall'indice che sul disco non ci sono (CD copiato a metà).
    pub mancanti: usize,
    /// Oggetti che non sono immagini (referti strutturati, stati di presentazione…).
    pub altri: usize,
}

impl Indice {
    /// La frase per chi ha aperto il CD (corta: sta nella riga degli avvisi).
    pub fn frase(&self) -> String {
        let n = |v: usize, uno: &str, tanti: &str| format!("{v} {}", crate::i18n::tr(if v == 1 { uno } else { tanti }));
        let mut s = format!("{}: {} · {}", crate::i18n::tr("CD DICOM"), n(self.immagini.len(), "immagine", "immagini"), n(self.pazienti, "paziente", "pazienti"));
        if self.mancanti > 0 {
            s.push_str(&format!(" · {} {}", self.mancanti, crate::i18n::tr("mancanti")));
        }
        s
    }

    fn unisci(&mut self, altro: Indice) {
        self.immagini.extend(altro.immagini);
        self.pazienti += altro.pazienti;
        self.studi += altro.studi;
        self.serie += altro.serie;
        self.mancanti += altro.mancanti;
        self.altri += altro.altri;
    }
}

/// Il nome di un file come lo scrive un CD: senza il «;1» della ISO 9660 e senza il punto finale.
fn nome_cd(s: &str) -> &str {
    let s = s.split(';').next().unwrap_or(s);
    s.strip_suffix('.').unwrap_or(s)
}

/// Vero se il file è un DICOMDIR: lo standard vuole proprio questo nome (sul CD anche «DICOMDIR;1»).
/// Solo dal nome: le cartelle con migliaia di file non si aprono uno per uno per saperlo.
pub fn is_dicomdir(path: &Path) -> bool {
    let nome = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    nome_cd(nome).eq_ignore_ascii_case("DICOMDIR") && path.is_file()
}

/// Un UL di un record. Lo standard vuole il DICOMDIR in little endian, ma qualcuno lo scrive big endian.
fn ul(el: &[Element], t: u32, big: bool) -> u32 {
    let b = find(el, t).and_then(|e| e.bytes().get(0..4)).map(|b| [b[0], b[1], b[2], b[3]]).unwrap_or([0; 4]);
    if big {
        u32::from_be_bytes(b)
    } else {
        u32::from_le_bytes(b)
    }
}

/// Il file indicato da `DIR1\DIR2\FILE`, cercato senza badare a maiuscole e minuscole: un CD letto su
/// Linux mostra spesso i nomi in minuscolo, o con il «;1» in fondo.
fn trova(base: &Path, parti: &[&str]) -> Option<PathBuf> {
    let mut p = base.to_path_buf();
    for parte in parti {
        let diretto = p.join(parte);
        if diretto.exists() {
            p = diretto;
            continue;
        }
        let voce = std::fs::read_dir(&p).ok()?.flatten().find(|e| nome_cd(&e.file_name().to_string_lossy()).eq_ignore_ascii_case(parte))?;
        p = voce.path();
    }
    p.is_file().then_some(p)
}

/// Legge un DICOMDIR: i file delle immagini nell'ordine dell'indice (paziente, studio, serie, immagine),
/// seguendo i collegamenti tra i record; i record che l'indice non collega vengono dopo, nell'ordine del file.
pub fn leggi(path: &Path) -> Result<Indice, String> {
    let bytes = crate::input_file::read(path).map_err(|e| e.to_string())?;
    let records = items_with_offsets(&bytes, RECORDS)?;
    if records.is_empty() {
        return Err(crate::i18n::tr_string("il DICOMDIR non elenca nessun file"));
    }
    let f = DicomFile::read(&bytes)?;
    let big = f.syntax == super::Syntax::ExplicitBE;
    let radice = ul(&f.body, ROOT, big);
    let posto: HashMap<u64, usize> = records.iter().enumerate().map(|(i, (o, _))| (*o, i)).collect();
    let mut ordine = Vec::with_capacity(records.len());
    let mut visti = HashSet::new();
    // Una catena di record fratelli, scendendo in ognuno: profondità limitata, i giri in tondo si fermano.
    fn catena(da: u32, liv: u32, big: bool, records: &[(u64, Vec<Element>)], posto: &HashMap<u64, usize>, visti: &mut HashSet<usize>, ordine: &mut Vec<usize>) {
        let mut cur = da;
        while cur != 0 && liv < 16 {
            let Some(&i) = posto.get(&(cur as u64)) else { return };
            if !visti.insert(i) {
                return;
            }
            ordine.push(i);
            let sotto = ul(&records[i].1, LOWER, big);
            if sotto != 0 {
                catena(sotto, liv + 1, big, records, posto, visti, ordine);
            }
            cur = ul(&records[i].1, NEXT, big);
        }
    }
    catena(radice, 0, big, &records, &posto, &mut visti, &mut ordine);
    ordine.extend((0..records.len()).filter(|i| !visti.contains(i)));

    let base = path.parent().unwrap_or(Path::new("."));
    let mut out = Indice::default();
    let mut gia = HashSet::new();
    for i in ordine {
        let el = &records[i].1;
        // Un record cancellato (flag 0) resta nel file ma non conta.
        if find(el, IN_USE).and_then(|e| e.bytes().get(0..2)).is_some_and(|b| b == [0, 0]) {
            continue;
        }
        let tipo = find(el, TYPE).map(|e| text(e.bytes()).to_ascii_uppercase()).unwrap_or_default();
        match tipo.as_str() {
            "PATIENT" => out.pazienti += 1,
            "STUDY" => out.studi += 1,
            "SERIES" => out.serie += 1,
            _ => {}
        }
        let Some(id) = find(el, FILE_ID).map(|e| text(e.bytes())).filter(|s| !s.is_empty()) else { continue };
        if !matches!(tipo.as_str(), "IMAGE" | "RT DOSE" | "") {
            out.altri += 1;
            continue;
        }
        let parti: Vec<&str> = id.split(['\\', '/']).map(str::trim).filter(|s| !s.is_empty() && *s != "." && *s != "..").collect();
        match trova(base, &parti) {
            Some(p) if gia.insert(p.clone()) => out.immagini.push(p),
            Some(_) => {}
            None => out.mancanti += 1,
        }
    }
    Ok(out)
}

/// I file di una cartella (o scelti uno per uno) con i DICOMDIR sciolti: al posto di ogni indice le sue
/// immagini in ordine. Di un CD con l'indice si tengono solo le immagini e i PDF (i referti): il
/// programma di visualizzazione che i CD si portano dietro non serve.
pub fn espandi(file: Vec<PathBuf>) -> (Vec<PathBuf>, Option<Indice>) {
    let indici: Vec<PathBuf> = file.iter().filter(|p| is_dicomdir(p)).cloned().collect();
    if indici.is_empty() {
        return (file, None);
    }
    let mut tot = Indice::default();
    let mut immagini = Vec::new();
    let mut radici = Vec::new();
    for d in &indici {
        match leggi(d) {
            Ok(ix) => {
                radici.push(d.parent().unwrap_or(Path::new(".")).to_path_buf());
                immagini.extend(ix.immagini.iter().cloned());
                tot.unisci(Indice { immagini: Vec::new(), ..ix });
            }
            Err(_) => {}
        }
    }
    tot.immagini = immagini;
    let presi: HashSet<PathBuf> = tot.immagini.iter().cloned().collect();
    let mut out = tot.immagini.clone();
    for f in file {
        if indici.contains(&f) || presi.contains(&f) {
            continue;
        }
        let nel_cd = radici.iter().any(|r| f.starts_with(r));
        if nel_cd && crate::engine::extension(&f) != "pdf" {
            continue;
        }
        out.push(f);
    }
    (out, Some(tot))
}

#[cfg(test)]
mod tests {
    use super::super::prove::{ct, el};
    use super::super::{write_element, Syntax, Value};
    use super::*;

    fn rec(tipo: &str, id: Option<&str>) -> Vec<Element> {
        let mut v = vec![
            Element { tag: NEXT, vr: *b"UL", value: Value::Bytes(vec![0; 4]) },
            Element { tag: IN_USE, vr: *b"US", value: Value::Bytes(vec![0xFF, 0xFF]) },
            Element { tag: LOWER, vr: *b"UL", value: Value::Bytes(vec![0; 4]) },
            el(TYPE, b"CS", tipo.as_bytes()),
        ];
        if let Some(id) = id {
            v.push(el(FILE_ID, b"CS", id.as_bytes()));
        }
        v
    }

    fn scrivi(root: u32, records: &[Vec<Element>]) -> Vec<u8> {
        let mut out = vec![0u8; 128];
        out.extend_from_slice(b"DICM");
        let meta = [el(tag(0x0002, 0x0002), b"UI", SOP_DICOMDIR.as_bytes()), el(tag(0x0002, 0x0010), b"UI", b"1.2.840.10008.1.2.1")];
        for e in &meta {
            write_element(&mut out, e, Syntax::ExplicitLE);
        }
        write_element(&mut out, &Element { tag: ROOT, vr: *b"UL", value: Value::Bytes(root.to_le_bytes().to_vec()) }, Syntax::ExplicitLE);
        write_element(&mut out, &Element { tag: RECORDS, vr: *b"SQ", value: Value::Seq(records.to_vec()) }, Syntax::ExplicitLE);
        out
    }

    fn metti(r: &mut [Element], t: u32, v: u64) {
        r.iter_mut().find(|e| e.tag == t).unwrap().value = Value::Bytes((v as u32).to_le_bytes().to_vec());
    }

    #[test]
    fn segue_i_collegamenti_e_trova_i_file_anche_in_minuscolo() {
        let base = std::env::temp_dir().join(format!("cedro-dicomdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("dicom/st0")).unwrap();
        std::fs::create_dir_all(base.join("Viewer")).unwrap();
        let px = ct(2, 2, &[0, 1, 2, 3], Syntax::ImplicitLE);
        for f in ["dicom/st0/img1", "dicom/st0/img2;1", "dicom/st0/img3"] {
            std::fs::write(base.join(f), &px).unwrap();
        }
        std::fs::write(base.join("Viewer/viewer.exe"), b"MZ").unwrap();
        std::fs::write(base.join("Viewer/logo.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(base.join("referto.pdf"), b"%PDF-1.4").unwrap();
        // Nel file i record sono in disordine: prima le immagini (2 e 1), poi uno SR, la serie, lo studio,
        // il paziente e un'immagine che sul disco non c'è. L'ordine giusto viene dai collegamenti.
        let mut r = vec![
            rec("IMAGE", Some("DICOM\\ST0\\IMG2")),
            rec("IMAGE", Some("DICOM\\ST0\\IMG1")),
            rec("SR DOCUMENT", Some("DICOM\\ST0\\SR1")),
            rec("SERIES", None),
            rec("STUDY", None),
            rec("PATIENT", None),
            rec("IMAGE", Some("DICOM\\ST0\\IMG9")),
            rec("IMAGE", Some("DICOM\\ST0\\IMG3")),
        ];
        let o: Vec<u64> = items_with_offsets(&scrivi(0, &r), RECORDS).unwrap().iter().map(|(o, _)| *o).collect();
        metti(&mut r[5], LOWER, o[4]);
        metti(&mut r[4], LOWER, o[3]);
        metti(&mut r[3], LOWER, o[1]);
        metti(&mut r[1], NEXT, o[0]);
        metti(&mut r[0], NEXT, o[2]);
        metti(&mut r[2], NEXT, o[6]);
        let dd = base.join("DICOMDIR");
        std::fs::write(&dd, scrivi(o[5] as u32, &r)).unwrap();

        let ix = leggi(&dd).unwrap();
        let nomi: Vec<String> = ix.immagini.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(nomi, ["img1", "img2;1", "img3"]);
        assert_eq!((ix.pazienti, ix.studi, ix.serie, ix.mancanti, ix.altri), (1, 1, 1, 1, 1));
        assert!(ix.frase().contains("3"));

        // La cartella intera: le immagini dell'indice e il referto PDF, niente programma del CD.
        let (file, cd) = espandi(crate::cartelle::file_in_cartella(&base, 100));
        let rel: Vec<String> = file.iter().map(|p| p.strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(rel, ["dicom/st0/img1", "dicom/st0/img2;1", "dicom/st0/img3", "referto.pdf"]);
        assert_eq!(cd.unwrap().immagini.len(), 3);
        assert!(is_dicomdir(&dd));
        assert!(!is_dicomdir(&base.join("dicom/st0/img1")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn senza_collegamenti_vale_l_ordine_del_file_e_un_indice_rovinato_non_fa_danni() {
        let base = std::env::temp_dir().join(format!("cedro-dicomdir2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("B"), b"x").unwrap();
        std::fs::write(base.join("A"), b"x").unwrap();
        let dd = base.join("DICOMDIR");
        std::fs::write(&dd, scrivi(0, &[rec("IMAGE", Some("B")), rec("IMAGE", Some("A"))])).unwrap();
        let ix = leggi(&dd).unwrap();
        assert_eq!(ix.immagini, [base.join("B"), base.join("A")]);
        std::fs::write(&dd, &scrivi(0, &[rec("IMAGE", Some("B"))])[..150]).unwrap();
        assert!(leggi(&dd).is_err());
        // Un indice illeggibile non toglie i file: restano quelli della cartella.
        let (file, _) = espandi(vec![dd.clone(), base.join("A")]);
        assert!(file.contains(&base.join("A")));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Prova a mano su DICOMDIR veri: `CEDRO_DICOMDIR=percorso/DICOMDIR cargo test --lib dicomdir_veri -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dicomdir_veri_a_mano() {
        let p = PathBuf::from(std::env::var("CEDRO_DICOMDIR").unwrap());
        let ix = leggi(&p).unwrap();
        let base = p.parent().unwrap();
        println!("{}", ix.immagini.len());
        for f in &ix.immagini {
            println!("{}", f.strip_prefix(base).unwrap().to_string_lossy());
        }
        eprintln!("{} | altri {}", ix.frase(), ix.altri);
    }
}
