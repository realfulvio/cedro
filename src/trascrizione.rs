//! Trascrizione audio: dalla voce a un Word o a un testo semplice, tutto sul PC.
//!
//! Il riconoscimento della voce non è dentro Cedro: lo fa `cedro-trascrivi` (whisper.cpp), un programma
//! a parte che si scarica dalla release di questa versione solo alla prima trascrizione, insieme al
//! modello scelto. Il motore parte quando la coda inizia e si chiude quando finisce: il modello esce
//! dalla memoria e il processore torna libero. Qui stanno il download, il dialogo con il motore, le
//! righe per la finestra «Cosa sta succedendo» e la scrittura del Word e del testo.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::engine::{self, Options};

/// Uno dei modelli di whisper che si possono scegliere in «Precisione».
pub struct Model {
    pub name: &'static str,
    /// A cosa serve, in una riga.
    pub note: &'static str,
    file: &'static str,
    /// Il peso, per dirlo prima di scaricare.
    pub megabytes: u32,
}

/// Su Android gli stessi modelli del computer (0.34): small, medium e large-v3-turbo quantizzati, misurati sul Mi 9T.
#[cfg(target_os = "android")]
pub const MODELS: &[Model] = &[
    Model { name: "Veloce", note: "Per appunti e messaggi vocali", file: "ggml-small-q8_0.bin", megabytes: 264 },
    Model { name: "Buona", note: "Per riunioni e interviste", file: "ggml-medium-q5_0.bin", megabytes: 539 },
    Model { name: "Massima", note: "Per audio difficili, la più precisa", file: "ggml-large-v3-turbo-q5_0.bin", megabytes: 574 },
];

#[cfg(not(target_os = "android"))]
pub const MODELS: &[Model] = &[
    // Scelti con la misura su 200 frasi italiane di FLEURS (errori sulle parole, in parlato pulito e in «messaggio vocale»
    // disturbato): small q8 7,5 % e 17 %, medium q5 4,1 % e 9 %, large-v3-turbo q5 2,6 % e 5,7 %. Il vecchio «Veloce»
    // (base) sbagliava 1 parola su 6.
    Model { name: "Veloce", note: "Per appunti e messaggi vocali", file: "ggml-small-q8_0.bin", megabytes: 264 },
    Model { name: "Buona", note: "Per riunioni e interviste", file: "ggml-medium-q5_0.bin", megabytes: 539 },
    // Provato anche large-v3 completo (0.34, mai uscito): sul vocale breve sbaglia qualche parola in meno, ma su 14 minuti
    // di frasi FLEURS entra in un giro e ripete la stessa frase per minuti (errori sulle parole 42-90 % contro 2,6 % del
    // turbo). Resta il turbo, che col prompt e la ricerca a fascio mette punteggiatura e non si incanta.
    Model { name: "Massima", note: "Per audio difficili, la più precisa", file: "ggml-large-v3-turbo-q5_0.bin", megabytes: 574 },
];

/// Il modello di partenza senza scheda video: «Buona».
pub const DEFAULT_MODEL: usize = 1;

/// Il livello preselezionato: «Massima» se il motore può usare la scheda video (un'ora di audio in circa due minuti),
/// altrimenti «Buona».
pub fn default_model() -> usize {
    if crate::settings::read_settings().gpu && real_gpu_present() {
        MODELS.len() - 1
    } else {
        DEFAULT_MODEL
    }
}

/// I modelli delle versioni precedenti (non più usati): si tolgono dal PC appena arriva quello nuovo.
const OLD_MODELS: &[&str] = &["ggml-base-q8_0.bin", "ggml-large-v3-turbo-q8_0.bin", "ggml-large-v3-q5_0.bin"];

fn forget_old_models() {
    let dir = data_dir();
    for file in OLD_MODELS {
        let _ = std::fs::remove_file(dir.join(file));
        let _ = std::fs::remove_file(dir.join(format!("{file}.sha256")));
    }
}

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
const RELEASE_TAG_URL: &str = "https://api.github.com/repos/realfulvio/cedro/releases/tags/";
/// Dalla 0.18.1 i motori stanno in una pre-release a parte («motori-v<versione>»), così la release
/// della versione mostra solo i tre pacchetti da scaricare. Le versioni prima li cercavano nella release «v<versione>».
const ENGINE_TAGS: [&str; 2] = ["motori-v", "v"];

/// In cosa si salva la trascrizione.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Word,
    Text,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Word => "docx",
            Format::Text => "txt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Word => "Word",
            Format::Text => "testo",
        }
    }
}

/// I formati che la scheda Trascrizione accetta oltre a quelli delle categorie Audio e Video:
/// i messaggi vocali (Opus, AMR), WMA, AIFF e simili.
const EXTRA_EXTENSIONS: &[&str] = &["opus", "oga", "weba", "wma", "amr", "3ga", "3gp", "aiff", "aif", "aifc", "caf", "m4b", "mka", "mpga", "mp2", "ac3", "wv"];

/// Vero se la scheda Trascrizione prende questo file.
pub fn transcribable(path: &Path) -> bool {
    matches!(engine::kind_of(path), engine::Kind::Audio | engine::Kind::Video) || EXTRA_EXTENSIONS.contains(&engine::extension(path).as_str())
}

/// Vero se il file entra solo nella trascrizione (per esempio un vocale .opus).
pub fn only_transcribable(path: &Path) -> bool {
    engine::kind_of(path) == engine::Kind::Other && transcribable(path)
}

/// Le scelte della scheda Trascrizione.
#[derive(Clone)]
pub struct Request {
    pub format: Format,
    pub model: usize,
    /// Dalle Impostazioni: usare la scheda video se c'è.
    pub gpu: bool,
    /// Dalle Impostazioni: usare le istruzioni AVX-512 se il processore le ha.
    pub avx512: bool,
    /// Quante persone parlano, dalla finestrella prima di trascrivere: 0 = le conta Cedro, 1 = una sola
    /// (niente riconoscimento delle voci), 2..=8 = esattamente quelle.
    pub speakers: u8,
    /// I termini che il motore deve aspettarsi (per esempio quelli dei referti veterinari): il nome di
    /// una raccolta pronta («veterinaria») o parole separate da virgole. `None` = nessuno.
    pub glossario: Option<String>,
}

/// Il tono di una riga della finestra «Cosa sta succedendo», per colorarla.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// Un passo (scarico, carico, salvo): parola chiave in azzurro.
    Step,
    /// Una frase trascritta, con il suo orario.
    Speech,
    /// Un avviso o l'avanzamento.
    Note,
    /// Un errore.
    Error,
    /// Una riga secondaria, grigia.
    Muted,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub tone: Tone,
    /// La prima parola, evidenziata (per esempio «Modello»); vuota se non c'è.
    pub key: String,
    pub text: String,
    /// Chi dice la frase (per le righe `Speech` con il riconoscimento delle voci acceso).
    pub speaker: Option<u8>,
}

/// Cosa la trascrizione racconta a chi la guarda.
#[derive(Debug)]
pub enum Event {
    Log(Line),
    /// Il file numero `index` della coda è a `percent`.
    Percent { index: usize, percent: u8 },
    /// Un file è finito: il file scritto e il messaggio per la coda, oppure l'errore.
    Finished { path: PathBuf, result: Result<(PathBuf, String), String> },
}

pub(crate) fn log(sink: &dyn Fn(Event), tone: Tone, key: &str, text: impl Into<String>) {
    sink(Event::Log(Line { tone, key: key.to_string(), text: text.into(), speaker: None }));
}

/// La cartella dei modelli e del motore: fuori dal profilo che si sposta in rete, perché pesa.
pub fn data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("Cedro").join("trascrizione");
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(data).join("cedro").join("trascrizione");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("cedro").join("trascrizione");
    }
    engine::config_dir().join("trascrizione")
}

/// Vero se il modello è già sul PC (scaricato e controllato).
pub fn model_ready(index: usize) -> bool {
    MODELS.get(index).is_some_and(|m| {
        let dir = data_dir();
        dir.join(m.file).is_file() && dir.join(format!("{}.sha256", m.file)).is_file()
    })
}

/// Il nome del modello di whisper, come si legge sul sito («small», «large-v3-turbo»), con il tipo di compressione.
pub fn model_names(index: usize) -> String {
    let file = MODELS.get(index).map(|m| m.file).unwrap_or("");
    let core = file.trim_start_matches("ggml-").trim_end_matches(".bin");
    match core.rsplit_once('-') {
        Some((name, quant)) => format!("{name} {quant}"),
        None => core.to_string(),
    }
}

/// I modelli già sul PC, per nome («base q8_0», «small q8_0»).
pub fn installed_models() -> Vec<String> {
    (0..MODELS.len()).filter(|i| model_ready(*i)).map(model_names).collect()
}

/// Vero se il motore (cedro-trascrivi) è già sul PC.
pub fn engine_installed() -> bool {
    std::fs::read_dir(data_dir())
        .map(|d| {
            d.flatten().any(|e| {
                // Su Linux il motore non ha estensione («cedro-trascrivi-avx2»); .version, .part e .sha256 non contano.
                e.file_name().to_string_lossy().starts_with("cedro-trascrivi-")
                    && match e.path().extension() {
                        Some(x) => x == "exe",
                        None => !cfg!(windows),
                    }
            })
        })
        .unwrap_or(false)
}

/// Il peso del modello scritto come lo legge una persona: «490 MB», «1,6 GB».
pub fn size_text(megabytes: u32) -> String {
    if megabytes >= 1000 {
        // Arrotondato ai decimi prima di dividere: 1960 MB è «2,0 GB», non «1,10 GB».
        let tenths = (megabytes + 50) / 100;
        format!("{},{} GB", tenths / 10, tenths % 10)
    } else {
        format!("{} MB", (megabytes + 5) / 10 * 10)
    }
}

/// Le versioni del motore: una per tipo di processore, più quella per la scheda video.
/// Quelle x86 e quelle ARM esistono tutte nel codice, ma ogni architettura ne sceglie solo le sue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub(crate) enum Variant {
    Base,
    Avx2,
    /// AVX2 con AVX-VNNI: Intel dalla 12ª generazione (che non ha AVX-512).
    AvxVnni,
    Avx512,
    /// AVX-512 con VNNI e VBMI: AMD Ryzen 7000 e successivi, Intel Ice Lake e successivi.
    Avx512Vnni,
    /// ARM con dotprod (Snapdragon X, Apple, Neoverse e quasi tutti gli ARM degli ultimi anni).
    Dotprod,
    /// ARM con dotprod e i8mm (Snapdragon X Elite, Cobalt 100 e più recenti).
    I8mm,
    Vulkan,
}

impl Variant {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Variant::Base => "base",
            Variant::Avx2 => "avx2",
            Variant::AvxVnni => "avxvnni",
            Variant::Avx512 => "avx512",
            Variant::Avx512Vnni => "avx512vnni",
            Variant::Dotprod => "dotprod",
            Variant::I8mm => "i8mm",
            Variant::Vulkan => "vulkan",
        }
    }

    /// Il nome dell'istruzione, per la spia nella finestra (vuoto: nessuna istruzione avanzata).
    pub(crate) fn isa(self) -> &'static str {
        match self {
            Variant::Base => "",
            Variant::Avx2 => "AVX2",
            Variant::AvxVnni => "AVX-VNNI",
            Variant::Avx512 => "AVX-512",
            Variant::Avx512Vnni => "AVX-512 VNNI",
            Variant::Dotprod => "dotprod",
            Variant::I8mm => "i8mm",
            Variant::Vulkan => "Vulkan",
        }
    }

    pub(crate) fn describe(self) -> &'static str {
        match self {
            Variant::Base => "per processore",
            Variant::Avx2 => "per processore, con AVX2",
            Variant::AvxVnni => "per processore, con AVX-VNNI",
            Variant::Avx512 => "per processore, con AVX-512",
            Variant::Avx512Vnni => "per processore, con AVX-512 VNNI",
            Variant::Dotprod => "per processore, con dotprod",
            Variant::I8mm => "per processore, con i8mm",
            Variant::Vulkan => "per scheda video",
        }
    }
}

/// La versione per processore più veloce che questo PC sa eseguire. AVX-512 (x86) e i8mm (ARM) solo se le Impostazioni lo permettono.
pub(crate) fn cpu_variant(avx512: bool) -> Variant {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::is_x86_feature_detected as has;
        let avx2 = has!("avx2") && has!("fma") && has!("f16c") && has!("bmi2");
        let full512 = has!("avx512f") && has!("avx512bw") && has!("avx512vl") && has!("avx512dq") && has!("avx512cd");
        if avx512 && avx2 && full512 {
            return if has!("avx512vnni") && has!("avx512vbmi") { Variant::Avx512Vnni } else { Variant::Avx512 };
        }
        if avx2 {
            return if has!("avxvnni") { Variant::AvxVnni } else { Variant::Avx2 };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        let hw = crate::accel::hardware();
        // i8mm solo se le Impostazioni lo permettono (come AVX-512); dotprod c'è su quasi tutti.
        if avx512 && hw.avx512 && hw.dotprod {
            return Variant::I8mm;
        }
        if hw.dotprod {
            return Variant::Dotprod;
        }
    }
    let _ = avx512;
    Variant::Base
}

/// Quanti thread dare al motore: i core fisici, e sui processori Intel ibridi solo i core P
/// (gli E-core restano al resto del PC). Al massimo 8: oltre, whisper non va più veloce.
pub fn engine_threads() -> usize {
    let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    #[cfg(target_os = "android")]
    {
        if let Some(n) = std::env::var("CEDRO_THREADS").ok().and_then(|n| n.parse::<usize>().ok()) {
            return n.clamp(1, 8); // per le misure
        }
        // Misurato sul Mi 9T (2 grandi + 6 piccoli): whisper va ~30 % più veloce con 7 thread che con i soli 2 core grandi
        // (ggml divide il lavoro a parti uguali, e i piccoli aiutano comunque); un core resta libero per l'interfaccia.
        if android_big_cores().is_some() {
            return logical.saturating_sub(1).clamp(2, 8);
        }
    }
    let cores = performance_cores().unwrap_or(logical / 2).max(1);
    cores.clamp(1, 8)
}

/// Android (big.LITTLE): i core «grandi» sono quelli con la capacità più alta; whisper va meglio solo su quelli.
#[cfg(target_os = "android")]
fn android_big_cores() -> Option<usize> {
    let mut cap = Vec::new();
    for i in 0..32 {
        let base = format!("/sys/devices/system/cpu/cpu{i}");
        let c = std::fs::read_to_string(format!("{base}/cpu_capacity")).or_else(|_| std::fs::read_to_string(format!("{base}/cpufreq/cpuinfo_max_freq")));
        match c.ok().and_then(|s| s.trim().parse::<u64>().ok()) {
            Some(v) => cap.push(v),
            None => break,
        }
    }
    let max = *cap.iter().max()?;
    let top = cap.iter().filter(|c| **c * 10 >= max * 9).count();
    (top < cap.len()).then_some(top.max(2))
}

#[cfg(windows)]
fn performance_cores() -> Option<usize> {
    use windows::Win32::System::SystemInformation::{GetLogicalProcessorInformationEx, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX};
    unsafe {
        let mut len = 0u32;
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        GetLogicalProcessorInformationEx(RelationProcessorCore, Some(buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX), &mut len).ok()?;
        let mut classes = Vec::new();
        let mut at = 0usize;
        while at + std::mem::size_of::<u32>() * 2 <= len as usize {
            let info = &*(buf.as_ptr().add(at) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
            if info.Size == 0 {
                break;
            }
            classes.push(info.Anonymous.Processor.EfficiencyClass);
            at += info.Size as usize;
        }
        let top = *classes.iter().max()?;
        Some(classes.iter().filter(|c| **c == top).count())
    }
}

#[cfg(not(windows))]
fn performance_cores() -> Option<usize> {
    // Sui processori ibridi Linux elenca a parte i core P («cpu_core») e gli E-core («cpu_atom»).
    let allowed: Option<Vec<usize>> = std::fs::read_to_string("/sys/devices/cpu_core/cpus").ok().map(|list| cpu_list(&list));
    let mut cores = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(n) = name.strip_prefix("cpu").and_then(|n| n.parse::<usize>().ok()) else { continue };
        if allowed.as_ref().is_some_and(|a| !a.contains(&n)) {
            continue;
        }
        if let Ok(siblings) = std::fs::read_to_string(entry.path().join("topology/core_cpus_list")) {
            cores.insert(siblings.trim().to_string());
        }
    }
    (!cores.is_empty()).then_some(cores.len())
}

#[cfg(not(windows))]
fn cpu_list(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in text.trim().split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    out.extend(a..=b);
                }
            }
            None => out.extend(part.parse::<usize>().ok()),
        }
    }
    out
}

/// Vero se il PC ha davvero una scheda video che il motore può usare: il caricatore Vulkan non basta
/// (una macchina virtuale o un PC vecchio può averlo senza scheda adatta).
pub(crate) fn real_gpu_present() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
        if !vulkan_loader_present() {
            return false;
        }
        // NVIDIA, AMD, Intel e Qualcomm (Adreno): le schede con driver Vulkan. Il resto (adattatori virtuali, software) no.
        const VENDORS: [u32; 5] = [0x10DE, 0x1002, 0x8086, 0x5143, 0x4D4F_4351];
        unsafe {
            let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return false };
            let mut index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(index) {
                index += 1;
                if let Ok(desc) = adapter.GetDesc1() {
                    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 && VENDORS.contains(&desc.VendorId) {
                        return true;
                    }
                }
            }
            false
        }
    }
    #[cfg(not(windows))]
    {
        vulkan_loader_present() && Path::new("/dev/dri").read_dir().is_ok_and(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("renderD")))
    }
}

/// Vero se il sistema ha il caricatore Vulkan (lo installano i driver della scheda video).
/// Senza, il motore per la scheda video non partirebbe nemmeno.
pub(crate) fn vulkan_loader_present() -> bool {
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::Foundation::FreeLibrary;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        unsafe {
            match LoadLibraryW(w!("vulkan-1.dll")) {
                Ok(module) => {
                    let _ = FreeLibrary(module);
                    true
                }
                Err(_) => false,
            }
        }
    }
    #[cfg(not(windows))]
    {
        let dirs = ["/usr/lib", "/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/lib/aarch64-linux-gnu", "/lib64", "/usr/local/lib", "/run/opengl-driver/lib"];
        dirs.iter().any(|d| Path::new(d).join("libvulkan.so.1").exists())
    }
}

/// I motori che Cedro scarica: il nome del programma e la variabile che, nelle prove, indica una
/// cartella con i motori già compilati.
#[derive(Clone, Copy)]
pub(crate) struct EngineKind {
    pub name: &'static str,
    pub env_dir: &'static str,
}

pub(crate) const TRASCRIVI: EngineKind = EngineKind { name: "cedro-trascrivi", env_dir: "CEDRO_TRASCRIVI_DIR" };

pub(crate) fn engine_exe(kind: EngineKind, variant: Variant) -> String {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    format!("{}-{}{suffix}", kind.name, variant.tag())
}

pub(crate) fn engine_asset(kind: EngineKind, variant: Variant) -> String {
    let os = if cfg!(windows) { "windows" } else { "linux" };
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    format!("{}-{}-{os}-{arch}-{}{suffix}", kind.name, crate::update::CURRENT, variant.tag())
}

fn http() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("cedro/{}", crate::update::CURRENT))
        .build()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// «di il modello» → «del modello»: la preposizione articolata giusta davanti a `what`.
pub(crate) fn di(what: &str) -> String {
    for (art, prep) in [("il ", "del "), ("lo ", "dello "), ("la ", "della "), ("i ", "dei "), ("gli ", "degli "), ("le ", "delle "), ("l'", "dell'")] {
        if let Some(rest) = what.strip_prefix(art) {
            return format!("{prep}{rest}");
        }
    }
    format!("di {what}")
}

/// Quante volte di fila si riprova senza che arrivi un byte nuovo, prima di arrendersi.
const FETCH_TRIES: u32 = 8;

/// Scarica `url` in `dest` passando da un file `.part`, con lo SHA-256 controllato se noto.
/// Se la rete cade (Wi-Fi che cambia, VPN, telefono che va in standby) riprende da dove era arrivato
/// (richiesta «Range») e riprova da solo; anche un download lasciato a metà la volta prima riparte da lì.
/// Ogni 10% scrive a che punto è. Ritorna lo SHA-256 del file scaricato.
pub(crate) fn fetch(url: &str, dest: &Path, expected_sha: Option<&str>, megabytes: u32, what: &str, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cartella {} non creata: {e}", dir.display()))?;
    }
    let part = dest.with_extension("part");
    // Quello che c'è già nel .part (download interrotto la volta prima) si tiene: si rilegge per lo SHA-256.
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    if let Ok(mut old) = std::fs::File::open(&part) {
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            match old.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    hasher.update(&buf[..n]);
                    done += n as u64;
                }
                Err(_) => {
                    hasher = Sha256::new();
                    done = 0;
                    break;
                }
            }
        }
    }
    let from_before = done > 0;
    let mut total = megabytes as u64 * 1_000_000;
    let mut known_total = false;
    let mut shown = 0u64;
    let mut misses = 0u32;
    let mut last_err = String::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.to_string());
        }
        if known_total && done >= total {
            break;
        }
        if misses > 0 {
            if misses >= FETCH_TRIES {
                // Il .part resta: al prossimo tentativo si riparte da qui.
                return Err(format!("download {} interrotto: {last_err}. Riprova: riparte da dove era arrivato", di(what)));
            }
            log(sink, Tone::Muted, "", format!("Rete interrotta, riprovo ({misses} di {})…", FETCH_TRIES - 1));
            let wait = Instant::now() + Duration::from_secs((1u64 << misses.min(5)).min(30));
            while Instant::now() < wait {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CANCELLED.to_string());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        let mut request = http().get(url);
        if done > 0 {
            request = request.set("Range", &format!("bytes={done}-"));
        }
        let response = match request.call() {
            Ok(r) => r,
            // Il file era già tutto nel .part.
            Err(ureq::Error::Status(416, _)) if done > 0 => break,
            Err(ureq::Error::Status(code, _)) if code < 500 && code != 408 && code != 429 => {
                return Err(format!("download {} non riuscito: errore {code} dal server", di(what)));
            }
            Err(e) => {
                last_err = e.to_string();
                misses += 1;
                continue;
            }
        };
        let resumed = response.status() == 206;
        if done > 0 && !resumed {
            // Il server non sa riprendere: si ricomincia da capo.
            done = 0;
            hasher = Sha256::new();
        }
        if let Some(t) = response.header("Content-Range").and_then(|v| v.rsplit('/').next()).and_then(|v| v.parse::<u64>().ok()) {
            (total, known_total) = (t, true);
        } else if let Some(len) = response.header("Content-Length").and_then(|v| v.parse::<u64>().ok()) {
            (total, known_total) = (len + if resumed { done } else { 0 }, true);
        }
        let file = if resumed { std::fs::OpenOptions::new().append(true).open(&part) } else { std::fs::File::create(&part) };
        let mut file = file.map_err(|e| format!("{} non scrivibile: {e}", part.display()))?;
        let mut reader = response.into_reader();
        let before = done;
        let outcome: Result<(), String> = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.to_string());
            }
            let n = match reader.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) => break Err(e.to_string()),
            };
            hasher.update(&buf[..n]);
            if let Err(e) = file.write_all(&buf[..n]) {
                return Err(format!("scrittura {} non riuscita (disco pieno?): {e}", di(what)));
            }
            done += n as u64;
            let tenth = done * 10 / total.max(1);
            if tenth > shown && tenth < 10 {
                shown = tenth;
                log(sink, Tone::Muted, "", format!("Scaricati {} di {}", size_text((done / 1_000_000) as u32), size_text((total / 1_000_000) as u32)));
            }
        };
        drop(file);
        if done > before {
            misses = 0;
        }
        match outcome {
            Ok(()) if !known_total || done >= total => break,
            Ok(()) => last_err = "la connessione si è chiusa prima della fine".into(),
            Err(e) => last_err = e,
        }
        misses += 1;
    }
    let sha = hex(&hasher.finalize());
    if let Some(expected) = expected_sha {
        if !sha.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            if from_before {
                // Il pezzo della volta prima era di un'altra versione del file: si riscarica tutto.
                return fetch(url, dest, expected_sha, megabytes, what, sink, cancel);
            }
            return Err(format!("{what} scaricato non corrisponde all'originale (SHA-256 diverso): scartato, riprova"));
        }
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| format!("{} non salvato: {e}", dest.display()))?;
    Ok(sha)
}

/// Lo SHA-256 del modello pubblicato: Hugging Face lo dice nell'intestazione, senza scaricare nulla.
fn published_model_sha(model: &Model, wait: Duration) -> Option<String> {
    published_sha(&format!("{MODEL_BASE_URL}{}", model.file), wait)
}

/// Lo SHA-256 di un file pubblicato su Hugging Face, dall'intestazione (senza scaricarlo).
pub(crate) fn published_sha(url: &str, wait: Duration) -> Option<String> {
    let agent = ureq::AgentBuilder::new().timeout(wait).redirects(0).user_agent(&format!("cedro/{}", crate::update::CURRENT)).build();
    let response = match agent.head(url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(_) => return None,
    };
    let tag = response.header("x-linked-etag").or_else(|| response.header("etag"))?;
    let tag = tag.trim().trim_start_matches("W/").trim_matches('"').to_lowercase();
    (tag.len() == 64 && tag.chars().all(|c| c.is_ascii_hexdigit())).then_some(tag)
}

/// Il modello pronto sul PC: scaricato la prima volta, riscaricato se ne è uscita una versione nuova.
fn ensure_model(index: usize, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let model = MODELS.get(index).ok_or("modello sconosciuto")?;
    let dir = data_dir();
    let path = dir.join(model.file);
    let sidecar = dir.join(format!("{}.sha256", model.file));
    let have = std::fs::read_to_string(&sidecar).ok().map(|s| s.trim().to_lowercase()).filter(|_| path.is_file());
    let updating = have.is_some();
    match have {
        Some(local) => {
            // Un controllo veloce: senza internet si usa quello che c'è.
            match published_model_sha(model, Duration::from_secs(5)) {
                Some(remote) if remote != local => {
                    log(sink, Tone::Step, "Modello", format!("«{}»: è uscita una versione nuova, la scarico ({})…", model.name, size_text(model.megabytes)));
                }
                _ => {
                    log(sink, Tone::Step, "Modello", format!("«{}» già sul {}", model.name, if cfg!(target_os = "android") { "telefono" } else { "PC" }));
                    return Ok(path);
                }
            }
        }
        None => log(sink, Tone::Step, "Modello", format!("«{}»: lo scarico la prima volta ({})…", model.name, size_text(model.megabytes))),
    }
    let expected = published_model_sha(model, Duration::from_secs(20));
    let url = format!("{MODEL_BASE_URL}{}", model.file);
    let sha = match fetch(&url, &path, expected.as_deref(), model.megabytes, "il modello", sink, cancel) {
        Ok(sha) => sha,
        // La versione nuova non è arrivata (rete che cade, disco pieno): quella di prima è intatta, perché si
        // scarica nel .part, e si usa; la prossima volta il download riparte da dove era arrivato.
        Err(e) if e != CANCELLED && updating && path.is_file() => {
            log(sink, Tone::Note, "Modello", format!("versione nuova non scaricata ({e}): uso quella che c'è"));
            return Ok(path);
        }
        Err(e) => return Err(e),
    };
    std::fs::write(&sidecar, &sha).map_err(|e| format!("{} non scritto: {e}", sidecar.display()))?;
    log(sink, Tone::Step, "Modello", format!("«{}» scaricato e controllato", model.name));
    forget_old_models();
    Ok(path)
}

/// Il motore pronto sul PC, nella versione giusta per questa versione di Cedro.
/// `CEDRO_TRASCRIVI_DIR` (per le prove) indica una cartella con i motori già compilati.
fn ensure_engine(variant: Variant, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    ensure_engine_of(TRASCRIVI, variant, sink, cancel)
}

/// Come `ensure_engine`, per un motore qualsiasi (trascrizione o riassunto).
pub(crate) fn ensure_engine_of(kind: EngineKind, variant: Variant, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let exe_name = |v| engine_exe(kind, v);
    let asset_name = |v| engine_asset(kind, v);
    if let Some(dir) = std::env::var_os(kind.env_dir) {
        let path = PathBuf::from(dir).join(exe_name(variant));
        return if path.is_file() { Ok(path) } else { Err(format!("{} non c'è", path.display())) };
    }
    let dir = data_dir();
    let path = dir.join(exe_name(variant));
    let stamp = dir.join(format!("{}.version", exe_name(variant)));
    if path.is_file() && std::fs::read_to_string(&stamp).is_ok_and(|v| v.trim() == crate::update::CURRENT) {
        return Ok(path);
    }
    let first = !path.is_file();
    log(
        sink,
        Tone::Step,
        "Motore",
        if first { format!("lo scarico la prima volta ({})…", variant.describe()) } else { format!("lo aggiorno alla versione {} ({})…", crate::update::CURRENT, variant.describe()) },
    );
    let (url, sha, megabytes) = release_asset(&asset_name(variant))?;
    fetch(&url, &path, Some(&sha), megabytes, "il motore", sink, cancel)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::write(&stamp, crate::update::CURRENT).map_err(|e| e.to_string())?;
    log(sink, Tone::Step, "Motore", "scaricato e controllato");
    Ok(path)
}

/// Indirizzo, SHA-256 e peso di un motore di questa versione: prima nella release dei motori, poi in quella della versione.
fn release_asset(name: &str) -> Result<(String, String, u32), String> {
    let mut last = String::new();
    for prefix in ENGINE_TAGS {
        match release_asset_in(&format!("{RELEASE_TAG_URL}{prefix}{}", crate::update::CURRENT), name) {
            Ok(found) => return Ok(found),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn release_asset_in(url: &str, name: &str) -> Result<(String, String, u32), String> {
    let response = match http().get(url).set("Accept", "application/vnd.github+json").call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return Err(format!("il motore di questa versione ({name}) non è ancora pubblicato: riprova tra qualche minuto")),
        Err(e) => return Err(format!("GitHub non risponde: {e}")),
    };
    let body = response.into_string().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    engine_in_release(&json, name)
}

fn engine_in_release(json: &serde_json::Value, name: &str) -> Result<(String, String, u32), String> {
    let asset = json["assets"]
        .as_array()
        .and_then(|list| list.iter().find(|a| a["name"].as_str() == Some(name)))
        .ok_or_else(|| format!("il motore di questa versione ({name}) non è ancora pubblicato: riprova tra qualche minuto"))?;
    let url = asset["browser_download_url"].as_str().ok_or("indirizzo del motore mancante")?.to_string();
    let sha = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).ok_or("la release non indica lo SHA-256 del motore")?.to_string();
    let megabytes = (asset["size"].as_u64().unwrap_or(0) / 1_000_000) as u32;
    Ok((url, sha, megabytes.max(1)))
}

/// Il messaggio di un lavoro fermato con Annulla (lo stesso delle conversioni).
pub(crate) const CANCELLED: &str = "Annullato";

/// Quello che arriva dal motore mentre si aspetta.
pub(crate) enum Next {
    Line(serde_json::Value),
    /// Si è premuto Annulla.
    Cancelled,
    /// Il motore si è chiuso.
    Gone,
}

/// Il motore acceso: il processo, dove scrivergli e da dove leggere le sue righe.
pub(crate) struct Running {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Receiver<serde_json::Value>,
    /// Android: il motore è un thread dell'APK, le richieste vanno per canale.
    #[cfg(target_os = "android")]
    requests: Option<std::sync::mpsc::Sender<serde_json::Value>>,
}

impl Running {
    fn start(exe: &Path, model: &Path, cpu: bool, voices: Option<&VoicePaths>) -> Result<Running, String> {
        let mut command = Command::new(exe);
        command.arg("--model").arg(model).arg("--threads").arg(engine_threads().to_string());
        if cpu {
            command.arg("--cpu");
        }
        if let Some((seg, emb)) = voices {
            command.arg("--voci-seg").arg(seg).arg("--voci-emb").arg(emb);
        }
        Running::spawn(command)
    }

    /// Avvia un motore già preparato con i suoi argomenti: righe JSON su stdin e stdout, priorità bassa.
    pub(crate) fn spawn(mut command: Command) -> Result<Running, String> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Nessuna finestra nera, e priorità sotto le finestre: il PC resta reattivo.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
            command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
        }
        let mut child = command.spawn().map_err(|e| format!("il motore non parte: {e}"))?;
        let stdout = child.stdout.take().ok_or("il motore non risponde")?;
        let stdin = child.stdin.take();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Running {
            child: Some(child),
            stdin,
            lines,
            #[cfg(target_os = "android")]
            requests: None,
        })
    }

    /// Android: whisper.cpp è dentro l'APK (src/whisper_android), niente eseguibile da scaricare.
    #[cfg(target_os = "android")]
    fn embedded(model: &Path, gpu: bool, voices: Option<&VoicePaths>) -> Running {
        let (requests, lines) = crate::whisper_android::spawn(model, engine_threads(), gpu, voices.cloned());
        Running { child: None, stdin: None, lines, requests: Some(requests) }
    }

    pub(crate) fn send(&mut self, value: serde_json::Value) -> Result<(), String> {
        #[cfg(target_os = "android")]
        if let Some(requests) = &self.requests {
            return requests.send(value).map_err(|_| "il motore si è chiuso".to_string());
        }
        let stdin = self.stdin.as_mut().ok_or("il motore si è chiuso")?;
        writeln!(stdin, "{value}").and_then(|_| stdin.flush()).map_err(|_| "il motore si è chiuso".to_string())
    }

    /// La prossima riga del motore; `None` se si è chiuso.
    #[cfg(target_os = "android")]
    fn next(&self) -> Option<serde_json::Value> {
        self.lines.recv().ok()
    }

    /// La prossima riga del motore, guardando intanto se si è premuto Annulla.
    pub(crate) fn next_or_cancel(&self, cancel: &AtomicBool) -> Next {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Next::Cancelled;
            }
            match self.lines.recv_timeout(Duration::from_millis(200)) {
                Ok(v) => return Next::Line(v),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Next::Gone,
            }
        }
    }

    /// Annulla: lo si ferma subito, senza aspettare un modello che sta ancora calcolando.
    pub(crate) fn kill(mut self) {
        #[cfg(target_os = "android")]
        drop(self.requests.take());
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Chiude stdin: il motore finisce e libera la memoria. Se non esce da solo, lo si ferma.
    pub(crate) fn stop(mut self) {
        #[cfg(target_os = "android")]
        drop(self.requests.take());
        drop(self.stdin.take());
        let Some(mut child) = self.child.take() else { return };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

// A panic in an event sink must not leave whisper using RAM/CPU after the job gate is released.
impl Drop for Running {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Un segmento di parlato: inizio e fine in centesimi di secondo.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Segment {
    pub t0: i64,
    pub t1: i64,
    pub text: String,
    /// Chi parla (0 = Interlocutore 1), se le voci sono state riconosciute.
    pub speaker: Option<u8>,
}

// ---------------------------------------------------------------- chi parla

/// Il nome di partenza di una voce: «Interlocutore 1», «Interlocutore 2»…
pub fn speaker_name(index: usize) -> String {
    format!("{} {}", crate::i18n::tr("Interlocutore"), index + 1)
}

/// Al massimo quante voci si riconoscono.
pub const MAX_VOICES: usize = 8;

/// I nomi di partenza per `n` voci.
pub fn default_names(n: usize) -> Vec<String> {
    (0..n).map(speaker_name).collect()
}

/// Un turno di parola trovato dal riconoscimento delle voci (centesimi di secondo).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    pub t0: i64,
    pub t1: i64,
    pub speaker: u8,
}

/// Chi dice la frase da `t0` a `t1`: la voce che parla di più in quell'intervallo. Se nessuna voce lo
/// copre (una frase nel silenzio), quella del turno più vicino.
pub fn speaker_of(turns: &[Turn], t0: i64, t1: i64) -> Option<u8> {
    if turns.is_empty() {
        return None;
    }
    let mut overlap = [0i64; 256];
    for t in turns {
        let o = t1.min(t.t1) - t0.max(t.t0);
        if o > 0 {
            overlap[t.speaker as usize] += o;
        }
    }
    let (best, amount) = overlap.iter().enumerate().max_by_key(|(_, o)| **o).map(|(i, o)| (i as u8, *o))?;
    if amount > 0 {
        return Some(best);
    }
    let mid = (t0 + t1) / 2;
    turns.iter().min_by_key(|t| if mid < t.t0 { t.t0 - mid } else { (mid - t.t1).max(0) }).map(|t| t.speaker)
}

/// I due modelli delle voci (pyannote-segmentation-3.0 e CAM++ di 3D-Speaker), dalle release di
/// sherpa-onnx. Lo SHA-256 è fissato qui: ogni versione di Cedro sa quali file vuole.
struct VoiceModel {
    /// Il nome sul PC.
    file: &'static str,
    url: &'static str,
    /// SHA-256 di quello che si scarica.
    sha: &'static str,
    megabytes: u32,
    /// Se si scarica un archivio .tar.bz2: il file da tirarne fuori e il suo SHA-256.
    inner: Option<(&'static str, &'static str)>,
}

const VOICE_SEGMENTATION: VoiceModel = VoiceModel {
    file: "voci-segmentazione-pyannote-3.0.onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
    sha: "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488",
    megabytes: 7,
    inner: Some(("sherpa-onnx-pyannote-segmentation-3-0/model.onnx", "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079")),
};

const VOICE_EMBEDDING: VoiceModel = VoiceModel {
    file: "voci-impronte-campplus-zh-en-advanced.onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
    sha: "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2",
    megabytes: 28,
    inner: None,
};

/// Il peso dei modelli delle voci, per dirlo prima di scaricarli.
pub const VOICE_MEGABYTES: u32 = 40;

fn voice_dir() -> PathBuf {
    data_dir().join("voci")
}

fn voice_model_ready(m: &VoiceModel) -> bool {
    let path = voice_dir().join(m.file);
    let want = m.inner.map_or(m.sha, |(_, sha)| sha);
    path.is_file() && std::fs::read_to_string(voice_dir().join(format!("{}.sha256", m.file))).is_ok_and(|s| s.trim().eq_ignore_ascii_case(want))
}

/// Vero se i modelli delle voci sono già sul PC.
pub fn voice_models_ready() -> bool {
    voice_model_ready(&VOICE_SEGMENTATION) && voice_model_ready(&VOICE_EMBEDDING)
}

/// Il file `name` dentro un archivio .tar.bz2 (tar «ustar» semplice, come quelli di sherpa-onnx).
fn untar_bz2(archive: &Path, name: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("archivio dei modelli non aperto: {e}"))?;
    let mut tar = bzip2::read::BzDecoder::new(std::io::BufReader::new(file));
    let mut header = [0u8; 512];
    loop {
        tar.read_exact(&mut header).map_err(|e| format!("archivio dei modelli rovinato: {e}"))?;
        if header.iter().all(|b| *b == 0) {
            return Err(format!("{name} non c'è nell'archivio"));
        }
        let field = |a: usize, b: usize| String::from_utf8_lossy(&header[a..b]).trim_end_matches('\0').trim().to_string();
        let mut entry = field(0, 100);
        let prefix = field(345, 500);
        if !prefix.is_empty() {
            entry = format!("{prefix}/{entry}");
        }
        let size = u64::from_str_radix(&field(124, 136), 8).map_err(|_| "archivio dei modelli rovinato (misura)".to_string())?;
        let padded = size.div_ceil(512) * 512;
        if entry == name {
            let mut data = vec![0u8; size as usize];
            tar.read_exact(&mut data).map_err(|e| format!("archivio dei modelli rovinato: {e}"))?;
            return Ok(data);
        }
        std::io::copy(&mut (&mut tar).take(padded), &mut std::io::sink()).map_err(|e| format!("archivio dei modelli rovinato: {e}"))?;
    }
}

fn ensure_voice_model(m: &VoiceModel, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let dir = voice_dir();
    let path = dir.join(m.file);
    if voice_model_ready(m) {
        return Ok(path);
    }
    let what = "i modelli delle voci";
    match m.inner {
        None => {
            let sha = fetch(m.url, &path, Some(m.sha), m.megabytes, what, sink, cancel)?;
            std::fs::write(dir.join(format!("{}.sha256", m.file)), sha).map_err(|e| e.to_string())?;
        }
        Some((inner, inner_sha)) => {
            let archive = dir.join(format!("{}.tar.bz2", m.file));
            fetch(m.url, &archive, Some(m.sha), m.megabytes, what, sink, cancel)?;
            let data = untar_bz2(&archive, inner);
            let _ = std::fs::remove_file(&archive);
            let data = data?;
            if !hex(&Sha256::digest(&data)).eq_ignore_ascii_case(inner_sha) {
                return Err("il modello delle voci estratto non corrisponde all'originale".into());
            }
            std::fs::write(&path, &data).map_err(|e| format!("{} non scritto: {e}", path.display()))?;
            std::fs::write(dir.join(format!("{}.sha256", m.file)), inner_sha).map_err(|e| e.to_string())?;
        }
    }
    Ok(path)
}

/// I modelli delle voci pronti sul PC: scaricati la prima volta (circa 35 MB).
fn ensure_voice_models(sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<(PathBuf, PathBuf), String> {
    if !voice_models_ready() {
        log(sink, Tone::Step, "Voci", format!("scarico la prima volta il riconoscimento delle voci ({})…", size_text(VOICE_MEGABYTES)));
    }
    let seg = ensure_voice_model(&VOICE_SEGMENTATION, sink, cancel)?;
    let emb = ensure_voice_model(&VOICE_EMBEDDING, sink, cancel)?;
    Ok((seg, emb))
}

/// «00:12:40» da centesimi di secondo.
pub fn clock(centis: i64) -> String {
    let secs = centis.max(0) / 100;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Il tempo che manca, senza spazi (il riquadro del registro andrebbe a capo tra il numero e l'unità).
fn minutes_text(secs: f64) -> String {
    let mins = (secs / 60.0).round() as u64;
    match mins {
        0 => format!("{} s", secs.round() as u64),
        1..=59 => format!("{mins} min"),
        _ => format!("{} h {:02} min", mins / 60, mins % 60),
    }
}

/// Il nome della lingua in italiano, dal codice di whisper.
pub fn language_name(code: &str) -> String {
    let name = match code {
        "it" => "italiano",
        "en" => "inglese",
        "fr" => "francese",
        "de" => "tedesco",
        "es" => "spagnolo",
        "pt" => "portoghese",
        "ro" => "rumeno",
        "sq" => "albanese",
        "ar" => "arabo",
        "zh" => "cinese",
        "uk" => "ucraino",
        "ru" => "russo",
        "pl" => "polacco",
        "nl" => "olandese",
        "hi" => "hindi",
        "ur" => "urdu",
        "pa" => "punjabi",
        "bn" => "bengalese",
        "tr" => "turco",
        "el" => "greco",
        "sr" => "serbo",
        "hr" => "croato",
        "sl" => "sloveno",
        "ja" => "giapponese",
        "ko" => "coreano",
        other => return other.to_string(),
    };
    name.to_string()
}

/// Lingua per il correttore di Word, dal codice di whisper.
fn word_language(code: &str) -> String {
    match code {
        "it" => "it-IT".into(),
        "en" => "en-GB".into(),
        "fr" => "fr-FR".into(),
        "de" => "de-DE".into(),
        "es" => "es-ES".into(),
        "pt" => "pt-PT".into(),
        other => other.into(),
    }
}

/// Un paragrafo del documento: da quando, chi parla (se le voci sono riconosciute) e il testo.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub t0: i64,
    pub speaker: Option<u8>,
    pub text: String,
}

/// I segmenti raggruppati in paragrafi: si va a capo quando cambia chi parla, dopo una pausa o quando
/// il paragrafo è lungo.
pub fn paragraphs(segments: &[Segment]) -> Vec<Paragraph> {
    let mut out: Vec<Paragraph> = Vec::new();
    let mut last_end = 0i64;
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let pause = seg.t0 - last_end;
        let start_new = match out.last() {
            None => true,
            Some(p) => {
                let len = p.text.chars().count();
                let sentence_ended = p.text.ends_with(['.', '?', '!', '…']);
                p.speaker != seg.speaker || pause >= 150 || (len > 600 && sentence_ended) || len > 1200
            }
        };
        if start_new {
            out.push(Paragraph { t0: seg.t0, speaker: seg.speaker, text: text.to_string() });
        } else if let Some(p) = out.last_mut() {
            p.text.push(' ');
            p.text.push_str(text);
        }
        last_end = seg.t1;
    }
    out
}

/// Il nome di chi parla in un paragrafo: dai nomi dati, altrimenti «Interlocutore N».
pub fn name_of(names: &[String], speaker: Option<u8>) -> Option<String> {
    let i = speaker? as usize;
    if names.is_empty() {
        return None;
    }
    Some(names.get(i).map(|n| n.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| speaker_name(i)))
}

/// Quanto ha parlato ognuna delle voci (secondi), nell'ordine dei nomi.
pub fn talk_time(segments: &[Segment], voices: usize) -> Vec<f64> {
    let mut out = vec![0.0; voices];
    for s in segments {
        if let Some(v) = s.speaker.map(|v| v as usize).filter(|v| *v < voices) {
            out[v] += (s.t1 - s.t0).max(0) as f64 / 100.0;
        }
    }
    out
}

/// I colori delle voci nel Word (gli stessi dei temi chiari di Cedro).
pub const VOICE_COLORS: [&str; 8] = ["2563B8", "D4570E", "1E8A55", "B0308F", "A87A0C", "0D7C8F", "C8323C", "5B4BD1"];

/// Le voci che compaiono davvero nelle frasi (dopo le correzioni qualcuna può sparire).
fn used_voices(segments: &[Segment], names: &[String]) -> Vec<usize> {
    (0..names.len()).filter(|v| segments.iter().any(|s| s.speaker == Some(*v as u8))).collect()
}

/// Testo per l'XML di Word: i caratteri speciali escapati, quelli vietati in XML 1.0 (controllo) tolti.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// La data di oggi scritta per esteso («26 ottobre 2026»), dall'orologio del PC.
fn today_text() -> String {
    let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 86_400) as i64;
    // Da giorni dal 1970 a data civile (algoritmo di Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
    format!("{day} {} {year}", crate::i18n::tr(MONTHS[(month - 1) as usize]))
}

/// Intestazione comune: nome del file, data, durata e lingua.
fn heading(source: &Path, secs: f64, lang: &str) -> (String, String) {
    let title = source.file_stem().and_then(|s| s.to_str()).map(str::to_string).unwrap_or_else(|| crate::i18n::tr_string("Trascrizione"));
    // Il documento è nella lingua dell'interfaccia: la data e il nome della lingua sono già tradotti.
    let meta = crate::i18n::tr_string(&format!("Trascrizione del {} · {} di audio · lingua: {}", today_text(), minutes_text(secs), crate::i18n::tr(&language_name(lang))));
    (title, meta)
}

/// Il testo semplice. Con `names` (voci riconosciute) ogni paragrafo ha il nome di chi parla dopo l'orario.
pub fn text_bytes(source: &Path, secs: f64, lang: &str, segments: &[Segment], names: &[String]) -> Vec<u8> {
    text_bytes_with(source, secs, lang, segments, names, None)
}

/// Come `text_bytes`, con il Riassunto AI in cima (prima dell'elenco di chi parla).
pub fn text_bytes_with(source: &Path, secs: f64, lang: &str, segments: &[Segment], names: &[String], summary: Option<&crate::riassunto::Sections>) -> Vec<u8> {
    let (title, meta) = heading(source, secs, lang);
    let mut out = format!("{title}\n{meta}\n");
    if let Some(sum) = summary {
        out.push('\n');
        out.push_str(&sum.plain());
        out.push('\n');
    }
    let used = used_voices(segments, names);
    if !used.is_empty() {
        let list: Vec<String> = used.iter().filter_map(|v| name_of(names, Some(*v as u8))).collect();
        out.push_str(&format!("{}: {}\n", crate::i18n::tr("Interlocutori"), list.join(", ")));
    }
    out.push('\n');
    for p in paragraphs(segments) {
        match name_of(names, p.speaker) {
            Some(name) => out.push_str(&format!("[{}] {name}: {}\n\n", clock(p.t0), p.text)),
            None => out.push_str(&format!("[{}] {}\n\n", clock(p.t0), p.text)),
        }
    }
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    out.trim_end().replace('\n', eol).into_bytes()
}

/// Il Word. Con `names` (voci riconosciute) in cima c'è la legenda delle voci con quanto ha parlato
/// ognuna, e ogni paragrafo comincia col nome di chi parla, nel suo colore.
pub fn docx_bytes(source: &Path, secs: f64, lang: &str, segments: &[Segment], names: &[String]) -> Vec<u8> {
    docx_bytes_with(source, secs, lang, segments, names, None)
}

/// Il Riassunto AI nel Word: un riquadro azzurro con il bordo blu a sinistra, prima della legenda delle voci.
/// I paragrafi di fila con lo stesso bordo e lo stesso fondo Word li unisce in un riquadro solo.
fn docx_summary(sum: &crate::riassunto::Sections) -> String {
    let para = |style: &str, runs: String| format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>{runs}</w:p>");
    let run = |text: &str| format!("<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>", xml_escape(text));
    let bold = |text: &str, color: Option<&str>| {
        let c = color.map(|c| format!("<w:color w:val=\"{c}\"/>")).unwrap_or_default();
        format!("<w:r><w:rPr><w:b/>{c}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", xml_escape(text))
    };
    let item = |runs: String| para("SummaryItem", format!("<w:r><w:t xml:space=\"preserve\">•\t</w:t></w:r>{runs}"));
    let color = |v: Option<usize>| v.map(|v| VOICE_COLORS[v % VOICE_COLORS.len()]);
    let mut out = para("SummaryTitle", bold(&crate::i18n::tr("Riassunto"), None));
    out.push_str(&para("Summary", run(&sum.brief)));
    if !sum.who.is_empty() {
        out.push_str(&para("SummaryHead", bold(&crate::i18n::tr("Chi ha detto cosa"), None)));
        for w in &sum.who {
            out.push_str(&item(format!("{}{}", bold(&w.name, color(w.voice)), run(&format!(": {}", w.text)))));
        }
    }
    out.push_str(&para("SummaryHead", bold(&crate::i18n::tr("Decisioni"), None)));
    if sum.decisions.is_empty() {
        out.push_str(&item(run(&crate::i18n::tr("Nessuna"))));
    }
    for d in &sum.decisions {
        out.push_str(&item(run(d)));
    }
    out.push_str(&para("SummaryHead", bold(&crate::i18n::tr("Da fare"), None)));
    if sum.todo.is_empty() {
        out.push_str(&item(run(&crate::i18n::tr("Nessuno"))));
    }
    for t in &sum.todo {
        let owner = t.owner.as_ref().map(|o| format!("{}{}{}", run(" ("), bold(o, color(t.voice)), run(")"))).unwrap_or_default();
        out.push_str(&item(format!("{}{owner}", run(&t.what))));
    }
    out.push_str(&para("SummaryNote", run(&crate::i18n::tr(crate::riassunto::WARNING))));
    out
}

/// Come `docx_bytes`, con il Riassunto AI in cima (prima della legenda delle voci).
pub fn docx_bytes_with(source: &Path, secs: f64, lang: &str, segments: &[Segment], names: &[String], summary: Option<&crate::riassunto::Sections>) -> Vec<u8> {
    let (title, meta) = heading(source, secs, lang);
    let mut body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Meta\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(&title),
        xml_escape(&meta)
    );
    if let Some(sum) = summary {
        body.push_str(&docx_summary(sum));
    }
    let used = used_voices(segments, names);
    if !used.is_empty() {
        let talk = talk_time(segments, names.len());
        let mut legend = String::from("<w:p><w:pPr><w:pStyle w:val=\"Voices\"/></w:pPr>");
        for (k, v) in used.iter().enumerate() {
            let color = VOICE_COLORS[v % VOICE_COLORS.len()];
            let name = name_of(names, Some(*v as u8)).unwrap_or_default();
            if k > 0 {
                legend.push_str("<w:r><w:t xml:space=\"preserve\">     </w:t></w:r>");
            }
            legend.push_str(&format!(
                "<w:r><w:rPr><w:color w:val=\"{color}\"/></w:rPr><w:t>■ </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{} </w:t></w:r><w:r><w:rPr><w:color w:val=\"5D7080\"/></w:rPr><w:t>{}</w:t></w:r>",
                xml_escape(&name),
                xml_escape(&minutes_text(talk[*v]))
            ));
        }
        legend.push_str("</w:p>");
        body.push_str(&legend);
    }
    for p in paragraphs(segments) {
        let who = match (name_of(names, p.speaker), p.speaker) {
            (Some(name), Some(v)) => format!(
                "<w:r><w:rPr><w:b/><w:color w:val=\"{}\"/></w:rPr><w:t xml:space=\"preserve\">{}  </w:t></w:r>",
                VOICE_COLORS[v as usize % VOICE_COLORS.len()],
                xml_escape(&name)
            ),
            _ => String::new(),
        };
        body.push_str(&format!(
            "<w:p><w:r><w:rPr><w:rStyle w:val=\"Time\"/></w:rPr><w:t xml:space=\"preserve\">[{}]  </w:t></w:r>{who}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            clock(p.t0),
            xml_escape(&p.text)
        ));
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let word_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
</Relationships>";
    // Calibri 11 con la lingua dell'audio per il correttore; titolo blu del Comune, orari grigi.
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:lang w:val=\"{lang}\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:spacing w:after=\"60\"/></w:pPr><w:rPr><w:b/><w:color w:val=\"005E8E\"/><w:sz w:val=\"36\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Meta\"><w:name w:val=\"Meta\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
<w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"6\" w:color=\"D6DEE5\"/></w:pBdr><w:spacing w:after=\"280\"/></w:pPr><w:rPr><w:color w:val=\"5D7080\"/><w:sz w:val=\"20\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Voices\"><w:name w:val=\"Interlocutori\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
<w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"6\" w:color=\"D6DEE5\"/></w:pBdr><w:spacing w:after=\"280\"/></w:pPr><w:rPr><w:sz w:val=\"20\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Summary\"><w:name w:val=\"Riassunto\"/><w:basedOn w:val=\"Normal\"/>\
<w:pPr><w:pBdr><w:left w:val=\"single\" w:sz=\"24\" w:space=\"8\" w:color=\"1F3864\"/></w:pBdr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F4F7FB\"/>\
<w:spacing w:after=\"80\"/><w:ind w:left=\"170\" w:right=\"170\"/></w:pPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"SummaryTitle\"><w:name w:val=\"Riassunto titolo\"/><w:basedOn w:val=\"Summary\"/><w:next w:val=\"Summary\"/>\
<w:pPr><w:spacing w:before=\"0\" w:after=\"60\"/></w:pPr><w:rPr><w:color w:val=\"1F3864\"/><w:sz w:val=\"26\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"SummaryHead\"><w:name w:val=\"Riassunto parte\"/><w:basedOn w:val=\"Summary\"/><w:next w:val=\"SummaryItem\"/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"80\" w:after=\"40\"/></w:pPr><w:rPr><w:sz w:val=\"21\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"SummaryItem\"><w:name w:val=\"Riassunto elenco\"/><w:basedOn w:val=\"Summary\"/>\
<w:pPr><w:tabs><w:tab w:val=\"left\" w:pos=\"454\"/></w:tabs><w:spacing w:after=\"40\"/><w:ind w:left=\"454\" w:hanging=\"284\"/></w:pPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"SummaryNote\"><w:name w:val=\"Riassunto avviso\"/><w:basedOn w:val=\"Summary\"/>\
<w:pPr><w:spacing w:before=\"80\" w:after=\"320\"/></w:pPr><w:rPr><w:color w:val=\"777777\"/><w:sz w:val=\"18\"/></w:rPr></w:style>\
<w:style w:type=\"character\" w:styleId=\"Time\"><w:name w:val=\"Orario\"/><w:rPr><w:color w:val=\"7D93A3\"/><w:sz w:val=\"18\"/></w:rPr></w:style>\
</w:styles>",
        lang = word_language(lang)
    );
    crate::zipstore::zip_archive(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", word_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ])
}

/// Un audio che il motore non sa leggere, passato prima dal sistema: Media Foundation su Windows,
/// ffmpeg o GStreamer su Linux. Ritorna un WAV temporaneo.
fn system_decode(input: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    // In una cartella nuova e solo nostra: su Linux /tmp è di tutti e ffmpeg scrive con -y.
    let dir = crate::engine::private_temp_dir("cedro-trascrivi")?;
    let temp = dir.join("audio.wav");
    #[cfg(windows)]
    {
        let _ = cancel;
        if let Err(e) = crate::video::audio_to_wav(input, &temp) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        Ok(temp)
    }
    #[cfg(not(windows))]
    {
        // Anche l'ffmpeg che Cedro ha scaricato per YouTube, se sul sistema non c'è.
        let ffmpeg_bin = crate::youtube::ffmpeg_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
        let ffmpeg = wait_or_cancel(
            Command::new(ffmpeg_bin).args(["-nostdin", "-loglevel", "error", "-y", "-i"]).arg(input).args(["-vn", "-ac", "1", "-ar", "16000"]).arg(&temp),
            cancel,
        );
        if cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(CANCELLED.into());
        }
        if ffmpeg && temp.is_file() {
            return Ok(temp);
        }
        let location = format!("location={}", input.display());
        let out = format!("location={}", temp.display());
        let gst = wait_or_cancel(
            Command::new("gst-launch-1.0").args(["-q", "filesrc", &location, "!", "decodebin", "!", "audioconvert", "!", "audioresample", "!", "audio/x-raw,rate=16000,channels=1", "!", "wavenc", "!", "filesink", &out]),
            cancel,
        );
        if gst && temp.metadata().is_ok_and(|m| m.len() > 44) {
            return Ok(temp);
        }
        let _ = std::fs::remove_dir_all(&dir);
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        Err("formato audio non supportato: installa ffmpeg per leggerlo".into())
    }
}

/// Esegue un programma di conversione e lo aspetta; con Annulla lo ferma. Vero se è finito bene.
#[cfg(not(windows))]
fn wait_or_cancel(cmd: &mut Command, cancel: &AtomicBool) -> bool {
    let Ok(mut child) = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Quello che il motore dice su un file, fino alla fine.
enum Outcome {
    /// `voices`: quante voci diverse sono state riconosciute (0 = nessun riconoscimento).
    Done { lang: String, secs: f64, segments: Vec<Segment>, voices: usize },
    Unsupported(String),
    Failed(String),
    /// Il motore si è chiuso (o non riceve più): va riacceso per i file che restano.
    EngineDied(String),
    /// Si è premuto Annulla.
    Cancelled,
}

/// Trascrive un file con il motore già acceso, raccontando i passi.
fn one_file(engine: &mut Running, path: &Path, index: usize, speakers: u8, glossario: Option<&str>, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Outcome {
    // Il percorso va al motore in JSON, cioè in UTF-8: un nome con byte non validi (Linux) arriverebbe storpiato
    // e il file «non si apre». Lo si fa leggere al sistema, che riceve il percorso com'è, in un WAV dal nome semplice.
    if path.to_str().is_none() {
        return Outcome::Unsupported("nome del file non in UTF-8".into());
    }
    let mut request = serde_json::json!({"file": path.to_string_lossy(), "lang": "auto"});
    if speakers != 1 {
        request["speakers"] = speakers.into();
    }
    if let Some(g) = glossario.map(str::trim).filter(|g| !g.is_empty()) {
        request["glossario"] = g.into();
    }
    if let Err(e) = engine.send(request) {
        return Outcome::EngineDied(e);
    }
    let mut segments = Vec::new();
    let mut turns: Vec<Turn> = Vec::new();
    let mut voices = 0usize;
    let mut voice_shown = 0u8;
    let mut secs = 0.0;
    let mut shown = 0;
    let mut first: Option<(Instant, u8)> = None;
    loop {
        let event = match engine.next_or_cancel(cancel) {
            Next::Line(event) => event,
            Next::Cancelled => return Outcome::Cancelled,
            Next::Gone => return Outcome::EngineDied("il motore si è chiuso all'improvviso".into()),
        };
        match event["e"].as_str().unwrap_or("") {
            "audio" => {
                secs = event["secs"].as_f64().unwrap_or(0.0);
                log(sink, Tone::Step, "Audio", format!("{} di parlato da trascrivere", minutes_text(secs)));
            }
            "lang" => log(sink, Tone::Step, "Lingua", format!("riconosciuta: {}", language_name(event["code"].as_str().unwrap_or("?")))),
            "voices_start" => log(sink, Tone::Step, "Voci", "riconosco chi parla…"),
            "voices_progress" => {
                let p = event["p"].as_u64().unwrap_or(0).min(100) as u8;
                if p / 25 > voice_shown && p < 100 {
                    voice_shown = p / 25;
                    log(sink, Tone::Muted, "", format!("Voci: {p}%"));
                }
            }
            "voices" => {
                turns = event["turns"]
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(|t| Some(Turn { t0: t[0].as_i64()?, t1: t[1].as_i64()?, speaker: t[2].as_u64()?.min(255) as u8 }))
                            .collect()
                    })
                    .unwrap_or_default();
                voices = turns.iter().map(|t| t.speaker as usize + 1).max().unwrap_or(0);
                let secs = event["ms"].as_f64().unwrap_or(0.0) / 1000.0;
                let text = match voices {
                    0 => "nessuna voce chiara: trascrivo senza nomi".to_string(),
                    1 => format!("una sola voce ({secs:.0} s)"),
                    n => format!("trovate {n} voci in {} ({secs:.0} s)", minutes_text(event_secs(&turns))),
                };
                log(sink, Tone::Step, "Voci", text);
            }
            "voices_error" => log(sink, Tone::Note, "Voci", format!("non riconosciute ({}): trascrivo senza nomi", event["msg"].as_str().unwrap_or("?"))),
            "seg" => {
                let (t0, t1) = (event["t0"].as_i64().unwrap_or(0), event["t1"].as_i64().unwrap_or(0));
                let speaker = if voices >= 2 { speaker_of(&turns, t0, t1) } else { None };
                let seg = Segment { t0, t1, text: event["text"].as_str().unwrap_or("").to_string(), speaker };
                sink(Event::Log(Line { tone: Tone::Speech, key: format!("[{}]", clock(seg.t0)), text: seg.text.clone(), speaker }));
                segments.push(seg);
            }
            "progress" => {
                let p = event["p"].as_i64().unwrap_or(0).clamp(0, 100) as u8;
                sink(Event::Percent { index, percent: p });
                // La velocità si misura dalla prima percentuale, come nel riquadro della finestra:
                // il tempo per riconoscere le voci e caricare il modello non conta.
                let (t0, p0) = *first.get_or_insert((Instant::now(), p));
                if p / 10 > shown && p < 100 {
                    shown = p / 10;
                    let spent = t0.elapsed().as_secs_f64();
                    let text = if p > p0 && spent >= 6.0 { format!("{p}% · restano circa {}", minutes_text((spent * (100 - p) as f64 / (p - p0) as f64).max(1.0))) } else { format!("{p}%") };
                    log(sink, Tone::Note, "Avanzamento", text);
                }
            }
            "end" => {
                let voices = if voices >= 2 { voices } else { 0 };
                return Outcome::Done { lang: event["lang"].as_str().unwrap_or("it").to_string(), secs, segments, voices };
            }
            "error" if event["unsupported"].as_bool() == Some(true) => return Outcome::Unsupported(event["msg"].as_str().unwrap_or("").to_string()),
            "error" => return Outcome::Failed(event["msg"].as_str().unwrap_or("errore del motore").to_string()),
            _ => {}
        }
    }
}

/// La durata dell'audio coperta dai turni, per il registro.
fn event_secs(turns: &[Turn]) -> f64 {
    turns.iter().map(|t| t.t1).max().unwrap_or(0) as f64 / 100.0
}

/// Accende il motore scelto e aspetta che il modello sia in memoria. Con la scheda video, se il
/// motore non ne trova una o non parte, ripiega su quello per processore.
/// I due modelli delle voci sul PC (segmentazione, impronte).
type VoicePaths = (PathBuf, PathBuf);

fn power_on(request: &Request, model: &Path, voices: Option<&VoicePaths>, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<Running, String> {
    #[cfg(target_os = "android")]
    {
        let _ = cancel;
        // whisper.cpp è compilato con le istruzioni dotprod+fp16 (ARMv8.2): tutti i telefoni dal 2018 le hanno, ma si controlla
        // a runtime (HWCAP) invece di lasciar partire un'istruzione illegale.
        const HWCAP_FPHP: u64 = 1 << 9;
        const HWCAP_ASIMDDP: u64 = 1 << 20;
        let caps = unsafe { libc::getauxval(libc::AT_HWCAP) } as u64;
        if caps & HWCAP_FPHP == 0 || caps & HWCAP_ASIMDDP == 0 {
            return Err("questo telefono non ha le istruzioni (dotprod, fp16) che la trascrizione usa".into());
        }
        log(sink, Tone::Step, "Motore", "carico il modello in memoria…");
        let gpu = request.gpu && vulkan_loader_present();
        let mut running = Running::embedded(model, gpu, voices);
        let mut ready = running.next();
        if gpu && !matches!(&ready, Some(v) if v["e"] == "ready" && v["gpu"].is_string()) {
            log(sink, Tone::Note, "Scheda video", "non disponibile: uso il processore");
            running.stop();
            running = Running::embedded(model, false, voices);
            ready = running.next();
        }
        return match ready {
            Some(v) if v["e"] == "ready" => {
                let secs = v["ms"].as_f64().unwrap_or(0.0) / 1000.0;
                match v["gpu"].as_str() {
                    Some(g) => log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · scheda video: {g}")),
                    None => {
                        let threads = v["threads"].as_u64().unwrap_or(0);
                        crate::accel::set_in_use(false, format!("{threads} thread"));
                        log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · processore, {threads} thread"));
                    }
                }
                Ok(running)
            }
            Some(v) => Err(v["msg"].as_str().unwrap_or("il motore non è partito").to_string()),
            None => Err("il motore si è chiuso subito".into()),
        };
    }
    #[cfg(not(target_os = "android"))]
    power_on_process(request, model, voices, sink, cancel)
}

#[cfg(not(target_os = "android"))]
fn power_on_process(request: &Request, model: &Path, voices: Option<&VoicePaths>, sink: &dyn Fn(Event), cancel: &AtomicBool) -> Result<Running, String> {
    let cpu = cpu_variant(request.avx512);
    let mut tries: Vec<Variant> = Vec::new();
    let model_bytes = std::fs::metadata(model).map(|m| m.len()).unwrap_or(0);
    let gpu_budget = crate::resources::gpu_headroom(crate::resources::available_memory(), model_bytes);
    if request.gpu && !gpu_budget {
        log(sink, Tone::Note, "Memoria", "RAM libera limitata: uso il processore con lo stesso modello e la stessa precisione");
    }
    if request.gpu && gpu_budget {
        if vulkan_loader_present() {
            tries.push(Variant::Vulkan);
        } else {
            log(sink, Tone::Muted, "", "Nessun driver Vulkan per la scheda video: uso il processore");
        }
    }
    tries.push(cpu);
    let mut last = String::new();
    let mut told = false;
    for (n, variant) in tries.iter().copied().enumerate() {
        let fallback_left = n + 1 < tries.len();
        let exe = match ensure_engine(variant, sink, cancel) {
            Ok(exe) => exe,
            Err(e) if e == CANCELLED => return Err(e),
            Err(e) if fallback_left => {
                log(sink, Tone::Note, "Scheda video", format!("non disponibile ({e}): uso il processore"));
                continue;
            }
            Err(e) => return Err(e),
        };
        if !std::mem::replace(&mut told, true) {
            log(sink, Tone::Step, "Motore", "lo accendo e carico il modello in memoria…");
        }
        let running = match Running::start(&exe, model, false, voices) {
            Ok(running) => running,
            // Un motore che non parte (antivirus, file rovinato) non ferma tutto: c'è quello per processore.
            Err(e) if fallback_left => {
                log(sink, Tone::Note, "Scheda video", format!("non partita ({e}): uso il processore"));
                continue;
            }
            Err(e) => return Err(e),
        };
        let ready = match running.next_or_cancel(cancel) {
            Next::Line(v) => Some(v),
            Next::Gone => None,
            Next::Cancelled => {
                running.kill();
                return Err(CANCELLED.into());
            }
        };
        match ready {
            Some(v) if v["e"] == "ready" => {
                let secs = v["ms"].as_f64().unwrap_or(0.0) / 1000.0;
                let threads = v["threads"].as_u64().unwrap_or(0);
                match v["gpu"].as_str() {
                    Some(gpu) if variant == Variant::Vulkan => {
                        log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · scheda video: {gpu}"));
                        crate::accel::set_in_use(true, crate::accel::gpu_model(gpu));
                        return Ok(running);
                    }
                    _ if variant == Variant::Vulkan => {
                        log(sink, Tone::Note, "Scheda video", "nessuna adatta: uso il processore");
                        running.stop();
                        continue;
                    }
                    _ => {
                        log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · processore, {threads} thread ({})", variant.describe()));
                        let isa = variant.isa();
                        crate::accel::set_in_use(false, if isa.is_empty() { format!("{threads} thread") } else { format!("{isa} · {threads} thread") });
                        return Ok(running);
                    }
                }
            }
            Some(v) => last = v["msg"].as_str().unwrap_or("il motore non è partito").to_string(),
            None => last = "il motore si è chiuso subito".into(),
        }
        running.stop();
        if fallback_left {
            log(sink, Tone::Note, "Scheda video", format!("non partita ({last}): uso il processore"));
        }
    }
    Err(last)
}

/// Trascrive la coda, un file dopo l'altro, con un solo motore acceso per tutta la coda.
/// `dest` è la cartella scelta (None = accanto all'originale); `opts` dice cosa fare se il nome esiste già.
pub fn run(files: &[PathBuf], request: &Request, dest: Option<&Path>, opts: &Options, sink: &dyn Fn(Event)) {
    run_cancellable(files, request, dest, opts, &AtomicBool::new(false), sink)
}

/// Come `run`, ma si ferma appena `cancel` si accende (Annulla): i file che restano finiscono come «Annullato».
pub fn run_cancellable(files: &[PathBuf], request: &Request, dest: Option<&Path>, opts: &Options, cancel: &AtomicBool, sink: &dyn Fn(Event)) {
    if crate::resources::busy_elsewhere() {
        log(sink, Tone::Note, "", "Un altro lavoro è in corso: la trascrizione parte appena finisce");
    }
    let Some(_job) = crate::resources::acquire(Some(cancel)) else {
        for path in files { sink(Event::Finished { path: path.clone(), result: Err(CANCELLED.into()) }); }
        return;
    };
    let fail_all = |e: &str, from: usize| {
        for path in &files[from..] {
            sink(Event::Finished { path: path.clone(), result: Err(e.to_string()) });
        }
    };
    // Modello e motore si accendono solo al primo file che va davvero trascritto: se i nomi esistono già
    // e vanno saltati, non si scarica né si carica nulla.
    let mut model: Option<PathBuf> = None;
    let mut engine: Option<Running> = None;
    // Chi parla: i modelli delle voci si preparano insieme al motore. Se non si possono avere (niente
    // internet la prima volta) si trascrive lo stesso, senza nomi.
    let mut speakers = request.speakers;
    let mut voices: Option<VoicePaths> = None;
    for (index, path) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            // break, non return: il motore acceso si spegne qui sotto.
            fail_all(CANCELLED, index);
            break;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        log(sink, Tone::Step, "File", format!("{} di {}: {name}", index + 1, files.len()));
        let dir = dest.map(Path::to_path_buf).unwrap_or_else(|| path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")));
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("trascrizione");
        // Il nome si decide prima: se il file esiste e va saltato, non si trascrive neppure.
        let Some(out) = engine::choose_output(path, &dir, stem, request.format.ext(), opts) else {
            log(sink, Tone::Muted, "", format!("{name}: il file esiste già, saltato"));
            sink(Event::Finished { path: path.clone(), result: Err("il file esiste già: saltato".into()) });
            continue;
        };
        if engine.is_none() {
            if speakers != 1 && voices.is_none() {
                match ensure_voice_models(sink, cancel) {
                    Ok(v) => voices = Some(v),
                    Err(e) if e == CANCELLED => return fail_all(&e, index),
                    Err(e) => {
                        log(sink, Tone::Note, "Voci", format!("{e}: trascrivo senza riconoscere chi parla"));
                        speakers = 1;
                    }
                }
            }
            let started = match &model {
                Some(m) => power_on(request, m, voices.as_ref(), sink, cancel),
                None => ensure_model(request.model, sink, cancel).and_then(|m| {
                    let r = power_on(request, &m, voices.as_ref(), sink, cancel);
                    model = Some(m);
                    r
                }),
            };
            match started {
                Ok(r) => engine = Some(r),
                Err(e) if e == CANCELLED => return fail_all(&e, index),
                Err(e) => {
                    log(sink, Tone::Error, "Errore", e.clone());
                    return fail_all(&e, index);
                }
            }
        }
        let running = engine.as_mut().expect("motore acceso");
        let mut outcome = one_file(running, path, index, speakers, request.glossario.as_deref(), sink, cancel);
        // A driver can run out of shared memory after loading successfully.
        // Retry this file once on CPU, retaining the exact model and precision.
        if matches!(outcome, Outcome::EngineDied(_)) && crate::accel::in_use().is_some_and(|a| a.gpu) && !cancel.load(Ordering::Relaxed) {
            if let Some(dead) = engine.take() { dead.stop(); }
            log(sink, Tone::Note, "Scheda video", "motore interrotto: riprovo sul processore, con la stessa precisione");
            let cpu_request = Request { gpu: false, ..request.clone() };
            match power_on(&cpu_request, model.as_ref().expect("modello caricato"), voices.as_ref(), sink, cancel) {
                Ok(mut cpu) => { outcome = one_file(&mut cpu, path, index, speakers, request.glossario.as_deref(), sink, cancel); engine = Some(cpu); }
                Err(e) => outcome = Outcome::Failed(e),
            }
        }
        let mut temp = None;
        if let Outcome::Unsupported(why) = &outcome {
            log(sink, Tone::Muted, "", format!("Il motore non legge questo formato ({why}): lo apro con il sistema"));
            match system_decode(path, cancel) {
                Ok(wav) => {
                    outcome = one_file(engine.as_mut().expect("motore acceso"), &wav, index, speakers, request.glossario.as_deref(), sink, cancel);
                    temp = Some(wav);
                }
                Err(e) if e == CANCELLED => outcome = Outcome::Cancelled,
                Err(e) => outcome = Outcome::Failed(e),
            }
        }
        if let Some(dir) = temp.as_deref().and_then(Path::parent) {
            let _ = std::fs::remove_dir_all(dir);
        }
        if matches!(outcome, Outcome::Cancelled) {
            // Il motore sta ancora calcolando: lo si ferma subito.
            if let Some(e) = engine.take() {
                e.kill();
            }
            return fail_all(CANCELLED, index);
        }
        let engine_gone = matches!(outcome, Outcome::EngineDied(_));
        let result = match outcome {
            Outcome::Done { segments, .. } if segments.is_empty() => Err("nessun parlato riconosciuto nell'audio".to_string()),
            Outcome::Done { lang, secs, segments, voices } => {
                let names = default_names(voices);
                // Le frasi restano salvate: «Rivedi» le riapre anche dopo aver chiuso Cedro.
                let _ = crate::revisione::save(path, secs, &lang, &segments, &names);
                let bytes = match request.format {
                    Format::Word => docx_bytes(path, secs, &lang, &segments, &names),
                    Format::Text => text_bytes(path, secs, &lang, &segments, &names),
                };
                std::fs::create_dir_all(&dir)
                    .and_then(|_| {
                        // Un file scritto a metà (disco pieno) non deve restare: con «Salta» la prossima volta
                        // verrebbe saltato come se la trascrizione ci fosse già.
                        std::fs::write(&out, bytes).inspect_err(|_| {
                            let _ = std::fs::remove_file(&out);
                        })
                    })
                    .map(|_| {
                        let file = out.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                        log(sink, Tone::Step, "Salvato", file.clone());
                        (out.clone(), format!("{file} · {}", language_name(&lang)))
                    })
                    .map_err(|e| format!("{} non scritto: {e}", out.display()))
            }
            Outcome::Unsupported(e) | Outcome::Failed(e) | Outcome::EngineDied(e) => Err(e),
            Outcome::Cancelled => Err(CANCELLED.to_string()),
        };
        if let Err(e) = &result {
            log(sink, Tone::Error, "Errore", format!("{name}: {e}"));
        }
        sink(Event::Finished { path: path.clone(), result });
        if engine_gone && index + 1 < files.len() {
            // Il motore è caduto: lo si riaccende per i file che restano.
            if let Some(dead) = engine.take() {
                dead.stop();
            }
            if let Some(m) = &model {
                match power_on(request, m, voices.as_ref(), sink, cancel) {
                    Ok(r) => engine = Some(r),
                    Err(e) if e == CANCELLED => return fail_all(&e, index + 1),
                    Err(e) => {
                        log(sink, Tone::Error, "Errore", e.clone());
                        return fail_all(&e, index + 1);
                    }
                }
            }
        }
    }
    if let Some(running) = engine {
        running.stop();
        log(sink, Tone::Muted, "", "Motore spento: memoria liberata, processore libero.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unwinding_reaps_the_transcription_process() {
        let child = Command::new("sleep").arg("60").stdin(Stdio::piped()).spawn().unwrap();
        let pid = child.id() as libc::pid_t;
        let result = std::panic::catch_unwind(move || {
            let (_tx, lines) = channel();
            let _running = Running {
                child: Some(child), stdin: None, lines,
                #[cfg(target_os = "android")]
                requests: None,
            };
            panic!("event sink failed");
        });
        assert!(result.is_err());
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    fn seg(t0: i64, t1: i64, text: &str) -> Segment {
        Segment { t0, t1, text: text.into(), speaker: None }
    }

    fn said(t0: i64, t1: i64, speaker: u8, text: &str) -> Segment {
        Segment { t0, t1, text: text.into(), speaker: Some(speaker) }
    }

    #[test]
    fn finds_the_engine_in_a_release() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"assets": [{"name": "cedro-trascrivi-0.18.1-windows-x64-avx2.exe", "browser_download_url": "https://x/m.exe",
                "digest": "sha256:ab", "size": 3356160}]}"#,
        )
        .unwrap();
        let found = engine_in_release(&json, "cedro-trascrivi-0.18.1-windows-x64-avx2.exe").unwrap();
        assert_eq!(found, ("https://x/m.exe".to_string(), "ab".to_string(), 3));
        assert!(engine_in_release(&json, "cedro-trascrivi-0.18.1-windows-x64-base.exe").is_err());
        assert_eq!(ENGINE_TAGS[0], "motori-v");
    }

    #[test]
    fn xml_escape_drops_forbidden_characters() {
        assert_eq!(xml_escape("a & b <c> \"d\""), "a &amp; b &lt;c&gt; &quot;d&quot;");
        assert_eq!(xml_escape("x\u{1}y\u{b}z\u{1f}"), "xyz");
        assert_eq!(xml_escape("riga\tuno\n"), "riga\tuno\n");
        assert_eq!(xml_escape("perché"), "perché");
    }

    #[test]
    fn clock_and_sizes_read_well() {
        assert_eq!(clock(4), "00:00:00");
        assert_eq!(clock(76_000), "00:12:40");
        assert_eq!(clock(366_100), "01:01:01");
        assert_eq!(size_text(148), "150 MB");
        assert_eq!(size_text(488), "490 MB");
        assert_eq!(size_text(1624), "1,6 GB");
        assert_eq!(size_text(1960), "2,0 GB");
        assert_eq!(size_text(874), "870 MB");
    }

    #[test]
    fn pauses_start_new_paragraphs() {
        let segs = [seg(0, 300, "Buongiorno a tutti."), seg(320, 600, "Iniziamo la seduta."), seg(900, 1200, "Primo punto.")];
        let p: Vec<(i64, String)> = paragraphs(&segs).into_iter().map(|p| (p.t0, p.text)).collect();
        assert_eq!(p, vec![(0, "Buongiorno a tutti. Iniziamo la seduta.".to_string()), (900, "Primo punto.".to_string())]);
    }

    #[test]
    fn a_new_speaker_starts_a_new_paragraph() {
        let segs = [said(0, 300, 0, "Buongiorno a tutti."), said(320, 600, 0, "Apriamo la seduta."), said(620, 900, 1, "Chiedo la parola.")];
        let p = paragraphs(&segs);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].speaker, p[0].text.as_str()), (Some(0), "Buongiorno a tutti. Apriamo la seduta."));
        assert_eq!((p[1].t0, p[1].speaker), (620, Some(1)));
    }

    #[test]
    fn each_sentence_goes_to_who_speaks_most_in_it() {
        let turns = [Turn { t0: 0, t1: 500, speaker: 0 }, Turn { t0: 450, t1: 1000, speaker: 1 }, Turn { t0: 1500, t1: 2000, speaker: 0 }];
        assert_eq!(speaker_of(&turns, 100, 480), Some(0));
        assert_eq!(speaker_of(&turns, 400, 900), Some(1));
        // Nel silenzio tra due turni: il più vicino.
        assert_eq!(speaker_of(&turns, 1300, 1400), Some(0));
        assert_eq!(speaker_of(&turns, 1050, 1100), Some(1));
        assert_eq!(speaker_of(&[], 0, 100), None);
    }

    #[test]
    fn outputs_carry_the_speakers_names() {
        let segs = [said(400, 900, 0, "Apriamo la seduta."), said(1000, 1500, 1, "Chiedo la parola."), said(1600, 1900, 0, "Prego.")];
        let names = vec!["Sindaco".to_string(), String::new()];
        let txt = String::from_utf8(text_bytes(Path::new("/x/consiglio.mp3"), 60.0, "it", &segs, &names)).unwrap();
        assert!(txt.contains("Interlocutori: Sindaco, Interlocutore 2"), "{txt}");
        assert!(txt.contains("[00:00:04] Sindaco: Apriamo la seduta."));
        assert!(txt.contains("[00:00:10] Interlocutore 2: Chiedo la parola."));
        assert!(txt.contains("[00:00:16] Sindaco: Prego."));
        // Senza nomi resta come prima.
        let plain = String::from_utf8(text_bytes(Path::new("/x/consiglio.mp3"), 60.0, "it", &segs, &[])).unwrap();
        assert!(plain.contains("[00:00:04] Apriamo la seduta.") && !plain.contains("Interlocutor"));
        let docx = docx_bytes(Path::new("/x/consiglio.mp3"), 60.0, "it", &segs, &names);
        let dir = std::env::temp_dir().join(format!("cedro-voci-docx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("v.docx");
        std::fs::write(&file, &docx).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        assert!(xml.contains("Sindaco") && xml.contains("Interlocutore 2"));
        assert!(xml.contains(VOICE_COLORS[0]) && xml.contains(VOICE_COLORS[1]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untar_finds_the_model_in_a_bz2_archive() {
        use std::io::Write as _;
        // Un tar minimo con due file, compresso in bz2.
        fn entry(name: &str, data: &[u8]) -> Vec<u8> {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            let size = format!("{:011o}\0", data.len());
            h[124..136].copy_from_slice(size.as_bytes());
            h[156] = b'0';
            let mut out = h.to_vec();
            out.extend_from_slice(data);
            out.resize(512 + data.len().div_ceil(512) * 512, 0);
            out
        }
        let mut tar = entry("cartella/LEGGIMI", b"ciao");
        tar.extend(entry("cartella/model.onnx", &[7u8; 700]));
        tar.extend([0u8; 1024]);
        let dir = std::env::temp_dir().join(format!("cedro-untar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.tar.bz2");
        let mut enc = bzip2::write::BzEncoder::new(std::fs::File::create(&path).unwrap(), bzip2::Compression::fast());
        enc.write_all(&tar).unwrap();
        enc.finish().unwrap();
        assert_eq!(untar_bz2(&path, "cartella/model.onnx").unwrap(), vec![7u8; 700]);
        assert!(untar_bz2(&path, "cartella/altro").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transcribable_takes_voice_messages_and_videos() {
        // Un vocale AMR entra solo nella trascrizione; Opus ora si converte anche, quindi è Audio.
        assert!(transcribable(Path::new("vocale.amr")));
        assert!(only_transcribable(Path::new("vocale.amr")));
        assert!(transcribable(Path::new("vocale.opus")) && !only_transcribable(Path::new("vocale.opus")));
        assert!(transcribable(Path::new("riunione.m4a")) && !only_transcribable(Path::new("riunione.m4a")));
        assert!(transcribable(Path::new("seduta.mp4")));
        assert!(!transcribable(Path::new("foto.jpg")));
    }

    #[test]
    fn outputs_carry_title_times_and_text() {
        let segs = [seg(400, 900, "Buongiorno & benvenuti <tutti>."), seg(2100, 2500, "Primo punto.")];
        let txt = String::from_utf8(text_bytes(Path::new("/x/giunta.m4a"), 3240.0, "it", &segs, &[])).unwrap();
        assert!(txt.starts_with("giunta"));
        assert!(txt.contains("54 min di audio · lingua: italiano"));
        assert!(txt.contains("[00:00:04] Buongiorno & benvenuti <tutti>."));
        assert!(txt.contains("[00:00:21] Primo punto."));
        let docx = docx_bytes(Path::new("/x/giunta.m4a"), 3240.0, "it", &segs, &[]);
        let dir = std::env::temp_dir().join(format!("cedro-trascr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.docx");
        std::fs::write(&file, &docx).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        assert!(xml.contains("Buongiorno &amp; benvenuti &lt;tutti&gt;."));
        assert!(xml.contains("[00:00:21]"));
        let mut styles = String::new();
        zip.by_name("word/styles.xml").unwrap().read_to_string(&mut styles).unwrap();
        assert!(styles.contains("it-IT"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prova sul PC: con `CEDRO_TRASCRIVI_DIR` (i motori già compilati, nomi `cedro-trascrivi-<variante>`) e
    /// `espeak-ng` trascrive una frase vera, scaricando il modello «Veloce» se manca. Altrove non fa niente.
    /// Con `CEDRO_TRASCRIVI_GPU=1` prova la variante per scheda video.
    #[test]
    fn transcribes_a_spoken_sentence() {
        if std::env::var_os("CEDRO_TRASCRIVI_DIR").is_none() {
            return;
        }
        let work = std::env::temp_dir().join(format!("cedro-voce-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();
        let wav = work.join("voce.wav");
        let spoke = Command::new("espeak-ng").args(["-v", "it", "-s", "130", "-w"]).arg(&wav).arg("Buongiorno a tutti. Oggi parliamo del bilancio comunale.").status();
        if !spoke.is_ok_and(|s| s.success()) {
            eprintln!("espeak-ng non c'è: prova saltata");
            return;
        }
        let request = Request { format: Format::Text, model: 0, gpu: std::env::var_os("CEDRO_TRASCRIVI_GPU").is_some(), avx512: false, speakers: 1, glossario: None };
        let lines = std::sync::Mutex::new(Vec::new());
        let finished = std::sync::Mutex::new(None);
        run(std::slice::from_ref(&wav), &request, Some(&work), &Options::default(), &|event| match event {
            Event::Log(line) => lines.lock().unwrap().push(format!("{} {}", line.key, line.text)),
            Event::Finished { result, .. } => *finished.lock().unwrap() = Some(result),
            Event::Percent { .. } => {}
        });
        for line in lines.lock().unwrap().iter() {
            eprintln!("{line}");
        }
        let result = finished.lock().unwrap().take().expect("nessun risultato");
        let (out, _) = result.expect("trascrizione");
        let text = std::fs::read_to_string(out).unwrap().to_lowercase();
        // La voce di espeak è robotica e il modello «Veloce» sbaglia le parole difficili: bastano queste.
        assert!(text.contains("oggi parliamo"), "testo: {text}");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// Un download che si interrompe a metà riprende da dove era arrivato (richiesta «Range»).
    #[test]
    fn a_broken_download_resumes() {
        use sha2::{Digest, Sha256};
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/modello.bin", listener.local_addr().unwrap());
        let ranges = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            for (i, stream) in listener.incoming().take(2).enumerate() {
                let Ok(mut stream) = stream else { continue };
                let mut request = [0u8; 2048];
                let n = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..n]).to_lowercase();
                seen.lock().unwrap().push(head.lines().find(|l| l.starts_with("range:")).unwrap_or("").to_string());
                if i == 0 {
                    // Promette venti byte, ne manda dieci e chiude: come una rete che cade.
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n0123456789");
                } else {
                    let _ = stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 10-19/20\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabcdefghij");
                }
            }
        });
        let dir = std::env::temp_dir().join(format!("cedro-fetch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("modello.bin");
        let want = hex(&Sha256::digest(b"0123456789abcdefghij"));
        let sha = fetch(&url, &dest, Some(&want), 1, "il modello", &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(sha, want);
        assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789abcdefghij");
        assert!(!dir.join("modello.part").exists());
        assert_eq!(ranges.lock().unwrap()[1].trim(), "range: bytes=10-");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preposizioni_articolate() {
        assert_eq!(di("il modello"), "del modello");
        assert_eq!(di("il motore"), "del motore");
        assert_eq!(di("i modelli delle voci"), "dei modelli delle voci");
        assert_eq!(di("Cedro"), "di Cedro");
    }
}
