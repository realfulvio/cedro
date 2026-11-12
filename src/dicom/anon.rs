//! Anonimizzazione dei DICOM, sul modello del profilo base dello standard (PS3.15, allegato E) con
//! due scelte pensate per la ricerca: le date si spostano tutte dello stesso numero di giorni per lo
//! stesso paziente (gli intervalli tra gli esami restano veri) e il nome diventa un codice che per lo
//! stesso paziente è sempre uguale. Nei file veterinari via proprietario, allevamento e registro; specie,
//! razza, sesso, età e peso restano, perché servono a chi studia.

use super::{dictionary_vr, tag, text, text_bytes, ui_bytes, DicomFile, Element, Value};

#[derive(Clone, Copy, Debug)]
pub struct Choices {
    /// Vero: date spostate (consigliato); falso: date tolte.
    pub shift_dates: bool,
    /// Vero: nome e codice del paziente diventano un codice fisso; falso: si svuotano.
    pub code_names: bool,
}

impl Default for Choices {
    fn default() -> Self {
        Choices { shift_dates: true, code_names: true }
    }
}

#[derive(Default, Debug)]
pub struct Report {
    /// Cosa è stato tolto o cambiato, in parole semplici e senza doppioni.
    pub done: Vec<&'static str>,
    pub warnings: Vec<String>,
    /// Il codice dato al paziente, se c'era un paziente.
    pub code: Option<String>,
    /// Quanti giorni indietro sono state spostate le date.
    pub shift_days: Option<i64>,
}

impl Report {
    fn add(&mut self, what: &'static str) {
        if !self.done.contains(&what) {
            self.done.push(what);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Remove,
    Empty,
    Code,
}

const NAME: &str = "nome del paziente";
const ID: &str = "codice del paziente";
const BIRTH: &str = "data di nascita";
const OWNER: &str = "proprietario";
const CLINIC: &str = "clinica o ospedale";
const DOCTORS: &str = "medici e operatori";
const CONTACTS: &str = "indirizzi e telefoni";
const NOTES: &str = "note e commenti";
const NUMBERS: &str = "numeri di accettazione e di studio";
const DEVICE: &str = "numero di serie dell'apparecchio";

const RULES: &[(u32, Act, &str)] = &[
    (tag(0x0010, 0x0010), Act::Code, NAME),
    (tag(0x0010, 0x0020), Act::Code, ID),
    (tag(0x0010, 0x0021), Act::Remove, ID),
    (tag(0x0010, 0x0030), Act::Empty, BIRTH),
    (tag(0x0010, 0x0032), Act::Remove, BIRTH),
    (tag(0x0010, 0x1000), Act::Remove, ID),
    (tag(0x0010, 0x1001), Act::Remove, NAME),
    (tag(0x0010, 0x1002), Act::Remove, ID),
    (tag(0x0010, 0x1005), Act::Remove, NAME),
    (tag(0x0010, 0x1040), Act::Remove, CONTACTS),
    (tag(0x0010, 0x1060), Act::Remove, NAME),
    (tag(0x0010, 0x1080), Act::Remove, NOTES),
    (tag(0x0010, 0x1081), Act::Remove, NOTES),
    (tag(0x0010, 0x1090), Act::Remove, ID),
    (tag(0x0010, 0x2150), Act::Remove, CONTACTS),
    (tag(0x0010, 0x2152), Act::Remove, CONTACTS),
    (tag(0x0010, 0x2154), Act::Remove, CONTACTS),
    (tag(0x0010, 0x2160), Act::Remove, NOTES),
    (tag(0x0010, 0x2180), Act::Remove, NOTES),
    (tag(0x0010, 0x21B0), Act::Remove, NOTES),
    (tag(0x0010, 0x4000), Act::Remove, NOTES),
    // Veterinaria: chi è responsabile dell'animale e i registri di razza.
    (tag(0x0010, 0x2294), Act::Remove, OWNER),
    (tag(0x0010, 0x2295), Act::Remove, OWNER),
    (tag(0x0010, 0x2296), Act::Remove, OWNER),
    (tag(0x0010, 0x2297), Act::Remove, OWNER),
    (tag(0x0010, 0x2298), Act::Remove, OWNER),
    (tag(0x0010, 0x2299), Act::Remove, OWNER),
    (tag(0x0008, 0x0050), Act::Empty, NUMBERS),
    (tag(0x0020, 0x0010), Act::Empty, NUMBERS),
    (tag(0x0008, 0x0080), Act::Remove, CLINIC),
    (tag(0x0008, 0x0081), Act::Remove, CLINIC),
    (tag(0x0008, 0x0082), Act::Remove, CLINIC),
    (tag(0x0008, 0x1010), Act::Remove, CLINIC),
    (tag(0x0008, 0x1040), Act::Remove, CLINIC),
    (tag(0x0008, 0x0090), Act::Empty, DOCTORS),
    (tag(0x0008, 0x0092), Act::Remove, CONTACTS),
    (tag(0x0008, 0x0094), Act::Remove, CONTACTS),
    (tag(0x0008, 0x0096), Act::Remove, DOCTORS),
    (tag(0x0008, 0x009C), Act::Remove, DOCTORS),
    (tag(0x0008, 0x009D), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1048), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1049), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1050), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1052), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1060), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1062), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1070), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1072), Act::Remove, DOCTORS),
    (tag(0x0008, 0x1080), Act::Remove, NOTES),
    (tag(0x0008, 0x1120), Act::Remove, ID),
    (tag(0x0008, 0x2111), Act::Remove, NOTES),
    (tag(0x0018, 0x1000), Act::Remove, DEVICE),
    (tag(0x0018, 0x1002), Act::Remove, DEVICE),
    (tag(0x0018, 0x1004), Act::Remove, DEVICE),
    (tag(0x0020, 0x4000), Act::Remove, NOTES),
    (tag(0x0032, 0x1032), Act::Remove, DOCTORS),
    (tag(0x0032, 0x1033), Act::Remove, CLINIC),
    (tag(0x0032, 0x4000), Act::Remove, NOTES),
    (tag(0x0038, 0x0010), Act::Remove, NUMBERS),
    (tag(0x0038, 0x0300), Act::Remove, CLINIC),
    (tag(0x0038, 0x0400), Act::Remove, CLINIC),
    (tag(0x0038, 0x0500), Act::Remove, NOTES),
    (tag(0x0040, 0x0006), Act::Remove, DOCTORS),
    (tag(0x0040, 0x0241), Act::Remove, CLINIC),
    (tag(0x0040, 0x0242), Act::Remove, CLINIC),
    (tag(0x0040, 0x0243), Act::Remove, CLINIC),
    (tag(0x0040, 0x0275), Act::Remove, NUMBERS),
    (tag(0x0040, 0x0280), Act::Remove, NOTES),
    (tag(0x0040, 0x1001), Act::Remove, NUMBERS),
    (tag(0x0040, 0x2016), Act::Remove, NUMBERS),
    (tag(0x0040, 0x2017), Act::Remove, NUMBERS),
    (tag(0x0040, 0xA075), Act::Remove, DOCTORS),
    (tag(0x0040, 0xA123), Act::Remove, DOCTORS),
    (tag(0x0070, 0x0084), Act::Remove, DOCTORS),
    (tag(0x4008, 0x0114), Act::Remove, DOCTORS),
];

/// I campi che non si toccano anche se sono nomi o date: servono a capire l'immagine.
const KEEP: &[u32] = &[
    tag(0x0010, 0x0040), // sesso
    tag(0x0010, 0x1010), // età
    tag(0x0010, 0x1020),
    tag(0x0010, 0x1030), // peso
    tag(0x0010, 0x2201), // specie
    tag(0x0010, 0x2202),
    tag(0x0010, 0x2203), // castrato/sterilizzata
    tag(0x0010, 0x2292), // razza
    tag(0x0010, 0x2293),
];

/// Gli UID che identificano l'esame: diventano nuovi (sempre gli stessi per lo stesso originale).
const UIDS: &[u32] = &[
    tag(0x0008, 0x0014),
    tag(0x0008, 0x0018),
    tag(0x0008, 0x1155),
    tag(0x0008, 0x1195),
    tag(0x0020, 0x000D),
    tag(0x0020, 0x000E),
    tag(0x0020, 0x0052),
    tag(0x0020, 0x0200),
    tag(0x0040, 0xA124),
    tag(0x0088, 0x0140),
    tag(0x3006, 0x0024),
    tag(0x0004, 0x1511),
];

/// HMAC-SHA256: codici e UID nuovi che nessuno può ricostruire senza la chiave di questo computer.
/// La chiave di questo PC per i codici dei pazienti: creata la prima volta e tenuta nella cartella di
/// Cedro. Con la stessa chiave lo stesso paziente prende sempre lo stesso codice (e le stesse date
/// spostate), così gli studi di uno stesso animale restano collegati; senza la chiave non si risale al nome.
pub fn local_key() -> Vec<u8> {
    let path = crate::engine::config_dir().join("anonimizza.chiave");
    if let Ok(k) = std::fs::read(&path) {
        if k.len() >= 32 {
            return k;
        }
    }
    let key = fresh_key();
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(&path, &key);
    key
}

/// 32 byte imprevedibili senza crate in più: i semi casuali dell'hash della libreria standard, l'ora
/// in nanosecondi e il processo, passati in SHA-256.
fn fresh_key() -> Vec<u8> {
    use sha2::{Digest, Sha256};
    use std::hash::{BuildHasher, Hasher};
    let mut h = Sha256::new();
    for i in 0..8u64 {
        let mut s = std::collections::hash_map::RandomState::new().build_hasher();
        s.write_u64(i);
        h.update(s.finish().to_le_bytes());
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    h.update(now.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    let here = 0u8;
    h.update((&here as *const u8 as usize).to_le_bytes());
    h.finalize().to_vec()
}

pub fn hmac(key: &[u8], msg: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = Sha256::new();
    ipad.update(k.map(|b| b ^ 0x36));
    ipad.update(msg);
    let inner = ipad.finalize();
    let mut opad = Sha256::new();
    opad.update(k.map(|b| b ^ 0x5c));
    opad.update(inner);
    opad.finalize().into()
}

/// Il codice del paziente: «CED-» e otto lettere e cifre (niente lettere che si confondono).
pub fn code_for(key: &[u8], who: &str) -> String {
    const ALPHA: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let h = hmac(key, format!("paziente:{}", who.trim().to_uppercase()).as_bytes());
    let mut v = u64::from_be_bytes(h[..8].try_into().unwrap());
    let mut s = String::from("CED-");
    for _ in 0..8 {
        s.push(ALPHA[(v % 32) as usize] as char);
        v /= 32;
    }
    s
}

/// I giorni di cui spostare indietro le date di questo paziente: da 1 a 10 anni circa, sempre gli stessi.
pub fn shift_for(key: &[u8], who: &str) -> i64 {
    let h = hmac(key, format!("date:{}", who.trim().to_uppercase()).as_bytes());
    365 + (u32::from_be_bytes(h[..4].try_into().unwrap()) % 3287) as i64
}

/// Un UID nuovo nella radice 2.25 (UUID in decimale), lungo al massimo 44 caratteri.
pub fn new_uid(key: &[u8], old: &str) -> String {
    let h = hmac(key, format!("uid:{}", old.trim()).as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    // Versione 4 e variante RFC 4122, come un UUID.
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    format!("2.25.{}", u128::from_be_bytes(b))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
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

fn parse_da(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 8 || !s[..8].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y = s[0..4].parse().ok()?;
    let m = s[4..6].parse().ok()?;
    let d = s[6..8].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

fn format_da(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Sposta indietro di `by` giorni ogni data del valore (anche le liste «a\b» e gli intervalli «a-b»).
pub fn shift_value(v: &str, by: i64, datetime: bool) -> String {
    v.split('\\')
        .map(|one| {
            one.split('-')
                .map(|part| {
                    if part.len() >= 8 {
                        if let Some(d) = parse_da(&part[..8]) {
                            let rest = if datetime { &part[8..] } else { "" };
                            return format!("{}{}", format_da(d - by), rest);
                        }
                    }
                    part.to_string()
                })
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join("\\")
}

/// L'età DICOM («007Y», «005M», «012D») tra due date in giorni.
fn age(birth: i64, at: i64) -> Option<String> {
    let days = at - birth;
    if days < 0 {
        return None;
    }
    let (by, bm, bd) = civil_from_days(birth);
    let (ay, am, ad) = civil_from_days(at);
    let mut months = (ay - by) * 12 + (am - bm);
    if ad < bd {
        months -= 1;
    }
    Some(if months >= 24 {
        format!("{:03}Y", (months / 12).min(999))
    } else if months >= 1 {
        format!("{months:03}M")
    } else {
        format!("{:03}D", days.min(999))
    })
}

fn is_private(t: u32) -> bool {
    (t >> 16) & 1 == 1 && (t >> 16) > 0x0008
}

fn put(list: &mut Vec<Element>, t: u32, vr: &[u8; 2], v: Vec<u8>) {
    match list.iter_mut().find(|e| e.tag == t) {
        Some(e) => e.value = Value::Bytes(v),
        None => {
            let at = list.iter().position(|e| e.tag > t).unwrap_or(list.len());
            list.insert(at, Element { tag: t, vr: *vr, value: Value::Bytes(v) });
        }
    }
}

struct Ctx<'a> {
    key: &'a [u8],
    choices: Choices,
    code: String,
    shift: i64,
    report: Report,
}

fn vr_of(e: &Element) -> [u8; 2] {
    if &e.vr == b"UN" {
        dictionary_vr(e.tag)
    } else {
        e.vr
    }
}

fn clean(list: &mut Vec<Element>, cx: &mut Ctx, top: bool) {
    let mut out = Vec::with_capacity(list.len());
    for mut e in std::mem::take(list) {
        let t = e.tag;
        if is_private(t) {
            cx.report.add("dati privati del produttore");
            continue;
        }
        // I gruppi delle curve e degli overlay possono avere scritte: le curve (5xxx) vanno via.
        if (t >> 16) & 0xFF00 == 0x5000 {
            cx.report.add(NOTES);
            continue;
        }
        if KEEP.contains(&t) {
            out.push(e);
            continue;
        }
        if let Some((_, act, label)) = RULES.iter().find(|(rt, _, _)| *rt == t) {
            match act {
                Act::Remove => {
                    cx.report.add(label);
                    continue;
                }
                Act::Empty => {
                    if !e.bytes().is_empty() || matches!(e.value, Value::Seq(_)) {
                        cx.report.add(label);
                    }
                    e.value = Value::Bytes(Vec::new());
                    out.push(e);
                    continue;
                }
                Act::Code => {
                    let had = !text(e.bytes()).is_empty();
                    if cx.choices.code_names && top {
                        e.value = Value::Bytes(text_bytes(&cx.code));
                    } else {
                        e.value = Value::Bytes(Vec::new());
                    }
                    if had {
                        cx.report.add(label);
                    }
                    out.push(e);
                    continue;
                }
            }
        }
        let vr = vr_of(&e);
        if UIDS.contains(&t) {
            let old = text(e.bytes());
            if !old.is_empty() {
                e.value = Value::Bytes(ui_bytes(&new_uid(cx.key, &old)));
                cx.report.add("identificativi dell'esame (UID)");
            }
            out.push(e);
            continue;
        }
        match &vr {
            b"PN" => {
                // Qualunque altro nome di persona.
                if !text(e.bytes()).is_empty() {
                    cx.report.add(DOCTORS);
                }
                e.value = Value::Bytes(Vec::new());
            }
            b"DA" | b"DT" => {
                let v = text(e.bytes());
                if !v.is_empty() {
                    if cx.choices.shift_dates {
                        e.value = Value::Bytes(text_bytes(&shift_value(&v, cx.shift, &vr == b"DT")));
                        cx.report.add("date (spostate tutte insieme)");
                    } else {
                        e.value = Value::Bytes(Vec::new());
                        cx.report.add("date");
                    }
                }
            }
            _ => {}
        }
        if let Value::Seq(items) = &mut e.value {
            for item in items.iter_mut() {
                clean(item, cx, false);
            }
        }
        out.push(e);
    }
    *list = out;
}

/// Toglie i dati personali dal file. `key` è la chiave locale (vedi `crate::anonimizza::local_key`).
pub fn anonymize(f: &mut DicomFile, key: &[u8], choices: Choices) -> Report {
    let id = f.string(tag(0x0010, 0x0020)).or_else(|| f.string(tag(0x0010, 0x0010))).unwrap_or_default();
    let code = if id.is_empty() { String::new() } else { code_for(key, &id) };
    let shift = shift_for(key, &id);
    // L'età prima di togliere la data di nascita: è un dato utile e non identifica.
    let birth = f.string(tag(0x0010, 0x0030)).and_then(|s| parse_da(&s));
    let when = [tag(0x0008, 0x0020), tag(0x0008, 0x0022), tag(0x0008, 0x0023), tag(0x0008, 0x0021)].iter().find_map(|t| f.string(*t).and_then(|s| parse_da(&s)));
    let needs_age = f.string(tag(0x0010, 0x1010)).is_none();
    let burned = super::pixel::burned_in(f);
    let mut cx = Ctx { key, choices, code: code.clone(), shift, report: Report::default() };
    clean(&mut f.body, &mut cx, true);
    if needs_age {
        if let (Some(b), Some(w)) = (birth, when) {
            if let Some(a) = age(b, w) {
                put(&mut f.body, tag(0x0010, 0x1010), b"AS", text_bytes(&a));
            }
        }
    }
    // Il file dice di essere stato anonimizzato, e come.
    put(&mut f.body, tag(0x0012, 0x0062), b"CS", text_bytes("YES"));
    let method = if choices.shift_dates { "Cedro: profilo base, date spostate, specie e razza tenute" } else { "Cedro: profilo base, date tolte, specie e razza tenute" };
    put(&mut f.body, tag(0x0012, 0x0063), b"LO", text_bytes(method));
    // Nell'intestazione: niente nome di chi ha creato il file.
    f.meta.retain(|e| e.tag != tag(0x0002, 0x0016) && e.tag != tag(0x0002, 0x0100) && e.tag != tag(0x0002, 0x0102));
    let mut report = cx.report;
    if burned {
        report.warnings.push("Questa immagine può avere nome o data scritti dentro i pixel (ecografie, catture dello schermo): controllala prima di condividerla.".into());
    }
    if choices.code_names && !code.is_empty() {
        report.code = Some(code);
    }
    if choices.shift_dates {
        report.shift_days = Some(shift);
    }
    report
}

/// La frase per l'utente: cosa è stato tolto, il codice e gli avvisi.
pub fn sentence(r: &Report) -> String {
    let mut s = if r.done.is_empty() { "Nessun dato personale trovato".to_string() } else { format!("Tolti: {}", r.done.join(", ")) };
    if let Some(c) = &r.code {
        s.push_str(&format!(". Codice del paziente: {c}"));
    }
    s.push_str(". Restano specie, razza, sesso, età e peso.");
    for w in &r.warnings {
        s.push(' ');
        s.push_str(w);
    }
    s
}

#[cfg(test)]
mod prove {
    use super::super::prove::ct;
    use super::super::Syntax;
    use super::*;

    const KEY: &[u8] = b"chiave di prova per Cedro, 32 by";

    #[test]
    fn via_proprietario_clinica_e_dati_privati_resta_la_specie() {
        for s in [Syntax::ImplicitLE, Syntax::ExplicitLE] {
            let mut f = DicomFile::read(&ct(2, 2, &[0; 4], s)).unwrap();
            let r = anonymize(&mut f, KEY, Choices::default());
            let again = DicomFile::read(&f.write()).unwrap();
            let code = code_for(KEY, "MC380260000123456");
            assert_eq!(again.string(tag(0x0010, 0x0010)), Some(code.clone()));
            assert_eq!(again.string(tag(0x0010, 0x0020)), Some(code.clone()));
            assert!(again.get(tag(0x0010, 0x2297)).is_none(), "proprietario");
            assert!(again.get(tag(0x0010, 0x2299)).is_none(), "allevamento");
            assert!(again.get(tag(0x0008, 0x0080)).is_none(), "clinica");
            assert!(again.get(tag(0x0009, 0x1001)).is_none(), "privati");
            assert_eq!(again.string(tag(0x0008, 0x0090)), None, "medico svuotato");
            assert_eq!(again.string(tag(0x0010, 0x2201)).as_deref(), Some("Canine"));
            assert_eq!(again.string(tag(0x0010, 0x2292)).as_deref(), Some("Labrador Retriever"));
            assert_eq!(again.string(tag(0x0010, 0x0040)).as_deref(), Some("M"));
            // L'età calcolata prima di togliere la nascita: 2017-02-10 → 2024-03-15 = 7 anni.
            assert_eq!(again.string(tag(0x0010, 0x1010)).as_deref(), Some("007Y"));
            assert_eq!(again.string(tag(0x0010, 0x0030)), None);
            // Date spostate dello stesso numero di giorni.
            let shifted = again.string(tag(0x0008, 0x0020)).unwrap();
            assert_eq!(parse_da("20240315").unwrap() - parse_da(&shifted).unwrap(), shift_for(KEY, "MC380260000123456"));
            // UID nuovi, anche dentro le sequenze; intestazione coerente.
            let sop = again.string(tag(0x0008, 0x0018)).unwrap();
            assert!(sop.starts_with("2.25."));
            assert_eq!(super::super::find(&again.meta, tag(0x0002, 0x0003)).map(|e| text(e.bytes())), Some(sop));
            assert!(again.get(tag(0x0008, 0x1120)).is_none());
            assert_eq!(again.string(tag(0x0012, 0x0062)).as_deref(), Some("YES"));
            assert!(r.done.contains(&OWNER) && r.done.contains(&CLINIC));
            assert_eq!(r.code, Some(code));
            // I pixel non si toccano.
            assert_eq!(again.get(super::super::PIXEL_DATA), DicomFile::read(&ct(2, 2, &[0; 4], s)).unwrap().get(super::super::PIXEL_DATA));
        }
    }

    #[test]
    fn stesso_paziente_stesso_codice_e_stesso_spostamento() {
        assert_eq!(code_for(KEY, "abc"), code_for(KEY, " ABC "));
        assert_ne!(code_for(KEY, "abc"), code_for(KEY, "abd"));
        assert_ne!(code_for(KEY, "abc"), code_for(b"altra chiave", "abc"));
        assert_eq!(new_uid(KEY, "1.2.3"), new_uid(KEY, "1.2.3"));
        assert!(new_uid(KEY, "1.2.3").len() <= 64);
        assert_eq!(shift_value("20240315\\20240316", 1, false), "20240314\\20240315");
        assert_eq!(shift_value("20240301-20240310", 1, false), "20240229-20240309");
        assert_eq!(shift_value("20240101120000.5", 1, true), "20231231120000.5");
        assert_eq!(age(parse_da("20240101").unwrap(), parse_da("20240520").unwrap()).as_deref(), Some("004M"));
    }

    #[test]
    fn senza_spostare_le_date_si_tolgono() {
        let mut f = DicomFile::read(&ct(2, 2, &[0; 4], Syntax::ExplicitLE)).unwrap();
        anonymize(&mut f, KEY, Choices { shift_dates: false, code_names: false });
        assert_eq!(f.string(tag(0x0008, 0x0020)), None);
        assert_eq!(f.string(tag(0x0010, 0x0010)), None);
    }
}
