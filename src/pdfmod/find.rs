//! Cercare nel testo di una pagina: una parola o una frase, oppure un modello (codici fiscali, IBAN, indirizzi
//! e-mail, telefoni). Ogni risultato dà i rettangoli (uno per riga) da evidenziare o da oscurare.

use super::text::TextPage;
use super::Rect;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub text: String,
    /// Un rettangolo per riga di testo.
    pub rects: Vec<Rect>,
}

/// I modelli che si possono cercare tutti insieme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    TaxCode,
    Iban,
    Email,
    Phone,
}

impl Pattern {
    pub const ALL: [Pattern; 4] = [Pattern::TaxCode, Pattern::Iban, Pattern::Email, Pattern::Phone];

    pub fn label(self) -> &'static str {
        match self {
            Pattern::TaxCode => "Codici fiscali",
            Pattern::Iban => "IBAN",
            Pattern::Email => "Indirizzi e-mail",
            Pattern::Phone => "Telefoni",
        }
    }
}

struct Flat {
    chars: Vec<char>,
    map: Vec<Option<usize>>,
}

fn flat(tp: &TextPage) -> Flat {
    let (text, map) = tp.flat();
    let mut chars = Vec::new();
    let mut m = Vec::new();
    for (c, g) in text.chars().zip(map) {
        chars.push(if c == '\n' { ' ' } else { c });
        m.push(g);
    }
    Flat { chars, map: m }
}

fn rects_of(tp: &TextPage, f: &Flat, a: usize, b: usize) -> Vec<Rect> {
    let mut out: Vec<(f32, Rect)> = Vec::new();
    for i in a..b.min(f.map.len()) {
        let Some(g) = f.map[i] else { continue };
        let g = &tp.glyphs[g];
        if g.is_space() {
            continue;
        }
        let r = g.rect();
        match out.last_mut() {
            Some((base, cur)) if (*base - g.base).abs() <= 0.3 * g.size.max(1.0) => *cur = cur.union(&r),
            _ => out.push((g.base, r)),
        }
    }
    out.into_iter().map(|(_, r)| r).collect()
}

fn boundary(chars: &[char], a: usize, b: usize) -> bool {
    let before = a == 0 || !chars[a - 1].is_alphanumeric();
    let after = b >= chars.len() || !chars[b].is_alphanumeric();
    before && after
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Cerca `query` (le frasi vanno anche a capo). `case`: maiuscole e minuscole contano; `whole`: parola intera.
pub fn search(tp: &TextPage, query: &str, case: bool, whole: bool) -> Vec<Hit> {
    let q: Vec<char> = query.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    if q.is_empty() {
        return Vec::new();
    }
    let f = flat(tp);
    // Gli spazi doppi (o un a capo) valgono uno solo.
    let (mut chars, mut idx) = (Vec::with_capacity(f.chars.len()), Vec::with_capacity(f.chars.len()));
    for (i, c) in f.chars.iter().enumerate() {
        if *c == ' ' && chars.last() == Some(&' ') {
            continue;
        }
        chars.push(*c);
        idx.push(i);
    }
    let norm = |c: char| if case { c } else { fold(c) };
    let hay: Vec<char> = chars.iter().map(|c| norm(*c)).collect();
    let needle: Vec<char> = q.iter().map(|c| norm(*c)).collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == needle[..] && (!whole || boundary(&chars, i, i + needle.len())) {
            let (a, b) = (idx[i], idx[i + needle.len() - 1] + 1);
            let rects = rects_of(tp, &f, a, b);
            if !rects.is_empty() {
                hits.push(Hit { text: chars[i..i + needle.len()].iter().collect(), rects });
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}

fn is_digitish(c: char, extra: &str) -> bool {
    c.is_ascii_digit() || extra.contains(c)
}

fn tax_code(c: &[char], i: usize) -> Option<usize> {
    if i + 16 > c.len() {
        return None;
    }
    let s: Vec<char> = c[i..i + 16].iter().map(|c| c.to_ascii_uppercase()).collect();
    let letter = |x: char| x.is_ascii_uppercase();
    let digit = |x: char| x.is_ascii_digit() || "LMNPQRSTUV".contains(x);
    let ok = s[..6].iter().all(|x| letter(*x)) && digit(s[6]) && digit(s[7]) && "ABCDEHLMPRST".contains(s[8]) && digit(s[9]) && digit(s[10]) && letter(s[11]) && digit(s[12]) && digit(s[13]) && digit(s[14]) && letter(s[15]);
    (ok && boundary(c, i, i + 16)).then_some(i + 16)
}

fn iban(c: &[char], i: usize) -> Option<usize> {
    if i + 4 > c.len() || (i > 0 && c[i - 1].is_alphanumeric()) {
        return None;
    }
    if !(c[i].is_ascii_uppercase() && c[i + 1].is_ascii_uppercase() && c[i + 2].is_ascii_digit() && c[i + 3].is_ascii_digit()) {
        return None;
    }
    // Scritto di seguito, oppure a gruppi di 4 separati da uno spazio (l'ultimo gruppo è più corto).
    let run = |from: usize| c[from..].iter().take_while(|x| x.is_ascii_alphanumeric()).count();
    let first = run(i);
    let (mut n, mut end) = (first, i + first);
    if first == 4 {
        while end + 1 < c.len() && c[end] == ' ' {
            let g = run(end + 1);
            if g == 0 || n + g > 34 {
                break;
            }
            n += g;
            end += 1 + g;
            if g < 4 {
                break;
            }
        }
    }
    let ok = if (c[i], c[i + 1]) == ('I', 'T') { n == 27 } else { (15..=34).contains(&n) };
    (ok && (end >= c.len() || !c[end].is_alphanumeric())).then_some(end)
}

fn email(c: &[char], at: usize) -> Option<(usize, usize)> {
    if c[at] != '@' {
        return None;
    }
    let local = |x: char| x.is_ascii_alphanumeric() || "._%+-".contains(x);
    let domain = |x: char| x.is_ascii_alphanumeric() || ".-".contains(x);
    let mut a = at;
    while a > 0 && local(c[a - 1]) {
        a -= 1;
    }
    let mut b = at + 1;
    while b < c.len() && domain(c[b]) {
        b += 1;
    }
    while b > at + 1 && c[b - 1] == '.' {
        b -= 1;
    }
    let dom: String = c[at + 1..b].iter().collect();
    let tld_ok = dom.rsplit('.').next().is_some_and(|t| t.len() >= 2 && t.chars().all(|x| x.is_ascii_alphabetic()));
    (a < at && dom.contains('.') && tld_ok).then_some((a, b))
}

fn phone(c: &[char], i: usize) -> Option<usize> {
    if i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '+') {
        return None;
    }
    if !(c[i] == '+' || c[i] == '0' || c[i] == '3') {
        return None;
    }
    let mut j = i + usize::from(c[i] == '+');
    let (mut digits, mut end) = (0, j);
    while j < c.len() && is_digitish(c[j], " .-/()") {
        if c[j].is_ascii_digit() {
            digits += 1;
            end = j + 1;
        }
        j += 1;
    }
    if !(8..=14).contains(&digits) || (end < c.len() && c[end].is_alphanumeric()) {
        return None;
    }
    // «2024-05-12» o «12/05/2024» non sono telefoni: serve un gruppo lungo.
    let s: String = c[i..end].iter().collect();
    let dates = s.matches(['/', '-']).count() >= 2 && s.split(['/', '-']).any(|p| p.len() <= 2);
    (!dates).then_some(end)
}

/// Tutte le corrispondenze di un modello.
pub fn pattern(tp: &TextPage, p: Pattern) -> Vec<Hit> {
    let f = flat(tp);
    let c = &f.chars;
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let end = match p {
            Pattern::TaxCode => tax_code(c, i).map(|e| (i, e)),
            Pattern::Iban => iban(c, i).map(|e| (i, e)),
            Pattern::Email => email(c, i),
            Pattern::Phone => phone(c, i).map(|e| (i, e)),
        };
        match end {
            Some((a, b)) if b > a && a >= i.saturating_sub(64) => {
                if spans.last().is_none_or(|l| a >= l.1) {
                    spans.push((a, b));
                }
                i = b.max(i + 1);
            }
            _ => i += 1,
        }
    }
    spans
        .into_iter()
        .filter_map(|(a, b)| {
            let rects = rects_of(tp, &f, a, b);
            (!rects.is_empty()).then(|| Hit { text: c[a..b].iter().collect(), rects })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn patterns_match_italian_data() {
        let c = chars("CF RSSMRA85T10A562S ok");
        assert_eq!(tax_code(&c, 3), Some(19));
        let c = chars("IBAN IT60 X054 2811 1010 0000 0123 456 fine");
        assert!(iban(&c, 5).is_some());
        let c = chars("scrivi a mario.rossi@comune.bondeno.fe.it oggi");
        let (a, b) = email(&c, c.iter().position(|x| *x == '@').unwrap()).unwrap();
        assert_eq!(c[a..b].iter().collect::<String>(), "mario.rossi@comune.bondeno.fe.it");
        let c = chars("tel 0532 893111 e 333 1234567.");
        assert!(phone(&c, 4).is_some());
        assert!(phone(&chars("data 12/05/2024"), 5).is_none());
    }
}
