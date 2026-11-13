//! Documenti d'ufficio stampati come li stampa il programma che li ha scritti: Word, ODT, RTF e DOC
//! con tabelle, stili, elenchi, intestazioni e note; Excel, Calc e XLS con le celle formattate, le
//! pagine e le impostazioni di stampa del file. Tutto in Rust, con i caratteri del sistema.

pub mod font;
pub mod pagina;
pub mod pdf;
pub mod numfmt;
pub mod foglio;
pub mod stampa_fogli;
pub mod ods;
pub mod documento;
pub mod docx;
pub mod impagina_doc;
pub mod odt;
pub mod rtf;
pub mod doc;
pub mod scrivi_doc;

/// Le scelte del pannello per i documenti d'ufficio (i valori sono quelli del pannello).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scelte {
    /// La carta: `None` quella del file.
    pub carta: Option<crate::engine::PageSize>,
    /// Fogli di calcolo: solo il foglio attivo (spento: tutti quelli visibili).
    pub solo_attivo: bool,
    /// 0 come il file, 1 tutte le colonne in larghezza, 2 ogni foglio in una pagina, 3 misura vera.
    pub adatta: u8,
    /// 0 come il file, 1 automatico, 2 verticale, 3 orizzontale.
    pub orienta: u8,
    /// 0 come il file, 1 sì, 2 no.
    pub griglia: u8,
    pub ripeti_prima: bool,
    pub piede_nome: bool,
    pub embed: bool,
    pub bookmarks: bool,
    pub pdfa: bool,
}

impl Default for Scelte {
    fn default() -> Self {
        Scelte { carta: None, solo_attivo: false, adatta: 0, orienta: 0, griglia: 0, ripeti_prima: false, piede_nome: false, embed: true, bookmarks: true, pdfa: false }
    }
}

fn carta_pt(p: crate::engine::PageSize) -> (f32, f32) {
    match p {
        crate::engine::PageSize::A4 => (595.0, 842.0),
        crate::engine::PageSize::A3 => (842.0, 1191.0),
        crate::engine::PageSize::Letter => (612.0, 792.0),
    }
}

/// Vero se l'estensione è un foglio di calcolo che Cedro sa stampare.
pub fn is_foglio(ext: &str) -> bool {
    matches!(ext, "xlsx" | "xlsm" | "ods" | "xls")
}

/// Legge un foglio di calcolo per stamparlo.
pub fn leggi_foglio(ext: &str, bytes: &[u8], fonts: &mut font::Fonti) -> Result<foglio::Cartella, String> {
    match ext {
        "xlsx" | "xlsm" => foglio::leggi_xlsx(bytes, fonts),
        "ods" => ods::leggi_ods(bytes),
        "xls" => xls::leggi_xls(bytes, fonts),
        _ => Err(format!("«{ext}» non è un foglio di calcolo")),
    }
}

/// Un foglio di calcolo stampato in PDF come lo stampa il suo programma.
pub fn foglio_pdf(ext: &str, bytes: &[u8], file_name: &str, title: &str, s: &Scelte) -> Result<Vec<u8>, String> {
    let mut fonts = font::Fonti::new();
    let book = leggi_foglio(ext, bytes, &mut fonts)?;
    let o = stampa_fogli::OpzioniFogli {
        solo_attivo: s.solo_attivo,
        adatta: match s.adatta {
            1 => stampa_fogli::Adatta::Larghezza,
            2 => stampa_fogli::Adatta::Pagina,
            3 => stampa_fogli::Adatta::Vera,
            _ => stampa_fogli::Adatta::ComeFile,
        },
        orienta: match s.orienta {
            1 => stampa_fogli::Orienta::Auto,
            2 => stampa_fogli::Orienta::Verticale,
            3 => stampa_fogli::Orienta::Orizzontale,
            _ => stampa_fogli::Orienta::ComeFile,
        },
        carta: s.carta.map(carta_pt),
        griglia: match s.griglia {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        },
        ripeti_prima: s.ripeti_prima,
        piede_nome: s.piede_nome,
        file_name: file_name.to_string(),
    };
    let doc = stampa_fogli::impagina(&book, fonts, &o, title)?;
    if doc.pages.is_empty() {
        return Err("il foglio di calcolo è vuoto: non c'è niente da stampare".into());
    }
    pdf::scrivi(&doc, &pdf::Opzioni { embed: s.embed || s.pdfa, bookmarks: s.bookmarks, pdfa: s.pdfa, links: true })
}
/// I documenti di testo che Cedro mette in pagina da sé.
pub fn is_documento(ext: &str) -> bool {
    matches!(ext, "docx" | "docm" | "dotx" | "dotm" | "odt" | "ott" | "rtf" | "doc" | "dot")
}

pub fn leggi_documento(ext: &str, bytes: &[u8]) -> Result<documento::Documento, String> {
    // Conta il contenuto, non il nome: un RTF salvato come .doc, un .doc rinominato .docx.
    let ext = match tipo_documento(bytes) {
        Some(vero) if vero != tipo_documento_ext(ext) => vero,
        _ => ext,
    };
    match ext {
        "docx" | "docm" | "dotx" | "dotm" => docx::leggi_docx(bytes),
        "odt" | "ott" => odt::leggi_odt(bytes),
        "rtf" => rtf::leggi_rtf(bytes),
        "doc" | "dot" => doc::leggi_doc(bytes),
        _ => Err(format!("«{ext}» non è un documento di testo")),
    }
}

/// Apre un file OLE (vecchi Word ed Excel). Alcuni programmi scrivono una FAT più lunga del file:
/// Word e Excel li aprono lo stesso, qui si allunga il file con settori vuoti e si riprova.
pub(crate) fn apri_ole(bytes: &[u8]) -> std::io::Result<cfb::CompoundFile<std::io::Cursor<std::borrow::Cow<'_, [u8]>>>> {
    use std::borrow::Cow;
    match cfb::CompoundFile::open(std::io::Cursor::new(Cow::Borrowed(bytes))) {
        Err(e) if e.to_string().contains("but file has only") => {
            let voci: usize = e.to_string().split("FAT has ").nth(1).and_then(|r| r.split(' ').next()).and_then(|n| n.parse().ok()).unwrap_or(0);
            let settore = if bytes.get(30) == Some(&12) { 4096 } else { 512 };
            let serve = settore * (voci + 1);
            if serve <= bytes.len() || serve > bytes.len().saturating_mul(8).max(1 << 20) {
                return Err(e);
            }
            let mut v = bytes.to_vec();
            v.resize(serve, 0);
            cfb::CompoundFile::open(std::io::Cursor::new(Cow::Owned(v)))
        }
        r => r,
    }
}

/// Il formato vero di un documento dai suoi primi byte (`None`: non si capisce).
fn tipo_documento(bytes: &[u8]) -> Option<&'static str> {
    let t = bytes.iter().position(|b| !b.is_ascii_whitespace()).map_or(&bytes[..0], |i| &bytes[i..]);
    if t.starts_with(b"{\\rtf") {
        return Some("rtf");
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        // Un Word nuovo protetto da password è chiuso dentro un file OLE: lo dice il lettore DOCX.
        let nome: Vec<u8> = "EncryptionInfo".encode_utf16().flat_map(u16::to_le_bytes).collect();
        return Some(if bytes.windows(nome.len()).any(|w| w == nome.as_slice()) { "docx" } else { "doc" });
    }
    if bytes.starts_with(b"PK") {
        let odt = bytes.get(30..).is_some_and(|r| r.starts_with(b"mimetype") && r.get(8..).is_some_and(|m| m.starts_with(b"application/vnd.oasis.opendocument.text")));
        return Some(if odt { "odt" } else { "docx" });
    }
    None
}

fn tipo_documento_ext(ext: &str) -> &'static str {
    match ext {
        "odt" | "ott" => "odt",
        "rtf" => "rtf",
        "doc" | "dot" => "doc",
        _ => "docx",
    }
}

/// Un documento di testo stampato in PDF come lo stampa Word.
pub fn documento_pdf(ext: &str, bytes: &[u8], title: &str, s: &Scelte) -> Result<Vec<u8>, String> {
    let mut doc = leggi_documento(ext, bytes)?;
    if let Some(c) = s.carta {
        let (w, h) = carta_pt(c);
        for sez in &mut doc.sezioni {
            let landscape = sez.w > sez.h;
            (sez.w, sez.h) = if landscape { (h, w) } else { (w, h) };
        }
    }
    let st = impagina_doc::impagina(&doc, font::Fonti::new(), title);
    pdf::scrivi(&st, &pdf::Opzioni { embed: s.embed || s.pdfa, bookmarks: s.bookmarks, pdfa: s.pdfa, links: true })
}

pub mod xls;

#[cfg(test)]
mod prove_vere {
    /// Prova a mano: `CEDRO_FOGLIO=file.xlsx cargo test --lib prove_vere -- --ignored` scrive file.xlsx.pdf.
    #[test]
    #[ignore]
    fn stampa_un_file() {
        let Ok(list) = std::env::var("CEDRO_FOGLIO") else { return };
        for path in list.split(':') {
            let bytes = std::fs::read(path).unwrap();
            let ext = path.rsplit('.').next().unwrap().to_lowercase();
            let t = std::time::Instant::now();
            let made = if super::is_documento(&ext) { super::documento_pdf(&ext, &bytes, "prova", &super::Scelte::default()) } else { super::foglio_pdf(&ext, &bytes, path, "prova", &super::Scelte::default()) };
            let pdf = match made {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{path}: ERRORE {e}");
                    continue;
                }
            };
            eprintln!("{path}: {} byte in {:?}", pdf.len(), t.elapsed());
            std::fs::write(format!("{path}.cedro.pdf"), pdf).unwrap();
        }
    }
}
