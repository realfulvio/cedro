//! Banco: quanto costa ridisegnare la finestra Conversioni durante un lavoro (trascrizione in corso).
//! `cargo run --release --example banco_fotogrammi -- [scala] [giri]`

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cedro::ui::model::{Activity, FileState, Job, JobKind};
use cedro::ui::screen::{Screen, Surface};

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
    let scale: f32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let giri: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(100);
    let dir = std::env::temp_dir().join("cedro-banco-fotogrammi");
    std::fs::create_dir_all(&dir).unwrap();
    let audio: Vec<PathBuf> = ["riunione-1.wav", "riunione-2.wav"].iter().map(|n| { let p = dir.join(n); wav(&p); p }).collect();
    let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
    s.app.activity = Activity::Transcribe;
    s.drop_files(audio.clone());
    for _ in 0..5 { s.pump(); }
    let n = s.app.files.len();
    s.app.files[0].state = FileState::Running;
    s.app.job = Some(Job { kind: JobKind::Transcribe, total: n, done: 0, failed: 0, cancel: Arc::new(AtomicBool::new(false)), key: String::new(), targets: (0..n).collect(), part: 0.4, file_started: None });
    s.app.transcribe.percent = 40;
    s.app.transcribe.since = Some((Instant::now() - Duration::from_secs(150), 2));
    let (w, h) = s.preferred_size();
    let (w, h) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
    // primo fotogramma (cache fredde)
    let t0 = Instant::now();
    let _ = s.render(w, h, scale);
    let primo = t0.elapsed();
    let mut tempi = Vec::with_capacity(giri);
    for _ in 0..giri {
        let t = Instant::now();
        let _ = s.render(w, h, scale);
        tempi.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    tempi.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = tempi[tempi.len() / 2];
    if let Ok(png) = std::env::var("BANCO_PNG") {
        let p = s.render(w, h, scale);
        image::RgbaImage::from_raw(w, h, p.pixmap.data().to_vec()).unwrap().save(png).unwrap();
    }
    println!("finestra {w}x{h} scala {scale}: primo {:.1} ms, mediana {:.2} ms, animating={}", primo.as_secs_f64() * 1000.0, med, s.animating());
}
