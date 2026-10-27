//! Fotografie della nuova interfaccia senza schermo: tutti i temi nelle due modalità, con file
//! di prova. `cargo run --example ui_foto -- <cartella>` scrive i PNG nella cartella.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::model::Activity;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

#[allow(dead_code)]
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

fn demo_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let mut out = Vec::new();
    for (i, name) in ["Riunione-giunta-2026.jpg", "Sopralluogo-via-Roma.jpg", "Festa-del-paese.jpg"].iter().enumerate() {
        let p = dir.join(name);
        let (w, h) = (1600 + i as u32 * 320, 1200);
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            let t = x as f32 / w as f32;
            let s = y as f32 / h as f32;
            match i {
                0 => image::Rgb([(60.0 + 120.0 * t) as u8, (110.0 + 80.0 * s) as u8, 170]),
                1 => image::Rgb([(200.0 * s) as u8 + 40, 150, (90.0 + 100.0 * t) as u8]),
                _ => image::Rgb([230, (120.0 + 100.0 * t) as u8, (60.0 + 120.0 * s) as u8]),
            }
        });
        img.save(&p).unwrap();
        out.push(p);
    }
    let pdf = dir.join("Circolare-uffici.pdf");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/circolare.pdf"), &pdf).unwrap();
    out.push(pdf);
    out
}

fn settle(s: &mut Screen) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(2500) {
        s.pump();
        let ready = s.app.files.iter().all(|f| s.app.media_of(&f.path).is_some_and(|m| m.thumb.is_some()));
        if ready && t.elapsed() > Duration::from_millis(300) {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn main() {
    // LANG_FOTO=en: le foto in inglese, e a fine corsa l'elenco dei testi senza traduzione.
    if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
        cedro::i18n::set(cedro::i18n::Lang::En);
        cedro::i18n::record_misses(true);
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::args().nth(2);
    let files = demo_files(&std::env::temp_dir().join("cedro-ui-demo"));
    let images: Vec<PathBuf> = files.iter().filter(|p| p.extension().is_some_and(|e| e == "jpg")).cloned().collect();
    for theme in THEMES {
        if only.as_deref().is_some_and(|o| o != theme.key()) {
            continue;
        }
        for simple in [false, true] {
            let surfaces: &[Surface] = if theme == ThemeId::CedroCompatto { &[Surface::Main, Surface::Compact] } else { &[Surface::Main] };
            for (act, surface) in [Activity::Convert, Activity::Transcribe, Activity::YouTube].into_iter().flat_map(|a| surfaces.iter().map(move |s| (a, *s))) {
                let mut s = Screen::new(surface, Arc::new(|| {}));
                s.request_appearance(theme, simple);
                s.drop_files(vec![files[3].clone()]);
                s.drop_files(images.clone());
                s.app.activity = act;
                if surface == Surface::Compact {
                    s.ui.compact_tab = match act {
                        Activity::Convert => 0,
                        Activity::Transcribe => 2,
                        _ => 1,
                    };
                }
                if act == Activity::YouTube {
                    s.app.youtube.add("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                }
                if let Some(st) = std::env::var("STEP").ok().and_then(|v| v.parse().ok()) {
                    s.ui.step = st;
                }
                settle(&mut s);
                let (mut w, mut h) = s.preferred_size();
                if let Some((sw, sh)) = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))) {
                    (w, h) = (sw, sh);
                }
                let name = format!("{}{}-{}-{}{}.png", theme.key(), if surface == Surface::Compact { "-popup" } else { "" }, if simple { "sempl" } else { "std" }, format!("{act:?}").to_lowercase(), std::env::var("STEP").map(|v| format!("-{v}")).unwrap_or_default());
                let p = s.render(w as u32, h as u32, 1.0);
                p.save_png(&out.join(&name)).unwrap();
                println!("{name}");
            }
        }
    }
    let _ = ThemeId::WindowsEssenziale;
    for m in cedro::i18n::misses() {
        println!("SENZA TRADUZIONE: {m:?}");
    }
}
