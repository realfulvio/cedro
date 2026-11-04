//! Fotografie della trascrizione in corso (barra di avanzamento e spia) in tutti i temi, normale e
//! semplificata. `cargo run --example trascrivi_foto -- <cartella>` scrive i PNG nella cartella.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::trascrizione::{Line, Tone};
use cedro::ui::model::{Activity, FileState, Job, JobKind};
use cedro::ui::screen::{Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

fn wav(path: &Path) {
    let n = 16000u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16000u32.to_le_bytes());
    b.extend_from_slice(&32000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    b.resize(b.len() + n as usize * 2, 0);
    std::fs::write(path, b).unwrap();
}

fn main() {
    cedro::i18n::fix();
    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
        cedro::i18n::Lang::En
    } else {
        cedro::i18n::Lang::It
    });
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "foto-trascrivi".into()),
    );
    let only = std::env::args().nth(2);
    std::fs::create_dir_all(&out).unwrap();
    let dir = std::env::temp_dir().join("cedro-trascrivi-demo");
    std::fs::create_dir_all(&dir).unwrap();
    let audio: Vec<PathBuf> = [
        "Consiglio-comunale-12-ottobre.wav",
        "Intervista-assessore.wav",
    ]
    .iter()
    .map(|n| {
        let p = dir.join(n);
        wav(&p);
        p
    })
    .collect();
    // (nome, percentuale, scheda video?)
    let cases = [("gpu", 42u8, true), ("cpu", 78, false), ("inizio", 0, true)];
    for theme in THEMES {
        if only.as_deref().is_some_and(|o| o != theme.key()) {
            continue;
        }
        for simple in [false, true] {
            let surfaces: &[Surface] = if theme == ThemeId::CedroCompatto {
                &[Surface::Main, Surface::Compact]
            } else {
                &[Surface::Main]
            };
            for surface in surfaces {
                for (case, percent, gpu) in cases {
                    let mut s = Screen::new(*surface, Arc::new(|| {}));
                    s.request_appearance(theme, simple);
                    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") {
                        cedro::i18n::Lang::En
                    } else {
                        cedro::i18n::Lang::It
                    });
                    s.app.activity = Activity::Transcribe;
                    s.drop_files(audio.clone());
                    for _ in 0..5 {
                        s.pump();
                    }
                    let n = s.app.files.len();
                    s.app.files[0].state = FileState::Running;
                    s.app.files[0].message = format!("In corso · {percent}%");
                    s.app.job = Some(Job {
                        kind: JobKind::Transcribe,
                        total: n,
                        done: 0,
                        failed: 0,
                        cancel: Arc::new(AtomicBool::new(false)),
                        key: String::new(),
                        targets: (0..n).collect(),
                        part: percent as f32 / 100.0,
                        file_started: None,
                    });
                    s.app.transcribe.percent = percent;
                    s.app.transcribe.since =
                        (percent > 0).then(|| (Instant::now() - Duration::from_secs(150), 2));
                    s.app.transcribe.log = vec![
                        Line {
                            tone: Tone::Step,
                            key: "Motore".into(),
                            text: "lo accendo e carico il modello in memoria…".into(),
                            speaker: None,
                        },
                        Line {
                            tone: Tone::Step,
                            key: "Voci".into(),
                            text: "riconosco chi parla…".into(),
                            speaker: None,
                        },
                    ];
                    if gpu {
                        cedro::accel::set_in_use(
                            true,
                            cedro::accel::gpu_model("NVIDIA Quadro RTX 4000"),
                        );
                    } else {
                        cedro::accel::set_in_use(false, "AVX2 · 8 thread");
                    }
                    if theme == ThemeId::PercorsoGuidato {
                        s.ui.step = 2;
                    }
                    let (w, h) = s.preferred_size();
                    let name = format!(
                        "{}{}-{}-{case}.png",
                        theme.key(),
                        if *surface == Surface::Compact {
                            "-popup"
                        } else {
                            ""
                        },
                        if simple { "sempl" } else { "std" }
                    );
                    s.render(w as u32, h as u32, 1.0)
                        .save_png(&out.join(&name))
                        .unwrap();
                    cedro::accel::clear_in_use();
                    println!("{name}");
                }
            }
        }
    }
}
