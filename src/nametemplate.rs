//! Il nome del file di uscita a modello: `%nome%_%data%` diventa «foto_2026-10-28».

/// Data e ora locali, come numeri.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub unix: u64,
}

/// L'ora di adesso nel fuso del sistema.
pub fn now() -> Stamp {
    local(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}

/// Un istante Unix nel fuso del sistema.
pub fn local(unix: u64) -> Stamp {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::SYSTEMTIME;
        use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
        // Dal fuso di Windows, con le ore legali di quella data (non quelle di adesso).
        let u = utc(unix);
        let from = SYSTEMTIME { wYear: u.year as u16, wMonth: u.month as u16, wDayOfWeek: 0, wDay: u.day as u16, wHour: u.hour as u16, wMinute: u.minute as u16, wSecond: u.second as u16, wMilliseconds: 0 };
        let mut to = SYSTEMTIME::default();
        if unsafe { SystemTimeToTzSpecificLocalTime(None, &from, &mut to) }.is_ok() {
            return Stamp { year: to.wYear as i32, month: to.wMonth as u32, day: to.wDay as u32, hour: to.wHour as u32, minute: to.wMinute as u32, second: to.wSecond as u32, unix };
        }
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let secs = unix as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
            return Stamp {
                year: tm.tm_year + 1900,
                month: (tm.tm_mon + 1) as u32,
                day: tm.tm_mday as u32,
                hour: tm.tm_hour as u32,
                minute: tm.tm_min as u32,
                second: tm.tm_sec as u32,
                unix,
            };
        }
    }
    #[allow(unreachable_code)]
    utc(unix)
}

/// Data e ora UTC di un istante Unix (calendario gregoriano, senza librerie).
pub fn utc(unix: u64) -> Stamp {
    let days = (unix / 86_400) as i64;
    let rest = (unix % 86_400) as u32;
    // Algoritmo di Howard Hinnant per i giorni dal 1970.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
    Stamp { year, month, day, hour: rest / 3600, minute: rest % 3600 / 60, second: rest % 60, unix }
}

/// Toglie dal nome i caratteri che i file system non accettano.
fn clean(name: &str) -> String {
    let swapped: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    swapped.trim().trim_end_matches('.').trim().to_string()
}

/// Il nome secondo il modello. Segnaposto: `%nome%` (il nome del file di partenza, senza estensione),
/// `%ext%` (la sua estensione), `%data%` (2026-10-28), `%ora%` (14-05-09), `%unix%`; in inglese
/// anche `%name%`, `%date%`, `%time%`.
/// `tail` è la parte che il motore aggiunge in fondo (per esempio «-compresso»). Se il modello è
/// vuoto o dà un nome vuoto, ritorna `None` e vale il nome di sempre.
pub fn expand(template: &str, source_stem: &str, source_ext: &str, tail: &str, when: Stamp) -> Option<String> {
    expand_caso(template, source_stem, source_ext, tail, when, "")
}

/// Come `expand`, con il caso o campione di lavoro: `%caso%` (in inglese `%case%`) diventa il suo codice.
/// Senza caso il segnaposto sparisce e con lui i trattini bassi che gli stavano attorno: «%data%_%caso%_%nome%»
/// dà «2026-10-28_foto» e non «2026-10-28__foto».
pub fn expand_caso(template: &str, source_stem: &str, source_ext: &str, tail: &str, when: Stamp, caso: &str) -> Option<String> {
    let template = template.trim();
    if template.is_empty() {
        return None;
    }
    let data = format!("{:04}-{:02}-{:02}", when.year, when.month, when.day);
    let ora = format!("{:02}-{:02}-{:02}", when.hour, when.minute, when.second);
    let caso = clean(caso);
    let mut body = template
        .replace("%caso%", &caso)
        .replace("%case%", &caso)
        .replace("%nome%", source_stem)
        .replace("%name%", source_stem)
        .replace("%date%", &data)
        .replace("%time%", &ora)
        .replace("%ext%", source_ext)
        .replace("%data%", &data)
        .replace("%ora%", &ora)
        .replace("%unix%", &when.unix.to_string());
    if caso.is_empty() && (template.contains("%caso%") || template.contains("%case%")) {
        // Il segnaposto vuoto non lascia separatori doppi né in testa.
        while body.contains("__") {
            body = body.replace("__", "_");
        }
        body = body.trim_matches('_').to_string();
    }
    let name = clean(&format!("{body}{tail}"));
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_matches_known_dates() {
        let t = utc(0);
        assert_eq!((t.year, t.month, t.day, t.hour), (1970, 1, 1, 0));
        // 2026-10-28 14:05:09 UTC
        let t = utc(1_793_196_309);
        assert_eq!((t.year, t.month, t.day, t.hour, t.minute, t.second), (2026, 10, 28, 14, 5, 9));
        // Un 29 febbraio.
        let t = utc(1_709_164_800);
        assert_eq!((t.year, t.month, t.day), (2024, 2, 29));
    }

    #[test]
    fn template_fills_the_names() {
        let when = utc(1_793_196_309);
        assert_eq!(expand("%nome%_%data%", "foto", "png", "", when).unwrap(), "foto_2026-10-28");
        assert_eq!(expand("%nome%_%ora%", "foto", "png", "-compresso", when).unwrap(), "foto_14-05-09-compresso");
        assert_eq!(expand("Copia di %nome% (%ext%)", "gita", "jpg", "", when).unwrap(), "Copia di gita (jpg)");
        assert_eq!(expand("%name%-%date%", "foto", "png", "", when).unwrap(), "foto-2026-10-28");
        assert_eq!(expand("a/b:c", "x", "y", "", when).unwrap(), "a_b_c");
        assert!(expand("   ", "x", "y", "", when).is_none());
        // Il caso entra nel nome; senza caso sparisce con i suoi separatori.
        assert_eq!(expand_caso("%data%_%caso%_%nome%", "vetrino", "svs", "_anonimo", when, "V-2026-0457").unwrap(), "2026-10-28_V-2026-0457_vetrino_anonimo");
        assert_eq!(expand_caso("%data%_%caso%_%nome%", "vetrino", "svs", "", when, "").unwrap(), "2026-10-28_vetrino");
        assert_eq!(expand_caso("%caso%_%nome%", "vetrino", "svs", "", when, "  ").unwrap(), "vetrino");
        assert_eq!(expand_caso("%case%-%name%", "a", "b", "", when, "X/1").unwrap(), "X_1-a");
        assert_eq!(expand("%nome%_%caso%", "foto", "png", "", when).unwrap(), "foto");
        assert!(expand("...", "x", "y", "", when).is_none());
    }
}
