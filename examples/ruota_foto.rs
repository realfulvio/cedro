//! Fotografie della ruota nuova: `cargo run --example ruota_foto -- <cartella>`.
use cedro::Kind;
use cedro::ui::gfx::{Fonts, GlyphCache, Image, Painter};
use cedro::ui::wheel::WheelUi;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let mut cache = GlyphCache::new();
    let img = image::RgbaImage::from_fn(400, 400, |x, y| image::Rgba([(240.0 - y as f32 * 0.4) as u8, (110.0 + x as f32 * 0.1) as u8, (60.0 + y as f32 * 0.3) as u8, 255]));
    let thumb = Image::from_rgba(400, 400, img.as_raw());
    for (name, simple, hover, mode) in [("std", false, 0, 0), ("std-hover", false, 3, 0), ("sempl", true, 0, 0), ("sempl-hover", true, 0, 0), ("std-tools", false, 1, 1)] {
        let mut ui = WheelUi::new(Kind::Image, mode, simple);
        ui.name = "tramonto.png".into();
        ui.has_file = true;
        ui.thumb = thumb.clone();
        ui.beside = cedro::ui::wheel::Beside::On;
        ui.hover = if name.ends_with("hover") || name == "std-tools" { hover } else { -1 };
        if simple && name.ends_with("hover") {
            ui.hover = 0;
        }
        let (w, h) = ui.size();
        let s = 0.85;
        let mut p = Painter::new((w * s) as u32, (h * s) as u32, s, Fonts::shared(), &mut cache);
        p.clear((200, 215, 235));
        ui.draw(&mut p);
        p.save_png(&out.join(format!("ruota-{name}.png"))).unwrap();
    }
}
