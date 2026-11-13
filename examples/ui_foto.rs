//! Fotografie della nuova interfaccia senza schermo: tutti i temi nelle due modalità, con file
//! di prova. `cargo run --example ui_foto -- <cartella>` scrive i PNG nella cartella.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::model::Activity;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

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

fn main() {
    // LANG_FOTO=en: le foto in inglese, e a fine corsa l'elenco dei testi senza traduzione.
    if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
        cedro::i18n::set(cedro::i18n::Lang::En);
        cedro::i18n::record_misses(true);
    } else if matches!(std::env::args().nth(2).as_deref(), Some("cedro-labs") | Some("cedro-caso")) {
        cedro::i18n::set(cedro::i18n::Lang::It);
        cedro::i18n::fix();
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::args().nth(2);
    let files = demo_files(&std::env::temp_dir().join("cedro-ui-demo"));
    let images: Vec<PathBuf> = files.iter().filter(|p| p.extension().is_some_and(|e| e == "jpg")).cloned().collect();
    // Il Cedro di partenza: una foto per pagina nelle due viste, Inizio, l'elenco e (TINTE=1) le quattro tinte.
    if only.as_deref().is_some_and(|o| o.starts_with("cedro") && o != "cedro-labs" && o != "cedro-caso" && o != "cedro-compatto") {
        use cedro::ui::model::Lab;
        let size = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1536, 1024));
        let wav = std::env::temp_dir().join("cedro-ui-demo").join("Consiglio-comunale-12-ottobre.wav");
        {
            let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::create(&wav, spec).unwrap();
            for n in 0..16000 * 6 {
                let t = n as f32 / 16000.0;
                let env = (t * 3.1).sin().abs() * (t * 0.7).cos().abs();
                w.write_sample(((t * 220.0 * std::f32::consts::TAU).sin() * env * 12000.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }
        let _ = std::fs::copy(&files[3], files[3].with_file_name("Circolare-copia.pdf"));
        let themes: Vec<ThemeId> = THEMES.into_iter().filter(|t| t.is_cedro() && (std::env::var("TINTE").is_ok() || only.as_deref() == Some(t.key()))).collect();
        for theme in themes {
            for simple in [true, false] {
                let pages: Vec<(Lab, &str)> = vec![(Lab::Home, "vuoto"), (Lab::Home, ""), (Lab::Converti, ""), (Lab::Converti, "elenco"), (Lab::Pdf, ""), (Lab::Pdf, "unisci"), (Lab::Pdf, "pagine"), (Lab::Trascrivi, ""), (Lab::Video, ""), (Lab::Firma, "")];
                for (lab, extra) in pages {
                    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
                    s.request_appearance(theme, simple);
                    if extra != "vuoto" {
                        s.drop_files(images.clone());
                        s.drop_files(vec![files[3].clone(), files[3].with_file_name("Circolare-copia.pdf")]);
                        if lab == Lab::Trascrivi {
                            s.drop_files(vec![wav.clone()]);
                        }
                    }
                    s.app.youtube.add("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                    s.app.youtube.add("https://www.raiplay.it/video/2026/10/esempio.html");
                    s.app.set_lab(lab, simple);
                    if lab == Lab::Converti {
                        s.app.cedro_leave_pdf(simple);
                    }
                    if extra == "elenco" {
                        s.ui.open.insert(cedro::ui::core::Fold::ListView);
                    }
                    if extra == "unisci" {
                        s.app.set_pdf_mode(cedro::ui::model::PdfMode::Merge, simple);
                    }
                    if extra == "pagine" {
                        s.app.set_pdf_mode(cedro::ui::model::PdfMode::Pages, simple);
                    }
                    if lab == Lab::Home && extra.is_empty() {
                        s.app.lab = Lab::Home;
                    }
                    settle(&mut s);
                    s.render(size.0, size.1, 1.0);
                    settle(&mut s);
                    s.render(size.0, size.1, 1.0);
                    settle(&mut s);
                    let name = format!("{}-{}-{}{}.png", theme.key(), if simple { "sempl" } else { "compl" }, format!("{lab:?}").to_lowercase(), if extra.is_empty() { String::new() } else { format!("-{extra}") });
                    s.render(size.0, size.1, 1.0).save_png(&out.join(&name)).unwrap();
                    println!("{name}");
                }
            }
        }
        for m in cedro::i18n::misses() {
            println!("SENZA TRADUZIONE: {m:?}");
        }
        return;
    }
    // Il caso e i «Lavori»: barra con il caso, finestrella, pagina dei lavori (da lanciare con XDG_CONFIG_HOME/APPDATA su una cartella vuota).
    if only.as_deref() == Some("cedro-caso") {
        use cedro::ui::model::Lab;
        let size = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1536, 1024));
        for simple in [false, true] {
            for (nome, passo) in [("barra-vuota", 0), ("dialogo", 1), ("barra-caso", 2), ("lavori", 3)] {
                let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
                s.request_appearance(ThemeId::CedroLabs, simple);
                s.drop_files(images.clone());
                s.app.set_lab(Lab::Home, simple);
                if passo >= 1 {
                    s.app.caso_bozza = ("V-2026-0457".into(), "Luna, labrador".into());
                }
                if passo == 1 {
                    s.app.caso_aperto = true;
                }
                if passo >= 2 {
                    s.app.caso_conferma();
                }
                if passo == 3 {
                    let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                    for (i, (w, outs)) in [("Salva immagini e vetrini", vec!["2026-11-13_V-2026-0457_vetrino.png", "2026-11-13_V-2026-0457_vetrino_2.png"]), ("Togli i nomi", vec!["2026-11-13_V-2026-0457_referto.pdf"])].into_iter().enumerate() {
                        cedro::caso::registra(&mut s.app.lavori, cedro::caso::Lavoro { when: when - 600 * i as u64, what: w.into(), caso: "V-2026-0457".into(), outputs: outs.into_iter().map(String::from).collect() });
                    }
                    s.app.lavori_aperti = true;
                }
                settle(&mut s);
                s.render(size.0, size.1, 1.0);
                settle(&mut s);
                let name = format!("caso-{}-{nome}.png", if simple { "sempl" } else { "compl" });
                s.render(size.0, size.1, 1.0).save_png(&out.join(&name)).unwrap();
                println!("{name}");
            }
        }
        return;
    }
    // Cedro 4 Labs: una foto per pagina, nelle due viste, più Inizio vuoto e con un file.
    if only.as_deref() == Some("cedro-labs") {
        use cedro::ui::model::Lab;
        let size = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))).unwrap_or((1536, 1024));
        for simple in [false, true] {
            for (lab, pieno) in std::iter::once((Lab::Home, false)).chain(std::iter::once(Lab::Home).chain(Lab::LAB).chain(Lab::STANDARD).map(|l| (l, true))) {
                let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
                s.request_appearance(ThemeId::CedroLabs, simple);
                if pieno {
                    s.drop_files(images.clone());
                    s.drop_files(vec![files[3].clone()]);
                }
                s.app.set_lab(lab, simple);
                settle(&mut s);
                s.render(size.0, size.1, 1.0);
                settle(&mut s);
                let name = format!("labs-{}-{}{}.png", if simple { "sempl" } else { "compl" }, format!("{lab:?}").to_lowercase(), if pieno { "" } else { "-vuoto" });
                s.render(size.0, size.1, 1.0).save_png(&out.join(&name)).unwrap();
                println!("{name}");
            }
        }
        for m in cedro::i18n::misses() {
            println!("SENZA TRADUZIONE: {m:?}");
        }
        return;
    }
    for theme in THEMES {
        if only.as_deref().is_some_and(|o| o != theme.key()) {
            continue;
        }
        for simple in [false, true] {
            let surfaces: &[Surface] = if theme == ThemeId::CedroCompatto { &[Surface::Main, Surface::Compact] } else { &[Surface::Main] };
            for (act, surface) in [Activity::Convert, Activity::Transcribe, Activity::YouTube].into_iter().flat_map(|a| surfaces.iter().map(move |s| (a, *s))) {
                let mut s = Screen::new(surface, Arc::new(|| {}));
                s.request_appearance(theme, simple);
                // VUOTO=1: senza file, per vedere la zona «Trascina qui».
                if std::env::var("VUOTO").is_err() {
                    s.drop_files(vec![files[3].clone()]);
                    s.drop_files(images.clone());
                }
                s.app.activity = act;
                if surface == Surface::Compact {
                    s.ui.compact_tab = match act {
                        Activity::Convert => 0,
                        Activity::Transcribe => 2,
                        _ => 1,
                    };
                }
                if act == Activity::YouTube {
                    s.app.youtube.add("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                }
                if let Some(st) = std::env::var("STEP").ok().and_then(|v| v.parse().ok()) {
                    s.ui.step = st;
                }
                settle(&mut s);
                let (mut w, mut h) = s.preferred_size();
                if let Some((sw, sh)) = std::env::var("SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))) {
                    (w, h) = (sw, sh);
                }
                let name = format!("{}{}-{}-{}{}.png", theme.key(), if surface == Surface::Compact { "-popup" } else { "" }, if simple { "sempl" } else { "std" }, format!("{act:?}").to_lowercase(), std::env::var("STEP").map(|v| format!("-{v}")).unwrap_or_default());
                let p = s.render(w as u32, h as u32, 1.0);
                p.save_png(&out.join(&name)).unwrap();
                println!("{name}");
            }
        }
    }
    let _ = ThemeId::WindowsEssenziale;
    for m in cedro::i18n::misses() {
        println!("SENZA TRADUZIONE: {m:?}");
    }
}
