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
    let body = agent()
        .get(LATEST)
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
    let installer = json["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|asset| {
            let name = asset["name"].as_str()?;
            if !(name.starts_with("Cedro-Setup") && name.ends_with(".exe")) {
                return None;
            }
            let sha256 = asset["digest"]
                .as_str()
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(str::to_string)
                .or_else(|| sha_in_notes(notes, name));
            Some(Asset {
                name: name.to_string(),
                url: asset["browser_download_url"].as_str()?.to_string(),
                sha256,
            })
        })
    });
    Ok(Release { version, page, installer })
}

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
    let response = agent().get(&asset.url).call().map_err(|e| format!("download non riuscito: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("download interrotto: {e}"))?;
    let expected = asset.sha256.as_deref().ok_or("la release non indica lo SHA-256 dell'installatore")?;
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("lo SHA-256 dell'installatore non corrisponde: file scartato".into());
    }
    let path = std::env::temp_dir().join(&asset.name);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn skip_file() -> PathBuf {
    crate::config_dir().join("update-skip.txt")
}

/// Vero se l'utente ha già detto no a questa versione.
pub fn skipped(version: &str) -> bool {
    std::fs::read_to_string(skip_file()).is_ok_and(|text| text.trim() == version)
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
        let release = parse_release(body).unwrap();
        assert_eq!(release.version, "0.8.0");
        let asset = release.installer.unwrap();
        assert_eq!(asset.url, "https://x/Cedro-Setup-0.8.0.exe");
        assert_eq!(asset.sha256.as_deref(), Some("410cf4149103e3c6bf74a241ec6ea87e9cdff471f02d8242f858c73f2b0dced0"));
    }

    #[test]
    fn prefers_asset_digest() {
        let body = r#"{"tag_name": "0.9.0", "assets": [
            {"name": "Cedro-Setup-0.9.0.exe", "browser_download_url": "https://x/a.exe", "digest": "sha256:abc"}
        ]}"#;
        let asset = parse_release(body).unwrap().installer.unwrap();
        assert_eq!(asset.sha256.as_deref(), Some("abc"));
    }
}
