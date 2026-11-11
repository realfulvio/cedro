//! Come si presenta ogni voce del menu «Formato»: il nome da mostrare, l'iconcina, il colore
//! (uno per famiglia di formati, uno per strumento) e due parole grigie che dicono a cosa serve.
//! Le icone sono a tratto 24×24 come quelle del kit; `fill` è la parte piena (può mancare).

use super::gfx::Rgb;
use crate::engine::Kind;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub stroke: &'static str,
    pub fill: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Voce {
    pub label: &'static str,
    pub hint: &'static str,
    pub glyph: Glyph,
    pub color: Rgb,
}

const fn hex(v: u32) -> Rgb {
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

// Le famiglie di colore (gli stessi toni dei tipi di file).
const WEB: Rgb = hex(0x1F8A62);
const NUOVI: Rgb = hex(0x7A3FC8);
const COMP: Rgb = hex(0x0D7C8F);
const PRO: Rgb = hex(0xA2760A);
const PDF: Rgb = hex(0xD2382C);
const OFFICE: Rgb = hex(0x2B6FD3);
const AUDIO: Rgb = hex(0xD4570E);
// Gli strumenti: un colore ciascuno.
const BLU: Rgb = hex(0x2563B8);
const PETROLIO: Rgb = hex(0x0E7C86);
const GRAFITE: Rgb = hex(0x56606B);
const ARANCIO: Rgb = hex(0xD4570E);
const ROSSO: Rgb = hex(0xC8323C);
const VIOLA: Rgb = hex(0x663FC0);
/// Il colore del laboratorio (figure, tabelle dei dati, microscopia).
const LAB: Rgb = hex(0xC2306B);

const fn g(stroke: &'static str, fill: &'static str) -> Glyph {
    Glyph { stroke, fill }
}

const PNG: Glyph = g("M4 4h16v16H4z", "M4 4h8v8H4zM12 12h8v8h-8z");
const JPEG: Glyph = g("M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M4 18l5-6 4 4 3-3 4 5 M6.5 8a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0z", "");
const WEBP: Glyph = g("M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0z M8 12a4 9 0 1 0 8 0a4 9 0 1 0-8 0z M3 12h18", "");
const GIF: Glyph = g("M4 5h16v14H4z", "M10 9l5 3-5 3z");
const AVIF: Glyph = g("M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z M18 15l.8 2.2 2.2.8-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z", "");
const JXL: Glyph = g("M6 4h12l3 5-9 11L3 9z M3 9h18 M12 20 8.5 9 10 4 M12 20l3.5-11L14 4", "");
const BMP: Glyph = g("M4 4h16v16H4z M9.3 4v16 M14.7 4v16 M4 9.3h16 M4 14.7h16", "");
const TIFF: Glyph = g("M7 3h10v5H7z M4 8h16v8H4z M7 13h10v8H7z", "");
const ICO: Glyph = g("M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z", "");
const TGA: Glyph = g("M12 3l8 4.5v9L12 21l-8-4.5v-9z M12 12l8-4.5 M12 12v9 M12 12 4 7.5", "");
const QOI: Glyph = g("M13 2 4 14h7l-1 8 9-12h-7z", "");
const PNM: Glyph = g("M4 5h16v14H4z M7 9l3 3-3 3 M12 15h5", "");
const HDR: Glyph = g("M8 12a4 4 0 1 0 8 0a4 4 0 1 0-8 0z M12 2v2.5 M12 19.5V22 M2 12h2.5 M19.5 12H22 M4.9 4.9l1.8 1.8 M17.3 17.3l1.8 1.8 M4.9 19.1l1.8-1.8 M17.3 6.7l1.8-1.8", "");
const EXR: Glyph = g("M4 9h16v11H4z M4 9l1.5-5H20l-1.5 5 M9.5 4 8 9 M15 4l-1.5 5", "");
const DOCPDF: Glyph = g("M6 3h8l4 4v14H6z M14 3v5h4 M9 13h6 M9 17h4", "");
const WORD: Glyph = g("M6 3h8l4 4v14H6z M14 3v5h4 M8.5 11l1.5 6 2-4.5 2 4.5 1.5-6", "");
const XLS: Glyph = g("M4 4h16v16H4z M4 9.5h16 M4 15h16 M10 4v16", "");
const TXT: Glyph = g("M6 3h8l4 4v14H6z M14 3v5h4 M9 12h6 M9 15.5h6 M9 19h3", "");
const HTML: Glyph = g("M8 7l-5 5 5 5 M16 7l5 5-5 5 M13.5 5l-3 14", "");
const MD: Glyph = g("M3 6h18v12H3z M6 15V9l2.5 3L11 9v6 M16 9v6 M13.5 12.5 16 15l2.5-2.5", "");
const CSV: Glyph = g("M4 4h16v16H4z M4 9.5h16 M4 15h16 M9.3 4v16 M14.7 4v16", "");
const JSON: Glyph = g("M9 4H8a2 2 0 0 0-2 2v3.5L4 12l2 2.5V18a2 2 0 0 0 2 2h1 M15 4h1a2 2 0 0 1 2 2v3.5l2 2.5-2 2.5V18a2 2 0 0 1-2 2h-1", "");
const MP3: Glyph = g("M9 18V5l11-2v13 M3 18a3 3 0 1 0 6 0a3 3 0 1 0-6 0z M14 16a3 3 0 1 0 6 0a3 3 0 1 0-6 0z", "");
const WAV: Glyph = g("M3 12h2l2-6 3 12 3-9 2 6 2-3h4", "");
const FLAC: Glyph = g("M4 9v6 M8 5v14 M12 2v20 M16 5v14 M20 9v6", "");
const OPUS: Glyph = g("M9 3h6v10a3 3 0 0 1-6 0z M5 11a7 7 0 0 0 14 0 M12 18v3", "");
const AIFF: Glyph = g("M4 9h4l5-4v14l-5-4H4z M16.5 8.5a5 5 0 0 1 0 7 M19 6a8.5 8.5 0 0 1 0 12", "");
const MP4: Glyph = g("M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z", "M10 8.5l5.5 3.5-5.5 3.5z");
const COMPRIMI: Glyph = g("M4 4l6 6 M10 5v5H5 M20 20l-6-6 M14 19v-5h5", "");
const META: Glyph = g("M3 3h18v18H3z", "M3 12h9v9H3z");
const BN: Glyph = g("M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0z", "M12 3a9 9 0 0 0 0 18z");
const RUOTA: Glyph = g("M20 12a8 8 0 1 1-2.3-5.7 M20 4v5h-5", "");
const PULISCI: Glyph = g("M12 21s-6-5.5-6-11a6 6 0 0 1 10.4-4.1 M18 9.5c-.3 3.8-3.4 7.6-4.6 8.9 M3 3l18 18", "");
const OCR: Glyph = g("M4 8V4h4 M16 4h4v4 M20 16v4h-4 M8 20H4v-4 M8 9h8 M12 9v7", "");
const MAIUSC: Glyph = g("M3 18 7.5 6 12 18 M4.7 13.5h5.6 M14 18V6h3.5a3 3 0 0 1 0 6H14 M14 12h4a3 3 0 0 1 0 6h-4", "");
const MINUSC: Glyph = g("M4 14.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0z M11 11v7 M14 6v12 M14 14.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0z", "");
const UNISCI: Glyph = g("M5 4h9v6H5z M5 14h9v6H5z M17 7h2v10h-2 M19 12h2", "");
const DIVIDI: Glyph = g("M6 3h12v7H6z M6 14h12v7H6z M3 12h2 M8 12h2 M14 12h2 M19 12h2", "");
const FIGURA: Glyph = g("M3 3h8v8H3z M13 3h8v8h-8z M3 13h8v8H3z M13 13h8v8h-8z M5 9.5h3", "");
const VERIFICA: Glyph = g("M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6l8-3z M8.5 12l2.5 2.5 4.5-5", "");
const ALTRO: Glyph = g("M6 3h8l4 4v14H6z M14 3v5h4", "");

const fn v(label: &'static str, hint: &'static str, glyph: Glyph, color: Rgb) -> Voce {
    Voce { label, hint, glyph, color }
}

/// La voce del menu per un'azione di una categoria.
pub fn voce(kind: Kind, id: &str) -> Voce {
    let label = crate::conversioni::action_label(kind, id);
    let fallback = v(if label.is_empty() { id_label(id) } else { label }, "", ALTRO, COMP);
    match (kind, id) {
        // Immagini
        (Kind::Image, "png") => v("PNG", "trasparenza, nessuna perdita", PNG, WEB),
        (Kind::Image, "jpeg") => v("JPEG", "foto, leggero", JPEG, WEB),
        (Kind::Image, "webp") => v("WebP", "per siti e WhatsApp", WEBP, WEB),
        (Kind::Image, "avif") => v("AVIF", "piccolissimo", AVIF, NUOVI),
        (Kind::Image, "jxl") => v("JPEG XL", "nessuna perdita, leggero", JXL, NUOVI),
        (Kind::Image, "bmp") => v("BMP", "senza compressione", BMP, COMP),
        (Kind::Image, "gif") => v("GIF", "animazioni", GIF, WEB),
        (Kind::Image, "tiff") => v("TIFF", "stampa e archivio", TIFF, COMP),
        (Kind::Image, "ico") => v("ICO", "icone di Windows", ICO, COMP),
        (Kind::Image, "tga") => v("TGA", "grafica 3D e giochi", TGA, COMP),
        (Kind::Image, "qoi") => v("QOI", "veloce da aprire", QOI, COMP),
        (Kind::Image, "pnm") => v("PNM", "programmi Linux", PNM, COMP),
        (Kind::Image, "hdr") => v("HDR", "luci ad alta dinamica", HDR, PRO),
        (Kind::Image, "exr") => v("OpenEXR", "cinema ed effetti", EXR, PRO),
        (Kind::Image, "pdf") => v("PDF", "con testo ricercabile", DOCPDF, PDF),
        (Kind::Image, "compress") => v("Comprimi", "stesso aspetto, meno MB", COMPRIMI, BLU),
        (Kind::Image, "half") => v("Dimezza le misure", "metà larghezza e altezza", META, PETROLIO),
        (Kind::Image, "gray") => v("Bianco e nero", "toglie i colori", BN, GRAFITE),
        (Kind::Image, "rot90") => v("Ruota di 90° a destra", "senso orario", RUOTA, ARANCIO),
        (Kind::Image, "strip") => v("Togli GPS e dati nascosti", "luogo, data, fotocamera", PULISCI, ROSSO),
        (Kind::Image, "ocr") => v("Leggi il testo (OCR)", "copia le scritte", OCR, VIOLA),
        (Kind::Image, "figure") => v("Figura per un articolo", "pannelli A B C, 300 dpi", FIGURA, LAB),
        // PDF
        (Kind::Pdf, "jpg") => v("JPG", "una foto per pagina", JPEG, WEB),
        (Kind::Pdf, "docx") => v("Word", "da modificare, anche scansioni", WORD, OFFICE),
        (Kind::Pdf, "xlsx") => v("Excel", "tabelle in fogli di calcolo", XLS, WEB),
        (Kind::Pdf, "png") => v("PNG", "un'immagine per pagina", PNG, WEB),
        (Kind::Pdf, "txt") => v("Solo testo", "senza impaginazione", TXT, COMP),
        (Kind::Pdf, "ocr") => v("Rendi il testo ricercabile (OCR)", "per le scansioni", OCR, VIOLA),
        (Kind::Pdf, "compress") => v("Comprimi", "PDF più leggero", COMPRIMI, BLU),
        (Kind::Pdf, "merge") => v("Unisci", "un solo PDF", UNISCI, PETROLIO),
        (Kind::Pdf, "split") => v("Dividi", "una pagina per file", DIVIDI, ARANCIO),
        (Kind::Pdf, "strip") => v("Togli autore e dati nascosti", "autore, programma, commenti", PULISCI, ROSSO),
        // Audio
        (Kind::Audio, "mp3") => v("MP3", "va bene ovunque", MP3, AUDIO),
        (Kind::Audio, "wav") => v("WAV", "senza compressione", WAV, COMP),
        (Kind::Audio, "flac") => v("FLAC", "nessuna perdita, leggero", FLAC, NUOVI),
        (Kind::Audio, "opus") => v("Opus", "piccolissimo, ottimo per la voce", OPUS, AUDIO),
        (Kind::Audio, "aiff") => v("AIFF", "Mac, senza compressione", AIFF, COMP),
        // Video
        (Kind::Video, "mp4") => v("MP4", "si apre ovunque", MP4, NUOVI),
        (Kind::Video, "mp3") => v("Solo l'audio (MP3)", "toglie le immagini", MP3, AUDIO),
        // Documenti
        (Kind::Doc, "pdf") => v("PDF", "si apre ovunque", DOCPDF, PDF),
        (Kind::Doc, "strip") => v("Togli autore e dati nascosti", "autore, revisioni, commenti", PULISCI, ROSSO),
        // Testo
        (Kind::Text, "txt") => v("TXT", "testo semplice", TXT, COMP),
        (Kind::Text, "html") => v("HTML", "pagina web", HTML, WEB),
        (Kind::Text, "md") => v("Markdown", "titoli ed elenchi", MD, NUOVI),
        (Kind::Text, "csv") => v("CSV", "tabella per Excel", CSV, WEB),
        (Kind::Text, "xlsx") => v("Excel", "numeri veri, geni come testo", XLS, OFFICE),
        (Kind::Text, "json") => v("JSON", "dati per programmi", JSON, COMP),
        (Kind::Text, "pdf") => v("PDF", "pronto da stampare", DOCPDF, PDF),
        (Kind::Text, "upper") => v("Tutto MAIUSCOLO", "", MAIUSC, BLU),
        (Kind::Text, "lower") => v("tutto minuscolo", "", MINUSC, PETROLIO),
        // Archivi
        (Kind::Archive, "z-webp") => v("Foto in WebP", "tutte le foto dello zip", WEBP, WEB),
        (Kind::Archive, "z-jpeg") => v("Foto in JPEG", "", JPEG, WEB),
        (Kind::Archive, "z-png") => v("Foto in PNG", "", PNG, WEB),
        (Kind::Archive, "z-mp3") => v("Audio in MP3", "tutti gli audio", MP3, AUDIO),
        (Kind::Archive, "z-wav") => v("Audio in WAV", "", WAV, COMP),
        (Kind::Archive, "z-pdf") => v("Documenti in PDF", "Word, ODT, testi", DOCPDF, PDF),
        (Kind::Archive, "verify-pack") => v("Verifica il pacchetto di dati", "impronte SHA-256 di ogni file", VERIFICA, LAB),
        _ => fallback,
    }
}

fn id_label(id: &str) -> &'static str {
    match id {
        "merge" => "Unisci",
        "split" => "Dividi",
        "figure" => "Figura",
        _ => "",
    }
}

// Le opzioni con un elenco di valori: un'iconcina per opzione e due parole per valore.
const O_MISURE: Glyph = g("M4 20 20 4 M14 4h6v6 M10 20H4v-6", "");
const O_PAGINA: Glyph = g("M6 3h8l4 4v14H6z M14 3v5h4", "");
const O_DPI: Glyph = g("M4 4h16v16H4z M4 9.3h16 M4 14.7h16 M9.3 4v16 M14.7 4v16", "");
const O_CODIFICA: Glyph = g("M3 18 7.5 6 12 18 M4.7 13.5h5.6 M14 14.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0z M21 11v7", "");
const O_SEPARATORE: Glyph = g("M4 4h16v16H4z M9.3 4v16 M14.7 4v16", "");
const O_CANALI: Glyph = g("M3 9h3l4-4v14l-4-4H3z M14 9a4 4 0 0 1 0 6 M17 6a8 8 0 0 1 0 12", "");
const O_SILENZIO: Glyph = g("M3 6a3 3 0 1 0 6 0a3 3 0 1 0-6 0z M3 18a3 3 0 1 0 6 0a3 3 0 1 0-6 0z M8.1 8.1 20 20 M8.1 15.9 20 4", "");
const O_SCHERMO: Glyph = g("M3 4h18v12H3z M8 20h8 M12 16v4", "");
const O_FOTOGRAMMI: Glyph = g("M4 4h16v16H4z M8 4v16 M16 4v16 M4 8h4 M4 12h4 M4 16h4 M16 8h4 M16 12h4 M16 16h4", "");
const O_ESISTE: Glyph = g("M9 7h10a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1z M16 7V3H4v14h4", "");

/// L'iconcina di un'opzione a elenco (nome e pulsante chiuso). `None` per le altre.
pub fn opzione(opt: crate::conversioni::Opt) -> Option<Voce> {
    use crate::conversioni::Opt as O;
    let (glyph, color) = match opt {
        O::MaxSide => (O_MISURE, WEB),
        O::PageSize => (O_PAGINA, OFFICE),
        O::Dpi => (O_DPI, PDF),
        O::Encoding => (O_CODIFICA, COMP),
        O::CsvSeparator => (O_SEPARATORE, WEB),
        O::Mp3Kbps => (MP3, AUDIO),
        O::Channels => (O_CANALI, PETROLIO),
        O::TrimSilence => (O_SILENZIO, PRO),
        O::VideoHeight => (O_SCHERMO, NUOVI),
        O::Fps => (O_FOTOGRAMMI, VIOLA),
        _ => return None,
    };
    Some(Voce { label: "", hint: "", glyph, color })
}

/// L'iconcina della scelta «Se il file esiste già» (per i valori del menu: si vede solo il suggerimento).
pub fn esiste() -> Voce {
    Voce { label: "", hint: "", glyph: O_ESISTE, color: GRAFITE }
}

/// Le due parole grigie accanto a un valore di un'opzione.
pub fn suggerimento(opt: crate::conversioni::Opt, value: u32) -> &'static str {
    use crate::conversioni::Opt as O;
    match (opt, value) {
        (O::MaxSide, 0) => "nessun ridimensionamento",
        (O::MaxSide, 3840) => "4K, schermi grandi",
        (O::MaxSide, 1920) => "Full HD, va bene quasi sempre",
        (O::MaxSide, 1280) => "per email e chat",
        (O::MaxSide, 800) => "anteprime, leggerissime",
        (O::PageSize, 0) => "21 × 29,7 cm",
        (O::PageSize, 1) => "29,7 × 42 cm",
        (O::PageSize, 2) => "USA, 21,6 × 27,9 cm",
        (O::Dpi, 72) => "schermo, leggero",
        (O::Dpi, 144) => "schermo nitido",
        (O::Dpi, 200) => "buona da leggere",
        (O::Dpi, 300) => "per la stampa",
        (O::TrimSilence, 0) => "lascia tutto",
        (O::TrimSilence, 30) => "taglia di più",
        (O::TrimSilence, 40) => "equilibrato",
        (O::TrimSilence, 50) => "solo il silenzio vero",
        (O::Encoding, 0) => "va bene ovunque",
        (O::Encoding, 1) => "vecchi programmi Windows",
        (O::Encoding, 2) => "vecchi sistemi",
        (O::CsvSeparator, 1) => "Excel in italiano",
        (O::CsvSeparator, 2) => "Excel in inglese, programmi",
        (O::CsvSeparator, 3) => "da incollare in Excel",
        (O::Mp3Kbps, 64) => "voce, leggerissimo",
        (O::Mp3Kbps, 96) => "voce, leggero",
        (O::Mp3Kbps, 128) => "voce e podcast",
        (O::Mp3Kbps, 160) => "musica, leggero",
        (O::Mp3Kbps, 192) => "musica",
        (O::Mp3Kbps, 256) => "musica, alta qualità",
        (O::Mp3Kbps, 320) => "massima qualità",
        (O::Channels, 2) => "destra e sinistra",
        (O::Channels, 1) => "metà peso, per la voce",
        (O::VideoHeight, 1080) => "Full HD",
        (O::VideoHeight, 720) => "HD, circa metà peso",
        (O::VideoHeight, 480) => "leggero, per il telefono",
        (O::Fps, 30) => "fluido",
        (O::Fps, 25) => "TV europea",
        (O::Fps, 24) => "cinema",
        _ => "",
    }
}

/// Le due parole grigie accanto a una scelta di «Se il file esiste già».
pub fn suggerimento_esiste(e: crate::engine::Exists) -> &'static str {
    use crate::engine::Exists;
    match e {
        Exists::Number => "tiene tutti e due",
        Exists::Replace => "il vecchio si perde",
        Exists::Skip => "lascia il vecchio",
        Exists::Ask => "decidi tu, file per file",
    }
}


/// Il titolo del gruppo degli strumenti di una categoria.
pub fn tools_heading(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "Modifica l'immagine",
        Kind::Pdf => "Modifica il PDF",
        Kind::Text => "Modifica il testo",
        Kind::Doc => "Modifica il documento",
        _ => "Strumenti",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ogni_voce_ha_icona_e_nome() {
        for kind in [Kind::Image, Kind::Pdf, Kind::Audio, Kind::Video, Kind::Doc, Kind::Text, Kind::Archive] {
            for s in crate::conversioni::formats(kind).iter().chain(crate::conversioni::tools(kind)) {
                let v = voce(kind, s.id);
                assert!(!v.label.is_empty(), "{kind:?}/{}", s.id);
                assert!(v.glyph != ALTRO, "{kind:?}/{} senza icona", s.id);
                assert!(super::super::svgpath::parse(v.glyph.stroke).is_some(), "{kind:?}/{}", s.id);
                for d in crate::conversioni::option_defs(kind, s.id) {
                    if let crate::conversioni::Control::Choice(choices) = d.control {
                        assert!(opzione(d.opt).is_some(), "{:?} senza icona", d.opt);
                        for c in choices {
                            assert!(!suggerimento(d.opt, c.value).is_empty() || c.label.starts_with("Come"), "{:?}={} senza suggerimento", d.opt, c.value);
                        }
                    }
                }
                if !v.glyph.fill.is_empty() {
                    assert!(super::super::svgpath::parse(v.glyph.fill).is_some(), "{kind:?}/{}", s.id);
                }
            }
        }
    }
}
