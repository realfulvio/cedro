//! Fotografie del Riassunto AI senza schermo: il pulsante nella coda e Rivedi con la scheda «Riassunto»
//! nei quattro momenti (prima volta, al lavoro, poca memoria, pronto), nei temi; più il Word e il testo
//! con il riassunto in cima. `cargo run --example riassunto_foto -- <cartella>` scrive tutto nella cartella.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cedro::riassunto::{Failure, Step, Stored};
use cedro::trascrizione::{self, Segment};
use cedro::ui::model::Activity;
use cedro::ui::summary::Job;
use cedro::ui::review::Review;
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

/// Un audio finto (un secondo di silenzio) per la coda.
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

const SAY: [(i64, u8, &str); 9] = [
    (4, 0, "Buonasera a tutti. Constatato il numero legale, dichiaro aperta la seduta del Consiglio comunale."),
    (11, 0, "Passiamo al primo punto all'ordine del giorno: approvazione dei verbali della seduta precedente."),
    (19, 1, "I verbali sono stati inviati a tutti i consiglieri il 5 ottobre. Non sono arrivate osservazioni."),
    (27, 0, "Bene. Chi è favorevole? Contrari? Astenuti? Approvato all'unanimità."),
    (36, 2, "Chiedo la parola sul secondo punto, la variazione di bilancio per le scuole di Scortichino."),
    (44, 2, "Vorrei capire da dove arrivano i 120 mila euro e se il cantiere parte prima dell'inverno."),
    (53, 3, "Arrivano dall'avanzo vincolato del 2025. Il cantiere apre il 3 novembre, la ditta è già incaricata."),
    (62, 2, "Grazie, per me è sufficiente."),
    (66, 0, "Se non ci sono altri interventi metto in votazione il secondo punto."),
];

const ANSWER: &str = "## In breve\nSeduta del Consiglio comunale del 12 ottobre, 47 minuti. Approvati all'unanimità i verbali della seduta precedente. Discussa la variazione di bilancio da 120 mila euro per le scuole di Scortichino, poi messa in votazione.\n\n## Chi ha detto cosa\n- Sindaco: Apre la seduta e guida le due votazioni.\n- Segretaria comunale: I verbali sono stati inviati il 5 ottobre, senza osservazioni.\n- Interlocutore 3: Chiede da dove arrivano i 120 mila euro e quando parte il cantiere.\n- Interlocutore 4: Fondi dall'avanzo vincolato 2025; cantiere dal 3 novembre, ditta già incaricata.\n\n## Decisioni\n- Verbali della seduta precedente approvati all'unanimità.\n- Variazione di bilancio per le scuole di Scortichino messa in votazione.\n\n## Da fare\n- Aprire il cantiere delle scuole di Scortichino entro il 3 novembre (Interlocutore 4)\n";

fn main() {
    cedro::i18n::fix();
    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It });
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let dir = std::env::temp_dir().join("cedro-riassunto-demo");
    std::fs::create_dir_all(&dir).unwrap();
    let audio: Vec<PathBuf> = ["Consiglio-comunale-12-ottobre.wav", "Intervista-assessore.wav"]
        .iter()
        .map(|n| {
            let p = dir.join(n);
            wav(&p);
            p
        })
        .collect();
    let segments: Vec<Segment> = SAY.iter().map(|(t, v, text)| Segment { t0: t * 100, t1: t * 100 + 680, text: text.to_string(), speaker: Some(*v) }).collect();
    // I nomi quando è stato fatto il riassunto: due ancora da dare. Dopo li si cambia, e il riassunto li segue.
    let mut then = trascrizione::default_names(4);
    then[0] = "Sindaco".into();
    then[1] = "Segretaria comunale".into();
    let mut names = then.clone();
    names[2] = "Consigliera Bianchi".into();
    names[3] = "Assessore Neri".into();
    for a in &audio {
        cedro::revisione::save(a, 2832.0, "it", &segments, &names).unwrap();
    }
    let stored = Stored { text: ANSWER.into(), names: then, ms: 160_000, gpu: Some("AMD Radeon Graphics".into()), done: Vec::new() };
    let shot = |s: &mut Screen, name: &str, w: f32, h: f32| {
        let p = s.render(w as u32, h as u32, 1.0);
        p.save_png(&out.join(name)).unwrap();
        println!("{name}");
    };
    for theme in THEMES {
        // La coda con «Rivedi» e «Riassunto AI».
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(theme, false);
        s.app.activity = Activity::Transcribe;
        s.drop_files(audio.clone());
        for _ in 0..5 {
            s.pump();
        }
        let (w, h) = s.preferred_size();
        shot(&mut s, &format!("{}-0-coda.png", theme.key()), w, h);
        if theme == ThemeId::CedroCompatto {
            continue;
        }
        let review = |summary: Option<Stored>| {
            let mut r = Review::new(&audio[0], 2832.0, "it".into(), segments.clone(), names.clone(), summary);
            r.bars = Some((0..900).map(|i| (0.3 + 0.6 * ((i as f32 / 7.0).sin() * 0.5 + 0.5)) * if (i / 40) % 5 == 4 { 0.1 } else { 1.0 }).collect());
            r.tab = 2;
            r
        };
        let (w, h) = (1500.0, 1080.0);
        // 1. Prima volta.
        s.app.review = Some(review(None));
        shot(&mut s, &format!("{}-1-prima-volta.png", theme.key()), w, h);
        // 2. Al lavoro: parte 3 di 4.
        s.app.summary_job = Some(Job {
            source: audio[0].clone(),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            step: Step::Reading(2, 4),
            percent: 62,
            free: Some(9_200_000_000),
            total: Some(16 * (1u64 << 30)),
            reading_since: Some(std::time::Instant::now() - std::time::Duration::from_secs(95)),
            gpu: Some("AMD Radeon Graphics".into()),
            ready: true,
        });
        shot(&mut s, &format!("{}-2-al-lavoro.png", theme.key()), w, h);
        s.app.summary_job = None;
        // 3. Poca memoria.
        s.app.summary_failure = Some((audio[0].clone(), Failure::LowMemory(2_100_000_000)));
        shot(&mut s, &format!("{}-3-poca-memoria.png", theme.key()), w, h);
        s.app.summary_failure = None;
        // 4. Pronto.
        s.app.review = Some(review(Some(stored.clone())));
        shot(&mut s, &format!("{}-4-pronto.png", theme.key()), w, h);
    }
    // Il Word e il testo con il riassunto in cima.
    let r = Review::new(&audio[0], 2832.0, "it".into(), segments.clone(), names.clone(), Some(stored));
    let sum = r.summary_sections();
    std::fs::write(out.join("riassunto.docx"), trascrizione::docx_bytes_with(&audio[0], 2832.0, "it", &segments, &names, sum.as_ref())).unwrap();
    std::fs::write(out.join("riassunto.txt"), trascrizione::text_bytes_with(&audio[0], 2832.0, "it", &segments, &names, sum.as_ref())).unwrap();
    println!("riassunto.docx riassunto.txt");
}
