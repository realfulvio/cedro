//! Referti dettati già impaginati: la dettatura con i termini veterinari esce come un referto del
//! laboratorio (logo e nome della clinica in testa, i campi caso, specie, campione e data, le parti
//! «Descrizione macroscopica», «Diagnosi»… riconosciute dalle parole dette) invece che come una
//! trascrizione con gli orari. Finché i punti da riascoltare non sono confermati resta una BOZZA, scritta
//! in testa a ogni pagina, e non ha la riga della firma.

use std::path::Path;

use crate::i18n::{tr, tr_string};
use crate::trascrizione::{minutes_text, today_text, word_language, xml_escape, Segment};

/// La testa del referto: chi lo fa e per quale caso.
#[derive(Clone, Debug, Default)]
pub struct Intestazione {
    pub clinica: String,
    /// Il logo della clinica in PNG (quello ricordato per le radiografie del proprietario).
    pub logo: Option<Vec<u8>>,
    pub caso: Option<String>,
}

impl Intestazione {
    /// Quella di adesso: nome della clinica dalle impostazioni, logo ricordato, caso aperto.
    pub fn attuale() -> Intestazione {
        let logo = std::fs::read(crate::engine::logo_clinica()).ok().filter(|b| b.starts_with(b"\x89PNG\r\n\x1a\n"));
        Intestazione { clinica: crate::settings::read_settings().nome_clinica.trim().to_string(), logo, caso: crate::caso::carica().map(|c| c.code) }
    }
}

/// Una parte del referto, col suo titolo (nessuno per il testo prima della prima parte detta).
#[derive(Clone, Debug, PartialEq)]
pub struct Sezione {
    pub titolo: Option<&'static str>,
    pub paragrafi: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Referto {
    pub specie: Option<String>,
    pub campione: Option<String>,
    pub sezioni: Vec<Sezione>,
}

/// Le parti di un referto e le parole con cui si dettano (le più lunghe prima). `true`: la parola da
/// sola apre la parte anche senza punteggiatura dopo («Diagnosi carcinoma…»).
const PARTI: &[(&str, &str, bool)] = &[
    ("descrizione macroscopica", "Descrizione macroscopica", true),
    ("esame macroscopico", "Descrizione macroscopica", true),
    ("macroscopia", "Descrizione macroscopica", true),
    ("descrizione microscopica", "Descrizione microscopica", true),
    ("esame istologico", "Descrizione microscopica", true),
    ("esame microscopico", "Descrizione microscopica", true),
    ("esame citologico", "Descrizione microscopica", true),
    ("microscopia", "Descrizione microscopica", true),
    ("istologia", "Descrizione microscopica", true),
    ("diagnosi istologica", "Diagnosi", true),
    ("diagnosi morfologica", "Diagnosi", true),
    ("diagnosi citologica", "Diagnosi", true),
    ("diagnosi", "Diagnosi", true),
    ("notizie cliniche", "Anamnesi", true),
    ("storia clinica", "Anamnesi", true),
    ("anamnesi", "Anamnesi", true),
    ("margini chirurgici", "Margini", false),
    ("margini", "Margini", false),
    ("commento", "Commento", true),
    ("conclusioni", "Conclusioni", true),
    ("conclusione", "Conclusioni", false),
    ("note", "Note", false),
    ("nota", "Note", false),
];

/// Le parole dopo cui la parte non comincia: è una frase normale («Diagnosi differenziale…»).
const NON_PARTE: &[&str] = &["differenziale", "differenziali", "è", "sono", "non", "che", "del", "della", "dei", "delle", "degli", "dello"];

/// Le preposizioni dopo cui un campo non è un campo: «Campione di cute del dorso» è descrizione.
const PREPOSIZIONI: &[&str] = &["di", "del", "della", "dei", "delle", "degli", "dello", "in", "con", "da", "a", "al", "è", "e", "inviato", "inviata", "prelevato", "prelevata"];

fn maiuscola(s: &str) -> String {
    let s = s.trim().trim_start_matches([':', ',', ';', '-', '–', '.', ' ']).trim();
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Se la frase comincia con `parola` seguita da punteggiatura, spazio o niente: quello che resta.
fn dopo<'a>(frase: &'a str, minuscola: &str, parola: &str) -> Option<(&'a str, bool)> {
    if !minuscola.starts_with(parola) || !frase.is_char_boundary(parola.len()) {
        return None;
    }
    let resto = &frase[parola.len()..];
    match resto.chars().next() {
        None => Some(("", true)),
        Some(c) if matches!(c, ':' | ',' | '.' | ';' | '-' | '–') => Some((resto, true)),
        Some(c) if c.is_whitespace() => Some((resto, false)),
        _ => None,
    }
}

fn prima_parola(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// «Specie: cane», «Campione biopsia cutanea»: il valore del campo.
fn campo(frase: &str, nomi: &[&str]) -> Option<String> {
    let m = frase.to_lowercase();
    for n in nomi {
        if let Some((resto, punteggiatura)) = dopo(frase, &m, n) {
            if !punteggiatura && PREPOSIZIONI.contains(&prima_parola(resto).as_str()) {
                return None;
            }
            let v = maiuscola(resto);
            let v = v.trim_end_matches(['.', ';', ',']).trim().to_string();
            return (!v.is_empty()).then_some(v);
        }
    }
    None
}

/// «Diagnosi: carcinoma…»: il titolo della parte e quello che resta della frase.
fn parte(frase: &str) -> Option<(&'static str, String)> {
    let m = frase.to_lowercase();
    for (parola, titolo, da_sola) in PARTI {
        let Some((resto, punteggiatura)) = dopo(frase, &m, parola) else { continue };
        if !punteggiatura && (!da_sola || NON_PARTE.contains(&prima_parola(resto).as_str())) {
            continue;
        }
        // «Diagnosi di carcinoma»: sotto il titolo «Diagnosi» basta «Carcinoma».
        let resto = resto.trim_start_matches([':', ',', ';', '-', '–', '.', ' ']);
        let resto = if *titolo == "Diagnosi" { resto.strip_prefix("di ").unwrap_or(resto) } else { resto };
        return Some((titolo, maiuscola(resto)));
    }
    None
}

/// «Punto e a capo» detto in fondo alla frase: la frase senza quelle parole, e se c'era.
fn a_capo(frase: &str) -> (&str, bool) {
    let t = frase.trim_end_matches(['.', ',', ';', ' ']);
    let m = t.to_lowercase();
    for fine in ["e a capo", "a capo"] {
        if m.ends_with(fine) && (m.len() == fine.len() || m.as_bytes()[m.len() - fine.len() - 1] == b' ') {
            return (t[..t.len() - fine.len()].trim_end_matches([',', ' ']), true);
        }
    }
    (frase, false)
}

/// Il testo diviso dove è stato detto «punto e a capo» (anche in mezzo a una frase di whisper).
fn dividi_a_capo(testo: &str) -> Vec<&str> {
    let b = testo.as_bytes();
    let mut out = Vec::new();
    let mut da = 0;
    let mut i = 0;
    while i < b.len() {
        let preso = ["punto e a capo", "punto a capo", "nuovo paragrafo"].iter().find_map(|c| {
            let fine = i + c.len();
            let uguale = b.get(i..fine).is_some_and(|w| w.eq_ignore_ascii_case(c.as_bytes()));
            let bordi = (i == 0 || !b[i - 1].is_ascii_alphanumeric()) && b.get(fine).is_none_or(|n| !n.is_ascii_alphanumeric());
            (uguale && bordi).then_some(fine)
        });
        match preso {
            Some(fine) => {
                out.push(testo[da..i].trim_end_matches([',', ' ', ';']));
                da = fine;
                i = fine;
            }
            None => i += 1,
        }
    }
    out.push(&testo[da..]);
    out
}

/// Il referto dalle frasi dettate: i campi detti, le parti e i paragrafi.
pub fn componi(segments: &[Segment]) -> Referto {
    let testo = segments.iter().map(|s| s.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    let mut r = Referto { sezioni: vec![Sezione { titolo: None, paragrafi: Vec::new() }], ..Default::default() };
    let mut nuovo_paragrafo = true;
    let frasi: Vec<(bool, &str)> = dividi_a_capo(&testo).into_iter().enumerate().flat_map(|(k, blocco)| crate::revisione::frasi(blocco).into_iter().enumerate().map(move |(j, f)| (k > 0 && j == 0, f))).collect();
    for (dopo_capo, frase) in frasi {
        nuovo_paragrafo |= dopo_capo;
        let frase = frase.trim_start_matches(['.', ',', ';', ' ']);
        let (frase, capo) = a_capo(frase);
        let frase = frase.trim();
        if !frase.is_empty() {
            if r.specie.is_none() {
                if let Some(v) = campo(frase, &["specie", "animale"]) {
                    r.specie = Some(v);
                    nuovo_paragrafo |= capo;
                    continue;
                }
            }
            if r.campione.is_none() {
                if let Some(v) = campo(frase, &["tipo di campione", "materiale inviato", "campione", "materiale", "prelievo"]) {
                    r.campione = Some(v);
                    nuovo_paragrafo |= capo;
                    continue;
                }
            }
            let mut frase = frase.to_string();
            if let Some((titolo, resto)) = parte(&frase) {
                r.sezioni.push(Sezione { titolo: Some(titolo), paragrafi: Vec::new() });
                nuovo_paragrafo = true;
                frase = resto;
            }
            if !frase.is_empty() {
                // Una frase senza punto in fondo (l'ultima, o tagliata da «a capo») lo prende.
                if !frase.ends_with(['.', '!', '?', ';', ':']) {
                    frase.push('.');
                }
                let sez = r.sezioni.last_mut().expect("almeno una parte");
                match sez.paragrafi.last_mut() {
                    Some(p) if !nuovo_paragrafo => {
                        p.push(' ');
                        p.push_str(&frase);
                    }
                    _ => sez.paragrafi.push(maiuscola(&frase)),
                }
                nuovo_paragrafo = false;
            }
        }
        nuovo_paragrafo |= capo;
    }
    r.sezioni.retain(|s| s.titolo.is_some() || !s.paragrafi.is_empty());
    r
}

/// Il referto in testo semplice.
pub fn testo(source: &Path, secs: f64, segments: &[Segment], intest: &Intestazione, draft: Option<&str>) -> Vec<u8> {
    let r = componi(segments);
    let mut out = String::new();
    if !intest.clinica.is_empty() {
        out.push_str(&format!("{}\n\n", intest.clinica));
    }
    out.push_str(&format!("{}\n", tr("REFERTO")));
    out.push_str(&format!("{}\n", meta(source, secs)));
    let campi: Vec<String> = campi(&r, intest).into_iter().map(|(k, v)| format!("{}: {}", tr(k), if v.is_empty() { "—".to_string() } else { v })).collect();
    out.push_str(&format!("\n{}\n", campi.join("   ")));
    if let Some(d) = draft {
        out.push_str(&format!("\n{d}\n"));
    }
    for s in &r.sezioni {
        out.push('\n');
        if let Some(t) = s.titolo {
            out.push_str(&format!("{}\n", tr(t).to_uppercase()));
        }
        for p in &s.paragrafi {
            out.push_str(&format!("{p}\n"));
        }
    }
    if draft.is_none() {
        out.push_str(&format!("\n\n{}: ______________________________\n", tr("Firma")));
    }
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    out.trim_end().replace('\n', eol).into_bytes()
}

fn meta(source: &Path, secs: f64) -> String {
    let nome = source.file_name().and_then(|s| s.to_str()).unwrap_or("");
    tr_string(&format!("Dettato il {} · {} · {} di audio", today_text(), nome, minutes_text(secs)))
}

fn campi(r: &Referto, intest: &Intestazione) -> [(&'static str, String); 4] {
    [
        ("Caso", intest.caso.clone().unwrap_or_default()),
        ("Specie", r.specie.clone().unwrap_or_default()),
        ("Campione", r.campione.clone().unwrap_or_default()),
        ("Data", today_text()),
    ]
}

/// Le misure del logo in EMU: alto 1,5 cm, largo al massimo 5 cm.
fn misure_logo(png: &[u8]) -> Option<(u64, u64)> {
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(png)).with_guessed_format().ok()?.into_dimensions().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    let (mut cy, mut cx) = (540_000u64, 540_000u64 * w as u64 / h as u64);
    if cx > 1_800_000 {
        cy = cy * 1_800_000 / cx;
        cx = 1_800_000;
    }
    Some((cx, cy))
}

const BLU: &str = "1F4E79";
const ROSSO: &str = "B3261E";
const GRIGIO: &str = "5D7080";

/// Il referto in Word: testa con logo e clinica (e «BOZZA» finché lo è), campi, parti, firma e numeri di pagina.
pub fn docx(source: &Path, secs: f64, lang: &str, segments: &[Segment], intest: &Intestazione, draft: Option<&str>) -> Vec<u8> {
    let r = componi(segments);
    let x = |s: &str| xml_escape(s);
    let run = |s: &str, rpr: &str| format!("<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", x(s));
    let mut body = String::new();
    body.push_str(&format!("<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr>{}</w:p>", run(&tr("Referto"), "")));
    body.push_str(&format!("<w:p><w:pPr><w:pStyle w:val=\"Meta\"/></w:pPr>{}</w:p>", run(&meta(source, secs), "")));
    // I campi: una riga di quattro caselle, il nome piccolo e grigio sopra il valore.
    let bordo = format!("<w:top w:val=\"single\" w:sz=\"4\" w:color=\"C9D3DC\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"C9D3DC\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"C9D3DC\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"C9D3DC\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"C9D3DC\"/>");
    body.push_str(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"5000\" w:type=\"pct\"/><w:tblBorders>{bordo}</w:tblBorders><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:top w:w=\"80\" w:type=\"dxa\"/><w:left w:w=\"120\" w:type=\"dxa\"/><w:bottom w:w=\"80\" w:type=\"dxa\"/><w:right w:w=\"120\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>"));
    for _ in 0..4 {
        body.push_str("<w:gridCol w:w=\"2409\"/>");
    }
    body.push_str("</w:tblGrid><w:tr>");
    for (k, v) in campi(&r, intest) {
        body.push_str(&format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"1250\" w:type=\"pct\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F4F7FB\"/></w:tcPr><w:p><w:pPr><w:spacing w:after=\"20\"/></w:pPr>{}</w:p><w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr>{}</w:p></w:tc>",
            run(&tr(k).to_uppercase(), &format!("<w:color w:val=\"{GRIGIO}\"/><w:sz w:val=\"16\"/>")),
            run(&v, "<w:b/>")
        ));
    }
    body.push_str("</w:tr></w:tbl>");
    if let Some(d) = draft {
        body.push_str(&format!("<w:p><w:pPr><w:spacing w:before=\"200\"/></w:pPr>{}</w:p>", run(d, &format!("<w:b/><w:color w:val=\"{ROSSO}\"/>"))));
    }
    for s in &r.sezioni {
        if let Some(t) = s.titolo {
            body.push_str(&format!("<w:p><w:pPr><w:pStyle w:val=\"Parte\"/></w:pPr>{}</w:p>", run(&tr(t), "")));
        } else {
            body.push_str("<w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr></w:p>");
        }
        for p in &s.paragrafi {
            let stile = if s.titolo == Some("Diagnosi") { "<w:b/>" } else { "" };
            body.push_str(&format!("<w:p>{}</w:p>", run(p, stile)));
        }
    }
    if draft.is_none() {
        body.push_str(&format!(
            "<w:p><w:pPr><w:keepNext/><w:spacing w:before=\"720\" w:after=\"0\"/><w:ind w:left=\"5103\"/></w:pPr>{}</w:p>\
<w:p><w:pPr><w:spacing w:before=\"480\"/><w:ind w:left=\"5103\"/><w:pBdr><w:top w:val=\"single\" w:sz=\"4\" w:space=\"4\" w:color=\"333333\"/></w:pBdr></w:pPr></w:p>",
            run(&tr("Firma del refertante"), &format!("<w:color w:val=\"{GRIGIO}\"/>"))
        ));
    }

    let ns = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"";
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document {ns}><w:body>{body}\
<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:footerReference w:type=\"default\" r:id=\"rIdF\"/>\
<w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1418\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"454\" w:footer=\"454\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );

    // La testa: logo e nome della clinica a sinistra, «BOZZA» in rosso a destra finché lo è.
    let logo = intest.logo.as_ref().and_then(|b| misure_logo(b).map(|m| (b, m)));
    let mut testa = String::from("<w:p><w:pPr><w:tabs><w:tab w:val=\"right\" w:pos=\"9638\"/></w:tabs><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"4\" w:color=\"1F4E79\"/></w:pBdr><w:spacing w:after=\"0\"/></w:pPr>");
    if let Some((_, (cx, cy))) = logo {
        testa.push_str(&format!(
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"1\" name=\"Logo\"/>\
<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"1\" name=\"logo.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdLogo\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        ));
        if !intest.clinica.is_empty() {
            testa.push_str("<w:r><w:t xml:space=\"preserve\">   </w:t></w:r>");
        }
    }
    if !intest.clinica.is_empty() {
        testa.push_str(&run(&intest.clinica, &format!("<w:b/><w:color w:val=\"{BLU}\"/><w:sz w:val=\"28\"/>")));
    }
    if draft.is_some() {
        testa.push_str("<w:r><w:tab/></w:r>");
        testa.push_str(&run(&tr("BOZZA · da non firmare"), &format!("<w:b/><w:color w:val=\"{ROSSO}\"/><w:sz w:val=\"22\"/>")));
    }
    testa.push_str("</w:p>");
    let header = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:hdr {ns}>{testa}</w:hdr>");
    let pagina = tr_string("Pagina {} di {}");
    let mut piede = String::from("<w:p><w:pPr><w:jc w:val=\"center\"/><w:spacing w:after=\"0\"/></w:pPr>");
    let grigio = format!("<w:color w:val=\"{GRIGIO}\"/><w:sz w:val=\"16\"/>");
    if let Some(c) = &intest.caso {
        piede.push_str(&run(&format!("{} {c}  ·  ", tr("Caso")), &grigio));
    }
    let mut pezzi = pagina.splitn(3, "{}");
    let campo_xml = |f: &str| format!("<w:r><w:rPr>{grigio}</w:rPr><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:rPr>{grigio}</w:rPr><w:instrText xml:space=\"preserve\"> {f} </w:instrText></w:r><w:r><w:rPr>{grigio}</w:rPr><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:rPr>{grigio}</w:rPr><w:t>1</w:t></w:r><w:r><w:rPr>{grigio}</w:rPr><w:fldChar w:fldCharType=\"end\"/></w:r>");
    piede.push_str(&run(pezzi.next().unwrap_or(""), &grigio));
    piede.push_str(&campo_xml("PAGE"));
    piede.push_str(&run(pezzi.next().unwrap_or(""), &grigio));
    piede.push_str(&campo_xml("NUMPAGES"));
    piede.push_str(&run(pezzi.next().unwrap_or(""), &grigio));
    piede.push_str("</w:p>");
    let footer = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:ftr {ns}>{piede}</w:ftr>");

    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/>\
<Override PartName=\"/word/footer1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let word_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
<Relationship Id=\"rIdH\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/header\" Target=\"header1.xml\"/>\
<Relationship Id=\"rIdF\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer\" Target=\"footer1.xml\"/>\
</Relationships>";
    let header_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rIdLogo\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/logo.png\"/>\
</Relationships>";
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:lang w:val=\"{lang}\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:spacing w:before=\"120\" w:after=\"40\"/></w:pPr><w:rPr><w:b/><w:caps/><w:spacing w:val=\"20\"/><w:color w:val=\"{BLU}\"/><w:sz w:val=\"34\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Meta\"><w:name w:val=\"Meta\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
<w:pPr><w:spacing w:after=\"200\"/></w:pPr><w:rPr><w:color w:val=\"{GRIGIO}\"/><w:sz w:val=\"18\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Parte\"><w:name w:val=\"Parte del referto\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"280\" w:after=\"80\"/><w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"2\" w:color=\"D6DEE5\"/></w:pBdr></w:pPr>\
<w:rPr><w:b/><w:caps/><w:color w:val=\"{BLU}\"/><w:sz w:val=\"21\"/></w:rPr></w:style>\
</w:styles>",
        lang = word_language(lang)
    );
    let mut parti: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", word_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
        ("word/header1.xml", header.as_bytes()),
        ("word/footer1.xml", footer.as_bytes()),
    ];
    if let Some((png, _)) = logo {
        parti.push(("word/_rels/header1.xml.rels", header_rels.as_bytes()));
        parti.push(("word/media/logo.png", png));
    }
    crate::zipstore::zip_archive(&parti)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t: &str) -> Segment {
        Segment { t0: 0, t1: 100, text: t.into(), speaker: None }
    }

    #[test]
    fn riconosce_campi_parti_e_a_capo() {
        let r = componi(&[
            seg("Specie: cane. Campione, nodulo cutaneo del fianco sinistro."),
            seg("Descrizione macroscopica. Campione di cute di 2 per 1,5 cm con nodulo rilevato, punto e a capo"),
            seg("Al taglio superficie biancastra."),
            seg("Descrizione microscopica: proliferazione di cellule rotonde disposte in nidi."),
            seg("Diagnosi di mastocitoma cutaneo di grado 2."),
            seg("Diagnosi differenziale con istiocitoma esclusa. Margini indenni."),
        ]);
        assert_eq!(r.specie.as_deref(), Some("Cane"));
        assert_eq!(r.campione.as_deref(), Some("Nodulo cutaneo del fianco sinistro"));
        let titoli: Vec<_> = r.sezioni.iter().map(|s| s.titolo).collect();
        assert_eq!(titoli, [Some("Descrizione macroscopica"), Some("Descrizione microscopica"), Some("Diagnosi")]);
        // «Campione di cute…» è descrizione, non il campo; «a capo» divide i paragrafi.
        assert_eq!(r.sezioni[0].paragrafi, ["Campione di cute di 2 per 1,5 cm con nodulo rilevato.", "Al taglio superficie biancastra."]);
        assert_eq!(r.sezioni[1].paragrafi, ["Proliferazione di cellule rotonde disposte in nidi."]);
        assert_eq!(r.sezioni[2].paragrafi, ["Mastocitoma cutaneo di grado 2. Diagnosi differenziale con istiocitoma esclusa. Margini indenni."]);
    }

    #[test]
    fn senza_parti_dette_resta_un_testo_solo() {
        let r = componi(&[seg("Lesione compatibile con dermatite. Si consiglia controllo.")]);
        assert_eq!(r.specie, None);
        assert_eq!(r.sezioni.len(), 1);
        assert_eq!(r.sezioni[0].titolo, None);
        assert_eq!(r.sezioni[0].paragrafi, ["Lesione compatibile con dermatite. Si consiglia controllo."]);
    }

    fn parte_zip(zip: &[u8], nome: &str) -> Option<String> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(zip)).ok()?;
        let mut f = a.by_name(nome).ok()?;
        let mut s = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut s).ok()?;
        Some(String::from_utf8_lossy(&s).into_owned())
    }

    #[test]
    fn il_word_ha_testa_campi_e_bozza_finche_non_e_confermato() {
        let mut logo = Vec::new();
        image::DynamicImage::new_rgb8(300, 100).write_to(&mut std::io::Cursor::new(&mut logo), image::ImageFormat::Png).unwrap();
        let intest = Intestazione { clinica: "Laboratorio Estense".into(), logo: Some(logo), caso: Some("ISTO-2026-114".into()) };
        let s = [seg("Specie gatto. Diagnosi: carcinoma squamocellulare.")];
        let bozza = docx(Path::new("dettato.m4a"), 95.0, "it", &s, &intest, Some("BOZZA — 1 punto da riascoltare: [00:00] «carcinoma»"));
        let doc = parte_zip(&bozza, "word/document.xml").unwrap();
        assert!(doc.contains("ISTO-2026-114") && doc.contains("Gatto") && doc.contains("Carcinoma squamocellulare."), "{doc}");
        assert!(doc.contains("1 punto da riascoltare") && !doc.contains("Firma del refertante"));
        let testa = parte_zip(&bozza, "word/header1.xml").unwrap();
        assert!(testa.contains("Laboratorio Estense") && testa.contains("BOZZA") && testa.contains("rIdLogo"));
        // Logo 3:1, alto 1,5 cm: largo 4,5 cm.
        assert!(testa.contains("cx=\"1620000\" cy=\"540000\""), "{testa}");
        assert!(parte_zip(&bozza, "word/media/logo.png").is_some());
        assert!(parte_zip(&bozza, "word/footer1.xml").unwrap().contains("NUMPAGES"));

        let finale = docx(Path::new("dettato.m4a"), 95.0, "it", &s, &Intestazione::default(), None);
        let doc = parte_zip(&finale, "word/document.xml").unwrap();
        assert!(doc.contains("Firma del refertante"));
        assert!(!parte_zip(&finale, "word/header1.xml").unwrap().contains("BOZZA"));
        assert!(parte_zip(&finale, "word/media/logo.png").is_none());

        let t = String::from_utf8(testo(Path::new("dettato.m4a"), 95.0, &s, &intest, None)).unwrap();
        assert!(t.starts_with("Laboratorio Estense") && t.contains("DIAGNOSI") && t.contains("Specie: Gatto") && t.contains("Firma"), "{t}");
    }

    /// Un referto d'esempio da guardare: `CEDRO_REFERTO_OUT=cartella cargo test --lib referto_esempio -- --ignored`.
    #[test]
    #[ignore]
    fn referto_esempio_a_mano() {
        let dir = std::path::PathBuf::from(std::env::var("CEDRO_REFERTO_OUT").unwrap());
        let logo = std::env::var("CEDRO_REFERTO_LOGO").ok().and_then(|p| std::fs::read(p).ok());
        let intest = Intestazione { clinica: "Laboratorio di Anatomia Patologica Veterinaria Estense".into(), logo, caso: Some("ISTO-2026-114".into()) };
        let s = [
            seg("Specie: cane, meticcio, maschio di 9 anni. Campione, nodulo cutaneo del fianco sinistro."),
            seg("Anamnesi. Nodulo a crescita rapida comparso da circa due mesi, punto e a capo"),
            seg("Descrizione macroscopica. Campione di cute di 3 per 2,5 cm con nodulo rilevato di 1,8 cm. Al taglio superficie biancastra, consistenza duro-elastica."),
            seg("Descrizione microscopica: proliferazione di cellule rotonde disposte in nidi e cordoni, con granuli metacromatici. Si osservano 4 mitosi in 10 campi a forte ingrandimento."),
            seg("Margini: indenni, distanza minima 4 millimetri."),
            seg("Diagnosi di mastocitoma cutaneo, grado 2 secondo Patnaik, basso grado secondo Kiupel."),
            seg("Commento. Si consiglia stadiazione e controllo a tre mesi."),
        ];
        let draft = Some("BOZZA — 2 punti da riascoltare: [00:41] «4 mitosi in 10», [01:02] «grado 2»");
        std::fs::write(dir.join("referto - bozza.docx"), docx(Path::new("dettato 114.m4a"), 128.0, "it", &s, &intest, draft)).unwrap();
        std::fs::write(dir.join("referto - rivisto.docx"), docx(Path::new("dettato 114.m4a"), 128.0, "it", &s, &intest, None)).unwrap();
        std::fs::write(dir.join("referto - rivisto.txt"), testo(Path::new("dettato 114.m4a"), 128.0, &s, &intest, None)).unwrap();
    }
}
