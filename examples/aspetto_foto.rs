//! Fotografia della pagina Aspetto delle Impostazioni: `cargo run --example aspetto_foto -- <cartella>`.
use std::sync::Arc;
use std::time::Duration;

use cedro::ui::aspetto::Aspetto;
use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;

fn main() {
    if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
        cedro::i18n::set(cedro::i18n::Lang::En);
    }
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let mut a = Aspetto::new(ThemeId::PercorsoGuidato, false, Arc::new(|| {}));
    std::thread::sleep(Duration::from_secs(14));
    let mut cache = GlyphCache::new();
    for (name, w, h) in [("largo", 560u32, 520u32), ("stretto", 420, 520)] {
        let mut p = Painter::new(w, h, 1.0, Fonts::shared(), &mut cache);
        a.draw(&mut p, w as f32, h as f32);
        p.save_png(&out.join(format!("aspetto-{name}.png"))).unwrap();
    }
    a.simple = true;
    let mut p = Painter::new(560, 700, 1.0, Fonts::shared(), &mut cache);
    a.draw(&mut p, 560.0, 700.0);
    p.save_png(&out.join("aspetto-sempl.png")).unwrap();
}
