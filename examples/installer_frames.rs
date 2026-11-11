//! Le miniature della pagina «Come lavori con Cedro?» dell'installatore di Windows: tre scene per aspetto
//! (Standard e Cedro 4 Labs), fotografate dalle finestre vere di Cedro e ridotte a 300×200 in BMP.
//! `cargo run --example installer_frames -- installer/img` (ripetere quando cambia l'aspetto dei due temi).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::model::Activity;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::ThemeId;

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


/// Riduce una foto della finestra a una miniatura e la scrive come BMP a 24 bit (l'installatore di Windows
/// legge i BMP).
fn miniatura(png: &Path, bmp: &Path, w: u32, h: u32) {
    let img = image::open(png).unwrap().to_rgb8();
    let small = image::imageops::resize(&img, w, h, image::imageops::FilterType::Lanczos3);
    small.save_with_format(bmp, image::ImageFormat::Bmp).unwrap();
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "installer/img".into()));
    std::fs::create_dir_all(&out).unwrap();
    let tmp = std::env::temp_dir().join("cedro-installer-frames");
    std::fs::create_dir_all(&tmp).unwrap();
    let files = demo_files(&std::env::temp_dir().join("cedro-ui-demo"));
    let images: Vec<PathBuf> = files.iter().filter(|p| p.extension().is_some_and(|e| e == "jpg")).cloned().collect();
    // Tre scene per aspetto: le attività che l'aspetto mostra davvero (Standard ha anche YouTube, Labs no).
    for (nome, theme, scene) in [
        ("std", ThemeId::WindowsEssenziale, [Activity::Convert, Activity::Transcribe, Activity::YouTube]),
        ("labs", ThemeId::CedroLabs, [Activity::Convert, Activity::Transcribe, Activity::Firma]),
    ] {
        for (i, act) in scene.into_iter().enumerate() {
            let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
            s.request_appearance(theme, false);
            s.drop_files(vec![files[3].clone()]);
            s.drop_files(images.clone());
            s.app.activity = act;
            if act == Activity::YouTube {
                s.app.youtube.add("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
            }
            settle(&mut s);
            let (w, h) = s.preferred_size();
            let png = tmp.join(format!("{nome}-{}.png", i + 1));
            s.render(w as u32, h as u32, 1.0).save_png(&png).unwrap();
            let bmp = out.join(format!("{nome}-{}.bmp", i + 1));
            miniatura(&png, &bmp, 300, 200);
            println!("{}", bmp.display());
        }
    }
}
