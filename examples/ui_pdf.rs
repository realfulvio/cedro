//! Fotografie della scheda PDF con «Pagine» e «Comprimi», senza schermo.
//! `cargo run --example ui_pdf -- <cartella>`; LANG_FOTO=en per l'inglese.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::core::Act;
use cedro::ui::model::PdfMode;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

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

fn demo(dir: &Path) -> Vec<PathBuf> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let fx = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let col = dir.join("colonne.pdf");
    std::fs::copy(fx.join("colonne.pdf"), &col).unwrap();
    let circ = dir.join("circolare.pdf");
    std::fs::copy(fx.join("circolare.pdf"), &circ).unwrap();
    let scan = dir.join("rubrica.jpg");
    std::fs::copy(fx.join("scan.jpg"), &scan).unwrap();
    let rub = cedro::perform_into(&scan, "pdf", 92, Some(dir)).unwrap().path;
    std::fs::remove_file(&scan).unwrap();
    let all = cedro::merge_paths(&[col.clone(), circ.clone(), rub.clone()], Some(dir)).unwrap().path;
    let news = dir.join("Notiziario-completo.pdf");
    std::fs::rename(&all, &news).unwrap();
    vec![news, col, circ, rub]
}

fn settle(s: &mut Screen, ms: u64) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        s.pump();
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn shot(s: &mut Screen, out: &Path, name: &str) {
    let (w, h) = s.preferred_size();
    let (w, h) = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))).unwrap_or((w, h));
    let p = s.render(w as u32, h as u32, 1.0);
    p.save_png(&out.join(name)).unwrap();
    println!("{name}");
}

fn main() {
    if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
        cedro::i18n::set(cedro::i18n::Lang::En);
        cedro::i18n::record_misses(true);
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::args().nth(2);
    let files = demo(&std::env::temp_dir().join("cedro-ui-pdf"));
    let mut plat = Finta;
    for theme in THEMES {
        if only.as_deref().is_some_and(|o| o != theme.key()) {
            continue;
        }
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(theme, false);
        s.drop_files(files.clone());
        settle(&mut s, 800);
        // Pagine: due scelte, una tolta, una girata.
        s.act(Act::PdfMode(PdfMode::Pages), &mut plat);
        settle(&mut s, 1500);
        let first = cedro::ui::views::split_file(&s.app).unwrap();
        s.app.page_click(first, 1);
        s.app.pages_remove(first);
        s.app.page_click(first, 1);
        s.app.page_click(first, 4);
        s.app.pages_rotate(first, 1);
        s.app.page_click(first, 3);
        settle(&mut s, 300);
        shot(&mut s, &out, &format!("{}-pagine.png", theme.key()));
        // Numeri e filigrana accesi.
        for opt in [cedro::conversioni::Opt::PageNumbers, cedro::conversioni::Opt::Watermark] {
            s.act(Act::OptSwitch(opt), &mut plat);
        }
        settle(&mut s, 2500);
        shot(&mut s, &out, &format!("{}-timbri.png", theme.key()));
        // Comprimi.
        s.act(Act::PdfMode(PdfMode::Compress), &mut plat);
        settle(&mut s, 4000);
        shot(&mut s, &out, &format!("{}-comprimi.png", theme.key()));
        // Semplificata: «Più piccolo».
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(theme, true);
        s.drop_files(vec![files[3].clone()]);
        settle(&mut s, 500);
        s.act(Act::Action("compress"), &mut plat);
        settle(&mut s, 2500);
        shot(&mut s, &out, &format!("{}-semplice.png", theme.key()));
    }
    let _ = ThemeId::WindowsEssenziale;
    for m in cedro::i18n::misses() {
        println!("SENZA TRADUZIONE: {m:?}");
    }
}
