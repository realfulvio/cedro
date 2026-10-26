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

/// Una scelta di un'opzione extra (per esempio "150 dpi").
pub struct Choice {
    pub label: &'static str,
    pub value: u32,
}

/// Un'opzione extra del pannello: un titolo e una lista di valori, il primo è quello di partenza.
pub struct Extra {
    pub key: ExtraKey,
    pub title: &'static str,
    pub note: &'static str,
    pub choices: &'static [Choice],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExtraKey {
    /// PDF in JPG: risoluzione delle pagine.
    Dpi,
    /// Immagini: lato più lungo massimo.
    MaxSide,
}

const DPI: Extra = Extra {
    key: ExtraKey::Dpi,
    title: "RISOLUZIONE PAGINE",
    note: "Più dpi, immagini più nitide e più pesanti.",
    choices: &[
        Choice { label: "144 dpi", value: 144 },
        Choice { label: "72 dpi", value: 72 },
        Choice { label: "200 dpi", value: 200 },
        Choice { label: "300 dpi", value: 300 },
    ],
};

const MAX_SIDE: Extra = Extra {
    key: ExtraKey::MaxSide,
    title: "DIMENSIONE",
    note: "Rimpicciolisce solo le immagini più grandi, senza deformarle.",
    choices: &[
        Choice { label: "Originale", value: 0 },
        Choice { label: "3840 px", value: 3840 },
        Choice { label: "1920 px", value: 1920 },
        Choice { label: "1280 px", value: 1280 },
        Choice { label: "800 px", value: 800 },
    ],
};

/// L'opzione extra che vale per questa categoria e questa voce, se c'è.
pub fn extra(kind: Kind, action: &str) -> Option<&'static Extra> {
    match (kind, action) {
        (Kind::Pdf, "jpg") => Some(&DPI),
        (Kind::Image, "ocr") => None,
        (Kind::Image, _) => Some(&MAX_SIDE),
        _ => None,
    }
}

/// Le opzioni del motore per il valore scelto di `extra` (0 = nessuna modifica).
pub fn options_for(extra: Option<&Extra>, value: u32) -> engine::Options {
    let mut opts = engine::Options::default();
    match extra.map(|e| e.key) {
        Some(ExtraKey::Dpi) => opts.dpi = Some(value),
        Some(ExtraKey::MaxSide) if value > 0 => opts.max_side = Some(value),
        _ => {}
    }
    opts
}

/// Esegue `action` su un file. Con `dest` le copie nuove finiscono in quella cartella
/// invece che accanto all'originale; il nome non ne sovrascrive mai un altro.
pub fn convert_one(path: &Path, action: &str, quality: u8, dest: Option<&Path>, opts: &engine::Options) -> Result<String, String> {
    engine::perform_with(path, action, quality, dest, opts).map(|out| out.message).map_err(|e| e.0)
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
    fn extras_exist_only_where_the_engine_uses_them() {
        assert_eq!(extra(Kind::Pdf, "jpg").map(|e| e.key), Some(ExtraKey::Dpi));
        assert!(extra(Kind::Pdf, "docx").is_none());
        assert!(extra(Kind::Image, "ocr").is_none());
        assert_eq!(extra(Kind::Image, "jpeg").map(|e| e.key), Some(ExtraKey::MaxSide));
        assert!(extra(Kind::Video, "mp4").is_none());
        assert_eq!(options_for(extra(Kind::Image, "png"), 0), engine::Options::default());
        assert_eq!(options_for(extra(Kind::Image, "png"), 800).max_side, Some(800));
        assert_eq!(options_for(extra(Kind::Pdf, "jpg"), 300).dpi, Some(300));
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
        convert_one(&src, "md", 82, Some(&dest), &Default::default()).unwrap();
        assert!(dest.join("nota.md").is_file());
        assert!(!base.join("nota.md").exists());
        assert!(src.is_file());
        // Una seconda conversione non sovrascrive: il nome prende il -2.
        convert_one(&src, "md", 82, Some(&dest), &Default::default()).unwrap();
        assert!(dest.join("nota-2.md").is_file());
        let _ = fs::remove_dir_all(&base);
    }
}
