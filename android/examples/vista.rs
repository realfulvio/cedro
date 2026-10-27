//! Disegna le schermate di Cedro per Android in PNG, sul computer: `cargo run -p cedro-android --example vista -- cartella-uscita`.
use std::path::PathBuf;
use std::time::Instant;

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;
use cedro_android::ui::{App, Archivio, Lavoro, Recente, Scheda, Stato};

fn recenti(app: &mut App) {
    let img = PathBuf::from("docs/hero.png");
    let t = cedro::ui::media::thumb_of(&img, 140);
    for (n, k) in [("verbale.pdf", cedro::Kind::Pdf), ("foto 1.jpg", cedro::Kind::Image), ("foto 2.jpg", cedro::Kind::Image), ("nota.m4a", cedro::Kind::Audio), ("piano.txt", cedro::Kind::Text)] {
        app.recenti.push(Recente { path: img.clone(), nome: n.into(), kind: k, thumb: if k == cedro::Kind::Image { t.clone() } else { None } });
    }
}

fn lavori(app: &mut App) {
    let l = |id, o: &str, e: &str, conv, st| Lavoro { id, origine: o.into(), etichetta: e.into(), conv, inizio: Instant::now(), quando: cedro_android::ui::adesso() - 120 * id as u64, stato: st, prog: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(if id == 1 { 430 } else { 1000 })) };
    let fatto = |n: &str, m: &str| Stato::Fatto { path: "x".into(), nome: n.into(), msg: m.into() };
    app.lavori = vec![
        l(1, "Riunione.m4a", "Word", true, Stato::InCorso),
        l(2, "condiviso.png", "JPG", true, fatto("condiviso.jpg", "282 KB → 94 KB · 66% in meno")),
        l(3, "Verbale.pdf", "Testo", true, fatto("Verbale.txt", "12 pagine")),
        l(4, "foto.png", "Comprimi", false, Stato::Errore("Non ho potuto leggere il file.".into())),
        l(5, "scansione.png", "PNG", true, fatto("scansione.png", "1,2 MB")),
    ];
}

fn scena(nome: &str, w: f32, h: f32, out: &PathBuf, f: impl FnOnce(&mut App)) {
    let mut app = App::new(ThemeId::WindowsEssenziale);
    let img = PathBuf::from("docs/hero.png");
    let file = Archivio { path: img.clone(), nome: "Verbale_giunta.png".into(), dim: 2_400_000 };
    app.imposta_file(file, cedro::Kind::Image, false);
    app.miniatura = cedro::ui::media::thumb_of(&img, 600);
    recenti(&mut app);
    f(&mut app);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::current(), &mut cache);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/vista".into()));
    std::fs::create_dir_all(&out).unwrap();
    let (pw, ph, lw, lh) = (392.0, 800.0, 800.0, 360.0);
    scena("foglio", pw, ph, &out, |a| a.foglio = true);
    scena("ruota", pw, ph, &out, |_| {});
    scena("ruota-strumenti", pw, ph, &out, |a| {
        a.modo = 1;
        a.scelta = Some(0);
    });
    scena("ruota-vuota", pw, ph, &out, |a| {
        a.file = None;
        a.miniatura = None;
    });
    scena("scegli", pw, ph, &out, |a| a.scelta_file = true);
    scena("lavori", pw, ph, &out, |a| {
        lavori(a);
        a.scheda = Scheda::Lavori;
    });
    scena("aspetto", pw, ph, &out, |a| a.scheda = Scheda::Aspetto);
    scena("scuro-ruota", pw, ph, &out, |a| a.tema = ThemeId::BancoDiLavoro);
    scena("orizz-ruota", lw, lh, &out, |_| {});
    scena("orizz-foglio", lw, lh, &out, |a| a.foglio = true);
    scena("orizz-lavori", lw, lh, &out, |a| {
        lavori(a);
        a.scheda = Scheda::Lavori;
    });
}
