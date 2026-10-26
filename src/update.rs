//! Controllo aggiornamenti: confronta la versione del programma con l'ultima release su GitHub.
//! Su Windows scarica l'installatore della release e ne controlla lo SHA-256.

use std::io::Read;
use std::path::PathBuf;

const LATEST: &str = "https://api.github.com/repos/realfulvio/cedro/releases/latest";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub installer: Option<Asset>,
    /// Il pacchetto per Linux (`cedro-<versione>-linux-x64.tar.gz`).
    pub package: Option<Asset>,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(&format!("cedro/{CURRENT}"))
        .build()
}

/// L'ultima release pubblicata, senza bozze né pre-release.
pub fn latest() -> Result<Release, String> {
    #[cfg(debug_assertions)]
    let latest = std::env::var("CEDRO_UPDATE_URL").unwrap_or_else(|_| LATEST.to_string());
    #[cfg(not(debug_assertions))]
    let latest = LATEST.to_string();
    let body = agent()
        .get(&latest)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub non risponde: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse_release(&body)
}

/// La release, solo se è più nuova di questa versione.
pub fn check() -> Result<Option<Release>, String> {
    let release = latest()?;
    Ok(newer(CURRENT, &release.version).then_some(release))
}

fn parse_release(body: &str) -> Result<Release, String> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let tag = json["tag_name"].as_str().ok_or("release senza tag")?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    let page = json["html_url"].as_str().unwrap_or("https://github.com/realfulvio/cedro/releases/latest").to_string();
    let notes = json["body"].as_str().unwrap_or("");
    let find = |wanted: &dyn Fn(&str) -> bool| {
        json["assets"].as_array().and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset["name"].as_str()?;
                if !wanted(name) {
                    return None;
                }
                let sha256 = asset["digest"]
                    .as_str()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(str::to_string)
                    .or_else(|| sha_in_notes(notes, name));
                Some(Asset { name: name.to_string(), url: asset["browser_download_url"].as_str()?.to_string(), sha256 })
            })
        })
    };
    let installer = find(&|name| is_installer_for_this_arch(name));
    let suffix = format!("-linux-{}.tar.gz", package_arch());
    let package = find(&|name| name.starts_with("cedro-") && name.ends_with(&suffix));
    Ok(Release { version, page, installer, package })
}

/// L'installatore x64 è `Cedro-Setup-<v>.exe`, quello per Windows on ARM `Cedro-ARM64-Setup-<v>.exe`.
/// Il nome ARM non comincia con «Cedro-Setup» apposta: fino alla 0.17 Cedro prendeva il primo file
/// «Cedro-Setup….exe» della release, e deve continuare a trovare quello x64.
/// Su ARM vale anche il nome della 0.18.0 (`Cedro-Setup-0.18.0-arm64.exe`).
fn is_installer_for_this_arch(name: &str) -> bool {
    if !name.ends_with(".exe") {
        return false;
    }
    let arm = name.starts_with(ARM_INSTALLER) || (name.starts_with("Cedro-Setup") && name.ends_with("-arm64.exe"));
    let x64 = name.starts_with("Cedro-Setup") && !name.ends_with("-arm64.exe");
    if cfg!(target_arch = "aarch64") { arm } else { x64 }
}

const ARM_INSTALLER: &str = "Cedro-ARM64-Setup";

/// Le note delle release scrivono «SHA-256 di <file>: `<hex>`».
fn sha_in_notes(notes: &str, name: &str) -> Option<String> {
    let line = notes.lines().find(|line| line.contains("SHA-256") && line.contains(name))?;
    let hex = line.split('`').nth(1)?.trim();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_lowercase())
}

/// Vero se `latest` è una versione più alta di `current` (0.7.1 < 0.7.10 < 0.8.0).
pub fn newer(current: &str, latest: &str) -> bool {
    match (parts(current), parts(latest)) {
        (Some(a), Some(b)) => b > a,
        _ => false,
    }
}

fn parts(version: &str) -> Option<Vec<u64>> {
    let core = version.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut out: Vec<u64> = core.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    while out.len() < 3 {
        out.push(0);
    }
    Some(out)
}

/// Scarica l'installatore nella cartella temporanea e ne verifica lo SHA-256.
pub fn download(asset: &Asset) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    // Senza codice di controllo non si scarica nemmeno.
    let expected = asset.sha256.as_deref().ok_or("la release non indica lo SHA-256 dell'installatore")?;
    let response = agent().get(&asset.url).call().map_err(|e| format!("download non riuscito: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("download interrotto: {e}"))?;
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("lo SHA-256 dell'installatore non corrisponde: file scartato".into());
    }
    // Solo il nome del file: un nome con delle cartelle dentro non esce dalla cartella temporanea.
    let name = std::path::Path::new(&asset.name).file_name().ok_or("nome dell'installatore non valido")?;
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Come il pacchetto per Linux chiama questa architettura (`cedro-<versione>-linux-<arch>.tar.gz`).
pub fn package_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// Dove `install.sh` mette Cedro: sempre nella cartella dell'utente, da dove Cedro parte anche
/// se prima girava da `target/`, da `/usr/local` o da una cartella a scelta.
#[cfg(unix)]
pub fn installed_binary() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).ok_or("la cartella dell'utente (HOME) non è nota")?;
    Ok(PathBuf::from(home).join(".local/opt/cedro/cedro"))
}

/// Una cartella nuova, solo dell'utente (0700), per scaricare e aprire il pacchetto: nessun altro processo
/// può sostituire l'archivio o `install.sh` tra il controllo dello SHA-256 e l'installazione.
/// Sta in `XDG_RUNTIME_DIR` (già privata) o, se manca, nella cartella di Cedro; mai in `/tmp` con un nome prevedibile.
#[cfg(unix)]
fn private_dir() -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()) {
        Some(dir) => dir,
        None => {
            let dir = crate::config_dir().join("tmp");
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| format!("cartella provvisoria non creata: {e}"))?;
            dir
        }
    };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    for _ in 0..8 {
        let name = format!("cedro-aggiornamento-{}-{nanos:x}-{}", std::process::id(), CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let dir = base.join(name);
        // `create` non riesce se esiste già (anche come link): una cartella preparata da altri non viene mai usata.
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cartella provvisoria non creata: {e}")),
        }
    }
    Err("cartella provvisoria non creata".to_string())
}

/// Scarica il pacchetto in una cartella privata, ne controlla lo SHA-256 sulla copia che poi installa.
#[cfg(unix)]
fn download_private(asset: &Asset, dir: &std::path::Path) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let response = agent().get(&asset.url).call().map_err(|e| format!("download non riuscito: {e}"))?;
    let mut bytes = Vec::new();
    response.into_reader().take(512 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| format!("download interrotto: {e}"))?;
    let expected = asset.sha256.as_deref().ok_or("la release non indica lo SHA-256 del pacchetto")?;
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("lo SHA-256 del pacchetto non corrisponde: file scartato".into());
    }
    let name = std::path::Path::new(&asset.name).file_name().ok_or("nome del pacchetto non valido")?;
    let path = dir.join(name);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Su Linux: scarica il pacchetto (con lo SHA-256 controllato) e lo installa. Ritorna il Cedro nuovo.
/// Lavora tutto in una cartella privata; da chiamare fuori dal thread delle finestre (scarica e copia file).
#[cfg(unix)]
pub fn install_package(asset: &Asset) -> Result<PathBuf, String> {
    let dir = private_dir()?;
    let result = download_private(asset, &dir).and_then(|archive| install_archive_in(&archive, &dir));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Apre un pacchetto già scaricato e lancia il suo `install.sh --no-start`. La copia del pacchetto passa
/// prima in una cartella privata, così quel che si apre è quel che si è ricevuto.
#[cfg(unix)]
pub fn install_archive(archive: &std::path::Path) -> Result<PathBuf, String> {
    let dir = private_dir()?;
    let result = (|| {
        let copy = dir.join("pacchetto.tar.gz");
        std::fs::copy(archive, &copy).map_err(|e| e.to_string())?;
        install_archive_in(&copy, &dir)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Apre `archive` (dentro `dir`, privata) in `dir/aperto` e lancia il suo `install.sh --no-start`: copia il
/// programma in `~/.local/opt/cedro` e lascia a Cedro, al primo avvio, il compito di riscrivere menu, icona e avvio automatico.
#[cfg(unix)]
fn install_archive_in(archive: &std::path::Path, dir: &std::path::Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let target = installed_binary()?;
    let open = dir.join("aperto");
    std::fs::DirBuilder::new().mode(0o700).create(&open).map_err(|e| e.to_string())?;
    let unpacked = std::process::Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&open).status().map_err(|e| format!("tar non parte: {e}"))?;
    if !unpacked.success() {
        return Err("il pacchetto non si apre".to_string());
    }
    let script = std::fs::read_dir(&open)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|d| d.path().join("install.sh"))
        .find(|p| p.is_file())
        .ok_or("il pacchetto non ha install.sh")?;
    let done = std::process::Command::new("bash").arg(&script).arg("--no-start").output().map_err(|e| e.to_string())?;
    if !done.status.success() {
        return Err(format!("install.sh non è riuscito: {}", String::from_utf8_lossy(&done.stderr).trim()));
    }
    if target.is_file() { Ok(target) } else { Err("il programma nuovo non è al suo posto".to_string()) }
}

fn skip_file() -> PathBuf {
    crate::config_dir().join("update-skip.txt")
}

/// Vero se l'utente ha già detto no a questa versione.
pub fn skipped(version: &str) -> bool {
    std::fs::read_to_string(skip_file()).is_ok_and(|text| text.trim() == version)
}

/// La versione a cui l'utente ha detto di no, se c'è.
pub fn skipped_version() -> Option<String> {
    std::fs::read_to_string(skip_file()).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Torna a proporre anche la versione rifiutata.
pub fn unskip() {
    let _ = std::fs::remove_file(skip_file());
}

/// Non chiedere più per questa versione, all'avvio.
pub fn skip(version: &str) {
    let _ = std::fs::create_dir_all(crate::config_dir());
    let _ = std::fs::write(skip_file(), version);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.7.1", "0.7.2"));
        assert!(newer("0.7.1", "v0.8.0"));
        assert!(newer("0.7.9", "0.7.10"));
        assert!(newer("0.7", "0.7.1"));
        assert!(!newer("0.7.1", "0.7.1"));
        assert!(!newer("0.7.1", "v0.7.0"));
        assert!(!newer("1.0.0", "0.9.9"));
        assert!(!newer("0.7.1", "nonsense"));
    }

    /// Su ARM64 l'installatore si chiama «Cedro-ARM64-Setup-…»: i file di prova lo prendono come quelli veri.
    fn for_this_arch(text: &str) -> String {
        if cfg!(target_arch = "aarch64") {
            text.replace("Cedro-Setup-", "Cedro-ARM64-Setup-")
        } else {
            text.to_string()
        }
    }

    #[test]
    fn picks_the_installer_of_this_architecture() {
        // GitHub elenca i file in ordine alfabetico: quello ARM viene prima.
        for arm_name in ["Cedro-ARM64-Setup-0.18.1.exe", "Cedro-Setup-0.18.0-arm64.exe"] {
            let body = format!(
                r#"{{"tag_name": "0.18.1", "assets": [
                {{"name": "{arm_name}", "browser_download_url": "https://x/arm.exe", "digest": "sha256:aa"}},
                {{"name": "Cedro-Setup-0.18.1.exe", "browser_download_url": "https://x/x64.exe", "digest": "sha256:bb"}}
            ]}}"#
            );
            let asset = parse_release(&body).unwrap().installer.unwrap();
            let (url, sha) = if cfg!(target_arch = "aarch64") { ("https://x/arm.exe", "aa") } else { ("https://x/x64.exe", "bb") };
            assert_eq!((asset.url.as_str(), asset.sha256.as_deref()), (url, Some(sha)), "{arm_name}");
        }
    }

    /// Una release completa come le vere (ordine alfabetico di GitHub): ogni sistema prende il suo file.
    #[test]
    fn every_system_finds_its_file_in_a_full_release() {
        let names = [
            "Cedro-ARM64-Setup-0.18.1.exe",
            "Cedro-Setup-0.18.1.exe",
            "cedro-0.18.1-linux-x64.tar.gz",
            "cedro-trascrivi-0.18.1-linux-x64-avx2",
            "cedro-trascrivi-0.18.1-windows-arm64-i8mm.exe",
            "cedro-trascrivi-0.18.1-windows-x64-avx2.exe",
        ];
        let assets: Vec<String> = names
            .iter()
            .map(|n| format!(r#"{{"name": "{n}", "browser_download_url": "https://x/{n}", "digest": "sha256:00"}}"#))
            .collect();
        let body = format!(r#"{{"tag_name": "v0.18.1", "assets": [{}]}}"#, assets.join(","));
        let release = parse_release(&body).unwrap();
        let installer = release.installer.unwrap().name;
        if cfg!(target_arch = "aarch64") {
            assert_eq!(installer, "Cedro-ARM64-Setup-0.18.1.exe");
        } else {
            assert_eq!(installer, "Cedro-Setup-0.18.1.exe");
        }
        assert_eq!(release.package.unwrap().name, "cedro-0.18.1-linux-x64.tar.gz");
    }

    /// Fino alla 0.17 Cedro prendeva il primo «Cedro-Setup….exe»: l'installatore ARM non deve somigliargli.
    #[test]
    fn old_versions_do_not_pick_the_arm_installer() {
        let old_rule = |name: &str| name.starts_with("Cedro-Setup") && name.ends_with(".exe");
        assert!(!old_rule("Cedro-ARM64-Setup-0.18.1.exe"));
        assert!(old_rule("Cedro-Setup-0.18.1.exe"));
    }

    #[test]
    fn reads_release() {
        let body = r#"{
            "tag_name": "v0.8.0",
            "html_url": "https://github.com/realfulvio/cedro/releases/tag/v0.8.0",
            "body": "Note.\n\nSHA-256 di Cedro-Setup-0.8.0.exe: `410CF4149103e3c6bf74a241ec6ea87e9cdff471f02d8242f858c73f2b0dced0`",
            "assets": [
                {"name": "sorgenti.zip", "browser_download_url": "https://x/sorgenti.zip"},
                {"name": "Cedro-Setup-0.8.0.exe", "browser_download_url": "https://x/Cedro-Setup-0.8.0.exe"}
            ]
        }"#;
        let release = parse_release(&for_this_arch(body)).unwrap();
        assert_eq!(release.version, "0.8.0");
        let asset = release.installer.unwrap();
        assert_eq!(asset.url, for_this_arch("https://x/Cedro-Setup-0.8.0.exe"));
        assert_eq!(asset.sha256.as_deref(), Some("410cf4149103e3c6bf74a241ec6ea87e9cdff471f02d8242f858c73f2b0dced0"));
    }

    #[test]
    fn finds_the_linux_package() {
        let body = r#"{"tag_name": "0.9.0", "assets": [
            {"name": "Cedro-Setup-0.9.0.exe", "browser_download_url": "https://x/a.exe", "digest": "sha256:abc"},
            {"name": "cedro-trascrivi-0.9.0-linux-x64-base", "browser_download_url": "https://x/t"},
            {"name": "cedro-0.9.0-linux-x64.tar.gz", "browser_download_url": "https://x/c.tar.gz", "digest": "sha256:def"}]}"#;
        let release = parse_release(&for_this_arch(body)).unwrap();
        let package = release.package.unwrap();
        assert_eq!(package.url, "https://x/c.tar.gz");
        assert_eq!(package.sha256.as_deref(), Some("def"));
        assert_eq!(release.installer.unwrap().sha256.as_deref(), Some("abc"));
    }

    /// Un server HTTP da prova: risponde con `body` alla prima richiesta e basta.
    fn serve_once(body: Vec<u8>) -> String {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 1024];
                let _ = std::io::Read::read(&mut stream, &mut request);
                let _ = write!(stream, "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

", body.len());
                let _ = stream.write_all(&body);
            }
        });
        url
    }

    fn sha_of(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn download_checks_the_sha256() {
        let body = b"pacchetto finto".to_vec();
        let good = Asset { name: "cedro-prova-ok.bin".into(), url: serve_once(body.clone()), sha256: Some(sha_of(&body)) };
        let path = download(&good).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        let _ = std::fs::remove_file(path);
        let bad = Asset { name: "cedro-prova-ko.bin".into(), url: serve_once(body), sha256: Some("0".repeat(64)) };
        assert!(download(&bad).unwrap_err().contains("non corrisponde"));
        assert!(!std::env::temp_dir().join("cedro-prova-ko.bin").exists());
        let none = Asset { name: "cedro-prova-no.bin".into(), url: "http://127.0.0.1:9/x".into(), sha256: None };
        assert!(download(&none).unwrap_err().contains("SHA-256"));
        // Un nome con delle cartelle resta dentro la cartella temporanea.
        let body = b"x".to_vec();
        let sneaky = Asset { name: "../cedro-prova-fuori.bin".into(), url: serve_once(body.clone()), sha256: Some(sha_of(&body)) };
        let path = download(&sneaky).unwrap();
        assert_eq!(path, std::env::temp_dir().join("cedro-prova-fuori.bin"));
        let _ = std::fs::remove_file(path);
    }

    /// La cartella dove si apre il pacchetto è nuova, solo dell'utente e mai la stessa due volte.
    #[cfg(unix)]
    #[test]
    fn the_update_folder_is_private_and_unique() {
        use std::os::unix::fs::PermissionsExt;
        let a = private_dir().unwrap();
        let b = private_dir().unwrap();
        assert_ne!(a, b);
        for dir in [&a, &b] {
            assert_eq!(std::fs::metadata(dir).unwrap().permissions().mode() & 0o777, 0o700, "{}", dir.display());
            assert!(dir.starts_with(std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(|| crate::config_dir().join("tmp"))));
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[cfg(unix)]
    #[test]
    fn installs_a_package_with_its_install_script() {
        let work = std::env::temp_dir().join(format!("cedro-test-pacchetto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let pkg = work.join("cedro-9.9.9-linux-x64");
        std::fs::create_dir_all(&pkg).unwrap();
        let marker = work.join("fatto.txt");
        std::fs::write(pkg.join("install.sh"), format!("#!/bin/sh
echo \"$1\" > {}
mkdir -p \"$HOME/.local/opt/cedro\" && : > \"$HOME/.local/opt/cedro/cedro\"
", marker.display())).unwrap();
        let archive = work.join("pacchetto.tar.gz");
        let made = std::process::Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&work).arg("cedro-9.9.9-linux-x64").status().unwrap();
        assert!(made.success());
        let bytes = std::fs::read(&archive).unwrap();
        let asset = Asset { name: format!("cedro-prova-{}-linux-x64.tar.gz", std::process::id()), url: serve_once(bytes.clone()), sha256: Some(sha_of(&bytes)) };
        with_fake_home(&work.join("home"), || install_package(&asset)).unwrap();
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "--no-start");
        // Un pacchetto con lo SHA-256 sbagliato non viene nemmeno aperto.
        std::fs::remove_file(&marker).unwrap();
        let bad = Asset { name: format!("cedro-prova-ko-{}-linux-x64.tar.gz", std::process::id()), url: serve_once(bytes), sha256: Some("1".repeat(64)) };
        assert!(install_package(&bad).is_err());
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(&work);
    }

    /// Esegue `f` con un HOME finto: i test dell'installazione non devono toccare quello di chi li lancia.
    #[cfg(unix)]
    fn with_fake_home<T>(home: &std::path::Path, f: impl FnOnce() -> T) -> T {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(home).unwrap();
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        let out = f();
        match previous {
            Some(p) => std::env::set_var("HOME", p),
            None => std::env::remove_var("HOME"),
        }
        out
    }

    #[test]
    fn package_follows_the_architecture() {
        let body = format!(r#"{{"tag_name": "0.9.0", "assets": [
            {{"name": "cedro-0.9.0-linux-other.tar.gz", "browser_download_url": "https://x/o"}},
            {{"name": "cedro-0.9.0-linux-{}.tar.gz", "browser_download_url": "https://x/mine"}}]}}"#, package_arch());
        assert_eq!(parse_release(&body).unwrap().package.unwrap().url, "https://x/mine");
        let none = r#"{"tag_name": "0.9.0", "assets": [{"name": "Cedro-Setup-0.9.0.exe", "browser_download_url": "https://x/a"}]}"#;
        assert!(parse_release(none).unwrap().package.is_none());
    }

    /// Il pacchetto vero (il suo install.sh) messo in una cartella dell'utente finta.
    #[cfg(unix)]
    #[test]
    fn installs_a_package_into_the_user_folder() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("cedro-upd-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pack = root.join("pack/cedro-9.9.9-linux-x64");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("cedro"), "#!/bin/sh\necho nuovo\n").unwrap();
        std::fs::set_permissions(pack.join("cedro"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(pack.join("install.sh"), include_str!("../installer/install-linux.sh")).unwrap();
        let archive = root.join("cedro-9.9.9-linux-x64.tar.gz");
        let made = std::process::Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(root.join("pack")).arg("cedro-9.9.9-linux-x64").status().unwrap();
        assert!(made.success());
        let home = root.join("home");
        let result = with_fake_home(&home, || install_archive(&archive));
        let installed = result.unwrap();
        assert_eq!(installed, home.join(".local/opt/cedro/cedro"));
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "#!/bin/sh\necho nuovo\n");
        assert!(home.join(".local/bin/cedro").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prefers_asset_digest() {
        let body = r#"{"tag_name": "0.9.0", "assets": [
            {"name": "Cedro-Setup-0.9.0.exe", "browser_download_url": "https://x/a.exe", "digest": "sha256:abc"}
        ]}"#;
        let asset = parse_release(&for_this_arch(body)).unwrap().installer.unwrap();
        assert_eq!(asset.sha256.as_deref(), Some("abc"));
    }
}
