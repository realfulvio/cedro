//! La revoca dei certificati: OCSP (si chiede al certificatore «questo certificato è ancora
//! buono?») e CRL (l'elenco dei certificati revocati che il certificatore pubblica). Le risposte
//! sono firmate dal certificatore, quindi si possono scaricare anche in HTTP: Cedro controlla la
//! firma prima di crederci. I risultati restano in memoria per la sessione.

use super::asn1::{self, R};
use super::cert::Cert;
use super::crypto;
use super::oid;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Good,
    /// Revocato da quel momento, con il motivo se c'è.
    Revoked { at: i64, reason: Option<String> },
    /// Nessuna risposta utile: il perché.
    Unknown(String),
}

#[derive(Clone, Debug)]
pub struct Revocation {
    pub status: Status,
    /// Da dove: «OCSP», «CRL», «CRL nella busta».
    pub source: String,
    /// Quando è stata prodotta la risposta.
    pub produced: i64,
}

static CACHE: Mutex<Option<HashMap<Vec<u8>, Revocation>>> = Mutex::new(None);

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn key(c: &Cert) -> Vec<u8> {
    let mut k = c.issuer_raw.clone();
    k.extend_from_slice(&c.serial);
    k
}

/// Lo stato di `c` (rilasciato da `issuer`). `crls` sono le CRL già a disposizione (nella busta o
/// nel PDF); `online` permette di chiedere al certificatore.
pub fn check(c: &Cert, issuer: &Cert, crls: &[Vec<u8>], online: bool) -> Revocation {
    let k = key(c);
    if let Some(r) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&k)) {
        return r.clone();
    }
    // Prima quello che c'è già nel file.
    for crl in crls {
        if let Ok(r) = from_crl(crl, c, issuer) {
            return Revocation { source: "CRL nel file".into(), ..r };
        }
    }
    if !online {
        return Revocation { status: Status::Unknown("controllo online spento".into()), source: String::new(), produced: 0 };
    }
    let mut why = Vec::new();
    for url in &c.ocsp_urls {
        match ocsp(url, c, issuer) {
            Ok(r) => return remember(k, r),
            Err(e) => why.push(e),
        }
    }
    for url in &c.crl_urls {
        match fetch(url, None).and_then(|d| from_crl(&d, c, issuer)) {
            Ok(r) => return remember(k, r),
            Err(e) => why.push(e),
        }
    }
    if why.is_empty() {
        why.push("il certificato non dice dove controllare".into());
    }
    Revocation { status: Status::Unknown(why.join("; ")), source: String::new(), produced: 0 }
}

fn remember(k: Vec<u8>, r: Revocation) -> Revocation {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(k, r.clone());
    r
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(6))
        .timeout_read(std::time::Duration::from_secs(20))
        .user_agent(&format!("cedro/{}", crate::update::CURRENT))
        .build()
}

/// Scarica (GET) o invia (POST, per OCSP) e legge al massimo 64 MB.
pub fn fetch(url: &str, post: Option<&[u8]>) -> R<Vec<u8>> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("indirizzo non supportato: {url}"));
    }
    let r = match post {
        Some(body) => agent().post(url).set("Content-Type", "application/ocsp-request").send_bytes(body),
        None => agent().get(url).call(),
    }
    .map_err(|e| format!("{url}: {e}"))?;
    let mut v = Vec::new();
    use std::io::Read;
    r.into_reader().take(64 << 20).read_to_end(&mut v).map_err(|e| format!("{url}: {e}"))?;
    Ok(v)
}

/// La CertID di OCSP (con SHA-1, quella che tutti i certificatori capiscono).
fn cert_id(c: &Cert, issuer: &Cert) -> Vec<u8> {
    let alg = asn1::seq(&[&asn1::oid_der(oid::SHA1), &[0x05, 0x00]]);
    let name_hash = crypto::digest(oid::SHA1, &c.issuer_raw).unwrap_or_default();
    let key_hash = crypto::digest(oid::SHA1, &issuer.key).unwrap_or_default();
    let mut serial = c.serial.clone();
    if serial.first().is_some_and(|b| b & 0x80 != 0) {
        serial.insert(0, 0);
    }
    asn1::seq(&[&alg, &asn1::tlv(0x04, &name_hash), &asn1::tlv(0x04, &key_hash), &asn1::tlv(0x02, &serial)])
}

pub fn ocsp_request(c: &Cert, issuer: &Cert) -> Vec<u8> {
    let req = asn1::seq(&[&cert_id(c, issuer)]);
    let list = asn1::seq(&[&req]);
    let tbs = asn1::seq(&[&list]);
    asn1::seq(&[&tbs])
}

fn ocsp(url: &str, c: &Cert, issuer: &Cert) -> R<Revocation> {
    let body = fetch(url, Some(&ocsp_request(c, issuer)))?;
    parse_ocsp(&body, c, issuer)
}

fn reason_name(code: i64) -> String {
    match code {
        1 => "chiave compromessa",
        2 => "certificatore compromesso",
        3 => "dati cambiati",
        4 => "sostituito",
        5 => "attività cessata",
        6 => "sospeso",
        9 => "privilegi ritirati",
        _ => "motivo non indicato",
    }
    .into()
}

/// Legge una risposta OCSP e controlla che sia firmata dal certificatore (o da chi lui ha incaricato).
pub fn parse_ocsp(data: &[u8], c: &Cert, issuer: &Cert) -> R<Revocation> {
    let top = asn1::parse(data)?;
    let k = top.children()?;
    let status = k.first().ok_or("OCSP: risposta vuota")?;
    if status.body.first().copied().unwrap_or(1) != 0 {
        return Err(format!("OCSP: il servizio ha risposto con errore {}", status.body.first().copied().unwrap_or(0)));
    }
    let rb = k.get(1).filter(|n| n.ctx() == Some(0)).ok_or("OCSP: risposta senza contenuto")?.children()?;
    let rb = rb.first().ok_or("OCSP")?.children()?;
    if rb.first().ok_or("OCSP")?.oid()? != oid::OCSP_BASIC {
        return Err("OCSP: tipo di risposta sconosciuto".into());
    }
    let basic_der = rb.get(1).ok_or("OCSP")?.octets()?;
    let basic = asn1::parse(&basic_der)?;
    let bk = basic.children()?;
    let tbs = bk.first().ok_or("OCSP")?;
    let (sig_alg, sig_params) = super::cert::alg(bk.get(1).ok_or("OCSP")?)?;
    let sig = bk.get(2).ok_or("OCSP")?.bits()?;
    let mut certs = Vec::new();
    if let Some(cs) = bk.get(3).filter(|n| n.ctx() == Some(0)) {
        if let Some(seq) = cs.children()?.first() {
            for x in seq.children()? {
                if let Ok(ce) = Cert::parse(x.raw) {
                    certs.push(ce);
                }
            }
        }
    }
    let tbs_bytes = if tbs.indefinite { tbs.to_der()? } else { tbs.raw.to_vec() };
    // Chi firma: il certificatore stesso o un suo incaricato con l'uso «OCSP signing».
    let signer_ok = |s: &Cert| crypto::verify(&sig_alg, &sig_params, None, &s.key_alg, &s.key_params, &s.key, &tbs_bytes, &sig).unwrap_or(false);
    let trusted = signer_ok(issuer)
        || certs.iter().any(|r| r.ext_key_usage.iter().any(|e| e == oid::EKU_OCSP_SIGNING) && crypto::cert_signed_by(r, issuer) && signer_ok(r));
    if !trusted {
        return Err("OCSP: risposta non firmata dal certificatore".into());
    }
    let mut t = tbs.children()?.into_iter().peekable();
    if t.peek().is_some_and(|n| n.ctx() == Some(0)) {
        t.next();
    }
    t.next(); // responderID
    let produced = t.next().ok_or("OCSP")?.time()?;
    let responses = t.next().ok_or("OCSP")?.children()?;
    let want_serial = |s: &[u8]| {
        let strip = |x: &[u8]| x.iter().skip_while(|&&b| b == 0).copied().collect::<Vec<_>>();
        strip(s) == strip(&c.serial)
    };
    for r in responses {
        let rk = r.children()?;
        let id = rk.first().ok_or("OCSP")?.children()?;
        if !want_serial(id.get(3).ok_or("OCSP")?.body) {
            continue;
        }
        let st = rk.get(1).ok_or("OCSP")?;
        let status = match st.ctx() {
            Some(0) => Status::Good,
            Some(1) => {
                let ri = st.children()?;
                let at = ri.first().ok_or("OCSP")?.time()?;
                let reason = ri.get(1).and_then(|x| x.children().ok()).and_then(|v| v.first().map(|e| e.body.first().copied().unwrap_or(0) as i64)).map(reason_name);
                Status::Revoked { at, reason }
            }
            _ => Status::Unknown("il certificatore non conosce questo certificato".into()),
        };
        return Ok(Revocation { status, source: "OCSP".into(), produced });
    }
    Err("OCSP: la risposta non parla di questo certificato".into())
}

/// Cerca il certificato in una CRL firmata da `issuer`.
pub fn from_crl(data: &[u8], c: &Cert, issuer: &Cert) -> R<Revocation> {
    let top = asn1::parse(data)?;
    let k = top.children()?;
    let tbs = k.first().ok_or("CRL vuota")?;
    let (sig_alg, sig_params) = super::cert::alg(k.get(1).ok_or("CRL")?)?;
    let sig = k.get(2).ok_or("CRL")?.bits()?;
    let mut t = tbs.children()?.into_iter().peekable();
    if t.peek().is_some_and(|n| n.tag == 0x02) {
        t.next();
    }
    t.next(); // algoritmo
    let iss = t.next().ok_or("CRL")?;
    if iss.raw != c.issuer_raw.as_slice() && !super::cms::same_name(iss.raw, &c.issuer_raw) {
        return Err("CRL di un altro certificatore".into());
    }
    let tbs_bytes = if tbs.indefinite { tbs.to_der()? } else { tbs.raw.to_vec() };
    if !crypto::verify(&sig_alg, &sig_params, None, &issuer.key_alg, &issuer.key_params, &issuer.key, &tbs_bytes, &sig).unwrap_or(false) {
        return Err("CRL non firmata dal certificatore".into());
    }
    let this_update = t.next().ok_or("CRL")?.time()?;
    let strip = |x: &[u8]| x.iter().skip_while(|&&b| b == 0).copied().collect::<Vec<_>>();
    let want = strip(&c.serial);
    for n in t {
        if n.tag != 0x30 {
            continue; // nextUpdate (ora) o estensioni [0]
        }
        for e in n.children()? {
            let ek = e.children()?;
            if ek.first().is_some_and(|s| strip(s.body) == want) {
                let at = ek.get(1).ok_or("CRL")?.time()?;
                let mut reason = None;
                if let Some(exts) = ek.get(2) {
                    for x in exts.children()? {
                        let xk = x.children()?;
                        if xk.first().and_then(|o| o.oid().ok()).as_deref() == Some(oid::CRL_REASON) {
                            if let Some(v) = xk.last().and_then(|v| v.octets().ok()) {
                                if let Ok(en) = asn1::parse(&v) {
                                    reason = Some(reason_name(en.body.first().copied().unwrap_or(0) as i64));
                                }
                            }
                        }
                    }
                }
                return Ok(Revocation { status: Status::Revoked { at, reason }, source: "CRL".into(), produced: this_update });
            }
        }
    }
    Ok(Revocation { status: Status::Good, source: "CRL".into(), produced: this_update })
}

/// Scarica il certificato di chi ha rilasciato `c` dall'indirizzo scritto nel certificato
/// (caIssuers), quando nel file non c'è.
pub fn fetch_issuer(c: &Cert) -> Option<Cert> {
    for url in &c.ca_issuer_urls {
        let Ok(data) = fetch(url, None) else { continue };
        if let Ok(ce) = Cert::parse(&data) {
            if crypto::cert_signed_by(c, &ce) {
                return Some(ce);
            }
        }
        // A volte è un .p7c con dentro più certificati.
        if let Ok((_, n)) = super::cms::content_info(&data) {
            if let Ok(sd) = super::cms::parse_signed_data(&n) {
                if let Some(ce) = sd.certs.into_iter().find(|x| crypto::cert_signed_by(c, x)) {
                    return Some(ce);
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
pub fn age_hours(r: &Revocation) -> i64 {
    (now() - r.produced) / 3600
}
