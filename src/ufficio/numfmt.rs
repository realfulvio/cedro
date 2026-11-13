//! I formati dei numeri di Excel e Calc («#.##0,00 €», «gg/mm/aaaa», «0%», «[Rosso]-0,00»…): il
//! valore della cella scritto come lo mostra il foglio, con la virgola dei decimali e i nomi dei mesi
//! della lingua di Cedro.

use super::pagina::Rgb;

/// Le convenzioni della lingua: separatori, ordine della data, nomi.
pub struct Locale {
    pub dec: char,
    pub thou: char,
    pub months: [&'static str; 12],
    pub days: [&'static str; 7],
    /// La data «breve» del sistema (formato 14 di Excel).
    pub short_date: &'static str,
}

pub const IT: Locale = Locale {
    dec: ',',
    thou: '.',
    months: ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
    days: ["domenica", "lunedì", "martedì", "mercoledì", "giovedì", "venerdì", "sabato"],
    short_date: "dd/mm/yyyy",
};

pub const EN: Locale = Locale {
    dec: '.',
    thou: ',',
    months: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    days: ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    short_date: "m/d/yyyy",
};

pub fn locale() -> &'static Locale {
    if crate::i18n::english() {
        &EN
    } else {
        &IT
    }
}

/// Il codice dei formati che Excel non scrive nel file (gli id fissi).
pub fn builtin(id: u32, l: &Locale) -> String {
    let date_time = format!("{} h:mm", l.short_date);
    match id {
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "#,##0 \"€\";-#,##0 \"€\"",
        6 => "#,##0 \"€\";[Red]-#,##0 \"€\"",
        7 => "#,##0.00 \"€\";-#,##0.00 \"€\"",
        8 => "#,##0.00 \"€\";[Red]-#,##0.00 \"€\"",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => l.short_date,
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => return date_time,
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        41 => "_-* #,##0_-;-* #,##0_-;_-* \"-\"_-;_-@_-",
        42 => "_-* #,##0 \"€\"_-;-* #,##0 \"€\"_-;_-* \"-\" \"€\"_-;_-@_-",
        43 => "_-* #,##0.00_-;-* #,##0.00_-;_-* \"-\"??_-;_-@_-",
        44 => "_-* #,##0.00 \"€\"_-;-* #,##0.00 \"€\"_-;_-* \"-\"?? \"€\"_-;_-@_-",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mm:ss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => "General",
    }
    .to_string()
}

/// Un pezzo di una sezione del formato.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Lit(String),
    /// Spazio largo come il carattere (`_x`): qui uno spazio.
    Space,
    Digit(char),
    Dec,
    Thou,
    Percent,
    Exp(bool),
    Slash,
    At,
    Date(String),
    AmPm,
    Elapsed(char),
}

struct Section {
    toks: Vec<Tok>,
    color: Option<Rgb>,
    cond: Option<(String, f64)>,
}

fn named_color(name: &str) -> Option<Rgb> {
    Some(match name.to_ascii_lowercase().as_str() {
        "black" | "nero" => (0, 0, 0),
        "red" | "rosso" => (255, 0, 0),
        "green" | "verde" => (0, 128, 0),
        "blue" | "blu" => (0, 0, 255),
        "yellow" | "giallo" => (255, 255, 0),
        "magenta" => (255, 0, 255),
        "cyan" | "ciano" => (0, 255, 255),
        "white" | "bianco" => (255, 255, 255),
        _ => return None,
    })
}

fn parse_section(code: &str) -> Section {
    let mut toks = Vec::new();
    let mut color = None;
    let mut cond = None;
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    let lit = |toks: &mut Vec<Tok>, s: &str| match toks.last_mut() {
        Some(Tok::Lit(l)) => l.push_str(s),
        _ => toks.push(Tok::Lit(s.to_string())),
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let end = chars[i + 1..].iter().position(|&x| x == '"').map_or(chars.len(), |p| i + 1 + p);
                lit(&mut toks, &chars[i + 1..end.min(chars.len())].iter().collect::<String>());
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    lit(&mut toks, &n.to_string());
                }
                i += 2;
                continue;
            }
            '_' => {
                toks.push(Tok::Space);
                i += 2;
                continue;
            }
            '*' => {
                i += 2;
                continue;
            }
            '[' => {
                let end = chars[i + 1..].iter().position(|&x| x == ']').map_or(chars.len(), |p| i + 1 + p);
                let inside: String = chars[i + 1..end.min(chars.len())].iter().collect();
                let low = inside.to_ascii_lowercase();
                if let Some(c) = named_color(&inside) {
                    color = Some(c);
                } else if low.starts_with("color") {
                    color = low[5..].trim().parse::<usize>().ok().and_then(super::foglio::indexed_color);
                } else if let Some(rest) = inside.strip_prefix('$') {
                    // [$€-410]: il simbolo della valuta.
                    lit(&mut toks, rest.split('-').next().unwrap_or(""));
                } else if inside.starts_with(['<', '>', '=']) {
                    let op: String = inside.chars().take_while(|c| matches!(c, '<' | '>' | '=')).collect();
                    let v = inside[op.len()..].trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
                    cond = Some((op, v));
                } else if !low.is_empty() && low.chars().all(|c| c == 'h' || c == 'm' || c == 's') {
                    toks.push(Tok::Elapsed(low.chars().next().unwrap_or('h')));
                }
                i = end + 1;
                continue;
            }
            '0' | '#' | '?' => toks.push(Tok::Digit(c)),
            '.' => toks.push(Tok::Dec),
            ',' => toks.push(Tok::Thou),
            '%' => toks.push(Tok::Percent),
            '@' => toks.push(Tok::At),
            '/' => {
                // Una barra tra cifre è una frazione; nelle date è un separatore.
                let digits_around = toks.iter().rev().take_while(|t| !matches!(t, Tok::Lit(_))).any(|t| matches!(t, Tok::Digit(_))) && chars.get(i + 1).is_some_and(|c| matches!(c, '0' | '#' | '?' | '1'..='9'));
                if digits_around {
                    toks.push(Tok::Slash);
                } else {
                    lit(&mut toks, "/");
                }
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+' | '-')) => {
                toks.push(Tok::Exp(chars[i + 1] == '+'));
                i += 2;
                continue;
            }
            'A' | 'a' if chars[i..].iter().take(5).collect::<String>().eq_ignore_ascii_case("AM/PM") => {
                toks.push(Tok::AmPm);
                i += 5;
                continue;
            }
            'A' | 'a' if chars[i..].iter().take(3).collect::<String>().eq_ignore_ascii_case("A/P") => {
                toks.push(Tok::AmPm);
                i += 3;
                continue;
            }
            'y' | 'Y' | 'd' | 'D' | 'm' | 'M' | 'h' | 'H' | 's' | 'S' | 'g' | 'G' | 'a' => {
                // Lettere ripetute: un pezzo di data o di ora (anche in italiano: g = giorno, a = anno).
                let lower = c.to_ascii_lowercase();
                let mut n = 1;
                while chars.get(i + n).is_some_and(|x| x.to_ascii_lowercase() == lower) {
                    n += 1;
                }
                let unit = match lower {
                    'g' => 'd',
                    'a' => 'y',
                    o => o,
                };
                toks.push(Tok::Date(std::iter::repeat_n(unit, n).collect()));
                i += n;
                continue;
            }
            ' ' | '-' | '+' | '(' | ')' | ':' | '$' | '€' | '£' | '!' | '^' | '&' | '\'' | '~' | '{' | '}' | '<' | '>' | '=' => lit(&mut toks, &c.to_string()),
            other => lit(&mut toks, &other.to_string()),
        }
        i += 1;
    }
    // «m» dopo un'ora o prima dei secondi sono minuti.
    let n = toks.len();
    for k in 0..n {
        if let Tok::Date(d) = &toks[k] {
            if d.starts_with('m') && d.len() <= 2 {
                let prev = toks[..k].iter().rev().find(|t| matches!(t, Tok::Date(_) | Tok::Elapsed(_)));
                let next = toks[k + 1..].iter().find(|t| matches!(t, Tok::Date(_) | Tok::Elapsed(_)));
                let after_h = matches!(prev, Some(Tok::Date(p)) if p.starts_with('h')) || matches!(prev, Some(Tok::Elapsed('h')));
                let before_s = matches!(next, Some(Tok::Date(p)) if p.starts_with('s')) || matches!(next, Some(Tok::Elapsed('s')));
                if after_h || before_s {
                    toks[k] = Tok::Date(d.replace('m', "M"));
                }
            }
        }
    }
    Section { toks, color, cond }
}

fn split_sections(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let (mut quoted, mut bracket, mut escape) = (false, false, false);
    for c in code.chars() {
        if escape {
            out.last_mut().expect("sezione").push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' if !quoted => escape = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().expect("sezione").push(c);
    }
    out
}

/// Il numero di giorni dal 1970 e il giorno civile.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Un numero di serie di Excel come data e ora.
fn date_parts(v: f64, date1904: bool) -> (i64, u32, u32, u32, u32, f64, u32) {
    let days = v.floor() as i64;
    // Come Excel: prima al millesimo di secondo, poi si tagliano le parti che il formato non mostra.
    let mut secs = ((v - v.floor()) * 86_400_000.0).round() / 1000.0;
    let mut day = days;
    if secs >= 86_400.0 {
        day += 1;
        secs = 0.0;
    }
    let base = if date1904 { 24_107 } else if day < 61 { 25_568 } else { 25_569 };
    let unix_day = day - base;
    let (y, m, d) = civil(unix_day);
    let weekday = (unix_day + 4).rem_euclid(7) as u32;
    let whole = secs.floor();
    (y, m, d, (whole / 3600.0) as u32, (whole / 60.0 % 60.0) as u32, secs - (whole - whole % 60.0), weekday)
}

fn group(int: &str, thou: char) -> String {
    let mut out = String::new();
    let n = int.len();
    for (i, c) in int.chars().enumerate() {
        out.push(c);
        if (n - i - 1) % 3 == 0 && i + 1 < n {
            out.push(thou);
        }
    }
    out
}

/// Il formato «Generale»: fino a 10 cifre significative, notazione scientifica per i numeri enormi o minuscoli.
pub fn general(v: f64, l: &Locale) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if !v.is_finite() {
        return "#NUM!".into();
    }
    let a = v.abs();
    if !(1e-9..1e11).contains(&a) {
        let s = format!("{v:.5E}");
        let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
        let m = m.trim_end_matches('0').trim_end_matches('.');
        let e: i32 = e.parse().unwrap_or(0);
        return format!("{}E{}{:02}", m.replace('.', &l.dec.to_string()), if e < 0 { '-' } else { '+' }, e.abs());
    }
    let digits = 10 - (a.log10().floor() as i32 + 1).max(1);
    let s = format!("{:.*}", digits.max(0) as usize, v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', &l.dec.to_string())
}

/// Il testo di un numero con un formato. Restituisce anche il colore chiesto dal formato.
pub fn format(v: f64, code: &str, date1904: bool, l: &Locale) -> (String, Option<Rgb>) {
    let code = code.trim();
    if code.is_empty() || code.eq_ignore_ascii_case("general") || code.eq_ignore_ascii_case("standard") {
        return (general(v, l), None);
    }
    let sections = split_sections(code);
    let parsed: Vec<Section> = sections.iter().map(|s| parse_section(s)).collect();
    // Quale sezione: le condizioni se ci sono, altrimenti positivi;negativi;zero.
    let has_cond = parsed.iter().any(|s| s.cond.is_some());
    let (idx, abs) = if has_cond {
        let ok = |s: &Section| {
            s.cond.as_ref().is_some_and(|(op, x)| match op.as_str() {
                "<" => v < *x,
                "<=" => v <= *x,
                ">" => v > *x,
                ">=" => v >= *x,
                "=" => v == *x,
                "<>" => v != *x,
                _ => false,
            })
        };
        match parsed.iter().position(ok) {
            Some(i) => (i, false),
            None => (parsed.iter().position(|s| s.cond.is_none()).unwrap_or(0), false),
        }
    } else if v < 0.0 && parsed.len() > 1 && !parsed[1].toks.is_empty() {
        (1, true)
    } else if v == 0.0 && parsed.len() > 2 {
        (2, false)
    } else {
        (0, false)
    };
    let sec = &parsed[idx.min(parsed.len() - 1)];
    let x = if abs { v.abs() } else { v };
    (render(x, sec, date1904, l, idx == 0 && !has_cond && parsed.len() > 1 && v < 0.0), sec.color)
}

/// Il testo con il formato del testo (la quarta sezione, «@»), se il formato ne ha una.
pub fn format_text(s: &str, code: &str) -> String {
    let sections = split_sections(code);
    let Some(sec) = sections.get(3).or_else(|| sections.first().filter(|f| f.contains('@'))) else { return s.to_string() };
    let sec = parse_section(sec);
    let mut out = String::new();
    for t in &sec.toks {
        match t {
            Tok::At => out.push_str(s),
            Tok::Lit(l) => out.push_str(l),
            Tok::Space => out.push(' '),
            _ => {}
        }
    }
    out
}

fn render(v: f64, sec: &Section, date1904: bool, l: &Locale, keep_minus: bool) -> String {
    let is_date = sec.toks.iter().any(|t| matches!(t, Tok::Date(_) | Tok::Elapsed(_) | Tok::AmPm));
    if is_date {
        return render_date(v, sec, date1904, l);
    }
    let has_digits = sec.toks.iter().any(|t| matches!(t, Tok::Digit(_)));
    if !has_digits {
        // Solo testo fisso («-», «zero»), o niente: il numero non si vede.
        let mut out = String::new();
        for t in &sec.toks {
            match t {
                Tok::Lit(s) => out.push_str(s),
                Tok::Space => out.push(' '),
                Tok::Percent => out.push('%'),
                Tok::At => out.push_str(&general(v, l)),
                _ => {}
            }
        }
        return out;
    }
    let mut x = v;
    let percent = sec.toks.iter().filter(|t| matches!(t, Tok::Percent)).count();
    for _ in 0..percent {
        x *= 100.0;
    }
    // Le virgole in fondo alle cifre dividono per mille.
    let last_digit = sec.toks.iter().rposition(|t| matches!(t, Tok::Digit(_))).unwrap_or(0);
    let mut scale = 0;
    for t in &sec.toks[last_digit + 1..] {
        match t {
            Tok::Thou => scale += 1,
            Tok::Digit(_) | Tok::Dec => break,
            _ => break,
        }
    }
    for _ in 0..scale {
        x /= 1000.0;
    }
    let exp_at = sec.toks.iter().position(|t| matches!(t, Tok::Exp(_)));
    let slash_at = sec.toks.iter().position(|t| matches!(t, Tok::Slash));
    let dec_at = sec.toks.iter().position(|t| matches!(t, Tok::Dec));
    let grouping = sec.toks.iter().enumerate().any(|(i, t)| matches!(t, Tok::Thou) && i < last_digit && sec.toks[..i].iter().any(|t| matches!(t, Tok::Digit(_))));

    if let Some(si) = slash_at {
        return render_fraction(x, sec, si, l, keep_minus);
    }

    let end_mantissa = exp_at.unwrap_or(sec.toks.len());
    let int_toks: Vec<char> = sec.toks[..dec_at.unwrap_or(end_mantissa).min(end_mantissa)].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
    let frac_toks: Vec<char> = match dec_at {
        Some(d) if d < end_mantissa => sec.toks[d + 1..end_mantissa].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect(),
        _ => Vec::new(),
    };
    let mut exponent = 0i32;
    if exp_at.is_some() && x != 0.0 {
        // Notazione scientifica: le cifre intere decidono il passo dell'esponente (##0.0E+0 → multipli di 3).
        let step = int_toks.len().max(1) as i32;
        let e = x.abs().log10().floor() as i32;
        exponent = if step > 1 { e.div_euclid(step) * step } else { e - (int_toks.iter().filter(|c| **c == '0').count() as i32 - 1).max(0) };
        x /= 10f64.powi(exponent);
    }
    let neg = x < 0.0;
    let ax = x.abs();
    let decimals = frac_toks.len();
    let rounded = format!("{ax:.decimals$}");
    let (int_part, frac_part) = rounded.split_once('.').map_or((rounded.clone(), String::new()), |(a, b)| (a.to_string(), b.to_string()));
    let mut int_part = int_part.trim_start_matches('0').to_string();
    let zeros_needed = int_toks.iter().filter(|c| **c == '0').count();
    while int_part.len() < zeros_needed {
        int_part.insert(0, '0');
    }
    let q_needed = int_toks.iter().filter(|c| **c == '?').count();
    let pad_int = if int_part.len() < zeros_needed + q_needed { " ".repeat(zeros_needed + q_needed - int_part.len()) } else { String::new() };
    let int_text = if grouping { group(&int_part, l.thou) } else { int_part };
    // Le cifre decimali: «0» resta, «#» si toglie se è zero in fondo, «?» diventa spazio.
    let mut frac: Vec<char> = frac_part.chars().collect();
    for (k, t) in frac_toks.iter().enumerate().rev() {
        if frac.get(k) == Some(&'0') && k + 1 == frac.len() {
            match t {
                '#' => {
                    frac.pop();
                }
                '?' => {
                    frac[k] = ' ';
                }
                _ => break,
            }
        } else {
            break;
        }
    }
    let frac_text: String = frac.into_iter().collect();

    let _ = keep_minus;
    let mut out = String::new();
    if neg && ax >= 0.5 * 10f64.powi(-(decimals as i32)) {
        out.push('-');
    }
    let mut int_done = false;
    let mut dec_done = false;
    for (i, t) in sec.toks.iter().enumerate() {
        match t {
            Tok::Lit(s) => out.push_str(s),
            Tok::Space => out.push(' '),
            Tok::Percent => out.push('%'),
            Tok::Digit(_) if dec_at.is_none_or(|d| i < d) && exp_at.is_none_or(|e| i < e) => {
                if !int_done {
                    out.push_str(&pad_int);
                    out.push_str(&int_text);
                    int_done = true;
                }
            }
            Tok::Dec if exp_at.is_none_or(|e| i < e) => {
                if !int_done {
                    out.push_str(&int_text);
                    int_done = true;
                }
                out.push(l.dec);
            }
            Tok::Digit(_) if exp_at.is_none_or(|e| i < e) => {
                if !dec_done {
                    out.push_str(&frac_text);
                    dec_done = true;
                }
            }
            Tok::Exp(plus) => {
                out.push('E');
                if exponent < 0 {
                    out.push('-');
                } else if *plus {
                    out.push('+');
                }
                let digits = sec.toks[i + 1..].iter().filter(|t| matches!(t, Tok::Digit(_))).count().max(1);
                out.push_str(&format!("{:0digits$}", exponent.abs()));
                // Le cifre dell'esponente sono già scritte.
                let rest: String = sec.toks[i + 1..].iter().filter_map(|t| if let Tok::Lit(s) = t { Some(s.as_str()) } else { None }).collect();
                out.push_str(&rest);
                break;
            }
            Tok::At => out.push_str(&general(v, l)),
            _ => {}
        }
    }
    // Il segno meno davanti alle cifre, dopo un eventuale simbolo scritto prima («€ -12»: Excel mette «-€ 12»).
    out
}

fn render_fraction(x: f64, sec: &Section, si: usize, l: &Locale, _keep_minus: bool) -> String {
    let neg = x < 0.0;
    let ax = x.abs();
    let den_digits = sec.toks[si + 1..].iter().take_while(|t| matches!(t, Tok::Digit(_))).count().max(1);
    let fixed: Option<u64> = {
        let lit: String = sec.toks[si + 1..].iter().take_while(|t| matches!(t, Tok::Lit(_))).filter_map(|t| if let Tok::Lit(s) = t { Some(s.as_str()) } else { None }).collect();
        lit.trim().parse().ok()
    };
    // C'è la parte intera se prima del numeratore ci sono cifre separate da uno spazio.
    let has_int = sec.toks[..si].iter().any(|t| matches!(t, Tok::Lit(s) if s.contains(' ')));
    let (int, frac) = if has_int { (ax.trunc(), ax.fract()) } else { (0.0, ax) };
    let max_den = 10u64.pow(den_digits as u32) - 1;
    let (mut best_n, mut best_d) = (0u64, 1u64);
    if let Some(d) = fixed {
        best_d = d.max(1);
        best_n = (frac * best_d as f64).round() as u64;
    } else {
        let mut best_err = f64::MAX;
        for d in 1..=max_den {
            let n = (frac * d as f64).round();
            let err = (frac - n / d as f64).abs();
            if err < best_err - 1e-12 {
                best_err = err;
                best_n = n as u64;
                best_d = d;
            }
        }
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    let int_val = int as u64 + if best_n == best_d && has_int { 1 } else { 0 };
    if best_n == best_d && has_int {
        best_n = 0;
    }
    if has_int && int_val > 0 {
        out.push_str(&group(&int_val.to_string(), l.thou).replace(l.thou, ""));
        if best_n > 0 {
            out.push(' ');
        }
    }
    if best_n > 0 || !has_int || int_val == 0 {
        if !has_int || best_n > 0 {
            out.push_str(&format!("{best_n}/{best_d}"));
        } else {
            out.push('0');
        }
    }
    out
}

fn render_date(v: f64, sec: &Section, date1904: bool, l: &Locale) -> String {
    if v < 0.0 {
        return "#".repeat(8);
    }
    let (y, mo, d, mut h, mi, s, wd) = date_parts(v, date1904);
    let ampm = sec.toks.iter().any(|t| matches!(t, Tok::AmPm));
    let pm = h >= 12;
    if ampm {
        h %= 12;
        if h == 0 {
            h = 12;
        }
    }
    // Le frazioni di secondo: «ss.0».
    let mut out = String::new();
    let mut i = 0;
    let toks = &sec.toks;
    while i < toks.len() {
        match &toks[i] {
            Tok::Lit(s) => out.push_str(s),
            Tok::Space => out.push(' '),
            Tok::AmPm => out.push_str(if pm { "PM" } else { "AM" }),
            Tok::Elapsed(u) => {
                let total = (v * 86_400.0).round() as i64;
                let n = match u {
                    'h' => total / 3600,
                    'm' => total / 60,
                    _ => total,
                };
                out.push_str(&n.to_string());
            }
            Tok::Date(p) => {
                let n = p.len();
                match p.chars().next().unwrap_or('d') {
                    'y' => out.push_str(&if n <= 2 { format!("{:02}", y.rem_euclid(100)) } else { format!("{y:04}") }),
                    'm' => match n {
                        1 => out.push_str(&mo.to_string()),
                        2 => out.push_str(&format!("{mo:02}")),
                        3 => out.push_str(&l.months[(mo - 1) as usize].chars().take(3).collect::<String>()),
                        5 => out.push_str(&l.months[(mo - 1) as usize].chars().take(1).collect::<String>()),
                        _ => out.push_str(l.months[(mo - 1) as usize]),
                    },
                    'M' => out.push_str(&if n == 1 { mi.to_string() } else { format!("{mi:02}") }),
                    'd' => match n {
                        1 => out.push_str(&d.to_string()),
                        2 => out.push_str(&format!("{d:02}")),
                        3 => out.push_str(&l.days[wd as usize].chars().take(3).collect::<String>()),
                        _ => out.push_str(l.days[wd as usize]),
                    },
                    'h' => out.push_str(&if n == 1 { h.to_string() } else { format!("{h:02}") }),
                    's' => {
                        let whole = s.floor() as u32;
                        out.push_str(&if n == 1 { whole.to_string() } else { format!("{whole:02}") });
                        // .0, .00 dopo i secondi
                        if matches!(toks.get(i + 1), Some(Tok::Dec)) {
                            let k = toks[i + 2..].iter().take_while(|t| matches!(t, Tok::Digit('0'))).count();
                            if k > 0 {
                                let f = format!("{:.k$}", s - s.floor());
                                out.push(l.dec);
                                out.push_str(f.split_once('.').map_or("", |x| x.1));
                                i += 1 + k;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Tok::Dec => out.push(l.dec),
            Tok::Digit(c) => out.push(*c),
            Tok::Percent => out.push('%'),
            Tok::Thou => out.push(','),
            Tok::Slash => out.push('/'),
            Tok::At => {}
            Tok::Exp(_) => out.push('E'),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64, code: &str) -> String {
        format(v, code, false, &IT).0
    }

    #[test]
    fn numbers_in_italian() {
        assert_eq!(f(1234.567, "#,##0.00"), "1.234,57");
        assert_eq!(f(-1234.5, "#,##0.00"), "-1.234,50");
        assert_eq!(f(0.125, "0.0%"), "12,5%");
        assert_eq!(f(7.0, "000"), "007");
        assert_eq!(f(1234.0, "#,##0 \"€\""), "1.234 €");
        assert_eq!(f(1234.5, "[$€-410] #,##0.00"), "€ 1.234,50");
        assert_eq!(f(-5.0, "0;[Red](0)"), "(5)");
        assert_eq!(format(-5.0, "0;[Red](0)", false, &IT).1, Some((255, 0, 0)));
        assert_eq!(f(0.0, "0.00;-0.00;\"-\""), "-");
        assert_eq!(f(12345.0, "0.00E+00"), "1,23E+04");
        assert_eq!(f(1.5, "0.##"), "1,5");
        assert_eq!(f(2.0, "0.##"), "2,");
        assert_eq!(f(1500000.0, "#,##0,"), "1.500");
        assert_eq!(f(0.75, "# ?/?"), "3/4");
        assert_eq!(f(1.5, "# ?/?"), "1 1/2");
        assert_eq!(general(0.1 + 0.2, &IT), "0,3");
        assert_eq!(general(1234567.0, &IT), "1234567");
        assert_eq!(general(1.0 / 3.0, &IT), "0,333333333");
        assert_eq!(general(1e12, &IT), "1E+12");
    }

    #[test]
    fn dates_and_times() {
        // 46337 = 11/11/2026.
        assert_eq!(f(46337.0, "dd/mm/yyyy"), "11/11/2026");
        assert_eq!(f(46337.0, "d mmmm yyyy"), "11 novembre 2026");
        assert_eq!(f(46337.0, "dddd"), "mercoledì");
        assert_eq!(f(46337.0, "gg/mm/aaaa"), "11/11/2026");
        assert_eq!(f(46337.0, &builtin(14, &IT)), "11/11/2026");
        assert_eq!(format(46337.0, &builtin(14, &EN), false, &EN).0, "11/11/2026");
        assert_eq!(f(46337.604166666664, "hh:mm"), "14:30");
        assert_eq!(f(46337.604166666664, "h:mm AM/PM"), "2:30 PM");
        assert_eq!(f(1.5, "[h]:mm"), "36:00");
        assert_eq!(f(46337.0, "mmm-yy"), "nov-26");
        assert_eq!(f(0.000694444, "mm:ss"), "01:00");
    }

    #[test]
    fn text_sections() {
        assert_eq!(format_text("Nord", "0;-0;0;\"Zona \"@"), "Zona Nord");
        assert_eq!(format_text("x", "0.00"), "x");
    }
}
