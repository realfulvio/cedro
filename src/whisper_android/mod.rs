//! whisper.cpp dentro l'APK (libreria, non un programma a parte): stessa conversazione a eventi del motore esterno
//! `cedro-trascrivi`, ma su un thread del processo. Su Android non si scarica né si avvia nessun eseguibile: l'APK
//! porta il codice, il primo uso scarica solo il modello (tiny o base, in `trascrizione::MODELS`).

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

use serde_json::{json, Value};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

mod decode;

pub(crate) enum Failure {
    Unsupported(String),
    Other(String),
}

/// Accende il motore su un thread: dal `Sender` arrivano le richieste (`{"file": ..}`), dal `Receiver` gli eventi.
/// Chiuso il `Sender` il thread finisce e il modello torna libero.
pub(crate) fn spawn(model: &Path, threads: usize, gpu: bool) -> (Sender<Value>, Receiver<Value>) {
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
            while let Ok(request) = req_rx.recv() {
                let ev = match transcribe(&ctx, &request, threads, &ev_tx) {
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

fn transcribe(ctx: &WhisperContext, request: &Value, threads: usize, ev: &Sender<Value>) -> Result<String, Failure> {
    let file = request["file"].as_str().ok_or_else(|| Failure::Other("manca il file".into()))?;
    let wanted = request["lang"].as_str().unwrap_or("auto");
    let begin = Instant::now();
    let samples = decode::to_16k_mono(Path::new(file))?;
    if samples.len() < decode::RATE as usize / 10 {
        return Err(Failure::Other("l'audio è vuoto".into()));
    }
    let _ = ev.send(json!({"e": "audio", "secs": samples.len() as f64 / decode::RATE as f64, "ms": begin.elapsed().as_millis() as u64}));
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

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
    params.set_n_threads(threads as i32);
    params.set_language(Some(&lang));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Niente «[Musica]», «(applausi)» e simili al posto del parlato.
    params.set_suppress_nst(true);
    params.set_segment_callback_safe_lossy({
        let ev = ev.clone();
        move |seg: whisper_rs::SegmentCallbackData| {
        let text = seg.text.trim();
        if !text.is_empty() {
            let _ = ev.send(json!({"e": "seg", "t0": seg.start_timestamp, "t1": seg.end_timestamp, "text": text}));
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
