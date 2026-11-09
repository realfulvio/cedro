//! La matematica: impronte (SHA-1/2) e controllo delle firme RSA e ECDSA con le librerie RustCrypto.

use super::asn1::{self, R};
use super::oid;
use sha2::Digest;

/// L'impronta di `data` con l'algoritmo indicato.
pub fn digest(alg: &str, data: &[u8]) -> R<Vec<u8>> {
    Ok(match alg {
        oid::SHA1 | oid::RSA_SHA1 | oid::ECDSA_SHA1 => sha1::Sha1::digest(data).to_vec(),
        oid::SHA224 | oid::RSA_SHA224 => sha2::Sha224::digest(data).to_vec(),
        oid::SHA256 | oid::RSA_SHA256 | oid::ECDSA_SHA256 => sha2::Sha256::digest(data).to_vec(),
        oid::SHA384 | oid::RSA_SHA384 | oid::ECDSA_SHA384 => sha2::Sha384::digest(data).to_vec(),
        oid::SHA512 | oid::RSA_SHA512 | oid::ECDSA_SHA512 => sha2::Sha512::digest(data).to_vec(),
        other => return Err(format!("algoritmo di impronta sconosciuto ({other})")),
    })
}

/// Le impronte calcolate a pezzi (i PDF firmati: due intervalli del file).
pub fn digest_parts(alg: &str, parts: &[&[u8]]) -> R<Vec<u8>> {
    fn go<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
        let mut h = D::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().to_vec()
    }
    Ok(match alg {
        oid::SHA1 => go::<sha1::Sha1>(parts),
        oid::SHA224 => go::<sha2::Sha224>(parts),
        oid::SHA256 => go::<sha2::Sha256>(parts),
        oid::SHA384 => go::<sha2::Sha384>(parts),
        oid::SHA512 => go::<sha2::Sha512>(parts),
        other => return Err(format!("algoritmo di impronta sconosciuto ({other})")),
    })
}

/// L'algoritmo di impronta che va con un algoritmo di firma (se lo dice da solo).
pub fn hash_of_sig(sig_alg: &str) -> Option<&'static str> {
    Some(match sig_alg {
        oid::RSA_SHA1 | oid::ECDSA_SHA1 => oid::SHA1,
        oid::RSA_SHA224 => oid::SHA224,
        oid::RSA_SHA256 | oid::ECDSA_SHA256 => oid::SHA256,
        oid::RSA_SHA384 | oid::ECDSA_SHA384 => oid::SHA384,
        oid::RSA_SHA512 | oid::ECDSA_SHA512 => oid::SHA512,
        _ => return None,
    })
}

pub fn hash_name(alg: &str) -> &'static str {
    match alg {
        oid::SHA1 => "SHA-1",
        oid::SHA224 => "SHA-224",
        oid::SHA256 => "SHA-256",
        oid::SHA384 => "SHA-384",
        oid::SHA512 => "SHA-512",
        _ => "?",
    }
}

/// Descrizione breve della chiave: «RSA 2048», «ECDSA P-256».
pub fn key_name(key_alg: &str, key_params: &[u8], key: &[u8]) -> String {
    match key_alg {
        oid::RSA => {
            use rsa::pkcs1::DecodeRsaPublicKey;
            use rsa::traits::PublicKeyParts;
            match rsa::RsaPublicKey::from_pkcs1_der(key) {
                Ok(k) => format!("RSA {}", k.n().bits()),
                Err(_) => "RSA".into(),
            }
        }
        oid::EC_PUBLIC_KEY => {
            let curve = asn1::parse(key_params).ok().and_then(|n| n.oid().ok()).unwrap_or_default();
            match curve.as_str() {
                oid::P256 => "ECDSA P-256".into(),
                oid::P384 => "ECDSA P-384".into(),
                _ => "ECDSA".into(),
            }
        }
        _ => "chiave sconosciuta".into(),
    }
}

/// Controlla una firma. `hash_alg` è l'impronta da usare quando l'algoritmo di firma non la dice
/// (nei CMS si trova scritto «rsaEncryption» con l'impronta a parte).
pub fn verify(sig_alg: &str, sig_params: &[u8], hash_alg: Option<&str>, key_alg: &str, key_params: &[u8], key: &[u8], data: &[u8], sig: &[u8]) -> R<bool> {
    if sig_alg == oid::RSA_PSS {
        return verify_pss(sig_params, key, data, sig);
    }
    let h = hash_of_sig(sig_alg).or(hash_alg).ok_or_else(|| format!("algoritmo di firma sconosciuto ({sig_alg})"))?;
    let hashed = digest(h, data)?;
    verify_hashed(sig_alg, h, key_alg, key_params, key, &hashed, sig)
}

/// Come `verify`, con l'impronta già calcolata.
pub fn verify_hashed(sig_alg: &str, hash_alg: &str, key_alg: &str, key_params: &[u8], key: &[u8], hashed: &[u8], sig: &[u8]) -> R<bool> {
    match key_alg {
        oid::RSA => {
            use rsa::pkcs1::DecodeRsaPublicKey;
            if !matches!(sig_alg, oid::RSA | oid::RSA_SHA1 | oid::RSA_SHA224 | oid::RSA_SHA256 | oid::RSA_SHA384 | oid::RSA_SHA512) {
                return Err(format!("firma RSA con un algoritmo inatteso ({sig_alg})"));
            }
            let k = rsa::RsaPublicKey::from_pkcs1_der(key).map_err(|e| format!("chiave RSA illeggibile: {e}"))?;
            // DigestInfo con i parametri NULL (il caso normale) e senza (qualche programma vecchio).
            let alg_null = asn1::seq(&[&asn1::oid_der(hash_alg), &[0x05, 0x00]]);
            let alg_bare = asn1::seq(&[&asn1::oid_der(hash_alg)]);
            for a in [alg_null, alg_bare] {
                let info = asn1::seq(&[&a, &asn1::tlv(0x04, hashed)]);
                if k.verify(rsa::Pkcs1v15Sign::new_unprefixed(), &info, sig).is_ok() {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        oid::EC_PUBLIC_KEY => {
            use p256::ecdsa::signature::hazmat::PrehashVerifier;
            let curve = asn1::parse(key_params).and_then(|n| n.oid()).unwrap_or_default();
            match curve.as_str() {
                oid::P256 => {
                    let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| "chiave P-256 illeggibile".to_string())?;
                    let s = match p256::ecdsa::Signature::from_der(sig) {
                        Ok(s) => s,
                        Err(_) => p256::ecdsa::Signature::from_slice(sig).map_err(|_| "firma ECDSA illeggibile".to_string())?,
                    };
                    Ok(vk.verify_prehash(hashed, &s).is_ok())
                }
                oid::P384 => {
                    let vk = p384::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| "chiave P-384 illeggibile".to_string())?;
                    let s = match p384::ecdsa::Signature::from_der(sig) {
                        Ok(s) => s,
                        Err(_) => p384::ecdsa::Signature::from_slice(sig).map_err(|_| "firma ECDSA illeggibile".to_string())?,
                    };
                    Ok(vk.verify_prehash(hashed, &s).is_ok())
                }
                other => Err(format!("curva ellittica non supportata ({other})")),
            }
        }
        other => Err(format!("tipo di chiave non supportato ({other})")),
    }
}

fn verify_pss(params: &[u8], key: &[u8], data: &[u8], sig: &[u8]) -> R<bool> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    // RSASSA-PSS-params: [0] hash, [1] maskGen, [2] salt (default SHA-1, 20).
    let mut hash = oid::SHA1.to_string();
    let mut salt = 20usize;
    if let Ok(p) = asn1::parse(params) {
        if p.constructed {
            for k in p.children()? {
                match k.ctx() {
                    Some(0) => {
                        if let Some(a) = k.children()?.first() {
                            hash = super::cert::alg(a)?.0;
                        }
                    }
                    Some(2) => {
                        if let Some(n) = k.children()?.first() {
                            salt = n.int()? as usize;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let k = rsa::RsaPublicKey::from_pkcs1_der(key).map_err(|e| format!("chiave RSA illeggibile: {e}"))?;
    let hashed = digest(&hash, data)?;
    let ok = match hash.as_str() {
        oid::SHA256 => k.verify(rsa::Pss::new_with_salt::<sha2::Sha256>(salt), &hashed, sig).is_ok(),
        oid::SHA384 => k.verify(rsa::Pss::new_with_salt::<sha2::Sha384>(salt), &hashed, sig).is_ok(),
        oid::SHA512 => k.verify(rsa::Pss::new_with_salt::<sha2::Sha512>(salt), &hashed, sig).is_ok(),
        oid::SHA1 => k.verify(rsa::Pss::new_with_salt::<sha1::Sha1>(salt), &hashed, sig).is_ok(),
        _ => return Err("firma RSA-PSS con un'impronta non supportata".into()),
    };
    Ok(ok)
}

/// Il certificato `cert` è stato firmato dalla chiave di `issuer`?
pub fn cert_signed_by(cert: &super::cert::Cert, issuer: &super::cert::Cert) -> bool {
    verify(&cert.sig_alg, &cert.sig_params, None, &issuer.key_alg, &issuer.key_params, &issuer.key, &cert.tbs, &cert.signature).unwrap_or(false)
}
