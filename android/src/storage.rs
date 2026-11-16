//! Primitive di I/O limitato, testabili anche senza Android.
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

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
