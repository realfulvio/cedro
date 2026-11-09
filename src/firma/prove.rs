//! Prove con i file firmati di tests/data/firma (PKI finta «Cedro Test», vedi genera.py).

use super::verify::{verify_bytes, At, Level, Opts, Report};
use super::{asn1, trust};

fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/firma")
}

fn read(n: &str) -> Vec<u8> {
    std::fs::read(dir().join(n)).unwrap()
}

fn t(y: i64, m: i64, d: i64) -> i64 {
    asn1::days_from_civil(y, m, d) * 86400 + 12 * 3600
}

fn setup() {
    let xml = String::from_utf8(read("tl.xml")).unwrap();
    let entries = trust::parse_tl(&xml, "IT").unwrap();
    assert_eq!(entries.len(), 2);
    trust::set_store(trust::Store { entries, updated: Some(t(2026, 11, 9)) });
}

fn check(name: &str) -> Report {
    check_with(name, None)
}

fn check_with(name: &str, detached: Option<Vec<u8>>) -> Report {
    setup();
    let opts = Opts { at: At::Now, online: false, detached, now: Some(t(2026, 11, 9)) };
    let r = verify_bytes(name, &read(name), &opts);
    eprintln!("{name}: {:?} {} — {}", r.level, r.headline, r.detail);
    for s in &r.signers {
        eprintln!("  {} {:?} {} | {} | {:?}", s.name, s.level, s.summary, s.why, s.checks.iter().map(|c| format!("{}:{}", c.what, c.text)).collect::<Vec<_>>());
    }
    r
}

fn status(r: &Report, who: &str, what: &str) -> (Level, String) {
    let s = r.signers.iter().find(|s| s.name.contains(who)).unwrap_or_else(|| panic!("manca {who}"));
    let c = s.checks.iter().find(|c| c.what == what).unwrap();
    (c.level, c.text.clone())
}

#[test]
fn p7m_valida() {
    let r = check("mario.txt.p7m");
    assert_eq!(r.level, Level::Ok, "{}", r.detail);
    assert_eq!(r.signers.len(), 1);
    assert_eq!(r.signers[0].name, "MARIO ROSSI");
    assert_eq!(r.headline, "Firma valida");
    assert_eq!(r.doc_name, "mario.txt");
    assert_eq!(r.doc, read("documento.txt"));
    assert_eq!(status(&r, "MARIO", "Certificato").1, "qualificato, in elenco UE");
    assert_eq!(status(&r, "MARIO", "Data certa").0, Level::Warn);
    assert!(r.signers[0].chain.first().is_some_and(|c| c == "Elenco UE (AgID)"));
    assert!(r.signers[0].details.iter().any(|(k, v)| k == "Codice fiscale" && v.contains("RSSMRA")));
}

#[test]
fn p7m_con_marca() {
    let r = check("mario_t.txt.p7m");
    assert_eq!(r.level, Level::Ok);
    assert!(r.signers[0].when_certain);
    assert_eq!(status(&r, "MARIO", "Data certa").0, Level::Ok);
    assert!(r.signers[0].kind.starts_with("CAdES-T"));
}

#[test]
fn busta_dentro_busta() {
    let r = check("doppia.txt.p7m.p7m");
    assert_eq!(r.signers.len(), 2);
    assert_eq!(r.doc_name, "doppia.txt");
    assert_eq!(r.doc, read("documento.txt"));
    // Giulia: certificato scaduto il 31/10, ma con la marca del 3/10.
    assert_eq!(status(&r, "GIULIA", "Certificato"), (Level::Warn, "scaduto dopo la firma".into()));
    assert_eq!(r.signers.iter().find(|s| s.name.contains("GIULIA")).unwrap().summary, "Valida, con un avviso");
    assert_eq!(status(&r, "MARIO", "Documento").0, Level::Ok);
    assert!(r.layers.iter().filter(|l| l.kind == super::verify::LayerKind::Envelope).count() == 2);
}

#[test]
fn revocato_prima() {
    let r = check("paolo.txt.p7m");
    assert_eq!(r.level, Level::Bad);
    let (l, txt) = status(&r, "PAOLO", "Revoca");
    assert_eq!(l, Level::Bad);
    assert_eq!(txt, "revocato il 12/09/2026");
    assert!(r.signers[0].why.contains("revocato"));
}

#[test]
fn contenuto_cambiato() {
    let r = check("cambiato.txt.p7m");
    assert_eq!(r.level, Level::Bad);
    assert_eq!(status(&r, "MARIO", "Documento"), (Level::Bad, "cambiato dopo la firma".into()));
    assert_eq!(r.signers[0].summary, "Non valida: documento cambiato");
}

#[test]
fn firma_staccata() {
    let r = check("documento.txt.p7s");
    assert!(r.needs_original);
    let r = check_with("documento.txt.p7s", Some(read("documento.txt")));
    assert_eq!(r.level, Level::Ok);
    let r = check_with("documento.txt.p7s", Some(b"altro".to_vec()));
    assert_eq!(r.level, Level::Bad);
}

#[test]
fn base64() {
    let r = check("base64.txt.p7m");
    assert_eq!(r.level, Level::Ok);
    assert_eq!(r.doc, read("documento.txt"));
}

#[test]
fn controfirma() {
    let r = check("controfirma.txt.p7m");
    assert_eq!(r.signers.len(), 2);
    let anna = r.signers.iter().position(|s| s.name.contains("ANNA")).unwrap();
    assert_eq!(r.signers[anna].counter_of, Some(0));
    assert_eq!(r.signers[anna].level, Level::Ok, "{}", r.signers[anna].why);
    assert!(r.signers[anna].role.contains("controfirma"));
    assert_eq!(r.ordered(), vec![0, anna]);
}

#[test]
fn tsd_m7m_tsr() {
    for n in ["mario.txt.p7m.tsd", "mario.txt.m7m"] {
        let r = check(n);
        assert_eq!(r.level, Level::Ok, "{n}");
        assert_eq!(r.timestamps.len(), 1, "{n}");
        assert!(r.timestamps[0].qualified, "{n}");
        assert!(r.signers[0].when_certain, "{n}");
        assert_eq!(r.doc, read("documento.txt"), "{n}");
    }
    let r = check_with("mario.txt.p7m.tsr", Some(read("mario.txt.p7m")));
    assert_eq!(r.timestamps.len(), 1);
    assert_eq!(r.level, Level::Ok);
    // La marca staccata porta dentro anche le firme del file marcato.
    assert_eq!(r.signers.len(), 1);
    assert!(r.signers[0].when_certain);
    assert_eq!(r.doc, read("documento.txt"));
    let r = check_with("mario.txt.p7m.tsr", Some(b"altro".to_vec()));
    assert_eq!(r.level, Level::Bad);
}

#[test]
fn pdf_firmato() {
    let r = check("determina_firmata.pdf");
    assert_eq!(r.level, Level::Ok, "{}", r.detail);
    let s = &r.signers[0];
    assert_eq!(s.checks[0].text, "PDF non modificato");
    assert!(s.when_certain);
    let p = s.place.as_ref().unwrap();
    assert_eq!(p.page, Some(1));
    assert!(p.visible);
    assert_eq!(p.field.as_deref(), Some("Firma1"));
    assert!(s.kind.starts_with("PAdES B-T"));
}

#[test]
fn pdf_due_firme() {
    let r = check("determina_due_firme.pdf");
    assert_eq!(r.signers.len(), 2);
    assert!(r.signers.iter().all(|s| s.level == Level::Ok), "{:?}", r.signers.iter().map(|s| &s.why).collect::<Vec<_>>());
    assert_eq!(r.headline, "2 firme, tutte valide");
}

#[test]
fn pdf_cambiato() {
    let r = check("determina_cambiata.pdf");
    assert_eq!(r.level, Level::Bad);
    let s = &r.signers[0];
    assert_eq!(s.summary, "Non valida: documento cambiato");
    assert!(s.details.iter().any(|(k, v)| k == "Cosa è cambiato" && v.contains("pagina 1")));
    let rev = r.signed_revision(0).unwrap();
    assert_eq!(rev, &read("determina_firmata.pdf")[..rev.len()]);
}

#[test]
fn revoca_crl() {
    let c = super::cert::Cert::parse(&read("paolo.cer")).unwrap();
    let ca = super::cert::Cert::parse(&read("ca.cer")).unwrap();
    let r = super::revoca::from_crl(&read("ca.crl"), &c, &ca).unwrap();
    assert!(matches!(r.status, super::revoca::Status::Revoked { .. }));
    let m = super::cert::Cert::parse(&read("mario.cer")).unwrap();
    assert_eq!(super::revoca::from_crl(&read("ca.crl"), &m, &ca).unwrap().status, super::revoca::Status::Good);
}

#[test]
fn non_firmato() {
    setup();
    let r = verify_bytes("x.txt", b"ciao", &Opts { online: false, ..Default::default() });
    assert!(r.error.is_some());
    assert_eq!(r.level, Level::Bad);
}

#[test]
fn rapporto() {
    let r = check("doppia.txt.p7m.p7m");
    let pdf = super::rapporto::pdf(&[&r]).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(super::rapporto::text(&[&r]).contains("GIULIA") || super::rapporto::text(&[&r]).contains("Giulia"));
}
