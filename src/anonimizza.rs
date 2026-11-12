//! «Togli dati nascosti» per PDF e documenti Office/OpenDocument: autore, chi ha modificato, azienda,
//! date, programma, commenti firmati e revisioni con il nome di chi le ha fatte, metadati XMP e
//! JavaScript. Il testo che si vede nel documento non si tocca: se il nome è scritto lì, resta (lo si
//! dice nel messaggio). Per le foto vale `pulisci`, per i DICOM `dicom::anon`.

use std::io::{Cursor, Read, Write};

/// Cosa è stato tolto, in parole, senza doppioni.
#[derive(Default, Debug)]
pub struct Fatto {
    pub tolti: Vec<&'static str>,
    pub avvisi: Vec<String>,
}

impl Fatto {
    fn tolto(&mut self, cosa: &'static str) {
        if !self.tolti.contains(&cosa) {
            self.tolti.push(cosa);
        }
    }

    pub fn frase(&self) -> String {
        let mut s = if self.tolti.is_empty() { "Nessun dato nascosto trovato".to_string() } else { format!("Tolti: {}", self.tolti.join(", ")) };
        s.push_str(". Il testo del documento resta com'è: se il nome è scritto lì, va tolto a mano.");
        for a in &self.avvisi {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// Le estensioni che questo modulo pulisce.
pub fn gestito(ext: &str) -> bool {
    matches!(ext, "pdf" | "docx" | "xlsx" | "pptx" | "docm" | "xlsm" | "pptm" | "odt" | "ods" | "odp")
}

/// Il file ripulito e cosa è stato tolto.
pub fn pulisci(ext: &str, bytes: &[u8]) -> Result<(Vec<u8>, Fatto), String> {
    match ext {
        "pdf" => pdf(bytes),
        "docx" | "xlsx" | "pptx" | "docm" | "xlsm" | "pptm" | "odt" | "ods" | "odp" => office(bytes),
        _ => Err("formato non gestito".into()),
    }
}

// ---------------------------------------------------------------- PDF

fn pdf(bytes: &[u8]) -> Result<(Vec<u8>, Fatto), String> {
    use lopdf::{Document, Object};
    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF non letto: {e}"))?;
    if doc.is_encrypted() {
        return Err("PDF protetto da password: togli prima la password e riprova".into());
    }
    let mut fatto = Fatto::default();

    // Le informazioni del documento (autore, titolo, programma, date).
    if let Ok(info) = doc.trailer.get(b"Info").and_then(Object::as_reference) {
        if let Ok(Object::Dictionary(d)) = doc.get_object(info) {
            for (k, cosa) in [
                (&b"Author"[..], "autore"),
                (b"Creator", "programma"),
                (b"Producer", "programma"),
                (b"CreationDate", "date di creazione e modifica"),
                (b"ModDate", "date di creazione e modifica"),
                (b"Title", "titolo"),
                (b"Subject", "oggetto"),
                (b"Keywords", "parole chiave"),
            ] {
                if d.has(k) {
                    fatto.tolto(cosa);
                }
            }
            if d.iter().any(|(k, _)| !matches!(k.as_slice(), b"Author" | b"Creator" | b"Producer" | b"CreationDate" | b"ModDate" | b"Title" | b"Subject" | b"Keywords" | b"Trapped")) {
                fatto.tolto("altre informazioni del documento");
            }
        }
        doc.objects.remove(&info);
    }
    doc.trailer.remove(b"Info");

    // Su tutti gli oggetti: XMP, dati privati dei programmi, autori e date dei commenti, JavaScript.
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let mut xmp_streams = Vec::new();
    let mut firmato = false;
    for id in ids {
        let Some(obj) = doc.objects.get_mut(&id) else { continue };
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => {
                if s.dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Metadata") {
                    xmp_streams.push(id);
                    continue;
                }
                &mut s.dict
            }
            _ => continue,
        };
        if dict.remove(b"Metadata").is_some() {
            fatto.tolto("metadati XMP");
        }
        if dict.remove(b"PieceInfo").is_some() {
            fatto.tolto("dati privati dei programmi");
        }
        let is_annot = dict.get(b"Subtype").is_ok() && (dict.has(b"Rect") || dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Annot"));
        // Nei campi dei moduli (Widget) /T è il nome del campo, non l'autore: senza, il modulo non va più.
        let widget = dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Widget");
        if is_annot && !widget {
            if dict.remove(b"T").is_some() {
                fatto.tolto("autori dei commenti");
            }
            if dict.remove(b"M").is_some() | dict.remove(b"CreationDate").is_some() {
                fatto.tolto("date dei commenti");
            }
        }
        if dict.get(b"S").and_then(Object::as_name).is_ok_and(|n| n == b"JavaScript") || dict.remove(b"JS").is_some() {
            dict.remove(b"JS");
            fatto.tolto("JavaScript");
        }
        if dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Sig") || (dict.has(b"ByteRange") && dict.has(b"Contents") && dict.has(b"Filter")) {
            // Una firma digitale dopo la pulizia non sarebbe più valida e porta il nome del firmatario,
            // anche dentro il certificato (/Contents e /Cert): va via tutta.
            for k in [&b"Name"[..], b"Location", b"Reason", b"ContactInfo", b"M", b"Contents", b"Cert", b"ByteRange", b"Prop_Build", b"Reference"] {
                if dict.remove(k).is_some() {
                    fatto.tolto("firma digitale con il certificato del firmatario");
                    firmato = true;
                }
            }
        }
        // Il campo della firma resta, senza firma.
        if dict.get(b"FT").and_then(Object::as_name).is_ok_and(|n| n == b"Sig") && dict.remove(b"V").is_some() {
            fatto.tolto("firma digitale con il certificato del firmatario");
            firmato = true;
        }
        // Gli allegati restano (sono contenuto), ma vanno controllati.
        if dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Filespec") && dict.has(b"EF") {
            let avviso = "Il PDF contiene allegati: non sono stati aperti, controllali.".to_string();
            if !fatto.avvisi.contains(&avviso) {
                fatto.avvisi.push(avviso);
            }
        }
    }
    for id in xmp_streams {
        doc.objects.remove(&id);
        fatto.tolto("metadati XMP");
    }
    // Il catalogo: niente azioni all'apertura né nomi di JavaScript.
    if let Ok(cat) = doc.catalog_mut() {
        if cat.remove(b"OpenAction").is_some() {
            fatto.tolto("azioni all'apertura");
        }
        cat.remove(b"Metadata");
        cat.remove(b"PieceInfo");
        // I permessi legati a una firma (DocMDP) rimandano alla firma tolta.
        if firmato {
            cat.remove(b"Perms");
        }
    }
    if firmato {
        fatto.avvisi.push("Il PDF era firmato: la firma è stata tolta, perché dopo la pulizia non sarebbe più valida.".into());
    }
    if let Ok(names) = doc.catalog().and_then(|c| c.get(b"Names")).and_then(Object::as_reference) {
        if let Ok(Object::Dictionary(n)) = doc.get_object_mut(names) {
            if n.remove(b"JavaScript").is_some() {
                fatto.tolto("JavaScript");
            }
        }
    } else if let Ok(cat) = doc.catalog_mut() {
        if let Ok(Object::Dictionary(n)) = cat.get_mut(b"Names") {
            if n.remove(b"JavaScript").is_some() {
                fatto.tolto("JavaScript");
            }
        }
    }
    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("PDF non scritto: {e}"))?;
    Ok((out, fatto))
}

// ---------------------------------------------------------------- Office e OpenDocument

/// Le parti da togliere del tutto: proprietà personalizzate, elenco delle persone, miniatura.
fn parte_da_togliere(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if n == "docprops/custom.xml" {
        Some("proprietà personalizzate")
    } else if n.ends_with("/people.xml") || n.contains("/persons/") {
        Some("elenco delle persone")
    } else if n == "docprops/thumbnail.jpeg" || n == "docprops/thumbnail.wmf" || n == "thumbnails/thumbnail.png" {
        Some("miniatura")
    } else {
        None
    }
}

fn office(bytes: &[u8]) -> Result<(Vec<u8>, Fatto), String> {
    let mut zin = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("documento non letto: {e}"))?;
    let mut fatto = Fatto::default();
    let mut out = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut out);
        for i in 0..zin.len() {
            let mut f = zin.by_index(i).map_err(|e| format!("documento non letto: {e}"))?;
            let name = f.name().map_err(|e| e.to_string())?.to_string();
            if f.is_dir() {
                continue;
            }
            if let Some(cosa) = parte_da_togliere(&name) {
                fatto.tolto(cosa);
                continue;
            }
            let mut data = Vec::new();
            f.read_to_end(&mut data).map_err(|e| format!("documento non letto: {e}"))?;
            // Il «mimetype» degli OpenDocument va primo e senza compressione.
            let method = if name == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
            if name.ends_with(".xml") || name.ends_with(".rels") {
                if let Ok(text) = String::from_utf8(data.clone()) {
                    let pulito = xml(&name, &text, &mut fatto);
                    data = pulito.into_bytes();
                }
            }
            // Le foto dentro il documento portano i loro dati (GPS, fotocamera, data): si puliscono
            // come le foto sciolte, senza ricomprimerle.
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
                if let Some(Ok((pulita, tolto))) = crate::pulisci::pulisci(&ext, &data) {
                    if pulita != data {
                        fatto.tolto(if tolto.gps { "GPS e dati delle foto" } else { "dati delle foto" });
                        data = pulita;
                    }
                }
            }
            let opts = zip::write::SimpleFileOptions::default().compression_method(method);
            zw.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            zw.write_all(&data).map_err(|e| e.to_string())?;
        }
        zw.finish().map_err(|e| e.to_string())?;
    }
    // Il riferimento alle parti tolte resta in [Content_Types].xml e nei .rels: Office lo tollera, ma si
    // toglie lo stesso per non lasciare il file «da riparare».
    let pulito = togli_riferimenti(out.into_inner())?;
    Ok((pulito, fatto))
}

/// Svuota il testo dentro gli elementi `tags` (`<dc:creator>Mario</dc:creator>` → `<dc:creator></dc:creator>`).
fn svuota(text: &str, tag: &str) -> (String, bool) {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut cambiato = false;
    while let Some(i) = rest.find(&open) {
        // Che sia proprio quel tag (non `<dc:creatorX`).
        let after = &rest[i + open.len()..];
        if !after.starts_with('>') && !after.starts_with(' ') && !after.starts_with('/') {
            out.push_str(&rest[..i + open.len()]);
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            out.push_str(&rest[..i + open.len() + gt + 1]);
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        out.push_str(&rest[..i + open.len() + gt + 1]);
        if !body[..end].trim().is_empty() {
            cambiato = true;
        }
        out.push_str(&close);
        rest = &body[end + close.len()..];
    }
    out.push_str(rest);
    (out, cambiato)
}

/// Sostituisce il valore degli attributi `attr="…"` con `value`.
fn attributo(text: &str, attr: &str, value: &str) -> (String, bool) {
    let pat = format!(" {attr}=\"");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut cambiato = false;
    while let Some(i) = rest.find(&pat) {
        let start = i + pat.len();
        let Some(end) = rest[start..].find('"') else { break };
        out.push_str(&rest[..start]);
        if &rest[start..start + end] != value {
            cambiato = true;
        }
        out.push_str(value);
        out.push('"');
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    (out, cambiato)
}

fn xml(name: &str, text: &str, fatto: &mut Fatto) -> String {
    let mut t = text.to_string();
    let n = name.to_ascii_lowercase();
    let via = |t: &mut String, tag: &str, cosa: &'static str, fatto: &mut Fatto| {
        let (s, c) = svuota(t, tag);
        if c {
            fatto.tolto(cosa);
        }
        *t = s;
    };
    if n == "docprops/core.xml" || n == "meta.xml" {
        for (tag, cosa) in [
            ("dc:creator", "autore"),
            ("cp:lastModifiedBy", "ultimo a modificare"),
            ("meta:initial-creator", "autore"),
            ("dc:title", "titolo"),
            ("dc:subject", "oggetto"),
            ("dc:description", "descrizione"),
            ("cp:keywords", "parole chiave"),
            ("meta:keyword", "parole chiave"),
            ("cp:category", "categoria"),
            ("dcterms:created", "date di creazione e modifica"),
            ("dcterms:modified", "date di creazione e modifica"),
            ("cp:lastPrinted", "date di creazione e modifica"),
            ("meta:creation-date", "date di creazione e modifica"),
            ("dc:date", "date di creazione e modifica"),
            ("meta:print-date", "date di creazione e modifica"),
            ("meta:printed-by", "chi ha stampato"),
            ("meta:generator", "programma"),
            ("cp:revision", "numero di revisioni"),
            ("meta:editing-cycles", "numero di revisioni"),
            ("meta:editing-duration", "tempo di modifica"),
            ("meta:template", "modello"),
        ] {
            via(&mut t, tag, cosa, fatto);
        }
        // Le proprietà personalizzate degli OpenDocument.
        let (s, c) = svuota(&t, "meta:user-defined");
        if c {
            fatto.tolto("proprietà personalizzate");
        }
        t = s;
    }
    if n == "docprops/app.xml" {
        for (tag, cosa) in [("Company", "azienda"), ("Manager", "responsabile"), ("Template", "modello"), ("HyperlinkBase", "indirizzo dei collegamenti"), ("TotalTime", "tempo di modifica")] {
            via(&mut t, tag, cosa, fatto);
        }
    }
    // Commenti e revisioni: il nome di chi li ha scritti diventa «Autore», le iniziali «A».
    for (attr, value, cosa) in [
        ("w:author", "Autore", "autori di commenti e revisioni"),
        ("w:initials", "A", "autori di commenti e revisioni"),
        ("w:date", "2000-01-01T00:00:00Z", "date di commenti e revisioni"),
        ("w15:userId", "Autore", "autori di commenti e revisioni"),
        ("w15:providerId", "None", "autori di commenti e revisioni"),
        ("authorId", "0", "autori di commenti e revisioni"),
        ("userId", "Autore", "autori di commenti e revisioni"),
        ("displayName", "Autore", "autori di commenti e revisioni"),
        ("p:name", "Autore", "autori di commenti e revisioni"),
        ("providerId", "None", "autori di commenti e revisioni"),
        // Chi ha protetto la cartella di Excel (fileSharing) e le revisioni condivise.
        ("userName", "Autore", "nomi utente"),
    ] {
        let (s, c) = attributo(&t, attr, value);
        if c {
            fatto.tolto(cosa);
        }
        t = s;
    }
    if n.starts_with("xl/comments") {
        let (s, c) = svuota(&t, "author");
        if c {
            fatto.tolto("autori di commenti e revisioni");
        }
        t = s;
    }
    // PowerPoint: gli autori dei commenti, vecchi (commentAuthors) e nuovi (authors), con le iniziali.
    if n.starts_with("ppt/commentauthors") || n.starts_with("ppt/authors") {
        for (attr, value) in [("name", "Autore"), ("initials", "A")] {
            let (s, c) = attributo(&t, attr, value);
            if c {
                fatto.tolto("autori di commenti e revisioni");
            }
            t = s;
        }
    }
    if n.ends_with(".rels") {
        t = collegamenti(&t, fatto);
    }
    if n == "content.xml" || n == "styles.xml" {
        for tag in ["dc:creator", "dc:date"] {
            let (s, c) = svuota(&t, tag);
            if c {
                fatto.tolto("autori di commenti e revisioni");
            }
            t = s;
        }
    }
    // L'Excel ricorda il percorso in cui era salvato (con il nome utente di Windows).
    if n == "xl/workbook.xml" {
        if let Some(i) = t.find("<x15ac:absPath") {
            if let Some(j) = t[i..].find("/>") {
                t.replace_range(i..i + j + 2, "");
                fatto.tolto("percorso del file sul PC");
            }
        }
    }
    t
}

/// Nei .rels il modello collegato (di solito Normal.dotm) è un percorso del PC con il nome utente di
/// Windows: resta solo il nome del modello. Gli altri collegamenti a file del PC sono contenuto: restano,
/// ma si dice di controllarli.
fn collegamenti(text: &str, fatto: &mut Fatto) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("<Relationship ") {
        let Some(j) = rest[i..].find('>') else { break };
        out.push_str(&rest[..i]);
        let el = &rest[i..i + j + 1];
        let external = value_of(el, "TargetMode") == Some("External");
        let target = value_of(el, "Target").unwrap_or("");
        if external && value_of(el, "Type").is_some_and(|t| t.ends_with("/attachedTemplate")) {
            let nome = target.rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).unwrap_or("Normal.dotm");
            if nome != target {
                out.push_str(&el.replacen(&format!("Target=\"{target}\""), &format!("Target=\"{nome}\""), 1));
                fatto.tolto("percorso del modello sul PC");
            } else {
                out.push_str(el);
            }
        } else {
            if external && target.to_ascii_lowercase().starts_with("file:") {
                let avviso = "Il documento ha collegamenti a file del computer (il percorso può contenere il nome utente): controllali.".to_string();
                if !fatto.avvisi.contains(&avviso) {
                    fatto.avvisi.push(avviso);
                }
            }
            out.push_str(el);
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// Toglie da [Content_Types].xml e dai .rels i riferimenti alle parti eliminate.
fn togli_riferimenti(zip_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut zin = zip::ZipArchive::new(Cursor::new(&zip_bytes)).map_err(|e| e.to_string())?;
    let present: Vec<String> = zin.file_names().filter_map(|s| s.ok().map(|s| s.to_string())).collect();
    let tolta = |target: &str| -> bool {
        let t = target.trim_start_matches('/').to_ascii_lowercase();
        parte_da_togliere(&t).is_some() && !present.iter().any(|p| p.to_ascii_lowercase() == t)
    };
    let mut out = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut out);
        for i in 0..zin.len() {
            let mut f = zin.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().map_err(|e| e.to_string())?.to_string();
            let mut data = Vec::new();
            f.read_to_end(&mut data).map_err(|e| e.to_string())?;
            // Anche il manifest degli OpenDocument elenca le parti (la miniatura).
            if name == "[Content_Types].xml" || name.ends_with(".rels") || name == "META-INF/manifest.xml" {
                if let Ok(text) = String::from_utf8(data.clone()) {
                    let mut kept = String::with_capacity(text.len());
                    let mut rest = text.as_str();
                    // Ogni elemento <Override PartName="…"/> o <Relationship … Target="…"/> che punta a una parte tolta va via.
                    while let Some(i) = rest.find('<') {
                        kept.push_str(&rest[..i]);
                        let Some(j) = rest[i..].find('>') else {
                            kept.push_str(&rest[i..]);
                            rest = "";
                            break;
                        };
                        let el = &rest[i..i + j + 1];
                        let target = value_of(el, "PartName").or_else(|| value_of(el, "Target")).or_else(|| value_of(el, "manifest:full-path"));
                        let voce = el.starts_with("<Override") || el.starts_with("<Relationship ") || (el.starts_with("<manifest:file-entry") && el.ends_with("/>"));
                        let drop = voce && target.is_some_and(|t| tolta(&resolve(&name, t)));
                        if !drop {
                            kept.push_str(el);
                        }
                        rest = &rest[i + j + 1..];
                    }
                    kept.push_str(rest);
                    data = kept.into_bytes();
                }
            }
            let method = if name == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
            zw.start_file(name.as_str(), zip::write::SimpleFileOptions::default().compression_method(method)).map_err(|e| e.to_string())?;
            zw.write_all(&data).map_err(|e| e.to_string())?;
        }
        zw.finish().map_err(|e| e.to_string())?;
    }
    Ok(out.into_inner())
}

fn value_of<'a>(el: &'a str, attr: &str) -> Option<&'a str> {
    let pat = format!("{attr}=\"");
    let i = el.find(&pat)? + pat.len();
    let j = el[i..].find('"')?;
    Some(&el[i..i + j])
}

/// Il percorso di una parte a cui punta un .rels (relativo alla cartella della parte che lo possiede).
fn resolve(rels_name: &str, target: &str) -> String {
    if target.starts_with('/') || rels_name == "[Content_Types].xml" || rels_name == "META-INF/manifest.xml" {
        return target.trim_start_matches('/').to_string();
    }
    // «word/_rels/document.xml.rels» descrive «word/document.xml»: la base è «word/».
    let base = rels_name.split("_rels/").next().unwrap_or("");
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod prove {
    use super::*;

    fn docx() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            let mut put = |n: &str, s: &str| {
                zw.start_file(n, o).unwrap();
                zw.write_all(s.as_bytes()).unwrap();
            };
            put("[Content_Types].xml", r#"<?xml version="1.0"?><Types><Override PartName="/word/document.xml" ContentType="x"/><Override PartName="/docProps/custom.xml" ContentType="y"/><Override PartName="/docProps/core.xml" ContentType="z"/></Types>"#);
            put("_rels/.rels", r#"<Relationships><Relationship Id="r1" Target="word/document.xml"/><Relationship Id="r2" Target="docProps/custom.xml"/></Relationships>"#);
            put("docProps/core.xml", r#"<cp:coreProperties><dc:creator>Mario Rossi</dc:creator><cp:lastModifiedBy>Laura Bianchi</cp:lastModifiedBy><dcterms:created xsi:type="x">2024-03-15T10:00:00Z</dcterms:created><dc:title/></cp:coreProperties>"#);
            put("docProps/app.xml", r#"<Properties><Company>Clinica San Rocco</Company><Application>Microsoft Office Word</Application></Properties>"#);
            put("docProps/custom.xml", r#"<Properties><property name="Paziente"><vt:lpwstr>Fido</vt:lpwstr></property></Properties>"#);
            put("word/document.xml", r#"<w:document><w:body><w:p><w:ins w:id="1" w:author="Laura Bianchi" w:date="2024-03-16T09:00:00Z"><w:r><w:t>Referto istologico</w:t></w:r></w:ins></w:p></w:body></w:document>"#);
            put("word/comments.xml", r#"<w:comments><w:comment w:id="0" w:author="Mario Rossi" w:initials="MR"><w:p><w:r><w:t>ok</w:t></w:r></w:p></w:comment></w:comments>"#);
            zw.finish().unwrap();
        }
        out.into_inner()
    }

    fn tutto(zip_bytes: &[u8]) -> String {
        let mut z = zip::ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
        let mut s = String::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            s.push_str(&format!("== {}\n", f.name().unwrap()));
            f.read_to_string(&mut s).unwrap();
        }
        s
    }

    #[test]
    fn un_docx_perde_autori_azienda_e_proprieta_ma_non_il_testo() {
        let (out, fatto) = pulisci("docx", &docx()).unwrap();
        let s = tutto(&out);
        for nome in ["Mario Rossi", "Laura Bianchi", "Clinica San Rocco", "Fido", "MR\"", "2024-03"] {
            assert!(!s.contains(nome), "è rimasto «{nome}»:\n{s}");
        }
        assert!(s.contains("Referto istologico"));
        assert!(!s.contains("docProps/custom.xml"), "{s}");
        assert!(s.contains("Microsoft Office Word"));
        let frase = fatto.frase();
        assert!(frase.contains("autore") && frase.contains("azienda") && frase.contains("proprietà personalizzate"), "{frase}");
    }

    #[test]
    fn un_pdf_perde_autore_programma_xmp_e_autori_dei_commenti() {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf 72 720 Td (Referto) Tj ET".to_vec()));
        let annot = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()], "T" => Object::string_literal("Mario Rossi"), "Contents" => Object::string_literal("da rivedere") });
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Contents" => content, "Annots" => vec![annot.into()] });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let xmp = doc.add_object(Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, b"<x:xmpmeta>Mario Rossi</x:xmpmeta>".to_vec()));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "Metadata" => xmp });
        doc.trailer.set("Root", catalog);
        let info = doc.add_object(dictionary! { "Author" => Object::string_literal("Mario Rossi"), "Producer" => Object::string_literal("Programma 1.0") });
        doc.trailer.set("Info", info);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let (out, fatto) = pulisci("pdf", &bytes).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(!s.contains("Mario Rossi"), "{s}");
        assert!(!s.contains("Programma 1.0"));
        assert!(s.contains("da rivedere"));
        let frase = fatto.frase();
        assert!(frase.contains("autore") && frase.contains("metadati XMP") && frase.contains("autori dei commenti"), "{frase}");
        Document::load_mem(&out).unwrap();
    }

    fn zip_di(parti: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut out);
            for (n, b) in parti {
                zw.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                zw.write_all(b).unwrap();
            }
            zw.finish().unwrap();
        }
        out.into_inner()
    }

    fn parte(zip_bytes: &[u8], nome: &str) -> Option<Vec<u8>> {
        let mut z = zip::ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
        let mut f = z.by_name(nome).ok()?;
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        Some(b)
    }

    #[test]
    fn un_pptx_perde_nomi_e_iniziali_degli_autori_dei_commenti() {
        let z = zip_di(&[
            ("[Content_Types].xml", br#"<Types/>"#),
            ("ppt/authors.xml", br#"<p188:authorLst><p188:author id="{1}" name="Mario Rossi" initials="MR" userId="mario@clinica.it" providerId="AD"/></p188:authorLst>"#),
            ("ppt/commentAuthors.xml", br#"<p:cmAuthorLst><p:cmAuthor id="0" name="Laura Bianchi" initials="LB" lastIdx="1" clrIdx="0"/></p:cmAuthorLst>"#),
            ("ppt/slides/slide1.xml", br#"<p:sld><p:cSld name="Titolo"/></p:sld>"#),
        ]);
        let (out, fatto) = pulisci("pptx", &z).unwrap();
        let s = tutto(&out);
        for nome in ["Mario Rossi", "\"MR\"", "Laura Bianchi", "\"LB\"", "mario@clinica.it"] {
            assert!(!s.contains(nome), "è rimasto «{nome}»:\n{s}");
        }
        // Il nome di una diapositiva non è una persona: resta.
        assert!(s.contains(r#"name="Titolo""#), "{s}");
        assert!(fatto.frase().contains("autori di commenti"));
    }

    #[test]
    fn modello_collegato_e_nomi_utente_di_excel_non_restano() {
        let rels = br#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/attachedTemplate" Target="file:///C:\Users\mario.rossi\AppData\Roaming\Microsoft\Templates\Normal.dotm" TargetMode="External"/><Relationship Id="rId2" Type="http://x/hyperlink" Target="https://www.clinica.it" TargetMode="External"/></Relationships>"#;
        let (out, fatto) = pulisci("docx", &zip_di(&[("[Content_Types].xml", br#"<Types/>"#), ("word/_rels/settings.xml.rels", rels)])).unwrap();
        let s = tutto(&out);
        assert!(!s.contains("mario.rossi"), "{s}");
        assert!(s.contains(r#"Target="Normal.dotm""#) && s.contains("https://www.clinica.it"), "{s}");
        assert!(fatto.frase().contains("modello"), "{}", fatto.frase());
        let z = zip_di(&[
            ("[Content_Types].xml", br#"<Types/>"#),
            ("xl/workbook.xml", br#"<workbook><fileSharing readOnlyRecommended="1" userName="Mario Rossi"/></workbook>"#),
            ("xl/revisions/revisionHeaders.xml", br#"<headers><header guid="{1}" dateTime="2024-03-15T10:00:00" userName="Laura Bianchi"/></headers>"#),
        ]);
        let s = tutto(&pulisci("xlsx", &z).unwrap().0);
        assert!(!s.contains("Mario Rossi") && !s.contains("Laura Bianchi"), "{s}");
    }

    #[test]
    fn le_foto_dentro_il_documento_perdono_il_gps() {
        let mut jpg = Vec::new();
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 100, 50])).write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg).unwrap();
        // Un blocco EXIF subito dopo l'inizio, con un segno riconoscibile.
        let mut exif = b"Exif\0\0II*\0\x08\0\0\0\0\0\0\0\0\0GPS-45.4642N-9.19".to_vec();
        let len = (exif.len() + 2) as u16;
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend_from_slice(&len.to_be_bytes());
        app1.append(&mut exif);
        jpg.splice(2..2, app1);
        let z = zip_di(&[("[Content_Types].xml", br#"<Types/>"#), ("word/media/image1.jpeg", &jpg)]);
        let (out, fatto) = pulisci("docx", &z).unwrap();
        let foto = parte(&out, "word/media/image1.jpeg").unwrap();
        assert!(!foto.windows(12).any(|w| w == b"GPS-45.4642N"));
        assert!(image::load_from_memory(&foto).is_ok());
        assert!(fatto.frase().contains("foto"), "{}", fatto.frase());
    }

    #[test]
    fn la_miniatura_degli_opendocument_esce_anche_dal_manifest() {
        let manifest = br#"<manifest:manifest><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="Thumbnails/thumbnail.png" manifest:media-type="image/png"/></manifest:manifest>"#;
        let z = zip_di(&[("mimetype", b"application/vnd.oasis.opendocument.text"), ("META-INF/manifest.xml", manifest), ("content.xml", b"<office:document-content/>"), ("Thumbnails/thumbnail.png", b"PNG")]);
        let (out, _) = pulisci("odt", &z).unwrap();
        assert!(parte(&out, "Thumbnails/thumbnail.png").is_none());
        let m = String::from_utf8(parte(&out, "META-INF/manifest.xml").unwrap()).unwrap();
        assert!(!m.contains("thumbnail") && m.contains("content.xml"), "{m}");
    }

    fn pdf_con(extra: impl FnOnce(&mut lopdf::Document, lopdf::ObjectId) -> Vec<(&'static str, lopdf::Object)>) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Contents" => content });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let mut cat = dictionary! { "Type" => "Catalog", "Pages" => pages };
        for (k, v) in extra(&mut doc, page) {
            cat.set(k, v);
        }
        let catalog = doc.add_object(cat);
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn i_campi_dei_moduli_tengono_il_loro_nome() {
        use lopdf::{dictionary, Object};
        let bytes = pdf_con(|doc, page| {
            let field = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "P" => page, "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()], "T" => Object::string_literal("nome_paziente"), "V" => Object::string_literal("") });
            vec![("AcroForm", Object::Dictionary(dictionary! { "Fields" => vec![field.into()] }))]
        });
        let (out, _) = pulisci("pdf", &bytes).unwrap();
        assert!(String::from_utf8_lossy(&out).contains("nome_paziente"));
    }

    #[test]
    fn la_firma_digitale_va_via_con_il_certificato() {
        use lopdf::{dictionary, Object, StringFormat};
        let bytes = pdf_con(|doc, page| {
            // /Contents della firma: il PKCS#7 con il certificato (qui il CN in chiaro).
            let sig = doc.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite", "Name" => Object::string_literal("Mario Rossi"), "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()], "Contents" => Object::String(b"CN=Mario Rossi, O=Clinica".to_vec(), StringFormat::Literal) });
            let field = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig", "P" => page, "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()], "T" => Object::string_literal("Firma1"), "V" => sig });
            vec![("AcroForm", Object::Dictionary(dictionary! { "Fields" => vec![field.into()], "SigFlags" => 3 })), ("Perms", Object::Dictionary(dictionary! { "DocMDP" => sig }))]
        });
        let (out, fatto) = pulisci("pdf", &bytes).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(!s.contains("Mario Rossi") && s.contains("Firma1"), "{s}");
        assert!(fatto.frase().contains("firma"), "{}", fatto.frase());
        lopdf::Document::load_mem(&out).unwrap();
    }
}
