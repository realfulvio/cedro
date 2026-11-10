//! I certificati X.509: chi è il titolare, chi l'ha rilasciato, da quando a quando vale, se è
//! qualificato (QcStatements) e dove si controlla la revoca.

use super::asn1::{self, Node, R};
use super::oid;

#[derive(Clone, Debug)]
pub struct Cert {
    pub der: Vec<u8>,
    /// I byte firmati dal certificatore (tbsCertificate).
    pub tbs: Vec<u8>,
    pub sig_alg: String,
    pub sig_params: Vec<u8>,
    pub signature: Vec<u8>,
    pub serial: Vec<u8>,
    /// Emittente e titolare così come sono scritti (servono per confrontarli e per OCSP).
    pub issuer_raw: Vec<u8>,
    pub subject_raw: Vec<u8>,
    pub issuer: Name,
    pub subject: Name,
    pub not_before: i64,
    pub not_after: i64,
    /// subjectPublicKeyInfo intero e la chiave da sola.
    pub spki: Vec<u8>,
    pub key_alg: String,
    pub key_params: Vec<u8>,
    pub key: Vec<u8>,
    pub ski: Option<Vec<u8>>,
    pub aki: Option<Vec<u8>>,
    pub is_ca: bool,
    pub non_repudiation: bool,
    pub ocsp_urls: Vec<String>,
    pub ca_issuer_urls: Vec<String>,
    pub crl_urls: Vec<String>,
    pub policies: Vec<String>,
    pub qc_compliance: bool,
    pub qc_sscd: bool,
    pub qc_esign: bool,
    pub ext_key_usage: Vec<String>,
    pub ocsp_nocheck: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Name {
    pub attrs: Vec<(String, String)>,
}

impl Name {
    pub fn get(&self, oid: &str) -> Option<&str> {
        self.attrs.iter().find(|(o, _)| o == oid).map(|(_, v)| v.as_str())
    }

    /// Il nome da mostrare: Nome Cognome se ci sono, altrimenti il CN.
    pub fn person(&self) -> String {
        match (self.get(oid::GIVEN_NAME), self.get(oid::SURNAME)) {
            (Some(g), Some(s)) => format!("{} {}", g, s).to_uppercase(),
            _ => self.common_name().to_string(),
        }
    }

    pub fn common_name(&self) -> &str {
        self.get(oid::CN).or(self.get(oid::ORG)).or(self.get(oid::OU)).unwrap_or("(senza nome)")
    }

    /// Una riga leggibile, tipo «CN=…, O=…, C=IT».
    pub fn line(&self) -> String {
        let label = |o: &str| match o {
            oid::CN => "CN",
            oid::ORG => "O",
            oid::OU => "OU",
            oid::COUNTRY => "C",
            oid::SURNAME => "SN",
            oid::GIVEN_NAME => "G",
            oid::SERIAL_NUMBER => "SERIALNUMBER",
            oid::TITLE => "T",
            oid::ORG_ID => "OrgId",
            oid::DN_QUALIFIER => "dnQualifier",
            oid::EMAIL => "E",
            _ => "",
        };
        self.attrs
            .iter()
            .filter(|(o, _)| !label(o).is_empty())
            .map(|(o, v)| format!("{}={}", label(o), v))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn parse_name(n: &Node) -> R<Name> {
    let mut attrs = Vec::new();
    for rdn in n.children()? {
        for atv in rdn.children()? {
            let k = atv.children()?;
            if k.len() >= 2 {
                attrs.push((k[0].oid()?, k[1].string().trim().to_string()));
            }
        }
    }
    Ok(Name { attrs })
}

impl Cert {
    pub fn parse(der: &[u8]) -> R<Cert> {
        let top = asn1::parse(der)?;
        let k = top.children()?;
        if k.len() < 3 {
            return Err("certificato non valido".into());
        }
        let tbs_node = k[0];
        let (sig_alg, sig_params) = alg(&k[1])?;
        let signature = k[2].bits()?;
        let mut t = tbs_node.children()?.into_iter().peekable();
        if t.peek().is_some_and(|n| n.ctx() == Some(0)) {
            t.next();
        }
        let serial = t.next().ok_or("certificato: manca il numero")?.body.to_vec();
        t.next(); // algoritmo, ripetuto
        let issuer_n = t.next().ok_or("certificato: manca l'emittente")?;
        let validity = t.next().ok_or("certificato: manca la validità")?.children()?;
        let subject_n = t.next().ok_or("certificato: manca il titolare")?;
        let spki_n = t.next().ok_or("certificato: manca la chiave")?;
        let sk = spki_n.children()?;
        let (key_alg, key_params) = alg(sk.first().ok_or("certificato: chiave")?)?;
        let key = sk.get(1).ok_or("certificato: chiave")?.bits()?;
        let mut c = Cert {
            der: top.raw.to_vec(),
            tbs: tbs_node.to_der()?,
            sig_alg,
            sig_params,
            signature,
            serial,
            issuer_raw: issuer_n.raw.to_vec(),
            subject_raw: subject_n.raw.to_vec(),
            issuer: parse_name(&issuer_n)?,
            subject: parse_name(&subject_n)?,
            not_before: validity.first().ok_or("validità")?.time()?,
            not_after: validity.get(1).ok_or("validità")?.time()?,
            spki: spki_n.raw.to_vec(),
            key_alg,
            key_params,
            key,
            ski: None,
            aki: None,
            is_ca: false,
            non_repudiation: false,
            ocsp_urls: vec![],
            ca_issuer_urls: vec![],
            crl_urls: vec![],
            policies: vec![],
            qc_compliance: false,
            qc_sscd: false,
            qc_esign: false,
            ext_key_usage: vec![],
            ocsp_nocheck: false,
        };
        // Se l'originale non era DER (succede), si firmano comunque i byte originali.
        if !tbs_node.indefinite {
            c.tbs = tbs_node.raw.to_vec();
        }
        for n in t {
            if n.ctx() == Some(3) {
                for ext in n.children()?.first().map(|s| s.children()).transpose()?.unwrap_or_default() {
                    let e = ext.children()?;
                    let id = e.first().ok_or("estensione")?.oid()?;
                    let val = e.last().ok_or("estensione")?.octets()?;
                    let _ = c.extension(&id, &val);
                }
            }
        }
        Ok(c)
    }

    fn extension(&mut self, id: &str, val: &[u8]) -> R<()> {
        let v = asn1::parse(val)?;
        match id {
            oid::EXT_SKI => self.ski = Some(v.octets()?),
            oid::EXT_AKI => {
                for k in v.children()? {
                    if k.ctx() == Some(0) {
                        self.aki = Some(k.body.to_vec());
                    }
                }
            }
            oid::EXT_BASIC_CONSTRAINTS => {
                if let Some(first) = v.children()?.first() {
                    self.is_ca = first.tag == 0x01 && first.body.first().is_some_and(|&b| b != 0);
                }
            }
            oid::EXT_KEY_USAGE => {
                let b = v.octets()?;
                // bit 1 = nonRepudiation (contentCommitment)
                self.non_repudiation = b.get(1).is_some_and(|x| x & 0x40 != 0);
            }
            oid::EXT_EXT_KEY_USAGE => {
                for k in v.children()? {
                    self.ext_key_usage.push(k.oid()?);
                }
            }
            oid::EXT_AIA => {
                for ad in v.children()? {
                    let k = ad.children()?;
                    let method = k.first().ok_or("estensione")?.oid()?;
                    if k.get(1).is_some_and(|l| l.tag == 0x86) {
                        let url = String::from_utf8_lossy(k[1].body).to_string();
                        if method == oid::AD_OCSP {
                            self.ocsp_urls.push(url);
                        } else if method == oid::AD_CA_ISSUERS {
                            self.ca_issuer_urls.push(url);
                        }
                    }
                }
            }
            oid::EXT_CRL_DP => {
                // DistributionPoint { [0] DistributionPointName { [0] fullName GeneralNames } }
                fn walk(n: &Node, out: &mut Vec<String>) {
                    if n.tag == 0x86 {
                        out.push(String::from_utf8_lossy(n.body).to_string());
                    } else if n.constructed {
                        if let Ok(k) = n.children() {
                            for c in k {
                                walk(&c, out);
                            }
                        }
                    }
                }
                walk(&v, &mut self.crl_urls);
                self.crl_urls.retain(|u| u.starts_with("http"));
            }
            oid::EXT_CERT_POLICIES => {
                for p in v.children()? {
                    if let Some(first) = p.children()?.first() {
                        self.policies.push(first.oid()?);
                    }
                }
            }
            oid::EXT_QC_STATEMENTS => {
                for st in v.children()? {
                    let k = st.children()?;
                    match k.first().ok_or("estensione")?.oid()?.as_str() {
                        oid::QC_COMPLIANCE => self.qc_compliance = true,
                        oid::QC_SSCD => self.qc_sscd = true,
                        oid::QC_TYPE => {
                            if let Some(types) = k.get(1) {
                                for t in types.children()? {
                                    if t.oid()? == oid::QC_TYPE_ESIGN {
                                        self.qc_esign = true;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            oid::EXT_OCSP_NOCHECK => self.ocsp_nocheck = true,
            _ => {}
        }
        Ok(())
    }

    /// Il numero di serie in esadecimale, come lo mostrano i programmi di firma.
    pub fn serial_hex(&self) -> String {
        let s: &[u8] = if self.serial.len() > 1 && self.serial[0] == 0 { &self.serial[1..] } else { &self.serial };
        s.iter().map(|b| format!("{b:02X}")).collect()
    }

    /// Il codice fiscale dal serialNumber («TINIT-RSSMRA…» o «IT:RSSMRA…»), se c'è.
    pub fn tax_code(&self) -> Option<String> {
        let s = self.subject.get(oid::SERIAL_NUMBER)?;
        Some(s.to_string())
    }

    pub fn self_signed(&self) -> bool {
        self.issuer_raw == self.subject_raw
    }

    /// Valido a quella data (secondi dal 1970).
    pub fn valid_at(&self, t: i64) -> bool {
        t >= self.not_before && t <= self.not_after
    }

    pub fn sha256(&self) -> Vec<u8> {
        use sha2::Digest;
        sha2::Sha256::digest(&self.der).to_vec()
    }
}

/// AlgorithmIdentifier: l'identificatore e i parametri (grezzi).
pub fn alg(n: &Node) -> R<(String, Vec<u8>)> {
    let k = n.children()?;
    let id = k.first().ok_or("algoritmo")?.oid()?;
    Ok((id, k.get(1).map(|p| p.raw.to_vec()).unwrap_or_default()))
}
