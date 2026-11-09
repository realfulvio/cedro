//! Un lettore ASN.1 piccolo e tollerante: legge il DER e anche il BER (lunghezze indefinite, stringhe
//! spezzate in pezzi) che si trova nei .p7m fatti dai programmi vecchi. Serve solo a leggere: i pezzi
//! restano fette del file, così le firme si controllano sui byte originali.

/// Un elemento: tipo, se contiene altri elementi, i byte interi e il contenuto.
#[derive(Clone, Copy, Debug)]
pub struct Node<'a> {
    /// Classe e numero insieme: 0x02 INTEGER, 0x30 SEQUENCE, 0xA0 [0] costruito, eccetera
    /// (il bit «costruito» è compreso, come nel primo byte del DER).
    pub tag: u8,
    /// Numero del tipo quando non sta nel primo byte (tag lunghi, rarissimi): altrimenti 0.
    pub long_tag: u32,
    pub constructed: bool,
    /// Tutto l'elemento, intestazione compresa (e i due zeri finali se la lunghezza è indefinita).
    pub raw: &'a [u8],
    /// Solo il contenuto.
    pub body: &'a [u8],
    /// Lunghezza indefinita (BER).
    pub indefinite: bool,
}

pub type R<T> = Result<T, String>;

fn bad(what: &str) -> String {
    format!("struttura ASN.1 non valida ({what})")
}

/// Legge un elemento all'inizio di `data`; restituisce l'elemento e quello che resta.
pub fn read(data: &[u8]) -> R<(Node<'_>, &[u8])> {
    read_depth(data, 0)
}

fn read_depth(data: &[u8], depth: u32) -> R<(Node<'_>, &[u8])> {
    if depth > 64 {
        return Err(bad("troppo annidato"));
    }
    if data.len() < 2 {
        return Err(bad("finisce troppo presto"));
    }
    let tag = data[0];
    let constructed = tag & 0x20 != 0;
    let mut i = 1;
    let mut long_tag = 0u32;
    if tag & 0x1F == 0x1F {
        loop {
            let b = *data.get(i).ok_or_else(|| bad("tag"))?;
            i += 1;
            long_tag = long_tag.checked_mul(128).ok_or_else(|| bad("tag"))? | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                break;
            }
        }
    }
    let lb = *data.get(i).ok_or_else(|| bad("lunghezza"))?;
    i += 1;
    if lb == 0x80 {
        // Lunghezza indefinita: finisce con 00 00 dopo i figli.
        if !constructed {
            return Err(bad("lunghezza indefinita su un tipo semplice"));
        }
        let start = i;
        let mut rest = &data[i..];
        loop {
            if rest.len() >= 2 && rest[0] == 0 && rest[1] == 0 {
                let end = data.len() - rest.len();
                let node = Node { tag, long_tag, constructed, raw: &data[..end + 2], body: &data[start..end], indefinite: true };
                return Ok((node, &rest[2..]));
            }
            let (_, r) = read_depth(rest, depth + 1)?;
            rest = r;
        }
    }
    let len = if lb & 0x80 == 0 {
        lb as usize
    } else {
        let n = (lb & 0x7F) as usize;
        if n == 0 || n > 8 {
            return Err(bad("lunghezza"));
        }
        let mut l = 0usize;
        for _ in 0..n {
            let b = *data.get(i).ok_or_else(|| bad("lunghezza"))?;
            i += 1;
            l = l.checked_mul(256).ok_or_else(|| bad("lunghezza"))? | b as usize;
        }
        l
    };
    let end = i.checked_add(len).ok_or_else(|| bad("lunghezza"))?;
    if end > data.len() {
        return Err(bad("il file è troncato"));
    }
    Ok((Node { tag, long_tag, constructed, raw: &data[..end], body: &data[i..end], indefinite: false }, &data[end..]))
}

/// Legge un elemento che deve occupare tutto `data` (accetta zeri di riempimento in coda).
pub fn parse(data: &[u8]) -> R<Node<'_>> {
    let (n, rest) = read(data)?;
    if rest.iter().any(|&b| b != 0) && rest.len() > 2 {
        // Alcuni .p7m hanno un a capo o spazzatura dopo la busta: si ignora.
    }
    Ok(n)
}

impl<'a> Node<'a> {
    /// I figli di un elemento costruito.
    pub fn children(&self) -> R<Vec<Node<'a>>> {
        if !self.constructed {
            return Err(bad("non contiene elementi"));
        }
        let mut out = Vec::new();
        let mut rest = self.body;
        while !rest.is_empty() {
            let (n, r) = read(rest)?;
            out.push(n);
            rest = r;
        }
        Ok(out)
    }

    /// Il numero di contesto ([0], [1]…) se l'elemento è di classe contesto.
    pub fn ctx(&self) -> Option<u8> {
        (self.tag & 0xC0 == 0x80).then_some(self.tag & 0x1F)
    }

    pub fn is(&self, tag: u8) -> bool {
        self.tag == tag
    }

    /// Il contenuto di una OCTET STRING (o BIT STRING senza il primo byte), anche spezzata in pezzi (BER).
    pub fn octets(&self) -> R<Vec<u8>> {
        if !self.constructed {
            return Ok(self.body.to_vec());
        }
        let mut out = Vec::new();
        for c in self.children()? {
            out.extend(c.octets()?);
        }
        Ok(out)
    }

    /// I bit di una BIT STRING (senza il byte dei bit inutilizzati).
    pub fn bits(&self) -> R<Vec<u8>> {
        let o = self.octets()?;
        if o.is_empty() {
            return Err(bad("bit string vuota"));
        }
        Ok(o[1..].to_vec())
    }

    /// Un OBJECT IDENTIFIER in forma puntata («1.2.840.113549.1.7.2»).
    pub fn oid(&self) -> R<String> {
        if self.tag != 0x06 {
            return Err(bad("manca un identificatore"));
        }
        oid_string(self.body)
    }

    /// Un intero piccolo.
    pub fn int(&self) -> R<i64> {
        if self.body.is_empty() || self.body.len() > 8 {
            return Err(bad("intero"));
        }
        let mut v: i64 = if self.body[0] & 0x80 != 0 { -1 } else { 0 };
        for &b in self.body {
            v = (v << 8) | b as i64;
        }
        Ok(v)
    }

    /// Una data (UTCTime o GeneralizedTime) in secondi dal 1970, UTC.
    pub fn time(&self) -> R<i64> {
        let s = std::str::from_utf8(self.body).map_err(|_| bad("data"))?;
        match self.tag {
            0x17 => parse_time(s, true),
            0x18 => parse_time(s, false),
            _ => Err(bad("data")),
        }
    }

    /// Una stringa (UTF8, Printable, IA5, BMP, Teletex…).
    pub fn string(&self) -> String {
        match self.tag {
            0x1E => {
                // BMPString: UTF-16 big endian.
                let u: Vec<u16> = self.body.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            }
            0x14 => self.body.iter().map(|&b| b as char).collect(), // T61: in pratica latin-1
            _ => match std::str::from_utf8(self.body) {
                Ok(s) => s.to_string(),
                Err(_) => self.body.iter().map(|&b| b as char).collect(),
            },
        }
    }

    /// L'elemento rifatto in DER: lunghezze definite (i .p7m BER le hanno indefinite). Serve per gli
    /// attributi firmati, che si firmano sempre in DER.
    pub fn to_der(&self) -> R<Vec<u8>> {
        let mut out = Vec::new();
        self.der_into(&mut out, None)?;
        Ok(out)
    }

    /// Come `to_der` ma con un altro tag in testa (gli attributi firmati: da [0] a SET).
    pub fn to_der_as(&self, tag: u8) -> R<Vec<u8>> {
        let mut out = Vec::new();
        self.der_into(&mut out, Some(tag))?;
        Ok(out)
    }

    fn der_into(&self, out: &mut Vec<u8>, tag: Option<u8>) -> R<()> {
        let t = tag.unwrap_or(self.tag);
        if !self.indefinite && !self.constructed {
            push_tlv(out, t, self.long_tag, self.body);
            return Ok(());
        }
        if !self.indefinite && tag.is_none() {
            // Già definito: va bene così solo se tutti i figli lo sono. Si ricostruisce comunque, costa poco.
        }
        // OCTET STRING costruita: in DER va unita in un solo pezzo.
        if self.tag & 0x1F == 0x04 && self.tag & 0xC0 == 0 {
            push_tlv(out, t & !0x20, self.long_tag, &self.octets()?);
            return Ok(());
        }
        let mut body = Vec::new();
        for c in self.children()? {
            c.der_into(&mut body, None)?;
        }
        push_tlv(out, t, self.long_tag, &body);
        Ok(())
    }
}

fn push_tlv(out: &mut Vec<u8>, tag: u8, long_tag: u32, body: &[u8]) {
    out.push(tag);
    if tag & 0x1F == 0x1F {
        let mut bytes = Vec::new();
        let mut v = long_tag;
        bytes.push((v & 0x7F) as u8);
        v >>= 7;
        while v > 0 {
            bytes.push(0x80 | (v & 0x7F) as u8);
            v >>= 7;
        }
        bytes.reverse();
        out.extend(bytes);
    }
    push_len(out, body.len());
    out.extend_from_slice(body);
}

pub fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let b = (len as u64).to_be_bytes();
        let skip = b.iter().take_while(|&&x| x == 0).count();
        out.push(0x80 | (8 - skip) as u8);
        out.extend_from_slice(&b[skip..]);
    }
}

/// Scrittura DER minima (per le richieste OCSP e di marca temporale).
pub fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 6);
    push_tlv(&mut out, tag, 0, body);
    out
}

pub fn seq(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

pub fn oid_der(dotted: &str) -> Vec<u8> {
    let nums: Vec<u64> = dotted.split('.').filter_map(|p| p.parse().ok()).collect();
    let mut body = Vec::new();
    if nums.len() >= 2 {
        let mut first = vec![nums[0] * 40 + nums[1]];
        first.extend_from_slice(&nums[2..]);
        for n in first {
            let mut v = n;
            let mut b = vec![(v & 0x7F) as u8];
            v >>= 7;
            while v > 0 {
                b.push(0x80 | (v & 0x7F) as u8);
                v >>= 7;
            }
            b.reverse();
            body.extend(b);
        }
    }
    tlv(0x06, &body)
}

pub fn oid_string(body: &[u8]) -> R<String> {
    if body.is_empty() {
        return Err(bad("identificatore vuoto"));
    }
    let mut parts: Vec<u64> = Vec::new();
    let mut v: u64 = 0;
    for &b in body {
        v = v.checked_mul(128).ok_or_else(|| bad("identificatore"))? | (b & 0x7F) as u64;
        if b & 0x80 == 0 {
            parts.push(v);
            v = 0;
        }
    }
    let first = parts.first().copied().ok_or_else(|| bad("identificatore"))?;
    let (a, b) = if first < 40 { (0, first) } else if first < 80 { (1, first - 40) } else { (2, first - 80) };
    let mut s = format!("{a}.{b}");
    for p in &parts[1..] {
        s.push('.');
        s.push_str(&p.to_string());
    }
    Ok(s)
}

/// Giorni dal 1970-01-01 (algoritmo di Howard Hinnant).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// «YYMMDDhhmm[ss]Z» o «YYYYMMDDhhmmss[.fff]Z», anche con lo scostamento ±hhmm.
pub fn parse_time(s: &str, utc: bool) -> R<i64> {
    let b = s.as_bytes();
    let num = |from: usize, n: usize| -> R<i64> {
        s.get(from..from + n).and_then(|t| t.parse::<i64>().ok()).ok_or_else(|| bad("data"))
    };
    let (y, mut i) = if utc {
        let yy = num(0, 2)?;
        (if yy >= 50 { 1900 + yy } else { 2000 + yy }, 2)
    } else {
        (num(0, 4)?, 4)
    };
    let mo = num(i, 2)?;
    let d = num(i + 2, 2)?;
    let h = num(i + 4, 2)?;
    let mi = num(i + 6, 2)?;
    i += 8;
    let mut sec = 0;
    if b.get(i).is_some_and(|c| c.is_ascii_digit()) {
        sec = num(i, 2)?;
        i += 2;
    }
    if b.get(i) == Some(&b'.') || b.get(i) == Some(&b',') {
        i += 1;
        while b.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
    }
    let mut off = 0;
    match b.get(i) {
        Some(b'+') | Some(b'-') => {
            let sign = if b[i] == b'+' { 1 } else { -1 };
            off = sign * (num(i + 1, 2)? * 3600 + num(i + 3, 2).unwrap_or(0) * 60);
        }
        _ => {}
    }
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return Err(bad("data"));
    }
    Ok(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec - off)
}

/// GeneralizedTime in DER per le richieste.
pub fn gen_time(secs: i64) -> Vec<u8> {
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let r = secs.rem_euclid(86400);
    tlv(0x18, format!("{y:04}{m:02}{d:02}{:02}{:02}{:02}Z", r / 3600, r / 60 % 60, r % 60).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_indefinite_and_chunked() {
        // SEQUENCE (indefinita) { OCTET STRING costruita indefinita { "ab", "c" } }
        let data = [0x30, 0x80, 0x24, 0x80, 0x04, 0x02, b'a', b'b', 0x04, 0x01, b'c', 0, 0, 0, 0];
        let n = parse(&data).unwrap();
        assert!(n.indefinite);
        let k = n.children().unwrap();
        assert_eq!(k.len(), 1);
        assert_eq!(k[0].octets().unwrap(), b"abc");
        assert_eq!(n.to_der().unwrap(), vec![0x30, 0x05, 0x04, 0x03, b'a', b'b', b'c']);
    }

    #[test]
    fn oids_and_times() {
        let d = oid_der("1.2.840.113549.1.7.2");
        let n = parse(&d).unwrap();
        assert_eq!(n.oid().unwrap(), "1.2.840.113549.1.7.2");
        assert_eq!(parse_time("261105104231Z", true).unwrap(), parse_time("20261105104231Z", false).unwrap());
        let t = parse_time("20261105104231Z", false).unwrap();
        assert_eq!(civil_from_days(t / 86400), (2026, 11, 5));
        assert_eq!(parse_time("20261105114231+0100", false).unwrap(), t);
    }
}
