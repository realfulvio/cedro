//! La conversione di un file di strumento come la fa la riga di comando: i casi che non vanno bene
//! devono dire chiaro che cosa non va.

use std::path::{Path, PathBuf};

fn fixture(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/strumenti").join(nome)
}

fn cartella(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cedro-test-strumenti-cli-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn un_ruolo_su_un_pozzetto_che_non_c_e_viene_detto() {
    cedro::i18n::fix();
    cedro::i18n::set(cedro::i18n::Lang::It);
    let d = cartella("ruolo");
    let f = d.join("cfx-maestro-esempio.csv");
    std::fs::copy(fixture("cfx-maestro-esempio.csv"), &f).unwrap();
    let mut s = cedro::strumenti::Scelte::default();
    s.ruoli.push(("Z99".into(), 'B'));
    s.ruoli.push(("A1".into(), 'S'));
    let opts = cedro::Options { strumento: s, ..cedro::Options::default() };
    let out = cedro::perform_with(&f, "strumento", cedro::read_quality(), None, &opts).expect("la conversione riesce");
    let e = cedro::esito_strumento(&out.path).expect("l'esito del file appena scritto");
    let nota = e.note.iter().find(|n| n.titolo == "Ruolo senza pozzetto.").expect("la nota sul ruolo ignorato");
    assert!(nota.testo.contains("Z99") && !nota.testo.contains("A1"), "la nota nomina solo il pozzetto che non c'è: {}", nota.testo);
    assert!(nota.attenzione);
    // Senza ruoli sbagliati la nota non c'è.
    let opts = cedro::Options { strumento: cedro::strumenti::Scelte::default(), ..cedro::Options::default() };
    let out = cedro::perform_with(&f, "strumento", cedro::read_quality(), None, &opts).unwrap();
    let e = cedro::esito_strumento(&out.path).unwrap();
    assert!(e.note.iter().all(|n| n.titolo != "Ruolo senza pozzetto."));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn una_cartella_al_posto_del_file_e_un_errore_chiaro() {
    cedro::i18n::fix();
    cedro::i18n::set(cedro::i18n::Lang::It);
    let d = cartella("cartella");
    let falsa = d.join("una cartella.csv");
    std::fs::create_dir_all(&falsa).unwrap();
    let t = match cedro::perform_with(&falsa, "strumento", cedro::read_quality(), None, &cedro::Options::default()) {
        Ok(_) => panic!("una cartella non è un file"),
        Err(e) => e.to_string(),
    };
    assert!(t.contains("cartella"), "il messaggio dice che è una cartella: {t}");
    assert!(!t.to_lowercase().contains("os error"), "niente messaggi del sistema operativo: {t}");
    let _ = std::fs::remove_dir_all(&d);
}
