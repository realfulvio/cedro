//! Prova dal vero della trascrizione nell'interfaccia, senza schermo: trascrive un file vero col
//! motore vero e fotografa la finestra mentre lavora (riquadro dell'avanzamento, spia, tempo che resta).
//! `cargo run --release --example trascrivi_vero -- <audio> <cartella> [tema]`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::model::Activity;
use cedro::ui::screen::{Screen, Surface};
use cedro::ui::theme::THEMES;

fn main() {
    cedro::i18n::fix();
    cedro::i18n::set(cedro::i18n::Lang::It);
    let audio = PathBuf::from(std::env::args().nth(1).expect("manca il file audio"));
    let out = PathBuf::from(std::env::args().nth(2).unwrap_or_else(|| "foto-vere".into()));
    let theme = std::env::args().nth(3);
    std::fs::create_dir_all(&out).unwrap();
    let theme = THEMES.iter().copied().find(|t| theme.as_deref().is_none_or(|k| k == t.key())).unwrap_or(THEMES[0]);
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    s.request_appearance(theme, false);
    cedro::i18n::set(cedro::i18n::Lang::It);
    s.app.activity = Activity::Transcribe;
    s.drop_files(vec![audio]);
    s.pump();
    s.app.start_transcribe();
    let t0 = Instant::now();
    let mut next_shot = 0u64;
    let mut n = 0;
    while s.app.busy() {
        s.pump();
        let secs = t0.elapsed().as_secs();
        if secs >= next_shot {
            let (w, h) = s.preferred_size();
            let name = format!("{:02}-{}s-{}pc.png", n, secs, s.app.transcribe.percent);
            s.render(w as u32, h as u32, 1.0).save_png(&out.join(&name)).unwrap();
            let spia = s.app.spia().map(|u| format!("{} {}", if u.gpu { "GPU" } else { "CPU" }, u.detail)).unwrap_or_default();
            println!("{secs:>4}s {:>3}% resta={:?} spia={spia:?} barra={:?}", s.app.transcribe.percent, s.app.transcribe_left(), s.app.progress());
            n += 1;
            next_shot = secs + if secs < 20 { 3 } else { 10 };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    s.pump();
    let (w, h) = s.preferred_size();
    s.render(w as u32, h as u32, 1.0).save_png(&out.join("99-fine.png")).unwrap();
    println!("fine in {} s: {:?}", t0.elapsed().as_secs(), s.app.files.first().map(|f| f.message.clone()));
}
