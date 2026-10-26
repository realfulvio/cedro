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
    if cfg!(all(windows, target_arch = "aarch64")) {
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
    c
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
    let own = tools_dir().join(exe("ffmpeg"));
    if ffmpeg_ready() {
        return Ok(ffmpeg_path().unwrap_or(own));
    }
    emit(Event::Log("ffmpeg non c'è ancora: lo scarico (una volta sola, circa 140 MB)".into()));
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
    emit(Event::Log("ffmpeg scaricato".into()));
    Ok(own)
}

/// Estrae da uno zip il file che finisce con `suffix` e lo scrive in `target`.
fn extract_zip_entry(archive: &Path, suffix: &str, target: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let index = (0..zip.len())
        .find(|&i| zip.by_index(i).map(|f| f.name().ends_with(suffix)).unwrap_or(false))
        .ok_or_else(|| format!("{suffix} non è nell'archivio"))?;
    let mut entry = zip.by_index(index).map_err(|_| "archivio rovinato".to_string())?;
    let part = target.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    drop(out);
    std::fs::rename(part, target).map_err(|e| e.to_string())
}

/// Scarica deno se manca.
pub fn ensure_deno(emit: &mut dyn FnMut(Event), cancel: &AtomicBool) -> Result<PathBuf, String> {
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

fn stamp_path() -> PathBuf {
    tools_dir().join("yt-dlp.aggiornato")
}

/// Cerca una versione più nuova di yt-dlp, al massimo ogni `UPDATE_EVERY_HOURS` ore.
pub fn update_ytdlp_if_due(ytdlp: &Path, emit: &mut dyn FnMut(Event)) {
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
    match command(ytdlp).arg("-U").stdin(Stdio::null()).output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let line = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            if out.status.success() {
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
    let out = command(path).arg(if name == "ffmpeg" { "-version" } else { "--version" }).stdin(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?;
    let mut words = first.split_whitespace();
    let version = if name == "ffmpeg" { words.nth(2)? } else { words.nth(1)? };
    out.status.success().then(|| version.to_string())
}

/// La versione del programma scaricato, per la scheda Informazioni. Ricordata in un file accanto al programma:
/// se manca (o il programma è stato aggiornato) la cerca in disparte e la mostra la volta dopo.
pub fn installed_version(name: &str) -> Option<String> {
    static BUSY: AtomicBool = AtomicBool::new(false);
    let path = tool_file(name)?;
    let cache = tools_dir().join(format!("{name}.versione"));
    let fresh = match (std::fs::metadata(&cache).and_then(|m| m.modified()), std::fs::metadata(&path).and_then(|m| m.modified())) {
        (Ok(c), Ok(p)) => c >= p,
        _ => false,
    };
    if fresh {
        if let Ok(text) = std::fs::read_to_string(&cache) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    if !BUSY.swap(true, Ordering::SeqCst) {
        let name = name.to_string();
        std::thread::spawn(move || {
            if let Some(v) = tool_version_now(&name, &path) {
                let _ = std::fs::write(tools_dir().join(format!("{name}.versione")), v);
            }
            BUSY.store(false, Ordering::SeqCst);
        });
    }
    None
}

pub fn ytdlp_version(ytdlp: &Path) -> Option<String> {
    let out = command(ytdlp).arg("--version").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
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
    Ok(Info { title, author, seconds, entries })
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
        let _ = Command::new("kill").args(["-KILL", "--", &format!("-{}", child.id())]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Esegue `cmd` e ne raccoglie le uscite, fermandolo se `cancel` si accende.
fn capture(cmd: &mut Command, cancel: &AtomicBool) -> Result<(String, String, bool), String> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
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
        if cancel.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            let _ = (t_out.join(), t_err.join());
            return Err("Annullato".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
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
    } else if low.contains("unable to download") || low.contains("getaddrinfo") || low.contains("timed out") {
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
        a.push(format!("deno:{}", d.display()));
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
        let sel = format!("bv*{h}+ba/b{h}");
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

/// «[download] Downloading item 3 of 18» → (3, 18).
pub fn parse_item(line: &str) -> Option<(usize, usize)> {
    let rest = line.split("Downloading item ").nth(1).or_else(|| line.split("Downloading video ").nth(1))?;
    let mut it = rest.split_whitespace();
    let n = it.next()?.parse().ok()?;
    if it.next()? != "of" {
        return None;
    }
    Some((n, it.next()?.parse().ok()?))
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
    let ffmpeg = ensure_ffmpeg(emit, cancel)?;
    let deno = ensure_deno(emit, cancel)?;
    update_ytdlp_if_due(&ytdlp, emit);
    if let Some(v) = ytdlp_version(&ytdlp) {
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
                let args = build_args(r, Some(&ffmpeg), Some(&deno), info.entries.is_some());
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
                    let percent = ((item - 1) as f32 * 100.0 + p) / items as f32;
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
    let status = child.wait().map_err(|e| e.to_string())?;
    if files.is_empty() && !status.success() {
        return Err(first_error(&errors));
    }
    Ok(files)
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
        assert!(j.contains("-f bv*[height<=1080]+ba/b[height<=1080]"), "{j}");
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
        assert!(j.contains("-f bv*+ba/b") && !j.contains("height"));
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
