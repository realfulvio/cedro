//! Scarico da YouTube: Cedro non contiene yt-dlp né ffmpeg. Al primo uso li scarica nella cartella
//! di configurazione (`tools`), poi li tiene aggiornati. Qui c'è tutto il resto: i comandi, la
//! coda, l'avanzamento e l'annullamento.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
const FFMPEG_BASE: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/";
const DENO_BASE: &str = "https://github.com/denoland/deno/releases/latest/download/";
/// Dopo quante ore si cerca una versione più nuova di yt-dlp (YouTube cambia spesso).
const UPDATE_EVERY_HOURS: u64 = 12;
/// deno e ffmpeg cambiano poco: li riguardo ogni tanto, non a ogni uso.
const REFRESH_EVERY_DAYS: u64 = 30;

/// Quello che il pannello mostra e il motore esegue.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub urls: Vec<String>,
    pub audio: bool,
    /// Video: altezza massima (0 = la migliore). Audio: 0 migliore, 1 buona, 2 leggera.
    pub quality: u32,
    /// Video: mp4, mkv, webm. Audio: mp3, m4a, opus, wav, flac.
    pub format: String,
    /// 0 nessuno, 1 italiano, 2 italiano e inglese, 3 tutti.
    pub subtitles: u32,
    pub thumbnail: bool,
    pub chapters: bool,
    pub split_chapters: bool,
    pub skip_sponsors: bool,
    pub normalize: bool,
    pub playlist: bool,
    pub skip_existing: bool,
    pub dest: PathBuf,
}

impl Request {
    pub fn new(urls: Vec<String>, audio: bool, dest: PathBuf) -> Request {
        Request {
            urls,
            audio,
            quality: if audio { 1 } else { 1080 },
            format: if audio { "mp3" } else { "mp4" }.into(),
            subtitles: 0,
            thumbnail: true,
            chapters: true,
            split_chapters: false,
            skip_sponsors: false,
            normalize: false,
            playlist: true,
            skip_existing: true,
            dest,
        }
    }
}

/// Cosa si sa di un link prima di scaricarlo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub title: String,
    pub author: String,
    pub seconds: u64,
    /// Quanti video ha, se è una playlist.
    pub entries: Option<usize>,
    /// Pagina letta dall'estrattore generico (nessun estrattore dedicato al sito).
    pub generic: bool,
}

/// Cosa succede durante il lavoro.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Una riga per la finestra «Cosa sta succedendo».
    Log(String),
    /// Avanzamento del link `index` (da 0) di `total`: `percent` da 0 a 100.
    Progress { index: usize, total: usize, percent: f32, label: String },
    /// Un file finito.
    File(PathBuf),
    /// Titolo e durata del link `index`, appena letti.
    Info { index: usize, info: Info },
    /// Il link `index` è finito: `Ok` con quanti file, oppure il motivo dell'errore.
    LinkDone { index: usize, result: Result<usize, String> },
}

/// Un link web qualsiasi (http/https): quale sito sia supportato lo decide yt-dlp.
pub fn is_video_url(text: &str) -> bool {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")) else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
    host.contains('.') && !host.starts_with('.') && !host.ends_with('.') && !t.contains(char::is_whitespace)
}

pub fn is_youtube_url(text: &str) -> bool {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")) else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or("");
    matches!(host, "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "www.youtu.be" | "youtube-nocookie.com" | "www.youtube-nocookie.com")
        && rest.len() > host.len() + 1
}

/// I link dentro un testo incollato (uno per riga o separati da spazi): solo YouTube,
/// oppure (`any`, scheda «Video dal web») di qualsiasi sito.
pub fn urls_in(text: &str, any: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| matches!(c, '<' | '>' | '"' | '\'' | ',' | ';'));
        let ok = if any { is_video_url(word) } else { is_youtube_url(word) };
        if ok && !out.iter().any(|u| u == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// Perché un testo incollato non ha dato link da aggiungere.
pub fn link_note(text: &str, any: bool) -> &'static str {
    if !any && !urls_in(text, true).is_empty() {
        "Questo non è un link di YouTube: per gli altri siti usa «Video dal web»."
    } else if any {
        "Questo non sembra un link: deve iniziare con https://"
    } else {
        "Questo non sembra un link di YouTube."
    }
}

// ---------------------------------------------------------------------------
// Programmi di aiuto

pub fn tools_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    return android::home().join("tools");
    #[cfg(not(target_os = "android"))]
    crate::config_dir().join("tools")
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

pub fn ytdlp_path() -> PathBuf {
    tools_dir().join(exe("yt-dlp"))
}

/// ffmpeg: quello di Cedro se c'è; su Linux anche quello del sistema.
pub fn ffmpeg_path() -> Option<PathBuf> {
    let own = tools_dir().join(exe("ffmpeg"));
    if own.is_file() {
        return Some(own);
    }
    if cfg!(target_os = "linux") {
        if let Some(path) = std::env::var_os("PATH") {
            return std::env::split_paths(&path).map(|d| d.join("ffmpeg")).find(|p| p.is_file());
        }
    }
    None
}

/// ffprobe: lo cerca yt-dlp accanto a ffmpeg (copertine in MKV, informazioni sui file).
fn own_ffprobe() -> PathBuf {
    tools_dir().join(exe("ffprobe"))
}

/// ffmpeg è pronto: quello di Cedro con il suo ffprobe, oppure (Linux) quello del sistema.
fn ffmpeg_ready() -> bool {
    let own = tools_dir().join(exe("ffmpeg"));
    (own.is_file() && own_ffprobe().is_file()) || (!own.is_file() && ffmpeg_path().is_some())
}

/// deno: YouTube chiede a yt-dlp un runtime JavaScript, senza il quale restano solo i formati piccoli.
pub fn deno_path() -> PathBuf {
    tools_dir().join(exe("deno"))
}

/// Stato dei programmi di aiuto, per il pannello: (yt-dlp, ffmpeg, deno).
pub fn helpers_ready() -> (bool, bool, bool) {
    (ytdlp_path().is_file(), ffmpeg_ready(), deno_path().is_file())
}

/// Quello che Cedro scaricherà al primo uso, per dirlo prima; `None` se c'è già tutto.
pub fn first_use_note() -> Option<String> {
    let (y, f, d) = helpers_ready();
    let mut parts: Vec<&str> = Vec::new();
    let mut mb = 0;
    if !y {
        parts.push("yt-dlp");
        mb += 20;
    }
    if !f {
        parts.push("ffmpeg");
        mb += 140;
    }
    if !d {
        parts.push("deno");
        mb += 45;
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("Al primo uso Cedro scarica da solo {} (circa {mb} MB, una volta sola) e poi li tiene aggiornati.", parts.join(", ")))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("cedro/{}", crate::update::CURRENT))
        .build()
}

fn get_text(url: &str) -> Result<String, String> {
    agent().get(url).call().map_err(|e| format!("{url}: {e}"))?.into_string().map_err(|e| e.to_string())
}

/// Scarica `url` in `dest` (via un file `.part`), controllando lo SHA-256 se lo si conosce.
fn download(url: &str, dest: &Path, sha256: Option<&str>, what: &str, emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<(), String> {
    let response = agent().get(url).call().map_err(|e| format!("Non riesco a scaricare {what}: {e}"))?;
    let total: u64 = response.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = response.into_reader();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut last_pct) = (0u64, -1i32);
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err("Annullato".into());
        }
        // Un download a metà non resta sul disco (ffmpeg pesa 140 MB).
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err(format!("Scaricamento di {what} interrotto: {e}"));
            }
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err(format!("Scrittura di {what} non riuscita (disco pieno?): {e}"));
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if total > 0 {
            let pct = (done * 100 / total) as i32;
            if pct != last_pct {
                last_pct = pct;
                emit(Event::Progress { index: 0, total: 1, percent: pct as f32, label: format!("Scarico {what}") });
            }
        }
    }
    drop(file);
    if let Some(expected) = sha256 {
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{what}: il file scaricato non corrisponde al codice di controllo"));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Il codice di controllo di `file` in un elenco «codice  nome» (formato sha256sum).
fn sum_for(list: &str, file: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let sum = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        (name == file || name.ends_with(&format!("/{file}"))).then(|| sum.to_string())
    })
}

fn ytdlp_asset() -> &'static str {
    if cfg!(target_os = "android") {
        // La versione in Python: su Android la fa girare il Python di youtubedl-android.
        "yt-dlp"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "yt-dlp_arm64.exe"
    } else if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    }
}

fn ffmpeg_asset() -> &'static str {
    if cfg!(all(windows, target_arch = "aarch64")) {
        "ffmpeg-master-latest-winarm64-lgpl.zip"
    } else if cfg!(windows) {
        "ffmpeg-master-latest-win64-lgpl.zip"
    } else if cfg!(target_arch = "aarch64") {
        "ffmpeg-master-latest-linuxarm64-lgpl.tar.xz"
    } else {
        "ffmpeg-master-latest-linux64-lgpl.tar.xz"
    }
}

fn deno_asset() -> &'static str {
    if cfg!(all(windows, target_arch = "aarch64")) {
        "deno-aarch64-pc-windows-msvc.zip"
    } else if cfg!(windows) {
        "deno-x86_64-pc-windows-msvc.zip"
    } else if cfg!(target_arch = "aarch64") {
        "deno-aarch64-unknown-linux-gnu.zip"
    } else {
        "deno-x86_64-unknown-linux-gnu.zip"
    }
}

/// Il primo codice SHA-256 (64 cifre esadecimali) in un testo, in qualunque formato sia scritto.
fn first_sha256(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_hexdigit()).find(|w| w.len() == 64).map(|w| w.to_ascii_lowercase())
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    let _ = path;
}

fn command(program: &Path) -> Command {
    #[cfg(target_os = "android")]
    #[allow(unused_mut)]
    let mut c = android::command(program);
    #[cfg(not(target_os = "android"))]
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // niente finestra nera
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0); // un gruppo suo, per fermare anche i figli
    }
    // Se Cedro cade, il programma lanciato cade con lui (il segnale arriva quando muore il thread che l'ha lanciato,
    // cioè quello che lo aspetta).
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        c.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    c
}

/// I gruppi dei programmi in corso (yt-dlp e i suoi figli), da fermare se Cedro esce.
#[cfg(unix)]
static GROUPS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// Tiene nota di un programma appena partito: su Linux il suo gruppo, su Windows lo mette nel job di Cedro,
/// che Windows chiude (fermando tutto l'albero) quando Cedro finisce, in qualunque modo.
fn track(child: &std::process::Child) {
    #[cfg(unix)]
    GROUPS.lock().unwrap_or_else(|e| e.into_inner()).push(child.id());
    #[cfg(windows)]
    join_job(child);
    #[cfg(not(any(unix, windows)))]
    let _ = child;
}

/// Il programma è finito o è stato fermato: non va più fermato all'uscita.
fn untrack(child: &std::process::Child) {
    #[cfg(unix)]
    GROUPS.lock().unwrap_or_else(|e| e.into_inner()).retain(|&g| g != child.id());
    #[cfg(not(unix))]
    let _ = child;
}

/// Ferma yt-dlp, ffmpeg e deno ancora in corso: da chiamare prima di uscire con `process::exit`.
pub fn kill_all() {
    #[cfg(unix)]
    for group in GROUPS.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        kill_group(group);
    }
}

/// Ferma tutto il gruppo di processi guidato da `group` (vedi `command`).
#[cfg(unix)]
fn kill_group(group: u32) {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        libc::killpg(group as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = Command::new("kill").args(["-KILL", "--", &format!("-{group}")]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

/// Windows: un job con «ferma tutto alla chiusura». I figli di yt-dlp (ffmpeg, deno) ci entrano da soli.
#[cfg(windows)]
fn join_job(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    // L'handle come numero: HANDLE non si può tenere in una static. Resta aperto finché Cedro vive.
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let Ok(job) = CreateJobObjectW(None, windows::core::PCWSTR::null()) else { return 0 };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        match SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const core::ffi::c_void, size) {
            Ok(()) => job.0 as usize,
            Err(_) => 0,
        }
    });
    if job != 0 {
        unsafe {
            let _ = AssignProcessToJobObject(HANDLE(job as *mut core::ffi::c_void), HANDLE(child.as_raw_handle()));
        }
    }
}

/// Scarica yt-dlp se manca.
pub fn ensure_ytdlp(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let path = ytdlp_path();
    if path.is_file() {
        return Ok(path);
    }
    emit(Event::Log("yt-dlp non c'è ancora: lo scarico (una volta sola, circa 20 MB)".into()));
    let sums = get_text(&format!("{YTDLP_BASE}SHA2-256SUMS")).ok();
    let sum = sums.as_deref().and_then(|s| sum_for(s, ytdlp_asset()));
    if sum.is_none() {
        return Err("Non trovo il codice di controllo di yt-dlp: riprova più tardi".into());
    }
    download(&format!("{YTDLP_BASE}{}", ytdlp_asset()), &path, sum.as_deref(), "yt-dlp", emit, cancel)?;
    make_executable(&path);
    emit(Event::Log("yt-dlp scaricato".into()));
    Ok(path)
}

/// Scarica ffmpeg se manca (e non c'è sul sistema).
pub fn ensure_ffmpeg(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    return android::ensure_packages(emit, cancel).map(|_| android::ffmpeg());
    #[allow(unreachable_code)]
    let own = tools_dir().join(exe("ffmpeg"));
    if ffmpeg_ready() {
        return Ok(ffmpeg_path().unwrap_or(own));
    }
    emit(Event::Log("ffmpeg non c'è ancora: lo scarico (una volta sola, circa 140 MB)".into()));
    download_ffmpeg(emit, cancel)
}

/// Scarica ffmpeg di Cedro (anche se sul sistema ce n'è un altro).
fn download_ffmpeg(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let own = tools_dir().join(exe("ffmpeg"));
    let dir = tools_dir();
    let archive = dir.join(ffmpeg_asset());
    let sums = get_text(&format!("{FFMPEG_BASE}checksums.sha256")).ok();
    let sum = sums.as_deref().and_then(|s| sum_for(s, ffmpeg_asset()));
    if sum.is_none() {
        return Err("Non trovo il codice di controllo di ffmpeg: riprova più tardi".into());
    }
    download(&format!("{FFMPEG_BASE}{}", ffmpeg_asset()), &archive, sum.as_deref(), "ffmpeg", emit, cancel)?;
    let result = extract_bin(&archive, "ffmpeg", &own).and_then(|_| extract_bin(&archive, "ffprobe", &own_ffprobe()));
    let _ = std::fs::remove_file(&archive);
    result?;
    make_executable(&own);
    make_executable(&own_ffprobe());
    remember_sum("ffmpeg", sum.as_deref());
    emit(Event::Log("ffmpeg scaricato".into()));
    Ok(own)
}

/// Estrae da uno zip il file che finisce con `suffix` e lo scrive in `target`.
fn extract_zip_entry(archive: &Path, suffix: &str, target: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let name = zip.file_names().flatten().find(|n| n.ends_with(suffix)).map(|n| n.into_owned()).ok_or_else(|| format!("{suffix} non è nell'archivio"))?;
    let mut entry = zip.by_name(&name).map_err(|_| "archivio rovinato".to_string())?;
    let part = target.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut entry, &mut out).and_then(|_| out.sync_all());
    drop(out);
    // Un file a metà (disco pieno, archivio rovinato) non resta sul disco.
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(e.to_string());
    }
    std::fs::rename(&part, target).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        e.to_string()
    })
}

/// Scarica deno se manca.
pub fn ensure_deno(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
    // Su Android il runtime JavaScript è QuickJS, nell'APK.
    #[cfg(target_os = "android")]
    return Ok(android::quickjs());
    #[allow(unreachable_code)]
    let target = deno_path();
    if target.is_file() {
        return Ok(target);
    }
    emit(Event::Log("deno non c'è ancora: lo scarico (una volta sola, circa 45 MB)".into()));
    let sum = get_text(&format!("{DENO_BASE}{}.sha256sum", deno_asset())).ok().as_deref().and_then(first_sha256);
    if sum.is_none() {
        return Err("Non trovo il codice di controllo di deno: riprova più tardi".into());
    }
    let archive = tools_dir().join(deno_asset());
    download(&format!("{DENO_BASE}{}", deno_asset()), &archive, sum.as_deref(), "deno", emit, cancel)?;
    let result = extract_zip_entry(&archive, if cfg!(windows) { "deno.exe" } else { "deno" }, &target);
    let _ = std::fs::remove_file(&archive);
    result?;
    make_executable(&target);
    remember_sum("deno", sum.as_deref());
    emit(Event::Log("deno scaricato".into()));
    Ok(target)
}

#[cfg(windows)]
fn extract_bin(archive: &Path, name: &str, target: &Path) -> Result<(), String> {
    extract_zip_entry(archive, &format!("/bin/{name}.exe"), target)
}

#[cfg(not(windows))]
fn extract_bin(archive: &Path, name: &str, target: &Path) -> Result<(), String> {
    let part = target.with_extension("part");
    let out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let status = Command::new("tar")
        .args(["-xJf"])
        .arg(archive)
        .args(["--wildcards", &format!("*/bin/{name}"), "-O"])
        .stdout(out)
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Per aprire ffmpeg serve il programma «tar»: {e}"))?;
    if !status.success() || std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::remove_file(&part);
        return Err("Non riesco ad aprire l'archivio di ffmpeg (servono tar e xz)".into());
    }
    std::fs::rename(part, target).map_err(|e| e.to_string())
}

fn sum_path(name: &str) -> PathBuf {
    tools_dir().join(format!("{name}.sha256"))
}

fn remember_sum(name: &str, sum: Option<&str>) {
    if let Some(sum) = sum {
        let _ = std::fs::write(sum_path(name), sum);
    }
}

/// Il codice di controllo della versione più recente di ffmpeg o deno.
fn remote_sum(name: &str) -> Option<String> {
    if name == "ffmpeg" {
        get_text(&format!("{FFMPEG_BASE}checksums.sha256")).ok().as_deref().and_then(|s| sum_for(s, ffmpeg_asset()))
    } else {
        get_text(&format!("{DENO_BASE}{}.sha256sum", deno_asset())).ok().as_deref().and_then(first_sha256)
    }
}

/// Se ffmpeg o deno scaricati da Cedro hanno una versione più nuova (controllo al massimo ogni
/// `REFRESH_EVERY_DAYS` giorni), li riscarica. Se qualcosa va storto resta quello vecchio.
pub fn refresh_helpers_if_due(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) {
    // Su Android ffmpeg e QuickJS hanno la versione fissata nell'APK.
    if cfg!(target_os = "android") {
        return;
    }
    for name in ["ffmpeg", "deno"] {
        let program = tools_dir().join(exe(name));
        let stamp = tools_dir().join(format!("{name}.controllato"));
        if !program.is_file() {
            continue;
        }
        let due = std::fs::metadata(&stamp)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|age| age > Duration::from_secs(REFRESH_EVERY_DAYS * 86400))
            .unwrap_or(true);
        if !due {
            continue;
        }
        let Some(latest) = remote_sum(name) else { continue };
        let _ = std::fs::write(&stamp, b"ok");
        if std::fs::read_to_string(sum_path(name)).map(|s| s.trim() == latest).unwrap_or(false) {
            continue;
        }
        emit(Event::Log(format!("{name} ha una versione più nuova: la scarico")));
        let backup = program.with_extension("vecchio");
        if std::fs::rename(&program, &backup).is_err() {
            continue;
        }
        let backup_probe = own_ffprobe().with_extension("vecchio");
        let had_probe = name == "ffmpeg" && std::fs::rename(own_ffprobe(), &backup_probe).is_ok();
        let result = if name == "ffmpeg" { download_ffmpeg(emit, cancel) } else { ensure_deno(emit, cancel) };
        if result.is_ok() {
            let _ = std::fs::remove_file(&backup);
            let _ = std::fs::remove_file(&backup_probe);
            let _ = std::fs::remove_file(tools_dir().join(format!("{name}.versione")));
        } else {
            let _ = std::fs::remove_file(&program);
            let _ = std::fs::rename(&backup, &program);
            if had_probe {
                let _ = std::fs::rename(&backup_probe, own_ffprobe());
            }
            emit(Event::Log(format!("Non riesco ad aggiornare {name}: resta la versione che c'è")));
        }
    }
}

fn stamp_path() -> PathBuf {
    tools_dir().join("yt-dlp.aggiornato")
}

/// Cerca una versione più nuova di yt-dlp, al massimo ogni `UPDATE_EVERY_HOURS` ore.
pub fn update_ytdlp_if_due(ytdlp: &Path, emit: &mut dyn FnMut(Event), cancel: &AtomicBool) {
    let due = std::fs::metadata(stamp_path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age > Duration::from_secs(UPDATE_EVERY_HOURS * 3600))
        .unwrap_or(true);
    if !due {
        return;
    }
    emit(Event::Log("Controllo se yt-dlp ha una versione più nuova".into()));
    // Con una rete lenta o un proxy bloccato -U può non finire mai: lo si ferma con Annulla o dopo 2 minuti.
    match capture_until(command(ytdlp).arg("-U"), cancel, Duration::from_secs(120)) {
        Ok((text, _, ok)) => {
            let line = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            if ok {
                let _ = std::fs::write(stamp_path(), b"ok");
                emit(Event::Log(if line.is_empty() { "yt-dlp aggiornato".into() } else { line }));
            } else {
                emit(Event::Log(format!("Non riesco ad aggiornare yt-dlp ({})", if line.is_empty() { "nessuna risposta" } else { &line })));
            }
        }
        Err(e) => emit(Event::Log(format!("Non riesco ad aggiornare yt-dlp: {e}"))),
    }
}

/// Il programma di aiuto (yt-dlp, ffmpeg, deno) se è già sul PC.
fn tool_file(name: &str) -> Option<PathBuf> {
    let path = match name {
        "yt-dlp" => ytdlp_path(),
        "ffmpeg" => ffmpeg_path()?,
        "deno" => deno_path(),
        _ => return None,
    };
    path.is_file().then_some(path)
}

pub fn tool_installed(name: &str) -> bool {
    tool_file(name).is_some()
}

fn tool_version_now(name: &str, path: &Path) -> Option<String> {
    if name == "yt-dlp" {
        return ytdlp_version(path);
    }
    let (text, _, ok) = capture_until(command(path).arg(if name == "ffmpeg" { "-version" } else { "--version" }), &AtomicBool::new(false), Duration::from_secs(30)).ok()?;
    let first = text.lines().next()?;
    let mut words = first.split_whitespace();
    let version = if name == "ffmpeg" { words.nth(2)? } else { words.nth(1)? };
    ok.then(|| version.to_string())
}

fn version_cache(name: &str) -> PathBuf {
    tools_dir().join(format!("{name}.versione"))
}

/// La versione ricordata, se il file è più recente del programma (cioè non è stato aggiornato dopo).
fn cached_version(name: &str, path: &Path) -> Option<String> {
    let cache = version_cache(name);
    let fresh = match (std::fs::metadata(&cache).and_then(|m| m.modified()), std::fs::metadata(path).and_then(|m| m.modified())) {
        (Ok(c), Ok(p)) => c >= p,
        _ => false,
    };
    if !fresh {
        return None;
    }
    let text = std::fs::read_to_string(&cache).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// La versione del programma scaricato, per la scheda Informazioni. Ricordata in un file accanto al programma:
/// se manca (o il programma è stato aggiornato) la cerca in disparte e la mostra la volta dopo.
pub fn installed_version(name: &str) -> Option<String> {
    static BUSY: AtomicBool = AtomicBool::new(false);
    let path = tool_file(name)?;
    if let Some(v) = cached_version(name, &path) {
        return Some(v);
    }
    // Un solo thread per tutti i programmi: la scheda chiede le tre versioni una dopo l'altra.
    if !BUSY.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            for n in ["yt-dlp", "ffmpeg", "deno"] {
                if let Some(p) = tool_file(n) {
                    if cached_version(n, &p).is_none() {
                        if let Some(v) = tool_version_now(n, &p) {
                            let _ = std::fs::write(version_cache(n), v);
                        }
                    }
                }
            }
            BUSY.store(false, Ordering::SeqCst);
        });
    }
    None
}

pub fn ytdlp_version(ytdlp: &Path) -> Option<String> {
    let (text, _, ok) = capture_until(command(ytdlp).arg("--version"), &AtomicBool::new(false), Duration::from_secs(30)).ok()?;
    ok.then(|| text.trim().to_string())
}

// ---------------------------------------------------------------------------
// Informazioni su un link

pub fn parse_info(json: &str) -> Result<Info, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let title = v["title"].as_str().unwrap_or("").to_string();
    if title.is_empty() {
        return Err("Il link non ha un titolo".into());
    }
    // Fuori da YouTube spesso manca l'autore: allora il nome del sito.
    let author = ["uploader", "channel", "webpage_url_domain", "extractor_key"].iter().find_map(|k| v[*k].as_str().filter(|s| !s.is_empty())).unwrap_or("").to_string();
    let entries = v["entries"].as_array().map(|e| e.len());
    let seconds = if entries.is_some() {
        v["entries"].as_array().map(|e| e.iter().map(|x| x["duration"].as_f64().unwrap_or(0.0)).sum::<f64>() as u64).unwrap_or(0)
    } else {
        v["duration"].as_f64().unwrap_or(0.0) as u64
    };
    let generic = v["extractor_key"].as_str() == Some("Generic");
    Ok(Info { title, author, seconds, entries, generic })
}

/// Ferma un processo e i suoi figli (yt-dlp lancia ffmpeg), poi lo aspetta.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = command(Path::new("taskkill")).args(["/PID", &child.id().to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(unix)]
    {
        // Il processo guida un gruppo suo (vedi `command`): si ferma tutto il gruppo.
        kill_group(child.id());
    }
    let _ = child.kill();
    let _ = child.wait();
    untrack(child);
}

/// Esegue `cmd` e ne raccoglie le uscite, fermandolo se `cancel` si accende.
fn capture(cmd: &mut Command, cancel: &AtomicBool) -> Result<(String, String, bool), String> {
    capture_until(cmd, cancel, Duration::from_secs(365 * 86400))
}

/// Come `capture`, ma lo ferma anche dopo `limit`.
fn capture_until(cmd: &mut Command, cancel: &AtomicBool, limit: Duration) -> Result<(String, String, bool), String> {
    let deadline = std::time::Instant::now() + limit;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    track(&child);
    let mut out = child.stdout.take().ok_or("niente uscita")?;
    let mut err = child.stderr.take().ok_or("niente uscita")?;
    let t_out = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
        text
    });
    let t_err = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err.read_to_string(&mut text);
        text
    });
    let status = loop {
        let late = std::time::Instant::now() >= deadline;
        if cancel.load(Ordering::Relaxed) || late {
            kill_tree(&mut child);
            let _ = (t_out.join(), t_err.join());
            return Err(if late { "tempo scaduto".into() } else { "Annullato".into() });
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                kill_tree(&mut child);
                let _ = (t_out.join(), t_err.join());
                return Err(e.to_string());
            }
        }
    };
    untrack(&child);
    Ok((t_out.join().unwrap_or_default(), t_err.join().unwrap_or_default(), status.success()))
}

/// Titolo, autore e durata di un link, senza scaricarlo.
pub fn probe(ytdlp: &Path, url: &str, playlist: bool, cancel: &AtomicBool) -> Result<Info, String> {
    let mut cmd = command(ytdlp);
    cmd.args(["--dump-single-json", "--flat-playlist", "--no-warnings", "--skip-download"]).arg(if playlist { "--yes-playlist" } else { "--no-playlist" }).arg("--").arg(url);
    let (stdout, stderr, ok) = capture(&mut cmd, cancel)?;
    if !ok {
        return Err(first_error(&stderr));
    }
    parse_info(&stdout)
}

/// L'errore di yt-dlp in una riga, in italiano quando lo riconosciamo.
pub fn first_error(stderr: &str) -> String {
    let line = stderr.lines().rev().find(|l| l.contains("ERROR")).unwrap_or(stderr.lines().last().unwrap_or("")).trim();
    let line = line.trim_start_matches("ERROR:").trim();
    let low = line.to_lowercase();
    if low.contains("unsupported url") {
        "Questo sito non è supportato (o il link non porta a un video)".into()
    } else if low.contains("drm") {
        "Il video è protetto da DRM e non si può scaricare".into()
    } else if low.contains("geo restrict") || low.contains("not available in your country") {
        "Il video non è disponibile nel tuo Paese".into()
    } else if low.contains("login required") || low.contains("requires authentication") || low.contains("logged-in") || low.contains("--cookies") {
        "Il video richiede un accesso: non posso scaricarlo".into()
    } else if low.contains("private video") {
        "Il video è privato".into()
    } else if low.contains("sign in to confirm your age") || low.contains("age-restricted") {
        "Il video è per maggiorenni: serve l'accesso a YouTube".into()
    } else if low.contains("unavailable") || low.contains("this video is not available") || low.contains("has been removed") {
        "Il video non è disponibile".into()
    } else if low.contains("members-only") || low.contains("join this channel") {
        "Il video è solo per gli iscritti al canale".into()
    } else if low.contains("not a bot") || low.contains("sign in to confirm") {
        "YouTube chiede di confermare che non sei un robot: riprova più tardi".into()
    } else if low.contains("http error 429") || low.contains("too many requests") {
        "Il sito ha limitato le richieste: riprova fra qualche minuto".into()
    } else if low.contains("http error 403") || low.contains("http error 401") {
        "Il sito ha rifiutato la richiesta: riprova più tardi".into()
    } else if low.contains("http error") || low.contains("unable to download") && !low.contains("getaddrinfo") && !low.contains("timed out") && !low.contains("connection") && !low.contains("unreachable") {
        format!("Il sito ha risposto con un errore: {line}")
    } else if low.contains("getaddrinfo") || low.contains("timed out") || low.contains("connection") || low.contains("unreachable") || low.contains("name resolution") {
        "Non riesco a raggiungere il sito: controlla la connessione".into()
    } else if line.is_empty() {
        "yt-dlp non ha dato risposta".into()
    } else {
        line.to_string()
    }
}

// ---------------------------------------------------------------------------
// Il comando

fn quality_audio(q: u32) -> &'static str {
    match q {
        0 => "0",
        1 => "5",
        _ => "8",
    }
}

/// Gli argomenti di yt-dlp per un link. Non include il link.
/// `in_playlist`: il link è una playlist, i file vanno in una cartella con il suo nome.
pub fn build_args(r: &Request, ffmpeg: Option<&Path>, deno: Option<&Path>, in_playlist: bool) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    push(&["--newline", "--no-quiet", "--no-colors", "--ignore-config", "--no-mtime", "--embed-metadata"]);
    push(&["--progress-template", "download:CEDRO|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s"]);
    push(&["--print", "after_move:CEDROFILE|%(filepath)s"]);
    if cfg!(windows) {
        push(&["--windows-filenames"]);
    }
    if let Some(f) = ffmpeg {
        a.push("--ffmpeg-location".into());
        a.push(f.to_string_lossy().into_owned());
    }
    if let Some(d) = deno {
        a.push("--js-runtimes".into());
        a.push(format!("{}:{}", if cfg!(target_os = "android") { "quickjs" } else { "deno" }, d.display()));
    }
    // I file provvisori (parti, copertine) stanno in una cartella a parte, tolta a fine lavoro.
    a.push("-P".into());
    a.push(format!("temp:{}", temp_dir(&r.dest).display()));
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    push(&[if r.playlist { "--yes-playlist" } else { "--no-playlist" }]);
    if r.skip_existing {
        push(&["--no-overwrites"]);
    } else {
        push(&["--force-overwrites"]);
    }
    push(&["-o", if in_playlist { "%(playlist_title).100B/%(playlist_index)02d - %(title).150B.%(ext)s" } else { "%(title).150B.%(ext)s" }]);
    if r.skip_sponsors {
        push(&["--sponsorblock-remove", "sponsor,selfpromo"]);
    }
    if r.audio {
        push(&["-f", "ba/b", "-x", "--audio-format", &r.format, "--audio-quality", quality_audio(r.quality)]);
        if r.normalize {
            push(&["--postprocessor-args", "ExtractAudio:-af loudnorm=I=-16:TP=-1.5:LRA=11"]);
        }
        if r.thumbnail && r.format != "wav" {
            push(&["--embed-thumbnail"]);
        }
    } else {
        let h = if r.quality > 0 { format!("[height<={}]", r.quality) } else { String::new() };
        // In fondo `b*`: qualunque formato, per i siti letti dall'estrattore generico (html5).
        let h2 = if r.quality > 0 { format!("[height<=?{}]", r.quality) } else { String::new() };
        let sel = format!("bv*{h}+ba/b{h}/bv*{h2}+ba/b*{h2}");
        push(&["-f", &sel]);
        if r.format == "mp4" {
            push(&["-S", "ext:mp4:m4a,vcodec:h264"]);
        }
        push(&["--merge-output-format", &r.format]);
        // La copertina si incorpora solo dove yt-dlp sa farlo (non in WebM).
        if r.thumbnail && matches!(r.format.as_str(), "mp4" | "mkv") {
            push(&["--embed-thumbnail"]);
        }
        match r.subtitles {
            0 => {}
            1 => push(&["--write-subs", "--write-auto-subs", "--sub-langs", "it,it-orig", "--embed-subs"]),
            2 => push(&["--write-subs", "--write-auto-subs", "--sub-langs", "it,it-orig,en,en-orig", "--embed-subs"]),
            _ => push(&["--write-subs", "--sub-langs", "all,-live_chat", "--embed-subs"]),
        }
    }
    if r.chapters {
        push(&["--embed-chapters"]);
    }
    if r.split_chapters {
        push(&["--split-chapters", "-o", "chapter:%(title).100B/%(section_number)02d - %(section_title).100B.%(ext)s"]);
    }
    a
}

fn temp_dir(dest: &Path) -> PathBuf {
    dest.join(".cedro-temp")
}

/// Una riga di avanzamento (`CEDRO|scaricati|totale|stimato|velocità|secondi`).
pub fn parse_progress(line: &str) -> Option<(f32, String)> {
    let rest = line.strip_prefix("CEDRO|")?;
    let f: Vec<&str> = rest.split('|').collect();
    if f.len() < 5 {
        return None;
    }
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let done = num(f[0])?;
    let total = num(f[1]).or_else(|| num(f[2]))?;
    if total <= 0.0 {
        return None;
    }
    let percent = ((done / total) * 100.0).clamp(0.0, 100.0) as f32;
    let mut label = format!("{:.0}% · {} di {}", percent, size(done), size(total));
    if let Some(speed) = num(f[3]) {
        label.push_str(&format!(" · {}/s", size(speed)));
    }
    if let Some(eta) = f.get(4).and_then(|s| num(s)) {
        label.push_str(&format!(" · restano {}", eta_text(eta as u64)));
    }
    Some((percent, label))
}

/// 6128 → «1:42:08»; 125 → «2:05».
pub fn clock(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}:{:02}:{:02}", seconds / 3600, seconds % 3600 / 60, seconds % 60)
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

fn size(bytes: f64) -> String {
    if bytes >= 1_073_741_824.0 {
        format!("{:.1} GB", bytes / 1_073_741_824.0).replace('.', ",")
    } else if bytes >= 1_048_576.0 {
        format!("{:.0} MB", bytes / 1_048_576.0)
    } else {
        format!("{:.0} KB", bytes / 1024.0)
    }
}

fn eta_text(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{} h {} min", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{} min", (seconds + 30) / 60)
    } else {
        format!("{seconds} s")
    }
}

/// «[download] Downloading item 3 of 18» → (3, 18). Solo all'inizio della riga: un titolo che contiene
/// le stesse parole (nel nome del file) non conta.
pub fn parse_item(line: &str) -> Option<(usize, usize)> {
    let line = line.trim_start();
    let rest = line.strip_prefix("[download] Downloading item ").or_else(|| line.strip_prefix("[download] Downloading video "))?;
    let mut it = rest.split_whitespace();
    let n: usize = it.next()?.parse().ok()?;
    if it.next()? != "of" {
        return None;
    }
    let m: usize = it.next()?.parse().ok()?;
    (1..=m).contains(&n).then_some((n, m))
}

/// Righe di yt-dlp che non dicono nulla a chi guarda (copertine mancanti e simili).
fn noise(line: &str) -> bool {
    line.contains("Thumbnail") && line.contains("does not exist")
}

enum Line {
    Out(String),
    Err(String),
}

/// Scarica tutti i link della richiesta. Restituisce i file prodotti; un link che fallisce non
/// ferma gli altri, ma se nessuno riesce il risultato è un errore.
pub fn run(r: &Request, emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<Vec<PathBuf>, String> {
    if r.urls.is_empty() {
        return Err("Incolla almeno un link di YouTube".into());
    }
    std::fs::create_dir_all(&r.dest).map_err(|e| format!("La cartella di destinazione non si apre: {e}"))?;
    let ytdlp = ensure_ytdlp(emit, cancel)?;
    refresh_helpers_if_due(emit, cancel);
    let ffmpeg = ensure_ffmpeg(emit, cancel)?;
    let deno = ensure_deno(emit, cancel)?;
    update_ytdlp_if_due(&ytdlp, emit, cancel);
    // La versione ricordata per Informazioni, se c'è: yt-dlp.exe impiega qualche secondo solo a partire.
    let version = cached_version("yt-dlp", &ytdlp).or_else(|| {
        let v = ytdlp_version(&ytdlp)?;
        let _ = std::fs::write(version_cache("yt-dlp"), &v);
        Some(v)
    });
    if let Some(v) = version {
        emit(Event::Log(format!("yt-dlp versione {v}")));
    }
    let mut files = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (index, url) in r.urls.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Annullato".into());
        }
        emit(Event::Log(format!("Link {} di {}: {url}", index + 1, r.urls.len())));
        emit(Event::Progress { index, total: r.urls.len(), percent: 0.0, label: "Preparo il download".into() });
        let outcome = match probe(&ytdlp, url, r.playlist, cancel) {
            Ok(info) => {
                // Una pagina qualunque con più video incorporati (anteprime, correlati): solo il primo.
                let mut info = info;
                let single = info.generic && info.entries.is_some();
                if single {
                    info.entries = None;
                }
                let mut args = build_args(r, Some(&ffmpeg), Some(&deno), info.entries.is_some());
                if single {
                    args.extend(["--playlist-items".to_string(), "1".to_string()]);
                }
                emit(Event::Log(match info.entries {
                    Some(n) => format!("Playlist «{}»: {n} video", info.title),
                    None => format!("«{}» · {}", info.title, clock(info.seconds)),
                }));
                emit(Event::Info { index, info });
                run_one(&ytdlp, &args, url, &r.dest, index, r.urls.len(), emit, cancel)
            }
            Err(e) => Err(e),
        };
        let _ = std::fs::remove_dir_all(temp_dir(&r.dest));
        match outcome {
            Ok(mut got) => {
                emit(Event::LinkDone { index, result: Ok(got.len()) });
                files.append(&mut got);
            }
            Err(e) if e == "Annullato" => return Err(e),
            Err(e) => {
                emit(Event::Log(format!("Errore: {e}")));
                emit(Event::LinkDone { index, result: Err(e.clone()) });
                failures.push(e);
            }
        }
    }
    if files.is_empty() {
        return Err(failures.into_iter().next().unwrap_or_else(|| "Nessun file scaricato".into()));
    }
    Ok(files)
}

#[allow(clippy::too_many_arguments)]
fn run_one(ytdlp: &Path, args: &[String], url: &str, dest: &Path, index: usize, total: usize, emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<Vec<PathBuf>, String> {
    let mut child = command(ytdlp)
        .args(args)
        .arg("--")
        .arg(url)
        .current_dir(dest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("yt-dlp non parte: {e}"))?;
    track(&child);
    let (tx, rx) = mpsc::channel::<Line>();
    let out = child.stdout.take().ok_or("yt-dlp senza uscita")?;
    let err = child.stderr.take().ok_or("yt-dlp senza uscita")?;
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(Line::Out(line)).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if tx2.send(Line::Err(line)).is_err() {
                break;
            }
        }
    });
    let mut files = Vec::new();
    let mut errors = String::new();
    let (mut item, mut items) = (1usize, 1usize);
    loop {
        if cancel.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            return Err("Annullato".into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Line::Out(line)) => {
                if let Some((p, label)) = parse_progress(&line) {
                    // con una playlist l'avanzamento del link si divide per video
                    let percent = (item.saturating_sub(1) as f32 * 100.0 + p) / items as f32;
                    emit(Event::Progress { index, total, percent, label });
                } else if let Some(path) = line.strip_prefix("CEDROFILE|") {
                    let path = dest.join(path.trim());
                    emit(Event::Log(format!("Pronto: {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
                    emit(Event::File(path.clone()));
                    files.push(path);
                } else if let Some((n, m)) = parse_item(&line) {
                    item = n;
                    items = m.max(1);
                    emit(Event::Log(format!("Video {n} di {m} della playlist")));
                } else if !line.trim().is_empty() && !noise(&line) {
                    emit(Event::Log(line));
                }
            }
            Ok(Line::Err(line)) => {
                errors.push_str(&line);
                errors.push('\n');
                if !line.trim().is_empty() && !noise(&line) {
                    emit(Event::Log(line));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait();
    untrack(&child);
    let status = status.map_err(|e| e.to_string())?;
    if files.is_empty() && !status.success() {
        return Err(first_error(&errors));
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Android

/// Su Android i programmi non si possono eseguire dalla cartella dell'app: Python (che fa girare
/// yt-dlp), QuickJS e ffmpeg partono dalla cartella delle librerie dell'APK, dove li mette
/// `android/build.sh`. Le loro librerie (Python 14 MB, ffmpeg 35 MB) si scaricano al primo uso da
/// youtubedl-android su Maven Central, leggendo dall'archivio solo le parti per arm64.
#[cfg(target_os = "android")]
pub mod android {
    use super::*;
    use std::io::{Seek, SeekFrom};
    use std::sync::OnceLock;

    struct Dirs {
        libs: PathBuf,
        home: PathBuf,
    }

    static DIRS: OnceLock<Dirs> = OnceLock::new();

    /// L'app dice dove sono gli eseguibili (`nativeLibraryDir`) e la sua cartella privata.
    pub fn set_dirs(libs: PathBuf, home: PathBuf) {
        let _ = DIRS.set(Dirs { libs, home });
    }

    pub(super) fn libs() -> PathBuf {
        DIRS.get().map(|d| d.libs.clone()).unwrap_or_default()
    }

    pub(super) fn home() -> PathBuf {
        DIRS.get().map(|d| d.home.clone()).unwrap_or_else(|| PathBuf::from("/data/local/tmp"))
    }

    const MAVEN: &str = "https://repo1.maven.org/maven2/io/github/junkfood02/youtubedl-android";
    const VERSION: &str = "0.18.1";
    /// (archivio su Maven, voce dentro l'archivio, cartella in `tools`, SHA-256 della voce, MB).
    const PACKAGES: [(&str, &str, &str, &str, u32); 2] = [
        ("library", "jni/arm64-v8a/libpython.zip.so", "python", "7edecb6c9bd3a94456168d44911d8aea9f917bedef90c320a29500d92ac3eb62", 14),
        ("ffmpeg", "jni/arm64-v8a/libffmpeg.zip.so", "ffmpeg", "4664a895283f9f31786be39b0c484c1f96631f02c7c7f9c70a4a4a2da324a092", 35),
    ];

    fn python_home() -> PathBuf {
        tools_dir().join("python").join("usr")
    }

    /// yt-dlp passa da Python; ffmpeg e QuickJS partono da soli. Tutti trovano le librerie scaricate.
    pub(super) fn command(program: &Path) -> Command {
        let mut c = if program.file_name().is_some_and(|n| n == "yt-dlp") {
            let mut c = Command::new(libs().join("libpython.so"));
            c.arg(program);
            c
        } else {
            Command::new(program)
        };
        let py = python_home();
        let ff = tools_dir().join("ffmpeg").join("usr");
        c.env("LD_LIBRARY_PATH", format!("{}:{}", py.join("lib").display(), ff.join("lib").display()))
            .env("PYTHONHOME", &py)
            .env("SSL_CERT_FILE", py.join("etc").join("tls").join("cert.pem"))
            .env("HOME", home())
            .env("TMPDIR", tools_dir().join("tmp"));
        c
    }

    pub(super) fn ffmpeg() -> PathBuf {
        libs().join("libffmpeg.so")
    }

    pub(super) fn quickjs() -> PathBuf {
        libs().join("libqjs.so")
    }

    fn ready(dir: &str, sum: &str) -> bool {
        std::fs::read_to_string(tools_dir().join(dir).join(".pronto")).is_ok_and(|s| s.trim() == sum)
    }

    /// Scarica ed estrae Python e ffmpeg se mancano (una volta sola, circa 50 MB).
    pub(super) fn ensure_packages(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<(), String> {
        std::fs::create_dir_all(tools_dir().join("tmp")).map_err(|e| e.to_string())?;
        for (aar, entry, dir, sum, mb) in PACKAGES {
            if ready(dir, sum) {
                continue;
            }
            emit(Event::Log(format!("{dir} non c'è ancora: lo scarico (una volta sola, circa {mb} MB)")));
            let archive = tools_dir().join(format!("{dir}.zip"));
            let url = format!("{MAVEN}/{aar}/{VERSION}/{aar}-{VERSION}.aar");
            let fetched = fetch_entry(&url, entry, &archive, sum, dir, emit, cancel);
            let result = fetched.and_then(|_| unpack(&archive, &tools_dir().join(dir)));
            let _ = std::fs::remove_file(&archive);
            result?;
            std::fs::write(tools_dir().join(dir).join(".pronto"), sum).map_err(|e| e.to_string())?;
            emit(Event::Log(format!("{dir} pronto")));
        }
        Ok(())
    }

    /// Copia una voce di uno zip remoto in `dest`, controllando lo SHA-256.
    fn fetch_entry(url: &str, entry: &str, dest: &Path, sum: &str, what: &str, emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<(), String> {
        let remote = Remote::open(url).map_err(|e| format!("Non riesco a scaricare {what}: {e}"))?;
        let mut zip = zip::ZipArchive::new(remote).map_err(|e| format!("Non riesco a leggere l'archivio di {what}: {e}"))?;
        let mut file = zip.by_name(entry).map_err(|e| format!("{entry} non è nell'archivio: {e}"))?;
        let total = file.size().max(1);
        let part = dest.with_extension("part");
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let (mut done, mut last_pct) = (0u64, -1i32);
        let fail = |e: String| {
            let _ = std::fs::remove_file(&part);
            Err(e)
        };
        loop {
            if cancel.load(Ordering::Relaxed) {
                return fail("Annullato".into());
            }
            let n = match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return fail(format!("Scaricamento di {what} interrotto: {e}")),
            };
            if let Err(e) = out.write_all(&buf[..n]) {
                return fail(format!("Scrittura di {what} non riuscita (memoria piena?): {e}"));
            }
            hasher.update(&buf[..n]);
            done += n as u64;
            let pct = (done * 100 / total) as i32;
            if pct != last_pct {
                last_pct = pct;
                emit(Event::Progress { index: 0, total: 1, percent: pct as f32, label: format!("Scarico {what}") });
            }
        }
        drop(out);
        if !hex(&hasher.finalize()).eq_ignore_ascii_case(sum) {
            return fail(format!("{what}: il file scaricato non corrisponde al codice di controllo"));
        }
        std::fs::rename(&part, dest).map_err(|e| e.to_string())
    }

    /// Estrae uno zip con i suoi collegamenti simbolici (le librerie di Termux ne hanno molti).
    fn unpack(archive: &Path, dir: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::remove_dir_all(dir);
        let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = f.enclosed_name() else { continue };
            let path = dir.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if f.is_symlink() {
                let mut target = String::new();
                f.read_to_string(&mut target).map_err(|e| e.to_string())?;
                let _ = std::fs::remove_file(&path);
                std::os::unix::fs::symlink(target, &path).map_err(|e| e.to_string())?;
                continue;
            }
            let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
            if let Some(mode) = f.unix_mode() {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777));
            }
        }
        Ok(())
    }

    /// Un file remoto letto a pezzi con le richieste «Range»: dello zip si scarica solo quello che serve.
    struct Remote {
        url: String,
        len: u64,
        pos: u64,
        buf: Vec<u8>,
        buf_at: u64,
    }

    const CHUNK: u64 = 4 * 1024 * 1024;

    impl Remote {
        fn open(url: &str) -> Result<Remote, String> {
            let r = agent().head(url).call().map_err(|e| e.to_string())?;
            let len = r.header("Content-Length").and_then(|v| v.parse().ok()).ok_or("dimensione sconosciuta")?;
            Ok(Remote { url: url.to_string(), len, pos: 0, buf: Vec::new(), buf_at: 0 })
        }
    }

    impl Read for Remote {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.len || out.is_empty() {
                return Ok(0);
            }
            let inside = self.pos >= self.buf_at && self.pos < self.buf_at + self.buf.len() as u64;
            if !inside {
                let end = (self.pos + CHUNK.max(out.len() as u64)).min(self.len) - 1;
                let range = format!("bytes={}-{end}", self.pos);
                let mut attempt = 0;
                let r = loop {
                    match agent().get(&self.url).set("Range", &range).call() {
                        // Maven Central a volte risponde «troppe richieste»: si riprova con calma.
                        Err(ureq::Error::Status(429, _)) if attempt < 4 => {
                            attempt += 1;
                            std::thread::sleep(Duration::from_secs(5 * attempt));
                        }
                        other => break other.map_err(|e| std::io::Error::other(e.to_string()))?,
                    }
                };
                if r.status() != 206 {
                    return Err(std::io::Error::other("il server non manda pezzi del file"));
                }
                let mut buf = Vec::with_capacity((end + 1 - self.pos) as usize);
                r.into_reader().read_to_end(&mut buf)?;
                if buf.is_empty() {
                    return Err(std::io::Error::other("risposta vuota"));
                }
                self.buf = buf;
                self.buf_at = self.pos;
            }
            let from = (self.pos - self.buf_at) as usize;
            let n = out.len().min(self.buf.len() - from);
            out[..n].copy_from_slice(&self.buf[from..from + n]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for Remote {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            let next = match to {
                SeekFrom::Start(n) => n as i64,
                SeekFrom::End(n) => self.len as i64 + n,
                SeekFrom::Current(n) => self.pos as i64 + n,
            };
            if next < 0 {
                return Err(std::io::Error::other("posizione prima dell'inizio"));
            }
            self.pos = next as u64;
            Ok(self.pos)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::sync::Arc;

    fn req(audio: bool) -> Request {
        Request::new(vec!["https://youtu.be/x".into()], audio, PathBuf::from("/tmp/d"))
    }

    #[test]
    fn recognises_youtube_links() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(is_youtube_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_youtube_url("https://music.youtube.com/playlist?list=PL1"));
        assert!(!is_youtube_url("https://youtube.com.evil.example/watch?v=1"));
        assert!(!is_youtube_url("https://www.youtube.com"));
        assert!(!is_youtube_url("ftp://youtu.be/x"));
        assert!(!is_youtube_url("ciao"));
    }

    #[test]
    fn finds_links_in_pasted_text() {
        let text = "guarda <https://youtu.be/aaa>, poi\nhttps://www.youtube.com/watch?v=bbb e di nuovo https://youtu.be/aaa";
        assert_eq!(urls_in(text, false), vec!["https://youtu.be/aaa", "https://www.youtube.com/watch?v=bbb"]);
    }

    #[test]
    fn video_arguments() {
        let a = build_args(&req(false), Some(Path::new("/x/ffmpeg")), Some(Path::new("/x/deno")), false);
        let j = a.join(" ");
        assert!(j.contains("-f bv*[height<=1080]+ba/b[height<=1080]/bv*[height<=?1080]+ba/b*[height<=?1080]"), "{j}");
        assert!(j.contains("--merge-output-format mp4"));
        assert!(j.contains("-S ext:mp4:m4a,vcodec:h264"));
        assert!(j.contains("--ffmpeg-location /x/ffmpeg") && j.contains("--js-runtimes deno:/x/deno") && j.contains(&format!("-P temp:{}", Path::new("/tmp/d").join(".cedro-temp").display())), "{j}");
        assert!(j.contains("--embed-thumbnail") && j.contains("--embed-chapters") && j.contains("--yes-playlist") && j.contains("--no-overwrites"));
        assert!(!j.contains("--write-subs") && !j.contains("-x "));
    }

    #[test]
    fn best_video_has_no_height_limit_and_mkv_skips_mp4_sort() {
        let mut r = req(false);
        r.quality = 0;
        r.format = "mkv".into();
        let j = build_args(&r, None, None, false).join(" ");
        assert!(j.contains("-f bv*+ba/b/bv*+ba/b*") && !j.contains("height"));
        assert!(!j.contains("-S ") && j.contains("--merge-output-format mkv"));
    }

    #[test]
    fn playlist_files_go_in_a_folder() {
        let one = build_args(&req(false), None, None, false).join(" ");
        let many = build_args(&req(false), None, None, true).join(" ");
        assert!(one.contains("-o %(title).150B.%(ext)s"));
        assert!(many.contains("-o %(playlist_title).100B/%(playlist_index)02d - %(title).150B.%(ext)s"));
    }

    #[test]
    fn no_thumbnail_embedding_in_webm() {
        let mut r = req(false);
        r.format = "webm".into();
        assert!(!build_args(&r, None, None, false).join(" ").contains("--embed-thumbnail"));
        r.format = "mkv".into();
        assert!(build_args(&r, None, None, false).join(" ").contains("--embed-thumbnail"));
    }

    #[test]
    fn subtitle_choices() {
        let mut r = req(false);
        r.subtitles = 1;
        assert!(build_args(&r, None, None, false).join(" ").contains("--sub-langs it,it-orig --embed-subs"));
        r.subtitles = 2;
        assert!(build_args(&r, None, None, false).join(" ").contains("it,it-orig,en,en-orig"));
        r.subtitles = 3;
        assert!(build_args(&r, None, None, false).join(" ").contains("all,-live_chat"));
    }

    #[test]
    fn audio_arguments() {
        let mut r = req(true);
        r.normalize = true;
        r.skip_sponsors = true;
        r.split_chapters = true;
        r.playlist = false;
        r.skip_existing = false;
        let j = build_args(&r, None, None, false).join(" ");
        assert!(j.contains("-x --audio-format mp3 --audio-quality 5"), "{j}");
        assert!(j.contains("loudnorm") && j.contains("--sponsorblock-remove") && j.contains("--split-chapters"));
        assert!(j.contains("--no-playlist") && j.contains("--force-overwrites"));
        assert!(!j.contains("--merge-output-format"));
        r.format = "wav".into();
        assert!(!build_args(&r, None, None, false).join(" ").contains("--embed-thumbnail"));
    }

    #[test]
    fn parses_progress_lines() {
        let (p, label) = parse_progress("CEDRO|412000000|880000000|NA|9876543|50").unwrap();
        assert!((p - 46.8).abs() < 0.1, "{p}");
        assert_eq!(label, "47% · 393 MB di 839 MB · 9 MB/s · restano 50 s");
        let (p, _) = parse_progress("CEDRO|100|NA|400|NA|NA").unwrap();
        assert_eq!(p, 25.0);
        assert!(parse_progress("CEDRO|1|NA|NA|NA|NA").is_none());
        assert!(parse_progress("[download] 5%").is_none());
    }

    #[test]
    fn parses_playlist_items() {
        assert_eq!(parse_item("[download] Downloading item 3 of 18"), Some((3, 18)));
        assert_eq!(parse_item("[download] Downloading video 2 of 5"), Some((2, 5)));
        assert_eq!(parse_item("[download] 50%"), None);
        // Un titolo con le stesse parole, o numeri fuori misura, non è un avanzamento della playlist.
        assert_eq!(parse_item("[download] Destination: /tmp/Downloading item 0 of 5 x.webm"), None);
        assert_eq!(parse_item("[download] Downloading item 0 of 5"), None);
        assert_eq!(parse_item("[download] Downloading item 9 of 2"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_stuck_program_is_stopped_in_time() {
        let started = std::time::Instant::now();
        let got = capture_until(command(Path::new("sleep")).arg("30"), &AtomicBool::new(false), Duration::from_millis(300));
        assert_eq!(got, Err("tempo scaduto".to_string()));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(capture(command(Path::new("sleep")).arg("1"), &AtomicBool::new(true)), Err("Annullato".to_string()));
    }

    #[test]
    fn parses_info_for_a_video_and_a_playlist() {
        let v = parse_info(r#"{"title":"Consiglio","uploader":"Comune","duration":6128.0}"#).unwrap();
        assert_eq!((v.title.as_str(), v.author.as_str(), v.seconds, v.entries), ("Consiglio", "Comune", 6128, None));
        let p = parse_info(r#"{"title":"Estate","channel":"Pro Loco","entries":[{"duration":60},{"duration":30.5}]}"#).unwrap();
        assert_eq!((p.entries, p.seconds, p.author.as_str()), (Some(2), 90, "Pro Loco"));
        assert!(parse_info("{}").is_err());
    }

    #[test]
    fn clock_text() {
        assert_eq!((clock(6128), clock(125), clock(5)), ("1:42:08".to_string(), "2:05".to_string(), "0:05".to_string()));
    }

    #[test]
    fn sha256_in_any_format() {
        let sum = "e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609";
        assert_eq!(first_sha256(&format!("{sum}  deno-x86_64-unknown-linux-gnu.zip")).as_deref(), Some(sum));
        assert_eq!(first_sha256(&format!("Algorithm : SHA256\r\nHash      : {}\r\nPath : x", sum.to_uppercase())).as_deref(), Some(sum));
        assert!(first_sha256("niente").is_none());
    }

    #[test]
    fn quiet_about_missing_thumbnails() {
        assert!(noise("[info] Video Thumbnail 43..38 does not exist"));
        assert!(!noise("[download] Destination: a.mp4"));
    }

    #[test]
    fn checksum_lookup() {
        let list = "aaa  yt-dlp\nbbb *yt-dlp.exe\nccc  ./ffmpeg-master-latest-win64-lgpl.zip\n";
        assert_eq!(sum_for(list, "yt-dlp.exe").as_deref(), Some("bbb"));
        assert_eq!(sum_for(list, "yt-dlp").as_deref(), Some("aaa"));
        assert_eq!(sum_for(list, "ffmpeg-master-latest-win64-lgpl.zip").as_deref(), Some("ccc"));
        assert!(sum_for(list, "altro").is_none());
    }

    #[test]
    fn any_site_link_is_accepted() {
        assert!(is_video_url("https://vimeo.com/76979871"));
        assert!(is_video_url("https://archive.org/details/x"));
        assert!(is_video_url("https://www.youtube.com/watch?v=abc"));
        assert!(!is_video_url("vimeo.com/1"));
        assert!(!is_video_url("https://localhost/x"));
        assert!(!is_video_url("https://a.b/x y"));
        assert_eq!(urls_in("ciao https://vimeo.com/1 e https://archive.org/details/y", true), vec!["https://vimeo.com/1", "https://archive.org/details/y"]);
        assert!(urls_in("https://vimeo.com/1", false).is_empty());
        assert_eq!(link_note("https://vimeo.com/1", false), "Questo non è un link di YouTube: per gli altri siti usa «Video dal web».");
        let info = parse_info(r#"{"title":"Film","duration":60,"webpage_url_domain":"archive.org"}"#).unwrap();
        assert_eq!(info.author, "archive.org");
        assert_eq!(first_error("ERROR: Unsupported URL: https://x.it/"), "Questo sito non è supportato (o il link non porta a un video)");
        assert_eq!(first_error("ERROR: [vimeo] 1: The web client only works when logged-in. Use --cookies"), "Il video richiede un accesso: non posso scaricarlo");
    }

    /// Prova vera, in rete, su siti leciti: `cargo test --release real_sites -- --ignored --nocapture`.
    /// Scarica yt-dlp e FFmpeg come al primo uso; legge Vimeo e Internet Archive, scarica un video
    /// da almeno uno dei due e verifica il messaggio di un sito non supportato.
    #[test]
    #[ignore]
    fn real_sites() {
        let cancel = AtomicBool::new(false);
        let mut log = |e: Event| {
            if let Event::Log(t) = e {
                println!("  {t}");
            }
        };
        let ytdlp = ensure_ytdlp(&mut log, &cancel).unwrap();
        let mut ok = 0;
        for url in ["https://vimeo.com/76979871", "https://archive.org/details/Popeye_forPresident"] {
            match probe(&ytdlp, url, false, &cancel) {
                Ok(info) => {
                    println!("OK {url}: {} · {} · {}", info.title, info.author, clock(info.seconds));
                    let dest = std::env::temp_dir().join(format!("cedro-web-{}-{ok}", std::process::id()));
                    let mut r = Request::new(vec![url.to_string()], false, dest.clone());
                    r.quality = 480;
                    match run(&r, &mut |e| if let Event::File(f) = e { println!("  file {}", f.display()) }, &cancel) {
                        Ok(files) if !files.is_empty() && files.iter().all(|f| f.metadata().map(|m| m.len() > 10_000).unwrap_or(false)) => ok += 1,
                        other => println!("  download {url}: {other:?}"),
                    }
                    let _ = std::fs::remove_dir_all(dest);
                }
                Err(e) => println!("ERRORE {url}: {e}"),
            }
        }
        assert!(ok > 0, "nessun sito scaricato");
        let err = probe(&ytdlp, "https://www.rust-lang.org/", false, &cancel).unwrap_err();
        println!("non supportato: {err}");
        assert!(err.starts_with("Questo sito non è supportato"), "{err}");
    }

    #[test]
    fn errors_in_italian() {
        assert_eq!(first_error("WARNING: x\nERROR: [youtube] abc: Private video. Sign in"), "Il video è privato");
        assert_eq!(first_error("ERROR: [youtube] abc: Video unavailable"), "Il video non è disponibile");
        assert_eq!(first_error("ERROR: [youtube] aaaaaaaaaaa: This video is unavailable"), "Il video non è disponibile");
        assert_eq!(first_error("ERROR: qualcosa di nuovo"), "qualcosa di nuovo");
        assert_eq!(first_error(""), "yt-dlp non ha dato risposta");
    }

    /// Un finto yt-dlp (script) prova l'orchestrazione: avanzamento, file prodotti, errore, annullo.
    #[cfg(unix)]
    #[test]
    fn runs_a_fake_ytdlp() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("cedro-yt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("yt-dlp");
        std::fs::write(
            &fake,
            "#!/bin/sh\nfor a in \"$@\"; do last=\"$a\"; done\ncase \"$last\" in\n*bad*) echo 'ERROR: [youtube] bad: Private video' >&2; exit 1;;\nesac\necho '[download] Downloading item 1 of 2'\necho 'CEDRO|50|100|NA|10|5'\necho 'CEDRO|100|100|NA|10|0'\ntouch \"$last-out.mp4\"\necho \"CEDROFILE|$last-out.mp4\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        let args = build_args(&req(false), None, None, false);
        let files = run_one(&fake, &args, "good", &dir, 0, 1, &mut |e| events.push(e), &cancel).unwrap();
        assert_eq!(files, vec![dir.join("good-out.mp4")]);
        assert!(dir.join("good-out.mp4").exists());
        assert!(events.iter().any(|e| matches!(e, Event::Progress { percent, .. } if *percent > 49.0 && *percent < 51.0)), "{events:?}");
        let err = run_one(&fake, &args, "bad", &dir, 0, 1, &mut |_| {}, &cancel).unwrap_err();
        assert_eq!(err, "Il video è privato");
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(run_one(&fake, &args, "good", &dir, 0, 1, &mut |_| {}, &cancel).unwrap_err(), "Annullato");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Annullare ferma subito anche un processo lento e i suoi figli.
    #[cfg(unix)]
    #[test]
    fn cancel_stops_a_slow_child_tree() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        let mut cmd = command(Path::new("sh"));
        cmd.args(["-c", "sleep 30 & sleep 30"]);
        assert_eq!(capture(&mut cmd, &cancel).unwrap_err(), "Annullato");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
