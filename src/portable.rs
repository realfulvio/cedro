//! Versione portable (Windows x64): lo zip si apre dove si vuole e Cedro parte senza installazione.
//! È «portable» quando accanto a cedro.exe c'è il file `cedro.portable`: allora impostazioni, registro,
//! modelli e programmi scaricati (whisper.cpp, ffmpeg, yt-dlp…) stanno nella cartella `Dati` accanto all'exe,
//! e l'aggiornamento sostituisce i file della cartella invece di lanciare l'installatore.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Il file che rende «portable» la cartella dove sta cedro.exe.
pub const MARKER: &str = "cedro.portable";
/// La cartella con tutti i dati, accanto a cedro.exe.
pub const DATA: &str = "Dati";

/// La cartella di cedro.exe, se Cedro gira in modalità portable.
pub fn root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        dir.join(MARKER).is_file().then_some(dir)
    })
    .as_deref()
}

pub fn is_portable() -> bool {
    root().is_some()
}

/// `Dati` accanto a cedro.exe (solo in modalità portable).
pub fn data_root() -> Option<PathBuf> {
    root().map(|r| r.join(DATA))
}

/// Il nome dello zip portable di una release: `Cedro-Portable-<versione>-x64.zip`.
pub fn is_package_name(name: &str) -> bool {
    name.strip_prefix("Cedro-Portable-").is_some_and(|rest| rest.ends_with("-x64.zip") && rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// Aggiorna la cartella portable con lo zip della release nuova (già scaricato e controllato).
/// `cedro.exe` in uso non si può sovrascrivere ma si può rinominare: il vecchio diventa `cedro.old.exe`
/// (tolto al prossimo avvio) e il nuovo prende il suo posto. `Dati` e `cedro.portable` non si toccano.
pub fn apply_zip(zip_path: &Path, dir: &Path) -> Result<(), String> {
    use std::io::Read;
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("lo zip non si apre: {e}"))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Dentro lo zip c'è una cartella sola («Cedro-Portable»): si prende il nome del file, senza percorsi.
        let Some(name) = entry.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())) else { continue };
        if name.eq_ignore_ascii_case(MARKER) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.by_ref().take(512 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        files.push((name, bytes));
    }
    let exe_bytes = files.iter().find(|(n, _)| n.eq_ignore_ascii_case("cedro.exe")).map(|(_, b)| b.clone()).ok_or("lo zip non ha cedro.exe")?;
    if exe_bytes.len() < 1_000_000 || &exe_bytes[..2] != b"MZ" {
        return Err("il cedro.exe dello zip non è valido".into());
    }
    let exe = dir.join("cedro.exe");
    let old = dir.join("cedro.old.exe");
    let new = dir.join("cedro.new.exe");
    std::fs::write(&new, &exe_bytes).map_err(|e| format!("cartella non scrivibile: {e}"))?;
    let _ = std::fs::remove_file(&old);
    if exe.exists() {
        if let Err(e) = std::fs::rename(&exe, &old) {
            let _ = std::fs::remove_file(&new);
            return Err(format!("cedro.exe non si sostituisce: {e}"));
        }
    }
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("cedro.exe nuovo non messo a posto: {e}"));
    }
    for (name, bytes) in files.iter().filter(|(n, _)| !n.eq_ignore_ascii_case("cedro.exe")) {
        let _ = std::fs::write(dir.join(name), bytes);
    }
    Ok(())
}

/// Al primo avvio dopo un aggiornamento toglie il cedro.exe vecchio.
pub fn cleanup() {
    if let Some(root) = root() {
        let _ = std::fs::remove_file(root.join("cedro.old.exe"));
        let _ = std::fs::remove_file(root.join("cedro.new.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn package_name() {
        assert!(is_package_name("Cedro-Portable-0.40.0-x64.zip"));
        assert!(!is_package_name("Cedro-Portable-0.40.0-arm64.zip"));
        assert!(!is_package_name("Cedro-Setup-0.40.0.exe"));
    }

    #[test]
    fn apply_zip_replaces_exe_and_keeps_data() {
        let tmp = std::env::temp_dir().join(format!("cedro-portable-prova-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("Dati")).unwrap();
        std::fs::write(tmp.join("Dati").join("config.txt"), "x").unwrap();
        std::fs::write(tmp.join("cedro.exe"), b"vecchio").unwrap();
        std::fs::write(tmp.join(MARKER), b"").unwrap();
        let zip_path = tmp.join("nuovo.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opt = zip::write::SimpleFileOptions::default();
            let mut exe = b"MZ".to_vec();
            exe.resize(1_500_000, 7);
            z.start_file("Cedro-Portable/cedro.exe", opt).unwrap();
            z.write_all(&exe).unwrap();
            z.start_file("Cedro-Portable/LICENSE", opt).unwrap();
            z.write_all(b"licenza").unwrap();
            z.start_file("Cedro-Portable/cedro.portable", opt).unwrap();
            z.write_all(b"").unwrap();
            z.finish().unwrap();
        }
        apply_zip(&zip_path, &tmp).unwrap();
        assert_eq!(std::fs::metadata(tmp.join("cedro.exe")).unwrap().len(), 1_500_000);
        assert_eq!(std::fs::read(tmp.join("cedro.old.exe")).unwrap(), b"vecchio");
        assert_eq!(std::fs::read(tmp.join("LICENSE")).unwrap(), b"licenza");
        assert_eq!(std::fs::read(tmp.join("Dati").join("config.txt")).unwrap(), b"x");
        assert!(!tmp.join("cedro.new.exe").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
