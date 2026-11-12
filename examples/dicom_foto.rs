//! Fotografie del riquadro «DICOM» nelle Conversioni, in tutti i temi:
//! `cargo run --example dicom_foto -- <cartella> <file.dcm>…`.
use std::sync::Arc;
use std::time::Duration;

use cedro::ui::screen::{Screen, Surface};
use cedro::ui::theme::THEMES;

fn main() {
    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It });
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().unwrap_or_else(|| "foto".into()));
    let files: Vec<std::path::PathBuf> = args.map(Into::into).collect();
    std::fs::create_dir_all(&out).unwrap();
    let action = std::env::var("VOCE").unwrap_or_else(|_| "png".into());
    for theme in THEMES {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.theme = theme;
        s.simple = false;
        s.drop_files(files.clone());
        s.app.set_action(&action);
        let (w, h) = s.preferred_size();
        // Le informazioni dei file arrivano da un thread: qualche fotogramma per lasciarle arrivare.
        for _ in 0..30 {
            s.app.pump();
            let _ = s.render(w as u32, h as u32, 1.0);
            std::thread::sleep(Duration::from_millis(100));
        }
        let p = s.render(w as u32, h as u32, 1.0);
        p.save_png(&out.join(format!("dicom-{}-{action}.png", theme.key()))).unwrap();
    }
}
