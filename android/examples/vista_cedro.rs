//! Le schermate del nuovo Cedro per Android (quattro tinte) in PNG, sul computer:
//! `cargo run -p cedro-android --example vista_cedro -- cartella-uscita` (dalla radice del repository).
use std::path::PathBuf;
use std::time::Instant;

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;
use cedro_android::ui::{App, Lavoro, Recente, Scheda, Stato};

fn foto(nome: &str, tema: ThemeId, out: &PathBuf, f: impl FnOnce(&mut App)) {
    let mut app = App::new(tema);
    app.anim.calmo = true;
    app.vai_casa();
    let img = PathBuf::from("docs/hero.png");
    let t = cedro::ui::media::thumb_of(&img, 140);
    let m = cedro::ui::media::thumb_of(&img, 600);
    for (n, k) in [("verbale.pdf", cedro::Kind::Pdf), ("foto 1.jpg", cedro::Kind::Image), ("foto 2.jpg", cedro::Kind::Image), ("nota.m4a", cedro::Kind::Audio)] {
        app.recenti.push(Recente { path: img.clone(), nome: n.into(), kind: k, thumb: if k == cedro::Kind::Image { t.clone() } else { None } });
    }
    f(&mut app);
    let (w, h) = (392.0, 800.0);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::for_theme(tema), &mut cache);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

fn coda(a: &mut App, n: usize, kind: cedro::Kind, nome: &str) {
    let img = PathBuf::from("docs/hero.png");
    let v: Vec<_> = (0..n).map(|i| (img.clone(), format!("{nome} {}", i + 1), kind)).collect();
    a.aggiungi_coda(v);
    let m = cedro::ui::media::thumb_of(&img, 600);
    a.info_voce(&img, m, Some((4032, 3024)));
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/vista-cedro".into()));
    std::fs::create_dir_all(&out).unwrap();
    for (k, t) in [ThemeId::Cedro, ThemeId::CedroNotte, ThemeId::CedroBosco, ThemeId::CedroCarta].into_iter().enumerate() {
        foto(&format!("{k}-inizio"), t, &out, |_| {});
        foto(&format!("{k}-converti-semplice"), t, &out, |a| {
            coda(a, 3, cedro::Kind::Image, "vacanza");
            a.vista_nav(1);
        });
        foto(&format!("{k}-converti-completa"), t, &out, |a| {
            coda(a, 3, cedro::Kind::Image, "vacanza");
            a.vista_nav(1);
            a.vista_completa(true);
        });
        foto(&format!("{k}-converti-vuota"), t, &out, |a| a.vista_nav(1));
        foto(&format!("{k}-pdf"), t, &out, |a| {
            coda(a, 2, cedro::Kind::Pdf, "delibera");
            a.vista_nav(2);
        });
        foto(&format!("{k}-trascrivi"), t, &out, |a| {
            coda(a, 2, cedro::Kind::Audio, "consiglio");
            a.vista_nav(3);
            a.vista_completa(true);
        });
        foto(&format!("{k}-video"), t, &out, |a| {
            a.vista_nav(4);
            a.incolla("https://www.youtube.com/watch?v=k3ZqAbCdEfG".into());
        });
        foto(&format!("{k}-firma"), t, &out, |a| a.vista_nav(5));
        foto(&format!("{k}-lavori"), t, &out, |a| {
            let l = |id, o: &str, e: &str, st| Lavoro { id, origine: o.into(), etichetta: e.into(), conv: true, inizio: Instant::now(), quando: cedro_android::ui::adesso() - 120 * id as u64, stato: st, prog: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(if id == 1 { 430 } else { 1000 })), sorgente: None, controlli: None };
            a.lavori = vec![l(1, "Riunione.m4a", "Word", Stato::InCorso), l(2, "condiviso.png", "JPG", Stato::Fatto { path: "x".into(), nome: "condiviso.jpg".into(), msg: "282 KB → 94 KB".into() })];
            a.sopra = Some(Scheda::Lavori);
        });
        foto(&format!("{k}-aspetto"), t, &out, |a| a.sopra = Some(Scheda::Aspetto));
    }
}
