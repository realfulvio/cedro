//! Riassunto AI: dopo la trascrizione, Cedro scrive il riassunto della riunione (in breve, chi ha detto
//! cosa, le decisioni e le cose da fare) con Qwen3.5-4B, sul PC e senza internet.
//!
//! Il modello gira nel motore `cedro-riassumi` (llama.cpp in Rust, cartella `riassumi/`), un programma
//! a parte come quello della trascrizione: parte al clic e si chiude a fine riassunto, così la memoria
//! torna libera. Prima di partire si guarda la RAM libera: se non bastano 4 GB non parte, per non
//! bloccare il PC. Il modello (2,7 GB) si scarica solo la prima volta, con lo SHA-256 controllato.
//!
//! Qui c'è tutto quello che non è disegno: il motore, la lettura della risposta, il riassunto salvato
//! e come va nel Word e nel testo.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::trascrizione::{self, EngineKind, Event as TrEvent, Next, Running, Segment, Variant, CANCELLED};

/// Il nome del modello, come si scrive sotto il riassunto.
pub const MODEL_NAME: &str = "Qwen3.5-4B";
const MODEL_FILE: &str = "Qwen3.5-4B-Q4_K_M.gguf";
/// La versione Q4_K_M di Unsloth: la stessa provata su WS-ACCLUC il 3 novembre 2026 (Apache 2.0).
const MODEL_URL: &str = "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf";
pub const MODEL_MB: u32 = 2741;
/// La RAM libera che serve per partire. Misurato: il motore arriva a circa 3,6 GB.
pub const NEED_BYTES: u64 = 4_000_000_000;
/// Quanto occupa il riassunto mentre lavora, per la barra della memoria (misurato su WS-ACCLUC).
pub const USE_BYTES: u64 = 3_600_000_000;

const RIASSUMI: EngineKind = EngineKind { name: "cedro-riassumi", env_dir: "CEDRO_RIASSUMI_DIR" };

fn model_path() -> PathBuf {
    trascrizione::data_dir().join(MODEL_FILE)
}

fn sidecar() -> PathBuf {
    trascrizione::data_dir().join(format!("{MODEL_FILE}.sha256"))
}

/// Vero se il modello è già sul PC (scaricato e controllato): la prima volta si dice che va scaricato.
pub fn model_ready() -> bool {
    model_path().is_file() && sidecar().is_file()
}

/// Vero se il motore del riassunto è già sul PC (per i crediti).
pub fn engine_installed() -> bool {
    std::fs::read_dir(trascrizione::data_dir())
        .map(|d| {
            d.flatten().any(|e| {
                e.file_name().to_string_lossy().starts_with("cedro-riassumi-")
                    && match e.path().extension() {
                        Some(x) => x == "exe",
                        None => !cfg!(windows),
                    }
            })
        })
        .unwrap_or(false)
}

/// La RAM fisica del PC (per la barra della memoria).
pub fn total_memory() -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        return GlobalMemoryStatusEx(&mut m).ok().map(|_| m.ullTotalPhys);
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        return text.lines().find_map(|l| l.strip_prefix("MemTotal:")?.split_whitespace().next()?.parse::<u64>().ok()).map(|kb| kb * 1024);
    }
    #[allow(unreachable_code)]
    None
}

/// «9,2 GB»: i gigabyte con un decimale, come li legge una persona.
pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9).replace('.', ",")
}

// ---------------------------------------------------------------- la trascrizione per il modello

/// Le righe da dare al modello: «[00:12:40] Nome: testo». Le frasi di fila della stessa persona
/// diventano una riga sola (meno orari e nomi da leggere, quindi meno token e un riassunto più veloce).
pub fn lines_for(segments: &[Segment], names: &[String]) -> Vec<String> {
    const MAX_CHARS: usize = 700;
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<(Option<u8>, i64, usize)> = None;
    for s in segments {
        let text = s.text.trim();
        if text.is_empty() {
            continue;
        }
        let same = last.is_some_and(|(v, end, len)| v == s.speaker && s.t0 - end < 300 && len < MAX_CHARS);
        if same {
            let line = out.last_mut().expect("riga");
            line.push(' ');
            line.push_str(text);
        } else {
            let line = match trascrizione::name_of(names, s.speaker) {
                Some(name) => format!("[{}] {name}: {text}", trascrizione::clock(s.t0)),
                None => format!("[{}] {text}", trascrizione::clock(s.t0)),
            };
            out.push(line);
        }
        let len = out.last().map_or(0, |l| l.chars().count());
        last = Some((s.speaker, s.t1, len));
    }
    out
}

// ---------------------------------------------------------------- il lavoro

/// A che punto è.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Un altro lavoro pesante tiene il turno: si parte appena finisce.
    Waiting,
    /// Scarica il modello (la prima volta): il testo dice quanto.
    Download(String),
    /// Accende il motore e carica il modello.
    Loading,
    /// Legge la riunione: pezzo i (da 0) di n. Con n = 1 la legge in una volta sola.
    Reading(usize, usize),
    /// Stringe gli appunti (solo le riunioni lunghissime).
    Merging,
    Writing,
}

/// Quello che arriva mentre lavora.
#[derive(Clone, Debug)]
pub enum Progress {
    Step(Step),
    /// Il motore è pronto: su quale scheda video (None: il processore).
    Ready { gpu: Option<String> },
    Percent(u8),
    Done(Result<Stored, Failure>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// Poca RAM libera: quanta ce n'è.
    LowMemory(u64),
    Cancelled,
    Other(String),
}

/// Le scelte delle Impostazioni.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub gpu: bool,
    pub avx512: bool,
}

/// Fa il riassunto delle righe (`lines_for`). `names` sono i nomi delle voci di adesso: restano col
/// riassunto, così se poi si cambia un nome cambia anche lì. Manda tutto a `sink`, fino a `Done`.
pub fn run(lines: Vec<String>, names: Vec<String>, opts: Options, cancel: &AtomicBool, sink: &dyn Fn(Progress)) {
    let begin = Instant::now();
    let result = summarize(&lines, opts, cancel, sink).map(|(text, gpu)| Stored { text, names, ms: begin.elapsed().as_millis() as u64, gpu, done: Vec::new() });
    sink(Progress::Done(result));
}

fn summarize(lines: &[String], opts: Options, cancel: &AtomicBool, sink: &dyn Fn(Progress)) -> Result<(String, Option<String>), Failure> {
    if lines.is_empty() {
        return Err(Failure::Other("la trascrizione è vuota".into()));
    }
    let enough = |free: Option<u64>| free.is_none_or(|f| f >= NEED_BYTES);
    if let Some(free) = crate::resources::available_memory().filter(|f| !enough(Some(*f))) {
        return Err(Failure::LowMemory(free));
    }
    if crate::resources::busy_elsewhere() {
        sink(Progress::Step(Step::Waiting));
    }
    let Some(_job) = crate::resources::acquire(Some(cancel)) else { return Err(Failure::Cancelled) };
    // Dopo l'attesa la memoria può essere cambiata: si guarda di nuovo.
    if let Some(free) = crate::resources::available_memory().filter(|f| !enough(Some(*f))) {
        return Err(Failure::LowMemory(free));
    }
    let fail = |e: String| if e == CANCELLED { Failure::Cancelled } else { Failure::Other(e) };
    // I messaggi di download del modello e del motore arrivano come righe della trascrizione: diventano il passo «Scarico».
    let tr_sink = |e: TrEvent| {
        if let TrEvent::Log(line) = e {
            if line.text.starts_with("Scaricati") || line.text.contains("scarico") {
                sink(Progress::Step(Step::Download(line.text)));
            }
        }
    };
    let model = ensure_model(&tr_sink, cancel).map_err(fail)?;
    sink(Progress::Step(Step::Loading));
    let (mut running, mut variant, mut gpu) = power_on(&model, opts, &tr_sink, cancel).map_err(fail)?;
    sink(Progress::Ready { gpu: gpu.clone() });
    let request = json!({"lines": lines, "seed": seed()});
    loop {
        match one_pass(&mut running, &request, cancel, sink) {
            Pass::Done(text) => {
                running.stop();
                return Ok((text, gpu));
            }
            Pass::Cancelled => {
                running.kill();
                return Err(Failure::Cancelled);
            }
            Pass::Failed(e) => {
                running.stop();
                return Err(Failure::Other(e));
            }
            // Il driver della scheda video può cedere a metà (memoria condivisa finita): si riprova una
            // volta sul processore, con lo stesso modello.
            Pass::Died(_) if variant == Variant::Vulkan => {
                running.stop();
                sink(Progress::Step(Step::Loading));
                let cpu = Options { gpu: false, ..opts };
                let started = power_on(&model, cpu, &tr_sink, cancel).map_err(fail)?;
                (running, variant, gpu) = started;
                sink(Progress::Ready { gpu: None });
            }
            Pass::Died(e) => {
                running.stop();
                return Err(Failure::Other(e));
            }
        }
    }
}

enum Pass {
    Done(String),
    Cancelled,
    Failed(String),
    /// Il motore si è chiuso a metà.
    Died(String),
}

fn one_pass(running: &mut Running, request: &Value, cancel: &AtomicBool, sink: &dyn Fn(Progress)) -> Pass {
    if running.send(request.clone()).is_err() {
        return Pass::Died("il motore si è chiuso".into());
    }
    loop {
        let v = match running.next_or_cancel(cancel) {
            Next::Line(v) => v,
            Next::Cancelled => return Pass::Cancelled,
            Next::Gone => return Pass::Died("il motore si è chiuso a metà del riassunto".into()),
        };
        match v["e"].as_str().unwrap_or("") {
            "plan" if v["parts"].as_u64() == Some(1) => sink(Progress::Step(Step::Reading(0, 1))),
            "part" => sink(Progress::Step(Step::Reading(v["i"].as_u64().unwrap_or(0) as usize, v["n"].as_u64().unwrap_or(1) as usize))),
            "merge" => sink(Progress::Step(Step::Merging)),
            "final" => sink(Progress::Step(Step::Writing)),
            "progress" => sink(Progress::Percent(v["p"].as_u64().unwrap_or(0).min(100) as u8)),
            "end" => return Pass::Done(v["text"].as_str().unwrap_or("").trim().to_string()),
            // Sulla scheda video scriveva troppo piano (occupata da un altro programma): il motore è uscito
            // e si rifà sul processore, come quando il driver cede.
            "slow" => return Pass::Died(format!("scheda video occupata ({} token/s)", v["tps"].as_f64().unwrap_or(0.0))),
            "error" | "fatal" => return Pass::Failed(v["msg"].as_str().unwrap_or("il riassunto non è riuscito").to_string()),
            _ => {}
        }
    }
}

/// Un seme diverso a ogni riassunto: «Rifai» dà un testo nuovo.
fn seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() ^ d.as_secs() as u32).unwrap_or(42)
}

/// Il modello pronto sul PC: scaricato la prima volta, riscaricato se ne è uscita una versione nuova.
fn ensure_model(sink: &dyn Fn(TrEvent), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let path = model_path();
    let have = std::fs::read_to_string(sidecar()).ok().map(|s| s.trim().to_lowercase()).filter(|_| path.is_file());
    if let Some(local) = have {
        // Un controllo veloce: senza internet si usa quello che c'è.
        match trascrizione::published_sha(MODEL_URL, Duration::from_secs(5)) {
            Some(remote) if remote != local => trascrizione::log(sink, trascrizione::Tone::Step, "Modello", "è uscita una versione nuova, la scarico"),
            _ => return Ok(path),
        }
    } else {
        trascrizione::log(sink, trascrizione::Tone::Step, "Modello", "lo scarico la prima volta");
    }
    let expected = trascrizione::published_sha(MODEL_URL, Duration::from_secs(20));
    let sha = trascrizione::fetch(MODEL_URL, &path, expected.as_deref(), MODEL_MB, "il modello", sink, cancel)?;
    std::fs::write(sidecar(), &sha).map_err(|e| format!("{} non scritto: {e}", sidecar().display()))?;
    Ok(path)
}

/// Accende il motore: prima la versione per la scheda video (se c'è e le Impostazioni la vogliono),
/// poi quella per il processore più veloce che il PC sa eseguire.
fn power_on(model: &std::path::Path, opts: Options, sink: &dyn Fn(TrEvent), cancel: &AtomicBool) -> Result<(Running, Variant, Option<String>), String> {
    #[cfg(target_os = "android")]
    {
        let _ = (opts, sink);
        return android::power_on(model, cancel);
    }
    #[allow(unreachable_code)]
    let mut tries = Vec::new();
    if opts.gpu && trascrizione::real_gpu_present() {
        tries.push(Variant::Vulkan);
    }
    tries.push(trascrizione::cpu_variant(opts.avx512));
    let mut last = String::from("il motore non è partito");
    for (n, variant) in tries.iter().copied().enumerate() {
        let fallback_left = n + 1 < tries.len();
        let exe = match trascrizione::ensure_engine_of(RIASSUMI, variant, sink, cancel) {
            Ok(exe) => exe,
            Err(e) if e == CANCELLED => return Err(e),
            Err(e) if fallback_left => {
                last = e;
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut command = Command::new(&exe);
        command.arg("--model").arg(model).arg("--threads").arg(trascrizione::engine_threads().to_string());
        let running = Running::spawn(command)?;
        let ready = match running.next_or_cancel(cancel) {
            Next::Line(v) => v,
            Next::Gone => {
                running.stop();
                last = "il motore si è chiuso subito".into();
                continue;
            }
            Next::Cancelled => {
                running.kill();
                return Err(CANCELLED.into());
            }
        };
        if ready["e"] != "ready" {
            last = ready["msg"].as_str().unwrap_or("il motore non è partito").to_string();
            running.stop();
            continue;
        }
        let gpu = ready["gpu"].as_str().map(str::to_string);
        if variant == Variant::Vulkan && gpu.is_none() && fallback_left {
            // Nessuna scheda adatta (o troppo poca memoria video): la versione per processore è più veloce.
            running.stop();
            continue;
        }
        let threads = ready["threads"].as_u64().unwrap_or(0);
        match &gpu {
            Some(g) => crate::accel::set_in_use(true, crate::accel::gpu_model(g)),
            None => {
                let isa = variant.isa();
                crate::accel::set_in_use(false, if isa.is_empty() { format!("{threads} thread") } else { format!("{isa} · {threads} thread") });
            }
        }
        return Ok((running, variant, gpu));
    }
    Err(last)
}

/// Android: il motore `cedro-riassumi` è nell'APK, nella cartella delle librerie (l'unico posto da cui Android
/// lascia partire un programma), col nome `libcedro_riassumi.so`. Niente da scaricare tranne il modello.
#[cfg(target_os = "android")]
pub mod android {
    use super::*;
    use std::sync::OnceLock;

    static LIBS: OnceLock<PathBuf> = OnceLock::new();

    /// L'app dice dov'è la cartella delle librerie dell'APK (`nativeLibraryDir`).
    pub fn set_libs(dir: PathBuf) {
        let _ = LIBS.set(dir);
    }

    pub fn engine() -> Option<PathBuf> {
        LIBS.get().map(|d| d.join("libcedro_riassumi.so")).filter(|p| p.is_file())
    }

    pub(super) fn power_on(model: &std::path::Path, cancel: &AtomicBool) -> Result<(Running, Variant, Option<String>), String> {
        // Compilato con dotprod+fp16 come whisper: si controlla prima di farlo partire.
        const HWCAP_FPHP: u64 = 1 << 9;
        const HWCAP_ASIMDDP: u64 = 1 << 20;
        let caps = unsafe { libc::getauxval(libc::AT_HWCAP) } as u64;
        if caps & HWCAP_FPHP == 0 || caps & HWCAP_ASIMDDP == 0 {
            return Err("questo telefono non ha le istruzioni (dotprod, fp16) che il riassunto usa".into());
        }
        let exe = engine().ok_or("il motore del riassunto non è in questo APK")?;
        let mut command = Command::new(&exe);
        command.arg("--model").arg(model).arg("--threads").arg(trascrizione::engine_threads().to_string());
        let running = Running::spawn(command)?;
        match running.next_or_cancel(cancel) {
            Next::Line(v) if v["e"] == "ready" => {
                let threads = v["threads"].as_u64().unwrap_or(0);
                crate::accel::set_in_use(false, format!("{threads} thread"));
                Ok((running, trascrizione::cpu_variant(false), None))
            }
            Next::Line(v) => {
                running.stop();
                Err(v["msg"].as_str().unwrap_or("il motore non è partito").to_string())
            }
            Next::Gone => {
                running.stop();
                Err("il motore si è chiuso subito".into())
            }
            Next::Cancelled => {
                running.kill();
                Err(CANCELLED.into())
            }
        }
    }
}

// ---------------------------------------------------------------- il riassunto salvato

/// Il riassunto fatto, come resta nella revisione.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Stored {
    /// Il testo del modello, con i quattro titoli.
    pub text: String,
    /// I nomi delle voci quando è stato fatto: se poi si cambia un nome, nel riassunto cambia anche lui.
    pub names: Vec<String>,
    pub ms: u64,
    /// La scheda video usata (None: il processore).
    pub gpu: Option<String>,
    /// Le cose da fare spuntate.
    pub done: Vec<usize>,
}

impl Stored {
    pub fn to_json(&self) -> Value {
        json!({"text": self.text, "names": self.names, "ms": self.ms, "gpu": self.gpu, "done": self.done})
    }

    pub fn from_json(v: &Value) -> Option<Stored> {
        let text = v["text"].as_str()?.to_string();
        Some(Stored {
            text,
            names: v["names"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default(),
            ms: v["ms"].as_u64().unwrap_or(0),
            gpu: v["gpu"].as_str().map(str::to_string),
            done: v["done"].as_array().map(|a| a.iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect()).unwrap_or_default(),
        })
    }

    /// Il riassunto pronto da mostrare, con i nomi di adesso (`now`, uno per voce).
    pub fn sections(&self, now: &[String]) -> Sections {
        let raw = parse(&self.text);
        let rename = Renamer::new(&self.names, now);
        let who = raw
            .who
            .into_iter()
            .map(|(name, said)| {
                let voice = voice_of(&name, &self.names);
                let shown = voice.and_then(|v| now.get(v).cloned()).unwrap_or_else(|| rename.apply(&name));
                Said { voice, name: shown, text: rename.apply(&said) }
            })
            .collect();
        let todo = raw
            .todo
            .into_iter()
            .map(|(what, owner)| {
                let voice = owner.as_deref().and_then(|o| voice_of(o, &self.names));
                let owner = owner.map(|o| voice.and_then(|v| now.get(v).cloned()).unwrap_or_else(|| rename.apply(&o)));
                Todo { what: rename.apply(&what), voice, owner }
            })
            .collect();
        Sections { brief: rename.apply(&raw.brief), who, decisions: raw.decisions.iter().map(|d| rename.apply(d)).collect(), todo }
    }

    /// «2 min 40 s»: quanto ci ha messo.
    pub fn took(&self) -> String {
        let s = (self.ms + 500) / 1000;
        match s {
            0..=59 => format!("{s} s"),
            _ if s % 60 == 0 => format!("{} min", s / 60),
            _ => format!("{} min {} s", s / 60, s % 60),
        }
    }
}

/// Una riga di «Chi ha detto cosa».
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    /// La voce (indice nei nomi), se il nome è uno di quelli della trascrizione.
    pub voice: Option<usize>,
    pub name: String,
    pub text: String,
}

impl Said {
    /// «Nome: testo», o solo il testo se il modello non ha dato un nome (con una voce sola capita).
    pub fn line(&self) -> String {
        if self.name.trim().is_empty() { self.text.clone() } else { format!("{}: {}", self.name, self.text) }
    }
}

/// Una cosa da fare, con chi la fa.
#[derive(Clone, Debug, PartialEq)]
pub struct Todo {
    pub what: String,
    pub voice: Option<usize>,
    pub owner: Option<String>,
}

/// Il riassunto diviso nelle sue parti, con i nomi di adesso.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sections {
    pub brief: String,
    pub who: Vec<Said>,
    pub decisions: Vec<String>,
    pub todo: Vec<Todo>,
}

#[cfg(not(target_os = "android"))]
pub const WARNING: &str = "Riassunto automatico fatto sul PC: da controllare.";
#[cfg(target_os = "android")]
pub const WARNING: &str = "Riassunto automatico fatto sul telefono: da controllare.";

impl Sections {
    /// Il riassunto come testo semplice: per «Copia» e in cima al file di testo.
    pub fn plain(&self) -> String {
        let mut out = format!("RIASSUNTO\n{}\n", self.brief);
        if !self.who.is_empty() {
            out.push_str("\nChi ha detto cosa:\n");
            for w in &self.who {
                out.push_str(&format!("- {}\n", w.line()));
            }
        }
        out.push_str("\nDecisioni:\n");
        if self.decisions.is_empty() {
            out.push_str("- Nessuna\n");
        }
        for d in &self.decisions {
            out.push_str(&format!("- {d}\n"));
        }
        out.push_str("\nDa fare:\n");
        if self.todo.is_empty() {
            out.push_str("- Nessuno\n");
        }
        for t in &self.todo {
            match &t.owner {
                Some(o) => out.push_str(&format!("- {} ({o})\n", t.what)),
                None => out.push_str(&format!("- {}\n", t.what)),
            }
        }
        out.push_str(&format!("({WARNING})\n"));
        out
    }
}

/// Il nome scritto dal modello è una delle voci? Uguale (senza maiuscole e spazi), o contenuto
/// («Il Sindaco» per «Sindaco»); tra più voci che vanno bene, quella col nome più lungo.
fn voice_of(name: &str, names: &[String]) -> Option<usize> {
    let norm = |s: &str| s.trim().trim_matches(|c: char| c == '*' || c == '_' || c == '"').trim().to_lowercase();
    let n = norm(name);
    if n.is_empty() {
        return None;
    }
    if let Some(i) = names.iter().position(|x| norm(x) == n) {
        return Some(i);
    }
    names
        .iter()
        .enumerate()
        .filter(|(_, x)| {
            let x = norm(x);
            x.chars().count() >= 3 && (n.contains(&x) || x.contains(&n))
        })
        .max_by_key(|(_, x)| x.chars().count())
        .map(|(i, _)| i)
}

/// Sostituisce nel testo i nomi vecchi con quelli di adesso, tutti insieme (un nome nuovo uguale a un
/// altro vecchio non si cambia due volte).
struct Renamer {
    pairs: Vec<(String, String)>,
}

impl Renamer {
    fn new(old: &[String], now: &[String]) -> Renamer {
        let mut pairs: Vec<(String, String)> = old
            .iter()
            .zip(now)
            .filter(|(o, n)| o.trim().chars().count() >= 3 && !n.trim().is_empty() && o.trim() != n.trim())
            .map(|(o, n)| (o.trim().to_string(), n.trim().to_string()))
            .collect();
        // Il più lungo prima: «Interlocutore 1» non deve mangiare un pezzo di un nome che lo contiene.
        pairs.sort_by_key(|(o, _)| std::cmp::Reverse(o.chars().count()));
        Renamer { pairs }
    }

    fn apply(&self, text: &str) -> String {
        if self.pairs.is_empty() {
            return text.to_string();
        }
        let mut out = text.to_string();
        for (i, (old, _)) in self.pairs.iter().enumerate() {
            out = out.replace(old.as_str(), &format!("\u{E000}{i}\u{E001}"));
        }
        for (i, (_, new)) in self.pairs.iter().enumerate() {
            out = out.replace(&format!("\u{E000}{i}\u{E001}"), new);
        }
        out
    }
}

/// Il testo del modello diviso nelle quattro parti, con i nomi come li ha scritti lui.
#[derive(Debug, Default, PartialEq)]
struct Raw {
    brief: String,
    who: Vec<(String, String)>,
    decisions: Vec<String>,
    todo: Vec<(String, Option<String>)>,
}

/// Legge la risposta del modello. Regge le piccole varianti (titoli in grassetto, con i due punti, elenchi
/// con * o numeri); se non trova nessun titolo, tutto il testo va in «In breve», così non si perde nulla.
fn parse(text: &str) -> Raw {
    #[derive(Clone, Copy, PartialEq)]
    enum Part {
        None,
        Brief,
        Who,
        Decisions,
        Todo,
    }
    let mut raw = Raw::default();
    let mut part = Part::None;
    let mut found = false;
    let mut brief: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let title = t.trim_start_matches('#').trim().trim_matches('*').trim().trim_end_matches(':').trim().to_lowercase();
        let is_title = t.starts_with('#') || (t.starts_with("**") && t.ends_with("**")) || t.ends_with(':');
        if is_title {
            let next = match title.as_str() {
                "in breve" => Some(Part::Brief),
                "chi ha detto cosa" => Some(Part::Who),
                "decisioni" => Some(Part::Decisions),
                "da fare" => Some(Part::Todo),
                _ => None,
            };
            if let Some(next) = next {
                part = next;
                found = true;
                continue;
            }
        }
        let item = bullet(t);
        let clean = |s: &str| s.replace("**", "").trim().to_string();
        let none = |s: &str| {
            let l = s.trim().trim_end_matches('.').to_lowercase();
            l == "nessuna" || l == "nessuno" || l.starts_with("nessuna decisione") || l.starts_with("nessun compito") || l.starts_with("nessuna cosa")
        };
        match part {
            Part::None | Part::Brief => brief.push(clean(item.unwrap_or(t))),
            Part::Who => {
                let s = clean(item.unwrap_or(t));
                match s.split_once(':') {
                    Some((name, said)) if !name.trim().is_empty() && name.chars().count() <= 60 => raw.who.push((name.trim().to_string(), said.trim().to_string())),
                    _ => match raw.who.last_mut() {
                        // Una riga che continua la precedente.
                        Some(last) if item.is_none() => {
                            last.1.push(' ');
                            last.1.push_str(&s);
                        }
                        _ => raw.who.push((String::new(), s)),
                    },
                }
            }
            Part::Decisions => {
                let s = clean(item.unwrap_or(t));
                if !none(&s) {
                    raw.decisions.push(s);
                }
            }
            Part::Todo => {
                let s = clean(item.unwrap_or(t));
                if none(&s) {
                    continue;
                }
                raw.todo.push(owner_split(&s));
            }
        }
    }
    raw.brief = brief.join(" ");
    // Una persona sola per riga: il modello a volte la ripete (soprattutto dagli appunti a pezzi).
    let mut who: Vec<(String, String)> = Vec::with_capacity(raw.who.len());
    for (name, said) in std::mem::take(&mut raw.who) {
        let key = name.to_lowercase();
        match who.iter_mut().find(|(n, _)| !n.is_empty() && n.to_lowercase() == key) {
            Some(prev) if !said.is_empty() && !prev.1.contains(said.as_str()) => {
                let end = prev.1.trim_end().ends_with(['.', '!', '?']);
                prev.1 = format!("{}{} {said}", prev.1.trim_end(), if end { "" } else { "." });
            }
            Some(_) => {}
            None => who.push((name, said)),
        }
    }
    raw.who = who;
    raw.dedup_lists();
    if !found {
        raw.brief = text.trim().to_string();
    }
    raw
}

impl Raw {
    /// Toglie le decisioni e i compiti ripetuti uguali (succede unendo gli appunti dei pezzi).
    fn dedup_lists(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.decisions.retain(|d| seen.insert(d.to_lowercase()));
        let mut seen = std::collections::HashSet::new();
        self.todo.retain(|(w, o)| seen.insert((w.to_lowercase(), o.as_ref().map(|o| o.to_lowercase()))));
    }
}

/// Il testo di una riga d'elenco («- …», «* …», «• …», «1. …»), o None se non è un elenco.
fn bullet(t: &str) -> Option<&str> {
    for p in ["- ", "* ", "• ", "– "] {
        if let Some(rest) = t.strip_prefix(p) {
            return Some(rest.trim());
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 3 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(r.trim());
        }
    }
    None
}

/// «Aprire il cantiere (Assessore Neri)» → («Aprire il cantiere», Some(«Assessore Neri»)).
fn owner_split(s: &str) -> (String, Option<String>) {
    let t = s.trim().trim_end_matches('.').trim();
    if t.ends_with(')') {
        if let Some(open) = t.rfind('(') {
            let owner = t[open + 1..t.len() - 1].trim();
            let what = t[..open].trim().trim_end_matches([',', '-', '–', ':']).trim();
            if !owner.is_empty() && !what.is_empty() && owner.chars().count() <= 60 {
                return (what.to_string(), Some(owner.to_string()));
            }
        }
    }
    (s.trim().to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = "## In breve\nSeduta del Consiglio comunale. Approvati i verbali.\n\n## Chi ha detto cosa\n- Sindaco: Apre la seduta.\n- **Consigliera Bianchi**: Chiede dei 120 mila euro.\n- Assessore Neri: Fondi dall'avanzo 2025.\n\n## Decisioni\n- Verbali approvati all'unanimità.\n\n## Da fare\n- Aprire il cantiere entro il 3 novembre (Assessore Neri)\n";

    fn names() -> Vec<String> {
        ["Sindaco", "Segretaria comunale", "Consigliera Bianchi", "Assessore Neri"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_four_parts_are_read() {
        let s = Stored { text: ANSWER.into(), names: names(), ..Default::default() };
        let x = s.sections(&names());
        assert_eq!(x.brief, "Seduta del Consiglio comunale. Approvati i verbali.");
        assert_eq!(x.who.iter().map(|w| (w.voice, w.name.as_str())).collect::<Vec<_>>(), [(Some(0), "Sindaco"), (Some(2), "Consigliera Bianchi"), (Some(3), "Assessore Neri")]);
        assert_eq!(x.decisions, ["Verbali approvati all'unanimità."]);
        assert_eq!(x.todo, [Todo { what: "Aprire il cantiere entro il 3 novembre".into(), voice: Some(3), owner: Some("Assessore Neri".into()) }]);
    }

    #[test]
    fn a_name_changed_later_changes_in_the_summary() {
        let old: Vec<String> = trascrizione::default_names(2);
        let text = "## In breve\nInterlocutore 1 apre, Interlocutore 2 risponde.\n## Chi ha detto cosa\n- Interlocutore 1: Apre.\n- Interlocutore 2: Risponde a Interlocutore 1.\n## Decisioni\n- Nessuna\n## Da fare\n- Nessuno";
        let s = Stored { text: text.into(), names: old, ..Default::default() };
        // Scambiati apposta: «Interlocutore 2» diventa «Interlocutore 1» e viceversa, senza cambiarli due volte.
        let now = vec!["Interlocutore 2".to_string(), "Sindaco".to_string()];
        let x = s.sections(&now);
        assert_eq!(x.brief, "Interlocutore 2 apre, Sindaco risponde.");
        assert_eq!(x.who[1], Said { voice: Some(1), name: "Sindaco".into(), text: "Risponde a Interlocutore 2.".into() });
        assert!(x.decisions.is_empty() && x.todo.is_empty());
        assert!(x.plain().contains("Decisioni:\n- Nessuna\n"));
    }

    #[test]
    fn small_variations_and_no_titles() {
        let text = "**In breve:**\nUna riunione breve.\n**Chi ha detto cosa:**\n* Il Sindaco: saluta.\n**Decisioni:**\n1. Si rinvia.\n**Da fare:**\n- Mandare il verbale, entro venerdì";
        let x = parse(text);
        assert_eq!(x.brief, "Una riunione breve.");
        assert_eq!(voice_of(&x.who[0].0, &names()), Some(0));
        let twice = parse("## Chi ha detto cosa\n- Sindaco: apre la seduta\n- Bianchi: chiede i fondi\n- Sindaco: mette ai voti.\n## Decisioni\n- Approvato\n- Approvato\n## Da fare\n- Nessuno");
        assert_eq!(twice.who, [("Sindaco".to_string(), "apre la seduta. mette ai voti.".to_string()), ("Bianchi".to_string(), "chiede i fondi".to_string())]);
        assert_eq!(twice.decisions, ["Approvato"]);
        assert_eq!(x.decisions, ["Si rinvia."]);
        assert_eq!(x.todo, [("Mandare il verbale, entro venerdì".to_string(), None)]);
        assert_eq!(parse("Solo testo, senza titoli.").brief, "Solo testo, senza titoli.");
    }

    #[test]
    fn lines_join_the_same_speaker() {
        let seg = |t0: i64, v: u8, text: &str| Segment { t0, t1: t0 + 400, text: text.into(), speaker: Some(v) };
        let segs = [seg(400, 0, "Buonasera."), seg(900, 0, "Apro la seduta."), seg(1500, 1, "Grazie."), seg(9000, 1, "Dopo una pausa.")];
        let lines = lines_for(&segs, &names());
        assert_eq!(lines, ["[00:00:04] Sindaco: Buonasera. Apro la seduta.", "[00:00:15] Segretaria comunale: Grazie.", "[00:01:30] Segretaria comunale: Dopo una pausa."]);
        let plain = [Segment { t0: 0, t1: 100, text: "Ciao.".into(), speaker: None }];
        assert_eq!(lines_for(&plain, &[]), ["[00:00:00] Ciao."]);
    }

    #[test]
    fn stored_round_trip_and_time() {
        let s = Stored { text: "x".into(), names: names(), ms: 160_000, gpu: Some("Radeon".into()), done: vec![0] };
        assert_eq!(Stored::from_json(&s.to_json()), Some(s.clone()));
        assert_eq!(s.took(), "2 min 40 s");
        assert_eq!(Stored { ms: 34_200, ..s }.took(), "34 s");
    }
}
