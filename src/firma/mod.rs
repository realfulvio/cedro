//! Firma digitale: aprire e verificare i file firmati (.p7m, .p7s, .m7m, .tsd, .tsr, PDF firmati),
//! con l'elenco europeo dei certificatori, la revoca e la data certa delle marche temporali.
//! Tutto scritto in Rust, senza programmi esterni.

pub mod asn1;
pub mod cert;
pub mod cms;
pub mod crypto;
pub mod oid;
pub mod pades;
pub mod rapporto;
pub mod revoca;
pub mod trust;
pub mod verify;

pub use verify::{verify_bytes, verify_path, At, Check, DocKind, Layer, Level, Opts, Report, Signer, TsReport};

/// Le estensioni dei file che il visualizzatore apre col doppio clic.
pub const SIGNED_EXTENSIONS: &[&str] = &["p7m", "p7s", "m7m", "tsd", "tsr", "p7e"];

/// È un file firmato (dal nome)?
pub fn is_signed_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    SIGNED_EXTENSIONS.iter().any(|e| l.ends_with(&format!(".{e}")))
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// L'ora italiana (CET/CEST, regola europea dell'ora legale) di un istante UTC.
pub fn italy(t: i64) -> i64 {
    let (y, _, _) = asn1::civil_from_days(t.div_euclid(86400));
    let last_sunday = |m: i64| {
        let last = asn1::days_from_civil(y, m, 31);
        // 1970-01-01 era giovedì: domenica ha resto 3.
        last - (last - 3).rem_euclid(7)
    };
    let start = last_sunday(3) * 86400 + 3600;
    let end = last_sunday(10) * 86400 + 3600;
    t + if t >= start && t < end { 7200 } else { 3600 }
}

pub struct Local {
    pub y: i64,
    pub mo: i64,
    pub d: i64,
    pub h: i64,
    pub mi: i64,
    pub s: i64,
}

pub fn local(t: i64) -> Local {
    let l = italy(t);
    let (y, mo, d) = asn1::civil_from_days(l.div_euclid(86400));
    let r = l.rem_euclid(86400);
    Local { y, mo, d, h: r / 3600, mi: r % 3600 / 60, s: r % 60 }
}

/// 14/03/2027
pub fn date(t: i64) -> String {
    let l = local(t);
    format!("{:02}/{:02}/{}", l.d, l.mo, l.y)
}

/// 05/11/2026 10:42:31
pub fn date_time(t: i64) -> String {
    let l = local(t);
    format!("{:02}/{:02}/{} {:02}:{:02}:{:02}", l.d, l.mo, l.y, l.h, l.mi, l.s)
}

/// 10:42
pub fn hm(t: i64) -> String {
    let l = local(t);
    format!("{:02}:{:02}", l.h, l.mi)
}

const MONTHS: [&str; 12] = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];

/// 5 novembre 2026
pub fn long_date(t: i64) -> String {
    let l = local(t);
    format!("{} {} {}", l.d, MONTHS[(l.mo - 1).clamp(0, 11) as usize], l.y)
}

/// «oggi alle 14:36», «ieri alle 17:48», «il 30/10/2026»
pub fn relative(t: i64) -> String {
    let day = |x: i64| italy(x).div_euclid(86400);
    let diff = day(now()) - day(t);
    match diff {
        0 => format!("oggi alle {}", hm(t)),
        1 => format!("ieri alle {}", hm(t)),
        _ => format!("il {}", date(t)),
    }
}

/// «Mario Rossi» da «MARIO ROSSI».
pub fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ora_italiana() {
        // 9 novembre 2026 10:02 UTC = 11:02 ora solare
        let t = asn1::days_from_civil(2026, 11, 9) * 86400 + 10 * 3600 + 2 * 60;
        assert_eq!(date_time(t), "09/11/2026 11:02:00");
        // 1 luglio: ora legale, +2
        let t = asn1::days_from_civil(2026, 7, 1) * 86400 + 10 * 3600;
        assert_eq!(hm(t), "12:00");
        // 2026: l'ora legale comincia il 29 marzo alle 01:00 UTC
        let t = asn1::days_from_civil(2026, 3, 29) * 86400 + 3600 - 1;
        assert_eq!(hm(t), "01:59");
        assert_eq!(hm(t + 1), "03:00");
        assert_eq!(long_date(t), "29 marzo 2026");
        assert_eq!(title_case("MARIO ROSSI"), "Mario Rossi");
    }
}

#[cfg(test)]
mod prove;
