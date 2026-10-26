//! Il pannello Conversioni: categorie, formati e conversione di una coda di file.

use std::path::{Path, PathBuf};

use crate::engine::{self, Kind, Mode, Slice};

/// Una voce del menu a sinistra del pannello.
pub struct Category {
    pub kind: Kind,
    pub name: &'static str,
    pub hint: &'static str,
    pub accepts: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Image, name: "Immagini", hint: "Cambia formato, comprimi e ruota le foto.", accepts: "PNG, JPEG, WebP, BMP, GIF, TIFF" },
    Category { kind: Kind::Doc, name: "Documenti", hint: "Da Word, ODT e RTF a PDF.", accepts: "DOCX, DOC, ODT, RTF" },
    Category { kind: Kind::Pdf, name: "PDF", hint: "Da PDF a Word modificabile o a immagini.", accepts: "PDF" },
    Category { kind: Kind::Text, name: "Testo", hint: "TXT, Markdown, CSV, JSON, HTML e XML.", accepts: "TXT, MD, CSV, JSON, HTML, XML" },
    Category { kind: Kind::Audio, name: "Audio", hint: "WAV, MP3, OGG, M4A, AAC e FLAC.", accepts: "WAV, MP3, OGG, M4A, AAC, FLAC" },
    Category { kind: Kind::Video, name: "Video", hint: "MP4, WebM, MOV, MKV e AVI.", accepts: "MP4, WebM, MOV, MKV, AVI" },
];

/// I formati di uscita di una categoria.
pub fn formats(kind: Kind) -> &'static [Slice] {
    engine::slices(kind, Mode::Convert)
}

/// Gli strumenti di una categoria (comprimi, ruota, OCR…). Vuoto se non ce ne sono.
pub fn tools(kind: Kind) -> &'static [Slice] {
    let list = engine::slices(kind, Mode::Tools);
    if list.iter().any(|s| s.id == "none") {
        &[]
    } else {
        list
    }
}

/// I file di `paths` che appartengono alla categoria.
pub fn belonging(kind: Kind, paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().filter(|p| engine::kind_of(p) == kind).cloned().collect()
}

/// Esegue `action` su un file. Con `dest` le copie nuove finiscono in quella cartella
/// invece che accanto all'originale; il nome non ne sovrascrive mai un altro.
pub fn convert_one(path: &Path, action: &str, quality: u8, dest: Option<&Path>) -> Result<String, String> {
    engine::perform_into(path, action, quality, dest).map(|out| out.message).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn every_category_has_formats() {
        for c in CATEGORIES {
            assert!(!formats(c.kind).is_empty(), "{}", c.name);
        }
    }

    #[test]
    fn belonging_filters_by_kind() {
        let files = vec![PathBuf::from("a.png"), PathBuf::from("b.pdf"), PathBuf::from("c.jpg")];
        assert_eq!(belonging(Kind::Image, &files).len(), 2);
        assert_eq!(belonging(Kind::Pdf, &files), vec![PathBuf::from("b.pdf")]);
    }

    #[test]
    fn tools_hide_the_placeholder() {
        assert!(tools(Kind::Video).is_empty());
        assert!(!tools(Kind::Image).is_empty());
    }

    #[test]
    fn output_goes_to_the_chosen_folder() {
        let base = std::env::temp_dir().join(format!("cedro-conv-{}", std::process::id()));
        let dest = base.join("fuori");
        fs::create_dir_all(&base).unwrap();
        let src = base.join("nota.txt");
        fs::write(&src, "ciao\nmondo\n").unwrap();
        convert_one(&src, "md", 82, Some(&dest)).unwrap();
        assert!(dest.join("nota.md").is_file());
        assert!(!base.join("nota.md").exists());
        assert!(src.is_file());
        // Una seconda conversione non sovrascrive: il nome prende il -2.
        convert_one(&src, "md", 82, Some(&dest)).unwrap();
        assert!(dest.join("nota-2.md").is_file());
        let _ = fs::remove_dir_all(&base);
    }
}
