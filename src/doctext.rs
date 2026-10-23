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
        .map(|page| doc.extract_text(&[page]).map(|t| t.trim().to_string()).unwrap_or_default())
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
    Ok(ooxml_text(&xml))
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
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("archivio non letto: {e}"))?;
    let mut entry = archive.by_name(name).map_err(|_| format!("manca {name}"))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).map_err(|e| e.to_string())?;
    Ok(xml)
}

fn ooxml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else { break };
        let raw = &rest[..end];
        rest = &rest[end + 1..];
        let tag = raw.split_whitespace().next().unwrap_or(raw).trim_end_matches('/');
        if tag == "w:t" {
            let Some(close) = rest.find('<') else { break };
            out.push_str(&xml_decode(&rest[..close]));
            rest = &rest[close..];
        } else if tag == "w:tab" {
            out.push('\t');
        } else if tag == "w:br" || tag == "w:cr" || tag == "/w:p" {
            out.push('\n');
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
        let tag = raw.split_whitespace().next().unwrap_or(raw);
        if tag == "text:tab" || tag.starts_with("text:tab ") {
            out.push('\t');
        } else if tag == "text:line-break" || tag.starts_with("text:line-break ") {
            out.push('\n');
        } else if tag == "text:s" || tag.starts_with("text:s ") {
            let count = attr_u32(raw, "text:c").unwrap_or(1);
            for _ in 0..count {
                out.push(' ');
            }
        } else if tag == "/text:p" || tag == "/text:h" {
            out.push('\n');
        } else if !tag.starts_with('/') && !tag.starts_with('?') && !tag.starts_with('!') {
            if let Some(close) = rest.find('<') {
                let chunk = rest[..close].trim();
                if !chunk.is_empty() {
                    out.push_str(&xml_decode(chunk));
                }
            }
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

fn rtf_text(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
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
                        } else if i < bytes.len() && bytes[i] != b'\\' && bytes[i] != b'{' && bytes[i] != b'}' {
                            i += 1;
                        }
                    }
                    _ => {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                        while i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                            i += 1;
                        }
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        match word {
                            "par" | "line" | "page" => out.push('\n'),
                            "tab" => out.push('\t'),
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
    for piece in 0..n {
        let cp = u32::from_le_bytes(plc[piece * 4..piece * 4 + 4].try_into().unwrap_or([0; 4]));
        let cp_next = u32::from_le_bytes(plc[(piece + 1) * 4..(piece + 1) * 4 + 4].try_into().unwrap_or([0; 4]));
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
