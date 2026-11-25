//! Date e ore come le scrivono gli strumenti, in ISO 8601 per Allotrope. Senza fuso nel file si
//! scrive +00:00, come fa allotropy.

/// La data e ora ISO («2026-11-24T14:05:00+00:00»). `giorno_prima`: 03/04/2026 è il 3 aprile
/// (Europa); altrimenti il 4 marzo (Stati Uniti). Se una delle due cifre supera 12 vince quella.
pub fn iso(s: &str, giorno_prima: bool) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Fuso già scritto (2026-11-24T14:05:00+01:00 o Z): si tiene.
    let (corpo, fuso) = fuso(s);
    let corpo = corpo.replace('T', " ");
    let mut parti = corpo.split_whitespace();
    let data = parti.next()?;
    let mut resto: Vec<&str> = parti.collect();
    // «31 Aug 2021  4:31:23 PM»: giorno, mese a lettere, anno.
    let (y, m, d) = match data.parse::<u32>().ok().zip(resto.first().and_then(|m| mese(m))).zip(resto.get(1).and_then(|a| a.parse::<u32>().ok())) {
        Some(((g, m), a)) if g <= 31 && a > 31 => {
            resto.drain(..2);
            (a, m, g)
        }
        _ => data_parti(data, giorno_prima)?,
    };
    let (mut h, mi, se) = match resto.first() {
        Some(o) if o.contains(':') => ora_parti(o)?,
        _ => (0, 0, 0),
    };
    let pm = resto.iter().any(|t| t.eq_ignore_ascii_case("pm") || t.eq_ignore_ascii_case("p.m."));
    let am = resto.iter().any(|t| t.eq_ignore_ascii_case("am") || t.eq_ignore_ascii_case("a.m."));
    if pm && h < 12 {
        h += 12;
    }
    if am && h == 12 {
        h = 0;
    }
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{se:02}{}", fuso.unwrap_or_else(|| "+00:00".into())))
}

/// L'ordine delle date di un file: `true` se il giorno viene prima del mese. Lo dicono le date con
/// una cifra sopra 12; se nessuna lo dice, l'AM/PM è un segno dei file americani.
pub fn giorno_prima<'a>(date: impl IntoIterator<Item = &'a str>) -> bool {
    let mut pm = false;
    for d in date {
        let primo = d.trim().split_whitespace().next().unwrap_or("");
        let sep = if primo.contains('/') { '/' } else if primo.contains('.') { '.' } else { continue };
        let n: Vec<u32> = primo.split(sep).filter_map(|x| x.parse().ok()).collect();
        if n.len() == 3 && n[0] <= 31 {
            if n[0] > 12 {
                return true;
            }
            if n[1] > 12 {
                return false;
            }
        }
        let t = d.to_ascii_lowercase();
        pm |= t.contains(" pm") || t.contains(" am");
    }
    !pm
}

/// Le sigle dei fusi che scrivono gli strumenti americani («… 03:14:51 AM EDT»).
fn sigla(s: &str) -> Option<(String, &'static str)> {
    let t = s.trim_end();
    let (corpo, ultima) = t.rsplit_once(' ')?;
    let off = match ultima {
        "UTC" | "GMT" | "UT" => "+00:00",
        "EDT" => "-04:00",
        "EST" | "CDT" => "-05:00",
        "CST" | "MDT" => "-06:00",
        "MST" | "PDT" => "-07:00",
        "PST" => "-08:00",
        "CET" => "+01:00",
        "CEST" => "+02:00",
        _ => return None,
    };
    Some((corpo.trim_end().to_string(), off))
}

fn fuso(s: &str) -> (String, Option<String>) {
    if let Some((c, off)) = sigla(s) {
        return (c, Some(off.to_string()));
    }
    if let Some(c) = s.strip_suffix('Z') {
        if c.contains(':') {
            return (c.to_string(), Some("+00:00".into()));
        }
    }
    // «…14:05:00+01:00» o «…-05:00»: un segno dopo l'ora.
    if let Some(t) = s.find('T').or_else(|| s.find(' ')) {
        let dopo = &s[t + 1..];
        if let Some(p) = dopo.rfind(['+', '-']) {
            let f = &dopo[p..];
            if f.len() == 6 && f.as_bytes()[3] == b':' && f[1..3].bytes().all(|b| b.is_ascii_digit()) {
                return (s[..t + 1 + p].to_string(), Some(f.to_string()));
            }
        }
    }
    (s.to_string(), None)
}

/// Il numero del mese da «Aug», «agosto», «Sept».
fn mese(m: &str) -> Option<u32> {
    let m = m.trim_end_matches('.').to_ascii_lowercase();
    let nomi = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let it = ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"];
    if m.len() < 3 || !m.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    nomi.iter().chain(it.iter()).position(|x| m.starts_with(x)).map(|i| (i % 12) as u32 + 1)
}

fn data_parti(d: &str, giorno_prima: bool) -> Option<(u32, u32, u32)> {
    let sep = if d.contains('-') { '-' } else if d.contains('/') { '/' } else if d.contains('.') { '.' } else { return None };
    let n: Vec<u32> = d.split(sep).map(|x| x.trim().parse().ok()).collect::<Option<Vec<_>>>()?;
    if n.len() != 3 {
        return None;
    }
    if n[0] > 31 {
        return Some((n[0], n[1], n[2]));
    }
    let y = if n[2] < 100 { 2000 + n[2] } else { n[2] };
    let (a, b) = (n[0], n[1]);
    let (m, d) = if a > 12 {
        (b, a)
    } else if b > 12 {
        (a, b)
    } else if giorno_prima {
        (b, a)
    } else {
        (a, b)
    };
    Some((y, m, d))
}

fn ora_parti(o: &str) -> Option<(u32, u32, u32)> {
    let n: Vec<&str> = o.split(':').collect();
    let h = n.first()?.parse().ok()?;
    let m = n.get(1).map_or(Some(0), |x| x.parse().ok())?;
    let s: u32 = n.get(2).map_or(Some(0.0), |x| x.parse::<f64>().ok())?.floor() as u32;
    Some((h, m, s))
}

#[cfg(test)]
mod prove {
    use super::iso;

    #[test]
    fn date_degli_strumenti() {
        assert_eq!(iso("2021-07-06 09:43:07", false).unwrap(), "2021-07-06T09:43:07+00:00");
        assert_eq!(iso("7/19/2023 3:27:29 PM", false).unwrap(), "2023-07-19T15:27:29+00:00");
        assert_eq!(iso("29/02/2016 14:34:46", true).unwrap(), "2016-02-29T14:34:46+00:00");
        assert_eq!(iso("03/04/2026", true).unwrap(), "2026-04-03T00:00:00+00:00");
        assert_eq!(iso("03/04/2026", false).unwrap(), "2026-03-04T00:00:00+00:00");
        assert_eq!(iso("2026-11-24T14:05:00+01:00", true).unwrap(), "2026-11-24T14:05:00+01:00");
        assert_eq!(iso("2026-11-24 12:00:00 AM", false).unwrap(), "2026-11-24T00:00:00+00:00");
        assert_eq!(iso("31 Aug 2021  4:31:23 PM", true).unwrap(), "2021-08-31T16:31:23+00:00");
        assert_eq!(iso("2 Mar 2023  8:31:38 AM", true).unwrap(), "2023-03-02T08:31:38+00:00");
        assert!(iso("ieri", true).is_none());
    }
}

/// Giorni dal 1970-01-01 di una data del calendario (anni bisestili compresi).
fn giorni_da_civile(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civile_da_giorni(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Una data ISO («2024-01-01T12:00:15+00:00») più dei secondi, con lo stesso fuso.
pub fn piu_secondi(iso: &str, secondi: f64) -> Option<String> {
    if iso.len() < 19 || !secondi.is_finite() {
        return None;
    }
    let n = |a: usize, b: usize| iso.get(a..b).and_then(|t| t.parse::<i64>().ok());
    let (y, m, d, h, mi, se) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let totale = giorni_da_civile(y, m, d) * 86400 + h * 3600 + mi * 60 + se + secondi.round() as i64;
    let (g, resto) = (totale.div_euclid(86400), totale.rem_euclid(86400));
    let (y, m, d) = civile_da_giorni(g);
    Some(format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{}", resto / 3600, resto % 3600 / 60, resto % 60, &iso[19..]))
}
