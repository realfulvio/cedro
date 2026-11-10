//! Le buste CMS (PKCS#7): SignedData dei .p7m e dei PDF firmati, le marche temporali (TSTInfo) e
//! i controlli di ogni firmatario: impronta del documento, firma matematica, certificato indicato.

use super::asn1::{self, Node, R};
use super::cert::{self, Cert};
use super::crypto;
use super::oid;

#[derive(Clone, Debug)]
pub struct SignedData {
    pub content_type: String,
    /// Il contenuto firmato, se è dentro la busta (no per le firme «staccate» .p7s e dei PDF).
    pub content: Option<Vec<u8>>,
    pub certs: Vec<Cert>,
    pub crls: Vec<Vec<u8>>,
    pub signers: Vec<SignerInfo>,
}

#[derive(Clone, Debug)]
pub enum Sid {
    IssuerSerial { issuer: Vec<u8>, serial: Vec<u8> },
    Ski(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct Attr {
    pub oid: String,
    /// Ogni valore, in DER grezzo.
    pub values: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct SignerInfo {
    pub sid: Sid,
    pub digest_alg: String,
    /// Gli attributi firmati rifatti come SET in DER: sono i byte che il firmatario ha firmato.
    pub signed_attrs_der: Option<Vec<u8>>,
    pub signed: Vec<Attr>,
    pub sig_alg: String,
    pub sig_params: Vec<u8>,
    pub signature: Vec<u8>,
    pub unsigned: Vec<Attr>,
}

impl SignerInfo {
    pub fn attr(&self, id: &str) -> Option<&[u8]> {
        self.signed.iter().find(|a| a.oid == id).and_then(|a| a.values.first()).map(|v| v.as_slice())
    }

    pub fn unsigned_attrs(&self, id: &str) -> Vec<&[u8]> {
        self.unsigned.iter().filter(|a| a.oid == id).flat_map(|a| a.values.iter().map(|v| v.as_slice())).collect()
    }

    /// L'ora scritta dal computer di chi firma (attributo signingTime): non è una data certa.
    pub fn signing_time(&self) -> Option<i64> {
        asn1::parse(self.attr(oid::SIGNING_TIME)?).ok()?.time().ok()
    }

    pub fn message_digest(&self) -> Option<Vec<u8>> {
        asn1::parse(self.attr(oid::MESSAGE_DIGEST)?).ok()?.octets().ok()
    }

    /// Il certificato del firmatario, tra quelli della busta (e quelli in più che si passano).
    pub fn find_cert<'c>(&self, certs: &'c [Cert]) -> Option<&'c Cert> {
        certs.iter().find(|c| match &self.sid {
            Sid::IssuerSerial { issuer, serial } => same_int(&c.serial, serial) && (c.issuer_raw == *issuer || same_name(&c.issuer_raw, issuer)),
            Sid::Ski(k) => c.ski.as_deref() == Some(k.as_slice()),
        })
    }
}

fn same_int(a: &[u8], b: &[u8]) -> bool {
    let strip = |x: &[u8]| -> Vec<u8> {
        let mut v = x.to_vec();
        while v.len() > 1 && v[0] == 0 {
            v.remove(0);
        }
        v
    };
    strip(a) == strip(b)
}

/// Due nomi uguali anche se scritti con tipi di stringa diversi.
pub fn same_name(a: &[u8], b: &[u8]) -> bool {
    let p = |x: &[u8]| asn1::parse(x).and_then(|n| cert::parse_name(&n)).map(|n| n.attrs.into_iter().map(|(o, v)| (o, v.to_lowercase())).collect::<Vec<_>>());
    matches!((p(a), p(b)), (Ok(x), Ok(y)) if x == y)
}

/// ContentInfo: il tipo e il contenuto.
pub fn content_info(data: &[u8]) -> R<(String, Node<'_>)> {
    let top = asn1::parse(data)?;
    let k = top.children()?;
    let ty = k.first().ok_or("busta vuota")?.oid()?;
    let inner = k.get(1).filter(|n| n.ctx() == Some(0)).ok_or("busta senza contenuto")?;
    let c = inner.children()?.into_iter().next().ok_or("busta senza contenuto")?;
    Ok((ty, c))
}

pub fn parse_signed_data(n: &Node) -> R<SignedData> {
    let k = n.children()?;
    let mut i = 2; // versione, algoritmi
    let eci = k.get(i).ok_or("busta: manca il contenuto")?.children()?;
    i += 1;
    let content_type = eci.first().ok_or("busta: tipo")?.oid()?;
    let content = match eci.get(1) {
        Some(e) if e.ctx() == Some(0) => match e.children()?.first() {
            Some(o) => Some(o.octets()?),
            None => None,
        },
        _ => None,
    };
    let mut certs = Vec::new();
    let mut crls = Vec::new();
    while let Some(x) = k.get(i) {
        match x.ctx() {
            Some(0) => {
                for c in x.children()? {
                    if c.tag == 0x30 {
                        if let Ok(ce) = Cert::parse(c.raw) {
                            certs.push(ce);
                        }
                    }
                }
            }
            Some(1) => {
                for c in x.children()? {
                    crls.push(c.raw.to_vec());
                }
            }
            _ => break,
        }
        i += 1;
    }
    let mut signers = Vec::new();
    if let Some(set) = k.get(i) {
        for s in set.children()? {
            signers.push(parse_signer(&s)?);
        }
    }
    Ok(SignedData { content_type, content, certs, crls, signers })
}

fn attrs(n: &Node) -> R<Vec<Attr>> {
    let mut out = Vec::new();
    for a in n.children()? {
        let k = a.children()?;
        let id = k.first().ok_or("attributo")?.oid()?;
        let values = match k.get(1) {
            Some(set) => set.children()?.iter().map(|v| v.to_der()).collect::<R<Vec<_>>>()?,
            None => vec![],
        };
        out.push(Attr { oid: id, values });
    }
    Ok(out)
}

pub fn parse_signer(n: &Node) -> R<SignerInfo> {
    let k = n.children()?;
    let mut it = k.iter().skip(1).peekable();
    let sid_n = it.next().ok_or("firmatario")?;
    let sid = if sid_n.ctx() == Some(0) {
        Sid::Ski(sid_n.octets()?)
    } else {
        let s = sid_n.children()?;
        Sid::IssuerSerial { issuer: s.first().ok_or("firmatario")?.raw.to_vec(), serial: s.get(1).ok_or("firmatario")?.body.to_vec() }
    };
    let digest_alg = cert::alg(it.next().ok_or("firmatario")?)?.0;
    let mut signed_attrs_der = None;
    let mut signed = Vec::new();
    if it.peek().is_some_and(|x| x.ctx() == Some(0)) {
        let a = it.next().unwrap();
        // Si firma la codifica DER con il tag SET (0x31) al posto di [0].
        signed_attrs_der = Some(if a.indefinite || !a.constructed { a.to_der_as(0x31)? } else {
            let mut v = a.raw.to_vec();
            v[0] = 0x31;
            v
        });
        signed = attrs(a)?;
    }
    let (sig_alg, sig_params) = cert::alg(it.next().ok_or("firmatario")?)?;
    let signature = it.next().ok_or("firmatario")?.octets()?;
    let mut unsigned = Vec::new();
    if let Some(u) = it.next().filter(|x| x.ctx() == Some(1)) {
        unsigned = attrs(u)?;
    }
    Ok(SignerInfo { sid, digest_alg, signed_attrs_der, signed, sig_alg, sig_params, signature, unsigned })
}

/// Il risultato dei controlli «matematici» di un firmatario.
#[derive(Clone, Debug, Default)]
pub struct SignerCheck {
    pub cert: Option<Cert>,
    /// L'impronta del documento coincide con quella firmata (documento non modificato).
    pub digest_ok: bool,
    /// La firma matematica torna con la chiave del certificato.
    pub signature_ok: bool,
    /// Il certificato indicato negli attributi (ESS) è proprio quello usato: None se non indicato.
    pub ess_ok: Option<bool>,
    pub error: Option<String>,
}

/// Controlla un firmatario: `content_digest` calcola l'impronta del contenuto firmato con un algoritmo.
pub fn check_signer(si: &SignerInfo, certs: &[Cert], content_digest: &dyn Fn(&str) -> R<Vec<u8>>) -> SignerCheck {
    let mut out = SignerCheck::default();
    let Some(c) = si.find_cert(certs) else {
        out.error = Some("il certificato di chi ha firmato non è nella busta".into());
        return out;
    };
    out.cert = Some(c.clone());
    let digest = match content_digest(&si.digest_alg) {
        Ok(d) => d,
        Err(e) => {
            out.error = Some(e);
            return out;
        }
    };
    let (data, ok_digest) = match &si.signed_attrs_der {
        Some(sa) => {
            let md = si.message_digest();
            (sa.clone(), md.as_deref() == Some(digest.as_slice()))
        }
        // Senza attributi si firma direttamente il contenuto: l'impronta si controlla con la firma.
        None => (Vec::new(), true),
    };
    out.digest_ok = ok_digest;
    let sig_ok = if si.signed_attrs_der.is_some() {
        crypto::verify(&si.sig_alg, &si.sig_params, Some(&si.digest_alg), &c.key_alg, &c.key_params, &c.key, &data, &si.signature)
    } else {
        crypto::verify_hashed(&si.sig_alg, crypto::hash_of_sig(&si.sig_alg).unwrap_or(&si.digest_alg), &c.key_alg, &c.key_params, &c.key, &digest, &si.signature)
    };
    match sig_ok {
        Ok(v) => out.signature_ok = v,
        Err(e) => out.error = Some(e),
    }
    if si.signed_attrs_der.is_none() {
        out.digest_ok = out.signature_ok;
    }
    out.ess_ok = ess_matches(si, c);
    out
}

/// signingCertificate(V2): l'impronta del certificato scritta negli attributi firmati.
fn ess_matches(si: &SignerInfo, c: &Cert) -> Option<bool> {
    let (raw, v2) = match si.attr(oid::SIGNING_CERT_V2) {
        Some(r) => (r, true),
        None => (si.attr(oid::SIGNING_CERT)?, false),
    };
    let n = asn1::parse(raw).ok()?;
    let certs = n.children().ok()?.into_iter().next()?.children().ok()?;
    let first = certs.first()?.children().ok()?;
    let (alg, hash) = if v2 {
        match first.first()? {
            a if a.tag == 0x30 => (cert::alg(a).ok()?.0, first.get(1)?.octets().ok()?),
            h => (oid::SHA256.to_string(), h.octets().ok()?),
        }
    } else {
        (oid::SHA1.to_string(), first.first()?.octets().ok()?)
    };
    Some(crypto::digest(&alg, &c.der).ok()? == hash)
}

/// Una marca temporale (TimeStampToken) letta e controllata.
#[derive(Clone, Debug)]
pub struct Timestamp {
    pub gen_time: i64,
    pub hash_alg: String,
    pub imprint: Vec<u8>,
    pub tsa: Option<Cert>,
    pub tsa_name: String,
    /// La firma della marca è a posto (impronta del TSTInfo e firma del TSA).
    pub signature_ok: bool,
    pub serial_hex: String,
    pub der: Vec<u8>,
}

impl Timestamp {
    /// La marca riguarda proprio questi byte?
    pub fn covers(&self, data: &[u8]) -> bool {
        crypto::digest(&self.hash_alg, data).map(|d| d == self.imprint).unwrap_or(false)
    }
}

/// Legge una TimeStampToken (ContentInfo con SignedData e TSTInfo).
pub fn parse_timestamp(token: &[u8]) -> R<Timestamp> {
    let (ty, sd_n) = content_info(token)?;
    if ty != oid::SIGNED_DATA {
        return Err("la marca temporale non è una busta firmata".into());
    }
    let sd = parse_signed_data(&sd_n)?;
    if sd.content_type != oid::TST_INFO {
        return Err("la busta non contiene una marca temporale".into());
    }
    let tst = sd.content.clone().ok_or("marca temporale vuota")?;
    let t = asn1::parse(&tst)?;
    let k = t.children()?;
    let mi = k.get(2).ok_or("marca")?.children()?;
    let hash_alg = cert::alg(mi.first().ok_or("marca")?)?.0;
    let imprint = mi.get(1).ok_or("marca")?.octets()?;
    let serial_hex = k.get(3).map(|s| s.body.iter().map(|b| format!("{b:02X}")).collect()).unwrap_or_default();
    let gen_time = k.get(4).ok_or("marca senza data")?.time()?;
    let mut tsa_name = String::new();
    for x in k.iter().skip(5) {
        if x.ctx() == Some(0) {
            // GeneralName: [4] directoryName
            if let Some(dn) = x.children()?.first() {
                if dn.ctx() == Some(4) {
                    if let Some(name) = dn.children()?.first() {
                        tsa_name = cert::parse_name(name)?.common_name().to_string();
                    }
                }
            }
        }
    }
    let mut ok = false;
    let mut tsa = None;
    if let Some(si) = sd.signers.first() {
        let chk = check_signer(si, &sd.certs, &|alg| crypto::digest(alg, &tst));
        ok = chk.digest_ok && chk.signature_ok;
        tsa = chk.cert;
    }
    if tsa_name.is_empty() {
        tsa_name = tsa.as_ref().map(|c| c.subject.common_name().to_string()).unwrap_or_else(|| "servizio sconosciuto".into());
    }
    Ok(Timestamp { gen_time, hash_alg, imprint, tsa, tsa_name, signature_ok: ok, serial_hex, der: token.to_vec() })
}

/// TimeStampResp (.tsr): status + token. Anche una .tsr può contenere direttamente il token.
pub fn timestamp_from_tsr(data: &[u8]) -> R<Timestamp> {
    if let Ok(t) = parse_timestamp(data) {
        return Ok(t);
    }
    let top = asn1::parse(data)?;
    let k = top.children()?;
    let token = k.get(1).ok_or("risposta della marca senza marca")?;
    parse_timestamp(token.raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmatario_rovinato_non_manda_in_panico() {
        // SignerInfo con l'identificativo (issuerAndSerialNumber) vuoto.
        let raw = [0x30, 0x05, 0x02, 0x01, 0x01, 0x30, 0x00];
        assert!(parse_signer(&asn1::parse(&raw).unwrap()).is_err());
    }
}
