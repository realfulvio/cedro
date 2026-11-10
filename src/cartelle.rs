//! «Aggiungi cartella»: i file che stanno dentro una cartella, anche nelle sottocartelle, in ordine.

use std::fs;
use std::path::{Path, PathBuf};

/// Quante sottocartelle in profondità si scende (oltre non è una cartella di lavoro: è un disco intero).
const PROFONDITA: usize = 12;

/// I file di `dir` e delle sue sottocartelle, ognuna nel suo ordine di nome, con un massimo di `max`.
/// Le cartelle nascoste (`.git`, `.cache`…) e i collegamenti a cartelle si saltano: non portano a lavoro
/// vero e i collegamenti possono girare in tondo.
pub fn file_in_cartella(dir: &Path, max: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scendi(dir, 0, max, &mut out);
    out
}

fn scendi(dir: &Path, livello: usize, max: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut voci: Vec<_> = rd.flatten().collect();
    voci.sort_by_key(|e| e.file_name());
    for e in voci {
        if out.len() >= max {
            return;
        }
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            let nascosta = e.file_name().to_string_lossy().starts_with('.');
            if !nascosta && livello < PROFONDITA {
                scendi(&p, livello + 1, max, out);
            }
        } else if ft.is_file() || (ft.is_symlink() && p.is_file()) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scende_nelle_sottocartelle_in_ordine_e_salta_le_nascoste() {
        let base = std::env::temp_dir().join(format!("cedro-cartelle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        for d in ["a/b", ".git", "c"] {
            fs::create_dir_all(base.join(d)).unwrap();
        }
        for f in ["z.jpg", "a/1.jpg", "a/b/2.jpg", ".git/x.jpg", "c/3.jpg"] {
            fs::write(base.join(f), b"x").unwrap();
        }
        let v: Vec<String> = file_in_cartella(&base, 100).iter().map(|p| p.strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(v, ["a/1.jpg", "a/b/2.jpg", "c/3.jpg", "z.jpg"]);
        assert_eq!(file_in_cartella(&base, 2).len(), 2);
        let _ = fs::remove_dir_all(&base);
    }
}
