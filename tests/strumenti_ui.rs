//! Cedro 4 Labs, «Converti dati e piastre» con il file di uno strumento: la finestra senza schermo
//! vero (ogni scheda, ogni scelta, ogni tinta si disegna), la piastra coi ruoli segnati e il giro
//! completo leggere → scegliere → convertire → ritrovare Excel, CSV, Allotrope e il lavoro registrato.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use cedro::ui::core::Act;
use cedro::ui::model::{FileState, Lab};
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::strumenti::{Pronta, StrumAct, Uscita, Vista};
use cedro::ui::theme::ThemeId;

struct Finta;

impl Platform for Finta {
    fn pick_files(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_folder(&mut self, _: &str) -> Option<PathBuf> {
        None
    }
    fn open_path(&mut self, _: &Path) {}
    fn copy_text(&mut self, _: &str) {}
    fn paste_text(&mut self) -> Option<String> {
        None
    }
    fn open_guide(&mut self) {}
    fn open_settings(&mut self) {}
    fn minimize(&mut self) {}
    fn toggle_maximize(&mut self) {}
    fn close(&mut self) {}
    fn open_full(&mut self) {}
}

static UNA_VOLTA: Once = Once::new();

/// Le impostazioni e il registro dei lavori vanno in una cartella di prova, non in quella di chi lancia le prove.
fn ambiente() {
    UNA_VOLTA.call_once(|| {
        let casa = std::env::temp_dir().join(format!("cedro-test-strumenti-casa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&casa);
        std::fs::create_dir_all(&casa).unwrap();
        std::env::set_var("HOME", &casa);
        std::env::remove_var("APPDATA");
        cedro::i18n::fix();
        cedro::i18n::set(cedro::i18n::Lang::It);
    });
}

fn fixture(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/strumenti").join(nome)
}

fn cartella(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cedro-test-strumenti-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn aspetta(s: &mut Screen, secondi: u64, fatto: impl Fn(&Screen) -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secondi) {
        s.pump();
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        if fatto(s) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    fatto(s)
}

fn disegna(s: &mut Screen) {
    let (w, h) = s.preferred_size();
    let _ = s.render(w as u32, h as u32, 1.0);
}

fn apri(s: &mut Screen, plat: &mut Finta, dir: &Path, fixture_nome: &str, come: &str) -> PathBuf {
    let path = dir.join(come);
    std::fs::copy(fixture(fixture_nome), &path).unwrap();
    s.drop_files(vec![path.clone()]);
    assert!(aspetta(s, 30, |s| s.app.files.iter().any(|f| f.path == path)), "il file non è stato preso");
    assert!(aspetta(s, 30, |s| s.app.strum_corrente().is_some()), "{come}: non è stato riconosciuto come file di uno strumento");
    disegna(s);
    let _ = plat;
    path
}

fn schermo() -> Screen {
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    s.request_appearance(ThemeId::CedroLabs, false);
    s
}

#[test]
fn la_piastra_si_legge_si_disegna_e_si_segnano_i_ruoli() {
    ambiente();
    let dir = cartella("piastra");
    let mut plat = Finta;
    let mut s = schermo();
    s.act(Act::Lab(Lab::Dati), &mut plat);
    let path = apri(&mut s, &mut plat, &dir, "Magellan_Pro_7_5_compact_metadata.xlsx", "Piastra ELISA.xlsx");
    let (_, l) = s.app.strum_corrente().unwrap();
    assert!(l.a_piastra(), "il lettore di piastre dà una piastra");
    assert!(l.conta() > 0);

    // Ogni scheda, in «Completa» e in «Semplice».
    for semplice in [false, true] {
        s.act(Act::SetSimple(semplice), &mut plat);
        for v in [Vista::Piastra, Vista::Elenco, Vista::Strumento] {
            s.act(Act::Strum(StrumAct::Vista(v)), &mut plat);
            disegna(&mut s);
        }
    }
    s.act(Act::SetSimple(false), &mut plat);
    assert!(s.app.strum_corrente().is_some());
    s.act(Act::Strum(StrumAct::Vista(Vista::Piastra)), &mut plat);

    // Si sceglie un rettangolo di pozzetti trascinando e si segna «Bianco»; una riga intera diventa «Standard».
    s.app.strum_premi((0, 0));
    s.app.strum_muovi((1, 1));
    s.app.strum_rilascia();
    assert_eq!(s.app.strum_file(&path).sel.len(), 4);
    s.act(Act::Strum(StrumAct::Ruolo('B')), &mut plat);
    s.act(Act::Strum(StrumAct::Colonna(1)), &mut plat);
    s.act(Act::Strum(StrumAct::Ruolo('S')), &mut plat);
    let pf = s.app.strum_file(&path);
    assert!(pf.ruoli.iter().any(|(_, c)| *c == 'B') && pf.ruoli.iter().any(|(_, c)| *c == 'S'), "{:?}", pf.ruoli);
    // Un clic fermo sullo stesso pozzetto lo toglie dalla scelta.
    s.app.strum_premi((1, 1));
    s.app.strum_rilascia();
    assert!(!s.app.strum_file(&path).sel.contains(&(1, 1)));
    s.act(Act::Strum(StrumAct::Nessuno), &mut plat);
    s.act(Act::Strum(StrumAct::Daltonici(true)), &mut plat);
    s.act(Act::Strum(StrumAct::MenoBianco(true)), &mut plat);
    disegna(&mut s);

    // Allotrope acceso: compare la scheda e si disegna.
    s.act(Act::Strum(StrumAct::Esce(Uscita::Allotrope)), &mut plat);
    s.act(Act::Strum(StrumAct::Vista(Vista::Allotrope)), &mut plat);
    disegna(&mut s);
    s.act(Act::Strum(StrumAct::Calcoli), &mut plat);
    s.act(Act::Strum(StrumAct::Percorso(true)), &mut plat);
    disegna(&mut s);
    // Spento Allotrope la scheda sparisce e si torna alla piastra.
    s.act(Act::Strum(StrumAct::Esce(Uscita::Allotrope)), &mut plat);
    assert_eq!(s.app.strum_file(&path).vista, Vista::Piastra);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn convertire_scrive_i_file_e_registra_il_lavoro() {
    ambiente();
    let dir = cartella("giro");
    let mut plat = Finta;
    let mut s = schermo();
    s.act(Act::Lab(Lab::Dati), &mut plat);
    let path = apri(&mut s, &mut plat, &dir, "Magellan_Pro_7_5_compact_metadata.xlsx", "Piastra ELISA.xlsx");
    // «Completa»: Excel, CSV, Allotrope e CSV piatto; un pozzetto segnato come bianco.
    s.act(Act::SetSimple(false), &mut plat);
    s.app.strum_premi((0, 0));
    s.app.strum_rilascia();
    s.act(Act::Strum(StrumAct::Ruolo('B')), &mut plat);
    s.act(Act::Strum(StrumAct::Esce(Uscita::Csv)), &mut plat);
    s.act(Act::Strum(StrumAct::Esce(Uscita::Allotrope)), &mut plat);
    s.act(Act::Strum(StrumAct::Esce(Uscita::Piatto)), &mut plat);
    s.act(Act::Start, &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.files.iter().all(|f| f.state == FileState::Done || f.state == FileState::Failed) && !s.app.busy()), "il lavoro non è finito");
    let f = s.app.files.iter().find(|f| f.path == path).unwrap();
    assert_eq!(f.state, FileState::Done, "{}", f.message);

    // I quattro file stanno accanto all'originale, con nomi diversi dall'originale.
    let mut nomi: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    nomi.sort();
    assert!(nomi.iter().any(|n| n == "Piastra ELISA-pulito.xlsx"), "{nomi:?}");
    assert!(nomi.iter().any(|n| n == "Piastra ELISA_pozzetti.csv"), "{nomi:?}");
    assert!(nomi.iter().any(|n| n == "Piastra ELISA.asm.json"), "{nomi:?}");
    assert!(nomi.iter().any(|n| n == "Piastra ELISA_piatto.csv"), "{nomi:?}");
    // L'originale non si tocca.
    assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(fixture("Magellan_Pro_7_5_compact_metadata.xlsx")).unwrap());

    // Il file Allotrope è JSON e ha il suo schema.
    let asm: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("Piastra ELISA.asm.json")).unwrap()).unwrap();
    assert!(asm["$asm.manifest"].as_str().is_some_and(|m| m.contains("allotrope")), "{}", asm["$asm.manifest"]);

    // Il lavoro dice da quale strumento e da quale originale (con la sua impronta) vengono i dati.
    let l = s.app.lavori.last().expect("un lavoro registrato");
    assert!(!l.strumento.is_empty(), "il lavoro non dice lo strumento");
    assert_eq!(l.origini.len(), 1);
    assert_eq!(l.origini[0].0, "Piastra ELISA.xlsx");
    assert_eq!(l.origini[0].1.len(), 64, "{}", l.origini[0].1);
    assert!(l.outputs.len() >= 4, "{:?}", l.outputs);
    assert!(l.per_quaderno().contains("SHA-256"));
    // E l'esito ha le note da mostrare a destra.
    assert!(s.app.strum_esiti.contains_key(&path) || !s.app.strum_esiti.is_empty());
    disegna(&mut s);

    // Cambiata una scelta, il file già fatto si rifà (non resta «Fatto» con le scelte vecchie).
    s.act(Act::Strum(StrumAct::Esce(Uscita::Csv)), &mut plat);
    disegna(&mut s);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn aggiungere_un_file_non_rifa_quelli_gia_fatti() {
    ambiente();
    let dir = cartella("gia-fatti");
    let mut plat = Finta;
    let mut s = schermo();
    s.act(Act::Lab(Lab::Dati), &mut plat);
    let a = apri(&mut s, &mut plat, &dir, "Magellan_Pro_7_5_compact_metadata.xlsx", "Piastra A.xlsx");
    s.act(Act::Start, &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.files.iter().all(|f| f.state == FileState::Done || f.state == FileState::Failed) && !s.app.busy()), "il lavoro non è finito");
    assert!(s.app.todo().is_empty(), "dopo «Converti» non resta niente da fare");
    let uscite = |d: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    let prima = uscite(&dir);

    // Arriva un altro file: il lavoro è solo per lui, quello già fatto non si rifà (e non nascono copie «-2»).
    let b = apri(&mut s, &mut plat, &dir, "thermo_fisher_qubit4_example_1.csv", "Qubit B.csv");
    assert!(aspetta(&mut s, 30, |s| s.app.strum_lettura(&b).is_some()), "il secondo file non è stato riconosciuto");
    let da_fare: Vec<PathBuf> = s.app.todo().into_iter().map(|i| s.app.files[i].path.clone()).collect();
    assert_eq!(da_fare, vec![b.clone()], "da fare: {da_fare:?}");
    s.act(Act::Start, &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.files.iter().all(|f| f.state == FileState::Done || f.state == FileState::Failed) && !s.app.busy()), "il secondo lavoro non è finito");
    let dopo = uscite(&dir);
    assert!(!dopo.iter().any(|n| n.contains("-2")), "copie doppie: {dopo:?}");
    assert!(prima.iter().all(|n| dopo.contains(n)), "{prima:?} / {dopo:?}");

    // Cambiato il ruolo di un pozzetto di A, si rifà A e solo A.
    s.app.selected = s.app.files.iter().position(|f| f.path == a);
    disegna(&mut s);
    s.app.strum_premi((0, 0));
    s.app.strum_rilascia();
    s.act(Act::Strum(StrumAct::Ruolo('B')), &mut plat);
    let da_fare: Vec<PathBuf> = s.app.todo().into_iter().map(|i| s.app.files[i].path.clone()).collect();
    assert_eq!(da_fare, vec![a], "da fare: {da_fare:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn i_profili_piccoli_si_leggono_e_si_disegnano() {
    ambiente();
    let dir = cartella("piccoli");
    let mut plat = Finta;
    let mut s = schermo();
    s.act(Act::Lab(Lab::Dati), &mut plat);
    // Un fluorimetro, un contacellule, un qPCR e un ciclatore: nessuna piastra, o una piastra di qPCR.
    for (file, nome) in [
        ("thermo_fisher_qubit4_example_1.csv", "Qubit.csv"),
        ("Beckman_Vi-Cell-BLU_different_mu_character.csv", "Contacellule.csv"),
        ("appbio_quantstudio_multiple_cal_doc_wells.txt", "qPCR.txt"),
        ("cfx-maestro-esempio.csv", "CFX.csv"),
    ] {
        let path = apri(&mut s, &mut plat, &dir, file, nome);
        s.app.selected = s.app.files.iter().position(|f| f.path == path);
        let (_, l) = s.app.strum_corrente().expect(nome);
        assert!(l.conta() > 0, "{nome}: nessuna misura");
        for v in [Vista::Piastra, Vista::Elenco, Vista::Strumento] {
            s.act(Act::Strum(StrumAct::Vista(v)), &mut plat);
            disegna(&mut s);
        }
        for p in [Pronta::Excel, Pronta::Csv, Pronta::Allotrope] {
            s.act(Act::SetSimple(true), &mut plat);
            s.act(Act::Strum(StrumAct::Pronta(p)), &mut plat);
            disegna(&mut s);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Con `CEDRO_PNG_OUT=/cartella` (e `CEDRO_ALLOTROPY` per i file d'esempio grandi) la finestra si salva in immagini,
/// per confrontarla a occhio con l'anteprima approvata. Senza la variabile non fa niente.
#[test]
fn foto_della_finestra() {
    let Some(out) = std::env::var_os("CEDRO_PNG_OUT").map(PathBuf::from) else { return };
    ambiente();
    std::fs::create_dir_all(&out).unwrap();
    let dir = cartella("foto");
    let root = std::env::var_os("CEDRO_ALLOTROPY").map(PathBuf::from);
    let mut casi: Vec<(String, PathBuf)> = vec![
        ("magellan-piccolo".into(), fixture("Magellan_Pro_7_5_compact_metadata.xlsx")),
        ("qubit".into(), fixture("thermo_fisher_qubit4_example_1.csv")),
        ("cfx".into(), fixture("cfx-maestro-esempio.csv")),
        ("vicell".into(), fixture("Beckman_Vi-Cell-BLU_different_mu_character.csv")),
    ];
    if let Some(r) = &root {
        casi.push(("magellan-od".into(), r.join("tests/parsers/tecan_magellan/testdata/SLA01P01_OD.xlsx")));
        casi.push(("magellan-elisa".into(), r.join("tests/parsers/tecan_magellan/testdata/SLA01P01_ELISA.xlsx")));
    }
    let mut plat = Finta;
    for (nome, file) in casi {
        let mut s = schermo();
        s.act(Act::Lab(Lab::Dati), &mut plat);
        let ext = file.extension().unwrap().to_string_lossy().into_owned();
        let path = dir.join(format!("{nome}.{ext}"));
        std::fs::copy(&file, &path).unwrap();
        s.drop_files(vec![path.clone()]);
        assert!(aspetta(&mut s, 30, |s| s.app.strum_corrente().is_some()), "{nome}: non riconosciuto");
        disegna(&mut s);
        let (_, l) = s.app.strum_corrente().unwrap();
        // Qualche pozzetto segnato, per vedere i ruoli e la scelta.
        if l.a_piastra() {
            s.app.strum_premi((0, 0));
            s.app.strum_muovi((0, 1));
            s.app.strum_rilascia();
            s.act(Act::Strum(StrumAct::Ruolo('B')), &mut plat);
            s.app.strum_premi((1, 0));
            s.app.strum_muovi((1, 2));
            s.app.strum_rilascia();
        }
        s.app.strum.sopra = Some((1, 1));
        for (sfx, semplice, vista) in [("piastra", false, Vista::Piastra), ("elenco", false, Vista::Elenco), ("strumento", false, Vista::Strumento), ("semplice", true, Vista::Piastra)] {
            s.act(Act::SetSimple(semplice), &mut plat);
            s.act(Act::Strum(StrumAct::Vista(vista)), &mut plat);
            let (w, h) = s.preferred_size();
            for _ in 0..3 {
                let _ = s.render(w as u32, h as u32, 1.0);
            }
            s.render(w as u32, h as u32, 1.0).save_png(&out.join(format!("{nome}-{sfx}.png"))).unwrap();
        }
        s.act(Act::SetSimple(false), &mut plat);
        s.act(Act::Strum(StrumAct::Esce(Uscita::Allotrope)), &mut plat);
        s.act(Act::Strum(StrumAct::Vista(Vista::Allotrope)), &mut plat);
        let (w, h) = s.preferred_size();
        for _ in 0..3 {
            let _ = s.render(w as u32, h as u32, 1.0);
        }
        s.render(w as u32, h as u32, 1.0).save_png(&out.join(format!("{nome}-allotrope.png"))).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Come `foto_della_finestra`, ma alla misura minima della finestra e con sei file in coda, uno rotto:
/// serve a vedere a occhio che nessun testo esce dal suo riquadro.
#[test]
fn foto_della_finestra_minima() {
    let Some(out) = std::env::var_os("CEDRO_PNG_OUT").map(PathBuf::from) else { return };
    ambiente();
    std::fs::create_dir_all(&out).unwrap();
    let dir = cartella("minima");
    let mut plat = Finta;
    let mut s = schermo();
    s.act(Act::Lab(Lab::Dati), &mut plat);
    let (w, h) = s.min_size();
    let (w, h) = (w as u32, h as u32);
    let foto = |s: &mut Screen, nome: &str| {
        for _ in 0..3 {
            let _ = s.render(w, h, 1.0);
        }
        s.render(w, h, 1.0).save_png(&out.join(format!("minima-{nome}.png"))).unwrap();
    };
    let mut percorsi = Vec::new();
    for (file, nome) in [
        ("Magellan_Pro_7_5_compact_metadata.xlsx", "Piastra ELISA.xlsx"),
        ("thermo_fisher_qubit4_example_1.csv", "Qubit.csv"),
        ("Beckman_Vi-Cell-BLU_different_mu_character.csv", "Contacellule.csv"),
        ("appbio_quantstudio_multiple_cal_doc_wells.txt", "qPCR.txt"),
        ("cfx-maestro-esempio.csv", "CFX.csv"),
    ] {
        percorsi.push(apri(&mut s, &mut plat, &dir, file, nome));
    }
    let rotto = dir.join("Rotto.xlsx");
    std::fs::write(&rotto, "questo non è un foglio di calcolo, ma un testo qualunque").unwrap();
    s.drop_files(vec![rotto.clone()]);
    assert!(aspetta(&mut s, 30, |s| s.app.files.iter().any(|f| f.path == rotto)));
    // La piastra con i ruoli, e tutte le uscite accese: il pannello più pieno.
    s.app.selected = s.app.files.iter().position(|f| f.path == percorsi[0]);
    disegna(&mut s);
    s.app.strum_premi((0, 0));
    s.app.strum_muovi((0, 1));
    s.app.strum_rilascia();
    s.act(Act::Strum(StrumAct::Ruolo('B')), &mut plat);
    s.act(Act::SetSimple(false), &mut plat);
    for u in [Uscita::Csv, Uscita::Allotrope, Uscita::Piatto] {
        s.act(Act::Strum(StrumAct::Esce(u)), &mut plat);
    }
    s.act(Act::Strum(StrumAct::MenoBianco(true)), &mut plat);
    foto(&mut s, "1-piastra");
    s.act(Act::Strum(StrumAct::Vista(Vista::Elenco)), &mut plat);
    foto(&mut s, "2-elenco");
    s.act(Act::Strum(StrumAct::Vista(Vista::Strumento)), &mut plat);
    foto(&mut s, "3-strumento");
    s.act(Act::Strum(StrumAct::Vista(Vista::Piastra)), &mut plat);
    s.act(Act::SetSimple(true), &mut plat);
    foto(&mut s, "4-semplice");
    // Si converte tutto: cinque riescono, uno no.
    s.act(Act::Start, &mut plat);
    assert!(aspetta(&mut s, 120, |s| s.app.files.iter().all(|f| f.state == FileState::Done || f.state == FileState::Failed) && !s.app.busy()), "il lavoro non è finito");
    foto(&mut s, "5-finito-scelto-buono");
    s.app.selected = s.app.files.iter().position(|f| f.path == rotto);
    foto(&mut s, "6-finito-scelto-rotto");
    s.act(Act::SetSimple(false), &mut plat);
    foto(&mut s, "7-finito-scelto-rotto-completa");
    let _ = std::fs::remove_dir_all(&dir);
}
