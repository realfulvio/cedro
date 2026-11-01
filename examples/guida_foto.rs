//! Fotografie della Guida: `cargo run --example guida_foto -- <cartella>` (un PNG per tema e argomento).
use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::guida::{Guida, H, PAGE_COUNT, W};
use cedro::ui::theme::THEMES;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let only: Option<usize> = std::env::var("TEMA").ok().and_then(|v| v.parse().ok());
    let mut cache = GlyphCache::new();
    for (ti, theme) in THEMES.into_iter().enumerate() {
        if only.is_some_and(|o| o != ti) {
            continue;
        }
        for page in 0..PAGE_COUNT {
            let mut g = Guida::new(theme, 0);
            g.key(cedro::ui::screen::Key::Escape, 0);
            for _ in 0..page {
                g.key(cedro::ui::screen::Key::Right, 0);
            }
            let mut p = Painter::new(W as u32, H as u32, 1.0, Fonts::for_theme(theme), &mut cache);
            g.draw(&mut p, 6200);
            p.save_png(&out.join(format!("{}-{}.png", theme.key(), page + 1))).unwrap();
        }
    }
}
