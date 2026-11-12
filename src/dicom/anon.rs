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
const DESCR: &str = "descrizioni dell'esame e della serie";
const ICON: &str = "miniatura dell'immagine";
const OVERLAY: &str = "sovrapposizioni (overlay) con possibili scritte";

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
    // Testi liberi, recapiti e riferimenti in cui finiscono spesso nomi, indirizzi e telefoni.
    (tag(0x0008, 0x0116), Act::Remove, CLINIC),
    (tag(0x0008, 0x1030), Act::Remove, DESCR),
    (tag(0x0008, 0x103E), Act::Remove, DESCR),
    (tag(0x0018, 0x1030), Act::Remove, DESCR),
    (tag(0x0008, 0x1111), Act::Remove, NUMBERS),
    (tag(0x0008, 0x4000), Act::Remove, NOTES),
    (tag(0x0010, 0x0050), Act::Remove, ID),
    (tag(0x0010, 0x1050), Act::Remove, ID),
    (tag(0x0010, 0x2155), Act::Remove, CONTACTS),
    (tag(0x0010, 0x21F0), Act::Remove, NOTES),
    (tag(0x0018, 0x4000), Act::Remove, NOTES),
    (tag(0x0020, 0x9158), Act::Remove, NOTES),
    (tag(0x0028, 0x4000), Act::Remove, NOTES),
    (tag(0x0032, 0x1060), Act::Remove, DESCR),
    (tag(0x0038, 0x0011), Act::Remove, NUMBERS),
    (tag(0x0038, 0x0050), Act::Remove, NOTES),
    (tag(0x0038, 0x0060), Act::Remove, NUMBERS),
    (tag(0x0038, 0x0062), Act::Remove, NOTES),
    (tag(0x0038, 0x4000), Act::Remove, NOTES),
    (tag(0x0040, 0x0007), Act::Remove, DESCR),
    (tag(0x0040, 0x0009), Act::Remove, NUMBERS),
    (tag(0x0040, 0x0010), Act::Remove, CLINIC),
    (tag(0x0040, 0x0011), Act::Remove, CLINIC),
    (tag(0x0040, 0x0253), Act::Remove, NUMBERS),
    (tag(0x0040, 0x0254), Act::Remove, DESCR),
    (tag(0x0040, 0x1010), Act::Remove, DOCTORS),
    (tag(0x0040, 0x1102), Act::Remove, CONTACTS),
    (tag(0x0040, 0x1103), Act::Remove, CONTACTS),
    (tag(0x0040, 0x1104), Act::Remove, CONTACTS),
    (tag(0x0040, 0x1400), Act::Remove, NOTES),
    (tag(0x0040, 0x2008), Act::Remove, DOCTORS),
    (tag(0x0040, 0x2010), Act::Remove, CONTACTS),
    (tag(0x0040, 0x2400), Act::Remove, NOTES),
    (tag(0x0040, 0x3001), Act::Remove, NOTES),
    (tag(0x0040, 0xA027), Act::Remove, CLINIC),
    (tag(0x0040, 0xA160), Act::Remove, NOTES),
    (tag(0x0088, 0x0200), Act::Remove, ICON),
    (tag(0x4000, 0x4000), Act::Remove, NOTES),
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
/// Oltre a questi diventa nuovo ogni altro UID, tranne quelli dello standard e quelli di `KEEP_UIDS`.
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

/// Gli UID che dicono il tipo di oggetto o di codifica, non l'esame: restano.
const KEEP_UIDS: &[u32] = &[
    tag(0x0004, 0x1510), // tipo dell'oggetto nel DICOMDIR
    tag(0x0004, 0x1512), // sintassi nel DICOMDIR
    tag(0x0008, 0x0016), // tipo dell'oggetto (SOP Class)
    tag(0x0008, 0x001A),
    tag(0x0008, 0x0062),
    tag(0x0008, 0x010C), // schema dei codici
    tag(0x0008, 0x010D),
    tag(0x0008, 0x1150), // tipo dell'oggetto a cui si rimanda
    tag(0x0040, 0xDB0D),
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
    // Sui byte: un testo con lettere accentate non deve far cadere il programma.
    if s.len() < 8 || !s.as_bytes()[..8].iter().all(u8::is_ascii_digit) {
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
/// Una data che non si sa leggere (formati vecchi come «2024.03.15», testi) non resta vera: si svuota.
pub fn shift_value(v: &str, by: i64, datetime: bool) -> String {
    v.split('\\')
        .map(|one| {
            let mut found = false;
            let parts: Vec<String> = one
                .split('-')
                .map(|part| {
                    if let Some(d) = part.get(..8).and_then(parse_da) {
                        found = true;
                        let rest = if datetime { &part[8..] } else { "" };
                        return format!("{}{}", format_da(d - by), rest);
                    }
                    part.to_string()
                })
                .collect();
            if found {
                parts.join("-")
            } else {
                String::new()
            }
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
        // Gli overlay (60xx, gruppi pari fino a 601E) possono avere nomi disegnati o scritti: vanno via tutti.
        if (t >> 16) & 0xFF00 == 0x6000 && (t >> 16) & 0xFF <= 0x1E {
            cx.report.add(OVERLAY);
            continue;
        }
        // Il vecchio gruppo dei referti (4008): autori, testi e destinatari.
        if t >> 16 == 0x4008 {
            cx.report.add(NOTES);
            continue;
        }
        // Le lunghezze di gruppo (gggg,0000) dopo le modifiche sarebbero sbagliate: non servono e vanno via.
        if t & 0xFFFF == 0 {
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
        if UIDS.contains(&t) || (&vr == b"UI" && !KEEP_UIDS.contains(&t)) {
            let old = text(e.bytes());
            // Gli UID dello standard (tipi, sintassi, codici) sono uguali per tutti: restano.
            if !old.is_empty() && !old.starts_with("1.2.840.10008.") {
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
        report.warnings.push("Questa immagine può avere nome o data scritti dentro i pixel (ecografie, radiografie, catture dello schermo): controllala prima di condividerla.".into());
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

    fn el(t: u32, vr: &[u8; 2], v: &[u8]) -> Element {
        super::super::prove::el(t, vr, v)
    }

    fn sq(t: u32, items: Vec<Vec<Element>>) -> Element {
        Element { tag: t, vr: *b"SQ", value: Value::Seq(items) }
    }

    /// Un referto (SR) con i dati personali nei posti meno ovvi: testi liberi, recapiti, UID degli
    /// eventi, la miniatura, gli overlay e una lunghezza di gruppo dei programmi vecchi.
    fn nascosti(syntax: Syntax) -> Vec<u8> {
        let mut body = vec![
            el(tag(0x0008, 0x0016), b"UI", b"1.2.840.10008.5.1.4.1.1.88.11"),
            el(tag(0x0008, 0x0018), b"UI", b"1.2.3.4.5.6.7"),
            el(tag(0x0008, 0x0020), b"DA", b"20240315"),
            el(tag(0x0008, 0x0060), b"CS", b"DX"),
            el(tag(0x0008, 0x1030), b"LO", b"RX torace Fido Rossi"),
            el(tag(0x0008, 0x3010), b"UI", b"1.2.3.4.5.6.7.8.9.3010"),
            el(tag(0x0010, 0x0010), b"PN", b"Fido^Rossi"),
            el(tag(0x0010, 0x0020), b"LO", b"MC380260000123456"),
            el(tag(0x0010, 0x2155), b"LT", b"mario.rossi@example.it"),
            el(tag(0x0010, 0x2201), b"LO", b"Canine"),
            el(tag(0x0040, 0x1010), b"PN", b"Verdi^Giuseppe"),
            el(tag(0x0040, 0x1103), b"LO", b"333 7654321"),
            el(tag(0x0040, 0xA027), b"LO", b"Clinica Ospedale Verdi"),
            sq(
                tag(0x0040, 0xA730),
                vec![vec![
                    el(tag(0x0008, 0x1150), b"UI", b"1.2.840.10008.5.1.4.1.1.1.1"),
                    el(tag(0x0040, 0xA040), b"CS", b"TEXT"),
                    el(tag(0x0040, 0xA121), b"DA", b"20240316"),
                    el(tag(0x0040, 0xA160), b"UT", b"Il cane di Mario Rossi, via Roma 1"),
                    el(tag(0x0040, 0xA120), b"DT", b"2024.03.16"),
                ]],
            ),
            sq(tag(0x0088, 0x0200), vec![vec![el(tag(0x0028, 0x0010), b"US", &[1, 0]), el(super::super::PIXEL_DATA, b"OB", b"ICONA-CON-NOME-Rossi")]]),
            el(tag(0x6000, 0x0010), b"US", &[1, 0]),
            el(tag(0x6000, 0x0022), b"LO", b"Overlay Rossi"),
            el(tag(0x6000, 0x3000), b"OW", b"OVERLAY-Rossi"),
        ];
        body.sort_by_key(|e| e.tag);
        let at = body.iter().position(|e| e.tag >> 16 == 0x0010).unwrap();
        body.insert(at, Element { tag: tag(0x0010, 0x0000), vr: *b"UL", value: Value::Bytes(0x60u32.to_le_bytes().to_vec()) });
        let meta = vec![el(tag(0x0002, 0x0002), b"UI", b"1.2.840.10008.5.1.4.1.1.88.11")];
        DicomFile { meta, body, syntax, ts: super::super::syntax_uid(syntax).into() }.write()
    }

    #[test]
    fn non_restano_dati_personali_nei_posti_meno_ovvi() {
        // In Implicit VR il VR non è scritto: nomi e date vanno riconosciuti dal dizionario.
        for s in [Syntax::ExplicitLE, Syntax::ImplicitLE, Syntax::ExplicitBE] {
            let mut f = DicomFile::read(&nascosti(s)).unwrap();
            let r = anonymize(&mut f, KEY, Choices::default());
            let out = f.write();
            let testo = String::from_utf8_lossy(&out);
            for nome in ["Rossi", "Verdi", "333 7654321", "mario.rossi", "1.2.3.4.5.6.7.8.9.3010", "20240316", "2024.03.16"] {
                assert!(!testo.contains(nome), "{s:?}: è rimasto «{nome}»");
            }
            let again = DicomFile::read(&out).unwrap();
            // Gli UID dello standard (tipi di oggetto) restano, la specie pure.
            assert_eq!(again.string(tag(0x0008, 0x0016)).as_deref(), Some("1.2.840.10008.5.1.4.1.1.88.11"));
            assert!(testo.contains("1.2.840.10008.5.1.4.1.1.1.1"), "{s:?}: tolto il tipo dell'oggetto citato");
            assert_eq!(again.string(tag(0x0010, 0x2201)).as_deref(), Some("Canine"));
            assert!(again.body.iter().all(|e| e.tag >> 16 != 0x6000), "{s:?}: overlay rimasto");
            assert!(r.done.contains(&OVERLAY) && r.done.contains(&ICON), "{:?}", r.done);
        }
    }

    #[test]
    fn le_lunghezze_di_gruppo_vecchie_vanno_via() {
        let mut f = DicomFile::read(&nascosti(Syntax::ExplicitLE)).unwrap();
        assert!(f.get(tag(0x0010, 0x0000)).is_some());
        anonymize(&mut f, KEY, Choices::default());
        let again = DicomFile::read(&f.write()).unwrap();
        assert!(again.body.iter().all(|e| e.tag & 0xFFFF != 0));
    }

    #[test]
    fn le_date_scritte_male_non_fanno_cadere_il_programma_e_non_restano() {
        // Un byte Latin-1 dentro una data: text() lo fa diventare «é», due byte in UTF-8.
        assert_eq!(shift_value("1234567é", 10, false), "");
        assert_eq!(parse_da("1234567é"), None);
        // Il formato ACR-NEMA «aaaa.mm.gg» non si sa spostare: si svuota.
        assert_eq!(shift_value("2024.03.15", 400, false), "");
        assert_eq!(shift_value("20240315\\2024.03.16", 1, false), "20240314\\");
        // Il fuso orario di un DT resta.
        assert_eq!(shift_value("20240101120000-0500", 1, true), "20231231120000-0500");
        let body = vec![
            el(tag(0x0008, 0x0018), b"UI", b"1.2.3"),
            el(tag(0x0010, 0x0010), b"PN", b"Fido"),
            Element { tag: tag(0x0010, 0x0030), vr: *b"DA", value: Value::Bytes(b"1234567\xE9".to_vec()) },
            Element { tag: tag(0x0008, 0x0020), vr: *b"DA", value: Value::Bytes(b"1234567\xE9".to_vec()) },
        ];
        let mut f = DicomFile { meta: vec![], body, syntax: Syntax::ExplicitLE, ts: super::super::TS_EXPLICIT.into() };
        anonymize(&mut f, KEY, Choices::default());
        assert_eq!(f.string(tag(0x0008, 0x0020)), None);
    }

    #[test]
    fn senza_spostare_le_date_si_tolgono() {
        let mut f = DicomFile::read(&ct(2, 2, &[0; 4], Syntax::ExplicitLE)).unwrap();
        anonymize(&mut f, KEY, Choices { shift_dates: false, code_names: false });
        assert_eq!(f.string(tag(0x0008, 0x0020)), None);
        assert_eq!(f.string(tag(0x0010, 0x0010)), None);
    }
}
