//! cedro-trascrivi: il motore di trascrizione di Cedro, con whisper.cpp.
//!
//! Cedro lo scarica alla prima trascrizione e lo avvia solo mentre trascrive. Il programma carica il
//! modello una volta, poi legge da stdin una riga JSON per file (`{"file": "...", "lang": "auto"}`) e
//! risponde su stdout con una riga JSON per ogni evento. Chiuso stdin, esce: la memoria del modello
//! torna libera.
//!
//! Con `--voci-seg` e `--voci-emb` (i due modelli delle voci) e `"speakers"` nella richiesta (0 =
//! automatico, 2..8 = quante persone), prima di trascrivere riconosce chi parla e quando: manda
//! `{"e":"voices","n":3,"turns":[[t0,t1,voce],…]}` (tempi in centesimi), poi le frasi come sempre.
//!
//! Con `--vad <file>` (il rilevatore di parlato Silero in formato ggml) l'audio non si dà a whisper tutto insieme:
//! si trovano i tratti di parlato e si trascrivono a pezzi da ~25 s, ciascuno per conto suo. Senza questo, su un audio
//! lungo whisper passa il testo di una finestra alla successiva e, sul parlato disturbato, entra in un giro che ripete
//! la stessa frase decine di volte (o la inventa sul silenzio).

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

mod decode;
mod parlato;
mod rifinitura;
mod voci;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Per le prove: quante finestre di parlato trova il rilevatore in ogni file indicato.
    if let Some(i) = args.iter().position(|a| a == "--parlato") {
        for f in &args[i + 1..] {
            match decode::to_16k_mono(Path::new(f)) {
                Ok(s) => println!("{}\t{:.1}\t{}\t{}", parlato::moving_windows(&s), parlato::best_range(&s), s.len() as f64 / decode::RATE as f64, f),
                Err(_) => println!("errore\t-\t{f}"),
            }
        }
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
    let mut voices = Voices { seg: value(&args, "--voci-seg"), emb: value(&args, "--voci-emb"), loaded: None };
    let vad = value(&args, "--vad");
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
        match transcribe(&ctx, &request, threads, &mut voices, vad.as_deref()) {
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

fn requested_language(request: &Value) -> Result<&str, Failure> {
    let lang = request["lang"].as_str().unwrap_or("auto");
    // whisper-rs constructs a CString and panics on NUL, even for a valid JSON string.
    if lang.contains('\0') {
        return Err(Failure::Other("lingua non valida: contiene un byte nullo".into()));
    }
    Ok(lang)
}

/// I modelli delle voci: si caricano alla prima richiesta che li vuole e restano per la coda.
struct Voices {
    seg: Option<String>,
    emb: Option<String>,
    loaded: Option<voci::Diarizer>,
}

impl Voices {
    fn get(&mut self) -> Result<&voci::Diarizer, String> {
        if self.loaded.is_none() {
            let (Some(seg), Some(emb)) = (&self.seg, &self.emb) else {
                return Err("modelli delle voci non indicati".into());
            };
            self.loaded = Some(voci::Diarizer::load(Path::new(seg), Path::new(emb))?);
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

/// Riconosce chi parla e lo manda a Cedro. Se qualcosa va storto la trascrizione continua senza nomi.
fn diarize(samples: &[f32], wanted: voci::Speakers, threads: usize, voices: &mut Voices) {
    let begin = Instant::now();
    emit(json!({"e": "voices_start"}));
    let diarizer = match voices.get() {
        Ok(d) => d,
        Err(msg) => return emit(json!({"e": "voices_error", "msg": msg})),
    };
    let last = std::cell::Cell::new(255u8);
    let progress = |p: u8| {
        if p / 10 != last.get() / 10 || last.get() == 255 {
            last.set(p);
            emit(json!({"e": "voices_progress", "p": p}));
        }
    };
    let never = std::sync::atomic::AtomicBool::new(false);
    match diarizer.run(samples, wanted, threads, &progress, &never) {
        Ok(turns) => {
            let n = turns.iter().map(|t| t.speaker + 1).max().unwrap_or(0);
            let list: Vec<Value> = turns.iter().map(|t| json!([t.t0, t.t1, t.speaker])).collect();
            emit(json!({"e": "voices", "n": n, "turns": list, "ms": begin.elapsed().as_millis() as u64}));
        }
        Err(msg) => emit(json!({"e": "voices_error", "msg": msg})),
    }
}

fn transcribe(ctx: &WhisperContext, request: &Value, threads: usize, voices: &mut Voices, vad: Option<&str>) -> Result<String, Failure> {
    let file = request["file"].as_str().ok_or_else(|| Failure::Other("manca il file".into()))?;
    let wanted = requested_language(request)?;
    let begin = Instant::now();
    let samples = decode::to_16k_mono(Path::new(file))?;
    if samples.len() < decode::RATE as usize / 10 {
        return Err(Failure::Other("l'audio è vuoto".into()));
    }
    emit(json!({"e": "audio", "secs": samples.len() as f64 / decode::RATE as f64, "ms": begin.elapsed().as_millis() as u64}));
    // Silenzio, ronzii, soffi, toni: senza parlato whisper inventa frasi (e con la lingua automatica sceglie lingue
    // a caso). Niente frasi: Cedro dice «nessun parlato riconosciuto».
    if !parlato::has_speech(&samples) {
        emit(json!({"e": "nospeech"}));
        return Ok(if wanted == "auto" { "it".to_string() } else { wanted.to_string() });
    }
    if let Some(wanted) = requested_speakers(request) {
        diarize(&samples, wanted, threads, voices);
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
    emit(json!({"e": "lang", "code": lang}));

    let prompt = rifinitura::prompt(&lang, request["glossario"].as_str());
    // Ogni frase parte quando arriva la successiva: l'ultima si guarda a parte, perché a fine audio whisper a volte
    // inventa un «Grazie.» che nessuno ha detto.
    let pending: std::rc::Rc<std::cell::RefCell<Option<(i64, i64, String)>>> = Default::default();
    // Dove comincia il pezzo in corso, in centesimi di secondo: whisper conta i tempi dall'inizio del pezzo.
    let offset: std::rc::Rc<std::cell::Cell<i64>> = Default::default();
    // Quanto del lavoro è già fatto prima di questo pezzo e quanto pesa il pezzo (0..1), per la barra.
    let share: std::rc::Rc<std::cell::Cell<(f64, f64)>> = std::rc::Rc::new(std::cell::Cell::new((0.0, 1.0)));
    let last_progress: std::rc::Rc<std::cell::Cell<i32>> = std::rc::Rc::new(std::cell::Cell::new(-1));

    // I pezzi da trascrivere: i tratti di parlato trovati dal VAD; se il VAD non c'è o non trova nulla, tutto l'audio
    // in una volta, come prima.
    let total = samples.len();
    let chunks = vad.and_then(|v| speech_chunks(v, &samples, threads)).unwrap_or_else(|| vec![(0, total)]);
    let n = chunks.len();
    for (ci, (c0, c1)) in chunks.iter().copied().enumerate() {
        offset.set((c0 as u64 * 100 / decode::RATE as u64) as i64);
        share.set((ci as f64 / n as f64, 1.0 / n as f64));
        // Ricerca a fascio: whisper tiene 5 ipotesi e sceglie la frase più probabile, non la prima parola per volta.
        // Su un messaggio vocale vero toglie parole storpiate per circa il 15 % di tempo in più.
        let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        params.set_n_threads(threads as i32);
        params.set_language(Some(&lang));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Niente «[Musica]», «(applausi)» e simili al posto del parlato.
        params.set_suppress_nst(true);
        if let Some(prompt) = &prompt {
            params.set_initial_prompt(prompt);
        }
        let (held, off) = (pending.clone(), offset.clone());
        params.set_segment_callback_safe_lossy(move |seg: whisper_rs::SegmentCallbackData| {
            // Un giro di frasi uguali dentro un pezzo si riduce a una frase sola.
            let text = rifinitura::collapse_loops(seg.text.trim());
            if !text.is_empty() {
                let (t0, t1) = (seg.start_timestamp + off.get(), seg.end_timestamp + off.get());
                if let Some((t0, t1, text)) = held.borrow_mut().replace((t0, t1, text)) {
                    emit(json!({"e": "seg", "t0": t0, "t1": t1, "text": text}));
                }
            }
        });
        let (share, last) = (share.clone(), last_progress.clone());
        params.set_progress_callback_safe(move |p: i32| {
            let (base, span) = share.get();
            let overall = ((base + span * p as f64 / 100.0) * 100.0) as i32;
            if overall != last.get() {
                last.set(overall);
                emit(json!({"e": "progress", "p": overall}));
            }
        });
        state.full(params, &samples[c0..c1]).map_err(|e| Failure::Other(format!("trascrizione interrotta: {e}")))?;
    }
    let audio_end = (samples.len() as u64 * 100 / decode::RATE as u64) as i64;
    if let Some((t0, t1, text)) = pending.borrow_mut().take() {
        if !rifinitura::phantom_tail(&text, t0, t1, audio_end) {
            emit(json!({"e": "seg", "t0": t0, "t1": t1, "text": text}));
        }
    }
    Ok(lang)
}

/// Il parlato più lungo che whisper riceve in un pezzo: una finestra intera è 30 s, e i pezzi restano sotto.
const CHUNK_SECS: f32 = 29.0;

/// I tratti di parlato dell'audio, in campioni (inizio, fine), uniti in pezzi lunghi al massimo `CHUNK_SECS`.
/// Il rilevatore è Silero (ggml, quello di whisper.cpp): soglia 0,5, parlato di almeno 0,25 s, pausa di almeno 0,5 s,
/// ogni tratto al massimo 25 s con 0,2 s di margine. `None` se non parte o non trova parlato: si trascrive tutto.
fn speech_chunks(vad: &str, samples: &[f32], threads: usize) -> Option<Vec<(usize, usize)>> {
    let mut context = whisper_rs::WhisperVadContextParams::new();
    context.set_n_threads(threads as i32);
    context.set_use_gpu(false);
    let mut vad = whisper_rs::WhisperVadContext::new(vad, context).ok()?;
    let mut params = whisper_rs::WhisperVadParams::new();
    params.set_threshold(0.5);
    params.set_min_speech_duration(250);
    params.set_min_silence_duration(500);
    params.set_max_speech_duration(25.0);
    params.set_speech_pad(200);
    let found: Vec<(f32, f32)> = vad.segments_from_samples(params, samples).ok()?.map(|s| (s.start, s.end)).collect();
    let chunks = merge_speech(&found, samples.len(), CHUNK_SECS);
    (!chunks.is_empty()).then_some(chunks)
}

/// Unisce i tratti di parlato (inizio e fine in centesimi di secondo) in pezzi che durano al massimo `max_secs`,
/// dall'inizio del primo tratto alla fine dell'ultimo; i silenzi tra un pezzo e l'altro restano fuori.
fn merge_speech(found: &[(f32, f32)], total: usize, max_secs: f32) -> Vec<(usize, usize)> {
    let per_cs = decode::RATE as f32 / 100.0;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for &(from, to) in found {
        let (from, to) = (((from * per_cs) as usize).min(total), ((to * per_cs) as usize).min(total));
        if to <= from {
            continue;
        }
        match merged.last_mut() {
            Some(last) if (to - last.0) as f32 / decode::RATE as f32 <= max_secs => last.1 = to,
            _ => merged.push((from, to)),
        }
    }
    merged
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

/// Scrive un evento per Cedro. Se Cedro non c'è più (chiuso o caduto) nessuno legge: il motore esce
/// subito invece di finire da solo un file lungo, col processore al massimo e il modello in memoria.
fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{v}").and_then(|_| out.flush()).is_err() {
        std::process::exit(0);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_language_with_nul_is_an_error_not_a_panic() {
        let request: Value = serde_json::from_str(r#"{"lang":"it\u0000"}"#).unwrap();
        assert!(requested_language(&request).is_err());
        assert_eq!(requested_language(&json!({})).ok(), Some("auto"));
        assert_eq!(requested_language(&json!({"lang": "it"})).ok(), Some("it"));
    }

    #[test]
    fn speech_is_merged_in_chunks_under_the_limit() {
        let rate = decode::RATE as usize;
        // Tratti in centesimi di secondo: 0-10 s, 12-20 s, 21-30 s, 31-35 s, 60-70 s.
        let found = [(0.0, 1000.0), (1200.0, 2000.0), (2100.0, 3000.0), (3100.0, 3500.0), (6000.0, 7000.0)];
        let chunks = merge_speech(&found, 80 * rate, 29.0);
        // 0-20 s (il tratto 21-30 s farebbe 30 s), poi 21-35 s, poi 60-70 s: i silenzi tra i pezzi restano fuori.
        assert_eq!(chunks, vec![(0, 20 * rate), (21 * rate, 35 * rate), (60 * rate, 70 * rate)]);
        for (from, to) in &chunks {
            assert!((*to - *from) as f32 / rate as f32 <= 29.0);
        }
        assert!(merge_speech(&[], 10 * rate, 29.0).is_empty());
        // Un tratto oltre la fine dell'audio si ferma alla fine; uno vuoto sparisce.
        assert_eq!(merge_speech(&[(0.0, 5000.0), (900.0, 900.0)], 10 * rate, 29.0), vec![(0, 10 * rate)]);
    }

    #[test]
    fn speakers_in_the_request() {
        assert!(requested_speakers(&json!({})).is_none());
        assert!(requested_speakers(&json!({"speakers": 1})).is_none());
        assert!(matches!(requested_speakers(&json!({"speakers": 0})), Some(voci::Speakers::Auto)));
        assert!(matches!(requested_speakers(&json!({"speakers": 3})), Some(voci::Speakers::Exactly(3))));
        assert!(matches!(requested_speakers(&json!({"speakers": 40})), Some(voci::Speakers::Exactly(8))));
    }
}
