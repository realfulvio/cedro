//! Fotografie del riconoscimento di chi parla senza schermo: la finestrella «Quante persone parlano?»,
//! il riquadro «Cosa sta succedendo» con le voci e Rivedi con la scheda «Chi parla», nei temi.
//! `cargo run --example voci_foto -- <cartella>` scrive i PNG nella cartella.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cedro::trascrizione::{self, Line, Segment, Tone};
use cedro::ui::core::Act;
use cedro::ui::model::Activity;
use cedro::ui::review::Review;
use cedro::ui::screen::{Platform, Screen, Surface};
use cedro::ui::theme::{ThemeId, THEMES};

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

fn main() {
    // LANG_FOTO=en: le foto in inglese; altrimenti in italiano, qualunque sia la lingua del sistema.
    cedro::i18n::fix();
    cedro::i18n::set(if std::env::var("LANG_FOTO").is_ok_and(|v| v == "en") { cedro::i18n::Lang::En } else { cedro::i18n::Lang::It });
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "foto".into()));
    std::fs::create_dir_all(&out).unwrap();
    let dir = std::env::temp_dir().join("cedro-voci-demo");
    std::fs::create_dir_all(&dir).unwrap();
    let audio: Vec<PathBuf> = ["Consiglio-comunale-12-ottobre.wav", "Intervista-assessore.wav", "Vocale-ufficio.wav"]
        .iter()
        .map(|n| {
            let p = dir.join(n);
            wav(&p);
            p
        })
        .collect();
    let shot = |s: &mut Screen, name: &str, w: f32, h: f32| {
        let p = s.render(w as u32, h as u32, 1.0);
        p.save_png(&out.join(name)).unwrap();
        println!("{name}");
    };
    for theme in THEMES {
        for simple in [false, true] {
            let surfaces: &[Surface] = if theme == ThemeId::CedroCompatto { &[Surface::Main, Surface::Compact] } else { &[Surface::Main] };
            for &surface in surfaces {
                let tag = format!("{}{}-{}", theme.key(), if surface == Surface::Compact { "-popup" } else { "" }, if simple { "sempl" } else { "std" });
                let mut s = Screen::new(surface, Arc::new(|| {}));
                s.request_appearance(theme, simple);
                s.app.activity = Activity::Transcribe;
                if surface == Surface::Compact {
                    s.ui.compact_tab = 2;
                }
                s.drop_files(audio.clone());
                for _ in 0..5 {
                    s.pump();
                }
                let (w, h) = s.preferred_size();
                // La finestrella, su Automatico e su 4.
                s.act(Act::Start, &mut Finta);
                shot(&mut s, &format!("{tag}-1-domanda.png"), w, h);
                s.act(Act::VoicesPick(4), &mut Finta);
                shot(&mut s, &format!("{tag}-2-domanda-4.png"), w, h);
                s.act(Act::VoicesCancel, &mut Finta);
                if simple || surface == Surface::Compact {
                    continue;
                }
                // Il riquadro mentre lavora.
                let note = |key: &str, text: &str| Line { tone: Tone::Note, key: key.into(), text: text.into(), speaker: None };
                s.app.transcribe.log.push(note("Modello", "Buona · motore Vulkan (Radeon)"));
                s.app.transcribe.log.push(note("Voci", "trovate 4 voci in 47:12 (38 s)"));
                for (t, v, text) in SAY.iter().take(7) {
                    s.app.transcribe.log.push(Line { tone: Tone::Speech, key: format!("[00:00:{t:02}]"), text: text.to_string(), speaker: Some(*v) });
                }
                shot(&mut s, &format!("{tag}-3-lavoro.png"), w, h);
            }
        }
        // Rivedi con le voci.
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(theme, false);
        let segments: Vec<Segment> = SAY.iter().map(|(t, v, text)| Segment { t0: t * 100, t1: t * 100 + 680, text: text.to_string(), speaker: Some(*v) }).collect();
        let mut names = trascrizione::default_names(4);
        names[0] = "Sindaco".into();
        names[1] = "Segretaria comunale".into();
        let mut r = Review::new(&audio[0], 75.0, "it".into(), segments, names, None);
        r.bars = Some((0..900).map(|i| (0.3 + 0.6 * ((i as f32 / 7.0).sin() * 0.5 + 0.5)) * if (i / 40) % 5 == 4 { 0.1 } else { 1.0 }).collect());
        s.app.review = Some(r);
        let (w, h) = (1500.0, 980.0);
        shot(&mut s, &format!("{}-4-rivedi.png", theme.key()), w, h);
        s.act(Act::ReviewSpeakerMenu(4), &mut Finta);
        shot(&mut s, &format!("{}-5-rivedi-menu.png", theme.key()), w, h);
        s.ui.menu = None;
        s.act(Act::ReviewTab(0), &mut Finta);
        shot(&mut s, &format!("{}-6-rivedi-controlli.png", theme.key()), w, h);
    }
}
