//! Cedro 4 Labs senza schermo: le schermate della finestra unica (elenco, comando, caso, procedure, vuoto,
//! scorciatoie, lavoro fatto). `HOME=/tmp/labs-home cargo run --example labs_foto -- <cartella>` scrive i PNG.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::core::Act;
use cedro::ui::labsx::Pannello;
use cedro::ui::model::Lab;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::ThemeId;

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

fn demo(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let mut out = Vec::new();
    for (i, name) in ["macro_0457_01.jpg", "macro_0457_02.jpg", "blot_actina_2026.jpg", "fig2_confocale.jpg"].iter().enumerate() {
        let p = dir.join(name);
        let (w, h) = (1600 + i as u32 * 160, 1200);
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            let (t, s) = (x as f32 / w as f32, y as f32 / h as f32);
            image::Rgb([(60.0 + 120.0 * t) as u8, (110.0 + 80.0 * s) as u8, (90 + i * 30) as u8])
        });
        img.save(&p).unwrap();
        out.push(p);
    }
    let pdf = dir.join("referto_0457_bozza.pdf");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/circolare.pdf"), &pdf).unwrap();
    out.push(pdf);
    let csv = dir.join("elisa_piastra1.csv");
    std::fs::write(&csv, "pozzetto,od\nA1,0.12\nA2,0.34\nA3,0.56\n").unwrap();
    out.push(csv);
    let wav = dir.join("dettato_0457.wav");
    let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&wav, spec).unwrap();
    for n in 0..16000 * 3 {
        let t = n as f32 / 16000.0;
        w.write_sample(((t * 220.0 * std::f32::consts::TAU).sin() * 9000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
    out.push(wav);
    out
}

fn settle(s: &mut Screen) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(2500) {
        s.pump();
        let ready = s.app.files.iter().all(|f| s.app.media_of(&f.path).is_some_and(|m| m.thumb.is_some() || m.about.is_some() || m.dati.is_some()));
        if ready && t.elapsed() > Duration::from_millis(300) {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn foto(s: &mut Screen, out: &Path, name: &str, size: (u32, u32)) {
    settle(s);
    s.render(size.0, size.1, 1.0);
    settle(s);
    s.render(size.0, size.1, 1.0).save_png(&out.join(name)).unwrap();
    println!("{name}");
}

fn main() {
    // LANG_FOTO=en|es|de|fr|pt: le foto in quella lingua (per en, a fine corsa l'elenco dei testi senza traduzione).
    let lingua = std::env::var("LANG_FOTO").ok();
    let inglese = lingua.as_deref() == Some("en");
    match lingua.as_deref() {
        Some("en") => {
            cedro::i18n::set(cedro::i18n::Lang::En);
            cedro::i18n::record_misses(true);
        }
        Some(altra @ ("es" | "de" | "fr" | "pt")) => {
            cedro::i18n::set(match altra {
                "es" => cedro::i18n::Lang::Es,
                "de" => cedro::i18n::Lang::De,
                "fr" => cedro::i18n::Lang::Fr,
                _ => cedro::i18n::Lang::Pt,
            });
            cedro::i18n::fix();
        }
        _ => {
            cedro::i18n::set(cedro::i18n::Lang::It);
            cedro::i18n::fix();
        }
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let size = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1440, 900));
    let files = demo(&std::env::temp_dir().join("cedro-labs-demo"));
    let mut p = Finta;
    let nuova = |files: &[PathBuf]| {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(ThemeId::CedroLabs, true);
        s.drop_files(files.to_vec());
        s
    };

    // Coda vuota.
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    s.request_appearance(ThemeId::CedroLabs, true);
    foto(&mut s, &out, "01-vuoto.png", size);

    // L'elenco con due casi e tre file scelti.
    let mut s = nuova(&files);
    settle(&mut s);
    for (k, f) in s.app.files.clone().iter().enumerate() {
        s.app.lx.caso_di.insert(f.path.clone(), if k < 2 { "V-2026-0457".into() } else if k == 2 { "MIC-2026-22".into() } else { String::new() });
    }
    s.app.lx_ricorda_caso("V-2026-0457", "Cane, meticcio · mammella");
    s.app.lx_ricorda_caso("MIC-2026-22", "Progetto TK-2 · confocale");
    for i in [0, 1, 2] {
        s.app.lx_clic_riga(i, false, true);
    }
    foto(&mut s, &out, "02-elenco.png", size);

    // Il comando «Togli i nomi» aperto.
    s.act(Act::Lab(Lab::Anonimizza), &mut p);
    foto(&mut s, &out, "03-comando.png", size);

    // Lo stesso, nella vista completa.
    s.act(Act::SetSimple(false), &mut p);
    foto(&mut s, &out, "03b-completa.png", size);
    s.act(Act::SetSimple(true), &mut p);

    // Il lavoro: parte, finisce, e sulle righe c'è «Già fatto».
    s.act(Act::Start, &mut p);
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(40) {
        s.pump();
        if !s.app.busy() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    s.act(Act::LxRegistro, &mut p);
    foto(&mut s, &out, "04-fatto.png", size);
    s.act(Act::LxRegistro, &mut p);

    // Un caso a sinistra: la fascia con le tappe.
    s.act(Act::LxFiltroCaso(Some("V-2026-0457".into())), &mut p);
    foto(&mut s, &out, "05-caso.png", size);
    s.act(Act::LxScheda, &mut p);
    foto(&mut s, &out, "05b-scheda.png", size);
    s.act(Act::LxScheda, &mut p);
    s.act(Act::LxFiltroCaso(None), &mut p);

    // Scelto tutto: il pannello dice quanti file lavora ogni comando.
    s.act(Act::LxSceglitutti, &mut p);
    s.act(Act::LxPannello(Pannello::Dettagli), &mut p);
    foto(&mut s, &out, "06-tutto.png", size);
    s.act(Act::Lab(Lab::Immagini), &mut p);
    foto(&mut s, &out, "07-restano-fuori.png", size);

    // «Salva come procedura» e il comando Procedure.
    s.act(Act::LxProcSalva, &mut p);
    foto(&mut s, &out, "08-salva-procedura.png", size);
    s.act(Act::LxProcNomeOk, &mut p);
    s.act(Act::LxProcedure, &mut p);
    foto(&mut s, &out, "09-procedure.png", size);

    // Una pagina intera, con «Torna all'elenco».
    s.act(Act::Lab(Lab::Trascrivi), &mut p);
    foto(&mut s, &out, "10-pagina.png", size);
    s.act(Act::LxPannello(Pannello::Dettagli), &mut p);

    s.act(Act::LxAiuto, &mut p);
    foto(&mut s, &out, "11-scorciatoie.png", size);
    s.act(Act::LxAiuto, &mut p);

    // Un errore spiegato.
    s.app.files[3].state = cedro::ui::model::FileState::Failed;
    s.app.files[3].message = "Il file non si apre: sembra danneggiato.".into();
    s.act(Act::LxErrore(3), &mut p);
    foto(&mut s, &out, "12-errore.png", size);
    if inglese {
        for m in cedro::i18n::misses() {
            println!("SENZA TRADUZIONE: {m:?}");
        }
    }
}
