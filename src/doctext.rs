use crate::zipstore::zip_archive;
use std::fs;
use std::io::Read;
use std::path::Path;

pub fn document_text(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = match ext.as_str() {
        "docx" => docx_text(path)?,
        "odt" => odt_text(path)?,
        "rtf" => rtf_text(&fs::read(path).map_err(|e| e.to_string())?),
        "doc" => doc_text(path)?,
        _ => return Err("formato di documento non letto".into()),
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err("il documento non contiene testo".into())
    } else {
        Ok(trimmed.to_string())
    }
}

/// I paragrafi del documento con il livello dei titoli (1-3): da DOCX (stili Heading/Titolo e struttura),
/// ODT (`text:h`) e RTF (stili «heading N» e `\\outlinelevel`). Per DOC e per quello che non ha titoli, righe di testo.
pub fn document_blocks(path: &Path) -> Result<Vec<crate::pdfdoc::DocBlock>, String> {
    use crate::pdfdoc::DocBlock;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut blocks = match ext.as_str() {
        "docx" => {
            // Le immagini: r:embed → word/_rels/document.xml.rels → word/media/….
            let rels = zip_entry(path, "word/_rels/document.xml.rels").unwrap_or_default();
            let media = |id: &str, size: Option<(f32, f32)>, align: u8| {
                let rel = elements_self(&rels, "Relationship").into_iter().find(|r| attr_text(r, "Id").as_deref() == Some(id))?;
                if attr_text(rel, "TargetMode").as_deref() == Some("External") {
                    return None;
                }
                let target = attr_text(rel, "Target")?;
                let name = match target.strip_prefix('/') {
                    Some(abs) => abs.to_string(),
                    None => format!("word/{target}"),
                };
                crate::pdfdoc::DocImage::from_bytes(&zip_bytes(path, &name).ok()?, size, align)
            };
            docx_blocks(&zip_entry(path, "word/document.xml")?, &zip_entry(path, "word/styles.xml").unwrap_or_default(), &media)
        }
        "odt" => {
            let media = |href: &str, size: Option<(f32, f32)>, align: u8| crate::pdfdoc::DocImage::from_bytes(&zip_bytes(path, href.trim_start_matches("./")).ok()?, size, align);
            odt_blocks(&zip_entry(path, "content.xml")?, &media)
        }
        "rtf" => rtf_blocks(&fs::read(path).map_err(|e| e.to_string())?),
        _ => Vec::new(),
    };
    if blocks.iter().all(|b| b.text.trim().is_empty() && b.image.is_none()) {
        blocks = document_text(path)?.split('\n').map(|l| DocBlock { text: l.trim_end().to_string(), heading: 0, image: None }).collect();
    }
    // Le righe vuote di troppo in fondo non servono.
    while blocks.last().is_some_and(|b| b.text.trim().is_empty() && b.image.is_none()) {
        blocks.pop();
    }
    Ok(blocks)
}

/// Il livello (1-3) di un titolo dal nome dello stile: «heading 2», «Titolo 1», «Title».
fn heading_level_of(name: &str) -> Option<u8> {
    let n = name.trim().to_lowercase();
    let rest = n.strip_prefix("heading").or_else(|| n.strip_prefix("titolo"))?;
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.parse::<u8>() {
        Ok(d) if d >= 1 => Some(d.min(3)),
        _ if rest.trim().is_empty() => Some(1),
        _ => None,
    }
}

fn attr_text(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// La posizione del prossimo `<tag` (seguito da `>`, spazio o `/`) da `from`.
fn find_open(xml: &str, from: usize, open: &str) -> Option<usize> {
    let mut at = from;
    while let Some(found) = xml[at..].find(open) {
        let start = at + found;
        if matches!(xml[start + open.len()..].chars().next(), Some('>' | ' ' | '/')) {
            return Some(start);
        }
        at = start + open.len();
    }
    None
}

/// Le porzioni di `xml` da `<tag` (seguito da `>`, spazio o `/`) fino alla chiusura `</tag>` corrispondente:
/// un `<w:p>` dentro una casella di testo non chiude il paragrafo che la contiene.
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = find_open(xml, at, &open) {
        let tag_end = xml[start..].find('>').map_or(xml.len(), |e| start + e + 1);
        if xml[start..tag_end].ends_with("/>") {
            out.push(&xml[start..tag_end]);
            at = tag_end;
            continue;
        }
        let mut depth = 1usize;
        let mut pos = tag_end;
        let end = loop {
            let Some(c) = xml[pos..].find(&close).map(|c| pos + c) else { break xml.len() };
            match find_open(xml, pos, &open) {
                Some(o) if o < c => {
                    let o_end = xml[o..].find('>').map_or(xml.len(), |e| o + e + 1);
                    if !xml[o..o_end].ends_with("/>") {
                        depth += 1;
                    }
                    pos = o_end;
                }
                _ => {
                    depth -= 1;
                    pos = c + close.len();
                    if depth == 0 {
                        break pos;
                    }
                }
            }
        };
        out.push(&xml[start..end]);
        at = end;
    }
    out
}

/// Un'immagine del documento dal suo riferimento (r:embed o percorso), con la dimensione in punti e l'allineamento.
type Media<'a> = dyn Fn(&str, Option<(f32, f32)>, u8) -> Option<crate::pdfdoc::DocImage> + 'a;

fn docx_blocks(document: &str, styles: &str, media: &Media) -> Vec<crate::pdfdoc::DocBlock> {
    // styleId → livello, dai nomi degli stili e dal loro livello di struttura.
    let mut levels: Vec<(String, u8)> = Vec::new();
    for style in elements(styles, "w:style") {
        let Some(id) = attr_text(style, "w:styleId") else { continue };
        let name = style.find("<w:name ").and_then(|i| attr_text(&style[i..], "w:val"));
        let outline = style.find("<w:outlineLvl ").and_then(|i| attr_text(&style[i..], "w:val")).and_then(|v| v.parse::<u8>().ok());
        let level = name.as_deref().and_then(heading_level_of).or(outline.filter(|n| *n < 9).map(|n| (n + 1).min(3)));
        if let Some(level) = level {
            levels.push((id, level));
        }
    }
    // Di mc:AlternateContent si tiene solo la scelta principale: il ripiego (VML) ripete immagini e caselle di testo.
    let document = &without_fallbacks(document);
    elements(document, "w:p")
        .into_iter()
        .flat_map(|p| {
            let style = p.find("<w:pStyle ").and_then(|i| attr_text(&p[i..], "w:val")).unwrap_or_default();
            let outline = p.find("<w:outlineLvl ").and_then(|i| attr_text(&p[i..], "w:val")).and_then(|v| v.parse::<u8>().ok());
            let heading = levels
                .iter()
                .find(|(id, _)| *id == style)
                .map(|(_, l)| *l)
                .or_else(|| heading_level_of(&style))
                .or(outline.filter(|n| *n < 9).map(|n| (n + 1).min(3)))
                .unwrap_or(0);
            // Gli a capo (w:br, paragrafi delle caselle di testo) separano le parole: nel PDF diventano spazi.
            let text = ooxml_text(p).trim_end_matches('\n').replace('\t', "    ").replace('\n', " ");
            let align = match p.find("<w:jc ").and_then(|i| attr_text(&p[i..], "w:val")).as_deref() {
                Some("center") => 1,
                Some("right" | "end") => 2,
                _ => 0,
            };
            let mut images = Vec::new();
            for d in elements(p, "w:drawing") {
                // wp:extent in EMU (12700 per punto).
                let size = d.find("<wp:extent ").and_then(|i| {
                    let t = &d[i..];
                    Some((attr_text(t, "cx")?.parse::<f32>().ok()? / 12700.0, attr_text(t, "cy")?.parse::<f32>().ok()? / 12700.0))
                });
                if let Some(id) = d.find("<a:blip ").and_then(|i| attr_text(&d[i..], "r:embed")) {
                    images.extend(media(&id, size, align));
                }
            }
            for v in elements(p, "w:pict") {
                if let Some(id) = v.find("<v:imagedata ").and_then(|i| attr_text(&v[i..], "r:id")) {
                    images.extend(media(&id, None, align));
                }
            }
            let mut out = Vec::new();
            if images.is_empty() || !text.trim().is_empty() {
                out.push(crate::pdfdoc::DocBlock { text, heading, image: None });
            }
            out.extend(images.into_iter().map(|i| crate::pdfdoc::DocBlock { text: String::new(), heading: 0, image: Some(i) }));
            out
        })
        .collect()
}

/// Il testo senza i blocchi `<mc:Fallback>…</mc:Fallback>`.
fn without_fallbacks(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<mc:Fallback") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</mc:Fallback>") {
            Some(end) => rest = &rest[start + end + "</mc:Fallback>".len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Gli elementi `<tag …/>` o `<tag …>` (solo l'apertura), per i file di relazioni.
fn elements_self<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag} ");
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = xml[at..].find(&open) {
        let start = at + found;
        let end = xml[start..].find('>').map_or(xml.len(), |e| start + e + 1);
        out.push(&xml[start..end]);
        at = end;
    }
    out
}

/// Una lunghezza ODF («17cm», «2.5in», «120mm», «300pt», «400px») in punti.
fn odf_length(v: &str) -> Option<f32> {
    let v = v.trim();
    let num: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    let n: f32 = num.parse().ok()?;
    let k = match &v[num.len()..] {
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "in" => 72.0,
        "pt" => 1.0,
        "pc" => 12.0,
        "px" => 0.75,
        _ => return None,
    };
    Some(n * k)
}

fn odt_blocks(xml: &str, media: &Media) -> Vec<crate::pdfdoc::DocBlock> {
    let mut found: Vec<(usize, crate::pdfdoc::DocBlock)> = Vec::new();
    for (tag, is_heading) in [("text:h", true), ("text:p", false)] {
        let mut at = 0;
        for element in elements(xml, tag) {
            let position = xml[at..].find(element).map_or(at, |i| at + i);
            at = position;
            let heading = if is_heading {
                attr_text(element, "text:outline-level").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, 3)
            } else {
                0
            };
            let text = odt_plain(element).trim_end_matches('\n').replace('\t', "    ");
            let mut images = Vec::new();
            for frame in elements(element, "draw:frame") {
                let size = (|| Some((odf_length(&attr_text(frame, "svg:width")?)?, odf_length(&attr_text(frame, "svg:height")?)?)))();
                if let Some(href) = frame.find("<draw:image ").and_then(|i| attr_text(&frame[i..], "xlink:href")) {
                    images.extend(media(&href, size, 0));
                }
            }
            if images.is_empty() || !text.trim().is_empty() {
                found.push((position, crate::pdfdoc::DocBlock { text, heading, image: None }));
            }
            for i in images {
                found.push((position, crate::pdfdoc::DocBlock { text: String::new(), heading: 0, image: Some(i) }));
            }
        }
    }
    found.sort_by_key(|(position, _)| *position);
    found.into_iter().map(|(_, b)| b).collect()
}

fn rtf_blocks(bytes: &[u8]) -> Vec<crate::pdfdoc::DocBlock> {
    let raw: String = bytes.iter().map(|b| *b as char).collect();
    // Gli stili «heading N» della tavolozza: {\\sN … nome;}
    let mut styles: Vec<(String, u8)> = Vec::new();
    if let Some(at) = raw.find("{\\stylesheet") {
        let sheet = &raw[at..];
        let mut from = 0;
        while let Some(found) = sheet[from..].find("{\\s") {
            let start = from + found + 3;
            from = start;
            let id: String = sheet[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if id.is_empty() {
                continue;
            }
            let end = sheet[start..].find('}').map_or(sheet.len(), |e| start + e);
            let mut name = &sheet[start + id.len()..end];
            // Via le parole di controllo davanti al nome.
            loop {
                name = name.trim_start();
                if let Some(rest) = name.strip_prefix('\\') {
                    name = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '-');
                } else {
                    break;
                }
            }
            if let Some(level) = heading_level_of(name.trim_end_matches(';')) {
                styles.push((id, level));
            }
        }
    }
    // I paragrafi che sono titoli: per stile o per livello di struttura.
    let mut headings: Vec<(String, u8)> = Vec::new();
    let mut paragraphs: Vec<&str> = Vec::new();
    let mut last = 0;
    let visible: String = rtf_without_hidden_groups(bytes).iter().map(|b| *b as char).collect();
    let raw = visible;
    let marks: Vec<usize> = raw
        .match_indices("\\par")
        .filter(|(i, _)| !raw[i + 4..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map(|(i, _)| i)
        .collect();
    for m in marks {
        paragraphs.push(&raw[last..m]);
        last = m + 4;
    }
    paragraphs.push(&raw[last..]);
    for segment in paragraphs {
        let outline = segment
            .split("\\outlinelevel")
            .nth(1)
            .and_then(|r| r.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u8>().ok())
            .map(|n| (n + 1).min(3));
        let by_style = styles.iter().find(|(id, _)| {
            segment.match_indices("\\s").any(|(i, _)| {
                let digits: String = segment[i + 2..].chars().take_while(|c| c.is_ascii_digit()).collect();
                digits == *id
            })
        });
        if let Some(level) = outline.or(by_style.map(|(_, l)| *l)) {
            let plain: Vec<u8> = segment.chars().filter(|c| *c != '{' && *c != '}').map(|c| c as u32 as u8).collect();
            let text = rtf_text(&plain).trim().to_string();
            if !text.is_empty() {
                headings.push((text, level));
            }
        }
    }
    rtf_text(bytes)
        .split('\n')
        .map(|line| {
            let heading = headings.iter().find(|(t, _)| line.trim() == t).map_or(0, |(_, l)| *l);
            crate::pdfdoc::DocBlock { text: line.trim_end().to_string(), heading, image: None }
        })
        .collect()
}

pub fn pdf_text(path: &Path) -> Result<String, String> {
    let pages = pdf_page_texts(path)?;
    let text = pages.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
    if text.is_empty() {
        Err("questo PDF non ha testo da copiare".into())
    } else {
        Ok(text)
    }
}

/// Il testo incorporato di ogni pagina, in ordine. Vuoto se la pagina non ne ha.
pub fn pdf_page_texts(path: &Path) -> Result<Vec<String>, String> {
    let doc = lopdf::Document::load(path).map_err(|e| format!("PDF non letto: {e}"))?;
    let mut pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    pages.sort_unstable();
    Ok(pages
        .into_iter()
        // Si tengono i pezzi leggibili: un solo font non decodificabile non deve far perdere la pagina.
        .map(|page| doc.extract_text_chunks(&[page]).into_iter().filter_map(Result::ok).collect::<String>().trim().to_string())
        .collect())
}

pub fn docx_bytes(text: &str) -> Vec<u8> {
    let mut body = String::new();
    for para in text.replace("\r\n", "\n").split('\n') {
        body.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
        body.push_str(&xml_escape(para));
        body.push_str("</w:t></w:r></w:p>");
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let word_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
</Relationships>";
    // Calibri 11, lingua italiana per il correttore, un po' di spazio dopo ogni paragrafo.
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:lang w:val=\"it-IT\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
</w:styles>";
    zip_archive(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", word_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ])
}

fn docx_text(path: &Path) -> Result<String, String> {
    let xml = zip_entry(path, "word/document.xml")?;
    // Le caselle di testo sono scritte due volte (wps e ripiego VML): se ne tiene una.
    Ok(ooxml_text(&without_fallbacks(&xml)))
}

fn odt_text(path: &Path) -> Result<String, String> {
    let xml = zip_entry(path, "content.xml")?;
    Ok(odt_plain(&xml))
}

#[cfg(test)]
pub fn zip_entry_for_test(path: &Path, name: &str) -> String {
    zip_entry(path, name).unwrap()
}

fn zip_entry(path: &Path, name: &str) -> Result<String, String> {
    zip_entry_at_most(path, name, MAX_XML)
}

fn zip_entry_at_most(path: &Path, name: &str, max: u64) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("archivio non letto: {e}"))?;
    let entry = archive.by_name(name).map_err(|_| format!("manca {name}"))?;
    // Un documento vero ha pochi MB di XML: un archivio fatto apposta (una «zip bomb») che si
    // gonfia a GB non deve riempire la memoria.
    let mut xml = String::new();
    entry.take(max + 1).read_to_string(&mut xml).map_err(|e| e.to_string())?;
    if xml.len() as u64 > max {
        return Err(format!("{name} è troppo grande per un documento (oltre {} MB)", max / 1_000_000));
    }
    Ok(xml)
}

/// Un file binario dell'archivio (le immagini), al massimo 64 MB.
fn zip_bytes(path: &Path, name: &str) -> Result<Vec<u8>, String> {
    const MAX: u64 = 64 * 1_000_000;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("archivio non letto: {e}"))?;
    let entry = archive.by_name(name).map_err(|_| format!("manca {name}"))?;
    let mut out = Vec::new();
    entry.take(MAX + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    if out.len() as u64 > MAX {
        return Err(format!("{name} è troppo grande"));
    }
    Ok(out)
}

/// Il massimo di XML che si legge da una parte del documento.
const MAX_XML: u64 = 512 * 1_000_000;

fn ooxml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    // Dentro <w:tabs> i <w:tab> sono le posizioni delle tabulazioni, non tabulazioni nel testo.
    let mut in_tabs = false;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else { break };
        let raw = &rest[..end];
        rest = &rest[end + 1..];
        let tag = raw.split_whitespace().next().unwrap_or(raw).trim_end_matches('/');
        match tag {
            "w:t" => {
                let Some(close) = rest.find('<') else { break };
                out.push_str(&xml_decode(&rest[..close]));
                rest = &rest[close..];
            }
            "w:tabs" => in_tabs = !raw.ends_with('/'),
            "/w:tabs" => in_tabs = false,
            "w:tab" if !in_tabs => out.push('\t'),
            "w:br" | "w:cr" | "/w:p" => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn odt_plain(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else { break };
        let raw = &rest[..end];
        rest = &rest[end + 1..];
        let tag = raw.split_whitespace().next().unwrap_or(raw).trim_end_matches('/');
        if tag == "text:tab" {
            out.push('\t');
        } else if tag == "text:line-break" {
            out.push('\n');
        } else if tag == "text:s" {
            // Un documento vero ne ha qualche decina: un file ostile non deve riempire la memoria.
            let n = attr_u32(raw, "text:c").unwrap_or(1).min(1024) as usize;
            out.extend(std::iter::repeat_n(' ', n));
        } else if tag == "/text:p" || tag == "/text:h" {
            out.push('\n');
        }
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        // Il testo fino al tag dopo: gli spazi contano, l'indentazione dell'XML no.
        let chunk = &rest[..rest.find('<').unwrap_or(rest.len())];
        if !chunk.trim().is_empty() || (!chunk.is_empty() && !chunk.contains('\n')) {
            out.push_str(&xml_decode(&chunk.replace(['\n', '\r'], " ")));
        }
    }
    out
}

fn attr_u32(tag: &str, name: &str) -> Option<u32> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

/// L'RTF senza i gruppi che non sono testo del documento (tavolozze dei font, dei colori e degli stili, info, immagini…).
fn rtf_without_hidden_groups(bytes: &[u8]) -> Vec<u8> {
    const HIDDEN: [&[u8]; 8] = [b"{\\fonttbl", b"{\\colortbl", b"{\\stylesheet", b"{\\info", b"{\\pict", b"{\\*", b"{\\header", b"{\\footer"];
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && HIDDEN.iter().any(|h| bytes[i..].starts_with(h)) {
            let mut depth = 0i32;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 1,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn rtf_text(bytes: &[u8]) -> String {
    let bytes = &rtf_without_hidden_groups(bytes)[..];
    let mut out = String::new();
    let mut i = 0;
    // Quanti caratteri di riserva seguono \uN (\ucN, di norma 1).
    let mut fallback = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'}' | b'\r' | b'\n' => i += 1,
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'\\' | b'{' | b'}' => {
                        out.push(bytes[i] as char);
                        i += 1;
                    }
                    b'\'' => {
                        if i + 2 < bytes.len() {
                            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                            if let Ok(b) = u8::from_str_radix(hex, 16) {
                                out.push(cp1252(b));
                            }
                            i += 3;
                        } else {
                            break;
                        }
                    }
                    b'u' if i + 1 < bytes.len() && (bytes[i + 1].is_ascii_digit() || bytes[i + 1] == b'-') => {
                        i += 1;
                        let neg = bytes[i] == b'-';
                        if neg {
                            i += 1;
                        }
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                        if let Ok(mut n) = std::str::from_utf8(&bytes[start..i]).unwrap_or("0").parse::<i32>() {
                            if neg {
                                n = -n;
                            }
                            if n < 0 {
                                n += 65536;
                            }
                            if let Some(ch) = char::from_u32(n as u32) {
                                out.push(ch);
                            }
                        }
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        // Dopo \uN vengono i caratteri di riserva per chi non legge l'Unicode (di solito uno: `?` o `\'80`).
                        for _ in 0..fallback {
                            match bytes.get(i) {
                                Some(b'\\') if bytes.get(i + 1) == Some(&b'\'') => i += 4,
                                Some(b'\\' | b'{' | b'}') | None => break,
                                Some(_) => i += 1,
                            }
                        }
                    }
                    _ => {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                        let param = i;
                        while i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                            i += 1;
                        }
                        let param = std::str::from_utf8(&bytes[param..i]).ok().and_then(|p| p.parse::<usize>().ok());
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        match word {
                            "par" | "line" | "page" => out.push('\n'),
                            "tab" => out.push('\t'),
                            "uc" => fallback = param.unwrap_or(1),
                            _ => {}
                        }
                    }
                }
            }
            b if b >= 32 => {
                out.push(b as char);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn doc_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut compound = cfb::CompoundFile::open(file).map_err(|e| format!("DOC non letto: {e}"))?;
    let mut word = Vec::new();
    compound
        .open_stream("/WordDocument")
        .map_err(|e| format!("DOC senza testo: {e}"))?
        .read_to_end(&mut word)
        .map_err(|e| e.to_string())?;
    let (fc, lcb, table_one) = clx_place(&word).ok_or("DOC senza indice del testo")?;
    let table_name = if table_one { "/1Table" } else { "/0Table" };
    let mut table = Vec::new();
    if compound.open_stream(table_name).and_then(|mut s| s.read_to_end(&mut table)).is_err() {
        let other = if table_one { "/0Table" } else { "/1Table" };
        table.clear();
        compound
            .open_stream(other)
            .and_then(|mut s| s.read_to_end(&mut table))
            .map_err(|_| "DOC senza tabella del testo".to_string())?;
    }
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if start >= end {
        return Err("DOC senza testo".into());
    }
    Ok(clx_text(&word, &table[start..end]))
}

fn clx_place(word: &[u8]) -> Option<(u32, u32, bool)> {
    if word.len() < 12 || u16::from_le_bytes([word[0], word[1]]) != 0xA5EC {
        return None;
    }
    let table_one = u16::from_le_bytes([word[0x0A], word[0x0B]]) & 0x0200 != 0;
    let mut at = 32usize;
    let csw = u16::from_le_bytes(word.get(at..at + 2)?.try_into().ok()?) as usize;
    at += 2 + csw * 2;
    let cslw = u16::from_le_bytes(word.get(at..at + 2)?.try_into().ok()?) as usize;
    at += 2 + cslw * 4;
    let pairs = u16::from_le_bytes(word.get(at..at + 2)?.try_into().ok()?) as usize;
    at += 2;
    if pairs < 34 || word.len() < at + 272 {
        return None;
    }
    let fc = u32::from_le_bytes(word[at + 264..at + 268].try_into().ok()?);
    let lcb = u32::from_le_bytes(word[at + 268..at + 272].try_into().ok()?);
    if lcb == 0 {
        return None;
    }
    Some((fc, lcb, table_one))
}

fn clx_text(word: &[u8], clx: &[u8]) -> String {
    let mut i = 0;
    while i < clx.len() && clx[i] == 0x01 {
        if i + 3 > clx.len() {
            return String::new();
        }
        let cb = u16::from_le_bytes([clx[i + 1], clx[i + 2]]) as usize;
        i += 3 + cb;
    }
    if i >= clx.len() || clx[i] != 0x02 || i + 5 > clx.len() {
        return String::new();
    }
    let lcb = u32::from_le_bytes(clx[i + 1..i + 5].try_into().unwrap_or([0; 4])) as usize;
    i += 5;
    if lcb < 4 || i + lcb > clx.len() {
        return String::new();
    }
    let plc = &clx[i..i + lcb];
    let n = lcb.saturating_sub(4) / 12;
    if n == 0 || 4 * (n + 1) + 8 * n > plc.len() {
        return String::new();
    }
    let mut out = String::new();
    // Il testo di un DOC vero non supera lo stream: pezzi che lo ripetono tutto non devono riempire la memoria.
    let limit = word.len().saturating_mul(2).max(1 << 20);
    let mut prev_cp = 0u32;
    for piece in 0..n {
        let cp = u32::from_le_bytes(plc[piece * 4..piece * 4 + 4].try_into().unwrap_or([0; 4]));
        let cp_next = u32::from_le_bytes(plc[(piece + 1) * 4..(piece + 1) * 4 + 4].try_into().unwrap_or([0; 4]));
        // CP non crescenti: tabella rovinata.
        if cp < prev_cp || cp_next < cp || out.len() > limit {
            break;
        }
        prev_cp = cp_next;
        let pcd = 4 * (n + 1) + piece * 8;
        if pcd + 6 > plc.len() {
            break;
        }
        let fc_raw = u32::from_le_bytes(plc[pcd + 2..pcd + 6].try_into().unwrap_or([0; 4]));
        let compressed = fc_raw & (1 << 30) != 0;
        let fc = fc_raw & ((1 << 30) - 1);
        let count = cp_next.saturating_sub(cp) as usize;
        if compressed {
            let offset = (fc / 2) as usize;
            for k in 0..count {
                let Some(&b) = word.get(offset + k) else { break };
                match b {
                    0x0B | 0x0D | 0x07 | 0x0C => out.push('\n'),
                    0x09 => out.push('\t'),
                    0x00..=0x1F => {}
                    _ => out.push(cp1252(b)),
                }
            }
        } else {
            let offset = fc as usize;
            for k in 0..count {
                let at = offset + k * 2;
                let Some(pair) = word.get(at..at + 2) else { break };
                let u = u16::from_le_bytes([pair[0], pair[1]]);
                match u {
                    0x0B | 0x0D | 0x07 | 0x0C => out.push('\n'),
                    0x09 => out.push('\t'),
                    0x00..=0x1F => {}
                    _ => out.push(char::from_u32(u as u32).unwrap_or(' ')),
                }
            }
        }
    }
    out
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Word rifiuta il file se l'XML contiene caratteri di controllo.
            '\t' => out.push(' '),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn xml_decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            out.push_str(rest);
            return out;
        };
        let name = &rest[..end];
        rest = &rest[end + 1..];
        match name {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if name.starts_with("#x") => {
                if let Some(ch) = u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            _ if name.starts_with('#') => {
                if let Some(ch) = name[1..].parse::<u32>().ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            _ => {
                out.push('&');
                out.push_str(name);
                out.push(';');
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn cp1252(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        _ => b as char,
    }
}

#[cfg(test)]
mod block_tests {
    use super::*;
    use crate::engine::{perform_with, Options};
    use crate::pdfdoc::DocBlock;

    fn dir() -> std::path::PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("cedro-blocks-{n}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Una parte del documento che si gonfia oltre il limite: errore, non la memoria piena.
    #[test]
    fn a_swollen_document_part_is_refused() {
        let d = dir();
        let path = d.join("gonfio.docx");
        let xml = vec![b' '; 3_000_001];
        fs::write(&path, crate::zipstore::zip_archive(&[("word/document.xml", &xml)])).unwrap();
        assert!(zip_entry_at_most(&path, "word/document.xml", 3_000_000).unwrap_err().contains("troppo grande"));
        assert_eq!(zip_entry_at_most(&path, "word/document.xml", 3_000_001).unwrap().len(), 3_000_001);
        let _ = fs::remove_dir_all(d);
    }

    /// Un DOCX fatto solo di immagini (la carta d'identità fotografata) diventa comunque un PDF con le immagini.
    #[test]
    fn a_docx_of_only_images_becomes_a_pdf_with_images() {
        let d = dir();
        let path = d.join("carta.docx");
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(40, 30, image::Rgb([200, 30, 30])));
        let mut jpeg = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let document = "<w:document><w:body>\
<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:drawing><wp:inline><wp:extent cx=\"2540000\" cy=\"1905000\"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"rId1\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>\
<w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"2540000\" cy=\"1905000\"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"rId2\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>\
</w:body></w:document>";
        let rels = "<Relationships><Relationship Id=\"rId1\" Type=\"image\" Target=\"media/fronte.jpeg\"/><Relationship Id=\"rId2\" Type=\"image\" Target=\"media/retro.png\"/></Relationships>";
        fs::write(&path, crate::zipstore::zip_archive(&[("word/document.xml", document.as_bytes()), ("word/_rels/document.xml.rels", rels.as_bytes()), ("word/media/fronte.jpeg", &jpeg), ("word/media/retro.png", &png)])).unwrap();
        assert_eq!(document_text(&path).unwrap_err(), "il documento non contiene testo");
        let blocks = document_blocks(&path).unwrap();
        let images: Vec<_> = blocks.iter().filter_map(|b| b.image.as_ref()).collect();
        assert_eq!(images.len(), 2);
        assert!(images[0].jpeg && !images[1].jpeg);
        let pdf = crate::pdfdoc::build(&blocks, &crate::pdfdoc::PdfOptions { size: crate::engine::PageSize::A4, embed: false, bookmarks: false, pdfa: false, title: "carta".into() }).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("/Subtype /Image").count(), 2);
        assert!(text.contains("/DCTDecode"));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn a_docx_alternate_image_is_taken_once() {
        let document = "<w:document><w:body><w:p><w:r><mc:AlternateContent>\
<mc:Choice Requires=\"wps\"><w:drawing><wp:inline><wp:extent cx=\"127000\" cy=\"127000\"/><a:blip r:embed=\"rId1\"/></wp:inline></w:drawing></mc:Choice>\
<mc:Fallback><w:pict><v:shape><v:imagedata r:id=\"rId1\"/></v:shape></w:pict></mc:Fallback>\
</mc:AlternateContent></w:r></w:p></w:body></w:document>";
        let seen = std::cell::RefCell::new(Vec::new());
        let blocks = docx_blocks(document, "", &|id, size, _| {
            seen.borrow_mut().push((id.to_string(), size));
            None
        });
        assert_eq!(*seen.borrow(), vec![("rId1".to_string(), Some((10.0, 10.0)))]);
        assert_eq!(blocks.len(), 1);
    }

    /// Una casella di testo come la salva Word: wps nella scelta, VML nel ripiego.
    const TEXTBOX: &str = "<w:document><w:body><w:p><w:r><w:t>Prima</w:t></w:r><w:r><mc:AlternateContent>\
<mc:Choice Requires=\"wps\"><w:drawing><wps:txbx><w:txbxContent><w:p><w:r><w:t>Casella</w:t></w:r></w:p></w:txbxContent></wps:txbx></w:drawing></mc:Choice>\
<mc:Fallback><w:pict><v:textbox><w:txbxContent><w:p><w:r><w:t>Casella</w:t></w:r></w:p></w:txbxContent></v:textbox></w:pict></mc:Fallback>\
</mc:AlternateContent></w:r><w:r><w:t>dopo</w:t></w:r></w:p><w:p><w:r><w:t>Fine</w:t></w:r></w:p></w:body></w:document>";

    #[test]
    fn a_docx_text_box_is_read_once_and_the_paragraph_keeps_its_end() {
        let blocks = docx_blocks(TEXTBOX, "", &|_, _, _| None);
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, vec!["PrimaCasella dopo", "Fine"]);
        assert_eq!(ooxml_text(&without_fallbacks(TEXTBOX)).matches("Casella").count(), 1);
    }

    #[test]
    fn nested_elements_close_at_their_own_end() {
        let xml = "<a><a x=\"1\">uno</a><a/>due</a><ab>no</ab><a>tre</a>";
        assert_eq!(elements(xml, "a"), vec!["<a><a x=\"1\">uno</a><a/>due</a>", "<a>tre</a>"]);
        // Senza chiusura: fino in fondo, senza blocchi.
        assert_eq!(elements("<a><a>x</a>", "a"), vec!["<a><a>x</a>"]);
    }

    #[test]
    fn docx_tab_stops_are_not_tabs_in_the_text() {
        let p = "<w:p><w:pPr><w:tabs><w:tab w:val=\"left\" w:pos=\"2835\"/><w:tab w:val=\"right\" w:pos=\"9000\"/></w:tabs></w:pPr>\
<w:r><w:t>Nome</w:t></w:r><w:r><w:tab/><w:t>Data</w:t></w:r></w:p>";
        assert_eq!(ooxml_text(p), "Nome\tData\n");
        assert_eq!(ooxml_text("<w:p><w:pPr><w:tabs/></w:pPr><w:r><w:tab/><w:t>x</w:t></w:r></w:p>"), "\tx\n");
    }

    #[test]
    fn odt_spaces_have_a_limit() {
        let text = odt_plain("<text:p>a<text:s text:c=\"4000000000\"/>b<text:s text:c=\"3\"/>c<text:s/>d</text:p>");
        assert_eq!(text.len(), 1 + 1024 + 1 + 3 + 1 + 1 + 1 + 1);
        assert!(text.ends_with("b   c d\n"));
    }

    /// Una piece table con `n` pezzi, ciascuno di `len` caratteri compressi dall'inizio dello stream.
    fn clx(cps: &[u32]) -> Vec<u8> {
        let n = cps.len() - 1;
        let mut plc = Vec::new();
        for cp in cps {
            plc.extend_from_slice(&cp.to_le_bytes());
        }
        for _ in 0..n {
            plc.extend_from_slice(&[0, 0]);
            plc.extend_from_slice(&(1u32 << 30).to_le_bytes());
            plc.extend_from_slice(&[0, 0]);
        }
        let mut out = vec![0x02];
        out.extend_from_slice(&(plc.len() as u32).to_le_bytes());
        out.extend_from_slice(&plc);
        out
    }

    #[test]
    fn doc_pieces_cannot_repeat_the_stream_without_end() {
        let word = vec![b'a'; 1000];
        assert_eq!(clx_text(&word, &clx(&[0, 3, 5])), "aaaaa");
        // 5000 pezzi che ripetono tutto lo stream: 5 MB di testo da 1 KB.
        let cps: Vec<u32> = (0..=5000).map(|i| i * 1000).collect();
        let text = clx_text(&word, &clx(&cps));
        assert!(text.len() <= (1 << 20) + 1000, "{}", text.len());
        // CP che tornano indietro: tabella rovinata, ci si ferma.
        assert_eq!(clx_text(&word, &clx(&[0, 3, 1, 4])), "aaa");
    }

    fn levels(blocks: &[DocBlock]) -> Vec<(u8, &str)> {
        blocks.iter().filter(|b| b.heading > 0).map(|b| (b.heading, b.text.as_str())).collect()
    }

    #[test]
    fn docx_headings_come_from_style_names_ids_and_outline_levels() {
        let document = "<w:document><w:body>\
<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Delibera</w:t></w:r></w:p>\
<w:p><w:r><w:t>Premessa normale</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Titolo2\"/></w:pPr><w:r><w:t>Dispositivo</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Mio\"/></w:pPr><w:r><w:t>Stile mio</w:t></w:r></w:p>\
<w:p><w:pPr><w:outlineLvl w:val=\"2\"/></w:pPr><w:r><w:t>Livello tre</w:t></w:r></w:p>\
</w:body></w:document>";
        let styles = "<w:styles><w:style w:type=\"paragraph\" w:styleId=\"Mio\"><w:name w:val=\"heading 2\"/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Corpo\"><w:name w:val=\"Body\"/></w:style></w:styles>";
        let blocks = docx_blocks(document, styles, &|_, _, _| None);
        assert_eq!(blocks.len(), 5);
        assert_eq!(levels(&blocks), vec![(1, "Delibera"), (2, "Dispositivo"), (2, "Stile mio"), (3, "Livello tre")]);
        assert_eq!(blocks[1], DocBlock { text: "Premessa normale".into(), heading: 0, image: None });
    }

    #[test]
    fn odt_headings_come_from_outline_levels() {
        let xml = "<office:text><text:h text:outline-level=\"1\">Verbale</text:h><text:p>Corpo <text:span>del</text:span> testo</text:p>\
<text:h text:outline-level=\"2\">Presenti</text:h><text:p text:style-name=\"P1\">Fine</text:p></office:text>";
        let blocks = odt_blocks(xml, &|_, _, _| None);
        assert_eq!(levels(&blocks), vec![(1, "Verbale"), (2, "Presenti")]);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[1].text, "Corpo del testo");
    }

    #[test]
    fn rtf_headings_come_from_style_names_and_outline_levels() {
        let rtf = br"{\rtf1\ansi{\stylesheet{\s1 heading 1;}{\s2 heading 2;}{\s3 Normal;}}\pard\s1 Capitolo uno\par\pard\s3 Testo normale\par\pard\s2 Paragrafo\par\pard\outlinelevel0 Altro titolo\par}";
        let blocks = rtf_blocks(rtf);
        assert_eq!(levels(&blocks), vec![(1, "Capitolo uno"), (2, "Paragrafo"), (1, "Altro titolo")], "{blocks:?}");
    }

    #[test]
    fn rtf_unicode_skips_the_fallback_characters() {
        // Come lo scrivono Word e LibreOffice: \uN seguito dal carattere di riserva in cp1252.
        assert_eq!(rtf_text(br"{\rtf1\ansi Prezzo 10\u8364\'80, citt\u224\'e0\par}").trim(), "Prezzo 10€, città");
        assert_eq!(rtf_text(br"{\rtf1\ansi \u8364 ? e \u8364?.}").trim(), "€ e €.");
        assert_eq!(rtf_text(br"{\rtf1\ansi\uc2 \u8364\'80\'80 fine\uc0 \u224 x}").trim(), "€ fineàx");
    }

    #[test]
    fn a_document_with_titles_gets_pdf_bookmarks() {
        let d = dir();
        let docx = d.join("delibera.docx");
        let document = "<w:document><w:body>\
<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Oggetto</w:t></w:r></w:p>\
<w:p><w:r><w:t>Testo della premessa.</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Dispositivo</w:t></w:r></w:p>\
<w:p><w:r><w:t>Si approva.</w:t></w:r></w:p></w:body></w:document>";
        let archive = crate::zipstore::zip_archive(&[("word/document.xml", document.as_bytes())]);
        fs::write(&docx, archive).unwrap();
        let with = perform_with(&docx, "pdf", 80, None, &Options::default()).unwrap();
        assert!(with.message.contains("segnalibri") && with.message.contains("incorporati"), "{}", with.message);
        let pdf = lopdf::Document::load(&with.path).unwrap();
        let root = pdf.get_dictionary(pdf.catalog().unwrap().get(b"Outlines").unwrap().as_reference().unwrap()).unwrap();
        assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), 2);
        let without = perform_with(&docx, "pdf", 80, None, &Options { bookmarks: false, exists: crate::engine::Exists::Replace, ..Default::default() }).unwrap();
        let pdf = lopdf::Document::load(&without.path).unwrap();
        assert!(pdf.catalog().unwrap().get(b"Outlines").is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
