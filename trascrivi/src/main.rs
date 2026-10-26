//! cedro-trascrivi: il motore di trascrizione di Cedro, con whisper.cpp.
//!
//! Cedro lo scarica alla prima trascrizione e lo avvia solo mentre trascrive. Il programma carica il
//! modello una volta, poi legge da stdin una riga JSON per file (`{"file": "...", "lang": "auto"}`) e
//! risponde su stdout con una riga JSON per ogni evento. Chiuso stdin, esce: la memoria del modello
//! torna libera.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

mod decode;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let Some(model) = value(&args, "--model") else {
        fail("uso: cedro-trascrivi --model <file ggml> [--threads N] [--cpu]");
    };
    let threads = value(&args, "--threads").and_then(|t| t.parse().ok()).filter(|t: &usize| *t > 0).unwrap_or_else(default_threads);
    let cpu_only = args.iter().any(|a| a == "--cpu");
    // Il PC deve restare reattivo mentre trascrive: priorità più bassa delle finestre.
    #[cfg(unix)]
    unsafe {
        libc::nice(10);
    }
    let start = Instant::now();
    let mut params = WhisperContextParameters::default();
    params.use_gpu(cfg!(feature = "vulkan") && !cpu_only);
    let ctx = match WhisperContext::new_with_params(&model, params) {
        Ok(ctx) => ctx,
        Err(e) => fail(&format!("il modello non si apre ({e}): forse il file è rovinato")),
    };
    let gpu = if cfg!(feature = "vulkan") && !cpu_only { video_card() } else { None };
    emit(json!({"e": "ready", "ms": start.elapsed().as_millis() as u64, "threads": threads, "gpu": gpu}));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        // Alcuni programmi (PowerShell, .NET) mettono un BOM UTF-8 in testa a stdin.
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                emit(json!({"e": "error", "msg": format!("richiesta non valida: {e}")}));
                continue;
            }
        };
        match transcribe(&ctx, &request, threads) {
            Ok(lang) => emit(json!({"e": "end", "lang": lang})),
            Err(Failure::Unsupported(msg)) => emit(json!({"e": "error", "msg": msg, "unsupported": true})),
            Err(Failure::Other(msg)) => emit(json!({"e": "error", "msg": msg})),
        }
    }
}

pub enum Failure {
    /// Il formato non si legge qui: Cedro può decodificarlo con il sistema e riprovare con un WAV.
    Unsupported(String),
    Other(String),
}

fn transcribe(ctx: &WhisperContext, request: &Value, threads: usize) -> Result<String, Failure> {
    let file = request["file"].as_str().ok_or_else(|| Failure::Other("manca il file".into()))?;
    let wanted = request["lang"].as_str().unwrap_or("auto");
    let begin = Instant::now();
    let samples = decode::to_16k_mono(Path::new(file))?;
    if samples.len() < decode::RATE as usize / 10 {
        return Err(Failure::Other("l'audio è vuoto".into()));
    }
    emit(json!({"e": "audio", "secs": samples.len() as f64 / decode::RATE as f64, "ms": begin.elapsed().as_millis() as u64}));
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
    emit(json!({"e": "lang", "code": lang}));

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
    params.set_n_threads(threads as i32);
    params.set_language(Some(&lang));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Niente «[Musica]», «(applausi)» e simili al posto del parlato.
    params.set_suppress_nst(true);
    params.set_segment_callback_safe_lossy(|seg: whisper_rs::SegmentCallbackData| {
        let text = seg.text.trim();
        if !text.is_empty() {
            emit(json!({"e": "seg", "t0": seg.start_timestamp, "t1": seg.end_timestamp, "text": text}));
        }
    });
    let mut last = -1;
    params.set_progress_callback_safe(move |p: i32| {
        if p != last {
            last = p;
            emit(json!({"e": "progress", "p": p}));
        }
    });
    state.full(params, &samples).map_err(|e| Failure::Other(format!("trascrizione interrotta: {e}")))?;
    Ok(lang)
}

/// Il nome della scheda video che whisper.cpp ha acceso (Vulkan), se ce n'è una. Lo chiede a ggml, che
/// elenca i dispositivi trovati: «Vulkan0» è il primo, la descrizione ha il nome commerciale.
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

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn fail(msg: &str) -> ! {
    emit(json!({"e": "fatal", "msg": msg}));
    std::process::exit(1)
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// Lascia un processore libero al resto del PC; oltre 8 whisper non va più veloce.
fn default_threads() -> usize {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    n.saturating_sub(1).clamp(1, 8)
}
