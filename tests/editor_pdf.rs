//! Prove dell'editor dei PDF (scheda PDF → «Modifica»): lo stato (modifiche, annulla, contatori), la schermata
//! senza schermo vero (ogni scheda e ogni strumento si disegna in tutte le tinte) e il giro completo
//! aprire → modificare → salvare una copia → uscire. Il file di partenza non si tocca mai.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::pdfmod::{self, apply, Edit, FieldKind, MarkKind, Rect, SaveOpts};
use cedro::ui::core::{Act, Scroll};
use cedro::ui::model::PdfMode;
use cedro::ui::pdfedact::EdAct;
use cedro::ui::pdfeditor::{EdTab, PdfEd};
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::ThemeId;

struct Finta {
    aperti: Vec<PathBuf>,
    /// La finestra è stata chiusa dalla piattaforma.
    chiusa: bool,
}

impl Platform for Finta {
    fn pick_files(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_folder(&mut self, _: &str) -> Option<PathBuf> {
        None
    }
    fn open_path(&mut self, p: &Path) {
        self.aperti.push(p.to_path_buf());
    }
    fn copy_text(&mut self, _: &str) {}
    fn paste_text(&mut self) -> Option<String> {
        None
    }
    fn open_guide(&mut self) {}
    fn open_settings(&mut self) {}
    fn minimize(&mut self) {}
    fn toggle_maximize(&mut self) {}
    fn close(&mut self) {
        self.chiusa = true;
    }
    fn open_full(&mut self) {}
}

fn italiano() {
    cedro::i18n::fix();
    cedro::i18n::set(cedro::i18n::Lang::It);
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

fn cartella(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cedro-test-editor-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Fa girare lo schermo finché `fatto` non è vero (o passano `secondi`).
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

fn apri_editor(s: &mut Screen, plat: &mut Finta, path: &Path) {
    s.drop_files(vec![path.to_path_buf()]);
    assert!(aspetta(s, 20, |s| s.app.files.iter().any(|f| f.path == path)), "il file non è stato preso");
    s.act(Act::PdfMode(PdfMode::Edit), plat);
    assert!(aspetta(s, 30, |s| s.app.ed.as_ref().is_some_and(|e| e.opened.is_some())), "l'editor non ha aperto il PDF");
}

#[test]
fn state_edits_undo_redo_and_counters() {
    let mut ed = PdfEd::new(Path::new("/tmp/Richiesta suolo.pdf"));
    assert_eq!(ed.unsaved(), 0);
    assert_eq!(ed.out_name(), "Richiesta suolo-modificato.pdf");
    assert!(ed.undo.is_empty() && ed.redo.is_empty());

    ed.push(Edit::Note { page: 0, x: 40.0, y: 40.0, text: "Controllare".into(), color: [250, 210, 60] });
    ed.push(Edit::Mark { page: 0, kind: MarkKind::Highlight, rects: vec![Rect::new(70.0, 100.0, 120.0, 12.0)], color: [255, 235, 59], note: String::new() });
    assert_eq!(ed.edits.len(), 2);
    assert_eq!(ed.unsaved(), 2);
    assert!(ed.status().ends_with("2 modifiche non salvate"), "{}", ed.status());

    // Le aree da oscurare si contano finché non sono applicate; i campi compilati una volta per campo.
    ed.push(Edit::Redact { page: 0, rect: Rect::new(10.0, 10.0, 80.0, 20.0), applied: false });
    ed.push(Edit::Redact { page: 1, rect: Rect::new(10.0, 10.0, 80.0, 20.0), applied: true });
    assert_eq!(ed.pending_redacts(), 1);
    ed.set_field("Cognome", "Rossi".into());
    ed.set_field("Cognome", "Bianchi".into());
    ed.set_field("Nome", "Mario".into());
    assert_eq!(ed.filled_fields(), 2);
    assert_eq!(ed.field_value("Cognome"), "Bianchi");
    assert_eq!(ed.edits.iter().filter(|e| matches!(e, Edit::FieldValue { name, .. } if name == "Cognome")).count(), 1);

    // Annulla e ripeti tornano indietro e avanti una modifica alla volta; una modifica nuova svuota il «ripeti».
    let n = ed.edits.len();
    ed.undo();
    assert_eq!(ed.edits.len(), n - 1);
    assert_eq!(ed.filled_fields(), 1);
    ed.undo(); // «Cognome» torna a «Rossi»
    assert_eq!(ed.field_value("Cognome"), "Rossi");
    ed.undo();
    assert_eq!(ed.filled_fields(), 0);
    ed.redo();
    assert_eq!(ed.filled_fields(), 1);
    ed.push(Edit::Note { page: 1, x: 1.0, y: 1.0, text: "x".into(), color: [0, 0, 0] });
    assert!(ed.redo.is_empty());
    while !ed.undo.is_empty() {
        ed.undo();
    }
    assert!(ed.edits.is_empty());

    // I nomi dei campi nuovi non si ripetono.
    ed.push(Edit::NewField { page: 0, rect: Rect::new(0.0, 0.0, 100.0, 20.0), kind: FieldKind::Text, name: "Testo1".into(), options: vec![] });
    assert_eq!(ed.new_field_name(FieldKind::Text), "Testo2");
    assert_eq!(ed.new_field_name(FieldKind::Check), "Spunta1");
}

#[test]
fn every_tab_and_tool_draws_in_every_tint() {
    italiano();
    let dir = cartella("disegno");
    let path = dir.join("Circolare.pdf");
    std::fs::write(&path, fixture("circolare.pdf")).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    // Ogni strumento di ogni scheda, nella tinta di partenza: pannello Completo e Semplice, a sinistra pagine e commenti.
    for tab in EdTab::ALL {
        s.act(Act::Ed(EdAct::Tab(tab)), &mut plat);
        for gruppo in tab.tools() {
            for tool in gruppo.iter() {
                s.act(Act::Ed(EdAct::Tool(*tool)), &mut plat);
                for semplice in [false, true] {
                    s.act(Act::Ed(EdAct::Simple(semplice)), &mut plat);
                    let (w, h) = s.preferred_size();
                    let _ = s.render(w as u32, h as u32, 1.0);
                }
                s.act(Act::Ed(EdAct::Left(1)), &mut plat);
                let _ = s.render(900, 620, 1.0);
                s.act(Act::Ed(EdAct::Left(0)), &mut plat);
            }
        }
    }
    // Le altre tinte: una scheda per una.
    for tema in [ThemeId::CedroNotte, ThemeId::CedroBosco, ThemeId::CedroCarta] {
        s.request_appearance(tema, false);
        for tab in EdTab::ALL {
            s.act(Act::Ed(EdAct::Tab(tab)), &mut plat);
            let (w, h) = s.preferred_size();
            let _ = s.render(w as u32, h as u32, 1.0);
        }
    }
    // Due pagine affiancate e il passaggio a pagina intera.
    s.act(Act::Ed(EdAct::TwoPages(true)), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    s.act(Act::Ed(EdAct::TwoPages(false)), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_save_copy_and_leave() {
    italiano();
    let dir = cartella("giro");
    let path = dir.join("Richiesta.pdf");
    let originale = fixture("circolare.pdf");
    std::fs::write(&path, &originale).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);

    // Senza modifiche si esce subito.
    assert_eq!(s.app.ed.as_ref().unwrap().unsaved(), 0);

    // Qualche modifica: una nota, un'evidenziatura, un rettangolo e un timbro.
    {
        let ed = s.app.ed.as_mut().unwrap();
        ed.push(Edit::Note { page: 0, x: 440.0, y: 90.0, text: "Controllare la data".into(), color: [250, 210, 60] });
        ed.push(Edit::Mark { page: 0, kind: MarkKind::Highlight, rects: vec![Rect::new(70.0, 120.0, 150.0, 12.0)], color: [255, 235, 59], note: String::new() });
        ed.push(Edit::Shape { page: 0, kind: pdfmod::ShapeKind::Rect, a: (70.0, 500.0), b: (220.0, 560.0), color: [224, 60, 72], fill: None, width: 2.0 });
        ed.push(Edit::Stamp { page: 0, rect: Rect::new(330.0, 700.0, 150.0, 40.0), label: "APPROVATO".into(), color: [40, 170, 90] });
    }
    assert_eq!(s.app.ed.as_ref().unwrap().unsaved(), 4);
    let _ = s.render(1536, 1024, 1.0);

    // Con modifiche non salvate «Esci» chiede prima: «Resta» le tiene, «Esci senza salvare» le butta.
    s.act(Act::Ed(EdAct::Leave(None)), &mut plat);
    assert!(s.app.ed.as_ref().unwrap().leave.is_some(), "doveva chiedere");
    s.act(Act::Ed(EdAct::LeaveStay), &mut plat);
    assert!(s.app.ed.as_ref().is_some_and(|e| e.leave.is_none() && e.unsaved() == 4));

    // Salvare scrive una copia accanto all'originale, che resta com'era.
    s.act(Act::Ed(EdAct::Save), &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.ed.as_ref().is_some_and(|e| e.saved.is_some())), "il salvataggio non è finito");
    let fuori = s.app.ed.as_ref().unwrap().saved.clone().unwrap();
    assert_eq!(fuori.file_name().unwrap().to_string_lossy(), "Richiesta-modificato.pdf");
    assert_eq!(std::fs::read(&path).unwrap(), originale, "l'originale non va toccato");
    let bytes = std::fs::read(&fuori).unwrap();
    let aperto = pdfmod::open(&bytes).expect("la copia si apre");
    assert_eq!(aperto.sizes.len(), pdfmod::open(&originale).unwrap().sizes.len());
    assert!(aperto.comments.len() >= 3, "i commenti devono stare nel file: {}", aperto.comments.len());
    assert_eq!(s.app.ed.as_ref().unwrap().unsaved(), 0);

    // «Apri il PDF» e «Apri la cartella» passano dalla piattaforma.
    s.act(Act::Ed(EdAct::OpenSaved), &mut plat);
    s.act(Act::Ed(EdAct::OpenFolder), &mut plat);
    assert_eq!(plat.aperti, vec![fuori.clone(), dir.clone()]);

    // Una modifica nuova e poi «Esci senza salvare»: l'editor si chiude, il file di prima resta.
    s.act(Act::Ed(EdAct::KeepEditing), &mut plat);
    s.app.ed.as_mut().unwrap().push(Edit::Note { page: 0, x: 10.0, y: 10.0, text: "ancora".into(), color: [0, 0, 0] });
    s.act(Act::Ed(EdAct::Leave(None)), &mut plat);
    s.act(Act::Ed(EdAct::LeaveDiscard), &mut plat);
    assert!(s.app.ed.is_none(), "doveva chiudersi");
    assert_eq!(std::fs::read(&fuori).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn form_fields_are_filled_and_saved_for_real() {
    italiano();
    let dir = cartella("moduli");
    let base = fixture("circolare.pdf");
    let campi = vec![
        Edit::NewField { page: 0, rect: Rect::new(70.0, 330.0, 200.0, 22.0), kind: FieldKind::Text, name: "Cognome".into(), options: vec![] },
        Edit::NewField { page: 0, rect: Rect::new(70.0, 370.0, 14.0, 14.0), kind: FieldKind::Check, name: "Informativa".into(), options: vec![] },
    ];
    let path = dir.join("Modulo.pdf");
    std::fs::write(&path, apply(&base, &campi, &SaveOpts::default()).unwrap()).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    assert_eq!(s.app.ed.as_ref().unwrap().opened.as_ref().unwrap().fields.len(), 2);
    {
        let ed = s.app.ed.as_mut().unwrap();
        ed.set_field("Cognome", "Rossi".into());
        ed.set_field("Informativa", "On".into());
        assert_eq!(ed.filled_fields(), 2);
    }
    s.act(Act::Ed(EdAct::Save), &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.ed.as_ref().is_some_and(|e| e.saved.is_some())), "il salvataggio non è finito");
    let fuori = s.app.ed.as_ref().unwrap().saved.clone().unwrap();
    let aperto = pdfmod::open(&std::fs::read(&fuori).unwrap()).unwrap();
    let v = |n: &str| aperto.fields.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_default();
    assert_eq!(v("Cognome"), "Rossi");
    assert!(v("Informativa") == "On" || !v("Informativa").is_empty() && v("Informativa") != "Off", "la spunta doveva restare accesa: {:?}", v("Informativa"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn applied_redaction_removes_the_text_for_real() {
    italiano();
    let dir = cartella("oscura");
    let path = dir.join("Circolare.pdf");
    let base = fixture("circolare.pdf");
    std::fs::write(&path, &base).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    // Il testo di pagina 1 e il rettangolo della prima parola da oscurare.
    assert!(aspetta(&mut s, 20, |s| {
        let _ = s;
        true
    }));
    let testo = pdfmod::read_texts(&base, &[0]).ok().and_then(|v| v.into_iter().next().flatten()).expect("testo della pagina");
    let parola = testo.words.first().expect("almeno una parola").clone();
    let r = parola.rect.inflate(1.0);
    {
        let ed = s.app.ed.as_mut().unwrap();
        ed.push(Edit::Redact { page: 0, rect: r, applied: false });
        assert_eq!(ed.pending_redacts(), 1);
    }
    // «Applica l'oscuramento» le rende definitive: da quel momento non ce n'è più di segnate.
    s.act(Act::Ed(EdAct::Tab(EdTab::Redact)), &mut plat);
    s.act(Act::Ed(EdAct::ApplyRedact), &mut plat);
    assert!(aspetta(&mut s, 30, |s| s.app.ed.as_ref().is_some_and(|e| e.pending_redacts() == 0)), "l'oscuramento non è stato applicato");
    assert!(s.app.ed.as_ref().unwrap().edits.iter().any(|e| matches!(e, Edit::Redact { applied: true, .. })));
    s.act(Act::Ed(EdAct::Save), &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.ed.as_ref().is_some_and(|e| e.saved.is_some())), "il salvataggio non è finito");
    let fuori = s.app.ed.as_ref().unwrap().saved.clone().unwrap();
    let dopo = pdfmod::read_texts(&std::fs::read(&fuori).unwrap(), &[0]).ok().and_then(|v| v.into_iter().next().flatten()).expect("testo dopo");
    assert!(!dopo.words.iter().any(|w| w.text == parola.text && w.rect.intersects(&r)), "la parola {:?} doveva sparire dal file", parola.text);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn page_buttons_and_thumbnails_scroll_the_page() {
    italiano();
    let dir = cartella("scorri");
    let path = dir.join("Colonne.pdf");
    std::fs::write(&path, fixture("colonne.pdf")).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    let _ = s.render(1536, 1024, 1.0);
    let n = s.app.ed.as_ref().unwrap().page_count();
    assert!(n >= 3, "serve un PDF di più pagine: {n}");
    assert_eq!(s.ui.scroll(Scroll::EdPage), 0.0);

    // «Pagina avanti» porta la lettura alla pagina dopo, e la pagina di adesso la segue.
    s.act(Act::Ed(EdAct::Next), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    let uno = s.ui.scroll(Scroll::EdPage);
    assert!(uno > 100.0, "«Pagina avanti» doveva scorrere la pagina: {uno}");
    assert_eq!(s.app.ed.as_ref().unwrap().lay.borrow().cur, 1);

    // Una miniatura porta alla sua pagina, anche più in là.
    s.act(Act::Ed(EdAct::Thumb(2)), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    let tre = s.ui.scroll(Scroll::EdPage);
    assert!(tre > uno, "la miniatura 3 doveva stare più in giù: {tre} contro {uno}");
    assert_eq!(s.app.ed.as_ref().unwrap().lay.borrow().cur, 2);

    // «Pagina indietro» e poi la prima miniatura tornano su.
    s.act(Act::Ed(EdAct::Prev), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    assert!(s.ui.scroll(Scroll::EdPage) < tre);
    s.act(Act::Ed(EdAct::Thumb(0)), &mut plat);
    let _ = s.render(1536, 1024, 1.0);
    assert!(s.ui.scroll(Scroll::EdPage) < uno);
    assert_eq!(s.app.ed.as_ref().unwrap().lay.borrow().cur, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_window_with_unsaved_edits_asks_first() {
    italiano();
    let dir = cartella("chiusura");
    let path = dir.join("Chiudi.pdf");
    let originale = fixture("circolare.pdf");
    std::fs::write(&path, &originale).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    let _ = s.render(1536, 1024, 1.0);

    // Senza modifiche la X chiude subito.
    assert!(!s.close_guard(), "senza modifiche non serve chiedere");

    // Con una modifica la X ferma la finestra e chiede (salva, butta o resta).
    let nota = |s: &mut Screen| s.app.ed.as_mut().unwrap().push(Edit::Note { page: 0, x: 40.0, y: 40.0, text: "da non perdere".into(), color: [250, 210, 60] });
    nota(&mut s);
    assert!(s.close_guard(), "con modifiche non salvate doveva chiedere");
    assert!(s.app.ed.as_ref().unwrap().leave.is_some());
    let _ = s.render(1536, 1024, 1.0);

    // «Resta qui»: la finestra resta aperta e le modifiche pure.
    s.act(Act::Ed(EdAct::LeaveStay), &mut plat);
    assert!(!plat.chiusa && !s.take_close());
    assert!(s.app.ed.as_ref().is_some_and(|e| e.leave.is_none() && e.unsaved() == 1));

    // «Esci senza salvare»: l'editor si chiude e con lui la finestra; il file non è stato toccato.
    assert!(s.close_guard());
    s.act(Act::Ed(EdAct::LeaveDiscard), &mut plat);
    assert!(s.app.ed.is_none() && plat.chiusa, "doveva chiudere la finestra");
    assert_eq!(std::fs::read(&path).unwrap(), originale);
    assert!(!dir.join("Chiudi-modificato.pdf").exists());

    // «Salva ed esci»: scrive la copia e, a salvataggio finito, chiede di chiudere la finestra.
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    nota(&mut s);
    assert!(s.close_guard());
    s.act(Act::Ed(EdAct::LeaveSave), &mut plat);
    assert!(aspetta(&mut s, 60, |s| s.app.ed.is_none() || s.app.close_wanted), "il salvataggio non è finito");
    assert!(s.take_close(), "a salvataggio finito doveva chiedere di chiudere");
    assert!(dir.join("Chiudi-modificato.pdf").exists(), "la copia doveva essere scritta");
    assert_eq!(std::fs::read(&path).unwrap(), originale);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_text_starts_black_and_the_ink_stays_red_for_marks() {
    italiano();
    let dir = cartella("colori");
    let path = dir.join("Colori.pdf");
    std::fs::write(&path, fixture("circolare.pdf")).unwrap();
    let mut plat = Finta { aperti: Vec::new(), chiusa: false };
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    apri_editor(&mut s, &mut plat, &path);
    let ed = s.app.ed.as_ref().unwrap();
    assert_eq!(ed.text_ink, cedro::ui::pdfeditor::INK_COLORS[0], "il testo nuovo parte nero");
    assert_eq!(ed.ink, cedro::ui::pdfeditor::INK_COLORS[2], "segni e disegni partono rossi");
    // Scegliere un colore con «Aggiungi testo» in mano cambia solo il colore del testo.
    s.act(Act::Ed(EdAct::Tab(EdTab::Text)), &mut plat);
    s.app.ed.as_mut().unwrap().tool = cedro::ui::pdfeditor::Tool::AddText;
    s.act(Act::Ed(EdAct::Ink(cedro::ui::pdfeditor::INK_COLORS[1])), &mut plat);
    let ed = s.app.ed.as_ref().unwrap();
    assert_eq!(ed.text_ink, cedro::ui::pdfeditor::INK_COLORS[1]);
    assert_eq!(ed.ink, cedro::ui::pdfeditor::INK_COLORS[2]);
    let _ = std::fs::remove_dir_all(&dir);
}
