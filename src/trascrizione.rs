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
use std::sync::mpsc::{channel, Receiver};
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

pub const MODELS: &[Model] = &[
    // Scelti con la misura su 200 frasi italiane di FLEURS (errori sulle parole, in parlato pulito e in «messaggio vocale»
    // disturbato): small q8 7,5 % e 17 %, medium q5 4,1 % e 9 %, large-v3-turbo q5 2,6 % e 5,7 %. Il vecchio «Veloce»
    // (base) sbagliava 1 parola su 6.
    Model { name: "Veloce", note: "Per appunti e messaggi vocali", file: "ggml-small-q8_0.bin", megabytes: 264 },
    Model { name: "Buona", note: "Per riunioni e interviste", file: "ggml-medium-q5_0.bin", megabytes: 539 },
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
const OLD_MODELS: &[&str] = &["ggml-base-q8_0.bin", "ggml-large-v3-turbo-q8_0.bin"];

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

fn log(sink: &dyn Fn(Event), tone: Tone, key: &str, text: impl Into<String>) {
    sink(Event::Log(Line { tone, key: key.to_string(), text: text.into() }));
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
        .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("cedro-trascrivi-") && e.path().extension().is_some_and(|x| x == "exe" || x.is_empty())))
        .unwrap_or(false)
}

/// Il peso del modello scritto come lo legge una persona: «490 MB», «1,6 GB».
pub fn size_text(megabytes: u32) -> String {
    if megabytes >= 1000 {
        format!("{},{} GB", megabytes / 1000, (megabytes % 1000 + 50) / 100)
    } else {
        format!("{} MB", (megabytes + 5) / 10 * 10)
    }
}

/// Le versioni del motore: una per tipo di processore, più quella per la scheda video.
/// Quelle x86 e quelle ARM esistono tutte nel codice, ma ogni architettura ne sceglie solo le sue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum Variant {
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
    fn tag(self) -> &'static str {
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

    fn describe(self) -> &'static str {
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
fn cpu_variant(avx512: bool) -> Variant {
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
    let cores = performance_cores().unwrap_or(logical / 2).max(1);
    cores.clamp(1, 8)
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
fn real_gpu_present() -> bool {
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
fn vulkan_loader_present() -> bool {
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

fn exe_name(variant: Variant) -> String {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    format!("cedro-trascrivi-{}{suffix}", variant.tag())
}

fn asset_name(variant: Variant) -> String {
    let os = if cfg!(windows) { "windows" } else { "linux" };
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    format!("cedro-trascrivi-{}-{os}-{arch}-{}{suffix}", crate::update::CURRENT, variant.tag())
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

/// Scarica `url` in `dest` passando da un file `.part`, con lo SHA-256 controllato se noto.
/// Ogni 10% scrive a che punto è. Ritorna lo SHA-256 del file scaricato.
fn fetch(url: &str, dest: &Path, expected_sha: Option<&str>, megabytes: u32, what: &str, sink: &dyn Fn(Event)) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let response = http().get(url).call().map_err(|e| format!("download di {what} non riuscito: {e}"))?;
    let total = response.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(megabytes as u64 * 1_000_000);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cartella {} non creata: {e}", dir.display()))?;
    }
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{} non scrivibile: {e}", part.display()))?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut shown) = (0u64, 0u64);
    let copied = loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) => break Err(format!("download di {what} interrotto: {e}")),
        };
        hasher.update(&buf[..n]);
        if let Err(e) = file.write_all(&buf[..n]) {
            break Err(format!("scrittura di {what} non riuscita (disco pieno?): {e}"));
        }
        done += n as u64;
        let tenth = done * 10 / total.max(1);
        if tenth > shown && tenth < 10 {
            shown = tenth;
            log(sink, Tone::Muted, "", format!("Scaricati {} di {}", size_text((done / 1_000_000) as u32), size_text((total / 1_000_000) as u32)));
        }
    };
    drop(file);
    // Un download a metà (rete caduta, disco pieno) non resta sul disco: un modello pesa centinaia di MB.
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let sha = hex(&hasher.finalize());
    if let Some(expected) = expected_sha {
        if !sha.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{what} scaricato non corrisponde all'originale (SHA-256 diverso): scartato"));
        }
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| format!("{} non salvato: {e}", dest.display()))?;
    Ok(sha)
}

/// Lo SHA-256 del modello pubblicato: Hugging Face lo dice nell'intestazione, senza scaricare nulla.
fn published_model_sha(model: &Model, wait: Duration) -> Option<String> {
    let agent = ureq::AgentBuilder::new().timeout(wait).redirects(0).user_agent(&format!("cedro/{}", crate::update::CURRENT)).build();
    let url = format!("{MODEL_BASE_URL}{}", model.file);
    let response = match agent.head(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(_) => return None,
    };
    let tag = response.header("x-linked-etag").or_else(|| response.header("etag"))?;
    let tag = tag.trim().trim_start_matches("W/").trim_matches('"').to_lowercase();
    (tag.len() == 64 && tag.chars().all(|c| c.is_ascii_hexdigit())).then_some(tag)
}

/// Il modello pronto sul PC: scaricato la prima volta, riscaricato se ne è uscita una versione nuova.
fn ensure_model(index: usize, sink: &dyn Fn(Event)) -> Result<PathBuf, String> {
    let model = MODELS.get(index).ok_or("modello sconosciuto")?;
    let dir = data_dir();
    let path = dir.join(model.file);
    let sidecar = dir.join(format!("{}.sha256", model.file));
    let have = std::fs::read_to_string(&sidecar).ok().map(|s| s.trim().to_lowercase()).filter(|_| path.is_file());
    match have {
        Some(local) => {
            // Un controllo veloce: senza internet si usa quello che c'è.
            match published_model_sha(model, Duration::from_secs(5)) {
                Some(remote) if remote != local => {
                    log(sink, Tone::Step, "Modello", format!("«{}»: è uscita una versione nuova, la scarico ({})…", model.name, size_text(model.megabytes)));
                }
                _ => {
                    log(sink, Tone::Step, "Modello", format!("«{}» già sul PC", model.name));
                    return Ok(path);
                }
            }
        }
        None => log(sink, Tone::Step, "Modello", format!("«{}»: lo scarico la prima volta ({})…", model.name, size_text(model.megabytes))),
    }
    let expected = published_model_sha(model, Duration::from_secs(20));
    let url = format!("{MODEL_BASE_URL}{}", model.file);
    let sha = fetch(&url, &path, expected.as_deref(), model.megabytes, "il modello", sink)?;
    std::fs::write(&sidecar, &sha).map_err(|e| format!("{} non scritto: {e}", sidecar.display()))?;
    log(sink, Tone::Step, "Modello", format!("«{}» scaricato e controllato", model.name));
    forget_old_models();
    Ok(path)
}

/// Il motore pronto sul PC, nella versione giusta per questa versione di Cedro.
/// `CEDRO_TRASCRIVI_DIR` (per le prove) indica una cartella con i motori già compilati.
fn ensure_engine(variant: Variant, sink: &dyn Fn(Event)) -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("CEDRO_TRASCRIVI_DIR") {
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
    fetch(&url, &path, Some(&sha), megabytes, "il motore", sink)?;
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

/// Il motore acceso: il processo, dove scrivergli e da dove leggere le sue righe.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<serde_json::Value>,
}

impl Running {
    fn start(exe: &Path, model: &Path, cpu: bool) -> Result<Running, String> {
        let mut command = Command::new(exe);
        command.arg("--model").arg(model).arg("--threads").arg(engine_threads().to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if cpu {
            command.arg("--cpu");
        }
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
        Ok(Running { child, stdin, lines })
    }

    fn send(&mut self, value: serde_json::Value) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("il motore si è chiuso")?;
        writeln!(stdin, "{value}").and_then(|_| stdin.flush()).map_err(|_| "il motore si è chiuso".to_string())
    }

    /// La prossima riga del motore; `None` se si è chiuso.
    fn next(&self) -> Option<serde_json::Value> {
        self.lines.recv().ok()
    }

    /// Chiude stdin: il motore finisce e libera la memoria. Se non esce da solo, lo si ferma.
    fn stop(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Un segmento di parlato: inizio e fine in centesimi di secondo.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub t0: i64,
    pub t1: i64,
    pub text: String,
}

/// «00:12:40» da centesimi di secondo.
pub fn clock(centis: i64) -> String {
    let secs = centis.max(0) / 100;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Il tempo che manca, senza spazi (il riquadro del registro andrebbe a capo tra il numero e l'unità).
fn left_text(secs: f64) -> String {
    minutes_text(secs).replace(' ', "")
}

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

/// I segmenti raggruppati in paragrafi: si va a capo dopo una pausa o quando il paragrafo è lungo.
pub fn paragraphs(segments: &[Segment]) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    let mut last_end = 0i64;
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let pause = seg.t0 - last_end;
        let sentence_ended = out.last().is_some_and(|(_, p)| p.ends_with(['.', '?', '!', '…']));
        let start_new = match out.last() {
            None => true,
            Some((_, p)) => pause >= 150 || (p.chars().count() > 600 && sentence_ended) || p.chars().count() > 1200,
        };
        if start_new {
            out.push((seg.t0, text.to_string()));
        } else if let Some((_, p)) = out.last_mut() {
            p.push(' ');
            p.push_str(text);
        }
        last_end = seg.t1;
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
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
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

/// Intestazione comune: nome del file, data, durata e lingua.
fn heading(source: &Path, secs: f64, lang: &str) -> (String, String) {
    let title = source.file_stem().and_then(|s| s.to_str()).unwrap_or("Trascrizione").to_string();
    let meta = format!("Trascrizione del {} · {} di audio · lingua: {}", today_text(), minutes_text(secs), language_name(lang));
    (title, meta)
}

pub fn text_bytes(source: &Path, secs: f64, lang: &str, segments: &[Segment]) -> Vec<u8> {
    let (title, meta) = heading(source, secs, lang);
    let mut out = format!("{title}\n{meta}\n\n");
    for (t0, text) in paragraphs(segments) {
        out.push_str(&format!("[{}] {text}\n\n", clock(t0)));
    }
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    out.trim_end().replace('\n', eol).into_bytes()
}

pub fn docx_bytes(source: &Path, secs: f64, lang: &str, segments: &[Segment]) -> Vec<u8> {
    let (title, meta) = heading(source, secs, lang);
    let mut body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Meta\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(&title),
        xml_escape(&meta)
    );
    for (t0, text) in paragraphs(segments) {
        body.push_str(&format!(
            "<w:p><w:r><w:rPr><w:rStyle w:val=\"Time\"/></w:rPr><w:t xml:space=\"preserve\">[{}]  </w:t></w:r><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            clock(t0),
            xml_escape(&text)
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
fn system_decode(input: &Path) -> Result<PathBuf, String> {
    // In una cartella nuova e solo nostra: su Linux /tmp è di tutti e ffmpeg scrive con -y.
    let dir = crate::engine::private_temp_dir("cedro-trascrivi")?;
    let temp = dir.join("audio.wav");
    #[cfg(windows)]
    {
        if let Err(e) = crate::video::audio_to_wav(input, &temp) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        Ok(temp)
    }
    #[cfg(not(windows))]
    {
        let ffmpeg = Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(input)
            .args(["-vn", "-ac", "1", "-ar", "16000"])
            .arg(&temp)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if ffmpeg.is_ok_and(|s| s.success()) && temp.is_file() {
            return Ok(temp);
        }
        let location = format!("location={}", input.display());
        let out = format!("location={}", temp.display());
        let gst = Command::new("gst-launch-1.0")
            .args(["-q", "filesrc", &location, "!", "decodebin", "!", "audioconvert", "!", "audioresample", "!", "audio/x-raw,rate=16000,channels=1", "!", "wavenc", "!", "filesink", &out])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if gst.is_ok_and(|s| s.success()) && temp.metadata().is_ok_and(|m| m.len() > 44) {
            return Ok(temp);
        }
        let _ = std::fs::remove_dir_all(&dir);
        Err("formato audio non supportato: installa ffmpeg per leggerlo".into())
    }
}

/// Quello che il motore dice su un file, fino alla fine.
enum Outcome {
    Done { lang: String, secs: f64, segments: Vec<Segment> },
    Unsupported(String),
    Failed(String),
}

/// Trascrive un file con il motore già acceso, raccontando i passi.
fn one_file(engine: &mut Running, path: &Path, index: usize, sink: &dyn Fn(Event)) -> Outcome {
    if let Err(e) = engine.send(serde_json::json!({"file": path.to_string_lossy(), "lang": "auto"})) {
        return Outcome::Failed(e);
    }
    let mut segments = Vec::new();
    let mut secs = 0.0;
    let mut started = Instant::now();
    let mut shown = 0;
    loop {
        let Some(event) = engine.next() else { return Outcome::Failed("il motore si è chiuso all'improvviso".into()) };
        match event["e"].as_str().unwrap_or("") {
            "audio" => {
                secs = event["secs"].as_f64().unwrap_or(0.0);
                started = Instant::now();
                log(sink, Tone::Step, "Audio", format!("{} di parlato da trascrivere", minutes_text(secs)));
            }
            "lang" => log(sink, Tone::Step, "Lingua", format!("riconosciuta: {}", language_name(event["code"].as_str().unwrap_or("?")))),
            "seg" => {
                let seg = Segment { t0: event["t0"].as_i64().unwrap_or(0), t1: event["t1"].as_i64().unwrap_or(0), text: event["text"].as_str().unwrap_or("").to_string() };
                log(sink, Tone::Speech, &format!("[{}]", clock(seg.t0)), seg.text.clone());
                segments.push(seg);
            }
            "progress" => {
                let p = event["p"].as_i64().unwrap_or(0).clamp(0, 100) as u8;
                sink(Event::Percent { index, percent: p });
                if p / 10 > shown && p < 100 {
                    shown = p / 10;
                    let elapsed = started.elapsed().as_secs_f64();
                    let left = elapsed * (100.0 - p as f64) / (p as f64).max(1.0);
                    log(sink, Tone::Note, "Avanzamento", format!("{p}% · restano circa {}", left_text(left.max(1.0))));
                }
            }
            "end" => return Outcome::Done { lang: event["lang"].as_str().unwrap_or("it").to_string(), secs, segments },
            "error" if event["unsupported"].as_bool() == Some(true) => return Outcome::Unsupported(event["msg"].as_str().unwrap_or("").to_string()),
            "error" => return Outcome::Failed(event["msg"].as_str().unwrap_or("errore del motore").to_string()),
            _ => {}
        }
    }
}

/// Accende il motore scelto e aspetta che il modello sia in memoria. Con la scheda video, se il
/// motore non ne trova una o non parte, ripiega su quello per processore.
fn power_on(request: &Request, model: &Path, sink: &dyn Fn(Event)) -> Result<Running, String> {
    let cpu = cpu_variant(request.avx512);
    let mut tries: Vec<Variant> = Vec::new();
    if request.gpu {
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
        let exe = match ensure_engine(variant, sink) {
            Ok(exe) => exe,
            Err(e) if fallback_left => {
                log(sink, Tone::Note, "Scheda video", format!("non disponibile ({e}): uso il processore"));
                continue;
            }
            Err(e) => return Err(e),
        };
        if !std::mem::replace(&mut told, true) {
            log(sink, Tone::Step, "Motore", "lo accendo e carico il modello in memoria…");
        }
        let running = Running::start(&exe, model, false)?;
        let ready = running.next();
        match ready {
            Some(v) if v["e"] == "ready" => {
                let secs = v["ms"].as_f64().unwrap_or(0.0) / 1000.0;
                let threads = v["threads"].as_u64().unwrap_or(0);
                match v["gpu"].as_str() {
                    Some(gpu) if variant == Variant::Vulkan => {
                        log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · scheda video: {gpu}"));
                        return Ok(running);
                    }
                    _ if variant == Variant::Vulkan => {
                        log(sink, Tone::Note, "Scheda video", "nessuna adatta: uso il processore");
                        running.stop();
                        continue;
                    }
                    _ => {
                        log(sink, Tone::Step, "Motore", format!("pronto in {secs:.1} s · processore, {threads} thread ({})", variant.describe()));
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
    let fail_all = |e: &str, from: usize| {
        for path in &files[from..] {
            sink(Event::Finished { path: path.clone(), result: Err(e.to_string()) });
        }
    };
    // Modello e motore si accendono solo al primo file che va davvero trascritto: se i nomi esistono già
    // e vanno saltati, non si scarica né si carica nulla.
    let mut model: Option<PathBuf> = None;
    let mut engine: Option<Running> = None;
    for (index, path) in files.iter().enumerate() {
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
            let started = match &model {
                Some(m) => power_on(request, m, sink),
                None => ensure_model(request.model, sink).and_then(|m| {
                    let r = power_on(request, &m, sink);
                    model = Some(m);
                    r
                }),
            };
            match started {
                Ok(r) => engine = Some(r),
                Err(e) => {
                    log(sink, Tone::Error, "Errore", e.clone());
                    return fail_all(&e, index);
                }
            }
        }
        let running = engine.as_mut().expect("motore acceso");
        let mut outcome = one_file(running, path, index, sink);
        let mut temp = None;
        if let Outcome::Unsupported(why) = &outcome {
            log(sink, Tone::Muted, "", format!("Il motore non legge questo formato ({why}): lo apro con il sistema"));
            match system_decode(path) {
                Ok(wav) => {
                    outcome = one_file(running, &wav, index, sink);
                    temp = Some(wav);
                }
                Err(e) => outcome = Outcome::Failed(e),
            }
        }
        if let Some(dir) = temp.as_deref().and_then(Path::parent) {
            let _ = std::fs::remove_dir_all(dir);
        }
        let result = match outcome {
            Outcome::Done { lang, secs, segments } if segments.is_empty() => {
                let _ = lang;
                let _ = secs;
                Err("nessun parlato riconosciuto nell'audio".to_string())
            }
            Outcome::Done { lang, secs, segments } => {
                let bytes = match request.format {
                    Format::Word => docx_bytes(path, secs, &lang, &segments),
                    Format::Text => text_bytes(path, secs, &lang, &segments),
                };
                std::fs::create_dir_all(&dir)
                    .and_then(|_| std::fs::write(&out, bytes))
                    .map(|_| {
                        let file = out.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                        log(sink, Tone::Step, "Salvato", file.clone());
                        (out.clone(), format!("{file} · {}", language_name(&lang)))
                    })
                    .map_err(|e| format!("{} non scritto: {e}", out.display()))
            }
            Outcome::Unsupported(e) | Outcome::Failed(e) => Err(e),
        };
        if let Err(e) = &result {
            log(sink, Tone::Error, "Errore", format!("{name}: {e}"));
        }
        let engine_gone = matches!(&result, Err(e) if e.contains("chiuso"));
        sink(Event::Finished { path: path.clone(), result });
        if engine_gone && index + 1 < files.len() {
            // Il motore è caduto: lo si riaccende per i file che restano.
            if let Some(dead) = engine.take() {
                dead.stop();
            }
            if let Some(m) = &model {
                match power_on(request, m, sink) {
                    Ok(r) => engine = Some(r),
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

    fn seg(t0: i64, t1: i64, text: &str) -> Segment {
        Segment { t0, t1, text: text.into() }
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
    fn clock_and_sizes_read_well() {
        assert_eq!(clock(4), "00:00:00");
        assert_eq!(clock(76_000), "00:12:40");
        assert_eq!(clock(366_100), "01:01:01");
        assert_eq!(size_text(148), "150 MB");
        assert_eq!(size_text(488), "490 MB");
        assert_eq!(size_text(1624), "1,6 GB");
        assert_eq!(size_text(874), "870 MB");
    }

    #[test]
    fn pauses_start_new_paragraphs() {
        let segs = [seg(0, 300, "Buongiorno a tutti."), seg(320, 600, "Iniziamo la seduta."), seg(900, 1200, "Primo punto.")];
        let p = paragraphs(&segs);
        assert_eq!(p, vec![(0, "Buongiorno a tutti. Iniziamo la seduta.".to_string()), (900, "Primo punto.".to_string())]);
    }

    #[test]
    fn transcribable_takes_voice_messages_and_videos() {
        assert!(transcribable(Path::new("vocale.opus")));
        assert!(only_transcribable(Path::new("vocale.opus")));
        assert!(transcribable(Path::new("riunione.m4a")) && !only_transcribable(Path::new("riunione.m4a")));
        assert!(transcribable(Path::new("seduta.mp4")));
        assert!(!transcribable(Path::new("foto.jpg")));
    }

    #[test]
    fn outputs_carry_title_times_and_text() {
        let segs = [seg(400, 900, "Buongiorno & benvenuti <tutti>."), seg(2100, 2500, "Primo punto.")];
        let txt = String::from_utf8(text_bytes(Path::new("/x/giunta.m4a"), 3240.0, "it", &segs)).unwrap();
        assert!(txt.starts_with("giunta"));
        assert!(txt.contains("54 min di audio · lingua: italiano"));
        assert!(txt.contains("[00:00:04] Buongiorno & benvenuti <tutti>."));
        assert!(txt.contains("[00:00:21] Primo punto."));
        let docx = docx_bytes(Path::new("/x/giunta.m4a"), 3240.0, "it", &segs);
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
        let request = Request { format: Format::Text, model: 0, gpu: std::env::var_os("CEDRO_TRASCRIVI_GPU").is_some(), avx512: false };
        let lines = std::sync::Mutex::new(Vec::new());
        let finished = std::sync::Mutex::new(None);
        run(&[wav.clone()], &request, Some(&work), &Options::default(), &|event| match event {
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

    /// Un download che si interrompe a metà non lascia il file `.part` sul disco.
    #[test]
    fn a_broken_download_leaves_nothing_behind() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/modello.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 1024];
                let _ = std::io::Read::read(&mut stream, &mut request);
                // Promette un MB e ne manda dieci byte, poi chiude.
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n0123456789");
            }
        });
        let dir = std::env::temp_dir().join(format!("cedro-fetch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("modello.bin");
        assert!(fetch(&url, &dest, None, 1, "il modello", &|_| {}).is_err());
        assert!(!dest.exists());
        assert!(!dir.join("modello.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
