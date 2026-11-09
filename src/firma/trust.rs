//! L'elenco dei certificatori di fiducia: la lista europea (LOTL) rimanda alle liste dei singoli
//! Stati (per l'Italia quella di AgID). Ogni lista dice quali enti possono rilasciare certificati
//! di firma qualificati e marche temporali qualificate, con i loro certificati e lo stato nel tempo.
//! Cedro la scarica una volta al giorno in sottofondo e la tiene in un file suo; le liste si
//! prendono solo dagli indirizzi ufficiali in HTTPS.

use super::cert::Cert;
use base64::Engine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const LOTL_URL: &str = "https://ec.europa.eu/tools/lotl/eu-lotl.xml";
const CA_QC: &str = "http://uri.etsi.org/TrstSvc/Svctype/CA/QC";
const TSA_QTST: &str = "http://uri.etsi.org/TrstSvc/Svctype/TSA/QTST";
const TSA: &str = "http://uri.etsi.org/TrstSvc/Svctype/TSA";
const DAY: i64 = 86400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Rilascia certificati qualificati (CA/QC).
    QualifiedCa,
    /// Marche temporali qualificate.
    QualifiedTsa,
    /// Marche temporali non qualificate.
    Tsa,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    pub country: String,
    pub provider: String,
    pub service: String,
    /// (concesso, da quando): lo stato più recente per primo.
    pub history: Vec<(bool, i64)>,
    pub cert: Cert,
}

impl Entry {
    /// Il servizio era autorizzato a quella data?
    pub fn granted_at(&self, t: i64) -> bool {
        let mut h = self.history.clone();
        h.sort_by_key(|x| x.1);
        let mut state = false;
        for (g, from) in h {
            if from <= t {
                state = g;
            }
        }
        state
    }
}

#[derive(Clone, Debug, Default)]
pub struct Store {
    pub entries: Vec<Entry>,
    /// Quando è stata scaricata (secondi dal 1970), se mai.
    pub updated: Option<i64>,
}

impl Store {
    /// I servizi con questo certificato (o con lo stesso titolare e la stessa chiave).
    pub fn find(&self, c: &Cert) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.cert.der == c.der || (e.cert.subject_raw == c.subject_raw && e.cert.key == c.key)).collect()
    }

    /// I certificati di chi può aver firmato `c` (stesso nome dell'emittente).
    pub fn issuers_of(&self, c: &Cert) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| e.cert.subject_raw == c.issuer_raw || super::cms::same_name(&e.cert.subject_raw, &c.issuer_raw))
            .filter(|e| match (&c.aki, &e.cert.ski) {
                (Some(a), Some(s)) => a == s,
                _ => true,
            })
            .collect()
    }

    pub fn country_name(code: &str) -> &str {
        match code {
            "IT" => "Italia",
            "DE" => "Germania",
            "FR" => "Francia",
            "ES" => "Spagna",
            "AT" => "Austria",
            "BE" => "Belgio",
            "NL" => "Paesi Bassi",
            "PT" => "Portogallo",
            "PL" => "Polonia",
            "SI" => "Slovenia",
            "HR" => "Croazia",
            "CH" => "Svizzera",
            "EU" => "Unione europea",
            other => other,
        }
    }
}

fn file() -> PathBuf {
    crate::engine::config_dir().join("firma").join("fiducia.txt")
}

static STORE: Mutex<Option<Arc<Store>>> = Mutex::new(None);
static UPDATING: Mutex<bool> = Mutex::new(false);

/// L'elenco in uso (letto dal disco la prima volta).
pub fn store() -> Arc<Store> {
    let mut g = STORE.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(|| Arc::new(load().unwrap_or_default())).clone()
}

/// Per le prove, e dopo averlo scaricato: usa questo elenco.
pub fn set_store(s: Store) {
    *STORE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(s));
}

/// Libera la memoria dell'elenco (si rilegge dal disco alla prossima verifica): per Cedro che resta
/// aperto vicino all'orologio quando si chiudono le finestre.
pub fn forget() {
    *STORE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Quando è stato scaricato l'elenco e quanti certificatori ha, senza tenerlo in memoria.
pub fn summary() -> (Option<i64>, usize) {
    if let Some(s) = STORE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return (s.updated, s.entries.len());
    }
    let n = std::fs::read_to_string(file()).map(|t| t.lines().count().saturating_sub(1)).unwrap_or(0);
    (saved_at(), n)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Se l'elenco ha più di un giorno (o non c'è), lo riscarica in sottofondo. `done` viene chiamato alla fine.
pub fn refresh_if_old(done: impl FnOnce(bool) + Send + 'static) {
    let s = store();
    if s.updated.is_some_and(|t| now() - t < DAY) {
        return;
    }
    {
        let mut u = UPDATING.lock().unwrap_or_else(|e| e.into_inner());
        if *u {
            return;
        }
        *u = true;
    }
    std::thread::spawn(move || {
        let r = download();
        let ok = match r {
            Ok(st) => {
                let _ = save(&st);
                set_store(st);
                true
            }
            Err(e) => {
                crate::append_log(&format!("firma: elenco dei certificatori non scaricato: {e}"));
                false
            }
        };
        *UPDATING.lock().unwrap_or_else(|e| e.into_inner()) = false;
        done(ok);
    });
}

/// Quando è stato scaricato l'elenco salvato, letto dalla prima riga del file (senza caricarlo).
fn saved_at() -> Option<i64> {
    use std::io::BufRead;
    let f = std::fs::File::open(file()).ok()?;
    let mut head = String::new();
    std::io::BufReader::new(f).read_line(&mut head).ok()?;
    head.trim_end().strip_prefix("cedro-fiducia 1\t")?.parse::<i64>().ok().filter(|&t| t > 0)
}

/// Per Cedro che resta aperto vicino all'orologio: ogni giorno riscarica l'elenco europeo dei
/// certificatori e lo salva, senza tenerlo in memoria (la verifica lo legge quando serve).
pub fn daily() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(150));
        loop {
            if saved_at().is_none_or(|t| now() - t >= DAY) {
                match download() {
                    Ok(st) => {
                        let _ = save(&st);
                    }
                    Err(e) => crate::append_log(&format!("firma: elenco dei certificatori non scaricato: {e}")),
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(3 * 3600));
        }
    });
}

pub fn updating() -> bool {
    *UPDATING.lock().unwrap_or_else(|e| e.into_inner())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(&format!("cedro/{}", crate::update::CURRENT))
        .build()
}

fn get(url: &str) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err(format!("indirizzo non sicuro: {url}"));
    }
    let r = agent().get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut s = String::new();
    use std::io::Read;
    r.into_reader().take(64 << 20).read_to_string(&mut s).map_err(|e| format!("{url}: {e}"))?;
    Ok(s)
}

/// Scarica la lista europea e quelle degli Stati.
pub fn download() -> Result<Store, String> {
    let lotl = get(LOTL_URL)?;
    let pointers = lotl_pointers(&lotl)?;
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = pointers.iter().map(|(country, url)| s.spawn(move || (country.clone(), get(url)))).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let mut entries = Vec::new();
    let mut failed = Vec::new();
    for (country, body) in results {
        match body.and_then(|b| parse_tl(&b, &country)) {
            Ok(mut e) => entries.append(&mut e),
            Err(e) => failed.push(format!("{country}: {e}")),
        }
    }
    if entries.is_empty() {
        return Err(format!("nessuna lista letta ({})", failed.join("; ")));
    }
    if !failed.is_empty() {
        crate::append_log(&format!("firma: liste non lette: {}", failed.join("; ")));
    }
    Ok(Store { entries, updated: Some(now()) })
}

fn local<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text(n: Option<roxmltree::Node>) -> String {
    n.and_then(|x| x.text()).unwrap_or("").trim().to_string()
}

/// Gli indirizzi delle liste degli Stati (solo XML).
pub fn lotl_pointers(xml: &str) -> Result<Vec<(String, String)>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("lista europea illeggibile: {e}"))?;
    let mut out = Vec::new();
    for p in doc.descendants().filter(|n| n.tag_name().name() == "OtherTSLPointer") {
        let loc = text(local(p, "TSLLocation"));
        let xml_mime = p.descendants().any(|n| n.tag_name().name() == "MimeType" && n.text().is_some_and(|t| t.contains("xml")));
        let country = p.descendants().find(|n| n.tag_name().name() == "SchemeTerritory").and_then(|n| n.text()).unwrap_or("").trim().to_string();
        if xml_mime && loc.starts_with("https://") && country != "EU" {
            out.push((country, loc));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn status_granted(s: &str) -> bool {
    // Gli stati di prima del 2016 («accredited», «undersupervision»…) valgono come concessi.
    let s = s.rsplit('/').next().unwrap_or(s);
    matches!(s, "granted" | "accredited" | "undersupervision" | "supervisionincessation" | "recognisedatnationallevel")
}

fn time_of(s: &str) -> i64 {
    super::asn1::parse_time(&s.replace(['-', ':', 'T'], ""), false).unwrap_or(0)
}

/// I servizi di una lista nazionale che interessano a Cedro.
pub fn parse_tl(xml: &str, country: &str) -> Result<Vec<Entry>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("lista illeggibile: {e}"))?;
    let mut out = Vec::new();
    for tsp in doc.descendants().filter(|n| n.tag_name().name() == "TrustServiceProvider") {
        let provider = tsp
            .descendants()
            .find(|n| n.tag_name().name() == "TSPName")
            .and_then(|n| n.children().find(|c| c.tag_name().name() == "Name"))
            .and_then(|n| n.text())
            .unwrap_or("")
            .trim()
            .to_string();
        for svc in tsp.descendants().filter(|n| n.tag_name().name() == "TSPService") {
            let Some(info) = local(svc, "ServiceInformation") else { continue };
            let ty = text(local(info, "ServiceTypeIdentifier"));
            let kind = match ty.as_str() {
                CA_QC => Kind::QualifiedCa,
                TSA_QTST => Kind::QualifiedTsa,
                TSA => Kind::Tsa,
                _ => continue,
            };
            let service = info
                .descendants()
                .find(|n| n.tag_name().name() == "ServiceName")
                .and_then(|n| n.children().find(|c| c.tag_name().name() == "Name"))
                .and_then(|n| n.text())
                .unwrap_or("")
                .trim()
                .to_string();
            let mut history = vec![(status_granted(&text(local(info, "ServiceStatus"))), time_of(&text(local(info, "StatusStartingTime"))))];
            if let Some(h) = local(svc, "ServiceHistory") {
                for inst in h.children().filter(|c| c.tag_name().name() == "ServiceHistoryInstance") {
                    history.push((status_granted(&text(local(inst, "ServiceStatus"))), time_of(&text(local(inst, "StatusStartingTime")))));
                }
            }
            for x in info.descendants().filter(|n| n.tag_name().name() == "X509Certificate") {
                let b64: String = x.text().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
                let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64) else { continue };
                let Ok(cert) = Cert::parse(&der) else { continue };
                out.push(Entry { kind, country: country.to_string(), provider: provider.clone(), service: service.clone(), history: history.clone(), cert });
            }
        }
    }
    Ok(out)
}

fn kind_code(k: Kind) -> &'static str {
    match k {
        Kind::QualifiedCa => "CA",
        Kind::QualifiedTsa => "QTSA",
        Kind::Tsa => "TSA",
    }
}

pub fn save(s: &Store) -> std::io::Result<()> {
    let p = file();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = format!("cedro-fiducia 1\t{}\n", s.updated.unwrap_or(0));
    for e in &s.entries {
        let hist: Vec<String> = e.history.iter().map(|(g, t)| format!("{}{}", if *g { 'g' } else { 'w' }, t)).collect();
        let clean = |x: &str| x.replace(['\t', '\n'], " ");
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            kind_code(e.kind),
            e.country,
            clean(&e.provider),
            clean(&e.service),
            hist.join(","),
            base64::engine::general_purpose::STANDARD.encode(&e.cert.der)
        ));
    }
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, out)?;
    std::fs::rename(tmp, p)
}

pub fn load() -> Option<Store> {
    let s = std::fs::read_to_string(file()).ok()?;
    parse_saved(&s)
}

pub fn parse_saved(s: &str) -> Option<Store> {
    let mut lines = s.lines();
    let head = lines.next()?;
    let updated = head.strip_prefix("cedro-fiducia 1\t")?.parse::<i64>().ok().filter(|&t| t > 0);
    let mut entries = Vec::new();
    for l in lines {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != 6 {
            continue;
        }
        let kind = match f[0] {
            "CA" => Kind::QualifiedCa,
            "QTSA" => Kind::QualifiedTsa,
            "TSA" => Kind::Tsa,
            _ => continue,
        };
        let history = f[4]
            .split(',')
            .filter_map(|h| {
                let (g, t) = h.split_at(1);
                Some((g == "g", t.parse().ok()?))
            })
            .collect();
        let Ok(der) = base64::engine::general_purpose::STANDARD.decode(f[5]) else { continue };
        let Ok(cert) = Cert::parse(&der) else { continue };
        entries.push(Entry { kind, country: f[1].into(), provider: f[2].into(), service: f[3].into(), history, cert });
    }
    Some(Store { entries, updated })
}
