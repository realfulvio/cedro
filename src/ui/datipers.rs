//! Cedro 4 Labs, colonna «Dati personali»: quello che un file dice di chi lo ha fatto, o di chi è il
//! caso, prima ancora di toglierlo. Si legge solo, non si scrive niente: la stessa logica di «Togli i
//! nomi» (etichette dei vetrini, tag dei DICOM, GPS delle foto, autore dei documenti) vista da fuori.

use std::path::Path;

use super::media::SlideInfo;
use crate::engine::Kind;

/// Com'è un file rispetto ai dati personali.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stato {
    /// Non ancora letto.
    #[default]
    NonLetto,
    /// Letto, niente che identifichi.
    Nessuno,
    /// Ci sono campi precisi, elencati in `campi`.
    Campi,
    /// Cedro non sa dirlo (il nome può stare nei pixel o nella voce): lo guarda una persona.
    DaControllare,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dati {
    pub stato: Stato,
    /// I campi trovati, in parole («nome del paziente», «GPS della foto»).
    pub campi: Vec<String>,
    /// Perché è «Da controllare», in una riga.
    pub nota: String,
}

impl Dati {
    fn nessuno() -> Dati {
        Dati { stato: Stato::Nessuno, ..Default::default() }
    }

    fn da_controllare(nota: &str) -> Dati {
        Dati { stato: Stato::DaControllare, campi: Vec::new(), nota: nota.to_string() }
    }

    fn con(campi: Vec<String>) -> Dati {
        if campi.is_empty() {
            Dati::nessuno()
        } else {
            Dati { stato: Stato::Campi, campi, nota: String::new() }
        }
    }

    /// Quanti campi, per la colonna («1 campo», «5 campi»).
    pub fn n(&self) -> usize {
        self.campi.len()
    }
}

/// I tag del DICOM che dicono chi è il paziente (o il proprietario) e chi ha fatto l'esame.
const TAG_DICOM: &[(u16, u16, &str)] = &[
    (0x0010, 0x0010, "nome del paziente"),
    (0x0010, 0x0020, "ID del paziente"),
    (0x0010, 0x0030, "data di nascita"),
    (0x0010, 0x1000, "altri ID del paziente"),
    (0x0010, 0x1040, "indirizzo"),
    (0x0008, 0x0050, "numero di accesso"),
    (0x0008, 0x0080, "istituto"),
    (0x0008, 0x0090, "medico richiedente"),
    (0x0008, 0x1050, "medico che ha eseguito"),
    (0x0008, 0x1070, "operatore"),
];

/// Un DICOM: i tag con un valore e, se il nome può stare nei pixel, un campo da controllare.
pub fn dicom(f: &crate::dicom::DicomFile, burned_in: bool) -> Dati {
    let mut campi: Vec<String> = TAG_DICOM.iter().filter(|(g, e, _)| f.string(crate::dicom::tag(*g, *e)).is_some_and(|s| !s.trim().is_empty())).map(|(_, _, c)| c.to_string()).collect();
    if campi.is_empty() && burned_in {
        return Dati::da_controllare("il nome può essere scritto dentro l'immagine");
    }
    if burned_in {
        campi.push("scritte nei pixel".into());
    }
    Dati::con(campi)
}

/// Un vetrino: etichetta e foto d'insieme (spesso con il codice scritto sopra), e per l'Aperio la
/// descrizione dello scanner. Gli altri scanner senza etichetta né foto restano da controllare.
pub fn vetrino(s: &SlideInfo, aperio: bool) -> Dati {
    let mut campi = Vec::new();
    if s.label {
        campi.push("etichetta con il codice".to_string());
    }
    if s.macro_image {
        campi.push("foto d'insieme del vetrino".to_string());
    }
    if aperio {
        campi.push("descrizione dello scanner (nome file, data, ID)".to_string());
    }
    if campi.is_empty() {
        return Dati::da_controllare("lo scanner può scrivere il nome nei metadati");
    }
    Dati::con(campi)
}

/// Le foto e i documenti: si prova «Togli i nomi» sui byte in memoria e si guarda cosa toglierebbe.
/// Oltre i 48 MB non si legge (la tabella deve restare pronta): «Da controllare».
pub fn file_letto(path: &Path, kind: Kind) -> Option<Dati> {
    const MAX: u64 = 48 * 1024 * 1024;
    let ext = crate::engine::extension(path);
    let foto = matches!(ext.as_str(), "jpg" | "jpeg" | "jfif" | "png" | "webp");
    let doc = crate::anonimizza::gestito(&ext);
    if !(foto || doc) {
        return None;
    }
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX {
        return Some(Dati::da_controllare("file grande: non l'ho letto"));
    }
    let bytes = crate::input_file::read(path).ok()?;
    if foto && matches!(kind, Kind::Image) {
        let (_, t) = crate::pulisci::pulisci(&ext, &bytes)?.ok()?;
        let mut c = Vec::new();
        for (on, nome) in [
            (t.gps, "posizione GPS"),
            (t.fotocamera, "fotocamera"),
            (t.data, "data di scatto"),
            (t.miniatura, "miniatura"),
            (t.xmp, "XMP"),
            (t.iptc, "IPTC"),
            (t.testi, "commenti"),
            (t.coda, "dati dopo l'immagine"),
        ] {
            if on {
                c.push(nome.to_string());
            }
        }
        if c.is_empty() && t.altro_exif {
            c.push("altri dati EXIF".to_string());
        }
        return Some(Dati::con(c));
    }
    if doc {
        let (_, fatto) = crate::anonimizza::pulisci(&ext, &bytes).ok()?;
        return Some(Dati::con(fatto.tolti.iter().map(|s| s.to_string()).collect()));
    }
    None
}

/// Le registrazioni: nessun campo da leggere, ma una voce dice i nomi.
pub fn audio() -> Dati {
    Dati::da_controllare("la voce può dire nomi e dati")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senza_campi_il_file_non_ha_nulla() {
        assert_eq!(Dati::con(Vec::new()).stato, Stato::Nessuno);
        let d = Dati::con(vec!["a".into(), "b".into()]);
        assert_eq!((d.stato, d.n()), (Stato::Campi, 2));
    }

    #[test]
    fn un_vetrino_senza_etichetta_resta_da_controllare() {
        let s = SlideInfo { label: false, macro_image: false, ..Default::default() };
        assert_eq!(vetrino(&s, false).stato, Stato::DaControllare);
        let s = SlideInfo { label: true, macro_image: false, ..Default::default() };
        assert_eq!(vetrino(&s, false).campi, vec!["etichetta con il codice".to_string()]);
        // L'Aperio ha sempre la descrizione dello scanner.
        assert_eq!(vetrino(&s, true).n(), 2);
    }

    #[test]
    fn una_foto_con_gps_lo_dice() {
        // Un JPEG senza EXIF: niente da togliere.
        let dir = std::env::temp_dir().join(format!("cedro-datipers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.jpg");
        image::RgbImage::from_pixel(8, 8, image::Rgb([90, 120, 80])).save(&p).unwrap();
        let d = file_letto(&p, Kind::Image).expect("un jpeg si legge");
        assert_eq!(d.stato, Stato::Nessuno);
        let _ = std::fs::remove_dir_all(dir);
    }
}
