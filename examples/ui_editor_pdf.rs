//! Fotografie dell'editor dei PDF (scheda PDF → «Modifica»), senza schermo.
//! `cargo run --example ui_editor_pdf -- <cartella> [tinta]`; la tinta è una di cedro, notte, bosco, carta.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::pdfmod::{apply, Edit, FieldKind, Rect, SaveOpts};
use cedro::ui::core::Act;
use cedro::ui::model::PdfMode;
use cedro::ui::pdfedact::EdAct;
use cedro::ui::pdfeditor::{EdTab, Tool};
use cedro::ui::screen::{Platform, Screen, Surface};
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

fn settle(s: &mut Screen, ms: u64) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        s.pump();
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn shot(s: &mut Screen, out: &Path, name: &str) {
    settle(s, 250);
    let (w, h) = s.preferred_size();
    let p = s.render(w as u32, h as u32, 1.0);
    p.save_png(&out.join(format!("{name}.png"))).unwrap();
    println!("{name}");
}

/// Dove sta sullo schermo un punto (in punti) della pagina `page`.
fn at(s: &Screen, page: usize, x: f32, y: f32) -> (f32, f32) {
    let ed = s.app.ed.as_ref().expect("editor");
    let lay = ed.lay.borrow();
    let (_, r) = lay.pages.iter().find(|(p, _)| *p == page).expect("pagina in vista");
    (r.0 + x * lay.k, r.1 + y * lay.k)
}

fn click(s: &mut Screen, plat: &mut Finta, x: f32, y: f32) {
    s.pointer_move(x, y);
    s.pointer_down(x, y);
    s.pointer_up(x, y, plat);
    settle(s, 150);
}

fn drag(s: &mut Screen, plat: &mut Finta, a: (f32, f32), b: (f32, f32)) {
    s.pointer_move(a.0, a.1);
    s.pointer_down(a.0, a.1);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        s.pointer_move(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    }
    s.pointer_up(b.0, b.1, plat);
    settle(s, 150);
}

fn theme_of(name: &str) -> ThemeId {
    match name {
        "notte" => ThemeId::CedroNotte,
        "bosco" => ThemeId::CedroBosco,
        "carta" => ThemeId::CedroCarta,
        _ => ThemeId::Cedro,
    }
}

fn main() {
    cedro::i18n::fix();
    let en = std::env::var("LANG_FOTO").is_ok_and(|v| v == "en");
    cedro::i18n::set(if en { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It });
    if en {
        cedro::i18n::record_misses(true);
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto-editor".into()));
    std::fs::create_dir_all(&out).unwrap();
    let tint = std::env::args().nth(2).unwrap_or_else(|| "cedro".into());
    let dir = std::env::temp_dir().join("cedro-ui-editor");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fx = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let base = std::fs::read(fx.join("circolare.pdf")).unwrap();
    // Un PDF con qualche campo del modulo, per la scheda «Moduli».
    let fields = vec![
        Edit::NewField { page: 0, rect: Rect::new(70.0, 330.0, 200.0, 22.0), kind: FieldKind::Text, name: "Cognome".into(), options: vec![] },
        Edit::NewField { page: 0, rect: Rect::new(290.0, 330.0, 170.0, 22.0), kind: FieldKind::Text, name: "Nome".into(), options: vec![] },
        Edit::NewField { page: 0, rect: Rect::new(70.0, 370.0, 14.0, 14.0), kind: FieldKind::Check, name: "Informativa".into(), options: vec![] },
        Edit::NewField { page: 0, rect: Rect::new(70.0, 400.0, 200.0, 22.0), kind: FieldKind::List, name: "Tipo".into(), options: vec!["Dehors".into(), "Cantiere".into(), "Mercato".into()] },
    ];
    let form = apply(&base, &fields, &SaveOpts::default()).unwrap();
    let path = dir.join("Richiesta-suolo-pubblico.pdf");
    std::fs::write(&path, &base).unwrap();
    let form_path = dir.join("Modulo-occupazione.pdf");
    std::fs::write(&form_path, &form).unwrap();

    let mut plat = Finta;
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    s.request_appearance(theme_of(&tint), false);
    s.drop_files(vec![path.clone()]);
    settle(&mut s, 600);
    shot(&mut s, &out, "01-scheda-pdf");
    s.act(Act::PdfMode(PdfMode::Edit), &mut plat);
    settle(&mut s, 3000);
    shot(&mut s, &out, "02-modifica-testo-vuoto");

    // Modifica testo: clic su un paragrafo e un po' di testo.
    {
        let ed = s.app.ed.as_ref().expect("editor");
        let b = ed.fresh_text(0).and_then(|t| t.blocks.iter().find(|b| b.editable() && b.text.chars().count() > 30).cloned());
        if let Some(b) = b {
            let (x, y) = at(&s, 0, b.rect.x + 10.0, b.rect.y + 4.0);
            s.pointer_move(x, y);
            settle(&mut s, 200);
            shot(&mut s, &out, "03a-paragrafo-sotto-il-mouse");
            click(&mut s, &mut plat, x, y);
            s.text_input(" (rivisto da Luca)");
            settle(&mut s, 300);
            shot(&mut s, &out, "03-modifica-testo");
            s.key(cedro::ui::screen::Key::Escape, &mut plat);
        } else {
            println!("nessun paragrafo modificabile trovato");
        }
    }

    // Commenti: evidenzia, nota, timbro.
    s.act(Act::Ed(EdAct::Tab(EdTab::Comments)), &mut plat);
    s.act(Act::Ed(EdAct::Tool(Tool::Highlight)), &mut plat);
    settle(&mut s, 800);
    {
        let ed = s.app.ed.as_ref().expect("editor");
        let words: Vec<_> = ed.fresh_text(0).map(|t| t.words.iter().take(40).map(|w| w.rect).collect()).unwrap_or_default();
        if words.len() > 12 {
            let a = at(&s, 0, words[8].x + 1.0, words[8].y + 3.0);
            let b = at(&s, 0, words[12].right() - 1.0, words[12].y + 3.0);
            drag(&mut s, &mut plat, a, b);
        }
    }
    s.act(Act::Ed(EdAct::Tool(Tool::Note)), &mut plat);
    {
        let (x, y) = at(&s, 0, 440.0, 90.0);
        click(&mut s, &mut plat, x, y);
        s.text_input("Controllare la data prima di protocollare");
        let (x, y) = at(&s, 0, 300.0, 600.0);
        click(&mut s, &mut plat, x, y);
    }
    s.act(Act::Ed(EdAct::Tool(Tool::Stamp)), &mut plat);
    {
        let a = at(&s, 0, 330.0, 700.0);
        let b = at(&s, 0, 480.0, 740.0);
        drag(&mut s, &mut plat, a, b);
    }
    s.act(Act::Ed(EdAct::Left(1)), &mut plat);
    settle(&mut s, 600);
    shot(&mut s, &out, "04-commenti");

    // Forme e immagini.
    s.act(Act::Ed(EdAct::Tab(EdTab::Shapes)), &mut plat);
    s.act(Act::Ed(EdAct::Tool(Tool::Rect)), &mut plat);
    {
        let a = at(&s, 0, 70.0, 500.0);
        let b = at(&s, 0, 220.0, 560.0);
        drag(&mut s, &mut plat, a, b);
    }
    settle(&mut s, 600);
    shot(&mut s, &out, "04b-forme");

    // Moduli: un file con i campi.
    s.act(Act::Ed(EdAct::Leave(None)), &mut plat);
    s.act(Act::Ed(EdAct::LeaveDiscard), &mut plat);
    settle(&mut s, 300);
    s.app.ed_open(&form_path);
    settle(&mut s, 3000);
    s.act(Act::Ed(EdAct::Tab(EdTab::Forms)), &mut plat);
    s.act(Act::Ed(EdAct::Tool(Tool::Fill)), &mut plat);
    settle(&mut s, 800);
    {
        let (x, y) = at(&s, 0, 100.0, 340.0);
        click(&mut s, &mut plat, x, y);
        s.text_input("Rossi");
        let (x, y) = at(&s, 0, 77.0, 377.0);
        click(&mut s, &mut plat, x, y);
    }
    settle(&mut s, 600);
    shot(&mut s, &out, "05-moduli-compilazione");

    // Oscura: cerca e oscura.
    s.act(Act::Ed(EdAct::Tab(EdTab::Redact)), &mut plat);
    s.act(Act::Ed(EdAct::Tool(Tool::Area)), &mut plat);
    {
        let a = at(&s, 0, 70.0, 150.0);
        let b = at(&s, 0, 300.0, 180.0);
        drag(&mut s, &mut plat, a, b);
    }
    s.act(Act::Ed(EdAct::Tool(Tool::SearchRedact)), &mut plat);
    settle(&mut s, 800);
    shot(&mut s, &out, "06-oscura");

    // Trova e sostituisci.
    s.act(Act::Ed(EdAct::Tab(EdTab::Text)), &mut plat);
    s.act(Act::Ed(EdAct::Tool(Tool::Find)), &mut plat);
    if let Some(ed) = s.app.ed.as_mut() {
        ed.find = "del".into();
    }
    s.act(Act::Ed(EdAct::FindRun), &mut plat);
    settle(&mut s, 1500);
    shot(&mut s, &out, "07-trova");

    // Due pagine affiancate.
    s.act(Act::Ed(EdAct::TwoPages(true)), &mut plat);
    settle(&mut s, 800);
    shot(&mut s, &out, "08-due-pagine");

    // Il salvataggio.
    s.act(Act::Ed(EdAct::Save), &mut plat);
    settle(&mut s, 3000);
    shot(&mut s, &out, "09-salvato");
    for m in cedro::i18n::misses() {
        println!("SENZA TRADUZIONE: {m:?}");
    }
}
