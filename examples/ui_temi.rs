//! Verifica di tutti i temi × modalità con le voci nuove (Archivi, nome da modello, spia CPU/GPU,
//! «Apri risultato»/«Ricomincia», menu del tasto destro, Video dal web). Foto senza schermo:
//! `cargo run --release --example ui_temi -- <cartella> [tema]`; LANG_FOTO=en per l'inglese.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cedro::conversioni::{self, Opt, CATEGORIES};
use cedro::ui::model::{Activity, FileState, Job, JobKind};
use cedro::ui::screen::{Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

fn make(dir: &Path) -> (PathBuf, Vec<PathBuf>, PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let mut imgs = Vec::new();
    for (i, n) in ["Riunione-giunta-2026.jpg", "Sopralluogo-via-Roma.jpg"].iter().enumerate() {
        let p = dir.join(n);
        image::RgbImage::from_fn(800, 600, |x, y| image::Rgb([(x / 4) as u8, (y / 3) as u8, 80 + i as u8 * 70])).save(&p).unwrap();
        imgs.push(p);
    }
    let zip = dir.join("Pratiche-ottobre-2026.zip");
    {
        use std::io::Write;
        let f = std::fs::File::create(&zip).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("foto/a.jpg", o).unwrap();
        z.write_all(&std::fs::read(&imgs[0]).unwrap()).unwrap();
        z.finish().unwrap();
    }
    let vid = dir.join("Consiglio-comunale.mp4");
    std::fs::write(&vid, b"x").unwrap();
    (zip, imgs, vid)
}

fn cat(name: &str) -> usize {
    CATEGORIES.iter().position(|c| c.name == name).unwrap()
}

fn main() {
    let lang = if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It };
    cedro::i18n::set(lang);
                    cedro::i18n::fix();
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto-temi".into()));
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::args().nth(2);
    let (zip, imgs, _vid) = make(&std::env::temp_dir().join("cedro-temi-demo"));
    let scen = ["archivi", "archivi-altri", "nome-modello", "menu-destro", "lavoro-spia", "finito-uno", "finito-molti", "video-web", "video-formati"];
    for theme in THEMES {
        if only.as_deref().is_some_and(|o| o != theme.key()) {
            continue;
        }
        for simple in [false, true] {
            let surfaces: &[Surface] = if theme == ThemeId::CedroCompatto { &[Surface::Main, Surface::Compact] } else { &[Surface::Main] };
            for surface in surfaces {
                for sc in scen {
                    let mut s = Screen::new(*surface, Arc::new(|| {}));
                    cedro::i18n::set(lang);
                    cedro::i18n::fix();
                    s.request_appearance(theme, simple);
                    match sc {
                        "archivi" | "archivi-altri" => {
                            s.drop_files(vec![zip.clone()]);
                            if sc == "archivi-altri" {
                                s.drop_files(imgs.clone());
                                s.app.set_category(cat("Archivi"), simple);
                            }
                        }
                        "nome-modello" | "menu-destro" | "finito-uno" | "finito-molti" | "lavoro-spia" => {
                            s.drop_files(imgs.clone());
                            if sc == "finito-uno" {
                                s.app.files.truncate(1);
                            }
                            if sc.starts_with("finito") {
                                s.app.start();
                                let t0 = std::time::Instant::now();
                                while (s.app.busy() || t0.elapsed().as_millis() < 300) && t0.elapsed().as_secs() < 30 {
                                    s.pump();
                                    std::thread::sleep(std::time::Duration::from_millis(30));
                                }
                            } else if sc == "lavoro-spia" {
                                let n = s.app.files.len();
                                s.app.files[0].state = FileState::Running;
                                s.app.job = Some(Job { kind: JobKind::Convert, total: n, done: 0, failed: 0, cancel: Arc::new(Default::default()), key: String::new(), targets: (0..n).collect(), part: 0.0, file_started: None });
                                cedro::accel::set_in_use(true, "Vulkan · NVIDIA Quadro RTX 4000 con nome molto lungo");
                            } else {
                                s.ui.open.insert(cedro::ui::core::Fold::MoreOptions);
                                let defs = conversioni::option_defs(s.app.kind(), &s.app.action);
                                if let Some(d) = defs.iter().find(|d| d.opt == Opt::NameTemplate) {
                                    s.app.values.set_text(d, "%nome%_%data%_%ora%_versione-finale-per-stampa");
                                }
                            }
                        }
                        "video-web" => {
                            s.app.activity = Activity::Web;
                            s.app.web.add("https://www.example.org/video/consiglio-comunale-del-27-ottobre-2026-seduta-straordinaria");
                        }
                        "video-formati" => {
                            s.app.set_category(cat("Video"), simple);
                            s.drop_files(vec![std::env::temp_dir().join("cedro-temi-demo/Consiglio-comunale.mp4")]);
                        }
                        _ => {}
                    }
                    if theme == ThemeId::PercorsoGuidato {
                        s.ui.step = if matches!(sc, "finito-uno" | "finito-molti" | "lavoro-spia") { 2 } else { 1 };
                    }
                    for _ in 0..60 {
                        s.pump();
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    let (w, h) = s.preferred_size();
                    let _ = s.render(w as u32, h as u32, 1.0);
                    if sc == "menu-destro" {
                        'f: for y in (0..h as i32).step_by(6) {
                            for x in (0..w as i32).step_by(6) {
                                if s.context_click(x as f32, y as f32) {
                                    break 'f;
                                }
                            }
                        }
                    }
                    let name = format!("{}{}-{}-{}.png", theme.key(), if *surface == Surface::Compact { "-popup" } else { "" }, if simple { "sempl" } else { "std" }, sc);
                    s.render(w as u32, h as u32, 1.0).save_png(&out.join(&name)).unwrap();
                    cedro::accel::clear_in_use();
                    println!("{name}");
                }
            }
        }
    }
}
