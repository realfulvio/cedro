//! whisper.cpp dentro l'APK (libreria, non un programma a parte): stessa conversazione a eventi del motore esterno
//! `cedro-trascrivi`, ma su un thread del processo. Su Android non si scarica né si avvia nessun eseguibile: l'APK
//! porta il codice, il primo uso scarica solo il modello (tiny o base, in `trascrizione::MODELS`).

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

use serde_json::{json, Value};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

mod decode;
// Prompt italiano e frase fantasma finale: le stesse regole del motore del computer (v0.34).
#[path = "../../trascrivi/src/rifinitura.rs"]
mod rifinitura;
// Chi parla: lo stesso codice del motore del computer (pyannote 3.0 + CAM++ in Rust con rten).
#[path = "../../trascrivi/src/voci.rs"]
#[allow(dead_code)]
mod voci;

pub(crate) enum Failure {
    Unsupported(String),
    Other(String),
}

/// Accende il motore su un thread: dal `Sender` arrivano le richieste (`{"file": ..}`), dal `Receiver` gli eventi.
/// Chiuso il `Sender` il thread finisce e il modello torna libero.
pub(crate) fn spawn(model: &Path, threads: usize, gpu: bool, voices: Option<(std::path::PathBuf, std::path::PathBuf)>) -> (Sender<Value>, Receiver<Value>) {
    let (req_tx, req_rx) = channel::<Value>();
    let (ev_tx, ev_rx) = channel::<Value>();
    let model = model.to_path_buf();
    std::thread::Builder::new()
        .name("whisper".into())
        .stack_size(16 << 20)
        .spawn(move || {
            let start = Instant::now();
            let mut params = WhisperContextParameters::default();
            params.use_gpu(gpu);
            let ctx = match WhisperContext::new_with_params(&model, params) {
                Ok(c) => c,
                Err(e) => {
                    let _ = ev_tx.send(json!({"e": "fatal", "msg": format!("il modello non si apre ({e})")}));
                    return;
                }
            };
            let _ = ev_tx.send(json!({"e": "ready", "ms": start.elapsed().as_millis() as u64, "threads": threads, "gpu": if gpu { video_card() } else { None }}));
            let mut voci = Voci { paths: voices, loaded: None };
            while let Ok(request) = req_rx.recv() {
                let ev = match transcribe(&ctx, &request, threads, &ev_tx, &mut voci) {
                    Ok(lang) => json!({"e": "end", "lang": lang}),
                    Err(Failure::Unsupported(msg)) => json!({"e": "error", "msg": msg, "unsupported": true}),
                    Err(Failure::Other(msg)) => json!({"e": "error", "msg": msg}),
                };
                if ev_tx.send(ev).is_err() {
                    break;
                }
            }
        })
        .expect("thread whisper");
    (req_tx, ev_rx)
}

/// I modelli delle voci: si caricano alla prima richiesta che li vuole e restano per la coda.
struct Voci {
    paths: Option<(std::path::PathBuf, std::path::PathBuf)>,
    loaded: Option<voci::Diarizer>,
}

impl Voci {
    fn get(&mut self) -> Result<&voci::Diarizer, String> {
        if self.loaded.is_none() {
            let Some((seg, emb)) = &self.paths else { return Err("modelli delle voci non indicati".into()) };
            self.loaded = Some(voci::Diarizer::load(seg, emb)?);
        }
        Ok(self.loaded.as_ref().expect("caricato"))
    }
}

/// Quante voci chiede la richiesta: None = nessun riconoscimento (manca o è 1).
fn requested_speakers(request: &Value) -> Option<voci::Speakers> {
    match request["speakers"].as_u64() {
        Some(0) => Some(voci::Speakers::Auto),
        Some(n) if n >= 2 => Some(voci::Speakers::Exactly((n as usize).min(voci::MAX_SPEAKERS))),
        _ => None,
    }
}

/// Riconosce chi parla, con gli stessi eventi del motore del computer. Se va storto si trascrive senza nomi.
fn diarize(samples: &[f32], wanted: voci::Speakers, threads: usize, ev: &Sender<Value>, voices: &mut Voci) {
    let begin = Instant::now();
    let _ = ev.send(json!({"e": "voices_start"}));
    let diarizer = match voices.get() {
        Ok(d) => d,
        Err(msg) => {
            let _ = ev.send(json!({"e": "voices_error", "msg": msg}));
            return;
        }
    };
    let last = std::cell::Cell::new(255u8);
    let progress = |p: u8| {
        if p / 10 != last.get() / 10 || last.get() == 255 {
            last.set(p);
            let _ = ev.send(json!({"e": "voices_progress", "p": p}));
        }
    };
    let never = std::sync::atomic::AtomicBool::new(false);
    let _ = match diarizer.run(samples, wanted, threads, &progress, &never) {
        Ok(turns) => {
            let n = turns.iter().map(|t| t.speaker + 1).max().unwrap_or(0);
            let list: Vec<Value> = turns.iter().map(|t| json!([t.t0, t.t1, t.speaker])).collect();
            ev.send(json!({"e": "voices", "n": n, "turns": list, "ms": begin.elapsed().as_millis() as u64}))
        }
        Err(msg) => ev.send(json!({"e": "voices_error", "msg": msg})),
    };
}

fn transcribe(ctx: &WhisperContext, request: &Value, threads: usize, ev: &Sender<Value>, voices: &mut Voci) -> Result<String, Failure> {
    let file = request["file"].as_str().ok_or_else(|| Failure::Other("manca il file".into()))?;
    let wanted = request["lang"].as_str().unwrap_or("auto");
    let begin = Instant::now();
    let samples = decode::to_16k_mono(Path::new(file))?;
    if samples.len() < decode::RATE as usize / 10 {
        return Err(Failure::Other("l'audio è vuoto".into()));
    }
    let _ = ev.send(json!({"e": "audio", "secs": samples.len() as f64 / decode::RATE as f64, "ms": begin.elapsed().as_millis() as u64}));
    if let Some(wanted) = requested_speakers(request) {
        diarize(&samples, wanted, threads, ev, voices);
    }
    let mut state = ctx.create_state().map_err(|e| Failure::Other(format!("whisper non parte: {e}")))?;
    let lang: String = if wanted == "auto" {
        // La lingua si riconosce dai primi 30 secondi, prima di trascrivere, così si vede subito.
        let head = &samples[..samples.len().min(30 * decode::RATE as usize)];
        state.pcm_to_mel(head, threads).map_err(|e| Failure::Other(format!("lingua non riconosciuta: {e}")))?;
        let (id, _) = state.lang_detect(0, threads).map_err(|e| Failure::Other(format!("lingua non riconosciuta: {e}")))?;
        whisper_rs::get_lang_str(id).unwrap_or("en").to_string()
    } else {
        wanted.to_string()
    };
    let _ = ev.send(json!({"e": "lang", "code": lang}));

    // Come sul computer (v0.34): ricerca a fascio con 5 ipotesi, che toglie parole storpiate per circa il 15 % di tempo in più.
    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
    params.set_n_threads(threads as i32);
    params.set_language(Some(&lang));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Niente «[Musica]», «(applausi)» e simili al posto del parlato.
    params.set_suppress_nst(true);
    // Un esempio con la punteggiatura: senza, il parlato informale esce minuscolo e senza punti.
    if let Some(prompt) = rifinitura::initial_prompt(&lang) {
        params.set_initial_prompt(prompt);
    }
    // Ogni frase parte quando arriva la successiva: l'ultima si guarda a parte, perché a fine audio whisper a volte
    // inventa un «Grazie.» che nessuno ha detto.
    let pending: std::sync::Arc<std::sync::Mutex<Option<(i64, i64, String)>>> = Default::default();
    params.set_segment_callback_safe_lossy({
        let ev = ev.clone();
        let held = pending.clone();
        move |seg: whisper_rs::SegmentCallbackData| {
            let text = seg.text.trim();
            if !text.is_empty() {
                let prima = held.lock().unwrap_or_else(|e| e.into_inner()).replace((seg.start_timestamp, seg.end_timestamp, text.to_string()));
                if let Some((t0, t1, text)) = prima {
                    let _ = ev.send(json!({"e": "seg", "t0": t0, "t1": t1, "text": text}));
                }
            }
        }
    });
    let mut last = -1;
    let ev_p = ev.clone();
    params.set_progress_callback_safe(move |p: i32| {
        if p != last {
            last = p;
            let _ = ev_p.send(json!({"e": "progress", "p": p}));
        }
    });
    state.full(params, &samples).map_err(|e| Failure::Other(format!("trascrizione interrotta: {e}")))?;
    let audio_end = (samples.len() as u64 * 100 / decode::RATE as u64) as i64;
    let ultima = pending.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((t0, t1, text)) = ultima {
        if !rifinitura::phantom_tail(&text, t0, t1, audio_end) {
            let _ = ev.send(json!({"e": "seg", "t0": t0, "t1": t1, "text": text}));
        }
    }
    Ok(lang)
}


/// Il nome della GPU che ggml ha acceso (Vulkan), se c'è.
fn video_card() -> Option<String> {
    use std::ffi::CStr;
    unsafe {
        for i in 0..whisper_rs_sys::ggml_backend_dev_count() {
            let dev = whisper_rs_sys::ggml_backend_dev_get(i);
            let name = whisper_rs_sys::ggml_backend_dev_name(dev);
            if name.is_null() || !CStr::from_ptr(name).to_string_lossy().starts_with("Vulkan") {
                continue;
            }
            let desc = whisper_rs_sys::ggml_backend_dev_description(dev);
            let text = if desc.is_null() { String::new() } else { CStr::from_ptr(desc).to_string_lossy().trim().to_string() };
            return Some(if text.is_empty() { "scheda video".to_string() } else { text });
        }
    }
    None
}
