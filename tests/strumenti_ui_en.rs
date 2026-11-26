//! La finestra dei dati degli strumenti in inglese: nessun testo deve restare in italiano. La lingua è una
//! sola per tutto il processo, per questo la prova sta in un file a parte.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::i18n::{misses, record_misses, set, Lang};
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

#[test]
fn nessun_testo_resta_in_italiano() {
    let casa = std::env::temp_dir().join(format!("cedro-test-strumenti-en-casa-{}", std::process::id()));
    let dir = std::env::temp_dir().join(format!("cedro-test-strumenti-en-{}", std::process::id()));
    for d in [&casa, &dir] {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).unwrap();
    }
    std::env::set_var("HOME", &casa);
    std::env::remove_var("APPDATA");
    cedro::i18n::fix();
    set(Lang::En);
    record_misses(true);
    let fx = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/strumenti");
    let mut plat = Finta;
    for (file, nome) in [
        ("Magellan_Pro_7_5_compact_metadata.xlsx", "Plate.xlsx"),
        ("thermo_fisher_qubit4_example_1.csv", "Qubit.csv"),
        ("Beckman_Vi-Cell-BLU_different_mu_character.csv", "Cells.csv"),
        ("appbio_quantstudio_multiple_cal_doc_wells.txt", "qPCR.txt"),
        ("cfx-maestro-esempio.csv", "CFX.csv"),
    ] {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(ThemeId::CedroLabs, false);
        s.act(Act::Lab(Lab::Dati), &mut plat);
        let path = dir.join(nome);
        std::fs::copy(fx.join(file), &path).unwrap();
        s.drop_files(vec![path.clone()]);
        assert!(aspetta(&mut s, 30, |s| s.app.strum_corrente().is_some()), "{nome}");
        // Ruoli segnati su qualche pozzetto: così si disegnano anche il riassunto dei ruoli, la scelta «meno il bianco»
        // e le colonne dei ruoli, che senza bianchi non compaiono.
        for (da, a, ruolo) in [((0, 0), (0, 1), 'B'), ((1, 0), (1, 1), 'S'), ((2, 0), (2, 1), 'C'), ((3, 0), (3, 1), 'R')] {
            s.app.strum_premi(da);
            s.app.strum_muovi(a);
            s.app.strum_rilascia();
            s.act(Act::Strum(StrumAct::Ruolo(ruolo)), &mut plat);
        }
        for semplice in [false, true] {
            s.act(Act::SetSimple(semplice), &mut plat);
            for v in [Vista::Piastra, Vista::Elenco, Vista::Strumento] {
                s.act(Act::Strum(StrumAct::Vista(v)), &mut plat);
                s.app.strum_premi((0, 0));
                s.app.strum_muovi((1, 1));
                s.app.strum_rilascia();
                disegna(&mut s);
                s.act(Act::Strum(StrumAct::Nessuno), &mut plat);
                disegna(&mut s);
            }
            for p in [Pronta::Excel, Pronta::Csv, Pronta::Allotrope] {
                s.act(Act::Strum(StrumAct::Pronta(p)), &mut plat);
                disegna(&mut s);
            }
        }
        s.act(Act::SetSimple(false), &mut plat);
        s.act(Act::Strum(StrumAct::Esce(Uscita::Allotrope)), &mut plat);
        s.act(Act::Strum(StrumAct::Esce(Uscita::Piatto)), &mut plat);
        s.act(Act::Strum(StrumAct::MenoBianco(true)), &mut plat);
        s.act(Act::Strum(StrumAct::Daltonici(true)), &mut plat);
        s.act(Act::Strum(StrumAct::Vista(Vista::Allotrope)), &mut plat);
        disegna(&mut s);
        s.act(Act::Strum(StrumAct::CambiaStrumento), &mut plat);
        disegna(&mut s);
        s.act(Act::Strum(StrumAct::Profilo("tecan_magellan".into())), &mut plat);
        aspetta(&mut s, 10, |s| !s.app.strum_file(&path).rileggendo);
        disegna(&mut s);
        s.act(Act::Strum(StrumAct::Profilo("vi_cell_xr".into())), &mut plat);
        aspetta(&mut s, 10, |s| !s.app.strum_file(&path).rileggendo);
        disegna(&mut s);
        s.act(Act::Strum(StrumAct::Profilo(String::new())), &mut plat);
        disegna(&mut s);
        s.act(Act::Start, &mut plat);
        assert!(aspetta(&mut s, 60, |s| !s.app.busy() && s.app.files.iter().all(|f| matches!(f.state, FileState::Done | FileState::Failed))), "{nome}: non finito");
        for _ in 0..3 {
            disegna(&mut s);
        }
        let _ = s.app.lavori.last().map(|l| l.per_quaderno());
    }
    // Restano senza traduzione solo i nomi che sono uguali in inglese, i testi del resto della finestra
    // (la barra di sinistra, che qui si traduce due volte) e il nome dei fogli di Excel, che sta in italiano.
    const UGUALI: [&str; 12] = ["Allotrope", "Allotrope (.asm.json)", "CSV", "Cedro", "Schema", "standard", "qPCR", "Mappa, Pozzetti, Strumento", "4 Labs", "Home", "Jobs", "Transcribe and"];
    let mancano: Vec<String> = misses()
        .into_iter()
        .filter(|m| m.chars().count() > 1 && !UGUALI.contains(&m.as_str()) && !m.contains(" pagin") && !m.starts_with("Example: ") && !m.starts_with("+ Case") && !m.starts_with("Convert ") && !m.starts_with("Prepare ") && !m.starts_with("Remove ") && !m.starts_with("Save ") && !m.starts_with("Sign ") && !m.starts_with("Transcribe") && !m.starts_with("Work on") && m != "summarise" && !e_misura(m))
        .collect();
    // Le misure dei file («2 KB») sono uguali in tutte le lingue: senza il numero di pagine restano da sole.
    fn e_misura(m: &str) -> bool {
        m.split_once(' ').is_some_and(|(n, u)| n.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') && matches!(u, "B" | "KB" | "MB" | "GB"))
    }
    assert!(mancano.is_empty(), "testi rimasti in italiano:\n{}", mancano.join("\n"));
    // E le frasi con i numeri e i pezzi di file si traducono come devono.
    let t = |s: &str| cedro::i18n::tr_string(s);
    assert_eq!(t("Qui le prime 4 colonne di 12: nell'Excel ci sono tutte."), "Here are the first 4 columns of 12: the Excel has them all.");
    assert_eq!(t("4 pozzetti"), "4 wells");
    assert_eq!(t("1 pozzetto"), "1 well");
    assert_eq!(t(&format!("2 pozzetti «{}».", cedro::i18n::lit("Undetermined"))), "2 wells «Undetermined».");
    assert_eq!(t("Tecan Magellan · 4 misure → Excel + CSV + Allotrope + CSV piatto"), "Tecan Magellan · 4 measurements → Excel + CSV + Allotrope + flat CSV");
    assert_eq!(t(&cedro::i18n::lit("Normal Wash")), "Normal Wash");
    let _ = std::fs::remove_dir_all(&dir);
}
