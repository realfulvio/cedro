//! La lingua dell'interfaccia: italiano (quella in cui è scritto il programma) o inglese.
//! Tutti i testi restano in italiano nel codice; dove si disegnano o si mostrano passano da `tr`,
//! che in inglese li sostituisce con quelli di `i18n/en.tsv` (una riga per testo: italiano, TAB, inglese).
//! Un testo che nel file non c'è si vede in italiano: una traduzione mancante non rompe niente.
//! I testi con parti che cambiano («Fatto · {}») hanno `{}` al posto della parte variabile; in inglese
//! si può riordinarle con `{1}`, `{2}`…

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    It,
    En,
}

impl Lang {
    /// Il codice breve (`it`, `en`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
        }
    }
}

/// Le lingue tra cui scegliere nelle Impostazioni: chiave del file, nome nella lingua stessa.
pub const CHOICES: &[(&str, &str)] = &[("it", "Italiano"), ("en", "English")];

static CURRENT: AtomicU8 = AtomicU8::new(0);
static FIXED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Per chi sceglie la lingua da sé (Android): da ora le impostazioni lette non la cambiano più.
pub fn fix() {
    FIXED.store(true, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::It
    }
}

pub fn set(l: Lang) {
    CURRENT.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

/// Vero se l'interfaccia è in inglese.
pub fn english() -> bool {
    lang() == Lang::En
}

/// Chiamata da `settings` a ogni lettura o scrittura: `auto` (o vuoto) segue il sistema, `it` e `en` sono fisse.
pub(crate) fn segui(setting: &str) {
    if FIXED.load(Ordering::Relaxed) {
        return;
    }
    set(from_setting(setting));
}

/// La lingua per un valore di `language`.
pub fn from_setting(setting: &str) -> Lang {
    match setting.trim() {
        "it" => Lang::It,
        "en" => Lang::En,
        _ => detect(),
    }
}

/// La lingua del sistema: italiano se è italiano, inglese per ogni altra.
pub fn detect() -> Lang {
    // I test sono scritti in italiano: non dipendono dalla lingua della macchina che li esegue.
    if cfg!(test) {
        return Lang::It;
    }
    from_locale(&system_locale())
}

/// La lingua per un codice del sistema («it», «it_IT», «en-US»…): per chi la chiede al sistema in un altro modo (Android).
pub fn from_tag(tag: &str) -> Lang {
    from_locale(tag)
}

fn from_locale(tag: &str) -> Lang {
    let t = tag.trim().to_ascii_lowercase();
    if t.starts_with("it") {
        Lang::It
    } else {
        Lang::En
    }
}

#[cfg(windows)]
fn system_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    // SAFETY: il buffer è abbastanza grande (LOCALE_NAME_MAX_LENGTH) e la chiamata scrive al massimo `len` elementi.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)])
    } else {
        String::new()
    }
}

#[cfg(not(windows))]
fn system_locale() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let first = v.split(':').next().unwrap_or("").trim().to_string();
            if !first.is_empty() && first != "C" && first != "POSIX" {
                return first;
            }
        }
    }
    String::new()
}

struct Template {
    /// I pezzi fissi tra un segnaposto e l'altro (uno in più dei segnaposto).
    parts: Vec<String>,
    /// Per ogni segnaposto: vero se è `{#}`, che accetta solo un numero (cifre, punti, virgole, %).
    numeric: Vec<bool>,
    en: String,
}

struct Table {
    exact: HashMap<String, String>,
    templates: Vec<Template>,
    /// Pezzi di frase (righe che iniziano con `~`): per i messaggi fatti di più parti incollate insieme.
    fragments: Vec<(String, String)>,
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Taglia un testo italiano sui segnaposto `{}` (qualsiasi testo) e `{#}` (solo numeri).
fn split_slots(it: &str) -> (Vec<String>, Vec<bool>) {
    let mut parts = vec![String::new()];
    let mut numeric = Vec::new();
    let mut rest = it;
    while let Some(i) = rest.find('{') {
        let tail = &rest[i..];
        let kind = if tail.starts_with("{}") {
            Some((false, 2))
        } else if tail.starts_with("{#}") {
            Some((true, 3))
        } else {
            None
        };
        match kind {
            Some((n, len)) => {
                parts.last_mut().unwrap().push_str(&rest[..i]);
                parts.push(String::new());
                numeric.push(n);
                rest = &tail[len..];
            }
            None => {
                parts.last_mut().unwrap().push_str(&rest[..i + 1]);
                rest = &tail[1..];
            }
        }
    }
    parts.last_mut().unwrap().push_str(rest);
    (parts, numeric)
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '%'))
}

fn parse_table(text: &str) -> Table {
    let mut exact = HashMap::new();
    let mut templates = Vec::new();
    let mut fragments = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((it, en)) = line.split_once('\t') else { continue };
        let (it, en) = (unescape(it), unescape(en));
        if en.is_empty() {
            continue;
        }
        if let Some(piece) = it.strip_prefix('~') {
            fragments.push((piece.to_string(), en));
        } else if it.contains("{}") || it.contains("{#}") {
            let (parts, numeric) = split_slots(&it);
            // Un modello fatto solo di segnaposto e spazi prenderebbe qualsiasi testo.
            if parts.iter().map(|p| p.trim().chars().count()).sum::<usize>() >= 2 {
                templates.push(Template { parts, numeric, en });
            }
        } else {
            exact.insert(it, en);
        }
    }
    // Prima i modelli con più testo fisso: sono i più precisi.
    templates.sort_by_key(|t| std::cmp::Reverse(t.parts.iter().map(|p| p.chars().count()).sum::<usize>()));
    fragments.sort_by_key(|f| std::cmp::Reverse(f.0.chars().count()));
    Table { exact, templates, fragments }
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| parse_table(include_str!("../i18n/en.tsv")))
}

/// Se `s` ha la forma del modello, le parti variabili.
fn captures<'a>(parts: &[String], numeric: &[bool], s: &'a str) -> Option<Vec<&'a str>> {
    let first = parts.first()?;
    let last = parts.last()?;
    if parts.len() < 2 || s.len() < first.len() + last.len() || !s.starts_with(first.as_str()) || !s.ends_with(last.as_str()) {
        return None;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    let mut out = Vec::new();
    for mid in &parts[1..parts.len() - 1] {
        let at = if mid.is_empty() { 0 } else { rest.find(mid.as_str())? };
        out.push(&rest[..at]);
        rest = &rest[at + mid.len()..];
    }
    out.push(rest);
    if out.iter().zip(numeric).any(|(c, num)| *num && !is_number(c)) {
        return None;
    }
    Some(out)
}

fn fill(en: &str, caps: &[&str], depth: u8) -> String {
    let mut out = String::with_capacity(en.len() + 16);
    let mut next = 0usize;
    let mut rest = en;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find('}') else { break };
        let key = &tail[1..end];
        let idx = if key.is_empty() {
            next += 1;
            Some(next - 1)
        } else {
            key.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
        };
        match idx.and_then(|n| caps.get(n)) {
            Some(c) => out.push_str(&translate(c, depth + 1)),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn translate(s: &str, depth: u8) -> String {
    let t = table();
    if let Some(en) = t.exact.get(s) {
        return en.clone();
    }
    if depth < 3 {
        for tpl in &t.templates {
            if let Some(caps) = captures(&tpl.parts, &tpl.numeric, s) {
                return fill(&tpl.en, &caps, depth);
            }
        }
    }
    // Messaggi fatti di più frasi incollate: ognuna si traduce per conto suo.
    if depth == 0 && s.contains(". ") {
        let pieces: Vec<&str> = s.split_inclusive(". ").map(str::trim).collect();
        let done: Vec<String> = pieces.iter().map(|p| translate(p, 1)).collect();
        if done.iter().zip(&pieces).any(|(d, p)| d != p) {
            return done.join(" ");
        }
    }
    if depth <= 1 && !t.fragments.is_empty() {
        let mut out = s.to_string();
        for (it, en) in &t.fragments {
            if out.contains(it.as_str()) {
                out = out.replace(it.as_str(), en);
            }
        }
        return out;
    }
    s.to_string()
}

fn cache() -> &'static Mutex<HashMap<String, String>> {
    static C: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

static RECORD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MISSES: Mutex<std::collections::BTreeSet<String>> = Mutex::new(std::collections::BTreeSet::new());

/// Per i controlli: ricorda i testi mostrati in inglese che non hanno traduzione (vedi `misses`).
pub fn record_misses(on: bool) {
    RECORD.store(on, Ordering::Relaxed);
}

/// I testi senza traduzione visti da quando è acceso `record_misses`.
pub fn misses() -> Vec<String> {
    MISSES.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

const LIT_OPEN: char = '\u{E000}';
const LIT_CLOSE: char = '\u{E001}';
const LIT_HOLD: char = '\u{E002}';

/// Un testo dell'utente (un nome di file, per esempio) da lasciare com'è: `tr` non lo traduce e toglie i segni.
pub fn lit(s: &str) -> String {
    format!("{LIT_OPEN}{s}{LIT_CLOSE}")
}

/// Il testo nella lingua scelta. In italiano è il testo stesso, senza costo.
pub fn tr(s: &str) -> Cow<'_, str> {
    if !s.contains(LIT_OPEN) {
        return tr_plain(s);
    }
    // Le parti dell'utente si mettono da parte, si traduce il resto, poi tornano al loro posto.
    let mut kept: Vec<&str> = Vec::new();
    let mut shell = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(LIT_OPEN) {
        shell.push_str(&rest[..i]);
        let after = &rest[i + LIT_OPEN.len_utf8()..];
        let end = after.find(LIT_CLOSE).unwrap_or(after.len());
        shell.push(LIT_HOLD);
        shell.push_str(&kept.len().to_string());
        shell.push(LIT_HOLD);
        kept.push(&after[..end]);
        rest = after.get(end + LIT_CLOSE.len_utf8()..).unwrap_or("");
    }
    shell.push_str(rest);
    let mut out = tr_plain(&shell).into_owned();
    for (i, k) in kept.iter().enumerate() {
        out = out.replace(&format!("{LIT_HOLD}{i}{LIT_HOLD}"), k);
    }
    Cow::Owned(out)
}

fn tr_plain(s: &str) -> Cow<'_, str> {
    if lang() == Lang::It || s.is_empty() {
        return Cow::Borrowed(s);
    }
    let t = table();
    if let Some(en) = t.exact.get(s) {
        // La tabella vive quanto il programma: nessuna copia.
        return Cow::Borrowed(en.as_str());
    }
    // Un testo con numeri (percentuali, pesi, tempi) cambia a ogni avanzamento: ricordarlo riempirebbe
    // la memoria e farebbe dimenticare i testi fissi.
    if s.bytes().any(|b| b.is_ascii_digit()) {
        let v = translate(s, 0);
        note_miss(s, &v);
        return Cow::Owned(v);
    }
    // I modelli costano di più: il risultato (anche «nessuna traduzione») si ricorda.
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = c.get(s) {
        return Cow::Owned(v.clone());
    }
    let v = translate(s, 0);
    note_miss(s, &v);
    if c.len() > 4000 {
        c.clear();
    }
    c.insert(s.to_string(), v.clone());
    Cow::Owned(v)
}

/// Per i controlli: un testo rimasto in italiano.
fn note_miss(s: &str, v: &str) {
    if v == s && RECORD.load(Ordering::Relaxed) && s.chars().any(|ch| ch.is_alphabetic()) {
        MISSES.lock().unwrap_or_else(|e| e.into_inner()).insert(s.to_string());
    }
}

/// Come `tr`, ma restituisce un `String`.
pub fn tr_string(s: &str) -> String {
    tr(s).into_owned()
}

/// Le parole tra le quali si sceglie nelle Impostazioni (per l'OCR e per la lingua).
pub fn choice_names() -> Vec<&'static str> {
    CHOICES.iter().map(|c| c.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_stays_as_it_is() {
        let t = lit("Foto e altri ricordi.jpg");
        assert_eq!(tr(&t), "Foto e altri ricordi.jpg");
    }

    #[test]
    fn locale_tags() {
        assert_eq!(from_locale("it_IT.UTF-8"), Lang::It);
        assert_eq!(from_locale("it-IT"), Lang::It);
        assert_eq!(from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(from_locale("de-DE"), Lang::En);
        assert_eq!(from_locale(""), Lang::En);
    }

    #[test]
    fn templates_and_exact() {
        let t = parse_table("Esci\tExit\nFatto · {}\tDone · {}\n{} file su {}\t{2} files, {1} done\n");
        assert_eq!(t.exact.get("Esci").map(String::as_str), Some("Exit"));
        let tpl = t.templates.iter().find(|x| x.parts[0] == "Fatto · ").unwrap();
        let caps = captures(&tpl.parts, &tpl.numeric, "Fatto · 3 file").unwrap();
        assert_eq!(caps, vec!["3 file"]);
        let two = t.templates.iter().find(|x| x.parts.len() == 3).unwrap();
        let caps = captures(&two.parts, &two.numeric, "2 file su 5").unwrap();
        assert_eq!(fill(&two.en, &caps, 0), "5 files, 2 done");
        assert!(captures(&tpl.parts, &tpl.numeric, "Altro testo").is_none());
        let t = parse_table("{#} file\t{} files\n");
        let n = &t.templates[0];
        assert!(captures(&n.parts, &n.numeric, "12 file").is_some());
        assert!(captures(&n.parts, &n.numeric, "Aggiungi file").is_none());
    }

    #[test]
    fn real_table_is_well_formed() {
        let text = include_str!("../i18n/en.tsv");
        let mut seen = std::collections::HashSet::new();
        for (n, line) in text.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (it, en) = line.split_once('\t').unwrap_or_else(|| panic!("riga {}: manca il TAB", n + 1));
            assert!(!en.contains('\t'), "riga {}: due TAB", n + 1);
            assert!(!en.trim().is_empty(), "riga {}: traduzione vuota", n + 1);
            assert!(seen.insert(it.to_string()), "riga {}: testo doppio «{it}»", n + 1);
            if it.starts_with('~') {
                continue;
            }
            let slots = it.matches("{}").count() + it.matches("{#}").count();
            for n in 1..=9 {
                if en.contains(&format!("{{{n}}}")) {
                    assert!(n <= slots, "riga {}: {{{n}}} senza segnaposto in italiano", n);
                }
            }
        }
    }
}
