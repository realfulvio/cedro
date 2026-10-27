//! Disegna le schermate di Cedro per Android in PNG, sul computer: `cargo run -p cedro-android --example vista -- cartella-uscita`.
use std::path::PathBuf;

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;
use cedro_android::ui::{App, Archivio, Lavoro, Schermo};

fn scena(nome: &str, w: f32, h: f32, out: &PathBuf, f: impl FnOnce(&mut App)) {
    let mut app = App::new(ThemeId::WindowsEssenziale);
    let img = PathBuf::from("docs/hero.png");
    let file = Archivio { path: img.clone(), nome: "Verbale_giunta.png".into(), dim: 2_400_000 };
    app.imposta_file(file, cedro::Kind::Image, false);
    app.miniatura = cedro::ui::media::thumb_of(&img, 600);
    f(&mut app);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::current(), &mut cache);
    // Il foglio sale: nei fotogrammi fermi lo mostriamo arrivato.
    app.apertura -= std::time::Duration::from_secs(2);
    app.apertura_aspetto -= std::time::Duration::from_secs(2);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/vista".into()));
    std::fs::create_dir_all(&out).unwrap();
    let (pw, ph, lw, lh) = (392.0, 800.0, 800.0, 360.0);
    scena("foglio", pw, ph, &out, |a| a.schermo = Schermo::Foglio);
    scena("ruota", pw, ph, &out, |_| {});
    scena("ruota-scelta", pw, ph, &out, |a| a.scelta = Some(1));
    scena("ruota-strumenti", pw, ph, &out, |a| {
        a.modo = 1;
        a.scelta = Some(0);
    });
    scena("ruota-fatto", pw, ph, &out, |a| a.lavoro = Lavoro::Fatto { nome: "Verbale_giunta.jpg".into(), msg: "276 KB → 94 KB · 66% in meno".into() });
    scena("ruota-vuota", pw, ph, &out, |a| {
        a.file = None;
        a.miniatura = None;
    });
    scena("aspetto", pw, ph, &out, |a| a.aspetto = true);
    scena("foglio-fatto", pw, ph, &out, |a| {
        a.schermo = Schermo::Foglio;
        a.lavoro = Lavoro::Fatto { nome: "Verbale_giunta.jpg".into(), msg: "276 KB → 94 KB · 66% in meno".into() };
    });
    scena("scuro-ruota", pw, ph, &out, |a| {
        a.tema = ThemeId::BancoDiLavoro;
        a.scelta = Some(0);
    });
    scena("orizz-ruota", lw, lh, &out, |a| a.scelta = Some(1));
    scena("orizz-foglio", lw, lh, &out, |a| a.schermo = Schermo::Foglio);
}
