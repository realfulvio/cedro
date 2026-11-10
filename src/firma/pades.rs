//! Le firme dentro i PDF (PAdES e le vecchie adbe.pkcs7): ogni firma copre i byte del file fino a
//! un certo punto (/ByteRange), tranne il buco dove sta la firma stessa (/Contents). Quello che
//! viene dopo sono aggiunte successive: altre firme, marche, annotazioni o modifiche vere.

use super::asn1::R;

#[derive(Clone, Debug)]
pub struct PdfSig {
    /// [inizio1, lunghezza1, inizio2, lunghezza2]
    pub range: [usize; 4],
    /// La busta CMS (o la marca, per le DocTimeStamp).
    pub contents: Vec<u8>,
    pub sub_filter: String,
    pub doc_timestamp: bool,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub name: Option<String>,
    /// La data scritta nel PDF (/M): dichiarata, non certa.
    pub m: Option<i64>,
    /// Il nome del campo e la pagina (da 1), se si trovano.
    pub field: Option<String>,
    pub page: Option<u32>,
    pub visible: bool,
    /// Il riquadro della firma visibile, in frazioni della pagina (x0, y0, x1, y1; y dall'alto).
    pub rect: Option<[f32; 4]>,
}

impl PdfSig {
    /// Fin dove arriva questa firma: la versione firmata del file sono i byte fino a qui.
    pub fn end(&self) -> usize {
        self.range[2] + self.range[3]
    }

    /// I due pezzi firmati.
    pub fn parts<'a>(&self, data: &'a [u8]) -> R<[&'a [u8]; 2]> {
        let [a, b, c, d] = self.range;
        let (Some(ab), Some(cd)) = (a.checked_add(b), c.checked_add(d)) else {
            return Err("intervallo della firma fuori dal file".into());
        };
        if ab > data.len() || cd > data.len() || ab > c {
            return Err("intervallo della firma fuori dal file".into());
        }
        Ok([&data[a..ab], &data[c..cd]])
    }
}

pub fn is_pdf(data: &[u8]) -> bool {
    data.len() > 5 && data[..data.len().min(1024)].windows(5).any(|w| w == b"%PDF-")
}

fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    if from >= h.len() {
        return None;
    }
    h[from..].windows(n.len()).position(|w| w == n).map(|p| p + from)
}

fn rfind(h: &[u8], n: &[u8], before: usize) -> Option<usize> {
    let end = before.min(h.len());
    h[..end].windows(n.len()).rposition(|w| w == n)
}

fn numbers(s: &[u8]) -> Vec<usize> {
    String::from_utf8_lossy(s).split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect()
}

fn unhex(s: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = s.iter().filter_map(|&c| (c as char).to_digit(16).map(|d| d as u8)).collect();
    let mut out: Vec<u8> = digits.chunks(2).map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0)).collect();
    // Il buco è riempito di zeri dopo la busta: si tiene solo la busta.
    if let Ok((n, _)) = super::asn1::read(&out) {
        let l = n.raw.len();
        out.truncate(l);
    }
    out
}

/// Una stringa PDF semplice: (testo) o <esadecimale>, anche UTF-16.
fn pdf_string(dict: &[u8], key: &[u8]) -> Option<String> {
    let mut i = 0;
    let p = loop {
        let p = find(dict, key, i)?;
        // «/Name» non deve prendere «/NameX».
        if dict.get(p + key.len()).is_some_and(|c| c.is_ascii_alphanumeric()) {
            i = p + 1;
            continue;
        }
        break p;
    };
    let rest = &dict[p + key.len()..];
    let start = rest.iter().position(|c| !c.is_ascii_whitespace())?;
    let rest = &rest[start..];
    let bytes = match rest.first()? {
        b'(' => {
            let mut out = Vec::new();
            let mut depth = 0;
            let mut j = 1;
            while j < rest.len() {
                let c = rest[j];
                match c {
                    b'\\' => {
                        j += 1;
                        match rest.get(j) {
                            Some(b'n') => out.push(b'\n'),
                            Some(b'r') => out.push(b'\r'),
                            Some(b't') => out.push(b'\t'),
                            Some(d) if (b'0'..=b'7').contains(d) => {
                                let mut v = 0u32;
                                let mut k = 0;
                                while k < 3 && rest.get(j + k).is_some_and(|x| (b'0'..=b'7').contains(x)) {
                                    v = v * 8 + (rest[j + k] - b'0') as u32;
                                    k += 1;
                                }
                                out.push(v as u8);
                                j += k - 1;
                            }
                            Some(&x) => out.push(x),
                            None => {}
                        }
                    }
                    b'(' => {
                        depth += 1;
                        out.push(c);
                    }
                    b')' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                        out.push(c);
                    }
                    _ => out.push(c),
                }
                j += 1;
            }
            out
        }
        b'<' if rest.get(1) != Some(&b'<') => {
            let end = rest.iter().position(|&c| c == b'>')?;
            let digits: Vec<u8> = rest[1..end].iter().filter_map(|&c| (c as char).to_digit(16).map(|d| d as u8)).collect();
            digits.chunks(2).map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0)).collect()
        }
        b'/' => {
            let end = rest[1..].iter().position(|c| c.is_ascii_whitespace() || b"/<>[]()".contains(c)).map(|e| e + 1).unwrap_or(rest.len());
            rest[1..end].to_vec()
        }
        _ => return None,
    };
    let s = if bytes.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = bytes[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        String::from_utf16_lossy(&u)
    } else {
        bytes.iter().map(|&b| crate::doctext::cp1252(b)).collect()
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Le date dei PDF: D:20261105104231+01'00'
pub fn pdf_date(s: &str) -> Option<i64> {
    let s = s.trim().trim_start_matches("D:");
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return None;
    }
    let num = |a: usize, b: usize, def: i64| digits.get(a..b).and_then(|x| x.parse().ok()).unwrap_or(def);
    let days = super::asn1::days_from_civil(num(0, 4, 1970), num(4, 6, 1), num(6, 8, 1));
    let mut t = days * 86400 + num(8, 10, 0) * 3600 + num(10, 12, 0) * 60 + num(12, 14, 0);
    let tz = &s[digits.len()..];
    if let Some(sign) = tz.chars().next().filter(|c| *c == '+' || *c == '-') {
        let z: Vec<i64> = tz[1..].split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
        let off = z.first().copied().unwrap_or(0) * 3600 + z.get(1).copied().unwrap_or(0) * 60;
        t -= if sign == '+' { off } else { -off };
    }
    Some(t)
}

/// Tutte le firme del PDF, dalla più vecchia alla più recente.
pub fn signatures(data: &[u8]) -> Vec<PdfSig> {
    let mut out: Vec<PdfSig> = Vec::new();
    let mut i = 0;
    while let Some(p) = find(data, b"/ByteRange", i) {
        i = p + 10;
        let Some(open) = find(data, b"[", p) else { break };
        let Some(close) = find(data, b"]", open) else { break };
        if open - p > 20 {
            continue;
        }
        let n = numbers(&data[open..close]);
        if n.len() != 4 {
            continue;
        }
        let range = [n[0], n[1], n[2], n[3]];
        // Numeri assurdi (file rovinato o fatto apposta): niente somme che traboccano.
        let (Some(hole_start), Some(end)) = (range[0].checked_add(range[1]), range[2].checked_add(range[3])) else { continue };
        if out.iter().any(|s| s.range == range) || hole_start > range[2] || end > data.len() {
            continue;
        }
        let hole = &data[hole_start..range[2]];
        let contents = unhex(hole);
        if contents.is_empty() {
            continue;
        }
        // Il dizionario: dall'«obj» prima fino all'«endobj» dopo.
        let ds = rfind(data, b"obj", hole_start).unwrap_or(p.saturating_sub(2000)).min(hole_start);
        let de = find(data, b"endobj", range[2]).unwrap_or(data.len().min(range[2] + 4000));
        let mut dict = data[ds..hole_start].to_vec();
        dict.extend_from_slice(&data[range[2]..de]);
        let sub_filter = pdf_string(&dict, b"/SubFilter").unwrap_or_default();
        let doc_timestamp = sub_filter == "ETSI.RFC3161" || find(&dict, b"/DocTimeStamp", 0).is_some();
        out.push(PdfSig {
            range,
            contents,
            sub_filter,
            doc_timestamp,
            reason: pdf_string(&dict, b"/Reason"),
            location: pdf_string(&dict, b"/Location"),
            name: pdf_string(&dict, b"/Name"),
            m: pdf_string(&dict, b"/M").and_then(|d| pdf_date(&d)),
            field: None,
            page: None,
            visible: false,
            rect: None,
        });
    }
    out.sort_by_key(|s| s.end());
    if !out.is_empty() {
        fields(data, &mut out);
    }
    out
}

/// Il /MediaBox di una pagina, anche ereditato dai nodi sopra.
fn media_box(doc: &lopdf::Document, mut id: lopdf::ObjectId) -> Option<[f32; 4]> {
    for _ in 0..16 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(a) = d.get(b"MediaBox").and_then(|o| doc.dereference(o).map(|x| x.1)).and_then(|o| o.as_array()) {
            let v: Vec<f32> = a.iter().filter_map(|o| o.as_float().ok()).collect();
            return (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]]);
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Nome del campo, pagina e se la firma si vede: dai campi del modulo (con lopdf).
fn fields(data: &[u8], sigs: &mut [PdfSig]) {
    use lopdf::Object;
    let Ok(doc) = lopdf::Document::load_mem(data) else { return };
    let pages: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let page_of = |id: lopdf::ObjectId| -> Option<u32> {
        for (n, pid) in &pages {
            if let Ok(pd) = doc.get_dictionary(*pid) {
                if let Ok(Object::Array(a)) = pd.get(b"Annots").and_then(|o| doc.dereference(o).map(|x| x.1)) {
                    if a.iter().any(|o| o.as_reference().ok() == Some(id)) {
                        return Some(*n);
                    }
                }
            }
        }
        None
    };
    for (id, obj) in doc.objects.iter() {
        let Ok(d) = obj.as_dict() else { continue };
        let is_sig_field = d.get(b"FT").ok().and_then(|o| o.as_name().ok()) == Some(b"Sig".as_slice());
        if !is_sig_field {
            continue;
        }
        let Ok(v) = d.get(b"V") else { continue };
        let Ok((_, Object::Dictionary(sd))) = doc.dereference(v) else { continue };
        let Ok(br) = sd.get(b"ByteRange").and_then(|o| o.as_array()) else { continue };
        let r: Vec<usize> = br.iter().filter_map(|o| o.as_i64().ok()).map(|x| x as usize).collect();
        if r.len() != 4 {
            continue;
        }
        let Some(s) = sigs.iter_mut().find(|s| s.range[..] == r[..]) else { continue };
        s.field = d.get(b"T").ok().and_then(|o| o.as_str().ok()).map(|b| {
            if b.starts_with(&[0xFE, 0xFF]) {
                String::from_utf16_lossy(&b[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect::<Vec<_>>())
            } else {
                String::from_utf8_lossy(b).into_owned()
            }
        });
        // Il riquadro: nel campo stesso (widget unito) o nei suoi figli.
        let mut widgets = vec![*id];
        if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
            widgets.extend(kids.iter().filter_map(|k| k.as_reference().ok()));
        }
        for w in widgets {
            let Ok(wd) = doc.get_dictionary(w) else { continue };
            let page = wd.get(b"P").ok().and_then(|p| p.as_reference().ok()).and_then(|p| pages.iter().find(|x| x.1 == p).map(|x| x.0)).or_else(|| page_of(w));
            if let Ok(rect) = wd.get(b"Rect").and_then(|o| o.as_array()) {
                let v: Vec<f32> = rect.iter().filter_map(|o| o.as_float().ok()).collect();
                if v.len() == 4 && (v[2] - v[0]).abs() > 1.0 && (v[3] - v[1]).abs() > 1.0 {
                    s.visible = true;
                    // In frazioni della pagina, per portarci lo sguardo nel visualizzatore.
                    let pid = page.and_then(|n| pages.iter().find(|x| x.0 == n).map(|x| x.1));
                    let mb = pid.and_then(|p| media_box(&doc, p)).unwrap_or([0.0, 0.0, 595.0, 842.0]);
                    let (pw, ph) = ((mb[2] - mb[0]).max(1.0), (mb[3] - mb[1]).max(1.0));
                    let (x0, x1) = (v[0].min(v[2]), v[0].max(v[2]));
                    let (y0, y1) = (v[1].min(v[3]), v[1].max(v[3]));
                    s.rect = Some([(x0 - mb[0]) / pw, 1.0 - (y1 - mb[1]) / ph, (x1 - mb[0]) / pw, 1.0 - (y0 - mb[1]) / ph].map(|t| t.clamp(0.0, 1.0)));
                }
            }
            if page.is_some() {
                s.page = page;
            }
        }
    }
}

/// Cosa è cambiato nel PDF dopo una firma.
#[derive(Clone, Debug, PartialEq)]
pub enum After {
    /// Niente: la firma copre tutto il file.
    Nothing,
    /// Solo altre firme, marche o dati per la verifica nel tempo.
    OnlySignatures,
    /// Annotazioni o campi del modulo, ma il testo e il disegno delle pagine sono uguali.
    Annotations,
    /// Il contenuto delle pagine è cambiato: quali (da 1).
    Content(Vec<u32>),
}

/// Confronta la versione firmata (i byte fino a `end`) con il file intero.
pub fn changes_after(data: &[u8], end: usize) -> After {
    let tail = &data[end.min(data.len())..];
    // Dopo «%%EOF» possono esserci solo a capo o spazi.
    if tail.iter().all(|c| c.is_ascii_whitespace() || *c == 0) {
        return After::Nothing;
    }
    let old = lopdf::Document::load_mem(&data[..end]);
    let new = lopdf::Document::load_mem(data);
    let (Ok(old), Ok(new)) = (old, new) else {
        return After::Annotations;
    };
    let po = old.get_pages();
    let pn = new.get_pages();
    let mut changed = Vec::new();
    for (n, id_new) in &pn {
        match po.get(n) {
            Some(id_old) => {
                let a = old.get_page_content(*id_old);
                let b = new.get_page_content(*id_new);
                if a != b {
                    changed.push(*n);
                }
            }
            None => changed.push(*n),
        }
    }
    if po.len() > pn.len() {
        changed.extend((pn.len() as u32 + 1)..=(po.len() as u32));
    }
    if !changed.is_empty() {
        return After::Content(changed);
    }
    // Il resto: altre firme, marche, dati per la verifica nel tempo (DSS) o annotazioni.
    if annotation_changes(&old, &new) {
        After::Annotations
    } else {
        After::OnlySignatures
    }
}

/// Annotazioni nuove o diverse che non sono riquadri di firma.
fn annotation_changes(old: &lopdf::Document, new: &lopdf::Document) -> bool {
    let list = |d: &lopdf::Document| -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for (_, pid) in d.get_pages() {
            let Ok(pd) = d.get_dictionary(pid) else { continue };
            let Ok((_, lopdf::Object::Array(a))) = pd.get(b"Annots").and_then(|o| d.dereference(o)) else { continue };
            for o in a {
                let Ok((_, lopdf::Object::Dictionary(ad))) = d.dereference(o) else { continue };
                let sub = ad.get(b"Subtype").ok().and_then(|s| s.as_name().ok()).unwrap_or(b"");
                let ft = ad.get(b"FT").ok().and_then(|s| s.as_name().ok()).unwrap_or(b"");
                if sub == b"Widget" && (ft == b"Sig" || ad.get(b"Parent").is_ok() && ad.get(b"V").is_err()) {
                    continue;
                }
                let mut key = sub.to_vec();
                if let Ok(c) = ad.get(b"Contents").and_then(|c| c.as_str()) {
                    key.extend_from_slice(c);
                }
                if let Ok(v) = ad.get(b"V") {
                    key.extend_from_slice(format!("{v:?}").as_bytes());
                }
                if let Ok(r) = ad.get(b"Rect") {
                    key.extend_from_slice(format!("{r:?}").as_bytes());
                }
                out.push(key);
            }
        }
        out.sort();
        out
    };
    list(old) != list(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date() {
        // 05/11/2026 10:42:31 ora italiana = 09:42:31 UTC
        let t = pdf_date("D:20261105104231+01'00'").unwrap();
        let (y, m, d) = super::super::asn1::civil_from_days(t.div_euclid(86400));
        assert_eq!((y, m, d), (2026, 11, 5));
        assert_eq!(t.rem_euclid(86400), 9 * 3600 + 42 * 60 + 31);
    }

    #[test]
    fn strings() {
        assert_eq!(pdf_string(b"/Reason (Approvazione \\(ok\\))/M", b"/Reason").as_deref(), Some("Approvazione (ok)"));
        assert_eq!(pdf_string(b"/Name <FEFF004D0052>", b"/Name").as_deref(), Some("MR"));
        // «\8» non è un ottale: si tiene la cifra (prima andava in panico per un conto sotto zero).
        assert_eq!(pdf_string(b"/Reason (a\\8b)", b"/Reason").as_deref(), Some("a8b"));
        assert_eq!(pdf_string(b"/SubFilter/ETSI.CAdES.detached/Type", b"/SubFilter").as_deref(), Some("ETSI.CAdES.detached"));
    }

    #[test]
    fn byte_range_strano_non_manda_in_panico() {
        // /ByteRange molto dopo il buco e senza «obj» prima: il dizionario non deve finire al contrario.
        let mut pdf = b"%PDF-1.4\n<3003020100>".to_vec();
        pdf.extend(std::iter::repeat_n(b' ', 3000));
        pdf.extend(b"/ByteRange [0 9 21 5]");
        let _ = signatures(&pdf);
        // Numeri che traboccano.
        let mut pdf = b"%PDF-1.4\n<3003020100>".to_vec();
        pdf.extend(format!("/ByteRange [{} 2 1 5]", usize::MAX).as_bytes());
        assert!(signatures(&pdf).is_empty());
    }
}
