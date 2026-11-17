//! Primitive di I/O limitato, testabili anche senza Android.
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Il tipo MIME di un file dalla sua estensione (per «Apri» e «Condividi»).
pub(crate) fn mime_da_percorso(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "html" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/x-wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "doc" => "application/msword",
        "dcm" => "application/dicom",
        "heic" | "heif" => "image/heic",
        "p7m" => "application/pkcs7-mime",
        "p7s" => "application/pkcs7-signature",
        "tsd" => "application/timestamped-data",
        "tsr" => "application/timestamp-reply",
        "zip" => "application/zip",
        "opus" => "audio/ogg",
        "aif" | "aiff" => "audio/aiff",
        "avif" => "image/avif",
        "jxl" => "image/jxl",
        "ico" => "image/x-icon",
        "tga" => "image/x-tga",
        "pnm" | "ppm" | "pgm" | "pbm" => "image/x-portable-anymap",
        "exr" => "image/x-exr",
        "srt" => "application/x-subrip",
        "vtt" => "text/vtt",
        _ => "application/octet-stream",
    }
}

/// Originale delle firme staccate: conserva il grant SAF e un cursore indipendente.
pub(crate) fn leggi_originale(path: &Path, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Acquire) { return Err("Annullato".into()); }
    let mut file=cedro::input_file::File::open(path).map_err(|e|e.to_string())?;
    let mut bytes=Vec::new();
    let mut chunk=[0u8;64*1024];
    loop {
        if cancel.load(Ordering::Acquire) { return Err("Annullato".into()); }
        let n=file.read(&mut chunk).map_err(|e|e.to_string())?;
        if n==0 { break; }
        bytes.try_reserve(n).map_err(|_|"Memoria insufficiente per verificare il file originale".to_string())?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(bytes)
}

pub fn nome_sicuro(nome: &str) -> String {
    let s: String = nome.chars().map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':') { '_' } else { c }).take(180).collect();
    if s.trim_matches('.').trim().is_empty() { "file".into() } else { s }
}

pub fn nuova_cartella(base: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let p = base.join(format!("{stamp:x}-{n:x}"));
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// La destinazione non compare finché il file non è completo e sincronizzato.
pub fn copia(mut src: impl Read, dest: &Path, budget: u64, cancel: &AtomicBool, progresso: impl Fn(u64)) -> io::Result<u64> {
    let temp = dest.with_file_name(format!(".{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    let r = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        let mut buf = [0u8; 64 * 1024];
        let mut n = 0u64;
        loop {
            if cancel.load(Ordering::Acquire) { return Err(io::Error::new(io::ErrorKind::Interrupted, "Annullato")); }
            let got = src.read(&mut buf)?;
            if got == 0 { break; }
            n = n.checked_add(got as u64).ok_or_else(|| io::Error::other("File troppo grande"))?;
            if n > budget { return Err(io::Error::other("Spazio insufficiente per importare il file")); }
            f.write_all(&buf[..got])?;
            progresso(n);
        }
        f.sync_all()?;
        if cancel.load(Ordering::Acquire) { return Err(io::Error::new(io::ErrorKind::Interrupted, "Annullato")); }
        std::fs::rename(&temp, dest)?;
        Ok(n)
    })();
    if r.is_err() { let _ = std::fs::remove_file(temp); }
    r
}

pub fn atomico(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("Cartella assente"))?;
    let dir = nuova_cartella(parent)?;
    let hidden=parent.join(format!(".stato-{}",dir.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::rename(&dir,&hidden)?;
    let dir=hidden;
    let tmp = dir.join("stato");
    let r = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    let _ = std::fs::remove_dir_all(dir);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_video_mime_is_used_for_publication_and_intents() {
        assert_eq!(mime_da_percorso(Path::new("film.webm")),"video/webm");
        assert_eq!(mime_da_percorso(Path::new("film.MKV")),"video/x-matroska");
        assert_eq!(mime_da_percorso(Path::new("film.mp4")),"video/mp4");
        assert_eq!(mime_da_percorso(Path::new("file.unknown")),"application/octet-stream");
    }

    #[cfg(unix)]
    #[test]
    fn detached_original_uses_registered_grant_and_independent_cursor() {
        use std::io::{Seek,SeekFrom};
        use std::os::fd::AsRawFd;
        use std::sync::Arc;
        let dir=nuova_cartella(&std::env::temp_dir()).unwrap();
        let path=dir.join("originale");
        // Il pathname del grant non è riapribile: il registro conserva il vero descriptor.
        let data=(0..32*1024*1024+17).map(|i|(i%251) as u8).collect::<Vec<_>>();
        std::fs::write(&path,&data).unwrap();
        let mut source=std::fs::File::open(&path).unwrap();source.seek(SeekFrom::Start(137)).unwrap();
        let source=Arc::new(source);
        let alias=dir.join("originale.pdf");
        let unavailable_fd=-1;
        std::os::unix::fs::symlink(format!("/proc/self/fd/{unavailable_fd}"),&alias).unwrap();
        cedro::input_file::register(&alias,unavailable_fd,source.clone());
        assert!(std::fs::File::open(&alias).is_err());
        let mut other=cedro::input_file::File::open(&alias).unwrap();
        assert_eq!(other.as_raw_fd(),source.as_raw_fd());
        other.seek(SeekFrom::Start(29)).unwrap();
        assert_eq!(leggi_originale(&alias,&AtomicBool::new(false)).unwrap(),data);
        assert_eq!(other.stream_position().unwrap(),29);
        assert_eq!((&*source).stream_position().unwrap(),137);
        assert_eq!(leggi_originale(&alias,&AtomicBool::new(true)).unwrap_err(),"Annullato");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn collisioni_budget_e_annullamento_non_lasciano_parziali() {
        let root = nuova_cartella(&std::env::temp_dir()).unwrap();
        let a = nuova_cartella(&root).unwrap();
        let b = nuova_cartella(&root).unwrap();
        assert_ne!(a,b);
        let dest = a.join("uguale.pdf");
        assert!(copia(&b"123456"[..], &dest, 3, &AtomicBool::new(false), |_| {}).is_err());
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(&a).unwrap().count(),0);
        assert!(copia(&b"123"[..], &dest, 3, &AtomicBool::new(true), |_| {}).is_err());
        copia(&b"123"[..], &dest, 3, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(),b"123");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn nomi_non_possono_uscire_dalla_cartella() {
        for n in ["../segreto", "..", "/a/b", "a\\b", "\0nome\n"] {
            let safe=nome_sicuro(n);
            assert_eq!(Path::new(&safe).components().count(),1);
            assert!(!safe.chars().any(char::is_control));
        }
    }
}
