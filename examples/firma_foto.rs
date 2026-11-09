//! Fotografie della «Firma digitale» senza schermo: la pagina della sezione nei temi (vuota e con la
//! verifica di più file) e il visualizzatore sui file firmati di prova (tests/data/firma).
//! `cargo run --example firma_foto -- <cartella>` scrive tutto nella cartella.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::firma::trust;
use cedro::ui::model::Activity;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::THEMES;

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

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/firma")
}

/// Fa girare lo schermo finché le verifiche e le anteprime sono arrivate.
fn settle(s: &mut Screen, w: f32, h: f32) {
    let start = Instant::now();
    loop {
        s.pump();
        let _ = s.render(w as u32, h as u32, 1.0);
        let waiting = s.app.firma.busy() || s.app.previews.values().any(|v| v.is_none()) || s.app.page_thumbs.values().any(|v| v.is_none());
        if (!waiting && start.elapsed() > Duration::from_millis(600)) || start.elapsed() > Duration::from_secs(20) {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn main() {
    cedro::i18n::fix();
    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It });
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    // L'elenco dei certificatori di prova (la PKI finta «Cedro Test»), aggiornato adesso.
    let xml = std::fs::read_to_string(data().join("tl.xml")).unwrap();
    // Chiusa una finestra Cedro scorda l'elenco: lo si rimette prima di ognuna.
    let tl = || trust::set_store(trust::Store { entries: trust::parse_tl(&xml, "IT").unwrap(), updated: Some(cedro::firma::now()) });
    let only = std::env::var("SOLO").ok();
    let shot = |s: &mut Screen, name: &str, w: f32, h: f32| {
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            return;
        }
        settle(s, w, h);
        let p = s.render(w as u32, h as u32, 1.0);
        p.save_png(&out.join(name)).unwrap();
        println!("{name}");
    };
    let files: Vec<PathBuf> = ["determina_due_firme.pdf", "paolo.txt.p7m", "doppia.txt.p7m.p7m", "mario.txt.p7m.tsd", "cambiato.txt.p7m", "controfirma.txt.p7m"].iter().map(|n| data().join(n)).collect();
    for theme in THEMES {
        let key = theme.key();
        tl();
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(theme, false);
        s.app.activity = Activity::Firma;
        shot(&mut s, &format!("pagina-{key}.png"), 1536.0, 1024.0);
        s.drop_files(files.clone());
        shot(&mut s, &format!("cartella-{key}.png"), 1536.0, 1024.0);
    }
    // Il visualizzatore.
    let views = [
        ("vw-due-firme", "determina_due_firme.pdf"),
        ("vw-cambiato-pdf", "determina_cambiata.pdf"),
        ("vw-p7m", "mario.txt.p7m"),
        ("vw-revocato", "paolo.txt.p7m"),
        ("vw-busta-doppia", "doppia.txt.p7m.p7m"),
        ("vw-tsd", "mario.txt.p7m.tsd"),
        ("vw-p7s", "documento.txt.p7s"),
        ("vw-controfirma", "controfirma.txt.p7m"),
        ("vw-non-firmato", "tl.xml"),
        ("vw-tsr", "mario.txt.p7m.tsr"),
    ];
    for (name, file) in views {
        tl();
        let mut s = Screen::new(Surface::Viewer, Arc::new(|| {}));
        s.open_viewer(data().join(file));
        let (w, h) = s.preferred_size();
        shot(&mut s, &format!("{name}.png"), w, h);
        if name == "vw-cambiato-pdf" {
            // La versione firmata.
            s.act(cedro::ui::core::Act::VwRevision(0), &mut Finta);
            shot(&mut s, "vw-cambiato-versione-firmata.png", w, h);
        }
        if name == "vw-due-firme" {
            s.act(cedro::ui::core::Act::VwSigner(1), &mut Finta);
            s.act(cedro::ui::core::Act::VwTech(0), &mut Finta);
            shot(&mut s, "vw-due-firme-aperte.png", w, h);
            s.act(cedro::ui::core::Act::VwAt(2), &mut Finta);
            s.act(cedro::ui::core::Act::VwPanel, &mut Finta);
            shot(&mut s, "vw-due-firme-senza-pannello.png", w, h);
        }
        if name == "vw-p7m" {
            shot(&mut s, "vw-p7m-piccolo.png", 1000.0, 700.0);
        }
    }
}
