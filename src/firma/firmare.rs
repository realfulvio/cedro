//! Firmare: le buste CAdES (.p7m e .p7s) nuove, la seconda firma accanto a quelle che ci sono
//! (firma parallela), la controfirma e la busta dentro la busta. La chiave privata non si vede mai:
//! chi firma davvero è la chiavetta o la smart card (vedi `pkcs11`), qui si prepara cosa firmare.

use super::asn1::{self, Node, R};
use super::cert::Cert;
use super::oid;
use sha2::Digest;

/// Chi firma: il certificato e la chiave che firma un'impronta SHA-256 già calcolata.
pub trait Key {
    /// Il certificato di firma (DER).
    fn cert(&self) -> &[u8];
    /// Gli altri certificati da mettere nella busta (la catena, se la chiavetta ce l'ha).
    fn chain(&self) -> Vec<Vec<u8>> {
        Vec::new()
    }
    /// Firma l'impronta SHA-256 `digest`: per RSA PKCS#1 v1.5, per ECDSA la firma in DER.
    fn sign_sha256(&self, digest: &[u8]) -> R<Vec<u8>>;
}

/// L'algoritmo di firma da scrivere nella busta, dal tipo di chiave del certificato.
pub fn sig_alg_der(c: &Cert) -> R<Vec<u8>> {
    Ok(match c.key_alg.as_str() {
        oid::RSA => asn1::seq(&[&asn1::oid_der(oid::RSA_SHA256), &[0x05, 0x00]]),
        oid::EC_PUBLIC_KEY => asn1::seq(&[&asn1::oid_der(oid::ECDSA_SHA256)]),
        other => return Err(format!("questo tipo di chiave non lo so usare per firmare ({other})")),
    })
}

fn sha256_alg() -> Vec<u8> {
    asn1::seq(&[&asn1::oid_der(oid::SHA256)])
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    sha2::Sha256::digest(data).to_vec()
}

/// L'ora per l'attributo signingTime: UTCTime fino al 2049 (come vuole il CMS), poi GeneralizedTime.
fn cms_time(t: i64) -> Vec<u8> {
    let (y, m, d) = asn1::civil_from_days(t.div_euclid(86400));
    let r = t.rem_euclid(86400);
    if (1950..2050).contains(&y) {
        asn1::tlv(0x17, format!("{:02}{m:02}{d:02}{:02}{:02}{:02}Z", y % 100, r / 3600, r / 60 % 60, r % 60).as_bytes())
    } else {
        asn1::gen_time(t)
    }
}

fn attr(id: &str, value: &[u8]) -> Vec<u8> {
    asn1::seq(&[&asn1::oid_der(id), &asn1::tlv(0x31, value)])
}

/// Un SET OF in DER: gli elementi in ordine di codifica.
fn der_set(tag: u8, mut items: Vec<Vec<u8>>) -> Vec<u8> {
    items.sort();
    asn1::tlv(tag, &items.concat())
}

/// signingCertificateV2 (ESS): l'impronta SHA-256 del certificato e chi l'ha rilasciato.
fn signing_cert_v2(c: &Cert) -> Vec<u8> {
    let general_names = asn1::seq(&[&asn1::tlv(0xA4, &c.issuer_raw)]);
    let issuer_serial = asn1::seq(&[&general_names, &asn1::tlv(0x02, &c.serial)]);
    // L'algoritmo SHA-256 è quello sottinteso: non si scrive.
    let id = asn1::seq(&[&asn1::tlv(0x04, &sha256(&c.der)), &issuer_serial]);
    asn1::seq(&[&asn1::seq(&[&id])])
}

/// Come si firma (CAdES): con o senza il tipo di contenuto e l'ora (i PDF non la vogliono dentro).
pub struct AttrOpts {
    pub content_type: bool,
    pub signing_time: Option<i64>,
}

/// Gli attributi firmati: SET in DER (sono i byte che firma la chiavetta).
pub fn signed_attrs(c: &Cert, digest: &[u8], o: &AttrOpts) -> Vec<u8> {
    let mut items = Vec::new();
    if o.content_type {
        items.push(attr(oid::CONTENT_TYPE, &asn1::oid_der(oid::DATA)));
    }
    if let Some(t) = o.signing_time {
        items.push(attr(oid::SIGNING_TIME, &cms_time(t)));
    }
    items.push(attr(oid::MESSAGE_DIGEST, &asn1::tlv(0x04, digest)));
    items.push(attr(oid::SIGNING_CERT_V2, &signing_cert_v2(c)));
    der_set(0x31, items)
}

/// Un SignerInfo completo per l'impronta `digest`: la chiavetta firma gli attributi.
pub fn signer_info(key: &dyn Key, digest: &[u8], o: &AttrOpts) -> R<Vec<u8>> {
    let c = Cert::parse(key.cert())?;
    let attrs = signed_attrs(&c, digest, o);
    let sig = key.sign_sha256(&sha256(&attrs))?;
    let sid = asn1::seq(&[&c.issuer_raw, &asn1::tlv(0x02, &c.serial)]);
    let mut signed = attrs.clone();
    signed[0] = 0xA0;
    Ok(asn1::seq(&[&[0x02, 0x01, 0x01], &sid, &sha256_alg(), &signed, &sig_alg_der(&c)?, &asn1::tlv(0x04, &sig)]))
}

/// I certificati da mettere nella busta: quello di firma e la sua catena, senza doppioni.
fn certs_of(key: &dyn Key) -> Vec<Vec<u8>> {
    let mut out = vec![key.cert().to_vec()];
    for c in key.chain() {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

fn content_info(signed_data: &[u8]) -> Vec<u8> {
    asn1::seq(&[&asn1::oid_der(oid::SIGNED_DATA), &asn1::tlv(0xA0, signed_data)])
}

/// Una busta nuova: con il documento dentro (.p7m) o staccata (.p7s, e per i PDF).
pub fn cades(content: &[u8], key: &dyn Key, attached: bool, signing_time: Option<i64>) -> R<Vec<u8>> {
    let si = signer_info(key, &sha256(content), &AttrOpts { content_type: true, signing_time })?;
    Ok(content_info(&signed_data(content, attached, &certs_of(key), &si)))
}

/// La busta dei PDF (PAdES): staccata, senza l'ora (la scrive il PDF) e con l'impronta dei due pezzi.
pub fn pades_cms(digest: &[u8], key: &dyn Key) -> R<Vec<u8>> {
    let si = signer_info(key, digest, &AttrOpts { content_type: true, signing_time: None })?;
    Ok(content_info(&signed_data(&[], false, &certs_of(key), &si)))
}

fn signed_data(content: &[u8], attached: bool, certs: &[Vec<u8>], si: &[u8]) -> Vec<u8> {
    let mut eci = asn1::oid_der(oid::DATA);
    if attached {
        eci.extend(asn1::tlv(0xA0, &asn1::tlv(0x04, content)));
    }
    asn1::seq(&[&[0x02, 0x01, 0x01], &asn1::tlv(0x31, &sha256_alg()), &asn1::tlv(0x30, &eci), &asn1::tlv(0xA0, &certs.concat()), &asn1::tlv(0x31, si)])
}

/// Le parti di una busta che c'è già, per rifarla con una firma in più.
struct Parts {
    version: i64,
    digest_algs: Vec<Vec<u8>>,
    eci: Vec<u8>,
    certs: Vec<Vec<u8>>,
    crls: Option<Vec<u8>>,
    signers: Vec<Vec<u8>>,
    content: Option<Vec<u8>>,
}

fn parts(p7m: &[u8]) -> R<Parts> {
    let (ty, sd) = super::cms::content_info(p7m)?;
    if ty != oid::SIGNED_DATA {
        return Err("non è una busta firmata (CAdES)".into());
    }
    let k = sd.children()?;
    if k.len() < 4 {
        return Err("busta firmata incompleta".into());
    }
    let version = k[0].int().unwrap_or(1);
    let digest_algs = k[1].children()?.iter().map(|a| a.to_der()).collect::<R<Vec<_>>>()?;
    let eci = k[2].to_der()?;
    let content = super::cms::parse_signed_data(&sd)?.content;
    let mut certs = Vec::new();
    let mut crls = None;
    let mut i = 3;
    while let Some(x) = k.get(i) {
        match x.ctx() {
            Some(0) => {
                for c in x.children()? {
                    certs.push(c.to_der()?);
                }
            }
            Some(1) => crls = Some(x.to_der()?),
            _ => break,
        }
        i += 1;
    }
    let set = k.get(i).ok_or("busta firmata senza firmatari")?;
    let signers = set.children()?.iter().map(|s| s.to_der()).collect::<R<Vec<_>>>()?;
    Ok(Parts { version, digest_algs, eci, certs, crls, signers, content })
}

fn rebuild(p: &Parts) -> Vec<u8> {
    let mut body = asn1::tlv(0x02, &[p.version.clamp(1, 5) as u8]);
    body.extend(asn1::tlv(0x31, &p.digest_algs.concat()));
    body.extend(&p.eci);
    body.extend(asn1::tlv(0xA0, &p.certs.concat()));
    if let Some(c) = &p.crls {
        body.extend(c);
    }
    body.extend(asn1::tlv(0x31, &p.signers.concat()));
    content_info(&asn1::tlv(0x30, &body))
}

fn add_certs(p: &mut Parts, key: &dyn Key) {
    for c in certs_of(key) {
        if !p.certs.contains(&c) {
            p.certs.push(c);
        }
    }
}

/// Toglie una busta in base64 (qualche .p7m arriva così) e restituisce i byte DER/BER.
pub fn raw_envelope(data: &[u8]) -> Vec<u8> {
    if data.first() == Some(&0x30) {
        return data.to_vec();
    }
    use base64::Engine;
    let text: String = String::from_utf8_lossy(data).lines().filter(|l| !l.starts_with("-----")).collect::<String>();
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(clean).unwrap_or_else(|_| data.to_vec())
}

/// Firma parallela: la mia firma accanto a quelle che ci sono, sullo stesso documento.
pub fn add_parallel(p7m: &[u8], key: &dyn Key, signing_time: Option<i64>) -> R<Vec<u8>> {
    let data = raw_envelope(p7m);
    let mut p = parts(&data)?;
    let content = p.content.clone().ok_or("la busta non contiene il documento (firma staccata): serve il file originale")?;
    let si = signer_info(key, &sha256(&content), &AttrOpts { content_type: true, signing_time })?;
    let sha = sha256_alg();
    let has_sha = p.digest_algs.iter().any(|a| asn1::parse(a).ok().and_then(|n| n.children().ok()).and_then(|k| k.first().and_then(|o| o.oid().ok())).as_deref() == Some(oid::SHA256));
    if !has_sha {
        p.digest_algs.push(sha);
    }
    add_certs(&mut p, key);
    p.signers.push(si);
    Ok(rebuild(&p))
}

/// Controfirma: la mia firma sopra quelle che ci sono (firma la loro firma, non il documento).
/// `targets`: quali firme controfirmare (indici dei firmatari della busta); vuoto = tutte.
pub fn add_counter(p7m: &[u8], key: &dyn Key, signing_time: Option<i64>, targets: &[usize]) -> R<Vec<u8>> {
    let data = raw_envelope(p7m);
    let mut p = parts(&data)?;
    let n = p.signers.len();
    for i in 0..n {
        if !targets.is_empty() && !targets.contains(&i) {
            continue;
        }
        let raw = p.signers[i].clone();
        let node = asn1::parse(&raw)?;
        let k = node.children()?;
        let sig = k.iter().find(|x| x.tag == 0x04).ok_or("firma da controfirmare illeggibile")?.octets()?;
        let cs = signer_info(key, &sha256(&sig), &AttrOpts { content_type: false, signing_time })?;
        p.signers[i] = with_unsigned(&k, &attr(oid::COUNTER_SIGNATURE, &cs))?;
    }
    add_certs(&mut p, key);
    Ok(rebuild(&p))
}

/// Il SignerInfo con un attributo non firmato in più.
fn with_unsigned(k: &[Node], extra: &[u8]) -> R<Vec<u8>> {
    let mut body = Vec::new();
    let mut unsigned: Vec<Vec<u8>> = Vec::new();
    for x in k {
        if x.ctx() == Some(1) && x.constructed {
            for a in x.children()? {
                unsigned.push(a.to_der()?);
            }
        } else {
            body.extend(x.to_der()?);
        }
    }
    unsigned.push(extra.to_vec());
    body.extend(asn1::tlv(0xA1, &unsigned.concat()));
    Ok(asn1::tlv(0x30, &body))
}

/// Chi ha già firmato un file (busta .p7m o PDF): i nomi dei titolari, senza controllare le firme.
pub fn signer_names(data: &[u8]) -> Vec<String> {
    fn of_cms(env: &[u8], out: &mut Vec<String>) {
        let Ok((ty, sd)) = super::cms::content_info(env) else { return };
        if ty != oid::SIGNED_DATA {
            return;
        }
        let Ok(sd) = super::cms::parse_signed_data(&sd) else { return };
        for si in &sd.signers {
            if let Some(c) = si.find_cert(&sd.certs) {
                out.push(c.subject.person());
            }
            for cs in si.unsigned_attrs(oid::COUNTER_SIGNATURE) {
                if let Some(c) = asn1::parse(cs).ok().and_then(|n| super::cms::parse_signer(&n).ok()).and_then(|x| x.find_cert(&sd.certs).cloned()) {
                    out.push(c.subject.person());
                }
            }
        }
        // Busta dentro la busta.
        if sd.content_type == oid::DATA {
            if let Some(inner) = sd.content.as_deref().filter(|c| c.first() == Some(&0x30)) {
                of_cms(inner, out);
            }
        }
    }
    let mut out = Vec::new();
    if super::pades::is_pdf(data) {
        for s in super::pades::signatures(data) {
            if !s.doc_timestamp {
                of_cms(&s.contents, &mut out);
            }
        }
    } else {
        of_cms(&raw_envelope(data), &mut out);
    }
    out
}

/// La busta contiene il documento (si può firmare accanto o sopra)?
pub fn is_envelope(data: &[u8]) -> bool {
    let raw = raw_envelope(data);
    parts(&raw).is_ok_and(|p| p.content.is_some())
}

/// Una chiave in memoria, per le prove (la chiave vera sta sempre nella chiavetta).
#[cfg(test)]
pub struct SoftKey {
    pub cert: Vec<u8>,
    pub key: p256::ecdsa::SigningKey,
}

#[cfg(test)]
impl Key for SoftKey {
    fn cert(&self) -> &[u8] {
        &self.cert
    }
    fn sign_sha256(&self, digest: &[u8]) -> R<Vec<u8>> {
        use p256::ecdsa::signature::hazmat::PrehashSigner;
        let s: p256::ecdsa::Signature = self.key.sign_prehash(digest).map_err(|e| e.to_string())?;
        Ok(s.to_der().as_bytes().to_vec())
    }
}

/// Un certificato fatto qui, firmato da sé stesso (per le prove e per la chiavetta di prova).
#[cfg(test)]
pub fn self_signed(cn: &str, key: &p256::ecdsa::SigningKey) -> Vec<u8> {
    use p256::ecdsa::signature::Signer;
    let name = asn1::seq(&[
        &asn1::tlv(0x31, &asn1::seq(&[&asn1::oid_der(oid::COUNTRY), &asn1::tlv(0x13, b"IT")])),
        &asn1::tlv(0x31, &asn1::seq(&[&asn1::oid_der(oid::CN), &asn1::tlv(0x0C, cn.as_bytes())])),
    ]);
    let point = key.verifying_key().to_sec1_point(false);
    let spki = asn1::seq(&[&asn1::seq(&[&asn1::oid_der(oid::EC_PUBLIC_KEY), &asn1::oid_der(oid::P256)]), &asn1::tlv(0x03, &[&[0u8][..], point.as_bytes()].concat())]);
    let validity = asn1::seq(&[&cms_time(super::now() - 86400), &cms_time(super::now() + 365 * 86400)]);
    let alg = asn1::seq(&[&asn1::oid_der(oid::ECDSA_SHA256)]);
    // keyUsage: nonRepudiation (bit 1).
    let ku = asn1::seq(&[&asn1::oid_der(oid::EXT_KEY_USAGE), &[0x01, 0x01, 0xFF], &asn1::tlv(0x04, &[0x03, 0x02, 0x06, 0x40])]);
    let exts = asn1::tlv(0xA3, &asn1::seq(&[&ku]));
    let tbs = asn1::seq(&[&[0xA0, 0x03, 0x02, 0x01, 0x02], &[0x02, 0x02, 0x10, 0x01], &alg, &name, &validity, &name, &spki, &exts]);
    let sig: p256::ecdsa::Signature = key.sign(&tbs);
    asn1::seq(&[&tbs, &alg, &asn1::tlv(0x03, &[&[0u8][..], sig.to_der().as_bytes()].concat())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firma::verify::{verify_bytes, Opts};

    pub fn soft(cn: &str, seed: u8) -> SoftKey {
        let key = p256::ecdsa::SigningKey::from_bytes(&[seed; 32].into()).unwrap();
        SoftKey { cert: self_signed(cn, &key), key }
    }

    fn opts() -> Opts {
        Opts { online: false, ..Default::default() }
    }

    fn doc_ok(r: &crate::firma::Report, who: &str) {
        let s = r.signers.iter().find(|s| s.name.contains(who)).unwrap_or_else(|| panic!("manca {who}: {:?}", r.signers.iter().map(|s| &s.name).collect::<Vec<_>>()));
        let c = s.checks.iter().find(|c| c.what == "Documento").unwrap();
        assert_eq!(c.level, crate::firma::Level::Ok, "{who}: {}", c.text);
        let f = s.checks.iter().find(|c| c.what == "Firma").map(|c| c.level);
        assert_ne!(f, Some(crate::firma::Level::Bad), "{who}: {:?}", s.checks.iter().map(|c| format!("{}:{}", c.what, c.text)).collect::<Vec<_>>());
    }

    #[test]
    fn busta_nuova_parallela_controfirma() {
        let mario = soft("MARIO ROSSI", 7);
        let p7m = cades(b"Determina n. 412", &mario, true, Some(super::super::now())).unwrap();
        let r = verify_bytes("determina.txt.p7m", &p7m, &opts());
        assert_eq!(r.doc, b"Determina n. 412");
        assert_eq!(r.signers.len(), 1);
        doc_ok(&r, "MARIO");
        assert!(r.signers[0].when.is_some());

        let giulia = soft("GIULIA BIANCHI", 9);
        let two = add_parallel(&p7m, &giulia, Some(super::super::now())).unwrap();
        let r = verify_bytes("determina.txt.p7m", &two, &opts());
        assert_eq!(r.signers.len(), 2);
        doc_ok(&r, "MARIO");
        doc_ok(&r, "GIULIA");

        let anna = soft("ANNA VERDI", 11);
        let three = add_counter(&two, &anna, Some(super::super::now()), &[0]).unwrap();
        let r = verify_bytes("determina.txt.p7m", &three, &opts());
        assert_eq!(r.signers.len(), 3);
        let a = r.signers.iter().find(|s| s.name.contains("ANNA")).unwrap();
        assert_eq!(a.counter_of, Some(0));
        doc_ok(&r, "ANNA");

        // Busta dentro la busta.
        let nested = cades(&three, &anna, true, None).unwrap();
        let r = verify_bytes("determina.txt.p7m.p7m", &nested, &opts());
        assert_eq!(r.signers.len(), 4);
        assert_eq!(r.doc, b"Determina n. 412");
    }

    #[test]
    fn staccata() {
        let mario = soft("MARIO ROSSI", 7);
        let p7s = cades(b"originale", &mario, false, None).unwrap();
        let r = verify_bytes("x.txt.p7s", &p7s, &Opts { online: false, detached: Some(b"originale".to_vec()), ..Default::default() });
        doc_ok(&r, "MARIO");
    }

    #[test]
    fn parallela_su_busta_vera() {
        // Un .p7m fatto da un altro programma (pyhanko), in BER/DER misto: la sua firma deve restare buona.
        let src = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/firma/mario.txt.p7m")).unwrap();
        let giulia = soft("GIULIA BIANCHI", 9);
        let two = add_parallel(&src, &giulia, None).unwrap();
        let r = verify_bytes("mario.txt.p7m", &two, &opts());
        assert_eq!(r.signers.len(), 2);
        doc_ok(&r, "MARIO");
        doc_ok(&r, "GIULIA");
    }
}
